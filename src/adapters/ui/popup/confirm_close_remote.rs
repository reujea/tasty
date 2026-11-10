//! 저장된 원격 행을 원격에서 닫기 전 확인한다. 행 메뉴의 "원격 워크스페이스 닫기…"와, 연결된 행의
//! 마지막 탭·pane·surface 닫기가 연다. 확인하면 담아 둔 구조 요청을 이 팝업 출처로 다시 보내고,
//! dispatch는 그 출처를 확인을 마친 요청으로 읽어 서버로 보낸다. 대상이 사라지면 닫는다.
//! 콘텐츠는 갤러리와 같은 `tasty_ui_widgets::close_remote_confirm`이 그린다.

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;
use crate::app::attach_client::workspace_close::SavedRowClose;
use crate::app::command::DomainIntent;
use crate::i18n::t;
use crate::state::MainViewState;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::CloseRemoteConfirmView;

pub const CONFIRM_CLOSE_REMOTE_POPUP_ID: &str = "confirm_close_remote";

/// 확인을 기다리는 닫기.
#[derive(Debug, Clone)]
pub struct PendingCloseRemote {
    pub(crate) close: SavedRowClose,
    /// 확인하면 다시 보낼 요청.
    pub(crate) intent: DomainIntent,
    /// 직전 프레임에 잰 콘텐츠 높이. 처음 프레임은 글자 수로 어림한다.
    pub(crate) measured_h: Option<LogicalPx>,
    /// Cancel에 기본 포커스를 준 뒤에는 다시 옮기지 않는다.
    pub(crate) focused: bool,
}

/// 확인을 연다. 이미 열려 있으면 새 요청으로 바꾼다.
pub(crate) fn open(state: &mut MainViewState, close: SavedRowClose, intent: DomainIntent) {
    state.dialogs.pending_close_remote = Some(PendingCloseRemote {
        close,
        intent,
        measured_h: None,
        focused: false,
    });
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup {
            id: CONFIRM_CLOSE_REMOTE_POPUP_ID,
            mode: crate::intent::OpenPopupMode::CenteredFocused,
        }
        .from_user_menu("workspace/close-remote"),
    );
}

/// 이 확인에서 다시 보낸 요청인지. dispatch가 같은 요청을 다시 묻지 않게 한다.
pub(crate) fn is_confirmed(origin: &crate::intent::IntentOrigin) -> bool {
    matches!(
        origin,
        crate::intent::IntentOrigin::User {
            source: crate::intent::UserSource::Menu(CONFIRM_CLOSE_REMOTE_POPUP_ID),
        }
    )
}

/// 표시할 이름과 프로필. 행이 없어졌거나 저장된 원격 행이 아니면 None이다.
fn target(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<(String, String, bool)> {
    let pending = state.dialogs.pending_close_remote.as_ref()?;
    let ws = engine
        .workspaces()
        .into_iter()
        .find(|ws| ws.id == pending.close.row)?;
    let binding = ws.remote_row.as_ref().filter(|_| ws.mirror)?;
    Some((ws.name.clone(), binding.profile.clone(), pending.close.last))
}

pub fn confirm_close_remote_title(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> String {
    let name = target(state, engine).map(|(name, _, _)| name);
    crate::i18n::t_fmt(
        "remote.row.close_remote.title",
        name.as_deref().unwrap_or(""),
    )
}

/// 글리프 줄·본문·버튼 행의 어림 높이. 폭 440에서 본문 한 줄 글자 수로 줄 수를 센다.
fn estimate_h(last: bool) -> LogicalPx {
    let th = theme::theme();
    const APPROX_CHARS_PER_LINE: usize = 60;
    let lines = |key: &'static str| t(key).chars().count().div_ceil(APPROX_CHARS_PER_LINE) as f32;
    let body_lines = lines("remote.row.close_remote.body")
        + if last {
            lines("remote.row.close_remote.last")
        } else {
            0.0
        };
    let line = th.font_size_body * th.line_height_ui;
    let title = th.font_size_max * th.line_height_ui;
    th.spacing_md * 3.0
        + th.spacing_sm * 2.0
        + title * 2.0
        + line * body_lines
        + th.item_height_interactive
}

pub fn confirm_close_remote_default_size() -> egui::Vec2 {
    egui::vec2(
        theme::theme().info_modal_width().value(),
        estimate_h(false).value(),
    )
}

pub fn confirm_close_remote_sizer(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let h = state
        .dialogs
        .pending_close_remote
        .as_ref()
        .map(|p| p.measured_h.unwrap_or_else(|| estimate_h(p.close.last)))
        .unwrap_or_else(|| estimate_h(false));
    egui::vec2(theme::theme().info_modal_width().value(), h.value())
}

/// 어떤 경로로 닫히든(취소·바깥·Escape) 대기를 비운다. 담아 둔 요청은 보내지 않는다.
pub fn on_close_confirm_close_remote(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.dialogs.pending_close_remote = None;
}

pub fn draw_confirm_close_remote(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        state.dialogs.pending_close_remote = None;
        return PopupAction::Close;
    }
    let Some((name, profile, last)) = target(state, engine) else {
        state.dialogs.pending_close_remote = None;
        return PopupAction::Close;
    };
    let th = theme::theme();
    let title = crate::i18n::t_fmt("remote.row.close_remote.title", &name);
    let body = crate::i18n::t_fmt("remote.row.close_remote.body", &profile);
    let glyph = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        icons::ALERT_TRIANGLE
            .image(rect.height(), c)
            .paint_at(ui, rect);
    };
    let focus_cancel = state
        .dialogs
        .pending_close_remote
        .as_ref()
        .is_some_and(|p| !p.focused);
    let top = ui.cursor().top();
    let out = tasty_ui_widgets::close_remote_confirm(
        ui,
        &th,
        &CloseRemoteConfirmView {
            title: &title,
            last: last.then(|| t("remote.row.close_remote.last")),
            body: &body,
            cancel: t("button.cancel"),
            confirm: t("remote.row.close_remote.confirm"),
            glyph: &glyph,
            focus_cancel,
        },
    );
    let measured = LogicalPx(ui.cursor().top() - top);
    if let Some(pending) = state.dialogs.pending_close_remote.as_mut() {
        pending.focused = true;
        pending.measured_h = Some(measured);
    }
    if out.cancel {
        state.dialogs.pending_close_remote = None;
        return PopupAction::Close;
    }
    if out.confirm {
        confirm(state);
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 담아 둔 요청을 확인을 마친 출처로 다시 보낸다. 버튼 없이도 시험에서 부를 수 있다.
pub(crate) fn confirm(state: &mut MainViewState) {
    if let Some(pending) = state.dialogs.pending_close_remote.take() {
        state.dispatch_intent(pending.intent.from_user_menu(CONFIRM_CLOSE_REMOTE_POPUP_ID));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 이 팝업이 다시 보낸 요청만 확인을 마친 것으로 본다. 다른 메뉴·컨텍스트 메뉴 요청은 다시 묻는다.
    #[test]
    fn only_the_resend_from_this_popup_counts_as_confirmed() {
        let intent = || DomainIntent::CloseWorkspace { workspace_id: 1 };
        assert!(is_confirmed(
            &intent()
                .from_user_menu(CONFIRM_CLOSE_REMOTE_POPUP_ID)
                .origin
        ));
        assert!(!is_confirmed(
            &intent().from_user_menu("workspace/close").origin
        ));
        assert!(!is_confirmed(&intent().from_user_context_menu().origin));
        assert!(!is_confirmed(&intent().from_agent_ipc().origin));
    }
}
