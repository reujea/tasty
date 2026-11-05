//! 정사각 아이콘 버튼. 투명한 ghost와 채운 solid 형태를 제공한다.
//! 선택·호버·누름 상태를 즉시 표시하며 아이콘은 호출자의 IconPainter로 그린다.

use tasty_type_appearance::theme::Theme;

use crate::control::ControlSize;

/// 디자인 IconButton variant. `ghost`(기본) / `solid`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconButtonVariant {
    Ghost,
    Solid,
}

/// 글리프 painter: 위젯이 계산한 `rect`(정사각, 중앙) + `color`(상태별 해소)로
/// 아이콘을 그린다. 본체: `|ui, rect, c| icons::CLOSE.image(sz, c).paint_at(ui, rect)`.
pub type IconPainter<'a> = &'a dyn Fn(&mut egui::Ui, egui::Rect, egui::Color32);

/// IconButton 빌더.
pub struct IconButton {
    variant: IconButtonVariant,
    size: ControlSize,
    active: bool,
    enabled: bool,
    ground: crate::Ground,
}

impl Default for IconButton {
    fn default() -> Self {
        Self {
            variant: IconButtonVariant::Ghost,
            size: ControlSize::Md,
            active: false,
            enabled: true,
            ground: crate::Ground::Panel,
        }
    }
}

impl IconButton {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn variant(mut self, variant: IconButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// 지속 선택 상태(engaged tool). accent fg + active overlay.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 놓인 바탕. 선택 행 위에서는 `Ground::Active`를 넘긴다.
    pub fn ground(mut self, ground: crate::Ground) -> Self {
        self.ground = ground;
        self
    }

    /// 그리고 클릭 응답을 반환한다. `paint_icon` 으로 글리프를 주입.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &Theme,
        paint_icon: IconPainter<'_>,
    ) -> egui::Response {
        let side = self.size.height(theme);
        let sense = if self.enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), sense);
        let radius = theme.icon_button_radius().value();

        // solid 배경과 테두리는 대응 컴포넌트 토큰이 없어 의미별 토큰을 사용한다.
        // disabled solid는 disabled 상자 role을 읽는다.
        if self.variant == IconButtonVariant::Solid {
            let (fill, edge) = if self.enabled {
                (theme.surface_raised(), theme.border_default())
            } else {
                (theme.state_disabled_fill(), theme.state_disabled_border())
            };
            ui.painter().rect(
                rect,
                radius,
                fill.to_egui(),
                egui::Stroke::new(theme.border_width.value(), edge.to_egui()),
                egui::StrokeKind::Inside,
            );
        }

        let color = paint_icon_button_state(
            ui.painter(),
            theme,
            rect,
            IconButtonState {
                variant: self.variant,
                enabled: self.enabled,
                active: self.active,
                hovered: resp.hovered(),
                pressed: resp.is_pointer_button_down_on(),
                ground: self.ground,
            },
        );

        let glyph = self.size.icon_glyph(theme);
        let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
        paint_icon(ui, icon_rect, color);
        resp
    }
}

/// IconButton 한 칸의 상태. painter만 쓰고 hit-test를 직접 하는 호출부(popup 타이틀바)가
/// 판정 결과를 넘긴다.
#[derive(Clone, Copy, Debug)]
pub struct IconButtonState {
    pub variant: IconButtonVariant,
    pub enabled: bool,
    pub active: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub ground: crate::Ground,
}

impl IconButtonState {
    /// 사용 가능한 ghost 버튼.
    pub fn ghost(hovered: bool, pressed: bool) -> Self {
        Self {
            variant: IconButtonVariant::Ghost,
            enabled: true,
            active: false,
            hovered,
            pressed,
            ground: crate::Ground::Panel,
        }
    }
}

/// IconButton의 active·hover 배경을 `rect`에 칠하고 글리프 색을 반환한다. 위젯과 painter
/// 호출부가 같은 규칙을 쓴다. solid 바탕은 위젯만 그린다.
pub fn paint_icon_button_state(
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    state: IconButtonState,
) -> egui::Color32 {
    let radius = theme.icon_button_radius().value();
    // disabled는 active·hover 배경을 그리지 않는다.
    if state.enabled && (state.active || state.pressed) {
        painter.rect_filled(
            rect,
            radius,
            theme.icon_button_bg_active().to_egui_premultiplied(),
        );
    } else if state.enabled && state.hovered {
        painter.rect_filled(
            rect,
            radius,
            theme.icon_button_overlay_hover().to_egui_premultiplied(),
        );
    }

    // disabled 글리프는 opacity 없이 disabled ink를 쓴다.
    if !state.enabled {
        theme.state_disabled_fg().to_egui()
    } else if state.ground == crate::Ground::Active {
        theme.text_on_active().to_egui()
    } else if state.active {
        // 켜진 도구는 active overlay 위 글리프다.
        match state.ground {
            crate::Ground::Raised => theme.accent_primary_raised().to_egui(),
            _ => theme.accent_primary().to_egui(),
        }
    } else if state.hovered || state.pressed {
        theme.text_secondary().to_egui()
    } else {
        theme.icon_button_fg().to_egui()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 상태 하나를 칠하고 (칠한 배경들, 반환한 글리프 색)을 돌려준다.
    fn paint(theme: &Theme, state: IconButtonState) -> (Vec<egui::Color32>, egui::Color32) {
        let ctx = egui::Context::default();
        let mut glyph = None;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("icon_button_state_test"),
            ));
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(24.0, 24.0));
            glyph = Some(paint_icon_button_state(&painter, theme, rect, state));
        });
        let fills = output
            .shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Rect(r) => Some(r.fill),
                _ => None,
            })
            .collect();
        (fills, glyph.expect("closure ran"))
    }

    /// 상태표: 배경은 enabled 일 때만, active·pressed 가 hover 보다 먼저다.
    /// 글리프는 disabled → active → (solid 또는 hover) → 기본 순서이고 pressed 는 글리프를 바꾸지 않는다.
    #[test]
    fn every_state_combination_paints_the_tabled_background_and_glyph() {
        let theme = tasty_themes::mocha_fallback();
        let bg_active = theme.icon_button_bg_active().to_egui_premultiplied();
        let bg_hover = theme.icon_button_overlay_hover().to_egui_premultiplied();
        let disabled = theme.state_disabled_fg().to_egui();
        let accent = theme.accent_primary().to_egui();
        let fg_hover = theme.icon_button_fg_hover().to_egui();
        let fg = theme.icon_button_fg().to_egui();
        // 같은 값이 섞이면 표가 순서를 가르지 못한다.
        assert_ne!(bg_active, bg_hover);
        let glyphs = [disabled, accent, fg_hover, fg];
        for (i, a) in glyphs.iter().enumerate() {
            for b in &glyphs[i + 1..] {
                assert_ne!(a, b, "glyph colours must differ: {glyphs:?}");
            }
        }

        let mut seen = 0;
        for variant in [IconButtonVariant::Ghost, IconButtonVariant::Solid] {
            for enabled in [true, false] {
                for hovered in [false, true] {
                    for pressed in [false, true] {
                        for active in [false, true] {
                            let state = IconButtonState {
                                variant,
                                enabled,
                                active,
                                hovered,
                                pressed,
                                ground: crate::Ground::Panel,
                            };
                            let want_bg = match (enabled, active || pressed, hovered) {
                                (false, _, _) => vec![],
                                (true, true, _) => vec![bg_active],
                                (true, false, true) => vec![bg_hover],
                                (true, false, false) => vec![],
                            };
                            let want_glyph = if !enabled {
                                disabled
                            } else if active {
                                accent
                            } else if variant == IconButtonVariant::Solid || hovered {
                                theme.text_secondary().to_egui()
                            } else {
                                fg
                            };
                            let (bg, glyph) = paint(&theme, state);
                            assert_eq!(bg, want_bg, "background for {state:?}");
                            assert_eq!(glyph, want_glyph, "glyph for {state:?}");
                            seen += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(seen, 32);
    }

    /// 표의 우선순위를 이름으로 고정한 행. 조합 표와 같은 규칙을 다른 모양으로 한 번 더 적는다.
    #[test]
    fn named_rows_pin_the_priority_between_states() {
        let theme = tasty_themes::mocha_fallback();
        let bg_active = theme.icon_button_bg_active().to_egui_premultiplied();
        let all_on = |variant, enabled| IconButtonState {
            ground: crate::control::Ground::Panel,
            variant,
            enabled,
            active: true,
            hovered: true,
            pressed: true,
        };

        // disabled 는 다른 상태가 모두 켜져도 배경이 없고 disabled ink 다.
        let (bg, glyph) = paint(&theme, all_on(IconButtonVariant::Ghost, false));
        assert!(bg.is_empty(), "disabled painted {bg:?}");
        assert_eq!(glyph, theme.state_disabled_fg().to_egui());

        // active 가 hover 보다 먼저다 — 배경도 글리프도.
        let (bg, glyph) = paint(
            &theme,
            IconButtonState {
                ground: crate::control::Ground::Panel,
                pressed: false,
                ..all_on(IconButtonVariant::Ghost, true)
            },
        );
        assert_eq!(bg, vec![bg_active]);
        assert_eq!(glyph, theme.accent_primary().to_egui());

        // pressed 는 overlay-active 를 칠하지만 글리프는 hover 규칙을 따른다.
        let (bg, glyph) = paint(&theme, IconButtonState::ghost(true, true));
        assert_eq!(bg, vec![bg_active]);
        assert_eq!(glyph, theme.text_secondary().to_egui());

        // solid 는 hover 없이도 fg-hover 글리프다.
        let (bg, glyph) = paint(
            &theme,
            IconButtonState {
                ground: crate::control::Ground::Panel,
                variant: IconButtonVariant::Solid,
                ..IconButtonState::ghost(false, false)
            },
        );
        assert!(bg.is_empty());
        assert_eq!(glyph, theme.text_secondary().to_egui());
    }
}
