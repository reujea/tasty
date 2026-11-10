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
