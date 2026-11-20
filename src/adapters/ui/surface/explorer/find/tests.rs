use super::*;
use crate::app::local_reads::LocalReads;
use tasty_model::ExplorerPanel;
use tasty_ui_widgets::match_range;

fn names(entries: &[DirEntryInfo]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

fn typed() -> ExplorerFindEvents {
    ExplorerFindEvents {
        changed: true,
        field_focused: true,
        ..Default::default()
    }
}

/// 폴더를 읽어 둔 로컬 view 와 그 worker.
fn loaded(dir: &Path) -> (ExplorerPanel, ExplorerView, LocalReads) {
    let panel = ExplorerPanel::new(1, dir.into());
    let mut view = ExplorerView::new();
    let mut owner = LocalReads::default();
    view.sync(&panel, None);
    owner.drive(|requests| view.poll_local_reads(requests));
    (panel, view, owner)
}

fn set_query(view: &mut ExplorerView, query: &str, deep: bool) {
    view.find.as_mut().unwrap().query = query.into();
    view.apply_find_events(typed(), deep);
}

fn finish(owner: &mut LocalReads) {
    while owner.poll_shutdown() != 0 {
        std::thread::yield_now();
    }
}

#[test]
fn the_filter_hides_rows_that_do_not_match_and_counts_what_is_shown() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["Report.pdf", "mockup-exports", "notes.md"] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    let (_panel, mut view, mut owner) = loaded(dir.path());
    view.select_only(&dir.path().join("notes.md"));
    view.open_find();
    set_query(&mut view, "RE", false);
    assert_eq!(names(&view.shown_entries()), ["Report.pdf"]);
    assert!(
        view.selected.is_empty(),
        "a hidden row leaves the selection so commands never reach it"
    );
    assert_eq!(view.find_query(), "RE");
    assert_eq!(view.status_text(), t_fmt2("explorer.find.status", "1", "3"));
    assert!(
        view.text_input_active(),
        "type-ahead stops while the field has the keys"
    );
    assert!(view.find_screen().is_none());

    view.select_all();
    assert_eq!(
        view.selected,
        std::collections::HashSet::from([dir.path().join("Report.pdf")]),
        "select all takes only the rows that are shown"
    );

    set_query(&mut view, "zzz", false);
    assert!(view.shown_entries().is_empty());
    match view.find_screen() {
        Some(FindScreen::NoFilterMatches { query }) => {
            crate::i18n::init("en");
            assert_eq!(
                super::super::state_screen::no_filter_matches_title(&query),
                "No names match “zzz”"
            );
        }
        _ => panic!("a filter without matches shows its own state"),
    }
    finish(&mut owner);
}

#[test]
fn esc_clears_the_text_first_and_closes_the_bar_second() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"").unwrap();
    let (_panel, mut view, mut owner) = loaded(dir.path());
    view.open_find();
    set_query(&mut view, "a", false);
    let esc = ExplorerFindEvents {
        escape: true,
        ..Default::default()
    };
    view.apply_find_events(esc, false);
    assert_eq!(view.find_query(), "");
    assert!(view.find.is_some(), "the first Esc keeps the bar");
    view.apply_find_events(esc, false);
    assert!(view.find.is_none(), "the second Esc closes it");

    view.toggle_find();
    let close = ExplorerFindEvents {
        close: true,
        ..Default::default()
    };
    view.apply_find_events(close, false);
    assert!(view.find.is_none(), "× closes the bar");
    finish(&mut owner);
}

#[test]
fn leaving_the_folder_closes_the_bar() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let (mut panel, mut view, mut owner) = loaded(dir.path());
    view.open_find();
    set_query(&mut view, "s", false);
    view.sync(&panel, None);
    assert!(view.find.is_some(), "staying in the folder keeps the bar");
    panel.active_tab_mut().navigate_to(dir.path().join("sub"));
    view.sync(&panel, None);
    assert!(view.find.is_none());
    finish(&mut owner);
}

#[test]
fn subfolders_search_streams_hits_with_their_folder_and_stops_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("Documents").join("finance");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(dir.path().join("report.pdf"), b"").unwrap();
    std::fs::write(deep.join("q3-report.xlsx"), b"").unwrap();
    let (_panel, mut view, mut owner) = loaded(dir.path());
    view.open_find();
    set_query(&mut view, "report", true);
    assert!(view.search_root().is_some());
    let running = |view: &ExplorerView| {
        view.find
            .as_ref()
            .and_then(|f| f.search.as_ref())
            .is_some_and(|s| s.outcome == Outcome::Running)
    };
    owner.drive(|requests| {
        view.poll_local_reads(requests);
        !running(&view)
    });
    assert_eq!(
        names(&view.shown_entries()),
        ["q3-report.xlsx", "report.pdf"],
        "hits are sorted like the listing"
    );
    assert_eq!(view.status_text(), t_fmt("explorer.find.found", "2"));
    let hit = deep.join("q3-report.xlsx");
    assert_eq!(
        hit_folder(dir.path(), &hit),
        Path::new("Documents").join("finance").display().to_string()
    );
    view.select_only(&hit);
    assert_eq!(
        view.status_text(),
        Path::new("Documents")
            .join("finance")
            .join("q3-report.xlsx")
            .display()
            .to_string(),
        "the status line shows where the selected hit is"
    );

    // 체크를 풀면 지금 폴더 거르기로 돌아간다.
    view.apply_find_events(
        ExplorerFindEvents {
            subfolders_toggled: true,
            ..Default::default()
        },
        false,
    );
    assert!(view.search_root().is_none());
    assert_eq!(names(&view.shown_entries()), ["report.pdf"]);
    owner.reap();

    // Stop 은 결과를 남기고 Stopped 로 끝난다.
    set_query(&mut view, "report", true);
    view.apply_find_events(
        ExplorerFindEvents {
            stop: true,
            ..Default::default()
        },
        true,
    );
    owner.drive(|requests| {
        view.poll_local_reads(requests);
        !running(&view)
    });
    let outcome = &view.find.as_ref().unwrap().search.as_ref().unwrap().outcome;
    assert!(
        matches!(outcome, Outcome::Stopped),
        "stopped before it started"
    );
    finish(&mut owner);
}

#[test]
fn a_search_that_cannot_read_its_start_folder_fails_and_nothing_found_is_a_state() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"").unwrap();
    let (_panel, mut view, mut owner) = loaded(dir.path());
    view.open_find();
    set_query(&mut view, "zzz", true);
    let running = |view: &ExplorerView| {
        view.find
            .as_ref()
            .and_then(|f| f.search.as_ref())
            .is_some_and(|s| s.outcome == Outcome::Running)
    };
    owner.drive(|requests| {
        view.poll_local_reads(requests);
        !running(&view)
    });
    assert!(matches!(
        view.find_screen(),
        Some(FindScreen::NoSearchMatches { .. })
    ));
    owner.reap();

    // 시작 폴더가 사라지면 실패 화면이다.
    let root = dir.path().to_path_buf();
    drop(dir);
    view.find.as_mut().unwrap().restart();
    owner.drive(|requests| {
        view.poll_local_reads(requests);
        !running(&view)
    });
    assert!(matches!(view.find_screen(), Some(FindScreen::Failed(_))));
    assert!(!root.exists());
    finish(&mut owner);
}

#[test]
fn batches_inserted_in_order_match_a_full_sort() {
    let entry = |name: &str, is_dir: bool, size: u64| DirEntryInfo {
        path: PathBuf::from("/s").join(name),
        name: name.into(),
        is_dir,
        size,
        modified: None,
        ext: String::new(),
        ..Default::default()
    };
    let batches = vec![
        vec![
            entry("m", false, 5),
            entry("B", false, 1),
            entry("dir-z", true, 0),
        ],
        vec![entry("a", false, 9), entry("b", false, 3)],
        vec![],
        vec![
            entry("dir-a", true, 0),
            entry("z", false, 1),
            entry("A", false, 7),
        ],
    ];
    for (col, dir) in [
        (SortColumn::Name, SortDir::Asc),
        (SortColumn::Name, SortDir::Desc),
        (SortColumn::Size, SortDir::Asc),
    ] {
        let mut hits = Vec::new();
        for batch in batches.clone() {
            insert_sorted(&mut hits, batch, col, dir);
        }
        let mut all: Vec<DirEntryInfo> = batches.iter().flatten().cloned().collect();
        sort_entries(&mut all, col, dir);
        let order = |v: &[DirEntryInfo]| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(order(&hits), order(&all), "{col:?} {dir:?}");
    }
}

/// 검색 결과를 손으로 채운 Find 상태.
fn searched(outcome: Outcome, skipped: usize) -> FindState {
    FindState {
        matcher: FindQuery::default(),
        show_hidden: false,
        root: PathBuf::from("/srv"),
        query: "rs".into(),
        deep: true,
        focus: false,
        field_focused: false,
        field_id: None,
        filtered: None,
        search: Some(Search {
            receipt: None,
            hits: Vec::new(),
            skipped: (0..skipped)
                .map(|i| PathBuf::from(format!("/srv/{i}")))
                .collect(),
            outcome,
        }),
    }
}

#[test]
fn one_skipped_folder_and_the_hit_cap_have_their_own_words() {
    crate::i18n::init("en");
    let (text, skipped, _) = status_parts(&searched(Outcome::Done, 1), 0, 0);
    assert_eq!(skipped, "1 folder skipped");
    assert_eq!(text, t_fmt("explorer.find.found", "0"));
    let (_, skipped, _) = status_parts(&searched(Outcome::Done, 2), 0, 0);
    assert_eq!(skipped, t_fmt("explorer.find.skipped", "2"));

    let (text, _, _) = status_parts(&searched(Outcome::Capped, 0), 0, 0);
    assert_eq!(text, "5,000+ found · stopped");
    let mut view = ExplorerView::new();
    view.find = Some(searched(Outcome::Capped, 0));
    assert_eq!(
        view.status_text(),
        "Showing the first 5,000. Type more to narrow the search."
    );
}

fn file(i: usize) -> DirEntryInfo {
    DirEntryInfo {
        path: PathBuf::from(format!("/w/file{i:05}.txt")),
        name: format!("file{i:05}.txt"),
        is_dir: false,
        size: 0,
        modified: None,
        ext: "txt".into(),
        ..Default::default()
    }
}

/// 거르는 중인 view. 목록은 `n` 개의 이름 붙은 파일이다.
fn filtering(n: usize, query: &str) -> (tempfile::TempDir, ExplorerView) {
    let dir = tempfile::tempdir().unwrap();
    let (_panel, mut view, _owner) = loaded(dir.path());
    view.set_entries((0..n).map(file).collect());
    view.open_find();
    set_query(&mut view, query, false);
    (dir, view)
}

fn cached_hits(view: &ExplorerView) -> Option<*const usize> {
    let filtered = view.find.as_ref()?.filtered.as_ref()?;
    Some(filtered.hits.as_ptr())
}

/// 거른 결과를 쓰지 않고 이름을 하나씩 대 본 결과.
fn matched_by_hand(view: &ExplorerView, query: &str) -> Vec<String> {
    (view.entries.iter())
        .filter(|e| match_range(&e.name, query).is_some())
        .map(|e| e.name.clone())
        .collect()
}

#[test]
fn the_filter_is_not_run_again_while_the_query_and_the_list_stay() {
    let (_dir, mut view) = filtering(20_000, "file1");
    view.refresh_filter();
    let first = cached_hits(&view).expect("the filter result is kept");
    for _ in 0..5 {
        view.refresh_filter();
        assert_eq!(cached_hits(&view), Some(first), "filtered again");
    }
    assert_eq!(view.shown_count(), 10_000);
    let range: Vec<String> = view
        .shown_range(9_998..10_005)
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(range, ["file19998.txt", "file19999.txt"]);
    assert_eq!(
        view.shown().nth(3).map(|e| e.name.as_str()),
        Some("file10003.txt")
    );
}

#[test]
fn a_new_query_or_a_new_list_filters_again() {
    let (_dir, mut view) = filtering(300, "file001");
    view.refresh_filter();
    let shown = |v: &ExplorerView| {
        v.shown_entries()
            .into_iter()
            .map(|e| e.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(shown(&view), matched_by_hand(&view, "file001"));

    set_query(&mut view, "file0029", false);
    assert_eq!(
        shown(&view),
        [
            "file00290.txt",
            "file00291.txt",
            "file00292.txt",
            "file00293.txt",
            "file00294.txt",
            "file00295.txt",
            "file00296.txt",
            "file00297.txt",
            "file00298.txt",
            "file00299.txt"
        ]
    );
    view.refresh_filter();
    assert_eq!(shown(&view), matched_by_hand(&view, "file0029"));

    // 같은 길이의 새 목록. 이름이 달라 예전 번호로 고르면 틀린다.
    let renamed: Vec<DirEntryInfo> = (0..300)
        .map(|i| DirEntryInfo {
            name: format!("other{i:05}.txt"),
            ..file(i)
        })
        .collect();
    let before = cached_hits(&view);
    view.set_entries(renamed);
    assert!(view.shown_entries().is_empty(), "a stale result was used");
    view.refresh_filter();
    assert_ne!(cached_hits(&view), before);
    assert_eq!(view.shown_count(), 0);

    // Subfolders 를 켜면 거른 결과를 버린다.
    set_query(&mut view, "other", true);
    view.refresh_filter();
    assert!(cached_hits(&view).is_none());
}

fn named(root: &Path, rel: &str) -> DirEntryInfo {
    let path = root.join(rel);
    DirEntryInfo {
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        path,
        ..file(0)
    }
}

#[test]
fn a_new_query_keeps_only_the_selected_items_it_still_shows() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (_panel, mut view, _owner) = loaded(root);
    let mut entries: Vec<DirEntryInfo> = ["alpha1.txt", "alpha2.txt", "beta.txt"]
        .iter()
        .map(|n| named(root, n))
        .collect();
    entries.extend((0..20_000).map(|i| named(root, &format!("fill{i:05}"))));
    view.set_entries(entries);
    view.open_find();
    // 하위 폴더 검색에서 고른 뒤 남은 항목. 이름은 맞지만 지금 폴더 항목이 아니다.
    let deep_pick = root.join("sub/alpha9.txt");
    view.selected = [
        root.join("alpha1.txt"),
        root.join("beta.txt"),
        deep_pick.clone(),
    ]
    .into();
    view.anchor = Some(root.join("beta.txt"));

    set_query(&mut view, "alpha", false);
    assert_eq!(
        view.selected,
        std::collections::HashSet::from([root.join("alpha1.txt")])
    );
    assert_eq!(view.anchor, None, "the anchor left with its row");

    view.anchor = Some(root.join("alpha1.txt"));
    set_query(&mut view, "ALPHA1", false);
    assert_eq!(
        view.selected,
        std::collections::HashSet::from([root.join("alpha1.txt")])
    );
    assert_eq!(view.anchor, Some(root.join("alpha1.txt")));

    set_query(&mut view, "", false);
    assert_eq!(
        view.selected,
        std::collections::HashSet::from([root.join("alpha1.txt")])
    );
}

#[test]
fn subfolder_results_keep_only_the_selected_hits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let (_panel, mut view, _owner) = loaded(root);
    view.set_entries(vec![named(root, "alpha1.txt")]);
    view.open_find();
    let hit = named(root, "sub/alpha9.txt");
    let find = view.find.as_mut().unwrap();
    find.query = "alpha".into();
    find.deep = true;
    find.search = Some(Search {
        receipt: None,
        hits: vec![hit.clone()],
        skipped: Vec::new(),
        outcome: Outcome::Done,
    });
    view.selected = [hit.path.clone(), root.join("alpha1.txt")].into();
    view.retain_shown_selection();
    assert_eq!(view.selected, std::collections::HashSet::from([hit.path]));
}

/// 하위 폴더 검색 결과 `hit` 를 보이는 view. 결과는 지금 폴더 목록(`entries`)에 없다.
fn showing_hit(root: &Path, hit: &DirEntryInfo) -> (ExplorerPanel, ExplorerView, LocalReads) {
    let (panel, mut view, owner) = loaded(root);
    view.set_entries(vec![named(root, "alpha1.txt")]);
    view.open_find();
    let find = view.find.as_mut().unwrap();
    find.query = "alpha".into();
    find.deep = true;
    find.search = Some(Search {
        receipt: None,
        hits: vec![hit.clone()],
        skipped: Vec::new(),
        outcome: Outcome::Done,
    });
    (panel, view, owner)
}

#[test]
fn a_selected_subfolder_hit_is_what_preview_and_properties_act_on() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let hit = named(root, "sub/alpha9.txt");
    let (_panel, mut view, _owner) = showing_hit(root, &hit);
    view.select_only(&hit.path);
    assert_eq!(
        super::super::preview::preview_target(&view).map(|e| e.path),
        Some(hit.path.clone()),
        "the preview shows the picked result"
    );
    assert_eq!(
        crate::adapters::ui::popup::explorer_properties::shortcut_paths(Some(&view), root),
        vec![hit.path.clone()],
        "the properties shortcut opens the picked result, not the folder"
    );
    assert_eq!(
        view.listed_entry(&hit.path).map(|e| e.path.clone()),
        Some(hit.path.clone()),
        "a mirror's properties find the result's listed facts"
    );

    view.selected.clear();
    assert_eq!(
        crate::adapters::ui::popup::explorer_properties::shortcut_paths(Some(&view), root),
        vec![root.to_path_buf()],
        "with nothing picked the shortcut opens the current folder"
    );
}

#[test]
fn a_broken_link_among_subfolder_hits_is_reported_as_broken() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let hit = DirEntryInfo {
        link: crate::core::fs_list::EntryLink::Broken,
        ..named(root, "sub/alpha-gone")
    };
    let (_panel, view, _owner) = showing_hit(root, &hit);
    assert!(view.is_broken_link(&hit.path));
    assert!(!view.is_broken_link(&root.join("alpha1.txt")));
}

#[test]
fn extension_filter_matches_recursive_search_and_removes_excluded_selection() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["note.rs", "NOTE.md", "other.rs", "note.txt"] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    std::fs::create_dir(dir.path().join("note.rs-dir")).unwrap();
    let (_panel, mut view, mut owner) = loaded(dir.path());
    view.select_only(&dir.path().join("note.txt"));
    view.open_find();
    set_query(&mut view, "note ext:rs EXT:.md", false);
    view.refresh_filter();
    assert_eq!(names(&view.shown_entries()), ["NOTE.md", "note.rs"]);
    assert!(view.selected.is_empty());
    assert_eq!(
        view.find_query(),
        "note",
        "only the name clause is highlighted"
    );
    set_query(&mut view, "note ext:rs EXT:.md", true);
    owner.drive(|requests| {
        view.poll_local_reads(requests);
        !view.search_running()
    });
    assert_eq!(names(&view.shown_entries()), ["NOTE.md", "note.rs"]);
    finish(&mut owner);
}

#[test]
fn changing_hidden_cancels_the_receipt_discards_queued_hits_and_searches_again() {
    use std::sync::atomic::Ordering;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".private")).unwrap();
    std::fs::write(dir.path().join(".private/note.rs"), b"").unwrap();
    std::fs::write(dir.path().join("note.rs"), b"").unwrap();
    let (_panel, mut view, mut owner) = loaded(dir.path());
    view.open_find();
    set_query(&mut view, "ext:rs", true);
    let (receipt, sender, stopped) = SearchQuery::test_channel();
    view.find.as_mut().unwrap().search.as_mut().unwrap().receipt = Some(receipt);
    let stale = DirEntryInfo {
        name: "stale.rs".into(),
        path: dir.path().join("stale.rs"),
        ..Default::default()
    };
    assert!(sender.send(SearchEvent::Hits(vec![stale])).is_ok());
    view.toggle_hidden();
    assert!(stopped.load(Ordering::Acquire));
    assert!(
        sender
            .send(SearchEvent::Done {
                stopped: false,
                capped: false
            })
            .is_err(),
        "old receiver was dropped"
    );
    owner.drive(|requests| {
        view.poll_local_reads(requests);
        !view.search_running()
    });
    assert_eq!(view.shown_count(), 2);
    assert!(
        view.shown().all(|e| e.name == "note.rs"),
        "queued stale hit must not leak"
    );
    view.select_only(&dir.path().join(".private/note.rs"));
    view.toggle_hidden();
    assert!(
        view.search_running(),
        "completed search also restarts on a toggle"
    );
    assert!(
        view.shown_entries().is_empty(),
        "old hits are removed immediately"
    );
    assert!(view.selected.is_empty());
    owner.drive(|requests| {
        view.poll_local_reads(requests);
        !view.search_running()
    });
    assert_eq!(
        view.shown_entries()
            .iter()
            .map(|e| &e.path)
            .collect::<Vec<_>>(),
        [&dir.path().join("note.rs")]
    );
    finish(&mut owner);
}

#[test]
#[ignore = "explicit serial release measurement of real search batch insertion and path index rebuild"]
fn search_batch_index_cpu_samples() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .finish();
    let _log = tracing::subscriber::set_default(subscriber);
    let dir = tempfile::tempdir().unwrap();
    let (_panel, mut view, mut owner) = loaded(dir.path());
    view.search_subfolders("item");
    let (receipt, sender, _) = SearchQuery::test_channel();
    view.find.as_mut().unwrap().search.as_mut().unwrap().receipt = Some(receipt);
    let mut samples = Vec::new();
    for start in (0..5000).step_by(256) {
        let end = (start + 256).min(5000);
        let batch: Vec<_> = (start..end)
            .map(|i| {
                let name = format!("item{i:06}.txt");
                DirEntryInfo {
                    path: dir.path().join(&name),
                    name,
                    ..Default::default()
                }
            })
            .collect();
        let target = batch.last().unwrap().path.clone();
        sender.send(SearchEvent::Hits(batch)).unwrap();
        let started = std::time::Instant::now();
        owner.drive(|requests| view.poll_find(requests));
        assert_eq!(view.shown_position(&target), Some(end - 1));
        samples.push(serde_json::json!({"hits":end,"poll_and_first_lookup_ms":started.elapsed().as_secs_f64()*1000.0}));
    }
    assert_eq!(view.shown_count(), 5000);
    sender
        .send(SearchEvent::Done {
            stopped: false,
            capped: true,
        })
        .unwrap();
    owner.drive(|requests| view.poll_find(requests));
    finish(&mut owner);
    tracing::info!(
        "EXPLORER_SEARCH_BATCH {}",
        serde_json::json!({"batches":samples})
    );
}

#[test]
fn recursive_identical_file_names_keep_full_path_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let (_panel, mut view, mut owner) = loaded(dir.path());
    view.search_subfolders("same");
    let (receipt, sender, _) = SearchQuery::test_channel();
    view.find.as_mut().unwrap().search.as_mut().unwrap().receipt = Some(receipt);
    let entries: Vec<_> = (0..5000)
        .map(|i| DirEntryInfo {
            path: dir.path().join(format!("dir{i:06}/same.rs")),
            name: "same.rs".into(),
            ..Default::default()
        })
        .collect();
    for batch in entries.chunks(256) {
        sender.send(SearchEvent::Hits(batch.to_vec())).unwrap();
        owner.drive(|requests| view.poll_find(requests));
    }
    assert_eq!(view.shown_count(), 5000);
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(view.shown_position(&entry.path), Some(i));
    }
    assert_eq!(
        view.shown_position(&dir.path().join("missing/same.rs")),
        None
    );
    finish(&mut owner);
}
