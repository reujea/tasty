//! 원격 파일 전송의 진행·실패 팝업 예제.
//! 진행 막대는 비율을 바로 반영하며 애니메이션을 추가하지 않는다.
//! 실제 전송이나 배경 어둡게 하기는 실행하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

// 여백은 본체와 같은 transfer-* 토큰이고 폭과 함께 UI 배율을 따른다. 줄 높이는 각 줄의
// 글자 크기 × line-height-ui다.

/// 글자 크기 한 줄의 높이.
pub(super) fn line_h(theme: &Theme, font: LogicalPx) -> LogicalPx {
    font.scaled(theme.line_height_ui)
}

/// 시안 Stage의 배율 비교 세 단계.
const ZOOMS: [(&str, f32); 3] = [
    ("ui_scale 0.85", 0.85),
    ("ui_scale 1", 1.0),
    ("ui_scale 1.2", 1.2),
];

/// 단일·다중 파일 전송의 진행 상태를 비교한다.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "receiving (mid-transfer)", |ui| {
            progress_card(
                ui,
                theme,
                &[ProgressRow {
                    name: "sprint-42-demo.mp4",
                    pct: 27,
                    done: "34.6 MiB",
                    total: "128.0 MiB",
                    rate: "2.1 MiB/s",
                }],
            );
        });
        spec::cluster(ui, theme, "multiple files (row repeat)", |ui| {
            progress_card(
                ui,
                theme,
                &[
                    ProgressRow {
                        name: "clip-0001.png",
                        pct: 100,
                        done: "1.2 MiB",
                        total: "1.2 MiB",
                        rate: "8.4 MiB/s",
                    },
                    ProgressRow {
                        name: "very-long-capture-filename-that-elides.png",
                        pct: 41,
                        done: "0.5 MiB",
                        total: "1.2 MiB",
                        rate: "3.1 MiB/s",
                    },
                ],
            );
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("width", "transfer-popup-width"),
            ("header", "download glyph · title · mono %"),
            ("filename", "mono 13 · ellipsized"),
            ("bar", "4px track + accent fill · no animation"),
            ("stats", "done / total · rate (mono, muted)"),
            ("close", "auto on completion · Cancel aborts"),
            ("outside click", "does not dismiss"),
        ],
        &[
            TokenChip::new(
                "progress-track-bg",
                "recessed track",
                theme.progress_track_bg().to_egui(),
            ),
            TokenChip::new(
                "progress-fill-bg",
                "determinate fill",
                theme.progress_fill_bg().to_egui(),
            ),
            TokenChip::without_color("progress-height", "4px thickness"),
            TokenChip::new("bg-panel", "frame", theme.bg_panel().to_egui()),
        ],
    );

    // 1.2 카드는 문서 열에서 앞의 두 카드와 한 줄에 들어가지 않는다. horizontal_wrapped는 크기를
    // 미리 모르는 cluster를 다음 줄로 넘기지 못해 경계를 넘으므로 세로로 쌓는다.
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (label, zoom) in ZOOMS {
            let zoomed = Theme::with_colors_and_zoom(theme.to_colors(), theme.is_light, zoom);
            spec::cluster(
                ui,
                theme,
                &format!("{label} — width and insets scale together"),
                |ui| {
                    progress_card(
                        ui,
                        &zoomed,
                        &[ProgressRow {
                            name: "sprint-42-demo.mp4",
                            pct: 27,
                            done: "34.6 MiB",
                            total: "128.0 MiB",
                            rate: "2.1 MiB/s",
                        }],
                    );
                },
            );
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "scale",
                "on-scale: width and every inset below multiply by ui_scale",
            ),
            (
                "header",
                "pad-y transfer-header-pad-y 12 · pad-x transfer-pad-x 12 · gap space-sm",
            ),
            (
                "body",
                "padding transfer-pad-x 12 · block gap transfer-body-gap 8",
            ),
            ("footer", "pad-y transfer-footer-pad-y 8 · pad-x 12"),
            (
                "reason well",
                "transfer-well-pad-y 8 · transfer-well-pad-x 12",
            ),
            (
                "line heights",
                "not tokens — each line = its font size × line-height-ui \
                 (header row = max(glyph md, title line))",
            ),
        ],
        &[
            TokenChip::without_color("transfer-pad-x", "→ space-md 12"),
            TokenChip::without_color("transfer-header-pad-y", "→ space-md 12"),
            TokenChip::without_color("transfer-body-gap", "→ space-sm 8"),
            TokenChip::without_color("transfer-footer-pad-y", "→ space-sm 8"),
            TokenChip::without_color("transfer-well-pad-y", "→ space-sm 8"),
            TokenChip::without_color("transfer-well-pad-x", "→ space-md 12"),
        ],
    );
}

/// 시작 전 거부와 전송 중 실패의 확인·재시도 버튼을 비교한다.
pub fn draw_error(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "rejected before start (capacity)", |ui| {
            error_card(
                ui,
                theme,
                "sprint-42-demo.mp4",
                "capacity exceeded — transfers folder is at its 500 MiB limit",
                false,
            );
        });
        spec::cluster(ui, theme, "failed mid-transfer (retry)", |ui| {
            error_card(
                ui,
                theme,
                "sprint-42-demo.mp4",
                "connection lost while receiving",
                true,
            );
        });
    });
}

pub(super) struct ProgressRow {
    pub(super) name: &'static str,
    pub(super) pct: u32,
    pub(super) done: &'static str,
    pub(super) total: &'static str,
    pub(super) rate: &'static str,
}

/// `transfer-popup-width` 프레임 셸 (bg-panel + 1px border-strong + modal shadow). item_spacing 0 —
/// 각 구역이 자체 패딩을 가진다.
pub(super) fn frame(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_strong().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .shadow(theme.shadow_modal().to_egui())
        .show(ui, |ui| {
            ui.set_width(theme.transfer_popup_width().value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.vertical(|ui| {
                ui.set_width(theme.transfer_popup_width().value());
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                add(ui);
            });
        });
}

/// 헤더 띠 — glyph + 제목(+ 우측 trailing). 하단 separator.
pub(super) fn header_band(
    ui: &mut egui::Ui,
    theme: &Theme,
    glyph: icons::MockGlyph,
    glyph_color: egui::Color32,
    title: &str,
    trailing: Option<&str>,
) {
    let content_h = theme
        .icon_glyph_size_md
        .max(line_h(theme, theme.font_size_max));
    let pad_y = theme.transfer_header_pad_y();
    let pad_x = theme.transfer_pad_x();
    let band_h = pad_y.scaled(2.0) + content_h;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(theme.transfer_popup_width().value(), band_h.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x.value(), rect.top() + pad_y.value()),
        egui::pos2(rect.right() - pad_x.value(), rect.bottom() - pad_y.value()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    kit::icon(&mut child, glyph, theme.icon_glyph_size_md, glyph_color);
    child.label(
        egui::RichText::new(title)
            .size(theme.font_size_max.value())
            .strong()
            .color(theme.text_primary().to_egui()),
    );
    if let Some(pct) = trailing {
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(pct)
                    .monospace()
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            );
        });
    }
}

/// 진행 카드.
fn progress_card(ui: &mut egui::Ui, theme: &Theme, rows: &[ProgressRow]) {
    frame(ui, theme, |ui| {
        // 다중 파일이면 헤더 pct 는 전체 평균 대신 첫 행 기준(단일 파일이 표준).
        let head_pct = rows.first().map(|r| r.pct).unwrap_or(0);
        header_band(
            ui,
            theme,
            icons::DOWNLOAD,
            theme.text_muted().to_egui(),
            "Receiving file",
            Some(&format!("{head_pct}%")),
        );
        body_region(ui, theme, |ui| {
            for (i, row) in rows.iter().enumerate() {
                if i > 0 {
                    ui.add_space(theme.transfer_body_gap().value());
                }
                progress_row(ui, theme, row);
            }
        });
        footer_buttons(ui, theme, |ui| {
            Button::new("Cancel")
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, theme);
        });
    });
}

/// 한 파일 진행 행 — 파일명 → determinate bar → done/total · rate.
pub(super) fn progress_row(ui: &mut egui::Ui, theme: &Theme, row: &ProgressRow) {
    let w = ui.available_width();
    let left_center = egui::Layout::left_to_right(egui::Align::Center);
    let file_h = theme
        .icon_glyph_size_md
        .max(line_h(theme, theme.font_size_body))
        .value();
    ui.allocate_ui_with_layout(egui::vec2(w, file_h), left_center, |ui| {
        ui.set_min_height(file_h);
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        kit::icon(
            ui,
            icons::FILE,
            theme.icon_glyph_size_md,
            theme.text_muted().to_egui(),
        );
        let avail = ui.available_width();
        let name = elide_mono(ui, theme, row.name, avail);
        ui.label(
            egui::RichText::new(name)
                .monospace()
                .size(theme.font_size_body.value())
                .color(theme.text_primary().to_egui()),
        );
    });
    ui.add_space(theme.transfer_body_gap().value());
    progress_bar(ui, theme, row.pct);
    ui.add_space(theme.transfer_body_gap().value());
    let stats_h = line_h(theme, theme.font_size_caption).value();
    ui.allocate_ui_with_layout(egui::vec2(w, stats_h), left_center, |ui| {
        ui.set_min_height(stats_h);
        ui.label(
            egui::RichText::new(format!("{} / {}", row.done, row.total))
                .monospace()
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(row.rate)
                    .monospace()
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            );
        });
    });
}

/// determinate progress bar — recessed track + accent fill (0ms 무애니).
/// 토큰: height=`--tasty-progress-height` · radius=radius-sm · track=bg-app · fill=accent-primary.
pub(super) fn progress_bar(ui: &mut egui::Ui, theme: &Theme, pct: u32) {
    let h = theme.progress_height().value();
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let r = theme.corner_radius_sm.value();
    ui.painter().rect_filled(rect, r, theme.bg_app().to_egui());
    let frac = (pct.min(100) as f32) / 100.0;
    if frac > 0.0 {
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, h));
        ui.painter()
            .rect_filled(fill, r, theme.accent_primary().to_egui());
    }
}

/// 실패 카드.
fn error_card(ui: &mut egui::Ui, theme: &Theme, name: &str, reason: &str, retry: bool) {
    frame(ui, theme, |ui| {
        header_band(
            ui,
            theme,
            icons::ALERT_TRIANGLE,
            theme.accent_danger().to_egui(),
            "Transfer failed",
            None,
        );
        body_region(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(
                    egui::RichText::new(name)
                        .monospace()
                        .strong()
                        .size(theme.font_size_body.value())
                        .color(theme.text_primary().to_egui()),
                );
                ui.label(
                    egui::RichText::new(crate::i18n::t("transfer.error.body_suffix"))
                        .size(theme.font_size_body.value())
                        .color(theme.text_secondary().to_egui()),
                );
            });
            ui.add_space(theme.transfer_body_gap().value());
            reason_well(ui, theme, reason);
        });
        // 푸터 버튼 — danger-fill 금지 (ghost/secondary 만).
        footer_buttons(ui, theme, |ui| {
            if retry {
                Button::new("Retry")
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .show(ui, theme);
                Button::new("Dismiss")
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .show(ui, theme);
            } else {
                Button::new("Dismiss")
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .show(ui, theme);
            }
        });
    });
}

/// command-well 패턴 — bg-app + 1px separator + radius, mono danger 텍스트.
fn reason_well(ui: &mut egui::Ui, theme: &Theme, reason: &str) {
    reason_well_kind(ui, theme, reason, false);
}

/// 사유 칸. 기계가 보고한 원인은 mono 인용, 번역한 문장(`sentence`)은 UI 서체 본문 크기로 감는다.
pub(super) fn reason_well_kind(ui: &mut egui::Ui, theme: &Theme, reason: &str, sentence: bool) {
    egui::Frame::new()
        .fill(theme.bg_app().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            theme.transfer_well_pad_x().value() as i8,
            theme.transfer_well_pad_y().value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if sentence {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(reason)
                            .size(theme.font_size_body.value())
                            .line_height(Some(line_h(theme, theme.font_size_body).value()))
                            .color(theme.accent_danger_raised().to_egui()),
                    )
                    .wrap(),
                );
            } else {
                ui.label(
                    egui::RichText::new(reason)
                        .monospace()
                        .size(theme.font_size_caption.value())
                        .color(theme.accent_danger_raised().to_egui()),
                );
            }
        });
}

/// 바디 region (사방 transfer-pad-x, 전체폭).
pub(super) fn body_region(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(theme.transfer_pad_x().value() as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui);
        });
}

/// 푸터 (transfer-footer-pad-y / transfer-pad-x, borderTop separator, 우측정렬). `add` 는 우→좌 순서로
/// 위젯을 넣는다(먼저 넣은 것이 우측 끝).
pub(super) fn footer_buttons(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    let btn_h = LogicalPx(ControlSize::Sm.height(theme));
    let pad_y = theme.transfer_footer_pad_y();
    let pad_x = theme.transfer_pad_x();
    let band_h = pad_y.scaled(2.0) + btn_h;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(theme.transfer_popup_width().value(), band_h.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x.value(), rect.top() + pad_y.value()),
        egui::pos2(rect.right() - pad_x.value(), rect.bottom() - pad_y.value()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    add(&mut child);
}

/// mono 문자열을 폭에 맞게 앞은 두고 뒤를 `…` 로 자른다(폰트 메트릭 근사).
pub(super) fn elide_mono(ui: &egui::Ui, theme: &Theme, s: &str, max_w: f32) -> String {
    let font = egui::FontId::monospace(theme.font_size_body.value());
    let w = |t: &str| {
        ui.fonts(|f| {
            f.layout_no_wrap(t.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .rect
                .width()
        })
    };
    if w(s) <= max_w {
        return s.to_owned();
    }
    let mut cut = s.chars().collect::<Vec<_>>();
    while !cut.is_empty() {
        cut.pop();
        let candidate: String = cut.iter().collect::<String>() + "…";
        if w(&candidate) <= max_w {
            return candidate;
        }
    }
    "…".to_owned()
}
