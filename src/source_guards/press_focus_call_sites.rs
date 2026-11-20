//! 누름이 포커스 노드를 정하는 자리(J6)가 창의 누름 경로와 debug 포인터 주입에서 같은 함수를 거치는지 확인한다.
//! 포커스 노드를 옮기는 판정(`MainViewState::take_press_snapshot`)과 입력 끊기(`MainViewState::cut_key_frame`)의 동작은
//! 장면 시험(`adapters::ui::popup::route::tests::repro`)이 그 함수를 직접 불러 잰다. 창 호출부가 그 함수를 부르지
//! 않으면 장면 시험은 보지 못하므로 호출부의 모양을 여기서 고정한다.
//!
//! 잡지 못하는 것: 호출부가 함수를 부르되 결과를 버리는 다른 길을 더하는 것, 창 밖 별도 창의 누름 경로.

use super::{fn_body, repo_root, strip_comments};

fn read(rel: &str) -> String {
    strip_comments(
        &std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
            .replace("\r\n", "\n"),
    )
}

fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn at(body: &str, needle: &str, what: &str) -> usize {
    assert_eq!(
        body.matches(needle).count(),
        1,
        "{what}: `{needle}` 이 하나가 아니다"
    );
    body.find(needle).unwrap_or_default()
}

/// 창의 누름·키: 사건을 처리하기 전에(튜토리얼 입력·egui 전달·누름 스냅샷보다 먼저) 입력 몫을 끊고, 누름 처리가
/// 누름 스냅샷 함수로 포커스 노드를 옮겨 그 프레임에 남긴다.
#[test]
fn a_window_press_moves_the_focus_node_through_the_press_snapshot() {
    let src = read("src/view/main.rs");
    let cut = squeeze(&fn_body(&src, "fn cut_input_segment(").expect("cut_input_segment"));
    at(
        &cut,
        "WindowEvent::MouseInput{state:winit::event::ElementState::Pressed,..}=>Incoming::Press,WindowEvent::KeyboardInput{..}|WindowEvent::Ime(_)=>Incoming::Key,_=>return,",
        "끊을 사건",
    );
    at(
        &cut,
        "for_in0..self.state.cut_key_frame(engine,self.base.gpu.egui_input_events_mut(),incoming){self.base.gpu.cut_egui_input_here();}",
        "입력 끊기",
    );
    let handle = squeeze(&fn_body(&src, "fn handle_engine_event(").expect("handle_engine_event"));
    let normalize = at(
        &handle,
        "self.normalize_focus_on_key(engine,&event);",
        "키의 정규화",
    );
    let cut_at = at(
        &handle,
        "self.cut_input_segment(engine,&event);",
        "입력 끊기 호출",
    );
    let tutorial = at(
        &handle,
        "self.route_tutorial_input(&event)",
        "튜토리얼 입력",
    );
    let keys = at(&handle, "self.feed_egui_keyboard(", "키의 egui 전달");
    let other = at(
        &handle,
        "self.feed_egui_pointer_or_other(&event)",
        "키가 아닌 사건의 egui 전달",
    );
    assert!(
        normalize < cut_at && cut_at < tutorial && tutorial < keys && keys < other,
        "정규화 → 끊기 → 튜토리얼 → egui 전달 순서가 아니다"
    );
    let mouse = read("src/view/main/mouse.rs");
    let handle = squeeze(&fn_body(&mouse, "fn handle_mouse_input(").expect("handle_mouse_input"));
    at(
        &handle,
        "ifbutton_state==ElementState::Pressed&&letSome(snapshot)=self.press_snapshot_now(engine,button){self.state.keep_press_snapshot(snapshot);}",
        "창 누름의 스냅샷",
    );
    let now = squeeze(&fn_body(&mouse, "fn press_snapshot_now(").expect("press_snapshot_now"));
    at(
        &now,
        "Some(self.state.take_press_snapshot(engine,pos,button))",
        "누름 스냅샷 함수",
    );
}

/// debug 포인터 주입: 누름이면 `press_focus`(끊기 → 누름 스냅샷 → 남기기)를 거친 뒤 egui 입력에 넣는다. `press_focus` 의
/// 동작은 `view::main::debug_input` 의 시험이 그 함수를 직접 불러 잰다.
#[test]
fn a_debug_pointer_press_moves_the_focus_node_through_the_press_snapshot() {
    let src = read("src/view/main/debug_input.rs");
    let body = squeeze(
        &fn_body(&src, "fn debug_inject_egui_pointer(").expect("debug_inject_egui_pointer"),
    );
    let focus = at(
        &body,
        "press_focus(&mutself.state,engine,&mutself.base.gpu,pos,map_egui_button(*button),);",
        "누름의 포커스 처리",
    );
    let push = at(
        &body,
        "push_egui_pointer_at(&mutself.base.gpu,pos,action,modifiers);",
        "egui 입력에 넣기",
    );
    assert!(focus < push, "포커스 처리 → egui 입력 순서가 아니다");
    let segment = read("src/view/main/input_segment.rs");
    let press = squeeze(&fn_body(&segment, "fn press_focus(").expect("press_focus"));
    let cut = at(
        &press,
        "for_in0..state.cut_key_frame(core,sink.queued_events(),Incoming::Press){sink.cut_queue_here();}",
        "입력 끊기",
    );
    let take = at(
        &press,
        ".take_press_snapshot(core,pos,button);",
        "누름 스냅샷 함수",
    );
    let keep = at(
        &press,
        "state.keep_press_snapshot(snapshot);",
        "스냅샷 남기기",
    );
    assert!(
        cut < take && take < keep,
        "끊기 → 스냅샷 → 남기기 순서가 아니다"
    );
}

/// 메인 창의 egui 패스(일반 프레임과 전체 화면 무대)는 모두 `run_main_pass` 하나로 열고 닫는다. 그 함수만 끊은 자리로
/// 입력을 나누고, 같은 자리에서 입력 몫과 누름 스냅샷을 꺼낸다. 패스 하나 = 끊은 자리 하나 = 몫 하나가 깨지지 않게
/// 호출부와 나누는 자리를 고정한다. 몫을 함께 꺼내는 동작은 `press_cut_tests` 의 무대 장면 시험이 잰다.
#[test]
fn every_main_window_pass_opens_and_closes_through_one_function() {
    let gpu = read("src/gfx/gpu.rs");
    let bridge = read("src/gfx/gpu/egui_bridge.rs");
    for (src, sig) in [
        (&gpu, "fn render_fullscreen_stage("),
        (&bridge, "fn run_egui_frame("),
    ] {
        let body = squeeze(&fn_body(src, sig).unwrap_or_else(|| panic!("{sig}")));
        at(&body, "self.run_main_pass(", sig);
        assert!(
            !body.contains("take_egui_input(") && !body.contains("take_pass_input("),
            "{sig}: 입력을 직접 꺼낸다"
        );
    }
    let pass = squeeze(&fn_body(&bridge, "fn run_main_pass(").expect("run_main_pass"));
    let begin = at(&pass, "state.popups.begin_key_pass(", "패스 열기");
    let take = at(
        &pass,
        concat!("self.take_pass_input", "(window,true)"),
        "나눈 입력 꺼내기",
    );
    let run = at(&pass, "self.egui_ctx.run(", "패스 실행");
    let close = at(&pass, "state.close_input_pass(&self.egui_ctx);", "몫 닫기");
    assert!(
        begin < take && take < run && run < close,
        "열기 → 입력 → 실행 → 닫기 순서가 아니다"
    );
    let mut splitting = Vec::new();
    for (path, text) in super::rust_sources() {
        let code = squeeze(&strip_comments(&text));
        // 이 가드 자신의 문자열이 세이지 않도록 이어 붙인다.
        let n = code
            .matches(concat!("take_pass_input", "(window,true)"))
            .count()
            + code.matches(concat!(".split", "_pass(")).count();
        if n > 0 {
            splitting.push((path.display().to_string(), n));
        }
    }
    let allowed = [
        "src/gfx/gpu/egui_bridge.rs",
        "src/gfx/gpu/text_selection.rs",
        "src/adapters/ui/popup/window_input_test_support.rs",
    ];
    assert!(
        splitting
            .iter()
            .all(|(p, n)| *n == 1 && allowed.iter().any(|a| p.ends_with(a))),
        "끊은 자리로 입력을 나누는 자리가 run_main_pass 밖에 있다: {splitting:?}"
    );
}

/// debug 키 주입: 메인 창 입구가 `inject_key`(끊기 → 넣기)로 키를 넣는다. 포인터(`press_focus`)·글자(`key_focus`)
/// 주입과 진입 순서가 같다. `inject_key` 의 동작은 `adapters::ui::popup::route::tests::pass_focus` 가 그 함수를 직접
/// 불러 잰다.
#[test]
fn a_debug_key_injection_cuts_the_segment_before_the_key() {
    let src = read("src/app/ipc/window_required.rs");
    let code = squeeze(&src);
    let at_key = code
        .find("ifcmd.request.method==\"debug.inject_egui_key\"{")
        .expect("debug.inject_egui_key 입구");
    let branch = &code[at_key..];
    let branch = &branch[..branch.find("returnIpcStep::Handled;").expect("입구 끝")];
    at(branch, "egui_key_body_with(", "키 본문");
    at(
        branch,
        "crate::view::main::debug_input::inject_key(&mutw.state,&read,&mutw.base.gpu,key,pressed,modifiers,)",
        "끊고 넣는 주입",
    );
    assert!(
        !branch.contains("egui_key_body("),
        "끊지 않고 넣는 본문을 쓴다"
    );
    let input = read("src/view/main/debug_input.rs");
    let inject = squeeze(&fn_body(&input, "fn inject_key<").expect("inject_key"));
    let cut = at(&inject, "key_focus(state,core,sink);", "입력 끊기");
    let push = at(
        &inject,
        "push_egui_key(sink,key_name,pressed,modifiers)",
        "키 넣기",
    );
    assert!(cut < push, "끊기 → 넣기 순서가 아니다");
    let text = squeeze(&fn_body(&input, "fn debug_inject_egui_text(").expect("text"));
    let text_cut = at(
        &text,
        "key_focus(&mutself.state,engine,&mutself.base.gpu);",
        "글자 주입의 입력 끊기",
    );
    let text_push = at(&text, "push_egui_text(&mutself.base.gpu,text)", "글자 넣기");
    assert!(
        text_cut < text_push,
        "글자 주입이 끊기 → 넣기 순서가 아니다"
    );
}

/// 창의 패스 경로(무대·일반 화면)는 지금 무대 상태로 고른다(`MainViewState::pass_draws_stage`). 그 몫의 포커스 노드가 그 화면에 없으면(닫힌 무대의 몫,
/// 그리기 전에 연 무대 앞 몫) 패스 입력을 꺼낸 자리에서 사용자 입력을 뺀다(`MainViewState::drop_input_without_its_node`).
/// 남긴 뗌은 egui 가 쥔 단추 것만 두고, 그 앞에 클릭 거리 밖 이동을 넣고 끌기·드롭 payload 를 지워 클릭·끌기 끝·드롭을
/// 만들지 않게 한다(`end_press_without_click`).
/// 몫 노드로 경로를 고르면 닫힌 무대의 몫이 빈 무대 프레임을 제출한다. 동작은 `adapters::ui::popup::route::tests::pass_focus`
/// 의 무대 장면이 잰다.
#[test]
fn the_window_picks_the_stage_pass_by_the_live_stage_and_drops_input_without_its_node() {
    let gpu = read("src/gfx/gpu.rs");
    let render = squeeze(&fn_body(&gpu, "pub fn render(").expect("render"));
    at(
        &render,
        "ifstate.pass_draws_stage(){returnself.render_fullscreen_stage(state,engine,window);}",
        "무대 경로 선택",
    );
    assert!(
        !render.contains("pass_focus_node"),
        "패스 경로를 몫의 포커스 노드로 고른다"
    );
    let node = read("src/state/focus_node.rs");
    let pick = squeeze(&fn_body(&node, "fn pass_draws_stage(").expect("pass_draws_stage"));
    assert_eq!(
        pick, "{self.fullscreen_stage_active()}",
        "무대 경로를 지금 무대 상태가 아닌 값으로 고른다"
    );
    let dropper = squeeze(
        &fn_body(&node, "fn drop_input_without_its_node(").expect("drop_input_without_its_node"),
    );
    let retain = at(
        &dropper,
        "events.retain(|e|!delivers_input(e));",
        "입력 사건만 빼기",
    );
    let end = at(
        &dropper,
        "end_press_without_click(ctx,events);",
        "남긴 뗌 정리",
    );
    assert!(retain < end, "입력을 뺀 뒤 남긴 뗌을 정리한다");
    let ender =
        squeeze(&fn_body(&node, "fn end_press_without_click(").expect("end_press_without_click"));
    for (needle, what) in [
        (
            "egui::Event::PointerButton{button,pressed:false,..}=>ctx.input(|i|i.pointer.button_down(*button)),",
            "egui 가 쥐지 않은 단추의 뗌 빼기",
        ),
        ("ctx.stop_dragging();", "끌기 끝 없이 끌기 풀기"),
        (
            "egui::DragAndDrop::clear_payload(ctx);",
            "드롭 payload 지우기",
        ),
        (
            "events.insert(first,egui::Event::PointerMoved(origin+egui::vec2(away,0.0)));",
            "클릭 후보 지우기(뗌 앞 이동)",
        ),
    ] {
        at(&ender, needle, what);
    }
    let delivers = squeeze(&fn_body(&node, "fn delivers_input(").expect("delivers_input"));
    for needle in [
        "egui::Event::Key{pressed,..}|egui::Event::PointerButton{pressed,..}=>*pressed,",
        "egui::Event::Touch{phase,..}=>{!matches!(phase,egui::TouchPhase::End|egui::TouchPhase::Cancel)}",
        "_=>false,",
    ] {
        at(&delivers, needle, "누름을 끝내는 사건은 남긴다");
    }
    assert!(!delivers.contains("PointerGone"), "포인터 사라짐을 뺀다");
    let bridge = read("src/gfx/gpu/egui_bridge.rs");
    let pass = squeeze(&fn_body(&bridge, "fn run_main_pass(").expect("run_main_pass"));
    let take = at(
        &pass,
        // 위 가드가 세는 문자열이라 이어 붙인다.
        concat!("letmutraw_input=self.take_pass_input", "(window,true);"),
        "패스 입력 꺼내기",
    );
    let drop = at(
        &pass,
        "state.drop_input_without_its_node(&self.egui_ctx,&mutraw_input.events);",
        "노드 없는 입력 빼기",
    );
    let run = at(&pass, "self.egui_ctx.run(raw_input,", "egui 패스");
    assert!(
        take < drop && drop < run,
        "입력 꺼내기 → 빼기 → 패스 순서가 아니다"
    );
}

/// `window_id` 로 메인 창을 지정한 egui 주입(포인터·글자·키)도 ID 를 생략한 주입과 같은 진입(`ipc_step_main_window`:
/// 포인터 `press_focus`, 글자·키 `key_focus`)을 거친다. 라우터가 보조 창 직접 경로보다 먼저 메인 창을 고른다. 그 진입의
/// 끊기 동작은 `a_debug_key_injection_cuts_the_segment_before_the_key` 와 pass_focus 시험이 잰다.
#[test]
fn a_window_id_injection_into_a_main_window_takes_the_main_window_entry() {
    let router = squeeze(&read("src/app/ipc/debug_methods.rs"));
    let main = at(
        &router,
        "ifletSome((window,wid))=self.debug_main_window_by_id(&cmd.request.params){returnself.ipc_step_main_window(cmd,window,Some(wid));}",
        "메인 창 진입",
    );
    let direct = at(
        &router,
        "returnself.ipc_handle_debug_egui_to_window(cmd,kind);",
        "보조 창 직접 경로",
    );
    assert!(
        main < direct,
        "메인 창을 보조 창 직접 경로보다 먼저 고르지 않는다"
    );
    // 창을 지정한 포인터 주입의 `surface_id` 는 창 종류를 고르기 전에 한 번 거절한다(메인 창도 창 전체 좌표만 받는다).
    let reject = at(
        &router,
        "ifmatches!(kind,EguiInjection::Pointer)&&cmd.request.params.get(\"surface_id\").is_some_and(|v|!v.is_null()){returnsuper::debug_egui_window::reject_surface_with_window(cmd);}",
        "surface_id 거절",
    );
    assert!(
        reject < main,
        "surface_id 거절이 메인 창 진입보다 뒤에 있다"
    );
    let window = squeeze(&read("src/app/ipc/window_required.rs"));
    at(
        &window,
        "self.ipc_step_main_window(cmd,focused_id,None)",
        "ID 생략 경로의 같은 진입",
    );
    let pick = squeeze(&read("src/app/ipc/debug_egui_window.rs"));
    at(
        &pick,
        ".find(|(w,view)|u64::from(**w)==wid&&view.as_main().is_some())",
        "메인 창 판정",
    );
    assert!(
        !pick.contains("as_main_mut"),
        "보조 창 직접 경로에 닿지 않는 메인 창 갈래가 남았다"
    );
}

/// debug 클립보드 주입: 메인 창(ID 생략·메인 창 ID)은 `inject_clipboard`(끊기 → 넣기)로 넣는다. 보조 창만 큐에 바로
/// 넣는다. 동작은 `adapters::ui::popup::route::tests::key_transition` 의 붙여넣기 장면이 잰다.
#[test]
fn a_debug_clipboard_injection_into_a_main_window_cuts_the_segment_first() {
    let src = squeeze(&read("src/app/ipc/debug_egui_clipboard.rs"));
    let pick = at(
        &src,
        "ifletSome((window,wid))=main&&letSome((w,engine))=self.engines_mut().window_pair(window){crate::view::main::debug_input::inject_clipboard(&mutw.state,&engine.read(),&mutw.base.gpu,event,);",
        "메인 창 진입",
    );
    let direct = at(
        &src,
        "view.base_mut().gpu.debug_push_egui_events(vec![event]);",
        "보조 창 직접 경로",
    );
    assert!(
        pick < direct,
        "메인 창을 보조 창 직접 경로보다 먼저 고르지 않는다"
    );
    let input = read("src/view/main/debug_input.rs");
    let body = squeeze(&fn_body(&input, "fn inject_clipboard<").expect("inject_clipboard"));
    let cut = at(&body, "key_focus(state,core,sink);", "입력 끊기");
    let push = at(&body, "sink.push_events(vec![event]);", "넣기");
    assert!(cut < push, "끊기 → 넣기 순서가 아니다");
}

/// 창 입력의 키·IME 사건 하나를 view 가 처리한 뒤, 사용자 입력이 포커스를 주며 연 팝업 요청이 대기열에 있으면 그 자리에서
/// 대기열을 처리한다(`App::apply_user_focus_opens`). 그래야 같은 묶음의 다음 키가 사건 시점의 노드(연 팝업)로 끊긴다. 창
/// view 앞에서 소비한 plugin 단축키와 편집 보류 재생(원래 키·view 로 가는 줄 선 입력)도 같은 처리를 거친다. 판정
/// (`user_focus_open_queued`)은 host intent 대기열을 처리기와 같은 값(`intent::popup::opened_for`)으로, plugin 팝업 열기
/// 대기열을 여는 처리와 같은 출처(`PendingPopupOpen::opener`)로 본다. 동작은 `adapters::ui::popup::route::tests::key_transition`
/// 이 잰다(그 시험은 이 함수 대신 같은 판정과 대기열 처리를 부른다).
#[test]
fn a_key_that_opens_a_focused_popup_applies_the_open_before_the_next_event() {
    let src = read("src/app/edit_key_replay.rs");
    let route = squeeze(&fn_body(&src, "fn route_window_input(").expect("route_window_input"));
    let plugin = at(
        &route,
        "&&self.try_plugin_shortcut(id,ke){self.apply_user_focus_opens(id);return;}",
        "plugin 단축키 뒤 즉시 처리",
    );
    let view = at(&route, "self.dispatch_key_to_view(id,event);", "view 처리");
    assert!(plugin < view, "plugin 단축키 → view 순서가 아니다");
    let to_view =
        squeeze(&fn_body(&src, "fn dispatch_key_to_view(").expect("dispatch_key_to_view"));
    let key = at(
        &to_view,
        "letkey=matches!(event,WindowEvent::KeyboardInput{..}|WindowEvent::Ime(_));",
        "키 사건 판정",
    );
    let dispatch = at(
        &to_view,
        "self.dispatch_window_event_to_view(id,event);",
        "view 넘김",
    );
    let apply = at(
        &to_view,
        "ifkey{self.apply_user_focus_opens(id);}",
        "포커스 팝업 즉시 처리",
    );
    assert!(
        key < dispatch && dispatch < apply,
        "판정 → view → 즉시 처리 순서가 아니다"
    );
    let replay = squeeze(&fn_body(&src, "fn replay_edit_steps(").expect("replay_edit_steps"));
    assert_eq!(
        replay
            .matches("self.dispatch_key_to_view(id,event);")
            .count(),
        2,
        "편집 보류 재생의 원래 키·view 갈래가 즉시 처리를 거치지 않는다"
    );
    assert!(
        !replay.contains("dispatch_window_event_to_view"),
        "편집 보류 재생이 즉시 처리 없이 view 로 넘긴다"
    );
    let apply_fn =
        squeeze(&fn_body(&src, "fn apply_user_focus_opens(").expect("apply_user_focus_opens"));
    at(
        &apply_fn,
        ".is_some_and(|main|main.state.user_focus_open_queued());ifqueued{self.dispatch_pending_intents();self.dispatch_pending_popup_opens();}",
        "두 대기열 처리",
    );
    let state = read("src/state.rs");
    let queued =
        squeeze(&fn_body(&state, "fn user_focus_open_queued(").expect("user_focus_open_queued"));
    at(
        &queued,
        ".any(|intent|crate::intent::popup::gives_user_focus(self,intent))",
        "host intent 판정",
    );
    at(
        &queued,
        "crate::plugin_bridge::popup_scope::opened_by_user(open.opener())",
        "plugin 팝업 열기 판정",
    );
    let opens = squeeze(
        &fn_body(&read("src/app/dispatch/popup_opens.rs"), "fn open_one(").expect("open_one"),
    );
    at(&opens, "letopener=open.opener();", "여는 처리의 출처");
    at(
        &opens,
        "focus_opened_popup(state,mgr,instance_id,opener);",
        "여는 처리의 포커스",
    );
    let popup = read("src/intent/popup.rs");
    let handle = squeeze(&fn_body(&popup, "pub fn handle(").expect("handle"));
    at(
        &handle,
        "UiIntent::TogglePopup(request)=>{matchopened_for(state,request)",
        "토글 처리의 열린 인스턴스",
    );
    for (name, needle) in [
        (
            "fn open(",
            "ifletSome((instance,_))=opened_for(state,request){",
        ),
        (
            "fn reopen(",
            "ifletSome((instance,same_scope))=opened_for(state,request){",
        ),
    ] {
        let body = squeeze(&fn_body(&popup, name).expect(name));
        at(&body, needle, name);
    }
    let gives = squeeze(&fn_body(&popup, "fn gives_user_focus(").expect("gives_user_focus"));
    for needle in [
        "r.focus&&opened_for(state,r).is_none()",
        "r.focus&&!opened_for(state,r).is_some_and(|(_,same_scope)|same_scope)",
    ] {
        at(&gives, needle, "즉시 처리 판정의 열린 인스턴스");
    }
}
