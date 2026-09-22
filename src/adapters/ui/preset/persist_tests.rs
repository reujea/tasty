//! 편집 화면의 저장과 에이전트 `preset.save` 의 경합 — 캐시가 지어진 뒤 저장소가 바뀌었으면
//! 캐시로 덮지 않는다(원칙 1). 설정 화면 확인과 구조 편집 자동 저장이 둘 다
//! [`persist_layout`] 을 지나므로 여기서 한 번에 고정한다.

use super::*;
use tasty_presets::{PresetPane, PresetSurface, PresetTab, WorkspacePreset};

fn terminal_tab() -> PresetTab {
    PresetTab {
        explicit_name: None,
        layout: PresetSurfaceLayout::Leaf {
            surface: PresetSurface {
                id: None,
                kind: "terminal".into(),
                cwd: None,
                startup_command: None,
                params: serde_json::Value::Null,
            },
        },
    }
}

/// 탭 `tabs` 개짜리 pane 하나로 된 workspace preset.
fn ws(name: &str, tabs: usize) -> WorkspacePreset {
    WorkspacePreset {
        name: name.into(),
        subtitle: String::new(),
        description: String::new(),
        layout: PresetPaneNode::Leaf {
            pane: PresetPane {
                tabs: (0..tabs).map(|_| terminal_tab()).collect(),
                active_tab: 0,
            },
        },
    }
}

fn tab_count(store: &PresetStore, name: &str) -> usize {
    count_ws_tabs(&store.get_workspace(name).expect("preset").layout)
}

/// 사용자가 편집 화면에서 연 preset("dev", 탭 1 개)의 캐시.
fn opened(store: &mut PresetStore) -> DemoCache {
    store.save_workspace(ws("dev", 1)).expect("seed");
    build_cache(store, PresetKind::Workspace, "dev", &KindCatalog::default()).expect("cache")
}

/// 사용자가 확인할 변경 — 캐시와 다른 레이아웃(탭 3 개).
fn users_change() -> DemoLayout {
    DemoLayout::from_workspace(&ws("dev", 3), &KindCatalog::default())
}

/// 에이전트가 같은 이름으로 저장한 뒤 사용자가 확인하면 에이전트의 쓰기가 남는다.
#[test]
fn an_agent_save_behind_the_cache_is_not_overwritten() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = PresetStore::load_from(tmp.path().into());
    let cache = opened(&mut store);

    // 에이전트의 `preset.save`(overwrite) — 탭 2 개.
    store
        .save_workspace_overwrite(ws("dev", 2))
        .expect("agent save");

    let out = persist_layout(
        &mut store,
        PresetKind::Workspace,
        "dev",
        &users_change(),
        &cache.base,
    )
    .expect("persist");
    assert_eq!(out, Persisted::Conflict);
    assert_eq!(tab_count(&store, "dev"), 2, "에이전트의 쓰기가 남아야 한다");
    // 디스크도 — 새로 읽은 저장소에서 같은 값이어야 한다.
    let reread = PresetStore::load_from(tmp.path().into());
    assert_eq!(tab_count(&reread, "dev"), 2);
}

/// 경합이 없으면 전처럼 쓰고, 새 기준 판은 저장 뒤 저장소의 값이다 — 이어지는 두 번째
/// 저장이 자기 첫 저장을 경합으로 보지 않게.
#[test]
fn without_contention_the_save_goes_through_and_rebases() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = PresetStore::load_from(tmp.path().into());
    let cache = opened(&mut store);

    let out = persist_layout(
        &mut store,
        PresetKind::Workspace,
        "dev",
        &users_change(),
        &cache.base,
    )
    .expect("persist");
    assert_eq!(tab_count(&store, "dev"), 3);
    let Persisted::Saved(base) = out else {
        panic!("경합이 없는데 거절했다: {out:?}");
    };
    assert_eq!(
        base,
        LayoutBase::current(&store, PresetKind::Workspace, "dev")
    );

    let again = persist_layout(
        &mut store,
        PresetKind::Workspace,
        "dev",
        &DemoLayout::from_workspace(&ws("dev", 4), &KindCatalog::default()),
        &base,
    )
    .expect("persist again");
    assert!(matches!(again, Persisted::Saved(_)), "{again:?}");
    assert_eq!(tab_count(&store, "dev"), 4);
}

/// 에이전트가 메타(subtitle)만 바꿨으면 저장은 레이아웃만 갈아 쓰므로 덮을 것이 없다 —
/// 경합으로 막지 않고, 그 subtitle 도 남는다.
#[test]
fn a_metadata_only_agent_write_is_not_a_conflict() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = PresetStore::load_from(tmp.path().into());
    let cache = opened(&mut store);

    let mut meta = ws("dev", 1);
    meta.subtitle = "from agent".into();
    store.save_workspace_overwrite(meta).expect("agent save");

    let out = persist_layout(
        &mut store,
        PresetKind::Workspace,
        "dev",
        &users_change(),
        &cache.base,
    )
    .expect("persist");
    assert!(matches!(out, Persisted::Saved(_)), "{out:?}");
    assert_eq!(tab_count(&store, "dev"), 3);
    assert_eq!(
        store.get_workspace("dev").expect("p").subtitle,
        "from agent"
    );
}

/// 에이전트가 지웠으면 전처럼 아무것도 쓰지 않는다(되살리지 않는다).
#[test]
fn a_deleted_preset_is_not_recreated() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = PresetStore::load_from(tmp.path().into());
    let cache = opened(&mut store);
    store
        .delete(PresetKind::Workspace, "dev")
        .expect("agent delete");

    let out = persist_layout(
        &mut store,
        PresetKind::Workspace,
        "dev",
        &users_change(),
        &cache.base,
    )
    .expect("persist");
    assert_eq!(out, Persisted::Saved(None));
    assert!(store.get_workspace("dev").is_none());
}

/// 경합 뒤 처리: 캐시가 저장소 판으로 다시 지어지고, 선택이 풀리고, toast 가 하나 뜬다.
#[test]
fn a_conflict_reloads_the_cache_and_tells_the_user() {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = PresetStore::load_from(tmp.path().into());
    let mut cache = opened(&mut store);
    store
        .save_workspace_overwrite(ws("dev", 2))
        .expect("agent save");

    let mut selected = Some(0);
    let mut toasts = ToastManager::new();
    reload_after_conflict(
        &store,
        PresetKind::Workspace,
        "dev",
        &KindCatalog::default(),
        &mut cache,
        &mut selected,
        &mut toasts,
    );
    assert_eq!(
        cache.base,
        LayoutBase::current(&store, PresetKind::Workspace, "dev")
    );
    assert_eq!(
        cache.layout.rebuild_pane_node().map(|n| count_ws_tabs(&n)),
        Some(2),
        "미리보기가 에이전트의 판을 보여야 한다"
    );
    assert_eq!(selected, None);
    assert_eq!(toasts.len(), 1);
}
