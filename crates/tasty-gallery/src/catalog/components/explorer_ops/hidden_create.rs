//! 숨김 파일과 폴더 안 만들기 예제(시안 batch 11 "Remaining states"). 이름 색은 본체와 같은
//! explorer-hidden-fg, 편집 줄은 본체와 같은 공용 `explorer_name_row_in`, 대상 표시는 `paint_drop_tint`.

use tasty_ui_widgets::{DropTintSlot, ExplorerNameOptions, explorer_name_row_in, paint_drop_ring};

use super::search::status_bar;
use super::*;

/// 시안 폴더 안 만들기 칸 폭(`w={560}`). 숨김 예제 칸은 시안 `w={400}` 라 `NARROW_W` 를 쓴다.
const CREATE_W: LogicalPx = LogicalPx(560.0);

/// (글리프, 이름, 크기, 수정일, 종류 문구 키(폴더는 빈 칸), 숨김). 종류는 본체 Type 열과 같은 lang 문구다.
type Row = (
    MockGlyph,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    bool,
);

const HIDDEN_ROWS: &[Row] = &[
    (icons::FOLDER, ".git", "—", "2026-10-09 18:12", "", true),
    (
        icons::FILE,
        ".env",
        "210 B",
        "2026-10-01 07:45",
        "ENV file",
        true,
    ),
    (
        icons::FOLDER,
        "Documents",
        "—",
        "2026-10-02 10:14",
        "",
        false,
    ),
    (
        icons::FILE,
        "backup-2026.tar.gz",
        "1.2 GB",
        "2026-09-30 22:01",
        "explorer.kind.archive",
        false,
    ),
    (
        icons::FILE,
        "report.pdf",
        "2.4 MB",
        "2026-06-24 09:12",
        "explorer.kind.pdf",
        false,
    ),
];

pub fn draw_hidden_files(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        listing(
            ui,
            theme,
            "hidden shown",
            HIDDEN_ROWS,
            &t_fmt("explorer.status.items", "8"),
        );
        let off = [
            t_fmt("explorer.status.items", "5"),
            t_fmt("explorer.status.hidden", "3"),
        ]
        .join(" · ");
        listing(
            ui,
            theme,
            "hidden off — status count",
            &HIDDEN_ROWS[2..4],
            &off,
        );
        wrap_item(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                lbl(ui, theme, "More — hidden row");
                let items = [
                    Mi::Item(
                        Some(icons::FOLDER_PLUS),
                        t("explorer.command.new_folder"),
                        false,
                    ),
                    Mi::Sep,
                    Mi::Item(Some(icons::SEARCH), t("explorer.command.find"), false),
                    Mi::Item(Some(icons::EYE), t("explorer.more.hidden_show"), false),
                ];
                render_menu(ui, theme, theme.tools_menu_min_width().value(), &items);
            });
        });
    });
    meta(
        ui,
        theme,
        &[
            (
                "toggle",
                "More row in action words (Show ↔ Hide hidden files) · explorer_toggle_hidden Ctrl+Shift+. / ⌘⇧. · default off · per explorer, remembered",
            ),
            ("hidden", "name starts with a dot (not ..)"),
            (
                "shown",
                "name + glyph explorer-hidden-fg · selected name stays text-primary · no opacity",
            ),
            ("while hidden", "status “{n} items · {h} hidden”"),
            ("menu", "native menu — no shortcut column"),
        ],
        &[TokenChip::new(
            "explorer-hidden-fg",
            "→ text-muted",
            theme.explorer_hidden_fg().to_egui(),
        )],
    );
}

fn listing(ui: &mut egui::Ui, theme: &Theme, label: &str, rows: &[Row], status: &str) {
    wrap_item(ui, |ui| {
        ui.set_width(NARROW_W.value());
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        lbl(ui, theme, label);
        framed(ui, theme, |ui| {
            ui.set_width(NARROW_W.value());
            let rows: Vec<&Row> = rows.iter().collect();
            detail_columns_table(
                ui,
                theme,
                &rows,
                &format!("gallery_exp_hidden_{label}"),
                |ui, th, r, col| row_cell(ui, th, r, col),
            );
            status_bar(ui, theme, status);
        });
    });
}

/// 숨김 행은 이름·글리프를 explorer-hidden-fg 로 그린다. 그 밖의 칸은 공용 예제와 같다.
fn row_cell(ui: &mut egui::Ui, th: &Theme, row: &Row, col: usize) {
    let &(g, name, size, date, kind, hidden) = row;
    let kind = if kind.is_empty() { "" } else { t(kind) };
    if col != 0 || !hidden {
        return cell_text(ui, th, col, g, name, size, date, kind);
    }
    let fg = th.explorer_hidden_fg().to_egui();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let sz = th.icon_glyph_size_md.value();
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(sz), egui::Sense::hover());
        g.image(sz, fg).paint_at(ui, rect);
        ui.label(
            egui::RichText::new(name)
                .size(th.font_size_body.value())
                .color(fg),
        );
    });
}

const CREATE_ROWS: &[Option<Row>] = &[
    Some((
        icons::FOLDER,
        "Documents",
        "—",
        "2026-10-02 10:14",
        "",
        false,
    )),
    Some((
        icons::FOLDER,
        "mockup-exports",
        "—",
        "2026-06-20 14:30",
        "",
        false,
    )),
    None,
    Some((
        icons::FILE,
        "report.pdf",
        "2.4 MB",
        "2026-06-24 09:12",
        "explorer.kind.pdf",
        false,
    )),
];

thread_local! {
    static IN_FOLDER: RefCell<Option<ExplorerNameEdit>> = const { RefCell::new(None) };
}

pub fn draw_create_in_folder(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Solo, |ui| {
        themes(ui, theme, |ui, th| {
            ui.set_width(CREATE_W.value());
            framed(ui, th, |ui| {
                ui.set_width(CREATE_W.value());
                let top = ui.cursor().top();
                let left = ui.max_rect().left();
                let width = ui.available_width();
                let tint = DropTintSlot::reserve(ui.painter());
                detail_columns_table(
                    ui,
                    th,
                    CREATE_ROWS,
                    "gallery_exp_create_in",
                    |ui, th, r, col| {
                        if let Some(r) = r {
                            row_cell(ui, th, r, col);
                        }
                    },
                );
                status_bar(ui, th, &t_fmt("explorer.status.items", "5"));
                // 표 머리 다음 두 번째 행이 대상 폴더, 세 번째 행이 비워 둔 편집 줄 자리다.
                let row_h = th.table_cell_height().value();
                let row = |i: f32| {
                    egui::Rect::from_min_size(
                        egui::pos2(left, top + row_h * i),
                        egui::vec2(width, row_h),
                    )
                };
                let radius = th.corner_radius.value();
                tint.fill(ui.ctx(), th, row(2.0), radius);
                paint_drop_ring(ui.painter(), th, row(2.0), radius);
                let caption = t_fmt("explorer.create.in_folder", "mockup-exports");
                IN_FOLDER.with(|cell| {
                    let mut slot = cell.borrow_mut();
                    let edit = slot.get_or_insert_with(|| {
                        ExplorerNameEdit::new(t("explorer.new.file_default").into(), 0..8)
                    });
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row(3.0)));
                    explorer_name_row_in(
                        &mut child,
                        th,
                        ExplorerNameLayout::Detail {
                            inset: DETAIL_INSET.value(),
                            name_width: width - explorer_detail_tail_width(false, th),
                        },
                        ExplorerNameOptions {
                            field_id: None,
                            indent: th.explorer_create_indent().value(),
                            caption: Some(&caption),
                            pending: None,
                        },
                        icons::FILE,
                        edit,
                        false,
                    );
                });
            });
        });
    });
    meta(
        ui,
        theme,
        &[
            (
                "input",
                "under the target row · indent explorer-create-indent 16 · field max explorer-create-field-max-width 240 · caption “in {folder}” at space-sm after it, caption size, text-muted",
            ),
            (
                "target",
                "drop-target ring + fill while the field is open · same ring as drag and drop, radius",
            ),
            (
                "grid view",
                "new cell right after the folder cell · no caption",
            ),
            ("keys", "Enter creates inside · Esc cancels · listing stays"),
        ],
        &[
            TokenChip::without_color("explorer-create-indent", "→ space-lg 16"),
            TokenChip::without_color("explorer-create-field-max-width", "→ size-240"),
            TokenChip::new(
                "explorer-drop-target-border",
                "target",
                theme.explorer_drop_target_border().to_egui(),
            ),
        ],
    );
}
