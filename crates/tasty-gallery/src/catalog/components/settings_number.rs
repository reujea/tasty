//! 숫자 입력의 기본·범위 초과·비활성 상태 예제.
//! 편집 중에는 값을 유지하고 확정할 때 범위를 제한한다. 본체와 같은 공용 Input을 쓴다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::Input;

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 디자인 specimen 의 행 폭(`width: 420`).
const ROW_WIDTH: LogicalPx = LogicalPx(420.0);

/// 이 specimen 이 전시하는 줌 범위 — 디자인 문구가 그대로 쓰는 25~200 %.
const ZOOM_MIN: f64 = 25.0;
const ZOOM_MAX: f64 = 200.0;

struct State {
    /// 세 행의 편집 버퍼. 본체가 egui 메모리에 두는 것과 같은 자리다.
    bufs: [String; 3],
}

thread_local! {
    /// 디자인 상태 셋의 초기 글자 — 범위 안 · 범위 밖 · disabled.
    static STATE: RefCell<State> = RefCell::new(State {
        bufs: [
            String::from("150"),
            String::from("420"),
            String::from("100"),
        ],
    });
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            STATE.with(|s| {
                let st = &mut *s.borrow_mut();
                for (i, label) in ["default", "out of range — clamp on commit", "disabled"]
                    .iter()
                    .enumerate()
                {
                    let enabled = i != 2;
                    row(ui, theme, label, &mut st.bufs[i], enabled);
                }
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("control", "Input — 새 컴포넌트 아님"),
            ("font", "mono · 자릿수 우측 정렬"),
            ("width", "field_width_xs (90)"),
            (
                "suffix",
                "필드 밖 정적 텍스트, muted · caption 11(font_size_caption)",
            ),
            ("clamp", "확정(blur / ↵) 때만 — 치는 중엔 안 건드린다"),
            (
                "out of range",
                "danger 테두리 + 범위 한 줄 · 입력칸 아래, 입력칸 왼쪽 끝에서 시작",
            ),
        ],
        &[
            TokenChip::new(
                "accent-danger",
                "범위 밖 테두리 · 줄",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::new("text-muted", "단위 suffix", theme.text_muted().to_egui()),
            TokenChip::new(
                "text-disabled",
                "disabled 라벨 · 단위",
                theme.text_disabled().to_egui(),
            ),
            TokenChip::without_color("field-width-xs", "numeric field width"),
        ],
    );
}

/// 토스트 표시 시간의 범위(초)와 눈금. 본체 `tabs/overlay.rs`의 NumberSpec과 같은 값이다.
const TOAST_SECS_MIN: f64 = 1.0;
const TOAST_SECS_MAX: f64 = 10.0;
const TOAST_SECS_STEP: f64 = 0.5;

thread_local! {
    /// 시안 `ToastDragValue` 상태의 초기 글자 — 평소 · 범위 밖.
    static TOAST_BUFS: RefCell<[String; 2]> =
        RefCell::new([String::from("2.0"), String::from("14")]);
}

/// Settings › General › Overlay — 토스트 표시 시간. 설정 숫자 모양(mono Input 90 · 정적 단위 s ·
/// 확정 때 1.0~10.0 범위 제한과 0.5 눈금)을 평소와 범위 밖 두 상태로 보인다.
pub fn draw_toast_duration(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            TOAST_BUFS.with(|b| {
                let bufs = &mut *b.borrow_mut();
                for (i, caption) in ["rest", "out of range — commits as 10.0"]
                    .iter()
                    .enumerate()
                {
                    toast_row(ui, theme, caption, &mut bufs[i]);
                }
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("L2 position", "4th — after Accessibility"),
            ("label", "Toast duration"),
            ("unit", "seconds — mono \"2.0 s\""),
            ("range / step", "1.0–10.0 s, step 0.5"),
            ("default", "2.0 s (= DEFAULT_LIFETIME 2000ms)"),
            (
                "control",
                "mono Input (90) + static \"s\" — the Numbers in settings shape",
            ),
            ("commit", "blur / ↵ · clamps to 1.0–10.0 and snaps to 0.5"),
            (
                "out of range",
                "danger border + \"Between 1.0 and 10.0.\" line",
            ),
            ("hint", "muted line below the grid"),
        ],
        &[
            TokenChip::without_color("field-width-xs", "field width 90"),
            TokenChip::new(
                "border-focus",
                "editing border",
                theme.border_focus().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "out of range",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::without_color("font-mono", "value text"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Scope is this one row only — the tab name is the umbrella so future overlay settings can land here. The value is stored in ms; only the display is in seconds. The editing state is the shared Input focus border and shows when the field is clicked.",
    );
}

/// 토스트 표시 시간 한 행 — 상태 캡션 + (라벨 · 필드 · 단위 s) + (범위 밖이면) 한 줄.
fn toast_row(ui: &mut egui::Ui, theme: &Theme, caption: &str, buf: &mut String) {
    let settled = buf
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| {
            (
                (v.clamp(TOAST_SECS_MIN, TOAST_SECS_MAX) / TOAST_SECS_STEP).round()
                    * TOAST_SECS_STEP,
                v,
            )
        })
        .and_then(|(s, v)| (s != v).then_some(s));
    let mut input_left = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.label(
            egui::RichText::new(caption)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            ui.label(egui::RichText::new("Toast duration").color(theme.text_secondary().to_egui()));
            let field = Input::new()
                .mono(true)
                .align(egui::Align::RIGHT)
                .width(theme.field_width_xs.value())
                .invalid(settled.is_some())
                .show(ui, theme, buf);
            input_left = Some(field.rect.left());
            ui.label(
                egui::RichText::new("s")
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            );
        });
        if let Some(settled) = settled {
            range_line(
                ui,
                theme,
                input_left,
                format!(
                    "Between {TOAST_SECS_MIN:.1} and {TOAST_SECS_MAX:.1}. Commits as {settled:.1}."
                ),
            );
        }
    });
}

/// 한 행 — 상태 캡션 + (라벨 · 필드 · 단위) + (범위 밖이면) 한 줄.
fn row(ui: &mut egui::Ui, theme: &Theme, caption: &str, buf: &mut String, enabled: bool) {
    let settled = out_of_range(buf);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.label(
            egui::RichText::new(caption)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        let mut input_left = None;
        ui.allocate_ui(
            egui::vec2(ROW_WIDTH.value(), theme.input_height().value()),
            |ui| {
                // 단위와 필드를 오른쪽에 놓고 남은 폭을 라벨에 준다.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    let muted = if enabled {
                        theme.text_muted()
                    } else {
                        theme.text_disabled()
                    };
                    ui.label(
                        egui::RichText::new("%")
                            .size(theme.font_size_caption.value())
                            .color(muted),
                    );
                    let field = Input::new()
                        .mono(true)
                        .align(egui::Align::RIGHT)
                        .width(theme.field_width_xs.value())
                        .enabled(enabled)
                        .invalid(settled.is_some())
                        .show(ui, theme, buf);
                    input_left = Some(field.rect.left());
                    let ink = if enabled {
                        theme.text_secondary()
                    } else {
                        theme.text_disabled()
                    };
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Default zoom").color(ink));
                    });
                });
            },
        );
        if let Some(settled) = settled {
            range_line(
                ui,
                theme,
                input_left,
                format!("Between {ZOOM_MIN:.0} and {ZOOM_MAX:.0}. Commits as {settled:.0}."),
            );
        }
    });
}

/// 범위 밖 한 줄. 본체처럼 입력칸 아래, 입력칸 왼쪽 끝에서 시작한다(컨트롤 칸 안).
fn range_line(ui: &mut egui::Ui, theme: &Theme, input_left: Option<f32>, text: String) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if let Some(left) = input_left {
            ui.add_space((left - ui.cursor().left()).max(0.0));
        }
        ui.label(
            egui::RichText::new(text)
                .size(theme.font_size_caption.value())
                .color(theme.accent_danger_raised().to_egui()),
        );
    });
}

/// 확정 시 범위 제한으로 값이 바뀌는지 확인한다. 본체 number::out_of_range와 같은 계산이다.
fn out_of_range(buf: &str) -> Option<f64> {
    let typed = buf.trim().parse::<f64>().ok()?;
    if !typed.is_finite() {
        return None;
    }
    let settled = typed.clamp(ZOOM_MIN, ZOOM_MAX);
    (settled != typed).then_some(settled)
}
