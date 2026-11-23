//! CLI 명령 적재가 더 높은 Tasty를 요구하는 플러그인을 건너뛰는지 확인한다.

use super::build::discover_plugin_clis;

fn plugin(root: &std::path::Path, name: &str, min: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tasty-plugin.toml"),
        format!(
            r#"
manifest_version = 1
id = "com.example.{name}"
name = "X"
version = "0.1.0"
api_version = "1"
{min}

[entry]
type = "process"
command = "x"

[[contributes.ipc_namespace]]
prefix = "{name}"

[[contributes.cli]]
name = "{name}"
subcommands = [
  {{ name = "ping", ipc_method = "{name}.ping", args = "empty" }},
]

[contributes.cli.arg_groups.empty]
"#
        ),
    )
    .unwrap();
}

#[test]
fn discover_skips_a_plugin_requiring_a_newer_tasty() {
    // 이 시험 바이너리에서 본체 버전을 정하는 곳은 여기뿐이다.
    tasty_plugin_manifest::set_host_tasty_version(
        "1.0.0",
        tasty_plugin_manifest::VersionShortfall::Refuse,
    )
    .expect("host version");
    let dir = tempfile::tempdir().expect("tempdir");
    plugin(dir.path(), "newer", r#"min_tasty_version = "1.0.1""#);
    plugin(dir.path(), "same", r#"min_tasty_version = "1.0.0""#);
    plugin(dir.path(), "none", "");
    let entries = discover_plugin_clis(dir.path());
    let mut names: Vec<&str> = entries.iter().map(|e| e.cli.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["none", "same"]);
}
