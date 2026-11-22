//! `scripts/lib/run-lock.sh`(두 스크립트의 동시 실행 잠금)를 실제 bash로 확인한다.
//! - 잡기 결과 0 잡음·1 이미 있음·2 만들 수 없음, 놓은 뒤 잠금·표지가 남지 않음, 호출자의 noclobber 보존.
//! - 같은 경로를 두 bash가 동시에 잡으면 하나만 잡는다. PATH 앞에 늘 0으로 끝나는 가짜 `mkdir`이 있어도 같다
//!   (도우미는 외부 `mkdir`을 쓰지 않는다).
//! - 호출자 꼴 cleanup에서 신호로 끝나도 자기 잠금·표지를 남기지 않고 남의 잠금은 건드리지 않는다.
//!   반복 시험의 신호는 TERM만 쓴다(bash 5.2.21에서 INT를 계속 받으며 자식을 거듭 만드는 반복이 드물게 멈추는
//!   것을 관측했다. 도우미 머리 주석의 한계). INT는 대기 중과 cleanup 중에 한 번씩 보낸다.
//! - bash는 PATH의 것을 쓴다. 이 시험을 다른 bash로 돌리려면 PATH 앞에 그 bash를 둔다.

#![cfg(unix)]

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use tasty_doc_guards::temp_scratch::Scratch;

fn script() -> PathBuf {
    tasty_doc_guards::repo_root().join("scripts/lib/run-lock.sh")
}

/// `root`를 cwd로 도우미를 source한 뒤 `body`를 실행한다. (종료 코드, stdout, stderr).
fn run(root: &Path, body: &str) -> (i32, String, String) {
    let out = Command::new("bash")
        .arg("-c")
        .arg(format!(". \"$1\"\n{body}"))
        .arg("run-lock-test")
        .arg(script())
        .current_dir(root)
        .output()
        .expect("bash 실행");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .expect("폴더 읽기")
        .map(|e| e.expect("항목").file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn acquiring_a_missing_lock_makes_a_file_naming_an_existing_token() {
    let dir = Scratch::new("run-lock-acquire");
    let (rc, out, err) = run(
        dir.path(),
        "r=0; run_lock_acquire L || r=$?; printf '%s|%s' \"$r\" \"$run_lock_token\"",
    );
    assert_eq!(rc, 0, "{err}");
    let (r, token) = out.split_once('|').expect("출력 형식");
    assert_eq!(r, "0", "없는 잠금을 잡지 못했다: {err}");
    let lock = dir.path().join("L");
    assert!(lock.is_file(), "잠금이 일반 파일이 아니다");
    assert_eq!(
        std::fs::read_to_string(&lock).expect("잠금 읽기"),
        format!("{token}\n"),
        "잠금 내용이 표지 경로가 아니다"
    );
    assert!(dir.path().join(token).is_file(), "표지 파일이 없다");
}

#[test]
fn a_held_lock_is_reported_as_existing_and_left_alone() {
    let dir = Scratch::new("run-lock-held");
    let (rc, _, err) = run(dir.path(), "run_lock_acquire L");
    assert_eq!(rc, 0, "{err}");
    let before = std::fs::read(dir.path().join("L")).expect("잠금 읽기");
    let listed = names(dir.path());
    let (rc, out, err) = run(
        dir.path(),
        "r=0; run_lock_acquire L || r=$?; printf '%s' \"$r\"",
    );
    assert_eq!(
        (rc, out.as_str()),
        (0, "1"),
        "잡힌 잠금을 다시 잡았다: {err}"
    );
    assert_eq!(std::fs::read(dir.path().join("L")).expect("잠금"), before);
    assert_eq!(names(dir.path()), listed, "실패한 잡기가 파일을 남겼다");
}

#[test]
fn an_old_lock_folder_is_reported_as_existing() {
    let dir = Scratch::new("run-lock-folder");
    std::fs::create_dir(dir.path().join("L")).expect("옛 잠금 폴더");
    let (_, out, err) = run(
        dir.path(),
        "r=0; run_lock_acquire L || r=$?; printf '%s' \"$r\"",
    );
    assert_eq!(out, "1", "옛 판의 잠금 폴더를 거부하지 않았다: {err}");
    assert_eq!(names(dir.path()), vec!["L"]);
}

#[test]
fn a_missing_parent_folder_is_reported_as_cannot_create() {
    let dir = Scratch::new("run-lock-parent");
    let (_, out, err) = run(
        dir.path(),
        "r=0; run_lock_acquire nope/L || r=$?; printf '%s' \"$r\"",
    );
    assert_eq!(out, "2", "부모 폴더가 없는데 2가 아니다: {err}");
    assert!(names(dir.path()).is_empty());
}

#[test]
fn a_released_lock_leaves_nothing_and_can_be_taken_again() {
    let dir = Scratch::new("run-lock-release");
    let (rc, out, err) = run(
        dir.path(),
        "run_lock_acquire L || exit 9
run_lock_release || exit 8
ls -A
run_lock_acquire L || exit 7
run_lock_release || exit 6
ls -A",
    );
    assert_eq!(rc, 0, "{err}");
    assert_eq!(out, "", "놓은 뒤 남은 파일: {out}");
    assert_eq!(err, "", "경고가 나왔다");
}

#[test]
fn the_caller_noclobber_state_is_kept() {
    let dir = Scratch::new("run-lock-noclobber");
    let (rc, out, err) = run(
        dir.path(),
        "on() { case $- in *C*) printf C ;; *) printf - ;; esac; }
run_lock_acquire A; on; run_lock_acquire A; on
set -C
run_lock_acquire B; on; run_lock_acquire B; on",
    );
    assert_eq!(rc, 0, "{err}");
    assert_eq!(out, "--CC", "noclobber 상태가 바뀌었다");
}

#[test]
fn a_failed_token_write_is_cannot_create_and_leaves_nothing() {
    let dir = Scratch::new("run-lock-write");
    // 파일 크기 제한 0이면 빈 파일은 만들어지고 쓰기만 실패한다. 제한은 서브셸에만 둔다.
    let (rc, out, err) = run(
        dir.path(),
        "( trap '' XFSZ; ulimit -f 0; r=0; run_lock_acquire L 2>/dev/null || r=$?; exit \"$r\" )
printf '%s' \"$?\"",
    );
    assert_eq!(
        (rc, out.as_str()),
        (0, "2"),
        "쓰기 실패가 2가 아니다: {err}"
    );
    assert!(
        names(dir.path()).is_empty(),
        "남은 파일: {:?}",
        names(dir.path())
    );
}

#[test]
fn a_pending_release_removes_only_a_lock_holding_exactly_its_token() {
    let dir = Scratch::new("run-lock-pending");
    // 잡은 뒤 held를 pending으로 되돌려, 잡기 도중에 끝난 경우의 정리를 흉내 낸다.
    let (rc, out, err) = run(
        dir.path(),
        "run_lock_acquire L || exit 9
run_lock_held=pending
printf '%s\\nextra\\n' \"$run_lock_token\" > L
run_lock_release; printf '%s ' \"$?\"; ls -A | tr '\\n' ' '
rm L
run_lock_acquire L || exit 8
run_lock_held=pending
run_lock_release; printf '%s %s' \"$?\" \"$(ls -A | wc -l | tr -d ' ')\"",
    );
    assert_eq!(rc, 0, "{err}");
    assert_eq!(
        out, "0 L 0 0",
        "추가 줄이 붙은 잠금을 지웠거나 자기 잠금을 남겼다"
    );
}

/// 같은 경로를 두 bash가 시작 표지에 맞춰 함께 잡는 회차를 돈다. (둘 다 잡음, 둘 다 못 잡음) 회차 수.
fn contend(dir: &Path, rounds: u32, path_prefix: &str) -> (u32, u32) {
    const DRIVER: &str = r#"helper="$1"; work="$2"; rounds="$3"; prefix="$4"
both=0; none=0; i=0
while [ "$i" -lt "$rounds" ]; do
    i=$((i + 1))
    rm -rf "$work"; mkdir "$work"
    for k in 1 2; do
        ( PATH="$prefix$PATH"; . "$helper"; until [ -e "$work/go" ]; do :; done
          r=0; run_lock_acquire "$work/L" || r=$?; echo "$r" > "$work/r$k" ) &
    done
    : > "$work/go"
    wait
    case "$(cat "$work/r1")$(cat "$work/r2")" in
        01 | 10) ;;
        00) both=$((both + 1)) ;;
        *) none=$((none + 1)) ;;
    esac
done
rm -rf "$work"
printf '%s %s' "$both" "$none"
"#;
    let out = Command::new("bash")
        .args(["-c", DRIVER, "contend"])
        .arg(script())
        .arg(dir.join("w"))
        .arg(rounds.to_string())
        .arg(path_prefix)
        .output()
        .expect("bash 실행");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("출력");
    let (b, n) = text.split_once(' ').expect("출력 형식");
    (b.parse().expect("수"), n.parse().expect("수"))
}

#[test]
fn two_shells_racing_for_the_same_lock_get_exactly_one_holder() {
    let dir = Scratch::new("run-lock-race");
    assert_eq!(contend(dir.path(), 300, ""), (0, 0), "PATH 그대로");
    // 늘 0으로 끝나는 가짜 mkdir. 잠금이 mkdir에 기대면 두 쪽 모두 잡는다.
    let fake = dir.path().join("fake");
    std::fs::create_dir(&fake).expect("가짜 폴더");
    write_executable(&fake.join("mkdir"), "#!/bin/sh\nexit 0\n");
    let prefix = format!("{}:", fake.display());
    assert_eq!(contend(dir.path(), 300, &prefix), (0, 0), "가짜 mkdir 앞");
}

fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).expect("쓰기");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("권한");
}

/// 호출자 꼴 셸. `$2` 폴더에서 `$3` 동작을 한다: own(잡기·놓기 반복), other(남의 잠금 반복 잡기), hold(잡고 대기).
/// cleanup은 운영 스크립트와 같은 꼴이고, `$4`가 있으면 정리 중에 그만큼 쉰다.
const CALLER: &str = r#". "$1"
dir=$2 pause=${4:-}
cleanup() {
    run_lock_defer_signals "$?"
    local rc=$run_lock_status
    trap - EXIT
    case $- in *C*) echo "noclobber left on" >&2 ;; esac
    if [ -n "$pause" ]; then : > "$dir/cleaning"; sleep "$pause" || :; fi
    run_lock_release || echo "Warning: could not remove $dir/L" >&2
    run_lock_finish "$rc"
}
trap cleanup EXIT
trap 'run_lock_signal 130' INT
trap 'run_lock_signal 143' TERM
: > "$2/ready"
while :; do
    r=0
    run_lock_acquire "$2/L" || r=$?
    case $3 in
        own)
            [ "$r" -eq 0 ] || { echo "acquire $r" >&2; exit 9; }
            run_lock_release || { echo "release failed" >&2; exit 8; }
            [ -z "$run_lock_exit" ] || exit "$run_lock_exit"
            ;;
        other) [ "$r" -eq 1 ] || { echo "acquire $r" >&2; exit 7; } ;;
        hold)
            [ "$r" -eq 0 ] || { echo "acquire $r" >&2; exit 6; }
            while :; do sleep 0.01; done
            ;;
    esac
done
"#;

fn spawn_caller(dir: &Path, mode: &str, pause: &str) -> Child {
    // 새 프로세스 그룹으로 띄워 터미널 Ctrl-C처럼 그룹 전체에 신호를 보낸다. `&`로 띄운 셸과 달리 SIGINT가
    // 무시된 채 시작하지 않는다.
    Command::new("bash")
        .args(["-c", CALLER, "caller"])
        .arg(script())
        .arg(dir)
        .arg(mode)
        .arg(pause)
        .stderr(std::fs::File::create(dir.join("err")).expect("stderr 파일"))
        .process_group(0)
        .spawn()
        .expect("bash 실행")
}

fn signal_group(child: &Child, sig: &str) {
    // 이미 끝났으면 kill이 실패한다. 그 경우는 wait에서 판정하므로 kill의 종료 코드는 보지 않는다.
    Command::new("kill")
        .args([&format!("-{sig}"), "--", &format!("-{}", child.id())])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill 실행");
}

fn wait_for(path: &Path) {
    let end = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < end,
            "{} 를 기다리다 시간이 지났다",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// 끝날 때까지 `sig`를 다시 보내며 기다린다(5초). 종료 코드를 돌려준다.
fn finish(child: &mut Child, sig: Option<&str>) -> i32 {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            return status.code().unwrap_or(-1);
        }
        if Instant::now() > end {
            signal_group(child, "KILL");
            child.wait().expect("강제 종료한 셸 거두기");
            panic!("호출자 셸이 끝나지 않았다");
        }
        std::thread::sleep(Duration::from_millis(50));
        if let Some(sig) = sig {
            signal_group(child, sig);
        }
    }
}

/// 0..10ms 사이의 서로 다른 지연(회차마다 바뀐다).
fn jitter(i: u32) -> Duration {
    Duration::from_micros(u64::from(i.wrapping_mul(7919) % 10_000))
}

fn round_dir(base: &Path, i: u32) -> PathBuf {
    let d = base.join(format!("r{i}"));
    std::fs::create_dir(&d).expect("회차 폴더");
    d
}

fn left(d: &Path) -> Vec<String> {
    names(d)
        .into_iter()
        .filter(|n| !matches!(n.as_str(), "ready" | "err" | "cleaning"))
        .collect()
}

#[test]
fn term_while_taking_and_releasing_leaves_no_lock_or_token() {
    let dir = Scratch::new("run-lock-term-own");
    for i in 0..60 {
        let d = round_dir(dir.path(), i);
        let mut child = spawn_caller(&d, "own", "");
        wait_for(&d.join("ready"));
        std::thread::sleep(jitter(i));
        signal_group(&child, "TERM");
        let rc = finish(&mut child, Some("TERM"));
        let err = std::fs::read_to_string(d.join("err")).unwrap_or_default();
        assert_eq!(rc, 143, "회차 {i}: {err}");
        assert!(
            left(&d).is_empty(),
            "회차 {i}: 남은 파일 {:?} {err}",
            left(&d)
        );
        assert!(!err.contains("noclobber left on"), "회차 {i}: {err}");
    }
}

#[test]
fn term_while_another_run_holds_the_lock_leaves_that_lock_alone() {
    let dir = Scratch::new("run-lock-term-other");
    for i in 0..40 {
        let d = round_dir(dir.path(), i);
        std::fs::write(d.join("L"), "other\n").expect("남의 잠금");
        let mut child = spawn_caller(&d, "other", "");
        wait_for(&d.join("ready"));
        std::thread::sleep(jitter(i));
        signal_group(&child, "TERM");
        let rc = finish(&mut child, Some("TERM"));
        assert_eq!(rc, 143, "회차 {i}");
        assert_eq!(left(&d), vec!["L"], "회차 {i}");
        assert_eq!(
            std::fs::read_to_string(d.join("L")).expect("잠금"),
            "other\n"
        );
    }
}

#[test]
fn int_while_holding_and_int_during_cleanup_end_with_130_and_no_lock() {
    let dir = Scratch::new("run-lock-int");
    // 잡고 대기하는 중 INT 한 번.
    let d = round_dir(dir.path(), 0);
    let mut child = spawn_caller(&d, "hold", "");
    wait_for(&d.join("ready"));
    wait_for(&d.join("L"));
    signal_group(&child, "INT");
    assert_eq!(finish(&mut child, None), 130);
    assert!(left(&d).is_empty(), "남은 파일 {:?}", left(&d));
    // TERM으로 정리를 시작시킨 뒤 정리 중 INT 한 번. 정리를 마치고 130으로 끝난다.
    let d = round_dir(dir.path(), 1);
    let mut child = spawn_caller(&d, "hold", "0.3");
    wait_for(&d.join("ready"));
    wait_for(&d.join("L"));
    signal_group(&child, "TERM");
    wait_for(&d.join("cleaning"));
    signal_group(&child, "INT");
    assert_eq!(finish(&mut child, None), 130);
    assert!(left(&d).is_empty(), "남은 파일 {:?}", left(&d));
}

/// 다른 실행이 스테이징 잠금을 쥐고 있으면 stage-dev-plugins.sh가 기존 두 줄과 종료 1로 끝나고 번들 폴더를 만들지 않는다.
#[test]
fn staging_refuses_a_held_lock_with_the_existing_message() {
    let dir = Scratch::new("run-lock-stage");
    let profile = dir.path().join("debug");
    std::fs::create_dir(&profile).expect("프로필 폴더");
    let lock = profile.join("builtin-plugins.lock");
    std::fs::write(&lock, "other\n").expect("남의 잠금");
    let out = Command::new("bash")
        .arg(tasty_doc_guards::repo_root().join("scripts/stage-dev-plugins.sh"))
        .args(["--platform", "linux-x64"])
        .env("CARGO_TARGET_DIR", dir.path())
        .output()
        .expect("bash 실행");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    let bundle = profile.join("builtin-plugins");
    for line in [
        format!(
            "Error: another run is staging into {} ({} exists).",
            bundle.display(),
            lock.display()
        ),
        format!(
            "  Wait for it to finish and run this again. If no run is in progress, remove {}.",
            lock.display()
        ),
    ] {
        assert!(
            err.lines().any(|l| l == line),
            "안내 줄이 없다: {line}\n{err}"
        );
    }
    assert!(!bundle.exists(), "번들 폴더를 만들었다");
    assert_eq!(std::fs::read_to_string(&lock).expect("잠금"), "other\n");
    assert_eq!(
        names(&profile),
        vec!["builtin-plugins.lock"],
        "다른 파일을 남겼다"
    );
}
