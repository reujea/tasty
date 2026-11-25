//! host가 실은 HTML을 host 전용 주소로 응답하는 규칙. Windows `NavigateToString`이 상한(2MB)을 넘는
//! 문서를 거절하면 같은 HTML을 이 주소로 한 번 다시 싣는다(docs/design/systems/webview.md "탐색").
//! 주소는 이름 해석이 되지 않는 예약 호스트를 쓰고, 그 호스트의 요청은 view의 리소스 처리기가 모두
//! 로컬에서 끝낸다. 이 모듈은 OS와 무관한 판정과 본문 표만 갖는다.

use std::sync::Arc;

use tasty_model::html_script::top_level::{StartKind, same_target};

/// host 전용 주소의 호스트 접미사. `.invalid`는 이름 해석이 실패하도록 예약된 최상위 이름이다.
pub(super) const RESERVED_HOST_SUFFIX: &str = ".tasty-host-document.invalid";

/// 예약 호스트 요청을 모든 출처·요청 종류에서 받는 리소스 필터. 포트·경로가 붙어도 걸린다. 걸린 요청이
/// 예약 호스트인지는 `is_reserved`가 다시 판정한다(필터의 `*`는 더 넓게 걸린다).
pub(super) const RESERVED_FILTERS: [&str; 2] = [
    "http://*.tasty-host-document.invalid*",
    "https://*.tasty-host-document.invalid*",
];

/// Win32 `NavigateToString`이 받는 문자열의 최대 크기(UTF-16 바이트).
const NAVIGATE_TO_STRING_MAX_BYTES: usize = 2 * 1024 * 1024;

/// `E_INVALIDARG`. 상한을 넘는 문서에서 관측한 거절 코드다(0x80070057).
const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;

/// view 인스턴스 `view`가 `surface`에서 `load`번째로 실은 문서의 주소. 같은 view의 문서는 출처가 같다.
pub(super) fn host_document_url(surface: u32, view: u64, load: u64) -> String {
    format!("http://v{surface}-{view}{RESERVED_HOST_SUFFIX}/{load}")
}

/// 주소의 호스트가 예약 호스트인지. scheme은 http·https 모두 본다(로컬 종결 대상을 넓게 잡는다).
pub(super) fn is_reserved(uri: &str) -> bool {
    let Some(rest) = uri
        .strip_prefix("http://")
        .or_else(|| uri.strip_prefix("https://"))
    else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default();
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host.ends_with(RESERVED_HOST_SUFFIX)
}

/// 거절된 `NavigateToString`을 host 전용 주소로 다시 실을지. 상한을 넘는 문서의 `E_INVALIDARG`이고,
/// 그 로드가 지금 로드이며, view가 이 경로를 허용할 때만이다. 다른 거절은 실패로 끝난다.
pub(super) fn retry_via_host_document(
    code: i32,
    html: &str,
    latest: u64,
    rejected: u64,
    allowed: bool,
) -> bool {
    allowed
        && latest == rejected
        && code == E_INVALIDARG
        && html.encode_utf16().count() * 2 > NAVIGATE_TO_STRING_MAX_BYTES
}

/// 다시 싣기를 준비할 만큼 큰 문서인지. UTF-16 바이트는 UTF-8 길이의 두 배를 넘지 않으므로, 이 값이
/// 거짓이면 `retry_via_host_document`도 거짓이다. 작은 문서마다 본문을 복사하지 않으려는 거름이다.
pub(super) fn may_exceed_limit(html: &str) -> bool {
    html.len() * 2 > NAVIGATE_TO_STRING_MAX_BYTES
}

/// 표의 문서 하나. `shown`은 이 view에 commit된(지금 또는 마지막으로 보인) 문서다.
struct Entry {
    load: u64,
    url: String,
    body: Arc<str>,
    /// 이 문서를 연 main 탐색의 `NavigationId`. 시작 전에는 None, ID를 모르면 Some(None).
    /// 보낸 문서는 시작한 탐색 하나에만 묶인다. 같은 주소의 다른 탐색은 호스트가 보낸 것이 아니다.
    nav: Option<Option<u64>>,
    shown: bool,
}

/// 진행 중인 main 탐색.
struct Main {
    nav: Option<u64>,
    /// 예약 주소 탐색이면 표의 그 문서 주소, 아니면 None.
    url: Option<String>,
    /// 이 탐색의 문서 요청에 본문을 실어 보냈다(`SetResponse` 성공까지).
    served: bool,
}

/// view 하나의 host 전용 문서 표. 보낼 문서와 마지막으로 commit된 문서만 갖는다.
#[derive(Default)]
pub(super) struct HostDocuments {
    entries: Vec<Entry>,
    main: Option<Main>,
    /// 시작 전에 취소한 탐색. 그 완료는 chrome 상태를 바꾸지 않는다.
    cancelled: Option<u64>,
}

/// 탐색 시작에 대한 판정.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartDecision {
    /// 예약 주소가 아니다. 표는 관여하지 않는다.
    NotReserved,
    /// 호스트가 보낸 다시 싣기다. 사용자 탐색이 아니다(WebView2는 API `Navigate`도 사용자 시작으로 본다).
    HostLoad,
    /// commit된 문서의 다시 읽기(WebView2가 Reload로 보고한 탐색).
    Reload,
    /// 위 둘이 아닌 예약 주소 탐색이다(표에 없는 주소, 페이지가 스스로 연 주소, 리다이렉트, 이력 이동).
    /// 시작 전에 취소한다.
    Cancel,
}

impl HostDocuments {
    /// 다시 싣기 직전에 그 로드의 본문을 넣는다.
    pub(super) fn sent(&mut self, load: u64, url: String, body: Arc<str>) {
        self.entries.retain(|e| e.load != load);
        self.entries.push(Entry {
            load,
            url,
            body,
            nav: None,
            shown: false,
        });
    }

    /// 다시 싣기 `Navigate`가 바로 거절됐다. 시작 신호가 오지 않으므로 여기서 지운다.
    pub(super) fn rejected(&mut self, load: u64) {
        self.entries.retain(|e| e.load != load || e.shown);
    }

    /// 시작 전에 취소한 탐색을 기록한다. ID를 모르면 기록하지 않는다.
    pub(super) fn cancelled(&mut self, nav: Option<u64>) {
        if nav.is_some() {
            self.cancelled = nav;
        }
    }

    /// 이 완료가 시작 전에 취소한 탐색의 것인지. 맞으면 기록을 지운다.
    pub(super) fn take_cancelled(&mut self, nav: Option<u64>) -> bool {
        if nav.is_some() && self.cancelled == nav {
            self.cancelled = None;
            return true;
        }
        false
    }

    /// main frame `NavigationStarting`. 새 탐색은 앞 탐색을 대신한다. `kind`는 WebView2가 보고한 탐색
    /// 종류(모르면 `Other`), `redirected`는 리다이렉트 hop인지다.
    ///
    /// 예약 주소는 둘만 받는다. 호스트가 보낸 다시 싣기(보낸 문서의 주소로, 그 문서가 아직 어느 탐색에도
    /// 묶이지 않았을 때 처음 시작한 리다이렉트가 아닌 새 탐색)와 commit된 문서의 Reload다. WebView2
    /// `Navigate`는 탐색 ID를 돌려주지 않아, 보낸 문서를 처음 시작한 탐색 ID에 묶고 그 뒤 같은 주소의
    /// 다른 탐색은 막는다.
    pub(super) fn navigation_started(
        &mut self,
        uri: &str,
        nav: Option<u64>,
        kind: StartKind,
        redirected: bool,
    ) -> StartDecision {
        if !is_reserved(uri) {
            self.main = Some(Main {
                nav,
                url: None,
                served: false,
            });
            return StartDecision::NotReserved;
        }
        let found = match (redirected, kind) {
            (false, StartKind::Other) => self
                .entries
                .iter_mut()
                .find(|e| !e.shown && e.nav.is_none() && same_target(&e.url, uri))
                .map(|entry| {
                    entry.nav = Some(nav);
                    (entry.url.clone(), StartDecision::HostLoad)
                }),
            (false, StartKind::Reload) => self
                .entries
                .iter()
                .find(|e| e.shown && same_target(&e.url, uri))
                .map(|entry| (entry.url.clone(), StartDecision::Reload)),
            _ => None,
        };
        let Some((url, decision)) = found else {
            self.main = None;
            return StartDecision::Cancel;
        };
        self.main = Some(Main {
            nav,
            url: Some(url),
            served: false,
        });
        decision
    }

    /// 예약 호스트 리소스 요청에 보낼 본문. None이면 404다. 지금 main 탐색의 문서 요청이고
    /// 같은 주소의 subframe 탐색이 없을 때만 본문을 준다.
    pub(super) fn respond(
        &mut self,
        uri: &str,
        is_get: bool,
        is_document: bool,
        subframe_pending: bool,
    ) -> Option<Arc<str>> {
        if !is_get || !is_document || subframe_pending {
            return None;
        }
        let main = self.main.as_mut()?;
        let url = main.url.as_deref()?;
        if !same_target(url, uri) {
            return None;
        }
        let body = self.entries.iter().find(|e| e.url == url)?.body.clone();
        main.served = true;
        Some(body)
    }

    /// 본문 응답을 보내지 못했다(`SetResponse` 실패). 그 탐색은 성공으로 끝나도 실패로 본다.
    pub(super) fn response_failed(&mut self) {
        if let Some(main) = self.main.as_mut() {
            main.served = false;
        }
    }

    /// main frame commit(`ContentLoading`). 본문을 보낸 그 탐색이 오류 페이지가 아닌 문서로 commit하면
    /// 그 문서를 commit된 문서로 두고 지난 문서를 지운다. 예약 주소가 아닌 문서가 commit하면 지난
    /// host 문서를 지운다.
    pub(super) fn committed(&mut self, nav: Option<u64>, is_error_page: bool) {
        let Some(main) = self.main.as_ref().filter(|m| same_nav(m.nav, nav)) else {
            return;
        };
        if is_error_page {
            return;
        }
        match main.url.clone() {
            Some(url) if main.served => self.show(&url),
            Some(_) => {}
            None => self.entries.retain(|e| !e.shown),
        }
    }

    fn show(&mut self, url: &str) {
        let Some(load) = self.entries.iter().find(|e| e.url == url).map(|e| e.load) else {
            return;
        };
        // 앞 로드와 다른 commit 문서는 지운다. 뒤에 보낸 로드는 아직 나가는 중이라 남긴다.
        self.entries
            .retain(|e| e.load >= load && (e.load == load || !e.shown));
        for e in &mut self.entries {
            if e.load == load {
                e.shown = true;
            }
        }
    }

    /// main frame `NavigationCompleted`. 실패했거나 본문을 보내지 않은 탐색이 연 보낸 문서는 지운다
    /// (commit된 문서는 남는다). 지금 예약 주소 탐색인데 본문을 로컬로 보내지 못했으면 true다.
    /// 그 완료는 성공으로 와도 chrome을 실패로 끝내야 한다.
    pub(super) fn navigation_completed(&mut self, nav: Option<u64>, success: bool) -> bool {
        let current = self.main.as_ref().is_some_and(|m| same_nav(m.nav, nav));
        if !current {
            // 대신된 앞 탐색의 늦은 종료. 그 탐색이 연, commit하지 않은 보낸 문서만 지운다.
            if nav.is_some() {
                self.entries.retain(|e| e.shown || e.nav != Some(nav));
            }
            return false;
        }
        let Some(main) = self.main.take() else {
            return false;
        };
        let Some(url) = main.url else {
            if success {
                self.entries.retain(|e| !e.shown);
            }
            return false;
        };
        if success && main.served {
            // commit 신호를 받지 못한 성공(ContentLoading 누락)도 commit으로 본다.
            self.show(&url);
        } else {
            self.entries.retain(|e| e.url != url || e.shown);
        }
        !main.served
    }

    /// 렌더 process가 끝났다. commit하지 않은 문서는 버리고 commit된 문서만 남긴다.
    pub(super) fn renderer_ended(&mut self) {
        self.entries.retain(|e| e.shown);
        self.main = None;
    }

    /// browser process가 끝났다. 이 view는 다시 만들어야 하므로 표를 비운다.
    pub(super) fn browser_ended(&mut self) {
        self.entries.clear();
        self.main = None;
    }

    #[cfg(test)]
    fn loads(&self) -> Vec<(u64, bool)> {
        self.entries.iter().map(|e| (e.load, e.shown)).collect()
    }
}

/// 두 `NavigationId`가 같은 탐색인지. 한쪽을 모르면 같다고 본다(`load_generation::is_current_load`와 같다).
fn same_nav(a: Option<u64>, b: Option<u64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

#[cfg(test)]
mod tests;
