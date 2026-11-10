//! 원격 차단 중 top-level `http(s)` 탐색을 열지 정한다(docs/surfaces/html/index.md "원격 콘텐츠 차단").
//!
//! 원격 차단은 화면 문서의 하위 리소스를 막는다. 그 로드의 top-level 문서 요청은 아래 출처일 때 연다.
//!
//! - 호스트가 지목한 로드: 주소창·`html open`·plugin·복원(`load_url`), 새로고침, 뒤로·앞으로.
//! - 허용된 화면 문서(전역 설정 또는 문서 단위 허용)가 시작한 탐색.
//! - `http(s)` 화면 문서에서 사용자가 링크를 눌러 시작한 탐색.
//!
//! 연 판단은 그 로드의 리다이렉트 hop에도 이어지고 commit에서 끝난다. 새 문서의 하위 리소스는 그 문서
//! 자신의 차단 상태를 따른다. 판단 근거는 탐색이 시작된 시점의 화면 문서이며 새 문서가 허용을 받지 않는다.

/// 호스트가 지목한 로드. 다음 탐색 시작 중 맞는 것이 소비한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Designation {
    /// 이 주소를 연다(`load_url`).
    Url(String),
    /// 새로고침.
    Reload,
    /// 뒤로·앞으로.
    History,
    /// 다음 main frame 탐색 시작. main frame 탐색만 알리는 backend(WebView2)가 쓴다.
    Next,
}

/// 탐색 시작 신호가 알려 준 탐색 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    Reload,
    History,
    Other,
}

/// 탐색이 시작된 시점의 화면 문서.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Origin {
    /// 화면 문서에 원격이 열려 있다(전역 설정 또는 문서 단위 허용).
    pub remote_allowed: bool,
    /// 화면 문서가 `http(s)` 문서다.
    pub remote_document: bool,
}

/// 한 webview의 top-level 탐색 허용 상태.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopLevelAdmission {
    designation: Option<Designation>,
    /// 연 로드의 현재 hop 주소. commit·종료에서 비운다.
    target: Option<String>,
}

/// `http://`·`https://` 주소인지.
pub fn is_remote_url(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// 비교용 주소. WHATWG URL로 정규화하고(scheme·host 소문자, IDN punycode, 기본 포트 생략, 점 경로 해소,
/// 퍼센트 인코딩, 빈 경로 `/`) fragment를 뺀다. 엔진이 탐색 신호에 넣는 주소도 같은 정규화를 거친다.
/// 해석할 수 없는 주소는 fragment만 빼고 그대로 비교한다.
fn normalized(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) => {
            parsed.set_fragment(None);
            parsed.into()
        }
        Err(_) => super::strip_fragment(url).to_string(),
    }
}

/// 엔진이 정규화한 주소와 호스트가 넘긴 주소가 같은 문서를 가리키는지.
pub fn same_target(a: &str, b: &str) -> bool {
    a == b || normalized(a) == normalized(b)
}

impl Designation {
    fn matches(&self, url: &str, kind: StartKind) -> bool {
        match self {
            Self::Url(designated) => kind == StartKind::Other && same_target(designated, url),
            Self::Reload => kind == StartKind::Reload,
            Self::History => kind == StartKind::History,
            Self::Next => true,
        }
    }
}

impl TopLevelAdmission {
    /// 호스트가 로드를 지목했다. 앞 지목을 대신한다.
    pub fn designate(&mut self, designation: Designation) {
        self.designation = Some(designation);
    }

    /// 리다이렉트가 아닌 탐색 시작. 이 탐색의 top-level 요청을 열면 `true`.
    ///
    /// 맞는 지목은 대상과 관계없이 소비한다. 연 `http(s)` 탐색만 대상을 기록하고, 열지 않은 탐색은
    /// 기록을 바꾸지 않는다(서브프레임 탐색을 구분하지 못하는 backend가 진행 중인 main 로드를 잃지 않게).
    pub fn start(
        &mut self,
        url: &str,
        kind: StartKind,
        user_gesture: bool,
        origin: Origin,
    ) -> bool {
        let designated = self
            .designation
            .as_ref()
            .is_some_and(|d| d.matches(url, kind));
        if designated {
            self.designation = None;
        }
        let admitted =
            designated || origin.remote_allowed || (user_gesture && origin.remote_document);
        if admitted && is_remote_url(url) {
            self.target = Some(url.to_string());
        }
        admitted
    }

    /// 연 로드의 리다이렉트 hop. 열린 로드가 있으면 대상을 옮기고 `true`.
    pub fn redirect(&mut self, url: &str) -> bool {
        if self.target.is_none() {
            return false;
        }
        self.target = Some(url.to_string());
        true
    }

    /// 열린 로드가 있는지.
    pub fn admitting(&self) -> bool {
        self.target.is_some()
    }

    /// 이 주소가 열린 로드의 현재 hop인지.
    pub fn admits(&self, url: &str) -> bool {
        self.target.as_deref().is_some_and(|t| same_target(t, url))
    }

    /// main frame commit 또는 로드 종료. 열린 로드를 닫는다. 지목은 남긴다.
    pub fn end(&mut self) {
        self.target = None;
    }
}

#[cfg(test)]
mod tests;
