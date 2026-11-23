//! 제품 상태 시험: App plugin 단축키 단계(`plugin_shortcut_in_window`)를 창 상태와 plugin 관리자로 부른다. winit 키는
//! 사건 값(`PluginKeyGate::Winit`)으로, native WebView 가 넘긴 키는 그 입구의 분류와 받은 surface
//! (`PluginKeyGate::Forwarded`)로 부르고, 실행된 명령은 그 명령의 action(plugin 팝업 열기)이 남긴 열기 요청
//! (`pending_popup_opens`의 팝업 id·대상 surface)으로 본다.
//!
//! 범위 밖: App 의 창 조회·modal 판정(`dispatch_plugin_shortcut_key`), action 이 없는 명령의 plugin 요청, 실제 팝업 열기.

use std::sync::Arc;

use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};

use super::{PluginKeyGate, plugin_shortcut_in_window};
use crate::adapters::ui::input::key_class::{KeyEventValue, egui_modifiers};
use crate::adapters::ui::input::key_nodes::ForwardedKey;
use crate::adapters::ui::popup::PopupScope;
use crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID;
use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
use crate::plugin::PluginManager;
use crate::state::MainViewState;

const PLUGIN: &str = "com.example.key-gate";
/// plugin surface A 의 id. 같은 pane 의 첫 탭이다.
const A: u32 = 1;
/// 터미널 surface B 의 id. 같은 pane 의 둘째 탭이다.
const B: u32 = 2;

/// 명령과 조합. 같은 조합 `ctrl+f9`에 Surface → Global 순서(매니페스트 순서)로 둘을 묶는다. 기본 프리셋의 host
/// 바인딩과 겹치지 않는다. 명령마다 action 이 `PLUGIN/<명령>` 팝업을 연다.
const COMMANDS: [(&str, &str, &str); 5] = [
    ("surf", "surface", "ctrl+f9"),
    ("glob", "global", "ctrl+f9"),
    ("glob_only", "global", "ctrl+f10"),
    ("surf_only", "surface", "ctrl+f11"),
    ("esc", "global", "escape"),
];

/// 같은 pane 에 plugin surface A(첫 탭)와 터미널 B(둘째 탭)가 있는 창. `focus` 탭을 고른다.
fn scene(focus: u32) -> (MainViewState, crate::runtime::engine_session::EngineSession) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    crate::i18n::init("en");
    let mut events = vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: 1,
            name: "w".into(),
            category: 0,
            index: 0,
            pane: 1,
        },
    ];
    for (index, (id, kind)) in [(A, "demo"), (B, "terminal")].into_iter().enumerate() {
        events.push(E::TabCreated {
            id,
            pane: 1,
            index,
            name: kind.into(),
            surface: SurfaceSpec {
                id,
                kind: kind.into(),
                data: None,
            },
        });
    }
    let (mut state, mut session) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    session.runtime.surfaces.insert(
        A,
        Box::new(crate::plugin_bridge::remote_surface::RemoteSurface::new(
            A,
            "demo",
            PLUGIN.into(),
            "demo".into(),
        )),
    );
    state.goto_tab_in_pane(&session.core_state, usize::from(focus == B));
    assert_eq!(state.focused_surface_id(&session.read()), Some(focus));
    (state, session)
}

/// `COMMANDS`를 등록한 관리자.
fn manager() -> PluginManager {
    let mut mgr = PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let mut toml = format!(
        "manifest_version = 1\nid = \"{PLUGIN}\"\nname = \"Key gate\"\nversion = \"1.0.0\"\n\
         api_version = \"{}\"\n[entry]\ntype = \"process\"\ncommand = \"echo\"\n",
        crate::plugin::manifest::HOST_API_VERSION
    );
    for (id, scope, combo) in COMMANDS {
        toml.push_str(&format!(
            "[[contributes.commands]]\nid = \"{id}\"\ntitle_i18n_key = \"{id}.title\"\n\
             default_keybinding = \"{combo}\"\nscope = \"{scope}\"\n\
             action = {{ kind = \"open_popup\", popup_id = \"{PLUGIN}/{id}\" }}\n"
        ));
    }
    let manifest: tasty_plugin_manifest::Manifest = toml::from_str(&toml).expect("manifest");
    mgr.install_packages_for_test(vec![tasty_host_plugin::PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/key-gate"),
        manifest: manifest.clone(),
    }]);
    mgr.command_registry.register_plugin(&manifest);
    mgr
}

fn f_key(n: u8) -> (Key, KeyCode) {
    let (named, code) = match n {
        9 => (NamedKey::F9, KeyCode::F9),
        10 => (NamedKey::F10, KeyCode::F10),
        11 => (NamedKey::F11, KeyCode::F11),
        _ => unreachable!(),
    };
    (Key::Named(named), code)
}

fn escape() -> (Key, KeyCode) {
    (Key::Named(NamedKey::Escape), KeyCode::Escape)
}

/// winit 키 하나를 App plugin 단축키 단계로 처리한다. 처리했으면 참이다.
fn winit_key(
    state: &mut MainViewState,
    session: &mut crate::runtime::engine_session::EngineSession,
    mgr: &mut PluginManager,
    (logical, code): (Key, KeyCode),
    mods: ModifiersState,
) -> bool {
    let engine = session.borrow_mut();
    let value = KeyEventValue {
        logical: &logical,
        physical: PhysicalKey::Code(code),
        pressed: true,
        mods,
        egui_mods: egui_modifiers(mods),
    };
    let lookup = crate::shortcuts::shortcut_lookup_key(&logical, &value.physical, mods);
    plugin_shortcut_in_window(
        state,
        &engine,
        Some(mgr),
        |_| false,
        &lookup,
        mods,
        PluginKeyGate::Winit(&value),
    )
}

/// `sid` 가 넘긴 키 하나를 그 입구의 분류 `class` 로 App plugin 단축키 단계에 넣는다. 처리했으면 참이다.
fn forwarded_key(
    state: &mut MainViewState,
    session: &mut crate::runtime::engine_session::EngineSession,
    mgr: &mut PluginManager,
    (logical, _): (Key, KeyCode),
    mods: ModifiersState,
    class: ForwardedKey,
    sid: u32,
) -> bool {
    let engine = session.borrow_mut();
    plugin_shortcut_in_window(
        state,
        &engine,
        Some(mgr),
        |_| false,
        &logical,
        mods,
        PluginKeyGate::Forwarded(class, sid),
    )
}

/// 실행된 명령(열기 요청의 팝업 이름)과 대상 surface 를 꺼낸다.
fn ran(state: &mut MainViewState) -> Vec<(String, Option<u32>)> {
    std::mem::take(&mut state.pending_popup_opens)
        .into_iter()
        .map(|open| {
            let name = open
                .popup_id
                .strip_prefix(&format!("{PLUGIN}/"))
                .unwrap_or(&open.popup_id)
                .to_string();
            (name, open.target_surface)
        })
        .collect()
}

fn names(state: &mut MainViewState) -> Vec<String> {
    ran(state).into_iter().map(|(name, _)| name).collect()
}

/// 창 범위 명령 팔레트(범위를 덮는 `Text` 팝업)를 `scope`에 포커스로 연다.
fn open_palette(
    state: &mut MainViewState,
    session: &crate::runtime::engine_session::EngineSession,
    scope: PopupScope,
) {
    state
        .popups
        .open_centered_focused(COMMAND_PALETTE_POPUP_ID, scope)
        .expect("팔레트가 열린다");
    state.normalize_focus(&session.read());
    assert!(matches!(
        state.focus_node(&session.read()),
        crate::state::focus_node::FocusNode::Popup(_)
    ));
}

/// `sid` 에 붙은 검색 띠(덮지 않는 `Text` 팝업)를 포커스로 연다.
fn open_search(
    state: &mut MainViewState,
    session: &crate::runtime::engine_session::EngineSession,
    sid: u32,
) {
    state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(sid))
                .focused(),
            true,
        )
        .expect("검색 띠가 열린다");
    state.normalize_focus(&session.read());
    assert!(matches!(
        state.focus_node(&session.read()),
        crate::state::focus_node::FocusNode::Popup(_)
    ));
}

const CTRL: ModifiersState = ModifiersState::CONTROL;
const NONE: ModifiersState = ModifiersState::empty();

/// T-12b-6 (가): plugin surface A 를 덮는 팝업이 포커스. 같은 조합의 Surface → Global 명령에서 Surface 는 경로에
/// 없어 걸러지고 Global 이 돈다.
#[test]
fn a_covered_plugin_surface_runs_only_its_global_command() {
    let (mut state, mut session) = scene(A);
    let mut mgr = manager();
    open_palette(&mut state, &session, PopupScope::Window);
    assert!(winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(9),
        CTRL
    ));
    assert_eq!(names(&mut state), ["glob"]);
}

/// T-12b-6 (나): 같은 덮음에서 Global 전용 조합은 돈다.
#[test]
fn a_covered_plugin_surface_runs_a_global_only_command() {
    let (mut state, mut session) = scene(A);
    let mut mgr = manager();
    open_palette(&mut state, &session, PopupScope::Window);
    assert!(winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(10),
        CTRL
    ));
    assert_eq!(names(&mut state), ["glob_only"]);
}

/// T-12b-6 (가) 대조: 덮는 팝업이 없으면 같은 조합에서 매니페스트 순서의 첫 명령(Surface)이 돈다.
#[test]
fn an_uncovered_plugin_surface_runs_its_first_command() {
    let (mut state, mut session) = scene(A);
    let mut mgr = manager();
    assert!(winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(9),
        CTRL
    ));
    assert_eq!(ran(&mut state), [("surf".to_string(), Some(A))]);
}

/// T-12b-6 (다): 검색 띠(덮지 않는 `Text`)가 포커스여도 그 팝업이 쓰지 않는 plugin Global 명령은 돈다.
#[test]
fn a_search_bar_focus_lets_an_unused_plugin_global_command_run() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    open_search(&mut state, &session, B);
    assert!(winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(10),
        CTRL
    ));
    assert_eq!(names(&mut state), ["glob_only"]);
}

/// T-12b-6 (라): 무대에서 무대 종료 키와 같은 조합의 plugin Global 은 돌지 않는다(무대 종료 단계로 간다). 다른
/// 조합의 plugin Global 은 돌고, 같은 조합의 Surface 명령은 경로에 없다.
#[test]
fn a_stage_keeps_its_exit_key_from_a_plugin_global_command() {
    let (mut state, mut session) = scene(A);
    {
        let engine = session.borrow_mut();
        assert!(
            engine
                .runtime
                .settings
                .keybindings
                .set_field("fullscreen_stage_exit", "ctrl+f10")
        );
    }
    let mut mgr = manager();
    assert!(state.open_fullscreen_stage("blank"));
    assert!(!winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(10),
        CTRL
    ));
    assert!(names(&mut state).is_empty(), "무대 종료 키");
    assert!(winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(9),
        CTRL
    ));
    assert_eq!(names(&mut state), ["glob"], "무대가 쓰지 않는 키");
}

/// T-Esc-1: Esc 를 `Ignore`로 선언한 팝업이 포커스면 plugin 의 Esc 명령은 돌지 않는다(bare Esc 는 지금 게이트).
#[test]
fn an_ignoring_popup_keeps_a_bare_escape_from_a_plugin_command() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    state
        .popups
        .open_centered_focused(
            crate::adapters::ui::popup::explorer_conflict::EXPLORER_CONFLICT_POPUP_ID,
            PopupScope::Window,
        )
        .expect("충돌 질문이 열린다");
    state.normalize_focus(&session.read());
    assert!(!winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        escape(),
        NONE
    ));
    assert!(names(&mut state).is_empty());
}

/// T-Esc-1: 튜토리얼 말풍선이 키보드를 가지면 bare Esc 는 지금 게이트가 닫아 plugin 의 Esc 명령이 돌지 않는다.
/// 말풍선 노드는 Esc 를 끝내지 않으므로(`key_route::start_node_ends_key`) bare Esc 에 경로 게이트를 쓰면 갈리는
/// 장면이다. 포커스할 수 있는 `Ignore` 팝업(explorer_conflict)은 `Text`라 경로 게이트도 Esc 를 끝내 갈리지 않는다.
#[test]
fn a_tutorial_keeps_a_bare_escape_from_a_plugin_command() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    state.tutorial.request_start(0);
    state.tutorial.start_pending();
    assert!(state.tutorial.keyboard_focus);
    assert_eq!(
        state.focus_node(&session.read()),
        crate::state::focus_node::FocusNode::Tutorial
    );
    assert!(!winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        escape(),
        NONE
    ));
    assert!(names(&mut state).is_empty());
}

/// T-Esc-1 대조: 터미널 포커스에서 plugin Esc 명령은 돈다.
#[test]
fn a_terminal_focus_lets_a_plugin_escape_command_run() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    assert!(winit_key(
        &mut state,
        &mut session,
        &mut mgr,
        escape(),
        NONE
    ));
    assert_eq!(names(&mut state), ["esc"]);
}

/// T-12b-18: 오버레이 없음, 모델 포커스 B. plugin surface A 가 넘긴 A 의 Surface 범위 명령은 A 에서 돈다.
#[test]
fn a_forwarded_surface_command_runs_on_the_receiving_surface() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    let key = f_key(11);
    assert!(forwarded_key(
        &mut state,
        &mut session,
        &mut mgr,
        key,
        CTRL,
        ForwardedKey::Path,
        A
    ));
    assert_eq!(ran(&mut state), [("surf_only".to_string(), Some(A))]);
}

/// T-12b-18 대조(M-12b-19 P): 모델 포커스도 A 면 같은 명령이 A 에서 돈다.
#[test]
fn a_forwarded_surface_command_runs_when_the_focus_is_the_receiving_surface() {
    let (mut state, mut session) = scene(A);
    let mut mgr = manager();
    assert!(forwarded_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(11),
        CTRL,
        ForwardedKey::Path,
        A
    ));
    assert_eq!(ran(&mut state), [("surf_only".to_string(), Some(A))]);
}

/// T-12b-18 (C12b-2): 다른 곳(B)의 검색 띠가 포커스여도 A 가 넘긴 경로 키의 명령은 A 에서 돈다.
#[test]
fn a_search_bar_elsewhere_lets_a_forwarded_command_run() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    open_search(&mut state, &session, B);
    assert!(forwarded_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(11),
        CTRL,
        ForwardedKey::Path,
        A
    ));
    assert_eq!(ran(&mut state), [("surf_only".to_string(), Some(A))]);
}

/// T-12b-18 (C12b-2): 덮음은 받은 surface 에서 본다. 모델 포커스 B 만 덮는 팝업은 A 가 넘긴 명령을 막지 않는다.
#[test]
fn a_cover_over_another_surface_lets_a_forwarded_command_run() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    open_palette(&mut state, &session, PopupScope::Surface(B));
    assert!(forwarded_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(11),
        CTRL,
        ForwardedKey::Path,
        A
    ));
    assert_eq!(ran(&mut state), [("surf_only".to_string(), Some(A))]);
}

/// T-12b-18 대조: A 를 덮는 팝업은 A 가 넘긴 명령을 막는다.
#[test]
fn a_cover_over_the_receiving_surface_gates_a_forwarded_command() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    open_palette(&mut state, &session, PopupScope::Window);
    assert!(!forwarded_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(11),
        CTRL,
        ForwardedKey::Path,
        A
    ));
    assert!(ran(&mut state).is_empty());
}

/// T-12b-7 w3 직접 호출: 지금 경로로 남는 분류(클립보드 조합 등)는 지금 게이트라 다른 곳의 덮지 않는 팝업
/// 포커스에서 닫힌다. 같은 상태의 경로 키 분류는 `a_search_bar_elsewhere_lets_a_forwarded_command_run`이 본다.
#[test]
fn a_forwarded_current_key_keeps_the_window_gate() {
    let (mut state, mut session) = scene(B);
    let mut mgr = manager();
    open_search(&mut state, &session, B);
    assert!(!forwarded_key(
        &mut state,
        &mut session,
        &mut mgr,
        f_key(11),
        CTRL,
        ForwardedKey::Current,
        A
    ));
    assert!(ran(&mut state).is_empty());
}
