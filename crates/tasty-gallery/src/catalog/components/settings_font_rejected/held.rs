//! 거절 줄의 칸이 눌림과 판정 대기를 지나는 단계. 시안 `font-file-press-close`, `font-file-press-open`,
//! `font-file-pending`, `font-file-no-pointer`, `font-file-use-default`, `font-file-press-dropped`,
//! `font-file-press-stacked`, `font-family-settle`.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{OverrideCell, SettingsRow, checkbox, override_row};

use crate::i18n::t_lang;

use super::{
    DROPPED_W, FRAME_W, Family, File, NOTES_TXT, STACKED_W, frame, override_family_row_with,
    override_file_row_checked, stage_row, state, themes,
};
use crate::catalog::components::settings_font_override as grid;

const NEW_FILE: &str = "~/fonts/New.ttf";
const FIRA: &str = "Fira Code";
const JETBRAINS: &str = "JetBrains Mono";

/// 시안 `BelowRow`: 칸이 움직일 때 클릭이 겨누는 아래 행.
pub(super) fn below_row(ui: &mut egui::Ui, th: &Theme, checked: &mut bool) {
    let col = th.settings_label_width();
    let label = grid::ROWS[2].0;
    let row = SettingsRow::new(label);
    let use_default = t_lang("en", "settings.appearance.font.use_default_label");
    override_row(
        ui,
        th,
        col,
        th.field_width_xs,
        use_default,
        |ui, cell| match cell {
            OverrideCell::Label => {
                row.show_label(ui, th, col, th.settings_row_min_height());
            }
            OverrideCell::Control => {
                let mut value = "14".to_owned();
                grid::control(ui, th, 2, &mut value, true);
            }
            OverrideCell::UseDefault => {
                checkbox(
                    ui,
                    th,
                    checked,
                    use_default,
                    true,
                    tasty_ui_widgets::Ground::Panel,
                );
            }
        },
    );
}

/// 파일 행과 아래 행 한 단계.
fn file_step(ui: &mut egui::Ui, th: &Theme, name: &str, value: &str, st: File, below: bool) {
    state(ui, th, name, |ui| {
        override_file_row_checked(ui, th, "en", value, st, st.default_on());
        below_row(ui, th, &mut { below });
    });
}

/// 좁은 폭에서 자기 체크박스를 누르는 세 단계. 시안의 `udChecked` 는 파일 행의 체크박스다.
fn own_checkbox_steps(ui: &mut egui::Ui, th: &Theme) {
    let row = |ui: &mut egui::Ui, st: File, checked: bool| {
        override_file_row_checked(ui, th, "en", "", st, checked)
    };
    state(
        ui,
        th,
        "press on checkbox · focus gone — box kept, checkbox stays",
        |ui| row(ui, File::Held, false),
    );
    state(
        ui,
        th,
        "release — checkbox toggles, box kept this frame",
        |ui| row(ui, File::Held, true),
    );
    state(ui, th, "next frame — box closes, checkbox rises", |ui| {
        row(ui, File::Rest, true)
    });
}

fn stage_frames(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    width: LogicalPx,
    add: impl Fn(&mut egui::Ui, &Theme),
) {
    stage_row(ui, theme, |ui| {
        for (name, th) in &themes(theme) {
            frame(ui, th, &format!("{name} · {title}"), width, &add);
        }
    });
}

/// 파일 Spec 의 단계 프레임들.
pub(super) fn draw_file_steps(ui: &mut egui::Ui, theme: &Theme) {
    stage_frames(
        ui,
        theme,
        "Clicking the row below to leave an emptied value",
        FRAME_W,
        |ui, th| {
            let steps = [
                (
                    "focused · emptied — box kept, no ink",
                    File::FocusRejected,
                    false,
                ),
                (
                    "press on row below · focus gone — box kept, row below stays",
                    File::Held,
                    false,
                ),
                (
                    "release — checkbox toggles, box kept this frame",
                    File::Held,
                    true,
                ),
                ("next frame — box closes, row below rises", File::Rest, true),
            ];
            for (name, st, below) in steps {
                file_step(ui, th, name, "", st, below);
            }
        },
    );
    stage_frames(
        ui,
        theme,
        "Clicking the row below to leave a rejected value, with no box",
        FRAME_W,
        |ui, th| {
            let steps = [
                ("focused · no box — nothing red", File::Focus, false),
                (
                    "press on row below — edge shows, no line, row below stays",
                    File::Due,
                    false,
                ),
                ("release — checkbox toggles, still no line", File::Due, true),
                (
                    "next frame — line opens, row below drops",
                    File::Rejected,
                    true,
                ),
            ];
            for (name, st, below) in steps {
                file_step(ui, th, name, NOTES_TXT, st, below);
            }
        },
    );
    stage_frames(
        ui,
        theme,
        "Focus has left; the verdict comes later",
        FRAME_W,
        |ui, th| {
            let steps = [
                (
                    "verdict pending · box from before — box kept, no ink",
                    File::Held,
                ),
                ("verdict rejected — ink shows, same height", File::Rejected),
                (
                    "verdict usable · no pointer button held — box closes this frame",
                    File::Rest,
                ),
                (
                    "verdict usable · pointer button held — box kept until after release",
                    File::Held,
                ),
                ("verdict pending · no box — no ink", File::Rest),
                (
                    "verdict rejected · no pointer button held — line opens this frame",
                    File::Rejected,
                ),
                (
                    "verdict rejected · pointer button held — edge now, line after release",
                    File::Due,
                ),
            ];
            for (name, st) in steps {
                file_step(ui, th, name, NEW_FILE, st, false);
            }
        },
    );
    stage_frames(
        ui,
        theme,
        "Focus leaves via keyboard or window, with no pointer button held",
        FRAME_W,
        |ui, th| {
            let steps = [
                (
                    "Tab · emptied — box closes on the frame focus moves",
                    "",
                    File::Rest,
                ),
                (
                    "Tab · rejected · box present — ink returns, same height",
                    NOTES_TXT,
                    File::Rejected,
                ),
                (
                    "window loses focus · rejected — same as leaving the field",
                    NOTES_TXT,
                    File::Rejected,
                ),
                (
                    "window loses focus · emptied — box closes, return doesn't reopen it",
                    "",
                    File::Rest,
                ),
            ];
            for (name, value, st) in steps {
                file_step(ui, th, name, value, st, false);
            }
        },
    );
    stage_frames(
        ui,
        theme,
        "Clicking Use default on the same row",
        FRAME_W,
        |ui, th| {
            let steps = [
                (
                    "rejected · press on Use default — nothing changes",
                    NOTES_TXT,
                    File::Rejected,
                ),
                (
                    "release — Use default on, no edge or ink, box kept this frame",
                    "",
                    File::DefaultHeld,
                ),
                ("next frame — box closes", "", File::Default),
            ];
            for (name, value, st) in steps {
                file_step(ui, th, name, value, st, false);
            }
        },
    );
    stage_row(ui, theme, |ui| {
        for (name, th) in &themes(theme) {
            let label = format!(
                "{name} · Width 460: clicking the checkbox under the line from field focus"
            );
            frame(ui, th, &label, DROPPED_W, own_checkbox_steps);
        }
        for (name, th) in &themes(theme) {
            let label = format!(
                "{name} · Width 360: control at row start; same click on checkbox under line"
            );
            frame(ui, th, &label, STACKED_W, own_checkbox_steps);
        }
    });
}

/// family Spec 의 목록이 닫힌 뒤 단계.
pub(super) fn draw_family_settle(ui: &mut egui::Ui, theme: &Theme) {
    stage_frames(
        ui,
        theme,
        "Family row after the list closes",
        FRAME_W,
        |ui, th| {
            let steps = [
                (
                    "new family · verdict pending · box from before — box kept, no ink",
                    FIRA,
                    Family::Held,
                    false,
                ),
                (
                    "verdict rejected — ink shows, same height",
                    FIRA,
                    Family::Rejected,
                    false,
                ),
                (
                    "verdict usable · no pointer button held — box closes this frame",
                    FIRA,
                    Family::Rest,
                    false,
                ),
                (
                    "list closed by press on row below — box kept, row below stays",
                    JETBRAINS,
                    Family::Held,
                    false,
                ),
                (
                    "no box · verdict rejected · pointer button held — nothing shows yet",
                    FIRA,
                    Family::Rest,
                    false,
                ),
                (
                    "next frame after release — line opens, row below drops",
                    FIRA,
                    Family::Rejected,
                    true,
                ),
            ];
            for (name, value, st, below) in steps {
                state(ui, th, name, |ui| {
                    override_family_row_with(ui, th, "en", value, st);
                    below_row(ui, th, &mut { below });
                });
            }
        },
    );
}
