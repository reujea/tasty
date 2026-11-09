//! 플러그인 surface를 호스트 레이아웃에 보관한다.
//! WebView 콘텐츠는 네이티브 오버레이가 그리고 호스트는 주소·탐색 UI를 표시한다.
//! 이름과 snapshot은 매니저가 공유 핸들로 갱신한다.
//! GUI별 접근 범위는 docs/dev-guide/headless-build-boundaries.md를 따른다.

mod html;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[cfg(any(feature = "gui", test))]
use crate::model::NavState;
use crate::model::{Surface, SurfaceId};
use serde_json::Value;

pub struct RemoteSurface {
    pub id: SurfaceId,
    /// 등록 시 프로세스 수명으로 할당한 kind 문자열.
    pub kind_static: &'static str,
    pub plugin_id: String,
    /// 플러그인에서 받은 snapshot. 저장·복원에 사용한다.
    pub snapshot_cache: Arc<Mutex<Option<Value>>>,
    /// 플러그인 응답으로 갱신하는 표시 이름.
    pub display_name: Arc<Mutex<String>>,
    /// webview.set_url로 받은 URL. sync_webviews가 네이티브 WebView에 반영한다.
    pub webview_url: Arc<Mutex<Option<String>>>,
    /// webview.set_url의 선택 인자 label. host chrome이 URL 대신 보이는 문서 이름이다.
    /// url이 raw HTML이어도 chrome에 원문을 보이지 않도록 출처를 나눈다.
    #[cfg(feature = "gui")]
    pub webview_label: Arc<Mutex<Option<String>>>,
    /// 현재 URL을 설정한 호출자가 surface 소유 플러그인인지 표시한다.
    /// 사용자 navigation 판정은 소유 플러그인이 설정한 페이지에서만 허용한다.
    #[cfg(feature = "gui")]
    pub webview_page_by_owner: Arc<AtomicBool>,
    /// 마지막 조회 뒤 URL 작성자가 외부 호출자에서 소유 플러그인으로 바뀌었는지 표시한다.
    /// 클릭 이후 페이지가 바뀌면 작성자를 잘못 판단할 수 있어 해당 프레임의 기록을 버린다.
    #[cfg(feature = "gui")]
    pub webview_owner_took_over: Arc<AtomicBool>,
    /// webview.post_message로 받은 아직 실행하지 않은 메시지 스크립트. webview 동기화가 도착 순서대로 비운다.
    /// 스크립트는 각각 독립된 조작이라 URL처럼 마지막 값만 남기지 않고 모두 쌓는다.
    #[cfg(any(feature = "gui", test))]
    pub pending_scripts: Arc<Mutex<Vec<String>>>,
    /// 네이티브 WebView의 탐색 상태 사본. 호스트 UI가 로딩·오류를 표시할 때 쓴다.
    #[cfg(any(feature = "gui", test))]
    pub nav_state: Arc<Mutex<NavState>>,
    /// 네이티브 WebView의 commit 주소와 히스토리 이동 가능 여부 사본. 주소 표시와 조회가 쓴다.
    #[cfg(feature = "gui")]
    pub webview_nav: Arc<Mutex<tasty_model::WebViewNavInfo>>,
    /// html 문서의 스크립트 감지와 문서 단위 허용(docs/surfaces/html/index.md). native backend의 navigation 콜백과 공유한다.
    #[cfg(feature = "gui")]
    pub html_script: Arc<Mutex<tasty_model::html_script::HtmlScriptState>>,
    /// 호스트가 보관하는 cwd. 생성 시 선언된 파일 경로나 상속 cwd로 채우고,
    /// surface.set_cwd로 갱신할 수 있다.
    pub cwd: Arc<Mutex<Option<PathBuf>>>,
    /// 생성 인자의 file. 플러그인 snapshot이 오기 전에도 attach 원문 대상 경로를 알린다.
    pub initial_file: Option<PathBuf>,
    /// Requested HTML source, shared across GUI/headless and never written by native commit callbacks.
    pub html_mirror: Option<Arc<Mutex<tasty_remote::html_mirror::HtmlMirror>>>,
    pub html_source: Arc<Mutex<tasty_remote::html_source::HtmlSourceState>>,
    #[cfg(all(
        feature = "gui",
        any(target_os = "linux", target_os = "macos", windows)
    ))]
    pub(crate) http_session: Option<Arc<crate::app::http_session::HttpSession>>,
    #[cfg(all(
        feature = "gui",
        any(target_os = "linux", target_os = "macos", windows)
    ))]
    pub(crate) http_native_store: Arc<crate::app::http_session::NativeStore>,
}

// 필드별로 poison을 처음 한 번 기록한다. 값 하나를 교체하는 락이므로 기존 값을 복구한다.
// 정책: docs/dev-guide/error-handling.md.
pub(crate) static SNAPSHOT_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
static DISPLAY_NAME_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
static WEBVIEW_URL_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(any(feature = "gui", test))]
static PENDING_SCRIPTS_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(any(feature = "gui", test))]
static NAV_STATE_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
static HTML_SOURCE_POISONED: AtomicBool = AtomicBool::new(false);
static CWD_POISON_REPORTED: AtomicBool = AtomicBool::new(false);

pub(crate) const SNAPSHOT_WHAT: &str = "remote surface snapshot cache";
const DISPLAY_NAME_WHAT: &str = "remote surface display name";
const WEBVIEW_URL_WHAT: &str = "remote surface webview url";
#[cfg(any(feature = "gui", test))]
const PENDING_SCRIPTS_WHAT: &str = "remote surface pending scripts";
#[cfg(feature = "gui")]
static WEBVIEW_LABEL_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "gui")]
const WEBVIEW_LABEL_WHAT: &str = "remote surface webview label";
#[cfg(any(feature = "gui", test))]
const NAV_STATE_WHAT: &str = "remote surface nav state";
const CWD_WHAT: &str = "remote surface cwd";
#[cfg(feature = "gui")]
static WEBVIEW_NAV_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "gui")]
const WEBVIEW_NAV_WHAT: &str = "remote surface webview nav info";
#[cfg(feature = "gui")]
static HTML_SCRIPT_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "gui")]
const HTML_SCRIPT_WHAT: &str = "remote surface html script state";

impl RemoteSurface {
    pub fn new(
        id: SurfaceId,
        kind_static: &'static str,
        plugin_id: String,
        initial_name: String,
    ) -> Self {
        Self {
            id,
            kind_static,
            plugin_id,
            snapshot_cache: Arc::new(Mutex::new(None)),
            display_name: Arc::new(Mutex::new(initial_name)),
            webview_url: Arc::new(Mutex::new(None)),
            #[cfg(feature = "gui")]
            webview_label: Arc::new(Mutex::new(None)),
            #[cfg(feature = "gui")]
            webview_page_by_owner: Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "gui")]
            webview_owner_took_over: Arc::new(AtomicBool::new(false)),
            #[cfg(any(feature = "gui", test))]
            pending_scripts: Arc::new(Mutex::new(Vec::new())),
            #[cfg(any(feature = "gui", test))]
            nav_state: Arc::new(Mutex::new(NavState::Idle)),
            #[cfg(feature = "gui")]
            webview_nav: Arc::default(),
            #[cfg(feature = "gui")]
            html_script: Arc::default(),
            cwd: Arc::new(Mutex::new(None)),
            initial_file: None,
            html_source: Arc::default(),
            html_mirror: None,
            #[cfg(all(
                feature = "gui",
                any(target_os = "linux", target_os = "macos", windows)
            ))]
            http_session: None,
            #[cfg(all(
                feature = "gui",
                any(target_os = "linux", target_os = "macos", windows)
            ))]
            http_native_store: Arc::default(),
        }
    }

    /// URL·chrome 라벨과 작성자를 갱신한다. by_owner는 호출자가 이 surface의 소유 플러그인인지 나타낸다.
    /// label이 None이면 이전 라벨을 지운다. 라벨은 URL과 함께 바뀌어야 다른 문서의 이름이 남지 않는다.
    pub fn set_webview_url(&self, url: Option<String>, label: Option<String>, by_owner: bool) {
        if self.kind_static == "html" {
            crate::poison::recover_mutex(
                self.html_source.lock(),
                "HTML source",
                &HTML_SOURCE_POISONED,
            )
            .set_webview(url.as_deref());
        }
        #[cfg(feature = "gui")]
        {
            *crate::poison::recover_mutex(
                self.webview_label.lock(),
                WEBVIEW_LABEL_WHAT,
                &WEBVIEW_LABEL_POISON_REPORTED,
            ) = label;
        }
        #[cfg(not(feature = "gui"))]
        let _ = (label, by_owner); // Native chrome/ownership do not exist in headless.
        let mut slot = crate::poison::recover_mutex(
            self.webview_url.lock(),
            WEBVIEW_URL_WHAT,
            &WEBVIEW_URL_POISON_REPORTED,
        );
        *slot = url;
        // URL 변경과 함께 작성자를 기록하고, 작성자보다 전이 표지를 먼저 저장한다.
        #[cfg(feature = "gui")]
        {
            let was_owner = self
                .webview_page_by_owner
                .load(std::sync::atomic::Ordering::Acquire);
            if by_owner && !was_owner {
                self.webview_owner_took_over
                    .store(true, std::sync::atomic::Ordering::Release);
            }
            self.webview_page_by_owner
                .store(by_owner, std::sync::atomic::Ordering::Release);
        }
    }

    /// 외부 작성자에서 소유 플러그인으로 바뀐 표지를 반환하고 지운다.
    /// sync_webviews가 해당 프레임의 navigation을 반영한 뒤 호출한다.
    #[cfg(feature = "gui")]
    pub fn take_webview_owner_takeover(&self) -> bool {
        self.webview_owner_took_over
            .swap(false, std::sync::atomic::Ordering::AcqRel)
    }

    #[cfg(feature = "gui")]
    pub fn webview_page_by_owner(&self) -> bool {
        self.webview_page_by_owner
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// webview.set_url이 함께 보낸 chrome 라벨.
    #[cfg(feature = "gui")]
    pub fn webview_label(&self) -> Option<String> {
        crate::poison::recover_mutex(
            self.webview_label.lock(),
            WEBVIEW_LABEL_WHAT,
            &WEBVIEW_LABEL_POISON_REPORTED,
        )
        .clone()
    }

    /// webview.post_message가 메시지 스크립트를 큐에 넣는다.
    #[cfg(any(feature = "gui", test))]
    pub fn queue_script(&self, script: String) {
        crate::poison::recover_mutex(
            self.pending_scripts.lock(),
            PENDING_SCRIPTS_WHAT,
            &PENDING_SCRIPTS_POISON_REPORTED,
        )
        .push(script);
    }

    /// 쌓인 스크립트를 도착 순서대로 꺼내고 큐를 비운다.
    #[cfg(any(feature = "gui", test))]
    pub fn take_scripts(&self) -> Vec<String> {
        std::mem::take(&mut *crate::poison::recover_mutex(
            self.pending_scripts.lock(),
            PENDING_SCRIPTS_WHAT,
            &PENDING_SCRIPTS_POISON_REPORTED,
        ))
    }

    #[cfg(any(feature = "gui", test))]
    pub fn set_nav_state(&self, s: NavState) {
        *crate::poison::recover_mutex(
            self.nav_state.lock(),
            NAV_STATE_WHAT,
            &NAV_STATE_POISON_REPORTED,
        ) = s;
    }

    #[cfg(any(feature = "gui", test))]
    pub fn nav_state(&self) -> NavState {
        *crate::poison::recover_mutex(
            self.nav_state.lock(),
            NAV_STATE_WHAT,
            &NAV_STATE_POISON_REPORTED,
        )
    }

    /// backend 관측값으로 사본을 바꾸고, 바뀌었으면 참을 돌려준다.
    #[cfg(feature = "gui")]
    pub fn set_webview_nav(&self, info: tasty_model::WebViewNavInfo) -> bool {
        let mut slot = crate::poison::recover_mutex(
            self.webview_nav.lock(),
            WEBVIEW_NAV_WHAT,
            &WEBVIEW_NAV_POISON_REPORTED,
        );
        if *slot == info {
            return false;
        }
        *slot = info;
        true
    }

    #[cfg(feature = "gui")]
    pub fn webview_nav(&self) -> tasty_model::WebViewNavInfo {
        crate::poison::recover_mutex(
            self.webview_nav.lock(),
            WEBVIEW_NAV_WHAT,
            &WEBVIEW_NAV_POISON_REPORTED,
        )
        .clone()
    }

    pub fn set_cwd(&self, cwd: Option<PathBuf>) {
        *crate::poison::recover_mutex(self.cwd.lock(), CWD_WHAT, &CWD_POISON_REPORTED) = cwd;
    }

    /// 호스트가 보관한 cwd. `source_cwd`와 달리 파일 source의 부모를 섞지 않는다.
    pub fn held_cwd(&self) -> Option<PathBuf> {
        crate::poison::recover_mutex(self.cwd.lock(), CWD_WHAT, &CWD_POISON_REPORTED).clone()
    }

    #[cfg(test)]
    pub fn set_display_name(&self, name: String) {
        *crate::poison::recover_mutex(
            self.display_name.lock(),
            DISPLAY_NAME_WHAT,
            &DISPLAY_NAME_POISON_REPORTED,
        ) = name;
    }

    /// 생성 인자에서 비어 있지 않은 file을 기억한다.
    pub fn with_initial_file(mut self, params: &Value) -> Self {
        self.initial_file = params
            .get("file")
            .and_then(|v| v.as_str())
            .filter(|f| !f.is_empty())
            .map(PathBuf::from);
        if self.kind_static == "html" {
            self.set_html_source(params.get("url").and_then(Value::as_str));
        }
        self
    }

    pub fn cache_snapshot(&self, data: Value) {
        if self.kind_static == "html" {
            let mut source = crate::poison::recover_mutex(
                self.html_source.lock(),
                "HTML source",
                &HTML_SOURCE_POISONED,
            );
            if source.source.is_none() {
                let cwd =
                    crate::poison::recover_mutex(self.cwd.lock(), CWD_WHAT, &CWD_POISON_REPORTED);
                source.set(data.get("url").and_then(Value::as_str), cwd.as_deref());
            }
        }
        *crate::poison::recover_mutex(
            self.snapshot_cache.lock(),
            SNAPSHOT_WHAT,
            &SNAPSHOT_POISON_REPORTED,
        ) = Some(data);
    }

    pub fn handles(&self) -> crate::plugin_bridge::host_cmd::SurfaceHandles {
        crate::plugin_bridge::host_cmd::SurfaceHandles {
            display_name: self.display_name.clone(),
            snapshot_cache: self.snapshot_cache.clone(),
        }
    }
}

impl Surface for RemoteSurface {
    tasty_model::impl_surface_any!();

    fn kind(&self) -> &'static str {
        self.kind_static
    }

    fn type_name(&self) -> &'static str {
        "Remote"
    }

    fn surface_id(&self) -> Option<SurfaceId> {
        Some(self.id)
    }

    /// 호스트에 보관된 cwd를 반환한다. 로컬 실행에 사용하기 전에는 출처를 확인해야 한다.
    fn source_cwd(&self) -> Option<std::path::PathBuf> {
        if self.kind_static == "html" {
            let state = crate::poison::recover_mutex(
                self.html_source.lock(),
                "HTML source",
                &HTML_SOURCE_POISONED,
            );
            if let Some(tasty_ipc::html_transfer::HtmlSource::File { path, .. }) = &state.source {
                return PathBuf::from(path).parent().map(PathBuf::from);
            }
        }
        crate::poison::recover_mutex(self.cwd.lock(), CWD_WHAT, &CWD_POISON_REPORTED).clone()
    }

    fn display_name(&self) -> String {
        crate::poison::recover_mutex(
            self.display_name.lock(),
            DISPLAY_NAME_WHAT,
            &DISPLAY_NAME_POISON_REPORTED,
        )
        .clone()
    }

    fn webview_url(&self) -> Option<String> {
        #[cfg(all(
            feature = "gui",
            any(target_os = "linux", target_os = "macos", windows)
        ))]
        if let Some(session) = &self.http_session {
            return (session.is_current()
                || session.entry_body().is_some()
                || session.native_store.is_live())
            .then(|| session.source.address().to_owned());
        }
        if let Some(mirror) = &self.html_mirror {
            return crate::poison::recover_mutex(
                mirror.lock(),
                "remote HTML mirror",
                &HTML_SOURCE_POISONED,
            )
            .body_store()
            .current()
            .map(|entry| entry.native_uri().to_owned());
        }
        crate::poison::recover_mutex(
            self.webview_url.lock(),
            WEBVIEW_URL_WHAT,
            &WEBVIEW_URL_POISON_REPORTED,
        )
        .clone()
    }

    /// attach 원문 전달 후보의 kind·소유 플러그인·snapshot의 file 경로를 반환한다.
    /// 실제 허용 여부는 attach_runtime::content_mirror_candidates가 별도로 판단한다.
    fn attach_content_info(&self) -> Option<(&str, &str, Option<PathBuf>)> {
        let file = crate::poison::recover_mutex(
            self.snapshot_cache.lock(),
            SNAPSHOT_WHAT,
            &SNAPSHOT_POISON_REPORTED,
        )
        .as_ref()
        .and_then(|v| v.get("file"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        // 변환 직후 원격 전달은 플러그인 snapshot보다 먼저 나간다.
        .or_else(|| self.initial_file.clone());
        Some((self.kind_static, self.plugin_id.as_str(), file))
    }

    fn to_tree_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind_static,
            "type": "Remote",
            "id": self.id,
            "plugin_id": self.plugin_id,
            "display_name": self.display_name(),
        })
    }
}

#[cfg(test)]
// reason: 시험 코드의 Result 무시는 사유 주석 대상에서 제외한다.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;

    // 스크립트는 각각 독립된 조작이라 덮어쓰면 갱신이 사라진다. 꺼낸 뒤에는 비어야 같은 조작이 두 번 가지 않는다.
    #[test]
    fn queued_scripts_keep_their_order_and_count() {
        let s = RemoteSurface::new(1, "markdown", "com.x".into(), "Doc".into());
        s.queue_script("a()".into());
        s.queue_script("b()".into());
        s.queue_script("c()".into());
        assert_eq!(s.take_scripts(), vec!["a()", "b()", "c()"]);
        assert!(s.take_scripts().is_empty());
    }

    // 큐는 surface마다 따로다. 한 탭의 갱신이 다른 탭으로 가면 안 된다.
    #[test]
    fn two_surfaces_do_not_share_a_queue() {
        let a = RemoteSurface::new(1, "markdown", "com.x".into(), "A".into());
        let b = RemoteSurface::new(2, "markdown", "com.x".into(), "B".into());
        a.queue_script("only_a()".into());
        assert!(b.take_scripts().is_empty());
        assert_eq!(a.take_scripts(), vec!["only_a()"]);
    }

    #[test]
    fn surface_kind_returns_static() {
        let s = RemoteSurface::new(1, "explorer", "com.x".into(), "Files".into());
        assert_eq!(s.kind(), "explorer");
        assert_eq!(s.type_name(), "Remote");
        assert_eq!(s.display_name(), "Files");
    }

    // poison 뒤에도 기존 이름과 탐색 상태를 읽고 갱신할 수 있어야 한다.
    #[test]
    fn poisoned_mirrors_still_read_their_values() {
        let s = RemoteSurface::new(1, "explorer", "com.x".into(), "Files".into());
        s.set_nav_state(NavState::Loading);

        let name_lock = Arc::clone(&s.display_name);
        // 이유: poison을 만들기 위해 발생시킨 패닉이므로 join 오류를 무시한다.
        let _ = std::thread::spawn(move || {
            let _guard = name_lock.lock().expect("fresh lock");
            panic!("poison the display name mirror on purpose");
        })
        .join();
        let nav_lock = Arc::clone(&s.nav_state);
        // 이유: 같은 방식으로 탐색 상태 락의 poison을 만든다.
        let _ = std::thread::spawn(move || {
            let _guard = nav_lock.lock().expect("fresh lock");
            panic!("poison the nav state mirror on purpose");
        })
        .join();
        assert!(s.display_name.is_poisoned() && s.nav_state.is_poisoned());

        assert_eq!(s.display_name(), "Files");
        assert_eq!(s.nav_state(), NavState::Loading);
        s.set_display_name("Browser".into());
        assert_eq!(s.display_name(), "Browser");
    }

    /// snapshot이 오기 전에는 생성 인자의 file을, 온 뒤에는 snapshot의 file을 알린다.
    #[test]
    fn attach_content_file_falls_back_to_the_create_param_until_a_snapshot_arrives() {
        let s = RemoteSurface::new(1, "markdown", "com.tasty.markdown".into(), "md".into())
            .with_initial_file(&serde_json::json!({ "file": "/remote/a.md" }));
        let file = |s: &RemoteSurface| s.attach_content_info().and_then(|(_, _, f)| f);
        assert_eq!(file(&s), Some(PathBuf::from("/remote/a.md")));
        s.cache_snapshot(serde_json::json!({ "file": "/remote/b.md" }));
        assert_eq!(file(&s), Some(PathBuf::from("/remote/b.md")));

        let empty = RemoteSurface::new(2, "markdown", "com.tasty.markdown".into(), "md".into())
            .with_initial_file(&serde_json::json!({ "file": "" }));
        assert_eq!(file(&empty), None);
    }

    #[test]
    fn set_display_name_updates() {
        let s = RemoteSurface::new(1, "explorer", "com.x".into(), "Files".into());
        s.set_display_name("Browser".into());
        assert_eq!(s.display_name(), "Browser");
    }

    #[test]
    fn initial_cwd_is_none_and_source_cwd_returns_none() {
        let s = RemoteSurface::new(1, "explorer", "com.x".into(), "Files".into());
        assert_eq!(s.source_cwd(), None);
    }

    #[test]
    fn set_cwd_then_source_cwd_returns_path() {
        let s = RemoteSurface::new(1, "explorer", "com.x".into(), "Files".into());
        let p = PathBuf::from("/tmp/foo");
        s.set_cwd(Some(p.clone()));
        assert_eq!(s.source_cwd(), Some(p));
        s.set_cwd(None);
        assert_eq!(s.source_cwd(), None);
    }
}
