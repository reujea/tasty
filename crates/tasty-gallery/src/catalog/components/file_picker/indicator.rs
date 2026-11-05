//! 원격 대상 표시 방식 세 후보(배지 · 글리프 + 호스트 · 프레임 테두리)의 비교 예제.
//! 본체와 기본 예제는 A안(배지)을 쓰고, B·C안은 비교를 위해서만 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::{FpState, HOST, Variant, card, host_badge};
use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};

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
        Indicator::Badge => host_badge(ui, theme, HOST),
        Indicator::Glyph => {
            ui.label(
                egui::RichText::new(HOST)
                    .font(egui::FontId::monospace(
                        GLYPH_HOST_FONT_PRIMITIVE_12.value(),
                    ))
                    .color(theme.accent_info_raised().to_egui()),
            );
        }
        Indicator::Border => {}
    }
}
