//! 터미널 생성 실패가 자식 프로세스 생성 전에 났는지 후에 났는지 구분한다.
//!
//! 호출자는 이 구분으로 실패를 확정할 수 있는지 판단한다. 자식이 생기기 전 실패는 OS에
//! 아무것도 남기지 않으므로 알려진 실패다. 자식이 생긴 뒤 실패는 [`Pty`](crate::Pty) 의
//! Drop 이 자식을 회수하지만 호출자는 그 회수를 관측하지 못한다.
//!
//! 단계 경계는 portable-pty 의 `SlavePty::spawn_command` 반환이다.
//! - Unix: `openpty`, 실행 파일 검색(`access(X_OK)`), stdio 복제는 fork 전에 실패한다.
//!   fork 자체의 실패는 자식을 만들지 않는다. fork 뒤 pre_exec 의 `setsid`·`TIOCSCTTY` 가
//!   실패하면 std `Command::spawn` 이 오류 파이프로 errno 를 받고 그 자식을 회수한 뒤 오류를
//!   돌려준다. 따라서 `spawn_command` 가 오류를 돌려줄 때 살아 있는 자식은 없다.
//!   exec 실패는 오류로 돌아오지 않는다. pre_exec 가 stderr 위의 모든 descriptor 를 닫으면서
//!   std 의 오류 파이프도 닫기 때문이다. 그 자식은 생성에 성공한 뒤 스스로 종료한다.
//! - Windows: Git Bash 가 아닌 셸의 거절은 `spawn_command` 전에 일어난다.
//!   `openpty` 의 `CreatePseudoConsole`, `spawn_command` 안의 속성 목록·명령줄 준비는
//!   `CreateProcessW` 전에 실패한다. `CreateProcessW` 가 0 을 돌려주면 프로세스가 만들어지지
//!   않는다.

use std::fmt;

/// 생성 실패가 난 단계.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnStage {
    /// PTY 할당, 실행 파일 확인, 프로세스 생성 중 하나가 실패해 자식이 남지 않았다.
    BeforeChild,
    /// 자식이 생긴 뒤 입출력 준비나 worker 시작이 실패했다. 자식은 Drop 한 Pty 가 회수한다.
    AfterChild,
}

/// 단계가 붙은 터미널 생성 오류. 표시 문구는 원래 오류와 같다.
#[derive(Debug)]
pub struct SpawnError {
    stage: SpawnStage,
    error: anyhow::Error,
}

impl SpawnError {
    pub(crate) fn before_child(error: impl Into<anyhow::Error>) -> Self {
        Self {
            stage: SpawnStage::BeforeChild,
            error: error.into(),
        }
    }

    pub(crate) fn after_child(error: impl Into<anyhow::Error>) -> Self {
        Self {
            stage: SpawnStage::AfterChild,
            error: error.into(),
        }
    }

    pub fn stage(&self) -> SpawnStage {
        self.stage
    }

    /// 자식이 남지 않은 실패인지.
    pub fn is_before_child(&self) -> bool {
        self.stage == SpawnStage::BeforeChild
    }
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for SpawnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}
