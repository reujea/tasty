//! mirror 터미널의 파일 경로 링크 아래 원격 위치 툴팁의 예제.
//! 링크 셀은 모든 surface 와 같은 모양(selection 채움, text-on-active, 밑줄)이고, 툴팁은 본체와
//! 같은 공용 `LinkRemoteTip` 으로 링크 런 아래에 그린다. Mocha·Latte 를 나란히 보인다.
//! 호스트 글자는 attach 프로필 이름이다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::LinkRemoteTip;

use crate::catalog::spec::{StageVariant, TokenChip, do_, dont, meta, stage, wrap_item};

/// (라벨, 호스트, 링크 글자, 툴팁 여부). 호스트가 None 이면 로컬 surface 다.
type Row = (&'static str, Option<&'static str>, &'static str, bool);

const ROWS: [Row; 5] = [
    (
        "local surface — path link, no tooltip",
        None,
        "src/main.rs:42",
        false,
    ),
    (
        "mirror — profile gx10, tooltip says where it lives",
        Some("gx10"),
        "src/main.rs:42",
        true,
    ),
    (
        "mirror — profile mac-company-attach",
        Some("mac-company-attach"),
        "src/main.rs:42",
        true,
    ),
    (
        "mirror — long profile name, end-elided in the tooltip",
        Some("build-runner-eu-west-07-staging-attach"),
        "src/main.rs:42",
        true,
    ),
    (
        "mirror — https link, no tooltip (opens in the local browser)",
        Some("gx10"),
        "https://docs.rs/tokio",
        false,
    ),
];

/// 터미널 한 장 — 두 줄 출력과 링크 런, 그 아래 툴팁 자리.
fn term(ui: &mut egui::Ui, theme: &Theme, row: &Row) {
    let (_, host, link, tip) = *row;
    let surface = theme.surface("terminal");
    let fg: egui::Color32 = surface.focused_fg.into();
    let font = egui::FontId::monospace(theme.font_size_caption.value());
    let line_h = ui.fonts(|f| f.row_height(&font));
    let pad = theme.spacing_sm.value();
    let word = crate::i18n::t("attach.mirror_pill_label");
    let bubble = host.map(|host| LinkRemoteTip { word, host });
    let tip_h = bubble.as_ref().map_or(0.0, |b| {
        b.bubble_size(ui.ctx(), theme).y + theme.tooltip_offset().value()
    });
    let width = ui.available_width().min(420.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, pad * 2.0 + line_h * 2.0 + tip_h),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(surface.focused_bg),
        egui::Stroke::new(
            theme.border_width.value(),
            egui::Color32::from(theme.border_default()),
        ),
        egui::StrokeKind::Inside,
    );
    let left = rect.left() + pad;
    let top = rect.top() + pad;
    painter.text(
        egui::pos2(left, top),
        egui::Align2::LEFT_TOP,
        "$ cargo build",
        font.clone(),
        fg,
    );
    let lead = "error: mismatched types at ";
    let lead_w = ui
        .fonts(|f| f.layout_no_wrap(lead.into(), font.clone(), fg))
        .rect
        .width();
    let y = top + line_h;
    painter.text(
        egui::pos2(left, y),
        egui::Align2::LEFT_TOP,
        lead,
        font.clone(),
        fg,
    );
    // 링크 런 — 모든 surface 와 같은 모양.
    let galley =
        ui.fonts(|f| f.layout_no_wrap(link.into(), font.clone(), theme.text_on_active().into()));
    let run = egui::Rect::from_min_size(
        egui::pos2(left + lead_w, y),
        egui::vec2(galley.rect.width(), line_h),
    );
    painter.rect_filled(run, 0.0, egui::Color32::from(theme.selection_bg));
    painter.galley(run.min, galley, theme.text_on_active().into());
    painter.hline(
        run.x_range(),
        run.bottom() - theme.border_width.value(),
        egui::Stroke::new(
            theme.border_width.value(),
            egui::Color32::from(theme.accent_primary_raised()),
        ),
    );
    if let (true, Some(bubble)) = (tip, bubble) {
        let size = bubble.bubble_size(ui.ctx(), theme);
        let at = tasty_ui_widgets::link_tip_rect(run, size, theme.tooltip_offset().value(), rect);
        bubble.paint(ui, theme, at);
    }
}

fn theme_card(ui: &mut egui::Ui, theme: &Theme, name: &str, rows: &[Row]) {
    wrap_item(ui, |ui| theme_frame(ui, theme, name, rows));
}

fn theme_frame(ui: &mut egui::Ui, theme: &Theme, name: &str, rows: &[Row]) {
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
                ui.set_width(420.0);
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                ui.label(
                    egui::RichText::new(name)
                        .size(theme.font_size_caption.value())
                        .color(egui::Color32::from(theme.text_secondary())),
                );
                for row in rows {
                    ui.label(
                        egui::RichText::new(row.0)
                            .size(theme.font_size_caption.value())
                            .color(egui::Color32::from(theme.text_muted())),
                    );
                    term(ui, theme, row);
                }
            });
        });
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let mocha = tasty_themes::mocha_fallback();
    let latte = crate::host_shell::latte_theme();
    stage(ui, theme, StageVariant::Wrap, |ui| {
        theme_card(ui, &mocha, "Mocha", &ROWS);
        theme_card(ui, &latte, "Latte", &ROWS);
    });
    meta(
        ui,
        theme,
        &[
            (
                "when",
                "link modifier held + pointer on a PATH link in a mirror surface",
            ),
            (
                "not shown",
                "local surface · URL with a scheme (http, https, …) · no link captured",
            ),
            (
                "link cells",
                "unchanged — selection fill, text-on-active, accent-primary-raised underline",
            ),
            (
                "means",
                "tooltip below the link run; above when there is no room; clamped to the window",
            ),
            (
                "content",
                "attach.mirror_pill_label in tooltip-fg · space-sm · remote glyph 12 + host in link-remote-fg",
            ),
            (
                "host",
                "attach profile name · mono · end-elided inside the tooltip max width (240)",
            ),
            ("no host text", "the word + glyph only"),
            (
                "show",
                "tooltip delay, then instant hide on release — no fade on terminal cells",
            ),
        ],
        &[
            TokenChip::new(
                "link-remote-fg",
                "glyph + host",
                egui::Color32::from(theme.link_remote_fg()),
            ),
            TokenChip::without_color("link-remote-gap", "glyph ↔ host"),
            TokenChip::new(
                "tooltip-fg",
                "the word Remote",
                egui::Color32::from(theme.tooltip_fg()),
            ),
            TokenChip::without_color("tooltip-max-width", "240 — the host elides inside it"),
        ],
    );
    dont(
        ui,
        theme,
        "Don't recolour the underline or the selection fill for a remote link. Colour alone does \
         not say which host, and the link grammar is shared by every surface.",
    );
    do_(
        ui,
        theme,
        "Do keep the tooltip off the link run itself, so the path you are about to click stays \
         readable.",
    );
}
