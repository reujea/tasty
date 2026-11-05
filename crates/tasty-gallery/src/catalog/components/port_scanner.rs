//! 수신 포트 목록·상태 필터·즐겨찾기 예제. 실제 포트 조회는 실행하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, IconButton, IconButtonVariant, PORTS_PANEL_PAD_X, PortsColumn,
    StatusKind, TableAlign, TableColumn, TableColumnWidth, TagVariant, fixed_total_width,
    ports_favorite_detail_and_state, ports_process_cell, ports_star_column_width, ports_table,
    status_dot, tag,
};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

const WIDTH: LogicalPx = LogicalPx(660.0);

struct PortRow {
    port: &'static str,
    proto: &'static str,
    addr: &'static str,
    proc: &'static str,
    pid: &'static str,
    ws: &'static str,
    state: &'static str,
    selected: bool,
    favorited: bool,
}

/// 즐겨찾기 섹션 1행 mock — 매칭(LISTEN/other) 또는 NONE.
struct FavoriteRow {
    key: &'static str,
    detail: &'static str,
    /// `None` → NONE 배지(idle). `Some(true)` → running(pulse). `Some(false)` → waiting.
    listening: Option<bool>,
}

const FAVORITE_ROWS: &[FavoriteRow] = &[
    FavoriteRow {
        key: "127.0.0.1:3000",
        detail: "node · 48213 · Project A",
        listening: Some(true),
    },
    FavoriteRow {
        key: "0.0.0.0:9443",
        detail: "not running",
        listening: None,
    },
];

const ROWS: &[PortRow] = &[
    PortRow {
        port: "3000",
        proto: "tcp",
        addr: "127.0.0.1",
        proc: "node",
        pid: "48213",
        ws: "Project A",
        state: "LISTEN",
        selected: false,
        favorited: true,
    },
    PortRow {
        port: "5173",
        proto: "tcp",
        addr: "127.0.0.1",
        proc: "vite",
        pid: "48990",
        ws: "Project A",
        state: "LISTEN",
        selected: false,
        favorited: false,
    },
    PortRow {
        port: "8080",
        proto: "tcp",
        addr: "0.0.0.0",
        proc: "tasty-agent",
        pid: "50321",
        ws: "Project B",
        state: "LISTEN",
        selected: true,
        favorited: false,
    },
    PortRow {
        port: "8443",
        proto: "tcp6",
        addr: "::",
        proc: "tasty-agent",
        pid: "50321",
        ws: "Project B",
        state: "LISTEN",
        selected: false,
        favorited: false,
    },
    PortRow {
        port: "9229",
        proto: "tcp",
        addr: "127.0.0.1",
        proc: "node",
        pid: "48213",
        ws: "Project A",
        state: "CLOSE_WAIT",
        selected: false,
        favorited: false,
    },
];

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    kit::icon(
                        ui,
                        icons::PORT,
                        theme.icon_glyph_size_md,
                        theme.text_secondary().to_egui(),
                    );
                    kit::title(ui, theme, "Listening ports");
                    tag(ui, theme, "5 listening", TagVariant::Accent, false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        IconButton::new().variant(IconButtonVariant::Ghost).show(
                            ui,
                            theme,
                            &|ui, rect, c| icons::CLOSE.image(rect.height(), c).paint_at(ui, rect),
                        );
                        IconButton::new().variant(IconButtonVariant::Ghost).show(
                            ui,
                            theme,
                            &|ui, rect, c| {
                                icons::COLUMNS.image(rect.height(), c).paint_at(ui, rect)
                            },
                        );
                        IconButton::new().variant(IconButtonVariant::Ghost).show(
                            ui,
                            theme,
                            &|ui, rect, c| {
                                icons::REFRESH.image(rect.height(), c).paint_at(ui, rect)
                            },
                        );
                        kit::field(
                            ui,
                            theme,
                            Some(theme.field_width_md),
                            "Filter…",
                            true,
                            false,
                        );
                    });
                });
            });
            kit::hsep(ui, theme);

            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    let s = theme.icon_glyph_size_md.value();
                    let (r, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
                    ui.painter().rect_filled(
                        r,
                        theme.corner_radius_sm.value(),
                        theme.accent_primary().to_egui(),
                    );
                    icons::SHIELD_CHECK
                        .image(s, theme.text_on_accent().to_egui())
                        .paint_at(ui, r);
                    kit::body(
                        ui,
                        theme,
                        crate::i18n::t("port_scanner.filter_show_all_system"),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        funnel_button(ui, theme, "State · 1/3", true);
                    });
                });
            });

            draw_favorites_section(ui, theme, FAVORITE_ROWS);

            // 본체 popup 과 같은 열 정의(공용 PortsColumn)와 표 꾸밈을 쓴다. 열 하한의 합이 넘치면
            // 가로로 스크롤한다. Workspace·Tab 은 열 선택에서 숨긴 예제다.
            // 디자인 표는 popup 가장자리에서 별 열(28)부터 시작한다. 즐겨찾기 행도 같다.
            ports_table(columns(theme, FRAME_COLUMNS), theme)
                .id_salt("ports_table")
                .max_scroll_height(theme.measure_md * 0.7)
                .show(
                    ui,
                    theme,
                    ROWS,
                    |r| r.selected,
                    |ui, theme, row, c| cell(ui, theme, row, column_at(FRAME_COLUMNS, c)),
                );
            kit::hsep(ui, theme);

            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("5 of 5 ports")
                            .size(theme.font_size_caption.value())
                            .color(theme.text_disabled().to_egui()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        Button::new("Close")
                            .variant(ButtonVariant::Secondary)
                            .show(ui, theme);
                        Button::new("Copy address")
                            .variant(ButtonVariant::Ghost)
                            .show(ui, theme);
                    });
                });
            });
        });
    });

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        funnel_button(ui, theme, "State", false);
        // 버튼에 붙는 드롭다운이므로 본체 egui popup 과 같은 메뉴 컨테이너 틀을 쓴다.
        kit::frame_card_menu(ui, theme, LogicalPx(216.0), |ui| {
            // 본체 egui 팝오버와 같은 앵커 메뉴 안쪽 둘레.
            let ring = theme.popup_content_margin();
            kit::region_sym(ui, ring, ring, |ui| {
                kit::caption(ui, theme, "Filter by state", true);
                ui.add_space(theme.spacing_xs.value());
                check_row(ui, theme, "LISTEN", true);
                check_row(ui, theme, "ESTABLISHED", false);
                check_row(ui, theme, "CLOSE_WAIT", false);
                kit::hsep(ui, theme);
                ui.horizontal(|ui| {
                    Button::new("Select all")
                        .variant(ButtonVariant::Ghost)
                        .show(ui, theme);
                    Button::new("Deselect all")
                        .variant(ButtonVariant::Ghost)
                        .show(ui, theme);
                });
                ui.horizontal(|ui| {
                    Button::new("Reset (LISTEN only)")
                        .variant(ButtonVariant::Ghost)
                        .show(ui, theme);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        Button::new("Apply")
                            .variant(ButtonVariant::Primary)
                            .show(ui, theme);
                    });
                });
            });
        });
    });

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // 팝업 전체가 아닌 내부 섹션 예제이므로 그림자를 추가하지 않는다.
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            draw_favorites_section(ui, theme, &[]);
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "660×520 · bg-panel"),
            (
                "header",
                "icon · title · count · filter · columns · refresh · close",
            ),
            ("table", "min-width cols · h-scroll · sticky header"),
            ("fav column", "leading 28px, no header label, always shown"),
            (
                "favorites",
                "bounded caption(22) + list(max 112) · system-wide",
            ),
            (
                "favorites empty",
                "caption stays · faded star(37%) + hint row",
            ),
            ("columns", "chooser hides cols (Workspace hidden here)"),
            (
                "state filter",
                "funnel button · dropdown · default LISTEN-only",
            ),
            ("header bg", "bg-sidebar · mono caption"),
            ("footer", "count · Copy address · Close"),
            ("identity", "(addr, port)"),
            ("star", "22×22 hit · tight 28px column"),
            ("row height", "22 (tree density)"),
            ("scope", "always system-wide"),
        ],
        &[
            TokenChip::new("bg-panel", "frame", theme.bg_panel().to_egui()),
            TokenChip::new("bg-sidebar", "header row", theme.bg_sidebar().to_egui()),
            TokenChip::new(
                "surface-active",
                "selected row",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "accent-success",
                "listening",
                theme.accent_success().to_egui(),
            ),
            TokenChip::without_color("font-mono", "port/addr/pid"),
            TokenChip::new(
                "accent-warning",
                "star on (favorited)",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "port-star-on",
                "registered star",
                theme.port_star_on().to_egui(),
            ),
            TokenChip::new(
                "port-star-off",
                "outline star",
                theme.port_star_off().to_egui(),
            ),
            TokenChip::new(
                "port-favorites-bg",
                "section tone",
                theme.port_favorites_bg().to_egui(),
            ),
            TokenChip::new(
                "port-state-none-dot",
                "NONE dot",
                theme.port_state_none_dot().to_egui(),
            ),
            TokenChip::without_color("port-favorites-max-height", "scroll cap"),
            TokenChip::without_color("port-process-col-min-width", "Process floor"),
            TokenChip::without_color("port-star-col-width", "leading star column"),
        ],
    );

    spec::note(
        ui,
        theme,
        "Port, Proto and Address cells are monospace so they line up by glyph; \
         Process and the other cells use the UI font. Each column has a minimum \
         width; when the visible columns' minimums exceed the frame the table \
         scrolls horizontally (instead of ellipsizing). The columns chooser \
         (header) toggles which columns show — here Workspace is hidden. \
         Selecting a row enables Copy address; the sticky header keeps column \
         labels visible while scrolling. The state filter (funnel button, filter \
         row) defaults to LISTEN-only; its dropdown is a shown set — checked \
         states are shown, Reset restores LISTEN-only, Apply commits the draft. \
         The favorites section (between the filter row and the table) always \
         shows pinned ports regardless of the table's scope/search/state \
         filter — its LISTEN/NONE judgment is system-wide. Its list is bounded \
         to 112px (5 rows) before scrolling; a leading 28px star column (no \
         header label, not hideable) toggles favorites in both the section and \
         the main table. In a favorites row the detail starts space-md after \
         addr:port and ellipsizes; the state dot sits right-aligned in a column \
         at least 112px (size-112) wide at the row end, so a long detail never \
         touches the dot.",
    );
    spec::do_(
        ui,
        theme,
        "Do keep the favorites rows as summary rows (addr:port · process · state), not the \
         7-column grid — a stopped port has no process/workspace/tab data to show, and the \
         summary row never needs the table's horizontal scroll. The star column width is \
         shared, so stars still line up across both regions.",
    );
    spec::note(
        ui,
        theme,
        "Strings: port_scanner.favorites_heading (\"Favorites\") · favorites_count · \
         favorites_system_wide (\"system-wide\") · favorites_empty · favorites_not_running · \
         state_none_label (\"NONE\"). Leave 20–40% growth room for ko/ja/de — the caption row \
         and the empty line are single-line by design.",
    );
}

/// 상태 필터 funnel 버튼 mock — 본체 `state_filter_button` 전사. applied 면 accent
/// 채움 + on-accent, 아니면 surface-raised + border.
fn funnel_button(ui: &mut egui::Ui, theme: &Theme, label: &str, applied: bool) {
    let text_col = if applied {
        theme.text_on_accent().to_egui()
    } else {
        theme.text_primary().to_egui()
    };
    let fill = if applied {
        theme.accent_primary().to_egui()
    } else {
        theme.surface_raised().to_egui()
    };
    let stroke = if applied {
        egui::Stroke::NONE
    } else {
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui())
    };
    ui.add(
        egui::Button::image_and_text(
            icons::FUNNEL.image(theme.icon_glyph_size_sm.value(), text_col),
            egui::RichText::new(label)
                .color(text_col)
                .size(theme.font_size_body.value()),
        )
        .fill(fill)
        .stroke(stroke),
    );
}

/// 드롭다운 체크박스 행 mock — checked 면 accent 채움 + check, 아니면 빈 보더 박스.
fn check_row(ui: &mut egui::Ui, theme: &Theme, label: &str, checked: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let s = theme.icon_glyph_size_md.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
        if checked {
            ui.painter().rect_filled(
                r,
                theme.corner_radius_sm.value(),
                theme.accent_primary().to_egui(),
            );
            icons::CHECK
                .image(s, theme.text_on_accent().to_egui())
                .paint_at(ui, r);
        } else {
            ui.painter().rect_stroke(
                r,
                theme.corner_radius_sm.value(),
                egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
                egui::StrokeKind::Inside,
            );
        }
        kit::body(ui, theme, label);
    });
}

/// 기본 프레임 예제가 보여 주는 열(별 열 제외). Workspace·Tab 은 숨긴 상태다.
const FRAME_COLUMNS: &[PortsColumn] = &[
    PortsColumn::Port,
    PortsColumn::Proto,
    PortsColumn::Address,
    PortsColumn::Process,
    PortsColumn::State,
];

/// Process 열 예제가 보여 주는 열(별 열 제외). 시안 popup 컬럼 세트와 같고 Tab 은 숨긴 상태다.
const SPECIMEN_COLUMNS: &[PortsColumn] = &[
    PortsColumn::Port,
    PortsColumn::Proto,
    PortsColumn::Address,
    PortsColumn::Process,
    PortsColumn::Workspace,
    PortsColumn::State,
];

fn column_title(col: PortsColumn) -> &'static str {
    match col {
        PortsColumn::Port => "Port",
        PortsColumn::Proto => "Proto",
        PortsColumn::Address => "Address",
        PortsColumn::Process => "Process",
        PortsColumn::Workspace => "Workspace",
        PortsColumn::Tab => "Tab",
        PortsColumn::State => "State",
    }
}

/// 별 열과 `visible` 열의 공용 Table 열 정의.
fn columns(theme: &Theme, visible: &[PortsColumn]) -> Vec<TableColumn<'static, ()>> {
    let star = TableColumn {
        title: "",
        width: ports_star_column_width(theme),
        align: TableAlign::Left,
        sort_id: None,
    };
    std::iter::once(star)
        .chain(visible.iter().map(|c| TableColumn {
            title: column_title(*c),
            width: c.width(theme),
            align: c.align(),
            sort_id: None,
        }))
        .collect()
}

/// 표의 열 번호를 열 종류로 바꾼다. 0번은 별 열이라 `None` 이다.
fn column_at(visible: &[PortsColumn], index: usize) -> Option<PortsColumn> {
    index.checked_sub(1).map(|i| visible[i])
}

/// 즐겨찾기 캡션과 높이가 제한된 목록. 비어 있어도 캡션과 안내는 표시한다.
fn draw_favorites_section(ui: &mut egui::Ui, theme: &Theme, favorites: &[FavoriteRow]) {
    let fav_row_h = theme.item_height_tree.value();
    let fav_ir = egui::Frame::NONE
        .fill(theme.bg_sidebar().to_egui())
        .show(ui, |ui| {
            // 행은 왼쪽 여백 없이 별 칸부터 시작하고, 캡션과 빈 안내 줄만 왼쪽 여백을 둔다.
            // 본체 popup과 같은 좌우 여백(디자인 `--tasty-size-14`).
            let inset = f32::from(PORTS_PANEL_PAD_X);
            let margin = egui::Margin {
                left: 0,
                right: PORTS_PANEL_PAD_X,
                top: 0,
                bottom: 0,
            };
            kit::region(ui, margin, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), fav_row_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(inset);
                        let heading = if favorites.is_empty() {
                            "Favorites".to_string()
                        } else {
                            format!("Favorites · {}", favorites.len())
                        };
                        kit::caption(ui, theme, &heading, false);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            kit::caption(ui, theme, "system-wide", false);
                        });
                    },
                );

                if favorites.is_empty() {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), fav_row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(inset);
                            ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
                            let sz = theme.icon_glyph_size_sm.value();
                            let (r, _) =
                                ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::hover());
                            // 즐겨찾기 별 아이콘 톤. 대응 토큰 없음 — 본체와 같은 값을 여기 다시 적는다.
                            const FAV_STAR_ICON_OPACITY: f32 = 0.37;
                            icons::STAR
                                .image(
                                    sz,
                                    theme
                                        .text_muted()
                                        .to_egui()
                                        .gamma_multiply(FAV_STAR_ICON_OPACITY),
                                )
                                .paint_at(ui, r);
                            ui.label(
                                egui::RichText::new(crate::i18n::t("port_scanner.favorites_empty"))
                                    .italics()
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                        },
                    );
                } else {
                    for fav in favorites {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), fav_row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(theme.port_star_col_width().value(), fav_row_h),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| star(ui, theme, true),
                                );
                                ui.label(
                                    egui::RichText::new(fav.key)
                                        .monospace()
                                        .size(theme.font_size_caption.value())
                                        .color(theme.text_primary().to_egui()),
                                );
                                let (kind, state, pulse) = match fav.listening {
                                    Some(true) => (StatusKind::Running, "LISTEN", true),
                                    Some(false) => (StatusKind::Waiting, "CLOSE_WAIT", false),
                                    None => (StatusKind::Idle, "NONE", false),
                                };
                                ports_favorite_detail_and_state(
                                    ui, theme, fav.detail, kind, state, pulse, false,
                                );
                            },
                        );
                    }
                }
            });
        });
    ui.painter().hline(
        fav_ir.response.rect.x_range(),
        fav_ir.response.rect.bottom(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

/// `PortStar` mock — 22×22, on(채운 STAR_FILL + accent-warning) / off(outline STAR
/// + text-muted). 본체 `port_scanner.rs::draw_port_star` 전사.
fn star(ui: &mut egui::Ui, theme: &Theme, on: bool) {
    let side = theme.item_height_tree.value();
    // 표의 tight 열과 즐겨찾기 행의 별 칸 모두 디자인처럼 가운데 둔다.
    ui.add_space(((ui.available_width() - side) * 0.5).max(0.0));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let glyph = theme.icon_glyph_size_sm.value();
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
    let selected = tasty_ui_widgets::in_selected_table_row(ui);
    if on {
        let fg = if selected {
            theme.port_star_on_active()
        } else {
            theme.port_star_on()
        };
        icons::STAR_FILL
            .image(glyph, fg.to_egui())
            .paint_at(ui, icon_rect);
    } else {
        let fg = if selected {
            theme.port_star_off_active()
        } else {
            theme.port_star_off()
        };
        icons::STAR
            .image(glyph, fg.to_egui())
            .paint_at(ui, icon_rect);
    }
}

fn cell(ui: &mut egui::Ui, theme: &Theme, row: &PortRow, col: Option<PortsColumn>) {
    // kit `Table` 의 td 는 font-size-body 이고 mono 열만 font-mono 다.
    let text = |ui: &mut egui::Ui, text: &str, mono: bool| {
        let rich = egui::RichText::new(text)
            .size(theme.font_size_body.value())
            .color(tasty_ui_widgets::table_cell_ink(
                ui,
                theme,
                theme.text_primary(),
            ));
        ui.label(if mono { rich.monospace() } else { rich });
    };
    let Some(col) = col else {
        // 별 열은 kit 의 tight 열이라 여백 없이 가운데 둔다.
        star(ui, theme, row.favorited);
        return;
    };
    // 값 셀은 본체·kit `Table` td 처럼 정렬 쪽에 table-cell-padding-x 를 둔다. 오른쪽 정렬 열은
    // 오른쪽에서 왼쪽으로 쌓으므로 같은 여백이 값 오른쪽에 붙는다.
    ui.add_space(theme.table_cell_padding_x().value());
    // kit 열 정의: Port·Proto·Address 는 mono, Process 는 strong(UI 글꼴)이라 모두
    // text-primary 다. Workspace 는 표의 행 글자색을 따르고 State 는 StatusDot 이다.
    match col {
        PortsColumn::Port => text(ui, row.port, true),
        PortsColumn::Proto => text(ui, row.proto, true),
        PortsColumn::Address => text(ui, row.addr, true),
        PortsColumn::Process => {
            ports_process_cell(ui, theme, row.proc, Some(row.pid));
        }
        PortsColumn::Workspace | PortsColumn::Tab => {
            let value = if col == PortsColumn::Workspace {
                row.ws
            } else {
                ""
            };
            if value.is_empty() {
                ui.colored_label(
                    tasty_ui_widgets::table_cell_ink(ui, theme, theme.text_muted()),
                    "—",
                );
            } else {
                ui.label(egui::RichText::new(value).size(theme.font_size_body.value()));
            }
        }
        PortsColumn::State => {
            let listen = row.state == "LISTEN";
            let kind = if listen {
                StatusKind::Running
            } else {
                StatusKind::Waiting
            };
            status_dot(ui, theme, kind, row.state, listen, false);
        }
    }
}

/// 시안 "Process column" 예제의 표 폭: 열 하한 합보다 넓은 860, popup 폭 660.
const PROC_TABLE_WIDE: LogicalPx = LogicalPx(860.0);
const PROC_TABLE_POPUP: LogicalPx = LogicalPx(660.0);

/// 시안 예제의 세 행. 5432 는 Tasty 밖의 프로세스라 Workspace 가 비어 있다.
const PROC_ROWS: &[PortRow] = &[
    PortRow {
        port: "3000",
        proto: "tcp",
        addr: "127.0.0.1",
        proc: "node /usr/local/bin/vite --host --strictPort",
        pid: "41822",
        ws: "Project A",
        state: "LISTEN",
        selected: false,
        favorited: false,
    },
    PortRow {
        port: "5432",
        proto: "tcp",
        addr: "127.0.0.1",
        proc: "postgres: checkpointer",
        pid: "913",
        ws: "",
        state: "LISTEN",
        selected: false,
        favorited: false,
    },
    PortRow {
        port: "8080",
        proto: "tcp",
        addr: "0.0.0.0",
        proc: "tasty-agent",
        pid: "50321",
        ws: "Project B",
        state: "LISTEN",
        selected: false,
        favorited: true,
    },
];

/// Overlays › Listening ports — Process 열은 고정 폭이 아니라 최소 폭이다. 본체 popup 의 열 정의와
/// 공용 Table 로 그리므로 헤더·행 높이·글꼴·여백이 popup 과 같다.
pub fn draw_process_column(ui: &mut egui::Ui, theme: &Theme) {
    let cols = columns(theme, SPECIMEN_COLUMNS);
    let widths: Vec<TableColumnWidth> = cols.iter().map(|c| c.width).collect();
    let budget = fixed_total_width(&widths, LogicalPx(ui.spacing().item_spacing.x));
    let addr = theme.port_addr_col_min_width();
    // 시안대로 두 예제 사이는 space-lg(Column stage 간격), 캡션 아래는 space-sm 이다.
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (label, w) in [
            (
                format!(
                    "860 — wider than the column budget ({:.0}): Process takes ALL the spare width, Address stays at {:.0}",
                    budget.value(),
                    addr.value()
                ),
                PROC_TABLE_WIDE,
            ),
            (
                format!(
                    "660 — popup width, under the budget ({:.0}): the body scrolls sideways; nothing shrinks or hides",
                    budget.value()
                ),
                PROC_TABLE_POPUP,
            ),
        ] {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                ui.label(
                    egui::RichText::new(label)
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted().to_egui()),
                );
                process_table(ui, theme, w);
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("component", "Table — the popup's column defs, shared"),
            (
                "Process",
                "Flex — min width port-process-col-min-width · ellipsis · PID Tag stays",
            ),
            ("value", "200 — unchanged"),
            ("Address", "fixed · port-addr-col-min-width (floor = width)"),
            ("spare width", "100% to Process"),
            (
                "stage gaps",
                "gallery Column stage space-lg between tables · cluster caption space-sm",
            ),
            (
                "overflow",
                "body scrolls horizontally below the column budget (also at 660); no column hides",
            ),
            (
                "fixed columns",
                "width = floor (star · Port · Proto · Workspace · State)",
            ),
            (
                "metrics",
                "Table's own — table-cell-height header and rows · table-cell-padding-x · caps header",
            ),
            ("zoom", "scales with the UI scale, like every width token"),
        ],
        &[
            TokenChip::without_color("port-process-col-min-width", "Process floor"),
            TokenChip::without_color("port-addr-col-min-width", "Address floor + width"),
            TokenChip::without_color("port-star-col-width", "leading star column"),
            TokenChip::without_color("table-cell-height", "header + rows"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Drawn with the popup's own column definitions on the shared Table, so there are no \
         specimen-only numbers. New Table column width Flex (the design's minWidth) — additive.",
    );
}

/// `width` 폭 테두리 상자 안의 포트 표. 열 하한 합보다 좁으면 표 본문이 가로로 스크롤한다.
fn process_table(ui: &mut egui::Ui, theme: &Theme, width: LogicalPx) {
    let bw = theme.border_width.value();
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(bw, theme.border_strong().to_egui()))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            let inner = width.value() - bw * 2.0;
            ui.set_width(inner);
            ports_table(columns(theme, SPECIMEN_COLUMNS), theme)
                .id_salt(("ports_process_specimen", width.value() as u32))
                .show(
                    ui,
                    theme,
                    PROC_ROWS,
                    |r| r.selected,
                    |ui, theme, row, c| cell(ui, theme, row, column_at(SPECIMEN_COLUMNS, c)),
                );
        });
}
