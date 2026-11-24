//! 창 오른쪽 위 안내 열에 띄우는 복원 실패·저장 멈춤 안내. 안내 한 장이 창 범위 팝업 인스턴스 하나다.
//!
//! 안내 내용은 App이 넘긴다. 안내는 포커스를 받지 않고 범위를 덮지 않으며 바깥 클릭에 사라지지 않는다.
//! 다른 팝업 아래에 그린다. 바깥의 키·선택·스크롤은 터미널로 그대로 간다. ×·Continue·안쪽 컨트롤
//! 포커스의 Esc는 같은 응답이며, 그 안내를 닫고 그 id를 App이 확인 응답으로 가져가게 남긴다.

use crate::adapters::ui::popup::{PopupAction, PopupCx, PopupId, PopupPayload, PopupScope};
use crate::model::{LogicalPx, LogicalRect, PhysicalRect};
use crate::state::MainViewState;

/// 복원 안내 팝업 선언 id. 안내마다 창 범위 인스턴스 하나다.
pub const RESTORE_NOTICE_POPUP_ID: PopupId = "restore_notice";

/// 안내 한 장의 표시 문구. 모두 번역된 문자열이다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestoreNoticeEntry {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) cause: String,
    pub(crate) label: String,
    pub(crate) path: String,
    pub(crate) body: String,
}

impl RestoreNoticeEntry {
    /// 같은 안내인지 가린다. 제목·원인·경로가 같으면 열에 다시 쌓지 않는다.
    fn same_as(&self, other: &Self) -> bool {
        self.title == other.title && self.cause == other.cause && self.path == other.path
    }
}

/// 안내 인스턴스의 데이터.
#[derive(Debug)]
pub(crate) struct NoticePayload {
    pub(crate) entry: RestoreNoticeEntry,
    /// 직전 프레임에 그린 안내 높이. 처음 그리기 전에는 없다.
    height: Option<LogicalPx>,
    /// 한 번이라도 그렸는지. 프레임을 그리지 않는 halt 중에 창을 다시 그리게 한다.
    drawn: bool,
    /// 직전 프레임에 안쪽 컨트롤이 키보드 포커스를 가졌는지. 이때만 키를 egui로 보낸다.
    focused_inside: bool,
}

/// 창이 가진 안내 열의 결과.
#[derive(Default)]
pub(crate) struct RestoreNotices {
    /// 사용자가 닫은 안내 id. App이 가져간다.
    acknowledged: Vec<u64>,
    /// 이번 프레임에 안내 열이 차지한 사각형. 안내가 없으면 `None`이다. 같은 열 아래에 이어 그리는 attach
    /// 대기 칩은 이 값의 아래쪽 + `notice-column-gap`에서 시작한다.
    pub(crate) column: Option<egui::Rect>,
    /// 팝업을 그린 뒤의 배너·범위 위쪽 팝업 자리로 다시 잰 안내 열의 위쪽. attach 대기 칩이 쓴다.
    pub(crate) column_top: Option<LogicalPx>,
}

impl RestoreNotices {
    pub(crate) fn take_acknowledged(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.acknowledged)
    }
}

/// 열린 안내 인스턴스와 그 데이터.
fn notices(
    popups: &crate::adapters::ui::PopupManager,
) -> impl Iterator<Item = (&crate::adapters::ui::popup::PopupState, &NoticePayload)> {
    popups
        .instances_of(RESTORE_NOTICE_POPUP_ID)
        .filter_map(|p| match &p.payload {
            PopupPayload::RestoreNotice(notice) => Some((p, &**notice)),
            _ => None,
        })
}

/// 안내를 연다. 같은 안내가 이미 떠 있으면 열지 않고 false다.
pub(crate) fn push(state: &mut MainViewState, entry: RestoreNoticeEntry) -> bool {
    if notices(&state.popups).any(|(_, shown)| shown.entry.same_as(&entry)) {
        return false;
    }
    // intent-exempt: 복원 실패·저장 멈춤이라는 시스템 사건이 연다. halt 중에는 Intent를 적용하지 않으므로 직접 연다.
    state.popups.open_request(
        &crate::intent::OpenRequest::new(RESTORE_NOTICE_POPUP_ID, PopupScope::Window).with_payload(
            PopupPayload::RestoreNotice(Box::new(NoticePayload {
                entry,
                height: None,
                drawn: false,
                focused_inside: false,
            })),
        ),
        false,
    );
    true
}

/// 아직 그리지 않은 안내가 있는지.
pub(crate) fn has_undrawn(state: &MainViewState) -> bool {
    notices(&state.popups).any(|(_, notice)| !notice.drawn)
}

/// 떠 있는 안내 수.
pub(crate) fn count(state: &MainViewState) -> usize {
    notices(&state.popups).count()
}

/// 안내 안쪽 컨트롤이 키보드 포커스를 가졌는지. 안내가 나타나는 것만으로는 true가 되지 않는다.
pub(crate) fn has_keyboard_focus(popups: &crate::adapters::ui::PopupManager) -> bool {
    notices(popups).any(|(_, notice)| notice.focused_inside)
}

/// 처음 그리기 전 안내 높이. 그린 프레임에 잰 높이로 바뀐다.
pub(crate) const UNMEASURED_HEIGHT: LogicalPx = LogicalPx(1.0);

/// 안내 크기. 폭은 관리자가 열 폭으로 맞추고, 높이는 직전 프레임에 잰 값이다.
pub(crate) fn notice_size(
    _: &MainViewState,
    _: &crate::runtime::engine_read::EngineRead<'_>,
    payload: &PopupPayload,
) -> egui::Vec2 {
    let width = crate::theme::theme().notice_column_width().value();
    let height = match payload {
        PopupPayload::RestoreNotice(notice) => notice.height.unwrap_or(UNMEASURED_HEIGHT),
        _ => UNMEASURED_HEIGHT,
    };
    egui::vec2(width, height.value())
}

/// 닫힌 안내의 id를 App이 확인 응답으로 가져가게 남긴다.
pub(crate) fn on_close_notice(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    if let PopupPayload::RestoreNotice(notice) = &closed.payload {
        state.restore_notices.acknowledged.push(notice.entry.id);
    }
}

/// 안내 한 장을 그린다. ×·Continue·안쪽 포커스의 Esc면 닫는다.
pub(crate) fn draw_notice_popup(
    ui: &mut egui::Ui,
    _state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    cx: &mut PopupCx<'_>,
) -> PopupAction {
    let PopupPayload::RestoreNotice(notice) = &mut *cx.payload else {
        return PopupAction::Close;
    };
    let theme = crate::theme::theme();
    let entry = &notice.entry;
    let view = tasty_ui_widgets::RestoreNoticeView {
        id_salt: egui::Id::new("restore_notice").with(entry.id),
        title: &entry.title,
        close_label: crate::i18n::t("notice.close"),
        cause: &entry.cause,
        label: &entry.label,
        path: &entry.path,
        copy_label: crate::i18n::t("notice.copy_path"),
        body: &entry.body,
        continue_label: crate::i18n::t("notice.continue"),
        title_bar_height: super::popup::title_bar_height(),
    };
    // 카드의 그림자와 바깥쪽 테두리는 카드 사각형 밖에 그린다. 관리자가 정한 콘텐츠 자르기를 창 전체로 넓힌다.
    ui.set_clip_rect(ui.ctx().screen_rect());
    let width = ui.max_rect().width();
    let output = tasty_ui_widgets::restore_notice(ui, &theme, &view, width);
    if let Some(error) = &output.glyph_error {
        tracing::warn!("restore notice glyph failed to load: {error}");
    }
    if output.copy_path {
        ui.ctx().copy_text(entry.path.clone());
    }
    let height = LogicalPx(output.rect.height());
    if notice.height != Some(height) {
        // 잰 높이로 다음 프레임에 열을 다시 쌓는다.
        notice.height = Some(height);
        ui.ctx().request_repaint();
    }
    notice.drawn = true;
    notice.focused_inside = output.focused_inside && !output.dismissed;
    if output.dismissed {
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 열이 시작할 y. 탭 바 아래(`base`)에서 시작하되, 열과 가로로 겹치고 열 시작점보다 위에서
/// 시작하는 장애물(창 범위 배너, 범위 위쪽에 붙은 팝업)이 있으면 그 아래로 내린다.
/// 장애물은 옮기지 않는다. 반환값에는 `notice_column_rect`가 inset을 더한다.
fn column_top(
    base: LogicalPx,
    inset: LogicalPx,
    columns: impl Fn(LogicalPx) -> LogicalRect,
    obstacles: &[LogicalRect],
) -> LogicalPx {
    let mut top = base;
    let mut sorted: Vec<LogicalRect> = obstacles.to_vec();
    sorted.sort_by(|a, b| a.y.value().total_cmp(&b.y.value()));
    for obstacle in sorted {
        let column = columns(top);
        let beside =
            obstacle.x + obstacle.width <= column.x || obstacle.x >= column.x + column.width;
        let bottom = obstacle.y + obstacle.height;
        if !beside && obstacle.y <= column.y + inset && bottom > top {
            top = bottom;
        }
    }
    top
}

/// egui가 돌려준 논리 사각형을 배치 계산용 타입으로 옮긴다.
fn logical_rect(rect: egui::Rect) -> LogicalRect {
    LogicalRect {
        x: LogicalPx(rect.min.x),
        y: LogicalPx(rect.min.y),
        width: LogicalPx(rect.width()),
        height: LogicalPx(rect.height()),
    }
}

/// 작업 영역(`terminal_rect`) 오른쪽 위, 탭 바 아래의 안내 열 자리. `obstacles`는 창 범위 배너와 범위
/// 위쪽에 붙은 팝업(열린 검색 막대 등)의 사각형이다. 열은 그 아래에서 시작한다.
pub(crate) fn notice_column(
    terminal_rect: PhysicalRect,
    tab_bar_height: crate::model::PhysicalPx,
    scale: f32,
    obstacles: &[egui::Rect],
) -> crate::adapters::ui::popup::NoticeColumn {
    let theme = crate::theme::theme();
    let work = super::to_egui_rect(terminal_rect, scale);
    let base = LogicalPx(work.top()) + tab_bar_height.to_logical(scale);
    let columns = |top: LogicalPx| tasty_ui_widgets::notice_column_rect(&theme, work, top.value());
    let obstacles: Vec<LogicalRect> = obstacles.iter().copied().map(logical_rect).collect();
    let top = column_top(
        base,
        theme.notice_column_inset(),
        |top| logical_rect(columns(top)),
        &obstacles,
    );
    crate::adapters::ui::popup::NoticeColumn {
        rect: columns(top),
        gap: theme.notice_column_gap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> LogicalRect {
        LogicalRect {
            x: LogicalPx(x),
            y: LogicalPx(y),
            width: LogicalPx(width),
            height: LogicalPx(height),
        }
    }

    fn columns(top: LogicalPx) -> LogicalRect {
        rect(632.0, top.value() + 8.0, 360.0, 100.0)
    }

    const INSET: LogicalPx = LogicalPx(8.0);

    #[test]
    fn the_column_starts_under_a_window_banner_and_an_open_search_bar() {
        let banner = rect(0.0, 36.0, 1000.0, 32.0);
        let search = rect(700.0, 76.0, 292.0, 32.0);
        let base = LogicalPx(36.0);
        assert_eq!(column_top(base, INSET, columns, &[]), base);
        assert_eq!(column_top(base, INSET, columns, &[banner]), LogicalPx(68.0));
        assert_eq!(
            column_top(base, INSET, columns, &[search, banner]),
            LogicalPx(108.0)
        );
    }

    #[test]
    fn obstacles_beside_or_below_the_column_do_not_move_it() {
        let left = rect(0.0, 36.0, 600.0, 32.0);
        let lower = rect(700.0, 400.0, 292.0, 32.0);
        let base = LogicalPx(36.0);
        assert_eq!(column_top(base, INSET, columns, &[left, lower]), base);
    }
}

#[cfg(test)]
#[path = "restore_notice_popup_tests.rs"]
mod popup_tests;
