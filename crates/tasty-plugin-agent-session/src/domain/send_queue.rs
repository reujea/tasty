//! 턴이 도는 동안 보낸 말을 **탭이 들고 있다가**, 턴이 끝나면 한 메시지로 합쳐 보낸다.
//!
//! # 왜 탭이 드나
//!
//! 종전에는 보낼 때마다 곧장 `session.send_input` 을 불렀다. 턴이 도는 세션에 하나를 더
//! 보내면 데몬이 전역 잠금을 쥔 채 앞 턴을 기다려, **데몬 전체**가 다른 요청에 답하지
//! 못했다(실측 2026-10-12: 잠금 보유 1305초). 세션에 요청이 겹치지 않게 하면 그 입구가
//! 닫힌다.
//!
//! 합쳐 보내는 모양은 Claude Code 의 동작을 따른다. 텍스트만 쓰는 턴 동안 쌓인 입력은
//! 그 턴이 끝난 뒤 **다음 턴 하나**로 함께 들어간다(실측 2026-10-13, 2.1.287 · TUI 와
//! stream-json 이 같았다). 도는 턴에 끼워 넣는 쪽(도구 턴에서의 동작)은 데몬이 잠금 없이
//! 입력을 쓰고 합쳐진 턴을 알려 줘야 해서 여기서 하지 않는다.
//!
//! # 화면의 짝
//!
//! 페이지는 제출한 말을 곧바로 대기 칸에 `staged` 로 올린다(번호 `n` 을 단다). 보낼지
//! 쌓을지는 plugin 이 정하고 [`settle_script`] 로 그 자리를 고친다 — 쌓으면 확정 항목이
//! 되고, 보내면 대기 칸에서 빠져 대화 끝의 진행 중 턴이 된다. 페이지가 판정을 흉내 내지
//! 않으므로 둘이 어긋날 수 없다.
//!
//! # 파일
//!
//! 쌓인 말은 바뀔 때마다 `<데이터 폴더>/send-queue/<세션 id>.json` 에 쓰고, 꺼내 보내면 지운다
//! ([`push`] · [`take_batch`]). 레이아웃 스냅샷은 호스트가 저장할 때만 기록되므로 그 사이 plugin 이나
//! 앱이 죽으면 쌓인 말이 사라진다. 탭을 되살릴 때는 파일이 스냅샷보다 앞선다([`restore`]).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::convo::Convo;

/// 쌓인 말을 이을 때의 구분 — 빈 줄 하나.
const JOIN: &str = "\n\n";

/// 지금 보낼 말을 돌려준다 — 세션이 바쁘면 쌓고 `None`.
///
/// 이미 쌓인 것이 있으면 바쁘지 않아도 **그 뒤에 쌓고 모두 함께** 보낸다 — 먼저 친 말이
/// 나중 말보다 늦게 가면 안 된다.
pub fn take_or_queue(convo: &mut Convo, text: &str, busy: bool) -> Option<String> {
    if busy {
        push(convo, text);
        return None;
    }
    if convo.queued.is_empty() {
        return Some(text.to_string());
    }
    push(convo, text);
    take_batch(convo)
}

/// 말 하나를 쌓고 파일에 쓴다 — 대기열에 넣는 자리는 모두 이 길로 온다.
pub fn push(convo: &mut Convo, text: &str) {
    convo.queued.push(text.to_string());
    save(convo);
}

/// 쌓인 말을 한 메시지로 꺼낸다 — 비었으면 `None`. 꺼낸 말은 파일에서도 지운다(소비).
pub fn take_batch(convo: &mut Convo) -> Option<String> {
    if convo.queued.is_empty() {
        return None;
    }
    let batch = std::mem::take(&mut convo.queued).join(JOIN);
    save(convo);
    Some(batch)
}

/// 세션의 대기열 파일 — `<데이터 폴더>/send-queue/<세션 id>.json`. 경로 문자가 섞인 id 는 쓰지 않는다.
pub fn queue_path(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    crate::adapters::driven::store::turn_archive::archive_path(data_dir, session_id).map(|p| {
        data_dir
            .join("send-queue")
            .join(p.with_extension("json").file_name().unwrap_or_default())
    })
}

/// 지금 대기열을 파일에 옮긴다 — 비었으면 지운다. 세션 id · 데이터 폴더가 없으면 메모리에만 둔다.
fn save(convo: &Convo) {
    if let Some(path) = convo
        .session_id
        .as_deref()
        .zip(crate::adapters::driven::store::turn_archive::data_dir())
        .and_then(|(s, d)| queue_path(&d, s))
        && let Err(e) = write_queue(&path, &convo.queued)
    {
        tracing::warn!(
            "[agent_session] 대기열 파일을 못 썼다({}): {e}",
            path.display()
        );
    }
}

/// 대기열을 파일에 쓴다 — 임시 파일에 쓰고 바꿔 단다(쓰다 죽어도 반쪽 파일이 남지 않는다). 비었으면 지운다.
pub fn write_queue(path: &Path, queued: &[String]) -> std::io::Result<()> {
    if queued.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec(queued).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(&tmp, path)
}

/// 대기열 파일을 읽는다 — 없으면 `None`, 모양이 틀리면 빈 대기열.
pub fn read_queue(path: &Path) -> Option<Vec<String>> {
    let bytes = std::fs::read(path).ok()?;
    Some(
        serde_json::from_slice::<Vec<String>>(&bytes).unwrap_or_else(|e| {
            tracing::warn!(
                "[agent_session] 대기열 파일을 못 읽었다({}): {e}",
                path.display()
            );
            Vec::new()
        }),
    )
}

/// 탭을 되살릴 때의 대기열 — 파일이 있으면 파일(마지막으로 바뀐 값), 없으면 스냅샷.
pub fn restore(session_id: Option<&str>, snap: &Value) -> Vec<String> {
    session_id
        .zip(crate::adapters::driven::store::turn_archive::data_dir())
        .and_then(|(s, d)| queue_path(&d, s))
        .and_then(|p| read_queue(&p))
        .unwrap_or_else(|| queued_from_snapshot(snap))
}

/// 세션의 대기열 파일을 지운다 — 탭을 닫을 때. 다시 열면 스냅샷이 되살린다.
pub fn remove_for(session_id: &str) {
    if let Some(path) = crate::adapters::driven::store::turn_archive::data_dir()
        .and_then(|d| queue_path(&d, session_id))
        && let Err(e) = write_queue(&path, &[])
    {
        tracing::warn!(
            "[agent_session] 대기열 파일을 못 지웠다({}): {e}",
            path.display()
        );
    }
}

/// 제출 URL 의 번호(`agent-send=<n>`) — 페이지가 대기 칸에 단 것과 같은 값이다.
pub fn parse_nonce(url: &str) -> Option<u64> {
    let q = crate::adapters::driving::url::query_of(url)?;
    q.split('&').find_map(|kv| {
        let rest = kv.strip_prefix(crate::adapters::driving::url::FRAG_MARK)?;
        let v = rest
            .strip_prefix('=')
            .or_else(|| rest.strip_prefix("%3D"))
            .or_else(|| rest.strip_prefix("%3d"))?;
        v.parse().ok()
    })
}

/// 대기 칸 — 문서를 다시 실어도 쌓인 말이 보이게 서버가 같은 모양을 낸다.
pub fn queue_html(queued: &[String]) -> String {
    let items: String = queued
        .iter()
        .map(|q| {
            format!(
                r#"<div class="q">{}</div>"#,
                crate::adapters::driving::html::escape(q)
            )
        })
        .collect();
    format!(
        r#"<div id="queue"{hidden}><div class="qh">{label}</div>{items}</div>"#,
        hidden = if queued.is_empty() { " hidden" } else { "" },
        label = crate::adapters::driving::html::escape(&crate::domain::i18n::t(
            "sup.chat.queued_label"
        )),
    )
}

/// 판정을 화면에 옮기는 스크립트.
///
/// - `staged`: 이번 판정의 대상인 페이지 쪽 임시 항목 번호(없으면 plugin 이 스스로 보낸 것).
/// - `queued`: 판정 뒤 쌓여 있는 말 — 확정 항목을 이것으로 다시 그린다. 아직 판정을 못 받은
///   다른 임시 항목은 건드리지 않는다(그 판정은 뒤따라 온다).
/// - `sent`: 지금 보낸 말 — 대화 끝에 진행 중 턴으로 붙인다.
///
/// 텍스트는 `textContent` 로 넣는다 — `innerHTML` 이면 사용자 입력의 태그가 실행된다.
pub fn settle_msg(staged: Option<u64>, queued: &[String], sent: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "type": "queue.settle",
        "staged": staged,
        "queued": queued,
        "sent": sent,
        "thinking": crate::domain::i18n::t("sup.chat.thinking") + "…",
    })
}

/// 스냅샷에 쌓인 말을 싣는다 — 재시작으로 사용자가 친 말이 사라지면 안 된다.
pub fn with_queued(mut snap: Value, queued: &[String]) -> Value {
    if !queued.is_empty()
        && let Some(o) = snap.as_object_mut()
    {
        o.insert("queued".to_string(), serde_json::json!(queued));
    }
    snap
}

/// 스냅샷에서 쌓인 말을 되읽는다 — 모양이 틀리면 비운다.
pub fn queued_from_snapshot(snap: &Value) -> Vec<String> {
    snap.get("queued")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_session_queues_and_an_idle_one_sends() {
        let mut c = Convo::default();
        assert_eq!(take_or_queue(&mut c, "a", false).as_deref(), Some("a"));
        assert_eq!(take_or_queue(&mut c, "b", true), None);
        assert_eq!(take_or_queue(&mut c, "c", true), None);
        assert_eq!(c.queued, vec!["b", "c"]);
        assert_eq!(take_batch(&mut c).as_deref(), Some("b\n\nc"));
        assert!(c.queued.is_empty());
        assert_eq!(take_batch(&mut c), None);
    }

    /// 쌓인 것이 있으면 한가해도 새 말을 앞질러 보내지 않는다 — 친 순서대로 함께 간다.
    #[test]
    fn a_new_message_does_not_overtake_the_queue() {
        let mut c = Convo::default();
        take_or_queue(&mut c, "first", true);
        assert_eq!(
            take_or_queue(&mut c, "second", false).as_deref(),
            Some("first\n\nsecond")
        );
        assert!(c.queued.is_empty());
    }

    #[test]
    fn the_nonce_is_read_from_the_submit_url() {
        assert_eq!(parse_nonce("about:blank%23agent-send=7&t=PING"), Some(7));
        assert_eq!(parse_nonce("about:blank?t=x&agent-send=12%23"), Some(12));
        assert_eq!(parse_nonce("about:blank#other=1"), None);
    }

    #[test]
    fn queued_messages_survive_a_snapshot() {
        let q = vec!["a".to_string(), "b".to_string()];
        let snap = with_queued(serde_json::json!({ "session_id": "s" }), &q);
        assert_eq!(queued_from_snapshot(&snap), q);
        let empty = with_queued(serde_json::json!({}), &[]);
        assert!(empty.get("queued").is_none(), "빈 대기열이 칸을 만든다");
        assert!(queued_from_snapshot(&serde_json::json!({ "queued": 3 })).is_empty());
    }

    /// 사용자 텍스트는 데이터로 가고 처리기가 `textContent` 로 넣는다 — 원문 그대로 싣는다.
    #[test]
    fn user_text_cannot_break_out_of_the_script() {
        let s = settle_msg(Some(1), &["</script><b>x".into()], Some("</script>"));
        assert_eq!(s["queued"], serde_json::json!(["</script><b>x"]));
        assert_eq!(s["sent"], "</script>");
        assert_eq!(s["staged"], 1);
        let page = crate::adapters::driving::messages::script_html();
        assert!(page.contains("d.textContent=v"));
        // 보낸 턴은 앱이 그린다 — 글은 자식 텍스트라 Preact 가 이스케이프한다.
        assert!(include_str!("../../assets/app.js").contains("${auto ? L.auto : t.sent}</div>"));
    }

    /// 쌓으면 파일이 생기고, 꺼내면 지워지고, 되살리면 파일이 스냅샷을 이긴다.
    #[test]
    fn the_queue_file_tracks_pushes_and_consumption() {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tasty-send-queue-{}-{n}", std::process::id()));
        let path = queue_path(&dir, "sess-1").expect("안전한 id");
        assert!(path.ends_with("send-queue/sess-1.json"));
        assert!(queue_path(&dir, "../x").is_none(), "경로 문자를 받는다");
        write_queue(&path, &["a".into(), "b\n\"q\"".into()]).expect("쓰기");
        assert_eq!(
            read_queue(&path),
            Some(vec!["a".to_string(), "b\n\"q\"".to_string()])
        );
        assert!(
            !path.with_extension("json.tmp").exists(),
            "임시 파일이 남는다"
        );
        write_queue(&path, &[]).expect("지우기");
        assert!(!path.exists(), "비운 대기열 파일이 남는다");
        assert_eq!(read_queue(&path), None);
        write_queue(&path, &[]).expect("없는 파일을 지워도 오류가 아니다");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{").unwrap();
        assert_eq!(read_queue(&path), Some(Vec::new()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 대기열에 넣는 자리는 모두 파일에 쓰는 길을 지난다 — 직접 `queued.push` 하면 파일이 낡는다.
    #[test]
    fn every_enqueue_goes_through_push() {
        for (name, src) in [
            ("main.rs", crate::test_support::wiring_source()),
            ("start.rs", include_str!("../application/start.rs")),
            (
                "retry_resend.rs",
                include_str!("../application/retry_resend.rs"),
            ),
            ("clear.rs", include_str!("../application/clear.rs")),
            ("add_dir.rs", include_str!("../application/add_dir.rs")),
        ] {
            let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
            for bad in [
                "queued.push(",
                "queued.extend(",
                "queued.clear(",
                ".queued = ",
            ] {
                assert!(!prod.contains(bad), "{name} 가 대기열을 직접 고친다: {bad}");
            }
        }
        assert!(crate::test_support::wiring_source().contains("send_queue::restore("));
        assert!(crate::test_support::wiring_source().contains("send_queue::remove_for(&id)"));
    }

    #[test]
    fn the_queue_box_hides_when_empty() {
        assert!(queue_html(&[]).contains(" hidden>"));
        let h = queue_html(&["<i>x</i>".into()]);
        assert!(!h.contains(" hidden>") && h.contains("&lt;i&gt;"));
    }
}
