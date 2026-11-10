//! 원격(`http(s)`) 리소스 차단의 문서 단위 허용과 안내 판정(docs/surfaces/html/index.md "원격 콘텐츠 차단").
//!
//! 허용 기록은 스크립트 허용과 따로 두지만 수명은 같다. 그 문서(URL + 지문, `http(s)` 페이지는 URL과
//! 허용 재로드 한 번)에만 묶이고 다른 문서가 commit되면 풀린다. 응답 결정 단계에서 허용 기록이 맞을 때만
//! 그 로드의 차단을 푼다
//! ([`HtmlScriptState::document_remote_allowed`]). 전역 "Allow remote content"가 켜져 있으면 감지·안내를
//! 하지 않는다. entry mirror·HTTP view는 차단을 다른 곳이 정하므로 안내와 허용을 지원하지 않는다.
//! 두 view의 정책은 다르다([`RemoteView`]). mirror는 전역 설정과 관계없이 원격을 늘 막고, HTTP view는
//! endpoint가 client의 전역 설정과 같은 origin 규칙으로 정한다.
//!
//! 표시 판정은 스크립트 안내와 같은 열람 규칙을 쓴다. 원격 안내는 정적 참조가 있는 문서와 로컬 view에
//! 연 top-level `http(s)` 페이지에 뜬다. 정적 참조 없이 OS가 차단을 알린 문서는 표지만 남기고, 표지를
//! 누르면 안내를 보인다.

use super::{AllowError, Allowed, BannerFlags, BannerPhase, HtmlScriptState};

/// 원격 차단 상태. 필드는 [`HtmlScriptState`]가 직접 다룬다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteState {
    /// 전역 "Allow remote content". 켜져 있으면 모든 문서에서 원격을 받는다.
    pub(super) global: bool,
    /// 원격 차단을 정하는 view 종류. 문서 단위 허용은 `Local`에서만 지원한다.
    pub(super) view: RemoteView,
    pub(super) allowance: Option<Allowed>,
    pub(super) banner: BannerFlags,
    /// 정적 참조 없이 관측한 차단을 사용자가 표지로 열었다.
    pub(super) reshown: bool,
    /// OS가 현재 문서의 하위 리소스 차단을 알렸다(Windows).
    pub(super) observed: bool,
    /// 원격 허용 뒤 재로드가 끝나지 않았다.
    pub(super) reloading_after_allow: bool,
}

impl Default for RemoteState {
    fn default() -> Self {
        Self {
            global: false,
            view: RemoteView::Local,
            allowance: None,
            banner: BannerFlags::default(),
            reshown: false,
            observed: false,
            reloading_after_allow: false,
        }
    }
}

/// 원격 차단을 정하는 view 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteView {
    /// 로컬 문서. `전역 설정 || 문서 단위 허용`으로 정한다.
    Local,
    /// 원격 파일 entry mirror. 응답 경계(CSP)가 전역 설정과 관계없이 원격을 늘 막는다.
    Mirror,
    /// 원격 HTTP(S) view. endpoint가 전역 설정과 같은 origin 기본 허용으로 정한다.
    Http,
}

impl RemoteView {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Mirror => "mirror",
            Self::Http => "http",
        }
    }
}

/// 주소 칸 끝에 남기는 원격 차단 표지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteMarker {
    /// 차단된 채 안내가 보이지 않는다. 누르면 안내를 보인다(허용하지 않는다).
    Blocked,
    /// 이 문서에 원격 콘텐츠를 허용했다.
    Allowed,
}

impl RemoteMarker {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Allowed => "allowed",
        }
    }
}

/// 원격 허용을 받을 수 없는 이유. 스크립트 허용의 이유에 원격 고유의 경우를 더한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteAllowError {
    Document(AllowError),
    /// 전역 설정이 이미 원격을 허용한다.
    GloballyAllowed,
    /// entry mirror·HTTP view라 이 허용이 차단에 영향을 주지 않는다.
    UnsupportedView,
}

impl std::fmt::Display for RemoteAllowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Document(e) => e.fmt(f),
            Self::GloballyAllowed => {
                f.write_str("remote content is already allowed by the global setting")
            }
            Self::UnsupportedView => f.write_str(
                "this view decides remote loads elsewhere (mirror or HTTP view); \
                 a per-document allow has no effect",
            ),
        }
    }
}

impl HtmlScriptState {
    /// 전역 "Allow remote content".
    pub fn remote_global(&self) -> bool {
        self.remote.global
    }

    /// 전역 설정을 바꾸고 문서 단위 허용 값을 돌려준다. 허용 기록은 지우지 않는다.
    pub fn set_remote_global(&mut self, allowed: bool) -> bool {
        self.remote.global = allowed;
        self.document_remote_allowed()
    }

    /// 이 view가 문서 단위 원격 허용을 지원하는지.
    pub fn remote_supported(&self) -> bool {
        self.remote.view == RemoteView::Local
    }

    /// 원격 차단을 정하는 view 종류.
    pub fn remote_view(&self) -> RemoteView {
        self.remote.view
    }

    /// entry mirror·HTTP view로 바뀌었다. 원격 안내·표지를 내리고 문서 단위 허용을 받지 않는다.
    pub fn set_remote_view(&mut self, view: RemoteView) {
        self.remote.view = view;
        if view != RemoteView::Local {
            self.remote.allowance = None;
        }
    }

    /// 원격 허용 기록.
    pub fn remote_allowance(&self) -> Option<&Allowed> {
        self.remote.allowance.as_ref()
    }

    /// 원격 안내 표지.
    pub fn remote_banner(&self) -> BannerFlags {
        self.remote.banner
    }

    /// OS가 현재 문서의 하위 리소스 차단을 알렸다.
    pub fn remote_observed(&self) -> bool {
        self.remote.observed
    }

    /// 현재 문서에 원격 참조가 있는지(정적 감지). 문서나 지문이 없으면 `None`.
    pub fn current_remote_refs(&self) -> Option<bool> {
        self.current.as_ref()?.scan.map(|s| s.remote_refs)
    }

    /// 현재 문서가 원격 허용 기록과 같은 문서인지.
    pub fn current_is_remote_allowed(&self) -> bool {
        match (&self.current, &self.remote.allowance) {
            (Some(doc), Some(allow)) => allow.matches_document(doc),
            _ => false,
        }
    }

    /// 문서 단위로 원격 차단을 풀지. 전역 설정은 backend가 따로 더한다(실효값 = 전역 || 이 값).
    ///
    /// 로드가 진행 중이고 아직 commit 전이면 응답 단계의 판단(없으면 차단)을 따른다.
    /// 그렇지 않으면 화면 문서의 허용 여부를 따른다.
    pub fn document_remote_allowed(&self) -> bool {
        if !self.remote_supported() {
            return false;
        }
        if self.loading_before_commit() {
            return self.pending.as_ref().is_some_and(|p| p.matched_remote);
        }
        self.current_is_remote_allowed()
    }

    /// 원격 콘텐츠가 실제로 열려 있는지. 로컬 문서는 전역 또는 이 문서 허용, mirror는 늘 닫혀 있고,
    /// HTTP view는 전역 설정을 따른다(같은 origin은 endpoint가 설정과 관계없이 허용한다).
    pub fn remote_effectively_allowed(&self) -> bool {
        match self.remote.view {
            RemoteView::Local => self.remote.global || self.current_is_remote_allowed(),
            RemoteView::Mirror => false,
            RemoteView::Http => self.remote.global,
        }
    }

    /// 지금 시작하는 탐색의 출처인 화면 문서. 로드 중이어도 commit된 문서를 본다. top-level 탐색 허용
    /// ([`super::top_level`])의 근거이며, 진행 중인 로드의 대상은 허용 근거가 되지 않는다.
    pub fn navigation_origin(&self) -> super::top_level::Origin {
        super::top_level::Origin {
            remote_allowed: self.remote_effectively_allowed(),
            remote_document: self
                .current
                .as_ref()
                .is_some_and(|doc| super::top_level::is_remote_url(&doc.url)),
        }
    }

    /// `http(s)` `src` 스크립트만 있는 문서의 스크립트가 실행될 수 있는지. HTTP view는 그 스크립트가
    /// 같은 origin일 수 있고 endpoint가 정하므로 원래대로 실행될 수 있다고 본다.
    pub(super) fn remote_scripts_runnable(&self) -> bool {
        match self.remote.view {
            RemoteView::Http => true,
            RemoteView::Local | RemoteView::Mirror => self.remote_effectively_allowed(),
        }
    }

    /// OS가 현재 문서의 하위 리소스 요청을 막았다. 새 로드가 시작된 뒤 도착한 알림은 호스트가 거른다.
    pub fn on_remote_block_observed(&mut self) {
        if self.remote_supported() && !self.remote.global && !self.loading_before_commit() {
            self.remote.observed = true;
        }
    }

    /// 현재 문서에 원격을 허용으로 기록하고 재로드를 요청한다. 거절 조건은 스크립트 허용과 같다.
    pub fn allow_remote_current(&mut self) -> Result<(), RemoteAllowError> {
        if !self.remote_supported() {
            return Err(RemoteAllowError::UnsupportedView);
        }
        if self.remote.global {
            return Err(RemoteAllowError::GloballyAllowed);
        }
        if self.awaiting_verdict() {
            return Err(RemoteAllowError::Document(AllowError::Loading));
        }
        let target = self
            .current_allow_target()
            .map_err(RemoteAllowError::Document)?;
        self.remote.allowance = Some(target);
        self.arm_allow_reload();
        self.remote.reloading_after_allow = true;
        Ok(())
    }

    /// 안내가 필요한 문서인지. 닫힘도 반영한다.
    fn wants_remote_banner(&self) -> bool {
        if !self.remote_supported() || self.remote.global || self.current_is_remote_allowed() {
            return false;
        }
        // 페이지는 스캔이 없어 참조를 모르고 OS 관측은 Windows만 알린다. 페이지면 늘 안내가 필요하다.
        let refs = self.display_scan().is_some_and(|s| s.remote_refs) || self.current_is_web_page();
        (refs || (self.remote.observed && self.remote.reshown)) && !self.remote.banner.dismissed
    }

    /// 원격 안내 표지를 갱신한다. [`Self::update_banner`]가 부른다.
    pub(super) fn update_remote_banner(&mut self) {
        let wants = self.wants_remote_banner();
        let flags = &mut self.remote.banner;
        if wants && flags.viewed {
            flags.shown = true;
        }
        flags.pending_view = wants && !flags.viewed;
    }

    /// 표지를 바꾸지 않고 원격 안내 단계를 읽는다.
    pub fn remote_banner_phase(&self) -> BannerPhase {
        if !self.remote_supported() || self.remote.global || self.load_failed {
            return BannerPhase::Hidden;
        }
        if self.remote.reloading_after_allow && self.remote.banner.shown {
            return BannerPhase::Reloading;
        }
        if self.remote.banner.shown && self.wants_remote_banner() {
            if self.awaiting_verdict() {
                BannerPhase::Loading
            } else {
                BannerPhase::Blocked
            }
        } else {
            BannerPhase::Hidden
        }
    }

    /// 주소 칸 끝 표지. 안내가 보이는 동안에는 없다.
    pub fn remote_marker(&self) -> Option<RemoteMarker> {
        if !self.remote_supported() || self.remote.global || self.load_failed {
            return None;
        }
        if self.current_is_remote_allowed() {
            return Some(RemoteMarker::Allowed);
        }
        if self.remote_banner_phase() != BannerPhase::Hidden {
            return None;
        }
        let refs = self.display_scan().is_some_and(|s| s.remote_refs) || self.current_is_web_page();
        let dismissed = self.remote.banner.dismissed;
        // 정적 참조가 없으면 안내를 표지로 연 뒤 닫았을 때만 다시 표지가 된다.
        let blocked = if refs {
            dismissed
        } else {
            self.remote.observed && (!self.remote.reshown || dismissed)
        };
        blocked.then_some(RemoteMarker::Blocked)
    }

    /// 사용자가 원격 안내를 닫았다.
    pub fn dismiss_remote_banner(&mut self) {
        self.remote.banner.dismissed = true;
    }

    /// 사용자가 표지로 원격 안내를 불렀다. 표지를 누른 것 자체가 문서를 본 것이다. 허용하지 않는다.
    pub fn reshow_remote_banner(&mut self) {
        self.remote.banner.dismissed = false;
        self.remote.banner.viewed = true;
        self.remote.reshown = true;
    }

    /// 스크립트 안내가 원격 안내와 함께 보이며 원격 스크립트도 막혀 있어 `body_remote` 본문을 써야 하는지.
    /// `http(s)` 페이지는 두 안내가 함께 보이지 않아 쓰지 않는다.
    pub fn script_banner_needs_remote_body(&self) -> bool {
        !self.current_is_web_page()
            && self.display_scan().is_some_and(|s| s.remote_scripts)
            && self.remote_banner_phase() != BannerPhase::Hidden
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod web_page_tests;
