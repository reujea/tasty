//! 시안 Layouts › Loading overlay의 일곱 예제. 탭·LNB 카드·레일 아바타는 갤러리 kit의 칸 함수로,
//! 서피스는 빈 서피스·검색줄 예제의 부품으로 그린다. 스피너는 본체와 같은 공용 오버레이다.
//! 모든 예제는 같은 기하의 off/on 쌍이며 Mocha·Latte 두 테마를 차례로 보인다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Attention, HtmlScriptMarkerKind, RowState, TabScrollArrowInk, TabScrollArrowSide, Tooltip,
    TooltipPlacement, paint_tab_scroll_arrow,
};

use crate::catalog::components::rail_marks::{self, Avatar};
use crate::catalog::components::sidebar::{self, WsRowSpec};
use crate::catalog::components::tab_bar::{self, CellCfg};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;
use tasty_ui_widgets::tokens::STRUCT_GAP_2;

use super::surface::{Term, term_size, term_surface};

// 시안 예제 틀의 크기(`--tasty-size-*`)다. 제품 치수가 아니다.
const NARROW_TAB_W: LogicalPx = LogicalPx(120.0);
pub(super) const SCALE_TAB_W: LogicalPx = LogicalPx(140.0);
const OVERFLOW_W: LogicalPx = LogicalPx(460.0);

const OVERFLOW_TABS: [&str; 6] = ["server", "build-server", "vim", "logs", "tests", "db"];

fn palettes(theme: &Theme) -> [(&'static str, Theme); 2] {
    [
        (
            "Mocha",
            sidebar::with_zoom(&tasty_themes::mocha_fallback(), theme.ui_zoom),
        ),
        (
            "Latte",
            sidebar::with_zoom(&crate::host_shell::latte_theme(), theme.ui_zoom),
        ),
    ]
}

/// 시안 `Pair`: 라벨 아래 off·on 두 예제를 같은 기하로 나란히 그린다.
pub(super) fn pair(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    mut draw: impl FnMut(&mut egui::Ui, bool),
) {
    spec::wrap_item(ui, |ui| {
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
        tab_bar::caption(ui, th, label);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_md.value();
            for (name, on) in [("off", false), ("on", true)] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
                    tab_bar::caption(ui, th, name);
                    draw(ui, on);
                });
            }
        });
    });
}

/// 시안 `TabBed`: bg-sidebar 위의 탭 한 칸.
pub(super) fn tab_bed(ui: &mut egui::Ui, th: &Theme, width: LogicalPx, cfg: CellCfg) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), th.tab_height().value()),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 0.0, egui::Color32::from(th.bg_sidebar()));
    tab_bar::tab_cell(ui, th, rect, cfg);
}

/// 시안 `LnbBed`: bg-sidebar 위의 워크스페이스 카드 한 장.
pub(super) fn lnb_bed(ui: &mut egui::Ui, th: &Theme, row: WsRowSpec) {
    sidebar::ws_column(ui, th, &[row]);
}

/// 시안 `RailBed`: bg-sidebar 위의 레일 아바타 하나.
pub(super) fn rail_bed(ui: &mut egui::Ui, th: &Theme, avatar: Avatar) {
    let size = rail_marks::avatar_size(th);
    let pad = th.spacing_sm.value();
    let (bed, _) =
        ui.allocate_exact_size(size + egui::Vec2::splat(pad * 2.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(bed, 0.0, egui::Color32::from(th.bg_sidebar()));
    rail_marks::paint_avatar(
        ui,
        th,
        egui::Rect::from_center_size(bed.center(), size),
        avatar,
    );
}

pub(super) fn row(name: &'static str) -> WsRowSpec<'static> {
    WsRowSpec {
        name,
        badge: None,
        state: RowState::Rest,
        running: 1,
        attached: false,
        mirror: false,
        subtitle: None,
        description: None,
        loading: false,
        keycap: None,
        boxes: false,
    }
}

pub(super) fn per_theme(
    ui: &mut egui::Ui,
    theme: &Theme,
    mut add: impl FnMut(&mut egui::Ui, &Theme),
) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (name, th) in palettes(theme) {
            tab_bar::themed(ui, &th, name, |ui| add(ui, &th));
        }
    });
}

pub(super) fn placements(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "Loading overlay — four placements over existing content",
        Some(
            "The overlay is drawn on top of what is already there and adds no tab, slot or \
             title. Dashed = the overlay rect: surface rect · 14 kind-icon slot · 16 dot slot · \
             36 avatar. No hit-test, no focus.",
        ),
    );
    per_theme(ui, theme, |ui, th| {
        pair(ui, th, "surface", |ui, on| {
            let term = Term {
                loading: on,
                boxes: on,
                ..Term::default()
            };
            term_surface(ui, th, term_size(), term);
        });
        pair(ui, th, "tab", |ui, on| {
            let cfg = CellCfg {
                label: "build-server",
                active: true,
                terminal: true,
                loading: on,
                boxes: on,
                ..CellCfg::BASE
            };
            tab_bed(ui, th, th.tab_width, cfg);
        });
        pair(ui, th, "LNB card", |ui, on| {
            let mut r = row("build-farm");
            r.state = RowState::Selected;
            r.loading = on;
            r.boxes = on;
            lnb_bed(ui, th, r);
        });
        pair(ui, th, "rail avatar", |ui, on| {
            let av = Avatar {
                letter: "B",
                active: true,
                loading: on,
                boxes: on,
                ..Avatar::NONE
            };
            rail_bed(ui, th, av);
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "surface",
                "rect = surface · bed 24 bottom-right, inset 8 · spinner 16",
            ),
            (
                "tab",
                "rect = kind-icon slot 14 · bed = cell fill · spinner 14",
            ),
            (
                "LNB card",
                "rect = dot slot 16 × title line · bed = row fill · spinner 14",
            ),
            ("rail", "rect = avatar 36 · no bed · ring 28, 2px"),
            ("mask", "none — no dim, no scrim, nothing outside the box"),
            ("geometry", "unchanged on/off"),
        ],
        &[
            TokenChip::new(
                "loading-overlay-indicator",
                "→ spinner-indicator",
                theme.loading_overlay_indicator().to_egui(),
            ),
            TokenChip::new(
                "loading-overlay-surface-bed-bg",
                "→ surface-raised",
                theme.loading_overlay_surface_bed_bg().to_egui(),
            ),
            TokenChip::new(
                "loading-overlay-surface-bed-border",
                "→ border-default",
                theme.loading_overlay_surface_bed_border().to_egui(),
            ),
            TokenChip::without_color("loading-overlay-tab-size", "→ tab-icon-size 14"),
            TokenChip::without_color("loading-overlay-lnb-size", "→ icon-size-sm 14"),
            TokenChip::without_color("loading-overlay-rail-size", "→ size-28"),
            TokenChip::without_color("loading-overlay-surface-inset", "→ space-sm 8"),
        ],
    );
}

const TAB_CASES: &[(&str, CellCfg)] = &[
    ("inactive", CellCfg::TERMINAL),
    (
        "active",
        CellCfg {
            active: true,
            ..CellCfg::TERMINAL
        },
    ),
    (
        "hover · close shows",
        CellCfg {
            hover: true,
            ..CellCfg::TERMINAL
        },
    ),
    (
        "attention · needs-input",
        CellCfg {
            attention: Some(Attention::NeedsInput),
            ..CellCfg::TERMINAL
        },
    ),
    (
        "attention · completion",
        CellCfg {
            attention: Some(Attention::Completion),
            ..CellCfg::TERMINAL
        },
    ),
    (
        "busy + loading",
        CellCfg {
            active: true,
            busy: true,
            ..CellCfg::TERMINAL
        },
    ),
    (
        "move + HTML marker",
        CellCfg {
            label: "report.html",
            terminal: false,
            mv: true,
            marker: Some((HtmlScriptMarkerKind::Blocked, false)),
            ..CellCfg::TERMINAL
        },
    ),
    (
        "Ctrl held · keycap above",
        CellCfg {
            active: true,
            keycap: Some("2"),
            ..CellCfg::TERMINAL
        },
    ),
];

/// 시안 `OverflowStrip`: 양끝 화살표 사이에 탭 여섯 칸. `loading`의 탭에 스피너를 겹친다.
fn overflow_strip(ui: &mut egui::Ui, th: &Theme, loading: &[usize]) {
    let h = th.tab_height().value();
    let w = OVERFLOW_W.value().min(ui.available_width());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, egui::Color32::from(th.surface_raised()));
    let aw = th.tab_scroll_arrow_width().value();
    let left = egui::Rect::from_min_size(rect.min, egui::vec2(aw, h));
    let right =
        egui::Rect::from_min_size(egui::pos2(rect.max.x - aw, rect.min.y), egui::vec2(aw, h));
    let viewport =
        egui::Rect::from_min_max(egui::pos2(left.max.x, rect.min.y), right.left_bottom());
    let mut tabs = ui.new_child(egui::UiBuilder::new().max_rect(viewport));
    tabs.set_clip_rect(viewport.intersect(ui.clip_rect()));
    let tab_w = th.tab_width.value();
    for (i, label) in OVERFLOW_TABS.iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            egui::pos2(viewport.min.x + tab_w * i as f32, viewport.min.y),
            egui::vec2(tab_w, h),
        );
        let cfg = CellCfg {
            label,
            active: i == 1,
            loading: loading.contains(&i),
            ..CellCfg::TERMINAL
        };
        tab_bar::tab_cell(&tabs, th, cell, cfg);
    }
    paint_tab_scroll_arrow(
        ui,
        th,
        left,
        TabScrollArrowSide::Left,
        TabScrollArrowInk::Disabled,
        false,
    );
    paint_tab_scroll_arrow(
        ui,
        th,
        right,
        TabScrollArrowSide::Right,
        TabScrollArrowInk::Enabled,
        false,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - th.border_width.value() * 0.5,
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

pub(super) fn tabs(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "Tab — the spinner takes the kind-icon slot, nothing else",
        Some(
            "The spinner covers only the 14 kind icon on a bed of the cell's current fill. Label, \
             attention colour, indicator and the right cluster (marker · move · busy · close) \
             stay. The Ctrl switch keycap draws above the spinner. In a narrow tab the label \
             ellipsises first.",
        ),
    );
    per_theme(ui, theme, |ui, th| {
        for (label, cfg) in TAB_CASES {
            pair(ui, th, label, |ui, on| {
                tab_bed(
                    ui,
                    th,
                    th.tab_width,
                    CellCfg {
                        loading: on,
                        ..*cfg
                    },
                );
            });
        }
        pair(ui, th, "narrow 120 · long title", |ui, on| {
            let cfg = CellCfg {
                label: "remote-attach-build-farm-eu",
                active: true,
                busy: true,
                mv: true,
                loading: on,
                ..CellCfg::TERMINAL
            };
            tab_bed(ui, th, NARROW_TAB_W, cfg);
        });
        tab_bar::caption(ui, th, "overflow strip · off");
        overflow_strip(ui, th, &[]);
        tab_bar::caption(ui, th, "overflow strip · on (2 tabs)");
        overflow_strip(ui, th, &[1, 4]);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("covers", "kind icon only"),
            (
                "bed",
                "the cell's resolved fill (tab-bg / surface-hover / tab-bg-active)",
            ),
            (
                "kept",
                "label · attention colour · indicator · marker · move · busy · close",
            ),
            ("busy + loading", "both visible — arc left, green dot right"),
            ("Ctrl keycap", "above the spinner (same slot)"),
            ("off-screen tab", "not drawn · no repaint"),
        ],
        &[TokenChip::without_color(
            "loading-overlay-tab-size",
            "→ tab-icon-size 14",
        )],
    );
}

pub(super) fn lnb(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "LNB card — the spinner takes the dot slot",
        Some(
            "The spinner sits in the card's 16 dot slot over the status dot and attached ring, \
             on a bed of the row's current fill. Title, REMOTE, subtitle, description, badge, \
             accent bar and hover are untouched. The Alt switch keycap draws above it.",
        ),
    );
    per_theme(ui, theme, |ui, th| {
        let idle = WsRowSpec {
            running: 0,
            ..row("build-farm")
        };
        let cases: [(&str, WsRowSpec); 6] = [
            ("inactive", idle),
            (
                "active",
                WsRowSpec {
                    state: RowState::Selected,
                    ..row("build-farm")
                },
            ),
            (
                "hover",
                WsRowSpec {
                    state: RowState::Hover,
                    ..idle
                },
            ),
            (
                "attached ring",
                WsRowSpec {
                    attached: true,
                    ..row("build-farm")
                },
            ),
            (
                "REMOTE · subtitle · description · notif",
                WsRowSpec {
                    mirror: true,
                    subtitle: Some("build-01 · 4 surfaces"),
                    description: Some(
                        "Nightly build farm, EU region. Mirrors the remote host's layout.",
                    ),
                    badge: Some("3"),
                    ..row("build-farm")
                },
            ),
            (
                "Alt held · keycap above",
                WsRowSpec {
                    state: RowState::Selected,
                    keycap: Some("2"),
                    ..row("build-farm")
                },
            ),
        ];
        for (label, spec_row) in cases {
            pair(ui, th, label, |ui, on| {
                lnb_bed(
                    ui,
                    th,
                    WsRowSpec {
                        loading: on,
                        ..spec_row
                    },
                );
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("covers", "status dot + attached ring (dot slot 16)"),
            (
                "bed",
                "bg-sidebar + the row fill (surface-active / overlay-hover)",
            ),
            (
                "kept",
                "title · REMOTE · subtitle · description · badge · accent bar · hover",
            ),
            ("Alt keycap", "above the spinner"),
            (
                "collapsed category",
                "card not drawn · header and rail --- show nothing",
            ),
        ],
        &[
            TokenChip::without_color("loading-overlay-lnb-size", "→ icon-size-sm 14"),
            TokenChip::without_color("workspace-dot-slot", "→ size-16 (rect)"),
        ],
    );
}

pub(super) fn rail(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "Rail avatar — a ring around the initial",
        Some(
            "The spinner is a 28 ring around the initial with no bed. Corner chips (refusal · \
             attention · move · mirror) and the attached ring draw above it; the switch keycap \
             draws above everything.",
        ),
    );
    let base = Avatar {
        letter: "B",
        ..Avatar::NONE
    };
    let cases: [(&str, Avatar); 10] = [
        ("idle", base),
        (
            "active",
            Avatar {
                active: true,
                ..base
            },
        ),
        (
            "hover",
            Avatar {
                hover: true,
                ..base
            },
        ),
        (
            "notif",
            Avatar {
                notif: true,
                ..base
            },
        ),
        ("attached ring", Avatar { ring: true, ..base }),
        (
            "mirror",
            Avatar {
                mirror: true,
                ..base
            },
        ),
        ("move", Avatar { mv: true, ..base }),
        (
            "refused",
            Avatar {
                refused: true,
                ..base
            },
        ),
        (
            "all marks",
            Avatar {
                active: true,
                notif: true,
                ring: true,
                mirror: true,
                mv: true,
                refused: true,
                ..base
            },
        ),
        (
            "keycap above",
            Avatar {
                keycap: Some("2"),
                ..base
            },
        ),
    ];
    per_theme(ui, theme, |ui, th| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::splat(th.spacing_lg.value());
            for (label, av) in cases {
                pair(ui, th, label, |ui, on| {
                    rail_bed(ui, th, Avatar { loading: on, ..av });
                });
            }
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("ring", "28 · 2px · centred on the 36 avatar"),
            ("initial", "kept, inside the ring"),
            ("corner chips", "above the ring (halo cuts it)"),
            ("attached ring", "unchanged, outside"),
            ("collapsed category", "avatar not drawn · nothing on ---"),
        ],
        &[TokenChip::without_color(
            "loading-overlay-rail-size",
            "→ size-28",
        )],
    );
}

pub(super) fn labels(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(ui, theme, "Labels and tooltips", None);
    per_theme(ui, theme, |ui, th| {
        tab_bar::caption(ui, th, "loading tab · hover tooltip");
        let (anchor, _) = ui.allocate_exact_size(
            egui::vec2(th.tab_width.value(), th.tab_height().value()),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(anchor, 0.0, egui::Color32::from(th.bg_sidebar()));
        let cfg = CellCfg {
            label: "build-server",
            active: true,
            loading: true,
            ..CellCfg::TERMINAL
        };
        tab_bar::tab_cell(ui, th, anchor, cfg);
        // 버블이 들어갈 자리를 아래에 비워 둔다. 버블은 고정 위치에 그린다.
        let line = th.font_size_caption.value() * th.line_height_ui;
        let room =
            line * 2.0 + th.spacing_xs.scaled(4.0).value() + th.border_width.scaled(2.0).value();
        ui.add_space(room);
        // 버블은 스크롤과 무관한 전경 Area라, 예제가 화면 밖일 때 그리면 화면 가장자리로 끌려온다.
        let bubble = anchor.with_max_y(anchor.max.y + room);
        if ui.clip_rect().contains_rect(bubble) {
            Tooltip::new(cfg.label)
                .second_line(t("loading.remote_attach"))
                .id_source(("loading_tooltip_specimen", th.is_light))
                .placement(TooltipPlacement::Bottom)
                .show(ui, th, anchor);
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("loading.generic", t("loading.generic")),
            ("loading.remote_attach", t("loading.remote_attach")),
            ("tooltip", "existing tooltip + second line (text-muted)"),
            (
                "a11y",
                "design: role=status, label = same key · not exposed yet (paint-only overlay)",
            ),
        ],
        &[],
    );
    spec::dont(
        ui,
        theme,
        "Don't write the loading label into the tab title or workspace name, and don't add a \
         slot beside the title for it.",
    );
}
