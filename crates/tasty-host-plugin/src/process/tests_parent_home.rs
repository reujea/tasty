//! Exercise the production env boundary in separate host/SDK processes.
use super::*;

const TEST: &str = "process::tests_parent_home::parent_home_matrix";
const ID: &str = "com.example.viewer";

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn run(cmd: &mut Command) {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{cmd:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn child() -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", TEST, "--nocapture"]);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("TASTY_") {
            cmd.env_remove(key);
        }
    }
    cmd
}

#[test]
fn parent_home_matrix() {
    match std::env::var("PARENT_HOME_TEST_MODE").as_deref() {
        Ok("sdk") => return check_sdk(),
        Ok("host") => return check_host(),
        _ => {}
    }
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let home = root.join("host-home");
    let plugin = root.join("installed-plugin");
    let other = root.join("other-cwd");
    std::fs::create_dir_all(&other).unwrap();
    for code in tasty_i18n::BUILTIN_CODES.iter().copied().chain(["zz"]) {
        write(
            &plugin.join(format!("lang/{code}.toml")),
            &format!(
                "[viewer]\nlabel='installed {code}'\n{}",
                if code == "en" {
                    "english='English fallback'\n"
                } else {
                    ""
                }
            ),
        );
    }
    write(&home.join("lang/zz/pack.toml"), "[font]\nbuiltin=true\n");
    let mut cases = 0;
    for absolute in [false, true] {
        for cwd in [&plugin, &other] {
            for shadow in [false, true] {
                for code in tasty_i18n::BUILTIN_CODES
                    .iter()
                    .copied()
                    .chain(["zz", "missing"])
                {
                    for override_present in [true, false] {
                        check_case(&root, cwd, code, absolute, shadow, override_present);
                        cases += 1;
                    }
                }
            }
        }
    }
    // 설치 경로 2 × 작업 폴더 2 × 가림 2 × 언어(내장 + zz, missing) × 재정의 2
    assert_eq!(cases, 2 * 2 * 2 * (tasty_i18n::BUILTIN_CODES.len() + 2) * 2);
    tracing::info!("host/SDK parent home matrix: {cases} cases passed");
}

#[cfg(test)]
fn check_sdk() {
    let expected = std::env::var("PARENT_HOME_EXPECTED").unwrap();
    let env = tasty_plugin_sdk::PluginEnv::load().unwrap();
    let sdk = tasty_plugin_sdk::i18n::Translator::from_plugin_env(&env);
    assert_eq!(sdk.t("viewer.label"), expected);
    assert_eq!(sdk.t("viewer.english"), "English fallback");
    let home = std::path::PathBuf::from(std::env::var_os("TASTY_PARENT_HOME").unwrap());
    assert!(home.is_absolute());
    assert_eq!(env.data_dir.unwrap(), home.join("plugin-data").join(ID));
    assert_eq!(
        env.config_path.unwrap(),
        home.join("plugin-config").join(format!("{ID}.toml"))
    );
}

#[cfg(test)]
fn check_host() {
    let expected = std::env::var("PARENT_HOME_EXPECTED").unwrap();
    let code = std::env::var("PARENT_HOME_CODE").unwrap();
    let report = tasty_i18n::init(&code);
    let plugin = std::path::PathBuf::from(std::env::var_os("PARENT_HOME_PLUGIN").unwrap());
    tasty_i18n::register_namespace(ID, &plugin.join("lang"));
    assert_eq!(tasty_i18n::t("viewer.label"), expected);
    assert_eq!(tasty_i18n::t("viewer.english"), "English fallback");
    assert_eq!(
        report.effective,
        if code == "missing" { "en" } else { &code }
    );
    let package = PluginPackage {
            dir: plugin.clone(),
            manifest: toml::from_str(&format!("manifest_version=1\nid='{ID}'\nname='Viewer'\nversion='1.0.0'\napi_version='1'\n[entry]\ntype='process'\ncommand='unused'\n")).unwrap(),
        };
    let mut cmd = child();
    cmd.current_dir(std::env::var_os("PARENT_HOME_CHILD_CWD").unwrap())
        .env("PARENT_HOME_TEST_MODE", "sdk")
        .env("TASTY_PLUGIN_ID", ID)
        .env("TASTY_PLUGIN_DIR", plugin)
        .env("TASTY_PLUGIN_TOKEN", "test")
        .env("TASTY_HOST_IPC_PORT", "1")
        .env("TASTY_LOCALE", report.effective);
    inject_plugin_data_env(&mut cmd, &package, &package.dir.join("test.log")).unwrap();
    run(&mut cmd);
}

fn check_case(
    root: &Path,
    cwd: &Path,
    code: &str,
    absolute: bool,
    shadow: bool,
    override_present: bool,
) {
    let home = root.join("host-home");
    let plugin = root.join("installed-plugin");
    let effective = if code == "missing" { "en" } else { code };
    let relative =
        tasty_i18n::plugin_catalog::override_path(Path::new("lang"), ID, effective).unwrap();
    let user = home.join(&relative);
    let wrong = cwd.join("host-home").join(&relative);
    for (path, present, value) in [
        (&user, override_present, "user choice"),
        (&wrong, shadow, "WRONG_CWD_OVERRIDE"),
    ] {
        if present {
            write(path, &format!("[viewer]\nlabel='{value}'\n"));
        } else if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }
    let expected = if override_present {
        "user choice".to_owned()
    } else {
        format!("installed {effective}")
    };
    run(child()
        .current_dir(root)
        .env("PARENT_HOME_TEST_MODE", "host")
        .env("PARENT_HOME_CODE", code)
        .env("PARENT_HOME_EXPECTED", expected)
        .env("PARENT_HOME_PLUGIN", &plugin)
        .env("PARENT_HOME_CHILD_CWD", cwd)
        .env(
            "TASTY_HOME",
            if absolute {
                home.as_os_str()
            } else {
                std::ffi::OsStr::new("host-home")
            },
        ));
}
