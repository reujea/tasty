//! 창별 편집 키 판정의 App 쪽 연결. 보조 창과 부팅 오류 화면에 적용된 편집 바인딩을 넣고, 메인 창에서
//! 보류가 풀린 편집 키를 그 창의 평소 이벤트 경로로 재생한다. 원래 키는 이미 판정한 편집으로 한 번만
//! 넘기고, 보류 중에 온 이벤트는 받은 순서대로 다시 처리한다.
//! 계약: `docs/design/policies/key-mapping.md`의 "편집 키 판정" 절.

use winit::event::WindowEvent;
use winit::window::WindowId;

use crate::adapters::ui::input::edit_key::{EditBindings, EditKey, Release};
use crate::app::App;

/// 보류가 입력을 붙잡은 단계. 재생할 입력이 그 앞 단계를 이미 거쳤는지 정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoldSite {
    /// 창 view 안(호스트 입력칸 보류). 원래 키와 줄 선 입력 모두 플러그인 단축키 단계를 거친 뒤 붙잡혔다.
    View,
    /// 플러그인 단축키 단계 앞(플러그인 입력칸 보류의 줄). 원래 키는 view 에서 붙잡혔고, 줄 선 입력은 단축키
    /// 단계를 아직 거치지 않았다.
    BeforePluginShortcuts,
}

/// 재생 한 단계.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplayStep<E> {
    /// 판정한 편집으로 창 view 에 한 번 넘긴다.
    Original(E, EditKey),
    /// 창 view 로 넘긴다. 새 플러그인 보류가 있으면 그 뒤에 줄을 선다.
    View(E),
    /// 플러그인 보류 줄부터 창 입력의 마지막 단계를 다시 거친다([`App::route_window_input`]).
    Input(E),
}

/// 풀린 보류의 재생 순서. 원래 키가 먼저이고, 줄 선 입력은 받은 순서대로 붙잡힌 단계부터 다시 거친다.
pub(crate) fn replay_steps<E>(release: Release<E>, site: HoldSite) -> Vec<ReplayStep<E>> {
    let original = release
        .replay
        .map(|(event, edit)| ReplayStep::Original(event, edit));
    let queued = release.queued.into_iter().map(|event| match site {
        HoldSite::View => ReplayStep::View(event),
        HoldSite::BeforePluginShortcuts => ReplayStep::Input(event),
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
        self.replay_edit_steps(id, replay_steps(release, HoldSite::View));
    }

    /// 풀린 보류를 그 창의 평소 이벤트 경로로 재생한다. 원래 키는 판정한 편집으로 한 번만 넘긴다. 재생 중에
    /// 플러그인 편집 보류가 새로 생기면 남은 입력은 그 보류 뒤에 줄을 선다.
    pub(crate) fn replay_edit_steps(&mut self, id: WindowId, steps: Vec<ReplayStep<WindowEvent>>) {
        for step in steps {
            match step {
                ReplayStep::Original(event, edit) => {
                    self.set_edit_replay(id, Some(edit));
                    self.dispatch_key_to_view(id, event);
                    // 창이 키를 받지 않고 돌아갔으면 남은 표시가 다음 키에 붙지 않게 지운다.
                    self.set_edit_replay(id, None);
                }
                ReplayStep::View(event) => {
                    if !self.queue_behind_plugin_edit(id, &event) {
                        self.dispatch_key_to_view(id, event);
                    }
                }
                ReplayStep::Input(event) => self.route_window_input(id, event),
            }
        }
    }

    /// 창 입력의 마지막 단계: 플러그인 편집 보류 줄, 포커스된 plugin 의 단축키, 창 view 순서다. 보류가 없을 때의
    /// 창 이벤트와 플러그인 보류 뒤에 줄 섰던 입력이 같은 단계를 거친다.
    pub(super) fn route_window_input(&mut self, id: WindowId, event: WindowEvent) {
        if self.queue_behind_plugin_edit(id, &event) {
            return;
        }
        // 포커스된 plugin surface의 단축키를 호스트 action보다 먼저 처리한다.
        // 단축키가 연 plugin 팝업도 창 view 로 간 키처럼 그 자리에서 적용한다.
        if let WindowEvent::KeyboardInput { event: ke, .. } = &event
            && self.try_plugin_shortcut(id, ke)
        {
            self.apply_user_focus_opens(id);
            return;
        }
        self.dispatch_key_to_view(id, event);
    }

    /// 사건을 창 view 로 넘기고, 키·IME 사건이면 그 사건이 포커스를 주며 연 팝업을 그 자리에서 적용한다. 창 입력과
    /// 편집 보류 재생(원래 키·view 로 가는 줄 선 입력)이 이 길을 함께 쓴다.
    fn dispatch_key_to_view(&mut self, id: WindowId, event: WindowEvent) {
        let key = matches!(
            event,
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_)
        );
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
            .is_some_and(|main| main.state.user_focus_open_queued());
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
