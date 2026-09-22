//! IPC e2e 테스트 하네스 — 실 tasty 바이너리를 spawn 해 JSON-RPC 로 조작한다.
//!
//! 진입점이 둘이다.
//!
//! * [`shared()`] — **test binary 하나가 공유하는** 인스턴스. 첫 호출에서만
//!   프로세스를 띄우고 이후 호출은 같은 핸들을 돌려준다. 테스트별 격리는
//!   [`TastyInstance::create_workspace`] 로 자기 workspace 를 만들어 확보한다.
//! * [`TastyInstance::spawn`] — 전용 인스턴스. 프로세스 기동 시점 설정이
//!   달라야 하거나(`spawn_with_inherit_cwd`), 프로세스 전체를 외부에서
//!   측정해야 할 때(soak) 쓴다.
//!
//! 인스턴스 공유 원칙(binary 당 1 개 · workspace 격리)·격리 전략·timeout 정책은
//! [`docs/dev-guide/e2e-tests.md`], 그 결정 근거는 ADR-0090. 원칙 위반은
//! `tests/e2e_single_instance_guard.rs` 가 잡는다 — 그 가드는 **헤드리스 조합에서만**
//! 자동으로 돈다(`check-headless` 가 전체 스위트를 돌리고 그 잡의 `--skip` 목록에
//! 없다). 기본 조합에는 컴파일 채널뿐이다. 정본은 `docs/dev-guide/ci-gates.md`.

// 시험 하네스라 unsafe 는 시험을 세우는 데만 쓴다. `cfg_attr(test, ..)` 형태를 쓰는 것은
// 이 파일이 `check-allow-reason` 의 좌변 밖(루트 `tests/`)이라 사유 주석이 어느 게이트에도
// 안 걸리기 때문이다 — 그래서 "테스트라서 뺐다" 를 **형태**가 남기게 한다. 프로덕션 자리는
// 같은 lint 라도 무조건 `#![allow]` + 사유이고, 그 둘이 형태로 갈린다.
#![cfg_attr(test, allow(clippy::multiple_unsafe_ops_per_block))]
// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다 — 전수 가드
// (`crates/tasty-doc-guards/tests/let_underscore_documented.rs`)가 테스트 본문을 제외하므로, 여기서 나는
// `let_underscore_must_use` 경고는 정책상 조치 대상이 될 수 없다. 끄지 않으면
// 프로덕션의 진짜 신호가 그 안에 묻힌다 — `docs/dev-guide/error-handling.md`.
#![allow(clippy::let_underscore_must_use)]
// 다중 test binary 가 공유하는 test-support 모듈 — binary 마다 사용하는 부분집합이
// 달라 개별 binary 기준 dead_code 판정이 무의미하다 (의도된 superset API).
#![allow(dead_code)]

// `pub` 인 이유: 소비 바이너리가 같은 모듈을 자기 이름으로 한 번 더 들이면 **두 벌이
// 컴파일되고 그 안의 시험이 두 번 돈다**(실측 2026-09-08: 그 형태로 이름이 겹쳐 나왔다).
// 여기서 한 번 들여 그대로 내주면 사본이 하나로 남는다.
#[path = "../spawn_diag/mod.rs"]
pub mod spawn_diag;

mod startup;
use startup::{StartupFailure, StartupTimeline};

/// 실패 문구 끝에 이어 붙이는 번들 스테이징 진단 — 정의와 근거는 `spawn_diag`.
///
/// 이유: 이 helper 는 raw 응답을 스스로 단언하는 스위트에서만 직접 불린다.
#[allow(dead_code)]
pub fn bundle_staging_note() -> String {
    spawn_diag::bundle_staging_note()
}

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

// S1=port file 작성 (init_app_state 후), S2=first surface PTY prompt.
// 값과 그 근거는 두 하네스가 공유한다 — `tests/spawn_diag`.
// stderr 포착(링·마지막 줄 시각·배출 스레드)도 같은 자리에서 온다 — 셋이 각자 들고 있던
// 것을 `StderrCapture` 하나로 모았다.
use spawn_diag::{SPAWN_PORT_TIMEOUT, SPAWN_SHELL_TIMEOUT, STDERR_TAIL_LINES, StderrCapture};

// ───── 공유 인스턴스 (test binary 단위) ─────

static SHARED_INSTANCE: OnceLock<TastyInstance> = OnceLock::new();
/// `shared()` 가 실제로 프로세스를 띄운 횟수 — 하네스 자체 검증용(항상 1 이어야).
static SHARED_SPAWN_COUNT: AtomicUsize = AtomicUsize::new(0);
/// 첫 spawn 이 panic 으로 끝났는지. `OnceLock::get_or_init` 은 초기화 클로저가
/// panic 하면 미초기화 상태로 남아 **다음 테스트가 그대로 재시도**한다 — 부팅이
/// timeout 되는 상황(과부하 머신 등)에서 테스트 수만큼 GUI 프로세스가 더 뜨는
/// 증폭을 막기 위해, 한 번 실패하면 이후 호출은 즉시 실패시킨다.
static SHARED_SPAWN_FAILED: AtomicBool = AtomicBool::new(false);

/// 이 test binary 가 공유하는 tasty 인스턴스. 첫 호출에서만 프로세스를 spawn 하고
/// 이후 호출은 같은 핸들을 돌려준다.
///
/// **공유 범위는 "test binary 하나"다 — `cargo test` 전체가 아니다.** `OnceLock` 은
/// 프로세스 로컬 정적 상태이고 cargo 는 test 타겟마다 별도 프로세스를 띄우므로,
/// 이 하네스로 도달 가능한 하한은 *바이너리당 인스턴스 1개*다. 저장소 전체를
/// 1개로 줄이려면 test binary 개수 자체를 줄여야 한다.
///
/// **lock 을 잡지 않는다.** `gui_common::shared()` 는 `MutexGuard` 를 돌려줘 테스트를
/// 완전 직렬화하지만(실제 데스크톱 마우스/포커스를 뺏는 입력 주입을 쓰므로 직렬화가
/// 필수다), 이쪽은 IPC 만 쓴다 — IPC 서버는 연결마다 별도 스레드로 받아 mpsc 로
/// 큐잉하므로 동시 호출이 안전하고, 테스트 간 격리는 [`TastyInstance::create_workspace`]
/// 로 각자 자기 workspace 를 잡아 확보한다(attach 점유도 workspace/surface 단위 lock).
/// 따라서 `&'static` 핸들만 공유하고 테스트는 그대로 병렬 실행한다.
///
/// **주의 — workspace 로 격리되지 않는 전역 상태가 있다.** headless PTY(`pty.*`),
/// `global_hook.*`, notification 은 전역 목록이라 같은 binary 의 다른 테스트가 만든
/// 항목까지 함께 조회된다. 공유 인스턴스 위에서 목록을 검증할 때는 "내 것이 있는가"
/// (`any`) 형태로 쓰고, 길이나 `[0]` 번째를 assert 하지 않는다.
pub fn shared() -> &'static TastyInstance {
    SHARED_INSTANCE.get_or_init(|| {
        assert!(
            !SHARED_SPAWN_FAILED.swap(true, Ordering::SeqCst),
            "공유 인스턴스 spawn 이 이미 실패했다 — 재시도하지 않는다 (첫 실패의 panic 메시지에 stderr tail 이 붙어 있다)"
        );
        let instance = TastyInstance::spawn();
        SHARED_SPAWN_FAILED.store(false, Ordering::SeqCst);
        SHARED_SPAWN_COUNT.fetch_add(1, Ordering::Relaxed);
        // 정적 저장이라 `Drop` 이 영원히 돌지 않는다 — 정리 경로를 프로세스 종료
        // 시점으로 분리한다. libtest 는 `process::exit` 로 끝나므로 atexit 가 돈다.
        // SAFETY: atexit 는 process-lifetime callback 등록. `on_exit` 는 'static fn
        // 포인터이고, `get_or_init` 안이라 중복 등록되지 않는다.
        unsafe {
            libc::atexit(on_shared_exit);
        }
        instance
    })
}

/// `shared()` 가 프로세스를 spawn 한 횟수. 재사용이 실제로 일어났는지(=1) 를
/// 하네스 자체 테스트가 확인하는 용도.
pub fn shared_spawn_count() -> usize {
    SHARED_SPAWN_COUNT.load(Ordering::Relaxed)
}

extern "C" fn on_shared_exit() {
    if let Some(instance) = SHARED_INSTANCE.get() {
        instance.terminate();
    }
}

/// 프로세스 트리 강제 종료. `Drop`(전용 인스턴스)과 atexit(공유 인스턴스)이 함께 쓴다.
fn force_kill(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        // 이미 종료된 pid 면 taskkill 이 실패로 끝난다 — 목적(살아있으면 죽인다)은
        // 어느 쪽이든 달성되므로 status 를 보지 않는다.
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    // SAFETY: SIGKILL 송신은 thread-safe POSIX. pid 가 이미 종료된 상태여도
    // kill 은 errno 만 set 하고 UB 가 없다.
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

/// `workspace.create` 응답이 그대로 돌려준 테스트 전용 격리 단위
/// ([`TastyInstance::create_workspace`] 참조).
///
/// 이 workspace 는 테스트가 끝나도 회수하지 않는다 — `workspace.close` IPC 자체가
/// 없고, 공유 인스턴스가 test 프로세스와 함께 죽으므로 회수할 이유도 없다. 대신
/// **각 테스트가 자기 workspace 밖을 건드리지 않는 것**이 격리의 전부다.
#[derive(Debug, Clone, Copy)]
pub struct TestWorkspace {
    /// workspace id — attach 점유/조회의 대상 키.
    pub id: u64,
    /// `engine.workspaces` 내 인덱스 (생성 시점 기준).
    pub index: usize,
    /// 이 workspace 와 함께 생성된 첫 surface id.
    pub surface_id: u64,
}

pub struct TastyInstance {
    process: Child,
    port: u16,
    port_file: PathBuf,
    isolated_home: PathBuf,
    /// 자식 stderr 의 꼬리와 **마지막 줄의 시각** — 상한을 넘겼을 때 "느리다" 와
    /// "멈췄다" 를 가른다. 생산자는 `spawn_diag` 한 곳이다.
    stderr: StderrCapture,
    startup: StartupTimeline,
}

fn write_isolated_config(isolated_home: &std::path::Path, inherit_cwd: bool) {
    let tasty_dir = isolated_home.join(".tasty");
    std::fs::create_dir_all(&tasty_dir).expect("failed to create isolated .tasty dir");

    let shell_path = if cfg!(windows) {
        // Git Bash 표준 경로. 미설치면 host 의 auto-detect 로 떨어지지만 본 phase 의
        // primary target 은 macOS/Linux 의 `cargo test --workspace` flaky 해소.
        "C:/Program Files/Git/bin/bash.exe"
    } else {
        // /bin/sh 는 모든 POSIX 환경에서 보장 — 가장 보수적.
        "/bin/sh"
    };

    let config = format!(
        r#"[general]
shell = "{shell}"
shell_mode = "default"
shell_args = ""
startup_command = ""
language = "en"
scrollback_lines = 10000
confirm_close_running = false
click_to_move_cursor = true
inherit_cwd = {inherit_cwd}
close_behavior = "ask"
restore_layout = false
restore_surface_content = false
link_click_modifier = "ctrl"
"#,
        shell = shell_path,
        inherit_cwd = inherit_cwd
    );
    std::fs::write(tasty_dir.join("config.toml"), config)
        .expect("failed to write isolated config.toml");
}

impl TastyInstance {
    /// **전용** 인스턴스를 새로 띄운다. 테스트마다 GUI 창이 하나씩 더 뜨므로,
    /// 전용 프로세스가 꼭 필요한 경우가 아니면 [`shared()`] 를 쓴다
    /// (전용이 맞는 경우: 프로세스 기동 시점 config 이 달라야 하거나
    /// — [`Self::spawn_with_inherit_cwd`] — 프로세스 RSS 를 외부에서 재는 soak 하네스).
    pub fn spawn() -> Self {
        Self::spawn_with_inherit_cwd(false)
    }

    /// `inherit_cwd` 설정만 바꿔 띄우는 변형. 이 설정이 게이트하는 동작(convert /
    /// split 의 cwd carry)을 실제 서버 프로세스 상대로 검증할 때 쓴다.
    ///
    /// **이 경로는 공유 대상이 아니다** — `inherit_cwd` 는 격리 HOME 의
    /// `config.toml` 에 미리 써넣는 *프로세스 기동 시점* 설정이라, 이미 떠 있는
    /// 인스턴스에 런타임으로 바꿔 끼울 수 없다. 값이 다른 인스턴스가 필요하면
    /// 항상 별도 프로세스로 남는다.
    pub fn spawn_with_inherit_cwd(inherit_cwd: bool) -> Self {
        Self::spawn_configured(inherit_cwd, &[])
    }

    /// 환경 변수를 더해 띄우는 변형. debug 전용 조절 손잡이(`TASTY_DEBUG_*`)를 켠 인스턴스를
    /// 공유 인스턴스와 따로 세울 때 쓴다.
    pub fn spawn_with_env(extra_env: &[(&str, &str)]) -> Self {
        Self::spawn_configured(false, extra_env)
    }

    fn spawn_configured(inherit_cwd: bool, extra_env: &[(&str, &str)]) -> Self {
        // 유일화 키에 **시각을 안 쓴다.** 시계의 해상도는 플랫폼의 성질이라 같은 코드가
        // 어떤 OS 에서는 유일하고 어떤 OS 에서는 겹친다 — 겹치면 두 완주가 같은 경로를
        // 쓰고 먼저 끝난 쪽이 다른 쪽의 파일을 지운다. 단조 카운터는 해상도가 없어
        // 플랫폼을 안 읽고, 프로세스 전역이라 같은 스레드의 재호출도 가른다.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let port_file = std::env::temp_dir().join(format!("tasty-test-{}.port", unique));

        // 격리된 HOME — 사용자의 ~/.zshrc / oh-my-zsh / p10k / ~/.tasty/ 등이
        // e2e PTY shell 에 새어들어와 prompt 형태와 ZLE binding 을 바꾸는 일을
        // 차단한다. tasty 본 바이너리의 paths 도 모두 HOME 기반이라 같이 격리됨.
        let isolated_home = std::env::temp_dir().join(format!("tasty-test-home-{}", unique));
        std::fs::create_dir_all(&isolated_home).expect("failed to create isolated home");
        // 빈 .zshrc — zsh 가 사용자 customization 안 보게. ZDOTDIR 이 이 디렉토리를
        // 가리키므로 zsh 는 여기서 rc 를 찾는다.
        std::fs::write(isolated_home.join(".zshrc"), "").ok();
        std::fs::write(isolated_home.join(".bashrc"), "").ok();

        // 격리된 ~/.tasty/config.toml 사전 작성 — shell auto-detect 분기를 결정적으로
        // 차단하여 host /etc/passwd 와 $SHELL 의존을 제거한다. shell_setup_mode 진입
        // 경로를 막아 port file 이 항상 작성되도록 보장한다.
        write_isolated_config(&isolated_home, inherit_cwd);

        let mut command = Command::new(spawn_diag::instance_bin());
        command
            .arg("--port-file")
            .arg(port_file.to_str().unwrap())
            .env("HOME", &isolated_home)
            // HOME 만으로는 Windows 에서 격리되지 않는다 — tasty 루트 해석
            // (tasty-utils path.rs)은 directories::BaseDirs(=USERPROFILE) 기반이라
            // 실사용자의 ~/.tasty-debug 를 읽어 세션 복원·설정이 새어든다.
            // TASTY_HOME 이 루트 override 의 SoT 이므로 명시 지정 (전 OS 일관).
            .env("TASTY_HOME", isolated_home.join(".tasty"))
            .env("ZDOTDIR", &isolated_home)
            .env_remove("OH_MY_ZSH")
            .env_remove("ZSH")
            .env_remove("SHELL")
            // 부모 프로세스가 tasty 안에서 실행 중이면 TASTY_SURFACE_ID 가 상속되어
            // child 가 augmented help 만 출력하고 종료한다 (boot/cli_routing.rs:55).
            // 자식은 항상 본 GUI 로 부팅해야 하므로 명시 제거.
            .env_remove("TASTY_SURFACE_ID")
            // host 의 로그 레벨이 새어들어와 child 가 polled stderr 보다 빠르게
            // write 하면 OS pipe buffer 가 가득 차서 child 가 block 될 위험이 있다.
            // drain thread 가 1차 방어, verbosity cap 이 2차.
            //
            // 이름과 값 모두 `spawn_diag` 가 유일한 정의 자리다 — 이름은 한 번
            // `RUST_LOG` 로 틀렸던 자리이고, 값은 제품 기본 필터와 모양이 어긋나면
            // 억제가 풀려 오히려 로그가 늘어난다(그 상수의 doc 에 실측이 있다).
            .env(spawn_diag::LOG_ENV, spawn_diag::LOG_FILTER)
            .stderr(Stdio::piped());
        command.envs(extra_env.iter().copied());
        // 자식의 OS 열기(브라우저 · 파일 관리자)는 실행자의 데스크톱에 닿는다 — 기록만 하게 한다.
        spawn_diag::apply_os_open_record(&mut command, &isolated_home);
        // 이 스위트가 번들 plugin 을 안 부르면 빈 번들로 띄운다 — 격리 홈으로 가는
        // 1 GB 복사가 통째로 사라진다. 명부와 근거는 `spawn_diag` 에 있다.
        spawn_diag::apply_bundle_opt_in(&mut command);
        // 자식이 어느 디스플레이에 창을 띄우는가. 이 줄이 없으면 부모의 값을 그대로
        // 물려받아 **실행자가 보고 있는 화면**이 시험의 디스플레이가 된다 — 그 상태는
        // 조용해서, 격리의 다른 축(HOME · TASTY_HOME · 포트 파일)이 다 맞아도 이 축만
        // 아무도 값으로 못 가른다. 정책과 실측은 `spawn_diag::DISPLAY_ENV`.
        spawn_diag::apply_display_policy(&mut command);
        // 부모(이 test binary)가 어떤 이유로든(SIGKILL 포함) 즉사하면 커널이 이
        // 자식을 대신 죽여준다. 아래 Drop 은 부모가 살아서 unwind 될 때만 자식을
        // 정리하므로, 부모가 그 전에 죽으면 Drop 이 실행되지 않아 자식이 고아로
        // 영구히 남는다 — `spawn_diag::spawn_child` 의 doc 을 반드시
        // 함께 읽을 것(스레드 종속성 문제로 naive prctl 은 공유 인스턴스를 깨뜨린다).
        //
        // 핸들(`Self`)이 서기 전의 패닉은 `Drop` 을 못 부른다 — 그 구간은 회수기가 소유한다.
        let started = Instant::now();
        let process = spawn_diag::spawn_child(command).expect("failed to spawn tasty");
        Self::finish_startup(process, port_file, isolated_home, started)
    }

    /// Complete the normal startup path with an owned child. The harness regression test
    /// supplies its own fake child here, without binary overrides or user configuration.
    pub(super) fn finish_startup(
        process: Child,
        port_file: PathBuf,
        isolated_home: PathBuf,
        started: Instant,
    ) -> Self {
        let mut process = spawn_diag::ChildReaper::new(process);
        let startup = StartupTimeline::new(started, process.child().id());
        let _failure = StartupFailure::new(&startup, None);

        // stderr 를 배경 스레드로 빨아들인다 — 안 읽으면 OS 파이프 역압에 자식이 막힌다
        // (Linux 64 KB / macOS 16 KB). 꼬리 줄과 마지막 줄의 시각이 spawn 단계 패닉의
        // 진단이 된다.
        let mut stderr = StderrCapture::start(process.child().stderr.take(), STDERR_TAIL_LINES);

        // Wait for port file
        let start = Instant::now();
        let port = loop {
            if start.elapsed() > SPAWN_PORT_TIMEOUT {
                // 아직 `Self` 를 못 만들어 `Drop` 이 없다. 자식은 `process`(회수기)가
                // 패닉의 되감기에서 죽이고 거둔다 — 느린 머신에서 부팅이 상한을 넘겨
                // 뒤늦게 뜨는 인스턴스가 고아로 남지 않는다.
                // 아래 둘 다 실패해도 곧바로 panic 이라 보고할 자리가 없다.
                let _ = std::fs::remove_file(&port_file);
                // 위와 동일.
                let _ = std::fs::remove_dir_all(&isolated_home);
                // 락을 `panic!` **인자 안에서** 잡으면 임시 가드가 되감기 끝까지 살아 있어
                // Mutex 가 오염되고, 배출 스레드가 다음 `lock()` 에서 죽어 **이후 실패의
                // stderr tail 이 조용히 사라진다**(F 는 그대로라 알아채기 어렵다).
                // `last_line_age()` 가 메서드인 것이 그 자리를 애초에 안 만든다 —
                // 가드가 메서드 안에서 떨어지므로 `panic!` 의 임시 범위에 안 들어간다.
                panic!(
                    "{}",
                    spawn_diag::spawn_timeout_message(
                        "tasty failed to start",
                        SPAWN_PORT_TIMEOUT,
                        stderr.tail_lines(),
                        &stderr.tail(),
                        stderr.last_line_age(),
                    )
                );
            }
            if let Ok(content) = std::fs::read_to_string(&port_file)
                && let Ok(port) = content.trim().parse::<u16>()
            {
                startup.port_found();
                break port;
            }
            // 자식이 이미 죽었으면 더 기다릴 이유가 없다. 부팅 실패는 대부분
            // 즉사라, 이 확인 하나가 상한 전체를 기다리는 것을 막는다.
            if let Ok(Some(status)) = process.child().try_wait() {
                // 정리 실패해도 곧바로 panic 이라 보고할 자리가 없다(위 timeout 경로와 동일).
                let _ = std::fs::remove_file(&port_file);
                // 위와 동일.
                let _ = std::fs::remove_dir_all(&isolated_home);
                panic!(
                    "{}",
                    spawn_diag::early_exit_message(
                        &status.to_string(),
                        stderr.tail_lines(),
                        // ★ 여기서만 `tail()` 이 아니라 이것을 쓴다 — 자식이 즉사하면
                        // `try_wait()` 가 배출 스레드를 이겨 링이 비어 있고, 진단이
                        // "볼 것이 없다" 로 나간다(그 메서드의 실측 참조).
                        &stderr.tail_after_exit(spawn_diag::STDERR_SETTLE_BUDGET),
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };

        let instance = Self {
            process: process.release(),
            port,
            port_file,
            isolated_home,
            stderr,
            startup: startup.clone(),
        };

        // Wait until the shell is actually ready (has screen content).
        instance.wait_for_shell(instance.first_surface_id());
        instance.startup.shell_ready();

        instance
    }

    /// 해당 surface 의 PTY 가 첫 출력(prompt)을 낼 때까지 폴링한다. spawn 직후의
    /// 첫 surface 뿐 아니라, [`Self::create_workspace`] 로 갓 만든 workspace 의
    /// surface 를 쓰기 전에도 호출한다.
    pub fn wait_for_shell(&self, surface_id: u64) {
        let start = Instant::now();
        loop {
            let text = self.screen_text_of(surface_id);
            if !text.trim().is_empty() {
                return;
            }
            if start.elapsed() > SPAWN_SHELL_TIMEOUT {
                panic!(
                    "{}",
                    spawn_diag::spawn_timeout_message(
                        "shell did not produce output",
                        SPAWN_SHELL_TIMEOUT,
                        self.stderr.tail_lines(),
                        &self.stderr.tail(),
                        self.stderr.last_line_age(),
                    )
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Read a bounded, already-drained diagnostic record without issuing another IPC.
    pub fn find_stderr(&self, pred: impl Fn(&str) -> bool) -> Option<String> {
        self.stderr.find(pred)
    }

    /// Send a JSON-RPC request and return the result value.
    /// Retries on timeout (event loop may be slow when window is unfocused).
    pub fn call(&self, method: &str, params: Value) -> Value {
        let _failure = StartupFailure::new(&self.startup, Some(&self.stderr));
        for attempt in 0..3 {
            let mut stream = match TcpStream::connect(format!("127.0.0.1:{}", self.port)) {
                Ok(s) => s,
                Err(e) => {
                    if attempt < 2 {
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    panic!("failed to connect for '{}': {}", method, e);
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(10))).ok();

            let request = serde_json::json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
                "id": 1
            });

            let mut msg = serde_json::to_string(&request).unwrap();
            msg.push('\n');
            if stream.write_all(msg.as_bytes()).is_err() {
                if attempt < 2 {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                panic!("failed to send for '{}'", method);
            }

            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(_) => {}
                Err(_) if attempt < 2 => {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                Err(e) => panic!("failed to read response for '{}': {}", method, e),
            }

            let resp: Value = serde_json::from_str(&line).expect("invalid JSON response");
            self.startup.first_response();
            if let Some(error) = resp.get("error") {
                panic!(
                    "IPC error for '{}': {}{}",
                    method,
                    error,
                    bundle_staging_note()
                );
            }
            return resp.get("result").cloned().unwrap_or(Value::Null);
        }
        unreachable!()
    }

    /// Fixed startup observations, independent of the rolling stderr tail.
    pub fn startup_diagnostics(&self) -> String {
        self.startup.snapshot()
    }

    /// Send a JSON-RPC request and return the full response (including errors).
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn call_raw(&self, method: &str, params: Value) -> Value {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", self.port))
            .expect("failed to connect to tasty IPC");
        stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });
        let mut msg = serde_json::to_string(&request).unwrap();
        msg.push('\n');
        stream.write_all(msg.as_bytes()).expect("failed to send");
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .expect("failed to read response");
        serde_json::from_str(&line).expect("invalid JSON response")
    }

    /// 테스트 전용 workspace 를 만들고 `workspace.create` 응답의 `id` / `index` /
    /// `surface_id` 를 그대로 돌려준다 — [`shared()`] 인스턴스 위에서 테스트가
    /// 서로의 상태를 밟지 않게 하는 격리 단위다.
    ///
    /// IPC 로 만든 workspace 는 `IntentOrigin::Agent` 라 active 를 전환하지 않으므로
    /// (원칙 1·3), 여러 테스트가 병렬로 호출해도 서로의 active 상태를 흔들지 않는다.
    /// 반환된 surface 의 PTY 가 필요하면 [`Self::wait_for_shell`] 로 기다린다.
    pub fn create_workspace(&self, name: &str) -> TestWorkspace {
        let result = self.call("workspace.create", serde_json::json!({ "name": name }));
        TestWorkspace {
            id: result["id"].as_u64().expect("workspace.create returns id"),
            index: result["index"]
                .as_u64()
                .expect("workspace.create returns index") as usize,
            surface_id: result["surface_id"]
                .as_u64()
                .expect("workspace.create returns surface_id"),
        }
    }

    /// Get the first surface ID from surface.list.
    ///
    /// **전용 인스턴스 전용.** [`shared()`] 위에서 쓰면 다른 테스트가 만든 surface 를
    /// 집어 비결정적이 된다 — 공유 경로에서는 [`Self::first_surface_id_in_workspace`]
    /// 또는 [`TestWorkspace::surface_id`] 를 쓴다.
    pub fn first_surface_id(&self) -> u64 {
        let surfaces = self.call("surface.list", serde_json::json!({}));
        surfaces.as_array().unwrap()[0]["id"].as_u64().unwrap()
    }

    /// Get the first pane ID from pane.list.
    ///
    /// **전용 인스턴스 전용** — 근거는 [`Self::first_surface_id`] 와 같다. 공유
    /// 경로에서는 [`Self::first_pane_id_in_workspace`] 를 쓴다.
    pub fn first_pane_id(&self) -> u64 {
        let panes = self.call("pane.list", serde_json::json!({}));
        panes.as_array().unwrap()[0]["id"].as_u64().unwrap()
    }

    /// 주어진 workspace 에 속한 첫 surface id — 공유 인스턴스용 `first_surface_id`.
    pub fn first_surface_id_in_workspace(&self, workspace_id: u64) -> u64 {
        let surfaces = self.call("surface.list", serde_json::json!({}));
        surfaces
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["workspace_id"].as_u64() == Some(workspace_id))
            .unwrap_or_else(|| panic!("no surface in workspace {workspace_id}"))["id"]
            .as_u64()
            .unwrap()
    }

    /// 주어진 workspace 에 속한 첫 pane id — 공유 인스턴스용 `first_pane_id`.
    pub fn first_pane_id_in_workspace(&self, workspace_id: u64) -> u64 {
        let panes = self.call("pane.list", serde_json::json!({}));
        panes
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["workspace_id"].as_u64() == Some(workspace_id))
            .unwrap_or_else(|| panic!("no pane in workspace {workspace_id}"))["id"]
            .as_u64()
            .unwrap()
    }

    /// Send text to a specific surface.
    pub fn send_text(&self, surface_id: u64, text: &str) {
        self.call(
            "surface.send",
            serde_json::json!({ "surface_id": surface_id, "text": text }),
        );
    }

    /// Set a read mark on a specific surface.
    pub fn set_mark(&self, surface_id: u64) {
        self.call(
            "surface.set_mark",
            serde_json::json!({ "surface_id": surface_id }),
        );
    }

    /// Read output since the last mark, stripping ANSI.
    pub fn read_since_mark(&self, surface_id: u64) -> String {
        let result = self.call(
            "surface.read_since_mark",
            serde_json::json!({ "surface_id": surface_id, "strip_ansi": true }),
        );
        result
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    }

    /// Wait until read_since_mark contains the expected text (with timeout).
    pub fn wait_for_output(&self, surface_id: u64, expected: &str, timeout: Duration) -> String {
        let start = Instant::now();
        loop {
            let output = self.read_since_mark(surface_id);
            if output.contains(expected) {
                return output;
            }
            if start.elapsed() > timeout {
                panic!(
                    "timeout waiting for '{}' in output. Got:\n{}",
                    expected, output
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Get screen text of a specific surface.
    pub fn screen_text_of(&self, surface_id: u64) -> String {
        let result = self.call(
            "surface.screen_text",
            serde_json::json!({ "surface_id": surface_id }),
        );
        result
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    }

    /// OS process id — soak 하네스의 외부 측정(프로세스 트리 RSS/핸들 수)용.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn pid(&self) -> u32 {
        self.process.id()
    }

    /// 이 인스턴스의 격리 `TASTY_HOME` — user 설정 파일을 써 넣고 reload 를 재는 test binary 용.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn tasty_home(&self) -> PathBuf {
        self.isolated_home.join(".tasty")
    }

    /// 자식이 띄우지 않고 기록한 OS 열기의 기록 파일(`spawn_diag::apply_os_open_record`).
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn os_open_log(&self) -> PathBuf {
        self.isolated_home.join(spawn_diag::OS_OPEN_LOG_FILE)
    }

    /// Loopback IPC port — attach stream 핸드셰이크처럼 `call()` 이 감싸지 않는
    /// raw `TcpStream` 을 직접 여는 test binary 용.
    #[allow(dead_code)] // 일부 test binary 만 사용
    pub fn port(&self) -> u16 {
        self.port
    }

    /// graceful shutdown 을 **best-effort 로** 요청한다 — 실패가 정상인 경로다.
    ///
    /// [`Self::call`] 을 쓰지 않는다. `call` 이 error 응답을 panic 으로 올리는 것은
    /// 다른 호출부에서는 옳다(거기서는 error 가 곧 테스트 실패다). 여기서는 실패가
    /// 두 가지 정상 사유로 일어난다:
    ///
    /// 1. **헤드리스 빌드에는 `system.shutdown` 핸들러가 없다.** `src/app.rs` 가
    ///    `app/ipc` 를 `gui` feature 로 게이트하고, `src/boot/headless_dispatch.rs`
    ///    가 그 생략을 설계로 명시한다. 그래서 `-32601` 이 돌아온다.
    /// 2. 이미 죽은 인스턴스는 연결부터 실패한다.
    ///
    /// 어느 쪽이든 뒤이은 force kill 이 회수를 완수하므로 실패가 문제되지 않는다.
    /// 호출부에서 `catch_unwind` 로 삼키는 것으로는 부족했다 — 기본 panic hook 이
    /// unwind **전에** stderr 로 찍어서, 헤드리스 회차마다 인스턴스를 띄우는 타깃
    /// 수만큼 실패처럼 보이는 줄이 산출물에 남는다(실측 8 건). 그래서 여기서는
    /// 애초에 panic 하지 않는 경로로 보낸다.
    ///
    /// [`Self::call_raw`] 로도 부족하다 — 그것은 error 응답은 돌려주지만 연결
    /// 실패에서 `.expect` 로 panic 한다. 위 2번이 정확히 그 경우다.
    pub fn shutdown(&self) {
        let Ok(mut stream) = TcpStream::connect(format!("127.0.0.1:{}", self.port)) else {
            return;
        };
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "system.shutdown",
            "params": {},
            "id": 1
        });
        let Ok(mut msg) = serde_json::to_string(&request) else {
            return;
        };
        msg.push('\n');
        if stream.write_all(msg.as_bytes()).is_err() {
            return;
        }
        let mut line = String::new();
        // 응답은 읽되 내용도 성패도 보지 않는다 — 성공이든 `-32601` 이든 이 경로의
        // 행동은 같고, 읽는 것은 서버가 처리를 마칠 시간을 주기 위해서다. 읽기가
        // 실패했다면 인스턴스가 이미 죽은 것이고, 그것도 이 자리에서는 정상이다.
        let _ = BufReader::new(&stream).read_line(&mut line);
    }

    /// 공유 인스턴스 정리 경로 — atexit 에서 호출한다.
    ///
    /// `Drop` 과 달리 `&self` 만 가진다(정적 저장이라 `&mut` 를 얻을 수 없다).
    /// 그래서 `Child::kill`/`wait` 대신 pid 기반 kill 을 쓴다 — 어차피 test
    /// 프로세스가 종료하는 중이라 reap 은 init 이 대신한다.
    fn terminate(&self) {
        // graceful shutdown 은 best-effort — `shutdown` 자체가 실패를 삼키므로
        // 여기서 감쌀 것이 없다. 회수는 뒤이은 force kill 이 완수한다.
        self.shutdown();
        std::thread::sleep(Duration::from_millis(200));
        force_kill(self.process.id());
        // atexit 안이라 로깅 대상(테스트 출력)이 이미 닫혀 있을 수 있다 — 회수
        // 실패를 보고할 곳이 없으므로 무시한다.
        let _ = std::fs::remove_file(&self.port_file);
        // 위와 동일.
        let _ = std::fs::remove_dir_all(&self.isolated_home);
    }
}

/// **전용 인스턴스 전용 정리 경로.** [`shared()`] 로 얻은 인스턴스는 `'static` 에
/// 살아 `Drop` 이 돌지 않고, 대신 atexit 가 [`TastyInstance::terminate`] 를 호출한다.
impl Drop for TastyInstance {
    fn drop(&mut self) {
        // Try graceful shutdown — `shutdown` 이 실패를 삼키므로 감싸지 않는다.
        self.shutdown();
        // Wait briefly, then force kill the entire process tree.
        std::thread::sleep(Duration::from_millis(200));
        force_kill(self.process.id());
        let _ = self.process.wait();
        // ★ `join()` 은 자식을 거둔 **뒤에** 부른다 — 파이프 EOF 가 자식 종료에서 오므로
        // 이르게 부르면 남은 수명만큼 막힌다(`StderrCapture::join` 의 실측 표).
        self.stderr.join();
        let _ = std::fs::remove_file(&self.port_file);
        let _ = std::fs::remove_dir_all(&self.isolated_home);
    }
}
