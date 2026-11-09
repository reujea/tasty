//! API 세션의 **잔액 대비 사용** — 상태줄의 `Total $쓴 것 / $한도`.
//!
//! # 왜 따로 세나
//!
//! 5시간·7일 사용률(`rate_limit_event`)은 claude.ai 로그인 세션에만 온다(실측
//! 2026-10-13: API 키 · `ANTHROPIC_AUTH_TOKEN` 프로필 셋 모두 0줄, 로그인 프로필은 매번
//! 1줄). API 로 쓰는 세션에서 그 자리를 비워 두는 대신, 사용자가 쓰는 claude 상태줄
//! (claudep 의 statusline)과 **같은 규칙**으로 그 세션이 지금 5시간 블록에 쓴 비용을
//! 잔액 한도와 나란히 보인다. 같은 규칙이라야 두 화면의 숫자가 맞는다.
//!
//! # 규칙 (statusline `blocks.rs` · `pricing.rs` · `rate_limits.rs` 와 같다)
//!
//! - **API 인가**: agent 프로필이 `ANTHROPIC_API_KEY` 또는 `ANTHROPIC_AUTH_TOKEN` 을
//!   준다 — 데몬의 agent 환경이나 그 프로필 `settings.json`(최상위 또는 `env`)에 그
//!   **이름이 있는지만** 본다. 값은 꺼내지 않고 어디에도 남기지 않는다.
//! - **어느 기록인가**: 그 세션 하나의 대화 기록
//!   (`<CLAUDE_CONFIG_DIR>/projects/*/<agent_session_id>.jsonl`). 프로필 전체가 아니다.
//! - **블록**: 첫 assistant 줄의 시각부터 정확히 5시간. 그 끝 뒤에 온 줄이 새 블록을
//!   연다. 지금이 마지막 블록 안이면 그 블록이 활성이고, 아니면 활성 블록이 없다(`-`).
//! - **비용**: assistant 줄마다 `message.usage` 의 네 토큰 × 모델 단가. statusline 처럼
//!   같은 메시지가 여러 줄로 적혀도 **중복을 걸러내지 않는다** — 숫자를 맞추려는 것이다.
//!   청구액이 아니라 추정이다.
//! - **한도**: 환경변수 `CLAUDEP_API_BALANCE`, 없거나 숫자가 아니면 50.
//!
//! # 어디서 도나
//!
//! 기록이 수십 MB 라 plugin 루프에서 읽지 않는다. 세션마다 있는 구독 워커가
//! [`Tracker`] 를 들고, 턴이 끝날 때마다 **덧붙은 만큼만** 읽어 값을 이벤트에 싣는다.

use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const BLOCK_SECS: i64 = 5 * 3600;
pub const BALANCE_ENV: &str = "CLAUDEP_API_BALANCE";
pub const DEFAULT_BALANCE: f64 = 50.0;
const AUTH_KEYS: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

/// 상태줄에 실을 값. `spent` 가 `None` 이면 지금 활성 블록이 없다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    pub spent: Option<f64>,
    pub balance: f64,
}

/// 잔액 한도 — statusline 과 같은 환경변수를 읽는다.
pub fn balance() -> f64 {
    balance_from(std::env::var(BALANCE_ENV).ok().as_deref())
}

fn balance_from(v: Option<&str>) -> f64 {
    v.and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(DEFAULT_BALANCE)
}

/// 이 agent 프로필이 API 로 인증하나 — 이름만 본다.
pub fn is_api(agent_env: &BTreeMap<String, String>, config_dir: &Path) -> bool {
    if AUTH_KEYS.iter().any(|k| agent_env.contains_key(*k)) {
        return true;
    }
    let Ok(text) = std::fs::read_to_string(config_dir.join("settings.json")) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    AUTH_KEYS
        .iter()
        .any(|k| v.get(*k).is_some() || v.pointer(&format!("/env/{k}")).is_some())
}

/// agent 프로필의 설정 폴더 — 데몬이 넘기는 `CLAUDE_CONFIG_DIR`, 없으면 `~/.claude`.
pub fn config_dir(agent_env: &BTreeMap<String, String>) -> Option<PathBuf> {
    if let Some(d) = agent_env.get("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(home.join(".claude"))
}

/// 그 프로필의 계정 파일 — 설정 폴더(`CLAUDE_CONFIG_DIR`)를 준 프로필은 그 안, 안 준 프로필은
/// 설정 폴더가 아니라 **홈 바로 아래**(`~/.claude.json`)다(실측 2026-10-22, Claude Code 2.1.304).
pub fn account_file(agent_env: &BTreeMap<String, String>) -> Option<PathBuf> {
    if let Some(claude_config_dir) = agent_env.get("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(claude_config_dir).join(".claude.json"));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(home.join(".claude.json"))
}

/// 그 계정 파일에서 로그인 이메일 한 칸(`oauthAccount.emailAddress`)만 꺼낸다. API 키로만 도는
/// 프로필은 칸이 없다.
pub fn account_email(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.pointer("/oauthAccount/emailAddress")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
}

/// 세션의 대화 기록 — 프로필마다 설정 폴더가 달라 그 아래 `projects` 에서 찾는다.
pub fn find_transcript(config_dir: &Path, agent_session_id: &str) -> Option<PathBuf> {
    tasty_plugin_agent_common::transcript::find_transcript(
        &config_dir.join("projects"),
        agent_session_id,
    )
}

/// 대화 기록이 **처음** 남긴 작업 폴더 — 그 대화가 시작된 자리다.
///
/// 이어받기(`adopt`)는 작업 폴더를 안 주면 데몬의 폴더에서 돈다. 그러면 상대 경로·그 폴더의
/// 지침이 원래 대화와 달라진다(실측 2026-10-20: 홈에서 시작한 대화가 데몬 폴더에서 이어졌다).
/// 마지막이 아니라 처음을 읽는 이유: 그렇게 잘못 이어진 뒤의 줄은 틀린 폴더를 적고 있다.
pub fn transcript_cwd(path: &Path) -> Option<String> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).ok()?;
    std::io::BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .find_map(|line| {
            let v: Value = serde_json::from_str(&line).ok()?;
            v.get("cwd")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(str::to_string)
        })
}

/// 한 블록.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Block {
    start: i64,
    end: i64,
    cost: f64,
}

/// 대화 기록 한 파일을 덧붙은 만큼씩 읽어 블록을 굴린다.
#[derive(Debug, Default)]
pub struct Ledger {
    offset: u64,
    /// 줄 끝이 아직 안 온 꼬리 — 다음 읽기에서 잇는다.
    tail: Vec<u8>,
    block: Option<Block>,
}

impl Ledger {
    /// `path` 에서 지난번 뒤로 붙은 것을 읽는다. 파일이 줄었으면(교체) 처음부터 다시 센다.
    pub fn catch_up(&mut self, path: &Path) -> std::io::Result<()> {
        let mut f = std::fs::File::open(path)?;
        let len = f.metadata()?.len();
        if len < self.offset {
            *self = Ledger::default();
        }
        f.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        self.offset += buf.len() as u64;
        self.feed(&buf);
        Ok(())
    }

    /// 바이트를 먹인다 — 줄 끝까지 온 줄만 센다.
    fn feed(&mut self, bytes: &[u8]) {
        self.tail.extend_from_slice(bytes);
        let Some(last) = self.tail.iter().rposition(|b| *b == b'\n') else {
            return;
        };
        let done: Vec<u8> = self.tail.drain(..=last).collect();
        for line in String::from_utf8_lossy(&done).lines() {
            self.line(line);
        }
    }

    fn line(&mut self, line: &str) {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
            return;
        };
        if v.get("type").and_then(Value::as_str) != Some("assistant") {
            return;
        }
        let Some(ts) = v.get("timestamp").and_then(Value::as_str).and_then(epoch) else {
            return;
        };
        let Some(msg) = v.get("message") else {
            return;
        };
        if self.block.is_none_or(|b| ts >= b.end) {
            self.block = Some(Block {
                start: ts,
                end: ts + BLOCK_SECS,
                cost: 0.0,
            });
        }
        let model = msg.get("model").and_then(Value::as_str).unwrap_or_default();
        if let (Some(b), Some(cost)) = (self.block.as_mut(), cost_of(model, msg.get("usage"))) {
            b.cost += cost;
        }
    }

    /// 지금(`now`, 유닉스 초) 활성 블록의 비용 — 없으면 `None`.
    pub fn spent(&self, now: i64) -> Option<f64> {
        self.block
            .filter(|b| b.start <= now && now < b.end)
            .map(|b| b.cost)
    }
}

/// 백만 토큰당 단가(입력 · 출력 · 캐시 쓰기 5분 · 캐시 읽기). statusline `pricing.rs` 와
/// 같은 표다 — 계열의 최신 단가, 다른 값을 받는 옛 판만 따로. 모르는 모델은 `None`
/// (어림하지 않는다).
fn price(model: &str) -> Option<[f64; 4]> {
    const OLD: [(&str, [f64; 4]); 4] = [
        ("opus-4-1", [15.0, 75.0, 18.75, 1.50]),
        ("opus-4-0", [15.0, 75.0, 18.75, 1.50]),
        ("opus-4-20", [15.0, 75.0, 18.75, 1.50]),
        ("3-5-haiku", [0.80, 4.0, 1.0, 0.08]),
    ];
    let id = model.to_ascii_lowercase();
    if let Some((_, p)) = OLD.iter().find(|(needle, _)| id.contains(needle)) {
        return Some(*p);
    }
    if id.contains("opus") {
        Some([5.0, 25.0, 6.25, 0.50])
    } else if id.contains("sonnet") {
        Some([3.0, 15.0, 3.75, 0.30])
    } else if id.contains("haiku") {
        Some([1.0, 5.0, 1.25, 0.10])
    } else {
        None
    }
}

fn cost_of(model: &str, usage: Option<&Value>) -> Option<f64> {
    let p = price(model)?;
    let n = |k: &str| {
        usage
            .and_then(|u| u.get(k))
            .and_then(Value::as_u64)
            .unwrap_or(0) as f64
    };
    Some(
        (n("input_tokens") * p[0]
            + n("output_tokens") * p[1]
            + n("cache_creation_input_tokens") * p[2]
            + n("cache_read_input_tokens") * p[3])
            / 1_000_000.0,
    )
}

/// RFC 3339 시각(`2026-10-14T00:48:49.051Z` · `+09:00` 꼴) → 유닉스 초. 소수 초는 버린다.
pub(crate) fn epoch(s: &str) -> Option<i64> {
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if s.get(4..5) != Some("-") || s.get(10..11).is_none_or(|t| t != "T" && t != " ") {
        return None;
    }
    // 시간대: 끝의 `Z` 또는 `±HH:MM`.
    let offset = if s.ends_with('Z') || s.ends_with('z') {
        0
    } else {
        let tz = s.get(s.len().checked_sub(6)?..)?;
        let sign = match &tz[..1] {
            "+" => 1,
            "-" => -1,
            _ => return None,
        };
        sign * (tz.get(1..3)?.parse::<i64>().ok()? * 3600 + tz.get(4..6)?.parse::<i64>().ok()? * 60)
    };
    // 그레고리력 날짜 → 1970-01-01 부터의 날 수(Howard Hinnant 의 days_from_civil).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + se - offset)
}

/// 지금 시각(유닉스 초).
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 한 세션의 출처 — 그 agent 프로필의 환경 · 설정 폴더 · 대화 기록.
pub struct Source {
    pub env: BTreeMap<String, String>,
    pub config_dir: PathBuf,
    /// 그 세션의 대화 기록. 아직 없거나(첫 턴 전) claude 가 아니면 `None`.
    pub transcript: Option<PathBuf>,
}

impl Source {
    /// 세션과 agent 명부로 찾는다. 세션이나 프로필이 없으면 `None`.
    pub fn resolve(sup: &impl crate::ports::supervisor::Supervisor, session: &str) -> Option<Self> {
        let status = sup.status(session).ok()?;
        let agent = status.get("agent_name").and_then(Value::as_str)?;
        let profile = sup
            .agent_list()
            .ok()?
            .into_iter()
            .find(|p| p.name == agent)?;
        let config_dir = config_dir(&profile.env)?;
        let transcript = status
            .get("agent_session_id")
            .and_then(Value::as_str)
            .and_then(|id| find_transcript(&config_dir, id));
        Some(Source {
            env: profile.env,
            config_dir,
            transcript,
        })
    }
}

/// 구독 워커가 드는 것 — 한 세션의 출처와 장부.
pub struct Tracker {
    transcript: Option<PathBuf>,
    ledger: Ledger,
}

impl Tracker {
    /// 이 세션이 API 로 도는지 세션 기록과 프로필로 알아낸다. API 가 아니거나 알 수 없으면
    /// `None` — 상태줄은 그때 원문 줄만으로 그린다.
    pub fn resolve(sup: &impl crate::ports::supervisor::Supervisor, session: &str) -> Option<Self> {
        let source = Source::resolve(sup, session)?;
        if !is_api(&source.env, &source.config_dir) {
            return None;
        }
        Some(Tracker {
            transcript: source.transcript,
            ledger: Ledger::default(),
        })
    }

    /// 덧붙은 기록을 읽고 지금 값을 낸다. 기록을 못 찾았으면 쓴 값은 `-` 다.
    pub fn budget(&mut self) -> Budget {
        let spent = self.transcript.as_ref().and_then(|p| {
            if let Err(e) = self.ledger.catch_up(p) {
                tracing::debug!("대화 기록을 못 읽었다({}): {e}", p.display());
            }
            self.ledger.spent(now())
        });
        Budget {
            spent,
            balance: balance(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ts: &str, model: &str, inp: u64, out: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"model":"{model}","usage":{{"input_tokens":{inp},"output_tokens":{out},"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#
        )
    }

    #[test]
    fn rfc3339_becomes_unix_seconds() {
        assert_eq!(epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch("2026-10-14T00:48:49.051Z"), Some(1_791_938_929));
        assert_eq!(epoch("2026-10-14T09:48:49+09:00"), Some(1_791_938_929));
        assert_eq!(epoch("2024-02-29T12:00:00Z"), Some(1_709_208_000));
        assert_eq!(epoch("not a time"), None);
    }

    /// 계정은 `oauthAccount.emailAddress` 에서 — 칸이 없거나 비면(API 키 프로필) 없다.
    #[test]
    fn the_account_email_comes_from_the_profile_settings() {
        // 이유: 이 경로는 이 시험 한 곳에서 한 번만 만들고 끝에 지운다 — 재호출이 없다.
        let dir = std::env::temp_dir().join(format!("sup-email-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let env: BTreeMap<String, String> =
            [("CLAUDE_CONFIG_DIR".to_string(), dir.display().to_string())].into();
        let f = account_file(&env).unwrap();
        assert_eq!(f.parent(), Some(dir.as_path()));
        std::fs::write(
            &f,
            r#"{"oauthAccount":{"emailAddress":" me@example.com "}}"#,
        )
        .unwrap();
        assert_eq!(account_email(&f).as_deref(), Some("me@example.com"));
        std::fs::write(&f, r#"{"oauthAccount":{"emailAddress":""}}"#).unwrap();
        assert_eq!(account_email(&f), None);
        std::fs::write(&f, r#"{"primaryApiKey":"x"}"#).unwrap();
        assert_eq!(account_email(&f), None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(account_email(&f), None);
        // 설정 폴더를 안 준 프로필은 설정 폴더 안이 아니라 홈 바로 아래다.
        if let Some(home) = std::env::var_os("HOME") {
            let f = account_file(&BTreeMap::new()).unwrap();
            assert_eq!(f.parent(), Some(std::path::Path::new(&home)));
        }
    }

    /// 블록은 첫 줄 시각부터 5시간이고, 그 뒤에 온 줄이 새 블록을 연다.
    #[test]
    fn a_block_runs_five_hours_from_its_first_line() {
        let mut l = Ledger::default();
        let text = [
            line("2026-10-14T00:00:00Z", "claude-opus-5", 1_000_000, 0),
            line("2026-10-14T04:59:59Z", "claude-opus-5", 0, 1_000_000),
            line("2026-10-14T05:00:00Z", "claude-haiku-4-5", 1_000_000, 0),
            String::new(),
        ]
        .join("\n");
        l.feed(text.as_bytes());
        let t = |s| epoch(s).unwrap();
        // 셋째 줄이 새 블록을 열었다 — 앞 블록의 $5 + $25 는 지금 값에 안 들어간다.
        assert_eq!(l.spent(t("2026-10-14T05:30:00Z")), Some(1.0));
        assert_eq!(l.spent(t("2026-10-14T10:00:00Z")), None);
    }

    /// 줄 끝이 안 온 꼬리는 다음 읽기와 이어 붙여 센다 — 반쪽 줄을 버리지 않는다.
    #[test]
    fn a_half_written_line_is_kept_for_the_next_read() {
        let first = format!(
            "{}\n",
            line("2026-10-14T00:00:00Z", "claude-sonnet-5", 1_000_000, 0)
        );
        let second = format!(
            "{}\n",
            line("2026-10-14T00:00:30Z", "claude-sonnet-5", 1_000_000, 0)
        );
        // 한 번에 온 조각이 온전한 줄 하나 + 다음 줄의 앞부분이다.
        let (a, b) = second.split_at(40);
        let at = epoch("2026-10-14T00:01:00Z").unwrap();
        let mut l = Ledger::default();
        l.feed(format!("{first}{a}").as_bytes());
        assert_eq!(l.spent(at), Some(3.0));
        l.feed(b.as_bytes());
        assert_eq!(l.spent(at), Some(6.0));
    }

    /// 모르는 모델은 어림하지 않는다 — 블록은 열되 비용은 더하지 않는다.
    #[test]
    fn an_unknown_model_adds_nothing() {
        assert_eq!(price("gpt-x"), None);
        assert_eq!(price("claude-opus-4-1-20250805").map(|p| p[0]), Some(15.0));
        assert_eq!(price("claude-opus-5").map(|p| p[0]), Some(5.0));
    }

    #[test]
    fn the_balance_defaults_to_fifty() {
        assert_eq!(balance_from(None), 50.0);
        assert_eq!(balance_from(Some("abc")), 50.0);
        assert_eq!(balance_from(Some(" 120.5 ")), 120.5);
    }

    /// 이름만 본다 — 데몬 환경 또는 settings.json 의 최상위·`env`.
    #[test]
    fn api_is_judged_by_key_names() {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("sup-budget-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let none = BTreeMap::new();
        assert!(!is_api(&none, &dir));
        std::fs::write(
            dir.join("settings.json"),
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"x"}}"#,
        )
        .unwrap();
        assert!(is_api(&none, &dir));
        std::fs::write(dir.join("settings.json"), r#"{"env":{"OTHER":"x"}}"#).unwrap();
        assert!(!is_api(&none, &dir));
        let mut env = BTreeMap::new();
        env.insert("ANTHROPIC_API_KEY".to_string(), "x".to_string());
        assert!(is_api(&env, &dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 대화 기록의 **처음** 작업 폴더를 읽는다 — 빈 값·없는 줄은 건너뛰고, 뒤에 다른
    /// 폴더가 적혀도(잘못 이어진 뒤의 줄) 처음 것을 낸다.
    #[test]
    fn the_transcript_cwd_is_where_the_conversation_began() {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("sup-cwd-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        std::fs::write(
            &path,
            "{\"type\":\"summary\"}\nnot json\n{\"cwd\":\"\"}\n{\"cwd\":\"/Users/me\"}\n{\"cwd\":\"/daemon\"}\n",
        )
        .unwrap();
        assert_eq!(transcript_cwd(&path).as_deref(), Some("/Users/me"));
        assert_eq!(transcript_cwd(&dir.join("없음.jsonl")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
