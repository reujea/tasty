//! surface 바탕의 포커스 값을 정한다. 값은 포커스와 점유가 정한다.
//! 계약은 `docs/design/policies/focus.md`의 "Surface 포커스 표현" 절에 있다.

use crate::core::attach::OccupancyRegistry;

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
}
