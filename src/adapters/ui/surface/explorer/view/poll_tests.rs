use super::*;
use crate::app::local_reads::LocalReads;
use tasty_model::{ExplorerPanel, SortColumn, SortDir};

/// `done` 이 참이 될 때까지 worker 를 돌린다. 끝난 job 을 거둬 동시 실행 상한에 막히지 않게 한다.
fn run_until(
    view: &mut ExplorerView,
    owner: &mut LocalReads,
    done: impl Fn(&ExplorerView) -> bool,
) {
    loop {
        owner.reap();
        owner.drive(|requests| {
            view.poll_local_reads(requests);
            true
        });
        if done(view) {
            return;
        }
        std::thread::yield_now();
    }
}

/// 패널을 그리고 읽기·확인 결과가 모두 돌아올 때까지 기다린다. `sync` 가 목록과 함께 거는
/// 쓰기 가능 확인도 기다린다. 그 결과가 뒤에 오면 다시 그리라는 신호가 되어, 확인만의 신호를
/// 재는 시험이 worker 순서에 따라 갈린다.
fn settle(view: &mut ExplorerView, panel: &ExplorerPanel, owner: &mut LocalReads) {
    view.sync(panel, None);
    run_until(view, owner, |v| {
        v.local_query.is_none()
            && v.writable_query.is_none()
            && v.tree_queries.is_empty()
            && v.poll.check.is_none()
    });
}

/// 걸린 확인이 끝날 때까지 돌린다. 그동안 다시 그리라는 신호가 있었는지 돌려준다.
fn finish_check(view: &mut ExplorerView, owner: &mut LocalReads) -> bool {
    let mut redraw = false;
    loop {
        owner.reap();
        owner.drive(|requests| {
            redraw |= view.poll_local_reads(requests);
            true
        });
        if view.poll.check.is_none() {
            return redraw;
        }
        std::thread::yield_now();
    }
}

fn shutdown(mut owner: LocalReads) {
    while owner.poll_shutdown() != 0 {
        std::thread::yield_now();
    }
}

/// 한 프레임: 프레임 시작 뒤 이 칸을 `focused` 로 그린다.
fn frame(store: &mut ExplorerViewStore, panel: &ExplorerPanel, focused: bool, now: Instant) {
    store.begin_poll_frame();
    store.get_or_init(panel, None).poll_external(focused, now);
}

#[test]
fn a_tab_without_focus_is_never_checked() {
    let panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
    let mut store = ExplorerViewStore::default();
    let t0 = Instant::now();
    for i in 0..3 {
        frame(&mut store, &panel, false, t0 + EXTERNAL_POLL_INTERVAL * i);
        let view = store.get(1).expect("view");
        assert!(view.poll.check.is_none(), "포커스가 없으면 확인하지 않는다");
    }
    assert_eq!(store.next_poll_at(), None, "깨울 이유가 없다");
}

#[test]
fn gaining_focus_checks_at_once_and_then_every_interval() {
    let panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
    let mut store = ExplorerViewStore::default();
    let t0 = Instant::now();
    frame(&mut store, &panel, false, t0);
    store.get_mut(1).expect("view").local_query = None;

    frame(&mut store, &panel, true, t0);
    let view = store.get_mut(1).expect("view");
    assert!(
        view.poll.check.is_some(),
        "포커스를 얻은 프레임에 바로 확인한다"
    );
    view.poll.check = None;
    assert_eq!(store.next_poll_at(), Some(t0 + EXTERNAL_POLL_INTERVAL));

    frame(&mut store, &panel, true, t0 + EXTERNAL_POLL_INTERVAL);
    assert!(
        store.get(1).expect("view").poll.check.is_none(),
        "주기 확인은 프레임이 시작하지 않는다"
    );
    assert!(
        !store.start_due_checks(t0 + EXTERNAL_POLL_INTERVAL / 2),
        "주기 전에는 다시 확인하지 않는다"
    );
    assert!(
        store.start_due_checks(t0 + EXTERNAL_POLL_INTERVAL),
        "주기가 지나면 그리지 않고 확인한다"
    );
    assert!(store.get(1).expect("view").poll.check.is_some());
    assert_eq!(store.next_poll_at(), Some(t0 + EXTERNAL_POLL_INTERVAL * 2));

    // 다른 탭으로 갔다가 돌아오면 주기를 기다리지 않는다.
    store.get_mut(1).expect("view").poll.check = None;
    frame(&mut store, &panel, false, t0 + EXTERNAL_POLL_INTERVAL);
    assert_eq!(store.next_poll_at(), None);
    frame(&mut store, &panel, true, t0 + EXTERNAL_POLL_INTERVAL);
    assert!(store.get(1).expect("view").poll.check.is_some());
}

#[test]
fn only_the_focused_tab_of_the_focused_window_is_checked_and_regaining_focus_checks_at_once() {
    assert!(checks_outside_changes(true, true));
    assert!(!checks_outside_changes(true, false), "창의 다른 탭");
    assert!(
        !checks_outside_changes(false, true),
        "Tasty 가 배경 앱이거나 다른 창이 포커스"
    );
    assert!(!checks_outside_changes(false, false));

    let panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
    let mut store = ExplorerViewStore::default();
    let t0 = Instant::now();
    frame(&mut store, &panel, checks_outside_changes(true, true), t0);
    store.get_mut(1).expect("view").local_query = None;
    frame(&mut store, &panel, checks_outside_changes(true, true), t0);
    store.get_mut(1).expect("view").poll.check = None;

    // 창이 포커스를 잃은 동안에는 주기가 지나도 확인하지 않고 깨우지도 않는다.
    for i in 1..4 {
        let at = t0 + EXTERNAL_POLL_INTERVAL * i;
        frame(&mut store, &panel, checks_outside_changes(false, true), at);
        assert!(store.get(1).expect("view").poll.check.is_none());
        assert_eq!(store.next_poll_at(), None);
    }
    // 창이 포커스를 되찾은 프레임에 주기를 기다리지 않고 확인하고, 그 뒤 주기를 다시 시작한다.
    let back = t0 + EXTERNAL_POLL_INTERVAL * 3 + EXTERNAL_POLL_INTERVAL / 4;
    frame(&mut store, &panel, checks_outside_changes(true, true), back);
    assert!(store.get(1).expect("view").poll.check.is_some());
    assert_eq!(store.next_poll_at(), Some(back + EXTERNAL_POLL_INTERVAL));
}

#[test]
fn a_panel_not_drawn_this_frame_stops_waking() {
    let panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
    let mut store = ExplorerViewStore::default();
    let t0 = Instant::now();
    frame(&mut store, &panel, true, t0);
    assert!(store.next_poll_at().is_some());
    store.begin_poll_frame();
    assert_eq!(store.next_poll_at(), None, "배경 탭의 칸은 그려지지 않는다");
}

#[test]
fn a_mirror_explorer_is_not_checked() {
    let panel = ExplorerPanel::new(1, PathBuf::from("/remote/project"));
    let mut view = ExplorerView::new();
    view.sync(&panel, Some(7));
    view.poll_external(true, Instant::now());
    assert!(view.poll.check.is_none());
    assert_eq!(view.poll.next_at, None);
}

#[test]
fn an_unchanged_folder_is_not_read_again() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("a"), b"x").expect("write");
    let panel = ExplorerPanel::new(1, dir.path().into());
    let mut view = ExplorerView::new();
    let mut owner = LocalReads::default();
    settle(&mut view, &panel, &mut owner);
    assert_eq!(view.entries.len(), 1);

    view.poll_external(true, Instant::now());
    assert!(view.poll.check.is_some());
    assert!(
        !finish_check(&mut view, &mut owner),
        "바뀐 것이 없으면 다시 그리지 않는다"
    );
    assert!(
        view.local_query.is_none() && view.tree_queries.is_empty(),
        "표지가 같으면 목록을 읽지 않는다"
    );
    shutdown(owner);
}

#[test]
fn a_changed_folder_is_reread_while_the_old_list_stays() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("keep"), b"x").expect("write");
    std::fs::write(dir.path().join("gone"), b"x").expect("write");
    let panel = ExplorerPanel::new(1, dir.path().into());
    let mut view = ExplorerView::new();
    let mut owner = LocalReads::default();
    settle(&mut view, &panel, &mut owner);
    view.select_only(&dir.path().join("keep"));
    view.toggle_select(&dir.path().join("gone"));
    let generation = view.entries_gen;

    std::thread::sleep(Duration::from_millis(20));
    std::fs::remove_file(dir.path().join("gone")).expect("remove");
    std::fs::write(dir.path().join("new"), b"x").expect("write");
    view.poll_external(true, Instant::now());
    assert!(
        !finish_check(&mut view, &mut owner),
        "확인 결과만으로는 다시 그리지 않는다"
    );
    assert!(view.local_query.is_some(), "바뀐 폴더를 바로 다시 읽는다");
    assert_eq!(
        view.entries_gen, generation,
        "읽는 동안 보이던 목록을 그대로 둔다"
    );
    assert!(matches!(view.state, super::super::LoadState::Ok));
    settle(&mut view, &panel, &mut owner);
    let mut names: Vec<&str> = view.entries.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["keep", "new"]);
    assert_eq!(
        view.selected,
        std::collections::HashSet::from([dir.path().join("keep")]),
        "사라진 항목만 선택에서 빠진다"
    );
    shutdown(owner);
}

#[test]
fn only_a_changed_expanded_tree_folder_is_read_again() {
    let dir = tempfile::tempdir().expect("temp dir");
    for name in ["changed", "same"] {
        std::fs::create_dir_all(dir.path().join(name).join("old")).expect("mkdir");
    }
    let panel = ExplorerPanel::new(1, dir.path().into());
    let mut view = ExplorerView::new();
    let mut owner = LocalReads::default();
    settle(&mut view, &panel, &mut owner);
    for name in ["changed", "same"] {
        let path = dir.path().join(name);
        view.expanded.insert(path.clone());
        view.tree_children_of(&path, None);
    }
    settle(&mut view, &panel, &mut owner);

    std::thread::sleep(Duration::from_millis(20));
    let changed = dir.path().join("changed");
    std::fs::create_dir(changed.join("inner")).expect("mkdir");
    view.poll_external(true, Instant::now());
    finish_check(&mut view, &mut owner);
    assert!(view.tree_queries.contains_key(&changed));
    assert!(!view.tree_queries.contains_key(&dir.path().join("same")));
    assert!(view.local_query.is_none(), "현재 폴더는 바뀌지 않았다");
    assert_eq!(
        tree_names(&view, &changed),
        ["old"],
        "읽는 동안 보이던 하위 목록을 그대로 둔다"
    );
    settle(&mut view, &panel, &mut owner);
    assert_eq!(tree_names(&view, &changed), ["inner", "old"]);
    shutdown(owner);
}

#[test]
fn the_current_folder_in_the_tree_is_read_once_and_its_node_is_replaced_in_place() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir(dir.path().join("old")).expect("mkdir");
    let root = dir.path().to_path_buf();
    let panel = ExplorerPanel::new(1, root.clone());
    let mut view = ExplorerView::new();
    let mut owner = LocalReads::default();
    settle(&mut view, &panel, &mut owner);
    view.expanded.insert(root.clone());
    view.tree_children_of(&root, None);
    settle(&mut view, &panel, &mut owner);

    std::thread::sleep(Duration::from_millis(20));
    std::fs::create_dir(root.join("new")).expect("mkdir");
    view.poll_external(true, Instant::now());
    finish_check(&mut view, &mut owner);
    assert!(view.local_query.is_some(), "목록은 다시 읽는다");
    assert!(
        view.tree_queries.is_empty(),
        "트리의 같은 폴더는 따로 읽지 않는다"
    );
    assert_eq!(tree_names(&view, &root), ["old"]);
    settle(&mut view, &panel, &mut owner);
    assert_eq!(tree_names(&view, &root), ["new", "old"]);
    shutdown(owner);
}

#[test]
fn a_refresh_rereads_the_same_folder_while_the_old_list_and_tree_stay() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("keep"), b"x").expect("write");
    std::fs::write(root.join("gone"), b"x").expect("write");
    std::fs::create_dir_all(root.join("sub").join("old")).expect("mkdir");
    let panel = ExplorerPanel::new(1, root.clone());
    let mut view = ExplorerView::new();
    let mut owner = LocalReads::default();
    settle(&mut view, &panel, &mut owner);
    let sub = root.join("sub");
    view.expanded.insert(sub.clone());
    view.tree_children_of(&sub, None);
    settle(&mut view, &panel, &mut owner);
    view.select_only(&root.join("keep"));
    view.toggle_select(&root.join("gone"));
    let generation = view.entries_gen;

    std::fs::remove_file(root.join("gone")).expect("remove");
    std::fs::create_dir(sub.join("inner")).expect("mkdir");
    view.request_refresh();
    view.sync(&panel, None);
    assert!(view.local_query.is_some(), "같은 폴더를 다시 읽는다");
    assert!(
        view.tree_queries.contains_key(&sub),
        "읽어 둔 트리 폴더도 다시 읽는다"
    );
    assert_eq!(
        view.state,
        super::super::LoadState::Ok,
        "Loading 화면을 보이지 않는다"
    );
    assert_eq!(
        view.entries_gen, generation,
        "읽는 동안 보이던 목록을 그대로 둔다"
    );
    assert_eq!(
        tree_names(&view, &sub),
        ["old"],
        "읽는 동안 보이던 하위 목록을 그대로 둔다"
    );

    settle(&mut view, &panel, &mut owner);
    let mut names: Vec<&str> = view.entries.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["keep", "sub"]);
    assert_eq!(tree_names(&view, &sub), ["inner", "old"]);
    assert_eq!(
        view.selected,
        std::collections::HashSet::from([root.join("keep")]),
        "사라진 항목만 선택에서 빠진다"
    );
    shutdown(owner);
}

#[test]
fn a_refresh_of_a_failed_folder_shows_loading_and_a_file_operation_reload_empties_the_list() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("a"), b"x").expect("write");
    let panel = ExplorerPanel::new(1, root.clone());
    let mut view = ExplorerView::new();
    let mut owner = LocalReads::default();
    settle(&mut view, &panel, &mut owner);

    // 파일 작업 뒤 다시 읽기는 사용자 새로고침이 아니므로 지금처럼 목록을 비우고 읽는다.
    view.request_reload();
    view.sync(&panel, None);
    assert_eq!(view.state, super::super::LoadState::Loading);
    assert!(view.entries.is_empty());
    settle(&mut view, &panel, &mut owner);

    view.state = super::super::LoadState::Error("gone".into());
    view.request_refresh();
    view.sync(&panel, None);
    assert_eq!(
        view.state,
        super::super::LoadState::Loading,
        "오류 화면의 다시 시도는 Loading 으로 바꾼다"
    );
    settle(&mut view, &panel, &mut owner);
    assert_eq!(view.state, super::super::LoadState::Ok);
    shutdown(owner);
}

fn tree_names(view: &ExplorerView, dir: &Path) -> Vec<String> {
    view.tree_children[dir]
        .iter()
        .map(|e| e.name.clone())
        .collect()
}

/// 현재 폴더와 `n` 개의 펼친 트리 폴더(하위 목록을 읽어 둔 상태)를 가진 칸. 파일시스템은 읽지 않는다.
fn view_with_tree(n: usize) -> (ExplorerView, PathBuf, Vec<PathBuf>) {
    let root = PathBuf::from("/tmp/tasty-poll-cap");
    let mut view = ExplorerView::new();
    view.loaded = Some((root.clone(), SortColumn::Name, SortDir::Asc));
    let dirs: Vec<PathBuf> = (0..n).map(|i| root.join(format!("d{i:03}"))).collect();
    for dir in &dirs {
        view.expanded.insert(dir.clone());
        view.tree_children.insert(dir.clone(), Vec::new());
    }
    (view, root, dirs)
}

/// 시작한 확인의 대상 목록을 꺼내고 확인을 비운다.
fn take_targets(view: &mut ExplorerView) -> Vec<PathBuf> {
    let query = view.poll.check.take().expect("a check started");
    query.dirs_for_test()
}

#[test]
fn a_round_reads_at_most_the_cap_with_the_current_folder_first() {
    let (mut view, root, dirs) = view_with_tree(MAX_POLL_TARGETS * 3);
    let t0 = Instant::now();
    view.poll.focused = true;
    assert!(view.start_check(t0));
    let first = take_targets(&mut view);
    assert_eq!(
        first.len(),
        MAX_POLL_TARGETS,
        "한 회차의 표지 읽기 수는 상한을 넘지 않는다"
    );
    assert_eq!(first[0], root, "현재 폴더를 먼저 넣는다");
    assert_eq!(
        first[1..],
        dirs[..MAX_POLL_TARGETS - 1],
        "처음에는 경로 순이다"
    );
}

/// 트리 폴더 `n` 개를 여러 바퀴 돌린다. 매 회차 현재 폴더가 먼저이고, ⌈n / 31⌉ 회차 안에 모두 한 번씩
/// 확인되며, 어느 회차가 끝나도 폴더별 확인 횟수의 차이가 한 번을 넘지 않는다.
fn assert_fair_rotation(n: usize) {
    let (mut view, root, dirs) = view_with_tree(n);
    let per_round = MAX_POLL_TARGETS - 1;
    let rounds = n.div_ceil(per_round);
    let mut seen: std::collections::HashMap<PathBuf, usize> =
        dirs.iter().map(|d| (d.clone(), 0)).collect();
    let t0 = Instant::now();
    for round in 0..rounds * 4 {
        assert!(view.start_check(t0 + EXTERNAL_POLL_INTERVAL * round as u32));
        let targets = take_targets(&mut view);
        assert!(targets.len() <= MAX_POLL_TARGETS);
        assert_eq!(targets[0], root, "현재 폴더는 매 회차 확인한다");
        for dir in &targets[1..] {
            *seen.get_mut(dir).expect("a tree folder") += 1;
        }
        if round + 1 == rounds {
            assert!(
                seen.values().all(|&c| c >= 1),
                "n={n}: {rounds} 회차 안에 모든 트리 폴더를 한 번씩 확인한다"
            );
        }
        let max = seen.values().max().unwrap();
        let min = seen.values().min().unwrap();
        if round + 1 >= rounds {
            assert!(
                max - min <= 1,
                "n={n} round={round}: 경로가 앞선 폴더만 자주 확인하지 않는다 ({min}..{max})"
            );
        }
    }
}

#[test]
fn every_expanded_folder_is_checked_within_a_bounded_number_of_rounds() {
    // 상한을 조금 넘는 수(실제 실행과 같은 40), 상한의 배수 근처, 큰 수.
    for n in [
        MAX_POLL_TARGETS,
        40,
        MAX_POLL_TARGETS * 2,
        MAX_POLL_TARGETS * 3 + 5,
        300,
    ] {
        assert_fair_rotation(n);
    }
}

#[test]
fn a_folder_read_just_now_goes_to_the_back_and_new_folders_join_the_rotation() {
    let (mut view, _root, dirs) = view_with_tree(MAX_POLL_TARGETS * 2);
    let t0 = Instant::now();
    assert!(view.start_check(t0));
    take_targets(&mut view);
    // 아직 확인하지 않은 폴더 하나를 방금 읽었다(트리 읽기 결과). 순서의 뒤로 간다.
    let fresh = dirs[MAX_POLL_TARGETS].clone();
    view.poll.note_read(&fresh, None);
    // 새로 펼친 폴더는 확인한 적이 없으므로 먼저 들어간다.
    let added = PathBuf::from("/tmp/tasty-poll-cap/a-new");
    view.expanded.insert(added.clone());
    view.tree_children.insert(added.clone(), Vec::new());
    assert!(view.start_check(t0 + EXTERNAL_POLL_INTERVAL));
    let second = take_targets(&mut view);
    assert_eq!(second[1], added);
    assert!(
        !second.contains(&fresh),
        "방금 읽은 폴더는 이번 회차에 다시 넣지 않는다"
    );
    assert!(second.contains(&dirs[MAX_POLL_TARGETS + 1]));
}

#[test]
fn folders_left_out_of_a_round_keep_their_baseline_so_a_later_round_still_sees_the_change() {
    let (mut view, _root, dirs) = view_with_tree(MAX_POLL_TARGETS * 2);
    let late = dirs.last().expect("dir").clone();
    view.poll.note_read(&late, None);
    assert!(view.start_check(Instant::now()));
    let targets = take_targets(&mut view);
    assert!(!targets.contains(&late));
    assert!(
        view.poll.baseline.contains_key(&late),
        "이번 회차에 넣지 않은 폴더의 기준 표지도 남긴다"
    );
    view.expanded.remove(&late);
    view.poll.check = None;
    assert!(view.start_check(Instant::now()));
    assert!(
        !view.poll.baseline.contains_key(&late) && !view.poll.checked_seq.contains_key(&late),
        "접은 폴더의 기록은 지운다"
    );
}

/// 확인이 도는 동안 포커스를 잃거나 칸이 닫혀도 worker 가 남지 않고, 결과는 다음 확인을 막지 않는다.
#[test]
fn a_check_in_flight_survives_a_tab_switch_and_a_closed_view_leaks_no_worker() {
    let dir = tempfile::tempdir().expect("temp dir");
    let panel = ExplorerPanel::new(1, dir.path().into());
    let mut store = ExplorerViewStore::default();
    let mut owner = LocalReads::default();
    let t0 = Instant::now();
    frame(&mut store, &panel, false, t0);
    settle(store.get_mut(1).expect("view"), &panel, &mut owner);
    frame(&mut store, &panel, true, t0);
    assert!(store.get(1).expect("view").poll.check.is_some());
    // 다른 탭으로 옮겨도 걸린 확인은 끝까지 거둔다.
    frame(&mut store, &panel, false, t0);
    finish_check(store.get_mut(1).expect("view"), &mut owner);
    frame(&mut store, &panel, true, t0 + EXTERNAL_POLL_INTERVAL);
    assert!(
        store.get(1).expect("view").poll.check.is_some(),
        "늦게 온 결과가 다음 확인을 막지 않는다"
    );
    // 확인을 worker 에 넘긴 뒤 칸을 닫는다. 결과를 받을 쪽이 사라져도 job 은 끝나고 거둬진다.
    owner.drive(|requests| {
        store.get_mut(1).expect("view").poll_local_reads(requests);
        true
    });
    store.views.remove(&1);
    // poll_shutdown 은 끝난 job 을 거둘 뿐 새 읽기를 제출하지 않아 마지막 읽기로 쓴다.
    let deadline = Instant::now() + Duration::from_secs(10);
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || owner.poll_shutdown() == 0),
        "worker 가 남았다"
    );
}

/// 설정을 끄면 파일 내용만 바뀐 변경은 다시 읽지 않고, 켜면 다음 회차에 보이던 목록을 둔 채 다시 읽어
/// 크기·수정 시각과 content key 가 새 값이 된다. 선택은 그대로다.
#[test]
fn file_details_are_compared_only_when_the_setting_is_on() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("note.txt");
    std::fs::write(&file, b"x").expect("write");
    let panel = ExplorerPanel::new(1, dir.path().into());
    let mut view = ExplorerView::new();
    let mut owner = LocalReads::default();
    settle(&mut view, &panel, &mut owner);
    view.select_only(&file);
    let before = view.entries[0].content_key();

    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(&file, b"longer text").expect("rewrite");
    view.poll_external(true, Instant::now());
    finish_check(&mut view, &mut owner);
    assert!(
        view.local_query.is_none(),
        "꺼져 있으면 폴더 표지만 비교한다"
    );
    assert_eq!(view.entries[0].size, 1);

    view.set_file_details_check(true);
    view.poll.check = None;
    assert!(view.start_check(Instant::now()));
    finish_check(&mut view, &mut owner);
    assert!(
        view.local_query.is_some(),
        "켜면 바뀐 파일 정보로 다시 읽는다"
    );
    settle(&mut view, &panel, &mut owner);
    assert_eq!(view.entries[0].size, 11);
    assert_ne!(
        view.entries[0].content_key(),
        before,
        "미리보기·썸네일 캐시 키가 바뀐다"
    );
    assert!(view.selected.contains(&file), "선택은 그대로다");

    // 다시 읽은 목록이 새 기준이므로 그대로면 다시 읽지 않는다.
    view.poll.check = None;
    assert!(view.start_check(Instant::now()));
    finish_check(&mut view, &mut owner);
    assert!(view.local_query.is_none());
    shutdown(owner);
}

/// 활성이 아닌 탭은 새 외부 변경 확인을 시작하지 않는다. 떠날 때 걸려 있던 확인도 버리고, 돌아오면
/// 포커스를 되찾은 프레임처럼 바로 확인한다.
#[test]
fn only_the_active_tab_starts_outside_change_checks() {
    let dir = tempfile::tempdir().expect("fixture");
    let x = dir.path().join("x");
    let y = dir.path().join("y");
    std::fs::create_dir(&x).unwrap();
    std::fs::create_dir(&y).unwrap();
    let mut owner = LocalReads::default();
    let mut store = ExplorerViewStore::default();
    let mut panel = ExplorerPanel::new(1, x.clone());
    let now = Instant::now();
    let view = store.get_or_init(&panel, None);
    while view.local_query.is_some() || view.writable_query.is_some() {
        owner.reap();
        owner.drive(|r| view.poll_local_reads(r));
        std::thread::yield_now();
    }
    store.begin_poll_frame();
    store.get_or_init(&panel, None).poll_external(true, now);
    assert!(store.get(1).expect("view").poll.check.is_some());

    panel.add_tab();
    panel.active_tab_mut().navigate_to(y.clone());
    let view = store.get_or_init(&panel, None);
    view.for_each_parked(|v| {
        assert!(v.poll.check.is_none());
        assert!(!v.poll.focused && v.poll.next_at.is_none());
    });
    while view.local_query.is_some() || view.writable_query.is_some() {
        owner.reap();
        owner.drive(|r| view.poll_local_reads(r));
        std::thread::yield_now();
    }
    assert!(!store.start_due_checks(now + EXTERNAL_POLL_INTERVAL * 3));
    store
        .get_mut(1)
        .unwrap()
        .for_each_parked(|v| assert!(!v.start_due_check(now + EXTERNAL_POLL_INTERVAL * 3)));

    panel.active = 0;
    store.begin_poll_frame();
    store.get_or_init(&panel, None).poll_external(true, now);
    assert!(
        store.get(1).expect("view").poll.check.is_some(),
        "returning checks at once"
    );
    let view = store.get_mut(1).expect("view");
    view.poll.check = None;
    while owner.poll_shutdown() != 0 {
        std::thread::yield_now();
    }
}
