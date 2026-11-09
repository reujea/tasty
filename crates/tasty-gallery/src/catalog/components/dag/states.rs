//! 포커스·비포커스 바탕의 빈 DAG, 사라진 DAG, popup 의 DAG 없음·검색 결과 없음, 순환 경고 예제. 순환이 있어도 그래프는 계속 그린다.

use tasty_type_appearance::theme::Theme;

use super::{canvas, chrome};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 빈 상태 무대 한 칸의 최소 높이 — 시안 160.
fn empty_height(theme: &Theme) -> f32 {
    theme.dag_detail_log_max_height().value()
}

fn empty_box(ui: &mut egui::Ui, theme: &Theme, ground: egui::Color32, case: chrome::Empty<'_>) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), empty_height(theme)),
        egui::Sense::hover(),
    );
    let radius = theme.corner_radius.value();
    ui.painter().rect_filled(rect, radius, ground);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    chrome::paint_empty(ui, theme, rect, case);
}

/// `states` 섹션 Spec — 빈 상태 5 종 + 사이클 경고.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let cycle = super::cycle_dag();
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        let bed = |focused| tasty_ui_widgets::surface_bed(theme, focused);
        let popup = theme.bg_panel().to_egui();
        empty_box(ui, theme, bed(true), chrome::Empty::Surface);
        empty_box(ui, theme, bed(false), chrome::Empty::Surface);
        empty_box(
            ui,
            theme,
            bed(true),
            chrome::Empty::Missing("nightly-release"),
        );
        empty_box(ui, theme, popup, chrome::Empty::Popup);
        empty_box(ui, theme, popup, chrome::Empty::Search("deploy"));
        empty_box(ui, theme, popup, chrome::Empty::Filtered);
        let ids = cycle.cycle.clone().unwrap_or_default();
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), theme.dag_cycle_height().value()),
            egui::Sense::hover(),
        );
        chrome::paint_cycle_banner(ui, theme, rect, &ids);
    });
    canvas::cycle_stage(ui, theme);
    spec::meta(
        ui,
        theme,
        &[
            ("banner", "28px · pinned to canvas top"),
            ("banner copy", "names the cycle path"),
            (
                "empty A",
                "surface — how a DAG appears, on surface-bed-focused or surface-bed-unfocused",
            ),
            ("empty glyph", "glyph-dim, one ink on every ground"),
            (
                "empty A′",
                "surface — the DAG it showed is gone (host only)",
            ),
            ("empty C", "popup — no DAG in any workspace"),
            ("empty B", "popup search — echoes the query"),
            (
                "empty B′",
                "popup filtered — no query · status filter or workspace-only emptied the list · filter glyph",
            ),
            ("popup position", "centre of the list area"),
        ],
        &[
            TokenChip::new(
                "dag-cycle-bg",
                "banner wash",
                theme.dag_cycle_bg().to_egui(),
            ),
            TokenChip::new(
                "dag-cycle-fg",
                "banner text",
                theme.dag_cycle_fg().to_egui(),
            ),
            TokenChip::new("glyph-dim", "empty glyph", theme.glyph_dim().to_egui()),
        ],
    );
    spec::note(
        ui,
        theme,
        "The cycle warning stays above the graph. The gallery uses tasty-dag-layout to place \
         the nodes and keeps the cyclic graph visible.",
    );
}
