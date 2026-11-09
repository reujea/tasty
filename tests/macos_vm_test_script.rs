//! `scripts/macos-vm-test.sh` 의 거절 자리마다 **자기 문구**로 멈추는가.
//!
//! 이 스크립트의 거절은 전부 `die()` 하나로 rc 2 를 낸다. rc 만 보면 어느 자리였는지 못
//! 가른다(`refusals_are_told_apart_by_their_own_words`) — 그래서 각 시험은 그 자리의 문구를
//! 단정한다.
//!
//! 진짜 가상머신은 안 띄운다. `tart` · `sshpass` · `rsync` 를 스텁으로 갈아 끼우고, 스텁의
//! 답은 환경변수로 정한다. 디스크 여유를 묻는 게스트 명령(`tasty-free-kb` 표지)에는 `STUB_FREE_KB`
//! (기본: 넉넉한 값)로, 빌드 캐시 크기(`tasty-target-kb`)에는 `STUB_TARGET_KB`(기본 1 GB)로, 지난 빌드
//! 커밋 표지(`tasty-built-rev`)에는 `STUB_BUILT_REV`(기본 없음)로 답한다. ssh 스텁은 받은 명령을
//! `STUB_SSH_LOG` 에 적는다. `rsync` 는 받은 인자를 `STUB_RSYNC_LOG` 에 한 줄씩 적는다. PATH 를 스텁 디렉토리와 시스템 디렉토리로만 좁혀, 이 기계에 깔린
//! 진짜 `tart` 가 끼어들지 않게 한다.

#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

fn script_src() -> String {
    format!("{}/scripts/macos-vm-test.sh", env!("CARGO_MANIFEST_DIR"))
}

fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).expect("스텁 쓰기");
    let mut perm = fs::metadata(path).expect("스텁 권한 읽기").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    fs::set_permissions(path, perm).expect("스텁 실행권한");
}

/// 스텁 셋을 `dir/bin` 에 깐다. `with_tart=false` 면 `tart` 만 뺀다.
fn install_stubs(dir: &Path, with_tart: bool) -> std::path::PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("스텁 디렉토리");
    if with_tart {
        write_exec(
            &bin.join("tart"),
            "#!/bin/sh\n\
             case \"$1\" in\n\
             list) [ \"$STUB_VM\" = 1 ] || exit 0\n\
                   if [ \"$2\" = --quiet ]; then echo tasty-macos;\n\
                   else echo 'local  tasty-macos  50GB 47GB now running'; fi ;;\n\
             ip) [ \"$STUB_IP\" = 1 ] && echo 10.0.0.9 || exit 1 ;;\n\
             exec) case \"$*\" in\n\
                   *tasty-free-kb*) echo \"${STUB_FREE_KB:-999999999}\" ;;\n\
                   *tasty-target-kb*) echo \"${STUB_TARGET_KB:-1048576}\" ;;\n\
                   *tasty-built-rev*) echo \"${STUB_BUILT_REV:-}\" ;;\n\
                   *sqlite3*) sql=$(cat); case \"$sql\" in\n\
                       *'count(*)'*) echo \"${STUB_TCC_COUNT:-}\" ;;\n\
                       *'select client'*) echo \"${STUB_AGENT:-}\" ;;\n\
                   esac ;;\n\
                   *osascript*) echo \"${STUB_OSA:-}\" ;;\n\
                   esac ;;\n\
             *) exit 0 ;;\n\
             esac\n",
        );
    }
    write_exec(
        &bin.join("sshpass"),
        "#!/bin/sh\n\
         [ \"$STUB_SSH\" = 1 ] || exit 255\n\
         [ -z \"$STUB_SSH_LOG\" ] || echo \"$*\" >> \"$STUB_SSH_LOG\"\n\
         case \"$*\" in\n\
         *tasty-free-kb*) echo \"${STUB_FREE_KB:-999999999}\"; exit 0 ;;\n\
         *tasty-target-kb*) echo \"${STUB_TARGET_KB:-1048576}\"; exit 0 ;;\n\
         *tasty-built-rev*) echo \"${STUB_BUILT_REV:-}\"; exit 0 ;;\n\
         *x86_64-w64-mingw32-gcc*) exit \"${STUB_MINGW_RC:-0}\" ;;\n\
         *vm-unit-build.log*) exit \"${STUB_BUILD_RC:-0}\" ;;\n\
         esac\n\
         printf '%s\\n' \"${STUB_SSH_OUT:-}\"\n",
    );
    write_exec(
        &bin.join("rsync"),
        "#!/bin/sh\n[ -z \"$STUB_RSYNC_LOG\" ] || echo \"$*\" >> \"$STUB_RSYNC_LOG\"\nexit 0\n",
    );
    bin
}

/// 스크립트를 `root/scripts/` 에 두고 돌린다. `repo=true` 면 root 를 tasty 레포처럼 꾸민다.
fn run(
    dir: &Path,
    repo: bool,
    with_tart: bool,
    args: &[&str],
    env: &[(&str, &str)],
) -> (i32, String) {
    let root = dir.join("root");
    fs::create_dir_all(root.join("scripts")).expect("scripts");
    fs::copy(script_src(), root.join("scripts/macos-vm-test.sh")).expect("스크립트 복사");
    if repo {
        if !root.join("Cargo.toml").exists() {
            fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("Cargo.toml");
        }
        let ok = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .expect("git init")
            .success();
        assert!(ok, "git init 실패");
    }
    let bin = install_stubs(dir, with_tart);
    let out = Command::new("bash")
        .arg(root.join("scripts/macos-vm-test.sh"))
        .args(args)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("TASTY_VM_SSH_TRIES", "1")
        .envs(env.iter().copied())
        .output()
        .expect("스크립트 실행");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn without_tart_it_says_tart_is_missing() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let (rc, text) = run(d.path(), true, false, &["test", "gates"], &[]);
    assert_eq!(rc, 2, "{text}");
    assert!(text.contains(" 이 없다 — 받는 법: "), "{text}");
    assert!(text.contains("tart 이 없다"), "풀린 값이 없다:\n{text}");
}

#[test]
fn an_unknown_scope_is_named_back() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let (rc, text) = run(d.path(), true, true, &["test", "bogus"], &[]);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("범위는 all·unit·gates·gui·windows·pkg 중 하나다: "),
        "{text}"
    );
    assert!(text.contains("하나다: bogus"), "풀린 값이 없다:\n{text}");
}

#[test]
fn a_missing_vm_points_at_prepare() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let (rc, text) = run(
        d.path(),
        true,
        true,
        &["test", "gates"],
        &[("STUB_VM", "0")],
    );
    assert_eq!(rc, 2, "{text}");
    assert!(text.contains(" 가 없다 — 먼저 prepare"), "{text}");
    assert!(
        text.contains("tasty-macos 가 없다"),
        "풀린 값이 없다:\n{text}"
    );
}

#[test]
fn a_vm_without_an_address_says_so() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [("STUB_VM", "1"), ("STUB_IP", "0")];
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(text.contains(" 의 IP 를 120 초 안에 못 받았다"), "{text}");
}

#[test]
fn a_vm_that_never_opens_ssh_says_how_long_it_waited() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [("STUB_VM", "1"), ("STUB_IP", "1"), ("STUB_SSH", "0")];
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(text.contains(" 초 안에 안 열렸다"), "{text}");
    assert!(
        text.contains("(10.0.0.9) 에 ssh 가 2 초"),
        "풀린 값이 없다:\n{text}"
    );
}

/// 옮기기는 `--delete` 로 민다 — 원본이 레포가 아니면 옮기기 **전에** 멈춰야 한다.
#[test]
fn a_source_that_is_not_the_repo_is_refused_before_syncing() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [("STUB_VM", "1"), ("STUB_IP", "1"), ("STUB_SSH", "1")];
    let (rc, text) = run(d.path(), false, true, &["test", "gates"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(text.contains("옮길 원본이 tasty 레포가 아니다"), "{text}");
    assert!(
        !text.contains("작업 트리를 옮긴다"),
        "옮기기를 시작했다:\n{text}"
    );
}

/// 입력 권한이 없으면 GUI 시험이 조용히 Timeout 으로만 죽으므로, 돌리기 전에 멈춘다.
#[test]
fn a_session_without_input_access_stops_the_gui_scope() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "false"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gui"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("로 띄운 프로세스에 손쉬운 사용·이벤트 보내기 권한이 없다(got: "),
        "{text}"
    );
    assert!(text.contains("ssh 로 띄운"), "길 이름이 안 풀렸다:\n{text}");
    assert!(text.contains("(got: false)"), "풀린 값이 없다:\n{text}");
    assert!(
        text.contains("작업 트리를 옮긴다"),
        "레포 원본인데 옮기지 않았다:\n{text}"
    );
    assert!(
        !text.contains("▶ gui"),
        "권한 없이 시험을 시작했다:\n{text}"
    );
}

/// exec 길(bridged)은 게스트 에이전트가 시험을 띄운다 — 그 에이전트에 화면 기록 권한이 없으면
/// 권한 요청 창이 시험 창을 가리므로 GUI 단계를 시작하기 전에 멈춘다.
#[test]
fn a_guest_agent_without_screen_capture_stops_the_gui_scope() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("TASTY_VM_NET", "bridged:en5"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gui"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("게스트 에이전트에 화면 기록 권한이 없다"),
        "{text}"
    );
    assert!(
        !text.contains("▶ gui"),
        "권한 없이 시험을 시작했다:\n{text}"
    );
}

/// 권한이 있으면 GUI 단계가 실제로 시작한다 — 위 두 시험의 부정 단언("옮기기를 시작했다" ·
/// "시험을 시작했다")이 그 문구가 바뀌어도 초록으로 남지 않게 하는 짝이다.
#[test]
fn a_session_with_input_access_runs_the_gui_scope() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gui"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(text.contains("작업 트리를 옮긴다"), "{text}");
    assert!(text.contains("▶ gui"), "{text}");
}

/// pkg 범위는 잴 패키지를 TASTY_VM_PKG 나 TASTY_VM_BASE 로 받는다 — 둘 다 없으면 단계를 시작하지 않고
/// 무엇이 빠졌는지 말한다.
#[test]
fn the_pkg_scope_stops_without_a_package_list() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "pkg"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("pkg 범위는 TASTY_VM_PKG 나 TASTY_VM_BASE 가 필요하다"),
        "{text}"
    );
    assert!(!text.contains("▶ pkg"), "단계를 시작했다:\n{text}");
}

/// 위 시험의 부정 단언("▶ pkg" 가 없다)의 짝 — 패키지를 주면 그 단계가 실제로 시작한다.
#[test]
fn the_pkg_scope_runs_with_a_package_list() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("TASTY_VM_PKG", "tasty-plugin-sdk tasty-ipc"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "pkg"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(text.contains("▶ pkg"), "{text}");
}

/// 기본(auto)은 지난 빌드의 커밋 표지가 없으면 캐시를 지운다 — 출처 모르는 캐시를 믿지 않는다.
/// 지운 뒤 디스크 여유를 잰다.
#[test]
fn a_test_run_clears_the_build_cache_before_syncing() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(text.contains("빌드 캐시를 지운다"), "{text}");
    assert!(
        text.contains("빌드 캐시를 지운다 — 지난 빌드의 커밋 표지가 없다"),
        "{text}"
    );
    let clean = text.find("빌드 캐시를 지운다").expect(&text);
    let free = text.find("디스크 여유 ").expect(&text);
    let sync = text.find("작업 트리를 옮긴다").expect(&text);
    assert!(clean < free && free < sync, "순서가 다르다:\n{text}");
}

/// `TASTY_VM_CLEAN=0` 이면 캐시를 남기되 디스크 여유는 그대로 잰다.
#[test]
fn clean_zero_keeps_the_cache_but_still_checks_the_disk() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("TASTY_VM_CLEAN", "0"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(text.contains("빌드 캐시를 남긴다"), "{text}");
    assert!(!text.contains("빌드 캐시를 지운다"), "{text}");
    assert!(text.contains("디스크 여유 "), "{text}");
}

/// 정리한 뒤에도 여유가 모자라면 옮기기 **전에** 멈춘다 — 빌드 도중 디스크가 차면 시험 실패처럼 보인다.
#[test]
fn a_full_disk_stops_before_syncing() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        // 2 GB (1 KB 단위) — 기본 문턱 12 GB 아래.
        ("STUB_FREE_KB", "2097152"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("디스크 여유가 2 GB 로 필요한 12 GB 보다 적다"),
        "{text}"
    );
    assert!(text.contains("GB 보다 적다 — shell 로 들어가"), "{text}");
    assert!(
        !text.contains("작업 트리를 옮긴다"),
        "옮기기를 시작했다:\n{text}"
    );
}

/// worktree 는 `.git` 이 파일이다 — 원본으로 받고, 공용 저장소와 그 worktree 의 HEAD·index 를 옮긴다.
#[test]
fn a_worktree_is_synced_with_its_common_git_dir() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let main = d.path().join("main");
    fs::create_dir_all(main.join("scripts")).expect("scripts");
    fs::write(main.join("Cargo.toml"), "[workspace]\n").expect("Cargo.toml");
    fs::copy(script_src(), main.join("scripts/macos-vm-test.sh")).expect("스크립트 복사");
    let git = |dir: &Path, args: &[&str]| {
        let ok = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {args:?} 실패");
    };
    git(&main, &["init", "-q"]);
    git(&main, &["add", "-A"]);
    git(&main, &["commit", "-q", "--no-verify", "-m", "init"]);
    git(&main, &["worktree", "add", "-q", "../wt", "-b", "lane"]);
    let wt = d.path().join("wt");
    let bin = install_stubs(d.path(), true);
    let log = d.path().join("rsync.log");
    let out = Command::new("bash")
        .arg(wt.join("scripts/macos-vm-test.sh"))
        .args(["test", "gates"])
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("TASTY_VM_SSH_TRIES", "1")
        .envs([
            ("STUB_VM", "1"),
            ("STUB_IP", "1"),
            ("STUB_SSH", "1"),
            ("STUB_SSH_OUT", "true"),
        ])
        .env("STUB_RSYNC_LOG", &log)
        .output()
        .expect("스크립트 실행");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("worktree 를 옮긴다 (lane)"), "{text}");
    let calls = fs::read_to_string(&log).expect("rsync 기록");
    let main_git = main.canonicalize().expect("경로").join(".git");
    assert!(
        calls.contains(&format!("{}/ ", main_git.display())),
        "공용 저장소를 안 옮겼다:\n{calls}"
    );
    assert!(
        calls.contains(&format!(
            "{}/HEAD ",
            main_git.join("worktrees/wt").display()
        )),
        "worktree 의 HEAD 를 안 옮겼다:\n{calls}"
    );
    assert!(
        calls.contains("--exclude /.git "),
        "트리 옮기기가 .git 을 안 뺐다:\n{calls}"
    );
}

/// 디스크 여유를 숫자로 못 읽으면 문턱과 비교할 수 없다 — 통과로 넘기지 않고 멈춘다.
#[test]
fn an_unreadable_disk_free_stops_before_syncing() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("STUB_FREE_KB", "df: no such file"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(text.contains("디스크 여유를 못 읽었다"), "{text}");
    assert!(!text.contains("작업 트리를 옮긴다"), "{text}");
}

/// exec 길에서 Apple 이벤트 허용을 넣은 뒤에도 osascript 가 숫자를 못 돌려주면 GUI 단계 전에 멈춘다 —
/// 그대로 가면 클릭 시험이 -1743 으로 하나씩 죽는다.
#[test]
fn a_guest_agent_denied_apple_events_stops_the_gui_scope() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("TASTY_VM_NET", "bridged:en5"),
        ("STUB_TCC_COUNT", "1"),
        ("STUB_AGENT", "/opt/homebrew/bin/tart-guest-agent"),
        ("STUB_OSA", "execution error: Not authorized (-1743)"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gui"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("System Events 에 Apple 이벤트를 못 보낸다"),
        "{text}"
    );
    assert!(text.contains("(-1743)"), "받은 값을 안 보여준다:\n{text}");
    assert!(!text.contains("▶ gui"), "{text}");
}

/// windows 범위는 교차 링커가 없으면 빌드를 시작하지 않는다 — 수 분 뒤 링크 단계에서 죽지 않게.
#[test]
fn the_windows_scope_needs_mingw() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("STUB_MINGW_RC", "1"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "windows"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(text.contains("mingw-w64 를 깔지 않았다"), "{text}");
    assert!(!text.contains("▶ windows"), "{text}");
}

/// 교차 링커가 있으면 windows 단계를 시작한다 — 위 시험의 부정 단언이 기대는 짝이다.
#[test]
fn the_windows_scope_runs_with_mingw() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "windows"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(text.contains("▶ windows"), "{text}");
}

/// auto 판정을 보려고 커밋 하나가 있는 레포를 미리 만든다 — 크레이트 셋(a·b·c)과 루트 패키지.
/// `run` 이 같은 자리에 `git init` 을 다시 해도 커밋은 남는다. 돌려주는 값은 그 커밋이다.
fn seeded_repo(dir: &Path) -> String {
    let root = dir.join("root");
    for c in ["a", "b", "c"] {
        fs::create_dir_all(root.join(format!("crates/{c}/src"))).expect("crate");
        fs::write(root.join(format!("crates/{c}/Cargo.toml")), "[package]\n").expect("Cargo.toml");
        fs::write(root.join(format!("crates/{c}/src/lib.rs")), "\n").expect("lib.rs");
    }
    fs::create_dir_all(root.join("scripts")).expect("scripts");
    fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("Cargo.toml");
    fs::write(root.join(".gitignore"), "/scripts/\n").expect(".gitignore");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .current_dir(&root)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "seed"]);
    git(&["rev-parse", "HEAD"])
}

fn auto_env<'a>(rev: &'a str, log: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("STUB_BUILT_REV", rev),
        ("STUB_SSH_LOG", log),
    ]
}

/// 지난 빌드 뒤 크레이트 하나만 바뀌었으면(1/4) 캐시를 남기고, 옮긴 뒤 이번 커밋을 표지로 남긴다.
#[test]
fn auto_keeps_the_cache_when_few_crates_changed_and_stamps_the_commit() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let rev = seeded_repo(d.path());
    fs::write(d.path().join("root/crates/a/src/lib.rs"), "// x\n").expect("고치기");
    let log = d.path().join("ssh.log");
    let log = log.to_str().expect("경로");
    let (rc, text) = run(
        d.path(),
        true,
        true,
        &["test", "gates"],
        &auto_env(&rev, log),
    );
    assert_eq!(rc, 0, "{text}");
    assert!(
        text.contains("빌드 캐시를 남긴다 (바뀐 크레이트 1/4)"),
        "{text}"
    );
    assert!(!text.contains("빌드 캐시를 지운다"), "{text}");
    let cmds = fs::read_to_string(log).expect("ssh 기록");
    assert!(
        cmds.lines()
            .any(|l| l.contains(".tasty-vm-built-rev") && l.contains(&rev)),
        "표지를 안 남겼다:\n{cmds}"
    );
}

/// 바뀐 크레이트가 절반을 넘으면(3/4) 지운다.
#[test]
fn auto_clears_the_cache_when_most_crates_changed() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let rev = seeded_repo(d.path());
    for c in ["a", "b"] {
        fs::write(
            d.path().join(format!("root/crates/{c}/src/lib.rs")),
            "// x\n",
        )
        .expect("고치기");
    }
    fs::create_dir_all(d.path().join("root/src")).expect("src");
    fs::write(d.path().join("root/src/main.rs"), "fn main() {}\n").expect("main.rs");
    let log = d.path().join("ssh.log");
    let log = log.to_str().expect("경로");
    let (rc, text) = run(
        d.path(),
        true,
        true,
        &["test", "gates"],
        &auto_env(&rev, log),
    );
    assert_eq!(rc, 0, "{text}");
    assert!(
        text.contains("빌드 캐시를 지운다 — 바뀐 크레이트 3/4 가 50% 를 넘는다"),
        "{text}"
    );
}

/// 작업 공간 설정(Cargo.lock 등)이 바뀌면 바뀐 크레이트 수와 상관없이 지운다.
#[test]
fn auto_clears_the_cache_when_the_lockfile_changed() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let rev = seeded_repo(d.path());
    fs::write(d.path().join("root/Cargo.lock"), "# x\n").expect("Cargo.lock");
    let log = d.path().join("ssh.log");
    let log = log.to_str().expect("경로");
    let (rc, text) = run(
        d.path(),
        true,
        true,
        &["test", "gates"],
        &auto_env(&rev, log),
    );
    assert_eq!(rc, 0, "{text}");
    assert!(
        text.contains("빌드 캐시를 지운다 — 작업 공간 설정"),
        "{text}"
    );
}

/// 캐시가 문턱보다 크면 바뀐 것이 없어도 지운다.
#[test]
fn auto_clears_an_oversized_cache() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let rev = seeded_repo(d.path());
    let log = d.path().join("ssh.log");
    let log = log.to_str().expect("경로");
    let mut env = auto_env(&rev, log);
    // 30 GB (1 KB 단위) — 기본 문턱 24 GB 위.
    env.push(("STUB_TARGET_KB", "31457280"));
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(
        text.contains("빌드 캐시를 지운다 — 빌드 캐시 30 GB 가 24 GB 를 넘는다"),
        "{text}"
    );
}

/// 문턱 값이 숫자가 아니면 판정에 쓰기 전에 멈춘다.
#[test]
fn a_non_numeric_clean_threshold_is_refused() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [("TASTY_VM_CLEAN_DIRTY_PCT", "half")];
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("TASTY_VM_CLEAN_DIRTY_PCT 는 자연수다"),
        "{text}"
    );
}

/// 남긴 캐시는 빌드 몫을 이미 차지한다 — 캐시 10 GB · 여유 5 GB 면 하한(3 GB)만 요구하고 남긴다.
/// 고정 문턱(12 GB)이면 unit 한 번 뒤 늘 지우게 된다.
#[test]
fn auto_keeps_a_full_cache_when_the_disk_fits_the_rebuild() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let rev = seeded_repo(d.path());
    let log = d.path().join("ssh.log");
    let log = log.to_str().expect("경로");
    let mut env = auto_env(&rev, log);
    env.push(("STUB_TARGET_KB", "10485760"));
    env.push(("STUB_FREE_KB", "5242880"));
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(text.contains("빌드 캐시를 남긴다"), "{text}");
    assert!(text.contains("디스크 여유 5 GB (필요 3 GB)"), "{text}");
}

/// 캐시가 작으면 처음 빌드 몫에 가깝게 요구한다 — 캐시 1 GB · 여유 5 GB 면 11 GB 가 모자라 지운다.
#[test]
fn auto_clears_a_small_cache_when_the_disk_is_short() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let rev = seeded_repo(d.path());
    let log = d.path().join("ssh.log");
    let log = log.to_str().expect("경로");
    let mut env = auto_env(&rev, log);
    env.push(("STUB_FREE_KB", "5242880"));
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("빌드 캐시를 지운다 — 디스크 여유 5 GB 가 필요한 11 GB 보다 적다"),
        "{text}"
    );
}

/// 구간마다 걸린 시간을 `timings.tsv` 에 한 줄씩 남기고 끝에 요약한다 — 실행끼리 어디가 느려졌는지 비교한다.
#[test]
fn a_test_run_records_how_long_each_phase_took() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "gates"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(text.contains("구간별 시간"), "{text}");
    let tsv =
        fs::read_to_string(d.path().join("root/target/macos-vm/timings.tsv")).expect("timings.tsv");
    let phases: Vec<&str> = tsv
        .lines()
        .skip(1)
        .map(|l| l.split('\t').next().unwrap_or(""))
        .collect();
    assert_eq!(phases, ["boot", "clean", "sync", "gates"], "{tsv}");
    for line in tsv.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 3, "{line}");
        assert!(cols[1].parse::<u64>().is_ok(), "{line}");
        assert_eq!(cols[2], "0", "{line}");
    }
}

/// unit 은 빌드와 시험을 단계로 나눠 각자의 시간을 남기고, 빌드가 실패하면 시험을 돌리지 않는다.
#[test]
fn the_unit_scope_times_the_build_apart_from_the_tests() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "unit"], &env);
    assert_eq!(rc, 0, "{text}");
    let build = text.find("▶ unit-build").expect(&text);
    let test = text.find("▶ unit\n").expect(&text);
    assert!(build < test, "{text}");
    let tsv =
        fs::read_to_string(d.path().join("root/target/macos-vm/timings.tsv")).expect("timings.tsv");
    assert!(
        tsv.contains("\nunit-build\t") && tsv.contains("\nunit\t"),
        "{tsv}"
    );
}

/// 빌드가 실패하면 시험 단계는 돌지 않고 unit 도 실패로 센다.
#[test]
fn a_failed_unit_build_skips_the_tests() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("STUB_BUILD_RC", "1"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "unit"], &env);
    assert_eq!(rc, 1, "{text}");
    assert!(text.contains("▶ unit-build"), "{text}");
    assert!(!text.contains("▶ unit\n"), "{text}");
    assert!(text.contains("실패한 단계: unit-build unit"), "{text}");
    let tsv =
        fs::read_to_string(d.path().join("root/target/macos-vm/timings.tsv")).expect("timings.tsv");
    assert!(
        tsv.contains("\nunit-build\t") && tsv.trim_end().ends_with("\t1"),
        "{tsv}"
    );
}

fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(root)
        .status()
        .expect("git")
        .success();
    assert!(ok, "git {args:?} 실패");
}

/// `files` 를 쓰고 커밋한다 — 첫 호출이 기준 커밋이 된다.
fn commit(root: &Path, files: &[(&str, &str)]) {
    for (path, body) in files {
        let p = root.join(path);
        fs::create_dir_all(p.parent().expect("부모")).expect("디렉토리");
        fs::write(p, body).expect("파일 쓰기");
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "c"]);
}

/// TASTY_VM_BASE 만 주면 분기점 뒤로 바뀐 파일의 패키지를 고른다 — crate 는 디렉토리가 아니라 Cargo.toml
/// 의 이름으로, 루트 소스(커밋 안 한 것 포함)는 tasty 로, 문서는 tasty-doc-guards 로. workspace exclude
/// 의 crate 는 루트가 path 의존으로 적어도 고르지 않고, 루트 Cargo.toml · scripts 도 고르지 않는다. 워크스페이스를 먼저 짓고 고른 패키지를 시험한다.
#[test]
fn the_pkg_scope_picks_the_packages_changed_since_the_base() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let root = d.path().join("root");
    fs::create_dir_all(&root).expect("root");
    git(&root, &["init", "-q"]);
    commit(
        &root,
        &[
            ("crates/foo/Cargo.toml", "[package]\nname = \"foo-pkg\"\n"),
            ("crates/bar/Cargo.toml", "[package]\nname = \"bar\"\n"),
            (
                "Cargo.toml",
                "[workspace]\nexclude = [\"crates/bar\"]\n[dependencies]\nbar = { path = \"crates/bar\" }\n",
            ),
        ],
    );
    commit(
        &root,
        &[
            ("crates/foo/src/lib.rs", ""),
            ("crates/bar/src/lib.rs", ""),
            ("docs/a.md", "a"),
            ("scripts/x.sh", ""),
        ],
    );
    fs::create_dir_all(root.join("src")).expect("src");
    fs::write(root.join("src/new.rs"), "").expect("커밋 안 한 파일");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("TASTY_VM_BASE", "HEAD~1"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "pkg"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(
        text.contains("바뀐 패키지 — foo-pkg tasty tasty-doc-guards\n"),
        "{text}"
    );
    let build = text.find("▶ unit-build").expect("빌드 단계가 없다");
    let pkg = text.find("▶ pkg").expect("pkg 단계가 없다");
    assert!(build < pkg, "빌드보다 시험이 먼저다:\n{text}");
}

/// 고를 패키지가 없으면 빌드만 하고 통과한다 — 빈 `cargo test` 는 워크스페이스 전체를 돌리므로 부르지 않는다.
#[test]
fn the_pkg_scope_only_builds_when_no_package_changed() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let root = d.path().join("root");
    fs::create_dir_all(&root).expect("root");
    git(&root, &["init", "-q"]);
    commit(&root, &[("Cargo.toml", "[workspace]\n")]);
    commit(&root, &[("scripts/x.sh", "")]);
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("TASTY_VM_BASE", "HEAD~1"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "pkg"], &env);
    assert_eq!(rc, 0, "{text}");
    assert!(text.contains("바뀐 패키지 — (없음)"), "{text}");
    assert!(text.contains("시험할 패키지가 없다"), "{text}");
    assert!(text.contains("▶ unit-build"), "{text}");
    assert!(!text.contains("▶ pkg"), "{text}");
}

/// 기준을 못 읽으면 가상머신을 띄우기 전에 그 이름을 대고 멈춘다.
#[test]
fn the_pkg_scope_names_an_unknown_base() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("TASTY_VM_BASE", "no-such-rev"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "pkg"], &env);
    assert_eq!(rc, 2, "{text}");
    assert!(
        text.contains("TASTY_VM_BASE 와의 분기점을 못 찾는다"),
        "{text}"
    );
    assert!(
        text.contains(": no-such-rev"),
        "기준 이름을 대지 않았다:\n{text}"
    );
    assert!(!text.contains("작업 트리를 옮긴다"), "{text}");
}

/// pkg 범위도 빌드가 실패하면 시험을 건너뛰고 pkg 를 실패로 센다.
#[test]
fn a_failed_build_skips_the_pkg_tests() {
    let d = tempfile::tempdir().expect("임시 디렉토리");
    let env = [
        ("STUB_VM", "1"),
        ("STUB_IP", "1"),
        ("STUB_SSH", "1"),
        ("STUB_SSH_OUT", "true"),
        ("STUB_BUILD_RC", "1"),
        ("TASTY_VM_PKG", "foo"),
    ];
    let (rc, text) = run(d.path(), true, true, &["test", "pkg"], &env);
    assert_eq!(rc, 1, "{text}");
    assert!(!text.contains("▶ pkg"), "{text}");
    assert!(text.contains("실패한 단계: unit-build pkg"), "{text}");
}
