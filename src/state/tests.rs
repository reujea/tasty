use super::*;
use crate::model::SplitDirection;

// `pub(crate)`: `state::popup_close_tests`(sibling module)와
// `adapters::ui::notification::on_close_drain_tests`(on_close 훅 drain 메커니즘
// 테스트)가 동일 구성을 재사용한다 — popup close 뒷정리/훅 테스트가 여기와
// 동형의 AppState/CoreState 를 필요로 함.
pub(crate) fn test_state() -> (AppState, crate::core::CoreState) {
    let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
        std::sync::Arc::new(std::sync::Mutex::new(
            tasty_memory::testing::InMemoryStorage::new(),
        ));
    test_state_with_memory(memory)
}

/// `test_state` 와 같은 구성이되 memory 백엔드를 호출자가 넘긴다. 호출자가 concrete
/// `Arc<Mutex<InMemoryStorage>>` 를 따로 들고 있으면 close 이후 mock 의 호출 이력
/// (`purge_scope_call_count`)을 직접 검사할 수 있다.
pub(crate) fn test_state_with_memory(
    memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
) -> (AppState, crate::core::CoreState) {
    let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
    let mut engine = crate::core::CoreState::new(80, 24, waker).unwrap();
    // markdown surface kind는 com.tasty.markdown plugin 이 hello 시 rendering="webview"
    // 로 등록한다(Stage B, register_plugin_surface_kinds 의 Webview 분기와 동형) —
    // register_webview_kind(overlay 플래그) + register_remote_kind(SurfaceKindDef) 둘 다
    // 필요하다. 테스트에서는 plugin manager 를 띄우지 않으므로 직접 재현한다.
    let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(serde_json::json!({
        "kind": "markdown",
        "display_name_i18n_key": "surface.kind.markdown",
        "rendering": "webview",
        // 실제 tasty-plugin.toml 의 `[[surface_kinds.preset_fields]]` 와 동형 —
        // `derive_cwd=true` 가 있어야 markdown 파일의 부모 디렉토리가 cwd 상속
        // 시발점으로 파생된다(`PresetFieldSpec::derive_cwd`, 이 test 픽스처가 없으면
        // remote_kind::register_remote_kind 의 create 클로저가 파생할 게 없어
        // 호출자가 넘긴 일반 inherited-cwd 로 fallback 해버린다).
        "preset_fields": [{
            "id": "file",
            "label_key": "preset.field.file",
            "param_key": "file",
            "input_type": "file_path",
            "required": true,
            "derive_cwd": true,
        }],
    }))
    .expect("test SurfaceKindDecl");
    // WEBVIEW_KINDS 는 프로세스 전역이라, 이 register 가 webview_kind 의 poison/query
    // 테스트와 병렬로 끼어들면 그쪽의 `!is_webview_kind("markdown")` 단언을 깨뜨린다.
    // 그 전역을 만지는 테스트가 공유하는 락으로 이 register 를 감싼다.
    {
        let _g = crate::core::surface_registry::webview_kind::WEBVIEW_KIND_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        crate::core::surface_registry::webview_kind::register_webview_kind(
            "com.tasty.markdown",
            &decl.kind,
        );
    }
    // remote kind 등록은 gui 전용 모듈(`plugin_bridge::remote_kind`)이라 headless
    // 테스트 빌드에는 없다. 이 픽스처를 쓰는 headless 테스트(intent drain 등)는
    // markdown surface 생성 경로를 타지 않으므로 등록만 건너뛴다.
    #[cfg(feature = "gui")]
    {
        let (host_cmd_tx, _host_cmd_rx) = std::sync::mpsc::channel();
        crate::plugin_bridge::remote_kind::register_remote_kind(
            &engine.surface_registry,
            "com.tasty.markdown",
            &decl,
            host_cmd_tx,
        );
    }
    let preset_store = std::sync::Arc::new(std::sync::Mutex::new(
        tasty_presets::PresetStore::load_default(),
    ));
    let state = AppState::new(&mut engine, preset_store, memory);
    (state, engine)
}

/// 현재 활성 워크스페이스의 모든 surface ID를 수집한다.
fn collect_surface_ids(state: &mut AppState, engine: &mut crate::core::CoreState) -> Vec<u32> {
    let ws = state.active_workspace_mut(engine);
    let ws_ids: std::collections::HashSet<u32> = ws.all_surface_ids().into_iter().collect();
    engine
        .terminals
        .iter()
        .filter_map(|(sid, _)| ws_ids.contains(&sid).then_some(sid))
        .collect()
}

/// 모든 워크스페이스에 걸쳐 surface ID를 수집한다.
fn collect_all_surface_ids(_state: &mut AppState, engine: &mut crate::core::CoreState) -> Vec<u32> {
    engine.terminals.iter().map(|(sid, _)| sid).collect()
}

// ---- find_terminal_by_id ----

#[test]
fn find_terminal_by_id_exists() {
    let (mut state, mut engine) = test_state();
    let surface_ids = collect_surface_ids(&mut state, &mut engine);
    assert!(!surface_ids.is_empty());
    let first_id = surface_ids[0];
    assert!(engine.find_terminal_by_id(first_id).is_some());
}

#[test]
fn find_terminal_by_id_nonexistent() {
    let (_state, engine) = test_state();
    assert!(engine.find_terminal_by_id(9999).is_none());
}

#[test]
fn find_terminal_by_id_after_split() {
    let (mut state, mut engine) = test_state();
    let original_ids = collect_surface_ids(&mut state, &mut engine);
    let original_id = original_ids[0];

    state
        .test_split_pane(&mut engine, SplitDirection::Vertical)
        .unwrap();

    let all_ids = collect_surface_ids(&mut state, &mut engine);
    assert_eq!(all_ids.len(), 2);

    assert!(engine.find_terminal_by_id(original_id).is_some());
    let new_id = *all_ids.iter().find(|&&id| id != original_id).unwrap();
    assert!(engine.find_terminal_by_id(new_id).is_some());
}

#[test]
fn find_terminal_by_id_across_tabs() {
    let (mut state, mut engine) = test_state();
    let original_ids = collect_surface_ids(&mut state, &mut engine);
    let first_id = original_ids[0];

    state.add_tab(&mut engine).unwrap();

    let all_ids = collect_all_surface_ids(&mut state, &mut engine);
    assert_eq!(all_ids.len(), 2);

    assert!(engine.find_terminal_by_id(first_id).is_some());
    let second_id = *all_ids.iter().find(|&&id| id != first_id).unwrap();
    assert!(engine.find_terminal_by_id(second_id).is_some());
}

// ---- close operations ----

#[test]
fn close_active_pane_single_fails() {
    let (mut state, mut engine) = test_state();
    assert!(!state.close_active_pane(&mut engine));
}

#[test]
fn close_active_pane_after_split() {
    let (mut state, mut engine) = test_state();
    state
        .test_split_pane(&mut engine, SplitDirection::Vertical)
        .unwrap();

    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        2
    );
    assert!(state.close_active_pane(&mut engine));
    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        1
    );
}

#[test]
fn close_active_tab_single_fails() {
    let (mut state, mut engine) = test_state();
    assert!(!state.close_active_tab(&mut engine));
}

#[test]
fn close_active_tab_after_add() {
    let (mut state, mut engine) = test_state();
    state.add_tab(&mut engine).unwrap();

    let pane_id = state.active_workspace(&engine).focused_pane;
    let tab_count = state
        .active_workspace(&engine)
        .pane_layout()
        .find_pane(pane_id)
        .unwrap()
        .tabs
        .len();
    assert_eq!(tab_count, 2);

    assert!(state.close_active_tab(&mut engine));

    let tab_count_after = state
        .active_workspace(&engine)
        .pane_layout()
        .find_pane(pane_id)
        .unwrap()
        .tabs
        .len();
    assert_eq!(tab_count_after, 1);
}

// ---- mirror 워크스페이스 구조 op forward (UI/키보드 경로) ----
// mirror 워크스페이스의 close/new-tab 은 로컬 트리를 건드리지 않고 원격으로
// forward 돼야 한다(split 이 Core::apply→forward 로 하는 것과 동형). UI-layer
// 직접 조작 경로(close_active_*·add_tab·close_tab)는 Core::apply 를 우회하므로
// forward 를 직접 얹는다. 아래 테스트는 각 경로가 (1) 올바른 StructuralOp 를
// pending_structural_forward 에 쌓고 (2) 로컬 트리는 그대로 두는지 검증한다.

#[test]
fn mirror_close_active_surface_forwards_close_surface() {
    use crate::ipc::stream::StructuralOp;
    let (mut state, mut engine) = test_state();
    let sid = state.focused_surface_id(&engine).unwrap();
    state.active_workspace_mut(&mut engine).mirror = true;
    assert!(engine.pending_structural_forward.is_empty());

    // 폴백 체인 정지(true) + 로컬 트리 불변.
    assert!(state.close_active_surface(&mut engine));
    assert!(
        engine.find_terminal_by_id(sid).is_some(),
        "mirror close 는 로컬 surface 를 지우면 안 된다"
    );
    assert_eq!(engine.pending_structural_forward.len(), 1);
    let queued = &engine.pending_structural_forward[0];
    assert!(
        queued.user_triggered,
        "AppState 직접 호출 경로는 항상 GUI 유래"
    );
    match &queued.op {
        StructuralOp::CloseSurface { surface_id } => assert_eq!(*surface_id, sid),
        other => panic!("expected CloseSurface, got {other:?}"),
    }
}

#[test]
fn mirror_close_active_pane_forwards_close_pane() {
    use crate::ipc::stream::StructuralOp;
    let (mut state, mut engine) = test_state();
    let sid = state.focused_surface_id(&engine).unwrap();
    state.active_workspace_mut(&mut engine).mirror = true;

    assert!(state.close_active_pane(&mut engine));
    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        1,
        "mirror pane close 는 로컬 pane 을 제거하면 안 된다"
    );
    // mirror 는 forward 만 하고 로컬 흔적(스냅샷 포함)을 남기지 않는다:
    // 스냅샷 캡처 블록은 `forward_mirror_structural`의 이른 return **뒤**에
    // 있으므로 mirror 경로에선 애초에 실행되지 않는다.
    assert_eq!(
        engine.closed_items.len(),
        0,
        "mirror pane close 는 closed_items 스택에 아무것도 남기면 안 된다"
    );
    match &engine.pending_structural_forward[0].op {
        StructuralOp::ClosePane { anchor_surface_id } => assert_eq!(*anchor_surface_id, sid),
        other => panic!("expected ClosePane, got {other:?}"),
    }
}

#[test]
fn mirror_close_active_tab_forwards_close_tab() {
    use crate::ipc::stream::StructuralOp;
    let (mut state, mut engine) = test_state();
    let sid = state.focused_surface_id(&engine).unwrap();
    state.active_workspace_mut(&mut engine).mirror = true;

    assert!(state.close_active_tab(&mut engine));
    assert!(
        engine.find_terminal_by_id(sid).is_some(),
        "mirror tab close 는 로컬 surface 를 지우면 안 된다"
    );
    match &engine.pending_structural_forward[0].op {
        StructuralOp::CloseTab { anchor_surface_id } => assert_eq!(*anchor_surface_id, sid),
        other => panic!("expected CloseTab, got {other:?}"),
    }
}

#[test]
fn mirror_add_tab_forwards_new_tab() {
    use crate::ipc::stream::StructuralOp;
    let (mut state, mut engine) = test_state();
    let sid = state.focused_surface_id(&engine).unwrap();
    let pane_id = state.active_workspace(&engine).focused_pane;
    let tabs_before = state
        .active_workspace(&engine)
        .pane_layout()
        .find_pane(pane_id)
        .unwrap()
        .tabs
        .len();
    state.active_workspace_mut(&mut engine).mirror = true;

    state.add_tab(&mut engine).unwrap();
    let tabs_after = state
        .active_workspace(&engine)
        .pane_layout()
        .find_pane(pane_id)
        .unwrap()
        .tabs
        .len();
    assert_eq!(
        tabs_after, tabs_before,
        "mirror add_tab 은 로컬 탭을 만들면 안 된다"
    );
    match &engine.pending_structural_forward[0].op {
        StructuralOp::NewTab {
            anchor_surface_id,
            surface_kind,
            ..
        } => {
            assert_eq!(*anchor_surface_id, sid);
            assert_eq!(surface_kind, "terminal");
        }
        other => panic!("expected NewTab, got {other:?}"),
    }
}

// ---- 09: close 시 client-only 인접 focus 후보 계산 ----

/// close focus fallback — 같은 pane 안 탭이 2개일 때 마지막 탭을 닫으면(닫히는 탭이 마지막이므로) 이전
/// 탭의 focused surface 가 1순위 인접 후보로 담긴다.
#[test]
fn mirror_close_active_tab_computes_sibling_candidate() {
    use crate::ipc::stream::StructuralOp;
    let (mut state, mut engine) = test_state();
    let sid_first = state.focused_surface_id(&engine).unwrap();
    // 두번째 탭 추가(아직 비-mirror — 로컬 실행돼 active_tab 이 새 탭으로 이동한다).
    state.add_tab(&mut engine).unwrap();
    let sid_second = state.focused_surface_id(&engine).unwrap();
    assert_ne!(sid_first, sid_second);

    state.active_workspace_mut(&mut engine).mirror = true;
    assert!(state.close_active_tab(&mut engine));
    let queued = &engine.pending_structural_forward[0];
    assert!(queued.user_triggered);
    assert_eq!(
        queued.close_focus_candidates,
        vec![sid_first],
        "닫히는 탭이 마지막이면 이전 탭의 focused surface 가 1순위 후보"
    );
    match &queued.op {
        StructuralOp::CloseTab { anchor_surface_id } => assert_eq!(*anchor_surface_id, sid_second),
        other => panic!("expected CloseTab, got {other:?}"),
    }
}

/// close focus fallback — split 된 tab 안에서 focus 된 surface 를 닫으면, 같은 tab 안의 형제 surface
/// 가 인접 후보로 담긴다(pane 자체가 사라지지 않으므로 tab 레벨로 안 올라간다).
#[test]
fn mirror_close_active_surface_split_computes_sibling_candidate() {
    use crate::ipc::stream::StructuralOp;
    let (mut state, mut engine) = test_state();
    let sid_a = state.focused_surface_id(&engine).unwrap();
    let pane_id = state.active_workspace(&engine).focused_pane;
    let (ws_idx, _) = engine.find_workspace_index_for_surface(sid_a).unwrap();
    let sid_b = engine.next_ids.next_surface();
    engine.workspaces[ws_idx]
        .pane_layout_mut()
        .find_pane_mut(pane_id)
        .unwrap()
        .split_surface_by_id_marker(sid_a, SplitDirection::Horizontal, sid_b)
        .unwrap();
    engine
        .terminals
        .insert(sid_b, tasty_terminal::Terminal::new_detached(80, 24));
    // split_surface_by_id_marker 는 focused_surface 를 안 건드리므로 sid_a 가 여전히
    // focus — close_active_surface 가 그 surface 를 닫는다.
    assert_eq!(state.focused_surface_id(&engine), Some(sid_a));

    state.active_workspace_mut(&mut engine).mirror = true;
    assert!(state.close_active_surface(&mut engine));
    let queued = &engine.pending_structural_forward[0];
    assert!(queued.user_triggered);
    assert_eq!(
        queued.close_focus_candidates,
        vec![sid_b],
        "split tab 안 형제 surface 가 인접 후보"
    );
    match &queued.op {
        StructuralOp::CloseSurface { surface_id } => assert_eq!(*surface_id, sid_a),
        other => panic!("expected CloseSurface, got {other:?}"),
    }
}

/// split tab 안 surface 를 실제 로컬 close 경로(mirror 아님)로 닫으면
/// closed-item 스냅샷이 남아 Ctrl+Shift+T 로 복원 가능해야 한다.
#[test]
fn close_active_surface_split_saves_closed_item_snapshot() {
    let (mut state, mut engine) = test_state();
    let sid_a = state.focused_surface_id(&engine).unwrap();
    let pane_id = state.active_workspace(&engine).focused_pane;
    let (ws_idx, _) = engine.find_workspace_index_for_surface(sid_a).unwrap();
    let sid_b = engine.next_ids.next_surface();
    engine.workspaces[ws_idx]
        .pane_layout_mut()
        .find_pane_mut(pane_id)
        .unwrap()
        .split_surface_by_id_marker(sid_a, SplitDirection::Horizontal, sid_b)
        .unwrap();
    engine
        .terminals
        .insert(sid_b, tasty_terminal::Terminal::new_detached(80, 24));
    assert_eq!(state.focused_surface_id(&engine), Some(sid_a));

    assert_eq!(engine.closed_items.len(), 0);
    assert!(state.close_active_surface(&mut engine));

    assert_eq!(
        engine.closed_items.len(),
        1,
        "split surface 를 닫으면 ClosedItem 이 하나 쌓여야 한다"
    );
    match engine.closed_items.list().next().unwrap() {
        crate::model::ClosedItem::Surface { surface, .. } => {
            assert_eq!(surface.id, sid_a);
        }
        crate::model::ClosedItem::Tab(_) => panic!("expected ClosedItem::Surface, got Tab"),
        crate::model::ClosedItem::Pane { .. } => {
            panic!("expected ClosedItem::Surface, got Pane")
        }
        crate::model::ClosedItem::Workspace { .. } => {
            panic!("expected ClosedItem::Surface, got Workspace")
        }
    }
}

/// pane 을 전용 `close_pane` 단축키(`close_active_pane`)로 닫으면 closed-item
/// 스냅샷이 남아 `restore_closed`(Ctrl+Shift+T)로 복원 가능해야 한다. 이
/// 회귀를 잡아내려면 `close_case_pane`/`close_active_pane` 어느 경로도
/// `push_closed_item` 을 호출하지 않게 되는 상황(둘 다 있었던 실제 버그) —
/// pane close 가 복원 스택에 아예 기록되지 않아 직전에 다른 걸 안 닫았으면
/// no-op, 닫았으면 엉뚱한 항목이 복원되는 상황 — 을 구체적으로 검증해야 한다.
#[test]
fn close_pane_saves_closed_item_snapshot() {
    let (mut state, mut engine) = test_state();
    state
        .test_split_pane(&mut engine, SplitDirection::Vertical)
        .unwrap();
    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        2
    );

    assert_eq!(engine.closed_items.len(), 0);
    assert!(state.close_active_pane(&mut engine));

    assert_eq!(
        engine.closed_items.len(),
        1,
        "pane 을 닫으면 ClosedItem 이 하나 쌓여야 한다"
    );
    match engine.closed_items.list().next().unwrap() {
        crate::model::ClosedItem::Pane { pane, .. } => {
            assert_eq!(pane.tabs.len(), 1, "split 로 만든 새 pane 은 tab 1개");
        }
        crate::model::ClosedItem::Surface { .. } => {
            panic!("expected ClosedItem::Pane, got Surface")
        }
        crate::model::ClosedItem::Tab(_) => panic!("expected ClosedItem::Pane, got Tab"),
        crate::model::ClosedItem::Workspace { .. } => {
            panic!("expected ClosedItem::Pane, got Workspace")
        }
    }
}

/// 스냅샷이 남는 것만으로는 부족하다 — pane close → restore 왕복이 실제로
/// 트리에 pane 을 되살리는지까지 end-to-end 검증한다
/// (`DomainIntent::RestoreClosedItem` 을 `Core::apply` 로 디스패치). 트리
/// 재삽입 위치(`insert_pane_beside` + 캡처된 split geometry)가 합리적인지 —
/// 복원 후 다시 pane 2개가 되고, cascade 이벤트가
/// `RestoredKind::PaneIntoWorkspace` 인지 — 를 함께 확인한다.
#[test]
fn close_pane_then_restore_reinserts_pane() {
    use crate::core::builder::CoreBuilder;
    use crate::core::intent::{CoreEvent, DomainIntent, RestoredKind};

    let (mut state, mut engine) = test_state();
    state
        .test_split_pane(&mut engine, SplitDirection::Vertical)
        .unwrap();
    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        2
    );
    // `close_active_pane`는 *focused* pane(= test_split_pane 이 방금 만든 새
    // pane)을 닫는다 — target_pane_id 는 그 뒤에 남은(=닫힘 이후 focused) pane
    // 이어야 한다. 실제 `restore_closed` 흐름(`src/intent/closed_item.rs`)도
    // 복원 시점에 `state.focused_pane(engine)`을 읽으므로 close *이후* 값이다.
    assert!(state.close_active_pane(&mut engine));
    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        1,
        "pane close 직후엔 1개여야 한다"
    );
    assert_eq!(engine.closed_items.len(), 1);
    let remaining_pane_id = state.active_workspace(&engine).focused_pane;

    let mut core = CoreBuilder::new()
        .with_fs(std::sync::Arc::new(
            crate::adapters::test::mem_fs::MemFileSystem::new(),
        ))
        .with_clock(std::sync::Arc::new(
            crate::adapters::test::fake_clock::FakeClock::default(),
        ))
        .with_clipboard(std::sync::Arc::new(
            crate::adapters::test::mock_clipboard::MockClipboard::default(),
        ))
        .with_process(std::sync::Arc::new(
            crate::adapters::test::mock_process::MockProcessSpawner,
        ))
        .with_home(std::sync::Arc::new(
            crate::adapters::test::tmp_home::TmpHome::new(tempfile::tempdir().expect("tmp").keep()),
        ))
        .with_sound_player(std::sync::Arc::new(
            crate::ports::notification_sound::NoopPlayer,
        ))
        .with_memory(std::sync::Arc::new(std::sync::Mutex::new(
            tasty_memory::testing::InMemoryStorage::new(),
        )))
        .with_themes(std::sync::Arc::new(tasty_themes::ThemeStore::new()))
        .with_preset_store(std::sync::Arc::new(std::sync::Mutex::new(
            tasty_presets::PresetStore::load_default(),
        )))
        .with_settings_storage(std::sync::Arc::new(tasty_settings::FileSettingsStorage))
        .build()
        .expect("test Core");

    let events = core
        .apply(
            &mut engine,
            DomainIntent::RestoreClosedItem {
                target_pane_id: Some(remaining_pane_id),
                scope: crate::core::intent::RestoreScope::Local,
            },
        )
        .expect("restore should not error");

    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        2,
        "restore 후 pane 이 다시 2개여야 한다"
    );
    let restored_kind_ok = events.iter().any(|ev| {
        matches!(
            ev,
            CoreEvent::ClosedItemRestored {
                restored: true,
                kind: RestoredKind::PaneIntoWorkspace { .. },
            }
        )
    });
    assert!(
        restored_kind_ok,
        "ClosedItemRestored{{restored:true, kind:PaneIntoWorkspace}} 이벤트가 있어야 한다: {events:?}"
    );
    assert_eq!(engine.closed_items.len(), 0, "복원 후 스택은 비어야 한다");
}

/// close focus fallback — pane 레벨 close(`close_active_pane`)는 후보를 계산하지 않는다(로컬도
/// cascade 시 "워크스페이스 첫 pane" 으로 무조건 이동하는 것과 같은 스코프 결정).
#[test]
fn mirror_close_active_pane_has_no_focus_candidates() {
    let (mut state, mut engine) = test_state();
    state.active_workspace_mut(&mut engine).mirror = true;
    assert!(state.close_active_pane(&mut engine));
    assert!(
        engine.pending_structural_forward[0]
            .close_focus_candidates
            .is_empty()
    );
}

#[test]
fn close_surface_by_id_no_snapshot_recreates_when_emptied() {
    // 마지막 workspace 의 유일 surface 까지 닫아도 workspaces 가 비면 안 된다.
    // 다음 redraw 의 active_workspace() 호출 패닉을 막기 위한 invariant 회복.
    let (mut state, mut engine) = test_state();
    assert_eq!(engine.workspaces.len(), 1);
    let surface_ids = collect_surface_ids(&mut state, &mut engine);
    assert_eq!(surface_ids.len(), 1);
    let sid = surface_ids[0];

    assert!(state.close_surface_by_id_no_snapshot(&mut engine, sid, false));
    assert!(
        !engine.workspaces.is_empty(),
        "agent-initiated close must not leave the window with zero workspaces"
    );
    // 자동 재생성된 workspace 는 새 surface 를 갖는다.
    let new_surface_ids = collect_surface_ids(&mut state, &mut engine);
    assert_eq!(new_surface_ids.len(), 1);
    assert_ne!(new_surface_ids[0], sid);
}

// ---- close_surface_by_id_inner cascade characterization (C3) ----
//
// `close_surface_by_id_inner` 의 *실제 부수효과* 를 고정한다: 닫힌 surface 의
// Terminal 이 store 에서 제거되는지(cleanup_surface 실행), 형제가 생존하는지,
// 구조(tab/pane)가 제거·재배정되는지, plugin lifecycle 큐에 close 이벤트가
// enqueue 되는지. Case4(마지막 workspace) 는 `..._recreates_when_emptied` 가
// 이미 커버하므로 Case1/2/3 만 신규 추가한다. save_snapshot=false 경로
// (`close_surface_by_id_no_snapshot`) 로 호출해 undo 스냅샷은 배제하고
// cleanup/enqueue 부수효과만 관측한다.

/// Case 1: split tab 내 다중 surface 중 하나 close → cleanup 실행(Terminal 제거),
/// 형제 surface 생존, lifecycle 이벤트 1건.
#[test]
fn c3_case1_split_surface_close_cleans_up_and_keeps_sibling() {
    let (mut state, mut engine) = test_state();
    let sid_a = collect_surface_ids(&mut state, &mut engine)[0];
    let pane_id = state.active_workspace(&engine).focused_pane;
    let (ws_idx, _) = engine.find_workspace_index_for_surface(sid_a).unwrap();
    let sid_b = engine.next_ids.next_surface();
    engine.workspaces[ws_idx]
        .pane_layout_mut()
        .find_pane_mut(pane_id)
        .unwrap()
        .split_surface_by_id_marker(sid_a, SplitDirection::Horizontal, sid_b)
        .unwrap();
    engine
        .terminals
        .insert(sid_b, tasty_terminal::Terminal::new_detached(80, 24));
    assert!(engine.terminals.contains(sid_a));

    let _ = state.take_pending_lifecycle_events();
    assert!(state.close_surface_by_id_no_snapshot(&mut engine, sid_a, false));

    assert!(
        !engine.terminals.contains(sid_a),
        "닫힌 surface 의 Terminal 이 cleanup 돼야 함"
    );
    assert!(engine.terminals.contains(sid_b), "형제 surface 는 생존");
    let events = state.take_pending_lifecycle_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].surface_id, sid_a);
}

/// Case 2: sole-surface tab & pane 에 tab >1 → tab 제거 + 해당 leaf cleanup,
/// 형제 tab 의 surface 생존.
#[test]
fn c3_case2_tab_close_removes_tab_and_cleans_surface() {
    let (mut state, mut engine) = test_state();
    let sid0 = collect_surface_ids(&mut state, &mut engine)[0];
    state.add_tab(&mut engine).unwrap();
    let pane_id = state.active_workspace(&engine).focused_pane;
    let sid1 = *collect_surface_ids(&mut state, &mut engine)
        .iter()
        .find(|&&s| s != sid0)
        .unwrap();
    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .find_pane(pane_id)
            .unwrap()
            .tabs
            .len(),
        2
    );

    let _ = state.take_pending_lifecycle_events();
    assert!(state.close_surface_by_id_no_snapshot(&mut engine, sid1, false));

    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .find_pane(pane_id)
            .unwrap()
            .tabs
            .len(),
        1
    );
    assert!(
        !engine.terminals.contains(sid1),
        "닫힌 tab 의 surface 가 cleanup 돼야 함"
    );
    assert!(
        engine.terminals.contains(sid0),
        "형제 tab 의 surface 는 생존"
    );
    let events = state.take_pending_lifecycle_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].surface_id, sid1);
}

/// Case 3: last tab in pane & ws 에 pane >1 → pane 제거 + focused_pane 재배정 +
/// leaf cleanup, 형제 pane 의 surface 생존.
#[test]
fn c3_case3_pane_close_removes_pane_and_reassigns_focus() {
    let (mut state, mut engine) = test_state();
    let sid0 = collect_surface_ids(&mut state, &mut engine)[0];
    state
        .test_split_pane(&mut engine, SplitDirection::Vertical)
        .unwrap();
    let sid1 = *collect_surface_ids(&mut state, &mut engine)
        .iter()
        .find(|&&s| s != sid0)
        .unwrap();
    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        2
    );

    let _ = state.take_pending_lifecycle_events();
    assert!(state.close_surface_by_id_no_snapshot(&mut engine, sid1, false));

    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .all_pane_ids()
            .len(),
        1
    );
    assert!(!engine.terminals.contains(sid1));
    assert!(
        engine.terminals.contains(sid0),
        "형제 pane 의 surface 는 생존"
    );
    let focused = state.active_workspace(&engine).focused_pane;
    assert!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .find_pane(focused)
            .is_some(),
        "focused_pane 이 생존 pane 으로 재배정돼야 함"
    );
    let events = state.take_pending_lifecycle_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].surface_id, sid1);
}

// ---- deferred surface reify (display-point) ----

/// 포커스된 pane 에 deferred(lazy PTY) 탭을 하나 추가하고 그 surface_id 를 반환한다.
/// restore 경로가 만드는 `EmptySurface { deferred_spawn: Some(..) }` placeholder 와
/// 동등한 상태를 구성한다.
fn add_deferred_tab(state: &mut AppState, engine: &mut crate::core::CoreState) -> u32 {
    let tab_id = engine.next_ids.next_tab();
    let surface_id = engine.next_ids.next_surface();
    let sh = crate::core::state::ShellConfig::from_settings(&engine.settings);
    let waker = engine.make_waker(surface_id);
    // 복원 경로(restore.rs)가 만드는 deferred placeholder 와 동등하게 직접 구성한다.
    let spawn = crate::model::DeferredSpawn {
        shell: sh.shell_ref().map(|s| s.to_string()),
        shell_args: sh.args_ref().iter().map(|s| s.to_string()).collect(),
        extra_env: sh
            .envs_ref()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        cols: engine.default_cols,
        rows: engine.default_rows,
        waker,
        working_dir: None,
        restore_command: None,
        scrollback_persist_id: None,
    };
    let placeholder = crate::model::EmptySurface::new_deferred(surface_id, spawn);
    let tab =
        crate::model::Tab::new_named(tab_id, "Shell".to_string(), None, Box::new(placeholder));
    let pane = state.focused_pane_mut(engine).expect("focused pane");
    pane.tabs.push(tab);
    surface_id
}

#[test]
fn keyboard_tab_switch_reifies_deferred_surface() {
    // given: pane 에 tab0(활성, 즉시) + tab1(deferred placeholder)
    let (mut state, mut engine) = test_state();
    let sid = add_deferred_tab(&mut state, &mut engine);
    assert!(
        engine.is_surface_deferred(sid),
        "precondition: tab1 은 deferred"
    );

    // 키보드 next_tab 경로는 active_tab 만 바꾸고 reify 하지 않는다(설계상 분리).
    state.next_tab_in_pane(&mut engine);
    assert!(
        engine.is_surface_deferred(sid),
        "전환 핸들러 자체는 reify 하지 않는다(표시 지점에서 처리)"
    );

    // 표시 지점(매 프레임 렌더 직전)에서 reify 되어야 한다.
    state.reify_displayed_surfaces(&mut engine);
    assert!(
        !engine.is_surface_deferred(sid),
        "표시 즉시 reify 되어야 함"
    );
    assert!(
        engine.terminals.contains(sid),
        "PTY 가 store 에 insert 되어야 함"
    );
}

#[test]
fn close_active_tab_reifies_newly_active_deferred_surface() {
    // given: tab0(활성, 즉시) + tab1(deferred). close 시 active 가 tab1 로 이동.
    let (mut state, mut engine) = test_state();
    let sid = add_deferred_tab(&mut state, &mut engine);
    assert!(engine.is_surface_deferred(sid));

    // when: 활성 탭(tab0) close → tab1 이 새 활성 탭(deferred)
    assert!(state.close_active_tab(&mut engine));
    assert!(
        engine.is_surface_deferred(sid),
        "close 직후엔 아직 deferred"
    );

    // then: 표시 지점 reify 가 새 활성 deferred surface 를 살린다.
    state.reify_displayed_surfaces(&mut engine);
    assert!(
        !engine.is_surface_deferred(sid),
        "close 로 활성된 deferred 탭이 reify 되어야 함"
    );
    assert!(engine.terminals.contains(sid));
}

#[test]
fn reify_displayed_surfaces_is_noop_without_deferred() {
    // deferred 가 없으면 표시 지점 호출은 아무 것도 spawn 하지 않는다(이중 spawn 방지).
    let (mut state, mut engine) = test_state();
    let before = collect_all_surface_ids(&mut state, &mut engine);
    state.reify_displayed_surfaces(&mut engine);
    let after = collect_all_surface_ids(&mut state, &mut engine);
    assert_eq!(before.len(), after.len(), "deferred 없으면 no-op");
}

// ---- workspace operations ----

fn add_test_workspace(state: &mut AppState, engine: &mut crate::core::CoreState) {
    let event = crate::core::apply_create_workspace_inner(
        engine,
        crate::core::WorkspaceCreationParams::terminal(),
    )
    .unwrap();
    let crate::core::intent::CoreEvent::WorkspaceCreated { index, .. } = event else {
        panic!("apply_create_workspace_inner 가 WorkspaceCreated 외 반환");
    };
    state.active_workspace = index;
}

// ---- mirror surface: 로컬 attention 발동 억제 ----

/// mirror 플래그가 선 워크스페이스를 하나 추가하고 그 첫 surface id 를 돌려준다.
/// 실제 attach 세션 없이 `Workspace.mirror` 만 세우면 `is_mirror_surface` 판정에는
/// 충분하다 — 그 판정이 보는 것이 워크스페이스 플래그뿐이기 때문.
fn add_mirror_test_workspace(state: &mut AppState, engine: &mut crate::core::CoreState) -> u32 {
    add_test_workspace(state, engine);
    let idx = state.active_workspace;
    let ws = engine
        .workspaces
        .get_mut(idx)
        .expect("방금 만든 workspace 가 있어야 한다");
    ws.mirror = true;
    *ws.all_surface_ids()
        .first()
        .expect("새 workspace 에 surface 하나")
}

/// 로컬 producer 축(`raise_attention`)은 mirror surface 에 레코드를 만들지 않는다.
/// 같은 호출이 mirror 아닌 surface 에는 그대로 발동한다(대조군) — 억제가 전역
/// 무력화가 아니라 mirror 한정임을 함께 고정한다.
#[test]
fn local_attention_raise_is_suppressed_on_mirror_surface() {
    use crate::core::AttentionKind;

    let (mut state, mut engine) = test_state();
    let local_sid = *engine
        .workspaces
        .get(state.active_workspace)
        .unwrap()
        .all_surface_ids()
        .first()
        .expect("기본 workspace 에 surface 하나");
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);

    engine.raise_attention(mirror_sid, AttentionKind::Completion);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "mirror surface 는 로컬 발동 대상이 아니다 — 서버 push 가 유일 소스"
    );

    engine.raise_attention(local_sid, AttentionKind::Completion);
    assert_eq!(
        engine.attention_kind(local_sid),
        Some(AttentionKind::Completion),
        "mirror 아닌 surface 는 그대로 발동해야 한다(억제는 mirror 한정)"
    );
}

/// mirror 터미널도 서버가 흘려준 바이트를 그대로 파싱하므로 OSC 133 D 사건 자체는
/// 미러에서도 발화한다 — 그게 이 버그의 전제다. 그 사건에 붙은 producer(
/// `cascade_terminal_command_completed` 의 자동 경로 = `raise_attention(Completion)`)
/// 만 mirror 에서 억제되고, mirror 아닌 surface 에서는 그대로 발동한다.
#[test]
fn osc133_command_completed_raises_attention_only_off_mirror() {
    use crate::core::AttentionKind;
    use tasty_terminal::TerminalEventKind;

    /// OSC 133 D(명령 완료, exit 0). `handle_prompt_boundary` 가 phase 'D' 에서
    /// `TerminalCommandCompleted` 를 만든다.
    const OSC133_D: &[u8] = b"\x1b]133;D;0\x07";

    let (mut state, mut engine) = test_state();
    let local_sid = *engine
        .workspaces
        .get(state.active_workspace)
        .unwrap()
        .all_surface_ids()
        .first()
        .expect("기본 workspace 에 surface 하나");
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);

    for sid in [local_sid, mirror_sid] {
        engine
            .find_terminal_by_id_mut(sid)
            .expect("terminal surface")
            .feed_bytes(OSC133_D);
    }

    // 여기서 `engine.collect_events()` 를 쓰지 않는다. 그쪽은 `try_take_events()` 라
    // **상태 락을 못 잡으면 그 터미널을 통째로 건너뛴다**(ADR-0002 — 입력 스레드가 바쁜
    // 파서 스레드들과 직렬화되지 않게 한 설계). 호스트 루프에서는 파서가 다시 깨우므로
    // 그 건너뜀이 손실이 아니지만, **한 번만 묻는 테스트**에서는 그 자리가 곧 유실이다.
    //
    // 이 두 surface 는 실제 PTY 를 들고 있고 각자 파서 스레드가 셸의 프롬프트 출력을
    // 아무 때나 `ingest` 한다(`tasty-terminal` 의 파서 루프가 `state.lock()` 을 잡는다).
    // 그래서 `feed_bytes` 로 **이미 실린** 사건이 건너뛰기 한 번에 안 보일 수 있다.
    // 실측(2026-09-06, `--bin tasty`): 전수 42 회 중 1 회 이 자리에서 실패, 같은 시험
    // 단독 40 회는 0 회 — 형제가 있어야 나는 경합이다.
    //
    // 처방은 **막는 take** 다. `sleep` 은 재현율만 낮추고 경합을 안 없애며, 낮아진
    // 재현율은 "고쳤다" 와 구별되지 않는다. 단정을 느슨하게 하는 것도 답이 아니다 —
    // 이 시험의 명제는 "두 터미널이 다 파싱한다" 이지 "하나라도 파싱한다" 가 아니다.
    let boundaries: Vec<u32> = [local_sid, mirror_sid]
        .into_iter()
        .filter(|sid| {
            engine
                .find_terminal_by_id_mut(*sid)
                .expect("terminal surface")
                .take_events()
                .iter()
                .any(|e| {
                    matches!(&e.kind, TerminalEventKind::PromptBoundary { phase, .. } if *phase == 'D')
                })
        })
        .collect();
    assert!(
        boundaries.contains(&mirror_sid),
        "mirror 터미널도 OSC 133 D 를 파싱한다(억제 지점은 파서가 아니라 producer): {boundaries:?}"
    );
    assert!(
        boundaries.contains(&local_sid),
        "로컬 터미널의 OSC 133 D 파싱은 그대로여야 한다: {boundaries:?}"
    );

    // cascade 의 자동 경로가 하는 일 그대로 — 두 surface 에 동일 호출.
    for sid in [local_sid, mirror_sid] {
        engine.raise_attention(sid, AttentionKind::Completion);
    }
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "mirror surface 에 도착한 OSC 133 D 는 attention 을 만들지 않는다"
    );
    assert_eq!(
        engine.attention_kind(local_sid),
        Some(AttentionKind::Completion),
        "같은 사건이 mirror 아닌 surface 에서는 그대로 attention 을 만든다"
    );
}

/// 억제 대상은 attention 레코드 **한 줄뿐**이다 — Bell / OSC 9·777 cascade 가 같이
/// 만드는 알림 패널 아이템은 mirror 에서도 그대로 생겨야 한다. 이게 무너지면 원격
/// 작업 중 벨 알림이 통째로 사라지는 별개 회귀다.
#[test]
fn mirror_surface_notification_item_survives_the_attention_gate() {
    use crate::core::AttentionKind;

    let (mut state, mut engine) = test_state();
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);
    let mirror_ws_id = engine
        .workspaces
        .get(state.active_workspace)
        .expect("mirror workspace")
        .id;

    // 알림 생성 cascade(`NotificationPushRequested`)가 하는 순서 그대로.
    let created = engine.notifications.add(
        mirror_ws_id,
        mirror_sid,
        "bell".to_string(),
        "ring".to_string(),
    );
    assert!(
        created.is_some(),
        "mirror surface 의 벨/알림도 패널 아이템은 그대로 만들어야 한다"
    );
    engine.raise_attention(mirror_sid, AttentionKind::Completion);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "알림은 남기되 attention 레코드만 억제한다"
    );
}

/// `surface.completion` IPC/CLI 가 mirror 의 **로컬** surface id 를 대상으로 불려도
/// (미러 인스턴스에서 도는 에이전트/플러그인이 그럴 수 있다) 레코드를 만들지 않는다.
/// 정책은 "억제" — 서버로 forward 하지 않는다(`docs/adr/0098-...`).
#[test]
fn surface_completion_on_mirror_surface_is_suppressed() {
    use crate::core::AttentionKind;

    let (mut state, mut engine) = test_state();
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);

    // `cascade_surface_completion` 이 하는 일 그대로 — kind 는 호출자가 정한다.
    engine.raise_attention(mirror_sid, AttentionKind::NeedsInput);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "mirror 대상 surface.completion 은 억제된다(kind 무관)"
    );
}

/// 억제 게이트가 **서버 push 적용 경로를 막지 않는다.** 적용은 로컬 producer 축이
/// 아니라 원격 전용 진입점(`set_mirror_surface_attention`)이라, 같은 mirror surface
/// 에 대해서도 값이 그대로 남아야 한다 — 이게 막히면 미러 배지가 전부 사라진다.
#[test]
fn server_push_apply_is_not_blocked_by_the_mirror_gate() {
    use crate::core::AttentionKind;

    let (mut state, mut engine) = test_state();
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);

    engine.set_mirror_surface_attention(mirror_sid, Some(AttentionKind::NeedsInput));
    assert_eq!(
        engine.attention_kind(mirror_sid),
        Some(AttentionKind::NeedsInput),
        "서버 push 는 억제 대상이 아니다 — 미러의 유일한 attention 소스"
    );

    // 그 뒤에 로컬 producer 가 끼어들어도 서버 값을 덮어쓰지 못한다.
    engine.raise_attention(mirror_sid, AttentionKind::Completion);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        Some(AttentionKind::NeedsInput),
        "로컬 발동이 서버 push 값을 덮어쓰면 안 된다"
    );

    engine.set_mirror_surface_attention(mirror_sid, None);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "서버가 내려준 해제도 그대로 적용된다"
    );
}

// ---- occupancy > completion 우선순위, NeedsInput > occupancy (ADR-0040, ADR-0062) ----

/// 점유(soft) 중 surface 는 Completion 하이라이트가 억제된다: `regions_from_state` 의
/// 해당 region `kind` 가 `None`. 점유 없이 attention 만 있으면 `Some(Completion)`(대조군).
#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn occupancy_suppresses_completion_highlight() {
    use crate::adapters::ui::divider::regions_from_state;
    use crate::core::AttentionKind;
    use crate::model::{PhysicalPx, PhysicalRect};

    let (state, mut engine) = test_state();
    let sids = engine
        .workspaces
        .get(state.active_workspace)
        .unwrap()
        .all_surface_ids();
    let sid = *sids.first().expect("기본 workspace 에 surface 하나");

    let term_rect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(800.0),
        height: PhysicalPx(600.0),
    };

    // 대조군: attention 만 → kind Some(Completion).
    engine.raise_attention(sid, AttentionKind::Completion);
    let regions = regions_from_state(&state, &engine, term_rect, 1.0);
    assert!(
        regions
            .iter()
            .any(|r| r.kind == Some(AttentionKind::Completion)),
        "점유 없이 attention 만이면 완료 테두리가 그려져야 한다"
    );

    // soft 점유 추가 → 억제(kind None).
    engine
        .attach
        .acquire_soft(sid, /* parent */ 9999, Some("agent".into()))
        .expect("soft 점유 획득");
    let regions = regions_from_state(&state, &engine, term_rect, 1.0);
    assert!(
        regions.iter().all(|r| r.kind.is_none()),
        "점유 중이면 완료 테두리를 억제해야 한다(점유 > 완료)"
    );
}

/// NeedsInput 은 점유보다 우선순위가 높아 점유 중에도 억제되지 않는다 — "지금 답하지
/// 않으면 멈춘다"는 신호를 점유(정상적으로 잡혀 작업 중)가 가리면 안 되기 때문.
#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn needs_input_not_suppressed_by_occupancy() {
    use crate::adapters::ui::divider::regions_from_state;
    use crate::core::AttentionKind;
    use crate::model::{PhysicalPx, PhysicalRect};

    let (state, mut engine) = test_state();
    let sids = engine
        .workspaces
        .get(state.active_workspace)
        .unwrap()
        .all_surface_ids();
    let sid = *sids.first().expect("기본 workspace 에 surface 하나");

    let term_rect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(800.0),
        height: PhysicalPx(600.0),
    };

    engine
        .attach
        .acquire_soft(sid, /* parent */ 9999, Some("agent".into()))
        .expect("soft 점유 획득");
    engine.raise_attention(sid, AttentionKind::NeedsInput);
    let regions = regions_from_state(&state, &engine, term_rect, 1.0);
    assert!(
        regions
            .iter()
            .any(|r| r.kind == Some(AttentionKind::NeedsInput)),
        "점유 중이어도 NeedsInput 테두리는 억제되지 않아야 한다(NeedsInput > 점유)"
    );
}

#[test]
fn add_workspace_increments_count() {
    let (mut state, mut engine) = test_state();
    assert_eq!(engine.workspaces.len(), 1);
    add_test_workspace(&mut state, &mut engine);
    assert_eq!(engine.workspaces.len(), 2);
}

#[test]
fn switch_workspace_valid() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine);
    assert_eq!(state.active_workspace, 1);

    state.switch_workspace(&mut engine, 0);
    assert_eq!(state.active_workspace, 0);
}

#[test]
fn switch_workspace_out_of_range() {
    let (mut state, mut engine) = test_state();
    state.switch_workspace(&mut engine, 999);
    assert_eq!(state.active_workspace, 0);
}

// ---- next/prev workspace within active category ----

/// 단일 normal 카테고리에 워크스페이스 3개(A=0, B=1, C=2)일 때 next 는
/// A→B→C→A wrap, prev 는 역순으로 순환한다.
#[test]
fn next_prev_workspace_single_category_wraps() {
    let (mut state, mut engine) = test_state();
    // test_state 는 A(0) 하나로 시작 → B(1), C(2) 추가.
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    assert_eq!(engine.workspaces.len(), 3);

    state.switch_workspace(&mut engine, 0); // active = A
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 1); // B
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 2); // C
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 0); // wrap → A

    state.prev_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 2); // wrap → C
    state.prev_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 1); // B
}

/// normal 카테고리에 A(0)/C(2), work 카테고리에 B(1)/D(3) 일 때
/// 이동은 같은 카테고리 안에서만 wrap 하고 다른 카테고리 항목은 건너뛴다.
#[test]
fn next_workspace_in_active_category_wraps_within_category_only() {
    let (mut state, mut engine) = test_state();
    // A(0) 는 test_state 기본. B(1), C(2), D(3) 추가.
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    add_test_workspace(&mut state, &mut engine); // D=3
    assert_eq!(engine.workspaces.len(), 4);

    // add_test_workspace 는 카테고리를 안 붙이므로 work 카테고리를 만들어 B/D 재배정.
    let work = engine
        .create_category("work")
        .expect("create work category");
    engine.workspaces[1].set_category(work); // B
    engine.workspaces[3].set_category(work); // D

    // active = A (normal 카테고리): A(0) ↔ C(2) 사이에서만 순환.
    state.switch_workspace(&mut engine, 0);
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 2); // C (B=1 건너뜀)
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 0); // wrap → A
    state.prev_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 2); // wrap → C

    // active = B (work 카테고리): B(1) ↔ D(3) 사이에서만 순환.
    state.switch_workspace(&mut engine, 1);
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 3); // D (C=2 건너뜀)
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 1); // wrap → B
    state.prev_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 3); // wrap → D
}

// ---- workspace_switch_crosses_category (workspace 축 next/prev 의 카테고리 경계 넘기) ----

/// normal=A(0)/B(1), work=C(2)/D(3). 옵션 off(기본)면 카테고리 경계에서 로컬 wrap 만
/// 유지한다(회귀 없음).
#[test]
fn crosses_category_off_keeps_local_wrap() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    add_test_workspace(&mut state, &mut engine); // D=3
    let work = engine.create_category("work").expect("create work");
    engine.workspaces[2].set_category(work); // C
    engine.workspaces[3].set_category(work); // D
    assert!(!engine.settings.general.workspace_switch_crosses_category);

    state.switch_workspace(&mut engine, 1); // active = B (normal 의 마지막)
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 0); // wrap → A (normal 의 첫), work 로 넘어가지 않음
}

/// 옵션 on 이면 카테고리 마지막 워크스페이스에서 next 가 다음 카테고리의 **첫**
/// 워크스페이스로 이동한다(last-active 착지 아님).
#[test]
fn crosses_category_on_next_lands_on_next_category_first() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    add_test_workspace(&mut state, &mut engine); // D=3
    let work = engine.create_category("work").expect("create work");
    engine.workspaces[2].set_category(work); // C (work 의 first)
    engine.workspaces[3].set_category(work); // D

    // work 를 D(3) 로 마지막 방문해둬도 착지는 항상 first(C) 여야 한다(방향성 유지).
    state.switch_workspace(&mut engine, 3);
    state.switch_workspace(&mut engine, 1); // active = B (normal 의 마지막)
    engine.settings.general.workspace_switch_crosses_category = true;

    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 2); // work 의 first = C (D 의 last-active 아님)
}

/// 옵션 on 이면 카테고리 첫 워크스페이스에서 prev 가 이전 카테고리의 **마지막**
/// 워크스페이스로 이동한다.
#[test]
fn crosses_category_on_prev_lands_on_prev_category_last() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    add_test_workspace(&mut state, &mut engine); // D=3
    let work = engine.create_category("work").expect("create work");
    engine.workspaces[2].set_category(work); // C
    engine.workspaces[3].set_category(work); // D (work 의 last)
    engine.settings.general.workspace_switch_crosses_category = true;

    state.switch_workspace(&mut engine, 2); // active = C (work 의 첫)
    state.prev_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 1); // normal 의 last = B
}

/// 옵션 on 이면 카테고리 목록 자체도 wrap 한다 — 마지막 카테고리의 마지막
/// 워크스페이스에서 next 는 첫 카테고리의 첫 워크스페이스로 돌아온다.
#[test]
fn crosses_category_on_wraps_across_full_category_list() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    add_test_workspace(&mut state, &mut engine); // D=3
    let work = engine.create_category("work").expect("create work");
    engine.workspaces[2].set_category(work); // C (work 의 first)
    engine.workspaces[3].set_category(work); // D (work 의 last, 마지막 카테고리)
    engine.settings.general.workspace_switch_crosses_category = true;

    state.switch_workspace(&mut engine, 3); // active = D (마지막 카테고리의 마지막)
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 0); // wrap → normal 의 first = A
}

/// 옵션 on 이어도 카테고리가 1개뿐이면(카테고리 기능 off 포함) 넘어갈 인접 카테고리가
/// 없으므로 기존 로컬 wrap 과 동일하게 동작한다.
#[test]
fn crosses_category_on_single_category_falls_back_to_local_wrap() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    assert!(!engine.settings.general.workspace_categories_enabled);
    engine.settings.general.workspace_switch_crosses_category = true;

    state.switch_workspace(&mut engine, 2); // active = C (normal 의 마지막, 유일한 카테고리)
    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 0); // wrap → A, off 일 때와 동일
}

// ---- category quick-switch (T4WS ②⑤) ----

/// normal=A(0)/C(2), work=B(1)/D(3). 카테고리 전환은 대상 카테고리의 last-active 로 착지한다.
#[test]
fn switch_to_category_lands_on_last_active() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    add_test_workspace(&mut state, &mut engine); // D=3
    let work = engine.create_category("work").expect("create work");
    engine.workspaces[1].set_category(work); // B
    engine.workspaces[3].set_category(work); // D

    // work 안에서 D(3)를 마지막으로 방문 → last-active[work]=3.
    state.switch_workspace(&mut engine, 3);
    // normal 로 이동.
    state.switch_workspace(&mut engine, 0);
    assert_eq!(state.active_workspace, 0);

    // section 1 = work 로 카테고리 전환 → 마지막 방문 D(3) 로 착지.
    state.switch_to_category(&mut engine, 1);
    assert_eq!(state.active_workspace, 3);
}

/// 한 번도 방문 안 한 카테고리로 전환하면 그 카테고리의 first 로 착지한다.
#[test]
fn switch_to_category_falls_back_to_first_when_never_visited() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    add_test_workspace(&mut state, &mut engine); // D=3
    let work = engine.create_category("work").expect("create work");
    engine.workspaces[1].set_category(work); // B (work 의 first)
    engine.workspaces[3].set_category(work); // D

    // work 미방문 상태에서 active=A(0). section 1 로 전환 → work first = B(1).
    state.switch_workspace(&mut engine, 0);
    state.switch_to_category(&mut engine, 1);
    assert_eq!(state.active_workspace, 1);
}

/// 접힌 카테고리로 전환하면 auto-expand(collapsed=false) 되고 그 안으로 착지한다.
#[test]
fn switch_to_category_auto_expands_collapsed() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    let work = engine.create_category("work").expect("create work");
    engine.workspaces[1].set_category(work); // B
    engine.set_category_collapsed(work, true);
    assert!(engine.categories()[1].collapsed);

    state.switch_workspace(&mut engine, 0); // active=A(normal)
    state.switch_to_category(&mut engine, 1); // → work
    assert!(!engine.categories()[1].collapsed); // auto-expand
    assert_eq!(state.active_workspace, 1); // work first = B
}

/// 존재하지 않는 섹션 인덱스는 no-op.
#[test]
fn switch_to_category_out_of_range_noop() {
    let (mut state, mut engine) = test_state();
    state.switch_workspace(&mut engine, 0);
    state.switch_to_category(&mut engine, 99);
    assert_eq!(state.active_workspace, 0);
}

// ---- category axis next/prev (S-9) ----

/// normal=A(0), work=B(1), play=C(2) — 각 카테고리에 워크스페이스 1개씩. next_category
/// 는 카테고리 리스트(0=normal, 1.., 등록 순서) 를 wrap-around 순회하며 각 카테고리의
/// (미방문이므로) first 워크스페이스로 착지한다.
#[test]
fn next_prev_category_wraps_across_categories() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    let work = engine.create_category("work").expect("create work");
    let play = engine.create_category("play").expect("create play");
    engine.workspaces[1].set_category(work); // B
    engine.workspaces[2].set_category(play); // C

    state.switch_workspace(&mut engine, 0); // active = A (normal)
    state.next_category(&mut engine);
    assert_eq!(state.active_workspace, 1); // work → B
    state.next_category(&mut engine);
    assert_eq!(state.active_workspace, 2); // play → C
    state.next_category(&mut engine);
    assert_eq!(state.active_workspace, 0); // wrap → normal → A

    state.prev_category(&mut engine);
    assert_eq!(state.active_workspace, 2); // wrap → play → C
    state.prev_category(&mut engine);
    assert_eq!(state.active_workspace, 1); // work → B
}

/// 카테고리가 normal 하나뿐이면 next/prev_category 가 no-op.
#[test]
fn next_prev_category_noop_when_single_category() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // 같은 normal 카테고리에 워크스페이스 추가.
    assert_eq!(state.active_workspace, 1);

    state.next_category(&mut engine);
    assert_eq!(state.active_workspace, 1);
    state.prev_category(&mut engine);
    assert_eq!(state.active_workspace, 1);
}

/// 카테고리 전환은 대상 카테고리의 last-active 로 착지(switch_to_category 재사용 확인).
#[test]
fn next_category_lands_on_last_active() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine); // B=1
    add_test_workspace(&mut state, &mut engine); // C=2
    let work = engine.create_category("work").expect("create work");
    engine.workspaces[1].set_category(work); // B
    engine.workspaces[2].set_category(work); // C

    // work 안에서 C(2)를 마지막으로 방문.
    state.switch_workspace(&mut engine, 2);
    state.switch_workspace(&mut engine, 0); // normal 로 복귀.

    state.next_category(&mut engine); // → work, last-active = C.
    assert_eq!(state.active_workspace, 2);
}

/// 카테고리 내 워크스페이스가 자기 자신 하나뿐이면 next/prev 가 no-op.
#[test]
fn next_prev_workspace_in_active_category_noop_when_alone() {
    let (mut state, mut engine) = test_state();
    assert_eq!(engine.workspaces.len(), 1);
    assert_eq!(state.active_workspace, 0);

    state.next_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 0);
    state.prev_workspace_in_active_category(&mut engine);
    assert_eq!(state.active_workspace, 0);
}

// ---- resolve_inherit_cwd_from_surface ----

#[cfg(feature = "gui")] // markdown surface 생성이 gui 전용 remote-kind 등록에 의존한다
#[test]
fn resolve_inherit_cwd_from_markdown_surface() {
    // markdown(EguiMeshSurface) 의 source_cwd(파일 부모 디렉터리) 로 검증.
    let (mut state, mut engine) = test_state();
    #[cfg(windows)]
    let (root, file) = ("C:\\workspace\\proj", "C:\\workspace\\proj\\readme.md");
    #[cfg(not(windows))]
    let (root, file) = ("/workspace/proj", "/workspace/proj/readme.md");
    state
        .test_add_markdown_tab(&mut engine, file.to_string())
        .unwrap();

    let mut sid_opt = None;
    for ws in &engine.workspaces {
        for pid in ws.pane_layout().all_pane_ids() {
            if let Some(p) = ws.pane_layout().find_pane(pid) {
                for tab in &p.tabs {
                    if let Some(s) = tab.layout().find_surface(tab.focused_surface)
                        && s.kind() == "markdown"
                    {
                        sid_opt = s.surface_id();
                    }
                }
            }
        }
    }
    let sid = sid_opt.expect("markdown surface should exist");
    assert_eq!(
        state.resolve_inherit_cwd_from_surface(&engine, sid),
        Some(std::path::PathBuf::from(root))
    );
}

#[cfg(feature = "gui")] // markdown surface 생성이 gui 전용 remote-kind 등록에 의존한다
#[test]
fn resolve_inherit_cwd_from_surface_respects_toggle_off() {
    let (mut state, mut engine) = test_state();
    engine.settings.general.inherit_cwd = false;

    #[cfg(windows)]
    let file = "C:\\workspace\\proj\\readme.md";
    #[cfg(not(windows))]
    let file = "/workspace/proj/readme.md";
    state
        .test_add_markdown_tab(&mut engine, file.to_string())
        .unwrap();

    let mut sid_opt = None;
    for ws in &engine.workspaces {
        for pid in ws.pane_layout().all_pane_ids() {
            if let Some(p) = ws.pane_layout().find_pane(pid) {
                for tab in &p.tabs {
                    if let Some(s) = tab.layout().find_surface(tab.focused_surface)
                        && s.kind() == "markdown"
                    {
                        sid_opt = s.surface_id();
                    }
                }
            }
        }
    }
    let sid = sid_opt.expect("markdown surface should exist");
    assert_eq!(state.resolve_inherit_cwd_from_surface(&engine, sid), None);
}

#[test]
fn resolve_inherit_cwd_from_unknown_surface_is_none() {
    let (state, engine) = test_state();
    let _ = &engine;
    assert_eq!(state.resolve_inherit_cwd_from_surface(&engine, 99999), None);
}

// ---- 원격 출처 cwd 는 로컬 실행 자리로 새지 않는다 (surface-cwd §3-2) ----

/// explorer 탭을 열어 focus 시키고 그 surface id 를 돌려준다. explorer root 는 서버가 mirror
/// 디스크립터에 싣는 원격 root 가 오늘도 `source_cwd()` 로 흘러나오는 갈래라, push 채널 없이도
/// 원격 출처 cwd 를 재현하는 가장 짧은 경로다.
fn focused_explorer(
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
) -> (u32, std::path::PathBuf) {
    let root = crate::test_support::abs_path("remote/proj");
    let (_tab, sid) = state
        .add_kind_tab(
            engine,
            "explorer",
            &serde_json::json!({ "path": root.to_string_lossy() }),
        )
        .expect("add explorer tab");
    assert_eq!(
        state.focused_surface_id(engine),
        Some(sid),
        "새 탭이 focus 를 받는다"
    );
    (sid, root)
}

#[test]
fn local_explorer_cwd_is_local_and_inherited() {
    let (mut state, mut engine) = test_state();
    let (sid, root) = focused_explorer(&mut state, &mut engine);
    assert_eq!(
        engine.surface_cwd(sid),
        Some(crate::core::state::SurfaceCwd::Local(root.clone()))
    );
    assert_eq!(state.resolve_inherit_cwd(&engine), Some(root.clone()));
    assert_eq!(
        state.resolve_inherit_cwd_from_surface(&engine, sid),
        Some(root)
    );
}

/// mirror explorer 를 focus 한 채 새 워크스페이스를 만들면 그 첫 PTY 의 `working_dir` 은
/// `resolve_inherit_cwd` 에서 온다(`intent/workspace.rs` · `handler/workspace.rs`). 그 값이 원격
/// root 가 아니라 `None`(= 홈)이어야 한다.
#[test]
fn mirror_explorer_cwd_is_remote_and_not_inherited_locally() {
    let (mut state, mut engine) = test_state();
    let (sid, root) = focused_explorer(&mut state, &mut engine);
    state.active_workspace_mut(&mut engine).mirror = true;

    assert_eq!(
        engine.surface_cwd(sid),
        Some(crate::core::state::SurfaceCwd::Remote(
            crate::core::state::RemoteCwd::new(root.to_string_lossy().into_owned())
        )),
        "mirror 워크스페이스의 cwd 는 원격 출처로 분류된다"
    );
    assert_eq!(engine.local_surface_cwd(sid), None);
    assert_eq!(state.resolve_inherit_cwd(&engine), None);
    assert_eq!(state.resolve_inherit_cwd_from_surface(&engine, sid), None);
}

/// popup context 의 cwd 키 셋 — `cwd`(게이트·로컬) · `observed_cwd`(게이트 없음·로컬) ·
/// `remote_cwd`(mirror) 가 서로 섞이지 않는다.
#[test]
fn popup_context_splits_cwd_keys_by_gate_and_provenance() {
    let (mut state, mut engine) = test_state();
    let (sid, root) = focused_explorer(&mut state, &mut engine);
    let root_s = root.to_string_lossy().into_owned();

    let local = state.popup_surface_context(&engine, Some(sid));
    assert_eq!(local["cwd"], serde_json::json!(root_s));
    assert_eq!(local["observed_cwd"], serde_json::json!(root_s));
    assert_eq!(local["origin_surface_id"], serde_json::json!(sid));
    assert!(local.get("remote_cwd").is_none());
    assert!(local.get("mirror").is_none());

    // `inherit_cwd` off — 새 surface 용 `cwd` 만 비고, 보고 있는 폴더는 남는다.
    engine.settings.general.inherit_cwd = false;
    let gated = state.popup_surface_context(&engine, Some(sid));
    assert!(gated["cwd"].is_null());
    assert_eq!(gated["observed_cwd"], serde_json::json!(root_s));

    // mirror — 원격 경로는 `remote_cwd` 에만 실린다.
    engine.settings.general.inherit_cwd = true;
    state.active_workspace_mut(&mut engine).mirror = true;
    let mirror = state.popup_surface_context(&engine, Some(sid));
    assert!(
        mirror["cwd"].is_null(),
        "원격 경로는 `cwd` 키로 새지 않는다"
    );
    assert!(mirror.get("observed_cwd").is_none());
    assert_eq!(mirror["remote_cwd"], serde_json::json!(root_s));
    assert_eq!(mirror["mirror"], serde_json::json!(true));
    assert_eq!(mirror["local_surface_id"], serde_json::json!(sid));
}

#[test]
fn surface_display_path_returns_workspace_and_tab_names() {
    let (mut state, mut engine) = test_state();
    let surface_ids = collect_surface_ids(&mut state, &mut engine);
    let sid = surface_ids[0];
    let path = engine
        .surface_display_path(sid)
        .expect("path for existing surface");
    let ws = state.active_workspace(&engine);
    assert_eq!(path.workspace_name, ws.name);
    assert!(path.tab_name.is_some());
}

#[test]
fn surface_display_path_unknown_surface_is_none() {
    let (_state, engine) = test_state();
    assert!(engine.surface_display_path(99999).is_none());
}

/// 활성 워크스페이스에서 `sid` 의 ExplorerPanel 을 찾아 반환(테스트 헬퍼).
fn explorer_of<'a>(
    engine: &'a crate::core::CoreState,
    state: &AppState,
    sid: u32,
) -> Option<&'a crate::model::ExplorerPanel> {
    let ws = state.active_workspace(engine);
    for pid in ws.pane_layout().all_pane_ids() {
        let Some(pane) = ws.pane_layout().find_pane(pid) else {
            continue;
        };
        for tab in &pane.tabs {
            if let Some(s) = tab.layout().find_surface(sid) {
                return s.as_any().downcast_ref::<crate::model::ExplorerPanel>();
            }
        }
    }
    None
}

#[test]
fn add_kind_tab_by_owner_opens_explorer_with_folder_cwd() {
    let (mut state, mut engine) = test_state();
    // 초기 pane 의 focused surface(터미널)를 owner 로 지정.
    let owner = state.focused_surface_id(&engine).expect("focused surface");
    // explorer root 는 절대경로만 채택하므로 경로 리터럴도 플랫폼 절대경로로 만든다.
    let folder = crate::test_support::abs_path("proj/sub");
    let (_tab, sid) = state
        .add_kind_tab_by_owner(
            &mut engine,
            owner,
            "explorer",
            &serde_json::json!({ "path": folder.to_string_lossy() }),
        )
        .expect("add explorer tab in owner pane");
    let ex = explorer_of(&engine, &state, sid).expect("explorer surface exists");
    // 새 explorer 는 cwd=current=folder (source_cwd 는 model 단위 테스트에서 검증).
    assert_eq!(ex.cwd(), folder.as_path());
    assert_eq!(ex.current_root(), folder.as_path());
}

#[test]
fn set_explorer_cwd_moves_root_and_clears_history() {
    let (mut state, mut engine) = test_state();
    let (_t, sid) = state
        .add_kind_tab(
            &mut engine,
            "explorer",
            &serde_json::json!({ "path": "/proj" }),
        )
        .expect("add explorer tab");
    state.set_explorer_cwd(&mut engine, sid, std::path::PathBuf::from("/other"));
    let ex = explorer_of(&engine, &state, sid).expect("explorer surface exists");
    assert_eq!(ex.cwd(), std::path::Path::new("/other"));
    assert_eq!(ex.current_root(), std::path::Path::new("/other"));
    assert!(!ex.active_tab().can_go_back());
}

// ---- 19: 탭바 클릭 → 비-focused pane 으로 focus 이동 ----
//
// 2-pane 구성(pane_a=원래 pane, pane_b=split 로 새로 생긴 pane — split 직후
// focused)에서, pane_a(비-focused) 유래 탭바 액션을 `apply_tab_bar_actions` 로
// 적용했을 때 `focused_pane` 이 pane_a 로 옮겨가는지 검증한다.

/// pane 이 2개(pane_a, pane_b)인 워크스페이스를 만들고, split 직후 focus 인
/// pane_b 와 비-focused pane_a 의 ID를 반환한다.
#[cfg(feature = "gui")]
fn two_pane_setup(
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
) -> (
    u32, /* pane_a: 비focused */
    u32, /* pane_b: focused */
) {
    let pane_a = state.active_workspace(engine).focused_pane;
    state
        .test_split_pane(engine, SplitDirection::Vertical)
        .unwrap();
    let pane_b = state.active_workspace(engine).focused_pane;
    assert_ne!(pane_a, pane_b, "split 은 새 pane 을 만들어야 한다");
    (pane_a, pane_b)
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn switch_tab_on_other_pane_moves_focus() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    let (mut state, mut engine) = test_state();
    let (pane_a, pane_b) = two_pane_setup(&mut state, &mut engine);
    assert_eq!(state.active_workspace(&engine).focused_pane, pane_b);

    apply_tab_bar_actions(
        &mut state,
        &mut engine,
        vec![TabBarAction::SwitchTab {
            pane_id: pane_a,
            tab_index: 0,
        }],
        &[],
        160.0,
        1.0,
    );

    assert_eq!(
        state.active_workspace(&engine).focused_pane,
        pane_a,
        "비-focused pane 의 탭 클릭은 그 pane 으로 focus 를 옮겨야 한다"
    );
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn focus_pane_action_moves_focus_without_switching_tab() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    let (mut state, mut engine) = test_state();
    let (pane_a, pane_b) = two_pane_setup(&mut state, &mut engine);

    // pane_a 에 탭을 하나 더 추가하고 두번째 탭을 활성으로 만든다(빈 영역 클릭이
    // active_tab 을 건드리지 않는지 확인하기 위한 대조군).
    state.active_workspace_mut(&mut engine).focused_pane = pane_a;
    state.add_tab(&mut engine).unwrap();
    let active_before = state
        .active_workspace(&engine)
        .pane_layout()
        .find_pane(pane_a)
        .unwrap()
        .active_tab;
    assert_eq!(active_before, 1);
    state.active_workspace_mut(&mut engine).focused_pane = pane_b;

    apply_tab_bar_actions(
        &mut state,
        &mut engine,
        vec![TabBarAction::FocusPane { pane_id: pane_a }],
        &[],
        160.0,
        1.0,
    );

    assert_eq!(
        state.active_workspace(&engine).focused_pane,
        pane_a,
        "탭바 빈 영역 클릭은 그 pane 으로 focus 를 옮겨야 한다"
    );
    let active_after = state
        .active_workspace(&engine)
        .pane_layout()
        .find_pane(pane_a)
        .unwrap()
        .active_tab;
    assert_eq!(
        active_after, active_before,
        "빈 영역 클릭은 focus 만 이동하고 active_tab 은 건드리지 않아야 한다"
    );
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn scroll_left_and_right_on_other_pane_move_focus() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    for action_of in [
        (|pane_id| TabBarAction::ScrollLeft { pane_id }) as fn(u32) -> TabBarAction,
        (|pane_id| TabBarAction::ScrollRight { pane_id }) as fn(u32) -> TabBarAction,
    ] {
        let (mut state, mut engine) = test_state();
        let (pane_a, pane_b) = two_pane_setup(&mut state, &mut engine);
        assert_eq!(state.active_workspace(&engine).focused_pane, pane_b);

        apply_tab_bar_actions(
            &mut state,
            &mut engine,
            vec![action_of(pane_a)],
            &[],
            160.0,
            1.0,
        );

        assert_eq!(
            state.active_workspace(&engine).focused_pane,
            pane_a,
            "스크롤 화살표 클릭도 그 pane 으로 focus 를 옮겨야 한다"
        );
    }
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn close_tab_on_other_pane_moves_focus() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    let (mut state, mut engine) = test_state();
    let (pane_a, pane_b) = two_pane_setup(&mut state, &mut engine);

    // close_tab 은 마지막 탭을 보호하므로(pane.rs::close_tab) pane_a 에 탭을 하나
    // 더 추가해 close 가 실제로 일어나게 한다 — pane 자체는 사라지지 않는다.
    state.active_workspace_mut(&mut engine).focused_pane = pane_a;
    state.add_tab(&mut engine).unwrap();
    state.active_workspace_mut(&mut engine).focused_pane = pane_b;

    apply_tab_bar_actions(
        &mut state,
        &mut engine,
        vec![TabBarAction::CloseTab {
            pane_id: pane_a,
            tab_index: 0,
        }],
        &[],
        160.0,
        1.0,
    );

    assert_eq!(
        state.active_workspace(&engine).focused_pane,
        pane_a,
        "다른 pane 의 탭을 close 해도 그 pane 으로 focus 를 옮겨야 한다"
    );
    assert_eq!(
        state
            .active_workspace(&engine)
            .pane_layout()
            .find_pane(pane_a)
            .unwrap()
            .tabs
            .len(),
        1,
        "탭이 실제로 닫혀야 한다"
    );
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn context_menu_actions_do_not_move_focus() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    let (mut state, mut engine) = test_state();
    let (pane_a, pane_b) = two_pane_setup(&mut state, &mut engine);
    assert_eq!(state.active_workspace(&engine).focused_pane, pane_b);

    let actions = vec![
        TabBarAction::OpenContextMenu {
            pane_id: pane_a,
            tab_index: 0,
            pos: egui::Pos2::ZERO,
        },
        TabBarAction::OpenPaneContextMenu {
            pane_id: pane_a,
            pos: egui::Pos2::ZERO,
        },
        TabBarAction::OpenNewTabButtonContextMenu {
            pane_id: pane_a,
            pos: egui::Pos2::ZERO,
        },
    ];
    apply_tab_bar_actions(&mut state, &mut engine, actions, &[], 160.0, 1.0);

    assert_eq!(
        state.active_workspace(&engine).focused_pane,
        pane_b,
        "우클릭 컨텍스트 메뉴는 focus 를 옮기면 안 된다(대상은 pending_native_menu 의 pane_id 로 이미 결정됨)"
    );
}

// ── cleanup_surface 의 memory scope purge ──────────────────────────────────

/// `test_state_with_memory` 로 concrete mock 을 들고 AppState 를 만든다.
/// 반환한 `Arc<Mutex<InMemoryStorage>>` 로 close 이후 호출 이력을 검사한다.
fn test_state_with_mock_memory() -> (
    AppState,
    crate::core::CoreState,
    std::sync::Arc<std::sync::Mutex<tasty_memory::testing::InMemoryStorage>>,
) {
    let mock = std::sync::Arc::new(std::sync::Mutex::new(
        tasty_memory::testing::InMemoryStorage::new(),
    ));
    let (state, engine) = test_state_with_memory(mock.clone());
    (state, engine, mock)
}

#[test]
fn cleanup_surface_purges_surface_scope_exactly_once() {
    let (mut state, mut engine, mock) = test_state_with_mock_memory();
    let sid = collect_surface_ids(&mut state, &mut engine)[0];
    let scope = tasty_memory::Scope::Surface(sid);

    // seed 단계의 put 은 purge 이력에 잡히지 않는다 — 카운터는 purge_scope 전용.
    state.with_memory(|m| {
        crate::surface_meta::SurfaceMetaStore::set(m, sid, "nickname", "before-close").unwrap();
    });
    assert_eq!(
        mock.lock().unwrap().purge_scope_call_count(&scope),
        0,
        "close 전에는 purge 가 없어야 한다"
    );

    state.cleanup_surface(&mut engine, sid, None);

    // 회귀 대상: 과거엔 SurfaceMetaStore::remove 와 purge_surface_memory_scope 가
    // 같은 인자로 같은 함수를 불러 2였다. purge_scope 는 매 호출 끝에 memory 테이블
    // 풀스캔을 하므로 이 중복이 그대로 close 비용이 된다.
    // 락은 한 번만 잡아 지역으로 뽑는다 — 어서션 실패 메시지에서 다시 `mock.lock()`
    // 하면 첫 guard 가 살아 있는 채로 재진입해 std Mutex 가 데드락한다(성공 경로에선
    // 포맷 인자가 평가되지 않아 드러나지 않고, 회귀가 났을 때만 멈춘다).
    let (count, calls) = {
        let guard = mock.lock().unwrap();
        (
            guard.purge_scope_call_count(&scope),
            guard.purge_scope_calls().to_vec(),
        )
    };
    assert_eq!(
        count, 1,
        "surface close 당 purge_scope(Scope::Surface) 는 1회여야 한다 (호출 이력: {calls:?})"
    );
}

#[test]
fn cleanup_surface_still_clears_surface_scope_entries() {
    let (mut state, mut engine, _mock) = test_state_with_mock_memory();
    let sid = collect_surface_ids(&mut state, &mut engine)[0];

    state.with_memory(|m| {
        crate::surface_meta::SurfaceMetaStore::set(m, sid, "nickname", "doomed").unwrap();
        crate::surface_meta::SurfaceMetaStore::set(m, sid, "restore.command", "claude -r x")
            .unwrap();
    });
    assert_eq!(
        state
            .with_memory(|m| crate::surface_meta::SurfaceMetaStore::list(m, sid))
            .len(),
        2
    );

    state.cleanup_surface(&mut engine, sid, None);

    // 중복 제거가 "삭제를 통째로 빼먹는" 형태로 잘못 구현되지 않았는지 — 남은 1회가
    // 실제로 scope 를 비워야 한다.
    assert!(
        state
            .with_memory(|m| crate::surface_meta::SurfaceMetaStore::list(m, sid))
            .is_empty(),
        "close 후 surface scope 에 키가 남으면 안 된다"
    );
    assert_eq!(
        state.with_memory(|m| crate::surface_meta::SurfaceMetaStore::get(m, sid, "nickname")),
        None
    );
}

#[test]
fn workspace_close_purges_each_surface_scope_once() {
    let (mut state, mut engine, mock) = test_state_with_mock_memory();
    // 탭을 늘려 N-surface 워크스페이스를 만든다 — 중복이 남아 있으면 2N 이 된다.
    state.add_tab(&mut engine).expect("add_tab");
    state.add_tab(&mut engine).expect("add_tab");
    let sids = collect_surface_ids(&mut state, &mut engine);
    assert!(sids.len() >= 3, "탭 3개 이상이어야 의미 있다: {sids:?}");

    for sid in &sids {
        state.cleanup_surface(&mut engine, *sid, None);
    }

    let guard = mock.lock().unwrap();
    for sid in &sids {
        assert_eq!(
            guard.purge_scope_call_count(&tasty_memory::Scope::Surface(*sid)),
            1,
            "surface {sid} 의 purge 가 1회가 아니다"
        );
    }
    assert_eq!(
        guard.purge_scope_calls().len(),
        sids.len(),
        "surface 수만큼만 purge 해야 한다 (2N 이면 중복 회귀)"
    );
}

// ---- 에이전트 close 가 사용자 포커스를 옮기지 않는다 (workspace/tab/pane 3계층) ----
//
// 세 계층 모두 인덱스(`active_workspace` / `active_tab`) 또는 무조건 대입
// (`focused_pane`)을 쓰고 있어, 사용자가 **보고 있지 않은** 대상을 닫아도 시야가
// 밀렸다(불가침 원칙 1 위반). 단정은 인덱스가 아니라 **id** 로 한다 — 인덱스는
// 보존돼도 가리키는 대상이 바뀔 수 있기 때문이다.
//
// 규칙: 닫힌 것이 사용자가 보던 대상 **자체**일 때만 시야가 움직인다.

/// workspace 계층 — 앞쪽 워크스페이스가 통째로 닫혀도 보던 워크스페이스가 유지된다.
#[test]
fn closing_an_earlier_workspace_keeps_the_viewed_workspace() {
    let (mut state, mut engine) = test_state();
    let victim_sid = engine.workspaces[0].all_surface_ids()[0];
    for _ in 0..3 {
        add_test_workspace(&mut state, &mut engine);
    }
    state.switch_workspace(&mut engine, 2);
    let viewed_id = engine.workspaces[2].id;

    // 에이전트가 index 0 워크스페이스의 마지막 surface 를 닫는다 → workspace 째 cascade.
    assert!(state.close_surface_by_id_no_snapshot(&mut engine, victim_sid, false));

    assert_eq!(engine.workspaces.len(), 3);
    assert_eq!(
        engine.workspaces[state.active_workspace].id, viewed_id,
        "앞쪽 워크스페이스가 닫혀도 사용자가 보던 워크스페이스는 그대로여야 한다"
    );
}

/// workspace 계층 — 보던 워크스페이스 **자체**를 닫으면 이동은 정상이다(대상 소멸).
#[test]
fn closing_the_viewed_workspace_moves_to_a_neighbour() {
    let (mut state, mut engine) = test_state();
    for _ in 0..2 {
        add_test_workspace(&mut state, &mut engine);
    }
    state.switch_workspace(&mut engine, 1);
    let viewed_sid = engine.workspaces[1].all_surface_ids()[0];
    let survivors: Vec<u32> = [engine.workspaces[0].id, engine.workspaces[2].id].into();

    assert!(state.close_surface_by_id_no_snapshot(&mut engine, viewed_sid, false));

    assert_eq!(engine.workspaces.len(), 2);
    assert!(
        survivors.contains(&engine.workspaces[state.active_workspace].id),
        "닫힌 대상이 보던 워크스페이스였으면 생존 워크스페이스로 이동한다"
    );
}

/// tab 계층 — 앞쪽 탭이 닫혀도 보던 탭(=focused surface)이 유지된다.
#[test]
fn closing_an_earlier_tab_keeps_the_viewed_tab() {
    let (mut state, mut engine) = test_state();
    let sid0 = collect_surface_ids(&mut state, &mut engine)[0];
    state.add_tab(&mut engine).unwrap();
    state.add_tab(&mut engine).unwrap();
    let pane_id = state.active_workspace(&engine).focused_pane;
    // 사용자는 가운데 탭(index 1)을 본다.
    engine.workspaces[state.active_workspace]
        .pane_layout_mut()
        .find_pane_mut(pane_id)
        .unwrap()
        .active_tab = 1;
    let viewed_tab_id = {
        let pane = state
            .active_workspace(&engine)
            .pane_layout()
            .find_pane(pane_id)
            .unwrap();
        pane.tabs[1].id
    };

    assert!(state.close_surface_by_id_no_snapshot(&mut engine, sid0, false));

    let pane = state
        .active_workspace(&engine)
        .pane_layout()
        .find_pane(pane_id)
        .unwrap();
    assert_eq!(pane.tabs.len(), 2);
    assert_eq!(
        pane.tabs[pane.active_tab].id, viewed_tab_id,
        "앞쪽 탭이 닫혀도 사용자가 보던 탭은 그대로여야 한다"
    );
}

/// pane 계층 — 포커스와 무관한 pane 이 닫혀도 `focused_pane` 이 유지된다.
#[test]
fn closing_an_unfocused_pane_keeps_the_focused_pane() {
    let (mut state, mut engine) = test_state();
    let sid0 = collect_surface_ids(&mut state, &mut engine)[0];
    state
        .test_split_pane(&mut engine, SplitDirection::Vertical)
        .unwrap();
    state
        .test_split_pane(&mut engine, SplitDirection::Vertical)
        .unwrap();
    let pane_ids = state.active_workspace(&engine).pane_layout().all_pane_ids();
    assert_eq!(pane_ids.len(), 3);
    // 사용자는 마지막 pane 에 포커스를 두고 있다. sid0 은 첫 pane 소속.
    let focused_pane = *pane_ids.last().unwrap();
    engine.workspaces[state.active_workspace].focused_pane = focused_pane;

    assert!(state.close_surface_by_id_no_snapshot(&mut engine, sid0, false));

    assert_eq!(
        state.active_workspace(&engine).focused_pane,
        focused_pane,
        "포커스와 무관한 pane 이 닫혔는데 포커스가 움직이면 안 된다"
    );
}

/// pane 계층 — 포커스 pane 자체를 닫으면 생존 pane 으로 재배정된다(대상 소멸).
#[test]
fn closing_the_focused_pane_reassigns_focus() {
    let (mut state, mut engine) = test_state();
    let sid0 = collect_surface_ids(&mut state, &mut engine)[0];
    state
        .test_split_pane(&mut engine, SplitDirection::Vertical)
        .unwrap();
    let (_, sid0_pane) = engine.find_workspace_index_for_surface(sid0).unwrap();
    engine.workspaces[state.active_workspace].focused_pane = sid0_pane;

    assert!(state.close_surface_by_id_no_snapshot(&mut engine, sid0, false));

    let ws = state.active_workspace(&engine);
    assert_ne!(ws.focused_pane, sid0_pane);
    assert!(
        ws.pane_layout().find_pane(ws.focused_pane).is_some(),
        "포커스 pane 을 닫았으면 생존 pane 으로 재배정돼야 한다"
    );
}

// ---- 에이전트 close 와 사용자 close 가 갈리는 축 ----
//
// 포커스는 위 3계층 테스트가 고정한다. 여기서 고정하는 것은 `close_workspace_at`
// 이 `WorkspaceCloseOrigin` 에서 파생시키는 세 부수효과다 — 되돌리기 스택,
// plugin 에 실리는 close reason, 그리고 (아래 핸들러 테스트에서) 계측 경로값.
// 불가침 원칙 1: 에이전트 행동의 부수효과는 사용자 상태에 닿지 않는다.

#[test]
fn agent_close_does_not_record_the_workspace_for_undo() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine);

    assert!(state.close_workspace_at(&mut engine, 0, WorkspaceCloseOrigin::Agent));

    assert!(
        engine.closed_items.is_empty(),
        "에이전트가 닫은 것은 사용자의 되돌리기 스택에 들어가면 안 된다"
    );
}

#[test]
fn user_close_still_records_the_workspace_for_undo() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine);

    assert!(state.close_workspace_at(&mut engine, 0, WorkspaceCloseOrigin::User));

    assert_eq!(engine.closed_items.len(), 1);
}

/// 에이전트가 닫으면 plugin `surface.closed` 의 reason 도 에이전트여야 한다.
///
/// 되돌리기 스택과 **같은 축**인데 값이 따로 있어서, 예전에는 스냅샷만 갈리고
/// 이쪽은 사용자로 나갔다. `LifecycleReason::User`/`::Ipc` 매핑은
/// `app::dispatch::surface_lifecycle` 에서 이 플래그 하나로 결정된다.
#[test]
fn agent_close_reports_agent_origin_to_plugins() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine);

    assert!(state.close_workspace_at(&mut engine, 0, WorkspaceCloseOrigin::Agent));

    let events = state.take_pending_lifecycle_events();
    assert!(
        !events.is_empty(),
        "닫힌 surface 의 lifecycle 이벤트가 하나는 있어야 한다"
    );
    let flags: Vec<bool> = events.iter().map(|e| e.is_user_close).collect();
    assert!(
        flags.iter().all(|f| !*f),
        "에이전트가 닫았는데 plugin 에는 사용자 close 로 나간다: {flags:?}"
    );
}

#[test]
fn user_close_reports_user_origin_to_plugins() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine);

    assert!(state.close_workspace_at(&mut engine, 0, WorkspaceCloseOrigin::User));

    let events = state.take_pending_lifecycle_events();
    assert!(!events.is_empty());
    assert!(events.iter().all(|e| e.is_user_close));
}

/// `workspace.closed` host event 는 **origin 과 무관하게** 나간다.
///
/// 어느 명령을 썼느냐에 따라 plugin 이 받는 이벤트가 달라지면 안 된다. 제거 경로별
/// 발화는 아래 `inline_cascade_...` 가 따로 고정한다.
#[test]
fn closing_a_workspace_emits_the_host_event_for_both_origins() {
    for origin in [WorkspaceCloseOrigin::Agent, WorkspaceCloseOrigin::User] {
        let (mut state, mut engine) = test_state();
        add_test_workspace(&mut state, &mut engine);
        let workspace_id = engine.workspaces[0].id;

        assert!(state.close_workspace_at(&mut engine, 0, origin));

        let emitted = state.take_pending_host_events().iter().any(|e| {
            matches!(
                e,
                crate::state::PendingHostEvent::WorkspaceClosed { workspace_id: id }
                    if *id == workspace_id
            )
        });
        assert!(emitted, "{origin:?}: workspace.closed host event 가 없다");
    }
}

/// `save_snapshot` 과 `is_user_close` 는 **독립 축**이다 — 인라인 cascade 경로에서
/// 한 값으로 접으면 안 된다.
///
/// PTY 프로세스가 스스로 종료돼 도는 cleanup(`cascade_terminal_process_exited`)은
/// `save_snapshot=false, is_user_close=true` 로 부른다: 셸이 이미 끝나 되살릴 것이
/// 없으니 되돌리기 스택에는 안 넣지만, 그 종료를 일으킨 것은 에이전트가 아니라
/// 사람이므로 plugin 에는 사용자 close 로 나가야 한다. 워크스페이스 close 쪽
/// (`WorkspaceCloseOrigin`)처럼 하나로 접으면 이 조합에서 둘 중 하나가 반드시
/// 틀린 값이 된다.
#[test]
fn pty_exit_close_skips_the_snapshot_but_still_reports_a_user_close() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine);
    let ws_idx = state.active_workspace;
    let surface = engine.workspaces[ws_idx].all_surface_ids()[0];
    let closed_before = engine.closed_items.len();

    // PTY 종료 cleanup 과 같은 조합: 스냅샷 없음 + 사용자 close.
    assert!(state.close_surface_by_id_no_snapshot(&mut engine, surface, true));

    assert_eq!(
        engine.closed_items.len(),
        closed_before,
        "스스로 끝난 셸은 되돌리기 스택에 쌓이지 않는다(save_snapshot=false)"
    );
    let events = state.take_pending_lifecycle_events();
    assert!(!events.is_empty(), "close 는 lifecycle 이벤트를 낸다");
    assert!(
        events.iter().all(|e| e.is_user_close),
        "같은 호출이 plugin 에는 사용자 close 로 나가야 한다(is_user_close=true)"
    );
}

/// 워크스페이스의 **마지막 surface 가 스스로 닫혀** 워크스페이스까지 사라지는
/// 인라인 cascade(`AppState::close_case_workspace`)에서도 `workspace.closed` 가
/// 나간다.
///
/// 이 경로는 PTY 프로세스 종료 cleanup 과 egui close 가 쓴다. 한때 여기만 발화를
/// 빠뜨려서, 같은 소멸이라도 `workspace.close` 로 일으키면 plugin 이 이벤트를 받고
/// 터미널이 스스로 죽어 사라지면 못 받았다 — 무엇으로 사라졌느냐가 plugin 이 보는
/// 사실을 갈랐다. 발화를 초크포인트([`AppState::after_workspace_removed`])로 모아
/// 고쳤고, 이 테스트가 그 경로를 고정한다.
#[test]
fn inline_cascade_emits_the_workspace_closed_host_event() {
    let (mut state, mut engine) = test_state();
    add_test_workspace(&mut state, &mut engine);
    let ws_idx = state.active_workspace;
    let workspace_id = engine.workspaces[ws_idx].id;
    let surface_ids = engine.workspaces[ws_idx].all_surface_ids();
    assert_eq!(
        surface_ids.len(),
        1,
        "워크스페이스에 surface 가 하나여야 한다"
    );
    let before = engine.workspaces.len();

    // 마지막 surface 를 닫으면 워크스페이스까지 사라진다(Case 4/5).
    assert!(state.close_surface_by_id_no_snapshot(&mut engine, surface_ids[0], false));
    assert_eq!(
        engine.workspaces.len(),
        before - 1,
        "워크스페이스가 실제로 사라져야 이 경로를 지난 것이다"
    );

    let emitted = state.take_pending_host_events().iter().any(|e| {
        matches!(
            e,
            crate::state::PendingHostEvent::WorkspaceClosed { workspace_id: id }
                if *id == workspace_id
        )
    });
    assert!(
        emitted,
        "인라인 cascade 로 사라진 워크스페이스에 workspace.closed 가 없다"
    );
}

/// 원격 attach 가 **하드 점유**한 surface 를 GUI close 경로가 죽이지 않는다.
///
/// 하드 점유(ADR-0040)는 "지금 원격 사용자가 이 터미널을 쓰고 있다" 는 선언이다.
/// `workspace.close` IPC 는 이미 거절하는데(ADR-0120 ④) **사용자 경로는 열려 있었다** —
/// 같은 파괴가 에이전트에게는 막히고 사람에게는 무경고로 열린 비대칭이었다.
///
/// 이 모듈은 진입점마다 **거절과 통과를 짝으로** 고정한다. 거절만 세면 "전부 막았다" 와
/// 구별이 안 되기 때문이다 — 점유가 없을 때 같은 제스처가 여전히 닫는 것을 같은 자리에서
/// 확인한다. 픽스처는 전부 합성이라(`test_state`) 실제 문서·레이아웃을 고쳐도 안 흔들린다.
///
/// 로컬 사용자가 갇히지 않는 근거는 강제 해제 버튼이다
/// (`adapters/ui/egui_panels.rs` 의 `draw_occupied_overlays`) — 점유를 끊고 다시 닫으면 된다.
#[cfg(test)]
mod close_refuses_hard_occupied {
    use super::*;

    /// 점유 client id. 값 자체에 의미는 없고 `acquire` 가 holder 를 요구할 뿐이다.
    const HOLDER: u32 = 1;

    fn add_ws(engine: &mut crate::core::CoreState) -> usize {
        let event = crate::core::apply_create_workspace_inner(
            engine,
            crate::core::WorkspaceCreationParams::terminal(),
        )
        .expect("워크스페이스 생성");
        let crate::core::intent::CoreEvent::WorkspaceCreated { index, .. } = event else {
            panic!("expected WorkspaceCreated");
        };
        index
    }

    /// surface 가 아직 살아 있는가 — **렌즈 둘을 모두** 본다.
    ///
    /// 고침의 완료 판정을 고친 경로로 하면 안 된다. `close_*` 의 반환값은 그 경로가
    /// 스스로 하는 말이고, 레이아웃 트리는 그 경로가 방금 손댄 자료구조다. 그래서
    /// **터미널 레지스트리**(`engine.terminals` — cleanup 이 지우는 별도 저장소)를 함께
    /// 본다. 둘이 어긋나면 그것 자체가 결함이므로 여기서 갈라 알린다.
    fn alive(engine: &crate::core::CoreState, sid: u32) -> bool {
        let in_tree = engine
            .workspaces
            .iter()
            .any(|w| w.all_surface_ids().contains(&sid));
        let has_terminal = engine.terminals.get(sid).is_some();
        assert_eq!(
            in_tree, has_terminal,
            "surface {sid}: 레이아웃 트리({in_tree})와 터미널 레지스트리({has_terminal})가 \
             어긋난다 — 한쪽만 정리된 것이다"
        );
        in_tree
    }

    /// 두 번째 pane 을 만들고 포커스된 surface 를 돌려준다.
    fn split_pane(state: &mut AppState, engine: &mut crate::core::CoreState) -> u32 {
        state
            .test_split_pane(engine, SplitDirection::Vertical)
            .expect("pane split");
        state.focused_surface_id(engine).expect("포커스 surface")
    }

    #[test]
    fn closing_a_workspace_holding_an_occupied_surface_is_refused() {
        let (mut state, mut engine) = test_state();
        let idx = add_ws(&mut engine);
        let sid = engine.workspaces[idx].all_surface_ids()[0];
        engine.attach.acquire(sid, HOLDER).expect("하드 점유");

        let closed = state.close_workspace_at(&mut engine, idx, WorkspaceCloseOrigin::User);

        assert!(!closed, "점유된 워크스페이스는 닫히면 안 된다");
        assert!(alive(&engine, sid), "surface 가 살아 있어야 한다");
        assert_eq!(engine.workspaces.len(), 2, "거절이면 아무것도 안 사라진다");
        assert!(
            engine.attach.is_hard_occupied(sid),
            "거절 경로가 점유 상태를 건드리면 안 된다"
        );
    }

    /// 통과 대조 — 같은 제스처가 점유가 없으면 여전히 닫는다.
    #[test]
    fn closing_an_unoccupied_workspace_still_works() {
        let (mut state, mut engine) = test_state();
        let idx = add_ws(&mut engine);

        assert!(state.close_workspace_at(&mut engine, idx, WorkspaceCloseOrigin::User));
        assert_eq!(engine.workspaces.len(), 1);
    }

    #[test]
    fn closing_the_focused_surface_when_occupied_is_refused() {
        let (mut state, mut engine) = test_state();
        let sid_a = state.focused_surface_id(&engine).expect("포커스 surface");
        let pane_id = state.active_workspace(&engine).focused_pane;
        let (ws_idx, _) = engine
            .find_workspace_index_for_surface(sid_a)
            .expect("워크스페이스");
        let sid_b = engine.next_ids.next_surface();
        engine.workspaces[ws_idx]
            .pane_layout_mut()
            .find_pane_mut(pane_id)
            .expect("pane")
            .split_surface_by_id_marker(sid_a, SplitDirection::Horizontal, sid_b)
            .expect("surface split");
        engine
            .terminals
            .insert(sid_b, tasty_terminal::Terminal::new_detached(80, 24));
        engine.attach.acquire(sid_a, HOLDER).expect("하드 점유");

        assert!(!state.close_active_surface(&mut engine), "거절해야 한다");
        assert!(alive(&engine, sid_a));
    }

    /// 통과 대조.
    #[test]
    fn closing_an_unoccupied_focused_surface_still_works() {
        let (mut state, mut engine) = test_state();
        let sid_a = state.focused_surface_id(&engine).expect("포커스 surface");
        let pane_id = state.active_workspace(&engine).focused_pane;
        let (ws_idx, _) = engine
            .find_workspace_index_for_surface(sid_a)
            .expect("워크스페이스");
        let sid_b = engine.next_ids.next_surface();
        engine.workspaces[ws_idx]
            .pane_layout_mut()
            .find_pane_mut(pane_id)
            .expect("pane")
            .split_surface_by_id_marker(sid_a, SplitDirection::Horizontal, sid_b)
            .expect("surface split");
        engine
            .terminals
            .insert(sid_b, tasty_terminal::Terminal::new_detached(80, 24));

        assert!(state.close_active_surface(&mut engine));
        assert!(!alive(&engine, sid_a));
    }

    #[test]
    fn closing_a_pane_holding_an_occupied_surface_is_refused() {
        let (mut state, mut engine) = test_state();
        let sid = split_pane(&mut state, &mut engine);
        engine.attach.acquire(sid, HOLDER).expect("하드 점유");

        assert!(!state.close_active_pane(&mut engine), "거절해야 한다");
        assert!(alive(&engine, sid));
        assert_eq!(
            state
                .active_workspace(&engine)
                .pane_layout()
                .all_pane_ids()
                .len(),
            2,
            "거절이면 pane 도 그대로다"
        );
    }

    /// 통과 대조.
    #[test]
    fn closing_an_unoccupied_pane_still_works() {
        let (mut state, mut engine) = test_state();
        let sid = split_pane(&mut state, &mut engine);

        assert!(state.close_active_pane(&mut engine));
        assert!(!alive(&engine, sid));
    }

    #[test]
    fn closing_a_tab_holding_an_occupied_surface_is_refused() {
        let (mut state, mut engine) = test_state();
        state.add_tab(&mut engine).expect("탭 추가");
        let sid = state.focused_surface_id(&engine).expect("포커스 surface");
        engine.attach.acquire(sid, HOLDER).expect("하드 점유");

        assert!(!state.close_active_tab(&mut engine), "거절해야 한다");
        assert!(alive(&engine, sid));
    }

    /// 통과 대조.
    #[test]
    fn closing_an_unoccupied_tab_still_works() {
        let (mut state, mut engine) = test_state();
        state.add_tab(&mut engine).expect("탭 추가");
        let sid = state.focused_surface_id(&engine).expect("포커스 surface");

        assert!(state.close_active_tab(&mut engine));
        assert!(!alive(&engine, sid));
    }

    /// **사후 정리 경로는 막지 않는다.**
    ///
    /// 셸이 스스로 끝나서 도는 정리(`cascade_terminal_process_exited` → 이 함수)는 이미
    /// 죽은 프로세스를 치운다. 여기서 점유를 이유로 거절하면 락 때문에 **좀비 surface 가
    /// 영구히 남는다.** 그래서 검사는 공용 cascade 초크포인트가 아니라 요청 진입점에만
    /// 붙어 있고, 이 테스트가 그 경계를 고정한다 — 이 자리가 거절로 바뀌면 빨개진다.
    #[test]
    fn the_post_mortem_cleanup_path_still_closes_an_occupied_surface() {
        let (mut state, mut engine) = test_state();
        add_ws(&mut engine);
        let sid = state.focused_surface_id(&engine).expect("포커스 surface");
        engine.attach.acquire(sid, HOLDER).expect("하드 점유");

        assert!(
            state.close_surface_by_id_no_snapshot(&mut engine, sid, false),
            "사후 정리는 점유와 무관하게 통해야 한다"
        );
        assert!(!alive(&engine, sid));
    }
}

/// 파괴된 surface 의 점유 흔적은 남지 않는다 — 그러나 **형제의 점유는 살아남는다.**
///
/// `cleanup_surface` 가 점유를 안 지우면 레지스트리가 없는 surface 를 점유 중이라고
/// 계속 말한다(`attach.list` · `surface_held_by`). 반대로 너무 많이 지우면 닫지도 않은
/// 형제 surface 의 점유가 함께 풀려 holder 가 워크스페이스에서 쫓겨난다 — 로컬 강제
/// 끊기용 `release_occupancy` 를 그대로 쓰면 실제로 그렇게 된다. 두 방향을 짝으로 박는다.
#[cfg(test)]
mod cleanup_forgets_only_the_closed_surface {
    use super::*;

    const HOLDER: u32 = 7;

    #[test]
    fn the_closed_surfaces_own_lock_is_gone() {
        let (mut state, mut engine) = test_state();
        crate::core::apply_create_workspace_inner(
            &mut engine,
            crate::core::WorkspaceCreationParams::terminal(),
        )
        .expect("워크스페이스 생성");
        let sid = state.focused_surface_id(&engine).expect("포커스 surface");
        engine.attach.acquire(sid, HOLDER).expect("하드 점유");

        // 사후 정리 경로 — 요청 경로가 아니라 여기로만 점유 surface 가 도달한다.
        assert!(state.close_surface_by_id_no_snapshot(&mut engine, sid, false));

        assert!(
            !engine.attach.is_hard_occupied(sid),
            "사라진 surface 를 점유 중이라고 말하면 안 된다"
        );
        assert!(
            engine
                .attach
                .locks_snapshot()
                .iter()
                .all(|(s, _)| *s != sid),
            "attach.list 스냅샷에도 남으면 안 된다"
        );
    }

    /// 반대 방향 — 워크스페이스 점유의 멤버 하나가 닫혀도 나머지와 워크스페이스 락은
    /// 그대로다. 이 자리가 깨지면 holder 가 통째로 쫓겨난다.
    #[test]
    fn a_workspace_holders_other_surfaces_keep_their_occupancy() {
        let mut reg = crate::core::attach::OccupancyRegistry::new();
        let members = vec![10, 11, 12];
        reg.acquire_workspace(100, &members, &members, HOLDER)
            .expect("워크스페이스 점유");

        assert!(reg.forget_closed_surface(10), "닫힌 자리는 지워진다");

        assert!(!reg.is_hard_occupied(10), "닫힌 surface 는 풀린다");
        assert!(reg.is_hard_occupied(11), "형제는 그대로여야 한다");
        assert!(reg.is_hard_occupied(12), "형제는 그대로여야 한다");
        assert_eq!(
            reg.workspace_holder(100),
            Some(HOLDER),
            "워크스페이스 락 자체는 유지된다"
        );
    }

    /// 점유가 없던 surface 를 닫는 것은 아무것도 바꾸지 않는다.
    #[test]
    fn closing_an_unoccupied_surface_touches_no_lock() {
        let mut reg = crate::core::attach::OccupancyRegistry::new();
        reg.acquire(10, HOLDER).expect("점유");

        assert!(!reg.forget_closed_surface(99), "없던 자리는 지울 것이 없다");
        assert!(reg.is_hard_occupied(10), "무관한 점유는 그대로");
    }
}
