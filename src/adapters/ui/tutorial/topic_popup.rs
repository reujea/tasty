//! 튜토리얼 주제 선택. PopupManager의 셸 안에 목록과 시작·재개 버튼을 표시한다.

use tasty_ui_widgets::tokens::{STRUCT_GAP_2, TUTORIAL_STEP_GAP_X};
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, margin_all,
};

use crate::adapters::ui::popup::PopupAction;
use crate::adapters::ui::tutorial::all_topics;
use crate::i18n::t;
use crate::state::MainViewState;
use crate::theme;

/// 팝업 id. `defs.rs::all_defs()` 및 도구 메뉴 배선에서 참조.
pub const TUTORIAL_TOPICS_POPUP_ID: &str = "tutorial_topics";

/// 기본 크기(360 × 헤더+리스트+푸터). 리스트는 내부 스크롤(max 200).
pub fn tutorial_topics_default_size() -> egui::Vec2 {
    egui::vec2(360.0, 360.0)
}

pub fn draw_tutorial_topics_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    if !state.tutorial.catalog_loaded {
        crate::adapters::ui::tutorial::open_catalog(state);
    }
    let th = theme::theme();
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return PopupAction::Close;
    }

    let mut action = PopupAction::None;
    if ui.ctx().input(|i| i.key_pressed(egui::Key::ArrowDown)) {
        state.tutorial.popup_selected =
            (state.tutorial.popup_selected + 1).min(all_topics().len() - 1);
    }
    if ui.ctx().input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        state.tutorial.popup_selected = state.tutorial.popup_selected.saturating_sub(1);
    }

    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let width = ui.available_width();

    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: th.spacing_lg.value() as i8,
            right: th.spacing_lg.value() as i8,
            top: th.spacing_md.value() as i8,
            bottom: th.spacing_md.value() as i8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t("tutorial.popup_title"))
                        .size(th.font_size_body.value())
                        .strong()
                        .color(th.text_primary().to_egui()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, &th, &|ui, rect, c| {
                            crate::adapters::ui::icons::CLOSE
                                .image(rect.height(), c)
                                .paint_at(ui, rect)
                        })
                        .clicked()
                    {
                        action = PopupAction::Close;
                    }
                });
            });
        });
    hsep(ui, &th, width);

    egui::Frame::new()
        .inner_margin(egui::Margin::same(th.spacing_sm.value() as i8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(th.tutorial_topic_body_max_height().value())
                .auto_shrink([false, true])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                    for (i, topic) in all_topics().iter().enumerate() {
                        let sel = state.tutorial.popup_selected == i;
                        let status = if state.tutorial.progress[i].completed {
                            t("tutorial.status_done")
                        } else if state.tutorial.progress[i].started {
                            t("tutorial.status_progress")
                        } else {
                            t("tutorial.status_new")
                        };
                        let description = format!("{} · {}", t(topic.desc_key), status);
                        if ui
                            .push_id(topic.id, |ui| {
                                topic_row(ui, &th, i + 1, t(topic.title_key), &description, sel)
                            })
                            .inner
                        {
                            state.tutorial.popup_selected = i;
                        }
                    }
                });
        });
    hsep(ui, &th, width);

    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: th.spacing_lg.value() as i8,
            right: th.spacing_lg.value() as i8,
            top: th.spacing_md.value() as i8,
            bottom: th.spacing_md.value() as i8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t("tutorial.esc_hint"))
                        .family(tasty_ui_widgets::label_font_family())
                        .size(th.font_size_micro.value())
                        .color(th.text_muted().to_egui()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let selected = state.tutorial.popup_selected;
                    let resume = state.tutorial.progress[selected].resume > 0
                        || (state.tutorial.progress[selected].started
                            && !state.tutorial.progress[selected].completed);
                    if resume
                        && Button::new(t("tutorial.btn_restart"))
                            .variant(ButtonVariant::Secondary)
                            .size(ControlSize::Sm)
                            .show(ui, &th)
                            .clicked()
                    {
                        state.tutorial.progress[selected].resume = 0;
                        state.tutorial.request_start(selected);
                        action = PopupAction::Close;
                    }
                    if Button::new(t(if resume {
                        "tutorial.btn_resume"
                    } else if state.tutorial.progress[selected].completed {
                        "tutorial.btn_replay"
                    } else {
                        "tutorial.btn_start"
                    }))
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Sm)
                    .show(ui, &th)
                    .clicked()
                    {
                        state.tutorial.request_start(state.tutorial.popup_selected);
                        action = PopupAction::Close;
                    }
                });
            });
        });

    if ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
        state.tutorial.request_start(state.tutorial.popup_selected);
        action = PopupAction::Close;
    }
    if state.tutorial.save_error {
        ui.label(t("tutorial.save_error"));
    }
    action
}

/// 주제 행 — 인덱스 캡 + 제목/설명. 클릭되면 `true`(선택). 선택 시 surface-active
/// + accent 캡 + accent40% 보더.
fn topic_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    n: usize,
    title: &str,
    desc: &str,
    sel: bool,
) -> bool {
    // 선택된 토픽 테두리 — accent 의 40% alpha. 대응 토큰 없음.
    const SELECTED_BORDER_ALPHA: u8 = 102;
    let border = if sel {
        th.accent_primary()
            .with_alpha(SELECTED_BORDER_ALPHA)
            .to_egui()
    } else {
        egui::Color32::TRANSPARENT
    };
    let fill = if sel {
        th.surface_active().to_egui()
    } else {
        egui::Color32::TRANSPARENT
    };
    let resp = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(th.border_width.value(), border))
        .corner_radius(th.corner_radius.value())
        .inner_margin(margin_all(th.spacing_xs * 2.5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = TUTORIAL_STEP_GAP_X;
                let (cap, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                let (cap_bg, cap_fg) = if sel {
                    (th.accent_primary().to_egui(), th.text_on_accent().to_egui())
                } else {
                    (
                        th.surface_raised().to_egui(),
                        th.text_muted_raised().to_egui(),
                    )
                };
                ui.painter()
                    .rect_filled(cap, th.corner_radius_sm.value(), cap_bg);
                ui.painter().text(
                    cap.center(),
                    egui::Align2::CENTER_CENTER,
                    n.to_string(),
                    egui::FontId::monospace(th.font_size_micro.value()),
                    cap_fg,
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
                    let (title_fg, desc_fg) = if sel {
                        (th.text_on_active(), th.text_on_active())
                    } else {
                        (th.text_primary(), th.text_muted())
                    };
                    ui.label(
                        egui::RichText::new(title)
                            .size(th.font_size_body.value())
                            .color(title_fg.to_egui()),
                    );
                    ui.label(
                        egui::RichText::new(desc)
                            .size(th.font_size_caption.value())
                            .color(desc_fg.to_egui()),
                    );
                });
            });
        });
    ui.interact(resp.response.rect, resp.response.id, egui::Sense::click())
        .clicked()
}

fn hsep(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme, width: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, th.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}

/// Every catalog close path clears only its load latch, never a queued start.
pub fn on_close(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.tutorial.catalog_loaded = false;
}
