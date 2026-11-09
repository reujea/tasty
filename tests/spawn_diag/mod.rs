//! 인스턴스 하네스의 바이너리·환경 설정과 stderr 수집·진단을 공유한다.
//! 종료 상태와 로그 시그니처를 함께 보여 주며 로그의 양이나 시각만으로 부팅 원인을 확정하지 않는다.

// 공유 모듈을 포함하는 테스트 바이너리마다 사용하는 함수가 달라 미사용 함수도 제공한다.
#![allow(dead_code)]
// 시험 본문은 제품 코드의 let _ 사유 주석 정책에서 제외된다.
#![allow(clippy::let_underscore_must_use)]

use std::time::Duration;

/// 제품의 로그 환경변수와 일치하는지 harness_log_env에서 확인한다.
pub const LOG_ENV: &str = "TASTY_LOG";

/// 기본 필터와 추가 필터가 같은 리터럴을 쓰도록 매크로로 공유한다.
macro_rules! product_default_filter {
    () => {
        "warn,wgpu_hal=error,wgpu_core=error,naga=error,egui_winit::clipboard=off"
    };
}

/// 명시한 로그 필터는 제품 기본값을 대체하므로 기본 억제 설정도 함께 포함한다.
pub const LOG_FILTER: &str = product_default_filter!();

/// 웹훅 리스너 시작 로그를 추가한다. 꼬리에 이 줄이 없다는 사실만으로 시작 실패를 단정하지는 않는다.
pub const LOG_FILTER_WEBHOOK: &str =
    concat!(product_default_filter!(), ",tasty::webhook::listener=info");

/// libtest 캡처에 하네스 로그를 남긴다. 이미 subscriber가 설치돼 있으면 유지한다.
pub fn init_test_tracing() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
}

/// 기본은 테스트와 같은 feature로 빌드한 CARGO_BIN_EXE_tasty다.
/// TASTY_E2E_BIN으로 미리 빌드한 헤드리스 데몬을 지정할 수 있다. 빌드 조합에 의존하는 시험은 이 값을 적용하지 않는다.
/// 서로 다른 조합이 같은 tasty 경로를 덮지 않도록 override는 별도 CARGO_TARGET_DIR의 산출물을 사용한다.
/// 플러그인이 필요한 시험은 작업 공간의 번들 탐색 경로와 --workspace 빌드도 준비해야 한다.
/// 자세한 절차는 docs/dev-guide/e2e-tests.md를 따른다.
pub fn instance_bin() -> std::ffi::OsString {
    let from_env = std::env::var_os(INSTANCE_BIN_ENV);
    // override를 적용하지 않는 스위트도 입력 경로가 파일인지 확인한다. 실행 권한까지 검사하지는 않는다.
    if let Some(v) = from_env.as_deref()
        && !v.is_empty()
        && !std::path::Path::new(v).is_file()
    {
        panic!(
            "{INSTANCE_BIN_ENV} 가 가리키는 경로에 실행 파일이 없다: {}\n\
             별도 CARGO_TARGET_DIR 로 빌드한 산출물의 절대경로여야 한다 — \
             docs/dev-guide/e2e-tests.md",
            std::path::Path::new(v).display()
        );
    }
    let effective = effective_override(daemon_kind(), from_env);
    if let Some(v) = effective.as_deref()
        && !v.is_empty()
        && let Some(newer) = source_newer_than(std::path::Path::new(v), repo_roots())
    {
        panic!(
            "{INSTANCE_BIN_ENV} 바이너리보다 mtime이 같거나 새로운 Rust 소스를 찾았다.\n  바이너리: {}\n  소스: {}\nmtime만으로 내용 차이를 확정하지는 못한다. 현재 소스로 다시 빌드하거나(scripts/build-e2e-headless.sh) {INSTANCE_BIN_ENV}=로 override를 해제한다. docs/dev-guide/e2e-tests.md",
            std::path::Path::new(v).display(),
            newer.display()
        );
    }
    resolve_instance_bin(effective.as_deref(), env!("CARGO_BIN_EXE_tasty"))
}

fn effective_override(
    kind: DaemonKind,
    from_env: Option<std::ffi::OsString>,
) -> Option<std::ffi::OsString> {
    match kind {
        DaemonKind::SameCombo => None,
        DaemonKind::HeadlessOk => from_env,
    }
}

/// Rust 소스의 시각만 비교한다. 설정·자원 파일 변경은 이 검사에 포함되지 않는다.
fn repo_roots() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![root.join("src"), root.join("crates")]
}

/// 바이너리 mtime 이상인 Rust 파일을 하나 찾으면 반환한다. 같은 시각은 쓰기 순서를 알 수 없어 거절한다.
/// 내용을 비교하지 않으므로 실제 빌드 신선도의 증명은 아니다. 파일·디렉터리 읽기 실패는 건너뛰어 누락할 수 있다.
fn source_newer_than(
    bin: &std::path::Path,
    roots: Vec<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let bin_mtime = std::fs::metadata(bin).and_then(|m| m.modified()).ok()?;
    let mut stack = roots;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let newer = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .map(|m| m >= bin_mtime)
                .unwrap_or(false);
            if newer {
                return Some(p);
            }
        }
    }
    None
}

pub enum DaemonKind {
    /// 테스트와 같은 빌드 조합이 필요해 override를 적용하지 않는다.
    SameCombo,
    /// 헤드리스 override를 사용할 수 있는 스위트다.
    HeadlessOk,
}

/// 알려진 스위트만 헤드리스 override를 허용한다. 미등록 스위트는 같은 빌드 조합을 유지한다.
/// 목록은 e2e_single_instance_guard의 인스턴스 스위트 목록과 대조한다.
const HEADLESS_OK_SUITES: &[&str] = &[
    "attach_attention_loopback",
    "attach_convert_cwd_loopback",
    "attach_git_query_loopback",
    "attach_list_dir_loopback",
    "attach_local_creation_tap",
    "attach_markdown_content_loopback",
    "attach_html_transfer_loopback",
    "attach_silent_disconnect",
    "hook_env_integration",
    "hooks_detection_e2e",
    "plugin_disable_withdraws_surface_kinds",
    "shared_instance_harness",
    "soak_memory",
    "webhook_integration",
];

/// 테스트의 cfg뿐 아니라 검증 대상 서버 경로가 빌드 조합에 의존할 수도 있다.
/// attach_structure_sync_loopback처럼 같은 단정으로 서로 다른 서버 함수를 검사하는 경우도 같은 조합을 유지한다.
pub fn daemon_kind() -> DaemonKind {
    if HEADLESS_OK_SUITES.contains(&env!("CARGO_CRATE_NAME")) {
        DaemonKind::HeadlessOk
    } else {
        DaemonKind::SameCombo
    }
}

/// 환경변수를 직접 바꾸지 않고 선택 규칙을 검사하도록 분리했다. 빈 override는 미설정과 같다.
fn resolve_instance_bin(
    from_env: Option<&std::ffi::OsStr>,
    default_bin: &str,
) -> std::ffi::OsString {
    match from_env {
        Some(v) if !v.is_empty() => v.to_os_string(),
        _ => std::ffi::OsString::from(default_bin),
    }
}

pub const INSTANCE_BIN_ENV: &str = "TASTY_E2E_BIN";

/// 포트 파일을 기다리는 공통 상한이다. 기존 두 하네스의 상한 중 큰 값을 유지했다.
pub const SPAWN_PORT_TIMEOUT: Duration = Duration::from_secs(40);

/// 포트 확인 뒤 첫 서피스의 셸 출력을 기다리는 상한이다.
pub const SPAWN_SHELL_TIMEOUT: Duration = Duration::from_secs(20);

/// OnceLock 초기화가 panic해도 다음 호출이 다시 spawn하지 않도록 막는다.
/// 상태는 하네스별로 보관해 한 하네스의 실패가 다른 하네스의 초기화를 막지 않게 한다.
///
/// ```ignore
/// static SPAWN_LATCH: spawn_diag::SpawnOnceLatch = spawn_diag::SpawnOnceLatch::new();
/// SHARED_INSTANCE.get_or_init(|| {
///     SPAWN_LATCH.entering("gui 공유 인스턴스");
///     let inst = Instance::spawn();
///     SPAWN_LATCH.succeeded();
///     inst
/// })
/// ```
///
/// 타입 단위 시험은 재진입 차단·성공 해제를 확인한다. spawn_latch_precedes_the_spawn은 배치 순서를 확인한다.
/// shared_instance_harness의 부팅 차단 시험은 common 하네스의 실제 효과를 확인하며 GUI 하네스까지 검증하지는 않는다.
pub struct SpawnOnceLatch {
    failed: std::sync::atomic::AtomicBool,
}

impl SpawnOnceLatch {
    pub const fn new() -> Self {
        Self {
            failed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 초기화 클로저에 들어갈 때 호출한다. 이전 진입이 성공하지 않았다면 재시도 대신 첫 실패 진단으로 안내한다.
    pub fn entering(&self, what: &str) {
        assert!(
            !self.failed.swap(true, std::sync::atomic::Ordering::SeqCst),
            "{what}의 첫 spawn이 이미 실패해 재시도하지 않는다. 원인과 stderr는 이 테스트 바이너리의 첫 실패 메시지를 확인한다."
        );
    }

    /// 초기화 성공 뒤 래치를 해제한다. 성공한 인스턴스의 후속 사용까지 차단하면 안 된다.
    pub fn succeeded(&self) {
        self.failed
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for SpawnOnceLatch {
    fn default() -> Self {
        Self::new()
    }
}

/// stderr를 읽지 않으면 파이프가 차서 자식이 쓰기에서 멈출 수 있어 배경 스레드로 수집한다.
/// last_line_age는 값을 반환하기 전에 락을 해제한다. panic 인자에서 락 가드를 보관하면 해제 전에 panic해 뮤텍스를 poison 상태로 만들 수 있다.
pub struct StderrCapture {
    ring: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
    last_at: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
    drain: Option<std::thread::JoinHandle<()>>,
    tail_lines: usize,
}

/// 링 보관량과 진단에 표시할 꼬리 줄 수는 별개다.
const STDERR_RING_CAPACITY: usize = 256;

pub const STDERR_TAIL_LINES: usize = 30;

/// 종료한 자식의 stderr 배출을 기다리는 상한이다. 스레드 지연이나 파이프를 물려받은 프로세스 때문에 무한 대기하지 않게 한다.
pub const STDERR_SETTLE_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

impl StderrCapture {
    /// stderr가 없으면 수집 없이 빈 꼬리를 반환한다.
    pub fn start(stderr: Option<std::process::ChildStderr>, tail_lines: usize) -> Self {
        let ring = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::with_capacity(STDERR_RING_CAPACITY),
        ));
        let last_at = std::sync::Arc::new(std::sync::Mutex::new(None));
        let drain = stderr.map(|stderr| {
            let ring = std::sync::Arc::clone(&ring);
            let last_at = std::sync::Arc::clone(&last_at);
            std::thread::spawn(move || {
                use std::io::BufRead as _;
                let reader = std::io::BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    *lock(&last_at) = Some(std::time::Instant::now());
                    let mut ring = lock(&ring);
                    if ring.len() == STDERR_RING_CAPACITY {
                        ring.pop_front();
                    }
                    ring.push_back(line);
                }
            })
        });
        Self {
            ring,
            last_at,
            drain,
            tail_lines,
        }
    }

    pub fn tail(&self) -> String {
        let ring = lock(&self.ring);
        let start = ring.len().saturating_sub(self.tail_lines);
        ring.iter()
            .skip(start)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn tail_lines(&self) -> usize {
        self.tail_lines
    }

    /// 마지막 줄 이후 경과를 반환하고 이 메서드 안에서 락을 해제한다.
    pub fn last_line_age(&self) -> Option<std::time::Duration> {
        let at = *lock(&self.last_at);
        at.map(|t| t.elapsed())
    }

    /// 자식 종료 확인 뒤 호출한다. 종료 확인이 배출 스레드보다 빠를 수 있어 상한까지 기다린다.
    /// 상한이 지나면 현재 꼬리와 수집이 끝나지 않았다는 진단을 반환한다.
    pub fn tail_after_exit(&mut self, budget: std::time::Duration) -> String {
        let deadline = std::time::Instant::now() + budget;
        let settled = loop {
            match self.drain.as_ref() {
                None => break true,
                Some(handle) if handle.is_finished() => break true,
                Some(_) if std::time::Instant::now() >= deadline => break false,
                Some(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        };
        if settled {
            self.join();
            return self.tail();
        }
        format!(
            "{}\n(stderr 수집이 {budget:?} 안에 끝나지 않아 꼬리가 일부 빠졌을 수 있다. 배출 스레드 지연과 파이프를 상속한 프로세스를 확인한다.)",
            self.tail()
        )
    }

    pub fn find(&self, pred: impl Fn(&str) -> bool) -> Option<String> {
        let ring = lock(&self.ring);
        ring.iter().find(|line| pred(line)).cloned()
    }

    /// 배출 스레드를 join한다. EOF가 오기 전에는 기다리므로 자식을 종료한 뒤 호출한다.
    /// 손자가 파이프를 상속하면 자식 종료만으로 EOF가 보장되지 않는다. Drop에서 자동 join하지 않으며 호출 순서는 하네스가 책임진다.
    pub fn join(&mut self) {
        if let Some(handle) = self.drain.take() {
            // 이유: 정리 중 스레드 panic을 다시 전파해 나머지 정리와 원래 실패 진단을 가리지 않는다.
            let _ = handle.join();
        }
    }
}

/// 진단 수집이 중단되지 않도록 poison 상태에서도 링과 시각 데이터를 사용한다.
fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 부팅 대기 중의 자식을 소유한다. 완성된 인스턴스로 넘기기 전에 panic하면 종료와 회수를 시도한다. Child 자체의 Drop은 종료하지 않는다.
pub struct ChildReaper {
    child: Option<std::process::Child>,
}

impl ChildReaper {
    pub fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }

    pub fn child(&mut self) -> &mut std::process::Child {
        self.child
            .as_mut()
            .expect("release 뒤에는 이 값이 없다 — release 는 self 를 소비한다")
    }

    pub fn release(mut self) -> std::process::Child {
        self.child
            .take()
            .expect("release 는 한 번만 불린다 — self 를 소비한다")
    }
}

impl Drop for ChildReaper {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            kill_process_tree(&mut child);
            // 이유: panic 중 정리 오류를 재전파하지 않는다. kill과 wait의 실패도 무시하므로 종료 성공을 보장하지는 않는다.
            let _ = child.wait();
        }
    }
}

/// Windows는 프로세스 트리, 다른 플랫폼은 직접 자식의 종료를 요청한다.
pub fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(target_os = "windows")]
    {
        // 이유: 정리 중 taskkill 실패를 무시한다. 이미 종료됐거나 다른 오류가 난 경우를 구별하지 않는다.
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child.id().to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 이유: 정리 중 kill 실패를 무시한다. 이미 종료된 경우 외의 오류도 있을 수 있다.
        let _ = child.kill();
    }
}

/// Linux에서는 부모 스레드 종료 시 자식을 종료하도록 설정한다. 다른 플랫폼은 일반 spawn이다.
/// libtest 작업 스레드는 일찍 끝날 수 있어 fork를 전용 스레드에서 수행하고 그 스레드를 프로세스 수명 동안 유지한다.
pub fn spawn_child(mut command: std::process::Command) -> std::io::Result<std::process::Child> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;

        let bind_to_parent = || {
            // SAFETY: 인자를 포인터로 받지 않는 async-signal-safe 시스템 콜이다(힙 할당·락 없음).
            let ret = unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
            if ret != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // fork와 prctl 사이에 부모가 종료돼 PID1로 재부모화된 경우 exec를 거절한다.
            // SAFETY: getppid는 인자가 없는 시스템 콜이다.
            if unsafe { libc::getppid() } == 1 {
                return Err(std::io::Error::other("parent already gone before exec"));
            }
            Ok(())
        };
        // SAFETY: 포인터 인자가 없는 시스템 콜을 자식의 exec 전에 호출한다. 오류 생성 경로도 fork 이후의 할당·락 제한을 따라야 한다.
        unsafe {
            command.pre_exec(bind_to_parent);
        }

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("tasty-test-fork-anchor".into())
            .spawn(move || {
                let result = command.spawn();
                // 이유: 수신자가 없어지면 spawn 결과를 보고할 곳도 없다.
                let _ = tx.send(result);
                // 이 스레드가 끝나면 커널이 자식을 종료하므로 반환하지 않는다.
                loop {
                    std::thread::park();
                }
            })
            .expect("spawn fork-anchor thread");
        rx.recv().expect("fork-anchor thread died before replying")
    }
    #[cfg(not(target_os = "linux"))]
    {
        command.spawn()
    }
}

enum BootBlocker {
    /// 디스플레이 설정 부재 또는 연결 실패를 나타내는 로그다.
    NoDisplay(&'static str),
    /// GPU 관련 로그 단서이며 실패 원인을 확정하지 않는다.
    GpuFallback(&'static str),
}

/// GPU 폴백이나 장치 관련 로그 표지다. 정상 부팅에도 나올 수 있어 실패 원인을 확정하는 데 쓰지 않는다.
const GPU_FALLBACK_MARKERS: &[&str] = &[
    "renderD128", // DRM 렌더 노드 — 열지 못했다(점유 중이거나 접근 불가)
    "VK_ERROR_",  // Vulkan 초기화 실패 전반
    "DRI3",       // X 서버가 DRI3 를 못 주는 경우(가속 경로 상실)
    "libEGL",     // EGL 경고 — 위 둘과 함께 나오는 것이 보통
    "tu_knl",     // mesa/turnip 커널 인터페이스 오류
    "failed to open device",
];

/// 디스플레이 환경변수 부재 또는 연결 오류의 로그 표지다.
const NO_DISPLAY_MARKERS: &[&str] = &[
    "neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set",
    "cannot open display",
];

fn detect_blocker(stderr_tail: &str) -> Option<BootBlocker> {
    if let Some(m) = NO_DISPLAY_MARKERS
        .iter()
        .find(|m| stderr_tail.contains(**m))
    {
        return Some(BootBlocker::NoDisplay(m));
    }
    GPU_FALLBACK_MARKERS
        .iter()
        .find(|m| stderr_tail.contains(**m))
        .map(|m| BootBlocker::GpuFallback(m))
}

pub fn boot_blocker_verdict(stderr_tail: &str) -> Option<String> {
    match detect_blocker(stderr_tail)? {
        BootBlocker::NoDisplay(marker) => Some(format!(
            "디스플레이 설정 또는 연결 오류가 있다(시그니처: {marker}). DISPLAY·Wayland 설정과 접근 가능 여부를 확인하고 격리된 xvfb-run -a 등의 디스플레이에서 실행한다."
        )),
        BootBlocker::GpuFallback(marker) => Some(format!(
            "GPU 관련 로그가 있다(시그니처: {marker}). 정상 부팅에서도 나올 수 있어 이것만으로는 원인 판정이 되지 않는다. 다른 워크트리의 실행 상황과 부팅·플러그인·셸 로그를 함께 확인한다."
        )),
    }
}

fn verdict_or_default(tail: &str, fallback: &str) -> String {
    if let Some(verdict) = boot_blocker_verdict(tail) {
        return verdict;
    }
    if tail.trim().is_empty() {
        return "수집된 stderr가 비어 있다. 자식이 로그를 내지 않았는지 배출이 늦는지는 이 꼬리만으로 구별할 수 없다.".to_string();
    }
    fallback.to_string()
}

pub fn early_exit_message(status: &str, tail_lines: usize, tail: &str) -> String {
    let verdict = verdict_or_default(tail, "stderr 의 마지막 오류를 그대로 읽는다.");
    format!(
        "tasty 프로세스가 부팅 중 종료했다 ({status}) — 상한을 기다리지 않고 즉시 실패시킨다.\n{verdict}\n--- stderr (last {tail_lines} lines) ---\n{tail}"
    )
}

/// 마지막 stderr 수집 시각을 상한의 절반과 비교해 진단한다. 부팅 진행·정지 여부를 직접 관측하는 값은 아니다.
pub fn stderr_silence_verdict(last_line_age: Option<Duration>, limit: Duration) -> String {
    match last_line_age {
        None => {
            "stderr가 한 줄도 수집되지 않았다. 로그 설정과 배출 스레드·자식 상태를 함께 확인한다."
                .to_string()
        }
        Some(age) if age * 2 >= limit => format!(
            "마지막 stderr 이후 {age:?}가 지난 긴 무출력 구간이다. 이 시간만으로 부팅 정지를 증명하지 않는다. 자식 상태와 로그 수집을 확인한다."
        ),
        Some(age) => format!(
            "최근 stderr가 {age:?} 전에 수집됐다. 최근 로그만으로 부팅 진행이나 시간 예산의 적절함을 확정하지 않는다."
        ),
    }
}

pub fn spawn_timeout_message(
    stage: &str,
    limit: Duration,
    tail_lines: usize,
    tail: &str,
    last_line_age: Option<Duration>,
) -> String {
    let verdict = verdict_or_default(
        tail,
        "수집된 꼬리에서 등록된 부팅 시그니처를 찾지 못했다. 부팅 지연이나 설정 경로를 본다.",
    );
    let silence = stderr_silence_verdict(last_line_age, limit);
    format!(
        "{stage} within {limit:?}.\n{verdict}\n{silence}\n\
         --- stderr (last {tail_lines} lines) ---\n{tail}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_target_dir_tells_an_opted_in_suite_what_to_build() {
        let probe = Scratch::new("bundle-empty");
        let dir = probe.path();
        let note = staged_bundle_note(dir, true).expect("0 개면 진단이 나와야 한다");
        assert!(
            note.contains("cargo build --workspace"),
            "무엇을 지을지 말해야 한다: {note}"
        );
        assert!(
            note.contains(&dir.display().to_string()),
            "어디를 봤는지 말해야 한다: {note}"
        );
    }

    /// 플러그인이 아닌 파일은 있어도 플러그인 준비로 인정하지 않아야 한다.
    #[test]
    fn a_staged_plugin_binary_silences_the_note_but_a_lookalike_does_not() {
        let probe = Scratch::new("bundle-staged");
        let dir = probe.path();

        std::fs::write(dir.join("tasty-not-a-plugin"), b"x")
            .expect("가짜 파일을 쓸 수 있어야 한다");
        assert!(
            staged_bundle_note(dir, true).is_some(),
            "plugin 이 아닌 이름은 스테이징으로 세면 안 된다"
        );

        std::fs::write(dir.join("tasty-plugin-markdown"), b"x")
            .expect("가짜 바이너리를 쓸 수 있어야 한다");
        assert_eq!(
            staged_bundle_note(dir, true),
            None,
            "하나라도 지어져 있으면 조용해야 한다"
        );
    }

    #[test]
    fn a_suite_outside_the_roster_gets_no_build_prescription() {
        let probe = Scratch::new("bundle-optout");
        let dir = probe.path();
        assert_eq!(staged_bundle_note(dir, false), None);
    }

    #[test]
    fn the_default_binary_is_the_one_cargo_built_for_this_test() {
        let picked = resolve_instance_bin(None, "/built/by/cargo");
        assert_eq!(picked, std::ffi::OsString::from("/built/by/cargo"));
    }

    #[test]
    fn an_override_path_replaces_the_cargo_built_binary() {
        let picked = resolve_instance_bin(
            Some(std::ffi::OsStr::new("/prebuilt/headless/tasty")),
            "/built/by/cargo",
        );
        assert_eq!(picked, std::ffi::OsString::from("/prebuilt/headless/tasty"));
    }

    #[test]
    fn only_the_combo_dependent_suites_ignore_the_override() {
        let given = || Some(std::ffi::OsString::from("/some/headless/tasty"));

        assert_eq!(
            effective_override(DaemonKind::SameCombo, given()),
            None,
            "자기 조합의 데몬이 필요한 스위트는 override 를 받으면 안 된다"
        );
        assert_eq!(
            effective_override(DaemonKind::HeadlessOk, given()),
            given(),
            "IPC 만 쓰는 스위트는 override 를 그대로 받아야 한다 — 안 받으면 이 설계가 \
             아무것도 안 하는 것과 같다"
        );
    }

    /// mtime 비교는 현재 시계 대신 알려진 시각을 지정해 파일시스템 해상도에 의존하지 않게 검사한다.
    fn known_stamp() -> std::time::SystemTime {
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000)
    }

    fn stamp(path: &std::path::Path, t: std::time::SystemTime) {
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("탐침 파일을 열 수 있어야 한다");
        f.set_modified(t).expect("mtime 을 찍을 수 있어야 한다");
    }

    /// 시험마다 별도 Scratch를 사용해 다른 시험의 파일을 삭제하지 않게 한다.
    use tasty_doc_guards::temp_scratch::Scratch;

    /// 문서를 확실히 더 새롭게 만들어 .rs 확장자 제외가 실제로 적용되는지 확인한다.
    #[test]
    fn a_stale_override_binary_is_detected_and_a_fresh_one_is_not() {
        let probe = Scratch::new("stale");
        let dir = probe.path();
        let src = dir.join("src");
        std::fs::create_dir_all(&src).expect("탐침 디렉토리를 만들 수 있어야 한다");
        let bin = dir.join("tasty");
        std::fs::write(&bin, b"bin").expect("가짜 바이너리를 쓸 수 있어야 한다");
        stamp(&bin, known_stamp());
        let newer = known_stamp() + std::time::Duration::from_secs(1);

        assert_eq!(
            source_newer_than(&bin, vec![src.clone()]),
            None,
            "소스가 하나도 없으면 낡지 않았다"
        );

        let notes = src.join("notes.md");
        std::fs::write(&notes, b"x").expect("문서를 쓸 수 있어야 한다");
        stamp(&notes, newer);
        assert_eq!(
            source_newer_than(&bin, vec![src.clone()]),
            None,
            "이 mtime 검사는 .rs 파일만 대상으로 한다"
        );

        let app = src.join("app.rs");
        std::fs::write(&app, b"fn main() {}").expect("소스를 쓸 수 있어야 한다");
        stamp(&app, newer);
        assert_eq!(
            source_newer_than(&bin, vec![src.clone()]).as_deref(),
            Some(app.as_path()),
            "바이너리보다 새로운 `.rs` 가 있으면 그 경로를 대야 한다"
        );
    }

    /// 동일한 mtime도 비교 대상으로 삼는지 알려진 시각으로 확인한다.
    #[test]
    fn a_source_stamped_to_the_same_tick_is_still_seen() {
        let probe = Scratch::new("tick");
        let dir = probe.path();
        let src = dir.join("src");
        std::fs::create_dir_all(&src).expect("탐침 디렉토리를 만들 수 있어야 한다");
        let bin = dir.join("tasty");
        std::fs::write(&bin, b"bin").expect("가짜 바이너리를 쓸 수 있어야 한다");
        let app = src.join("app.rs");
        std::fs::write(&app, b"fn main() {}").expect("소스를 쓸 수 있어야 한다");

        stamp(&bin, known_stamp());
        stamp(&app, known_stamp());

        assert_eq!(
            source_newer_than(&bin, vec![src.clone()]).as_deref(),
            Some(app.as_path()),
            "같은 눈금에 떨어진 소스를 못 보면 낡은 바이너리가 조용히 통과한다"
        );

        stamp(&bin, known_stamp() + std::time::Duration::from_secs(1));
        assert_eq!(
            source_newer_than(&bin, vec![src.clone()]),
            None,
            "바이너리가 더 새것이면 낡지 않았다"
        );
    }

    #[test]
    fn an_empty_override_means_the_default_not_an_empty_path() {
        let picked = resolve_instance_bin(Some(std::ffi::OsStr::new("")), "/built/by/cargo");
        assert_eq!(picked, std::ffi::OsString::from("/built/by/cargo"));
    }

    /// 2026-09-04 포트 파일 작성까지 성공한 부팅에서 수집한 GPU 로그다.
    const SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL: &str = "\
libEGL warning: DRI3 error: Could not get DRI3 device
libEGL warning: Ensure your X server supports DRI3 to get accelerated rendering
TU: error: ../src/freedreno/vulkan/tu_knl.cc:387: failed to open device /dev/dri/renderD128 (VK_ERROR_INCOMPATIBLE_DRIVER)";

    #[test]
    fn a_gpu_fallback_tail_does_not_rule_out_code() {
        let verdict = boot_blocker_verdict(SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL)
            .expect("단서는 실어야 한다 — 다만 단정하지 않는다");
        assert!(
            !verdict.contains("디스플레이 설정 또는 연결 오류"),
            "GPU 로그만으로 디스플레이 오류를 안내하면 안 된다: {verdict}"
        );
        assert!(
            verdict.contains("원인 판정이 되지 않는다"),
            "단정하지 않는다는 사실을 문장에 실어야 한다: {verdict}"
        );
        assert!(
            verdict.contains("다른 워크트리"),
            "함께 확인할 실행 상황을 안내해야 한다: {verdict}"
        );
    }

    /// 디스플레이 오류와 GPU 로그 단서를 구별해 안내한다.
    #[test]
    fn the_two_verdicts_do_not_carry_the_same_certainty() {
        let display = boot_blocker_verdict(
            "Error: neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set.",
        )
        .expect("디스플레이 부재는 판정 대상이다");
        let gpu = boot_blocker_verdict(SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL).expect("단서는 실린다");
        assert!(
            display.contains("디스플레이 설정 또는 연결 오류"),
            "{display}"
        );
        assert!(!gpu.contains("디스플레이 설정 또는 연결 오류"), "{gpu}");
    }

    #[test]
    fn missing_display_is_reported_as_display_not_gpu() {
        let tail = "Error: os error at winit/src/platform_impl/linux/mod.rs:765: \
                    neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set.";
        let verdict = boot_blocker_verdict(tail).expect("디스플레이 부재도 판정 대상이다");
        assert!(
            verdict.contains("디스플레이 설정 또는 연결 오류"),
            "{verdict}"
        );
        assert!(
            verdict.contains("xvfb-run"),
            "다음 사람이 바로 조치할 수 있게 방법을 실어야 한다: {verdict}"
        );
    }

    #[test]
    fn an_ordinary_slow_boot_gets_no_false_verdict() {
        let tail = "INFO tasty: plugin discovery finished\nINFO tasty: theme loaded";
        assert!(boot_blocker_verdict(tail).is_none());
        let msg = spawn_timeout_message(
            "tasty failed to start",
            SPAWN_PORT_TIMEOUT,
            30,
            tail,
            Some(Duration::from_millis(200)),
        );
        assert!(msg.contains("등록된 부팅 시그니처를 찾지 못했다"), "{msg}");
    }

    #[test]
    fn an_empty_tail_is_not_reported_as_an_absent_signature() {
        let empty =
            spawn_timeout_message("tasty failed to start", SPAWN_PORT_TIMEOUT, 30, "", None);
        assert!(
            !empty.contains("등록된 부팅 시그니처를 찾지 못했다"),
            "빈 꼬리와 내용이 있지만 표지가 없는 꼬리를 구별해야 한다: {empty}"
        );
        assert!(empty.contains("수집된 stderr가 비어 있다"), "{empty}");
        assert!(empty.contains("한 줄도 수집되지 않았다"), "{empty}");

        let noisy = spawn_timeout_message(
            "tasty failed to start",
            SPAWN_PORT_TIMEOUT,
            30,
            "INFO tasty: plugin discovery finished",
            Some(Duration::from_millis(200)),
        );
        assert!(
            noisy.contains("등록된 부팅 시그니처를 찾지 못했다"),
            "내용이 있는 꼬리에서는 등록된 표지를 찾지 못했다는 안내가 필요하다: {noisy}"
        );
        assert!(!noisy.contains("수집된 stderr가 비어 있다"), "{noisy}");
    }

    /// 두 표지가 함께 있으면 디스플레이 오류를 먼저 안내하는 순서를 확인한다.
    #[test]
    fn a_display_failure_that_also_logs_gpu_noise_is_still_a_display_failure() {
        let both = format!(
            "{SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL}\nError: neither WAYLAND_DISPLAY nor \
             WAYLAND_SOCKET nor DISPLAY is set."
        );
        let verdict = boot_blocker_verdict(&both).expect("둘 다 있으면 판정 대상이다");
        assert!(
            verdict.contains("디스플레이 설정 또는 연결 오류"),
            "GPU 로그가 섞여도 디스플레이 오류를 먼저 안내해야 한다: {verdict}"
        );
        assert!(
            !verdict.contains("원인 판정이 되지 않는다"),
            "디스플레이 오류 대신 GPU 진단이 선택됐다: {verdict}"
        );

        let gpu_only =
            boot_blocker_verdict(SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL).expect("단서는 실린다");
        assert!(
            !gpu_only.contains("디스플레이 설정 또는 연결 오류"),
            "{gpu_only}"
        );
    }

    /// 종료한 자식의 진단과 살아 있지만 시간 제한에 걸린 자식의 진단을 구별한다.
    #[test]
    fn a_dead_child_and_a_stalled_one_do_not_get_the_same_prescription() {
        let unremarkable = "INFO tasty: plugin discovery finished";

        let died = early_exit_message("exit status: 1", 30, unremarkable);
        assert!(died.contains("부팅 중 종료했다"), "{died}");
        assert!(died.contains("상한을 기다리지 않고"), "{died}");
        assert!(
            died.contains("마지막 오류를 그대로 읽는다"),
            "종료한 자식에게는 남은 stderr 을 읽으라고 해야 한다: {died}"
        );
        assert!(
            !died.contains("부팅 지연"),
            "이미 끝난 프로세스에게 지연을 보라고 한다 — 기다릴 부팅이 없다: {died}"
        );

        let stalled = spawn_timeout_message(
            "tasty failed to start",
            SPAWN_PORT_TIMEOUT,
            30,
            unremarkable,
            Some(Duration::from_millis(200)),
        );
        assert!(
            stalled.contains("부팅 지연이나 설정 경로를 본다"),
            "{stalled}"
        );
        assert!(
            !stalled.contains("마지막 오류를 그대로 읽는다"),
            "아직 종료하지 않은 자식에게 종료 진단을 안내했다: {stalled}"
        );

        let died_silent = early_exit_message("signal: 9", 30, "   \n  ");
        assert!(
            died_silent.contains("수집된 stderr가 비어 있다"),
            "{died_silent}"
        );
        assert!(
            !died_silent.contains("마지막 오류를 그대로 읽는다"),
            "읽을 stderr 이 없는데 읽으라고 한다: {died_silent}"
        );

        let died_with_signature = early_exit_message(
            "exit status: 1",
            30,
            "Error: neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set.",
        );
        assert!(
            died_with_signature.contains("디스플레이 설정 또는 연결 오류"),
            "{died_with_signature}"
        );
        assert!(
            !died_with_signature.contains("마지막 오류를 그대로 읽는다"),
            "판정이 있는데 폴백이 덮었다: {died_with_signature}"
        );
    }

    #[test]
    fn the_silence_verdict_separates_stalled_from_merely_slow() {
        let limit = Duration::from_secs(40);

        let none = stderr_silence_verdict(None, limit);
        assert!(none.contains("한 줄도 수집되지 않았다"), "{none}");

        let stalled = stderr_silence_verdict(Some(Duration::from_secs(30)), limit);
        assert!(stalled.contains("긴 무출력 구간"), "{stalled}");
        assert!(stalled.contains("정지를 증명하지 않는다"), "{stalled}");

        let slow = stderr_silence_verdict(Some(Duration::from_millis(200)), limit);
        assert!(slow.contains("최근 stderr"), "{slow}");
        assert!(
            !slow.contains("긴 무출력 구간") && !slow.contains("한 줄도"),
            "무출력 시간에 따른 진단이 구별되지 않는다: {slow}"
        );

        // 상한 절반의 경계 양쪽을 확인한다.
        assert!(stderr_silence_verdict(Some(limit / 2), limit).contains("긴 무출력 구간"));
        assert!(
            stderr_silence_verdict(Some(limit / 2 - Duration::from_millis(1)), limit)
                .contains("최근 stderr")
        );
    }

    #[test]
    fn both_harnesses_share_one_bound_for_the_same_stage() {
        assert!(SPAWN_PORT_TIMEOUT > SPAWN_SHELL_TIMEOUT);
    }

    #[cfg(unix)]
    fn long_lived_child() -> std::process::Child {
        std::process::Command::new("sleep")
            .arg("60")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("오래 사는 자식을 못 띄웠다")
    }

    #[cfg(unix)]
    fn pid_exists(pid: u32) -> bool {
        // SAFETY: 시그널 0 은 아무것도 보내지 않고 존재·권한만 묻는다. 인자는 정수뿐이다.
        let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }

    /// 초기화 중 panic했을 때 종료 요청과 회수가 이뤄지는지 확인한다.
    #[cfg(unix)]
    #[test]
    fn a_panic_before_the_handle_exists_kills_and_reaps_the_child() {
        let child = long_lived_child();
        let pid = child.id();
        let started = std::time::Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _reaper = ChildReaper::new(child);
            panic!("부팅 대기 중 실패를 흉내 낸다");
        }));
        assert!(
            result.is_err(),
            "panic이 발생하지 않아 실패 중 회수를 확인할 수 없다"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "자식 회수가 {:?} 동안 완료되지 않았다. 종료 요청과 대기 상태를 확인한다.",
            started.elapsed()
        );
        assert!(
            !pid_exists(pid),
            "인스턴스로 넘기기 전 panic이 발생했지만 자식 PID {pid}가 남아 있다"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_released_child_is_handed_over_alive() {
        let mut reaper = ChildReaper::new(long_lived_child());
        assert!(
            reaper.child().try_wait().expect("try_wait").is_none(),
            "핸들 이전 전에 자식이 종료돼 수명 이전을 확인할 수 없다"
        );
        let mut child = reaper.release();
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "release 가 자식을 죽였다 — 핸들이 넘겨받은 인스턴스가 즉사한다"
        );
        kill_process_tree(&mut child);
        child.wait().expect("대조 자식을 못 거뒀다");
    }

    fn child_that_prints_stderr_lines(n: usize) -> std::process::Child {
        #[cfg(windows)]
        let mut cmd = {
            let mut c = std::process::Command::new("cmd");
            c.arg("/C")
                .arg(format!("for /L %i in (1,1,{n}) do @echo line %i 1>&2"));
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = std::process::Command::new("sh");
            c.arg("-c").arg(format!(
                "i=1; while [ $i -le {n} ]; do echo \"line $i\" 1>&2; i=$((i+1)); done"
            ));
            c
        };
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("stderr 를 뱉는 자식을 못 띄웠다")
    }

    /// 종료한 자식의 꼬리 내용과 줄 수를 확인한다. 배출 스레드가 늦어지는 경쟁 상태를 강제로 만들지는 못한다.
    #[test]
    fn a_tail_read_after_the_child_died_waits_for_the_drain_instead_of_reporting_empty() {
        let emitted = 45;
        let mut child = child_that_prints_stderr_lines(emitted);
        let mut cap = StderrCapture::start(child.stderr.take(), STDERR_TAIL_LINES);
        child.wait().expect("자식을 못 거뒀다");

        let tail = cap.tail_after_exit(std::time::Duration::from_secs(30));
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(
            lines.len(),
            STDERR_TAIL_LINES,
            "종료 뒤 수집한 꼬리의 줄 수가 예상과 다르다. 수집 상한 초과 안내도 확인한다:\n{tail}"
        );
        assert_eq!(
            lines.last().copied(),
            Some(format!("line {emitted}").as_str()),
            "종료 뒤 수집한 꼬리에 마지막 줄이 없다:\n{tail}"
        );
    }

    #[test]
    fn the_capture_rings_at_capacity_and_shows_only_the_tail_it_promises() {
        let emitted = STDERR_RING_CAPACITY + 44;
        let mut child = child_that_prints_stderr_lines(emitted);
        let mut cap = StderrCapture::start(child.stderr.take(), 7);
        child.wait().expect("자식을 못 거뒀다");
        cap.join();

        let tail = cap.tail();
        let lines: Vec<&str> = tail.lines().collect();

        assert_eq!(
            lines.len(),
            cap.tail_lines(),
            "꼬리 줄 수가 약속과 다르다: {tail}"
        );

        assert_eq!(
            lines.last().copied(),
            Some(format!("line {emitted}").as_str())
        );
        assert!(
            cap.find(|l| l == "line 1").is_none(),
            "용량을 {STDERR_RING_CAPACITY} 넘겨 {emitted} 줄을 넣었는데 첫 줄이 남아 있다"
        );
        assert!(cap.find(|l| l == format!("line {emitted}")).is_some());

        assert!(cap.last_line_age().is_some());
    }

    #[test]
    fn a_capture_without_a_pipe_stays_empty_instead_of_dying() {
        let mut cap = StderrCapture::start(None, 9);
        assert_eq!(cap.tail(), "");
        assert_eq!(cap.last_line_age(), None);
        assert!(cap.find(|_| true).is_none());
        cap.join(); // 거둘 스레드가 없어도 막히지 않는다
    }

    /// StderrCapture는 poison을 복구하므로 별도 뮤텍스로 panic 인자 안팎의 가드 수명 차이를 확인한다.
    #[test]
    fn a_guard_born_inside_a_panic_argument_poisons_and_one_dropped_before_it_does_not() {
        let inside = std::sync::Mutex::new(7u32);
        let hit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("나이 {:?}", *inside.lock().expect("첫 lock 은 성해야 한다"));
        }));
        assert!(
            hit.is_err(),
            "panic이 발생하지 않아 가드 해제 순서를 확인할 수 없다"
        );
        assert!(
            inside.lock().is_err(),
            "panic 인자에 남은 락 가드가 poison 상태를 만들지 않았다"
        );

        let outside = std::sync::Mutex::new(7u32);
        let hit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let v = *outside.lock().expect("첫 lock 은 성해야 한다");
            panic!("나이 {v:?}");
        }));
        assert!(hit.is_err());
        assert!(
            outside.lock().is_ok(),
            "락 가드를 먼저 해제했지만 poison 상태가 됐다"
        );
    }

    #[test]
    fn the_latch_blocks_the_second_spawn_and_a_success_releases_it() {
        let latch = SpawnOnceLatch::new();

        latch.entering("시험용 하네스");
        let blocked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            latch.entering("시험용 하네스");
        }));
        assert!(blocked.is_err(), "래치가 두 번째 진입을 안 막았다");

        let opened = SpawnOnceLatch::new();
        opened.entering("시험용 하네스");
        opened.succeeded();
        opened.entering("시험용 하네스"); // 안 죽어야 한다
    }

    #[test]
    fn a_daemon_without_a_window_is_asked_for_no_display() {
        assert_eq!(
            display_policy(false, None),
            DisplayPolicy::NotRequired,
            "창을 안 만드는 조합에 디스플레이를 요구했다"
        );
        assert_eq!(
            display_policy(false, Some(":77")),
            DisplayPolicy::NotRequired
        );
    }

    #[test]
    fn declaring_the_inherit_keeps_todays_behavior() {
        assert_eq!(
            display_policy(true, Some(DISPLAY_INHERIT)),
            DisplayPolicy::Inherit
        );
    }

    #[test]
    fn a_named_display_is_handed_to_the_child_verbatim() {
        assert_eq!(
            display_policy(true, Some(":77")),
            DisplayPolicy::Pin(":77".to_string())
        );
        assert_eq!(
            display_policy(true, Some(" :77 ")),
            DisplayPolicy::Pin(":77".to_string())
        );
    }

    #[test]
    fn an_empty_declaration_is_not_a_declaration() {
        assert_eq!(display_policy(true, None), DisplayPolicy::Unspecified);
        assert_eq!(display_policy(true, Some("")), DisplayPolicy::Unspecified);
        assert_eq!(
            display_policy(true, Some("   ")),
            DisplayPolicy::Unspecified
        );
    }

    #[test]
    fn the_refusal_prints_both_ways_out() {
        let msg = unspecified_display_message();
        assert!(
            msg.contains("Xvfb"),
            "전용 디스플레이를 만드는 길이 없다: {msg}"
        );
        assert!(
            msg.contains(&format!("{DISPLAY_ENV}={DISPLAY_INHERIT}")),
            "자기 화면을 선언하는 길이 없다: {msg}"
        );
        assert!(
            msg.contains("docs/dev-guide/e2e-tests.md"),
            "절차 문서를 안 가리킨다: {msg}"
        );
    }

    fn write(p: &std::path::Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// 내용이 같아도 복사일 수 있으므로 inode로 hardlink를 확인한다.
    #[cfg(unix)]
    #[test]
    fn prefilled_home_shares_inodes_with_the_snapshot() {
        use std::os::unix::fs::MetadataExt;
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("bundle");
        write(
            &bundle.join("markdown/tasty-plugin.toml"),
            "version = \"0.1.0\"",
        );
        write(&bundle.join("markdown/tasty-plugin-markdown"), "bin");
        write(&bundle.join("markdown/lang/en.toml"), "k = 'v'");
        let cache = root.path().join("cache");
        let snapshot = cache.join("key");
        copy_snapshot(&bundle, &cache, &snapshot).unwrap();
        let home = root.path().join("home/plugins");
        link_tree(&snapshot, &home).unwrap();
        for rel in [
            "markdown/tasty-plugin.toml",
            "markdown/tasty-plugin-markdown",
            "markdown/lang/en.toml",
        ] {
            let a = std::fs::metadata(snapshot.join(rel)).unwrap();
            let b = std::fs::metadata(home.join(rel)).unwrap();
            assert_eq!(
                (a.dev(), a.ino()),
                (b.dev(), b.ino()),
                "{rel} 가 hardlink 가 아니다"
            );
            assert_eq!(
                std::fs::read(bundle.join(rel)).unwrap(),
                std::fs::read(home.join(rel)).unwrap()
            );
        }
        // 빌드 번들의 제자리 쓰기가 시험 홈에 전파되지 않도록 스냅숏은 원본과 inode가 달라야 한다.
        let src = std::fs::metadata(bundle.join("markdown/tasty-plugin-markdown")).unwrap();
        let snap = std::fs::metadata(snapshot.join("markdown/tasty-plugin-markdown")).unwrap();
        assert_ne!(src.ino(), snap.ino(), "스냅숏이 번들과 inode 를 공유한다");
        let names: Vec<_> = std::fs::read_dir(&cache)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("key")]);
    }

    #[test]
    fn bundle_signature_follows_the_bundle() {
        let root = tempfile::tempdir().unwrap();
        write(&root.path().join("a/bin"), "one");
        let before = bundle_signature(root.path()).unwrap();
        assert_eq!(
            before,
            bundle_signature(root.path()).unwrap(),
            "같은 번들이면 같다"
        );
        write(&root.path().join("a/bin"), "one-longer");
        assert_ne!(
            before,
            bundle_signature(root.path()).unwrap(),
            "크기가 바뀌었다"
        );
        let resized = bundle_signature(root.path()).unwrap();
        write(&root.path().join("a/lang/en.toml"), "");
        assert_ne!(
            resized,
            bundle_signature(root.path()).unwrap(),
            "파일이 늘었다"
        );
    }

    #[test]
    fn prune_keeps_the_current_snapshot_and_young_builds() {
        let root = tempfile::tempdir().unwrap();
        for d in ["keep", "old", ".building-1-ff"] {
            std::fs::create_dir_all(root.path().join(d)).unwrap();
        }
        prune_old_snapshots(root.path(), "keep");
        let mut names: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![".building-1-ff".to_string(), "keep".to_string()]
        );
    }
}

/// 번들 플러그인 호출이나 플러그인 서피스가 필요한 스위트만 등록한다.
/// 미등록 스위트는 빈 번들 루트를 사용해 불필요한 복사를 피한다.
pub const SUITES_THAT_CALL_BUNDLED_PLUGINS: &[&str] = &[
    "attach_markdown_content_loopback",
    "attach_html_transfer_loopback",
    "e2e_tests",
    "plugin_disable_withdraws_surface_kinds",
    "soak_memory",
    "agent_session_gui",
];

/// 공유 인스턴스가 한 번 초기화되므로 번들 사용 여부는 시험 함수가 아니라 바이너리별로 정한다.
pub fn current_suite_name() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_str()?;
    match stem.rsplit_once('-') {
        Some((name, hash))
            if !name.is_empty()
                && !hash.is_empty()
                && hash.bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            Some(name.to_string())
        }
        _ => Some(stem.to_string()),
    }
}

const PLUGIN_BIN_PREFIX: &str = "tasty-plugin-";

fn suite_calls_bundled_plugins() -> bool {
    current_suite_name().is_some_and(|s| SUITES_THAT_CALL_BUNDLED_PLUGINS.contains(&s.as_str()))
}

/// exe 옆에서 플러그인 접두사의 일반 파일을 하나도 찾지 못하면 빌드 방법을 안내한다.
/// 실행 권한이나 부분 빌드·필요 플러그인의 완전성까지 검사하지는 않는다.
fn staged_bundle_note(exe_dir: &std::path::Path, opted_in: bool) -> Option<String> {
    if !opted_in {
        return None;
    }
    let staged = std::fs::read_dir(exe_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    // Cargo 의존 목록인 .d 파일은 바이너리 후보에서 제외한다.
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.starts_with(PLUGIN_BIN_PREFIX) && !n.ends_with(".d"))
                        && e.file_type().is_ok_and(|k| k.is_file())
                })
                .count()
        })
        .unwrap_or(0);
    if staged > 0 {
        return None;
    }
    Some(format!(
        "\n이 스위트는 번들 플러그인이 필요한데 {}에서 플러그인 바이너리 이름의 일반 파일을 찾지 못했다. 경로와 빌드 결과를 확인한다. cargo test --test <스위트>만으로 플러그인이 준비되지는 않는다.\n  cargo build --workspace\n",
        exe_dir.display()
    ))
}

/// 번들 누락 진단은 해당 시험이 실패했을 때 붙여 플러그인을 쓰지 않는 시험까지 막지 않는다.
pub fn bundle_staging_note() -> String {
    std::path::PathBuf::from(instance_bin())
        .parent()
        .and_then(|dir| staged_bundle_note(dir, suite_calls_bundled_plugins()))
        .unwrap_or_default()
}

pub const OS_OPEN_LOG_FILE: &str = "os-open.log";

/// debug 자식의 호스트 OS 열기를 기록으로 대체한다. release는 이 스위치를 사용하지 않는다.
/// 격리 홈·디스플레이만으로 기존 브라우저에 전달되는 요청을 막을 수는 없다. 자식 프로세스의 열기는 별도 BROWSER 설정을 사용한다.
pub fn apply_os_open_record(
    command: &mut std::process::Command,
    home: &std::path::Path,
) -> std::path::PathBuf {
    let log = home.join(OS_OPEN_LOG_FILE);
    #[cfg(debug_assertions)]
    command.env(tasty_platform::debug_os_open::ENV, &log);
    apply_fake_browser(command, home);
    log
}

pub const FAKE_BROWSER_FILE: &str = "os-open-browser.sh";

/// BROWSER를 읽는 Linux·BSD의 webbrowser 경로를 기록용 스크립트로 대체한다.
/// macOS·Windows와 BROWSER를 무시하는 프로그램의 열기까지 막지는 못한다.
/// 경로에 BROWSER 구분자가 있으면 true로 대체해 기록 없이 성공시킨다. 빈 값은 사용하지 않는다.
pub fn apply_fake_browser(command: &mut std::process::Command, home: &std::path::Path) {
    #[cfg(unix)]
    {
        let script = write_fake_browser(home).expect("write fake BROWSER script");
        let value = match script.to_str() {
            Some(s) if !s.contains(|c: char| c == ':' || c.is_whitespace()) => s,
            _ => "true",
        };
        command.env("BROWSER", value);
    }
    #[cfg(not(unix))]
    {
        let _ = (command, home);
    }
}

#[cfg(unix)]
fn write_fake_browser(home: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(home)?;
    let script = home.join(FAKE_BROWSER_FILE);
    // 기록 경로를 스크립트 위치에서 구해 본문에 절대 경로를 삽입하지 않는다.
    let body = format!(
        "#!/bin/sh\nprintf 'BROWSER\\t%s\\n' \"$*\" >> \"$(dirname \"$0\")/{OS_OPEN_LOG_FILE}\"\n"
    );
    tasty_test_support::write_executable(&script, body)?;
    Ok(script)
}

/// 번들이 필요 없는 스위트에는 빈 루트를 지정한다. 필요한 스위트는 스냅숏을 hardlink로 미리 채운다.
/// 제품의 설치·업그레이드 판정은 그대로 적용된다.
pub fn apply_bundle_opt_in(command: &mut std::process::Command, tasty_home: &std::path::Path) {
    // 실패 이유를 출력할 수 있도록 번들 준비 전에 subscriber를 설치한다.
    init_test_tracing();
    if suite_calls_bundled_plugins() {
        prefill_bundle_links(tasty_home);
        return;
    }
    // 이유: 내용이 없는 공용 디렉터리로 사용한다. 이 하네스는 파일을 쓰거나 지우지 않는다.
    // 디렉터리 생성이 실패하면 환경변수를 설정하지 않아 제품의 원래 번들 탐색을 따른다.
    let empty = std::env::temp_dir().join("tasty-test-empty-plugin-bundle");
    if std::fs::create_dir_all(&empty).is_ok() && empty.is_dir() {
        command.env("TASTY_BUILTIN_PLUGINS_DIR", &empty);
    }
}

/// 자식 바이너리 옆에 두어 빌드 결과와 같은 범위에서 정리할 수 있게 한다.
pub const BUNDLE_LINK_CACHE_DIR: &str = "test-bundle-links";

/// 스냅숏을 hardlink로 미리 채워 반복 복사를 줄인다. 실패한 파일은 부팅 시 제품의 내용 대조·복사 경로에 맡긴다.
/// 원본 번들에 직접 hardlink하면 빌드의 제자리 쓰기가 시험 중 파일을 바꿀 수 있어 별도 스냅숏을 사용한다.
fn prefill_bundle_links(tasty_home: &std::path::Path) {
    let Some(snapshot) = bundle_link_snapshot() else {
        return;
    };
    if let Err(e) = link_tree(snapshot, &tasty_home.join("plugins")) {
        tracing::warn!(
            "번들 hardlink 미리 채우기가 중간에 멈췄다({e}) — 못 넣은 파일은 host 가 복사로 채운다"
        );
    }
}

fn bundle_link_snapshot() -> Option<&'static std::path::Path> {
    static SNAPSHOT: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    SNAPSHOT
        .get_or_init(|| match build_bundle_link_snapshot() {
            Ok(p) => Some(p),
            Err(why) => {
                tracing::warn!("번들 hardlink 를 안 쓴다 — host 가 복사로 채운다: {why}");
                None
            }
        })
        .as_deref()
}

/// 번들의 경로·크기·mtime 서명으로 이름 붙인 스냅숏을 재사용한다.
fn build_bundle_link_snapshot() -> Result<std::path::PathBuf, String> {
    // override는 프로필이 다를 수 있어 테스트 프로필로 대신 번들을 준비하지 않는다.
    if effective_override(daemon_kind(), std::env::var_os(INSTANCE_BIN_ENV))
        .is_some_and(|v| !v.is_empty())
    {
        return Err(format!(
            "{INSTANCE_BIN_ENV} override 중이라 자식의 번들을 대신 못 정한다"
        ));
    }
    let bin = std::path::PathBuf::from(instance_bin());
    let bin_dir = bin
        .parent()
        .ok_or_else(|| format!("바이너리 경로에 부모가 없다: {}", bin.display()))?;
    let source = child_bundle_root(bin_dir)?;
    let cache_root = bin_dir.join(BUNDLE_LINK_CACHE_DIR);
    std::fs::create_dir_all(&cache_root)
        .map_err(|e| format!("{} 를 못 만든다: {e}", cache_root.display()))?;
    probe_hard_link_to_temp(&cache_root)?;

    let key = format!(
        "{:016x}",
        bundle_signature(&source).map_err(|e| format!("번들 서명 실패: {e}"))?
    );
    let snapshot = cache_root.join(&key);
    if !snapshot.is_dir() {
        copy_snapshot(&source, &cache_root, &snapshot)?;
    }
    prune_old_snapshots(&cache_root, &key);
    Ok(snapshot)
}

/// 상속한 번들 환경변수를 우선 사용하고 없으면 제품의 exe 기준 탐색 함수를 호출한다.
fn child_bundle_root(bin_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    if let Some(p) = std::env::var_os("TASTY_BUILTIN_PLUGINS_DIR").map(std::path::PathBuf::from)
        && p.is_dir()
    {
        return Ok(p);
    }
    tasty_host_plugin::builtin::bundle_root_from_exe_dir(bin_dir)
        .ok_or_else(|| format!("{} 옆에 번들이 없다", bin_dir.display()))
}

/// 큰 스냅숏을 복사하기 전에 캐시와 임시 홈 사이 hardlink 가능 여부를 확인한다.
fn probe_hard_link_to_temp(cache_root: &std::path::Path) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let probe = cache_root.join(format!(".probe-{unique}"));
    let linked = std::env::temp_dir().join(format!("tasty-test-bundle-link-probe-{unique}"));
    std::fs::write(&probe, b"probe").map_err(|e| format!("probe 쓰기 실패: {e}"))?;
    let result = std::fs::hard_link(&probe, &linked);
    // 정리 실패는 무시한다. 탐침은 PID와 카운터로 구별한다.
    let _ = std::fs::remove_file(&linked);
    let _ = std::fs::remove_file(&probe);
    result.map_err(|e| {
        format!(
            "{} 와 {} 사이에 hardlink 를 못 건다(다른 파일시스템?): {e}",
            cache_root.display(),
            linked.parent().unwrap_or(&linked).display()
        )
    })
}

/// 상대 경로·크기·mtime을 해시한다. 내용은 읽지 않아 같은 크기와 시각으로 교체한 파일은 구별하지 못한다.
/// 이 경우 제품이 부팅 중 내용 차이를 확인해 갱신한다. 번들의 심볼릭 링크는 따라간다.
fn bundle_signature(root: &std::path::Path) -> std::io::Result<u64> {
    let mut files = Vec::new();
    collect_files(root, std::path::Path::new(""), &mut files)?;
    files.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for rel in files {
        let meta = std::fs::metadata(root.join(&rel))?;
        let mtime = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        feed(rel.to_string_lossy().as_bytes());
        feed(&[0]);
        feed(&meta.len().to_le_bytes());
        feed(&mtime.to_le_bytes());
    }
    Ok(h)
}

fn collect_files(
    root: &std::path::Path,
    rel: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root.join(rel))? {
        let entry = entry?;
        let child = rel.join(entry.file_name());
        if std::fs::metadata(entry.path())?.is_dir() {
            collect_files(root, &child, out)?;
        } else {
            out.push(child);
        }
    }
    Ok(())
}

/// 옆 임시 디렉터리에 복사를 마친 뒤 rename한다. 다른 실행이 같은 이름을 먼저 만들었으면 그 디렉터리를 사용한다.
fn copy_snapshot(
    source: &std::path::Path,
    cache_root: &std::path::Path,
    snapshot: &std::path::Path,
) -> Result<(), String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let building = cache_root.join(format!(".building-{}-{nanos:x}", std::process::id()));
    let result = copy_tree(source, &building)
        .map_err(|e| format!("스냅숏 복사 실패: {e}"))
        .and_then(|()| match std::fs::rename(&building, snapshot) {
            Ok(()) => Ok(()),
            Err(_) if snapshot.is_dir() => Ok(()),
            Err(e) => Err(format!("스냅숏 rename 실패: {e}")),
        });
    if building.exists() {
        // 삭제 실패는 무시한다. 이후 캐시 정리에서 오래된 임시 디렉터리를 다시 삭제할 수 있다.
        let _ = std::fs::remove_dir_all(&building);
    }
    result
}

fn copy_tree(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if std::fs::metadata(entry.path())?.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// 다른 서명의 스냅숏은 지운다. 생성 중인 임시 디렉터리는 mtime을 읽을 수 있고 한 시간이 지났을 때만 삭제한다.
/// 동시 link가 실패하면 제품 복사로 대체하며 이미 만든 hardlink는 유지된다.
fn prune_old_snapshots(cache_root: &std::path::Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(cache_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == keep || name.starts_with(".probe-") {
            continue;
        }
        if name.starts_with(".building-") {
            let young = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_none_or(|age| age < Duration::from_secs(3600));
            if young {
                continue;
            }
        }
        if let Err(e) = std::fs::remove_dir_all(entry.path()) {
            tracing::warn!(
                "낡은 번들 스냅숏 {} 를 못 지웠다: {e}",
                entry.path().display()
            );
        }
    }
}

fn link_tree(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            link_tree(&entry.path(), &to)?;
        } else {
            std::fs::hard_link(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Linux GUI 자식이 쓸 디스플레이를 명시한다. 지정 없이 부모 화면을 상속해 시험 창이 사용자 화면에 뜨지 않게 한다.
pub const DISPLAY_ENV: &str = "TASTY_E2E_DISPLAY";

/// 부모 디스플레이를 사용자가 의도적으로 상속할 때 쓰는 값이다.
pub const DISPLAY_INHERIT: &str = "inherit";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayPolicy {
    /// 디스플레이 설정을 적용하지 않는다.
    NotRequired,
    /// 부모의 디스플레이 환경을 상속한다.
    Inherit,
    /// 지정한 디스플레이 값을 적용한다.
    Pin(String),
    /// 디스플레이가 필요한데 지정되지 않아 실행을 거절한다.
    Unspecified,
}

/// 환경을 직접 바꾸지 않고 정책을 검사한다. 빈 문자열은 미지정으로 취급한다.
fn display_policy(required: bool, declared: Option<&str>) -> DisplayPolicy {
    if !required {
        return DisplayPolicy::NotRequired;
    }
    match declared.map(str::trim) {
        None | Some("") => DisplayPolicy::Unspecified,
        Some(v) if v == DISPLAY_INHERIT => DisplayPolicy::Inherit,
        Some(v) => DisplayPolicy::Pin(v.to_string()),
    }
}

/// Linux GUI에서 헤드리스 override를 사용하지 않을 때만 디스플레이를 요구한다.
/// override 파일의 빌드 조합은 확인하지 않으므로 GUI 바이너리를 주면 이 요구를 건너뛸 수 있다.
fn display_required() -> bool {
    if !cfg!(all(target_os = "linux", feature = "gui")) {
        return false;
    }
    effective_override(daemon_kind(), std::env::var_os(INSTANCE_BIN_ENV)).is_none()
}

fn unspecified_display_message() -> String {
    format!(
        "{DISPLAY_ENV}가 지정되지 않았다. GUI 시험에 쓸 디스플레이를 명시한다.\n전용 디스플레이 예:\n  Xvfb :77 -screen 0 1920x1080x24 -nolisten tcp -ac &\n  {DISPLAY_ENV}=:77 cargo test ...\n부모 화면 사용이 의도라면 {DISPLAY_ENV}={DISPLAY_INHERIT}로 지정한다. 창이 필요 없는 시험의 헤드리스 데몬 절차는 docs/dev-guide/e2e-tests.md를 따른다."
    )
}

/// Pin은 DISPLAY를 설정하고 WAYLAND_DISPLAY를 제거한다. WAYLAND_SOCKET 등 다른 환경값까지 제거하지는 않는다.
pub fn apply_display_policy(command: &mut std::process::Command) {
    match display_policy(
        display_required(),
        std::env::var(DISPLAY_ENV).ok().as_deref(),
    ) {
        DisplayPolicy::NotRequired | DisplayPolicy::Inherit => {}
        DisplayPolicy::Pin(display) => {
            command
                .env("DISPLAY", display)
                .env_remove("WAYLAND_DISPLAY");
        }
        DisplayPolicy::Unspecified => panic!("{}", unspecified_display_message()),
    }
}
