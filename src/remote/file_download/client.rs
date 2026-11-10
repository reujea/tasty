//! client 쪽 원격 파일 다운로드. 사용자가 시작한 다운로드 하나마다 서버에 `stat` 을 묻고, 1 MiB 를 넘으면
//! 확인을 받은 뒤 조각을 차례로 요청해 실행별 임시 폴더에 원래 파일 이름으로 쓴다.
//!
//! 메인 루프가 [`Client::sync`] 로 요청을 보내고 결과를 [`Client::receive`] 로 넘긴다. 결과는 보낸 연결
//! 세대와 같을 때만 받는다. 연결이 끊기거나 바뀌거나 서버가 거절하거나 사용자가 취소하면 임시 폴더를
//! 지우고 [`Event::Failed`] 를 낸다. 끝까지 받으면 [`Event::Done`] 으로 로컬 경로를 넘긴다.
//! 받은 파일은 재사용하지 않는다. 같은 파일도 다운로드마다 새로 받는다.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use base64::Engine;
use tasty_ipc::file_download::{
    CONFIRM_ABOVE_BYTES, DownloadOp, DownloadOutcome, DownloadRefusal, FileDownloadMsg,
    FileIdentity,
};
use tasty_remote::connection::ConnectionEpoch;

/// Tasty 가 관리하는 임시 위치의 폴더 이름. 데이터 홈 아래에 둔다.
pub(crate) const RUN_ROOT_DIR: &str = "download-run";
/// 이보다 오래된 실행별 폴더는 다음 다운로드를 시작할 때 지운다.
pub(crate) const RUN_KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// 다운로드가 끝나지 못한 이유.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    /// 서버가 이 기능을 알리지 않은 옛 Tasty 다.
    Unsupported,
    /// 연결이 없거나, 요청 뒤 끊기거나 다른 연결로 바뀌었다.
    Disconnected,
    Refused(DownloadRefusal),
    /// 서버 응답이 형식에 맞지 않는다.
    Protocol,
    /// client 임시 폴더에 쓰지 못했다.
    LocalWrite,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    /// 크기가 [`CONFIRM_ABOVE_BYTES`] 를 넘는다. [`Client::confirm`] 전에는 조각을 요청하지 않는다.
    Confirm {
        id: u64,
        name: String,
        size: u64,
    },
    /// `workspace` 는 다운로드를 시작한 mirror workspace 의 로컬 ID 다.
    Done {
        id: u64,
        workspace: u32,
        path: PathBuf,
    },
    Failed {
        id: u64,
        failure: Failure,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Stat,
    Confirm,
    Read { offset: u64 },
}

struct Download {
    workspace: u32,
    path: String,
    phase: Phase,
    /// 서버가 준 이름에서 경로 구성 요소를 지운 값. `stat` 결과 전에는 비어 있다.
    name: String,
    identity: Option<FileIdentity>,
    run_dir: Option<PathBuf>,
    file: Option<(std::fs::File, PathBuf)>,
    sent: Option<(u64, ConnectionEpoch)>,
}

/// 한 세션의 연결 상태. 메인 루프가 세션마다 매번 알린다.
pub(crate) struct SessionView {
    pub workspace: u32,
    pub capable: bool,
    pub connected: bool,
    pub epoch: ConnectionEpoch,
}

pub(crate) struct Client {
    root: PathBuf,
    next_id: u64,
    next_request: u64,
    sessions: HashMap<u32, (bool, bool)>,
    downloads: HashMap<u64, Download>,
    events: Vec<Event>,
}

static CLIENT: LazyLock<Mutex<Client>> = LazyLock::new(|| Mutex::new(Client::new(run_root())));

/// 받은 파일을 두는 폴더. 데이터 홈을 못 찾는 환경에서는 OS 임시 폴더 아래에 둔다.
pub(crate) fn run_root() -> PathBuf {
    tasty_utils::path::tasty_home()
        // 이유: 이 폴더는 실행별 하위 폴더(`<ms>-<pid>-<id>`)만 담고, 오래된 실행 폴더 정리를 위해
        // 같은 데이터 홈의 인스턴스가 함께 쓴다.
        .unwrap_or_else(|| std::env::temp_dir().join("tasty-data"))
        .join(RUN_ROOT_DIR)
}

pub(crate) fn client() -> MutexGuard<'static, Client> {
    CLIENT.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("remote file download state was poisoned; keeping its entries");
        poisoned.into_inner()
    })
}

/// 서버가 준 이름에서 경로 구성 요소를 지운다. 남는 것이 없으면 고정 이름을 쓴다.
pub(crate) fn safe_file_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .filter(|c| !(c.is_control() || cfg!(windows) && "<>:\"|?*".contains(*c)))
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        "download".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 다운로드가 만든 파일인지. 받은 파일을 여는 핸들러가 탭을 어디에 만들지 정할 때 쓴다.
pub(crate) fn is_downloaded_file(path: &Path) -> bool {
    inside(&run_root(), path)
}

/// `..` 로 폴더 밖을 가리키는 경로는 안으로 보지 않는다.
fn inside(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

impl Client {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            next_id: 0,
            next_request: 0,
            sessions: HashMap::new(),
            downloads: HashMap::new(),
            events: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn owns(&self, path: &Path) -> bool {
        inside(&self.root, path)
    }

    /// 다운로드를 시작한다. 서버가 기능을 알리지 않았거나 연결되지 않았으면 바로 실패를 돌려준다.
    #[cfg_attr(
        not(any(debug_assertions, test)),
        expect(
            dead_code,
            reason = "the download button waits for a design; until then only the debug IPC starts, confirms and cancels downloads"
        )
    )]
    pub(crate) fn start(&mut self, workspace: u32, path: String) -> Result<u64, Failure> {
        match self.sessions.get(&workspace).copied() {
            None | Some((true, false)) => return Err(Failure::Disconnected),
            Some((false, _)) => return Err(Failure::Unsupported),
            Some((true, true)) => {}
        }
        self.sweep_old_runs(SystemTime::now());
        self.next_id += 1;
        let id = self.next_id;
        self.downloads.insert(
            id,
            Download {
                workspace,
                path,
                phase: Phase::Stat,
                name: String::new(),
                identity: None,
                run_dir: None,
                file: None,
                sent: None,
            },
        );
        Ok(id)
    }

    /// 1 MiB 를 넘는 파일의 확인. 이후 다시 묻지 않고 끝까지 받는다.
    #[cfg_attr(
        not(any(debug_assertions, test)),
        expect(
            dead_code,
            reason = "the download button waits for a design; until then only the debug IPC starts, confirms and cancels downloads"
        )
    )]
    pub(crate) fn confirm(&mut self, id: u64) {
        let Some(d) = self.downloads.get_mut(&id) else {
            return;
        };
        if d.phase == Phase::Confirm {
            d.phase = Phase::Read { offset: 0 };
            if let Err(failure) = Self::open_target(&self.root, id, d) {
                self.fail(id, failure);
            }
        }
    }

    /// 진행 중인 다운로드를 멈춘다. 다음 조각을 요청하지 않으며 받은 부분과 임시 폴더를 지운다.
    #[cfg_attr(
        not(any(debug_assertions, test)),
        expect(
            dead_code,
            reason = "the download button waits for a design; until then only the debug IPC starts, confirms and cancels downloads"
        )
    )]
    pub(crate) fn cancel(&mut self, id: u64) {
        if self.downloads.contains_key(&id) {
            self.fail(id, Failure::Cancelled);
        }
    }

    pub(crate) fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// 세션 목록을 갱신하고 끊긴 세션의 다운로드를 끝낸다. 보낼 요청을 (workspace, 메시지)로 돌려준다.
    pub(crate) fn sync(&mut self, sessions: &[SessionView]) -> Vec<(u32, FileDownloadMsg)> {
        self.sessions = sessions
            .iter()
            .map(|s| (s.workspace, (s.capable, s.connected)))
            .collect();
        let mut out = Vec::new();
        let mut failed = Vec::new();
        let ids: Vec<u64> = self.downloads.keys().copied().collect();
        for id in ids {
            let d = self.downloads.get_mut(&id).expect("id from keys");
            let Some(view) = sessions.iter().find(|s| s.workspace == d.workspace) else {
                failed.push((id, Failure::Disconnected));
                continue;
            };
            if let Some((_, epoch)) = &d.sent {
                if !view.connected || !epoch.same(&view.epoch) {
                    failed.push((id, Failure::Disconnected));
                }
                continue;
            }
            if !view.connected {
                failed.push((id, Failure::Disconnected));
                continue;
            }
            let op = match (d.phase, &d.identity) {
                (Phase::Stat, _) => DownloadOp::Stat,
                (Phase::Read { offset }, Some(identity)) => DownloadOp::Read {
                    offset,
                    identity: identity.clone(),
                },
                (Phase::Confirm, _) | (Phase::Read { .. }, None) => continue,
            };
            self.next_request += 1;
            let request_id = self.next_request;
            out.push((
                d.workspace,
                FileDownloadMsg::FileDownloadRequest {
                    request_id,
                    path: d.path.clone(),
                    op,
                },
            ));
            d.sent = Some((request_id, view.epoch.clone()));
        }
        for (id, failure) in failed {
            self.fail(id, failure);
        }
        out
    }

    /// 보낸 요청이 실패하면 그 다운로드를 끝낸다.
    pub(crate) fn send_failed(&mut self, request_id: u64) {
        if let Some(id) = self.by_request(request_id) {
            self.fail(id, Failure::Disconnected);
        }
    }

    /// 결과를 받는다. 보낸 연결 세대와 다르거나 기다리던 요청이 아니면 버린다.
    pub(crate) fn receive(
        &mut self,
        epoch: &ConnectionEpoch,
        request_id: u64,
        outcome: DownloadOutcome,
    ) {
        let Some(id) = self.by_request(request_id) else {
            return;
        };
        let d = self.downloads.get_mut(&id).expect("found by request");
        if !d.sent.as_ref().is_some_and(|(_, sent)| sent.same(epoch)) {
            return;
        }
        d.sent = None;
        let result = match (d.phase, outcome) {
            (_, DownloadOutcome::Refused { reason }) => Err(Failure::Refused(reason)),
            (Phase::Stat, DownloadOutcome::Stat { name, identity }) => {
                let size = identity.size;
                d.name = safe_file_name(&name);
                d.identity = Some(identity);
                if size > CONFIRM_ABOVE_BYTES {
                    // 확인 전에는 폴더도 만들지 않는다.
                    d.phase = Phase::Confirm;
                    self.events.push(Event::Confirm {
                        id,
                        name: d.name.clone(),
                        size,
                    });
                    Ok(None)
                } else {
                    d.phase = Phase::Read { offset: 0 };
                    Self::open_target(&self.root, id, d).map(|()| None)
                }
            }
            (
                Phase::Read { offset },
                DownloadOutcome::Chunk {
                    offset: at,
                    data_b64,
                    eof,
                },
            ) if at == offset => Self::write_chunk(d, offset, &data_b64, eof),
            _ => Err(Failure::Protocol),
        };
        match result {
            Ok(Some(path)) => {
                let workspace = self.downloads.remove(&id).map_or(0, |d| d.workspace);
                self.events.push(Event::Done {
                    id,
                    workspace,
                    path,
                });
            }
            Ok(None) => {}
            Err(failure) => self.fail(id, failure),
        }
    }

    fn write_chunk(
        d: &mut Download,
        offset: u64,
        data_b64: &str,
        eof: bool,
    ) -> Result<Option<PathBuf>, Failure> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_b64)
            .map_err(|_| Failure::Protocol)?;
        let size = d.identity.as_ref().map_or(0, |i| i.size);
        let next = offset + bytes.len() as u64;
        if next > size || (eof != (next == size)) || (bytes.is_empty() && !eof) {
            return Err(Failure::Protocol);
        }
        let (file, path) = d.file.as_mut().ok_or(Failure::LocalWrite)?;
        file.write_all(&bytes).map_err(|_| Failure::LocalWrite)?;
        if !eof {
            d.phase = Phase::Read { offset: next };
            return Ok(None);
        }
        file.sync_all().map_err(|_| Failure::LocalWrite)?;
        Ok(Some(path.clone()))
    }

    /// 실행별 폴더를 만들고 원래 이름의 파일을 새로 만든다.
    fn open_target(root: &Path, id: u64, d: &mut Download) -> Result<(), Failure> {
        let stamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_millis());
        let run_dir = root.join(format!("{stamp}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&run_dir).map_err(|_| Failure::LocalWrite)?;
        d.run_dir = Some(run_dir.clone());
        let path = run_dir.join(&d.name);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| Failure::LocalWrite)?;
        d.file = Some((file, path));
        Ok(())
    }

    fn by_request(&self, request_id: u64) -> Option<u64> {
        self.downloads
            .iter()
            .find(|(_, d)| d.sent.as_ref().is_some_and(|(r, _)| *r == request_id))
            .map(|(id, _)| *id)
    }

    fn fail(&mut self, id: u64, failure: Failure) {
        if let Some(d) = self.downloads.remove(&id) {
            drop(d.file);
            if let Some(dir) = d.run_dir
                && let Err(error) = std::fs::remove_dir_all(&dir)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(dir = %dir.display(), "could not remove a failed download: {error}");
            }
            self.events.push(Event::Failed { id, failure });
        }
    }

    /// 다른 다운로드가 쓰는 폴더는 남기고 [`RUN_KEEP`] 보다 오래된 실행별 폴더를 지운다.
    fn sweep_old_runs(&self, now: SystemTime) {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return;
        };
        let active: Vec<&PathBuf> = self
            .downloads
            .values()
            .filter_map(|d| d.run_dir.as_ref())
            .collect();
        for entry in entries.flatten() {
            let path = entry.path();
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age > RUN_KEEP));
            if old
                && !active.contains(&&path)
                && let Err(error) = std::fs::remove_dir_all(&path)
            {
                tracing::warn!(dir = %path.display(), "could not remove an old download: {error}");
            }
        }
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
