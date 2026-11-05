//! 버튼과 아이콘 버튼의 크기를 Theme 토큰에서 읽는다.

use tasty_type_appearance::theme::Theme;

/// 컨트롤이 놓인 바탕(시안 `ground`). surface-raised 카드·메뉴·배너 위에서는 `Raised`,
/// 선택 행 위의 채움 없는 버튼은 `Active`를 써서 그 바탕에서 보이는 토큰으로 바꾼다.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Ground {
    #[default]
    Panel,
    Raised,
    Active,
}

/// 디자인 control-height 축. md = 28(기본), sm = 24, lg = 32.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlSize {
    Sm,
    Md,
    Lg,
}

impl ControlSize {
    /// 컨트롤 높이(정사각 IconButton 의 한 변이기도 하다).
    pub fn height(self, theme: &Theme) -> f32 {
        match self {
            // 작은·중간 컨트롤은 공통 높이, 큰 버튼은 전용 높이를 사용한다.
            ControlSize::Sm => theme.item_height_tab.value(),
            ControlSize::Md => theme.item_height_interactive.value(),
            ControlSize::Lg => theme.button_height_lg().value(),
        }
    }

    /// 좌우 inner padding. 디자인 Button: sm=space-sm, md=space-md, lg=space-lg.
    /// (Button 전용 축 — IconButton 은 정사각이라 pad_x 를 쓰지 않는다.)
    pub fn pad_x(self, theme: &Theme) -> f32 {
        match self {
            // 중간 버튼만 전용 패딩 토큰이 있다.
            ControlSize::Sm => theme.spacing_sm.value(),
            ControlSize::Md => theme.button_padding_x().value(),
            ControlSize::Lg => theme.spacing_lg.value(),
        }
    }

    /// 라벨 폰트 크기. 디자인 Button: sm=caption(11), md/lg=body(13).
    /// (Button 전용 축.)
    pub fn font_size(self, theme: &Theme) -> f32 {
        match self {
            ControlSize::Sm => theme.font_size_caption.value(),
            _ => theme.button_font_size().value(),
        }
    }

    /// IconButton 글리프 크기. 디자인 icon scale: sm=14, md/lg=16(기본).
    /// 글리프 크기는 semantic `icon-size-*` 로, 대응 component 토큰이 없다.
    pub fn icon_glyph(self, theme: &Theme) -> f32 {
        match self {
            ControlSize::Sm => theme.icon_glyph_size_sm.value(),
            _ => theme.icon_glyph_size_md.value(),
        }
    }
}
