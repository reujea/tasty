//! CSD 타이틀바의 활성·비활성·닫기 호버 상태 예제.
//! 본체 패널 대신 주어진 Ui 영역에 그린다. 오른쪽 버튼은 닫기가 가장 바깥에 오도록 역순 배치한다.
//! macOS는 네이티브 버튼을 사용하므로 이 예제에서 별도 버튼 행을 그리지 않는다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 본체 `WindowButton` 과 같은 3종.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// 본체 `draw_button_glyph` 전사. 글리프 폭은 titlebar-window-glyph-size이고 `g`는 중심에서 각 변까지(그 절반)다.
pub(super) fn button_glyph(
    p: &egui::Painter,
    theme: &Theme,
    center: egui::Pos2,
    button: WindowButton,
    stroke: egui::Stroke,
) {
    let g = theme.titlebar_window_glyph_size().value() * 0.5;
    match button {
        WindowButton::Minimize => {
            p.line_segment(
                [
                    egui::pos2(center.x - g, center.y),
                    egui::pos2(center.x + g, center.y),
                ],
                stroke,
            );
        }
        WindowButton::Maximize => {
            p.rect_stroke(
                egui::Rect::from_center_size(center, egui::vec2(g * 2.0, g * 2.0)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        WindowButton::Close => {
            p.line_segment(
                [
                    egui::pos2(center.x - g, center.y - g),
                    egui::pos2(center.x + g, center.y + g),
                ],
                stroke,
            );
            p.line_segment(
                [
                    egui::pos2(center.x - g, center.y + g),
                    egui::pos2(center.x + g, center.y - g),
                ],
                stroke,
            );
        }
    }
}

/// 컨트롤 클러스터를 우측에 그리고 strip 폭을 돌려준다 (본체 `draw_window_buttons`).
/// `hovered_close` 면 close 버튼만 hover 상태로 그린다(정적 재현).
fn window_buttons(
    p: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    active: bool,
    hovered_close: bool,
) -> f32 {
    const BUTTONS: [WindowButton; 3] = [
        WindowButton::Minimize,
        WindowButton::Maximize,
        WindowButton::Close,
    ];
    let d = theme.window_button_size.value();
    let edge_pad = theme.spacing_sm.value();
    let gap = theme.spacing_xs.value();
    let n = BUTTONS.len() as f32;
    let strip_w = edge_pad * 2.0 + d * n + gap * (n - 1.0);

    let cy = rect.center().y;
    let mut cx = rect.right() - edge_pad - d * 0.5;
    let step = -(d + gap);

    // Right 측면 — 역순 순회라 close 가 가장 우측.
    for button in BUTTONS.iter().rev() {
        let center = egui::pos2(cx, cy);
        let is_close = *button == WindowButton::Close;
        let hovered = is_close && hovered_close;
        if hovered {
            p.circle_filled(center, d * 0.5, theme.accent_window_close().to_egui());
        }
        let fg = if hovered {
            theme.text_on_window_close()
        } else if active {
            theme.titlebar_fg()
        } else {
            theme.titlebar_fg_inactive()
        };
        button_glyph(
            p,
            theme,
            center,
            *button,
            egui::Stroke::new(theme.border_width.value(), fg.to_egui()),
        );
        cx += step;
    }
    strip_w
}

/// 타이틀바 1줄 — tasty 가 컨트롤을 그리는 경로(Linux DE / Windows).
fn bar(ui: &mut egui::Ui, theme: &Theme, active: bool, hovered_close: bool) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(theme.measure_lg.value(), theme.titlebar_height.value()),
        egui::Sense::hover(),
    );
    let p = ui.painter_at(rect);

    let bg = if active {
        theme.titlebar_bg()
    } else {
        theme.titlebar_bg_inactive()
    };
    p.rect_filled(rect, 0.0, bg.to_egui());

    window_buttons(&p, theme, rect, active, hovered_close);

    p.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.titlebar_border().to_egui_premultiplied(),
        ),
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        spec::cluster(ui, theme, "Active — DE buttons (right)", |ui| {
            bar(ui, theme, true, false)
        });
        spec::cluster(ui, theme, "Inactive (dimmed)", |ui| {
            bar(ui, theme, false, false)
        });
        spec::cluster(ui, theme, "Close hovered", |ui| bar(ui, theme, true, true));
    });

    spec::meta(
        ui,
        theme,
        &[
            ("height", "titlebar-height(36) · 하단 1px titlebar-border"),
            (
                "button",
                "window-button-size(24) 원형 · gap spacing-xs · edge spacing-sm",
            ),
            (
                "glyph",
                "titlebar-window-glyph-size(0.44 × 버튼) · 1px border-width stroke",
            ),
            (
                "close",
                "hover/press 시 accent-window-close 배경 + 반전 글리프",
            ),
            ("drag", "좌 inset · 우 strip 을 뺀 나머지 (버튼과 비중첩)"),
        ],
        &[
            TokenChip::new("titlebar-bg", "bar", theme.titlebar_bg().to_egui()),
            TokenChip::new(
                "titlebar-fg",
                "glyph (active)",
                theme.titlebar_fg().to_egui(),
            ),
            TokenChip::new(
                "titlebar-border",
                "bottom hairline",
                theme.titlebar_border().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "accent-window-close",
                "close hover",
                theme.accent_window_close().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "본체는 데스크톱 환경이 제공한 버튼 목록·순서·위치를 따른다. 오른쪽 버튼은 역순으로 배치해 닫기를 바깥에 둔다. Windows 캡션은 별도 경로로 그리고 macOS는 네이티브 신호등을 유지한다. 이 갤러리는 일반 버튼의 활성·비활성·호버 상태만 보여준다.",
    );
}
