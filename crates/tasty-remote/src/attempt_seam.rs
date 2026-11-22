//! 시험 전용: 연결 단계 사이에 시험이 끼어드는 자리. 시도 토큰을 등록한 직후와 open 직전에 시험이 건
//! 함수를 한 번 부르고, 어느 자리에 닿았는지 남긴다. 연결 함수는 시험 스레드에서 직접 부르므로 thread-local
//! 로 충분하다.

use std::cell::RefCell;
use std::net::TcpStream;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// `register_socket` 성공 직후, 소켓 기한 설정 전.
    AfterRegister,
    /// 기한 설정 뒤 handshake open 직전.
    BeforeOpen,
}

type Hook = Box<dyn FnOnce(&TcpStream)>;

thread_local! {
    static HOOK: RefCell<Option<(Stage, Hook)>> = const { RefCell::new(None) };
    static REACHED: RefCell<Vec<Stage>> = const { RefCell::new(Vec::new()) };
}

/// 이 스레드의 기록을 지우고 `stage` 에서 부를 함수를 건다.
pub fn arm(stage: Stage, hook: impl FnOnce(&TcpStream) + 'static) {
    REACHED.with(|reached| reached.borrow_mut().clear());
    HOOK.with(|slot| *slot.borrow_mut() = Some((stage, Box::new(hook))));
}

/// 이 스레드에서 연결 함수가 `stage` 에 닿았는지.
pub fn reached(stage: Stage) -> bool {
    REACHED.with(|reached| reached.borrow().contains(&stage))
}

/// 연결 함수가 `stage` 에 닿았다고 남기고, 그 자리에 건 함수가 있으면 한 번 부른다.
pub fn at(stage: Stage, socket: &TcpStream) {
    REACHED.with(|reached| reached.borrow_mut().push(stage));
    let hook = HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_some_and(|(armed, _)| *armed == stage) {
            slot.take().map(|(_, hook)| hook)
        } else {
            None
        }
    });
    if let Some(hook) = hook {
        hook(socket);
    }
}
