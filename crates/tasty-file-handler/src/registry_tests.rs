//! `FileHandlerRegistry` 단위 테스트.

use super::*;
use tasty_file_format::DetectorId;

fn load_host(reg: &FileHandlerRegistry) {
    reg.install_host_defaults(include_str!("defaults/default-file-handlers.toml"));
}

/// markdown surface 가 별도 plugin (`com.tasty.markdown`) 으로 분리됐다.
/// 기존 테스트가 가정하던 markdown handler/detector 동작을 보존하기 위해 plugin
/// install 을 흉내내는 헬퍼. handler id 는 `com.tasty.markdown/viewer`.
fn install_markdown_plugin(reg: &FileHandlerRegistry) {
    let decls = vec![HandlerDecl::<PluginHandlerActionDecl> {
        id: "viewer".into(),
        detector: "markdown".into(),
        priority: 50,
        display_name_i18n_key: None,
        disabled: false,
        action: PluginHandlerActionDecl::OpenSurface {
            surface_kind: "markdown".into(),
            param_key: "file".into(),
        },
    }];
    reg.install_plugin_handlers("com.tasty.markdown", &decls);
}

const MD_VIEWER_ID: &str = "com.tasty.markdown/viewer";

#[test]
fn markdown_plugin_loads_handlers_for_markdown() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    install_markdown_plugin(&reg);
    let v = reg.handlers_for(&DetectorId("markdown".into()));
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].id.as_str(), MD_VIEWER_ID);
    matches!(v[0].action, HandlerAction::OpenSurface { .. });
}

#[test]
fn plugin_handler_with_lower_priority_sorts_first() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    install_markdown_plugin(&reg);
    let decls = vec![HandlerDecl::<PluginHandlerActionDecl> {
        id: "viewer".into(),
        detector: "markdown".into(),
        priority: 10,
        display_name_i18n_key: None,
        disabled: false,
        action: PluginHandlerActionDecl::OpenSurface {
            surface_kind: "mdx_view".into(),
            param_key: "file".into(),
        },
    }];
    reg.install_plugin_handlers("com.example.mdx", &decls);
    let v = reg.handlers_for(&DetectorId("markdown".into()));
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].id.as_str(), "com.example.mdx/viewer");
    assert_eq!(v[1].id.as_str(), MD_VIEWER_ID);
}

#[test]
fn user_can_disable_plugin_handler() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    install_markdown_plugin(&reg);
    let user_toml = format!(
        r#"
        [[handler]]
        id = "{MD_VIEWER_ID}"
        disabled = true
    "#
    );
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);
    let v = reg.handlers_for(&DetectorId("markdown".into()));
    assert!(v.is_empty());
}

#[test]
fn uninstall_plugin_removes_only_its_handlers() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    install_markdown_plugin(&reg);
    let decls = vec![HandlerDecl::<PluginHandlerActionDecl> {
        id: "viewer".into(),
        detector: "markdown".into(),
        priority: 10,
        display_name_i18n_key: None,
        disabled: false,
        action: PluginHandlerActionDecl::Ipc {
            method: "com.example.mdx.open".into(),
        },
    }];
    reg.install_plugin_handlers("com.example.mdx", &decls);
    assert_eq!(reg.handlers_for(&DetectorId("markdown".into())).len(), 2);
    reg.uninstall_plugin("com.example.mdx");
    let v = reg.handlers_for(&DetectorId("markdown".into()));
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].id.as_str(), MD_VIEWER_ID);
}

#[test]
fn reload_user_config_replaces_user_handlers_keeps_plugin() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    install_markdown_plugin(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    // 1차: user 가 com.tasty.markdown/viewer priority 만 override.
    std::fs::write(
        &p,
        format!(
            r#"
                [[handler]]
                id = "{MD_VIEWER_ID}"
                priority = 10
            "#
        ),
    )
    .unwrap();
    reg.install_user_config(&p);
    let v = reg.handlers_for(&DetectorId("markdown".into()));
    assert_eq!(v[0].priority, 10);

    // 2차: user 가 priority override 빼고 새 user/handler 추가 → reload.
    std::fs::write(
        &p,
        r#"
            [[handler]]
            id = "user/my-md"
            detector = "markdown"
            priority = 20
            [handler.action]
            kind = "system"
        "#,
    )
    .unwrap();
    reg.reload_user_config(&p);

    let v = reg.handlers_for(&DetectorId("markdown".into()));
    // plugin viewer 는 plugin default priority (= 50) 로 복귀.
    let mdv = v.iter().find(|h| h.id.as_str() == MD_VIEWER_ID).unwrap();
    assert_eq!(mdv.priority, 50);
    // user/my-md 가 잡혀야 함.
    assert!(v.iter().any(|h| h.id.as_str() == "user/my-md"));
}

#[test]
fn reload_user_config_parse_error_keeps_previous_state() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        r#"
            [[handler]]
            id = "user/my-md"
            detector = "markdown"
            priority = 20
            [handler.action]
            kind = "system"
        "#,
    )
    .unwrap();
    reg.install_user_config(&p);
    assert!(
        reg.handlers_for(&DetectorId("markdown".into()))
            .iter()
            .any(|h| h.id.as_str() == "user/my-md")
    );

    // 파일을 깨뜨림 → reload 거부, 기존 user 항목 보존.
    std::fs::write(&p, "[[handler\n id = broken").unwrap();
    reg.reload_user_config(&p);
    assert!(
        reg.handlers_for(&DetectorId("markdown".into()))
            .iter()
            .any(|h| h.id.as_str() == "user/my-md")
    );
}

#[test]
fn handlers_for_priority_tiebreak_uses_owner_order() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    install_markdown_plugin(&reg);
    // 두 plugin + user 모두 priority 50 (markdown plugin viewer 도 50)
    let p = vec![HandlerDecl::<PluginHandlerActionDecl> {
        id: "viewer".into(),
        detector: "markdown".into(),
        priority: 50,
        display_name_i18n_key: None,
        disabled: false,
        action: PluginHandlerActionDecl::Ipc {
            method: "com.example.x.open".into(),
        },
    }];
    reg.install_plugin_handlers("com.example.x", &p);
    let user_toml = r#"
        [[handler]]
        id = "user/my-viewer"
        detector = "markdown"
        priority = 50
        [handler.action]
        kind = "system"
    "#;
    let dir = tempfile::tempdir().unwrap();
    let pth = dir.path().join("file-handlers.toml");
    std::fs::write(&pth, user_toml).unwrap();
    reg.install_user_config(&pth);

    let v = reg.handlers_for(&DetectorId("markdown".into()));
    // priority 모두 50 → tie-break: user > plugin. plugin 끼리는 id 알파벳 순.
    let owners: Vec<&str> = v.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(owners[0], "user/my-viewer");
    assert_eq!(owners[1], "com.example.x/viewer");
    assert_eq!(owners[2], MD_VIEWER_ID);
}

#[test]
fn all_handlers_returns_every_enabled() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    let v = reg.all_handlers();
    // host default 2개 (html-system, directory-system).
    // markdown-viewer 는 com.tasty.markdown plugin 이 제공, image-viewer 는 com.tasty.image plugin.
    assert_eq!(v.len(), 2);
}

// ── cross-module integration: file_format + file_handler ──────────
//
// 시나리오: 사용자가 PDF 로 새 detector 와 핸들러를 등록한 뒤
// 1) `identify(*.pdf)` 가 user detector 를 반환하고
// 2) `handlers_for(pdf)` 가 user handler 를 반환하는지 확인.

use tasty_file_format::{DetectDepth, FileFormatRegistry, FileTarget};

fn make_user_toml(toml_text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(&p, toml_text).unwrap();
    dir
}

#[test]
fn user_pdf_detector_and_handler_round_trip() {
    let formats = FileFormatRegistry::new();
    formats.install_host_defaults(tasty_file_format::HOST_DEFAULTS_TOML);

    let handlers = FileHandlerRegistry::new();
    load_host(&handlers);

    let user_toml = r#"
        [[detector]]
        id = "pdf"
        [[detector.rule]]
        kind = "extension"
        values = ["pdf"]

        [[handler]]
        id = "user/pdf-preview"
        detector = "pdf"
        priority = 30
        [handler.action]
        kind = "system"
    "#;
    let dir = make_user_toml(user_toml);
    let p = dir.path().join("file-handlers.toml");
    formats.install_user_config(&p);
    handlers.install_user_config(&p);

    // identify
    let id = formats.identify(
        &FileTarget::new(std::path::PathBuf::from("docs/spec.pdf")),
        DetectDepth::Cheap,
    );
    assert_eq!(id, Some(tasty_file_format::DetectorId("pdf".into())));

    // handlers_for
    let v = handlers.handlers_for(&tasty_file_format::DetectorId("pdf".into()));
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].id.as_str(), "user/pdf-preview");
    assert!(matches!(v[0].action, HandlerAction::System));
}

// ── export_user_config / save_user_config (MD4) ─────────────────────

#[test]
fn export_user_handler_emits_only_user_origin() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    install_markdown_plugin(&reg);
    // 사용자가 com.tasty.markdown/viewer 를 disable 하고 자기 핸들러 user/my-md 추가.
    let user_toml = format!(
        r#"
        [[handler]]
        id = "{MD_VIEWER_ID}"
        disabled = true

        [[handler]]
        id = "user/my-md"
        detector = "markdown"
        priority = 20
        display_name_i18n_key = "user.md"
        [handler.action]
        kind = "system"
    "#
    );
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let exported = reg.export_user_config();
    assert!(exported.contains(MD_VIEWER_ID));
    assert!(exported.contains("disabled = true"));
    assert!(exported.contains("user/my-md"));
    assert!(exported.contains("\"markdown\""));
    // plugin 의 markdown viewer action(OpenSurface) 는 user 가 손대지 않았으므로
    // export 결과의 plugin viewer entry 에는 action 이 없어야 한다.
    let lines: Vec<&str> = exported.split("[[handler]]").collect();
    let md_section = lines
        .iter()
        .find(|s| s.contains(MD_VIEWER_ID))
        .expect("section present");
    assert!(
        !md_section.contains("kind = \"open_surface\""),
        "user export should not leak plugin action: {md_section}"
    );
}

#[test]
fn export_user_handler_round_trip() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    let user_toml = r#"
        [[handler]]
        id = "user/my-md"
        detector = "markdown"
        priority = 25
        [handler.action]
        kind = "open_surface"
        surface_kind = "markdown"
        param_key = "file"
    "#;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(&p, user_toml).unwrap();
    reg.install_user_config(&p);

    let exported = reg.export_user_config();

    let reg2 = FileHandlerRegistry::new();
    load_host(&reg2);
    let p2 = dir.path().join("re-emit.toml");
    std::fs::write(&p2, &exported).unwrap();
    reg2.install_user_config(&p2);

    let v1 = reg.handlers_for(&DetectorId("markdown".into()));
    let v2 = reg2.handlers_for(&DetectorId("markdown".into()));
    let ids1: Vec<_> = v1.iter().map(|h| h.id.as_str().to_string()).collect();
    let ids2: Vec<_> = v2.iter().map(|h| h.id.as_str().to_string()).collect();
    assert_eq!(ids1, ids2);
    // user/my-md 의 priority 가 보존되었는지
    let h2 = v2
        .iter()
        .find(|h| h.id.as_str() == "user/my-md")
        .expect("user handler present");
    assert_eq!(h2.priority, 25);
}

#[test]
fn save_user_handler_atomic_write() {
    let reg = FileHandlerRegistry::new();
    let user_toml = r#"
        [[handler]]
        id = "user/my-md"
        detector = "markdown"
        priority = 25
        [handler.action]
        kind = "system"
    "#;
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.toml");
    std::fs::write(&src, user_toml).unwrap();
    reg.install_user_config(&src);

    let dst = dir.path().join("subdir").join("dst.toml");
    reg.save_user_config(&dst).unwrap();
    assert!(dst.exists());
    let written = std::fs::read_to_string(&dst).unwrap();
    assert!(written.contains("user/my-md"));
    assert!(written.contains("kind = \"system\""));
}

#[test]
fn export_empty_when_no_user_contributions() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    assert_eq!(reg.export_user_config(), "");
}

#[test]
fn directory_target_does_not_match_file_detectors() {
    let formats = FileFormatRegistry::new();
    formats.install_host_defaults(tasty_file_format::HOST_DEFAULTS_TOML);
    let handlers = FileHandlerRegistry::new();
    load_host(&handlers);

    let dir = tempfile::tempdir().unwrap();
    let target = FileTarget::new(dir.path().to_path_buf());
    let id = formats
        .identify(&target, DetectDepth::Cheap)
        .expect("directory should identify");
    assert_eq!(id.as_str(), "$directory");
    let v = handlers.handlers_for(&id);
    assert!(!v.is_empty(), "host should register a directory handler");
}

// ── DetectorInfo 주입 (Phase E ME1) ──────────────────────────────

/// com.tasty.markdown plugin 의 detector contribution 흉내 (md/markdown 확장자).
fn install_markdown_plugin_detector(formats: &tasty_file_format::FileFormatRegistry) {
    use tasty_file_format::{DetectorDecl, DetectorRuleDecl};
    let decls = vec![DetectorDecl {
        id: "markdown".into(),
        display_name_i18n_key: Some("file_handler.format.markdown".into()),
        icon: None,
        disabled: None,
        rule: vec![DetectorRuleDecl::Extension {
            values: vec!["md".into(), "markdown".into()],
        }],
    }];
    formats.install_plugin_detectors("com.tasty.markdown", &decls);
}

#[test]
fn attach_detector_info_stores_arc_and_returns_clone() {
    use tasty_file_format::FileFormatRegistry;
    let formats = std::sync::Arc::new(FileFormatRegistry::new());
    formats.install_host_defaults(tasty_file_format::HOST_DEFAULTS_TOML);
    install_markdown_plugin_detector(&formats);

    let handlers = FileHandlerRegistry::new();
    assert!(handlers.detector_info().is_none());

    handlers.attach_detector_info(formats.clone());
    let info = handlers
        .detector_info()
        .expect("detector_info should be Some after attach");
    // 주입된 info 로 markdown 의 광고된 확장자 조회 가능.
    let exts = info.advertised_extensions(&DetectorId("markdown".into()));
    assert!(exts.contains(&"md".to_string()));
}

// ── 부팅 시 enabled plugin contribute 자동 등록 (boot registration) ──────
//
// `discover_and_start()` 는 enabled plugin 의 `contributes.detector` /
// `contributes.handler`(둘 다 `Vec<serde_json::Value>`) 를 trait-impl
// (`FileFormatRegistryPort` / `FileHandlerRegistryPort`) 의 JSON 디코드 경로로
// install 한다. 아래 테스트는 그 부팅 경로를 manifest JSON 값 그대로 모사한다.

/// markdown manifest 의 `[[contributes.detector]]` 와 동일한 JSON 값.
fn markdown_detector_json() -> Vec<serde_json::Value> {
    vec![serde_json::json!({
        "id": "markdown",
        "display_name_i18n_key": "file_handler.format.markdown",
        "rule": [{ "kind": "extension", "values": ["md", "markdown"] }],
    })]
}

/// markdown manifest 의 `[[contributes.handler]]` 와 동일한 JSON 값.
fn markdown_handler_json() -> Vec<serde_json::Value> {
    vec![serde_json::json!({
        "id": "viewer",
        "detector": "markdown",
        "priority": 50,
        "display_name_i18n_key": "file_handler.host.markdown-viewer",
        "action": { "kind": "open_surface", "surface_kind": "markdown", "param_key": "file" },
    })]
}

#[test]
fn boot_registration_via_manifest_json_enables_dispatch() {
    use tasty_plugin_protocol::host_port::{FileFormatRegistryPort, FileHandlerRegistryPort};
    let formats = FileFormatRegistry::new();
    formats.install_host_defaults(tasty_file_format::HOST_DEFAULTS_TOML);
    let handlers = FileHandlerRegistry::new();
    load_host(&handlers);

    // 부팅 경로와 동일: enabled plugin 의 manifest contribute(JSON) 를 install.
    FileFormatRegistryPort::install_plugin_detectors(
        &formats,
        "com.tasty.markdown",
        &markdown_detector_json(),
    );
    FileHandlerRegistryPort::install_plugin_handlers(
        &handlers,
        "com.tasty.markdown",
        &markdown_handler_json(),
    );

    // detector 가 .md 를 식별하고, 그 detector 로 핸들러가 조회되어야 한다
    // (= 부팅 직후 별도 enable 없이 .md 디스패치 가능).
    let id = formats.identify(
        &FileTarget::new(std::path::PathBuf::from("README.md")),
        DetectDepth::Cheap,
    );
    assert_eq!(id, Some(DetectorId("markdown".into())));
    let v = handlers.handlers_for(&DetectorId("markdown".into()));
    assert!(v.iter().any(|h| h.id.as_str() == MD_VIEWER_ID));
}

#[test]
fn boot_registration_idempotent_and_first_is_deterministic() {
    use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;
    let handlers = FileHandlerRegistry::new();
    load_host(&handlers);

    // 같은 detector 에 priority 동일(50) 핸들러 둘 — tie-break 는 plugin id 알파벳순.
    let other = vec![serde_json::json!({
        "id": "viewer",
        "detector": "markdown",
        "priority": 50,
        "action": { "kind": "ipc", "method": "com.example.mdx.open" },
    })];

    // 부팅 등록 (1회).
    FileHandlerRegistryPort::install_plugin_handlers(
        &handlers,
        "com.tasty.markdown",
        &markdown_handler_json(),
    );
    FileHandlerRegistryPort::install_plugin_handlers(&handlers, "com.example.mdx", &other);
    let first_id = handlers.handlers_for(&DetectorId("markdown".into()))[0]
        .id
        .as_str()
        .to_string();

    // 멱등: disable→enable 모사로 같은 plugin 을 재install 해도 중복 누적 없음
    // (push_contribution 이 같은 owner 를 retain 으로 교체).
    FileHandlerRegistryPort::install_plugin_handlers(
        &handlers,
        "com.tasty.markdown",
        &markdown_handler_json(),
    );
    FileHandlerRegistryPort::install_plugin_handlers(&handlers, "com.example.mdx", &other);

    let v = handlers.handlers_for(&DetectorId("markdown".into()));
    assert_eq!(v.len(), 2, "재install 후에도 핸들러 2개 — 중복 누적 없음");
    // 다중 핸들러 1순위 자동선택의 결정론: 재install 후에도 first 동일.
    assert_eq!(v[0].id.as_str(), first_id);
    // priority 동일 → plugin id 알파벳순. com.example.mdx < com.tasty.markdown.
    assert_eq!(v[0].id.as_str(), "com.example.mdx/viewer");
}

#[test]
fn attach_detector_info_second_call_is_ignored() {
    use tasty_file_format::FileFormatRegistry;
    let formats_a = std::sync::Arc::new(FileFormatRegistry::new());
    let formats_b = std::sync::Arc::new(FileFormatRegistry::new());
    formats_a.install_host_defaults(tasty_file_format::HOST_DEFAULTS_TOML);
    install_markdown_plugin_detector(&formats_a);
    // formats_b 는 host default + plugin detector 안 깐 빈 registry.

    let handlers = FileHandlerRegistry::new();
    handlers.attach_detector_info(formats_a.clone());
    // 2번째 호출은 무시 → formats_b 가 주입되지 않음.
    handlers.attach_detector_info(formats_b.clone());

    let info = handlers.detector_info().expect("Some after first attach");
    // 첫번째 (formats_a) 가 보유한 markdown 광고가 보여야 함.
    let exts = info.advertised_extensions(&DetectorId("markdown".into()));
    assert!(!exts.is_empty(), "first registry should still be attached");
}

/// poison 된 레지스트리가 **조용히 아무것도 안 하는** 대신 계속 동작한다.
///
/// 이전에는 락 획득 19 곳이 전부 `read().ok()?` / `Err(_) => return` 이라, poison
/// 이후 handler 설치는 무음 no-op 이 되고 조회는 빈 결과를 돌려줬다. 증상은
/// "그 확장자가 안 열린다" 인데 관측 지점이 0 이었다. `upsert_user_handler` 는 한술 더
/// 떠 poison 을 `InvalidShortName("lock poisoned")` 으로 보고해, 사용자에게 id 형식이
/// 틀렸다고 말하면서 진짜 원인은 남기지 않았다.
#[test]
fn a_poisoned_registry_still_installs_and_resolves() {
    let reg = std::sync::Arc::new(FileHandlerRegistry::new());

    let held = std::sync::Arc::clone(&reg);
    let joined = std::thread::spawn(move || {
        let _guard = held.inner.write().expect("fresh rwlock");
        panic!("a thread dies while holding the registry");
    })
    .join();
    assert!(joined.is_err(), "그 스레드는 패닉했어야 한다");
    assert!(reg.inner.read().is_err(), "poison 됐어야 한다");

    load_host(&reg);
    install_markdown_plugin(&reg);

    assert!(
        reg.handler(&HandlerId(MD_VIEWER_ID.to_string())).is_some(),
        "poison 이후에도 설치가 반영돼야 한다"
    );
    assert!(
        !reg.all_handlers().is_empty(),
        "poison 이후에도 조회가 빈 결과가 아니어야 한다"
    );
}

/// reload 는 적용하지 않은 user 항목을 사유와 함께 돌려준다 — 경고 로그에만 있던 사실이다.
/// 접두사 없는 id 는 설치 전에, detector·action 이 빈 id 는 finalize 에서 버려진다.
/// 기존 handler 를 메타만 덮는 patch 와 온전한 user 항목은 보고에 없다.
#[test]
fn reload_user_config_reports_the_entries_it_dropped() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        r#"
            [[handler]]
            id = "md-as-html"
            detector = "markdown"
            [handler.action]
            kind = "system"

            [[handler]]
            id = "user/no-action"
            detector = "markdown"

            [[handler]]
            id = "user/good"
            detector = "markdown"
            [handler.action]
            kind = "system"

            [[handler]]
            id = "host/html-system"
            priority = 7
        "#,
    )
    .unwrap();

    let rejected = reg.reload_user_config(&p);

    assert_eq!(
        rejected,
        vec![
            RejectedUserHandler {
                id: "md-as-html".into(),
                reason: UserHandlerRejectReason::MissingOwnerPrefix,
            },
            RejectedUserHandler {
                id: "user/no-action".into(),
                reason: UserHandlerRejectReason::MissingDetectorOrAction,
            },
        ]
    );
    // 보고는 finalize 의 실제 판정과 같아야 한다 — 보고한 것은 없고, 안 한 것은 있다.
    let ids: Vec<String> = reg
        .all_handlers()
        .into_iter()
        .map(|h| h.id.as_str().to_string())
        .collect();
    assert!(
        !ids.iter()
            .any(|i| i == "user/no-action" || i == "md-as-html")
    );
    assert!(ids.iter().any(|i| i == "user/good"));
    let patched = reg
        .get(&HandlerId("host/html-system".into()))
        .expect("host handler 는 patch 로 살아 있다");
    assert_eq!(patched.priority, 7);
}

/// 적용하지 않은 것이 없으면 빈 목록이다. 파싱 실패로 reload 가 멈춘 경우도 항목을 모르므로 빈 목록이다.
#[test]
fn reload_user_config_reports_nothing_when_nothing_was_dropped() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        r#"
            [[handler]]
            id = "user/good"
            detector = "markdown"
            [handler.action]
            kind = "system"
        "#,
    )
    .unwrap();
    assert!(reg.reload_user_config(&p).is_empty());

    std::fs::write(&p, "[[handler\n id = broken").unwrap();
    assert!(reg.reload_user_config(&p).is_empty());
}

/// 사유 코드는 응답에 나가는 계약이다.
#[test]
fn reject_reason_codes_are_stable() {
    assert_eq!(
        UserHandlerRejectReason::MissingOwnerPrefix.as_str(),
        "missing_owner_prefix"
    );
    assert_eq!(
        UserHandlerRejectReason::MissingDetectorOrAction.as_str(),
        "missing_detector_or_action"
    );
    assert_eq!(
        UserHandlerRejectReason::TargetNotContributed.as_str(),
        "target_not_contributed"
    );
}

/// plugin handler 를 patch 하는 user 항목은 그 plugin 이 contribute 하지 않은 동안 등록되지
/// 않는다. 그런데 **버려진 것이 아니다** — 남아 있다가 plugin 이 contribute 하면 적용된다. 그래서
/// 사유는 `missing_detector_or_action` 이 아니라 `target_not_contributed` 다. 오타 id 도 같은
/// 모양이라 같은 사유로 보고된다(둘은 여기서 가를 수 없다).
#[test]
fn a_patch_whose_plugin_has_not_contributed_is_reported_as_target_not_contributed() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        format!(
            r#"
                [[handler]]
                id = "{MD_VIEWER_ID}"
                priority = 10

                [[handler]]
                id = "com.tasty.markdown/viewer-typo"
                priority = 10
            "#
        ),
    )
    .unwrap();

    let rejected = reg.reload_user_config(&p);

    assert_eq!(
        rejected,
        vec![
            RejectedUserHandler {
                id: MD_VIEWER_ID.into(),
                reason: UserHandlerRejectReason::TargetNotContributed,
            },
            RejectedUserHandler {
                id: "com.tasty.markdown/viewer-typo".into(),
                reason: UserHandlerRejectReason::TargetNotContributed,
            },
        ]
    );
}

/// 대상 plugin 이 contribute 한 뒤 같은 파일로 reload 하면 그 patch 는 보고에서 빠지고 실제로
/// 적용된다. 오타 id 는 대상이 여전히 없으므로 남는다.
#[test]
fn a_patch_leaves_the_report_once_its_plugin_contributes() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        format!(
            r#"
                [[handler]]
                id = "{MD_VIEWER_ID}"
                priority = 10

                [[handler]]
                id = "com.tasty.markdown/viewer-typo"
                priority = 10
            "#
        ),
    )
    .unwrap();
    assert_eq!(reg.reload_user_config(&p).len(), 2);

    install_markdown_plugin(&reg);
    // plugin 이 contribute 하는 것만으로 patch 가 적용된다 — reload 를 기다리지 않는다.
    let viewer = reg
        .get(&HandlerId(MD_VIEWER_ID.into()))
        .expect("plugin 이 contribute 한 뒤에는 handler 가 있다");
    assert_eq!(
        viewer.priority, 10,
        "plugin 이 나중에 와도 user patch 의 priority 가 이겨야 한다"
    );
    let rejected = reg.reload_user_config(&p);

    assert_eq!(
        rejected,
        vec![RejectedUserHandler {
            id: "com.tasty.markdown/viewer-typo".into(),
            reason: UserHandlerRejectReason::TargetNotContributed,
        }]
    );
    let viewer = reg
        .get(&HandlerId(MD_VIEWER_ID.into()))
        .expect("plugin 이 contribute 한 뒤에는 patch 가 적용된 handler 가 있다");
    assert_eq!(viewer.priority, 10);
}

/// 부팅 순서 — user 설정을 plugin 보다 **먼저** 읽는다. 그래도 plugin handler 를 겨냥한 user
/// patch 가 이긴다. 설치 순서로 병합하던 때는 plugin 의 priority 가 user 값을 덮었다.
#[test]
fn a_user_patch_wins_over_a_plugin_installed_after_the_boot_load() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        format!(
            r#"
                [[handler]]
                id = "{MD_VIEWER_ID}"
                priority = 10
            "#
        ),
    )
    .unwrap();

    reg.install_user_config(&p);
    install_markdown_plugin(&reg);

    let viewer = reg.get(&HandlerId(MD_VIEWER_ID.into())).unwrap();
    assert_eq!(
        viewer.priority, 10,
        "user patch 가 plugin 기본값 50 을 덮어야 한다"
    );
    assert!(matches!(viewer.owner, HandlerOwner::User));
}

/// reload 로 user patch 를 먼저 넣고 plugin 이 **reload 없이** 나중에 contribute 해도 user
/// patch 가 이긴다 — plugin 을 끄고 다시 켜는 것과 같은 흐름이다.
#[test]
fn a_user_patch_wins_over_a_plugin_that_contributes_later_without_a_reload() {
    let reg = FileHandlerRegistry::new();
    load_host(&reg);
    install_markdown_plugin(&reg);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("file-handlers.toml");
    std::fs::write(
        &p,
        format!(
            r#"
                [[handler]]
                id = "{MD_VIEWER_ID}"
                priority = 10
            "#
        ),
    )
    .unwrap();
    assert!(reg.reload_user_config(&p).is_empty());

    reg.uninstall_plugin("com.tasty.markdown");
    install_markdown_plugin(&reg);

    let viewer = reg.get(&HandlerId(MD_VIEWER_ID.into())).unwrap();
    assert_eq!(
        viewer.priority, 10,
        "다시 contribute 한 plugin 이 user patch 를 덮으면 안 된다"
    );
}
