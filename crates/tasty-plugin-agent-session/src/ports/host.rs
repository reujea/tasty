//! 호스트 포트 — 로직이 호스트(Tasty 본체)에 닿는 유일한 길.
//!
//! 실물은 조립 루트(`main.rs`)의 `SdkHost` 이고, 시험은 `surface_link::tests_support` 의 가짜를 쓴다.

use serde_json::Value;

/// 호스트 IPC 한 줄을 부르는 수단 — SDK 의 `host.call` 을 감싼다.
///
/// trait 으로 둔 이유는 **시험이 호스트 없이 이 논리를 재기 위해서**다.
/// `agent-stream` 의 `HostCall` 과 같은 형태이며, 그쪽 crate 에 의존하지
/// 않으려고 여기 다시 선언한다(plugin 간 의존을 만들지 않는다).
pub trait HostCall {
    fn call(&self, method: &str, params: Value) -> Result<Value, String>;

    /// 이 plugin 에게 IPC 메서드를 되던진다 — 워커 스레드가 결과를 처리 스레드로 넘기는 길(SDK `self_invoke`).
    ///
    /// 실물(main.rs 의 `SdkHost`)은 덮어쓴다. 기본값은 거절이다 — 되던지기를 보지 않는 시험 가짜가 구현을
    /// 생략할 수 있게 둔다. 되던진 것을 재는 시험은 가짜에서 덮어쓴다.
    fn self_invoke(&self, method: &str, _params: Value) -> Result<(), String> {
        Err(format!(
            "self_invoke({method}) is not available on this host"
        ))
    }
}

/// 빌린 호스트도 호스트다 — `&H` 를 받는 함수가 다시 `&impl HostCall` 로 넘길 수 있게.
impl<T: HostCall + ?Sized> HostCall for &T {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        (**self).call(method, params)
    }

    fn self_invoke(&self, method: &str, params: Value) -> Result<(), String> {
        (**self).self_invoke(method, params)
    }
}

/// 워커 스레드로 옮길 수 있는 호스트 — 복제해 스레드에 넘기고 거기서 되던진다(`self_invoke`).
///
/// 스레드를 띄우는 자리만 이것을 요구한다. 실물은 SDK 핸들을 소유한 `SdkHost` 다.
pub trait SendHost: HostCall + Clone + Send + 'static {}

impl<T: HostCall + Clone + Send + 'static> SendHost for T {}

/// 호스트가 그 메서드를 모른다는 거절인가 — 이 plugin 은 호스트에 없을 수 있는 표시용
/// 메서드(`surface.set_busy`)를 부르고, 없으면 그 표시 없이 돈다.
pub fn host_lacks_method(err: &str) -> bool {
    err.contains("unknown ipc method")
}
