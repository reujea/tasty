//! 러너의 실행·오류·대기 작업 상태를 보여주는 배지.
//! 라벨 글꼴과 전용 간격을 쓰므로 공용 StatusDot 대신 직접 그린다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

use super::Runner;
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 본체 CLI의 러너 시작 명령.
fn resume_cmd() -> &'static str {
    crate::i18n::t("dag.runner.resume_hint_command")
}

/// 헤더 한 줄에 들어가는 힌트 문구 — 알약 옆에 이어 붙인다.
pub const RESUME_HINT: &str = "resume with tasty agent task-run --workspace <N> --action start";

/// 배지 톤 — 오류나 실행 가능한 작업을 남긴 정지 상태를 강조한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Crashed,
    Stalled,
    Normal,
}

fn tone_of(r: &Runner) -> Tone {
    if r.crashed {
        Tone::Crashed
    } else if !r.running && r.ready > 0 {
        Tone::Stalled
    } else {
        Tone::Normal
    }
}

fn text_of(r: &Runner) -> String {
    match tone_of(r) {
        Tone::Crashed => crate::i18n::t_fmt("dag.runner.crashed", &r.ready.to_string()),
        Tone::Stalled => crate::i18n::t_fmt("dag.runner.stalled", &r.ready.to_string()),
        Tone::Normal if r.running => crate::i18n::t_fmt2(
            "dag.runner.running",
            &r.active.to_string(),
            &r.ready.to_string(),
        ),
        Tone::Normal => crate::i18n::t("dag.runner.idle").to_owned(),
    }
}

fn colors(theme: &Theme, r: &Runner) -> (HexColor, HexColor, HexColor, HexColor) {
    // (bg, border, fg, dot)
    match tone_of(r) {
        Tone::Crashed => (
            theme.dag_runner_crashed_bg(),
            theme.dag_runner_crashed_border(),
            theme.dag_runner_crashed_fg(),
            theme.status_dot_danger(),
        ),
        Tone::Stalled => (
            theme.dag_runner_stalled_bg(),
            theme.dag_runner_stalled_border(),
            theme.dag_runner_stalled_fg(),
            theme.status_dot_warning(),
        ),
        Tone::Normal if r.running => (
            theme.dag_runner_bg(),
            theme.dag_runner_border(),
            theme.dag_runner_fg(),
            theme.status_dot_success(),
        ),
        Tone::Normal => (
            theme.dag_runner_bg(),
            theme.dag_runner_border(),
            theme.dag_runner_idle_fg(),
            theme.status_dot_idle(),
        ),
    }
}

/// 알약 폭 — 헤더가 자리를 잡을 때 미리 알아야 한다.
pub fn badge_width(ui: &egui::Ui, theme: &Theme, r: &Runner) -> f32 {
    let font = tasty_ui_widgets::label_font(theme.font_size_caption.value());
    theme.dag_runner_padding_x().value() * 2.0
        + theme.status_dot_size().value()
        + theme.dag_runner_gap().value()
        + super::node::text_width(ui, &text_of(r), &font)
}

/// 알약 하나를 `rect` 에 그린다.
pub fn paint_badge(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, r: &Runner) {
    let (bg, border, fg, dot) = colors(theme, r);
    let radius = theme.dag_runner_radius().value();
    ui.painter().rect_filled(rect, radius, bg.to_egui());
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), border.to_egui()),
        egui::StrokeKind::Inside,
    );
    let pad = theme.dag_runner_padding_x().value();
    let d = theme.status_dot_size().value();
    ui.painter().circle_filled(
        egui::pos2(rect.min.x + pad + d / 2.0, rect.center().y),
        d / 2.0,
        dot.to_egui(),
    );
    ui.painter().text(
        egui::pos2(
            rect.min.x + pad + d + theme.dag_runner_gap().value(),
            rect.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        text_of(r),
        tasty_ui_widgets::label_font(theme.font_size_caption.value()),
        fg.to_egui(),
    );
}

/// 재개 힌트를 붙일 상태인지 — crashed 또는 stalled 일 때만.
pub fn wants_hint(r: &Runner) -> bool {
    tone_of(r) != Tone::Normal
}

/// 알약 + (해당되면) 힌트를 한 줄로 배치한다.
pub fn row(ui: &mut egui::Ui, theme: &Theme, r: &Runner, hint: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let w = badge_width(ui, theme, r);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(w, theme.dag_runner_height().value()),
            egui::Sense::hover(),
        );
        paint_badge(ui, theme, rect, r);
        if hint && wants_hint(r) {
            ui.label(
                egui::RichText::new("resume with")
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            );
            ui.label(
                egui::RichText::new(resume_cmd())
                    .size(theme.font_size_caption.value())
                    .monospace()
                    .color(theme.text_muted().to_egui()),
            );
        }
    });
}

/// `runner` 섹션 Spec — 상태 5 종.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for r in [
            Runner {
                running: true,
                crashed: false,
                ready: 3,
                active: 2,
            },
            Runner {
                running: true,
                crashed: false,
                ready: 0,
                active: 0,
            },
            Runner {
                running: false,
                crashed: false,
                ready: 4,
                active: 0,
            },
            Runner {
                running: false,
                crashed: false,
                ready: 0,
                active: 0,
            },
            Runner {
                running: false,
                crashed: true,
                ready: 2,
                active: 0,
            },
        ] {
            row(ui, theme, &r, true);
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("pill", "22px · radius 2 · 1px border"),
            ("dot", "static status dot in this gallery"),
            ("counts", "mono 11"),
            ("hint", "hidden on a narrow header"),
        ],
        &[
            TokenChip::new(
                "dag-runner-stalled-fg",
                "stopped + ready",
                theme.dag_runner_stalled_fg().to_egui(),
            ),
            TokenChip::new(
                "dag-runner-crashed-fg",
                "crashed",
                theme.dag_runner_crashed_fg().to_egui(),
            ),
            TokenChip::new(
                "dag-runner-idle-fg",
                "stopped, no work",
                theme.dag_runner_idle_fg().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "\"Stopped with no work\" is not a warning — it is the resting state of a finished graph, \
         so it stays muted. Only stopped-with-ready earns the yellow.",
    );
}
