//! 포트 스캔 폴링은 about_to_wait와 redraw_main_window마다 돈다. 이 경로에서 EventLoopProxy를 clone하면
//! macOS는 clone마다 런루프를 깨우고 Wayland는 clone한 sender를 drop할 때마다 깨워 idle 메인 루프가 쉬지 않는다.
//! 요청을 꺼낸 뒤에만 clone하도록 src/app/port_scans.rs의 폴링 함수 세 개만 검사한다.
//! 다른 폴링 함수, 호출 관계, 실제 깨움 수는 보지 않는다. 시험 루프(X11)는 clone으로 깨지 않아 깨움 수로는 잴 수 없다.

use super::{fn_body, mask_non_code, repo_root};

const HOME: &str = "src/app/port_scans.rs";
const REQUEST: &str = "take_request()";
/// proxy를 clone하거나 만드는 표현. worker_wake와 spawn_scan_worker는 안에서 clone한다.
const CLONES: &[&str] = &[
    "proxy.clone()",
    "create_proxy(",
    "worker_wake(",
    "spawn_scan_worker(",
];
/// 요청을 꺼내는 일 없이 폴링마다 실행되는 함수. 이 안에는 clone이 하나도 없어야 한다.
const ALWAYS: &[&str] = &["fn poll_port_scans", "fn collect_port_scan_results"];
const START: &str = "fn start_pending_port_scans";

/// 첫 요청 꺼내기보다 앞에 있는 clone 표현을 돌려준다. 요청을 꺼내지 않는 본문이면 모든 clone이 위반이다.
fn clones_before_request(body: &str) -> Vec<&'static str> {
    let request = body.find(REQUEST).unwrap_or(body.len());
    CLONES
        .iter()
        .copied()
        .filter(|clone| body.find(clone).is_some_and(|at| at < request))
        .collect()
}

fn body_of(signature: &str) -> String {
    let src = std::fs::read_to_string(repo_root().join(HOME))
        .unwrap_or_else(|e| panic!("{HOME}을 읽지 못했다: {e}"));
    let body = fn_body(&src, signature).unwrap_or_else(|| {
        panic!("{HOME}에서 `{signature}` 본문을 찾지 못했다. 이름·위치를 확인한다.")
    });
    mask_non_code(&body)
}

#[test]
fn port_scan_polling_clones_the_proxy_only_after_taking_a_request() {
    let start = body_of(START);
    assert!(
        start.contains(REQUEST),
        "`{START}`에 `{REQUEST}`가 없다. 요청을 꺼내는 자리가 바뀌었으면 이 검사의 기준도 함께 옮긴다."
    );
    assert!(
        CLONES.iter().any(|clone| start.contains(clone)),
        "`{START}`에서 worker용 proxy를 만드는 표현을 찾지 못했다. 검사 목록을 갱신한다."
    );
    let early = clones_before_request(&start);
    assert!(
        early.is_empty(),
        "`{START}`가 요청을 꺼내기 전에 proxy를 clone한다: {early:?}. 요청이 없는 idle 폴링마다 메인 루프가 깨어난다."
    );
    for signature in ALWAYS {
        let early = clones_before_request(&body_of(signature));
        assert!(
            early.is_empty(),
            "`{signature}`는 매 폴링마다 실행되는데 proxy를 clone한다: {early:?}"
        );
    }
}

#[test]
fn the_judgment_flags_a_clone_ahead_of_the_request() {
    let defect = "{ let proxy = self.view.proxy.clone(); for v in views { let Some(r) = slot.take_request() else { continue }; spawn_scan_worker(&proxy, r); } }";
    assert_eq!(clones_before_request(defect), vec!["proxy.clone()"]);
    let fixed = "{ for v in views { let Some(r) = slot.take_request() else { continue }; spawn_scan_worker(&self.view.proxy, r); } }";
    assert!(clones_before_request(fixed).is_empty());
    assert_eq!(
        clones_before_request("{ self.start(); worker_wake(&p); }"),
        vec!["worker_wake("]
    );
}
