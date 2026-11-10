//! 저장된 원격 행의 연결 팝업. 연결되지 않은 행이 활성 workspace 이면 그 콘텐츠 영역 가운데에 연다.
//! 형상(마지막 화면)이 남은 행은 콘텐츠 영역을 scrim 으로 덮고 scrim 이 포인터를 받는다. 빈 workspace 는
//! scrim 없이 띄운다. 연결 상태가 여닫기를 정하며 사용자 포커스는 옮기지 않는다.
//! 콘텐츠는 갤러리와 같은 `tasty_ui_widgets::remote_row_popup`이 그린다.

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::{DragHandle, PopupAction, PopupDef, PopupId, PopupScope};
use crate::i18n::t;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme;
use tasty_ui_widgets::{
    REMOTE_ROW_POPUP_WIDTH, REMOTE_ROW_USER_BODY_STAY, RemoteRowPopupBody, RemoteRowPopupKind,
    RemoteRowPopupView, RemoteRowState,
};

/// 빈 workspace 위 팝업(scrim 없음, popover 그림자).
pub const REMOTE_ROW_POPUP_ID: &str = "remote_row";
/// 마지막 화면 위 팝업(콘텐츠 영역 scrim, modal 그림자).
pub const REMOTE_ROW_SCRIM_POPUP_ID: &str = "remote_row_over_screen";

pub(crate) fn is_remote_row(id: PopupId) -> bool {
    id == REMOTE_ROW_POPUP_ID || id == REMOTE_ROW_SCRIM_POPUP_ID
}

/// scrim 이 포인터를 받는 팝업(R-58 Q1). 마지막 화면은 선택·복사·스크롤·링크가 막힌다.
pub(crate) fn scrim_takes_pointer(id: PopupId) -> bool {
    id == REMOTE_ROW_SCRIM_POPUP_ID
}

/// 이 창의 연결 팝업 상태.
#[derive(Debug, Clone, Default)]
pub struct RemoteRowPopupState {
    /// 지금 연 팝업과 그 행.
    shown: Option<(PopupId, u32)>,
    /// 직전 프레임에 잰 콘텐츠 높이.
    measured_h: Option<f32>,
    /// 콘텐츠 영역 폭. 팝업 폭이 영역에서 양쪽 space-md 를 뺀 값을 넘지 않게 한다.
    bounds_w: Option<f32>,
    /// 연결돼 있던 행이 이번 프레임에 끊겨 scrim 을 열었다. 링크 툴팁을 닫는 쪽이 꺼내 간다.
    dropped: bool,
}

impl RemoteRowPopupState {
    /// 행이 끊겨 scrim 이 막 덮였는지 꺼낸다. 열려 있던 링크 툴팁은 닫히고 다시 열리지 않는다(Q1-a).
    pub(crate) fn take_dropped(&mut self) -> bool {
        std::mem::take(&mut self.dropped)
    }
}

/// 저장된 원격 행이 지금 보일 팝업. 연결된 행과 다른 workspace 는 없다.
pub(crate) fn wanted(engine: &EngineRead<'_>, ws: &crate::model::Workspace) -> Option<PopupId> {
    if !ws.is_saved_remote_row() {
        return None;
    }
    match engine.remote_row_state(ws.id) {
        RemoteRowState::Connected => None,
        state if state.had_shape() => Some(REMOTE_ROW_SCRIM_POPUP_ID),
        _ => Some(REMOTE_ROW_POPUP_ID),
    }
}

/// 활성 workspace 에 맞춰 팝업을 열고 닫는다. 끊긴 행에서 열려 있던 파일 처리기 선택은 닫는다.
pub(crate) fn sync(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    draw_ctx: &crate::adapters::ui::LayoutContext,
) {
    let ws = state.active_workspace(engine);
    let want = wanted(engine, ws).map(|id| (id, ws.id));
    let row = &mut state.dialogs.remote_row_popup;
    row.bounds_w = draw_ctx.workspace_rect.map(|r| r.width());
    if row.shown == want {
        return;
    }
    let previous = std::mem::replace(&mut row.shown, want);
    if let Some((id, _)) = previous
        && want.is_none_or(|(w, _)| w != id)
    {
        // intent-exempt: 연결 상태를 따라 매 프레임 맞추는 수동 팝업이다. Intent 큐는 한 프레임 늦어 연결된 행에 팝업이 남는다.
        state.popups.close(id);
    }
    let Some((id, ws_id)) = want else {
        return;
    };
    // intent-exempt: 연결 상태를 따라 매 프레임 맞추는 수동 팝업이다. 포커스를 받지 않고 사용자 선택을 바꾸지 않는다.
    state
        .popups
        .open_with_scope(id, PopupScope::Workspace(ws_id));
    state.popups.set_focused(id, false);
    state.popups.mark_passive(id);
    if id == REMOTE_ROW_SCRIM_POPUP_ID && previous.is_none_or(|(_, row)| row == ws_id) {
        state.dialogs.remote_row_popup.dropped = true;
        if state
            .popups
            .is_open(super::file_handler_picker::PICKER_POPUP_ID)
        {
            // intent-exempt: scrim 이 덮는 같은 프레임에 닫아야 picker 가 scrim 아래에서 입력을 받지 않는다.
            state
                .popups
                .close(super::file_handler_picker::PICKER_POPUP_ID);
        }
    }
}

/// 연결 상태와 행 사실로 팝업 종류를 고른다. 이유는 사이드바 표지와 같은 안내다.
pub(crate) fn popup_kind(
    state: RemoteRowState,
    view: crate::remote::state::RowView,
    has_reason: bool,
) -> RemoteRowPopupKind {
    use RemoteRowPopupKind as K;
    match state {
        RemoteRowState::Connecting | RemoteRowState::Connected => K::Connecting,
        RemoteRowState::Reconnecting => K::Reconnecting,
        RemoteRowState::Disconnected if view.closed_elsewhere => K::Closed,
        RemoteRowState::Disconnected if view.user_disconnected => K::User,
        RemoteRowState::Disconnected if has_reason => K::Failed,
        RemoteRowState::Disconnected => K::GaveUp,
        RemoteRowState::NotConnected if view.closed_elsewhere => K::Closed,
        RemoteRowState::NotConnected if has_reason => K::Failed,
        RemoteRowState::NotConnected => K::Idle,
    }
}

fn width() -> f32 {
    let th = theme::theme();
    (REMOTE_ROW_POPUP_WIDTH.value() * th.ui_zoom).round()
}

/// 첫 프레임의 높이 어림: 여백, 머리 한 줄, 필드 두 줄, 상태 블록 두 줄, 버튼. 이후는 잰 값을 쓴다.
fn estimate_h() -> f32 {
    let th = theme::theme();
    let line = |size: f32| size * th.line_height_ui;
    th.spacing_md.value() * 5.0
        + th.spacing_xs.value() * 2.0
        + line(th.font_size_body.value()) * 3.0
        + line(th.font_size_caption.value()) * 2.0
        + th.item_height_tab.value()
}

fn sizer(state: &MainViewState, _engine: &EngineRead<'_>) -> egui::Vec2 {
    let row = &state.dialogs.remote_row_popup;
    let md = theme::theme().spacing_md.value();
    let w = row
        .bounds_w
        .map_or(width(), |bounds| width().min((bounds - md * 2.0).max(0.0)));
    egui::vec2(w, row.measured_h.unwrap_or_else(estimate_h))
}

fn on_close(_ctx: &egui::Context, state: &mut MainViewState, _engine: &EngineRead<'_>) {
    state.dialogs.remote_row_popup.measured_h = None;
}

fn draw(ui: &mut egui::Ui, state: &mut MainViewState, engine: &EngineRead<'_>) -> PopupAction {
    let Some((id, row)) = state.dialogs.remote_row_popup.shown else {
        return PopupAction::None;
    };
    let Some(ws) = engine.workspaces().into_iter().find(|ws| ws.id == row) else {
        return PopupAction::None;
    };
    let Some(binding) = ws.remote_row.clone() else {
        return PopupAction::None;
    };
    let name = ws.name.clone();
    let row_state = engine.remote_row_state(row);
    let view = engine.remote_row_view(row);
    let reason = state
        .attach_notices
        .get(row)
        .map(|notice| crate::adapters::ui::sidebar::remote_row::reason_of(notice.kind));
    let kind = popup_kind(row_state, view, reason.is_some());
    let shape = id == REMOTE_ROW_SCRIM_POPUP_ID;
    let copy = tasty_ui_widgets::remote_row_popup_copy(kind, shape, REMOTE_ROW_USER_BODY_STAY);
    let reason = if kind == RemoteRowPopupKind::Closed {
        Some(tasty_ui_widgets::RemoteRowReason::WorkspaceMissing)
    } else {
        reason
    };
    let seconds = engine
        .settings
        .remote_attach
        .reconnect_timeout_secs
        .to_string();
    let body = match copy.body {
        RemoteRowPopupBody::None => None,
        RemoteRowPopupBody::Seconds(key) => Some(crate::i18n::t_fmt(key, &seconds)),
        RemoteRowPopupBody::Plain(key) => Some(t(key).to_owned()),
        RemoteRowPopupBody::Reason => {
            reason.map(|r| crate::adapters::ui::sidebar::remote_row::reason_text(r, &binding))
        }
    };
    let remote = crate::adapters::ui::sidebar::remote_row::remote_label(&binding);
    let remote_glyph = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        icons::REMOTE.image(rect.height(), c).paint_at(ui, rect);
    };
    let warn_glyph = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        icons::ALERT_TRIANGLE
            .image(rect.height(), c)
            .paint_at(ui, rect);
    };
    let th = theme::theme();
    let top = ui.cursor().top();
    let clicked = tasty_ui_widgets::remote_row_popup(
        ui,
        &th,
        &RemoteRowPopupView {
            name: &name,
            profile_label: t("remote.row.popup.profile"),
            profile: &binding.profile,
            remote_label: t("remote.row.popup.remote_workspace"),
            remote: &remote,
            title: copy.title.map(t),
            body: body.as_deref(),
            button: t(copy.button),
            copy,
            remote_glyph: &remote_glyph,
            warn_glyph: &warn_glyph,
        },
    );
    state.dialogs.remote_row_popup.measured_h = Some(ui.cursor().top() - top);
    if clicked {
        state.dispatch_intent(
            crate::intent::Intent::SavedRow {
                workspace_id: row,
                action: crate::intent::SavedRowAction::Connect,
            }
            .from_user_menu(REMOTE_ROW_POPUP_ID),
        );
    }
    PopupAction::None
}

/// 두 팝업의 정의. 제목줄·닫기·이동이 없고 바깥을 눌러도 닫히지 않는다.
pub(crate) fn def(id: PopupId) -> PopupDef {
    PopupDef {
        id,
        title_key: "remote.row.popup.connect",
        fullscreen_stage: None,
        title_fn: None,
        default_size: egui::vec2(width(), estimate_h()),
        sizer: Some(sizer),
        default_scope: PopupScope::Window,
        close_on_outside_click: false,
        headless: true,
        sticky_focus: false,
        drag_handle: DragHandle::None,
        resizable: false,
        min_size: None,
        draw_fn: draw,
        on_close: Some(on_close),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::state::RowView;
    use RemoteRowPopupKind as K;

    /// 끊긴 행은 다른 곳에서 닫힘 › 사용자 끊음 › 이유 › 시간 초과 순서로 종류를 고른다.
    #[test]
    fn a_disconnected_row_picks_closed_then_user_then_reason_then_timed_out() {
        let view = |closed, user| RowView {
            closed_elsewhere: closed,
            user_disconnected: user,
            trying_since: None,
        };
        let d = RemoteRowState::Disconnected;
        assert_eq!(popup_kind(d, view(true, true), true), K::Closed);
        assert_eq!(popup_kind(d, view(false, true), true), K::User);
        assert_eq!(popup_kind(d, view(false, false), true), K::Failed);
        assert_eq!(popup_kind(d, view(false, false), false), K::GaveUp);
        let n = RemoteRowState::NotConnected;
        assert_eq!(popup_kind(n, view(false, false), true), K::Failed);
        assert_eq!(popup_kind(n, view(true, false), true), K::Closed);
        assert_eq!(popup_kind(n, view(false, false), false), K::Idle);
        assert_eq!(
            popup_kind(RemoteRowState::Reconnecting, RowView::default(), false),
            K::Reconnecting
        );
    }

    fn layout(workspace_id: u32) -> crate::adapters::ui::LayoutContext {
        crate::adapters::ui::LayoutContext {
            active_workspace: 0,
            active_workspace_id: workspace_id,
            pane_rects: Vec::new(),
            surface_rects: Vec::new(),
            active_tabs: Vec::new(),
            active_tab_ids: Vec::new(),
            workspace_rect: None,
        }
    }

    /// 활성 workspace 를 저장된 원격 행으로 만들고 그 ID 를 돌려준다.
    fn saved_row(
        state: &mut crate::state::RequestContext,
        session: &mut crate::runtime::engine_session::EngineSession,
    ) -> u32 {
        let engine = session.borrow_mut();
        let id = 900_001;
        let mut workspace =
            crate::model::Workspace::new_with_terminal_marker(id, "row".into(), id, id, id);
        workspace.mirror = true;
        workspace.remote_row = Some(crate::model::RemoteBinding {
            profile: "gx10".into(),
            remote_workspace: 4,
        });
        engine.core.push_mirror_workspace(workspace);
        state.set_active_workspace_index(&engine.read(), engine.workspaces().len() - 1);
        id
    }

    /// 연결된 저장 행이 Resync 로 소켓을 놓으면 상태는 재연결 중이고 scrim 팝업이 덮는다.
    /// 다시 연결되면 scrim 이 걷힌다.
    #[test]
    fn a_resync_on_a_connected_row_shows_the_scrim_until_it_is_connected_again() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let row = saved_row(&mut state, &mut session);
        let publish = |session: &mut crate::runtime::engine_session::EngineSession, name| {
            session.remote.row_states = std::collections::HashMap::from([(row, name)]);
        };

        publish(&mut session, "connected");
        sync(&mut state, &session.read(), &layout(row));
        assert!(!state.popups.is_open(REMOTE_ROW_SCRIM_POPUP_ID));

        // Resync 구간: App 은 단계가 Connected 여도 소켓이 끊겼으면 reconnecting 을 싣는다.
        publish(&mut session, "reconnecting");
        let engine = session.read();
        assert_eq!(engine.remote_row_state(row), RemoteRowState::Reconnecting);
        sync(&mut state, &engine, &layout(row));
        assert!(state.popups.is_open(REMOTE_ROW_SCRIM_POPUP_ID));
        assert!(
            !state.popups.is_focused(REMOTE_ROW_SCRIM_POPUP_ID),
            "no focus taken"
        );
        assert!(state.dialogs.remote_row_popup.take_dropped());
        drop(engine);

        publish(&mut session, "connected");
        sync(&mut state, &session.read(), &layout(row));
        assert!(!state.popups.is_open(REMOTE_ROW_SCRIM_POPUP_ID));
    }

    /// 끊긴 행에서 열려 있던 파일 처리기 선택은 닫히고 다시 연결돼도 다시 열리지 않는다(Q1-a).
    #[test]
    fn a_picker_open_on_the_row_closes_when_it_drops_and_stays_closed() {
        let picker = super::super::file_handler_picker::PICKER_POPUP_ID;
        let (mut state, mut session) = crate::state::tests::test_state();
        let row = saved_row(&mut state, &mut session);
        session.remote.row_states = std::collections::HashMap::from([(row, "connected")]);
        sync(&mut state, &session.read(), &layout(row));
        state
            .popups
            .open_with_scope(picker, PopupScope::Workspace(row));
        assert!(state.popups.is_open(picker));

        session.remote.row_states = std::collections::HashMap::from([(row, "reconnecting")]);
        sync(&mut state, &session.read(), &layout(row));
        assert!(!state.popups.is_open(picker));

        session.remote.row_states = std::collections::HashMap::from([(row, "connected")]);
        sync(&mut state, &session.read(), &layout(row));
        assert!(!state.popups.is_open(picker));
    }

    /// 빈 workspace(형상 없음)는 scrim 없는 팝업, 끊긴 형상은 scrim 팝업이다.
    #[test]
    fn the_bare_row_gets_the_popup_without_a_scrim() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let row = saved_row(&mut state, &mut session);
        session.remote.row_states = std::collections::HashMap::from([(row, "not_connected")]);
        sync(&mut state, &session.read(), &layout(row));
        assert!(state.popups.is_open(REMOTE_ROW_POPUP_ID));
        assert!(!state.popups.is_open(REMOTE_ROW_SCRIM_POPUP_ID));
        assert!(!state.dialogs.remote_row_popup.take_dropped());
        assert!(!crate::adapters::ui::PopupManager::popup_has_scrim(
            REMOTE_ROW_POPUP_ID
        ));
        assert!(crate::adapters::ui::PopupManager::popup_has_scrim(
            REMOTE_ROW_SCRIM_POPUP_ID
        ));
    }
}
