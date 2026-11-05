//! MultiSelect 예제. 시안 `components.jsx` 의 9칸 상태 그리드 + Select·MultiSelect 폼 짝.
//! 닫힌 칸은 공용 `multi_select` 위젯 그대로, 열린 칸은 공용 위젯이 한 번에 하나의 팝업만
//! 열 수 있어 같은 토큰으로 트리거와 메뉴를 정적으로 그린다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    MultiSelectAllToggle, MultiSelectLabels, checkbox, checkbox_width, multi_select, select,
};

use super::glyph::MockGlyph;
use crate::catalog::icons::CHEVRON_UP;
use crate::catalog::spec::{StageVariant, TokenChip, cluster, meta, note, stage};

// 시안 그리드·무대의 전시 치수. 대응 토큰이 없다.
/// 상태 그리드 한 칸 폭 — `gridTemplateColumns: repeat(3, 240px)`.
const CELL_W: LogicalPx = LogicalPx(240.0);
/// 그리드 칸 사이 — `gap: 20`. 무대 안쪽 여백(`padding: 20`)도 같은 값이다.
const GRID_GAP: LogicalPx = LogicalPx(20.0);
/// 그리드와 폼 짝 사이 — 무대 `gap: 22`.
const STAGE_GAP: LogicalPx = LogicalPx(22.0);
/// 닫힌 트리거 한 칸의 최소 높이 — 칸 `minHeight: 44`.
const CELL_MIN_CLOSED: LogicalPx = LogicalPx(44.0);
/// 열린 메뉴 칸의 최소 높이 — 칸 `minHeight: 230`.
const CELL_MIN_OPEN: LogicalPx = LogicalPx(230.0);
/// 긴 라벨 칸의 최소 높이 — 칸 `minHeight: 200`.
const CELL_MIN_LONG: LogicalPx = LogicalPx(200.0);
/// 폼 짝 패널의 세로 여백 — `padding: "14px 16px"` 의 14.
const PAIR_PAD_Y: LogicalPx = LogicalPx(14.0);
/// 폼 짝 라벨과 컨트롤 사이 — `gap: 6`.
const PAIR_LABEL_GAP: LogicalPx = LogicalPx(6.0);

/// 시안 `MS_STATES` — DAG 목록 상태 필터(첫 소비자).
const STATES: [&str; 6] = ["Waiting", "Ready", "Running", "Done", "Failed", "Cancelled"];
/// 시안 `MS_LONG`.
const LONG: [&str; 3] = [
    "release-candidate-nightly-build-pipeline",
    "integration-tests-postgres-and-redis",
    "docs",
];
/// 시안 `MS_MANY` — "Workspace 01" … "Workspace 20".
const MANY: [&str; 20] = [
    "Workspace 01",
    "Workspace 02",
    "Workspace 03",
    "Workspace 04",
    "Workspace 05",
    "Workspace 06",
    "Workspace 07",
    "Workspace 08",
    "Workspace 09",
    "Workspace 10",
    "Workspace 11",
    "Workspace 12",
    "Workspace 13",
    "Workspace 14",
    "Workspace 15",
    "Workspace 16",
    "Workspace 17",
    "Workspace 18",
    "Workspace 19",
    "Workspace 20",
];

/// 시안 `msSummary` — 0 → placeholder · N → "N selected" · 전부 → "All".
const LABELS: MultiSelectLabels<'static> = MultiSelectLabels {
    none: "No status",
    some: "{} selected",
    some_one: "{} selected",
    all: "All",
};

struct MsState {
    live: [bool; 6],
    pair_sort: usize,
    pair_status: [bool; 6],
    // 아래는 시안 정적 칸을 공용 위젯으로 직접 조작해 보는 회귀 예제.
    live_long: [bool; 3],
    live_rows: [bool; 6],
    live_many: [bool; 20],
    live_masked: [bool; 6],
}

thread_local! {
    static STATE: RefCell<MsState> = const {
        RefCell::new(MsState {
            live: [true, true, true, false, false, false],
            pair_sort: 0,
            pair_status: [true, true, true, false, false, false],
            live_long: [true, true, false],
            live_rows: [false, false, true, false, false, false],
            live_many: {
                let mut m = [false; 20];
                m[1] = true;
                m[4] = true;
                m
            },
            live_masked: [false, true, true, true, true, true],
        })
    };
}

/// 정적 메뉴 한 개의 내용.
struct MenuSpec<'a> {
    options: &'a [&'a str],
    selected: &'a [bool],
    disabled: &'a [bool],
    /// 키보드 ↑↓ 활성 행. 시안은 `activeIndex` 를 주지 않으면 0 행이다.
    active: Option<usize>,
    /// 포인터 hover 행(`hoverIndex`).
    hover: Option<usize>,
    /// `allToggle` 행 문구. 전부 켜지지 않았으면 "Select all".
    all_toggle: Option<&'static str>,
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let cell_w = CELL_W.value();
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        stage(ui, theme, StageVariant::Tight, |ui| {
            egui::Frame::new()
                .fill(theme.bg_app().to_egui())
                .corner_radius(theme.corner_radius.value())
                .inner_margin(egui::Margin::same(GRID_GAP.value() as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = STAGE_GAP.value();
                    ui.vertical(|ui| {
                        grid(ui, theme, cell_w, &mut st);
                        centered_pair(ui, theme, cell_w, &mut st);
                    });
                });
            egui::Frame::new()
                .inner_margin(egui::Margin::same(GRID_GAP.value() as i8))
                .show(ui, |ui| live_regressions(ui, theme, &mut st));
        });
    });

    meta(
        ui,
        theme,
        &[
            (
                "trigger",
                "Select language — 28px multiselect-height · 12 left · 28 chevron room",
            ),
            (
                "open trigger",
                "border-focus + 1px ring, chevron rotates 180°",
            ),
            (
                "summary",
                "3 branches, caller-injected · plain text · single line + ellipsis",
            ),
            (
                "menu",
                "4 below the trigger · min-width = trigger · grows to content up to multiselect-menu-max-width (320)",
            ),
            ("row", "28px · 12 padding-x · 8 box→label · Checkbox 16px"),
            ("checked row", "checkmark only — no background"),
            (
                "overflow",
                "scrolls past 220 multiselect-menu-max-height · .tasty-scroll",
            ),
            (
                "bulk row",
                "allToggle — accent action row + separator, off below ~8 options",
            ),
            (
                "keys",
                "↓/↵/Space open · ↑↓ Home End move · Space/↵ toggle (menu stays) · Esc close",
            ),
            ("motion", "border/chevron 120ms · check state 0ms"),
        ],
        &[
            TokenChip::new(
                "multiselect-bg",
                "trigger fill",
                egui::Color32::from(theme.multiselect_bg()),
            ),
            TokenChip::new(
                "multiselect-border",
                "trigger edge",
                egui::Color32::from(theme.multiselect_border()),
            ),
            TokenChip::new(
                "multiselect-border-focus",
                "focus / open",
                egui::Color32::from(theme.multiselect_border_focus()),
            ),
            TokenChip::new(
                "multiselect-summary-fg-empty",
                "0 selected",
                egui::Color32::from(theme.multiselect_summary_fg_empty()),
            ),
            TokenChip::new(
                "multiselect-menu-bg",
                "menu fill",
                egui::Color32::from(theme.multiselect_menu_bg()),
            ),
            TokenChip::new(
                "multiselect-row-bg-hover",
                "pointer hover",
                theme.multiselect_row_bg_hover().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "multiselect-row-bg-active",
                "keyboard-active",
                egui::Color32::from(theme.multiselect_row_bg_active()),
            ),
            TokenChip::new(
                "multiselect-row-fg",
                "row label (primary, not muted)",
                egui::Color32::from(theme.multiselect_row_fg()),
            ),
            TokenChip::new(
                "checkbox-bg-checked",
                "checked box",
                egui::Color32::from(theme.checkbox_bg_checked()),
            ),
            TokenChip::new(
                "multiselect-all-fg",
                "bulk select/clear",
                egui::Color32::from(theme.multiselect_all_fg()),
            ),
        ],
    );

    note(
        ui,
        theme,
        "Decisions (implementation spec). Sibling, not a Select variant — the open surface is a \
         different thing (checkbox menu, non-native, stays open), so it earns its own catalog \
         entry; Select is untouched. Open = focus treatment + flipped chevron, no third visual \
         state. Bulk toggle exists but is opt-in (allToggle): one accent action row that reads \
         \"Select all\" and becomes \"Clear all\" once everything is on — a checkbox there would \
         need an indeterminate state this system doesn't have. Menu may outgrow the trigger up \
         to 320, because the trigger holds a short summary while the rows hold real labels. \
         Per-option disabled is supported (a status with no runs, a permission-gated option) — \
         row keeps state-disabled-opacity and is not togglable. Count is plain text, not \
         a Badge: badges live on tabs and rails, not inside form controls. Zero selected is \
         allowed — for a filter, nothing checked should mean no filter (show all) rather than an \
         empty view; forcing a minimum is consumer policy, not a control rule. Row labels are \
         text-primary (never muted) so Latte stays ≥4.5:1 on surface-raised. First consumer: \
         the DAG list status filter (ui_kits/terminal/overlays/dag_view.jsx) — 6 statuses, \
         allToggle off.",
    );
    note(
        ui,
        theme,
        "The live cells open the shared multi_select widget \
         (crates/tasty-ui-widgets/src/multi_select.rs), whose menu rows are checkbox height with \
         no pointer hover fill, not the kit 28px rows with a hover background.",
    );
}

/// 시안 9칸 그리드 — 3열 × 3행, 칸마다 라벨 + 컨트롤.
fn grid(ui: &mut egui::Ui, theme: &Theme, cell_w: f32, st: &mut MsState) {
    let open3 = [true, true, true, false, false, false];
    let none6 = [false; 6];
    let mut cancelled = none6;
    cancelled[5] = true;
    let mut running = none6;
    running[2] = true;
    let row_w = cell_w * 3.0 + GRID_GAP.value() * 2.0;
    ui.vertical(|ui| {
        ui.set_width(row_w);
        ui.spacing_mut().item_spacing.y = GRID_GAP.value();
        grid_row(ui, cell_w, [CELL_MIN_CLOSED; 3], |ui, i| match i {
            0 => cell(ui, theme, "live — click to open, toggle freely", |ui| {
                multi_select(
                    ui,
                    theme,
                    "g_ms_live",
                    &mut st.live,
                    &STATES,
                    None,
                    &LABELS,
                    None,
                    cell_w,
                    true,
                );
            }),
            1 => cell(
                ui,
                theme,
                "summary — 0 selected (placeholder tone)",
                |ui| {
                    let mut v = none6;
                    multi_select(
                        ui,
                        theme,
                        "g_ms_zero",
                        &mut v,
                        &STATES,
                        None,
                        &LABELS,
                        None,
                        cell_w,
                        true,
                    );
                },
            ),
            _ => cell(ui, theme, "summary — all selected", |ui| {
                let mut v = [true; 6];
                multi_select(
                    ui, theme, "g_ms_all", &mut v, &STATES, None, &LABELS, None, cell_w, true,
                );
            }),
        });
        grid_row(ui, cell_w, [CELL_MIN_OPEN; 3], |ui, i| match i {
            0 => cell(
                ui,
                theme,
                "focus / open — border-focus + 1px ring, chevron flipped",
                |ui| {
                    pinned(
                        ui,
                        theme,
                        cell_w,
                        "3 selected",
                        &MenuSpec {
                            options: &STATES,
                            selected: &open3,
                            disabled: &none6,
                            active: Some(0),
                            hover: None,
                            all_toggle: None,
                        },
                    );
                },
            ),
            1 => cell(
                ui,
                theme,
                "rows — hover (row 4) vs keyboard-active (row 1)",
                |ui| {
                    pinned(
                        ui,
                        theme,
                        cell_w,
                        "3 selected",
                        &MenuSpec {
                            options: &STATES,
                            selected: &open3,
                            disabled: &none6,
                            active: Some(0),
                            hover: Some(3),
                            all_toggle: None,
                        },
                    );
                },
            ),
            _ => cell(
                ui,
                theme,
                "per-option disabled (row 6: no cancelled runs)",
                |ui| {
                    pinned(
                        ui,
                        theme,
                        cell_w,
                        "1 selected",
                        &MenuSpec {
                            options: &STATES,
                            selected: &running,
                            disabled: &cancelled,
                            active: None,
                            hover: None,
                            all_toggle: None,
                        },
                    );
                },
            ),
        });
        grid_row(
            ui,
            cell_w,
            [CELL_MIN_CLOSED, CELL_MIN_LONG, CELL_MIN_OPEN],
            |ui, i| match i {
                0 => {
                    cell(ui, theme, "disabled control", |ui| {
                        let mut v = running;
                        multi_select(
                            ui,
                            theme,
                            "g_ms_disabled",
                            &mut v,
                            &STATES,
                            None,
                            &LABELS,
                            None,
                            cell_w,
                            false,
                        );
                    });
                }
                1 => {
                    cell(
                        ui,
                        theme,
                        "long labels — ellipsis in trigger and rows",
                        |ui| {
                            pinned(
                                ui,
                                theme,
                                cell_w,
                                "2 pipelines selected — very long summary",
                                &MenuSpec {
                                    options: &LONG,
                                    selected: &[true, true, false],
                                    disabled: &[false; 3],
                                    active: None,
                                    hover: None,
                                    all_toggle: None,
                                },
                            );
                        },
                    );
                }
                _ => cell(ui, theme, "20 options → internal scroll at 220", |ui| {
                    let mut sel = [false; 20];
                    sel[1] = true;
                    sel[4] = true;
                    pinned(
                        ui,
                        theme,
                        cell_w,
                        "2 selected",
                        &MenuSpec {
                            options: &MANY,
                            selected: &sel,
                            disabled: &[false; 20],
                            active: None,
                            hover: None,
                            all_toggle: Some("Select all"),
                        },
                    );
                }),
            },
        );
    });
}

/// 그리드 한 행 — 세 칸을 위쪽 정렬로 고정 열 위치에 둔다. 칸마다 시안 `minHeight` 를 받는다.
/// 시안 메뉴는 `position: absolute` 라 칸 폭을 밀지 않는다. 그래서 열 위치는 고정하고
/// 행 높이만 칸 내용에 맞춘다. 메뉴가 칸보다 넓으면 오른쪽 칸 위로 겹친다(시안과 같다).
fn grid_row(
    ui: &mut egui::Ui,
    cell_w: f32,
    min_h: [LogicalPx; 3],
    mut add_cell: impl FnMut(&mut egui::Ui, usize),
) {
    let origin = ui.cursor().min;
    let mut bottom = origin.y;
    for (i, h) in min_h.iter().enumerate() {
        let x = origin.x + (cell_w + GRID_GAP.value()) * i as f32;
        let rect =
            egui::Rect::from_min_size(egui::pos2(x, origin.y), egui::vec2(cell_w, f32::INFINITY));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_width(cell_w);
        child.set_min_height(h.value());
        add_cell(&mut child, i);
        bottom = bottom.max(child.min_rect().bottom());
    }
    let row_w = cell_w * 3.0 + GRID_GAP.value() * 2.0;
    ui.allocate_exact_size(egui::vec2(row_w, bottom - origin.y), egui::Sense::hover());
}

/// 칸 라벨(11px muted) + 컨트롤, 사이 8.
fn cell(ui: &mut egui::Ui, theme: &Theme, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
    ui.label(
        egui::RichText::new(label)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
    add(ui);
}

/// 열린 상태로 고정한 트리거 + 4 아래의 메뉴.
fn pinned(ui: &mut egui::Ui, theme: &Theme, width: f32, summary: &str, menu: &MenuSpec<'_>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.multiselect_menu_gap().value();
        static_trigger(ui, theme, width, summary);
        static_menu(ui, theme, width, menu);
    });
}

/// 열린 트리거 — border-focus 테두리 + 바깥 1px 링, 위를 향한 chevron.
fn static_trigger(ui: &mut egui::Ui, theme: &Theme, width: f32, summary: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, theme.multiselect_height().value()),
        egui::Sense::hover(),
    );
    let bw = theme.border_width.value();
    let radius = theme.multiselect_radius().value();
    let focus = theme.multiselect_border_focus().to_egui();
    let painter = ui.painter();
    painter.rect(
        rect,
        radius,
        theme.multiselect_bg().to_egui(),
        egui::Stroke::new(bw, focus),
        egui::StrokeKind::Inside,
    );
    // 시안 `box-shadow: 0 0 0 1px border-focus` — 테두리 바깥의 1px 링.
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, focus),
        egui::StrokeKind::Outside,
    );
    let pad_x = theme.multiselect_padding_x().value();
    let text_w = (width - pad_x - theme.multiselect_chevron_room().value()).max(0.0);
    let mut job = egui::text::LayoutJob::simple_singleline(
        summary.to_owned(),
        egui::FontId::proportional(theme.multiselect_font_size().value()),
        theme.multiselect_summary_fg().to_egui(),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
    let galley = ui.fonts(|f| f.layout_job(job));
    let pos = egui::pos2(
        rect.left() + pad_x,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter()
        .galley(pos, galley, theme.multiselect_summary_fg().to_egui());
    chevron(ui, theme, rect, CHEVRON_UP);
}

/// chevron 글리프 — 오른쪽 chevron-offset 안쪽, icon-size-sm.
fn chevron(ui: &mut egui::Ui, theme: &Theme, trigger: egui::Rect, glyph: MockGlyph) {
    let size = theme.icon_glyph_size_sm.value();
    let rect = egui::Rect::from_min_size(
        egui::pos2(
            trigger.right() - theme.multiselect_chevron_offset().value() - size,
            trigger.center().y - size * 0.5,
        ),
        egui::vec2(size, size),
    );
    glyph
        .image(size, theme.multiselect_chevron_fg().to_egui())
        .paint_at(ui, rect);
}

/// 메뉴 — min-width = 트리거, 내용만큼 320 까지 늘어난다. 행은 28 높이 · 12 좌우 여백.
fn static_menu(ui: &mut egui::Ui, theme: &Theme, trigger_w: f32, m: &MenuSpec<'_>) {
    let pad = theme.multiselect_menu_padding().value();
    let bw = theme.border_width.value();
    let row_pad = theme.multiselect_row_padding_x().value();
    let chrome = (pad + bw) * 2.0;
    let widest = m
        .options
        .iter()
        .map(|o| checkbox_width(ui, theme, o))
        .fold(0.0_f32, f32::max);
    let menu_w = (widest + row_pad * 2.0 + chrome).clamp(
        trigger_w,
        theme.multiselect_menu_max_width().value().max(trigger_w),
    );
    egui::Frame::new()
        .fill(theme.multiselect_menu_bg().to_egui())
        .stroke(egui::Stroke::new(
            bw,
            theme.multiselect_menu_border().to_egui(),
        ))
        .corner_radius(theme.multiselect_menu_radius().value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin::same(pad as i8))
        .show(ui, |ui| {
            let inner_w = menu_w - chrome;
            ui.set_width(inner_w);
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(label) = m.all_toggle {
                all_row(ui, theme, inner_w, label);
                let gap = theme.spacing_xs.value();
                let (rect, _) = ui
                    .allocate_exact_size(egui::vec2(inner_w, gap * 2.0 + bw), egui::Sense::hover());
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(rect.left(), rect.top() + gap),
                        egui::vec2(inner_w, bw),
                    ),
                    0.0,
                    theme.multiselect_separator().to_egui_premultiplied(),
                );
            }
            egui::ScrollArea::vertical()
                .id_salt(ui.next_auto_id().with("g_ms_static_list"))
                .max_height(theme.multiselect_menu_max_height().value())
                .auto_shrink([false, true])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, opt) in m.options.iter().enumerate() {
                        let bg = if m.active == Some(i) {
                            Some(theme.multiselect_row_bg_active().to_egui())
                        } else if m.hover == Some(i) {
                            Some(theme.multiselect_row_bg_hover().to_egui_premultiplied())
                        } else {
                            None
                        };
                        let checked = m.selected.get(i).copied().unwrap_or(false);
                        let disabled = m.disabled.get(i).copied().unwrap_or(false);
                        option_row(ui, theme, inner_w, opt, checked, !disabled, bg);
                    }
                });
        });
}

fn option_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    w: f32,
    label: &str,
    checked: bool,
    enabled: bool,
    bg: Option<egui::Color32>,
) {
    let h = theme.multiselect_row_height().value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    if let Some(c) = bg {
        ui.painter()
            .rect_filled(rect, theme.menu_item_radius().value(), c);
    }
    let inner = rect.shrink2(egui::vec2(theme.multiselect_row_padding_x().value(), 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            // 정적 칸이라 토글 결과는 버린다 — 상태는 칸마다 고정이다.
            let mut v = checked;
            checkbox(
                ui,
                theme,
                &mut v,
                label,
                enabled,
                tasty_ui_widgets::Ground::Panel,
            );
        },
    );
}

/// 일괄 토글 행 — accent 글자, 옵션 행과 같은 28 높이 · 12 좌우 여백.
fn all_row(ui: &mut egui::Ui, theme: &Theme, w: f32, label: &str) {
    let h = theme.multiselect_row_height().value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(theme.multiselect_font_size().value()),
            theme.multiselect_all_fg().to_egui(),
        )
    });
    let pos = egui::pos2(
        rect.left() + theme.multiselect_row_padding_x().value(),
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter()
        .galley(pos, galley, theme.multiselect_all_fg().to_egui());
}

/// 폼 짝을 그리드 폭 가운데에 둔다(무대 `center`). 직전 프레임에 잰 폭으로 왼쪽 여백을 정한다.
fn centered_pair(ui: &mut egui::Ui, theme: &Theme, cell_w: f32, st: &mut MsState) {
    let row_w = cell_w * 3.0 + GRID_GAP.value() * 2.0;
    let id = ui.id().with("g_ms_pair_w");
    let prev: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(row_w);
    ui.horizontal(|ui| {
        ui.add_space(((row_w - prev) * 0.5).max(0.0));
        let w = form_pair(ui, theme, st);
        ui.data_mut(|d| d.insert_temp(id, w));
    });
}

/// 시안 폼 짝 — 한 값은 Select, 여러 값은 MultiSelect. 두 트리거가 한 가족으로 읽힌다.
fn form_pair(ui: &mut egui::Ui, theme: &Theme, st: &mut MsState) -> f32 {
    let w = theme.field_width_md.value();
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_lg.value() as i8,
            PAIR_PAD_Y.value() as i8,
        ))
        .show(ui, |ui| {
            // 시안 `alignItems: "flex-end"` — 두 칸을 아래쪽에 맞춘다.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Max), |ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
                pair_field(ui, theme, "Sort by — Select (one value)", |ui| {
                    select(
                        ui,
                        theme,
                        "g_ms_pair_sort",
                        &mut st.pair_sort,
                        &["Started", "Name", "Duration"],
                        w,
                        true,
                    );
                });
                pair_field(ui, theme, "Status — MultiSelect (many)", |ui| {
                    multi_select(
                        ui,
                        theme,
                        "g_ms_pair_status",
                        &mut st.pair_status,
                        &STATES,
                        None,
                        &LABELS,
                        None,
                        w,
                        true,
                    );
                });
            });
        })
        .response
        .rect
        .width()
}

fn pair_field(ui: &mut egui::Ui, theme: &Theme, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = PAIR_LABEL_GAP.value();
        ui.label(
            egui::RichText::new(label)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        add(ui);
    });
}

/// 정적 칸(긴 라벨 · 행 비활성 · 20 옵션 + 일괄 토글)을 공용 위젯으로 직접 열어 보는 회귀 예제.
/// 시안에는 없는 갤러리 전용 줄이다. 마지막 칸은 일괄 토글이 비활성 행을 바꾸지 않는지 본다.
fn live_regressions(ui: &mut egui::Ui, theme: &Theme, st: &mut MsState) {
    let w = CELL_W.value();
    let mut cancelled = [false; 6];
    cancelled[5] = true;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(GRID_GAP.value(), GRID_GAP.value());
        cluster(ui, theme, "live widget — long labels", |ui| {
            let labels = MultiSelectLabels {
                none: "None",
                some: "{} pipelines selected — very long summary",
                some_one: "{} pipelines selected — very long summary",
                all: "All",
            };
            multi_select(
                ui,
                theme,
                "g_ms_live_long",
                &mut st.live_long,
                &LONG,
                None,
                &labels,
                None,
                w,
                true,
            );
        });
        cluster(ui, theme, "live widget — row 6 disabled", |ui| {
            multi_select(
                ui,
                theme,
                "g_ms_live_rows",
                &mut st.live_rows,
                &STATES,
                Some(&cancelled),
                &LABELS,
                None,
                w,
                true,
            );
        });
        cluster(ui, theme, "live widget — 20 options + allToggle", |ui| {
            multi_select(
                ui,
                theme,
                "g_ms_live_many",
                &mut st.live_many,
                &MANY,
                None,
                &LABELS,
                Some(MultiSelectAllToggle {
                    select_all: "Select all",
                    clear_all: "Clear all",
                }),
                w,
                true,
            );
        });
        cluster(
            ui,
            theme,
            "live widget — allToggle + rows 1–2 disabled",
            |ui| {
                multi_select(
                    ui,
                    theme,
                    "g_ms_live_masked",
                    &mut st.live_masked,
                    &STATES,
                    Some(&[true, true, false, false, false, false]),
                    &LABELS,
                    Some(MultiSelectAllToggle {
                        select_all: "Select all",
                        clear_all: "Clear all",
                    }),
                    w,
                    true,
                );
            },
        );
    });
}
