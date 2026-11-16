//! `PhysicalRect` — 픽셀 좌표 사각형.

use crate::direction::SplitDirection;
use crate::length::{LogicalPx, PhysicalPx};

/// A pixel rectangle in physical (device) pixels, used for viewport/scissor calculations.
#[derive(Debug, Clone, Copy)]
pub struct PhysicalRect {
    pub x: PhysicalPx,
    pub y: PhysicalPx,
    pub width: PhysicalPx,
    pub height: PhysicalPx,
}

/// 논리 픽셀 사각형. PhysicalRect와 변환할 때 네 필드에 같은 배율을 적용한다.
/// egui에 의존하지 않으므로 해당 타입으로 바꾸는 작업은 호출자가 맡는다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRect {
    pub x: LogicalPx,
    pub y: LogicalPx,
    pub width: LogicalPx,
    pub height: LogicalPx,
}

impl LogicalRect {
    /// 논리 사각형을 물리 좌표로 올린다.
    pub fn to_physical(self, scale_factor: f32) -> PhysicalRect {
        PhysicalRect {
            x: self.x.to_physical(scale_factor),
            y: self.y.to_physical(scale_factor),
            width: self.width.to_physical(scale_factor),
            height: self.height.to_physical(scale_factor),
        }
    }

    /// 점 (x, y) 가 이 사각형 안인지. `PhysicalRect::contains` 와 같이 왼쪽·위 끝은 넣고 오른쪽·아래 끝은
    /// 뺀다. 맞닿은 두 사각형의 경계 점은 오른쪽·아래 사각형 하나에만 든다.
    pub fn contains(&self, x: LogicalPx, y: LogicalPx) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

impl PhysicalRect {
    /// 물리 사각형을 egui 가 쓰는 논리 좌표로 내린다.
    pub fn to_logical(self, scale_factor: f32) -> LogicalRect {
        LogicalRect {
            x: self.x.to_logical(scale_factor),
            y: self.y.to_logical(scale_factor),
            width: self.width.to_logical(scale_factor),
            height: self.height.to_logical(scale_factor),
        }
    }

    /// Check if a point (x, y) is inside this rectangle.
    pub fn contains(&self, x: PhysicalPx, y: PhysicalPx) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Check if two rects are approximately equal (within 1px tolerance).
    pub fn approx_eq(&self, other: &PhysicalRect) -> bool {
        (self.x - other.x).abs() < PhysicalPx(1.0)
            && (self.y - other.y).abs() < PhysicalPx(1.0)
            && (self.width - other.width).abs() < PhysicalPx(1.0)
            && (self.height - other.height).abs() < PhysicalPx(1.0)
    }

    /// 두 영역으로 분할한다. gap은 두 영역 사이의 간격이며 호출자가 지정한다.
    pub fn split_with_gap(
        self,
        direction: SplitDirection,
        ratio: f32,
        gap: PhysicalPx,
    ) -> (PhysicalRect, PhysicalRect) {
        match direction {
            SplitDirection::Vertical => {
                let usable = (self.width - gap).max(PhysicalPx(0.0));
                let first_w = (usable * ratio).floor();
                let second_w = usable - first_w;
                (
                    PhysicalRect {
                        x: self.x,
                        y: self.y,
                        width: first_w,
                        height: self.height,
                    },
                    PhysicalRect {
                        x: self.x + first_w + gap,
                        y: self.y,
                        width: second_w,
                        height: self.height,
                    },
                )
            }
            SplitDirection::Horizontal => {
                let usable = (self.height - gap).max(PhysicalPx(0.0));
                let first_h = (usable * ratio).floor();
                let second_h = usable - first_h;
                (
                    PhysicalRect {
                        x: self.x,
                        y: self.y,
                        width: self.width,
                        height: first_h,
                    },
                    PhysicalRect {
                        x: self.x,
                        y: self.y + first_h + gap,
                        width: self.width,
                        height: second_h,
                    },
                )
            }
        }
    }
}

/// 분할 보더 (divider) 정보. 사용자가 hover 시 그릴 영역.
#[derive(Debug, Clone, Copy)]
pub struct DividerInfo {
    /// The direction of the split this divider belongs to.
    pub direction: SplitDirection,
    /// The rect of the parent split node that owns this divider.
    pub split_rect: PhysicalRect,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 두 좌표계를 왕복한 뒤 원래 사각형으로 돌아오는지 확인한다.
    #[test]
    fn physical_and_logical_round_trip_cancels() {
        let physical = PhysicalRect {
            x: PhysicalPx(120.0),
            y: PhysicalPx(48.0),
            width: PhysicalPx(640.0),
            height: PhysicalPx(360.0),
        };
        for sf in [1.0_f32, 1.5, 2.0, 3.0] {
            let back = physical.to_logical(sf).to_physical(sf);
            assert!(
                back.approx_eq(&physical),
                "sf={sf} 에서 왕복이 어긋난다: {back:?}"
            );
        }
    }

    #[test]
    fn to_logical_divides_every_side_by_the_scale_factor() {
        let l = PhysicalRect {
            x: PhysicalPx(100.0),
            y: PhysicalPx(200.0),
            width: PhysicalPx(300.0),
            height: PhysicalPx(400.0),
        }
        .to_logical(2.0);
        assert_eq!(l.x, LogicalPx(50.0));
        assert_eq!(l.y, LogicalPx(100.0));
        assert_eq!(l.width, LogicalPx(150.0));
        assert_eq!(l.height, LogicalPx(200.0));
    }

    /// 논리 사각형도 물리 사각형과 같은 반열림 규칙으로 점을 판정한다.
    #[test]
    fn logical_contains_uses_the_same_half_open_rule_as_physical() {
        let physical = PhysicalRect {
            x: PhysicalPx(10.0),
            y: PhysicalPx(20.0),
            width: PhysicalPx(30.0),
            height: PhysicalPx(40.0),
        };
        let logical = physical.to_logical(1.0);
        for (x, y) in [
            (10.0, 20.0),
            (39.9, 59.9),
            (40.0, 30.0),
            (20.0, 60.0),
            (9.9, 30.0),
        ] {
            assert_eq!(
                logical.contains(LogicalPx(x), LogicalPx(y)),
                physical.contains(PhysicalPx(x), PhysicalPx(y)),
                "({x}, {y})"
            );
        }
        assert!(logical.contains(LogicalPx(10.0), LogicalPx(20.0)));
        assert!(!logical.contains(LogicalPx(40.0), LogicalPx(30.0)));
        assert!(!logical.contains(LogicalPx(20.0), LogicalPx(60.0)));
    }
}
