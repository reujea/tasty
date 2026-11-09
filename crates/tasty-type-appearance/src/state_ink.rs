//! 채움이 정해진 자리의 글자·글리프 역할과, 생성기가 만들지 못하는 semantic 역할.
//! Mocha·Latte 값이 서로 다른 색 조합이나 color-mix라 Theme 필드 하나로 대응되지 않는다.
//! `-raised`는 raised와 그 아래 바탕, `-strong`은 hover·press와 불투명 틴트까지,
//! `-on-active`는 선택 행과 그 hover·press에서 등급을 유지한다.

use crate::color::HexColor;
use crate::theme::{TINT_FILL_ALPHA, Theme, mix_srgb};

/// color-mix가 섞는 DTCG `primitive.color-black`.
const BLACK: HexColor = crate::hex!("#000000");
/// color-mix가 섞는 DTCG `primitive.color-white`.
const WHITE: HexColor = crate::hex!("#ffffff");
/// 내장 Latte 테마의 `search_match_active_bg`.
const LATTE_ACTIVE_MATCH: HexColor = crate::hex!("#df8e1d80");
/// `semantic.search-match-active-selected-bg`의 Latte 값(yellow 80%).
const LATTE_ACTIVE_MATCH_SELECTED: HexColor = crate::hex!("#df8e1dcc");

impl Theme {
    #[inline]
    fn band(&self, mocha: HexColor, latte: HexColor) -> HexColor {
        if self.is_light { latte } else { mocha }
    }

    /// surface-raised 위 채움. Latte는 neutral-200으로 바꿔 n1100 글자의 대비를 확보한다.
    #[inline]
    pub fn surface_on_raised(&self) -> HexColor {
        if self.is_light {
            self.base
        } else {
            self.surface_hover()
        }
    }

    /// surface-active 행의 모든 글자. 어떤 채움도 세 글자 단계를 함께 통과시키지 못해 하나로 합친다.
    #[inline]
    pub fn text_on_active(&self) -> HexColor {
        self.band(
            self.text_primary(),
            mix_srgb(self.text_primary(), 0.7, BLACK),
        )
    }

    /// surface-active 행의 강조 글리프.
    #[inline]
    pub fn accent_primary_on_active(&self) -> HexColor {
        self.band(
            mix_srgb(self.accent_primary(), 0.65, WHITE),
            mix_srgb(self.accent_primary(), 0.65, BLACK),
        )
    }

    /// raised 바탕 위 경고 글자. 채움과 테두리는 accent-warning을 유지한다.
    #[inline]
    pub fn accent_warning_raised(&self) -> HexColor {
        self.band(
            self.accent_warning(),
            mix_srgb(self.accent_warning(), 0.5, BLACK),
        )
    }

    /// raised 바탕 위 주의 글자. 채움과 테두리는 accent-attention을 유지한다.
    #[inline]
    pub fn accent_attention_raised(&self) -> HexColor {
        self.band(
            self.accent_attention(),
            mix_srgb(self.accent_attention(), 0.55, BLACK),
        )
    }

    /// raised 바탕 위 안내 글자. 채움과 테두리는 accent-info를 유지한다.
    #[inline]
    pub fn accent_info_raised(&self) -> HexColor {
        self.band(
            self.accent_info(),
            mix_srgb(self.accent_info(), 0.55, BLACK),
        )
    }

    /// 약한 글자. Latte는 neutral-900이 bg-app 위에서 4.5:1에 못 미쳐 neutral-1000으로 올린다.
    #[inline]
    pub fn text_muted(&self) -> HexColor {
        if self.is_light {
            self.subtext1
        } else {
            self.subtext0
        }
    }

    /// surface-active 행의 경고 글리프. `semantic.accent-warning-on-active`.
    #[inline]
    pub fn accent_warning_on_active(&self) -> HexColor {
        self.band(
            self.accent_warning(),
            mix_srgb(self.accent_warning(), 0.45, BLACK),
        )
    }

    /// surface-active 행의 성공 글리프. `semantic.accent-success-on-active`.
    #[inline]
    pub fn accent_success_on_active(&self) -> HexColor {
        self.band(
            self.accent_success(),
            mix_srgb(self.accent_success(), 0.55, BLACK),
        )
    }

    /// surface-active 행의 위험 글리프. `semantic.accent-danger-on-active`.
    #[inline]
    pub fn accent_danger_on_active(&self) -> HexColor {
        self.band(
            mix_srgb(self.accent_danger(), 0.6, WHITE),
            mix_srgb(self.accent_danger(), 0.7, BLACK),
        )
    }

    /// surface-active 행의 agent·plugin 글리프. `semantic.accent-agent-on-active`.
    #[inline]
    pub fn accent_agent_on_active(&self) -> HexColor {
        self.band(
            mix_srgb(self.accent_agent(), 0.7, WHITE),
            mix_srgb(self.accent_agent(), 0.7, BLACK),
        )
    }

    /// surface-active 행의 원격 글리프. `semantic.accent-remote-on-active`.
    #[inline]
    pub fn accent_remote_on_active(&self) -> HexColor {
        self.accent_info_on_active()
    }

    /// raised 바탕 위 attached 글자. 고리는 accent-attached를 유지한다.
    #[inline]
    pub fn accent_attached_raised(&self) -> HexColor {
        self.band(
            self.border_attached(),
            mix_srgb(self.border_attached(), 0.55, BLACK),
        )
    }

    /// surface-active 행의 attached 고리. `semantic.accent-attached-on-active`.
    #[inline]
    pub fn accent_attached_on_active(&self) -> HexColor {
        self.band(
            mix_srgb(self.border_attached(), 0.85, WHITE),
            mix_srgb(self.border_attached(), 0.5, BLACK),
        )
    }

    /// 입력 컨트롤의 채움. `semantic.surface-field`.
    #[inline]
    pub fn surface_field(&self) -> HexColor {
        self.band(self.surface_raised(), self.base)
    }

    /// 빈 컨트롤을 알아보게 하는 테두리. `semantic.border-control`.
    #[inline]
    pub fn border_control(&self) -> HexColor {
        self.band(self.overlay0, mix_srgb(self.overlay2, 0.85, self.text))
    }

    /// raised 위 컨트롤 테두리. `semantic.border-control-raised`.
    #[inline]
    pub fn border_control_raised(&self) -> HexColor {
        self.band(self.overlay1, self.subtext0)
    }

    /// 꺼진 스위치의 손잡이. `semantic.control-thumb`.
    #[inline]
    pub fn control_thumb(&self) -> HexColor {
        self.band(self.text, WHITE)
    }

    /// raised 위 꺼진 스위치의 손잡이. `semantic.control-thumb-raised`.
    #[inline]
    pub fn control_thumb_raised(&self) -> HexColor {
        WHITE
    }

    /// 스크롤바 손잡이. `semantic.scrollbar-thumb`.
    #[inline]
    pub fn scrollbar_thumb(&self) -> HexColor {
        self.border_control()
    }

    /// raised 위 스크롤바 손잡이. `semantic.scrollbar-thumb-raised`.
    #[inline]
    pub fn scrollbar_thumb_raised(&self) -> HexColor {
        self.border_control_raised()
    }

    /// 포인터가 올라간 스크롤바 손잡이. `semantic.scrollbar-thumb-hover`.
    #[inline]
    pub fn scrollbar_thumb_hover(&self) -> HexColor {
        self.text_muted_raised()
    }

    /// 선택 행의 hover·press 글자. `semantic.text-on-active-hover`.
    #[inline]
    pub fn text_on_active_hover(&self) -> HexColor {
        self.band(WHITE, mix_srgb(self.text, 0.6, BLACK))
    }

    /// 한발 물러난 글자. `semantic.text-quiet`.
    #[inline]
    pub fn text_quiet(&self) -> HexColor {
        self.band(self.overlay1, self.subtext1)
    }

    /// 물러나는 글리프. `semantic.glyph-dim`.
    #[inline]
    pub fn glyph_dim(&self) -> HexColor {
        self.band(self.overlay0, self.subtext0)
    }

    /// raised 위 물러나는 글리프. `semantic.glyph-dim-raised`.
    #[inline]
    pub fn glyph_dim_raised(&self) -> HexColor {
        self.band(self.overlay1, self.subtext0)
    }

    /// 유휴 상태 점. `semantic.status-idle`.
    #[inline]
    pub fn status_idle(&self) -> HexColor {
        self.band(self.overlay0, self.subtext0)
    }

    /// `semantic.accent-primary-raised`.
    #[inline]
    pub fn accent_primary_raised(&self) -> HexColor {
        self.band(
            self.accent_primary(),
            mix_srgb(self.accent_primary(), 0.7, BLACK),
        )
    }

    /// `semantic.accent-agent-raised`.
    #[inline]
    pub fn accent_agent_raised(&self) -> HexColor {
        self.band(
            self.accent_agent(),
            mix_srgb(self.accent_agent(), 0.8, BLACK),
        )
    }

    /// `semantic.accent-success-raised`.
    #[inline]
    pub fn accent_success_raised(&self) -> HexColor {
        self.accent_success_ink()
    }

    /// `semantic.accent-remote-raised`.
    #[inline]
    pub fn accent_remote_raised(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// `semantic.accent-move-raised`.
    #[inline]
    pub fn accent_move_raised(&self) -> HexColor {
        self.band(
            self.accent_move(),
            mix_srgb(self.accent_move(), 0.55, BLACK),
        )
    }

    /// `semantic.accent-route-raised`.
    #[inline]
    pub fn accent_route_raised(&self) -> HexColor {
        self.accent_attached_raised()
    }

    /// `semantic.accent-primary-strong`.
    #[inline]
    pub fn accent_primary_strong(&self) -> HexColor {
        self.band(
            mix_srgb(self.accent_primary(), 0.85, WHITE),
            mix_srgb(self.accent_primary(), 0.65, BLACK),
        )
    }

    /// `semantic.accent-info-strong`.
    #[inline]
    pub fn accent_info_strong(&self) -> HexColor {
        self.band(
            self.accent_info(),
            mix_srgb(self.accent_info(), 0.45, BLACK),
        )
    }

    /// `semantic.accent-success-strong`.
    #[inline]
    pub fn accent_success_strong(&self) -> HexColor {
        self.band(
            self.accent_success(),
            mix_srgb(self.accent_success(), 0.5, BLACK),
        )
    }

    /// `semantic.accent-warning-strong`.
    #[inline]
    pub fn accent_warning_strong(&self) -> HexColor {
        self.band(
            self.accent_warning(),
            mix_srgb(self.accent_warning(), 0.45, BLACK),
        )
    }

    /// `semantic.accent-attention-strong`.
    #[inline]
    pub fn accent_attention_strong(&self) -> HexColor {
        self.band(
            self.accent_attention(),
            mix_srgb(self.accent_attention(), 0.45, BLACK),
        )
    }

    /// `semantic.accent-danger-strong`.
    #[inline]
    pub fn accent_danger_strong(&self) -> HexColor {
        self.band(
            mix_srgb(self.accent_danger(), 0.75, WHITE),
            mix_srgb(self.accent_danger(), 0.7, BLACK),
        )
    }

    /// `semantic.accent-agent-strong`.
    #[inline]
    pub fn accent_agent_strong(&self) -> HexColor {
        self.band(
            mix_srgb(self.accent_agent(), 0.9, WHITE),
            mix_srgb(self.accent_agent(), 0.65, BLACK),
        )
    }

    /// `semantic.accent-attached-strong`.
    #[inline]
    pub fn accent_attached_strong(&self) -> HexColor {
        self.band(
            self.border_attached(),
            mix_srgb(self.border_attached(), 0.5, BLACK),
        )
    }

    /// `semantic.accent-remote-strong`.
    #[inline]
    pub fn accent_remote_strong(&self) -> HexColor {
        self.accent_info_strong()
    }

    /// `semantic.accent-info-on-active`.
    #[inline]
    pub fn accent_info_on_active(&self) -> HexColor {
        self.band(self.accent_info(), mix_srgb(self.accent_info(), 0.5, BLACK))
    }

    /// `semantic.accent-attention-on-active`.
    #[inline]
    pub fn accent_attention_on_active(&self) -> HexColor {
        self.band(
            mix_srgb(self.accent_attention(), 0.85, WHITE),
            mix_srgb(self.accent_attention(), 0.5, BLACK),
        )
    }

    /// `semantic.accent-move-on-active`.
    #[inline]
    pub fn accent_move_on_active(&self) -> HexColor {
        self.band(
            self.accent_move(),
            mix_srgb(self.accent_move(), 0.45, BLACK),
        )
    }

    /// `semantic.accent-route-on-active`.
    #[inline]
    pub fn accent_route_on_active(&self) -> HexColor {
        self.accent_attached_on_active()
    }

    /// `semantic.accent-success-fill`.
    #[inline]
    pub fn accent_success_fill(&self) -> HexColor {
        self.band(
            self.accent_success(),
            mix_srgb(self.accent_success(), 0.75, BLACK),
        )
    }

    /// `semantic.attention-needs-input-fill`.
    #[inline]
    pub fn attention_needs_input_fill(&self) -> HexColor {
        self.notify_warning_fill()
    }

    /// `semantic.tint-primary-bg`.
    #[inline]
    pub fn tint_primary_bg(&self) -> HexColor {
        tint(self.accent_primary())
    }

    /// `semantic.tint-info-bg`.
    #[inline]
    pub fn tint_info_bg(&self) -> HexColor {
        tint(self.accent_info())
    }

    /// `semantic.tint-success-bg`.
    #[inline]
    pub fn tint_success_bg(&self) -> HexColor {
        tint(self.accent_success())
    }

    /// `semantic.tint-warning-bg`.
    #[inline]
    pub fn tint_warning_bg(&self) -> HexColor {
        tint(self.accent_warning())
    }

    /// `semantic.tint-danger-bg`.
    #[inline]
    pub fn tint_danger_bg(&self) -> HexColor {
        tint(self.accent_danger())
    }

    /// `semantic.tint-agent-bg`.
    #[inline]
    pub fn tint_agent_bg(&self) -> HexColor {
        tint(self.accent_agent())
    }

    /// 터미널 vi 커서 칸의 글자. `semantic.vi-cursor-fg`.
    #[inline]
    pub fn vi_cursor_fg(&self) -> HexColor {
        self.band(self.crust, BLACK)
    }

    /// DAG binding 선처럼 데이터 흐름을 그리는 진한 글리프. `semantic.accent-data-strong`.
    #[inline]
    pub fn accent_data_strong(&self) -> HexColor {
        self.band(
            self.accent_data(),
            mix_srgb(self.accent_data(), 0.85, BLACK),
        )
    }

    /// 포커스 없는 터미널의 현재 검색 매치 글자. `semantic.search-match-active-unfocused-fg`.
    #[inline]
    pub fn search_match_active_unfocused_fg(&self) -> HexColor {
        self.band(self.search_match_active_fg, BLACK)
    }

    /// 선택 위 검색 매치 글자. 포커스와 상관없이 같다. `semantic.search-match-selected-fg`.
    #[inline]
    pub fn search_match_selected_fg(&self) -> HexColor {
        self.band(self.crust, BLACK)
    }

    /// 선택 위 현재 매치 바탕. `semantic.search-match-active-selected-bg`.
    /// 테마 키가 없어 내장 Latte 값일 때만 올린다. 사용자 테마는 자기 현재 매치 바탕을 쓴다.
    #[inline]
    pub fn search_match_active_selected_bg(&self) -> HexColor {
        if self.search_match_active_bg == LATTE_ACTIVE_MATCH {
            LATTE_ACTIVE_MATCH_SELECTED
        } else {
            self.search_match_active_bg
        }
    }

    /// 포커스 받은 surface의 Tasty 소유 바탕. `semantic.surface-bed-focused`.
    /// 토큰 체인대로 터미널 focused 바탕을 읽으므로 사용자 테마의 `[surfaces.terminal]`을 따른다.
    #[inline]
    pub fn surface_bed_focused(&self) -> HexColor {
        self.surface("terminal").focused_bg
    }

    /// 포커스 없는 surface의 Tasty 소유 바탕. `semantic.surface-bed-unfocused`.
    #[inline]
    pub fn surface_bed_unfocused(&self) -> HexColor {
        self.band(self.bg_panel(), self.bg_app())
    }

    /// 포인터가 올라간 비활성 탭. `component.tab-bg-hover`.
    #[inline]
    pub fn tab_bg_hover(&self) -> HexColor {
        mix_srgb(self.bg_panel(), 0.6, self.bg_sidebar())
    }
}

/// 강조색을 틴트 채움 비율로 낮춘다. 비율은 8비트 alpha로 반올림한다.
fn tint(accent: HexColor) -> HexColor {
    accent.with_alpha((TINT_FILL_ALPHA * 255.0).round() as u8)
}
