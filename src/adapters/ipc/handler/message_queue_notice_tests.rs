//! message.send 뒤 받는 surface의 notify 로그에 쓰는 큐 도착 줄을 핸들러 경로로 확인한다.
//! 시계를 주입해 재알림 간격 전·후를 정하고, writer는 파일 대신 줄을 모은다.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::json;

use crate::core::state::queue_notice::QUEUE_NOTICE_REPEAT;
use crate::ports::clock::Clock;

struct StepClock(Mutex<Instant>);

impl StepClock {
    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }
}

impl Clock for StepClock {
    fn now_instant(&self) -> Instant {
        *self.0.lock().unwrap()
    }
    fn now_system(&self) -> SystemTime {
        SystemTime::now()
    }
    fn now_unix_millis(&self) -> i64 {
        0
    }
}

type Lines = crate::app::queue_notice_writer::RecordedLines;

fn core(clock: Arc<StepClock>) -> (crate::app::services::AppServices, Lines) {
    let mut core = super::cli_entry_tests::test_core_builder()
        .with_clock(clock)
        .build()
        .expect("test AppServices");
    let (writer, lines) = crate::app::queue_notice_writer::QueueNoticeWriter::recording();
    core.queue_notices = writer;
    (core, lines)
}

fn take(lines: &Lines) -> Vec<(u32, String)> {
    std::mem::take(&mut *lines.lock().unwrap())
}

#[test]
fn queue_arrivals_write_edge_and_repeat_lines_without_the_content() {
    let clock = Arc::new(StepClock(Mutex::new(Instant::now())));
    let (mut core, lines) = core(clock.clone());
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let to = engine
        .workspace_at(0)
        .expect("workspace")
        .all_surface_ids()
        .first()
        .copied()
        .expect("surface");
    let send = |core: &mut _, engine: &mut _, content: &str| {
        super::message::handle_message_send(
            core,
            engine,
            json!(1),
            &json!({ "to_surface_id": to, "from_surface_id": 900, "content": content }),
        )
    };

    // 빈 큐에 첫 메시지: 바로 한 줄, 내용은 없다. 응답 스키마는 그대로다.
    let resp = send(&mut core, &mut engine, "secret-x");
    let result = resp.result.expect("success");
    assert_eq!(result["to_surface_id"], json!(to));
    assert_eq!(result["from_surface_id"], json!(900));
    assert!(result["id"].is_u64());
    assert_eq!(result.as_object().unwrap().len(), 3);
    let got = take(&lines);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, to);
    assert!(got[0].1.starts_with("[queue] "), "{got:?}");
    assert!(!got[0].1.contains("secret-x"), "{got:?}");
    assert!(got[0].1.contains("900"), "{got:?}");

    // 간격 안의 추가 메시지: 줄 없음.
    clock.advance(QUEUE_NOTICE_REPEAT - Duration::from_secs(1));
    send(&mut core, &mut engine, "y");
    assert!(take(&lines).is_empty());

    // 간격이 지난 뒤: 대기 수 3이 적힌 한 줄.
    clock.advance(Duration::from_secs(1));
    send(&mut core, &mut engine, "z");
    let got = take(&lines);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].1.contains(" 3 "), "{got:?}");

    // peek은 상태를 지우지 않는다.
    super::message::handle_message_read(
        &mut core,
        &mut engine,
        json!(2),
        &json!({ "surface_id": to, "peek": true }),
    );
    send(&mut core, &mut engine, "w");
    assert!(take(&lines).is_empty());

    // 소비 읽기로 큐가 비면 다음 메시지는 간격과 무관하게 바로 알린다.
    super::message::handle_message_read(
        &mut core,
        &mut engine,
        json!(3),
        &json!({ "surface_id": to }),
    );
    send(&mut core, &mut engine, "v");
    assert_eq!(take(&lines).len(), 1);

    // clear도 같다.
    super::message::handle_message_clear(
        &mut core,
        &mut engine,
        json!(4),
        &json!({ "surface_id": to }),
    );
    send(&mut core, &mut engine, "u");
    assert_eq!(take(&lines).len(), 1);
}

#[test]
fn a_missing_surface_gets_no_line_and_the_same_response() {
    let clock = Arc::new(StepClock(Mutex::new(Instant::now())));
    let (mut core, lines) = core(clock);
    let (_state, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let resp = super::message::handle_message_send(
        &mut core,
        &mut engine,
        json!(1),
        &json!({ "to_surface_id": 99999, "from_surface_id": 1, "content": "x" }),
    );
    let result = resp.result.expect("success");
    assert_eq!(result["to_surface_id"], json!(99999));
    assert!(take(&lines).is_empty());
    assert_eq!(
        engine.message_count(99999),
        1,
        "큐 저장 동작은 바뀌지 않는다"
    );
}
