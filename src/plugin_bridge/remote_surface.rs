//! Plugin이 제공하는 surface를 본체 layout에 끼울 수 있는 stand-in.
//!
//! webview-kind(html) surface 의 실 vehicle — host 는 URL/navigation chrome 만
//! 그리고 콘텐츠는 native WebView overlay 가 담당한다. (UiNode tree 렌더 경로는
//! C1 에서 제거됨.)
//!
//! `Surface` trait의 `kind() -> &'static str` 제약 때문에 plugin manifest의
//! 동적 kind 문자열은 `register_remote_kind`에서 `Box::leak`으로 한 번 정적화한다
//! (plugin 등록 시 1회, 메모리 누수는 plugin 종류 수만큼이라 무시 가능).
//!
//! 이름·snapshot 은 host 가 `SurfaceHandles` 의 `Arc` 로 직접 갱신한다(pump). webview URL ·
//! navigation mirror · attach 재구성용 핸들 공유는 GUI 만 쓰므로 `cfg(feature = "gui")` 다 —
//! 경계 기준은 `docs/dev-guide/headless-build-boundaries.md`.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[cfg(any(feature = "gui", test))]
use crate::model::NavState;
use crate::model::{Surface, SurfaceId};
use serde_json::Value;

pub struct RemoteSurface {
    pub id: SurfaceId,
    /// `Box::leak` 으로 정적화된 plugin kind. registry에 등록 시 한 번만 leak.
    pub kind_static: &'static str,
    pub plugin_id: String,
    /// snapshot 데이터 캐시 — plugin이 미리 보낸 값. 영속화 시 사용.
    pub snapshot_cache: Arc<Mutex<Option<Value>>>,
    /// 탭 제목 등에 표시되는 이름. plugin이 surface.create / event 응답에서 갱신 가능.
    pub display_name: Arc<Mutex<String>>,
    /// webview-enabled kind 인 경우 plugin 이 `webview.set_url` 로 전달한 URL.
    /// host 의 sync_webviews 가 매 프레임 이 값을 읽어 native webview 동기화.
    pub webview_url: Arc<Mutex<Option<String>>>,
    /// webview-enabled kind 의 navigation 생명주기 상태 mirror. host 의 sync_webviews 가
    /// 매 프레임 native `PlatformWebView.nav_state()` 를 이 값에 복사하고, egui 렌더 경로
    /// (egui_panels → webview_chrome)가 여기서 읽어 loading/error chrome 을 그린다.
    /// 쓰는 자도 읽는 자도 GUI 뿐이다 — headless 에는 webview 가 없다.
    #[cfg(any(feature = "gui", test))]
    pub nav_state: Arc<Mutex<NavState>>,
    /// plugin 이 `surface.set_cwd` 로 통보한 현재 cwd. `source_cwd()` 가 이 값을
    /// 반환하여 다음 surface 의 carry 후보 cwd 로 사용된다 (예: explorer 가 root
    /// 변경 시 갱신). 초기값 None — host 가 SurfaceCreateCtx.cwd 로 받은 carry cwd
    /// 를 생성 직후 `set_cwd` 로 채워 넣는다.
    pub cwd: Arc<Mutex<Option<PathBuf>>>,
}

/// mirror 필드별 poison 보고 플래그(각각 첫 1 회만).
///
/// 다섯 필드 모두 임계구역이 값 한 칸이라 패닉이 나도 불변식이 성립한다 — 복구가 맞다.
/// 반대로 여기서 패닉하면 읽기 쪽이 **egui 렌더 경로**(`nav_state` · `display_name` 은
/// 매 프레임 돈다)라 모든 창이 죽는다. 조용히 버리면 plugin 이 IPC 로 바꾼 상태가
/// 반영되지 않는데 에러도 없다 — 탭 이름이 kind 이름으로 되돌아가거나 loading chrome
/// 이 영영 안 걷히는 형태로만 보인다. 근거 `docs/dev-guide/error-handling.md` "락 poison".
///
/// 필드를 나눠 두는 이유: 어느 mirror 가 굳었는지가 곧 증상의 이름이라, 하나로 묶으면
/// 첫 필드만 보고되고 나머지는 침묵한다.
pub(crate) static SNAPSHOT_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
static DISPLAY_NAME_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
static WEBVIEW_URL_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
#[cfg(any(feature = "gui", test))]
static NAV_STATE_POISON_REPORTED: AtomicBool = AtomicBool::new(false);
static CWD_POISON_REPORTED: AtomicBool = AtomicBool::new(false);

pub(crate) const SNAPSHOT_WHAT: &str = "remote surface snapshot cache";
const DISPLAY_NAME_WHAT: &str = "remote surface display name";
const WEBVIEW_URL_WHAT: &str = "remote surface webview url";
#[cfg(any(feature = "gui", test))]
const NAV_STATE_WHAT: &str = "remote surface nav state";
const CWD_WHAT: &str = "remote surface cwd";

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
            #[cfg(any(feature = "gui", test))]
            nav_state: Arc::new(Mutex::new(NavState::Idle)),
            cwd: Arc::new(Mutex::new(None)),
        }
    }

    /// 같은 surface 를 가리키는 두 번째 값 — 모든 공유 상태(snapshot·이름·webview URL·
    /// navigation 상태·cwd)의 `Arc` 를 그대로 나눠 갖는다. plugin 에는 아무것도 보내지
    /// 않는다. attach mirror 가 트리를 통째로 다시 지을 때 survivor surface 를 새 트리에
    /// 옮겨 싣는 데 쓴다(`src/app/attach_client.rs` 의 markdown mirror 재구성) — 새로
    /// 만들면 plugin 이 `surface.create` 를 다시 받아 문서를 처음부터 연다.
    #[cfg(feature = "gui")]
    pub fn share_handles(&self) -> Self {
        Self {
            id: self.id,
            kind_static: self.kind_static,
            plugin_id: self.plugin_id.clone(),
            snapshot_cache: Arc::clone(&self.snapshot_cache),
            display_name: Arc::clone(&self.display_name),
            webview_url: Arc::clone(&self.webview_url),
            nav_state: Arc::clone(&self.nav_state),
            cwd: Arc::clone(&self.cwd),
        }
    }

    /// `webview.set_url` IPC 가 호출 — webview-enabled kind 의 surface 만 의미 있음.
    #[cfg(feature = "gui")]
    pub fn set_webview_url(&self, url: Option<String>) {
        *crate::poison::recover_mutex(
            self.webview_url.lock(),
            WEBVIEW_URL_WHAT,
            &WEBVIEW_URL_POISON_REPORTED,
        ) = url;
    }

    /// sync_webviews 가 매 프레임 native nav_state 를 mirror 할 때 호출.
    #[cfg(any(feature = "gui", test))]
    pub fn set_nav_state(&self, s: NavState) {
        *crate::poison::recover_mutex(
            self.nav_state.lock(),
            NAV_STATE_WHAT,
            &NAV_STATE_POISON_REPORTED,
        ) = s;
    }

    /// 현재 mirror 된 navigation 상태. egui 렌더 경로가 chrome 분기에 읽는다.
    #[cfg(any(feature = "gui", test))]
    pub fn nav_state(&self) -> NavState {
        *crate::poison::recover_mutex(
            self.nav_state.lock(),
            NAV_STATE_WHAT,
            &NAV_STATE_POISON_REPORTED,
        )
    }

    /// `surface.set_cwd` IPC 가 호출. plugin 측 root/working dir 변경을 host 에 통보.
    /// `Surface::source_cwd()` 가 다음 surface 의 carry 후보로 이 값을 노출.
    pub fn set_cwd(&self, cwd: Option<PathBuf>) {
        *crate::poison::recover_mutex(self.cwd.lock(), CWD_WHAT, &CWD_POISON_REPORTED) = cwd;
    }

    /// 제품 경로는 이름을 `SurfaceHandles` 로 직접 쓴다 — 이 setter 는 시험이 mirror 를
    /// 바꾸는 손잡이다.
    #[cfg(test)]
    pub fn set_display_name(&self, name: String) {
        *crate::poison::recover_mutex(
            self.display_name.lock(),
            DISPLAY_NAME_WHAT,
            &DISPLAY_NAME_POISON_REPORTED,
        ) = name;
    }

    pub fn cache_snapshot(&self, data: Value) {
        *crate::poison::recover_mutex(
            self.snapshot_cache.lock(),
            SNAPSHOT_WHAT,
            &SNAPSHOT_POISON_REPORTED,
        ) = Some(data);
    }

    /// manager가 surface와 동기화하기 위해 필요한 핸들 묶음을 클론하여 반환.
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

    /// RemoteSurface 는 `cwd` 필드 (plugin 이 `surface.set_cwd` 로 갱신) 의 현재
    /// 값을 반환. 초기값은 host 가 생성 시점에 `set_cwd(SurfaceCreateCtx.cwd)` 로
    /// 채워둔 carry cwd. explorer 같이 root 가 *현재 폴더* 의 의미를 갖는 surface 는
    /// root 변경마다 `surface.set_cwd` 를 발사하여 이 값을 갱신한다. cwd 의미가
    /// 없는 다른 RemoteSurface kind 는 None 유지.
    fn source_cwd(&self) -> Option<std::path::PathBuf> {
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
        crate::poison::recover_mutex(
            self.webview_url.lock(),
            WEBVIEW_URL_WHAT,
            &WEBVIEW_URL_POISON_REPORTED,
        )
        .clone()
    }

    /// ADR-0255 — 이 surface 는 렌더 결과가 아니라 **원문**을 attach 채널로 나를 수
    /// 있는 후보다. 경로는 plugin 이 `surface.create`/`restore` 응답에 실어 올린
    /// snapshot(`{"file": ...}`)에서 꺼낸다 — markdown plugin 의 `open_file_surface`
    /// 가 그 형태로 올리고 host 가 [`Self::snapshot_cache`] 에 캐시한다.
    ///
    /// **여기서 kind 를 좁히지 않는다.** 어떤 `(kind, plugin_id)` 조합이 실제로 이
    /// 채널을 타는지는 앱 계층의 화이트리스트(`attach_runtime::content_mirror_candidates`)
    /// 가 정한다 — `attach_mesh_info` 와 같은 두 단 구조이고, 그래서 이 override 는
    /// `RemoteSurface` 전체에 붙는다.
    fn attach_content_info(&self) -> Option<(&str, &str, Option<PathBuf>)> {
        let file = crate::poison::recover_mutex(
            self.snapshot_cache.lock(),
            SNAPSHOT_WHAT,
            &SNAPSHOT_POISON_REPORTED,
        )
        .as_ref()
        .and_then(|v| v.get("file"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from);
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
// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다(전수 가드가 제외한다) —
// 여기 경고는 조치 대상이 될 수 없어 프로덕션 신호만 가린다. error-handling.md.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;

    #[test]
    fn surface_kind_returns_static() {
        let s = RemoteSurface::new(1, "explorer", "com.x".into(), "Files".into());
        assert_eq!(s.kind(), "explorer");
        assert_eq!(s.type_name(), "Remote");
        assert_eq!(s.display_name(), "Files");
    }

    /// mirror 가 poison 돼도 값이 그대로 읽힌다.
    ///
    /// 조용히 버리는 구현이면 탭 이름이 kind 이름("explorer")으로 되돌아가고,
    /// nav_state 는 `Idle` 로 굳어 loading chrome 이 영영 안 걷힌다 — 둘 다 원인이
    /// 화면에 남지 않는 형태라 한 테스트로 같이 고정한다.
    #[test]
    fn poisoned_mirrors_still_read_their_values() {
        let s = RemoteSurface::new(1, "explorer", "com.x".into(), "Files".into());
        s.set_nav_state(NavState::Loading);

        let name_lock = Arc::clone(&s.display_name);
        // 이유: 이 스레드는 패닉하는 것이 목적이라 join 결과는 항상 Err 다 — 버린다.
        let _ = std::thread::spawn(move || {
            let _guard = name_lock.lock().expect("fresh lock");
            panic!("poison the display name mirror on purpose");
        })
        .join();
        let nav_lock = Arc::clone(&s.nav_state);
        // 이유: 위와 같다.
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
