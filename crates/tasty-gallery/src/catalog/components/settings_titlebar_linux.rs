//! Linux 설정창의 앱 제목 줄 예제. 시안 `settings-titlebar-linux` 카드처럼 창 위 테두리·제목 줄·L1 밴드를 잘라
//! Mocha·Latte, 활성·비활성, 닫기 평소·hover(누름과 같음)·키보드 포커스로 보인다.
//! 본체는 메인 타이틀바 렌더러(`draw_titlebar_view`)에 닫기 버튼 하나와 키보드 포커스 표시를 넘겨 그린다.

use tasty_type_appearance::theme::Theme;

use super::titlebar::{WindowButton, button_glyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

/// 닫기 버튼의 고정 상태. hover와 누름은 같은 모양이라 한 칸으로 보인다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Close {
    Rest,
    Hover,
    Focus,
}

/// 팔레트 하나를 현재 UI 배율로 다시 만든다. 한 카드에 Mocha와 Latte를 나란히 놓는다.
fn palette(base: &Theme, zoom: f32) -> Theme {
    Theme::with_colors_and_zoom(base.to_colors(), base.is_light, zoom)
}

/// 제목 줄 한 줄. 닫기 버튼은 오른쪽 끝에서 space-sm 안쪽, 36 줄 전체의 세로 가운데다.
fn bar(ui: &mut egui::Ui, th: &Theme, active: bool, close: Close, tip: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), th.titlebar_height.value()),
        egui::Sense::hover(),
    );
    let p = ui.painter_at(rect);
    let bg = if active {
        th.titlebar_bg()
    } else {
        th.titlebar_bg_inactive()
    };
    p.rect_filled(rect, 0.0, bg.to_egui());

    let d = th.titlebar_window_button_size().value();
    let center = egui::pos2(
        rect.right() - th.spacing_sm.value() - d * 0.5,
        rect.center().y,
    );
    let fg = match close {
        Close::Hover => {
            p.circle_filled(center, d * 0.5, th.titlebar_close_hover_bg().to_egui());
            th.titlebar_close_hover_fg()
        }
        Close::Rest | Close::Focus if active => th.titlebar_fg(),
        Close::Rest | Close::Focus => th.titlebar_fg_inactive(),
    };
    button_glyph(
        &p,
        th,
        center,
        WindowButton::Close,
        egui::Stroke::new(th.border_width.value(), fg.to_egui()),
    );
    // 본체 렌더러와 같이 버튼 둘레에서 focus-ring-offset만큼 띄운 focus-ring-width 링.
    if close == Close::Focus {
        let ring = th.focus_ring_width.value();
        let offset = tasty_design_tokens::generated::semantic::FOCUS_RING_OFFSET.value();
        ui.painter().circle_stroke(
            center,
            d * 0.5 + offset + ring * 0.5,
            egui::Stroke::new(ring, th.border_focus().to_egui()),
        );
    }
    p.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        egui::Stroke::new(
            th.border_width.value(),
            th.titlebar_border().to_egui_premultiplied(),
        ),
    );
    // hover와 키보드 포커스에서는 Close 툴팁이 버튼 아래, 오른쪽 변을 버튼에 맞춰 열려 있다.
    if close != Close::Rest {
        let button = egui::Rect::from_center_size(center, egui::vec2(d, d));
        tasty_ui_widgets::Tooltip::new(crate::i18n::t("titlebar.close"))
            .id_source(("settings_titlebar_linux_tip", tip))
            .below_end(ui.ctx(), th, button)
            .show(ui, th, button);
    }
}

/// 창 위 테두리·제목 줄·L1 밴드를 자른 한 칸.
fn frame(ui: &mut egui::Ui, th: &Theme, active: bool, close: Close, tip: &str) {
    let width = th.measure_lg.value();
    egui::Frame::new()
        .fill(th.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.border_strong().to_egui(),
        ))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            bar(ui, th, active, close, tip);
            let band = th.settings_l1_height() - th.border_width;
            super::settings::l1_band(ui, th, band);
            kit::hsep(ui, th);
        });
}

const FRAMES: &[(bool, Close, &str)] = &[
    (true, Close::Rest, "active · close at rest"),
    (
        true,
        Close::Hover,
        "active · close under hover and press · Close tooltip",
    ),
    (
        true,
        Close::Focus,
        "active · close with keyboard focus · Close tooltip",
    ),
    (false, Close::Rest, "inactive · close at rest"),
    (
        false,
        Close::Hover,
        "inactive · close under hover and press · Close tooltip",
    ),
];

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let themes = [
        (
            "Mocha",
            palette(&tasty_themes::mocha_fallback(), theme.ui_zoom),
        ),
        (
            "Latte",
            palette(&crate::host_shell::latte_theme(), theme.ui_zoom),
        ),
    ];
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (name, th) in &themes {
            for (active, close, label) in FRAMES {
                let tip = format!("{name} · {label}");
                spec::cluster(ui, theme, &tip, |ui| frame(ui, th, *active, *close, &tip));
            }
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("window width", "settings-window-width"),
            (
                "window height, inner and minimum",
                "settings-window-csd-height (736)",
            ),
            (
                "window height with OS decorations (no row)",
                "settings-window-height (700)",
            ),
            (
                "title bar height",
                "titlebar-height · 1px titlebar-border inside",
            ),
            ("title bar padding-x", "space-sm"),
            ("close button, round", "titlebar-window-button-size"),
            ("close X span, 1px stroke", "titlebar-window-glyph-size"),
            (
                "keyboard focus",
                "focus-ring-width ring · border-focus · focus-ring-offset from the button",
            ),
            ("L1 band below", "settings-l1-height"),
            ("title", "none — the L1 band keeps its Settings label"),
            ("buttons", "close only, right end"),
            (
                "drag",
                "the rest of the row moves the window · double click does nothing",
            ),
            (
                "close tooltip",
                "Tooltip, titlebar.close, below the button, right edges aligned, on hover and keyboard focus",
            ),
        ],
        &[
            TokenChip::new(
                "titlebar-bg",
                "active ground",
                theme.titlebar_bg().to_egui(),
            ),
            TokenChip::new(
                "titlebar-bg-inactive",
                "inactive ground",
                theme.titlebar_bg_inactive().to_egui(),
            ),
            TokenChip::new(
                "titlebar-border",
                "1px line above the L1 band",
                theme.titlebar_border().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "titlebar-fg",
                "close glyph at rest, active",
                theme.titlebar_fg().to_egui(),
            ),
            TokenChip::new(
                "titlebar-fg-inactive",
                "close glyph at rest, inactive",
                theme.titlebar_fg_inactive().to_egui(),
            ),
            TokenChip::new(
                "titlebar-close-hover-bg",
                "close fill on hover and press",
                theme.titlebar_close_hover_bg().to_egui(),
            ),
            TokenChip::new(
                "titlebar-close-hover-fg",
                "close glyph on hover and press",
                theme.titlebar_close_hover_fg().to_egui(),
            ),
            TokenChip::new(
                "border-focus",
                "keyboard focus ring",
                theme.border_focus().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "The row uses the main window's Linux title bar with one close button. Close drops the draft, like Cancel and the settings shortcut. When the compositor forces its own decorations the row is not drawn and the window keeps settings-window-height. macOS and Windows keep the OS title bar and the 1100 × 700 window.",
    );
    spec::dont(
        ui,
        theme,
        "Don't put Settings in the title bar as well: the L1 band already says it one row below.",
    );
}
