//! 설정 › 일반 › 원격 attach 탭의 예제. 실제 설정 저장은 하지 않는다.
//!
//! 시안 `SettingsRemoteAttachFrame`의 콘텐츠 컬럼이다. 헤딩과 숫자 행 둘(끊기 전 대기 시간, 재연결 시도
//! 시간)을 평소 값과 범위 밖 입력으로 Mocha·Latte에 나란히 보이고 ko·ja, 좁은 폭의 쌓인 행을 함께 둔다.
//! 범위 밖 입력은 첫 행에만 넣고 둘째 행은 기본값 300을 보인다.

use std::cell::RefCell;

use tasty_settings::{HEARTBEAT_TIMEOUT_SECS_RANGE, RECONNECT_TIMEOUT_SECS_RANGE};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Input, SettingsRow, settings_label_column};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

use super::settings_remote_transfer::WIDTH;

/// 칸이 라벨 옆에 들어가지 않는 전시용 프레임 폭. 대응하는 고정 치수는 없고, 쌓을지는 `settings_stack_row`가
/// 남은 폭으로 정한다.
const STACKED_WIDTH: LogicalPx = LogicalPx(280.0);

#[derive(Clone, Copy)]
enum Ground {
    Mocha,
    Latte,
}

struct Specimen {
    ground: Ground,
    lang: &'static str,
    caption: &'static str,
    typed: &'static str,
    width: LogicalPx,
}

const fn specimen(
    ground: Ground,
    lang: &'static str,
    caption: &'static str,
    typed: &'static str,
    width: LogicalPx,
) -> Specimen {
    Specimen {
        ground,
        lang,
        caption,
        typed,
        width,
    }
}

const SPECIMENS: [Specimen; 12] = [
    specimen(Ground::Mocha, "en", "Mocha · rest · 30", "30", WIDTH),
    specimen(Ground::Latte, "en", "Latte · rest · 30", "30", WIDTH),
    specimen(
        Ground::Mocha,
        "en",
        "Mocha · 5 typed · saves 10",
        "5",
        WIDTH,
    ),
    specimen(
        Ground::Latte,
        "en",
        "Latte · 5 typed · saves 10",
        "5",
        WIDTH,
    ),
    specimen(
        Ground::Mocha,
        "en",
        "Mocha · 1000 typed · saves 600",
        "1000",
        WIDTH,
    ),
    specimen(
        Ground::Latte,
        "en",
        "Latte · 1000 typed · saves 600",
        "1000",
        WIDTH,
    ),
    specimen(Ground::Mocha, "ko", "Mocha · ko · rest", "30", WIDTH),
    specimen(Ground::Latte, "ko", "Latte · ko · 5 typed", "5", WIDTH),
    specimen(Ground::Mocha, "ja", "Mocha · ja · rest", "30", WIDTH),
    specimen(
        Ground::Latte,
        "ja",
        "Latte · ja · 1000 typed",
        "1000",
        WIDTH,
    ),
    specimen(
        Ground::Mocha,
        "en",
        "Mocha · en · stacked · 5 typed",
        "5",
        STACKED_WIDTH,
    ),
    specimen(
        Ground::Latte,
        "ja",
        "Latte · ja · stacked",
        "30",
        STACKED_WIDTH,
    ),
];

thread_local! {
    static BUFS: RefCell<Vec<(String, String)>> = RefCell::new(
        SPECIMENS
            .iter()
            .map(|s| (s.typed.to_owned(), "300".to_owned()))
            .collect(),
    );
}

/// 카탈로그 등록 항목.
pub fn entry() -> crate::catalog::Spec {
    crate::catalog::spec(
        "settings-remote-attach",
        "General › Remote attach",
        Some(
            "Attach connections · two integer rows (10–600 and 0–3600, mono s) · range line only when out of range",
        ),
        draw,
    )
}

fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        BUFS.with(|b| {
            let bufs = &mut *b.borrow_mut();
            for (i, (s, buf)) in SPECIMENS.iter().zip(bufs.iter_mut()).enumerate() {
                let th = match s.ground {
                    Ground::Mocha => &mocha,
                    Ground::Latte => &latte,
                };
                spec::wrap_item(ui, |ui| {
                    ui.push_id(i, |ui| frame(ui, th, s, buf));
                });
            }
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "L2 position",
                "6th — after Remote transfer · macOS Display and Permissions follow",
            ),
            (
                "heading",
                "font-label micro uppercase · letter-spacing-caps · muted",
            ),
            (
                "rows",
                "two · Disconnect after, then Keep reconnecting for · SettingsRow + shared integer field",
            ),
            (
                "field",
                "mono Input that fits its value (number-field-min-width 90 floor) · right-aligned · static mono unit",
            ),
            (
                "range",
                "10 … 600 · default 30 / 0 … 3600 · default 300 (0 is valid) · clamp on commit",
            ),
            (
                "range line",
                "only when out of range (danger) · no range_note · space-xs under the field",
            ),
            (
                "caption",
                "under its row · states when a saved value applies and the default",
            ),
        ],
        &[
            TokenChip::without_color("settings-row-gap", "heading → row"),
            TokenChip::without_color("settings-row-caption-gap", "row → caption"),
            TokenChip::without_color("settings-row-min-height", "row height"),
            TokenChip::without_color("number-field-min-width", "→ field-width-xs 90"),
            TokenChip::without_color("number-field-unit-gap", "field → unit"),
            TokenChip::new(
                "text-muted",
                "heading · unit · caption",
                theme.text_muted().to_egui(),
            ),
            TokenChip::new(
                "accent-danger-raised",
                "range line",
                theme.accent_danger_raised().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "The screen names what the user chooses and leaves out the signal that keeps the connection \
         alive. Each Tasty uses its own value, so the caption does not promise that the two sides agree.",
    );
}

/// 한 칸의 설정 본문. 본체 `draw_remote_attach_tab`과 같은 위젯과 토큰을 쓴다.
fn frame(ui: &mut egui::Ui, th: &Theme, s: &Specimen, bufs: &mut (String, String)) {
    let copy = |key: &'static str| match s.lang {
        "en" => crate::i18n::t(key),
        code => crate::i18n::t_lang(code, key),
    };
    kit::frame_card_flat(ui, th, s.width, kit::panel_fill(th), |ui| {
        kit::region_sym(ui, th.spacing_lg, th.spacing_md, |ui| {
            ui.spacing_mut().item_spacing.y = th.settings_row_gap().value();
            ui.label(
                egui::RichText::new(s.caption)
                    .size(th.font_size_caption.value())
                    .color(th.text_muted().to_egui()),
            );
            ui.label(
                egui::RichText::new(copy("settings.remote_attach.section").to_uppercase())
                    .family(tasty_ui_widgets::label_font_family())
                    .size(th.font_size_micro.value())
                    .extra_letter_spacing(th.letter_spacing_caps(th.font_size_micro).value())
                    .color(th.text_muted().to_egui()),
            );
            let row = SettingsRow::new(copy("settings.remote_attach.heartbeat_timeout"))
                .caption(copy("settings.remote_attach.heartbeat_timeout_desc"));
            let reconnect = SettingsRow::new(copy("settings.remote_attach.reconnect_window"))
                .caption(copy("settings.remote_attach.reconnect_window_desc"));
            let col = settings_label_column(ui, th, [&row, &reconnect]);
            let unit = copy("settings.number.unit_s");
            let range = copy("settings.number.range_between");
            row.show(ui, th, col, |ui| {
                seconds_control(
                    ui,
                    th,
                    unit,
                    range,
                    HEARTBEAT_TIMEOUT_SECS_RANGE,
                    &mut bufs.0,
                );
            });
            reconnect.show(ui, th, col, |ui| {
                seconds_control(
                    ui,
                    th,
                    unit,
                    range,
                    RECONNECT_TIMEOUT_SECS_RANGE,
                    &mut bufs.1,
                );
            });
        });
    });
}

/// 숫자 칸과 단위, 범위 밖이면 그 아래 danger 한 줄(본체 숫자 칸과 같은 자리).
fn seconds_control(
    ui: &mut egui::Ui,
    th: &Theme,
    unit: &str,
    range: &str,
    (min, max): (u64, u64),
    buf: &mut String,
) {
    let settled = buf
        .trim()
        .parse::<i64>()
        .ok()
        .map(|v| (v, v.clamp(min as i64, max as i64)))
        .and_then(|(v, c)| (v != c).then_some(c));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = th.number_field_unit_gap().value();
            Input::new()
                .mono(true)
                .align(egui::Align::RIGHT)
                .width(tasty_ui_widgets::number_field_width(ui, th, buf).value())
                .invalid(settled.is_some())
                .show(ui, th, buf);
            ui.label(
                egui::RichText::new(unit)
                    .family(tasty_ui_widgets::label_font_family())
                    .size(th.font_size_caption.value())
                    .color(th.text_muted().to_egui()),
            );
        });
        if let Some(settled) = settled {
            let line = tasty_i18n::fill_args(
                range,
                &[&min.to_string(), &max.to_string(), &settled.to_string()],
            );
            ui.label(
                egui::RichText::new(line)
                    .size(th.font_size_caption.value())
                    .color(th.accent_danger_raised().to_egui()),
            );
        }
    });
}
