//! 창의 입력 순서를 GPU 없이 밟는 시험 도우미. 누름은 debug 주입과 같은 함수(`input_segment::press_focus`)로 지금 몫을
//! 끊고 포커스 노드를 옮긴다. 키·글자는 창처럼 같은 끊기(`input_segment::key_focus`)와 egui 게이트
//! (`window_feeds_egui`)를 거쳐 큐에 들어간다(`key`). 패스 입력은 egui 패스와 같은 순서로 꺼낸다
//! (`PopupManager::begin_key_pass` 뒤 큐를 꺼내 앞 끊은 자리에서 나눔, `InputCuts::split_pass`). 몫을 닫는 패스는 제품
//! 패스와 같은 순서 함수(`gfx::gpu::input_pass::run_input_pass`)로 돈다(`run_pass`). Esc 처럼 창이 따로 고르는
//! 키(`window_escape`)는 창과 같은 함수로 거른다(`escape`).

use crate::adapters::ui::popup::frame_keys::InputCuts;
use crate::gfx::gpu::input_pass::{InputPassHost, run_input_pass};
use crate::gfx::gpu::text_selection::HeldPastes;
use crate::state::RequestContext;
use crate::view::main::input_segment::{EguiPressSink, key_focus, press_focus};

/// 창 하나의 egui 입력 큐와 끊은 자리.
#[derive(Default)]
pub(crate) struct WindowInput {
    queue: Vec<egui::Event>,
    cuts: InputCuts,
    /// 직전 패스가 끊은 자리에서 나눴는지. 창은 나눈 패스 뒤에 다음 패스를 요청한다.
    split: bool,
}

/// 하네스 패스가 창의 두 자리에서 부르는 일(`run_pass`).
pub(crate) enum PassHook<'a> {
    /// 끊은 자리에서 나눈 직후·몫 필터 전의 패스 입력. 창은 여기서 편집 보류에 패스 입력을 알리고 도착한 붙여넣기를
    /// 합친다(`GpuState::take_pass_input`).
    Taken(&'a mut HeldPastes, &'a mut Vec<egui::Event>),
    /// 몫 필터가 입력을 뺐다. 창은 편집 보류를 대상이 사라진 것으로 끝낸다.
    Dropped,
}

/// 하네스 패스의 주체.
struct HarnessPass<'a> {
    input: &'a mut WindowInput,
    ctx: &'a egui::Context,
    screen: egui::Rect,
    held: &'a mut HeldPastes,
    hook: &'a mut dyn FnMut(PassHook<'_>),
}

impl InputPassHost for HarnessPass<'_> {
    fn escape_held(&self) -> bool {
        self.ctx.input(|i| i.key_down(egui::Key::Escape))
    }

    fn held_pastes(&mut self) -> &mut HeldPastes {
        self.held
    }

    fn take_input(&mut self) -> egui::RawInput {
        let mut events = self.input.split_off_pass();
        (self.hook)(PassHook::Taken(self.held, &mut events));
        egui::RawInput {
            screen_rect: Some(self.screen),
            events,
            ..Default::default()
        }
    }

    fn egui_ctx(&self) -> &egui::Context {
        self.ctx
    }

    fn input_dropped(&mut self) {
        (self.hook)(PassHook::Dropped);
    }

    fn focus_tag(&self) -> u64 {
        self.ctx.memory(|m| m.focused()).map_or(0, |id| id.value())
    }

    fn run(
        &mut self,
        raw_input: egui::RawInput,
        state: &mut RequestContext,
        mut ui: impl FnMut(&egui::Context, &mut RequestContext),
    ) -> egui::FullOutput {
        self.ctx.run(raw_input, |ctx| ui(ctx, state))
    }

    fn cuts_and_raw_left(&mut self) -> (&mut InputCuts, usize) {
        (&mut self.input.cuts, self.input.queue.len())
    }
}

impl EguiPressSink for WindowInput {
    fn queued_events(&mut self) -> &mut Vec<egui::Event> {
        &mut self.queue
    }

    fn cut_queue_here(&mut self) {
        self.cuts.cut_at(self.queue.len());
    }
}

/// debug 키 주입(`debug_input::inject_key`)이 넣는 큐. 수식 키는 유지하지 않고 클립보드는 비었다.
#[cfg(debug_assertions)]
impl crate::view::main::debug_input::EguiKeySink for WindowInput {
    fn held_modifiers(&self) -> egui::Modifiers {
        egui::Modifiers::NONE
    }

    fn hold_modifiers(&mut self, _modifiers: egui::Modifiers) {}

    fn clipboard_text(&mut self) -> Option<String> {
        None
    }

    fn push_events(&mut self, events: Vec<egui::Event>) {
        self.queue.extend(events);
    }
}

impl WindowInput {
    /// 창이 받은 키·글자 사건 하나. 창처럼 입력 몫 끊기(`key_focus`) → egui 게이트(`window_feeds_egui`) → egui 큐
    /// 순서로 밟는다. `egui_surface` 는 포커스 surface 가 egui 로 입력을 받는지다. 게이트 결과를 돌려준다(`None` 이면
    /// egui 로 가지 않았다).
    pub(crate) fn key(
        &mut self,
        state: &mut RequestContext,
        core: &crate::core::CoreState,
        egui_surface: bool,
        event: egui::Event,
    ) -> Option<bool> {
        self.key_events(state, core, egui_surface, vec![event])
    }

    /// 창이 받은 Esc 누름·뗌 하나(`feed_egui_keyboard` → `keyboard::handle_keyboard_input`). 끊기 → egui 게이트를
    /// 거쳐, egui 로 가면 큐에 싣고 가지 않는 뗌은 그 몫에 남긴다(`MainView::note_unfed_key`). 누름은 창의 Esc 처리
    /// (`window_escape`, 직전 패스의 Escape 눌림)를 거친다. 창이 처리했으면 참이다.
    pub(crate) fn escape(
        &mut self,
        ctx: &egui::Context,
        state: &mut RequestContext,
        core: &crate::core::CoreState,
        pressed: bool,
    ) -> bool {
        key_focus(state, core, self);
        let event = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        if state.window_feeds_egui(false, true).is_some() {
            self.queue.push(event);
        } else if !pressed {
            state.popups.note_unfed_escape_release();
        }
        pressed
            && state.window_escape(
                &mut self.queue,
                ctx.input(|i| i.key_down(egui::Key::Escape)),
            )
    }

    /// 창이 처리할 Esc 를 고를 큐(`MainViewState::window_escape` 의 `pending`).
    pub(crate) fn queue_mut(&mut self) -> &mut Vec<egui::Event> {
        &mut self.queue
    }

    /// 주입한 왼쪽 버튼 누름. 포커스 처리 뒤 포인터 이동과 누름 사건을 싣는다(`push_egui_pointer_at` 순서).
    pub(crate) fn press(
        &mut self,
        state: &mut RequestContext,
        core: &crate::core::CoreState,
        pos: egui::Pos2,
    ) {
        press_focus(state, core, self, pos, egui::PointerButton::Primary);
        self.queue.push(egui::Event::PointerMoved(pos));
        self.queue.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
    }

    /// 다음 egui 패스의 입력. 몫을 닫지 않는 시험이 쓴다.
    pub(crate) fn take_pass(
        &mut self,
        state: &mut RequestContext,
        ctx: &egui::Context,
    ) -> Vec<egui::Event> {
        state
            .popups
            .begin_key_pass(ctx.input(|i| i.key_down(egui::Key::Escape)));
        let mut events = self.split_off_pass();
        state.drop_input_without_its_node(ctx, &mut events);
        events
    }

    /// 큐를 앞 끊은 자리에서 나눠 이번 패스 몫을 꺼낸다(`InputCuts::split_pass`).
    fn split_off_pass(&mut self) -> Vec<egui::Event> {
        let mut events = std::mem::take(&mut self.queue);
        self.split = self.cuts.split_pass(&mut events, &mut self.queue);
        events
    }

    /// 제품 패스와 같은 순서 함수(`run_input_pass`)로 egui 패스 하나를 돌리고 몫을 닫는다. `hook` 은 창이 편집 보류와
    /// 붙여넣기를 다루는 두 자리다(`PassHook`). 이 패스에 실제로 든 사건과 패스 출력을 돌려준다.
    pub(crate) fn run_pass(
        &mut self,
        state: &mut RequestContext,
        ctx: &egui::Context,
        screen: egui::Rect,
        held: &mut HeldPastes,
        hook: &mut dyn FnMut(PassHook<'_>),
        ui: impl FnMut(&egui::Context, &mut RequestContext),
    ) -> (Vec<egui::Event>, egui::FullOutput) {
        let mut drawn = Vec::new();
        let output = run_input_pass(
            &mut HarnessPass {
                input: self,
                ctx,
                screen,
                held,
                hook,
            },
            state,
            |raw| drawn.clone_from(&raw.events),
            ui,
        );
        (drawn, output)
    }

    /// 창이 받은 키 사건 하나와 그 사건이 egui 에 남기는 원시 사건들(수식 키 뗌은 0개). 창처럼 끊기(`key_focus`) →
    /// egui 게이트(`window_feeds_egui`, 키 사건이라 키 주인을 묶는다) → egui 큐 순서로 밟는다
    /// (`MainView::feed_egui_keyboard`). 게이트 결과를 돌려준다.
    pub(crate) fn key_events(
        &mut self,
        state: &mut RequestContext,
        core: &crate::core::CoreState,
        egui_surface: bool,
        events: Vec<egui::Event>,
    ) -> Option<bool> {
        key_focus(state, core, self);
        let fed = state.window_feeds_egui(egui_surface, true);
        if fed.is_some() {
            self.queue.extend(events);
        }
        fed
    }

    /// egui 게이트 앞에서 멈춘 창 키 하나(편집 판정이 붙잡은 키, `MainView::edit_key_gate`). 창처럼 키 분류와
    /// 끊기(`key_focus`)만 거친다.
    pub(crate) fn key_without_event(
        &mut self,
        state: &mut RequestContext,
        core: &crate::core::CoreState,
    ) {
        key_focus(state, core, self);
    }

    /// 남은 끊은 자리의 수(`GpuState` 가 편집 판정에 알리는 값, `EditKeyState::set_cuts`).
    pub(crate) fn cut_count(&self) -> usize {
        self.cuts.len()
    }

    /// 남은 끊은 자리(egui 입력 큐 좌표)의 글.
    pub(crate) fn cuts_text(&self) -> String {
        format!("{:?}", self.cuts)
    }

    /// 창이 더 그릴 패스가 없는지. 꺼내지 않은 사건이나 끊은 자리가 남았거나 직전 패스가 나눴으면 창은 빈 몫이어도
    /// 다음 패스를 그린다(나눈 패스가 다음 패스를 요청한다, `GpuState::take_pass_input`).
    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.cuts.is_empty() && !self.split
    }
}
