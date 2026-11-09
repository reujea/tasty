//! 탭바 아래에 표시하는 배너의 외형·닫기 버튼·메뉴·겹침 예제.
//! 본체는 스코프당 한 개를 표시하고 최대 다섯 개를 대기시킨다. 이 예제는 큐를 실행하지 않는다.
//! 카운트다운은 고정된 숫자로 그리며 호버·만료·키보드 포커스 처리는 재현하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    BannerMoreLabel, Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Input,
    banner_more_row, banner_shell, kbd, switch,
};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 후면 배너를 아래로 내려 겹침을 드러내는 전시용 오프셋. 텍스트 크기와 무관하다.
const STACK_REAR_OVERHANG_Y: LogicalPx = LogicalPx(14.0);
/// 전면 배너 child 영역의 높이. 뒤쪽 배너의 아래 부분이 남도록 정한 데모 기하다.
const STACK_FRONT_BANNER_H: LogicalPx = LogicalPx(56.0);
/// 셸 통합 안내 예제 카드의 최대 폭. 디자인은 `--tasty-size-480`을 쓴다.
const SHELL_CARD_MAX_W: LogicalPx = LogicalPx(480.0);
/// 셸 통합 안내의 TTL(초). 본체 `defs::SHELL_INTEGRATION_MISSING_TTL_SECONDS`와 같은 디자인 고정값이다.
const SHELL_INTEGRATION_TTL_SECONDS: u32 = 10;

fn dim(color: egui::Color32, opacity: f32) -> egui::Color32 {
    color.gamma_multiply(opacity)
}

/// 인라인 leading 글리프 — `size` 정사각, `color` tint.
fn glyph(ui: &mut egui::Ui, g: MockGlyph, size: f32, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    g.image(size, color).paint_at(ui, rect);
}

/// 제목 줄 — body(13) semibold, text-primary.
fn title_line(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_body.value())
            .strong()
            .color(theme.text_primary().to_egui()),
    );
}

/// 본문 줄 — caption(11), text-muted.
fn body_line(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_caption.value())
            .color(theme.banner_body_fg().to_egui()),
    );
}

/// 우상단 dismiss × (hover 노출 상태). sm IconButton.
fn dismiss_x(ui: &mut egui::Ui, theme: &Theme) {
    IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .show(ui, theme, &|ui, rect, c| {
            icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
        });
}

/// 마우스 캡처 배너의 더보기 버튼 표시 상태.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MoreTriggerState {
    /// hover 전 — ⋯/× 둘 다 숨김(폭은 예약된 채 비어 있음).
    Hidden,
    /// hover 중 — ⋯ + × 둘 다 노출(⋯가 왼쪽, 4px gap).
    Hovered,
    /// 메뉴가 열려 있음 — ⋯ 는 hover 여부와 무관하게 계속 표시 + active 강조.
    Open,
}

/// 우상단 "더보기"(⋯) 트리거. sm IconButton, `active` 면 icon-button-bg-active 강조.
fn more_trigger(ui: &mut egui::Ui, theme: &Theme, active: bool) {
    IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .active(active)
        .show(ui, theme, &|ui, rect, c| {
            icons::MORE.image(rect.height(), c).paint_at(ui, rect)
        });
}

/// 남은 시간을 고정폭 글꼴로 표시한다.
fn countdown(ui: &mut egui::Ui, theme: &Theme, seconds: u32) {
    ui.label(
        egui::RichText::new(seconds.to_string())
            .monospace()
            .size(theme.font_size_micro.value())
            .color(theme.banner_countdown_fg().to_egui()),
    );
}

/// 탭바를 덮지 않는 배너 영역을 반환한다. 탭 높이는 이 예제의 디자인 값이다.
fn faux_chrome(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: f32,
    height: f32,
    tabs: [&str; 3],
    top_line: Option<&str>,
    bottom_note: Option<&str>,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let radius = theme.corner_radius.value();
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, radius, theme.ansi_black.to_egui());

    let tab_h = 28.0;
    let tab_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), tab_h));
    painter.rect_filled(
        tab_rect,
        egui::CornerRadius {
            nw: radius as u8,
            ne: radius as u8,
            sw: 0,
            se: 0,
        },
        theme.bg_sidebar().to_egui(),
    );
    painter.hline(
        tab_rect.x_range(),
        tab_rect.bottom(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let mut x = tab_rect.left();
    for (i, label) in tabs.iter().enumerate() {
        let active = i == 2;
        let pad = theme.spacing_md.value();
        let galley = painter.layout_no_wrap(
            label.to_string(),
            egui::FontId::proportional(theme.font_size_body.value()),
            if active {
                theme.text_primary().to_egui()
            } else {
                theme.text_muted().to_egui()
            },
        );
        let tab_w = galley.size().x + pad * 2.0;
        let this =
            egui::Rect::from_min_size(egui::pos2(x, tab_rect.top()), egui::vec2(tab_w, tab_h));
        if active {
            painter.rect_filled(this, 0.0, theme.bg_panel().to_egui());
            let bar = egui::Rect::from_min_size(
                egui::pos2(
                    this.left(),
                    this.bottom() - theme.tab_indicator_width().value(),
                ),
                egui::vec2(this.width(), theme.tab_indicator_width().value()),
            );
            painter.rect_filled(bar, 0.0, theme.accent_primary().to_egui());
        }
        painter.galley(
            egui::pos2(x + pad, tab_rect.center().y - galley.size().y / 2.0),
            galley,
            theme.text_primary().to_egui(),
        );
        x += tab_w;
        painter.vline(
            x,
            tab_rect.y_range(),
            egui::Stroke::new(
                theme.border_width.value(),
                theme.separator.to_egui_premultiplied(),
            ),
        );
    }

    let content_pad = theme.spacing_md.value();
    if let Some(line) = top_line {
        painter.text(
            egui::pos2(rect.left() + content_pad, tab_rect.bottom() + content_pad),
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::monospace(theme.font_size_term_sm.value()),
            dim(theme.text_muted().to_egui(), theme.opacity_recessed()),
        );
    }
    if let Some(note) = bottom_note {
        painter.text(
            egui::pos2(rect.left() + content_pad, rect.bottom() - content_pad),
            egui::Align2::LEFT_BOTTOM,
            note,
            egui::FontId::monospace(theme.font_size_caption.value()),
            theme.text_disabled().to_egui(),
        );
    }

    let margin = theme.spacing_sm.value();
    egui::Rect::from_min_max(
        egui::pos2(rect.left() + margin, tab_rect.bottom() + margin),
        egui::pos2(rect.right() - margin, rect.bottom() - margin),
    )
}

fn faux_scope(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: f32,
    height: f32,
    banner: impl FnOnce(&mut egui::Ui),
) {
    let zone = faux_chrome(
        ui,
        theme,
        width,
        height,
        ["server", "dev", "vim"],
        Some("~/tasty $ vim src/main.rs"),
        None,
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(zone));
    banner(&mut child);
}

/// 더보기와 닫기 버튼의 폭을 항상 확보해 호버 전후에 본문 폭이 바뀌지 않게 한다.
fn mouse_capture_banner(ui: &mut egui::Ui, theme: &Theme, more: MoreTriggerState) {
    mouse_capture_banner_body(ui, theme, more, None);
}

/// `body` 가 있으면 기본 본문(Shift 키캡 두 개) 대신 그 한 줄을 쓴다(짧은 로캘 예).
pub(super) fn mouse_capture_banner_body(
    ui: &mut egui::Ui,
    theme: &Theme,
    more: MoreTriggerState,
    body: Option<&str>,
) {
    // ⋯/× 열은 본문 흐름 밖에 고정한다(시안 absolute top banner-padding-y, right space-sm).
    let avail = ui.max_rect();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
        // 앞 글리프는 banner-glyph-offset 만큼 내려 제목 줄에 맞춘다.
        let size = theme.icon_glyph_size_md.value();
        let offset = theme.banner_glyph_offset().value();
        let (slot, _) =
            ui.allocate_exact_size(egui::vec2(size, size + offset), egui::Sense::hover());
        icons::MOUSE
            .image(size, theme.banner_icon_fg().to_egui())
            .paint_at(
                ui,
                egui::Rect::from_min_size(
                    egui::pos2(slot.left(), slot.top() + offset),
                    egui::vec2(size, size),
                ),
            );
        // 본문 컬럼 — 오른쪽에 banner-more-reserve 를 비워 ⋯/× 열 자리를 남긴다.
        let body_w = (ui.available_width() - theme.banner_more_reserve().value()).max(0.0);
        ui.vertical(|ui| {
            ui.set_width(body_w);
            ui.spacing_mut().item_spacing.y = theme.banner_text_gap().value();
            title_line(ui, theme, crate::i18n::t("banner.mouse_capture.title"));
            if let Some(text) = body {
                body_line(ui, theme, text);
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
                body_line(ui, theme, "This app is capturing the mouse. Hold");
                kbd(ui, theme, "Shift", tasty_ui_widgets::Ground::Raised);
                body_line(ui, theme, "+drag to select text,");
                kbd(ui, theme, "Shift", tasty_ui_widgets::Ground::Raised);
                body_line(ui, theme, "+Right-click for the tasty menu.");
            });
        });
    });
    if more != MoreTriggerState::Hidden {
        let right = avail.right() + theme.banner_padding_x().value() - theme.spacing_sm.value();
        let column = egui::Rect::from_min_max(
            egui::pos2(right - theme.banner_more_reserve().value(), avail.top()),
            egui::pos2(right, avail.top() + ControlSize::Sm.height(theme)),
        );
        let mut column_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(column)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        column_ui.spacing_mut().item_spacing.x = theme.banner_more_column_gap().value();
        dismiss_x(&mut column_ui, theme);
        more_trigger(&mut column_ui, theme, more == MoreTriggerState::Open);
    }
}
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        faux_scope(ui, theme, theme.measure_lg.value(), 200.0, |ui| {
            banner_shell(ui, theme, 1.0, |ui| {
                mouse_capture_banner(ui, theme, MoreTriggerState::Hovered);
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("family", "Modal · Popup · Toast · Banner"),
            ("position", "content-top, below the tab bar"),
            ("width", "100% − 16 (8 each side)"),
            ("margin", "8 top/sides · 0 bottom"),
            (
                "lifetime",
                "no action → TTL (auto-dismiss) · has action → until the action or ×",
            ),
            ("fires on", "user click only (never IPC)"),
        ],
        &[
            TokenChip::new("banner-bg", "surface0 fill", theme.banner_bg().to_egui()),
            TokenChip::new("banner-border", "1px edge", theme.banner_border().to_egui()),
            TokenChip::without_color("banner-margin", "8px top/sides"),
            TokenChip::without_color("banner-shadow", "floating lift"),
            TokenChip::new(
                "banner-icon-fg",
                "mouse glyph",
                theme.banner_icon_fg().to_egui(),
            ),
            TokenChip::without_color("banner-recessed-opacity", "dimmed variant"),
        ],
    );

    spec::do_(
        ui,
        theme,
        "Reach for a Banner when an action could attach (Popup = independent feature, \
         Banner = info ± action, Toast = info only). This first instance is action-less: \
         the bypass keys (Shift+drag / Shift+Right-click) are the message.",
    );

    spec::note(
        ui,
        theme,
        "There is no Info/Warning/Error kind — each banner's id is its kind and styles \
         its own leading glyph. The mouse-capture banner is persistent (no TTL / countdown); \
         its interactive elements are the × and the ⋯ \"more\" trigger, both hover-revealed \
         (see the banner-more-menu spec for the ⋯ context menu).",
    );
}
pub fn draw_hit_zone(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        let zone = faux_chrome(
            ui,
            theme,
            theme.measure_lg.value(),
            220.0,
            ["server", "dev", "htop"],
            None,
            Some("surface body below the card — pass-through; clicks/drag reach the app"),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(zone));
        banner_shell(&mut child, theme, 1.0, |ui| {
            mouse_capture_banner(ui, theme, MoreTriggerState::Hovered);
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("anchor", "surface content top, under tab bar"),
            ("margins", "8 top / 8 sides · 0 bottom"),
            ("consume zone", "the drawn card rect only"),
            ("below card", "pass-through to the app"),
            ("focus", "never steals keyboard focus"),
            ("inactive surface", "first click only focuses it"),
        ],
        &[
            TokenChip::new("banner-margin", "8px gap", theme.banner_bg().to_egui()),
            TokenChip::new(
                "accent-info",
                "consume-zone label",
                theme.accent_info().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "The card rect — not the whole scope — is the mouse-consume + hover zone. Clicks on \
         the surface body below the card pass through to the capturing app, so mouse reporting \
         keeps working everywhere except the card itself.",
    );
}
pub fn draw_blacklist(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            caption_label(ui, theme, "filled · one row hovered");
            blacklist_editor(ui, theme, false);
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            caption_label(ui, theme, "empty (default) · Add disabled");
            blacklist_editor(ui, theme, true);
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("lives in", "Settings › Terminal"),
            ("group", "hint switch + blacklist"),
            ("row", "pattern (mono) + × remove"),
            ("add", "Input + Add (disabled when empty)"),
            ("match", "case-insensitive substring or *"),
            ("default", "empty list"),
        ],
        &[
            TokenChip::without_color("overlay-hover", "row hover (8%)"),
            TokenChip::new(
                "accent-warning",
                "match-rule notice",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new("input-bg", "add field", theme.input_bg().to_egui()),
        ],
    );

    spec::note(
        ui,
        theme,
        "The exclusion list keeps wheel events forwarded to the program while handling clicks and drags locally. A matching foreground program also suppresses the mouse-capture banner.",
    );
}

/// 블랙리스트 한 행 — 패턴(mono) + 우측 × 제거. hover 행은 overlay-hover 배경.
fn blacklist_row(ui: &mut egui::Ui, theme: &Theme, pattern: &str, hover: bool) {
    let fill = if hover {
        theme.overlay_hover().to_egui_premultiplied()
    } else {
        egui::Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(theme.corner_radius_sm.value())
        .inner_margin(egui::Margin {
            left: theme.spacing_sm.value() as i8,
            right: theme.spacing_xs.value() as i8,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(pattern)
                        .monospace()
                        .size(theme.font_size_caption.value())
                        .color(theme.text_primary().to_egui()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    dismiss_x(ui, theme);
                });
            });
        });
}

/// 마우스 캡처 제외 목록 설정의 채운 상태와 빈 상태를 비교한다.
fn blacklist_editor(ui: &mut egui::Ui, theme: &Theme, empty: bool) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.set_width(theme.measure_sm.value());
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();

            ui.label(
                egui::RichText::new("MOUSE CAPTURE")
                    .family(tasty_ui_widgets::label_font_family())
                    .size(theme.font_size_micro.value())
                    .color(theme.text_muted().to_egui()),
            );

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Show mouse-capture hint")
                        .size(theme.font_size_body.value())
                        .color(theme.text_secondary().to_egui()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut on = true;
                    switch(ui, theme, &mut on, None, true);
                });
            });

            let (sep, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), theme.border_width.value()),
                egui::Sense::hover(),
            );
            ui.painter()
                .rect_filled(sep, 0.0, theme.separator.to_egui_premultiplied());

            ui.label(
                egui::RichText::new("Disable capture for these programs")
                    .size(theme.font_size_body.value())
                    .color(theme.text_secondary().to_egui()),
            );

            if empty {
                ui.label(
                    egui::RichText::new(crate::i18n::t(
                        "settings.terminal.mouse_capture_blacklist_empty",
                    ))
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
                );
            } else {
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    blacklist_row(ui, theme, "htop", false);
                    blacklist_row(ui, theme, "vim", true);
                    blacklist_row(ui, theme, "ht*", false);
                });
            }

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    Button::new("Add")
                        .variant(ButtonVariant::Secondary)
                        .size(ControlSize::Sm)
                        .enabled(!empty)
                        .show(ui, theme);
                    let mut buf = String::new();
                    Input::new()
                        .placeholder(crate::i18n::t(
                            "settings.terminal.mouse_capture_blacklist_add_placeholder",
                        ))
                        .mono(true)
                        .show(ui, theme, &mut buf);
                });
            });

            ui.label(
                egui::RichText::new(
                    "Case-insensitive substring or * wildcard on the process name. \
                     When a listed program is foreground, clicks/drags are handled locally; \
                     the wheel is still sent.",
                )
                .size(theme.font_size_caption.value())
                .color(theme.accent_warning_raised().to_egui()),
            );
        });
}
pub fn draw_dismiss(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            caption_label(
                ui,
                theme,
                "has an action → no TTL · hover the banner to reveal ×",
            );
            ui.scope(|ui| {
                ui.set_max_width(theme.measure_md.value());
                banner_shell(ui, theme, 1.0, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                        body_secondary(ui, theme, "Stays until the action or × (× on hover)");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            dismiss_x(ui, theme);
                            Button::new(crate::i18n::t("remote.size_sync.retry"))
                                .variant(ButtonVariant::Secondary)
                                .size(ControlSize::Sm)
                                .show(ui, theme);
                        });
                    });
                });
            });
        });

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            caption_label(ui, theme, "no action → TTL 6s · countdown → × on hover");
            ui.scope(|ui| {
                ui.set_max_width(theme.measure_md.value());
                banner_shell(ui, theme, 1.0, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                        glyph(
                            ui,
                            icons::CHECK,
                            theme.icon_glyph_size_md.value(),
                            theme.accent_success().to_egui(),
                        );
                        body_secondary(ui, theme, "Preset Dev split applied to this tab.");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            countdown(ui, theme, 6);
                        });
                    });
                });
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("slot", "top-right · one affordance"),
            ("has action", "no TTL · × hidden → reveal on hover"),
            ("no action", "TTL · seconds count → × on hover"),
            ("expiry", "0 → auto-dismiss"),
            ("pause", "hover · backgrounded scope"),
            ("motion", "120ms alpha fade, no move"),
        ],
        &[
            TokenChip::new(
                "banner-countdown-fg",
                "seconds",
                theme.banner_countdown_fg().to_egui(),
            ),
            TokenChip::without_color("banner-fade", "120ms appear/dismiss"),
            TokenChip::without_color("banner-countdown-font", "mono digits"),
            TokenChip::new(
                "accent-success",
                "per-banner glyph",
                theme.accent_success().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "The host pauses the countdown while the banner is hovered or its scope is backgrounded and uses a 120ms alpha fade. This example only paints the close button and a fixed countdown; it does not run the timer or fade.",
    );
}

/// 배너 수명 규칙: 사용자 동작이 없으면 TTL, 있으면 TTL 없음.
pub fn draw_lifetime(ui: &mut egui::Ui, theme: &Theme) {
    spec::meta(
        ui,
        theme,
        &[
            ("no action", "TTL — countdown, auto-dismiss at 0"),
            ("has action", "no TTL — until the action or ×"),
            ("×", "not an action"),
            (
                "exception",
                "developer-directed, code comment at the definition (none in use for mouse capture)",
            ),
            (
                "plugin banners",
                "same rule as authoring guidance (host can't see plugin content)",
            ),
        ],
        &[TokenChip::new(
            "banner-countdown-fg",
            "TTL seconds",
            theme.banner_countdown_fg().to_egui(),
        )],
    );

    spec::note(
        ui,
        theme,
        "A banner exists only while it is valid. While valid, a banner that offers no user \
         action (inline button or ⋯ menu row) has a TTL and closes on its own; a banner that \
         offers one has no TTL and stays until the action or ×. The × is not an action. \
         Host banners with a TTL: shell integration missing (10 s). Without a TTL: mouse \
         capture (⋯ rows), auto-attach refused, attach size sync, HTML script notice, HTML \
         mirror size / aggregate.",
    );
}

/// 셸 통합 안내 카드 한 장. 본체 `content_shell_integration_missing`과 같은 구성이다.
fn shell_integration_banner(ui: &mut egui::Ui, theme: &Theme, hovered: bool) {
    banner_shell(ui, theme, 1.0, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
            glyph(
                ui,
                icons::TERMINAL,
                theme.icon_glyph_size_md.value(),
                theme.banner_icon_fg().to_egui(),
            );
            ui.vertical(|ui| {
                ui.set_max_width(
                    (ui.available_width() - theme.item_height_interactive.value()).max(0.0),
                );
                ui.spacing_mut().item_spacing.y = theme.banner_text_gap().value();
                ui.label(
                    egui::RichText::new(crate::i18n::t("banner.shell_integration_missing.title"))
                        .size(theme.font_size_body.value())
                        .strong()
                        .color(theme.banner_fg().to_egui()),
                );
                body_line(
                    ui,
                    theme,
                    crate::i18n::t("banner.shell_integration_missing.body"),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if hovered {
                    dismiss_x(ui, theme);
                } else {
                    countdown(ui, theme, SHELL_INTEGRATION_TTL_SECONDS);
                }
            });
        });
    });
}

/// `shell-integration-missing` — 동작이 없어 TTL 10초로 닫히는 surface 배너.
pub fn draw_shell_integration(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
        for (label, hovered) in [
            ("countdown · 10 s TTL", false),
            ("hovered · countdown paused, × shown", true),
        ] {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                caption_label(ui, theme, label);
                ui.scope(|ui| {
                    ui.set_max_width(SHELL_CARD_MAX_W.value());
                    shell_integration_banner(ui, theme, hovered);
                });
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("scope", "surface"),
            ("actions", "none → TTL"),
            ("TTL", "10 s — fixed design constant, not a setting"),
            ("pause", "hover · scope backgrounded"),
            ("re-raise (shown)", "countdown resets to 10"),
            (
                "text",
                "banner.shell_integration_missing.* (en/ko/ja/zh-Hans)",
            ),
        ],
        &[
            TokenChip::new(
                "banner-countdown-fg",
                "seconds",
                theme.banner_countdown_fg().to_egui(),
            ),
            TokenChip::without_color("banner-countdown-font", "mono digits"),
            TokenChip::new(
                "banner-icon-fg",
                "terminal glyph",
                theme.banner_icon_fg().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Why 10 s: the body is two sentences plus a docs path — about 7–8 s of reading, plus \
         time to notice a card over live output. Hover pauses the count. No setting: a \
         per-banner TTL setting would be the only one of its kind.",
    );
}

pub fn draw_stack(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        let width = theme.measure_md.value();
        let height = 116.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());

        let lower = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() + STACK_REAR_OVERHANG_Y.value()),
            egui::pos2(rect.right(), rect.bottom()),
        );
        let mut lower_ui = ui.new_child(egui::UiBuilder::new().max_rect(lower));
        banner_shell(&mut lower_ui, theme, theme.opacity_recessed(), |ui| {
            ui.set_min_height(84.0);
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                body_line(
                    ui,
                    theme,
                    "Pane banner — recessed to 40% opacity behind the higher banner; only its overhang shows",
                );
            });
        });

        let upper = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.right(), rect.top() + STACK_FRONT_BANNER_H.value()),
        );
        let mut upper_ui = ui.new_child(egui::UiBuilder::new().max_rect(upper));
        banner_shell(&mut upper_ui, theme, 1.0, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                glyph(
                    ui,
                    icons::ALERT_TRIANGLE,
                    theme.icon_glyph_size_md.value(),
                    theme.accent_warning().to_egui(),
                );
                body_secondary(
                    ui,
                    theme,
                    "Workspace banner — in front, full opacity, higher z-index",
                );
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("per scope", "1 shown + up to 5 queued"),
            ("dup (shown)", "resets the countdown"),
            ("dup (queued)", "ignored"),
            ("queue full", "new banner dropped"),
            ("z-order", "View > Workspace > Pane > Tab > Surface"),
            ("recessed", "lower scope → 40% opacity"),
        ],
        &[
            TokenChip::new(
                "banner-recessed-opacity",
                "dimmed lower",
                dim(theme.banner_bg().to_egui(), theme.opacity_recessed()),
            ),
            TokenChip::new("banner-bg", "both shells", theme.banner_bg().to_egui()),
        ],
    );

    spec::note(
        ui,
        theme,
        "The host displays one banner per scope and queues up to five more. Higher scopes appear in front (View > Workspace > Pane > Tab > Surface); lower banners use 40% opacity. This example paints the two layers without running the queue.",
    );
}
pub fn draw_more_menu(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();

        // 시안은 세 배너를 세로로 쌓는다(flexDirection: column).
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
            for (state, caption) in [
                (MoreTriggerState::Hidden, "closed — hover to reveal"),
                (MoreTriggerState::Hovered, "hover — ⋯ left of ×, 4px gap"),
                (MoreTriggerState::Open, "menu open — ⋯ stays + active tint"),
            ] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                    caption_label(ui, theme, caption);
                    ui.scope(|ui| {
                        ui.set_max_width(theme.measure_md.value());
                        banner_shell(ui, theme, 1.0, |ui| {
                            mouse_capture_banner(ui, theme, state);
                        });
                    });
                });
            }
        });

        // 실제 메뉴의 위치를 정적으로 보여준다.
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            caption_label(
                ui,
                theme,
                "menu — anchored 4px below the trigger, right-aligned",
            );
            mouse_capture_menu(ui, theme, "vim", 240.0);
        });

        // 고정 라벨을 유지하면서 긴 프로그램 이름만 줄이는지 비교한다.
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            caption_label(
                ui,
                theme,
                "forced 200px — the fixed copy plus one name glyph does not fit, so that row \
                 wraps; the fixed text never ellipsizes (the app widens this menu to 288 instead, \
                 see Elastic width)",
            );
            mouse_capture_menu(ui, theme, "a-very-long-tui-program-name", 200.0);
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("trigger", "24px Ghost IconButton · 3-dot `more` icon"),
            ("order", "⋯ left of × · 4px gap between"),
            ("reserve", "56px (2×24 + gap4 + gap4) — always reserved"),
            (
                "anchor",
                "trigger bottom +4px · right-aligned · flips up if clipped",
            ),
            ("menu size", "200–288px wide · 4px inner padding"),
            (
                "items",
                "suppress banner (bell) · disable capture (mouse) — both neutral",
            ),
            ("reveal", "banner hover — same rule as ×"),
            ("open state", "active (accent) while menu is open"),
            ("card size", "unchanged"),
        ],
        &[
            TokenChip::new(
                "menu-item-bg-hover",
                "row hover",
                theme.menu_item_bg_hover().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "icon-button-bg-active",
                "⋯ active tint",
                theme.icon_button_bg_active().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "border-strong",
                "menu edge",
                theme.border_strong().to_egui(),
            ),
            TokenChip::without_color("banner-more-column-gap", "4px ⋯ ↔ ×"),
            TokenChip::without_color("banner-more-reserve", "56px body right padding"),
            TokenChip::new(
                "overlay-active",
                "open-trigger wash",
                theme.overlay_active().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "accent-primary",
                "open-trigger glyph",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "banner-more-menu-bg",
                "menu fill",
                theme.banner_more_menu_bg().to_egui(),
            ),
            TokenChip::new(
                "banner-more-menu-border",
                "edge",
                theme.banner_more_menu_border().to_egui(),
            ),
            TokenChip::without_color("banner-more-menu-shadow", "popover lift"),
            TokenChip::without_color("banner-more-menu-offset", "4px below trigger"),
            TokenChip::without_color("banner-more-menu-min-width", "200px floor"),
            TokenChip::without_color("banner-more-menu-max-width", "288px ceiling"),
            TokenChip::without_color("banner-more-app-font", "mono program name"),
            TokenChip::new(
                "banner-more-app-fg",
                "program name tone",
                theme.banner_more_app_fg().to_egui(),
            ),
            TokenChip::without_color("menu-item-height", "28px rows"),
            TokenChip::new(
                "accent-danger",
                "rejected variant only",
                theme.accent_danger().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "The ⋯ trigger shows on hover (same condition as ×) and stays + active-tints while \
         its menu is open — re-click ⋯ to reuse. Both menu items run immediately and close \
         the menu; neither is danger-toned (both are reversible from Settings › Terminal). \
         Suppressing the banner also closes it immediately; disabling capture leaves it open \
         so the user can read the confirmation. The program name renders as its own mono \
         segment so only it ellipsizes — the fixed label text never wraps or truncates.",
    );
    spec::note(
        ui,
        theme,
        "New glyph more (icons/more.svg — three dots, horizontal, same 2px round-cap language \
         as the dot glyphs). A unicode ⋯ was rejected: the system renders every affordance \
         through the Icon component so it inherits currentColor and the 16px sm size.",
    );
    spec::dont(
        ui,
        theme,
        "Don't keep the trigger permanently visible. A persistent banner with a permanently \
         drawn ⋯ reads as a second dismiss control and adds chrome to a notice that is already \
         at the top of live output — hover is where every other banner affordance lives.",
    );
}

/// 배너 알림 끄기와 마우스 캡처 끄기 메뉴의 고정 순서·가변 폭 예제.
fn mouse_capture_menu(ui: &mut egui::Ui, theme: &Theme, app: &str, width: f32) {
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_strong().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            mouse_capture_menu_row(
                ui,
                theme,
                icons::BELL,
                crate::i18n::t("popup.mouse_capture_banner_menu.suppress_prefix"),
                app,
                crate::i18n::t("popup.mouse_capture_banner_menu.suppress_suffix"),
                true,
            );
            mouse_capture_menu_row(
                ui,
                theme,
                icons::MOUSE,
                crate::i18n::t("popup.mouse_capture_banner_menu.disable_prefix"),
                app,
                crate::i18n::t("popup.mouse_capture_banner_menu.disable_suffix"),
                false,
            );
        });
}

/// 고정 라벨은 유지하고 프로그램 이름만 줄인다. 본체 mouse_capture_menu와 같은 공용 행이다.
fn mouse_capture_menu_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: MockGlyph,
    prefix: &str,
    app: &str,
    suffix: &str,
    hovered: bool,
) {
    let _row = banner_more_row(
        ui,
        theme,
        &|ui, rect, c| icon.image(rect.height(), c).paint_at(ui, rect),
        BannerMoreLabel {
            prefix,
            app,
            suffix,
        },
        false,
        hovered,
        false,
    );
}

/// 데모 라벨 — caption(11), text-muted (스테이지 내 상태 주석).
pub(super) fn caption_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 단일 줄 본문 — body(13), text-secondary, 남는 폭을 채운다.
fn body_secondary(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_body.value())
            .color(theme.text_secondary().to_egui()),
    );
}
