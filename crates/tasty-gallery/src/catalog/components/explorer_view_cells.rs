//! 탐색기의 그리드·목록·상세 보기 예제. 목록은 tree_row, 상세는 공용 Table을 사용한다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    ExplorerDetailHeads, RowParent, RowState, Table, TableSortDir, explorer_detail_columns,
    in_hovered_table_row, tree_row, tree_row_cut,
};

use crate::catalog::icons::{FILE, FOLDER, IMAGE, MockGlyph};
use crate::catalog::spec::{StageVariant, TokenChip, body_column, cluster, meta, note, stage};

/// 셀 폭.
const CELL_W: LogicalPx = LogicalPx(80.0);

/// 샘플 항목의 종류. 이미지는 accent-info 로 칠한다(본체 explorer 와 같은 규칙).
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Folder,
    File,
    Image,
    /// 그리드 썸네일을 다 만든 이미지. 목록·상세에서는 `Image` 와 같이 그린다.
    Thumb,
}

impl Kind {
    fn glyph(self) -> MockGlyph {
        match self {
            Kind::Folder => FOLDER,
            Kind::File => FILE,
            Kind::Image | Kind::Thumb => IMAGE,
        }
    }

    /// 이미지만 accent-info, 나머지는 `glyph`(호출부가 정한 글리프 색)를 쓴다.
    fn tint(self, theme: &Theme, glyph: egui::Color32) -> egui::Color32 {
        match self {
            Kind::Image | Kind::Thumb => egui::Color32::from(theme.accent_info()),
            Kind::Folder | Kind::File => glyph,
        }
    }

    /// 선택 행의 글리프 색. 본체 `entry_icon` 과 같다.
    fn selected_tint(self, theme: &Theme) -> egui::Color32 {
        theme.text_on_active().into()
    }
}

/// 시안이 정적으로 보여 주는 셀 상태. 선택은 클릭으로 바뀌므로 여기 두지 않는다.
#[derive(Clone, Copy, PartialEq)]
enum Mark {
    None,
    Hover,
    Cut,
}

#[derive(Clone, Copy)]
struct Entry {
    kind: Kind,
    name: &'static str,
    mark: Mark,
}

const fn entry(kind: Kind, name: &'static str, mark: Mark) -> Entry {
    Entry { kind, name, mark }
}

/// 시안 `ExpGridMini`. 선택 초기값은 diagram.png(GRID_SEL).
const GRID: &[Entry] = &[
    entry(Kind::Folder, "mockup-exports", Mark::None),
    // 긴 이름 샘플 — 3줄 wrap + '…' 말줄임 시연.
    entry(Kind::File, "rust-toolchain.toml", Mark::None),
    entry(Kind::Image, "diagram.png", Mark::None),
    entry(Kind::File, "notes.md", Mark::Hover),
    entry(Kind::File, "THIRD_PARTY_LICENSES.md", Mark::None),
    entry(Kind::Folder, "node_modules", Mark::None),
];

/// 시안 `ExpListMini`. 선택 초기값은 diagram.png(LIST_SEL).
const LIST: &[Entry] = &[
    entry(Kind::Folder, "mockup-exports", Mark::None),
    entry(Kind::File, "report.pdf", Mark::None),
    entry(Kind::Image, "diagram.png", Mark::None),
    entry(Kind::File, "notes.md", Mark::Hover),
    entry(Kind::File, "build.sh", Mark::None),
    entry(Kind::File, "archive.zip", Mark::Cut),
];

pub(super) struct DetailRow {
    pub(super) kind: Kind,
    pub(super) name: &'static str,
    pub(super) size: &'static str,
    pub(super) modified: &'static str,
    pub(super) kind_label: &'static str,
    /// 잘라내기 대기 행 — 이름을 `cut-pending-fg` 이탤릭으로 그린다.
    pub(super) cut: bool,
}

/// 시안 View modes 의 Detail 열. 선택 초기값은 diagram.png(DETAIL_SEL).
const DETAIL: &[DetailRow] = &[
    DetailRow {
        kind: Kind::Folder,
        name: "mockup-exports",
        size: "—",
        modified: "06-20 14:30",
        kind_label: "Folder",
        cut: false,
    },
    DetailRow {
        kind: Kind::File,
        name: "report.pdf",
        size: "2.4 MB",
        modified: "06-24 09:12",
        kind_label: "PDF document",
        cut: false,
    },
    DetailRow {
        kind: Kind::Image,
        name: "diagram.png",
        size: "488 KB",
        modified: "06-26 18:05",
        kind_label: "PNG image",
        cut: false,
    },
];

thread_local! {
    static GRID_SEL: RefCell<usize> = const { RefCell::new(2) };
    static LIST_SEL: RefCell<usize> = const { RefCell::new(2) };
    static DETAIL_SEL: RefCell<usize> = const { RefCell::new(2) };
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_panel()))
            .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing =
                        egui::vec2(theme.spacing_md.value(), theme.spacing_md.value());
                    GRID_SEL.with(|s| {
                        let mut sel = s.borrow_mut();
                        for (i, e) in GRID.iter().enumerate() {
                            if grid_cell(ui, theme, e, i == *sel, false) {
                                *sel = i;
                            }
                        }
                    });
                });
            });
    });

    // 잘라내기 대기는 전경만 흐리게 하고 선택·호버 배경은 유지한다.
    cluster(
        ui,
        theme,
        "cut (cut-pending-fg, italic name until paste)",
        |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from(theme.bg_panel()))
                .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing =
                            egui::vec2(theme.spacing_md.value(), theme.spacing_md.value());
                        grid_cell(ui, theme, &GRID[0], true, true);
                        grid_cell(ui, theme, &GRID[2], false, true);
                    });
                });
        },
    );

    cluster(ui, theme, "list — single column (tree_row reuse)", |ui| {
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_panel()))
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                egui::Color32::from(theme.border_default()),
            ))
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
            .show(ui, |ui| {
                ui.set_width(theme.measure_sm.value());
                ui.spacing_mut().item_spacing.y = 0.0;
                // cluster 의 가로 배치를 물려받으면 행 scope 가 옆으로 붙는다. 세로 배치 안에 둔다.
                ui.vertical(|ui| {
                    LIST_SEL.with(|s| {
                        let mut sel = s.borrow_mut();
                        for (i, e) in LIST.iter().enumerate() {
                            let clicked = ui
                                .scope(|ui| {
                                    if e.mark == Mark::Hover {
                                        static_row_hover(ui, theme);
                                    }
                                    let kind = e.kind;
                                    let selected = i == *sel;
                                    let glyph =
                                        |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
                                            let fg = if selected {
                                                kind.selected_tint(theme)
                                            } else {
                                                kind.tint(theme, c)
                                            };
                                            kind.glyph()
                                                .image(rect.height(), fg)
                                                .paint_at(ui, rect);
                                        };
                                    let resp = if e.mark == Mark::Cut {
                                        tree_row_cut(
                                            ui,
                                            theme,
                                            Some(&glyph),
                                            e.name,
                                            "",
                                            selected,
                                            0.0,
                                            theme.cut_pending_fg().to_egui(),
                                        )
                                        .0
                                    } else {
                                        tree_row(
                                            ui,
                                            theme,
                                            0,
                                            false,
                                            false,
                                            Some(&glyph),
                                            e.name,
                                            None,
                                            selected,
                                        )
                                    };
                                    resp.clicked()
                                })
                                .inner;
                            if clicked {
                                *sel = i;
                            }
                        }
                    });
                });
            });
    });

    cluster(
        ui,
        theme,
        "detail — sortable columns (Table reuse)",
        |ui| {
            // 페이지 본문은 가로 폭 제한이 없어 Remainder 열이 남은 폭을 모두 차지한다.
            // 설명과 같은 본문 컬럼 폭의 세로 배치 안에 두어 네 열이 본문 안에 들어오게 한다.
            body_column(ui, |ui| {
                DETAIL_SEL.with(|s| {
                    let mut sel = s.borrow_mut();
                    let height = theme.overlay_top_offset * 2.0;
                    if let Some(i) = detail_table(
                        ui,
                        theme,
                        DETAIL,
                        *sel,
                        height,
                        "explorer_detail_demo",
                        RowParent::Base,
                    ) {
                        *sel = i;
                    }
                });
            });
        },
    );

    meta(
        ui,
        theme,
        &[
            (
                "grid cell",
                "40 slot (glyph 16 centred or thumbnail) + 3-line label (…) · fixed height",
            ),
            ("list row", "22 · tree-row-height (file tree row)"),
            (
                "detail row / header",
                "28 · table-cell-height (shared Table)",
            ),
            (
                "detail cells",
                "Name flex · Size/Date mono 11 · Size padR 8 (th+td)",
            ),
            ("selected", "surface-active (no border)"),
            ("detail name", "table-row-fg · selected text-primary"),
            ("glyph", "folder/file text-muted · image accent-info"),
            (
                "cut",
                "cut-pending-fg · italic name until paste · no opacity",
            ),
            ("sort", "header indicator (accent-primary)"),
        ],
        &[
            TokenChip::without_color("tree-row-height", "List row (22)"),
            TokenChip::without_color("table-cell-height", "Detail row + header (28)"),
            TokenChip::without_color("overlay-hover", "row hover (8%)"),
            TokenChip::new(
                "surface-raised",
                "icon box",
                egui::Color32::from(theme.surface_raised()),
            ),
            TokenChip::new(
                "surface-active",
                "selected",
                egui::Color32::from(theme.surface_active()),
            ),
            TokenChip::new(
                "accent-primary",
                "sel border / sort",
                egui::Color32::from(theme.accent_primary()),
            ),
            TokenChip::new(
                "table-row-fg",
                "detail name",
                egui::Color32::from(theme.table_row_fg()),
            ),
            TokenChip::new(
                "text-muted",
                "detail meta",
                egui::Color32::from(theme.text_muted_raised()),
            ),
        ],
    );

    note(
        ui,
        theme,
        "Every grid cell reserves the 40 thumbnail slot; this example draws glyphs only (thumbnails are under Explorer properties and preview). List and detail views use the shared tree_row and Table widgets. Cut-pending cells use cut-pending-fg with an italic name and keep the selection or hover background. No explorer-specific row height: the list reuses the tree row (22), and the detail header and body rows reuse the shared Table height (28).",
    );
}

/// 그리드 셀을 그린다. 클릭하면 true를 반환하며 cut 상태는 이름만 바뀐다.
fn grid_cell(ui: &mut egui::Ui, theme: &Theme, e: &Entry, selected: bool, cut: bool) -> bool {
    grid_cell_parts(
        ui,
        theme,
        e.kind,
        e.name,
        selected,
        e.mark == Mark::Hover,
        cut,
    )
    .clicked()
}

/// 그리드 셀 하나. 모든 셀이 `explorer_grid_thumb_size`(40) 슬롯을 잡아 썸네일 유무와 관계없이
/// 행 높이가 같다. 썸네일이 없는 항목은 16 글리프를 슬롯 가운데에 둔다.
pub(super) fn grid_cell_parts(
    ui: &mut egui::Ui,
    theme: &Theme,
    kind: Kind,
    name: &str,
    selected: bool,
    hover: bool,
    cut: bool,
) -> egui::Response {
    let glyph = theme.icon_glyph_size_md.value(); // design glyph 16
    let slot = theme.explorer_grid_thumb_size().value();
    // 라벨: caption(11), line_h ≈ round(11 × 1.3)=14. 고정 3줄 예약 → 그리드 행 정렬 균일.
    let label_font = theme.font_size_caption.value();
    let label_line_h = (label_font * 1.3).round();
    let label_h = label_line_h * 3.0;
    let cell_h = theme.spacing_sm.value()
        + slot
        + theme.spacing_xs.value()
        + label_h
        + theme.spacing_sm.value();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(CELL_W.value(), cell_h), egui::Sense::click());
    let p = ui.painter_at(rect);

    if selected {
        p.rect_filled(
            rect,
            theme.corner_radius.value(),
            egui::Color32::from(theme.surface_active()),
        );
    } else if resp.hovered() || hover {
        p.rect_filled(
            rect,
            theme.corner_radius.value(),
            theme.overlay_hover().to_egui_premultiplied(),
        );
    }

    let slot_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.center().x,
            rect.top() + theme.spacing_sm.value() + slot / 2.0,
        ),
        egui::vec2(slot, slot),
    );
    if matches!(kind, Kind::Thumb) {
        thumb_stand_in(ui, theme, slot_rect);
    } else {
        kind.glyph()
            .image(
                glyph,
                if selected {
                    kind.selected_tint(theme)
                } else {
                    kind.tint(theme, egui::Color32::from(theme.text_secondary()))
                },
            )
            .paint_at(
                ui,
                egui::Rect::from_center_size(slot_rect.center(), egui::vec2(glyph, glyph)),
            );
    }

    let label_color = egui::Color32::from(if selected {
        theme.text_on_active()
    } else if cut {
        theme.cut_pending_fg()
    } else {
        theme.text_secondary()
    });
    let mut job = egui::text::LayoutJob {
        halign: egui::Align::Center,
        wrap: egui::text::TextWrapping {
            max_width: (CELL_W - theme.spacing_xs.scaled(2.0)).value(),
            max_rows: 3,
            overflow_character: Some('…'),
            ..Default::default()
        },
        ..Default::default()
    };
    job.append(
        name,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(label_font),
            color: label_color,
            line_height: Some(label_line_h),
            italics: cut,
            ..Default::default()
        },
    );
    let galley = ui.fonts(|f| f.layout_job(job));
    p.galley(
        egui::pos2(
            rect.center().x,
            slot_rect.bottom() + theme.spacing_xs.value(),
        ),
        galley,
        label_color,
    );

    resp
}

/// 디코딩한 그림 대신 놓는 자리 표시. 시안처럼 1px separator 테두리 · radius-sm · surface-hover 바탕에
/// 아래쪽 "img" 글자를 둔다. 본체는 이 자리에 40 × 40 에 맞춘 그림을 그린다.
pub(super) fn thumb_stand_in(ui: &egui::Ui, theme: &Theme, slot: egui::Rect) {
    let p = ui.painter();
    let radius = theme.corner_radius_sm.value();
    p.rect_filled(slot, radius, theme.surface_hover().to_egui());
    p.rect_stroke(
        slot,
        radius,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );
    p.text(
        egui::pos2(slot.center().x, slot.bottom() - theme.border_width.value()),
        egui::Align2::CENTER_BOTTOM,
        "img",
        egui::FontId::monospace(theme.font_size_micro.value()),
        theme.text_muted().to_egui(),
    );
}

/// 상세 보기 표. 공용 `Table` 로 그리며 클릭한 행 번호를 돌려준다.
/// `cut` 행은 본체처럼 이름을 `cut-pending-fg` 이탤릭으로 그린다.
pub(super) fn detail_table(
    ui: &mut egui::Ui,
    theme: &Theme,
    rows: &'static [DetailRow],
    selected: usize,
    max_height: LogicalPx,
    id_salt: &str,
    parent: RowParent,
) -> Option<usize> {
    let columns = explorer_detail_columns(
        theme,
        ExplorerDetailHeads {
            name: ("Name", 0_usize),
            size: ("Size", 1),
            modified: ("Date modified", 2),
            kind: ("Type", 3),
            folder: None,
        },
    );
    let is_sel = |row: &DetailRow| rows.iter().position(|r| r.name == row.name) == Some(selected);
    let out = Table::new(columns)
        .active_sort(0_usize, TableSortDir::Asc)
        .header_fill(egui::Color32::from(theme.table_header_bg()))
        // design DetailHeader: Size 제목 paddingRight 8, 본문 Size 셀과 같은 여백.
        .header_pad_right(theme.spacing_sm)
        .selectable(true)
        .max_scroll_height(max_height)
        .id_salt(id_salt)
        .show(ui, theme, rows, is_sel, |ui, th, row, col| {
            let sel = is_sel(row);
            let state = RowState::of(true, sel, in_hovered_table_row(ui), false);
            let muted = egui::Color32::from(state.meta(th, parent));
            match col {
                0 => {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                        let sz = th.icon_glyph_size_md.value();
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::hover());
                        row.kind
                            .glyph()
                            .image(
                                sz,
                                if sel {
                                    row.kind.selected_tint(th)
                                } else {
                                    row.kind.tint(th, egui::Color32::from(th.text_secondary()))
                                },
                            )
                            .paint_at(ui, rect);
                        let rest = if row.cut {
                            th.cut_pending_fg()
                        } else {
                            th.table_row_fg()
                        };
                        let name_fg = state.label(th, parent, rest);
                        let name = egui::RichText::new(row.name)
                            .size(th.font_size_body.value())
                            .color(egui::Color32::from(name_fg));
                        ui.label(if row.cut { name.italics() } else { name });
                    });
                }
                1 => {
                    ui.add_space(th.spacing_sm.value());
                    ui.label(
                        egui::RichText::new(row.size)
                            .font(egui::FontId::monospace(th.font_size_caption.value()))
                            .color(muted),
                    );
                }
                2 => {
                    ui.label(
                        egui::RichText::new(row.modified)
                            .font(egui::FontId::monospace(th.font_size_caption.value()))
                            .color(muted),
                    );
                }
                _ => {
                    ui.label(
                        egui::RichText::new(row.kind_label)
                            .size(th.font_size_caption.value())
                            .color(muted),
                    );
                }
            }
        });
    out.clicked_row
}

/// 시안이 hover 상태로 보여 주는 목록 행의 배경. 다음 `tree_row` 자리에 먼저 칠한다.
/// 실제 포인터가 올라가 있으면 `tree_row` 가 같은 배경을 칠하므로 겹쳐 칠하지 않는다.
fn static_row_hover(ui: &mut egui::Ui, theme: &Theme) {
    let rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), theme.tree_row_height().value()),
    );
    if !ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.tree_row_bg_hover().to_egui_premultiplied(),
        );
    }
}
