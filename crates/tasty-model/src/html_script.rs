//! html surface 문서의 스크립트 감지 결과와 문서 단위 허용 상태.
//!
//! 규칙은 docs/surfaces/html/index.md을 따른다. 허용은 main frame 문서의 URL(fragment 제외)과
//! 로드 때 구한 파일 전체 지문에 묶이고, 다른 main frame 문서가 commit되면 풀린다.
//! backend의 navigation 콜백이 아래 `on_*` 메서드를 순서대로 부르고 돌려받은 값으로 JS를 켜거나 끈다.

pub mod banner;
pub mod remote;
pub mod scan;
pub mod top_level;

pub use banner::{BannerPhase, ScriptMarker};
pub use remote::{RemoteAllowError, RemoteMarker, RemoteView};

/// 문서 원본 전체의 SHA-256 지문.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint(pub [u8; 32]);

/// 원본 스캔으로 판정한 실행 가능한 스크립트의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptDetection {
    /// 실행 가능한 스크립트가 없다. 상한 뒤에만 있는 경우도 여기에 속한다.
    #[default]
    None,
    /// 로컬 또는 inline 스크립트, 이벤트 핸들러 속성, `javascript:` URL이 있다.
    Scripts,
    /// `http(s)` `src` 스크립트만 있다. 원격 콘텐츠가 차단돼 있으면 허용해도 실행되지 않는다.
    ScriptsRemoteOnly,
}

impl ScriptDetection {
    pub fn has_scripts(self) -> bool {
        !matches!(self, Self::None)
    }

    /// 조회 응답에 쓰는 이름.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Scripts => "scripts",
            Self::ScriptsRemoteOnly => "scripts_remote_only",
        }
    }
}

impl Fingerprint {
    /// 소문자 16진수 64자.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// 한 번의 원본 읽기로 구한 감지 결과와 지문.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptScan {
    pub fingerprint: Fingerprint,
    pub detection: ScriptDetection,
    /// 원격(`http(s)`) 리소스 참조가 있다. 상한 뒤의 참조, 외부 CSS 안의 참조, 런타임에 넣은 요소는 모른다.
    pub remote_refs: bool,
    /// 원격 `src` 스크립트가 있다. 원격을 막으면 허용해도 이 스크립트는 실행되지 않는다.
    pub remote_scripts: bool,
}

/// 마지막 main frame commit에서 기록한 화면 문서.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRecord {
    /// fragment를 뺀 URL.
    pub url: String,
    /// 응답 단계 없이 commit된 문서나 `file://`이 아닌 문서는 `None`이다. 이 문서는 허용할 수 없다.
    pub scan: Option<ScriptScan>,
}

/// 허용 기록. 허용 클릭 때의 현재 문서를 그대로 옮긴다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowance {
    pub url: String,
    pub fingerprint: Fingerprint,
}

/// 응답 단계에서 시작한 원본 스캔의 식별자. 늦게 끝난 스캔이 다른 로드에 붙지 않도록 로드마다 새로 받는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanTicket(u64);

/// 응답 단계의 스캔 결과. 스캔이 끝나지 않았으면 그 표를 들고 있다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanSlot {
    Done(Option<ScriptScan>),
    Waiting(ScanTicket),
}

/// 응답 단계의 결과. 로드 시작 때 비우고 commit에서 소비한다.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingResponse {
    url: String,
    scan: ScanSlot,
    matched: bool,
    /// 원격 허용 기록과 같은 문서다. 이 로드의 원격 차단을 푼다.
    matched_remote: bool,
}

/// 스캔이 끝나기 전에 commit된 문서. 같은 URL의 재로드면 이전 문서의 배너 표지를 잠정 유지하고,
/// 스캔 결과가 이전 문서와 다르면 그때 새 문서의 표지로 바꾼다.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScanWait {
    ticket: ScanTicket,
    previous: Option<DocumentRecord>,
    /// 새 문서로 판정될 때 쓸 열람 표지(commit 시점에 계산).
    viewed_if_new: bool,
}

/// 배너 표시에 필요한 문서 단위 표지. 판정 규칙은 [`banner`] 모듈에 있다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BannerFlags {
    /// 사용자가 이 문서를 봤다. 사용자 선택이나 사용자가 보던 문서의 페이지 안 이동으로 선다.
    pub viewed: bool,
    /// 감지 결과는 있지만 사용자가 아직 이 문서를 보지 않았다.
    pub pending_view: bool,
    /// 이 문서에서 배너를 띄웠다.
    pub shown: bool,
    /// 사용자가 이 문서의 배너를 닫았다.
    pub dismissed: bool,
}

/// 허용할 수 없는 이유.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowError {
    /// 아직 commit된 문서가 없다.
    NoDocument,
    /// 현재 문서의 지문이 없다(`file://`이 아니거나 응답 단계 없이 commit됐다).
    NoFingerprint,
    /// 새 문서를 로드하는 중이고 아직 commit 전이다. 허용은 commit된 문서에만 묶는다.
    Loading,
}

impl std::fmt::Display for AllowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDocument => f.write_str("no document has been committed yet"),
            Self::NoFingerprint => f.write_str(
                "the current document has no content fingerprint (not a file:// document, \
                 or it was committed without a response decision)",
            ),
            Self::Loading => {
                f.write_str("a new document is loading; allowing waits until it commits")
            }
        }
    }
}

/// surface 하나의 스크립트 허용 상태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlScriptState {
    /// 전역 "Sandbox scripts" 설정. 꺼져 있으면 모든 문서에서 JS를 켠다.
    sandbox: bool,
    current: Option<DocumentRecord>,
    allowance: Option<Allowance>,
    pending: Option<PendingResponse>,
    /// 현재 문서의 스캔이 아직 끝나지 않았다.
    current_wait: Option<ScanWait>,
    next_ticket: u64,
    /// 마지막 로드 시작 뒤 끝나지 않았다.
    load_in_flight: bool,
    /// 마지막 로드 시작 뒤 main frame commit이 있었다.
    committed_since_start: bool,
    /// 허용 직후 호스트가 같은 문서를 다시 로드해야 한다.
    reload_requested: bool,
    /// 허용 뒤 재로드가 끝나지 않았다. 배너가 재로드 중 상태를 보인다.
    reloading_after_allow: bool,
    /// 다음 로드는 호스트가 URL을 넣은 로드다(plugin·IPC·복원). 이전 문서의 열람을 이어받지 않는다.
    host_requested_load: bool,
    /// 문서가 아직 없거나 로드 중일 때 사용자가 이 surface를 선택했다. 다음 commit 문서를 본 것으로 한다.
    viewed_before_commit: bool,
    /// 마지막 로드가 문서를 commit하지 못하고 실패했다. 다음 commit까지 배너와 탭 표지를 내린다.
    load_failed: bool,
    banner: BannerFlags,
    /// 원격 리소스 차단 상태. 스크립트 허용과 독립이다.
    remote: remote::RemoteState,
}

impl Default for HtmlScriptState {
    fn default() -> Self {
        Self::new(true)
    }
}

/// fragment를 뺀 URL. 같은 문서인지 비교할 때 쓴다.
pub fn strip_fragment(url: &str) -> &str {
    url.split_once('#').map_or(url, |(base, _)| base)
}

impl HtmlScriptState {
    pub fn new(sandbox: bool) -> Self {
        Self {
            sandbox,
            current: None,
            allowance: None,
            pending: None,
            current_wait: None,
            next_ticket: 0,
            load_in_flight: false,
            committed_since_start: false,
            reload_requested: false,
            reloading_after_allow: false,
            host_requested_load: false,
            viewed_before_commit: false,
            load_failed: false,
            banner: BannerFlags::default(),
            remote: remote::RemoteState::default(),
        }
    }

    pub fn sandbox(&self) -> bool {
        self.sandbox
    }

    pub fn current(&self) -> Option<&DocumentRecord> {
        self.current.as_ref()
    }

    pub fn allowance(&self) -> Option<&Allowance> {
        self.allowance.as_ref()
    }

    pub fn banner(&self) -> BannerFlags {
        self.banner
    }

    pub fn banner_mut(&mut self) -> &mut BannerFlags {
        &mut self.banner
    }

    /// 현재 문서의 감지 결과. 문서나 지문이 없으면 `None`.
    pub fn current_detection(&self) -> Option<ScriptDetection> {
        self.current.as_ref()?.scan.map(|s| s.detection)
    }

    /// 배너·탭 표지에 쓰는 감지 결과. 같은 URL의 재로드 스캔이 끝나기 전에는 이전 문서의 결과를 쓴다.
    pub(crate) fn display_detection(&self) -> Option<ScriptDetection> {
        self.display_scan().map(|s| s.detection)
    }

    /// 배너·표지에 쓰는 스캔 결과. 규칙은 [`Self::display_detection`]과 같다.
    pub(crate) fn display_scan(&self) -> Option<ScriptScan> {
        match &self.current_wait {
            Some(wait) => wait
                .previous
                .as_ref()
                .filter(|prev| self.current.as_ref().is_some_and(|cur| cur.url == prev.url))
                .and_then(|prev| prev.scan),
            None => self.current.as_ref()?.scan,
        }
    }

    /// 현재 문서의 스캔이 끝나지 않았다. 이 동안 허용을 받지 않는다.
    pub fn scan_pending(&self) -> bool {
        self.current_wait.is_some()
            || self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.scan, ScanSlot::Waiting(_)))
    }

    /// 로드나 스캔이 끝나지 않아 허용 판정을 아직 할 수 없다.
    pub fn awaiting_verdict(&self) -> bool {
        self.loading_before_commit() || self.current_wait.is_some()
    }

    /// 이 URL의 응답에서 JS나 원격 차단 값을 정하려면 스캔 결과가 먼저 있어야 하는지.
    /// sandbox가 켜져 있고 같은 URL에 스크립트 허용 기록이 있거나, 같은 URL에 원격 허용 기록이 있을 때만
    /// 그렇다. 그 밖에는 결과와 관계없이 두 값이 같다.
    pub fn needs_scan_before_js(&self, url: &str) -> bool {
        let url = strip_fragment(url);
        let same = |a: &Option<Allowance>| a.as_ref().is_some_and(|a| a.url == url);
        // 원격 허용은 전역 설정이 켜져 있어도 기다린다. 결정 없이 commit하면 허용 기록이 풀려, 전역을
        // 다시 껐을 때 같은 문서가 막힌다(허용 수명은 문서 단위다).
        (self.sandbox && same(&self.allowance)) || same(&self.remote.allowance)
    }

    /// 현재 문서가 허용 기록과 같은 문서인지.
    pub fn current_is_allowed(&self) -> bool {
        match (&self.current, &self.allowance) {
            (Some(doc), Some(allow)) => doc
                .scan
                .is_some_and(|s| doc.url == allow.url && s.fingerprint == allow.fingerprint),
            _ => false,
        }
    }

    /// 로드가 시작됐고 아직 main frame commit 전이다. 이 동안 허용을 받지 않는다.
    pub fn loading_before_commit(&self) -> bool {
        self.load_in_flight && !self.committed_since_start
    }

    /// 지금 webview에 적용할 JS 값.
    ///
    /// 로드가 진행 중이고 아직 commit 전이면 응답 단계의 판단(없으면 끔)을 따른다.
    /// 그렇지 않으면 화면 문서의 허용 여부를 따른다.
    pub fn effective_js(&self) -> bool {
        if !self.sandbox {
            return true;
        }
        if self.loading_before_commit() {
            return self.pending.as_ref().is_some_and(|p| p.matched);
        }
        self.current_is_allowed()
    }

    /// 전역 sandbox 설정을 바꾸고 적용할 JS 값을 돌려준다.
    pub fn set_sandbox(&mut self, sandbox: bool) -> bool {
        self.sandbox = sandbox;
        self.effective_js()
    }

    /// main frame 로드가 시작됐다. 응답 단계의 이전 결과를 비운다.
    pub fn on_load_started(&mut self) -> bool {
        self.pending = None;
        self.load_in_flight = true;
        self.committed_since_start = false;
        self.effective_js()
    }

    /// main frame main resource의 응답 결정. `scan`은 `file://` 문서에서만 있다.
    pub fn on_main_response(&mut self, url: &str, scan: Option<ScriptScan>) -> bool {
        let url = strip_fragment(url).to_string();
        let matched = matches(&self.allowance, &url, scan);
        let matched_remote = matches(&self.remote.allowance, &url, scan);
        self.record_response(url, ScanSlot::Done(scan), matched, matched_remote)
    }

    /// 스캔을 다른 스레드에서 하는 응답 결정. 결과는 돌려준 표로 [`Self::on_scan_finished`]에 넘긴다.
    /// 결과가 오기 전에는 허용과 맞지 않는 문서로 다룬다.
    pub fn on_main_response_pending(&mut self, url: &str) -> (bool, ScanTicket) {
        self.next_ticket += 1;
        let ticket = ScanTicket(self.next_ticket);
        let url = strip_fragment(url).to_string();
        let js = self.record_response(url, ScanSlot::Waiting(ticket), false, false);
        (js, ticket)
    }

    fn record_response(
        &mut self,
        url: String,
        scan: ScanSlot,
        matched: bool,
        matched_remote: bool,
    ) -> bool {
        self.pending = Some(PendingResponse {
            url,
            scan,
            matched,
            matched_remote,
        });
        // 응답 단계가 로드 시작 신호보다 먼저 올 수 없지만, 신호를 놓쳐도 이 로드의 판단을 따른다.
        self.load_in_flight = true;
        self.effective_js()
    }

    /// 다른 스레드의 스캔이 끝났다. 표가 아직 commit 전 응답이나 현재 문서에 속하면 결과를 붙이고
    /// 적용할 JS 값을 준다. 새 로드·실패·다른 commit으로 표가 낡았으면 버리고 `None`을 준다.
    pub fn on_scan_finished(
        &mut self,
        ticket: ScanTicket,
        scan: Option<ScriptScan>,
    ) -> Option<bool> {
        if let Some(pending) = &self.pending
            && pending.scan == ScanSlot::Waiting(ticket)
        {
            let matched = matches(&self.allowance, &pending.url, scan);
            let matched_remote = matches(&self.remote.allowance, &pending.url, scan);
            let pending = self.pending.as_mut().expect("checked above");
            pending.scan = ScanSlot::Done(scan);
            pending.matched = matched;
            pending.matched_remote = matched_remote;
            return Some(self.effective_js());
        }
        let wait = self.current_wait.take_if(|w| w.ticket == ticket)?;
        let current = self
            .current
            .as_mut()
            .expect("a scan wait belongs to a committed document");
        current.scan = scan;
        if wait.previous.as_ref() != Some(&*current) {
            self.reset_document_flags(wait.viewed_if_new);
        }
        Some(self.effective_js())
    }

    /// main frame 문서가 commit됐다. 응답 단계 결과를 소비해 현재 문서로 기록한다.
    ///
    /// 응답 단계가 없었으면(bfcache 복원 등) `committed_url`로 지문 없는 문서를 기록한다.
    pub fn on_committed(&mut self, committed_url: Option<&str>) -> bool {
        let pending = self.pending.take();
        let matched = pending.as_ref().is_some_and(|p| p.matched);
        if !matched {
            self.allowance = None;
        }
        if !pending.as_ref().is_some_and(|p| p.matched_remote) {
            self.remote.allowance = None;
        }
        // 차단 관측은 commit된 문서의 하위 리소스 요청에서 다시 쌓는다.
        self.remote.observed = false;
        let (record, waiting) = match pending {
            Some(p) => {
                let (scan, waiting) = match p.scan {
                    ScanSlot::Done(scan) => (scan, None),
                    ScanSlot::Waiting(ticket) => (None, Some(ticket)),
                };
                (Some(DocumentRecord { url: p.url, scan }), waiting)
            }
            None => (
                committed_url.map(|u| DocumentRecord {
                    url: strip_fragment(u).to_string(),
                    scan: None,
                }),
                None,
            ),
        };
        let viewed_if_new =
            self.viewed_before_commit || (self.banner.viewed && !self.host_requested_load);
        let same_url = matches!(
            (&self.current, &record),
            (Some(a), Some(b)) if a.url == b.url
        );
        let same_document = match waiting {
            // 스캔이 끝나야 같은 문서인지 알 수 있다. 같은 URL이면 표지를 잠정 유지한다.
            Some(_) => same_url,
            None => matches!((&self.current, &record), (Some(a), Some(b)) if a == b),
        };
        let previous = std::mem::replace(&mut self.current, record);
        self.current_wait = waiting.map(|ticket| ScanWait {
            ticket,
            previous: previous.filter(|_| same_url),
            viewed_if_new,
        });
        if !same_document {
            self.reset_document_flags(viewed_if_new);
        }
        self.end_load_marks();
        self.load_failed = false;
        self.committed_since_start = true;
        self.effective_js()
    }

    /// main frame 로드가 끝났다. commit 없이 끝났으면 화면 문서의 허용 상태로 되돌릴 값을 준다.
    pub fn on_load_finished(&mut self) -> Option<bool> {
        let restore = self.load_in_flight && !self.committed_since_start;
        self.load_in_flight = false;
        self.pending = None;
        self.end_load_marks();
        restore.then(|| self.effective_js())
    }

    /// main frame 로드가 실패했다. 호스트 chrome이 실패 상태를 보이는 동안 배너와 탭 표지는
    /// 이전 문서를 가리키므로 내린다. 로드 종료 처리는 호스트가 [`Self::on_load_finished`]로 따로 알린다.
    pub fn on_load_failed(&mut self) {
        self.load_failed = true;
        // commit한 로드가 그 뒤 실패했으면 그 로드의 스캔 결과로 허용 가능한 지문을 만들지 않는다.
        // commit 전에 실패한 다음 로드는 화면 문서의 대기를 건드리지 않는다.
        if self.committed_since_start {
            self.current_wait = None;
        }
    }

    /// 마지막 로드가 실패한 채 새 문서가 commit되지 않았다.
    pub fn load_failed(&self) -> bool {
        self.load_failed
    }

    /// 현재 문서를 허용으로 기록하고 재로드를 요청한다. 파일을 다시 읽지 않는다.
    ///
    /// 로드 중(commit 전)에는 거절한다. 그때 기록하면 곧 commit될 새 문서의 판정이 허용을 풀기 때문이다.
    /// 현재 문서의 스캔이 끝나기 전에도 거절한다. 허용을 묶을 지문이 아직 없다.
    pub fn allow_current(&mut self) -> Result<(), AllowError> {
        if self.awaiting_verdict() {
            return Err(AllowError::Loading);
        }
        let doc = self.current.as_ref().ok_or(AllowError::NoDocument)?;
        let scan = doc.scan.ok_or(AllowError::NoFingerprint)?;
        self.allowance = Some(Allowance {
            url: doc.url.clone(),
            fingerprint: scan.fingerprint,
        });
        self.reload_requested = true;
        self.reloading_after_allow = true;
        Ok(())
    }

    /// 새 문서로 바뀌었다. 두 배너의 문서 단위 표지를 내리고 열람만 이어받는다.
    fn reset_document_flags(&mut self, viewed: bool) {
        let fresh = BannerFlags {
            viewed,
            ..BannerFlags::default()
        };
        self.banner = fresh;
        self.remote.banner = fresh;
        self.remote.reshown = false;
    }

    /// 허용 직후의 재로드 요청을 한 번 꺼낸다.
    pub fn take_reload_request(&mut self) -> bool {
        std::mem::take(&mut self.reload_requested)
    }
}

/// 허용 기록이 이 응답의 URL·지문과 같은지.
fn matches(allowance: &Option<Allowance>, url: &str, scan: Option<ScriptScan>) -> bool {
    match (allowance, scan) {
        (Some(allow), Some(scan)) => allow.url == url && allow.fingerprint == scan.fingerprint,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
