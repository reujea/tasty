//! agent 프로필 — 세션을 띄우는 방법 한 벌.
//!
//! 프로필은 `$TASTY_PLUGIN_DATA_DIR/profiles/<이름>.json` 파일 하나다. 파일 이름이 프로필
//! 이름이고, 칸은 전부 생략할 수 있다.
//!
//! ```json
//! {
//!   "agent_type": "claude",
//!   "command": "claude",
//!   "cwd": "/work",
//!   "env": { "CLAUDE_CONFIG_DIR": "/home/me/.claude-work" },
//!   "model": "opus",
//!   "permission_mode": "acceptEdits",
//!   "settings": { "includeCoAuthoredBy": false },
//!   "add_dirs": ["/shared"],
//!   "args": [],
//!   "cache_ttl": "1h"
//! }
//! ```
//!
//! `agent_type` 은 [`AGENT_TYPES`] 중 하나다(`claude` · `pi`). 옛 파일의 `kind` 도 같은 칸으로 읽는다.
//! `command` 를 비우면 종류 이름(`claude` · `pi`)을 띄운다. pi 프로필은 보통 `env` 에
//! `PI_CODING_AGENT_DIR`(pi 설정 폴더)를 둔다. `permission_mode` · `settings` · `add_dirs` 는 claude 만 쓴다.
//!
//! `cache_ttl`(`5m` · `1h`)은 그 프로필 세션의 프롬프트 캐시 수명이다. 탭이 턴에서 캐시 종류를 아직
//! 못 본 때(비우기 직후 · plugin 재기동 직후) 유휴 판정이 이 값을 쓴다 — 관측한 값이 늘 먼저다.
//!
//! 폴더가 비었으면 [`DEFAULT_NAME`] 하나를 낸다 — 설정 없이도 `claude` 를 띄울 수 있어야 한다.
//! 목록은 이름 순이다.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// 폴더가 비었을 때 내는 프로필 이름.
pub const DEFAULT_NAME: &str = "claude";

/// 띄울 수 있는 agent 종류 — 실행기(`local`)가 종류마다 명령줄과 줄 형식을 안다.
pub const AGENT_TYPES: &[&str] = &["claude", "pi"];

/// 이 종류를 띄울 수 있나.
pub fn supported(agent_type: &str) -> bool {
    AGENT_TYPES.contains(&agent_type)
}

/// agent 프로필 하나.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgentProfile {
    pub name: String,
    /// agent 종류(`agent_type`) — [`AGENT_TYPES`] 중 하나.
    pub kind: String,
    /// 실행 파일과 앞붙이 인자 — 공백으로 나눈다. 비면 종류 이름.
    pub command: String,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    /// `--settings` 로 넘길 JSON.
    pub settings: Option<Value>,
    pub add_dirs: Vec<String>,
    /// 맨 뒤에 붙일 인자.
    pub args: Vec<String>,
    /// 프롬프트 캐시 수명 — 탭이 아직 관측하지 못한 때 유휴 판정이 쓴다.
    pub cache_ttl: Option<CacheTtl>,
}

/// 프로필이 밝힌 프롬프트 캐시 수명.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheTtl {
    FiveMin,
    OneHour,
}

impl CacheTtl {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "5m" => Some(Self::FiveMin),
            "1h" => Some(Self::OneHour),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FiveMin => "5m",
            Self::OneHour => "1h",
        }
    }

    pub fn is_1h(self) -> bool {
        self == Self::OneHour
    }
}

impl AgentProfile {
    /// 설정 없이 쓰는 기본 프로필.
    pub fn default_claude() -> Self {
        Self {
            name: DEFAULT_NAME.into(),
            kind: "claude".into(),
            command: "claude".into(),
            ..Self::default()
        }
    }

    /// 파일 본문을 읽는다 — 이름은 파일 이름에서 온다.
    pub fn from_json(name: &str, v: &Value) -> Result<Self, String> {
        let obj = v
            .as_object()
            .ok_or_else(|| crate::domain::i18n::t1("sup.profile.not_object", name))?;
        let s = |k: &str| obj.get(k).and_then(Value::as_str).map(str::to_string);
        let list = |k: &str| -> Vec<String> {
            obj.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let env = obj
            .get("env")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let cache_ttl = match s("cache_ttl") {
            None => None,
            Some(t) => Some(
                CacheTtl::parse(&t)
                    .ok_or_else(|| crate::domain::i18n::t1("sup.profile.bad_cache_ttl", &t))?,
            ),
        };
        let kind = s("agent_type")
            .or_else(|| s("kind"))
            .unwrap_or_else(|| "claude".into());
        Ok(Self {
            name: name.to_string(),
            command: s("command")
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| kind.clone()),
            kind,
            cwd: s("cwd"),
            env,
            model: s("model"),
            permission_mode: s("permission_mode"),
            settings: obj.get("settings").filter(|v| !v.is_null()).cloned(),
            add_dirs: list("add_dirs"),
            args: list("args"),
            cache_ttl,
        })
    }

    /// 파일에 쓸 본문 — 이름은 빼고 비지 않은 칸만 싣는다.
    pub fn to_json(&self) -> Value {
        let mut v = json!({ "agent_type": self.kind, "command": self.command });
        let o = v.as_object_mut().expect("json! 객체");
        for (k, val) in [
            ("cwd", &self.cwd),
            ("model", &self.model),
            ("permission_mode", &self.permission_mode),
        ] {
            if let Some(s) = val {
                o.insert(k.into(), Value::String(s.clone()));
            }
        }
        if !self.env.is_empty() {
            o.insert("env".into(), json!(self.env));
        }
        if let Some(s) = &self.settings {
            o.insert("settings".into(), s.clone());
        }
        if !self.add_dirs.is_empty() {
            o.insert("add_dirs".into(), json!(self.add_dirs));
        }
        if !self.args.is_empty() {
            o.insert("args".into(), json!(self.args));
        }
        if let Some(t) = self.cache_ttl {
            o.insert("cache_ttl".into(), json!(t.as_str()));
        }
        v
    }
}

/// 프로필 이름 규칙 — 파일 이름이 되므로 경로 문자를 막는다.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.starts_with('.')
}

/// 프로필 폴더 — `$TASTY_PLUGIN_DATA_DIR/profiles`.
pub fn default_dir() -> Option<PathBuf> {
    std::env::var_os("TASTY_PLUGIN_DATA_DIR").map(|d| PathBuf::from(d).join("profiles"))
}

/// 폴더를 읽는다. 못 읽는 파일은 건너뛰고 로그를 남긴다 — 하나가 깨져도 나머지는 쓴다.
pub fn load(dir: Option<&Path>) -> Vec<AgentProfile> {
    let mut out = Vec::new();
    if let Some(dir) = dir
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !valid_name(name) {
                continue;
            }
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
                .and_then(|v| AgentProfile::from_json(name, &v));
            match parsed {
                Ok(p) => out.push(p),
                Err(why) => tracing::warn!("프로필을 못 읽었다({}): {why}", path.display()),
            }
        }
    }
    if out.is_empty() {
        out.push(AgentProfile::default_claude());
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 프로필을 쓴다 — 같은 이름이 있으면 바꾼다.
pub fn save(dir: &Path, p: &AgentProfile) -> Result<PathBuf, String> {
    if !valid_name(&p.name) {
        return Err(crate::domain::i18n::t1("sup.profile.bad_name", &p.name));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", p.name));
    let body = serde_json::to_string_pretty(&p.to_json()).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(path)
}

/// 프로필을 지운다 — 없으면 오류다(이름을 잘못 친 것을 조용히 넘기지 않는다).
pub fn remove(dir: &Path, name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(crate::domain::i18n::t1("sup.profile.bad_name", name));
    }
    std::fs::remove_file(dir.join(format!("{name}.json")))
        .map_err(|e| crate::domain::i18n::tn("sup.profile.remove_failed", &[name, &e.to_string()]))
}

/// 주어진 kind 의 프로필만.
pub fn of_kind<'a>(all: &'a [AgentProfile], kind: &str) -> Vec<&'a AgentProfile> {
    all.iter().filter(|a| a.kind == kind).collect()
}

/// 명부에 나온 kind — 상수가 아니라 명부에서 온다.
pub fn kinds(all: &[AgentProfile]) -> Vec<String> {
    let mut v: Vec<String> = all.iter().map(|a| a.kind.clone()).collect();
    v.sort();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, kind: &str) -> AgentProfile {
        AgentProfile {
            name: name.into(),
            kind: kind.into(),
            ..AgentProfile::default()
        }
    }

    fn tmp(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!(
            "agent-session-profiles-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_cache_ttl_round_trips_and_rejects_other_values() {
        let got = AgentProfile::from_json("dev", &json!({ "cache_ttl": "1h" })).unwrap();
        assert_eq!(got.cache_ttl, Some(CacheTtl::OneHour));
        assert_eq!(got.to_json()["cache_ttl"], "1h");
        let none = AgentProfile::from_json("dev", &json!({})).unwrap();
        assert_eq!(none.cache_ttl, None);
        assert!(none.to_json().get("cache_ttl").is_none());
        assert!(AgentProfile::from_json("dev", &json!({ "cache_ttl": "2h" })).is_err());
        // 저장한 파일을 다시 읽어도 같다.
        let dir = tmp("ttl");
        save(&dir, &got).unwrap();
        let back = load(Some(&dir));
        assert_eq!(back[0].cache_ttl, Some(CacheTtl::OneHour));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn profiles_are_filtered_by_kind() {
        let all = vec![p("cc", "claude"), p("pi-dev", "pi"), p("cc-sdk", "claude")];
        let got: Vec<&str> = of_kind(&all, "claude")
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(got, vec!["cc", "cc-sdk"]);
        assert_eq!(of_kind(&all, "pi").len(), 1);
    }

    #[test]
    fn kinds_come_from_the_roster_not_a_constant() {
        let all = vec![p("a", "claude"), p("b", "pi"), p("c", "claude")];
        assert_eq!(kinds(&all), vec!["claude".to_string(), "pi".to_string()]);
    }

    #[test]
    fn an_empty_folder_still_offers_the_default_profile() {
        let d = tmp("empty");
        let all = load(Some(&d));
        assert_eq!(all, vec![AgentProfile::default_claude()]);
        assert_eq!(load(None), vec![AgentProfile::default_claude()]);
    }

    #[test]
    fn saved_profiles_load_back_sorted_and_the_default_steps_aside() {
        let d = tmp("save");
        let mut work = AgentProfile::default_claude();
        work.name = "work".into();
        work.env.insert("CLAUDE_CONFIG_DIR".into(), "/c".into());
        work.settings = Some(json!({"model":"x"}));
        work.add_dirs = vec!["/shared".into()];
        save(&d, &work).unwrap();
        let mut alt = AgentProfile::default_claude();
        alt.name = "alt".into();
        save(&d, &alt).unwrap();
        let all = load(Some(&d));
        assert_eq!(
            all.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["alt", "work"]
        );
        assert_eq!(all[1], work);
        remove(&d, "alt").unwrap();
        assert_eq!(load(Some(&d)).len(), 1);
        assert!(remove(&d, "alt").is_err(), "없는 프로필 지우기는 오류다");
    }

    #[test]
    fn a_broken_file_does_not_hide_the_others() {
        let d = tmp("broken");
        std::fs::write(d.join("bad.json"), "{ not json").unwrap();
        std::fs::write(d.join("good.json"), r#"{"model":"m"}"#).unwrap();
        let all = load(Some(&d));
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "good");
        assert_eq!(all[0].model.as_deref(), Some("m"));
        assert_eq!(all[0].kind, "claude", "kind 를 생략하면 claude");
    }

    #[test]
    fn names_that_would_escape_the_folder_are_refused() {
        for bad in ["", "../x", "a/b", ".hidden", "a b"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        for good in ["claude", "work-2", "a_b.c"] {
            assert!(valid_name(good), "{good:?}");
        }
        let d = tmp("refuse");
        let mut bad = AgentProfile::default_claude();
        bad.name = "../escape".into();
        assert!(save(&d, &bad).is_err());
    }

    /// 종류는 `agent_type` 칸이다 — 옛 `kind` 도 읽고, 쓸 때는 `agent_type` 으로 쓴다. 명령을 비우면
    /// 종류 이름을 띄운다.
    #[test]
    fn the_agent_type_field_is_read_written_and_picks_the_command() {
        let pi = AgentProfile::from_json(
            "pi-dev",
            &json!({"agent_type":"pi","env":{"PI_CODING_AGENT_DIR":"/p"}}),
        )
        .unwrap();
        assert_eq!((pi.kind.as_str(), pi.command.as_str()), ("pi", "pi"));
        assert_eq!(pi.to_json()["agent_type"], "pi");
        assert!(pi.to_json().get("kind").is_none());
        let old = AgentProfile::from_json("cc", &json!({"kind":"claude","command":""})).unwrap();
        assert_eq!(
            (old.kind.as_str(), old.command.as_str()),
            ("claude", "claude")
        );
        assert!(supported("claude") && supported("pi") && !supported("codex"));
    }
}
