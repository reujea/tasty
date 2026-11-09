//! 시안 Loading overlay의 서피스 예제와 세 자리 동시 예제. 터미널 출력 위에 이동 링·스피너·검색줄을
//! 쌓고, egui 서피스는 빈 서피스 예제의 부품을 쓴다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    LoadingPlacement, RowState, paint_dashed_outline, paint_loading_overlay, paint_move_source_ring,
};

use crate::catalog::components::rail_marks::{self, Avatar};
use crate::catalog::components::sidebar::{self, WsRowSpec};
use crate::catalog::components::tab_bar::{self, CellCfg};
use crate::catalog::components::{empty_surface, search_bar};
use crate::catalog::spec::{self, StageVariant, TokenChip};

use super::matrix::{SCALE_TAB_W, lnb_bed, pair, per_theme, rail_bed, row, tab_bed};

// 시안 예제 틀의 크기(`--tasty-size-*`)다. 제품 치수가 아니다.
const TERM_W: LogicalPx = LogicalPx(288.0);
const TERM_H: LogicalPx = LogicalPx(140.0);
const WINDOW_W: LogicalPx = LogicalPx(560.0);
const WINDOW_H: LogicalPx = LogicalPx(200.0);
/// 시안 터미널 예제의 출력 줄.
const TERM: [&str; 5] = [
    "$ tasty remote attach build-01",
    "connecting to build-01:7420 …",
    "handshake ok · tasty 0.9.4",
    "requesting workspace layout",
    "$ ▌",
];

#[derive(Clone, Copy, Default)]
pub(super) struct Term {
    pub(super) loading: bool,
    pub(super) search: bool,
    pub(super) move_ring: bool,
    pub(super) boxes: bool,
}

/// 시안 `TermSurface`: 터미널 출력 위에 이동 링·스피너·검색줄 순서로 쌓는다.
pub(super) fn term_surface(ui: &mut egui::Ui, th: &Theme, size: egui::Vec2, term: Term) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    paint_term(ui, th, rect, term);
}

fn paint_term(ui: &mut egui::Ui, th: &Theme, rect: egui::Rect, term: Term) {
    let p = ui.painter_at(rect);
    p.rect(
        rect,
        0.0,
        egui::Color32::from(th.bg_panel()),
        egui::Stroke::new(
            th.border_width.value(),
            egui::Color32::from(th.border_default()),
        ),
        egui::StrokeKind::Inside,
    );
    let font = egui::FontId::monospace(th.font_size_term_sm.value());
    let line = ui.fonts(|f| f.row_height(&font));
    let pad = th.spacing_sm.value();
    for (i, text) in TERM.iter().enumerate() {
        p.text(
            rect.min + egui::vec2(pad, pad + line * i as f32),
            egui::Align2::LEFT_TOP,
            *text,
            font.clone(),
            egui::Color32::from(th.text_secondary()),
        );
    }
    if term.move_ring {
        paint_move_source_ring(&p, th, rect);
    }
    if term.loading {
        paint_loading_overlay(
            ui,
            th,
            rect,
            LoadingPlacement::Surface,
            egui::Color32::TRANSPARENT,
        );
        if term.boxes {
            paint_dashed_outline(&p, th, rect, 0.0, egui::Color32::from(th.accent_move()));
        }
    }
    if term.search {
        let w = search_bar::WIDTH.value();
        let area = egui::Rect::from_min_size(
            egui::pos2(rect.right() - pad - w, rect.top() + pad),
            egui::vec2(w, rect.height() - pad),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        search_bar::bar(&mut child, th, "1/1", false);
    }
}

pub(super) fn term_size() -> egui::Vec2 {
    egui::vec2(TERM_W.value(), TERM_H.value())
}

/// 검색줄 예제는 실제 검색줄(360)이 들어가도록 서피스를 넓힌다.
fn search_term_size(th: &Theme) -> egui::Vec2 {
    egui::vec2(
        (search_bar::WIDTH + th.spacing_sm.scaled(2.0)).value(),
        TERM_H.value(),
    )
}

/// 본체 egui 서피스(빈 서피스) 위의 스피너. 시안의 markdown 예제는 Tasty에서 WebView라 대신한다.
fn egui_surface(ui: &mut egui::Ui, th: &Theme, loading: bool) {
    let rect = empty_surface::surface_body(ui, th);
    if loading {
        paint_loading_overlay(
            ui,
            th,
            rect,
            LoadingPlacement::Surface,
            egui::Color32::TRANSPARENT,
        );
    }
}

/// 네이티브 WebView 서피스: 안에는 아무것도 그리지 않는다는 의도만 보인다.
fn webview_intent(ui: &mut egui::Ui, th: &Theme) {
    let (rect, _) = ui.allocate_exact_size(term_size(), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, egui::Color32::from(th.bg_panel()));
    paint_dashed_outline(&p, th, rect, 0.0, egui::Color32::from(th.border_strong()));
    let pad = th.spacing_md.value();
    let mut job = egui::text::LayoutJob::simple(
        "WebView (native, drawn above Tasty) · no in-surface spinner · the tab and the LNB card \
         carry it"
            .to_owned(),
        egui::FontId::monospace(th.font_size_micro.value()),
        egui::Color32::from(th.text_muted()),
        rect.width() - pad * 2.0,
    );
    job.halign = egui::Align::Center;
    let galley = ui.fonts(|f| f.layout_job(job));
    p.galley(
        egui::pos2(rect.center().x, rect.center().y - galley.size().y * 0.5),
        galley,
        egui::Color32::PLACEHOLDER,
    );
}

pub(super) fn surfaces(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "Surface — corner bed, content stays live",
        Some(
            "Inside a surface the spinner sits on a 24 bed bottom-right, 8 in from the rect. No \
             dim: cells keep drawing. The move ring stays on the edge; the search bar draws \
             above the spinner. A native WebView surface draws nothing inside; the tab and the \
             LNB card carry the loading.",
        ),
    );
    per_theme(ui, theme, |ui, th| {
        pair(ui, th, "terminal", |ui, on| {
            let term = Term {
                loading: on,
                ..Term::default()
            };
            term_surface(ui, th, term_size(), term);
        });
        pair(ui, th, "terminal · search bar + move ring", |ui, on| {
            let term = Term {
                loading: on,
                search: true,
                move_ring: true,
                boxes: false,
            };
            term_surface(ui, th, search_term_size(th), term);
        });
        pair(ui, th, "egui surface (empty surface)", |ui, on| {
            egui_surface(ui, th, on);
        });
        tab_bar::caption(ui, th, "HTML · native WebView (intent)");
        webview_intent(ui, th);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("place", "bottom-right · inset 8 · inside the rect"),
            ("bed", "24 · surface-raised · 1px border-default · radius 4"),
            (
                "terminal",
                "no dim, no motion on cells · PTY grid unchanged",
            ),
            (
                "z-order",
                "content < move ring < spinner < search bar / popups",
            ),
            (
                "WebView",
                "nothing in-surface; tab + LNB show it; never hide the WebView",
            ),
        ],
        &[
            TokenChip::without_color("loading-overlay-surface-size", "→ spinner-size 16"),
            TokenChip::without_color("loading-overlay-surface-bed-size", "→ size-24"),
        ],
    );
}

/// 시안 `MiniWindow`: 레일 + 탭 줄 + 터미널 서피스. 세 자리가 함께 켜진다.
fn mini_window(ui: &mut egui::Ui, th: &Theme, loading: bool) {
    let w = WINDOW_W.value().min(ui.available_width());
    let (frame, _) = ui.allocate_exact_size(egui::vec2(w, WINDOW_H.value()), egui::Sense::hover());
    // 레일 폭은 레일 표지 예제의 접힌 레일과 같은 식이다.
    let rail_w =
        th.sidebar_collapsed_slot_width.value() + th.spacing_lg.value() + th.spacing_xs.value();
    let rail = egui::Rect::from_min_size(frame.min, egui::vec2(rail_w, frame.height()));
    let p = ui.painter_at(frame);
    p.rect_filled(rail, 0.0, egui::Color32::from(th.bg_sidebar()));
    p.vline(
        rail.right(),
        rail.y_range(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
    let size = rail_marks::avatar_size(th);
    let gap = th.spacing_xs.value();
    let avatars = [
        Avatar {
            letter: "T",
            ..Avatar::NONE
        },
        Avatar {
            letter: "B",
            active: true,
            mirror: true,
            loading,
            ..Avatar::NONE
        },
        Avatar {
            letter: "S",
            notif: true,
            ..Avatar::NONE
        },
    ];
    for (i, av) in avatars.into_iter().enumerate() {
        let top = rail.top() + th.spacing_sm.value() + (size.y + gap) * i as f32;
        let rect = egui::Rect::from_min_size(egui::pos2(rail.center().x - size.x * 0.5, top), size);
        rail_marks::paint_avatar(ui, th, rect, av);
    }
    let body = egui::Rect::from_min_max(egui::pos2(rail.right(), frame.top()), frame.max);
    let strip =
        egui::Rect::from_min_size(body.min, egui::vec2(body.width(), th.tab_height().value()));
    p.rect_filled(strip, 0.0, egui::Color32::from(th.surface_raised()));
    let tab_w = th.tab_width.value();
    let tabs = [
        CellCfg {
            label: "build-server",
            active: true,
            loading,
            ..CellCfg::TERMINAL
        },
        CellCfg {
            label: "logs",
            busy: true,
            ..CellCfg::TERMINAL
        },
    ];
    for (i, cfg) in tabs.into_iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            egui::pos2(strip.left() + tab_w * i as f32, strip.top()),
            egui::vec2(tab_w, strip.height()),
        );
        tab_bar::tab_cell(ui, th, cell, cfg);
    }
    let surface = egui::Rect::from_min_max(egui::pos2(body.left(), strip.bottom()), body.max);
    paint_term(
        ui,
        th,
        surface,
        Term {
            loading,
            ..Term::default()
        },
    );
    p.rect_stroke(
        frame,
        th.corner_radius.value(),
        egui::Stroke::new(
            th.border_width.value(),
            egui::Color32::from(th.border_frame()),
        ),
        egui::StrokeKind::Inside,
    );
}

pub(super) fn together(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "All three at once · UI scale · reduced motion · timing",
        Some(
            "Each placement is switched by its caller. The overlay appears only if the request is \
             still running after the loading delay, without a fade, and goes the instant loading \
             ends. With reduced motion it is a still arc on its track. Sizes follow the UI scale.",
        ),
    );
    per_theme(ui, theme, |ui, th| {
        // 창 예제 둘을 나란히 두면 문서 칸(1000)을 넘으므로 off·on을 세로로 쌓는다.
        for (label, on) in [
            ("rail + tab + surface · off", false),
            ("rail + tab + surface · on", true),
        ] {
            tab_bar::caption(ui, th, label);
            mini_window(ui, th, on);
        }
        let mut still = th.clone();
        still.reduced_motion = true;
        tab_bar::caption(ui, th, "reduced motion · still arc");
        mini_window(ui, &still, true);
    });
    let mocha = tasty_themes::mocha_fallback();
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for zoom in [1.0, 1.25, 1.5] {
            let th = sidebar::with_zoom(&mocha, zoom * theme.ui_zoom);
            spec::cluster(ui, &th, &format!("Mocha · ×{zoom}"), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                    let cfg = CellCfg {
                        label: "build-server",
                        active: true,
                        busy: true,
                        loading: true,
                        ..CellCfg::TERMINAL
                    };
                    tab_bed(ui, &th, SCALE_TAB_W.scaled(zoom), cfg);
                    lnb_bed(
                        ui,
                        &th,
                        WsRowSpec {
                            state: RowState::Selected,
                            loading: true,
                            ..row("build-farm")
                        },
                    );
                    let av = Avatar {
                        letter: "B",
                        active: true,
                        mirror: true,
                        notif: true,
                        loading: true,
                        ..Avatar::NONE
                    };
                    rail_bed(ui, &th, av);
                });
            });
        }
    });
    let delay = format!(
        "{} ms · loading-overlay-delay",
        theme.loading_overlay_delay().0
    );
    let rotation = format!("{} ms · spinner-duration", theme.spinner_duration().0);
    spec::meta(
        ui,
        theme,
        &[
            ("switching", "per placement, by its caller"),
            ("delay", &delay),
            (
                "in / out",
                "no fade · gone immediately on finish or failure",
            ),
            ("rotation", &rotation),
            ("reduced motion", "still arc + track"),
            ("scale", "token sizes × ui-scale; stroke 2px at 1×"),
            (
                "hidden placements",
                "off-screen tab / collapsed card not drawn → no repaint",
            ),
        ],
        &[
            TokenChip::without_color("loading-overlay-delay", "→ motion-ui-fade 200"),
            TokenChip::without_color("spinner-duration", "→ duration-900"),
        ],
    );
}
