//! 창 egui 패스 하나의 입력 순서. 제품 패스(`GpuState::run_main_pass`)와 창 입력 시험 하네스가 이 함수 하나로
//! 패스를 연다. 그래서 몫 필터·편집 폐기 알림·Find 붙여넣기 수령·패스 닫기·그리기 중 전환 봉인의 순서는 제품과 시험이
//! 같다. 패스 입력 꺼내기(끊은 자리에서 나누고 편집 보류에 알린 뒤 도착한 붙여넣기를 합치기)와 egui 실행은 주체가 한다.
//! 몫을 여닫는 창 상태는 `InputPassState`다.

use crate::adapters::ui::fullscreen::StageId;
use crate::adapters::ui::popup::frame_keys::InputCuts;
use crate::gfx::gpu::text_selection::HeldPastes;
use crate::state::MainViewState;

/// 패스 하나의 입력 몫을 여닫는 창 상태. 메인 창은 `MainViewState`다.
pub(crate) trait InputPassState {
    /// 이번 패스 몫을 연다(`PopupManager::begin_key_pass`).
    fn begin_key_pass(&mut self, escape_held: bool);
    /// 직전 일반 패스가 그린 Find 입력칸 id.
    fn find_fields(&self) -> &[u64];
    /// 그 몫의 노드가 지금 그리는 화면에 없으면 입력을 뺀다. 뺐으면 참이다.
    fn drop_input_without_its_node(
        &mut self,
        ctx: &egui::Context,
        events: &mut Vec<egui::Event>,
    ) -> bool;
    /// Find 입력칸이 받을 붙여넣기를 이번 패스에 넣을 수 있는지.
    fn find_paste_open(&self) -> bool;
    /// 이번 패스가 그리는 화면(전체화면 무대).
    fn fullscreen_stage_id(&self) -> Option<StageId>;
    /// 몫을 닫고 다음 몫을 남긴다.
    fn close_input_pass(&mut self, ctx: &egui::Context);
    /// 그리기 중 전환을 처리한다.
    fn seal_after_drawing_transition(
        &mut self,
        drawn: Option<StageId>,
        cuts: &mut InputCuts,
        raw_left: usize,
    );
}

impl InputPassState for MainViewState {
    fn begin_key_pass(&mut self, escape_held: bool) {
        self.popups.begin_key_pass(escape_held);
    }

    fn find_fields(&self) -> &[u64] {
        &self.find_fields
    }

    fn drop_input_without_its_node(
        &mut self,
        ctx: &egui::Context,
        events: &mut Vec<egui::Event>,
    ) -> bool {
        MainViewState::drop_input_without_its_node(self, ctx, events)
    }

    fn find_paste_open(&self) -> bool {
        MainViewState::find_paste_open(self)
    }

    fn fullscreen_stage_id(&self) -> Option<StageId> {
        MainViewState::fullscreen_stage_id(self)
    }

    fn close_input_pass(&mut self, ctx: &egui::Context) {
        MainViewState::close_input_pass(self, ctx);
    }

    fn seal_after_drawing_transition(
        &mut self,
        drawn: Option<StageId>,
        cuts: &mut InputCuts,
        raw_left: usize,
    ) {
        MainViewState::seal_after_drawing_transition(self, drawn, cuts, raw_left);
    }
}

/// 패스 하나를 여는 주체. 제품은 GPU 상태, 시험은 창 입력 하네스다. `S`는 몫을 여닫는 창 상태다.
pub(crate) trait InputPassHost<S> {
    /// 이 창 egui 의 Escape 눌림 기록(직전 패스가 남긴 값).
    fn escape_held(&self) -> bool;
    /// 이 창의 붙여넣기 보류.
    fn held_pastes(&mut self) -> &mut HeldPastes;
    /// 이번 패스 입력. 끊은 자리에서 나눈 직후·몫 필터 전에 편집 보류에 패스 입력을 알리고 도착한 붙여넣기를 합친다.
    fn take_input(&mut self) -> egui::RawInput;
    fn egui_ctx(&self) -> &egui::Context;
    /// 몫 필터가 이번 패스 입력을 뺐다. 편집 보류는 대상이 사라진 것으로 끝난다.
    fn input_dropped(&mut self);
    /// 붙여넣기 태그로 쓰는 egui 포커스 위젯 값.
    fn focus_tag(&self) -> u64;
    /// egui 패스를 돌린다.
    fn run(
        &mut self,
        raw_input: egui::RawInput,
        state: &mut S,
        ui: impl FnMut(&egui::Context, &mut S),
    ) -> egui::FullOutput;
    /// 끊은 자리와 아직 꺼내지 않은 원시 사건 수.
    fn cuts_and_raw_left(&mut self) -> (&mut InputCuts, usize);
}

/// 패스 하나는 입력 몫 하나다. 몫을 열고(`InputPassState::begin_key_pass`) 직전 일반 패스가 그린 Find 입력칸을 알린 뒤
/// 입력을 꺼낸다. 몫 필터(`MainViewState::drop_input_without_its_node`)가 입력을 빼면 편집 보류에 알리고, 그 뒤 Find
/// 입력칸이 받을 붙여넣기를 넣는다(`HeldPastes::receive_find`). `adjust` 뒤 패스를 돌리고 몫을 닫은 다음
/// (`MainViewState::close_input_pass`) 그리기 중 전환을 봉인한다(`MainViewState::seal_after_drawing_transition`).
pub(crate) fn run_input_pass<S: InputPassState>(
    host: &mut impl InputPassHost<S>,
    state: &mut S,
    adjust: impl FnOnce(&mut egui::RawInput),
    ui: impl FnMut(&egui::Context, &mut S),
) -> egui::FullOutput {
    state.begin_key_pass(host.escape_held());
    host.held_pastes().set_find_fields(state.find_fields());
    let mut raw_input = host.take_input();
    let dropped = state.drop_input_without_its_node(host.egui_ctx(), &mut raw_input.events);
    if dropped {
        host.input_dropped();
    }
    let focused = host.focus_tag();
    let found = host
        .held_pastes()
        .receive_find(focused, state.find_paste_open());
    raw_input
        .events
        .extend(found.into_iter().map(egui::Event::Paste));
    adjust(&mut raw_input);
    let drawn = state.fullscreen_stage_id();
    let output = host.run(raw_input, state, ui);
    state.close_input_pass(host.egui_ctx());
    let (cuts, raw_left) = host.cuts_and_raw_left();
    state.seal_after_drawing_transition(drawn, cuts, raw_left);
    output
}
