//! 대화 탭 복제 — 원본 탭과 **같은 프로필 · 작업 폴더 · 설정**으로 **새** 세션을 띄울 값을 만든다.
//!
//! 탭·surface 우클릭의 "Surface 복제"(호스트가 `copy_of` 를 실어 새 대화 탭을 연다)와
//! `tasty agent-session start --copy-of` 가 이 한 곳을 지난다. 세션 id · 대화 · 대기열은 옮기지 않는다 —
//! 복제는 같은 조건의 **빈 대화**다.
//!
//! 값의 출처는 둘이다:
//! - 데몬 세션 기록(`session.status`) — `agent_name`(프로필) · `cwd` · `permission_mode` · `add_dirs`.
//!   데몬이 재개 때도 쓰는 기록이라 사용자가 시작 화면에서 고른 값이 그대로 남아 있다.
//! - 원본 탭 상태줄이 본 모델(`surface_link::USAGE_META_KEY`) — 데몬은 모델을 기록하지 않는다.
//!   아직 턴이 없어 모르면 칸을 비워 프로필 기본으로 뜬다.
use serde_json::{Map, Value, json};

use crate::application::surface_link;
use crate::ports::host::HostCall;
use crate::ports::supervisor::Supervisor;

/// 탭 만들기 · `start` 매개변수에서 복제 원본 surface 를 가리키는 칸.
pub const COPY_OF: &str = "copy_of";

/// 복제할 설정.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopySettings {
    pub agent: String,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub add_dirs: Vec<String>,
    /// 역할 폴더 이름 — 원본 세션 기록의 `roles`.
    pub roles: Vec<String>,
}

/// 매개변수가 복제를 청하면 원본 surface id. 폼 값처럼 `params.params` 아래에 와도 본다.
pub fn source_of(params: &Value) -> Option<u32> {
    params
        .pointer(&format!("/params/{COPY_OF}"))
        .or_else(|| params.get(COPY_OF))
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.trim().parse().ok()))
        .and_then(|n| u32::try_from(n).ok())
}

/// 데몬 `session.status` 원문과 상태줄 모델에서 복제 설정을 고른다.
///
/// 프로필(`agent_name`)이 없으면 거절한다 — 데몬도 그 세션의 재개를 거절하는 v3 이전 기록이고,
/// `kind` 로 짐작해 띄우면 남의 프로필이 된다.
pub fn settings_from_status(
    source: u32,
    status: &Value,
    model: Option<String>,
) -> Result<CopySettings, String> {
    let s = |k: &str| {
        status
            .get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let agent = s("agent_name")
        .ok_or_else(|| crate::domain::i18n::t1("sup.err.copy_no_profile", &source.to_string()))?;
    let add_dirs = status
        .get("add_dirs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(CopySettings {
        agent,
        cwd: s("cwd"),
        model: model.filter(|m| !m.trim().is_empty()),
        permission_mode: s("permission_mode"),
        add_dirs,
        roles: crate::adapters::driven::store::roles::names_of(status),
    })
}

/// 원본 탭에서 복제 설정을 읽는다.
pub fn read_source(
    host: &impl HostCall,
    sup: &impl Supervisor,
    source: u32,
) -> Result<CopySettings, String> {
    let session = surface_link::session_of_surface(host, source)
        .map_err(|_| crate::domain::i18n::t1("sup.err.copy_not_linked", &source.to_string()))?;
    let status = sup.status(&session).map_err(|e| e.to_string())?;
    let model = surface_link::load_usage(host, source)
        .and_then(|v| crate::domain::usage::from_snapshot(&v).model);
    settings_from_status(source, &status, model)
}

/// 복제 설정을 매개변수에 얹는다 — 탭 만들기 갈래(`agent` 로 새로 띄운다)가 그대로 읽는 평평한 칸이다.
/// 원본을 가리키는 칸은 지운다(복원 스냅샷에 남아 다시 복제되지 않게).
pub fn apply_to(params: &Value, c: &CopySettings) -> Value {
    let mut out = match params {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    };
    out.remove(COPY_OF);
    if let Some(Value::Object(inner)) = out.get_mut("params") {
        inner.remove(COPY_OF);
    }
    out.insert("agent".into(), json!(c.agent));
    for (k, v) in [
        ("cwd", &c.cwd),
        ("model", &c.model),
        ("permission_mode", &c.permission_mode),
    ] {
        if let Some(v) = v {
            out.insert(k.into(), json!(v));
        }
    }
    if !c.add_dirs.is_empty() {
        out.insert("add_dirs".into(), json!(c.add_dirs));
    }
    if !c.roles.is_empty() {
        out.insert("roles".into(), json!(c.roles));
    }
    Value::Object(out)
}

/// 매개변수의 추가 작업 폴더 — 폼 값처럼 `params.params` 아래에 와도 본다.
pub fn add_dirs_of(params: &Value) -> Vec<String> {
    params
        .pointer("/params/add_dirs")
        .or_else(|| params.get("add_dirs"))
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

    fn status() -> Value {
        json!({
            "id": "s1",
            "agent_name": "sdk",
            "cwd": "/w/tasty",
            "permission_mode": "bypassPermissions",
            "permission_prompts": true,
            "add_dirs": ["/w/other"],
            "roles": ["builder", "tester"],
            "agent_session_id": "c1",
            "state": "running",
        })
    }

    #[test]
    fn copy_of_is_read_flat_nested_and_as_text() {
        assert_eq!(source_of(&json!({ "copy_of": 7 })), Some(7));
        assert_eq!(source_of(&json!({ "params": { "copy_of": 8 } })), Some(8));
        assert_eq!(source_of(&json!({ "copy_of": "9" })), Some(9));
        assert_eq!(source_of(&json!({ "agent": "sdk" })), None);
        assert_eq!(source_of(&json!({ "copy_of": -1 })), None);
    }

    #[test]
    fn every_launch_setting_comes_over_and_the_session_id_does_not() {
        let c = settings_from_status(5, &status(), Some("claude-opus-5-5".into())).unwrap();
        assert_eq!(
            c,
            CopySettings {
                agent: "sdk".into(),
                cwd: Some("/w/tasty".into()),
                model: Some("claude-opus-5-5".into()),
                permission_mode: Some("bypassPermissions".into()),
                add_dirs: vec!["/w/other".into()],
                roles: vec!["builder".into(), "tester".into()],
            }
        );
        let p = apply_to(&json!({ "copy_of": 5, "params": { "copy_of": 5 } }), &c);
        assert_eq!(source_of(&p), None, "복원 스냅샷이 다시 복제하면 안 된다");
        for k in ["session_id", "adopt", "agent_session_id"] {
            assert!(p.get(k).is_none(), "{k} 가 새 세션으로 넘어갔다");
        }
        assert_eq!(p["agent"], "sdk");
        assert_eq!(p["cwd"], "/w/tasty");
        assert_eq!(p["model"], "claude-opus-5-5");
        assert_eq!(p["permission_mode"], "bypassPermissions");
        assert_eq!(add_dirs_of(&p), vec!["/w/other".to_string()]);
        assert_eq!(
            crate::adapters::driven::store::roles::names_of(&p),
            vec!["builder".to_string(), "tester".to_string()],
            "역할이 복제로 안 넘어갔다"
        );
    }

    #[test]
    fn missing_values_stay_unset_so_the_profile_defaults_apply() {
        let c = settings_from_status(5, &json!({ "agent_name": "dev" }), None).unwrap();
        let p = apply_to(&json!({}), &c);
        assert_eq!(p, json!({ "agent": "dev" }));
    }

    /// `supervisor start --copy-of` 가 원본 세션 기록의 설정을 **실제 spawn 까지** 나른다.
    #[test]
    fn cli_start_with_copy_of_spawns_with_the_source_settings() {
        use crate::ports::supervisor::fake::FakeSupervisor;
        let sup = FakeSupervisor::new();
        *sup.status_extra.borrow_mut() = Some(status());
        // 원본 5 는 세션에 이어져 있고, 대상 9 는 빈 대화 탭이다.
        let host = surface_link::tests_support::FakeHost::linked(5, "s-src")
            .with_surfaces(vec![json!({ "id": 5 }), json!({ "id": 9 })]);
        let r = crate::adapters::driving::ipc::handle_start_or_copy(
            &host,
            &sup,
            &json!({ "surface_id": 9, "copy_of": 5 }),
        );

        assert!(r.is_ok(), "{r:?}");
        assert!(
            sup.calls.borrow().iter().any(|c| c == "spawn:sdk:/w/tasty"),
            "원본 프로필 · 폴더로 안 띄웠다: {:?}",
            sup.calls.borrow()
        );
        assert_eq!(
            sup.last_spawn.borrow().clone(),
            Some((
                None,
                Some("bypassPermissions".into()),
                vec!["/w/other".to_string()]
            ))
        );
        assert_eq!(
            surface_link::session_of_surface(&host, 9).ok().as_deref(),
            Some("새세션")
        );
    }

    /// 원본에 세션이 없으면 띄우지 않고 거절한다.
    #[test]
    fn cli_start_refuses_a_source_without_a_session() {
        use crate::ports::supervisor::fake::FakeSupervisor;
        let sup = FakeSupervisor::new();
        let host = surface_link::tests_support::FakeHost::new()
            .with_surfaces(vec![json!({ "id": 5 }), json!({ "id": 9 })]);
        let r = crate::adapters::driving::ipc::handle_start_or_copy(
            &host,
            &sup,
            &json!({ "surface_id": 9, "copy_of": 5 }),
        );

        assert!(r.is_err());
        assert!(!sup.calls.borrow().iter().any(|c| c.starts_with("spawn")));
    }

    #[test]
    fn a_session_without_a_profile_is_refused_instead_of_guessed() {
        let e = settings_from_status(5, &json!({ "kind": "claude" }), None).unwrap_err();
        assert!(e.contains('5'), "{e}");
    }
}
