//! 부팅 첫머리의 데이터 홈 writer 잠금 선점. GUI와 헤드리스가 함께 쓴다.
//!
//! 로그 파일·memory.db·설정·플러그인·저널·IPC를 건드리기 전에 이 홈의 구조 저널 writer 잠금을 잡는다.
//! 다른 Tasty가 쥐고 있으면(`Held`) 실행 중인 인스턴스의 홈에 부작용을 내지 않고 끝낸다.

use std::path::{Path, PathBuf};

pub(crate) enum HomeWriterLock {
    Acquired {
        lock: tasty_event_store::WriterLock,
        /// GUI는 이 홈에 인스턴스 기록(`tasty.instance`)을 쓴다. 헤드리스는 쓰지 않는다.
        #[cfg(feature = "gui")]
        home: PathBuf,
    },
    /// 다른 프로세스가 이 홈의 저널 writer 잠금을 쥐고 있다.
    Held(PathBuf),
    /// 홈을 알 수 없거나 잠금을 쓸 수 없다. 저널 worker가 다시 시도하고 그 결과로 판정한다.
    Unchecked,
}

/// 잡혀 있으면 저널 worker와 같은 구간(`WRITER_LOCK_WAIT`) 동안 다시 시도한다.
pub(crate) fn preempt_home_writer_lock() -> HomeWriterLock {
    let Some(home) = tasty_utils::path::tasty_home() else {
        return HomeWriterLock::Unchecked;
    };
    // release 단일 실행: PID·시작 시각이 살아 있고 포트가 있는 인스턴스 기록이면 잠금 재시도
    // (`WRITER_LOCK_WAIT`)를 기다리지 않고 바로 넘긴다. 낡은 기록이나 포트 없는 기록은 재시도 경로로 간다.
    // 인스턴스 기록은 GUI만 쓰므로 헤드리스는 이 지름길을 쓰지 않는다.
    #[cfg(feature = "gui")]
    if !cfg!(debug_assertions)
        && matches!(
            super::single_instance::instance_file::read_state(&home),
            super::single_instance::instance_file::InstanceState::Live(_)
        )
    {
        return HomeWriterLock::Held(home);
    }
    let database = crate::runtime::journal_product::journal_database_path(&home);
    match tasty_event_store::preempt_writer_lock(&database) {
        Ok(tasty_event_store::WriterPreempt::Acquired(lock)) => HomeWriterLock::Acquired {
            lock,
            #[cfg(feature = "gui")]
            home,
        },
        Ok(tasty_event_store::WriterPreempt::Held) => HomeWriterLock::Held(home),
        Err(e) => {
            tracing::warn!(
                target: "tasty::boot",
                "journal writer lock preempt failed for {}: {e}; the journal worker will retry",
                database.display()
            );
            HomeWriterLock::Unchecked
        }
    }
}

/// "이미 사용 중인 데이터 폴더" 문구(제목, 본문, 힌트). 본문에 홈 경로를 넣는다.
pub(crate) fn home_in_use_text(home: &Path) -> (String, String, String) {
    let title = crate::i18n::t("boot.home_in_use.title").to_string();
    let body = crate::i18n::t_fmt("boot.home_in_use.body", &home.display().to_string());
    let hint = crate::i18n::t("boot.home_in_use.hint").to_string();
    (title, body, hint)
}

/// 헤드리스의 `Held` 거절. 홈의 파일을 만들거나 바꾸지 않고 stderr에 문구를 쓴 뒤 종료 코드 1을 돌려준다.
/// 로그 필터(`TASTY_LOG`)와 무관하게 보이도록 tracing이 아니라 stderr에 직접 쓴다. stderr 쓰기 실패로
/// 패닉하면 크래시 보고가 홈에 기록될 수 있어 쓰기 오류를 흡수하는 출력 함수를 쓴다.
#[cfg(not(feature = "gui"))]
pub(crate) fn refuse_headless(home: &Path) -> std::process::ExitCode {
    let (title, body, hint) = home_in_use_text(home);
    tasty_cli::out::err_line(format_args!("{title}\n{body}\n{hint}"));
    std::process::ExitCode::from(1)
}
