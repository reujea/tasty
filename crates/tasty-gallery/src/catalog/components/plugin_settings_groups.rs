//! 묶음이 있는 플러그인 설정 페이지 예제. 스위치 없는 머리는 늘 펼쳐 두고, 스위치 머리를 끄면
//! 그 묶음의 행을 그리지 않는다(본체 `draw_plugin_settings_page` 의 묶음 규칙). 읽기 전용 글 행은
//! 고칠 수 없는 문구를 컨트롤 열에서 줄을 바꿔 보인다(`kind = "text"`).

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Input, SettingsRow, settings_group_header, settings_label_column, settings_text_row, switch,
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
                ]
                .map(SettingsRow::new);
                let col = settings_label_column(ui, theme, &labels);
                STATE.with(|s| {
                    let st = &mut *s.borrow_mut();
                    // 스위치 없는 머리 — 행은 늘 보인다.
                    settings_group_header(ui, theme, col, "History", None, true);
                    number_row(ui, theme, col, "Turns kept", &mut st.history_turns, "turns");
                    row(ui, theme, col, "Move overflow to disk", |ui| {
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
