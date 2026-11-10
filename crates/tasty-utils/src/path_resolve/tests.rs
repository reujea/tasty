use super::*;

/// 이 스레드의 실행 폴더와 홈을 바꾸고 drop에서 되돌린다.
struct Scope;

impl Scope {
    fn new(launch: LaunchDir, home: Option<PathBuf>) -> Self {
        push_launch_dir_override(launch);
        push_os_home_override(home);
        Scope
    }

    fn known(dir: &Path) -> Self {
        Self::new(LaunchDir::Known(dir.to_path_buf()), Some(dir.join("home")))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        pop_os_home_override();
        pop_launch_dir_override();
    }
}

#[test]
fn empty_nul_and_url_inputs_are_errors() {
    let _s = Scope::known(Path::new(if cfg!(windows) { r"D:\d" } else { "/d" }));
    assert_eq!(resolve_path(""), Err(PathResolveError::Empty));
    assert_eq!(resolve_path("a\0b"), Err(PathResolveError::Invalid));
    assert_eq!(
        resolve_path("http://x"),
        Err(PathResolveError::NotLocalPath)
    );
    assert_eq!(
        resolve_path("file:///x"),
        Err(PathResolveError::NotLocalPath)
    );
    assert_eq!(resolve_command(""), Err(PathResolveError::Empty));
    assert_eq!(
        resolve_command("http://x"),
        Err(PathResolveError::NotLocalPath)
    );
}

#[test]
fn bare_name_rule_is_lexical() {
    for name in ["zsh", "bash.exe", "my-sh", "a.md", "~name"] {
        assert!(is_bare_name(name), "{name}");
    }
    for path in ["", "~", ".", "..", "a/b", "./zsh", "/bin/zsh", "~/x"] {
        assert!(!is_bare_name(path), "{path}");
    }
    // `\`와 드라이브 접두는 Windows에서만 구분자다.
    assert_eq!(is_bare_name(r"a\b"), !cfg!(windows));
    assert_eq!(is_bare_name("C:zsh"), !cfg!(windows));
}

#[cfg(unix)]
mod unix {
    use super::*;

    fn d() -> PathBuf {
        PathBuf::from("/launch/d")
    }

    #[test]
    fn relative_inputs_are_anchored_at_the_launch_dir() {
        let _s = Scope::known(&d());
        assert_eq!(resolve_path("./x"), Ok(d().join("x")));
        assert_eq!(
            resolve_path("../y/x"),
            Ok(PathBuf::from("/launch/d/../y/x"))
        );
        assert_eq!(resolve_path("rel/x"), Ok(d().join("rel/x")));
        assert_eq!(resolve_path("."), Ok(d()));
        assert_eq!(resolve_path(".."), Ok(PathBuf::from("/launch/d/..")));
    }

    /// 파일 경로는 구분자가 없어도 PATH를 보지 않고 실행 폴더 기준이다.
    #[test]
    fn a_bare_file_name_is_a_launch_dir_path_not_a_path_search() {
        let _s = Scope::known(&d());
        assert_eq!(resolve_path("a.md"), Ok(d().join("a.md")));
        assert_eq!(
            resolve_command("a.md"),
            Ok(CommandTarget::Search("a.md".into()))
        );
    }

    #[test]
    fn absolute_inputs_only_lose_dot_segments() {
        let _s = Scope::known(&d());
        assert_eq!(resolve_path("/a/b"), Ok(PathBuf::from("/a/b")));
        assert_eq!(resolve_path("/a/./b//c"), Ok(PathBuf::from("/a/b/c")));
        assert_eq!(resolve_path("/a/../b"), Ok(PathBuf::from("/a/../b")));
    }

    #[test]
    fn tilde_expands_to_the_fixed_home_only_in_its_bare_forms() {
        let _s = Scope::new(LaunchDir::Known(d()), Some(PathBuf::from("/home/u")));
        assert_eq!(resolve_path("~"), Ok(PathBuf::from("/home/u")));
        assert_eq!(resolve_path("~/x"), Ok(PathBuf::from("/home/u/x")));
        assert_eq!(resolve_path("~user"), Ok(d().join("~user")));
        assert_eq!(resolve_path(r"~\x"), Ok(d().join(r"~\x")));
        assert_eq!(
            resolve_command("~"),
            Ok(CommandTarget::Path(PathBuf::from("/home/u")))
        );
        assert_eq!(
            resolve_command("~name"),
            Ok(CommandTarget::Search("~name".into()))
        );
    }

    /// `~` 뒤 구분자가 여럿이어도 홈 기준을 잃지 않는다.
    #[test]
    fn repeated_separators_after_tilde_stay_under_home() {
        let _s = Scope::new(LaunchDir::Known(d()), Some(PathBuf::from("/home/u")));
        assert_eq!(
            resolve_path("~//bin/sh"),
            Ok(PathBuf::from("/home/u/bin/sh"))
        );
        assert_eq!(resolve_path("~///"), Ok(PathBuf::from("/home/u")));
        assert_eq!(
            resolve_command("~//bin/sh"),
            Ok(CommandTarget::Path(PathBuf::from("/home/u/bin/sh")))
        );
    }

    /// 상대 홈은 고정 기준이 아니므로 `~` 해석에 쓰지 않는다.
    #[test]
    fn a_relative_home_is_not_used_for_tilde() {
        let _s = Scope::new(LaunchDir::Known(d()), Some(PathBuf::from("relhome")));
        assert_eq!(resolve_path("~/sh"), Err(PathResolveError::HomeUnavailable));
        assert_eq!(resolve_path("~"), Err(PathResolveError::HomeUnavailable));
    }

    /// UTF-8이 아닌 실행 폴더 이름도 바이트 그대로 고정하고 기준으로 쓴다.
    #[test]
    fn a_non_utf8_launch_dir_is_kept_byte_for_byte() {
        use std::os::unix::ffi::OsStrExt;
        let dir = PathBuf::from(OsStr::from_bytes(b"/launch/\xff-dir"));
        assert_eq!(
            launch_dir_from_cwd(Ok(dir.clone())),
            LaunchDir::Known(dir.clone())
        );
        let _s = Scope::known(&dir);
        assert_eq!(resolve_path("./sh"), Ok(dir.join("sh")));
    }

    /// `canonical`도 실제 경로를 바이트 그대로 돌려준다.
    #[test]
    fn canonical_keeps_a_non_utf8_real_path() {
        use std::os::unix::ffi::OsStrExt;
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join(OsStr::from_bytes(b"\xfe-real"));
        std::fs::create_dir(&dir).expect("dir");
        assert_eq!(
            canonical(&dir).expect("canonical"),
            std::fs::canonicalize(&dir).expect("std canonicalize")
        );
    }

    #[test]
    fn tilde_without_a_home_is_an_error() {
        let _s = Scope::new(LaunchDir::Known(d()), None);
        assert_eq!(resolve_path("~/x"), Err(PathResolveError::HomeUnavailable));
    }

    #[test]
    fn an_unavailable_launch_dir_rejects_only_relative_inputs() {
        let _s = Scope::new(
            LaunchDir::Unavailable("gone".into()),
            Some(PathBuf::from("/home/u")),
        );
        assert_eq!(
            resolve_path("a"),
            Err(PathResolveError::LaunchDirUnavailable("gone".into()))
        );
        assert_eq!(resolve_path("/a"), Ok(PathBuf::from("/a")));
        assert_eq!(resolve_path("~/a"), Ok(PathBuf::from("/home/u/a")));
        assert_eq!(
            resolve_command("./sh"),
            Err(PathResolveError::LaunchDirUnavailable("gone".into()))
        );
        assert_eq!(
            resolve_command("sh"),
            Ok(CommandTarget::Search("sh".into()))
        );
    }

    #[test]
    fn commands_with_a_separator_resolve_at_the_launch_dir() {
        let _s = Scope::known(&d());
        assert_eq!(
            resolve_command("zsh"),
            Ok(CommandTarget::Search("zsh".into()))
        );
        assert_eq!(
            resolve_command("./sh"),
            Ok(CommandTarget::Path(d().join("sh")))
        );
        assert_eq!(
            resolve_command("rel/sh"),
            Ok(CommandTarget::Path(d().join("rel/sh")))
        );
        assert_eq!(resolve_command("."), Ok(CommandTarget::Path(d())));
        assert_eq!(
            resolve_command(r"a\b"),
            Ok(CommandTarget::Search(r"a\b".into()))
        );
    }

    #[test]
    fn a_relative_base_is_first_anchored_at_the_launch_dir() {
        let _s = Scope::known(&d());
        assert_eq!(
            resolve_path_against("x.md", Path::new("docs")),
            Ok(d().join("docs/x.md"))
        );
        assert_eq!(
            resolve_path_against("x.md", Path::new("/abs")),
            Ok(PathBuf::from("/abs/x.md"))
        );
    }

    fn exec(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, "#!/bin/sh\n").expect("write");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn join(dirs: &[&Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs).expect("join")
    }

    #[test]
    fn a_bare_name_is_found_on_path_not_in_the_launch_dir() {
        let root = tempfile::tempdir().expect("tempdir");
        let launch = root.path().join("launch");
        let opt = root.path().join("opt");
        exec(&launch.join("zsh"));
        exec(&opt.join("zsh"));
        let _s = Scope::known(&launch);
        let target = resolve_command("zsh").expect("classify");
        let path = join(&[&opt, Path::new("/usr/bin")]);
        assert_eq!(locate_command_in(&target, &path), Ok(opt.join("zsh")));
    }

    #[test]
    fn a_name_missing_from_path_is_not_taken_from_the_launch_dir() {
        let root = tempfile::tempdir().expect("tempdir");
        let launch = root.path().join("launch");
        exec(&launch.join("tasty-no-such-shell"));
        let _s = Scope::known(&launch);
        let target = resolve_command("tasty-no-such-shell").expect("classify");
        let path = join(&[&root.path().join("empty")]);
        assert_eq!(
            locate_command_in(&target, &path),
            Err(LocateError::NotFoundInPath("tasty-no-such-shell".into()))
        );
    }

    #[test]
    fn relative_and_empty_path_entries_are_anchored_at_the_launch_dir() {
        let root = tempfile::tempdir().expect("tempdir");
        let launch = root.path().join("launch");
        exec(&launch.join("rel/sh"));
        exec(&launch.join("only-here"));
        let _s = Scope::known(&launch);
        let sh = resolve_command("sh").expect("classify");
        assert_eq!(
            locate_command_in(&sh, OsStr::new("rel")),
            Ok(launch.join("rel/sh"))
        );
        let here = resolve_command("only-here").expect("classify");
        assert_eq!(
            locate_command_in(&here, OsStr::new("/nonexistent-tasty-dir:")),
            Ok(launch.join("only-here"))
        );
        // 기준을 명시하는 판은 실행 폴더 대신 그 기준을 쓴다.
        assert_eq!(
            locate_command_in_against(&sh, OsStr::new("rel"), &launch),
            Ok(launch.join("rel/sh"))
        );
    }

    #[test]
    fn directories_and_non_executable_files_are_skipped() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().expect("tempdir");
        let a = root.path().join("a");
        let b = root.path().join("b");
        let c = root.path().join("c");
        std::fs::create_dir_all(a.join("sh")).expect("dir");
        std::fs::create_dir_all(&b).expect("dir");
        std::fs::write(b.join("sh"), "").expect("write");
        std::fs::set_permissions(b.join("sh"), std::fs::Permissions::from_mode(0o644))
            .expect("chmod");
        exec(&c.join("sh"));
        let _s = Scope::known(root.path());
        let target = CommandTarget::Search("sh".into());
        assert_eq!(
            locate_command_in(&target, &join(&[&a, &b, &c])),
            Ok(c.join("sh"))
        );
    }

    #[test]
    fn a_path_target_is_returned_without_searching() {
        let target = CommandTarget::Path(PathBuf::from("/nonexistent/sh"));
        assert_eq!(
            locate_command_in(&target, OsStr::new("/usr/bin")),
            Ok(PathBuf::from("/nonexistent/sh"))
        );
    }
}

/// 고정값(override 없음)은 스레드와 파일시스템 변화에 관계없이 같은 결과를 낸다.
#[test]
fn the_fixed_launch_dir_gives_the_same_result_everywhere() {
    let root = tempfile::tempdir().expect("tempdir");
    let first = resolve_path("rel/x");
    let threads: Vec<_> = (0..2)
        .map(|_| std::thread::spawn(|| resolve_path("rel/x")))
        .collect();
    for t in threads {
        assert_eq!(t.join().expect("thread"), first);
    }
    // 절대화 결과는 대상 파일이 생기거나 지워져도 바뀌지 않는다.
    let target = root.path().join("made");
    let raw = target.to_str().expect("utf-8 temp path").to_owned();
    let before = resolve_path(&raw);
    std::fs::write(&target, "").expect("write");
    let during = resolve_path(&raw);
    std::fs::remove_file(&target).expect("remove");
    assert_eq!(before, during);
    assert_eq!(before, resolve_path(&raw));
    assert_eq!(resolve_path("~"), resolve_path("~"));
}

#[test]
fn the_plugin_launch_dir_comes_only_from_an_absolute_env_value() {
    assert!(matches!(
        launch_dir_from_env(None),
        LaunchDir::Unavailable(_)
    ));
    assert!(matches!(
        launch_dir_from_env(Some(OsStr::new(""))),
        LaunchDir::Unavailable(_)
    ));
    assert!(matches!(
        launch_dir_from_env(Some(OsStr::new("rel"))),
        LaunchDir::Unavailable(_)
    ));
    let abs = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        launch_dir_from_env(Some(abs.path().as_os_str())),
        LaunchDir::Known(abs.path().to_path_buf())
    );
}

#[cfg(windows)]
mod windows_platform {
    use super::*;

    #[test]
    fn windows_inputs_follow_the_rule_table() {
        let _s = Scope::new(
            LaunchDir::Known(PathBuf::from(r"D:\launch")),
            Some(PathBuf::from(r"C:\Users\u")),
        );
        assert_eq!(resolve_path(r"C:\a"), Ok(PathBuf::from(r"C:\a")));
        assert_eq!(resolve_path(r"C:/a\b"), Ok(PathBuf::from(r"C:\a\b")));
        assert_eq!(
            resolve_path(r"\\srv\share\a"),
            Ok(PathBuf::from(r"\\srv\share\a"))
        );
        assert_eq!(resolve_path(r"\\?\C:\a"), Ok(PathBuf::from(r"C:\a")));
        assert_eq!(resolve_path("/c/a"), Ok(PathBuf::from(r"C:\a")));
        assert_eq!(resolve_path(r"\a"), Err(PathResolveError::RootRelative));
        assert_eq!(resolve_path("C:a"), Err(PathResolveError::DriveRelative));
        assert_eq!(
            resolve_path(r"rel\x"),
            Ok(PathBuf::from(r"D:\launch\rel\x"))
        );
        assert_eq!(resolve_path(r"~\x"), Ok(PathBuf::from(r"C:\Users\u\x")));
        assert_eq!(
            resolve_command(r"a\b"),
            Ok(CommandTarget::Path(PathBuf::from(r"D:\launch\a\b")))
        );
        assert_eq!(
            resolve_command("C:zsh"),
            Err(PathResolveError::DriveRelative)
        );
    }

    #[test]
    fn repeated_separators_after_tilde_stay_under_home_on_windows() {
        let _s = Scope::new(
            LaunchDir::Known(PathBuf::from(r"D:\launch")),
            Some(PathBuf::from(r"C:\Users\u")),
        );
        assert_eq!(
            resolve_path(r"~\\bin\bash.exe"),
            Ok(PathBuf::from(r"C:\Users\u\bin\bash.exe"))
        );
    }

    /// 드라이브 상대·루트 상대 PATH 항목은 건너뛰고 결과는 언제나 절대 경로다.
    #[test]
    fn drive_dependent_path_entries_are_skipped() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("bash.exe"), "").expect("write");
        let _s = Scope::known(root.path());
        let target = resolve_command("bash.exe").expect("classify");
        let path = std::env::join_paths([
            PathBuf::from("C:tools"),
            PathBuf::from(r"\tools"),
            root.path().to_path_buf(),
        ])
        .expect("join");
        let found = locate_command_in(&target, &path).expect("found");
        assert!(found.is_absolute());
        assert_eq!(found, root.path().join("bash.exe"));
    }

    #[test]
    fn a_bare_name_gets_executable_extensions_on_path() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("bash.exe"), "").expect("write");
        let _s = Scope::known(root.path());
        let target = resolve_command("bash").expect("classify");
        let found = locate_command_in(&target, root.path().as_os_str()).expect("found");
        // 후보 이름은 PATHEXT 표기(보통 대문자 .EXE)를 따르고 NTFS는 대소문자를 가리지 않으므로
        // 파일 이름은 대소문자를 무시해 비교한다.
        assert_eq!(found.parent(), Some(root.path()));
        let name = found
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 name");
        assert!(name.eq_ignore_ascii_case("bash.exe"), "{name}");
    }
}
