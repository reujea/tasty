//! pane 구분선과 surface 알림 테두리. 상태에서 그릴 영역을 모아 공용 view에 전달한다.

use egui::emath::GuiRounding as _;
use tasty_type_appearance::theme::Theme;

use crate::core::AttentionKind;
use crate::model::PhysicalRect;
use crate::state::MainViewState;
use crate::theme;

pub fn draw_pane_dividers(ctx: &egui::Context, dividers: &[PhysicalRect], scale_factor: f32) {
    let th = theme::theme();
    if dividers.is_empty() {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("pane_dividers"),
    ));
    let border_color = th.border_frame();
    for div in dividers {
        let rect = crate::adapters::ui::to_egui_rect(*div, scale_factor).round_ui();
        painter.rect_filled(rect, 0.0, border_color);
    }
}

/// 물리 좌표의 surface 영역과 알림 종류. kind가 없으면 테두리를 그리지 않는다.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceHighlightRegion {
    pub rect: PhysicalRect,
    pub kind: Option<AttentionKind>,
}

pub struct SurfaceHighlightsProps<'a> {
    pub theme: &'a Theme,
    pub regions: &'a [SurfaceHighlightRegion],
    pub scale_factor: f32,
}

/// 공용 attention 표시 판정에 넘기는 종류. 사이드바·탭 바·테두리가 같은 변환을 쓴다.
impl From<AttentionKind> for tasty_ui_widgets::Attention {
    fn from(kind: AttentionKind) -> Self {
        match kind {
            AttentionKind::NeedsInput => tasty_ui_widgets::Attention::NeedsInput,
            AttentionKind::Completion => tasty_ui_widgets::Attention::Completion,
        }
    }
}

/// 알림이 있는 영역에 Theme.focus_ring_width로 테두리를 그린다.
pub fn draw_surface_highlights_view(ctx: &egui::Context, props: &SurfaceHighlightsProps<'_>) {
    if props.regions.iter().all(|r| r.kind.is_none()) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("surface_highlights"),
    ));
    let scale_factor = props.scale_factor;
    for region in props.regions {
        let Some(kind) = region.kind else {
            continue;
        };
        let r = region.rect;
        let egui_rect = crate::adapters::ui::to_egui_rect(r, scale_factor).round_ui();
        painter.rect_stroke(
            egui_rect,
            0.0,
            tasty_ui_widgets::attention_edge_stroke(props.theme, kind.into()),
            egui::StrokeKind::Inside,
        );
    }
}

pub(crate) fn regions_from_state(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    terminal_rect: PhysicalRect,
    scale_factor: f32,
) -> Vec<SurfaceHighlightRegion> {
    let regions = state.surface_regions(engine, terminal_rect, scale_factor);
    let mut out = Vec::new();
    for (_pane_id, _pane_rect, surface_regions) in &regions {
        for r in surface_regions {
            let kind = crate::adapters::ui::surface_rings::attention_ring(engine, r.id);
            out.push(SurfaceHighlightRegion { rect: r.rect, kind });
        }
    }
    out
}

pub fn draw_surface_highlights(
    ctx: &egui::Context,
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    terminal_rect: PhysicalRect,
    scale_factor: f32,
) {
    let th = theme::theme();
    let regions = regions_from_state(state, engine, terminal_rect, scale_factor);
    let props = SurfaceHighlightsProps {
        theme: &th,
        regions: &regions,
        scale_factor,
    };
    draw_surface_highlights_view(ctx, &props);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PhysicalPx;

    fn mk_rect(x: f32, y: f32, w: f32, h: f32) -> PhysicalRect {
        PhysicalRect {
            x: PhysicalPx(x),
            y: PhysicalPx(y),
            width: PhysicalPx(w),
            height: PhysicalPx(h),
        }
    }

    fn test_theme() -> Theme {
        tasty_themes::mocha_fallback()
    }

    fn run_view(regions: &[SurfaceHighlightRegion], scale_factor: f32) {
        let ctx = egui::Context::default();
        let theme = test_theme();
        // FullOutput 불필요 — panic-free 만 검증.
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            let props = SurfaceHighlightsProps {
                theme: &theme,
                regions,
                scale_factor,
            };
            draw_surface_highlights_view(ctx, &props);
        }));
    }

    #[test]
    fn view_empty_regions_no_panic() {
        run_view(&[], 1.0);
    }

    #[test]
    fn view_all_unhighlighted_no_panic() {
        let regions = vec![
            SurfaceHighlightRegion {
                rect: mk_rect(0.0, 0.0, 100.0, 50.0),
                kind: None,
            },
            SurfaceHighlightRegion {
                rect: mk_rect(100.0, 0.0, 100.0, 50.0),
                kind: None,
            },
        ];
        run_view(&regions, 1.0);
    }

    #[test]
    fn view_mixed_highlights_no_panic() {
        let regions = vec![
            SurfaceHighlightRegion {
                rect: mk_rect(0.0, 0.0, 100.0, 50.0),
                kind: Some(AttentionKind::Completion),
            },
            SurfaceHighlightRegion {
                rect: mk_rect(100.0, 0.0, 100.0, 50.0),
                kind: None,
            },
            SurfaceHighlightRegion {
                rect: mk_rect(200.0, 0.0, 100.0, 50.0),
                kind: Some(AttentionKind::NeedsInput),
            },
        ];
        run_view(&regions, 1.0);
    }

    #[test]
    fn view_handles_non_unit_scale_factor() {
        let regions = vec![SurfaceHighlightRegion {
            rect: mk_rect(0.0, 0.0, 200.0, 100.0),
            kind: Some(AttentionKind::Completion),
        }];
        run_view(&regions, 2.0);
    }

    #[test]
    fn view_handles_zero_size_rect() {
        let regions = vec![SurfaceHighlightRegion {
            rect: mk_rect(0.0, 0.0, 0.0, 0.0),
            kind: Some(AttentionKind::NeedsInput),
        }];
        run_view(&regions, 1.0);
    }
}
