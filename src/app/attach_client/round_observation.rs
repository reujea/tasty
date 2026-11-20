//! 출력 회차 시작에 찍는 세션 전송의 끝 관측값.
//!
//! 한 회차는 대기 연결 설치를 세션 끝 분류보다 먼저 한다. reader 스레드는 그 사이에도 전송 표지를
//! 세울 수 있으므로, 설치 검사와 끝 분류가 각자 표지를 읽으면 같은 회차 안에서 서로 다른 시점을
//! 본다. 설치 검사는 다른 행의 강제 끊김을 못 보고 설치했는데, 끝 분류는 그 강제 끊김을 보고 표지를
//! 저장하는 순서가 생긴다. 두 판정이 회차 시작의 관측값 하나를 쓰면, 관측 뒤에 선 표지는 다음 회차에
//! 분류되어 그 끊김은 이번 회차의 설치 뒤에 일어난 것과 같아진다.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tasty_remote::client_session::{AttachClientSession, SessionState};

/// 세션 하나의 관측값. 전송은 `disconnected` 표지의 `Arc` 로 알아본다. 회차 중에 설치가 세션의
/// 전송을 바꾸면 관측값은 그 세션에 맞지 않는다.
struct SessionEnd {
    transport: Arc<AtomicBool>,
    disconnected: bool,
    force_detached: bool,
    phase: SessionState,
    saved_row: Option<u32>,
}

pub(crate) struct RoundObservation {
    sessions: Vec<SessionEnd>,
}

impl RoundObservation {
    /// 지금 세션들의 전송 표지를 읽는다. `disconnected` 를 먼저 읽는다. reader 는 통지를 처리해
    /// `force_detached` 를 세운 뒤 `disconnected` 를 세우므로, 끊김을 본 관측에서 통지가 빠지지 않는다.
    pub(crate) fn observe(sessions: &[AttachClientSession]) -> Self {
        let sessions = sessions
            .iter()
            .map(|session| {
                let disconnected = session.transport.disconnected.load(Ordering::SeqCst);
                SessionEnd {
                    transport: session.transport.disconnected.clone(),
                    disconnected,
                    force_detached: session.transport.force_detached.load(Ordering::SeqCst),
                    phase: session.state.phase,
                    saved_row: session.state.saved_row,
                }
            })
            .collect();
        Self { sessions }
    }

    /// 세션의 (끊김, 강제 끊김) 관측값. 회차 밖이거나 회차 시작 뒤에 전송이 바뀐 세션은 지금 값을 읽는다.
    pub(crate) fn end_of(round: Option<&Self>, session: &AttachClientSession) -> (bool, bool) {
        round
            .into_iter()
            .flat_map(|round| round.sessions.iter())
            .find(|end| Arc::ptr_eq(&end.transport, &session.transport.disconnected))
            .map(|end| (end.disconnected, end.force_detached))
            .unwrap_or_else(|| {
                let disconnected = session.transport.disconnected.load(Ordering::SeqCst);
                (
                    disconnected,
                    session.transport.force_detached.load(Ordering::SeqCst),
                )
            })
    }

    /// 회차 시작에 끝 분류 전(Connected)이면서 강제로 끊긴 저장 행. 이번 회차의 끝 분류가 강제
    /// 끊김으로 분류할 행들이다.
    pub(crate) fn force_detached_rows(&self) -> impl Iterator<Item = u32> + '_ {
        self.sessions
            .iter()
            .filter(|end| {
                end.phase == SessionState::Connected && end.disconnected && end.force_detached
            })
            .filter_map(|end| end.saved_row)
    }
}
