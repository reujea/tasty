//! 내보내기 실패, 여러 경고, 줄 번호 없는 파싱 오류 예제.
//! 실제 그리기는 entry·notices의 공통 함수를 사용한다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::ButtonVariant;

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};

use super::entry::action_row;
use super::notices::{dropped_notice, notice_block, notice_line, parse_failure};
use super::paint::{caption, intro_secondary};
use super::{IE_FILE, NOTICE_FOLD_AT, SPECIMEN_W, STATE, State};

/// 경고 블록 데모 줄 — 고정 순서(스키마 → 모르는 액션 → 빈 그룹). 넷째 줄은 접힌다.
const NOTICE_LINES: &[&str] = &[
    "Written by a newer schema (v3, this build reads v2) — unreadable parts were skipped.",
    "2 unknown actions were skipped (pane.zoom_cycle, tab.pin).",
    "The group [keybindings.image] is empty — nothing to import from it.",
    "The group [keybindings.markdown] is empty — nothing to import from it.",
];

pub fn draw_open_values(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        ui.vertical(|ui| {
            ui.set_width(SPECIMEN_W.value());
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            STATE.with(|s| {
                let st = &mut *s.borrow_mut();
                caption(ui, theme, "1 · export failure — inline in the Export row");
                export_failure_row(ui, theme, st);
                caption(
                    ui,
                    theme,
                    "2 · bundle warnings — one block, info line apart",
                );
                bundle_notices(ui, theme, st);
                caption(
                    ui,
                    theme,
                    "3 · parse failure — with and without a line number",
                );
                parse_failure(ui, theme, true);
                parse_failure(ui, theme, false);
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("export success", "toast with the resolved path"),
            ("export failure", "inline danger block in the Export row"),
            ("failure actions", "Try again · Choose another location…"),
            ("info line", "dropped overrides — muted, unchanged"),
            (
                "warning block",
                "one block, 1 line per notice, count in header",
            ),
            ("notice order", "schema → unknown actions → empty groups"),
            ("fold", "3 lines, then “Show {n} more”"),
            (
                "parse copy",
                "line clause replaced by “the file isn't TOML.”",
            ),
        ],
        &[
            TokenChip::new(
                "accent-warning",
                "warning block",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "failure blocks",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::without_color(
                "kb-ie-notice-inset",
                "12 — notice / card inset, both axes (NEW, was off-grid 14)",
            ),
            TokenChip::without_color("kb-ie-slot-min-width", "140 — slot chip min width (NEW)"),
            TokenChip::without_color("kb-ie-slot-height", "24 — replacement slot chip (NEW)"),
            TokenChip::without_color(
                "kb-ie-from-column-width",
                "120 — original shortcut column (NEW)",
            ),
            TokenChip::without_color(
                "kb-ie-action-column-width",
                "288 — action label column + sub-line indent (NEW)",
            ),
            TokenChip::without_color("kb-ie-select-column-width", "32 — diff select column (NEW)"),
            TokenChip::without_color(
                "settings-content-max-width",
                "620 — content column cap (NEW)",
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Warnings share one block. Informational notices stay outside it so the reader can distinguish problems from facts that need no action.",
    );
    spec::dont(
        ui,
        theme,
        "Don't toast an export failure. A toast auto-dismisses and carries no retry, so the one \
         case where the user must act is the one case that disappears on its own.",
    );
}

/// jsx `IeExportFailG` — Export 행(버튼 꺼짐) + 그 안의 실패 블록. 두 액션 모두 블록을 닫고
/// 버튼을 되살린다(체크박스 없이 눌러 볼 수 있게). 닫힌 뒤에는 캡션 옆 버튼이 다시 띄운다.
fn export_failure_row(ui: &mut egui::Ui, theme: &Theme, st: &mut State) {
    let failed = st.export_failed;
    let mut dismissed = false;
    let mut notice = |ui: &mut egui::Ui| {
        let clicked = notice_block(
            ui,
            theme,
            theme.accent_danger().to_egui(),
            icons::ALERT_CIRCLE,
            crate::i18n::t("settings.keybindings.ie_export_failure_title"),
            None,
            |ui| {
                intro_secondary(
                    ui,
                    theme,
                    &format!("~/tasty/{IE_FILE} — the folder is read-only. Nothing was written."),
                );
            },
            &[
                ("Try again", ButtonVariant::Secondary),
                (
                    crate::i18n::t("settings.keybindings.ie_export_choose_another"),
                    ButtonVariant::Ghost,
                ),
            ],
        );
        dismissed = clicked.is_some();
    };
    action_row(
        ui,
        theme,
        icons::DOWNLOAD,
        "Export",
        crate::i18n::t("settings.keybindings.ie_export_desc"),
        "Export…",
        ButtonVariant::Secondary,
        !failed,
        None,
        failed.then_some(&mut notice as &mut dyn FnMut(&mut egui::Ui)),
    );
    if dismissed {
        st.export_failed = false;
    }
    if !st.export_failed
        && tasty_ui_widgets::Button::new("Show the failure again")
            .variant(ButtonVariant::Ghost)
            .size(tasty_ui_widgets::ControlSize::Sm)
            .show(ui, theme)
            .clicked()
    {
        st.export_failed = true;
    }
}

/// jsx `IeBundleNoticesG` — 정보 줄(그대로) + 경고 블록 하나(개수 헤더 · 세 줄 · 접기).
fn bundle_notices(ui: &mut egui::Ui, theme: &Theme, st: &mut State) {
    dropped_notice(ui, theme);
    let hidden = if st.notices_expanded {
        0
    } else {
        NOTICE_LINES.len().saturating_sub(NOTICE_FOLD_AT)
    };
    let shown = NOTICE_LINES.len() - hidden;
    let more = format!("Show {hidden} more");
    let actions: &[(&str, ButtonVariant)] = if hidden > 0 {
        &[(&more, ButtonVariant::Ghost)]
    } else {
        &[]
    };
    let clicked = notice_block(
        ui,
        theme,
        theme.accent_warning().to_egui(),
        icons::ALERT_TRIANGLE,
        "Read with warnings",
        Some(&crate::i18n::t_count(
            "settings.keybindings.ie_notices_count",
            NOTICE_LINES.len() as u64,
            &[&NOTICE_LINES.len().to_string()],
        )),
        |ui| {
            for line in &NOTICE_LINES[..shown] {
                notice_line(ui, theme, line);
            }
        },
        actions,
    );
    if clicked.is_some() {
        st.notices_expanded = true;
    }
}
