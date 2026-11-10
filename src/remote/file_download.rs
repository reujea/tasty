//! attach holder 의 원격 파일 하나 가져오기(`file_download_request`)의 서버 쪽.
//! 규칙은 [Attach 메커니즘](../../docs/dev-guide/attach-behavior.md)의 "원격 파일 다운로드" 절을 따른다.
//!
//! 서버는 요청 사이에 상태를 두지 않는다. 일반 파일만 보내며 폴더·FIFO·장치와 폴더를 가리키는 링크는
//! 파일을 열기 전에 거절한다. 파일을 가리키는 링크는 대상 파일을 보낸다. 조각마다 `stat` 이 준 식별 정보와
//! 지금 파일을 대조해 전송 중 교체·변경을 [`DownloadRefusal::Changed`] 로 돌려준다.
//! client 쪽 대기·임시 파일은 [`client`] 에 있다.

#[cfg(feature = "gui")]
pub(crate) mod client;

use base64::Engine;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use tasty_ipc::file_download::{
    CHUNK_BYTES, DownloadOp, DownloadOutcome, DownloadRefusal, FileDownloadMsg, FileIdentity,
};
use tasty_ipc::stream::{StreamFrame, StreamTag};
use tasty_ipc::stream_hub::StreamHub;

/// 요청에 답한다. `holds` 는 이 연결이 이 인스턴스의 workspace 를 하나라도 점유했는가다.
/// 디렉터리 조회와 같이 경로를 점유 workspace 안으로 제한하지 않는다.
pub(crate) fn handle(hub: &StreamHub, client_id: u32, holds: bool, msg: &FileDownloadMsg) {
    let FileDownloadMsg::FileDownloadRequest {
        request_id,
        path,
        op,
    } = msg
    else {
        return;
    };
    let outcome = if holds {
        answer(Path::new(path), op)
    } else {
        refused(DownloadRefusal::NotAttached)
    };
    let reply = FileDownloadMsg::FileDownloadResult {
        request_id: *request_id,
        outcome,
    };
    match serde_json::to_vec(&reply) {
        Ok(bytes) => {
            // 손실·끊김은 허브가 처리한다. client 는 연결 세대가 바뀌면 전송을 실패로 끝낸다.
            let _ = hub.push(client_id, StreamFrame::new(StreamTag::Control, bytes));
        }
        Err(error) => tracing::warn!("file download reply did not serialize: {error}"),
    }
}

pub(crate) fn answer(path: &Path, op: &DownloadOp) -> DownloadOutcome {
    match op {
        DownloadOp::Stat => stat(path),
        DownloadOp::Read { offset, identity } => {
            read(path, *offset, identity).unwrap_or_else(refused)
        }
    }
}

fn refused(reason: DownloadRefusal) -> DownloadOutcome {
    DownloadOutcome::Refused { reason }
}

fn stat(path: &Path) -> DownloadOutcome {
    match regular_metadata(path) {
        Ok(meta) => DownloadOutcome::Stat {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            identity: identity(&meta),
        },
        Err(reason) => refused(reason),
    }
}

/// 링크를 따라간 대상이 일반 파일일 때만 메타데이터를 돌려준다. 파일을 열지 않는다.
fn regular_metadata(path: &Path) -> Result<std::fs::Metadata, DownloadRefusal> {
    let meta = std::fs::metadata(path).map_err(|e| io_refusal(&e))?;
    if meta.is_dir() {
        Err(DownloadRefusal::Directory)
    } else if !meta.is_file() {
        Err(DownloadRefusal::NotRegular)
    } else {
        Ok(meta)
    }
}

fn read(
    path: &Path,
    offset: u64,
    expected: &FileIdentity,
) -> Result<DownloadOutcome, DownloadRefusal> {
    // 여는 동안 다른 종류로 바뀌어도 FIFO 에서 멈추지 않도록 먼저 확인하고 열린 파일을 다시 확인한다.
    let before = regular_metadata(path).map_err(changed_unless_missing)?;
    if identity(&before) != *expected {
        return Err(DownloadRefusal::Changed);
    }
    let mut file = open_without_blocking(path).map_err(|e| io_refusal(&e))?;
    let same = |file: &std::fs::File| {
        file.metadata()
            .is_ok_and(|meta| meta.is_file() && identity(&meta) == *expected)
    };
    if !same(&file) || offset > expected.size {
        return Err(DownloadRefusal::Changed);
    }
    let want = (expected.size - offset).min(CHUNK_BYTES);
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| DownloadRefusal::Io)?;
    let mut bytes = Vec::with_capacity(want as usize);
    file.by_ref()
        .take(want)
        .read_to_end(&mut bytes)
        .map_err(|_| DownloadRefusal::Io)?;
    // 줄어든 파일이나 읽는 동안 바뀐 파일은 보낸 앞부분과 이어지지 않는다.
    if bytes.len() as u64 != want || !same(&file) {
        return Err(DownloadRefusal::Changed);
    }
    Ok(DownloadOutcome::Chunk {
        offset,
        data_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        eof: offset + want >= expected.size,
    })
}

/// 첫 응답 뒤 경로가 사라졌거나 폴더로 바뀌었으면 전송 중 변경이다.
fn changed_unless_missing(reason: DownloadRefusal) -> DownloadRefusal {
    match reason {
        DownloadRefusal::PermissionDenied | DownloadRefusal::Io => reason,
        _ => DownloadRefusal::Changed,
    }
}

fn io_refusal(error: &std::io::Error) -> DownloadRefusal {
    match error.kind() {
        std::io::ErrorKind::NotFound => DownloadRefusal::NotFound,
        std::io::ErrorKind::PermissionDenied => DownloadRefusal::PermissionDenied,
        _ => DownloadRefusal::Io,
    }
}

#[cfg(unix)]
fn open_without_blocking(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
fn open_without_blocking(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

fn identity(meta: &std::fs::Metadata) -> FileIdentity {
    FileIdentity {
        size: meta.len(),
        modified_ns: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)),
        file_id: file_id(meta),
    }
}

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_id(_meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
#[path = "file_download/tests.rs"]
mod tests;
