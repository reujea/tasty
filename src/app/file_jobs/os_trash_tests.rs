//! 휴지통 호출의 결과가 보고로 이어지는 계약. 대역으로 재는 시험은 모든 OS 에서 돌고, 실제 구현으로 재는
//! 시험은 Linux 의 격리한 자식 프로세스에서만 돈다.

use super::*;

fn a_file(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    std::fs::write(&path, name).expect("write");
    path
}

fn copy_of(path: &Path) -> UndoStep {
    UndoStep::Created(path.to_path_buf(), modified_at(path))
}

/// 휴지통이 거절한 되돌리기는 성공으로 세지 않고 이유를 남기며, 파일을 그대로 둔다.
#[test]
fn an_undo_the_trash_refuses_keeps_the_copy_and_says_why() {
    let root = tempfile::tempdir().expect("tempdir");
    let copy = a_file(root.path(), "b.txt");
    let shared = Shared::fixed(Choice::KeepBoth);
    shared.refuse_trash("no trash here");
    let undone = run_undo(&shared, &[copy_of(&copy)]);
    assert_eq!(undone.done, 0);
    assert_eq!(undone.failed.len(), 1);
    assert!(
        matches!(&undone.failed[0].reason, Reason::Os(why) if why.contains("no trash here")),
        "{:?}",
        undone.failed
    );
    assert!(copy.exists());
}

/// 휴지통이 받지 않은 항목은 영구 삭제로 대신하지 않고, 결과 카드가 알릴 수 있게 표시한다.
#[test]
fn a_refused_trash_deletes_nothing_and_is_marked_unavailable() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = a_file(root.path(), "a.txt");
    let shared = Shared::fixed(Choice::KeepBoth);
    shared.refuse_trash("no trash here");
    let report = run_trash(&shared, std::slice::from_ref(&path));
    assert_eq!((report.done, report.failed.len()), (0, 1));
    assert!(report.trash_unavailable);
    assert!(path.exists());
}

/// 취소는 항목 사이에서 듣는다. 처리한 항목은 끝난 것으로 세고, 남은 항목에는 휴지통을 부르지 않는다.
#[test]
fn a_cancel_between_items_leaves_the_rest_alone() {
    let root = tempfile::tempdir().expect("tempdir");
    let (a, b) = (a_file(root.path(), "a.txt"), a_file(root.path(), "b.txt"));
    let shared = Shared::fixed(Choice::KeepBoth);
    shared.cancel_after_trash.store(true, Ordering::Release);
    let report = run_trash(&shared, &[a.clone(), b.clone()]);
    assert!(report.cancelled);
    assert_eq!(report.done, 1);
    assert_eq!(report.unprocessed, std::slice::from_ref(&b));
    assert!(!a.exists() && b.exists());

    let (c, d) = (a_file(root.path(), "c.txt"), a_file(root.path(), "d.txt"));
    let shared = Shared::fixed(Choice::KeepBoth);
    shared.cancel_after_trash.store(true, Ordering::Release);
    // 되돌리기는 뒤에서부터 간다.
    let undone = run_undo(&shared, &[copy_of(&c), copy_of(&d)]);
    assert!(undone.cancelled);
    assert_eq!(undone.done, 1);
    assert!(c.exists() && !d.exists());
}

/// 시험 빌드의 대역은 한계 폴더 밖 항목을 지우지 않는다. 시험 바이너리의 위치와 무관하게 재도록
/// 임시 폴더 둘로 한계 안팎을 만든다.
#[test]
#[should_panic(expected = "to the trash from outside")]
fn the_stand_in_refuses_a_path_outside_its_limit() {
    let limit = tempfile::tempdir().expect("tempdir");
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let outside = a_file(elsewhere.path(), "a.txt");
    let limit = limit.path().canonicalize().expect("canonical");
    drop(os_trash::remove_within(&limit, &outside));
}

/// 대역의 한계는 시험의 임시 항목이 놓이는 자리 그대로다. 넓어지면 사용자 파일을 되살릴 수 없게 지울 수 있다.
#[test]
fn the_stand_in_limit_is_where_temp_items_go() {
    let item = tempfile::tempdir().expect("tempdir");
    let parent = item.path().parent().expect("its parent");
    assert_eq!(
        os_trash::temp_limit(),
        parent.canonicalize().expect("canonical")
    );
}

/// 휴지통 호출 경로의 대역도 그 한계를 쓴다. 없는 경로로 재므로 한계가 넓어져도 지워지는 것이 없다.
#[test]
#[should_panic(expected = "to the trash from outside")]
fn a_trash_call_outside_the_temp_folder_stops_the_test() {
    let limit = os_trash::temp_limit();
    let root = limit.ancestors().last().expect("the root");
    let missing = root.join("tasty-os-trash-test-no-such-entry");
    run_trash(&Shared::fixed(Choice::KeepBoth), &[missing]);
}

/// 없는 항목은 지운 것으로 세지 않는다.
#[test]
fn the_stand_in_reports_a_missing_path_like_the_trash() {
    let root = tempfile::tempdir().expect("tempdir");
    let limit = root.path().canonicalize().expect("canonical");
    let missing = root.path().join("missing");
    assert!(matches!(
        os_trash::remove_within(&limit, &missing),
        Err(trash::Error::CouldNotAccess { .. })
    ));
}

#[cfg(target_os = "linux")]
fn probe(name: &str) {
    tests::run_with_isolated_trash(&tests::probe_path(module_path!(), name));
}

/// 실제 구현: 되돌린 사본은 내용 그대로 휴지통으로 가고, 원래 자리에서 없어진다.
#[cfg(target_os = "linux")]
#[test]
fn a_real_trash_takes_an_undone_copy() {
    probe("a_real_trash_takes_an_undone_copy_probe");
}

#[cfg(target_os = "linux")]
#[test]
fn a_real_trash_takes_an_undone_copy_probe() {
    tests::in_isolated_trash(|trash| {
        let root = tempfile::tempdir().expect("tempdir");
        let copy = a_file(root.path(), "b.txt");
        let undone = run_undo(&Shared::fixed(Choice::KeepBoth), &[copy_of(&copy)]);
        assert_eq!((undone.done, undone.failed.len()), (1, 0), "{undone:?}");
        assert!(!copy.exists());
        assert_eq!(
            std::fs::read_to_string(trash.join("b.txt")).expect("the trashed copy"),
            "b.txt"
        );
    });
}

/// 실제 구현: 휴지통 폴더를 만들 수 없으면 두 작업 모두 실패를 보고하고 파일을 그대로 둔다.
#[cfg(target_os = "linux")]
#[test]
fn a_real_trash_failure_reaches_the_report() {
    probe("a_real_trash_failure_reaches_the_report_probe");
}

#[cfg(target_os = "linux")]
#[test]
fn a_real_trash_failure_reaches_the_report_probe() {
    tests::in_isolated_trash(|trash| {
        let data = trash
            .parent()
            .and_then(Path::parent)
            .expect("the data folder");
        std::fs::create_dir_all(data.parent().expect("its parent")).expect("mkdir");
        std::fs::write(data, "a file where the data folder goes").expect("write");
        let root = tempfile::tempdir().expect("tempdir");
        let copy = a_file(root.path(), "b.txt");

        let undone = run_undo(&Shared::fixed(Choice::KeepBoth), &[copy_of(&copy)]);
        assert_eq!((undone.done, undone.failed.len()), (0, 1), "{undone:?}");
        assert!(matches!(undone.failed[0].reason, Reason::Os(_)));
        assert!(copy.exists());

        let report = run_trash(
            &Shared::fixed(Choice::KeepBoth),
            std::slice::from_ref(&copy),
        );
        assert_eq!((report.done, report.failed.len()), (0, 1), "{report:?}");
        assert!(report.trash_unavailable);
        assert!(copy.exists());
    });
}
