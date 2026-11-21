//! hook 실패 기록이 없는 polling·auto_wait 디스패치를 hook 명령이 사용하지 않도록 검사한다.
//! 대상은 제품의 is_hook_method로 고르고 번들 매니페스트에서 직접 수집한다.
//! 해당 경로를 지원하려면 먼저 실패 기록도 구현해야 한다. 번역된 최종 오류만 감싸면 영어 원본을 잃을 수 있다.
//! 관련 검사는 hook_failure_reason_stays_english에 있다.

use std::path::PathBuf;

use tasty_cli::hook_failure::is_hook_method;
use tasty_plugin_manifest::{CliSubcommandDecl, Manifest};

fn dispatch_without_recording(sub: &CliSubcommandDecl) -> Option<&'static str> {
    if sub.polling.is_some() {
        return Some("polling");
    }
    if sub.auto_wait.is_some() {
        return Some("auto_wait");
    }
    None
}

/// 번들 플러그인 매니페스트. 공식 플러그인은 잠금 파일 산출물의 픽스처를 읽는다.
fn manifest_paths() -> Vec<PathBuf> {
    tasty_test_support::bundled_plugin_manifest_dirs()
        .into_iter()
        .map(|dir| dir.join("tasty-plugin.toml"))
        .collect()
}

/// 매니페스트·전체 하위 명령·hook 하위 명령의 수를 각각 확인한다.
#[derive(Default)]
struct Census {
    manifests: usize,
    subcommands: usize,
    hook_subcommands: usize,
}

#[test]
fn no_hook_command_declares_a_dispatch_that_skips_recording() {
    let paths = manifest_paths();
    let mut census = Census::default();
    let mut violations = Vec::new();

    for path in &paths {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("매니페스트를 읽을 수 없다 {}: {e}", path.display()));
        let manifest: Manifest = toml::from_str(&text)
            .unwrap_or_else(|e| panic!("매니페스트 파싱 실패 {}: {e}", path.display()));
        census.manifests += 1;
        for command in &manifest.contributes.cli {
            for sub in &command.subcommands {
                census.subcommands += 1;
                if !is_hook_method(&sub.ipc_method) {
                    continue;
                }
                census.hook_subcommands += 1;
                if let Some(field) = dispatch_without_recording(sub) {
                    violations.push(format!(
                        "{}  {} {} (ipc_method={}) 가 `{field}` 를 선언했다",
                        path.display(),
                        command.name,
                        sub.name,
                        sub.ipc_method
                    ));
                }
            }
        }
    }

    assert_eq!(
        census.manifests,
        paths.len(),
        "매니페스트를 {} 개 찾아 {} 개만 읽었다",
        paths.len(),
        census.manifests
    );
    assert!(
        census.manifests > 0,
        "번들 매니페스트를 하나도 못 찾았다 — 스캔 루트가 옮겨졌는지 확인해라"
    );
    assert!(
        census.subcommands > 0,
        "서브커맨드를 하나도 못 찾았다 — 매니페스트 스키마의 cli 경로가 바뀌었는지 확인해라"
    );
    assert!(
        census.hook_subcommands > 0,
        "hook 모양 서브커맨드를 하나도 못 찾았다({} 개 중) — 기록 대상 술어나 명명 관례가 \
         바뀌었으면 이 가드도 함께 옮겨라",
        census.subcommands
    );

    assert!(
        violations.is_empty(),
        "hook 명령에 실패 기록이 없는 polling 또는 auto_wait가 설정됐다. 설정을 제거하거나 해당 디스패치의 기록을 구현한다:\n{}",
        violations.join("\n")
    );
}

#[test]
fn a_hook_subcommand_that_declares_polling_is_a_violation() {
    let offender: CliSubcommandDecl = toml::from_str(
        r#"
        name = "hook"
        ipc_method = "claude.hook"
        args = "hook_args"
        polling = { state_field = "state", terminal_states = ["done"] }
        "#,
    )
    .expect("픽스처 파싱");
    assert!(is_hook_method(&offender.ipc_method));
    assert_eq!(dispatch_without_recording(&offender), Some("polling"));

    let chained: CliSubcommandDecl = toml::from_str(
        r#"
        name = "checklist-hook"
        ipc_method = "claude.checklist_hook"
        args = "hook_args"
        auto_wait = { method = "claude.wait", polling = { state_field = "state", terminal_states = ["done"] } }
        "#,
    )
    .expect("픽스처 파싱");
    assert!(is_hook_method(&chained.ipc_method));
    assert_eq!(dispatch_without_recording(&chained), Some("auto_wait"));
}

/// hook이 아닌 명령의 polling·auto_wait는 이 검사의 대상이 아니다.
#[test]
fn a_plain_hook_and_a_blocking_non_hook_are_both_fine() {
    let plain: CliSubcommandDecl = toml::from_str(
        r#"
        name = "hook"
        ipc_method = "codex.hook"
        args = "hook_args"
        "#,
    )
    .expect("픽스처 파싱");
    assert!(is_hook_method(&plain.ipc_method));
    assert_eq!(dispatch_without_recording(&plain), None);

    let spawn: CliSubcommandDecl = toml::from_str(
        r#"
        name = "spawn"
        ipc_method = "claude.spawn"
        args = "spawn_args"
        polling = { state_field = "state", terminal_states = ["idle"] }
        "#,
    )
    .expect("픽스처 파싱");
    assert!(!is_hook_method(&spawn.ipc_method));
    assert!(dispatch_without_recording(&spawn).is_some());
}
