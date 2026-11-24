//! 띠 leaf 한 장면에서 띠 행 경계를 읽는 자리들이 같은 물리 경계를 쓰는지 본다. mesh 프레임, attention 링,
//! 점유 링, chrome이 모두 `collect`가 정한 행 아래에서 시작하고, 바탕 kind는 leaf 전체를 쓴다.

use crate::model::{PhysicalPx, PhysicalRect};
use crate::runtime::engine_session::EngineSession;
use crate::state::MainViewState;
use crate::state::mesh_frames::MeshSide;

const SID: u32 = 701_001;
const SCALE: f32 = 1.25;

/// 위 끝이 물리 px 경계에 맞지 않는 화면. 띠 행 경계가 반올림으로 갈리는 자리다.
fn screen() -> PhysicalRect {
    PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(30.75),
        width: PhysicalPx(800.0),
        height: PhysicalPx(600.0),
    }
}

/// workspace 하나·탭 하나에 `surface`만 둔 장면. `official`이면 그 kind를 공식 plugin으로 등록한다.
fn scene(
    kind: &str,
    surface: Box<dyn crate::model::Surface>,
    official: Option<&str>,
) -> (MainViewState, EngineSession) {
    use tasty_core::DomainEvent as E;
    let model = crate::state::tests::test_model(vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: SID,
            name: "Edge".into(),
            category: 0,
            index: 0,
            pane: SID,
        },
        E::TabCreated {
            id: SID,
            pane: SID,
            index: 0,
            name: kind.into(),
            surface: tasty_core::SurfaceSpec {
                id: SID,
                kind: kind.into(),
                data: None,
            },
        },
    ]);
    let (mut state, mut session) = crate::state::tests::test_state_from_model(model);
    session.runtime.surfaces.insert(SID, surface);
    if let Some(plugin) = official {
        let decl: tasty_plugin_manifest::SurfaceKindDecl =
            serde_json::from_value(serde_json::json!({
                "kind": kind,
                "display_name_i18n_key": "surface.kind.image",
                "rendering": "egui-mesh",
            }))
            .expect("kind decl");
        assert!(
            crate::runtime::surface_registry::egui_mesh::register_egui_mesh_kind(
                &session.borrow_mut().runtime.surface_registry,
                plugin,
                &decl,
                tasty_plugin_manifest::HOST_API_VERSION,
            )
        );
    }
    state.reconcile_presentation(&session.core_state);
    (state, session)
}

fn local_mesh(kind: &'static str, plugin: &str) -> Box<dyn crate::model::Surface> {
    Box::new(crate::runtime::egui_mesh_surface::EguiMeshSurface::new(
        SID,
        kind,
        plugin.into(),
        "Mesh".into(),
        None,
    ))
}

fn mirror_mesh(bed_focus: bool) -> Box<dyn crate::model::Surface> {
    Box::new(crate::model::AttachMeshSurface::new(
        SID,
        "mesh-demo",
        "com.example.demo".into(),
        "Mesh".into(),
        bed_focus,
    ))
}

fn pane_rects(
    state: &MainViewState,
    read: &crate::runtime::engine_read::EngineRead<'_>,
) -> Vec<(u32, PhysicalRect)> {
    let ws = read
        .workspace_at(state.active_workspace_index(read))
        .expect("workspace");
    state.pane_rects(read, ws, screen(), SCALE)
}

/// 장면의 leaf 하나(물리 px).
fn leaf(state: &MainViewState, read: &crate::runtime::engine_read::EngineRead<'_>) -> PhysicalRect {
    let regions = state.surface_regions(read, screen(), SCALE);
    let (_, _, leaves) = regions.first().expect("pane");
    leaves.iter().find(|r| r.id == SID).expect("leaf").rect
}

/// kind 이름, surface, 공식 등록 plugin, 그리는 쪽, 띠 행 예약 여부.
type Case = (
    &'static str,
    Box<dyn crate::model::Surface>,
    Option<&'static str>,
    MeshSide,
    bool,
);

/// mesh 프레임은 띠 kind(선언 없는 로컬 mesh, `bed_focus` 없는 mirror mesh)에서만 띠 행 아래에서 시작한다.
/// 바탕 kind(공식 image, `bed_focus`를 실은 mirror mesh)는 leaf 전체다.
#[test]
fn a_mesh_frame_starts_below_the_edge_only_for_an_edge_kind() {
    let theme = crate::theme::theme();
    let cases: [Case; 4] = [
        (
            "mesh-demo",
            local_mesh("mesh-demo", "com.example.demo"),
            None,
            MeshSide::Local,
            true,
        ),
        (
            "image",
            local_mesh("image", "com.tasty.image"),
            Some("com.tasty.image"),
            MeshSide::Local,
            false,
        ),
        (
            "mesh-demo",
            mirror_mesh(false),
            None,
            MeshSide::Attach,
            true,
        ),
        (
            "mesh-demo",
            mirror_mesh(true),
            None,
            MeshSide::Attach,
            false,
        ),
    ];
    for (kind, surface, official, side, edge) in cases {
        let (state, mut session) = scene(kind, surface, official);
        let engine = session.borrow_mut();
        let read = engine.read();
        let leaf = leaf(&state, &read);
        let frames = state.mesh_frame_regions(&read, screen(), SCALE);
        assert_eq!(frames.len(), 1, "{kind} {side:?}");
        assert_eq!(frames[0].side, side);
        let want = if edge {
            crate::state::surface_focus::below_edge(&theme, leaf, SCALE)
        } else {
            leaf
        };
        assert_eq!(
            (frames[0].rect.y.value(), frames[0].rect.height.value()),
            (want.y.value(), want.height.value()),
            "{kind} {side:?} edge={edge}"
        );
        assert_eq!(
            crate::state::surface_focus::reserves_edge(&read, SID),
            edge,
            "{kind} {side:?}"
        );
        if edge {
            assert!(frames[0].rect.y.value() > leaf.y.value(), "{kind} {side:?}");
        }
    }
}

/// 띠 leaf의 attention 링과 chrome은 `collect`가 정한 행 아래 끝에서 시작한다. 행 경계는 물리 정수 px다.
#[test]
fn the_attention_ring_and_the_chrome_start_on_the_row_boundary() {
    let (mut state, mut session) = scene(
        "mesh-demo",
        local_mesh("mesh-demo", "com.example.demo"),
        None,
    );
    let mut engine = session.borrow_mut();
    engine.raise_attention(SID, crate::core::AttentionKind::NeedsInput);
    let read = engine.read();
    let panes = pane_rects(&state, &read);
    super::collect(&mut state, &read, &panes, SCALE);
    let edge = *state.surface_focus_edges.first().expect("edge leaf");
    let leaf = leaf(&state, &read);
    let below = crate::state::surface_focus::below_edge(&crate::theme::theme(), leaf, SCALE);
    assert_eq!(edge.rows.bottom, below.y.value());
    assert_eq!(edge.rows.bottom.fract(), 0.0, "a whole physical pixel");
    assert_eq!(edge.below, crate::adapters::ui::to_egui_rect(below, SCALE));
    assert_eq!(super::chrome_top(&state, SID), Some(edge.below.top()));

    let rings = crate::adapters::ui::divider::regions_from_state(&state, &read, screen(), SCALE);
    let ring = rings
        .iter()
        .find(|r| r.kind.is_some())
        .expect("attention ring");
    assert_eq!(ring.clip, Some(edge.below));
}

/// 띠 leaf의 점유 링은 행 아래로 잘라 칠한다. 행 안의 링은 띠가 다시 칠한다.
#[test]
fn the_occupancy_ring_is_clipped_below_the_row() {
    let (mut state, mut session) = scene(
        "mesh-demo",
        local_mesh("mesh-demo", "com.example.demo"),
        None,
    );
    let engine = session.borrow_mut();
    engine.live.occupancy.acquire(SID, 41).expect("occupy");
    let read = engine.read();
    let panes = pane_rects(&state, &read);
    super::collect(&mut state, &read, &panes, SCALE);
    let below = state.surface_focus_edges.first().expect("edge leaf").below;
    let stroke = tasty_ui_widgets::occupancy_edge_stroke(&crate::theme::theme(), true);
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(SCALE);
    let active = state.active_workspace_index(&read);
    let tab_bar = state.tab_bar_height;
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        crate::adapters::ui::egui_panels::draw_occupied_overlays(
            ctx, &mut state, active, tab_bar, &read, &panes, SCALE,
        );
    });
    let clips: Vec<egui::Rect> = output
        .shapes
        .iter()
        .filter(|c| matches!(&c.shape, egui::Shape::Rect(r) if r.stroke == stroke))
        .map(|c| c.clip_rect)
        .collect();
    assert_eq!(clips, vec![below]);
}
