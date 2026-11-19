//! 입력 이벤트가 난 범위 경로(window → workspace → pane → tab → surface)를 계산한다.
//! 팝업이 이벤트에 반응할지는 그 팝업의 범위가 이 경로에 있는지로 정한다.
//! 포인터는 좌표가 든 가장 깊은 노드까지, 키보드는 포커스된 surface 와 그 조상을 경로로 쓴다.

use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::popup::PopupScope;
use crate::core::CoreState;
use crate::model::{LogicalPx, LogicalRect};
use crate::state::MainViewState;

/// 이벤트가 속한 범위 노드들. 비어 있는 단계는 이벤트가 그 깊이까지 내려가지 않았다는 뜻이다.
/// 경로는 이 모듈의 계산 함수로만 만든다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScopePath {
    workspace: Option<u32>,
    pane: Option<u32>,
    tab: Option<u32>,
    surface: Option<u32>,
}

impl ScopePath {
    /// Window 만 담은 경로. 모듈 밖에서 계산 없이 경로를 만들지 못하게 `Default` 대신 비공개로 둔다.
    const WINDOW: Self = Self {
        workspace: None,
        pane: None,
        tab: None,
        surface: None,
    };

    /// `scope` 가 이 경로 위에 있는지. `Window` 는 모든 경로의 뿌리라 항상 참이다.
    pub(crate) fn contains(&self, scope: &PopupScope) -> bool {
        match scope {
            PopupScope::Window => true,
            PopupScope::Workspace(id) => self.workspace == Some(*id),
            PopupScope::Pane(id) => self.pane == Some(*id),
            PopupScope::Tab(id) => self.tab == Some(*id),
            PopupScope::Surface(id) => self.surface == Some(*id),
        }
    }
}

/// 좌표 `pos` 의 범위 경로. surface 안이면 surface 와 그 조상을, 같은 탭의 surface 사이 분할선처럼 탭
/// 콘텐츠 안이지만 surface 밖이면 tab 까지를, 탭 바처럼 pane 안이지만 탭 콘텐츠 밖이면 pane 과 workspace
/// 를, workspace 작업 영역 밖(사이드바·상태바)이면 window 만 담는다.
/// Workspace 는 작업 영역(`workspace_rect`) 안일 때만 담는다. 작업 영역이 없는 창은 담지 않는다.
/// 사각형 판정은 포인터의 surface 판정(`surface_id_at_position`)과 같은 반열림 규칙이라, 맞닿은 경계 점은
/// 오른쪽·아래 영역에 든다.
pub(crate) fn path_at(layout: &LayoutContext, pos: egui::Pos2) -> ScopePath {
    let (x, y) = (LogicalPx(pos.x), LogicalPx(pos.y));
    let hit = |r: &egui::Rect| logical(*r).contains(x, y);
    let workspace = layout
        .workspace_rect
        .is_some_and(|r| hit(&r))
        .then_some(layout.active_workspace_id);
    if let Some(&(surface, _)) = layout.surface_rects.iter().find(|(_, r)| hit(r)) {
        let owner = layout.surface_owners.iter().find(|o| o.surface == surface);
        return ScopePath {
            workspace,
            pane: owner.map(|o| o.pane),
            tab: owner.map(|o| o.tab),
            surface: Some(surface),
        };
    }
    if let Some(&(pane, _)) = layout.pane_rects.iter().find(|(_, r)| hit(r)) {
        return ScopePath {
            workspace,
            pane: Some(pane),
            tab: tab_content_at(layout, pane, x, y),
            ..ScopePath::WINDOW
        };
    }
    ScopePath {
        workspace,
        ..ScopePath::WINDOW
    }
}

/// `pane` 의 활성 탭 콘텐츠 영역(`tab_content_rects`, 탭 바 아래) 안이면 그 활성 탭. 콘텐츠 영역은 surface
/// 배치에 쓰는 사각형이라 surface 사이 분할선과 attach 대기 중인 새 탭·새 surface 자리도 덮는다.
fn tab_content_at(layout: &LayoutContext, pane: u32, x: LogicalPx, y: LogicalPx) -> Option<u32> {
    let (_, content) = layout
        .tab_content_rects
        .iter()
        .find(|(id, _)| *id == pane)?;
    if !logical(*content).contains(x, y) {
        return None;
    }
    layout
        .active_tab_ids
        .iter()
        .find(|(id, _)| *id == pane)
        .map(|(_, tab)| *tab)
}

/// egui 가 준 논리 사각형을 길이 타입 사각형으로 옮긴다.
fn logical(r: egui::Rect) -> LogicalRect {
    LogicalRect {
        x: LogicalPx(r.min.x),
        y: LogicalPx(r.min.y),
        width: LogicalPx(r.width()),
        height: LogicalPx(r.height()),
    }
}

/// 포커스된 surface 와 그 조상의 범위 경로. `EngineRead` 는 `CoreState` 로 역참조되므로 두 쪽에서 부를 수
/// 있다. 포커스된 surface 가 없으면 활성 workspace 까지만 담는다.
pub(crate) fn path_of_focus(state: &MainViewState, core: &CoreState) -> ScopePath {
    if core.workspaces().is_empty() {
        return ScopePath::WINDOW;
    }
    let Some(surface) = state.focused_surface_id(core) else {
        return ScopePath {
            workspace: Some(state.active_workspace(core).id),
            ..ScopePath::WINDOW
        };
    };
    let workspace = core
        .find_workspace_index_for_surface(surface)
        .and_then(|(index, _)| core.workspace_at(index))
        .map(|ws| ws.id);
    ScopePath {
        workspace,
        pane: core.find_pane_for_surface(surface),
        tab: core.find_tab_for_surface(surface),
        surface: Some(surface),
    }
}

/// 범위 노드 `scope`와 그 조상의 경로. 범위 대상이 엔진에 없으면 아는 단계까지만 담는다.
pub(crate) fn path_of_scope(core: &CoreState, scope: &PopupScope) -> ScopePath {
    let ws_of_pane = |pane: u32| {
        core.find_workspace_index_for_pane(pane)
            .and_then(|i| core.workspace_at(i))
            .map(|ws| ws.id)
    };
    match *scope {
        PopupScope::Window => ScopePath::WINDOW,
        PopupScope::Workspace(id) => ScopePath {
            workspace: Some(id),
            ..ScopePath::WINDOW
        },
        PopupScope::Pane(pane) => ScopePath {
            workspace: ws_of_pane(pane),
            pane: Some(pane),
            ..ScopePath::WINDOW
        },
        PopupScope::Tab(tab) => {
            let pane = core.find_pane_for_tab(tab);
            ScopePath {
                workspace: pane.and_then(ws_of_pane),
                pane,
                tab: Some(tab),
                surface: None,
            }
        }
        PopupScope::Surface(surface) => {
            let pane = core.find_pane_for_surface(surface);
            ScopePath {
                workspace: pane.and_then(ws_of_pane),
                pane,
                tab: core.find_tab_for_surface(surface),
                surface: Some(surface),
            }
        }
    }
}

/// 포커스 노드의 경로. 포커스된 팝업(키 주인, host·plugin)이 있으면 그 논리 범위의 경로이고, 없으면 엔진
/// 포커스 surface 의 경로다.
pub(crate) fn path_of_focus_node(state: &MainViewState, core: &CoreState) -> ScopePath {
    state.key_owner_scope().map_or_else(
        || path_of_focus(state, core),
        |scope| path_of_scope(core, &scope),
    )
}

/// 누름 하나의 버튼·좌표·누르기 직전 포커스 노드 경로. winit 이 누름을 처리하는 시점에 한 이벤트에서 함께
/// 남긴다. 그 처리(click-to-activate, 선택 시작)가 egui 프레임보다 먼저 포커스를 옮기므로 프레임 안에서 읽으면
/// 이미 옮겨진 값이다. 한 프레임에 누름이 여럿이면 첫 누름이다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PressSnapshot {
    pos: egui::Pos2,
    button: egui::PointerButton,
    focus_before: ScopePath,
}

impl PressSnapshot {
    /// 지금 상태에서 `pos`(logical px)를 `button`으로 누른 스냅샷.
    pub(crate) fn take(
        state: &MainViewState,
        core: &CoreState,
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) -> Self {
        Self {
            pos,
            button,
            focus_before: path_of_focus_node(state, core),
        }
    }
}

/// 한 프레임의 누름 하나. 팝업이 누름에 반응할지는 이 값 하나로 정한다(host·plugin 공통).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RoutedPress {
    /// 누른 좌표(logical px).
    pub(crate) pos: egui::Pos2,
    /// 누른 좌표의 범위 경로.
    pub(crate) path: ScopePath,
    /// 누르기 직전의 포커스 노드 경로. 그 경로가 팝업의 범위를 담지 않았으면 다른 범위에서 들어오는 첫
    /// 클릭이라 바깥 클릭이 아니다.
    pub(crate) focus_before: ScopePath,
    pub(crate) button: egui::PointerButton,
    /// 누름 스냅샷(`MainViewState::take_press_snapshot`)이 이 누름의 포커스 위치를 이미 정했는지. 스냅샷 없이
    /// 프레임에서 만든 누름(별도 창)은 그리기가 같은 함수(`PopupManager::focus_on_press`)로 정한다.
    pub(crate) focus_decided: bool,
}

impl RoutedPress {
    /// 이번 프레임 egui 입력에 눌린 버튼(왼쪽·오른쪽·가운데)과 그 좌표.
    pub(crate) fn egui_press(ctx: &egui::Context) -> Option<(egui::Pos2, egui::PointerButton)> {
        ctx.input(|i| {
            let button = [
                egui::PointerButton::Primary,
                egui::PointerButton::Secondary,
                egui::PointerButton::Middle,
            ]
            .into_iter()
            .find(|b| i.pointer.button_pressed(*b))?;
            Some((i.pointer.interact_pos()?, button))
        })
    }

    /// 스냅샷 하나에서 만든 누름. 좌표·버튼·직전 포커스가 모두 그 스냅샷 것이다.
    pub(crate) fn from_snapshot(snapshot: PressSnapshot, layout: Option<&LayoutContext>) -> Self {
        Self {
            pos: snapshot.pos,
            path: layout.map_or(ScopePath::WINDOW, |l| path_at(l, snapshot.pos)),
            focus_before: snapshot.focus_before,
            button: snapshot.button,
            focus_decided: true,
        }
    }

    /// 포커스 경로를 모르는 그리기(별도 창, 시험)의 누름. 누른 경로를 직전 포커스로 본다. 별도 창은
    /// Window 범위 팝업만 있어 결과가 같다.
    pub(crate) fn from_frame_at_press(
        ctx: &egui::Context,
        layout: Option<&LayoutContext>,
    ) -> Option<Self> {
        let (pos, button) = Self::egui_press(ctx)?;
        let path = layout.map_or(ScopePath::WINDOW, |l| path_at(l, pos));
        Some(Self {
            pos,
            path,
            focus_before: path,
            button,
            focus_decided: false,
        })
    }

    pub(crate) fn is_primary(&self) -> bool {
        self.button == egui::PointerButton::Primary
    }
}

/// 키 입력이 갈 곳.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyTarget {
    /// 이 surface 가 키를 받는다.
    Surface(u32),
    /// 팝업·대화상자 같은 오버레이가 키를 받는다.
    Overlay,
}

/// 지금 입력 경로가 쓰는 키 게이트를 그대로 감싼다. 게이트가 열려 있거나 전체화면 무대가 떠 있으면
/// `Overlay`, 아니면 포커스된 surface 다. 무대는 창의 키 처리가 게이트보다 먼저 소비하므로 같은 답을
/// 낸다. 포커스된 surface 가 없으면 `None` 이다.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "key routing adopts the focus path in a later step"
    )
)]
pub(crate) fn keyboard_target(state: &MainViewState, core: &CoreState) -> Option<KeyTarget> {
    if state.window_keys_to_overlay() {
        return Some(KeyTarget::Overlay);
    }
    state.focused_surface_id(core).map(KeyTarget::Surface)
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
