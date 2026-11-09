//! 훅 이벤트에서 사용량 기록(`telemetry.record`)을 만든다 — 턴 경과 시간과 알림 문구의 토큰 수.

use super::HostCall;
use crate::state::ClaudeState;

/// session-start에서 시각을 기록하고 종료 이벤트에서 경과 시간을 계산한다.
/// notification에 토큰 사용량이 있으면 input_tokens 기록도 만든다.
pub(crate) fn telemetry_for_hook(
    state: &mut ClaudeState,
    event: &str,
    surface_id: u32,
    message: Option<&str>,
    now_ms: u64,
) -> Vec<HostCall> {
    let mut out = Vec::new();
    match event {
        "session-start" => {
            state.mark_session_start(surface_id, now_ms);
        }
        // subagent-stop 은 메인 턴의 끝이 아니므로 경과 시간을 닫지 않는다.
        "stop" | "stop-failure" | "session-end" => {
            if let Some(elapsed) = state.take_wall_time(surface_id, now_ms) {
                out.push(HostCall::TelemetryRecord {
                    metric: "wall_time_ms",
                    value: elapsed as f64,
                    surface_id,
                });
            }
        }
        "notification" => {
            if let Some(text) = message
                && let Some(n) = extract_tokens(text)
            {
                out.push(HostCall::TelemetryRecord {
                    metric: "input_tokens",
                    value: n as f64,
                    surface_id,
                });
            }
        }
        _ => {}
    }
    out
}

/// token/Token에 선택적인 s/S, 콜론, 공백·탭, 숫자가 이어지는 표현을 찾는다.
/// 앞뒤는 ASCII 영숫자가 아니어야 하며 정규식의 Unicode 단어 경계와는 다르다.
fn extract_tokens(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // 시작 위치이거나 앞 바이트가 ASCII 영숫자가 아니어야 한다.
        let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if prev_ok && (bytes[i..].starts_with(b"token") || bytes[i..].starts_with(b"Token")) {
            let mut j = i + 5;
            if j < bytes.len() && (bytes[j] == b's' || bytes[j] == b'S') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b':' {
                j += 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let start = j;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > start {
                    let after_ok = j == bytes.len() || !bytes[j].is_ascii_alphanumeric();
                    if after_ok
                        && let Ok(n) = std::str::from_utf8(&bytes[start..j])
                            .unwrap()
                            .parse::<u64>()
                    {
                        return Some(n);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tokens_variants() {
        assert_eq!(extract_tokens("tokens: 99"), Some(99));
        assert_eq!(extract_tokens("token:5"), Some(5));
        assert_eq!(extract_tokens("Tokens:   1000  used"), Some(1000));
        assert_eq!(extract_tokens("notokens: 5"), None);
        assert_eq!(extract_tokens("tokens 5"), None); // 콜론 없음
        assert_eq!(extract_tokens("xtoken: 5"), None); // 워드 경계 위반
    }
}
