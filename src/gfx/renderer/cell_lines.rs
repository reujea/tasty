//! 셀이 긋는 선의 기하. 글꼴의 선 지표를 읽지 않아 대체 글꼴이 섞여도 한 줄이다.
//! 모두 물리 픽셀이고 셀 위쪽에서 잰다. 정본은 디자인 RULES의 "Lines a cell draws"다.

/// 모든 선의 굵기 `T = max(1, round(H / 16))`. 반올림은 half up이다.
pub(crate) fn line_thickness(cell_height: u32) -> u32 {
    ((cell_height + 8) / 16).max(1)
}

/// 격자 기준선 바로 아래 행 `B`. 대체 글꼴도 같은 격자 기준선에 놓인다.
pub(crate) fn row_under_baseline(baseline: f32) -> u32 {
    baseline.ceil().max(0.0) as u32
}

/// 밑줄 위쪽 가장자리 `U = min(B + T, 셀 아래쪽 - h)`. `h`는 그 밑줄이 차지하는 높이다.
pub(crate) fn underline_top(baseline_row: u32, thickness: u32, cell_height: u32, h: u32) -> u32 {
    (baseline_row + thickness).min(cell_height.saturating_sub(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thickness_follows_the_cell_height_rounding_half_up() {
        for (height, thickness) in [
            (1, 1),
            (7, 1),
            (8, 1),
            (14, 1),
            (17, 1),
            (23, 1),
            (24, 2),
            (34, 2),
            (39, 2),
            (40, 3),
            (51, 3),
        ] {
            assert_eq!(line_thickness(height), thickness, "H {height}");
        }
    }

    #[test]
    fn the_row_under_a_fractional_baseline_is_the_next_whole_row() {
        assert_eq!(row_under_baseline(11.34), 12);
        assert_eq!(row_under_baseline(12.0), 12);
        assert_eq!(row_under_baseline(0.2), 1);
    }

    #[test]
    fn an_underline_sits_one_thickness_under_the_baseline_row() {
        // D2Coding 없이 잰 28px, 줄 높이 1.2 칸의 값이다.
        assert_eq!(underline_top(26, 2, 34, 2), 28);
    }

    #[test]
    fn a_line_stays_inside_its_cell() {
        // 14px, 줄 높이 1.0: 위치가 칸 아래쪽 한 행으로 눌린다.
        assert_eq!(underline_top(12, 1, 14, 1), 13);
        assert_eq!(underline_top(13, 1, 14, 1), 13);
        // 세 칸 높이를 쓰는 밑줄 종류는 그만큼 더 올라온다.
        assert_eq!(underline_top(13, 1, 14, 3), 11);
        // 칸보다 큰 선은 위쪽에 닿는다.
        assert_eq!(underline_top(1, 2, 1, 2), 0);
    }
}
