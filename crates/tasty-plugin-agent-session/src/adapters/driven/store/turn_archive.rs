//! 대화 탭의 **턴 보관 한도와 디스크 스왑** — 터미널 scrollback 의 `scrollback_lines` +
//! `scrollback_disk_swap` 과 같은 축이다.
//!
//! # 왜 있나
//!
//! 대화 탭의 턴은 상한 없이 세 자리에 쌓였다 — plugin 메모리(`Convo.turns`), 레이아웃
//! 스냅샷(저장할 때마다 대화 전체), webview 문서(다시 그릴 때마다 전체 HTML). 오래 쓰는
//! 탭일수록 셋이 함께 자란다.
//!
//! 설정 두 개(plugin 설정 페이지)로 정한다:
//! - `memory_turns` — 메모리·화면에 둘 턴 수. **0 이면 무제한**(이 모듈이 생기기 전 동작).
//! - `disk_swap` — 넘친 턴을 버리지 않고 `TASTY_PLUGIN_DATA_DIR` 의 세션별 파일로 옮긴다.
//!   꺼져 있으면 넘친 턴은 버린다(터미널 scrollback 과 같은 규칙).
//!
//! # 언제 자르나
//!
//! **도는 턴이 없을 때만.** 진행 중인 턴은 자리(index)로 추적된다(`begin_turn` 이 돌려준
//! 값을 워커가 들고 있다). 앞을 잘라 index 가 밀리면 답이 엉뚱한 턴에 붙는다. 그래서
//! 대기 중인 턴이나 상한을 넘긴 턴(`overdue`)이 있으면 다음 기회로 미룬다.
//!
//! 스왑을 켰는데 데이터 폴더가 없으면 **자르지 않는다** — 옮길 곳 없이 자르면 사용자가
//! 고른 "버리지 않는다" 를 어기게 된다.
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::domain::convo::Convo;
use crate::domain::convo::Turn;
use crate::ports::host::HostCall;

/// plugin 설정의 저장 키 — 매니페스트 `[[contributes.settings_pages.items]]` 의 `storage_key`.
pub const KEEP_KEY: &str = "memory_turns";
pub const SWAP_KEY: &str = "disk_swap";

/// "이전 대화" 를 한 번에 몇 개씩 펼치나.
pub const OLDER_PAGE: usize = 50;

/// 지금 적용할 한도.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limits {
    /// 메모리·화면에 둘 턴 수 — 0 이면 무제한.
    pub keep: usize,
    pub swap: bool,
}

/// 설정을 읽는다 — 못 읽거나 값이 없으면 기본값(무제한 · 스왑 꺼짐). 기본값은 매니페스트의
/// `default` 와 같아야 한다(호스트는 사용자가 한 번도 안 건드린 값을 `null` 로 돌려준다).
pub fn read_limits<H: HostCall>(host: &H) -> Limits {
    let get = |key: &str| {
        host.call("settings.get_plugin_setting", json!({ "storage_key": key }))
            .ok()
            .and_then(|v| v.get("value").cloned())
            .unwrap_or(Value::Null)
    };
    let keep = get(KEEP_KEY)
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map_or(0, |n| n as usize);
    let swap = get(SWAP_KEY).as_bool().unwrap_or(false);
    Limits { keep, swap }
}

/// 세션의 보관 파일 — `<데이터 폴더>/turn-archive/<세션 id>.jsonl`. 세션 id 에 경로 문자가
/// 섞이면 보관하지 않는다(데이터 폴더 밖에 쓰지 않는다).
pub fn archive_path(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    let safe = !session_id.is_empty()
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    safe.then(|| {
        data_dir
            .join("turn-archive")
            .join(format!("{session_id}.jsonl"))
    })
}

/// 세션의 보관 파일을 지운다 — 탭을 닫을 때와 대화를 비울 때. 없으면 할 일이 없다.
pub fn remove_for(session_id: &str) {
    if let Some(path) = data_dir().and_then(|d| archive_path(&d, session_id))
        && let Err(e) = std::fs::remove_file(&path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(
            "[agent_session] 대화 보관 파일을 못 지웠다({}): {e}",
            path.display()
        );
    }
}

/// 이 plugin 의 데이터 폴더 — 호스트가 띄울 때 준다.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("TASTY_PLUGIN_DATA_DIR").map(PathBuf::from)
}

fn row(t: &Turn) -> Value {
    json!({ "sent": t.sent, "got": t.got, "error": t.error, "at": t.at })
}

fn turn_of(v: &Value) -> Option<Turn> {
    Some(Turn {
        sent: v.get("sent")?.as_str()?.to_string(),
        got: v
            .get("got")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        pending: false,
        at: v.get("at").and_then(Value::as_i64),
        error: v.get("error").and_then(Value::as_str).map(str::to_string),
    })
}

/// 턴들을 보관 파일 끝에 붙인다(한 줄에 하나).
pub fn append(path: &Path, turns: &[Turn]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    for t in turns {
        writeln!(f, "{}", row(t))?;
    }
    f.flush()
}

/// 보관된 턴 중 **가장 최근 `n` 개**(오래된 순). 파일이 없거나 줄이 깨졌으면 그 줄은 건너뛴다.
pub fn read_tail(path: &Path, n: usize) -> Vec<Turn> {
    let Ok(f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    // 끝의 `n` 개만 메모리에 둔다 — 파일 전체를 모았다가 자르면 보관 파일 크기만큼 메모리를 쓴다.
    let mut tail = std::collections::VecDeque::with_capacity(n.min(OLDER_PAGE) + 1);
    for t in BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
        .filter_map(|v| turn_of(&v))
    {
        tail.push_back(t);
        if tail.len() > n {
            tail.pop_front();
        }
    }
    tail.into_iter().collect()
}

/// 보관된 턴 수 — 파일이 없으면 0.
pub fn count(path: &Path) -> usize {
    std::fs::File::open(path)
        .map(|f| BufReader::new(f).lines().map_while(Result::ok).count())
        .unwrap_or(0)
}

/// 앞쪽 턴을 한도까지 잘라 낸다 — 잘린 턴을 돌려준다(옮길지 버릴지는 호출부가 정한다).
///
/// 도는 턴·상한을 넘긴 턴이 있으면 아무것도 안 자른다(모듈 문서 "언제 자르나"). 멈춘 턴의
/// 자리(`stopped`)는 잘린 수만큼 당긴다.
pub fn trim(convo: &mut Convo, keep: usize) -> Vec<Turn> {
    if keep == 0
        || convo.turns.len() <= keep
        || !convo.overdue.is_empty()
        || convo.turns.iter().any(|t| t.pending)
    {
        return Vec::new();
    }
    let cut = convo.turns.len() - keep;
    let removed: Vec<Turn> = convo.turns.drain(..cut).collect();
    crate::domain::sys_note::shift(&mut convo.sys, cut);
    convo.stopped = convo
        .stopped
        .iter()
        .filter(|&&i| i >= cut)
        .map(|&i| i - cut)
        .collect();
    removed
}

/// 한도를 적용한다 — 잘린 턴을 스왑이면 파일로 옮기고 `convo.archived` 를 늘린다.
/// 잘린 수를 돌려준다(0 이면 화면을 안 고쳐도 된다).
pub fn apply(convo: &mut Convo, limits: Limits, data_dir: Option<&Path>) -> usize {
    if limits.keep == 0 {
        return 0;
    }
    let target = if limits.swap {
        let Some(path) = convo
            .session_id
            .as_deref()
            .zip(data_dir)
            .and_then(|(s, d)| archive_path(d, s))
        else {
            tracing::warn!("대화 디스크 스왑을 켰는데 보관할 자리가 없다 — 자르지 않는다");
            return 0;
        };
        Some(path)
    } else {
        None
    };
    let removed = trim(convo, limits.keep);
    if removed.is_empty() {
        return 0;
    }
    let Some(path) = target else {
        convo.dropped += removed.len();
        return removed.len();
    };
    let n = removed.len();
    match append(&path, &removed) {
        Ok(()) => convo.archived += n,
        // 옮기지 못한 턴은 이미 메모리에서 빠졌다 — 되돌려 넣는다(사용자가 고른 것은
        // "버리지 않는다" 다).
        Err(e) => {
            tracing::warn!(
                "대화 보관 실패({}): {e} — 자른 것을 되돌린다",
                path.display()
            );
            convo.turns.splice(0..0, removed);
            convo.stopped = convo.stopped.iter().map(|&i| i + n).collect();
            return 0;
        }
    }
    n
}

/// 잘린 턴을 화면에서도 지운다 — 문서를 다시 싣지 않는다(다시 실으면 치던 글자가 사라진다).
/// 대화(`#log`)의 앞 `cut` 개를 지우고, 아직 안 펼친 보관 수(`remaining`)를 "이전 대화" 링크에
/// 싣는다. 보관된 것이 있으면 줄을 보이고, 남은 것이 없으면 링크만 숨긴다.
pub fn trim_msg(cut: usize, archived: usize, remaining: usize) -> serde_json::Value {
    serde_json::json!({ "type": "turns.trim", "cut": cut, "archived": archived, "remaining": remaining })
}

/// 대화 탭의 턴 한도를 적용한다 — 잘렸으면 그 자리만 화면에서 지운다(문서를 다시 싣지 않는다).
pub fn enforce_on_tab<H: crate::ports::host::HostCall>(
    host: &H,
    sid: u32,
    c: &mut crate::domain::convo::Convo,
) {
    let limits = read_limits(host);
    let cut = apply(c, limits, data_dir().as_deref());
    if cut > 0 {
        tracing::info!(
            "[agent_session] surface={sid}: 턴 {cut} 개를 {} (보관 {})",
            if limits.swap {
                "디스크로 옮겼다"
            } else {
                "버렸다"
            },
            c.archived
        );
        let remaining = c.archived.saturating_sub(c.older.len());
        crate::application::session_surface::patch(
            host,
            sid,
            &trim_msg(cut, c.archived, remaining),
        );
    }
}

/// 보관된 이전 대화를 한 쪽([`OLDER_PAGE`]) 더 펼쳐 다시 그린다.
pub fn expand_older<H: crate::ports::host::HostCall, S: crate::ports::supervisor::Supervisor>(
    host: &H,
    sup: &S,
    sid: u32,
    c: &mut crate::domain::convo::Convo,
) {
    let path = c
        .session_id
        .as_deref()
        .zip(data_dir())
        .and_then(|(s, d)| archive_path(&d, s));
    if let Some(path) = path {
        c.older = read_tail(&path, c.older.len() + OLDER_PAGE);
    }
    crate::application::session_surface::push(host, sup, sid, c, false);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn turn(s: &str) -> Turn {
        Turn {
            sent: s.into(),
            got: format!("re {s}"),
            pending: false,
            at: None,
            error: None,
        }
    }

    fn convo(n: usize) -> Convo {
        Convo {
            session_id: Some("sess-1".into()),
            turns: (0..n).map(|i| turn(&i.to_string())).collect(),
            ..Default::default()
        }
    }

    pub(crate) fn tmp(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // 같은 프로세스에서 다시 불려도 갈리게 카운터를 싣는다 — pid 만으로는 재호출이 겹친다.
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("sup-archive-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d); // 앞 회차 찌꺼기 — 없으면 그만이다
        d
    }

    /// 한도 0 은 무제한이다 — 이 모듈이 생기기 전 동작을 바꾸지 않는다.
    #[test]
    fn zero_keeps_everything() {
        let mut c = convo(30);
        assert_eq!(
            apply(
                &mut c,
                Limits {
                    keep: 0,
                    swap: true
                },
                None
            ),
            0
        );
        assert_eq!(c.turns.len(), 30);
    }

    /// 스왑을 끄면 넘친 턴은 버린다 — 남는 것은 가장 최근 것들이다.
    #[test]
    fn without_swap_the_oldest_are_dropped() {
        let mut c = convo(30);
        assert_eq!(
            apply(
                &mut c,
                Limits {
                    keep: 10,
                    swap: false
                },
                None
            ),
            20
        );
        assert_eq!(c.turns.len(), 10);
        assert_eq!(c.turns[0].sent, "20");
        assert_eq!((c.archived, c.dropped), (0, 20));
    }

    /// 스왑을 켜면 넘친 턴이 파일로 가고, 다시 읽으면 같은 순서로 나온다.
    #[test]
    fn with_swap_the_oldest_go_to_disk_in_order() {
        let d = tmp("swap");
        let mut c = convo(30);
        assert_eq!(
            apply(
                &mut c,
                Limits {
                    keep: 10,
                    swap: true
                },
                Some(&d)
            ),
            20
        );
        assert_eq!((c.turns.len(), c.archived), (10, 20));
        let path = archive_path(&d, "sess-1").expect("경로");
        assert_eq!(count(&path), 20);
        let tail = read_tail(&path, 5);
        let sent: Vec<&str> = tail.iter().map(|t| t.sent.as_str()).collect();
        assert_eq!(sent, ["15", "16", "17", "18", "19"]);
        assert_eq!(tail[0].got, "re 15");
        let _ = std::fs::remove_dir_all(&d); // 시험 찌꺼기 정리
    }

    /// 스왑을 켰는데 옮길 자리가 없으면 자르지 않는다.
    #[test]
    fn swap_without_a_place_keeps_everything() {
        let mut c = convo(30);
        assert_eq!(
            apply(
                &mut c,
                Limits {
                    keep: 10,
                    swap: true
                },
                None
            ),
            0
        );
        assert_eq!(c.turns.len(), 30);
    }

    /// 도는 턴이 있으면 자르지 않는다 — 워커가 든 자리(index)가 밀린다.
    #[test]
    fn a_running_turn_blocks_trimming() {
        let mut c = convo(30);
        c.turns[29].pending = true;
        assert!(trim(&mut c, 10).is_empty());
        c.turns[29].pending = false;
        c.overdue.push(29);
        assert!(trim(&mut c, 10).is_empty());
        c.overdue.clear();
        assert_eq!(trim(&mut c, 10).len(), 20);
    }

    /// 멈춘 턴의 자리는 잘린 만큼 당겨지고, 잘려 나간 것은 빠진다.
    #[test]
    fn stopped_marks_follow_the_cut() {
        let mut c = convo(30);
        c.stopped = vec![5, 25];
        trim(&mut c, 10);
        assert_eq!(c.stopped, vec![5]);
        assert_eq!(c.turns[5].sent, "25");
    }

    /// 경로 문자가 든 세션 id 로는 보관하지 않는다 — 데이터 폴더 밖에 쓰지 않는다.
    #[test]
    fn a_session_id_cannot_escape_the_data_dir() {
        let d = Path::new("/data");
        assert!(archive_path(d, "../x").is_none());
        assert!(archive_path(d, "a/b").is_none());
        assert!(archive_path(d, "").is_none());
        assert!(archive_path(d, "05b729aa-6056").is_some());
    }

    /// 자르기 메시지는 앞쪽 턴 수와 보관 · 남은 수를 싣고, 처리기가 그 값으로 단추를 숨긴다.
    #[test]
    fn the_trim_script_removes_leading_turns_and_updates_the_count() {
        assert_eq!(
            trim_msg(3, 20, 15),
            serde_json::json!({ "type": "turns.trim", "cut": 3, "archived": 20, "remaining": 15 })
        );
        let app = include_str!("../../../../assets/app.js");
        assert!(app.contains("log.turns.splice(0, m.cut);"));
        assert!(app.contains("log.older.archived = m.archived;"));
    }
}
