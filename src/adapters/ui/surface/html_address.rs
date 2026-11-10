//! html surface 위쪽의 주소 띠를 그리고 입력을 사용자 탐색으로 바꾼다(docs/surfaces/html/index.md "주소 표시줄").
//!
//! 띠 자체는 공용 `tasty_ui_widgets::html_address_bar`가 그린다. 여기서는 보일 주소 고르기,
//! 입력 해석(`tasty_utils::html_address`), 로컬 파일 확인, 사용자 탐색 큐 넣기를 맡는다.
//! 확정 주소(backend가 관측한 commit 주소)와 시도 주소(로드 중·실패한 주소)를 나눠 기록한다.
//! 로컬 파일 확인은 그리기 밖의 작업 스레드가 하고, 결과가 오기 전에 입력이 바뀌면 버린다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlAddressAction, HtmlAddressKind, HtmlAddressLabels, HtmlAddressState, HtmlAddressView,
    HtmlRemoteMarkerKind, HtmlRemoteMarkerView, html_address_bar, html_address_field_id,
    html_address_surrender_focus,
};
use tasty_utils::html_address::{
    AddressError, AddressTarget, LocalFileStatus, resolve_html_address,
};

use crate::webview::{NavState, WebViewUserNav};

/// 파일 확인을 기다리는 동안 결과를 보러 다시 그리는 간격. 확인이 끝나면 예약하지 않는다.
const LOOKUP_POLL: Duration = Duration::from_millis(50);

/// 창 하나의 주소 띠 상태. surface ID로 나눈다.
#[derive(Default)]
pub(crate) struct HtmlAddressBands {
    bands: HashMap<u32, Band>,
    /// 주소창 포커스 단축키가 고른 surface. 다음에 그 띠를 그릴 때 한 번 포커스를 준다.
    focus_request: Option<u32>,
    /// 지난 프레임에 키보드 포커스를 가진 띠와, 그 프레임에 포커스를 새로 얻었는지.
    focused: Option<(u32, bool)>,
    /// 페이지가 키보드 포커스를 가져가 편집을 끝낼 띠.
    drop_focus: Option<u32>,
    /// 띠가 포커스를 가진 동안 마지막으로 본 페이지의 키보드 포커스.
    page_focus: bool,
    /// 이번 프레임에 그린 띠. 포커스를 가진 띠를 그리지 않은 프레임(탭 전환 등)에는 포커스를 놓는다.
    drawn: std::collections::HashSet<u32>,
}

#[derive(Default)]
struct Band {
    field: HtmlAddressState,
    attempt: Option<Attempt>,
    lookup: Option<Lookup>,
    /// 마지막으로 본 호스트 로드 주소(`webview_url`). 바뀌면 그 주소를 시도 주소로 기록한다.
    host_url: Option<String>,
}

/// 로드를 시작했거나 시작할 주소. 실패하면 남아 띠가 이 주소를 보인다.
#[derive(Debug, PartialEq)]
struct Attempt {
    url: String,
    loading_seen: bool,
}

struct Lookup {
    /// 확인을 시작한 입력. 그 사이 입력이 바뀌면 결과를 버린다.
    input: String,
    target: AddressTarget,
    rx: mpsc::Receiver<LocalFileStatus>,
}

/// 띠를 그릴 surface의 현재 값.
pub(crate) struct HtmlAddressSource<'a> {
    pub nav: NavState,
    pub info: &'a tasty_model::WebViewNavInfo,
    /// 호스트가 넣은 주소(`webview_url`). 확정 주소로 쓰지 않고 시도 주소 기록에만 쓴다.
    pub host_url: Option<&'a str>,
    pub cwd: Option<&'a Path>,
    pub home: Option<&'a Path>,
    /// mirror workspace의 html leaf. 수신 모델이 없는 placeholder에는 띠를 숨긴다.
    pub mirror: bool,
    pub received: Option<&'a crate::runtime::html_receive::ReceiveSnapshot>,
    /// 주소 칸 끝 원격 콘텐츠 표지. 원격 안내가 화면에 있으면 호출자가 `None`을 넣는다.
    pub remote_marker: Option<tasty_ui_widgets::HtmlRemoteMarkerKind>,
    /// 화면 문서가 `http(s)` 페이지다. allowed 표지 툴팁이 페이지 문구를 쓴다.
    pub web_page: bool,
    /// egui 위에 그려지는 WebView 영역. 표지 툴팁이 피한다.
    pub native: &'a [egui::Rect],
}

/// 입력을 확정했을 때 할 일.
#[derive(Debug, PartialEq)]
enum Submit {
    Nothing,
    Navigate(String),
    Lookup(AddressTarget),
    Reject(AddressError),
}

fn plan_submit(
    input: &str,
    document: Option<&str>,
    cwd: Option<&Path>,
    home: Option<&Path>,
) -> Submit {
    match resolve_html_address(input, document, cwd, home) {
        Err(AddressError::Empty) => Submit::Nothing,
        Err(e) => Submit::Reject(e),
        Ok(target) if target.local_path.is_some() => Submit::Lookup(target),
        Ok(target) => Submit::Navigate(target.url.to_string()),
    }
}

/// 주소를 띠에 보일 형태로 바꾼다. 로컬 문서는 파일 경로(홈은 `~`), 웹 문서는 URL이다.
/// `file://`·`http(s)://`가 아닌 주소는 받은 그대로 보인다.
fn display(url: &str, home: Option<&Path>) -> (String, HtmlAddressKind) {
    match resolve_html_address(url, None, None, home) {
        Ok(t) if t.local_path.is_some() => (t.display(home), HtmlAddressKind::Local),
        Ok(t) => (t.display(home), HtmlAddressKind::Web),
        Err(_) => (url.to_owned(), HtmlAddressKind::Web),
    }
}

fn error_text(e: &AddressError, home: Option<&Path>) -> String {
    match e {
        AddressError::NoBase => crate::i18n::t("html_address.no_base").to_owned(),
        AddressError::MissingFile(p) | AddressError::LookupFailed(p) => {
            crate::i18n::t_fmt("html_address.not_found", &display_path(p, home))
        }
        _ => crate::i18n::t("html_address.invalid").to_owned(),
    }
}

/// 띠의 로컬 주소와 같은 형태(홈은 `~`)로 경로를 보인다.
fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rel) if rel.as_os_str().is_empty() => "~".into(),
        Some(rel) => format!("~/{}", rel.to_string_lossy()),
        None => path.to_string_lossy().into_owned(),
    }
}

fn local_file_status(path: &Path) -> LocalFileStatus {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => LocalFileStatus::File,
        Ok(_) => LocalFileStatus::Other,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LocalFileStatus::Missing,
        Err(e) => {
            tracing::debug!("html address lookup {}: {e}", path.display());
            LocalFileStatus::Unavailable
        }
    }
}

fn spawn_lookup(path: PathBuf) -> mpsc::Receiver<LocalFileStatus> {
    let (tx, rx) = mpsc::channel();
    let fallback = tx.clone();
    let spawned = std::thread::Builder::new()
        .name("html-address-lookup".into())
        .spawn(move || {
            if tx.send(local_file_status(&path)).is_err() {
                tracing::debug!("html address lookup finished after the input changed");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("html address lookup thread: {e}");
        if fallback.send(LocalFileStatus::Unavailable).is_err() {
            tracing::debug!("html address lookup receiver dropped");
        }
    }
    rx
}

impl Band {
    /// backend 관측값으로 시도 주소를 갱신한다. 호스트 로드 주소가 바뀌면 그 주소가 새 시도다.
    /// 시도 주소는 로드를 본 뒤 끝나거나(Done·Idle) 확정 주소가 그 주소가 되면 지운다.
    /// 실패하면 다음 시도까지 남긴다.
    fn observe(&mut self, nav: NavState, committed: Option<&str>, host_url: Option<&str>) {
        if host_url != self.host_url.as_deref() {
            self.host_url = host_url.map(str::to_owned);
            if let Some(url) = host_url {
                self.attempt = Some(Attempt {
                    url: url.to_owned(),
                    loading_seen: false,
                });
            }
        }
        let Some(attempt) = &mut self.attempt else {
            return;
        };
        match nav {
            NavState::Loading | NavState::Failed => attempt.loading_seen = true,
            NavState::Done | NavState::Idle => {
                if attempt.loading_seen || committed == Some(attempt.url.as_str()) {
                    self.attempt = None;
                }
            }
        }
    }

    /// 띠에 보일 주소. 로드 중·실패면 시도 주소를, 아니면 확정 주소를 보인다.
    fn shown<'a>(&'a self, nav: NavState, committed: Option<&'a str>) -> Option<&'a str> {
        match nav {
            NavState::Loading | NavState::Failed => {
                self.attempt.as_ref().map(|a| a.url.as_str()).or(committed)
            }
            NavState::Done | NavState::Idle => committed,
        }
    }

    /// 기다리던 파일 확인이 끝났으면 결과를 돌려준다. 확인 중에 입력이 바뀌었으면 버린다.
    fn poll_lookup(&mut self) -> Option<Result<String, AddressError>> {
        let lookup = self.lookup.as_ref()?;
        if lookup.input != self.field.buffer {
            self.lookup = None;
            return None;
        }
        let status = match lookup.rx.try_recv() {
            Ok(status) => status,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => LocalFileStatus::Unavailable,
        };
        let lookup = self.lookup.take()?;
        Some(
            lookup
                .target
                .validate_local(|_| status)
                .map(|()| lookup.target.url.to_string()),
        )
    }

    /// 확정 주소로 열 입력을 처리한다. 열 주소가 정해지면 돌려준다.
    fn submit(
        &mut self,
        input: &str,
        document: Option<&str>,
        source: &HtmlAddressSource<'_>,
        lookup: impl FnOnce(PathBuf) -> mpsc::Receiver<LocalFileStatus>,
    ) -> Option<String> {
        self.lookup = None;
        match plan_submit(input, document, source.cwd, source.home) {
            Submit::Nothing => None,
            Submit::Navigate(url) => Some(url),
            Submit::Reject(e) => {
                self.field.reject(error_text(&e, source.home));
                None
            }
            Submit::Lookup(target) => {
                let path = target.local_path.clone()?;
                self.lookup = Some(Lookup {
                    input: input.to_owned(),
                    target,
                    rx: lookup(path),
                });
                None
            }
        }
    }

    /// 열기로 정한 주소를 큐에 넣고 편집을 끝낸다.
    fn admit(&mut self, url: String) -> WebViewUserNav {
        self.field.accepted();
        self.attempt = Some(Attempt {
            url: url.clone(),
            loading_seen: false,
        });
        WebViewUserNav::Open(url)
    }
}

/// 이번 프레임에 그린 띠의 결과.
pub(crate) struct BandFrame {
    /// 띠와 오류 행의 높이. 숨긴 띠는 0이다.
    pub height: LogicalPx,
    /// 탐색 큐에 넣을 사용자 조작.
    pub nav: Option<WebViewUserNav>,
    /// 사용자가 차단 원격 표지를 눌러 안내를 다시 불렀다.
    pub remote_reshow: bool,
}

impl HtmlAddressBands {
    /// 주소창 포커스 단축키. 다음에 이 surface의 띠를 그릴 때 포커스를 주고 주소 전체를 고른다.
    pub(crate) fn request_focus(&mut self, sid: u32) {
        self.focus_request = Some(sid);
    }

    /// WebView 동기화가 부른다. `page_holds`는 그 surface의 페이지가 OS 키보드 포커스를 가졌는지다.
    /// 띠가 포커스를 새로 얻었고 페이지가 포커스를 가졌으면 회수할 surface를 돌려준다.
    /// 띠가 포커스를 가진 동안 페이지 포커스가 새로 생기면(페이지 클릭) 다음에 그릴 때 편집을 끝낸다.
    /// 회수 요청이 늦게 반영되는 동안 남은 페이지 포커스는 새로 생긴 것으로 보지 않는다.
    pub(crate) fn sync_page_focus(&mut self, page_holds: impl FnOnce(u32) -> bool) -> Option<u32> {
        let Some((sid, newly)) = self.focused else {
            self.page_focus = false;
            return None;
        };
        let holds = page_holds(sid);
        let gained = holds && !self.page_focus;
        self.page_focus = holds;
        if newly {
            return holds.then_some(sid);
        }
        if gained {
            self.drop_focus = Some(sid);
        }
        None
    }

    /// 띠가 키보드 포커스를 가졌는지. 가진 동안 키·IME는 egui로 가고 터미널·단축키로 가지 않는다
    /// (`MainViewState::keyboard_overlay_open`).
    pub(crate) fn has_keyboard_focus(&self) -> bool {
        self.focused.is_some()
    }

    /// 키보드 포커스를 가진 띠의 surface. 공통 새로고침이 그 surface 를 대상으로 삼는다.
    pub(crate) fn focused_surface(&self) -> Option<u32> {
        self.focused.map(|(s, _)| s)
    }

    /// `sid` 의 띠가 키보드 포커스를 가졌는지. 페이지 입력 shape를 바꾸는 Linux 경로만 쓴다.
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn has_keyboard_focus_on(&self, sid: u32) -> bool {
        self.focused.is_some_and(|(s, _)| s == sid)
    }

    /// 한 프레임의 띠를 다 그린 뒤 부른다. 포커스를 가진 띠를 그리지 않았으면 포커스를 놓는다.
    pub(crate) fn finish_frame(&mut self) {
        if self.focused.is_some_and(|(s, _)| !self.drawn.contains(&s)) {
            self.focused = None;
        }
        self.drawn.clear();
    }

    /// 사라진 surface의 상태를 버린다.
    pub(crate) fn retain(&mut self, live: impl Fn(u32) -> bool) {
        self.bands.retain(|sid, _| live(*sid));
    }

    /// 띠 하나를 그린다. `rect`는 띠가 놓일 패널 위쪽 영역이다.
    pub(crate) fn draw(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        sid: u32,
        rect: egui::Rect,
        source: &HtmlAddressSource<'_>,
    ) -> BandFrame {
        let id = egui::Id::new(("html_address", sid));
        let field_id = html_address_field_id(id);
        self.drawn.insert(sid);
        let band = self.bands.entry(sid).or_default();
        let committed = source.info.url.as_deref();
        band.observe(source.nav, committed, source.host_url);
        let committed_display = committed
            .map(|u| display(u, source.home).0)
            .unwrap_or_default();
        let (mut address, kind) = match band.shown(source.nav, committed) {
            Some(url) => display(url, source.home),
            None => (String::new(), HtmlAddressKind::Empty),
        };
        if source.received.is_some() {
            band.lookup = None;
            band.field.accepted();
        }
        let kind = if let Some(received) = source.received {
            address.clone_from(&received.address);
            HtmlAddressKind::Mirror
        } else if source.mirror {
            HtmlAddressKind::MirrorPlaceholder
        } else {
            kind
        };
        if self.drop_focus.take_if(|s| *s == sid).is_some() {
            html_address_surrender_focus(ui, id);
            if band.field.error.is_none() {
                band.field.revert(&committed_display);
            }
        }
        let focus = self.focus_request.take_if(|s| *s == sid).is_some();
        let had_focus = self.focused.is_some_and(|(s, _)| s == sid);
        let t = crate::i18n::t;
        let skipped = source
            .received
            .filter(|r| r.skipped > 0)
            .map(|r| crate::i18n::t_fmt("html_remote.skipped", &r.skipped.to_string()));
        let declined = source
            .received
            .filter(|r| r.skipped > 0)
            .map(|r| crate::i18n::t_fmt("html_remote.declined", &r.address));
        let view = HtmlAddressView {
            address: &address,
            committed_address: &committed_display,
            kind,
            can_back: source.info.can_go_back,
            can_forward: source.info.can_go_forward,
            loading: source
                .received
                .map_or(source.nav == NavState::Loading, |r| {
                    r.receiving && r.size_notice.is_none() && !r.paused && r.connected
                }),
            disconnected: source.received.is_some_and(|r| !r.connected),
            skipped: skipped.as_deref().zip(declined.as_deref()),
            remote_marker: source.remote_marker.map(|kind| HtmlRemoteMarkerView {
                kind,
                tooltip: t(match kind {
                    HtmlRemoteMarkerKind::Blocked => "banner.html_remote.marker_blocked",
                    HtmlRemoteMarkerKind::Allowed if source.web_page => {
                        "banner.html_remote.marker_allowed_web"
                    }
                    HtmlRemoteMarkerKind::Allowed => "banner.html_remote.marker_allowed",
                }),
                force_hover: false,
                native: source.native,
            }),
            focus,
            labels: HtmlAddressLabels {
                placeholder: t("html_address.placeholder"),
                back: t("html_address.back"),
                forward: t("html_address.forward"),
                reload: t("html_address.reload"),
                disconnected: t("html_address.disconnected"),
            },
        };
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(rect));
        child.set_clip_rect(ui.clip_rect().intersect(rect));
        let out = html_address_bar(
            &mut child,
            theme,
            id,
            LogicalPx(rect.width()),
            &mut band.field,
            &view,
        );
        if out.has_keyboard_focus {
            self.focused = Some((sid, !had_focus));
        } else if had_focus {
            self.focused = None;
        }
        let nav = match out.action {
            HtmlAddressAction::Back => Some(WebViewUserNav::Back),
            HtmlAddressAction::Forward => Some(WebViewUserNav::Forward),
            HtmlAddressAction::Reload => Some(WebViewUserNav::Reload),
            HtmlAddressAction::Revert => {
                band.lookup = None;
                None
            }
            HtmlAddressAction::Open(input) => band
                .submit(&input, committed, source, spawn_lookup)
                .map(|url| band.admit(url)),
            HtmlAddressAction::None => band.poll_lookup().and_then(|result| match result {
                Ok(url) => Some(band.admit(url)),
                Err(e) => {
                    band.field.reject(error_text(&e, source.home));
                    None
                }
            }),
        };
        if band.lookup.is_some() {
            ui.ctx().request_repaint_after(LOOKUP_POLL);
        }
        // 연 뒤에는 편집을 끝내 띠가 시도 주소와 확정 주소를 보이게 한다.
        if matches!(nav, Some(WebViewUserNav::Open(_))) {
            ui.memory_mut(|m| m.surrender_focus(field_id));
            if self.focused.is_some_and(|(s, _)| s == sid) {
                self.focused = None;
            }
        }
        BandFrame {
            height: LogicalPx(out.rect.height()),
            nav,
            remote_reshow: out.remote_marker_clicked,
        }
    }
}

#[cfg(test)]
mod tests;
