//! 자동 attach 매핑을 연결하지 않았을 때의 안내 — 사이드바 행 표지와 Workspace 배너.
//! 배너와 행 표지는 본체와 같은 `tasty_ui_widgets` 함수로 그리고, 사이드바 행과 터미널 자리만 여기서 흉내 낸다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    AttachRefusalBannerView, RailDot, attach_refusal_avatar_tooltip, attach_refusal_banner,
    attach_refusal_mark, banner_is_narrow, move_source_glyph_size, paint_attach_refusal_chip,
    paint_move_source_chip, paint_move_source_glyph, paint_rail_dot, running_count_column,
    workspace_attention_badges,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// 사이드바 흉내 폭. 디자인은 `--tasty-size-200`을 쓴다.
const SIDEBAR_W: LogicalPx = LogicalPx(200.0);
/// 배너와 터미널 자리를 담는 열의 폭. 디자인은 `--tasty-size-460`을 쓴다.
const CONTENT_W: LogicalPx = LogicalPx(460.0);
/// 배너 아래 터미널 자리의 높이. 디자인은 `--tasty-size-64`를 쓴다.
const TERMINAL_H: LogicalPx = LogicalPx(64.0);
/// 다른 이유 배너의 최대 폭. 디자인은 `--tasty-size-560`을 쓴다.
const OTHER_REASON_MAX_W: LogicalPx = LogicalPx(560.0);
/// 사이드바 흉내 행 높이. 디자인 `RefusalRowG`의 `--tasty-size-28`이다.
const ROW_H: LogicalPx = LogicalPx(28.0);
/// 사이드바 흉내 행 사이 간격. 디자인은 `--tasty-size-1`을 쓴다.
const ROW_GAP: LogicalPx = LogicalPx(1.0);

/// 거절 이유 하나. 대상은 디자인 예제의 값이다.
#[derive(Clone, Copy)]
enum Reason {
    SelfInstance,
    ProfileMissing,
    Unresolved,
}

impl Reason {
    fn target(self) -> &'static str {
        match self {
            Reason::SelfInstance => "127.0.0.1:7420",
            Reason::ProfileMissing => "prod-web",
            Reason::Unresolved => "build-eu:7420",
        }
    }

    fn reason_key(self) -> &'static str {
        match self {
            Reason::SelfInstance => "remote.refusal.self",
            Reason::ProfileMissing => "remote.refusal.profile_missing",
            Reason::Unresolved => "remote.refusal.unresolved",
        }
    }
}

/// 번역문 `remote.refusal.title`의 `{}` 앞뒤.
fn title_parts() -> (&'static str, &'static str) {
    t("remote.refusal.title")
        .split_once("{}")
        .unwrap_or((t("remote.refusal.title"), ""))
}

/// `scope_width`는 배너가 놓인 스코프의 폭이며 본체처럼 이 폭으로 좁은 배치를 판정한다.
fn banner(ui: &mut egui::Ui, theme: &Theme, reason: Reason, scope_width: LogicalPx) {
    let (before, after) = title_parts();
    let body = format!("{} {}", t(reason.reason_key()), t("remote.refusal.hint"));
    attach_refusal_banner(
        ui,
        theme,
        &AttachRefusalBannerView {
            title_before: before,
            target: reason.target(),
            title_after: after,
            body: &body,
            remove: t("remote.refusal.remove"),
            dismiss: t("remote.refusal.dismiss"),
            narrow: banner_is_narrow(scope_width.value(), theme),
        },
    );
}

/// 좁은 배치 Spec 의 거절 표본. `scope_width` 스코프에 놓인 자기 자신 매핑 배너다.
pub(super) fn refused_sample(ui: &mut egui::Ui, theme: &Theme, scope_width: LogicalPx) {
    banner(ui, theme, Reason::SelfInstance, scope_width);
}

/// 거절 표지와 레일 칩의 툴팁. 디자인 예제의 대상과 이유다.
fn refusal_tooltip() -> String {
    format!(
        "{}\n{}",
        t("remote.refusal.title").replacen("{}", Reason::SelfInstance.target(), 1),
        t(Reason::SelfInstance.reason_key())
    )
}

/// 디자인 `RefusalRowG`의 행 끝 표시.
#[derive(Clone, Copy, Default)]
struct RowMarks {
    refused: bool,
    move_source: bool,
    needs_input: usize,
    completion: usize,
}

/// 디자인 `RefusalRowG` — 점 · 이름 · move 글리프 · 거절 표지 · 배지 묶음.
/// 배지 묶음이 가장 오른쪽이고 항목 사이는 행 간격(`space-sm`)이다.
fn row(ui: &mut egui::Ui, theme: &Theme, name: &str, active: bool, marks: RowMarks) {
    let fill = if active {
        theme.surface_active().to_egui()
    } else {
        egui::Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(theme.corner_radius_sm.value())
        .inner_margin(egui::Margin::symmetric(theme.spacing_sm.value() as i8, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ROW_H.value());
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = theme.workspace_running_count_gap().value();
                let dot = theme.status_dot_size().value();
                let (dot_rect, _) = ui.allocate_exact_size(
                    egui::vec2(theme.workspace_dot_slot().value(), dot),
                    egui::Sense::hover(),
                );
                ui.painter().circle_filled(
                    dot_rect.center(),
                    dot * 0.5,
                    theme.status_dot_idle().to_egui(),
                );
                ui.spacing_mut().item_spacing.x = theme.workspace_dot_gap().value();
                running_count_column(ui, theme, 0, dot, theme.text_muted());
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 배지 사이 간격은 badge-group-gap만 쓰도록 묶음 안에서는 행 간격을 끈다.
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        workspace_attention_badges(ui, theme, marks.needs_input, marks.completion);
                    });
                    if marks.refused {
                        attach_refusal_mark(ui, theme, &refusal_tooltip());
                    }
                    if marks.move_source {
                        let size = move_source_glyph_size(theme);
                        let (slot, _) =
                            ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                        paint_move_source_glyph(ui, theme, slot);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let color = if active {
                            theme.text_on_active()
                        } else {
                            theme.text_secondary()
                        };
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(name)
                                    .size(theme.font_size_body.value())
                                    .color(color),
                            )
                            .truncate(),
                        );
                    });
                });
            });
        });
}

/// 디자인 `RefusalRailG` — 머리글자 아바타와 네 모서리 표시.
/// 왼쪽 위 거절 칩 · 오른쪽 위 알림 점 · 왼쪽 아래 move 칩이다. 우선순위 없이 함께 나온다.
fn rail_avatar(
    ui: &mut egui::Ui,
    theme: &Theme,
    ch: &str,
    refused: bool,
    dot: Option<RailDot>,
    move_source: bool,
) {
    // 디자인 `RefusalRailG`의 28 정사각 아바타 — 본체 레일 워크스페이스 높이와 같은 값이다.
    let side = theme.sidebar_collapsed_workspace_height.value();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        theme.corner_radius.value(),
        theme.surface_raised().to_egui(),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        ch,
        egui::FontId::monospace(theme.font_size_body.value()),
        theme.text_secondary_raised().to_egui(),
    );
    let bed: egui::Color32 = theme.bg_sidebar().into();
    if let Some(dot) = dot {
        paint_rail_dot(ui.painter(), theme, rect, dot);
    }
    if move_source {
        paint_move_source_chip(ui, theme, rect, bed);
    }
    if refused {
        paint_attach_refusal_chip(ui, theme, rect, bed);
        attach_refusal_avatar_tooltip(ui, theme, &resp, &refusal_tooltip(), None);
    }
}

/// 한 테마의 접힌 레일과 펼친 행 묶음.
fn rail_panel(ui: &mut egui::Ui, theme: &Theme) {
    egui::Frame::new()
        .fill(theme.bg_app().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                egui::Frame::new()
                    .fill(theme.bg_sidebar().to_egui())
                    .corner_radius(theme.corner_radius.value())
                    .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                            rail_avatar(ui, theme, "T", false, None, false);
                            rail_avatar(ui, theme, "S", true, None, false);
                            rail_avatar(ui, theme, "D", true, Some(RailDot::NeedsInput), true);
                        });
                    });
                egui::Frame::new()
                    .fill(theme.bg_sidebar().to_egui())
                    .corner_radius(theme.corner_radius.value())
                    .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
                    .show(ui, |ui| {
                        let inner = SIDEBAR_W.value() - 2.0 * theme.spacing_xs.value();
                        ui.set_width(inner);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = ROW_GAP.value();
                            row(
                                ui,
                                theme,
                                "tasty-core",
                                false,
                                RowMarks {
                                    completion: 3,
                                    ..Default::default()
                                },
                            );
                            row(
                                ui,
                                theme,
                                "staging-mirror",
                                true,
                                RowMarks {
                                    refused: true,
                                    needs_input: 1,
                                    completion: 5,
                                    ..Default::default()
                                },
                            );
                            row(
                                ui,
                                theme,
                                "data-etl",
                                false,
                                RowMarks {
                                    refused: true,
                                    move_source: true,
                                    needs_input: 2,
                                    ..Default::default()
                                },
                            );
                        });
                    });
            });
        });
}

/// 한 테마의 사이드바 + 배너 + 터미널 자리 묶음.
fn panel(ui: &mut egui::Ui, theme: &Theme) {
    egui::Frame::new()
        .fill(theme.bg_app().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                egui::Frame::new()
                    .fill(theme.bg_sidebar().to_egui())
                    .corner_radius(theme.corner_radius.value())
                    .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
                    .show(ui, |ui| {
                        let inner = SIDEBAR_W.value() - 2.0 * theme.spacing_xs.value();
                        ui.set_width(inner);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = ROW_GAP.value();
                            row(ui, theme, "tasty-core", false, RowMarks::default());
                            row(
                                ui,
                                theme,
                                "staging-mirror",
                                true,
                                RowMarks {
                                    refused: true,
                                    ..Default::default()
                                },
                            );
                            row(ui, theme, "scratch", false, RowMarks::default());
                        });
                    });
                ui.vertical(|ui| {
                    ui.set_width(CONTENT_W.value());
                    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                    banner(ui, theme, Reason::SelfInstance, CONTENT_W);
                    egui::Frame::new()
                        .fill(egui::Color32::from(theme.surface("terminal").focused_bg))
                        .corner_radius(theme.corner_radius.value())
                        .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_height(TERMINAL_H.value() - 2.0 * theme.spacing_sm.value());
                            ui.label(
                                egui::RichText::new("local terminal — still usable")
                                    .monospace()
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                        });
                });
            });
        });
}

/// 시안 "Auto-attach refused": 행 표지와 Workspace 배너를 두 테마로, 다른 이유 두 가지를 아래에 보인다.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = tasty_themes::mocha_fallback();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // 디자인 Stage는 flex-wrap이다. 문서 폭에 두 묶음이 나란히 들어가지 않아 세로로 쌓인다.
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
                    panel(ui, &mocha);
                    panel(ui, &latte);
                });
            });
    });
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                ui.label(
                    egui::RichText::new("other mapping errors — same banner, own reason")
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted_raised().to_egui()),
                );
                for reason in [Reason::ProfileMissing, Reason::Unresolved] {
                    ui.scope(|ui| {
                        ui.set_max_width(OTHER_REASON_MAX_W.value());
                        banner(ui, theme, reason, OTHER_REASON_MAX_W);
                    });
                }
            });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "row mark",
                "alertTriangle 14 · attach-refusal-glyph · after the move glyph, left of the badge group · tooltip = target + reason",
            ),
            (
                "banner scope",
                "Workspace · shown while that workspace is active",
            ),
            ("glyph", "alertTriangle 16 · accent-warning"),
            ("title", "Remote not attached — {target} (target mono)"),
            (
                "body",
                "{reason} Change or remove the mapping for this workspace.",
            ),
            (
                "action",
                "Remove mapping · banner button (secondary sm on banner-button tokens)",
            ),
            ("×", "hides for this activation; the row mark stays"),
            (
                "clears",
                "mapping changed / removed · profile re-check succeeds",
            ),
            ("focus", "never moved — agent-made mappings included"),
            ("toast", "none"),
        ],
        &[
            TokenChip::new(
                "accent-warning",
                "banner glyph",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "attach-refusal-glyph",
                "row mark",
                theme.attach_refusal_glyph().to_egui(),
            ),
            TokenChip::new("banner-bg", "shell", theme.banner_bg().to_egui()),
            TokenChip::new(
                "banner-button-bg",
                "Remove mapping",
                theme.banner_button_bg().to_egui(),
            ),
            TokenChip::without_color("banner-body-font-size", "reason 11"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Strings (en): remote.refusal.title · remote.refusal.self · remote.refusal.profile_missing · \
         remote.refusal.unresolved · remote.refusal.hint · remote.refusal.remove. No new tokens.",
    );
    draw_rail(ui, theme);
}

/// 시안 "Collapsed rail · beside the attention badges": 레일 왼쪽 위 칩과 행 끝 순서를 두 테마로 보인다.
fn draw_rail(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = tasty_themes::mocha_fallback();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing =
                        egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
                    rail_panel(ui, &mocha);
                    rail_panel(ui, &latte);
                });
            });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "rail chip",
                "top-left · 12 · alertTriangle 8 · accent-warning on bg-sidebar",
            ),
            (
                "rail corners",
                "TR attention dot · BR mirror · BL move · TL refusal — no priority",
            ),
            ("row order", "name · move · refusal · badges"),
            ("row gap", "space-sm (row item spacing)"),
            ("tooltip", "same as the row · not clickable"),
        ],
        &[
            TokenChip::new(
                "attach-refusal-glyph",
                "→ accent-warning",
                theme.attach_refusal_glyph().to_egui(),
            ),
            TokenChip::without_color("attach-refusal-chip-size", "→ move-source-chip-size 12"),
            TokenChip::without_color(
                "attach-refusal-chip-glyph-size",
                "→ move-source-chip-glyph-size 8",
            ),
        ],
    );
}
