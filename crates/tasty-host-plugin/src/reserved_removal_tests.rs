//! 예약 id 폴더 제거의 경로 안전 조건. 지우지 말아야 할 폴더가 남는지를 함께 본다.

use std::path::Path;

use super::remove_reserved_folder;

fn write_plugin(root: &Path, folder: &str, id: &str) -> std::path::PathBuf {
    let dir = root.join(folder);
    std::fs::create_dir_all(&dir).expect("plugin dir");
    std::fs::write(
        dir.join("tasty-plugin.toml"),
        format!(
            "manifest_version = 1\nid = \"{id}\"\nname = \"x\"\nversion = \"0.1.0\"\napi_version = \"1\"\n[entry]\ntype = \"process\"\ncommand = \"x\"\n"
        ),
    )
    .expect("manifest");
    std::fs::write(dir.join("payload.bin"), b"x").expect("payload");
    dir
}

#[test]
fn removes_the_given_folder_when_its_name_differs_from_the_id() {
    let root = tempfile::tempdir().expect("root");
    let other = write_plugin(root.path(), "other", "com.tasty.fake");
    // 지워지는 폴더의 id 와 이름이 같은 다른 폴더가 남아야 한다.
    let named = write_plugin(root.path(), "com.tasty.fake", "com.tasty.other");
    remove_reserved_folder(root.path(), &other).expect("removed");
    assert!(!other.exists());
    assert!(named.join("payload.bin").exists());
}

#[test]
fn removes_only_one_of_two_folders_with_one_id() {
    let root = tempfile::tempdir().expect("root");
    let first = write_plugin(root.path(), "first", "com.tasty.same");
    let second = write_plugin(root.path(), "second", "com.tasty.same");
    remove_reserved_folder(root.path(), &first).expect("removed");
    assert!(!first.exists());
    assert!(second.join("payload.bin").exists());
}

#[test]
fn a_folder_already_gone_counts_as_removed() {
    let root = tempfile::tempdir().expect("root");
    remove_reserved_folder(root.path(), &root.path().join("gone")).expect("already gone");
}

#[test]
fn refuses_a_folder_outside_the_plugin_root() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    let dir = write_plugin(outside.path(), "x", "com.tasty.fake");
    let error = remove_reserved_folder(root.path(), &dir).expect_err("refused");
    assert!(!error.deletion_started);
    assert!(dir.join("payload.bin").exists());
}

#[test]
fn refuses_a_nested_folder_and_a_parent_reference() {
    let root = tempfile::tempdir().expect("root");
    let outer = write_plugin(root.path(), "outer", "com.tasty.fake");
    let nested = write_plugin(&outer, "nested", "com.tasty.fake");
    remove_reserved_folder(root.path(), &nested).expect_err("nested refused");
    assert!(nested.join("payload.bin").exists());
    // `root/..` 의 parent 는 root 다. 이름 없는 경로는 부모 비교 전에 걸러야 한다.
    let up = root.path().join("..");
    remove_reserved_folder(root.path(), &up).expect_err("parent reference refused");
    assert!(root.path().exists());
}

#[test]
fn refuses_when_the_manifest_is_now_an_admitted_id() {
    let root = tempfile::tempdir().expect("root");
    for id in ["com.example.x", "com.tasty.markdown"] {
        let dir = write_plugin(root.path(), "changed", id);
        let error = remove_reserved_folder(root.path(), &dir).expect_err(id);
        assert!(!error.deletion_started, "{id}");
        assert!(dir.join("payload.bin").exists(), "{id}");
    }
}

#[test]
fn refuses_when_the_manifest_exists_but_cannot_be_read() {
    let root = tempfile::tempdir().expect("root");
    let dir = write_plugin(root.path(), "broken", "com.tasty.fake");
    std::fs::write(dir.join("tasty-plugin.toml"), "not toml [").expect("break");
    let error = remove_reserved_folder(root.path(), &dir).expect_err("refused");
    assert!(!error.deletion_started);
    assert!(dir.join("payload.bin").exists());
}

#[test]
fn retries_after_a_partial_removal_took_the_manifest() {
    let root = tempfile::tempdir().expect("root");
    let dir = write_plugin(root.path(), "partial", "com.tasty.fake");
    std::fs::remove_file(dir.join("tasty-plugin.toml")).expect("manifest gone");
    remove_reserved_folder(root.path(), &dir).expect("removed");
    assert!(!dir.exists());
}

#[test]
fn refuses_a_plain_file() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("file");
    std::fs::write(&file, b"x").expect("file");
    remove_reserved_folder(root.path(), &file).expect_err("refused");
    assert!(file.exists());
}

#[cfg(unix)]
#[test]
fn a_linked_folder_keeps_its_target() {
    let root = tempfile::tempdir().expect("root");
    let target = tempfile::tempdir().expect("target");
    let real = write_plugin(target.path(), "real", "com.tasty.fake");
    let link = root.path().join("linked");
    std::os::unix::fs::symlink(&real, &link).expect("link");
    let error = remove_reserved_folder(root.path(), &link).expect_err("refused");
    assert!(!error.deletion_started);
    assert!(real.join("payload.bin").exists());
    assert!(std::fs::symlink_metadata(&link).is_ok());
}

/// 디렉터리 심볼릭 링크를 만들 권한이 없는 Windows 계정에서도 도는 정션 판정이다.
#[cfg(windows)]
#[test]
fn a_junction_keeps_its_target() {
    let root = tempfile::tempdir().expect("root");
    let target = tempfile::tempdir().expect("target");
    let real = write_plugin(target.path(), "real", "com.tasty.fake");
    let link = root.path().join("linked");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&real)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("mklink");
    assert!(status.success());
    let error = remove_reserved_folder(root.path(), &link).expect_err("refused");
    assert!(!error.deletion_started);
    assert!(real.join("payload.bin").exists());
    assert!(std::fs::symlink_metadata(&link).is_ok());
}
