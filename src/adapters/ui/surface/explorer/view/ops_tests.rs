//! 파일 작업 표시의 결과 문구·수명 시험.

use super::*;
use crate::app::explorer_files::job::{Failure, Resend};
use std::time::Duration;

fn report(kind: OpKind, total: usize, done: usize) -> Report {
    Report {
        kind,
        dest: Some(PathBuf::from("/tmp/Documents")),
        total,
        done,
        undo: (0..done)
            .map(|i| UndoStep::Created(PathBuf::from(format!("/tmp/Documents/{i}")), None))
            .collect(),
        failed: Vec::new(),
        skipped: Vec::new(),
        cancelled: false,
        trash_unavailable: false,
        leftovers: Vec::new(),
        unprocessed: Vec::new(),
        staging_left: Vec::new(),
        resend: None,
    }
}

/// 원본이 남은 이동 항목. 카드 시험은 파일 상태를 보지 않으므로 없는 경로로 만든다.
fn leftover(path: &str) -> Leftover {
    Leftover::before_remove(Path::new(path), Path::new("/tmp/Documents/copy"))
}

fn labels(text: &CardText) -> Vec<String> {
    card_actions(text).into_iter().map(|(l, _, _)| l).collect()
}

fn card(report: Report) -> ResultCard {
    ResultCard {
        id: 1,
        report,
        undo_of: None,
        expires: None,
    }
}

fn failure(path: &str, reason: Reason) -> Failure {
    Failure {
        path: PathBuf::from(path),
        reason,
    }
}

#[test]
fn a_finished_copy_offers_undo_and_goes_away_on_time() {
    let r = report(OpKind::Copy, 2, 2);
    assert!(result_is_timed(&r));
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Success);
    assert_eq!(
        text.title,
        t_fmt2("explorer.result.copied", "2", "Documents")
    );
    assert!(text.undo);
    assert!(text.retry.is_empty());
    assert!(text.lines.is_empty());
}

#[test]
fn a_partial_move_stays_and_retries_only_what_can_run_again() {
    let mut r = report(OpKind::Move, 4, 2);
    r.failed = vec![
        failure("/a", Reason::Os("Permission denied".into())),
        failure("/b", Reason::SourceNotRemoved("busy".into())),
    ];
    r.leftovers = vec![leftover("/b")];
    r.skipped = vec![PathBuf::from("/c")];
    assert!(!result_is_timed(&r));
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_args("explorer.result.partial_move", &["2", "4", "1"])
    );
    assert_eq!(text.retry, [PathBuf::from("/a"), PathBuf::from("/c")]);
    // 원본이 남은 /b 는 다시 옮기지 않고 원본 삭제만 다시 하며, Retry 의 수에 함께 센다.
    assert_eq!(text.leftovers, [leftover("/b")]);
    assert_eq!(labels(&text)[0], t_fmt("explorer.result.retry", "3"));
    assert!(!text.undo, "undo is only offered when everything finished");
    assert_eq!(text.lines.len(), 3);
    assert!(text.more.is_none());
}

#[test]
fn more_than_three_problems_are_summarised() {
    let mut r = report(OpKind::Copy, 5, 0);
    r.failed = (0..5)
        .map(|i| failure(&format!("/f{i}"), Reason::Os("No space left".into())))
        .collect();
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Error);
    assert_eq!(text.title, t_fmt("explorer.result.failed_copy", "5"));
    assert_eq!(text.lines.len(), MAX_RESULT_LINES);
    assert_eq!(text.more, Some(t_fmt("explorer.result.more", "2")));
}

#[test]
fn trash_that_is_not_available_says_nothing_was_deleted() {
    let mut r = report(OpKind::Trash, 1, 0);
    r.dest = None;
    r.failed = vec![failure("/x", Reason::Os("no trash".into()))];
    r.trash_unavailable = true;
    assert!(!result_is_timed(&r));
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Error);
    assert_eq!(text.title, t("explorer.trash.unavailable"));
}

#[test]
fn a_cancelled_job_retries_what_failed_or_was_not_done_but_never_undoes() {
    let mut r = report(OpKind::Copy, 10, 3);
    r.cancelled = true;
    r.failed = vec![failure("/a", Reason::Os("Permission denied".into()))];
    r.unprocessed = vec![PathBuf::from("/e"), PathBuf::from("/f")];
    // 고를 일이 남은 카드라 시간이 지나도 사라지지 않는다.
    assert!(!result_is_timed(&r));
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Info);
    assert_eq!(
        text.title,
        t_fmt2("explorer.result.cancelled_copy", "3", "10")
    );
    assert!(!text.undo);
    // 끝난 세 항목은 다시 하지 않는다.
    assert_eq!(
        text.retry,
        [
            PathBuf::from("/a"),
            PathBuf::from("/e"),
            PathBuf::from("/f")
        ]
    );
    assert_eq!(labels(&text)[0], t_fmt("explorer.result.retry", "3"));
}

#[test]
fn a_cancelled_job_with_nothing_left_goes_away_on_time_without_retry() {
    let mut r = report(OpKind::Copy, 2, 2);
    r.cancelled = true;
    assert!(result_is_timed(&r));
    let text = card_text(&card(r));
    assert!(text.retry.is_empty());
    assert!(!text.undo);
}

/// 취소한 이동의 남은 원본도 Retry 로 원본 삭제만 다시 하고, 그 삭제는 사본 검사를 거친다.
#[test]
fn a_cancelled_move_still_retries_its_leftovers_through_the_copy_check() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("a.txt");
    std::fs::write(&source, "original").expect("write");
    let copy = dir.path().join("dst").join("a.txt");
    let mut r = report(OpKind::Move, 3, 1);
    r.undo.clear();
    r.cancelled = true;
    r.failed = vec![Failure {
        path: source.clone(),
        reason: Reason::SourceNotRemoved("Permission denied".into()),
    }];
    r.leftovers = vec![Leftover::before_remove(&source, &copy)];
    r.unprocessed = vec![dir.path().join("b.txt")];
    assert!(!result_is_timed(&r));
    let text = card_text(&card(r));
    assert_eq!(text.retry, [dir.path().join("b.txt")]);
    assert_eq!(text.leftovers.len(), 1);
    assert_eq!(labels(&text)[0], t_fmt("explorer.result.retry", "2"));

    // 사본이 없으므로 Retry 의 원본 삭제는 원본을 남긴다.
    let shared = crate::app::explorer_files::job::Shared::fixed(
        crate::app::explorer_files::job::Choice::KeepBoth,
    );
    let retried = crate::app::explorer_files::job::leftover::run_remove_leftovers(
        &shared,
        None,
        &text.leftovers,
    );
    assert_eq!(
        std::fs::read_to_string(&source).expect("source"),
        "original"
    );
    assert_eq!(retried.failed.len(), 1);
    assert_eq!(retried.failed[0].reason, Reason::CopyMissing);
}

#[test]
fn an_undo_result_lists_what_could_not_be_put_back() {
    let mut r = report(OpKind::Undo, 2, 1);
    r.undo.clear();
    r.failed = vec![failure("/a", Reason::NewerThere)];
    let mut c = card(r);
    c.undo_of = Some(UndoOf::Move);
    let text = card_text(&c);
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_count("explorer.result.undo_partial_move", 1, &["1"])
    );
    assert_eq!(text.lines[0].1, t("explorer.result.newer_there"));
    assert!(!text.undo);
    assert!(text.retry.is_empty());
}

/// 이름 변경·새 항목을 되돌린 카드는 수 없이 결과만 말하고, 이유는 그 작업에 맞춘 문구다.
#[test]
fn undoing_a_rename_or_new_item_names_that_operation() {
    let mut done = report(OpKind::Undo, 1, 1);
    done.undo.clear();
    for (of, key) in [
        (UndoOf::Rename, "explorer.result.undo_done_rename"),
        (UndoOf::NewFolder, "explorer.result.undo_done_new_folder"),
        (UndoOf::NewFile, "explorer.result.undo_done_new_file"),
    ] {
        let mut c = card(done.clone());
        c.undo_of = Some(of);
        let text = card_text(&c);
        assert_eq!(
            (text.kind, text.title),
            (ToastKind::Success, t(key).to_owned())
        );
    }

    let mut failed = report(OpKind::Undo, 1, 0);
    failed.undo.clear();
    for (of, reason, title, line) in [
        (
            UndoOf::Rename,
            Reason::ChangedAfterMove,
            "explorer.result.undo_failed_rename",
            "explorer.result.changed_after_rename",
        ),
        (
            UndoOf::Rename,
            Reason::NewerThere,
            "explorer.result.undo_failed_rename",
            "explorer.result.name_taken",
        ),
        (
            UndoOf::NewFolder,
            Reason::ChangedSince,
            "explorer.result.undo_failed_new_folder",
            "explorer.result.changed_after_create",
        ),
        (
            UndoOf::NewFile,
            Reason::ChangedSince,
            "explorer.result.undo_failed_new_file",
            "explorer.result.changed_after_create",
        ),
    ] {
        let mut r = failed.clone();
        r.failed = vec![failure("/a", reason)];
        let mut c = card(r);
        c.undo_of = Some(of);
        let text = card_text(&c);
        assert_eq!(
            (text.kind, text.title),
            (ToastKind::Warning, t(title).to_owned())
        );
        assert_eq!(text.lines[0].1, t(line));
        assert!(!text.undo && text.retry.is_empty());
    }
}

#[test]
fn timed_cards_expire_and_others_stay() {
    let mut ops = OpsState::default();
    let now = Instant::now();
    ops.push_result(report(OpKind::Copy, 1, 1), None, Some(now));
    ops.push_result(report(OpKind::Copy, 1, 0), None, None);
    assert_eq!(ops.next_expiry(), Some(now));
    assert!(ops.expire(now + Duration::from_millis(1)));
    assert_eq!(ops.result_count(), 1);
    assert_eq!(ops.next_expiry(), None);
}

#[test]
fn a_move_that_left_originals_stays_without_undo_and_retries_the_delete() {
    let mut r = report(OpKind::Move, 2, 2);
    r.undo.truncate(1);
    r.failed = vec![failure(
        "/a",
        Reason::SourceNotRemoved("Permission denied".into()),
    )];
    r.leftovers = vec![leftover("/a")];
    assert!(
        !result_is_timed(&r),
        "a left original must not vanish on a timer"
    );
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_count("explorer.result.source_left_move", 1, &["2", "2", "1"])
    );
    assert!(!text.undo);
    assert!(text.retry.is_empty(), "the move itself is not sent again");
    assert_eq!(text.leftovers, [leftover("/a")]);
    assert_eq!(
        labels(&text),
        [
            t_fmt("explorer.result.retry", "1"),
            t("explorer.result.copy_paths").to_owned()
        ]
    );
    assert_eq!(
        text.lines[0].1,
        t_fmt("explorer.result.source_not_removed", "Permission denied")
    );
}

#[test]
fn an_undo_result_names_a_copy_kept_because_it_changed() {
    let mut r = report(OpKind::Undo, 1, 0);
    r.undo.clear();
    r.failed = vec![failure("/a", Reason::ChangedSince)];
    let mut c = card(r);
    c.undo_of = Some(UndoOf::Copy);
    let text = card_text(&c);
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_count("explorer.result.undo_kept_copy", 1, &["1"])
    );
    assert_eq!(text.lines[0].1, t("explorer.result.changed_kept"));
}

/// 지우지 못한 사본과 바뀌어 남긴 사본이 섞이면 지우지 못한 수가 제목이 되고, 남긴 사본도 줄에 남는다.
#[test]
fn a_mixed_undo_result_titles_the_items_that_could_not_be_removed() {
    let mut r = report(OpKind::Undo, 3, 1);
    r.undo.clear();
    r.failed = vec![
        failure("/locked", Reason::Os("in use".into())),
        failure("/a", Reason::ChangedSince),
        failure("/b", Reason::ChangedSince),
    ];
    let mut c = card(r);
    c.undo_of = Some(UndoOf::Copy);
    let text = card_text(&c);
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_count("explorer.result.undo_partial_copy", 1, &["1"])
    );
    assert_eq!(text.lines.len(), 3);
    assert_eq!(text.lines[1].1, t("explorer.result.changed_kept"));
}

/// 다시 지울 때 사본이 없어 원본을 남긴 항목은 그 사유를 보인다.
#[test]
fn a_retried_delete_names_an_original_kept_because_the_copy_is_gone() {
    let mut r = report(OpKind::Move, 1, 0);
    r.undo.clear();
    r.failed = vec![failure("/a", Reason::CopyMissing)];
    let text = card_text(&card(r));
    assert_eq!(text.lines[0].1, t("explorer.result.copy_missing"));
    // 남긴 원본은 다시 지우지 않는다. Retry 는 이동 자체를 다시 요청한다.
    assert!(text.leftovers.is_empty());
    assert_eq!(text.retry, [PathBuf::from("/a")]);
}

/// 폴더 원본에서 사본에 없는 항목을 남겼으면 원본이 남은 경고로 보이고, 원본 삭제만 다시 할 수 있다.
#[test]
fn a_folder_original_with_items_kept_stays_a_source_left_card() {
    let mut r = report(OpKind::Move, 1, 1);
    r.undo.clear();
    r.failed = vec![failure("/a", Reason::KeptNotInCopy(2))];
    r.leftovers = vec![leftover("/a")];
    assert!(!result_is_timed(&r));
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_count("explorer.result.source_left_move", 1, &["1", "1", "1"])
    );
    assert_eq!(
        text.lines[0].1,
        t_fmt("explorer.result.kept_not_in_copy", "2")
    );
    assert!(text.retry.is_empty(), "the folder is not moved again");
    assert_eq!(labels(&text)[0], t_fmt("explorer.result.retry", "1"));
    assert!(!text.undo);
}

/// 원본 삭제 다시 하기를 취소하면 남은 원본 카드로 돌아가 Retry 가 다시 있다.
#[test]
fn a_cancelled_delete_retry_returns_to_the_source_left_card() {
    let mut r = report(OpKind::Move, 3, 3);
    r.undo.clear();
    r.failed = vec![
        failure("/a", Reason::RemoveCancelled),
        failure("/b", Reason::RemoveCancelled),
    ];
    r.leftovers = vec![leftover("/a"), leftover("/b")];
    assert!(!result_is_timed(&r));
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_count("explorer.result.source_left_move", 2, &["3", "3", "2"])
    );
    assert_eq!(text.lines[0].1, t("explorer.result.remove_cancelled"));
    assert_eq!(labels(&text)[0], t_fmt("explorer.result.retry", "2"));
    assert!(!text.undo);
}

/// 사본이 원본과 같은 파일이라 남긴 원본도 원본이 남은 경고 카드에 사유와 Retry 로 보인다.
#[test]
fn a_source_kept_as_the_same_file_shows_why_and_retry() {
    let mut r = report(OpKind::Move, 1, 1);
    r.undo.clear();
    r.failed = vec![failure("/a", Reason::SameAsCopy)];
    r.leftovers = vec![leftover("/a")];
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(text.lines[0].1, t("explorer.result.same_as_copy"));
    assert_eq!(labels(&text)[0], t_fmt("explorer.result.retry", "1"));
}

/// 원격 연결이 끊겨 결과를 모르는 카드는 시간이 지나도 남고 대기열 보기를 붙이지 않는다.
#[test]
fn a_lost_connection_card_stays_until_dismissed() {
    let mut ops = OpsState::default();
    let now = Instant::now();
    ops.push_lost("title".into(), "body".into());
    assert_eq!(ops.lost(), [("title".to_string(), "body".to_string())]);
    assert_eq!(ops.next_expiry(), None);
    assert!(!ops.expire(now + Duration::from_secs(3600)));
    assert_eq!(ops.lost().len(), 1);
    let kind = notice::NoticeKind::Lost {
        title: String::new(),
        body: String::new(),
    };
    assert!(!notice::offers_show_queue(&kind, &ops));
}

/// 받지 않은 요청 카드는 표준 시간 뒤 사라지고, 대기열 보기는 이 칸에 실행 중 작업이 있을 때만 붙는다.
#[test]
fn a_refused_request_card_expires_and_offers_the_queue_only_while_running() {
    let mut ops = OpsState::default();
    let now = Instant::now();
    ops.push_refused(Refused::QueueFull, now + Duration::from_secs(2));
    ops.push_refused(Refused::QueueFull, now + Duration::from_secs(3));
    ops.push_refused(Refused::TooLarge, now + Duration::from_secs(1));
    assert_eq!(ops.refused(), [Refused::QueueFull, Refused::TooLarge]);
    assert_eq!(ops.next_expiry(), Some(now + Duration::from_secs(1)));
    assert!(!notice::offers_show_queue(
        &notice::NoticeKind::Refused(Refused::QueueFull),
        &ops
    ));
    ops.running = Some(Running {
        shared: Arc::new(Shared::fixed(
            crate::app::explorer_files::job::Choice::KeepBoth,
        )),
        kind: OpKind::Copy,
        dest: None,
        total: 1,
        in_history: false,
    });
    assert!(notice::offers_show_queue(
        &notice::NoticeKind::Refused(Refused::QueueFull),
        &ops
    ));
    assert!(!notice::offers_show_queue(
        &notice::NoticeKind::Refused(Refused::TooLarge),
        &ops
    ));
    assert!(ops.expire(now + Duration::from_millis(1500)));
    assert_eq!(ops.refused(), [Refused::QueueFull]);
    assert!(ops.expire(now + Duration::from_secs(3)));
    assert!(ops.refused().is_empty());
}

fn snapshot(
    done: usize,
    total: usize,
    removing: bool,
) -> crate::app::explorer_files::job::Snapshot {
    crate::app::explorer_files::job::Snapshot {
        files: None,
        items_done: done,
        items_total: total,
        bytes_done: 0,
        bytes_total: UNKNOWN_BYTES,
        current: "archive.zip".into(),
        removing,
    }
}

/// 원본 지우기는 이동 문구를 빌리지 않는다. 비교하는 동안은 Checking, 지우는 동안은 Removing originals.
#[test]
fn removing_originals_has_its_own_progress_queue_and_success_words() {
    assert_eq!(
        progress_text(OpKind::RemoveOriginals, &snapshot(2, 12, false)),
        t_args("explorer.progress.checking", &["3", "12", "archive.zip"])
    );
    assert_eq!(
        progress_text(OpKind::RemoveOriginals, &snapshot(4, 12, true)),
        t_args(
            "explorer.progress.removing_originals",
            &["5", "12", "archive.zip"]
        )
    );
    assert_eq!(
        queue_title(OpKind::RemoveOriginals, 12, Some(Path::new("/dest"))),
        t_fmt("explorer.queue.remove_originals", "12")
    );
    let mut r = report(OpKind::RemoveOriginals, 12, 12);
    r.undo.clear();
    assert!(result_is_timed(&r));
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Success);
    assert_eq!(text.title, t_fmt("explorer.result.removed_originals", "12"));
    assert!(labels(&text).is_empty(), "no Undo, nothing to retry");

    // 하나만 지웠으면 en 은 단수 변형을 쓴다. 다른 내장 언어는 한 문자열이다.
    let mut one = report(OpKind::RemoveOriginals, 1, 1);
    one.undo.clear();
    let title = card_text(&card(one)).title;
    assert_eq!(
        title,
        t_count("explorer.result.removed_originals", 1, &["1"])
    );
    assert_ne!(title, t_fmt("explorer.result.removed_originals", "1"));
    assert_eq!(
        queue_title(OpKind::RemoveOriginals, 1, None),
        t_count("explorer.queue.remove_originals", 1, &["1"])
    );
    assert_ne!(
        queue_title(OpKind::RemoveOriginals, 1, None),
        t_fmt("explorer.queue.remove_originals", "1"),
        "the English catalog has a singular for one"
    );
}

/// 원본 지우기에서 다시 남은 원본은 원본이 남은 경고 카드로 돌아가고 Retry 는 같은 종류로 보낸다.
#[test]
fn originals_kept_again_return_to_the_source_left_card() {
    let mut r = report(OpKind::RemoveOriginals, 2, 2);
    r.undo.clear();
    r.failed = vec![failure("/a", Reason::SameAsCopy)];
    r.leftovers = vec![leftover("/a")];
    let text = card_text(&card(r));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_count("explorer.result.source_left_move", 1, &["2", "2", "1"])
    );
    assert_eq!(labels(&text)[0], t_fmt("explorer.result.retry", "1"));
}

/// 한 누름의 두 작업은 둘 다 끝나야 카드 하나가 된다. 수는 합치고 원본이 남았으면 경고 카드다.
#[test]
fn one_retry_press_shows_one_card_when_both_jobs_end() {
    let mut ops = OpsState::default();
    let press = ops.expect_press(vec![7, 8], Duration::from_secs(5));
    let mut removed = report(OpKind::RemoveOriginals, 2, 2);
    removed.undo.clear();
    removed.failed = vec![failure("/a", Reason::SameAsCopy)];
    removed.leftovers = vec![leftover("/a")];
    // 되돌릴 수 있는 이동이 먼저 끝나도 합친 카드에는 Undo 가 없다.
    let moved = report(OpKind::Move, 3, 3);
    assert!(!moved.undo.is_empty());
    assert!(
        ops.arrive(press, 8, moved).is_none(),
        "the removal is still running"
    );
    let merged = ops.arrive(press, 7, removed).expect("both ended");
    assert_eq!(merged.kind, OpKind::Move);
    assert_eq!((merged.done, merged.total), (5, 5));
    assert!(merged.undo.is_empty(), "part of the press can't be undone");
    let text = card_text(&card(merged));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_count("explorer.result.source_left_move", 1, &["5", "5", "1"])
    );
    assert_eq!(labels(&text)[0], t_fmt("explorer.result.retry", "1"));
}

/// 묶음의 요청이 시작 전에 빠지면 남은 작업의 결과만으로 카드를 낸다. 취소는 실패를 가리지 않는다.
#[test]
fn a_press_without_its_dropped_request_still_ends_and_failures_win_over_cancel() {
    let mut ops = OpsState::default();
    let press = ops.expect_press(vec![1, 2], Duration::from_secs(5));
    ops.forget_request(2);
    assert_eq!(ops.results.len(), 0, "1 has not ended");
    let mut cancelled = report(OpKind::Move, 2, 1);
    cancelled.cancelled = true;
    let alone = ops
        .arrive(press, 1, cancelled)
        .expect("nothing else to wait for");
    assert!(alone.cancelled);

    let press = ops.expect_press(vec![3, 4], Duration::from_secs(5));
    let mut cancelled = report(OpKind::Move, 2, 1);
    cancelled.cancelled = true;
    assert!(ops.arrive(press, 3, cancelled).is_none());
    let mut kept = report(OpKind::RemoveOriginals, 1, 1);
    kept.failed = vec![failure("/b", Reason::RemoveCancelled)];
    kept.leftovers = vec![leftover("/b")];
    let merged = ops.arrive(press, 4, kept).expect("both ended");
    assert!(!merged.cancelled);
    assert_eq!(card_text(&card(merged)).leftovers.len(), 1);
}

/// 묶음의 남은 결과만으로 낸 카드도 누를 때의 표준 시간 뒤 사라진다. 실패가 있으면 닫을 때까지 남는다.
#[test]
fn a_card_left_by_a_dropped_request_follows_the_standard_time() {
    let life = Duration::from_secs(5);
    let mut ops = OpsState::default();
    let press = ops.expect_press(vec![1, 2], life);
    assert!(
        ops.arrive(press, 1, report(OpKind::RemoveOriginals, 3, 3))
            .is_none()
    );
    let before = Instant::now();
    ops.forget_request(2);
    assert_eq!(ops.results.len(), 1);
    let at = ops.next_expiry().expect("a finished result is timed");
    assert!(at >= before + life && at <= Instant::now() + life);
    assert!(ops.expire(at));
    assert!(ops.results.is_empty());

    let press = ops.expect_press(vec![3, 4], life);
    let mut kept = report(OpKind::RemoveOriginals, 1, 1);
    kept.failed = vec![failure("/b", Reason::SameAsCopy)];
    assert!(ops.arrive(press, 3, kept).is_none());
    ops.forget_request(4);
    assert_eq!(ops.results.len(), 1);
    assert_eq!(ops.next_expiry(), None, "a failure stays until dismissed");
}

/// 원격 서버가 줄여 보낸 결과의 Retry 는 남은 실패 목록이 아니라 처음 보낸 목록을 그대로 다시 보낸다.
/// 한 누름으로 합친 결과에서도 줄인 쪽은 보낸 목록, 나머지는 자기 목록을 쓴다.
#[test]
fn a_truncated_remote_result_retries_what_was_sent() {
    let mut cut = report(OpKind::Copy, 3, 0);
    cut.undo.clear();
    cut.failed = vec![failure("/srv/a", Reason::SameAsCopy)];
    cut.skipped = vec!["/srv/b".into()];
    cut.resend = Some(Box::new(Resend {
        paths: vec!["/srv/a".into(), "/srv/b".into(), "/srv/c".into()],
        leftovers: vec![leftover("/srv/d")],
    }));
    let text = card_text(&card(cut.clone()));
    assert_eq!(text.retry, cut.resend.as_ref().unwrap().paths);
    assert_eq!(text.leftovers, vec![leftover("/srv/d")]);

    let mut ops = OpsState::default();
    let press = ops.expect_press(vec![1, 2], Duration::from_secs(5));
    let mut whole = report(OpKind::Copy, 1, 0);
    whole.undo.clear();
    whole.skipped = vec!["/w/e".into()];
    assert!(ops.arrive(press, 1, cut).is_none());
    let merged = ops.arrive(press, 2, whole).expect("both ended");
    let text = card_text(&card(merged));
    let paths: Vec<PathBuf> = ["/srv/a", "/srv/b", "/srv/c", "/w/e"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    assert_eq!(text.retry, paths);
    assert_eq!(text.leftovers, vec![leftover("/srv/d")]);
}

/// 원본 지우기 뒤에 도착한 다시 옮기기가 취소로 하지 못한 항목은 합친 카드에도 남는다. 카드는
/// 저절로 사라지지 않고 그 항목을 Retry 한다.
#[test]
fn a_merged_card_keeps_the_items_the_later_job_did_not_reach() {
    let mut ops = OpsState::default();
    let press = ops.expect_press(vec![7, 8], Duration::from_secs(5));
    let mut removed = report(OpKind::RemoveOriginals, 1, 1);
    removed.undo.clear();
    let mut moved = report(OpKind::Move, 3, 1);
    moved.cancelled = true;
    moved.unprocessed = vec!["/w/b".into(), "/w/c".into()];
    assert!(ops.arrive(press, 7, removed).is_none());
    let merged = ops.arrive(press, 8, moved).expect("both ended");
    assert_eq!(
        merged.unprocessed,
        vec![PathBuf::from("/w/b"), PathBuf::from("/w/c")]
    );
    assert!(!result_is_timed(&merged));
    let text = card_text(&card(merged));
    assert_eq!(
        text.retry,
        vec![PathBuf::from("/w/b"), PathBuf::from("/w/c")]
    );
}

#[test]
fn counting_has_no_bar_and_transfer_uses_files_and_zero_or_unknown_byte_fallback() {
    use crate::app::explorer_files::job::FileProgress;
    crate::i18n::init("en");
    let mut snap = snapshot(0, 1, false);
    snap.files = Some(FileProgress {
        done: 0,
        total: 48210,
        counting: true,
    });
    assert_eq!(
        progress_text(OpKind::Copy, &snap),
        "Counting items to copy · 48,210 found"
    );
    assert_eq!(progress_fraction(&snap), None);
    snap.files.as_mut().unwrap().total = 1;
    assert_eq!(
        progress_text(OpKind::Move, &snap),
        "Counting item to move · 1 found"
    );
    snap.files = Some(FileProgress {
        done: 1203,
        total: 200000,
        counting: false,
    });
    snap.current = "file-01204.txt".into();
    assert_eq!(
        progress_text(OpKind::Copy, &snap),
        "Copying 1,204 of 200,000 · file-01204.txt"
    );
    for bytes in [0, UNKNOWN_BYTES] {
        snap.bytes_total = bytes;
        assert_eq!(progress_fraction(&snap), Some(1203.0 / 200000.0));
    }
    snap.bytes_total = 100;
    snap.bytes_done = 25;
    assert_eq!(progress_fraction(&snap), Some(0.25));
    snap.files.as_mut().unwrap().counting = true;
    assert_eq!(progress_fraction(&snap), None);
    snap.files = None;
    snap.bytes_total = UNKNOWN_BYTES;
    assert_eq!(
        progress_fraction(&snap),
        None,
        "legacy non-transfer progress stays indeterminate"
    );
}

fn staging(path: &str, error: &str) -> Failure {
    failure(path, Reason::Os(error.into()))
}

fn staging_line(path: &str, error: &str) -> (String, String) {
    (
        home_tilde(Path::new(path)),
        t_fmt("explorer.result.staging_left", error),
    )
}

/// 남은 임시 폴더는 고정 문구(OS 오류를 괄호에) 줄로 보이고 카드는 Warning 으로 올라 닫을 때까지 남는다.
/// 항목 결과는 바꾸지 않으므로 제목·Undo 는 그대로이고 Retry 는 없다. Copy paths 는 폴더 경로를 담는다.
#[test]
fn a_staging_folder_left_behind_raises_the_card_and_lists_its_path() {
    let mut r = report(OpKind::Copy, 2, 2);
    r.staging_left = vec![staging("/tmp/Documents/.tasty-copy-x", "Permission denied")];
    assert!(!result_is_timed(&r));
    let text = card_text(&card(r.clone()));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_fmt2("explorer.result.copied", "2", "Documents")
    );
    assert_eq!(
        text.lines,
        [staging_line(
            "/tmp/Documents/.tasty-copy-x",
            "Permission denied"
        )]
    );
    assert!(text.undo && text.retry.is_empty() && text.leftovers.is_empty());
    assert_eq!(
        labels(&text),
        [
            t("explorer.result.copy_paths").to_owned(),
            t("explorer.result.undo").to_owned()
        ]
    );
    assert_eq!(all_paths(&r), "/tmp/Documents/.tasty-copy-x");
}

/// 취소(Info)와 Undo 결과(Success)도 Warning 으로 오르고, 실패만 있는 카드(Error)는 Error 로 남는다.
#[test]
fn a_staging_folder_raises_info_and_success_but_keeps_danger() {
    let left = || {
        vec![staging(
            "/tmp/Documents/.tasty-copy-q",
            "Directory not empty",
        )]
    };
    let mut cancelled = report(OpKind::Copy, 40, 12);
    cancelled.cancelled = true;
    cancelled.unprocessed = (12..40)
        .map(|i| PathBuf::from(format!("/tmp/src/{i}")))
        .collect();
    cancelled.staging_left = left();
    let text = card_text(&card(cancelled));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.title,
        t_fmt2("explorer.result.cancelled_copy", "12", "40")
    );
    // Retry 는 하지 않은 28개만 센다. 남은 폴더는 다시 할 대상이 아니다.
    assert_eq!(text.retry.len(), 28);

    let mut undone = report(OpKind::Undo, 1, 1);
    undone.staging_left = left();
    let mut c = card(undone);
    c.undo_of = Some(UndoOf::Copy);
    let text = card_text(&c);
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(text.title, t("explorer.result.undo_done_copy"));
    assert_eq!(labels(&text), [t("explorer.result.copy_paths").to_owned()]);

    let mut failed = report(OpKind::Copy, 2, 0);
    failed.failed = vec![
        failure("/tmp/a", Reason::Os("No space left on device".into())),
        failure("/tmp/b", Reason::Os("No space left on device".into())),
    ];
    failed.staging_left = left();
    assert_eq!(card_text(&card(failed)).kind, ToastKind::Error);
}

/// 남은 폴더 줄은 항목 줄 뒤에 오고 "and n more" 에 함께 센다. Retry 는 실패 항목만 센다.
#[test]
fn a_staging_folder_line_comes_after_the_item_lines_and_counts_toward_more() {
    let mut r = report(OpKind::Copy, 3, 2);
    r.failed = vec![failure(
        "/tmp/src/a",
        Reason::Os("Permission denied".into()),
    )];
    r.staging_left = vec![staging(
        "/tmp/Documents/.tasty-copy-p",
        "Device or resource busy",
    )];
    let text = card_text(&card(r.clone()));
    assert_eq!(text.kind, ToastKind::Warning);
    assert_eq!(
        text.lines.last(),
        Some(&staging_line(
            "/tmp/Documents/.tasty-copy-p",
            "Device or resource busy"
        ))
    );
    assert_eq!(text.retry, [PathBuf::from("/tmp/src/a")]);

    r.failed = (0..3)
        .map(|i| {
            failure(
                &format!("/tmp/src/{i}"),
                Reason::Os("Permission denied".into()),
            )
        })
        .collect();
    let text = card_text(&card(r));
    assert_eq!(text.lines.len(), MAX_RESULT_LINES);
    assert!(text.lines.iter().all(|(_, why)| why == "Permission denied"));
    assert_eq!(text.more, Some(t_fmt("explorer.result.more", "1")));
    assert_eq!(text.retry.len(), 3);
}
