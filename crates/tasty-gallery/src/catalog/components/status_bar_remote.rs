//! 포커스 surface 가 mirror 일 때 상태바 좌측 맨 앞에 붙는 원격 위치 항목의 예제.
//! 실제 `draw_status_bar_view` 를 Mocha·Latte 두 테마로 나란히 그린다. 축소 단계는 위젯이
//! 폭에 따라 고르며, 예제 폭은 디자인 표본의 폭이다.
//! 호스트 글자는 attach 프로필 이름이다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{StatusBarData, StatusBarRemote, draw_status_bar_view};

use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage, wrap_item};

/// attach 프로필 이름 표본(결정된 호스트 글자).
const HOST: &str = "gx10";
const MID: &str = "mac-company-attach";
const LONG: &str = "build-runner-eu-west-07-staging-attach";

/// 디자인 표본(BAR_ROWS)의 바 폭. 축소 단계를 보이려고 고른 표본 폭이며 제품 치수가 아니다.
const BAR_W: LogicalPx = LogicalPx(470.0);
const NARROW_W: LogicalPx = LogicalPx(300.0);
const NARROWER_W: LogicalPx = LogicalPx(230.0);
const FLOOR_W: LogicalPx = LogicalPx(120.0);
const FLOOR_RECONNECTING_W: LogicalPx = LogicalPx(200.0);

/// 원격 항목. 문구는 본체와 같은 번역 키에서 읽는다.
fn remote(host: &str, reconnecting: bool) -> StatusBarRemote {
    StatusBarRemote {
        host: host.into(),
        reconnecting,
        empty_label: crate::i18n::t("attach.mirror_pill_label").into(),
        reconnecting_label: crate::i18n::t("status_bar.remote_reconnecting").into(),
        tooltip: crate::i18n::t(if reconnecting {
            "attach.toast.mirror_reconnecting"
        } else {
            "attach.mirror_workspace"
        })
        .into(),
    }
}

/// mirror 바 — 브랜치와 셸은 없고(mirror 에서는 원래 없다) surface id·그리드·팔레트·테마가 있다.
fn mirror_bar(theme: &Theme, remote: Option<StatusBarRemote>) -> StatusBarData {
    StatusBarData {
        remote,
        branch: None,
        surface_id: Some(3),
        pane_id: Some(1),
        shell: None,
        grid: Some((120, 32)),
        theme_is_light: theme.is_light,
        palette_keys: "Ctrl+K".into(),
        palette_tooltip: crate::i18n::t("status_bar.palette_tooltip").into(),
        theme_tooltip: crate::i18n::t("status_bar.theme_tooltip").into(),
    }
}

/// 로컬 바 — 원격 항목이 없다.
fn local_bar(theme: &Theme) -> StatusBarData {
    StatusBarData {
        branch: Some("feat/dag-detail".into()),
        shell: Some("zsh".into()),
        ..mirror_bar(theme, None)
    }
}

/// 한 줄 — 라벨과 주어진 폭의 바. 카드가 더 좁으면 카드 폭이 이긴다.
fn row(ui: &mut egui::Ui, theme: &Theme, label: &str, w: LogicalPx, data: &StatusBarData) {
    ui.label(
        egui::RichText::new(label)
            .size(theme.font_size_caption.value())
            .color(egui::Color32::from(theme.text_muted())),
    );
    let w = LogicalPx(ui.available_width()).min(w);
    draw_status_bar_view(ui, theme, w, data);
}

/// 한 테마의 표본 묶음. 바탕은 그 테마의 bg-app 이다.
fn theme_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    rows: &[(&str, LogicalPx, StatusBarData)],
) {
    wrap_item(ui, |ui| theme_frame(ui, theme, name, rows));
}

fn theme_frame(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    rows: &[(&str, LogicalPx, StatusBarData)],
) {
    egui::Frame::new()
        .fill(egui::Color32::from(theme.bg_app()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            egui::Color32::from(theme.border_default()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(470.0);
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                ui.label(
                    egui::RichText::new(name)
                        .size(theme.font_size_caption.value())
                        .color(egui::Color32::from(theme.text_secondary())),
                );
                for (label, w, data) in rows {
                    row(ui, theme, label, *w, data);
                }
            });
        });
}

/// 기본 표본 — 디자인 Spec 의 BAR_ROWS 와 같은 줄.
fn main_rows(theme: &Theme) -> Vec<(&'static str, LogicalPx, StatusBarData)> {
    let bar = |host: &str, reconnecting: bool| mirror_bar(theme, Some(remote(host, reconnecting)));
    vec![
        ("local surface — no remote item", BAR_W, local_bar(theme)),
        ("mirror · connected — profile gx10", BAR_W, bar(HOST, false)),
        (
            "mirror · connected — profile mac-company-attach",
            BAR_W,
            bar(MID, false),
        ),
        (
            "mirror · reconnecting — neutral ink + status word",
            BAR_W,
            bar(MID, true),
        ),
        (
            "long profile name — capped at 160, end-elided",
            BAR_W,
            bar(LONG, false),
        ),
        (
            "narrow — grid and surface id drop",
            NARROW_W,
            bar(LONG, false),
        ),
        ("narrower — palette cap drops", NARROWER_W, bar(LONG, false)),
        (
            "floor — host text drops, glyph stays",
            FLOOR_W,
            bar(LONG, false),
        ),
        (
            "floor · reconnecting — the status word stays",
            FLOOR_RECONNECTING_W,
            bar(LONG, true),
        ),
    ]
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let mocha = tasty_themes::mocha_fallback();
    let latte = crate::host_shell::latte_theme();
    stage(ui, theme, StageVariant::Wrap, |ui| {
        theme_card(ui, &mocha, "Mocha", &main_rows(&mocha));
        theme_card(ui, &latte, "Latte", &main_rows(&latte));
    });

    meta(
        ui,
        theme,
        &[
            (
                "shown",
                "focused surface is in a mirror workspace — local surface: absent",
            ),
            ("position", "first in the left cluster, before branch"),
            (
                "connected",
                "remote glyph 12 + host · both statusbar-remote-fg",
            ),
            (
                "while reconnecting",
                "glyph statusbar-glyph · host text-muted · then the status word in font-label, text-muted",
            ),
            ("gap", "statusbar-remote-gap 4 — glyph ↔ host ↔ word"),
            (
                "host text",
                "attach profile name, as it was at connect time",
            ),
            ("host font", "mono caption (a config name)"),
            (
                "host cap",
                "statusbar-remote-max-width 160 on the host text · end-elided with …",
            ),
            (
                "no host text",
                "the word Remote (attach.mirror_pill_label) in the host slot, font-label",
            ),
            (
                "drop order",
                "1 grid → 2 shell → 3 surface id → 4 palette cap → 5 branch text → 6 host text (remote glyph stays)",
            ),
            (
                "tooltip",
                "bar tooltip: attach.mirror_workspace or attach.toast.mirror_reconnecting, then the full host in mono",
            ),
            ("click", "none — read-only"),
            ("refresh", "follows the session state on the next read"),
        ],
        &[
            TokenChip::new(
                "statusbar-remote-fg",
                "glyph + host, connected",
                egui::Color32::from(theme.statusbar_remote_fg()),
            ),
            TokenChip::new(
                "statusbar-glyph",
                "glyph, reconnecting",
                egui::Color32::from(theme.statusbar_glyph()),
            ),
            TokenChip::new(
                "text-muted",
                "host + word, reconnecting",
                egui::Color32::from(theme.text_muted()),
            ),
            TokenChip::without_color("statusbar-remote-gap", "4"),
            TokenChip::without_color("statusbar-remote-max-width", "160 host cap"),
        ],
    );
    note(
        ui,
        theme,
        "The remote hue is the only colour in the bar. It is an identity (where this surface \
         lives), not a status, so it is never a dot. Losing it on reconnect is the signal, and \
         the word says why.",
    );
    dont(
        ui,
        theme,
        "Don't animate the reconnecting state (no spinner, no pulse). The bar reads once a \
         second and carries nothing that changes faster.",
    );
}
