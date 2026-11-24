//! 원격 대상 표시 방식 세 후보(배지 · 글리프 + 호스트 · 프레임 테두리)의 비교 예제.
//! 본체와 기본 예제는 A안(배지)을 쓰고, B·C안은 비교를 위해서만 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::{FpState, Variant, card};
use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// C안의 프레임 위 띠 높이. 시안 `FilePickerFrame` 의 2px 리터럴이며 대응 토큰이 없다.
const BORDER_STRIP_H: LogicalPx = LogicalPx(2.0);

/// B안에서 제목 옆에 적는 호스트 글자 크기. 시안 `fontSize: 12` 리터럴이며 대응 UI 토큰이 없다.
const GLYPH_HOST_FONT_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

/// 원격 표시 방식 — 시안 `FilePickerFrame indicator`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Indicator {
    /// A안 — 제목 옆 mono `user@host` 칩(채택).
    Badge,
    /// B안 — 헤더 글리프를 remote 로 바꾸고 호스트를 글자로 적는다.
    Glyph,
    /// C안 — 프레임 테두리와 위 2px 띠를 info 색으로 칠한다.
    Border,
}

pub fn draw_remote_indicator(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, ind) in [
            ("A · host badge", Indicator::Badge),
            ("B · glyph + host", Indicator::Glyph),
            ("C · frame border", Indicator::Border),
        ] {
            spec::cluster(ui, theme, label, |ui| {
                card(
                    ui,
                    theme,
                    Variant::open(FpState::Loaded, true, false).indicated(ind),
                );
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("A badge", "mono host chip · info-tinted · explicit"),
            ("B glyph", "remote icon + inline host · lightest"),
            ("C border", "info edge + 2px top strip · loudest"),
            ("shared axis", "accent-info (never danger)"),
            ("local", "no indicator · file glyph · / root"),
        ],
        &[
            TokenChip::new(
                "accent-info",
                "all three indicators",
                theme.accent_info().to_egui(),
            ),
            TokenChip::new("font-mono", "host string", theme.text_secondary().to_egui()),
        ],
    );

    spec::dont(
        ui,
        theme,
        "Don't use a danger/warning tone for the remote indicator — remote is a normal, \
         expected mode, not an error. Reserve peach/red for the connection-lost state below.",
    );
}

/// C안이면 프레임 맨 위에 info 색 띠를 그리고 그 높이를 돌려준다. 다른 안은 0.
pub(super) fn top_strip(ui: &mut egui::Ui, theme: &Theme, v: Variant) -> LogicalPx {
    if !v.border_mode() {
        return LogicalPx(0.0);
    }
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(v.w.value(), BORDER_STRIP_H.value()),
        egui::Sense::hover(),
    );
    // 시안은 프레임의 overflow hidden 으로 띠의 위 모서리를 둥글게 자른다.
    let r = theme.corner_radius.value() as u8;
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        theme.accent_info().to_egui(),
    );
    BORDER_STRIP_H
}

/// 헤더 글리프와 색. 원격 B·C안은 remote 글리프, B안만 info 색이다.
pub(super) fn header_glyph(theme: &Theme, v: Variant) -> (MockGlyph, egui::Color32) {
    if matches!(v.mode, super::Mode::Folder { .. }) {
        return (icons::FOLDER, theme.text_muted().to_egui());
    }
    match (v.remote, v.indicator) {
        (true, Indicator::Glyph) => (icons::REMOTE, theme.accent_info().to_egui()),
        (true, Indicator::Border) => (icons::REMOTE, theme.text_muted().to_egui()),
        _ => (icons::FILE, theme.text_muted().to_egui()),
    }
}

/// 제목 뒤에 붙는 원격 표시. A안은 배지, B안은 mono 호스트 글자, C안은 없다.
pub(super) fn after_title(ui: &mut egui::Ui, theme: &Theme, v: Variant) {
    if !v.remote {
        return;
    }
    match v.indicator {
        Indicator::Badge => host_badge(
            ui,
            theme,
            v.host,
            t("attach.mirror_pill_label"),
            tasty_ui_widgets::label_font_family(),
        ),
        Indicator::Glyph => {
            let (text, font) = host_text(
                v.host,
                t("attach.mirror_pill_label"),
                tasty_ui_widgets::label_font_family(),
                GLYPH_HOST_FONT_PRIMITIVE_12.value(),
            );
            ui.label(
                egui::RichText::new(text)
                    .font(font)
                    .color(theme.accent_info_raised().to_egui()),
            );
        }
        Indicator::Border => {}
    }
}

/// 디자인의 숨은 View `file-picker-remote-states-no-profile` — 빈 호스트 원격의 로딩과 두 연결 끊김 원인.
/// 본문은 앱의 호스트 없는 값이고, 제목 배지는 낱말 Remote다.
pub(super) fn draw_no_profile_states_view(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing =
            egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
        spec::cluster(
            ui,
            theme,
            "View · file-picker-remote-states-no-profile — no profile name (--ssh or --target-port from the CLI or IPC): loading and both connection-lost causes with the app's host-free values",
            |ui| {
                for (label, state) in [
                    ("loading (remote, no profile name)", FpState::Loading),
                    (
                        "connection lost — session lost (remote, no profile name)",
                        FpState::ErrorConn,
                    ),
                    (
                        "connection lost — timeout (remote, no profile name)",
                        FpState::ErrorConnTimeout,
                    ),
                ] {
                    // 세 프레임은 문서 칸에서 줄을 바꾼다(디자인 flex-wrap).
                    spec::wrap_item(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                        ui.label(
                            egui::RichText::new(label)
                                .size(theme.font_size_caption.value())
                                .color(theme.text_muted().to_egui()),
                        );
                        card(ui, theme, Variant::open(state, true, false).hostless());
                    });
                }
            },
        );
    });
}

/// 디자인의 숨은 View `file-picker-remote-no-profile` — 빈 호스트 원격 프레임과 언어별 배지.
pub(super) fn draw_no_profile_view(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing =
            egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
        spec::cluster(
            ui,
            theme,
            "View · file-picker-remote-no-profile — no profile name (--ssh or --target-port from the CLI or IPC): the word Remote in the host badge, then the badge in every language",
            |ui| {
                card(
                    ui,
                    theme,
                    Variant::open(FpState::Loaded, true, false).hostless(),
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
                    for lang in std::iter::once("en").chain(crate::i18n::translated_languages()) {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                            ui.label(
                                egui::RichText::new(lang)
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                            egui::Frame::new()
                                .fill(theme.bg_panel().to_egui())
                                .stroke(egui::Stroke::new(
                                    theme.border_width.value(),
                                    theme.border_strong().to_egui(),
                                ))
                                .corner_radius(theme.corner_radius.value())
                                .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
                                .show(ui, |ui| {
                                    host_badge(
                                        ui,
                                        theme,
                                        "",
                                        crate::i18n::t_lang(lang, "attach.mirror_pill_label"),
                                        tasty_ui_widgets::label_font_family_for(lang),
                                    );
                                });
                        });
                    }
                });
            },
        );
    });
}

/// 호스트 배지의 글리프 크기(디자인 FpHostBadge의 remote 아이콘 13). 대응 Theme 크기 토큰이 없다.
const HOST_BADGE_GLYPH: LogicalPx = LogicalPx(13.0);
/// 호스트 배지의 글리프 ↔ 글자 간격(디자인 FpHostBadge gap 5). 대응 Theme 간격 토큰이 없다.
const HOST_BADGE_GAP: LogicalPx = LogicalPx(5.0);

/// 원격 호스트를 표시하는 배지. 채움·테두리는 accent-info 틴트, 글리프와 글자 잉크는 accent-info-raised다.
/// 호스트가 비면(프로필 이름이 없는 attach) `empty_label` 낱말을 label 글꼴로 쓴다. 글리프·채움·잉크는 같다.
pub(super) fn host_badge(
    ui: &mut egui::Ui,
    theme: &Theme,
    host: &str,
    empty_label: &str,
    label_family: egui::FontFamily,
) {
    let info = theme.accent_info().to_egui();
    let ink = theme.accent_info_raised().to_egui();
    let (text, font) = host_text(
        host,
        empty_label,
        label_family,
        theme.font_size_caption.value(),
    );
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER);
    let glyph = HOST_BADGE_GLYPH.value();
    let gap = HOST_BADGE_GAP.value();
    let pad_x = theme.spacing_sm.value();
    let h = super::HOST_BADGE_H.value();
    let w = pad_x * 2.0 + glyph + gap + galley.rect.width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let radius = theme.corner_radius.value();
    ui.painter()
        .rect_filled(rect, radius, info.gamma_multiply(theme.tint_fill_alpha()));
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            theme.border_width.value(),
            info.gamma_multiply(theme.tint_border_alpha()),
        ),
        egui::StrokeKind::Inside,
    );
    let gy = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pad_x, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    icons::REMOTE.image(glyph, ink).paint_at(ui, gy);
    let pos = egui::pos2(
        rect.left() + pad_x + glyph + gap,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, ink);
}

/// 호스트 표시 글자와 글꼴. 이름은 mono, 빈 호스트의 낱말은 `label_family`(그 낱말 언어의 label 글꼴)다.
pub(super) fn host_text<'a>(
    host: &'a str,
    empty_label: &'a str,
    label_family: egui::FontFamily,
    size: f32,
) -> (&'a str, egui::FontId) {
    if host.is_empty() {
        (empty_label, egui::FontId::new(size, label_family))
    } else {
        (host, egui::FontId::monospace(size))
    }
}
