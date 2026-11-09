//! 대화 탭 `/` 목록에 더할 **스킬** — 사용자 · 플러그인 · 프로젝트 스킬을 파일로 찾는다.
//!
//! # 왜 파일을 훑나
//!
//! claude 의 `system/init` 은 스킬 이름을 싣지만 **첫 입력 뒤**에 오고 설명이 없다(실측 2026-11-04,
//! claude 2.1.311). 그래서 연결하자마자 `/` 를 친 사람에게 보일 목록은 claude 가 읽는 자리를 같이 읽어
//! 만든다. 실측으로 init `skills` 와 맞춘 자리:
//!
//! | 출처 | 자리 | 이름 |
//! |---|---|---|
//! | 사용자 | `<config>/skills/*/SKILL.md` | 앞머리 `name`, 없으면 폴더 이름 |
//! | 플러그인 | `<config>/plugins/installed_plugins.json` 의 `installPath` 중 `settings.json` `enabledPlugins` 가 켠 것의 `skills/*/SKILL.md` | `<플러그인>:<스킬>` |
//! | 프로젝트 | 작업 폴더와 그 모든 상위 · 각 추가 폴더의 `.claude/skills/*/SKILL.md` | 앞머리 `name`, 없으면 폴더 이름 |
//!
//! 내장 번들 스킬(`code-review` · `simplify` 등)은 디스크에 없어 잡히지 않는다 — 싣지 않기로 한 것과 같다.
//! 앞머리에 `user-invocable: false` 가 있으면 뺀다 — claude 도 init `skills` · `slash_commands` 양쪽에서
//! 뺀다(실측 2026-11-05, 2.1.311). `disable-model-invocation: true` 는 사람이 부르는 것을 막지 않아 둔다.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

/// 설명을 한 줄로 접은 뒤 남길 글자 수.
const DESCRIPTION_MAX_CHARS: usize = 120;

/// `/` 목록의 스킬 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
}

/// 훑기 요청 — (surface, 세션, 에이전트 종류). 작업 스레드가 받는다.
pub struct Job {
    pub surface_id: u32,
    pub session: String,
    pub agent_kind: Option<String>,
}

/// 작업 스레드로 가는 길. 시험처럼 시작하지 않았으면 요청은 버려진다.
static WORKER: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<Job>>> =
    std::sync::OnceLock::new();

/// 연결마다 한 번 — 이미 요청한 (surface, 세션) 짝.
static REQUESTED: Mutex<BTreeSet<(u32, String)>> = Mutex::new(BTreeSet::new());

/// 작업 스레드를 띄운다. `run` 이 훑고 결과를 페이지에 보낸다 — 플러그인 처리 스레드를 막지 않는다.
pub fn start(run: impl Fn(Job) + Send + 'static) {
    let (tx, rx) = std::sync::mpsc::channel::<Job>();
    if WORKER.set(Mutex::new(tx)).is_err() {
        tracing::warn!("[agent_session] 스킬 작업 스레드가 이미 있다");
        return;
    }
    if let Err(e) = std::thread::Builder::new()
        .name("agent-session-skills".to_string())
        .spawn(move || {
            for job in rx {
                run(job);
            }
        })
    {
        tracing::warn!(
            "[agent_session] 스킬 작업 스레드를 못 띄웠다 — `/` 목록은 고정 커맨드만: {e}"
        );
    }
}

fn send(job: Job) {
    let Some(tx) = WORKER.get() else {
        return;
    };
    let tx = tx.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Err(e) = tx.send(job) {
        tracing::warn!("[agent_session] 스킬 훑기를 못 맡겼다: {e}");
    }
}

/// 문서를 실을 때 — 그 탭이 그 세션에 붙은 뒤 처음이면 **그 자리에서** 훑어 이 문서에 싣는다(세션 연결 · 탭
/// 복원마다 한 번).
///
/// 작업 스레드에 맡기면 결과 메시지가 막 갈아 끼우는 문서보다 먼저 닿아 사라졌다(실측 2026-11-05 VM: 훑기 0ms ·
/// 목록 빈 채). 훑기는 파일 몇십 개 읽기라 처리 스레드에서 해도 짧다 — 실측은 `scan` 의 로그. 이미 뜬 문서를
/// 바꾸는 다시 훑기([`rescan`])만 작업 스레드가 한다.
pub fn on_document<S: crate::ports::supervisor::Supervisor>(
    sup: &S,
    surface_id: u32,
    session: &str,
    agent_kind: Option<&str>,
) {
    if agent_kind == Some("pi") {
        return;
    }
    let first = REQUESTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert((surface_id, session.to_string()));
    if !first {
        return;
    }
    let started = std::time::Instant::now();
    if let Some(found) = for_session(sup, session) {
        store(session, found);
    }
    tracing::info!(
        "[agent_session] surface={surface_id} 연결 스킬 훑기 {}ms",
        started.elapsed().as_millis()
    );
}

/// 다시 훑는다 — `/reload-skills` · `/reload-plugins` · `/add-dir` 뒤.
pub fn rescan(surface_id: u32, session: &str, agent_kind: Option<&str>) {
    if agent_kind == Some("pi") {
        return;
    }
    send(Job {
        surface_id,
        session: session.to_string(),
        agent_kind: agent_kind.map(str::to_string),
    });
}

/// 탭이 닫혔다 — 다시 열리면 새 연결로 훑는다.
pub fn forget(surface_id: u32) {
    REQUESTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .retain(|(sid, _)| *sid != surface_id);
}

/// 다시 훑어야 하는 커맨드인가 — 보낸 말이 이것이면 턴 끝에 [`rescan`] 한다. `/add-dir` 는 추가 폴더의
/// 프로젝트 스킬이 바뀌어서다(그 결과도 턴 끝으로 온다).
pub fn reloads(sent: &str) -> bool {
    let s = sent.trim();
    matches!(s, "/reload-skills" | "/reload-plugins") || s.starts_with("/add-dir ")
}

/// 세션 id → 그 세션에서 찾은 스킬. 문서를 다시 실을 때 이 값을 그대로 싣는다.
static FOUND: Mutex<Option<HashMap<String, Vec<Skill>>>> = Mutex::new(None);

/// 그 세션에서 찾아 둔 스킬 — 아직 훑지 않았으면 빈 목록이다.
pub fn cached(session: &str) -> Vec<Skill> {
    FOUND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .and_then(|m| m.get(session).cloned())
        .unwrap_or_default()
}

pub fn store(session: &str, skills: Vec<Skill>) {
    FOUND
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_insert_with(Default::default)
        .insert(session.to_string(), skills);
}

/// 세션의 프로필 설정 폴더 · 작업 폴더 · 추가 폴더로 훑는다. claude 세션이 아니면 `None`.
/// 작업 폴더는 자식을 띄울 때와 같은 순서(세션 → 프로필 → 홈)로 정한다 — 세션 기록의 `cwd` 는 시작할 때
/// 폴더를 고른 경우에만 있다. 추가 폴더는 세션 것과 프로필 것을 함께 본다.
pub fn for_session<S: crate::ports::supervisor::Supervisor>(
    sup: &S,
    session: &str,
) -> Option<Vec<Skill>> {
    let st = sup.status(session).ok()?;
    let agent = st.get("agent_name").and_then(Value::as_str)?;
    let profile = sup
        .agent_list()
        .ok()?
        .into_iter()
        .find(|p| p.name == agent && p.kind == "claude")?;
    let config = crate::adapters::driven::store::budget::config_dir(&profile.env)?;
    let cwd = st
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| profile.cwd.clone())
        .filter(|c| !c.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
    let mut add_dirs: Vec<PathBuf> = st
        .get("add_dirs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default();
    add_dirs.extend(profile.add_dirs.iter().map(PathBuf::from));
    Some(scan(&config, cwd.as_deref(), &add_dirs))
}

/// 세 출처를 훑어 이름 순으로 낸다. 같은 이름은 먼저 찾은 것(사용자 → 프로젝트 → 플러그인)만 둔다.
pub fn scan(config: &Path, cwd: Option<&Path>, add_dirs: &[PathBuf]) -> Vec<Skill> {
    let mut counts = Counts::default();
    let mut found = Vec::new();
    collect(&config.join("skills"), None, &mut found, &mut counts);
    if let Some(cwd) = cwd {
        for dir in cwd.ancestors() {
            collect(&dir.join(".claude/skills"), None, &mut found, &mut counts);
        }
    }
    for dir in add_dirs {
        collect(&dir.join(".claude/skills"), None, &mut found, &mut counts);
    }
    for (plugin, root) in enabled_plugins(config, cwd) {
        collect(&root.join("skills"), Some(&plugin), &mut found, &mut counts);
    }
    let mut seen = BTreeSet::new();
    let mut out: Vec<Skill> = found
        .into_iter()
        .filter(|s| seen.insert(s.name.clone()))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    tracing::info!(
        "[agent_session] 스킬 {}개 — 파일 {} · 숨김 {} · 앞머리 못 읽음 {}",
        out.len(),
        counts.files,
        counts.hidden,
        counts.broken
    );
    out
}

#[derive(Default)]
struct Counts {
    files: usize,
    hidden: usize,
    broken: usize,
}

/// `dir/*/SKILL.md` 를 읽는다. 폴더가 없는 것은 흔해서(상위 폴더 대부분) 남기지 않는다.
fn collect(dir: &Path, plugin: Option<&str>, out: &mut Vec<Skill>, counts: &mut Counts) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    for skill_dir in dirs {
        let file = skill_dir.join("SKILL.md");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        counts.files += 1;
        let Some(front) = front_matter(&text) else {
            counts.broken += 1;
            tracing::warn!(
                "[agent_session] 스킬 앞머리를 못 읽었다: {}",
                file.display()
            );
            continue;
        };
        if front.get("user-invocable").map(String::as_str) == Some("false") {
            counts.hidden += 1;
            continue;
        }
        let Some(folder) = skill_dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let base = front
            .get("name")
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| folder.to_string());
        let name = match plugin {
            Some(p) => format!("{p}:{base}"),
            None => base,
        };
        let description = fold(front.get("description").map_or("", String::as_str));
        out.push(Skill { name, description });
    }
}

/// 켜진 플러그인의 (이름, 설치 폴더). 프로젝트 범위로 설치한 것은 그 프로젝트 안에서만 켠다.
fn enabled_plugins(config: &Path, cwd: Option<&Path>) -> Vec<(String, PathBuf)> {
    let read = |p: PathBuf| -> Option<Value> {
        serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
    };
    let Some(installed) = read(config.join("plugins/installed_plugins.json")) else {
        return Vec::new();
    };
    let enabled = read(config.join("settings.json"))
        .and_then(|s| s.get("enabledPlugins").cloned())
        .unwrap_or(Value::Null);
    let Some(plugins) = installed.get("plugins").and_then(Value::as_object) else {
        tracing::warn!("[agent_session] installed_plugins.json 에 plugins 가 없다");
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, entries) in plugins {
        if enabled.get(key).and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let entries: Vec<&Value> = match entries {
            Value::Array(a) => a.iter().collect(),
            other => vec![other],
        };
        for e in entries {
            let Some(root) = e.get("installPath").and_then(Value::as_str) else {
                continue;
            };
            let in_scope = e
                .get("projectPath")
                .and_then(Value::as_str)
                .is_none_or(|p| cwd.is_some_and(|c| c.starts_with(p)));
            if !in_scope {
                continue;
            }
            let root = PathBuf::from(root);
            let name = plugin_name(key, &root);
            out.push((name, root));
        }
    }
    out
}

/// 플러그인 이름 — 설치 폴더의 `.claude-plugin/plugin.json` `name`, 없으면 키의 `@` 앞.
fn plugin_name(key: &str, root: &Path) -> String {
    std::fs::read_to_string(root.join(".claude-plugin/plugin.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_string))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| key.split('@').next().unwrap_or(key).to_string())
}

/// `---` 로 감싼 앞머리의 윗단 키. 값이 `|` · `>` 거나 비어 있으면 이어지는 들여쓴 줄을 한 줄로 잇는다.
/// 첫 줄이 `---` 가 아니거나 닫는 줄이 없으면 `None`.
fn front_matter(text: &str) -> Option<HashMap<String, String>> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let mut map = HashMap::new();
    let mut current: Option<String> = None;
    for line in lines {
        if line.trim_end() == "---" {
            return Some(map);
        }
        if line.starts_with([' ', '\t']) {
            if let Some(key) = &current {
                let v: &mut String = map.entry(key.clone()).or_default();
                if !v.is_empty() {
                    v.push(' ');
                }
                v.push_str(line.trim());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            current = None;
            continue;
        };
        let value = value.trim();
        let value = if matches!(value, "|" | ">" | "|-" | ">-") {
            ""
        } else {
            value.trim_matches(|c| c == '"' || c == '\'')
        };
        current = Some(key.trim().to_string());
        map.insert(key.trim().to_string(), value.to_string());
    }
    None
}

/// 공백을 하나로 접고 상한을 넘으면 말줄임을 붙인다.
fn fold(s: &str) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= DESCRIPTION_MAX_CHARS {
        return one;
    }
    let mut cut: String = one.chars().take(DESCRIPTION_MAX_CHARS).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_dir_all(&self.0) {
                tracing::warn!("임시 폴더 정리 실패: {e}");
            }
        }
    }

    fn tmp(tag: &str) -> Tmp {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("tasty-skills-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).expect("임시 폴더");
        Tmp(p)
    }

    fn skill(dir: &Path, folder: &str, body: &str) {
        let d = dir.join(folder);
        std::fs::create_dir_all(&d).expect("스킬 폴더");
        std::fs::write(d.join("SKILL.md"), body).expect("SKILL.md");
    }

    fn names(v: &[Skill]) -> Vec<&str> {
        v.iter().map(|s| s.name.as_str()).collect()
    }

    /// 사용자 · 플러그인(켬/끔) · 상위 폴더 · 추가 폴더 · 이름 없음 · 깨진 앞머리 · 숨김 — 개수까지 맞춘다.
    #[test]
    fn every_source_is_read_and_counted() {
        let root = tmp("all");
        let config = root.0.join("config");
        let user = config.join("skills");
        skill(
            &user,
            "u-one",
            "---\nname: user-one\ndescription: first\n---\nbody",
        );
        skill(&user, "no-name", "---\ndescription: falls back\n---\n");
        skill(&user, "broken", "no front matter\n");
        skill(
            &user,
            "hidden",
            "---\nname: hidden\nuser-invocable: false\n---\n",
        );
        skill(
            &user,
            "nomodel",
            "---\nname: nomodel\ndisable-model-invocation: true\n---\n",
        );
        let on = root.0.join("plug-on");
        skill(
            &on.join("skills"),
            "s",
            "---\nname: tool\ndescription: from plugin\n---\n",
        );
        let off = root.0.join("plug-off");
        skill(&off.join("skills"), "s", "---\nname: off\n---\n");
        std::fs::create_dir_all(config.join("plugins")).expect("plugins");
        std::fs::write(
            config.join("plugins/installed_plugins.json"),
            serde_json::json!({ "version": 2, "plugins": {
                "on-plug@market": [{ "scope": "user", "installPath": on }],
                "off-plug@market": [{ "scope": "user", "installPath": off }],
            }})
            .to_string(),
        )
        .expect("installed");
        std::fs::write(
            config.join("settings.json"),
            r#"{"enabledPlugins":{"on-plug@market":true,"off-plug@market":false}}"#,
        )
        .expect("settings");
        let project = root.0.join("repo");
        let cwd = project.join("sub/deeper");
        std::fs::create_dir_all(&cwd).expect("cwd");
        skill(
            &project.join(".claude/skills"),
            "proj",
            "---\nname: proj\n---\n",
        );
        let extra = root.0.join("extra");
        skill(
            &extra.join(".claude/skills"),
            "ext",
            "---\nname: ext\n---\n",
        );

        let got = scan(&config, Some(&cwd), std::slice::from_ref(&extra));
        assert_eq!(
            names(&got),
            vec![
                "ext",
                "no-name",
                "nomodel",
                "on-plug:tool",
                "proj",
                "user-one"
            ]
        );
        assert_eq!(got.len(), 6, "개수: {got:?}");
        let one = got.iter().find(|s| s.name == "user-one").expect("user-one");
        assert_eq!(one.description, "first");
    }

    /// 자식을 띄울 때처럼 세션 `cwd` 가 없으면 프로필 `cwd` 를 쓰고, 추가 폴더는 세션 것과 프로필 것을 다 본다.
    #[test]
    fn a_session_is_scanned_where_its_child_runs() {
        use crate::ports::supervisor::fake::FakeSupervisor;
        let root = tmp("session");
        let config = root.0.join("config");
        skill(&config.join("skills"), "u", "---\nname: user\n---\n");
        let profile_cwd = root.0.join("profile-cwd");
        skill(
            &profile_cwd.join(".claude/skills"),
            "p",
            "---\nname: from-profile-cwd\n---\n",
        );
        let session_cwd = root.0.join("session-cwd");
        skill(
            &session_cwd.join(".claude/skills"),
            "s",
            "---\nname: from-session-cwd\n---\n",
        );
        let profile_extra = root.0.join("profile-extra");
        skill(
            &profile_extra.join(".claude/skills"),
            "e",
            "---\nname: from-profile-add-dir\n---\n",
        );
        let session_extra = root.0.join("session-extra");
        skill(
            &session_extra.join(".claude/skills"),
            "x",
            "---\nname: from-session-add-dir\n---\n",
        );

        let sup = FakeSupervisor::new();
        *sup.agent_session.borrow_mut() = Some("c1".into());
        sup.dev_env
            .borrow_mut()
            .insert("CLAUDE_CONFIG_DIR".into(), config.display().to_string());
        *sup.dev_cwd.borrow_mut() = Some(profile_cwd.display().to_string());
        *sup.dev_add_dirs.borrow_mut() = vec![profile_extra.display().to_string()];
        *sup.status_extra.borrow_mut() =
            Some(serde_json::json!({ "add_dirs": [session_extra.display().to_string()] }));
        let got = for_session(&sup, "s1").expect("claude 세션");
        assert_eq!(
            names(&got),
            vec![
                "from-profile-add-dir",
                "from-profile-cwd",
                "from-session-add-dir",
                "user"
            ]
        );

        *sup.status_extra.borrow_mut() =
            Some(serde_json::json!({ "cwd": session_cwd.display().to_string() }));
        let got = for_session(&sup, "s1").expect("claude 세션");
        assert!(names(&got).contains(&"from-session-cwd"), "{got:?}");
        assert!(
            !names(&got).contains(&"from-profile-cwd"),
            "세션 cwd 가 있으면 프로필 cwd 를 보지 않는다: {got:?}"
        );
    }

    #[test]
    fn a_plugin_installed_for_a_project_is_only_on_inside_it() {
        let root = tmp("scope");
        let config = root.0.join("config");
        let plug = root.0.join("plug");
        skill(&plug.join("skills"), "s", "---\nname: s\n---\n");
        std::fs::create_dir_all(config.join("plugins")).expect("plugins");
        let project = root.0.join("project");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::write(
            config.join("plugins/installed_plugins.json"),
            serde_json::json!({ "plugins": { "p@m": [{ "scope": "project", "installPath": plug, "projectPath": project }] }})
                .to_string(),
        )
        .expect("installed");
        std::fs::write(
            config.join("settings.json"),
            r#"{"enabledPlugins":{"p@m":true}}"#,
        )
        .expect("settings");
        assert_eq!(names(&scan(&config, Some(&project), &[])), vec!["p:s"]);
        assert!(scan(&config, Some(&root.0), &[]).is_empty());
    }

    #[test]
    fn the_plugin_manifest_name_wins_over_the_key() {
        let root = tmp("pname");
        std::fs::create_dir_all(root.0.join(".claude-plugin")).expect("dir");
        std::fs::write(
            root.0.join(".claude-plugin/plugin.json"),
            r#"{"name":"real"}"#,
        )
        .expect("plugin.json");
        assert_eq!(plugin_name("key@market", &root.0), "real");
        assert_eq!(plugin_name("key@market", Path::new("/nonexistent")), "key");
    }

    #[test]
    fn a_duplicate_name_keeps_the_first_source() {
        let root = tmp("dup");
        let config = root.0.join("config");
        skill(
            &config.join("skills"),
            "a",
            "---\nname: same\ndescription: user\n---\n",
        );
        let cwd = root.0.join("repo");
        skill(
            &cwd.join(".claude/skills"),
            "a",
            "---\nname: same\ndescription: project\n---\n",
        );
        let got = scan(&config, Some(&cwd), &[]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].description, "user");
    }

    #[test]
    fn front_matter_reads_folded_values_and_refuses_an_open_block() {
        let f = front_matter("---\nname: x\ndescription: >\n  two\n  lines\n---\n").expect("front");
        assert_eq!(f.get("description").map(String::as_str), Some("two lines"));
        assert_eq!(
            front_matter("---\nname: \"quoted\"\n---\n")
                .and_then(|f| f.get("name").cloned())
                .as_deref(),
            Some("quoted")
        );
        assert!(front_matter("---\nname: open\n").is_none());
        assert!(front_matter("name: none\n").is_none());
    }

    #[test]
    fn a_long_description_is_folded_and_cut() {
        let long = "word ".repeat(100);
        let f = fold(&long);
        assert_eq!(f.chars().count(), DESCRIPTION_MAX_CHARS + 1);
        assert!(f.ends_with('…'));
        assert_eq!(fold(" a \n b "), "a b");
    }

    #[test]
    fn only_the_reload_commands_ask_for_a_rescan() {
        assert!(reloads(" /reload-skills "));
        assert!(reloads("/reload-plugins"));
        assert!(!reloads("/reload-skills now"));
        assert!(reloads("/add-dir /tmp/x"));
        assert!(!reloads("/add-dirx"));
        assert!(!reloads("/clear"));
    }

    #[test]
    fn a_tab_asks_once_per_session_until_it_is_forgotten() {
        let key = |s: &str| (987_654, s.to_string());
        let sup = crate::ports::supervisor::fake::FakeSupervisor::new();
        on_document(&sup, 987_654, "sess-a", Some("claude"));
        on_document(&sup, 987_654, "sess-a", Some("claude"));
        on_document(&sup, 987_654, "sess-pi", Some("pi"));
        assert_eq!(
            sup.methods().iter().filter(|m| *m == "status").count(),
            1,
            "같은 탭 · 세션은 한 번만 훑는다: {:?}",
            sup.methods()
        );
        {
            let r = REQUESTED.lock().expect("lock");
            assert!(r.contains(&key("sess-a")));
            assert!(!r.contains(&key("sess-pi")), "pi 는 훑지 않는다");
        }
        forget(987_654);
        assert!(!REQUESTED.lock().expect("lock").contains(&key("sess-a")));
    }

    #[test]
    fn the_cache_is_per_session() {
        store(
            "skills-test-a",
            vec![Skill {
                name: "x".into(),
                description: String::new(),
            }],
        );
        assert_eq!(cached("skills-test-a").len(), 1);
        assert!(cached("skills-test-b").is_empty());
    }
}
