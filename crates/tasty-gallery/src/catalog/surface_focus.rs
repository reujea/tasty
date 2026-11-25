//! Layouts › Surface focus. 시안 `layouts/surface-focus.jsx`의 두 Spec을 옮긴다.
//! 띠는 본체와 같은 `tasty_ui_widgets::SurfaceFocusEdge`, 바탕은 같은 `surface_bed`로 그린다.

use std::cell::Cell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{SurfaceFocusEdge, surface_bed};

use crate::catalog::components::markdown_viewer::document;
use crate::catalog::spec::{StageVariant, TokenChip, cluster, do_, dont, meta, note, stage};
use crate::catalog::{Section, Spec};

/// 시안 `band`의 높이(`size-20`).
const BAND_H: LogicalPx = LogicalPx(20.0);
/// 시안 `frame`의 높이(`size-64`).
const FRAME_H: LogicalPx = LogicalPx(64.0);
/// 시안 `FocusDemo`의 높이(`size-160`).
const DEMO_H: LogicalPx = LogicalPx(160.0);
/// 시안 `.fixed-view` 한 칸의 전시 폭. 시안은 칸 폭을 그리드가 정한다.
const VIEW_W: LogicalPx = LogicalPx(400.0);
/// 시안 `Marked`는 내용 폭이다. 가장 긴 줄이 들어가는 전시 폭으로 고정한다.
const MARK_W: LogicalPx = LogicalPx(220.0);
/// 시안 dag 노드 표시의 크기(`size-48`×`size-16`).
const NODE_W: LogicalPx = LogicalPx(48.0);
const NODE_H: LogicalPx = LogicalPx(16.0);
/// 시안 markdown 표 띠의 높이(`size-4`).
const MD_STRIP_H: LogicalPx = LogicalPx(4.0);
/// 시안 `bed-field-focus` 입력 칸의 위치와 높이(`size-4`, `size-16`).
const FIELD_INSET: LogicalPx = LogicalPx(4.0);
const FIELD_H: LogicalPx = LogicalPx(16.0);
/// 시안 states 칸의 띠 폭(`size-64`)과 칸 크기.
const EDGE_W: LogicalPx = LogicalPx(64.0);
const EDGE_CELL_W: LogicalPx = LogicalPx(80.0);
const EDGE_CELL_H: LogicalPx = LogicalPx(24.0);
/// 시안 popup-focus `LeafUnderPopup`의 leaf 크기(`size-240`×`size-112`).
const UNDER_LEAF_W: LogicalPx = LogicalPx(240.0);
const UNDER_LEAF_H: LogicalPx = LogicalPx(112.0);
/// 같은 시안의 팝업 위치와 폭(`size-28` 아래, `size-140` 폭).
const UNDER_POPUP_TOP: LogicalPx = LogicalPx(28.0);
const UNDER_POPUP_W: LogicalPx = LogicalPx(140.0);

/// surface 종류 하나의 표본.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Terminal,
    Markdown,
    Empty,
    Explorer,
    DagGraph,
    Image,
    Html,
    PluginWebview,
    PluginMesh,
}

const KINDS: [Kind; 9] = [
    Kind::Terminal,
    Kind::Markdown,
    Kind::Empty,
    Kind::Explorer,
    Kind::DagGraph,
    Kind::Image,
    Kind::Html,
    Kind::PluginWebview,
    Kind::PluginMesh,
];

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Terminal => "terminal",
            Kind::Markdown => "markdown",
            Kind::Empty => "empty",
            Kind::Explorer => "explorer",
            Kind::DagGraph => "dag_graph",
            Kind::Image => "image",
            Kind::Html => "html",
            Kind::PluginWebview => "plugin, webview",
            Kind::PluginMesh => "plugin, egui-mesh",
        }
    }

    /// 바탕을 호스트가 보장할 수 없어 띠를 쓰는 kind.
    fn has_edge(self) -> bool {
        matches!(self, Kind::Html | Kind::PluginWebview | Kind::PluginMesh)
    }
}

/// 링 하나: 색과 폭.
#[derive(Clone, Copy)]
struct Ring(egui::Color32, f32);

fn soft(theme: &Theme) -> Ring {
    Ring(
        theme.surface_occupied_soft_border().to_egui(),
        theme.surface_occupied_border_width().value(),
    )
}

fn hard(theme: &Theme) -> Ring {
    Ring(
        theme.surface_occupied_hard_border().to_egui(),
        theme.surface_occupied_border_width().value(),
    )
}

fn input(theme: &Theme) -> Ring {
    Ring(
        theme.surface_highlight_input_border().to_egui(),
        theme.surface_highlight_input_width().value(),
    )
}

fn done(theme: &Theme) -> Ring {
    Ring(
        theme.surface_highlight_done_border().to_egui(),
        theme.surface_highlight_done_width().value(),
    )
}

fn stroke(ring: Ring) -> egui::Stroke {
    egui::Stroke::new(ring.1, ring.0)
}

fn mono(theme: &Theme) -> egui::FontId {
    egui::FontId::monospace(theme.font_size_caption.value())
}

/// 시안 `band`: 내용 위에 놓이는 고정 높이 띠. 남은 영역을 돌려준다.
fn band(
    p: &egui::Painter,
    theme: &Theme,
    area: egui::Rect,
    bg: egui::Color32,
    text: &str,
) -> egui::Rect {
    let (top, rest) = area.split_top_bottom_at_y(area.top() + BAND_H.value());
    p.rect_filled(top, 0.0, bg);
    p.text(
        egui::pos2(top.left() + theme.spacing_sm.value(), top.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        mono(theme),
        theme.text_secondary().to_egui(),
    );
    rest
}

/// 시안 `fill`: 남은 영역을 칠하고 안쪽 여백 위치에 글자를 쓴다. 글자 없이 쓰면 빈 면이다.
fn fill(
    p: &egui::Painter,
    theme: &Theme,
    area: egui::Rect,
    bg: egui::Color32,
    ink: egui::Color32,
    text: &str,
) -> egui::Pos2 {
    p.rect_filled(area, 0.0, bg);
    let at = area.min + egui::vec2(theme.spacing_sm.value(), theme.spacing_xs.value());
    if !text.is_empty() {
        p.text(at, egui::Align2::LEFT_TOP, text, mono(theme), ink);
    }
    at
}

/// 시안 `KINDS[].body`. `area`는 띠 행 아래의 내용 영역이다.
fn paint_body(
    ui: &egui::Ui,
    p: &egui::Painter,
    theme: &Theme,
    area: egui::Rect,
    kind: Kind,
    focused: bool,
) {
    let bed = surface_bed(theme, focused);
    let secondary = theme.text_secondary().to_egui();
    let muted = theme.text_muted().to_egui();
    let panel = theme.bg_panel().to_egui();
    match kind {
        Kind::Terminal => {
            let term = theme.surface("terminal");
            let ink = if focused {
                term.focused_fg
            } else {
                term.unfocused_fg
            };
            fill(p, theme, area, bed, ink.to_egui(), "$ cargo test");
        }
        Kind::Markdown => {
            let rest = band(
                p,
                theme,
                area,
                theme.md_address_bar_bg().to_egui(),
                "notes.md",
            );
            let at = fill(p, theme, rest, bed, theme.md_doc_fg().to_egui(), "Document");
            let (row, zebra) = document::table_fills(theme, focused);
            let line_h = theme.font_size_caption.value() * 1.4;
            let gap = theme.border_width.value();
            let strip = MD_STRIP_H.value();
            let w = rest.right() - theme.spacing_sm.value() - at.x;
            let first = egui::Rect::from_min_size(
                egui::pos2(at.x, at.y + line_h + gap),
                egui::vec2(w, strip),
            );
            p.rect_filled(first, 0.0, row);
            p.rect_filled(first.translate(egui::vec2(0.0, strip + gap)), 0.0, zebra);
        }
        Kind::Empty => {
            fill(p, theme, area, bed, secondary, "");
        }
        Kind::Explorer => {
            let rest = band(p, theme, area, theme.bg_sidebar().to_egui(), "src");
            fill(p, theme, rest, bed, secondary, "main.rs");
        }
        Kind::DagGraph => {
            let rest = band(p, theme, area, theme.bg_sidebar().to_egui(), "build");
            fill(p, theme, rest, bed, secondary, "");
            p.rect_filled(
                egui::Rect::from_center_size(
                    rest.center(),
                    egui::vec2(NODE_W.value(), NODE_H.value()),
                ),
                theme.corner_radius.value(),
                theme.dag_node_bg().to_egui(),
            );
        }
        Kind::Image => {
            let rest = band(p, theme, area, panel, "photo.png");
            fill(p, theme, rest, bed, secondary, "");
            let size = theme.icon_glyph_size_md.value();
            crate::catalog::icons::IMAGE
                .image(size, secondary)
                .paint_at(
                    ui,
                    egui::Rect::from_center_size(rest.center(), egui::vec2(size, size)),
                );
        }
        Kind::Html => {
            let rest = band(
                p,
                theme,
                area,
                theme.html_address_bar_bg().to_egui(),
                "example.com",
            );
            fill(p, theme, rest, panel, muted, "The page paints itself");
        }
        Kind::PluginWebview => {
            fill(
                p,
                theme,
                area,
                panel,
                muted,
                "The plugin page paints itself",
            );
        }
        Kind::PluginMesh => {
            fill(
                p,
                theme,
                area,
                panel,
                muted,
                "The plugin frame paints itself",
            );
        }
    }
}

/// leaf 하나. 바탕 kind는 내용만, 띠 kind는 띠 행을 남긴 내용과 띠를 그린다.
#[allow(clippy::too_many_arguments)] // reason: 갤러리 표본의 내부 함수로 페인터 문맥과 leaf의 상태값을 함께 받는다.
fn paint_leaf(
    ui: &egui::Ui,
    p: &egui::Painter,
    theme: &Theme,
    leaf: egui::Rect,
    kind: Kind,
    focused: bool,
    bed_focused: bool,
    ring: Option<Ring>,
) {
    let r = theme.corner_radius.value();
    if kind.has_edge() {
        let area = egui::Rect::from_min_max(
            egui::pos2(
                leaf.left(),
                leaf.top() + theme.surface_focus_edge_width().value(),
            ),
            leaf.max,
        );
        paint_body(ui, p, theme, area, kind, bed_focused);
        SurfaceFocusEdge {
            focused,
            ring: ring.map(stroke),
            corner_radius: theme.corner_radius,
        }
        .paint(p, theme, leaf);
    } else {
        paint_body(ui, p, theme, leaf, kind, bed_focused);
        if let Some(ring) = ring {
            p.rect_stroke(leaf, r, stroke(ring), egui::StrokeKind::Inside);
        }
    }
}

fn border_stroke(theme: &Theme) -> egui::Stroke {
    egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui())
}

/// 시안 `frame`. 링은 시안 `SurfaceRing`처럼 테두리 안쪽 leaf에 그려 위쪽 변이 띠 행에 놓인다.
fn frame_rect(
    ui: &mut egui::Ui,
    theme: &Theme,
    size: egui::Vec2,
    kind: Kind,
    focused: bool,
    bed_focused: bool,
    ring: Option<Ring>,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let r = theme.corner_radius.value();
    let leaf = rect.shrink(theme.border_width.value());
    paint_leaf(ui, &p, theme, leaf, kind, focused, bed_focused, ring);
    p.rect_stroke(rect, r, border_stroke(theme), egui::StrokeKind::Inside);
    rect
}

/// 시안 states 계약 `Surface focus edge`: 바탕 넷 위의 rest(비포커스), focus(포커스) 띠.
fn edge_states(ui: &mut egui::Ui, theme: &Theme) {
    let hosts = [
        ("crust", theme.bg_app().to_egui()),
        ("mantle", theme.bg_sidebar().to_egui()),
        ("base", theme.bg_panel().to_egui()),
        ("term", theme.surface_bed_focused().to_egui()),
    ];
    let head = |ui: &mut egui::Ui, text: &str| {
        ui.label(
            egui::RichText::new(text)
                .font(mono(theme))
                .color(theme.text_muted()),
        );
    };
    egui::Grid::new("surface-focus-edge-states")
        .spacing(egui::vec2(
            theme.spacing_xs.value(),
            theme.spacing_xs.value(),
        ))
        .show(ui, |ui| {
            head(ui, "host");
            head(ui, "rest");
            head(ui, "focus");
            ui.end_row();
            for (name, host) in hosts {
                head(ui, name);
                for focused in [false, true] {
                    let (cell, _) = ui.allocate_exact_size(
                        egui::vec2(EDGE_CELL_W.value(), EDGE_CELL_H.value()),
                        egui::Sense::hover(),
                    );
                    let p = ui.painter_at(cell);
                    p.rect_filled(cell, 0.0, host);
                    let edge = egui::Rect::from_center_size(
                        cell.center(),
                        egui::vec2(EDGE_W.value(), theme.surface_focus_edge_width().value()),
                    );
                    SurfaceFocusEdge {
                        focused,
                        ring: None,
                        corner_radius: LogicalPx(0.0),
                    }
                    .paint(&p, theme, edge);
                }
                ui.end_row();
            }
        });
}

/// 시안 `ThemeRow`: 한 테마의 focused, unfocused 두 칸.
fn theme_row(ui: &mut egui::Ui, th: &Theme, label: &str, kind: Kind) {
    egui::Frame::new()
        .fill(th.bg_panel().to_egui())
        .inner_margin(th.spacing_xs.value())
        .show(ui, |ui| {
            ui.set_width(VIEW_W.value());
            ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
            ui.label(
                egui::RichText::new(format!("{label}: focused, unfocused"))
                    .size(th.font_size_caption.value())
                    .color(th.text_secondary().to_egui()),
            );
            ui.horizontal(|ui| {
                let gap = th.spacing_xs.value();
                ui.spacing_mut().item_spacing.x = gap;
                let w = (VIEW_W.value() - gap) * 0.5;
                let size = egui::vec2(w, FRAME_H.value());
                frame_rect(ui, th, size, kind, true, true, None);
                frame_rect(ui, th, size, kind, false, false, None);
            });
        });
}

thread_local! {
    /// 포커스 예제에서 포커스를 가진 칸.
    static DEMO_AT: Cell<usize> = const { Cell::new(0) };
}

/// 시안 `FocusDemo`: 칸을 누르면 포커스가 옮고 바탕이나 띠가 그 프레임에 바뀐다.
fn focus_demo(ui: &mut egui::Ui, theme: &Theme) {
    const LEAVES: [Kind; 3] = [Kind::Terminal, Kind::Markdown, Kind::Html];
    let gap = theme.spacing_sm.value();
    let w = ((ui.available_width() - gap * 2.0) / 3.0).max(0.0);
    let at = DEMO_AT.with(Cell::get);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, kind) in LEAVES.into_iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(w, DEMO_H.value()), egui::Sense::click());
            let p = ui.painter_at(rect);
            let leaf = rect.shrink(theme.border_width.value());
            paint_leaf(ui, &p, theme, leaf, kind, at == i, at == i, None);
            p.rect_stroke(
                rect,
                theme.corner_radius.value(),
                border_stroke(theme),
                egui::StrokeKind::Inside,
            );
            if response.clicked() {
                DEMO_AT.with(|c| c.set(i));
            }
        }
    });
}

fn draw_every_kind(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .inner_margin(theme.spacing_md.value())
            .show(ui, |ui| focus_demo(ui, theme));
    });
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for kind in KINDS {
            cluster(ui, theme, kind.label(), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    theme_row(ui, &mocha, "Mocha", kind);
                    theme_row(ui, &latte, "Latte", kind);
                });
            });
        }
    });
    stage(ui, theme, StageVariant::Wrap, |ui| {
        cluster(ui, theme, "States: surface focus edge", |ui| {
            edge_states(ui, theme)
        });
    });
    meta(
        ui,
        theme,
        &[
            ("focused bed", "Mocha #000000, Latte #ffffff"),
            ("unfocused bed", "Mocha base #1e1e2e, Latte crust #dce0e8"),
            (
                "edge position",
                "html and plugin surfaces: top of the leaf, full width, above any address band",
            ),
            (
                "edge height",
                "surface-focus-edge-width, 2px in both states, zoom-exempt",
            ),
            ("motion", "0ms"),
            ("unknown focus", "unfocused"),
            (
                "focused means",
                "host key focus, not pane or tab selection · a popup that holds key focus leaves every surface unfocused (Overlays › Dialogs, popup focus)",
            ),
        ],
        &[
            TokenChip::new(
                "surface-bed-focused",
                "focused bed",
                theme.surface_bed_focused().to_egui(),
            ),
            TokenChip::new(
                "surface-bed-unfocused",
                "unfocused bed",
                theme.surface_bed_unfocused().to_egui(),
            ),
            TokenChip::new(
                "surface-focus-edge-focused",
                "edge, focused",
                theme.surface_focus_edge_focused().to_egui(),
            ),
            TokenChip::new(
                "surface-focus-edge-unfocused",
                "edge, unfocused",
                theme.surface_focus_edge_unfocused().to_egui(),
            ),
        ],
    );
    note(
        ui,
        theme,
        "In the states grid, rest is an unfocused edge and focus a focused one. A plugin can follow the two beds itself, and the edge still shows, because the host cannot tell whether it did.",
    );
    do_(
        ui,
        theme,
        "Do reserve the edge row on an html or plugin leaf at all times. A webview starts below it, so the host can always paint it.",
    );
    dont(
        ui,
        theme,
        "Don't make the edge blue. Blue is the keyboard focus ring and the completion highlight, and a surface can show those at the same time.",
    );
}

/// 시안 `Marked`. 링과 함께 그린 leaf 하나.
struct Marked {
    label: &'static str,
    kind: Kind,
    focused: bool,
    bed_focused: bool,
    ring: Option<Ring>,
    /// 시안 `bed-field-focus`의 키보드 포커스 칸.
    field: bool,
}

fn marked(ui: &mut egui::Ui, theme: &Theme, m: &Marked) {
    cluster(ui, theme, m.label, |ui| {
        let size = egui::vec2(MARK_W.value(), FRAME_H.value());
        let frame = frame_rect(ui, theme, size, m.kind, m.focused, m.bed_focused, m.ring);
        if m.field {
            let inset = FIELD_INSET.value();
            let field = egui::Rect::from_min_size(
                frame.min + egui::vec2(inset, inset),
                egui::vec2(MARK_W.value() - inset * 2.0, FIELD_H.value()),
            );
            ui.painter().rect_stroke(
                field,
                theme.corner_radius_sm.value(),
                egui::Stroke::new(
                    theme.focus_ring_width.value(),
                    theme.input_border_focus().to_egui(),
                ),
                egui::StrokeKind::Outside,
            );
        }
    });
}

/// 시안 `WebviewMarked`: 페이지가 링을 가려 링의 위쪽 변만 띠 행에 보인다.
fn webview_marked(ui: &mut egui::Ui, theme: &Theme, label: &str, focused: bool, ring: Ring) {
    cluster(ui, theme, label, |ui| {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(MARK_W.value(), FRAME_H.value()),
            egui::Sense::hover(),
        );
        let p = ui.painter_at(rect);
        let r = theme.corner_radius.value();
        let leaf = rect.shrink(theme.border_width.value());
        let edge_h = theme.surface_focus_edge_width().value();
        let (row, page) = leaf.split_top_bottom_at_y(leaf.top() + edge_h);
        paint_body(ui, &p, theme, page, Kind::PluginWebview, focused);
        SurfaceFocusEdge {
            focused,
            ring: Some(stroke(ring)),
            corner_radius: LogicalPx(0.0),
        }
        .paint(&p.with_clip_rect(row.intersect(p.clip_rect())), theme, leaf);
        p.rect_stroke(rect, r, border_stroke(theme), egui::StrokeKind::Inside);
    });
}

/// 시안 popup-focus `LeafUnderPopup`(terminal, completed). 팝업이 키 포커스를 가진 동안 leaf는 unfocused 바탕에
/// 완료 링을 남기고 PTY 커서를 그리지 않는다. 키 포커스가 leaf로 돌아오면 링이 풀리고 focused 바탕이 된다.
fn leaf_under_popup(ui: &mut egui::Ui, theme: &Theme, popup_has_keys: bool) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(UNDER_LEAF_W.value(), UNDER_LEAF_H.value()),
        egui::Sense::hover(),
    );
    let p = ui.painter_at(rect);
    let r = theme.corner_radius.value();
    let leaf = rect.shrink(theme.border_width.value());
    let ring = popup_has_keys.then(|| done(theme));
    paint_leaf(
        ui,
        &p,
        theme,
        leaf,
        Kind::Terminal,
        !popup_has_keys,
        !popup_has_keys,
        ring,
    );
    p.rect_stroke(rect, r, border_stroke(theme), egui::StrokeKind::Inside);
    let margin = theme.popup_content_margin().value();
    let caption = theme.font_size_caption.value();
    let gap = theme.spacing_xs.value();
    let popup = egui::Rect::from_min_size(
        egui::pos2(
            rect.right() - theme.spacing_sm.value() - UNDER_POPUP_W.value(),
            rect.top() + UNDER_POPUP_TOP.value(),
        ),
        egui::vec2(
            UNDER_POPUP_W.value(),
            margin * 2.0 + caption * 2.0 * 1.4 + gap,
        ),
    );
    p.add(theme.shadow_popover().to_egui().as_shape(popup, r));
    p.rect_filled(popup, r, theme.bg_panel().to_egui());
    p.rect_stroke(
        popup,
        r,
        egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
        egui::StrokeKind::Inside,
    );
    if popup_has_keys {
        tasty_ui_widgets::paint_popup_focus_edge(&p, theme, popup);
    }
    let at = popup.min + egui::vec2(margin, margin);
    p.text(
        at,
        egui::Align2::LEFT_TOP,
        "Plugin popover",
        egui::FontId::proportional(caption),
        theme.text_primary().to_egui(),
    );
    p.text(
        at + egui::vec2(0.0, caption * 1.4 + gap),
        egui::Align2::LEFT_TOP,
        "surface scope, no scrim",
        egui::FontId::proportional(caption),
        theme.text_muted().to_egui(),
    );
}

/// 시안 popup-focus View `popup-focus-leaf-under-terminal-completed`. 시안 `GroundThemes`처럼 Mocha·Latte 바탕마다
/// 팝업이 키를 가진 칸과 돌려준 칸을 나란히 둔다(좁으면 줄을 바꾼다).
fn draw_completed_under_popup(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let grounds = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ];
    cluster(
        ui,
        theme,
        "Completed terminal under a popup in an active OS window",
        |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing =
                    egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
                for (label, th) in &grounds {
                    crate::catalog::spec::wrap_item(ui, |ui| ground_under_popup(ui, th, label));
                }
            });
        },
    );
}

/// 시안 `GroundThemes`의 한 테마 바탕: 테마 이름과 두 칸.
fn ground_under_popup(ui: &mut egui::Ui, th: &Theme, label: &str) {
    let cap = |text: &str| {
        egui::RichText::new(text)
            .font(egui::FontId::monospace(th.font_size_micro.value()))
            .color(th.text_muted().to_egui())
    };
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
                ui.spacing_mut().item_spacing.y = th.spacing_md.value();
                ui.label(cap(label));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = th.spacing_lg.value();
                    for (caption, popup_has_keys) in [
                        (
                            "Popup holds key focus · ring stays, unfocused bed, no cursor",
                            true,
                        ),
                        ("Key focus returns · ring clears, focused bed", false),
                    ] {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                            ui.label(cap(caption));
                            leaf_under_popup(ui, th, popup_has_keys);
                        });
                    }
                });
            });
        });
}

fn draw_with_marks(ui: &mut egui::Ui, theme: &Theme) {
    let edge = |label, focused, ring| Marked {
        label,
        kind: Kind::PluginMesh,
        focused,
        bed_focused: focused,
        ring: Some(ring),
        field: false,
    };
    let bed = |label, focused, ring| Marked {
        label,
        kind: Kind::Terminal,
        focused,
        bed_focused: focused,
        ring: Some(ring),
        field: false,
    };
    let items = [
        edge("Occupied soft, focused", true, soft(theme)),
        edge("Occupied hard or mirror, focused", true, hard(theme)),
        edge("Occupied hard or mirror, unfocused", false, hard(theme)),
        edge("Needs input, focused", true, input(theme)),
        edge("Needs input, unfocused", false, input(theme)),
        edge("Completed, focused (inactive OS window)", true, done(theme)),
        edge("Completed, unfocused", false, done(theme)),
        bed("Bed kind, needs input, focused", true, input(theme)),
        bed("Bed kind, needs input, unfocused", false, input(theme)),
        Marked {
            label: "Bed kind, hard occupied, focused: unfocused values",
            bed_focused: false,
            ..bed("", true, hard(theme))
        },
        bed(
            "Bed kind, completed, focused (inactive OS window)",
            true,
            done(theme),
        ),
        bed("Bed kind, completed, unfocused", false, done(theme)),
        Marked {
            label: "Markdown, hard occupied, focused: unfocused values, the webview covers the ring",
            kind: Kind::Markdown,
            focused: true,
            bed_focused: false,
            ring: None,
            field: false,
        },
        Marked {
            label: "Bed kind, focused, with a focused field",
            kind: Kind::Explorer,
            focused: true,
            bed_focused: true,
            ring: None,
            field: true,
        },
    ];
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for item in &items {
            marked(ui, theme, item);
        }
        for (label, focused, ring) in [
            ("Webview, occupied soft, focused", true, soft(theme)),
            ("Webview, occupied soft, unfocused", false, soft(theme)),
            (
                "Webview, occupied hard or mirror, focused",
                true,
                hard(theme),
            ),
            (
                "Webview, occupied hard or mirror, unfocused",
                false,
                hard(theme),
            ),
            ("Webview, needs input, focused", true, input(theme)),
            ("Webview, needs input, unfocused", false, input(theme)),
            ("Webview, completed, focused", true, done(theme)),
            ("Webview, completed, unfocused", false, done(theme)),
        ] {
            webview_marked(ui, theme, label, focused, ring);
        }
        draw_completed_under_popup(ui, theme);
    });
    meta(
        ui,
        theme,
        &[
            ("paint order", "unfocused edge, then ring, then lit edge"),
            (
                "lit edge with a ring",
                "lower row only, border-width tall; the ring keeps the upper row",
            ),
            (
                "webview leaf",
                "only the ring's top side shows, in the edge's row. Unmeasured in the app",
            ),
            (
                "ring priority",
                "needs input, occupancy, completion, as before",
            ),
            (
                "keyboard focus",
                "unchanged, blue input-border-focus ring on the field",
            ),
            ("hard occupied", "unfocused bed and ink, edge follows focus"),
            ("mirror", "edge from the focus of the client that draws it"),
        ],
        &[
            TokenChip::new(
                "surface-occupied-soft-border",
                "soft ring",
                theme.surface_occupied_soft_border().to_egui(),
            ),
            TokenChip::new(
                "surface-occupied-hard-border",
                "hard ring",
                theme.surface_occupied_hard_border().to_egui(),
            ),
            TokenChip::new(
                "surface-highlight-input-border",
                "needs input ring",
                theme.surface_highlight_input_border().to_egui(),
            ),
            TokenChip::new(
                "surface-highlight-done-border",
                "completion ring",
                theme.surface_highlight_done_border().to_egui(),
            ),
        ],
    );
    note(
        ui,
        theme,
        "Completion clears when the leaf has key focus, the OS window is active, and the leaf is free of hard occupancy. A focused leaf keeps its completion highlight while the OS window is inactive, even when a soft occupancy ring hides it. On a hard-occupied leaf, the occupancy ring takes precedence over completion. On a focused leaf, needs input stays visible only while the OS window is inactive or the leaf is hard occupied. Otherwise it clears on the next frame. Hard occupancy uses the unfocused bed, so the focused bed with needs input shows an inactive OS window. Regardless of OS window activation, key focus clears soft occupancy whose parent surface is gone, with any hard occupancy keeping the hard ring visible and delaying the clear until the first frame after it ends. Soft occupancy with a live parent stays and can show its ring on a focused leaf.",
    );
}

pub fn section() -> Section {
    Section {
        id: "surface-focus",
        title: "Surface focus",
        specs: vec![
            Spec {
                id: "surface-focus-every-kind",
                title: "Surface focus on every kind",
                when: Some(
                    "bed on Tasty's surfaces · 2px edge on html and plugin · click to move focus",
                ),
                draw: draw_every_kind,
            },
            Spec {
                id: "surface-focus-edge-with-marks",
                title: "Focus with rings, marks and keyboard focus",
                when: Some(
                    "unfocused edge, ring, lit edge · hard occupied keeps the unfocused bed",
                ),
                draw: draw_with_marks,
            },
        ],
    }
}
