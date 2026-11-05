//! 항목을 골라 상세 화면으로 들어가는 목록. DrillDown과 함께 사용할 수 있다.
//! 선택·호버·비활성을 구분하며 보조 설명이 있으면 행 높이를 늘린다.
//! 아이콘과 오른쪽 액션은 호출자가 제공하고, 결과 인덱스로 자신의 모델을 조회한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::control::Ground;
use crate::icon_button::IconPainter;
use crate::tokens::STRUCT_GAP_1;

/// trailing 슬롯 renderer: 행 우측(chevron 왼쪽)에 Tag/Badge 등 상태 표지 하나를 그린다.
/// 예: `|ui, theme| { tag(ui, theme, "Active", TagVariant::Success, true); }`.
/// disabled 행에서는 [`crate::disabled_chip_scope`] 안에서 호출되므로 `tag`·`badge`는
/// 자동으로 disabled 변형이 된다. 직접 그리는 글자·글리프는
/// [`crate::in_disabled_chip_scope`]로 신호를 읽어 disabled ink를 쓴다.
pub type ListCtrlTrailing<'a> = &'a dyn Fn(&mut egui::Ui, &Theme);

/// 목록의 한 행. 클릭 결과는 items 기준 인덱스다.
pub struct ListCtrlItem<'a> {
    /// 주 라벨 (body 13).
    pub label: &'a str,
    /// 보조 줄 (muted, caption 11). 라벨 아래 1px 간격.
    pub description: Option<&'a str>,
    /// leading 글리프 (icon-size-md 16, muted).
    pub icon: Option<IconPainter<'a>>,
    /// chevron 앞 trailing 슬롯 (Tag/Badge 등).
    pub trailing: Option<ListCtrlTrailing<'a>>,
    /// 선택 불가. 글자·글리프는 disabled ink, chevron은 숨기고 trailing은 disabled 변형으로 둔다.
    pub disabled: bool,
}

impl<'a> ListCtrlItem<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            description: None,
            icon: None,
            trailing: None,
            disabled: false,
        }
    }

    pub fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    pub fn icon(mut self, icon: IconPainter<'a>) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn trailing(mut self, trailing: ListCtrlTrailing<'a>) -> Self {
        self.trailing = Some(trailing);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// 행 하나의 표시 상태.
#[derive(Clone, Copy)]
struct RowMark {
    selected: bool,
    active: bool,
    is_last: bool,
}

/// `ListCtrl::show` 결과.
pub struct ListCtrlOutput {
    /// 이번 프레임에 클릭된 행의 인덱스 (`items` 기준). disabled 행은 제외.
    pub clicked: Option<usize>,
}

/// ListCtrl 빌더. 프레젠테이션 설정만 담고, 상태(`items`/`selected`)는 `show` 인자.
pub struct ListCtrl<'a> {
    chevron: bool,
    divided: bool,
    empty_label: &'a str,
    /// 리스트 폭. `None` 이면 가용 폭(디자인 기본 — 풀폭).
    width: Option<f32>,
    /// 키보드 활성 행. 선택과 별개이며 선택 막대 없이 선택 행 채움과 잉크를 쓴다.
    active: Option<usize>,
    /// 목록이 놓인 바탕. disabled 행의 표지 테두리가 따른다.
    ground: Ground,
}

impl Default for ListCtrl<'_> {
    fn default() -> Self {
        Self {
            chevron: true,
            divided: true,
            empty_label: "",
            width: None,
            active: None,
            ground: Ground::Panel,
        }
    }
}

impl<'a> ListCtrl<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// 키보드 활성 행의 인덱스 (`items` 기준). disabled 행에는 적용하지 않는다.
    pub fn active(mut self, active: Option<usize>) -> Self {
        self.active = active;
        self
    }

    /// 목록이 놓인 바탕. 기본 `Panel`.
    pub fn ground(mut self, ground: Ground) -> Self {
        self.ground = ground;
        self
    }

    /// 우측 drill-in chevron 표시 여부. 기본 true.
    pub fn chevron(mut self, chevron: bool) -> Self {
        self.chevron = chevron;
        self
    }

    /// 행 사이 헤어라인. 기본 true.
    pub fn divided(mut self, divided: bool) -> Self {
        self.divided = divided;
        self
    }

    /// 빈 목록일 때 표시할 비상호작용 라벨 (호출측이 `t()` 로 넘긴다).
    pub fn empty_label(mut self, empty_label: &'a str) -> Self {
        self.empty_label = empty_label;
        self
    }

    /// 리스트 폭 고정. 미지정 시 가용 폭.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// 리스트를 그린다. `selected` 는 선택 행 인덱스 (호출측 소유 상태).
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &Theme,
        items: &[ListCtrlItem<'_>],
        selected: Option<usize>,
    ) -> ListCtrlOutput {
        let width = self.width.unwrap_or_else(|| ui.available_width());

        if items.is_empty() {
            self.empty_state(ui, theme, width);
            return ListCtrlOutput { clicked: None };
        }

        let mut clicked = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, item) in items.iter().enumerate() {
                let is_last = i + 1 == items.len();
                let state = RowMark {
                    selected: selected == Some(i),
                    active: self.active == Some(i),
                    is_last,
                };
                if self.row(ui, theme, item, width, state) {
                    clicked = Some(i);
                }
            }
        });
        ListCtrlOutput { clicked }
    }

    /// 빈 목록 상태 — space-lg 패딩, 중앙, muted, body.
    fn empty_state(&self, ui: &mut egui::Ui, theme: &Theme, width: f32) {
        let pad = theme.spacing_lg.value();
        let galley = ui.painter().layout(
            self.empty_label.to_owned(),
            egui::FontId::proportional(theme.listctrl_font_size().value()),
            theme.text_muted().to_egui(),
            (width - pad * 2.0).max(0.0),
        );
        let h = galley.rect.height() + pad * 2.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
        let pos = egui::pos2(
            rect.center().x - galley.rect.width() * 0.5,
            rect.center().y - galley.rect.height() * 0.5,
        );
        ui.painter().galley(pos, galley, egui::Color32::PLACEHOLDER);
    }

    /// 행 하나. 클릭되면 true.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        item: &ListCtrlItem<'_>,
        width: f32,
        mark: RowMark,
    ) -> bool {
        let RowMark {
            selected: is_selected,
            active,
            is_last,
        } = mark;
        let keyboard_active = active && !item.disabled;
        let pad_x = theme.listctrl_row_padding_x().value();
        let pad_y = theme.listctrl_row_padding_y().value();
        let gap = theme.listctrl_row_gap().value();
        let label_font = egui::FontId::proportional(theme.listctrl_font_size().value());
        let desc_font = egui::FontId::proportional(theme.listctrl_desc_font_size().value());

        let label_h = ui.fonts(|f| f.row_height(&label_font));
        let desc_h = item
            .description
            .map(|_| ui.fonts(|f| f.row_height(&desc_font)));
        let row_h = row_height(
            theme.listctrl_row_min_height(),
            LogicalPx(pad_y),
            LogicalPx(label_h),
            desc_h.map(LogicalPx),
            STRUCT_GAP_1,
        )
        .value();

        let sense = if item.disabled {
            egui::Sense::hover()
        } else {
            egui::Sense::click()
        };
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, row_h), sense);
        let hovered = !item.disabled && resp.hovered();
        let state = if keyboard_active && !is_selected {
            crate::RowState::Selected
        } else {
            crate::RowState::of(
                !item.disabled,
                is_selected,
                hovered,
                !item.disabled && resp.is_pointer_button_down_on(),
            )
        };
        let radius = row_radius(theme.listctrl_radius(), self.divided, is_last).value();

        if is_selected {
            ui.painter()
                .rect_filled(rect, radius, theme.listctrl_row_bg_selected().to_egui());
            let bar = egui::Rect::from_min_max(
                rect.min,
                egui::pos2(
                    rect.left() + theme.listctrl_selected_bar_width().value(),
                    rect.bottom(),
                ),
            );
            ui.painter()
                .rect_filled(bar, 0.0, theme.listctrl_selected_bar().to_egui());
        } else if keyboard_active {
            ui.painter()
                .rect_filled(rect, radius, theme.listctrl_row_bg_selected().to_egui());
        } else if hovered {
            ui.painter().rect_filled(
                rect,
                radius,
                theme.listctrl_row_bg_hover().to_egui_premultiplied(),
            );
        }

        if self.divided && !is_last {
            let y = rect.bottom() - theme.border_width.value() * 0.5;
            ui.painter().line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(
                    theme.border_width.value(),
                    theme.listctrl_divider().to_egui_premultiplied(),
                ),
            );
        }

        // 목록은 mantle·base 양쪽에 놓인다. hover·press는 두 칸에서 등급을 지키는 쪽을 읽는다.
        let parent = crate::RowParent::Mantle;
        let glyph_fg = match state {
            crate::RowState::Rest => theme.listctrl_icon_fg(),
            _ => state.glyph(theme),
        }
        .to_egui();
        let chevron_fg = match state {
            crate::RowState::Rest => theme.listctrl_chevron_fg(),
            _ => state.glyph(theme),
        }
        .to_egui();

        let mut x = rect.left() + pad_x;

        if let Some(paint) = item.icon {
            let glyph = theme.icon_glyph_size_md.value();
            let irect = egui::Rect::from_center_size(
                egui::pos2(x + glyph * 0.5, rect.center().y),
                egui::vec2(glyph, glyph),
            );
            paint(ui, irect, glyph_fg);
            x += glyph + gap;
        }

        let mut right = rect.right() - pad_x;
        if self.chevron && !item.disabled {
            let glyph = theme.icon_glyph_size_sm.value();
            paint_chevron_right(
                ui,
                theme,
                egui::pos2(right - glyph * 0.5, rect.center().y),
                chevron_fg,
            );
            right -= glyph + theme.spacing_sm.value();
        }
        if let Some(trailing) = item.trailing {
            let trail_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left() + pad_x, rect.top()),
                egui::pos2(right, rect.bottom()),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(trail_rect)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if item.disabled {
                // egui의 비활성 Ui는 색을 흐리므로 쓰지 않는다. 표지는 disabled 변형으로 그리고
                // 선택 가능한 라벨만 끈다. 행 자체는 위에서 hover만 감지한다.
                child.style_mut().interaction.selectable_labels = false;
                crate::chip::disabled_chip_scope(&mut child, self.ground, |ui| trailing(ui, theme));
            } else {
                trailing(&mut child, theme);
            }
            right = child.min_rect().left() - theme.spacing_sm.value();
        }

        let text_w = (right - x - gap).max(0.0);
        let label_fg = state
            .label(theme, parent, theme.listctrl_label_fg())
            .to_egui();
        let label_galley = truncated_galley(ui, item.label, label_font, label_fg, text_w);
        let desc_galley = item.description.map(|d| {
            truncated_galley(
                ui,
                d,
                desc_font,
                match state {
                    crate::RowState::Rest => theme.listctrl_desc_fg(),
                    _ => state.meta(theme, parent),
                }
                .to_egui(),
                text_w,
            )
        });
        let content_h = label_galley.rect.height()
            + desc_galley
                .as_ref()
                .map(|g| g.rect.height() + STRUCT_GAP_1.value())
                .unwrap_or(0.0);
        let mut y = rect.center().y - content_h * 0.5;
        ui.painter().galley(
            egui::pos2(x, y),
            label_galley.clone(),
            egui::Color32::PLACEHOLDER,
        );
        y += label_galley.rect.height() + STRUCT_GAP_1.value();
        if let Some(g) = desc_galley {
            ui.painter()
                .galley(egui::pos2(x, y), g, egui::Color32::PLACEHOLDER);
        }

        !item.disabled && resp.clicked()
    }
}

/// 한 줄 말줄임 galley — 폭 초과 시 '…' 로 잘라낸다 (디자인 ellipsis 전사).
fn truncated_galley(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width);
    ui.fonts(|f| f.layout_job(job))
}

/// drill-in chevron(›) — tree_row 관례대로 painter 폴리라인으로 직접 그린다.
fn paint_chevron_right(ui: &egui::Ui, theme: &Theme, center: egui::Pos2, color: egui::Color32) {
    let s = 3.0;
    let pts = vec![
        egui::pos2(center.x - s * 0.6, center.y - s),
        egui::pos2(center.x + s * 0.6, center.y),
        egui::pos2(center.x - s * 0.6, center.y + s),
    ];
    ui.painter().add(egui::Shape::line(
        pts,
        egui::Stroke::new(theme.icon_stroke_width.value(), color),
    ));
}

/// 행 높이 — `max(row-min-height, label + (1px + desc) + 상하 패딩)`.
fn row_height(
    min_height: LogicalPx,
    pad_y: LogicalPx,
    label_h: LogicalPx,
    desc_h: Option<LogicalPx>,
    desc_gap: LogicalPx,
) -> LogicalPx {
    let content = label_h.value() + desc_h.map(|d| d.value() + desc_gap.value()).unwrap_or(0.0);
    LogicalPx((content + pad_y.value() * 2.0).max(min_height.value()))
}

/// 행 radius — divided 모드에선 헤어라인을 가진(마지막 아닌) 행만 radius 0
/// (디자인 CSS `.tasty-listctrl--divided .row:not(:last-child) { border-radius: 0 }`).
fn row_radius(radius: LogicalPx, divided: bool, is_last: bool) -> LogicalPx {
    if divided && !is_last {
        LogicalPx(0.0)
    } else {
        radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_height_desc_없으면_min_height() {
        // label 13 + pad 16 = 29 < 36 → min-height 로 클램프.
        let h = row_height(
            LogicalPx(36.0),
            LogicalPx(8.0),
            LogicalPx(13.0),
            None,
            LogicalPx(1.0),
        );
        assert_eq!(h.value(), 36.0);
    }

    #[test]
    fn row_height_desc_있으면_내용만큼_증가() {
        // 13 + 1 + 11 + 16 = 41 > 36.
        let h = row_height(
            LogicalPx(36.0),
            LogicalPx(8.0),
            LogicalPx(13.0),
            Some(LogicalPx(11.0)),
            LogicalPx(1.0),
        );
        assert_eq!(h.value(), 41.0);
    }

    /// 행 둘(enabled·disabled)을 그리고 `click` 행을 누른 결과, trailing이 받은 신호,
    /// trailing 글자색을 모은다.
    fn run_rows(click: Option<usize>) -> (Option<usize>, Vec<bool>, Vec<egui::Color32>) {
        use std::cell::RefCell;
        let theme = tasty_themes::mocha_fallback();
        let seen = RefCell::new(Vec::new());
        let trailing = |ui: &mut egui::Ui, th: &Theme| {
            seen.borrow_mut()
                .push(crate::chip::in_disabled_chip_scope(ui));
            crate::chip::tag(ui, th, "Active", crate::chip::TagVariant::Success, true);
        };
        let items = [
            ListCtrlItem::new("Default").trailing(&trailing),
            ListCtrlItem::new("Readline")
                .trailing(&trailing)
                .disabled(true),
        ];
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
        let mut clicked = None;
        let mut frame = |events: Vec<egui::Event>| {
            let raw = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    seen.borrow_mut().clear();
                    if let Some(i) = ListCtrl::new().show(ui, &theme, &items, None).clicked {
                        clicked = Some(i);
                    }
                });
            })
        };
        let mut output = frame(Vec::new());
        if let Some(row) = click {
            // 행 가운데를 누르고 뗀다. 행 높이는 min-height, 패널 위 여백은 CentralPanel 기본값이다.
            let top = ctx.style().spacing.window_margin.top as f32;
            let row_h = theme.listctrl_row_min_height().value();
            let pos = egui::pos2(100.0, top + row_h * (row as f32 + 0.5));
            let press = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(pos), press(true)]);
            output = frame(vec![press(false)]);
        }
        let mut inks = Vec::new();
        for clipped in &output.shapes {
            if let egui::Shape::Text(t) = &clipped.shape
                && t.galley.text() == "Active"
            {
                inks.push(t.fallback_color);
            }
        }
        (clicked, seen.into_inner(), inks)
    }

    /// disabled 행은 누르면 clicked가 없고, trailing은 disabled 신호를 받아 fade 없는 disabled ink로 그린다.
    #[test]
    fn disabled_row_ignores_clicks_and_signals_its_trailing() {
        let theme = tasty_themes::mocha_fallback();
        // 대조: 같은 방법으로 enabled 행을 누르면 clicked가 잡힌다.
        assert_eq!(run_rows(Some(0)).0, Some(0));
        let (clicked, seen, inks) = run_rows(Some(1));
        assert_eq!(clicked, None);
        assert_eq!(seen, vec![false, true]);
        assert_eq!(
            inks,
            vec![
                theme.accent_success().to_egui(),
                theme.tag_disabled_fg().to_egui()
            ]
        );
    }

    #[test]
    fn row_radius_divided_는_마지막_행만_radius_유지() {
        let r = LogicalPx(2.0);
        assert_eq!(row_radius(r, true, false).value(), 0.0);
        assert_eq!(row_radius(r, true, true).value(), 2.0);
        assert_eq!(row_radius(r, false, false).value(), 2.0);
        assert_eq!(row_radius(r, false, true).value(), 2.0);
    }
}
