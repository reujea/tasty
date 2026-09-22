//! 웹훅 HTTP 응답 = **단방향 ACK 전용** (CRITICAL 불변식).
//!
//! [`build_ack`] 는 **IpcSequence 실행 결과에 접근하는 인자를 갖지 않는다.** 응답은
//! 고정 상태코드 + 최소 바디뿐이며, 어떤 params/치환 조합에서도 tasty 내부 데이터가
//! 응답으로 샐 수 없다 — "안 담는다" 가 아니라 담을 수 있는 코드 경로 자체를 두지
//! 않는다(응답 빌더 시그니처가 실행 경로와 분리).

use std::io::Cursor;

use tiny_http::Response;

/// 웹훅 응답 상태 — 고정 enum. 인증(401)·만료(410)·body 상한(413)·남용차단(429) 반영.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckStatus {
    /// 200 — 매칭 성공, 핸들러에 전달됨.
    Received,
    /// 401 — 웹훅에 인증이 설정됐으나 토큰 미제시/불일치.
    Unauthorized,
    /// 404 — 등록된 웹훅 path 없음.
    NotFound,
    /// 405 — path 는 있으나 HTTP 메서드 불일치.
    MethodNotAllowed,
    /// 410 — lifetime 만료(시간 초과 / 횟수 소진). 호출 시 lazy 삭제됨.
    Gone,
    /// 413 — body 가 JSON 입력 상한을 넘었다. 응답 뒤 잔여 body 를 읽지 않고 연결을 닫는다.
    PayloadTooLarge,
    /// 429 — 남용 차단 쿨다운 중인 출처(실패 반복 — `abuse::counts_as_failure`).
    /// 매칭 전 즉시 거부.
    TooManyRequests,
}

impl AckStatus {
    fn parts(self) -> (u16, &'static str) {
        match self {
            AckStatus::Received => (200, "received"),
            AckStatus::Unauthorized => (401, "unauthorized"),
            AckStatus::NotFound => (404, "not found"),
            AckStatus::MethodNotAllowed => (405, "method not allowed"),
            AckStatus::Gone => (410, "gone"),
            AckStatus::PayloadTooLarge => (413, "payload too large"),
            AckStatus::TooManyRequests => (429, "too many requests"),
        }
    }
}

/// 단방향 ACK 빌더. **실행 결과 인자를 받지 않는다** — 이 시그니처가 단방향
/// 불변식의 타입 강제선이다. 바디는 고정 문자열뿐.
pub fn build_ack(status: AckStatus) -> Response<Cursor<Vec<u8>>> {
    let (code, body) = status.parts();
    Response::from_string(body).with_status_code(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ack_bodies_are_fixed_strings() {
        // 어떤 상태든 바디는 고정 문자열 — 내부 데이터가 섞일 여지 없음.
        assert_eq!(build_ack(AckStatus::Received).status_code().0, 200);
        assert_eq!(build_ack(AckStatus::Unauthorized).status_code().0, 401);
        assert_eq!(build_ack(AckStatus::NotFound).status_code().0, 404);
        assert_eq!(build_ack(AckStatus::MethodNotAllowed).status_code().0, 405);
        assert_eq!(build_ack(AckStatus::Gone).status_code().0, 410);
        assert_eq!(build_ack(AckStatus::TooManyRequests).status_code().0, 429);
    }
}
