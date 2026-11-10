//! 마우스 캡처 배너의 상태 예제. 시안 `overlays-banners.jsx` 의 "Anatomy & states" 와
//! 더보기 메뉴의 "Elastic width". 첫 카드는 직전 프레임의 카드 영역에 포인터가 있으면 ⋯/× 를 드러낸다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    BannerMoreLabel, BannerMoreUnknownCopy, banner_more_row, banner_more_row_natural_width,
    banner_more_unknown, banner_shell,
};

use super::banner::{MoreTriggerState, caption_label, mouse_capture_banner_body};
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};

// 시안 무대의 전시 치수. 대응 토큰이 없다.
/// 무대 안쪽 여백 — `padding: 20`.
const STAGE_PAD: LogicalPx = LogicalPx(20.0);
/// 예제 칸 최대 폭 — `maxWidth: 480`.
const CARD_MAX_W: LogicalPx = LogicalPx(480.0);
/// 탄력 폭 예제 칸 사이 — `gap: 18`.
const ELASTIC_GAP: LogicalPx = LogicalPx(18.0);
/// 탄력 폭 예제 라벨과 메뉴 사이 — `gap: 6`.
const ELASTIC_LABEL_GAP: LogicalPx = LogicalPx(6.0);
/// 메뉴 행의 고정 문구(앞, 뒤) — 시안 `BannerMoreMenuG` 의 행 문구. 본체 `row_labels` 와 같은 키 쌍이다.
fn suppress_text() -> (&'static str, &'static str) {
    (
        crate::i18n::t("popup.mouse_capture_banner_menu.suppress_prefix"),
        crate::i18n::t("popup.mouse_capture_banner_menu.suppress_suffix"),
    )
}
fn disable_text() -> (&'static str, &'static str) {
    (
        crate::i18n::t("popup.mouse_capture_banner_menu.disable_prefix"),
        crate::i18n::t("popup.mouse_capture_banner_menu.disable_suffix"),
    )
}
/// 내장 언어 `code` 로케일의 두 행 고정 문구(앞, 뒤). 본체 `row_labels` 와 같은 키 쌍을 그 언어의
/// `lang/{code}.toml` 에서 읽으므로 번역이 바뀌면 예제도 따라 바뀐다. en 은 갤러리 기본 언어다.
fn locale_texts(code: &str) -> [(&'static str, &'static str); 2] {
    use crate::i18n::t_lang;
    [
        (
            t_lang(code, "popup.mouse_capture_banner_menu.suppress_prefix"),
            t_lang(code, "popup.mouse_capture_banner_menu.suppress_suffix"),
        ),
        (
            t_lang(code, "popup.mouse_capture_banner_menu.disable_prefix"),
            t_lang(code, "popup.mouse_capture_banner_menu.disable_suffix"),
        ),
    ]
}
/// ja 줄바꿈 예제의 고정 문구 — 시안 `BannerMoreMenuG` 의 `texts`. 긴 고정 문구로 줄바꿈 규칙만 보인다.
const JA_TEXTS: [(&str, &str); 2] = [
    ("", " の通知をオフにする"),
    (
        "",
        " について、このプログラムのマウスキャプチャを常に無効にする",
    ),
];

/// bg-app 무대 — 가운데 정렬 세로 묶음.
fn app_stage(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(STAGE_PAD.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(add);
            });
    });
}

/// 라벨 + 480 폭 카드 한 칸.
fn card_slot(ui: &mut egui::Ui, theme: &Theme, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let w = CARD_MAX_W.value().min(ui.available_width());
    ui.allocate_ui_with_layout(
        egui::vec2(w, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            caption_label(ui, theme, label);
            add(ui);
        },
    );
}

pub fn draw_anatomy(ui: &mut egui::Ui, theme: &Theme) {
    app_stage(ui, theme, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
        card_slot(ui, theme, "default · hover the card to reveal ×", |ui| {
            let id = ui.id().with("g_banner_anatomy_card");
            let prev: Option<egui::Rect> = ui.data(|d| d.get_temp(id));
            let hovered = prev.is_some_and(|r| ui.rect_contains_pointer(r));
            let state = if hovered {
                MoreTriggerState::Hovered
            } else {
                MoreTriggerState::Hidden
            };
            let top = ui.cursor().min;
            banner_shell(ui, theme, 1.0, |ui| {
                mouse_capture_banner_body(ui, theme, state, None);
            });
            let rect = egui::Rect::from_min_max(top, ui.min_rect().max);
            ui.data_mut(|d| d.insert_temp(id, rect));
        });
        card_slot(ui, theme, "one-line body (short locale)", |ui| {
            banner_shell(ui, theme, 1.0, |ui| {
                mouse_capture_banner_body(
                    ui,
                    theme,
                    MoreTriggerState::Hidden,
                    Some("Mouse captured — hold Shift to select."),
                );
            });
        });
        card_slot(
            ui,
            theme,
            "recessed — dimmed behind a higher-scope banner",
            |ui| {
                // 시안은 카드 전체(셸 + 내용)에 opacity 를 건다. banner_shell 의 opacity 는 셸만 흐린다.
                ui.scope(|ui| {
                    ui.multiply_opacity(theme.opacity_recessed());
                    banner_shell(ui, theme, 1.0, |ui| {
                        mouse_capture_banner_body(ui, theme, MoreTriggerState::Hidden, None);
                    });
                });
            },
        );
    });

    spec::meta(
        ui,
        theme,
        &[
            ("title", "\"Mouse input captured\" · 13/600 banner-fg"),
            ("body", "caption · banner-body-fg · 1–3 lines"),
            ("glyph", "mouse · banner-icon-fg"),
            ("affordance", "⋯ + × on hover (no TTL / countdown)"),
            ("fires", "once per tracking session · user click only"),
            ("dismiss", "suppresses for the session"),
        ],
        &[
            TokenChip::new("banner-bg", "card fill", theme.banner_bg().to_egui()),
            TokenChip::new(
                "banner-icon-fg",
                "mouse glyph",
                theme.banner_icon_fg().to_egui(),
            ),
            TokenChip::without_color("banner-recessed-opacity", "dimmed variant"),
        ],
    );
    spec::note(
        ui,
        theme,
        "The bypass keys (Shift+drag / Shift+Right-click) are the message — keep them in the \
         body. No inline action buttons: the body stays text-only, and the two per-app opt-outs \
         live behind the ⋯ trigger next to the × (see Banner more menu below).",
    );
}

pub fn draw_elastic(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(STAGE_PAD.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing =
                        egui::vec2(ELASTIC_GAP.value(), ELASTIC_GAP.value());
                    elastic_slot(
                        ui,
                        theme,
                        "short name — content-sized (the vim row already passes the 200 floor)",
                        |ui| {
                            more_menu(ui, theme, "vim", None, false);
                        },
                    );
                    elastic_slot(ui, theme, "medium — grows to fit", |ui| {
                        more_menu(ui, theme, "python3.11", Some(0), false);
                    });
                    elastic_slot(
                        ui,
                        theme,
                        "long — capped at 288px, name ellipsises",
                        |ui| {
                            more_menu(ui, theme, "some-very-long-tool-name", None, false);
                        },
                    );
                    elastic_slot(
                        ui,
                        theme,
                        "ja — fixed copy alone exceeds 288: that row wraps, menu stays 288 (2026-10-08)",
                        |ui| {
                            more_menu_texts(ui, theme, JA_TEXTS, "vim", Some(1), false);
                        },
                    );
                    elastic_slot(
                        ui,
                        theme,
                        "rejected — danger tone on the capture row",
                        |ui| {
                            more_menu(ui, theme, "vim", None, true);
                        },
                    );
                });
            });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("label", "fixed text + app name (2 spans)"),
            ("truncates", "the app name only"),
            (
                "app name",
                "mono · row ink (banner-more-app-fg → menu-item-fg / -fg-hover)",
            ),
            (
                "width",
                "content-sized, 200 ≤ w ≤ 288 — never widens past 288",
            ),
            (
                "wrap",
                "only when the FIXED copy + 1 name glyph does not fit: whole label wraps, row grows \
                 (min 28, pad-y menu-item-wrap-padding-y); the fixed copy never ellipsises",
            ),
            ("tooltip", "full program name on the row"),
            ("tone", "both rows neutral (no danger)"),
        ],
        &[
            TokenChip::without_color("banner-more-app-font", "mono program name"),
            TokenChip::new(
                "banner-more-app-fg",
                "program name = row ink",
                theme.banner_more_app_fg().to_egui(),
            ),
            TokenChip::new(
                "banner-more-app-fg-hover",
                "hover",
                theme.banner_more_app_fg_hover().to_egui(),
            ),
            TokenChip::without_color("menu-item-wrap-padding-y", "wrapped row pad-y"),
            TokenChip::without_color("menu-item-height", "28px rows"),
            TokenChip::new(
                "accent-danger",
                "rejected variant only",
                theme.accent_danger().to_egui(),
            ),
        ],
    );
    spec::do_(
        ui,
        theme,
        "Do keep the program name as the emphasised, mono part of the row — it is the one thing \
         the user must verify before writing a permanent per-app setting, and mono marks it as a \
         process name rather than prose.",
    );
    spec::dont(
        ui,
        theme,
        "Don't paint the \u{201c}Let Tasty handle clicks\u{201d} row in danger tone (last card above, \
         shown for the record). Nothing is destroyed and nothing is lost: the setting is \
         reversible from Settings › Terminal, and it returns the mouse to tasty. Danger in this \
         system means delete — spending it here would make the two rows look like different \
         classes of action when they are the same class at two strengths.",
    );
}

/// 시안 "Row copy — en / ko / ja / zh-Hans". en·ko·ja 예제 메뉴와 모든 내장 언어의 두 행 문구 표를
/// 본체와 같은 키로 그린다. en 은 앞 문구만, ko·ja 는 뒤 문구만, zh-Hans 는 앞뒤 문구를 모두 쓴다.
/// 긴 이름 예제는 288 상한에서 이름만 줄어드는지 보인다.
pub fn draw_row_copy(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(STAGE_PAD.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing =
                        egui::vec2(ELASTIC_GAP.value(), ELASTIC_GAP.value());
                    for (caption, app, code) in ROW_COPY_EXAMPLES {
                        elastic_slot(ui, theme, caption, |ui| {
                            more_menu_texts(ui, theme, locale_texts(code), app, None, false);
                        });
                    }
                });
            });
    });

    let mut specs: Vec<(String, String)> = Vec::new();
    for index in 0..2 {
        for code in tasty_i18n::BUILTIN_CODES {
            let (prefix, suffix) = locale_texts(code)[index];
            specs.push((
                format!("row {} {code}", index + 1),
                format!("prefix \"{prefix}\" · suffix \"{suffix}\""),
            ));
        }
    }
    specs.push((
        "keys".to_string(),
        "[popup.mouse_capture_banner_menu] prefix / suffix".to_string(),
    ));
    let specs: Vec<(&str, &str)> = specs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    spec::meta(
        ui,
        theme,
        &specs,
        &[TokenChip::without_color(
            "banner-more-menu-max-width",
            "288 — unchanged",
        )],
    );
    spec::note(
        ui,
        theme,
        "Settings › Terminal › Mouse Capture and the user guide take the same verbs: the banner \
         list is \u{201c}Don't show the capture notice for these programs\u{201d} and the \
         capture list is \u{201c}Let Tasty handle clicks in these programs\u{201d}, with the \
         existing description (clicks and drags go to selection and the menu; the wheel is \
         still sent).",
    );
}

/// 내장 언어 `code` 로케일의 이름 미상 문구. 본체 `unknown_copy` 와 같은 키를 그 언어 파일에서 읽는다.
fn unknown_texts(code: &str) -> BannerMoreUnknownCopy<'static> {
    use crate::i18n::t_lang;
    BannerMoreUnknownCopy {
        reason: t_lang(code, "popup.mouse_capture_banner_menu.unknown_reason"),
        rows: [
            t_lang(code, "popup.mouse_capture_banner_menu.notice_unknown"),
            t_lang(code, "popup.mouse_capture_banner_menu.capture_unknown"),
        ],
    }
}

/// 이름 미상 메뉴 — 시안 `BannerMoreMenuUnknownG`. 폭은 `banner-more-menu-max-width` 고정이고,
/// 사유 줄·구분선·비활성 두 행은 본체와 같은 공용 `banner_more_unknown` 이 그린다.
fn unknown_menu(ui: &mut egui::Ui, theme: &Theme, copy: BannerMoreUnknownCopy<'_>) {
    let pad = theme.banner_more_menu_padding().value();
    let bw = theme.border_width.value();
    let inner = theme.banner_more_menu_max_width().value() - (pad + bw) * 2.0;
    egui::Frame::new()
        .fill(theme.banner_more_menu_bg().to_egui())
        .stroke(egui::Stroke::new(
            bw,
            theme.banner_more_menu_border().to_egui(),
        ))
        .corner_radius(theme.banner_more_menu_radius().value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin::same(pad as i8))
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.set_max_width(inner);
            ui.spacing_mut().item_spacing.y = 0.0;
            let paint = |icon: tasty_icons::Icon| {
                move |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
                    icon.image(rect.height(), c).paint_at(ui, rect)
                }
            };
            let (info, bell, mouse) = (paint(icons::INFO), paint(icons::BELL), paint(icons::MOUSE));
            let _rows = banner_more_unknown(ui, theme, &info, [&bell, &mouse], copy);
        });
}

/// 시안 "Program name unknown — rows shown disabled". 이름을 모를 때도 ⋯ 는 그대로 그리고(예약 56),
/// 메뉴는 사유 줄 + 구분선 + 비활성 두 행이다. 원격 mirror 는 이름이 늘 미상이라 이 메뉴가 곧 mirror 의
/// 메뉴다(별도 예제 없음).
pub fn draw_unknown_name(ui: &mut egui::Ui, theme: &Theme) {
    app_stage(ui, theme, |ui| {
        card_slot(ui, theme, "name unknown — menu open (en)", |ui| {
            banner_shell(ui, theme, 1.0, |ui| {
                mouse_capture_banner_body(ui, theme, MoreTriggerState::Open, None);
            });
            ui.add_space(theme.banner_more_menu_offset().value());
            // 메뉴는 트리거 아래 오른쪽 정렬이다. 카드 오른쪽 끝에 메뉴(288)의 오른쪽 끝을 맞춘다.
            let menu_w = theme.banner_more_menu_max_width().value();
            let right = ui.min_rect().right();
            let slot = egui::Rect::from_min_size(
                egui::pos2(right - menu_w, ui.cursor().top()),
                egui::vec2(menu_w, 0.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(slot), |ui| {
                unknown_menu(ui, theme, unknown_texts("en"));
            });
        });
    });
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(STAGE_PAD.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing =
                        egui::vec2(ELASTIC_GAP.value(), ELASTIC_GAP.value());
                    for code in tasty_i18n::BUILTIN_CODES {
                        elastic_slot(ui, theme, &format!("{code} · unknown"), |ui| {
                            unknown_menu(ui, theme, unknown_texts(code));
                        });
                    }
                });
            });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "unknown when",
                "remote mirror · before first poll · lookup failed",
            ),
            ("⋯ trigger", "always drawn — same as known name"),
            ("body reserve", "56 (unchanged)"),
            (
                "reason line",
                "info glyph 14 · caption · text-muted · wraps · not focusable, no hover",
            ),
            ("rows", "MenuItem disabled as is · wrap on"),
            (
                "row ink",
                "text + icon state-disabled-fg · no overlay · no click",
            ),
            ("tooltip", "none — the reason is already in the menu"),
            ("click", "no-op · menu stays open · nothing saved"),
            ("keyboard", "arrows skip disabled rows"),
            ("live update", "no — next open"),
            ("lifetime", "has ⋯ → no TTL (normal rule, no exception)"),
            ("keys", "[popup.mouse_capture_banner_menu]"),
            (
                "key names",
                "unknown_reason · notice_unknown · capture_unknown (whole strings)",
            ),
        ],
        &[
            TokenChip::without_color("banner-more-reserve", "56 — always"),
            TokenChip::new(
                "state-disabled-fg",
                "disabled rows",
                theme.state_disabled_fg().to_egui(),
            ),
            TokenChip::new("text-muted", "reason line", theme.text_muted().to_egui()),
            TokenChip::without_color("font-size-caption", "reason line 11"),
            TokenChip::without_color("banner-more-reason-glyph-offset", "reason glyph top 1"),
            TokenChip::new(
                "menu-separator",
                "reason ↔ rows (border-strong)",
                theme.menu_separator().to_egui(),
            ),
        ],
    );
    spec::do_(
        ui,
        theme,
        "Do keep the rows the user would see with a known name, disabled, so the menu teaches \
         what exists; the reason line says why it can't run now, without a hover.",
    );
    spec::dont(
        ui,
        theme,
        "Don't hide the ⋯ or the rows when the name is unknown. Don't carry the reason in a \
         tooltip on a disabled row — a disabled row takes no pointer, and a tooltip is invisible \
         to anyone who doesn't hover.",
    );
}

/// "Row copy" 예제 (캡션, 프로그램 이름, 언어). 시안 순서다.
const ROW_COPY_EXAMPLES: [(&str, &str, &str); 5] = [
    ("en", "vim", "en"),
    ("ko", "vim", "ko"),
    ("ja", "vim", "ja"),
    ("en · long name", "some-very-long-tool-name", "en"),
    ("ja · long name", "some-very-long-tool-name", "ja"),
];

/// 라벨 + 메뉴 한 칸. 줄바꿈 줄에서 칸째로 넘어가도록 wrap_item 으로 감싼다.
fn elastic_slot(ui: &mut egui::Ui, theme: &Theme, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    spec::wrap_item(ui, |ui| {
        ui.spacing_mut().item_spacing.y = ELASTIC_LABEL_GAP.value();
        caption_label(ui, theme, label);
        add(ui);
    });
}

/// 두 행 메뉴 — 시안의 en 문구.
fn more_menu(ui: &mut egui::Ui, theme: &Theme, app: &str, hovered: Option<usize>, danger: bool) {
    more_menu_texts(
        ui,
        theme,
        [suppress_text(), disable_text()],
        app,
        hovered,
        danger,
    );
}

/// 두 행 메뉴 — 폭은 내용에 맞추되 min/max-width 토큰 사이로 묶는다. 상한에서 고정 문구가 들어가지
/// 않는 행은 공용 행이 줄을 바꾼다. `hovered` 는 시안 MenuItem `active`(surface-active)다.
fn more_menu_texts(
    ui: &mut egui::Ui,
    theme: &Theme,
    texts: [(&'static str, &'static str); 2],
    app: &str,
    hovered: Option<usize>,
    danger: bool,
) {
    let pad = theme.banner_more_menu_padding().value();
    let bw = theme.border_width.value();
    let chrome = (pad + bw) * 2.0;
    let label = |(prefix, suffix): (&'static str, &'static str)| BannerMoreLabel {
        prefix,
        app,
        suffix,
    };
    let row_w = texts
        .iter()
        .map(|t| banner_more_row_natural_width(ui.ctx(), theme, label(*t)))
        .fold(0.0_f32, f32::max);
    let outer = (row_w + chrome).clamp(
        theme.banner_more_menu_min_width().value(),
        theme.banner_more_menu_max_width().value(),
    );
    egui::Frame::new()
        .fill(theme.banner_more_menu_bg().to_egui())
        .stroke(egui::Stroke::new(
            bw,
            theme.banner_more_menu_border().to_egui(),
        ))
        .corner_radius(theme.banner_more_menu_radius().value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin::same(pad as i8))
        .show(ui, |ui| {
            ui.set_width(outer - chrome);
            ui.spacing_mut().item_spacing.y = 0.0;
            let icons = [icons::BELL, icons::MOUSE];
            for (i, (icon, text)) in icons.into_iter().zip(texts).enumerate() {
                let _row = banner_more_row(
                    ui,
                    theme,
                    &|ui, rect, c| icon.image(rect.height(), c).paint_at(ui, rect),
                    label(text),
                    hovered == Some(i),
                    false,
                    danger && i == 1,
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::locale_texts;

    /// 번역 예제는 본체와 같은 키를 그 언어 파일에서 읽는다.
    #[test]
    fn locale_examples_read_the_app_copy_of_that_language() {
        for code in crate::i18n::translated_languages() {
            let lang = crate::i18n::catalog_value(code);
            let menu = &lang["popup"]["mouse_capture_banner_menu"];
            let key = |k: &str| menu[k].as_str().expect("menu key").to_string();
            let [suppress, disable] = locale_texts(code);
            assert_eq!(
                (suppress.0.to_string(), suppress.1.to_string()),
                (key("suppress_prefix"), key("suppress_suffix"))
            );
            assert_eq!(
                (disable.0.to_string(), disable.1.to_string()),
                (key("disable_prefix"), key("disable_suffix"))
            );
            assert_ne!(
                (suppress.0, suppress.1),
                (
                    crate::i18n::t("popup.mouse_capture_banner_menu.suppress_prefix"),
                    crate::i18n::t("popup.mouse_capture_banner_menu.suppress_suffix"),
                ),
                "{code}"
            );
        }
    }

    /// 이름 미상 예제는 본체와 같은 세 키를 그 언어 파일에서 읽는다.
    #[test]
    fn unknown_examples_read_the_app_copy_of_that_language() {
        for code in tasty_i18n::BUILTIN_CODES {
            let lang = crate::i18n::catalog_value(code);
            let menu = &lang["popup"]["mouse_capture_banner_menu"];
            let key = |k: &str| menu[k].as_str().expect("menu key").to_string();
            let copy = super::unknown_texts(code);
            assert_eq!(copy.reason, key("unknown_reason"), "{code}");
            assert_eq!(copy.rows[0], key("notice_unknown"), "{code}");
            assert_eq!(copy.rows[1], key("capture_unknown"), "{code}");
        }
    }

    /// 시안 "Row copy": 언어마다 프로그램 이름의 앞(prefix)과 뒤(suffix) 중 어느 쪽에 문구가 붙는지
    /// 정해져 있다. 값은 (prefix 있음, suffix 있음)이다. 언어를 등록하면 시안의 어순을 확인해 이 표에 더한다.
    const NAME_FIRST: &[(&str, (bool, bool))] = &[
        ("en", (true, false)),
        ("ko", (false, true)),
        ("ja", (false, true)),
        ("zh-Hans", (true, true)),
    ];

    /// 문구는 시안이 정한 쪽에만 붙고, 양쪽이 모두 빈 언어는 없다. 앞 문구는 이름 앞에 공백 하나를 두며,
    /// 양쪽에 문구가 있는 언어는 이름과 뒤 문구 사이에도 공백 하나를 둔다.
    #[test]
    fn each_language_names_the_program_on_its_designed_side() {
        let listed: Vec<&str> = NAME_FIRST.iter().map(|(code, _)| *code).collect();
        assert_eq!(
            listed,
            tasty_i18n::BUILTIN_CODES,
            "NAME_FIRST에 내장 언어의 어순을 빠짐없이 적어야 한다"
        );
        for (code, (has_prefix, has_suffix)) in NAME_FIRST {
            assert!(*has_prefix || *has_suffix, "{code}: 양쪽이 모두 비었다");
            for (prefix, suffix) in locale_texts(code) {
                if *has_prefix {
                    assert!(
                        prefix.ends_with(' ') && prefix.trim() != "",
                        "{code} {prefix:?}"
                    );
                } else {
                    assert!(prefix.is_empty(), "{code} {prefix:?}");
                }
                if *has_suffix {
                    assert!(!suffix.trim().is_empty(), "{code} {suffix:?}");
                } else {
                    assert!(suffix.is_empty(), "{code} {suffix:?}");
                }
                if *has_prefix && *has_suffix {
                    assert!(suffix.starts_with(' '), "{code} {suffix:?}");
                }
            }
        }
    }
}
