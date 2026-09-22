//! 보기 모드 미리보기는 저장소를 따라가고, 편집 모드의 캐시는 따라가지 않는다
//! (`docs/adr/0564-the-preset-view-mode-follows-the-store-and-the-edit-mode-does-not.md`).
//! 두 성질을 [`draw_preview`] 를 헤드리스 egui 프레임으로 돌려 캐시 칸에서 잰다 —
//! 새로고침을 부르는 자리가 어느 모드 갈래에 있는지가 곧 시험 대상이다.

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

/// `draw_preview` 를 한 프레임 그린다.
fn frame(
    ctx: &egui::Context,
    store: &mut PresetStore,
    editing: bool,
    selected_node: &mut Option<usize>,
) {
    let theme = tasty_themes::mocha_fallback();
    let catalog = KindCatalog::default();
    let kb = KeybindingSettings::default();
    let mut toasts = ToastManager::new();
    let mut surface_cfg = None;
    drop(ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            draw_preview(
                ui,
                store,
                &theme,
                PresetKind::Workspace,
                "dev",
                rect,
                editing,
                selected_node,
                &mut surface_cfg,
                &mut toasts,
                &catalog,
                &kb,
            );
        });
    }));
}

/// 캐시 칸에 든 미리보기의 탭 수.
fn cached_tabs(ctx: &egui::Context) -> usize {
    let cache: DemoCache = ctx
        .data(|d| d.get_temp(demo_cache_id()))
        .expect("미리보기 캐시");
    count_ws_tabs(&cache.layout.rebuild_pane_node().expect("workspace layout"))
}

fn seeded() -> (tempfile::TempDir, PresetStore) {
    let tmp = tempfile::tempdir().expect("tmp");
    let mut store = PresetStore::load_from(tmp.path().into());
    store.save_workspace(ws("dev", 1)).expect("seed");
    (tmp, store)
}

/// 보기 모드: 에이전트가 같은 이름으로 저장하면 다음 프레임의 미리보기가 새 판을 보인다.
#[test]
fn view_mode_shows_an_agent_save_on_the_next_frame() {
    let (_tmp, mut store) = seeded();
    let ctx = egui::Context::default();
    let mut selected = None;
    frame(&ctx, &mut store, false, &mut selected);
    assert_eq!(cached_tabs(&ctx), 1);

    store
        .save_workspace_overwrite(ws("dev", 2))
        .expect("agent save");
    frame(&ctx, &mut store, false, &mut selected);
    assert_eq!(
        cached_tabs(&ctx),
        2,
        "보기 모드는 저장소의 새 판을 보여야 한다"
    );

    // 새 판이 기준 판이 됐으므로 이어서 편집 모드로 들어간 첫 저장은 경합이 아니다.
    let cache: DemoCache = ctx.data(|d| d.get_temp(demo_cache_id())).expect("cache");
    assert_eq!(
        cache.base,
        LayoutBase::current(&store, PresetKind::Workspace, "dev")
    );
}

/// 편집 모드: 에이전트가 저장해도 캐시(사용자가 고치고 있는 트리)와 선택은 그대로다 —
/// 대조는 ADR-0531 대로 저장 직전에만 한다.
#[test]
fn edit_mode_keeps_its_cache_through_an_agent_save() {
    let (_tmp, mut store) = seeded();
    let ctx = egui::Context::default();
    let mut selected = Some(0);
    frame(&ctx, &mut store, true, &mut selected);
    assert_eq!(cached_tabs(&ctx), 1);

    store
        .save_workspace_overwrite(ws("dev", 2))
        .expect("agent save");
    frame(&ctx, &mut store, true, &mut selected);
    assert_eq!(
        cached_tabs(&ctx),
        1,
        "편집 모드의 캐시를 갈아 끼우면 안 된다"
    );
    assert_eq!(selected, Some(0), "편집 모드의 선택을 풀면 안 된다");
}

/// 보기 모드라도 저장소의 레이아웃이 그대로면 캐시를 다시 짓지 않는다 — 미리보기에서
/// 사용자가 바꾼 상태(누른 탭 등, 저장되지 않는 것)가 매 프레임 지워지지 않게.
#[test]
fn view_mode_keeps_its_cache_while_the_store_is_unchanged() {
    let (_tmp, mut store) = seeded();
    let ctx = egui::Context::default();
    let mut selected = None;
    frame(&ctx, &mut store, false, &mut selected);

    // 캐시만 다른 트리로 바꾼다(기준 판은 그대로) — 저장소와 무관한 미리보기 쪽 상태의 대역.
    let mut cache: DemoCache = ctx.data(|d| d.get_temp(demo_cache_id())).expect("cache");
    cache.layout = DemoLayout::from_workspace(&ws("dev", 3), &KindCatalog::default());
    ctx.data_mut(|d| d.insert_temp(demo_cache_id(), cache));

    frame(&ctx, &mut store, false, &mut selected);
    assert_eq!(
        cached_tabs(&ctx),
        3,
        "저장소가 그대로인데 캐시를 다시 지었다"
    );
}
