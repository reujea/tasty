//! `quit-modal` specimen — 종료 확인 모달 창 (Overlays).
//!
//! 본체 `src/view/quit.rs::QuitView::render` 의 구조 전사. 이 모달은 popup 이
//! 아니라 **독립 winit 창**(400×200, non-resizable)이고 `close_behavior = "ask"`
//! 경로에서만 뜬다.
//!
//! 세로 구성:
//! - **본문**(CentralPanel, `vertical_centered`) — `spacing_xl` 여백 뒤 제목,
//!   `spacing_md` 뒤 안내문, `spacing_sm` 뒤 설정 힌트(작게·흐리게).
//! - **푸터**(bottom panel, 높이 52 = `item_height_interactive` + `spacing_md`×2) —
//!   좌우 `spacing_lg` 여백 안에서 두 버튼이 가용 폭을 **정확히 반씩** 나눠 갖고
//!   사이 간격은 `spacing_sm`. 버튼 높이는 `item_height_interactive`.
//!
//! **토큰 이관 2건** (구조·치수 보존):
//! - 본체 리터럴 `52.0` / `28.0` / `- 32.0` / `- 4.0` → 각각
//!   `item_height_interactive + spacing_md*2` / `item_height_interactive` /
//!   `spacing_lg*2` / `spacing_sm` 에서 도출. 값은 전부 동일하다.
//! - 본체 `egui::Button::new` + `ui.heading`/`ui.label` → 공용
//!   `tasty_ui_widgets::Button` + Theme 폰트 토큰
//!   (`docs/architecture/ui-widgets-crate.md#무엇을-공용-위젯으로` 목표 상태).

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant};

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 본체 `open_quit_modal` 의 창 크기.
const WINDOW_W: LogicalPx = LogicalPx(400.0);
const WINDOW_H: LogicalPx = LogicalPx(200.0);

fn window(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(WINDOW_W.value(), WINDOW_H.value()),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, theme.corner_radius.value(), theme.bg_app().to_egui());
    ui.painter().rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );

    // 푸터 높이 — 본체 exact_height(52) = 28 + 12 + 12.
    let footer_h = theme.item_height_interactive.value() + theme.spacing_md.value() * 2.0;
    let body_rect =
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - footer_h));
    let footer_rect = egui::Rect::from_min_max(egui::pos2(rect.min.x, body_rect.max.y), rect.max);

    // ── 본문 (중앙 정렬 3단) ──
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
    body.vertical_centered(|ui| {
        ui.add_space(theme.spacing_xl.value());
        ui.label(
            egui::RichText::new("Close Tasty")
                .size(theme.font_size_max.value())
                .strong()
                .color(theme.text_primary().to_egui()),
        );
        ui.add_space(theme.spacing_md.value());
        ui.label(
            egui::RichText::new("Would you like to quit or minimize to background?")
                .size(theme.font_size_body.value())
                .color(theme.text_primary().to_egui()),
        );
        ui.add_space(theme.spacing_sm.value());
        ui.label(
            egui::RichText::new("You can change the default behavior in Settings > General.")
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
    });

    // ── 푸터 (좌우 spacing_lg · 반반 분할 · 사이 spacing_sm) ──
    let side = theme.spacing_lg.value();
    let gap = theme.spacing_sm.value();
    let inner_w = footer_rect.width() - side * 2.0;
    let button_w = (inner_w - gap) * 0.5;
    let btn_y = footer_rect.min.y + theme.spacing_md.value();
    let btn_h = theme.item_height_interactive.value();

    let mut place = |x: f32, label: &str, variant: ButtonVariant| {
        let slot = egui::Rect::from_min_size(egui::pos2(x, btn_y), egui::vec2(button_w, btn_h));
        let mut cell = ui.new_child(egui::UiBuilder::new().max_rect(slot));
        Button::new(label)
            .variant(variant)
            .block(true)
            .show(&mut cell, theme);
    };
    place(footer_rect.min.x + side, "Quit", ButtonVariant::Primary);
    place(
        footer_rect.min.x + side + button_w + gap,
        "Minimize",
        ButtonVariant::Secondary,
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "Quit confirmation", |ui| window(ui, theme));
    });

    spec::meta(
        ui,
        theme,
        &[
            ("window", "400×200 · non-resizable · 독립 winit 창"),
            (
                "body",
                "spacing-xl → 제목 → spacing-md → 안내 → spacing-sm → 힌트",
            ),
            (
                "footer",
                "높이 28+12×2 · 좌우 spacing-lg · 반반 · 사이 spacing-sm",
            ),
            ("hint", "font-size-caption text-muted"),
        ],
        &[
            TokenChip::new("bg-app", "window", theme.bg_app().to_egui()),
            TokenChip::new(
                "text-primary",
                "title · message",
                theme.text_primary().to_egui(),
            ),
            TokenChip::new("text-muted", "settings hint", theme.text_muted().to_egui()),
        ],
    );

    spec::note(
        ui,
        theme,
        "`close_behavior = \"ask\"` 일 때만 뜬다. 이미 열려 있는 상태에서 다시 종료를 요청하면 \
         묻지 않고 즉시 종료한다 — 같은 확인을 두 번 쌓지 않는다. 두 버튼은 각각 종료와 \
         백그라운드 최소화로 갈리고, 기본 동작은 Settings › General 에서 바꾼다.",
    );
}
