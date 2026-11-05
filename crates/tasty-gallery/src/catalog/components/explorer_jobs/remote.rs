//! 원격(attach) 탐색기의 쓰기 — 디자인 `gallery/explorer-ops-remote.jsx`(#remotewrite) 예제를 옮겼다.
//! 쓸 수 없는 원격의 꺼진 쓰기 행과 이유, 원격의 답을 기다리는 이름 입력, 연결이 끊긴 결과 카드,
//! 진행 줄·대기열의 호스트, 드래그 칩의 거절 이유. 그리기는 본체와 같은 공용 위젯이 맡는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_appearance::toast_kind::ToastKind;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    DragChipProps, DragOp, ExplorerNameEdit, ExplorerNameLayout, ExplorerNameOptions, HostMark,
    MenuItemVariant, OpStatusProps, QueueHeader, QueueRowProps, ResultCardProps, drag_chip,
    explorer_name_row_in, menu_item, menu_separator, op_queue_popover, op_status_line, result_card,
};

use super::{BODY_H, CELL_W, QUEUE_BODY_H, cell, glyph_painter};
use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::{t, t_args, t_fmt, t_fmt2};

/// 디자인 예제의 원격 호스트 이름.
const HOST: &str = "build-eu";
/// 디자인 컨텍스트 메뉴 폭(`K.CtxMenu`, 220). 전시 치수다.
const MENU_W: LogicalPx = LogicalPx(220.0);
/// 디자인 툴팁 표본의 최대 폭(`--tasty-size-240`). 역할 토큰이 없어 전시 치수로 둔다.
const TIP_MAX_W: LogicalPx = LogicalPx(240.0);

/// (글리프, 라벨, 켜짐, 위험 행)
type Row<'a> = (Option<MockGlyph>, &'a str, bool, bool);

fn menu(ui: &mut egui::Ui, theme: &Theme, caption: &str, rows: &[Option<Row<'_>>]) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        ui.label(
            egui::RichText::new(caption.to_uppercase())
                .size(theme.font_size_micro.value())
                .color(theme.text_muted_raised().to_egui()),
        );
        egui::Frame::new()
            .fill(theme.surface_raised().to_egui())
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                theme.border_strong().to_egui(),
            ))
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
            .show(ui, |ui| {
                ui.set_width(
                    MENU_W.value()
                        - theme.spacing_xs.value() * 2.0
                        - theme.border_width.value() * 2.0,
                );
                ui.spacing_mut().item_spacing.y = 0.0;
                for row in rows {
                    let Some((glyph, label, enabled, danger)) = row else {
                        menu_separator(ui, theme);
                        continue;
                    };
                    let variant = if *danger && *enabled {
                        MenuItemVariant::Danger
                    } else {
                        MenuItemVariant::Normal
                    };
                    let paint = glyph.map(glyph_painter);
                    menu_item(
                        ui,
                        theme,
                        paint
                            .as_ref()
                            .map(|p| p as &dyn Fn(&mut egui::Ui, egui::Rect, egui::Color32)),
                        label,
                        None,
                        variant,
                        false,
                        *enabled,
                    );
                }
            });
    });
}

/// 꺼진 행에 마우스를 올렸을 때의 툴팁 모양. 본체 네이티브 메뉴가 그리는 툴팁을 고정 표본으로 보인다.
fn tip(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    egui::Frame::new()
        .fill(theme.menu_bg().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.menu_border().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_sm.value() as i8,
            theme.spacing_xs.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_max_width(TIP_MAX_W.value());
            ui.add(
                egui::Label::new(
                    egui::RichText::new(text)
                        .size(theme.font_size_caption.value())
                        .color(theme.text_primary().to_egui()),
                )
                .wrap(),
            );
        });
}

/// D1 — 쓸 수 없는 원격은 쓰기 행을 숨기지 않고 꺼 두며, 이유를 툴팁과 단축키 토스트로 알린다.
pub fn draw_write_refused(ui: &mut egui::Ui, theme: &Theme) {
    let reason = t_fmt("explorer.remote.write_needs_update", HOST);
    let close = glyph_painter(icons::CLOSE);
    let rows = |writable: bool| -> Vec<Option<Row<'static>>> {
        vec![
            Some((
                Some(icons::COPY),
                t("explorer.context_menu.copy_path"),
                true,
                false,
            )),
            Some((
                Some(icons::COPY),
                t("explorer.context_menu.copy_files"),
                true,
                false,
            )),
            Some((None, t("explorer.context_menu.cut"), writable, false)),
            Some((None, t("explorer.context_menu.paste_into"), writable, false)),
            None,
            Some((
                Some(icons::FOLDER_PLUS),
                t("explorer.command.new_folder"),
                writable,
                false,
            )),
            Some((
                Some(icons::FILE_PLUS),
                t("explorer.command.new_file"),
                writable,
                false,
            )),
            Some((
                Some(icons::EDIT),
                t("explorer.context_menu.rename"),
                writable,
                false,
            )),
            Some((
                Some(icons::TRASH),
                t("explorer.context_menu.delete"),
                writable,
                true,
            )),
        ]
    };
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing =
            egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
        menu(ui, theme, "writable remote (new Tasty)", &rows(true));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            menu(
                ui,
                theme,
                "remote that can't write — disabled",
                &rows(false),
            );
            tip(ui, theme, &reason);
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            ui.label(
                egui::RichText::new("SHORTCUT — INFO TOAST IN THE CELL")
                    .size(theme.font_size_micro.value())
                    .color(theme.text_muted().to_egui()),
            );
            result_card(
                ui,
                theme,
                theme.toast_max_width().value(),
                &ResultCardProps {
                    kind: ToastKind::Info,
                    title: &reason,
                    body: None,
                    lines: &[],
                    more: None,
                    actions: &[],
                    dismiss_tip: t("explorer.result.dismiss"),
                },
                &close,
            );
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("choice", "show disabled · tooltip = reason (Linux / macOS)"),
            (
                "rows",
                "Cut · Paste · New folder · New file · Rename · Move to Trash",
            ),
            ("read rows", "Copy path · Copy · Open stay enabled"),
            ("toolbar", "create group disabled · tooltip = reason"),
            (
                "shortcut",
                "info toast in the cell · the same reason string",
            ),
            (
                "reason",
                "older Tasty on the remote → update hint with the host",
            ),
        ],
        &[
            TokenChip::new(
                "state-disabled-fg",
                "disabled row / button",
                theme.state_disabled_fg().to_egui(),
            ),
            TokenChip::new(
                "toast-accent-info",
                "reason toast",
                theme.toast_accent_info().to_egui(),
            ),
        ],
    );
}

/// D2 — 원격의 답을 기다리는 동안 새 항목 줄은 제자리에 읽기 전용으로 남고 Spinner 를 단다.
/// 이름 변경 팝업은 열린 채 입력과 두 버튼을 막고 대기 줄을 보인다.
pub fn draw_remote_waiting(ui: &mut egui::Ui, theme: &Theme) {
    let waiting = t_fmt("explorer.remote.waiting", HOST);
    let renaming = t_fmt("explorer.remote.renaming", HOST);
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        let (rect, _) = cell(ui, theme, BODY_H);
        let row = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width(), theme.table_cell_height().value()),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row));
        let mut edit = ExplorerNameEdit {
            buf: t("explorer.new.folder_default").to_owned(),
            initial_selection: None,
        };
        explorer_name_row_in(
            &mut child,
            theme,
            ExplorerNameLayout::List,
            ExplorerNameOptions {
                pending: Some(&waiting),
                ..Default::default()
            },
            icons::FOLDER,
            &mut edit,
            false,
        );
        ui.add_space(theme.spacing_md.value());
        rename_card(ui, theme, Some(&renaming));
    });
    spec::meta(
        ui,
        theme,
        &[
            ("input row", "stays in place · Input readOnly · text kept"),
            (
                "indicator",
                "Spinner icon-size-sm · space-sm after the field · tooltip “Waiting for {host}”",
            ),
            ("Esc / blur", "ignored while waiting"),
            ("success", "row → item at its sorted place, selected"),
            (
                "failure",
                "field editable again, text kept · error toast (unchanged)",
            ),
            (
                "Rename popup",
                "open · field readOnly · Spinner line · Cancel + Rename disabled · closes on success",
            ),
        ],
        &[
            TokenChip::new(
                "input-readonly-fg",
                "sent text",
                theme.input_readonly_fg().to_egui(),
            ),
            TokenChip::new(
                "spinner-indicator",
                "waiting",
                theme.spinner_indicator().to_egui(),
            ),
        ],
    );
}

/// 본체 rename 팝업과 같은 공용 view 의 예제 한 장. 바탕은 dialog-bg 다. `waiting` 이 있으면 이름 칸을
/// 읽기 전용으로 두고 Spinner 대기 줄을 보이며 두 버튼을 끈다.
fn rename_card(ui: &mut egui::Ui, theme: &Theme, waiting: Option<&str>) {
    crate::catalog::components::rename_popup::card(
        ui,
        theme,
        crate::catalog::components::rename_popup::Card {
            layout: tasty_ui_widgets::RenamePopupLayout::Explorer,
            title: t("explorer.popup.rename.title"),
            save: t("button.save"),
            name: "report-final.pdf",
            error: None,
            waiting,
        },
    );
}

/// rename 팝업 바탕 — 평소와 원격 대기 중을 나란히 둔다. 팝업은 dialog 라 바탕이 dialog-bg(bg-panel)이고,
/// 그 위의 읽기 전용 칸은 기본 상자(state-disabled-fill · state-disabled-border)로 경계가 보인다.
pub fn draw_rename_ground(ui: &mut egui::Ui, theme: &Theme) {
    let renaming = t_fmt("explorer.remote.renaming", HOST);
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "normal", |ui| rename_card(ui, theme, None));
        spec::cluster(ui, theme, "waiting — field read-only", |ui| {
            rename_card(ui, theme, Some(&renaming))
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "rename popup ground",
                "dialog-bg → bg-panel · every rename popup",
            ),
            ("field · normal", "Input · input-bg + input-border"),
            (
                "field · waiting",
                "Input readOnly · input-readonly-bg + input-readonly-border on dialog-bg",
            ),
            ("ink", "readonly input-readonly-fg · buttons disabled"),
        ],
        &[
            TokenChip::new("dialog-bg", "popup ground", theme.dialog_bg().to_egui()),
            TokenChip::new("input-bg", "normal field", theme.input_bg().to_egui()),
            TokenChip::new(
                "input-readonly-bg",
                "waiting field",
                theme.input_readonly_bg().to_egui(),
            ),
            TokenChip::new(
                "input-readonly-border",
                "waiting edge",
                theme.input_readonly_border().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Same layout as the product popup: field, waiting line, Cancel and Save. The product \
         popup has no key hint line, so the normal card shows none. Inputs that sit on \
         surface-raised use the raised ground shown with the Input primitive.",
    );
}

/// D3 — 결과를 받기 전에 연결이 끊긴 작업. 경고 카드는 닫을 때까지 남고 × 만 둔다.
pub fn draw_connection_lost(ui: &mut egui::Ui, theme: &Theme) {
    let close = glyph_painter(icons::CLOSE);
    let title = t_fmt("explorer.remote.lost_title", HOST);
    let body = t_fmt("explorer.remote.lost_body", t("explorer.remote.op_copy"));
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        result_card(
            ui,
            theme,
            theme.toast_max_width().value(),
            &ResultCardProps {
                kind: ToastKind::Warning,
                title: &title,
                body: Some(&body),
                lines: &[],
                more: None,
                actions: &[],
                dismiss_tip: t("explorer.result.dismiss"),
            },
            &close,
        );
    });
    spec::meta(
        ui,
        theme,
        &[
            ("tone", "warning · stays until dismissed"),
            ("title", "Connection to {host} lost · result unknown"),
            (
                "body",
                "caption · text-secondary · stopped at the item · may have changed · reload on reconnect",
            ),
            ("actions", "× only — no Retry, no Undo"),
            ("conflict popup", "closes with the job"),
            (
                "verb",
                "{op} = copy · move · move to Trash · rename · new folder · new file",
            ),
        ],
        &[TokenChip::new(
            "toast-accent-warning",
            "card",
            theme.toast_accent_warning().to_egui(),
        )],
    );
}

/// D4 — 원격 작업의 진행 줄 맨 앞과 대기열 머리에 호스트를 보인다.
pub fn draw_remote_progress(ui: &mut egui::Ui, theme: &Theme) {
    let close = glyph_painter(icons::CLOSE);
    let layers = glyph_painter(icons::LAYERS);
    let remote = glyph_painter(icons::REMOTE);
    let copying = t_args(
        "explorer.op.copying",
        &["2", "6", "nginx-access-2026-10-11.log.gz"],
    );
    let bytes = t_fmt2("explorer.op.bytes", "14 MB", "52 MB");
    let queued = t_fmt("explorer.op.queued", "1");
    let queue_copy = t_fmt2("explorer.op.queue_copy", "6", "logs/archive");
    let queue_trash = t_fmt("explorer.op.queue_trash", "3");
    let queue_sub = format!(
        "{} · {bytes}",
        t_fmt2("explorer.op.queue_progress", "2", "6")
    );
    let title = t("explorer.remote.queue_title");
    let (before, after) = title.split_once("{}").unwrap_or((title, ""));
    let mark = HostMark {
        name: HOST,
        paint_glyph: &remote,
    };
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.set_width(CELL_W.value());
        let (rect, status) = cell(ui, theme, QUEUE_BODY_H);
        op_status_line(
            ui,
            theme,
            status,
            &OpStatusProps {
                show_bar: true,
                fraction: Some(0.27),
                waiting: false,
                text: &copying,
                bytes: Some(&bytes),
                queued: Some(&queued),
                show_label: t("explorer.op.show"),
                cancel_tip: t("explorer.op.cancel"),
                host: Some(mark),
            },
            &close,
        );
        op_queue_popover(
            ui,
            theme,
            egui::Id::new("explorer_remote_queue_specimen"),
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
                    title: &queue_trash,
                    sub: t("explorer.op.queue_waiting"),
                    running: false,
                    remove_tip: t("explorer.op.remove_from_queue"),
                },
            ],
            Some(&QueueHeader {
                before,
                host: mark,
                after,
            }),
            &close,
            &layers,
        );
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "host mark",
                "remote glyph (workspace-mirror-icon-size) + host · mono caption · workspace-mirror-fg · first on the line",
            ),
            ("queued", "“+n queued” = this cell's remote queue"),
            (
                "queue header",
                "“On {host} · one at a time” · caption · muted",
            ),
            ("rest", "same bar · Cancel · rows as local"),
        ],
        &[
            TokenChip::new(
                "workspace-mirror-fg",
                "host mark",
                theme.workspace_mirror_fg().to_egui(),
            ),
            TokenChip::without_color("workspace-mirror-gap", "glyph ↔ host"),
        ],
    );
}

/// D5 — 다른 컴퓨터 사이와 쓸 수 없는 원격으로의 끌어 놓기 거절 이유.
pub fn draw_remote_drag(ui: &mut egui::Ui, theme: &Theme) {
    let items = t_fmt("explorer.drag.items", "3");
    let move_to = t_fmt("explorer.drag.move_to", "conf.d");
    let other = t_fmt(
        "explorer.drag.refused_reason",
        t("explorer.drag.other_computer"),
    );
    let remote = t_fmt("explorer.drag.refused_reason", t("explorer.drag.remote"));
    let file = glyph_painter(icons::FILE);
    let layers = glyph_painter(icons::LAYERS);
    let moved = glyph_painter(icons::MOVE);
    let refused = glyph_painter(icons::CLOSE);
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        drag_chip(
            ui,
            theme,
            &DragChipProps {
                label: "nginx.conf",
                line: &move_to,
                reason: None,
                op: DragOp::Move,
            },
            &file,
            &moved,
        );
        for reason in [&other, &remote] {
            drag_chip(
                ui,
                theme,
                &DragChipProps {
                    label: &items,
                    line: t("explorer.drag.refused"),
                    reason: Some(reason),
                    op: DragOp::Refused,
                },
                &layers,
                &refused,
            );
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("inside one remote", "Move to / Copy to as local"),
            (
                "local ↔ remote · remote ↔ remote",
                "refused · “different computer”",
            ),
            (
                "remote can't write",
                "refused · “this remote can't change files”",
            ),
            ("conflict popup", "unchanged"),
        ],
        &[TokenChip::new(
            "explorer-drag-refused-fg",
            "refused line",
            theme.explorer_drag_refused_fg().to_egui(),
        )],
    );
}

pub fn specs() -> [crate::catalog::Spec; 6] {
    use crate::catalog::Spec;
    [
        Spec {
            id: "explorer-remote-write-refused",
            title: "Remote that can't write — rows disabled, reason in the tooltip and toast",
            when: Some("an older Tasty on the remote · the toolbar create group follows"),
            draw: draw_write_refused,
        },
        Spec {
            id: "explorer-remote-waiting",
            title: "Waiting for the remote — the input row stays, read-only, with a spinner",
            when: Some("remote new folder · new file · rename popup"),
            draw: draw_remote_waiting,
        },
        Spec {
            id: "explorer-rename-popup-ground",
            title: "Rename popup — the read-only field against the popup ground",
            when: Some("every rename popup is a dialog on dialog-bg · normal and waiting"),
            draw: draw_rename_ground,
        },
        Spec {
            id: "explorer-remote-connection-lost",
            title: "Remote connection lost — result unknown",
            when: Some("warning · stays until dismissed · × only"),
            draw: draw_connection_lost,
        },
        Spec {
            id: "explorer-remote-progress",
            title: "Remote progress line and queue — host shown",
            when: Some("remote jobs run one at a time per connection"),
            draw: draw_remote_progress,
        },
        Spec {
            id: "explorer-remote-drag",
            title: "Drag chip — different computer · remote can't change files",
            when: Some("drops inside one remote work like local"),
            draw: draw_remote_drag,
        },
    ]
}
