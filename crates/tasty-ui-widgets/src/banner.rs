//! 배너 셸과 inset 배치 기하. 본체 BannerManager와 갤러리 예제가 같은 함수를 호출한다.
//! 큐·TTL·레이어 순서는 본체 정책이라 여기 두지 않는다. 규칙은 docs/design/systems/banner.md를 따른다.

use tasty_type_appearance::theme::Theme;

/// Theme의 배너 배경·테두리·모서리·그림자로 카드를 그린다.
/// opacity로 어둡게 표시하며 실제 카드 영역을 반환해 입력 판정에 쓴다.
/// 내용은 배너 문맥([`crate::banner_surface`])에서 그려 배너 위 Secondary 버튼이 한 단계 위 상자를 쓴다.
pub fn banner_shell(
    ui: &mut egui::Ui,
    theme: &Theme,
    opacity: f32,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    let dim = |c: egui::Color32| c.gamma_multiply(opacity);
    let mut shadow = theme.shadow_popover().to_egui();
    shadow.color = shadow.color.gamma_multiply(opacity);
    egui::Frame::new()
        .fill(dim(theme.banner_bg().to_egui()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            dim(theme.banner_border().to_egui()),
        ))
        .corner_radius(theme.banner_radius().value())
        .shadow(shadow)
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_md.value() as i8,
            theme.spacing_sm.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            crate::banner_surface(ui, content);
        })
        .response
        .rect
}

/// 배너가 놓인 스코프 폭이 `banner_narrow_below`보다 좁으면 액션을 본문 아래 줄로 내린다.
/// 스코프 크기가 바뀔 때마다 다시 판정하며 내용 길이로 임계값을 바꾸지 않는다.
pub fn banner_is_narrow(scope_width: f32, theme: &Theme) -> bool {
    scope_width < theme.banner_narrow_below().value()
}

/// [글리프 | 글 열 | 버튼 · 닫기] 행의 치수. 닫기(×)는 버튼이 아니라 닫기 표지라 좁아도 오른쪽 위에 남는다.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ActionRow {
    pub(crate) row_w: f32,
    pub(crate) glyph: f32,
    /// 글리프 ↔ 글 열 ↔ 닫기 사이, 좁을 때 글 ↔ 버튼 줄 사이 간격(`banner-gap`).
    pub(crate) gap: f32,
    pub(crate) button: egui::Vec2,
    /// 닫기 버튼 한 변.
    pub(crate) close: f32,
    /// 넓을 때 버튼 ↔ 닫기 간격(space-xs).
    pub(crate) pair_gap: f32,
    pub(crate) narrow: bool,
}

/// 행 안의 위치(행 왼쪽 위 기준)와 행 높이.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ActionRowPlace {
    pub(crate) row_h: f32,
    pub(crate) button: egui::Pos2,
    pub(crate) close: egui::Pos2,
}

impl ActionRow {
    /// 넓을 때 버튼과 닫기를 함께 세로 가운데에 맞춘 묶음의 크기.
    fn pair(&self) -> egui::Vec2 {
        egui::vec2(
            self.button.x + self.pair_gap + self.close,
            self.button.y.max(self.close),
        )
    }

    /// 글 열이 쓸 폭. 좁으면 버튼이 다음 줄이라 닫기 칸만 비운다.
    pub(crate) fn text_width(&self) -> f32 {
        let right = if self.narrow {
            self.close
        } else {
            self.pair().x
        };
        (self.row_w - self.glyph - 2.0 * self.gap - right).max(0.0)
    }

    /// `glyph_h`는 글리프 칸 높이(오프셋 포함), `text_h`는 글 열 높이다.
    /// 좁으면 버튼을 글 열 아래 `gap` 뒤, 본문 왼쪽 가장자리에 둔다. 닫기는 늘 오른쪽 위다.
    pub(crate) fn place(&self, glyph_h: f32, text_h: f32) -> ActionRowPlace {
        let close_x = self.row_w - self.close;
        if self.narrow {
            let button_top = text_h + self.gap;
            ActionRowPlace {
                row_h: glyph_h.max(button_top + self.button.y).max(self.close),
                button: egui::pos2(self.glyph + self.gap, button_top),
                close: egui::pos2(close_x, 0.0),
            }
        } else {
            let pair = self.pair();
            ActionRowPlace {
                row_h: glyph_h.max(text_h).max(pair.y),
                button: egui::pos2(self.row_w - pair.x, (pair.y - self.button.y) * 0.5),
                close: egui::pos2(close_x, (pair.y - self.close) * 0.5),
            }
        }
    }
}

/// 행 안 `min`에서 시작하는 `size` 크기의 자식 Ui. 그 안의 위젯을 세로 가운데에 맞춘다.
pub(crate) fn action_slot(ui: &mut egui::Ui, min: egui::Pos2, size: egui::Vec2) -> egui::Ui {
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(min, size))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    )
}

/// 배너 닫기(×) — Ghost sm IconButton.
pub(crate) fn banner_close_button(ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
    crate::icon_button::IconButton::new()
        .variant(crate::icon_button::IconButtonVariant::Ghost)
        .size(crate::control::ControlSize::Sm)
        .show(ui, theme, &|ui, r, c| {
            tasty_icons::CLOSE.image(r.height(), c).paint_at(ui, r)
        })
}

/// inset 배치에서 배너를 둘 영역. 스코프 콘텐츠 rect의 위·좌·우를 `banner_margin`만큼 줄인다.
/// 높이는 배너가 정하므로 아래 경계는 스코프 바닥까지 열어 둔다.
pub fn inset_banner_zone(scope: egui::Rect, theme: &Theme) -> egui::Rect {
    let margin = theme.banner_margin().value();
    egui::Rect::from_min_max(
        egui::pos2(scope.left() + margin, scope.top() + margin),
        egui::pos2(scope.right() - margin, scope.bottom()),
    )
}

/// inset 배치에서 네이티브 콘텐츠가 차지할 rect. 배너 카드 아래 `banner_inset_gap` 뒤에서 시작한다.
/// 배너가 스코프보다 커도 높이가 음수가 되지 않도록 바닥에 맞춘다.
pub fn inset_content_rect(scope: egui::Rect, banner: egui::Rect, theme: &Theme) -> egui::Rect {
    let top = (banner.bottom() + theme.banner_inset_gap().value()).min(scope.bottom());
    egui::Rect::from_min_max(egui::pos2(scope.left(), top), scope.max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    #[test]
    fn narrow_starts_just_below_the_token_width() {
        let t = theme();
        let below = t.banner_narrow_below().value();
        assert!(banner_is_narrow(below - 1.0, &t));
        assert!(!banner_is_narrow(below, &t));
    }

    fn row(narrow: bool) -> ActionRow {
        ActionRow {
            row_w: 300.0,
            glyph: 16.0,
            gap: 8.0,
            button: egui::vec2(96.0, 24.0),
            close: 24.0,
            pair_gap: 4.0,
            narrow,
        }
    }

    #[test]
    fn a_narrow_action_row_moves_only_the_button_under_the_text_column() {
        let narrow = row(true);
        // 글리프 16 + gap 8 + 글 열 + gap 8 + 닫기 24
        assert_eq!(narrow.text_width(), 244.0);
        let at = narrow.place(17.0, 40.0);
        assert_eq!(at.button, egui::pos2(24.0, 48.0));
        assert_eq!(at.close, egui::pos2(276.0, 0.0));
        assert_eq!(at.row_h, 72.0);
    }

    #[test]
    fn a_wide_action_row_keeps_the_button_and_close_together_on_the_right() {
        let wide = row(false);
        // 묶음 96 + 4 + 24 = 124
        assert_eq!(wide.text_width(), 300.0 - 16.0 - 16.0 - 124.0);
        let at = wide.place(17.0, 16.0);
        assert_eq!(at.button, egui::pos2(176.0, 0.0));
        assert_eq!(at.close, egui::pos2(276.0, 0.0));
        assert_eq!(at.row_h, 24.0);
    }

    #[test]
    fn inset_leaves_margin_on_all_four_sides() {
        let t = theme();
        let scope = egui::Rect::from_min_size(egui::pos2(0.0, 24.0), egui::vec2(400.0, 300.0));
        let zone = inset_banner_zone(scope, &t);
        let m = t.banner_margin().value();
        assert_eq!(zone.left(), scope.left() + m);
        assert_eq!(zone.right(), scope.right() - m);
        assert_eq!(zone.top(), scope.top() + m);

        let card = egui::Rect::from_min_size(zone.min, egui::vec2(zone.width(), 60.0));
        let content = inset_content_rect(scope, card, &t);
        assert_eq!(content.top(), card.bottom() + t.banner_inset_gap().value());
        assert_eq!(content.left(), scope.left());
        assert_eq!(content.right(), scope.right());
        assert_eq!(content.bottom(), scope.bottom());
    }

    #[test]
    fn inset_content_never_inverts() {
        let t = theme();
        let scope = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 40.0));
        let card = egui::Rect::from_min_size(egui::pos2(8.0, 8.0), egui::vec2(184.0, 80.0));
        let content = inset_content_rect(scope, card, &t);
        assert_eq!(content.height(), 0.0);
    }
}
