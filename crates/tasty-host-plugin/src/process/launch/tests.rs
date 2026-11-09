//! Separate processes keep host CWD and SDK environment changes out of the test runner.
use std::{ffi::OsStr, path::Path, process::Command};

use super::command;
use tasty_plugin_manifest::PluginPackage;

const TEST: &str = "process::launch::tests::relative_install_matrix";
const ID: &str = "com.example.launch";

fn package(dir: &Path, entry: &str) -> PluginPackage {
    PluginPackage {
        dir: dir.to_owned(),
        manifest: toml::from_str(&format!(
            "manifest_version=1\nid='{ID}'\nname='Launch'\nversion='1.0.0'\napi_version='1'\n[entry]\ntype='process'\ncommand='{entry}'\n"
        )).unwrap(),
    }
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn isolate(cmd: &mut Command) {
    let explicit: std::collections::HashSet<_> =
        cmd.get_envs().map(|(key, _)| key.to_owned()).collect();
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("TASTY_") && !explicit.contains(&key) {
            cmd.env_remove(key);
        }
    }
    cmd.args(["--exact", TEST, "--nocapture"]);
}

#[test]
fn relative_install_matrix() {
    match std::env::var("INSTALL_TEST_MODE").as_deref() {
        Ok("sdk") => return sdk(),
        Ok("host") => return host(),
        _ => {}
    }
    let root = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    isolate(&mut cmd);
    let output = cmd
        .current_dir(root.path())
        .env("INSTALL_TEST_MODE", "host")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sdk() {
    let env = tasty_plugin_sdk::PluginEnv::load().unwrap();
    assert!(env.plugin_dir.as_ref().unwrap().is_absolute());
    let catalog = tasty_plugin_sdk::i18n::Translator::from_plugin_env(&env);
    assert_eq!(
        catalog.t("launch.label"),
        std::env::var("INSTALL_EXPECTED").unwrap()
    );
    assert_eq!(catalog.t("launch.fallback"), "English fallback");
    assert_eq!(catalog.t("launch.empty"), "Installed empty fallback");
    // 양쪽 경로를 canonicalize해 Windows verbatim 접두어와 임시 경로 symlink 차이를 없앤다.
    assert_eq!(
        std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
        std::fs::canonicalize(std::env::var_os("INSTALL_CWD").unwrap()).unwrap()
    );
}

fn host() {
    let root = std::env::current_dir().unwrap();
    let install = root.join("home/plugins/launch");
    let other = root.join("other");
    std::fs::create_dir_all(install.join("bin")).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let binary = install.join(format!("probe{}", std::env::consts::EXE_SUFFIX));
    let relative_binary = install.join("bin").join(binary.file_name().unwrap());
    for dest in [&binary, &relative_binary] {
        #[cfg(unix)]
        std::os::unix::fs::symlink(std::env::current_exe().unwrap(), dest).unwrap();
        #[cfg(not(unix))]
        std::fs::copy(std::env::current_exe().unwrap(), dest).unwrap();
    }
    for locale in tasty_i18n::BUILTIN_CODES.iter().copied().chain(["zz"]) {
        write(
            &install.join(format!("lang/{locale}.toml")),
            &format!(
                "[launch]\nlabel='installed {locale}'\nempty='Installed empty fallback'\n{}",
                if locale == "en" {
                    "fallback='English fallback'\n"
                } else {
                    ""
                }
            ),
        );
    }
    let mut matrix = Vec::new();
    for absolute in [false, true] {
        let dir = if absolute {
            install.clone()
        } else {
            Path::new("home/plugins/launch").to_owned()
        };
        for entry in [
            binary.file_name().unwrap().to_str().unwrap().to_owned(),
            format!("bin/{}", binary.file_name().unwrap().to_str().unwrap()),
            binary.to_string_lossy().into_owned(),
        ] {
            let pkg = package(&dir, &entry);
            for cwd in [&install, &other] {
                for shadow in [false, true] {
                    for locale in tasty_i18n::BUILTIN_CODES
                        .iter()
                        .copied()
                        .chain(["zz", "missing"])
                    {
                        let effective = if locale == "missing" { "en" } else { locale };
                        for user in ["missing", "empty", "override"] {
                            matrix.push(run_case(&Case {
                                root: &root,
                                install: &install,
                                dir: &dir,
                                pkg: &pkg,
                                cwd,
                                absolute,
                                entry: &entry,
                                shadow,
                                locale,
                                effective,
                                user,
                            }));
                        }
                    }
                }
            }
        }
    }
    // 설치 경로 2 × 실행 항목 3 × 작업 폴더 2 × 가림 2 × 언어(내장 + zz, missing) × 사용자 번역 3
    assert_eq!(
        matrix.len(),
        2 * 3 * 2 * 2 * (tasty_i18n::BUILTIN_CODES.len() + 2) * 3
    );
    assert_missing_paths_still_build(&binary);
    if let Some(path) = std::env::var_os("INSTALL_MATRIX_PATH") {
        std::fs::write(path, serde_json::to_vec_pretty(&matrix).unwrap()).unwrap();
    }
}

/// 설치 경로가 없어도 명령 구성은 가능하며, 실제 실행은 별도로 실패해야 한다.
fn assert_missing_paths_still_build(binary: &Path) {
    for entry in ["not-installed-probe", "bin/not-installed-probe"] {
        let cmd = command(&package(Path::new("does-not-exist"), entry)).unwrap();
        assert_eq!(cmd.get_program(), OsStr::new(entry));
        assert!(cmd.get_current_dir().unwrap().is_absolute());
    }
    let mut missing = command(&package(
        Path::new("does-not-exist"),
        binary.to_str().unwrap(),
    ))
    .unwrap();
    // 없는 작업 디렉터리의 오류는 OS마다 다르다: Unix는 NotFound, Windows는 NotADirectory.
    // 제품은 이 구분 대신 spawn_failed로 보고한다.
    #[cfg(windows)]
    let expected = std::io::ErrorKind::NotADirectory;
    #[cfg(not(windows))]
    let expected = std::io::ErrorKind::NotFound;
    assert_eq!(missing.spawn().unwrap_err().kind(), expected);
}

/// 실행 조건 하나. 이름 있는 필드로 조건을 구분한다.
struct Case<'a> {
    root: &'a Path,
    install: &'a Path,
    dir: &'a Path,
    pkg: &'a PluginPackage,
    cwd: &'a Path,
    absolute: bool,
    entry: &'a str,
    shadow: bool,
    locale: &'a str,
    effective: &'a str,
    user: &'a str,
}

/// 사용자 override 파일을 준비한다. missing이면 이전 조건에서 만든 파일도 지운다.
fn place_user_override(user_path: &Path, user: &str) {
    if user == "missing" {
        if user_path.exists() {
            std::fs::remove_file(user_path).unwrap();
        }
        return;
    }
    let value = if user == "empty" {
        "  "
    } else {
        "User override"
    };
    write(user_path, &format!("[launch]\nlabel='{value}'\nempty=''\n"));
}

/// CWD에 같은 이름의 카탈로그를 두어도 설치 경로의 값을 읽는지 확인한다.
fn place_shadow(shadow: bool, wrong_install: &Path, wrong_user: &Path, effective: &str) {
    if shadow {
        write(
            &wrong_install.join(format!("{effective}.toml")),
            "[launch]\nlabel='WRONG INSTALL'\nfallback='WRONG FALLBACK'\n",
        );
        write(wrong_user, "[launch]\nlabel='WRONG USER'\n");
        return;
    }
    if wrong_install.exists() {
        std::fs::remove_dir_all(wrong_install).unwrap();
    }
    if wrong_user.exists() {
        std::fs::remove_file(wrong_user).unwrap();
    }
}

/// 호스트와 SDK 자식이 같은 값을 읽는지 확인하고 실행 조건과 결과를 기록한다.
fn run_case(case: &Case) -> serde_json::Value {
    let Case {
        root,
        install,
        dir,
        pkg,
        cwd,
        absolute,
        entry,
        shadow,
        locale,
        effective,
        user,
    } = *case;
    let user_path =
        tasty_i18n::plugin_catalog::override_path(&root.join("home/lang"), ID, effective).unwrap();
    place_user_override(&user_path, user);
    let wrong_install = cwd.join("home/plugins/launch/lang");
    let wrong_user =
        tasty_i18n::plugin_catalog::override_path(&cwd.join("home/lang"), ID, effective).unwrap();
    place_shadow(shadow, &wrong_install, &wrong_user, effective);
    let expected = if user == "override" {
        "User override".to_owned()
    } else {
        format!("installed {effective}")
    };
    let host_catalog = tasty_i18n::plugin_catalog::load(
        &dir.join("lang"),
        effective,
        ID,
        Some(&root.join("home/lang")),
    );
    assert_eq!(host_catalog.get("launch.label"), Some(&expected));
    let mut cmd = command(pkg).unwrap();
    assert!(Path::new(cmd.get_program()).is_absolute());
    assert_eq!(cmd.get_current_dir(), Some(install));
    isolate(&mut cmd);
    cmd.current_dir(cwd)
        .env("INSTALL_TEST_MODE", "sdk")
        .env("INSTALL_EXPECTED", &expected)
        .env("INSTALL_CWD", cwd)
        .env("TASTY_PARENT_HOME", root.join("home"))
        .env("TASTY_PLUGIN_ID", ID)
        .env("TASTY_PLUGIN_TOKEN", "test")
        .env("TASTY_HOST_IPC_PORT", "1")
        .env("TASTY_LOCALE", effective);
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{dir:?} {entry} {locale} {user}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::json!({"absolute_install":absolute,"entry":entry,"cwd":cwd,"shadow":shadow,"requested_locale":locale,"effective":effective,"user":user,"expected":expected,"exit":output.status.code()})
}

#[cfg(unix)]
#[test]
fn path_fallback_and_missing_or_nonexecutable_entries_keep_os_errors() {
    let root = tempfile::tempdir().unwrap();
    let mut shell = command(&package(root.path(), "sh")).unwrap();
    assert_eq!(shell.get_program(), OsStr::new("sh"));
    assert!(
        shell
            .env("PATH", "/bin:/usr/bin")
            .args(["-c", "exit 0"])
            .status()
            .unwrap()
            .success()
    );
    for entry in ["missing-launch-probe", "bin/missing-launch-probe"] {
        let mut cmd = command(&package(root.path(), entry)).unwrap();
        assert_eq!(cmd.get_program(), OsStr::new(entry));
        assert_eq!(
            cmd.env("PATH", root.path()).spawn().unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    }
    let missing_absolute = root.path().join("missing-absolute");
    let mut cmd = command(&package(root.path(), missing_absolute.to_str().unwrap())).unwrap();
    assert_eq!(cmd.get_program(), missing_absolute.as_os_str());
    assert_eq!(
        cmd.spawn().unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    write(
        &root.path().join("not-executable"),
        "no executable permission",
    );
    assert_eq!(
        command(&package(root.path(), "not-executable"))
            .unwrap()
            .spawn()
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    // The command retains the symbolic installation name; only the OS resolves it.
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(root.path(), &alias).unwrap();
    let cmd = command(&package(&alias, "/bin/sh")).unwrap();
    assert_eq!(cmd.get_current_dir(), Some(alias.as_path()));
}
