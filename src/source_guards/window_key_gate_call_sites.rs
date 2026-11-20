//! 창이 키를 egui·터미널로 보내는 두 게이트가 시험이 밟는 판정 함수를 그대로 쓰는지 확인한다. 팝업 시험
//! (`adapters::ui::popup::window_key_gate_tests`)은 창 없이 같은 함수(`window_feeds_egui`, `window_escape`,
//! `window_holds_terminal_keys`)로 키를 보낸다. 판정 함수 안의 변이는 그 장면 시험이 잡는다. 호출부가 그 함수를
//! 거치지 않고 키를 보내는 우회는 장면 시험이 보지 못하므로 호출부의 모양을 여기서 고정한다.
//!
//! 잡지 못하는 것: 호출부 앞에 같은 키를 다른 길로 egui·PTY 에 보내는 코드를 더하는 것(첫 게이트만 본다).

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

/// egui 전달 게이트: `feed_egui_keyboard` 는 egui 로 보내기 전에 `window_feeds_egui` 의 결과로만 돌아간다.
#[test]
fn the_egui_feed_goes_through_the_window_feed_gate() {
    let src = read("src/view/main.rs");
    let body = squeeze(&fn_body(&src, "fn feed_egui_keyboard(").expect("feed_egui_keyboard"));
    let gate = at(
        &body,
        "letSome(bound)=self.state.window_feeds_egui(egui_surface,binds)else{self.note_unfed_key(event);return(false,false);};",
        "egui 전달 게이트",
    );
    let feed = at(
        &body,
        "self.base.gpu.handle_egui_event(",
        "egui 로 보내는 호출",
    );
    assert!(gate < feed, "egui 로 보낸 뒤에 게이트를 본다");
    at(
        &body,
        "letbinds=matches!(event,WindowEvent::KeyboardInput{..}|WindowEvent::Ime(_));",
        "키 주인을 묶는 키 종류",
    );
    assert_eq!(
        body.matches("return(false,false)").count(),
        1,
        "egui 전달 게이트 말고 보내지 않고 돌아가는 갈래가 생겼다. 시험이 보는 게이트가 하나가 아니다"
    );
}

/// 터미널 게이트: `handle_keyboard_input` 은 Esc 를 창의 Esc 처리로 보낸 뒤 `window_holds_terminal_keys` 가 참이면
/// 터미널로 보내지 않는다.
#[test]
fn the_terminal_goes_through_the_window_hold_gate() {
    let src = read("src/view/main/keyboard.rs");
    let body = squeeze(&fn_body(&src, "fn handle_keyboard_input(").expect("handle_keyboard_input"));
    let escape = at(
        &body,
        "ifself.try_consume_escape_key(event){return;}",
        "Esc 처리",
    );
    let hold = at(
        &body,
        "letoverlay_open=self.state.window_holds_terminal_keys();",
        "터미널 게이트",
    );
    // 게이트가 참이면 갈래 판정(`input_destination`)이 오버레이 갈래로 멈춘다. 갈래 모양은
    // `source_guards::input_destination_call_sites` 가 본다.
    let stop = at(
        &body,
        "matchself.state.input_destination(engine,overlay_open){InputDestination::Overlay=>return,",
        "터미널 게이트의 멈춤",
    );
    let pty = at(
        &body,
        "InputDestination::Terminal(_)=>self.forward_key_to_terminal(engine,event)",
        "터미널로 보내는 호출",
    );
    assert!(
        escape < hold && hold < stop && stop < pty,
        "Esc 처리 → 터미널 게이트 → 터미널 순서가 아니다"
    );
    let consume =
        squeeze(&fn_body(&src, "fn try_consume_escape_key(").expect("try_consume_escape_key"));
    at(
        &consume,
        "letescape_held=self.base.gpu.egui_escape_held();",
        "창의 Esc 처리가 직전 egui 패스의 Escape 눌림 상태를 넘긴다",
    );
    at(
        &consume,
        ".window_escape(self.base.gpu.egui_input_events_mut(),escape_held)",
        "창의 Esc 처리가 이번 egui 입력에서 Esc 를 뺀다",
    );
}

/// native WebView 가 넘긴 키: `pump_webview_key_events` 는 수집한 키를 `consume_forwarded_keys` 로만 소비하고, 그 루프는
/// 키마다 처리 → 포커스 열기 적용 → 결과 알림 순서로 돈다. 제품 소비자(`App`)의 적용은 winit 키와 같은
/// `apply_user_focus_opens` 다. 순서 자체는 `app::webview_keys` 의 소비 순서 시험이 잰다. 이 가드는 펌프가 루프를
/// 거치지 않고 키를 처리하거나 제품 소비자의 적용이 다른 함수로 바뀌는 우회를 잡는다.
#[test]
fn forwarded_webview_keys_are_consumed_through_the_consumer_loop() {
    let src = read("src/app/webview_keys.rs");
    let pump = squeeze(&fn_body(&src, "fn pump_webview_key_events(").expect("pump"));
    at(
        &pump,
        "consume_forwarded_keys(self,key_batch);",
        "소비 루프",
    );
    assert!(
        !pump.contains("dispatch_forwarded_webview_key(")
            && !pump.contains("finish_forwarded_webview_key("),
        "펌프가 소비 루프 밖에서 키를 처리한다"
    );
    let consume = squeeze(&fn_body(&src, "fn consume_forwarded_keys(").expect("consume"));
    let dispatch = at(&consume, "letoutcome=consumer.dispatch(id,ev);", "키 처리");
    let apply = at(
        &consume,
        "consumer.apply_focus_opens(id);",
        "포커스 열기 적용",
    );
    let finish = at(
        &consume,
        "consumer.finish(id,surface_id,key_id,outcome);",
        "결과 알림",
    );
    assert!(
        dispatch < apply && apply < finish,
        "처리 → 적용 → 알림 순서가 아니다"
    );
    // 대역 소비자(시험 모듈)와 trait 선언에도 같은 이름이 있으므로 제품 소비자 impl 만 잘라 읽는다.
    let start = src
        .find("impl ForwardedKeyConsumer for App")
        .expect("제품 소비자");
    let end = start
        + src[start..]
            .find("fn consume_forwarded_keys(")
            .expect("소비 루프");
    let consumer = &src[start..end];
    let applies = squeeze(&fn_body(consumer, "fn apply_focus_opens(").expect("apply"));
    assert_eq!(applies, "{self.apply_user_focus_opens(id);}");
    let dispatches = squeeze(&fn_body(consumer, "fn dispatch(").expect("dispatch"));
    assert_eq!(dispatches, "{self.dispatch_forwarded_webview_key(id,ev)}");
    let finishes = squeeze(&fn_body(consumer, "fn finish(").expect("finish"));
    assert_eq!(
        finishes,
        "{self.finish_forwarded_webview_key(id,surface_id,key_id,outcome);}"
    );
}

/// 조기 적용의 App 배선: 제품 상태 시험은 같은 판정(`user_focus_open_queued`) 뒤 host Ui 요청을 `intent::popup::handle`
/// 로, plugin 팝업 열기를 `open_pending_popups` 로 처리한다. App 은 그 둘을 `dispatch_pending_intents`(Ui 요청은 즉시
/// 갈래의 `dispatch_one_intent`)와 `dispatch_pending_popup_opens` 로 부른다. 판정 → 두 대기열 순서는
/// `press_focus_call_sites` 가 고정한다. 이 가드는 두 디스패처의 본문 전체(`dispatch_pending_popup_opens`,
/// `dispatch_pending_intents`), 창 요청 처리(`process_state_batches`)가 세션 묶음 처리(`dispatch_state_intents`)를 부르고
/// 그 안에 즉시 갈래의 `dispatch_one_intent` 호출이 있음, `classify_intent` 가 Ui 요청을 즉시 갈래로 둠, `dispatch_one_intent` 의 Ui 갈래가 `intent::popup::handle` 임을
/// 고정한다. `process_state_batches` 안의 반복·조건(창 묶음을 건너뛰는 변경)은 문자열 하나로만 보므로 고정하지 않는다.
#[test]
fn early_focus_opens_reach_the_handlers_the_state_test_calls() {
    let opens = read("src/app/dispatch/popup_opens.rs");
    let body = squeeze(
        &fn_body(&opens, "fn dispatch_pending_popup_opens(").expect("dispatch_pending_popup_opens"),
    );
    assert_eq!(
        body,
        "{open_pending_popups(self.plugin_manager.as_mut(),crate::app::window_access::engines_mut!(self).sessions(),);}",
        "plugin 팝업 열기 대기열이 open_pending_popups 로 가지 않는다"
    );
    let intents = read("src/app/dispatch/intents.rs");
    let pending = squeeze(
        &fn_body(&intents, "fn dispatch_pending_intents(").expect("dispatch_pending_intents"),
    );
    // 본문 전체를 고정한다. 창 요청 처리 호출을 조건으로 감싸거나 건너뛰는 우회(팔레트 열기 요청이 처리되지 않아 같은
    // 묶음의 다음 키가 막히지 않음)도 여기서 걸린다.
    assert_eq!(
        pending,
        "{letbatches=self.drain_pending_batches();letmutdomain_batch:Vec<(crate::app::dispatch_domain::DispatchSource,crate::intent::DispatchedIntent,)>=Vec::new();self.process_state_batches(batches,&mutdomain_batch);self.run_domain_cascade(domain_batch);self.refresh_approval_presentations();#[cfg(feature=\"gui\")]self.redraw_windows_for_shared_lists();}",
        "창 요청이 꺼낸 그대로 처리 단계로 가지 않는다"
    );
    let process_fn =
        squeeze(&fn_body(&intents, "fn process_state_batches(").expect("process_state_batches"));
    at(
        &process_fn,
        "letdomain=dispatch_state_intents(core,state,&mutengine,batch,self.plugin_manager.as_ref(),view.as_deref(),);",
        "세션 묶음 처리",
    );
    let session =
        squeeze(&fn_body(&intents, "fn dispatch_state_intents(").expect("dispatch_state_intents"));
    at(
        &session,
        "IntentClass::Immediate=>{App::dispatch_one_intent(core,state,engine,&intent,plugins,view)}",
        "즉시 갈래",
    );
    let classify = squeeze(&fn_body(&intents, "fn classify_intent(").expect("classify_intent"));
    assert!(
        !classify.contains("Intent::Ui"),
        "Ui 요청이 즉시 갈래가 아니다"
    );
    let one = squeeze(&fn_body(&intents, "fn dispatch_one_intent(").expect("dispatch_one_intent"));
    at(
        &one,
        "Intent::Ui(_)=>{crate::intent::popup::handle(state,engine,intent);}",
        "Ui 요청 처리",
    );
}

/// native WebView 가 넘긴 키의 제품 디스패치는 제품 상태 시험(`app::webview_keys::state_tests`)이 부르는 창 단계를 같은
/// 순서로 부른다: 게이트·plugin 단축키(`forwarded_gate_and_plugin_steps`, 결과값까지 함께 쓴다) → 새로고침 → 받은
/// surface 의 내장 단축키. 내장 단축키는 `handle_keybinding_shortcuts` 로, 결과 알림의 재전송은 `resends_to_page` 로
/// 정한다. 시험은 이 단계 안의 변이를 잡고, 이 가드는 제품이 단계를 건너뛰거나 순서를 바꾸는
/// 우회를 잡는다.
#[test]
fn forwarded_webview_keys_go_through_the_window_steps_the_state_test_calls() {
    let src = read("src/app/webview_keys.rs");
    let dispatch = squeeze(
        &fn_body(&src, "fn dispatch_forwarded_webview_key(")
            .expect("dispatch_forwarded_webview_key"),
    );
    let steps = [
        (
            "forwarded_gate_and_plugin_steps(&mutmain.state,&engine,modal_active,self.plugin_manager.as_mut(),|sid|webviews.contains_key(&sid),&ev,){returnoutcome;}",
            "게이트·plugin 단축키",
        ),
        (
            "dispatch_native_refresh_key(main,&mutengine,&ev)",
            "새로고침",
        ),
        (
            "main.handle_received_shortcut(&engine.read(),&ev.key,ev.mods,&target)",
            "내장 단축키",
        ),
    ];
    let mut last = 0;
    for (needle, what) in steps {
        let pos = at(&dispatch, needle, what);
        assert!(last <= pos, "{what} 의 순서가 바뀌었다");
        last = pos;
    }
    let finish = squeeze(
        &fn_body(&src, "fn finish_forwarded_webview_key(").expect("finish_forwarded_webview_key"),
    );
    at(
        &finish,
        "webview.finish_forwarded_key(key_id,outcome.resends_to_page());",
        "재전송 판정",
    );
    let steps = squeeze(
        &fn_body(&src, "fn forwarded_gate_and_plugin_steps(")
            .expect("forwarded_gate_and_plugin_steps"),
    );
    let gate = at(
        &steps,
        "if!forwarded_key_gate_open(state,engine,modal_active){returnSome(ForwardedKeyOutcome::Gated);}",
        "게이트",
    );
    let shortcut = at(
        &steps,
        "crate::app::plugin_glue::shortcut::plugin_shortcut_in_window(state,",
        "plugin 창 단계",
    );
    assert!(gate < shortcut, "게이트 → plugin 단축키 순서가 아니다");
    let plugin = read("src/app/plugin_glue/shortcut.rs");
    let window = squeeze(
        &fn_body(&plugin, "fn dispatch_plugin_shortcut_key(")
            .expect("dispatch_plugin_shortcut_key"),
    );
    at(
        &window,
        "plugin_shortcut_in_window(&mutmain.state,",
        "plugin 창 단계",
    );
    let received = squeeze(
        &fn_body(
            &read("src/adapters/ui/input/shortcuts/dispatch.rs"),
            "fn run_keybinding_shortcuts(",
        )
        .expect("run_keybinding_shortcuts"),
    );
    at(
        &received,
        "if!Self::handle_keybinding_shortcuts(",
        "내장 단축키 단계",
    );
}
