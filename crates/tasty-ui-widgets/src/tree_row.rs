//! `TreeRow` — 사이드바/트리 행 (디자인 `components/navigation/TreeRow`).
//!
//! [chevron] [icon] label [meta]. height control-height-tree(22), depth 들여쓰기.
//! hover overlay-hover+text-primary, selected surface-active+tree-row-fg-active(아이콘
//! tree-row-icon-fg-active, chevron·meta tree-row-part-fg-active). chevron 은 has_children 일 때만, open 이면 90° 회전.
//! disabled 상태는 없다. 열 수 없는 항목도 일반 행으로 두고, 여는 쪽이 이유를 알린다.

use tasty_type_appearance::theme::Theme;

use crate::icon_button::IconPainter;

const CHEVRON_SLOT: f32 = 14.0;
const ICON_GLYPH: f32 = 14.0;
const GAP: f32 = 6.0;

/// 깊이 0 행의 왼쪽 끝에서 아이콘 가운데까지의 거리. 같은 목록에 끼는 다른 행이 아이콘을 맞출 때 쓴다.
pub fn tree_row_icon_center(theme: &Theme) -> f32 {
    theme.tree_row_gap().value() + CHEVRON_SLOT + GAP + ICON_GLYPH * 0.5
}

/// 트리 행. `selected` 면 surface-active. 클릭 응답 반환(행 전체 클릭).
#[allow(clippy::too_many_arguments)]
pub fn tree_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    depth: u16,
    has_children: bool,
    open: bool,
    icon: Option<IconPainter<'_>>,
    label: &str,
    meta: Option<&str>,
    selected: bool,
) -> egui::Response {
    tree_row_matching(
        ui,
        theme,
        depth,
        has_children,
        open,
        icon,
        label,
        "",
        meta,
        selected,
    )
}

/// `tree_row` 와 같고, 이름에서 `query` 와 맞는 부분(대소문자 무시)을 explorer-match-fg 로 칠한다.
#[allow(clippy::too_many_arguments)] // 이유: tree_row 의 인자에 검색어 하나를 더했다 — 같은 순서를 지켜 호출부를 맞바꿀 수 있게 한다
pub fn tree_row_matching(
    ui: &mut egui::Ui,
    theme: &Theme,
    depth: u16,
    has_children: bool,
    open: bool,
    icon: Option<IconPainter<'_>>,
    label: &str,
    query: &str,
    meta: Option<&str>,
    selected: bool,
) -> egui::Response {
    let row = Row {
        depth,
        has_children,
        open,
        meta,
        selected,
        tail: 0.0,
        rest_fg: None,
    };
    draw(ui, theme, row, icon, label, query).0
}

/// 깊이 0 의 `tree_row_matching` 에 이름 뒤 표지 자리 `tail` 을 먼저 뺀다. 이름은 그 앞에서 잘린다.
/// 응답과 함께 보이는 이름 글자가 끝나는 x 를 돌려준다. 표지는 그 자리부터 `tail` 폭 안에 그린다.
pub fn tree_row_with_tail(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: Option<IconPainter<'_>>,
    label: &str,
    query: &str,
    selected: bool,
    tail: f32,
) -> (egui::Response, f32) {
    tree_row_with_tail_fg(ui, theme, icon, label, query, selected, tail, None)
}

/// `tree_row_with_tail` 과 같고, 선택·호버가 아닐 때의 이름 색을 `rest_fg` 로 바꿀 수 있다.
/// 탐색기가 숨김 항목을 explorer-hidden-fg 로 그릴 때 쓴다.
#[allow(clippy::too_many_arguments)] // 이유: tree_row_with_tail 의 인자 순서를 그대로 두고 색 하나를 끝에 더했다
pub fn tree_row_with_tail_fg(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: Option<IconPainter<'_>>,
    label: &str,
    query: &str,
    selected: bool,
    tail: f32,
    rest_fg: Option<egui::Color32>,
) -> (egui::Response, f32) {
    let row = Row {
        depth: 0,
        has_children: false,
        open: false,
        meta: None,
        selected,
        tail,
        rest_fg,
    };
    draw(ui, theme, row, icon, label, query)
}

/// 행 모양. `tail` 은 이름 오른쪽에 남겨 둘 폭이고, `rest_fg` 는 선택·호버가 아닐 때의 이름 색이다.
struct Row<'a> {
    depth: u16,
    has_children: bool,
    open: bool,
    meta: Option<&'a str>,
    selected: bool,
    tail: f32,
    rest_fg: Option<egui::Color32>,
}

fn draw(
    ui: &mut egui::Ui,
    theme: &Theme,
    row: Row<'_>,
    icon: Option<IconPainter<'_>>,
    label: &str,
    query: &str,
) -> (egui::Response, f32) {
    let Row {
        depth,
        has_children,
        open,
        meta,
        selected,
        tail,
        rest_fg,
    } = row;
    let height = theme.tree_row_height().value();
    let pad_l = theme.tree_row_gap().value();
    let pad_r = theme.spacing_sm.value();
    let radius = theme.corner_radius_sm.value();
    let body = theme.tree_row_font_size().value();
    let width = ui.available_width();

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let indent_per_depth = theme.tree_row_indent().value();

    let state = crate::RowState::from_response(selected, &resp);
    // 트리 행은 mantle·base 양쪽에 놓인다. hover·press 글자는 두 칸에서 모두 등급을 지키는 쪽을 읽는다.
    let parent = crate::RowParent::Mantle;
    if selected {
        ui.painter()
            .rect_filled(rect, radius, theme.tree_row_bg_active().to_egui());
        if state != crate::RowState::Selected {
            let overlay = if state == crate::RowState::SelectedPress {
                theme.overlay_active()
            } else {
                theme.overlay_hover()
            };
            ui.painter()
                .rect_filled(rect, radius, overlay.to_egui_premultiplied());
        }
    } else if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            radius,
            theme.tree_row_bg_hover().to_egui_premultiplied(),
        );
    }

    let fg = match (state, rest_fg) {
        (crate::RowState::Rest, Some(rest)) => rest,
        _ => state.label(theme, parent, theme.tree_row_fg()).to_egui(),
    };
    let meta_fg = state.meta(theme, parent).to_egui();
    let part = state.glyph(theme).to_egui();

    let mut x = rect.left() + pad_l + depth as f32 * indent_per_depth;

    let chev_c = egui::pos2(x + CHEVRON_SLOT * 0.5, rect.center().y);
    if has_children {
        let s = 3.0;
        let pts = if open {
            vec![
                egui::pos2(chev_c.x - s, chev_c.y - s * 0.6),
                egui::pos2(chev_c.x, chev_c.y + s * 0.6),
                egui::pos2(chev_c.x + s, chev_c.y - s * 0.6),
            ]
        } else {
            vec![
                egui::pos2(chev_c.x - s * 0.6, chev_c.y - s),
                egui::pos2(chev_c.x + s * 0.6, chev_c.y),
                egui::pos2(chev_c.x - s * 0.6, chev_c.y + s),
            ]
        };
        ui.painter().add(egui::Shape::line(
            pts,
            egui::Stroke::new(theme.icon_stroke_width.value(), part),
        ));
    }
    x += CHEVRON_SLOT + GAP;

    if let Some(paint) = icon {
        let icon_color = if selected {
            theme.tree_row_icon_fg_active().to_egui()
        } else {
            part
        };
        let irect = egui::Rect::from_center_size(
            egui::pos2(x + ICON_GLYPH * 0.5, rect.center().y),
            egui::vec2(ICON_GLYPH, ICON_GLYPH),
        );
        paint(ui, irect, icon_color);
        x += ICON_GLYPH + GAP;
    }

    let mut right = rect.right() - pad_r;
    if let Some(m) = meta {
        let g = ui.painter().layout_no_wrap(
            m.to_owned(),
            egui::FontId::monospace(theme.tree_row_meta_font_size().value()),
            egui::Color32::PLACEHOLDER,
        );
        let pos = egui::pos2(
            right - g.rect.width(),
            rect.center().y - g.rect.height() * 0.5,
        );
        ui.painter().galley(pos, g.clone(), meta_fg);
        right -= g.rect.width() + GAP;
    }

    let g = ui.painter().layout_job(crate::explorer_match_job(
        theme,
        label,
        query,
        egui::FontId::proportional(body),
        egui::Color32::PLACEHOLDER,
        selected,
    ));
    let right = (right - tail).max(x);
    let label_rect =
        egui::Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(right, rect.bottom()));
    let label_end = (x + g.rect.width()).min(right);
    let pos = egui::pos2(x, rect.center().y - g.rect.height() * 0.5);
    ui.painter().with_clip_rect(label_rect).galley(pos, g, fg);

    (resp, label_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 폭 `width` 의 칸에 `label` 을 꼬리 `tail` 과 함께 그려 (행 오른쪽 끝, 이름 끝 x) 를 돌려준다.
    fn draw_with_tail(width: f32, label: &str, tail: f32) -> (f32, f32) {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let ctx = egui::Context::default();
        let mut out = None;
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui(egui::vec2(width, theme.tree_row_height().value()), |ui| {
                    let (resp, end) = tree_row_with_tail(ui, &theme, None, label, "", false, tail);
                    out = Some((resp.rect.right() - theme.spacing_sm.value(), end));
                });
            });
        }));
        out.expect("drawn")
    }

    #[test]
    fn a_long_name_stops_before_the_tail() {
        let tail = 16.0;
        let (right, end) = draw_with_tail(120.0, &"long-link-name-".repeat(8), tail);
        assert!(
            (end - (right - tail)).abs() < 0.01,
            "end {end}, right {right}"
        );
    }

    #[test]
    fn a_short_name_keeps_the_tail_right_after_it() {
        let (right, end) = draw_with_tail(400.0, "a.md", 16.0);
        assert!(end < right - 16.0 - 100.0, "end {end}, right {right}");
    }
}
