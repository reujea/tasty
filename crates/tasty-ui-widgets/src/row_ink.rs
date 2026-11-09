//! 행의 상태 우선순위와 상태별 글자·글리프 토큰.
//! 순서는 disabled, 선택+press, 선택+hover, 선택, press, hover, rest다.
//! 메타의 hover 칸은 부모 바탕에 따라 달라지므로 호출자가 [`RowParent`]를 넘긴다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

/// 행이 놓인 바탕. crust는 mantle과 같은 칸을 읽는다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowParent {
    Mantle,
    Base,
}

impl RowParent {
    /// 포커스 바탕 위 행이 읽는 칸. `term`의 hover·press·놓기 칸은 base와 같고,
    /// 더 어두운 `term-unfocused`는 같은 상태에서 mantle처럼 `-raised` 글자를 읽는다.
    pub fn bed(focused: bool) -> Self {
        if focused { Self::Base } else { Self::Mantle }
    }
}

/// 겹칠 수 있는 행 상태를 우선순위로 정리한 값 하나.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowState {
    Disabled,
    SelectedPress,
    SelectedHover,
    Selected,
    Press,
    Hover,
    /// 쉬는 행 위에 놓기 대상 틴트가 덮였다. 틴트는 호출자가 칠한다.
    DropTarget,
    Rest,
}

impl RowState {
    pub fn of(enabled: bool, selected: bool, hovered: bool, pressed: bool) -> Self {
        match (enabled, selected, pressed, hovered) {
            (false, ..) => Self::Disabled,
            (_, true, true, _) => Self::SelectedPress,
            (_, true, _, true) => Self::SelectedHover,
            (_, true, ..) => Self::Selected,
            (_, _, true, _) => Self::Press,
            (_, _, _, true) => Self::Hover,
            _ => Self::Rest,
        }
    }

    /// 쉬는 행이 놓기 대상 틴트 아래에 있으면 그 칸을 읽는다. 선택·hover·press는 그대로다.
    pub fn under_drop(self, under: bool) -> Self {
        if under && self == Self::Rest {
            Self::DropTarget
        } else {
            self
        }
    }

    /// egui 응답에서 활성 행의 상태를 읽는다.
    pub fn from_response(selected: bool, resp: &egui::Response) -> Self {
        Self::of(
            true,
            selected,
            resp.hovered(),
            resp.is_pointer_button_down_on(),
        )
    }

    pub fn is_selected(self) -> bool {
        matches!(
            self,
            Self::SelectedPress | Self::SelectedHover | Self::Selected
        )
    }

    /// 행의 채움. 선택 행의 hover·press는 surface-active 위에 overlay를 합성한 불투명 색이다.
    pub fn fill(self, theme: &Theme) -> egui::Color32 {
        let active = theme.surface_active().to_egui();
        match self {
            Self::Disabled | Self::DropTarget | Self::Rest => egui::Color32::TRANSPARENT,
            Self::Selected => active,
            Self::SelectedHover => over(theme.overlay_hover().to_egui_premultiplied(), active),
            Self::SelectedPress => over(theme.overlay_active().to_egui_premultiplied(), active),
            Self::Hover => theme.overlay_hover().to_egui_premultiplied(),
            Self::Press => theme.overlay_active().to_egui_premultiplied(),
        }
    }

    /// 행의 채움을 칠한다.
    pub fn paint_fill(self, painter: &egui::Painter, rect: egui::Rect, radius: f32, theme: &Theme) {
        let fill = self.fill(theme);
        if fill != egui::Color32::TRANSPARENT {
            painter.rect_filled(rect, radius, fill);
        }
    }

    /// 행 라벨. `rest`는 그 행의 평상 토큰이다.
    /// 라벨의 hover 칸은 부모 바탕과 무관하게 같고, 놓기 칸만 부모 바탕을 따른다.
    pub fn label(self, theme: &Theme, parent: RowParent, rest: HexColor) -> HexColor {
        match self {
            Self::Disabled => theme.state_disabled_fg(),
            Self::SelectedPress | Self::SelectedHover => theme.text_on_active_hover(),
            Self::Selected => theme.text_on_active(),
            Self::Press | Self::Hover => theme.text_secondary_raised(),
            Self::DropTarget if parent == RowParent::Mantle => theme.text_secondary_raised(),
            Self::DropTarget | Self::Rest => rest,
        }
    }

    /// 메타·설명처럼 라벨보다 약한 글자.
    pub fn meta(self, theme: &Theme, parent: RowParent) -> HexColor {
        match self {
            Self::Disabled => theme.state_disabled_fg(),
            Self::SelectedPress | Self::SelectedHover => theme.text_on_active_hover(),
            Self::Selected => theme.text_on_active(),
            Self::Press => theme.text_muted_raised(),
            Self::Hover | Self::DropTarget if parent == RowParent::Base => theme.text_muted(),
            Self::Hover | Self::DropTarget => theme.text_muted_raised(),
            Self::Rest => theme.text_muted(),
        }
    }

    /// 모양이 뜻을 담는 글리프(chevron, 닫기, 파일 종류).
    pub fn glyph(self, theme: &Theme) -> HexColor {
        match self {
            Self::Disabled => theme.state_disabled_fg(),
            Self::SelectedPress | Self::SelectedHover | Self::Selected => theme.text_on_active(),
            Self::Press | Self::Hover | Self::DropTarget | Self::Rest => theme.text_secondary(),
        }
    }

    /// 색이 뜻을 담는 글리프. 선택 행은 `on_active`, hover·press는 `raised`, 평상은 `rest`다.
    pub fn hue(
        self,
        theme: &Theme,
        rest: HexColor,
        raised: HexColor,
        on_active: HexColor,
    ) -> HexColor {
        match self {
            Self::Disabled => theme.state_disabled_fg(),
            Self::SelectedPress | Self::SelectedHover | Self::Selected => on_active,
            Self::Press | Self::Hover => raised,
            Self::DropTarget | Self::Rest => rest,
        }
    }
}

/// premultiplied 색을 불투명 바탕 위에 합성한다. GPU 블렌딩과 같은 8비트 source-over다.
#[allow(clippy::disallowed_methods)] // reason: Theme 색의 합성 결과이며 새 색을 정의하지 않는다.
fn over(top: egui::Color32, ground: egui::Color32) -> egui::Color32 {
    let keep = 255 - u32::from(top.a());
    let channel = |t: u8, g: u8| (u32::from(t) + (u32::from(g) * keep + 127) / 255).min(255) as u8;
    egui::Color32::from_rgb(
        channel(top.r(), ground.r()),
        channel(top.g(), ground.g()),
        channel(top.b(), ground.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_states_resolve_in_the_contract_order() {
        assert_eq!(RowState::of(false, true, true, true), RowState::Disabled);
        assert_eq!(
            RowState::of(true, true, true, true),
            RowState::SelectedPress
        );
        assert_eq!(
            RowState::of(true, true, true, false),
            RowState::SelectedHover
        );
        assert_eq!(RowState::of(true, true, false, false), RowState::Selected);
        assert_eq!(RowState::of(true, false, true, true), RowState::Press);
        assert_eq!(RowState::of(true, false, true, false), RowState::Hover);
        assert_eq!(RowState::of(true, false, false, false), RowState::Rest);
    }

    #[test]
    fn only_a_resting_row_reads_the_drop_cell() {
        assert_eq!(RowState::Rest.under_drop(true), RowState::DropTarget);
        assert_eq!(RowState::Rest.under_drop(false), RowState::Rest);
        for state in [RowState::Hover, RowState::Press, RowState::Selected] {
            assert_eq!(state.under_drop(true), state);
        }
    }

    /// RULES 6절의 `term+hover`·`term-unfocused+hover`·`drop-target@term*` 칸.
    #[test]
    fn the_unfocused_bed_reads_the_raised_inks_in_the_design_cells() {
        // Mocha는 `-raised`가 원래 값과 같아 칸이 바뀌어도 값으로 드러나지 않는다.
        let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
        let (partial, is_light) = file.to_partial();
        let mut colors = tasty_themes::mocha_fallback_colors();
        colors.apply_partial(&partial);
        let theme = Theme::with_colors(colors, is_light.unwrap_or(true));
        assert_ne!(theme.text_muted(), theme.text_muted_raised());
        assert_ne!(theme.text_secondary(), theme.text_secondary_raised());
        let (on, off) = (RowParent::bed(true), RowParent::bed(false));
        let rest = theme.text_secondary();
        assert_eq!(RowState::Hover.meta(&theme, on), theme.text_muted());
        assert_eq!(RowState::Hover.meta(&theme, off), theme.text_muted_raised());
        assert_eq!(RowState::Press.meta(&theme, on), theme.text_muted_raised());
        assert_eq!(RowState::Press.meta(&theme, off), theme.text_muted_raised());
        assert_eq!(RowState::DropTarget.meta(&theme, on), theme.text_muted());
        assert_eq!(
            RowState::DropTarget.meta(&theme, off),
            theme.text_muted_raised()
        );
        assert_eq!(RowState::DropTarget.label(&theme, on, rest), rest);
        assert_eq!(
            RowState::DropTarget.label(&theme, off, rest),
            theme.text_secondary_raised()
        );
    }
}
