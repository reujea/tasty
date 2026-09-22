//! macOS 권한 — 파일 TCC · 화면 기록 · 손쉬운 사용 pre-warm + Full Disk Access 추정/안내.
//!
//! macOS 는 보호 리소스에 **실제로 접근하는 그 순간**에만 권한 프롬프트를 띄운다.
//! 파일 계열 TCC 서비스에는 "미리 물어보는" API 가 없으므로, 프롬프트 시점을 앞당기는
//! 유일한 방법은 앱이 부팅 직후 그 리소스를 스스로 한 번 건드리는 것이다
//! (디렉터리 1 회 `read_dir`). PTY 자식 프로세스(zsh, 그 안의 AI 에이전트)가 보호
//! 폴더에 접근하면 macOS 가 그 접근의 responsible process 를 부모 GUI 앱으로 귀속
//! 시키므로, 터미널 작업 *도중에* 프롬프트가 떠서 자율 진행이 멈추는 것이 원래
//! 증상이다. 부팅 직후로 몰아두면 그 중단이 사라진다.
//!
//! 이미 허용/거부가 결정된 항목에는 프롬프트가 뜨지 않으므로 매 부팅 반복해도
//! 무해하다 — "첫 실행" 플래그를 따로 두지 않는 이유다. 새 마운트는 실행할 때마다
//! 달라져 1 회로는 못 덮고, 플래그만 남고 TCC 가 초기화된 상태(재설치·`tccutil reset`
//! 이후)에서는 pre-warm 이 영영 안 도는 어긋남이 생긴다.
//!
//! 이 모듈은 **목록 결정**(순수, 전 플랫폼 컴파일·테스트 가능)과 **실제 접근**
//! (`#[cfg(all(target_os = "macos", feature = "gui"))]`) 을 분리한다. cfg 로 잘린
//! 코드는 rustc 가 타입체크 전에 걷어내므로, 로직을 순수부에 몰아둘수록 비-macOS
//! 에서도 검증되는 면적이 넓어진다.
//!
//! 기능 문서: `docs/features/macos-permissions/index.md`.

// 이유: 비-macOS / headless 빌드에서는 결정 로직을 호출하는 실행부가 cfg 로 잘려 나간다.
// 그래도 로직 자체는 컴파일한다 — cfg 로 잘린 코드는 타입체크조차 되지 않으므로,
// 다른 플랫폼에서 검증 가능한 면적을 남겨두는 것이 이 분리의 목적이다.
#![cfg_attr(not(all(target_os = "macos", feature = "gui")), allow(dead_code))]

use std::path::{Path, PathBuf};

/// pre-warm 할 홈 하위 폴더 — `SystemPolicy{Downloads,Documents,Desktop}Folder` 대응.
/// 순서가 곧 프롬프트가 뜨는 순서다.
const HOME_SUBDIRS: [&str; 3] = ["Downloads", "Documents", "Desktop"];

/// 마운트 루트. 이동식(`SystemPolicyRemovableVolumes`)·네트워크
/// (`SystemPolicyNetworkVolumes`) 볼륨이 모두 여기 하위에 붙는다.
const VOLUMES_ROOT: &str = "/Volumes";

/// 목록 결정에 필요한 파일시스템 조회. 실제 IO 없이 결정 로직만 검증할 수 있도록
/// 추상화한다 — TCC 가 없는 CI 에서 `read_dir` 을 돌리면 헤드리스 러너가 프롬프트를
/// 기다리며 멈출 수 있고, 그 환경 의존성을 테스트에 들이지 않기 위함이다.
trait FsProbe {
    /// 디렉터리로 존재하는가. 없는 폴더는 읽어봐야 프롬프트가 안 뜨므로 건너뛴다.
    fn is_dir(&self, path: &Path) -> bool;

    /// depth-1 나열. 실패(권한·부재)는 빈 목록으로 접는다 — 마운트 목록을 못 읽는 것은
    /// pre-warm 을 중단할 사유가 아니다.
    fn list_dir(&self, path: &Path) -> Vec<PathBuf>;
}

/// 실제 파일시스템. pre-warm 실행부와 같은 조건으로만 컴파일한다.
#[cfg(all(target_os = "macos", feature = "gui"))]
struct RealFs;

#[cfg(all(target_os = "macos", feature = "gui"))]
impl FsProbe for RealFs {
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn list_dir(&self, path: &Path) -> Vec<PathBuf> {
        match std::fs::read_dir(path) {
            Ok(entries) => entries.filter_map(Result::ok).map(|e| e.path()).collect(),
            // 마운트 루트를 못 읽어도 홈 폴더 pre-warm 은 그대로 진행한다.
            Err(err) => {
                tracing::debug!(path = %path.display(), %err, "prewarm: 마운트 루트 나열 실패");
                Vec::new()
            }
        }
    }
}

/// pre-warm 대상 경로를 **순서대로** 결정한다.
///
/// 홈 폴더 3 곳이 먼저고, 마운트된 볼륨이 마지막이다. 네트워크 볼륨의 `read_dir` 은
/// 응답 없는 마운트에서 수 초~수십 초 걸리거나 영영 안 끝날 수 있어, 앞에 두면 사용자가
/// 실제로 겪는 홈 폴더 프롬프트가 그만큼 늦어진다. 볼륨은 `/Volumes` 를 depth-1 로만
/// 나열해 항목당 한 번씩만 건드린다.
///
/// 존재하지 않는 경로는 빠진다. `home` 이 `None` 이면 홈 항목 전체가 빠진다.
fn prewarm_targets(home: Option<&Path>, fs: &dyn FsProbe) -> Vec<PathBuf> {
    let mut targets = Vec::new();

    if let Some(home) = home {
        for sub in HOME_SUBDIRS {
            let path = home.join(sub);
            if fs.is_dir(&path) {
                targets.push(path);
            }
        }
    }

    let volumes_root = Path::new(VOLUMES_ROOT);
    if fs.is_dir(volumes_root) {
        let mut volumes: Vec<PathBuf> = fs
            .list_dir(volumes_root)
            .into_iter()
            .filter(|p| fs.is_dir(p))
            .collect();
        // `read_dir` 순서는 파일시스템 마음이라 프롬프트 순서가 실행마다 달라진다.
        // 경로로 정렬해 사용자가 보는 순서를 고정한다.
        volumes.sort();
        targets.extend(volumes);
    }

    targets
}

// CoreGraphics 의 화면 기록 권한 API. 파일 계열 TCC 와 달리 "미리 물어보는" 공개
// API 가 있어서, 리소스를 몰래 건드려 유도할 필요 없이 정식으로 요청할 수 있다.
// 새 크레이트를 들이지 않고 두 함수만 직접 선언한다 — `surface.raw_key` 의
// CoreGraphics 선언(`src/adapters/ipc/handler/input_source.rs`)과 같은 방식.
//
// - `CGPreflightScreenCaptureAccess`: 현재 승인 상태만 조회한다. 프롬프트를 띄우지 않는다.
// - `CGRequestScreenCaptureAccess`: 미결정 상태면 시스템 프롬프트를 띄운다. 이미 거부된
//   상태면 프롬프트 없이 즉시 false 를 반환한다(사용자가 시스템 설정에서 직접 켜야 한다).
//
// 둘 다 macOS 10.15+ 이고 번들의 `LSMinimumSystemVersion` 은 11.0 이라 항상 존재한다.
#[cfg(all(target_os = "macos", feature = "gui"))]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// 화면 기록 권한이 지금 승인돼 있는가. 프롬프트를 띄우지 않는 순수 조회다.
///
/// **캡처 직전에 부른다** — 부팅 시점 값을 캐시해두면 그 사이 사용자가 시스템 설정에서
/// 권한을 바꾼 경우를 잘못 판정한다.
#[cfg(all(target_os = "macos", feature = "gui"))]
pub fn screen_recording_authorized() -> bool {
    // SAFETY: 인자도 반환 포인터도 없는 CoreGraphics C 함수 호출 — 포인터 수명/해제
    // 책임이 발생하지 않고, panic 을 가로지르는 상태도 남기지 않는다. 내부적으로
    // TCC 데몬에 현재 앱의 승인 상태를 묻기만 하며 AppKit 을 건드리지 않아
    // main thread 한정이 아니다(캡처 워커 스레드에서도 호출된다).
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// 비-macOS / headless — 화면 기록 권한이라는 개념이 없으므로 "승인됨"으로 답한다.
/// 그래야 캡처 경로가 다른 플랫폼에서 기존과 똑같이 동작한다.
#[cfg(not(all(target_os = "macos", feature = "gui")))]
pub fn screen_recording_authorized() -> bool {
    true
}

/// 화면 기록 권한을 **부팅당 1 회** 요청한다. 이미 승인돼 있으면 아무것도 하지 않는다.
///
/// 거부된 상태에서 다시 불러도 프롬프트는 뜨지 않고 즉시 false 가 돌아오므로, 재시도
/// 루프를 두지 않는다 — 그 상태를 되돌리는 것은 시스템 설정에서 사용자가 할 일이다.
#[cfg(all(target_os = "macos", feature = "gui"))]
fn prewarm_screen_recording() {
    if screen_recording_authorized() {
        tracing::debug!("prewarm: 화면 기록 권한 이미 승인됨");
        return;
    }
    // SAFETY: `screen_recording_authorized` 의 preflight 호출과 같은 근거 — 인자/반환
    // 포인터가 없는 CoreGraphics C 함수다. 미결정 상태에서만 프롬프트를 띄우고 사용자
    // 응답까지 블록할 수 있어 워커 스레드에서만 호출한다(메인 루프를 막지 않는다).
    let granted = unsafe { CGRequestScreenCaptureAccess() };
    tracing::debug!(granted, "prewarm: 화면 기록 권한 요청 결과");
}

// ── 손쉬운 사용 (Accessibility) ────────────────────────────────────────────────
//
// `surface.raw_key` 가 `CGEventPost` 로 시스템에 키를 주입하는데, 그 API 가
// `kTCCServiceAccessibility` 를 요구한다. 승인이 없으면 **프롬프트 없이 이벤트가
// 조용히 무시**돼 호출자는 성공 응답을 받고도 아무 일도 일어나지 않는 것을 본다.
//
// 화면 기록과 같은 성격으로 사전 요청 API 가 있다. `AXIsProcessTrusted()` 는 프롬프트
// 없이 현재 상태만 보고, `AXIsProcessTrustedWithOptions()` 에
// `kAXTrustedCheckOptionPrompt: true` 를 넘기면 안내를 띄운다.
//
// **프롬프트는 그 자리에서 권한을 켜주지 않는다** — "시스템 설정을 열겠느냐" 안내이고,
// 실제 토글은 사용자가 시스템 설정 > 개인정보 보호 및 보안 > 손쉬운 사용에서 한다.
// 켠 뒤에도 실행 중 프로세스에 즉시 반영되지 않아 재시작이 필요한 경우가 많다.
// 그래서 부팅당 1 회만 요청한다 — 미설정 상태에서 반복 호출하면 프롬프트가 계속 뜬다.

// 상태 조회 심볼. 소비자(주입 경로 · pre-warm · 설정 탭의 상태 행)가 전부 debug 로
// 내려가 release 에서는 참조가 0 이 되므로, 선언도 같은 cfg 로 내린다
// (gui 빌드는 `dead_code = deny` 라 선언만 남으면 빌드가 깨진다).
#[cfg(all(debug_assertions, target_os = "macos", feature = "gui"))]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

// 프롬프트를 띄우는 쪽(`prewarm_accessibility`)만 쓰는 심볼들. 그 함수가 debug 전용이
// 되면서 release 에서는 참조가 0 이 되는데, gui 빌드는 `dead_code = deny` 라 선언만
// 남아 있으면 빌드가 깨진다. 그래서 선언도 같은 cfg 로 내린다.
#[cfg(all(debug_assertions, target_os = "macos", feature = "gui"))]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
    /// 옵션 딕셔너리의 키(`CFStringRef` 전역). 문자열 값을 직접 만들지 않고 프레임워크가
    /// 내보내는 심볼을 그대로 쓴다 — 값이 바뀌어도 따라간다.
    static kAXTrustedCheckOptionPrompt: *const std::ffi::c_void;
}

// CoreFoundation 쪽도 전량 `prewarm_accessibility` 전용이다(위와 같은 이유로 debug 한정).
#[cfg(all(debug_assertions, target_os = "macos", feature = "gui"))]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const std::ffi::c_void;
    /// CFType 표준 콜백. 이걸 넘겨야 딕셔너리가 키를 **CFEqual 로** 비교하고
    /// retain/release 를 관리한다. null 콜백은 포인터 동일성 비교라 여기선 부적절하다.
    static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
    static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
    fn CFDictionaryCreate(
        alloc: *const std::ffi::c_void,
        keys: *const *const std::ffi::c_void,
        values: *const *const std::ffi::c_void,
        count: isize,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFRelease(cf: *const std::ffi::c_void);
}

/// 손쉬운 사용 권한이 지금 승인돼 있는가. 프롬프트를 띄우지 않는 순수 조회다.
///
/// **호출 시점마다 다시 묻는다** — 부팅 값을 캐시하면 그 사이 사용자가 설정을 바꾼
/// 경우를 잘못 판정한다. 이 권한은 켠 뒤 반영에 재시작이 필요한 경우까지 있어서
/// 캐시가 특히 위험하다.
///
/// **debug 빌드 전용.** 이 값을 읽는 곳은 셋뿐이고 셋 다 debug 다 — 주입 경로
/// (`surface.raw_key`), pre-warm 요청, 설정 권한 탭의 상태 행. release 에는 이
/// 권한을 소비하는 코드가 없으므로 상태를 물을 이유도 없다.
#[cfg(all(debug_assertions, target_os = "macos", feature = "gui"))]
pub fn accessibility_trusted() -> bool {
    // SAFETY: 인자도 반환 포인터도 없는 ApplicationServices C 함수 호출 — 포인터
    // 수명/해제 책임이 생기지 않는다. 현재 프로세스의 TCC 승인 상태를 묻기만 하고
    // 프롬프트를 띄우지 않으므로 블록하지 않으며, AppKit 을 건드리지 않아 main
    // thread 한정이 아니다(IPC 핸들러 스레드에서도 호출된다).
    unsafe { AXIsProcessTrusted() }
}

/// 비-macOS / headless — 손쉬운 사용 권한 개념이 없으므로 "승인됨" 으로 답한다.
/// 그래야 주입 경로가 다른 플랫폼에서 기존과 똑같이 동작한다. macOS 구현과 같은
/// 이유로 debug 한정이다.
#[cfg(all(debug_assertions, not(all(target_os = "macos", feature = "gui"))))]
pub fn accessibility_trusted() -> bool {
    true
}

/// 권한 판정 결과로 `surface.raw_key` 가 무엇을 할지. 승인 전에는 주입하지 않는다 —
/// 승인 없이 `CGEventPost` 를 부르면 조용히 무시돼 "성공했다는데 아무 일도 안 일어남"
/// 이 되고, 호출자가 원인을 알 방법이 없다.
///
/// 유일한 소비자(`surface.raw_key` 핸들러)가 debug 전용이라 release 에서는 참조가 0 이
/// 된다 — gui 빌드는 `dead_code = deny` 라 선언만 남으면 빌드가 깨지므로 선언도 같은
/// cfg 로 내린다. 순수 규칙 테스트는 release 테스트에서도 돌아야 하므로 `test` 를 포함한다.
#[cfg(any(debug_assertions, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKeyDecision {
    /// 승인됨 — 그대로 주입한다.
    Inject,
    /// 미승인 — 주입하지 않고 권한 부재를 에러로 돌려준다.
    PermissionDenied,
}

/// 위 판정의 **순수** 규칙. FFI 호출과 분리해 두면 macOS 밖에서도 검증된다.
#[cfg(any(debug_assertions, test))]
pub fn raw_key_decision(accessibility_trusted: bool) -> RawKeyDecision {
    if accessibility_trusted {
        RawKeyDecision::Inject
    } else {
        RawKeyDecision::PermissionDenied
    }
}

/// 손쉬운 사용 권한을 **부팅당 1 회** 요청한다. 이미 승인돼 있으면 아무것도 하지 않는다.
///
/// **debug 빌드 전용.** 이 권한을 소비하는 표면(`surface.raw_key` — `CGEventPost` 로
/// OS 이벤트 스트림에 키 주입)이 debug 로 격리돼 있어
/// ([ADR-0115](../../docs/adr/0115-input-reproduction-ipc-debug-isolation.md)),
/// release 빌드에는 이 권한을 쓰는 코드가 하나도 없다. 소비자가 0 인데 첫 실행에
/// "이 앱이 내 모든 입력을 볼 수 있게 해달라" 로 읽히는 프롬프트를 띄우는 것은
/// 최소권한 원칙에 어긋난다. 그래서 요청 자체를 debug 로 내린다 — release 사용자는
/// 이 프롬프트를 보지 않고, 손쉬운 사용은 켤 필요가 없는 항목이 된다.
#[cfg(all(debug_assertions, target_os = "macos", feature = "gui"))]
fn prewarm_accessibility() {
    if accessibility_trusted() {
        tracing::debug!("prewarm: 손쉬운 사용 권한 이미 승인됨");
        return;
    }
    // SAFETY: `kAXTrustedCheckOptionPrompt: kCFBooleanTrue` 딕셔너리를 만들어 넘기는
    // 표준 호출 시퀀스다.
    // - 키/값은 프레임워크가 소유하는 전역 CF 객체라 이쪽에 해제 책임이 없다.
    // - CFType 표준 콜백을 넘겨 딕셔너리가 키를 CFEqual 로 비교하고 retain 을 관리한다.
    // - `CFDictionaryCreate` 는 +1 retain 으로 돌아오므로 사용 직후 `CFRelease` 로 짝을
    //   맞춘다. 그 사이에 조기 반환이나 panic 지점이 없다.
    // - 프롬프트가 뜨는 동안 블록할 수 있어 워커 스레드에서만 호출한다.
    // CF 시퀀스가 한 트랜잭션이라 분할하면 retain/release 짝이 흩어진다.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    let granted = unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        );
        let trusted = AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
        trusted
    };
    // 프롬프트는 "설정을 열겠느냐" 안내라, 여기서 false 여도 사용자가 이제부터 켤 수 있다.
    tracing::debug!(granted, "prewarm: 손쉬운 사용 권한 요청 결과");
}

// ── Full Disk Access ──────────────────────────────────────────────────────────
//
// FDA(`kTCCServiceSystemPolicyAllFiles`)를 부여하면 "다른 앱의 데이터" 를 포함한
// **파일 접근 계열 전부**가 프롬프트 없이 통과한다. 파일 pre-warm 이 못 덮는
// AppData 계열을 없앨 수 있는 유일한 수단이다.
//
// 앱이 FDA 를 **요청할 방법은 없다** — 사용자가 시스템 설정에서 직접 추가해야 하고
// `tccutil`/TCC.db 조작은 SIP 가 막는다. 앱이 할 수 있는 건 (a) 보유 추정과
// (b) 해당 패널로 보내는 안내뿐이다.

/// 시스템 설정의 전체 디스크 접근 권한 패널 딥링크.
#[cfg(all(target_os = "macos", feature = "gui"))]
pub const FULL_DISK_ACCESS_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

/// FDA 보유를 **추정**하는 데 읽어보는 경로들. 앞쪽부터 시도해 하나라도 열리면
/// 보유로 본다.
///
/// FDA 없이는 열리지 않는 것으로 알려진 경로를 읽어보는 우회 판정이다 — 보유 여부를
/// 묻는 공개 API 자체가 없다. 이 경로들은 거부될 때 **프롬프트를 띄우지 않고 조용히**
/// `EPERM` 을 내므로 백그라운드에서 안전하게 시도할 수 있다.
fn fda_probe_paths(home: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(
        "/Library/Application Support/com.apple.TCC/TCC.db",
    )];
    // 보조 경로 — 시스템 경로의 보호 정책이 바뀌었을 때의 완화책. 사용자별 TCC 저장소도
    // 같은 보호를 받으므로 판정 신호로 쓸 수 있다.
    if let Some(home) = home {
        paths.push(home.join("Library/Application Support/com.apple.TCC/TCC.db"));
    }
    paths
}

/// 부팅 안내를 띄울지 결정하는 **순수** 규칙 — 아직 안내한 적이 없고 FDA 가 없어
/// 보일 때만 띄운다.
///
/// 추정이 틀릴 수 있으므로(아래 `full_disk_access_likely` 참고) 이 값은 **안내 표시
/// 여부에만** 쓰고 기능 분기에는 쓰지 않는다. 오탐으로 안내가 떠도 평생 1 회이며,
/// 설정에서 다시 켤 수 있다.
fn should_show_fda_notice(already_shown: bool, fda_likely: bool) -> bool {
    !already_shown && !fda_likely
}

/// FDA 를 갖고 있는 것으로 **보이는가**. 확정 판정이 아니다 — 공개 API 가 없어
/// "FDA 로만 읽히는 것으로 알려진 경로가 열리는가" 로 대신하는 휴리스틱이며,
/// macOS 가 그 경로의 보호 정책을 바꾸면 오탐이 날 수 있다.
#[cfg(all(target_os = "macos", feature = "gui"))]
pub fn full_disk_access_likely() -> bool {
    fda_probe_paths(home_dir().as_deref())
        .iter()
        .any(|p| std::fs::File::open(p).is_ok())
}

/// 부팅 시 FDA 안내를 띄워야 하는가.
#[cfg(all(target_os = "macos", feature = "gui"))]
pub fn wants_full_disk_access_notice(settings: &tasty_settings::Settings) -> bool {
    should_show_fda_notice(
        settings.general.macos_fda_notice_shown,
        full_disk_access_likely(),
    )
}

/// 비-macOS / headless — FDA 개념이 없으므로 안내하지 않는다.
#[cfg(not(all(target_os = "macos", feature = "gui")))]
pub fn wants_full_disk_access_notice(_settings: &tasty_settings::Settings) -> bool {
    false
}

/// 안내를 띄웠음을 기록하고 즉시 영속화한다 — 다음 부팅부터는 뜨지 않는다.
/// 저장 실패는 안내를 한 번 더 보게 될 뿐이라 치명적이지 않다(warn 로그).
#[cfg(all(target_os = "macos", feature = "gui"))]
pub fn mark_full_disk_access_notice_shown(settings: &mut tasty_settings::Settings) {
    settings.general.macos_fda_notice_shown = true;
    if let Err(err) = settings.save() {
        tracing::warn!(%err, "full disk access 안내 표시 기록 저장 실패");
    }
}

/// 비-macOS / headless — 기록할 것이 없다.
#[cfg(not(all(target_os = "macos", feature = "gui")))]
pub fn mark_full_disk_access_notice_shown(_settings: &mut tasty_settings::Settings) {}

/// 시스템 설정의 전체 디스크 접근 권한 패널을 연다. `open(1)` 로 띄운다 —
/// `x-apple.systempreferences:` 는 브라우저가 아니라 OS 기본 핸들러가 처리한다.
/// 프로세스를 기다리지 않는다(렌더 경로에서 호출될 수 있다).
#[cfg(all(target_os = "macos", feature = "gui"))]
pub fn open_full_disk_access_settings() {
    #[cfg(debug_assertions)]
    if crate::debug_os_open::intercepted("open_settings", FULL_DISK_ACCESS_SETTINGS_URL) {
        return;
    }
    if let Err(err) = std::process::Command::new("open")
        .arg(FULL_DISK_ACCESS_SETTINGS_URL)
        .spawn()
    {
        tracing::warn!(%err, "전체 디스크 접근 권한 설정 패널 열기 실패");
    }
}

/// 홈 디렉터리 — `DirectoriesHome` 과 같은 해석(`directories::BaseDirs`).
#[cfg(all(target_os = "macos", feature = "gui"))]
fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

/// 부팅 직후 권한 프롬프트를 순차로 발화한다. 호출 즉시 반환한다.
///
/// **반드시 워커 스레드**다. 프롬프트가 떠 있는 동안 `read_dir`(과 화면 기록 요청)은
/// 사용자가 응답할 때까지 리턴하지 않으므로, 메인 스레드(winit 이벤트 루프)에서
/// 호출하면 프롬프트가 떠 있는 내내 UI 가 얼어붙고 `boot_total` 계측도 사용자 응답
/// 시간만큼 부풀려진다.
///
/// **하나의 스레드에서 하나씩 순차로** 처리한다. 동시에 건드리면 프롬프트가 겹쳐 뜬다 —
/// 순차면 앞의 것을 닫아야 다음이 뜬다. 순서는 파일 폴더 → 화면 기록 → 손쉬운 사용:
/// 앞의 둘은 그 자리에서 허용/거부가 끝나지만 손쉬운 사용 프롬프트는 시스템 설정으로
/// 사용자를 내보내므로, 그 이탈을 시퀀스 맨 끝에 둔다. 마지막 손쉬운 사용은 **debug
/// 빌드에서만** 돈다 — release 에는 그 권한을 소비하는 코드가 없다
/// (`prewarm_accessibility` 참고).
#[cfg(all(target_os = "macos", feature = "gui"))]
pub fn spawn_prewarm() {
    std::thread::spawn(|| {
        let targets = prewarm_targets(home_dir().as_deref(), &RealFs);
        tracing::debug!(count = targets.len(), "prewarm: 파일 TCC 대상 결정");
        for path in targets {
            // 결과는 버린다 — 거부는 사용자의 정당한 선택이라 정상 결과이고, 성공해도
            // 엔트리를 쓸 데가 없다. 목적은 접근 시도 자체(= 프롬프트 발화)뿐이다.
            match std::fs::read_dir(&path) {
                Ok(_) => tracing::debug!(path = %path.display(), "prewarm: 접근 허용"),
                Err(err) => {
                    tracing::debug!(path = %path.display(), %err, "prewarm: 접근 불가(거부 또는 부재)")
                }
            }
        }
        prewarm_screen_recording();
        // 손쉬운 사용은 debug 빌드에서만 요청한다 — release 에는 소비자가 없다.
        #[cfg(debug_assertions)]
        prewarm_accessibility();
    });
}

/// 비-macOS / headless 는 no-op — 호출부에 `#[cfg]` 를 흩뿌리지 않기 위한 짝.
/// headless 에는 프롬프트를 띄울 GUI 주체가 없으므로 macOS 여도 돌지 않는다.
#[cfg(not(all(target_os = "macos", feature = "gui")))]
pub fn spawn_prewarm() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// 경로 집합만 들고 있는 가짜 파일시스템. `dirs` 는 디렉터리, `files` 는 그 외
    /// 엔트리 — `read_dir` 이 디렉터리가 아닌 것도 돌려준다는 사실을 재현한다.
    struct FakeFs {
        dirs: BTreeSet<PathBuf>,
        files: BTreeSet<PathBuf>,
    }

    impl FakeFs {
        fn new<'a>(dirs: impl IntoIterator<Item = &'a str>) -> Self {
            Self {
                dirs: dirs.into_iter().map(PathBuf::from).collect(),
                files: BTreeSet::new(),
            }
        }

        fn with_files<'a>(mut self, files: impl IntoIterator<Item = &'a str>) -> Self {
            self.files = files.into_iter().map(PathBuf::from).collect();
            self
        }
    }

    impl FsProbe for FakeFs {
        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.contains(path)
        }

        fn list_dir(&self, path: &Path) -> Vec<PathBuf> {
            // 실제 `read_dir` 처럼 순서를 보장하지 않는 소스를 흉내내려 역순으로 준다 —
            // 정렬 책임이 `prewarm_targets` 에 있는지 확인하기 위함.
            let mut out: Vec<PathBuf> = self
                .dirs
                .iter()
                .chain(self.files.iter())
                .filter(|p| p.parent() == Some(path))
                .cloned()
                .collect();
            out.sort();
            out.reverse();
            out
        }
    }

    #[test]
    fn raw_key_injects_only_when_accessibility_is_trusted() {
        assert_eq!(raw_key_decision(true), RawKeyDecision::Inject);
        // 미승인 상태에서 주입하면 CGEventPost 가 조용히 무시된다 — 성공으로 답하면 안 된다.
        assert_eq!(raw_key_decision(false), RawKeyDecision::PermissionDenied);
    }

    #[test]
    fn fda_notice_shows_only_when_unshown_and_access_missing() {
        assert!(should_show_fda_notice(false, false));
        // 이미 안내했으면 다시 띄우지 않는다.
        assert!(!should_show_fda_notice(true, false));
        // FDA 가 있어 보이면 안내할 이유가 없다.
        assert!(!should_show_fda_notice(false, true));
        assert!(!should_show_fda_notice(true, true));
    }

    #[test]
    fn fda_probe_includes_system_store_first_then_user_store() {
        let paths = fda_probe_paths(Some(Path::new("/Users/t")));
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/Library/Application Support/com.apple.TCC/TCC.db"),
                PathBuf::from("/Users/t/Library/Application Support/com.apple.TCC/TCC.db"),
            ]
        );
    }

    #[test]
    fn fda_probe_without_home_keeps_the_system_store() {
        assert_eq!(
            fda_probe_paths(None),
            vec![PathBuf::from(
                "/Library/Application Support/com.apple.TCC/TCC.db"
            )]
        );
    }

    #[test]
    fn home_folders_come_first_in_fixed_order() {
        let fs = FakeFs::new([
            "/Users/t/Downloads",
            "/Users/t/Documents",
            "/Users/t/Desktop",
        ]);
        let targets = prewarm_targets(Some(Path::new("/Users/t")), &fs);
        assert_eq!(
            targets,
            vec![
                PathBuf::from("/Users/t/Downloads"),
                PathBuf::from("/Users/t/Documents"),
                PathBuf::from("/Users/t/Desktop"),
            ]
        );
    }

    #[test]
    fn missing_paths_are_skipped() {
        // Documents 만 없는 홈.
        let fs = FakeFs::new(["/Users/t/Downloads", "/Users/t/Desktop"]);
        let targets = prewarm_targets(Some(Path::new("/Users/t")), &fs);
        assert_eq!(
            targets,
            vec![
                PathBuf::from("/Users/t/Downloads"),
                PathBuf::from("/Users/t/Desktop"),
            ]
        );
    }

    #[test]
    fn no_home_yields_no_home_targets() {
        let fs = FakeFs::new(["/Users/t/Downloads"]);
        assert!(prewarm_targets(None, &fs).is_empty());
    }

    #[test]
    fn absent_volumes_root_contributes_nothing() {
        let fs = FakeFs::new(["/Users/t/Desktop"]);
        let targets = prewarm_targets(Some(Path::new("/Users/t")), &fs);
        assert_eq!(targets, vec![PathBuf::from("/Users/t/Desktop")]);
    }

    #[test]
    fn empty_volumes_root_contributes_nothing() {
        let fs = FakeFs::new(["/Users/t/Desktop", "/Volumes"]);
        let targets = prewarm_targets(Some(Path::new("/Users/t")), &fs);
        assert_eq!(targets, vec![PathBuf::from("/Users/t/Desktop")]);
    }

    #[test]
    fn mounted_volumes_come_last_and_are_sorted() {
        let fs = FakeFs::new([
            "/Users/t/Downloads",
            "/Volumes",
            "/Volumes/Backup",
            "/Volumes/Archive",
        ]);
        let targets = prewarm_targets(Some(Path::new("/Users/t")), &fs);
        assert_eq!(
            targets,
            vec![
                PathBuf::from("/Users/t/Downloads"),
                PathBuf::from("/Volumes/Archive"),
                PathBuf::from("/Volumes/Backup"),
            ]
        );
    }

    #[test]
    fn volumes_are_probed_shallowly() {
        // 볼륨 하위 디렉터리는 목록에 들어가지 않는다 — 마운트당 한 번만 건드린다.
        let fs = FakeFs::new(["/Volumes", "/Volumes/Backup", "/Volumes/Backup/nested"]);
        let targets = prewarm_targets(None, &fs);
        assert_eq!(targets, vec![PathBuf::from("/Volumes/Backup")]);
    }

    #[test]
    fn non_directory_volume_entries_are_skipped() {
        // `/Volumes` 하위의 비-디렉터리(예: `.DS_Store`)는 대상이 아니다 — 파일을
        // `read_dir` 해봐야 볼륨 프롬프트가 뜨지 않는다.
        let fs = FakeFs::new(["/Volumes", "/Volumes/Backup"]).with_files(["/Volumes/.DS_Store"]);
        assert_eq!(
            prewarm_targets(None, &fs),
            vec![PathBuf::from("/Volumes/Backup")]
        );
    }
}
