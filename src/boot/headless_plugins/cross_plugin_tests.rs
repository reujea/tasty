//! 헤드리스 dispatch가 플러그인 호출을 공통 게이트 뒤 다른 플러그인의 namespace 소유자에게
//! 전달하고, 그 답을 호출자에게 돌려주는지 실제 dispatch 함수로 잰다.
//! GUI는 같은 매니저 전달 함수를 부르며 판정은 매니저 시험이 잰다.

use std::collections::HashSet;
use std::sync::{Arc, mpsc};

use serde_json::json;
use tasty_plugin_manifest::Permission;

use super::dispatch_plugin_ipc_calls_headless;

/// 시험마다 다른 소유자와 prefix 를 쓴다. 공유 시험 표에 동시에 올라가기 때문이다.
const OWNERS: [(&str, &str); 3] = [
    ("com.test.headless-cross-owner-ok", "hdlcrossok"),
    ("com.test.headless-cross-owner-no", "hdlcrossno"),
    ("com.test.headless-cross-owner-clip", "hdlcrossclip"),
];
const CALLER: &str = "com.test.headless-cross-caller";

/// 공통 게이트가 prefix 를 플러그인 namespace 로 알아보도록 공유 시험 표에 소유자를 올린다.
/// 부팅에서 설치하는 전역 표를 대신하며 놓을 때 자기 등록만 지운다.
struct GateRegistration(&'static str);

impl GateRegistration {
    fn new(owner: &'static str, prefix: &str) -> Self {
        crate::namespace_table_for_tests::installed_test_table()
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .register(owner, prefix)
            .expect("test prefix must be free");
        Self(owner)
    }
}

impl Drop for GateRegistration {
    fn drop(&mut self) {
        crate::namespace_table_for_tests::installed_test_table()
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .unregister_plugin(self.0);
    }
}

struct Fixture {
    _gate: GateRegistration,
    app: crate::app::App,
    session: crate::runtime::engine_session::EngineSession,
    state: crate::state::RequestContext,
    _home: crate::test_support::IsolatedHome,
}

fn fixture((owner, prefix): (&'static str, &str)) -> Fixture {
    let home = crate::test_support::IsolatedHome::new();
    let (events_tx, _events) = mpsc::channel();
    let waker = crate::adapters::production::headless_waker::HeadlessWaker::new(events_tx);
    let mut app =
        crate::app::App::new_headless(waker.journal_waker(), None, None).expect("headless app");
    app.plugin_manager = Some(crate::plugin::PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    ));
    let session = crate::runtime::engine_session::EngineSession::new(80, 24, Arc::new(|| {}))
        .expect("engine");
    let state = crate::state::RequestContext::new(
        &session.as_ref().read(),
        app.services.preset_store.clone(),
    );
    Fixture {
        _gate: GateRegistration::new(owner, prefix),
        app,
        session,
        state,
        _home: home,
    }
}

impl Fixture {
    /// 호스트 루프처럼 호출을 받은 뒤 게이트 결과를 기다리는 호출을 끝까지 진행한다.
    fn dispatch(&mut self) {
        dispatch_plugin_ipc_calls_headless(
            &mut self.app,
            &mut self.state,
            &mut self.session.borrow_mut(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !self.app.gate_queue.is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "a gated plugin call never finished"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
            let flow = crate::boot::headless_dispatch::advance_request_gates(
                &mut self.app,
                &mut self.state,
                &mut self.session,
            );
            assert!(flow.is_continue());
        }
    }

    fn manager(&mut self) -> &mut crate::plugin::PluginManager {
        self.app.plugin_manager.as_mut().expect("fixture manager")
    }
}

/// 허가된 호출자의 호출은 소유자에게 한 번 가고 params(호출자 문맥 포함)를 그대로 싣는다.
/// 소유자의 답은 같은 call_id 로 호출자에게 돌아온다. 전달이 없으면 호스트 처리기가
/// `Method not found` 로 답한다.
#[test]
fn a_cross_plugin_call_reaches_the_owner_and_its_answer_returns() {
    let (owner_id, prefix) = OWNERS[0];
    let mut fx = fixture(OWNERS[0]);
    let owner = fx
        .manager()
        .attach_namespace_stub_for_test(owner_id, prefix);
    let caller = fx.manager().attach_caller_stub_for_test(
        CALLER,
        HashSet::from([Permission::IpcInvoke(prefix.into())]),
    );
    let params = json!({"_tasty_context": {"provider": "p", "thread_id": "t"}});
    fx.manager()
        .queue_plugin_call_for_test(CALLER, 9, &format!("{prefix}.parent"), params.clone());
    fx.dispatch();

    let replies = caller.drain_requests();
    assert!(
        replies.is_empty(),
        "소유자가 답하기 전에 호출자에게 회신이 갔다(호스트 처리기로 샌 것): {replies:?}"
    );
    let sent = owner.drain_invoke_params();
    assert_eq!(sent.len(), 1, "소유자에게 한 번만 가야 한다: {sent:?}");
    assert_eq!(sent[0].1["method"], format!("{prefix}.parent"));
    assert_eq!(sent[0].1["params"], params);
    assert_eq!(sent[0].1["caller_plugin_id"], CALLER);

    fx.manager()
        .answer_namespace_stub_for_test(owner_id, sent[0].0, json!({"receiver": "r"}));
    let replies = caller.drain_requests();
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0].1["call_id"], 9);
    assert_eq!(replies[0].1["result"], json!({"receiver": "r"}));
}

/// 권한이 없는 호출은 공통 게이트나 전달 검사에서 권한 오류가 되고 소유자에게 가지 않는다.
#[test]
fn an_ungranted_cross_plugin_call_is_refused_without_reaching_the_owner() {
    let (owner_id, prefix) = OWNERS[1];
    let mut fx = fixture(OWNERS[1]);
    let owner = fx
        .manager()
        .attach_namespace_stub_for_test(owner_id, prefix);
    let caller = fx
        .manager()
        .attach_caller_stub_for_test(CALLER, HashSet::new());
    fx.manager()
        .queue_plugin_call_for_test(CALLER, 3, &format!("{prefix}.parent"), json!({}));
    fx.dispatch();

    assert!(owner.drain_invoke_params().is_empty());
    let replies = caller.drain_requests();
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0].1["call_id"], 3);
    assert_eq!(replies[0].1["error_code"], -32001, "{replies:?}");
}

/// plugin 의 `clipboard.set_text` 는 호스트 클립보드에 쓰고 결과를 돌려준다 — 외부 IPC 의 App 층 분기를
/// plugin 호출은 안 거치므로 engine 라우터가 답한다. 빠지면 "no dispatch arm" 으로 실패한다.
/// `clipboard.write` 권한이 없으면 공통 게이트가 막는다.
#[test]
fn a_plugin_clipboard_write_reaches_the_host_clipboard() {
    let mut fx = fixture(OWNERS[2]);
    let clip = Arc::new(crate::adapters::test::mock_clipboard::MockClipboard::default());
    fx.app.services.replace_clipboard_for_test(clip.clone());
    let caller = fx
        .manager()
        .attach_caller_stub_for_test(CALLER, HashSet::from([Permission::ClipboardWrite]));
    fx.manager().queue_plugin_call_for_test(
        CALLER,
        5,
        "clipboard.set_text",
        json!({"text": "/full/path"}),
    );
    fx.dispatch();
    let replies = caller.drain_requests();
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0].1["call_id"], 5);
    assert_eq!(replies[0].1["result"], json!({"ok": true}), "{replies:?}");
    use crate::ports::clipboard::ClipboardSystem as _;
    assert_eq!(clip.read_text().expect("written"), "/full/path");

    let denied = fx
        .manager()
        .attach_caller_stub_for_test("com.test.headless-clip-denied", HashSet::new());
    fx.manager().queue_plugin_call_for_test(
        "com.test.headless-clip-denied",
        6,
        "clipboard.set_text",
        json!({"text": "nope"}),
    );
    fx.dispatch();
    let replies = denied.drain_requests();
    assert_eq!(replies[0].1["error_code"], -32001, "{replies:?}");
    assert_eq!(clip.read_text().expect("kept"), "/full/path");
}
