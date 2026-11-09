//! 이동 대기 대상(서피스·탭·페인) 표시 예제. 디자인 `gallery/layouts-move.jsx`를 옮겼다.
//! 링과 글리프는 본체와 같은 `tasty_ui_widgets` painter로 그리고, 주변 화면은 정적 데이터로 흉내 낸다.
//! 링은 대상 rect에서 가장 마지막에 그린다. 본체의 대상 선택·해제 규칙은 이 예제가 실행하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    RowParent, RowState, move_source_glyph_size, paint_move_source_chip, paint_move_source_glyph,
    paint_move_source_ring, paint_running_count, running_count_column_width,
    workspace_row_body_offset,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 예제 화면 폭. 문서 컬럼(최대 1080) 안에 스테이지 여백과 함께 들어가는 specimen 값이다.
const SCREEN_W: LogicalPx = LogicalPx(960.0);
/// 예제 화면 높이. 디자인 specimen(layouts-move.jsx `Screen`)의 고정 높이이며 대응 토큰이 없다.
const SCREEN_H: LogicalPx = LogicalPx(196.0);
/// 예제 사이드바 폭. 디자인 specimen(`Screen`)이 정한 전시용 폭이며 대응 토큰이 없다.
const SIDEBAR_W: LogicalPx = LogicalPx(160.0);
/// 예제 워크스페이스 행 높이. 디자인 specimen(`WsRow`)의 값이며 대응 토큰이 없다.
const WS_ROW_H: LogicalPx = LogicalPx(28.0);
/// 겹침 예제의 서피스 높이. 디자인 specimen의 겹침 칸 높이다.
const OVERLAP_H: LogicalPx = LogicalPx(88.0);
/// 탭 칸 예제의 행 라벨 폭. 디자인 specimen의 첫 열 폭이다.
const ROW_LABEL_W: LogicalPx = LogicalPx(80.0);
/// 접힌 레일 아바타 한 변. 디자인 specimen(`RailAvatar`)의 값이다.
const AVATAR: LogicalPx = LogicalPx(28.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    NeedsInput,
    Completion,
    Hard,
}

#[derive(Clone, Copy, Default)]
struct TabCfg {
    label: &'static str,
    active: bool,
    hover: bool,
    ring: bool,
    cue: bool,
}

#[derive(Clone, Copy)]
struct SurfCfg {
    focused: bool,
    ring: bool,
    edge: Option<Edge>,
    cmd: &'static str,
}

const fn surf(focused: bool, ring: bool, cmd: &'static str) -> SurfCfg {
    SurfCfg {
        focused,
        ring,
        edge: None,
        cmd,
    }
}

struct PaneCfg<'a> {
    tabs: &'a [TabCfg],
    ring: bool,
    surfaces: &'a [SurfCfg],
}

#[derive(Clone, Copy)]
struct WsCfg {
    name: &'static str,
    active: bool,
    running: bool,
    cue: bool,
}

const fn ws(name: &'static str, active: bool, cue: bool) -> WsCfg {
    WsCfg {
        name,
        active,
        running: active,
        cue,
    }
}

const WS_DEFAULT: &[WsCfg] = &[
    ws("tasty-core", true, false),
    ws("data-etl", false, false),
    ws("docs-site", false, false),
];

const fn tab(label: &'static str, active: bool) -> TabCfg {
    TabCfg {
        label,
        active,
        hover: false,
        ring: false,
        cue: false,
    }
}

const ZSH: TabCfg = tab("zsh", true);
const BUILD: TabCfg = tab("build.log", false);
const SERVER: TabCfg = tab("server.log", true);
const DEV: &[SurfCfg] = &[surf(false, false, "npm run dev")];
const FOCUSED: &[SurfCfg] = &[surf(true, false, "cargo test")];

fn c(h: impl Into<egui::Color32>) -> egui::Color32 {
    h.into()
}

fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .monospace()
            .size(theme.font_size_micro.value())
            .color(c(theme.text_muted())),
    );
}

fn paint_tab(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, t: &TabCfg) {
    let p = ui.painter_at(rect);
    let bg = if t.active {
        theme.tab_bg_active()
    } else if t.hover {
        theme.surface_hover()
    } else {
        theme.tab_bg()
    };
    let fg = if t.active {
        theme.tab_fg_active()
    } else if t.hover {
        theme.tab_fg_hover()
    } else {
        theme.tab_fg()
    };
    p.rect_filled(rect, 0.0, c(bg));
    let sep = theme.border_width.value();
    p.rect_filled(
        egui::Rect::from_min_max(egui::pos2(rect.max.x - sep, rect.min.y), rect.max),
        0.0,
        theme.tab_separator().to_egui_premultiplied(),
    );
    if t.active {
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width(), theme.tab_indicator_width().value()),
        );
        p.rect_filled(bar, 0.0, c(theme.tab_indicator()));
    }
    let pad = theme.tab_padding_x().value();
    let gap = theme.tab_gap().value();
    let icon = theme.tab_icon_size().value();
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + pad, rect.center().y - icon * 0.5),
        egui::vec2(icon, icon),
    );
    tasty_icons::TERMINAL
        .image(icon, c(fg))
        .paint_at(ui, icon_rect);
    let glyph = move_source_glyph_size(theme);
    let mut text_right = rect.max.x - pad;
    if t.cue {
        // 제목 뒤에 둔다. 제목이 먼저 잘린다.
        let slot = egui::Rect::from_min_size(
            egui::pos2(text_right - glyph, rect.center().y - glyph * 0.5),
            egui::vec2(glyph, glyph),
        );
        paint_move_source_glyph(ui, theme, slot);
        text_right -= glyph + gap;
    }
    let font = egui::FontId::proportional(theme.tab_bar_label_font_size.value());
    let text_x = icon_rect.max.x + gap;
    let clip = egui::Rect::from_min_max(
        egui::pos2(text_x, rect.min.y),
        egui::pos2(text_right, rect.max.y),
    );
    ui.painter_at(clip).text(
        egui::pos2(text_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        t.label,
        font,
        c(fg),
    );
    if t.ring {
        paint_move_source_ring(&p, theme, rect);
    }
}

fn paint_surface(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, s: &SurfCfg) {
    let p = ui.painter_at(rect);
    let term = theme.surface("terminal");
    let (bg, fg) = if s.focused {
        (term.focused_bg, theme.text_primary())
    } else {
        (term.unfocused_bg, term.unfocused_fg)
    };
    p.rect_filled(rect, 0.0, c(bg));
    let pad = theme.spacing_sm.value();
    let font = egui::FontId::monospace(theme.font_size_term_sm.value());
    let line = theme.spacing_lg.value();
    let origin = rect.min + egui::vec2(pad, pad);
    p.text(
        origin,
        egui::Align2::LEFT_TOP,
        "~/tasty",
        font.clone(),
        c(theme.accent_success()),
    );
    p.text(
        origin + egui::Vec2::Y * line,
        egui::Align2::LEFT_TOP,
        format!("> {}", s.cmd),
        font,
        c(fg),
    );
    if let Some(edge) = s.edge {
        let (color, width) = match edge {
            Edge::NeedsInput => (
                theme.accent_warning(),
                theme.surface_highlight_input_width(),
            ),
            Edge::Completion => (theme.accent_primary(), theme.surface_highlight_done_width()),
            Edge::Hard => (
                theme.surface_occupied_hard_border(),
                theme.surface_occupied_border_width(),
            ),
        };
        p.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(width.value(), c(color)),
            egui::StrokeKind::Inside,
        );
    }
    // 링은 알림·점유 테두리 뒤에 그린다.
    if s.ring {
        paint_move_source_ring(&p, theme, rect);
    }
}

fn paint_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, pane: &PaneCfg<'_>) {
    let bar_h = theme.tab_bar_height.value();
    let strip = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), bar_h));
    ui.painter_at(rect)
        .rect_filled(strip, 0.0, c(theme.bg_sidebar()));
    let tab_w = theme.tab_width.value();
    let mut x = strip.min.x;
    for t in pane.tabs {
        let cell = egui::Rect::from_min_size(egui::pos2(x, strip.min.y), egui::vec2(tab_w, bar_h))
            .intersect(strip);
        if cell.width() > 0.0 {
            paint_tab(ui, theme, cell, t);
        }
        x += tab_w;
    }
    let sep = theme.border_width.value();
    let content = egui::Rect::from_min_max(egui::pos2(rect.min.x, strip.max.y), rect.max);
    ui.painter_at(rect)
        .rect_filled(content, 0.0, c(theme.separator.to_egui_premultiplied()));
    let n = pane.surfaces.len().max(1) as f32;
    let w = (content.width() - sep * (n - 1.0)) / n;
    for (i, s) in pane.surfaces.iter().enumerate() {
        let sx = content.min.x + (w + sep) * i as f32;
        let r = egui::Rect::from_min_size(
            egui::pos2(sx, content.min.y),
            egui::vec2(w, content.height()),
        );
        paint_surface(ui, theme, r, s);
    }
    if pane.ring {
        paint_move_source_ring(&ui.painter_at(rect), theme, rect);
    }
}

fn paint_ws_row(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, w: &WsCfg) {
    let p = ui.painter_at(rect);
    if w.active {
        p.rect_filled(rect, 0.0, c(theme.surface_active()));
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(
                theme.workspace_row_active_bar_width().value(),
                rect.height(),
            ),
        );
        p.rect_filled(bar, 0.0, c(theme.accent_primary()));
    }
    let pad = theme.workspace_row_padding_x().value();
    let slot = theme.workspace_dot_slot().value();
    let running = usize::from(w.running);
    let dot = if w.running {
        theme.status_dot_success()
    } else {
        theme.status_dot_idle()
    };
    let dot_c = egui::pos2(rect.min.x + pad + slot * 0.5, rect.center().y);
    p.circle_filled(dot_c, theme.status_dot_size().value() * 0.5, c(dot));
    let column = egui::Rect::from_min_size(
        egui::pos2(
            rect.min.x + pad + slot + theme.workspace_running_count_gap().value(),
            rect.min.y,
        ),
        egui::vec2(
            running_count_column_width(ui, theme, running),
            rect.height(),
        ),
    );
    let state = RowState::of(true, w.active, false, false);
    let ink = state.meta(theme, RowParent::Mantle);
    paint_running_count(&p, theme, column, running, ink);
    let fg = if w.active {
        theme.text_on_active()
    } else {
        theme.text_secondary()
    };
    p.text(
        egui::pos2(
            rect.min.x + pad + workspace_row_body_offset(ui, theme, running),
            rect.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        w.name,
        egui::FontId::proportional(theme.font_size_body.value()),
        c(fg),
    );
    if w.cue {
        // 배지 묶음 앞, 행의 오른쪽 끝에 둔다.
        let g = move_source_glyph_size(theme);
        let slot = egui::Rect::from_min_size(
            egui::pos2(rect.max.x - pad - g, rect.center().y - g * 0.5),
            egui::vec2(g, g),
        );
        paint_move_source_glyph(ui, theme, slot);
    }
}

fn screen(ui: &mut egui::Ui, theme: &Theme, wss: &[WsCfg], panes: &[PaneCfg<'_>]) {
    let w = ui.available_width().min(SCREEN_W.value());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, SCREEN_H.value()), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let sep = theme.border_width.value();
    p.rect_filled(rect, 0.0, c(theme.separator.to_egui_premultiplied()));
    let side = egui::Rect::from_min_size(rect.min, egui::vec2(SIDEBAR_W.value(), rect.height()));
    p.rect_filled(side, 0.0, c(theme.bg_sidebar()));
    let mut y = side.min.y + theme.spacing_sm.value();
    for row in wss {
        let r = egui::Rect::from_min_size(
            egui::pos2(side.min.x, y),
            egui::vec2(SIDEBAR_W.value(), WS_ROW_H.value()),
        );
        paint_ws_row(ui, theme, r, row);
        y += WS_ROW_H.value() + theme.border_width.value() * 2.0;
    }
    let main = egui::Rect::from_min_max(egui::pos2(side.max.x + sep, rect.min.y), rect.max);
    let n = panes.len().max(1) as f32;
    let pw = (main.width() - sep * (n - 1.0)) / n;
    for (i, pane) in panes.iter().enumerate() {
        let px = main.min.x + (pw + sep) * i as f32;
        let r =
            egui::Rect::from_min_size(egui::pos2(px, main.min.y), egui::vec2(pw, main.height()));
        paint_pane(ui, theme, r, pane);
    }
}

fn two_panes(ui: &mut egui::Ui, theme: &Theme, left: PaneCfg<'_>, right: PaneCfg<'_>) {
    screen(ui, theme, WS_DEFAULT, &[left, right]);
}

fn the_mark(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        caption(ui, theme, "1 · surface — right half of a split tab");
        two_panes(
            ui,
            theme,
            PaneCfg {
                tabs: &[ZSH, BUILD],
                ring: false,
                surfaces: &[
                    surf(true, false, "cargo test"),
                    surf(false, true, "tail -f app.log"),
                ],
            },
            PaneCfg {
                tabs: &[SERVER],
                ring: false,
                surfaces: DEV,
            },
        );
        caption(ui, theme, "2a · tab — active tab");
        two_panes(
            ui,
            theme,
            PaneCfg {
                tabs: &[TabCfg { ring: true, ..ZSH }, BUILD],
                ring: false,
                surfaces: FOCUSED,
            },
            PaneCfg {
                tabs: &[SERVER],
                ring: false,
                surfaces: DEV,
            },
        );
        caption(ui, theme, "2b · tab — inactive tab");
        two_panes(
            ui,
            theme,
            PaneCfg {
                tabs: &[
                    ZSH,
                    TabCfg {
                        ring: true,
                        ..BUILD
                    },
                ],
                ring: false,
                surfaces: FOCUSED,
            },
            PaneCfg {
                tabs: &[SERVER],
                ring: false,
                surfaces: DEV,
            },
        );
        caption(ui, theme, "3 · pane — tab strip + content in one ring");
        two_panes(
            ui,
            theme,
            PaneCfg {
                tabs: &[ZSH, BUILD],
                ring: false,
                surfaces: FOCUSED,
            },
            PaneCfg {
                tabs: &[SERVER],
                ring: true,
                surfaces: DEV,
            },
        );
    });
    let latte = crate::host_shell::latte_theme();
    spec::stage(ui, &latte, StageVariant::Column, |ui| {
        caption(ui, &latte, "Latte · 1 surface + 3 pane");
        two_panes(
            ui,
            &latte,
            PaneCfg {
                tabs: &[ZSH, BUILD],
                ring: false,
                surfaces: &[
                    surf(true, false, "cargo test"),
                    surf(false, true, "tail -f app.log"),
                ],
            },
            PaneCfg {
                tabs: &[SERVER],
                ring: true,
                surfaces: DEV,
            },
        );
    });
    spec::meta(
        ui,
        theme,
        &[
            ("shape", "dashed ring, 4 on / 4 off"),
            ("weight", "2px focus-ring-width"),
            ("placement", "INSIDE the rect — never on the shared divider"),
            ("surface", "surface rect"),
            ("tab", "tab cell rect (24 × tab width)"),
            ("pane", "pane rect = tab strip + content"),
            ("motion", "none — static"),
            ("input", "pass-through"),
        ],
        &[
            TokenChip::new(
                "move-source-ring",
                "ring (→ accent-move → pink)",
                c(theme.move_source_ring()),
            ),
            TokenChip::without_color("move-source-ring-width", "2px"),
            TokenChip::without_color("move-source-dash", "dash 4"),
            TokenChip::without_color("move-source-dash-gap", "gap 4"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Same language, different rect. A pane ring encloses the tab strip, a surface ring does \
         not. Pink is the one Catppuccin hue with no state attached. Inside, not outside: split \
         surfaces share a 1px divider, so an outside ring would paint over the neighbour.",
    );
}

fn tab_states(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        let tab_w = theme.tab_width.value();
        let bar_h = theme.tab_bar_height.value();
        for (label, base) in [
            (
                "active",
                TabCfg {
                    active: true,
                    ..BUILD
                },
            ),
            ("inactive", BUILD),
            (
                "hover",
                TabCfg {
                    hover: true,
                    ..BUILD
                },
            ),
        ] {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                let (lr, _) = ui.allocate_exact_size(
                    egui::vec2(ROW_LABEL_W.value(), bar_h),
                    egui::Sense::hover(),
                );
                ui.painter_at(lr).text(
                    lr.left_center(),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::monospace(theme.font_size_micro.value()),
                    c(theme.text_muted()),
                );
                for ring in [false, true] {
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(tab_w, bar_h), egui::Sense::hover());
                    ui.painter_at(r).rect_filled(r, 0.0, c(theme.bg_sidebar()));
                    paint_tab(ui, theme, r, &TabCfg { ring, ..base });
                }
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("rect", "tab cell, incl. its right separator"),
            ("active indicator", "dashes over it, gaps show it"),
            ("title / icon", "unchanged — attention tint still applies"),
        ],
        &[],
    );
}

fn off_screen(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        caption(ui, theme, "4a · target (any kind) is in workspace data-etl");
        screen(
            ui,
            theme,
            &[
                ws("tasty-core", true, false),
                ws("data-etl", false, true),
                ws("docs-site", false, false),
            ],
            &[
                PaneCfg {
                    tabs: &[ZSH, BUILD],
                    ring: false,
                    surfaces: FOCUSED,
                },
                PaneCfg {
                    tabs: &[SERVER],
                    ring: false,
                    surfaces: DEV,
                },
            ],
        );
        caption(
            ui,
            theme,
            "4b · target surface sits in inactive tab build.log, this workspace",
        );
        two_panes(
            ui,
            theme,
            PaneCfg {
                tabs: &[ZSH, TabCfg { cue: true, ..BUILD }],
                ring: false,
                surfaces: FOCUSED,
            },
            PaneCfg {
                tabs: &[SERVER],
                ring: false,
                surfaces: DEV,
            },
        );
        caption(ui, theme, "4c · collapsed rail — corner chip, bottom-left");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
            for (ch, cue) in [("T", false), ("D", true), ("S", false)] {
                let (r, _) = ui.allocate_exact_size(
                    egui::vec2(AVATAR.value(), AVATAR.value()),
                    egui::Sense::hover(),
                );
                let p = ui.painter_at(r.expand(theme.border_width.value()));
                p.rect_filled(r, theme.corner_radius.value(), c(theme.surface_raised()));
                p.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    ch,
                    egui::FontId::monospace(theme.font_size_body.value()),
                    c(theme.text_secondary_raised()),
                );
                if cue {
                    paint_move_source_chip(ui, theme, r, c(theme.bg_sidebar()));
                }
            }
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("row", "trailing, before the badge group"),
            ("tab cell", "after the title (title truncates)"),
            (
                "rail",
                "12px chip bottom-left — top-right = attention dot, bottom-right = mirror chip",
            ),
            ("text", "none — glyph only, no i18n length"),
        ],
        &[
            TokenChip::new(
                "move-source-glyph",
                "glyph (→ accent-move)",
                c(theme.move_source_glyph()),
            ),
            TokenChip::without_color("move-source-glyph-size", "12 — row · tab"),
            TokenChip::without_color("move-source-chip-size", "12 — rail chip"),
            TokenChip::without_color("move-source-chip-glyph-size", "8 — glyph in chip"),
        ],
    );
    spec::dont(
        ui,
        theme,
        "Don't ring a container. A ringed tab means \"this tab moves\"; a ringed workspace row \
         would read as \"this workspace moves\", which is not an action.",
    );
}

fn overlap_row(ui: &mut egui::Ui, theme: &Theme, prefix: &str) {
    let cases: [(&str, SurfCfg); 4] = [
        ("focused", surf(true, true, "cargo test")),
        (
            "+ needs-input",
            SurfCfg {
                edge: Some(Edge::NeedsInput),
                ..surf(false, true, "Overwrite? [y/N]")
            },
        ),
        (
            "+ completion",
            SurfCfg {
                edge: Some(Edge::Completion),
                ..surf(false, true, "build passed")
            },
        ),
        (
            "+ occupied · hard",
            SurfCfg {
                edge: Some(Edge::Hard),
                ..surf(false, true, "remote session")
            },
        ),
    ];
    let gap = theme.spacing_sm.value();
    let w = ((ui.available_width().min(SCREEN_W.value()) - gap * 3.0) / 4.0)
        .max(theme.tab_width.value());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (label, s) in cases {
            ui.vertical(|ui| {
                caption(ui, theme, &format!("{prefix}{label}"));
                let (r, _) =
                    ui.allocate_exact_size(egui::vec2(w, OVERLAP_H.value()), egui::Sense::hover());
                paint_surface(ui, theme, r, &s);
            });
        }
    });
}

fn overlap(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        overlap_row(ui, theme, "")
    });
    let latte = crate::host_shell::latte_theme();
    spec::stage(ui, &latte, StageVariant::Column, |ui| {
        overlap_row(ui, &latte, "Latte · ")
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "z-order",
                "focus bed < occupancy overlay < notification / occupancy edge < move ring",
            ),
            ("2px edge under", "gaps show it at full width"),
            ("1px edge under", "gaps show 1px edge + 1px bed"),
            ("clears", "move runs · another target armed · target closes"),
            ("kept on", "focus change · tab switch · workspace switch"),
        ],
        &[],
    );
    spec::note(
        ui,
        theme,
        "Not adopted: dimming the source the way Explorer dims a cut file (the source stays \
         usable, and a dim collides with the unfocused-surface dim), and marking destination \
         candidates (every other target is a candidate).",
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "The mark — one dashed ring, three scopes",
        Some(
            "The armed target carries a dashed 2px accent-move ring until the move runs, another \
             target is armed, or the target closes. Only the rect changes between kinds.",
        ),
    );
    the_mark(ui, theme);
    spec::spec(
        ui,
        theme,
        "Tab cell — ring over every tab state",
        Some("Pink dashes cover the active indicator; the blue shows through the gaps."),
    );
    tab_states(ui, theme);
    spec::spec(
        ui,
        theme,
        "Target not visible — move glyph on the nearest visible container",
        Some("Ring = this is it; glyph = it is in here. Only one cue is on screen at a time."),
    );
    off_screen(ui, theme);
    spec::spec(
        ui,
        theme,
        "Overlap — dashes on top, gaps show what is underneath",
        Some("The move ring is painted last on the target rect, above every other edge."),
    );
    overlap(ui, theme);
}
