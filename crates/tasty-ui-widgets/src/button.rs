//! Theme 토큰으로 채움·외곽선·투명 버튼을 그린다.
//! 호버와 누름 상태는 애니메이션 없이 바로 반영한다.
//! 별도 굵은 글꼴을 등록하지 않으므로 글꼴 굵기 대신 색으로 강조를 구분한다.
//! disabled는 opacity를 곱하지 않는다. 모든 variant가 같은 중립 상자와 disabled ink로 그려진다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::control::ControlSize;
use crate::icon_button::IconPainter;

/// 디자인 Button variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Ghost,
    Danger,
    Agent,
}

/// 배너 셸 안이라는 문맥을 기록하는 egui 임시 데이터 키. 값은 중첩 깊이다.
fn banner_surface_id() -> egui::Id {
    egui::Id::new("tasty_ui_widgets::button::banner_surface")
}

/// `content`를 배너 셸 문맥(시안 `data-surface="banner"`)에서 그린다.
/// 이 안의 Secondary 버튼은 배너 배경보다 한 단계 위의 채움과 테두리
/// (`banner_button_bg`·`banner_button_border`)를 쓴다. `banner_shell`이 모든 배너에 적용한다.
pub fn banner_surface<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    banner_surface_ctx(&ctx, || content(ui))
}

/// `content`가 `ctx`에 그리는 동안 배너 셸 문맥을 연다. 루트 `Ui` 없이 한 프레임 전체를
/// 감쌀 때 쓴다. plugin SDK의 egui-mesh 배너가 배너 전용 `Context`의 매 프레임을 이것으로
/// 감싸므로 plugin 작성자는 따로 감싸지 않는다. 끝나면 이전 깊이로 돌아가 같은 plugin의
/// surface·popup에는 새지 않는다.
pub fn banner_surface_ctx<R>(ctx: &egui::Context, content: impl FnOnce() -> R) -> R {
    let id = banner_surface_id();
    let prev = ctx.data(|d| d.get_temp::<u32>(id)).unwrap_or(0);
    ctx.data_mut(|d| d.insert_temp(id, prev + 1));
    let out = content();
    ctx.data_mut(|d| d.insert_temp(id, prev));
    out
}

/// `ctx`에 지금 배너 셸 문맥이 열려 있는지.
pub fn in_banner_surface(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<u32>(banner_surface_id()))
        .unwrap_or(0)
        > 0
}

/// 지금 그리는 위치가 배너 셸 문맥 안인지.
fn on_banner_surface(ui: &egui::Ui) -> bool {
    in_banner_surface(ui.ctx())
}

/// Button 빌더.
pub struct Button<'a> {
    label: &'a str,
    variant: ButtonVariant,
    size: ControlSize,
    enabled: bool,
    /// 컨테이너 폭을 채운다(디자인 `block`).
    block: bool,
    /// 라벨 앞 leading 아이콘(디자인 `leadingIcon`). icon-size-md, fg 색으로 그려짐.
    leading_icon: Option<IconPainter<'a>>,
    /// leading 아이콘 칸의 변 길이. `None` 이면 icon-size-md.
    leading_icon_size: Option<LogicalPx>,
    /// 라벨 뒤 trailing 아이콘(디자인 `trailingIcon`). icon-size-md, fg 색으로 그려짐.
    trailing_icon: Option<IconPainter<'a>>,
    /// 활성 상태의 글자·아이콘을 accent-danger 로 그린다(디자인 secondary 버튼의 `color: accent-danger`).
    danger_ink: bool,
    /// 놓인 바탕. raised 위 ghost는 글자가 button-ghost-raised-fg다. 다른 variant는 같다.
    ground: crate::Ground,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            variant: ButtonVariant::Primary,
            size: ControlSize::Md,
            enabled: true,
            block: false,
            leading_icon: None,
            leading_icon_size: None,
            trailing_icon: None,
            danger_ink: false,
            ground: crate::Ground::Panel,
        }
    }

    /// 놓인 바탕(시안 `ground`). surface-raised 카드 위 ghost는 `Ground::Raised`.
    pub fn ground(mut self, ground: crate::Ground) -> Self {
        self.ground = ground;
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn block(mut self, block: bool) -> Self {
        self.block = block;
        self
    }

    /// 라벨 앞 leading 아이콘(디자인 `leadingIcon`).
    pub fn leading_icon(mut self, icon: IconPainter<'a>) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    /// leading 아이콘 칸의 변 길이. 시안이 leading 자리에 icon-size-md 가 아닌 요소
    /// (예: Spinner 14)를 둘 때 그 크기로 칸을 잡는다.
    pub fn leading_icon_size(mut self, size: LogicalPx) -> Self {
        self.leading_icon_size = Some(size);
        self
    }

    /// 라벨 뒤 trailing 아이콘(디자인 `trailingIcon`).
    pub fn trailing_icon(mut self, icon: IconPainter<'a>) -> Self {
        self.trailing_icon = Some(icon);
        self
    }

    /// 채움과 테두리는 variant 그대로 두고 글자와 아이콘만 accent-danger 로 그린다.
    /// disabled 에서는 다른 버튼과 같은 disabled ink 를 쓴다.
    pub fn danger_ink(mut self, danger_ink: bool) -> Self {
        self.danger_ink = danger_ink;
        self
    }

    /// variant 가 정한 글자색에 `danger_ink` 를 적용한다.
    fn label_ink(&self, theme: &Theme, fg: egui::Color32) -> egui::Color32 {
        if self.enabled && self.danger_ink {
            match self.ground {
                crate::Ground::Active => fg,
                _ => theme.accent_danger_raised().to_egui(),
            }
        } else {
            fg
        }
    }

    /// 채움 없는 버튼과 Secondary의 아이콘. 채움 버튼의 아이콘은 라벨과 같은 잉크다.
    fn icon_ink(&self, theme: &Theme, label: egui::Color32) -> egui::Color32 {
        if !self.enabled || self.danger_ink {
            return label;
        }
        match self.variant {
            ButtonVariant::Ghost if self.ground == crate::Ground::Active => {
                theme.text_on_active().to_egui()
            }
            ButtonVariant::Ghost | ButtonVariant::Secondary => theme.text_secondary().to_egui(),
            _ => label,
        }
    }

    /// block 이 아닐 때 차지할 폭. 버튼을 줄바꿈할지 정하려고 그리기 전에 잰다.
    pub fn desired_width(&self, ui: &egui::Ui, theme: &Theme) -> LogicalPx {
        let font_id = egui::FontId::proportional(self.size.font_size(theme));
        let galley =
            ui.painter()
                .layout_no_wrap(self.label.to_owned(), font_id, egui::Color32::PLACEHOLDER);
        self.content_width(theme, LogicalPx(galley.rect.size().x))
    }

    fn content_width(&self, theme: &Theme, label_w: LogicalPx) -> LogicalPx {
        let icon_glyph = theme.icon_glyph_size_md;
        let leading_glyph = self.leading_icon_size.unwrap_or(icon_glyph);
        let gap = theme.button_gap();
        let none = LogicalPx(0.0);
        let icons_w = (if self.leading_icon.is_some() {
            leading_glyph + gap
        } else {
            none
        }) + (if self.trailing_icon.is_some() {
            icon_glyph + gap
        } else {
            none
        });
        label_w + icons_w + LogicalPx(self.size.pad_x(theme)) * 2.0
    }

    pub fn show(self, ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
        let height = self.size.height(theme);
        let radius = theme.button_radius().value();
        let bw = theme.border_width.value();
        let icon_glyph = theme.icon_glyph_size_md.value();
        let leading_glyph = self
            .leading_icon_size
            .map_or(icon_glyph, |size| size.value());
        let gap = theme.button_gap().value();
        let has_leading = self.leading_icon.is_some();
        let has_trailing = self.trailing_icon.is_some();

        // 글자를 배치한 뒤 상태별 색을 지정한다.
        let font_id = egui::FontId::proportional(self.size.font_size(theme));
        let galley =
            ui.painter()
                .layout_no_wrap(self.label.to_owned(), font_id, egui::Color32::PLACEHOLDER);

        let content_w = self
            .content_width(theme, LogicalPx(galley.rect.size().x))
            .value();
        let desired_w = if self.block {
            ui.available_width().max(content_w)
        } else {
            content_w
        };
        let sense = if self.enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(desired_w, height), sense);

        let pressed = resp.is_pointer_button_down_on();
        let (fill, border, fg) = if !self.enabled {
            // accent 채움은 빠지고 모든 variant가 같은 중립 상자를 쓴다. Ghost는 상자가 없다.
            let fg = theme.button_disabled_fg().to_egui();
            match self.variant {
                ButtonVariant::Ghost => (None, None, fg),
                _ => (
                    Some(theme.button_disabled_bg().to_egui()),
                    Some(theme.button_disabled_border().to_egui()),
                    fg,
                ),
            }
        } else {
            match self.variant {
                ButtonVariant::Primary => (
                    Some(theme.button_primary_bg().to_egui()),
                    None,
                    theme.button_primary_fg().to_egui(),
                ),
                ButtonVariant::Agent => (
                    Some(theme.button_agent_bg().to_egui()),
                    None,
                    theme.button_agent_fg().to_egui(),
                ),
                ButtonVariant::Danger => (
                    Some(theme.button_danger_bg().to_egui()),
                    None,
                    theme.button_danger_fg().to_egui(),
                ),
                // 배너 위에서는 채움과 테두리가 배너 배경보다 한 단계 위이며 hover에도 테두리가 그대로다.
                ButtonVariant::Secondary
                    if on_banner_surface(ui) || self.ground == crate::Ground::Raised =>
                {
                    let edge = if resp.hovered() || pressed {
                        theme.border_frame()
                    } else {
                        theme.banner_button_border()
                    };
                    let label = if pressed {
                        theme.text_on_active_hover()
                    } else {
                        theme.text_primary()
                    };
                    (
                        Some(theme.banner_button_bg().to_egui()),
                        Some(edge.to_egui()),
                        label.to_egui(),
                    )
                }
                ButtonVariant::Secondary => {
                    let (edge, label) = if pressed {
                        (theme.border_default(), theme.text_on_active())
                    } else if resp.hovered() {
                        (
                            theme.button_secondary_border_hover(),
                            theme.text_on_active(),
                        )
                    } else {
                        (theme.button_secondary_border(), theme.button_fg())
                    };
                    (
                        Some(theme.button_secondary_bg().to_egui()),
                        Some(edge.to_egui()),
                        label.to_egui(),
                    )
                }
                ButtonVariant::Ghost => {
                    let f = ghost_label(theme, self.ground, resp.hovered(), pressed);
                    (None, None, f.to_egui())
                }
            }
        };

        if let Some(f) = fill {
            ui.painter().rect_filled(rect, radius, f);
        }
        if let Some(b) = border {
            ui.painter().rect_stroke(
                rect,
                radius,
                egui::Stroke::new(bw, b),
                egui::StrokeKind::Inside,
            );
        }
        if self.enabled {
            if pressed {
                ui.painter().rect_filled(
                    rect,
                    radius,
                    theme.button_overlay_active().to_egui_premultiplied(),
                );
            } else if resp.hovered() {
                ui.painter().rect_filled(
                    rect,
                    radius,
                    theme.button_overlay_hover().to_egui_premultiplied(),
                );
            }
        }

        let label_w = galley.rect.size().x;
        let group_w = label_w
            + (if has_leading {
                leading_glyph + gap
            } else {
                0.0
            })
            + (if has_trailing { icon_glyph + gap } else { 0.0 });
        let mut x = rect.center().x - group_w * 0.5;
        let cy = rect.center().y;
        let fg_col = self.label_ink(theme, fg);
        let icon_col = self.icon_ink(theme, fg_col);

        if let Some(paint) = self.leading_icon {
            let irect = egui::Rect::from_center_size(
                egui::pos2(x + leading_glyph * 0.5, cy),
                egui::vec2(leading_glyph, leading_glyph),
            );
            paint(ui, irect, icon_col);
            x += leading_glyph + gap;
        }
        let text_pos = egui::pos2(x, cy - galley.rect.size().y * 0.5);
        ui.painter().galley(text_pos, galley, fg_col);
        x += label_w;
        if let Some(paint) = self.trailing_icon {
            x += gap;
            let irect = egui::Rect::from_center_size(
                egui::pos2(x + icon_glyph * 0.5, cy),
                egui::vec2(icon_glyph, icon_glyph),
            );
            paint(ui, irect, icon_col);
        }

        resp
    }
}

/// ghost 라벨. 채움이 없어 놓인 바탕과 상태로 칸을 고른다.
/// `Panel`의 hover·press는 crust와 mantle 칸을 읽으며 base+hover에서도 등급을 지킨다.
fn ghost_label(
    theme: &Theme,
    ground: crate::Ground,
    hovered: bool,
    pressed: bool,
) -> tasty_type_appearance::color::HexColor {
    match (ground, hovered || pressed) {
        (crate::Ground::Panel, false) => theme.button_ghost_fg(),
        (crate::Ground::Panel, true) => theme.text_secondary_raised(),
        (crate::Ground::Raised, false) => theme.button_ghost_raised_fg(),
        (crate::Ground::Raised, true) => theme.button_ghost_raised_fg_hover(),
        (crate::Ground::Active, false) => theme.text_on_active(),
        (crate::Ground::Active, true) => theme.text_on_active_hover(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_surface_marks_only_its_own_scope_and_nests() {
        let ctx = egui::Context::default();
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!on_banner_surface(ui));
                banner_surface(ui, |ui| {
                    assert!(on_banner_surface(ui));
                    banner_surface(ui, |ui| assert!(on_banner_surface(ui)));
                    assert!(on_banner_surface(ui));
                });
                assert!(!on_banner_surface(ui));
            });
        });
    }

    #[test]
    fn danger_ink_paints_the_label_danger_only_while_enabled() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let fg = theme.button_fg().to_egui();
        let danger = theme.accent_danger().to_egui();
        assert_ne!(fg, danger);
        let button = |enabled: bool, danger_ink: bool| {
            Button::new("Uninstall")
                .variant(ButtonVariant::Secondary)
                .enabled(enabled)
                .danger_ink(danger_ink)
        };
        assert_eq!(button(true, false).label_ink(&theme, fg), fg);
        assert_eq!(button(true, true).label_ink(&theme, fg), danger);
        assert_eq!(button(false, true).label_ink(&theme, fg), fg);
    }
}
