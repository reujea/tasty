//! attention 표시의 공용 판정과 그리기. 본체 사이드바·탭 바·surface 테두리와 갤러리가
//! 같은 함수를 불러 순위(NeedsInput > Completion)와 색 토큰이 갈라지지 않게 한다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

use crate::{BadgeVariant, badge};

/// 화면에 표시하는 attention 종류. 본체 상태의 종류를 이 값으로 옮겨 넘긴다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attention {
    NeedsInput,
    Completion,
}

/// 탭 제목 색. 순위는 NeedsInput > Completion > 활성 > 기본이다.
pub fn tab_title_color(theme: &Theme, attention: Option<Attention>, active: bool) -> HexColor {
    match attention {
        Some(Attention::NeedsInput) => theme.tab_fg_needs_input(),
        Some(Attention::Completion) => theme.tab_fg_completion(),
        None if active => theme.tab_fg_active(),
        None => theme.tab_fg(),
    }
}

/// surface 테두리에 남는 attention. NeedsInput은 점유보다 위이고 Completion은 점유 아래라
/// 점유 중에는 숨긴다.
pub fn surface_edge_attention(attention: Option<Attention>, occupied: bool) -> Option<Attention> {
    match attention {
        Some(Attention::NeedsInput) => Some(Attention::NeedsInput),
        Some(Attention::Completion) if !occupied => Some(Attention::Completion),
        _ => None,
    }
}

/// 점유 테두리를 그릴지. NeedsInput 테두리가 같은 자리에 오면 점유선은 그리지 않는다.
/// 한 테두리에는 선 하나만 남긴다(시안 Tab title & surface border — 두 선을 겹치지 않는다).
pub fn occupancy_edge_shows(attention: Option<Attention>) -> bool {
    attention != Some(Attention::NeedsInput)
}

/// attention 테두리의 선. 두 종류 모두 2px(focus-ring-width)이다.
pub fn attention_edge_stroke(theme: &Theme, attention: Attention) -> egui::Stroke {
    match attention {
        Attention::NeedsInput => egui::Stroke::new(
            theme.surface_highlight_input_width().value(),
            theme.surface_highlight_input_border(),
        ),
        Attention::Completion => egui::Stroke::new(
            theme.surface_highlight_done_width().value(),
            theme.surface_highlight_done_border(),
        ),
    }
}

/// 점유 테두리의 선. soft는 초록, hard(읽기 전용)는 peach이고 둘 다 1px이다.
pub fn occupancy_edge_stroke(theme: &Theme, hard: bool) -> egui::Stroke {
    let color = if hard {
        theme.surface_occupied_hard_border()
    } else {
        theme.surface_occupied_soft_border()
    };
    egui::Stroke::new(theme.surface_occupied_border_width().value(), color)
}

/// 개수를 숫자로 보이는 상한(`semantic.count-display-max`). 워크스페이스 배지·실행 중 개수·
/// 알림 배지가 이 값 하나를 따른다.
pub(crate) const COUNT_DISPLAY_MAX: usize =
    tasty_design_tokens::generated::semantic::COUNT_DISPLAY_MAX as usize;

/// 워크스페이스 행 개수 배지의 글자. 상한을 넘으면 `상한+`(99+)로 줄인다.
pub fn attention_count_label(count: usize) -> String {
    if count > COUNT_DISPLAY_MAX {
        format!("{COUNT_DISPLAY_MAX}+")
    } else {
        count.to_string()
    }
}

/// 워크스페이스 행 끝의 배지 묶음. 호출부의 right-to-left 레이아웃 안에서 부른다.
/// 하나만 있으면 끝 자리에 두고, 둘이면 NeedsInput이 앞, Completion이 끝이다.
/// 배지를 그렸으면 true를 돌려준다.
pub fn workspace_attention_badges(
    ui: &mut egui::Ui,
    theme: &Theme,
    needs_input: usize,
    completion: usize,
) -> bool {
    if completion > 0 {
        badge(
            ui,
            theme,
            &attention_count_label(completion),
            BadgeVariant::Primary,
        );
    }
    if needs_input > 0 {
        if completion > 0 {
            ui.add_space(theme.badge_group_gap().value());
        }
        badge(
            ui,
            theme,
            &attention_count_label(needs_input),
            BadgeVariant::Warning,
        );
    }
    needs_input + completion > 0
}

/// 접힌 레일 아바타의 점 하나. 순위는 NeedsInput > Completion > 실행 중이다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailDot {
    NeedsInput,
    Completion,
    Busy,
}

impl RailDot {
    pub fn resolve(needs_input: bool, completion: bool, busy: bool) -> Option<Self> {
        if needs_input {
            Some(RailDot::NeedsInput)
        } else if completion {
            Some(RailDot::Completion)
        } else if busy {
            Some(RailDot::Busy)
        } else {
            None
        }
    }
}

/// 아바타 오른쪽 위에 레일 점을 그린다. 실행 중 점을 포함한 모든 점을 사이드바 배경 고리로
/// 둘러 아바타와 겹쳐도 점이 떨어져 보이게 한다.
pub fn paint_rail_dot(painter: &egui::Painter, theme: &Theme, avatar: egui::Rect, dot: RailDot) {
    let radius = theme.status_dot_size_compact().value() * 0.5;
    let pad = theme.spacing_xs.value();
    let center = egui::pos2(avatar.max.x - pad - radius, avatar.min.y + pad + radius);
    let color = match dot {
        RailDot::NeedsInput => theme.status_dot_needs_input(),
        RailDot::Completion => theme.status_dot_completion(),
        RailDot::Busy => theme.status_dot_success(),
    };
    painter.circle_filled(
        center,
        radius + theme.status_dot_ring_width().value(),
        theme.status_dot_ring(),
    );
    painter.circle_filled(center, radius, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rail_dot_takes_the_highest_rank() {
        assert_eq!(
            RailDot::resolve(true, true, true),
            Some(RailDot::NeedsInput)
        );
        assert_eq!(
            RailDot::resolve(false, true, true),
            Some(RailDot::Completion)
        );
        assert_eq!(RailDot::resolve(false, false, true), Some(RailDot::Busy));
        assert_eq!(RailDot::resolve(false, false, false), None);
    }

    #[test]
    fn completion_edge_hides_under_occupancy_but_needs_input_does_not() {
        use Attention::*;
        assert_eq!(
            surface_edge_attention(Some(NeedsInput), true),
            Some(NeedsInput)
        );
        assert_eq!(surface_edge_attention(Some(Completion), true), None);
        assert_eq!(
            surface_edge_attention(Some(Completion), false),
            Some(Completion)
        );
        assert_eq!(surface_edge_attention(None, false), None);
    }

    #[test]
    fn needs_input_replaces_the_occupancy_edge_and_completion_does_not() {
        assert!(!occupancy_edge_shows(Some(Attention::NeedsInput)));
        assert!(occupancy_edge_shows(Some(Attention::Completion)));
        assert!(occupancy_edge_shows(None));
    }

    #[test]
    fn counts_over_two_digits_collapse() {
        assert_eq!(attention_count_label(99), "99");
        assert_eq!(attention_count_label(100), "99+");
    }

    #[test]
    fn every_count_notation_follows_the_design_maximum() {
        let max = tasty_design_tokens::generated::semantic::COUNT_DISPLAY_MAX as usize;
        let over = format!("{max}+");
        assert_eq!(attention_count_label(max), max.to_string());
        assert_eq!(attention_count_label(max + 1), over);
        assert_eq!(crate::notifications::count_label(max), max.to_string());
        assert_eq!(crate::notifications::count_label(max + 1), over);
        assert_eq!(
            crate::running_count::running_count_label(max + 1).as_deref(),
            Some(over.as_str())
        );
    }

    /// 값만 보는 시험은 상한이 99인 동안 리터럴 99로 되돌린 코드도 통과시킨다. 그래서 두 함수의
    /// 본문이 리터럴 대신 상수를 읽는지 소스에서 본다. 실행 중 개수는 attention_count_label을 부른다.
    #[test]
    fn the_count_labels_read_the_display_max_instead_of_a_literal() {
        fn body<'a>(src: &'a str, head: &str) -> &'a str {
            let start = src.find(head).unwrap_or_else(|| panic!("{head} not found"));
            let rest = &src[start..];
            let end = rest
                .find("\n}\n")
                .unwrap_or_else(|| panic!("{head} has no end"));
            &rest[..end]
        }
        for (file, src, head) in [
            (
                "attention.rs",
                include_str!("attention.rs"),
                "pub fn attention_count_label(",
            ),
            (
                "notifications.rs",
                include_str!("notifications.rs"),
                "pub(crate) fn count_label(",
            ),
        ] {
            let body = body(src, head);
            assert!(
                body.contains("COUNT_DISPLAY_MAX"),
                "{file} {head} must read COUNT_DISPLAY_MAX:\n{body}"
            );
            assert!(
                !body.contains("99"),
                "{file} {head} must not compare against a literal:\n{body}"
            );
        }
    }

    #[test]
    fn tab_title_attention_outranks_active() {
        let th = tasty_themes::mocha_fallback();
        assert_eq!(
            tab_title_color(&th, Some(Attention::Completion), true),
            th.tab_fg_completion()
        );
        assert_eq!(tab_title_color(&th, None, true), th.tab_fg_active());
        assert_eq!(tab_title_color(&th, None, false), th.tab_fg());
    }
}
