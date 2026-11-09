//! 탐색기 파일 작업 — 진행 상태줄·대기열·이름 충돌·결과 카드·드래그 칩·놓을 대상 표시.
//! 디자인 `gallery/explorer-ops.jsx`(Progress) · `explorer-ops-parts.jsx`(Drag) 의 예제를 옮겼다.
//! 그리기는 본체와 같은 `tasty_ui_widgets::explorer_ops` 함수가 맡는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_appearance::toast_kind::ToastKind;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    ButtonVariant, ConflictProps, DragChipProps, DragOp, OpStatusProps, QueueRowProps,
    ResultAction, ResultCardProps, ResultLine, conflict_card, drag_chip, op_queue_popover,
    op_status_line, paint_drop_ring, paint_drop_tint, result_card,
};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::{t, t_args, t_count, t_fmt, t_fmt2};

mod originals;
mod remote;
mod staging;

/// 시안 탐색기 칸 폭(`XCell w={520}`). 전시 치수다.
const CELL_W: LogicalPx = LogicalPx(520.0);
/// 시안 칸 본문 높이 — 상태줄 위 목록 자리. 전시 치수다.
const BODY_H: LogicalPx = LogicalPx(96.0);
/// 시안 대기열 예제의 본문 높이. 팝오버가 상태줄 위에 뜰 자리다. 전시 치수다.
const QUEUE_BODY_H: LogicalPx = LogicalPx(120.0);
/// 시안 드래그 예제의 목록 행 폭. 전시 치수다.
const ROW_W: LogicalPx = LogicalPx(300.0);

fn glyph_painter(glyph: MockGlyph) -> impl Fn(&mut egui::Ui, egui::Rect, egui::Color32) {
    move |ui, rect, c| glyph.image(rect.height(), c).paint_at(ui, rect)
}

/// 상태줄까지 그린 탐색기 칸 한 개. 본문은 비워 두고 상태줄 영역을 돌려준다.
fn cell(ui: &mut egui::Ui, theme: &Theme, body_h: LogicalPx) -> (egui::Rect, egui::Rect) {
    let status_h = theme.item_height_interactive.value();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(CELL_W.value(), body_h.value() + status_h),
        egui::Sense::hover(),
    );
    let radius = theme.corner_radius.value();
    ui.painter().rect(
        rect,
        radius,
        theme.bg_panel().to_egui(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    let status =
        egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - status_h), rect.max);
    ui.painter()
        .rect_filled(status, 0.0, theme.bg_sidebar().to_egui());
    ui.painter().hline(
        status.x_range(),
        status.top(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
    );
    (rect, status)
}

pub fn draw_progress(ui: &mut egui::Ui, theme: &Theme) {
    let copying = t_args(
        "explorer.op.copying",
        &["12", "40", "report-final-v3-signed.pdf"],
    );
    let bytes = t_fmt2("explorer.op.bytes", "1.2 GB", "3.4 GB");
    let queued = t_fmt("explorer.op.queued", "1");
    let waiting = t_count("explorer.op.waiting", 1, &["1"]);
    let queue_copy = t_fmt2("explorer.op.queue_copy", "40", "Documents");
    let queue_move = t_fmt2("explorer.op.queue_move", "3", "Archive");
    let queue_sub = format!(
        "{} · {bytes}",
        t_fmt2("explorer.op.queue_progress", "12", "40")
    );
    let close = glyph_painter(icons::CLOSE);
    let layers = glyph_painter(icons::LAYERS);
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
        for text in [
            t_count("explorer.op.counting_copy", 48210, &["48,210"]),
            t_count("explorer.op.counting_move", 1, &["1"]),
        ] {
            let (_, status) = cell(ui, theme, BODY_H);
            op_status_line(
                ui,
                theme,
                status,
                &OpStatusProps {
                    show_bar: false,
                    fraction: None,
                    waiting: false,
                    text: &text,
                    bytes: None,
                    queued: None,
                    show_label: t("explorer.op.show"),
                    cancel_tip: t("explorer.op.cancel"),
                    host: None,
                },
                &close,
            );
        }
        let empty_files = t_args(
            "explorer.op.copying",
            &["1,204", "200,000", "file-01204.txt"],
        );
        let (_, status) = cell(ui, theme, BODY_H);
        op_status_line(
            ui,
            theme,
            status,
            &OpStatusProps {
                show_bar: true,
                fraction: Some(1204.0 / 200000.0),
                waiting: false,
                text: &empty_files,
                bytes: None,
                queued: None,
                show_label: t("explorer.op.show"),
                cancel_tip: t("explorer.op.cancel"),
                host: None,
            },
            &close,
        );

        let (_, status) = cell(ui, theme, BODY_H);
        op_status_line(
            ui,
            theme,
            status,
            &OpStatusProps {
                show_bar: true,
                fraction: Some(0.34),
                waiting: false,
                text: &copying,
                bytes: Some(&bytes),
                queued: Some(&queued),
                show_label: t("explorer.op.show"),
                cancel_tip: t("explorer.op.cancel"),
                host: None,
            },
            &close,
        );
        let (rect, status) = cell(ui, theme, QUEUE_BODY_H);
        op_status_line(
            ui,
            theme,
            status,
            &OpStatusProps {
                show_bar: true,
                fraction: Some(0.34),
                waiting: false,
                text: &copying,
                bytes: Some(&bytes),
                queued: Some(&queued),
                show_label: t("explorer.op.show"),
                cancel_tip: t("explorer.op.cancel"),
                host: None,
            },
            &close,
        );
        // 디자인: 칸 오른쪽에서 space-sm, 상태줄 위로 space-xs 떨어져 뜬다.
        op_queue_popover(
            ui,
            theme,
            egui::Id::new("explorer_ops_queue_specimen"),
            egui::pos2(
                rect.right() - theme.spacing_sm.value(),
                status.top() - theme.spacing_xs.value(),
            ),
            &[
                QueueRowProps {
                    title: &queue_copy,
                    sub: &queue_sub,
                    running: true,
                    remove_tip: t("explorer.op.cancel"),
                },
                QueueRowProps {
                    title: &queue_move,
                    sub: t("explorer.op.queue_waiting"),
                    running: false,
                    remove_tip: t("explorer.op.remove_from_queue"),
                },
            ],
            None,
            &close,
            &layers,
        );
        let (_, status) = cell(ui, theme, BODY_H);
        op_status_line(
            ui,
            theme,
            status,
            &OpStatusProps {
                show_bar: true,
                fraction: Some(0.5),
                waiting: true,
                text: &waiting,
                bytes: None,
                queued: None,
                show_label: t("explorer.op.show"),
                cancel_tip: t("explorer.op.cancel"),
                host: None,
            },
            &close,
        );
    });
    spec::meta(
        ui,
        theme,
        &[
            ("where", "status line of the explorer that started the job"),
            (
                "bar",
                "explorer-progress-height 2 · top edge · bytes, else file count for zero/unknown totals",
            ),
            (
                "label",
                "Copying|Moving|Moving to Trash i of n · file · caption · ellipsis",
            ),
            (
                "counting",
                "copy / move words · no bar · at most 4 updates/s · discovered file count",
            ),
            ("bytes", "mono · known nonzero totals only"),
            ("queued", "Tag +n queued · opens the queue popover"),
            (
                "queue",
                "menu surface · 288 · running (Spinner ×) · queued (layers ×)",
            ),
            ("waiting", "accent-warning line + bar · Show"),
        ],
        &[
            TokenChip::without_color("explorer-progress-height", "→ size-2"),
            TokenChip::new(
                "explorer-progress-track",
                "→ surface-raised",
                theme.explorer_progress_track().to_egui(),
            ),
            TokenChip::new(
                "explorer-progress-fill",
                "→ accent-primary",
                theme.explorer_progress_fill().to_egui(),
            ),
            TokenChip::new(
                "accent-warning",
                "waiting",
                theme.accent_warning().to_egui(),
            ),
        ],
    );
}

fn conflict_shell(ui: &mut egui::Ui, theme: &Theme, folder: bool) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_strong().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .shadow(theme.shadow_modal().to_egui())
        .show(ui, |ui| {
            ui.set_width(theme.explorer_conflict_width().value());
            let title = if folder {
                t_fmt("explorer.conflict.title_folder", "assets")
            } else {
                t_fmt("explorer.conflict.title", "report.pdf")
            };
            let in_folder = t_fmt("explorer.conflict.in", "~/Documents");
            let apply_all = t_fmt("explorer.conflict.apply_all", "3");
            let (existing, incoming) = if folder {
                (
                    t_fmt("explorer.conflict.folder_items", "214"),
                    t_fmt("explorer.conflict.folder_items", "37"),
                )
            } else {
                (
                    "2.4 MB · 2026-06-24 09:12".to_owned(),
                    "2.6 MB · 2026-10-09 10:02".to_owned(),
                )
            };
            let props = ConflictProps {
                title: &title,
                in_folder: &in_folder,
                existing_label: t("explorer.conflict.existing"),
                existing: &existing,
                incoming_label: t("explorer.conflict.incoming"),
                incoming: &incoming,
                no_merge: folder.then(|| t("explorer.conflict.no_merge")),
                apply_all: (!folder).then_some(apply_all.as_str()),
                cancel_rest: t("explorer.conflict.cancel_rest"),
                skip: t("explorer.conflict.skip"),
                replace: (!folder).then(|| t("explorer.conflict.replace")),
                keep_both: t("explorer.conflict.keep_both"),
            };
            let mut checked = false;
            // 갤러리 예제는 답을 받지 않는다.
            if conflict_card(ui, theme, &props, &mut checked).is_some() {
                ui.ctx().request_repaint();
            }
        });
}

pub fn draw_conflict(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
        conflict_shell(ui, theme, false);
        conflict_shell(ui, theme, true);
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "scope",
                "popup clamped to the explorer cell · scrim over the cell",
            ),
            ("width", "explorer-conflict-width 400"),
            (
                "title",
                "14 · “{name}” already exists · caption “in {folder}”",
            ),
            ("compare", "Existing / Incoming · mono caption"),
            (
                "buttons",
                "Cancel the rest (ghost, Esc) · Skip · Replace (files only) · Keep both (primary, Enter)",
            ),
            ("apply to all", "Checkbox, only when more conflicts remain"),
            (
                "focus",
                "opens only when the explorer has focus · otherwise the status line waits",
            ),
        ],
        &[
            TokenChip::without_color("explorer-conflict-width", "→ size-400"),
            TokenChip::new("scrim-bg", "cell dim", theme.scrim().to_egui()),
        ],
    );
}

pub fn draw_results(ui: &mut egui::Ui, theme: &Theme) {
    let close = glyph_painter(icons::CLOSE);
    let w = theme.toast_max_width().value();
    let card = |ui: &mut egui::Ui,
                kind: ToastKind,
                title: &str,
                lines: &[ResultLine<'_>],
                more: Option<&str>,
                actions: &[ResultAction<'_>]| {
        result_card(
            ui,
            theme,
            w,
            &ResultCardProps {
                kind,
                title,
                body: None,
                lines,
                more,
                actions,
                dismiss_tip: t("explorer.result.dismiss"),
            },
            &close,
        );
    };
    let retry3 = t_fmt("explorer.result.retry", "3");
    let retry12 = t_fmt("explorer.result.retry", "12");
    let retry28 = t_fmt("explorer.result.retry", "28");
    let copy_paths = t("explorer.result.copy_paths");
    let not_removed = t_fmt(
        "explorer.result.source_not_removed",
        "Device or resource busy (os error 16)",
    );
    let failure_actions = |retry: &str| {
        [
            (retry.to_owned(), ButtonVariant::Secondary),
            (copy_paths.to_owned(), ButtonVariant::Ghost),
        ]
    };
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        card(
            ui,
            ToastKind::Success,
            &t_fmt2("explorer.result.copied", "40", "Documents"),
            &[],
            None,
            &[ResultAction {
                label: t("explorer.result.undo"),
                variant: ButtonVariant::Ghost,
            }],
        );
        card(
            ui,
            ToastKind::Info,
            &t_fmt2("explorer.result.cancelled_copy", "12", "40"),
            &[],
            None,
            // 본체 `card_actions` 와 같다 — 줄이 없으니 Copy paths 는 없고, 하지 않은 28개를 Retry 가 센다.
            &[ResultAction {
                label: &retry28,
                variant: ButtonVariant::Secondary,
            }],
        );
        let acts = failure_actions(&retry3);
        card(
            ui,
            ToastKind::Warning,
            &t_args("explorer.result.partial_move", &["37", "40", "3"]),
            &[
                ResultLine {
                    path: "~/Downloads/build/artifacts/app-release-unsigned.apk",
                    reason: "Permission denied (os error 13)",
                },
                ResultLine {
                    path: "~/Downloads/archive.zip",
                    reason: &not_removed,
                },
                ResultLine {
                    path: "~/Downloads/raw/IMG_2031.heic",
                    reason: "No space left on device (os error 28)",
                },
            ],
            None,
            &acts.each_ref().map(|(label, variant)| ResultAction {
                label,
                variant: *variant,
            }),
        );
        let acts = failure_actions(&retry12);
        card(
            ui,
            ToastKind::Error,
            &t_fmt("explorer.result.failed_copy", "12"),
            &[ResultLine {
                path: "~/Downloads/raw/IMG_2030.heic",
                reason: "No space left on device (os error 28)",
            }],
            Some(&t_fmt("explorer.result.more", "11")),
            &acts.each_ref().map(|(label, variant)| ResultAction {
                label,
                variant: *variant,
            }),
        );
        card(
            ui,
            ToastKind::Error,
            t("explorer.trash.unavailable"),
            &[],
            None,
            &[],
        );
        card(
            ui,
            ToastKind::Warning,
            &t_fmt("explorer.result.undo_partial_move", "2"),
            &[ResultLine {
                path: "~/Documents/report.pdf",
                reason: t("explorer.result.newer_there"),
            }],
            Some(&t_fmt("explorer.result.more", "1")),
            &[],
        );
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "placement",
                "inside the cell · bottom-right · above the status line · stack upward",
            ),
            ("done", "success · Undo (ghost) · standard time"),
            (
                "cancelled",
                "info · Retry n (failed · skipped · not done, never the finished) · stays · standard time when nothing is left",
            ),
            (
                "partial / failed",
                "warning / danger · stays · ≤ 3 paths + and n more · Retry n · Copy paths",
            ),
            (
                "path",
                "mono caption · ellipsis at the front · reason under it (muted)",
            ),
            ("trash unavailable", "danger · no fallback"),
            ("dismiss", "× on every result"),
        ],
        &[
            TokenChip::new(
                "toast-accent-success",
                "done",
                theme.toast_accent_success().to_egui(),
            ),
            TokenChip::new(
                "toast-accent-warning",
                "partial",
                theme.toast_accent_warning().to_egui(),
            ),
            TokenChip::new(
                "toast-accent-danger",
                "failed",
                theme.toast_accent_danger().to_egui(),
            ),
            TokenChip::without_color("toast-max-width", "320"),
        ],
    );
}

/// 다른 디스크로 옮기다 원본이 남은 카드와, Undo 가 바뀐 사본을 남긴 카드. 동작 버튼은 본체
/// `card_actions` 와 같다 — 원본이 남은 카드의 Retry 는 남은 원본 지우기만 다시 하고, 바뀐 사본을
/// 남긴 카드에는 Copy paths 만 붙는다.
pub fn draw_source_left(ui: &mut egui::Ui, theme: &Theme) {
    let close = glyph_painter(icons::CLOSE);
    let w = theme.toast_max_width().value();
    let copy_paths = t("explorer.result.copy_paths");
    let copy_only = [ResultAction {
        label: copy_paths,
        variant: ButtonVariant::Ghost,
    }];
    let retry2 = t_fmt("explorer.result.retry", "2");
    let retry_delete = [
        ResultAction {
            label: &retry2,
            variant: ButtonVariant::Secondary,
        },
        ResultAction {
            label: copy_paths,
            variant: ButtonVariant::Ghost,
        },
    ];
    let denied = t_fmt(
        "explorer.result.source_not_removed",
        "Permission denied (os error 13)",
    );
    let in_use = t_fmt(
        "explorer.result.source_not_removed",
        "Device or resource busy (os error 16)",
    );
    let left_title = t_args("explorer.result.source_left_move", &["40", "40", "2"]);
    let undo_title = t_count("explorer.result.undo_kept_copy", 1, &["1"]);
    let left_lines = [
        ResultLine {
            path: "~/Volumes/usb/photos/2026-09",
            reason: &denied,
        },
        ResultLine {
            path: "~/Volumes/usb/archive.zip",
            reason: &in_use,
        },
    ];
    let kept_lines = [ResultLine {
        path: "~/Documents/notes.md",
        reason: t("explorer.result.changed_kept"),
    }];
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        for (title, lines, actions) in [
            (left_title.as_str(), &left_lines[..], &retry_delete[..]),
            (undo_title.as_str(), &kept_lines[..], &copy_only[..]),
        ] {
            result_card(
                ui,
                theme,
                w,
                &ResultCardProps {
                    kind: ToastKind::Warning,
                    title,
                    body: None,
                    lines,
                    more: None,
                    actions,
                    dismiss_tip: t("explorer.result.dismiss"),
                },
                &close,
            );
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "source left",
                "warning · stays · no Undo · Retry n (deletes only what is left) · Copy paths",
            ),
            ("title", "explorer.result.source_left_move"),
            ("line", "explorer.result.source_not_removed · reason in ( )"),
            (
                "undo kept",
                "explorer.result.changed_kept on the undo result line",
            ),
        ],
        &[TokenChip::new(
            "toast-accent-warning",
            "both",
            theme.toast_accent_warning().to_egui(),
        )],
    );
}

pub fn draw_drag(ui: &mut egui::Ui, theme: &Theme) {
    let items = t_fmt("explorer.drag.items", "3");
    let move_to = t_fmt("explorer.drag.move_to", "Archive");
    let copy_to = t_fmt("explorer.drag.copy_to", "Backup");
    let refused_reason = t_fmt(
        "explorer.drag.refused_reason",
        t("explorer.drag.into_itself"),
    );
    let file = glyph_painter(icons::FILE);
    let layers = glyph_painter(icons::LAYERS);
    let moved = glyph_painter(icons::MOVE);
    let plus = glyph_painter(icons::PLUS);
    let close = glyph_painter(icons::CLOSE);
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
        drag_chip(
            ui,
            theme,
            &DragChipProps {
                label: "report.pdf",
                line: &move_to,
                reason: None,
                op: DragOp::Move,
            },
            &file,
            &moved,
        );
        drag_chip(
            ui,
            theme,
            &DragChipProps {
                label: &items,
                line: &copy_to,
                reason: None,
                op: DragOp::Copy,
            },
            &layers,
            &plus,
        );
        drag_chip(
            ui,
            theme,
            &DragChipProps {
                label: "src",
                line: t("explorer.drag.refused"),
                reason: Some(&refused_reason),
                op: DragOp::Refused,
            },
            &file,
            &close,
        );
        let (row, _) = ui.allocate_exact_size(
            egui::vec2(ROW_W.value(), theme.item_height_interactive.value()),
            egui::Sense::hover(),
        );
        let radius = theme.corner_radius.value();
        paint_drop_tint(ui.painter(), theme, row, radius);
        let g = theme.icon_glyph_size_sm.value();
        let glyph = egui::Rect::from_center_size(
            egui::pos2(
                row.left() + theme.spacing_sm.value() + g / 2.0,
                row.center().y,
            ),
            egui::vec2(g, g),
        );
        icons::FOLDER
            .image(g, theme.text_secondary().to_egui())
            .paint_at(ui, glyph);
        ui.painter().text(
            egui::pos2(glyph.right() + theme.spacing_sm.value(), row.center().y),
            egui::Align2::LEFT_CENTER,
            "Archive",
            egui::FontId::proportional(theme.font_size_body.value()),
            theme.text_primary().to_egui(),
        );
        paint_drop_ring(ui.painter(), theme, row, radius);
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "chip",
                "surface-raised · 1px border-strong · shadow-popover · max 240 · pointer +12 / +12",
            ),
            ("line 2", "caption · op glyph · op colour · target folder"),
            (
                "default op",
                "same disk → move · other disk → copy · modifier flips",
            ),
            (
                "refused",
                "onto itself / own subfolder · already there · remote · no write access",
            ),
            ("target", "inset 1px ring + tint fill · no layout change"),
            (
                "auto-scroll",
                "listing and sidebar tree · 24px band · 20 rows/s × t² · saturated outside · no indicator",
            ),
            (
                "closed tree folder",
                "explorer-drag-expand-delay (800 ms) · existing drop-target ring while waiting",
            ),
            (
                "OS files",
                "copied into the folder · window drop overlay not drawn over explorer cells",
            ),
        ],
        &[
            TokenChip::new(
                "explorer-drag-move-fg",
                "→ accent-primary",
                theme.explorer_drag_move_fg().to_egui(),
            ),
            TokenChip::new(
                "explorer-drag-copy-fg",
                "→ accent-success",
                theme.explorer_drag_copy_fg().to_egui(),
            ),
            TokenChip::new(
                "explorer-drag-refused-fg",
                "→ accent-danger",
                theme.explorer_drag_refused_fg().to_egui(),
            ),
            TokenChip::new(
                "explorer-drop-target-bg",
                "accent-primary × tint-fill-alpha",
                theme.explorer_drop_target_bg().to_egui(),
            ),
            TokenChip::without_color("explorer-drag-chip-max-width", "→ size-240"),
        ],
    );
}

/// explorer 절에 더하는 파일 작업 예제들. 동결된 catalog.rs 를 늘리지 않도록 여기에 둔다.
pub fn specs() -> Vec<crate::catalog::Spec> {
    use crate::catalog::Spec;
    let local = [
        Spec {
            id: "explorer-ops-progress",
            title: "File operation progress — status line · queue · waiting",
            when: Some("on the status line of the explorer that started the job"),
            draw: draw_progress,
        },
        Spec {
            id: "explorer-ops-conflict",
            title: "Name conflict — file (apply to all) · folder (no Replace)",
            when: Some("Keep both is the default · Replace only for files"),
            draw: draw_conflict,
        },
        Spec {
            id: "explorer-ops-results",
            title: "Results — done · cancelled · partial · failed · trash · undo",
            when: Some("cards inside the cell · failures stay until dismissed"),
            draw: draw_results,
        },
        Spec {
            id: "explorer-ops-results-source-left",
            title: "Results — originals left after a cross-disk move · Undo kept a changed copy",
            when: Some("warning · stays until dismissed · no Undo"),
            draw: draw_source_left,
        },
        Spec {
            id: "explorer-ops-results-staging-left",
            title: "Results — temporary folder left behind · done · cancelled · partial · undo",
            when: Some("warning at least · folder line after the items · stays until dismissed"),
            draw: staging::draw_staging_left,
        },
        Spec {
            id: "explorer-ops-drag",
            title: "Drag chip — move · copy · refused · drop target mark",
            when: Some("the chip says what the drop will do before release"),
            draw: draw_drag,
        },
        Spec {
            id: "explorer-ops-refused",
            title: "Refused request — queue full (Show queue) · too large",
            when: Some("warning card in the cell · standard time · not a result"),
            draw: originals::draw_refused,
        },
        Spec {
            id: "explorer-ops-remove-originals",
            title: "Retry of originals — checking · removing · removed · kept",
            when: Some("own progress and result words · one card per Retry press"),
            draw: originals::draw_remove_originals,
        },
        Spec {
            id: "explorer-keyboard-select",
            title: "Keyboard current item · drag-select rectangle",
            when: Some("the ring after a key · the rectangle while dragging from empty space"),
            draw: super::explorer_select::draw,
        },
    ];
    local.into_iter().chain(remote::specs()).collect()
}
