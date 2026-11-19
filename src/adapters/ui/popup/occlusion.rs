//! 겹친 팝업의 마우스 입력 대상을 판별한다. 상위 팝업에 덮인 좌표는 내 입력도
//! 바깥 클릭도 아니므로 닫기·포커스 변경을 하지 않는다.
//! host와 plugin은 같은 전역 z_seq를 사용한다.

use egui::{Pos2, Rect};

/// 한 좌표에 대한 popup 하나의 소유 판정.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointOwnership {
    /// 내 rect 안이고, 나보다 위에 있는 popup 이 그 좌표를 덮지 않았다 → 내가 소비.
    Mine,
    /// 나보다 z 가 높은 popup 이 그 좌표를 덮었다 → 내 것도 아니고 "바깥" 도 아니다.
    /// dismiss 도 focus-bump 도 하지 않는다.
    OccludedByHigher,
    /// 내 영역 밖이며 더 위의 팝업에도 가려지지 않은 좌표.
    OutsideAll,
}

/// `(rect, z_seq)` 로 표현한 다른 popup 하나. host/plugin 구분 없이 같은 배열에 담는다.
#[derive(Debug, Clone, Copy)]
pub struct Occluder {
    pub rect: Rect,
    pub z_seq: u64,
}

/// 상위 팝업에 가려졌는지 먼저 확인한 뒤 내 영역 안인지 검사한다.
/// 내 영역 밖이라도 위의 팝업이 덮고 있으면 바깥 클릭으로 처리하지 않는다.
/// others 순서는 무관하고 자기 자신을 포함해도 된다. 같은 z는 가림으로 보지 않는다.
/// 더 아래 팝업의 영역은 이 팝업의 판정에 영향을 주지 않는다.
pub fn point_ownership(my_rect: Rect, my_z: u64, others: &[Occluder], p: Pos2) -> PointOwnership {
    if others.iter().any(|o| o.z_seq > my_z && o.rect.contains(p)) {
        return PointOwnership::OccludedByHigher;
    }
    if my_rect.contains(p) {
        return PointOwnership::Mine;
    }
    PointOwnership::OutsideAll
}

/// 키 입력을 받는 팝업. host 쪽은 어느 팝업인지 가리지 않는다. host 팝업끼리는 host 규칙이 정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupKeyOwner {
    Host,
    /// plugin 팝업 instance id.
    Plugin(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
    }

    /// 재현 시나리오: 420×170 `file-open`(plugin) 위에 640×480 `file_picker`(host).
    /// 화면 중앙 정렬이라 작은 쪽이 큰 쪽 안에 들어간다.
    fn scenario() -> (Rect, Rect) {
        let parent = r(310.0, 415.0, 420.0, 170.0); // plugin file-open
        let child = r(200.0, 260.0, 640.0, 480.0); // host file_picker
        (parent, child)
    }

    #[test]
    fn overlapping_point_under_a_higher_popup_is_occluded() {
        let (parent, child) = scenario();
        let p = parent.center(); // 두 rect 모두 안
        assert_eq!(
            point_ownership(
                parent,
                1,
                &[Occluder {
                    rect: child,
                    z_seq: 2
                }],
                p
            ),
            PointOwnership::OccludedByHigher
        );
    }

    #[test]
    fn inside_higher_popup_but_outside_mine_is_occluded_not_outside() {
        let (parent, child) = scenario();
        let p = egui::pos2(child.min.x + 10.0, child.min.y + 10.0);
        assert!(!parent.contains(p));
        assert_eq!(
            point_ownership(
                parent,
                1,
                &[Occluder {
                    rect: child,
                    z_seq: 2
                }],
                p
            ),
            PointOwnership::OccludedByHigher
        );
    }

    #[test]
    fn point_only_inside_mine_is_mine() {
        let (parent, _) = scenario();
        let far = r(0.0, 0.0, 50.0, 50.0);
        assert_eq!(
            point_ownership(
                parent,
                1,
                &[Occluder {
                    rect: far,
                    z_seq: 9
                }],
                parent.center()
            ),
            PointOwnership::Mine
        );
    }

    #[test]
    fn point_outside_every_popup_is_outside_all() {
        let (parent, child) = scenario();
        let p = egui::pos2(5.0, 5.0);
        assert_eq!(
            point_ownership(
                parent,
                1,
                &[Occluder {
                    rect: child,
                    z_seq: 2
                }],
                p
            ),
            PointOwnership::OutsideAll
        );
    }

    /// z 역전(내가 위) — 겹치는 좌표는 내 것이다.
    #[test]
    fn higher_z_wins_the_overlap() {
        let (parent, child) = scenario();
        assert_eq!(
            point_ownership(
                parent,
                5,
                &[Occluder {
                    rect: child,
                    z_seq: 2
                }],
                parent.center()
            ),
            PointOwnership::Mine
        );
    }

    /// 아래 popup 이 덮는 좌표는 나에겐 여전히 바깥이다(내가 위이므로).
    #[test]
    fn lower_popup_coverage_does_not_shield_me() {
        let (parent, child) = scenario();
        let p = egui::pos2(child.min.x + 10.0, child.min.y + 10.0);
        assert_eq!(
            point_ownership(
                parent,
                5,
                &[Occluder {
                    rect: child,
                    z_seq: 2
                }],
                p
            ),
            PointOwnership::OutsideAll
        );
    }

    /// 같은 z는 가림으로 보지 않는다.
    #[test]
    fn equal_z_does_not_occlude() {
        let (parent, child) = scenario();
        assert_eq!(
            point_ownership(
                parent,
                2,
                &[Occluder {
                    rect: child,
                    z_seq: 2
                }],
                parent.center()
            ),
            PointOwnership::Mine
        );
    }
}
