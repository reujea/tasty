use super::*;
use crate::adapters::ui::tutorial::TutorialRuntime;
use std::time::{Duration, Instant};

fn writer() -> (
    tempfile::TempDir,
    crate::db::writer::StateWriterParts,
    StateQueue,
) {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::Db::open(&dir.path().join("state.db")).unwrap();
    let (parts, shared) = crate::db::writer::start(
        db,
        tasty_utils::batch_writer::Limits::default(),
        Arc::new(tasty_utils::batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    (dir, parts, shared.queue)
}

fn poll_until_settled(runtime: &mut TutorialRuntime) {
    let deadline = Instant::now() + Duration::from_secs(10);
    // 원래 순서는 읽기 → 기한 검사 → 거두기 → 쉼이다. 거두기는 검사를 통과한 뒤에만 돌리고,
    // 기한 뒤 관측 읽기에서는 끝난 결과만 거둔 뒤 한 번 더 읽는다.
    let mut started = false;
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::ZERO, |last| {
            if last || std::mem::replace(&mut started, true) {
                runtime.poll_progress();
                if !last {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            runtime.pending_progress_is_empty()
        }),
        "tutorial receipts never completed"
    );
}

/// state.db가 없는 실행에서는 접수부터 거절하고 View는 저장 오류를 표시한다.
#[test]
fn without_a_state_database_saves_are_refused_and_marked() {
    let progress = TutorialProgress::default();
    let mut runtime = TutorialRuntime::default();
    runtime.persistence = progress.client();
    runtime.save_progress(0);
    assert!(runtime.save_error);
    assert!(runtime.progress[0].dirty);
}

/// View의 저장이 writer를 거쳐 확정되면 버전을 받고 dirty를 지운다. 다른 View의 읽기가 그 결과를 본다.
#[test]
fn a_view_save_goes_through_the_writer_and_another_view_loads_it() {
    let (_dir, _parts, queue) = writer();
    let closed = Arc::new(AtomicBool::new(false));
    let mut first = TutorialRuntime::default();
    first.persistence = TutorialProgress::client_on(Some(queue.clone()), closed.clone());
    first.progress[0].completed = true;
    first.save_progress(0);
    poll_until_settled(&mut first);
    assert!(!first.progress[0].dirty);
    assert!(!first.save_error);
    assert_eq!(first.progress[0].row_version, 1);

    let mut second = TutorialRuntime::default();
    second.persistence = TutorialProgress::client_on(Some(queue), closed.clone());
    second.load_progress();
    poll_until_settled(&mut second);
    assert!(second.progress[0].completed);
    assert_eq!(second.progress[0].row_version, 1);

    // 종료가 시작되면 새 튜토리얼 요청만 거절한다.
    closed.store(true, Ordering::Release);
    second.save_progress(0);
    assert!(second.save_error);
}

/// state.db가 없으면 최근 파일 핸들은 서로 공유되지 않는 새 캐시다(현재 계약).
#[test]
fn without_a_state_database_each_recent_handle_starts_empty() {
    let mut recent = crate::recent_files::RecentFiles::load();
    recent.add("markdown", "/only-here.md".into());
    assert_eq!(recent.get("markdown"), vec!["/only-here.md"]);
    assert!(
        crate::recent_files::RecentFiles::load()
            .get("markdown")
            .is_empty()
    );
}
