//! 불투명 모드 native WebView 페이지 위에 뜬 팝업 예제. 장면마다 투명(현재 그대로)·불투명·
//! 숨김(현재 대체 동작) 세 열을 Mocha·Latte 두 줄로 놓는다. 불투명 열의 점선 사각형은 host 가
//! 페이지 위에 칠할 수 있는 상자(팝업·드롭다운)를 가리키며 상자 바깥 2px 에
//! specimen-annotation-border 로 그린다. B 장면의 기각 열은 상자를 불투명으로 두지만 점선은
//! 그리지 않는다(점선 절반이 scrim 위에 놓인다).

use tasty_egui_theme::overlay_edge::BoxEdge;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::{command_palette, convert, file_picker, tools_menu};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 장면 한 칸의 축소 배율 — 디자인 `ZOOM`. 셸과 팝업을 같은 배율 테마로 그린다.
const ZOOM: f32 = 0.56;
/// 장면 틀 크기(배율 전) — 디자인 `Frame` 기본 560 × 340, D 는 360 높이.
const FRAME_W: LogicalPx = LogicalPx(560.0);
const FRAME_H: LogicalPx = LogicalPx(340.0);
const FRAME_H_D: LogicalPx = LogicalPx(360.0);
/// 점선이 상자 바깥으로 떨어지는 거리 — 디자인 `outlineOffset: 2`.
const HOST_OUTLINE_OFFSET: LogicalPx = LogicalPx(2.0);
// 아래는 디자인 storyboard 의 장면 안 배치 좌표(배율 전)다. 본체 치수가 아니라 역할별 Theme
// 토큰이 없으며 이 예제에서만 쓴다.
/// 페이지 본문 안쪽 여백 — 디자인 `padding: 20px 24px`.
const PAGE_PAD_X: LogicalPx = LogicalPx(24.0);
const PAGE_PAD_Y: LogicalPx = LogicalPx(20.0);
/// A 장면 탭 막대 높이.
const SCENE_A_BAR_H: LogicalPx = LogicalPx(28.0);
/// B 장면 페이지 | 터미널 분할 위치.
const SCENE_B_SPLIT_X: LogicalPx = LogicalPx(279.0);
/// B 장면 팔레트 위쪽 거리.
const SCENE_B_PALETTE_TOP: LogicalPx = LogicalPx(36.0);
/// C 장면 사이드바 너비와 메뉴 왼쪽 거리.
const SCENE_C_SIDEBAR_W: LogicalPx = LogicalPx(150.0);
const SCENE_C_MENU_LEFT: LogicalPx = LogicalPx(12.0);
/// C 장면 메뉴 아래 변과 틀 아래 변의 거리 — 디자인 `opaque-over-page.jsx` SceneC 의 Host
/// `bottom: 52`. 메뉴 둘레 점선이 Tools 버튼(surface-active) 위를 지나지 않는다.
const SCENE_C_MENU_BOTTOM: LogicalPx = LogicalPx(52.0);
/// D 장면 피커 위치와 크기.
const SCENE_D_CARD_X: LogicalPx = LogicalPx(60.0);
const SCENE_D_CARD_Y: LogicalPx = LogicalPx(24.0);
const SCENE_D_CARD_W: LogicalPx = LogicalPx(440.0);
const SCENE_D_CARD_H: LogicalPx = LogicalPx(188.0);

/// 갤러리 Overlays 페이지의 "Over a web page — opaque mode" 절.
pub(crate) fn section() -> crate::catalog::Section {
    crate::catalog::section(
        "webview-opaque",
        "Over a web page — opaque mode",
        vec![
            crate::catalog::spec(
                "webview-opaque",
                "Contract — popup over a page in opaque mode",
                Some("square · no shadow · no scrim in a scope with a page · whole box"),
                draw_contract,
            ),
            crate::catalog::spec(
                "webview-opaque-convert",
                "A — convert_surface over a full-pane markdown page",
                Some("surface-scoped scrim is the page · transparent · opaque · hidden"),
                draw_convert,
            ),
            crate::catalog::spec(
                "webview-opaque-palette",
                "B — command palette across markdown page | terminal",
                Some("window scrim dropped on both panes · rejected blotch column"),
                draw_palette,
            ),
            crate::catalog::spec(
                "webview-opaque-tools",
                "C — Tools menu straddling sidebar | page",
                Some("the whole menu goes square, also over the sidebar"),
                draw_tools,
            ),
            crate::catalog::spec(
                "webview-opaque-file-picker",
                "D — file picker with the path-bar dropdown over the page",
                Some("two host rects · the dropdown follows its opaque parent"),
                draw_file_picker,
            ),
        ],
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Transparent,
    Opaque,
    Hidden,
    /// B 장면의 기각안 — 페이지 밖에만 scrim.
    Blotch,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::Transparent => "Transparent — unchanged",
            Mode::Opaque => "Opaque — this request",
            Mode::Hidden => "Current — page hidden",
            Mode::Blotch => "Rejected — scrim outside page only",
        }
    }

    fn edge(self) -> BoxEdge {
        match self {
            Mode::Opaque | Mode::Blotch => BoxEdge::OPAQUE,
            Mode::Transparent | Mode::Hidden => BoxEdge::NORMAL,
        }
    }
}

const COLS: [Mode; 3] = [Mode::Transparent, Mode::Opaque, Mode::Hidden];

fn zoomed(theme: &Theme) -> Theme {
    Theme::with_colors_and_zoom(theme.to_colors(), theme.is_light, ZOOM)
}

fn z(v: LogicalPx) -> LogicalPx {
    v * ZOOM
}

/// 장면 틀 하나를 잡고 안쪽 child Ui 를 넘긴다. 틀 밖으로 그림이 새지 않도록 자른다.
fn frame(
    ui: &mut egui::Ui,
    th: &Theme,
    h: LogicalPx,
    paint: impl FnOnce(&mut egui::Ui, egui::Rect),
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(z(FRAME_W).value(), z(h).value()),
        egui::Sense::hover(),
    );
    let p = ui.painter_at(rect);
    p.rect_filled(rect, th.corner_radius.value(), th.bg_app().to_egui());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect);
    paint(&mut child, rect);
    ui.painter_at(rect.expand(th.border_width.value()))
        .rect_stroke(
            rect,
            th.corner_radius.value(),
            egui::Stroke::new(th.border_width.value(), th.border_default().to_egui()),
            egui::StrokeKind::Inside,
        );
}

/// 페이지 자리. 숨김 열은 경계 타일, 나머지는 markdown 본문 모형이다.
fn ground(ui: &egui::Ui, th: &Theme, rect: egui::Rect, mode: Mode) {
    let p = ui.painter_at(rect);
    if mode == Mode::Hidden {
        p.rect_filled(rect, 0.0, th.bg_panel().to_egui());
        p.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(th.border_width.value(), th.border_default().to_egui()),
            egui::StrokeKind::Inside,
        );
        return;
    }
    let md = th.surface("markdown");
    p.rect_filled(rect, 0.0, md.focused_bg.to_egui());
    let pad = egui::vec2(z(PAGE_PAD_X).value(), z(PAGE_PAD_Y).value());
    let mut y = rect.top() + pad.y;
    let lines: [(&str, LogicalPx, egui::Color32); 6] = [
        (
            "Release notes — 0.14",
            th.font_size_body,
            md.focused_fg.to_egui(),
        ),
        (
            "Surface conversion keeps the running process and its scrollback.",
            th.font_size_caption,
            th.text_secondary().to_egui(),
        ),
        ("Changes", th.font_size_body, md.focused_fg.to_egui()),
        (
            "Explorer      Remote write queue",
            th.font_size_caption,
            th.text_secondary().to_egui(),
        ),
        (
            "Markdown      Address bar browse",
            th.font_size_caption,
            th.text_secondary().to_egui(),
        ),
        (
            "File picker   Folder mode",
            th.font_size_caption,
            th.text_secondary().to_egui(),
        ),
    ];
    for (text, size, color) in lines {
        p.text(
            egui::pos2(rect.left() + pad.x, y),
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::proportional(size.value()),
            color,
        );
        y += (size + th.spacing_sm).value();
    }
}

fn terminal(ui: &egui::Ui, th: &Theme, rect: egui::Rect) {
    let p = ui.painter_at(rect);
    let term = th.surface("terminal");
    p.rect_filled(rect, 0.0, term.focused_bg.to_egui());
    let font = egui::FontId::monospace(th.font_size_caption.value());
    let mut y = rect.top() + th.spacing_md.value();
    for text in [
        "❯ cargo build --release",
        "   Compiling tasty v0.14.0",
        "    Finished release [optimized]",
        "❯",
    ] {
        p.text(
            egui::pos2(rect.left() + th.spacing_md.value(), y),
            egui::Align2::LEFT_TOP,
            text,
            font.clone(),
            term.focused_fg.to_egui(),
        );
        y += (th.font_size_caption + th.spacing_xs).value();
    }
}

fn scrim(ui: &egui::Ui, th: &Theme, rect: egui::Rect) {
    ui.painter_at(rect)
        .rect_filled(rect, 0.0, th.scrim().to_egui());
}

/// host 가 칠할 수 있는 상자 표시 — 불투명 열에서만 그린다.
fn host_outline(ui: &egui::Ui, th: &Theme, boxed: egui::Rect) {
    let r = boxed.expand((z(HOST_OUTLINE_OFFSET) + th.border_width).value());
    let stroke = egui::Stroke::new(
        th.border_width.value(),
        th.specimen_annotation_border().to_egui(),
    );
    let dash = th.spacing_xs.value();
    let p = ui.ctx().layer_painter(ui.layer_id());
    for (a, b) in [
        (r.left_top(), r.right_top()),
        (r.right_top(), r.right_bottom()),
        (r.right_bottom(), r.left_bottom()),
        (r.left_bottom(), r.left_top()),
    ] {
        p.extend(egui::Shape::dashed_line(&[a, b], stroke, dash, dash));
    }
}

/// `pos` 에 카드 한 장을 그리고 그 사각형을 돌려준다.
fn place(ui: &mut egui::Ui, pos: egui::Pos2, add: impl FnOnce(&mut egui::Ui)) -> egui::Rect {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                pos,
                egui::vec2(f32::INFINITY, f32::INFINITY),
            ))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    add(&mut child);
    child.min_rect()
}

/// A — convert popup 이 markdown 페이지 하나를 덮는다. surface 범위 scrim 이 곧 페이지다.
fn scene_a(ui: &mut egui::Ui, th: &Theme, mode: Mode) {
    frame(ui, th, FRAME_H, |ui, rect| {
        let bar_h = z(SCENE_A_BAR_H);
        let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), bar_h.value()));
        let p = ui.painter_at(rect);
        p.rect_filled(bar, 0.0, th.bg_sidebar().to_egui());
        p.text(
            egui::pos2(bar.left() + th.spacing_md.value(), bar.center().y),
            egui::Align2::LEFT_CENTER,
            "release-notes.md",
            egui::FontId::proportional(th.font_size_caption.value()),
            th.text_secondary().to_egui(),
        );
        let page = egui::Rect::from_min_max(egui::pos2(rect.left(), bar.bottom()), rect.max);
        ground(ui, th, page, mode);
        if mode != Mode::Opaque {
            scrim(ui, th, page);
        }
        let w = th.convert_popup_width().value();
        let guess_h = th.item_height_interactive.value()
            + th.menu_item_height().value() * 5.0
            + th.spacing_xs.value() * 2.0;
        let pos = page.center() - egui::vec2(w, guess_h) * 0.5;
        let boxed = place(ui, pos, |ui| {
            convert::card(ui, th, crate::i18n::t("convert_popup.title"), mode.edge())
        });
        if mode == Mode::Opaque {
            host_outline(ui, th, boxed);
        }
    });
}

/// B — 명령 팔레트(창 범위 scrim)가 markdown 페이지 | 터미널 분할을 가로지른다.
fn scene_b(ui: &mut egui::Ui, th: &Theme, mode: Mode) {
    frame(ui, th, FRAME_H, |ui, rect| {
        let split = rect.left() + z(SCENE_B_SPLIT_X).value();
        let left = egui::Rect::from_min_max(rect.min, egui::pos2(split, rect.bottom()));
        let right = egui::Rect::from_min_max(
            egui::pos2(split + th.border_width.value(), rect.top()),
            rect.max,
        );
        ground(ui, th, left, mode);
        ui.painter_at(rect).vline(
            split,
            rect.y_range(),
            egui::Stroke::new(th.border_width.value(), th.border_frame().to_egui()),
        );
        terminal(ui, th, right);
        match mode {
            Mode::Transparent | Mode::Hidden => scrim(ui, th, rect),
            Mode::Blotch => scrim(ui, th, right),
            Mode::Opaque => {}
        }
        let w = th.palette_width().value();
        let pos = egui::pos2(
            rect.center().x - w * 0.5,
            rect.top() + z(SCENE_B_PALETTE_TOP).value(),
        );
        let boxed = place(ui, pos, |ui| {
            command_palette::palette_card(ui, th, mode.edge())
        });
        if mode == Mode::Opaque {
            host_outline(ui, th, boxed);
        }
    });
}

/// C — 배경을 어둡게 하지 않는 Tools 메뉴가 사이드바 | 페이지에 걸친다.
fn scene_c(ui: &mut egui::Ui, th: &Theme, mode: Mode) {
    frame(ui, th, FRAME_H, |ui, rect| {
        let side_w = z(SCENE_C_SIDEBAR_W);
        let side = egui::Rect::from_min_size(rect.min, egui::vec2(side_w.value(), rect.height()));
        let page = egui::Rect::from_min_max(egui::pos2(side.right(), rect.top()), rect.max);
        ground(ui, th, page, mode);
        let p = ui.painter_at(rect);
        p.rect_filled(side, 0.0, th.bg_sidebar().to_egui());
        p.vline(
            side.right(),
            rect.y_range(),
            egui::Stroke::new(th.border_width.value(), th.border_frame().to_egui()),
        );
        let btn = egui::Rect::from_min_max(
            egui::pos2(
                side.left() + th.spacing_sm.value(),
                side.bottom() - th.spacing_sm.value() - th.item_height_interactive.value(),
            ),
            egui::pos2(
                side.right() - th.spacing_sm.value(),
                side.bottom() - th.spacing_sm.value(),
            ),
        );
        p.rect_filled(btn, th.corner_radius.value(), th.surface_active().to_egui());
        p.text(
            egui::pos2(btn.left() + th.spacing_sm.value(), btn.center().y),
            egui::Align2::LEFT_CENTER,
            crate::i18n::t("sidebar.tools_button"),
            egui::FontId::proportional(th.font_size_caption.value()),
            th.text_on_active().to_egui(),
        );
        // 메뉴는 버튼 위, 왼쪽 정렬이다. 메뉴 하단을 기준점에 두려면 높이가 필요하므로 직전
        // 패스에 잰 높이를 쓰고, 높이를 모르거나 달라졌으면 이 패스를 버리고 다시 그리게 한다.
        let id = ui.id().with("tools_h");
        let prev_h: Option<f32> = ui.data(|d| d.get_temp(id));
        let pos = egui::pos2(
            rect.left() + z(SCENE_C_MENU_LEFT).value(),
            rect.bottom() - z(SCENE_C_MENU_BOTTOM).value() - prev_h.unwrap_or(0.0),
        );
        let boxed = place(ui, pos, |ui| tools_menu::tools_card(ui, th, mode.edge()));
        if prev_h != Some(boxed.height()) {
            ui.data_mut(|d| d.insert_temp(id, boxed.height()));
            ui.ctx().request_discard("webview opaque tools menu height");
        }
        if mode == Mode::Opaque {
            host_outline(ui, th, boxed);
        }
    });
}

/// D — 파일 피커 아래로 path bar 의 `…` 드롭다운이 페이지 위에 늘어진다.
fn scene_d(ui: &mut egui::Ui, th: &Theme, mode: Mode) {
    frame(ui, th, FRAME_H_D, |ui, rect| {
        ground(ui, th, rect, mode);
        if mode != Mode::Opaque {
            scrim(ui, th, rect);
        }
        let pos = rect.min + egui::vec2(z(SCENE_D_CARD_X).value(), z(SCENE_D_CARD_Y).value());
        let mut menu = None;
        let card = place(ui, pos, |ui| {
            menu = file_picker::deep_card_with_menu(
                ui,
                th,
                z(SCENE_D_CARD_W),
                z(SCENE_D_CARD_H),
                mode.edge(),
            );
        });
        if mode == Mode::Opaque {
            host_outline(ui, th, card);
            if let Some(menu) = menu {
                host_outline(ui, th, menu);
            }
        }
    });
}

/// 시안처럼 툴바 테마와 관계없이 Mocha·Latte 두 줄을 고정해 그린다.
fn grid(ui: &mut egui::Ui, _theme: &Theme, cols: &[Mode], scene: fn(&mut egui::Ui, &Theme, Mode)) {
    let mocha = tasty_themes::mocha_fallback();
    let latte = crate::host_shell::latte_theme();
    for (name, base) in [("Mocha", &mocha), ("Latte", &latte)] {
        let th = zoomed(base);
        spec::stage(ui, base, StageVariant::Wrap, |ui| {
            for mode in cols {
                spec::cluster(ui, base, &format!("{name} · {}", mode.label()), |ui| {
                    scene(ui, &th, *mode)
                });
            }
        });
    }
}

/// 계약 — 불투명 모드에서 페이지 위 팝업이 따르는 여섯 결정.
pub fn draw_contract(ui: &mut egui::Ui, theme: &Theme) {
    spec::meta(
        ui,
        theme,
        &[
            (
                "1 · corner",
                "square — box reads webview-opaque-radius (0); nothing to fill outside an arc",
            ),
            (
                "2 · shadow",
                "none — webview-opaque-shadow; the existing 1px border is the only silhouette",
            ),
            (
                "3 · scrim",
                "not drawn at all while any opaque-mode page is visible in the scrim's scope",
            ),
            ("  scope", "same rule for surface scope and window scope"),
            (
                "4 · straddle",
                "box rect intersects a page anywhere → the WHOLE box goes square + no shadow",
            ),
            (
                "5 · dropdown",
                "same rule; a dropdown also follows its parent shell (shell opaque → dropdown opaque)",
            ),
            (
                "6 · tokens",
                "webview-opaque-radius → radius-none → radius-0 · webview-opaque-shadow → shadow-none",
            ),
            (
                "unchanged",
                "shell structure · padding · rows · hover/active · focus look · border tokens",
            ),
        ],
        &[
            TokenChip::without_color("webview-opaque-radius", "→ radius-none → radius-0 (0)"),
            TokenChip::without_color("webview-opaque-shadow", "→ shadow-none (none)"),
            TokenChip::new(
                "border-frame",
                "shell edge kept (convert, dialogs)",
                theme.border_frame().to_egui(),
            ),
            TokenChip::new(
                "menu-border",
                "menu / dropdown edge kept",
                theme.menu_border().to_egui(),
            ),
            TokenChip::new(
                "scrim-bg",
                "not drawn while a page is visible",
                theme.scrim().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Input routing: while a target popup is visible, a click on the page body is that \
         popup's outside click and never reaches the page — popups that close on outside click \
         close (convert_surface; the command palette too), others stay open and only lose focus \
         (notification panel, search bar). Keys and IME go to the focused popup, never to the page.",
    );
    spec::note(
        ui,
        theme,
        "Why no scrim: the scrim cannot darken the page, so drawing it only on terminal / tab bar / \
         sidebar makes the page the brightest thing on screen while a modal is up. When no opaque \
         page is on screen, the scrim draws as today.",
    );
}

pub fn draw_convert(ui: &mut egui::Ui, theme: &Theme) {
    grid(ui, theme, &COLS, scene_a);
    spec::note(
        ui,
        theme,
        "Surface-scoped scrim, and the surface is the page. Opaque: no scrim, square shell, no \
         shadow, border-frame edge kept. Current: page hidden behind the boundary tile.",
    );
}

pub fn draw_palette(ui: &mut egui::Ui, theme: &Theme) {
    grid(
        ui,
        theme,
        &[Mode::Transparent, Mode::Opaque, Mode::Hidden, Mode::Blotch],
        scene_b,
    );
    spec::dont(
        ui,
        theme,
        "Don't keep the scrim on the non-page areas only: the page stays at full brightness while \
         the terminal, tab bar and sidebar go dark.",
    );
}

pub fn draw_tools(ui: &mut egui::Ui, theme: &Theme) {
    grid(ui, theme, &COLS, scene_c);
    spec::do_(
        ui,
        theme,
        "Do decide the edge once per box from its rect. A half-rounded, half-shadowed menu reads \
         as a rendering fault.",
    );
}

pub fn draw_file_picker(ui: &mut egui::Ui, theme: &Theme) {
    grid(ui, theme, &COLS, scene_d);
    spec::note(
        ui,
        theme,
        "A dropdown opened from an opaque-mode shell is opaque-mode even if its own rect ends over \
         a terminal, so the parent and child never disagree.",
    );
}

#[cfg(test)]
mod tests {
    use tasty_type_appearance::theme::Theme;

    use super::{
        HOST_OUTLINE_OFFSET, Mode, SCENE_C_MENU_BOTTOM, scene_a, scene_b, scene_c, scene_d, z,
        zoomed,
    };
    use crate::catalog::components::painted::{Painted, paint, paint_first_frame};

    type Scene = fn(&mut egui::Ui, &Theme, Mode);

    fn themes() -> [(&'static str, Theme); 2] {
        [
            ("Mocha", zoomed(&tasty_themes::mocha_fallback())),
            ("Latte", zoomed(&crate::host_shell::latte_theme())),
        ]
    }

    /// `color` 로 그은 선분 수.
    fn segments(p: &Painted, color: egui::Color32) -> usize {
        p.shapes
            .iter()
            .filter(
                |s| matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.color == color),
            )
            .count()
    }

    /// 불투명 열의 상자 표시는 specimen-annotation-border 점선이고, 다른 열과 B 장면의 기각 열에는
    /// 그 표시가 없다.
    #[test]
    fn host_outline_is_annotation_ink_only_in_the_opaque_column() {
        let scenes: [(&str, Scene, &[Mode]); 4] = [
            ("A", scene_a, &[Mode::Transparent, Mode::Hidden]),
            (
                "B",
                scene_b,
                &[Mode::Transparent, Mode::Hidden, Mode::Blotch],
            ),
            ("C", scene_c, &[Mode::Transparent, Mode::Hidden]),
            ("D", scene_d, &[Mode::Transparent, Mode::Hidden]),
        ];
        for (name, th) in themes() {
            let ink = th.specimen_annotation_border().to_egui();
            let warning = th.accent_warning().to_egui();
            assert_ne!(
                ink, warning,
                "{name}: the tokens must differ to tell them apart"
            );
            for (scene_name, scene, unmarked) in scenes {
                let opaque = paint(|ui| scene(ui, &th, Mode::Opaque));
                assert!(
                    segments(&opaque, ink) >= 4,
                    "{name} {scene_name}: the opaque column draws the dashed host rect in \
                     specimen-annotation-border"
                );
                assert_eq!(
                    segments(&opaque, warning),
                    0,
                    "{name} {scene_name}: no accent-warning mark"
                );
                for mode in unmarked {
                    let p = paint(|ui| scene(ui, &th, *mode));
                    assert_eq!(
                        segments(&p, ink),
                        0,
                        "{name} {scene_name} {}: no host rect mark",
                        mode.label()
                    );
                }
            }
        }
    }

    /// C 장면 surface-active 버튼의 글자는 text-on-active 로 그리고, 메뉴 둘레 점선은 그 버튼 위를
    /// 지나지 않는다. Mocha 는 두 글자 토큰 값이 같아 text-primary 와 가르는 것은 Latte 줄이다.
    #[test]
    fn tools_button_reads_text_on_active_and_keeps_the_mark_off_it() {
        let latte = crate::host_shell::latte_theme();
        assert_ne!(latte.text_on_active(), latte.text_primary());
        for (name, th) in themes() {
            let p = paint(|ui| scene_c(ui, &th, Mode::Opaque));
            let label = p
                .texts
                .iter()
                .find(|t| t.text == crate::i18n::t("sidebar.tools_button"))
                .expect("Tools button label");
            assert_eq!(
                p.fill_under(label.rect),
                th.surface_active().to_egui(),
                "{name}: the label sits on surface-active"
            );
            assert_eq!(label.color, th.text_on_active().to_egui(), "{name}");
            let button = p.rect_filled_around(th.surface_active().to_egui(), label.rect);
            let ink = th.specimen_annotation_border().to_egui();
            for s in &p.shapes {
                if let egui::Shape::LineSegment { points, stroke } = s
                    && stroke.color == ink
                {
                    let half = stroke.width * 0.5;
                    let seg = egui::Rect::from_two_pos(points[0], points[1]).expand(half);
                    assert!(
                        !seg.intersects(button),
                        "{name}: the host rect mark {seg:?} runs over the surface-active button \
                         {button:?}"
                    );
                }
            }
        }
    }

    /// 메뉴 높이를 기억하지 못한 첫 프레임에도 메뉴 하단이 틀 아래 SCENE_C_MENU_BOTTOM 자리에
    /// 있고, 메뉴 둘레 점선이 surface-active Tools 버튼 위를 지나지 않는다.
    #[test]
    fn tools_menu_sits_on_its_bottom_anchor_from_the_first_frame() {
        for (name, th) in themes() {
            let p = paint_first_frame(|ui| scene_c(ui, &th, Mode::Opaque));
            let label = p
                .texts
                .iter()
                .find(|t| t.text == crate::i18n::t("sidebar.tools_button"))
                .expect("Tools button label");
            let button = p.rect_filled_around(th.surface_active().to_egui(), label.rect);
            let ink = th.specimen_annotation_border().to_egui();
            let mark: Vec<egui::Rect> = p
                .shapes
                .iter()
                .filter_map(|s| match s {
                    egui::Shape::LineSegment { points, stroke } if stroke.color == ink => {
                        Some(egui::Rect::from_two_pos(points[0], points[1]))
                    }
                    _ => None,
                })
                .collect();
            assert!(mark.len() >= 4, "{name}: the opaque column draws the mark");
            // 버튼 아래 변은 틀 아래에서 spacing_sm 위다.
            let frame_bottom = button.bottom() + th.spacing_sm.value();
            let anchor = frame_bottom - z(SCENE_C_MENU_BOTTOM).value();
            let mark_bottom = mark.iter().map(|r| r.bottom()).fold(f32::MIN, f32::max);
            let menu_bottom = mark_bottom - (z(HOST_OUTLINE_OFFSET) + th.border_width).value();
            assert!(
                (menu_bottom - anchor).abs() < 0.5,
                "{name}: first frame menu bottom {menu_bottom} is not on the anchor {anchor}"
            );
            for seg in &mark {
                assert!(
                    !seg.expand(th.border_width.value() * 0.5).intersects(button),
                    "{name}: first frame mark {seg:?} runs over the surface-active button \
                     {button:?}"
                );
            }
        }
    }
}
