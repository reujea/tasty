//! 탐색기 파일 조작 예제 — 툴바 명령 묶음, 새 항목 이름 입력과 오류, Find 바와 하위 폴더 검색.
//! 명령 묶음·이름 입력·Find 바는 본체와 같은 공용 위젯을 부른다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    CenterState, ExplorerCommand, ExplorerCommandLabels, ExplorerCommandsView, ExplorerDetailHeads,
    ExplorerFindBar, ExplorerFindLabels, ExplorerFindStatus, ExplorerNameEdit, ExplorerNameLayout,
    ExplorerToggle, Table, TableSortDir, explorer_commands, explorer_commands_width,
    explorer_detail_columns, explorer_detail_tail_width, explorer_find_bar, explorer_match_job,
    explorer_name_error, explorer_name_row, tree_row,
};

use super::explorer_context_menu::{Mi, render_menu};
use super::explorer_surface::{nav_button, path_field};
use super::explorer_toolbar::{seg_toggle, seg_toggle_width};
use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{StageVariant, TokenChip, meta, note, stage, wrap_item};
use crate::catalog::{Section, Spec};
use crate::i18n::{t, t_count, t_fmt, t_fmt2};

mod drop_on_bed;
mod hidden_create;
mod kinds;
mod search;
pub use search::search_section;

/// 시안 툴바 예제의 최대 폭(`maxWidth: 640`).
const WIDE_W: LogicalPx = LogicalPx(640.0);
/// 시안 좁은 칸 예제 폭(`w={400}`). 접기 기준 440 보다 좁다.
const NARROW_W: LogicalPx = LogicalPx(400.0);
/// 시안 이름 입력·검색 예제의 칸 폭(`w={420}`·`w={520}`).
const CELL_W: LogicalPx = LogicalPx(420.0);
const SEARCH_W: LogicalPx = LogicalPx(520.0);
/// 시안 List 예제 폭(`w={220}`)과 Grid 예제 폭(`w={300}`).
const LIST_W: LogicalPx = LogicalPx(220.0);
const GRID_W: LogicalPx = LogicalPx(300.0);
/// 시안 Grid 칸 폭(`width: 80`)과 그 칸이 시작하는 왼쪽 여백(`paddingLeft: 48`).
const GRID_CELL_W: LogicalPx = LogicalPx(80.0);
const GRID_LEFT: LogicalPx = LogicalPx(48.0);
/// 시안 상세 행 왼쪽 여백(`padding: 0 10px`).
const DETAIL_INSET: LogicalPx = LogicalPx(10.0);
/// 시안 이름 입력 예제 칸 높이(`height: 172`).
const EDIT_CELL_H: LogicalPx = LogicalPx(172.0);
/// 시안 오류 예제 사이 간격(`gap: 44`)과 마지막 상자 아래 여백(`marginBottom: 32`).
const ERROR_GAP: LogicalPx = LogicalPx(44.0);
const ERROR_TAIL: LogicalPx = LogicalPx(32.0);

pub fn create_section() -> Section {
    Section {
        id: "explorer-create",
        title: "Explorer — create · commands",
        specs: vec![
            Spec {
                id: "explorer-commands",
                title: "Toolbar — create group and view group (wide · narrow · remote · read-only)",
                when: Some(
                    "create (New folder · New file) | view (Find) between the PathField and the SegToggle · \
                     cell < 440 folds both into More",
                ),
                draw: draw_commands,
            },
            Spec {
                id: "explorer-name-input",
                title: "Name input — inline, at the top of the list (Detail · List · Grid)",
                when: Some(
                    "“New folder” all selected · “untitled.txt” stem selected · Enter confirm · Esc cancel · blur confirms when valid",
                ),
                draw: draw_name_input,
            },
            Spec {
                id: "explorer-name-errors",
                title: "Name errors — empty · invalid character · already exists",
                when: Some("message box under the field, over the next rows — never a toast"),
                draw: draw_name_errors,
            },
            Spec {
                id: "explorer-hidden-files",
                title: "Hidden files — shown in muted ink · counted while hidden · More row",
                when: Some(
                    "dot names · default off · Ctrl+Shift+. / More row toggles · per explorer, remembered",
                ),
                draw: hidden_create::draw_hidden_files,
            },
            Spec {
                id: "explorer-create-in-folder",
                title: "Create inside a folder — input under the target row",
                when: Some(
                    "folder row menu → input right under the folder, indented, “in {folder}” · target ringed",
                ),
                draw: hidden_create::draw_create_in_folder,
            },
            Spec {
                id: "explorer-ops-drop-target-bed",
                title: "Drop target on the bed — folder row · list body",
                when: Some("the tint paints under the row · the ring paints last"),
                draw: drop_on_bed::draw_drop_target_on_the_bed,
            },
        ],
    }
}

/// 시안 `XLbl` — mono micro muted 한 줄.
fn lbl(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(egui::FontId::monospace(theme.font_size_micro.value()))
            .color(theme.text_muted().to_egui()),
    );
}

/// 예제 칸 테두리. 시안 `border: 1px border-default · radius · overflow hidden`.
fn framed(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            add(ui)
        });
}

/// 시안 Mocha·Latte 짝. 각 테마의 `bg-app` 상자 안에 라벨과 예제를 그린다.
fn themes(ui: &mut egui::Ui, theme: &Theme, draw: impl Fn(&mut egui::Ui, &Theme)) {
    let latte = crate::host_shell::latte_theme();
    let mocha = tasty_themes::mocha_fallback();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::splat(theme.spacing_lg.value());
        for (label, th) in [("Mocha", &mocha), ("Latte", &latte)] {
            wrap_item(ui, |ui| {
                egui::Frame::new()
                    .fill(th.bg_app().to_egui())
                    .stroke(egui::Stroke::new(
                        th.border_width.value(),
                        th.border_default().to_egui(),
                    ))
                    .corner_radius(th.corner_radius.value())
                    .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                            lbl(ui, th, label);
                            // 두 테마 칸이 같은 예제를 그리므로 표·스크롤 id 를 테마별로 나눈다.
                            ui.push_id(label, |ui| draw(ui, th));
                        });
                    });
            });
        }
    });
}

/// 쓰기를 받지 않는 원격 예제의 주소. 이 칸의 create 툴팁은 원격 이유를 쓴다.
const REMOTE_PATH: &str = "build-eu:~/logs";
/// 원격 예제의 호스트 이름.
const REMOTE_HOST: &str = "build-eu";

fn labels(cannot_write: &str) -> ExplorerCommandLabels<'_> {
    ExplorerCommandLabels {
        new_folder: t("explorer.command.new_folder"),
        new_file: t("explorer.command.new_file"),
        more: t("explorer.command.more"),
        cannot_write,
    }
}

/// 시안 `XToolbar` — 탐색 버튼 · PathField(남은 폭) · 명령 묶음 · 보기 전환.
fn toolbar(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: f32,
    path: &str,
    create: Option<bool>,
    find: bool,
    hidden: bool,
) {
    let pad = theme.spacing_sm.value();
    let h = theme.item_height_interactive.value() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let bw = theme.border_width.value();
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - bw * 0.5,
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
    );
    // 같은 페이지에 툴바가 여럿이라 보기 전환 칸의 id 가 겹치지 않게 툴바마다 id 를 나눈다.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(ui.next_auto_id())
            .max_rect(rect.shrink2(egui::Vec2::X * pad))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let gap = tasty_ui_widgets::tokens::STRUCT_GAP_1.value();
    child.spacing_mut().item_spacing.x = gap;
    for (g, enabled) in [
        (icons::CHEVRON_LEFT, true),
        (icons::CHEVRON_RIGHT, false),
        (icons::CHEVRON_UP, true),
        (icons::REFRESH, true),
    ] {
        nav_button(&mut child, theme, g, enabled);
    }
    child.add_space(pad - gap);
    child.spacing_mut().item_spacing.x = pad;
    // 본체는 바인딩 표시를 설정에서 읽는다. 갤러리는 기본 바인딩의 표시를 쓴다.
    let hidden_key = if hidden {
        "explorer.more.hidden_hide"
    } else {
        "explorer.more.hidden_show"
    };
    let hidden_tip = format!("{} (Ctrl+Shift+.)", t(hidden_key));
    let toggles = [
        ExplorerToggle {
            command: ExplorerCommand::Find,
            icon: icons::SEARCH,
            label: t("explorer.command.find"),
            active: find,
        },
        ExplorerToggle {
            command: ExplorerCommand::TogglePreview,
            icon: icons::COLUMNS,
            label: t("explorer.preview.toggle"),
            active: false,
        },
        ExplorerToggle {
            command: ExplorerCommand::ToggleHidden,
            icon: icons::EYE,
            label: &hidden_tip,
            active: hidden,
        },
    ];
    // 원격 칸은 로컬 쓰기 권한 대신 원격 이유를 툴팁으로 단다(본체 `cannot_write_tip`).
    let cannot_write = if path == REMOTE_PATH {
        t_fmt("explorer.remote.write_needs_update", REMOTE_HOST)
    } else {
        t("explorer.command.cannot_write").to_owned()
    };
    let view = ExplorerCommandsView {
        create,
        toggles: &toggles,
        compact: tasty_ui_widgets::explorer_commands_compact(theme, width),
        labels: labels(&cannot_write),
    };
    let commands_w = explorer_commands_width(theme, &view);
    let addr_w =
        (child.available_width() - commands_w - seg_toggle_width(theme) - 2.0 * pad).max(0.0);
    child.allocate_ui_with_layout(
        egui::vec2(addr_w, child.available_height()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            path_field(
                ui,
                theme,
                &format!("gallery_exp_ops_addr_{path}_{width}"),
                path,
            )
        },
    );
    explorer_commands(&mut child, theme, &view);
    seg_toggle(&mut child, theme, 2, None);
}

pub fn draw_commands(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        let wide = WIDE_W.value().min(ui.available_width());
        let rows: [(&str, &str, Option<bool>, bool, bool); 5] = [
            ("wide · local", "~/Downloads", Some(true), false, false),
            ("wide · find on", "~/Downloads", Some(true), true, false),
            (
                "wide · hidden files shown — eye pressed",
                "~/Downloads",
                Some(true),
                false,
                true,
            ),
            (
                "remote that can't write — create disabled, reason in the tooltip",
                REMOTE_PATH,
                Some(false),
                false,
                false,
            ),
            (
                "permission denied — create disabled",
                "/usr/share",
                Some(false),
                false,
                false,
            ),
        ];
        for (label, path, create, find, hidden) in rows {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                lbl(ui, theme, label);
                framed(ui, theme, |ui| {
                    toolbar(ui, theme, wide, path, create, find, hidden)
                });
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                lbl(ui, theme, "narrow cell (< 440) — More");
                framed(ui, theme, |ui| {
                    toolbar(
                        ui,
                        theme,
                        NARROW_W.value(),
                        "~/Downloads",
                        Some(true),
                        false,
                        false,
                    )
                });
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                lbl(ui, theme, "More menu — find open, preview closed");
                let items = [
                    Mi::Item(
                        Some(icons::FOLDER_PLUS),
                        t("explorer.command.new_folder"),
                        false,
                    ),
                    Mi::Item(
                        Some(icons::FILE_PLUS),
                        t("explorer.command.new_file"),
                        false,
                    ),
                    Mi::Sep,
                    Mi::Item(Some(icons::SEARCH), t("explorer.more.find_close"), false),
                    Mi::Item(Some(icons::COLUMNS), t("explorer.more.preview_show"), false),
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
                "order",
                "nav · PathField (flex 1) · create · | · view · SegToggle",
            ),
            (
                "create",
                "folderPlus · filePlus · IconButton sm, icon-only, tooltip = command",
            ),
            (
                "view",
                "search · columns (preview) · eye (hidden files, last) — toggles (active state) · eye tooltip = More row words + “(Ctrl+Shift+.)” (⌘⇧. on macOS)",
            ),
            ("separator", "1px × 16 · separator · 4 each side"),
            (
                "narrow",
                "cell < explorer-toolbar-compact-below 440 → one More (more glyph) · menu = same rows, Preview included",
            ),
            (
                "More rows",
                "no check marks · say the action: Find ↔ Close find · Show preview ↔ Hide preview",
            ),
            (
                "remote",
                "writable: as local · can't write: create disabled · tooltip = reason · view kept",
            ),
            (
                "no write access",
                "create disabled · tooltip “Can't write to this folder”",
            ),
            (
                "not in toolbar",
                "Delete · Rename · Copy / Cut / Paste (menu + keys)",
            ),
        ],
        &[
            TokenChip::without_color("explorer-toolbar-compact-below", "→ size-440"),
            TokenChip::new(
                "separator",
                "group separator",
                theme.separator.to_egui_premultiplied(),
            ),
            TokenChip::new(
                "state-disabled-fg",
                "disabled create",
                theme.state_disabled_fg().to_egui(),
            ),
        ],
    );
    note(
        ui,
        theme,
        "The app's More menu is the OS native menu, which has no check mark, so a toggle row there \
         does not show its on state. Keybinding actions: explorer.new_folder · explorer.new_file · \
         explorer.find — no default keys.",
    );
}

/// 이름 입력 예제의 편집 상태. 프레임마다 새로 만들면 선택 범위가 매번 다시 잡혀 입력할 수 없다.
struct EditState {
    detail: ExplorerNameEdit,
    list: ExplorerNameEdit,
    grid: ExplorerNameEdit,
}

thread_local! {
    static EDITS: RefCell<Option<EditState>> = const { RefCell::new(None) };
}

const LIST_ROWS: &[(MockGlyph, &str)] = &[
    (icons::FOLDER, "Documents"),
    (icons::FOLDER, "mockup-exports"),
    (icons::FILE, "report.pdf"),
    (icons::IMAGE, "diagram.png"),
];

pub fn draw_name_input(ui: &mut egui::Ui, theme: &Theme) {
    EDITS.with(|cell| {
        let mut slot = cell.borrow_mut();
        let edits = slot.get_or_insert_with(|| EditState {
            detail: ExplorerNameEdit::new(t("explorer.new.folder_default").into(), 0..10),
            list: ExplorerNameEdit::new(t("explorer.new.file_default").into(), 0..8),
            grid: ExplorerNameEdit::new(t("explorer.new.file_default").into(), 0..8),
        });
        stage(ui, theme, StageVariant::Wrap, |ui| {
            column(ui, theme, "Detail — new folder", CELL_W.value(), |ui| {
                detail_with_editor(ui, theme, &mut edits.detail, None);
            });
            column(ui, theme, "List — new file", LIST_W.value(), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                explorer_name_row(
                    ui,
                    theme,
                    ExplorerNameLayout::List,
                    icons::FILE,
                    &mut edits.list,
                    false,
                );
                for (g, name) in LIST_ROWS {
                    let g = *g;
                    tree_row(
                        ui,
                        theme,
                        0,
                        false,
                        false,
                        Some(&|ui, r, c| g.image(r.height(), c).paint_at(ui, r)),
                        name,
                        None,
                        false,
                    );
                }
            });
            column(
                ui,
                theme,
                "Grid — new file (editor spans 160, centred)",
                GRID_W.value(),
                |ui| {
                    ui.horizontal_top(|ui| {
                        ui.add_space(GRID_LEFT.value());
                        let slot = theme.explorer_grid_thumb_size().value();
                        let pad = theme.spacing_sm.value();
                        let cell = egui::vec2(
                            GRID_CELL_W.value(),
                            pad + slot
                                + theme.spacing_xs.value()
                                + theme.input_height().value()
                                + pad,
                        );
                        explorer_name_row(
                            ui,
                            theme,
                            ExplorerNameLayout::Grid { cell, slot },
                            icons::FILE,
                            &mut edits.grid,
                            false,
                        );
                    });
                },
            );
        });
    });
    meta(
        ui,
        theme,
        &[
            (
                "placement",
                "top of the listing while typing → sorted place after Enter",
            ),
            (
                "editor",
                "Input (28) in the name slot · Detail row stays 28 · List row grows 22 → 28 · Grid: 160 wide, centred under the glyph",
            ),
            (
                "defaults",
                "“New folder” all selected · “untitled.txt” stem selected · taken → “… 2”",
            ),
            ("Enter / Esc", "confirm / cancel (nothing on disk)"),
            ("blur", "valid → confirm · empty or invalid → cancel"),
            (
                "after",
                "sorted place, selected — only if the user still views that folder",
            ),
            ("write fails", "error Toast + reload"),
        ],
        &[
            TokenChip::new(
                "surface-active",
                "editing row",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "input-border-focus",
                "editor focus",
                theme.input_border_focus().to_egui(),
            ),
            TokenChip::without_color("table-cell-height", "row 28"),
        ],
    );
}

fn column(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    width: f32,
    add: impl FnOnce(&mut egui::Ui),
) {
    wrap_item(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        lbl(ui, theme, label);
        framed(ui, theme, |ui| {
            ui.set_width(width);
            ui.set_height(EDIT_CELL_H.value());
            add(ui);
        });
    });
}

const DETAIL_ROWS: &[(MockGlyph, &str, &str, &str, &str)] = &[
    (icons::FOLDER, "mockup-exports", "—", "2026-06-20 14:30", ""),
    (
        icons::FILE,
        "report.pdf",
        "2.4 MB",
        "2026-06-24 09:12",
        "PDF document",
    ),
    (
        icons::IMAGE,
        "diagram.png",
        "488 KB",
        "2026-06-26 18:05",
        "PNG image",
    ),
    (
        icons::FILE,
        "notes.md",
        "12 KB",
        "2026-06-27 11:40",
        "Markdown",
    ),
];

/// 상세 표 첫 행 자리에 편집 줄을 겹쳐 그린다. 표 첫 행은 비워 둔 자리다.
fn detail_with_editor(
    ui: &mut egui::Ui,
    theme: &Theme,
    edit: &mut ExplorerNameEdit,
    error: Option<&str>,
) {
    let mut rows: Vec<Option<usize>> = vec![None];
    rows.extend((0..DETAIL_ROWS.len()).map(Some));
    let top = ui.cursor().top();
    let left = ui.max_rect().left();
    let width = ui.available_width();
    detail_columns_table(
        ui,
        theme,
        &rows,
        "gallery_exp_ops_detail",
        |ui, th, row, col| {
            let Some(i) = row else { return };
            let (g, name, size, date, kind) = DETAIL_ROWS[*i];
            cell_text(ui, th, col, g, name, size, date, kind);
        },
    );
    let row_h = theme.table_cell_height().value();
    let rect = egui::Rect::from_min_size(egui::pos2(left, top + row_h), egui::vec2(width, row_h));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    let (_, field, _) = explorer_name_row(
        &mut child,
        theme,
        ExplorerNameLayout::Detail {
            inset: DETAIL_INSET.value(),
            name_width: width - explorer_detail_tail_width(false, theme),
        },
        icons::FOLDER,
        edit,
        error.is_some(),
    );
    if let Some(text) = error {
        explorer_name_error(ui, theme, field, text);
    }
}

fn heads(folder: Option<&str>) -> ExplorerDetailHeads<'_, usize> {
    ExplorerDetailHeads {
        name: (t("explorer.column.name"), 0),
        size: (t("explorer.column.size"), 1),
        modified: (t("explorer.column.modified"), 2),
        kind: (t("explorer.column.type"), 3),
        folder,
    }
}

fn detail_columns_table<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    rows: &[R],
    id: &str,
    cell: impl Fn(&mut egui::Ui, &Theme, &R, usize),
) {
    let columns = explorer_detail_columns(theme, heads(None));
    Table::new(columns)
        .active_sort(0_usize, TableSortDir::Asc)
        .header_fill(theme.table_header_bg().to_egui())
        .header_pad_right(theme.spacing_sm)
        .id_salt(id)
        .show(ui, theme, rows, |_| false, cell);
}

#[allow(clippy::too_many_arguments)] // 이유: 상세 표 한 행의 칸 값이 각각 따로 온다 — 묶을 구조체가 이 예제에만 쓰인다
fn cell_text(
    ui: &mut egui::Ui,
    th: &Theme,
    col: usize,
    g: MockGlyph,
    name: &str,
    size: &str,
    date: &str,
    kind: &str,
) {
    let muted = th.text_muted().to_egui();
    let mono = egui::FontId::monospace(th.font_size_caption.value());
    match col {
        0 => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                let sz = th.icon_glyph_size_md.value();
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(sz), egui::Sense::hover());
                g.image(sz, th.text_secondary().to_egui())
                    .paint_at(ui, rect);
                ui.label(
                    egui::RichText::new(name)
                        .size(th.font_size_body.value())
                        .color(th.table_row_fg().to_egui()),
                );
            });
        }
        1 => {
            ui.add_space(th.spacing_sm.value());
            ui.label(egui::RichText::new(size).font(mono).color(muted));
        }
        2 => {
            ui.label(egui::RichText::new(date).font(mono).color(muted));
        }
        _ => {
            // 폴더 행은 종류 칸을 비워 두고 본체와 같은 번역 문구를 쓴다.
            let kind = if kind.is_empty() {
                t("explorer.type.folder")
            } else {
                kind
            };
            ui.label(
                egui::RichText::new(kind)
                    .size(th.font_size_caption.value())
                    .color(muted),
            );
        }
    }
}

pub fn draw_name_errors(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Solo, |ui| {
        themes(ui, theme, |ui, th| {
            ui.vertical(|ui| {
                ui.set_width(CELL_W.value());
                ui.spacing_mut().item_spacing.y = ERROR_GAP.value();
                for (value, error) in [
                    ("", t("explorer.name.empty").to_string()),
                    ("drafts/2026", t_fmt("explorer.name.invalid_char", "/")),
                    ("report ", t("explorer.name.edge_space").to_string()),
                    ("Documents", t_fmt("explorer.name.exists", "Documents")),
                ] {
                    let mut edit = ExplorerNameEdit {
                        buf: value.to_string(),
                        initial_selection: None,
                    };
                    framed(ui, th, |ui| {
                        let (_, field, _) = explorer_name_row(
                            ui,
                            th,
                            ExplorerNameLayout::Detail {
                                inset: DETAIL_INSET.value(),
                                name_width: ui.available_width()
                                    - explorer_detail_tail_width(false, th),
                            },
                            icons::FOLDER,
                            &mut edit,
                            true,
                        );
                        explorer_name_error(ui, th, field, &error);
                    });
                }
                ui.add_space(ERROR_TAIL.value());
            });
        });
    });
    meta(
        ui,
        theme,
        &[
            ("input", "Input invalid (danger border)"),
            (
                "message",
                "under the field · menu-bg · 1px explorer-name-error-fg · caption · text-primary · max explorer-name-error-max-width 240 · overlaps rows below",
            ),
            ("empty", "“Enter a name.”"),
            (
                "invalid",
                "“A name can't contain “{char}”.” · Windows reserved: “This name is reserved by the system.”",
            ),
            (
                "edge space",
                "leading / trailing space refused, never trimmed · “Names can't start or end with a space.” · spaces only = empty · Windows trailing . = reserved",
            ),
            (
                "exists",
                "“{name}” already exists in this folder. — checked on Enter",
            ),
            ("after an error", "input stays open, text kept"),
        ],
        &[
            TokenChip::new(
                "explorer-name-error-fg",
                "→ accent-danger",
                theme.explorer_name_error_fg().to_egui(),
            ),
            TokenChip::new("menu-bg", "message box", theme.menu_bg().to_egui()),
            TokenChip::without_color("explorer-name-error-max-width", "→ size-240"),
            TokenChip::without_color("shadow-popover", "message lift"),
        ],
    );
    note(
        ui,
        theme,
        "i18n: explorer.new.folder_default · explorer.new.file_default · explorer.name.empty · \
         explorer.name.invalid_char · explorer.name.edge_space · explorer.name.reserved · \
         explorer.name.exists. The default \
         names are translated; the extension of the file default is not.",
    );
}
