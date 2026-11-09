//! 원격 파일 수신 폴더와 용량 상한 설정의 예제. 실제 설정 저장은 하지 않는다.

use std::cell::RefCell;
use tasty_type_geometry::length::LogicalPx;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, Input, SettingsRow, settings_label_column,
};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

/// 디자인 settings 콘텐츠 컬럼 근사 프레임 폭(settings_handler 와 동일).
pub(super) const WIDTH: LogicalPx = LogicalPx(560.0);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        dir: String::new(),
        max: String::from("500"),
    });
}

struct State {
    dir: String,
    max: String,
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // 설정 창 내부 콘텐츠이므로 그림자를 추가하지 않는다.
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_lg, theme.spacing_md, |ui| {
                ui.spacing_mut().item_spacing.y = theme.settings_row_gap().value();
                mono_head(ui, theme, "Received files");

                let rows = [
                    SettingsRow::new("Save folder")
                        .caption(crate::i18n::t("settings.remote_transfer.dir_desc")),
                    SettingsRow::new("Maximum size")
                        .caption(crate::i18n::t("settings.remote_transfer.max_capacity_desc")),
                ];
                let col = settings_label_column(ui, theme, &rows);
                let [folder, size] = rows;
                STATE.with(|s| {
                    let st = &mut *s.borrow_mut();

                    folder.show(ui, theme, col, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                            // specimen — 클릭 응답 불필요, 그리기만(폴더 피커는 host 소유).
                            let _ = Button::new("Browse…")
                                .variant(ButtonVariant::Secondary)
                                .size(ControlSize::Sm)
                                .leading_icon(&|ui, rect, c| {
                                    icons::FOLDER.image(rect.width(), c).paint_at(ui, rect);
                                })
                                .show(ui, theme);
                            Input::new()
                                .mono(true)
                                .placeholder("~/.tasty/transfers/")
                                .show(ui, theme, &mut st.dir);
                        });
                    });
                    separator_line(ui, theme);

                    size.show(ui, theme, col, |ui| {
                        // 본체 number_field 처럼 단위가 남은 폭에 들어가지 않으면 입력 칸 아래 줄로 내려간다.
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                            // 본체 숫자 칸처럼 값에 맞춰 자란다.
                            let width = tasty_ui_widgets::number_field_width(ui, theme, &st.max);
                            Input::new()
                                .mono(true)
                                .align(egui::Align::RIGHT)
                                .width(width.value())
                                .show(ui, theme, &mut st.max);
                            ui.label(
                                egui::RichText::new("MiB")
                                    .family(tasty_ui_widgets::label_font_family())
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                        });
                    });
                });
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("L2 position", "5th — after Overlay"),
            ("rows", "Save folder · Maximum size"),
            (
                "row grid",
                "settings row grid · label column 150 … 240 · gap 16 · caption under its row",
            ),
            ("row height", "settings-row-min-height"),
            ("folder row", "mono Input + Browse… (secondary · folder)"),
            (
                "size row",
                "numeric Input that fits its value (number-field-min-width 90 floor) + static mono “MiB”",
            ),
        ],
        &[
            TokenChip::without_color("settings-row-min-height", "row height"),
            TokenChip::new(
                "separator",
                "row divider",
                theme.separator.to_egui_premultiplied(),
            ),
            TokenChip::new(
                "text-muted",
                "descriptions + unit",
                theme.text_muted().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "These rows show the receiving folder and its total capacity limit. MiB is a static unit label outside the numeric input. The host rejects a new transfer when the current folder usage plus the incoming size exceeds the limit.",
    );
}

/// jsx `Mono` — font-label 10 uppercase, text-muted (settings_handler `mono_head` 관례).
fn mono_head(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .family(tasty_ui_widgets::label_font_family())
            .size(theme.font_size_micro.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 행 사이 1px separator (jsx `borderTop: 1px solid separator`).
fn separator_line(ui: &mut egui::Ui, theme: &Theme) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}
