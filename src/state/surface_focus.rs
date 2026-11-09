//! surface가 포커스를 어떻게 보이는지 정한다. 표현 방법은 kind가, 값은 포커스와 점유가 정한다.
//! 계약은 `docs/design/policies/focus.md`의 "Surface 포커스 표현" 절에 있다.

use crate::core::attach::OccupancyRegistry;
use crate::runtime::surface_registry::builtins::is_host_builtin_kind;

/// 포커스를 보이는 방법. 호스트가 바탕을 보장할 수 있는 kind만 바탕을 바꾸고 나머지는 띠를 그린다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FocusMark {
    Bed,
    Edge,
}

/// kind와 그 kind를 등록한 플러그인으로 표현을 고른다. 모르는 kind는 띠다.
/// markdown·image는 이름만 보지 않는다. 번들 플러그인이 빠진 설치에서는 제3자가 같은 이름을
/// 등록할 수 있고, 그 페이지의 바탕은 호스트가 보장하지 못한다. `com.tasty.` id는 공식
/// 플러그인 전용이므로(`tasty_host_plugin::official`) 제3자가 이 id를 쓸 수 없다.
pub(crate) fn focus_mark(kind: &str, plugin_id: Option<&str>) -> FocusMark {
    let bundled_bed = matches!(
        (kind, plugin_id),
        ("markdown", Some("com.tasty.markdown")) | ("image", Some("com.tasty.image"))
    );
    if bundled_bed || is_host_builtin_kind(kind) {
        FocusMark::Bed
    } else {
        FocusMark::Edge
    }
}

/// 바탕 kind가 focus 값을 칠할지 정한다. 포커스를 가져도 입력을 받지 않는 점유 내용은 unfocus 값이다.
/// `focused`는 attach 대기 선택이면 `None`이고, OS 창의 활성 여부를 보지 않는다.
/// 입력 판단에는 쓰지 않는다. 입력은 포커스 자체와 점유 입력 검사를 따로 본다.
pub(crate) fn bed_focused(
    focused: Option<u32>,
    surface: u32,
    occupancy: &OccupancyRegistry,
) -> bool {
    focused == Some(surface) && !occupancy.is_content_hidden(surface)
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

    #[test]
    fn the_mark_follows_the_kind_and_its_source() {
        for kind in ["terminal", "empty", "explorer", "dag_graph"] {
            assert_eq!(focus_mark(kind, None), FocusMark::Bed, "{kind}");
        }
        assert_eq!(
            focus_mark("markdown", Some("com.tasty.markdown")),
            FocusMark::Bed
        );
        assert_eq!(focus_mark("image", Some("com.tasty.image")), FocusMark::Bed);
        assert_eq!(focus_mark("html", Some("com.tasty.html")), FocusMark::Edge);
        assert_eq!(
            focus_mark("markdown", Some("com.example.markdown")),
            FocusMark::Edge,
            "the same kind name from another plugin"
        );
        assert_eq!(
            focus_mark("image", Some("com.example.image")),
            FocusMark::Edge
        );
        assert_eq!(focus_mark("markdown", None), FocusMark::Edge);
        assert_eq!(focus_mark("unknown", Some("com.example")), FocusMark::Edge);
    }
}
