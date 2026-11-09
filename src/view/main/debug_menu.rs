//! debug 전용 native 메뉴 응답과 이동 대기 조회. release에는 없다.
//! 호출자가 지정한 종류의 native 메뉴가 다음에 열릴 때 OS 팝업 없이 지정한 항목을 고른 것으로 끝낸다.
//! 다른 종류의 메뉴는 응답을 쓰지 않고 실제로 연다. 메뉴 항목을 만드는 일과 선택 뒤 처리는 사용자 우클릭과
//! 같은 경로를 쓴다. egui가 만드는 메뉴는 `debug.inject_egui_mouse`, 터미널 메뉴는 `debug.inject_window_mouse`
//! 우클릭으로 연다. 같은 종류의 응답이 있으면 `TASTY_DEBUG_SUPPRESS_NATIVE_MENU`의 포획보다 응답을 먼저 쓴다.

#![cfg(debug_assertions)]

use crate::platform::native_menu::MenuItem;
use crate::state::{PendingMove, PendingNativeMenu};

use super::MainView;

/// `open_native_menu`에서 둔 응답이 있으면 그 선택으로 끝내고 돌아가며, 없으면 `cont`를 되돌려 받는다.
/// 동결 파일(`redraw.rs`)에 남는 debug 줄을 한 문장으로 줄이려고 매크로로 둔다.
macro_rules! answer_or_return {
    ($view:expr, $engine:expr, $items:expr, $cont:ident) => {
        let Err($cont) = $view.debug_answer_menu($engine, $items, $cont) else {
            return;
        };
    };
}
pub(super) use answer_or_return;

/// `process_pending_native_menu`에서 메뉴 요청을 포획하거나(`TASTY_DEBUG_SUPPRESS_NATIVE_MENU`), 열 메뉴의
/// 종류를 기록한 뒤 요청을 돌려받는다. 포획했으면 돌아간다.
macro_rules! capture_or_note {
    ($view:expr, $pending:ident) => {
        let Some($pending) = $view.debug_note_menu($pending) else {
            return;
        };
    };
}
pub(super) use capture_or_note;

/// 메뉴 요청의 종류 이름과 대상 surface. `debug.pending_menu`의 `kind`와 응답의 `menu`가 같은 이름을 쓴다.
pub(crate) fn menu_kind(menu: &PendingNativeMenu) -> (&'static str, Option<u32>) {
    use PendingNativeMenu as M;
    match menu {
        M::Tab { .. } => ("Tab", None),
        M::Pane { .. } => ("Pane", None),
        M::Workspace { .. } => ("Workspace", None),
        M::TerminalSurface { surface_id, .. } => ("TerminalSurface", Some(*surface_id)),
        M::TerminalLink { link, .. } => ("TerminalLink", Some(link.surface_id)),
        M::Surface { surface_id, .. } => ("Surface", Some(*surface_id)),
        M::WebviewSurface { surface_id, .. } => ("WebviewSurface", Some(*surface_id)),
        M::Explorer { surface_id, .. } => ("Explorer", Some(*surface_id)),
        M::ExplorerFavorite { surface_id, .. } => ("ExplorerFavorite", Some(*surface_id)),
        M::ExplorerMore { surface_id, .. } => ("ExplorerMore", Some(*surface_id)),
        M::NewWorkspaceButton { .. } => ("NewWorkspaceButton", None),
        M::WorkspaceCategoryHeader { .. } => ("WorkspaceCategoryHeader", None),
        M::SidebarBackground { .. } => ("SidebarBackground", None),
        M::NewTabButton { .. } => ("NewTabButton", None),
    }
}

/// 응답이 받는 메뉴 종류 이름. [`menu_kind`]의 이름과 같다.
pub(crate) const MENU_KINDS: &[&str] = &[
    "Tab",
    "Pane",
    "Workspace",
    "TerminalSurface",
    "TerminalLink",
    "Surface",
    "WebviewSurface",
    "Explorer",
    "ExplorerFavorite",
    "ExplorerMore",
    "NewWorkspaceButton",
    "WorkspaceCategoryHeader",
    "SidebarBackground",
    "NewTabButton",
];

/// 지정한 종류의 다음 native 메뉴에 줄 응답.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MenuAnswer {
    /// 응답을 쓸 메뉴 종류([`MENU_KINDS`] 중 하나).
    pub menu: &'static str,
    pub choice: MenuChoice,
}

/// 메뉴에서 고를 것.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MenuChoice {
    /// 항목 id로 고른다.
    Item(u32),
    /// 표시 문구가 같은 항목을 고른다.
    Label(String),
    /// 항목을 고르지 않고 닫는다.
    Dismiss,
}

impl MainView {
    /// 지정한 종류의 다음 native 메뉴 하나에 쓸 응답을 둔다. 앞서 둔 응답이 남아 있으면 덮어쓰고 그 종류를 돌려준다.
    pub(crate) fn debug_set_menu_answer(&mut self, answer: MenuAnswer) -> Option<&'static str> {
        self.debug_menu_answer
            .replace(answer)
            .map(|previous| previous.menu)
    }

    /// 포획 모드면 요청을 보관하고 None을 돌려준다. 아니면 열 메뉴의 종류를 기록하고 요청을 돌려준다.
    /// 같은 종류의 응답이 있으면 포획 모드여도 메뉴를 연다. 응답이 OS 팝업 없이 선택을 끝낸다.
    pub(super) fn debug_note_menu(
        &mut self,
        pending: PendingNativeMenu,
    ) -> Option<PendingNativeMenu> {
        let kind = menu_kind(&pending).0;
        let answered = answer_applies(self.debug_menu_answer.as_ref(), Some(kind));
        if !answered && std::env::var_os("TASTY_DEBUG_SUPPRESS_NATIVE_MENU").is_some() {
            self.debug_captured_menu = Some(pending);
            return None;
        }
        self.debug_opening_menu = Some(kind);
        Some(pending)
    }

    /// `debug.inject_window_mouse`가 세운 메뉴 요청을 OS가 표시하기 전에 나눈다. 같은 종류의 응답이 있으면
    /// 요청을 슬롯에 남겨 다음 프레임의 `process_pending_native_menu`가 사용자 우클릭과 같은 경로로 응답을
    /// 쓰게 한다. 응답이 없거나 다른 종류면 요청을 포획해 OS 팝업을 띄우지 않고 응답도 그대로 둔다.
    pub(super) fn debug_route_injected_menu(&mut self) {
        let Some(menu) = self.state.dialogs.pending_native_menu.take() else {
            return;
        };
        if answer_applies(self.debug_menu_answer.as_ref(), Some(menu_kind(&menu).0)) {
            self.state.dialogs.pending_native_menu = Some(menu);
        } else {
            self.debug_captured_menu = Some(menu);
        }
    }

    /// 둔 응답이 있으면 OS 메뉴 대신 그 선택으로 `cont`를 실행한다(macOS·Windows의 즉시 완료와 같은 순서).
    /// 응답이 없으면 `cont`를 돌려줘 호출부가 실제 메뉴를 연다.
    pub(super) fn debug_answer_menu<F>(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        items: &[MenuItem],
        cont: F,
    ) -> Result<(), F>
    where
        F: FnOnce(&mut MainView, &crate::runtime::engine_read::EngineRead<'_>, Option<u32>),
    {
        let opening = self.debug_opening_menu.take();
        if !answer_applies(self.debug_menu_answer.as_ref(), opening) {
            return Err(cont);
        }
        let Some(answer) = self.debug_menu_answer.take() else {
            return Err(cont);
        };
        let result = resolve_answer(&answer.choice, items);
        if result.is_none() && answer.choice != MenuChoice::Dismiss {
            tracing::warn!("debug menu answer {answer:?} matched no enabled item; menu dismissed");
        }
        cont(self, engine, result);
        crate::view::ui::View::mark_dirty(self);
        Ok(())
    }

    /// 이 창의 이동 대기 슬롯.
    pub(crate) fn debug_pending_move(&self) -> serde_json::Value {
        pending_move_json(self.state.pending_move)
    }
}

/// 응답은 지정한 종류의 메뉴가 열릴 때만 쓴다. 종류를 모르는 메뉴에도 쓰지 않는다.
fn answer_applies(answer: Option<&MenuAnswer>, opening: Option<&str>) -> bool {
    matches!((answer, opening), (Some(a), Some(kind)) if a.menu == kind)
}

/// 사용자가 고를 수 있는 항목만 고른다. 구분선과 비활성 항목은 고를 수 없다.
fn resolve_answer(choice: &MenuChoice, items: &[MenuItem]) -> Option<u32> {
    let pickable = |item: &&MenuItem| item.enabled && !item.is_separator();
    match choice {
        MenuChoice::Item(id) => items.iter().filter(pickable).find(|i| i.id == *id),
        MenuChoice::Label(label) => items.iter().filter(pickable).find(|i| i.label == *label),
        MenuChoice::Dismiss => None,
    }
    .map(|item| item.id)
}

fn pending_move_json(pending: Option<PendingMove>) -> serde_json::Value {
    let (kind, id) = match pending {
        None => return serde_json::json!({ "present": false }),
        Some(PendingMove::Surface(id)) => ("surface", id),
        Some(PendingMove::Tab(id)) => ("tab", id),
        Some(PendingMove::Pane(id)) => ("pane", id),
    };
    serde_json::json!({ "present": true, "kind": kind, "id": id })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<MenuItem> {
        vec![
            MenuItem::new(1, "Close Tab"),
            MenuItem::separator(),
            MenuItem::new(7, "Move Tab"),
            MenuItem::disabled(9, "Move Pane"),
        ]
    }

    #[test]
    fn an_answer_picks_an_enabled_item_by_id_or_label() {
        assert_eq!(resolve_answer(&MenuChoice::Item(7), &items()), Some(7));
        assert_eq!(
            resolve_answer(&MenuChoice::Label("Move Tab".into()), &items()),
            Some(7)
        );
    }

    #[test]
    fn an_answer_never_picks_what_the_user_cannot_pick() {
        assert_eq!(resolve_answer(&MenuChoice::Item(9), &items()), None);
        assert_eq!(resolve_answer(&MenuChoice::Item(0), &items()), None);
        assert_eq!(resolve_answer(&MenuChoice::Item(42), &items()), None);
        assert_eq!(
            resolve_answer(&MenuChoice::Label(String::new()), &items()),
            None
        );
        assert_eq!(resolve_answer(&MenuChoice::Dismiss, &items()), None);
    }

    #[test]
    fn an_answer_applies_only_to_its_own_menu_kind() {
        let answer = MenuAnswer {
            menu: "Tab",
            choice: MenuChoice::Dismiss,
        };
        assert!(answer_applies(Some(&answer), Some("Tab")));
        assert!(!answer_applies(Some(&answer), Some("TerminalSurface")));
        assert!(!answer_applies(Some(&answer), None));
        assert!(!answer_applies(None, Some("TerminalSurface")));
    }

    fn answer(menu: &'static str) -> MenuAnswer {
        MenuAnswer {
            menu,
            choice: MenuChoice::Item(7),
        }
    }

    #[test]
    fn an_answer_applies_only_to_the_menu_kind_it_names() {
        assert!(answer_applies(Some(&answer("Tab")), Some("Tab")));
        assert!(!answer_applies(
            Some(&answer("Tab")),
            Some("TerminalSurface")
        ));
        assert!(!answer_applies(Some(&answer("Tab")), None));
        assert!(!answer_applies(None, Some("Tab")));
    }

    /// 응답이 받는 이름 목록과 메뉴 요청의 종류 이름이 같은 집합이다.
    #[test]
    fn the_answer_kinds_are_exactly_the_menu_kinds() {
        use crate::state::TerminalLinkMenu;
        use std::path::PathBuf;
        let point = tasty_selection::SelectionPoint {
            epoch: Default::default(),
            col: 0,
            absolute_row: 0,
        };
        let (x, y) = (0.0, 0.0);
        let menus = [
            PendingNativeMenu::Tab {
                pane_id: 1,
                tab_index: 0,
                x,
                y,
            },
            PendingNativeMenu::Pane { pane_id: 1, x, y },
            PendingNativeMenu::Workspace { ws_idx: 0, x, y },
            PendingNativeMenu::TerminalSurface {
                surface_id: 1,
                x,
                y,
            },
            PendingNativeMenu::TerminalLink {
                link: TerminalLinkMenu {
                    surface_id: 1,
                    start: point,
                    end: point,
                    text: String::new(),
                    open_with: None,
                    remote_path: false,
                },
                x,
                y,
            },
            PendingNativeMenu::WebviewSurface {
                surface_id: 1,
                plugin_id: String::new(),
                page_x: 0.0,
                page_y: 0.0,
                x,
                y,
            },
            PendingNativeMenu::Surface {
                surface_id: 1,
                x,
                y,
            },
            PendingNativeMenu::Explorer {
                surface_id: 1,
                paths: Vec::new(),
                cwd: PathBuf::new(),
                single_is_dir: false,
                x,
                y,
            },
            PendingNativeMenu::ExplorerFavorite {
                surface_id: 1,
                path: PathBuf::new(),
                x,
                y,
            },
            PendingNativeMenu::ExplorerMore {
                surface_id: 1,
                x,
                y,
            },
            PendingNativeMenu::NewWorkspaceButton { x, y },
            PendingNativeMenu::WorkspaceCategoryHeader { cat_id: 1, x, y },
            PendingNativeMenu::SidebarBackground { x, y },
            PendingNativeMenu::NewTabButton { pane_id: 1, x, y },
        ];
        let named: std::collections::BTreeSet<&str> =
            menus.iter().map(|m| menu_kind(m).0).collect();
        let listed: std::collections::BTreeSet<&str> = MENU_KINDS.iter().copied().collect();
        assert_eq!(named, listed);
        assert_eq!(listed.len(), MENU_KINDS.len(), "이름이 중복됐다");
    }

    #[test]
    fn the_pending_move_reads_back_its_kind_and_id() {
        assert_eq!(
            pending_move_json(None),
            serde_json::json!({ "present": false })
        );
        assert_eq!(
            pending_move_json(Some(PendingMove::Tab(5))),
            serde_json::json!({ "present": true, "kind": "tab", "id": 5 })
        );
        assert_eq!(
            pending_move_json(Some(PendingMove::Pane(3)))["kind"],
            "pane"
        );
        assert_eq!(
            pending_move_json(Some(PendingMove::Surface(9)))["kind"],
            "surface"
        );
    }
}
