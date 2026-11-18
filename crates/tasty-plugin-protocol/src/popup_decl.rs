//! 팝업 공통 선언 `PopupDecl`.
//!
//! 팝업 사이의 동작 차이는 이 선언 값으로만 정한다. 팝업 id를 비교해 규칙을 정하지 않는다.
//! host 팝업 정의(`PopupDef`)가 이 타입을 담는다. plugin이 실행 중에 보내는 선언은
//! [`crate::popup_spec::PopupDeclSpec`]이고, host가 [`PopupDecl::plugin`] 위에 덮어 이 타입을 만든다.
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
    /// 범위 대상마다 하나이고, 그 안에서 항목을 하나씩 차례로 보인다.
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
    /// 범위 위쪽에 붙는다. host는 이런 팝업이 차지한 띠를 다른 배치가 피할 자리로 알려 준다.
    ScopeTop,
    AtPoint,
    Cursor,
    /// 범위 오른쪽 위의 안내 열에 연 순서로 위에서부터 쌓는다. 열의 자리는 host가 프레임마다 정한다.
    Column,
}

/// 팝업이 받는 키.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupKeyboard {
    /// 키를 받지 않는다. 열거나 눌러도 키보드 포커스를 가져가지 않고 맨 앞으로 올라오지 않으며,
    /// 다른 팝업 아래에 그린다. 안쪽 컨트롤의 포커스는 팝업 콘텐츠가 따로 알린다.
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
    /// 공통 셸(배경·테두리·그림자)을 그리지 않는다. 팝업 콘텐츠가 셸을 직접 그린다.
    Content,
}

/// 크기를 바꾸려고 잡는 곳. `resizable`이 참일 때만 뜻이 있다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PopupResizeHandle {
    /// 네 변과 네 모서리.
    #[default]
    Edges,
    /// 오른쪽 아래 정사각형 그립만. 그립은 팝업 콘텐츠가 그린다.
    Grip { size: LogicalPx },
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
/// 지금 host가 읽는 필드: `covers_scope`(scrim), `instances`, `outside_click`, `sticky_focus`, `chrome`,
/// `drag_handle`, `resizable`, `resize_handle`, `min_size`, `size`, `shell`, `shadow`, `content_margin`,
/// `keyboard`(`None`인지만), `anchor`(`Column`과 `ScopeTop`만).
/// 선언만 있고 읽는 곳이 없는 필드: `scope_kind`(등록 범위는 Window이고 실제 범위는 여는 쪽이 준다),
/// `scope_target`, `fit`(지금 host는 범위에 맞춰 줄이기만 한다), 나머지 `anchor` 값(위치는 여는 쪽이
/// 정한다), `scope_inset`, `focus_on_open`, `escape`(Esc는 팝업 그리기 함수와 키 처리 경로가 처리한다).
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
    /// 지금 host의 notifications는 이 값이 참이지만 단축키로 열 때는 포커스를 가져가지 않는다(열기 경로 불일치).
    pub focus_on_open: bool,
    pub keyboard: PopupKeyboard,
    /// 포커스된 팝업에서 Esc를 눌렀을 때. 지금 host에서 하위 화면이 있는 팝업(dag_list, remote_tool)은
    /// `Close`지만 하위 화면에서는 첫 Esc가 하위 화면만 닫는다.
    pub escape: PopupEscape,
    pub outside_click: PopupOutsideClick,
    /// 바깥을 눌러도 키보드 포커스를 유지한다.
    pub sticky_focus: bool,
    pub chrome: PopupChrome,
    pub drag_handle: PopupDragHandle,
    /// 테두리를 끌어 크기를 바꿀 수 있다.
    pub resizable: bool,
    #[serde(default)]
    pub resize_handle: PopupResizeHandle,
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
            resize_handle: PopupResizeHandle::Edges,
            min_size: None,
            size,
            shell: PopupShell::Raised,
            shadow: PopupShadow::Modal,
            content_margin: PopupContentMargin::Default,
        }
    }

    /// plugin 팝업의 기본 선언. 동작 기본값은 [`PopupDecl::new`]와 같고, 셸 모양은 host가 정한다.
    ///
    /// plugin 콘텐츠는 egui-mesh로 영역 전체를 그리므로 제목줄과 끌기 손잡이가 없다.
    /// 지금 plugin 팝업 셸은 이 선언의 `shell`·`shadow`·`content_margin`을 읽지 않고
    /// plugin 팝업 렌더러가 정한 모양을 그린다.
    pub const fn plugin(scope_kind: PopupScopeKind, size: PopupSize) -> Self {
        let mut decl = Self::new(scope_kind, size);
        decl.chrome = PopupChrome::Headless;
        decl.drag_handle = PopupDragHandle::None;
        decl
    }

    /// 크기 값이 쓸 수 있는지 확인한다. 크기는 양수이고 최소 크기는 처음 크기를 넘지 않는다.
    pub fn check_sizes(&self) -> Result<(), String> {
        check_positive("size", self.size)?;
        if let Some(min) = self.min_size {
            check_positive("min_size", min)?;
            if min.width.value() > self.size.width.value()
                || min.height.value() > self.size.height.value()
            {
                return Err("min_size must not exceed size".to_string());
            }
        }
        Ok(())
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

fn check_positive(name: &str, size: PopupSize) -> Result<(), String> {
    let ok = |v: f32| v.is_finite() && v > 0.0;
    if ok(size.width.value()) && ok(size.height.value()) {
        Ok(())
    } else {
        Err(format!("{name} width and height must be positive numbers"))
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
    fn a_plugin_declaration_has_no_title_bar() {
        let d = PopupDecl::plugin(PopupScopeKind::Window, SIZE);
        assert!(d.is_headless());
        assert_eq!(d.drag_handle, PopupDragHandle::None);
        let mut expected = PopupDecl::new(PopupScopeKind::Window, SIZE);
        expected.chrome = PopupChrome::Headless;
        expected.drag_handle = PopupDragHandle::None;
        assert_eq!(d, expected);
    }

    #[test]
    fn sizes_must_be_positive_and_min_must_fit() {
        let mut d = PopupDecl::new(PopupScopeKind::Window, SIZE);
        assert!(d.check_sizes().is_ok());
        d.size.width = LogicalPx(0.0);
        assert!(d.check_sizes().is_err());
        d.size.width = LogicalPx(f32::NAN);
        assert!(d.check_sizes().is_err());
        d.size = SIZE;
        d.min_size = Some(PopupSize {
            width: LogicalPx(300.0),
            height: LogicalPx(50.0),
        });
        assert_eq!(
            d.check_sizes().expect_err("min wider than size"),
            "min_size must not exceed size"
        );
        d.min_size = Some(PopupSize {
            width: LogicalPx(200.0),
            height: LogicalPx(100.0),
        });
        assert!(d.check_sizes().is_ok());
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

    /// 그립 크기 조절은 크기와 함께 직렬화되고, 값이 없는 선언은 네 변 조절로 읽는다.
    #[test]
    fn the_resize_handle_round_trips_and_defaults_to_edges() {
        let mut d = PopupDecl::new(PopupScopeKind::Window, SIZE);
        d.resize_handle = PopupResizeHandle::Grip {
            size: LogicalPx(16.0),
        };
        d.anchor = PopupAnchor::Column;
        d.keyboard = PopupKeyboard::None;
        let mut json = serde_json::to_value(d).expect("serialize");
        assert_eq!(
            json["resize_handle"],
            serde_json::json!({ "grip": { "size": 16.0 } })
        );
        assert_eq!(json["anchor"], "column");
        let back: PopupDecl = serde_json::from_value(json.clone()).expect("deserialize");
        assert_eq!(back, d);
        json.as_object_mut()
            .expect("object")
            .remove("resize_handle");
        let old: PopupDecl = serde_json::from_value(json).expect("deserialize without the field");
        assert_eq!(old.resize_handle, PopupResizeHandle::Edges);
    }
}
