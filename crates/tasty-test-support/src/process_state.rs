//! `/proc/<pid>/stat`의 상태 글자로 프로세스가 끝났는지 읽는다.
//!
//! 커널은 끝난 프로세스를 `Z`(회수를 기다림), `X`(회수 중), `/proc` 항목 삭제 순서로 넘기고
//! 되돌리지 않는다. `Z`만 끝으로 읽으면 회수자가 두 읽기 사이에 `wait`를 끝냈을 때 "끝남" 다음에
//! "살아 있음"을 읽는다. 호출자는 파일을 읽지 못한 경우도 끝난 것으로 처리해야 판정이 단조롭다.

/// `stat` 한 줄이 끝난 프로세스(`Z`, `X`)를 가리키는지. 둘째 필드(comm)는 괄호와 공백을 품을 수
/// 있어 마지막 `)` 뒤에서 읽는다. 해석할 수 없는 줄은 끝난 것으로 보지 않는다.
pub fn proc_stat_reads_finished(stat: &str) -> bool {
    stat.rsplit_once(')')
        .and_then(|(_, rest)| rest.trim_start().chars().next())
        .is_some_and(|state| matches!(state, 'Z' | 'X'))
}

#[cfg(test)]
mod tests {
    use super::proc_stat_reads_finished as finished;

    fn stat(comm: &str, state: char) -> String {
        format!("12 ({comm}) {state} 1 12 12 0 -1 4194304 0 0 0 0 0 0 0 0 20 0 1 0 77 0 0")
    }

    #[test]
    fn a_zombie_and_a_process_being_reaped_read_as_finished() {
        assert!(finished(&stat("sh", 'Z')));
        assert!(finished(&stat("sh", 'X')));
    }

    #[test]
    fn running_sleeping_and_disk_waiting_processes_do_not_read_as_finished() {
        for state in ['R', 'S', 'D'] {
            assert!(!finished(&stat("sh", state)), "{state}");
        }
    }

    #[test]
    fn the_state_is_read_after_the_last_parenthesis_of_the_comm() {
        assert!(!finished(&stat("a) Z b", 'S')));
        assert!(!finished(&stat("a) X", 'R')));
        assert!(finished(&stat("a) R b", 'X')));
    }

    #[test]
    fn a_line_that_cannot_be_parsed_does_not_read_as_finished() {
        assert!(!finished(""));
        assert!(!finished("Z"));
        assert!(!finished("12 (sh)"));
    }
}
