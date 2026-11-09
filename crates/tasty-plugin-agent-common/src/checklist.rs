//! 체크리스트 게이트의 판정 핵심 — 답에 완료 표식이 있거나 회차가 상한에 이르렀으면 통과하고,
//! 아니면 한 회차 더 이어 간다.
//!
//! 회차를 어디에 두고 언제 0 으로 되돌릴지는 호출자가 정한다. claude 의 Stop 훅은 세션별 파일과
//! prompt_id 로, 대화 탭은 탭 상태와 보낸 말로 정한다. 두 쪽이 같은 표식과 같은 순서로 판정해야
//! 사용자가 터미널과 대화 탭에서 같은 말로 일을 끝낼 수 있다.

/// 기본 완료 표식. 일반 문장과 겹치지 않는 문자열을 부분 일치로 찾는다.
pub const DEFAULT_SENTINEL: &str = "[[TASTY-CHECKLIST-DONE]]";

/// 판정 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// 끝났다 — 더 이어 가지 않는다.
    Pass,
    /// 한 회차 더 이어 간다. `rounds` 는 이번이 몇 번째 회차인지다.
    Continue { rounds: u32 },
}

/// 끝난 턴의 답을 판정한다. `current_rounds` 는 같은 요청에 이미 쓴 회차다(새 요청이면 0).
pub fn decide(answer: &str, sentinel: &str, current_rounds: u32, round_limit: u32) -> Decision {
    if answer.contains(sentinel) || current_rounds >= round_limit {
        return Decision::Pass;
    }
    Decision::Continue {
        rounds: current_rounds + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_with_the_sentinel_passes_even_on_the_first_round() {
        let answer = format!("done {DEFAULT_SENTINEL}");
        assert_eq!(decide(&answer, DEFAULT_SENTINEL, 0, 3), Decision::Pass);
    }

    #[test]
    fn an_answer_without_the_sentinel_continues_with_the_next_round() {
        assert_eq!(
            decide("not yet", DEFAULT_SENTINEL, 1, 3),
            Decision::Continue { rounds: 2 }
        );
    }

    #[test]
    fn the_round_limit_passes_without_the_sentinel() {
        assert_eq!(decide("not yet", DEFAULT_SENTINEL, 3, 3), Decision::Pass);
        assert_eq!(decide("not yet", DEFAULT_SENTINEL, 4, 3), Decision::Pass);
    }
}
