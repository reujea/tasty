//! surface가 포커스를 어떻게 보이는지 정한다. 표현 방법은 kind가, 값은 포커스와 점유가 정한다.
//! 계약은 `docs/design/policies/focus.md`의 "Surface 포커스 표현" 절에 있다.

use crate::core::attach::OccupancyRegistry;
use crate::runtime::surface_registry::builtins::is_host_builtin_kind;

/// 포커스를 보이는 방법. 호스트가 바탕을 보장할 수 있는 kind만 바탕을 바꾼다. 나머지는 leaf 위쪽 띠다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FocusMark {
    Bed,
    Edge,
}

/// kind와 그 kind가 선언한 능력으로 표현을 고른다. `bed_focus`를 선언하지 않은 plugin kind는 띠다.
/// 선언은 페이지가 바탕을 host 테마 값으로 칠한다는 약속이다. 호스트는 그 페이지의 바탕을 직접 보장하지 못한다.
pub(crate) fn focus_mark(
    kind: &str,
    capabilities: crate::runtime::surface_registry::KindCapabilities,
) -> FocusMark {
    if capabilities.bed_focus || is_host_builtin_kind(kind) {
        FocusMark::Bed
    } else {
        FocusMark::Edge
    }
}

/// 이번 프레임에 포커스 띠를 칠할 leaf 하나(논리 px).
#[cfg(feature = "gui")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FocusEdgeLeaf {
    pub(crate) surface_id: u32,
    pub(crate) leaf: egui::Rect,
    /// leaf에서 띠 행을 뺀 내용 영역. 링은 띠 leaf에서 이 영역만 칠하고 chrome은 이 위 끝에서 시작한다.
    pub(crate) below: egui::Rect,
    /// 물리 leaf에서 정한 행 경계. 띠는 이 값으로 칠해 내용 예약과 같은 물리 px에서 끝난다.
    pub(crate) rows: tasty_ui_widgets::SurfaceFocusEdgeRows,
    /// 그 프레임의 host 키 포커스. hard 점유여도 키 포커스를 따른다.
    pub(crate) focused: bool,
    /// 띠 행 안에 다시 칠할 링.
    pub(crate) ring: Option<egui::Stroke>,
}

/// leaf 위에 띠 행을 예약하는 webview인지. 띠 kind인 webview kind만이다.
#[cfg(feature = "gui")]
pub(crate) fn webview_reserves_edge(
    kind: &str,
    capabilities: crate::runtime::surface_registry::KindCapabilities,
) -> bool {
    crate::runtime::surface_registry::webview_kind::is_webview_kind(kind)
        && focus_mark(kind, capabilities) == FocusMark::Edge
}

/// leaf 위에 띠 행을 예약하는 surface인지. 띠 kind인 webview·로컬 mesh·mirror mesh만 예약한다.
/// 로컬 mesh는 소유 plugin의 선언을, mirror mesh는 서버가 실은 선언을 본다. 그 밖의 surface는 띠가 없다.
#[cfg(feature = "gui")]
pub(crate) fn reserves_edge(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    surface_id: u32,
) -> bool {
    use crate::runtime::surface_registry::KindCapabilities;
    let Some(surface) = engine.find_surface_by_id(surface_id) else {
        return false;
    };
    if let Some(webview) = surface.remote_webview() {
        return webview_reserves_edge(webview.kind(), webview.capabilities());
    }
    if let Some(mesh) = surface.mesh() {
        let capabilities = engine
            .surface_registry
            .owner_capabilities(mesh.kind_static, &mesh.plugin_id);
        return focus_mark(mesh.kind_static, capabilities) == FocusMark::Edge;
    }
    if let Some(mesh) = surface.attach_mesh() {
        let capabilities = KindCapabilities {
            bed_focus: mesh.bed_focus,
            ..KindCapabilities::default()
        };
        return focus_mark(mesh.kind_static, capabilities) == FocusMark::Edge;
    }
    false
}

/// 물리 leaf의 띠 행 경계(정수 물리 px). 내용 예약·띠 칠하기·chrome이 모두 이 값을 쓴다.
#[cfg(feature = "gui")]
pub(crate) fn edge_rows(
    theme: &tasty_type_appearance::theme::Theme,
    leaf: crate::model::PhysicalRect,
    scale_factor: f32,
) -> tasty_ui_widgets::SurfaceFocusEdgeRows {
    let top = leaf.y.value();
    tasty_ui_widgets::surface_focus_edge_rows(theme, top, top + leaf.height.value(), scale_factor)
}

/// 띠 leaf의 내용 영역: leaf에서 띠 행을 뺀 나머지다. 경계는 [`edge_rows`]가 물리 px로 정하고
/// 띠가 leaf를 다 차지하면 높이 0이다.
#[cfg(feature = "gui")]
pub(crate) fn below_edge(
    theme: &tasty_type_appearance::theme::Theme,
    leaf: crate::model::PhysicalRect,
    scale_factor: f32,
) -> crate::model::PhysicalRect {
    use crate::model::PhysicalPx;
    let bottom = leaf.y.value() + leaf.height.value();
    let rows = edge_rows(theme, leaf, scale_factor);
    crate::model::PhysicalRect {
        x: leaf.x,
        y: PhysicalPx(rows.bottom),
        width: leaf.width,
        height: PhysicalPx((bottom - rows.bottom).max(0.0)),
    }
}

/// 바탕 kind가 focus 값을 칠할지 정한다. 키 포커스를 가져도 입력을 받지 않는 점유 내용은 unfocus 값이다.
/// `key_focus`는 그 프레임의 host 키 포커스 surface(`MainViewState::frame_key_focus`)이고, 키를 가진 팝업 아래나
/// attach 대기 선택이면 `None`이다. OS 창의 활성 여부를 보지 않는다.
/// 입력 판단에는 쓰지 않는다. 입력은 포커스 노드와 점유 입력 검사를 따로 본다.
pub(crate) fn bed_focused(
    key_focus: Option<u32>,
    surface: u32,
    occupancy: &OccupancyRegistry,
) -> bool {
    key_focus == Some(surface) && !occupancy.is_content_hidden(surface)
}

/// 터미널 하나의 포커스 칠하기 결정. 바탕·기본 잉크·검색 활성 잉크가 `bed_focused`를, PTY 커서가 `show_cursor`를
/// 읽는다. 값은 이 프레임의 키 포커스(`state::key_focus`)와 점유가 정한다. vi 복사 커서는 따로 그리는 overlay다.
#[cfg(feature = "gui")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalFocusPaint {
    pub(crate) bed_focused: bool,
    /// focused 바탕인데 출력 중이라 PTY 커서를 숨긴다.
    pub(crate) cursor_suppressed: bool,
    pub(crate) show_cursor: bool,
}

#[cfg(feature = "gui")]
impl TerminalFocusPaint {
    /// `suppress_during_output`은 focused 바탕일 때만 부른다(Windows에서는 터미널 락을 시도한다).
    pub(crate) fn of(
        key_focus: Option<u32>,
        surface_id: u32,
        occupancy: &OccupancyRegistry,
        suppress_during_output: impl FnOnce() -> bool,
    ) -> Self {
        let bed_focused =
            crate::state::surface_focus::bed_focused(key_focus, surface_id, occupancy);
        let cursor_suppressed = bed_focused && suppress_during_output();
        Self {
            bed_focused,
            cursor_suppressed,
            show_cursor: bed_focused && !cursor_suppressed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bed_is_focused_only_on_the_unoccupied_focused_surface() {
        let free = OccupancyRegistry::new();
        assert!(bed_focused(Some(1), 1, &free), "focused");
        assert!(!bed_focused(Some(2), 1, &free), "another surface focused");
        assert!(!bed_focused(None, 1, &free), "no focused surface");

        let mut hard = OccupancyRegistry::new();
        hard.acquire(1, 41).unwrap();
        assert!(!bed_focused(Some(1), 1, &hard), "hard occupied surface");

        let mut workspace = OccupancyRegistry::new();
        workspace.acquire_workspace(7, &[], &[1], 41).unwrap();
        assert!(
            !bed_focused(Some(1), 1, &workspace),
            "member of an occupied workspace"
        );
    }

    /// terminal은 점유 workspace에서도 surface lock을 함께 받는다. 그래서 `is_hard_occupied`로
    /// 판정하던 때와 terminal의 결과가 같다. 비터미널 멤버만 이 판정으로 값이 달라진다.
    #[test]
    fn an_occupied_workspace_terminal_reads_the_same_as_a_hard_lock() {
        let mut reg = OccupancyRegistry::new();
        reg.acquire_workspace(7, &[1], &[1, 2], 41).unwrap();
        assert!(reg.is_hard_occupied(1));
        assert!(!bed_focused(Some(1), 1, &reg));
        assert!(!reg.is_hard_occupied(2));
        assert!(!bed_focused(Some(2), 2, &reg));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn the_content_starts_below_the_edge_on_a_whole_physical_pixel() {
        use crate::model::{PhysicalPx, PhysicalRect};
        let theme = tasty_themes::mocha_fallback();
        let leaf = PhysicalRect {
            x: PhysicalPx(10.0),
            y: PhysicalPx(100.75),
            width: PhysicalPx(300.0),
            height: PhysicalPx(200.0),
        };
        let below = below_edge(&theme, leaf, 1.25);
        assert_eq!(below.y.value(), 103.0);
        assert_eq!(
            below.y.value() + below.height.value(),
            300.75,
            "bottom stays"
        );
        assert_eq!((below.x, below.width), (leaf.x, leaf.width));
        let at_one = below_edge(&theme, leaf, 1.0);
        assert_eq!(at_one.y.value(), 103.0);
        let flat = PhysicalRect {
            height: PhysicalPx(1.0),
            ..leaf
        };
        assert_eq!(below_edge(&theme, flat, 2.0).height.value(), 0.0);
    }

    #[test]
    fn the_mark_follows_the_kind_and_its_source() {
        use crate::runtime::surface_registry::KindCapabilities;
        let none = KindCapabilities::default();
        let bed = KindCapabilities {
            bed_focus: true,
            ..KindCapabilities::default()
        };
        for kind in ["terminal", "empty", "explorer", "dag_graph"] {
            assert_eq!(focus_mark(kind, none), FocusMark::Bed, "{kind}");
        }
        assert_eq!(focus_mark("my-viewer", bed), FocusMark::Bed);
        assert_eq!(
            focus_mark("markdown", none),
            FocusMark::Edge,
            "no declaration"
        );
        // 선언하지 않은 공식 kind는 등록 때 해석한 능력으로 바탕을 받는다.
        let resolve = |kind, plugin| {
            focus_mark(
                kind,
                KindCapabilities::resolve(&Default::default(), kind, plugin),
            )
        };
        assert_eq!(resolve("markdown", "com.tasty.markdown"), FocusMark::Bed);
        assert_eq!(resolve("image", "com.tasty.image"), FocusMark::Bed);
        assert_eq!(resolve("html", "com.tasty.html"), FocusMark::Edge);
        assert_eq!(
            resolve("markdown", "com.example.markdown"),
            FocusMark::Edge,
            "the same kind name from another plugin"
        );
        assert_eq!(resolve("image", "com.example.image"), FocusMark::Edge);
    }
}
