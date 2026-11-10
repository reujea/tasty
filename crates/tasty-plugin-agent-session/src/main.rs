#![forbid(unsafe_code)]
// 이유: 시험 빌드에서만 `let _` 로 값을 버린다 — `domain::i18n::tr()` 가 실물 lang
// 파일을 임시 폴더에 풀 때다(만들기·쓰기 실패는 빈 표가 되어 시험이
// 빨간불로 잡는다). 출하 경로에는 그 코드가 없다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

//! Agent Session plugin — Claude Code 를 stream-json 모드로 띄워 대화 탭(`agent_session`)으로 보인다.
//!
//! claude 자식은 이 plugin 프로세스가 직접 띄우고 쥔다([`local`]). 탭은 webview 문서이고 턴 기록은
//! `webview.post_message` 로 증분 갱신한다. 실행 설정은 이름별 프로필 파일(`agents`)에서 온다.
//! 동작 계약은 `docs/surfaces/agent-session/index.md` 에 있다.
//!
//! # 이 plugin 이 하지 않는 것
//!
//! - **호스트의 `terminal.*` 를 대체하지 않는다.** 그쪽은 *"이 surface 의 자식"* 을 묻고
//!   이쪽은 *"이 plugin 이 띄운 세션"* 을 묻는다 — 다른 물음이라 합치지 않는다.
//! - claude 가 아닌 에이전트는 띄우지 않는다.
//!
//! # 의존 방향
//!
//! `supervisor` 크레이트에 의존하지 **않는다**. 계약은 소켓과 JSON 뿐이며
//! 이 plugin 은 별도 OS 프로세스다 — 호스트 코드에도 의존하지 않고
//! `tasty-plugin-sdk` 만 쓴다.

mod adapters;
mod application;
mod domain;
mod ports;
#[cfg(test)]
mod shell_tests;
#[cfg(test)]
mod test_support;

/// 자동 sync 주기. 회수 임계(기본 600초)보다 훨씬 촘촘하되, 호출 비용
/// (~0.10초)이 묻힐 만큼은 길게.
const AUTO_SYNC_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

#[cfg(test)]
use adapters::driving::ipc::*;
use application::plugin::SupervisorPlugin;
use ports::host::HostCall;
use serde_json::{Value, json};
use tasty_plugin_sdk::{
    HostHandle, IpcMethodCtx, IpcMethodError, Plugin, SurfaceCreateCtx, SurfaceResult,
};

const PLUGIN_ID: &str = "com.tasty.agent-session";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Plugin for SupervisorPlugin<SdkHost, adapters::driven::local::LocalSupervisor> {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn version(&self) -> &str {
        PLUGIN_VERSION
    }

    /// 회수를 **주기적으로 화면에 맞춘다**.
    ///
    /// # 왜 폴링인가 — 구독으로는 못 잡는다
    ///
    /// 회수는 `session.subscribe` 채널에 **안 온다**(실측 2026-09-21:
    /// `stop` 을 걸어도 조용하다). 그쪽이 내는 것은 턴 이벤트뿐이라
    /// `watch` 로는 영영 못 본다.
    ///
    /// # 왜 별도 스레드인가
    ///
    /// 요청 하나가 plugin 루프를 점유한다(실측: 막힌 채로 두면 호스트가
    /// 60초에 죽인다). 그래서 **재우는 일은 워커가** 하고, 실제 작업은
    /// `self_invoke` 로 정규 경로에 되던진다 — `tasty-plugin-markdown` 이
    /// idle auto-reload 에 쓰는 형태와 같다.
    ///
    /// 비용은 호출당 **~0.10초**이고 **이어진 수와 무관하다**(실측 1건 대
    /// 6건이 같았다 — 대부분이 왕복 비용이다). 30초 주기면 부담이 없다.
    fn on_start(&mut self, host: HostHandle, _bus: tasty_plugin_sdk::BusHandle) {
        // SDK 가 주는 핸들을 여기서 포트로 감싼다 — 아래와 모듈은 감싼 것만 본다.
        let host = SdkHost(host);
        self.host = Some(host.clone());
        self.activity.spawn_poller(host.clone(), self.sup);
        let skills_host = host.clone();
        let skills_sup = self.sup;
        adapters::driven::skills::start(move |job| {
            let started = std::time::Instant::now();
            let Some(found) = adapters::driven::skills::for_session(&skills_sup, &job.session)
            else {
                return;
            };
            tracing::info!(
                "[agent_session] surface={} 스킬 훑기 {}ms",
                job.surface_id,
                started.elapsed().as_millis()
            );
            let items = domain::commands::menu_items(job.agent_kind.as_deref(), &found);
            adapters::driven::skills::store(&job.session, found);
            application::session_surface::patch(
                &skills_host,
                job.surface_id,
                &json!({ "type": "commands", "items": items }),
            );
        });
        self.meters.start();
        if std::thread::Builder::new()
            .name("agent-session-autosync".to_string())
            .spawn(move || {
                loop {
                    std::thread::sleep(AUTO_SYNC_EVERY);
                    // 실패는 **삼킨다** — 데몬이 없을 때가 정상이고, 그때마다
                    // 시끄러우면 로그가 쓸모없어진다. 다음 주기에 또 해 본다.
                    if host.self_invoke("agent_session.sync", json!({})).is_err() {
                        // 되던질 수 없으면 plugin 이 내려가는 중이다 — 끝낸다.
                        break;
                    }
                }
            })
            .is_err()
        {
            tracing::warn!("자동 sync 워커를 못 띄웠다 — 회수 표시는 손으로 sync 해야 한다");
        }
    }

    /// `supervisor` kind 탭이 열렸다 — 세션 목록을 그린다.
    ///
    /// ⚠️ **`host` 를 `on_start` 에서 받아 둔다** — 이 콜백은 host 를 안 준다
    /// (SDK 계약). html·markdown plugin 이 같은 형태다.
    fn create_surface(&mut self, ctx: SurfaceCreateCtx) -> SurfaceResult {
        self.open_session_surface(ctx.surface_id, &ctx.params, false);
        SurfaceResult {
            snapshot: Some(self.snapshot_for(ctx.surface_id)),
            ..Default::default()
        }
    }

    /// 탭을 닫으면 그 세션을 **재운다**(2026-10-02 · 사용자 결정).
    ///
    /// # 왜 재우나 — 죽이지도, 그대로 두지도 않는다
    ///
    /// 종전에는 이 훅을 **구현하지 않았다**(SDK 기본은 빈 몸통). 그래서
    /// 탭을 닫아도 세션이 `running` 으로 남아 프로세스를 계속 점유했고,
    /// 데몬의 유휴 회수가 임계 시간 뒤에야 거뒀다(사용자 보고).
    ///
    /// ⚠️ **`delete` 가 아니다.** `stop` 은 좌표를 보존하므로 되살릴 때
    /// (Ctrl+Shift+T·재시작 복원) `resume` 으로 **대화가 이어진다** —
    /// 실측(2026-10-02): `stopped` 세션에 `send` 하면 자동 resume 되고
    /// 이전 턴의 내용을 기억했다. 지우면 그 복원이 빈 화면이 된다.
    ///
    /// **실패를 삼키지 않는다.** 재우지 못하면 그 세션은 계속 도는데,
    /// 그 사실이 어디에도 안 남으면 *"닫았는데 왜 살아 있나"* 를 다시
    /// 겪는다(지금 이 결함이 그 모양이었다).
    fn destroy_surface(&mut self, surface_id: u32) {
        self.close_surface(surface_id);
    }

    /// 페이지가 navigation 을 시도했다 — 이 surface 의 **입력**이다.
    ///
    /// webview 는 plugin 에 직접 이벤트를 못 보낸다. 그래서 입력 폼이
    /// fragment 로 이동하고 호스트가 그것을 여기로 넘긴다(markdown 선례).
    fn on_webview_navigation_attempt(
        &mut self,
        ctx: tasty_plugin_sdk::WebviewNavigationAttemptCtx,
    ) {
        self.navigate(ctx.surface_id, &ctx.url);
    }

    /// host 가 미는 이벤트 — 지금은 폴더 피커 결과 하나다.
    fn on_event(&mut self, ctx: tasty_plugin_sdk::EventDispatchCtx) {
        self.on_host_event(&ctx.envelope.key, &ctx.envelope.payload);
    }

    /// 재시작 복원 — 같은 화면을 다시 그린다.
    ///
    /// 이 표면은 **상태를 안 든다**(매번 데몬에 묻는다). 그래서 복원이
    /// 저장된 것을 되읽을 필요 없이 그냥 다시 그리면 된다.
    fn restore_surface(&mut self, ctx: tasty_plugin_sdk::SurfaceRestoreCtx) -> SurfaceResult {
        // **`create_surface` 와 같은 갈래를 써야 한다.** 여기서 kind 를 안 보면
        // 재시작·복원 때 대화 탭이 **대시보드로 바뀐다**(실물에서 밟았다).
        self.open_session_surface(ctx.surface_id, &ctx.data, true);
        SurfaceResult {
            snapshot: Some(self.snapshot_for(ctx.surface_id)),
            ..Default::default()
        }
    }

    fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> Result<Value, IpcMethodError> {
        // `tell` 은 턴이 끝나야 답이 있다 — 답을 미뤄 worker 를 막지 않는다(`tell` 모듈).
        let deferred = adapters::driving::ipc::DEFERRED_METHODS
            .contains(&ctx.method.as_str())
            .then(|| ctx.defer())
            .flatten();
        let IpcMethodCtx {
            method,
            params,
            host,
            ..
        } = ctx;
        // 호스트도 **경계 하나로** 본다 — 여기서 한 번만 감싼다.
        self.dispatch_ipc(&method, &params, &SdkHost(host), deferred)
    }
}

// SDK 의 host 를 surface_link 의 좁은 trait 에 맞춘다.
//
// 어댑터를 두는 이유: `surface_link` 가 SDK 타입을 몰라야 시험이 호스트 없이
// 그 논리를 잰다. SDK 를 직접 쓰면 그 모듈을 검사하려고 plugin 런타임이
// 통째로 필요해진다(spec 의 "포트는 검증 가능성의 전제" 와 같은 축).
//
// 핸들을 소유한다 — 복제가 싸고(`Arc` 몇 개) `Send` 라 워커 스레드에 그대로 넘긴다. 모듈은 이 타입을
// 모르고 `HostCall`·`SendHost` 로만 받는다. 감싸는 것은 SDK 가 핸들을 주는 자리(`on_start`·IPC·이벤트)뿐이다.
#[derive(Clone)]
struct SdkHost(HostHandle);

impl ports::host::HostCall for SdkHost {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.0.call(method, params).map_err(|e| e.to_string())
    }

    fn self_invoke(&self, method: &str, params: Value) -> Result<(), String> {
        self.0
            .self_invoke(method, params)
            .map_err(|e| e.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    // 문구를 **기동 시 1회** 해석한다. `run()` 도 내부에서 `PluginEnv::load()`
    // 를 부르지만(연결용), 우리는 그보다 먼저 locale 을 알아야 한다 —
    // 첫 화면이 그려지기 전에 채워져야 키가 그대로 노출되지 않는다.
    //
    // 실패해도 죽지 않는다: `domain::i18n::t` 가 키를 그대로 낸다.
    if let Ok(env) = tasty_plugin_sdk::PluginEnv::load() {
        domain::i18n::init(tasty_plugin_sdk::i18n::Translator::from_plugin_env(&env));
    }
    adapters::driven::shell_path::warm();
    // 세션 쪽 포트의 실물은 여기서만 만든다 — 모듈과 워커는 plugin 이 든 이 핸들을 받는다.
    let ran = tasty_plugin_sdk::run(SupervisorPlugin::<SdkHost, _>::new(
        adapters::driven::local::LocalSupervisor::new(),
    ));
    // 호스트가 plugin 을 내리면 띄운 claude 자식도 함께 내린다 — 남기면 아무도 답을 안 읽는 프로세스가 된다.
    adapters::driven::local::LocalSupervisor::shutdown();
    ran?;
    Ok(())
}

#[cfg(test)]
mod handler_tests {
    //! 핸들러 논리 — **데몬 없이** 잰다.
    //!
    //! 인터페이스로 묶기 전에는 이 시험들을 쓸 수 없었다. `client` 의
    //! 자유 함수가 `port_file: &Path` 를 받아 **소켓이 있어야만** 돌았고,
    //! 그래서 `client` 시험 13건은 전부 *응답 파싱*만 재고 있었다.

    use super::*;
    use crate::adapters::driving::navigation::resolve_pick;
    use crate::ports::supervisor::fake::FakeSupervisor;

    fn wire(
        id: u64,
        paths: &[&str],
        cancelled: bool,
    ) -> adapters::driving::folder_pick::ResultWire {
        adapters::driving::folder_pick::ResultWire {
            request_id: id,
            paths: paths.iter().map(|s| (*s).to_string()).collect(),
            cancelled,
        }
    }

    /// 답이 **자기 탭으로** 돌아간다 — 여러 탭이 동시에 피커를 썼어도.
    #[test]
    fn a_pick_goes_back_to_the_tab_that_asked() {
        let mut pending = std::collections::HashMap::from([(7, 100), (8, 200)]);
        let got = resolve_pick(&mut pending, &wire(8, &["/w/two/f.md"], false));
        assert_eq!(got, Some((200, "/w/two".to_string())));
        // 다른 탭의 요청은 **그대로 남는다**.
        assert_eq!(pending.get(&7), Some(&100), "남의 요청까지 지웠다");
    }

    /// 내가 안 낸 요청은 **무시한다** — 다른 plugin 의 답이 섞여 와도
    /// 엉뚱한 탭의 폴더를 바꾸지 않는다.
    #[test]
    fn a_result_we_never_asked_for_is_ignored() {
        let mut pending = std::collections::HashMap::from([(7, 100)]);
        assert_eq!(
            resolve_pick(&mut pending, &wire(99, &["/w/x/f.md"], false)),
            None
        );
        assert_eq!(pending.len(), 1, "내 요청이 사라졌다");
    }

    /// 취소는 폴더를 **안 바꾼다**. 다만 표에서는 **지운다** — 안 지우면
    /// 취소된 요청이 영영 남는다.
    #[test]
    fn cancelling_changes_nothing_but_still_clears_the_slot() {
        let mut pending = std::collections::HashMap::from([(7, 100)]);
        assert_eq!(
            resolve_pick(&mut pending, &wire(7, &["/w/x/f.md"], true)),
            None
        );
        assert!(pending.is_empty(), "취소된 요청이 표에 남았다");
    }

    /// `sleep` 은 **재우고 나서** 표시한다 — 순서가 뒤집히면 안 된다.
    ///
    /// 표시를 먼저 켜면, 재우기가 실패했을 때 **자고 있지도 않은 탭에
    /// 하이라이트가 남는다**.
    #[test]
    fn sleeping_stops_the_session_before_marking_the_tab() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");

        let out = handle_sleep(&host, &sup, &json!({ "surface_id": 1 })).expect("sleep");

        assert_eq!(out["slept"], true);
        assert_eq!(sup.methods(), vec!["stop"], "데몬을 정확히 한 번 부른다");
        assert_eq!(out["marked"], true, "표시가 안 켜졌다");
    }

    /// 데몬이 없으면 **재우기가 실패하고 표시도 안 한다**.
    ///
    /// 실패했는데 표시가 켜지면 사용자는 *"재워졌다"* 고 읽는다 — 그 탭은
    /// 여전히 자원을 물고 있다.
    #[test]
    fn a_failed_sleep_does_not_mark_the_tab() {
        let sup = FakeSupervisor::failing("데몬 없음");
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");

        let r = handle_sleep(&host, &sup, &json!({ "surface_id": 1 }));

        assert!(r.is_err(), "데몬이 없는데 성공했다");
        assert_eq!(
            application::surface_link::attention_of(&host, 1).expect("조회"),
            None,
            "실패했는데 표시가 켜졌다 — 자원을 문 탭이 자는 것으로 보인다"
        );
    }

    /// `start` 는 **못 이으면 띄운 것을 되돌린다**.
    ///
    /// 되돌리지 않으면 세션은 떠 있는데 아무 탭도 그것을 모른다 —
    /// 자원만 문 유령이다.
    #[test]
    fn a_start_that_cannot_link_rolls_the_new_session_back() {
        let sup = FakeSupervisor::new();
        // 잇기가 실패하는 호스트(쓰기를 거절한다).
        let host = application::surface_link::tests_support::FakeHost::write_only_failure();

        let r = handle_start(&host, &sup, &json!({ "surface_id": 1, "agent": "cc" }));

        assert!(r.is_err(), "못 이었는데 성공했다");
        assert_eq!(
            sup.methods(),
            vec!["spawn", "stop"],
            "띄운 뒤 되돌리지 않았다 — 유령 세션이 남는다"
        );
    }

    /// sync 는 깨어 있는 세션의 **회수 표시만** 거둔다 — 턴 완료 표시는 남는다.
    #[test]
    fn sync_keeps_a_done_mark_and_clears_a_sleep_mark() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");
        application::surface_link::mark_done(&host, 1).expect("표시");
        handle_sync(&host, &sup, &mut Default::default()).expect("sync");
        assert_eq!(
            application::surface_link::attention_of(&host, 1)
                .expect("조회")
                .as_deref(),
            Some("completion"),
            "깨어 있는 세션의 완료 표시를 지웠다"
        );
        application::surface_link::mark_reclaimed(&host, 1).expect("표시");
        let out = handle_sync(&host, &sup, &mut Default::default()).expect("sync");
        assert_eq!(
            application::surface_link::attention_of(&host, 1).expect("조회"),
            None
        );
        assert_eq!(out["cleared"], json!([1]));
    }

    /// CLI 로 잇거나 띄운 뒤 그 탭을 다시 연다 — 안 열면 탭이 시작 폼에 남는다.
    #[test]
    fn cli_link_and_start_reopen_the_tab() {
        let main = crate::test_support::wiring_source();
        for arm in [
            r#""agent_session.link" => {"#,
            r#""agent_session.start" => {"#,
        ] {
            let body = main
                .split(arm)
                .nth(1)
                .and_then(|s| s.split("\n            }").next())
                .expect(arm);
            assert!(
                body.contains("self.reopen_linked("),
                "{arm} 가 탭을 다시 안 연다"
            );
        }
    }

    /// 탭에서 띄우는 세션은 물음을 탭이 받는다(`permission_prompts`) — 데몬 판은 `client` 가 거른다.
    #[test]
    fn a_tab_started_session_asks_for_prompts() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::new();
        // 결과는 보지 않는다 — 이 시험이 재는 것은 spawn 에 실린 칸이고, 잇기의 성패는 다른 시험 몫이다.
        let _ = handle_start(&host, &sup, &json!({ "surface_id": 1, "agent": "cc" }));
        assert!(
            sup.prompts_requested.get(),
            "탭에서 띄웠는데 물음을 안 받겠다고 했다"
        );
    }

    /// 회수 표시는 잠들 때 한 번만 켠다 — 사용자가 지운 표시를 다음 주기가 되살리지 않는다.
    #[test]
    fn sync_marks_a_sleeping_session_once() {
        let sup = FakeSupervisor::new();
        *sup.state.borrow_mut() = "stopped".into();
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");
        let mut marked = std::collections::BTreeSet::new();
        handle_sync(&host, &sup, &mut marked).expect("sync");
        assert_eq!(
            application::surface_link::attention_of(&host, 1)
                .unwrap()
                .as_deref(),
            Some("needs_input")
        );
        application::surface_link::clear_reclaimed(&host, 1).expect("사용자가 읽었다");
        handle_sync(&host, &sup, &mut marked).expect("sync");
        assert_eq!(
            application::surface_link::attention_of(&host, 1).unwrap(),
            None,
            "읽은 표시를 되살렸다"
        );
        *sup.state.borrow_mut() = "running".into();
        handle_sync(&host, &sup, &mut marked).expect("sync");
        *sup.state.borrow_mut() = "stopped".into();
        handle_sync(&host, &sup, &mut marked).expect("sync");
        assert_eq!(
            application::surface_link::attention_of(&host, 1)
                .unwrap()
                .as_deref(),
            Some("needs_input"),
            "다시 잠들었는데 안 켰다"
        );
    }

    /// 물음을 기다리는 세션의 `needs_input` 표시는 sync 가 거두지 않는다 — 회수 표시가 아니다.
    #[test]
    fn sync_keeps_the_mark_of_a_session_waiting_on_a_prompt() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");
        *sup.needs_input.borrow_mut() = Some(json!({ "request_id": "r", "kind": "permission" }));
        application::surface_link::mark_reclaimed(&host, 1).expect("표시");
        handle_sync(&host, &sup, &mut Default::default()).expect("sync");
        assert_eq!(
            application::surface_link::attention_of(&host, 1)
                .expect("조회")
                .as_deref(),
            Some("needs_input"),
            "물음을 기다리는데 표시를 거뒀다"
        );
    }

    /// `status` 는 턴이 도는가를 최상위 `turn` 으로도 낸다 — 작업 그래프의 완료 판정이 그 칸을 읽는다.
    #[test]
    fn status_carries_the_turn_state_at_the_top() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");
        let p = json!({ "surface_id": 1 });
        assert_eq!(
            handle_status(&host, &sup, &p).expect("status")["turn"],
            "idle"
        );
        sup.turn_active.set(true);
        assert_eq!(
            handle_status(&host, &sup, &p).expect("status")["turn"],
            "running"
        );
        // 매니페스트의 완료 전략이 이 칸을 본다.
        let manifest = include_str!("../tasty-plugin.toml");
        assert!(
            manifest.contains(r#"state_field = "turn""#)
                && manifest.contains(r#"poll_method = "agent_session.status""#)
        );
    }

    /// `tell` 의 본문은 위치 인자(`message`)나 `--text` 중 하나다 — 둘 다 없거나 둘 다 있으면 거절한다.
    #[test]
    fn tell_takes_the_text_either_way_but_not_twice() {
        assert_eq!(
            application::request::tell_text(&json!({ "message": "가" })).expect("위치"),
            "가"
        );
        assert_eq!(
            application::request::tell_text(&json!({ "text": "나" })).expect("플래그"),
            "나"
        );
        let twice = application::request::tell_text(&json!({ "message": "가", "text": "나" }))
            .expect_err("둘 다");
        assert!(twice.to_string().contains("--text"), "{twice}");
        let none = application::request::tell_text(&json!({ "text": "" })).expect_err("없음");
        assert!(
            none.to_string().contains("agent-session tell"),
            "사용법이 없다: {}",
            none
        );
    }

    /// 탭이 가리키는 세션이 지워졌으면 **연결이 낡았다**고 말하고 끊는 법을 준다.
    #[test]
    fn telling_a_deleted_session_names_the_stale_link() {
        let mut sup = FakeSupervisor::new();
        sup.fail = Some(ports::supervisor::ClientError::Daemon {
            kind: "not_found".into(),
            detail: "그런 세션이 없다".into(),
        });
        let host = application::surface_link::tests_support::FakeHost::linked(7, "지운세션");
        let e = handle_tell(&host, &sup, &json!({ "surface_id": 7, "text": "안녕" }))
            .expect_err("지워진 세션");
        let m = e.message;
        assert!(m.contains("지운세션") && m.contains("--surface 7"), "{m}");

        // 다른 거절은 그대로 올린다 — 낡은 연결로 둔갑시키지 않는다.
        let busy = FakeSupervisor::failing("소켓 없음");
        let e = handle_tell(&host, &busy, &json!({ "surface_id": 7, "text": "안녕" }))
            .expect_err("데몬 없음");
        assert!(!e.message.contains("link-remove"), "{}", e.message);
    }

    /// `tell` 이 거두는 것은 회수 표시뿐이다 — 그 턴이 켠 완료 표시는 남는다.
    #[test]
    fn telling_keeps_a_done_mark() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");
        application::surface_link::mark_done(&host, 1).expect("표시");
        let out =
            handle_tell(&host, &sup, &json!({ "surface_id": 1, "text": "안녕" })).expect("tell");
        assert_eq!(out["cleared"], false);
        assert_eq!(
            application::surface_link::attention_of(&host, 1)
                .expect("조회")
                .as_deref(),
            Some("completion")
        );
    }

    /// `tell` 은 턴이 돌면 **표시를 거둔다**.
    #[test]
    fn telling_clears_the_sleep_mark() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");
        application::surface_link::mark_reclaimed(&host, 1).expect("표시");

        let out =
            handle_tell(&host, &sup, &json!({ "surface_id": 1, "text": "안녕" })).expect("tell");

        assert_eq!(out["cleared"], true);
        assert_eq!(
            application::surface_link::attention_of(&host, 1).expect("조회"),
            None,
            "턴이 돌았는데 표시가 남았다"
        );
    }

    /// `output` 은 **이 탭의 세션**에게 묻는다 — 안 이어졌으면 오류다.
    ///
    /// 세션 id 를 손으로 옮기지 않는 것이 이 plugin 의 값이고, 그 값은
    /// 잇기가 서 있을 때만 난다.
    #[test]
    fn reading_output_asks_the_session_tied_to_this_surface() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::linked(1, "세션1");

        let out =
            handle_output(&host, &sup, &json!({ "surface_id": 1, "lines": 5 })).expect("output");

        assert_eq!(out["session_id"], "세션1");
        assert_eq!(sup.methods(), vec!["output"], "데몬을 한 번만 부른다");
    }

    /// 안 이어진 탭에서 `output` 은 **빈 답이 아니라 오류다**.
    #[test]
    fn reading_output_on_an_unlinked_surface_is_an_error() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::new();

        let r = handle_output(&host, &sup, &json!({ "surface_id": 1 }));

        assert!(r.is_err(), "안 이어졌는데 성공했다");
        assert!(sup.methods().is_empty(), "잇기도 안 됐는데 데몬을 불렀다");
    }

    /// **이미 다른 탭이 든 세션도 보여 준다 — 다만 못 고른다**
    /// (2026-10-02 에 뒤집혔다).
    ///
    /// 종전 규칙은 *"안 보여 준다"* 였다. surface 는 세션 하나만 들 수
    /// 있어(`surface.meta` 1:1) 물린 것을 고르면 앞 탭의 짝이 조용히
    /// 끊기기 때문이다. 그 걱정은 지금도 유효하다.
    ///
    /// 그런데 **빼 버리면 사용자 쪽에서 세션이 사라진 것처럼 보인다** —
    /// *"다른 탭에 있다"* 와 *"없다"* 가 같은 화면이 된다(사용자 보고).
    /// 규칙을 지우지 않고 뒤집어 적는다: 목록에 두되 `taken_by` 로
    /// 어느 탭인지 싣고, 화면이 붙기 링크를 안 낸다.
    #[test]
    fn a_session_another_tab_holds_is_shown_but_not_offered() {
        let rows = vec![
            ports::supervisor::SessionRow::for_test("free-1", "running", Some("cc")),
            ports::supervisor::SessionRow::for_test("taken-2", "running", Some("cc")),
        ];
        let taken = [("taken-2".to_string(), 7u32)].into_iter().collect();

        let (agents, all) = application::session_resolve::pick_for_form(&rows, &taken);

        // **둘 다 보인다** — 빼면 "어디 갔지" 가 된다.
        assert_eq!(all.len(), 2, "물린 세션이 목록에서 사라졌다");
        let taken_row = all
            .iter()
            .find(|a| a.id == "taken-2")
            .expect("물린 세션이 없다");
        assert_eq!(taken_row.taken_by, Some(7), "어느 탭이 쓰는지 안 싣는다");
        // 빈 것은 그대로 고를 수 있어야 한다(양성 대조 — 없으면
        // *"전부 물렸다"* 도 통과한다).
        let free_row = all
            .iter()
            .find(|a| a.id == "free-1")
            .expect("빈 세션이 없다");
        assert_eq!(free_row.taken_by, None, "빈 세션을 물렸다고 했다");
        // agent 단추는 물린 것도 포함한다 — 새로 띄울 때 쓰는 이름이라
        // 그 세션이 물렸는지와 무관하다.
        assert_eq!(agents, vec!["cc".to_string()]);
    }

    /// 폼이 `agent` 를 주면 **새로 띄우고 이 탭에 잇는다**.
    ///
    /// `session_id` 를 주면 그것에 붙고, 둘 다 없으면 이미 이어진 것을 쓴다 —
    /// 셋이 다른 길이라 하나로 뭉치면 사용자가 무엇이 일어날지 모른다.
    #[test]
    fn the_form_can_start_a_session_and_tie_it_to_the_tab() {
        let sup = FakeSupervisor::new();
        let host = application::surface_link::tests_support::FakeHost::new()
            .with_surfaces(vec![json!({ "id": 1 })]);

        let p = crate::test_support::TestPlugin::default();
        let got = p.start_session_for(
            &host,
            &sup,
            1,
            "cc",
            &json!({ "params": { "cwd": "/tmp" } }),
        );

        // 가짜 데몬은 `새세션` 을 낸다.
        assert_eq!(
            got.as_ref().map(|s| s.id.as_str()),
            Ok("새세션"),
            "띄운 세션 id 가 안 돌아왔다"
        );
        // 그리고 **탭에 이어져야** 한다 — 안 이으면 아무도 못 찾는다.
        assert_eq!(
            application::surface_link::session_of_surface(&host, 1)
                .ok()
                .as_deref(),
            Some("새세션"),
            "띄우기만 하고 안 이었다"
        );
        // cwd 가 데몬까지 갔는지 — 폼이 받은 값이 새는 자리다.
        assert!(
            sup.methods().iter().any(|m| m == "spawn"),
            "데몬을 안 불렀다"
        );
    }

    /// `broadcast` 는 **surface 를 안 본다** — 안 이어진 탭에서도 선다.
    #[test]
    fn broadcasting_does_not_need_the_surface_to_be_linked() {
        let sup = FakeSupervisor::new();
        let out =
            handle_broadcast(&sup, &json!({ "text": "전체", "agent": "cc" })).expect("broadcast");
        assert_eq!(out["sent"].as_array().map(Vec::len), Some(1));
        assert_eq!(sup.methods(), vec!["broadcast"]);
    }
}

#[cfg(test)]
mod snapshot_wiring_tests {
    /// 두 표면 경로가 **세션을 실은 스냅샷**을 낸다 — 옛 `dashboard::snapshot()`
    /// (늘 `{}`)으로 되돌아가면 재시작 후 탭이 시작 폼으로 떨어진다.
    ///
    /// 소스를 본다: 실물 호스트 없이 `create_surface`·`restore_surface` 를
    /// 부를 수 없어(SDK ctx 를 만들 수 없다) **배선 자체**를 고정한다.
    #[test]
    fn both_surface_paths_snapshot_the_linked_session() {
        let src = crate::test_support::wiring_source();
        // 이 시험 자신은 빼고 센다 — 안 그러면 여기 적힌 이름까지 세어
        // 계수가 어긋나고, 그 어긋남을 맞추려다 시험이 무의미해진다.
        let prod = src.split("mod snapshot_wiring_tests").next().unwrap_or(src);
        let code: Vec<&str> = prod
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect();
        // R4 — 저장과 복원이 **둘 다** 배선돼야 한다. 하나만 있으면
        // 저장은 되는데 화면에 안 나오거나(복원 누락), 복원이 늘 빈
        // 대화를 읽는다(저장 누락). 이 세션에서 배선 누락을 세 번 밟았다.
        assert!(
            code.iter().any(|l| l.contains("snapshot_with_turns(")),
            " 대화가 스냅샷에 안 실린다 — 닫으면 사라진다"
        );
        assert!(
            code.iter().any(|l| l.contains("turns_from_snapshot(")),
            " 복원이 대화를 안 읽는다 — 저장만 되고 화면에 안 나온다"
        );
        let wired = code.iter().filter(|l| l.contains("snapshot_for(")).count();
        // 정의 1 + 호출 2.
        assert_eq!(
            wired, 3,
            "create_surface·restore_surface 둘 다 `snapshot_for` 를 써야 한다"
        );
        assert!(
            !code.iter().any(|l| l.contains("dashboard::snapshot()")),
            "늘 빈 객체를 내던 옛 스냅샷으로 되돌아갔다"
        );
        // 상태줄 값은 스냅샷이 아니라 meta 로 저장 · 복원한다 — 짝이다(`application::surface_link::USAGE_META_KEY`).
        assert!(
            code.iter().any(|l| l.contains("surface_link::load_usage(")),
            "복원이 상태줄 값을 안 읽는다"
        );
    }
}

#[cfg(test)]
mod user_scenarios {
    //! 사용자가 **실제로 밟는 순서**를 한 줄로 관통한다.
    //!
    //! 계층별 단위 시험(`session_surface`·`surface_link`·`handler_tests`)은
    //! 각 조각이 맞는지 보지만, **조각들이 이어지는지**는 안 본다. 실물에서
    //! 난 사고는 대부분 이음매였다 — 폼은 맞는데 표식이 안 걸렸다거나,
    //! 잇기는 됐는데 복원이 그것을 안 읽었다거나.
    //!
    //! 여기서는 **데몬도 호스트도 가짜**다. 실물 GUI 없이 도는 것이 목적이다.

    use super::*;
    use crate::ports::supervisor::fake::FakeSupervisor;
    use application::surface_link::tests_support::FakeHost;

    /// 폼이 내는 URL 을 **브라우저가 하듯** 되돌린다.
    ///
    /// HTML 이스케이프(`&amp;`)를 풀어야 실제 네비게이션 URL 이 된다 —
    /// 이 한 겹을 빼먹으면 시험이 폼과 다른 문자열을 보게 된다.
    fn href_from(html: &str, after: &str) -> String {
        let at = html
            .find(after)
            .unwrap_or_else(|| panic!("'{after}' 가 폼에 없다"));
        // ⚠️ **클래스를 건너뛰고 `href=` 를 찾는다.** 종전에는
        // `<a href="` 를 통째로 찾았는데, 단추 모양을 입히며
        // `<a class="btn" href="` 가 되자 **멀쩡한 코드가 빨개졌다**
        // (2026-10-02). 이 헬퍼가 알고 싶은 것은 **어디로 보내는가**지
        // 그 단추가 어떤 모양인가가 아니다.
        //
        let rel = html[at..].find(r##"href=""##).expect("href 가 없다");
        let i = at + rel + r##"href=""##.len();
        let raw = &html[i..][..html[i..].find('"').expect("href 가 안 닫혔다")];
        format!("about:blank{}", raw.replace("&amp;", "&"))
    }

    /// 시나리오 1 — **처음 온 사용자**: 빈 탭 → 폼 → agent 골라 시작 → 대화.
    #[test]
    fn a_new_user_starts_a_session_from_the_form_and_talks() {
        let sup = FakeSupervisor::new();
        let host = FakeHost::new();

        // 1) 이어진 것이 없으니 **시작 폼**이 나온다.
        let form = adapters::driving::view::render_start(&[], &[], &[], "", None);
        assert!(form.contains("에이전트 세션 시작"), "시작 폼이 아니다");

        // 2) agent 를 골라 제출한다 — 폼이 내는 URL 을 그대로 판다.
        let url = "about:blank?a=cc&agent-start=1";
        let form =
            crate::adapters::driving::url::parse_start(url).expect("시작 요청으로 안 읽혔다");
        assert_eq!(form.agent, "cc");
        assert!(form.cwd.is_none(), "안 준 cwd 가 생겼다");

        // 3) 세션이 뜨고 **이 탭에 이어진다**.
        let plugin = crate::test_support::TestPlugin::default();
        let sid = plugin
            .start_session_for(&host, &sup, 7, &form.agent, &json!({}))
            .expect("세션이 안 떴다")
            .id;
        assert_eq!(
            application::surface_link::session_of_surface(&host, 7).ok(),
            Some(sid.clone()),
            "띄웠는데 탭에 안 이어졌다"
        );
    }

    /// **입력 한 번에 문서를 한 번만 다시 싣는다**(0.1.17).
    ///
    /// # 왜 세나
    ///
    /// 이 surface 는 갱신 수단이 `webview.set_url` **하나뿐**이다(JS off ·
    /// 부분 갱신 없음). 한 번 실을 때마다 문서가 통째로 다시 그려져
    /// **흰 바탕이 스친다** — 재적재 횟수가 곧 깜빡임 횟수다.
    ///
    /// 종전에는 보내기 경로가 `push` 를 **두 번** 불렀다(잠금 1 + 결과 1).
    /// 실측(2026-09-29)에 둘이 **3ms 차이**로 함께 났다 — `send` 가 동기
    /// 블로킹이라 그 사이 webview 가 그려지지 않는다. 잠금 화면은 **한 번도
    /// 보인 적이 없고** 깜빡임만 두 배였다.
    ///
    /// **횟수로 단언한다.** *"깜빡이지 않는다"* 는 화면을 봐야 알지만,
    /// 재적재 횟수는 여기서 셀 수 있다.
    #[test]
    fn sending_one_message_reloads_the_document_only_once() {
        let host = FakeHost::new();
        let sup = FakeSupervisor::new();
        let mut convo = crate::domain::convo::Convo {
            session_id: Some("s1".into()),
            ..Default::default()
        };

        let before = host.set_url_calls();
        // 보내기 경로가 하는 일 그대로 — 보내고, 결과를 싣는다.
        crate::test_support::send(&sup, &mut convo, "hi");
        application::session_surface::push(
            &host,
            &adapters::driven::local::LocalSupervisor::new(),
            7,
            &convo,
            false,
        );

        assert_eq!(
            host.set_url_calls() - before,
            1,
            "입력 한 번에 문서를 {}번 다시 실었다 — 그만큼 깜빡인다",
            host.set_url_calls() - before
        );
        // 보낸 것이 실제로 대화에 남았나 — 재적재를 줄이느라 갱신 자체를
        // 빼면 이 단언이 막는다(0 회도 통과하면 안 된다).
        assert_eq!(convo.turns.len(), 1, "보냈는데 대화에 안 남았다");
    }

    /// 시나리오 2 — **이미 도는 세션에 붙는다**: 목록에서 골라 이 탭에.
    #[test]
    fn a_user_attaches_this_tab_to_a_session_that_is_already_running() {
        let host = FakeHost::new();
        let att = crate::domain::convo::Attachable {
            id: "aaaaaaaa-1111".into(),
            agent: "cc".into(),
            state: "running".into(),
            reclaimed: false,
            cwd: Some("/w/proj".into()),
            // 아무 탭도 안 들었다 — 그래야 붙기 링크가 나온다.
            taken_by: None,
            agent_session_id: None,
            agent_session_name: None,
            remaining: Default::default(),
        };
        let form =
            adapters::driving::view::render_start(&[], std::slice::from_ref(&att), &[], "", None);

        // 목록에 그 세션이 보이고, 누르면 **붙기 요청**이 된다.
        assert!(form.contains("aaaaaaaa"), "세션이 목록에 없다");
        assert!(
            form.contains("/w/proj"),
            "폴더가 안 보여 어느 세션인지 못 가른다"
        );
        // **기준점을 그 링크로 잡는다.** 제목을 기준으로 두면
        // 제목과 표 사이에 생긴 새로고침·정리 단추를 먼저 집는다
        // (2026-10-02 에 실제로 그랬다).
        let url = href_from(&form, r##"href="#s=aaaaaaaa"##);
        let id = crate::adapters::driving::url::parse_attach(&url).expect("붙기로 안 읽혔다");
        assert_eq!(id, att.id);

        // 붙기는 **시작이 아니다.** 이 단언이 없으면 두 표식이 같은 값이
        // 돼도 시나리오가 통과한다 — 폼과 파서가 같은 상수를 쓰므로
        // 자기들끼리는 계속 맞기 때문이다(변이로 드러났다).
        assert_eq!(
            crate::adapters::driving::url::parse_start(&url),
            None,
            "붙기 요청이 새 세션을 띄운다 — 도는 세션이 둘로 늘어난다"
        );

        // 붙이면 그 탭이 그 세션을 든다.
        application::surface_link::link(&host, 9, &id).expect("잇기 실패");
        assert_eq!(
            application::surface_link::session_of_surface(&host, 9).ok(),
            Some(id),
            "붙였는데 안 이어졌다"
        );
    }

    /// 시나리오 3 — **작업 폴더를 고른다**: 찾기 단추 → 피커 → 칸에 실린다.
    #[test]
    fn a_user_picks_a_working_folder_and_it_lands_in_the_field() {
        let form = adapters::driving::view::render_start(&[], &[], &[], "", None);
        let url = href_from(&form, r##"class="with-btn""##);

        // 찾기는 **시작이 아니다** — 눌러도 세션이 안 뜬다.
        assert_eq!(
            crate::adapters::driving::url::parse_browse(&url),
            Some(crate::domain::convo::PickField::Start)
        );
        assert_eq!(
            crate::adapters::driving::url::parse_start(&url),
            None,
            "찾기가 세션을 띄웠다"
        );
        // 표식 자체가 시작과 **달라야** 한다. 위 단언만으로는 부족하다 —
        // 찾기 URL 에는 agent(`a=`)가 없어 표식이 겹쳐도 parse_start 가
        // 우연히 None 을 낸다(변이로 드러났다).
        assert!(
            !url.contains("agent-start="),
            "찾기가 시작 표식을 싣는다 — agent 가 함께 오면 세션이 떠 버린다: {url}"
        );

        // 피커가 고른 파일의 **부모 폴더**가 작업 디렉토리다.
        let mut pending = std::collections::HashMap::from([(42u64, 7u32)]);
        let wire = adapters::driving::folder_pick::ResultWire {
            request_id: 42,
            paths: vec!["/w/proj/README.md".into()],
            cancelled: false,
        };
        let (sid, dir) = crate::adapters::driving::navigation::resolve_pick(&mut pending, &wire)
            .expect("폴더가 안 정해졌다");
        assert_eq!((sid, dir.as_str()), (7, "/w/proj"));

        // 그 값이 **다시 그린 폼의 칸에** 실린다.
        let again = adapters::driving::view::render_start(&[], &[], &[], &dir, None);
        assert!(
            again.contains(r##"value="/w/proj""##),
            "고른 폴더가 칸에 안 실렸다"
        );
    }

    /// 시나리오 4 — **재시작**: 껐다 켜도 그 탭이 같은 세션으로 돌아온다.
    ///
    /// 이 자리가 조용히 깨지기 쉽다 — 스냅샷이 비어도 탭은 복원되므로
    /// **화면만 보면 정상**이고, 세션만 사라진다.
    #[test]
    fn a_tab_comes_back_to_the_same_session_after_a_restart() {
        let host = FakeHost::linked(3, "sess-keep");

        // 저장 — 이어진 세션이 스냅샷에 실린다.
        let linked = application::surface_link::session_of_surface(&host, 3).ok();
        let snap = crate::domain::convo::snapshot_of(linked.as_deref());
        assert_eq!(snap["session_id"], "sess-keep", "세션이 스냅샷에 안 실렸다");

        // 복원 — 호스트가 돌려준 `data` 를 그대로 읽으면 그 세션이 나온다.
        let back = snap
            .pointer("/session_id")
            .and_then(Value::as_str)
            .expect("복원이 세션을 못 읽었다");
        assert_eq!(back, "sess-keep");
    }

    /// 시나리오 5 — **데몬이 없다**: 사용자는 그것을 **알아야** 한다.
    ///
    /// 조용히 실패하면 사용자는 세션이 생긴 줄 알고 기다린다.
    #[test]
    fn when_the_daemon_is_down_the_user_is_told_and_nothing_is_linked() {
        let sup = FakeSupervisor::failing("데몬 없음");
        let host = FakeHost::new();
        let plugin = crate::test_support::TestPlugin::default();

        let got = plugin.start_session_for(&host, &sup, 7, "cc", &json!({}));

        assert!(got.is_err(), "데몬이 없는데 세션 id 를 냈다");
        assert_eq!(
            application::surface_link::session_of_surface(&host, 7).ok(),
            None,
            "실패했는데 탭에 뭔가 이어졌다"
        );
    }
}

#[cfg(test)]
mod failure_layers {
    //! 계층별 **실패**를 가른다 — 각 층이 아래층의 실패를 어떻게 다루나.
    //!
    //! | 층 | 실패 원인 | 사용자가 봐야 할 것 |
    //! |---|---|---|
    //! | 데몬 | 안 떠 있다 | *"세션을 못 띄웠다"* — 조용하면 안 된다 |
    //! | 호스트 | 잇기 거절 | 세션을 **되돌린다**(고아 방지) |
    //! | 폼 | 값이 모자람 | 아무 일도 안 일어난다 |
    //! | 피커 | 취소·빈 결과 | 쳐 둔 값이 **안 지워진다** |

    use super::*;
    use crate::ports::supervisor::fake::FakeSupervisor;
    use application::surface_link::tests_support::FakeHost;

    /// 데몬 층 — 죽어 있으면 **세션도 잇기도 없다**.
    #[test]
    fn daemon_down_yields_no_session_and_no_link() {
        let plugin = crate::test_support::TestPlugin::default();
        let got = plugin.start_session_for(
            &FakeHost::new(),
            &FakeSupervisor::failing("소켓 없음"),
            5,
            "cc",
            &json!({}),
        );
        assert!(got.is_err(), "데몬이 죽었는데 세션이 생겼다");
    }

    /// 호스트 층 — 잇기를 거절하면 **띄운 세션을 되돌린다**.
    ///
    /// 안 되돌리면 데몬에 세션이 남는데 어느 탭도 그것을 모른다(고아).
    #[test]
    fn a_host_that_refuses_the_link_makes_us_roll_the_session_back() {
        let sup = FakeSupervisor::new();
        let plugin = crate::test_support::TestPlugin::default();

        let got =
            plugin.start_session_for(&FakeHost::write_only_failure(), &sup, 5, "cc", &json!({}));

        assert!(got.is_err(), "잇지도 못했는데 세션 id 를 냈다");
        assert!(
            sup.methods().iter().any(|m| m == "delete" || m == "stop"),
            "띄운 세션을 안 거뒀다 — 아무도 모르는 세션이 남는다: {:?}",
            sup.methods()
        );
    }

    /// 폼 층 — agent 가 없으면 **시작 요청이 아니다**.
    #[test]
    fn a_form_without_an_agent_is_not_a_start_request() {
        assert_eq!(
            crate::adapters::driving::url::parse_start("x?agent-start=1"),
            None
        );
        assert_eq!(
            crate::adapters::driving::url::parse_start("x?a=&agent-start=1"),
            None
        );
        assert_eq!(
            crate::adapters::driving::url::parse_start("x?a=%20%20&agent-start=1"),
            None,
            "공백만 들어왔다"
        );
    }

    /// 피커 층 — 취소·빈 결과는 **아무것도 안 바꾼다**.
    ///
    /// 빈 값으로 덮으면 사용자가 이미 쳐 둔 경로가 사라진다.
    #[test]
    fn a_cancelled_or_empty_pick_changes_nothing() {
        for (paths, cancelled, what) in [
            (vec!["/w/a.md"], true, "취소"),
            (vec![], false, "빈 결과"),
            // `/` 는 빠졌다 — folder 모드에서 **고를 수 있는 폴더**다
            // (2026-09-28). 폴더도 부모도 아닌 것만 여기 남는다.
            (vec!["bare.txt"], false, "폴더도 부모도 아닌 경로"),
        ] {
            let mut pending = std::collections::HashMap::from([(1u64, 9u32)]);
            let w = adapters::driving::folder_pick::ResultWire {
                request_id: 1,
                paths: paths.iter().map(|s| (*s).to_string()).collect(),
                cancelled,
            };
            assert!(
                crate::adapters::driving::navigation::resolve_pick(&mut pending, &w).is_none(),
                "{what} 인데 폴더를 정했다"
            );
            // 표는 비워야 한다 — 안 그러면 그 요청이 영영 남는다.
            assert!(pending.is_empty(), "{what} 뒤에 요청이 표에 남았다");
        }
    }

    /// 표면 층 — 이어진 세션이 없는 탭에서 출력을 읽으면 **오류**다.
    ///
    /// 빈 결과로 답하면 *"아직 답이 없다"* 와 구분되지 않는다.
    #[test]
    fn reading_output_without_a_link_is_an_error_not_an_empty_answer() {
        let sup = FakeSupervisor::new();
        let got = handle_output(&FakeHost::new(), &sup, &json!({ "surface_id": 4 }));
        assert!(got.is_err(), "이어진 것이 없는데 성공으로 답했다");
        assert!(sup.methods().is_empty(), "이어진 것도 없이 데몬을 불렀다");
    }
}

#[cfg(test)]
mod start_failure_tests {
    /// **시작 실패가 화면까지 간다** — 배선이 빠지면 spinner 가 안 걷힌다.
    ///
    /// 렌더가 사유를 그릴 줄 알아도 **아무도 안 채우면** 화면은 그대로다.
    /// 그리고 실패한 폼은 직전과 바이트까지 같아 호스트가 재적재를
    /// 건너뛰므로, 증상이 *"Starting... 에서 안 끝난다"* 로 나타난다
    /// (실사용 보고 2026-09-30).
    #[test]
    fn a_failed_start_puts_its_reason_on_the_convo() {
        // 시작 흐름은 `start.rs` 에 있다 — 앞에 붙여 아래 `split` 이 그 파일을 자르지 않게 한다.
        let src = &format!(
            "{}{}",
            include_str!("application/start.rs"),
            crate::test_support::wiring_source()
        );
        let prod = src.split("mod start_failure_tests").next().unwrap_or(src);
        let body = prod
            .split("fn start_from_form")
            .nth(1)
            .expect("start_from_form 이 없다");
        let body = body.split("\n    fn ").next().unwrap_or(body);

        assert!(
            body.contains("start_error = Some("),
            "실패 사유를 대화에 안 싣는다 — 화면이 안 바뀌어 spinner 가 남는다"
        );
        // 사유는 **실제 오류**여야 한다. 고정 문구면 무엇이 잘못됐는지 모른다.
        assert!(body.contains("Some(why)"), "실패 사유가 실제 오류가 아니다");
    }
}

#[cfg(test)]
mod start_form_wiring_tests {
    /// **시작 폼의 값들이 실제로 쓰이는가**(0.1.51).
    ///
    /// 폼이 칸을 그리고 파서가 읽어도, `spawn` 에 안 실으면 아무 일도
    /// 없다. 이 저장소가 여러 번 밟은 형태라 배선을 값으로 고정한다.
    #[test]
    fn the_start_form_passes_model_and_name_to_spawn() {
        // 시작 흐름은 `start.rs` 에 있다 — 앞에 붙여 아래 `split` 이 그 파일을 자르지 않게 한다.
        let src = &format!(
            "{}{}",
            include_str!("application/start.rs"),
            crate::test_support::wiring_source()
        );
        let prod = src
            .split("mod start_form_wiring_tests")
            .next()
            .unwrap_or(src);
        let body = prod
            .split("fn start_from_form")
            .nth(1)
            .expect("start_from_form 이 없다")
            .split("\n    fn ")
            .next()
            .unwrap_or_default()
            .to_string();
        for (needle, what) in [
            ("\"cwd\": form.cwd", "cwd"),
            ("\"model\": form.model", "model"),
            ("\"name\": form.name", "name"),
        ] {
            assert!(body.contains(needle), "{what} 를 spawn 에 안 싣는다");
        }
        // **첫 메시지 칸은 없다**(0.1.56 · 사용자 지정) — 대화창에서 바로
        // 치면 되고, 시작 폼에 두면 같은 일을 두 자리에서 하게 된다.
        //
        // `spawn` 의 `prompt` 로 싣지 않는 것은 **그대로 지킨다**:
        // 그쪽은 답을 안 기다려 출력이 다음 턴의 응답으로 밀린다.
        // 칸이 없어진 지금도 누가 그 자리를 채우면 같은 결함이 난다.
        assert!(
            !body.contains("\"prompt\":"),
            "첫 메시지를 spawn 에 실었다 — 그 답은 다음 턴으로 밀린다"
        );
    }
}

#[cfg(test)]
mod send_worker_tests {
    ///  **전송이 plugin 루프를 막지 않는다**(0.1.23).
    ///
    /// # 왜 소스를 읽어 단언하나
    ///
    /// 워커는 스레드라 시험이 그 순서를 못 본다. 그런데 이 배선이
    /// 빠지면 증상이 **"두 번째 메시지가 사라진다"** 로 나타나고, 그것은
    /// 화면을 봐야만 드러난다. 배선 자체를 값으로 고정한다.
    ///
    /// ⚠️ `send_input` 은 모델이 답을 다 낼 때까지 안 돌아온다
    /// (`client.rs` 의 `TURN_TIMEOUT`). 그것을 navigation 핸들러에서
    /// 직접 부르면 60초 뒤 호스트가 plugin 을 죽인다.
    #[test]
    fn the_navigation_handler_never_blocks_on_send_input() {
        let src = crate::test_support::wiring_source();
        let prod = src.split("mod send_worker_tests").next().unwrap_or(src);
        // ⚠️ **전송이 `send_turn` 으로 빠졌다**(0.1.51) — 시작 폼의 첫
        // 메시지도 같은 길을 써야 해서다. 그래서 **두 자리를 본다**:
        // ①핸들러가 그것을 부르는가 ②그 함수가 워커에 넘기는가.
        // ①이 없으면 ②가 옳아도 아무 일도 안 일어난다(배선 가드).
        let nav = prod
            .split("fn navigate(")
            .nth(1)
            .expect("navigation 핸들러가 없다");
        let nav_body = nav.split("\n    pub(crate) fn ").next().unwrap_or(nav);
        assert!(
            nav_body.contains("self.send_turn("),
            "navigation 핸들러가 전송을 안 부른다 — 보낸 말이 아무 데도 안 간다"
        );

        let turn = prod
            .split("fn send_turn")
            .nth(1)
            .expect("send_turn 이 없다");
        let body = turn.split("\n    fn ").next().unwrap_or(turn);

        //  **동기 전송이 없어야 한다.**
        assert!(
            !body.contains("crate::test_support::send("),
            "navigation 핸들러가 동기 전송을 부른다 — 그 동안 두 번째 메시지가 유실된다"
        );
        // 스레드로 넘기고, 답은 정규 경로로 되던진다.
        assert!(
            body.contains("std::thread::Builder::new()"),
            "전송을 워커에 안 넘긴다"
        );
        assert!(
            body.contains(
                r#"self_invoke(
                    "agent_session.turn_done""#
            ) || body.contains(r#""agent_session.turn_done""#),
            "답을 되던지는 경로가 없다 — 화면이 영영 대기로 남는다"
        );
        // 보낸 말은 **즉시** 화면에 올린다(워커를 기다리지 않는다).
        assert!(body.contains("begin_turn"), "보낸 말을 즉시 안 올린다");
    }

    ///  **되던진 답을 받는 자리가 있다.** 워커가 불러도 라우팅이 없으면
    /// 그 답은 버려지고 화면은 대기로 남는다.
    #[test]
    fn the_answer_has_somewhere_to_land() {
        let src = crate::test_support::wiring_source();
        let prod = src.split("mod send_worker_tests").next().unwrap_or(src);
        assert!(
            prod.contains(r#""agent_session.turn_done" =>"#),
            "turn_done 라우팅이 없다"
        );
        assert!(prod.contains("finish_turn"), "받은 답으로 턴을 안 닫는다");
    }
}

#[cfg(test)]
mod brief_tests {
    use crate::application::start::session_brief;

    /// **세 가지를 다 담는다** — surface·세션·agent.
    ///
    /// 하나라도 빠지면 에이전트가 자기 자리를 못 가리킨다.
    #[test]
    fn the_brief_tells_the_session_where_it_is() {
        let b = session_brief(7, "sess-abc", "cc", Some("/w/proj"));
        assert!(b.contains("surface id: 7"), "surface id 가 없다:\n{b}");
        assert!(b.contains("sess-abc"), "세션 id 가 없다:\n{b}");
        assert!(b.contains("cc"), "agent 이름이 없다:\n{b}");
        assert!(b.contains("/w/proj"), "작업 폴더가 없다:\n{b}");
    }

    /// 폴더가 없으면 **그 줄을 안 적는다** — 빈 값을 적으면 에이전트가
    /// 빈 경로를 실제 경로로 읽는다.
    #[test]
    fn a_missing_cwd_leaves_no_empty_line() {
        let b = session_brief(7, "sess-abc", "cc", None);
        let label = crate::domain::i18n::t1("sup.brief.cwd", "");
        assert!(!b.contains(label.trim_end()), "없는 폴더를 적었다:\n{b}");
    }

    /// **확신할 수 없는 값은 적지 않는다.**
    ///
    /// 바이너리 경로·포트를 박아 주면 에이전트가 그것을 믿고 쓰다
    /// 실패한다 — 실측(2026-10-01)으로 PATH 의 `tasty` 가 7월 빌드라
    /// 명령 이름이 달랐다. 대신 `tasty --help` 로 **물어보게** 한다.
    #[test]
    fn the_brief_does_not_hardcode_a_binary_path_or_port() {
        let b = session_brief(7, "sess-abc", "cc", Some("/w/proj"));
        for bad in ["target/debug", "target/release", "127.0.0.1", ".port"] {
            assert!(
                !b.contains(bad),
                "확신할 수 없는 값 `{bad}` 를 적었다 — 틀리면 그대로 믿는다:\n{b}"
            );
        }
        // 대신 물어보는 길을 준다.
        assert!(b.contains("tasty list surfaces"), "확인하는 길이 없다");
    }

    /// **새 대화 탭을 만드는 명령을 알려 준다.**
    ///
    /// `agent_session` 은 plugin 종류라 `new tab --help` 의 `--type` 목록에 없다. 실측(2026-11-10)으로
    /// 에이전트가 `--type` 없이 셸 탭을 만들고 `start` 로 세션을 묶어, 탭이 대화 탭이 아닌 셸로 남았다.
    #[test]
    fn the_brief_names_the_tab_kind_for_a_new_chat_tab() {
        let b = session_brief(7, "sess-abc", "cc", Some("/w/proj"));
        assert!(
            b.contains("tasty new tab --pane") && b.contains("--type agent_session"),
            "대화 탭을 만드는 명령이 없다:\n{b}"
        );
    }

    /// **만들어 두고 안 보내면 아무 일도 없다.**
    ///
    /// 이 저장소에서 *"배선 ≠ 존재"* 로 여러 번 밟았다 — 함수가 있어도
    /// 호출부가 없으면 화면에서는 안 고쳐진 것과 같다.
    #[test]
    fn the_brief_is_actually_sent_on_start() {
        // 시작 흐름은 `start.rs` 에 있다 — 앞에 붙여 아래 `split` 이 그 파일을 자르지 않게 한다.
        let src = &format!(
            "{}{}",
            include_str!("application/start.rs"),
            crate::test_support::wiring_source()
        );
        let prod = src.split("mod brief_tests").next().unwrap_or(src);
        let start = prod
            .split("fn start_session_for")
            .nth(1)
            .expect("시작 경로가 없다");
        let body: String = start
            .split("fn begin_started")
            .next()
            .unwrap_or(start)
            .to_string();
        assert!(
            body.contains("session_brief("),
            "맥락을 만들지 않는다 — 세션이 자기 자리를 모른다"
        );
        //  시작 경로에서 곧장 보내면 턴이 끝날 때까지 plugin 루프가 멈춘다.
        assert!(
            !body.contains(".send_input("),
            "시작 경로가 plugin 루프에서 send_input 을 부른다 — 다른 탭이 모두 멈춘다"
        );
        let begin = prod
            .split("fn begin_started")
            .nth(1)
            .expect("첫 턴을 보내는 자리가 없다");
        let begin: String = begin.chars().take(2500).collect();
        assert!(
            begin.contains("thread::Builder") && begin.contains("send_input(&session, &brief)"),
            "맥락을 만들고 **워커로 안 보낸다** — 만든 것만으로는 아무 일도 없다"
        );
        // 두 시작 경로(탭 만들기 · 시작 폼)가 모두 그 자리를 부른다.
        assert_eq!(
            prod.matches("self.begin_started(").count(),
            2,
            "시작 경로 중 첫 턴을 안 보내는 쪽이 있다"
        );
        //  `spawn` 의 `prompt` 에 실으면 답이 다음 턴으로 밀린다.
        let spawn_opts = body
            .split("SpawnOpts {")
            .nth(1)
            .expect("SpawnOpts 가 없다")
            .chars()
            .take(400)
            .collect::<String>();
        assert!(
            spawn_opts.contains("prompt: None"),
            "맥락을 spawn 의 prompt 에 실었다 — 출력이 다음 턴으로 밀린다"
        );
    }

    /// 안내에 **답하지 말라**고 적는다 — 안 적으면 첫 턴이 이 문면에
    /// 대한 답으로 차고, 사용자는 자기가 안 보낸 말의 답을 본다.
    #[test]
    fn the_brief_says_it_needs_no_reply() {
        let b = session_brief(7, "sess-abc", "cc", None);
        assert!(b.contains("답하지 않아도"), "답하지 말라는 말이 없다:\n{b}");
    }
}

#[cfg(test)]
mod delete_wiring_tests {
    use super::*;

    /// **탭을 닫으면 세션을 재운다** — 죽이지도, 그대로 두지도 않는다.
    ///
    /// 종전에는 `destroy_surface` 를 **구현하지 않아**(SDK 기본은 빈
    /// 몸통) 탭을 닫아도 세션이 `running` 으로 남았다(사용자 보고
    /// 2026-10-02). 훅은 있었고 구현이 없었다 — "수단이 없다" 와
    /// "안 쓰기로 했다" 는 다른데, 그 자리가 조용했다.
    #[test]
    fn closing_a_tab_puts_its_session_to_sleep() {
        let src = crate::test_support::wiring_source();
        let prod = src.split("mod delete_wiring_tests").next().unwrap_or(src);
        let body = prod
            .split("fn close_surface")
            .nth(1)
            .expect("탭 닫힘을 안 듣는다 — 세션이 계속 돈다");
        let head: String = body.chars().take(900).collect();
        assert!(head.contains(".stop(&id)"), "닫아도 세션을 안 재운다");
        //  **`delete` 면 안 된다** — 좌표가 사라져 Ctrl+Shift+T 복원이
        // 빈 화면이 된다(실측: `stopped` 는 send 하면 자동 resume 되고
        // 대화도 이어진다).
        assert!(
            !head.contains(".delete("),
            "닫을 때 지운다 — 되살려도 대화가 없다"
        );
        // 실패를 삼키면 "왜 살아 있나" 를 다시 겪는다.
        assert!(head.contains("tracing::warn!"), "재우기 실패가 조용하다");
    }

    /// 되돌릴 수 없는 조작이 **배선돼 있고, 한 번 묻나**(2026-09-29 개정).
    ///
    /// #  이 가드는 뒤집혔다 — 근거가 바뀌었다
    ///
    /// 2026-09-28 판은 *"화면에 삭제 단추가 있으면 red"* 였다. 그때는
    /// 누르면 **즉시** 지워졌으므로, 실수로 누르는 것이 곧 좌표 소실이었다.
    ///
    /// 0.1.18 에 **확인 단계**가 생겼다(`parse_delete` 의 `confirmed`).
    /// 첫 클릭은 그 행을 *"지울까? [지운다] [취소]"* 로 바꿀 뿐 아무것도
    /// 지우지 않고, 실제 삭제는 `ok=1` 이 붙은 두 번째 링크만 낸다.
    ///
    /// ⇒ 그래서 이제 묻는 것이 다르다: *"단추가 있나"* 가 아니라
    /// **"확인 없이 지울 수 있나"** 다.
    ///
    /// **단추 금지로 되돌리지 않는다.** 그 규칙은 확인 단계가 없다는
    /// 전제 위에 섰고, 전제가 사라지면 규칙도 근거를 잃는다. 다만 확인을
    /// 없애면 **이 가드가 다시 red 가 되어야 한다** — 아래가 그 축이다.
    #[test]
    fn delete_is_reachable_and_never_fires_without_a_confirmation() {
        let src = crate::test_support::wiring_source();
        let prod = src.split("mod delete_wiring_tests").next().unwrap_or(src);
        let code: Vec<&str> = prod
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect();

        assert!(
            code.iter()
                .any(|l| l.contains(r#""agent_session.delete" =>"#)),
            "IPC 라우팅이 없다 — 핸들러가 있어도 부를 수 없다"
        );
        assert!(
            include_str!("adapters/driving/ipc.rs")
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .any(|l| l.contains("sup.delete(")),
            "핸들러가 delete 를 안 부른다"
        );

        // **확인 없는 요청은 되돌려보낸다.** 이 두 줄이 사라지면 첫
        // 클릭이 곧 삭제가 된다.
        let nav = prod
            .split("fn navigate(")
            .nth(1)
            .expect("navigation 핸들러가 없다");
        let del_arm = nav
            .split("parse_delete_many")
            .nth(1)
            .expect("삭제 갈래가 배선되지 않았다");
        let head: String = del_arm.chars().take(400).collect();
        assert!(
            head.contains("if !confirmed") && head.contains("return"),
            "확인 없이 삭제가 나간다 — 첫 클릭으로 세션이 사라진다"
        );
    }

    /// **일괄 삭제는 하나가 실패해도 나머지를 보낸다**(`broadcast` 와
    /// 같은 축).
    ///
    /// 중간에 멈추면 사용자는 *"어디까지 지워졌나"* 를 알 수 없다 —
    /// 되돌릴 수 없는 조작이라 그 모름이 특히 나쁘다.
    #[test]
    fn a_bulk_delete_does_not_stop_at_the_first_failure() {
        let src = crate::test_support::wiring_source();
        let prod = src.split("mod delete_wiring_tests").next().unwrap_or(src);
        let nav = prod
            .split("fn navigate(")
            .nth(1)
            .expect("navigation 핸들러가 없다");
        let del_arm = nav
            .split("parse_delete_many")
            .nth(1)
            .expect("삭제 갈래가 배선되지 않았다");
        let head: String = del_arm.chars().take(1200).collect();
        // 고른 것을 **전부** 넘겨야 한다 — 첫 건만 넘기면 나머지가 남는다.
        assert!(
            head.contains("delete_sessions(") && head.contains("&ids"),
            "고른 것을 전부 안 넘긴다 — 나머지가 남는다"
        );
    }

    /// 일괄 삭제가 **첫 실패에서 멈추지 않는다** — 가짜로 실제 호출을 센다.
    ///
    /// 소스 문자열이 아니라 **동작**을 잰다: 가운데가 실패해도 뒤가
    /// 호출돼야 한다.
    #[test]
    fn a_bulk_delete_keeps_going_after_a_failure() {
        use crate::ports::supervisor::fake::FakeSupervisor;
        let sup = FakeSupervisor::new();
        sup.fail_delete_for("b");
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let (done, failed) = delete_sessions(&sup, &ids);
        assert_eq!(done, 2, "실패 뒤가 안 지워졌다 — 첫 실패에서 멈췄다");
        assert_eq!(failed, 1, "실패를 안 셌다");
        assert_eq!(
            sup.deleted(),
            vec!["a", "b", "c"],
            "세 건을 다 시도하지 않았다"
        );
    }

    /// 빈 `session_id` 는 **요청 결함**이지 대상 없음이 아니다.
    #[test]
    fn an_empty_session_id_is_refused_before_the_daemon_sees_it() {
        use crate::ports::supervisor::fake::FakeSupervisor;
        let sup = FakeSupervisor::default();
        for bad in [
            serde_json::json!({}),
            serde_json::json!({ "session_id": "   " }),
        ] {
            assert!(
                super::handle_delete(&sup, &bad).is_err(),
                "빈 id 가 통과했다 — 데몬까지 가서 엉뚱한 오류가 된다: {bad}"
            );
        }
        // 멀쩡한 것은 통과한다(양성 대조).
        let ok = super::handle_delete(&sup, &serde_json::json!({ "session_id": "s1" }));
        assert!(ok.is_ok(), "정상 id 가 거절됐다: {ok:?}");
    }
}

#[cfg(test)]
mod adopt_tests {
    //! 시작 화면의 세션 id 칸(이어받기) — 기록 대조와 작업 폴더.
    use super::*;
    use crate::ports::supervisor::fake::FakeSupervisor;

    const CONVO: &str = "6d1d8650-fea4-4b93-8291-a73ba3b23d8b";

    fn host() -> application::surface_link::tests_support::FakeHost {
        application::surface_link::tests_support::FakeHost::new()
            .with_surfaces(vec![json!({ "id": 1 })])
    }

    fn row(id: &str, state: &str, resumable: bool) -> Value {
        json!({ "id": id, "agent_session_id": CONVO, "state": state, "resumable": resumable })
    }

    fn adopt(input: &str) -> Value {
        json!({ "params": { "adopt": input } })
    }

    /// 끊긴 기록이 있으면 **새로 띄우지 않고** 그 supervisor id 를 탭에 잇는다.
    #[test]
    fn a_disconnected_record_is_tied_instead_of_spawning() {
        let sup = FakeSupervisor::new();
        sup.rows.borrow_mut().push(row("f2e706b0", "stopped", true));
        let host = host();

        let got = crate::test_support::TestPlugin::default().start_session_for(
            &host,
            &sup,
            1,
            "dev",
            &adopt(CONVO),
        );

        assert_eq!(got.map(|s| s.id).as_deref(), Ok("f2e706b0"));
        assert_eq!(
            application::surface_link::session_of_surface(&host, 1)
                .ok()
                .as_deref(),
            Some("f2e706b0"),
            "기존 기록을 탭에 안 이었다"
        );
        assert!(
            !sup.methods().iter().any(|m| m == "spawn"),
            "같은 대화에 기록을 또 만들었다: {:?}",
            sup.methods()
        );
    }

    /// 연결된 기록이 있으면 **새로 띄우지 않고** 그 기록에 합류한다 — 같은 에이전트 세션 id 는
    /// 같은 supervisor 세션이다.
    #[test]
    fn a_connected_record_is_joined_without_spawning() {
        let sup = FakeSupervisor::new();
        sup.rows
            .borrow_mut()
            .push(row("f2e706b0", "running", false));
        let host = host();

        let got = crate::test_support::TestPlugin::default().start_session_for(
            &host,
            &sup,
            1,
            "dev",
            &adopt(CONVO),
        );

        assert_eq!(got.map(|s| s.id).as_deref(), Ok("f2e706b0"));
        assert_eq!(
            application::surface_link::session_of_surface(&host, 1)
                .ok()
                .as_deref(),
            Some("f2e706b0"),
            "연결된 기록을 탭에 안 이었다"
        );
        assert!(
            !sup.methods().iter().any(|m| m == "spawn"),
            "연결된 대화에 프로세스를 또 띄웠다: {:?}",
            sup.methods()
        );
    }

    /// 기존 기록을 다시 잇다가 못 이으면 **그 기록을 멈추지 않는다** — 우리가 띄운 것이 아니다.
    #[test]
    fn a_reused_record_is_not_stopped_when_tying_fails() {
        let sup = FakeSupervisor::new();
        sup.rows.borrow_mut().push(row("f2e706b0", "stopped", true));
        let host = application::surface_link::tests_support::FakeHost::write_only_failure();

        let got = crate::test_support::TestPlugin::default().start_session_for(
            &host,
            &sup,
            1,
            "dev",
            &adopt(CONVO),
        );

        assert!(got.is_err(), "못 이었는데 성공했다");
        assert!(
            !sup.methods().iter().any(|m| m == "stop"),
            "남의 기록을 멈췄다: {:?}",
            sup.methods()
        );
    }

    /// 데몬이 기존 기록을 되살려 돌려줬으면(`reused`) 못 이어도 그 기록을 멈추지 않는다 —
    /// 우리 목록 대조가 못 본 기록(그 사이에 생긴 것 등)도 데몬은 본다.
    #[test]
    fn a_session_the_daemon_reused_is_not_stopped_when_tying_fails() {
        let sup = FakeSupervisor::new();
        sup.spawn_reused.set(true);
        let host = application::surface_link::tests_support::FakeHost::write_only_failure();

        let got = crate::test_support::TestPlugin::default().start_session_for(
            &host,
            &sup,
            1,
            "dev",
            &adopt(CONVO),
        );

        assert!(got.is_err(), "못 이었는데 성공했다");
        assert!(
            !sup.methods().iter().any(|m| m == "stop"),
            "데몬이 되살린 남의 기록을 멈췄다: {:?}",
            sup.methods()
        );
    }

    /// 기록이 없으면 이어받되, 작업 폴더를 안 정했으면 **그 대화가 시작된 폴더**를 준다.
    #[test]
    fn a_new_adoption_runs_where_the_conversation_began() {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("sup-adopt-{}-{n}", std::process::id()));
        let began = root.join("began");
        let project = root.join("config/projects/-began");
        std::fs::create_dir_all(&began).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join(format!("{CONVO}.jsonl")),
            format!("{}\n", json!({ "cwd": began.to_string_lossy() })),
        )
        .unwrap();
        let sup = FakeSupervisor::new();
        sup.dev_env.borrow_mut().insert(
            "CLAUDE_CONFIG_DIR".into(),
            root.join("config").to_string_lossy().into_owned(),
        );
        let host = host();

        let got = crate::test_support::TestPlugin::default().start_session_for(
            &host,
            &sup,
            1,
            "dev",
            &adopt(CONVO),
        );

        assert!(got.as_ref().err().is_none(), "{:?}", got.err());
        let calls = sup.calls.borrow().clone();
        assert!(
            calls.contains(&format!("spawn:dev:{}", began.to_string_lossy())),
            "대화가 시작된 폴더를 안 줬다: {calls:?}"
        );
        assert!(calls.contains(&format!("adopt:{CONVO}")), "{calls:?}");
        // 폼에서 폴더를 정했으면 그것이 이긴다.
        let sup2 = FakeSupervisor::new();
        let host2 = self::host();
        let params = json!({ "params": { "adopt": CONVO, "cwd": "/tmp" } });
        // 결과는 안 본다 — 여기서 재는 것은 데몬에 넘긴 폴더다(아래 단언).
        let _ = crate::test_support::TestPlugin::default()
            .start_session_for(&host2, &sup2, 1, "dev", &params);
        assert!(sup2.calls.borrow().contains(&"spawn:dev:/tmp".to_string()));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 목록이 주기마다 에이전트 세션 id 를 대화에 옮겨 적는다(상태줄이 그것을 보인다).
    #[test]
    fn the_poll_notes_the_agent_session_id() {
        let mut p = crate::test_support::TestPlugin::default();
        p.convos.insert(
            1,
            crate::domain::convo::Convo {
                session_id: Some("f2e706b0".into()),
                ..Default::default()
            },
        );
        let host = host();
        let rows = json!({ "rows": [ { "id": "f2e706b0", "agent_session_id": CONVO } ] });

        p.activity.note_agent_sessions(&host, &mut p.convos, &rows);

        assert_eq!(p.convos[&1].agent_session_id.as_deref(), Some(CONVO));
    }
}

#[cfg(test)]
mod agent_session_wiring_tests {
    /// 폴러의 주기 처리가 에이전트 세션 id 를 옮겨 적는가 — 함수만 시험하면 그것을 부르는
    /// 자리가 빠져도 초록이다(변이로 확인했다). 핸들러 갈래에서 부르는 것을 고정한다.
    #[test]
    fn the_poll_arm_notes_agent_sessions() {
        let src = crate::test_support::wiring_source();
        let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
        let arm = prod
            .split("fn activity_polled(")
            .nth(1)
            .expect("폴러 갈래가 없다")
            .split("\n    }\n")
            .next()
            .unwrap_or_default();
        assert!(
            arm.contains(".note_agent_sessions("),
            "폴러 갈래가 에이전트 세션 id 를 안 옮겨 적는다"
        );
    }
}
