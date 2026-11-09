//! 시작 화면의 **에이전트 관리 · 역할 관리** 탭과 연결 탭의 역할 고르기 칸.
//!
//! 에이전트(프로필)는 세션 하나에 하나만 고르고(연결 탭의 select), 역할 폴더는 여럿을 고른다(체크
//! 칸 `r`). 관리는 같은 화면에서 한다 — 고친 뒤 화면을 다시 그리면 select 와 체크 칸에 바로 나온다.
//!
//! | 표식 | 하는 일 | 칸 |
//! |---|---|---|
//! | [`PROFILE_SAVE`] | 에이전트를 만들거나(`pmode=new`) 편집한다(`pmode=edit`) | `pn` 이름 · `pt` 종류 · `pc` 명령 · `pw` 폴더 · `pmo` 모델 · `pe` 환경 |
//! | [`PROFILE_DELETE`] | 에이전트를 삭제한다 | `pn` |
//! | [`ROLE_ADD`] | 역할 폴더를 더한다 | `rn` 이름 · `rp` 경로 |
//! | [`ROLE_SCAN`] | 폴더 아래 `CLAUDE.md` 를 가진 폴더를 모두 더한다 | `rs` 경로 |
//! | [`ROLE_DELETE`] | 역할을 삭제한다 | `rd` |
//!
//! 만들기는 있는 이름을 **거절한다** — 같은 폼이 만들기와 편집을 겸하면 실수로 덮어쓴다. 편집은
//! 목록의 편집 단추로만 들어가고 이름을 잠근다. 편집은 **적은 칸만** 바꾼다 — `settings` ·
//! `add_dirs` · `args` 처럼 폼에 없는 칸은 그대로 둔다(CLI `profile-set` 으로 고친다). 환경 칸은
//! 적었으면 통째로 바꾼다.
//!
//! 폼 제출은 스크립트가 가로채 fragment 로 보낸다 — GET 제출은 문서를 떠나 흰 화면이 된다
//! (`start_form` 의 제출 주석과 같은 이유).

mod view;

use std::collections::BTreeMap;
use std::path::Path;

use crate::adapters::driven::store::agents::{self, AgentProfile};
use crate::adapters::driven::store::roles;
use crate::adapters::driving::url::has_mark;
use crate::adapters::driving::url::percent_decode;
use crate::adapters::driving::url::query_of;
use crate::application::session_surface;
use crate::domain::i18n::t1;
use crate::ports::host::SendHost;

pub use view::{FORM_SCRIPT, ROLE_SCRIPT, render_agents, render_roles, role_table};

pub const PROFILE_SAVE: &str = "agent-prof-save";
pub const PROFILE_DELETE: &str = "agent-prof-del";
pub const ROLE_ADD: &str = "agent-role-add";
pub const ROLE_SCAN: &str = "agent-role-scan";
pub const ROLE_DELETE: &str = "agent-role-del";

/// 관리 탭이 보낸 요청.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    SaveProfile {
        name: String,
        /// 편집인가 — `false` 면 새로 만들기라 있는 이름을 거절한다.
        edit: bool,
        agent_type: String,
        command: Option<String>,
        cwd: Option<String>,
        model: Option<String>,
        /// `None` = 환경 칸을 비웠다(있는 값을 둔다).
        env: Option<BTreeMap<String, String>>,
    },
    DeleteProfile(String),
    AddRole {
        name: String,
        path: String,
    },
    ScanRoles(String),
    DeleteRole(String),
}

/// 관리 결과를 보일 탭.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Agents,
    Roles,
}

impl Tab {
    /// 시작 화면의 탭 이름(`tab-<이름>`).
    pub fn id(self) -> &'static str {
        match self {
            Tab::Agents => "agents",
            Tab::Roles => "roles",
        }
    }
}

impl Action {
    pub fn tab(&self) -> Tab {
        match self {
            Action::SaveProfile { .. } | Action::DeleteProfile(_) => Tab::Agents,
            Action::AddRole { .. } | Action::ScanRoles(_) | Action::DeleteRole(_) => Tab::Roles,
        }
    }
}

/// 관리 요청의 결과 한 줄 — 성공인지와 보일 탭을 함께 든다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    pub ok: bool,
    pub tab: Tab,
}

/// 줄마다 `KEY=VALUE` — `=` 가 없거나 키가 빈 줄은 뺀다.
fn env_of(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let k = k.trim();
            (!k.is_empty()).then(|| (k.to_string(), v.trim().to_string()))
        })
        .collect()
}

pub fn parse(url: &str) -> Option<Action> {
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    let get = |k: &str| -> Option<String> {
        let key = format!("{k}=");
        fields
            .iter()
            .find_map(|kv| kv.strip_prefix(key.as_str()))
            .map(percent_decode)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    if has_mark(&fields, PROFILE_SAVE) {
        return Some(Action::SaveProfile {
            name: get("pn")?,
            // 모드가 없으면 만들기로 본다 — 모르는 요청이 있는 에이전트를 덮지 않게 한다.
            edit: get("pmode").as_deref() == Some("edit"),
            agent_type: get("pt").unwrap_or_else(|| "claude".into()),
            command: get("pc"),
            cwd: get("pw"),
            model: get("pmo"),
            env: get("pe").map(|e| env_of(&e)),
        });
    }
    if has_mark(&fields, PROFILE_DELETE) {
        return get("pn").map(Action::DeleteProfile);
    }
    if has_mark(&fields, ROLE_ADD) {
        return Some(Action::AddRole {
            name: get("rn")?,
            path: get("rp")?,
        });
    }
    if has_mark(&fields, ROLE_SCAN) {
        return get("rs").map(Action::ScanRoles);
    }
    if has_mark(&fields, ROLE_DELETE) {
        return get("rd").map(Action::DeleteRole);
    }
    None
}

/// 요청을 반영한다 — 성공하면 화면에 남길 한 줄.
pub fn apply(action: &Action, profiles_dir: &Path, roles_file: &Path) -> Result<String, String> {
    match action {
        Action::SaveProfile {
            name,
            edit,
            agent_type,
            command,
            cwd,
            model,
            env,
        } => {
            if !agents::supported(agent_type) {
                return Err(t1("sup.err.kind_unsupported", agent_type));
            }
            // 화면의 목록과 같은 기준(파일이 없을 때의 기본 `claude` 포함)으로 있는지 본다 — 목록에
            // 보이는 이름을 만들기가 받으면 사용자에게는 덮어쓴 것으로 보인다.
            let known = agents::load(Some(profiles_dir));
            let exists = known.iter().any(|p| &p.name == name);
            if !edit && exists {
                return Err(t1("sup.manage.exists", name));
            }
            if *edit && !exists {
                return Err(t1("sup.manage.not_found", name));
            }
            let mut p = known
                .into_iter()
                .find(|p| &p.name == name && profiles_dir.join(format!("{name}.json")).is_file())
                .unwrap_or_else(|| AgentProfile {
                    name: name.clone(),
                    ..AgentProfile::default()
                });
            // 종류가 바뀌면 이전 종류의 명령을 들고 가지 않는다.
            if p.kind != *agent_type && command.is_none() {
                p.command = agent_type.clone();
            }
            p.kind = agent_type.clone();
            if let Some(c) = command {
                p.command = c.clone();
            }
            if p.command.trim().is_empty() {
                p.command = agent_type.clone();
            }
            if cwd.is_some() {
                p.cwd = cwd.clone();
            }
            if model.is_some() {
                p.model = model.clone();
            }
            if let Some(e) = env {
                p.env = e.clone();
            }
            agents::save(profiles_dir, &p)?;
            Ok(t1(
                if *edit {
                    "sup.manage.saved"
                } else {
                    "sup.manage.created"
                },
                name,
            ))
        }
        Action::DeleteProfile(name) => {
            agents::remove(profiles_dir, name)?;
            Ok(t1("sup.manage.removed", name))
        }
        Action::AddRole { name, path } => {
            roles::add(roles_file, name, path)?;
            Ok(t1("sup.manage.role_added", name))
        }
        Action::ScanRoles(path) => {
            let added = roles::scan(roles_file, path)?;
            Ok(t1("sup.manage.roles_scanned", &added.len().to_string()))
        }
        Action::DeleteRole(name) => {
            roles::remove(roles_file, name)?;
            Ok(t1("sup.manage.role_removed", name))
        }
    }
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 관리 요청을 반영하고 시작 화면을 다시 그린다 — 결과 한 줄을 그 탭 위에 남긴다.
    pub(crate) fn apply_manage(&mut self, host: &impl SendHost, sid: u32, action: &Action) {
        let (Some(dir), Some(file)) = (agents::default_dir(), roles::default_file()) else {
            tracing::warn!(
                "[agent_session] surface={sid}: plugin 데이터 폴더가 없어 관리 요청을 버린다"
            );
            return;
        };
        let note = match apply(action, &dir, &file) {
            Ok(n) => Note {
                text: n,
                ok: true,
                tab: action.tab(),
            },
            Err(why) => Note {
                text: t1("sup.manage.failed", &why),
                ok: false,
                tab: action.tab(),
            },
        };
        tracing::info!("[agent_session] surface={sid}: 관리 — {}", note.text);
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        c.profiles = agents::load(Some(&dir));
        c.roles = roles::load(Some(&file));
        c.manage_note = Some(note);
        session_surface::push(host, &self.sup, sid, c, false);
    }
}

#[cfg(test)]
mod tests {
    use super::view::{env_cell, kind_cell};
    use super::*;
    use crate::adapters::driven::store::roles::Role;
    use crate::adapters::driving::html::escape;
    use crate::adapters::driving::view::table;
    use crate::domain::i18n::t;

    fn tmp(tag: &str) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!(
            "agent-session-manage-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn save(name: &str, edit: bool, agent_type: &str, cwd: Option<&str>) -> Action {
        Action::SaveProfile {
            name: name.into(),
            edit,
            agent_type: agent_type.into(),
            command: None,
            cwd: cwd.map(str::to_string),
            model: None,
            env: None,
        }
    }

    /// 제출 스크립트가 만드는 모양(`URLSearchParams`: 공백은 `+`, 줄바꿈은 `%0A`) 그대로 읽는다.
    #[test]
    fn a_profile_form_is_read_back() {
        let url = "about:blank#pn=pi-dev&pt=pi&pc=&pw=%2Fwork&pmo=&pe=PI_CODING_AGENT_DIR%3D%2Fp%2F.pi%2Fagent%0AX%3Da+b&pmode=new&agent-prof-save=1&nonce=1";
        let Some(Action::SaveProfile {
            name,
            edit,
            agent_type,
            command,
            cwd,
            model,
            env,
        }) = parse(url)
        else {
            panic!("프로필 저장으로 안 읽혔다");
        };
        assert_eq!((name.as_str(), agent_type.as_str()), ("pi-dev", "pi"));
        assert!(!edit, "만들기를 편집으로 읽었다");
        assert_eq!((command, model), (None, None), "빈 칸은 값이 아니다");
        assert_eq!(cwd.as_deref(), Some("/work"));
        let env = env.unwrap();
        assert_eq!(env["PI_CODING_AGENT_DIR"], "/p/.pi/agent");
        assert_eq!(env["X"], "a b");
        assert!(matches!(
            parse("about:blank#pn=cc&pmode=edit&agent-prof-save=1"),
            Some(Action::SaveProfile { edit: true, .. })
        ));
        assert!(
            matches!(
                parse("about:blank#pn=cc&agent-prof-save=1"),
                Some(Action::SaveProfile { edit: false, .. })
            ),
            "모드가 없는 요청은 만들기여야 한다 — 편집이면 있는 것을 덮는다"
        );
        assert_eq!(
            parse("about:blank#rn=b&rp=%2Fr%2Fb&agent-role-add=1"),
            Some(Action::AddRole {
                name: "b".into(),
                path: "/r/b".into()
            })
        );
        assert_eq!(
            parse("about:blank#rd=b&agent-role-del=1"),
            Some(Action::DeleteRole("b".into()))
        );
        assert_eq!(
            parse("about:blank#a=cc&agent-start=1"),
            None,
            "시작은 관리가 아니다"
        );
    }

    /// 편집하면 적은 칸만 바뀐다 — 폼에 없는 `settings` · `args` 는 남는다.
    #[test]
    fn editing_a_profile_keeps_the_fields_the_form_does_not_show() {
        let d = tmp("save");
        let dir = d.join("profiles");
        let file = d.join("roles.json");
        let old = AgentProfile::from_json(
            "cc",
            &serde_json::json!({"agent_type":"claude","model":"opus","settings":{"a":1},"args":["--x"]}),
        )
        .unwrap();
        agents::save(&dir, &old).unwrap();
        let note = apply(&save("cc", true, "claude", Some("/w")), &dir, &file).unwrap();
        assert!(note.contains("cc"));
        let got = agents::load(Some(&dir));
        assert_eq!(got[0].cwd.as_deref(), Some("/w"));
        assert_eq!(got[0].model.as_deref(), Some("opus"));
        assert_eq!(got[0].settings, Some(serde_json::json!({"a":1})));
        assert_eq!(got[0].args, vec!["--x".to_string()]);
        // 종류를 바꾸면 명령이 그 종류로 간다.
        apply(&save("cc", true, "pi", None), &dir, &file).unwrap();
        let got = agents::load(Some(&dir));
        assert_eq!(
            (got[0].kind.as_str(), got[0].command.as_str()),
            ("pi", "pi")
        );
        let bad = apply(&save("x", false, "codex", None), &dir, &file);
        assert!(bad.is_err(), "모르는 종류는 거절한다");
    }

    /// 만들기는 있는 이름을 **거절한다** — 같은 폼으로 만들고 고치면 실수로 덮어쓴다.
    /// 편집은 없는 이름을 거절한다. 거절한 요청은 파일을 건드리지 않는다.
    #[test]
    fn creating_refuses_an_existing_name_and_editing_a_missing_one() {
        let d = tmp("create");
        let dir = d.join("profiles");
        let file = d.join("roles.json");
        apply(&save("cc", false, "claude", Some("/first")), &dir, &file).expect("새 이름은 만든다");
        let again = apply(&save("cc", false, "pi", Some("/second")), &dir, &file);
        assert!(again.is_err(), "있는 이름을 만들기로 덮었다");
        let got = agents::load(Some(&dir));
        assert_eq!(
            (got[0].kind.as_str(), got[0].cwd.as_deref()),
            ("claude", Some("/first")),
            "거절했는데 파일이 바뀌었다"
        );
        assert!(
            apply(&save("nope", true, "claude", None), &dir, &file).is_err(),
            "없는 이름의 편집이 새로 만들었다"
        );
        assert!(!dir.join("nope.json").exists());
        // 파일이 없을 때 보이는 기본 `claude` 도 목록에 있는 이름이다 — 만들기는 거절, 편집은 된다.
        let d2 = tmp("default");
        let dir2 = d2.join("profiles");
        assert!(apply(&save("claude", false, "claude", None), &dir2, &file).is_err());
        apply(&save("claude", true, "claude", Some("/w")), &dir2, &file)
            .expect("기본 에이전트를 편집하지 못했다");
        assert!(dir2.join("claude.json").is_file());
    }

    /// 표의 삭제 링크(브라우저가 `&amp;` 를 푼 모양 + 확인 단계의 `ok=1`)가 삭제로 읽힌다.
    #[test]
    fn a_row_delete_link_is_read_back() {
        let h = render_agents(&[AgentProfile::default_claude()], None, None);
        let href = h
            .split(r#"class="btn danger del" href=""#)
            .nth(1)
            .and_then(|r| r.split('"').next())
            .expect("삭제 링크가 없다")
            .replace("&amp;", "&");
        assert_eq!(
            parse(&format!("about:blank{href}&ok=1")),
            Some(Action::DeleteProfile("claude".into()))
        );
        let h = render_roles(
            &[Role {
                name: "b".into(),
                path: "/r/b".into(),
            }],
            None,
            None,
            None,
        );
        let href = h
            .split(r#"class="btn danger del" href=""#)
            .nth(1)
            .and_then(|r| r.split('"').next())
            .expect("역할 삭제 링크가 없다")
            .replace("&amp;", "&");
        assert_eq!(
            parse(&format!("about:blank{href}&ok=1")),
            Some(Action::DeleteRole("b".into()))
        );
    }

    /// 두 탭에 다섯 표식이 모두 있고, 역할 칸은 명부의 이름을 체크 칸 `r` 로 싣는다.
    #[test]
    fn the_tabs_carry_every_mark_and_the_picker_every_role() {
        let roles = vec![Role {
            name: "builder".into(),
            path: "/r/builder".into(),
        }];
        let a = render_agents(&[AgentProfile::default_claude()], None, None);
        let r = render_roles(&roles, None, None, None);
        assert!(
            a.contains(&format!("#pn=claude&amp;{PROFILE_DELETE}=1")),
            "표에 에이전트 삭제 표식이 없다"
        );
        assert!(
            r.contains(&format!("#rd=builder&amp;{ROLE_DELETE}=1")),
            "표에 역할 삭제 표식이 없다"
        );
        assert!(a.contains(&format!(r#"name="{PROFILE_SAVE}""#)));
        for m in [ROLE_ADD, ROLE_SCAN] {
            assert!(r.contains(&format!(r#"name="{m}""#)), "{m} 표식이 없다");
        }
        assert!(
            FORM_SCRIPT.contains("form.mg"),
            "관리 폼 제출을 가로채는 스크립트가 없다"
        );
        assert!(a.contains(r#"<option value="pi">pi</option>"#));
        let p = role_table(&roles);
        assert!(p.contains(r#"name="r" value="builder""#), "{p}");
    }

    /// 연결 탭의 역할 표 — 고르기 · 이름 · 경로 칸, 검색 칸, 쪽 넘김. 검색 키는 이름과 경로를
    /// 소문자로 싣는다(스크립트가 소문자로 비교한다).
    #[test]
    fn the_role_table_has_name_and_path_columns_a_search_box_and_pages() {
        let roles = [Role {
            name: "Builder".into(),
            path: "/w/Roles/builder".into(),
        }];
        let p = role_table(&roles);
        assert!(p.contains(r#"<table class="tbl" id="rpick""#), "{p}");
        assert_eq!(p.matches(r#"scope="col""#).count(), 3, "{p}");
        for k in ["sup.form.roles_col_name", "sup.form.roles_col_path"] {
            assert!(p.contains(&escape(&t(k))), "{k} 머리글이 없다: {p}");
        }
        assert!(p.contains(r#"<input id="rq""#), "검색 칸이 없다: {p}");
        assert!(
            p.contains(r#"id="rprev""#) && p.contains(r#"id="rnext""#),
            "{p}"
        );
        assert!(p.contains(r#"data-k="builder /w/roles/builder""#), "{p}");
        assert!(
            p.contains(r#"title="/w/Roles/builder""#),
            "경로 전체가 툴팁에 없다: {p}"
        );
        assert!(
            ROLE_SCRIPT.contains("PAGE=8,"),
            "스크립트의 쪽 크기가 문서의 8행과 다르다"
        );
        // 쪽을 넘겨도 고른 칸이 제출에 실리려면 행을 지우지 않고 숨겨야 한다.
        assert!(!ROLE_SCRIPT.contains("removeChild") && !ROLE_SCRIPT.contains(".remove()"));
    }

    /// 역할이 없어도 연결 탭에 역할 칸이 있다 — 회색 안내와 역할 관리 탭으로 가는 단추.
    #[test]
    fn the_role_picker_shows_a_way_to_the_roles_tab_when_empty() {
        let p = role_table(&[]);
        assert!(p.contains(r#"class="roles-empty""#), "{p}");
        assert!(
            p.contains(r#"data-goto="roles""#),
            "역할 관리로 가는 길이 없다: {p}"
        );
        assert!(!p.contains(r#"name="r""#));
    }

    /// 만들기와 편집이 갈린다 — 제목 · 단추 글이 모드마다 있고, 모드 칸이 제출에 실린다.
    #[test]
    fn the_agent_form_has_a_create_and_an_edit_mode() {
        let h = render_agents(&[AgentProfile::default_claude()], None, None);
        assert!(
            h.contains(r#"id="pmode" name="pmode" value="new""#),
            "모드 칸이 없다"
        );
        for (attr, key) in [
            ("data-new", "sup.manage.create"),
            ("data-edit", "sup.manage.edit_title"),
            ("data-new", "sup.manage.create_button"),
            ("data-edit", "sup.manage.save"),
        ] {
            assert!(
                h.contains(&format!(r#"{attr}="{}""#, escape(&t(key)))),
                "{attr} 에 {key} 가 없다"
            );
        }
        assert!(
            h.contains(r#"id="pcancel" hidden"#),
            "취소 단추는 편집 때만 보인다"
        );
        assert!(
            FORM_SCRIPT.contains("n.readOnly=edit"),
            "편집에서 이름을 안 잠근다"
        );
        assert!(FORM_SCRIPT.contains("'Escape'"), "Esc 로 편집을 못 끝낸다");
    }

    /// 종류와 실행 명령이 같으면 한 번만, 다르면 둘 다 보인다. 환경 값의 홈은 `~` 로 줄인다.
    #[test]
    fn the_kind_shows_the_command_only_when_it_differs() {
        let same = AgentProfile::default_claude();
        assert_eq!(kind_cell(&same), "claude");
        let other = AgentProfile {
            command: "/opt/pi --x".into(),
            kind: "pi".into(),
            ..AgentProfile::default()
        };
        assert!(kind_cell(&other).contains("pi · <span class=\"mono\">/opt/pi --x</span>"));
        let mut env = AgentProfile::default_claude();
        env.env.insert("K".into(), "v".into());
        assert!(env_cell(&env).contains(">K=v<"));
        assert_eq!(env_cell(&same), table::nil());
    }

    /// 실패한 요청은 그 탭의 폼에 되살리기 표시를 싣고, 결과 줄을 실패 모양으로 그린다.
    #[test]
    fn a_failed_request_keeps_its_input_and_shows_as_an_error() {
        let n = Note {
            text: "x".into(),
            ok: false,
            tab: Tab::Agents,
        };
        let a = render_agents(&[], Some(&n), None);
        assert!(a.contains(r##"id="pform" method="get" action="#" data-restore"##));
        assert!(a.contains(r#"class="mg-note err""#));
        let r = render_roles(&[], Some(&n), None, None);
        assert!(!r.contains("data-restore"), "다른 탭의 폼까지 되살린다");
        assert!(!r.contains("mg-note"), "다른 탭에 결과가 보인다");
        let picked = render_agents(&[], None, Some("/w/p"));
        assert!(picked.contains(r#"value="/w/p" data-picked="/w/p""#));
    }
}
