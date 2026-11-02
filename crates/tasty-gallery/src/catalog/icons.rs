//! tasty-icons의 아이콘을 용도별로 보여준다. 도형은 공용 크레이트에서 가져오고 색은 Theme로 지정한다.

use std::cell::RefCell;
use tasty_type_geometry::length::LogicalPx;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{IconButton, Input};

use crate::catalog::spec::{StageVariant, TokenChip, cluster, dont, meta, note, stage};

/// 예제에서 사용할 공용 Icon 타입의 별칭.
pub use tasty_icons::Icon as MockGlyph;
pub use tasty_icons::*;

/// 한 글리프 카탈로그 항목: (글리프, canonical name, role).
type Entry = (MockGlyph, &'static str, &'static str);

const ACTIONS: &[Entry] = &[
    (PLUS, "plus", "create / add"),
    (CLOSE, "close", "dismiss overlay (header X)"),
    (REFRESH, "refresh", "re-scan / reload"),
    (EDIT, "edit", "edit a row / value"),
    (TRASH, "trash", "delete / remove"),
    (COPY, "copy", "copy to clipboard"),
    (SEARCH, "search", "filter / search affordance"),
    (FUNNEL, "funnel", "state filter"),
    (SWAP, "swap", "swap / switch direction"),
    (DOWNLOAD, "download", "download / save to disk"),
    (UNDO, "undo", "undo an edit (image paint)"),
    (REDO, "redo", "redo an edit (image paint)"),
    (MORE, "more", "more actions (banner ⋯ trigger)"),
];

const NAV: &[Entry] = &[
    (CHEVRON_RIGHT, "chevronRight", "collapsed row · forward"),
    (CHEVRON_DOWN, "chevronDown", "expanded row"),
    (CHEVRON_LEFT, "chevronLeft", "back"),
    (CHEVRON_UP, "chevronUp", "go to parent dir"),
    (CHEVRONS_LEFT, "chevronsLeft", "collapse sidebar"),
    (CHEVRONS_RIGHT, "chevronsRight", "expand sidebar rail"),
    (ARROW_RIGHT, "arrowRight", "go / submit (markdown address)"),
    (MOVE, "move", "move / reposition (4-way)"),
    (FIT, "fit", "fullscreen stage (popup title bar)"),
];

const VIEW: &[Entry] = &[
    (LAYOUT_GRID, "layoutGrid", "grid / icon view"),
    (LIST, "listView", "list view"),
    (LAYOUT_DETAIL, "layoutDetail", "detail / table view"),
    (COLUMNS, "columns", "column split layout"),
    (STAR, "star", "favorite / bookmark"),
    (STAR_FILL, "starFill", "favorite / active (filled)"),
];

const SURFACES: &[Entry] = &[
    (TERMINAL, "terminal", "terminal surface / tab"),
    (
        AGENT,
        "agent",
        "agent turn — DAG agent task (Claude / Codex)",
    ),
    (MARKDOWN, "markdown", "markdown surface / tab"),
    (IMAGE, "image", "image surface / fallback"),
    (HTML, "html", "html surface / web view"),
    (SPLIT, "split", "split a pane"),
    (SPLIT_H, "splitH", "split a pane (horizontal divider)"),
    (PANE_EMPTY, "paneEmpty", "empty pane / no surface"),
    (FOLDER, "folder", "folder / workspace"),
    (FOLDER_OPEN, "folderOpen", "folder / workspace (open state)"),
    (FILE, "file", "file leaf"),
    (
        FOLDER_PLUS,
        "folderPlus",
        "new folder (explorer toolbar / menu)",
    ),
    (FILE_PLUS, "filePlus", "new file (explorer toolbar / menu)"),
    (INFO, "info", "properties / information"),
    (LINK, "link", "symbolic link (explorer Properties / rows)"),
    (REMOTE, "remote", "remote connection"),
    (PORT, "port", "listening port / target"),
    (LAYERS, "layers", "layout presets / stacked layers"),
    (CLIPBOARD, "clipboard", "clipboard viewer"),
    (TEXT_LEFT, "textLeft", "text content / paragraph"),
    (GIT_BRANCH, "gitBranch", "git branch"),
    (GIT_TREE, "gitTree", "git tree / lineage"),
];

const VISIBILITY: &[Entry] = &[
    (EYE, "eye", "reveal value"),
    (EYE_OFF, "eyeOff", "hide value"),
    (LOCK, "lock", "locked / secret held back"),
    (GLOBE, "globe", "remote content allowed (network)"),
    (GLOBE_OFF, "globeOff", "remote content blocked (network)"),
];

const STATUS: &[Entry] = &[
    (ALERT_TRIANGLE, "alertTriangle", "warning / unverified"),
    (ALERT_CIRCLE, "alertCircle", "error / failed"),
    (SHIELD_CHECK, "shieldCheck", "trusted / signed"),
    (BELL, "bell", "notification"),
    (HELP_CIRCLE, "helpCircle", "inline help hint (?)"),
];

const SYSTEM: &[Entry] = &[
    (TOOLS, "tools", "Tools menu"),
    (SETTINGS, "settings", "Settings window"),
    (PLUG, "plug", "Plugins"),
    (ROCKET, "rocket", "getting started / launch"),
    (THEME, "theme", "theme toggle (Mocha / Latte)"),
    (SUN, "sun", "empty state / no settings"),
    (HASH, "hash", "number / tab-switch digits"),
];

const KEYS: &[Entry] = &[
    (CMD_KEY, "cmdKey", "Command key symbol (⌘)"),
    (OPTION_KEY, "optionKey", "Option key symbol (⌥)"),
    (SHIFT_KEY, "shiftKey", "Shift key symbol (⇧)"),
];

// 대응 Theme 토큰이 없는 아이콘 카탈로그 전용 치수.
/// `.icongrid` auto-fill `minmax(132px, 1fr)` 의 셀 폭.
const TILE_W: LogicalPx = LogicalPx(132.0);
/// `.icontile` 높이 — padding 18/13 + glyph-box 36 + name/role.
const TILE_H: LogicalPx = LogicalPx(110.0);
/// `.icontile .glyph` 박스 36×36 안에 그리는 글리프 — jsx `<GIcon size={22}>`.
const TILE_GLYPH: LogicalPx = LogicalPx(22.0);
/// `.glyph` 박스 한 변 — 글리프 수직 중심 산출용.
const GLYPH_BOX: LogicalPx = LogicalPx(36.0);
/// 타일 위쪽 패딩에서 glyph 박스를 아래로 미는 광학 보정. 선 굵기와 무관하다.
const TILE_GLYPH_NUDGE_Y: LogicalPx = LogicalPx(2.0);

thread_local! {
    /// system-rules 데모의 filter Input 버퍼 (egui memory 에 포커스 유지).
    static FILTER_BUF: RefCell<String> = const { RefCell::new(String::new()) };
}

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

/// 아이콘 시스템 규칙 — size 스케일 / currentColor / IconButton 데모 + meta.
pub fn draw_system_rules(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        cluster(ui, theme, "size scale — set via size prop", |ui| {
            // 크기 비교가 목적이므로 각 크기를 직접 지정한다.
            for s in [26.0_f32, 20.0, 16.0, 14.0, 12.0] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                    paint_glyph(ui, PORT, rect.center(), s, ec(theme.text_secondary()));
                    ui.label(
                        egui::RichText::new(format!("{s:.0}px"))
                            .monospace()
                            .size(theme.font_size_micro.value())
                            .color(ec(theme.text_muted())),
                    );
                });
            }
        });
        cluster(ui, theme, "inherits currentColor", |ui| {
            let size = theme.icon_glyph_size_md.value() + theme.spacing_xs.value(); // 16 + 4 = 20
            let tints: [(MockGlyph, egui::Color32); 4] = [
                (REFRESH, ec(theme.text_muted())),
                (ALERT_TRIANGLE, ec(theme.accent_warning())),
                (SHIELD_CHECK, ec(theme.accent_success())),
                (ALERT_CIRCLE, ec(theme.accent_danger())),
            ];
            for (g, color) in tints {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                paint_glyph(ui, g, rect.center(), size, color);
            }
        });
        cluster(ui, theme, "in an IconButton — the common case", |ui| {
            IconButton::new().show(ui, theme, &|ui, rect, c| {
                REFRESH.image(rect.height(), c).paint_at(ui, rect)
            });
            IconButton::new().show(ui, theme, &|ui, rect, c| {
                CLOSE.image(rect.height(), c).paint_at(ui, rect)
            });
            FILTER_BUF.with(|b| {
                let mut buf = b.borrow_mut();
                Input::new()
                    .placeholder("Filter…")
                    .width(theme.field_width_md.value())
                    .icon(&|ui, rect, c| SEARCH.image(rect.height(), c).paint_at(ui, rect))
                    .show(ui, theme, &mut buf);
            });
        });
    });

    meta(
        ui,
        theme,
        &[
            ("viewBox", "0 0 24 24"),
            ("stroke", "2px · round cap + join"),
            ("fill", "none — stroke only"),
            ("color", "inherits currentColor"),
            ("sizes", "26 / 20 / 16 (default) / 14 / 12"),
            ("in IconButton", "28px square control-height"),
        ],
        &[
            TokenChip::new("text-muted", "rest glyph", ec(theme.text_muted())),
            TokenChip::new("accent-warning", "warn tint", ec(theme.accent_warning())),
            TokenChip::new("accent-success", "ok tint", ec(theme.accent_success())),
            TokenChip::new("accent-danger", "error tint", ec(theme.accent_danger())),
        ],
    );
    note(
        ui,
        theme,
        "This page is the display window for these glyphs; their geometry lives in the \
         tasty-icons crate. A new overlay's header X, refresh, and filter-search all pull \
         close / refresh / search from there — never re-inline a <path>.",
    );
    dont(
        ui,
        theme,
        "Don't bake a color into a glyph, mix stroke widths, or add a fill. A filled \
         state indicator is StatusDot / Badge, not an icon.",
    );
}

pub fn draw_actions(ui: &mut egui::Ui, theme: &Theme) {
    icongrid(ui, theme, ACTIONS);
}
pub fn draw_nav(ui: &mut egui::Ui, theme: &Theme) {
    icongrid(ui, theme, NAV);
}
pub fn draw_view(ui: &mut egui::Ui, theme: &Theme) {
    icongrid(ui, theme, VIEW);
}
pub fn draw_surfaces(ui: &mut egui::Ui, theme: &Theme) {
    icongrid(ui, theme, SURFACES);
}
pub fn draw_visibility(ui: &mut egui::Ui, theme: &Theme) {
    icongrid(ui, theme, VISIBILITY);
}
pub fn draw_status(ui: &mut egui::Ui, theme: &Theme) {
    icongrid(ui, theme, STATUS);
}
pub fn draw_system(ui: &mut egui::Ui, theme: &Theme) {
    icongrid(ui, theme, SYSTEM);
}
pub fn draw_keys(ui: &mut egui::Ui, theme: &Theme) {
    icongrid(ui, theme, KEYS);
}

/// 글리프 타일 그리드 — separator 배경 위에 1px gap 으로 panel 셀을 깐다
/// (`.icongrid` gap 1px on `--tasty-separator`, solo stage padding 0).
fn icongrid(ui: &mut egui::Ui, theme: &Theme, icons: &[Entry]) {
    egui::Frame::new()
        .fill(ec(theme.separator.to_egui_premultiplied()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            ec(theme.border_default()),
        ))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing =
                    egui::vec2(theme.border_width.value(), theme.border_width.value());
                for (g, name, role) in icons {
                    tile(ui, theme, *g, name, role);
                }
            });
        });
}

fn tile(ui: &mut egui::Ui, theme: &Theme, g: MockGlyph, name: &str, role: &str) {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(TILE_W.value(), TILE_H.value()),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);

    let bg = if resp.hovered() {
        ec(theme.overlay_hover().to_egui_premultiplied())
    } else {
        ec(theme.bg_panel())
    };
    painter.rect_filled(rect, 0.0, bg);

    let glyph_cy = LogicalPx(rect.top()) + theme.spacing_lg + TILE_GLYPH_NUDGE_Y + GLYPH_BOX / 2.0;
    let glyph_color = if resp.hovered() {
        ec(theme.text_primary())
    } else {
        ec(theme.text_secondary())
    };
    paint_glyph(
        ui,
        g,
        egui::pos2(rect.center().x, glyph_cy.value()),
        TILE_GLYPH.value(),
        glyph_color,
    );

    let name_y = glyph_cy + GLYPH_BOX / 2.0 + theme.spacing_sm;
    painter.text(
        egui::pos2(rect.center().x, name_y.value()),
        egui::Align2::CENTER_TOP,
        name,
        egui::FontId::monospace(theme.font_size_term_sm.value()),
        ec(theme.text_primary()),
    );
    painter.text(
        egui::pos2(rect.center().x, (name_y + theme.spacing_lg).value()),
        egui::Align2::CENTER_TOP,
        role,
        egui::FontId::proportional(theme.font_size_micro.value()),
        ec(theme.text_muted()),
    );

    resp.on_hover_text(role);
}

/// `center` 를 중심으로 한 `size` 정사각에 글리프를 `color` tint 로 그린다.
fn paint_glyph(
    ui: &mut egui::Ui,
    g: MockGlyph,
    center: egui::Pos2,
    size: f32,
    color: egui::Color32,
) {
    let r = egui::Rect::from_center_size(center, egui::vec2(size, size));
    g.image(size, color).paint_at(ui, r);
}
