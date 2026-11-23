//! 우클릭 메뉴의 "이동" / "이곳으로 이동" 2단계 조작(서피스·탭·페인). 대기 슬롯은 종류를 담는 하나뿐이다.
//! 사용자 우클릭 조작이라 GUI에서만 연다. 명세: docs/features/surface-move/index.md.

use super::MainView;
use crate::platform::native_menu::MenuItem;
use crate::state::PendingMove;

/// surface 메뉴 항목 id. 같은 메뉴의 다른 항목 id와 겹치지 않는다.
pub(super) const ITEM_MOVE_SURFACE: u32 = 10;
pub(super) const ITEM_MOVE_SURFACE_HERE: u32 = 11;

/// 탭 메뉴 항목 id. 탭 메뉴의 기존 항목(1~6)과 겹치지 않는다.
const ITEM_MOVE_TAB: u32 = 7;
const ITEM_MOVE_TAB_HERE: u32 = 8;
const ITEM_MOVE_PANE: u32 = 9;
const ITEM_MOVE_PANE_HERE: u32 = 10;

impl MainView {
    /// "서피스 이동"과 "서피스를 이곳으로 이동"을 붙인다. 뒤의 것은 서피스가 대기 중일 때만 켜진다.
    /// 자기 자신이 대기 중이어도 켠다(고르면 슬롯만 비운다).
    pub(super) fn push_surface_move_items(pending: Option<PendingMove>, items: &mut Vec<MenuItem>) {
        items.push(MenuItem::new(
            ITEM_MOVE_SURFACE,
            crate::i18n::t("surface_context_menu.move"),
        ));
        items.push(MenuItem {
            enabled: matches!(pending, Some(PendingMove::Surface(_))),
            ..MenuItem::new(
                ITEM_MOVE_SURFACE_HERE,
                crate::i18n::t("surface_context_menu.move_here"),
            )
        });
    }

    /// surface 메뉴의 이동 항목을 처리한다. 이동 항목이 아니면 false를 반환한다.
    pub(super) fn apply_surface_move_selection(&mut self, surface_id: u32, item: u32) -> bool {
        match item {
            ITEM_MOVE_SURFACE => {
                // 도메인 구조는 바꾸지 않고 대기 슬롯만 덮어쓴다.
                self.state.pending_move = Some(PendingMove::Surface(surface_id));
                self.state.toasts.push_info(
                    crate::i18n::t("toast.surface_cut"),
                    crate::adapters::ui::ToastScope::Surface(surface_id),
                );
                true
            }
            ITEM_MOVE_SURFACE_HERE => {
                if let Some(PendingMove::Surface(source)) = self.state.pending_move {
                    self.state.pending_move = None;
                    self.state.dispatch_intent(
                        crate::app::command::DomainIntent::MoveSurface {
                            source_surface_id: source,
                            target_surface_id: surface_id,
                        }
                        .from_user_context_menu(),
                    );
                }
                true
            }
            _ => false,
        }
    }

    /// 탭 메뉴 끝에 구분선과 "탭 이동"·"탭을 이곳으로 이동"을 붙인다. 뒤의 것은 다른 탭이 대기 중일 때만 켜진다.
    /// 그 탭이 속한 페인(`pane_id`)이 있으면 이어서 구분선과 "페인 이동"·"페인을 이곳으로 이동"을 붙이고,
    /// 뒤의 것은 다른 페인이 대기 중일 때만 켜진다.
    pub(super) fn push_tab_move_items(
        pending: Option<PendingMove>,
        items: &mut Vec<MenuItem>,
        tab_id: u32,
        pane_id: Option<u32>,
    ) {
        items.push(MenuItem::separator());
        items.push(MenuItem::new(
            ITEM_MOVE_TAB,
            crate::i18n::t("tab_context_menu.move_tab"),
        ));
        items.push(MenuItem {
            enabled: matches!(pending, Some(PendingMove::Tab(id)) if id != tab_id),
            ..MenuItem::new(
                ITEM_MOVE_TAB_HERE,
                crate::i18n::t("tab_context_menu.move_tab_here"),
            )
        });
        let Some(pane_id) = pane_id else {
            return;
        };
        items.push(MenuItem::separator());
        items.push(MenuItem::new(
            ITEM_MOVE_PANE,
            crate::i18n::t("tab_context_menu.move_pane"),
        ));
        items.push(MenuItem {
            enabled: matches!(pending, Some(PendingMove::Pane(id)) if id != pane_id),
            ..MenuItem::new(
                ITEM_MOVE_PANE_HERE,
                crate::i18n::t("tab_context_menu.move_pane_here"),
            )
        });
    }

    /// 탭 메뉴의 탭·페인 이동 항목을 처리한다. 이동 항목이 아니면 false를 반환한다.
    /// 페인 항목의 대상은 우클릭한 탭이 지금 속한 페인이다. 메뉴가 열린 동안 탭이 닫혔으면 아무것도 하지 않는다.
    pub(super) fn apply_tab_move_selection(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        tab_id: u32,
        item: u32,
    ) -> bool {
        match item {
            ITEM_MOVE_TAB => {
                if let Some(pane_id) = engine.find_pane_for_tab(tab_id) {
                    self.state.pending_move = Some(PendingMove::Tab(tab_id));
                    self.state.toasts.push_info(
                        crate::i18n::t("toast.tab_cut"),
                        crate::adapters::ui::ToastScope::Pane(pane_id),
                    );
                }
                true
            }
            ITEM_MOVE_TAB_HERE => {
                if engine.find_pane_for_tab(tab_id).is_some()
                    && let Some(PendingMove::Tab(source)) = self.state.pending_move
                {
                    self.state.pending_move = None;
                    self.state.dispatch_intent(
                        crate::app::command::DomainIntent::ReplaceTabWithTab {
                            source_tab_id: source,
                            target_tab_id: tab_id,
                        }
                        .from_user_context_menu(),
                    );
                }
                true
            }
            ITEM_MOVE_PANE => {
                if let Some(pane_id) = engine.find_pane_for_tab(tab_id) {
                    self.state.pending_move = Some(PendingMove::Pane(pane_id));
                    self.state.toasts.push_info(
                        crate::i18n::t("toast.pane_cut"),
                        crate::adapters::ui::ToastScope::Pane(pane_id),
                    );
                }
                true
            }
            ITEM_MOVE_PANE_HERE => {
                if let Some(pane_id) = engine.find_pane_for_tab(tab_id)
                    && let Some(PendingMove::Pane(source)) = self.state.pending_move
                {
                    self.state.pending_move = None;
                    self.state.dispatch_intent(
                        crate::app::command::DomainIntent::ReplacePaneWithPane {
                            source_pane_id: source,
                            target_pane_id: pane_id,
                        }
                        .from_user_context_menu(),
                    );
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (id, 활성). 구분선은 None.
    fn shape(items: &[MenuItem]) -> Vec<Option<(u32, bool)>> {
        items
            .iter()
            .map(|i| (!i.is_separator()).then_some((i.id, i.enabled)))
            .collect()
    }

    fn surface_items(pending: Option<PendingMove>) -> Vec<Option<(u32, bool)>> {
        let mut items = Vec::new();
        MainView::push_surface_move_items(pending, &mut items);
        shape(&items)
    }

    fn tab_items(pending: Option<PendingMove>, pane_id: Option<u32>) -> Vec<Option<(u32, bool)>> {
        let mut items = Vec::new();
        MainView::push_tab_move_items(pending, &mut items, 5, pane_id);
        shape(&items)
    }

    #[test]
    fn move_surface_here_stays_and_is_enabled_only_for_a_waiting_surface() {
        let here = |enabled| {
            vec![
                Some((ITEM_MOVE_SURFACE, true)),
                Some((ITEM_MOVE_SURFACE_HERE, enabled)),
            ]
        };
        assert_eq!(surface_items(None), here(false));
        assert_eq!(surface_items(Some(PendingMove::Tab(1))), here(false));
        assert_eq!(surface_items(Some(PendingMove::Pane(1))), here(false));
        assert_eq!(surface_items(Some(PendingMove::Surface(3))), here(true));
    }

    #[test]
    fn move_tab_and_pane_here_stay_and_are_enabled_only_for_another_waiting_target() {
        let both = |tab: bool, pane: bool| {
            vec![
                None,
                Some((ITEM_MOVE_TAB, true)),
                Some((ITEM_MOVE_TAB_HERE, tab)),
                None,
                Some((ITEM_MOVE_PANE, true)),
                Some((ITEM_MOVE_PANE_HERE, pane)),
            ]
        };
        // 탭 5, 페인 8 을 우클릭했다.
        assert_eq!(tab_items(None, Some(8)), both(false, false));
        assert_eq!(
            tab_items(Some(PendingMove::Surface(1)), Some(8)),
            both(false, false)
        );
        assert_eq!(
            tab_items(Some(PendingMove::Tab(5)), Some(8)),
            both(false, false),
            "itself"
        );
        assert_eq!(
            tab_items(Some(PendingMove::Tab(6)), Some(8)),
            both(true, false)
        );
        assert_eq!(
            tab_items(Some(PendingMove::Pane(8)), Some(8)),
            both(false, false),
            "its pane"
        );
        assert_eq!(
            tab_items(Some(PendingMove::Pane(9)), Some(8)),
            both(false, true)
        );
    }

    #[test]
    fn a_tab_without_a_pane_gets_only_the_tab_group() {
        assert_eq!(
            tab_items(Some(PendingMove::Tab(6)), None),
            vec![
                None,
                Some((ITEM_MOVE_TAB, true)),
                Some((ITEM_MOVE_TAB_HERE, true))
            ]
        );
    }
}
