//! 탐색기의 파일 트리와 하단 즐겨찾기 예제. 두 영역은 따로 스크롤한다.
//! 즐겨찾기 높이 계산은 본체 explorer.rs::favorites_pin_height와 같은 식을 사용한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    ButtonVariant, CompactStateGlyph, CompactStateRow, compact_state_row, tree_row,
};

use crate::catalog::icons::{ALERT_TRIANGLE, FOLDER, FOLDER_OPEN, LOCK, STAR, STAR_FILL};
use crate::catalog::spec::{StageVariant, TokenChip, body_column, cluster, meta, note, stage};

/// 시안 `TreeNode` 의 chevron 자리: 펼침 · 접힘 · 없음(leaf).
#[derive(Clone, Copy)]
pub(super) enum Fold {
    Open,
    Closed,
    Leaf,
}
use Fold::{Closed, Leaf, Open};

/// (label, depth, fold, active)
pub(super) type Node = (&'static str, u16, Fold, bool);

/// 짧은 트리(시안 `TREE_SHORT`), 스크롤 없이 상단 영역에 빈 공간을 남긴다.
pub(super) const TREE_SHORT: &[Node] = &[
    ("Home", 0, Open, false),
    ("Downloads", 1, Open, true),
    ("mockup-exports", 2, Leaf, false),
    ("Documents", 1, Closed, false),
    ("Projects", 1, Closed, false),
];

/// 긴 트리(시안 `TREE_LONG`), 상단 영역 스크롤을 유발한다.
const TREE_LONG: &[Node] = &[
    ("Home", 0, Open, false),
    ("Downloads", 1, Open, true),
    ("mockup-exports", 2, Leaf, false),
    ("invoices", 2, Leaf, false),
    ("Documents", 1, Open, false),
    ("contracts", 2, Leaf, false),
    ("notes", 2, Leaf, false),
    ("Projects", 1, Open, false),
    ("tasty", 2, Open, false),
    ("crates", 3, Leaf, false),
    ("docs", 3, Leaf, false),
    ("src", 3, Leaf, false),
    ("target", 3, Leaf, false),
    ("design-system", 2, Leaf, false),
    ("playground", 2, Leaf, false),
    ("Pictures", 1, Open, false),
    ("screenshots", 2, Leaf, false),
    ("wallpapers", 2, Leaf, false),
    ("Music", 1, Closed, false),
    ("Videos", 1, Closed, false),
    (".config", 1, Closed, false),
    (".cache", 1, Closed, false),
];

/// (label, active) — 소수 즐겨찾기(시안 `FAVS_DEFAULT`), 고정 영역 안에 전부 들어간다.
pub(super) const FAVS_FEW: &[(&str, bool)] = &[
    ("tasty", true),
    ("Documents", false),
    ("screenshots", false),
];

/// (label, active) — 다수 즐겨찾기(시안 `FAVS_MANY`, 10개), 고정 영역을 넘겨 자체 스크롤을 유발한다.
const FAVS_MANY: &[(&str, bool)] = &[
    ("tasty", true),
    ("crates", false),
    ("design-system", false),
    ("Documents", false),
    ("mockup-exports", false),
    ("screenshots", false),
    ("wallpapers", false),
    ("invoices", false),
    (".config", false),
    ("playground", false),
];

/// design ExpSidebar width 196.
pub(super) const SIDEBAR_W: LogicalPx = LogicalPx(196.0);
/// 시안 2-region Spec 네 예제의 body 높이(`ExpSidebar height={620}`). pin 240.
const SPLIT_BODY_H: LogicalPx = LogicalPx(620.0);
/// 시안 Favorites populated/empty Spec의 body 높이(`ExpSidebar height={300}`). pin 120.
const FAVORITES_BODY_H: LogicalPx = LogicalPx(300.0);
/// 시안 pin 비교 줄의 40% 구간 예제 body 높이(`[620, 560, 420, 300]`의 560). pin 224.
const PIN_STRIP_UPPER_H: LogicalPx = LogicalPx(560.0);
/// 시안 pin 비교 줄의 40% 구간 예제 body 높이(`[620, 560, 420, 300]`의 420). pin 168.
const PIN_STRIP_LOWER_H: LogicalPx = LogicalPx(420.0);
/// 시안 2-region Spec 아래 pin 비교 줄의 body 높이 순서. pin 240·224·168·120.
const PIN_STRIP_BODY_H: [LogicalPx; 4] = [
    SPLIT_BODY_H,
    PIN_STRIP_UPPER_H,
    PIN_STRIP_LOWER_H,
    FAVORITES_BODY_H,
];
/// 시안 Short cell Spec 의 body 높이(`[300, 240, 200, 84]`). 84 는 칸이 하한(`explorer_min_height`)에
/// 닿았을 때다.
const SHORT_STRIP_BODY_H: [LogicalPx; 4] = [
    FAVORITES_BODY_H,
    LogicalPx(240.0),
    LogicalPx(200.0),
    LogicalPx(84.0),
];
/// 시안 Short cell 라벨이 칸 하한을 적는 body 높이(`h === 84`).
const SHORT_FLOOR_BODY_H: LogicalPx = SHORT_STRIP_BODY_H[3];
/// 시안 compact 무대의 내용 높이(칸이 하한에 닿았을 때 84)와 줄 최대 폭(`maxWidth: 440`).
const COMPACT_BODY_H: LogicalPx = LogicalPx(84.0);
const COMPACT_ROW_MAX_W: LogicalPx = LogicalPx(440.0);

/// 시안 "Sidebar layout — Favorites PINNED to the bottom (2-region split)".
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    cluster(
        ui,
        theme,
        "files — long tree scrolls, favorites stays pinned",
        |ui| {
            stage(ui, theme, StageVariant::Tight, |ui| {
                panel(ui, theme, SPLIT_BODY_H, |ui| {
                    two_region(ui, theme, "a", SPLIT_BODY_H, TREE_LONG, FAVS_FEW);
                });
            });
        },
    );

    cluster(
        ui,
        theme,
        "files — short tree leaves blank space above pin",
        |ui| {
            stage(ui, theme, StageVariant::Tight, |ui| {
                panel(ui, theme, SPLIT_BODY_H, |ui| {
                    two_region(ui, theme, "b", SPLIT_BODY_H, TREE_SHORT, FAVS_FEW);
                });
            });
        },
    );

    cluster(ui, theme, "favorites — empty state", |ui| {
        stage(ui, theme, StageVariant::Tight, |ui| {
            panel(ui, theme, SPLIT_BODY_H, |ui| {
                two_region(ui, theme, "c", SPLIT_BODY_H, TREE_LONG, &[]);
            });
        });
    });

    cluster(
        ui,
        theme,
        "favorites — own scroll (many favorites)",
        |ui| {
            stage(ui, theme, StageVariant::Tight, |ui| {
                panel(ui, theme, SPLIT_BODY_H, |ui| {
                    two_region(ui, theme, "d", SPLIT_BODY_H, TREE_LONG, FAVS_MANY);
                });
            });
        },
    );

    // 시안 비교 줄: 긴 트리와 기본 즐겨찾기로 body 높이마다 pin 높이를 보여 준다.
    body_strip(
        ui,
        theme,
        "pin height by body height",
        "pin",
        &PIN_STRIP_BODY_H,
        pin_strip_label,
    );
    meta(
        ui,
        theme,
        &[
            ("structure", "2 regions · independent scroll state"),
            (
                "Files region",
                "flex 1 · own scroll · caption pinned at its top",
            ),
            (
                "Favorites region",
                "fixed height, bottom-pinned · explorer-favorites-pin-height",
            ),
            (
                "pin height",
                "body ≥ 600 → 240 · else 40% (4px-snapped) · min 120",
            ),
            (
                "boundary",
                "1px explorer-split-border at a fixed coordinate",
            ),
            ("sidebar width", "196 unchanged · explorer-sidebar-width"),
            (
                "resize",
                "recomputed from the live body height — no drag handle",
            ),
            (
                "row visuals",
                "unchanged (tree row · star row · empty state)",
            ),
        ],
        &[
            TokenChip::without_color("explorer-sidebar-width", "196 column"),
            TokenChip::without_color("explorer-favorites-pin-height", "pinned region height"),
            TokenChip::without_color("explorer-favorites-pin-threshold", "small-surface switch"),
            TokenChip::without_color("explorer-favorites-pin-min-height", "lower clamp"),
            TokenChip::new(
                "explorer-split-border",
                "fixed boundary line",
                theme.explorer_split_border().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "bg-sidebar",
                "both regions' fill",
                egui::Color32::from(theme.bg_sidebar()),
            ),
            TokenChip::new(
                "accent-warning",
                "filled star",
                egui::Color32::from(theme.accent_warning()),
            ),
        ],
    );

    note(
        ui,
        theme,
        "Favorites is pinned to a computed fixed height at the sidebar bottom so it stays \
         visible regardless of how long the Files tree grows — the two regions scroll \
         independently. Below the 600px body-height threshold the pin height shrinks to 40% \
         of the body (4px-grid snapped, 120px floor) instead of the 240px default; the switch \
         is a hard cutover, not an interpolation. The split border sits at a fixed coordinate \
         above the Favorites region — a short tree leaves blank background above it rather \
         than pushing it down or centering the tree.",
    );
    note(
        ui,
        theme,
        "The threshold basis is the sidebar body height (the split container itself), not the \
         whole explorer tab. The pinned region uses the shared hover-revealed scrollbar — no \
         always-on bar. The boundary is the same 1px separator as before — no shadow, no tint; \
         \"pinned\" is communicated by behaviour (the line never moves, each side scrolls \
         alone), not by extra decoration.",
    );
}

/// 시안 "Short cell — Favorites drops below 240, the cell stops at 180".
pub fn draw_short_cell(ui: &mut egui::Ui, theme: &Theme) {
    // 시안 Short cell: 240 미만에서 Favorites 가 빠지고 84 는 칸 하한에 닿은 본문이다.
    let floor = theme.explorer_min_height().value();
    body_strip(
        ui,
        theme,
        "short cell — favorites drops below 240",
        "short",
        &SHORT_STRIP_BODY_H,
        |body_h| short_strip_label(theme, body_h),
    );
    cluster(
        ui,
        theme,
        &format!(
            "content body {} (cell at {floor}) → compact row",
            COMPACT_BODY_H.value()
        ),
        |ui| compact_rows(ui, theme),
    );
    let cell_floor = format!(
        "{floor} · explorer-min-height · split drag stops here · tabs 28 + toolbar 44 + header 28 + 2×28 + status 24"
    );
    let sibling = theme.split_sibling_min_height().value();
    let split = format!(
        "explorer keeps {floor}, sibling takes the rest (≥ {sibling} · split-sibling-min-height); \
         refused if {sibling} can't be kept, if two explorer cells can't both keep {floor}, or if \
         an explorer under {floor} would shrink further"
    );

    let scope = format!(
        "floor = explorer cells only · other kinds have no floor (0) during drags and resizes; \
         {sibling} guards only a new split"
    );
    meta(
        ui,
        theme,
        &[
            (
                "hide Favorites",
                "body < 240 · explorer-favorites-hide-below",
            ),
            ("Files only", "caption + tree, own scroll, full body"),
            ("return", "body ≥ 240 → pin ladder as before (120 floor)"),
            ("cell floor", &cell_floor),
            ("split", &split),
            (
                "refusal",
                "user → info Toast “Not enough room to split this pane” · agent → split refused: not enough room",
            ),
            ("window resize", "floor not held"),
            (
                "compact state",
                "content body < 120 · explorer-state-compact-below · glyph · title · buttons on one row, reason in tooltip",
            ),
            ("scope", &scope),
            ("sidebar", "never hidden as a whole"),
        ],
        &[
            TokenChip::without_color("explorer-favorites-hide-below", "→ size-240"),
            TokenChip::without_color("explorer-min-height", "→ size-180 (was 160)"),
            TokenChip::without_color("explorer-state-compact-below", "→ size-120"),
            TokenChip::without_color("split-sibling-min-height", "→ size-56 (2026-10-09)"),
            TokenChip::without_color("explorer-favorites-pin-min-height", "120 floor (unchanged)"),
        ],
    );
    note(
        ui,
        theme,
        &format!(
            "Short cell: below a 240px body (the 120 Favorites floor + 120 for Files) the \
             Favorites region is not drawn and Files takes the whole body; it comes back at 240. \
             The explorer cell itself stops at {floor} while a split is dragged: internal tabs \
             28 · toolbar 44 · Detail header 28 · two rows 56 · status line 24. The explorer's \
             internal layout does not change at the floor, so the status line keeps its place \
             and Detail shows two rows. A split (tasty split or the split menu) that would leave \
             an explorer cell under the floor places the divider so the explorer keeps {floor} \
             and the sibling takes the rest. Only explorer cells have a floor; a sibling of \
             another kind has none of its own, but a new split never leaves it under {sibling} \
             (pane tab strip 28 + one 28 row). The split is refused when {sibling} can't be \
             kept, when two explorer cells can't both keep {floor}, or when an explorer cell \
             already under {floor} would be split further: a user split shows the info Toast \
             “Not enough room to split this pane”, an agent split gets split refused: not \
             enough room. The check uses the largest view showing the workspace; with no window \
             showing it, the floor is skipped. A window resize does not hold the floor. The \
             sidebar is never hidden as a whole."
        ),
    );
    note(
        ui,
        theme,
        "The Favorites height rule already reads tokens (explorer-favorites-pin-height 240, \
         -pin-threshold 600, -pin-ratio 0.4, -pin-min-height 120). The three dimensions scale \
         with the UI zoom like the other explorer tokens; the ratio is unitless and does not.",
    );
}

/// 시안 "Sidebar Favorites — populated vs. empty state".
pub fn draw_favorites(ui: &mut egui::Ui, theme: &Theme) {
    cluster(ui, theme, "with favorites", |ui| {
        stage(ui, theme, StageVariant::Tight, |ui| {
            panel(ui, theme, FAVORITES_BODY_H, |ui| {
                two_region(ui, theme, "e", FAVORITES_BODY_H, TREE_SHORT, FAVS_FEW);
            });
        });
    });

    cluster(ui, theme, "empty — caption persists", |ui| {
        stage(ui, theme, StageVariant::Tight, |ui| {
            panel(ui, theme, FAVORITES_BODY_H, |ui| {
                two_region(ui, theme, "f", FAVORITES_BODY_H, TREE_SHORT, &[]);
            });
        });
    });

    meta(
        ui,
        theme,
        &[
            ("caption", "always shown — micro caps, text-muted"),
            (
                "empty line",
                "outline star + \"No favorites yet\" · text-muted",
            ),
            ("hint", "1 line · text-placeholder"),
            ("align", "left, inside 196px column"),
            ("condition", "was: hide section at 0 → now: always show"),
        ],
        &[
            TokenChip::new(
                "text-muted",
                "caption + empty line",
                egui::Color32::from(theme.text_muted()),
            ),
            TokenChip::new(
                "text-placeholder",
                "hint line",
                egui::Color32::from(theme.text_placeholder()),
            ),
            TokenChip::new(
                "accent-warning",
                "filled star (populated)",
                egui::Color32::from(theme.accent_warning()),
            ),
        ],
    );
}

/// body 높이마다 사이드바 하나를 위쪽에 맞춰 늘어놓는 비교 줄. 네 예제가 창 끝을 넘지 않고
/// 줄을 바꾸도록 본문 컬럼 폭 안에 둔다.
/// 시안 Short cell 의 compact 줄 세 개(읽기 오류 · 권한 거부 · 빈 폴더). 본체와 같은 공용 함수가 그린다.
fn compact_rows(ui: &mut egui::Ui, theme: &Theme) {
    let muted = theme.text_muted().to_egui();
    let error = theme.explorer_error_fg().to_egui();
    let warning = theme.accent_warning().to_egui();
    let rows = [
        CompactStateRow {
            glyph: CompactStateGlyph::Icon(ALERT_TRIANGLE),
            glyph_color: error,
            title: crate::i18n::t("explorer.state.read_error"),
            title_color: error,
            tooltip: Some("No such file or directory (os error 2)"),
            actions: &[
                ("Retry", ButtonVariant::Secondary),
                ("Go up", ButtonVariant::Ghost),
            ],
        },
        CompactStateRow {
            glyph: CompactStateGlyph::Icon(LOCK),
            glyph_color: warning,
            title: "Permission denied",
            title_color: warning,
            tooltip: Some(crate::i18n::t("explorer.state.no_permission_sub")),
            actions: &[],
        },
        CompactStateRow {
            glyph: CompactStateGlyph::Icon(FOLDER_OPEN),
            glyph_color: muted,
            title: crate::i18n::t("explorer.state.empty"),
            title_color: theme.text_secondary().to_egui(),
            tooltip: None,
            actions: &[],
        },
    ];
    stage(ui, theme, StageVariant::Tight, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            let w = COMPACT_ROW_MAX_W.value().min(ui.available_width());
            for (i, row) in rows.iter().enumerate() {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(w, COMPACT_BODY_H.value()),
                    egui::Sense::hover(),
                );
                let radius = theme.corner_radius.value();
                ui.painter()
                    .rect_filled(rect, radius, theme.bg_panel().to_egui());
                ui.painter().rect_stroke(
                    rect,
                    radius,
                    egui::Stroke::new(
                        theme.border_width.value(),
                        theme.separator.to_egui_premultiplied(),
                    ),
                    egui::StrokeKind::Inside,
                );
                ui.push_id(i, |ui| compact_state_row(ui, theme, rect, row));
            }
        });
    });
}

fn body_strip(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    salt: &str,
    bodies: &[LogicalPx],
    label: impl Fn(LogicalPx) -> String,
) {
    body_column(ui, |ui| {
        cluster(ui, theme, title, |ui| {
            // 시안처럼 높이가 다른 예제를 위쪽에 맞춘다. 크기를 먼저 알려야 폭을 넘는 예제가
            // 다음 줄로 간다.
            let wrap_top = egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true);
            ui.with_layout(wrap_top, |ui| {
                for (i, &body_h) in bodies.iter().enumerate() {
                    let size = egui::vec2(SIDEBAR_W.value(), body_h.value());
                    ui.allocate_ui_with_layout(
                        size,
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                            ui.label(
                                egui::RichText::new(label(body_h))
                                    .font(egui::FontId::monospace(theme.font_size_micro.value()))
                                    .color(egui::Color32::from(theme.text_muted())),
                            );
                            stage(ui, theme, StageVariant::Tight, |ui| {
                                panel(ui, theme, body_h, |ui| {
                                    two_region(
                                        ui,
                                        theme,
                                        &format!("{salt}{i}"),
                                        body_h,
                                        TREE_LONG,
                                        FAVS_FEW,
                                    );
                                });
                            });
                        },
                    );
                }
            });
        });
    });
}

/// 데모 사이드바 컨테이너 — 배경 + 보더 + 고정 폭/높이(`body_h`).
fn panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    body_h: LogicalPx,
    contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(egui::Color32::from(theme.bg_sidebar()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ))
        .show(ui, |ui| {
            ui.set_width(SIDEBAR_W.value());
            ui.set_height(body_h.value());
            ui.spacing_mut().item_spacing.y = 0.0;
            contents(ui);
        });
}

/// 2-region 분할 — 본체 `explorer.rs::sidebar()` 구조 전사: 상단 Files(flex+스크롤),
/// 고정 좌표 구분선, 하단 Favorites(고정 높이+스크롤).
pub(super) fn two_region(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    body_h: LogicalPx,
    tree: &[Node],
    favs: &[(&str, bool)],
) {
    // 예제마다 같은 라벨을 쓰므로 위젯 ID의 범위를 나눈다.
    ui.push_id(id_salt, |ui| {
        two_region_inner(ui, theme, body_h, tree, favs)
    });
}

fn two_region_inner(
    ui: &mut egui::Ui,
    theme: &Theme,
    body_h: LogicalPx,
    tree: &[Node],
    favs: &[(&str, bool)],
) {
    // 시안 Short cell: body 가 explorer-favorites-hide-below 보다 낮으면 Files 만 그린다.
    let fav_h =
        (body_h >= theme.explorer_favorites_hide_below()).then(|| favorites_pin_height(body_h));
    let files_h = fav_h.map_or(body_h, |h| {
        (body_h - h - theme.border_width).max(LogicalPx(0.0))
    });

    ui.allocate_ui_with_layout(
        egui::vec2(SIDEBAR_W.value(), files_h.value()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            caption(ui, theme, "Files");
            egui::ScrollArea::vertical()
                .id_salt("files")
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    for (i, &(label, depth, fold, active)) in tree.iter().enumerate() {
                        ui.push_id(i, |ui| {
                            tree_row(
                                ui,
                                theme,
                                depth,
                                !matches!(fold, Leaf),
                                matches!(fold, Open),
                                Some(&|ui, rect, _c| {
                                    FOLDER
                                        .image(
                                            rect.height(),
                                            egui::Color32::from(theme.text_muted()),
                                        )
                                        .paint_at(ui, rect)
                                }),
                                label,
                                None,
                                active,
                            )
                        });
                    }
                });
        },
    );

    let Some(fav_h) = fav_h else {
        return;
    };
    section_separator(ui, theme);

    ui.allocate_ui_with_layout(
        egui::vec2(SIDEBAR_W.value(), fav_h.value()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            caption(ui, theme, "Favorites");
            egui::ScrollArea::vertical()
                .id_salt("favorites")
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    if favs.is_empty() {
                        favorites_empty(ui, theme);
                    } else {
                        for (i, (label, active)) in favs.iter().enumerate() {
                            ui.push_id(i, |ui| fav_row(ui, theme, label, *active));
                        }
                    }
                });
        },
    );
}

/// design `FAV_PIN` — `favPinHeight`가 쓰는 기본 높이·문턱·비율·하한.
const PIN_BASE: LogicalPx = LogicalPx(240.0);
const PIN_THRESHOLD: LogicalPx = LogicalPx(600.0);
const PIN_RATIO: f32 = 0.4;
const PIN_MIN: LogicalPx = LogicalPx(120.0);

/// 시안 비교 줄 라벨: `body {h} → pin {pin}` 뒤에 계산 구간을 붙인다.
fn pin_strip_label(body_h: LogicalPx) -> String {
    let pin = favorites_pin_height(body_h);
    let kind = if body_h >= PIN_THRESHOLD {
        format!(" ({} flat)", PIN_BASE.value())
    } else if pin <= PIN_MIN {
        " (min clamp)".to_string()
    } else {
        " (40%)".to_string()
    };
    format!("body {} → pin {}{kind}", body_h.value(), pin.value())
}

/// 시안 Short cell 라벨: `body {h} → pin {pin}` 또는 `Files only`, 90 이면 칸 하한을 덧붙인다.
fn short_strip_label(theme: &Theme, body_h: LogicalPx) -> String {
    let shown = if body_h >= theme.explorer_favorites_hide_below() {
        format!("pin {}", favorites_pin_height(body_h).value())
    } else {
        "Files only".to_string()
    };
    let floor = if body_h == SHORT_FLOOR_BODY_H {
        format!(
            " (cell at its {} floor)",
            theme.explorer_min_height().value()
        )
    } else {
        String::new()
    };
    format!("body {} → {shown}{floor}", body_h.value())
}

/// design `favPinHeight` 전사 — 본체 `explorer/favorites_pin.rs` 와 동일 공식.
fn favorites_pin_height(body_h: LogicalPx) -> LogicalPx {
    if body_h <= LogicalPx(0.0) || body_h >= PIN_THRESHOLD {
        return PIN_BASE;
    }
    (LogicalPx((body_h * PIN_RATIO / 4.0).value().round()) * 4.0).max(PIN_MIN)
}

fn fav_row(ui: &mut egui::Ui, theme: &Theme, label: &str, active: bool) {
    let star_color = egui::Color32::from(theme.accent_warning());
    tree_row(
        ui,
        theme,
        0,
        false,
        false,
        Some(&|ui, rect, _c| {
            STAR_FILL
                .image(rect.height(), star_color)
                .paint_at(ui, rect)
        }),
        label,
        None,
        active,
    );
}

fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add_space(theme.spacing_xs.value());
    ui.horizontal(|ui| {
        ui.add_space(theme.spacing_sm.value());
        ui.label(
            egui::RichText::new(text.to_uppercase())
                .font(tasty_ui_widgets::label_font(theme.font_size_micro.value()))
                .color(egui::Color32::from(theme.text_muted())),
        );
    });
    ui.add_space(theme.spacing_xs.value());
}

fn section_separator(ui: &mut egui::Ui, theme: &Theme) {
    let (sep, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        sep.x_range(),
        sep.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

fn favorites_empty(ui: &mut egui::Ui, theme: &Theme) {
    let inset = theme.spacing_sm.value();
    ui.add_space(theme.spacing_xs.value());
    ui.horizontal(|ui| {
        ui.add_space(inset);
        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
        let sz = theme.icon_glyph_size_sm.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::hover());
        // 즐겨찾기 별 아이콘 톤. 대응 토큰 없음 — 본체와 같은 값을 여기 다시 적는다.
        const FAV_STAR_ICON_OPACITY: f32 = 0.55;
        STAR.image(
            sz,
            egui::Color32::from(theme.text_muted()).gamma_multiply(FAV_STAR_ICON_OPACITY),
        )
        .paint_at(ui, r);
        ui.label(
            egui::RichText::new("No favorites yet")
                .size(theme.font_size_caption.value())
                .color(egui::Color32::from(theme.text_muted())),
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.add_space(inset);
        ui.spacing_mut().item_spacing.x = 0.0;
        let micro = theme.font_size_caption.value();
        ui.label(
            egui::RichText::new("Right-click a folder → ")
                .size(micro)
                .color(egui::Color32::from(theme.text_placeholder())),
        );
        ui.label(
            egui::RichText::new("Add to favorites")
                .size(micro)
                .color(egui::Color32::from(theme.text_muted())),
        );
        ui.label(
            egui::RichText::new(".")
                .size(micro)
                .color(egui::Color32::from(theme.text_placeholder())),
        );
    });
    ui.add_space(inset);
}
