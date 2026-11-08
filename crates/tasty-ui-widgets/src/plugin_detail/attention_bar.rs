//! Attention 상세의 액션 바. Installed 와 같은 바 틀에 상태 표시와 사유별 버튼을 넣는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::plugin_detail_bar_frame;
use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;

/// 상태 문구 글자 크기. 디자인 `fontSize: 12` 이며 대응 semantic 토큰이 없다.
const STATUS_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

/// 사유별 오른쪽 버튼. 서명 사유는 fingerprint 줄이 복사를 맡아 버튼이 없다.
#[derive(Clone, Copy)]
pub enum PluginAttentionBarAction<'a> {
    /// 권한 변경 재승인 — primary.
    Reapprove(&'a str),
    /// 실행 오류 — ghost, settings 아이콘.
    Configure(&'a str),
    /// 미인증 서명 사유 — danger. 누르기 전에는 등록하지 않는다.
    RegisterAnyway(&'a str),
    /// 예약 id 사유 — secondary, accent-danger-raised 글자. Installed 의 Uninstall 과 같다.
    Remove(&'a str),
}

/// Attention 액션 바 입력.
pub struct PluginAttentionBarView<'a> {
    /// `Not registered` 또는 `Needs review`.
    pub status: &'a str,
    /// 점의 색. 사유의 severity 색(danger 또는 warning)이다.
    pub dot: egui::Color32,
    /// 상태 문구 색. 예약 id 사유만 점보다 밝은 raised 잉크를 쓴다.
    pub ink: egui::Color32,
    pub action: Option<PluginAttentionBarAction<'a>>,
}

/// 왼쪽에 status-dot 크기의 점과 상태 문구, 오른쪽 끝에 사유별 버튼을 둔다.
/// 버튼을 눌렀으면 true.
pub fn plugin_attention_bar(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginAttentionBarView<'_>,
) -> bool {
    let mut clicked = false;
    plugin_detail_bar_frame(ui, theme, ControlSize::Md.height(theme), |ui| {
        let dot = theme.status_dot_size.value();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(dot, dot), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), dot * 0.5, view.dot);
        ui.label(
            egui::RichText::new(view.status)
                .size(STATUS_PRIMITIVE_12.value())
                .color(view.ink),
        );
        if let Some(action) = view.action {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clicked = action_button(ui, theme, action);
            });
        }
    });
    clicked
}

fn action_button(ui: &mut egui::Ui, theme: &Theme, action: PluginAttentionBarAction<'_>) -> bool {
    match action {
        PluginAttentionBarAction::Reapprove(label) => Button::new(label)
            .variant(ButtonVariant::Primary)
            .show(ui, theme)
            .clicked(),
        PluginAttentionBarAction::Configure(label) => Button::new(label)
            .variant(ButtonVariant::Ghost)
            .leading_icon(&|ui, rect, c| {
                tasty_icons::SETTINGS
                    .image(rect.height(), c)
                    .paint_at(ui, rect)
            })
            .show(ui, theme)
            .clicked(),
        PluginAttentionBarAction::RegisterAnyway(label) => Button::new(label)
            .variant(ButtonVariant::Danger)
            .show(ui, theme)
            .clicked(),
        PluginAttentionBarAction::Remove(label) => Button::new(label)
            .variant(ButtonVariant::Secondary)
            .danger_ink(true)
            .show(ui, theme)
            .clicked(),
    }
}
