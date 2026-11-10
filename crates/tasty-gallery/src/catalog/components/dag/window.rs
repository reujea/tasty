//! DAG 목록과 상세를 번갈아 보여주는 팝업 예제.
//! 팝업 폭이 상세 패널을 나란히 둘 기준보다 좁아 상세는 항상 아래에 표시한다.

use tasty_dag_layout::Orientation;
use tasty_icons as icons;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, IconButton, Input, MultiSelectLabels, checkbox, hspace, margin_sym,
    multi_select,
};

use super::detail::Dock;
use super::rows::Entry;
use super::{canvas, detail, rows, runner};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 시안 확정 크기 — `--tasty-dag-popup-width` / `-height`.
fn popup_size(theme: &Theme) -> egui::Vec2 {
    egui::vec2(
        theme.dag_popup_width().value(),
        theme.dag_popup_height().value(),
    )
}

/// back bar 높이 — `--tasty-drilldown-backbar-height`.
fn backbar_height(theme: &Theme) -> f32 {
    theme.drilldown_backbar_height().value()
}

/// 창 껍데기 — 보더 + 타이틀바. 내용 rect 를 돌려준다.
fn chrome(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, title: &str) -> egui::Rect {
    let radius = theme.corner_radius.value();
    // 본체와 같은 중앙 팝업이므로 modal 그림자를 배경보다 먼저 그린다.
    ui.painter()
        .add(theme.shadow_modal().to_egui().as_shape(rect, radius));
    ui.painter()
        .rect_filled(rect, radius, theme.bg_panel().to_egui());
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
        egui::StrokeKind::Inside,
    );

    let bar_h = theme.item_height_interactive.value();
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), bar_h));
    ui.painter()
        .rect_filled(bar, 0.0, theme.bg_sidebar().to_egui());
    ui.painter().hline(
        bar.x_range(),
        bar.max.y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );

    let pad = theme.spacing_md.value();
    let gap = theme.spacing_sm.value();
    let icon = theme.icon_glyph_size_sm.value();
    icons::GIT_TREE
        .image(icon, theme.text_muted().to_egui())
        .paint_at(
            ui,
            egui::Rect::from_center_size(
                egui::pos2(bar.min.x + pad + icon / 2.0, bar.center().y),
                egui::vec2(icon, icon),
            ),
        );
    ui.painter().text(
        egui::pos2(bar.min.x + pad + icon + gap, bar.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_primary().to_egui(),
    );
    let close_w = theme.item_height_tab.value();
    let mut close = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                egui::pos2(bar.max.x - theme.spacing_sm.value() - close_w, bar.min.y),
                egui::vec2(close_w, bar_h),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    IconButton::new().show(&mut close, theme, &|ui, rect, c| {
        icons::CLOSE.image(rect.height(), c).paint_at(ui, rect);
    });

    egui::Rect::from_min_max(egui::pos2(rect.min.x, bar.max.y), rect.max)
}

/// 목록 뷰 — 검색+상태 · 토글 · 목록 · 푸터.
fn list_view(ui: &mut egui::Ui, theme: &Theme, body: egui::Rect, entries: &[Entry], salt: &str) {
    // 이미 premultiply된 구분선 색에 알파를 다시 곱하지 않는다.
    let sep = egui::Stroke::new(
        theme.border_width.value(),
        theme.separator.to_egui_premultiplied(),
    );
    let row_h = theme.item_height_interactive.value();
    let filter_h = row_h + theme.spacing_sm.value() * 2.0;
    let toggle_h = theme.checkbox_size().value() + theme.spacing_xs.value() * 2.0;
    let footer_h = theme.item_height_interactive.value() + theme.spacing_sm.value() * 2.0;

    let filter = egui::Rect::from_min_size(body.min, egui::vec2(body.width(), filter_h));
    let mut fu = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(filter)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    egui::Frame::NONE
        .inner_margin(margin_sym(theme.spacing_md, theme.spacing_sm))
        .show(&mut fu, |ui| {
            // 본체 dag_list처럼 띠 안의 간격은 hspace로만 둔다. 물려받은 item_spacing이 더해지면
            // 선택 상자가 오른쪽 여백 밖으로 밀린다.
            ui.spacing_mut().item_spacing.x = 0.0;
            let filter_w = theme.field_width_md.value();
            let search_w = (filter.width()
                - theme.spacing_md.value() * 2.0
                - filter_w
                - theme.spacing_sm.value())
            .max(0.0);
            let mut query = String::new();
            Input::new()
                .icon(&|ui, rect, color| {
                    icons::SEARCH.image(rect.height(), color).paint_at(ui, rect);
                })
                .placeholder("Filter DAGs…")
                .width(search_w)
                .show(ui, theme, &mut query);
            hspace(ui, theme.spacing_sm);
            // 본체 popup 과 같은 위젯·같은 어휘 — rollup 6 종(취소/알 수 없음 없음).
            let mut picked = [false; super::ROLLUP_ORDER.len()];
            let labels: Vec<&str> = super::ROLLUP_ORDER.iter().map(|s| s.label()).collect();
            let summary = MultiSelectLabels {
                none: "Any status",
                some: crate::i18n::t("dag_list.status_some"),
                some_one: crate::i18n::t_count_template("dag_list.status_some", 1),
                all: "All statuses",
            };
            multi_select(
                ui,
                theme,
                salt,
                &mut picked,
                &labels,
                None,
                &summary,
                // 본체 DAG 목록과 같이 일괄 토글 없음.
                None,
                filter_w,
                true,
            );
        });
    ui.painter().hline(body.x_range(), filter.max.y, sep);

    let toggle = egui::Rect::from_min_size(
        egui::pos2(body.min.x, filter.max.y),
        egui::vec2(body.width(), toggle_h),
    );
    let mut tu = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(toggle)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    egui::Frame::NONE
        .inner_margin(margin_sym(theme.spacing_md, theme.spacing_xs))
        .show(&mut tu, |ui| {
            let mut on = false;
            checkbox(
                ui,
                theme,
                &mut on,
                "This workspace only",
                true,
                tasty_ui_widgets::Ground::Panel,
            );
        });
    ui.painter().hline(body.x_range(), toggle.max.y, sep);

    let list = egui::Rect::from_min_max(
        egui::pos2(body.min.x, toggle.max.y),
        egui::pos2(body.max.x, body.max.y - footer_h),
    );
    let mut lu = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(list)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    lu.set_clip_rect(list);
    rows::list(&mut lu, theme, entries, list.width(), salt);

    ui.painter().hline(body.x_range(), list.max.y, sep);
    let footer = egui::Rect::from_min_max(egui::pos2(body.min.x, list.max.y), body.max);
    let pad = theme.spacing_md.value();
    ui.painter().text(
        egui::pos2(footer.min.x + pad, footer.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{} of {} DAGs", entries.len(), entries.len()),
        tasty_ui_widgets::label_font(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
    let btn_w = theme.field_width_md.value() / 2.0;
    let mut bu = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                egui::pos2(footer.max.x - pad - btn_w, footer.min.y),
                egui::pos2(footer.max.x - pad, footer.max.y),
            ))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    Button::new("Close")
        .variant(ButtonVariant::Secondary)
        .show(&mut bu, theme);
}

/// 뒤로 가기 바에 줌 도구와 러너 배지를 두고 그 아래에 캔버스·상세를 그린다.
/// 목록에서 이미 DAG를 선택했으므로 별도 DAG 선택기는 두지 않는다.
fn detail_view(ui: &mut egui::Ui, theme: &Theme, body: egui::Rect, entry: &Entry, salt: &str) {
    let bar_h = backbar_height(theme);
    let bar = egui::Rect::from_min_size(body.min, egui::vec2(body.width(), bar_h));
    ui.painter().hline(
        bar.x_range(),
        bar.max.y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.drilldown_backbar_border().to_egui_premultiplied(),
        ),
    );
    let pad = theme.drilldown_backbar_padding_x().value();
    let glyph_w = theme.item_height_tab.value();
    ui.painter().text(
        egui::pos2(bar.min.x + pad + glyph_w / 2.0, bar.center().y),
        egui::Align2::CENTER_CENTER,
        "\u{2039}",
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_muted().to_egui(),
    );
    ui.painter().text(
        egui::pos2(
            bar.min.x + pad + glyph_w + theme.drilldown_backbar_gap().value(),
            bar.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        &entry.graph.name,
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_primary().to_egui(),
    );

    // 상세 시트는 하단 고정 — popup 폭이 640 아래라 우측 패널 분기가 없다.
    let sheet_h = theme
        .dag_detail_sheet_height()
        .value()
        .min((body.height() - bar_h) / 2.0);
    let rest = egui::Rect::from_min_max(egui::pos2(body.min.x, bar.max.y), body.max);
    let canvas_rect =
        egui::Rect::from_min_max(rest.min, egui::pos2(rest.max.x, rest.max.y - sheet_h));
    let sheet_rect =
        egui::Rect::from_min_max(egui::pos2(rest.min.x, rest.max.y - sheet_h), rest.max);

    // 배율 표시는 캔버스와 같은 fit 계산 결과를 쓴다.
    let layout = super::layout(&entry.graph, theme, Orientation::TopDown);
    let mut view = tasty_ui_widgets::GraphView::default();
    view.zoom = canvas::fit_zoom(canvas_rect.size(), &layout, theme);
    let cluster = tasty_ui_widgets::graph_zoom_cluster_size(theme, true);
    let badge_w = runner::badge_width(ui, theme, &entry.graph.runner);
    let badge_h = theme.dag_runner_height().value();
    let gap = theme.spacing_sm.value();
    let badge_rect = egui::Rect::from_min_size(
        egui::pos2(bar.max.x - pad - badge_w, bar.center().y - badge_h / 2.0),
        egui::vec2(badge_w, badge_h),
    );
    let cluster_rect = egui::Rect::from_min_size(
        egui::pos2(
            badge_rect.min.x - gap - cluster.x,
            bar.center().y - cluster.y / 2.0,
        ),
        cluster,
    );
    // 예제의 버튼은 눌러도 바꿀 상태가 없다. 동작은 본체 팝업에서 확인한다.
    if let Some(action) = tasty_ui_widgets::draw_zoom_cluster(
        ui,
        theme,
        cluster_rect,
        &view,
        Orientation::TopDown,
        &canvas::chrome_labels(),
    ) {
        tracing::trace!("gallery popup zoom cluster ignores {action:?}");
    }
    runner::paint_badge(ui, theme, badge_rect, &entry.graph.runner);

    let mut sel = Some("unit".to_owned());
    // 팝업은 미니맵을 숨기며 줌 도구는 위쪽 뒤로 가기 바에 둔다.
    canvas::paint(
        ui,
        theme,
        canvas_rect,
        &entry.graph,
        Orientation::TopDown,
        &mut sel,
        false,
        None,
    );
    if let Some(id) = sel {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(sheet_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(sheet_rect);
        child.push_id(salt, |ui| {
            detail::draw_docked(ui, theme, &entry.graph, &id, Dock::Sheet, sheet_rect.size());
        });
    }
}

/// popup 한 장.
fn paint(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    entries: &[Entry],
    open: Option<&Entry>,
    salt: &str,
) {
    let body = chrome(ui, theme, rect, "Task DAGs");
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(body);
    child.push_id(salt, |ui| match open {
        Some(entry) => detail_view(ui, theme, body, entry, salt),
        None => list_view(ui, theme, body, entries, salt),
    });
}

/// 시안 "Workspace popup — list ⇄ single DAG" — 목록 뷰 popup 한 장.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let entries = rows::entries();
    let size = popup_size(theme);
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        paint(ui, theme, rect, &entries, None, "list");
    });
    spec::meta(
        ui,
        theme,
        &[
            ("frame", "560 × 460 · resizable"),
            ("titlebar", "28px drag strip"),
            ("list rows", "36px · ~8 visible"),
            ("swap", "DrillDown, 0ms"),
            ("dismiss", "outside click · Esc · ×"),
        ],
        &[
            TokenChip::without_color("dag-popup-width", "560 frame"),
            TokenChip::without_color("dag-popup-height", "460 frame"),
            TokenChip::without_color("drilldown-backbar-height", "36 back bar"),
            TokenChip::without_color("shadow-modal", "lift"),
        ],
    );
    spec::note(
        ui,
        theme,
        "The popup is workspace-scoped: it hides when another workspace becomes active and comes \
         back — with its selection intact — on return. The list itself is not scoped, though; it \
         spans every workspace and labels each row, because \"which workspace did I leave that in\" \
         is the question people actually arrive with.",
    );
    spec::note(
        ui,
        theme,
        "Detail replaces the whole area rather than opening beside it. At 560px there is no room \
         for a side-by-side split that keeps both readable, and the back bar makes the swap \
         cheap to undo.",
    );
}

/// 시안 "Detail view — the header keeps the runner badge only" — 목록 뷰와 디테일 뷰를 나란히.
pub fn draw_detail(ui: &mut egui::Ui, theme: &Theme) {
    let entries = rows::entries();
    let size = popup_size(theme);
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        // 두 팝업이 칸에 함께 들어가지 않으면 시안처럼 위아래로 쌓는다.
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing =
                egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
            for (label, salt, open) in [
                ("list — full header", "list", None),
                (
                    "detail — open a row above: the back bar carries zoom + runner badge",
                    "detail",
                    Some(&entries[0]),
                ),
            ] {
                spec::cluster(ui, theme, label, |ui| {
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    paint(ui, theme, rect, &entries, open, salt);
                });
            }
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("frame", "560×460 — unchanged"),
            ("detail header", "none — the back bar is the chrome"),
            ("back bar actions", "compact zoom cluster + runner badge"),
            ("DAG selector", "hidden in detail"),
            ("back bar", "36 — drilldown-backbar-height"),
            ("back", "restores the list, keeps the selection"),
            ("close / reset", "unchanged host contract"),
        ],
        &[
            TokenChip::without_color("drilldown-backbar-height", "back bar"),
            TokenChip::without_color("dag-popup-width", "560 frame"),
            TokenChip::without_color("dag-popup-height", "460 frame"),
        ],
    );
    spec::note(
        ui,
        theme,
        "The actions slot was empty in the current build; this fills it rather than inventing a \
         second bar. If a future detail view needs no zoom, the slot simply goes back to empty.",
    );
}

/// 무대 높이 참고값 — popup 한 장 그대로.
pub const STAGE_HEIGHT: LogicalPx = LogicalPx(460.0);
