//! 공용 Table의 고정 헤더, 행 선택, 정렬, 스크롤을 보여주는 예제.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    StatusKind, Table, TableAlign, TableColumn, TableColumnWidth, TableSortDir, TagVariant,
    status_dot, tag_mono,
};

use crate::catalog::spec::{StageVariant, TokenChip, body_column, meta, stage};
use tasty_type_geometry::length::LogicalPx;

/// 정렬 가능 컬럼 키 (port 컬럼만 정렬 데모).
#[derive(Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Port,
}

struct Row {
    port: u16,
    proto: &'static str,
    addr: &'static str,
    proc: &'static str,
    pid: u32,
    state: &'static str,
}

thread_local! {
    // 디자인 selectedKey = 8080 → index 3.
    static SELECTED: RefCell<usize> = const { RefCell::new(3) };
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let rows = [
        Row {
            port: 22,
            proto: "tcp",
            addr: "0.0.0.0",
            proc: "sshd",
            pid: 712,
            state: "LISTEN",
        },
        Row {
            port: 3000,
            proto: "tcp",
            addr: "127.0.0.1",
            proc: "node",
            pid: 48213,
            state: "LISTEN",
        },
        Row {
            port: 5432,
            proto: "tcp",
            addr: "127.0.0.1",
            proc: "postgres",
            pid: 1192,
            state: "LISTEN",
        },
        Row {
            port: 8080,
            proto: "tcp",
            addr: "0.0.0.0",
            proc: "tasty-agent",
            pid: 50321,
            state: "LISTEN",
        },
        Row {
            port: 9229,
            proto: "tcp",
            addr: "127.0.0.1",
            proc: "node",
            pid: 48213,
            state: "CLOSE_WAIT",
        },
    ];

    // Remainder 열이 창 폭까지 늘지 않도록 설명과 같은 본문 컬럼 폭 안에 둔다.
    body_column(ui, |ui| {
        stage(ui, theme, StageVariant::Tight, |ui| {
            let columns = vec![
                TableColumn {
                    title: "Port",
                    width: TableColumnWidth::Initial {
                        initial: LogicalPx(84.0),
                        at_least: LogicalPx(60.0),
                    },
                    align: TableAlign::Right,
                    sort_id: Some(SortKey::Port),
                },
                TableColumn {
                    title: "Proto",
                    width: TableColumnWidth::Initial {
                        initial: LogicalPx(76.0),
                        at_least: LogicalPx(60.0),
                    },
                    align: TableAlign::Left,
                    sort_id: None,
                },
                TableColumn {
                    title: "Address",
                    width: TableColumnWidth::Remainder {
                        at_least: LogicalPx(100.0),
                        clip: false,
                    },
                    align: TableAlign::Left,
                    sort_id: None,
                },
                TableColumn {
                    title: "Process",
                    width: TableColumnWidth::Remainder {
                        at_least: LogicalPx(100.0),
                        clip: false,
                    },
                    align: TableAlign::Left,
                    sort_id: None,
                },
                TableColumn {
                    title: "State",
                    width: TableColumnWidth::Initial {
                        initial: LogicalPx(140.0),
                        at_least: LogicalPx(100.0),
                    },
                    align: TableAlign::Left,
                    sort_id: None,
                },
            ];

            SELECTED.with(|s| {
                let mut sel = s.borrow_mut();
                let selected = *sel;
                let out = Table::new(columns)
                    .active_sort(SortKey::Port, TableSortDir::Asc)
                    .header_fill(egui::Color32::from(theme.bg_sidebar()))
                    .selectable(true)
                    .max_scroll_height(theme.overlay_top_offset * 3.0)
                    .id_salt("prim_table_demo")
                    .show(
                        ui,
                        theme,
                        &rows,
                        |row: &Row| rows.iter().position(|r| r.port == row.port) == Some(selected),
                        |ui, th, row, col| match col {
                            // kit 의 Process 는 strong 열이라 UI 글꼴 · text-primary 다.
                            3 => {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                                    ui.label(
                                        egui::RichText::new(row.proc)
                                            .size(th.font_size_body.value())
                                            .color(tasty_ui_widgets::table_cell_ink(
                                                ui,
                                                th,
                                                th.text_primary(),
                                            )),
                                    );
                                    tag_mono(
                                        ui,
                                        th,
                                        &row.pid.to_string(),
                                        TagVariant::Default,
                                        false,
                                    );
                                });
                            }
                            4 => {
                                let listen = row.state == "LISTEN";
                                let kind = if listen {
                                    StatusKind::Running
                                } else {
                                    StatusKind::Waiting
                                };
                                status_dot(
                                    ui,
                                    th,
                                    kind,
                                    row.state,
                                    listen,
                                    false,
                                    tasty_ui_widgets::Ground::Panel,
                                );
                            }
                            // kit 의 Port·Proto·Address 는 mono 열이라 text-primary 다.
                            _ => {
                                let text = match col {
                                    0 => row.port.to_string(),
                                    1 => row.proto.to_string(),
                                    _ => row.addr.to_string(),
                                };
                                ui.label(
                                    egui::RichText::new(text)
                                        .size(th.font_size_body.value())
                                        .monospace()
                                        .color(tasty_ui_widgets::table_cell_ink(
                                            ui,
                                            th,
                                            th.text_primary(),
                                        )),
                                );
                            }
                        },
                    );
                if let Some(i) = out.clicked_row {
                    *sel = i;
                }
            });
        });
    });

    meta(
        ui,
        theme,
        &[
            ("row", "28 · dense 22"),
            ("header", "sticky bg-sidebar"),
            ("cell font", "mono columns → font-mono"),
            ("selected", "surface-active"),
            ("hover", "overlay-hover"),
        ],
        &[
            TokenChip::new(
                "bg-sidebar",
                "header fill",
                egui::Color32::from(theme.bg_sidebar()),
            ),
            TokenChip::new(
                "surface-active",
                "selected row",
                egui::Color32::from(theme.surface_active()),
            ),
            TokenChip::without_color("overlay-hover", "row hover"),
            TokenChip::new(
                "accent-primary",
                "sort indicator",
                egui::Color32::from(theme.accent_primary()),
            ),
            TokenChip::without_color("font-mono", "port/addr/pid cells"),
        ],
    );
}
