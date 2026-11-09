//! 탐색기 surface 한 장을 조립한 예제 — 내부 탭 줄, 도구 모음, 196px 사이드바, 상세 보기.
//! 부품은 explorer 의 다른 예제(탭 줄 · 경로 필드 · 보기 전환 · 사이드바 · 상세 표)를 그대로 쓴다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    ControlSize, IconButton, IconButtonVariant, PathField, RowParent, surface_bed,
};

use super::explorer_sidebar::{FAVS_FEW, SIDEBAR_W, TREE_SHORT, two_region};
use super::explorer_tab_bar::strip;
use super::explorer_toolbar::{seg_toggle, seg_toggle_width};
use super::explorer_view_cells::{DetailRow, Kind, detail_table};
use super::glyph;
use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// 시안 `ExplorerFrame` 의 최대 폭과 높이(`maxWidth: 760` · `height: 420`). 전시 치수다.
const FRAME_W: LogicalPx = LogicalPx(760.0);
const FRAME_H: LogicalPx = LogicalPx(420.0);

/// 시안 `ExpToolbar` 의 탐색 버튼 묶음 간격(`gap: 1`). 대응 토큰이 없는 리터럴이다.
const NAV_GAP: LogicalPx = LogicalPx(1.0);

/// 시안 `ExpInternalTabs` 의 탭 두 개.
const TABS: &[(&str, bool)] = &[("Downloads", true), ("src", false)];

/// 시안 `ExpDetail` 의 다섯 행. 선택은 diagram.png, 잘라내기 대기는 archive.zip 이다.
const ROWS: &[DetailRow] = &[
    DetailRow {
        kind: Kind::Folder,
        name: "mockup-exports",
        size: "—",
        modified: "2026-06-20 14:30",
        kind_label: "Folder",
        cut: false,
    },
    DetailRow {
        kind: Kind::File,
        name: "report.pdf",
        size: "2.4 MB",
        modified: "2026-06-24 09:12",
        kind_label: "PDF document",
        cut: false,
    },
    DetailRow {
        kind: Kind::Image,
        name: "diagram.png",
        size: "488 KB",
        modified: "2026-06-26 18:05",
        kind_label: "PNG image",
        cut: false,
    },
    DetailRow {
        kind: Kind::File,
        name: "notes.md",
        size: "12 KB",
        modified: "2026-06-27 11:40",
        kind_label: "Markdown",
        cut: false,
    },
    DetailRow {
        kind: Kind::File,
        name: "archive.zip",
        size: "64 MB",
        modified: "2026-06-18 22:01",
        kind_label: "Archive",
        cut: true,
    },
];
thread_local! {
    /// 상세 보기 선택 행. 처음은 시안처럼 diagram.png 다.
    static SELECTED: RefCell<usize> = const { RefCell::new(2) };
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
        for (focused, name) in [(true, "focused"), (false, "unfocused")] {
            spec::cluster(ui, theme, name, |ui| {
                ui.push_id(name, |ui| surface(ui, theme, focused));
            });
        }
    });

    let min_height = format!(
        "{} explorer-min-height — the split drag stops here",
        theme.explorer_min_height().value()
    );
    spec::meta(
        ui,
        theme,
        &[
            (
                "focus",
                "the file area takes the bed; tabs, toolbar, sidebar, header and status line stay",
            ),
            ("surface", "fills a work-area tile"),
            ("internal tabs", "24px · per-cwd, with × + ＋"),
            ("toolbar", "44px · nav · path field · view toggle"),
            ("sidebar", "196px — Files tree + Favorites"),
            ("splitter", "1px separator, drag to resize"),
            (
                "row height",
                "28 (Detail) · table-cell-height — the shared Table",
            ),
            (
                "detail header",
                "shared Table header — UI font · table-header-font-size 11 · weight medium · caps",
            ),
            ("min height", &min_height),
            ("selected row", "surface-active"),
        ],
        &[
            TokenChip::new(
                "bg-panel",
                "toolbar + active tab",
                theme.bg_panel().to_egui(),
            ),
            TokenChip::new(
                "surface-bed-focused",
                "file area, focused",
                theme.surface_bed_focused().to_egui(),
            ),
            TokenChip::new(
                "surface-bed-unfocused",
                "file area, unfocused",
                theme.surface_bed_unfocused().to_egui(),
            ),
            TokenChip::new(
                "bg-sidebar",
                "tabs + sidebar + header",
                theme.bg_sidebar().to_egui(),
            ),
            TokenChip::new(
                "surface-raised",
                "view toggle container",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "segtoggle-on-bg",
                "view toggle — selected segment",
                theme.segtoggle_on_bg().to_egui(),
            ),
            TokenChip::new("input-bg", "path field", theme.input_bg().to_egui()),
            TokenChip::new(
                "surface-active",
                "selected row (rows only)",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "accent-warning",
                "favorite star",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "active tab bar",
                theme.accent_primary().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Defaults chosen (briefs left these open): Detail is the default view; the right-hand \
         preview panel is dropped in favour of a wider content area (re-add later as a toggle \
         if needed); no in-toolbar filter search in this pass; the tree is rooted at Home. \
         Favorites are global across surfaces.",
    );

    spec::note(
        ui,
        theme,
        "Tab strip follows the body explorer tab bar: 24px, 13px label, padding 8/8, gap 4, \
         12px folder glyph, no max width, × on the active tab only (others on hover), a \
         separator only before inactive tabs, a border-strong bottom line and a square ＋ at \
         strip height. The kit draws 28px, 12px label, padding 0 8 0 10, gap 6, a 16px glyph, \
         140 max width with ellipsis, × on every tab at 0.6 opacity, a separator after every \
         tab, a separator bottom line and an IconButton sm ＋ with 4px side padding.",
    );

    spec::note(
        ui,
        theme,
        "Toolbar and rows follow the body: Up is the body chevronUp (the kit draws an up \
         arrow). The view toggle (pad 4 · gap 4 · 24×20 cells), the 28px detail rows and the \
         shared Table header font match the kit. Still drawn with the body values: 12px cell \
         padding (kit 0 10), Type at 11px (kit 12), cut dims the ink only (kit dims the whole \
         row to 0.5), and hover shows only under the pointer (the kit pins notes.md in hover).",
    );
}

/// 760×420 surface 한 장. 위에서부터 내부 탭 · 도구 모음 · 사이드바와 상세 보기.
fn surface(ui: &mut egui::Ui, theme: &Theme, focused: bool) {
    let salt = if focused { "focused" } else { "unfocused" };
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            ui.set_width(FRAME_W.value());
            ui.set_height(FRAME_H.value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                strip(ui, theme, FRAME_W.value(), TABS);
                toolbar(ui, theme, salt);
                let body_h = LogicalPx(ui.available_height().max(0.0));
                body(ui, theme, body_h, focused, salt);
            });
        });
}

/// 44px 도구 모음 — 탐색 버튼 넷, 남은 폭을 채우는 경로 필드, 보기 전환.
fn toolbar(ui: &mut egui::Ui, theme: &Theme, salt: &str) {
    let pad = theme.spacing_sm.value();
    let h = theme.item_height_interactive.value() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(FRAME_W.value(), h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - theme.border_width.value() * 0.5,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad, rect.top()),
        egui::pos2(rect.right() - pad, rect.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = NAV_GAP.value();
    for (g, enabled) in [
        (icons::CHEVRON_LEFT, true),
        (icons::CHEVRON_RIGHT, false),
        (icons::CHEVRON_UP, true),
        (icons::REFRESH, true),
    ] {
        nav_button(&mut child, theme, g, enabled);
    }
    child.add_space(pad - NAV_GAP.value());
    child.spacing_mut().item_spacing.x = pad;
    let addr_w = (child.available_width() - seg_toggle_width(theme) - pad).max(0.0);
    child.allocate_ui_with_layout(
        egui::vec2(addr_w, child.available_height()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let id = format!("gallery_exp_surface_addr_{salt}");
            path_field(ui, theme, &id, "~/Downloads");
        },
    );
    seg_toggle(&mut child, theme, 2, None);
}

pub(super) fn nav_button(ui: &mut egui::Ui, theme: &Theme, g: MockGlyph, enabled: bool) {
    IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .enabled(enabled)
        .show(ui, theme, &|ui, rect, c| {
            g.image(rect.height(), c).paint_at(ui, rect)
        });
}

/// 편집하지 않는 경로 필드. 매 프레임 같은 값으로 그리므로 입력은 남지 않는다.
pub(super) fn path_field(ui: &mut egui::Ui, theme: &Theme, id: &str, path: &str) {
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
    let mut buf = path.to_string();
    let mut editing = false;
    let mut active = None;
    PathField::new(id)
        .placeholder(t("explorer.address.placeholder"))
        .empty_label(t("explorer.address.empty"))
        .leading_icon(&folder_icon)
        .row_icon(&folder_icon)
        .go_icon(&go_icon)
        .show(ui, theme, &mut buf, &mut editing, &mut active, &[], path);
}

/// 196px 사이드바와 상세 보기. 둘 사이는 사이드바 오른쪽 1px 구분선이다.
fn body(ui: &mut egui::Ui, theme: &Theme, body_h: LogicalPx, focused: bool, salt: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        let (side, _) = ui.allocate_exact_size(
            egui::vec2(SIDEBAR_W.value(), body_h.value()),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(side, 0.0, theme.bg_sidebar().to_egui());
        ui.painter().vline(
            side.right() - theme.border_width.value() * 0.5,
            side.y_range(),
            egui::Stroke::new(
                theme.border_width.value(),
                theme.separator.to_egui_premultiplied(),
            ),
        );
        let mut side_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(side)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        side_ui.spacing_mut().item_spacing.y = 0.0;
        two_region(
            &mut side_ui,
            theme,
            &format!("explorer_surface_{salt}"),
            body_h,
            TREE_SHORT,
            FAVS_FEW,
        );
        ui.vertical(|ui| {
            let w = ui.available_width();
            ui.set_width(w);
            let area =
                egui::Rect::from_min_size(ui.next_widget_position(), egui::vec2(w, body_h.value()));
            ui.painter()
                .rect_filled(area, 0.0, surface_bed(theme, focused));
            let id = format!("explorer_surface_detail_{salt}");
            SELECTED.with(|s| {
                let mut sel = s.borrow_mut();
                let parent = RowParent::bed(focused);
                if let Some(i) = detail_table(ui, theme, ROWS, *sel, body_h, &id, parent) {
                    *sel = i;
                }
            });
        });
    });
}
