//! 명령 대상은 입력 때 정하고 실행 때 포커스를 다시 읽지 않는다.

use super::command_target_gone;
use super::surface_actions::tests::two_tabs;
use crate::adapters::ui::popup::command_palette::enabled_tests::press_enter_with_query;
use crate::state::attach_pending::deferred::CommandSurfaceTarget;
use crate::state::command_palette::PaletteCommand;
use crate::state::command_target::CommandTarget;
use crate::state::tests::select_pending_draft;
use crate::view::main::MainView;
use tasty_presets::PresetKind;

/// 지금 보이는 탭 id.
fn visible_tab(
    state: &crate::state::MainViewState,
    read: &crate::runtime::engine_read::EngineRead<'_>,
) -> u32 {
    state.command_target(read).tab.expect("visible tab")
}

/// 팔레트에서 Enter 로 `label_key` 의 내장 명령을 고르고, 큐에 넣은 명령 대상을 꺼낸다.
fn choose_host_command(
    state: &mut crate::state::MainViewState,
    read: &crate::runtime::engine_read::EngineRead<'_>,
    id: &str,
    label_key: &str,
) -> CommandTarget {
    let label = crate::i18n::t(label_key).trim_end_matches(':').to_string();
    press_enter_with_query(state, read, &label);
    let (cmd, target) = state.command_palette.pending_run.take().expect("queued");
    assert!(matches!(cmd, PaletteCommand::Host { id: chosen, .. } if chosen == id));
    target
}

/// 쌓인 요청 중 이름 바꾸기 팝업의 대상.
fn rename_requests(state: &mut crate::state::MainViewState) -> Vec<crate::state::RenameTarget> {
    state
        .take_pending_intents()
        .into_iter()
        .filter_map(|intent| match intent.body {
            crate::intent::Intent::Ui(crate::intent::UiIntent::ReopenPopup(req))
            | crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup(req)) => {
                req.payload.peek(|p| match p {
                    crate::adapters::ui::popup::PopupPayload::Rename(r) => Some(r.target.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// `dispatch_action_by_id` 의 `id` 팔 본문. MainView 는 EventLoopProxy 가 있어야 만들 수 있어 단위 시험에서
/// 부를 수 없으므로, 팔이 Enter 대상(`target`)을 그대로 실행 함수에 넘기는지 소스로 대조한다.
fn dispatch_arm(src: &str, id: &str) -> String {
    use tasty_doc_guards::match_arms::Source;
    let source = Source::new(src);
    let body = source
        .fn_bodies("dispatch_action_by_id")
        .first()
        .map(|b| source.slice(b).to_string())
        .expect("dispatch_action_by_id");
    let head = format!("\"{id}\" =>");
    let at = body.find(&head).unwrap_or_else(|| panic!("{id} arm"));
    let rest = &body[at + head.len()..];
    let end = rest.find("\n            \"").unwrap_or(rest.len());
    rest[..end].to_string()
}

const DISPATCH_SRC: &str = include_str!("dispatch.rs");

/// 팔레트 실행 팔이 Enter 대상으로 아래 실행 함수를 부른다. 팔을 비우면 검출한다.
#[test]
fn the_palette_arms_run_on_the_enter_target() {
    for (id, call) in [
        (
            "rename_tab",
            "super::open_tab_rename(state, engine, target)",
        ),
        (
            "close_pane",
            "super::close_pane_action(state, engine, target);",
        ),
        (
            "new_workspace",
            "super::new_workspace_action(state, engine, target);",
        ),
        (
            "apply_tab_preset",
            "Self::open_preset_apply_popup(state, engine, PresetApplyScope::Tab, target)",
        ),
    ] {
        assert!(dispatch_arm(DISPATCH_SRC, id).contains(call), "{id}");
    }
    let emptied = DISPATCH_SRC.replacen(
        "\"rename_tab\" => super::open_tab_rename(state, engine, target),",
        "\"rename_tab\" => {}",
        1,
    );
    assert_ne!(emptied, DISPATCH_SRC);
    assert!(!dispatch_arm(&emptied, "rename_tab").contains("open_tab_rename"));
}

/// 팔레트에서 고른 탭 이름 바꾸기는 Enter 뒤 다른 탭으로 옮겨도 고른 탭의 이름 바꾸기 팝업을 요청한다.
#[test]
fn a_palette_rename_tab_requests_the_tab_chosen_at_enter() {
    let (mut state, session) = two_tabs();
    let read = session.read();
    state.goto_tab_in_pane(&read, 0);
    let chosen = visible_tab(&state, &read);
    let target = choose_host_command(
        &mut state,
        &read,
        "rename_tab",
        "settings.keybindings.rename_tab_label",
    );
    state.goto_tab_in_pane(&read, 1);
    assert_ne!(visible_tab(&state, &read), chosen);
    super::open_tab_rename(&mut state, &read, &target);
    assert_eq!(
        rename_requests(&mut state),
        vec![crate::state::RenameTarget::TabName { tab_id: chosen }]
    );
}

/// 탭 11·12 의 구조 사건. 뒤에 사건을 더해 같은 기록의 다음 상태를 만든다.
fn tab_events() -> Vec<tasty_core::DomainEvent> {
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
    events
}

/// Enter 때 고른 탭 T1 이 실행 전에 닫히고 T2 가 남았으면, 대상이 사라진 것으로 보고 아무 요청도 만들지 않는다.
#[test]
fn a_palette_command_whose_tab_closed_makes_no_request() {
    use crate::state::tests::{test_model, test_state_from_model};
    let (mut state, session) = test_state_from_model(test_model(tab_events()));
    let read = session.read();
    state.goto_tab_in_pane(&read, 0);
    let target = choose_host_command(
        &mut state,
        &read,
        "rename_tab",
        "settings.keybindings.rename_tab_label",
    );
    assert_eq!(target.tab, Some(11));
    drop(read);
    let mut events = tab_events();
    events.push(tasty_core::DomainEvent::TabClosed { id: 11 });
    let (_, after) = test_state_from_model(test_model(events));
    let read = after.read();
    assert!(read.find_pane_by_id(1).is_some_and(|p| p.tabs.len() == 1));
    for id in ["rename_tab", "close_active", "next_tab", "apply_tab_preset"] {
        assert!(command_target_gone(&state, &read, id, &target), "{id}");
    }
    super::open_tab_rename(&mut state, &read, &target);
    super::close_active_action(
        &mut state,
        &read,
        &CommandTarget {
            pane: Some(987_654),
            workspace: Some(987_654),
            ..target
        },
    );
    assert!(state.take_pending_intents().is_empty());
}

/// 두 pane 에서 Enter 때 pane P1 을 고른 탭 프리셋·pane 닫기는 그 뒤 P2 로 옮겨도 P1 을 대상으로 한다.
#[test]
fn palette_preset_and_close_pane_keep_the_enter_pane() {
    use super::keybinding::PresetApplyScope;
    use crate::adapters::ui::popup::preset_apply::PresetApplyTarget;
    let (mut state, session) = crate::state::tests::two_pane_fixture();
    let read = session.read();
    let ws = read.workspace_at(0).expect("workspace");
    state.navigation.select_pane(ws, 10);
    let preset = choose_host_command(
        &mut state,
        &read,
        "apply_tab_preset",
        "settings.keybindings.apply_tab_preset_label",
    );
    let close = choose_host_command(
        &mut state,
        &read,
        "close_pane",
        "settings.keybindings.close_pane_label",
    );
    assert_eq!((preset.pane, preset.tab), (Some(10), Some(101)));
    assert_eq!(close.pane, Some(10));
    let ws = read.workspace_at(0).expect("workspace");
    state.navigation.select_pane(ws, 20);
    MainView::open_preset_apply_popup(&mut state, &read, PresetApplyScope::Tab, &preset);
    assert_eq!(
        state.dialogs.preset_apply_targets.get(PresetKind::Tab),
        Some(PresetApplyTarget::Tab(101))
    );
    state.take_pending_intents();
    super::close_pane_action(&mut state, &read, &close);
    let closes: Vec<u32> = state
        .take_pending_intents()
        .into_iter()
        .filter_map(|intent| match intent.body {
            crate::intent::Intent::Domain(crate::app::command::DomainIntent::ClosePane {
                pane_id,
            }) => Some(pane_id),
            _ => None,
        })
        .collect();
    assert_eq!(closes, vec![10]);
}

/// 탭 프리셋 적용 팝업은 명령 대상 탭에 결속되고, 초안이 선택돼 있었으면 열리지 않는다.
#[test]
fn a_tab_preset_targets_the_command_tab_and_skips_a_draft() {
    use super::keybinding::PresetApplyScope;
    use crate::adapters::ui::popup::preset_apply::PresetApplyTarget;
    let (mut state, session) = two_tabs();
    let read = session.read();
    state.goto_tab_in_pane(&read, 0);
    let target = state.command_target(&read);
    state.goto_tab_in_pane(&read, 1);
    MainView::open_preset_apply_popup(&mut state, &read, PresetApplyScope::Tab, &target);
    assert_eq!(
        state.dialogs.preset_apply_targets.get(PresetKind::Tab),
        target.tab.map(PresetApplyTarget::Tab)
    );

    let (mut state, session) = two_tabs();
    let read = session.read();
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_target(&read);
    assert!(target.draft.is_some());
    MainView::open_preset_apply_popup(&mut state, &read, PresetApplyScope::Tab, &target);
    assert_eq!(
        state.dialogs.preset_apply_targets.get(PresetKind::Tab),
        None
    );
    assert!(state.take_pending_intents().is_empty());
}

/// 초안이 선택된 동안의 스크린샷은 초안 뒤의 화면을 찍지 않는다.
#[test]
fn a_screenshot_for_a_draft_is_dropped() {
    let (mut state, session) = two_tabs();
    let read = session.read();
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_target(&read);
    MainView::queue_screenshot_to_clipboard(&mut state, &read, &target);
    assert!(state.take_pending_intents().is_empty());

    let target = CommandTarget {
        draft: None,
        ..target
    };
    MainView::queue_screenshot_to_clipboard(&mut state, &read, &target);
    assert_eq!(state.take_pending_intents().len(), 1);
}

/// 입력 때 정한 대상이 없어졌으면 그 명령은 실행하지 않는다. 대상이 없던 명령은 해당하지 않는다.
#[test]
fn a_command_whose_target_is_gone_is_dropped() {
    let (state, session) = two_tabs();
    let read = session.read();
    let live = state.command_target(&read);
    for id in [
        "rename_tab",
        "close_active",
        "new_tab",
        "close_workspace",
        "close_surface",
        "zoom_in",
    ] {
        assert!(
            !command_target_gone(&state, &read, id, &live),
            "{id} on a live target"
        );
    }
    let gone_tab = CommandTarget {
        tab: Some(987_654),
        ..live
    };
    assert!(command_target_gone(&state, &read, "rename_tab", &gone_tab));
    assert!(command_target_gone(
        &state,
        &read,
        "close_active",
        &gone_tab
    ));
    assert!(!command_target_gone(&state, &read, "new_tab", &gone_tab));
    let gone_pane = CommandTarget {
        pane: Some(987_654),
        ..live
    };
    assert!(command_target_gone(&state, &read, "new_tab", &gone_pane));
    assert!(command_target_gone(&state, &read, "rename_tab", &gone_pane));
    let gone_ws = CommandTarget {
        workspace: Some(987_654),
        ..live
    };
    assert!(command_target_gone(
        &state,
        &read,
        "close_workspace",
        &gone_ws
    ));
    let gone_surface = CommandTarget {
        surface: Some(CommandSurfaceTarget::Surface(987_654)),
        ..live
    };
    assert!(command_target_gone(
        &state,
        &read,
        "close_surface",
        &gone_surface
    ));
    assert!(command_target_gone(&state, &read, "zoom_in", &gone_surface));
    assert!(!command_target_gone(
        &state,
        &read,
        "new_workspace",
        &CommandTarget::default()
    ));
    assert!(!command_target_gone(
        &state,
        &read,
        "rename_tab",
        &CommandTarget::default()
    ));
}

/// 팔레트 실행은 Enter 때 정한 대상을 넘기고, 실행기는 포커스를 다시 읽지 않는다.
#[test]
fn the_palette_runs_host_commands_on_the_target_bound_at_enter() {
    use tasty_doc_guards::match_arms::Source;
    const REDRAW: &str = include_str!("../../../../view/main/redraw.rs");
    const DISPATCH: &str = include_str!("dispatch.rs");
    let redraw = Source::new(REDRAW);
    let body = redraw
        .fn_bodies("dispatch_pending_command_palette")
        .first()
        .map(|b| redraw.code[b.clone()].to_string())
        .expect("dispatch_pending_command_palette");
    assert!(
        body.contains(
            "Some((cmd, command_target)) = self.state.command_palette.pending_run.take()"
        )
    );
    assert!(body.contains("self.dispatch_action_by_id(engine, id, &command_target)"));
    assert!(
        !body.contains(".command_target("),
        "실행 때 대상을 다시 만들면 안 된다"
    );

    let dispatch = Source::new(DISPATCH);
    let bodies = dispatch.fn_bodies("dispatch_action_by_id");
    assert_eq!(bodies.len(), 1);
    let body = &dispatch.code[bodies[0].clone()];
    for reread in [
        ".command_target(",
        "focused_",
        "active_workspace",
        "command_surface_target",
    ] {
        assert!(
            !body.contains(reread),
            "dispatch_action_by_id 가 포커스를 다시 읽는다: {reread}"
        );
    }
    assert!(body.contains("command_target_gone(&self.state, engine, action_id, target)"));
}

/// 팔레트 복사·잘라내기·붙여넣기는 Enter 때 surface 가 사라졌거나 대상 draft 가 사라졌으면 버린다.
#[test]
fn palette_copy_cut_paste_are_dropped_when_the_target_is_gone() {
    let (state, session) = two_tabs();
    let read = session.read();
    let live = state.command_target(&read);
    let gone_surface = CommandTarget {
        surface: Some(CommandSurfaceTarget::Surface(987_654)),
        ..live
    };
    let gone_draft = CommandTarget {
        draft: Some(crate::state::attach_pending::PendingId(987_654)),
        ..live
    };
    for id in ["copy", "cut", "paste"] {
        assert!(!command_target_gone(&state, &read, id, &live), "{id}");
        assert!(
            command_target_gone(&state, &read, id, &gone_surface),
            "{id}"
        );
        assert!(command_target_gone(&state, &read, id, &gone_draft), "{id}");
    }
}

/// 팔레트 편집 경로가 Enter 대상의 surface 를 끝까지 넘기고, 그 아래 실행 함수는 포커스를 다시 읽지 않는다.
#[test]
fn palette_edit_paths_run_on_the_enter_surface() {
    use tasty_doc_guards::match_arms::Source;
    let body = |src: &'static str, name: &str| {
        let source = Source::new(src);
        source
            .fn_bodies(name)
            .first()
            .map(|b| source.code[b.clone()].to_string())
            .unwrap_or_else(|| panic!("{name}"))
    };
    let dispatch = body(DISPATCH_SRC, "dispatch_action_by_id");
    for call in [
        "let sid = target.surface_id();",
        "self.ask_plugin_surface_palette_edit(engine, edit, sid)",
        "self.run_copy_on(engine, sid)",
        "self.run_explorer_action_on(engine, ExplorerAction::CutFiles, sid)",
        "self.run_explorer_action_on(engine, ExplorerAction::PasteFiles, sid)",
        "self.run_paste_on(engine, sid)",
    ] {
        assert!(dispatch.contains(call), "{call}");
    }
    assert!(!dispatch.contains("self.run_copy(engine)"));
    const COPY_PASTE: &str = include_str!("copy_paste.rs");
    const CLIPBOARD: &str = include_str!("../../../../view/main/clipboard.rs");
    const PLUGIN_KEYS: &str = include_str!("../../../../view/main/plugin_surface_keys.rs");
    for (src, name) in [
        (COPY_PASTE, "run_copy_on"),
        (COPY_PASTE, "run_explorer_action_on"),
        (COPY_PASTE, "run_paste_on"),
        (CLIPBOARD, "paste_to_terminal_on"),
        (CLIPBOARD, "deliver_paste"),
        (CLIPBOARD, "paste_text_to"),
        (PLUGIN_KEYS, "ask_plugin_surface_palette_edit"),
        (PLUGIN_KEYS, "surface_edit_target_on"),
    ] {
        // 선택 영역의 surface 와 `sid` 를 비교하는 판정 함수 이름은 포커스를 읽지 않는다.
        let found = body(src, name).replace("should_copy_via_focused_selection", "");
        for focus_read in ["focused_", ".command_target(", "surface_edit_target_now"] {
            assert!(!found.contains(focus_read), "{name}: {focus_read}");
        }
    }
}

/// 쌓인 요청 중 새 workspace 요청의 카테고리.
fn new_workspace_requests(
    state: &mut crate::state::MainViewState,
) -> Vec<Option<crate::model::WorkspaceCategoryId>> {
    state
        .take_pending_intents()
        .into_iter()
        .filter_map(|intent| match intent.body {
            crate::intent::Intent::NewWorkspace { category, .. } => Some(category),
            _ => None,
        })
        .collect()
}

/// 새 workspace 는 Enter 때 정한 workspace 가 실행 전에 사라졌으면 만들지 않는다. 처음부터 대상 workspace 가
/// 없던 입력은 지금처럼 카테고리 없이 만들고, 대상이 살아 있으면 그 카테고리에 만든다.
#[test]
fn a_new_workspace_is_dropped_only_when_its_bound_workspace_is_gone() {
    let (mut state, session) = two_tabs();
    let read = session.read();
    let live = state.command_target(&read);
    let category = live.find_workspace(&read).expect("workspace").category;
    let gone = CommandTarget {
        workspace: Some(987_654),
        ..live
    };
    let unbound = CommandTarget::default();
    assert!(command_target_gone(&state, &read, "new_workspace", &gone));
    assert!(!command_target_gone(
        &state,
        &read,
        "new_workspace",
        &unbound
    ));
    assert!(!command_target_gone(&state, &read, "new_workspace", &live));
    super::new_workspace_action(&mut state, &read, &unbound);
    assert_eq!(new_workspace_requests(&mut state), vec![None]);
    super::new_workspace_action(&mut state, &read, &live);
    assert_eq!(new_workspace_requests(&mut state), vec![Some(category)]);
}

/// workspace 1 에 터미널 탭 11·12(pane 1), workspace 2 에 탭 21(pane 2). 모델 포커스는 workspace 2 다.
fn two_workspaces() -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    let mut events = vec![E::CategoryCreated {
        id: 0,
        name: "normal".into(),
        index: 0,
    }];
    for (ws, pane, tabs) in [(1, 1, vec![11, 12]), (2, 2, vec![21])] {
        events.push(E::WorkspaceCreated {
            id: ws,
            name: format!("w{ws}"),
            category: 0,
            index: (ws - 1) as usize,
            pane,
        });
        for (index, id) in tabs.into_iter().enumerate() {
            events.push(E::TabCreated {
                id,
                pane,
                index,
                name: "t".into(),
                surface: SurfaceSpec {
                    id,
                    kind: "terminal".into(),
                    data: None,
                },
            });
        }
    }
    let (mut state, session) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    state.set_active_workspace_index(&session.core_state, 1);
    (state, session)
}

/// WebView 가 받은 키의 대상은 그 surface 를 담은 workspace·pane·탭이다. 모델 포커스와 다른 surface 면
/// 모델 포커스로 만든 대상과 다르다.
#[test]
fn a_received_surface_target_is_the_tab_holding_that_surface() {
    let (state, session) = two_workspaces();
    let read = session.read();
    assert_eq!(state.focused_surface_id(&read), Some(21));
    let target = state
        .command_target_for_surface(&read, 12)
        .expect("surface 12");
    assert_eq!(
        target,
        CommandTarget {
            workspace: Some(1),
            pane: Some(1),
            tab: Some(12),
            surface: Some(CommandSurfaceTarget::Surface(12)),
            ..CommandTarget::default()
        }
    );
    assert_eq!(state.command_target_for_surface(&read, 987_654), None);
}

/// 받은 surface 의 탭이 F2(`rename_tab`)와 `close_active` 의 대상이다. 모델 포커스 탭은 건드리지 않는다.
#[test]
fn a_received_rename_and_close_target_the_receiving_tab() {
    use winit::keyboard::{Key, ModifiersState, NamedKey};
    let (mut state, session) = two_workspaces();
    let read = session.read();
    let target = state
        .command_target_for_surface(&read, 11)
        .expect("surface 11");
    let kb = crate::settings::KeybindingSettings::default();
    assert!(MainView::match_rename_bindings(
        &mut state,
        &read,
        &target,
        &kb,
        &Key::Named(NamedKey::F2),
        ModifiersState::empty(),
    ));
    let renames: Vec<_> = state
        .take_pending_intents()
        .into_iter()
        .filter_map(|intent| match intent.body {
            crate::intent::Intent::Ui(crate::intent::UiIntent::ReopenPopup(req))
            | crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup(req)) => {
                req.payload.peek(|p| match p {
                    crate::adapters::ui::popup::PopupPayload::Rename(r) => Some(r.target.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        renames,
        vec![crate::state::RenameTarget::TabName { tab_id: 11 }]
    );

    crate::shortcuts::close_active_action(&mut state, &read, &target);
    let closes: Vec<_> = state
        .take_pending_intents()
        .into_iter()
        .filter_map(|intent| match intent.body {
            crate::intent::Intent::Domain(crate::app::command::DomainIntent::CloseTab {
                tab_id,
            }) => Some(tab_id),
            _ => None,
        })
        .collect();
    assert_eq!(closes, vec![11]);
}

/// 다른 workspace 의 WebView 가 받은 숫자 탭 전환 키는 그 surface 가 있는 pane 의 탭을 바꾼다.
#[test]
fn a_received_tab_switch_changes_the_receiving_pane() {
    let (mut state, session) = two_workspaces();
    let read = session.read();
    let target = state
        .command_target_for_surface(&read, 11)
        .expect("surface 11");
    let kb = crate::settings::KeybindingSettings::default();
    let pane1 = read.find_pane_by_id(1).expect("pane 1");
    assert_eq!(state.navigation.tab_index(pane1), 0);
    assert!(MainView::handle_numeric_switch_shortcuts(
        &mut state,
        &read,
        &kb,
        &winit::keyboard::Key::Character(kb.tab_next_key().into()),
        winit::keyboard::ModifiersState::CONTROL,
        true,
        false,
        false,
        false,
        &target,
    ));
    let pane1 = read.find_pane_by_id(1).expect("pane 1");
    assert_eq!(state.navigation.tab_index(pane1), 1);
    assert_eq!(state.focused_surface_id(&read), Some(21));
}

/// WebView 키 진입은 받은 surface 로 대상을 만들어 두 갈래에 넘기고, 전체 단축키 경로(모델 포커스)를 부르지
/// 않는다.
#[test]
fn the_webview_key_entry_binds_the_receiving_surface() {
    use tasty_doc_guards::match_arms::Source;
    const WEBVIEW_KEYS: &str = include_str!("../../../../app/webview_keys.rs");
    let src = Source::new(WEBVIEW_KEYS);
    let body = src
        .fn_bodies("dispatch_forwarded_webview_key")
        .first()
        .map(|b| src.code[b.clone()].to_string())
        .expect("dispatch_forwarded_webview_key");
    assert!(body.contains(".command_target_for_surface(&engine.read(), ev.surface_id)"));
    assert!(body.contains("ev.surface_id,\n            &target,\n        )"));
    assert!(
        body.contains("main.handle_received_shortcut(&engine.read(), &ev.key, ev.mods, &target)")
    );
    assert!(!body.contains(".command_target(&"));
    assert!(!body.contains("handle_shortcut_after_edit_judgment"));

    const VIEW: &str = include_str!("surface_view_actions.rs");
    let view = Source::new(VIEW);
    let received = view
        .fn_bodies("run_received_surface_key")
        .first()
        .map(|b| view.code[b.clone()].to_string())
        .expect("run_received_surface_key");
    assert!(!received.contains(".command_target("));

    const DISPATCH: &str = include_str!("dispatch.rs");
    let dispatch = Source::new(DISPATCH);
    let handled = dispatch
        .fn_bodies("handle_received_shortcut")
        .first()
        .map(|b| dispatch.code[b.clone()].to_string())
        .expect("handle_received_shortcut");
    assert!(handled.contains("self.run_webview_edit_on(sid, action)"));
    assert!(handled.contains("self.run_target_shortcuts(engine, key, mods, target, false)"));
    for focus_read in [
        ".command_target(",
        "focused_",
        "handle_copy_shortcut",
        "handle_explorer_shortcut",
    ] {
        assert!(!handled.contains(focus_read), "{focus_read}");
    }
}

/// 카테고리 0 에 workspace 1·2, 카테고리 1 에 workspace 3·4, 카테고리 2 에 workspace 5 가 있고 workspace 3(인덱스 2)이
/// 활성이다.
fn two_categories() -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    let mut events = Vec::new();
    for id in 0..3 {
        events.push(E::CategoryCreated {
            id,
            name: format!("c{id}"),
            index: id as usize,
        });
    }
    for ws in 1..=5u32 {
        events.push(E::WorkspaceCreated {
            id: ws,
            name: format!("w{ws}"),
            category: ((ws - 1) / 2).min(2),
            index: (ws - 1) as usize,
            pane: ws,
        });
        events.push(E::TabCreated {
            id: ws * 10,
            pane: ws,
            index: 0,
            name: "t".into(),
            surface: SurfaceSpec {
                id: ws * 10,
                kind: "terminal".into(),
                data: None,
            },
        });
    }
    let (mut state, mut session) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    let engine = session.borrow_mut();
    engine.runtime.settings.general.workspace_categories_enabled = true;
    state.switch_workspace(&engine.read(), 2);
    (state, session)
}

/// 설정에서 `want` 전환으로 판정되는 modifier 조합(ctrl, shift, alt, option).
fn switch_held(
    kb: &crate::settings::KeybindingSettings,
    want: crate::adapters::ui::switch_overlay::SwitchTarget,
) -> [bool; 4] {
    use crate::adapters::ui::switch_overlay::switch_target_for;
    (0..16u8)
        .map(|bits| [0, 1, 2, 3].map(|i| bits & (1 << i) != 0))
        .find(|[c, s, a, o]| switch_target_for(kb, *c, *s, *a, *o) == Some(want))
        .expect("switch combo")
}

/// workspace 1(카테고리 0)의 surface 10 이 받은 전환 키를 처리한 뒤의 활성 workspace 인덱스.
fn received_switch(
    kb: &crate::settings::KeybindingSettings,
    want: crate::adapters::ui::switch_overlay::SwitchTarget,
    key: &str,
) -> usize {
    let [ctrl, shift, alt, option] = switch_held(kb, want);
    let (mut state, session) = two_categories();
    let read = session.read();
    assert_eq!(state.active_workspace_index(&read), 2);
    let target = state
        .command_target_for_surface(&read, 10)
        .expect("surface 10");
    assert!(
        MainView::handle_numeric_switch_shortcuts(
            &mut state,
            &read,
            kb,
            &winit::keyboard::Key::Character(key.into()),
            winit::keyboard::ModifiersState::empty(),
            ctrl,
            shift,
            alt,
            option,
            &target,
        ),
        "{key}"
    );
    state.active_workspace_index(&read)
}

/// WebView 가 받은 workspace 다음 키·슬롯 키는 받은 surface 의 workspace 와 그 카테고리를 기준으로 한다.
/// 활성 workspace(카테고리 1)를 기준으로 하면 인덱스 3 으로 간다.
#[test]
fn a_received_workspace_switch_uses_the_receiving_workspace() {
    use crate::adapters::ui::switch_overlay::SwitchTarget;
    let kb = crate::settings::KeybindingSettings::default();
    for key in [
        kb.workspace_next_key(),
        kb.workspace_switch_slot_keys[1].as_str(),
    ] {
        assert_eq!(
            received_switch(&kb, SwitchTarget::Workspace, key),
            1,
            "{key}"
        );
    }
}

/// WebView 가 받은 카테고리 다음·이전 키는 받은 surface 의 workspace 카테고리(0)에서 옮긴다. 활성 workspace 의
/// 카테고리(1)를 기준으로 하면 다음은 카테고리 2(인덱스 4), 이전은 카테고리 0(인덱스 0)으로 간다. 슬롯 키는
/// 섹션 번호로 정하므로 어느 workspace 가 받았어도 같은 카테고리로 간다.
#[test]
fn a_received_category_switch_uses_the_receiving_workspace() {
    use crate::adapters::ui::switch_overlay::SwitchTarget;
    let kb = crate::settings::KeybindingSettings::default();
    assert_eq!(
        received_switch(&kb, SwitchTarget::Category, kb.category_next_key()),
        2
    );
    assert_eq!(
        received_switch(&kb, SwitchTarget::Category, kb.category_prev_key()),
        4
    );
    assert_eq!(
        received_switch(
            &kb,
            SwitchTarget::Category,
            &kb.category_switch_slot_keys[2]
        ),
        4
    );
}
