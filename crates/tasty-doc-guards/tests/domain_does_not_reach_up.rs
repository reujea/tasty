//! 도메인 경계 가드 — 도메인(`src/core/` · `src/ports/`)의 **출하되는** 코드가 본체의
//! 조립·어댑터·GUI 모듈이나 GUI 크레이트를 이름으로 부르면 fail 한다. 그리고 도메인 안의
//! gui feature 게이트 수를 양방향으로 고정한다.
//!
//! # 왜 컴파일러가 아니라 이 가드인가
//!
//! 도메인은 본체와 **같은 크레이트**에 산다(크레이트를 떼지 않은 이유와 대안은
//! [ADR-0440](../../../docs/adr/0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md)).
//! 같은 크레이트 안에서는 `crate::app::…` 이 언제나 이름 해석된다 — 도메인이 창 조립부를
//! 거꾸로 불러도 컴파일은 통과한다. 크레이트 경계가 해 주었을 일을 이 가드가 대신한다.
//!
//! # 세 물음
//!
//! 1. **상위 참조** — [`UPPER`] 의 모듈을 출하 코드가 부르는가. 기대값 0, 베이스라인 없음.
//!    착수 시점(2026-09-21, `17e2a7f56`)에 24 자리였고 이동·포트 역전으로 0 이 된 뒤에
//!    세웠다 — 그래서 "줄기만 하는 한시 허용" 명부가 필요 없다. 새 자리는 곧 위반이다.
//! 2. **gui 게이트 수** — 도메인 출하 코드에 코드로 쓰인 `feature = "gui"` 의 개수. 도메인에
//!    GUI 전용 항목을 cfg 로 숨겨 들여오면 상위 참조가 없어 보여도 **도메인이 GUI 를 안다**.
//!    그 수를 [`GUI_GATES_IN_DOMAIN`] 에 고정한다. 늘어도 줄어도 실패한다 — 줄면 상수를 같이
//!    내린다(남는 여유가 곧 안 보는 구간이다).
//! 3. **GUI 크레이트** — 출하 코드가 `gui` feature 뒤의 외부 크레이트(egui · winit · wgpu ·
//!    webkit2gtk · gtk · objc2 계열 · webview2-com 등 — 목록은 매니페스트에서 읽는다,
//!    [`gui_crates`])나 `windows` 의 창·그리기 하위 경로·창 핸들을 부르는가. 2 번의 수는 **새 게이트**만
//!    센다 — 이미 있는 게이트 뒤 import 에 `egui::Context` 를 끼워 넣으면 수가 그대로라 안
//!    보였다([ADR-0490](../../../docs/adr/0490-boundary-guards-close-three-holes-found-by-mutation.md)).
//!    그래서 이 물음은 게이트를 안 빼고 읽고, 기존 자리를 **(파일, 경로) 목록**으로
//!    고정한다([`GUI_CRATE_PATHS_IN_DOMAIN`] — 오늘 비어 있다).
//!
//! # 좌변
//!
//! `src/core/**` · `src/ports/**` 의 `.rs` 중 **출하되는 것** — 파일 단위 test-only
//! ([`test_only_files`])를 빼고, 인라인 `#[cfg(test)]` 줄([`cfg_gated_lines`])을 뺀다.
//! 테스트는 픽스처(`adapters::test`, `state::tests`)를 부르는 것이 정상이고 출하 산출물에
//! 안 들어간다(layering 가드가 같은 이유로 테스트를 뺀다 — ADR-0123).
//!
//! 주석과 문자열은 [`mask_non_code`] 로 지운다. 도메인의 문서 주석은 "이 일은 창 쪽
//! `AppState` 가 한다" 처럼 상위 모듈을 **설명으로** 말하는 것이 정상이다.
//!
//! 경로는 줄이 아니라 **마스킹한 파일 전체**에서 읽는다 — 중괄호 import(`crate::{a, b::c}`)는
//! 항목마다 펴고, 마디 사이의 공백·줄바꿈은 건넌다. 줄 번호는 항목이 시작한 오프셋으로
//! 환산한다. 판정은 **앞마디 일치**다: [`UPPER`] 의 `file::dispatch` 는 `crate::file::dispatch::X`
//! 를 잡고 `crate::file::format::X` 는 안 잡는다.
//!
//! # 이 가드가 안 보는 것
//!
//! - **전이 의존.** 도메인이 부르는 형제 모듈(`file`·`store`·`hook_handler` 등)이 다시 상위
//!   모듈을 부르는 경로는 안 센다. 형제 모듈이 상위 항목을 **재수출**하면 그 이름으로 우회된다.
//!   크레이트를 떼는 날 그 경로가 경계를 넘는다 — ADR-0440 의 재검토 조건이 그 값을 잰다.
//! - **GUI 갈래를 가진 워크스페이스 크레이트.** `tasty-platform/gui` · `tasty-icons/egui` 처럼
//!   `gui` 가 **다른 크레이트의 feature** 를 켜는 것은 그 크레이트가 headless 에도 링크되어,
//!   `tasty_platform::…` 이 GUI 갈래를 부르는지 이름으로 안 갈린다.
//! - **지역 모듈과 같은 이름의 크레이트.** `use` 없이 `image::x` 로 부르는 지역 모듈은
//!   크레이트와 텍스트로 안 갈린다 — 오늘 도메인에 그런 모듈은 없다(적중 0).
//! - **`#[path]` 로 옮겨 붙인 모듈의 `super::` 깊이.** `super::` 이탈 판정은 파일 경로로
//!   모듈 깊이를 계산한다. 오늘 도메인에 `#[path]` 는 test 모듈에만 있다.

use std::path::{Path, PathBuf};

use tasty_doc_guards::cargo_manifest::feature_enabled_deps;
use tasty_doc_guards::cfg_predicate::cfg_gated_lines;
use tasty_doc_guards::crate_paths::{
    module_depth, path_is_under, shipped_external_references, shipped_references,
};
use tasty_doc_guards::repo_root;
use tasty_doc_guards::shipping_scope::test_only_files;
use tasty_doc_guards::source_text::{mask_comments, rust_sources};

/// 도메인 뿌리. `/` 로 끝나는 접두사다.
const DOMAIN_ROOTS: &[&str] = &["src/core/", "src/ports/"];

/// 도메인이 이름으로 부르면 안 되는 크레이트 루트 항목 — `(이름, 무엇이라 안 되는가)`.
///
/// 모듈과 함께 **lib 루트의 별칭**(`src/lib.rs` 의 `pub(crate) use …`)도 적는다. 별칭은
/// 같은 모듈의 다른 이름이라, 모듈만 막으면 별칭으로 우회된다. 거꾸로 별칭이 **형제 모듈의
/// 하위 항목**을 가리키면(`file_dispatch` = `file::dispatch`) 그 정식 경로도 여러 마디로 적는다 —
/// 별칭만 막으면 정식 경로로 우회된다.
const UPPER: &[(&str, &str)] = &[
    ("app", "창·이벤트 루프 조립(`App`)"),
    ("AppEvent", "`app::event::AppEvent` 의 lib 루트 별칭"),
    ("App", "`app::App` 의 lib 루트 별칭"),
    ("adapters", "어댑터 전체(IPC 핸들러 · UI · production 구현)"),
    ("ipc", "`adapters::ipc` 의 별칭 — 요청 핸들러 트리"),
    ("cli", "`adapters::cli` 의 별칭 — CLI 진입 계층"),
    ("plugin_bridge", "plugin 매니저와 본체 GUI 를 잇는 glue"),
    (
        "plugin",
        "`adapters::plugin` 의 별칭 — 매니페스트 타입은 `tasty_plugin_manifest` 로 직접 닿는다",
    ),
    (
        "state",
        "창 상태(`AppState`) — 도메인은 `core::cascade_window` 포트로만 닿는다",
    ),
    ("search_state", "`state::search` 의 별칭"),
    ("selection", "`state::selection` 의 별칭"),
    (
        "intent",
        "GUI intent 큐 — 발화 주체는 `core::origin` 에 있다",
    ),
    ("view", "창 view 트리"),
    ("window", "`view` 의 별칭"),
    ("gfx", "GPU 렌더링"),
    ("gpu", "`gfx::gpu` 의 별칭"),
    ("renderer", "`gfx::renderer` 의 별칭"),
    ("boot", "프로세스 기동·조립"),
    ("hub", "IPC 서버 조립"),
    (
        "identify_worker",
        "winit proxy 로 결과를 보내는 GUI worker — `core::identify_port` 로만 닿는다",
    ),
    (
        "file::identify_worker",
        "`identify_worker` 의 정식 경로 — `core::identify_port` 로만 닿는다",
    ),
    ("file_dispatch", "`file::dispatch` 의 gui 별칭"),
    (
        "file::dispatch",
        "창 상태를 받는 파일 열기 동작 — 도메인이 쓰는 발화 주체는 `core::origin` 에 있다",
    ),
    ("shortcuts", "UI 입력 단축키"),
    ("click_cursor", "UI 입력"),
    ("double_tap", "UI 입력"),
    ("preset_ui", "UI"),
    ("empty_ui", "UI"),
    ("explorer_ui", "UI"),
    ("webview_chrome_ui", "UI"),
    ("terminal_link", "UI"),
    ("plugins_ui", "UI"),
    ("settings_ui", "UI"),
    ("webview", "`host_api::webview` 의 gui 별칭"),
    ("host_api::webview", "`webview` 의 정식 경로 — GUI webview"),
    ("ClipboardContext", "GUI 클립보드 컨텍스트"),
    (
        "clipboard",
        "`ClipboardContext` 의 정식 모듈 — 도메인은 `ports::clipboard` 로 닿는다",
    ),
    ("waker_factory_winit", "winit waker"),
    ("debug_info", "`app::debug_info` 의 별칭"),
];

/// 도메인 출하 코드의 `feature = "gui"` 개수. 실측 2026-09-21, 이 가드와 같은 판정기로:
/// 현재 **258** · 경계 작업을 마친 트리 246 · 경계 작업 착수 트리(`17e2a7f56`) **239**.
///
/// 경계 작업이 이 수를 7 **올렸다**. 분해: `core/file.rs` −6(GUI 동작인 picker 적용을
/// `file::dispatch::picker_apply` 로 뺐다) · `core/cascade_window.rs` +9(포트 메서드 중 호출
/// 자리가 이미 gui 로 가려진 통지·튜토리얼 관찰 여덟 개와 그 import 하나) ·
/// `core/host_event.rs` +2(`state.rs` 의 모듈 단위 `allow(dead_code)` 가 가리던 headless
/// `expect` 가 드러났다) · `core/mod.rs` +1(`identify_port` 선언) · `core/origin.rs` +1
/// (`FileDispatchOrigin` 을 도메인으로 옮기며 `file::dispatch` 의 모듈 단위 `allow` 가 가리던
/// headless `expect` 가 드러났다). 늘어난 여덟은 새 GUI
/// 의존이 아니라 **이미 있던 게이트가 도메인 쪽 선언에 옮겨 적힌 것**이다 — 그 메서드들은 GUI
/// 타입을 하나도 안 부른다.
///
/// 그 뒤 `src/state.rs` 의 모듈 단위 `allow(dead_code)` 를 지운 것(ADR-0355 잔여 ②·③)이 5 를
/// 더 올렸다. 그 `allow` 는 dead 판정의 뿌리 노릇도 해서, 지우자 `core` 다섯 자리가 headless 에
/// 소비자 없는 정의로 드러났다 — `set_category_collapsed` · `reify_plugin_surface` ·
/// `SurfaceCwd` 재수출과 그 `as_str`(①·②) · `SurfaceKindDef::convert_input_popup`(③).
///
/// 그 뒤 도메인 안의 모듈 단위 headless `allow(dead_code)` 를 지우고 드러난 정의를 항목마다
/// 갈랐다. 모듈 속성 줄 자체가 `feature = "gui"` 를 하나 담고 있어 지울 때마다 −1 이다:
/// `core/attach.rs` +1(속성 −1 · `is_content_hidden` ② · `workspace_holders` ①) ·
/// `core/attach_readonly.rs` 0(속성 −1 · 모듈 전체가 ① 이라 `core/mod.rs` 의 선언 +1) ·
/// `core/state/soft_occupancy.rs` 0(속성 −1 · `reconcile_soft_occupancy_on_focus` ②).
/// 도메인 안에 모듈 단위 headless `allow` 는 이제 없다.
/// 도메인 밖의 모듈 단위 `allow` 도 뿌리 노릇을 했다 — `file/dispatch.rs` 의 것을 지우자
/// 도메인 `core/origin.rs` 의 `selects_result` · `require_origin_pane` 이 headless 소비자 없는
/// 정의로 드러났다(① +2). `intent.rs` 의 것을 지우자 도메인 `core/origin.rs` 의 사용자 발화
/// 주체(`IntentOrigin::User` · `UserSource`)가 headless 라이브러리에서 만들어지지 않는 variant 로
/// 드러났다(③ +2). `adapters/ipc.rs` 의 것을 지우자 도메인 `core/session.rs` 의 agent 권한
/// 임시 grant·revoke 둘이 드러났다(① +2 — headless IPC 표면이 그 두 메서드를 받지 않는다).
/// 그 뒤 headless 가 mirror 구조 op 를 forward 큐에 넣지 않고 거절하게 했다(ADR-0538, +6).
/// `core/impl_mirror.rs` +5 — op 를 만드는 `build_mirror_forward_op` · 큐에 넣는
/// `queue_mirror_forward` 의 두 갈래 · 그 원소 생성자 `PendingStructuralForward::agent` 는
/// headless 에 소비자가 없고(① 네 자리), 차단 문구의 headless 꼬리 한 줄이 다섯째다. `core/state.rs`
/// +1 — `pending_resize_forward` 는 채우는 쪽도 비우는 쪽도 gui 뿐이라 `expect(dead_code)`
/// 한 줄을 필드·초기화의 cfg 두 줄로 바꿨다(①).
/// 그 뒤 headless PTY drain 이 OSC 7 cwd 로 탭 이름을 다시 매기게 했다 — `core/state.rs` 의
/// `refresh_tab_display_name` 에 headless 소비자가 생겨 게이트가 빠졌고, `core/intent.rs` 의
/// `CoreEvent::TerminalCwdChanged` 도 칸을 headless 가 읽게 되어 `expect(dead_code)` 가 빠졌다(−2).
/// 그래서 지금 262 이다. 레포에 `cfg_attr(not(feature = "gui"), allow(dead_code))` 모듈 속성은 없다.
///
/// 이 수는 **목표가 아니라 현재 상태의 못**이다. 도메인이 GUI 전용 항목을 갖는 이유는
/// 대부분 "headless 에 소비자가 없다"(ADR-0346)이고 그 판정 자체는 정당하다. 이 못이 막는
/// 것은 **새 게이트가 조용히 들어오는 것**이다 — 들어올 때 이 수를 올리는 커밋이 그
/// 판단을 드러낸다.
const GUI_GATES_IN_DOMAIN: usize = 262;

/// 도메인 뿌리 아래 `.rs` 수의 하한. 실측 2026-09-21: 92 개(`src/core` 84 · `src/ports` 8),
/// 그중 출하되는 것 91.
/// 수집이 죽거나 뿌리가 옮겨지면 두 판정이 빈 집합을 훑고 초록이 된다.
const MIN_DOMAIN_FILES: usize = 80;

/// 순회가 도메인에 닿았음을 고정하는 앵커 — `core` 모듈의 루트 파일.
const DOMAIN_ANCHOR: &str = "src/core/mod.rs";

fn in_domain(rel: &Path) -> bool {
    let rel = rel.to_string_lossy().replace('\\', "/");
    DOMAIN_ROOTS.iter().any(|r| rel.starts_with(r))
}

/// 크레이트 루트부터의 마디들이 [`UPPER`] 의 어느 항목 아래인가 — 판정은
/// [`path_is_under`] 의 앞마디 일치다.
fn upper_match(path: &[String]) -> Option<&'static str> {
    UPPER
        .iter()
        .map(|(n, _)| *n)
        .find(|n| path_is_under(n, path))
}

/// 한 파일의 출하되는 코드가 부르는 상위 항목. `(1-기준 줄번호, 이름, 그 줄 원문)` —
/// 읽는 법은 [`shipped_references`].
fn upper_references(rel: &str, text: &str) -> Vec<(usize, &'static str, String)> {
    shipped_references(rel, text, upper_match)
}

/// 한 파일의 출하되는 줄에서 코드로 쓰인 `feature = "gui"` 개수.
fn gui_gates(text: &str) -> usize {
    let lines: Vec<&str> = text.lines().collect();
    let gated = cfg_gated_lines(&lines, "test");
    mask_comments(text)
        .lines()
        .enumerate()
        .filter(|(i, _)| !gated.get(*i).copied().unwrap_or(false))
        .map(|(_, code)| code.matches("feature = \"gui\"").count())
        .sum()
}

/// 출하되는 도메인 파일 `(레포 상대 경로, 원문)`. 하한·앵커를 여기서 확인한다.
fn shipped_domain_sources() -> Vec<(PathBuf, String)> {
    let root = repo_root();
    // 부모 선언을 따라가야 test-only 여부가 정해지므로 `src` 전체를 모은 뒤 거른다.
    let sources = rust_sources(&root, &["src"]);
    let not_shipped = test_only_files(&root, &sources);
    let domain: Vec<(PathBuf, String)> = sources
        .into_iter()
        .filter(|(rel, _)| in_domain(rel))
        .collect();
    assert!(
        domain.len() >= MIN_DOMAIN_FILES,
        "도메인 뿌리({}) 아래 `.rs` 를 {} 개만 모았다(하한 {MIN_DOMAIN_FILES}) — 수집이 죽었거나 \
         뿌리가 옮겨졌다. 빈 집합을 훑으면 아래 판정은 조용히 통과한다.\n\
         ★ 하한을 내려서 통과시키지 마라. 도메인이 정말 옮겨졌으면 `DOMAIN_ROOTS` 를 새 \
         자리로 바꿔라.",
        DOMAIN_ROOTS.join(" · "),
        domain.len()
    );
    assert!(
        domain
            .iter()
            .any(|(rel, _)| rel.to_string_lossy().replace('\\', "/") == DOMAIN_ANCHOR),
        "순회가 `{DOMAIN_ANCHOR}` 에 닿지 않았다 — `core` 모듈의 루트라 실재가 보장된다."
    );
    domain
        .into_iter()
        .filter(|(rel, _)| !not_shipped.contains(rel))
        .collect()
}

#[test]
fn the_domain_does_not_name_an_upper_layer() {
    let sources = shipped_domain_sources();
    let mut offenders = Vec::new();
    for (rel, text) in &sources {
        let rel = rel.to_string_lossy().replace('\\', "/");
        for (line, name, raw) in upper_references(&rel, text) {
            let why = UPPER
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, w)| *w)
                .unwrap_or("");
            offenders.push(format!("  {rel}:{line} — `{name}` ({why}): {raw}"));
        }
    }
    println!(
        "[도메인 경계] 출하 도메인 파일 {} 개 · 상위 참조 {} 자리",
        sources.len(),
        offenders.len()
    );
    assert!(
        offenders.is_empty(),
        "도메인(`src/core` · `src/ports`) 출하 코드가 상위 계층을 이름으로 부른다:\n{}\n\
         도메인은 조립·어댑터·GUI 를 모른다(ADR-0440). 처방은 방향을 뒤집는 것이다:\n\
         - 도메인이 쓰는 타입이 상위 모듈에 정의돼 있으면 **정의를 도메인으로 옮기고** \
           상위 모듈이 재수출한다(`core::origin` · `core::host_event` 가 그렇게 왔다).\n\
         - 도메인이 창 쪽 연산이 필요하면 **도메인이 trait 을 선언하고** 창 쪽이 구현한다 \
           (`core::cascade_window` · `core::identify_port`).\n\
         - GUI 동작이 도메인 안에 cfg 로 숨어 있으면 **GUI 쪽으로 옮긴다** \
           (`file::dispatch::picker_apply`).\n\
         ★ 이 가드에 면제 명부를 만들어 통과시키지 마라 — 명부가 비어 있는 것이 이 경계의 \
         현재 상태다.",
        offenders.join("\n")
    );
}

#[test]
fn gui_gates_in_the_domain_are_pinned() {
    let sources = shipped_domain_sources();
    let mut per_file: Vec<(String, usize)> = sources
        .iter()
        .map(|(rel, text)| (rel.to_string_lossy().replace('\\', "/"), gui_gates(text)))
        .filter(|(_, n)| *n > 0)
        .collect();
    per_file.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: usize = per_file.iter().map(|(_, n)| n).sum();
    println!(
        "[도메인 gui 게이트] {total} 개 / {} 파일 (고정값 {GUI_GATES_IN_DOMAIN})",
        per_file.len()
    );
    let listing = per_file
        .iter()
        .map(|(f, n)| format!("  {n:>3}  {f}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        total <= GUI_GATES_IN_DOMAIN,
        "도메인 출하 코드의 `feature = \"gui\"` 가 {total} 개로 늘었다(고정값 \
         {GUI_GATES_IN_DOMAIN}).\n{listing}\n\
         도메인에 GUI 전용 항목을 cfg 로 숨기면 상위 참조 가드가 못 본다 — 그 항목이 \
         창 상태나 GUI 부품을 부르지 않아도 **도메인이 GUI 를 안다**.\n\
         먼저 물어라: 이 항목은 headless 에 소비자가 없어서 가리는가(ADR-0346 ① — 정당), \
         아니면 GUI 동작이 도메인에 들어와서 가리는가(그러면 GUI 쪽으로 옮겨라).\n\
         앞쪽이면 이 상수를 {total} 으로 올리고 그 커밋 본문에 판정을 적어라.",
    );
    assert!(
        total >= GUI_GATES_IN_DOMAIN,
        "도메인 출하 코드의 `feature = \"gui\"` 가 {total} 개로 줄었다(고정값 \
         {GUI_GATES_IN_DOMAIN}). 좋은 일이다 — 상수를 {total} 으로 **같이 내려라**. \
         남겨 두면 그 차이만큼 새 게이트가 조용히 들어올 여유가 된다.\n{listing}",
    );
}

/// 도메인에 이름이 나오면 안 되는 GUI 크레이트 — 워크스페이스 `Cargo.toml` 의 `gui` feature 가
/// `dep:` 로 켜는 optional 의존 전부다([`feature_enabled_deps`]). 손으로 적은 목록이 아니라
/// 매니페스트에서 읽는다 — `gui` 에 크레이트가 더해지면 이 판정도 같이 넓어진다.
///
/// 실측 2026-09-22: 29 개(winit · wgpu · egui 계열 넷 · objc2 계열 넷 · block2 · webview2-com ·
/// webkit2gtk · gtk · gdkx11 · x11-dl · tray-icon · rfd · arboard · drag · cosmic-text 등).
/// GUI **타입**이 아닌 것(`png` · `image` · `pulldown-cmark` · `trash` · `bytemuck` ·
/// `webbrowser`)도 섞여 있다 — 그래도 뺄 이유가 없다: headless 그래프에 없는 크레이트이므로
/// 도메인이 그것을 부르면 gui 게이트 뒤에 숨겨야만 컴파일되고, 그것이 곧 "도메인이 GUI
/// 구성을 안다" 이다.
///
/// `tasty-platform/gui` 처럼 **다른 크레이트의 feature** 를 켜는 항목은 안 든다 — 그 크레이트는
/// headless 에도 링크되어, 이름만으로는 GUI 갈래를 부르는지 안 갈린다(아래 "안 보는 것").
fn gui_crates() -> Vec<String> {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("워크스페이스 Cargo.toml 을 읽을 수 없다");
    let crates: Vec<String> = feature_enabled_deps(&manifest, "gui")
        .into_iter()
        .map(|d| d.replace('-', "_"))
        .collect();
    for anchor in [
        "egui",
        "winit",
        "wgpu",
        "webkit2gtk",
        "gtk",
        "objc2_app_kit",
        "webview2_com",
    ] {
        assert!(
            crates.iter().any(|c| c == anchor),
            "`gui` feature 에서 읽은 크레이트 목록에 `{anchor}` 가 없다({crates:?}) — 매니페스트 \
             읽기가 깨졌거나 feature 가 옮겨졌다. 빈 목록을 훑으면 아래 판정은 조용히 통과한다."
        );
    }
    crates
}

/// `gui` feature 뒤가 아닌데 OS 창·그리기 타입을 담은 크레이트의 **하위 경로**. `windows` 는
/// Windows 타깃에서 조건 없이 링크된다(프로세스·콘솔·파일 시스템도 이것으로 부른다) —
/// 크레이트 전체를 막을 수는 없고, 창·그리기 갈래만 막는다.
///
/// 경로가 이 항목들의 **진접두사에서 끝나면**(`use windows::Win32 as w32;` · `use windows::Win32::*;` ·
/// `use windows::Win32::Foundation::{self, …}` · `use windows as w;`) 그것도 잡는다 — 들인 모듈
/// 이름으로 `w32::UI::…` 를 부르면 경로가 `UI` 에 닿기 전에 끊겨 항목 대조가 안 보기 때문이다.
/// 진접두사에서 끝나는 경로는 모듈을 들이는 import 뿐이다(식 안의 경로는 항목까지 간다).
///
/// 창 핸들과 창 프로시저 인자는 `UI` 가 아니라 `Foundation` 에 산다(windows 0.61 실측 —
/// `HWND` · `HINSTANCE` · `LPARAM` · `WPARAM`). `Foundation` 전체는 `HANDLE` · `BOOL` 같은 창
/// 아닌 타입을 담으므로 **항목 단위**로 적는다. 판정이 앞마디 일치라 항목 경로도 그대로 동작한다.
const OS_WINDOW_PATHS: &[&str] = &[
    "windows::Win32::UI",
    "windows::Win32::Graphics",
    "windows::Win32::Foundation::HWND",
    "windows::Win32::Foundation::HINSTANCE",
    "windows::Win32::Foundation::LPARAM",
    "windows::Win32::Foundation::WPARAM",
];

/// 도메인 출하 코드가 **이미** GUI 크레이트를 부르는 자리 — `(파일, 경로)`.
///
/// 실측 2026-09-22: **0**. 시험 전용 `windows::Win32::Foundation` 등(`core/layout_persistence/tests.rs`)
/// 은 출하되지 않아 좌변에 없다. 수가 아니라 **목록**인 이유: 기존 gui 게이트 안에 항목을
/// 하나 더 끼워 넣어도 게이트 수는 그대로다 — 수만 세면 그 추가가 또 안 보인다.
const GUI_CRATE_PATHS_IN_DOMAIN: &[(&str, &str)] = &[];

/// 한 파일의 출하 코드가 부르는 GUI 크레이트 경로. `(1-기준 줄번호, 경로, 그 줄 원문)`.
fn gui_crate_references(text: &str, crates: &[String]) -> Vec<(usize, String, String)> {
    let mut roots: Vec<&str> = crates.iter().map(String::as_str).collect();
    roots.extend(OS_WINDOW_PATHS.iter().filter_map(|p| p.split("::").next()));
    shipped_external_references(text, &roots)
        .into_iter()
        .filter_map(|(line, path, raw)| {
            // `{self, …}` 의 `self` 는 앞마디 모듈 자체를 들인다.
            let path = path
                .strip_suffix("::self")
                .map(str::to_string)
                .unwrap_or(path);
            let head = path.split("::").next().unwrap_or("");
            let hit = crates.iter().any(|c| c == head)
                || OS_WINDOW_PATHS.iter().any(|p| {
                    path == *p
                        || path.starts_with(&format!("{p}::"))
                        || p.starts_with(&format!("{path}::"))
                });
            hit.then_some((line, path, raw))
        })
        .collect()
}

#[test]
fn the_domain_names_no_gui_crate_beyond_the_pinned_list() {
    let crates = gui_crates();
    let sources = shipped_domain_sources();
    let mut found: Vec<(String, String, usize, String)> = Vec::new();
    for (rel, text) in &sources {
        let rel = rel.to_string_lossy().replace('\\', "/");
        for (line, path, raw) in gui_crate_references(text, &crates) {
            found.push((rel.clone(), path, line, raw));
        }
    }
    println!(
        "[도메인 GUI 크레이트] 크레이트 {} 개 · 출하 도메인 파일 {} 개 · 자리 {} (고정 목록 {})",
        crates.len(),
        sources.len(),
        found.len(),
        GUI_CRATE_PATHS_IN_DOMAIN.len()
    );
    let new: Vec<String> = found
        .iter()
        .filter(|(f, p, _, _)| !GUI_CRATE_PATHS_IN_DOMAIN.contains(&(f.as_str(), p.as_str())))
        .map(|(f, p, l, raw)| format!("  {f}:{l} — `{p}`: {raw}"))
        .collect();
    assert!(
        new.is_empty(),
        "도메인(`src/core` · `src/ports`) 출하 코드가 GUI 크레이트를 부른다:\n{}\n\
         도메인에 WebView·egui·OS 창 타입이 들어오면 안 된다(ADR-0440 · ADR-0490). gui 게이트 \
         뒤에 두어도 마찬가지다 — 게이트는 headless 컴파일만 가릴 뿐 도메인이 GUI 를 아는 \
         사실은 그대로다. 처방은 상위 참조와 같다: 그 타입을 쓰는 동작을 GUI 쪽으로 옮기거나, \
         도메인이 trait 을 선언하고 창 쪽이 구현한다(`core::cascade_window`).\n\
         ★ `GUI_CRATE_PATHS_IN_DOMAIN` 에 올려서 통과시키지 마라 — 그 목록은 이 가드를 세운 \
         날의 잔여를 적는 자리이고, 오늘 비어 있다.",
        new.join("\n")
    );
    let stale: Vec<String> = GUI_CRATE_PATHS_IN_DOMAIN
        .iter()
        .filter(|(f, p)| !found.iter().any(|(ff, pp, _, _)| ff == f && pp == p))
        .map(|(f, p)| format!("  {f} — `{p}`"))
        .collect();
    assert!(
        stale.is_empty(),
        "고정 목록에 있는데 트리에 없는 자리다 — 줄었으면 목록에서 **같이 지워라**. 남겨 두면 \
         그 자리에 같은 경로가 다시 들어와도 안 보인다:\n{}",
        stale.join("\n")
    );
}

/// GUI 크레이트 판정의 양성·음성 대조 — 생산 트리는 0 자리라 보고 갈래에 오늘 입력이 없다.
#[test]
fn the_gui_crate_reader_catches_the_spellings() {
    let crates: Vec<String> = ["egui", "winit", "image"].map(String::from).to_vec();
    let src = "\
#[cfg(feature = \"gui\")]
use egui::Context;
use ::winit::window::Window;
use egui as e;
use winit::{
    event::WindowEvent,
    dpi::PhysicalSize,
};
use {egui::Ui, std::fmt};
fn a() -> ::egui::Rect { todo!() }
use crate::image::Thumb;
fn b(s: &S) { s.egui.x(); super::image::y(); }
// egui::Painter 는 주석이다
fn c() { let _ = \"winit::x\"; }
use windows::Win32::UI::WindowsAndMessaging::SetFocus;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::{Graphics::Gdi::HDC, System::Console::X};
extern crate image;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32 as w32;
use windows::Win32::*;
use windows::Win32::System::*;
use windows::Win32::Foundation::{self, BOOL};
#[cfg(test)]
mod tests {
    use egui::Pos2;
}
";
    let got: Vec<(usize, String)> = gui_crate_references(src, &crates)
        .into_iter()
        .map(|(l, p, _)| (l, p))
        .collect();
    let want: Vec<(usize, String)> = vec![
        (2, "egui::Context"),
        (3, "winit::window::Window"),
        (4, "egui"),
        (6, "winit::event::WindowEvent"),
        (7, "winit::dpi::PhysicalSize"),
        (9, "egui::Ui"),
        (10, "egui::Rect"),
        (15, "windows::Win32::UI::WindowsAndMessaging::SetFocus"),
        (17, "windows::Win32::Graphics::Gdi::HDC"),
        (18, "image"),
        (19, "windows::Win32::Foundation::HWND"),
        (20, "windows::Win32"),
        (21, "windows::Win32"),
        (23, "windows::Win32::Foundation"),
    ]
    .into_iter()
    .map(|(l, p)| (l, p.to_string()))
    .collect();
    assert_eq!(
        got, want,
        "잡혀야 하는 것: gui 게이트 뒤(2) · 절대 경로(3 · 10) · `as` 별칭(4) · 여러 줄 중괄호(6 · 7) · \
         중괄호 루트(9) · 창·그리기 하위 경로(15 · 17) · `extern crate`(18) · Foundation 의 창 \
         핸들(19 의 `HWND`) · 창 경로의 진접두사에서 끝나는 별칭 · glob · `self`(20 · 21 · 23). \
         11–12 행(앞에 마디가 붙은 경로 · 필드)이 잡히면 첫 마디 판정이, 13–14 행이면 마스킹이, \
         16 · 17 행의 `System` 이나 16 · 19 행의 `HANDLE` · 22 행의 `System` glob · 23 행의 `BOOL` \
         이면 하위 경로 판정이, 26 행이면 test 필터가 죽은 것이다."
    );
}

/// 크레이트 목록을 매니페스트에서 읽는 판정의 형태 대조 — 여러 줄 배열 · 주석 속 항목 ·
/// 다른 크레이트의 feature 항목 · 한 줄에 둘.
#[test]
fn the_gui_crate_list_is_read_from_the_feature() {
    let manifest = "\
[dependencies]
gui = { version = \"1\" }

[features]
default = [\"gui\"]
gui = [
    \"dep:winit\",
    # \"dep:commented-out\"
    \"tasty-platform/gui\",
    \"dep:egui-wgpu\", \"dep:gtk\",
]
other = [\"dep:not-gui\"]
";
    assert_eq!(
        feature_enabled_deps(manifest, "gui"),
        vec!["winit", "egui-wgpu", "gtk"],
        "`[features]` 의 `gui` 배열에서 `dep:` 항목만 읽어야 한다 — 주석 속 항목 · \
         다른 크레이트의 feature · 다른 feature · 의존 절의 같은 이름은 빼고."
    );
}

/// 술어의 합성 양성 대조 — 생산 트리는 위반 0 이라 보고 갈래에 오늘 입력이 없다.
#[test]
fn the_predicates_catch_what_they_claim() {
    let src = "\
use crate::core::CoreState;
use crate::state::AppState;
// crate::app::App 은 주석이다
fn f() { let _s = \"crate::intent\"; }
fn g() -> crate::AppEvent { todo!() }
fn h() { super::super::adapters::x(); }
fn k() { super::super::core::x(); }
#[cfg(feature = \"gui\")]
fn gui_only() {}
// #[cfg(feature = \"gui\")] 주석 속 언급

#[cfg(test)]
mod tests {
    use crate::adapters::test::x;
    #[cfg(feature = \"gui\")]
    fn t() {}
}
";
    let got: Vec<(usize, &str)> = upper_references("src/core/agent/task.rs", src)
        .into_iter()
        .map(|(l, n, _)| (l, n))
        .collect();
    // `src/core/agent/task.rs` 의 모듈 깊이는 3 이라 `super::super::` 는 루트에 못 닿는다.
    assert_eq!(
        got,
        vec![(2, "state"), (5, "AppEvent")],
        "잡혀야 하는 것: 2 행(`crate::state`) · 5 행(`crate::AppEvent`). 3 행이 잡히면 주석 \
         마스킹이, 4 행이면 문자열 마스킹이, 14 행이면 인라인 test 필터가 죽은 것이다."
    );
    let got_root: Vec<(usize, &str)> = upper_references("src/core/attach.rs", src)
        .into_iter()
        .map(|(l, n, _)| (l, n))
        .collect();
    // 깊이 2 에서는 `super::super::` 가 크레이트 루트다 — 6 행만 추가로 잡히고 7 행
    // (`core`)은 도메인 자신이라 안 잡힌다.
    assert_eq!(
        got_root,
        vec![(2, "state"), (5, "AppEvent"), (6, "adapters")],
        "`super::` 사슬로 크레이트 루트에 올라가 상위 모듈을 부르는 형태가 안 잡힌다."
    );
    assert_eq!(
        gui_gates(src),
        1,
        "출하되는 코드의 gui 게이트는 8 행 하나다. 주석(10 행)이나 test 블록(15 행)이 세어지면 \
         마스킹이나 test 필터가 죽은 것이다."
    );
    assert_eq!(module_depth("src/core/mod.rs"), 1);
    assert_eq!(module_depth("src/core/attach.rs"), 2);
    assert_eq!(module_depth("src/core/agent/mod.rs"), 2);
    assert_eq!(module_depth("src/core/agent/task.rs"), 3);
    assert_eq!(module_depth("src/ports/clock.rs"), 2);

    // 정식 경로 · 중괄호 import · 줄을 넘는 경로의 양성 대조와, 같은 형제 모듈의 다른 하위
    // 항목이 안 잡히는 음성 대조. 한 줄짜리 첫 마디 판정은 이 형태들을 전부 놓쳤다.
    let src2 = "\
use crate::file::identify_worker::IdentifyWorker;
use crate::file::dispatch;
use crate::host_api::webview;
use crate::clipboard::ClipboardContext;
use crate::plugin::manager::PluginManager;
use crate::file::format::FileTarget;
use crate::host_api::hooks::global::GlobalHookManager;
use crate::ports::clipboard::ClipboardSystem;
use crate::{core::CoreState, state::AppState};
use crate::{file::{format::DetectDepth, dispatch::open_picker}};
use crate::
    app::App;
use crate::{
    core::origin::IntentOrigin,
    intent::Intent,
};
fn m() { crate :: view :: x(); }
fn n() -> crate::core::origin::FileDispatchOrigin { todo!() }
";
    let got2: Vec<(usize, &str)> = upper_references("src/core/attach.rs", src2)
        .into_iter()
        .map(|(l, n, _)| (l, n))
        .collect();
    assert_eq!(
        got2,
        vec![
            (1, "file::identify_worker"),
            (2, "file::dispatch"),
            (3, "host_api::webview"),
            (4, "clipboard"),
            (5, "plugin"),
            (9, "state"),
            (10, "file::dispatch"),
            (12, "app"),
            (15, "intent"),
            (17, "view"),
        ],
        "정식 경로(1–4 행) · plugin 별칭(5 행) · 중괄호 항목(9 · 10 행) · 줄을 넘는 경로(11–12 행 — \
         줄 번호는 항목 자리인 12 행) · 여러 줄 중괄호(15 행) · 마디 사이 공백(17 행)이 잡혀야 \
         한다. 6–8 · 18 행이 잡히면 앞마디 일치가 아니라 부분 일치가 된 것이다."
    );
}
