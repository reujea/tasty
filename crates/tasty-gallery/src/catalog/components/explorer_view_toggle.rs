//! 탐색기 보기 전환(SegToggle)이 세그먼트 규칙(채움)을 따른다는 결론 예제.
//! Mocha·Latte 두 테마에서 켜짐·꺼진 칸 hover·다른 값 켜짐을 정적으로 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{ControlSize, PathField};

use super::explorer_toolbar::seg_toggle;
use super::glyph;
use crate::catalog::spec::{self, StageVariant, TokenChip, wrap_item};

/// 시안 테마 상자의 안쪽 여백(`padding: 14`)과 줄 간격(`gap: 10`). 대응 토큰이 없는 리터럴이다.
const BOX_PAD: LogicalPx = LogicalPx(14.0);
const BOX_GAP: LogicalPx = LogicalPx(10.0);
/// 시안 줄 설명 칸 폭(`width: 190`). 전시 치수다.
const CAPTION_W: LogicalPx = LogicalPx(190.0);
/// 시안 무대의 테마 상자 사이 간격(`gap: 18`). 대응 토큰이 없다.
const THEME_GAP: LogicalPx = LogicalPx(18.0);
/// egui `TextEdit` 의 기본 좌우 안쪽 여백(`Margin::symmetric(4, 2)`). 공용 Input 이 이 값을 바꾸지
/// 않으므로 경로 필드의 내용 폭에 더한다.
const TEXT_EDIT_MARGIN_X: LogicalPx = LogicalPx(4.0);

const PATH: &str = "~/Downloads";

/// (줄 설명, 켜진 칸, hover 칸). 칸 번호는 grid 0 · list 1 · detail 2.
const ROWS: &[(&str, usize, Option<usize>)] = &[
    ("on = detail · off = grid, list", 2, None),
    ("hover on an off segment (list)", 2, Some(1)),
    ("on = grid", 0, None),
];

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ];
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(THEME_GAP.value(), THEME_GAP.value());
        for (label, th) in &themes {
            wrap_item(ui, |ui| theme_box(ui, th, label));
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "verdict",
                "a VALUE — segment recipe (fill), same as remote Connection",
            ),
            (
                "on",
                "segtoggle-on-bg → accent-primary · segtoggle-on-fg → text-on-accent",
            ),
            ("off", "text-muted glyph, no fill"),
            ("hover (off)", "overlay-hover"),
            (
                "container",
                "surface-raised + border-default 1px · radius — unchanged; width = pad*2 + seg*3 + gap*2 + border*2",
            ),
            (
                "ink contrast",
                "Mocha crust on blue ≈ 9.4:1 · Latte white on blue ≈ 4.8:1 — AA in both",
            ),
            ("motion", "0ms — click swaps immediately"),
            (
                "surface-active",
                "row selection only — not a segment fill, not a tab fill",
            ),
        ],
        &[
            TokenChip::new(
                "segtoggle-on-bg",
                "selected segment fill",
                theme.segtoggle_on_bg().to_egui(),
            ),
            TokenChip::new(
                "segtoggle-on-fg",
                "selected segment glyph",
                theme.segtoggle_on_fg().to_egui(),
            ),
            TokenChip::new(
                "text-muted",
                "off glyph",
                theme.text_muted_raised().to_egui(),
            ),
            TokenChip::new(
                "surface-raised",
                "container",
                theme.surface_raised().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Rule, extended: underline switches content (tab strip); fill picks a value (segment) \
         — including a value that only changes how the same content is drawn. surface-active \
         is row selection and belongs to neither. No third branch is needed; the word \
         \u{201c}view\u{201d} in a variable name does not make a control a tab strip.",
    );

    spec::note(
        ui,
        theme,
        "Drawn with the body parts: the toggle is the body SegToggle at pad 4 · gap 4 · 24×20 \
         cells, the same as the kit, and the address is the shared PathField at padding 12, \
         gap 8 and mono 11 with 8 before Go (the kit draws padding 0 8, gap 6, mono 12 and 6 \
         before Go).",
    );

    spec::dont(
        ui,
        theme,
        "Don't draw a 2px underline inside the toggle container — the segments sit on the \
         container's own border and an underline would collide with it; that vocabulary \
         belongs to tab strips at the top of a surface.",
    );
}

/// 테마 하나의 상자 — 테마 이름과 세 줄.
fn theme_box(ui: &mut egui::Ui, th: &Theme, label: &str) {
    egui::Frame::new()
        .fill(th.bg_app().to_egui())
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.border_default().to_egui(),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(BOX_PAD.value() as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = BOX_GAP.value();
                ui.label(
                    egui::RichText::new(label)
                        .size(th.font_size_caption.value())
                        .color(th.text_muted().to_egui()),
                );
                for (i, &(caption, on, hover)) in ROWS.iter().enumerate() {
                    ui.push_id((label, i), |ui| row(ui, th, caption, on, hover));
                }
            });
        });
}

/// 도구 모음 띠(경로 필드 + 보기 전환)와 오른쪽 설명 한 줄.
fn row(ui: &mut egui::Ui, th: &Theme, caption: &str, on: usize, hover: Option<usize>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_md.value();
        egui::Frame::new()
            .fill(th.bg_panel().to_egui())
            .stroke(egui::Stroke::new(
                th.border_width.value(),
                th.border_default().to_egui(),
            ))
            .corner_radius(th.corner_radius.value())
            .inner_margin(egui::Margin::symmetric(th.spacing_sm.value() as i8, 0))
            .show(ui, |ui| {
                let h = th.item_height_interactive.value() + th.spacing_sm.value() * 2.0;
                ui.set_height(h);
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                    path_field(ui, th);
                    seg_toggle(ui, th, on, hover);
                });
            });
        ui.allocate_ui_with_layout(
            egui::vec2(CAPTION_W.value(), ui.available_height()),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(CAPTION_W.value());
                ui.label(
                    egui::RichText::new(caption)
                        .size(th.font_size_caption.value())
                        .color(th.text_muted().to_egui()),
                );
            },
        );
    });
}

/// 편집하지 않는 경로 필드. 시안은 `flex: 1; min-width: 0` 이고 고정 폭이 없어 공용
/// PathField 의 내용 폭으로 잡는다: Input 좌우 여백 · 아이콘 · Input 간격 · 경로 글자 ·
/// TextEdit 좌우 여백 · 필드와 Go 사이 간격 · Go 버튼.
fn path_field(ui: &mut egui::Ui, th: &Theme) {
    let text_w = ui
        .painter()
        .layout_no_wrap(
            PATH.to_owned(),
            egui::FontId::monospace(th.font_size_caption.value()),
            egui::Color32::PLACEHOLDER,
        )
        .rect
        .width();
    let width = th.input_padding_x().value() * 2.0
        + th.icon_glyph_size_md.value()
        + th.input_gap().value()
        + text_w
        + TEXT_EDIT_MARGIN_X.value() * 2.0
        + th.spacing_sm.value()
        + ControlSize::Sm.height(th);
    let folder_icon = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        glyph::FOLDER_OPEN
            .image(rect.height(), c)
            .paint_at(ui, rect);
    };
    let go_icon = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        glyph::ARROW_RIGHT
            .image(rect.height(), c)
            .paint_at(ui, rect);
    };
    let mut buf = PATH.to_string();
    let mut editing = false;
    let mut active = None;
    PathField::new("gallery_exp_view_toggle_addr")
        .width(width)
        .leading_icon(&folder_icon)
        .row_icon(&folder_icon)
        .go_icon(&go_icon)
        .show(ui, th, &mut buf, &mut editing, &mut active, &[], PATH);
}
