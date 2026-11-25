//! 창별 편집 키 판정의 App 쪽 연결. 보조 창과 부팅 오류 화면에 적용된 편집 바인딩을 넣고, 메인 창에서
//! 보류가 풀린 편집 키를 그 창의 평소 이벤트 경로로 재생한다. 원래 키는 이미 판정한 편집으로 한 번만
//! 넘기고, 보류 중에 온 이벤트는 받은 순서대로 다시 처리한다.
//! 계약: `docs/design/policies/key-mapping.md`의 "편집 키 판정" 절.

use std::time::Instant;

use winit::event::WindowEvent;
use winit::window::WindowId;

use crate::adapters::ui::input::edit_key::{EditBindings, EditKey, LineItem, Release};
use crate::app::App;
use crate::app::window_access::engines_mut;

/// 재생 한 단계.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplayStep<E> {
    /// 판정한 편집으로 창 view 에 한 번 넘긴다. 원래 키는 view 안에서 붙잡혔다.
    Original(E, EditKey),
    /// 창 입력 맨 앞의 보류 검사부터 다시 거친다([`App::route_window_input_at`]). 줄 선 입력은 모두 그 검사에서
    /// 붙잡혔다(플러그인 편집 보류 줄, 창의 편집·정착 보류 줄). 시각은 처음 도착한 때다.
    Input(E, Instant),
    /// 줄 선 debug 주입. 그 창의 주입 입구 맨 앞의 보류 검사부터 다시 거친다([`App::enter_debug_input`]).
    #[cfg(debug_assertions)]
    Debug(crate::view::main::debug_input::DebugInput),
}

/// 풀린 보류의 재생 순서. 원래 키가 먼저이고, 줄 선 입력은 받은 순서대로 창 입력 처음부터 다시 거친다. 재생 중 새
/// 보류를 만나면 그 사건과 남은 재생 사건이 그 줄 앞에 그대로 선다(재생은 한 호출 안에서 끝나 그 사이 새 입력이
/// 끼지 않는다).
pub(crate) fn replay_steps<E>(release: Release<E>) -> Vec<ReplayStep<E>> {
    let (replay, queued) = release.split();
    let original = replay.map(|(event, edit)| ReplayStep::Original(event, edit));
    let queued = queued.into_iter().map(|item| match item {
        LineItem::Event(event, at) => ReplayStep::Input(event, at),
        #[cfg(debug_assertions)]
        LineItem::Debug(input) => ReplayStep::Debug(input),
    });
    original.into_iter().chain(queued).collect()
}

impl App {
    /// 최신 적용 설정의 편집 바인딩. 설정 창의 편집 초안이 아니라 적용한 값이다.
    pub(crate) fn applied_edit_bindings(&self) -> Option<EditBindings> {
        self.services
            .latest_settings()
            .map(|s| EditBindings::from_settings(&s.keybindings))
    }

    /// 메인 창이 아닌 창(설정·Preset·Plugins·종료 확인)에 적용된 편집 바인딩을 넣는다. 열린 동안 바인딩을
    /// 바꿔도 다음 키부터 따라간다. 메인 창은 자기 engine 설정으로 판정한다.
    pub(crate) fn sync_auxiliary_edit_bindings(&mut self, id: WindowId) {
        let bindings = self.applied_edit_bindings();
        if let Some(view) = self.view.views.get_mut(&id)
            && view.as_main().is_none()
        {
            view.base_mut().gpu.set_edit_bindings(bindings);
        }
    }

    pub(crate) fn replay_main_edit_keys(&mut self, id: WindowId) {
        let Some(release) = self
            .view
            .views
            .get_mut(&id)
            .and_then(|view| view.as_main_mut())
            .and_then(|main| main.base.gpu.take_edit_release())
        else {
            return;
        };
        self.replay_edit_steps(id, replay_steps(release));
    }

    /// 풀린 보류를 그 창의 평소 이벤트 경로로 재생한다. 원래 키는 판정한 편집으로 한 번만 넘긴다. 재생 중에
    /// 플러그인 편집 보류가 새로 생기면 남은 입력은 그 보류 뒤에 줄을 선다.
    pub(crate) fn replay_edit_steps(&mut self, id: WindowId, steps: Vec<ReplayStep<WindowEvent>>) {
        for step in steps {
            match step {
                // 원래 키는 편집 키 누름이라 더블탭·힌트 홀드 판정이 그 시각을 읽지 않는다.
                ReplayStep::Original(event, edit) => {
                    self.set_edit_replay(id, Some(edit));
                    self.dispatch_key_to_view(id, event, false);
                    // 창이 키를 받지 않고 돌아갔으면 남은 표시가 다음 키에 붙지 않게 지운다.
                    self.set_edit_replay(id, None);
                }
                ReplayStep::Input(event, at) => self.route_window_input_at(id, event, at),
                #[cfg(debug_assertions)]
                ReplayStep::Debug(input) => {
                    // 재생에는 IPC 응답이 없다. 줄에 다시 섰는지·넣은 것은 그 주입 입구가 처리했다.
                    let _ = self.enter_debug_input(id, input);
                }
            }
        }
    }

    /// 새로 도착한 창 입력([`Self::route_window_input_at`]의 도착 시각은 지금).
    pub(super) fn route_window_input(&mut self, id: WindowId, event: WindowEvent) {
        self.route_window_input_at(id, event, Instant::now());
    }

    /// `at` 에 처음 도착한 창 입력. 그 사건을 처리하는 동안 창은 그 시각으로 더블탭·힌트 홀드를 판정하고
    /// (`GpuState::replace_event_arrival`), 끝나면 앞 값으로 되돌린다.
    pub(super) fn route_window_input_at(&mut self, id: WindowId, event: WindowEvent, at: Instant) {
        let before = self.replace_event_arrival(id, Some(at));
        self.route_input_steps(id, event, at);
        self.replace_event_arrival(id, before);
    }

    /// 창 입력의 마지막 단계: 보류 검사, 키 사건의 출발 노드, 포커스된 plugin 의 단축키, 창 view 순서다. 보류 검사는
    /// 어느 단계도 사건을 읽기 전 한 자리다. 플러그인 편집 보류 줄과 창의 보류 줄(편집 보류, 정착되지 않은 앞 입력 뒤의
    /// 키) 중 열린 쪽이 사건을 받는다. 둘은 동시에 열리지 않는다(한쪽이 열린 동안의 사건은 그 줄에 서므로 다른 쪽을
    /// 만들지 못한다). 보류가 없을 때의 창 이벤트와 줄 섰던 입력이 같은 단계를 거친다.
    fn route_input_steps(&mut self, id: WindowId, event: WindowEvent, at: Instant) {
        if self.queue_behind_plugin_edit(id, &event, at) || self.hold_window_input(id, &event) {
            return;
        }
        // 키 사건의 출발 노드(정규화 → 끊기 → 수신자)를 plugin 단축키 단계 앞에서 한 번 정한다. plugin 단계와 창 view
        // 가 같은 사건 시점의 노드를 읽는다.
        let settled = self.settle_window_key(id, &event);
        // 포커스된 plugin surface의 단축키를 호스트 action보다 먼저 처리한다.
        // 단축키가 연 plugin 팝업도 창 view 로 간 키처럼 그 자리에서 적용한다.
        if let WindowEvent::KeyboardInput { event: ke, .. } = &event
            && self.try_plugin_shortcut(id, ke)
        {
            self.apply_user_focus_opens(id);
            return;
        }
        self.dispatch_key_to_view(id, event, settled);
    }

    /// 메인 창 보류 줄(`GpuState::hold_window_input`). 별도 창은 자기 입구에서 같은 검사를 한다.
    fn hold_window_input(&mut self, id: WindowId, event: &WindowEvent) -> bool {
        let Some(main) = self.view.views.get_mut(&id).and_then(|v| v.as_main_mut()) else {
            return false;
        };
        let handed = main.state.popups.has_undrawn_handed_escape();
        main.base
            .gpu
            .hold_window_input(&main.base.winit, event, handed)
    }

    /// 창이 처리할 사건의 도착 시각을 바꾸고 앞 값을 돌려준다.
    fn replace_event_arrival(&mut self, id: WindowId, at: Option<Instant>) -> Option<Instant> {
        self.view
            .views
            .get_mut(&id)
            .and_then(|view| view.base_mut().gpu.replace_event_arrival(at))
    }

    /// winit 키 사건의 출발 노드를 메인 창에서 정한다(`MainView::settle_key_event`). 정했으면 참이고, 창 view 는 같은
    /// 사건에서 다시 정하지 않는다. 키가 아닌 사건은 창 view 가 정하고, 보조 창은 자기 입력 경로를 쓴다. modal 이
    /// 열렸으면 창 view 가 정하기 전 단계(modal 차단)에서 끝내므로 정하지 않는다.
    fn settle_window_key(&mut self, id: WindowId, event: &WindowEvent) -> bool {
        if !matches!(event, WindowEvent::KeyboardInput { .. }) || self.view.is_modal_active() {
            return false;
        }
        let Some((main, engine)) = engines_mut!(self).window_pair(id) else {
            return false;
        };
        main.settle_key_event(&engine.read(), event);
        true
    }

    /// 사건을 창 view 로 넘기고, 키·IME 사건이면 그 사건이 포커스를 주며 연 팝업을 그 자리에서 적용한다. 창 입력과
    /// 편집 보류 재생(원래 키·view 로 가는 줄 선 입력)이 이 길을 함께 쓴다. `key_settled` 는 App 이 그 사건의 출발
    /// 노드를 이미 정했는지다(`settle_window_key`). 넘기기 직전에 메인 창에 적어 그 사건이 읽게 한다.
    fn dispatch_key_to_view(&mut self, id: WindowId, event: WindowEvent, key_settled: bool) {
        let key = matches!(
            event,
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_)
        );
        if let Some(main) = self.view.views.get_mut(&id).and_then(|w| w.as_main_mut()) {
            main.key_settled = key_settled;
        }
        self.dispatch_window_event_to_view(id, event);
        if key {
            self.apply_user_focus_opens(id);
        }
    }

    /// 키 사건이 포커스를 주며 여는 팝업을 요청했으면 대기열을 그 자리에서 처리한다. 포커스 노드 전이는 그 키 사건
    /// 처리 안에서 적용돼야 같은 묶음의 다음 키가 사건 시점의 노드(그 팝업)로 끊긴다(`MainViewState::cut_key_frame`).
    /// 판정(`user_focus_open_queued`)은 host intent 대기열과 plugin 팝업 열기 대기열을 함께 본다. 참이면 두 대기열을
    /// 평소 순서(intent 다음 plugin 팝업 열기)대로 모두 처리하고, 처리 시점만 앞당긴다.
    pub(crate) fn apply_user_focus_opens(&mut self, id: WindowId) {
        let queued = self
            .view
            .views
            .get(&id)
            .and_then(|view| view.as_main())
            .is_some_and(|main| {
                main.state
                    .user_focus_open_queued(self.plugin_manager.as_ref())
            });
        if queued {
            self.dispatch_pending_intents();
            self.dispatch_pending_popup_opens();
        }
    }

    fn set_edit_replay(&mut self, id: WindowId, edit: Option<EditKey>) {
        if let Some(main) = self.view.views.get_mut(&id).and_then(|v| v.as_main_mut()) {
            main.edit_replay = edit;
        }
    }
}

#[cfg(test)]
#[path = "edit_key_replay_tests.rs"]
mod tests;
