//! 아이콘·접미 라벨을 붙일 수 있는 한 줄 입력 필드.
//! 포커스와 잘못된 값의 테두리는 애니메이션 없이 바로 표시한다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::icon_button::IconPainter;
use crate::tooltip::{Tooltip, tooltip_hover_delay_elapsed};

/// Input 빌더.
pub struct Input<'a> {
    placeholder: &'a str,
    mono: bool,
    font_size: Option<LogicalPx>,
    icon_size: Option<LogicalPx>,
    text_margin: Option<LogicalPx>,
    invalid: bool,
    /// 포커스가 없을 때만 invalid 테두리를 그린다. 입력 중인 값은 아직 판정 대상이 아니다.
    invalid_at_rest: bool,
    /// 값이 칸보다 길면 hover Tooltip 으로 전체 값을 보인다.
    full_value_tooltip: bool,
    enabled: bool,
    /// 지금은 고칠 수 없지만 읽는 값(시안 `readOnly`). disabled와 함께면 무시한다.
    read_only: bool,
    /// 고정 폭. `None` 이면 가용 폭을 채운다(디자인 `block`).
    width: Option<f32>,
    icon: Option<IconPainter<'a>>,
    addon: Option<&'a str>,
    /// 텍스트 색 override. `None` 이면 `input_fg`(text-primary).
    text_color: Option<egui::Color32>,
    /// 글자 정렬. 기본은 좌측이고, 숫자 필드만 우측을 쓴다.
    align: egui::Align,
    /// 입력의 고정 id. `None` 이면 그린 순서로 정해진다. 그리는 자리가 프레임마다 바뀌어도 포커스를
    /// 지켜야 하는 입력이 쓴다.
    id: Option<egui::Id>,
    /// 포커스가 없어도 포커스 테두리를 그린다. 확정한 입력의 결과를 기다리는 동안 쓴다.
    focus_look: bool,
    /// 입력 뒤 정사각 칸과 그 변 길이. 칸을 그리는 painter 는 input-icon-fg 색으로 불린다.
    trailing: Option<(IconPainter<'a>, egui::Vec2)>,
    /// 비편집 표시만 대체한다. TextEdit에는 언제나 원문 버퍼를 전달한다.
    idle_text: Option<&'a str>,
    /// 놓인 바탕. raised면 상자·쉬는 테두리·disabled·readonly 상자가 raised 토큰이다.
    ground: crate::Ground,
}

/// 상자 채움. disabled > readonly > 기본 순이며 raised 바탕이면 raised 토큰이다.
fn box_fill(theme: &Theme, enabled: bool, read_only: bool, raised: bool) -> HexColor {
    match (!enabled, read_only, raised) {
        (true, _, false) => theme.state_disabled_fill(),
        (true, _, true) => theme.state_disabled_fill_raised(),
        (false, true, false) => theme.input_readonly_bg(),
        (false, true, true) => theme.input_readonly_raised_bg(),
        (false, false, false) => theme.input_bg(),
        (false, false, true) => theme.input_raised_bg(),
    }
}

/// 포커스·invalid가 아닐 때의 테두리. raised 바탕이면 raised 토큰이다.
fn box_rest_border(theme: &Theme, enabled: bool, read_only: bool, raised: bool) -> HexColor {
    match (!enabled, read_only, raised) {
        (true, _, false) => theme.state_disabled_border(),
        (true, _, true) => theme.state_disabled_border_raised(),
        (false, true, false) => theme.input_readonly_border(),
        (false, true, true) => theme.input_readonly_raised_border(),
        (false, false, false) => theme.input_border(),
        (false, false, true) => theme.input_raised_border(),
    }
}

impl Default for Input<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Input<'a> {
    pub fn new() -> Self {
        Self {
            placeholder: "",
            mono: false,
            font_size: None,
            icon_size: None,
            text_margin: None,
            invalid: false,
            invalid_at_rest: false,
            full_value_tooltip: false,
            enabled: true,
            read_only: false,
            width: None,
            icon: None,
            addon: None,
            text_color: None,
            align: egui::Align::LEFT,
            id: None,
            focus_look: false,
            trailing: None,
            idle_text: None,
            ground: crate::Ground::Panel,
        }
    }

    /// 놓인 바탕(시안 `ground`). surface-raised 위에서는 `Ground::Raised`.
    pub fn ground(mut self, ground: crate::Ground) -> Self {
        self.ground = ground;
        self
    }

    pub fn id(mut self, id: egui::Id) -> Self {
        self.id = Some(id);
        self
    }

    pub(crate) fn idle_text(mut self, text: Option<&'a str>) -> Self {
        self.idle_text = text;
        self
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }

    /// Override the text size with a Theme length; existing mono inputs retain caption size.
    pub fn font_size(mut self, size: LogicalPx) -> Self {
        self.font_size = Some(size);
        self
    }

    /// Override the leading icon slot with a Theme length; existing inputs retain md size.
    pub fn icon_size(mut self, size: LogicalPx) -> Self {
        self.icon_size = Some(size);
        self
    }

    /// Optional inner editor margin, in addition to the Input box padding. Unset uses
    /// zero horizontal margin and egui's default vertical margin. Zero also removes the latter.
    pub fn text_margin(mut self, margin: LogicalPx) -> Self {
        self.text_margin = Some(margin);
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// 포커스가 없을 때만 invalid 테두리를 그린다. 포커스가 있으면 일반 포커스 표시다.
    pub fn invalid_at_rest(mut self, invalid: bool) -> Self {
        self.invalid_at_rest = invalid;
        self
    }

    /// 값이 칸보다 길면 hover 시 `tooltip-delay` 뒤 전체 값을 mono Tooltip 으로 보인다.
    pub fn full_value_tooltip(mut self, on: bool) -> Self {
        self.full_value_tooltip = on;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 읽기 전용. disabled와 같은 중립 상자(input-readonly-bg·border)에 값은
    /// input-readonly-fg(text-secondary-raised)로 그린다. 편집은 막고 포커스·선택·복사는 허용하며,
    /// 포커스는 1px focus 테두리만 두고 ring은 그리지 않는다. `enabled(false)`면 무시한다.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// 고정 폭(px). 미지정 시 가용 폭을 채운다.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// 입력 앞의 아이콘. 크기와 색은 Theme에서 읽는다.
    pub fn icon(mut self, icon: IconPainter<'a>) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn addon(mut self, addon: &'a str) -> Self {
        self.addon = Some(addon);
        self
    }

    /// 입력 글자색을 지정한다. 기본은 input_fg이며 다른 색도 Theme에서 가져와야 한다.
    pub fn text_color(mut self, color: egui::Color32) -> Self {
        self.text_color = Some(color);
        self
    }

    /// 포커스가 없어도 포커스 테두리를 그린다.
    pub fn focus_look(mut self, focus_look: bool) -> Self {
        self.focus_look = focus_look;
        self
    }

    /// 입력 뒤에 변 길이 `size` 인 칸을 두고 `paint` 로 그린다(예: 기다리는 동안의 Spinner).
    pub fn trailing(mut self, paint: IconPainter<'a>, size: f32) -> Self {
        self.trailing = Some((paint, egui::vec2(size, size)));
        self
    }

    /// Reserve a rectangular trailing slot, such as a status Tag inside the input.
    pub fn trailing_box(
        mut self,
        paint: IconPainter<'a>,
        width: LogicalPx,
        height: LogicalPx,
    ) -> Self {
        self.trailing = Some((paint, egui::vec2(width.value(), height.value())));
        self
    }

    /// 기본은 왼쪽 정렬. 숫자를 비교하는 필드는 오른쪽 정렬을 사용할 수 있다.
    pub fn align(mut self, align: egui::Align) -> Self {
        self.align = align;
        self
    }

    /// 그리고 TextEdit 응답을 반환한다(`response.changed()` 로 변경 감지).
    pub fn show(self, ui: &mut egui::Ui, theme: &Theme, buf: &mut String) -> egui::Response {
        let height = theme.input_height().value();
        let pad_x = theme.input_padding_x().value();
        let gap = theme.input_gap().value();
        let radius = theme.input_radius().value();
        let body = theme.input_font_size().value();
        // trailing addon 의 mono caption — 대응 input component 토큰 없음(semantic).
        let caption = theme.font_size_caption.value();

        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (outer, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());

        // disabled는 opacity 없이 disabled 상자 role과 disabled ink를 쓴다.
        let read_only = self.enabled && self.read_only;
        let raised = self.ground == crate::Ground::Raised;
        let bg = box_fill(theme, self.enabled, read_only, raised);
        ui.painter().rect_filled(outer, radius, bg.to_egui());

        let inner = outer.shrink2(egui::vec2(pad_x, 0.0));
        let inner_w = inner.width();

        let icon_glyph = self.icon_size.unwrap_or(theme.icon_glyph_size_md).value();
        let icon_w = if self.icon.is_some() {
            icon_glyph + gap
        } else {
            0.0
        };
        let addon_font = egui::FontId::monospace(caption);
        let addon_galley = self.addon.map(|a| {
            ui.painter().layout_no_wrap(
                a.to_owned(),
                addon_font.clone(),
                egui::Color32::PLACEHOLDER,
            )
        });
        let addon_w = addon_galley
            .as_ref()
            .map(|g| g.rect.width() + gap)
            .unwrap_or(0.0);
        let trailing_w = self.trailing.map_or(0.0, |(_, size)| size.x + gap);
        let te_w = (inner_w - icon_w - addon_w - trailing_w).max(0.0);

        let muted = if self.enabled {
            theme.input_icon_fg().to_egui()
        } else {
            theme.state_disabled_fg().to_egui()
        };
        // 부모에는 위의 outer 한 칸만 할당한다. allocate_new_ui는 자식 영역(inner)으로 부모 커서를
        // 다시 옮겨 가로 배치에서 다음 위젯이 padding만큼 겹치므로, 할당하지 않는 자식 Ui에 그린다.
        let mut field = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let resp = {
            let ui = &mut field;
            ui.spacing_mut().item_spacing.x = gap;
            if let Some(paint) = self.icon {
                let (irect, _) = ui
                    .allocate_exact_size(egui::vec2(icon_glyph, icon_glyph), egui::Sense::hover());
                paint(ui, irect, muted);
            }
            let font = if self.mono {
                egui::FontId::monospace(self.font_size.map_or(caption, LogicalPx::value))
            } else {
                egui::FontId::proportional(self.font_size.map_or(body, LogicalPx::value))
            };
            let (hint, text_color) = if read_only {
                (
                    tasty_egui_theme::hint_text(theme, self.placeholder),
                    theme.input_readonly_fg().to_egui(),
                )
            } else if self.enabled {
                (
                    tasty_egui_theme::hint_text(theme, self.placeholder),
                    self.text_color
                        .unwrap_or_else(|| theme.input_fg().to_egui()),
                )
            } else {
                let ink = theme.state_disabled_fg().to_egui();
                (egui::RichText::new(self.placeholder).color(ink), ink)
            };
            // egui의 비활성 Ui는 색을 배경 쪽으로 흐리므로 대신 비대화형 TextEdit로 입력을 막는다.
            // 읽기 전용은 변경할 수 없는 &str 버퍼를 넘겨 편집만 막고 포커스·선택·복사는 둔다.
            let mut view: &str = buf.as_str();
            let text: &mut dyn egui::TextBuffer = if read_only { &mut view } else { buf };
            // 가로 여백은 상자 padding 이 맡는다(시안 `input { padding: 0 }`). egui 기본 가로 margin 을
            // 두면 글자 칸 바깥이 te_w 보다 넓어져 addon·trailing 이 오른쪽 padding 으로 밀린다.
            // 세로는 egui 기본값(2)을 그대로 둔다.
            let te = egui::TextEdit::singleline(text)
                .frame(false)
                .margin(crate::margin_sym(
                    LogicalPx(0.0),
                    crate::tokens::STRUCT_GAP_2,
                ))
                .desired_width(te_w)
                .hint_text(hint)
                .font(font.clone())
                .horizontal_align(self.align)
                // 원문은 입력·복사·커서용으로 남기고 표시 글자만 아래에서 덧그린다.
                .text_color(
                    self.idle_text
                        .map_or(text_color, |_| egui::Color32::TRANSPARENT),
                )
                .interactive(self.enabled);
            let te = if let Some(margin) = self.text_margin {
                te.margin(egui::vec2(margin.value(), margin.value()))
            } else {
                te
            };
            let te = match self.id {
                Some(id) => te.id(id),
                None => te,
            };
            let out = te.show(ui);
            paint_idle_text(ui, &out, self.idle_text, font, text_color);
            let clipped = out.galley.size().x > out.text_clip_rect.width();
            if self.full_value_tooltip {
                let hovered = clipped && out.response.hovered();
                if tooltip_hover_delay_elapsed(ui.ctx(), theme, out.response.id, hovered) {
                    let tip = Tooltip::new(out.galley.text())
                        .mono(true)
                        .id_source(out.response.id.with("full_value"));
                    // 버블이 포인터를 덮으면 hover 가 끊겨 깜박이므로 들어가는 쪽에 둔다.
                    tip.placement_top_then_bottom(ui.ctx(), theme, outer, ui.ctx().screen_rect())
                        .show(ui, theme, outer);
                }
            }
            let r = out.response;
            if let Some(g) = addon_galley {
                let (arect, _) = ui.allocate_exact_size(g.rect.size(), egui::Sense::hover());
                ui.painter().galley(arect.min, g, muted);
            }
            if let Some((paint, size)) = self.trailing {
                let (trect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                paint(ui, trect, muted);
            }
            r
        };
        let focused = resp.has_focus() || (self.focus_look && self.enabled);

        self.paint_edge(ui, theme, outer, read_only, resp.has_focus(), focused);

        // 팝오버 앵커가 아이콘 폭만큼 밀리지 않도록 응답 영역은 필드 전체로 바꾼다.
        // TextEdit의 ID와 포커스 상태는 유지한다.
        let mut resp = resp;
        resp.rect = outer;
        resp
    }

    /// 상자 테두리와 포커스 링. 읽기 전용은 링 없이 포커스 테두리만 바꾼다.
    fn paint_edge(
        &self,
        ui: &egui::Ui,
        theme: &Theme,
        outer: egui::Rect,
        read_only: bool,
        has_focus: bool,
        focused: bool,
    ) {
        let radius = theme.input_radius().value();
        let bw = theme.border_width.value();
        let raised = self.ground == crate::Ground::Raised;
        let invalid = self.invalid || (self.invalid_at_rest && !has_focus);
        let border = if !self.enabled || (read_only && !has_focus) {
            box_rest_border(theme, self.enabled, read_only, raised).to_egui()
        } else if read_only {
            theme.input_border_focus().to_egui()
        } else if invalid {
            theme.input_border_invalid().to_egui()
        } else if focused {
            theme.input_border_focus().to_egui()
        } else {
            box_rest_border(theme, true, false, raised).to_egui()
        };
        ui.painter().rect_stroke(
            outer,
            radius,
            egui::Stroke::new(bw, border),
            egui::StrokeKind::Inside,
        );
        if focused && !read_only {
            let ring = if invalid {
                theme.input_border_invalid().to_egui()
            } else {
                theme.input_border_focus().to_egui()
            };
            ui.painter().rect_stroke(
                outer.expand(bw),
                radius,
                egui::Stroke::new(bw, ring),
                egui::StrokeKind::Outside,
            );
        }
    }
}

/// 표시 문자열만 덧그리며 TextEdit의 원문·응답·커서 상태는 그대로 둔다.
fn paint_idle_text(
    ui: &egui::Ui,
    out: &egui::text_edit::TextEditOutput,
    display: Option<&str>,
    font: egui::FontId,
    text_color: egui::Color32,
) {
    let Some(display) = display else { return };
    let painter = ui.painter().with_clip_rect(out.text_clip_rect);
    if ui.ctx().memory(|m| m.has_focus(out.response.id)) {
        painter.galley_with_override_text_color(out.galley_pos, out.galley.clone(), text_color);
    } else {
        let galley = painter.layout_no_wrap(display.to_owned(), font, text_color);
        let overflow = LogicalPx((galley.size().x - out.text_clip_rect.width()).max(0.0));
        let x = LogicalPx(out.text_clip_rect.left()) - overflow;
        painter.galley(egui::pos2(x.value(), out.galley_pos.y), galley, text_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Frame {
        focused: bool,
        output: egui::FullOutput,
    }

    /// 읽기 전용 Input 하나를 `events`와 함께 한 프레임 그린다.
    fn frame(
        ctx: &egui::Context,
        theme: &Theme,
        buf: &mut String,
        events: Vec<egui::Event>,
    ) -> Frame {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 200.0),
            )),
            events,
            ..Default::default()
        };
        let mut focused = false;
        let output = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let resp = Input::new().mono(true).read_only(true).show(ui, theme, buf);
                focused = resp.has_focus();
            });
        });
        Frame { focused, output }
    }

    fn strokes(output: &egui::FullOutput) -> Vec<egui::Color32> {
        output
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Rect(r) if r.stroke.width > 0.0 => Some(r.stroke.color),
                _ => None,
            })
            .collect()
    }

    /// 읽기 전용 값은 클릭하면 포커스를 받고, 전체 선택·복사는 되지만 입력으로는 바뀌지 않는다.
    /// 포커스 때는 1px focus 테두리 하나만 있고 ring은 없다.
    #[test]
    fn read_only_input_focuses_and_copies_but_does_not_edit() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let mut buf = String::from("#89b4fa");

        let idle = frame(&ctx, &theme, &mut buf, Vec::new());
        assert!(!idle.focused);
        assert_eq!(
            strokes(&idle.output),
            vec![theme.input_readonly_border().to_egui()]
        );
        let fills: Vec<_> = idle
            .output
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Rect(r) if r.fill != egui::Color32::TRANSPARENT => Some(r.fill),
                _ => None,
            })
            .collect();
        assert!(
            fills.contains(&theme.input_readonly_bg().to_egui()),
            "{fills:?}"
        );
        let inks: Vec<_> = idle
            .output
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == "#89b4fa" => Some(t.fallback_color),
                _ => None,
            })
            .collect();
        assert_eq!(inks, vec![theme.input_readonly_fg().to_egui()]);

        let pos = egui::pos2(40.0, 8.0 + theme.input_height().value() * 0.5);
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &ctx,
            &theme,
            &mut buf,
            vec![egui::Event::PointerMoved(pos), press(true)],
        );
        let clicked = frame(&ctx, &theme, &mut buf, vec![press(false)]);
        let clicked = if clicked.focused {
            clicked
        } else {
            frame(&ctx, &theme, &mut buf, Vec::new())
        };
        assert!(clicked.focused, "a read-only value takes focus on click");

        let edited = frame(
            &ctx,
            &theme,
            &mut buf,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::Copy,
                egui::Event::Text("x".into()),
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(buf, "#89b4fa");
        assert!(edited.focused);
        let copied = edited
            .output
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "#89b4fa"));
        assert!(copied, "{:?}", edited.output.platform_output.commands);
        assert_eq!(
            strokes(&edited.output),
            vec![theme.input_border_focus().to_egui()]
        );
    }

    /// 가로 배치에서 Input은 outer 한 칸만 차지한다. 다음 위젯은 outer.right + item_spacing.x에서
    /// 시작하고, 필드 안쪽 padding만큼 겹치지 않는다.
    #[test]
    fn input_takes_only_its_outer_rect_in_a_horizontal_layout() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let mut buf = String::from("#89b4fa");
        let mut seen = None;
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            // 첫 프레임은 폰트 준비용이다. 출력은 쓰지 않는다.
            drop(ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let field = Input::new()
                            .width(theme.field_width_xs.value())
                            .show(ui, &theme, &mut buf);
                        let next =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        seen = Some((field.rect, next.0, ui.spacing().item_spacing.x));
                    });
                });
            }));
        }
        let (outer, next, gap) = seen.expect("frame ran");
        assert_eq!(outer.width(), theme.field_width_xs.value());
        assert_eq!(next.left(), outer.right() + gap);
    }

    /// 상자 padding 안쪽이 곧 글자·addon·trailing 칸이다(시안 `input { padding: 0 }`).
    /// 글자는 왼쪽 padding 에서 시작하고 addon 과 trailing 의 오른쪽 끝은 오른쪽 padding 에 닿는다.
    #[test]
    fn text_addon_and_trailing_sit_on_the_box_padding() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let pad = theme.input_padding_x().value();
        let size = theme.icon_glyph_size_md.value();
        let trail = std::cell::Cell::new(egui::Rect::NOTHING);
        let paint = |_: &mut egui::Ui, r: egui::Rect, _: egui::Color32| trail.set(r);
        let (mut num, mut name) = (String::from("2500000"), String::from("main"));
        let mut seen = None;
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            // 첫 프레임은 폰트 준비용이다.
            let out = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let a = Input::new()
                        .mono(true)
                        .addon("lines")
                        .width(theme.field_width_color.value())
                        .show(ui, &theme, &mut num);
                    let t = Input::new()
                        .trailing(&paint, size)
                        .width(theme.field_width_xs.value())
                        .show(ui, &theme, &mut name);
                    seen = Some((a.rect, t.rect));
                });
            });
            let text = |s: &str| {
                out.shapes.iter().find_map(|c| match &c.shape {
                    egui::Shape::Text(t) if t.galley.text() == s => {
                        Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                    }
                    _ => None,
                })
            };
            if let (Some(value), Some(addon)) = (text("2500000"), text("lines")) {
                let (a, t) = seen.expect("frame ran");
                assert_eq!(value.left(), a.left() + pad);
                assert!(
                    (addon.right() - (a.right() - pad)).abs() < 0.5,
                    "{addon:?} {a:?}"
                );
                assert_eq!(trail.get().right(), t.right() - pad);
            }
        }
        assert!(trail.get().is_positive(), "trailing was painted");
    }

    /// Tooltip 이 포인터를 덮지 않도록 입력 위에 비워 두는 높이.
    const FIELD_TOP: f32 = 100.0;

    /// `invalid_at_rest` Input 을 그리고 칠한 테두리 색과 Tooltip 글자를 돌려준다.
    fn at_rest_frame(
        ctx: &egui::Context,
        theme: &Theme,
        buf: &mut String,
        events: Vec<egui::Event>,
        time: f64,
    ) -> (Vec<egui::Color32>, Vec<String>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 200.0),
            )),
            time: Some(time),
            events,
            ..Default::default()
        };
        let output = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_space(FIELD_TOP);
                Input::new()
                    .id(egui::Id::new("field"))
                    .mono(true)
                    .width(100.0)
                    .invalid_at_rest(true)
                    .full_value_tooltip(true)
                    .show(ui, theme, buf);
            });
        });
        let texts = output
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        (strokes(&output), texts)
    }

    #[test]
    fn invalid_at_rest_is_red_only_without_focus() {
        let theme = tasty_themes::mocha_fallback();
        let red = theme.input_border_invalid().to_egui();
        let ctx = egui::Context::default();
        let mut buf = "x".to_owned();
        for t in 0..2 {
            let (rest, _) = at_rest_frame(&ctx, &theme, &mut buf, Vec::new(), f64::from(t));
            assert!(rest.contains(&red), "쉬는 상태에 빨강이 없다");
        }
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("field")));
        let (focused, _) = at_rest_frame(&ctx, &theme, &mut buf, Vec::new(), 2.0);
        assert!(!focused.contains(&red), "포커스 중에 빨강을 그렸다");
        assert!(focused.contains(&theme.input_border_focus().to_egui()));
    }

    /// 칸보다 긴 값만, 그리고 hover 지연 뒤에만 전체 값을 보인다.
    #[test]
    fn a_clipped_value_shows_whole_after_the_hover_delay() {
        let theme = tasty_themes::mocha_fallback();
        let hover = egui::Event::PointerMoved(egui::pos2(40.0, FIELD_TOP + 20.0));
        let shows = |value: &str| {
            let ctx = egui::Context::default();
            let mut buf = value.to_owned();
            let delay = theme.tooltip_delay().to_secs_f64();
            let mut early = false;
            let mut late = false;
            // 새 Area 는 첫 프레임에 크기만 재므로 지연 뒤 두 프레임을 돌린다.
            let frames = [
                (0.0, "a"),
                (0.1, "a"),
                (0.2, "e"),
                (delay + 1.0, "a"),
                (delay + 1.1, "l"),
            ];
            for (time, name) in frames {
                let (_, texts) = at_rest_frame(&ctx, &theme, &mut buf, vec![hover.clone()], time);
                let seen = texts.iter().filter(|t| *t == value).count() > 1;
                match name {
                    "e" => early = seen,
                    "l" => late = seen,
                    _ => {}
                }
            }
            (early, late)
        };
        let long = "/usr/local/share/fonts/NerdFonts/JetBrainsMonoNerdFont-Regular.ttf";
        assert_eq!(shows(long), (false, true));
        assert_eq!(shows("a.ttf"), (false, false));
    }
}
