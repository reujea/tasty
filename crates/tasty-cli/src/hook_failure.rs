//! IPC로 전달하지 못한 에이전트 훅 오류를 로컬 파일에 기록한다.
//! 연결 실패를 같은 IPC로 보고할 수 없어 tasty_home 아래에 남긴다.
//! TASTY_PARENT_HOME은 사용하지 않으며 별도 port-file override와 기록 홈은 다를 수 있다.
//! 기록 실패는 훅의 결과에 영향을 주지 않는다.

use std::io::Write;
use std::path::PathBuf;

/// 기록 파일명. 기존 `notify/<surface>.log` 와 같은 성격(저비용 append-only)이다.
const LOG_FILE: &str = "hook-failures.log";

/// 다음 기록 전에 회전할 크기 기준. 한 기록의 크기나 동시 쓰기를 제한하는 상한은 아니다.
const MAX_BYTES: u64 = 256 * 1024;

/// 마지막 메서드 구성요소가 hook 또는 *_hook인 호출만 기록한다.
pub fn is_hook_method(method: &str) -> bool {
    let tail = method.rsplit('.').next().unwrap_or(method);
    tail == "hook" || tail.ends_with("_hook")
}

fn log_path() -> Option<PathBuf> {
    tasty_utils::path::tasty_home().map(|d| d.join(LOG_FILE))
}

/// UTC ISO-8601(초 단위). 외부 크레이트 없이 `SystemTime` 에서 직접 만든다 —
/// 이 크레이트에 시간 포맷팅 의존성을 새로 들이지 않기 위함.
fn utc_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    utc_from_secs(secs)
}

/// 유닉스 초를 `YYYY-MM-DDTHH:MM:SSZ` 로 쓴다.
pub(crate) fn utc_from_secs(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// days-since-epoch → (year, month, day). Howard Hinnant 의 `civil_from_days`
/// 알고리즘(public domain) — 윤년/윤세기를 분기 없이 처리한다.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// params.event를 로그 필드로 만든다. 부재는 -, 공백은 _로 표시한다.
fn event_token(params: &serde_json::Value) -> String {
    let raw = params.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let folded: String = raw
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect();
    if folded.is_empty() {
        "-".into()
    } else {
        folded
    }
}

/// CLI가 만든 진단은 로케일과 무관한 영어로 기록한다.
/// 호스트·플러그인이 보낸 오류 문구는 번역될 수 있으므로 code 필드로 분류한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEnglish(String);

impl DiagnosticEnglish {
    /// CLI의 t/t_fmt 결과를 넘기지 않는다. 영문 진단 또는 외부 오류 응답에만 사용한다.
    /// 외부 응답의 언어는 통제하지 못하므로 오류 코드도 함께 기록한다.
    pub fn new_unchecked(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DiagnosticEnglish {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// key=value 한 줄 기록. 부재한 event/code는 -, reason의 개행은 공백으로 바꾼다.
/// reason만 공백을 포함할 수 있으므로 반드시 마지막 필드로 유지한다.
fn format_line(
    method: &str,
    params: &serde_json::Value,
    code: Option<i32>,
    reason: &str,
) -> String {
    let surface = std::env::var("TASTY_SURFACE_ID").unwrap_or_else(|_| "-".into());
    let event = event_token(params);
    let code = code.map_or_else(|| "-".to_string(), |c| c.to_string());
    // 줄 단위 레코드이므로 개행은 공백으로 접는다(한 실패 = 한 줄 불변식).
    let reason = reason.replace(['\n', '\r'], " ");
    format!(
        "{ts} method={method} event={event} surface={surface} code={code} reason={reason}\n",
        ts = utc_timestamp()
    )
}

/// 훅 메서드의 실패를 기록한다. CLI 진단은 영어, 사용자 stderr는 별도로 번역한다.
/// 기록 실패는 무시한다.
pub fn record(
    method: &str,
    params: &serde_json::Value,
    code: Option<i32>,
    reason: &DiagnosticEnglish,
) {
    let Some(path) = log_path() else { return };
    record_at(&path, method, params, code, reason.as_str());
}

/// 경로를 주입받는 실제 구현 — 테스트가 프로세스 전역 env(`TASTY_HOME`)를 건드리지
/// 않고 검증할 수 있게 분리했다(같은 프로세스에서 병렬로 도는 다른 테스트와의 경쟁 방지).
fn record_at(
    path: &std::path::Path,
    method: &str,
    params: &serde_json::Value,
    code: Option<i32>,
    reason: &str,
) {
    if !is_hook_method(method) {
        return;
    }
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    rotate_if_needed(path);
    // append-only. 열기/쓰기 실패는 그대로 포기한다 — 진단 기록이 hook 을 깨뜨리면 안 된다.
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "Best-effort diagnostics or cleanup must not replace the original failure."
        )]
        let _ = f.write_all(format_line(method, params, code, reason).as_bytes()); // best-effort
    }
}

/// 임계치를 넘었으면 `<name>.1` 로 밀어낸다. rename 실패는 무시 — 다음 기록이 그냥
/// 커진 파일에 이어 붙을 뿐이라 관측 가능성 자체는 유지된다.
fn rotate_if_needed(path: &std::path::Path) {
    let too_big = std::fs::metadata(path)
        .map(|m| m.len() >= MAX_BYTES)
        .unwrap_or(false);
    if too_big {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "Best-effort diagnostics or cleanup must not replace the original failure."
        )]
        let _ = std::fs::rename(path, path.with_extension("log.1")); // best-effort
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_methods_are_recognized() {
        assert!(is_hook_method("claude.hook"));
        assert!(is_hook_method("codex.hook"));
        assert!(is_hook_method("claude.checklist_hook"));
    }

    #[test]
    fn non_hook_methods_are_ignored() {
        assert!(!is_hook_method("claude.children"));
        assert!(!is_hook_method("terminal.spawn"));
        assert!(!is_hook_method("claude.install"));
        // `hook` 이 앞쪽에 있을 뿐인 method 도 대상이 아니다.
        assert!(!is_hook_method("hook.set"));
        assert!(!is_hook_method("claude.hook_status"));
    }

    #[test]
    fn line_is_single_line_and_greppable() {
        let line = format_line(
            "claude.hook",
            &serde_json::json!({ "event": "stop" }),
            Some(-32602),
            "boom\nsecond line",
        );
        assert_eq!(line.matches('\n').count(), 1, "한 실패 = 한 줄");
        assert!(line.ends_with('\n'));
        assert!(line.contains("method=claude.hook"));
        assert!(line.contains("reason=boom second line"));
    }

    #[test]
    fn event_token_is_carried_from_params() {
        for event in ["stop", "notification", "session-end", "subagent-stop"] {
            let line = format_line(
                "claude.hook",
                &serde_json::json!({ "event": event }),
                None,
                "could not connect",
            );
            assert!(line.contains(&format!("event={event}")), "{line}");
        }
    }

    #[test]
    fn missing_event_becomes_dash() {
        for params in [
            serde_json::json!({}),
            serde_json::json!({ "session_id": "abc" }),
            serde_json::json!({ "event": "" }),
            serde_json::json!({ "event": 3 }),
            serde_json::Value::Null,
        ] {
            let line = format_line("claude.checklist_hook", &params, None, "no port file");
            assert!(line.contains("event=-"), "{line}");
        }
    }

    /// `event` 는 줄 중간 필드라 공백이 섞이면 `key=value` 나열이 깨진다.
    #[test]
    fn event_whitespace_is_folded() {
        let line = format_line(
            "claude.hook",
            &serde_json::json!({ "event": "we ird\nname" }),
            None,
            "boom",
        );
        assert_eq!(line.matches('\n').count(), 1, "한 실패 = 한 줄");
        assert!(line.contains("event=we_ird_name"), "{line}");
        let fields: Vec<&str> = line.trim_end().split(' ').collect();
        assert_eq!(fields[1], "method=claude.hook");
        assert_eq!(fields[2], "event=we_ird_name");
        assert!(fields[3].starts_with("surface="));
    }

    /// epoch/알려진 날짜로 달력 변환을 고정한다(윤년 포함).
    #[test]
    fn timestamp_calendar_conversion() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(59), (1970, 3, 1));
        // 2024-02-29 (윤년) = 19782 days since epoch.
        assert_eq!(civil_from_days(19782), (2024, 2, 29));
    }

    /// 기록 → 파일에 한 줄 append, hook 이 아닌 method 는 무시, 임계치 초과 시 로테이션.
    #[test]
    fn record_appends_and_rotates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(LOG_FILE);

        let stop = serde_json::json!({ "event": "stop" });
        record_at(&path, "claude.hook", &stop, None, "could not connect");
        record_at(&path, "claude.children", &stop, None, "ignored");
        let body = std::fs::read_to_string(&path).expect("log written");
        assert_eq!(body.lines().count(), 1, "hook 실패만 기록된다");
        assert!(body.contains("method=claude.hook"));
        assert!(body.contains("event=stop"), "{body}");

        std::fs::write(&path, vec![b'x'; MAX_BYTES as usize]).expect("grow");
        record_at(
            &path,
            "codex.hook",
            &serde_json::json!({ "event": "session-end" }),
            None,
            "timed out",
        );
        assert!(dir.path().join("hook-failures.log.1").exists(), "로테이션");
        let body = std::fs::read_to_string(&path).expect("new log");
        assert_eq!(body.lines().count(), 1, "새 파일은 새 줄만");
    }

    #[test]
    fn the_code_is_a_field_not_something_to_parse_out_of_the_prose() {
        let line = format_line(
            "codex.hook",
            &serde_json::json!({ "event": "stop" }),
            Some(-32602),
            "invalid params: 알 수 없는 hook 이벤트",
        );
        assert!(line.contains(" code=-32602 "), "{line}");

        let fields: Vec<&str> = line.trim_end().split(' ').take(5).collect();
        assert_eq!(fields[1], "method=codex.hook");
        assert_eq!(fields[2], "event=stop");
        assert_eq!(fields[4], "code=-32602");

        let at = line.find("reason=").expect("reason 필드");
        assert!(
            !line[at..].trim_end().contains(' ') || line[at..].starts_with("reason="),
            "reason 이 마지막이 아니다: {line}"
        );
        assert!(
            line.trim_end().ends_with("알 수 없는 hook 이벤트"),
            "{line}"
        );
    }

    #[test]
    fn a_failure_without_a_code_keeps_the_column() {
        let line = format_line(
            "claude.hook",
            &serde_json::json!({ "event": "stop" }),
            None,
            "no port file",
        );
        assert!(line.contains(" code=- "), "{line}");
    }

    #[test]
    fn record_creates_missing_parent_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join(LOG_FILE);
        record_at(
            &path,
            "claude.hook",
            &serde_json::json!({ "event": "session-start" }),
            None,
            "no port file",
        );
        assert!(path.exists(), "부모 디렉터리를 만들어 기록해야 한다");
    }
}
