//! Linux 설정창의 앱 제목 줄 예제. 시안 `settings-titlebar-linux` 카드처럼 창 위 테두리·제목 줄·L1 밴드를 잘라
//! Mocha·Latte, 활성·비활성, 닫기 평소·hover(누름과 같음)·키보드 포커스로 보인다.
//! 본체는 메인 타이틀바 렌더러(`draw_titlebar_view`)에 닫기 버튼 하나와 키보드 포커스 표시를 넘겨 그린다.

use tasty_type_appearance::theme::Theme;

use super::settings::{L1Layout, l1_band};
use super::titlebar::{WindowButton, button_glyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

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
/// 위 두 모서리는 창 테두리 안쪽 반경(`corner`)으로 둥글다.
fn bar(ui: &mut egui::Ui, th: &Theme, active: bool, close: Close, tip: &str, corner: u8) {
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
    let top = egui::CornerRadius {
        nw: corner,
        ne: corner,
        sw: 0,
        se: 0,
    };
    p.rect_filled(rect, top, bg.to_egui());

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
    // epaint 원 획은 반지름 바깥으로 그리므로 반지름은 링의 안쪽 변이다.
    if close == Close::Focus {
        let ring = th.focus_ring_width.value();
        let offset = th.focus_ring_offset.value();
        ui.painter().circle_stroke(
            center,
            d * 0.5 + offset,
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
        tasty_ui_widgets::Tooltip::new(t("titlebar.close"))
            .id_source(("settings_titlebar_linux_tip", tip))
            .below_end(ui.ctx(), th, button)
            .show(ui, th, button);
    }
}

/// 창 위 테두리·제목 줄·L1 밴드를 자른 한 칸. 정본 칸처럼 높이는 border-width + titlebar-height +
/// settings-l1-height이고, 제품 창(settings-window-width)의 오른쪽 끝에 맞춰 잘라 닫기 버튼이 늘 보인다.
/// 칸 폭(`crop_w`)은 정본 칸처럼 한 줄 전체이고 창보다 좁으면 Settings 라벨 쪽이 왼쪽으로 잘린다.
fn frame(ui: &mut egui::Ui, th: &Theme, crop_w: f32, active: bool, close: Close, tip: &str) {
    let bw = th.border_width.value();
    let window_w = th.settings_window_width().value();
    let crop_h = bw + th.titlebar_height.value() + th.settings_l1_height.value();
    let (crop, _) = ui.allocate_exact_size(
        egui::vec2(crop_w.min(window_w), crop_h),
        egui::Sense::hover(),
    );
    // 창은 칸 아래로 이어진다. 칸 밖의 아래 테두리는 잘린다.
    let window = egui::Rect::from_min_size(
        egui::pos2(crop.right() - window_w, crop.top()),
        egui::vec2(window_w, th.settings_window_csd_height().value()),
    );
    let radius = th.corner_radius.value();
    let painter = ui.painter_at(crop);
    painter.rect_filled(window, radius, th.bg_panel().to_egui());

    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(window.shrink(bw))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    inner.set_clip_rect(crop);
    inner.spacing_mut().item_spacing = egui::Vec2::ZERO;
    let inner_radius = (radius - bw).max(0.0) as u8;
    bar(&mut inner, th, active, close, tip, inner_radius);
    // L1 띠 44는 아래 1px 선을 포함한다. 선은 띠의 mantle 위에 합성한다(본체·정본과 같다).
    let tabs = [
        t("settings.tab.general"),
        t("settings.tab.terminal"),
        t("settings.tab.appearance"),
        t("settings.tab.keybindings"),
        t("settings.tab.file_handler"),
        t("settings.tab.misc"),
        t("settings.tab.plugin"),
    ];
    l1_band(
        &mut inner,
        th,
        th.settings_l1_height - th.border_width,
        &L1Layout {
            title: t("settings.window.title"),
            tabs: &tabs,
            active: 2,
            tab_pad_x: th.spacing_md,
            divider_margin_l: th.spacing_sm,
        },
    );
    let (line, _) = inner.allocate_exact_size(
        egui::vec2(inner.available_width(), bw),
        egui::Sense::hover(),
    );
    let p = inner.painter();
    p.rect_filled(line, 0.0, th.bg_sidebar().to_egui());
    p.hline(
        line.x_range(),
        line.center().y,
        egui::Stroke::new(bw, th.separator.to_egui_premultiplied()),
    );
    painter.rect_stroke(
        window,
        radius,
        egui::Stroke::new(bw, th.border_strong().to_egui()),
        egui::StrokeKind::Inside,
    );
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
    // 칸 폭은 그리기 전에 페이지 열 폭에서 스테이지 여백·테두리를 빼 정한다. 칸 안에서 가용 폭을 읽으면 감싸는 줄이
    // 직전 프레임 크기를 다시 요청해 칸이 계속 넓어진다.
    let crop_w =
        ui.available_width() - 2.0 * (theme.spacing_xl.value() + theme.border_width.value());
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (name, th) in &themes {
            for (active, close, label) in FRAMES {
                let tip = format!("{name} · {label}");
                spec::cluster(ui, theme, &tip, |ui| {
                    frame(ui, th, crop_w, *active, *close, &tip)
                });
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
