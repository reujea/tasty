use super::*;

fn workspace_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let workspace = tempfile::tempdir().unwrap();
    let exe_dir = workspace.path().join("target/profile");
    let spec = &BUILTINS[0];
    let source = workspace.path().join("crates").join(spec.crate_dir);
    std::fs::create_dir_all(source.join("lang")).unwrap();
    std::fs::create_dir_all(&exe_dir).unwrap();
    std::fs::write(source.join("tasty-plugin.toml"), "edited manifest").unwrap();
    std::fs::write(source.join("tasty-plugin.toml.sig"), "old signature").unwrap();
    std::fs::write(source.join("lang/en.toml"), "edited language").unwrap();
    std::fs::write(exe_dir.join(spec.bin_name()), "new binary").unwrap();
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
    let spec = &BUILTINS[0];
    let staged = bundle.join(spec.id);
    let bin = spec.bin_name();
    std::fs::create_dir_all(staged.join("lang")).unwrap();
    let files = [
        ("tasty-plugin.toml", "signed manifest", "edited manifest"),
        ("tasty-plugin.toml.sig", "paired signature", "old signature"),
        ("lang/en.toml", "staged language", "edited language"),
        (bin.as_str(), "staged binary", "new binary"),
    ];
    for (path, original, _) in files {
        std::fs::write(staged.join(path), original).unwrap();
        std::fs::File::options()
            .write(true)
            .open(staged.join(path))
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH)
            .unwrap();
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
