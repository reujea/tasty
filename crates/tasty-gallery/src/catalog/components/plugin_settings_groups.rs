//! 묶음이 있는 플러그인 설정 페이지 예제. 스위치 없는 머리는 늘 펼쳐 두고, 스위치 머리를 끄면
//! 그 묶음의 행을 그리지 않는다(본체 `draw_plugin_settings_page` 의 묶음 규칙). 스위치 행과 묶음
//! 머리는 `caption_key` 설명을 아래에 둔다. 읽기 전용 글 행은 고칠 수 없는 문구를 컨트롤 열에서 줄을
//! 바꿔 보이고(`kind = "text"`), `editable` 글은 CodeArea 로 고친다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    CodeArea, Input, SettingsRow, StackRow, settings_group_header, settings_label_column,
    settings_row_caption, settings_stack_row, settings_text_row, switch,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

/// 설정 상세 영역 폭 근사 — `plugin_settings` specimen 과 같은 무대.
const WIDTH: LogicalPx = LogicalPx(440.0);

struct State {
    history_turns: String,
    swap: bool,
    working_on: bool,
    working_tokens: String,
    idle_on: bool,
    idle_tokens: String,
    instruction: String,
}

thread_local! {
    /// 상호작용 상태 — 둘째 스위치 묶음은 꺼 둔 채로 시작해 숨김 상태를 함께 보인다.
    static STATE: RefCell<State> = RefCell::new(State {
        history_turns: String::from("0"),
        swap: false,
        working_on: true,
        working_tokens: String::from("400000"),
        idle_on: false,
        idle_tokens: String::from("150000"),
        instruction: String::from("project sync"),
    });
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                let labels = [
                    "History",
                    "Turns kept",
                    "Move overflow to disk",
                    "Sync while working",
                    "Sync tokens",
                    "Sync when idle",
                    "Prompts",
                    "Check instruction",
                    "Completion marker",
                    "Sync instruction",
                ]
                .map(SettingsRow::new);
                let col = settings_label_column(ui, theme, &labels);
                STATE.with(|s| {
                    let st = &mut *s.borrow_mut();
                    // 스위치 없는 머리 — 행은 늘 보인다.
                    settings_group_header(ui, theme, col, "History", None, true);
                    number_row(ui, theme, col, "Turns kept", &mut st.history_turns, "turns");
                    // 스위치 행 아래 설명(`caption_key`).
                    ui.add_space(theme.spacing_sm.value());
                    SettingsRow::new("Move overflow to disk")
                        .caption("Turns past the limit go to a file instead of being dropped.")
                        .show(ui, theme, col, |ui| {
                            switch(ui, theme, &mut st.swap, None, true);
                        });
                    // 스위치 머리 — 켜면 행이 보인다.
                    settings_group_header(
                        ui,
                        theme,
                        col,
                        "Sync while working",
                        Some(&mut st.working_on),
                        false,
                    );
                    // 묶음 머리 아래 설명 — 스위치를 꺼도 남는다.
                    ui.add_space(theme.settings_row_caption_gap().value());
                    settings_row_caption(
                        ui,
                        theme,
                        "Runs project sync, then compact, when the context reaches the threshold.",
                    );
                    if st.working_on {
                        number_row(
                            ui,
                            theme,
                            col,
                            "Sync tokens",
                            &mut st.working_tokens,
                            "tokens",
                        );
                    }
                    // 스위치 머리 — 끄면 행을 그리지 않는다.
                    settings_group_header(
                        ui,
                        theme,
                        col,
                        "Sync when idle",
                        Some(&mut st.idle_on),
                        false,
                    );
                    if st.idle_on {
                        number_row(ui, theme, col, "Sync tokens", &mut st.idle_tokens, "tokens");
                    }
                    // 스위치 없는 머리 아래 읽기 전용 글 — 긴 문구는 컨트롤 열에서 줄을 바꾼다.
                    settings_group_header(ui, theme, col, "Prompts", None, false);
                    ui.add_space(theme.spacing_sm.value());
                    settings_text_row(
                        ui,
                        theme,
                        col,
                        "Check instruction",
                        "Before ending the turn, check that the result meets the request, \
                         that it was verified by running it, and that nothing is left undone.",
                    );
                    // 고칠 수 있는 글(`editable`) — 비우면 기본 글을 자리표시로 보인다.
                    ui.add_space(theme.spacing_sm.value());
                    let edit = StackRow {
                        id: ui.id().with("gallery_plugin_text_edit"),
                        label_col: col,
                        row_h: theme.settings_row_min_height(),
                        label: "Sync instruction",
                        hint: None,
                        align_top: true,
                    };
                    settings_stack_row(ui, theme, edit, |ui| {
                        CodeArea::new("gallery_plugin_text_edit")
                            .placeholder("project sync")
                            .min_rows(2)
                            .show(ui, theme, &mut st.instruction);
                    });
                    ui.add_space(theme.spacing_sm.value());
                    settings_text_row(ui, theme, col, "Completion marker", "[[DONE]]");
                });
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("header", "label column · font_size_heading · text-primary"),
            (
                "switch",
                "control column after settings-label-gap (28×16 track)",
            ),
            ("divider", "border_width · border-default, spacing_md above"),
            ("header gap", "spacing_sm below the divider"),
            ("plain group", "no switch — rows always shown"),
            ("switched group", "off hides the group's rows"),
            (
                "text row",
                "read-only · font_size_body · text-secondary · wraps · label top",
            ),
            (
                "caption",
                "below a switch row or a group header · settings-row-caption-gap",
            ),
            (
                "editable text",
                "CodeArea in the control column · label top",
            ),
        ],
        &[
            TokenChip::new(
                "text-primary",
                "group label",
                theme.text_primary().to_egui(),
            ),
            TokenChip::new(
                "text-secondary",
                "row label",
                theme.text_secondary().to_egui(),
            ),
            TokenChip::new(
                "border-default",
                "group divider",
                theme.border_default().to_egui(),
            ),
            TokenChip::new(
                "switch-track-bg-on",
                "group switch on",
                theme.switch_track_bg_on().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Groups come from manifest items of kind group. A group with a storage key carries a \
         switch; turning it off hides its rows and the plugin treats the feature as off.",
    );
}

/// 본체 `plugin_setting_row` 미러 — 앞에 `spacing_sm` 여백을 두고 설정 행 격자로 그린다.
fn row(
    ui: &mut egui::Ui,
    theme: &Theme,
    col: LogicalPx,
    label: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(theme.spacing_sm.value());
    SettingsRow::new(label).show(ui, theme, col, control);
}

/// 숫자 행 — mono Input + 필드 밖 정적 suffix(숫자 한 모양).
fn number_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    col: LogicalPx,
    label: &str,
    buf: &mut String,
    suffix: &str,
) {
    row(ui, theme, col, label, |ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        Input::new()
            .mono(true)
            .align(egui::Align::RIGHT)
            .width(theme.field_width_xs.value())
            .show(ui, theme, buf);
        ui.label(
            egui::RichText::new(suffix)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
    });
}
