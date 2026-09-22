//! `scripts/check-plugin-version-bump.sh` 의 판정을 **합성 git 저장소**로 양극성 고정한다.
//!
//! 왜 합성 저장소인가: 이 저장소 **자신의** 이력을 읽는 테스트는 배포 tarball(비-git),
//! shallow clone, 첫 커밋에서 판정 불가가 되고, 그 셋이 CI 에서 조용히 통과로 세어질
//! 위험이 크다. 여기서는 매번 저장소를 만들어 쓰므로 그 셋에 의존하지 않는다.
//!
//! **한 극성만 보면 항등식이다.** "bump 안 한 사본에서 FAIL" 만 보면 항상 FAIL 하는
//! 게이트도 통과하고, "bump 한 사본에서 PASS" 만 보면 아무것도 안 하는 게이트도
//! 통과한다. 그래서 최소 네 극성을 본다 — 부채(FAIL) · bump(PASS) · 포맷만(PASS) ·
//! 판정 불가(exit 2, 통과 아님).
//!
//! 판별식의 근거·측정·대안은 `docs/adr/0137-plugin-version-bump-is-judged-by-content-not-file-count.md`.
//!
//! **왜 `#![cfg(unix)]` 인가**: 이 테스트는 `bash` 로 셸 게이트를 직접 부른다. 셸이
//! 없는 플랫폼에서는 게이트의 판정이 아니라 셸의 부재가 결과를 정한다 — 그 자리에서
//! 나오는 빨강·초록은 어느 쪽도 게이트에 대해 아무것도 말하지 않는다. 같은 형태의
//! 형제 둘(`file_sloc_gate_fails_loudly` · `frozen_sum_ratchet_gate`)도 같은 근거로
//! 자기를 뺀다. 전제를 안 밝히면 통과 여부가 러너 환경(PATH 의 git-bash 유무)에
//! 달리고, 그 초록은 측정이 아니다.

//!
//! **Windows 에서 이 타깃은 빈다 — 커버리지 0 이고, 그 0 은 이제 미측정이 아니라 잰
//! 값이다.** `#![cfg(unix)]` 라 그렇고, 그 cfg 가 그 타깃에서 실제로 거짓인 것을 재서
//! 안다: `rustc --print cfg --target x86_64-pc-windows-gnu` 에 `unix` 선언이 **없다**
//! (호스트에는 있다). 컴파일은 거기서도 통과한다 — `cargo clippy -p tasty --all-targets
//! --locked --target x86_64-pc-windows-gnu` rc=0, 이 파일 진단 0 (실측 2026-09-08).
//! 즉 **Windows 잡의 초록은 이 게이트가 거기서 돈다는 뜻이 아니다.** 한때 이 자리에
//! "Windows 잡은 `--lib --bins` 라 안 본다" 고 적혀 있었는데, 그 잡은 `--all-targets`
//! 로 이 타깃을 **컴파일한다** — 안 보는 것은 잡이 아니라 `cfg` 다.

//!
//! ── 판정 불가는 **호출 지점 열여섯 곳**에서 나온다 ────────────────────────
//!
//! `CLAUDE.md` 는 "판정 불가(비-git · 없는 rev · rustfmt 부재)는 통과가 아니라 실패다"
//! 라고 적는다. 그 셋은 각각 아래 시험이 종료 코드로 물고, 실측(2026-09-08) 셋 다
//! 조용한 통과(`|| exit 0`)로 완화하면 그 시험이 죽인다 —
//! `a_non_git_directory_is_undecidable_not_a_pass` ·
//! `a_missing_rev_is_undecidable_not_a_pass` ·
//! `a_missing_rustfmt_is_undecidable_not_a_pass`.
//!
//! **그런데 게이트의 거절은 그 셋이 아니다.** 같은 날 `die` 호출 지점 **열여섯**에
//! 하나씩 완화 변이를 넣어 보면 죽는 것은 **넷**(위 셋 + 모드 인자 없음)이고 **열둘이
//! 살아남는다** — 저장소 루트 · 루트로 이동 · edition 읽기 · `--base` 의 rev ·
//! `--range` 의 두 rev · 알 수 없는 인자 · cargo tree · 의존 폐포 빔 · 작업 디렉토리 ·
//! 트리 펼치기 · 출하 판정기 · 인덱스 트리. 즉 저 문장은 **자기가 이름을 부른 것에
//! 대해서만** 참이고, 규칙처럼 읽히는 만큼은 아직 안 걸린다.
//!
//! 이름을 더 부르는 것은 답이 아니다 — 그것은 사람이 유지하는 표이고, 늘리는 비용이
//! 한 줄이며 리뷰에 안 뜬다. 형제 `tests/gates_pin_their_judge_absence.rs` 가 같은
//! 물음을 **기계가 세게** 한 선례다(소비자를 `scripts/` 에서 세고, 예외는 사유와 함께
//! 목록에 둔다). 여기도 그 형태가 필요하다.

#![cfg(unix)]

mod gate_env;

use std::fs;
use std::path::Path;
use std::process::Command;

fn script() -> String {
    format!(
        "{}/scripts/check-plugin-version-bump.sh",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} 실행 실패: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} 실패:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().expect("부모 경로")).expect("디렉토리 생성");
    fs::write(&p, body).unwrap_or_else(|e| panic!("{rel} 쓰기 실패: {e}"));
}

/// 스크립트를 돌리되 **출하 판정기를 안 보이게 하고** 돌린다.
///
/// 환경변수가 가리키는 것이 실행 가능하지 않으면 `resolve_judge` 는 기본 위치로 물러나지
/// 않는다(그렇게 물러나면 지목한 것과 다른 판정이 조용히 돈다). 그래서 이 한 줄이
/// "갓 클론한 트리" 를 재현한다 — 바깥 세션의 `target/debug` 에 무엇이 있든 무관하다.
fn check_without_the_stripper(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new("bash")
        .arg(script())
        .args(args)
        .current_dir(dir)
        .env("TASTY_STRIP_CFG_TEST_BIN", "/nonexistent/strip-cfg-test")
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    (run.code, run.output)
}

/// 스크립트를 돌리고 (exit code, stdout+stderr) 를 돌려준다.
fn check(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new("bash")
        .arg(script())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    (run.code, run.output)
}

const PLUGIN: &str = "crates/tasty-plugin-fixture";

/// 게이트 스크립트의 좌변을 넓힌 **사본**을 합성 트리에 놓고 그 경로를 준다.
///
/// "변수를 쓴다" 를 문자열로 확인하는 형태는 약하다 — 뿌리를 실제로 옮기고 **판정이
/// 따라오는가**로 잰다. 좌변의 소비처가 다섯이라(두 모드의 pathspec · 크레이트 뿌리를
/// 뽑는 sed · 매니페스트 명부 glob · 그 명부가 만드는 상대 경로) 하나만 옛 뿌리에
/// 남아 있어도 넓힌 트리의 plugin 이 안 보이고, 그러면 단언이 빨개진다.
fn widened_script(d: &Path, root: &str) -> String {
    let src = fs::read_to_string(script()).expect("게이트 스크립트를 읽을 수 있어야 한다");
    let from = "\nSCAN_ROOT=crates\n";
    assert_eq!(
        src.matches(from).count(),
        1,
        "좌변 선언이 한 자리가 아니다 — 이 시험이 무엇을 넓혔는지 알 수 없다"
    );
    // 스크립트는 공용 헬퍼를 **자기 위치 옆**에서 읽는다. 사본만 옮기면 그 줄이
    // 없는 파일을 가리켜, 좌변과 무관한 이유로 빨개진다. 그래서 헬퍼도 같이 옮긴다.
    // `scripts/` 자체에는 아무것도 안 만든다 — 그 디렉토리는 다른 가드의 모수다.
    let gate_dir = d.join(".gate");
    fs::create_dir_all(gate_dir.join("lib")).expect("사본 디렉토리 생성");
    let helper = format!("{}/scripts/lib/judge-bin.sh", env!("CARGO_MANIFEST_DIR"));
    fs::copy(&helper, gate_dir.join("lib/judge-bin.sh")).expect("공용 헬퍼 사본");
    let out = gate_dir.join("widened-gate.sh");
    fs::write(&out, src.replace(from, &format!("\nSCAN_ROOT={root}\n"))).expect("사본 쓰기");
    out.to_string_lossy().into_owned()
}

/// [`check`] 와 같되 **어느 스크립트를 돌릴지** 받는다.
fn check_with(gate: &str, dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new("bash")
        .arg(gate)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    (run.code, run.output)
}

/// 루트 Cargo.toml(스크립트가 edition 을 읽는다) + plugin 한 벌을 담은 저장소를 만들고
/// 첫 커밋까지 마친다.
fn seed_repo() -> tempfile::TempDir {
    seed_repo_under("crates")
}

/// [`seed_repo`] 와 같되 plugin 을 **어느 뿌리 아래에** 둘지 받는다.
fn seed_repo_under(root: &str) -> tempfile::TempDir {
    let plugin = format!("{root}/tasty-plugin-fixture");
    let tmp = tempfile::tempdir().expect("임시 디렉토리");
    let d = tmp.path();
    run_git(d, &["init", "--quiet"]);
    run_git(d, &["config", "user.email", "fixture@example.invalid"]);
    run_git(d, &["config", "user.name", "fixture"]);
    // 훅이 이 저장소에 끼어들면 판정이 아니라 훅을 재게 된다.
    run_git(d, &["config", "core.hooksPath", "/dev/null"]);

    write(
        d,
        "Cargo.toml",
        "[workspace]\n[package]\nedition = \"2024\"\n",
    );
    write(
        d,
        &format!("{plugin}/Cargo.toml"),
        "[package]\nname = \"tasty-plugin-fixture\"\nversion = \"0.1.0\"\n",
    );
    write(
        d,
        &format!("{plugin}/tasty-plugin.toml"),
        "id = \"com.tasty.fixture\"\nversion = \"0.1.0\"\n",
    );
    write(
        d,
        &format!("{plugin}/src/main.rs"),
        "fn main() {\n    let msg = \"a  b\";\n}\n",
    );
    run_git(d, &["add", "-A"]);
    run_git(d, &["commit", "--quiet", "-m", "seed"]);
    tmp
}

fn bump_to(d: &Path, v: &str) {
    write(
        d,
        &format!("{PLUGIN}/Cargo.toml"),
        &format!("[package]\nname = \"tasty-plugin-fixture\"\nversion = \"{v}\"\n"),
    );
    write(
        d,
        &format!("{PLUGIN}/tasty-plugin.toml"),
        &format!("id = \"com.tasty.fixture\"\nversion = \"{v}\"\n"),
    );
}

fn commit_all(d: &Path, msg: &str) {
    run_git(d, &["add", "-A"]);
    run_git(d, &["commit", "--quiet", "-m", msg]);
}

/// 좌변을 넓히면 **두 모드가 함께** 움직인다.
///
/// pathspec 이 `--staged` 와 `--range` 두 줄에 따로 적혀 있었다. 한 갈래만 재면
/// 나머지가 옛 뿌리에 남아 있어도 초록이라, 두 갈래를 같은 트리에서 본다.
///
/// **음성 대조가 먼저다** — 안 넓힌 원본이 이 트리를 정말 안 보는지 확인하지 않으면,
/// 아래 빨강이 "넓혀서 보인 것" 인지 "원래 보이던 것" 인지 못 가른다.
#[test]
fn widening_the_left_side_moves_both_modes() {
    let tmp = seed_repo_under("extra");
    let d = tmp.path();
    let changed = "extra/tasty-plugin-fixture/src/main.rs";
    write(d, changed, "fn main() {\n    let msg = \"changed\";\n}\n");
    commit_all(d, "feat(fixture): change behaviour");

    let (base_code, base_text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        base_code, 0,
        "안 넓힌 좌변이 `extra/` 를 보면 이 시험은 아무것도 안 재는 것이다:\n{base_text}"
    );
    assert!(
        base_text.contains("판정 대상 0 건"),
        "안 넓힌 좌변의 판정 대상은 0 이어야 한다:\n{base_text}"
    );

    let gate = widened_script(d, "extra");
    let (code, text) = check_with(&gate, d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 1, "--range 갈래가 넓힌 뿌리를 안 본다:\n{text}");
    assert!(
        text.contains("src/main.rs"),
        "어느 파일 때문인지 안 말한다:\n{text}"
    );

    write(
        d,
        changed,
        "fn main() {\n    let msg = \"changed twice\";\n}\n",
    );
    run_git(d, &["add", "-A"]);
    let (staged_code, staged_text) = check_with(&gate, d, &["--staged"]);
    assert_eq!(
        staged_code, 1,
        "--staged 갈래가 넓힌 뿌리를 안 본다 — pathspec 이 두 줄이라 한쪽만 따라올 수 있다:\n{staged_text}"
    );
}

#[test]
fn a_content_change_without_a_bump_is_rejected() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    commit_all(d, "feat(fixture): change behaviour");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 1, "부채인데 통과했다:\n{text}");
    assert!(
        text.contains("version 이 안 올랐다"),
        "위반 사유가 메시지에 없다:\n{text}"
    );
    assert!(
        text.contains("src/main.rs"),
        "어느 파일 때문인지 메시지에 없다:\n{text}"
    );
}

#[test]
fn the_same_change_with_a_bump_passes() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    bump_to(d, "0.1.1");
    commit_all(d, "feat(fixture): change behaviour + bump");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 0, "bump 했는데 막혔다:\n{text}");
    // **통과가 무동작과 구분되어야 한다** — 판정 대상이 0 이면 게이트가 죽어도 초록이다.
    assert!(
        text.contains("판정 대상 1 건"),
        "판정 대상 건수가 1 이 아니다(게이트가 이 변경을 아예 안 봤을 수 있다):\n{text}"
    );
}

#[test]
fn a_formatting_only_change_needs_no_bump() {
    let tmp = seed_repo();
    let d = tmp.path();
    // rustfmt 가 되돌릴 수 있는 차이만 준다. 문자열 리터럴 안의 두 칸 공백은 그대로 둔다 —
    // 공백을 통째로 제거하는 정규화였다면 여기서 리터럴 차이까지 삼켰을 자리다.
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn  main( )\n{\n        let  msg =   \"a  b\" ;\n}\n",
    );
    commit_all(d, "style(fixture): reformat");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 0, "포맷만 바뀌었는데 bump 를 요구했다:\n{text}");
    assert!(
        text.contains("판정 대상 0 건"),
        "포맷 변경이 판정 대상으로 세어졌다:\n{text}"
    );
    // 비영 대조: 변경 자체는 있었다. 0 건이 "아무것도 안 바뀌었다" 가 아님을 고정한다.
    assert!(
        text.contains("변경된 crates 파일 1 개"),
        "변경 파일 수가 안 찍힌다 — 0 건이 무변경인지 배제인지 구분되지 않는다:\n{text}"
    );
}

#[test]
fn a_literal_only_change_is_not_swallowed_by_normalization() {
    let tmp = seed_repo();
    let d = tmp.path();
    // 공백 제거 정규화였다면 `"a  b"` 와 `"ab"` 가 같아져 **거짓 음성**이 됐을 변경.
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"ab\";\n}\n",
    );
    commit_all(d, "fix(fixture): collapse the literal");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "문자열 리터럴 안의 공백 변화가 정규화에 삼켜졌다 — 거짓 음성이다:\n{text}"
    );
}

#[test]
fn a_docs_only_file_outside_the_build_output_needs_no_bump() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(d, &format!("{PLUGIN}/README.md"), "설명이 늘었다\n");
    commit_all(d, "docs(fixture): add a readme");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 0, "산출물 밖 파일에 bump 를 요구했다:\n{text}");
    assert!(text.contains("판정 대상 0 건"), "{text}");
}

#[test]
fn staged_mode_sees_the_index_before_a_commit_exists_for_it() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"staged\";\n}\n",
    );
    run_git(d, &["add", "-A"]);

    let (code, text) = check(d, &["--staged"]);
    assert_eq!(code, 1, "staged 부채를 못 봤다:\n{text}");

    bump_to(d, "0.1.1");
    run_git(d, &["add", "-A"]);
    let (code, text) = check(d, &["--staged"]);
    assert_eq!(code, 0, "staged 에서 bump 했는데 막혔다:\n{text}");
    assert!(text.contains("판정 대상 1 건"), "{text}");
}

#[test]
fn a_new_plugin_has_nothing_to_bump_from() {
    let tmp = seed_repo();
    let d = tmp.path();
    let other = "crates/tasty-plugin-newcomer";
    write(
        d,
        &format!("{other}/Cargo.toml"),
        "[package]\nname = \"tasty-plugin-newcomer\"\nversion = \"0.1.0\"\n",
    );
    write(
        d,
        &format!("{other}/tasty-plugin.toml"),
        "id = \"com.tasty.newcomer\"\nversion = \"0.1.0\"\n",
    );
    write(d, &format!("{other}/src/main.rs"), "fn main() {}\n");
    commit_all(d, "feat(newcomer): add a plugin");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 0,
        "새 plugin 의 최초 버전에 bump 를 요구했다:\n{text}"
    );
}

#[test]
fn a_version_that_goes_down_is_undecidable_not_a_pass() {
    // 값이 내려가는 항은 **위반이 아니라 판정 불가**다. 원인이 둘이고(범위가 뒤처졌다 /
    // 의도적으로 내렸다) 이 게이트는 어느 쪽인지 못 고른다. 그래서 1 이 아니라 2 다 —
    // 비영이라 train·pre-commit 은 그대로 막히지만, "네가 안 올렸다" 라고 말하지 않는다.
    // 번호만 보면 이 구분이 안 보이므로 **두 원인을 다 말하는가**까지 못박는다: 여기서
    // 지목을 틀리면 받는 쪽이 bump 로 풀고, 그건 남이 발행한 값을 덮는 짓이다.
    let tmp = seed_repo();
    let d = tmp.path();
    bump_to(d, "0.1.5");
    commit_all(d, "chore(fixture): move to 0.1.5");
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"down\";\n}\n",
    );
    bump_to(d, "0.1.4");
    commit_all(d, "feat(fixture): change with a lower version");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 2, "version 이 내려갔는데 판정 불가가 아니다:\n{text}");
    assert!(
        text.contains("앞 끝이 뒤 끝보다 새것"),
        "내려간 항을 지목하지 않았다:\n{text}"
    );
    assert!(
        text.contains("네가 뒤처졌다") && text.contains("의도적으로 내렸다"),
        "원인 둘 중 하나만 말한다 — 받는 쪽이 나머지 하나를 못 고른다:\n{text}"
    );
    assert!(
        !text.contains("version 이 안 올랐다"),
        "내려간 항을 '안 올렸다' 로 말했다 — bump 로 풀게 만든다:\n{text}"
    );
}

#[test]
fn a_version_change_alone_is_not_evidence_that_the_artifact_changed() {
    // 되돌림의 형태: 내용은 그대로 두고 값만 되돌린다. version 줄을 내용 증거로 세면
    // 그 줄 하나가 "달라졌다" 를 만들고 게이트는 되돌림에 또 한 번의 bump 를 요구한다 —
    // 올리면 되돌림이 아니다. 분할 착지에서 병합하는 쪽이 최종 값을 다시 정하는 흐름은
    // 규칙이 정상으로 규정한 것이라, 이 순환은 예외 상황이 아니다.
    let tmp = seed_repo();
    let d = tmp.path();
    bump_to(d, "0.1.5");
    commit_all(d, "chore(fixture): move to 0.1.5");
    bump_to(d, "0.1.4");
    commit_all(d, "revert(fixture): put the value back");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 0, "값만 되돌렸는데 또 올리라고 했다:\n{text}");
    assert!(
        text.contains("판정 대상 0 건"),
        "version 줄 자신이 판정 대상을 만들었다:\n{text}"
    );
}

#[test]
fn the_rest_of_the_manifest_is_still_content() {
    // 위 고침이 뺀 것은 **version 줄 한 줄**이다. `Cargo.toml` 을 통째로 뺐다면 feature·
    // 의존 변경이 산출물을 바꾸고도 조용히 통과한다 — 그 방향을 여기서 막는다.
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/Cargo.toml"),
        "[package]\nname = \"tasty-plugin-fixture\"\nversion = \"0.1.0\"\n\n\
         [features]\ndefault = [\"extra\"]\nextra = []\n",
    );
    commit_all(d, "feat(fixture): turn on a feature");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "version 줄 말고 다른 줄이 바뀌었는데 통과했다:\n{text}"
    );
    assert!(
        text.contains("Cargo.toml"),
        "어느 파일 때문인지 메시지에 없다:\n{text}"
    );
}

#[test]
fn a_non_git_directory_is_undecidable_not_a_pass() {
    let tmp = tempfile::tempdir().expect("임시 디렉토리");
    let (code, text) = check(tmp.path(), &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 2,
        "비-git 환경에서 판정 불가가 아니라 다른 코드를 냈다 — 0 이면 판정 불가를 \
         통과로 세는 것이다:\n{text}"
    );
    assert!(text.contains("판정 불가"), "{text}");
}

#[test]
fn a_missing_rev_is_undecidable_not_a_pass() {
    let tmp = seed_repo();
    let (code, text) = check(tmp.path(), &["--range", "deadbeefdeadbeef", "HEAD"]);
    assert_eq!(
        code, 2,
        "없는 rev(shallow clone 의 형태)에서 판정 불가가 아니다:\n{text}"
    );
    assert!(
        text.contains("shallow"),
        "shallow 단서가 메시지에 없다:\n{text}"
    );
}

#[test]
fn the_first_commit_has_no_baseline_and_says_so() {
    let tmp = tempfile::tempdir().expect("임시 디렉토리");
    let d = tmp.path();
    run_git(d, &["init", "--quiet"]);
    run_git(d, &["config", "user.email", "fixture@example.invalid"]);
    run_git(d, &["config", "user.name", "fixture"]);
    write(
        d,
        "Cargo.toml",
        "[workspace]\n[package]\nedition = \"2024\"\n",
    );
    write(
        d,
        &format!("{PLUGIN}/tasty-plugin.toml"),
        "id = \"com.tasty.fixture\"\nversion = \"0.1.0\"\n",
    );
    run_git(d, &["add", "-A"]);

    let (code, text) = check(d, &["--staged"]);
    assert_eq!(code, 0, "첫 커밋에서 막혔다:\n{text}");
    assert!(
        text.contains("첫 커밋"),
        "첫 커밋이라는 것이 출력에 안 나온다 — 조용한 통과와 구분되지 않는다:\n{text}"
    );
}

// ── 판정기의 신선도 ───────────────────────────────────────────────────
//
// 출하 범위를 좁히는 판정기(`strip-cfg-test`)는 **빌드 산출물**이다. 그래서 그것을 고친
// 사람과 판정을 돌리는 사람이 다르면 고침이 소스에 있는데 판정은 옛 규칙으로 돈다 —
// 그 오진은 조용하다(실측으로 밟았다). 없을 때와 같은 방향(좁히기를 끄고 넓게)으로
// 처리하되, **말은 한다.**
//
// 판정은 **판정기 자신에게 묻는다**(`--check-fresh`). mtime 으로 재던 판을 버린 이유는
// git 이 파일을 다시 쓰기만 해도 낡은 것으로 나왔기 때문이다 — 내용이 같아도 그렇고,
// 이 저장소의 표준 브랜치 왕복이 정확히 그것을 만든다. 아래 둘은 그 **계약**(종료코드로
// 답한다)을 고정한다; 지문 계산 자체는 `tasty_doc_guards::freshness` 의 단위 테스트가 본다.

/// `<repo>/target/debug/strip-cfg-test` 에 껍데기를 놓는다. `--check-fresh` 에 무엇으로
/// 답할지는 호출자가 정한다.
fn plant_judge(d: &Path, fresh: bool) {
    let bin = d.join("target/debug/strip-cfg-test");
    fs::create_dir_all(bin.parent().expect("부모")).expect("target 디렉토리");
    let body = if fresh {
        "#!/bin/sh\nexit 0\n"
    } else {
        "#!/bin/sh\n[ \"$1\" = \"--check-fresh\" ] && exit 1\nexit 0\n"
    };
    fs::write(&bin, body).expect("껍데기 판정기");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).expect("실행 권한");
    }
}

#[test]
fn a_judge_that_reports_itself_stale_is_announced_and_widens() {
    let tmp = seed_repo();
    let d = tmp.path();
    plant_judge(d, false);
    let (_, text) = check(d, &["--staged"]);
    assert!(
        text.contains("지금 소스로 지어진 것이 아니다"),
        "낡은 판정기가 조용히 쓰였다 — 옛 규칙으로 돈 판정을 아무도 못 본다:\n{text}"
    );
    assert!(
        text.contains("cargo build -p tasty-doc-guards"),
        "무엇을 하라는지가 없다:\n{text}"
    );
}

/// 반대 방향. 이것이 없으면 "항상 낡았다고 말하는" 검사도 위 테스트를 통과한다.
/// 그리고 이 갈래가 실제로 깨진 적이 있다 — mtime 판정이 내용이 같은 파일에도 켜졌다.
#[test]
fn a_judge_that_reports_itself_fresh_is_not_called_stale() {
    let tmp = seed_repo();
    let d = tmp.path();
    plant_judge(d, true);
    let (_, text) = check(d, &["--staged"]);
    assert!(
        !text.contains("지금 소스로 지어진 것이 아니다"),
        "신선한 판정기를 낡았다고 했다 — 매번 넓게 보면 좁히기가 죽은 것과 같다:\n{text}"
    );
}

// ── 산출물의 범위: plugin 디렉토리 밖 ─────────────────────────────────
//
// 번들 plugin 은 워크스페이스 크레이트를 링크한다. 그 크레이트가 바뀌면 plugin 바이너리가
// 달라지는데, 매니페스트가 없어 판정 대상에서 자연히 빠져 있었다. 아래 둘이 **양극성**이다 —
// 링크한 것은 요구하고 안 한 것은 안 요구한다. 한쪽만 보면 "전부 요구하는" 게이트도 통과한다.

/// 링크된 크레이트 하나와 그것을 쓰는 plugin, 그리고 **아무도 안 쓰는** 크레이트 하나를
/// 담은 진짜 cargo 워크스페이스. `cargo tree` 가 읽을 수 있어야 하므로 매니페스트가
/// 형식만 흉내 낸 것이면 안 된다.
fn seed_workspace() -> tempfile::TempDir {
    seed_workspace_under("crates")
}

/// [`seed_workspace`] 와 같되 세 크레이트를 **어느 뿌리 아래에** 둘지 받는다.
fn seed_workspace_under(root: &str) -> tempfile::TempDir {
    let plugin = format!("{root}/tasty-plugin-fixture");
    let tmp = tempfile::tempdir().expect("임시 디렉토리");
    let d = tmp.path();
    run_git(d, &["init", "--quiet"]);
    run_git(d, &["config", "user.email", "fixture@example.invalid"]);
    run_git(d, &["config", "user.name", "fixture"]);
    run_git(d, &["config", "core.hooksPath", "/dev/null"]);

    write(
        d,
        "Cargo.toml",
        &format!(
            "[workspace]\nresolver = \"2\"\n\
             members = [\"{root}/tasty-plugin-fixture\", \"{root}/tasty-shared\", \"{root}/tasty-lonely\"]\n\
             [workspace.package]\nedition = \"2024\"\n\
             [package]\nname = \"root-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\
             [lib]\npath = \"lib.rs\"\n"
        ),
    );
    write(d, "lib.rs", "pub fn nothing() {}\n");
    write(
        d,
        &format!("{plugin}/Cargo.toml"),
        "[package]\nname = \"tasty-plugin-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [dependencies]\ntasty-shared = { path = \"../tasty-shared\" }\n",
    );
    write(
        d,
        &format!("{plugin}/tasty-plugin.toml"),
        "id = \"com.tasty.fixture\"\nversion = \"0.1.0\"\n",
    );
    write(
        d,
        &format!("{plugin}/src/main.rs"),
        "fn main() {\n    tasty_shared::greet();\n}\n",
    );
    for c in ["tasty-shared", "tasty-lonely"] {
        write(
            d,
            &format!("{root}/{c}/Cargo.toml"),
            &format!("[package]\nname = \"{c}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        );
        write(
            d,
            &format!("{root}/{c}/src/lib.rs"),
            "pub fn greet() {\n    let _ = 1;\n}\n",
        );
    }
    run_git(d, &["add", "-A"]);
    run_git(d, &["commit", "--quiet", "-m", "seed"]);
    tmp
}

/// 좌변을 넓히면 **plugin 명부도** 따라 움직인다.
///
/// 위 [`widening_the_left_side_moves_both_modes`] 는 이 자리를 못 잰다 — 명부를 만드는
/// 두 줄(매니페스트 glob · 그것이 만드는 상대 경로)은 **공유 크레이트가 바뀐 갈래**
/// 에서만 돌기 때문이다. 실측: 그 둘만 옛 뿌리로 되돌려도 위 시험은 초록이었다.
#[test]
fn widening_the_left_side_moves_the_plugin_roster() {
    let tmp = seed_workspace_under("extra");
    let d = tmp.path();
    write(
        d,
        "extra/tasty-shared/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "fix(shared): change behaviour");

    let (base_code, base_text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        base_code, 0,
        "안 넓힌 좌변이 `extra/` 를 보면 이 시험은 아무것도 안 재는 것이다:\n{base_text}"
    );

    let gate = widened_script(d, "extra");
    let (code, text) = check_with(&gate, d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "넓힌 뿌리에서 명부가 plugin 을 못 찾았다 — 매니페스트 glob 이 옛 뿌리에 남았다:\n{text}"
    );
    assert!(
        text.contains("tasty-shared/src/lib.rs"),
        "어느 파일 때문인지 안 말한다:\n{text}"
    );
}

#[test]
fn a_change_in_a_linked_workspace_crate_demands_a_bump() {
    let tmp = seed_workspace();
    let d = tmp.path();
    write(
        d,
        "crates/tasty-shared/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "fix(shared): change behaviour");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "링크된 크레이트가 바뀌었는데 통과했다 — 이 자리가 여태 안 보이던 구멍이다:\n{text}"
    );
    assert!(
        text.contains("tasty-shared/src/lib.rs"),
        "어느 파일 때문인지 메시지에 없다:\n{text}"
    );
}

#[test]
fn a_change_in_an_unlinked_workspace_crate_does_not() {
    let tmp = seed_workspace();
    let d = tmp.path();
    write(
        d,
        "crates/tasty-lonely/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "fix(lonely): change behaviour");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 0,
        "아무 plugin 도 링크하지 않는 크레이트가 bump 를 요구했다:\n{text}"
    );
}

// ── 산출물의 범위: 워크스페이스 밖 path 의존 ─────────────────────────────
//
// `[patch]` 로 끼운 상류 사본(이 레포의 `vendor/tiny_http`)은 워크스페이스 `exclude` 에
// 있고 `crates/` 밖에 산다. 좌변이 `crates/` 뿐이면 그 사본만 고친 커밋이 판정 대상 0 건
// 으로 통과한다. 여기서도 양극성이다 — 링크된 사본은 요구하고, 아무도 안 링크하는 사본은
// 안 요구한다. 사본의 테스트 전용 변경이 출하 판정기를 거쳐 빠지는 갈래는 여기서 못 잰다
// — 합성 저장소에는 판정기 바이너리가 없다(위 "판정기의 신선도" 절). 그 갈래는 ADR 이
// 실저장소 변이로 잰 값을 적는다. 근거:
// docs/adr/0537-the-plugin-version-gate-follows-path-dependencies-outside-the-workspace.md

/// plugin 하나가 워크스페이스 밖 path 의존 `vendor/upstream` 을 링크하고, `vendor/orphan`
/// 은 아무도 안 링크하는 cargo 워크스페이스. 둘 다 `exclude` 에 있다 — 루트 아래 path
/// 의존은 exclude 하지 않으면 자동 멤버가 되기 때문이다(이 레포의 루트 Cargo.toml 과 같은 형태).
fn seed_vendored_workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("임시 디렉토리");
    let d = tmp.path();
    run_git(d, &["init", "--quiet"]);
    run_git(d, &["config", "user.email", "fixture@example.invalid"]);
    run_git(d, &["config", "user.name", "fixture"]);
    run_git(d, &["config", "core.hooksPath", "/dev/null"]);
    write(
        d,
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\n\
         members = [\"crates/tasty-plugin-fixture\"]\n\
         exclude = [\"vendor/upstream\", \"vendor/orphan\"]\n\
         [workspace.package]\nedition = \"2024\"\n\
         [package]\nname = \"root-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\
         [lib]\npath = \"lib.rs\"\n",
    );
    write(d, "lib.rs", "pub fn nothing() {}\n");
    write(
        d,
        &format!("{PLUGIN}/Cargo.toml"),
        "[package]\nname = \"tasty-plugin-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
         [dependencies]\nupstream = { path = \"../../vendor/upstream\" }\n",
    );
    write(
        d,
        &format!("{PLUGIN}/tasty-plugin.toml"),
        "id = \"com.tasty.fixture\"\nversion = \"0.1.0\"\n",
    );
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    upstream::greet();\n}\n",
    );
    for c in ["upstream", "orphan"] {
        write(
            d,
            &format!("vendor/{c}/Cargo.toml"),
            &format!("[package]\nname = \"{c}\"\nversion = \"0.1.0\"\nedition = \"2018\"\n"),
        );
        write(
            d,
            &format!("vendor/{c}/src/lib.rs"),
            "pub fn greet() {\n    let _ = 1;\n}\n\n#[cfg(test)]\nmod t {\n    #[test]\n    fn probe() {\n        assert_eq!(1, 1);\n    }\n}\n",
        );
    }
    run_git(d, &["add", "-A"]);
    run_git(d, &["commit", "--quiet", "-m", "seed"]);
    tmp
}

#[test]
fn a_change_in_a_linked_path_dependency_outside_the_workspace_demands_a_bump() {
    let tmp = seed_vendored_workspace();
    let d = tmp.path();
    write(
        d,
        "vendor/upstream/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n\n#[cfg(test)]\nmod t {\n    #[test]\n    fn probe() {\n        assert_eq!(1, 1);\n    }\n}\n",
    );
    run_git(d, &["add", "-A"]);

    let (code, text) = check(d, &["--staged"]);
    assert_eq!(
        code, 1,
        "링크된 워크스페이스 밖 사본이 바뀌었는데 통과했다 — `crates/` 만 보는 좌변의 구멍이다:\n{text}"
    );
    assert!(
        text.contains("vendor/upstream/src/lib.rs"),
        "어느 파일 때문인지 메시지에 없다:\n{text}"
    );

    run_git(
        d,
        &["commit", "--quiet", "-m", "fix(upstream): change behaviour"],
    );
    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 1, "`--range` 갈래가 사본을 못 본다:\n{text}");
}

#[test]
fn a_change_in_an_unlinked_path_dependency_outside_the_workspace_does_not() {
    let tmp = seed_vendored_workspace();
    let d = tmp.path();
    write(
        d,
        "vendor/orphan/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "fix(orphan): change behaviour");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 0,
        "아무 plugin 도 링크하지 않는 사본이 bump 를 요구했다:\n{text}"
    );
}

/// 워크스페이스 멤버 명부를 못 읽으면 **판정 불가**이고, 사유(cargo 의 stderr)가 남는다.
///
/// 명부가 비면 워크스페이스 밖 사본과 `crates/` 밖 멤버를 못 가르고, 스텁이 빈 출력으로
/// 성공을 흉내 내면 좌변이 조용히 달라진다. 그래서 실패를 통과로 흘리지 않는다.
#[test]
fn an_unreadable_member_roster_is_undecidable_and_says_why() {
    const MARK: &str = "zz-cargo-metadata-died-here";
    let tmp = seed_vendored_workspace();
    let d = tmp.path();
    write(
        d,
        "vendor/upstream/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "fix(upstream): change behaviour");

    let stub = tempfile::tempdir().expect("스텁 디렉토리");
    let cargo = stub.path().join("cargo");
    fs::write(
        &cargo,
        // `tree` 는 진짜 cargo 로 넘긴다 — 폐포가 비면 명부를 묻기 전에 다른 거절이 난다.
        format!(
            "#!/bin/sh\nif [ \"$1\" = metadata ]; then echo {MARK} >&2; exit 101; fi\nexec \"{real}\" \"$@\"\n",
            real = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
        ),
    )
    .expect("스텁 작성");
    let mut perm = fs::metadata(&cargo).expect("스텁 metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    fs::set_permissions(&cargo, perm).expect("실행권한");

    let out = Command::new("bash")
        .arg(script())
        .args(["--range", "HEAD^", "HEAD"])
        .current_dir(d)
        .env(
            "PATH",
            format!(
                "{}:{}",
                stub.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    assert_eq!(run, 2, "멤버 명부를 못 읽었는데 판정 불가가 아니다");
    assert!(
        run.output.contains("cargo metadata 가 실패했다"),
        "다른 거절이 대신 rc 2 를 냈다 — 명부 거절 자리의 문구가 없다:\n{}",
        run.output
    );
    assert!(
        run.output.contains(MARK),
        "판정 불가의 사유(cargo 의 stderr)가 출력에 없다:\n{}",
        run.output
    );
}

/// 의존 폐포를 못 읽으면 **판정 불가**이고, 그 실패의 사유(cargo 의 stderr)가 출력에 남는다.
///
/// 폐포 계산은 `links` 가 `$(...)` 안에서 부른다. 그 안의 `die` 는 **서브셸만** 끝내므로,
/// 그대로 두면 게이트는 빈 폐포를 "아무 plugin 도 링크하지 않는다" 로 읽고 계속 간다 —
/// 링크된 크레이트가 바뀌었는데 **rc 0 으로 통과한다.** 실패 문장은 stderr 에 한 줄 찍히지만
/// 판정은 rc 가 하므로 CI 는 초록이다. 게다가 그 서브셸이 빈 캐시 파일을 남겨, 다음 호출은
/// 비었는지 묻지도 않고 빈 폐포를 돌려준다.
///
/// 스텁 `cargo` 는 `tree` 에서만 죽는다 — 다른 이유로 죽은 것과 가르려고 표지를 stderr 에 찍는다.
#[test]
fn an_unreadable_dependency_closure_is_undecidable_and_says_why() {
    const MARK: &str = "zz-cargo-tree-died-here";
    let tmp = seed_workspace();
    let d = tmp.path();
    write(
        d,
        "crates/tasty-shared/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "fix(shared): change behaviour");

    let stub = tempfile::tempdir().expect("스텁 디렉토리");
    let cargo = stub.path().join("cargo");
    fs::write(
        &cargo,
        format!("#!/bin/sh\nif [ \"$1\" = tree ]; then echo {MARK} >&2; exit 101; fi\nexit 0\n"),
    )
    .expect("스텁 작성");
    let mut perm = fs::metadata(&cargo).expect("스텁 metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    fs::set_permissions(&cargo, perm).expect("실행권한");

    let out = Command::new("bash")
        .arg(script())
        .args(["--range", "HEAD^", "HEAD"])
        .current_dir(d)
        .env(
            "PATH",
            format!(
                "{}:{}",
                stub.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    assert_eq!(
        run, 2,
        "의존 폐포를 못 읽었는데 판정 불가가 아니다 — 0 이면 링크된 크레이트의 변경이 조용히 통과한 것이다"
    );
    assert!(
        run.output.contains("cargo tree 가 실패했다"),
        "다른 거절이 대신 rc 2 를 냈다 — 폐포 거절 자리의 문구가 없다:\n{}",
        run.output
    );
    assert!(
        run.output.contains(MARK),
        "판정 불가의 사유(cargo 의 stderr)가 출력에 없다:\n{}",
        run.output
    );
}

#[test]
fn a_missing_shipping_judge_widens_and_says_so() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    commit_all(d, "feat(fixture): change behaviour");

    // 합성 저장소에는 `target/` 이 없으므로 판정기가 없는 경로가 그대로 재현된다.
    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("출하 범위를 못 좁힌다"),
        "판정이 넓어진 것을 말하지 않는다 — 조용히 달라지면 다음 사람이 못 본다:\n{text}"
    );
}

#[test]
fn without_the_stripper_a_test_only_change_is_told_to_bump_and_that_is_deliberate() {
    // ★ 이 게이트만 판정기 부재에서 **판정 불가(2)로 안 나간다.** 형제 다섯은 2 로 나가고
    // `tests/gates_pin_their_judge_absence.rs` 가 그것을 센다 — 여기는 그 목록의 유일한
    // 예외이고, 예외인 것 자체를 값으로 박는 자리다.
    //
    // 왜 예외가 옳은가: 여섯 중 **pre-commit 이 부르는 것은 이것 하나뿐**이다. 갓 클론한
    // 트리에는 판정기가 정상적으로 없고, 거기서 2 로 죽으면 커밋이 막힌다. 넓게 본 결과는
    // 조용한 통과가 아니라 오탐(출하 밖 변경이 bump 를 요구)이고, 그 처방인 patch +1 은
    // 아무것도 헐겁게 만들지 않는다 — 다음 커밋이 덮는다. ADR-0137 의 비대칭이 그것이다.
    //
    // 그래서 **양쪽으로 못박는다.** 2 로 "일관성 있게" 고치면 훅이 갓 클론에서 막히고,
    // 0 으로 흘리면 출하 내용이 바뀐 커밋이 조용히 지나간다. 답은 정확히 1 이다.
    let tmp = seed_repo();
    let d = tmp.path();
    // 출하 밖 변경 하나만 얹는다. 판정기가 **있으면** 이 항은 0 건으로 걸러진다
    // (실측: 같은 모양을 이 저장소에 staged 로 얹으면 판정기 있음 rc=0 · 없음 rc=1).
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"a  b\";\n}\n\n#[cfg(test)]\nmod t {\n    #[test]\n    fn probe() {}\n}\n",
    );
    commit_all(d, "test(fixture): add a test-only block");

    let (code, text) = check_without_the_stripper(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "판정기 없이 돈 이 게이트는 **넓게 보고 bump 를 요구**해야 한다. 2 면 갓 클론한 \
         트리의 pre-commit 이 막히고, 0 이면 출하 변경이 조용히 지나간다:\n{text}"
    );
    assert!(
        text.contains("출하 범위를 못 좁힌다"),
        "격하됐다는 사실을 안 찍었다 — 그러면 이 오탐이 진짜 부채와 구분이 안 된다:\n{text}"
    );
}

/// ★ 출력에만 실리는 값 축 — **판정 대상 수는 rc 에 안 들어간다.**
///
/// 이 게이트의 rc 는 `VIOLATIONS` 와 `BEHIND` 가 정한다. 통과줄과 실패줄이 함께 찍는
/// `판정 대상 N 건` 은 판정에 안 들어가고, 실측(2026-09-08) 그 문구를 바꾸는 변이에서
/// 위 열아홉이 전부 통과했다.
///
/// 그 수는 장식이 아니다. 게이트 본문이 그것으로 **범위 오류**를 가른다 — 대상이 둘
/// 이상인데 전량이 걸리면 "한 lane 이 만들 수 있는 모양이 아니니 범위를 의심해라" 로
/// 나간다. 그 판정의 좌변이 이 수이고, 이 수가 실제 판정 수를 안 따라가면 그 경고가
/// 엉뚱한 때 뜨거나 안 뜬다.
///
/// **plugin 을 둘 두고 하나만 건드린다.** 하나로 재면 1 을 상수로 박은 상태도 통과하고,
/// 둘 다 건드리면 `판정 대상` 과 `변경된 crates 파일` 이 같은 값이 되어 두 수가 한
/// 수로 붕괴한 상태도 통과한다. 서로 다른 값이 나오는 배치라야 두 자리가 갈린다.
#[test]
fn the_reported_considered_count_follows_the_judged_plugins() {
    const OTHER: &str = "crates/tasty-plugin-second";
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{OTHER}/Cargo.toml"),
        "[package]\nname = \"tasty-plugin-second\"\nversion = \"0.1.0\"\n",
    );
    write(
        d,
        &format!("{OTHER}/tasty-plugin.toml"),
        "id = \"com.tasty.second\"\nversion = \"0.1.0\"\n",
    );
    write(d, &format!("{OTHER}/src/main.rs"), "fn main() {}\n");
    commit_all(d, "두 번째 plugin");

    // 하나만 내용이 바뀌고 값이 따라온다. 다른 하나는 그대로라 판정 대상이 아니다.
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    bump_to(d, "0.1.1");
    commit_all(d, "첫 plugin 만 고치고 올린다");

    let (code, text) = check(d, &["--range", "HEAD~1", "HEAD"]);
    assert_eq!(code, 0, "값이 따라왔는데 통과가 아니다:\n{text}");
    assert!(
        text.contains("판정 대상 1 건"),
        "판정 대상 수가 실제로 판정한 plugin 수를 안 따라간다 — 이 수가 범위 오류를 \
         가르는 좌변인데 그것이 맞는지는 rc 에 안 나타난다:\n{text}"
    );
    assert!(
        text.contains("crates 파일 3 개 중"),
        "변경된 파일 수가 판정 대상 수와 한 수로 붕괴했다 — 둘은 서로 다른 물음이다:\n{text}"
    );
}

// ── 문서가 참이라고 적어 둔 것과 코드가 그런 것은 다르다 ────────────────────
//
// 루트 CLAUDE.md 는 이 게이트에 대해 "판정 불가(비-git · 없는 rev · rustfmt 부재)는
// 통과가 아니라 실패다" 라고 적어 뒀다. **셋 중 둘만 박혀 있었다.**
// 실측 2026-09-08: `rustfmt` 부재 갈래의 `die` 를 `{ echo …; exit 0; }` 로 바꾸고
// 이 타깃과 `gates_pin_their_judge_absence` 를 돌리면 **한 건도 안 죽는다**. 인자 없음
// 갈래도 같다. `die()` **정의**를 바꾸면 둘이 죽지만(비-git · 없는 rev), 그것은 호출
// 자리 하나가 `die` 를 안 부르게 바뀌는 형태를 못 본다 — 그리고 그것이 실제로 일어나는
// 형태다(조건 하나를 완화하는 편집).

#[test]
fn a_missing_rustfmt_is_undecidable_not_a_pass() {
    let tmp = seed_repo();
    // `git` 은 보이고 `rustfmt` 만 안 보이는 PATH. 게이트는 저장소 판정까지 가고
    // 정규화기에서 멈춘다 — rc 만 보면 "다른 이유로 죽었다" 와 안 갈리므로 메시지도 본다.
    let path = gate_env::only(&["bash", "git"]);
    let out = Command::new("bash")
        .arg(script())
        .args(["--staged"])
        .current_dir(tmp.path())
        .env("PATH", path.path())
        .output()
        .expect("게이트 스크립트 실행");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        out.status.code(),
        Some(2),
        "rustfmt 가 없으면 포맷 변경과 실변경을 가를 수 없다 — 그 상태의 통과는 \
         '안 재고 통과' 다:\n{text}"
    );
    assert!(text.contains("rustfmt 가 없다"), "{text}");
}

#[test]
fn no_mode_argument_is_undecidable_not_a_pass() {
    let tmp = seed_repo();
    let (code, text) = check(tmp.path(), &[]);
    assert_eq!(
        code, 2,
        "무엇을 판정할지 안 정해졌는데 통과로 나가면, 인자를 빠뜨린 호출이 전부 \
         초록이 된다:\n{text}"
    );
    assert!(text.contains("--staged 또는 --range"), "{text}");
}
