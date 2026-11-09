//! plugin 껍데기 메서드를 가짜 호스트로 잰다 — 워커를 띄우고 호스트에 되던지는 자리들.
//!
//! plugin 이 호스트를 포트(`SendHost`)로 들기 때문에 시험이 기록하는 가짜(`SharedHost`)를 넣을 수 있다.
//! 데몬은 빈 임시 표의 실물 핸들이다 — 세션이 없어 보내는 말마다 거절되고, 워커는 그 결과를 되던진다.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::adapters::driven::local::LocalSupervisor;
use crate::application::app_error::AppError;
use crate::application::plugin::SupervisorPlugin;
use crate::application::surface_link::tests_support::SharedHost;
use crate::domain::convo::Convo;
use crate::domain::convo::Turn;

type Plugin = SupervisorPlugin<SharedHost, LocalSupervisor>;

fn plugin(tag: &str) -> (Plugin, SharedHost) {
    let host = SharedHost::default();
    let mut p = Plugin::new(LocalSupervisor::empty_for_test(tag));
    p.host = Some(host.clone());
    (p, host)
}

fn linked(session: &str) -> Convo {
    Convo {
        session_id: Some(session.into()),
        ..Default::default()
    }
}

fn turn(sent: &str) -> Turn {
    Turn {
        sent: sent.into(),
        got: "답".into(),
        pending: false,
        error: None,
        at: None,
    }
}

/// 워커가 되던질 때까지 기다린다 — 스레드라 곧바로 보이지 않는다.
fn wait_invoked(host: &SharedHost, method: &str) -> Vec<Value> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let got: Vec<Value> = host
            .invoked()
            .into_iter()
            .filter(|(m, _)| m == method)
            .map(|(_, p)| p)
            .collect();
        if !got.is_empty() || Instant::now() > until {
            return got;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 때가 된 429 재전송은 사용자의 말과 같은 길로 나간다 — 턴을 열고, 워커가 그 턴의 끝을 되던진다.
#[test]
fn a_due_retry_opens_a_turn_and_the_worker_reports_its_end() {
    let (mut p, host) = plugin("retry");
    let mut c = linked("s-retry");
    c.retry.pending = Some(crate::domain::retry_429::Pending {
        text: "다시 보낼 말".into(),
        due: Instant::now() - Duration::from_secs(1),
    });
    p.convos.insert(7, c);

    p.run_pending_retries();

    let c = &p.convos[&7];
    assert!(c.retry.pending.is_none(), "예약을 꺼내지 않았다");
    let last = c.turns.last().expect("턴을 안 열었다");
    assert_eq!((last.sent.as_str(), last.pending), ("다시 보낼 말", true));
    let done = wait_invoked(&host, "agent_session.turn_done");
    assert_eq!(done.len(), 1, "워커가 끝을 안 되던졌다");
    assert_eq!(done[0]["surface_id"], 7);
    assert_eq!(done[0]["at"], c.turns.len() - 1);
    assert_eq!(done[0]["ok"], false, "없는 세션에 보낸 말이 성공했다");
}

/// 때가 안 된 재전송은 그대로 둔다 — 턴도 워커도 없다.
#[test]
fn a_retry_that_is_not_due_stays_parked() {
    let (mut p, host) = plugin("retry-wait");
    let mut c = linked("s-wait");
    c.retry.pending = Some(crate::domain::retry_429::Pending {
        text: "나중".into(),
        due: Instant::now() + Duration::from_secs(60),
    });
    p.convos.insert(7, c);

    p.run_pending_retries();

    assert!(p.convos[&7].retry.pending.is_some());
    assert!(p.convos[&7].turns.is_empty());
    assert!(host.invoked().is_empty());
}

/// 이어지지 않은 surface 의 `tell` 은 곧바로 사유로 답한다 — 기다리게 두면 호출자는 상한까지 선다.
#[test]
fn tell_later_answers_an_unlinked_surface_at_once() {
    let (mut p, _host) = plugin("tell");
    let (tx, rx) = std::sync::mpsc::channel::<Result<Value, AppError>>();
    p.tell_later(
        &json!({ "surface_id": 9, "message": "안녕" }),
        Box::new(move |r| tx.send(r).is_ok()),
    );
    let got = rx.try_recv().expect("답을 미뤘다");
    assert!(got.is_err(), "이어지지 않은 탭에 보냈다: {got:?}");
}

/// 호스트를 아직 못 받았으면(기동 전) 워커를 못 띄운다는 사유로 답한다.
#[test]
fn tell_later_without_a_host_says_so() {
    let mut p = Plugin::new(LocalSupervisor::empty_for_test("tell-nohost"));
    let (tx, rx) = std::sync::mpsc::channel::<Result<Value, AppError>>();
    p.tell_later(
        &json!({ "surface_id": 9, "message": "안녕" }),
        Box::new(move |r| tx.send(r).is_ok()),
    );
    let got = rx.try_recv().expect("답을 미뤘다");
    assert_eq!(
        got.expect_err("호스트 없이 성공했다").to_string(),
        crate::domain::i18n::t("sup.err.worker_spawn")
    );
}

/// 비우기가 끝나면 대화를 비우고 문서를 다시 싣는다. 실패면 대화는 남기고 사유를 보인다.
#[test]
fn clear_done_empties_the_tab_or_keeps_it_with_the_reason() {
    let (mut p, host) = plugin("clear");
    let mut c = linked("s-clear");
    c.turns = vec![turn("하나"), turn("둘")];
    c.clearing = true;
    p.convos.insert(7, c);

    p.clear_done(7, Ok(()));

    let c = &p.convos[&7];
    assert!(c.turns.is_empty() && !c.clearing, "대화를 안 비웠다");
    assert_eq!(c.session_id.as_deref(), Some("s-clear"), "세션까지 끊었다");
    assert!(
        host.calls()
            .iter()
            .any(|(m, p)| m == "webview.set_url" && p["surface_id"] == 7),
        "문서를 다시 안 실었다"
    );

    let (mut p, host) = plugin("clear-fail");
    let mut c = linked("s-clear");
    c.turns = vec![turn("하나")];
    c.clearing = true;
    p.convos.insert(7, c);

    p.clear_done(7, Err("거절".into()));

    let c = &p.convos[&7];
    assert_eq!(c.turns.len(), 1, "실패했는데 대화를 지웠다");
    assert!(!c.clearing);
    assert!(
        host.posted_to(7)
            .iter()
            .any(|d| d["type"] == "note.set"
                && d["text"].as_str().is_some_and(|t| t.contains("거절"))),
        "실패 사유를 안 보였다"
    );
}

/// compact 가 끝나면 진행을 거두고 다음 턴 끝에서 바닥값을 보게 한다. 실패면 사유를 남기고 멈춘다.
#[test]
fn sync_done_settles_the_compact_phase() {
    use crate::application::sync_compact::Phase;
    use crate::domain::sync_rules::Mode;
    let (mut p, _host) = plugin("sync");
    let mut c = linked("s-sync");
    c.sync.phase = Phase::Compacting { mode: Mode::Manual };
    p.convos.insert(7, c);

    p.sync_done(7, Ok(()));

    let s = &p.convos[&7].sync;
    assert!(matches!(s.phase, Phase::Idle));
    assert!(s.verify && s.failed.is_none());

    let (mut p, host) = plugin("sync-fail");
    let mut c = linked("s-sync");
    c.sync.phase = Phase::Compacting { mode: Mode::Manual };
    p.convos.insert(7, c);

    p.sync_done(7, Err("끊김".into()));

    let s = &p.convos[&7].sync;
    assert!(matches!(s.phase, Phase::Idle));
    assert!(s.failed.as_deref().is_some_and(|w| w.contains("끊김")));
    assert!(
        host.posted_to(7).iter().any(|d| d["type"] == "note.set"),
        "실패 사유를 탭에 안 보였다"
    );
}
