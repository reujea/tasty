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
