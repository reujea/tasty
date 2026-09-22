//! 인스턴스 spawn 의 **공통 설정과 실패 판정**을 두 하네스가 공유하는 자리.
//!
//! `tests/common`(범용 인스턴스)과 `tests/webhook_common`(웹훅 인스턴스)은 같은
//! 바이너리를 같은 방식으로 띄운다([`instance_bin`] 이 그 하나를 정한다). 그런데 상한값이 서로 달랐고
//! (30/15 vs 40/20) 그 차이의 근거가 어디에도 없었다 — 같은 단계에 다른 잣대를
//! 대면 한쪽에서만 재현되는 flaky 가 생기고, 그때마다 "이 하네스는 원래 느린가" 를
//! 사람이 다시 판단해야 한다. 값을 여기 하나로 모은다. 자식의 로그 필터도 같다.
//!
//! 실패 원인 판정도 같이 둔다. spawn timeout 은 "느린 것" 과 "부팅이 아예 막힌 것"
//! 이 똑같은 메시지로 보였는데, 둘은 대응이 완전히 다르다(전자는 기다리면 되고
//! 후자는 기다려도 안 된다). 디스플레이 서버 부재는 stderr 시그니처로 갈리고,
//! 자식이 이미 죽은 경우는 조기 종료 감지로 갈린다.

// 이유: 이 모듈을 include 하는 test binary 마다 쓰는 부분집합이 달라, 안 쓰는 binary 에서
// 죽은 코드가 된다(통합 시험 공용 모듈 — `tests/common/mod.rs` 와 같은 이유).
#![allow(dead_code)]
// 두 하네스가 각자 일부만 쓴다

// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다 — 전수 가드
// (`crates/tasty-doc-guards/tests/let_underscore_documented.rs`)가 테스트 본문을 제외하므로, 여기서 나는
// `let_underscore_must_use` 경고는 정책상 조치 대상이 될 수 없다. 끄지 않으면
// 프로덕션의 진짜 신호가 그 안에 묻힌다 — `docs/dev-guide/error-handling.md`.
#![allow(clippy::let_underscore_must_use)]

use std::time::Duration;

/// 제품이 로그 필터로 읽는 환경변수. **`RUST_LOG` 이 아니다** —
/// `crates/tasty-platform/src/crash_report.rs` 의 `EnvFilter::try_from_env("TASTY_LOG")` 다.
/// 한 번 `RUST_LOG` 로 잘못 넣어 두 하네스가 몇 달 동안 필터 없이 돌았으므로,
/// 이름은 상수로 고정하고 `tests/harness_log_env.rs` 가 제품 소스와 대조한다.
pub const LOG_ENV: &str = "TASTY_LOG";

/// 필터 문자열의 유일한 정의 자리. 두 상수가 같은 리터럴을 각자 적으면 한쪽만
/// 고쳐져 어긋나므로 매크로로 한 번만 쓴다(`concat!` 은 리터럴만 받는다).
macro_rules! product_default_filter {
    () => {
        "warn,wgpu_hal=error,wgpu_core=error,naga=error,egui_winit::clipboard=off"
    };
}

/// 자식에게 주는 기본 로그 필터 — **제품 기본값과 같은 모양**이어야 한다.
///
/// `TASTY_LOG` 를 지정하는 순간 제품의 기본 필터는 통째로 대체된다. 그래서 `warn`
/// 한 단어만 주면 기본값에 들어 있던 억제(`wgpu_hal=error` 등)가 전부 풀려
/// **로그가 오히려 늘어난다** — 실측(격리 HOME, 정상 부팅, 12초): env 미지정 7줄 ·
/// `warn` 12줄(wgpu 5줄) · 이 값 7줄. 늘어난 줄은 `STDERR_TAIL_LINES`(30) 짜리
/// 진단 tail 을 그대로 밀어낸다. host 의 `TASTY_LOG=trace` 누수를 막으면서 노이즈는
/// 늘리지 않으려면 기본값과 같은 모양을 명시하는 수밖에 없다.
pub const LOG_FILTER: &str = product_default_filter!();

/// 웹훅 하네스용 필터 — [`LOG_FILTER`] 를 **그대로 앞에 두고** 뒤에만 덧붙인다.
///
/// 덧붙이는 것은 리스너 타깃의 `info` 한 줄(`webhook listener bound on {addr}`)이다.
/// 도난 **판정**에는 필요 없다 — 그건 제품이 `warn!` 으로 내므로 기본 필터에서도
/// 보인다. 필요한 것은 실패 보고를 사람이 읽을 때다: 실패 tail 에 `bound` 줄이
/// 있으면 "떴는데 connect 가 안 됐다", 없으면 "끝내 안 떴다" 로 갈린다.
pub const LOG_FILTER_WEBHOOK: &str =
    concat!(product_default_filter!(), ",tasty::webhook::listener=info");

/// libtest 캡처로 흘러가는 `tracing` subscriber 를 설치한다(프로세스당 1회, 실패는 무시).
///
/// 하네스 진단을 `eprintln!` 로 쓰면 훅 C.11 의 예외가 필요해진다. 예외를 만들지 않고도
/// 같은 자리에 출력이 남는다는 것을 실측으로 확인했다 — `with_test_writer()` 는 libtest 의
/// per-thread 캡처를 타므로, 실패한 테스트의 출력 블록에 `eprintln!` 과 나란히 찍힌다.
/// 이미 설치돼 있으면 `try_init` 이 `Err` 를 주고 그대로 두는 것이 맞다.
pub fn init_test_tracing() {
    // 두 번째 설치 시도는 정상 흐름이다(같은 바이너리의 다른 테스트가 이미 설치).
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
}

/// 하네스가 띄울 tasty 바이너리를 정하는 **유일한 자리**.
///
/// 기본값은 `CARGO_BIN_EXE_tasty` — **테스트 자신과 같은 feature 로 빌드된 자기
/// 바이너리**다. 그래서 기본(gui) 조합에서는 창과 GPU 디바이스를 만드는 바이너리가
/// 뜨고, `--no-default-features` 조합에서는 같은 경로가 곧 headless 데몬이 된다.
/// IPC 만 쓰는 스위트가 GPU 를 통과해야 하는 이유는 여기에 있다
/// (`docs/adr/0127-e2e-harness-binary-selection.md`).
///
/// `TASTY_E2E_BIN` 이 설정돼 있으면 그 경로를 대신 띄운다. 용도는 **미리 빌드해 둔
/// headless 바이너리를 가리키는 것** — 워크트리 여러 개가 같은 GPU 를 다투는 상황에서
/// IPC 전용 스위트를 GPU 밖으로 빼는 로컬 탈출구다. 절차와 함정은
/// `docs/dev-guide/e2e-tests.md`.
///
/// **함정**: `CARGO_BIN_EXE_tasty` 와 headless 빌드는 `target/debug/tasty` 라는 같은
/// 경로를 다툰다. 따라서 override 는 반드시 **별도 `CARGO_TARGET_DIR` 로 빌드한
/// 산출물의 경로**여야 한다 — 같은 target 디렉토리에 headless 를 빌드하면 다음
/// `cargo test` 가 그것을 gui 로 덮어써서, 아무것도 바뀌지 않았는데 override 가
/// 듣는 것처럼 보인다. 경로로 확정하지 않으면 검증이 조용히 다른 것을 잰다.
///
/// **함정 2**: 그 target 디렉토리를 **레포 밖에 두면** plugin 번들이 안 만들어진다. host 는
/// `exe_dir/builtin-plugins` 를 먼저 보고, 없으면 exe 의 두 단계 위를 워크스페이스 루트로
/// 역산하는데 — 레포 밖 디렉토리에서는 그 역산이 `crates/` 없는 경로를 가리켜 실패하고
/// 데몬이 plugin namespace 없이 올라온다.
///
/// **함정 3**: `--workspace` 없이 빌드하면 그 target 에 `tasty-plugin-*` 바이너리가 **하나도
/// 안 생긴다**. 함정 2 와 독립이다 — 레포 안에 두어 역산이 맞아도 동기화할 바이너리가 없다.
///
/// 두 함정의 증상이 같다: `Method not found: <plugin namespace>.<method>`. 이는 §0 의 stale
/// plugin drift 및 "headless 에 아직 배선되지 않은 경로" 와도 **문구가 같아** 빌드 절차 결함이
/// IPC 표면 차이로 오독된다. 그래서 override 절차는 레포 안 target + `--workspace` 둘 다를
/// 요구한다 (`docs/dev-guide/e2e-tests.md` §0-1 — 세 팔 실측표가 두 조건을 따로 가른다).
///
/// **함정 4 (닫혔다)**: 이 override 는 **데몬만** 조합을 바꾼다. 테스트 바이너리는
/// 자기 조합으로 컴파일된 채라, 데몬 동작을 `cfg(feature = "gui")` 로 갈라 단언하는
/// 테스트는 단언이 구조적으로 뒤집힌다. 그래서 **그런 단언을 가진 스위트는 override 를
/// 받지 않는다** — [`daemon_kind`] 가 스위트별로 가르고, `e2e_tests` 가 그 하나다.
/// 실측 2026-09-05(닫기 전): gui 테스트 바이너리 + 헤드리스 데몬으로 11 스위트를 돌려
/// `e2e_tests` 만 5 건 깨졌고, 그중 4 건이 조합 교차였다. 지금은 그 스위트가 자기
/// 조합의 데몬을 그대로 띄우므로 그 4 건이 나지 않는다.
///
/// 존재하지 않는 경로를 주면 spawn 이 "그냥 실패" 하는 대신 **여기서** 죽는다 —
/// 30 초를 기다린 뒤 port file 미작성으로 오진되는 것을 막는다.
pub fn instance_bin() -> std::ffi::OsString {
    let from_env = std::env::var_os(INSTANCE_BIN_ENV);
    // 경로 검증은 **이 스위트가 override 를 쓰든 안 쓰든** 한다. 오타를 쓴 사람은
    // 어느 스위트를 돌리든 그 자리에서 알아야 하고, 안 그러면 "왜 안 듣지" 가 된다.
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
            "{INSTANCE_BIN_ENV} 가 가리키는 바이너리가 소스보다 낡았다.\n\
             \x20 바이너리: {}\n\x20 더 새 소스: {}\n\
             낡은 데몬은 **정상 부팅해 정상 응답한다** — 그래서 이 스위트는 옛 코드에 대해 \
             통과하거나 실패하고, 그 오진은 양방향이다(고친 것이 안 고쳐진 것처럼도, \
             되돌린 것이 여전히 고쳐진 것처럼도 보인다).\n\
             다시 빌드하거나(`scripts/build-e2e-headless.sh`) `{INSTANCE_BIN_ENV}=` 로 꺼라 \
             — docs/dev-guide/e2e-tests.md",
            std::path::Path::new(v).display(),
            newer.display()
        );
    }
    resolve_instance_bin(effective.as_deref(), env!("CARGO_BIN_EXE_tasty"))
}

/// override 가 **이 스위트에 실제로 적용되는가**. 순수 함수로 둔다 — 환경변수를
/// 건드리지 않고 두 갈래를 다 시험할 수 있어야 한다.
///
/// 조합 의존 단언을 가진 스위트가 override 를 안 받는 것이 **이 설계의 안전장치
/// 전부**다. 그것이 무너지면 데몬만 조합이 바뀌어 그 단언들이 구조적으로 뒤집힌다
/// (함정 4). 그래서 여기에 테스트가 붙어 있다.
fn effective_override(
    kind: DaemonKind,
    from_env: Option<std::ffi::OsString>,
) -> Option<std::ffi::OsString> {
    match kind {
        DaemonKind::SameCombo => None,
        DaemonKind::HeadlessOk => from_env,
    }
}

/// 낡음 판정이 훑을 소스 뿌리. 문서·워크플로는 데몬 동작을 안 바꾸므로 안 본다.
fn repo_roots() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![root.join("src"), root.join("crates")]
}

/// `bin` 보다 **새로운** `.rs` 가 하나라도 있으면 그 경로를 준다.
///
/// 첫 하나에서 멈춘다 — 몇 개가 새것인지는 판정에 필요 없고, 전수로 훑으면 회차마다
/// 무는 비용이 된다. 실측 10~14 ms(`find` 등가).
///
/// **동률(`==`)은 새것으로 센다.** 소스 mtime 이 바이너리와 **같은 눈금**에 떨어지면
/// 순서를 알 수 없다 — 그 소스가 링크 전에 쓰였는지 후에 쓰였는지 파일시스템이 답을
/// 안 준다. 엄격 초과(`>`)로 재면 그 판정 불가가 **조용히 "안 낡았다" 로 흡수되고**,
/// 이 판정이 막으려는 바로 그것(옛 코드에 대고 재는 것, 오진이 양방향)이 통과한다.
/// 실측: 소스와 바이너리를 같은 값으로 찍으면 `>` 는 못 봤다 —
/// [`tests::a_source_stamped_to_the_same_tick_is_still_seen`] 가 그 자리를 잡는다.
/// 반대 방향의 비용은 **다시 빌드 한 번**이고, 아래 패닉이 끄는 법까지 알려 준다.
/// 판정 불가를 실패 방향으로 보내는 같은 극성 선택이 ADR-0182 의 명부에도 있다.
///
/// **mtime 을 못 읽는 경로는 "새것 아님" 으로 넘긴다.** 판정 불가를 빨강으로 만들면
/// 권한·심볼릭 링크 같은 환경 차이가 곧바로 거짓 빨강이 되는데, 이 판정의 목적은
/// 낡은 것을 잡는 것이지 파일시스템을 검사하는 것이 아니다.
/// ★ 이 갈래는 **선언된 거짓 음성**이고 결함이 아니다 — 다만 "빌드 스크립트가 먼저
/// 걸러 준다" 로 셈하지 마라. 그쪽도 같은 mtime 을 보므로 **읽을 수 없는 파일은 두 층
/// 모두 못 본다.** 받쳐 주는 층이 없는 갈래다.
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

/// 이 스위트가 어떤 데몬을 원하는가.
///
/// 두 값의 차이는 **조합 의존 단언을 가지는가** 하나다. 가진 스위트는 데몬이
/// 테스트 바이너리와 같은 조합이어야 그 단언이 뜻을 갖고, 안 가진 스위트는
/// 헤드리스 데몬으로 충분하다 — 그리고 그러면 그 스위트는 GUI 부팅을 통째로
/// 건너뛴다(창 + wgpu 디바이스 + boot 상태기계).
pub enum DaemonKind {
    /// 데몬이 **테스트 바이너리와 같은 조합**이어야 한다. override 를 무시한다.
    SameCombo,
    /// IPC / attach 스트림만 쓴다 — 헤드리스 데몬으로 충분하다.
    HeadlessOk,
}

/// 인스턴스를 띄우는 스위트 중 **헤드리스 데몬으로 충분한 것들.**
///
/// 여기 없는 스위트는 [`DaemonKind::SameCombo`] 로 떨어진다 — **모르는 것은 안전한
/// 쪽으로 보낸다.** 새 스위트가 조합 의존 단언을 갖고 들어왔는데 목록이 기본으로
/// 헤드리스면 override 를 켠 사람에게 **틀린 빨강**이 가지만, 반대 방향의 누락은
/// "최적화를 놓친다" 로 끝난다. 두 오류가 비대칭이라 기본값을 이쪽으로 둔다.
///
/// 명부가 `EXPECTED_INSTANCE_TESTS` 와 어긋나지 않는 것은
/// `tests/e2e_single_instance_guard.rs` 가 본다.
const HEADLESS_OK_SUITES: &[&str] = &[
    "attach_attention_loopback",
    "attach_convert_cwd_loopback",
    "attach_git_query_loopback",
    "attach_list_dir_loopback",
    "attach_local_creation_tap",
    "attach_markdown_content_loopback",
    "attach_silent_disconnect",
    "hook_env_integration",
    "hooks_detection_e2e",
    "shared_instance_harness",
    "soak_memory",
    "webhook_integration",
];

/// 이 스위트의 판정. `CARGO_CRATE_NAME` 은 통합 테스트에서 **test 타깃 이름**으로
/// 확장되고(실측), 이 모듈은 각 테스트 바이너리에 함께 컴파일되므로 스위트마다
/// 다른 값이 된다.
///
/// 명부 밖(= [`DaemonKind::SameCombo`])은 두 갈래다.
///
/// * 테스트 쪽 단언이 조합으로 갈린다 — `e2e_tests`. 실측 2026-09-05: 인스턴스를 띄우는
///   11 스위트 중 `cfg(feature = "gui")` 계열 사이트를 가진 것은 `e2e_tests.rs` 하나였다.
///   `docs/dev-guide/e2e-tests.md` §0-1 이 gui 테스트 바이너리 + 헤드리스 데몬으로 그
///   11 스위트를 돌려 `e2e_tests` 만 깨진 것을 기록해 두었다.
/// * 단언은 하나인데 **데몬 쪽**이 조합마다 다른 호출측을 잰다 —
///   `attach_structure_sync_loopback`(ADR-0482 의 forward 회신: gui 데몬과 헤드리스 데몬이
///   서로 다른 함수로 만든다). `cfg(feature` 를 세는 것으로는 안 보인다. 헤드리스 데몬을
///   받으면 초록은 그대로인데 gui 호출측을 안 잰다(실측 2026-09-23, ADR-0170 보강).
///
/// `gui_tests` 는 애초에 이 경로를 안 쓴다(`BIN_SELECTION_ALLOWLIST`).
pub fn daemon_kind() -> DaemonKind {
    if HEADLESS_OK_SUITES.contains(&env!("CARGO_CRATE_NAME")) {
        DaemonKind::HeadlessOk
    } else {
        DaemonKind::SameCombo
    }
}

/// [`instance_bin`] 의 선택 규칙만 떼어낸 것 — 환경변수를 건드리지 않고 시험할 수
/// 있게 순수 함수로 둔다(테스트가 병렬로 도는데 `set_var` 는 프로세스 전역이다).
///
/// 빈 문자열은 **미설정과 같게** 다룬다. 셸에서 `TASTY_E2E_BIN=` 로 비우는 것이
/// "기본으로 되돌린다" 는 뜻으로 읽히는 것이 자연스럽고, 빈 경로를 그대로 spawn 하면
/// 원인을 알 수 없는 실패가 된다.
fn resolve_instance_bin(
    from_env: Option<&std::ffi::OsStr>,
    default_bin: &str,
) -> std::ffi::OsString {
    match from_env {
        Some(v) if !v.is_empty() => v.to_os_string(),
        _ => std::ffi::OsString::from(default_bin),
    }
}

/// 하네스가 띄울 바이너리를 덮어쓰는 환경변수 이름.
pub const INSTANCE_BIN_ENV: &str = "TASTY_E2E_BIN";

/// S1 — `--port-file` 에 포트가 쓰이기까지. GUI 부팅(창 + GPU 디바이스 + boot
/// 상태기계)이 끝나야 IPC 가 시작되므로 이 단계가 가장 길다.
///
/// 값의 근거: dev cold path worst-case(GPU init + plugin discover/extract +
/// theme/db init, dev 프로필이 release 의 ~3.5 배) + self-hosted runner 변동 폭.
/// 두 하네스가 쓰던 30 s / 40 s 중 **큰 쪽**으로 맞춘다 — 웹훅 하네스의 상한을
/// 낮추는 것은 근거 없는 동작 축소이고, 반대로 올리는 쪽은 *이미 실패할 spawn* 이
/// 보고되기까지의 시간만 늘린다. 그 시간은 [`early_exit_message`] 경로가 대부분
/// 없앤다(자식이 죽었으면 상한을 기다리지 않는다).
pub const SPAWN_PORT_TIMEOUT: Duration = Duration::from_secs(40);

/// S2 — 첫 surface 의 PTY 가 프롬프트를 낼 때까지. S1 이 끝난 뒤라 GPU 와 무관하다.
pub const SPAWN_SHELL_TIMEOUT: Duration = Duration::from_secs(20);

/// 공유 인스턴스의 **첫 spawn 이 실패하면 다시 시도하지 않게** 막는 래치.
///
/// `OnceLock::get_or_init` 은 초기화 클로저가 panic 하면 **미초기화 상태로 남는다.**
/// 그래서 다음 테스트가 그 클로저를 그대로 다시 돈다 — 부팅이 막힌 조건(디스플레이
/// 부재·GPU 초기화 실패·포트 파일 미작성)에서는 **테스트 수만큼 프로세스가 실제로 더
/// 뜨고** 각각이 상한까지 기다린다. 실측(`gui_tests`, 디스플레이 없이 6 건):
/// 래치 없이 spawn 시도 **6 회**, 패닉 자리는 **1 곳**. 즉 실패 6 건이 사건 1 개다.
///
/// **왜 공유 static 이 아니라 타입인가.** 래치가 지켜야 하는 것은 "이 하네스의 공유
/// 인스턴스" 하나다. 한 test binary 가 하네스를 둘 이상 품으면 static 하나로는 한쪽의
/// 실패가 다른 쪽의 spawn 을 막아 **가짜 실패**를 만든다. 그래서 기전만 여기서 공유하고
/// 상태는 하네스가 각자 자기 `static` 으로 갖는다.
///
/// 사용법 — `get_or_init` 클로저의 **첫 줄**과 spawn 직후:
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
/// **채널의 유무가 아니라 그 채널이 축의 어디까지 덮는가가 행마다 다르다 — 갈라서 적는다.**
///
/// 그래서 아래 표의 오른쪽 칸은 시험 이름만 적지 않고 **무엇까지 보는지**를 함께 적는다.
/// ★ 이 자리에 분수를 쓰지 마라. 2026-09-08 이전 이 머리말은 "이 처방을 재는 채널은 **반만**
/// 있다" 였고, 그때 표가 두 행(있다 · ★ 없다)이라 그 "반" 은 실제로 센 값이었다. 같은 날
/// 나머지 행이 채워지면 그 분수는 **3 분의 3** 이 되는데, 문장을 그대로 두면 없는 구멍을
/// 계속 주장하고, 갱신하면 이번엔 **덮이지 않는 부분이 사라진 것처럼** 읽힌다. 둘 다
/// 틀리다 — 남은 구멍은 행의 **유무**가 아니라 행 **안의 범위**이고(`순서만` ·
/// `한쪽 하네스만`), 그것은 행을 세어서는 안 나온다. 아래 14 가 갈린 것과 같은 축이다:
/// 수는 남고 그 수를 낳던 술어가 밑에서 바뀐다.
///
/// | 무엇을 재나 | 채널 |
/// |---|---|
/// | 래치 **타입**이 계약대로 도는가 | `the_latch_blocks_the_second_spawn_and_a_success_releases_it` — 있다. 이 모듈을 **추이적으로** 들이는 test binary **14 개**에 컴파일되어 들어간다(아래 **그 14 를 낳는 술어**) |
/// | 하네스가 래치를 **맞는 자리에 뒀는가** | `spawn_latch_precedes_the_spawn` — **순서만** 본다(아래) |
/// | 그 배치가 부팅이 막힌 환경에서 실제로 증폭을 막는가 | `a_blocked_boot_costs_one_spawn_attempt_no_matter_how_many_tests_ask` — **한쪽 하네스만**(아래) |
///
/// **그 14 를 낳는 술어 — 수가 아니라 이것이 값이다.** 수만 적으면 다음 사람이 다른
/// 술어로 세고 다른 값을 낸다. 실측 2026-09-08: 세 쪽이 **14 · 15 · 4** 를 냈고 셋 다
/// 재현 가능한 값이었다 — 서로 다른 세 술어가 전부 "이 모듈을 들이는 수" 라는 **같은
/// 이름**을 달고 있었을 뿐이다. 여기 적은 14 는 루트 패키지의 `kind=test` 타깃(= 최상위
/// `tests/*.rs`) 중 이 모듈을 **추이적으로** 들이는 것의 수이고, 아래 셋이 함께 못 박혀야
/// 재현된다.
///
/// ① **이 파일 자신은 안 센다.** 카고는 하위 디렉터리의 `mod.rs` 를 test binary 로 만들지
///    않는다 — `cargo metadata --no-deps` 의 `kind=test` 타깃 중 `mod.rs` 는 **0 개**다.
///    자신을 세면 **15** 가 되고, 그것이 이 수가 갈리는 가장 흔한 형태다.
/// ② **"들인다" 는 추이적이다.** 직접 `mod spawn_diag;` 를 적은 자리는 **4** 뿐이다
///    (`tests/common/mod.rs` · `tests/gui_common/mod.rs` · `tests/webhook_common/mod.rs` ·
///    `tests/harness_log_env.rs`). 나머지 10 은 그 `*_common` 들을 거쳐 들어온다. 직접
///    선언만 세면 **4** 가 나온다.
/// ③ **`required-features` 가 붙은 타깃도 이 수에 든다** — `gui_tests` 가 그 하나다.
///    그래서 이 14 는 **컴파일되어 그 바이너리에 들어가는 수**이지 자동 채널에서 도는
///    수가 아니다. 이 문서가 아래에서 "`gui_tests` 는 어떤 자동 채널도 안 돈다" 고 적으므로,
///    두 수를 같은 낱말("돈다")로 부르면 그 문단과 어긋난다.
///
/// 재는 법. 셸 `grep` 으로 세지 마라 — 이 환경의 `grep` 은 ugrep 이라 `-E` 방언이 갈리고,
/// 빈 출력이 값인지 방언인지 구별되지 않는다.
///
/// ```ignore
/// cargo metadata --no-deps --format-version 1   // 모수: kind=test 타깃
/// // 개체: 각 타깃의 src_path 에서 `mod NAME;` 과 `#[path = "..."] mod NAME;` 간선을
/// //       추이적으로 따라가 tests/spawn_diag/mod.rs 에 닿는 타깃을 python 으로 센다.
/// ```
///
/// 래치를 **맞는 자리에 뒀는가**가 왜 배선의 성질인가. 위 사용법이 요구하는 것은 "`entering` 이 `get_or_init`
/// 클로저 **안** 첫 줄에 있을 것" 인데, 그 줄이 클로저 밖으로 나가거나 `spawn()` 뒤로
/// 밀려도 **컴파일되고 단위 시험도 초록**이다. 클로저 밖에서는 `OnceLock` 이 그 자리를 한
/// 번만 부르므로 래치가 영영 안 걸리고, 그 사실은 **부팅이 막힌 환경에서 벽시계로만**
/// 드러난다. 그 조건을 자동 잡이 만들지 않는다 — `gui_tests` 는 어떤 자동 채널도 돌리지
/// 않고(`check-headless` 도 안 본다), 나머지 두 하네스는 그 환경에서 부팅에 성공한다.
///
/// 그래서 2026-09-08 에 **회귀가 드러나는 축만** 잘라 정적 판정으로 옮겼다:
/// `crates/tasty-doc-guards/tests/spawn_latch_precedes_the_spawn.rs` 가 프로세스를 띄우는
/// `get_or_init` 클로저마다 래치 표지가 `spawn()` **앞**에 있는지 본다. 그 타깃은
/// `check-headless` 가 main push 마다 돌린다 — `gui_tests` 에는 없는 채널이다.
///
/// **순서는 텍스트로 보이지만 *효과*는 안 보인다** — 래치가 앞에 있어도 풀리는 시점이
/// 어긋나면 아무것도 안 막는다. 그 조건은 2026-09-08 부터 **만들어서** 잰다: 하네스가
/// 띄울 바이너리를 실재하지 않는 경로로 덮으면([`INSTANCE_BIN_ENV`]) [`instance_bin`] 이
/// 그 자리에서 죽어 프로세스를 하나도 안 띄우고 부팅 실패를 재현한다. 그 조건을 **자식
/// 프로세스**에서 만들고(공유 인스턴스는 프로세스 전역이라 같은 바이너리 안에서는
/// 나머지 스위트와 공존할 수 없다) 자식 출력에서 spawn 경로에 닿은 횟수가 **1** 인지,
/// 나머지가 래치의 문장을 달고 나오는지를 센다. `tests/shared_instance_harness.rs` 가
/// 그것이고 `check-headless` 가 main push 마다 돌린다.
///
/// ★ **그 채널은 `tests/common/mod.rs` 쪽 하네스만 덮는다.** `tests/gui_common/mod.rs` 는
/// `tests/gui_tests.rs` 에 살고 그 바이너리는 어떤 자동 채널도 안 돈다 — 거기에 같은
/// 시험을 넣어도 아무도 안 돌린다. 밖에서 그 바이너리를 재실행하려면 경로를 빌드 산출물
/// 디렉토리에서 주워야 하는데, 자동 잡이 도는 조합에서는 그 바이너리가 **빌드되지도
/// 않아** 시험이 건너뛰기로 끝난다. 건너뛴 잡은 0 건 발견과 구별되지 않으므로 그것은
/// 채널이 아니다. 그쪽은 여전히 **사람이 슬롯에서 벽시계를 재는 것**뿐이다. 재는 법:
/// 부팅이 막힌 환경에서 그 binary 를 통째로 돌리고 (1) spawn 시도 수가 **1** 인가,
/// (2) 실패 건수는 그대로인가(래치는 수를 안 줄인다 — 시간과 메시지를 바꾼다), (3) 두 번째
/// 이후 실패가 [`SpawnOnceLatch::entering`] 의 문장을 달고 나오는가를 본다. 세 값이 다
/// 맞아야 배선이 확인된다. 실측 예: `gui_tests` 33 건이 래치 이전에 **546 s**(≈ 33 × 15 s
/// 상한)였다.
pub struct SpawnOnceLatch {
    failed: std::sync::atomic::AtomicBool,
}

impl SpawnOnceLatch {
    pub const fn new() -> Self {
        Self {
            failed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 초기화 클로저에 **들어가면서** 부른다. 이미 한 번 실패했으면 여기서 죽는다 —
    /// 두 번째 프로세스를 띄우기 **전에** 막는 것이 이 함수의 전부다.
    ///
    /// 첫 실패의 panic 메시지에 stderr tail 과 실패 판정이 붙어 있으므로, 이 메시지는
    /// 원인을 다시 설명하지 않고 **어디를 보라고만** 말한다.
    pub fn entering(&self, what: &str) {
        assert!(
            !self.failed.swap(true, std::sync::atomic::Ordering::SeqCst),
            "{what} 의 첫 spawn 이 이미 실패했다 — 재시도하지 않는다. \
             원인과 stderr tail 은 이 binary 의 **첫 번째** 실패 메시지에 있다"
        );
    }

    /// spawn 이 성공했을 때 부른다. 이 호출이 없으면 래치가 내려간 채로 남아,
    /// 성공한 인스턴스를 쓰는 다음 호출이 잘못 막힌다.
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

/// 자식 stderr 의 **꼬리 N 줄**과 **마지막 줄의 시각**을 배경 스레드로 모은다.
///
/// 배경 스레드인 이유는 OS 파이프 역압이다 — 안 읽으면 자식이 stderr 쓰기에서 막힌다
/// (Linux 64 KB · macOS 16 KB). 이 두 값은 이 모듈의 판정 함수들이 그대로 소비한다
/// ([`spawn_timeout_message`] · [`stderr_silence_verdict`] · [`early_exit_message`]).
///
/// **왜 여기 있나.** 판정(소비자)은 이미 이 모듈에 모여 있었는데 포착(생산자)만 세 하네스에
/// 흩어져 있었고, 셋이 바이트 단위로 같았다. 같은 것이 셋이면 규칙도 셋이라, 하나를
/// 고쳐도 나머지 둘이 남는다.
///
/// ★ **[`Self::last_line_age`] 가 메서드인 것이 이 타입의 요점이다.** 예전에는 호출부가
/// `stderr_last_at.lock().unwrap().map(|t| t.elapsed())` 를 **`panic!` 의 인자 안**에서
/// 평가했고, 그러면 가드가 그 statement 끝까지 살아 있어 되감기 중에 Drop 되며 뮤텍스를
/// 오염시킨다. 그 뒤로는 배경 스레드가 다음 줄에서 죽어 **그 바이너리의 이후 실패가
/// stderr 꼬리를 잃는다** — 실패 수는 그대로인데 진단만 사라지는 형태다.
/// 실측(rustc, edition 2021·2024 동일): 인자 안이면 오염 `true`, 값을 먼저 꺼내
/// statement 밖에서 가드를 떨어뜨리면 `false`. 메서드로 감싸면 가드가 메서드 안에서
/// 떨어지므로 **오염될 자리가 애초에 생기지 않는다.**
pub struct StderrCapture {
    ring: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
    last_at: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
    drain: Option<std::thread::JoinHandle<()>>,
    tail_lines: usize,
}

/// 링이 붙드는 줄 수. 셋이 같은 값을 쓰고 있었으므로 정의를 하나로 모은다.
/// 꼬리로 **보여줄** 줄 수([`STDERR_TAIL_LINES`])는 이것과 별개다 — 링은 판정이 쓸 수
/// 있는 상한이고, 꼬리는 그중 사람에게 보여줄 몫이다.
const STDERR_RING_CAPACITY: usize = 256;

/// 실패 문구에 싣는 꼬리 줄 수. **[`StderrCapture::start`] 의 인자로 남는다** — 표시
/// 선택이지 판정 문턱이 아니라, 하네스가 다른 값을 줘도 flaky 를 안 만든다. 지금은 셋 다
/// 이 값을 준다.
///
/// ★ 한때 webhook 하네스만 40 이었다. 그 값이 태어난 커밋(`072e9399c`, 2026-07-11 —
/// 그 하네스의 신설 커밋)의 본문에도 소스 주석에도 근거가 없고, 바로 다음 줄에서 태어난
/// `SPAWN_PORT_TIMEOUT`(40 초)과 수가 같다. 근거가 아니라 이웃한 숫자를 옮겨 적은
/// 모양이라, 30 으로 모았다 — 근거 없는 차이는 유지 비용만 남긴다.
pub const STDERR_TAIL_LINES: usize = 30;

/// 자식이 죽은 뒤 [`StderrCapture::tail_after_exit`] 가 배출을 기다리는 상한.
///
/// 이 시간은 **이미 실패한 경로에서만** 쓰인다 — 초록 회차의 벽시계에 안 더해진다.
/// 그래서 넉넉히 잡는다: 죽은 자식의 파이프에 남은 것은 많아야 커널 버퍼 한 개
/// (Linux 64 KB)라 읽기 자체는 마이크로초지만, 부하가 높은 러너에서는 그 스레드가
/// 스케줄되는 것 자체가 늦는다. 이 축이 다루는 결함이 바로 그 부하 의존이다.
pub const STDERR_SETTLE_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

impl StderrCapture {
    /// `child.stderr.take()` 를 그대로 넘긴다. `None` 이면 포착 없이 빈 채로 산다 —
    /// 자식이 stderr 를 안 준 경우에도 실패 경로가 그대로 돌아야 한다.
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

    /// 꼬리 N 줄을 개행으로 이어 붙인 것. 진단 문구가 그대로 싣는다.
    pub fn tail(&self) -> String {
        let ring = lock(&self.ring);
        let start = ring.len().saturating_sub(self.tail_lines);
        ring.iter()
            .skip(start)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 이 포착이 보여주는 꼬리 줄 수 — 진단 문구가 같은 수를 함께 찍는다.
    pub fn tail_lines(&self) -> usize {
        self.tail_lines
    }

    /// 마지막 줄이 온 뒤 흐른 시간. [`stderr_silence_verdict`] 가 그대로 받는다.
    /// ★ 값을 돌려주고 가드는 여기서 떨어진다 — 타입 doc 의 이유 참조.
    pub fn last_line_age(&self) -> Option<std::time::Duration> {
        let at = *lock(&self.last_at);
        at.map(|t| t.elapsed())
    }

    /// 자식이 **이미 죽은 것을 확인한 뒤** 부르는 꼬리 읽기. 배출 스레드가 남은 줄을 다
    /// 읽을 때까지 `budget` 만큼 기다렸다가 꼬리를 준다.
    ///
    /// ★ **이 메서드가 있는 이유가 실측이다.** 즉사하는 자식(디스플레이 부재·GPU 초기화
    /// 실패 — 하네스 주석들이 "가장 흔한 부팅 실패" 로 적어 둔 바로 그것)에서는 하네스의
    /// `try_wait()` 가 배출 스레드보다 먼저 이긴다. 그러면 링이 아직 비어 있고 진단은
    /// **"stderr 이 비어 있다 — 볼 것이 없다"** 로 나간다. 실측 2026-09-08(같은 가짜 자식,
    /// 45 줄을 쓰고 종료): 곧바로 죽으면 꼬리 **0 줄**, `sleep 1` 을 끼우면 **30 줄**.
    /// 두 팔의 차이는 자식의 수명뿐이고, 그 문장은 **틀린 방향을 가리킨다** — 읽을 것이
    /// 없던 것이 아니라 아직 안 읽힌 것이다.
    ///
    /// 상한을 두는 이유는 [`Self::join`] 의 계약이다 — 손자 프로세스가 stderr 를 물려받아
    /// 살아 있으면 EOF 가 안 오고, 무한정 기다리면 **실패가 정지로 바뀐다.** 상한을 넘으면
    /// 그 사실을 꼬리 끝에 적는다: 빈 꼬리의 두 이유("볼 것이 없다" 와 "못 읽었다")가 같은
    /// 화면으로 나가면 다음 사람이 없는 원인을 찾는다.
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
            // 자식이 죽어 파이프가 EOF 를 냈으므로 여기서는 즉시 돌아온다(계약 표의 첫 줄).
            self.join();
            return self.tail();
        }
        format!(
            "{}\n(stderr 배출이 {budget:?} 안에 안 끝났다 — 위 꼬리는 **잘렸을 수 있다.** \
             자식이 죽었는데도 파이프가 안 닫혔다면 stderr 를 물려받은 손자가 살아 있는 것이다.)",
            self.tail()
        )
    }

    /// 꼬리에서 술어에 맞는 첫 줄. 특정 실패 시그니처(bind 실패 등)를 집을 때 쓴다.
    pub fn find(&self, pred: impl Fn(&str) -> bool) -> Option<String> {
        let ring = lock(&self.ring);
        ring.iter().find(|line| pred(line)).cloned()
    }

    /// 배경 스레드를 거둔다. **자식이 끝난 뒤** 하네스의 `Drop` 이 부른다.
    ///
    /// ★ **"자식이 끝난 뒤" 는 주의사항이 아니라 계약이다 — 양쪽으로 위험하다.**
    /// 배출 스레드는 `reader.lines()` 가 `None` 을 줄 때 끝나고, 파이프의 EOF 는 자식이
    /// stderr 를 닫을 때(대개 종료할 때) 온다. 실측(3 초 사는 자식, 0.3 초 뒤 호출):
    ///
    /// | 부르는 시점 | 결과 |
    /// |---|---|
    /// | 자식을 거둔 뒤 (계약대로) | **3.84 µs** 만에 반환 |
    /// | 자식이 살아 있는데 | **2.70 s 막힌다** — 남은 수명만큼 |
    /// | 안 부르고 `drop` | 3.17 µs 반환, **배경 스레드는 남는다** |
    ///
    /// 앞쪽이 더 나쁘다. 예산이 있는 하네스에서 이르게 부르면 **조용히 멈춰 있고**
    /// 실패 문구는 자식이나 부팅 단계를 지목한다 — 멈춘 자리를 안 가리킨다.
    ///
    /// ★★ **이 순서를 지키는 자동 채널은 없다.** 위 표는 손으로 잰 값이고, 어떤 시험도
    /// 하네스가 `kill`/`wait` 뒤에 `join()` 을 부르는지 확인하지 않는다. 순서를 어겨도
    /// 컴파일되고, 어긴 결과는 **빨강이 아니라 지연**이라 초록 회차에서는 안 보이고
    /// 빨간 회차에서는 다른 자리를 지목한다. 그 사실을 적어 두는 이유는, 안 적으면
    /// 이 타입을 쓰는 초록이 순서까지 지켰다는 뜻으로 읽히기 때문이다.
    ///
    /// 채널을 만든다면 자리는 여기가 아니라 **하네스 쪽**이다 — 자식을 거뒀다는 사실을
    /// 타입이 알 방법이 없다(`Child` 를 안 들고 있다). 들게 만들면 이 타입이 프로세스
    /// 수명까지 소유하게 되어, 지금 세 하네스가 각자 다르게 하는 정리 순서를 하나로
    /// 강제한다. 그 결정은 이동을 하는 소유자가 내린다.
    ///
    /// ★ **그래서 `Drop` 으로 자동화하면 안 된다.** `Drop` 에서 무조건 join 하면 위
    /// 두 번째 줄이 **기본 동작**이 된다. 뒤쪽(스레드가 남는 것)은 프로세스 전역이라
    /// 공짜가 아니지만, 테스트 바이너리의 수명 안에서 자식이 죽으면 함께 끝난다.
    /// 두 위험의 크기가 달라서 수동으로 남긴다.
    pub fn join(&mut self) {
        if let Some(handle) = self.drain.take() {
            // 이유: 배출 스레드의 패닉은 이 자리에서 할 수 있는 일이 없고, 정리 경로라
            // 되던지면 다른 정리(포트 파일·격리 홈 삭제)가 안 돈다.
            let _ = handle.join();
        }
    }
}

/// 오염된 락에서 복구한다. **에러를 무시하는 것이 아니다.**
///
/// 보호 대상은 링 버퍼와 `Option<Instant>` 한 칸뿐이라 패닉이 그 둘의 불변식을 깨지
/// 않는다. 반대로 오염을 남기면 **진단 수집기 자신이 죽어** 이후 실패의 stderr 꼬리가
/// 통째로 사라진다 — 고치는 쪽이 정보가 는다.
fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 인스턴스 핸들이 **서기 전**의 자식을 소유한다 — 그 사이에 패닉하면 자식을 죽이고 거둔다.
///
/// 하네스의 정리는 핸들(`TastyInstance` · `WebhookInstance` · `GuiTestInstance`)의 `Drop`
/// 에 있다. 그런데 부팅 대기(포트 파일 · 창 · 입력 장치)는 핸들을 만들기 **전**이고,
/// 그 구간의 패닉은 `Drop` 이 없는 맨 `Child` 를 떨어뜨린다. `Child` 의 `Drop` 은 죽이지
/// 않는다. 그러면 뒤늦게 부팅을 마친 인스턴스가 **시험이 끝난 뒤에도 산다.**
///
/// 그 고아가 무엇을 망가뜨리나: 레인은 idle 로 보이는데 CPU·메모리·디스플레이를 계속 쓰고,
/// 그것을 띄운 셸이나 배경 작업을 죽여도 안 잡힌다(부모가 이미 없다). 같은 기계의 다음
/// 회차 벽시계가 그만큼 편향된다.
///
/// 세 하네스 중 하나만 이 구간을 손으로 막고 있었다(상한 초과 갈래에서 직접 kill). 나머지
/// 둘은 같은 자리에서 그냥 패닉했다. 손으로 막으면 **갈래마다** 막아야 하고, 갈래가 늘면
/// 빠진 갈래가 조용하다 — 그래서 소유로 막는다: 핸들로 넘기는 [`Self::release`] 를
/// 거치지 않은 모든 경로가 여기서 회수된다.
pub struct ChildReaper {
    child: Option<std::process::Child>,
}

impl ChildReaper {
    pub fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }

    /// 대기 루프가 `try_wait` · `id` 를 부를 자리.
    pub fn child(&mut self) -> &mut std::process::Child {
        self.child
            .as_mut()
            .expect("release 뒤에는 이 값이 없다 — release 는 self 를 소비한다")
    }

    /// 부팅이 끝나 핸들이 수명을 넘겨받는다. 이 뒤로는 핸들의 `Drop` 이 회수한다.
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
            // 이유: 되감기 중의 정리 경로다. 거두기 실패를 되던지면 이중 패닉으로 프로세스가
            // 중단되어 **원래 실패 문구(stderr 꼬리)가 안 나간다.** kill 이 이미 보냈으므로
            // 거두지 못해도 자식은 죽는다.
            let _ = child.wait();
        }
    }
}

/// 자식과 그 아래(셸)를 죽인다. Windows 는 `Child::kill` 이 부모만 죽여 셸이 남으므로
/// 트리째 죽인다.
pub fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(target_os = "windows")]
    {
        // 이유: 이미 끝난 pid 면 taskkill 이 실패로 끝난다 — 목적(살아 있으면 죽인다)은
        // 어느 쪽이든 달성된다.
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child.id().to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 이유: 이미 끝난 자식이면 `InvalidInput` 이다 — 목적은 같다.
        let _ = child.kill();
    }
}

/// `Command::spawn()` 대신 이것을 쓴다. Linux 에서는 **이 테스트 바이너리가 어떤 이유로든
/// (SIGKILL 포함) 즉사하면** 커널이 자식을 대신 죽인다. 다른 OS 는 그냥 `spawn` 이다.
///
/// 하네스의 `Drop`·atexit 은 바이너리가 살아서 끝날 때만 돈다. 배경 작업이 완주 전에
/// 죽거나 사람이 회차를 끊으면 둘 다 안 돌고, 자식은 init 으로 넘어가 **영구히 남는다.**
///
/// 그냥 `pre_exec` 로 `PR_SET_PDEATHSIG` 를 걸면 **호출한 스레드**에 묶인다(`man 2 prctl`:
/// "the parent ... is considered to be the thread that created this process"). 공유
/// 인스턴스의 최초 호출자는 곧 끝나는 cargo test 워커 스레드라, 그 스레드가 끝나는 순간
/// 공유 인스턴스가 죽고 나머지 시험이 전부 "Connection reset" 으로 깨진다(실측: 이 함수
/// 없이 naive 하게 걸었을 때 `attach_git_query_loopback`/`shared_instance_harness` 가 바로
/// 그 증상으로 실패). 그래서 fork 자체를 **프로세스 수명 동안 파킹만 하는 전용 스레드**에서
/// 실행해 커널이 추적하는 "부모 스레드" 를 프로세스 수명과 맞춘다.
///
/// 이 함수는 한때 `tests/common` 과 `tests/webhook_common` 에 한 벌씩 있었고
/// `tests/gui_common` 에는 없었다 — 셋 중 GUI 를 띄우는 쪽만 바이너리가 죽을 때 창이 남았다.
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
            // fork 와 위 prctl 호출 사이에 부모(anchor 스레드)가 이미 죽었을
            // 경우의 레이스 — 그 경우 death 이벤트 자체가 이미 지나가버려
            // 시그널이 오지 않는다. getppid()==1 이면 이미 init 으로
            // reparent 된 것이므로 직접 자결한다.
            // SAFETY: 인자가 없는 async-signal-safe 시스템 콜이다.
            if unsafe { libc::getppid() } == 1 {
                return Err(std::io::Error::other("parent already gone before exec"));
            }
            Ok(())
        };
        // SAFETY: 클로저는 fork 이후 exec 이전, 자식 프로세스 단독 스레드에서
        // 실행된다. 안에서 부르는 것은 위 두 시스템 콜뿐이라 pre_exec 의 제약을 지킨다.
        unsafe {
            command.pre_exec(bind_to_parent);
        }

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("tasty-test-fork-anchor".into())
            .spawn(move || {
                let result = command.spawn();
                // 이유: 받는 쪽은 바로 아래 `recv` 이고, 그쪽이 없으면 보고할 자리도 없다.
                let _ = tx.send(result);
                // 절대 반환하지 않는다 — 반환/종료하는 순간이 곧 위 prctl 이
                // 추적하는 "부모 스레드 종료" 라, 그 즉시 방금 띄운 자식이 죽는다.
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

/// stderr 시그니처로 가릴 수 있는 것. **두 갈래의 확신 수준이 다르다.**
enum BootBlocker {
    /// 디스플레이 서버가 아예 없다 — winit 이 즉시 죽는다. 이 시그니처는 부팅에
    /// 성공한 인스턴스의 stderr 에는 **나오지 않으므로**(실측) 단독으로 원인이 된다.
    NoDisplay(&'static str),
    /// GPU 드라이버가 가속 경로를 못 잡고 폴백했다. **원인 판정이 아니다** —
    /// 아래 [`GPU_FALLBACK_MARKERS`] 의 주석 참조.
    GpuFallback(&'static str),
}

/// GPU 드라이버가 **가속 경로를 포기하고 폴백할 때** 스택이 남기는 문자열들.
///
/// 이름이 `..._FALLBACK_...` 인 것이 핵심이다. 이 줄들은 부팅 실패의 증거가 아니다 —
/// mesa/turnip 이 `/dev/dri/renderD128` 을 못 열고 소프트웨어 경로로 내려가는 **정상
/// 절차**의 흔적이고, 그렇게 내려간 뒤 부팅은 대개 성공한다. 실측(2026-09-04, 이
/// 개발 머신): port file 을 정상적으로 쓴 인스턴스의 stderr 에 여섯 개가 **전부** 있었다.
///
/// 그래서 이 마커는 "GPU 때문이다" 를 단정하는 데 쓸 수 없고, 판정문도 단정하지
/// 않는다([`boot_blocker_verdict`]). 정상 부팅에 안 나오는 GPU 시그니처를 대신 쓰고
/// 싶었으나, 이 머신에서는 GPU 초기화가 **항상** 소프트웨어 폴백으로 성공해서 그런
/// 로그를 채집할 수 없었다. 그런 시그니처를 실제로 관측하면 이 목록을 그것으로
/// 바꾸고 판정문의 단정을 되살릴 수 있다.
const GPU_FALLBACK_MARKERS: &[&str] = &[
    "renderD128", // DRM 렌더 노드 — 열지 못했다(점유 중이거나 접근 불가)
    "VK_ERROR_",  // Vulkan 초기화 실패 전반
    "DRI3",       // X 서버가 DRI3 를 못 주는 경우(가속 경로 상실)
    "libEGL",     // EGL 경고 — 위 둘과 함께 나오는 것이 보통
    "tu_knl",     // mesa/turnip 커널 인터페이스 오류
    "failed to open device",
];

/// 디스플레이 서버 부재를 가리키는 문자열들. 실측으로 정상 부팅 stderr 에는 없고
/// `DISPLAY`/`WAYLAND_*` 를 모두 지운 부팅에서만 나온다.
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

/// stderr tail 에서 실패 원인의 단서를 찾아 한 줄로 만든다.
///
/// 이 줄이 없으면 "느린 건지 환경이 막힌 건지" 를 매번 사람이 stderr 을 읽어 판정해야
/// 한다. 다만 **단서의 강도가 다르면 문장의 강도도 달라야 한다** — 확신에 찬 오답은
/// 조용한 타임아웃보다 나쁘다. 디스플레이 부재는 단정하고, GPU 폴백은 단정하지 않는다.
pub fn boot_blocker_verdict(stderr_tail: &str) -> Option<String> {
    match detect_blocker(stderr_tail)? {
        BootBlocker::NoDisplay(marker) => Some(format!(
            "디스플레이 서버가 없다 — 코드 인과가 아니다(시그니처: `{marker}`). 이 하네스는 실제 \
             GUI 를 띄우므로 `xvfb-run -a` 같은 디스플레이 위에서 돌려야 한다."
        )),
        BootBlocker::GpuFallback(marker) => Some(format!(
            "GPU 가속 경로 폴백 흔적이 있다(시그니처: `{marker}`) — **이것만으로는 원인 판정이 \
             되지 않는다.** 같은 줄이 부팅에 성공한 인스턴스의 stderr 에도 그대로 나온다. \
             먼저 다른 워크트리·인스턴스가 같은 GPU 디바이스를 쓰고 있는지 확인하고, 그것이 \
             아니면 코드 쪽(부팅 지연·plugin·셸 설정)을 그대로 본다."
        )),
    }
}

fn verdict_or_default(tail: &str, fallback: &str) -> String {
    if let Some(verdict) = boot_blocker_verdict(tail) {
        return verdict;
    }
    // ★ **빈 tail 은 "시그니처가 없다" 와 다른 세계다.** 폴백 문구는 둘 다
    // ("stderr 에 내용이 있었는데 안 걸렸다" 와 "stderr 이 아예 없었다") 에 쓰이는데,
    // 두 세계의 처방이 반대다 — 앞은 "부팅 지연·설정을 본다"(들어와서 느리다), 뒤는
    // "부팅에 들어가지도 못한 쪽을 본다". 한 메시지에 둘이 같이 실리면 서로를 지운다.
    //
    // 이 모듈의 존재 이유가 느린 것과 멈춘 것을 가르는 것인데, 그 아래 침묵 판정이
    // 가른 것을 이 줄이 도로 흐리고 있었다. 그래서 여기서는 **주장하지 않는다.**
    if tail.trim().is_empty() {
        return "stderr 이 비어 있다 — 시그니처가 없는 것이 아니라 볼 것이 없다.".to_string();
    }
    fallback.to_string()
}

/// 자식이 이미 죽었을 때의 panic 메시지. 상한을 다 기다릴 이유가 없는 경우다 —
/// 부팅 실패는 대부분 즉사(디스플레이 없음·설정 오류)라, 이 경로가 실제 대기 시간을
/// 수십 초에서 1 초 미만으로 줄인다.
pub fn early_exit_message(status: &str, tail_lines: usize, tail: &str) -> String {
    let verdict = verdict_or_default(tail, "stderr 의 마지막 오류를 그대로 읽는다.");
    format!(
        "tasty 프로세스가 부팅 중 종료했다 ({status}) — 상한을 기다리지 않고 즉시 실패시킨다.\n{verdict}\n--- stderr (last {tail_lines} lines) ---\n{tail}"
    )
}

/// spawn timeout panic 메시지. 단계 이름·상한·판정·stderr tail 을 한 형식으로 묶어
/// 두 하네스가 같은 모양으로 실패하게 한다.
/// **느린 것과 멈춘 것을 가른다.** 상한을 넘긴 부팅은 두 사건일 수 있다: 자식이 마감
/// 직전까지 진행 중이었거나(예산 부족 — 상한이 얇다), 한참 전부터 아무 말도 없었거나
/// (멈춤 — 상한을 올려도 그대로다). 종전 문구는 둘 다 `failed to start within 40s` 였다.
///
/// 가르는 값은 **마지막 stderr 이후 경과**다. 문턱은 상한의 절반 — 그 정도 침묵이면
/// 남은 예산을 더 줘도 같은 자리에 서 있을 것이라는 뜻이다. 순수 함수라 아래 단위
/// 테스트가 세 방향을 다 찌른다.
pub fn stderr_silence_verdict(last_line_age: Option<Duration>, limit: Duration) -> String {
    match last_line_age {
        None => "자식이 stderr 에 한 줄도 내지 않았다 — 부팅에 들어가지도 못한 쪽을 먼저 본다."
            .to_string(),
        Some(age) if age * 2 >= limit => format!(
            "마지막 stderr 이후 {age:?} 조용했다 — 느린 것이 아니라 멈춘 쪽이다. \
             상한 인상은 이 사건의 처방이 아니다."
        ),
        Some(age) => format!(
            "마지막 stderr 이 {age:?} 전이다 — 마감 직전까지 진행 중이었다. \
             예산 부족 쪽이라, 무엇이 그 시간을 쓰는지를 재라."
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
        "부팅 차단 시그니처는 없다 — 부팅 지연이나 설정 경로를 본다.",
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

    /// 지어진 것이 없으면 **무엇을 지을지** 말한다.
    ///
    /// 이 진단이 없던 동안 같은 상태가 `-32601 Method not found` 로만 나왔고, 그것은
    /// 메서드가 사라진 것과 문구가 같아 두 회차 동안 회귀로 오독됐다.
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

    /// **양방향으로 본다** — 늘 진단을 내는 것이면 위 시험은 아무것도 안 재는 것이다.
    ///
    /// 음성 대조를 `tasty-` 로 시작하되 plugin 이 아닌 이름으로 둔다. 접두사가 실제로
    /// 가르는지를 재는 자리라, 여기서 아무 이름이나 쓰면 "파일이 하나라도 있으면 조용"
    /// 으로도 통과한다.
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

    /// 명부 밖 스위트에는 이 처방이 **틀린 처방**이다 — 걔들은 빈 번들로 뜨는 것이 정상이다.
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

    /// **조합 의존 단언을 가진 스위트는 override 를 안 받는다** — 이 설계의 안전장치
    /// 전부가 이 한 줄이다. 무너지면 데몬만 조합이 바뀌어 `..._answers_in_both_combos`
    /// 계열의 단언이 구조적으로 뒤집힌다(함정 4).
    ///
    /// **양방향으로 본다** — 무시하는 쪽만 보면 "전부 무시" 도 통과한다.
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

    /// 낡은 override 바이너리를 잡는가. **이 판정이 죽으면 아무 소리도 안 난다** —
    /// 낡은 데몬은 정상 부팅해 정상 응답하고 스위트는 옛 코드에 대해 판정한다.
    ///
    /// 양방향: 새 소스가 있으면 잡고, 없으면 안 잡는다. 그리고 `.rs` 가 아닌 새 파일은
    /// 데몬 동작을 안 바꾸므로 잡지 않는다 — 그것까지 잡으면 문서만 고쳐도 빨개진다.
    /// 판정 기준으로 쓰는 **알려진 값**. 1970 + 1e6 초 = 1970-01-12.
    ///
    /// 시험이 "어느 쪽이 새것인가" 를 물을 때 두 파일을 **지금** 으로 쓰고 견주면
    /// 파일시스템 눈금에 의존한다 — 같은 눈금에 들어가면 나노초까지 같아진다. 대신
    /// 양쪽을 이 값으로 찍고 필요한 쪽만 옮기면 눈금 크기와 무관해진다.
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

    /// 탐침 디렉토리. 같은 프로세스에서 여러 시험이 쓰므로 이름에 용도를 넣는다 —
    /// 하나로 공유하면 한 시험의 정리가 다른 시험의 파일을 지운다.
    ///
    /// 손으로 만들고 손으로 지우던 것을 [`Scratch`] 로 바꿨다. 손 정리는 마지막 줄에
    /// 있어 **성공 경로에서만** 돌고, 정작 디렉토리에 볼 것이 남는 패닉 경로에서 안
    /// 돈다 — 그래서 잔여가 이 저장소의 `/tmp` 에 하루를 넘겨 쌓여 있었다(실측
    /// 2026-09-08, 이 lane 의 base `b134d28e3` 트리: `*-probe-*` 꼴 11 개, 빈 것 0 개).
    /// 유일화 성분과 그 근거는 `tasty_doc_guards::temp_scratch` 에 있다.
    use tasty_doc_guards::temp_scratch::Scratch;

    /// 시계 대신 **스탬프**로 앞뒤를 만든다.
    ///
    /// 전에는 `sleep(20 ms)` 로 간격을 벌렸다. 그것은 눈금이 잘게 나뉜 기계에서만
    /// 맞는다 — 눈금이 1 초인 파일시스템에서는 20 ms 뒤에 쓴 파일이 바이너리와 **같은
    /// 값**이 되고, 그러면 가운데 단정(`.md` 는 안 센다)이 **이유가 바뀐 채 통과한다**:
    /// 확장자로 걸러서가 아니라 안 새것이라서 `None` 이 된다. 거짓 초록이다.
    /// 지금은 `.md` 를 바이너리보다 **확실히 새것으로 찍으므로**, 확장자 거름이 무너지면
    /// 그 자리가 빨개진다.
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
            "`.rs` 가 아닌 파일은 데몬 동작을 안 바꾼다 — 이것까지 잡으면 문서만 고쳐도 \
             빨개진다"
        );

        let app = src.join("app.rs");
        std::fs::write(&app, b"fn main() {}").expect("소스를 쓸 수 있어야 한다");
        stamp(&app, newer);
        assert_eq!(
            source_newer_than(&bin, vec![src.clone()]).as_deref(),
            Some(app.as_path()),
            "바이너리보다 새로운 `.rs` 가 있으면 그 경로를 대야 한다"
        );

        // 탐침 디렉토리는 판정에 안 쓰이므로 정리 실패를 무시한다 — 남아도 temp 이고,
    }

    /// 소스와 바이너리가 **같은 눈금**에 떨어져도 봐야 한다.
    ///
    /// 고치기 전 이 자리는 `left: None` 이었다 — 엄격 초과(`>`)가 동률을 "안 낡았다" 로
    /// 흡수했다. 그 흡수는 **거짓 음성이자 곧 거짓 초록**이다: 낡은 데몬은 정상 부팅해
    /// 정상 응답하므로 그 스위트는 옛 코드에 대해 통과하거나 실패하고, 그 오진은 양방향이다.
    ///
    /// 시계에 안 기댄다 — 두 파일을 **같은 알려진 값**으로 찍는다. `sleep` 으로 간격을
    /// 벌리는 완화와 다르다. 그쪽은 눈금 크기를 모르는 채 고른 수라 눈금이 더 거친
    /// 기계에서 다시 뒤집힌다.
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

        // 음성 대조: 바이너리를 한 눈금 뒤로 보내면 안 걸려야 한다. 이것이 없으면
        // 위 단정은 "무엇이든 걸린다" 로도 통과한다.
        stamp(&bin, known_stamp() + std::time::Duration::from_secs(1));
        assert_eq!(
            source_newer_than(&bin, vec![src.clone()]),
            None,
            "바이너리가 더 새것이면 낡지 않았다"
        );
    }

    #[test]
    fn an_empty_override_means_the_default_not_an_empty_path() {
        // 셸에서 `TASTY_E2E_BIN=` 로 비우는 것은 "기본으로 되돌린다" 로 읽힌다.
        // 빈 경로를 그대로 spawn 하면 원인을 알 수 없는 실패가 된다.
        let picked = resolve_instance_bin(Some(std::ffi::OsStr::new("")), "/built/by/cargo");
        assert_eq!(picked, std::ffi::OsString::from("/built/by/cargo"));
    }

    /// 2026-09-04 실측 로그 — **부팅에 성공한** 인스턴스의 stderr 이다(port file 작성
    /// 확인, port=43499). 처음에는 이것을 "워크트리 4 곳이 GPU 를 경합한 증거" 로 읽고
    /// 판정문이 "코드 인과가 아니다" 를 단정했는데, 같은 세 줄이 아무 경합 없이 단독으로
    /// 띄운 정상 부팅에도 한 글자 다르지 않게 나온다. turnip 이 `/dev/dri/renderD128` 을
    /// 못 열고 소프트웨어로 내려가는 드라이버 폴백 상용구다.
    const SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL: &str = "\
libEGL warning: DRI3 error: Could not get DRI3 device
libEGL warning: Ensure your X server supports DRI3 to get accelerated rendering
TU: error: ../src/freedreno/vulkan/tu_knl.cc:387: failed to open device /dev/dri/renderD128 (VK_ERROR_INCOMPATIBLE_DRIVER)";

    /// 이 tail 로는 **코드를 무죄로 만들 수 없다.** 정상 부팅에도 그대로 나오는 줄이라,
    /// 이걸로 "코드 인과가 아니다" 를 말하면 어떤 원인의 타임아웃이든 전부 GPU 탓이 된다.
    #[test]
    fn a_gpu_fallback_tail_does_not_rule_out_code() {
        let verdict = boot_blocker_verdict(SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL)
            .expect("단서는 실어야 한다 — 다만 단정하지 않는다");
        assert!(
            !verdict.contains("코드 인과가 아니다"),
            "정상 부팅에도 나오는 시그니처로 코드를 무죄 판정하면 안 된다: {verdict}"
        );
        assert!(
            verdict.contains("원인 판정이 되지 않는다"),
            "단정하지 않는다는 사실을 문장에 실어야 한다: {verdict}"
        );
        assert!(
            verdict.contains("다른 워크트리"),
            "먼저 확인할 것을 알려야 한다: {verdict}"
        );
    }

    /// 디스플레이 부재는 반대다 — 정상 부팅 stderr 에는 나오지 않으므로(실측) 단정한다.
    /// 두 갈래의 확신 수준이 실제로 다르다는 것을 여기서 고정한다.
    #[test]
    fn the_two_verdicts_do_not_carry_the_same_certainty() {
        let display = boot_blocker_verdict(
            "Error: neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set.",
        )
        .expect("디스플레이 부재는 판정 대상이다");
        let gpu = boot_blocker_verdict(SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL).expect("단서는 실린다");
        assert!(display.contains("코드 인과가 아니다"), "{display}");
        assert!(!gpu.contains("코드 인과가 아니다"), "{gpu}");
    }

    #[test]
    fn missing_display_is_reported_as_display_not_gpu() {
        let tail = "Error: os error at winit/src/platform_impl/linux/mod.rs:765: \
                    neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set.";
        let verdict = boot_blocker_verdict(tail).expect("디스플레이 부재도 판정 대상이다");
        assert!(verdict.contains("디스플레이 서버가 없다"), "{verdict}");
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
        assert!(msg.contains("부팅 차단 시그니처는 없다"), "{msg}");
    }

    /// ★ 빈 stderr 에 대고 "시그니처가 없다" 를 말하면, 바로 아래 침묵 판정("한 줄도
    /// 내지 않았다 — 부팅에 들어가지도 못한 쪽을 먼저 본다")과 **반대 방향을 가리킨다.**
    /// 한 메시지가 두 처방을 순서 없이 싣는 것이 이 모듈이 없애려는 실패 형태다.
    #[test]
    fn an_empty_tail_is_not_reported_as_an_absent_signature() {
        let empty =
            spawn_timeout_message("tasty failed to start", SPAWN_PORT_TIMEOUT, 30, "", None);
        assert!(
            !empty.contains("부팅 차단 시그니처는 없다"),
            "stderr 이 비었는데 시그니처 부재를 주장한다 — 아래 침묵 판정과 반대를 가리킨다: {empty}"
        );
        assert!(empty.contains("볼 것이 없다"), "{empty}");
        // 침묵 판정은 제 자리를 지킨다 — 이 줄이 그 세계를 소유한다.
        assert!(empty.contains("한 줄도 내지 않았다"), "{empty}");

        // ★ 반대 방향 — 내용이 **있는데** 안 걸리는 tail 은 여전히 시그니처 부재를 말해야
        // 한다. 이게 없으면 위 초록은 "그 문장을 통째로 지웠다" 로도 설명된다.
        let noisy = spawn_timeout_message(
            "tasty failed to start",
            SPAWN_PORT_TIMEOUT,
            30,
            "INFO tasty: plugin discovery finished",
            Some(Duration::from_millis(200)),
        );
        assert!(
            noisy.contains("부팅 차단 시그니처는 없다"),
            "내용이 있는 tail 에서는 시그니처 부재가 여전히 정보다: {noisy}"
        );
        assert!(!noisy.contains("볼 것이 없다"), "{noisy}");
    }

    /// ★★ 두 마커 계열이 **함께** 나오는 것이 예외가 아니라 통상이다 — 디스플레이가 없는
    /// 부팅도 `libEGL`/`DRI3` 경고를 그대로 뱉는다. 그때 `detect_blocker` 의 검사 순서가
    /// 판정의 **확신 수준**을 정한다: 앞쪽은 단정하고("코드 인과가 아니다") 뒤쪽은
    /// 단정하지 않는다. 그런데 그 순서를 지키는 단정이 **하나도 없었다** — 기존 시험의
    /// 디스플레이 tail 에 GPU 마커가 없어서, 순서를 뒤집어도 전부 초록이었다.
    #[test]
    fn a_display_failure_that_also_logs_gpu_noise_is_still_a_display_failure() {
        let both = format!(
            "{SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL}\nError: neither WAYLAND_DISPLAY nor \
             WAYLAND_SOCKET nor DISPLAY is set."
        );
        let verdict = boot_blocker_verdict(&both).expect("둘 다 있으면 판정 대상이다");
        assert!(
            verdict.contains("디스플레이 서버가 없다"),
            "GPU 잡음이 섞였다고 디스플레이 부재가 강등되면 안 된다: {verdict}"
        );
        assert!(
            !verdict.contains("원인 판정이 되지 않는다"),
            "확신 수준이 낮은 쪽 문장이 나왔다 — 검사 순서가 뒤집혔다: {verdict}"
        );

        // ★ 반대 방향 — GPU 마커만 있으면 디스플레이를 말하면 안 된다. 위 단정이
        // "무엇이든 디스플레이라고 한다" 로 통과하는 것을 막는다.
        let gpu_only =
            boot_blocker_verdict(SUCCESSFUL_BOOT_GPU_FALLBACK_TAIL).expect("단서는 실린다");
        assert!(!gpu_only.contains("디스플레이 서버가 없다"), "{gpu_only}");
    }

    /// ★★ **죽은 자식과 멈춘 자식의 처방은 반대다** — 그런데 그것을 지키는 단정이
    /// 하나도 없었다. `early_exit_message` 를 부르는 시험이 이 모듈에 **0 건**이었고,
    /// 실측으로 확인했다: 이 함수의 폴백을 `spawn_timeout_message` 의 폴백으로 바꾸는
    /// 변이가 **20/0 으로 살아남았다.** 그 변이의 결과는 이미 죽은 프로세스에게
    /// "부팅 지연이나 설정 경로를 본다" 고 말하는 것이다 — 기다릴 부팅이 없는데
    /// 기다리는 쪽을 보라고 한다.
    ///
    /// 두 함수가 `verdict_or_default` 를 공유해서 **덮인 것처럼 보였던 것**이 이 구멍의
    /// 생김새다. 공유 부분은 세 시험이 찌르고 있었고, 각자 고르는 **폴백**만 아무도
    /// 안 봤다. 그런데 두 세계를 가르는 것이 정확히 그 폴백이다.
    #[test]
    fn a_dead_child_and_a_stalled_one_do_not_get_the_same_prescription() {
        // 내용은 있는데 차단 시그니처는 없는 tail — 폴백이 실제로 선택되는 국면이다.
        let unremarkable = "INFO tasty: plugin discovery finished";

        let died = early_exit_message("exit status: 1", 30, unremarkable);
        assert!(died.contains("부팅 중 종료했다"), "{died}");
        assert!(died.contains("상한을 기다리지 않고"), "{died}");
        assert!(
            died.contains("마지막 오류를 그대로 읽는다"),
            "죽은 자식에게는 남은 stderr 을 읽으라고 해야 한다: {died}"
        );
        assert!(
            !died.contains("부팅 지연"),
            "이미 끝난 프로세스에게 지연을 보라고 한다 — 기다릴 부팅이 없다: {died}"
        );

        // ★ 반대 방향 — 같은 tail 로 상한을 넘긴 쪽은 정확히 반대 문장을 내야 한다.
        // 이 짝이 없으면 위 초록은 "두 문장을 다 지웠다" 로도 통과한다.
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
            "아직 살아 있는 자식인데 유언을 읽으라고 한다: {stalled}"
        );

        // 빈 tail 은 죽은 쪽에서도 "시그니처가 없다" 가 아니다 — 볼 것이 없는 것이다.
        let died_silent = early_exit_message("signal: 9", 30, "   \n  ");
        assert!(died_silent.contains("볼 것이 없다"), "{died_silent}");
        assert!(
            !died_silent.contains("마지막 오류를 그대로 읽는다"),
            "읽을 stderr 이 없는데 읽으라고 한다: {died_silent}"
        );

        // ★ 폴백이 항상 이기지는 않는다 — 시그니처가 있으면 그 판정이 실린다.
        let died_with_signature = early_exit_message(
            "exit status: 1",
            30,
            "Error: neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set.",
        );
        assert!(
            died_with_signature.contains("디스플레이 서버가 없다"),
            "{died_with_signature}"
        );
        assert!(
            !died_with_signature.contains("마지막 오류를 그대로 읽는다"),
            "판정이 있는데 폴백이 덮었다: {died_with_signature}"
        );
    }

    /// 세 갈래가 서로 다른 문장을 내고, **남의 문장을 안 낸다**(양방향).
    #[test]
    fn the_silence_verdict_separates_stalled_from_merely_slow() {
        let limit = Duration::from_secs(40);

        let none = stderr_silence_verdict(None, limit);
        assert!(none.contains("한 줄도 내지 않았다"), "{none}");

        let stalled = stderr_silence_verdict(Some(Duration::from_secs(30)), limit);
        assert!(stalled.contains("멈춘 쪽"), "{stalled}");
        assert!(stalled.contains("처방이 아니다"), "{stalled}");

        let slow = stderr_silence_verdict(Some(Duration::from_millis(200)), limit);
        assert!(slow.contains("예산 부족"), "{slow}");
        assert!(
            !slow.contains("멈춘 쪽") && !slow.contains("한 줄도"),
            "갈래가 안 갈렸다: {slow}"
        );

        // 문턱은 상한의 절반이다 — 경계 양쪽을 함께 박는다.
        assert!(stderr_silence_verdict(Some(limit / 2), limit).contains("멈춘 쪽"));
        assert!(
            stderr_silence_verdict(Some(limit / 2 - Duration::from_millis(1)), limit)
                .contains("예산 부족")
        );
    }

    #[test]
    fn both_harnesses_share_one_bound_for_the_same_stage() {
        // 값 자체보다 "두 하네스가 같은 상수를 본다" 는 사실이 중요하다. 이 모듈이
        // 유일한 정의 자리이므로, 어느 한쪽이 자기 값을 되살리면 여기 상수가 안 쓰여
        // dead_code 로 드러난다. 상한 순서(S1 > S2)만 여기서 고정한다.
        assert!(SPAWN_PORT_TIMEOUT > SPAWN_SHELL_TIMEOUT);
    }

    /// 오래 사는 자식. 회수기가 죽이지 않으면 시험보다 오래 산다.
    #[cfg(unix)]
    fn long_lived_child() -> std::process::Child {
        std::process::Command::new("sleep")
            .arg("60")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("오래 사는 자식을 못 띄웠다")
    }

    /// 그 pid 의 프로세스가 아직 있는가(좀비 포함 — 거두지 않았으면 있다).
    #[cfg(unix)]
    fn pid_exists(pid: u32) -> bool {
        // SAFETY: 시그널 0 은 아무것도 보내지 않고 존재·권한만 묻는다. 인자는 정수뿐이다.
        let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }

    /// 핸들을 만들기 전에 패닉하면 **자식은 죽고 거둬진다.**
    ///
    /// 하네스의 부팅 대기 갈래(상한 초과 · 입력 장치 실패 등)가 전부 이 경로다. 회수가
    /// 빠지면 pid 가 남고(거두지도 죽이지도 않음), kill 만 빠지면 `wait` 가 자식의 남은
    /// 수명만큼 막힌다 — 두 변이가 서로 다른 단정에서 죽도록 둘 다 잰다.
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
            "패닉 경로를 안 탔다 — 이 시험은 아무것도 안 쟀다"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "회수가 자식의 수명({:?})만큼 막혔다 — 죽이지 않고 기다리기만 했다",
            started.elapsed()
        );
        assert!(
            !pid_exists(pid),
            "핸들이 서기 전에 패닉했는데 자식(pid {pid})이 남았다 — 그 인스턴스는 시험이 \
             끝난 뒤에도 산다"
        );
    }

    /// 대조: 핸들로 넘긴 자식은 **살아 있다.** 이것이 없으면 위 시험은 "무조건 죽이는
    /// 회수기" 로도 통과한다.
    #[cfg(unix)]
    #[test]
    fn a_released_child_is_handed_over_alive() {
        let mut reaper = ChildReaper::new(long_lived_child());
        assert!(
            reaper.child().try_wait().expect("try_wait").is_none(),
            "넘기기 전에 이미 죽었다 — 대조 조건이 안 섰다"
        );
        let mut child = reaper.release();
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "release 가 자식을 죽였다 — 핸들이 넘겨받은 인스턴스가 즉사한다"
        );
        kill_process_tree(&mut child);
        child.wait().expect("대조 자식을 못 거뒀다");
    }

    /// 줄을 많이 뱉는 자식. 링 용량을 넘겨야 링이 도는지 볼 수 있다.
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

    /// 자식이 끝난 뒤 읽은 꼬리는 **약속한 줄 수와 마지막 줄을 가진다.**
    ///
    /// 이 계약이 필요한 이유는 하네스의 조기 종료 갈래다. `try_wait()` 가 `Some` 을
    /// 주는 순간 진단을 만드는데, 그때 배출 스레드는 아직 한 줄도 못 넣었을 수 있고
    /// 그러면 진단이 "stderr 이 비어 있다" 로 나가 **없는 원인을 가리킨다.** 실측
    /// 2026-09-08(부하 준 러너, 45 줄 쓰고 즉시 종료하는 가짜 데몬, 팔을 번갈아 25 회씩):
    /// 안 기다리는 형태가 **8/25** 에서 꼬리를 잃었고 기다리는 형태는 **25/25** 살렸다.
    ///
    /// ★ **이 시험은 그 결함을 못 본다 — 계약만 박는다.** 기다림을 없애는 변이를 걸고
    /// 부하를 준 채 20 회 돌렸더니 **0 회** 실패했다(2026-09-08). 경합은 `wait()` 와 읽기
    /// 사이에서 배출 스레드가 굶어야 나는데, 이 시험은 그 굶음을 만들 수단이 없다.
    /// 위 확률은 하네스를 통째로 돌린 A/B 에서 나온 값이고 여기서 재는 값이 아니다.
    /// 그래서 이 시험의 초록은 "그 경합이 없다" 가 아니라 **"끝난 뒤 읽으면 잘린 꼬리를
    /// 주지 않는다"** 만 뜻한다.
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
            "꼬리가 약속한 줄 수가 아니다 — 잘림 안내가 붙었으면 배출이 예산 안에 \
             안 끝난 것이다:\n{tail}"
        );
        assert_eq!(
            lines.last().copied(),
            Some(format!("line {emitted}").as_str()),
            "자식이 죽은 뒤 읽었는데 마지막 줄이 없다 — 배출을 안 기다린 것이다:\n{tail}"
        );
    }

    /// ★ 이 타입은 세 하네스가 그 위로 옮겨 탈 자리인데 **한 번도 안 돌았다.**
    /// 링이 실제로 돌고, 꼬리가 `tail_lines` 로 잘리고, 시각이 찍히는지를 잰다.
    #[test]
    fn the_capture_rings_at_capacity_and_shows_only_the_tail_it_promises() {
        let emitted = STDERR_RING_CAPACITY + 44;
        let mut child = child_that_prints_stderr_lines(emitted);
        let mut cap = StderrCapture::start(child.stderr.take(), 7);
        child.wait().expect("자식을 못 거뒀다");
        cap.join();

        let tail = cap.tail();
        let lines: Vec<&str> = tail.lines().collect();

        // ① 약속한 줄 수만 보여준다 — 링 용량(256)이 아니라 tail_lines(7).
        assert_eq!(
            lines.len(),
            cap.tail_lines(),
            "꼬리 줄 수가 약속과 다르다: {tail}"
        );

        // ② 링이 **돌았다** — 마지막 줄이 남고 첫 줄은 밀려났다. 이것이 없으면
        //    용량을 넘겼을 때 오래된 줄이 남는지 새 줄이 남는지 아무도 모른다.
        assert_eq!(
            lines.last().copied(),
            Some(format!("line {emitted}").as_str())
        );
        assert!(
            cap.find(|l| l == "line 1").is_none(),
            "용량을 {STDERR_RING_CAPACITY} 넘겨 {emitted} 줄을 넣었는데 첫 줄이 남아 있다"
        );
        // ③ 양성 대조 — `find` 가 늘 `None` 이라서 ②가 통과한 것이 아니다.
        assert!(cap.find(|l| l == format!("line {emitted}")).is_some());

        // ④ 시각이 찍힌다. 이 값이 없으면 침묵 판정이 통째로 무정보다.
        assert!(cap.last_line_age().is_some());
    }

    /// stderr 를 안 준 자식에서도 살아야 한다 — 실패 경로가 그대로 돌아야 하기 때문이다.
    #[test]
    fn a_capture_without_a_pipe_stays_empty_instead_of_dying() {
        let mut cap = StderrCapture::start(None, 9);
        assert_eq!(cap.tail(), "");
        assert_eq!(cap.last_line_age(), None);
        assert!(cap.find(|_| true).is_none());
        cap.join(); // 거둘 스레드가 없어도 막히지 않는다
    }

    /// ★★ [`StderrCapture::last_line_age`] 가 **메서드인 이유**를 여기서 잰다.
    ///
    /// 타입 doc 이 "가드를 `panic!` 인자 안에서 만들면 오염된다" 를 주장하는데, 그 주장은
    /// 지금까지 이 레포 **밖**에서만 측정됐다. 기전이 바뀌면(에디션·컴파일러) 주장만 남고
    /// 아무도 모른다. 그래서 두 형태를 여기서 **양방향으로** 고정한다.
    ///
    /// `StderrCapture` 자신으로는 못 잰다 — 내부 `lock()` 이 오염을 복구하므로 밖에서
    /// 관측되지 않는다. 그것이 이 시험이 지역 뮤텍스로 기전을 잡는 이유다.
    #[test]
    fn a_guard_born_inside_a_panic_argument_poisons_and_one_dropped_before_it_does_not() {
        // ① 인자 안에서 만든 가드 — statement 끝까지 살아 되감기 중에 Drop 된다.
        let inside = std::sync::Mutex::new(7u32);
        let hit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("나이 {:?}", *inside.lock().expect("첫 lock 은 성해야 한다"));
        }));
        assert!(hit.is_err(), "패닉이 안 났으면 아래 판정이 무정보다");
        assert!(
            inside.lock().is_err(),
            "인자 안의 가드가 오염을 안 만든다 — 그러면 last_line_age 를 메서드로 둔 근거가 사라진다"
        );

        // ② 값을 먼저 꺼내고 가드를 statement 밖에서 떨어뜨린다 = 메서드가 하는 일.
        let outside = std::sync::Mutex::new(7u32);
        let hit = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let v = *outside.lock().expect("첫 lock 은 성해야 한다");
            panic!("나이 {v:?}");
        }));
        assert!(hit.is_err());
        assert!(
            outside.lock().is_ok(),
            "가드를 먼저 떨어뜨렸는데도 오염됐다 — 처방이 안 듣는다는 뜻이다"
        );
    }

    /// 래치가 **실제로 두 번째를 막는가.** 막는 것이 이 타입의 전부인데 안 재고 있었다.
    #[test]
    fn the_latch_blocks_the_second_spawn_and_a_success_releases_it() {
        let latch = SpawnOnceLatch::new();

        // ① 첫 진입은 통과한다.
        latch.entering("시험용 하네스");
        // ② 성공을 안 알리면 두 번째 진입에서 죽는다 — 프로세스를 띄우기 **전에** 막는다.
        let blocked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            latch.entering("시험용 하네스");
        }));
        assert!(blocked.is_err(), "래치가 두 번째 진입을 안 막았다");

        // ③ 양성 대조 — 래치가 **늘** 막는 것이 아니다. 성공을 알리면 다시 열린다.
        //    이것이 없으면 ②는 "두 번째는 무조건 죽는다" 로도 설명되고, 그러면 성공한
        //    인스턴스를 쓰는 다음 호출까지 잘못 막힌다.
        let opened = SpawnOnceLatch::new();
        opened.entering("시험용 하네스");
        opened.succeeded();
        opened.entering("시험용 하네스"); // 안 죽어야 한다
    }

    // ───── 자식이 어느 디스플레이에 뜨는가 ─────

    /// 디스플레이를 안 쓰는 조합에서는 아무것도 정하지 않는다.
    ///
    /// 이 갈래가 없으면 헤드리스 완주가 쓰지도 않는 값을 요구받고 멈춘다 — 규칙이
    /// 실재하지 않는 위반에 처방을 내는 형태다.
    #[test]
    fn a_daemon_without_a_window_is_asked_for_no_display() {
        assert_eq!(
            display_policy(false, None),
            DisplayPolicy::NotRequired,
            "창을 안 만드는 조합에 디스플레이를 요구했다"
        );
        // 선언이 있어도 요구가 없으면 정하지 않는다 — 요구 여부가 먼저다.
        assert_eq!(
            display_policy(false, Some(":77")),
            DisplayPolicy::NotRequired
        );
    }

    /// 자기 화면을 쓰겠다는 선언은 오늘 동작을 그대로 남긴다.
    #[test]
    fn declaring_the_inherit_keeps_todays_behavior() {
        assert_eq!(
            display_policy(true, Some(DISPLAY_INHERIT)),
            DisplayPolicy::Inherit
        );
    }

    /// 지정한 값은 그대로 자식에게 간다.
    #[test]
    fn a_named_display_is_handed_to_the_child_verbatim() {
        assert_eq!(
            display_policy(true, Some(":77")),
            DisplayPolicy::Pin(":77".to_string())
        );
        // 셸이 흘린 공백은 값이 아니다.
        assert_eq!(
            display_policy(true, Some(" :77 ")),
            DisplayPolicy::Pin(":77".to_string())
        );
    }

    /// 빈 값은 미지정과 **같다** — 통과가 아니다.
    ///
    /// 빈 `DISPLAY` 를 자식에게 넘기면 winit 이 디스플레이 부재와 같은 자리에서 죽어
    /// "안 줬다" 와 "없다" 가 한 문구로 합쳐진다. 두 세계의 처방이 다르므로 여기서 가른다.
    #[test]
    fn an_empty_declaration_is_not_a_declaration() {
        assert_eq!(display_policy(true, None), DisplayPolicy::Unspecified);
        assert_eq!(display_policy(true, Some("")), DisplayPolicy::Unspecified);
        assert_eq!(
            display_policy(true, Some("   ")),
            DisplayPolicy::Unspecified
        );
    }

    /// 멈추는 문구가 **두 길을 다 찍는다.**
    ///
    /// 한 길만 찍으면 그것이 유일한 처방으로 읽힌다 — 전용 디스플레이만 찍으면 창을
    /// 눈으로 봐야 하는 작업이 막히고, 선언만 찍으면 모두가 선언해서 결함이 그대로 남는다.
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

    // ───── 번들 hardlink 미리 채우기 ─────

    fn write(p: &std::path::Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// 미리 채운 홈의 파일은 스냅숏과 **같은 inode** 여야 한다 — 복사로 떨어지면 부팅당 쓰기가
    /// 그대로 남는데, 내용은 같아 host 도 시험도 초록이다. 그 조용한 퇴행을 inode 로 잡는다.
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
        // 스냅숏은 번들과 끊겨 있어야 한다 — 번들 쪽 제자리 `cp` 가 홈에 번지지 않게.
        let src = std::fs::metadata(bundle.join("markdown/tasty-plugin-markdown")).unwrap();
        let snap = std::fs::metadata(snapshot.join("markdown/tasty-plugin-markdown")).unwrap();
        assert_ne!(src.ino(), snap.ino(), "스냅숏이 번들과 inode 를 공유한다");
        // 이름이 보이는 스냅숏만 남고 임시 디렉터리는 치워진다.
        let names: Vec<_> = std::fs::read_dir(&cache)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("key")]);
    }

    /// 서명은 번들이 바뀌면 바뀌어야 한다 — 안 바뀌면 낡은 스냅숏이 계속 걸리고, host 가
    /// 매 부팅 차이를 다시 복사한다(초록인 채 쓰기가 되살아난다).
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

    /// 지금 서명이 아닌 스냅숏은 지우고, 막 만들어지는 임시 디렉터리는 남긴다.
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

/// 번들 plugin 을 **실제로 호출하는** 테스트 바이너리.
///
/// 여기 없는 스위트는 빈 번들 루트로 부팅한다 — host 는 spec 마다 원본을 못 찾아
/// 조용히 건너뛰고, 격리 홈으로 가는 복사가 통째로 사라진다. 실측(2026-09-06,
/// `attach_silent_disconnect`, 번들만 바꾼 대조): 홈 최대 1148 MB → 4 MB.
///
/// **판정은 성질로, 저장은 이름으로 한다.** 이 명부는 바이너리 이름으로 매칭하지만
/// 무엇을 넣을지는 이름 모양이 아니라 *그 스위트가 plugin 네임스페이스를 호출하거나
/// plugin 이 뒷받침하는 surface 타입을 여는가* 로 정했다. 이름으로 판정하면 샌다 —
/// `explorer` 는 plugin 처럼 생겼지만 호스트 view 이고, `webhook.*` 는 권한 등급에
/// `plugin` 이 붙어 있지만 핸들러는 host 다. 같은 층 구분을 다른 자리에서 이름 붙인
/// 예가 `crates/tasty-doc-guards/tests/ci_channel_claims_match_workflows.rs` 에 있다.
///
/// **극성이 opt-in 인 것도 의도다.** 빠뜨리면 그 스위트의 plugin 호출이
/// `-32601 Method not found` 로 실패해 **그 자리에서 빨개진다**. 반대 극성(기본
/// 스테이징 + 예외 명부)은 명부가 낡아도 초록이라 비용만 조용히 자란다 — 이 비용이
/// 오래 안 보였던 이유가 정확히 그것이다.
///
/// 새 스위트가 plugin 을 쓰기 시작하면 여기 추가한다.
pub const SUITES_THAT_CALL_BUNDLED_PLUGINS: &[&str] = &[
    "attach_markdown_content_loopback",
    "e2e_tests",
    "plugin_disable_withdraws_surface_kinds",
    "soak_memory",
];

/// 지금 도는 테스트 바이너리 이름 (`.../deps/e2e_tests-1a2b3c4d` → `e2e_tests`).
///
/// **바이너리 단위여야 한다.** 공유 인스턴스는 프로세스당 `OnceLock` 이라
/// (`tests/common` 의 `shared()`) 테스트 함수 안에서 opt-in 을 부르면 먼저 도는
/// 테스트가 초기화를 가져가 경합한다. 같은 사실이 "테스트 N 개 = 인스턴스 1 개" 라
/// 위 실측의 1148 MB 도 부팅 하나의 값이다.
pub fn current_suite_name() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let stem = exe.file_stem()?.to_str()?;
    // cargo 가 붙이는 `-<hex>` 만 벗긴다. 스위트 이름 자체에 `-` 는 안 쓴다.
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

/// 번들 plugin 실행 파일 이름의 공통 머리. 좌변은 **이 한 값이다**.
const PLUGIN_BIN_PREFIX: &str = "tasty-plugin-";

/// 이 스위트가 번들 plugin 을 부른다고 선언했는가.
///
/// 두 소비처(`apply_bundle_opt_in` 의 갈래와 [`bundle_staging_note`])가 같은 물음을
/// 각자 쓰면 한쪽만 고쳐져도 조용하다 — 한 값에서 낸다.
fn suite_calls_bundled_plugins() -> bool {
    current_suite_name().is_some_and(|s| SUITES_THAT_CALL_BUNDLED_PLUGINS.contains(&s.as_str()))
}

/// 부를 번들 plugin 이 **하나도 안 지어져 있으면** 그 사실을 문장으로 낸다.
///
/// [`instance_bin`] 의 doc 이 적어 둔 **함정 3** 이 실제로 무는 자리다. 그 함정의 증상은
/// `-32601 Method not found` 이고, 그 문구는 메서드가 **사라진 것**과 글자 그대로 같다.
/// 실측 2026-09-08: `target/debug` 에 지어진 plugin 바이너리가 0 개인 트리에서
/// `cargo test --test e2e_tests` 는 44 통과 · 2 실패였고 그 둘은 `markdown.*` 를 부르는
/// 것들이었다. `cargo build -p tasty-plugin-markdown` 하나로 둘 다 초록이 됐다. 그때까지
/// 그 빨강은 두 회차 동안 "회귀인지 환경인지 미확정" 으로 남아 있었다.
///
/// **명부를 안 베낀다.** 제품의 builtin 명부를 여기 옮겨 적으면 그 사본이 조용히 갈리고,
/// 갈린 사본은 "안 지어졌다" 를 "명부에 없다" 로 오독하게 만든다. 좌변은 이름 하나다 —
/// [`PLUGIN_BIN_PREFIX`] 로 시작하는 실행 파일이 exe 옆에 있는가.
///
/// **부분 스테이징은 안 잡는다 — 선언된 사각이다.** 아홉 중 여덟만 지어진 상태는 이
/// 판정을 통과한다. 그것까지 잡으려면 어느 plugin 이 필요한지 알아야 하고 그것이 곧 위의
/// 사본이다. cargo 가 실제로 만드는 상태는 0(루트 패키지만 짓는 조합) 아니면 전부
/// (`--workspace`)라, 이 사각이 실물이 되는 경로는 손으로 지운 경우뿐이다.
///
/// 검증의 전제를 산문이 아니라 실패 문구에 싣는 결정의 근거·대안:
/// `docs/adr/0217-a-precondition-lives-in-the-failure-text-not-in-prose.md`.
fn staged_bundle_note(exe_dir: &std::path::Path, opted_in: bool) -> Option<String> {
    if !opted_in {
        return None;
    }
    let staged = std::fs::read_dir(exe_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    // `.d` 는 cargo 가 같은 이름으로 남기는 의존 목록이라 실행 파일이 아니다.
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
        "\n★ 이 스위트는 번들 plugin 을 부른다고 선언했는데(`SUITES_THAT_CALL_BUNDLED_PLUGINS`), \
         {} 에 지어진 plugin 바이너리가 0 개다.\n\
        \x20 그 상태에서 plugin namespace 호출은 `-32601 Method not found` 로 떨어지고, 그 문구는 \
         메서드가 **사라진 것**과 같다 — 위 실패가 코드 회귀인지 이 상태인지는 바이너리를 \
         지어 본 뒤에야 갈린다.\n\
        \x20 ★ 시험을 지우거나 명부에서 빼서 통과시키지 마라. `cargo test --test <스위트>` 는 \
         plugin 바이너리를 **안 짓는다**. 지어라:\n\
        \x20     cargo build --workspace\n",
        exe_dir.display()
    ))
}

/// 지금 도는 스위트 기준의 [`staged_bundle_note`]. 실패 문구 끝에 그대로 이어 붙인다.
///
/// 진단을 spawn 이 아니라 **실패 자리**에 붙이는 것이 의도다. spawn 에서 죽이면 plugin 을
/// 안 쓰는 나머지 시험들까지 같이 빨개져, 참인 초록 44 개가 사라진다.
pub fn bundle_staging_note() -> String {
    // 바이너리를 정하는 자리는 하나다 — `tests/e2e_single_instance_guard.rs` 가
    // `CARGO_BIN_EXE_tasty` 를 직접 부르는 자리를 세어 그것을 지킨다.
    std::path::PathBuf::from(instance_bin())
        .parent()
        .and_then(|dir| staged_bundle_note(dir, suite_calls_bundled_plugins()))
        .unwrap_or_default()
}

/// [`apply_os_open_record`] 가 `home` 아래에 만드는 기록 파일 이름.
pub const OS_OPEN_LOG_FILE: &str = "os-open.log";

/// 자식이 OS 열기(브라우저 · 파일 관리자)를 **띄우지 않고** `<home>/`[`OS_OPEN_LOG_FILE`] 에 기록만
/// 하게 한다. 돌려주는 경로가 그 기록 파일이다.
///
/// 격리 홈도 전용 디스플레이도 이 축을 못 막는다 — 브라우저는 이미 떠 있는 자기 인스턴스에
/// URL 을 넘기는 원격 제어 채널을 가져서, 시험 인스턴스가 연 것이 **실행자의 브라우저 탭**
/// 으로 나타난다(ADR-0511). 스위치는 제품의 debug 격리라 release 로 지은 자식은 무시한다
/// — 그래서 이름을 문자열로 옮겨 적지 않고 제품 상수를 그대로 쓴다.
///
/// 스위치는 host 프로세스 안의 OS 열기만 덮는다(번들 markdown 의 외부 링크는 host
/// `webview.open_external` 을 거쳐 그 안이다 — ADR-0527). 자식이 띄운 다른 프로세스(PTY 셸 · 스스로
/// 여는 plugin)의 열기는 가짜 `BROWSER`([`apply_fake_browser`])가 막는다 — 같은 기록 파일에
/// `BROWSER\t<인자>` 로 남는다.
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

/// [`apply_fake_browser`] 가 `home` 아래에 쓰는 가짜 브라우저 스크립트 이름.
pub const FAKE_BROWSER_FILE: &str = "os-open-browser.sh";

/// 자식(과 그 자식인 plugin)의 `BROWSER` 를 받은 인자를 [`OS_OPEN_LOG_FILE`] 에 적기만 하는
/// 스크립트로 준다. 아무것도 열지 않는다.
///
/// `webbrowser` 는 unix(macOS 제외)에서 `BROWSER` 를 먼저 보고, 그 명령이 성공하면 거기서
/// 멈춘다 — 그래서 여기서 막히는 것은 **Linux·BSD 의 `webbrowser` 경로뿐**이다. macOS·Windows
/// 의 자식 프로세스 쪽 열기는 이것으로 안 막힌다(ADR-0511). 빌드 프로필과 무관하게 준다 — 제품
/// 스위치가 없는 release 자식도 Linux 에서는 이것으로 막힌다.
///
/// 빈 `BROWSER=` 는 막지 않는다 — `webbrowser` 가 빈 항목을 건너뛰고 xdg desktop entry 를
/// 직접 실행한다. 그래서 경로가 `BROWSER` 문법(공백·`:` 로 가른다)에 안 맞으면 `true` 로
/// 떨어진다 — 기록은 없지만 여전히 아무것도 안 연다. 스크립트를 못 쓰면 하네스가 선다.
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
        // `webbrowser` 가 `BROWSER` 를 읽지 않는 플랫폼 — 줄 것이 없다.
        let _ = (command, home);
    }
}

#[cfg(unix)]
fn write_fake_browser(home: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(home)?;
    let script = home.join(FAKE_BROWSER_FILE);
    // 기록 파일은 스크립트 옆이다 — 경로를 스크립트 본문에 박지 않아 따옴표 문제가 없다.
    let body = format!(
        "#!/bin/sh\nprintf 'BROWSER\\t%s\\n' \"$*\" >> \"$(dirname \"$0\")/{OS_OPEN_LOG_FILE}\"\n"
    );
    std::fs::write(&script, body)?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    Ok(script)
}

/// 번들 plugin 을 격리 홈에 어떻게 들일지 정한다 — 갈래는 스위트 단위로 둘이다.
///
/// - **안 부르는 스위트**: 빈 디렉터리를 번들 루트로 지정한다. 제품의 `bundle_root()` 는
///   `TASTY_BUILTIN_PLUGINS_DIR` 를 **최우선**으로 보므로, 이 한 줄이 workspace 스테이징
///   탐색과 격리 홈 복사를 **둘 다** 건너뛰게 한다(ADR-0182).
/// - **부르는 스위트**: 번들을 `tasty_home/plugins/` 에 **hardlink 로 미리 넣는다**
///   ([`prefill_bundle_links`]). host 는 같은 버전 갈래에서 내용으로 판정하므로 이미 같은
///   파일을 다시 쓰지 않는다 — 부팅마다 약 1.1 GB 이던 복사가 사라진다(ADR-0525).
///
/// 어느 갈래든 제품 코드의 설치 경로는 그대로다 — 서명·업그레이드 판정이 얹혀 있는 자리를
/// 테스트 사정으로 바꾸지 않는다. `tasty_home` 은 자식에게 주는 `TASTY_HOME` 이다.
pub fn apply_bundle_opt_in(command: &mut std::process::Command, tasty_home: &std::path::Path) {
    // 아래 갈래의 물러남 사유는 `tracing::warn!` 으로만 남는다. 구독자가 없으면 그 줄은
    // 어디에도 안 찍힌다 — `common` 은 spawn 뒤에야 설치하고 `gui_common` 은 아예 안 한다.
    // 그래서 판정보다 먼저 여기서 설치한다(멱등).
    init_test_tracing();
    if suite_calls_bundled_plugins() {
        prefill_bundle_links(tasty_home);
        return;
    }
    // 만들기에 실패하면 아무것도 안 한다 — 없는 경로를 넘기면 제품이 그 분기를
    // 무시하고 진짜 번들을 찾으므로, 실패는 "변경 전" 동작으로 되돌아간다.
    // `create_dir_all` 은 멱등이라 동시 생성도 안전하고, 유니크화하면 내용이 같은
    // 빈 디렉터리만 완주 수만큼 늘 뿐이다. 격리가 사는 자리는 여기가 아니라 홈이다.
    //
    // 이유: **비어 있다는 것이 이 디렉터리 내용의 전부**라 아무도 쓰지 않고 아무도
    // 지우지 않는다 — 고정 이름이 위험한 근거(동시 완주가 서로의 파일을 truncate
    // 하거나 디렉터리를 지운다)가 성립할 대상이 없다. 공유가 의도다.
    let empty = std::env::temp_dir().join("tasty-test-empty-plugin-bundle");
    if std::fs::create_dir_all(&empty).is_ok() && empty.is_dir() {
        command.env("TASTY_BUILTIN_PLUGINS_DIR", &empty);
    }
}

// ───── 번들 hardlink 미리 채우기 (ADR-0525) ─────

/// 하네스 소유 스냅숏을 두는 디렉터리 이름. 자식 바이너리 옆(`target/<profile>/`)에 둔다 —
/// 빌드 트리와 수명이 같고(`cargo clean` 이 지운다) 청소 범위가 그 트리 안으로 닫힌다.
pub const BUNDLE_LINK_CACHE_DIR: &str = "test-bundle-links";

/// 부르는 스위트의 격리 홈 `tasty_home/plugins/` 에 번들을 hardlink 로 넣는다.
///
/// **이것은 최적화이지 정확성의 자리가 아니다.** 무엇을 넣든(스냅숏이 낡았든, 일부만
/// 들어갔든, 아무것도 못 넣었든) host 가 부팅하며 번들과 **내용으로** 대조해 다른 파일만
/// 새로 쓴다. 그래서 여기서의 실패는 전부 "변경 전 동작(host 의 복사)" 으로 물러나고,
/// 시험을 세우지 않는다 — 사유만 로그에 남긴다.
///
/// 원본이 번들 자체가 아니라 [`bundle_link_snapshot`] 인 이유: hardlink 는 inode 를
/// 공유하므로, 번들(`builtin-plugins/`)에 직접 걸면 `just build-plugins` 의 제자리 `cp` 가
/// 돌고 있는 시험의 plugin 을 바꾸거나 `Text file busy` 로 실패하고, 반대로 홈 쪽 제자리
/// 쓰기가 개발 번들을 오염시킨다. 스냅숏은 하네스만 만들고 하네스만 지운다.
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

/// 스위트(테스트 바이너리)당 한 번 정하는 스냅숏 경로. 못 정하면 `None` — 그 스위트는
/// 변경 전처럼 host 의 복사로 간다.
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

/// 자식이 부팅하며 고를 번들을 정하고, 그 내용 서명으로 이름 붙인 스냅숏을 돌려준다.
/// 같은 서명의 스냅숏이 이미 있으면 그대로 쓴다 — 번들이 안 바뀌는 동안 스냅숏 쓰기는 0 이다.
fn build_bundle_link_snapshot() -> Result<std::path::PathBuf, String> {
    // override 된 바이너리는 테스트와 다른 프로필일 수 있다. 그때 아래의 dev 스테이징을
    // 테스트 프로필로 돌리면 자식이 안 할 쓰기를 남의 번들에 하게 된다 — 대신 정하지 않는다.
    // 판정은 [`instance_bin`] 과 같은 규칙([`effective_override`])으로 한다 — 바이너리를 고르는
    // 자리를 여기에 하나 더 만들지 않는다.
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

/// 자식이 고를 번들 루트 — 제품의 우선순위를 따른다. 자식은 이 프로세스의 환경을 물려받으므로
/// `TASTY_BUILTIN_PLUGINS_DIR` 가 걸려 있으면 그것이 이긴다. 아니면 제품의 exe-relative 해석을
/// **그대로 부른다**(debug 에서는 그 안의 dev 스테이징도 돈다 — 자식이 부팅하며 곧 할 쓰기를
/// 먼저 하는 것뿐이고, 자식 쪽은 그 뒤 no-op 이 된다). 같은 답을 여기 따로 적지 않는다.
fn child_bundle_root(bin_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    if let Some(p) = std::env::var_os("TASTY_BUILTIN_PLUGINS_DIR").map(std::path::PathBuf::from)
        && p.is_dir()
    {
        return Ok(p);
    }
    tasty_host_plugin::builtin::bundle_root_from_exe_dir(bin_dir)
        .ok_or_else(|| format!("{} 옆에 번들이 없다", bin_dir.display()))
}

/// 스냅숏 자리와 격리 홈(`temp_dir`)이 같은 파일시스템인지 hardlink 한 번으로 잰다. 다르면
/// 스냅숏을 만들어도 걸 수 없으므로 1.1 GB 를 쓰기 전에 멈춘다.
fn probe_hard_link_to_temp(cache_root: &std::path::Path) -> Result<(), String> {
    // 스냅숏은 `OnceLock` 으로 한 번만 정하지만 키에 카운터를 넣어 재호출도 가른다.
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
    // 정리 실패는 판정을 안 바꾼다 — 남는 것은 몇 바이트짜리 탐침 하나이고, 이름에 카운터가
    // 있어 다음 탐침과 부딪치지 않는다.
    let _ = std::fs::remove_file(&linked);
    // 위와 같은 이유 — 탐침 원본 쪽.
    let _ = std::fs::remove_file(&probe);
    result.map_err(|e| {
        format!(
            "{} 와 {} 사이에 hardlink 를 못 건다(다른 파일시스템?): {e}",
            cache_root.display(),
            linked.parent().unwrap_or(&linked).display()
        )
    })
}

/// 번들의 **값싼** 내용 서명 — 상대 경로 · 크기 · mtime 을 FNV-1a 로 접는다.
///
/// 바이트를 읽지 않는 것이 의도다(스위트마다 1.1 GB 를 읽게 된다). 그래서 크기와 mtime 이
/// 같고 내용만 다른 교체(`cp -p`)는 못 가른다 — 그때 낡은 스냅숏이 걸려도 host 가 내용으로
/// 대조해 다른 파일을 새로 쓰므로, 틀리는 것은 비용뿐이고 결과는 아니다. 심볼릭 링크는
/// 따라간다(`just link-plugins` 가 번들을 링크로 채운다).
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

/// 번들을 `snapshot` 으로 복사한다 — 옆 임시 디렉터리에 다 쓴 뒤 rename 하므로, 이름이 보이는
/// 스냅숏은 언제나 완성본이다. 동시에 같은 서명을 만든 다른 완주가 먼저 이름을 가져갔으면
/// 내 것을 버리고 그쪽을 쓴다.
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
        // 못 지운 임시 디렉터리는 한 시간 뒤 [`prune_old_snapshots`] 가 치운다.
        let _ = std::fs::remove_dir_all(&building);
    }
    result
}

fn copy_tree(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        // 링크를 따라간다 — 스냅숏에는 링크가 아니라 내용이 있어야 번들 쪽 교체와 끊긴다.
        if std::fs::metadata(entry.path())?.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// 지금 서명이 아닌 스냅숏을 지운다 — 번들이 바뀔 때마다 1.1 GB 가 쌓이지 않게. 다른 완주가
/// 방금 만들고 있는 임시 디렉터리(`.building-*`)는 한 시간이 지나기 전엔 안 건드린다.
/// 지우는 도중 다른 완주가 그 스냅숏에서 걸고 있었다면 그쪽 link 가 실패하고 host 의 복사로
/// 물러날 뿐이다 — 이미 걸린 link 는 inode 가 살아 있어 영향이 없다.
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

/// `src` 의 파일을 `dst` 아래 같은 상대 경로에 hardlink 로 건다. 첫 실패에서 멈춘다 — 이미
/// 걸린 것은 두고, 나머지는 host 가 내용 대조로 채운다.
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

// ───── 자식이 어느 디스플레이에 뜨는가 ─────

/// 자식 인스턴스가 쓸 디스플레이를 **명시**하는 환경변수.
///
/// gui 조합의 데몬은 창을 반드시 만들고([`instance_bin`] 의 doc), winit 은 그 창을
/// 어디에 띄울지 `DISPLAY`/`WAYLAND_DISPLAY` 로 고른다. 하네스가 그 둘을 정하지
/// 않으면 `Command` 가 부모의 값을 그대로 물려주므로 **실행자가 보고 있는 화면이
/// 그대로 시험의 디스플레이가 된다.** 그 상태는 조용하다 — 시험은 통과하고, 창만
/// 사람 화면에 뜬다.
///
/// 실측 2026-09-20(이 개발 박스, 전용 Xvfb `:77` 위에서 `shared_instance_harness`):
/// 자식 `/proc/<pid>/environ` 의 `DISPLAY` 가 부모가 준 `:77` 이었고, 그 디스플레이에
/// `1280x720` 짜리 `Tasty (Debug)` 창이 떴다. 부모가 사람이 보는 화면이면 창은 거기 뜬다.
///
/// **그렇다고 상속을 끊을 수는 없다.** 같은 날 같은 바이너리를 `DISPLAY`·`WAYLAND_DISPLAY`
/// 없이 띄우니 winit 이 `neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set`
/// 로 즉사하고 port file 이 안 써졌다([`NO_DISPLAY_MARKERS`] 가 그 시그니처다). gui
/// 조합에서 디스플레이는 격리해야 할 누수가 아니라 **필요한 입력**이고, 지우면 이
/// 하네스를 쓰는 스위트가 전부 부팅에 실패한다.
///
/// 그래서 남는 물음은 하나다 — **누가 그 입력을 정하는가.** 이 변수가 그 답을 값으로
/// 남긴다. 값이 없으면 시험을 세운다: 조용히 남의 화면을 쓰는 것보다 시끄럽게 멈추는
/// 쪽이 낫다.
pub const DISPLAY_ENV: &str = "TASTY_E2E_DISPLAY";

/// [`DISPLAY_ENV`] 의 "내가 보고 있는 화면을 그대로 쓰겠다" 선언.
///
/// 오늘의 동작을 그대로 남기는 값이다 — 없애지 않는 이유는 창을 눈으로 보면서
/// 고치는 작업이 실재하기 때문이다. 다만 그때는 그것이 **선택**이었음이 값으로 남는다.
pub const DISPLAY_INHERIT: &str = "inherit";

/// 자식에게 줄 디스플레이의 결정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayPolicy {
    /// 이 조합은 디스플레이를 안 쓴다 — 아무것도 정하지 않는다.
    NotRequired,
    /// 실행자가 자기 화면을 쓰겠다고 선언했다 — 오늘 동작(상속) 그대로.
    Inherit,
    /// 지정된 디스플레이를 자식에게 준다.
    Pin(String),
    /// 요구되는데 지정이 없다 — 세운다.
    Unspecified,
}

/// [`DisplayPolicy`] 의 판정만 떼어낸 순수 함수.
///
/// 환경변수를 건드리지 않고 네 갈래를 다 시험할 수 있어야 한다(테스트는 병렬로 돌고
/// `set_var` 는 프로세스 전역이다) — [`resolve_instance_bin`] 과 같은 이유다.
///
/// 빈 문자열은 **미지정과 같게** 다룬다. 셸에서 `TASTY_E2E_DISPLAY=` 로 비우는 것이
/// "안 준다" 로 읽히는 것이 자연스럽고, 빈 `DISPLAY` 를 자식에게 주면 winit 이
/// 디스플레이 부재와 같은 자리에서 죽어 원인이 한 겹 가려진다.
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

/// 이 완주가 **창을 만드는 데몬**을 띄우는가.
///
/// 세 조건의 곱이다.
///
/// * `target_os = "linux"` — 디스플레이를 환경변수로 고르는 플랫폼이라야 물음이 선다.
///   macOS 는 창을 항상 사용자 세션에 띄우고 Windows 도 같다. 그쪽에서 이 변수를
///   요구하면 줄 수 있는 답이 없다.
/// * `feature = "gui"` — 헤드리스 데몬은 창도 GPU 도 안 만든다. 그 조합에서 자식이
///   `DISPLAY` 를 물려받아도 아무 데도 안 쓴다.
/// * override 가 안 먹히는 스위트 — [`INSTANCE_BIN_ENV`] 의 용도는 미리 지어 둔
///   **헤드리스** 바이너리를 가리키는 것이다([`instance_bin`] 의 doc). 그 경로로 뜬
///   데몬에는 창이 없으므로 디스플레이를 요구할 것이 없다.
///
/// **마지막 조건은 선언된 사각이다.** override 가 gui 바이너리를 가리키면 창이 뜨는데
/// 이 판정은 안 요구한다. 경로만 보고 그 바이너리의 조합을 알 방법이 없어서다 — 그
/// 경우는 오늘 동작(조용한 상속)으로 남는다.
fn display_required() -> bool {
    if !cfg!(all(target_os = "linux", feature = "gui")) {
        return false;
    }
    effective_override(daemon_kind(), std::env::var_os(INSTANCE_BIN_ENV)).is_none()
}

/// [`DisplayPolicy::Unspecified`] 의 문구. 처방을 그대로 찍는다 — 이 자리에서 멈춘
/// 사람이 다시 검색하지 않아도 되게.
fn unspecified_display_message() -> String {
    format!(
        "{DISPLAY_ENV} 가 없다.\n\
         이 조합(gui)의 데몬은 창을 만들고, 어디에 띄울지는 물려받은 DISPLAY 가 정한다 — \
         즉 지금 이대로 돌리면 **네가 보고 있는 화면에** 시험의 창이 뜨고, 그 화면의 \
         컴포지터가 시험의 타이밍에 섞인다. 둘 다 조용하다(시험은 통과한다).\n\
         전용 디스플레이 위에서 돌려라:\n\
         \x20 Xvfb :77 -screen 0 1920x1080x24 -nolisten tcp -ac &\n\
         \x20 {DISPLAY_ENV}=:77 cargo test ...\n\
         자기 화면에 띄우는 것이 의도면 그렇게 선언해라: {DISPLAY_ENV}={DISPLAY_INHERIT}\n\
         창이 필요 없으면 헤드리스 데몬으로 돌려라 — docs/dev-guide/e2e-tests.md"
    )
}

/// 자식이 쓸 디스플레이를 정해 `command` 에 얹는다. 두 하네스가 spawn 직전에 부른다.
///
/// [`DisplayPolicy::Pin`] 은 `WAYLAND_DISPLAY` 를 함께 지운다 — 두 축이 같이 있으면
/// winit 이 어느 쪽을 고르는지가 백엔드 선택 규칙에 달리고, 그러면 지정한 디스플레이가
/// 실제로 쓰였는지를 값으로 말할 수 없다.
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
