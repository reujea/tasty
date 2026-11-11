//! GTK 초기화를 한 곳에서 묻고 첫 결과를 보관한다.
//!
//! 트레이·네이티브 메뉴·WebView·원격 웹주소가 각자 GTK를 필요로 한다. 초기화 실패는 대개 시작 환경
//! (`GDK_BACKEND` 등)에서 오지만, 기본 main context를 얻지 못한 경우처럼 실행 중 상황에서 오는 실패도
//! 있다. 그래도 다시 시도하지 않는다. 실패를 본 surface는 WebView 생성을 영구 실패로 멈추고, 원격
//! 웹주소를 쓸 수 있는지도 창을 만들 때 정한다. GTK만 나중에 살아나면 표시 경로 안내가 사라진 빈 화면이
//! 남는다. 첫 결과를 프로세스 수명 동안 고정해 안내와 surface 상태가 같은 사실을 가리키게 한다.

use std::sync::OnceLock;

static OUTCOME: OnceLock<Result<(), String>> = OnceLock::new();

/// GTK가 쓸 수 있는 상태인지. 다른 곳이 이미 초기화했으면 성공이다. 아니면 처음 한 번만 이 스레드에서
/// 초기화를 시도하고, 이후에는 그 결과를 돌려준다. 실패는 다시 시도하지 않는다.
pub fn ensure() -> Result<(), String> {
    if gtk::is_initialized() {
        return Ok(());
    }
    first_outcome(&OUTCOME, || gtk::init().map_err(|error| error.to_string()))
}

/// 보관된 첫 초기화 결과가 실패인지. 아직 아무도 묻지 않았으면 `false`다.
pub fn failed() -> bool {
    matches!(OUTCOME.get(), Some(Err(_)))
}

fn first_outcome(
    cell: &OnceLock<Result<(), String>>,
    init: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    cell.get_or_init(init).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given 첫 초기화가 실패했다, When 다시 묻는다, Then 초기화를 다시 부르지 않고 같은 실패를 돌려준다.
    #[test]
    fn a_failure_is_kept_and_not_retried() {
        let cell = OnceLock::new();
        let first = first_outcome(&cell, || Err("no display".to_owned()));
        let second = first_outcome(&cell, || panic!("init must not run again"));
        assert_eq!(first, Err("no display".to_owned()));
        assert_eq!(second, first);
        assert!(matches!(cell.get(), Some(Err(_))));
    }
}
