//! 복원 실패·저장 멈춤 비포커스 안내(RestoreNotice)와 창 오른쪽 위 안내 열의 예제.
//!
//! 안내 본체는 본체와 같은 `tasty_ui_widgets::restore_notice`를 호출하고, 열 위치는
//! `notice_column_rect`로 정한다. 창 모형은 작업 영역과 오른쪽 위 pane의 탭 바만 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{RestoreNoticeView, notice_column_rect, restore_notice};

use crate::catalog::popup_frame::TITLE_BAR_HEIGHT;
use crate::catalog::spec::{self, StageVariant, TokenChip};

// 대응 Theme 토큰이 없는 창 모형(액자) 전용 치수. 안내가 그 안에서 놓이는 모습만 보인다.
/// 흐름·저장 멈춤 창 모형의 폭.
const WINDOW_W: LogicalPx = LogicalPx(560.0);
/// 흐름 창 모형의 높이.
const FLOW_WINDOW_H: LogicalPx = LogicalPx(340.0);
/// 저장 멈춤 창 모형의 높이. 본문이 한 줄 길다.
const HALT_WINDOW_H: LogicalPx = LogicalPx(360.0);
/// 좁은 창 모형의 폭. 작업 영역 − 2 × inset 이 열 폭보다 좁아지는 경우를 보인다.
const NARROW_WINDOW_W: LogicalPx = LogicalPx(276.0);
/// 좁은 창 모형의 높이. 긴 경로가 여러 줄로 감긴다.
const NARROW_WINDOW_H: LogicalPx = LogicalPx(440.0);

const SHORT_PATH: &str = "~/.tasty/structure/journal.db";
const LONG_PATH: &str = "/Users/someone/Library/Application Support/dev.zilhak.tasty/profiles/work-laptop-2026/structure/journal.db";

#[derive(Clone, Copy)]
enum Variant {
    Restore,
    Halt,
}

#[derive(Clone, Copy)]
enum Cause {
    Denied,
    Corrupt,
    Write,
}

fn view(
    key: &str,
    variant: Variant,
    cause: Cause,
    path: &'static str,
) -> RestoreNoticeView<'static> {
    let t = crate::i18n::t;
    let (title, body) = match variant {
        Variant::Restore => (
            t("notice.restore_failed_title"),
            t("notice.restore_failed_body"),
        ),
        Variant::Halt => (t("notice.save_halted_title"), t("notice.save_halted_body")),
    };
    RestoreNoticeView {
        id_salt: egui::Id::new("gallery_restore_notice").with(key),
        title,
        close_label: t("notice.close"),
        cause: match cause {
            Cause::Denied => t("notice.cause_read_denied"),
            Cause::Corrupt => t("notice.cause_corrupt"),
            Cause::Write => t("notice.cause_write_failed"),
        },
        label: t("notice.saved_layout_label"),
        path,
        copy_label: t("notice.copy_path"),
        body,
        continue_label: t("notice.continue"),
        title_bar_height: TITLE_BAR_HEIGHT,
    }
}

/// 창 모형 하나. 탭 바 아래 작업 영역의 오른쪽 위 열에 안내를 건다. `notice`가 없으면 빈 창이다.
fn window(
    ui: &mut egui::Ui,
    theme: &Theme,
    size: egui::Vec2,
    notice: Option<RestoreNoticeView<'_>>,
) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme.corner_radius.value(), theme.bg_app().to_egui());
    let tab_bar = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width(), theme.tab_height().value()),
    );
    painter.rect_filled(tab_bar, 0.0, theme.bg_sidebar().to_egui());
    painter.rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    let Some(notice) = notice else { return };
    let column = notice_column_rect(theme, rect, tab_bar.max.y);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(column)
            .layout(egui::Layout::top_down(egui::Align::Max)),
    );
    child.set_clip_rect(rect);
    let output = restore_notice(&mut child, theme, &notice, column.width());
    if let Some(error) = output.glyph_error {
        tracing::warn!("gallery restore notice glyph failed to load: {error}");
    }
}

/// 흐름 1: 기본 workspace로 시작 → 안내(터미널이 포커스 유지) → Continue/× 뒤 빈 창.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let size = egui::vec2(WINDOW_W.value(), FLOW_WINDOW_H.value());
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, th) in [("Mocha", theme), ("Latte", &latte)] {
            spec::cluster(
                ui,
                th,
                &format!("{label} · 1 · default workspace opens"),
                |ui| {
                    window(ui, th, size, None);
                },
            );
            spec::cluster(
                ui,
                th,
                &format!("{label} · 2 · notice, terminal keeps focus"),
                |ui| {
                    window(
                        ui,
                        th,
                        size,
                        Some(view(
                            &format!("{label}-flow"),
                            Variant::Restore,
                            Cause::Denied,
                            SHORT_PATH,
                        )),
                    );
                },
            );
            spec::cluster(
                ui,
                th,
                &format!("{label} · 3 · Continue / × → notice gone"),
                |ui| {
                    window(ui, th, size, None);
                },
            );
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "place",
                "notice column · work-area top-right under the top-right pane's tab bar · inset 8 · width 360 (max = work area − 16)",
            ),
            (
                "frame",
                "popup title bar (title centred · ×) · bg-panel · 1px border-frame · shadow-popover · no scrim",
            ),
            (
                "body",
                "pad 12 · gap 8 · alertCircle 16 + cause · font-label caps label · path box · explanation term-sm · Continue secondary sm right",
            ),
            (
                "focus",
                "never on appear · Enter / Esc outside not captured · Esc on a control inside = Continue",
            ),
            (
                "dismiss",
                "× = Continue · never quits · saved layout untouched · error stays in Notifications",
            ),
        ],
        &[
            TokenChip::without_color("restore-notice-width", "→ notice-column-width → size-360"),
            TokenChip::new(
                "restore-notice-glyph",
                "→ notify-error",
                theme.restore_notice_glyph().to_egui(),
            ),
            TokenChip::without_color("restore-notice-padding", "→ space-md 12"),
            TokenChip::without_color("restore-notice-gap", "→ space-sm 8"),
            TokenChip::new(
                "restore-notice-body-fg",
                "→ text-secondary",
                theme.restore_notice_body_fg().to_egui(),
            ),
            TokenChip::new(
                "restore-notice-label-fg",
                "→ text-muted",
                theme.restore_notice_label_fg().to_egui(),
            ),
            TokenChip::new(
                "restore-notice-path-bg",
                "→ bg-sidebar",
                theme.restore_notice_path_bg().to_egui(),
            ),
            TokenChip::new(
                "restore-notice-path-border",
                "→ border-default",
                theme.restore_notice_path_border().to_egui(),
            ),
            TokenChip::new(
                "restore-notice-path-fg",
                "→ text-primary",
                theme.restore_notice_path_fg().to_egui(),
            ),
        ],
    );
}

/// 좁은 창의 긴 경로, 저장 멈춤 변형.
pub fn draw_variants(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, th) in [("Mocha", theme), ("Latte", &latte)] {
            spec::cluster(
                ui,
                th,
                &format!("{label} · narrow window · long path · corrupt"),
                |ui| {
                    window(
                        ui,
                        th,
                        egui::vec2(NARROW_WINDOW_W.value(), NARROW_WINDOW_H.value()),
                        Some(view(
                            &format!("{label}-narrow"),
                            Variant::Restore,
                            Cause::Corrupt,
                            LONG_PATH,
                        )),
                    );
                },
            );
            spec::cluster(ui, th, &format!("{label} · running · save halt"), |ui| {
                window(
                    ui,
                    th,
                    egui::vec2(WINDOW_W.value(), HALT_WINDOW_H.value()),
                    Some(view(
                        &format!("{label}-halt"),
                        Variant::Halt,
                        Cause::Write,
                        SHORT_PATH,
                    )),
                );
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "narrow",
                "width = work area − 2 × 8 · path wraps anywhere · Continue stays inside",
            ),
            (
                "halt",
                "same frame · cause write · says output and input continue",
            ),
        ],
        &[
            TokenChip::without_color("notice-column-inset", "→ space-sm 8"),
            TokenChip::without_color("notice-column-gap", "→ space-sm 8"),
            TokenChip::without_color("notice-column-width", "→ size-360"),
        ],
    );
}
