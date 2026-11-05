//! 탐색기 키보드 현재 항목 테두리와 영역 선택 사각형. 디자인 `gallery/explorer-ops-b11.jsx`
//! (§Keyboard current item · drag-select rectangle) 의 예제를 옮겼다. 그리기는 본체와 같은
//! `tasty_ui_widgets::paint_cursor_ring`·`paint_marquee` 가 맡는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{paint_cursor_ring, paint_marquee};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 목록 예제의 행 폭. 전시 치수다.
const ROW_W: LogicalPx = LogicalPx(260.0);
/// 격자 예제의 칸 크기. 전시 치수다.
const GRID_CELL: LogicalPx = LogicalPx(72.0);

const NAMES: [&str; 4] = ["assets", "notes.md", "report.pdf", "src"];

/// 이름 하나를 그린 행. `selected` 면 선택 채움을 깐다.
fn row(ui: &mut egui::Ui, theme: &Theme, name: &str, folder: bool, selected: bool) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ROW_W.value(), theme.item_height_interactive.value()),
        egui::Sense::hover(),
    );
    if selected {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius.value(),
            theme.surface_active().to_egui(),
        );
    }
    let g = theme.icon_glyph_size_sm.value();
    let glyph = egui::Rect::from_center_size(
        egui::pos2(
            rect.left() + theme.spacing_sm.value() + g / 2.0,
            rect.center().y,
        ),
        egui::vec2(g, g),
    );
    let icon = if folder { icons::FOLDER } else { icons::FILE };
    let glyph_fg = if selected {
        theme.text_on_active()
    } else {
        theme.text_muted()
    };
    icon.image(g, glyph_fg.to_egui()).paint_at(ui, glyph);
    let fg = if selected {
        theme.text_on_active()
    } else {
        theme.text_secondary()
    };
    ui.painter().text(
        egui::pos2(glyph.right() + theme.spacing_sm.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(theme.font_size_body.value()),
        fg.to_egui(),
    );
    rect
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
        // 키보드: 둘째 항목이 현재 항목(선택 + 테두리), 셋째는 선택만 넓힌 항목.
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, name) in NAMES.iter().enumerate() {
                let rect = row(ui, theme, name, i == 0 || i == 3, i == 1 || i == 2);
                if i == 2 {
                    paint_cursor_ring(ui.painter(), theme, rect);
                }
            }
        });
        // 영역 선택: 빈 곳에서 끈 사각형에 걸친 칸이 선택된다.
        let gap = theme.spacing_md.value();
        let side = GRID_CELL.value();
        let (area, _) = ui.allocate_exact_size(
            egui::vec2(side * 3.0 + gap * 2.0, side * 2.0 + gap),
            egui::Sense::hover(),
        );
        for k in 0..6 {
            let (col, line) = ((k % 3) as f32, (k / 3) as f32);
            let cell = egui::Rect::from_min_size(
                area.min + egui::vec2(col * (side + gap), line * (side + gap)),
                egui::vec2(side, side),
            );
            let picked = matches!(k, 1 | 2 | 4 | 5);
            if picked {
                ui.painter().rect_filled(
                    cell,
                    theme.corner_radius.value(),
                    theme.surface_active().to_egui(),
                );
            }
            let g = theme.icon_glyph_size_lg.value();
            let glyph = egui::Rect::from_center_size(cell.center(), egui::vec2(g, g));
            let glyph_fg = if picked {
                theme.text_on_active()
            } else {
                theme.text_muted()
            };
            icons::FILE.image(g, glyph_fg.to_egui()).paint_at(ui, glyph);
        }
        // 첫 열 오른쪽 간격 위에서 시작해 오른쪽 아래 빈 곳까지 끈 사각형이다.
        let start = area.min + egui::vec2(side + gap / 2.0, -gap / 2.0);
        let end = area.max + egui::vec2(gap / 2.0, gap / 2.0);
        paint_marquee(ui.painter(), theme, egui::Rect::from_two_pos(start, end));
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "current item",
                "inset 1px ring over the selection fill · shown after a key · hidden on the next click",
            ),
            (
                "keys",
                "arrows · Home · End · PageUp · PageDown move · Shift extends · left/right in Grid only",
            ),
            (
                "rectangle",
                "drag from empty list space · tint fill + 1px border · no radius",
            ),
            ("selects", "every item it crosses · Ctrl/Cmd/Shift adds"),
            (
                "edge band",
                "24 at the top and bottom of the list · scrolls faster nearer the edge · no indicator",
            ),
        ],
        &[
            TokenChip::new(
                "explorer-cursor-ring",
                "→ border-focus",
                theme.explorer_cursor_ring().to_egui(),
            ),
            TokenChip::without_color("explorer-cursor-ring-width", "→ border-width"),
            TokenChip::new(
                "explorer-marquee-bg",
                "accent-primary × tint-fill-alpha",
                theme.explorer_marquee_bg().to_egui(),
            ),
            TokenChip::new(
                "explorer-marquee-border",
                "accent-primary × tint-border-alpha",
                theme.explorer_marquee_border().to_egui(),
            ),
            TokenChip::without_color("explorer-autoscroll-zone", "→ size-24"),
        ],
    );
}
