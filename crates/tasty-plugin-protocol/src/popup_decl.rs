//! 팝업 공통 선언 `PopupDecl`.
//!
//! 팝업 사이의 동작 차이는 이 선언 값으로만 정한다. 팝업 id를 비교해 규칙을 정하지 않는다.
//! host 팝업 정의(`PopupDef`)가 이 타입을 담는다. plugin 팝업은 아직 manifest의 별도 선언을 쓴다.
//!
//! 속성 가운데 일부는 선언만 있고 아직 읽는 곳이 없다. 각 필드 문서에 읽는 곳이 있는지 적는다.
//! 읽는 곳이 없는 필드는 값과 실제 동작이 다를 수 있다. 그때 동작은 여는 쪽 코드가 정한다.

use serde::{Deserialize, Serialize};
use tasty_type_geometry::length::LogicalPx;

/// 팝업이 속하는 범위의 종류. 대상 id는 열 때 정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupScopeKind {
    Window,
    Workspace,
    Pane,
    Tab,
    Surface,
}

/// 범위 대상을 어디서 가져오는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupScopeTarget {
    /// 여는 요청이 나온 곳(surface·tab 등).
    Origin,
    /// 부모 팝업의 범위.
    Parent,
    /// 여는 쪽이 대상을 직접 지정한다.
    Explicit,
}

/// 같은 선언으로 여러 인스턴스를 열 수 있는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupInstances {
    /// 창 안에 하나.
    Single,
    /// 범위 대상마다 하나.
    PerScopeTarget,
    /// 같은 범위 안에 여럿.
    Multi,
    /// 하나씩 차례로 보인다.
    Queue,
}

/// 범위가 팝업보다 좁을 때의 처리.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupFit {
    /// 접었다가 펼친다.
    Collapse,
    /// 범위 안에 맞춰 줄인다.
    Shrink,
}

/// 범위 안에서 팝업을 놓는 기준.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupAnchor {
    ScopeCenter,
    ScopeTop,
    AtPoint,
    Cursor,
}

/// 팝업이 받는 키.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupKeyboard {
    None,
    /// 팝업의 조작 키만 받는다.
    Keys,
    /// 글자·편집·이동 키를 받는 입력칸이 있다.
    Text,
}

/// Esc를 눌렀을 때.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupEscape {
    Close,
    /// 팝업은 남기고 키보드 포커스만 놓는다.
    Unfocus,
    Ignore,
}

/// 팝업 바깥(범위 안)을 눌렀을 때.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupOutsideClick {
    Close,
    /// 팝업은 남기고 키보드 포커스만 놓는다.
    Unfocus,
    Ignore,
}

/// 셸의 제목줄.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupChrome {
    /// 공통 제목줄과 닫기 버튼을 그린다.
    TitleBar,
    /// 콘텐츠만 그린다.
    Headless,
}

/// 끌어 옮길 수 있는 영역.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupDragHandle {
    None,
    /// 공통 제목줄. 제목줄이 없으면 옮길 수 없다.
    TitleBar,
    /// 팝업이 정한 영역. host는 영역을 계산하는 함수를 정의에 함께 둔다.
    Region,
}

/// 셸의 배경·테두리·제목줄 아래 선 토큰 묶음.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupShell {
    /// 배경 `surface-raised`, 테두리 `border-frame`.
    Raised,
    /// 배경 `bg-panel`, 테두리 `border-frame`.
    Panel,
    /// 배경 `bg-panel`, 테두리 `border-frame`, 제목줄 아래 선 `info-modal-title-edge`.
    InfoPanel,
    /// 배경 `dialog-bg`, 테두리 `border-frame`.
    Dialog,
    /// 배경 `surface-raised`, 테두리 `menu-border`.
    Menu,
    /// 배경 `surface-raised`, 테두리 `banner-more-menu-border`.
    BannerMenu,
    /// 배경 `surface-raised`, 테두리 `search-bar-border`.
    SearchBar,
}

/// 셸 그림자 토큰.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupShadow {
    /// `shadow-modal`.
    Modal,
    /// `shadow-popover`.
    Popover,
}

/// 콘텐츠 영역 둘레의 공통 여백.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupContentMargin {
    /// 공통 내부 여백(`space-xs`).
    Default,
    /// 여백 없음. 콘텐츠가 구역별 여백을 직접 둔다.
    None,
}

/// 논리 픽셀 크기. JSON에는 각 값이 숫자 하나로 들어간다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PopupSize {
    pub width: LogicalPx,
    pub height: LogicalPx,
}

/// 팝업 공통 선언. 함수가 아닌 속성만 담는다.
///
/// 지금 host가 읽는 필드: `covers_scope`(scrim), `outside_click`, `sticky_focus`, `chrome`,
/// `drag_handle`, `resizable`, `min_size`, `size`, `shell`, `shadow`, `content_margin`.
/// 선언만 있고 읽는 곳이 없는 필드: `scope_kind`(등록 범위는 Window이고 실제 범위는 여는 쪽이 준다),
/// `scope_target`, `instances`(지금은 id당 하나), `fit`(지금 host는 범위에 맞춰 줄이기만 한다),
/// `anchor`(위치는 여는 쪽이 정한다), `scope_inset`, `focus_on_open`, `keyboard`, `escape`
/// (Esc는 팝업 그리기 함수와 키 처리 경로가 처리한다).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PopupDecl {
    pub scope_kind: PopupScopeKind,
    pub scope_target: PopupScopeTarget,
    pub instances: PopupInstances,
    /// 참이면 소속 범위를 scrim으로 덮는다.
    pub covers_scope: bool,
    pub fit: PopupFit,
    pub anchor: PopupAnchor,
    /// Surface 범위일 때 경계에서 안쪽으로 띄운다.
    pub scope_inset: bool,
    /// 사용자가 열면 키보드 포커스를 가져간다. 사용자 조작이 아닌 열기는 이 값과 관계없이 포커스가 없다.
    pub focus_on_open: bool,
    pub keyboard: PopupKeyboard,
    pub escape: PopupEscape,
    pub outside_click: PopupOutsideClick,
    /// 바깥을 눌러도 키보드 포커스를 유지한다.
    pub sticky_focus: bool,
    pub chrome: PopupChrome,
    pub drag_handle: PopupDragHandle,
    /// 테두리를 끌어 크기를 바꿀 수 있다.
    pub resizable: bool,
    /// 크기 조절의 최소 크기. 없으면 `size`.
    pub min_size: Option<PopupSize>,
    /// 처음 크기. 크기를 매 프레임 계산하는 팝업은 첫 프레임 값이다.
    pub size: PopupSize,
    pub shell: PopupShell,
    pub shadow: PopupShadow,
    pub content_margin: PopupContentMargin,
}

impl PopupDecl {
    /// 범위 종류와 크기만 정하고 나머지는 기본값을 쓴다.
    ///
    /// 기본값: 덮음(`covers_scope = true`), 바깥 클릭은 포커스 해제, Esc는 닫기,
    /// 범위 대상마다 인스턴스 하나, 좁은 범위에서는 접었다가 펼침.
    pub const fn new(scope_kind: PopupScopeKind, size: PopupSize) -> Self {
        Self {
            scope_kind,
            scope_target: PopupScopeTarget::Origin,
            instances: PopupInstances::PerScopeTarget,
            covers_scope: true,
            fit: PopupFit::Collapse,
            anchor: PopupAnchor::ScopeCenter,
            scope_inset: true,
            focus_on_open: true,
            keyboard: PopupKeyboard::Keys,
            escape: PopupEscape::Close,
            outside_click: PopupOutsideClick::Unfocus,
            sticky_focus: false,
            chrome: PopupChrome::TitleBar,
            drag_handle: PopupDragHandle::TitleBar,
            resizable: false,
            min_size: None,
            size,
            shell: PopupShell::Raised,
            shadow: PopupShadow::Modal,
            content_margin: PopupContentMargin::Default,
        }
    }

    /// 제목줄이 없는 셸인지.
    pub const fn is_headless(&self) -> bool {
        matches!(self.chrome, PopupChrome::Headless)
    }

    /// 바깥 클릭으로 닫히는지.
    pub const fn closes_on_outside_click(&self) -> bool {
        matches!(self.outside_click, PopupOutsideClick::Close)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: PopupSize = PopupSize {
        width: LogicalPx(200.0),
        height: LogicalPx(100.0),
    };

    #[test]
    fn the_defaults_are_the_decided_ones() {
        let d = PopupDecl::new(PopupScopeKind::Surface, SIZE);
        assert!(d.covers_scope);
        assert_eq!(d.outside_click, PopupOutsideClick::Unfocus);
        assert_eq!(d.escape, PopupEscape::Close);
        assert_eq!(d.instances, PopupInstances::PerScopeTarget);
        assert_eq!(d.fit, PopupFit::Collapse);
        assert_eq!(d.scope_kind, PopupScopeKind::Surface);
        assert_eq!(d.size, SIZE);
    }

    #[test]
    fn a_declaration_survives_a_json_round_trip() {
        let mut d = PopupDecl::new(PopupScopeKind::Window, SIZE);
        d.shell = PopupShell::SearchBar;
        d.fit = PopupFit::Shrink;
        d.min_size = Some(PopupSize {
            width: LogicalPx(10.0),
            height: LogicalPx(20.0),
        });
        let json = serde_json::to_value(d).expect("serialize");
        assert_eq!(json["shell"], "search_bar");
        assert_eq!(json["scope_kind"], "window");
        // 길이는 단위 이름 없이 숫자 그대로 직렬화된다.
        assert_eq!(
            json["size"],
            serde_json::json!({ "width": 200.0, "height": 100.0 })
        );
        assert_eq!(
            json["min_size"],
            serde_json::json!({ "width": 10.0, "height": 20.0 })
        );
        let back: PopupDecl = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, d);
    }
}
