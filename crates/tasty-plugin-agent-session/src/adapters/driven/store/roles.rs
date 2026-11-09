//! 역할 폴더 명부 — 세션을 띄울 때 여러 개를 골라 얹는 지침 묶음.
//!
//! 역할 폴더는 `CLAUDE.md`(지침) 를 가진 폴더 하나다. 명부는 `$TASTY_PLUGIN_DATA_DIR/roles.json`
//! 한 파일이고 이름 → 경로를 적는다.
//!
//! ```json
//! { "roles": [ { "name": "builder", "path": "/home/me/roles/builder" } ] }
//! ```
//!
//! 얹는 법은 agent 종류마다 다르다(`local::build_args`):
//!
//! | 종류 | 역할 폴더 하나당 |
//! |---|---|
//! | claude | `--add-dir <폴더>` + 환경 `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`(추가 폴더의 `CLAUDE.md` 를 읽게 한다) |
//! | pi | `--append-system-prompt <폴더>/CLAUDE.md`(pi 는 추가 폴더가 없다) |
//!
//! 프로필은 세션 하나에 하나만 고르고, 역할은 여럿을 고른다.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// 역할의 지침 파일 이름.
pub const GUIDE: &str = "CLAUDE.md";

/// claude 가 `--add-dir` 폴더의 `CLAUDE.md` 를 읽게 하는 환경.
pub const CLAUDE_ADD_DIR_GUIDE_ENV: &str = "CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    pub path: String,
}

impl Role {
    pub fn to_json(&self) -> Value {
        json!({ "name": self.name, "path": self.path, "guide": self.guide().is_some() })
    }

    /// 지침 파일 — 없으면 `None`.
    pub fn guide(&self) -> Option<PathBuf> {
        let p = Path::new(&self.path).join(GUIDE);
        p.is_file().then_some(p)
    }
}

/// 명부 파일 — `$TASTY_PLUGIN_DATA_DIR/roles.json`.
pub fn default_file() -> Option<PathBuf> {
    std::env::var_os("TASTY_PLUGIN_DATA_DIR").map(|d| PathBuf::from(d).join("roles.json"))
}

/// 명부를 읽는다 — 파일이 없으면 빈 명부, 못 읽으면 로그를 남기고 빈 명부다.
pub fn load(file: Option<&Path>) -> Vec<Role> {
    let Some(file) = file else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("역할 명부를 못 읽었다({}): {e}", file.display());
            return Vec::new();
        }
    };
    let mut out: Vec<Role> = v
        .get("roles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let s = |k: &str| r.get(k).and_then(Value::as_str).map(str::to_string);
            Some(Role {
                name: s("name")?,
                path: s("path")?,
            })
        })
        .filter(|r| crate::adapters::driven::store::agents::valid_name(&r.name))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn store(file: &Path, roles: &[Role]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = json!({ "roles": roles.iter().map(|r| json!({"name": r.name, "path": r.path})).collect::<Vec<_>>() });
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, body.to_string())
        .and_then(|_| std::fs::rename(&tmp, file))
        .map_err(|e| e.to_string())
}

/// 역할을 더하거나 바꾼다 — 폴더가 실제로 있어야 하고 경로는 절대 경로로 적는다.
pub fn add(file: &Path, name: &str, path: &str) -> Result<Role, String> {
    if !crate::adapters::driven::store::agents::valid_name(name) {
        return Err(crate::domain::i18n::t1("sup.profile.bad_name", name));
    }
    let abs = std::fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
    if !abs.is_dir() {
        return Err(crate::domain::i18n::t1("sup.role.not_dir", path));
    }
    let role = Role {
        name: name.to_string(),
        path: abs.display().to_string(),
    };
    let mut roles = load(Some(file));
    roles.retain(|r| r.name != name);
    roles.push(role.clone());
    roles.sort_by(|a, b| a.name.cmp(&b.name));
    store(file, &roles)?;
    Ok(role)
}

/// 폴더 아래에서 `CLAUDE.md` 를 가진 하위 폴더를 모두 역할로 더한다(이름 = 폴더 이름).
/// 이름 규칙에 안 맞는 폴더는 건너뛴다. 더한 역할을 돌려준다.
pub fn scan(file: &Path, parent: &str) -> Result<Vec<Role>, String> {
    let entries = std::fs::read_dir(parent).map_err(|e| format!("{parent}: {e}"))?;
    let mut found: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(GUIDE).is_file())
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            crate::adapters::driven::store::agents::valid_name(&name).then_some((name, p))
        })
        .collect();
    found.sort();
    let mut added = Vec::new();
    for (name, p) in found {
        added.push(add(file, &name, &p.display().to_string())?);
    }
    Ok(added)
}

/// 역할을 지운다 — 없으면 오류다.
pub fn remove(file: &Path, name: &str) -> Result<(), String> {
    let mut roles = load(Some(file));
    let before = roles.len();
    roles.retain(|r| r.name != name);
    if roles.len() == before {
        return Err(crate::domain::i18n::t1("sup.role.unknown", name));
    }
    store(file, &roles)
}

/// 요청 칸의 역할 이름 — 배열이거나 쉼표로 나눈 글(CLI `--roles a,b`)이다. 폼 값처럼 `params.params`
/// 아래에 와도 본다. 빈 이름은 뺀다.
pub fn names_of(params: &Value) -> Vec<String> {
    let v = params
        .pointer("/params/roles")
        .or_else(|| params.get("roles"));
    let names: Vec<String> = match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(s)) => s.split(',').map(str::to_string).collect(),
        _ => Vec::new(),
    };
    names
        .into_iter()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

/// 고른 이름을 명부에서 찾는다 — 모르는 이름은 `Err` 로 모아 낸다.
pub fn resolve<'a>(all: &'a [Role], names: &[String]) -> Result<Vec<&'a Role>, Vec<String>> {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for n in names {
        match all.iter().find(|r| &r.name == n) {
            Some(r) => found.push(r),
            None => missing.push(n.clone()),
        }
    }
    if missing.is_empty() {
        Ok(found)
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!(
            "agent-session-roles-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn role_dir(parent: &Path, name: &str, guide: bool) -> PathBuf {
        let d = parent.join(name);
        std::fs::create_dir_all(&d).unwrap();
        if guide {
            std::fs::write(d.join(GUIDE), "# role").unwrap();
        }
        d
    }

    #[test]
    fn roles_are_added_listed_resolved_and_removed() {
        let d = tmp("cycle");
        let file = d.join("roles.json");
        let b = role_dir(&d, "builder", true);
        let t = role_dir(&d, "tester", false);
        add(&file, "builder", b.to_str().unwrap()).unwrap();
        add(&file, "tester", t.to_str().unwrap()).unwrap();
        let all = load(Some(&file));
        assert_eq!(
            all.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["builder", "tester"]
        );
        assert!(all[0].guide().is_some() && all[1].guide().is_none());
        let picked = resolve(&all, &["tester".into(), "builder".into()]).unwrap();
        assert_eq!(picked[0].name, "tester", "고른 순서를 지킨다");
        assert_eq!(
            resolve(&all, &["nope".into(), "builder".into()]).unwrap_err(),
            vec!["nope".to_string()]
        );
        remove(&file, "tester").unwrap();
        assert_eq!(load(Some(&file)).len(), 1);
        assert!(remove(&file, "tester").is_err(), "없는 역할은 오류다");
    }

    #[test]
    fn a_missing_folder_or_a_bad_name_is_refused() {
        let d = tmp("refuse");
        let file = d.join("roles.json");
        assert!(add(&file, "x", d.join("absent").to_str().unwrap()).is_err());
        assert!(add(&file, "../x", d.to_str().unwrap()).is_err());
        assert!(load(Some(&file)).is_empty());
    }

    #[test]
    fn names_come_from_an_array_or_a_comma_list() {
        assert_eq!(
            names_of(&json!({"params":{"roles":["a","b"]}})),
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(
            names_of(&json!({"roles":" a, ,b "})),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(names_of(&json!({})).is_empty());
    }

    /// 템플릿 폴더 하나로 역할을 한꺼번에 더한다 — `CLAUDE.md` 없는 폴더는 역할이 아니다.
    #[test]
    fn scanning_a_parent_adds_every_folder_with_a_guide() {
        let d = tmp("scan");
        let parent = d.join("_role_template");
        role_dir(&parent, "builder", true);
        role_dir(&parent, "legal", true);
        role_dir(&parent, "notes", false);
        let file = d.join("roles.json");
        let added = scan(&file, parent.to_str().unwrap()).unwrap();
        assert_eq!(
            added.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["builder", "legal"]
        );
    }
}
