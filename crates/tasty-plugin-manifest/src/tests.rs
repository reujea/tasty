//! Manifest 파싱/검증 단위 테스트.

use super::*;

fn parse(src: &str) -> anyhow::Result<Manifest> {
    let m: Manifest = toml::from_str(src)?;
    m.validate()?;
    Ok(m)
}

#[test]
fn bundle_defaults_true_and_parses_false() {
    // 미지정 → true (기존 매니페스트가 배포에서 빠지지 않는다).
    let base = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(parse(base).expect("parses").bundle);

    // bundle = false → 배포 제외 플래그.
    let excluded = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        bundle = false
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(!parse(excluded).expect("parses").bundle);
}

#[test]
fn rejects_unsupported_api() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "999"
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn rejects_unsupported_manifest_version() {
    let s = r#"
        manifest_version = 99
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn rejects_invalid_plugin_id_no_dot() {
    let s = r#"
        manifest_version = 1
        id = "explorer"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn rejects_invalid_kind_uppercase() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "Explorer"
        display_name_i18n_key = "k"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn accepts_minimal_valid() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1.0"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.id, "com.example.x");
}

#[test]
fn rejects_unknown_permission() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        permissions = ["fs.read", "made.up.permission"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("unknown permission"), "got: {err}");
}

#[test]
fn parsed_permissions_returns_enum_set() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        permissions = ["fs.read", "surface.write", "notification"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    let perms = m.parsed_permissions().expect("should resolve");
    assert!(perms.contains(&Permission::FsRead));
    assert!(perms.contains(&Permission::SurfaceWrite));
    assert!(perms.contains(&Permission::Notification));
    assert_eq!(perms.len(), 3);
}

#[test]
fn accepts_full_manifest() {
    // TOML rule: top-level keys must come before any table headers.
    let s = r#"
        manifest_version = 1
        id = "com.example.explorer"
        name = "Explorer"
        version = "1.2.0"
        authors = ["alice@example.com"]
        description = "File explorer"
        homepage = "https://example.com"
        api_version = "1"
        permissions = ["fs.read", "surface.read"]

        [entry]
        type = "process"
        command = "tasty-plugin-explorer"
        args = []

        [[surface_kinds]]
        kind = "explorer"
        display_name_i18n_key = "surface.kind.explorer"
        icon = "folder"

        [[contributes.commands]]
        id = "explorer.refresh"
        title_i18n_key = "explorer.command.refresh"
        default_keybinding = "F5"
        scope = "surface"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.surface_kinds.len(), 1);
    assert_eq!(m.surface_kinds[0].kind, "explorer");
    assert_eq!(m.permissions.len(), 2);
    assert_eq!(m.contributes.commands.len(), 1);
    // binding_mode 미지정 → Independent 기본값
    assert_eq!(
        m.contributes.commands[0].binding_mode,
        BindingMode::Independent
    );
    // lang_dir 미지정 → "lang" 기본값
    assert_eq!(m.lang_dir, "lang");
}

#[test]
fn binding_mode_independent() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        binding_mode = "independent"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(
        m.contributes.commands[0].binding_mode,
        BindingMode::Independent
    );
}

#[test]
fn binding_mode_inherit() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.copy"
        title_i18n_key = "x.copy"
        binding_mode = "inherit:clipboard.copy"
    "#;
    let m = parse(s).expect("should parse");
    match &m.contributes.commands[0].binding_mode {
        BindingMode::InheritHost(action) => assert_eq!(action, "clipboard.copy"),
        _ => panic!("expected InheritHost"),
    }
}

#[test]
fn binding_mode_inherit_empty_action_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.copy"
        title_i18n_key = "x.copy"
        binding_mode = "inherit:"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn binding_mode_unknown_value_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        binding_mode = "wat"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn command_id_must_be_unique() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo.2"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn command_invalid_id_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "X-Foo!"
        title_i18n_key = "x.foo"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(
        err.contains("invalid contributes.commands id"),
        "got: {err}"
    );
}

#[test]
fn command_global_scope_single_key_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        default_keybinding = "F5"
        scope = "global"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("combination key"), "got: {err}");
}

#[test]
fn command_global_scope_combo_key_accepted() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        default_keybinding = "ctrl+shift+g"
        scope = "global"
    "#;
    parse(s).expect("should parse");
}

#[test]
fn command_surface_scope_single_key_accepted() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        default_keybinding = "F5"
        scope = "surface"
    "#;
    parse(s).expect("should parse");
}

#[test]
fn command_global_scope_is_default_when_unspecified() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.contributes.commands[0].scope, CommandScope::Global);
}

#[test]
fn command_inherit_unknown_host_action_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        binding_mode = "inherit:tab.new"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("inherit whitelist"), "got: {err}");
}

#[test]
fn command_inherit_known_host_action_accepted() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.copy"
        title_i18n_key = "x.copy"
        binding_mode = "inherit:clipboard.copy"
    "#;
    parse(s).expect("should parse");
}

#[test]
fn command_action_invalid_event_key_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        default_keybinding = "ctrl+shift+g"
        [contributes.commands.action]
        kind = "event"
        event_key = "not_a_dotted_key"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("concrete event key"), "got: {err}");
}

#[test]
fn command_action_open_popup_requires_ui_popup_permission() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        default_keybinding = "ctrl+shift+g"
        [contributes.commands.action]
        kind = "open_popup"
        popup_id = "com.example.x/main"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("'ui.popup'"), "got: {err}");
}

#[test]
fn command_action_open_popup_unknown_popup_ref_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        permissions = ["ui.popup"]
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        default_keybinding = "ctrl+shift+g"
        [contributes.commands.action]
        kind = "open_popup"
        popup_id = "com.example.x/ghost"

        [[contributes.popup]]
        id = "main"
        [contributes.popup.trigger]
        kind = "ipc"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("unknown popup"), "got: {err}");
}

#[test]
fn command_action_open_popup_known_ref_accepted() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        permissions = ["ui.popup"]
        [entry]
        type = "process"
        command = "x"
        [[contributes.commands]]
        id = "x.foo"
        title_i18n_key = "x.foo"
        default_keybinding = "ctrl+shift+g"
        [contributes.commands.action]
        kind = "open_popup"
        popup_id = "com.example.x/main"

        [[contributes.popup]]
        id = "main"
        [contributes.popup.trigger]
        kind = "ipc"
    "#;
    let m = parse(s).expect("should parse");
    assert!(matches!(
        &m.contributes.commands[0].action,
        Some(ToolAction::OpenPopup { .. })
    ));
}

#[test]
fn manifest_with_ipc_namespace_parses() {
    let s = r#"
        manifest_version = 1
        id = "com.example.codex"
        name = "Codex"
        version = "0.1.0"
        api_version = "1"
        [entry]
        type = "process"
        command = "tasty-plugin-codex"
        [[contributes.ipc_namespace]]
        prefix = "codex"
        description_i18n_key = "codex.namespace.desc"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.contributes.ipc_namespace.len(), 1);
    assert_eq!(m.contributes.ipc_namespace[0].prefix, "codex");
}

#[test]
fn manifest_with_cli_parses() {
    let s = r#"
        manifest_version = 1
        id = "com.example.codex"
        name = "Codex"
        version = "0.1.0"
        api_version = "1"

        [entry]
        type = "process"
        command = "tasty-plugin-codex"

        [[contributes.ipc_namespace]]
        prefix = "codex"

        [[contributes.cli]]
        name = "codex"
        subcommands = [
          { name = "spawn", ipc_method = "codex.spawn", args = "spawn_args" },
          { name = "wait",  ipc_method = "codex.wait",  args = "no_args" },
        ]

        [contributes.cli.arg_groups.spawn_args]
        flags = [
          { name = "surface", type = "u32",    flag = "--surface", required = false },
          { name = "prompt",  type = "string", flag = "--prompt",  required = false },
        ]

        [contributes.cli.arg_groups.no_args]
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.contributes.cli.len(), 1);
    let cli = &m.contributes.cli[0];
    assert_eq!(cli.name, "codex");
    assert_eq!(cli.subcommands.len(), 2);
    assert!(cli.arg_groups.contains_key("spawn_args"));
    assert!(cli.arg_groups.contains_key("no_args"));
    let spawn_args = &cli.arg_groups["spawn_args"];
    assert_eq!(spawn_args.flags.len(), 2);
    assert_eq!(spawn_args.flags[0].ty, CliArgType::U32);
}

#[test]
fn manifest_reserved_ipc_prefix_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.evil"
        name = "Evil"
        version = "0.1.0"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.ipc_namespace]]
        prefix = "surface"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("reserved"), "got: {err}");
}

/// 호스트 명령과 겹치는 CLI 이름은 **매니페스트 층에서 거절하지 않는다** — 이 크레이트는
/// 실제 clap 명령 집합을 볼 수 없어 손목록으로만 판정할 수 있었고, 그 목록은 늙었다.
/// 판정은 등록 시점 한 자리(`tasty-cli` 의 `build_augmented_cli`)에만 있다.
#[test]
fn manifest_does_not_judge_cli_names_against_host_commands() {
    let s = r#"
        manifest_version = 1
        id = "com.example.evil"
        name = "Evil"
        version = "0.1.0"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.cli]]
        name = "split"
    "#;
    let m = parse(s).expect("호스트 명령 이름이라는 이유로 매니페스트를 거절하지 않는다");
    assert_eq!(m.contributes.cli[0].name, "split");
}

#[test]
fn manifest_cli_args_ref_missing_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.codex"
        name = "Codex"
        version = "0.1.0"
        api_version = "1"

        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "codex"

        [[contributes.cli]]
        name = "codex"
        subcommands = [
          { name = "spawn", ipc_method = "codex.spawn", args = "missing" },
        ]
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("unknown arg group"), "got: {err}");
}

#[test]
fn manifest_ipc_method_outside_namespace_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.codex"
        name = "Codex"
        version = "0.1.0"
        api_version = "1"

        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "codex"

        [[contributes.cli]]
        name = "codex"
        subcommands = [
          { name = "evil", ipc_method = "claude.spawn", args = "no_args" },
        ]

        [contributes.cli.arg_groups.no_args]
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("not declared"), "got: {err}");
}

#[test]
fn manifest_cli_ipc_method_no_prefix_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.codex"
        name = "Codex"
        version = "0.1.0"
        api_version = "1"

        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "codex"

        [[contributes.cli]]
        name = "codex"
        subcommands = [
          { name = "evil", ipc_method = "noprefix", args = "no_args" },
        ]

        [contributes.cli.arg_groups.no_args]
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("no namespace prefix"), "got: {err}");
}

#[test]
fn manifest_duplicate_ipc_prefix_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.codex"
        name = "Codex"
        version = "0.1.0"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[contributes.ipc_namespace]]
        prefix = "codex"
        [[contributes.ipc_namespace]]
        prefix = "codex"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn manifest_flag_without_double_dash_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.codex"
        name = "Codex"
        version = "0.1.0"
        api_version = "1"

        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "codex"

        [[contributes.cli]]
        name = "codex"
        subcommands = [
          { name = "spawn", ipc_method = "codex.spawn", args = "spawn_args" },
        ]

        [contributes.cli.arg_groups.spawn_args]
        flags = [
          { name = "surface", type = "u32", flag = "-s", required = false },
        ]
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("must start with '--'"), "got: {err}");
}

#[test]
fn permission_ipc_invoke_token_round_trip() {
    let p = Permission::from_token("ipc.invoke:codex").expect("should parse");
    match &p {
        Permission::IpcInvoke(prefix) => assert_eq!(prefix, "codex"),
        _ => panic!("expected IpcInvoke"),
    }
    assert_eq!(p.as_token(), "ipc.invoke:codex");
}

#[test]
fn permission_ipc_invoke_empty_prefix_rejected() {
    assert!(Permission::from_token("ipc.invoke:").is_none());
}

#[test]
fn permission_ipc_invoke_invalid_prefix_rejected() {
    // 대문자 거부 (lowercase ascii only)
    assert!(Permission::from_token("ipc.invoke:Codex").is_none());
    // '.' 포함 거부
    assert!(Permission::from_token("ipc.invoke:co.dex").is_none());
    // 숫자 시작 거부
    assert!(Permission::from_token("ipc.invoke:1codex").is_none());
}

#[test]
fn permission_ipc_invoke_reserved_prefix_rejected() {
    // 호스트 예약 prefix는 plugin이 점유할 수 없으므로 토큰도 거부.
    assert!(Permission::from_token("ipc.invoke:surface").is_none());
    assert!(Permission::from_token("ipc.invoke:pane").is_none());
}

#[test]
fn manifest_accepts_ipc_invoke_permission() {
    let s = r#"
        manifest_version = 1
        id = "com.example.codex-helper"
        name = "Helper"
        version = "0.1.0"
        api_version = "1"
        permissions = ["ipc.invoke:codex", "surface.read"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    let perms = m.parsed_permissions().expect("resolve");
    assert!(perms.contains(&Permission::IpcInvoke("codex".into())));
    assert!(perms.contains(&Permission::SurfaceRead));
}

#[test]
fn lang_dir_custom() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        lang_dir = "i18n"
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.lang_dir, "i18n");
}

/// 번들된 com.tasty.image plugin의 실제 매니페스트가 파서를 통과하고
/// surface_kind가 egui-mesh로 인식되는지 확인 (B2/ADR-0030: image 는 host-rendered →
/// egui-mesh mesh-only 로 전환됨).
#[test]
fn bundled_image_plugin_manifest_validates() {
    // CARGO_MANIFEST_DIR = crates/tasty-plugin-manifest → 형제 crate 경로.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tasty-plugin-image");
    let m = Manifest::load(&path).expect("image plugin manifest should load");
    assert_eq!(m.id, "com.tasty.image");
    assert_eq!(m.surface_kinds.len(), 1);
    assert_eq!(m.surface_kinds[0].kind, "image");
    assert_eq!(m.surface_kinds[0].rendering, SurfaceKindRendering::EguiMesh);
    // ipc_namespace prefix가 "image"여야 하고 cli 매핑이 모두 image.* 메서드.
    assert!(
        m.contributes
            .ipc_namespace
            .iter()
            .any(|n| n.prefix == "image")
    );
    assert!(m.contributes.cli.iter().any(|c| c.name == "image"));
}

/// 번들된 com.tasty.claude plugin 의 실제 매니페스트가 파서를 통과하는지 확인.
/// 이 plugin 의 `settings_pages`/`ui.settings_page` 는 이미 추가되어 있고, 이
/// smoke test 가 그 기준선이 되어 권한 누락 회귀는 `settings_pages_requires_ui_permission`
/// (`docs/dev-guide/plugin-development.md`, `docs/dev-guide/plugin-permissions.md`
/// 참고)가 파서 레벨에서 잡는다.
#[test]
fn bundled_claude_plugin_manifest_validates() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tasty-plugin-claude");
    let m = Manifest::load(&path).expect("claude plugin manifest should load");
    assert_eq!(m.id, "com.tasty.claude");
}

/// 번들된 com.tasty.codex plugin 의 실제 매니페스트가 파서를 통과하는지 확인.
/// 이 plugin 의 `settings_pages`/`ui.settings_page` 는 이미 추가되어 있고, 이
/// smoke test 가 그 기준선이 되어 권한 누락 회귀는 `settings_pages_requires_ui_permission`
/// (`docs/dev-guide/plugin-development.md`, `docs/dev-guide/plugin-permissions.md`
/// 참고)가 파서 레벨에서 잡는다.
#[test]
fn bundled_codex_plugin_manifest_validates() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tasty-plugin-codex");
    let m = Manifest::load(&path).expect("codex plugin manifest should load");
    assert_eq!(m.id, "com.tasty.codex");
}

#[test]
fn surface_kind_rendering_defaults_to_remote() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "explorer"
        display_name_i18n_key = "k"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.surface_kinds[0].rendering, SurfaceKindRendering::Remote);
}

#[test]
fn surface_kind_rendering_host_rejected() {
    // host-rendered 채널은 제거됐다 — `rendering = "host"` 는 더 이상
    // 유효한 variant 가 아니며 매니페스트 파싱 자체가 실패해야 한다.
    let s = r#"
        manifest_version = 1
        id = "com.tasty.image"
        name = "Image"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "image"
        display_name_i18n_key = "surface.kind.image"
        rendering = "host"
    "#;
    assert!(parse(s).is_err(), "rendering = \"host\" must not parse");
}

#[test]
fn surface_kind_rendering_egui_mesh_parses_hyphenated_wire_key() {
    // 와이어 키는 하이픈 포함 "egui-mesh" (variant rename). rename_all="lowercase"
    // 가 만드는 "eguimesh" 가 아니라 이 키로만 파싱돼야 한다 (ADR-0028 / §2-2).
    let s = r#"
        manifest_version = 1
        id = "com.tasty.markdown"
        name = "Markdown"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "markdown"
        display_name_i18n_key = "surface.kind.markdown"
        rendering = "egui-mesh"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.surface_kinds[0].rendering, SurfaceKindRendering::EguiMesh);
}

#[test]
fn surface_kind_rendering_eguimesh_without_hyphen_rejected() {
    // lowercase 기본 변환형 "eguimesh" 는 유효 variant 가 아니다 — rename 덮어쓰기
    // 검증 (하이픈 함정, §9-9).
    let s = r#"
        manifest_version = 1
        id = "com.tasty.markdown"
        name = "Markdown"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "markdown"
        display_name_i18n_key = "surface.kind.markdown"
        rendering = "eguimesh"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn rendering_serializes_back_to_the_wire_key_it_parses_from() {
    // 밖으로 나가는 문자열은 직렬화기가 만든다. `Debug` 파생을 소문자화하면
    // "eguimesh" 가 나와 **입력으로 되돌릴 수 없는** 값이 되고, 그 어긋남을
    // 컴파일러는 모른다 — `plugin.show` 가 실제로 그 값을 내보내고 있었다.
    // 위 두 테스트가 "무엇이 파싱되나" 를 고정하므로, 이 테스트는 그 반대 방향을 고정한다.
    for (v, wire) in [
        (SurfaceKindRendering::Remote, "remote"),
        (SurfaceKindRendering::Webview, "webview"),
        (SurfaceKindRendering::EguiMesh, "egui-mesh"),
    ] {
        let json = serde_json::to_string(&v).expect("serializes");
        assert_eq!(json, format!("\"{wire}\""), "{v:?} 의 와이어 키가 어긋났다");
        // 왕복: 나간 값이 그대로 다시 들어와야 한다.
        let back: SurfaceKindRendering = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back, v);
    }
}

#[test]
fn binding_mode_serializes_back_to_its_toml_spelling() {
    // `BindingMode` 는 커스텀 Deserialize 로 "independent" / "inherit:<action>" 을 받는다.
    // 내보낼 때도 같은 표기여야 왕복이 성립한다. `Debug` 파생은
    // `InheritHost("clipboard.copy")` 를 만들어 그 왕복을 깬다.
    for (v, wire) in [
        (BindingMode::Independent, "independent"),
        (
            BindingMode::InheritHost("clipboard.copy".to_string()),
            "inherit:clipboard.copy",
        ),
    ] {
        let json = serde_json::to_string(&v).expect("serializes");
        assert_eq!(
            json,
            format!("\"{wire}\""),
            "{v:?} 의 와이어 표기가 어긋났다"
        );
        let back: BindingMode = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back, v);
    }
}

#[test]
fn surface_kind_rendering_unknown_value_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "explorer"
        display_name_i18n_key = "k"
        rendering = "exotic"
    "#;
    // serde가 lowercase enum의 알 수 없는 variant를 reject.
    assert!(parse(s).is_err());
}

#[test]
fn event_subscribe_accepts_exact_key_and_wildcard() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_subscribe = ["surface.created", "surface.*", "command.invoked"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.event_subscribe.len(), 3);
}

#[test]
fn event_subscribe_rejects_bare_wildcard() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_subscribe = ["*"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(
        err.contains("invalid event_subscribe pattern"),
        "got: {err}"
    );
}

#[test]
fn event_subscribe_rejects_leading_wildcard() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_subscribe = ["*.created"]
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn event_subscribe_rejects_middle_wildcard() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_subscribe = ["surface.*.created"]
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn event_subscribe_rejects_single_segment() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_subscribe = ["surface"]
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn event_subscribe_rejects_partial_wildcard() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_subscribe = ["surf*.created"]
        [entry]
        type = "process"
        command = "x"
    "#;
    assert!(parse(s).is_err());
}

#[test]
fn event_publish_rejects_reserved_namespace() {
    let s = r#"
        manifest_version = 1
        id = "com.example.evil"
        name = "Evil"
        version = "0.1"
        api_version = "1"
        event_publish = ["surface.created"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("reserved namespace"), "got: {err}");
}

#[test]
fn the_agent_namespace_is_the_hosts_to_publish_in() {
    // `RESERVED_IPC_PREFIXES` 는 `agent` 를 처음부터 들고 있었는데 사건 쪽 목록에만
    // 빠져 있었다. 호스트가 그 이름으로 발화하기 시작한 이상 plugin 이 같은 이름을
    // 자기 것으로 선언하면 두 발화자가 한 키를 다툰다.
    let s = r#"
        manifest_version = 1
        id = "com.example.evil"
        name = "Evil"
        version = "0.1"
        api_version = "1"
        event_publish = ["agent.task_finished"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("reserved namespace"), "got: {err}");
}

#[test]
fn subscribing_to_agent_events_stays_open_to_plugins() {
    // 예약은 **발화** 제약이다. 구독까지 막으면 이 사건을 만든 이유가 사라진다.
    let s = r#"
        manifest_version = 1
        id = "com.example.watcher"
        name = "Watcher"
        version = "0.1"
        api_version = "1"
        event_subscribe = ["agent.*"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.event_subscribe, vec!["agent.*".to_string()]);
}

#[test]
fn event_publish_accepts_plugin_namespace() {
    let s = r#"
        manifest_version = 1
        id = "com.example.claude"
        name = "Claude"
        version = "0.1"
        api_version = "1"
        event_publish = ["claude.activity.changed", "claude.session.*"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.event_publish.len(), 2);
}

#[test]
fn event_subscribe_accepts_reserved_namespace() {
    // subscribe는 어떤 namespace도 허용 (예약은 publish 전용 제약).
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_subscribe = ["surface.*", "system.shutdown"]
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.event_subscribe.len(), 2);
}

#[test]
fn events_emitted_parses_and_defaults_stable() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_publish = ["com.example.x.*"]
        [[events_emitted]]
        key = "com.example.x.child_state_changed"
        description = "child state"
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.events_emitted.len(), 1);
    let decl = &m.events_emitted[0];
    assert_eq!(decl.key, "com.example.x.child_state_changed");
    assert_eq!(decl.stability, EventStability::Stable);
    assert!(decl.payload_schema.is_none());
}

#[test]
fn events_emitted_rejects_wildcard_key() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_publish = ["com.example.x.*"]
        [[events_emitted]]
        key = "com.example.x.*"
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("invalid events_emitted key"), "got: {err}");
}

#[test]
fn events_emitted_rejects_reserved_namespace() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_publish = ["surface.created"]
        [[events_emitted]]
        key = "surface.created"
        [entry]
        type = "process"
        command = "x"
    "#;
    // event_publish 검증이 먼저 reserved를 잡지만, 다른 검증 단계라도 결국 거부됨.
    assert!(parse(s).is_err());
}

#[test]
fn events_emitted_rejects_uncovered_key() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_publish = ["com.example.x.foo.*"]
        [[events_emitted]]
        key = "com.example.x.bar"
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("not covered by"), "got: {err}");
}

#[test]
fn events_emitted_rejects_duplicate_key() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_publish = ["com.example.x.*"]
        [[events_emitted]]
        key = "com.example.x.foo"
        [[events_emitted]]
        key = "com.example.x.foo"
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn events_emitted_accepts_experimental_stability() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        event_publish = ["com.example.x.*"]
        [[events_emitted]]
        key = "com.example.x.alpha"
        stability = "experimental"
        payload_schema = "schemas/alpha.json"
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    let decl = &m.events_emitted[0];
    assert_eq!(decl.stability, EventStability::Experimental);
    assert_eq!(decl.payload_schema.as_deref(), Some("schemas/alpha.json"));
}

// ── contributes.hook_events 검증 ─────────────────────────────────────

#[test]
fn hook_events_parses_and_defaults_stable() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [[contributes.hook_events]]
        key = "claude-idle"
        description = "Claude turned idle"
        [[contributes.hook_events]]
        key = "needs-input"
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("should parse");
    assert_eq!(m.contributes.hook_events.len(), 2);
    let decl = &m.contributes.hook_events[0];
    assert_eq!(decl.key, "claude-idle");
    assert_eq!(decl.description, "Claude turned idle");
    assert_eq!(decl.stability, EventStability::Stable);
}

#[test]
fn hook_events_rejects_empty_key() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [[contributes.hook_events]]
        key = ""
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(
        err.contains("invalid contributes.hook_events key"),
        "got: {err}"
    );
}

#[test]
fn hook_events_rejects_builtin_collision() {
    for builtin in ["process-exit", "bell", "notification"] {
        let s = format!(
            r#"
            manifest_version = 1
            id = "com.example.x"
            name = "X"
            version = "0.1"
            api_version = "1"
            [[contributes.hook_events]]
            key = "{builtin}"
            [entry]
            type = "process"
            command = "x"
        "#
        );
        let err = parse(&s).unwrap_err().to_string();
        assert!(
            err.contains("collides with a built-in hook event"),
            "key '{builtin}' should collide, got: {err}"
        );
    }
}

#[test]
fn hook_events_rejects_wildcard_and_colon() {
    for bad in ["claude-*", "output-match:foo", "idle-timeout:5", "foo.bar"] {
        let s = format!(
            r#"
            manifest_version = 1
            id = "com.example.x"
            name = "X"
            version = "0.1"
            api_version = "1"
            [[contributes.hook_events]]
            key = "{bad}"
            [entry]
            type = "process"
            command = "x"
        "#
        );
        assert!(parse(&s).is_err(), "key '{bad}' should be rejected");
    }
}

#[test]
fn hook_events_rejects_duplicate_key() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [[contributes.hook_events]]
        key = "claude-idle"
        [[contributes.hook_events]]
        key = "claude-idle"
        [entry]
        type = "process"
        command = "x"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

// ── extends 블록 검증 ────────────────────────────────────────────────

fn extends_skeleton(extra: &str) -> String {
    format!(
        r#"
            manifest_version = 1
            id = "com.example.ext"
            name = "Ext"
            version = "0.1.0"
            api_version = "1"
            permissions = ["ext:com.tasty.clipboard"]
            [entry]
            type = "process"
            command = "x"
            [extends]
            plugin_id = "com.tasty.clipboard"
            version_req = ">=0.2.0, <0.3.0"
            api_version = "1"
            {extra}
        "#
    )
}

#[test]
fn extends_accepts_valid_hooks() {
    let s = extends_skeleton(
        r#"
            [[extends.pre_ipc]]
            method = "clipboard.add"
            modifies = ["entry"]
            mode = "transform"
            timeout_ms = 100

            [[extends.post_event]]
            event = "clipboard.entry_added"
            mode = "observe"
            timeout_ms = 50
        "#,
    );
    let m = parse(&s).expect("should parse");
    let decl = m.extends.as_ref().expect("extends present");
    assert_eq!(decl.plugin_id, "com.tasty.clipboard");
    assert_eq!(decl.pre_ipc.len(), 1);
    assert_eq!(decl.post_event.len(), 1);
    assert_eq!(decl.pre_ipc[0].mode, HookMode::Transform);
    assert_eq!(decl.post_event[0].mode, HookMode::Observe);
}

#[test]
fn extends_rejects_zero_hooks() {
    let s = extends_skeleton("");
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("at least one hook"), "got: {err}");
}

#[test]
fn extends_rejects_self_target() {
    let s = r#"
        manifest_version = 1
        id = "com.example.ext"
        name = "Ext"
        version = "0.1.0"
        api_version = "1"
        permissions = ["ext:com.example.ext"]
        [entry]
        type = "process"
        command = "x"
        [extends]
        plugin_id = "com.example.ext"
        version_req = ">=0.1.0"
        api_version = "1"
        [[extends.pre_ipc]]
        method = "x.run"
        modifies = ["entry"]
        mode = "transform"
        timeout_ms = 100
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("differ from this plugin"), "got: {err}");
}

#[test]
fn extends_rejects_invalid_version_req() {
    let s = extends_skeleton(
        r#"
            [[extends.pre_ipc]]
            method = "clipboard.add"
            modifies = ["entry"]
            mode = "transform"
            timeout_ms = 100
        "#,
    )
    .replace(">=0.2.0, <0.3.0", "not-a-semver-req~~");
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("version_req"), "got: {err}");
}

#[test]
fn extends_rejects_mismatched_api_version() {
    let s = r#"
        manifest_version = 1
        id = "com.example.ext"
        name = "Ext"
        version = "0.1.0"
        api_version = "1"
        permissions = ["ext:com.tasty.clipboard"]
        [entry]
        type = "process"
        command = "x"
        [extends]
        plugin_id = "com.tasty.clipboard"
        version_req = ">=0.2.0, <0.3.0"
        api_version = "999"
        [[extends.pre_ipc]]
        method = "clipboard.add"
        modifies = ["entry"]
        mode = "transform"
        timeout_ms = 100
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("api_version"), "got: {err}");
}

#[test]
fn extends_rejects_timeout_over_max() {
    let s = extends_skeleton(
        r#"
            [[extends.pre_ipc]]
            method = "clipboard.add"
            modifies = ["entry"]
            mode = "transform"
            timeout_ms = 5000
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("exceeds maximum"), "got: {err}");
}

#[test]
fn extends_rejects_zero_timeout() {
    let s = extends_skeleton(
        r#"
            [[extends.pre_ipc]]
            method = "clipboard.add"
            modifies = ["entry"]
            mode = "transform"
            timeout_ms = 0
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("timeout_ms"), "got: {err}");
}

#[test]
fn extends_rejects_event_wildcard() {
    let s = extends_skeleton(
        r#"
            [[extends.pre_event]]
            event = "clipboard.*"
            modifies = ["payload"]
            mode = "transform"
            timeout_ms = 100
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("concrete event key"), "got: {err}");
}

#[test]
fn extends_rejects_ipc_wildcard() {
    let s = extends_skeleton(
        r#"
            [[extends.pre_ipc]]
            method = "clipboard.*"
            modifies = ["entry"]
            mode = "transform"
            timeout_ms = 100
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("concrete method"), "got: {err}");
}

#[test]
fn extends_rejects_transform_without_modifies() {
    let s = extends_skeleton(
        r#"
            [[extends.pre_ipc]]
            method = "clipboard.add"
            modifies = []
            mode = "transform"
            timeout_ms = 100
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("non-empty 'modifies'"), "got: {err}");
}

#[test]
fn extends_requires_ext_permission_in_manifest() {
    // skeleton에는 ext:com.tasty.clipboard 권한이 들어 있다. 그걸 빼면 거부되어야.
    let s = extends_skeleton(
        r#"
            [[extends.pre_ipc]]
            method = "clipboard.add"
            modifies = ["entry"]
            mode = "transform"
            timeout_ms = 100
        "#,
    )
    .replace(
        "permissions = [\"ext:com.tasty.clipboard\"]",
        "permissions = []",
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("ext:com.tasty.clipboard"), "got: {err}");
}

#[test]
fn ext_permission_token_parses() {
    assert_eq!(
        Permission::from_token("ext:com.tasty.clipboard"),
        Some(Permission::Extension("com.tasty.clipboard".into()))
    );
    // 잘못된 plugin id는 거부.
    assert_eq!(Permission::from_token("ext:Bad-Id"), None);
    assert_eq!(Permission::from_token("ext:"), None);
    // round trip.
    assert_eq!(
        Permission::Extension("com.x.y".into()).as_token(),
        "ext:com.x.y"
    );
}

fn tool_skeleton(extra: &str) -> String {
    format!(
        r#"
        manifest_version = 1
        id = "com.example.tooly"
        name = "Tooly"
        version = "0.1.0"
        api_version = "1"
        permissions = ["ui.tool_item"]
        [entry]
        type = "process"
        command = "tooly"
        [[surface_kinds]]
        kind = "tooly_panel"
        display_name_i18n_key = "tooly.surface"
        [contributes]
        {extra}
        "#,
    )
}

#[test]
fn tool_event_action_parses() {
    let s = tool_skeleton(
        r#"
            [[contributes.tool]]
            id = "open-search"
            label_i18n_key = "tooly.tool.open_search"
            action = { kind = "event", event_key = "tooly.search_requested" }
        "#,
    );
    let m = parse(&s).expect("event tool should parse");
    assert_eq!(m.contributes.tool.len(), 1);
    match &m.contributes.tool[0].action {
        ToolAction::Event { event_key } => assert_eq!(event_key, "tooly.search_requested"),
        other => panic!("expected event action, got {other:?}"),
    }
    // 기본 order_hint = 100
    assert_eq!(m.contributes.tool[0].order_hint, 100);
}

#[test]
fn tool_open_surface_must_reference_declared_kind() {
    let s = tool_skeleton(
        r#"
            [[contributes.tool]]
            id = "open-panel"
            label_i18n_key = "tooly.tool.open_panel"
            action = { kind = "open_surface", surface_kind = "not_declared" }
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(
        err.contains("not declared in this plugin's [[surface_kinds]]"),
        "got: {err}"
    );
}

#[test]
fn tool_requires_ui_tool_item_permission() {
    let s = tool_skeleton(
        r#"
            [[contributes.tool]]
            id = "open-search"
            label_i18n_key = "tooly.tool.open_search"
            action = { kind = "event", event_key = "tooly.search_requested" }
        "#,
    )
    .replace("permissions = [\"ui.tool_item\"]", "permissions = []");
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("ui.tool_item"), "got: {err}");
}

#[test]
fn tool_id_must_be_unique() {
    let s = tool_skeleton(
        r#"
            [[contributes.tool]]
            id = "dup"
            label_i18n_key = "a"
            action = { kind = "event", event_key = "tooly.a" }
            [[contributes.tool]]
            id = "dup"
            label_i18n_key = "b"
            action = { kind = "event", event_key = "tooly.b" }
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn ui_tool_item_permission_token_parses() {
    assert_eq!(
        Permission::from_token("ui.tool_item"),
        Some(Permission::UiToolItem)
    );
    assert_eq!(Permission::UiToolItem.as_token(), "ui.tool_item");
}

#[test]
fn ui_popup_permission_token_parses() {
    assert_eq!(
        Permission::from_token("ui.popup"),
        Some(Permission::UiPopup)
    );
    assert_eq!(Permission::UiPopup.as_token(), "ui.popup");
}

#[test]
fn agent_permission_token_parses() {
    assert_eq!(
        Permission::from_token("agent"),
        Some(Permission::AgentManage)
    );
    assert_eq!(Permission::AgentManage.as_token(), "agent");
}

#[test]
fn memory_permission_tokens_parse() {
    assert_eq!(
        Permission::from_token("memory.read"),
        Some(Permission::MemoryRead)
    );
    assert_eq!(
        Permission::from_token("memory.write"),
        Some(Permission::MemoryWrite)
    );
    assert_eq!(Permission::MemoryRead.as_token(), "memory.read");
    assert_eq!(Permission::MemoryWrite.as_token(), "memory.write");
}

fn popup_skeleton(extra: &str) -> String {
    format!(
        r#"
        manifest_version = 1
        id = "com.example.popper"
        name = "Popper"
        version = "0.1.0"
        api_version = "1"
        permissions = ["ui.popup"]
        event_publish = ["com.example.popper.search_requested"]
        [entry]
        type = "process"
        command = "popper"
        [contributes]
        {extra}
        "#,
    )
}

#[test]
fn popup_event_trigger_parses_with_defaults() {
    let s = popup_skeleton(
        r#"
            [[contributes.popup]]
            id = "search"
            trigger = { kind = "event", event_key = "com.example.popper.search_requested" }
        "#,
    );
    let m = parse(&s).expect("popup with event trigger should parse");
    assert_eq!(m.contributes.popup.len(), 1);
    let p = &m.contributes.popup[0];
    assert_eq!(p.id, "search");
    match &p.trigger {
        PopupTrigger::Event { event_key } => {
            assert_eq!(event_key, "com.example.popper.search_requested")
        }
        other => panic!("expected event trigger, got {other:?}"),
    }
    assert_eq!(p.anchor, PopupAnchor::ScreenCenter);
    // scope 필드가 없는 기존 매니페스트는 window 로 읽힌다 — 하위 호환의 자리.
    assert_eq!(p.scope, PopupScopeDecl::Window);
    assert!(p.dismiss_on_outside_click);
    assert!(p.size_hint.is_none());
}

#[test]
fn popup_scope_surface_parses() {
    let s = popup_skeleton(
        r#"
            [[contributes.popup]]
            id = "search"
            trigger = { kind = "ipc" }
            scope = "surface"
        "#,
    );
    let m = parse(&s).expect("popup with surface scope should parse");
    assert_eq!(m.contributes.popup[0].scope, PopupScopeDecl::Surface);
}

#[test]
fn popup_scope_rejects_unknown_kind() {
    // host 전용 스코프(pane/tab/workspace)는 plugin 선언에 열려 있지 않다.
    let s = popup_skeleton(
        r#"
            [[contributes.popup]]
            id = "search"
            trigger = { kind = "ipc" }
            scope = "pane"
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("scope") || err.contains("pane"), "got: {err}");
}

#[test]
fn popup_ipc_trigger_with_size_and_anchor() {
    let s = popup_skeleton(
        r#"
            [[contributes.popup]]
            id = "panel"
            trigger = { kind = "ipc" }
            size_hint = { width = 480, height = 360 }
            anchor = "cursor"
            dismiss_on_outside_click = false
        "#,
    );
    let m = parse(&s).expect("popup with ipc trigger should parse");
    let p = &m.contributes.popup[0];
    assert!(matches!(p.trigger, PopupTrigger::Ipc));
    assert_eq!(p.anchor, PopupAnchor::Cursor);
    assert!(!p.dismiss_on_outside_click);
    let sz = p.size_hint.expect("size_hint set");
    assert_eq!(sz.width, 480);
    assert_eq!(sz.height, 360);
}

#[test]
fn popup_requires_ui_popup_permission() {
    let s = popup_skeleton(
        r#"
            [[contributes.popup]]
            id = "search"
            trigger = { kind = "ipc" }
        "#,
    )
    .replace("permissions = [\"ui.popup\"]", "permissions = []");
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("ui.popup"), "got: {err}");
}

#[test]
fn popup_id_must_be_unique() {
    let s = popup_skeleton(
        r#"
            [[contributes.popup]]
            id = "dup"
            trigger = { kind = "ipc" }
            [[contributes.popup]]
            id = "dup"
            trigger = { kind = "ipc" }
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn popup_size_hint_zero_rejected() {
    let s = popup_skeleton(
        r#"
            [[contributes.popup]]
            id = "search"
            trigger = { kind = "ipc" }
            size_hint = { width = 0, height = 360 }
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("size_hint"), "got: {err}");
}

// ── banner contribute (A3) ──

fn banner_skeleton(extra: &str) -> String {
    format!(
        r#"
        manifest_version = 1
        id = "com.example.notifier"
        name = "Notifier"
        version = "0.1.0"
        api_version = "1"
        permissions = ["ui.banner"]
        [entry]
        type = "process"
        command = "notifier"
        [contributes]
        {extra}
        "#,
    )
}

#[test]
fn banner_ipc_trigger_parses_with_defaults() {
    let s = banner_skeleton(
        r#"
            [[contributes.banner]]
            id = "sync-status"
            trigger = { kind = "ipc" }
        "#,
    );
    let m = parse(&s).expect("banner with ipc trigger should parse");
    assert_eq!(m.contributes.banner.len(), 1);
    let b = &m.contributes.banner[0];
    assert_eq!(b.id, "sync-status");
    assert!(matches!(b.trigger, BannerTrigger::Ipc));
    assert_eq!(b.scope, BannerScopeDecl::Surface);
    assert_eq!(b.rendering, BannerRendering::EguiMesh);
    assert!(b.ttl_seconds.is_none());
    assert!(b.size_hint.is_none());
}

#[test]
fn banner_with_ttl_and_size_hint() {
    let s = banner_skeleton(
        r#"
            [[contributes.banner]]
            id = "sync-status"
            trigger = { kind = "ipc" }
            ttl_seconds = 6
            size_hint = { height = 64 }
        "#,
    );
    let m = parse(&s).expect("banner with ttl/size should parse");
    let b = &m.contributes.banner[0];
    assert_eq!(b.ttl_seconds, Some(6));
    assert_eq!(b.size_hint.expect("size_hint set").height, 64);
}

#[test]
fn banner_requires_ui_banner_permission() {
    let s = banner_skeleton(
        r#"
            [[contributes.banner]]
            id = "sync-status"
            trigger = { kind = "ipc" }
        "#,
    )
    .replace("permissions = [\"ui.banner\"]", "permissions = []");
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("ui.banner"), "got: {err}");
}

#[test]
fn banner_id_must_be_unique() {
    let s = banner_skeleton(
        r#"
            [[contributes.banner]]
            id = "dup"
            trigger = { kind = "ipc" }
            [[contributes.banner]]
            id = "dup"
            trigger = { kind = "ipc" }
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn banner_zero_ttl_rejected() {
    let s = banner_skeleton(
        r#"
            [[contributes.banner]]
            id = "sync-status"
            trigger = { kind = "ipc" }
            ttl_seconds = 0
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("ttl_seconds"), "got: {err}");
}

#[test]
fn banner_zero_height_rejected() {
    let s = banner_skeleton(
        r#"
            [[contributes.banner]]
            id = "sync-status"
            trigger = { kind = "ipc" }
            size_hint = { height = 0 }
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("height"), "got: {err}");
}

#[test]
fn ui_banner_permission_token_roundtrip() {
    assert_eq!(
        Permission::from_token("ui.banner"),
        Some(Permission::UiBanner)
    );
    assert_eq!(Permission::UiBanner.as_token(), "ui.banner");
}

#[test]
fn extends_filter_mode_allows_empty_modifies() {
    let s = extends_skeleton(
        r#"
            [[extends.pre_ipc]]
            method = "clipboard.add"
            modifies = []
            mode = "filter"
            timeout_ms = 100
        "#,
    );
    let m = parse(&s).expect("filter without modifies should parse");
    assert_eq!(m.extends.unwrap().pre_ipc[0].mode, HookMode::Filter);
}

#[test]
fn file_handler_define_token_parses() {
    assert_eq!(
        Permission::from_token("file_handler.define"),
        Some(Permission::FileHandlerDefine)
    );
    assert_eq!(
        Permission::FileHandlerDefine.as_token(),
        "file_handler.define"
    );
}

#[test]
fn file_handler_extend_handle_tokens_parse() {
    let p = Permission::from_token("file_handler.extend:markdown").unwrap();
    assert_eq!(p, Permission::FileHandlerExtend("markdown".into()));
    assert_eq!(p.as_token(), "file_handler.extend:markdown");

    let p = Permission::from_token("file_handler.handle:pdf").unwrap();
    assert_eq!(p, Permission::FileHandlerHandle("pdf".into()));
    assert_eq!(p.as_token(), "file_handler.handle:pdf");

    // $directory 는 허용 (실제 등록된 reserved id)
    assert!(Permission::from_token("file_handler.handle:$directory").is_some());
}

#[test]
fn file_handler_unknown_sentinel_token_rejected() {
    assert!(Permission::from_token("file_handler.handle:$unknown").is_none());
    assert!(Permission::from_token("file_handler.extend:$unknown").is_none());
}

#[test]
fn hook_handler_define_token_parses() {
    assert_eq!(
        Permission::from_token("hook_handler.define"),
        Some(Permission::HookHandlerDefine)
    );
    assert_eq!(
        Permission::HookHandlerDefine.as_token(),
        "hook_handler.define"
    );
}

#[test]
fn hook_handler_handle_token_parses() {
    let p = Permission::from_token("hook_handler.handle:notify").unwrap();
    assert_eq!(p, Permission::HookHandlerHandle("notify".into()));
    assert_eq!(p.as_token(), "hook_handler.handle:notify");
}

#[test]
fn hook_handler_handle_token_bad_scope_rejected() {
    // 대문자·`$`-prefix·32 초과·`.` 는 hook handler short-name 규칙 위반.
    assert!(Permission::from_token("hook_handler.handle:Notify").is_none());
    assert!(Permission::from_token("hook_handler.handle:$directory").is_none());
    assert!(Permission::from_token("hook_handler.handle:").is_none());
    assert!(Permission::from_token("hook_handler.handle:a.b").is_none());
    let too_long = "a".repeat(33);
    assert!(Permission::from_token(&format!("hook_handler.handle:{too_long}")).is_none());
}

fn detector_skeleton(perms: &str, extra: &str) -> String {
    format!(
        r#"
        manifest_version = 1
        id = "com.example.pdf"
        name = "PDF"
        version = "0.1"
        api_version = "1"
        permissions = [{perms}]
        [entry]
        type = "process"
        command = "x"
        {extra}
        "#
    )
}

#[test]
fn detector_define_token_required_for_new_id() {
    let s = detector_skeleton(
        r#""file_handler.define""#,
        r#"
            [[contributes.detector]]
            id = "pdf"
            [[contributes.detector.rule]]
            kind = "extension"
            values = ["pdf"]
        "#,
    );
    parse(&s).expect("with file_handler.define should accept new detector");
}

#[test]
fn hook_handler_contribute_requires_define() {
    // define 권한 없이 [[contributes.hook_handler]] → reject.
    let s = detector_skeleton(
        r#""#,
        r#"
            [[contributes.hook_handler]]
            id = "notify"
            source = "webhook"
            priority = 100
            [contributes.hook_handler.action]
            kind = "ipc_sequence"
            calls = [{ method = "notification.create", params = { body = "hi" } }]
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("hook_handler.define"), "got: {err}");
}

#[test]
fn hook_handler_contribute_with_define_ok() {
    let s = detector_skeleton(
        r#""hook_handler.define""#,
        r#"
            [[contributes.hook_handler]]
            id = "notify"
            source = "webhook"
            priority = 100
            [contributes.hook_handler.action]
            kind = "ipc_sequence"
            calls = [{ method = "notification.create", params = { body = "hi" } }]
        "#,
    );
    parse(&s).expect("with hook_handler.define should accept hook handler");
}

#[test]
fn hook_handler_contribute_bad_id_rejected() {
    let s = detector_skeleton(
        r#""hook_handler.define""#,
        r#"
            [[contributes.hook_handler]]
            id = "Bad_Id"
            source = "webhook"
            priority = 100
            [contributes.hook_handler.action]
            kind = "ipc_sequence"
            calls = []
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(
        err.contains("invalid contributes.hook_handler id"),
        "got: {err}"
    );
}

#[test]
fn hook_handler_contribute_duplicate_id_rejected() {
    let s = detector_skeleton(
        r#""hook_handler.define""#,
        r#"
            [[contributes.hook_handler]]
            id = "notify"
            source = "webhook"
            priority = 100
            [contributes.hook_handler.action]
            kind = "ipc_sequence"
            calls = []

            [[contributes.hook_handler]]
            id = "notify"
            source = "hook"
            priority = 50
            [contributes.hook_handler.action]
            kind = "ipc_sequence"
            calls = []
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn detector_without_define_or_extend_rejected() {
    let s = detector_skeleton(
        r#""#,
        r#"
            [[contributes.detector]]
            id = "pdf"
            [[contributes.detector.rule]]
            kind = "extension"
            values = ["pdf"]
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("file_handler.define") || err.contains("file_handler.extend"));
}

#[test]
fn detector_reserved_id_rejected_for_plugin() {
    let s = detector_skeleton(
        r#""file_handler.define""#,
        r#"
            [[contributes.detector]]
            id = "$mything"
            [[contributes.detector.rule]]
            kind = "extension"
            values = ["x"]
        "#,
    );
    assert!(parse(&s).is_err());
}

#[test]
fn handler_requires_handle_permission() {
    let s = detector_skeleton(
        r#""file_handler.define", "surface.write""#,
        r#"
            [[surface_kinds]]
            kind = "pdf_view"
            display_name_i18n_key = "x"

            [[contributes.detector]]
            id = "pdf"
            [[contributes.detector.rule]]
            kind = "extension"
            values = ["pdf"]

            [[contributes.handler]]
            id = "viewer"
            detector = "pdf"
            priority = 100
            [contributes.handler.action]
            kind = "open_surface"
            surface_kind = "pdf_view"
        "#,
    );
    // permissions 에 file_handler.handle:pdf 가 없으므로 reject
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("file_handler.handle:pdf"), "got: {err}");
}

#[test]
#[ignore = "production validation 미구현 — handler.action.surface_kind 가 plugin 의 surface_kinds 와 cross-ref 되지 않음. validator 추가 후 ignore 해제."]
fn handler_open_surface_cross_ref() {
    // surface_kind 가 plugin 자체에 없으면 reject
    let s = detector_skeleton(
        r#""file_handler.define", "file_handler.handle:pdf", "surface.write""#,
        r#"
            [[contributes.detector]]
            id = "pdf"
            [[contributes.detector.rule]]
            kind = "extension"
            values = ["pdf"]

            [[contributes.handler]]
            id = "viewer"
            detector = "pdf"
            priority = 100
            [contributes.handler.action]
            kind = "open_surface"
            surface_kind = "missing"
        "#,
    );
    assert!(parse(&s).is_err());
}

#[test]
#[ignore = "production validation 미구현 — PluginHandlerActionDecl 가 'system' kind 를 silently accept. 별 PluginHandlerAction enum (System 제외) 분리 후 ignore 해제."]
fn handler_system_kind_rejected_in_plugin() {
    let s = detector_skeleton(
        r#""file_handler.define", "file_handler.handle:pdf""#,
        r#"
            [[contributes.detector]]
            id = "pdf"
            [[contributes.detector.rule]]
            kind = "extension"
            values = ["pdf"]

            [[contributes.handler]]
            id = "viewer"
            detector = "pdf"
            priority = 100
            [contributes.handler.action]
            kind = "system"
        "#,
    );
    // serde 가 PluginHandlerActionDecl 의 unknown variant 로 reject
    assert!(parse(&s).is_err());
}

// ─────────────────────────────────────────────────────────────────
//  F.H — Plugin manifest 확장
// ─────────────────────────────────────────────────────────────────

#[test]
fn window_spawn_permission_token_roundtrip() {
    assert_eq!(
        Permission::from_token("window.spawn"),
        Some(Permission::WindowSpawn)
    );
    assert_eq!(Permission::WindowSpawn.as_token(), "window.spawn");
}

#[test]
fn surface_kind_default_colors_parses() {
    let s = r##"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
        [surface_kinds.default_colors]
        focused_bg = "#000000"
        focused_fg = "#cdd6f4"
        unfocused_bg = "#181825"
        unfocused_fg = "#a6adc8"
    "##;
    let m = parse(s).expect("surface_kinds.default_colors should parse");
    let kind = &m.surface_kinds[0];
    let dc = kind.default_colors.as_ref().expect("default_colors set");
    assert!(dc.focused_bg.is_some());
    assert!(dc.focused_fg.is_some());
}

#[test]
fn surface_kind_default_colors_invalid_hex_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
        [surface_kinds.default_colors]
        focused_bg = "not-a-color"
    "#;
    // HexColor 의 Deserialize 가 reject (parse 단계).
    let m: Result<Manifest, _> = toml::from_str(s);
    assert!(m.is_err());
}

fn window_skeleton(perms: &str, extra: &str) -> String {
    format!(
        r#"
        manifest_version = 1
        id = "com.example.winapp"
        name = "WinApp"
        version = "0.1.0"
        api_version = "1"
        permissions = [{perms}]
        [entry]
        type = "process"
        command = "winapp"
        [contributes]
        {extra}
        "#,
    )
}

#[test]
fn window_contribute_parses_with_defaults() {
    let s = window_skeleton(
        r#""window.spawn""#,
        r#"
            [[contributes.window]]
            id = "editor"
            display_name_i18n_key = "winapp.editor"
        "#,
    );
    let m = parse(&s).expect("window contribute should parse");
    assert_eq!(m.contributes.window.len(), 1);
    let w = &m.contributes.window[0];
    assert_eq!(w.id, "editor");
    assert!(!w.multi_instance);
    assert!(w.default_size.is_none());
}

#[test]
fn window_contribute_with_size_and_multi_instance() {
    let s = window_skeleton(
        r#""window.spawn""#,
        r#"
            [[contributes.window]]
            id = "editor"
            display_name_i18n_key = "winapp.editor"
            multi_instance = true
            default_size = { width = 1024, height = 768 }
        "#,
    );
    let m = parse(&s).expect("window contribute with size should parse");
    let w = &m.contributes.window[0];
    assert!(w.multi_instance);
    let sz = w.default_size.expect("default_size set");
    assert_eq!(sz.width, 1024);
    assert_eq!(sz.height, 768);
}

#[test]
fn window_contribute_requires_permission() {
    let s = window_skeleton(
        "",
        r#"
            [[contributes.window]]
            id = "editor"
            display_name_i18n_key = "winapp.editor"
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("window.spawn"), "got: {err}");
}

#[test]
fn window_contribute_id_must_be_unique() {
    let s = window_skeleton(
        r#""window.spawn""#,
        r#"
            [[contributes.window]]
            id = "dup"
            display_name_i18n_key = "winapp.dup"
            [[contributes.window]]
            id = "dup"
            display_name_i18n_key = "winapp.dup"
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn window_contribute_id_format_validated() {
    let s = window_skeleton(
        r#""window.spawn""#,
        r#"
            [[contributes.window]]
            id = "Bad-Id"
            display_name_i18n_key = "winapp.bad"
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("invalid contributes.window id"), "got: {err}");
}

#[test]
fn example_markdown_plugin_manifest_parses() {
    // crates/tasty-plugin-markdown/tasty-plugin.toml — schema 확장 데모 +
    // 템플릿 plugin. BUILTINS 미등록이라 런타임 로드는 안 되지만, 컴파일타임
    // include + 런타임 parse 로 schema 호환성을 잠근다.
    let text = include_str!("../../tasty-plugin-markdown/tasty-plugin.toml");
    let m = parse(text).expect("example markdown manifest should parse and validate");
    assert_eq!(m.id, "com.tasty.markdown");
    assert_eq!(m.surface_kinds.len(), 1);
    assert!(m.surface_kinds[0].default_colors.is_some());
}

// ─────────────────────────────────────────────────────────────────
//  Settings pages contribute (Step 1 — manifest schema 확장)
// ─────────────────────────────────────────────────────────────────

#[test]
fn settings_pages_parses_with_default_empty() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
    "#;
    let m = parse(s).expect("manifest without settings_pages should parse");
    assert!(m.contributes.settings_pages.is_empty());
}

#[test]
fn settings_pages_with_font_override_parses() {
    let s = r#"
        manifest_version = 1
        id = "com.tasty.markdown"
        name = "Markdown"
        version = "0.1.0"
        api_version = "1"
        permissions = ["ui.settings_page"]
        [entry]
        type = "process"
        command = "x"

        [[contributes.settings_pages]]
        id = "markdown"
        title_key = "settings.subtab.markdown"
        category = "appearance"

        [[contributes.settings_pages.items]]
        kind = "font_override"
        id = "font"
        label_key = "settings.markdown.font"
        storage_key = "markdown"
    "#;
    let m = parse(s).expect("settings_pages with font_override should parse");
    assert_eq!(m.contributes.settings_pages.len(), 1);
    let page = &m.contributes.settings_pages[0];
    assert_eq!(page.id, "markdown");
    assert_eq!(page.title_key, "settings.subtab.markdown");
    assert_eq!(page.category, SettingsCategory::Appearance);
    assert_eq!(page.items.len(), 1);
    match &page.items[0] {
        SettingsItemDecl::FontOverride {
            id,
            label_key,
            storage_key,
        } => {
            assert_eq!(id, "font");
            assert_eq!(label_key, "settings.markdown.font");
            assert_eq!(storage_key, "markdown");
        }
        other => panic!("expected FontOverride, got {other:?}"),
    }
    // permission token round-trip
    assert_eq!(
        Permission::from_token("ui.settings_page"),
        Some(Permission::UiSettingsPage)
    );
    assert_eq!(Permission::UiSettingsPage.as_token(), "ui.settings_page");
}

/// 공통 manifest preamble + 주어진 items toml 을 합쳐 한 페이지짜리 매니페스트를 만든다.
fn settings_items_manifest(items: &str) -> String {
    format!(
        r#"
        manifest_version = 1
        id = "com.tasty.x"
        name = "X"
        version = "0.1.0"
        api_version = "1"
        permissions = ["ui.settings_page"]
        [entry]
        type = "process"
        command = "x"

        [[contributes.settings_pages]]
        id = "x"
        title_key = "settings.subtab.x"
        category = "appearance"
{items}
    "#
    )
}

#[test]
fn settings_pages_toggle_select_number_parse() {
    let s = settings_items_manifest(
        r#"
        [[contributes.settings_pages.items]]
        kind = "toggle"
        id = "wrap"
        label_key = "x.wrap"
        storage_key = "wrap"
        default = true

        [[contributes.settings_pages.items]]
        kind = "select"
        id = "scheme"
        label_key = "x.scheme"
        storage_key = "scheme"
        default = "dark"
        [[contributes.settings_pages.items.options]]
        value = "light"
        label_key = "x.light"
        [[contributes.settings_pages.items.options]]
        value = "dark"
        label_key = "x.dark"

        [[contributes.settings_pages.items]]
        kind = "number"
        id = "zoom"
        label_key = "x.zoom"
        storage_key = "zoom"
        default = 100.0
        min = 50.0
        max = 200.0
        suffix_key = "x.percent"
        "#,
    );
    let m = parse(&s).expect("toggle/select/number page should parse + validate");
    let items = &m.contributes.settings_pages[0].items;
    assert_eq!(items.len(), 3);
    match &items[0] {
        SettingsItemDecl::Toggle { id, default: d, .. } => {
            assert_eq!(id, "wrap");
            assert!(*d);
        }
        other => panic!("expected Toggle, got {other:?}"),
    }
    match &items[1] {
        SettingsItemDecl::Select {
            options, default, ..
        } => {
            assert_eq!(default, "dark");
            assert_eq!(options.len(), 2);
            assert_eq!(options[0].value, "light");
            assert_eq!(options[1].label_key, "x.dark");
        }
        other => panic!("expected Select, got {other:?}"),
    }
    match &items[2] {
        SettingsItemDecl::Number {
            default,
            min,
            max,
            suffix_key,
            ..
        } => {
            assert_eq!(*default, 100.0);
            assert_eq!(*min, Some(50.0));
            assert_eq!(*max, Some(200.0));
            assert_eq!(suffix_key.as_deref(), Some("x.percent"));
        }
        other => panic!("expected Number, got {other:?}"),
    }
}

#[test]
fn settings_pages_select_default_not_in_options_rejected() {
    let s = settings_items_manifest(
        r#"
        [[contributes.settings_pages.items]]
        kind = "select"
        id = "scheme"
        label_key = "x.scheme"
        storage_key = "scheme"
        default = "sepia"
        [[contributes.settings_pages.items.options]]
        value = "light"
        label_key = "x.light"
        [[contributes.settings_pages.items.options]]
        value = "dark"
        label_key = "x.dark"
        "#,
    );
    let err = parse(&s).expect_err("select default not in options must fail");
    assert!(
        err.to_string().contains("not among options"),
        "unexpected error: {err}"
    );
}

#[test]
fn settings_pages_number_min_gt_max_rejected() {
    let s = settings_items_manifest(
        r#"
        [[contributes.settings_pages.items]]
        kind = "number"
        id = "zoom"
        label_key = "x.zoom"
        storage_key = "zoom"
        default = 100.0
        min = 200.0
        max = 50.0
        "#,
    );
    let err = parse(&s).expect_err("number min > max must fail");
    assert!(err.to_string().contains("min"), "unexpected error: {err}");
}

#[test]
fn settings_pages_invalid_storage_key_rejected() {
    let s = settings_items_manifest(
        r#"
        [[contributes.settings_pages.items]]
        kind = "toggle"
        id = "wrap"
        label_key = "x.wrap"
        storage_key = "Wrap!"
        "#,
    );
    let err = parse(&s).expect_err("invalid storage_key must fail");
    assert!(
        err.to_string().contains("storage_key"),
        "unexpected error: {err}"
    );
}

#[test]
fn settings_pages_requires_ui_permission() {
    let s = r#"
        manifest_version = 1
        id = "com.tasty.markdown"
        name = "Markdown"
        version = "0.1.0"
        api_version = "1"
        permissions = []
        [entry]
        type = "process"
        command = "x"

        [[contributes.settings_pages]]
        id = "markdown"
        title_key = "settings.subtab.markdown"
        category = "appearance"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("ui.settings_page"), "got: {err}");
}

#[test]
fn settings_category_plugin_deserializes() {
    let cat: SettingsCategory =
        serde_json::from_str(r#""plugin""#).expect("\"plugin\" should deserialize");
    assert_eq!(cat, SettingsCategory::Plugin);
}

#[test]
fn settings_pages_with_plugin_category_parses() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        permissions = ["ui.settings_page"]
        [entry]
        type = "process"
        command = "x"

        [[contributes.settings_pages]]
        id = "main"
        title_key = "settings.subtab.x_main"
        category = "plugin"
    "#;
    let m = parse(s).expect("category = \"plugin\" should be accepted");
    assert_eq!(m.contributes.settings_pages.len(), 1);
    assert_eq!(
        m.contributes.settings_pages[0].category,
        SettingsCategory::Plugin
    );
}

#[test]
fn surface_kind_decl_new_fields_default() {
    // 새 3 필드 (required_params / param_aliases / consumes_egui_input) 는 모두
    // `#[serde(default)]` 이므로 기존 manifest 가 깨지지 않아야 한다.
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
    "#;
    let m = parse(s).expect("manifest without new surface_kind fields should parse");
    let kind = &m.surface_kinds[0];
    assert!(kind.required_params.is_empty());
    assert!(kind.param_aliases.is_empty());
    assert!(!kind.consumes_egui_input);
    assert!(kind.preset_fields.is_empty());

    // 명시 지정도 round-trip 되는지.
    let s2 = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
        required_params = ["file"]
        consumes_egui_input = true
        [surface_kinds.param_aliases]
        file_path = "file"
    "#;
    let m2 = parse(s2).expect("manifest with new surface_kind fields should parse");
    let kind2 = &m2.surface_kinds[0];
    assert_eq!(kind2.required_params, vec!["file".to_string()]);
    assert!(kind2.consumes_egui_input);
    assert_eq!(kind2.param_aliases.get("file_path"), Some(&"file".into()));
}

#[test]
fn surface_kind_preset_fields_parse() {
    use crate::types::PresetFieldInputType;
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "markdown"
        display_name_i18n_key = "k"
        [[surface_kinds.preset_fields]]
        id = "file"
        label_key = "preset.field.file"
        param_key = "file"
        input_type = "file_path"
        required = true
        placeholder_key = "preset.field.file_hint"
        derive_cwd = true
    "#;
    let m = parse(s).expect("manifest with preset_fields should parse");
    let fields = &m.surface_kinds[0].preset_fields;
    assert_eq!(fields.len(), 1);
    let f = &fields[0];
    assert_eq!(f.id, "file");
    assert_eq!(f.param_key, "file");
    assert_eq!(f.input_type, PresetFieldInputType::FilePath);
    assert!(f.required);
    assert!(f.derive_cwd);
    assert_eq!(f.placeholder_key.as_deref(), Some("preset.field.file_hint"));
}

#[test]
fn preset_field_input_type_defaults_to_text() {
    use crate::types::PresetFieldInputType;
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
        [[surface_kinds.preset_fields]]
        id = "note"
        label_key = "l"
        param_key = "note"
    "#;
    let m = parse(s).expect("preset_field without input_type should default to text");
    let f = &m.surface_kinds[0].preset_fields[0];
    assert_eq!(f.input_type, PresetFieldInputType::Text);
    assert!(!f.required);
    assert!(!f.derive_cwd);
}

#[test]
fn preset_field_derive_cwd_requires_file_path() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
        [[surface_kinds.preset_fields]]
        id = "u"
        label_key = "l"
        param_key = "url"
        input_type = "url"
        derive_cwd = true
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("derive_cwd"), "got: {err}");
}

#[test]
fn preset_field_duplicate_id_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
        [[surface_kinds.preset_fields]]
        id = "dup"
        label_key = "l"
        param_key = "a"
        [[surface_kinds.preset_fields]]
        id = "dup"
        label_key = "l"
        param_key = "b"
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("declared twice"), "got: {err}");
}

#[test]
fn preset_field_empty_param_key_rejected() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
        [[surface_kinds.preset_fields]]
        id = "f"
        label_key = "l"
        param_key = ""
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("param_key"), "got: {err}");
}

#[test]
fn required_params_must_match_required_preset_field() {
    // required_params 는 preset_fields 의 required=true param_key 집합과 정합해야 한다.
    let mismatch = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"
        [[surface_kinds]]
        kind = "foo"
        display_name_i18n_key = "k"
        required_params = ["file"]
        [[surface_kinds.preset_fields]]
        id = "file"
        label_key = "l"
        param_key = "file"
        input_type = "file_path"
    "#;
    // required=false 라 required_params 와 어긋남 → 거부.
    let err = parse(mismatch).unwrap_err().to_string();
    assert!(err.contains("required_params"), "got: {err}");

    // required=true 로 정합하면 통과.
    let ok = mismatch.replace(
        r#"input_type = "file_path""#,
        "input_type = \"file_path\"\n        required = true",
    );
    parse(&ok).expect("required_params matching a required field should parse");
}

#[test]
fn window_contribute_zero_default_size_rejected() {
    let s = window_skeleton(
        r#""window.spawn""#,
        r#"
            [[contributes.window]]
            id = "editor"
            display_name_i18n_key = "winapp.editor"
            default_size = { width = 0, height = 768 }
        "#,
    );
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("default_size"), "got: {err}");
}

#[test]
fn auto_wait_decl_parses_with_defaults() {
    // `tasty claude spawn` 자동 wait manifest 가 `map_from_response` /
    // `map_from_request` / default no_wait/timeout 필드 없이도 deserialize 되는지.
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "ex"

        [[contributes.cli]]
        name = "ex"
        subcommands = [
          { name = "spawn", ipc_method = "ex.spawn", args = "g", auto_wait = { method = "ex.wait", map_from_response = { parent_surface_id = "surface", child_index = "child_index" }, polling = { state_field = "state", terminal_states = ["idle"], interval_ms = 500 } } },
        ]

        [contributes.cli.arg_groups.g]
    "#;
    let m = parse(s).expect("manifest should parse");
    let sub = &m.contributes.cli[0].subcommands[0];
    let aw = sub.auto_wait.as_ref().expect("auto_wait declared");
    assert_eq!(aw.method, "ex.wait");
    assert_eq!(aw.no_wait_field, "no_wait");
    assert_eq!(aw.timeout_field, "timeout");
    assert!(aw.map_from_request.is_empty());
}

#[test]
fn rejects_subcommand_with_both_polling_and_auto_wait() {
    // 한 subcommand 가 polling (self-poll) 과 auto_wait (chained wait) 를 동시에
    // 선언하면 의미가 충돌 — validator 가 reject.
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "ex"

        [[contributes.cli]]
        name = "ex"
        subcommands = [
          { name = "spawn", ipc_method = "ex.spawn", args = "g", polling = { state_field = "state", terminal_states = ["idle"], interval_ms = 500 }, auto_wait = { method = "ex.wait", polling = { state_field = "state", terminal_states = ["idle"], interval_ms = 500 } } },
        ]

        [contributes.cli.arg_groups.g]
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("auto_wait"), "got: {err}");
}

fn auto_wait_strategy_manifest(strategy: &str) -> String {
    format!(
        r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "ex"

        [[contributes.cli]]
        name = "ex"
        subcommands = [
          {{ name = "spawn", ipc_method = "ex.spawn", args = "g", auto_wait = {{ method = "ex.wait", strategy = "{strategy}" }} }},
        ]

        [contributes.cli.arg_groups.g]
    "#
    )
}

#[test]
fn auto_wait_strategy_parses_when_owner_prefix_matches_plugin_id() {
    // strategy 의 owner-prefix 가 자기 자신의 plugin id 와 같으면 통과.
    let s = auto_wait_strategy_manifest("com.example.x/wait-ready");
    let m = parse(&s).expect("own-prefixed strategy reference should parse");
    let aw = m.contributes.cli[0].subcommands[0]
        .auto_wait
        .as_ref()
        .expect("auto_wait declared");
    assert_eq!(aw.strategy.as_deref(), Some("com.example.x/wait-ready"));
    assert!(aw.polling.is_none());
}

#[test]
fn auto_wait_strategy_rejects_mismatched_owner_prefix() {
    // 다른 plugin 의 strategy 를 참조하면 거부 — CLI 는 어차피 같은 매니페스트
    // 안에서만 이름을 해석하므로, 이건 조기 실패용 방어선.
    let s = auto_wait_strategy_manifest("com.other.plugin/wait-ready");
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("does not"), "got: {err}");
}

#[test]
fn auto_wait_strategy_rejects_missing_owner_prefix() {
    // '/' 구분자가 없으면 owner-prefix 를 판정할 수 없어 거부.
    let s = auto_wait_strategy_manifest("wait-ready");
    let err = parse(&s).unwrap_err().to_string();
    assert!(err.contains("no owner prefix"), "got: {err}");
}

#[test]
fn auto_wait_rejects_neither_polling_nor_strategy() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "ex"

        [[contributes.cli]]
        name = "ex"
        subcommands = [
          { name = "spawn", ipc_method = "ex.spawn", args = "g", auto_wait = { method = "ex.wait" } },
        ]

        [contributes.cli.arg_groups.g]
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("neither"), "got: {err}");
}

#[test]
fn auto_wait_rejects_both_polling_and_strategy() {
    let s = r#"
        manifest_version = 1
        id = "com.example.x"
        name = "X"
        version = "0.1"
        api_version = "1"
        [entry]
        type = "process"
        command = "x"

        [[contributes.ipc_namespace]]
        prefix = "ex"

        [[contributes.cli]]
        name = "ex"
        subcommands = [
          { name = "spawn", ipc_method = "ex.spawn", args = "g", auto_wait = { method = "ex.wait", strategy = "com.example.x/wait-ready", polling = { state_field = "state", terminal_states = ["idle"], interval_ms = 500 } } },
        ]

        [contributes.cli.arg_groups.g]
    "#;
    let err = parse(s).unwrap_err().to_string();
    assert!(err.contains("both"), "got: {err}");
}

#[test]
fn completion_strategy_decl_parses_with_defaults_and_converts_to_polling_decl() {
    // CompletionStrategyDecl 은 아직 어떤 [[contributes.*]] 배열에도 담기지 않는다
    // (registry 트랙 몫) — 여기서는 타입 자체의 deserialize 기본값과
    // to_polling_decl() 변환만 확인한다.
    let s = r#"
        poll_method = "ex.wait"
        state_field = "state"
        terminal_states = ["idle", "exited"]
    "#;
    let decl: CompletionStrategyDecl = toml::from_str(s).expect("minimal decl should parse");
    assert_eq!(decl.interval_ms, 500, "interval_ms should default to 500");
    assert_eq!(decl.timeout_ms, None);
    assert!(decl.map_from_response.is_empty());
    assert!(decl.map_from_request.is_empty());

    let polling = decl.to_polling_decl();
    assert_eq!(polling.state_field, "state");
    assert_eq!(polling.terminal_states, vec!["idle", "exited"]);
    assert_eq!(polling.interval_ms, 500);
    assert_eq!(
        polling.timeout_field, None,
        "named-strategy path does not support a CLI --timeout override in this round"
    );
}
