use super::*;
use tasty_ipc::file_download::CHUNK_BYTES;

const WS: u32 = 3;

fn epoch() -> ConnectionEpoch {
    tasty_remote::connection::channel().0.epoch()
}

fn view(epoch: &ConnectionEpoch, capable: bool, connected: bool) -> SessionView {
    SessionView {
        workspace: WS,
        capable,
        connected,
        epoch: epoch.clone(),
    }
}

/// 보낼 요청을 서버 처리부로 답해 결과를 돌려준다. 한 번의 메인 루프 회전과 같다.
fn turn(client: &mut Client, epoch: &ConnectionEpoch) -> usize {
    let sends = client.sync(&[view(epoch, true, true)]);
    let n = sends.len();
    for (_, msg) in sends {
        let FileDownloadMsg::FileDownloadRequest {
            request_id,
            path,
            op,
        } = msg
        else {
            panic!("client sent a result");
        };
        let outcome = crate::remote::file_download::answer(Path::new(&path), &op);
        client.receive(epoch, request_id, outcome);
    }
    n
}

fn run_until_idle(client: &mut Client, epoch: &ConnectionEpoch) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..100 {
        if turn(client, epoch) == 0 {
            break;
        }
        events.extend(client.take_events());
    }
    events.extend(client.take_events());
    events
}

fn fixture(size: u64) -> (tempfile::TempDir, PathBuf, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("remote.bin");
    let content: Vec<u8> = (0..size).map(|i| (i * 7 % 256) as u8).collect();
    std::fs::write(&path, &content).unwrap();
    (dir, path, content)
}

fn runs(root: &Path) -> usize {
    std::fs::read_dir(root).map_or(0, |entries| entries.count())
}

#[test]
fn exactly_one_mib_downloads_without_asking_into_a_run_folder() {
    let (_remote, path, content) = fixture(CONFIRM_ABOVE_BYTES);
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path().to_path_buf());
    let epoch = epoch();
    client.sync(&[view(&epoch, true, true)]);
    let id = client
        .start(WS, path.to_string_lossy().into_owned())
        .unwrap();
    let events = run_until_idle(&mut client, &epoch);
    let [
        Event::Done {
            id: done,
            workspace,
            path: local,
        },
    ] = events.as_slice()
    else {
        panic!("expected one Done without Confirm, got {events:?}");
    };
    assert_eq!(*done, id);
    assert_eq!(*workspace, WS);
    assert_eq!(local.file_name().unwrap(), "remote.bin");
    assert_eq!(local.parent().unwrap().parent().unwrap(), root.path());
    assert!(client.owns(local));
    assert_eq!(std::fs::read(local).unwrap(), content);
}

#[test]
fn one_byte_over_asks_once_and_sends_no_chunk_before_the_answer() {
    let (_remote, path, content) = fixture(CONFIRM_ABOVE_BYTES + 1);
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path().to_path_buf());
    let epoch = epoch();
    client.sync(&[view(&epoch, true, true)]);
    let id = client
        .start(WS, path.to_string_lossy().into_owned())
        .unwrap();
    let events = run_until_idle(&mut client, &epoch);
    assert_eq!(
        events,
        vec![Event::Confirm {
            id,
            name: "remote.bin".into(),
            size: CONFIRM_ABOVE_BYTES + 1
        }]
    );
    assert!(client.sync(&[view(&epoch, true, true)]).is_empty());
    assert_eq!(runs(root.path()), 0, "nothing is written before the answer");
    client.confirm(id);
    let events = run_until_idle(&mut client, &epoch);
    let [Event::Done { path: local, .. }] = events.as_slice() else {
        panic!("confirmed download must finish without asking again, got {events:?}");
    };
    assert_eq!(std::fs::read(local).unwrap(), content);
}

#[test]
fn cancelling_at_the_question_sends_nothing_and_leaves_no_file() {
    let (_remote, path, _) = fixture(CONFIRM_ABOVE_BYTES + 1);
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path().to_path_buf());
    let epoch = epoch();
    client.sync(&[view(&epoch, true, true)]);
    let id = client
        .start(WS, path.to_string_lossy().into_owned())
        .unwrap();
    run_until_idle(&mut client, &epoch);
    client.cancel(id);
    assert_eq!(
        client.take_events(),
        vec![Event::Failed {
            id,
            failure: Failure::Cancelled
        }]
    );
    assert!(client.sync(&[view(&epoch, true, true)]).is_empty());
    assert_eq!(runs(root.path()), 0);
}

#[test]
fn cancelling_mid_transfer_stops_requests_and_removes_the_partial_file() {
    let (_remote, path, _) = fixture(CHUNK_BYTES * 3);
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path().to_path_buf());
    let epoch = epoch();
    client.sync(&[view(&epoch, true, true)]);
    let id = client
        .start(WS, path.to_string_lossy().into_owned())
        .unwrap();
    turn(&mut client, &epoch); // stat
    turn(&mut client, &epoch); // first chunk
    assert_eq!(runs(root.path()), 1);
    client.cancel(id);
    assert!(client.sync(&[view(&epoch, true, true)]).is_empty());
    assert_eq!(runs(root.path()), 0);
}

#[test]
fn folders_are_refused_without_creating_anything() {
    let remote = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path().to_path_buf());
    let epoch = epoch();
    client.sync(&[view(&epoch, true, true)]);
    let id = client
        .start(WS, remote.path().to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(
        run_until_idle(&mut client, &epoch),
        vec![Event::Failed {
            id,
            failure: Failure::Refused(DownloadRefusal::Directory)
        }]
    );
    assert_eq!(runs(root.path()), 0);
}

#[test]
fn a_server_name_with_path_parts_stays_inside_the_run_folder() {
    for name in ["../../x", "..\\..\\x", "/etc/x", "x"] {
        assert_eq!(safe_file_name(name), "x", "{name}");
    }
    for name in ["", "..", ".", "a/..", "dir/"] {
        assert_eq!(safe_file_name(name), "download", "{name:?}");
    }
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path().to_path_buf());
    let epoch = epoch();
    let sends = {
        client.sync(&[view(&epoch, true, true)]);
        client.start(WS, "/remote/x".into()).unwrap();
        client.sync(&[view(&epoch, true, true)])
    };
    let FileDownloadMsg::FileDownloadRequest { request_id, .. } = &sends[0].1 else {
        panic!();
    };
    client.receive(
        &epoch,
        *request_id,
        DownloadOutcome::Stat {
            name: "../../escaped".into(),
            identity: FileIdentity {
                size: 1,
                modified_ns: None,
                file_id: None,
            },
        },
    );
    let sends = client.sync(&[view(&epoch, true, true)]);
    let FileDownloadMsg::FileDownloadRequest { request_id, .. } = &sends[0].1 else {
        panic!();
    };
    client.receive(
        &epoch,
        *request_id,
        DownloadOutcome::Chunk {
            offset: 0,
            data_b64: "eA==".into(),
            eof: true,
        },
    );
    let events = client.take_events();
    let [Event::Done { path, .. }] = events.as_slice() else {
        panic!("{events:?}");
    };
    assert_eq!(path.file_name().unwrap(), "escaped");
    assert_eq!(path.parent().unwrap().parent().unwrap(), root.path());
    assert!(!root.path().parent().unwrap().join("escaped").exists());
}

#[test]
fn a_lost_or_replaced_connection_fails_and_removes_the_partial_file() {
    let (_remote, path, _) = fixture(CHUNK_BYTES * 3);
    for replace in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut client = Client::new(root.path().to_path_buf());
        let first = epoch();
        client.sync(&[view(&first, true, true)]);
        let id = client
            .start(WS, path.to_string_lossy().into_owned())
            .unwrap();
        turn(&mut client, &first); // stat
        turn(&mut client, &first); // first chunk
        // 다음 조각을 보낸 뒤 연결이 바뀌거나 끊긴다.
        let sends = client.sync(&[view(&first, true, true)]);
        assert_eq!(sends.len(), 1);
        let after = if replace {
            vec![view(&epoch(), true, true)]
        } else {
            Vec::new()
        };
        client.sync(&after);
        assert_eq!(
            client.take_events(),
            vec![Event::Failed {
                id,
                failure: Failure::Disconnected
            }],
            "replace={replace}"
        );
        assert_eq!(runs(root.path()), 0);
        // 옛 연결의 늦은 결과는 버린다.
        let FileDownloadMsg::FileDownloadRequest { request_id, .. } = &sends[0].1 else {
            panic!();
        };
        client.receive(
            &first,
            *request_id,
            DownloadOutcome::Chunk {
                offset: CHUNK_BYTES,
                data_b64: String::new(),
                eof: false,
            },
        );
        assert!(client.take_events().is_empty());
    }
}

#[test]
fn a_file_changed_during_the_transfer_fails_and_removes_the_partial_file() {
    let (_remote, path, _) = fixture(CHUNK_BYTES * 3);
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path().to_path_buf());
    let epoch = epoch();
    client.sync(&[view(&epoch, true, true)]);
    let id = client
        .start(WS, path.to_string_lossy().into_owned())
        .unwrap();
    turn(&mut client, &epoch); // stat
    turn(&mut client, &epoch); // first chunk
    std::fs::write(&path, b"replaced").unwrap();
    let events = run_until_idle(&mut client, &epoch);
    assert_eq!(
        events,
        vec![Event::Failed {
            id,
            failure: Failure::Refused(DownloadRefusal::Changed)
        }]
    );
    assert_eq!(runs(root.path()), 0);
}

#[test]
fn an_old_server_or_a_missing_connection_refuses_at_start() {
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path().to_path_buf());
    let epoch = epoch();
    assert_eq!(client.start(WS, "/a".into()), Err(Failure::Disconnected));
    client.sync(&[view(&epoch, false, true)]);
    assert_eq!(client.start(WS, "/a".into()), Err(Failure::Unsupported));
    client.sync(&[view(&epoch, true, false)]);
    assert_eq!(client.start(WS, "/a".into()), Err(Failure::Disconnected));
}

#[test]
fn starting_a_download_removes_run_folders_older_than_a_day() {
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("old-run");
    let fresh = root.path().join("fresh-run");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&fresh).unwrap();
    let client = Client::new(root.path().to_path_buf());
    let later = SystemTime::now() + RUN_KEEP + Duration::from_secs(60);
    client.sweep_old_runs(later);
    assert!(!old.exists());
    assert!(!fresh.exists(), "both are older than a day at `later`");
    std::fs::create_dir_all(&fresh).unwrap();
    client.sweep_old_runs(SystemTime::now());
    assert!(fresh.exists());
}
