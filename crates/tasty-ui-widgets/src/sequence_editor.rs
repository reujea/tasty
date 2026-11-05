//! IpcSequence 를 한 줄에 호출 하나로 쓰는 인라인 편집기. CodeArea 아래에 첫 오류 한 줄(또는 빈
//! 시퀀스 안내), 형식 도움말, 오른쪽 정렬 Cancel · Apply 를 둔다. 문자열 해석은 호출자가 `check` 로
//! 넘긴다(본체는 `hook_handler::sequence_text`, 갤러리는 고정 표본).

use tasty_type_appearance::theme::Theme;

use crate::button::{Button, ButtonVariant};
use crate::code_area::{CodeArea, CodeAreaKeys};
use crate::control::ControlSize;
use crate::icon_button::IconPainter;

/// 해석 오류 하나. `sentence` 는 번역한 문장, `reason` 은 번역하지 않는 파서 원문이다.
pub struct SequenceEditorError {
    /// 1부터인 줄 번호. 거터와 띠가 이 줄을 표시한다.
    pub line: usize,
    pub sentence: String,
    pub reason: Option<String>,
}

/// 편집기 문구와 해석 함수.
pub struct SequenceEditorView<'a> {
    pub placeholder: &'a str,
    pub help: &'a str,
    /// 호출이 하나도 없을 때의 안내.
    pub empty_note: &'a str,
    pub cancel: &'a str,
    pub apply: &'a str,
    /// 오류 줄 앞의 `alertCircle` 글리프.
    pub alert_icon: IconPainter<'a>,
    /// 호출 수를 세거나 첫 오류를 돌려준다. 입력이 바뀔 때마다 부른다.
    pub check: &'a dyn Fn(&str) -> Result<usize, SequenceEditorError>,
    /// 입력칸의 Apply·Cancel 키. 없으면 버튼으로만 닫는다.
    pub keys: Option<CodeAreaKeys<'a>>,
}

/// 이번 프레임에 사용자가 고른 동작.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceEditorAction {
    None,
    /// Apply 버튼 또는 확정 키. 오류가 있으면 나오지 않는다.
    Apply,
    /// Cancel 버튼 또는 취소 키.
    Cancel,
}

pub fn sequence_editor(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: impl std::hash::Hash,
    view: &SequenceEditorView<'_>,
    buf: &mut String,
) -> SequenceEditorAction {
    let caption = theme.font_size_caption.value();
    let line_h = caption * theme.line_height_ui;
    let before = (view.check)(buf);
    let mut action = SequenceEditorAction::None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        let out = CodeArea::new(id_salt)
            .placeholder(view.placeholder)
            .error_line(before.as_ref().err().map(|e| e.line))
            .keys(view.keys)
            .show(ui, theme, buf);
        // 거터 표시는 이번 프레임 입력 전 해석으로 그렸으므로 바뀌었으면 한 번 더 그린다.
        if out.response.changed() {
            ui.ctx().request_repaint();
        }
        let checked = if out.response.changed() {
            (view.check)(buf)
        } else {
            before
        };
        match &checked {
            Err(e) => error_line(ui, theme, view, e, caption, line_h),
            Ok(0) => {
                ui.label(
                    egui::RichText::new(view.empty_note)
                        .size(caption)
                        .line_height(Some(line_h))
                        .color(theme.text_muted().to_egui()),
                );
            }
            Ok(_) => {}
        }
        ui.add(
            egui::Label::new(
                egui::RichText::new(view.help)
                    .size(caption)
                    .line_height(Some(line_h))
                    .color(theme.text_muted().to_egui()),
            )
            .wrap(),
        );
        let valid = checked.is_ok();
        ui.add_space(theme.spacing_xs.value());
        // 버튼 한 줄 높이만 쓴다. 바로 right_to_left 로 두면 남은 세로 공간을 모두 차지한다.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                let apply = Button::new(view.apply)
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .enabled(valid)
                    .show(ui, theme)
                    .clicked();
                let cancel = Button::new(view.cancel)
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .show(ui, theme)
                    .clicked();
                action = if cancel || out.cancel {
                    SequenceEditorAction::Cancel
                } else if valid && (apply || out.submit) {
                    SequenceEditorAction::Apply
                } else {
                    SequenceEditorAction::None
                };
            })
        });
    });
    action
}

/// 첫 오류 한 줄 — 글리프 · 번역 문장(accent-danger) · 파서 원문(mono text-muted). 좁으면 줄을 바꾼다.
fn error_line(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &SequenceEditorView<'_>,
    e: &SequenceEditorError,
    caption: f32,
    line_h: f32,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
        let glyph = theme.icon_glyph_size_xs.value();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(glyph, line_h), egui::Sense::hover());
        let icon = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
        (view.alert_icon)(ui, icon, theme.accent_danger().to_egui());
        ui.label(
            egui::RichText::new(&e.sentence)
                .size(caption)
                .line_height(Some(line_h))
                .color(theme.accent_danger_raised().to_egui()),
        );
        if let Some(reason) = &e.reason {
            ui.label(
                egui::RichText::new(reason)
                    .monospace()
                    .size(caption)
                    .line_height(Some(line_h))
                    .color(theme.text_muted().to_egui()),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 포커스 중 확정 키는 해석 오류가 없을 때만 Apply 다. 취소 키는 언제나 Cancel 이다.
    #[test]
    fn the_submit_key_applies_only_without_an_error() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let no_icon = |_: &mut egui::Ui, _: egui::Rect, _: egui::Color32| {};
        let submit_key = |i: &mut egui::InputState| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::Enter,
            ))
        };
        let cancel_key =
            |i: &mut egui::InputState| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        let run = |text: &str, key: Option<(egui::Key, egui::Modifiers)>| {
            let check = |s: &str| {
                if s.contains('{') {
                    Err(SequenceEditorError {
                        line: 1,
                        sentence: "bad".into(),
                        reason: None,
                    })
                } else {
                    Ok(s.lines().count())
                }
            };
            let view = SequenceEditorView {
                placeholder: "",
                help: "help",
                empty_note: "empty",
                cancel: "Cancel",
                apply: "Apply",
                alert_icon: &no_icon,
                check: &check,
                keys: Some(CodeAreaKeys {
                    submit: &submit_key,
                    cancel: &cancel_key,
                }),
            };
            let mut buf = text.to_string();
            let mut last = SequenceEditorAction::None;
            for frame in 0..3 {
                let events = match (frame, key) {
                    (2, Some((key, modifiers))) => vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    _ => Vec::new(),
                };
                let output = ctx.run(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            last = sequence_editor(ui, &theme, "seq", &view, &mut buf);
                            ui.memory_mut(|m| m.request_focus(egui::Id::new("seq")));
                        });
                    },
                );
                drop(output);
            }
            last
        };
        let submit = Some((egui::Key::Enter, egui::Modifiers::COMMAND));
        let esc = Some((egui::Key::Escape, egui::Modifiers::NONE));
        assert_eq!(run("system.info", submit), SequenceEditorAction::Apply);
        assert_eq!(run("system.info {", submit), SequenceEditorAction::None);
        assert_eq!(run("system.info {", esc), SequenceEditorAction::Cancel);
        assert_eq!(run("system.info", None), SequenceEditorAction::None);
    }
}
