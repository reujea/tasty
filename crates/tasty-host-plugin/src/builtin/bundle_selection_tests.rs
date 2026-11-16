use super::*;

/// debug 부팅이 workspace에서 동기화하는 플러그인.
fn workspace_spec() -> &'static BuiltinSpec {
    BUILTINS
        .iter()
        .find(|s| s.source == BuiltinSource::Workspace)
        .expect("workspace 원천 플러그인이 없다")
}

/// 릴리스 산출물을 스테이징해 쓰는 플러그인.
fn official_spec() -> &'static BuiltinSpec {
    BUILTINS
        .iter()
        .find(|s| s.source == BuiltinSource::OfficialRelease)
        .expect("릴리스 원천 플러그인이 없다")
}

/// workspace 크레이트·빌드 산출물 자리에 원본을 쓴다.
fn write_workspace_sources(workspace: &Path, exe_dir: &Path, spec: &BuiltinSpec) {
    let source = workspace.join("crates").join(spec.crate_dir);
    std::fs::create_dir_all(source.join("lang")).unwrap();
    std::fs::create_dir_all(exe_dir).unwrap();
    std::fs::write(source.join("tasty-plugin.toml"), "edited manifest").unwrap();
    std::fs::write(source.join("tasty-plugin.toml.sig"), "old signature").unwrap();
    std::fs::write(source.join("lang/en.toml"), "edited language").unwrap();
    std::fs::write(exe_dir.join(spec.bin_name), "new binary").unwrap();
}

/// 스테이징한 파일을 원본보다 옛 시각으로 만든다.
fn stamp_epoch(path: &Path) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH)
        .unwrap();
}

fn workspace_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let workspace = tempfile::tempdir().unwrap();
    let exe_dir = workspace.path().join("target/profile");
    let spec = workspace_spec();
    write_workspace_sources(workspace.path(), &exe_dir, spec);
    let bundle = exe_dir.join("builtin-plugins");
    (workspace, exe_dir, bundle)
}

#[cfg(debug_assertions)]
#[test]
fn debug_bundle_selection_refreshes_workspace_artifacts() {
    assert_staged_artifacts();
}

#[cfg(not(debug_assertions))]
#[test]
fn non_debug_bundle_selection_preserves_staged_release_artifacts() {
    assert_staged_artifacts();
}

fn assert_staged_artifacts() {
    let (_workspace, exe_dir, bundle) = workspace_fixture();
    let spec = workspace_spec();
    let staged = bundle.join(spec.id);
    std::fs::create_dir_all(staged.join("lang")).unwrap();
    let files = [
        ("tasty-plugin.toml", "signed manifest", "edited manifest"),
        ("tasty-plugin.toml.sig", "paired signature", "old signature"),
        ("lang/en.toml", "staged language", "edited language"),
        (spec.bin_name, "staged binary", "new binary"),
    ];
    for (path, original, _) in files {
        std::fs::write(staged.join(path), original).unwrap();
        stamp_epoch(&staged.join(path));
    }
    assert_eq!(bundle_root_from_exe_dir(&exe_dir), Some(bundle));
    for (path, original, updated) in files {
        assert_eq!(
            std::fs::read_to_string(staged.join(path)).unwrap(),
            if cfg!(debug_assertions) {
                updated
            } else {
                original
            },
            "{path}"
        );
    }
}

#[test]
fn bundle_selection_only_debug_creates_missing_bundle() {
    let (_workspace, exe_dir, bundle) = workspace_fixture();
    assert_eq!(
        bundle_root_from_exe_dir(&exe_dir),
        cfg!(debug_assertions).then(|| bundle.clone())
    );
    assert_eq!(bundle.exists(), cfg!(debug_assertions));
}

#[test]
fn bundle_selection_portable_bundle_precedes_workspace() {
    let (_workspace, exe_dir, bundle) = workspace_fixture();
    let portable = exe_dir.join("plugins");
    std::fs::create_dir(&portable).unwrap();
    assert_eq!(bundle_root_from_exe_dir(&exe_dir), Some(portable));
    assert!(!bundle.exists());
}

/// 공식 플러그인은 workspace에 크레이트와 더 새 빌드가 있어도 스테이징한 산출물을 그대로 쓴다.
#[test]
fn bundle_selection_keeps_staged_official_plugins_over_newer_workspace_builds() {
    let workspace = tempfile::tempdir().unwrap();
    let exe_dir = workspace.path().join("target/profile");
    let spec = official_spec();
    write_workspace_sources(workspace.path(), &exe_dir, spec);
    let bundle = exe_dir.join("builtin-plugins");
    let staged = bundle.join(spec.id);
    std::fs::create_dir_all(&staged).unwrap();
    for (path, content) in [
        ("tasty-plugin.toml", "released manifest"),
        (spec.bin_name, "released binary"),
    ] {
        std::fs::write(staged.join(path), content).unwrap();
        stamp_epoch(&staged.join(path));
    }

    assert_eq!(bundle_root_from_exe_dir(&exe_dir), Some(bundle));
    assert_eq!(
        std::fs::read_to_string(staged.join(spec.bin_name)).unwrap(),
        "released binary"
    );
    assert_eq!(
        std::fs::read_to_string(staged.join("tasty-plugin.toml")).unwrap(),
        "released manifest"
    );
    assert!(!staged.join("tasty-plugin.toml.sig").exists());
    assert!(!staged.join("lang").exists());
}

/// 공식 플러그인 workspace 빌드만 있으면 debug도 개발 번들을 만들지 않는다.
#[test]
fn bundle_selection_does_not_build_a_bundle_from_official_workspace_crates() {
    let workspace = tempfile::tempdir().unwrap();
    let exe_dir = workspace.path().join("target/profile");
    write_workspace_sources(workspace.path(), &exe_dir, official_spec());
    assert_eq!(bundle_root_from_exe_dir(&exe_dir), None);
    assert!(!exe_dir.join("builtin-plugins").exists());
}

/// workspace에서 동기화하는 것은 이 레포에 남는 mesh-demo뿐이다.
#[test]
fn only_mesh_demo_is_synced_from_the_workspace() {
    let synced: Vec<&str> = BUILTINS
        .iter()
        .filter(|s| s.source == BuiltinSource::Workspace)
        .map(|s| s.id)
        .collect();
    assert_eq!(synced, vec!["com.tasty.mesh-demo"]);
}
