use super::answer;
use base64::Engine;
use std::path::Path;
use tasty_ipc::file_download::{
    CHUNK_BYTES, DownloadOp, DownloadOutcome, DownloadRefusal, FileIdentity,
};

fn stat(path: &Path) -> Result<(String, FileIdentity), DownloadRefusal> {
    match answer(path, &DownloadOp::Stat) {
        DownloadOutcome::Stat { name, identity } => Ok((name, identity)),
        DownloadOutcome::Refused { reason } => Err(reason),
        other => panic!("unexpected stat outcome {other:?}"),
    }
}

/// stat 뒤 끝까지 조각을 이어 받는다. 거절되면 그 이유를 돌려준다.
fn fetch(path: &Path) -> Result<Vec<u8>, DownloadRefusal> {
    let (_, identity) = stat(path)?;
    let mut out = Vec::new();
    loop {
        let op = DownloadOp::Read {
            offset: out.len() as u64,
            identity: identity.clone(),
        };
        match answer(path, &op) {
            DownloadOutcome::Chunk {
                offset,
                data_b64,
                eof,
            } => {
                assert_eq!(offset, out.len() as u64);
                out.extend(
                    base64::engine::general_purpose::STANDARD
                        .decode(data_b64)
                        .unwrap(),
                );
                if eof {
                    return Ok(out);
                }
            }
            DownloadOutcome::Refused { reason } => return Err(reason),
            other => panic!("unexpected read outcome {other:?}"),
        }
    }
}

#[test]
fn a_regular_file_arrives_whole_across_several_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.bin");
    let content: Vec<u8> = (0..(CHUNK_BYTES * 2 + 17))
        .map(|i| (i % 251) as u8)
        .collect();
    std::fs::write(&path, &content).unwrap();
    let (name, identity) = stat(&path).unwrap();
    assert_eq!(name, "big.bin");
    assert_eq!(identity.size, content.len() as u64);
    assert_eq!(fetch(&path).unwrap(), content);
}

#[test]
fn an_empty_file_is_one_final_empty_chunk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty");
    std::fs::write(&path, b"").unwrap();
    assert_eq!(fetch(&path).unwrap(), Vec::<u8>::new());
}

#[test]
fn folders_and_links_to_folders_are_refused_before_reading() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(stat(dir.path()), Err(DownloadRefusal::Directory));
    #[cfg(unix)]
    {
        let link = dir.path().join("to-dir");
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        assert_eq!(stat(&link), Err(DownloadRefusal::Directory));
    }
}

#[cfg(unix)]
#[test]
fn a_link_to_a_file_sends_the_target_under_the_link_name() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.txt");
    std::fs::write(&target, b"linked content").unwrap();
    let link = dir.path().join("alias.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let (name, identity) = stat(&link).unwrap();
    assert_eq!(name, "alias.txt");
    assert_eq!(identity.size, 14);
    assert_eq!(fetch(&link).unwrap(), b"linked content");
}

#[cfg(unix)]
#[test]
fn a_fifo_is_refused_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("pipe");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    assert_eq!(stat(&fifo), Err(DownloadRefusal::NotRegular));
    let identity = FileIdentity {
        size: 0,
        modified_ns: None,
        created_ns: None,
        file_id: None,
    };
    assert_eq!(
        answer(
            &fifo,
            &DownloadOp::Read {
                offset: 0,
                identity
            }
        ),
        DownloadOutcome::Refused {
            reason: DownloadRefusal::Changed
        }
    );
}

#[test]
fn a_missing_path_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        stat(&dir.path().join("nope")),
        Err(DownloadRefusal::NotFound)
    );
}

#[test]
fn a_file_changed_or_replaced_after_stat_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.txt");
    std::fs::write(&path, b"first").unwrap();
    let (_, identity) = stat(&path).unwrap();
    let read = |identity: &FileIdentity| {
        answer(
            &path,
            &DownloadOp::Read {
                offset: 0,
                identity: identity.clone(),
            },
        )
    };
    // 크기가 바뀐 경우.
    std::fs::write(&path, b"first and more").unwrap();
    assert_eq!(
        read(&identity),
        DownloadOutcome::Refused {
            reason: DownloadRefusal::Changed
        }
    );
    // 같은 크기의 다른 파일로 교체된 경우.
    let (_, identity) = stat(&path).unwrap();
    let other = dir.path().join("other.txt");
    std::fs::write(&other, b"FIRST AND MORE").unwrap();
    std::fs::rename(&other, &path).unwrap();
    if cfg!(unix) {
        assert_eq!(
            read(&identity),
            DownloadOutcome::Refused {
                reason: DownloadRefusal::Changed
            }
        );
    }
    // 사라진 경우.
    let (_, identity) = stat(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        read(&identity),
        DownloadOutcome::Refused {
            reason: DownloadRefusal::Changed
        }
    );
}

mod worker {
    use super::super::{handle, handle_with};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use tasty_ipc::file_download::{DownloadOp, DownloadOutcome, DownloadRefusal, FileDownloadMsg};
    use tasty_ipc::stream_hub::StreamHub;

    static ENTERED: AtomicBool = AtomicBool::new(false);
    static RELEASE: AtomicBool = AtomicBool::new(false);

    /// 응답 없는 마운트처럼 풀어 줄 때까지 돌아오지 않는 읽기.
    fn hanging_reader(_: &std::path::Path, _: &DownloadOp) -> DownloadOutcome {
        ENTERED.store(true, Ordering::Release);
        while !RELEASE.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        DownloadOutcome::Refused {
            reason: DownloadRefusal::NotFound,
        }
    }

    fn request(request_id: u64, path: &str) -> FileDownloadMsg {
        FileDownloadMsg::FileDownloadRequest {
            request_id,
            path: path.into(),
            op: DownloadOp::Stat,
        }
    }

    fn result_of(frame: tasty_ipc::stream::StreamFrame) -> (u64, DownloadOutcome) {
        match serde_json::from_slice(&frame.payload).unwrap() {
            FileDownloadMsg::FileDownloadResult {
                request_id,
                outcome,
            } => (request_id, outcome),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_hanging_read_does_not_block_the_caller_and_answers_when_it_returns() {
        let hub = StreamHub::new();
        let rx = hub.register(41);
        // 처리 함수가 읽기를 기다리면 시험이 멈추지 않고 아래 단언으로 실패하도록 3초 뒤 풀어 준다.
        let watchdog = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(3));
            RELEASE.store(true, Ordering::Release);
        });
        let begun = Instant::now();
        handle_with(&hub, 41, true, &request(7, "/hung/mount/f"), hanging_reader);
        // 처리 함수는 읽기를 기다리지 않고 돌아온다(메인 루프가 다음 일을 한다).
        let waited = begun.elapsed();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ENTERED.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ENTERED.load(Ordering::Acquire), "the worker never started");
        assert!(waited < Duration::from_secs(1), "handle waited {waited:?}");
        assert!(rx.try_recv().is_err(), "no answer while the read hangs");
        RELEASE.store(true, Ordering::Release);
        watchdog.join().unwrap();
        let frame = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            result_of(frame),
            (
                7,
                DownloadOutcome::Refused {
                    reason: DownloadRefusal::NotFound
                }
            )
        );
    }

    #[test]
    fn a_real_read_answers_through_the_worker_and_a_non_holder_is_refused_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, b"abc").unwrap();
        let hub = StreamHub::new();
        let rx = hub.register(42);
        handle(&hub, 42, true, &request(1, &path.to_string_lossy()));
        let (id, outcome) = result_of(rx.recv_timeout(Duration::from_secs(5)).unwrap());
        assert_eq!(id, 1);
        assert!(
            matches!(outcome, DownloadOutcome::Stat { ref name, .. } if name == "f.txt"),
            "{outcome:?}"
        );
        handle(&hub, 42, false, &request(2, &path.to_string_lossy()));
        assert_eq!(
            result_of(rx.try_recv().expect("the refusal is sent without a worker")),
            (
                2,
                DownloadOutcome::Refused {
                    reason: DownloadRefusal::NotAttached
                }
            )
        );
    }
}

/// 생성 시각도 같은 파일인지 판단하는 값이다. Windows 처럼 (device, inode) 가 없는 서버에서는 같은 크기·수정
/// 시각의 새 파일을 이 값으로 알아본다. 여기서는 (device, inode) 는 그대로 두고 생성 시각만 다르게 한다.
#[test]
fn a_different_creation_time_is_a_changed_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.txt");
    std::fs::write(&path, b"same").unwrap();
    let (_, identity) = stat(&path).unwrap();
    // 서버가 OS 의 생성 시각을 그대로 싣는다(파일시스템이 주지 않으면 둘 다 비어 있다).
    assert_eq!(
        identity.created_ns,
        super::unix_ns(std::fs::metadata(&path).unwrap().created())
    );
    let read = |identity: &FileIdentity| {
        answer(
            &path,
            &DownloadOp::Read {
                offset: 0,
                identity: identity.clone(),
            },
        )
    };
    assert!(matches!(read(&identity), DownloadOutcome::Chunk { .. }));
    let mut replaced = identity.clone();
    replaced.created_ns = Some(identity.created_ns.map_or(1, |c| c + 1));
    assert_eq!(
        read(&replaced),
        DownloadOutcome::Refused {
            reason: DownloadRefusal::Changed
        }
    );
}
