use super::super::ExplorerViewStore;
use super::*;
use crate::app::local_reads::LocalReads;
use std::path::Path;

/// `done` 이 참이 될 때까지 worker 를 돌린다. 끝난 job 을 거둬 동시 실행 상한에 막히지 않게 한다.
fn run_until(
    view: &mut ExplorerView,
    owner: &mut LocalReads,
    done: impl Fn(&ExplorerView) -> bool,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    // holds_by 로 바꾸지 않는다. drive 는 새 읽기 worker 를 제출하는 진행이라 기한 뒤 관측에서
    // 부를 수 없고, 그것 없이는 끝난 결과가 view 에 들어오지 않는다.
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "read worker did not settle"
        );
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

fn idle(v: &ExplorerView) -> bool {
    v.local_query.is_none() && v.writable_query.is_none() && v.tree_queries.is_empty()
}

fn settle(view: &mut ExplorerView, panel: &ExplorerPanel, owner: &mut LocalReads) {
    view.sync(panel, None);
    run_until(view, owner, idle);
}

fn shutdown(mut owner: LocalReads) {
    while owner.poll_shutdown() != 0 {
        std::thread::yield_now();
    }
}

/// x: foo1 foo2 foo4 bar sub/foo3, y: only.txt
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let x = dir.path().join("x");
    let y = dir.path().join("y");
    std::fs::create_dir_all(x.join("sub")).expect("x");
    std::fs::create_dir_all(&y).expect("y");
    for f in [
        "foo1.txt",
        "foo2.txt",
        "foo4.txt",
        "bar.txt",
        "sub/foo3.txt",
    ] {
        std::fs::write(x.join(f), b"").expect("file");
    }
    std::fs::write(y.join("only.txt"), b"").expect("file");
    (dir, x, y)
}

fn names(view: &ExplorerView) -> Vec<String> {
    view.entries.iter().map(|e| e.name.clone()).collect()
}

/// 탭 A(x)·B(y)·C(x). 돌아온 탭은 보관한 목록·선택·Find 를 Loading 없이 그대로 보이고, 같은 폴더를 연
/// C 는 A 와 따로다.
#[test]
fn each_internal_tab_keeps_its_own_list_selection_and_find() {
    let (_dir, x, y) = fixture();
    let mut owner = LocalReads::default();
    let mut panel = ExplorerPanel::new(1, x.clone());
    let mut view = ExplorerView::new();
    settle(&mut view, &panel, &mut owner);
    view.click_select(&x.join("foo1.txt"), false, false);
    view.click_select(&x.join("foo4.txt"), true, false);
    view.click_select(&x.join("foo2.txt"), true, false);
    view.open_find();
    view.find.as_mut().expect("find").set_query("foo");
    let a = panel.active_tab().id();

    panel.add_tab();
    panel.active_tab_mut().navigate_to(y.clone());
    settle(&mut view, &panel, &mut owner);
    assert_eq!(names(&view), ["only.txt"]);
    assert!(view.selected.is_empty(), "B starts without A's selection");
    assert!(view.find.is_none(), "B starts without A's Find");
    view.select_only(&y.join("only.txt"));

    panel.add_tab();
    settle(&mut view, &panel, &mut owner);
    assert_eq!(panel.current_root(), x.as_path());
    assert!(
        view.selected.is_empty(),
        "C shows the same folder as A but not its state"
    );
    assert!(view.find.is_none());

    panel.active = panel.tab_index(a).expect("A");
    view.sync(&panel, None);
    assert_eq!(
        view.state,
        LoadState::Ok,
        "a tab switch is not a folder change"
    );
    assert!(view.local_query.is_none(), "no new read for a kept list");
    assert_eq!(view.selected.len(), 3);
    assert!(view.selected.contains(&x.join("foo4.txt")));
    assert_eq!(view.find.as_ref().map(|f| f.query()), Some("foo"));
    assert_eq!(view.parked_tab_count(), 2);

    panel.active = 1;
    view.sync(&panel, None);
    assert_eq!(names(&view), ["only.txt"]);
    assert!(view.selected.contains(&y.join("only.txt")));
    shutdown(owner);
}

/// 활성이 아닌 탭이 이미 시작한 목록 읽기와 하위 폴더 검색은 그 탭에 도착한다. 지금 탭에는 섞이지 않는다.
#[test]
fn late_reads_and_searches_land_in_the_tab_that_started_them() {
    let (_dir, x, y) = fixture();
    let mut owner = LocalReads::default();
    let mut panel = ExplorerPanel::new(1, y.clone());
    let mut view = ExplorerView::new();
    settle(&mut view, &panel, &mut owner);
    // A 가 x 로 이동해 읽기를 걸고, 같은 x 에서 하위 폴더 검색도 시작한다.
    panel.active_tab_mut().navigate_to(x.clone());
    view.sync(&panel, None);
    assert_eq!(view.state, LoadState::Loading);
    view.search_subfolders("foo");
    let a = panel.active_tab().id();

    panel.add_tab();
    panel.active_tab_mut().navigate_to(y.clone());
    view.sync(&panel, None);
    run_until(&mut view, &mut owner, |v| {
        idle(v)
            && v.parked
                .get(&a)
                .is_some_and(|s| s.local_query.is_none() && search_done(s.find.as_ref()))
    });
    assert_eq!(names(&view), ["only.txt"], "B keeps its own list");
    assert!(view.find.is_none(), "A's search does not show in B");

    panel.active = 0;
    view.sync(&panel, None);
    assert_eq!(
        view.state,
        LoadState::Ok,
        "A's read was collected while it was away"
    );
    assert!(names(&view).contains(&"foo1.txt".to_string()));
    let hits: Vec<&Path> = view
        .search_hits()
        .iter()
        .map(|e| e.path.as_path())
        .collect();
    assert!(hits.contains(&x.join("sub/foo3.txt").as_path()), "{hits:?}");
    shutdown(owner);
}

fn search_done(find: Option<&super::super::super::find::FindState>) -> bool {
    find.is_some_and(|f| f.search_finished())
}

/// 닫은 탭의 상태와 기다리던 읽기는 버린다. 닫힌 탭 ID 는 다시 쓰이지 않으므로 늦은 결과가 갈 곳이 없다.
#[test]
fn closing_a_tab_drops_its_state_and_pending_reads() {
    let (_dir, x, y) = fixture();
    let mut owner = LocalReads::default();
    let mut panel = ExplorerPanel::new(1, x.clone());
    let mut view = ExplorerView::new();
    settle(&mut view, &panel, &mut owner);
    view.search_subfolders("foo");
    panel.add_tab();
    panel.active_tab_mut().navigate_to(y.clone());
    view.sync(&panel, None);
    assert_eq!(view.parked_tab_count(), 1);
    assert!(
        view.parked
            .values()
            .next()
            .is_some_and(TabSession::has_pending)
    );

    panel.close_tab(0);
    view.sync(&panel, None);
    assert_eq!(view.parked_tab_count(), 0, "the closed tab's state is gone");
    run_until(&mut view, &mut owner, idle);
    shutdown(owner);
}

/// 파일 작업이 바꾼 폴더는 활성이 아닌 탭에도 다시 읽기를 표시한다. 돌아오면 그 탭이 다시 읽는다.
#[test]
fn a_local_file_action_marks_background_tabs_for_reload() {
    let (_dir, x, y) = fixture();
    let mut owner = LocalReads::default();
    let mut store = ExplorerViewStore::default();
    let mut panel = ExplorerPanel::new(1, x.clone());
    run_until(store.get_or_init(&panel, None), &mut owner, idle);
    panel.add_tab();
    panel.active_tab_mut().navigate_to(y.clone());
    run_until(store.get_or_init(&panel, None), &mut owner, idle);

    assert!(store.invalidate_local(std::slice::from_ref(&x), &[]));
    let view = store.get(1).expect("view");
    assert!(!view.reload_requested, "B shows another folder");
    assert!(
        view.parked
            .values()
            .next()
            .is_some_and(|s| s.reload_requested)
    );

    panel.active = 0;
    let view = store.get_or_init(&panel, None);
    assert!(
        view.local_query.is_some(),
        "A reads its folder again on return"
    );
    run_until(view, &mut owner, idle);
    shutdown(owner);
}

/// 숨김 파일을 다른 탭에서 바꿨으면 돌아온 탭의 목록도 그 값으로 다시 나눈다.
#[test]
fn a_returning_tab_follows_the_current_hidden_files_setting() {
    let (_dir, x, y) = fixture();
    std::fs::write(x.join(".dot"), b"").expect("dot");
    let mut owner = LocalReads::default();
    let mut panel = ExplorerPanel::new(1, x.clone());
    panel.show_hidden = false;
    let mut view = ExplorerView::new();
    settle(&mut view, &panel, &mut owner);
    assert!(!names(&view).contains(&".dot".to_string()));

    panel.add_tab();
    panel.active_tab_mut().navigate_to(y.clone());
    settle(&mut view, &panel, &mut owner);
    panel.show_hidden = true;
    view.sync(&panel, None);

    panel.active = 0;
    view.sync(&panel, None);
    assert!(names(&view).contains(&".dot".to_string()));
    shutdown(owner);
}

#[test]
fn remote_replies_belong_to_their_tab_even_with_the_same_root() {
    let root = PathBuf::from("/remote/x");
    let mut panel = ExplorerPanel::new(1, root.clone());
    let mut view = ExplorerView::new();
    view.sync(&panel, Some(7));
    let a = view.drain_outbox().pop().unwrap().request_id;
    panel.add_tab();
    view.sync(&panel, Some(7));
    let b = view.drain_outbox().pop().unwrap().request_id;
    assert_ne!(a, b);
    let entry = DirEntryInfo {
        path: root.join("a"),
        name: "a".into(),
        ..Default::default()
    };
    assert!(view.apply_remote_list_dir_result(a, &panel, Ok(vec![entry])));
    assert_eq!(view.state, LoadState::Loading);
    assert!(view.entries.is_empty());
    panel.active = 0;
    view.sync(&panel, Some(7));
    assert_eq!(names(&view), ["a"]);
    assert_eq!(view.state, LoadState::Ok);
    panel.close_tab(1);
    assert!(!view.apply_remote_list_dir_result(b, &panel, Ok(Vec::new())));
    assert_eq!(names(&view), ["a"]);
    assert!(view.drain_outbox().is_empty());
}

#[test]
fn closing_a_tab_discards_its_unsent_remote_request() {
    let mut panel = ExplorerPanel::new(1, "/remote/x".into());
    let mut view = ExplorerView::new();
    view.sync(&panel, Some(7));
    panel.add_tab();
    panel.close_tab(0);
    view.sync(&panel, Some(7));
    assert_eq!(view.drain_outbox().len(), 1);
}

#[test]
fn returning_to_the_same_root_does_not_resume_a_pending_reveal() {
    let (_dir, x, _) = fixture();
    let mut owner = LocalReads::default();
    let mut panel = ExplorerPanel::new(1, x.clone());
    let mut view = ExplorerView::new();
    settle(&mut view, &panel, &mut owner);
    view.reveal_created(&x.join("foo1.txt"));
    panel.add_tab();
    settle(&mut view, &panel, &mut owner);
    assert!(view.take_reveal().is_none());
    panel.active = 0;
    view.sync(&panel, None);
    assert!(view.take_reveal().is_none());
    assert!(view.selected.contains(&x.join("foo1.txt")));
    shutdown(owner);
}

#[test]
fn selection_identity_routes_operation_completion_without_changing_other_tabs() {
    let (_dir, x, _) = fixture();
    let mut owner = LocalReads::default();
    let mut panel = ExplorerPanel::new(1, x.clone());
    let mut view = ExplorerView::new();
    settle(&mut view, &panel, &mut owner);
    view.select_only(&x.join("foo1.txt"));
    let original = view.selection_identity();
    panel.add_tab();
    settle(&mut view, &panel, &mut owner);
    view.select_only(&x.join("bar.txt"));
    view.for_each_tab(|view| {
        if view.matches_selection(&original) {
            view.clear_selection();
        }
    });
    assert!(view.selected.contains(&x.join("bar.txt")));
    panel.active = 0;
    view.sync(&panel, None);
    assert!(view.selected.is_empty());
    shutdown(owner);
}

#[test]
fn separate_window_stores_and_surfaces_do_not_share_internal_tab_state() {
    let (_dir, x, _) = fixture();
    let mut owner = LocalReads::default();
    let panel = ExplorerPanel::new(1, x.clone());
    let other_panel = ExplorerPanel::new(2, x.clone());
    let mut first_window = ExplorerViewStore::default();
    let mut second_window = ExplorerViewStore::default();
    let view = first_window.get_or_init(&panel, None);
    settle(view, &panel, &mut owner);
    view.select_only(&x.join("foo1.txt"));
    view.open_find();
    view.find.as_mut().unwrap().set_query("foo");
    let other = first_window.get_or_init(&other_panel, None);
    settle(other, &other_panel, &mut owner);
    assert!(other.selected.is_empty() && other.find.is_none());
    let same_surface_elsewhere = second_window.get_or_init(&panel, None);
    settle(same_surface_elsewhere, &panel, &mut owner);
    assert!(same_surface_elsewhere.selected.is_empty() && same_surface_elsewhere.find.is_none());
    assert_eq!(first_window.get(1).unwrap().selected.len(), 1);
    shutdown(owner);
}
