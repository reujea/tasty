//! SDK 측 보조 핸들 채널 클라이언트.
//!
//! 메인 채널([`crate::connection::Connection`])이 핸드셰이크를 끝낸 직후 본 모듈의
//! [`HandleClient::connect`]가 호출되어 보조 채널을 연다. plugin spawn 환경변수
//! `TASTY_PLUGIN_HANDLE_ENDPOINT`가 비어 있으면 보조 채널을 사용하지 않는다 (host가
//! 활성화하지 않은 경우).
//!
//! HandleAttach로 공유 메모리 핸들을 받고 Dirty로 변경 영역을 알린다.

#[cfg(unix)]
use std::collections::VecDeque;
use std::io::{self};
#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use tasty_plugin_protocol::HandleChannelMessage;
use tasty_plugin_protocol::{AuthAckEnvelope, AuthMessage};

use crate::env::PluginEnv;
use crate::error::{PluginError, Result};

#[cfg(unix)]
const AUTH_ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// 보조 채널 클라이언트. 메인 채널 인증 직후 plugin runtime이 한 번 만든다.
///
/// Unix/Windows 양쪽 구현 완료. [`HandleClient::connect`]가 Unix는 `AF_UNIX` socket으로,
/// Windows는 Named Pipe(`CreateFileW` overlapped I/O)로 연결한 뒤 auth 핸드셰이크를 한다.
/// 연결 실패 시 호출자는 이 에러를 fatal로 취급하지 않고 warn 후 plugin 본 루프를 그대로
/// 진행한다 (보조 채널은 *추가* 동작).
#[derive(Debug)]
pub struct HandleClient {
    #[cfg(unix)]
    inner: std::os::unix::net::UnixStream,
    #[cfg(windows)]
    inner: self::windows::PipeClientStream,
}

impl HandleClient {
    /// `env.handle_endpoint`이 있어야 호출된다. 없으면 호출자가 미리 분기.
    pub fn connect(env: &PluginEnv) -> Result<Self> {
        let endpoint = env
            .handle_endpoint
            .as_deref()
            .ok_or(PluginError::EnvMissing("TASTY_PLUGIN_HANDLE_ENDPOINT"))?;
        Self::connect_to(endpoint, env)
    }

    #[cfg(unix)]
    fn connect_to(endpoint: &str, env: &PluginEnv) -> Result<Self> {
        use std::os::unix::net::UnixStream;

        let stream = UnixStream::connect(endpoint)?;
        // AuthAck 읽기 동안만 짧은 timeout.
        stream.set_read_timeout(Some(AUTH_ACK_TIMEOUT))?;

        let mut writer = stream.try_clone()?;
        let auth = AuthMessage {
            plugin_id: env.plugin_id.clone(),
            token: env.token.clone(),
        };
        let line = serde_json::to_string(&auth)?;
        writeln!(writer, "{line}")?;
        writer.flush()?;

        let cloned = stream.try_clone()?;
        let mut reader = BufReader::new(cloned);
        let mut ack_line = String::new();
        let read_result = reader.read_line(&mut ack_line);
        // 인증 뒤 읽기 timeout을 해제한다. 이후 수신은 블로킹 방식이다.
        stream.set_read_timeout(None)?;
        match read_result {
            Ok(0) => Err(PluginError::HandshakeTimeout),
            Ok(_) => {
                let trim = ack_line.trim();
                if trim.is_empty() {
                    return Err(PluginError::HandshakeTimeout);
                }
                let env_msg: AuthAckEnvelope = serde_json::from_str(trim)?;
                if env_msg.auth_ack.ok {
                    Ok(Self { inner: stream })
                } else {
                    Err(PluginError::HandshakeRejected {
                        reason: env_msg.auth_ack.reason,
                    })
                }
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                Err(PluginError::HandshakeTimeout)
            }
            Err(e) => Err(PluginError::Io(e)),
        }
    }

    #[cfg(windows)]
    fn connect_to(endpoint: &str, env: &PluginEnv) -> Result<Self> {
        let mut stream = self::windows::PipeClientStream::connect(endpoint)?;

        let auth = AuthMessage {
            plugin_id: env.plugin_id.clone(),
            token: env.token.clone(),
        };
        let line = serde_json::to_string(&auth)?;
        stream.write_line(&line)?;

        let ack_line = stream.read_line()?;
        let trim = ack_line.trim();
        if trim.is_empty() {
            return Err(PluginError::HandshakeTimeout);
        }
        let env_msg: AuthAckEnvelope = serde_json::from_str(trim)?;
        if env_msg.auth_ack.ok {
            Ok(Self { inner: stream })
        } else {
            Err(PluginError::HandshakeRejected {
                reason: env_msg.auth_ack.reason,
            })
        }
    }

    /// 한 줄을 NDJSON으로 송신.
    #[cfg(unix)]
    pub fn write_line(&mut self, line: &str) -> Result<()> {
        let mut buf = line.as_bytes().to_vec();
        buf.push(b'\n');
        self.inner.write_all(&buf)?;
        self.inner.flush()?;
        Ok(())
    }

    /// 한 줄을 NDJSON으로 송신. Windows Named Pipe write.
    #[cfg(windows)]
    pub fn write_line(&mut self, line: &str) -> Result<()> {
        self.inner.write_line(line)
    }

    /// blocking으로 한 줄 읽기. read timeout이 설정되어 있다면 그 이내에 못 받으면 io 에러.
    #[cfg(unix)]
    pub fn read_line(&mut self) -> Result<String> {
        let cloned = self.inner.try_clone()?;
        let mut reader = BufReader::new(cloned);
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Err(PluginError::HostClosed);
        }
        Ok(line)
    }

    /// blocking으로 한 줄 읽기. Windows Named Pipe read.
    #[cfg(windows)]
    pub fn read_line(&mut self) -> Result<String> {
        self.inner.read_line()
    }

    /// read timeout 조정. ping/pong 등 짧은 동기 대기 시 호출.
    #[cfg(unix)]
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        self.inner.set_read_timeout(timeout)?;
        Ok(())
    }

    /// Windows 파이프에는 이 메서드로 읽기 timeout을 설정하지 않는다.
    #[cfg(windows)]
    #[allow(clippy::unused_self)]
    pub fn set_read_timeout(&self, _timeout: Option<Duration>) -> Result<()> {
        Ok(())
    }

    /// 메시지 단위 송신. fd가 필요한 메시지는 SDK가 보내지 않으므로 일반 라인 쓰기.
    #[cfg(unix)]
    pub fn send_message(&mut self, msg: &HandleChannelMessage) -> Result<()> {
        let line = serde_json::to_string(msg)?;
        self.write_line(&line)
    }

    #[cfg(windows)]
    pub fn send_message(&mut self, msg: &HandleChannelMessage) -> Result<()> {
        let line = serde_json::to_string(msg)?;
        self.write_line(&line)
    }

    /// 수신 측 reader를 분리해서 반환. write 핸들은 self에 남는다.
    #[cfg(unix)]
    pub fn reader(&self) -> Result<HandleClientReader> {
        let cloned = self.inner.try_clone()?;
        Ok(HandleClientReader::from_unix(cloned))
    }

    /// Windows: duplex 파이프 핸들을 복제해 reader 스레드용 stream 을 분리한다.
    #[cfg(windows)]
    pub fn reader(&self) -> Result<HandleClientReader> {
        let cloned = self.inner.try_clone()?;
        Ok(HandleClientReader::from_windows(cloned))
    }

    /// Test 전용: 외부에서 만든 `UnixStream`을 그대로 감싸 `HandleClient`로 사용.
    /// 핸드셰이크 없이 사용하므로 단위 테스트에서만 호출한다.
    #[cfg(all(unix, test))]
    pub(crate) fn from_unix_stream(stream: std::os::unix::net::UnixStream) -> Self {
        Self { inner: stream }
    }
}

/// 보조 채널 위의 NDJSON 메시지를 한 줄씩 받는다. host가 보내는
/// [`HandleChannelMessage::HandleAttach`]는 ancillary data(SCM_RIGHTS)로 fd가 함께
/// 도착하므로, 반환값에 `Option<RawFd>`가 포함된다.
pub struct HandleClientReader {
    #[cfg(unix)]
    inner: std::os::unix::net::UnixStream,
    #[cfg(unix)]
    carry: Vec<u8>,
    #[cfg(unix)]
    fd_queue: VecDeque<std::os::fd::RawFd>,
    #[cfg(windows)]
    inner: self::windows::PipeClientStream,
    #[cfg(windows)]
    carry: Vec<u8>,
}

impl HandleClientReader {
    #[cfg(unix)]
    fn from_unix(stream: std::os::unix::net::UnixStream) -> Self {
        Self {
            inner: stream,
            carry: Vec::with_capacity(4096),
            fd_queue: VecDeque::new(),
        }
    }

    #[cfg(windows)]
    fn from_windows(stream: self::windows::PipeClientStream) -> Self {
        Self {
            inner: stream,
            carry: Vec::with_capacity(4096),
        }
    }

    /// 다음 한 건을 blocking으로 받는다. `HandleAttach`의 ancillary fd는 같이 반환.
    /// 연결이 닫히면 `HostClosed`.
    #[cfg(unix)]
    pub fn recv_message(&mut self) -> Result<(HandleChannelMessage, Option<std::os::fd::RawFd>)> {
        loop {
            if let Some(nl) = self.carry.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = self.carry.drain(..=nl).collect();
                let line_str = std::str::from_utf8(&line_bytes[..nl])
                    .map_err(|e| PluginError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?
                    .trim();
                if line_str.is_empty() {
                    continue;
                }
                let msg: HandleChannelMessage = serde_json::from_str(line_str)?;
                let aux_fd = match msg {
                    HandleChannelMessage::HandleAttach { .. } => self.fd_queue.pop_front(),
                    _ => None,
                };
                return Ok((msg, aux_fd));
            }

            let mut buf = [0u8; 4096];
            let (n, fds) = unix_wire::recv_with_fd(&self.inner, &mut buf)?;
            if n == 0 {
                return Err(PluginError::HostClosed);
            }
            self.carry.extend_from_slice(&buf[..n]);
            for fd in fds {
                self.fd_queue.push_back(fd);
            }
        }
    }

    /// Windows: 파이프에서 NDJSON 라인을 파싱한다. `HandleAttach` 의 in-band `handle`
    /// 필드(HANDLE u64)를 함께 반환한다 — plugin 은 이 값을 `tasty_shm::receive` 로 매핑.
    #[cfg(windows)]
    pub fn recv_message(&mut self) -> Result<(HandleChannelMessage, Option<u64>)> {
        loop {
            if let Some(nl) = self.carry.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = self.carry.drain(..=nl).collect();
                let line_str = std::str::from_utf8(&line_bytes[..nl])
                    .map_err(|e| PluginError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?
                    .trim();
                if line_str.is_empty() {
                    continue;
                }
                let msg: HandleChannelMessage = serde_json::from_str(line_str)?;
                let handle = match msg {
                    HandleChannelMessage::HandleAttach { handle, .. } => handle,
                    _ => None,
                };
                return Ok((msg, handle));
            }

            let n = self.inner.read_into(&mut self.carry)?;
            if n == 0 {
                return Err(PluginError::HostClosed);
            }
        }
    }
}

#[cfg(unix)]
mod unix_wire {
    //! SDK 측 `recvmsg` + `SCM_RIGHTS` 헬퍼. host 측 동일 헬퍼와 의도적으로 같은 동작.
    use std::io;
    use std::mem;
    use std::os::fd::{AsRawFd, RawFd};
    use std::os::unix::net::UnixStream;

    pub(super) fn recv_with_fd(
        stream: &UnixStream,
        buf: &mut [u8],
    ) -> io::Result<(usize, Vec<RawFd>)> {
        let mut iov = libc::iovec {
            iov_base: buf.as_mut_ptr() as *mut _,
            iov_len: buf.len(),
        };
        // SAFETY: zero-initialized msghdr는 recvmsg에 안전한 초기 상태.
        let mut msg: libc::msghdr = unsafe { mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;

        // 8B 정렬 보장된 cmsg 버퍼.
        #[repr(C)]
        union RecvCmsgBuf {
            bytes: [u8; 256],
            _align: [u64; 32],
        }
        let mut cmsg_buf = RecvCmsgBuf { bytes: [0u8; 256] };
        // SAFETY: bytes 필드 접근. union 두 필드 같은 메모리 공유.
        msg.msg_control = unsafe { cmsg_buf.bytes.as_mut_ptr() } as *mut _;
        msg.msg_controllen = 256 as _;

        let fd = stream.as_raw_fd();
        // SAFETY: fd는 open된 socket, msg는 valid한 초기화.
        let n = unsafe { libc::recvmsg(fd, &mut msg, 0) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }

        let mut fds = Vec::new();
        // SAFETY: msg가 valid한 cmsg buffer를 가짐.
        let mut cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
        while !cmsg.is_null() {
            // SAFETY: cmsg non-null, msg가 가진 cmsg 영역 내부 포인터.
            let (level, ty, len) = unsafe {
                let h = &*cmsg;
                (h.cmsg_level, h.cmsg_type, h.cmsg_len)
            };
            if level == libc::SOL_SOCKET && ty == libc::SCM_RIGHTS {
                // SAFETY: header 크기 계산.
                let header_len = unsafe { libc::CMSG_LEN(0) } as usize;
                // 이유: cmsghdr.cmsg_len의 타입은 플랫폼마다 달라 usize로 변환한다.
                #[allow(clippy::unnecessary_cast)]
                let data_len = (len as usize).saturating_sub(header_len);
                let n_fds = data_len / mem::size_of::<libc::c_int>();
                // SAFETY: CMSG_DATA는 cmsg 안의 data 시작 포인터.
                let data_ptr = unsafe { libc::CMSG_DATA(cmsg) } as *const libc::c_int;
                for i in 0..n_fds {
                    // SAFETY: data_ptr 부터 n_fds * sizeof(c_int) 범위. add 와
                    // read_unaligned 가 cmsg 단일 fd 읽기로 묶여있어 분할 시 가독성 저하.
                    #[allow(clippy::multiple_unsafe_ops_per_block)]
                    let f = unsafe { std::ptr::read_unaligned(data_ptr.add(i)) };
                    fds.push(f);
                }
            }
            // SAFETY: 동일 msg에 대한 다음 cmsg.
            cmsg = unsafe { libc::CMSG_NXTHDR(&msg, cmsg) };
        }

        Ok((n as usize, fds))
    }
}

/// Windows Named Pipe 클라이언트. 같은 파이프 객체의 읽기가 쓰기를 막지 않도록
/// overlapped I/O와 연산별 완료 이벤트를 사용한다.
#[cfg(windows)]
mod windows {
    use std::io;
    use std::mem;
    use std::ptr;

    use windows_sys::Win32::Foundation::{
        CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING,
        ERROR_PIPE_BUSY, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, ReadFile, WriteFile,
    };
    use windows_sys::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
    use windows_sys::Win32::System::Threading::{CreateEventW, GetCurrentProcess, ResetEvent};

    use crate::error::{PluginError, Result};

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    /// `WaitNamedPipeW` 최대 대기(ms). host 가 accept 인스턴스를 상시 유지하므로 busy 는
    /// 드물지만, spawn 직후 경합을 위해 여유를 둔다.
    const PIPE_BUSY_WAIT_MS: u32 = 5000;

    fn to_wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// close-on-drop HANDLE RAII.
    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                // SAFETY: null/INVALID 이 아닐 때만, Drop 은 한 번만 실행.
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    impl std::fmt::Debug for OwnedHandle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "OwnedHandle({:p})", self.0)
        }
    }

    // SAFETY: HANDLE 은 OS 관리 정수. 스레드 이동/공유 안전, 파이프 R/W thread-safe.
    unsafe impl Send for OwnedHandle {}
    // SAFETY: HANDLE 은 OS 관리 정수. 공유 참조로 스레드 간 공유해도 안전하며
    // 파이프 R/W 는 커널이 thread-safe 하게 처리한다(Send 와 동일 근거).
    unsafe impl Sync for OwnedHandle {}

    fn make_event() -> Result<OwnedHandle> {
        // SAFETY: 유효한 인자를 전달한다. 자동 리셋·초기 비신호 상태의 무명 이벤트를 만든다.
        let h = unsafe { CreateEventW(ptr::null(), 0, 0, ptr::null()) };
        if h.is_null() {
            return Err(PluginError::Io(io::Error::last_os_error()));
        }
        Ok(OwnedHandle(h))
    }

    /// 클라이언트 파이프 stream. [`HandleClient`](super::HandleClient) 의 Windows inner.
    /// 자기 완료 event 를 소유해 read/write 가 서로 간섭하지 않는다.
    #[derive(Debug)]
    pub(super) struct PipeClientStream {
        handle: OwnedHandle,
        event: OwnedHandle,
    }

    impl PipeClientStream {
        fn from_handle(handle: OwnedHandle) -> Result<Self> {
            let event = make_event()?;
            Ok(Self { handle, event })
        }

        /// endpoint(`\\.\pipe\...`)에 연결한다(overlapped). 인스턴스가 모두 사용 중이면
        /// `WaitNamedPipeW` 로 잠시 대기 후 재시도한다.
        pub(super) fn connect(name: &str) -> Result<Self> {
            let wide = to_wide(name);
            loop {
                // SAFETY: Win32 CreateFileW. wide 는 NUL 종단 UTF-16.
                let raw = unsafe {
                    CreateFileW(
                        wide.as_ptr(),
                        GENERIC_READ | GENERIC_WRITE,
                        0,
                        ptr::null(),
                        OPEN_EXISTING,
                        FILE_FLAG_OVERLAPPED,
                        ptr::null_mut(),
                    )
                };
                if raw != INVALID_HANDLE_VALUE && !raw.is_null() {
                    return Self::from_handle(OwnedHandle(raw));
                }
                // SAFETY: 부수효과 없는 last-error 조회.
                let err = unsafe { GetLastError() };
                if err == ERROR_PIPE_BUSY {
                    // SAFETY: Win32 WaitNamedPipeW. 실패해도 다음 루프에서 재시도.
                    let _ = unsafe { WaitNamedPipeW(wide.as_ptr(), PIPE_BUSY_WAIT_MS) };
                    continue;
                }
                return Err(PluginError::Io(io::Error::from_raw_os_error(err as i32)));
            }
        }

        /// overlapped op 하나를 blocking 으로 수행한다. BROKEN_PIPE 는 EOF(0) 로 정규화.
        fn blocking_io(
            &self,
            start: impl FnOnce(HANDLE, *mut OVERLAPPED, *mut u32) -> i32,
        ) -> Result<usize> {
            // SAFETY: ResetEvent 는 유효 event 핸들에 안전.
            unsafe {
                ResetEvent(self.event.0);
            }
            // SAFETY: OVERLAPPED 은 POD 이며 all-zero 가 유효한 초기 상태다.
            let mut ov: OVERLAPPED = unsafe { mem::zeroed() };
            ov.hEvent = self.event.0;
            let mut transferred: u32 = 0;
            let ok = start(self.handle.0, &mut ov, &mut transferred);
            if ok == 0 {
                // SAFETY: 부수효과 없는 last-error 조회.
                let err = unsafe { GetLastError() };
                if err == ERROR_BROKEN_PIPE {
                    return Ok(0);
                }
                if err != ERROR_IO_PENDING {
                    return Err(PluginError::Io(io::Error::from_raw_os_error(err as i32)));
                }
                // SAFETY: 유효 핸들·OVERLAPPED, 완료 대기(bWait=TRUE).
                let done = unsafe { GetOverlappedResult(self.handle.0, &ov, &mut transferred, 1) };
                if done == 0 {
                    // SAFETY: 부수효과 없는 last-error 조회.
                    let e2 = unsafe { GetLastError() };
                    if e2 == ERROR_BROKEN_PIPE {
                        return Ok(0);
                    }
                    return Err(PluginError::Io(io::Error::from_raw_os_error(e2 as i32)));
                }
            }
            Ok(transferred as usize)
        }

        fn read_raw(&self, buf: &mut [u8]) -> Result<usize> {
            self.blocking_io(|h, ov, transferred| {
                // SAFETY: 유효 핸들, buf 는 len 만큼 유효, overlapped read.
                unsafe { ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, transferred, ov) }
            })
        }

        /// 최대 4 KiB 를 읽어 `carry` 뒤에 붙인다. 반환 0 이면 EOF.
        pub(super) fn read_into(&mut self, carry: &mut Vec<u8>) -> Result<usize> {
            let mut buf = [0u8; 4096];
            let n = self.read_raw(&mut buf)?;
            carry.extend_from_slice(&buf[..n]);
            Ok(n)
        }

        /// 개행까지 한 줄 읽기. auth ack 뒤 바이트를 over-read 하지 않도록 1바이트씩 읽는다
        /// (같은 파이프를 공유하는 reader clone 이 이어질 바이트를 잃지 않게).
        pub(super) fn read_line(&mut self) -> Result<String> {
            let mut out = Vec::with_capacity(256);
            let mut byte = [0u8; 1];
            loop {
                let n = self.read_raw(&mut byte)?;
                if n == 0 {
                    break;
                }
                if byte[0] == b'\n' {
                    break;
                }
                out.push(byte[0]);
            }
            String::from_utf8(out)
                .map_err(|e| PluginError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))
        }

        pub(super) fn write_line(&mut self, line: &str) -> Result<()> {
            let mut buf = line.as_bytes().to_vec();
            buf.push(b'\n');
            let mut off = 0usize;
            while off < buf.len() {
                let chunk = &buf[off..];
                let n = self.blocking_io(|h, ov, transferred| {
                    // SAFETY: 유효 핸들, chunk 는 유효, overlapped write.
                    unsafe { WriteFile(h, chunk.as_ptr(), chunk.len() as u32, transferred, ov) }
                })?;
                if n == 0 {
                    return Err(PluginError::Io(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "handle channel pipe write returned 0",
                    )));
                }
                off += n;
            }
            Ok(())
        }

        /// 현재 프로세스 내 파이프 핸들 복제(새 event). reader/writer 스레드 분리용.
        // 이유: 현재 프로세스 핸들 조회와 DuplicateHandle 호출을 한 블록에 둔다.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        pub(super) fn try_clone(&self) -> Result<Self> {
            let mut dup: HANDLE = ptr::null_mut();
            // SAFETY: 유효 핸들 → 같은 프로세스 복제. 실패 시 rc==0.
            let rc = unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    self.handle.0,
                    GetCurrentProcess(),
                    &mut dup,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            if rc == 0 {
                return Err(PluginError::Io(io::Error::last_os_error()));
            }
            Self::from_handle(OwnedHandle(dup))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;

    fn env_for(endpoint: &str) -> PluginEnv {
        PluginEnv {
            plugin_id: "com.test.plugin".into(),
            host_port: 0,
            token: "test-token".into(),
            host_api_version: "1".into(),
            plugin_dir: None,
            data_dir: None,
            config_path: None,
            log_path: None,
            locale: "en".into(),
            locale_font: None,
            locale_font_face: 0,
            handle_endpoint: Some(endpoint.to_string()),
        }
    }

    fn unique_socket_path(suffix: &str) -> std::path::PathBuf {
        // macOS의 SUN_LEN(~104B) 제한 — 짧게 유지. /tmp는 모든 unix에 존재.
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::path::PathBuf::from(format!(
            "/tmp/tst-h-{}-{:x}-{}.sock",
            pid,
            (nanos as u64) & 0xFFFF_FFFF,
            suffix
        ))
    }

    #[test]
    fn connect_succeeds_when_host_acks_ok() {
        let path = unique_socket_path("ok");
        // 이전 테스트 잔여 socket 제거 (NotFound는 정상).
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            panic!("pre-cleanup {} failed: {e}", path.display());
        }
        let listener = UnixListener::bind(&path).unwrap();
        let path_clone = path.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let cloned = stream.try_clone().unwrap();
            let mut reader = BufReader::new(cloned);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(stream, "{{\"auth_ack\":{{\"ok\":true}}}}").expect("fake host: ack");
            stream.flush().expect("fake host: flush");
            thread::sleep(Duration::from_millis(200));
            drop(stream);
            // cleanup — server thread 종료 시 socket file 삭제. NotFound는 정상.
            if let Err(e) = std::fs::remove_file(&path_clone)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::trace!("post-test socket cleanup failed: {e}");
            }
        });
        let env = env_for(path.to_str().unwrap());
        let client = HandleClient::connect(&env).expect("auth ok");
        drop(client);
        server.join().unwrap();
    }

    #[test]
    fn connect_returns_rejected_when_host_acks_false() {
        let path = unique_socket_path("reject");
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            panic!("pre-cleanup {} failed: {e}", path.display());
        }
        let listener = UnixListener::bind(&path).unwrap();
        let path_clone = path.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let cloned = stream.try_clone().unwrap();
            let mut reader = BufReader::new(cloned);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(
                stream,
                "{{\"auth_ack\":{{\"ok\":false,\"reason\":\"nope\"}}}}"
            )
            .expect("fake host: reject");
            stream.flush().expect("fake host: flush");
            thread::sleep(Duration::from_millis(50));
            if let Err(e) = std::fs::remove_file(&path_clone)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::trace!("post-test socket cleanup failed: {e}");
            }
        });
        let env = env_for(path.to_str().unwrap());
        let err = HandleClient::connect(&env).expect_err("should be rejected");
        match err {
            PluginError::HandshakeRejected { reason } => {
                assert_eq!(reason.as_deref(), Some("nope"));
            }
            other => panic!("expected HandshakeRejected, got {other:?}"),
        }
        server.join().unwrap();
    }

    #[test]
    fn connect_returns_env_missing_when_endpoint_absent() {
        let mut env = env_for("/dummy");
        env.handle_endpoint = None;
        let err = HandleClient::connect(&env).expect_err("no endpoint");
        assert!(matches!(err, PluginError::EnvMissing(_)));
    }

    /// host측 unix_wire 헬퍼와 시그니처가 같은 sendmsg 보조. 테스트 외에는 SDK가 fd를
    /// 보낼 일이 없으므로 production에는 두지 않는다.
    fn test_sendmsg_with_fd(
        stream: &std::os::unix::net::UnixStream,
        bytes: &[u8],
        fd: std::os::fd::RawFd,
    ) -> std::io::Result<()> {
        use std::mem;
        use std::os::fd::AsRawFd;
        let iov = libc::iovec {
            iov_base: bytes.as_ptr() as *mut _,
            iov_len: bytes.len(),
        };
        // SAFETY: zero-init msghdr는 sendmsg에 valid한 초기 상태.
        let mut msg: libc::msghdr = unsafe { mem::zeroed() };
        msg.msg_iov = &iov as *const _ as *mut _;
        msg.msg_iovlen = 1;
        // 8B 정렬 보장된 cmsg 버퍼.
        #[repr(C)]
        union SendCmsgBuf {
            bytes: [u8; 64],
            _align: [u64; 8],
        }
        let mut cmsg_buf = SendCmsgBuf { bytes: [0u8; 64] };
        // SAFETY: CMSG_SPACE는 부수효과 없음.
        let cmsg_space = unsafe { libc::CMSG_SPACE(mem::size_of::<libc::c_int>() as u32) } as usize;
        // SAFETY: bytes 필드 접근. union 두 필드 같은 메모리 공유.
        msg.msg_control = unsafe { cmsg_buf.bytes.as_mut_ptr() } as *mut _;
        msg.msg_controllen = cmsg_space as _;
        // SAFETY: cmsg 버퍼에는 헤더와 fd를 담을 공간이 있고 계산한 data 위치에 쓴다.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            let cmsg_ptr = libc::CMSG_FIRSTHDR(&msg);
            (*cmsg_ptr).cmsg_level = libc::SOL_SOCKET;
            (*cmsg_ptr).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg_ptr).cmsg_len = libc::CMSG_LEN(mem::size_of::<libc::c_int>() as u32) as _;
            let data_ptr = libc::CMSG_DATA(cmsg_ptr) as *mut libc::c_int;
            std::ptr::write_unaligned(data_ptr, fd);
        }
        let sfd = stream.as_raw_fd();
        // SAFETY: sfd 유효, msg 유효.
        let n = unsafe { libc::sendmsg(sfd, &msg, 0) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    #[test]
    fn reader_decodes_handle_attach_with_fd() {
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;
        use tasty_plugin_protocol::SharedBufferId;

        // socketpair로 양방향 stream을 만들고, 한쪽을 host처럼 sendmsg, 다른쪽을 SDK reader처럼 처리.
        let (host_side, sdk_side) = UnixStream::pair().expect("socketpair");

        // 보낼 fd로 /dev/null 열기.
        let dev_null = std::fs::File::open("/dev/null").expect("open /dev/null");
        let send_fd = dev_null.as_raw_fd();

        let msg = HandleChannelMessage::HandleAttach {
            request_id: 7,
            id: SharedBufferId(99),
            size: 8192,
            handle: None,
        };
        let mut line = serde_json::to_string(&msg).unwrap();
        line.push('\n');
        test_sendmsg_with_fd(&host_side, line.as_bytes(), send_fd).expect("sendmsg with fd");

        // SDK reader는 보통 HandleClient::reader()로 만들어지지만, 테스트에서는 직접 구성.
        let mut reader = HandleClientReader::from_unix(sdk_side);
        let (got_msg, got_fd) = reader.recv_message().expect("recv");
        assert_eq!(got_msg, msg);
        let fd = got_fd.expect("expected fd on HandleAttach");
        // 받은 fd는 원래 fd와 *다른* 번호여야 한다 (kernel이 dup).
        assert_ne!(fd, send_fd, "expected dup'd fd");
        // 받은 fd로 /dev/null이 실제로 매핑됐는지 확인 — read는 빈 buffer를 반환.
        // SAFETY: fd는 방금 dup된 valid한 file descriptor.
        let n = unsafe {
            let mut buf = [0u8; 1];
            libc::read(fd, buf.as_mut_ptr() as *mut _, 1)
        };
        assert_eq!(n, 0, "/dev/null read returns 0 (EOF)");
        // 받은 fd close — leak 방지.
        // SAFETY: fd는 우리가 위에서 받은 valid한 file descriptor.
        unsafe { libc::close(fd) };
    }

    #[test]
    fn reader_decodes_plain_dirty_without_fd() {
        use std::io::Write;
        use std::os::unix::net::UnixStream;
        use tasty_plugin_protocol::{PixelRect, SharedBufferId};

        let (mut host_side, sdk_side) = UnixStream::pair().expect("socketpair");
        let msg = HandleChannelMessage::Dirty {
            id: SharedBufferId(3),
            rect: Some(PixelRect {
                x: 1,
                y: 2,
                w: 3,
                h: 4,
            }),
        };
        let line = serde_json::to_string(&msg).unwrap() + "\n";
        host_side.write_all(line.as_bytes()).unwrap();

        let mut reader = HandleClientReader::from_unix(sdk_side);
        let (got, fd) = reader.recv_message().expect("recv");
        assert_eq!(got, msg);
        assert!(fd.is_none(), "Dirty 메시지는 fd 없음");
    }
}
