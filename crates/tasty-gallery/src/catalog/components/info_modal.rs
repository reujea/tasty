//! `info-modal` specimen — 부팅 안내/에러 알림 모달 (Overlays).
//!
//! 본체 `src/adapters/ui/info_modal.rs::draw_info_modal` 의 구조 전사.
//! 큐(`DialogState.info_modal_queue`) head 를 보여주고 [OK]/Enter/Escape 로 pop
//! 하는 popup 이며, 큐가 빌 때까지 다음 메시지가 이어서 뜬다.
//!
//! - **frame**: 폭 `DEFAULT_WIDTH` 440, 높이는 body 길이로 산출해 140..360 clamp.
//!   제목은 큐 head 의 `title` 이 **타이틀바**에 실린다(`PopupDef.title_fn`).
//! - **body**: 콘텐츠 영역 좌우 8 / 상하 4 inset(`ContentInset::INSET` 과 동일)
//!   안에 `font_size_body` `text_primary` 산문 한 덩어리.
//! - **footer**: `bottom_up(RIGHT)` 로 바닥에 붙이고 `spacing_xs` 여백 뒤
//!   `right_to_left` — **[OK] 가 가장 오른쪽**, 추가 버튼이 그 왼쪽에 붙는다.
//!   추가 버튼은 OS 설정 패널로 보내는 안내(macOS Full Disk Access)처럼
//!   "안내만으로 끝나지 않는" 모달에서만 생긴다.
//!
//! **토큰 이관 1건**: 본체는 버튼을 egui 기본 `ui.button` 으로 그린다 → specimen 은
//! 공용 `tasty_ui_widgets::Button`(`docs/architecture/ui-widgets-crate.md#무엇을-공용-위젯으로` 목표 상태).

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant};

use crate::catalog::popup_frame::{self, ContentInset, TitleButtons};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 본체 `info_modal.rs` 의 `DEFAULT_WIDTH`.
const WIDTH: LogicalPx = LogicalPx(440.0);
/// 본체 `info_modal.rs` 의 `MIN_HEIGHT`.
const MIN_HEIGHT: LogicalPx = LogicalPx(140.0);
/// 본체 `info_modal.rs` 의 `MAX_HEIGHT`.
const MAX_HEIGHT: LogicalPx = LogicalPx(360.0);

/// 본체 `info_modal_sizer` 와 같은 규칙으로 높이를 낸다 —
/// 60자/줄 가정 · 줄높이 = body 폰트 × 1.5 · 하단 버튼 영역 48 · clamp.
fn sizer_height(theme: &Theme, body: &str) -> LogicalPx {
    let approx_lines = (body.chars().count() as f32 / 60.0).ceil().max(2.0);
    let line_h = theme.font_size_body * 1.5;
    let footer_h = theme.item_height_interactive + theme.spacing_lg + theme.spacing_xs;
    (popup_frame::TITLE_BAR_HEIGHT
        + popup_frame::CONTENT_MARGIN * 2.0
        + line_h * approx_lines
        + footer_h)
        .clamp(MIN_HEIGHT, MAX_HEIGHT)
}

/// 모달 1장. `extra` 는 [OK] 왼쪽에 붙는 추가 버튼 라벨.
fn modal(ui: &mut egui::Ui, theme: &Theme, title: &str, body: &str, extra: Option<&str>) {
    let h = sizer_height(theme, body);
    popup_frame::draw(
        ui,
        theme,
        title,
        WIDTH,
        h,
        ContentInset::INSET,
        TitleButtons::CLOSE,
        // 뷰포트를 점유하는 centered 표면 → modal 그림자(ADR-0254).
        Some(theme.shadow_modal()),
        |ui| {
            ui.label(
                egui::RichText::new(body)
                    .size(theme.font_size_body.value())
                    .color(theme.text_primary().to_egui()),
            );
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                ui.add_space(theme.spacing_xs.value());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    Button::new("OK")
                        .variant(ButtonVariant::Primary)
                        .show(ui, theme);
                    if let Some(label) = extra {
                        Button::new(label)
                            .variant(ButtonVariant::Secondary)
                            .show(ui, theme);
                    }
                });
            });
        },
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "Notice (OK only)", |ui| {
            modal(
                ui,
                theme,
                "Theme fallback",
                "The configured theme could not be read, so the built-in Mocha theme is in use. \
                 Your theme file is left untouched.",
                None,
            )
        });
        spec::cluster(ui, theme, "With follow-up action", |ui| {
            modal(
                ui,
                theme,
                "Full Disk Access required",
                "Tasty needs Full Disk Access to read this folder. Open System Settings, grant \
                 access, then reopen the folder.",
                Some("Open System Settings"),
            )
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "440px · height 140..360 (body 길이로 산출)"),
            ("title", "큐 head 의 title — 타이틀바에 실린다"),
            (
                "body",
                "font-size-body · text-primary · 좌우 8 / 상하 4 inset",
            ),
            ("footer", "bottom-up RIGHT · [OK] 가 가장 오른쪽"),
        ],
        &[
            TokenChip::new(
                "surface-raised",
                "popup frame",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "surface-hover",
                "title bar",
                theme.surface_hover().to_egui(),
            ),
            TokenChip::new("text-primary", "body", theme.text_primary().to_egui()),
        ],
    );

    spec::note(
        ui,
        theme,
        "큐 모델이다 — 여러 건이 쌓이면 [OK] 마다 다음 메시지로 넘어가고, 마지막을 확인해야 \
         popup 이 닫힌다. X 로 닫아도 head 를 pop 해 남은 안내가 유실되지 않는다. \
         추가 버튼(설정 패널 열기 등)은 모달을 닫지 않는다 — 안내를 다시 읽을 수 있어야 한다.",
    );
}
