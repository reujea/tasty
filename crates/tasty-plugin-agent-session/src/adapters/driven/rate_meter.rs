//! agent 프로필 단위 **분당 사용 계측** — 그 프로필로 도는 모든 세션이 최근 1분에 API 에 보낸 양.
//!
//! # 왜 프로필 단위인가
//!
//! API 한도(RPM · ITPM · OTPM)는 세션이 아니라 프로필이 쓰는 계정(조직)에 걸린다. 한 탭의 상태줄이
//! 자기 턴만 보면, 같은 프로필의 다른 탭 · 터미널의 claude · 하위 에이전트가 한도를 채워 가는 것을
//! 모른다. 그래서 그 프로필 설정 폴더(`CLAUDE_CONFIG_DIR`)의 대화 기록 **전부**에서 최근 몫을 센다.
//!
//! # 무엇을 세나 (Claude Code 대화 기록의 `assistant` 줄)
//!
//! | 값 | 규칙 |
//! |---|---|
//! | 요청(RPM) | 최근 [`WINDOW`] 안의 서로 다른 요청 — `requestId`, 없으면 `message.id` 로 중복을 걸러낸다(스트리밍은 한 응답을 여러 줄로 적는다) |
//! | 입력(ITPM) | 그 요청들의 `input_tokens + cache_creation_input_tokens` — 캐시 읽기는 분당 입력 한도에 세지 않는다 |
//! | 출력(OTPM) | 그 요청들의 `output_tokens` |
//! | 거절 | 최근 [`REJECT_WINDOW`] 안의 `"error": "rate_limit"` 줄 수 |
//! | 마지막 성공 · 마지막 거절 | 그 두 종류 줄의 가장 늦은 시각 — 다른 세션이 막혔는지 풀렸는지를 차단기(`rate_block`)에 알린다 |
//!
//! 기록에 남은 값으로 센 **추정**이다. 서버의 버킷 잔량(`anthropic-ratelimit-*` 헤더)은 Claude Code 가
//! 넘기지 않아 알 수 없다.
//!
//! # 어디서 도나
//!
//! 기록은 프로필마다 수천 파일 · 수 GB 다. plugin 루프에서 읽지 않고 별도 스레드가 [`PERIOD`] 마다
//! 열린 탭이 쓰는 프로필만 잰다. 최근 [`REJECT_WINDOW`] 안에 바뀐 파일만, 그것도 끝 [`TAIL_BYTES`] 만
//! 읽는다 — 최근 1분의 줄은 파일 끝에 있다.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

/// 분당 값을 세는 폭.
pub const WINDOW: i64 = 60;
/// 거절을 세는 폭.
pub const REJECT_WINDOW: i64 = 300;
/// 다시 재는 주기.
pub const PERIOD: Duration = Duration::from_secs(15);
/// 파일마다 읽는 끝부분 — 큰 컨텍스트 한 줄이 수백 KB 라 넉넉히 잡는다.
pub const TAIL_BYTES: u64 = 4 * 1024 * 1024;

/// 한 프로필의 최근 사용.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Meter {
    pub requests: u32,
    pub input: u64,
    pub output: u64,
    pub rejected: u32,
    pub last_ok: Option<i64>,
    pub last_rejected: Option<i64>,
}

/// 기록 줄들을 센다 — **순수 함수**(파일 · 시계 없음). 줄 순서는 상관없다.
pub fn aggregate<'a>(lines: impl IntoIterator<Item = &'a str>, now: i64) -> Meter {
    let mut m = Meter::default();
    let mut seen: HashSet<String> = HashSet::new();
    for line in lines {
        // 거의 모든 줄은 assistant 가 아니다 — 파싱 전에 거른다.
        if !line.contains("\"assistant\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(at) = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(crate::adapters::driven::store::budget::epoch)
        else {
            continue;
        };
        if at > now {
            continue;
        }
        if v.get("error").and_then(Value::as_str) == Some("rate_limit") {
            m.last_rejected = m.last_rejected.max(Some(at));
            if now - at < REJECT_WINDOW {
                m.rejected += 1;
            }
            continue;
        }
        let Some(usage) = v.pointer("/message/usage") else {
            continue;
        };
        m.last_ok = m.last_ok.max(Some(at));
        if now - at >= WINDOW {
            continue;
        }
        let key = v
            .get("requestId")
            .and_then(Value::as_str)
            .or_else(|| v.pointer("/message/id").and_then(Value::as_str))
            .unwrap_or_default();
        if !key.is_empty() && !seen.insert(key.to_string()) {
            continue;
        }
        let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
        m.requests += 1;
        m.input += n("input_tokens") + n("cache_creation_input_tokens");
        m.output += n("output_tokens");
    }
    m
}

/// 그 설정 폴더의 대화 기록 중 최근 [`REJECT_WINDOW`] 안에 바뀐 것 — 하위 에이전트 기록까지.
fn recent_files(config_dir: &Path, now: i64) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![config_dir.join("projects")];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(meta) = e.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "jsonl")
                && meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .is_some_and(|t| now - (t.as_secs() as i64) < REJECT_WINDOW)
            {
                out.push(p);
            }
        }
    }
    out
}

/// 파일 끝 [`TAIL_BYTES`] 를 줄로 — 잘린 첫 줄은 버린다.
fn tail(path: &Path) -> String {
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(TAIL_BYTES);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    if let Err(e) = f.read_to_end(&mut buf) {
        tracing::debug!(
            "[agent_session] 기록 끝을 못 읽었다 {}: {e}",
            path.display()
        );
        return String::new();
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    if start == 0 {
        text
    } else {
        text.split_once('\n')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_default()
    }
}

/// 그 설정 폴더 하나를 잰다.
pub fn measure(config_dir: &Path, now: i64) -> Meter {
    let texts: Vec<String> = recent_files(config_dir, now)
        .iter()
        .map(|p| tail(p))
        .collect();
    aggregate(texts.iter().flat_map(|t| t.lines()), now)
}

#[derive(Debug, Default)]
struct Shared {
    /// 잴 프로필 — 열린 탭이 쓰는 것만.
    watched: BTreeSet<String>,
    values: BTreeMap<String, Meter>,
}

/// 잰 값을 plugin 과 나누는 손잡이. 스레드는 [`Meters::start`] 로 한 번만 띄운다.
#[derive(Debug, Default, Clone)]
pub struct Meters {
    shared: Arc<Mutex<Shared>>,
    started: Arc<std::sync::atomic::AtomicBool>,
}

impl Meters {
    /// 잴 프로필을 바꾼다 — 빠진 프로필의 값은 지운다.
    pub fn watch(&self, profiles: BTreeSet<String>) {
        let mut s = lock(&self.shared);
        s.values.retain(|k, _| profiles.contains(k));
        s.watched = profiles;
    }

    pub fn get(&self, profile: &str) -> Option<Meter> {
        lock(&self.shared).values.get(profile).copied()
    }

    /// 측정 스레드를 띄운다 — 두 번째 부름은 아무것도 안 한다.
    pub fn start(&self) {
        use std::sync::atomic::Ordering;
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = Arc::clone(&self.shared);
        let spawned = std::thread::Builder::new()
            .name("agent-session-rate-meter".into())
            .spawn(move || {
                loop {
                    let watched = lock(&shared).watched.clone();
                    if !watched.is_empty() {
                        let profiles = crate::adapters::driven::store::agents::load(
                            crate::adapters::driven::store::agents::default_dir().as_deref(),
                        );
                        let now = crate::adapters::driven::store::budget::now();
                        let mut got = BTreeMap::new();
                        for name in &watched {
                            let Some(p) = profiles.iter().find(|p| &p.name == name) else {
                                continue;
                            };
                            if let Some(dir) =
                                crate::adapters::driven::store::budget::config_dir(&p.env)
                            {
                                got.insert(name.clone(), measure(&dir, now));
                            }
                        }
                        lock(&shared).values = got;
                    }
                    std::thread::sleep(PERIOD);
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("[agent_session] 분당 사용 계측 스레드를 못 띄웠다: {e}");
            self.started.store(false, Ordering::SeqCst);
        }
    }
}

/// 유닉스 초 → 지금 기준 `Instant`(미래면 지금).
fn instant_of(at: i64, now_unix: i64, now: std::time::Instant) -> std::time::Instant {
    let ago = u64::try_from(now_unix - at).unwrap_or(0);
    now.checked_sub(Duration::from_secs(ago)).unwrap_or(now)
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// `flush_queues` 의 앞 — 잴 프로필을 알리고, 잰 값을 탭과 차단기에 싣는다. 바뀐 탭만 상태줄을 다시 그린다.
    pub(crate) fn apply_meters(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let profiles: BTreeSet<String> = self
            .convos
            .values()
            .filter_map(|c| c.agent_name.clone())
            .collect();
        self.meters.watch(profiles.clone());
        let now = std::time::Instant::now();
        let now_unix = crate::adapters::driven::store::budget::now();
        for p in &profiles {
            let Some(m) = self.meters.get(p) else {
                continue;
            };
            let seen = |t: Option<i64>| t.map(|t| instant_of(t, now_unix, now));
            match self
                .blocks
                .note_observed(p, seen(m.last_rejected), seen(m.last_ok), now)
            {
                crate::domain::rate_block::Observed::Opened => tracing::info!(
                    "[agent_session] 프로필 {p:?} 의 다른 세션이 429 를 받았다 — 자동 전송을 멈춘다"
                ),
                crate::domain::rate_block::Observed::Released => tracing::info!(
                    "[agent_session] 프로필 {p:?} 의 다른 세션이 API 에 닿았다 — 차단을 푼다"
                ),
                crate::domain::rate_block::Observed::Unchanged => {}
            }
        }
        let mut changed = Vec::new();
        for (&sid, c) in self.convos.iter_mut() {
            let p = crate::domain::rate_block::profile_of(c);
            let meter = if p.is_empty() {
                None
            } else {
                self.meters.get(p)
            };
            let held = self
                .blocks
                .get(p)
                .map(|b| now.saturating_duration_since(b.since).as_secs());
            if c.meter != meter || c.held_secs != held {
                c.meter = meter;
                c.held_secs = held;
                changed.push(sid);
            }
        }
        for sid in changed {
            if let Some(c) = self.convos.get(&sid) {
                self.activity.refresh_status(&host, sid, c);
            }
        }
    }
}

fn lock(m: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    m.lock().unwrap_or_else(|p| {
        tracing::warn!("rate meter lock poisoned; keeping the last readings");
        p.into_inner()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_600_000; // 2026-11-02T06:13:20Z

    fn ts(at: i64) -> String {
        let s = at.rem_euclid(86_400);
        let days = at.div_euclid(86_400);
        // NOW 와 같은 날만 쓴다 — 날짜 부분은 고정.
        assert_eq!(days, NOW.div_euclid(86_400));
        format!(
            "2026-11-02T{:02}:{:02}:{:02}.000Z",
            s / 3600,
            s % 3600 / 60,
            s % 60
        )
    }

    fn ok(at: i64, req: &str, input: u64, create: u64, read: u64, out: u64) -> String {
        serde_json::json!({
            "type": "assistant", "timestamp": ts(at), "requestId": req,
            "message": { "id": format!("m-{req}"), "usage": {
                "input_tokens": input, "cache_creation_input_tokens": create,
                "cache_read_input_tokens": read, "output_tokens": out } }
        })
        .to_string()
    }

    fn rejected(at: i64) -> String {
        serde_json::json!({
            "type": "assistant", "timestamp": ts(at), "error": "rate_limit",
            "isApiErrorMessage": true,
            "message": { "content": [{ "type": "text",
                "text": "API Error: Request rejected (429) · This request would exceed your account's rate limit. Please try again later." }] }
        })
        .to_string()
    }

    /// 2026-11-02 03:41 의 모양 — 식은 캐시를 통째로 다시 쓴 한 요청이 스트리밍으로 세 줄 적혔다.
    /// 캐시 읽기는 분당 입력에 안 세고, 같은 요청은 한 번만 센다.
    #[test]
    fn a_minute_counts_each_request_once_and_skips_cache_reads() {
        let lines = [
            ok(NOW - 5, "r1", 2, 331_000, 13_000, 10),
            ok(NOW - 5, "r1", 2, 331_000, 13_000, 120),
            ok(NOW - 4, "r1", 2, 331_000, 13_000, 120),
            ok(NOW - 30, "r2", 1, 2_000, 140_000, 300),
            ok(NOW - 61, "r3", 1, 900_000, 0, 9),
            "{\"type\":\"user\"}".to_string(),
            "깨진 줄".to_string(),
        ];
        let m = aggregate(lines.iter().map(String::as_str), NOW);
        assert_eq!(m.requests, 2, "같은 요청을 두 번 셌거나 1분 밖을 셌다");
        assert_eq!(m.input, 2 + 331_000 + 1 + 2_000);
        assert_eq!(
            m.output,
            10 + 300,
            "첫 줄 값을 쓴다 — 줄 순서대로 처음 본 것"
        );
        assert_eq!(m.last_ok, Some(NOW - 4));
        assert_eq!(m.rejected, 0);
        assert_eq!(m.last_rejected, None);
    }

    #[test]
    fn rejections_count_for_five_minutes_and_both_last_outcomes_are_kept() {
        let lines = [
            ok(NOW - 400, "r1", 1, 1, 0, 1),
            rejected(NOW - 299),
            rejected(NOW - 10),
            rejected(NOW - 301),
        ];
        let m = aggregate(lines.iter().map(String::as_str), NOW);
        assert_eq!(m.rejected, 2);
        assert_eq!(m.requests, 0);
        assert_eq!(m.last_rejected, Some(NOW - 10));
        assert_eq!(m.last_ok, Some(NOW - 400), "429 줄을 성공으로 셌다");
        let after = [lines[2].clone(), ok(NOW - 1, "r9", 1, 1, 0, 1)];
        let a = aggregate(after.iter().map(String::as_str), NOW);
        assert_eq!(
            (a.last_rejected, a.last_ok),
            (Some(NOW - 10), Some(NOW - 1))
        );
    }

    /// 파일에서 — 최근에 바뀐 기록만, 하위 에이전트 폴더까지, 끝만 읽는다.
    #[test]
    fn measure_reads_recent_transcripts_including_subagents() {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "agent-session-rate-meter-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let sub = root.join("projects/-proj/sess-1/subagents");
        std::fs::create_dir_all(&sub).unwrap();
        let now = crate::adapters::driven::store::budget::now();
        let line = |req: &str, create: u64| {
            serde_json::json!({
                "type": "assistant",
                "timestamp": chrono_like(now - 2),
                "requestId": req,
                "message": { "usage": { "input_tokens": 1, "cache_creation_input_tokens": create, "output_tokens": 1 } }
            })
            .to_string()
        };
        std::fs::write(
            root.join("projects/-proj/sess-1.jsonl"),
            format!("{}\n", line("a", 100)),
        )
        .unwrap();
        std::fs::write(sub.join("agent-x.jsonl"), format!("{}\n", line("b", 200))).unwrap();
        std::fs::write(root.join("projects/-proj/notes.txt"), line("c", 999)).unwrap();
        // 한 시간 전에 마지막으로 바뀐 기록 — 줄 시각이 최근이어도 파일을 안 연다.
        let old = root.join("projects/-proj/old.jsonl");
        std::fs::write(&old, format!("{}\n", line("d", 5_000))).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
        let m = measure(&root, now);
        std::fs::remove_dir_all(&root).ok();
        assert_eq!(
            m.requests, 2,
            "하위 에이전트 기록을 빼거나 jsonl 아닌 파일을 셌다"
        );
        assert_eq!(m.input, 1 + 100 + 1 + 200);
    }

    /// 유닉스 초 → `YYYY-MM-DDTHH:MM:SSZ`(시험용, 1970 이후).
    fn chrono_like(at: i64) -> String {
        let (days, s) = (at.div_euclid(86_400), at.rem_euclid(86_400));
        // Howard Hinnant 의 civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            s / 3600,
            s % 3600 / 60,
            s % 60
        )
    }

    /// 2초 루프가 계측을 싣고, plugin 이 뜰 때 측정 스레드를 띄운다.
    #[test]
    fn the_loop_applies_meters_and_the_start_spawns_the_thread() {
        let main = crate::test_support::wiring_source();
        let prod = main.split("#[cfg(test)]").next().unwrap_or(main);
        let flush = prod
            .split("fn flush_queues(")
            .nth(1)
            .and_then(|s| s.split("\n    }\n").next())
            .expect("flush_queues 가 없다");
        let apply = flush
            .find("self.apply_meters();")
            .expect("계측을 안 싣는다");
        assert!(
            apply < flush.find("self.run_pending_syncs();").unwrap_or(0),
            "자동 현행화를 연 뒤에 차단을 반영한다"
        );
        assert!(
            prod.contains("self.meters.start();"),
            "측정 스레드를 안 띄운다"
        );
        let src = include_str!("rate_meter.rs");
        let body = src
            .split("fn apply_meters(")
            .nth(1)
            .and_then(|s| s.split("\n    }\n").next())
            .expect("apply_meters 가 없다");
        assert!(body.contains("self.meters.watch(profiles.clone());"));
        assert!(
            body.contains(".note_observed("),
            "다른 세션의 결과를 차단기에 안 싣는다"
        );
    }

    #[test]
    fn the_test_clock_round_trips() {
        assert_eq!(
            crate::adapters::driven::store::budget::epoch(&chrono_like(NOW)),
            Some(NOW)
        );
        assert_eq!(
            crate::adapters::driven::store::budget::epoch(&ts(NOW)),
            Some(NOW)
        );
    }
}
