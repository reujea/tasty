//! 키보드 가져오기·내보내기 알림 블록의 잉크 선택. 본체 설정과 갤러리가 같은 함수를 읽는다.
//! 블록 그리기는 아이콘 타입이 달라 각자 두고, 어느 토큰을 어느 칸에 쓰는지만 여기서 정한다.

use tasty_type_appearance::theme::Theme;

/// 알림 블록의 색. `tint`는 채움·테두리, `glyph`는 글리프, `title`은 제목과 개수의 잉크다.
/// 디자인 `IeBlockG`의 `tone`(제목)과 `glyphTone`(글리프)이 raised 위 danger tint에서 갈린다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoticeInk {
    pub tint: egui::Color32,
    pub glyph: egui::Color32,
    pub title: egui::Color32,
}

impl NoticeInk {
    /// 글리프와 제목이 같은 잉크인 블록.
    fn same(tint: egui::Color32, ink: egui::Color32) -> Self {
        Self {
            tint,
            glyph: ink,
            title: ink,
        }
    }
}

/// 내보내기 실패 블록의 색 — 디자인 `IeExportFailG`. raised 카드 위 danger tint라 제목은
/// accent-danger-strong, 글리프는 accent-danger-raised다.
pub fn export_failure_ink(th: &Theme) -> NoticeInk {
    NoticeInk {
        tint: th.accent_danger().to_egui(),
        glyph: th.accent_danger_raised().to_egui(),
        title: th.accent_danger_strong().to_egui(),
    }
}

/// 내보내기 실패 블록의 본문 잉크 — 디자인 `IeExportFailG` 본문의 text-on-active.
pub fn notice_body_ink(th: &Theme) -> egui::Color32 {
    th.text_on_active().to_egui()
}

/// 가져오기 파싱 실패 블록의 색 — 디자인 `IeParseFailG`. 제목과 글리프 모두 accent-danger-strong이다
/// (`glyphTone`을 따로 주지 않아 `tone`을 따른다).
pub fn parse_failure_ink(th: &Theme) -> NoticeInk {
    NoticeInk::same(
        th.accent_danger().to_egui(),
        th.accent_danger_strong().to_egui(),
    )
}

/// 가져오기 파싱 실패 블록의 본문 잉크 — 가져오기 detail의 text-secondary.
pub fn parse_failure_body_ink(th: &Theme) -> egui::Color32 {
    th.text_secondary().to_egui()
}

/// 번들 경고 블록의 색 — 디자인 `IeBundleNoticesG`. 제목·개수·글리프 모두 accent-warning-raised다.
pub fn bundle_notices_ink(th: &Theme) -> NoticeInk {
    NoticeInk::same(
        th.accent_warning().to_egui(),
        th.accent_warning_raised().to_egui(),
    )
}

/// 번들 경고 한 줄의 잉크 — 디자인 `IeBlockG` 본문 칸의 text-on-active.
pub fn bundle_notice_line_ink(th: &Theme) -> egui::Color32 {
    th.text_on_active().to_egui()
}
