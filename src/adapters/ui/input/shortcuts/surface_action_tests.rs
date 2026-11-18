//! 입력 대상이 생성 대기 초안일 때 내장 surface 액션을 거는 판정과, 생성된 surface 에서의 실행.

use super::{
    PaletteRun, Received, hold_for_draft, run_held_action, run_palette_action, run_received_key,
};
use crate::adapters::ui::tools_menu::run_deferred_tools;
use crate::intent::Intent;
use crate::intent::RefreshTrigger::Shortcut;
use crate::state::attach_pending::deferred::{CommandSurfaceTarget, settle_first_created};
use crate::state::tests::{select_pending_draft, test_mirror_state};

// 설정의 alt 토큰은 키 위치로 읽는다. macOS 에서는 Command(winit SUPER), 다른 OS 에서는 Alt 다.
#[cfg(target_os = "macos")]
const BINDING_ALT: winit::keyboard::ModifiersState = winit::keyboard::ModifiersState::SUPER;
#[cfg(not(target_os = "macos"))]
const BINDING_ALT: winit::keyboard::ModifiersState = winit::keyboard::ModifiersState::ALT;

fn pending(target: Option<CommandSurfaceTarget>) -> crate::state::attach_pending::PendingId {
    match target {
        Some(CommandSurfaceTarget::Pending(id)) => id,
        other => panic!("expected a draft target, got {other:?}"),
    }
}

fn converts(intents: &[crate::intent::DispatchedIntent], sid: u32) -> bool {
    intents
        .iter()
        .any(|i| matches!(&i.body, Intent::ConvertSurface { surface_id, .. } if *surface_id == sid))
}

/// 터미널 새 탭 초안 선택 중 `find` 는 초안에 걸리고, 생성된 터미널의 검색창으로 열린다.
#[test]
fn find_for_a_terminal_draft_opens_search_on_the_created_surface() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let anchor = state.focused_surface_id(&read).expect("focused surface");
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_surface_target(&read);
    let id = pending(target);
    assert_eq!(
        hold_for_draft(&mut state, &read, "find", target, Shortcut),
        Some(true)
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 1);
    assert_ne!(state.search.surface_id, anchor);
    settle_first_created(&mut state, &engine.core_state, anchor);
    assert!(run_deferred_tools(&mut state, &read).is_empty());
    assert_eq!(state.search.surface_id, anchor);
}

/// 변환 액션도 걸어 두었다가 생성된 surface 를 대상으로 요청한다.
#[test]
fn convert_to_explorer_for_a_draft_targets_the_created_surface() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let anchor = state.focused_surface_id(&read).expect("focused surface");
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_surface_target(&read);
    assert_eq!(
        hold_for_draft(&mut state, &read, "convert_to_explorer", target, Shortcut),
        Some(true)
    );
    assert!(!converts(&state.take_pending_intents(), anchor));
    settle_first_created(&mut state, &engine.core_state, anchor);
    run_deferred_tools(&mut state, &read);
    assert!(converts(&state.take_pending_intents(), anchor));
}

/// 초안 kind 가 받지 않는 액션(터미널 초안의 탐색기 액션)은 걸지 않고 처리한 것으로 본다.
/// 초안 대상이 아니거나 surface 액션이 아니면 호출자에게 넘긴다.
#[test]
fn a_draft_takes_only_actions_for_its_kind() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_surface_target(&read);
    let id = pending(target);
    for action in ["explorer_refresh", "copy_path"] {
        assert_eq!(
            hold_for_draft(&mut state, &read, action, target, Shortcut),
            Some(false),
            "{action}"
        );
    }
    assert_eq!(
        hold_for_draft(&mut state, &read, "new_tab", target, Shortcut),
        None
    );
    assert_eq!(
        hold_for_draft(&mut state, &read, "find", None, Shortcut),
        None
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 0);
}

/// 입력 뒤 초안이 실패로 끝났으면 늦게 거는 일 없이 버린다.
#[test]
fn an_action_for_a_finished_draft_is_dropped() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let draft = select_pending_draft(&mut state, &read);
    let target = state.command_surface_target(&read);
    let id = pending(target);
    let entry = &state.attach_pending.entries[0];
    entry.receipt.bind(0);
    let epoch = entry.epoch.clone();
    state.attach_pending.result(&epoch, 0, None, false);
    assert_eq!(
        run_palette_action(&mut state, &read, "find", target),
        PaletteRun::Done
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 0);
    drop(draft);
}

/// 실행 때 생성된 surface 의 kind 가 접수 때와 다르면 실행하지 않는다.
#[test]
fn a_held_action_is_not_run_on_a_surface_of_another_kind() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let anchor = state.focused_surface_id(&read).expect("focused surface");
    let before = state.search.surface_id;
    run_held_action(&mut state, &read, "find", "explorer", Shortcut, anchor);
    assert_eq!(state.search.surface_id, before);
    run_held_action(&mut state, &read, "find", "terminal", Shortcut, anchor);
    assert_eq!(state.search.surface_id, anchor);
}

/// 한 workspace 에 터미널 탭 둘(surface 11, 12).
pub(crate) fn two_tabs() -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
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
    for (index, id) in [11, 12].into_iter().enumerate() {
        events.push(E::TabCreated {
            id,
            pane: 1,
            index,
            name: "t".into(),
            surface: SurfaceSpec {
                id,
                kind: "terminal".into(),
                data: None,
            },
        });
    }
    crate::state::tests::test_state_from_model(crate::state::tests::test_model(events))
}

/// 팔레트는 Enter 때 고른 surface 에서 실행한다. 그 사이 포커스가 바뀌어도 다시 읽지 않는다.
#[test]
fn a_palette_action_runs_on_the_surface_bound_at_input() {
    let (mut state, engine) = two_tabs();
    let read = engine.read();
    let focused = state.focused_surface_id(&read).expect("focused surface");
    let other = if focused == 11 { 12 } else { 11 };
    assert_eq!(
        run_palette_action(
            &mut state,
            &read,
            "convert_to_explorer",
            Some(CommandSurfaceTarget::Surface(other)),
        ),
        PaletteRun::Done
    );
    let intents = state.take_pending_intents();
    assert!(converts(&intents, other) && !converts(&intents, focused));
    let gone = 987_654;
    assert_eq!(
        run_palette_action(
            &mut state,
            &read,
            "convert_to_explorer",
            Some(CommandSurfaceTarget::Surface(gone)),
        ),
        PaletteRun::Done
    );
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(
        run_palette_action(
            &mut state,
            &read,
            "new_tab",
            Some(CommandSurfaceTarget::Surface(focused)),
        ),
        PaletteRun::Dispatch
    );
}

/// 팔레트 대상 surface 의 kind 가 받지 않는 액션은 실행하지 않고 끝낸다. 액션 ID 처리로 넘기면 Enter 뒤
/// 바뀐 포커스(탐색기 B)를 다시 읽어 그쪽에서 실행하게 된다.
#[test]
fn a_palette_action_for_another_kind_does_not_follow_a_later_focus() {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
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
    for (index, (id, kind)) in [(11, "terminal"), (12, "explorer")].into_iter().enumerate() {
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
    let (mut state, engine) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    let read = engine.read();
    let terminal = Some(CommandSurfaceTarget::Surface(11));
    state.goto_tab_in_pane(&engine.core_state, 1);
    assert_eq!(state.focused_surface_id(&read), Some(12));
    assert_eq!(
        run_palette_action(&mut state, &read, "explorer_go_up", terminal),
        PaletteRun::Done
    );
    assert!(state.take_pending_intents().is_empty());
}

/// html 탭 하나(surface 1).
fn html_tab() -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    webview_tab(
        "html",
        crate::runtime::surface_registry::KindCapabilities::undeclared("html", "com.tasty.html"),
    )
}

/// WebView kind `kind` 의 탭 하나(surface 1). 그 surface 는 `capabilities` 를 싣는다.
fn webview_tab(
    kind: &'static str,
    capabilities: crate::runtime::surface_registry::KindCapabilities,
) -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    let model = crate::state::tests::test_model(vec![
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
        E::TabCreated {
            id: 1,
            pane: 1,
            index: 0,
            name: kind.into(),
            surface: SurfaceSpec {
                id: 1,
                kind: kind.into(),
                data: None,
            },
        },
    ]);
    let (state, mut session) = crate::state::tests::test_state_from_model(model);
    session.runtime.surfaces.insert(
        1,
        Box::new(
            crate::plugin_bridge::remote_surface::RemoteSurface::new(
                1,
                kind,
                "com.example.web".into(),
                kind.into(),
            )
            .with_capabilities(capabilities),
        ),
    );
    (state, session)
}

/// 팔레트의 주소창 포커스는 대상이 html 이면 그 html 의 주소 띠에 포커스를 요청한다.
#[test]
fn a_palette_address_focus_on_html_goes_to_its_address_band() {
    let (mut state, engine) = html_tab();
    let read = engine.read();
    assert_eq!(
        run_palette_action(
            &mut state,
            &read,
            "explorer_focus_address",
            Some(CommandSurfaceTarget::Surface(1)),
        ),
        PaletteRun::Done
    );
    assert_eq!(state.html_address.focus_requested(), Some(1));
}

/// 주소창 포커스는 kind 이름이 아니라 주소 띠 능력을 본다. 띠를 선언한 plugin kind 는 html 이 아니어도 받고,
/// 띠를 끈 html 은 받지 않는다.
#[test]
fn the_address_focus_follows_the_declared_address_band() {
    use crate::runtime::surface_registry::KindCapabilities;
    for (kind, address_band) in [("web", true), ("html", false)] {
        let capabilities = KindCapabilities {
            address_band,
            ..KindCapabilities::default()
        };
        let (mut state, engine) = webview_tab(kind, capabilities);
        let read = engine.read();
        assert_eq!(
            run_palette_action(
                &mut state,
                &read,
                "explorer_focus_address",
                Some(CommandSurfaceTarget::Surface(1)),
            ),
            PaletteRun::Done,
            "{kind}"
        );
        let expected = address_band.then_some(1);
        assert_eq!(state.html_address.focus_requested(), expected, "{kind}");
    }
}

/// 초안에 거는 주소창 포커스도 초안이 만들 kind 가 등록 때 선언한 주소 띠 능력으로 판정한다.
#[test]
fn a_draft_takes_the_address_focus_by_the_declared_address_band() {
    for (kind, address_band) in [("webband", true), ("webplain", false)] {
        let (mut state, engine, sid, _dir) = crate::state::tests::explorer_fixture(true);
        let decl: tasty_plugin_manifest::SurfaceKindDecl =
            serde_json::from_value(serde_json::json!({
                "kind": kind,
                "display_name_i18n_key": "surface.kind.markdown",
                "rendering": "webview",
                "address_band": address_band,
            }))
            .expect("test SurfaceKindDecl");
        let (host_cmd_tx, _host_cmd_rx) = std::sync::mpsc::channel();
        crate::plugin_bridge::remote_kind::register_remote_kind(
            &engine.runtime.surface_registry,
            "com.example.web",
            &decl,
            host_cmd_tx,
        );
        let read = engine.read();
        let (sender, _queue) = tasty_remote::connection::channel();
        let view = std::sync::Arc::new(());
        state
            .attach_pending
            .begin(
                &read,
                &state.navigation,
                &crate::ipc::stream::StructuralOp::NewTab {
                    anchor_surface_id: sid,
                    surface_kind: kind.into(),
                    params: serde_json::json!({}),
                    inherit_cwd: None,
                },
                sender.epoch(),
                std::sync::Arc::downgrade(&view),
                std::time::Instant::now(),
            )
            .expect("the draft begins");
        let target = state.command_surface_target(&read);
        assert_eq!(
            hold_for_draft(
                &mut state,
                &read,
                "explorer_focus_address",
                target,
                Shortcut
            ),
            Some(address_band),
            "{kind}"
        );
    }
}

/// surface 하나를 대상으로 하는 팔레트 액션은 모두 Enter 때 고른 surface 에서 실행한다. 그 뒤 초안을 골라
/// 포커스가 없어져도 다시 읽지 않는다. 창이 실행할 액션은 그 surface 와 함께 넘긴다.
#[test]
fn every_palette_surface_action_keeps_the_surface_bound_at_input() {
    let (mut state, engine, sid, _dir) = crate::state::tests::explorer_fixture(true);
    let read = engine.read();
    let target = Some(CommandSurfaceTarget::Surface(sid));
    super::ensure_explorer_view(&mut state, &read, sid);
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = std::sync::Arc::new(());
    state
        .attach_pending
        .begin(
            &read,
            &state.navigation,
            &crate::ipc::stream::StructuralOp::NewTab {
                anchor_surface_id: sid,
                surface_kind: "terminal".into(),
                params: serde_json::json!({}),
                inherit_cwd: None,
            },
            sender.epoch(),
            std::sync::Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .expect("the draft begins");
    assert_eq!(state.focused_surface_id(&read), None);
    assert_eq!(
        run_palette_action(&mut state, &read, "select_all", target),
        PaletteRun::Done
    );
    assert_eq!(
        run_palette_action(&mut state, &read, "copy_path", target),
        PaletteRun::View("copy_path", sid)
    );
}

/// 선택 항목·복사 모드처럼 갓 생성된 surface 의 실제 상태로 실행할 액션도 초안에 건다. 창이 필요한 액션은
/// 생성 뒤 그 창에 surface 와 함께 넘긴다.
#[test]
fn state_dependent_actions_are_held_and_run_on_the_created_surface() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let anchor = state.focused_surface_id(&read).expect("focused surface");
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_surface_target(&read);
    let id = pending(target);
    assert_eq!(
        hold_for_draft(&mut state, &read, "enter_copy_mode", target, Shortcut),
        Some(true)
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 1);
    settle_first_created(&mut state, &engine.core_state, anchor);
    run_deferred_tools(&mut state, &read);
    assert_eq!(
        state.attach_pending.deferred.take_view(),
        vec![("enter_copy_mode", anchor)]
    );
}

/// 탐색기 초안은 이동 이력·새 항목·선택 액션도 건다.
#[test]
fn an_explorer_draft_holds_history_create_and_selection_actions() {
    let (mut state, engine, sid, _dir) = crate::state::tests::explorer_fixture(true);
    let read = engine.read();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = std::sync::Arc::new(());
    state
        .attach_pending
        .begin(
            &read,
            &state.navigation,
            &crate::ipc::stream::StructuralOp::NewTab {
                anchor_surface_id: sid,
                surface_kind: "explorer".into(),
                params: serde_json::json!({}),
                inherit_cwd: None,
            },
            sender.epoch(),
            std::sync::Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .expect("the draft begins");
    let target = state.command_surface_target(&read);
    let id = pending(target);
    for action in [
        "select_all",
        "copy_path",
        "explorer_back",
        "explorer_forward",
        "explorer_new_folder",
        "explorer_new_file",
        "explorer_rename",
        "explorer_trash",
        "explorer_undo",
        "explorer_redo",
    ] {
        assert_eq!(
            hold_for_draft(&mut state, &read, action, target, Shortcut),
            Some(true),
            "{action}"
        );
    }
    for action in ["copy", "cut", "paste"] {
        assert_eq!(
            hold_for_draft(&mut state, &read, action, target, Shortcut),
            None,
            "{action}"
        );
    }
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 10);
}

/// 초안 A 에 건 변환은 그 사이 초안 B 를 골라 둔 채 A 가 생성돼도 거절되지 않고 A 를 변환한다.
#[test]
fn a_held_convert_runs_while_another_draft_is_selected() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let anchor = state.focused_surface_id(&read).expect("focused surface");
    let _first = select_pending_draft(&mut state, &read);
    let target = state.command_surface_target(&read);
    assert_eq!(
        hold_for_draft(&mut state, &read, "convert_to_explorer", target, Shortcut),
        Some(true)
    );
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = std::sync::Arc::new(());
    state
        .attach_pending
        .begin(
            &read,
            &state.navigation,
            &crate::ipc::stream::StructuralOp::NewTab {
                anchor_surface_id: anchor,
                surface_kind: "terminal".into(),
                params: serde_json::json!({}),
                inherit_cwd: None,
            },
            sender.epoch(),
            std::sync::Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .expect("the second draft begins");
    settle_first_created(&mut state, &engine.core_state, anchor);
    run_deferred_tools(&mut state, &read);
    assert_ne!(state.command_surface_target(&read), target);
    assert!(matches!(
        state.command_surface_target(&read),
        Some(CommandSurfaceTarget::Pending(_))
    ));
    let convert = state
        .take_pending_intents()
        .into_iter()
        .find(|i| matches!(i.body, Intent::ConvertSurface { .. }))
        .expect("the held convert is requested");
    drop(read);
    let resolved = crate::app::creation_intent::resolve(
        &state,
        &engine.as_ref(),
        &convert.body,
        &convert.origin,
    )
    .expect("a convert of the created surface is not refused")
    .domain();
    assert!(matches!(
        resolved,
        Some(crate::app::command::DomainIntent::ConvertSurface { surface_id, .. }) if surface_id == anchor
    ));
}

/// 팔레트의 전체 선택은 대상이 html 이면 그 surface 의 페이지 편집으로 창에 넘긴다.
#[test]
fn a_palette_select_all_on_html_stays_on_that_page() {
    let (mut state, engine) = html_tab();
    let read = engine.read();
    assert_eq!(
        run_palette_action(
            &mut state,
            &read,
            "select_all",
            Some(CommandSurfaceTarget::Surface(1)),
        ),
        PaletteRun::View("select_all", 1)
    );
}

/// 창 액션 뒤에 건 같은 초안의 명령은 그 창 액션이 실행된 뒤에 실행한다.
#[test]
fn held_commands_keep_their_order_around_a_window_action() {
    for (first, then) in [
        ("copy_path", "select_all"),
        ("explorer_rename", "explorer_go_up"),
    ] {
        let (mut state, engine, sid, _dir) = crate::state::tests::explorer_fixture(true);
        let read = engine.read();
        let (sender, _queue) = tasty_remote::connection::channel();
        let view = std::sync::Arc::new(());
        state
            .attach_pending
            .begin(
                &read,
                &state.navigation,
                &crate::ipc::stream::StructuralOp::NewTab {
                    anchor_surface_id: sid,
                    surface_kind: "explorer".into(),
                    params: serde_json::json!({}),
                    inherit_cwd: None,
                },
                sender.epoch(),
                std::sync::Arc::downgrade(&view),
                std::time::Instant::now(),
            )
            .expect("the draft begins");
        let target = state.command_surface_target(&read);
        for action in [first, then] {
            assert_eq!(
                hold_for_draft(&mut state, &read, action, target, Shortcut),
                Some(true),
                "{action}"
            );
        }
        settle_first_created(&mut state, &engine.core_state, sid);
        run_deferred_tools(&mut state, &read);
        assert!(
            state.attach_pending.deferred.take_ready().is_empty(),
            "{then} waits for {first}"
        );
        assert_eq!(
            state.attach_pending.deferred.take_view(),
            vec![(first, sid)]
        );
        let rest = state.attach_pending.deferred.take_ready();
        assert!(
            matches!(rest.as_slice(), [(local, crate::state::attach_pending::deferred::DeferredCommand::Action { id, .. })] if *local == sid && *id == then),
            "{first} then {then}"
        );
    }
}

/// 키·더블탭·팔레트 입력 경로가 surface 액션마다 초안 판정을 먼저 부른다.
#[test]
fn every_input_path_asks_the_draft_first() {
    const KEYS: &str = include_str!("keybinding.rs");
    const DOUBLE_TAP: &str = include_str!("double_tap.rs");
    const REDRAW: &str = include_str!("../../../../view/main/redraw.rs");
    // 줄바꿈 위치는 rustfmt 가 정하므로 공백을 빼고 비교한다.
    let flat = |src: &str| src.split_whitespace().collect::<String>();
    let (keys, double_tap) = (flat(KEYS), flat(DOUBLE_TAP));
    for id in [
        "find",
        "convert_surface",
        "convert_to_markdown",
        "convert_to_explorer",
        "refresh",
        "explorer_refresh",
        "explorer_go_up",
        "explorer_focus_address",
    ] {
        assert!(
            keys.contains(&flat(&format!(
                "held_for_draft(state, engine, t, \"{id}\")"
            ))),
            "keybinding.rs {id}"
        );
    }
    assert!(keys.contains(&flat("held_for_draft(state, engine, t, action)")));
    assert!(keys.contains(&flat(
        "held_for_draft(state, engine, t, \"enter_copy_mode\")"
    )));
    assert!(keys.contains(&flat(
        "(&kb.explorer_redo, \"explorer_redo\"), ] { if matches_any_binding(binding, key, mods) \
         && super::surface_actions::held_for_draft(state, engine, t, action)"
    )));
    const COPY_PASTE: &str = include_str!("copy_paste.rs");
    assert!(flat(COPY_PASTE).contains(&flat(
        "(&kb.explorer_trash, \"explorer_trash\"), ] { if matches_any_binding(combos, key, mods) \
         && super::surface_actions::held_for_draft(&mut self.state, engine, &target, id)"
    )));
    for id in [
        "convert_surface",
        "convert_to_markdown",
        "convert_to_explorer",
    ] {
        assert!(
            double_tap.contains(&format!(
                "held_for_draft(&mutself.state,engine,&target,\"{id}\",)"
            )),
            "double_tap.rs {id}"
        );
    }
    assert!(REDRAW.contains("self.run_palette_surface_command(engine, id, target)"));
    assert!(REDRAW.contains("self.run_held_view_actions(engine);"));
}

/// 탐색기 새 탭 초안에 건 액션은 아직 그려지지 않은 생성 탐색기에도 실행된다.
#[test]
fn an_explorer_draft_focuses_the_address_of_the_created_explorer() {
    let (mut state, engine, sid, _dir) = crate::state::tests::explorer_fixture(true);
    let read = engine.read();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = std::sync::Arc::new(());
    state
        .attach_pending
        .begin(
            &read,
            &state.navigation,
            &crate::ipc::stream::StructuralOp::NewTab {
                anchor_surface_id: sid,
                surface_kind: "explorer".into(),
                params: serde_json::json!({}),
                inherit_cwd: None,
            },
            sender.epoch(),
            std::sync::Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .expect("the draft begins");
    let target = state.command_surface_target(&read);
    assert_eq!(
        hold_for_draft(
            &mut state,
            &read,
            "explorer_focus_address",
            target,
            Shortcut
        ),
        Some(true)
    );
    assert!(state.explorer_views.get(sid).is_none());
    settle_first_created(&mut state, &engine.core_state, sid);
    run_deferred_tools(&mut state, &read);
    assert!(
        state
            .explorer_views
            .get(sid)
            .is_some_and(|v| v.focus_address)
    );
}

/// native WebView 가 넘긴 키는 모델 포커스(B)가 아니라 키를 받은 surface(A)에서 실행한다. A 가 받지 않는
/// 액션은 B 로 보내지 않고 넘기며, A 가 모델 포커스면 지금 경로에 맡긴다.
#[test]
fn a_key_from_a_webview_runs_on_the_surface_that_received_it() {
    use winit::keyboard::{Key, ModifiersState, NamedKey};
    let (mut state, engine) = two_tabs();
    let read = engine.read();
    let focused = state.focused_surface_id(&read).expect("focused surface");
    let received = if focused == 11 { 12 } else { 11 };
    let mut kb = crate::settings::KeybindingSettings::default();
    for (id, combo) in [
        ("convert_to_explorer", "ctrl+f9"),
        ("explorer_go_up", "ctrl+f10"),
    ] {
        assert!(
            kb.clear_field(id) && kb.add_binding(id, combo.into()),
            "{id}"
        );
    }
    let ctrl = ModifiersState::CONTROL;
    let f9 = Key::Named(NamedKey::F9);
    let f10 = Key::Named(NamedKey::F10);
    assert_eq!(
        run_received_key(&mut state, &read, &kb, &f9, ctrl, received),
        Received::Done
    );
    let intents = state.take_pending_intents();
    assert!(converts(&intents, received) && !converts(&intents, focused));
    assert_eq!(
        run_received_key(&mut state, &read, &kb, &f10, ctrl, received),
        Received::Pass
    );
    assert_eq!(
        run_received_key(&mut state, &read, &kb, &f10, ctrl, focused),
        Received::Unbound
    );
    assert!(state.take_pending_intents().is_empty());
    const WEBVIEW_KEYS: &str = include_str!("../../../../app/webview_keys.rs");
    assert!(WEBVIEW_KEYS.contains("main.run_received_surface_key("));
}

/// WebView 가 넘긴 F2 는 탐색기 이름 바꾸기(목록 범위)이자 탭 이름 바꾸기(전역)다. html 은 앞의 것을 받지
/// 않으므로 페이지로 넘기지 않고 전역 단축키만 처리한다. 모델 포커스가 탐색기여도 그 탐색기의 이름 바꾸기를
/// 고르는 목록 키 단계는 거치지 않는다. 전역 바인딩을 지우면 페이지로 넘긴다.
#[test]
fn a_webview_key_with_a_host_command_runs_only_the_global_shortcut() {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    use winit::keyboard::{Key, ModifiersState, NamedKey};
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
    for (index, (id, kind)) in [(1, "html"), (2, "explorer")].into_iter().enumerate() {
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
        1,
        Box::new(crate::plugin_bridge::remote_surface::RemoteSurface::new(
            1,
            "html",
            "com.tasty.html".into(),
            "html".into(),
        )),
    );
    state.goto_tab_in_pane(&session.core_state, 1);
    let read = session.read();
    assert_eq!(state.focused_surface_id(&read), Some(2));
    assert!(!crate::adapters::ui::input::shortcuts::focused_explorer_typing(&state, &read));
    let f2 = Key::Named(NamedKey::F2);
    let mut kb = crate::settings::KeybindingSettings::default();
    assert_eq!(
        run_received_key(&mut state, &read, &kb, &f2, ModifiersState::empty(), 1),
        Received::Global
    );
    assert!(state.take_pending_intents().is_empty());
    assert!(kb.clear_field("rename_tab"));
    assert_eq!(
        run_received_key(&mut state, &read, &kb, &f2, ModifiersState::empty(), 1),
        Received::Pass
    );
    // 전역 갈래는 모델 포커스 surface 의 내용을 읽는 단계(복사·링크 복사·탐색기 목록 키·붙여넣기)를 거치지
    // 않고, 키를 받은 surface 의 대상으로 창 제어·내장 단축키·스크립트·숫자 전환·zoom 만 처리한다.
    const VIEW: &str = include_str!("surface_view_actions.rs");
    assert!(VIEW.contains(
        "Received::Global => Some(self.run_target_shortcuts(engine, key, mods, target, false)),"
    ));
    const DISPATCH: &str = include_str!("dispatch.rs");
    let body = &DISPATCH[DISPATCH
        .find("fn run_target_shortcuts(")
        .expect("global path")..];
    let body = &body[..body.find("\n    }\n").expect("fn end")];
    for step in [
        "self.handle_window_control_shortcuts(",
        "self.run_keybinding_shortcuts(",
        "self.try_dispatch_script_shortcut(",
        "Self::handle_numeric_switch_shortcuts(",
        "Self::handle_zoom_shortcut(",
        "paste && self.handle_paste_shortcut(",
    ] {
        assert!(body.contains(step), "{step}");
    }
    for focus_read in [
        "handle_explorer_shortcut",
        "handle_copy_shortcut",
        "run_copy_link",
        "handle_webview_edit_shortcut",
        ".command_target(",
    ] {
        assert!(!body.contains(focus_read), "{focus_read}");
    }
}

/// WebView 가 넘긴 주소창 포커스 키도 받은 surface 의 주소 띠 능력으로 판정한다. 띠를 선언한 plugin kind 면
/// html 이 아니어도 그 surface 의 띠에 포커스를 요청한다.
#[test]
fn a_webview_address_key_follows_the_declared_address_band() {
    use crate::runtime::surface_registry::KindCapabilities;
    use winit::keyboard::{Key, ModifiersState, NamedKey};
    let capabilities = KindCapabilities {
        address_band: true,
        ..KindCapabilities::default()
    };
    let (mut state, engine) = webview_tab("web", capabilities);
    let read = engine.read();
    let mut kb = crate::settings::KeybindingSettings::default();
    assert!(
        kb.clear_field("explorer_focus_address")
            && kb.add_binding("explorer_focus_address", "ctrl+f11".into())
    );
    let f11 = Key::Named(NamedKey::F11);
    assert_eq!(
        run_received_key(&mut state, &read, &kb, &f11, ModifiersState::CONTROL, 1),
        Received::Done
    );
    assert_eq!(state.html_address.focus_requested(), Some(1));
}

/// 탐색기 탭 위에 터미널 새 탭 초안이 선택돼 있으면 포커스 surface 종류는 없다. 탐색기 키는 초안 뒤의 기존
/// 탐색기를 대상으로 잡지 않고, 소비하지도 않고 다음 처리자에 넘긴다.
#[test]
fn an_explorer_key_passes_while_a_terminal_draft_is_selected() {
    use winit::keyboard::{Key, NamedKey};
    let (mut state, engine, _sid, _dir) = crate::state::tests::explorer_fixture(true);
    let read = engine.read();
    assert_eq!(
        state.focused_surface_type(&read),
        crate::state::FocusedSurfaceType::Kind("explorer".into())
    );
    let _draft = select_pending_draft(&mut state, &read);
    assert_eq!(
        state.focused_surface_type(&read),
        crate::state::FocusedSurfaceType::None
    );
    let kb = crate::settings::KeybindingSettings::default();
    let up = Key::Named(NamedKey::ArrowUp);
    let target = state.command_target(&read);
    assert!(!crate::view::main::MainView::match_explorer_bindings(
        &mut state,
        &read,
        &target,
        &kb,
        &up,
        BINDING_ALT
    ));
    assert!(state.take_pending_intents().is_empty());
}

/// 탐색기 새 탭 초안이면 탐색기 키는 지금처럼 초안에 걸고 소비한다.
#[test]
fn an_explorer_key_is_held_for_an_explorer_draft() {
    use winit::keyboard::{Key, NamedKey};
    let (mut state, engine, _sid, _dir) = crate::state::tests::explorer_fixture(true);
    let read = engine.read();
    let _draft = crate::state::tests::select_pending_draft_of_kind(&mut state, &read, "explorer");
    let id = pending(state.command_surface_target(&read));
    let kb = crate::settings::KeybindingSettings::default();
    let up = Key::Named(NamedKey::ArrowUp);
    let target = state.command_target(&read);
    assert!(crate::view::main::MainView::match_explorer_bindings(
        &mut state,
        &read,
        &target,
        &kb,
        &up,
        BINDING_ALT
    ));
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 1);
}

fn font_zooms(intents: &[crate::intent::DispatchedIntent]) -> Vec<Option<String>> {
    use crate::app::engine_action::SettingsPatch;
    intents
        .iter()
        .filter_map(|i| match &i.body {
            Intent::PatchSettings(SettingsPatch::FontSize { kind, .. }) => Some(kind.clone()),
            _ => None,
        })
        .collect()
}

/// 터미널 새 탭 초안 선택 중 zoom 은 바로 적용하지 않고 걸어 두었다가, 생성된 터미널의 글꼴에 적용한다.
#[test]
fn zoom_for_a_draft_applies_to_the_created_surface_kind() {
    use super::super::zoom::ZoomAction;
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let anchor = state.focused_surface_id(&read).expect("focused surface");
    let _draft = select_pending_draft(&mut state, &read);
    let id = pending(state.command_surface_target(&read));
    let target = state.command_target(&read);
    assert!(crate::view::main::MainView::apply_zoom(
        &mut state,
        &read,
        ZoomAction::In,
        &target
    ));
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 1);
    settle_first_created(&mut state, &engine.core_state, anchor);
    run_deferred_tools(&mut state, &read);
    assert_eq!(font_zooms(&state.take_pending_intents()), vec![None]);
}

/// 탐색기 탭 위의 터미널 초안에 건 zoom 은 뒤의 탐색기 kind 를 바꾸지 않는다. 생성된 surface 의 kind 가
/// 접수 때와 다르면 버린다.
#[test]
fn zoom_for_a_terminal_draft_does_not_touch_the_explorer_behind_it() {
    use super::super::zoom::ZoomAction;
    let (mut state, engine, sid, _dir) = crate::state::tests::explorer_fixture(true);
    let read = engine.read();
    let _draft = select_pending_draft(&mut state, &read);
    let id = pending(state.command_surface_target(&read));
    let target = state.command_target(&read);
    assert!(crate::view::main::MainView::apply_zoom(
        &mut state,
        &read,
        ZoomAction::In,
        &target
    ));
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 1);
    settle_first_created(&mut state, &engine.core_state, sid);
    run_deferred_tools(&mut state, &read);
    assert!(font_zooms(&state.take_pending_intents()).is_empty());
}

/// 생성에 실패한 초안에 건 zoom 은 버린다.
#[test]
fn zoom_for_a_failed_draft_is_dropped() {
    use super::super::zoom::ZoomAction;
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let _draft = select_pending_draft(&mut state, &read);
    let id = pending(state.command_surface_target(&read));
    let target = state.command_target(&read);
    assert!(crate::view::main::MainView::apply_zoom(
        &mut state,
        &read,
        ZoomAction::Out,
        &target
    ));
    let entry = &state.attach_pending.entries[0];
    entry.receipt.bind(0);
    let epoch = entry.epoch.clone();
    state.attach_pending.result(&epoch, 0, None, false);
    state.attach_pending.retain(&engine.core_state);
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 0);
    run_deferred_tools(&mut state, &read);
    assert!(state.take_pending_intents().is_empty());
}
