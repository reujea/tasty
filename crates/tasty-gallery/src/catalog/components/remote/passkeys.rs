//! 원격 도구 Passkeys 탭 예제 — 시안 `PasskeyRow` 의 값 가림·보임·모르는 kind 상태를 Mocha·Latte 로 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, IconButton, IconButtonVariant, RemoteRowChip, TextWrap, remote_list_row,
    remote_row_title, selectable_text,
};

use super::{WIDTH, attach_header, tab_bar};
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

struct PasskeySample {
    name: &'static str,
    kind: &'static str,
    value: &'static str,
    revealed: bool,
}

/// 시안 `RemoteFrame tab="passkeys"` 의 네 행 — 가림, 보임(active + eyeOff, 긴 경로는
/// 끝 말줄임), 가림, 모르는 kind. 시안 seed 의 kind `file`·`secret` 은 tasty 의 kind 인
/// `path`·`inline` 으로 옮긴다.
const PASSKEYS: &[PasskeySample] = &[
    PasskeySample {
        name: "ed25519-main",
        kind: "path",
        value: "~/.ssh/id_ed25519",
        revealed: false,
    },
    PasskeySample {
        name: "deploy-key-with-a-very-long-name",
        kind: "path",
        value: "/Users/hyunjun/Library/Application Support/tasty/keys/deploy/ci-runner/id_ed25519_deploy",
        revealed: true,
    },
    PasskeySample {
        name: "vault-token",
        kind: "inline",
        value: "",
        revealed: false,
    },
    PasskeySample {
        name: "legacy",
        kind: "agent-x",
        value: "",
        revealed: false,
    },
];

/// 시안 `KIND_OPTIONS`. 이 밖의 kind 는 Tag 대신 경고 배지로 보인다.
const KNOWN_PASSKEY_KINDS: &[&str] = &["path", "inline"];

/// 값을 가리는 자리표시. 본체와 같은 마스크다.
const PASSKEY_MASK: &str = "••••••••";

pub fn draw_passkeys(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        with_zoom(tasty_themes::mocha_fallback()),
        with_zoom(crate::host_shell::latte_theme()),
    ];
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (i, th) in themes.iter().enumerate() {
            ui.push_id(i, |ui| passkeys_frame(ui, th));
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "row",
                "pad space-md space-xs · name 13 · kind Tag (unknown kind → warn badge)",
            ),
            (
                "value",
                "kind · value-or-mask · mono 11 · one line, ellipsis revealed or not",
            ),
            ("mask", "fixed 8-dot mask while hidden"),
            (
                "long value",
                "ellipsis at the end · actions never pushed out of the row",
            ),
            (
                "actions",
                "reveal · edit · trash — IconButton sm, gap size-1",
            ),
            (
                "revealed",
                "IconButton active (accent glyph + overlay-active) and eye → eyeOff",
            ),
            ("add-bar", "Add passkey — secondary sm + plus"),
        ],
        &[
            TokenChip::new(
                "accent-primary",
                "revealed glyph",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::without_color("overlay-active", "revealed button fill"),
            TokenChip::new("text-muted", "value mono", theme.text_muted().to_egui()),
            TokenChip::without_color("separator", "row dividers"),
        ],
    );
}

/// 시안 `RemoteFrame tab="passkeys"` — 머리 · 탭 · add-bar · 목록.
fn passkeys_frame(ui: &mut egui::Ui, theme: &Theme) {
    kit::frame_card(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
        attach_header(ui, theme);
        tab_bar(ui, theme, 2);

        kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
            ui.horizontal(|ui| {
                Button::new("Add passkey")
                    .variant(ButtonVariant::Secondary)
                    .size(tasty_ui_widgets::ControlSize::Sm)
                    .leading_icon(&|ui, rect, c| {
                        icons::PLUS.image(rect.height(), c).paint_at(ui, rect)
                    })
                    .show(ui, theme);
            });
        });

        kit::region_sym(ui, theme.spacing_md, LogicalPx(0.0), |ui| {
            for k in PASSKEYS {
                passkey_row(ui, theme, k);
            }
        });
    });
}

fn passkey_row(ui: &mut egui::Ui, theme: &Theme, k: &PasskeySample) {
    let kind_chip = if KNOWN_PASSKEY_KINDS.contains(&k.kind) {
        RemoteRowChip::TagMono(k.kind)
    } else {
        RemoteRowChip::Warn {
            text: k.kind,
            tooltip: "Unknown kind.",
        }
    };
    remote_list_row(
        ui,
        theme,
        3,
        |ui, w| {
            remote_row_title(ui, theme, k.name, None, false, &[kind_chip], w);
            let value = if k.revealed { k.value } else { PASSKEY_MASK };
            selectable_text(
                ui,
                &format!("{} · {}", k.kind, value),
                theme.text_muted(),
                theme.font_size_caption.value(),
                true,
                false,
                TextWrap::Truncate(w),
            );
        },
        |ui| {
            for glyph in [icons::TRASH, icons::EDIT] {
                IconButton::new()
                    .variant(IconButtonVariant::Ghost)
                    .size(tasty_ui_widgets::ControlSize::Sm)
                    .show(ui, theme, &|ui, rect, c| {
                        glyph.image(rect.height(), c).paint_at(ui, rect)
                    });
            }
            let reveal = if k.revealed {
                icons::EYE_OFF
            } else {
                icons::EYE
            };
            IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(tasty_ui_widgets::ControlSize::Sm)
                .active(k.revealed)
                .show(ui, theme, &|ui, rect, c| {
                    reveal.image(rect.height(), c).paint_at(ui, rect)
                });
        },
    );
}
