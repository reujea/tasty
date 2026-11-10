//! `Tick::Busy` 처리 — 모든 surface 의 busy 상태 갱신.

use crate::app::App;
use crate::view::ui::View as _;

impl App {
    /// Busy tick에서 상태를 갱신하고 busy·attention·cwd·전경 이름 변경을 점유 클라이언트에 전송한다.
    /// 전송 지연이나 실패가 있을 수 있어 원격 반영 시각은 보장하지 않는다.
    pub(crate) fn poll_busy_states(&mut self) {
        let hub = self.stream_hub.clone();
        for (_, main, mut engine) in self.engines_mut().window_pairs() {
            let mut changed = crate::app::services::AppServices::update_busy_surfaces(&mut engine);
            // 상태바는 포커스된 surface만 표시하므로 불필요한 Git 조회를 피한다.
            let focused = main.state.focused_surface_id(&engine.read());
            changed |= main
                .state
                .refresh_status_bar_branch(&engine.read(), focused);
            engine.forward_busy_activity(&hub);
            engine.forward_attention(&hub);
            engine.forward_surface_cwd(&hub);
            engine.forward_surface_foreground(&hub);
            close_stale_mouse_capture_banners(&mut main.state, &engine.read());
            if changed {
                main.mark_dirty();
            }
        }
        for mut engine in self.engines_mut().parked() {
            // 창이 없는 상태에서는 상태바용 브랜치 조회와 redraw가 필요 없다.
            crate::app::services::AppServices::update_busy_surfaces(&mut engine);
            engine.forward_busy_activity(&hub);
            engine.forward_attention(&hub);
            engine.forward_surface_cwd(&hub);
            engine.forward_surface_foreground(&hub);
        }
        let engines = self.engines();
        let engines: Vec<_> = engines.windowed_and_parked().collect();
        crate::remote::agent_children::tick(&hub, &engines);
    }
}

/// foreground 세대가 바뀌면 마우스 캡처 배너만 닫는다. 같은 surface의 다른 배너는 유지한다.
/// 그 배너에서 연 더보기 메뉴도 함께 닫는다. 메뉴는 클릭할 때 foreground 이름을 읽으므로,
/// 남겨 두면 배너를 띄운 프로그램이 아닌 다음 프로그램(또는 이름 없음)에 설정을 저장한다.
fn close_stale_mouse_capture_banners(
    state: &mut crate::state::MainViewState,
    core_state: &crate::runtime::engine_read::EngineRead<'_>,
) {
    use crate::adapters::ui::BannerScope;
    use crate::adapters::ui::banner::defs::BANNER_MOUSE_CAPTURE;

    let stale: Vec<BannerScope> = state
        .banners
        .shown_banners()
        .filter(|b| b.id == BANNER_MOUSE_CAPTURE)
        .filter_map(|b| {
            let BannerScope::Surface(sid) = b.scope else {
                return None;
            };
            let origin = b.origin_generation?;
            (origin != core_state.foreground_generation(sid)).then(|| b.scope.clone())
        })
        .collect();
    for scope in stale {
        state
            .banners
            .close_shown_if_id(&scope, BANNER_MOUSE_CAPTURE);
        close_mouse_capture_menu_for(state, &scope);
    }
}

/// `scope` 배너에 딸린 더보기 메뉴가 열려 있으면 닫고 대상을 비운다. 다른 surface의 메뉴는 그대로 둔다.
fn close_mouse_capture_menu_for(
    state: &mut crate::state::MainViewState,
    scope: &crate::adapters::ui::BannerScope,
) {
    use crate::adapters::ui::BannerScope;
    use crate::adapters::ui::mouse_capture_menu::MOUSE_CAPTURE_BANNER_MENU_POPUP_ID;

    let BannerScope::Surface(sid) = scope else {
        return;
    };
    if state.dialogs.mouse_capture_banner_menu_target != Some(*sid) {
        return;
    }
    // intent-exempt: 닫힌 배너에 딸린 메뉴의 수명 정리. 포커스·선택은 바꾸지 않는다.
    state.popups.close(MOUSE_CAPTURE_BANNER_MENU_POPUP_ID);
    state.dialogs.mouse_capture_banner_menu_target = None;
}

#[cfg(test)]
mod tests {
    use super::close_stale_mouse_capture_banners;
    use crate::adapters::ui::banner::defs::BANNER_MOUSE_CAPTURE;
    use crate::adapters::ui::mouse_capture_menu::MOUSE_CAPTURE_BANNER_MENU_POPUP_ID;
    use crate::adapters::ui::{BannerScope, BannerState};

    /// surface `sid` 에 foreground 세대 `generation` 의 캡처 배너를 띄우고 그 배너의 메뉴를 연다.
    fn open_menu(state: &mut crate::state::MainViewState, sid: u32, generation: u64) {
        state.banners.push(
            BannerState::persistent(BANNER_MOUSE_CAPTURE, BannerScope::Surface(sid))
                .with_origin_generation(generation),
        );
        state.dialogs.mouse_capture_banner_menu_target = Some(sid);
        state.popups.open(MOUSE_CAPTURE_BANNER_MENU_POPUP_ID);
    }

    fn menu_open(state: &crate::state::MainViewState) -> bool {
        state.popups.is_open(MOUSE_CAPTURE_BANNER_MENU_POPUP_ID)
    }

    #[test]
    fn a_foreground_name_change_closes_the_open_menu_of_that_banner() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        e.live.foreground_names.insert(42, "vim".into());
        e.live.foreground_generation.insert(42, 1);
        open_menu(&mut state, 42, 1);
        assert!(menu_open(&state));

        // 같은 세대면 메뉴는 남는다.
        close_stale_mouse_capture_banners(&mut state, &e.read());
        assert!(menu_open(&state));

        e.live.foreground_names.insert(42, "htop".into());
        e.live.foreground_generation.insert(42, 2);
        close_stale_mouse_capture_banners(&mut state, &e.read());
        assert!(!menu_open(&state));
        assert_eq!(state.dialogs.mouse_capture_banner_menu_target, None);
    }

    /// 이름을 더 관측하지 못하면 세대 항목이 사라져 0으로 읽힌다.
    #[test]
    fn a_lost_foreground_name_closes_the_open_menu_of_that_banner() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        e.live.foreground_names.insert(42, "vim".into());
        e.live.foreground_generation.insert(42, 1);
        open_menu(&mut state, 42, 1);

        e.live.foreground_names.remove(&42);
        e.live.foreground_generation.remove(&42);
        close_stale_mouse_capture_banners(&mut state, &e.read());
        assert!(!menu_open(&state));
        assert_eq!(state.dialogs.mouse_capture_banner_menu_target, None);
    }

    #[test]
    fn a_stale_banner_on_another_surface_leaves_the_open_menu_alone() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let e = session.borrow_mut();
        e.live.foreground_generation.insert(42, 1);
        e.live.foreground_generation.insert(43, 1);
        state.banners.push(
            BannerState::persistent(BANNER_MOUSE_CAPTURE, BannerScope::Surface(43))
                .with_origin_generation(1),
        );
        open_menu(&mut state, 42, 1);

        e.live.foreground_generation.insert(43, 2);
        close_stale_mouse_capture_banners(&mut state, &e.read());
        assert!(menu_open(&state));
        assert_eq!(state.dialogs.mouse_capture_banner_menu_target, Some(42));
        assert!(
            state
                .banners
                .shown_banners()
                .any(|b| b.scope == BannerScope::Surface(42))
        );
        assert!(
            !state
                .banners
                .shown_banners()
                .any(|b| b.scope == BannerScope::Surface(43))
        );
    }
}
