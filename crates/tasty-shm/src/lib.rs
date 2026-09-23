//! Cross-platform shared memory + handle passing primitives.
//!
//! 이 crate는 tasty plugin 시스템의 *인프라 계층*이다. 상위 SDK가 이 위에 안전한
//! `SharedBuffer` wrapper와 IPC transport 통합을 얹는다. 여기서는 OS-native API
//! (memfd / shm_open / CreateFileMapping)만 추상화한다.
//!
//! # 모델
//!
//! 1. **Producer**: `create(size)`로 새 공유 영역을 만든다 → `(SharedMemory,
//!    SendableHandle)` 쌍 반환. producer는 즉시 SharedMemory를 통해 영역에 읽기/쓰기
//!    가능하다.
//! 2. **전달**: producer가 `prepare_send(handle, peer)`로 핸들을 transport-ready
//!    페이로드로 변환한다. 호출자가 자기 IPC 채널 (Unix socket ancillary data /
//!    Named Pipe)에 실어 보낸다. 이 crate는 transport에 *touch하지 않는다*.
//! 3. **Consumer**: 받은 페이로드를 `receive(payload)`에 넘기면 같은 OS 영역에
//!    매핑된 `SharedMemory`가 반환된다. 이후 producer와 동일 메모리를 본다.
//!
//! # 안전성
//!
//! 두 프로세스가 같은 영역을 동시에 변경하면 data race(UB). 동기화는 상위 계층
//! 책임이다(generation counter, dirty rect 등 — `tasty-plugin-sdk` 의 shared buffer 와
//! `host.shared_buffer.dirty`).
//!
//! mmap된 메모리는 신뢰할 수 없는 외부 프로세스가 임의로 쓸 수 있으므로 `as_slice`
//! / `as_mut_slice`는 `unsafe`다. 호출자는 (1) 동기화가 보장된 시점에 읽고, (2)
//! 내용을 *코드로 해석하지 않는다*(픽셀/오디오/raw 바이트 외 용도 금지)는 두 조건을
//! 지켜야 한다.
#![deny(missing_docs)]

mod error;
mod platform;

pub mod footer;

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU64, Ordering};

pub use error::ShmError;

/// 송신측 PID 식별자. Windows의 `DuplicateHandle`이 peer 프로세스 핸들 테이블에
/// HANDLE을 복제할 때 필요. Unix에서는 무시된다 (SCM_RIGHTS는 PID 불요).
#[derive(Debug, Clone, Copy)]
pub enum PeerPid {
    /// 자기 자신 프로세스(테스트용).
    Same,
    /// 다른 프로세스의 OS PID.
    Other(u32),
}

/// OS shared memory section을 현재 프로세스에 매핑한 상태.
///
/// Drop 시 매핑이 해제되고 underlying 핸들이 닫힌다.
pub struct SharedMemory {
    ptr: *mut u8,
    len: usize,
    /// platform-specific cleanup state (e.g. owned fd / HANDLE).
    /// `_` prefix: Drop 시에만 사용되며 직접 접근하지 않는다.
    _handle: platform::PlatformMapping,
}

// SAFETY: SharedMemory는 OS가 매핑한 메모리 영역의 raw pointer를 들고 있다. pointer
// 자체는 thread-safe하고, slice 접근은 unsafe 메서드 뒤에 있어 호출자가 동기화 책임을
// 진다. 따라서 타입 자체는 Send/Sync 가능하다.
unsafe impl Send for SharedMemory {}
// SAFETY: 위와 동일 이유. 데이터 race 가능성은 unsafe 슬라이스 접근 시점에 호출자가
// 막아야 하며, 타입의 Sync는 핸들 메타데이터만 공유하므로 안전.
unsafe impl Sync for SharedMemory {}

impl SharedMemory {
    /// 매핑된 영역의 바이트 길이.
    pub fn len(&self) -> usize {
        self.len
    }

    /// 길이가 0인지 (실제로는 0-byte 영역을 만들 수 없으므로 항상 false).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 매핑된 영역을 raw byte slice로 본다.
    ///
    /// # Safety
    ///
    /// 호출자는 다음을 보장해야 한다:
    /// - 다른 프로세스가 같은 영역에 동시에 쓰는 동안 read를 수행하지 않는다(또는
    ///   수행 시 잘못된 값을 읽어도 무방함을 안다).
    /// - 반환된 slice의 lifetime이 다른 매핑/Drop과 겹치지 않는다.
    pub unsafe fn as_slice(&self) -> &[u8] {
        // SAFETY: ptr/len은 create/receive 시점에 mmap 결과로 받은 유효한 영역.
        // `&self` lifetime 동안 매핑이 살아있음이 보장된다(Drop 순서). 호출자의
        // 동기화 책임은 docstring에 명시.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// 매핑된 영역을 raw mutable byte slice로 본다.
    ///
    /// # Safety
    ///
    /// `as_slice`의 안전 조건에 더해, 다른 프로세스가 동일 영역을 동시에 read/write
    /// 하지 않거나, 동시 접근 결과의 비결정성을 호출자가 수용해야 한다.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn as_mut_slice(&self) -> &mut [u8] {
        // SAFETY: shared memory의 본질상 `&self`로도 다른 프로세스에서 mutate가 일어난다.
        // 동일 프로세스 내 aliasing 위반은 호출자가 막아야 함(docstring 조건).
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

/// 다른 프로세스로 보낼 수 있는 공유 메모리 핸들.
///
/// `prepare_send`를 호출하면 transport-ready 페이로드로 변환되고 본 핸들은 소비된다.
pub struct SendableHandle {
    inner: platform::PlatformSendable,
    size: usize,
}

impl SendableHandle {
    /// 영역 크기 (peer가 받을 mmap에 사용해야 하는 값).
    pub fn size(&self) -> usize {
        self.size
    }
}

/// `prepare_send` 결과. OS-native transport에 실어 보낼 수 있는 형태.
///
/// 이 crate는 transport를 다루지 않으므로, 호출자가 페이로드를 자기 IPC 채널에
/// 적절히 직렬화/전송해야 한다.
///
/// - Unix: `raw_fd()`로 fd를 얻어 sendmsg의 SCM_RIGHTS cmsg에 끼운다.
/// - Windows: `serialized_handle()`로 u64를 얻어 Named Pipe write로 보낸다.
pub struct TransportPayload {
    inner: platform::PlatformPayload,
    size: usize,
}

impl TransportPayload {
    /// 영역 크기.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Unix: 송신할 raw file descriptor. sendmsg cmsg에 끼워 보낸다.
    /// payload는 *소유권을 유지*하므로 fd를 sendmsg에 끼우는 동안 살아있어야 한다.
    #[cfg(unix)]
    pub fn raw_fd(&self) -> std::os::fd::RawFd {
        self.inner.raw_fd()
    }

    /// Windows: 송신할 직렬화된 HANDLE 값(u64). Named Pipe write로 그대로 보낸다.
    #[cfg(windows)]
    pub fn serialized_handle(&self) -> u64 {
        self.inner.serialized_handle()
    }
}

/// 받은 transport 페이로드를 재구성하기 위한 양식.
///
/// 호출자가 자기 transport에서 fd 또는 u64를 꺼내 이 enum으로 wrapping해 넘긴다.
#[cfg(unix)]
pub enum ReceivedPayload {
    /// SCM_RIGHTS cmsg에서 받은 fd + 영역 크기.
    Fd {
        /// 받은 raw file descriptor. `receive`가 호출되면 소유권이 이전된다.
        fd: std::os::fd::RawFd,
        /// 영역 크기.
        size: usize,
    },
}

/// 받은 transport 페이로드를 재구성하기 위한 양식 (Windows).
#[cfg(windows)]
pub enum ReceivedPayload {
    /// peer가 DuplicateHandle로 자신의 핸들 테이블에 복제해준 HANDLE의 u64 표현.
    Handle {
        /// 받은 HANDLE u64. `receive`가 호출되면 소유권이 이전된다.
        handle: u64,
        /// 영역 크기.
        size: usize,
    },
}

/// 단일 프로세스 내 단조 카운터 (macOS의 shm_open 이름 충돌 방지용 — macOS 전용).
#[cfg(target_os = "macos")]
static MONO_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
pub(crate) fn next_unique_id() -> u64 {
    MONO_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// 새 공유 메모리 영역을 만든다.
///
/// 반환된 `SharedMemory`는 현재 프로세스에 즉시 매핑되어 있고, `SendableHandle`은
/// 다른 프로세스에 핸들을 넘기기 위한 자료. `size`는 0보다 커야 한다.
///
/// 한 OS 영역에 대해 송신자(=호출자)와 수신자가 각각 별도 `SharedMemory` 객체를 갖게
/// 된다. Drop은 독립적으로 일어나며, 마지막 매핑이 해제될 때 OS 영역이 회수된다.
pub fn create(size: usize) -> Result<(SharedMemory, SendableHandle), ShmError> {
    if size == 0 {
        return Err(ShmError::ZeroSize);
    }
    platform::create(size)
}

/// `SendableHandle`을 transport-ready 페이로드로 변환한다.
///
/// 호출 후 본 핸들은 소비되어 재사용 불가. peer는 Windows에서만 의미를 가진다
/// (`DuplicateHandle` 대상 PID).
pub fn prepare_send(handle: SendableHandle, peer: PeerPid) -> Result<TransportPayload, ShmError> {
    platform::prepare_send(handle.inner, handle.size, peer).map(|inner| TransportPayload {
        inner,
        size: handle.size,
    })
}

/// 받은 transport 페이로드로부터 현재 프로세스에 영역을 매핑한다.
///
/// # Safety
///
/// `payload`가 담은 fd(unix) 또는 handle(windows)은 대응하는 `prepare_send`가 만든
/// 페이로드로부터 **SCM_RIGHTS recvmsg(unix) 또는 `DuplicateHandle`(windows)로 이
/// 프로세스에 방금 전달되어, 아직 어디에도 소유되지 않은 값**이어야 한다. 임의의
/// 정수, 이미 다른 목적으로 열려 있는 fd/handle, 또는 이미 close/receive된 값을
/// 넘기면 소유권 이중화(double-close, use-after-close, fd/handle aliasing) UB가
/// 발생한다. 호출자는 이 값의 출처를 직접 추적할 수 있는 코드에서만 호출해야 한다.
///
/// fd/handle 형태(열려 있는지, regular file/매핑 객체인지)는 내부적으로 검증해
/// 명백히 잘못된 값은 `Err`로 걸러내지만, "다른 목적으로 쓰이는 유효한 값"까지는
/// 구분하지 못한다 — 이 계약은 형태 검증으로 대체되지 않는다.
pub unsafe fn receive(payload: ReceivedPayload) -> Result<SharedMemory, ShmError> {
    // SAFETY: 호출자가 위 계약을 지켰다는 전제 하에 platform::receive에 위임.
    unsafe { platform::receive(payload) }
}

#[cfg(all(unix, test))]
mod tests {
    use super::*;

    #[test]
    fn receive_rejects_invalid_fd() {
        // 이미 닫혔거나 존재한 적 없는 fd 번호 — fcntl(F_GETFD)에서 EBADF로 걸려야 한다.
        // SAFETY: 이 테스트 자체가 "계약 위반(무효 fd)을 넘겼을 때 UB 대신 Err가
        // 나오는가"를 검증하는 목적 — fcntl/fstat 형태 검증이 mmap 전에 걸러내므로
        // 실제로는 UB가 발생하지 않는다.
        let payload = ReceivedPayload::Fd {
            fd: 99999,
            size: 4096,
        };
        // SAFETY: 위와 동일 — 계약 위반(무효 fd)을 형태 검증이 걸러낸다.
        let result = unsafe { receive(payload) };
        assert!(result.is_err());
    }

    #[test]
    fn receive_rejects_non_regular_file_fd() {
        // stdin(fd 0)은 열려는 있지만 memfd/shm backing과 다른 타입 — fstat의
        // S_IFREG 검증에 걸려야 한다.
        // SAFETY: 위와 동일 — 계약 위반(타입 불일치 fd)을 형태 검증이 걸러낸다.
        let payload = ReceivedPayload::Fd { fd: 0, size: 4096 };
        // SAFETY: 위와 동일 — 계약 위반(타입 불일치 fd)을 형태 검증이 걸러낸다.
        let result = unsafe { receive(payload) };
        assert!(result.is_err());
    }
}
