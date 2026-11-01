//! 탐색 결정을 보류할 수 없는 backend(Windows)에서 스캔 결과가 JS 값을 정하는 탐색을 처리한다.
//!
//! WebView2의 `NavigationStarting`에는 결정을 미루는 deferral이 없다. 그래서 그 탐색을 취소하고
//! 작업 스레드에서 파일을 읽은 뒤, UI 스레드에서 표를 확인하고 같은 종류의 탐색으로 다시 시작한다.
//! 다시 시작한 탐색은 파일을 다시 읽지 않고 이미 읽은 결과로 JS를 정한다(docs/surfaces/html/index.md).
//!
//! 이 모듈은 상태 전이만 가진다. native 호출(취소·재시작·JS 적용)은 backend가 한다.
//!
//! 다시 시작한 탐색을 확정할 식별자는 없다. `Navigate`·`Reload`·`GoBack`·`GoForward`는
//! `NavigationId`를 돌려주지 않고, 다시 시작한 탐색의 `IsUserInitiated`도 일정하지 않다(실 기기에서
//! 거짓으로 온 재로드를 관측). 재시작을 기다리는 동안 온 `NavigationStarting`은 주소와 다시 시작한
//! 종류가 맞고, 이 모듈이 취소한 탐색의 ID가 아니며, 그 경로의 파일 판([`FileStamp`])이 스캔한 판과
//! 같을 때만 결과를 받는다. 식별값이 같아도
//! 내용이 같다는 보장은 없으므로, 결과를 받은 탐색의 실제 문서 요청을 확인한다. 그 탐색이
//! commit되기 전까지 같은 파일의 문서 요청을 붙잡고(deferral), 작업 스레드가 파일 전체 지문을 다시
//! 구한다. 결과의 지문과 같으면 요청을 그대로 진행시킨다. 다르면 탐색을 멈추고(`Stop`) 그 탐색이
//! 끝난 뒤 요청을 놓아 주고 같은 종류로 다시 시작해 자기 스캔을 받게 한다. 그래서 결과를 어느
//! 탐색이 받았든 그 탐색이 받는 본문은 결과와 같은 지문이다(docs/surfaces/html/index.md).

use tasty_model::html_script::{Fingerprint, ScriptScan};

use std::path::PathBuf;

use super::file_stamp::FileStamp;
use super::script_gate::{DeferredScan, ReadSlot, Response, ScriptGate, file_url_to_path};

/// `NavigationStarting`이 알려 준 탐색 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    Reload,
    NewDocument,
    BackOrForward,
    /// 구형 런타임 등으로 종류를 읽지 못했다.
    Unknown,
}

/// 호스트가 요청한 히스토리 이동. 취소한 뒤로/앞으로 탐색을 같은 방향으로 다시 시작할 때 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryStep {
    Back,
    Forward,
}

/// 취소한 탐색을 다시 시작하는 방법. 원래 종류를 지켜 재로드가 히스토리 항목을 늘리지 않게 한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replay {
    Reload,
    Navigate,
    Back,
    Forward,
}

/// `NavigationStarting`에서 backend가 할 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// 이 JS 값을 적용하고 탐색을 진행한다.
    Js(bool),
    /// 이 모듈이 다시 시작한 탐색이다. JS 값을 적용하고 진행하되, 탐색 시도는 취소한 원래 탐색에서
    /// 이미 기록했으므로 다시 기록하지 않는다(사용자 제스처 여부를 원래 탐색 것으로 남긴다).
    Resumed(bool),
    /// 탐색을 취소한다. 취소가 실패하면 [`ScanResume::cancel_failed`]를 부른다.
    Cancel,
}

enum State {
    Idle,
    /// 취소한 탐색의 파일을 읽는 중.
    Scanning {
        scan: DeferredScan,
        read: ReadSlot,
        url: String,
        replay: Replay,
    },
    /// 표를 확인했고 다시 시작한 탐색의 `NavigationStarting`을 기다린다. 한 번만 쓴다.
    Resuming {
        url: String,
        replay: Replay,
        kind: StartKind,
        scan: Option<ScriptScan>,
        /// 스캔한 파일 판. 읽는 동안 파일이 바뀌었으면 없고, 그러면 결과를 누구에게도 넘기지 않는다.
        stamp: Option<FileStamp>,
        path: PathBuf,
    },
}

/// 앞 스캔 결과를 받은 탐색의 문서 요청 확인 창. 그 탐색이 commit되거나 끝나거나 더 새 탐색이
/// 시작되면 닫힌다.
struct Verify {
    nav_id: Option<u64>,
    path: PathBuf,
    url: String,
    replay: Replay,
    fingerprint: Fingerprint,
    /// 이 창에서 문서 요청을 하나라도 붙잡았는지.
    seen: bool,
}

/// 지문을 확인하는 동안 붙잡은 문서 요청. `D`는 backend의 deferral이다.
struct Held<D> {
    slot: ReadSlot,
    deferral: D,
    nav_id: Option<u64>,
    fingerprint: Fingerprint,
    /// 이 요청의 탐색이 이미 끝났거나 다른 문서의 commit으로 대체됐다. 본문이 문서에 닿을 수
    /// 없으므로 확인 결과와 관계없이 읽기가 끝나면 놓아 준다.
    ended: bool,
}

/// 문서 요청에서 backend가 할 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestAction {
    /// 확인할 요청이 아니다. 그대로 둔다.
    Ignore,
    /// 붙잡았다. 확인이 끝나면 [`ScanResume::poll_verify`]가 돌려준다.
    Held,
    /// 붙잡지 못해 확인할 수 없다. 탐색을 멈춘다(`Stop`).
    Stop,
}

/// [`ScanResume::poll_verify`]의 결과.
pub struct Verified<D> {
    /// 지금 완료해 진행시킬 요청.
    pub complete: Vec<D>,
    /// 지문이 달라 탐색을 멈춘다(`Stop`). 그 탐색의 완료 신호가 재시작을 준다.
    pub stop: bool,
}

/// 탐색 완료 신호에서 backend가 할 일.
pub struct Ended<D> {
    /// 이 모듈이 취소하거나 멈춘 탐색이다. 로드 종료로 보지 않는다.
    pub internal: bool,
    /// 이제 완료해도 되는 요청. 그 요청의 탐색은 이미 끝났으므로 본문이 전달되지 않는다.
    pub release: Vec<D>,
    /// 멈춘 탐색을 같은 종류로 다시 시작한다.
    pub restart: Option<(Replay, String)>,
}

/// 멈춘 탐색. 그 완료 신호를 기다려 재시작한다.
struct Stopped {
    nav_id: Option<u64>,
    replay: Replay,
    url: String,
}

/// 한 webview의 취소·재시작 상태. `D`는 문서 요청의 deferral이다.
pub struct ScanResume<D> {
    surface_id: u32,
    state: State,
    /// 이 모듈이 취소한 탐색의 NavigationId. 그 완료 신호는 로드 종료로 보지 않는다.
    cancelled: Vec<u64>,
    /// 호스트가 마지막으로 요청한 히스토리 이동. 다음 `NavigationStarting`에서 한 번 쓴다.
    host_step: Option<HistoryStep>,
    /// 재시작했지만 그 재시작으로 알아본 시작 신호가 아직 없는 횟수. 다시 시작한 탐색의 종류가
    /// 예상과 달라 매번 새 탐색으로 보이면 취소와 재시작이 끝나지 않으므로 상한을 둔다.
    unmatched_replays: u32,
    verify: Option<Verify>,
    held: Vec<Held<D>>,
    /// 지문이 달랐거나 창이 닫힌 뒤 확인이 끝난 요청. 그 탐색이 끝날 때까지 놓아 주지 않는다.
    awaiting_end: Vec<(Option<u64>, D)>,
    stopped: Option<Stopped>,
    /// 지문이 연달아 다른 횟수. 파일이 계속 바뀌면 재시작이 끝나지 않으므로 상한을 둔다.
    verify_failures: u32,
    /// 다음 스캔 대상 탐색을 스캔 없이 스크립트 없는 문서로 둔다.
    load_without_scripts: bool,
}

/// 알아보지 못한 재시작이 이만큼 이어지면 더 취소하지 않고 스크립트 없이 진행한다.
const MAX_UNMATCHED_REPLAYS: u32 = 2;

/// 문서 요청의 지문이 이만큼 연달아 다르면 다음 로드를 스크립트 없이 둔다.
const MAX_VERIFY_FAILURES: u32 = 2;

impl Replay {
    /// 이 방법으로 다시 시작한 탐색의 `NavigationStarting` 종류.
    fn start_kind(self) -> StartKind {
        match self {
            Replay::Reload => StartKind::Reload,
            Replay::Navigate => StartKind::NewDocument,
            Replay::Back | Replay::Forward => StartKind::BackOrForward,
        }
    }
}

impl<D> ScanResume<D> {
    pub fn new(surface_id: u32) -> Self {
        Self {
            surface_id,
            state: State::Idle,
            cancelled: Vec::new(),
            host_step: None,
            unmatched_replays: 0,
            verify: None,
            held: Vec::new(),
            awaiting_end: Vec::new(),
            stopped: None,
            verify_failures: 0,
            load_without_scripts: false,
        }
    }

    /// 호스트가 로드·재로드·히스토리 이동을 요청했다. 앞에서 기다리던 재시작은 버린다.
    pub fn host_request(&mut self, step: Option<HistoryStep>) {
        self.clear();
        self.unmatched_replays = 0;
        self.verify_failures = 0;
        self.load_without_scripts = false;
        self.host_step = step;
    }

    /// main frame `NavigationStarting`. 게이트 로드를 시작하고 JS 값이나 취소를 정한다.
    /// `user_initiated`는 `IsUserInitiated`이며 읽지 못했으면 `false`다.
    pub fn start(
        &mut self,
        gate: &ScriptGate,
        url: &str,
        nav_id: Option<u64>,
        kind: StartKind,
        user_initiated: bool,
    ) -> Start {
        let step = self.host_step.take();
        // 새 시작이 앞 탐색의 확인 창과 재시작을 대신한다.
        self.verify = None;
        self.stopped = None;
        if let Some(resumed) = self.resume(gate, url, nav_id, kind, user_initiated) {
            return resumed;
        }
        gate.load_started();
        // 새 탐색이 앞에서 기다리던 스캔과 재시작을 대신한다. load_started 뒤라 그 표는 이미 낡았다.
        self.clear();
        let scan = match gate.main_response(url) {
            Response::Decided(js) => {
                self.unmatched_replays = 0;
                return Start::Js(js);
            }
            Response::Deferred(scan) => scan,
        };
        let Some(scan) = self.unless_loading_without_scripts(scan) else {
            return Start::Js(false);
        };
        // 종류를 모르면 다시 시작한 탐색을 종류로 알아볼 수 없다(구형 런타임).
        let replay = match kind {
            StartKind::Reload => Some(Replay::Reload),
            StartKind::NewDocument => Some(Replay::Navigate),
            StartKind::Unknown => None,
            StartKind::BackOrForward => step.map(|step| match step {
                HistoryStep::Back => Replay::Back,
                HistoryStep::Forward => Replay::Forward,
            }),
        };
        if self.unmatched_replays >= MAX_UNMATCHED_REPLAYS {
            tracing::warn!(
                "WebView surface {}: {} replays in a row were not recognised; \
                 the document loads without scripts",
                self.surface_id,
                self.unmatched_replays
            );
            self.unmatched_replays = 0;
            scan.abandon();
            return Start::Js(false);
        }
        match (nav_id, replay) {
            (Some(id), Some(replay)) => {
                self.cancelled.push(id);
                self.state = State::Scanning {
                    read: scan.spawn_read(),
                    scan,
                    url: url.to_string(),
                    replay,
                };
                Start::Cancel
            }
            _ => {
                // 취소한 탐색을 나중에 구별하거나 같은 방향으로 다시 시작할 수 없다. UI 스레드에서
                // 읽지 않고 지문 없는 문서로 둔다(허용 불가).
                tracing::warn!(
                    "WebView surface {}: cannot resume this navigation after its scan \
                     (id known: {}, kind: {kind:?}); the document loads without scripts",
                    self.surface_id,
                    nav_id.is_some()
                );
                scan.abandon();
                Start::Js(false)
            }
        }
    }

    /// 지문이 연달아 달라 다음 로드를 스크립트 없이 두기로 했으면 스캔을 버리고 `None`을 준다.
    fn unless_loading_without_scripts(&mut self, scan: DeferredScan) -> Option<DeferredScan> {
        if !std::mem::take(&mut self.load_without_scripts) {
            return Some(scan);
        }
        tracing::warn!(
            "WebView surface {}: the file kept changing while its navigation was checked; \
             the document loads without scripts",
            self.surface_id
        );
        scan.abandon();
        None
    }

    /// 다시 시작한 탐색이면 읽어 둔 결과로 JS를 정하고, 그 탐색의 문서 요청 확인 창을 연다.
    fn resume(
        &mut self,
        gate: &ScriptGate,
        url: &str,
        nav_id: Option<u64>,
        kind: StartKind,
        user_initiated: bool,
    ) -> Option<Start> {
        let (scan, replay, path) = self.take_resume(url, nav_id, kind, user_initiated)?;
        self.unmatched_replays = 0;
        gate.load_started();
        self.verify = scan.map(|scan| Verify {
            nav_id,
            path,
            url: url.to_string(),
            replay,
            fingerprint: scan.fingerprint,
            seen: false,
        });
        Some(Start::Resumed(gate.main_response_scanned(url, scan)))
    }

    /// [`Start::Cancel`]을 받았지만 취소하지 못했다. 탐색은 스크립트 없이 진행된다.
    pub fn cancel_failed(&mut self, nav_id: Option<u64>) {
        if let Some(id) = nav_id {
            self.cancelled.retain(|c| *c != id);
        }
        self.clear();
    }

    /// 탐색 완료 신호. `internal`이면 이 모듈이 취소하거나 멈춘 탐색이고, backend는 그 신호를
    /// 로드 종료로 보지 않는다. 끝난 탐색에 붙잡아 둔 요청은 이제 놓아 준다.
    pub fn completed(&mut self, nav_id: Option<u64>) -> Ended<D> {
        let internal = match nav_id {
            Some(id) => match self.cancelled.iter().position(|c| *c == id) {
                Some(at) => {
                    self.cancelled.swap_remove(at);
                    true
                }
                None => false,
            },
            // 식별값을 모르면 멈춘 탐색이 있을 때만 그 탐색의 완료로 본다.
            None => self.stopped.as_ref().is_some_and(|s| s.nav_id.is_none()),
        };
        let restart = match &self.stopped {
            Some(s) if internal && s.nav_id == nav_id => {
                self.stopped.take().map(|s| (s.replay, s.url))
            }
            _ => None,
        };
        if self.verify.as_ref().is_some_and(|v| v.nav_id == nav_id) {
            self.verify = None;
        }
        // 뒤로·앞으로 재시작은 다음 시작 신호가 방향을 알아야 다시 취소·스캔할 수 있다. 호스트 요청과
        // 같이 방향을 남긴다. 남기지 않으면 그 탐색이 스캔 없이 지문 없는 문서가 된다.
        if let Some((replay, _)) = &restart {
            self.host_step = match replay {
                Replay::Back => Some(HistoryStep::Back),
                Replay::Forward => Some(HistoryStep::Forward),
                Replay::Reload | Replay::Navigate => None,
            };
        }
        Ended {
            internal,
            release: self.release_ended(|n| n == nav_id || nav_id.is_none()),
            restart,
        }
    }

    /// 끝났거나 대체된 탐색의 붙잡은 요청을 놓아 준다. 아직 읽는 요청은 끝났다고 표시해 두고,
    /// 읽기가 끝나면 [`Self::poll_verify`]가 결과와 관계없이 놓아 준다.
    fn release_ended(&mut self, ended: impl Fn(Option<u64>) -> bool) -> Vec<D> {
        for held in &mut self.held {
            if ended(held.nav_id) {
                held.ended = true;
            }
        }
        let (done, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.awaiting_end)
            .into_iter()
            .partition(|(n, _)| ended(*n));
        self.awaiting_end = keep;
        done.into_iter().map(|(_, d)| d).collect()
    }

    /// main frame 문서 요청(`WebResourceRequested`의 DOCUMENT 문맥). 확인 창이 열려 있고 같은 파일이면
    /// `defer`로 요청을 붙잡고 작업 스레드에서 지문을 구한다.
    pub fn document_request(
        &mut self,
        gate: &ScriptGate,
        url: &str,
        defer: impl FnOnce() -> Option<D>,
    ) -> RequestAction {
        let Some(verify) = self.verify.as_mut() else {
            return RequestAction::Ignore;
        };
        if file_url_to_path(url).as_deref() != Some(verify.path.as_path()) {
            return RequestAction::Ignore;
        }
        let Some(deferral) = defer() else {
            tracing::warn!(
                "WebView surface {}: could not hold the document request to check it; \
                 stopping the navigation (the body is not held)",
                self.surface_id
            );
            self.begin_stop();
            return RequestAction::Stop;
        };
        verify.seen = true;
        self.held.push(Held {
            slot: gate.spawn_verify(&verify.path),
            deferral,
            nav_id: verify.nav_id,
            fingerprint: verify.fingerprint,
            ended: false,
        });
        RequestAction::Held
    }

    /// 지문 확인이 끝난 요청을 처리한다. UI 스레드에서 부른다.
    pub fn poll_verify(&mut self) -> Verified<D> {
        let mut out = Verified {
            complete: Vec::new(),
            stop: false,
        };
        let mut waiting = Vec::new();
        for held in std::mem::take(&mut self.held) {
            let Some(read) = held.slot.take_stamped() else {
                waiting.push(held);
                continue;
            };
            let same = read.scan.map(|s| s.fingerprint) == Some(held.fingerprint);
            let open = self
                .verify
                .as_ref()
                .is_some_and(|v| v.nav_id == held.nav_id);
            if held.ended {
                out.complete.push(held.deferral);
            } else if same {
                if open {
                    self.verify_failures = 0;
                }
                out.complete.push(held.deferral);
            } else if open {
                tracing::debug!(
                    "WebView surface {}: the document changed after its scan; \
                     stopping the navigation to scan it again",
                    self.surface_id
                );
                self.awaiting_end.push((held.nav_id, held.deferral));
                self.begin_stop();
                out.stop = true;
            } else {
                // 멈춘 탐색의 다른 요청이거나 창이 닫힌 뒤(더 새 시작) 끝난 확인이다. 본문이 결과와
                // 다르므로 그 탐색이 끝날 때까지 놓아 주지 않는다.
                self.awaiting_end.push((held.nav_id, held.deferral));
            }
        }
        self.held = waiting;
        out
    }

    /// main frame commit(`ContentLoading`). 확인 창을 닫는다. 결과를 받은 탐색이 문서 요청을
    /// 거치지 않고 commit했으면 확인하지 못한 본문이므로 `true`를 주고, backend는 탐색을 멈춘다.
    /// 멈춘 탐색은 재로드로 다시 시작한다. commit은 앞 탐색들이 끝났다는 뜻이므로 그 요청을 놓아 준다.
    pub fn content_loading(&mut self, nav_id: Option<u64>) -> (bool, Vec<D>) {
        let mut stop = false;
        // commit으로 창을 닫는다. 요청을 붙잡은 적이 없으면 확인하지 못한 본문이다.
        if let Some(verify) = self
            .verify
            .take_if(|v| v.nav_id == nav_id)
            .filter(|v| !v.seen)
        {
            tracing::warn!(
                "WebView surface {}: the navigation committed without a document request \
                 to check; reloading it",
                self.surface_id
            );
            self.verify = Some(Verify {
                replay: Replay::Reload,
                ..verify
            });
            self.begin_stop();
            stop = true;
        }
        (stop, self.release_ended(|n| n != nav_id))
    }

    /// `Stop` 호출이 실패했다. 멈춘 탐색의 완료 신호가 오지 않을 수 있으므로 재시작을 버린다.
    /// 지문이 달라 붙잡은 요청이 있으면 놓아 주지 않는다. 그 탐색은 요청이 막힌 채 남고, 그 탐색이
    /// 끝나거나 다음 탐색이 commit하면 놓아 준다. 요청을 붙잡지 못했거나 이미 commit한 경우에는
    /// 막힌 본문이 없다. backend는 어느 쪽이든 이 로드를 실패로 끝낸다.
    pub fn stop_failed(&mut self) {
        self.stopped = None;
    }

    /// 확인 창의 탐색을 멈춘 것으로 기록한다. 그 완료 신호는 내부 신호이고 재시작을 준다.
    fn begin_stop(&mut self) {
        let Some(verify) = self.verify.take() else {
            return;
        };
        if let Some(id) = verify.nav_id {
            self.cancelled.push(id);
        }
        self.verify_failures += 1;
        if self.verify_failures > MAX_VERIFY_FAILURES {
            self.verify_failures = 0;
            self.load_without_scripts = true;
        }
        self.stopped = Some(Stopped {
            nav_id: verify.nav_id,
            replay: verify.replay,
            url: verify.url,
        });
    }

    /// 읽기가 끝났으면 표를 확인하고 다시 시작할 탐색을 준다. UI 스레드에서 부른다.
    /// 읽는 중이거나 표가 낡았으면 `None`이다.
    pub fn poll(&mut self) -> Option<(Replay, String)> {
        let read = match &self.state {
            State::Scanning { read, .. } => read.take_stamped()?,
            _ => return None,
        };
        let State::Scanning {
            scan, url, replay, ..
        } = std::mem::replace(&mut self.state, State::Idle)
        else {
            unreachable!("matched above");
        };
        let path = scan.path().to_path_buf();
        // 표 확인과 상태 반영은 다시 시작하기 직전 이 UI 스레드 호출 안에서 한다.
        scan.apply(read.scan)?;
        self.unmatched_replays += 1;
        self.state = State::Resuming {
            url: url.clone(),
            replay,
            kind: replay.start_kind(),
            scan: read.scan,
            stamp: read.stamp,
            path,
        };
        Some((replay, url))
    }

    /// 다시 시작하는 호출이 실패했다. 기다리던 재시작을 버린다.
    pub fn replay_failed(&mut self) {
        self.clear();
    }

    /// 다시 시작한 탐색이면 읽어 둔 결과를 꺼낸다. 주소·종류가 다르거나, 이 모듈이 취소한 탐색의
    /// ID이거나, 경로의 파일 판이 스캔한 판과 다르면 꺼내지 않는다(모듈 설명 참고).
    /// 파일 판 확인은 메타데이터 조회 한 번이며 내용을 읽지 않는다.
    ///
    /// `IsUserInitiated`는 조건이 아니다. WebView2 API로 다시 시작한 탐색도 거짓으로 오는 경우를
    /// Windows 실 기기에서 관측했다(재로드). 사용자 시작 여부는 기록·진단에만 쓰고, 결과가 다른
    /// 내용에 붙지 않는 것은 파일 판과 문서 요청의 지문 확인이 맡는다.
    fn take_resume(
        &mut self,
        url: &str,
        nav_id: Option<u64>,
        kind: StartKind,
        user_initiated: bool,
    ) -> Option<(Option<ScriptScan>, Replay, PathBuf)> {
        let cancelled_id = nav_id.is_some_and(|id| self.cancelled.contains(&id));
        match &self.state {
            State::Resuming {
                url: waiting,
                kind: expected,
                stamp: Some(stamp),
                path,
                ..
            } if waiting == url
                && *expected == kind
                && !cancelled_id
                && FileStamp::of_path(path).as_ref() == Some(stamp) => {}
            State::Resuming {
                url: waiting,
                kind: expected,
                stamp,
                path,
                ..
            } => {
                // 알아보지 못한 재시작은 상한에서 스크립트 없는 로드가 된다. 어느 조건이 어긋났는지 남긴다.
                tracing::debug!(
                    surface_id = self.surface_id,
                    url_matches = waiting == url,
                    ?expected,
                    ?kind,
                    user_initiated,
                    cancelled_id,
                    stamp_read = stamp.is_some(),
                    stamp_matches =
                        stamp.is_some() && FileStamp::of_path(path).as_ref() == stamp.as_ref(),
                    "a start while waiting for a replay was not the replay"
                );
                return None;
            }
            _ => return None,
        }
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Resuming {
                scan, replay, path, ..
            } => Some((scan, replay, path)),
            _ => unreachable!("matched above"),
        }
    }

    /// 스캔이나 재시작을 기다리는 중인지.
    #[cfg(test)]
    fn waiting(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    /// 기다리던 스캔과 재시작을 버린다. 확인 창과 멈춘 탐색의 재시작도 버린다. 붙잡은 요청은
    /// 그 탐색이 끝날 때 놓아 준다.
    pub fn clear(&mut self) {
        self.verify = None;
        self.stopped = None;
        if let State::Scanning { scan, .. } = std::mem::replace(&mut self.state, State::Idle) {
            scan.abandon();
        }
    }
}

#[cfg(test)]
#[path = "scan_resume/tests.rs"]
mod tests;
