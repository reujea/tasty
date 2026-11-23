use super::*;

fn collect(root: &Path, query: &str, stop: bool) -> Vec<SearchEvent> {
    collect_hidden(root, query, stop, false)
}

fn collect_hidden(root: &Path, query: &str, stop: bool, show_hidden: bool) -> Vec<SearchEvent> {
    let (sender, receiver) = mpsc::sync_channel(SEARCH_EVENT_CAPACITY);
    let spec = SearchSpec {
        id: 0,
        root: root.into(),
        query: query.into(),
        show_hidden,
        stop: Arc::new(AtomicBool::new(stop)),
    };
    let worker = std::thread::spawn(move || run(spec, sender, &|| {}));
    let events = receiver.iter().collect();
    worker.join().unwrap();
    events
}

fn hits(events: &[SearchEvent]) -> Vec<String> {
    let mut names: Vec<String> = events
        .iter()
        .flat_map(|e| match e {
            SearchEvent::Hits(h) => h.iter().map(|e| e.name.clone()).collect(),
            _ => Vec::new(),
        })
        .collect();
    names.sort();
    names
}

#[test]
fn subfolders_are_searched_case_insensitively_and_the_end_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("Documents").join("finance");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(dir.path().join("Report.pdf"), b"").unwrap();
    std::fs::write(deep.join("q3-report.xlsx"), b"").unwrap();
    std::fs::write(deep.join("notes.md"), b"").unwrap();
    let events = collect(dir.path(), "report", false);
    assert_eq!(hits(&events), ["Report.pdf", "q3-report.xlsx"]);
    assert!(matches!(
        events.last(),
        Some(SearchEvent::Done {
            stopped: false,
            capped: false
        })
    ));
    assert_eq!(
        relative_folder(dir.path(), &deep.join("q3-report.xlsx")),
        Path::new("Documents").join("finance").display().to_string()
    );
    assert_eq!(
        relative_folder(dir.path(), &dir.path().join("Report.pdf")),
        "."
    );
}

#[test]
fn a_stopped_search_reads_nothing_more_and_an_unreadable_start_fails() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("report"), b"").unwrap();
    let events = collect(dir.path(), "report", true);
    assert!(hits(&events).is_empty());
    assert!(matches!(
        events.last(),
        Some(SearchEvent::Done {
            stopped: true,
            capped: false
        })
    ));
    let events = collect(&dir.path().join("missing"), "report", false);
    assert!(matches!(events.as_slice(), [SearchEvent::Failed(_)]));
}

#[cfg(unix)]
#[test]
fn unreadable_subfolders_are_skipped_and_linked_folders_are_not_entered() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join("private");
    std::fs::create_dir(&private).unwrap();
    std::fs::write(private.join("report"), b"").unwrap();
    std::fs::write(dir.path().join("report.txt"), b"").unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("loop")).unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o000)).unwrap();
    // root 는 권한과 관계없이 읽으므로 그때는 건너뛴 폴더가 생기지 않는다.
    let readable_anyway = std::fs::read_dir(&private).is_ok();
    let events = collect(dir.path(), "report", false);
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o755)).unwrap();
    let skipped: Vec<&PathBuf> = events
        .iter()
        .filter_map(|e| match e {
            SearchEvent::Skipped(p) => Some(p),
            _ => None,
        })
        .collect();
    if !readable_anyway {
        assert_eq!(skipped, [&private]);
        // 링크 "loop" 를 따라 들어갔다면 report.txt 가 두 번 나온다.
        assert_eq!(hits(&events), ["report.txt"]);
    }
    assert!(matches!(
        events.last(),
        Some(SearchEvent::Done {
            stopped: false,
            capped: false
        })
    ));
}

#[test]
fn a_search_stops_at_the_hit_cap_and_keeps_what_it_found() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..=SEARCH_MAX_HITS {
        std::fs::write(dir.path().join(format!("report-{i}")), b"").unwrap();
    }
    let events = collect(dir.path(), "report", false);
    assert_eq!(hits(&events).len(), SEARCH_MAX_HITS);
    assert!(matches!(
        events.last(),
        Some(SearchEvent::Done {
            stopped: false,
            capped: true
        })
    ));
}

#[test]
fn hidden_folders_are_pruned_before_descent_and_extensions_filter_only_hits() {
    let dir = tempfile::tempdir().unwrap();
    for folder in [".hidden", "visible", "visible/.nested", "folder.rs"] {
        std::fs::create_dir_all(dir.path().join(folder)).unwrap();
    }
    for file in [
        ".secret.rs",
        ".hidden/note.rs",
        "visible/.nested/deep.rs",
        "visible/note.RS",
        "visible/note.md",
        "visible/other.txt",
        "folder.rs/child.rs",
    ] {
        std::fs::write(dir.path().join(file), b"").unwrap();
    }
    assert_eq!(
        hits(&collect_hidden(dir.path(), "ext:rs", false, false)),
        ["child.rs", "note.RS"]
    );
    assert_eq!(
        hits(&collect_hidden(dir.path(), "ext:rs", false, true)),
        [".secret.rs", "child.rs", "deep.rs", "note.RS", "note.rs"]
    );
    assert_eq!(
        hits(&collect_hidden(
            dir.path(),
            "NOTE ext:rs ext:md",
            false,
            false
        )),
        ["note.RS", "note.md"]
    );
}

fn full_receipt() -> (SearchQuery, mpsc::SyncSender<SearchEvent>) {
    let (receipt, sender, _) = SearchQuery::test_channel();
    for i in 0..SEARCH_EVENT_CAPACITY {
        sender
            .send(SearchEvent::Skipped(format!("queued-{i}").into()))
            .unwrap();
    }
    assert!(matches!(
        sender.try_send(SearchEvent::Skipped("extra".into())),
        Err(mpsc::TrySendError::Full(_))
    ));
    (receipt, sender)
}

#[test]
fn stop_on_a_full_channel_releases_the_worker_without_a_consumer_and_reports_done() {
    let (mut receipt, sender) = full_receipt();
    let stop = Arc::clone(&receipt.stop);
    let (started, ready) = mpsc::sync_channel(1);
    let (done, finished) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let signaled = AtomicBool::new(false);
        let sent = send_event(
            &sender,
            &stop,
            &|| {
                if !signaled.swap(true, Ordering::Relaxed) {
                    started.send(()).unwrap();
                }
            },
            SearchEvent::Hits(Vec::new()),
        );
        done.send(sent).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    receipt.stop();
    assert!(!finished.recv_timeout(Duration::from_secs(2)).unwrap());
    worker.join().unwrap();
    let mut requests = super::super::ReadRequests {
        pending: Vec::new(),
        available: 0,
    };
    let first = receipt.poll(&mut requests);
    assert_eq!(first.len(), SEARCH_EVENT_CAPACITY);
    assert!(matches!(
        receipt.poll(&mut requests).as_slice(),
        [SearchEvent::Done {
            stopped: true,
            capped: false
        }]
    ));
    assert!(receipt.poll(&mut requests).is_empty());
}

#[test]
fn closing_a_full_receipt_releases_the_worker_without_draining() {
    let (receipt, sender) = full_receipt();
    let stop = Arc::clone(&receipt.stop);
    let (ready, started) = mpsc::sync_channel(1);
    let (done, finished) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let signaled = AtomicBool::new(false);
        let sent = send_event(
            &sender,
            &stop,
            &|| {
                if !signaled.swap(true, Ordering::Relaxed) {
                    ready.send(()).unwrap();
                }
            },
            SearchEvent::Done {
                stopped: false,
                capped: false,
            },
        );
        done.send(sent).unwrap();
    });
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(receipt);
    assert!(!finished.recv_timeout(Duration::from_secs(2)).unwrap());
    worker.join().unwrap();
}

#[test]
fn a_slow_consumer_gets_every_event_in_order_under_backpressure() {
    let (mut receipt, sender, stop) = SearchQuery::test_channel();
    let worker = std::thread::spawn(move || {
        for i in 0..100 {
            assert!(send_event(
                &sender,
                &stop,
                &|| {},
                SearchEvent::Skipped(format!("{i}").into())
            ));
        }
        assert!(send_event(
            &sender,
            &stop,
            &|| {},
            SearchEvent::Done {
                stopped: false,
                capped: false
            }
        ));
    });
    let mut requests = super::super::ReadRequests {
        pending: Vec::new(),
        available: 0,
    };
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    // 용량 0 요청 상자로 poll 하므로 이미 보낸 사건만 꺼낸다. 기한 뒤 관측에서도 꺼낸 뒤 읽는다.
    let mut collect = |receipt: &mut SearchQuery| {
        let batch = receipt.poll(&mut requests);
        assert!(batch.len() <= SEARCH_EVENT_CAPACITY);
        for event in batch {
            if let SearchEvent::Skipped(path) = event {
                seen.push(path);
            }
        }
    };
    assert!(tasty_utils::test_wait::holds_by_every(
        deadline,
        Duration::from_millis(1),
        |last| {
            if last {
                collect(&mut receipt);
            }
            receipt.terminal_seen || {
                if !last {
                    collect(&mut receipt);
                }
                false
            }
        }
    ));
    worker.join().unwrap();
    assert_eq!(
        seen,
        (0..100)
            .map(|i| PathBuf::from(format!("{i}")))
            .collect::<Vec<_>>()
    );
}

#[test]
fn search_hit_batches_are_bounded_independently_of_the_directory_size() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..SEARCH_BATCH_HITS * 3 + 1 {
        std::fs::write(dir.path().join(format!("item{i}")), b"").unwrap();
    }
    let events = collect(dir.path(), "item", false);
    let sizes: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SearchEvent::Hits(batch) => Some(batch.len()),
            _ => None,
        })
        .collect();
    assert!(sizes.len() >= 4);
    assert!(sizes.iter().all(|&n| n <= SEARCH_BATCH_HITS));
    assert_eq!(sizes.iter().sum::<usize>(), SEARCH_BATCH_HITS * 3 + 1);
}
