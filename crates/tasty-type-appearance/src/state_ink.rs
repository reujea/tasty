//! 채움이 정해진 자리의 글자·글리프 역할. Mocha는 원래 역할을 쓰고 Latte만 값을 바꾼다.
//! 선택·키보드 활성 행과 raised 바탕에서 텍스트 4.5:1, 글리프 3:1을 맞추기 위한 값이다.

use crate::color::HexColor;
use crate::theme::{Theme, mix_srgb};

/// DTCG `semantic.text-on-active`의 Latte color-mix 비율.
const TEXT_ON_ACTIVE_RATIO: f32 = 0.7;
/// DTCG `semantic.accent-primary-on-active`의 Latte color-mix 비율.
const PRIMARY_ON_ACTIVE_RATIO: f32 = 0.7;
/// DTCG `semantic.accent-warning-raised`의 Latte color-mix 비율.
const WARNING_RAISED_RATIO: f32 = 0.5;
/// DTCG `semantic.accent-attention-raised`의 Latte color-mix 비율.
const ATTENTION_RAISED_RATIO: f32 = 0.55;
/// DTCG `semantic.accent-info-raised`의 Latte color-mix 비율.
const INFO_RAISED_RATIO: f32 = 0.55;
/// DTCG `semantic.accent-danger-on-active`의 Mocha color-mix 비율.
const DANGER_ON_ACTIVE_RATIO: f32 = 0.85;
/// DTCG `semantic.accent-agent-on-active`의 Latte color-mix 비율.
const AGENT_ON_ACTIVE_RATIO: f32 = 0.8;
/// DTCG `semantic.accent-attached-raised`의 Latte color-mix 비율.
const ATTACHED_RAISED_RATIO: f32 = 0.55;
/// 위 역할이 섞는 DTCG `primitive.color-black`.
const BLACK: HexColor = crate::hex!("#000000");
/// `semantic.accent-danger-on-active`가 Mocha에서 섞는 DTCG `primitive.color-white`.
const WHITE: HexColor = crate::hex!("#ffffff");

impl Theme {
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
        if self.is_light {
            mix_srgb(self.text_primary(), TEXT_ON_ACTIVE_RATIO, BLACK)
        } else {
            self.text_primary()
        }
    }

    /// surface-active 행의 강조 글리프.
    #[inline]
    pub fn accent_primary_on_active(&self) -> HexColor {
        if self.is_light {
            mix_srgb(self.accent_primary(), PRIMARY_ON_ACTIVE_RATIO, BLACK)
        } else {
            self.accent_primary()
        }
    }

    /// raised 바탕 위 경고 글자. 채움과 테두리는 accent-warning을 유지한다.
    #[inline]
    pub fn accent_warning_raised(&self) -> HexColor {
        if self.is_light {
            mix_srgb(self.accent_warning(), WARNING_RAISED_RATIO, BLACK)
        } else {
            self.accent_warning()
        }
    }

    /// raised 바탕 위 주의 글자. 채움과 테두리는 accent-attention을 유지한다.
    #[inline]
    pub fn accent_attention_raised(&self) -> HexColor {
        if self.is_light {
            mix_srgb(self.accent_attention(), ATTENTION_RAISED_RATIO, BLACK)
        } else {
            self.accent_attention()
        }
    }

    /// raised 바탕 위 안내 글자. 채움과 테두리는 accent-info를 유지한다.
    #[inline]
    pub fn accent_info_raised(&self) -> HexColor {
        if self.is_light {
            mix_srgb(self.accent_info(), INFO_RAISED_RATIO, BLACK)
        } else {
            self.accent_info()
        }
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
        self.accent_warning_raised()
    }

    /// surface-active 행의 성공 글리프. `semantic.accent-success-on-active`.
    #[inline]
    pub fn accent_success_on_active(&self) -> HexColor {
        self.accent_success_ink()
    }

    /// surface-active 행의 위험 글리프. `semantic.accent-danger-on-active`.
    #[inline]
    pub fn accent_danger_on_active(&self) -> HexColor {
        if self.is_light {
            self.accent_danger_raised()
        } else {
            mix_srgb(self.accent_danger(), DANGER_ON_ACTIVE_RATIO, WHITE)
        }
    }

    /// surface-active 행의 agent·plugin 글리프. `semantic.accent-agent-on-active`.
    #[inline]
    pub fn accent_agent_on_active(&self) -> HexColor {
        if self.is_light {
            mix_srgb(self.accent_agent(), AGENT_ON_ACTIVE_RATIO, BLACK)
        } else {
            self.accent_agent()
        }
    }

    /// surface-active 행의 원격 글리프. `semantic.accent-remote-on-active`.
    #[inline]
    pub fn accent_remote_on_active(&self) -> HexColor {
        self.accent_info_raised()
    }

    /// raised 바탕 위 attached 글자. 고리는 accent-attached를 유지한다.
    #[inline]
    pub fn accent_attached_raised(&self) -> HexColor {
        if self.is_light {
            mix_srgb(self.border_attached(), ATTACHED_RAISED_RATIO, BLACK)
        } else {
            self.border_attached()
        }
    }

    /// surface-active 행의 attached 고리. `semantic.accent-attached-on-active`.
    #[inline]
    pub fn accent_attached_on_active(&self) -> HexColor {
        self.accent_attached_raised()
    }
}
