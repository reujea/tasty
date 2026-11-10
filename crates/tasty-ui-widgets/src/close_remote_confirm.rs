//! 저장된 원격 행의 "원격 워크스페이스 닫기" 확인 콘텐츠.
//! 본체 popup 과 갤러리 예제가 이 함수를 함께 부른다. 문자열은 호출자가 주입하고 셸(배경·테두리·
//! 그림자·scrim)과 Escape 처리는 호출자가 맡는다. 콘텐츠 여백은 이 함수가 넣는다.

use tasty_type_appearance::theme::Theme;

use crate::button::{Button, ButtonVariant};
use crate::icon_button::IconPainter;

/// 확인 콘텐츠의 입력값. 본체는 번역 문자열을, 갤러리는 같은 lang 문자열을 넣는다.
pub struct CloseRemoteConfirmView<'a> {
    /// 워크스페이스 이름이 들어간 제목.
    pub title: &'a str,
    /// 마지막 탭·pane 닫기로 열렸을 때만 본문 앞에 놓는 한 줄.
    pub last: Option<&'a str>,
    /// 어느 프로필의 원격에서 무엇이 끝나는지.
    pub body: &'a str,
    pub cancel: &'a str,
    pub confirm: &'a str,
    /// 경고 글리프. 색은 이 함수가 accent-danger-raised 로 준다.
    pub glyph: IconPainter<'a>,
    /// 처음 그리는 프레임에 Cancel 로 포커스를 준다(기본 포커스).
    pub focus_cancel: bool,
}

/// 이번 프레임에 눌린 버튼.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CloseRemoteConfirmOutput {
    pub confirm: bool,
    pub cancel: bool,
}

/// 머리(글리프 + 제목) → 본문 문단 → 오른쪽 정렬 버튼 행. 폭은 호출자가 준 영역을 채운다.
pub fn close_remote_confirm(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &CloseRemoteConfirmView<'_>,
) -> CloseRemoteConfirmOutput {
    let md = theme.spacing_md.value();
    let sm = theme.spacing_sm.value();
    let mut out = CloseRemoteConfirmOutput::default();
    ui.spacing_mut().item_spacing = egui::vec2(sm, 0.0);

    let title_size = theme.font_size_max.value();
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: md as i8,
            right: md as i8,
            top: md as i8,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let glyph = theme.icon_glyph_size_md.value();
                // 글리프를 제목 첫 줄의 가운데에 맞춘다.
                let line = title_size * theme.line_height_ui;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(glyph, line), egui::Sense::hover());
                let icon = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
                (view.glyph)(ui, icon, theme.accent_danger_raised().to_egui());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(view.title)
                            .size(title_size)
                            .strong()
                            .color(theme.text_primary().to_egui()),
                    )
                    .wrap(),
                );
            });
        });

    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: md as i8,
            right: md as i8,
            top: sm as i8,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = sm;
            for text in view.last.into_iter().chain([view.body]) {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(theme.font_size_body.value())
                            .line_height(Some(theme.font_size_body.value() * theme.line_height_ui))
                            .color(theme.text_secondary().to_egui()),
                    )
                    .wrap(),
                );
            }
        });

    egui::Frame::NONE
        .inner_margin(egui::Margin::same(md as i8))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = sm;
                if Button::new(view.confirm)
                    .variant(ButtonVariant::Danger)
                    .show(ui, theme)
                    .clicked()
                {
                    out.confirm = true;
                }
                let cancel = Button::new(view.cancel)
                    .variant(ButtonVariant::Ghost)
                    .show(ui, theme);
                if view.focus_cancel {
                    cancel.request_focus();
                }
                if cancel.clicked() {
                    out.cancel = true;
                }
            });
        });
    out
}
