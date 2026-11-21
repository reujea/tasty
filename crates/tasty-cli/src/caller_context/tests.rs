use super::*;
use serde_json::json;
use tasty_plugin_manifest::{CallerIdFormat, CallerPathEnvDecl};

const THREAD: &str = "019f55e7-3dfa-7c21-9a0b-2f6c4d1e8a90";

/// 시험용 provider 선언. 실제 번들 선언은 `bundled_codex_declaration_keeps_the_previous_contract`가 읽는다.
fn decl() -> CallerContextDecl {
    CallerContextDecl {
        provider: "vendor".into(),
        id_env: "XTEST_VENDOR_THREAD".into(),
        id_format: CallerIdFormat::Uuid,
        id_field: "thread_id".into(),
        path_env: vec![CallerPathEnvDecl {
            env: "XTEST_VENDOR_HOME".into(),
            field: "vendor_home".into(),
        }],
        executable: Some("vendor".into()),
        suppress_env_surface: true,
    }
}

fn source(decl: CallerContextDecl) -> CallerContextSource {
    CallerContextSource {
        plugin_id: "com.example.vendor".into(),
        decl,
    }
}

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |name| {
        pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| OsString::from(v))
    }
}

fn no_program(_: &str) -> Option<ExecutionContext> {
    None
}

fn callers(pairs: &[(&str, &str)]) -> Vec<Caller> {
    resolve(&[source(decl())], env_of(pairs), no_program)
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().expect("object").clone()
}

/// plugin 인자 그룹이 surface·caller_surface 를 둘 다 선언한 경우.
fn both(name: &str) -> bool {
    matches!(name, "surface" | "caller_surface")
}

fn strict() -> Vec<String> {
    vec!["vendor".into()]
}

#[test]
fn execution_is_forwarded_without_parent_flags_or_other_environment_variables() {
    let root = tempfile::tempdir().unwrap();
    let program = root.path().join("vendor");
    let found = |name: &str| {
        assert_eq!(name, "vendor");
        Some(ExecutionContext::from_parts(program.clone(), root.path().into()).unwrap())
    };
    let callers = resolve(
        &[source(decl())],
        env_of(&[("XTEST_VENDOR_THREAD", THREAD)]),
        found,
    );
    let mut p = params(json!({}));
    apply_to_plugin_params(&mut p, both, &callers, &strict(), Some("1297")).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(
        p[CONTEXT_KEY]["execution"],
        json!({
            "executable": program,
            "search_path": root.path(),
        })
    );
    assert_eq!(p[CONTEXT_KEY].as_object().unwrap().len(), 3);
}

#[test]
fn an_id_is_read_only_in_the_declared_shape() {
    tasty_i18n::init("en");
    assert!(callers(&[]).is_empty());
    assert!(callers(&[("XTEST_VENDOR_THREAD", "")]).is_empty());
    let [
        Caller::Provided {
            provider, fields, ..
        },
    ] = &callers(&[("XTEST_VENDOR_THREAD", THREAD)])[..]
    else {
        panic!("thread");
    };
    assert_eq!(provider, "vendor");
    assert_eq!(
        Value::Object(fields.clone()),
        json!({ "provider": "vendor", "thread_id": THREAD })
    );
    for bad in [
        "surface-1297",
        "019f55e7",
        "019f55e7-3dfa-7c21-9a0b-2f6c4d1e8a9g",
    ] {
        let [Caller::Invalid { message, .. }] = &callers(&[("XTEST_VENDOR_THREAD", bad)])[..]
        else {
            panic!("{bad} must not be accepted");
        };
        assert!(message.contains(bad), "{message}");
        assert!(message.contains("XTEST_VENDOR_THREAD"), "{message}");
    }
    // text 형식은 비어 있지 않은 값을 그대로 받는다.
    let mut text = decl();
    text.id_format = CallerIdFormat::Text;
    assert!(matches!(
        caller_from(&text, env_of(&[("XTEST_VENDOR_THREAD", "surface-1297")])),
        Caller::Provided { .. }
    ));
}

#[test]
fn a_path_variable_is_carried_only_when_set_and_made_absolute() {
    let with =
        |home: &str| callers(&[("XTEST_VENDOR_THREAD", THREAD), ("XTEST_VENDOR_HOME", home)]);
    assert_eq!(with(""), callers(&[("XTEST_VENDOR_THREAD", THREAD)]));
    let [Caller::Provided { fields, .. }] = &with("rel/vendor")[..] else {
        panic!("relative home must still yield a caller");
    };
    let home = fields["vendor_home"].as_str().unwrap();
    assert!(std::path::Path::new(home).is_absolute(), "{home}");
    assert!(
        home.ends_with("rel/vendor") || home.ends_with("rel\\vendor"),
        "{home}"
    );
}

#[test]
fn a_plain_shell_keeps_the_env_surface_fill_and_marks_it_implicit() {
    let mut p = params(json!({}));
    apply_to_plugin_params(&mut p, both, &[], &strict(), Some("42")).unwrap();
    assert_eq!(p["surface"], 42);
    assert_eq!(p["surface_id"], 42);
    assert_eq!(p["caller_surface"], 42);
    assert_eq!(
        p[CONTEXT_KEY],
        json!({ "implicit": ["surface", "surface_id", "caller_surface"] })
    );

    // 인자를 선언하지 않았고 채운 것도 없으면 예약 키를 붙이지 않는다.
    let mut p = params(json!({ "text": "x" }));
    apply_to_plugin_params(&mut p, |_| false, &[], &strict(), Some("42")).unwrap();
    assert_eq!(p, params(json!({ "text": "x" })));

    // 환경 값이 없으면 기존처럼 아무것도 채우지 않는다.
    let mut p = params(json!({}));
    apply_to_plugin_params(&mut p, both, &[], &strict(), None).unwrap();
    assert!(p.is_empty(), "{p:?}");
}

#[test]
fn an_explicit_surface_is_kept_and_not_reported_as_implicit() {
    for callers in [Vec::new(), callers(&[("XTEST_VENDOR_THREAD", THREAD)])] {
        let mut p = params(json!({ "surface": 7 }));
        apply_to_plugin_params(&mut p, |n| n == "surface", &callers, &[], Some("42")).unwrap();
        assert_eq!(p["surface"], 7, "{callers:?}");
        assert_eq!(p["surface_id"], 7, "{callers:?}");
        let implicit = p.get(CONTEXT_KEY).and_then(|c| c.get("implicit"));
        assert!(implicit.is_none(), "{callers:?}: {p:?}");
    }
}

#[test]
fn a_caller_with_a_stale_env_surface_gets_no_surface_fill_in_any_plugin_command() {
    let callers = callers(&[
        ("XTEST_VENDOR_THREAD", THREAD),
        ("XTEST_VENDOR_HOME", "/vendor-home"),
    ]);
    // 선언한 플러그인 명령이든 다른 플러그인 명령이든 같은 문맥을 받는다.
    for strict in [strict(), Vec::new()] {
        let mut p = params(json!({ "child": 3 }));
        apply_to_plugin_params(&mut p, both, &callers, &strict, Some("42")).unwrap();
        assert!(!p.contains_key("surface"), "{p:?}");
        assert!(!p.contains_key("surface_id"), "{p:?}");
        assert!(!p.contains_key("caller_surface"), "{p:?}");
        assert_eq!(p["child"], 3);
        assert_eq!(
            p[CONTEXT_KEY],
            json!({
                "provider": "vendor",
                "thread_id": THREAD,
                "vendor_home": std::path::absolute("/vendor-home").unwrap().to_str().unwrap(),
            })
        );
    }
}

#[test]
fn a_caller_that_keeps_the_env_surface_still_fills_it() {
    let mut keep = decl();
    keep.suppress_env_surface = false;
    let callers = resolve(
        &[source(keep)],
        env_of(&[("XTEST_VENDOR_THREAD", THREAD)]),
        no_program,
    );
    let mut p = params(json!({}));
    apply_to_plugin_params(&mut p, both, &callers, &[], Some("42")).unwrap();
    assert_eq!(p["surface"], 42);
    assert_eq!(p[CONTEXT_KEY]["provider"], "vendor");
    assert_eq!(
        p[CONTEXT_KEY]["implicit"],
        json!(["surface", "surface_id", "caller_surface"])
    );
}

#[test]
fn an_invalid_id_refuses_strict_commands_and_leaves_other_plugins_alone() {
    tasty_i18n::init("en");
    let callers = callers(&[("XTEST_VENDOR_THREAD", "not-a-thread")]);
    let mut p = params(json!({}));
    let err = apply_to_plugin_params(&mut p, both, &callers, &strict(), Some("42")).unwrap_err();
    assert!(err.contains("not-a-thread"), "{err}");
    assert!(p.is_empty(), "{p:?}");

    // 이 provider를 요구하지 않은 플러그인 명령은 값이 없을 때와 같다.
    let mut p = params(json!({}));
    apply_to_plugin_params(&mut p, both, &callers, &["other".into()], Some("42")).unwrap();
    assert_eq!(p["surface"], 42);
    assert!(p[CONTEXT_KEY].get("provider").is_none(), "{p:?}");
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_utf8_is_invalid() {
    use std::os::unix::ffi::OsStringExt;
    tasty_i18n::init("en");
    let env = |name: &str| match name {
        "XTEST_VENDOR_THREAD" => Some(OsString::from(THREAD)),
        "XTEST_VENDOR_HOME" => Some(OsString::from_vec(b"/vendor-\xff".to_vec())),
        _ => None,
    };
    let Caller::Invalid { provider, message } = caller_from(&decl(), env) else {
        panic!("a non UTF-8 path must be refused");
    };
    assert_eq!(provider, "vendor");
    assert!(message.contains("XTEST_VENDOR_HOME"), "{message}");
}

#[test]
fn the_first_declared_caller_wins_when_several_are_present() {
    let mut second = decl();
    second.provider = "second".into();
    second.id_env = "XTEST_SECOND_THREAD".into();
    let callers = resolve(
        &[source(decl()), source(second)],
        env_of(&[
            ("XTEST_VENDOR_THREAD", THREAD),
            ("XTEST_SECOND_THREAD", THREAD),
        ]),
        no_program,
    );
    assert_eq!(callers.len(), 2);
    let mut p = params(json!({}));
    apply_to_plugin_params(&mut p, both, &callers, &[], None).unwrap();
    assert_eq!(p[CONTEXT_KEY]["provider"], "vendor");
}

#[test]
fn the_reserved_key_cannot_come_from_arguments() {
    tasty_i18n::init("en");
    let mut p = params(json!({ CONTEXT_KEY: { "provider": "vendor" } }));
    let err = apply_to_plugin_params(&mut p, both, &[], &[], None).unwrap_err();
    assert!(err.contains(CONTEXT_KEY), "{err}");
}

#[test]
fn bundled_codex_declaration_keeps_the_previous_contract() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tasty-test-support/fixtures/official-plugins/com.tasty.codex");
    let manifest = tasty_plugin_manifest::Manifest::load(&dir).expect("codex manifest");
    let sources: Vec<_> = manifest
        .contributes
        .caller_context
        .into_iter()
        .map(|decl| CallerContextSource {
            plugin_id: manifest.id.clone(),
            decl,
        })
        .collect();
    let callers = resolve(
        &sources,
        env_of(&[("CODEX_THREAD_ID", THREAD), ("CODEX_HOME", "/codex-home")]),
        no_program,
    );
    let mut p = params(json!({}));
    apply_to_plugin_params(&mut p, both, &callers, &[], Some("42")).unwrap();
    assert_eq!(
        p[CONTEXT_KEY],
        json!({
            "provider": "codex",
            "thread_id": THREAD,
            "codex_home": std::path::absolute("/codex-home").unwrap().to_str().unwrap(),
        })
    );
    assert_eq!(sources[0].decl.executable.as_deref(), Some("codex"));
    let invalid = resolve(
        &sources,
        env_of(&[("CODEX_THREAD_ID", "surface-1")]),
        no_program,
    );
    assert!(matches!(&invalid[..], [Caller::Invalid { provider, .. }] if provider == "codex"));
}
