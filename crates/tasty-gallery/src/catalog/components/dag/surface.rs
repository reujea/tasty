//! 탭 전체를 쓰는 DAG 예제. 좁은 화면에서는 헤더를 여러 줄로 나누고
//! 러너 힌트·미니맵을 숨기며 상세를 아래쪽으로 옮긴다.

use tasty_dag_layout::Orientation;
use tasty_icons as icons;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{IconButton, select};

use super::detail::Dock;
use super::{Graph, canvas, detail, runner};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 무대에서 좁은 변형에 주는 폭 — 디자인 "narrow floor 320px".
pub(super) const NARROW_STAGE_WIDTH: LogicalPx = LogicalPx(320.0);
/// 시안 `dag-tab-surface` 프레임 높이(320).
const FOCUS_VIEW_H: LogicalPx = LogicalPx(320.0);
/// 무대 높이 — 시안 520.
const STAGE_HEIGHT: LogicalPx = LogicalPx(520.0);

/// 헤더가 몇 줄로 접히는지. 시안은 `flex-wrap: wrap` 이라 320px 에서는 컨트롤이
/// 한 번 더 접혀 3 줄이 된다 — 그 자동 줄바꿈을 명시적으로 계산한다.
fn header_rows(
    ui: &egui::Ui,
    theme: &Theme,
    runner_state: &super::Runner,
    width: f32,
    narrow: bool,
) -> u32 {
    if !narrow {
        return 1;
    }
    let inner = width - theme.spacing_md.value() * 2.0;
    let need = min_select_width(theme)
        + runner::badge_width(ui, theme, runner_state)
        + theme.dag_chrome_height().value()
        + theme.spacing_sm.value() * 2.0;
    if need > inner { 3 } else { 2 }
}

/// Select 가 이보다 좁아지면 값이 안 읽혀 컨트롤 구실을 못 한다.
fn min_select_width(theme: &Theme) -> f32 {
    theme.field_width_lg.value() / 2.0
}

fn header_height(theme: &Theme, rows: u32) -> f32 {
    let row = theme.dag_chrome_height().value();
    let n = rows as f32;
    row * n + theme.spacing_xs.value() * (n - 1.0) + theme.spacing_sm.value() * 2.0
}

/// 헤더 — 정체성(아이콘 · 이름 · task 수) + 컨트롤(Select · 러너 배지 · 새로고침).
fn header(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    graphs: &[Graph],
    picked: &mut usize,
    rows: u32,
) {
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    ui.painter().hline(
        rect.x_range(),
        rect.max.y,
        // 이미 premultiply된 구분선 색에 알파를 다시 곱하지 않는다.
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );

    let narrow = rows > 1;
    let graph = &graphs[*picked];
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let gap = theme.spacing_sm.value();
    let row_h = theme.dag_chrome_height().value();
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + pad_x, rect.min.y + pad_y),
        egui::pos2(rect.max.x - pad_x, rect.max.y - pad_y),
    );
    let line = |i: u32| {
        let y = inner.min.y + (row_h + theme.spacing_xs.value()) * i as f32;
        egui::Rect::from_min_size(egui::pos2(inner.min.x, y), egui::vec2(inner.width(), row_h))
    };

    let ident = line(0);
    let icon = theme.dag_canvas_dot_gap().value();
    icons::GIT_TREE
        .image(icon, theme.text_muted().to_egui())
        .paint_at(
            ui,
            egui::Rect::from_center_size(
                egui::pos2(ident.min.x + icon / 2.0, ident.center().y),
                egui::vec2(icon, icon),
            ),
        );
    let name_font = egui::FontId::proportional(theme.font_size_body.value());
    // 앱 문자열 `{done} / {total} done`(빗금 양옆 공백) + 목록 행과 같은 건너뜀 접미사.
    let count = format!(
        "{} / {} done{}",
        super::done_count(&graph.nodes),
        graph.nodes.len(),
        super::skip_count_suffix(&graph.nodes)
    );
    let count_font = tasty_ui_widgets::label_font(theme.font_size_caption.value());
    let count_w = super::node::text_width(ui, &count, &count_font);
    let name_x = ident.min.x + icon + gap;
    let name_limit = if narrow {
        ident.max.x - count_w - gap - name_x
    } else {
        theme.field_width_lg.value()
    };
    let name = super::node::ellipsize(ui, &graph.name, &name_font, name_limit);
    let name_w = super::node::text_width(ui, &name, &name_font);
    ui.painter().text(
        egui::pos2(name_x, ident.center().y),
        egui::Align2::LEFT_CENTER,
        &name,
        name_font,
        theme.text_primary().to_egui(),
    );
    ui.painter().text(
        egui::pos2(name_x + name_w + gap, ident.center().y),
        egui::Align2::LEFT_CENTER,
        &count,
        count_font,
        theme.dag_header_count_fg().to_egui(),
    );

    // 고정 폭 요소를 먼저 뺀 뒤 남은 폭을 Select에 준다.
    let runner_state = &graphs[*picked].runner;
    let badge_w = runner::badge_width(ui, theme, runner_state);
    let refresh_w = row_h;
    let (select_rect, trailing) = match rows {
        1 => {
            let x = name_x + name_w + gap + count_w + gap;
            let r = egui::Rect::from_min_size(
                egui::pos2(x, inner.min.y),
                egui::vec2(theme.field_width_lg.value(), row_h),
            );
            (
                r,
                egui::Rect::from_min_max(
                    egui::pos2(r.max.x + gap, inner.min.y),
                    egui::pos2(inner.max.x, inner.min.y + row_h),
                ),
            )
        }
        2 => {
            let row = line(1);
            let w = (row.width() - badge_w - refresh_w - gap * 2.0).max(min_select_width(theme));
            let r = egui::Rect::from_min_size(row.min, egui::vec2(w, row_h));
            (
                r,
                egui::Rect::from_min_max(egui::pos2(r.max.x + gap, row.min.y), row.max),
            )
        }
        _ => {
            let r = line(1);
            (r, line(2))
        }
    };

    let mut picker = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(select_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let labels: Vec<String> = graphs
        .iter()
        .map(|g| format!("{} / {}", g.workspace, g.name))
        .collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    select(
        &mut picker,
        theme,
        "dag_surface_pick",
        picked,
        &refs,
        select_rect.width(),
        true,
    );

    let badge_rect = egui::Rect::from_min_size(
        egui::pos2(
            trailing.min.x,
            trailing.center().y - theme.dag_runner_height().value() / 2.0,
        ),
        egui::vec2(
            badge_w.min(trailing.width()),
            theme.dag_runner_height().value(),
        ),
    );
    runner::paint_badge(ui, theme, badge_rect, runner_state);
    if !narrow && runner::wants_hint(runner_state) {
        let font = egui::FontId::proportional(theme.font_size_caption.value());
        let avail = trailing.max.x - refresh_w - gap - (badge_rect.max.x + gap);
        ui.painter().text(
            egui::pos2(badge_rect.max.x + gap, trailing.center().y),
            egui::Align2::LEFT_CENTER,
            super::node::ellipsize(ui, runner::RESUME_HINT, &font, avail),
            font,
            theme.text_muted().to_egui(),
        );
    }

    let refresh_rect = egui::Rect::from_min_size(
        egui::pos2(trailing.max.x - refresh_w, trailing.min.y),
        egui::vec2(refresh_w, row_h),
    );
    let mut refresh = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(refresh_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    IconButton::new().show(&mut refresh, theme, &|ui, rect, c| {
        icons::REFRESH.image(rect.height(), c).paint_at(ui, rect);
    });
}

/// 서피스 한 장 — 헤더 + 캔버스 + (선택 시) 상세.
pub fn paint(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    graphs: &[Graph],
    salt: &str,
    initial: Option<&str>,
    // 탭 surface의 포커스. None이면 전시용 캔버스 톤을 쓴다.
    focused: Option<bool>,
) {
    let narrow = rect.width() < tasty_ui_widgets::GRAPH_NARROW_DETAIL_SHEET.value();
    let pick_key = ui.id().with(("dag_surface_pick", salt));
    let sel_key = ui.id().with(("dag_surface_sel", salt));
    let mut picked: usize = ui.data(|d| d.get_temp(pick_key)).unwrap_or(0);
    let mut sel: Option<String> = ui
        .data(|d| d.get_temp(sel_key))
        .unwrap_or_else(|| initial.map(str::to_owned));

    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());

    let rows = header_rows(ui, theme, &graphs[picked].runner, rect.width(), narrow);
    let hh = header_height(theme, rows);
    let header_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), hh));
    header(ui, theme, header_rect, graphs, &mut picked, rows);
    picked = picked.min(graphs.len() - 1);
    let graph = &graphs[picked];
    if sel.as_deref().is_some_and(|s| graph.node(s).is_none()) {
        sel = None;
    }

    let body = egui::Rect::from_min_max(egui::pos2(rect.min.x, header_rect.max.y), rect.max);
    let (canvas_rect, detail_rect) = match (&sel, narrow) {
        (None, _) => (body, None),
        (Some(_), false) => {
            let w = theme.dag_detail_width().value().min(body.width() / 2.0);
            (
                egui::Rect::from_min_max(body.min, egui::pos2(body.max.x - w, body.max.y)),
                Some(egui::Rect::from_min_max(
                    egui::pos2(body.max.x - w, body.min.y),
                    body.max,
                )),
            )
        }
        (Some(_), true) => {
            let h = theme
                .dag_detail_sheet_height()
                .value()
                .min(body.height() / 2.0);
            (
                egui::Rect::from_min_max(body.min, egui::pos2(body.max.x, body.max.y - h)),
                Some(egui::Rect::from_min_max(
                    egui::pos2(body.min.x, body.max.y - h),
                    body.max,
                )),
            )
        }
    };

    canvas::paint(
        ui,
        theme,
        canvas_rect,
        graph,
        Orientation::TopDown,
        &mut sel,
        true,
        focused.map(|f| tasty_ui_widgets::surface_bed(theme, f)),
    );

    if let (Some(dr), Some(id)) = (detail_rect, sel.clone()) {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(dr)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(dr);
        let dock = if narrow { Dock::Sheet } else { Dock::Side };
        if let Some(jump) = detail::draw_docked(&mut child, theme, graph, &id, dock, dr.size()) {
            sel = Some(jump);
        }
    }

    ui.data_mut(|d| {
        d.insert_temp(pick_key, picked);
        d.insert_temp(sel_key, sel);
    });
}

/// `surfaces` 섹션 Spec — 넓은 서피스 + 320px 좁은 서피스.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let graphs = vec![
        super::build_dag(),
        super::index_dag(),
        super::dense_dag(),
        super::cycle_dag(),
    ];
    let narrow_graphs = vec![super::build_dag(), super::index_dag()];
    let h = STAGE_HEIGHT.value();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            let narrow_w = NARROW_STAGE_WIDTH.value();
            let wide_w = (ui.available_width() - narrow_w - theme.spacing_lg.value()).max(narrow_w);
            for (graphs, w, salt, initial) in [
                (&graphs, wide_w, "wide", Some("build_linux")),
                (&narrow_graphs, narrow_w, "narrow", Some("unit")),
            ] {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
                let radius = theme.corner_radius.value();
                ui.painter().rect_stroke(
                    rect,
                    radius,
                    egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
                    egui::StrokeKind::Inside,
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink(theme.border_width.value()))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.set_clip_rect(rect);
                // 예제끼리 위젯 ID가 겹치지 않도록 별도 범위를 사용한다.
                child.push_id(salt, |ui| {
                    paint(
                        ui,
                        theme,
                        rect.shrink(theme.border_width.value()),
                        graphs,
                        salt,
                        initial,
                        None,
                    );
                });
            }
        });
    });
    let focus_graphs = [super::build_dag()];
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (focused, name) in [(true, "focused"), (false, "unfocused")] {
            spec::cluster(ui, theme, name, |ui| {
                let size = egui::vec2(NARROW_STAGE_WIDTH.value(), FOCUS_VIEW_H.value());
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.set_clip_rect(rect);
                child.push_id(name, |ui| {
                    paint(ui, theme, rect, &focus_graphs, name, None, Some(focused));
                });
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "focus",
                "the canvas takes the bed. Header, nodes and detail stay. The workspace popup is not a surface, and its canvas stays dag-canvas-bg",
            ),
            ("header", "8/12 padding · wraps < 640"),
            (
                "header count",
                "{done} / {total} done (spaces kept) + the list row skip suffix · font-label caption",
            ),
            ("canvas", "all remaining space"),
            ("detail", "288 side → 220 sheet"),
            ("narrow floor", "320px, nothing clipped"),
            ("DAG switch", "instant swap + auto-fit"),
        ],
        &[
            TokenChip::new("bg-sidebar", "header band", theme.bg_sidebar().to_egui()),
            TokenChip::without_color("separator", "header hairline"),
            TokenChip::new(
                "dag-header-count-fg",
                "header count",
                theme.dag_header_count_fg().to_egui(),
            ),
            TokenChip::without_color("dag-detail-sheet-height", "220 sheet"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Switching DAG in the header Select replaces the canvas instantly (0ms) and re-fits — a new \
         graph has no position to preserve, so fit is the honest default. Within one DAG the view \
         never re-fits on its own.",
    );
    spec::note(
        ui,
        theme,
        "Both variants open with a task already selected so the two docking modes are visible side \
         by side: 288px side panel on the wide surface, 220px bottom sheet on the 320px one. \
         The host opens with no selection and no detail.",
    );
}
