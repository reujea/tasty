//! help·-h·--help 진입점이 같은 명령 집합을 찾는지 실행 중인 인스턴스 없이 확인한다.
//! 합성 plugin은 실행 파일이 없는 매니페스트이며, 포트 파일은 수신만 하는 리스너를 가리킨다.
//! 도움말 출력이 plugin 실행·IPC 연결·stdin 소비 없이 끝나는지도 함께 확인한다.

#[path = "spawn_diag/mod.rs"]
mod spawn_diag;

use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const FIXTURE: &str = r#"
manifest_version = 1
id = "com.example.fixture"
name = "Fixture"
version = "1.0.0"
api_version = "1"
[entry]
type = "process"
command = "never-spawned"
[[contributes.ipc_namespace]]
prefix = "fixture"
[[contributes.cli]]
name = "fixture"
description = "fixture top"
subcommands = [{name="show", ipc_method="fixture.show", args="show", description="fixture sub"}]
[contributes.cli.arg_groups.show]
flags = [{name="name", type="string", flag="--name", help="fixture name"}]
"#;

fn write(path: &Path, value: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value).unwrap();
}

fn home(plugin: bool, broken: bool) -> tempfile::TempDir {
    home_in("en", plugin, broken)
}

fn home_in(language: &str, plugin: bool, broken: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("config.toml"),
        &format!("[general]\nlanguage='{language}'\n"),
    );
    if plugin {
        write(
            &dir.path()
                .join("plugins/com.example.fixture/tasty-plugin.toml"),
            FIXTURE,
        );
    }
    if broken {
        write(
            &dir.path()
                .join("plugins/com.example.broken/tasty-plugin.toml"),
            "manifest_version = [",
        );
    }
    dir
}

/// stdin을 열어 둔 채 실행한다. CLI가 stdin을 끝까지 읽으려 하면 끝나지 않으므로 상한으로 검출한다.
fn run(home: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(spawn_diag::instance_bin());
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("TASTY_") {
            cmd.env_remove(key);
        }
    }
    // 상속 변수를 지운 뒤 적용해야 빈 번들 경로가 남는다. 이 스위트는 번들 plugin을 부르지 않는다.
    spawn_diag::apply_bundle_opt_in(&mut cmd, home);
    let mut child = cmd
        .env("TASTY_HOME", home)
        .env("NO_COLOR", "1")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"{\"not\":\"consumed\"}").unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("{args:?}: 도움말이 30초 안에 끝나지 않았다(stdin 대기 또는 GUI 실행 의심)");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(stdin);
    child.wait_with_output().unwrap()
}

/// 도움말의 명령 목록 절(Commands 또는 목적별 그룹)에 나온 명령 이름 집합.
/// 옵션·인자 절과 시작 안내의 `tasty …` 예시는 제외한다.
fn command_names(stdout: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_commands = false;
    for line in stdout.lines() {
        if line.ends_with(':') && !line.starts_with(' ') {
            in_commands = !matches!(line, "Options:" | "Arguments:");
            continue;
        }
        if in_commands
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && let Some(name) = line.split_whitespace().next()
            && name != "tasty"
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    names
}

fn ok(home: &Path, args: &[&str]) -> String {
    let out = run(home, args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{args:?}\nstdout:{}\nstderr:{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn fails(home: &Path, args: &[&str]) -> String {
    let out = run(home, args);
    assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
    assert!(out.stdout.is_empty(), "{args:?}: {out:?}");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn help_forms_list_the_same_commands_with_or_without_plugins() {
    for plugin in [true, false] {
        let dir = home(plugin, false);
        let home = dir.path();
        let root = command_names(&ok(home, &["--help"]));
        assert_eq!(root.contains(&"fixture".to_string()), plugin, "{root:?}");
        assert!(root.contains(&"memory".to_string()), "{root:?}");
        for args in [&["help"][..], &["-h"], &["--port-file", "missing", "help"]] {
            assert_eq!(command_names(&ok(home, args)), root, "{args:?}");
        }
        assert_eq!(
            ok(home, &["help", "memory", "goal"]),
            ok(home, &["memory", "goal", "--help"])
        );
        let goal = command_names(&ok(home, &["memory", "goal", "--help"]));
        assert_eq!(goal, ["clear", "get", "help", "set"]);
    }
}

#[test]
fn plugin_paths_resolve_through_every_help_form() {
    let dir = home(true, false);
    let home = dir.path();
    let direct = ok(home, &["fixture", "show", "--help"]);
    assert!(direct.contains("--name"), "{direct}");
    for args in [
        &["help", "fixture", "show"][..],
        &["--port-file", "missing", "help", "fixture", "show"],
        &["--port-file", "missing", "fixture", "show", "--help"],
    ] {
        assert_eq!(ok(home, args), direct, "{args:?}");
    }
    assert_eq!(
        ok(home, &["help", "fixture"]),
        ok(home, &["fixture", "--help"])
    );
}

#[test]
fn unknown_paths_exit_2_with_the_nearest_parent_candidates() {
    let dir = home(true, false);
    let home = dir.path();
    for (args, parent) in [
        (
            &["help", "memory", "goal", "nosuch"][..],
            "tasty memory goal",
        ),
        (&["memory", "goal", "nosuch"], "tasty memory goal"),
        (
            &["--port-file", "missing", "fixture", "nosuch"],
            "tasty fixture",
        ),
        (&["help", "fixture", "nosuch"], "tasty fixture"),
    ] {
        let err = fails(home, args);
        assert!(err.contains(&format!("'{parent}'")), "{args:?}: {err}");
        assert!(err.contains(&format!("{parent} ")), "{args:?}: {err}");
    }
}

#[test]
fn a_broken_manifest_is_reported_and_host_help_still_works() {
    let dir = home(true, true);
    let home = dir.path();
    let out = run(home, &["help"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let names = command_names(&String::from_utf8_lossy(&out.stdout));
    assert!(names.contains(&"fixture".to_string()), "{names:?}");
    assert!(names.contains(&"list".to_string()), "{names:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("com.example.broken"), "{err}");
}

#[test]
fn help_never_connects_to_the_port_file_target() {
    let dir = home(true, false);
    let home = dir.path();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    write(&home.join("tasty.port"), &port);
    let port_file = home.join("tasty.port");
    let port_file = port_file.to_str().unwrap();
    for args in [
        &["help", "fixture", "show"][..],
        &["fixture", "show", "--help"],
        &["--port-file", port_file, "help", "fixture", "show"],
        &["--help"],
        &["-a"],
    ] {
        ok(home, args);
    }
    match listener.accept() {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("도움말이 IPC 포트에 연결했다: {other:?}"),
    }
}

#[test]
fn all_after_a_command_or_as_an_option_value_is_data() {
    let dir = home(true, false);
    let home = dir.path();
    let tree = ok(home, &["-a"]);
    assert!(tree.contains("memory"), "{tree}");
    // 연결할 인스턴스가 없으므로 전송 경로라면 포트 파일 오류로 끝난다. 트리를 출력하지 않는다.
    let out = run(
        home,
        &[
            "--port-file",
            "missing",
            "send",
            "text",
            "--surface",
            "1",
            "--",
            "--all",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("memory"));
}

/// en이 아닌 언어는 도움말 트리를 먼저 build해 clap의 help 하위 명령이 생긴다.
/// 그래도 `help`는 `-h`와 같고 `help <경로> nosuch`는 `<경로>`를 부모로 안내한다.
#[test]
fn help_forms_keep_their_meaning_in_every_language() {
    for language in tasty_i18n::BUILTIN_CODES {
        let dir = home_in(language, true, false);
        let home = dir.path();
        assert_eq!(ok(home, &["help"]), ok(home, &["-h"]), "{language}");
        for (args, parent) in [
            (
                &["help", "memory", "goal", "nosuch"][..],
                "tasty memory goal",
            ),
            (&["help", "fixture", "nosuch"], "tasty fixture"),
        ] {
            let err = fails(home, args);
            assert!(
                err.contains(&format!("'{parent}'")),
                "{language} {args:?}: {err}"
            );
            assert!(!err.contains("tasty help "), "{language} {args:?}: {err}");
        }
    }
}

/// clap이 예약한 `help`를 이름으로 쓰는 plugin은 언어와 관계없이 등록하지 않고 경고한다.
/// en은 build 전 트리라 clap의 help가 아직 없어도 같은 경로를 탄다.
#[test]
fn a_plugin_named_help_is_refused_with_a_warning_in_every_language() {
    const HELP_PLUGIN: &str = r#"
manifest_version = 1
id = "com.example.help"
name = "Help"
version = "1.0.0"
api_version = "1"
[entry]
type = "process"
command = "never-spawned"
[[contributes.ipc_namespace]]
prefix = "helpfixture"
[[contributes.cli]]
name = "help"
description = "shadowing help"
subcommands = [{name="show", ipc_method="helpfixture.show", args="show", description="help sub"}]
[contributes.cli.arg_groups.show]
flags = [{name="name", type="string", flag="--name", help="help name"}]
"#;
    for language in tasty_i18n::BUILTIN_CODES {
        let dir = home_in(language, true, false);
        let home = dir.path();
        write(
            &home.join("plugins/com.example.help/tasty-plugin.toml"),
            HELP_PLUGIN,
        );
        for args in [&["help"][..], &["-h"], &["--help"]] {
            let out = run(home, args);
            let err = String::from_utf8_lossy(&out.stderr);
            assert_eq!(out.status.code(), Some(0), "{language} {args:?}: {err}");
            assert!(err.contains("'help'"), "{language} {args:?}: {err}");
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("fixture"), "{language} {args:?}: {stdout}");
        }
    }
}
