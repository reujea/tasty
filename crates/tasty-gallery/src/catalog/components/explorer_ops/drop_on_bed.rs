//! 포커스 바탕 위의 놓을 대상. 칠 순서는 바탕, 틴트, 행 fill, 잉크, 링이다. 글자는 틴트
//! 아래가 아니라 위에 있으므로 `drop-target@*` 칸의 토큰 값 그대로다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    RowParent, RowState, explorer_match_job, paint_drop_ring, paint_drop_tint, surface_bed,
};

use super::{framed, lbl, themes};
use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{StageVariant, TokenChip, meta, stage};

const W: LogicalPx = LogicalPx(420.0);

struct Row {
    glyph: MockGlyph,
    name: &'static str,
    query: &'static str,
    date: &'static str,
    image: bool,
}

const ROWS: [Row; 3] = [
    Row {
        glyph: icons::FOLDER,
        name: "mockup-exports",
        query: "",
        date: "2026-10-02 18:40",
        image: false,
    },
    Row {
        glyph: icons::FILE,
        name: "report.pdf",
        query: "re",
        date: "2026-06-24 09:12",
        image: false,
    },
    Row {
        glyph: icons::IMAGE,
        name: "screenshot.png",
        query: "",
        date: "2026-08-11 09:30",
        image: true,
    },
];

fn listing(ui: &mut egui::Ui, th: &Theme, focused: bool, body: bool) {
    let row_h = th.table_cell_height().value();
    let size = egui::vec2(W.value(), row_h * ROWS.len() as f32);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let row_at = |i: usize| {
        egui::Rect::from_min_size(
            rect.min + egui::vec2(0.0, row_h * i as f32),
            egui::vec2(rect.width(), row_h),
        )
    };
    let (target, radius) = if body {
        (rect, 0.0)
    } else {
        (row_at(0), th.corner_radius.value())
    };
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, surface_bed(th, focused));
    paint_drop_tint(&p, th, target, radius);
    let parent = RowParent::bed(focused);
    let inset = th.spacing_md.value();
    let sz = th.icon_glyph_size_md.value();
    for (i, row) in ROWS.iter().enumerate() {
        let at = row_at(i);
        let state = RowState::Rest.under_drop(body || i == 0);
        let tinted = state == RowState::DropTarget;
        let glyph_fg = if row.image && !tinted {
            th.accent_info_raised()
        } else {
            state.glyph(th)
        };
        let glyph_rect = egui::Rect::from_center_size(
            egui::pos2(at.left() + inset + sz / 2.0, at.center().y),
            egui::Vec2::splat(sz),
        );
        row.glyph
            .image(sz, glyph_fg.to_egui())
            .paint_at(ui, glyph_rect);
        let ink = state.label(th, parent, th.table_row_fg()).to_egui();
        let font = egui::FontId::proportional(th.font_size_body.value());
        let name = p.layout_job(explorer_match_job(
            th, row.name, row.query, font, ink, false,
        ));
        let name_at = egui::pos2(
            glyph_rect.right() + th.spacing_sm.value(),
            at.center().y - name.rect.height() / 2.0,
        );
        p.galley(name_at, name, ink);
        p.text(
            egui::pos2(at.right() - inset, at.center().y),
            egui::Align2::RIGHT_CENTER,
            row.date,
            egui::FontId::monospace(th.font_size_caption.value()),
            state.meta(th, parent).to_egui(),
        );
    }
    paint_drop_ring(&p, th, target, radius);
}

pub fn draw_drop_target_on_the_bed(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Solo, |ui| {
        themes(ui, theme, |ui, th| {
            for (label, focused, body) in [
                ("Focused bed · folder row", true, false),
                ("Focused bed · list body", true, true),
                ("Unfocused bed · folder row", false, false),
                ("Unfocused bed · list body", false, true),
            ] {
                lbl(ui, th, label);
                ui.push_id(label, |ui| {
                    framed(ui, th, |ui| listing(ui, th, focused, body));
                });
            }
        });
    });
    meta(
        ui,
        theme,
        &[
            ("paint order", "bed · tint · row fill · ink · ring"),
            ("ink", "the drop-target@* cell, with no tint over it"),
            (
                "image row, list body",
                "glyph → text-secondary · glyph-info is forbidden on the tint",
            ),
            (
                "image row, outside a tint",
                "glyph-info → accent-info-raised",
            ),
            ("match run", "accent-primary-raised"),
            (
                "opaque row fill",
                "hides the tint on that row · the ring still shows",
            ),
        ],
        &[
            TokenChip::new(
                "explorer-drop-target-bg",
                "tint, under the row",
                theme.explorer_drop_target_bg().to_egui(),
            ),
            TokenChip::new(
                "explorer-drop-target-border",
                "ring, last",
                theme.explorer_drop_target_border().to_egui(),
            ),
        ],
    );
}
