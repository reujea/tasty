//! 메모리 절약 — 가려진 대화 탭의 WebView 를 내리고, 화면에만 있던 것은 디스크에 둔다.
//!
//! 설정 [`SAVER_KEY`] 를 켜면 호스트가 가려진 대화 탭의 WebView 를 없앤다(렌더링 프로세스도 내려간다).
//! 다시 보이면 새로 만들고 `surface.webview_created` 를 보내므로 문서를 다시 싣는다(`lazy_webview`).
//! 대화 · 대기열 · 상태는 plugin 이 쥐고 있어 잃지 않는다. 페이지에만 있던 **입력칸에 쓰던 글**은 페이지가
//! 바뀔 때마다(300ms 묶음) 보내고 plugin 이 `<데이터 폴더>/view-state/<세션 id>.json` 에 둔다 — 다시 실은
//! 문서가 그 글로 입력칸을 채운다. 스크롤은 다시 실으면 바닥에서 시작한다(종전과 같다).
//!
//! 끄면 WebView 를 한 번 만든 뒤 계속 둔다(종전 동작). 다시 실을 때 드는 시간은 대화 길이를 따르므로 긴
//! 대화는 "메모리 유지 턴 수"(`turn_archive`) 로 줄인다.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::domain::convo::Convo;
use crate::ports::host::SendHost;

/// 설정 키 — 호스트도 같은 키를 읽어 WebView 를 내린다(`webview_kind::RELEASE_HIDDEN_KEY`).
pub const SAVER_KEY: &str = "release_hidden_webview";

/// 페이지가 쓰던 글을 보내는 표식 — `agent-view=<n>&d=<글>`.
pub const VIEW_MARK: &str = "agent-view";

/// 페이지에만 있던 상태 중 다시 실을 때 되살리는 것. 쓰던 글은 파일에도 둔다.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ViewState {
    /// 입력칸에 쓰던 글 — 보내면 비운다.
    pub draft: String,
    /// 메모리 절약이 켜졌나 — 켜졌을 때만 페이지가 쓰던 글을 보낸다.
    pub save: bool,
}

/// 쓰던 글 표식이면 그 글.
pub fn parse(url: &str) -> Option<String> {
    let q = crate::adapters::driving::url::query_of(url)?;
    let mut fields = q.split('&');
    let head = fields.next()?;
    head.strip_prefix(VIEW_MARK)?.strip_prefix('=')?;
    Some(
        fields
            .find_map(|kv| kv.strip_prefix("d="))
            .map(crate::adapters::driving::url::percent_decode)
            .unwrap_or_default(),
    )
}

/// 세션의 화면 상태 파일 — `<데이터 폴더>/view-state/<세션 id>.json`. 경로 문자가 섞인 id 는 쓰지 않는다.
pub fn state_path(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    crate::domain::send_queue::queue_path(data_dir, session_id).and_then(|p| {
        p.file_name()
            .map(|name| data_dir.join("view-state").join(name))
    })
}

fn path_of(convo: &Convo) -> Option<PathBuf> {
    convo
        .session_id
        .as_deref()
        .zip(crate::adapters::driven::store::turn_archive::data_dir())
        .and_then(|(s, d)| state_path(&d, s))
}

/// 쓰던 글을 받아 둔다 — 비었으면 파일을 지운다.
pub fn keep(convo: &mut Convo, draft: String) {
    if convo.view.draft == draft {
        return;
    }
    convo.view.draft = draft;
    if let Some(path) = path_of(convo)
        && let Err(e) = write_state(&path, &convo.view.draft)
    {
        tracing::warn!(
            "[agent_session] 화면 상태 파일을 못 썼다({}): {e}",
            path.display()
        );
    }
}

/// 화면 상태를 파일에 쓴다 — 임시 파일을 거쳐 바꿔 단다. 쓰던 글이 비었으면 지운다.
pub fn write_state(path: &Path, draft: &str) -> std::io::Result<()> {
    if draft.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json!({ "draft": draft }).to_string())?;
    std::fs::rename(&tmp, path)
}

/// 화면 상태 파일을 읽는다 — 없거나 모양이 틀리면 빈 글.
pub fn read_draft(path: &Path) -> String {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.get("draft").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

/// 탭을 되살릴 때 쓰던 글 — 파일에서.
pub fn restore(session_id: Option<&str>) -> String {
    session_id
        .zip(crate::adapters::driven::store::turn_archive::data_dir())
        .and_then(|(s, d)| state_path(&d, s))
        .map(|p| read_draft(&p))
        .unwrap_or_default()
}

/// 세션의 화면 상태 파일을 지운다 — 탭을 닫을 때.
pub fn remove_for(session_id: &str) {
    if let Some(path) = crate::adapters::driven::store::turn_archive::data_dir()
        .and_then(|d| state_path(&d, session_id))
        && let Err(e) = write_state(&path, "")
    {
        tracing::warn!(
            "[agent_session] 화면 상태 파일을 못 지웠다({}): {e}",
            path.display()
        );
    }
}

/// 열린 문서에 메모리 절약이 바뀌었음을 알린다 — 다시 싣지 않고 페이지의 보내기만 켜고 끈다.
pub fn save_msg(on: bool) -> Value {
    json!({ "type": "view.save", "on": on })
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 페이지 이동 하나를 본다 — 쓰던 글 표식이면 받아 두고 `true`(다른 처리기에 넘기지 않는다). 보내기
    /// 표식이면 입력칸이 비었으므로 쓰던 글도 비운다 — 안 비우면 다시 실은 문서가 보낸 말을 되살린다.
    pub(crate) fn take_view_nav(&mut self, sid: u32, url: &str) -> bool {
        let Some(c) = self.convos.get_mut(&sid) else {
            return parse(url).is_some();
        };
        match parse(url) {
            Some(draft) => {
                c.sync.touch(std::time::Instant::now());
                keep(c, draft);
                true
            }
            None => {
                if crate::domain::send_queue::parse_nonce(url).is_some() {
                    keep(c, String::new());
                }
                false
            }
        }
    }

    /// 설정을 탭마다 싣고, 바뀐 탭의 페이지에 알린다(`flush_queues`).
    pub(crate) fn note_view_saver(&mut self, host: &impl SendHost, on: bool) {
        for (&sid, c) in self.convos.iter_mut() {
            if c.view.save != on {
                c.view.save = on;
                crate::application::session_surface::patch(host, sid, &save_msg(on));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draft_mark_is_parsed() {
        assert_eq!(
            parse("about:blank#agent-view=3&d=a%20b%0Ac").as_deref(),
            Some("a b\nc")
        );
        // 페이지의 encodeURIComponent 모양 — 글 속 `#` · `?` · `&` 가 표식을 자르지 않는다.
        assert_eq!(
            parse("about:blank#agent-view=2&d=ab%20%23%3F%26").as_deref(),
            Some("ab #?&")
        );
        assert_eq!(parse("about:blank#agent-view=4&d=").as_deref(), Some(""));
        assert_eq!(parse("about:blank#agent-view=5").as_deref(), Some(""));
        assert_eq!(parse("about:blank#agent-send=1&t=x"), None);
        assert_eq!(parse("about:blank#agent-viewer=1&d=x"), None);
    }

    #[test]
    fn the_state_file_round_trips_and_an_empty_draft_removes_it() {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tasty-view-state-{}-{n}", std::process::id()));
        let path = state_path(&dir, "sess-1").expect("안전한 id");
        assert!(path.ends_with("view-state/sess-1.json"));
        assert!(state_path(&dir, "../x").is_none());
        write_state(&path, "쓰던 \"글\"\n").expect("쓰기");
        assert_eq!(read_draft(&path), "쓰던 \"글\"\n");
        write_state(&path, "").expect("지우기");
        assert!(!path.exists());
        assert_eq!(read_draft(&path), "");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"[").unwrap();
        assert_eq!(read_draft(&path), "", "깨진 파일");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 쓰던 글 표식은 navigation 로그보다 먼저 거둔다 — 사용자가 치는 글이 300ms 마다 warn 로그에 남으면 안 된다.
    /// 보내기는 쓰던 글을 비운다 — 안 비우면 다시 실은 문서가 보낸 말을 입력칸에 되살린다.
    #[test]
    fn the_navigation_handler_takes_drafts_before_logging_and_clears_them_on_send() {
        let src = crate::test_support::wiring_source();
        let nav = src
            .split("fn navigate(")
            .nth(1)
            .and_then(|s| s.split("\n    fn ").next())
            .expect("navigation 핸들러가 없다");
        let take = nav
            .find("self.take_view_nav(surface_id, url)")
            .expect("쓰던 글을 안 받는다");
        let log = nav.find("navigation url = ").expect("로그가 없다");
        assert!(take < log, "쓰던 글이 로그에 먼저 찍힌다");
        let body = src.split("fn flush_queues(").nth(1).unwrap();
        assert!(body.contains("self.note_view_saver(&host, knobs.memory_saver);"));
        assert!(src.contains("view_state::remove_for(&id)"));
        assert!(src.contains("view_state::restore("));
        // 첫 문서가 지금 설정으로 그려진다 — 메시지에만 기대면 처리기보다 먼저 닿은 메시지가 사라진다.
        assert!(src.contains("save: self.knobs.get(&h, std::time::Instant::now()).memory_saver,"));
    }

    /// 보내기는 쓰던 글을 비우고, 쓰던 글 표식만 거둔다(다른 표식은 다음 처리기로 간다).
    #[test]
    fn sending_clears_the_draft_and_only_draft_marks_are_taken() {
        let mut p = crate::test_support::TestPlugin::default();
        p.convos.insert(7, Convo::default());
        assert!(p.take_view_nav(7, "about:blank#agent-view=1&d=hi"));
        assert_eq!(p.convos[&7].view.draft, "hi");
        assert!(
            p.convos[&7].sync.last_activity.is_some(),
            "치는 것도 손댄 것이다"
        );
        assert!(!p.take_view_nav(7, "about:blank#agent-send=2&t=hi"));
        assert_eq!(p.convos[&7].view.draft, "", "보낸 말이 남는다");
        assert!(!p.take_view_nav(7, "about:blank#agent-stop=1"));
        assert!(
            p.take_view_nav(99, "about:blank#agent-view=1&d=x"),
            "모르는 탭의 글도 로그로 안 간다"
        );
    }

    /// 매니페스트 기본값이 코드 기본값(꺼짐)과 같고, 키가 호스트가 읽는 키와 같다.
    #[test]
    fn the_manifest_toggle_matches_the_code() {
        let m = include_str!("../../tasty-plugin.toml");
        let at = m
            .find(&format!("storage_key = \"{SAVER_KEY}\""))
            .expect("토글이 없다");
        let item = &m[at..at + m[at..].find("\n\n").unwrap_or(m.len() - at)];
        assert!(item.contains("default = false"), "{item}");
        assert!(
            m.contains("lazy_webview = true"),
            "lazy 가 아니면 호스트가 안 내린다"
        );
    }
}
