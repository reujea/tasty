//! 이어받은 대화의 **이전 턴을 대화 기록에서 싣는다** — 메모리는 턴 한도만큼만 쓴다.
//!
//! # 왜 있나
//!
//! 다른 곳(터미널의 claude 등)에서 쓰던 대화를 탭이 이어받으면 탭은 빈 대화로 시작했다. 그
//! 대화의 기록은 claude 가 프로필 폴더에 남긴 jsonl(`<CLAUDE_CONFIG_DIR>/projects/*/<id>.jsonl`)에
//! 이미 있으므로, 탭을 열 때 그것을 읽어 턴으로 보인다.
//!
//! # 메모리를 셸 scrollback 처럼 쓰지 않는다
//!
//! - 파일을 **한 줄씩** 읽는다. 기록 전체를 메모리에 올리지 않는다.
//! - [`MAX_LINE`] 을 넘는 줄은 풀지 않고 건너뛴다 — 그 줄을 다 읽어 두지도 않는다.
//! - 메모리에는 턴 한도(`memory_turns`)만큼만 둔다. 한도가 0(무제한)이어도 [`UNLIMITED_CAP`] 을
//!   넘기지 않는다. 그보다 오래된 턴은 디스크 스왑이 켜져 있으면 세션별 보관 파일로 흘려보내고
//!   (기존 "디스크에 보관한 이전 대화" 가 펼친다), 꺼져 있으면 버린다.
//!
//! # 무엇을 턴으로 보나
//!
//! 사람이 쓴 글(`user` 줄의 글, `isMeta`·하위 에이전트·도구 결과·명령 표지는 뺀다) 하나와, 다음 글까지
//! 나온 답 글(`assistant` 줄의 `text`)이 한 턴이다. 도구 호출·생각은 싣지 않는다.
//!
//! ⚠️ jsonl 은 claude 의 내부 배치다 — 형식이 바뀌면 조용히 비어 보일 수 있다(그때 탭은 지금처럼
//! 빈 대화로 시작한다). 재는 법: 이어받은 탭의 턴 수와 기록의 사람 글 수를 대조한다.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use crate::adapters::driven::store::turn_archive::{self, Limits};
use crate::domain::convo::Turn;
use crate::ports::host::SendHost;
use crate::ports::supervisor::Supervisor;

/// 이 길이를 넘는 줄은 풀지 않는다 — 도구 결과·내부 기록이다(실측 2026-10-20: 4.5MB 기록에서
/// 64KB 를 넘는 줄 3 개가 전부 내부 기록이었고 가장 긴 것이 108KB 였다).
pub const MAX_LINE: usize = 64 * 1024;
/// 한도가 0(무제한)이어도 기록에서는 이만큼만 메모리에 싣는다.
pub const UNLIMITED_CAP: usize = 50;
/// 보관 파일로 흘려보낼 때 한 번에 쓰는 턴 수 — 메모리에 쌓아 두는 상한이기도 하다.
const ARCHIVE_BATCH: usize = 64;

/// 기록에서 실은 결과.
#[derive(Default)]
pub struct Loaded {
    /// 메모리·화면에 둘 최근 턴(오래된 순).
    pub turns: Vec<Turn>,
    /// 보관 파일로 옮긴 턴 수.
    pub archived: usize,
    /// 스왑이 꺼져 있어 버린 턴 수.
    pub dropped: usize,
    /// 너무 길어 풀지 않은 줄 수.
    pub skipped_lines: usize,
}

/// 한 줄을 `buf` 에 읽는다 — [`MAX_LINE`] 을 넘으면 나머지는 버리며 읽어 넘기고 `Some(false)`.
/// 파일 끝이면 `None`.
fn next_line(r: &mut impl BufRead, buf: &mut Vec<u8>) -> std::io::Result<Option<bool>> {
    Ok(read_capped(r, buf)?.map(|l| l.fits))
}

/// [`read_capped`] 가 읽은 줄 하나.
pub(crate) struct CappedLine {
    /// [`MAX_LINE`] 안이라 `buf` 에 실렸다.
    pub fits: bool,
    /// 개행으로 끝났다 — 아니면 쓰는 도중의 반쪽 줄이다.
    pub ended: bool,
    /// 개행까지 포함해 읽어 넘긴 바이트 수 — 이어 읽을 자리를 센다.
    pub bytes: u64,
}

/// 한 줄을 `buf` 에 읽는다 — [`next_line`] 과 같되 읽은 바이트와 개행 여부도 낸다. 파일 끝이면 `None`.
pub(crate) fn read_capped(
    r: &mut impl BufRead,
    buf: &mut Vec<u8>,
) -> std::io::Result<Option<CappedLine>> {
    buf.clear();
    let mut fits = true;
    let mut bytes = 0u64;
    loop {
        let chunk = r.fill_buf()?;
        if chunk.is_empty() {
            return Ok((bytes > 0).then_some(CappedLine {
                fits,
                ended: false,
                bytes,
            }));
        }
        let (part, done) = match chunk.iter().position(|&b| b == b'\n') {
            Some(i) => (&chunk[..i], Some(i + 1)),
            None => (chunk, None),
        };
        if fits && buf.len() + part.len() <= MAX_LINE {
            buf.extend_from_slice(part);
        } else {
            fits = false;
            buf.clear();
        }
        let used = done.unwrap_or(chunk.len());
        r.consume(used);
        bytes += used as u64;
        if done.is_some() {
            return Ok(Some(CappedLine {
                fits,
                ended: true,
                bytes,
            }));
        }
    }
}

/// 사람이 쓴 글 — 턴의 시작이면 `Some`.
fn user_text(v: &Value) -> Option<String> {
    if v.get("isMeta").and_then(Value::as_bool) == Some(true)
        || v.get("isSidechain").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let content = v.pointer("/message/content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            // 도구 결과도 `user` 줄로 온다 — 사람이 쓴 글이 아니다.
            if items
                .iter()
                .any(|x| x.get("type").and_then(Value::as_str) == Some("tool_result"))
            {
                return None;
            }
            items
                .iter()
                .filter(|x| x.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|x| x.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => return None,
    };
    let t = text.trim();
    // 명령 표지(`<command-name>`·`<local-command-stdout>`)와 중단 표지는 사람이 쓴 글이 아니다.
    if t.is_empty() || t.starts_with('<') || t.starts_with("[Request interrupted") {
        return None;
    }
    Some(t.to_string())
}

/// 답 글 — `assistant` 줄의 `text` 조각들.
fn assistant_text(v: &Value) -> Option<String> {
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let parts: Vec<&str> = v
        .pointer("/message/content")?
        .as_array()?
        .iter()
        .filter(|x| x.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|x| x.get("text").and_then(Value::as_str))
        .filter(|s| !s.trim().is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

/// 최근 `keep` 턴만 메모리에 두고 넘친 것은 보관 파일로 흘리거나 버리는 자리.
struct Ring<'a> {
    keep: usize,
    turns: VecDeque<Turn>,
    archive: Option<&'a Path>,
    pending: Vec<Turn>,
    out: Loaded,
}

impl Ring<'_> {
    fn push(&mut self, t: Turn) -> std::io::Result<()> {
        self.turns.push_back(t);
        if self.turns.len() <= self.keep {
            return Ok(());
        }
        let Some(old) = self.turns.pop_front() else {
            return Ok(());
        };
        match self.archive {
            Some(_) => {
                self.pending.push(old);
                if self.pending.len() >= ARCHIVE_BATCH {
                    self.flush()?;
                }
            }
            None => self.out.dropped += 1,
        }
        Ok(())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(p) = self.archive
            && !self.pending.is_empty()
        {
            turn_archive::append(p, &self.pending)?;
            self.out.archived += self.pending.len();
            self.pending.clear();
        }
        Ok(())
    }
}

/// 기록 파일에서 턴을 싣는다 — 메모리에는 최근 `keep` 턴만(0 이면 [`UNLIMITED_CAP`]).
pub fn load(path: &Path, keep: usize, archive: Option<&Path>) -> std::io::Result<Loaded> {
    let mut r = BufReader::new(std::fs::File::open(path)?);
    let keep = if keep == 0 { UNLIMITED_CAP } else { keep };
    let mut ring = Ring {
        keep,
        turns: VecDeque::with_capacity(keep.min(UNLIMITED_CAP) + 1),
        archive,
        pending: Vec::new(),
        out: Loaded::default(),
    };
    let mut buf = Vec::new();
    let mut cur: Option<Turn> = None;
    // 안내 턴의 답을 버리는 중인가 — 다음 사람 말이 오면 풀린다.
    let mut skip_answer = false;
    while let Some(fits) = next_line(&mut r, &mut buf)? {
        if !fits {
            ring.out.skipped_lines += 1;
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        match v.get("type").and_then(Value::as_str) {
            Some("user") => {
                if let Some(sent) = user_text(&v) {
                    // 탭이 숨겨 보낸 안내는 대화가 아니다 — 그 턴째(답까지) 건너뛴다.
                    if crate::application::start::is_brief(&sent) {
                        if let Some(done) = cur.take() {
                            ring.push(done)?;
                        }
                        skip_answer = true;
                        continue;
                    }
                    skip_answer = false;
                    if let Some(done) = cur.take() {
                        ring.push(done)?;
                    }
                    cur = Some(Turn {
                        sent,
                        got: String::new(),
                        pending: false,
                        at: v
                            .get("timestamp")
                            .and_then(Value::as_str)
                            .and_then(crate::adapters::driven::store::budget::epoch),
                        error: None,
                    });
                }
            }
            Some("assistant") if skip_answer => {}
            Some("assistant") => {
                if let (Some(t), Some(text)) = (cur.as_mut(), assistant_text(&v)) {
                    if !t.got.is_empty() {
                        t.got.push_str("\n\n");
                    }
                    t.got.push_str(&text);
                }
            }
            _ => {}
        }
    }
    if let Some(done) = cur.take() {
        ring.push(done)?;
    }
    ring.flush()?;
    ring.out.turns = ring.turns.into_iter().collect();
    Ok(ring.out)
}

/// 세션이 붙든 대화의 기록을 찾아 싣는다 — 찾을 수 없으면 `None`(탭은 빈 대화로 시작한다).
///
/// claude agent 만 본다(기록 위치가 claude 의 배치다).
pub fn fill<S: Supervisor>(
    sup: &S,
    session: &str,
    limits: Limits,
    data_dir: Option<&Path>,
) -> Option<Loaded> {
    let st = sup.status(session).ok()?;
    let agent = st.get("agent_name").and_then(Value::as_str)?;
    let convo = st.get("agent_session_id").and_then(Value::as_str)?;
    let profile = sup
        .agent_list()
        .ok()?
        .into_iter()
        .find(|p| p.name == agent && p.kind == "claude")?;
    let dir = crate::adapters::driven::store::budget::config_dir(&profile.env)?;
    let path = crate::adapters::driven::store::budget::find_transcript(&dir, convo)?;
    let archive = data_dir
        .filter(|_| limits.swap)
        .and_then(|d| turn_archive::archive_path(d, session));
    match load(&path, limits.keep, archive.as_deref()) {
        Ok(l) => {
            tracing::info!(
                "[agent_session] 대화 기록에서 실었다: 메모리 {} · 보관 {} · 버림 {} · 건너뛴 줄 {}",
                l.turns.len(),
                l.archived,
                l.dropped,
                l.skipped_lines
            );
            Some(l)
        }
        Err(e) => {
            tracing::warn!(
                "[agent_session] 대화 기록을 못 읽었다({}): {e}",
                path.display()
            );
            None
        }
    }
}

/// 탭을 열 때 — 다른 곳에서 쓰던 대화면 이전 턴을 대화 기록에서 싣는다.
///
/// 복원(`restoring`)은 스냅샷이 정본이고, 탭마다 데몬에 묻는 비용이 복원을 늦추므로 뺀다. 이미 턴이나
/// 보관된 턴이 있는 탭도 뺀다(두 번 실으면 보관 파일에 같은 턴이 겹친다).
pub fn for_tab<H: crate::ports::host::HostCall, S: Supervisor>(
    h: &H,
    sup: &S,
    session: Option<&str>,
    turns: Vec<Turn>,
    archived: usize,
    restoring: bool,
) -> (Vec<Turn>, usize) {
    match session {
        Some(s) if !restoring && turns.is_empty() && archived == 0 => {
            let limits = turn_archive::read_limits(h);
            fill(sup, s, limits, turn_archive::data_dir().as_deref())
                .map_or((turns, archived), |l| (l.turns, l.archived))
        }
        _ => (turns, archived),
    }
}

/// 복원된 탭의 이전 턴이 다 읽혔다 — 워커가 되던지는 메서드(`self_invoke`).
pub const METHOD: &str = "agent_session.history_loaded";

/// 복원됐는데 스냅샷에 턴이 없는 탭이면 **뒤에서** 기록을 읽는다 — 복원을 막지 않는다.
///
/// 복원은 스냅샷이 정본이지만, 스냅샷이 비어 오면(plugin 을 다시 띄웠거나 턴을 담기 전에 저장됐다)
/// 탭이 빈 대화로 남았다(실측 2026-10-21: plugin 재기동 뒤 대화 탭 7개가 모두 주고받음 0).
/// 그 자리에서 읽으면 탭마다 데몬에 묻고 파일을 훑는 시간이 복원 응답에 붙어, 탭이 여럿이면
/// 호스트가 plugin 을 응답 없음으로 죽이는 60초에 닿는다.
/// 그래서 워커가 읽고 [`METHOD`] 로 넘긴다 — 탭은 먼저 빈 채로 뜨고 조금 뒤 이전 턴이 채워진다.
pub fn load_in_background<S: Supervisor + Send + 'static>(
    host: impl SendHost,
    sup: S,
    surface_id: u32,
    session: String,
    limits: Limits,
) {
    let spawned = std::thread::Builder::new()
        .name(format!("agent-session-history-{surface_id}"))
        .spawn(move || {
            let Some(l) = fill(&sup, &session, limits, turn_archive::data_dir().as_deref()) else {
                return;
            };
            let params = serde_json::json!({
                "surface_id": surface_id,
                "session": session,
                "archived": l.archived,
                "snapshot": crate::domain::convo::snapshot_with_turns(Some(&session), &l.turns),
            });
            if let Err(e) = host.self_invoke(METHOD, params) {
                tracing::debug!("이전 턴을 넘기지 못했다(plugin 이 내려가는 중): {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("이전 턴을 읽을 워커를 못 띄웠다 — 복원된 탭이 빈 대화로 남는다: {e}");
    }
}

/// 워커가 읽은 턴을 탭에 싣는다 — 실었으면 다시 그릴 탭.
///
/// 그 사이 탭에 턴이 생겼거나(사용자가 보냈다 · 에이전트가 이어 갔다) 다른 세션으로 바뀌었으면
/// 싣지 않는다 — 앞에 끼우면 화면 순서와 턴 자리가 어긋난다.
pub fn apply_loaded(convos: &mut crate::domain::convo::Convos, params: &Value) -> Option<u32> {
    let field = |k: &str| {
        params
            .pointer(&format!("/params/{k}"))
            .or_else(|| params.get(k))
    };
    let sid = u32::try_from(field("surface_id")?.as_u64()?).ok()?;
    let session = field("session")?.as_str()?;
    let convo = convos.get_mut(&sid)?;
    if convo.session_id.as_deref() != Some(session)
        || !convo.turns.is_empty()
        || convo.archived != 0
    {
        return None;
    }
    convo.turns = crate::domain::convo::turns_from_snapshot(field("snapshot")?);
    convo.archived = field("archived").and_then(Value::as_u64).unwrap_or(0) as usize;
    (!convo.turns.is_empty() || convo.archived != 0).then_some(sid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::driven::store::turn_archive::tests::tmp;
    use crate::ports::supervisor::fake::FakeSupervisor;
    use serde_json::json;
    use std::io::Write;

    fn user(t: &str) -> Value {
        json!({ "type": "user", "message": { "role": "user", "content": t } })
    }
    fn asst(t: &str) -> Value {
        json!({ "type": "assistant", "message": { "content": [ { "type": "text", "text": t } ] } })
    }

    fn write(dir: &Path, lines: &[Value]) -> std::path::PathBuf {
        std::fs::create_dir_all(dir).expect("dir");
        let p = dir.join("c.jsonl");
        let mut f = std::fs::File::create(&p).expect("file");
        for l in lines {
            writeln!(f, "{l}").expect("write");
        }
        p
    }

    /// 턴의 시각은 사람 글 줄의 `timestamp` 다 — 화면의 날짜 줄과 `[HH:MM:ss]` 가 그 값을 쓴다.
    #[test]
    fn a_loaded_turn_takes_its_time_from_the_human_line() {
        let d = tmp("hist-time");
        let mut u = user("질문");
        u["timestamp"] = json!("2026-11-02T01:02:03.456Z");
        let p = write(&d, &[u, asst("답"), user("시각 없는 질문")]);
        let l = load(&p, 0, None).expect("load");
        assert_eq!(
            l.turns[0].at,
            Some(1_793_581_323),
            "기록의 시각을 안 읽었다"
        );
        assert_eq!(l.turns[1].at, None, "시각 없는 줄에 시각이 생겼다");
    }

    /// 사람 글 하나와 그 뒤 답 글들이 한 턴이다 — 도구·생각·메타·하위 에이전트는 빠진다.
    /// 탭이 숨겨 보낸 안내 턴은 그 답까지 이전 대화로 실리지 않는다.
    #[test]
    fn the_tab_brief_and_its_answer_are_not_loaded() {
        let d = tmp("brief");
        let brief = crate::application::start::session_brief(2, "s", "fake", None);
        let p = write(&d, &[user(&brief), asst("ack"), user("hi"), asst("hello")]);
        let l = load(&p, 0, None).expect("load");
        assert_eq!(
            l.turns.len(),
            1,
            "안내 턴이 실렸다: {:?}",
            l.turns.iter().map(|t| &t.sent).collect::<Vec<_>>()
        );
        assert_eq!(
            (l.turns[0].sent.as_str(), l.turns[0].got.as_str()),
            ("hi", "hello")
        );
    }

    #[test]
    fn a_turn_is_a_person_line_and_the_answers_after_it() {
        let d = tmp("hist-shape");
        let p = write(
            &d,
            &[
                user("첫 질문"),
                json!({ "type": "assistant", "message": { "content": [
                    { "type": "thinking", "thinking": "생각" },
                    { "type": "tool_use", "name": "Bash" } ] } }),
                json!({ "type": "user", "message": { "content": [
                    { "type": "tool_result", "content": "도구 출력" } ] } }),
                // 도구 결과에 글 조각이 함께 실려 와도 사람이 쓴 글이 아니다.
                json!({ "type": "user", "message": { "content": [
                    { "type": "tool_result", "content": "출력" },
                    { "type": "text", "text": "도구가 덧붙인 글" } ] } }),
                asst("답 하나"),
                asst("답 둘"),
                json!({ "type": "user", "isMeta": true, "message": { "content": "메타" } }),
                json!({ "type": "user", "message": { "content": "<command-name>/clear</command-name>" } }),
                json!({ "type": "assistant", "isSidechain": true, "message": { "content": [
                    { "type": "text", "text": "하위 에이전트" } ] } }),
                user("둘째 질문"),
                asst("둘째 답"),
            ],
        );
        let l = load(&p, 10, None).expect("load");
        assert_eq!(l.turns.len(), 2);
        assert_eq!(l.turns[0].sent, "첫 질문");
        assert_eq!(l.turns[0].got, "답 하나\n\n답 둘");
        assert_eq!(l.turns[1].sent, "둘째 질문");
        assert_eq!(l.turns[1].got, "둘째 답");
    }

    /// 메모리에는 한도만큼만 — 넘친 것은 스왑이면 보관 파일로, 아니면 버린다.
    #[test]
    fn only_the_limit_stays_in_memory() {
        let d = tmp("hist-limit");
        let lines: Vec<Value> = (0..200)
            .flat_map(|i| [user(&format!("q{i}")), asst(&format!("a{i}"))])
            .collect();
        let p = write(&d, &lines);

        let dropped = load(&p, 30, None).expect("load");
        assert_eq!(dropped.turns.len(), 30);
        assert_eq!(dropped.turns[0].sent, "q170");
        assert_eq!(dropped.dropped, 170);

        let arch = d.join("archive.jsonl");
        let swapped = load(&p, 30, Some(&arch)).expect("load");
        assert_eq!(swapped.turns.len(), 30);
        assert_eq!(swapped.archived, 170);
        assert_eq!(turn_archive::count(&arch), 170);
        // 보관 파일의 끝이 메모리 첫 턴 바로 앞이다 — 순서가 이어진다.
        assert_eq!(turn_archive::read_tail(&arch, 1)[0].sent, "q169");
    }

    /// 한도 0(무제한)이어도 기록 전체를 싣지 않는다.
    #[test]
    fn unlimited_still_has_a_cap() {
        let d = tmp("hist-cap");
        let lines: Vec<Value> = (0..120).map(|i| user(&format!("q{i}"))).collect();
        let l = load(&write(&d, &lines), 0, None).expect("load");
        assert_eq!(l.turns.len(), UNLIMITED_CAP);
    }

    /// 너무 긴 줄은 풀지 않고 건너뛴다 — 그 뒤 줄은 그대로 읽힌다.
    #[test]
    fn a_huge_line_is_skipped_not_loaded() {
        let d = tmp("hist-huge");
        let big = "x".repeat(MAX_LINE * 3);
        let p = write(&d, &[user("앞"), asst(&big), user("뒤"), asst("짧은 답")]);
        let l = load(&p, 10, None).expect("load");
        assert_eq!(l.skipped_lines, 1);
        assert_eq!(l.turns.len(), 2);
        assert_eq!(l.turns[0].got, "");
        assert_eq!(l.turns[1].got, "짧은 답");
    }

    /// 세션 → agent 프로필 → 기록 파일까지 실제로 찾아 간다.
    #[test]
    fn fill_finds_the_transcript_through_the_profile() {
        let d = tmp("hist-fill");
        let project = d.join("projects").join("-tmp-w");
        std::fs::create_dir_all(&project).expect("dir");
        let mut f = std::fs::File::create(project.join("convo-1.jsonl")).expect("file");
        writeln!(f, "{}", user("이전 질문")).expect("w");
        writeln!(f, "{}", asst("이전 답")).expect("w");
        let sup = FakeSupervisor::new();
        sup.dev_env
            .borrow_mut()
            .insert("CLAUDE_CONFIG_DIR".into(), d.display().to_string());
        *sup.agent_session.borrow_mut() = Some("convo-1".into());
        let l = fill(
            &sup,
            "sess-1",
            Limits {
                keep: 10,
                swap: false,
            },
            None,
        )
        .expect("loaded");
        assert_eq!(l.turns.len(), 1);
        assert_eq!(l.turns[0].sent, "이전 질문");
        assert_eq!(l.turns[0].got, "이전 답");
    }

    /// 탭을 열 때 실제로 이것을 부른다 — 함수만 있고 배선이 없으면 탭은 여전히 빈 대화로 열린다.
    /// 복원(`restoring`)과 이미 턴이 있는 탭은 건너뛰어야 한다(스냅샷이 정본이다).
    #[test]
    fn opening_a_tab_loads_the_history() {
        let main = crate::test_support::wiring_source();
        let body = main
            .split("fn open_session_surface")
            .nth(1)
            .and_then(|s| s.split("\n    fn ").next())
            .expect("open_session_surface");
        assert!(
            body.contains("history::for_tab("),
            "탭을 열 때 history::for_tab 을 안 부른다"
        );
        let me = include_str!("history.rs");
        let f = me.split("pub fn for_tab").nth(1).expect("for_tab");
        let at = f.find("fill(").expect("fill 배선이 없다");
        assert!(f[..at].contains("!restoring && turns.is_empty() && archived == 0"));
    }

    /// 뒤에서 읽은 턴은 **아직 빈 탭에만** 실린다 — 그 사이 턴이 생겼거나 세션이 바뀌었으면 버린다.
    #[test]
    fn loaded_turns_fill_only_a_still_empty_tab() {
        use crate::domain::convo::Convo;
        use crate::domain::convo::Convos;
        let turn = |s: &str| Turn {
            sent: s.into(),
            got: "답".into(),
            error: None,
            pending: false,
            at: None,
        };
        let msg = |sid: u32, session: &str| {
            json!({ "surface_id": sid, "session": session, "archived": 3,
                    "snapshot": crate::domain::convo::snapshot_with_turns(Some(session), &[turn("이전 말")]) })
        };
        let mut c: Convos = Default::default();
        c.insert(
            1,
            Convo {
                session_id: Some("s1".into()),
                ..Default::default()
            },
        );
        c.insert(
            2,
            Convo {
                session_id: Some("s2".into()),
                turns: vec![turn("새 말")],
                ..Default::default()
            },
        );
        assert_eq!(apply_loaded(&mut c, &msg(1, "s1")), Some(1));
        assert_eq!(c[&1].turns[0].sent, "이전 말");
        assert_eq!(c[&1].archived, 3);
        assert_eq!(
            apply_loaded(&mut c, &msg(2, "s2")),
            None,
            "턴이 생긴 탭에 끼웠다"
        );
        assert_eq!(c[&2].turns.len(), 1);
        c.insert(
            3,
            Convo {
                session_id: Some("other".into()),
                ..Default::default()
            },
        );
        assert_eq!(
            apply_loaded(&mut c, &msg(3, "s1")),
            None,
            "다른 세션의 턴을 실었다"
        );
        assert_eq!(apply_loaded(&mut c, &msg(9, "s1")), None);
    }

    /// 복원 경로가 뒤에서 읽기를 건다 — 스냅샷이 비었을 때만.
    #[test]
    fn a_restored_empty_tab_loads_in_the_background() {
        let main = crate::test_support::wiring_source();
        let body = main
            .split("fn open_session_surface")
            .nth(1)
            .and_then(|s| s.split("\n    fn ").next())
            .expect("open_session_surface");
        assert!(
            body.contains("history::load_in_background("),
            "복원 경로에 배선이 없다"
        );
        assert!(
            main.contains("history::METHOD =>"),
            "결과를 받는 라우팅이 없다"
        );
    }

    /// 대화 id 가 없는 세션(데몬이 좌표를 안 냄)은 싣지 않는다 — 빈 대화로 시작한다.
    #[test]
    fn no_coordinate_means_nothing_to_load() {
        let sup = FakeSupervisor::new();
        assert!(
            fill(
                &sup,
                "sess-1",
                Limits {
                    keep: 10,
                    swap: false
                },
                None
            )
            .is_none()
        );
    }
}
