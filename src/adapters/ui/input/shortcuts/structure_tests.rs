//! 구조 명령(분할·새 탭·닫은 항목 복원·새 workspace)은 입력 때 정한 대상과 cwd 원본으로 해석된다.
//! 입력과 해석 사이에 포커스가 옮겨져도 대상은 바뀌지 않고, 대상이 사라졌으면 버린다.

use super::structure;
use crate::app::command::DomainIntent;
use crate::app::creation_intent::{Resolved, resolve};
use crate::intent::{DispatchedIntent, Intent, StructureTarget};
use crate::model::SplitDirection;
use crate::runtime::engine_session::EngineSession;
use crate::state::MainViewState;

/// pane 10(탐색기 101, 루트 A)과 pane 20(탐색기 201, 루트 B). 포커스는 pane 20 이다.
fn two_explorer_panes() -> (
    MainViewState,
    EngineSession,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    use tasty_core::DomainEvent as E;
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
            pane: 10,
        },
        E::PaneSplit {
            target: 10,
            pane: 20,
            split: tasty_core::SplitSpec {
                direction: SplitDirection::Vertical,
                ratio: tasty_core::Ratio::from_f32(0.5),
                placement: tasty_core::Placement::After,
            },
        },
    ];
    for (id, pane) in [(101, 10), (201, 20)] {
        events.push(E::TabCreated {
            id,
            pane,
            index: 0,
            name: "e".into(),
            surface: tasty_core::SurfaceSpec {
                id,
                kind: "explorer".into(),
                data: None,
            },
        });
    }
    let (mut state, mut session) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    let a = crate::test_support::abs_path("struct/a");
    let b = crate::test_support::abs_path("struct/b");
    for (id, root) in [(101, &a), (201, &b)] {
        session.runtime.surfaces.insert(
            id,
            Box::new(crate::model::ExplorerPanel::new(id, root.clone())),
        );
    }
    session.runtime.settings.general.inherit_cwd = true;
    state.select_pane(&session.core_state, 20);
    (state, session, a, b)
}

fn resolved(state: &MainViewState, engine: &EngineSession, intent: &DispatchedIntent) -> Resolved {
    resolve(state, &engine.as_ref(), &intent.body, &intent.origin).expect("resolve")
}

/// 입력 뒤 포커스를 pane 10 으로 옮긴다.
fn move_focus_away(state: &mut MainViewState, engine: &EngineSession) {
    state.select_pane(&engine.core_state, 10);
    assert_eq!(state.focused_pane_id(&engine.read()), 10);
}

#[test]
fn split_pane_uses_the_pane_and_cwd_chosen_at_input() {
    let (mut state, engine, _a, b) = two_explorer_panes();
    let target = state.command_target(&engine.read());
    let intent = structure::split_pane(&target, SplitDirection::Vertical)
        .expect("no draft")
        .from_user_shortcut("split_pane_vertical");
    move_focus_away(&mut state, &engine);
    match resolved(&state, &engine, &intent).domain() {
        Some(DomainIntent::SplitPane {
            target_pane_id,
            cwd,
            ..
        }) => {
            assert_eq!(target_pane_id, 20);
            assert_eq!(cwd, Some(b));
        }
        other => panic!("expected a pane split, got {other:?}"),
    }
}

#[test]
fn split_surface_uses_the_surface_chosen_at_input() {
    let (mut state, engine, _, _) = two_explorer_panes();
    let target = state.command_target(&engine.read());
    let intent = structure::split_surface(&target, SplitDirection::Horizontal)
        .expect("no draft")
        .from_user_shortcut("split_surface_horizontal");
    move_focus_away(&mut state, &engine);
    assert!(matches!(
        resolved(&state, &engine, &intent).domain(),
        Some(DomainIntent::SplitSurface {
            target_surface_id: 201,
            ..
        })
    ));
}

#[test]
fn new_tab_uses_the_pane_and_cwd_chosen_at_input() {
    let (mut state, engine, _a, b) = two_explorer_panes();
    let target = state.command_target(&engine.read());
    let intent = structure::new_tab(&target, None, serde_json::Value::Null)
        .expect("no draft")
        .from_user_shortcut("new_tab");
    move_focus_away(&mut state, &engine);
    match resolved(&state, &engine, &intent).domain() {
        Some(DomainIntent::CreateTab { pane_id, cwd, .. }) => {
            assert_eq!(pane_id, 20);
            assert_eq!(cwd, Some(b));
        }
        other => panic!("expected a new tab, got {other:?}"),
    }
}

#[test]
fn restore_closed_uses_the_pane_chosen_at_input() {
    let (mut state, engine, _, _) = two_explorer_panes();
    let target = state.command_target(&engine.read());
    let intent = structure::restore_closed(&target).from_user_shortcut("restore_closed");
    move_focus_away(&mut state, &engine);
    assert!(matches!(
        resolved(&state, &engine, &intent).domain(),
        Some(DomainIntent::RestoreClosedItem {
            target_pane_id: Some(20),
            ..
        })
    ));
}

/// 새 workspace 와 튜토리얼 workspace 는 입력 때 고른 surface 의 cwd 를 상속한다.
#[test]
fn new_workspace_inherits_the_cwd_of_the_surface_chosen_at_input() {
    let (mut state, engine, _a, b) = two_explorer_panes();
    let source = state.command_target(&engine.read()).cwd_source();
    assert_eq!(source, Some(201));
    let intent = Intent::NewWorkspace {
        kind: None,
        params: serde_json::Value::Null,
        category: None,
        cwd_source: source,
    }
    .from_user_shortcut("new_workspace");
    move_focus_away(&mut state, &engine);
    assert!(matches!(
        resolved(&state, &engine, &intent).domain(),
        Some(DomainIntent::CreateWorkspace { cwd, .. }) if cwd == Some(b.clone())
    ));
    let tutorial = Intent::PrepareTutorial {
        ticket: std::sync::Weak::new(),
        cwd_source: source,
    }
    .from_user_menu("tutorial.prepare");
    assert!(matches!(
        resolved(&state, &engine, &tutorial).domain(),
        Some(DomainIntent::CreateWorkspace { cwd, .. }) if cwd == Some(b.clone())
    ));
}

/// 탭 바 분할 버튼은 포커스 pane 이 아니라 버튼이 있는 pane 을 나누고 그 pane 의 선택 surface cwd 를 쓴다.
#[test]
fn tab_bar_split_splits_the_pane_of_the_button() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};
    let (mut state, engine, a, _b) = two_explorer_panes();
    apply_tab_bar_actions(
        &mut state,
        &engine.read(),
        vec![TabBarAction::RequestSplit { pane_id: 10 }],
        &[],
        160.0,
        1.0,
    );
    let split = state
        .take_pending_intents()
        .into_iter()
        .find(|i| matches!(i.body, Intent::SplitPane { .. }))
        .expect("the split is requested");
    // 버튼은 입력 때 그 pane 으로 포커스를 옮긴다. 해석 전에 포커스가 다시 옮겨져도 대상은 그대로다.
    state.select_pane(&engine.core_state, 20);
    match resolved(&state, &engine, &split).domain() {
        Some(DomainIntent::SplitPane {
            target_pane_id,
            cwd,
            ..
        }) => {
            assert_eq!(target_pane_id, 10);
            assert_eq!(cwd, Some(a));
        }
        other => panic!("expected a pane split, got {other:?}"),
    }
}

/// 도구 메뉴의 surface 열기는 대상 surface 가 있는 pane 에 연다.
#[test]
fn tools_menu_open_surface_opens_in_the_pane_of_its_surface() {
    let (mut state, engine, _, _) = two_explorer_panes();
    let item = crate::adapters::ui::tools_menu::command_item(
        "p",
        "c",
        crate::plugin::manifest::ToolAction::OpenSurface {
            surface_kind: "explorer".into(),
        },
    );
    crate::adapters::ui::tools_menu::invoke_tool_on(&mut state, &engine.read(), &item, Some(101));
    let open = state
        .take_pending_intents()
        .into_iter()
        .find(|i| matches!(i.body, Intent::NewTab { .. }))
        .expect("the tab is requested");
    assert!(matches!(
        resolved(&state, &engine, &open).domain(),
        Some(DomainIntent::CreateTab { pane_id: 10, .. })
    ));
}

/// 입력 때 정한 대상이 사라졌으면 다른 pane·surface 로 옮기지 않고 버린다.
#[test]
fn a_structure_command_whose_target_is_gone_is_dropped() {
    let (state, engine, _, _) = two_explorer_panes();
    let gone = StructureTarget {
        pane: Some(999),
        cwd_source: Some(201),
    };
    let intents = [
        Intent::SplitPane {
            direction: SplitDirection::Vertical,
            target: gone,
        },
        Intent::SplitSurface {
            direction: SplitDirection::Vertical,
            surface: Some(999),
        },
        Intent::NewTab {
            kind: None,
            params: serde_json::Value::Null,
            target: gone,
        },
        Intent::RestoreClosedItem { pane: Some(999) },
        Intent::SplitPane {
            direction: SplitDirection::Vertical,
            target: StructureTarget::default(),
        },
    ];
    for intent in intents {
        let dispatched = intent.from_user_shortcut("gone");
        assert!(
            matches!(resolved(&state, &engine, &dispatched), Resolved::Dropped),
            "{:?}",
            dispatched.body
        );
    }
}

/// cwd 원본 surface 가 사라졌으면 상속하지 않는다. 포커스 surface 로 바꿔 읽지 않는다.
#[test]
fn a_gone_cwd_source_inherits_nothing() {
    let (state, engine, _, _) = two_explorer_panes();
    let intent = structure::split_pane_at(20, Some(999), SplitDirection::Vertical)
        .from_user_shortcut("split_pane_vertical");
    assert!(matches!(
        resolved(&state, &engine, &intent).domain(),
        Some(DomainIntent::SplitPane {
            target_pane_id: 20,
            cwd: None,
            ..
        })
    ));
}

/// 생성 대기 초안이 대상이면 분할은 입력 때 거절해 intent 를 만들지 않는다.
#[test]
fn a_split_whose_target_is_a_draft_is_refused_at_input() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let read = engine.read();
    let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
    let target = state.command_target(&read);
    assert!(target.draft.is_some());
    assert!(structure::split_pane(&target, SplitDirection::Vertical).is_none());
    assert!(structure::split_surface(&target, SplitDirection::Vertical).is_none());
}

/// 원격(mirror)으로 넘기는 새 탭·pane 분할의 cwd 원본은 입력 때 그 pane 에서 선택된 surface 다.
/// 입력 뒤 같은 pane 의 선택 탭을 바꿔도 넘어가는 원본은 바뀌지 않는다.
#[test]
fn a_forwarded_pane_creation_carries_the_cwd_source_chosen_at_input() {
    use crate::app::services::impl_mirror::{ClientCwd, build_mirror_forward_op};
    use tasty_ipc::stream::InheritCwd;
    use winit::keyboard::{Key, ModifiersState, SmolStr};
    let (mut state, mut engine) = crate::state::tests::two_pane_fixture();
    state.select_pane(&engine.core_state, 10);
    engine.runtime.settings.general.inherit_cwd = true;
    let target = state.command_target(&engine.read());
    assert_eq!(target.surface_id(), Some(101));
    let split = structure::split_pane(&target, SplitDirection::Vertical)
        .expect("no draft")
        .from_user_shortcut("split_pane_vertical");
    let new_tab = structure::new_tab(&target, None, serde_json::Value::Null)
        .expect("no draft")
        .from_user_shortcut("new_tab");
    let kb = crate::settings::KeybindingSettings {
        new_workspace: vec![],
        new_tab: vec!["ctrl+t".into()],
        ..Default::default()
    };
    assert!(crate::view::main::MainView::match_create_bindings(
        &mut state,
        &engine.read(),
        &target,
        &kb,
        &Key::Character(SmolStr::new("t")),
        ModifiersState::CONTROL,
    ));
    let key_tab = state
        .take_pending_intents()
        .into_iter()
        .find_map(|i| match i.body {
            Intent::Domain(d @ DomainIntent::CreateTab { .. }) => Some(d),
            _ => None,
        })
        .expect("the key makes a new tab");
    let pane = engine.core_state.find_pane_by_id(10).expect("pane 10");
    state.navigation.goto_tab(pane, 1);
    let client = ClientCwd {
        presentation: Some(&state.navigation),
        inherit: true,
    };
    let mut domains: Vec<DomainIntent> = [split, new_tab]
        .iter()
        .map(|i| resolved(&state, &engine, i).domain().expect("resolved"))
        .collect();
    domains.push(key_tab);
    for intent in domains {
        assert_eq!(
            build_mirror_forward_op(&engine.core_state, &intent, &client)
                .expect("creation op")
                .inherit_cwd(),
            Some(&InheritCwd::Surface(101)),
            "{intent:?}"
        );
    }
}

/// 탭 바의 새 탭 버튼도 누른 pane 에서 그때 선택된 surface 를 원격 cwd 원본으로 싣는다.
#[test]
fn a_tab_bar_new_tab_carries_the_cwd_source_of_its_pane() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};
    use crate::app::services::impl_mirror::{ClientCwd, build_mirror_forward_op};
    use tasty_ipc::stream::InheritCwd;
    let (mut state, engine) = crate::state::tests::two_pane_fixture();
    apply_tab_bar_actions(
        &mut state,
        &engine.read(),
        vec![TabBarAction::AddTab { pane_id: 10 }],
        &[],
        160.0,
        1.0,
    );
    let tab = state
        .take_pending_intents()
        .into_iter()
        .find_map(|i| match i.body {
            Intent::Domain(d @ DomainIntent::CreateTab { .. }) => Some(d),
            _ => None,
        })
        .expect("the tab is requested");
    let pane = engine.core_state.find_pane_by_id(10).expect("pane 10");
    state.navigation.goto_tab(pane, 1);
    let client = ClientCwd {
        presentation: Some(&state.navigation),
        inherit: true,
    };
    assert_eq!(
        build_mirror_forward_op(&engine.core_state, &tab, &client)
            .expect("creation op")
            .inherit_cwd(),
        Some(&InheritCwd::Surface(101))
    );
}

/// 팔레트와 더블탭의 새 탭은 cwd 를 읽은 surface 를 cwd 원본으로 싣는다.
/// 두 실행기는 창(MainView)이 있어야 돌아 실행 시험을 만들 수 없어 소스로 확인한다.
#[test]
fn palette_and_double_tap_new_tab_carry_the_cwd_source_they_read() {
    fn arm<'a>(src: &'a str, from: &str, to: &str) -> &'a str {
        let start = src.find(from).expect("new_tab arm");
        let end = start + src[start..].find(to).expect("next arm");
        &src[start..end]
    }
    let palette = arm(
        include_str!("dispatch.rs"),
        "\"new_tab\" =>",
        "\"split_pane_vertical\" =>",
    );
    assert!(palette.contains("let cwd = target\n                    .surface_id()"));
    assert!(palette.contains("cwd_source: target.surface_id(),"));
    let double_tap = arm(
        include_str!("double_tap.rs"),
        "\"new_tab\" =>",
        "\"close_pane\" =>",
    );
    assert!(double_tap.contains("let cwd = self.state.resolve_inherit_cwd(engine);"));
    assert!(double_tap.contains("let cwd_source = self.state.focused_surface_id(engine);"));
    assert!(double_tap.contains("cwd_source,"));
}

/// 메뉴 소유 surface 에서 연 새 탭은 그 소유 surface 를 원격 cwd 원본으로 싣는다.
#[test]
fn an_owner_menu_new_tab_carries_its_owner_as_the_cwd_source() {
    use crate::app::services::impl_mirror::{ClientCwd, build_mirror_forward_op};
    use tasty_ipc::stream::InheritCwd;
    let (mut state, engine) = crate::state::tests::two_pane_fixture();
    state
        .add_kind_tab_by_owner(&engine.read(), 102, "terminal", &serde_json::json!({}))
        .expect("owner exists");
    let tab = state
        .take_pending_intents()
        .into_iter()
        .find_map(|i| match i.body {
            Intent::Domain(d @ DomainIntent::CreateTab { .. }) => Some(d),
            _ => None,
        })
        .expect("the tab is requested");
    let client = ClientCwd {
        presentation: Some(&state.navigation),
        inherit: true,
    };
    assert_eq!(
        build_mirror_forward_op(&engine.core_state, &tab, &client)
            .expect("creation op")
            .inherit_cwd(),
        Some(&InheritCwd::Surface(102))
    );
}

/// pane 컨텍스트 메뉴의 새 터미널 탭은 cwd 를 읽은 그 pane 의 선택 surface 를 cwd 원본으로 싣는다.
/// 메뉴 결과 적용은 창(MainView)에서만 돌아 소스로 확인한다.
#[test]
fn the_pane_menu_new_tab_carries_the_cwd_source_it_read() {
    let src = include_str!("../../../../view/main/redraw.rs");
    let start = src
        .find("fn apply_pane_menu_selection(")
        .expect("apply_pane_menu_selection");
    let body = &src[start..start + src[start..].find("Some(2) =>").expect("next arm")];
    assert!(body.contains("self.state.resolve_inherit_cwd_from_surface(engine, surface)"));
    assert!(body.contains("cwd_source: selected,"));
}
