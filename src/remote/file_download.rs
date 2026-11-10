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
use std::path::{Path, PathBuf};
use std::sync::Weak;
use std::sync::atomic::{AtomicUsize, Ordering};
use tasty_ipc::file_download::{
    CHUNK_BYTES, DownloadOp, DownloadOutcome, DownloadRefusal, FileDownloadMsg, FileIdentity,
};
use tasty_ipc::stream::{StreamFrame, StreamTag};
use tasty_ipc::stream_hub::StreamHub;

/// 동시에 파일을 읽는 worker 상한. 응답 없는 마운트에 걸린 worker 는 끝날 때까지 자리를 차지한다.
const WORKER_CAP: usize = 8;
static WORKERS: AtomicUsize = AtomicUsize::new(0);

/// worker 자리 하나. 놓으면 자리를 돌려준다.
struct WorkerPermit;

impl WorkerPermit {
    fn acquire() -> Option<Self> {
        WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < WORKER_CAP).then_some(n + 1)
            })
            .ok()
            .map(|_| Self)
    }
}

impl Drop for WorkerPermit {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// 파일을 읽어 결과를 만드는 함수. 시험은 블록되는 가짜를 넣는다.
type Reader = fn(&Path, &DownloadOp) -> DownloadOutcome;

/// 요청에 답한다. `holds` 는 이 연결이 이 인스턴스의 workspace 를 하나라도 점유했는가다.
/// 디렉터리 조회와 같이 경로를 점유 workspace 안으로 제한하지 않는다.
///
/// 메인 루프에서 부른다. 파일 메타데이터·열기·읽기는 요청마다 worker 스레드에서 하고 결과 프레임만
/// 허브로 보낸다. 응답 없는 네트워크 마운트에서도 메인 루프는 멈추지 않는다. client 는 다운로드마다
/// 요청을 하나씩만 보내므로 요청 순서와 응답 대응은 `request_id` 로 그대로 유지된다.
pub(crate) fn handle(hub: &StreamHub, client_id: u32, holds: bool, msg: &FileDownloadMsg) {
    handle_with(hub, client_id, holds, msg, answer);
}

fn handle_with(
    hub: &StreamHub,
    client_id: u32,
    holds: bool,
    msg: &FileDownloadMsg,
    reader: Reader,
) {
    let FileDownloadMsg::FileDownloadRequest {
        request_id,
        path,
        op,
    } = msg
    else {
        return;
    };
    let request_id = *request_id;
    if !holds {
        reply(
            hub,
            client_id,
            None,
            request_id,
            refused(DownloadRefusal::NotAttached),
        );
        return;
    }
    // 연결이 이미 사라졌으면 답할 곳이 없다.
    let Some(binding) = hub.client_binding(client_id) else {
        return;
    };
    let Some(permit) = WorkerPermit::acquire() else {
        tracing::warn!("file download workers are all busy; refusing a request");
        reply(
            hub,
            client_id,
            Some(&binding),
            request_id,
            refused(DownloadRefusal::Io),
        );
        return;
    };
    let worker_hub = hub.clone();
    let worker_binding = binding.clone();
    let path = PathBuf::from(path);
    let op = op.clone();
    let spawned = std::thread::Builder::new()
        .name("file-download".into())
        .spawn(move || {
            let _permit = permit;
            let outcome = reader(&path, &op);
            reply(
                &worker_hub,
                client_id,
                Some(&worker_binding),
                request_id,
                outcome,
            );
        });
    // 이유: worker 는 join 하지 않는다. 걸린 마운트에서 끝나지 않을 수 있고, 끝나면 자리를 스스로 돌려준다.
    if let Err(error) = spawned {
        tracing::warn!(%error, "file download worker did not start");
        reply(
            hub,
            client_id,
            Some(&binding),
            request_id,
            refused(DownloadRefusal::Io),
        );
    }
}

/// 결과 프레임 하나를 보낸다. `binding` 이 있으면 그 연결이 그대로일 때만 보낸다.
fn reply(
    hub: &StreamHub,
    client_id: u32,
    binding: Option<&Weak<()>>,
    request_id: u64,
    outcome: DownloadOutcome,
) {
    let reply = FileDownloadMsg::FileDownloadResult {
        request_id,
        outcome,
    };
    match serde_json::to_vec(&reply) {
        Ok(bytes) => {
            let frame = StreamFrame::new(StreamTag::Control, bytes);
            // 결과를 돌려받지 않는다. 허브가 프레임을 버리면(Dropped) client 는 Loss 알림으로 그 연결의
            // 다운로드를 끝내고, 알림도 받지 못하면 요청별 응답 상한(REPLY_LIMIT)으로 끝낸다. 연결이
            // 끊기거나 바뀌면 client 의 연결 세대가 바뀌어 끝난다.
            let _ = match binding {
                Some(binding) => hub.push_bound(client_id, binding, frame),
                None => hub.push(client_id, frame),
            };
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
        modified_ns: unix_ns(meta.modified()),
        created_ns: unix_ns(meta.created()),
        file_id: file_id(meta),
    }
}

/// 시각을 UNIX 시각 이후 나노초로 바꾼다. 얻을 수 없으면 비운다.
fn unix_ns(at: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    at.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
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
