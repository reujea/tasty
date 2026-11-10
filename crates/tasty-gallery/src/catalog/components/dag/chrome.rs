//! DAG 캔버스의 줌 버튼, 미니맵, 순환 경고, 표시 상세도 안내.

use tasty_dag_layout::Orientation;
use tasty_icons as icons;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    GraphMinimap, GraphView, draw_zoom_cluster, graph_zoom_cluster_size, paint_graph_minimap,
};

use super::Plot;
use super::canvas::{SamplePainter, chrome_labels, node_size};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 사이클 경고 — 캔버스 상단 전폭에 고정. 그래프는 뒤에서 계속 그려진다.
pub fn paint_cycle_banner(ui: &mut egui::Ui, theme: &Theme, canvas: egui::Rect, ids: &[String]) {
    let h = theme.dag_cycle_height().value();
    let rect = egui::Rect::from_min_size(canvas.min, egui::vec2(canvas.width(), h));
    ui.painter()
        .rect_filled(rect, 0.0, theme.dag_cycle_bg().to_egui());
    ui.painter().hline(
        rect.x_range(),
        rect.max.y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.dag_cycle_border().to_egui(),
        ),
    );

    let pad = theme.spacing_md.value();
    let gap = theme.spacing_sm.value();
    let icon_side = theme.dag_chrome_height().value() / 2.0;
    let fg = theme.dag_cycle_fg().to_egui();
    icons::ALERT_TRIANGLE.image(icon_side, fg).paint_at(
        ui,
        egui::Rect::from_center_size(
            egui::pos2(rect.min.x + pad + icon_side / 2.0, rect.center().y),
            egui::vec2(icon_side, icon_side),
        ),
    );
    let font = egui::FontId::proportional(theme.font_size_caption.value());
    let lead = format!(
        "Cycle detected — the runner will not advance {} tasks: ",
        ids.len()
    );
    let x = rect.min.x + pad + icon_side + gap;
    let lead_w = super::node::text_width(ui, &lead, &font);
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &lead,
        font,
        fg,
    );
    // 첫 ID를 마지막에 반복해 순환 경로를 표시한다.
    let path = format!("{} \u{2192} {}", ids.join(" \u{2192} "), ids[0]);
    ui.painter().text(
        egui::pos2(x + lead_w, rect.center().y),
        egui::Align2::LEFT_CENTER,
        path,
        egui::FontId::monospace(theme.font_size_caption.value()),
        theme.text_secondary().to_egui(),
    );
}

/// 빈 상태 예제의 경우.
#[derive(Clone, Copy)]
pub enum Empty<'a> {
    /// 본체 surface — 워크스페이스에 DAG가 없다. 본체 문구(`dag.empty.none*`)를 쓴다.
    Surface,
    /// 본체 surface — 지정한 DAG가 사라졌다. 본체 문구(`dag.empty.missing*`)를 쓴다.
    Missing(&'a str),
    /// 본체 DAG 목록 popup — 어느 워크스페이스에도 DAG가 없다. 디자인 `DagEmpty variant="popup"`.
    Popup,
    /// 본체 DAG 목록 popup — 검색어에 맞는 DAG가 없다. 디자인 `DagEmpty variant="search"`.
    Search(&'a str),
    /// 본체 DAG 목록 popup — 검색어 없이 상태 필터·범위가 목록을 비웠다. 디자인 `DagEmpty variant="filtered"`.
    Filtered,
}

/// 빈 상태를 본체와 같은 `paint_dag_empty`로 그린다.
pub fn paint_empty(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, case: Empty<'_>) {
    let (icon, title, hint) = match case {
        Empty::Surface => (
            icons::GIT_TREE,
            crate::i18n::t("dag.empty.none").to_owned(),
            crate::i18n::t_fmt("dag.empty.none_hint", "3"),
        ),
        Empty::Missing(id) => (
            icons::SEARCH,
            crate::i18n::t_fmt("dag.empty.missing", id),
            crate::i18n::t("dag.empty.missing_hint").to_owned(),
        ),
        Empty::Popup => (
            icons::GIT_TREE,
            crate::i18n::t("dag_list.empty_none").to_owned(),
            crate::i18n::t("dag_list.empty_none_hint").to_owned(),
        ),
        Empty::Search(q) => (
            icons::SEARCH,
            crate::i18n::t_fmt("dag.popup.no_match", q),
            crate::i18n::t("dag.popup.no_match_hint").to_owned(),
        ),
        Empty::Filtered => (
            icons::FUNNEL,
            crate::i18n::t("dag.popup.no_match_filtered").to_owned(),
            crate::i18n::t("dag.popup.no_match_hint").to_owned(),
        ),
    };
    let view = tasty_ui_widgets::DagEmptyView {
        icon,
        title: &title,
        hint: &hint,
    };
    tasty_ui_widgets::paint_dag_empty(ui, theme, rect, &view);
}

/// `chrome` 섹션 Spec — 줌 클러스터 + 미니맵.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let graph = super::build_dag();
    let plot = Plot::new(&graph);
    let mut view = GraphView::default();
    view.zoom = 0.8;
    view.offset = egui::vec2(theme.spacing_xl.value(), theme.spacing_lg.value());
    let layout = view.layout(
        Some(&plot),
        &super::layout_config(theme, Orientation::TopDown),
    );
    let labels = chrome_labels();
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
            let size = egui::vec2(
                theme.dag_minimap_width().value(),
                theme.dag_minimap_height().value(),
            );
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let map = GraphMinimap {
                graph: &plot,
                painter_impl: &SamplePainter,
                layout: &layout,
                node_size: node_size(theme),
            };
            paint_graph_minimap(ui.painter(), theme, rect, size * 3.0, &view, &map);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
                for compact in [false, true] {
                    let s = graph_zoom_cluster_size(theme, compact);
                    let (r, _) = ui.allocate_exact_size(s, egui::Sense::hover());
                    // 예제의 버튼은 눌러도 바꿀 상태가 없다. 동작은 본체 캔버스에서 확인한다.
                    if let Some(action) = ui
                        .push_id(compact, |ui| {
                            draw_zoom_cluster(ui, theme, r, &view, Orientation::TopDown, &labels)
                        })
                        .inner
                    {
                        tracing::trace!("gallery zoom cluster ignores {action:?}");
                    }
                }
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("cluster", "28px row · 1px border"),
            ("inset", "8px from canvas edge"),
            ("minimap", "160 × 112"),
            ("viewport", "1px accent rect"),
            ("minimap cutoff", "surface < 560px"),
            ("compact cutoff", "surface < 400px"),
        ],
        &[
            TokenChip::new(
                "dag-chrome-bg",
                "cluster fill",
                theme.dag_chrome_bg().to_egui(),
            ),
            TokenChip::new(
                "dag-minimap-viewport",
                "viewport rect",
                theme.dag_minimap_viewport().to_egui(),
            ),
            TokenChip::without_color("dag-minimap-min-surface", "560 cutoff"),
            TokenChip::new(
                "dag-minimap-bg",
                "minimap bed",
                theme.dag_minimap_bg().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "The minimap paints every node in one colour and fades dimmed nodes, \
         so it shows the shape of the graph and where the viewport sits, not node status.",
    );
}
