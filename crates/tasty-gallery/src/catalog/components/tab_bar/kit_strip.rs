//! 페인 탭 스트립 시안(`gallery/layouts-tabstrip.jsx`)의 스크롤 화살표·숨은 이동 대상 예제.
//! 스트립·탭 칸은 정적 데이터로 그리고 화살표는 본체와 같은 `paint_tab_scroll_arrow`로 그린다.
//! 본체의 스크롤·노출 판정은 실행하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Attention, HtmlScriptMarkerKind, LoadingPlacement, TabScrollArrowInk,
    TabScrollArrowSide as Side, move_source_glyph_size, paint_dashed_outline,
    paint_loading_overlay, paint_move_source_glyph, paint_num_keycap, paint_tab_scroll_arrow,
    tab_title_color,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 시안 스트립 폭(`--tasty-size-560`). 무대가 좁으면 가용 폭으로 줄인다.
const STRIP_W: LogicalPx = LogicalPx(560.0);
/// 시안 `Strip`의 기본 탭 다섯 개. 두 번째 탭이 활성이다.
const STRIP_TABS: [&str; 5] = ["server", "dev", "vim", "logs", "tests"];
/// 시안 좁은 탭 예제의 칸 폭(`--tasty-size-120`). 제목이 먼저 말줄임되는 모습을 보인다.
const NARROW_TAB_W: LogicalPx = LogicalPx(120.0);

/// 스트립이 어디까지 스크롤됐는지. 시안 `at`.
#[derive(Clone, Copy, PartialEq)]
enum At {
    Start,
    Mid,
    End,
}

#[derive(Clone, Copy)]
struct StripCfg {
    focused: bool,
    at: At,
    hover: Option<Side>,
    /// 이동 대기 대상이 가려진 쪽. 시안 `moveSide`.
    mv: Option<Side>,
}

fn c(h: impl Into<egui::Color32>) -> egui::Color32 {
    h.into()
}

pub(crate) fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .monospace()
            .size(theme.font_size_micro.value())
            .color(c(theme.text_muted())),
    );
}

/// 시안 `Themed`: 테마 이름과 예제 줄을 담는 bg-app 카드.
pub(crate) fn themed(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    add: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(c(theme.bg_app()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            c(theme.border_default()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
                ui.label(
                    egui::RichText::new(name)
                        .size(theme.font_size_caption.value())
                        .color(c(theme.text_muted())),
                );
                add(ui);
            });
        });
}

/// 시안 `Arrow`의 잉크 우선순위: disabled > move > fg.
fn arrow(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, side: Side, cfg: StripCfg) {
    let disabled = match side {
        Side::Left => cfg.at == At::Start,
        Side::Right => cfg.at == At::End,
    };
    let ink = if disabled {
        TabScrollArrowInk::Disabled
    } else if cfg.mv == Some(side) {
        TabScrollArrowInk::Move
    } else {
        TabScrollArrowInk::Enabled
    };
    paint_tab_scroll_arrow(ui, theme, rect, side, ink, cfg.hover == Some(side));
}

/// 시안 `TabCellS`의 상태. 스트립 예제는 `label`·`active`만 쓴다.
#[derive(Clone, Copy)]
pub(crate) struct CellCfg {
    pub(crate) label: &'static str,
    pub(crate) active: bool,
    pub(crate) hover: bool,
    /// html 스크립트 표지 종류와 그 칸의 hover 여부.
    pub(crate) marker: Option<(HtmlScriptMarkerKind, bool)>,
    pub(crate) mv: bool,
    pub(crate) busy: bool,
    /// 터미널 탭이면 터미널 아이콘, 아니면 html 아이콘을 그린다.
    pub(crate) terminal: bool,
    /// 제목 색을 정하는 attention. 본체 `tab_title_color`와 같은 순위다.
    pub(crate) attention: Option<Attention>,
    /// attach 응답 대기 스피너를 아이콘 칸에 겹친다.
    pub(crate) loading: bool,
    /// 전환 키캡 숫자. 있으면 아이콘 대신 키캡을 그리고 스피너보다 위에 둔다.
    pub(crate) keycap: Option<&'static str>,
    /// 오버레이 rect(아이콘 칸)를 점선으로 보인다. 시안 첫 예제의 상자다.
    pub(crate) boxes: bool,
}

impl CellCfg {
    /// 시안 로딩 예제의 기본 칸: 비활성 터미널 탭 "build-server".
    pub(crate) const TERMINAL: Self = Self {
        label: "build-server",
        terminal: true,
        ..Self::BASE
    };

    /// 모든 표시를 끈 html 탭 칸. 상수 예제의 나머지 필드를 채운다.
    pub(crate) const BASE: Self = Self {
        label: "",
        active: false,
        hover: false,
        marker: None,
        mv: false,
        busy: false,
        terminal: false,
        attention: None,
        loading: false,
        keycap: None,
        boxes: false,
    };
}

/// 시안 `TabCellS`: [아이콘][제목…] · 오른쪽 고정 묶음 [표지][move][busy][close].
/// 묶음은 줄지 않고 제목이 먼저 말줄임된다. close 칸은 활성·hover 전에도 자리를 지킨다.
pub(crate) fn tab_cell(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, cfg: CellCfg) {
    let p = ui.painter();
    let (bg, fg) = if cfg.active {
        (theme.tab_bg_active(), theme.tab_fg_active())
    } else if cfg.hover {
        (theme.tab_bg_hover(), theme.tab_fg_hover())
    } else {
        (theme.tab_bg(), theme.tab_fg())
    };
    let fg = if cfg.attention.is_some() {
        tab_title_color(theme, cfg.attention, cfg.active)
    } else {
        fg
    };
    p.rect_filled(rect, 0.0, c(bg));
    let sep = theme.border_width.value();
    p.rect_filled(
        egui::Rect::from_min_max(egui::pos2(rect.max.x - sep, rect.min.y), rect.max),
        0.0,
        theme.tab_separator().to_egui_premultiplied(),
    );
    if cfg.active {
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width(), theme.tab_indicator_width().value()),
        );
        p.rect_filled(bar, 0.0, c(theme.tab_indicator()));
    }
    let gap = theme.tab_gap().value();
    let icon = theme.tab_icon_size().value();
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.min.x + theme.tab_padding_x().value(),
            rect.center().y - icon * 0.5,
        ),
        egui::vec2(icon, icon),
    );
    // 우선순위: 전환 키캡 > 스피너 > 숨은 아이콘.
    if let Some(digit) = cfg.keycap {
        paint_num_keycap(p, theme, icon_rect.center(), digit, cfg.active, 1.0);
    } else if cfg.loading {
        // 칸 함수는 공유 `&Ui`만 받으므로 같은 layer에 오버레이용 Ui를 따로 만든다.
        let mut overlay = egui::Ui::new(
            ui.ctx().clone(),
            ui.id()
                .with(("tab_cell_loading", rect.min.x as i32, rect.min.y as i32)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(icon_rect),
        );
        overlay.set_clip_rect(rect.intersect(ui.clip_rect()));
        paint_loading_overlay(&mut overlay, theme, icon_rect, LoadingPlacement::Tab, c(bg));
    } else {
        let glyph = if cfg.terminal {
            tasty_icons::TERMINAL
        } else {
            tasty_icons::HTML
        };
        glyph.image(icon, c(fg)).paint_at(ui, icon_rect);
    }
    // 오버레이 bed가 칸을 덮으므로 점선은 그 뒤에 그린다.
    if cfg.boxes {
        paint_dashed_outline(p, theme, icon_rect, 0.0, c(theme.accent_move()));
    }

    // 오른쪽 끝에서 왼쪽으로 close · busy · move · 표지 순서로 칸을 잡는다.
    let status_gap = theme.tab_status_gap().value();
    let mut right = rect.max.x - theme.spacing_xs.value();
    let mut slot = |w: f32| {
        let r = egui::Rect::from_min_max(
            egui::pos2(right - w, rect.center().y - w * 0.5),
            egui::pos2(right, rect.center().y + w * 0.5),
        );
        right -= w + status_gap;
        r
    };
    let close_rect = slot(theme.tab_close_size().value());
    if cfg.active || cfg.hover {
        let x = theme.icon_glyph_size_xs.value();
        tasty_icons::CLOSE.image(x, c(theme.text_muted())).paint_at(
            ui,
            egui::Rect::from_center_size(close_rect.center(), egui::vec2(x, x)),
        );
    }
    if cfg.busy {
        let dot = slot(theme.tab_dot_size().value());
        p.circle_filled(
            dot.center(),
            dot.width() * 0.5,
            c(theme.status_dot_success()),
        );
    }
    if cfg.mv {
        paint_move_source_glyph(ui, theme, slot(move_source_glyph_size(theme)));
    }
    if let Some((kind, hover)) = cfg.marker {
        let hit = slot(theme.html_script_marker_hit().value());
        let (glyph, ink) = match kind {
            HtmlScriptMarkerKind::Blocked => (tasty_icons::LOCK, theme.html_script_marker_fg()),
            HtmlScriptMarkerKind::Allowed => {
                (tasty_icons::SCRIPT, theme.html_script_marker_allowed_fg())
            }
        };
        if hover && kind == HtmlScriptMarkerKind::Blocked {
            p.rect_filled(
                hit,
                theme.corner_radius_sm.value(),
                theme.html_script_marker_hover_bg().to_egui_premultiplied(),
            );
        }
        let size = theme.html_script_marker_size().value();
        glyph.image(size, c(ink)).paint_at(
            ui,
            egui::Rect::from_center_size(hit.center(), egui::vec2(size, size)),
        );
    }
    // 묶음 왼쪽 끝(마지막 칸 뒤 간격을 되돌린다)에서 tab-gap을 띄운 곳까지가 제목 자리다.
    let label_right = right + status_gap - gap;
    let text_x = icon_rect.max.x + gap;
    let mut job = egui::text::LayoutJob::simple_singleline(
        cfg.label.to_owned(),
        egui::FontId::proportional(theme.font_size_caption.value()),
        c(fg),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: (label_right - text_x).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.fonts(|f| f.layout_job(job));
    let pos = egui::pos2(text_x, rect.center().y - galley.size().y * 0.5);
    ui.painter_at(rect.intersect(ui.clip_rect()))
        .galley(pos, galley, c(fg));
}

/// 시안 `Strip`: 양끝 화살표 사이의 뷰포트에 탭 다섯 개를 `at` 위치로 스크롤해 그린다.
fn strip(ui: &mut egui::Ui, theme: &Theme, cfg: StripCfg) {
    let h = theme.tab_height().value();
    let w = STRIP_W.value().min(ui.available_width());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let ground = if cfg.focused {
        theme.surface_raised()
    } else {
        theme.bg_sidebar()
    };
    ui.painter().rect_filled(rect, 0.0, c(ground));

    let aw = theme.tab_scroll_arrow_width().value();
    let left = egui::Rect::from_min_size(rect.min, egui::vec2(aw, h));
    let right =
        egui::Rect::from_min_size(egui::pos2(rect.max.x - aw, rect.min.y), egui::vec2(aw, h));
    let viewport =
        egui::Rect::from_min_max(egui::pos2(left.max.x, rect.min.y), right.left_bottom());
    let tab_w = theme.tab_width.value();
    let max_scroll = (tab_w * STRIP_TABS.len() as f32 - viewport.width()).max(0.0);
    let scroll = match cfg.at {
        At::Start => 0.0,
        At::Mid => (max_scroll * 0.5).round(),
        At::End => max_scroll,
    };
    let mut tabs = ui.new_child(egui::UiBuilder::new().max_rect(viewport));
    tabs.set_clip_rect(viewport.intersect(ui.clip_rect()));
    for (i, label) in STRIP_TABS.iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            egui::pos2(viewport.min.x - scroll + i as f32 * tab_w, rect.min.y),
            egui::vec2(tab_w, h),
        );
        if cell.intersects(viewport) {
            let cfg = CellCfg {
                label,
                active: i == 1,
                ..CellCfg::BASE
            };
            tab_cell(&tabs, theme, cell, cfg);
        }
    }

    arrow(ui, theme, left, Side::Left, cfg);
    arrow(ui, theme, right, Side::Right, cfg);
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - theme.border_width.value() * 0.5,
        egui::Stroke::new(
            theme.border_width.value(),
            c(theme.separator.to_egui_premultiplied()),
        ),
    );
}

fn strip_card(ui: &mut egui::Ui, theme: &Theme, name: &str, rows: &[(&str, StripCfg)]) {
    themed(ui, theme, name, |ui| {
        for (label, cfg) in rows {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                caption(ui, theme, label);
                strip(ui, theme, *cfg);
            });
        }
    });
}

const SHAPE_ROWS: [(&str, StripCfg); 5] = [
    (
        "focused · left end",
        StripCfg {
            focused: true,
            at: At::Start,
            hover: None,
            mv: None,
        },
    ),
    (
        "focused · middle · hover right",
        StripCfg {
            focused: true,
            at: At::Mid,
            hover: Some(Side::Right),
            mv: None,
        },
    ),
    (
        "focused · right end",
        StripCfg {
            focused: true,
            at: At::End,
            hover: None,
            mv: None,
        },
    ),
    (
        "unfocused · middle",
        StripCfg {
            focused: false,
            at: At::Mid,
            hover: None,
            mv: None,
        },
    ),
    (
        "unfocused · left end · hover left (no fill: disabled)",
        StripCfg {
            focused: false,
            at: At::Start,
            hover: Some(Side::Left),
            mv: None,
        },
    ),
];

/// 스크롤 화살표 모양: chevron 아이콘, 스트립 높이와 같은 정사각 칸, 자체 채움 없음.
pub fn draw_scroll_shape(ui: &mut egui::Ui, theme: &Theme) {
    let mocha = tasty_themes::mocha_fallback();
    let latte = crate::host_shell::latte_theme();
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        strip_card(ui, &mocha, "Mocha", &SHAPE_ROWS);
        strip_card(ui, &latte, "Latte", &SHAPE_ROWS);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("glyph", "chevronLeft / chevronRight · 14"),
            ("cell", "24 × 24, no fill"),
            ("hover", "overlay-hover, enabled side only"),
            ("disabled", "the end reached · no hover · no response"),
            ("scope", "pane strip + horizontal_tab_bar_with_arrows"),
            ("3:1 basis", "flat colour (Mocha 5.65 · Latte 3.65)"),
        ],
        &[
            TokenChip::without_color("tab-scroll-arrow-width", "→ control-height-tab 24"),
            TokenChip::without_color("tab-scroll-arrow-glyph-size", "→ icon-size-sm 14"),
            TokenChip::new(
                "tab-scroll-arrow-fg",
                "→ text-muted",
                c(theme.tab_scroll_arrow_fg()),
            ),
            TokenChip::new(
                "tab-scroll-arrow-fg-disabled",
                "→ text-disabled",
                c(theme.tab_scroll_arrow_fg_disabled()),
            ),
        ],
    );
    spec::dont(
        ui,
        theme,
        "Don't draw arrows with a font glyph. A proportional < at 11px renders thinner than the icon stroke and depends on the UI font.",
    );
}

const MOVE_ROWS: [(&str, StripCfg); 4] = [
    (
        "target out on the right",
        StripCfg {
            focused: true,
            at: At::Start,
            hover: None,
            mv: Some(Side::Right),
        },
    ),
    (
        "target out on the left",
        StripCfg {
            focused: true,
            at: At::End,
            hover: None,
            mv: Some(Side::Left),
        },
    ),
    (
        "target out on the right · hover",
        StripCfg {
            focused: true,
            at: At::Mid,
            hover: Some(Side::Right),
            mv: Some(Side::Right),
        },
    ),
    (
        "unfocused pane · target out on the left",
        StripCfg {
            focused: false,
            at: At::Mid,
            hover: None,
            mv: Some(Side::Left),
        },
    ),
];

/// 이동 대기 대상이 스크롤 밖에 있으면 그쪽 화살표가 move glyph 분홍으로 바뀐다.
pub fn draw_move_cue(ui: &mut egui::Ui, theme: &Theme) {
    let mocha = tasty_themes::mocha_fallback();
    let latte = crate::host_shell::latte_theme();
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        strip_card(ui, &mocha, "Mocha", &MOVE_ROWS);
        strip_card(ui, &latte, "Latte", &MOVE_ROWS);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("where", "the scroll arrow on the target's side"),
            ("colour", "accent-move (same as the glyph)"),
            ("tab · surface target", "same cue"),
            (
                "surface in the ACTIVE tab",
                "no cue — the content-area ring already shows it (2026-10-06)",
            ),
            ("visible", "cell fully inside the viewport"),
            ("input", "none of its own — the arrow's normal step scroll"),
            ("too-small rect", "no substitute"),
            ("maximise", "out of scope — new request later"),
        ],
        &[TokenChip::new(
            "tab-scroll-arrow-move-fg",
            "→ move-source-glyph",
            c(theme.tab_scroll_arrow_move_fg()),
        )],
    );
    spec::note(
        ui,
        theme,
        "Nothing new appears in the strip. The arrow already means \"more tabs this way\"; the pink says the move source is one of them.",
    );
}

const BLOCKED: Option<(HtmlScriptMarkerKind, bool)> = Some((HtmlScriptMarkerKind::Blocked, false));

/// 시안 cluster 예제 여덟 줄. 마지막 줄만 좁은 탭이다.
const CLUSTER_ROWS: [(&str, CellCfg); 8] = [
    (
        "marker only · inactive",
        CellCfg {
            label: "report.html",
            active: false,
            hover: false,
            marker: BLOCKED,
            mv: false,
            busy: false,
            ..CellCfg::BASE
        },
    ),
    (
        "marker only · active",
        CellCfg {
            label: "report.html",
            active: true,
            hover: false,
            marker: BLOCKED,
            mv: false,
            busy: false,
            ..CellCfg::BASE
        },
    ),
    (
        "allowed · active",
        CellCfg {
            label: "report.html",
            active: true,
            hover: false,
            marker: Some((HtmlScriptMarkerKind::Allowed, false)),
            mv: false,
            busy: false,
            ..CellCfg::BASE
        },
    ),
    (
        "marker + busy",
        CellCfg {
            label: "report.html",
            active: false,
            hover: false,
            marker: BLOCKED,
            mv: false,
            busy: true,
            ..CellCfg::BASE
        },
    ),
    (
        "marker + move",
        CellCfg {
            label: "report.html",
            active: false,
            hover: false,
            marker: BLOCKED,
            mv: true,
            busy: false,
            ..CellCfg::BASE
        },
    ),
    (
        "marker + move + busy · hover",
        CellCfg {
            label: "report.html",
            active: false,
            hover: true,
            marker: BLOCKED,
            mv: true,
            busy: true,
            ..CellCfg::BASE
        },
    ),
    (
        "hover on the lock",
        CellCfg {
            label: "report.html",
            active: false,
            hover: true,
            marker: Some((HtmlScriptMarkerKind::Blocked, true)),
            mv: false,
            busy: false,
            ..CellCfg::BASE
        },
    ),
    (
        "narrow tab — label ellipsises",
        CellCfg {
            label: "quarterly-report-final.html",
            active: true,
            hover: false,
            marker: BLOCKED,
            mv: true,
            busy: true,
            ..CellCfg::BASE
        },
    ),
];

fn cluster_card(ui: &mut egui::Ui, theme: &Theme, name: &str) {
    themed(ui, theme, name, |ui| {
        for (i, (label, cfg)) in CLUSTER_ROWS.iter().enumerate() {
            let w = if i + 1 == CLUSTER_ROWS.len() {
                NARROW_TAB_W.value()
            } else {
                theme.tab_width.value()
            };
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                caption(ui, theme, label);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(w, theme.tab_height().value()),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, 0.0, c(theme.bg_sidebar()));
                tab_cell(ui, theme, rect, *cfg);
            });
        }
    });
}

/// 탭 칸 오른쪽의 고정 상태 묶음: 표지 · move · busy · close.
pub fn draw_status_cluster(ui: &mut egui::Ui, theme: &Theme) {
    let mocha = tasty_themes::mocha_fallback();
    let latte = crate::host_shell::latte_theme();
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        cluster_card(ui, &mocha, "Mocha");
        cluster_card(ui, &latte, "Latte");
    });
    spec::meta(
        ui,
        theme,
        &[
            ("order", "label … | marker · move · busy · close"),
            ("cluster gap", "4 · tab-status-gap"),
            ("label → cluster", "8 · tab-gap"),
            ("squeeze", "label ellipsises; cluster fixed"),
            ("lock hit", "16 × 16, hover = overlay-hover"),
            (
                "lock click",
                "re-shows the banner only (no tab switch, no focus change) · mouse only",
            ),
            ("scriptFile", "tooltip only"),
            (
                "tooltip placement",
                "top → bottom → inside the strip (1px border-width tolerance) · none clears → inside the strip (see next spec)",
            ),
        ],
        &[
            TokenChip::without_color("tab-status-gap", "→ space-xs 4"),
            TokenChip::without_color("html-script-marker-hit", "→ size-16"),
            TokenChip::without_color("html-script-marker-hover-bg", "→ overlay-hover"),
            TokenChip::new(
                "html-script-marker-fg",
                "lock",
                c(theme.html_script_marker_fg()),
            ),
        ],
    );
}
