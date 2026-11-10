//! 원격 파일 "다운로드 후 실행"의 대화상자·큰 파일 확인·진행·실패 예제.
//! 전송 팝업과 같은 폭·여백·셸을 쓴다. 실제 다운로드는 실행하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize};

use super::transfer::{
    ProgressRow, body_region, elide_mono, footer_buttons, frame, header_band, line_h, progress_row,
    reason_well_kind,
};
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;
use crate::i18n::t;

/// 묻는 대화상자. 서버가 파일 다운로드를 알린 연결과 옛 서버를 비교한다.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "ready", |ui| {
            dialog_card(ui, theme, "/srv/reports/q3-summary.xlsx", true);
        });
        spec::cluster(
            ui,
            theme,
            "old server — action disabled, reason shown",
            |ui| {
                dialog_card(ui, theme, "/srv/reports/q3-summary.xlsx", false);
            },
        );
        spec::cluster(ui, theme, "long path — front elided", |ui| {
            dialog_card(
                ui,
                theme,
                "/home/builder/workspace/monorepo/services/ingest/fixtures/2026/11/very-long-capture-name.bin",
                true,
            );
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "when",
                "user file open on a mirror with no handler the remote host can open",
            ),
            (
                "header",
                "title font-size-max semibold (wraps) · host mark (remote glyph + host, fh-remote-fg) · space-sm · path mono caption muted, front elided",
            ),
            (
                "body",
                "description body text-secondary · one-file line caption text-muted",
            ),
            (
                "old server",
                "surface-raised well · info glyph · caption text-secondary · primary disabled",
            ),
            (
                "footer",
                "ghost Cancel · primary Download and run with download glyph",
            ),
            ("Esc / outside click", "same as Cancel"),
        ],
        &[
            TokenChip::new(
                "surface-raised",
                "old server well",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "accent-info-raised",
                "info glyph",
                theme.accent_info_raised().to_egui(),
            ),
            TokenChip::new("fh-remote-fg", "host mark", theme.fh_remote_fg().to_egui()),
            TokenChip::without_color("fh-remote-max-width", "host text cap"),
            TokenChip::without_color("transfer-popup-width", "frame width"),
        ],
    );
}

/// 1 MiB 를 넘는 파일의 확인.
pub fn draw_large(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "over 1 MiB — asked once", |ui| {
            frame(ui, theme, |ui| {
                header_band(
                    ui,
                    theme,
                    icons::ALERT_TRIANGLE,
                    theme.accent_attention_raised().to_egui(),
                    t("download_run.large.title"),
                    None,
                );
                body_region(ui, theme, |ui| {
                    ui.spacing_mut().item_spacing.y = theme.transfer_body_gap().value();
                    file_line(ui, theme, "build-artifacts.tar.gz");
                    wrapped(
                        ui,
                        theme,
                        &t("download_run.large.body").replace("{}", "3.2 MiB"),
                        theme.font_size_body,
                        theme.text_secondary().to_egui(),
                    );
                });
                dialog_footer(ui, theme, true);
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "glyph",
                "attention — nothing is wrong, the size is the question",
            ),
            (
                "size",
                "one decimal, rounded up by 0.1 · MiB below 1024 MiB, else GiB",
            ),
            (
                "Esc / outside click",
                "same as Cancel — nothing is received",
            ),
        ],
        &[TokenChip::new(
            "accent-attention-raised",
            "glyph",
            theme.accent_attention_raised().to_egui(),
        )],
    );
}

/// 받는 동안의 진행. 업로드 진행 팝업과 같은 형상에 다운로드 제목과 Stop download 를 둔다.
pub fn draw_progress(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "downloading", |ui| {
            frame(ui, theme, |ui| {
                header_band(
                    ui,
                    theme,
                    icons::DOWNLOAD,
                    theme.text_muted().to_egui(),
                    t("transfer.progress.title_download_run"),
                    Some("41%"),
                );
                body_region(ui, theme, |ui| {
                    progress_row(
                        ui,
                        theme,
                        &ProgressRow {
                            name: "build-artifacts.tar.gz",
                            pct: 41,
                            done: "1.3 MiB",
                            total: "3.2 MiB",
                            rate: "2.1 MiB/s",
                        },
                    );
                });
                footer_buttons(ui, theme, |ui| {
                    Button::new(t("transfer.progress.stop"))
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, theme);
                });
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "stop button",
                "stops the download and removes what was received",
            ),
            (
                "Esc / outside click",
                "do not close the popup and do not stop — only Stop download does",
            ),
            ("auto-close", "when every download is done"),
        ],
        &[],
    );
}

/// 실패. 원인마다 번역한 문장을 UI 서체로 쓰고 Dismiss 만 둔다.
pub fn draw_failed(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, key) in [
            ("folder", "download_run.reason.directory"),
            ("connection lost", "download_run.reason.disconnected"),
            ("no answer in time", "download_run.reason.timeout"),
        ] {
            spec::cluster(ui, theme, label, |ui| failed_card(ui, theme, t(key)));
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("title", "transfer.error.title_download_run"),
            (
                "reason",
                "one translated sentence per cause · UI face body 13 · line-height-ui",
            ),
            ("buttons", "Dismiss only"),
        ],
        &[],
    );
}

fn failed_card(ui: &mut egui::Ui, theme: &Theme, reason: &str) {
    frame(ui, theme, |ui| {
        header_band(
            ui,
            theme,
            icons::ALERT_TRIANGLE,
            theme.accent_danger_raised().to_egui(),
            t("transfer.error.title_download_run"),
            None,
        );
        body_region(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(
                    egui::RichText::new("q3-summary.xlsx")
                        .monospace()
                        .strong()
                        .size(theme.font_size_body.value())
                        .color(theme.text_primary().to_egui()),
                );
                ui.label(
                    egui::RichText::new(t("transfer.error.body_suffix"))
                        .size(theme.font_size_body.value())
                        .color(theme.text_secondary().to_egui()),
                );
            });
            ui.add_space(theme.transfer_body_gap().value());
            reason_well_kind(ui, theme, reason, true);
        });
        footer_buttons(ui, theme, |ui| {
            Button::new(t("transfer.error.dismiss"))
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .show(ui, theme);
        });
    });
}

fn dialog_card(ui: &mut egui::Ui, theme: &Theme, path: &str, capable: bool) {
    frame(ui, theme, |ui| {
        let head = egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(
                theme.transfer_pad_x().value() as i8,
                theme.transfer_header_pad_y().value() as i8,
            ))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(t("download_run.dialog.title"))
                            .size(theme.font_size_max.value())
                            .strong()
                            .line_height(Some(line_h(theme, theme.font_size_max).value()))
                            .color(theme.text_primary().to_egui()),
                    )
                    .wrap(),
                );
                remote_path_line(
                    ui,
                    theme,
                    &tasty_ui_widgets::RemoteMark {
                        host: "gx10",
                        empty_label: t("attach.mirror_pill_label"),
                    },
                    path,
                );
            });
        let rect = head.response.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            egui::Stroke::new(
                theme.border_width.value(),
                theme.separator.to_egui_premultiplied(),
            ),
        );
        body_region(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = theme.transfer_body_gap().value();
            wrapped(
                ui,
                theme,
                t("download_run.dialog.description"),
                theme.font_size_body,
                theme.text_secondary().to_egui(),
            );
            wrapped(
                ui,
                theme,
                t("download_run.dialog.single_file"),
                theme.font_size_caption,
                theme.text_muted().to_egui(),
            );
            if !capable {
                egui::Frame::new()
                    .fill(theme.surface_raised().to_egui())
                    .corner_radius(theme.corner_radius.value())
                    .inner_margin(egui::Margin::symmetric(
                        theme.transfer_well_pad_x().value() as i8,
                        theme.transfer_well_pad_y().value() as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                            kit::icon(
                                ui,
                                icons::INFO,
                                theme.icon_glyph_size_sm,
                                theme.accent_info_raised().to_egui(),
                            );
                            wrapped(
                                ui,
                                theme,
                                t("download_run.dialog.unsupported_server"),
                                theme.font_size_caption,
                                theme.text_secondary().to_egui(),
                            );
                        });
                    });
            }
        });
        dialog_footer(ui, theme, capable);
    });
}

fn dialog_footer(ui: &mut egui::Ui, theme: &Theme, enabled: bool) {
    footer_buttons(ui, theme, |ui| {
        Button::new(t("download_run.dialog.run"))
            .variant(ButtonVariant::Primary)
            .size(ControlSize::Sm)
            .enabled(enabled)
            .leading_icon(&|ui, rect, c| {
                icons::DOWNLOAD.image(rect.width(), c).paint_at(ui, rect);
            })
            .show(ui, theme);
        Button::new(t("download_run.dialog.cancel"))
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .show(ui, theme);
    });
}

fn file_line(ui: &mut egui::Ui, theme: &Theme, name: &str) {
    let w = ui.available_width();
    let h = theme
        .icon_glyph_size_md
        .max(line_h(theme, theme.font_size_body))
        .value();
    ui.allocate_ui_with_layout(
        egui::vec2(w, h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_height(h);
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            kit::icon(
                ui,
                icons::FILE,
                theme.icon_glyph_size_md,
                theme.text_muted().to_egui(),
            );
            let avail = ui.available_width();
            ui.label(
                egui::RichText::new(elide_mono(ui, theme, name, avail))
                    .monospace()
                    .size(theme.font_size_body.value())
                    .color(theme.text_primary().to_egui()),
            );
        },
    );
}

fn wrapped(
    ui: &mut egui::Ui,
    theme: &Theme,
    text: &str,
    font: tasty_type_geometry::length::LogicalPx,
    color: egui::Color32,
) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(font.value())
                .line_height(Some(line_h(theme, font).value()))
                .color(color),
        )
        .wrap(),
    );
}

/// 본체 `remote_path_line` 과 같은 형상: 공용 표지 위젯 + space-sm + 앞을 줄인 경로.
fn remote_path_line(
    ui: &mut egui::Ui,
    theme: &Theme,
    mark: &tasty_ui_widgets::RemoteMark<'_>,
    path: &str,
) {
    let font = egui::FontId::monospace(theme.font_size_caption.value());
    let line_h = ui.fonts(|f| f.row_height(&font));
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), line_h),
        egui::Sense::hover(),
    );
    let mark_w = tasty_ui_widgets::paint_fh_remote_mark(
        ui,
        theme,
        egui::pos2(rect.left(), rect.center().y),
        mark,
    );
    let left = rect.left() + (mark_w + theme.spacing_sm).value();
    let shown = elide_front_mono(ui, theme, path, rect.right() - left);
    ui.painter().text(
        egui::pos2(left, rect.top()),
        egui::Align2::LEFT_TOP,
        shown,
        font,
        theme.text_muted().to_egui(),
    );
}

/// 경로는 끝(파일 이름)이 보이도록 앞을 `…` 로 줄인다.
fn elide_front_mono(ui: &egui::Ui, theme: &Theme, s: &str, max_w: f32) -> String {
    let font = egui::FontId::monospace(theme.font_size_caption.value());
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
    let mut cut: std::collections::VecDeque<char> = s.chars().collect();
    while !cut.is_empty() {
        cut.pop_front();
        let candidate: String = std::iter::once('…').chain(cut.iter().copied()).collect();
        if w(&candidate) <= max_w {
            return candidate;
        }
    }
    "…".to_owned()
}
