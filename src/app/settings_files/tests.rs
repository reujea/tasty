#[cfg(unix)]
use super::*;

#[cfg(unix)]
#[test]
fn export_preserves_original_when_replacement_directory_is_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("bundle.toml");
    std::fs::write(&path, "original").unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = SettingsFileRequest::Export {
        path: path.clone(),
        text: "replacement".into(),
        observer: std::sync::Weak::new(),
    }
    .execute();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        result,
        SettingsFileResult::Export { result: Err(_), .. }
    ));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}
