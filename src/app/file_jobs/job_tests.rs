//! 파일 작업 엔진 시험. 휴지통은 사용자 홈의 실제 휴지통이라 여기서 부르지 않는다.

use super::*;
use std::sync::{Arc, OnceLock, Weak};

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read a test file")
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("list a test folder")
        .map(|e| {
            e.expect("read a test entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// 질문이 올라올 때까지 기다린다.
fn wait_ask(shared: &Shared) -> Ask {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(ask) = shared.pending_ask() {
            return ask;
        }
        assert!(Instant::now() < deadline, "no conflict question arrived");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn spawn_transfer(
    shared: &Arc<Shared>,
    sources: Vec<PathBuf>,
    dest: &Path,
    cut: bool,
) -> std::thread::JoinHandle<Report> {
    let shared = Arc::clone(shared);
    let dest = dest.to_path_buf();
    std::thread::spawn(move || run_transfer(&shared, &sources, &dest, cut))
}

#[test]
fn keep_both_is_the_default_and_never_overwrites() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    std::fs::write(src.join("a.txt"), "incoming").expect("write");
    std::fs::write(dst.join("a.txt"), "existing").expect("write");

    let report = run_transfer(
        &Shared::fixed(Choice::KeepBoth),
        &[src.join("a.txt")],
        &dst,
        false,
    );

    assert_eq!(report.done, 1);
    assert!(report.failed.is_empty());
    assert_eq!(read(&dst.join("a.txt")), "existing");
    assert_eq!(read(&dst.join("a (copy).txt")), "incoming");
    assert_eq!(names(&dst), ["a (copy).txt", "a.txt"]);
    let dst = dst.canonicalize().expect("canonical");
    assert_eq!(report.undo, [UndoStep::created(dst.join("a (copy).txt"))]);
}

#[test]
fn replace_overwrites_files_but_never_folders() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(src.join("assets")).expect("mkdir");
    std::fs::create_dir_all(dst.join("assets")).expect("mkdir");
    std::fs::write(src.join("a.txt"), "incoming").expect("write");
    std::fs::write(dst.join("a.txt"), "existing").expect("write");
    std::fs::write(src.join("assets/new.txt"), "n").expect("write");
    std::fs::write(dst.join("assets/old.txt"), "o").expect("write");

    let report = run_transfer(
        &Shared::fixed(Choice::Replace),
        &[src.join("a.txt"), src.join("assets")],
        &dst,
        false,
    );

    assert_eq!(report.done, 2);
    assert_eq!(read(&dst.join("a.txt")), "incoming");
    // 폴더는 합치지도 바꾸지도 않고 Keep both 로 둔다.
    assert_eq!(names(&dst.join("assets")), ["old.txt"]);
    assert_eq!(names(&dst.join("assets (copy)")), ["new.txt"]);
    let dst = dst.canonicalize().expect("canonical");
    assert_eq!(
        report.undo,
        [
            UndoStep::Replaced(dst.join("a.txt")),
            UndoStep::created(dst.join("assets (copy)")),
        ]
    );
}

#[test]
fn apply_all_answers_the_remaining_conflicts_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(src.join(name), "incoming").expect("write");
        std::fs::write(dst.join(name), "existing").expect("write");
    }
    let shared = Arc::new(Shared::interactive(|| {}));
    let worker = spawn_transfer(
        &shared,
        vec![src.join("a.txt"), src.join("b.txt"), src.join("c.txt")],
        &dst,
        false,
    );

    let ask = wait_ask(&shared);
    assert_eq!(ask.name, "a.txt");
    assert!(!ask.folder_conflict);
    assert_eq!(ask.remaining, 2);
    shared.answer(Answer {
        choice: Choice::Skip,
        apply_all: true,
    });
    let report = worker.join().expect("worker");

    assert_eq!(report.skipped.len(), 3);
    assert_eq!(report.done, 0);
    for name in ["a.txt", "b.txt", "c.txt"] {
        assert_eq!(read(&dst.join(name)), "existing");
    }
    assert_eq!(names(&dst), ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(report.retryable().len(), 3);
}

#[test]
fn replace_for_all_still_asks_again_on_a_folder_conflict() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(src.join("assets")).expect("mkdir");
    std::fs::create_dir_all(dst.join("assets")).expect("mkdir");
    std::fs::write(src.join("a.txt"), "incoming").expect("write");
    std::fs::write(dst.join("a.txt"), "existing").expect("write");
    let shared = Arc::new(Shared::interactive(|| {}));
    let worker = spawn_transfer(
        &shared,
        vec![src.join("a.txt"), src.join("assets")],
        &dst,
        false,
    );

    assert!(!wait_ask(&shared).folder_conflict);
    shared.answer(Answer {
        choice: Choice::Replace,
        apply_all: true,
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let ask = loop {
        if let Some(ask) = shared.pending_ask().filter(|a| a.name == "assets") {
            break ask;
        }
        assert!(
            Instant::now() < deadline,
            "the folder conflict was not asked"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(ask.folder_conflict);
    shared.answer(Answer {
        choice: Choice::Skip,
        apply_all: false,
    });
    let report = worker.join().expect("worker");

    assert_eq!(read(&dst.join("a.txt")), "incoming");
    assert_eq!(report.skipped, [src.join("assets")]);
    assert_eq!(names(&dst), ["a.txt", "assets"]);
}

#[test]
fn cancel_while_waiting_for_an_answer_changes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    std::fs::write(src.join("a.txt"), "incoming").expect("write");
    std::fs::write(dst.join("a.txt"), "existing").expect("write");
    let shared = Arc::new(Shared::interactive(|| {}));
    let worker = spawn_transfer(&shared, vec![src.join("a.txt")], &dst, true);

    wait_ask(&shared);
    shared.cancel();
    let report = worker.join().expect("worker");

    assert!(report.cancelled);
    assert_eq!(report.done, 0);
    assert_eq!(read(&dst.join("a.txt")), "existing");
    assert_eq!(read(&src.join("a.txt")), "incoming");
    assert_eq!(names(&dst), ["a.txt"]);
}

/// 취소하면 멈춘 항목과 그 뒤 항목을 하지 않은 것으로 기록하고, 끝난 항목은 넣지 않는다.
#[test]
fn cancel_records_the_item_it_stopped_and_the_rest_as_not_done() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(src.join(name), name).expect("write");
    }
    std::fs::write(dst.join("b.txt"), "existing").expect("write");
    let shared = Arc::new(Shared::interactive(|| {}));
    let sources = vec![src.join("a.txt"), src.join("b.txt"), src.join("c.txt")];
    let worker = spawn_transfer(&shared, sources, &dst, false);

    wait_ask(&shared);
    shared.cancel();
    let report = worker.join().expect("worker");

    assert!(report.cancelled);
    assert_eq!(report.done, 1);
    assert_eq!(report.unprocessed, [src.join("b.txt"), src.join("c.txt")]);
    assert_eq!(report.retryable(), [src.join("b.txt"), src.join("c.txt")]);
    assert_eq!(names(&dst), ["a.txt", "b.txt"]);
    assert_eq!(read(&dst.join("b.txt")), "existing");
}

#[test]
fn cancel_during_a_copy_leaves_no_partial_entry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("big.bin");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mkdir");
    std::fs::write(&src, vec![7u8; COPY_CHUNK * 4]).expect("write");
    // 첫 진행 알림에서 취소한다. 알림은 시작 뒤 WAKE_INTERVAL 이 지나야 나가므로 그만큼 기다린 뒤 시작한다.
    let me: Arc<OnceLock<Weak<Shared>>> = Arc::default();
    let hook = Arc::clone(&me);
    let shared = Arc::new(Shared::interactive(move || {
        if let Some(shared) = hook.get().and_then(Weak::upgrade) {
            shared.cancel();
        }
    }));
    assert!(me.set(Arc::downgrade(&shared)).is_ok());
    std::thread::sleep(WAKE_INTERVAL + Duration::from_millis(50));

    let report = run_transfer(&shared, std::slice::from_ref(&src), &dst, false);

    assert!(report.cancelled);
    assert_eq!(report.done, 0);
    assert!(report.undo.is_empty());
    assert!(
        names(&dst).is_empty(),
        "staging left behind: {:?}",
        names(&dst)
    );
    assert_eq!(
        std::fs::metadata(&src).expect("source").len(),
        (COPY_CHUNK * 4) as u64
    );
}

#[test]
fn moving_into_the_same_folder_changes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.txt"), "a").expect("write");
    let report = run_transfer(
        &Shared::fixed(Choice::KeepBoth),
        &[dir.path().join("a.txt")],
        dir.path(),
        true,
    );
    assert_eq!(report.done, 1);
    assert!(report.undo.is_empty());
    assert_eq!(names(dir.path()), ["a.txt"]);
}

#[test]
fn a_folder_cannot_go_inside_itself() {
    let dir = tempfile::tempdir().expect("tempdir");
    let folder = dir.path().join("f");
    std::fs::create_dir_all(folder.join("sub")).expect("mkdir");
    for cut in [false, true] {
        let report = run_transfer(
            &Shared::fixed(Choice::KeepBoth),
            std::slice::from_ref(&folder),
            &folder.join("sub"),
            cut,
        );
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].reason, Reason::IntoItself);
        assert_eq!(names(&folder.join("sub")), Vec::<String>::new());
    }
}

#[test]
fn undo_puts_a_moved_entry_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mkdir");
    std::fs::write(dir.path().join("a.txt"), "a").expect("write");
    let moved = run_transfer(
        &Shared::fixed(Choice::KeepBoth),
        &[dir.path().join("a.txt")],
        &dst,
        true,
    );
    assert_eq!(names(&dst), ["a.txt"]);

    let undone = run_undo(&Shared::fixed(Choice::KeepBoth), &moved.undo);

    assert_eq!(undone.done, 1);
    assert!(undone.failed.is_empty());
    assert_eq!(read(&dir.path().join("a.txt")), "a");
    assert!(names(&dst).is_empty());
}

#[test]
fn undo_leaves_a_newer_entry_in_place() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mkdir");
    std::fs::write(dir.path().join("a.txt"), "old").expect("write");
    let moved = run_transfer(
        &Shared::fixed(Choice::KeepBoth),
        &[dir.path().join("a.txt")],
        &dst,
        true,
    );
    std::fs::write(dir.path().join("a.txt"), "newer").expect("write");

    let undone = run_undo(&Shared::fixed(Choice::KeepBoth), &moved.undo);

    assert_eq!(undone.done, 0);
    assert_eq!(undone.failed[0].reason, Reason::NewerThere);
    assert_eq!(read(&dir.path().join("a.txt")), "newer");
    assert_eq!(read(&dst.join("a.txt")), "old");
}

#[test]
fn undo_reports_what_it_cannot_restore() {
    let dir = tempfile::tempdir().expect("tempdir");
    let steps = [
        UndoStep::Replaced(dir.path().join("r.txt")),
        UndoStep::Created(dir.path().join("gone.txt"), None),
    ];
    let undone = run_undo(&Shared::fixed(Choice::KeepBoth), &steps);
    // 뒤에서부터 되돌린다.
    assert_eq!(
        undone.failed,
        [
            Failure {
                path: dir.path().join("gone.txt"),
                reason: Reason::Gone,
            },
            Failure {
                path: dir.path().join("r.txt"),
                reason: Reason::Replaced,
            },
        ]
    );
}

#[test]
fn retry_skips_entries_whose_copy_already_exists() {
    let mut report = Report::new(OpKind::Move, None, 3);
    report.failed = vec![
        Failure {
            path: PathBuf::from("/a"),
            reason: Reason::Os("denied".into()),
        },
        Failure {
            path: PathBuf::from("/b"),
            reason: Reason::SourceNotRemoved("busy".into()),
        },
    ];
    report.skipped = vec![PathBuf::from("/c")];
    assert_eq!(
        report.retryable(),
        [PathBuf::from("/a"), PathBuf::from("/c")]
    );
}

#[test]
fn copy_reports_bytes_and_keeps_links_as_links() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("tree");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(src.join("inner")).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    std::fs::write(src.join("inner/x.txt"), "12345").expect("write");
    #[cfg(unix)]
    std::os::unix::fs::symlink("inner/x.txt", src.join("link")).expect("symlink");
    let shared = Shared::fixed(Choice::KeepBoth);

    let report = run_transfer(&shared, std::slice::from_ref(&src), &dst, false);

    assert_eq!(report.done, 1);
    assert_eq!(read(&dst.join("tree/inner/x.txt")), "12345");
    #[cfg(unix)]
    assert!(
        dst.join("tree/link")
            .symlink_metadata()
            .expect("link")
            .file_type()
            .is_symlink()
    );
    let snap = shared.snapshot();
    assert_eq!(snap.items_done, 1);
    assert_eq!(snap.items_total, 1);
    assert_eq!(snap.bytes_done, snap.bytes_total);
}

/// 질문 없이 한 항목을 다루는 Item. 경합·원본 삭제 실패처럼 run 으로 만들기 어려운 경로를 직접 부른다.
pub(super) fn with_item<R>(
    shared: &Shared,
    dest: &Path,
    cut: bool,
    f: impl FnOnce(&mut Item<'_>) -> R,
) -> R {
    let mut sticky = None;
    let mut left = 0;
    let mut staging_left = Vec::new();
    let mut item = Item {
        shared,
        dest,
        shown_dest: dest,
        cut,
        sticky: &mut sticky,
        conflicts_left: &mut left,
        staging_left: &mut staging_left,
        base: 0,
    };
    f(&mut item)
}

/// 이름을 확인한 뒤 공개하기 전에 같은 이름이 생기면, 공개는 그것을 덮어쓰지 않고 Keep both 이름으로 간다.
#[test]
fn publishing_never_overwrites_a_name_that_appears_late() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    std::fs::write(src.join("a.txt"), "incoming").expect("write");
    std::fs::write(src.join("b.txt"), "moving").expect("write");
    // 다른 프로그램이 확인 뒤에 만든 항목.
    std::fs::write(dst.join("a.txt"), "late").expect("write");
    std::fs::write(dst.join("b.txt"), "late").expect("write");
    let shared = Shared::fixed(Choice::KeepBoth);

    let a = src.join("a.txt");
    let meta = a.symlink_metadata().expect("meta");
    let copied = with_item(&shared, &dst, false, |item| {
        item.copy(&a, &meta, dst.join("a.txt"), Publish::NoReplace)
    });
    assert!(matches!(
        copied,
        Ok(Outcome::Done(UndoStep::Created(ref p, _))) if *p == dst.join("a (copy).txt")
    ));

    let mut target = dst.join("b.txt");
    let moved = with_item(&shared, &dst, true, |item| {
        item.rename(&src.join("b.txt"), &mut target, Publish::NoReplace)
    });
    assert!(matches!(
        moved,
        Ok(Some(Outcome::Done(UndoStep::Moved { .. })))
    ));
    assert_eq!(target, dst.join("b (copy).txt"));

    assert_eq!(read(&dst.join("a.txt")), "late");
    assert_eq!(read(&dst.join("b.txt")), "late");
    assert_eq!(read(&dst.join("a (copy).txt")), "incoming");
    assert_eq!(read(&dst.join("b (copy).txt")), "moving");
}

/// 다른 디스크 이동에서 원본을 일부만 지웠으면 사본은 하나뿐인 온전한 자료다.
/// 보고는 실패로 남고 되돌리기 단계가 없어 Undo 가 사본을 휴지통으로 보내지 않는다.
#[cfg(unix)]
#[test]
fn a_partly_removed_source_keeps_the_copy_out_of_undo() {
    use std::os::unix::fs::PermissionsExt;
    // root 는 권한을 무시해 원본이 다 지워진다. 이 경우 재현할 수 없다.
    // SAFETY: geteuid 는 인자 없이 현재 프로세스의 유효 사용자 ID 만 읽는다.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let payload = dir.path().join("src").join("payload");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(payload.join("ro")).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    for name in ["a1.txt", "a2.txt", "a3.txt"] {
        std::fs::write(payload.join(name), name).expect("write");
    }
    std::fs::write(payload.join("ro").join("b.txt"), "b").expect("write");
    let lock = |mode| {
        std::fs::set_permissions(payload.join("ro"), std::fs::Permissions::from_mode(mode))
            .expect("chmod")
    };
    lock(0o555);

    let shared = Shared::fixed(Choice::KeepBoth);
    let meta = payload.symlink_metadata().expect("meta");
    let result = with_item(&shared, &dst, true, |item| {
        item.copy(&payload, &meta, dst.join("payload"), Publish::NoReplace)
    });
    let mut report = Report::new(OpKind::Move, Some(dst.clone()), 1);
    let flow = record(&mut report, &shared, &payload, result);
    lock(0o755);

    assert!(flow.is_continue());
    assert_eq!(report.done, 1);
    assert!(
        report.undo.is_empty(),
        "the only full copy must not be undone"
    );
    assert!(matches!(
        report.failed.as_slice(),
        [Failure {
            reason: Reason::SourceNotRemoved(_),
            ..
        }]
    ));
    assert!(report.retryable().is_empty());
    // 사본은 온전하다. 원본은 지우지 못한 ro 를 포함해 남는다(그 밖에 얼마나 지워졌는지는 읽는 순서에 달렸다).
    assert_eq!(
        names(&dst.join("payload")),
        ["a1.txt", "a2.txt", "a3.txt", "ro"]
    );
    assert_eq!(names(&dst.join("payload").join("ro")), ["b.txt"]);
    assert_eq!(names(&payload.join("ro")), ["b.txt"]);
    assert!(run_undo(&shared, &report.undo).failed.is_empty());
    assert_eq!(names(&dst.join("payload")).len(), 4);
}

/// 작업 뒤 수정 시각이 바뀐 사본은 사용자가 고친 것으로 보고 되돌리기에서 남긴다.
#[test]
fn undo_keeps_a_copy_changed_after_the_job() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::create_dir_all(&dst).expect("mkdir");
    std::fs::write(src.join("a.txt"), "copied").expect("write");
    let report = run_transfer(
        &Shared::fixed(Choice::KeepBoth),
        &[src.join("a.txt")],
        &dst,
        false,
    );
    let copy = dst.canonicalize().expect("canonical").join("a.txt");
    assert!(matches!(report.undo.as_slice(), [UndoStep::Created(p, Some(_))] if *p == copy));

    std::fs::write(&copy, "edited").expect("edit the copy");
    let later = SystemTime::now() + Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&copy)
        .and_then(|f| f.set_modified(later))
        .expect("touch the copy");

    let undone = run_undo(&Shared::fixed(Choice::KeepBoth), &report.undo);
    assert_eq!(
        undone.failed,
        [Failure {
            path: copy.clone(),
            reason: Reason::ChangedSince,
        }]
    );
    assert_eq!(read(&copy), "edited");
}

#[test]
fn transfer_counts_nested_files_without_changing_top_level_results() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("source");
    let nested = src.join("nested");
    let dest = root.path().join("dest");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(src.join("one.txt"), "abc").unwrap();
    std::fs::write(nested.join("two.txt"), "de").unwrap();
    std::fs::write(nested.join("empty.txt"), "").unwrap();
    let shared = Shared::fixed(Choice::KeepBoth);
    let report = run_transfer(&shared, std::slice::from_ref(&src), &dest, false);
    assert_eq!(report.done, 1);
    assert_eq!(report.undo.len(), 1);
    let snap = shared.snapshot();
    assert_eq!((snap.items_done, snap.items_total), (1, 1));
    assert_eq!(
        snap.files,
        Some(FileProgress {
            done: 3,
            total: 3,
            counting: false
        })
    );
    assert_eq!((snap.bytes_done, snap.bytes_total), (5, 5));
    assert!(matches!(
        snap.current.as_str(),
        "one.txt" | "two.txt" | "empty.txt"
    ));
    assert_eq!(
        std::fs::read(dest.join("source/nested/two.txt")).unwrap(),
        b"de"
    );

    let moved = root.path().join("moved");
    std::fs::create_dir(&moved).unwrap();
    let shared = Shared::fixed(Choice::KeepBoth);
    let report = run_transfer(&shared, std::slice::from_ref(&src), &moved, true);
    assert_eq!(report.done, 1);
    assert_eq!(shared.snapshot().files.unwrap().done, 3);
    assert!(!src.exists());
}

#[cfg(unix)]
#[test]
fn zero_byte_files_and_links_have_file_progress_without_following_targets() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("source");
    let dest = root.path().join("dest");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(src.join("empty"), "").unwrap();
    symlink(".", src.join("cycle")).unwrap();
    symlink("absent", src.join("broken")).unwrap();
    let shared = Shared::fixed(Choice::KeepBoth);
    let report = run_transfer(&shared, &[src], &dest, false);
    assert_eq!(report.done, 1);
    let snap = shared.snapshot();
    assert_eq!(snap.bytes_total, 0);
    assert_eq!(
        snap.files,
        Some(FileProgress {
            done: 3,
            total: 3,
            counting: false
        })
    );
    assert_eq!(
        std::fs::read_link(dest.join("source/cycle")).unwrap(),
        PathBuf::from(".")
    );
}

#[test]
fn the_counting_phase_is_published_before_any_transfer_and_can_be_cancelled() {
    use std::sync::Arc;
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("source");
    let dest = root.path().join("dest");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(src.join("file"), "unchanged").unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let shared: Arc<Shared> = Arc::new_cyclic(|weak: &std::sync::Weak<Shared>| {
        let weak = weak.clone();
        let seen = seen.clone();
        let dest = dest.clone();
        Shared::interactive(move || {
            let shared = weak.upgrade().unwrap();
            let snap = shared.snapshot();
            if snap.files.as_ref().is_some_and(|f| f.counting) {
                assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0);
                seen.lock().unwrap().push(snap);
                shared.cancel();
            }
        })
    });
    let report = run_transfer(&shared, std::slice::from_ref(&src), &dest, false);
    assert!(report.cancelled);
    assert_eq!(report.done, 0);
    assert_eq!(report.unprocessed, std::slice::from_ref(&src));
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(std::fs::read_dir(dest).unwrap().count(), 0);
    assert_eq!(std::fs::read(src.join("file")).unwrap(), b"unchanged");
}

#[test]
fn skipping_a_nested_source_finishes_its_file_progress_but_keeps_the_skip_result() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("source");
    let dest = root.path().join("dest");
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir_all(dest.join("source")).unwrap();
    std::fs::write(src.join("first"), "a").unwrap();
    std::fs::write(src.join("second"), "b").unwrap();
    let shared = Shared::fixed(Choice::Skip);
    let report = run_transfer(&shared, std::slice::from_ref(&src), &dest, false);
    assert_eq!(report.done, 0);
    assert_eq!(report.skipped, [src]);
    assert!(report.undo.is_empty());
    assert_eq!(
        shared.snapshot().files,
        Some(FileProgress {
            done: 2,
            total: 2,
            counting: false
        })
    );
}

/// 찾기 결과처럼 폴더와 그 안의 항목을 함께 고르면 안쪽 항목은 폴더가 이미 나른다.
/// 안쪽 항목을 따로 복사하면 목적지 맨 위에 사본이 하나 더 생기고, 이동하면 폴더를 옮긴 뒤 사라진 항목으로 실패한다.
#[test]
fn a_folder_and_its_own_entries_are_transferred_once() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("source");
    std::fs::create_dir_all(src.join("inner")).unwrap();
    std::fs::write(src.join("top.txt"), "t").unwrap();
    std::fs::write(src.join("inner/deep.txt"), "d").unwrap();
    let selected = vec![
        src.join("inner/deep.txt"),
        src.clone(),
        src.join("top.txt"),
        src.clone(),
    ];

    let copied = root.path().join("copied");
    std::fs::create_dir(&copied).unwrap();
    let shared = Shared::fixed(Choice::KeepBoth);
    let report = run_transfer(&shared, &selected, &copied, false);
    assert_eq!((report.done, report.total), (1, 1));
    assert!(report.failed.is_empty() && report.skipped.is_empty());
    assert_eq!(names(&copied), ["source"]);
    assert_eq!(read(&copied.join("source/inner/deep.txt")), "d");
    let snap = shared.snapshot();
    assert_eq!((snap.items_done, snap.items_total), (1, 1));
    assert_eq!(snap.files.map(|f| (f.done, f.total)), Some((2, 2)));

    let moved = root.path().join("moved");
    std::fs::create_dir(&moved).unwrap();
    let report = run_transfer(&Shared::fixed(Choice::KeepBoth), &selected, &moved, true);
    assert_eq!((report.done, report.total), (1, 1));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(names(&moved), ["source"]);
    assert!(!src.exists());
    assert_eq!(report.undo.len(), 1);
}

/// 링크는 따라가지 않으므로 링크 너머의 항목은 링크와 함께 골라도 따로 처리한다.
#[cfg(unix)]
#[test]
fn an_entry_reached_through_a_selected_link_is_still_transferred() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("inside.txt"), "i").unwrap();
    let link = root.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let dest = root.path().join("dest");
    std::fs::create_dir(&dest).unwrap();
    let report = run_transfer(
        &Shared::fixed(Choice::KeepBoth),
        &[link.clone(), link.join("inside.txt")],
        &dest,
        false,
    );
    assert_eq!((report.done, report.total), (2, 2));
    assert_eq!(names(&dest), ["inside.txt", "link"]);
    assert!(dest.join("link").symlink_metadata().unwrap().is_symlink());
}

/// 복사 도중 worker 를 세워 두는 주입 지점. 첫 바이트를 쓴 뒤 처음 깨우는 자리에서 `reached` 를 보내고
/// `release` 를 받을 때까지 기다린다. 그 사이 시험은 준비 중인 임시 폴더를 보거나 바꿀 수 있다.
pub(crate) struct MidCopy {
    pub shared: Arc<Shared>,
    pub reached: std::sync::mpsc::Receiver<()>,
    pub release: std::sync::mpsc::SyncSender<()>,
}

pub(crate) fn paused_mid_copy() -> MidCopy {
    let (reached_tx, reached) = std::sync::mpsc::sync_channel(1);
    let (release, release_rx) = std::sync::mpsc::sync_channel(1);
    let reached_tx = Mutex::new(Some(reached_tx));
    let release_rx = Mutex::new(release_rx);
    let slept = AtomicBool::new(false);
    let shared = Arc::new_cyclic(|weak: &Weak<Shared>| {
        let weak = weak.clone();
        Shared::interactive(move || {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            let snap = shared.snapshot();
            let counted = snap.files.as_ref().is_some_and(|f| !f.counting);
            if counted && snap.bytes_done == 0 && !slept.swap(true, Ordering::AcqRel) {
                // 다음 진행 깨움이 간격 제한을 넘도록 집계가 끝난 깨움에서 한 번 쉰다.
                std::thread::sleep(WAKE_INTERVAL + Duration::from_millis(50));
            } else if snap.bytes_done > 0
                && let Some(tx) = reached_tx.lock().unwrap().take()
            {
                tx.send(()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
            }
        })
    });
    MidCopy {
        shared,
        reached,
        release,
    }
}

/// 여러 버퍼에 걸쳐 복사되는 파일 하나를 담은 폴더.
pub(crate) fn folder_with_a_large_file(root: &Path) -> PathBuf {
    let src = root.join("source");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("large.bin"), vec![7u8; COPY_CHUNK * 3]).unwrap();
    src
}

/// 임시 폴더를 지우지 못하면 그 경로와 OS 오류를 결과에 남긴다. 버리던 정리 오류를 보이게 한다.
#[cfg(unix)]
#[test]
fn a_staging_folder_that_cannot_be_removed_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let src = folder_with_a_large_file(root.path());
    let dest = root.path().join("dest");
    std::fs::create_dir(&dest).unwrap();
    let dest = dest.canonicalize().unwrap();
    let pause = paused_mid_copy();
    let worker = spawn_transfer(&pause.shared, vec![src.clone()], &dest, false);
    pause.reached.recv_timeout(Duration::from_secs(10)).unwrap();
    let staging = std::fs::read_dir(&dest)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".tasty-copy-")
        })
        .expect("a staging folder while copying");
    let entry = staging.join("entry");
    // 안의 파일을 지울 수 없게 해 정리를 실패시킨다.
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o555)).unwrap();
    pause.shared.cancel();
    pause.release.send(()).unwrap();
    let report = worker.join().unwrap();
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(report.cancelled);
    assert_eq!(report.unprocessed, [src]);
    assert_eq!(
        report
            .staging_left
            .iter()
            .map(|f| &f.path)
            .collect::<Vec<_>>(),
        [&staging]
    );
    assert!(matches!(report.staging_left[0].reason, Reason::Os(_)));
    assert!(report.failed.is_empty());
}

/// 정리에 성공하면 아무것도 남기지 않는다.
#[test]
fn a_cancelled_copy_removes_its_staging_folder_and_reports_nothing_left() {
    let root = tempfile::tempdir().unwrap();
    let src = folder_with_a_large_file(root.path());
    let dest = root.path().join("dest");
    std::fs::create_dir(&dest).unwrap();
    let pause = paused_mid_copy();
    let worker = spawn_transfer(&pause.shared, vec![src], &dest, false);
    pause.reached.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(names(&dest).len(), 1, "the staging folder exists");
    pause.shared.cancel();
    pause.release.send(()).unwrap();
    let report = worker.join().unwrap();
    assert!(report.cancelled);
    assert!(report.staging_left.is_empty());
    assert!(names(&dest).is_empty());
}

/// 격리한 휴지통으로 probe 를 실행하는 자식 프로세스에 주는 표지. 값은 완료 표지 파일 경로다.
#[cfg(target_os = "linux")]
const TRASH_PROBE_ENV: &str = "TASTY_ISOLATED_TRASH_PROBE";
#[cfg(target_os = "linux")]
const TRASH_PROBE_DONE: &str = "isolated-trash-probe-done";

/// `probe`(크레이트 이름을 뺀 시험 경로)를 이 시험 바이너리의 자식 프로세스로 실행한다. 자식의
/// `XDG_DATA_HOME` 을 임시 폴더로 바꿔 freedesktop 휴지통이 사용자 홈이 아닌 그곳에 쌓이게 한다.
/// 환경변수는 프로세스 전역이라 같은 프로세스의 다른 시험과 섞이지 않도록 자식에서만 바꾼다.
#[cfg(target_os = "linux")]
pub(crate) fn run_with_isolated_trash(probe: &str) {
    let exe = std::env::current_exe().expect("current_exe");
    let dir = tempfile::tempdir().expect("tempdir");
    let marker = dir.path().join("probe-done");
    let output = std::process::Command::new(exe)
        .args([probe, "--exact", "--test-threads=1", "--nocapture"])
        .env("XDG_DATA_HOME", dir.path().join("data"))
        .env(TRASH_PROBE_ENV, &marker)
        .output()
        .expect("spawn probe");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "probe failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap_or_default(),
        TRASH_PROBE_DONE,
        "probe did not run to the end\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// probe 본문을 격리한 휴지통 안에서만 실행한다. 부모 시험이 띄운 자식이 아니면 아무것도 하지 않는다.
/// 본문은 그 휴지통의 `files` 폴더를 받는다.
#[cfg(target_os = "linux")]
pub(crate) fn in_isolated_trash(body: impl FnOnce(&Path)) {
    let (Some(marker), Some(data)) = (
        std::env::var_os(TRASH_PROBE_ENV),
        std::env::var_os("XDG_DATA_HOME"),
    ) else {
        return;
    };
    body(&Path::new(&data).join("Trash/files"));
    std::fs::write(marker, TRASH_PROBE_DONE).expect("write the probe marker");
}

/// [`run_with_isolated_trash`] 가 띄운 자식인가. 이때만 시험 빌드가 실제 휴지통 구현을 부른다.
#[cfg(target_os = "linux")]
pub(crate) fn in_isolated_trash_child() -> bool {
    std::env::var_os(TRASH_PROBE_ENV).is_some() && std::env::var_os("XDG_DATA_HOME").is_some()
}

/// 크레이트 이름을 뺀 이 모듈의 시험 경로.
#[cfg(target_os = "linux")]
pub(crate) fn probe_path(module: &str, name: &str) -> String {
    let module = module.split_once("::").map_or(module, |(_, rest)| rest);
    format!("{module}::{name}")
}

/// 링크를 휴지통으로 보내면 링크만 가고, 가리키던 폴더와 파일은 그대로 남는다. 대상이 없는 링크도 같다.
#[cfg(target_os = "linux")]
#[test]
fn trashing_a_link_keeps_what_it_points_to() {
    run_with_isolated_trash(&probe_path(
        module_path!(),
        "trashing_a_link_keeps_what_it_points_to_probe",
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn trashing_a_link_keeps_what_it_points_to_probe() {
    in_isolated_trash(|trash| {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("keep.txt"), "k").unwrap();
        let folder_link = root.path().join("folder-link");
        let file_link = root.path().join("file-link");
        let broken = root.path().join("broken");
        std::os::unix::fs::symlink(&real, &folder_link).unwrap();
        std::os::unix::fs::symlink(real.join("keep.txt"), &file_link).unwrap();
        std::os::unix::fs::symlink(root.path().join("missing"), &broken).unwrap();
        let shared = Shared::fixed(Choice::KeepBoth);
        let report = run_trash(
            &shared,
            &[folder_link.clone(), file_link.clone(), broken.clone()],
        );
        assert_eq!((report.done, report.total), (3, 3), "{:?}", report.failed);
        assert_eq!(names(root.path()), ["real"]);
        assert_eq!(read(&real.join("keep.txt")), "k");
        assert_eq!(names(trash), ["broken", "file-link", "folder-link"]);
        for name in ["broken", "file-link", "folder-link"] {
            let meta = trash.join(name).symlink_metadata().unwrap();
            assert!(meta.file_type().is_symlink(), "{name} stays a link");
        }
        assert_eq!(std::fs::read_link(trash.join("folder-link")).unwrap(), real);
    });
}

/// 폴더와 그 안의 항목을 함께 휴지통으로 보내면 폴더 하나만 보낸다. 안쪽 항목을 따로 보내면 폴더가
/// 이미 간 뒤라 없는 항목으로 실패한다.
#[cfg(target_os = "linux")]
#[test]
fn trashing_a_folder_with_its_own_entries_sends_the_folder_once() {
    run_with_isolated_trash(&probe_path(
        module_path!(),
        "trashing_a_folder_with_its_own_entries_sends_the_folder_once_probe",
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn trashing_a_folder_with_its_own_entries_sends_the_folder_once_probe() {
    in_isolated_trash(|trash| {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("folder");
        std::fs::create_dir_all(folder.join("inner")).unwrap();
        std::fs::write(folder.join("inner/a.txt"), "a").unwrap();
        let report = run_trash(
            &Shared::fixed(Choice::KeepBoth),
            &[
                folder.join("inner/a.txt"),
                folder.clone(),
                folder.join("inner"),
            ],
        );
        assert_eq!((report.done, report.total), (1, 1), "{:?}", report.failed);
        assert!(report.failed.is_empty());
        assert!(!folder.exists());
        assert_eq!(names(trash), ["folder"]);
        assert_eq!(read(&trash.join("folder/inner/a.txt")), "a");
    });
}
