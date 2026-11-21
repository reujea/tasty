//! 합성 Git 저장소에서 플러그인 버전 검사의 통과·위반·측정 실패를 확인한다.
//! 실제 작업 저장소의 이력이나 shallow 상태에 의존하지 않는다. Bash를 사용하는 Unix 전용 시험이다.

//! Windows에서는 cfg에 따라 이 시험이 제외되므로 컴파일 성공을 실행 통과로 보지 않는다.

//! 서로 다른 오류가 같은 종료 코드를 낼 수 있다. 이 시험들이 스크립트의 모든 실패 분기를 독립적으로 검증하는 것은 아니다.

#![cfg(unix)]

mod gate_env;
mod plugin_version_bump_locked;

use std::fs;
use std::path::Path;

use tasty_doc_guards::git_env;

fn script() -> String {
    format!(
        "{}/scripts/check-plugin-version-bump.sh",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = git_env::command("git")
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

/// 기본 경로로 대체되지 않는 명시적 부재 경로를 주어 test 전용 코드 제외 도구가 없는 상태를 재현한다.
fn check_without_the_stripper(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = git_env::command("bash")
        .arg(script())
        .args(args)
        .current_dir(dir)
        .env("TASTY_STRIP_CFG_TEST_BIN", "/nonexistent/strip-cfg-test")
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    (run.code, run.output)
}

fn check(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = git_env::command("bash")
        .arg(script())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    (run.code, run.output)
}

const PLUGIN: &str = "crates/tasty-plugin-fixture";

/// 수집 루트를 바꾼 스크립트 사본으로 경로를 사용하는 각 처리 단계가 같은 범위를 보는지 확인한다.
fn widened_script(d: &Path, root: &str) -> String {
    let src = fs::read_to_string(script()).expect("게이트 스크립트를 읽을 수 있어야 한다");
    let from = "\nSCAN_ROOT=crates\n";
    assert_eq!(
        src.matches(from).count(),
        1,
        "변경할 수집 루트 선언이 정확히 한 곳이어야 한다"
    );
    // 스크립트가 상대 경로로 읽는 공용 헬퍼도 함께 복사한다.
    let gate_dir = d.join(".gate");
    fs::create_dir_all(gate_dir.join("lib")).expect("사본 디렉토리 생성");
    for helper in ["judge-bin.sh", "toml-scan.awk"] {
        let src = format!("{}/scripts/lib/{helper}", env!("CARGO_MANIFEST_DIR"));
        fs::copy(&src, gate_dir.join("lib").join(helper)).expect("공용 헬퍼 사본");
    }
    let out = gate_dir.join("widened-gate.sh");
    fs::write(&out, src.replace(from, &format!("\nSCAN_ROOT={root}\n"))).expect("사본 쓰기");
    out.to_string_lossy().into_owned()
}

fn check_with(gate: &str, dir: &Path, args: &[&str]) -> (i32, String) {
    let out = git_env::command("bash")
        .arg(gate)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    (run.code, run.output)
}

fn seed_repo() -> tempfile::TempDir {
    seed_repo_under("crates")
}

fn seed_repo_under(root: &str) -> tempfile::TempDir {
    let plugin = format!("{root}/tasty-plugin-fixture");
    let tmp = tempfile::tempdir().expect("임시 디렉토리");
    let d = tmp.path();
    run_git(d, &["init", "--quiet"]);
    run_git(d, &["config", "user.email", "fixture@example.invalid"]);
    run_git(d, &["config", "user.name", "fixture"]);
    // 사용자 Git 훅이 합성 저장소의 시험에 개입하지 않도록 한다.
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

/// 원래 범위에서는 보이지 않던 변경이 확장 뒤 staged와 range 두 모드에서 모두 검출되는지 비교한다.
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
        "범위를 늘리기 전에도 extra를 검사해 확장 효과를 비교할 수 없다:\n{base_text}"
    );
    assert!(
        base_text.contains("판정 대상 0 건"),
        "범위를 늘리기 전 판정 대상은 0 이어야 한다:\n{base_text}"
    );

    let gate = widened_script(d, "extra");
    let (code, text) = check_with(&gate, d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "--range 모드가 추가한 디렉터리를 검사하지 않았다:\n{text}"
    );
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
        "--staged 모드가 추가한 디렉터리를 검사하지 않았다:\n{staged_text}"
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
    assert_eq!(code, 1, "버전 증가가 필요한 변경인데 통과했다:\n{text}");
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
    assert!(
        text.contains("통과"),
        "대상이 있는 통과를 통과로 표기하지 않았다:\n{text}"
    );
    assert!(
        text.contains("판정 대상 1 건"),
        "판정 대상 건수가 1 이 아니다(게이트가 이 변경을 아예 안 봤을 수 있다):\n{text}"
    );
}

#[test]
fn a_formatting_only_change_needs_no_bump() {
    let tmp = seed_repo();
    let d = tmp.path();
    // 포맷 차이만 주고 문자열 안 공백은 유지한다.
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
    // 변경 파일도 확인해 판정 대상 0이 무변경 때문인 경우와 구별한다.
    assert!(
        text.contains("변경된 crates 파일 1 개"),
        "변경 파일 수가 안 찍힌다 — 0 건이 무변경인지 배제인지 구분되지 않는다:\n{text}"
    );
}

#[test]
fn a_literal_only_change_is_not_swallowed_by_normalization() {
    let tmp = seed_repo();
    let d = tmp.path();
    // 문자열 안 공백 변경은 포맷 정규화로 제거되면 안 된다.
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

/// 판정 대상이 없으면 rc 는 0 이지만 아무것도 검사하지 않았으므로 통과로 표기하지 않는다.
/// 대상이 있는 통과와 출력으로 구별돼야 한다.
#[test]
fn zero_judged_targets_read_as_none_not_as_a_pass() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(d, &format!("{PLUGIN}/README.md"), "설명이 늘었다\n");
    commit_all(d, "docs(fixture): add a readme");

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 0, "대상 없는 커밋은 정상이다:\n{text}");
    assert!(text.contains("판정 대상 없음"), "{text}");
    assert!(
        !text.contains("통과"),
        "판정 대상 0 건을 통과로 표기했다:\n{text}"
    );
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
    assert_eq!(
        code, 1,
        "staged 변경의 버전 증가 누락을 검출하지 못했다:\n{text}"
    );

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
    // 버전 감소는 잘못된 비교 범위와 의도한 되돌림을 구별할 수 없어 측정 실패로 다룬다. 두 가능성을 안내하는지도 확인한다.
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
    // 버전만 되돌린 변경을 새 내용 변경으로 세면 다시 버전을 올리라는 순환이 생긴다.
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
    // 버전 줄 제외가 매니페스트 전체 제외로 넓어지지 않도록 feature 변경도 검사한다.
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

fn rev_parse(dir: &Path, rev: &str) -> String {
    let out = git_env::command("git")
        .args(["rev-parse", "--verify", rev])
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git rev-parse {rev} 실행 실패: {e}"));
    assert!(out.status.success(), "rev 를 못 찾았다: {rev}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

// staged 모드는 --base와 index를 비교한다. pre-commit이 develop 공통 조상을 넘기므로
// base 뒤에 이미 커밋된 변경도 이 커밋의 판정에 들어와야 한다.

#[test]
fn a_base_that_is_not_a_commit_is_undecidable() {
    let tmp = seed_repo();
    let (code, text) = check(tmp.path(), &["--staged", "--base", "nosuchrev"]);
    assert_eq!(
        code, 2,
        "없는 --base 를 첫 커밋으로 보고 통과시켰다:\n{text}"
    );
    assert!(
        text.contains("--base rev 를 못 찾았다"),
        "같은 rc 2를 내는 다른 거절과 구별되는 --base 문구가 출력에 없다:\n{text}"
    );
    assert!(
        text.contains("nosuchrev"),
        "어느 rev 를 못 찾았는지 출력에 없다:\n{text}"
    );
}

#[test]
fn staged_mode_sees_commits_between_the_base_and_head() {
    let tmp = seed_repo();
    let d = tmp.path();
    let seed = rev_parse(d, "HEAD");
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"b\";\n}\n",
    );
    commit_all(d, "unbumped change");
    write(d, "README.txt", "x\n");
    run_git(d, &["add", "README.txt"]);

    let (code, text) = check(d, &["--staged", "--base", &seed]);
    assert_eq!(code, 1, "base 뒤의 커밋된 변경을 놓쳤다:\n{text}");
    assert!(
        text.contains("src/main.rs"),
        "어느 파일 때문인지 출력에 없다:\n{text}"
    );
}

#[test]
fn staged_mode_passes_when_the_commits_after_the_base_carry_their_bump() {
    let tmp = seed_repo();
    let d = tmp.path();
    let seed = rev_parse(d, "HEAD");
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"b\";\n}\n",
    );
    bump_to(d, "0.1.1");
    commit_all(d, "bumped change");
    write(d, "README.txt", "x\n");
    run_git(d, &["add", "README.txt"]);

    let (code, text) = check(d, &["--staged", "--base", &seed]);
    assert_eq!(code, 0, "base 뒤 커밋이 버전을 올렸는데 막혔다:\n{text}");
    assert!(text.contains("판정 대상 1 건"), "{text}");
}

#[test]
fn staged_mode_passes_when_only_the_bump_is_staged_after_an_unbumped_commit() {
    let tmp = seed_repo();
    let d = tmp.path();
    let seed = rev_parse(d, "HEAD");
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"b\";\n}\n",
    );
    commit_all(d, "unbumped change");
    bump_to(d, "0.1.1");
    run_git(d, &["add", "-A"]);

    let (code, text) = check(d, &["--staged", "--base", &seed]);
    assert_eq!(code, 0, "index의 버전 증가를 보지 못했다:\n{text}");
    assert!(text.contains("판정 대상 1 건"), "{text}");
}

// 판정 도구가 낡았는지는 --check-fresh 결과로 판단한다. 낡으면 test 코드 제외를 생략하고 그 사실을 알린다.
// 여기서는 종료 코드 계약을 검사하며 지문 계산은 공용 freshness 시험에서 확인한다.

fn plant_judge(d: &Path, fresh: bool) {
    let bin = d.join("target/debug/strip-cfg-test");
    fs::create_dir_all(bin.parent().expect("부모")).expect("target 디렉토리");
    let body = if fresh {
        "#!/bin/sh\nexit 0\n"
    } else {
        "#!/bin/sh\n[ \"$1\" = \"--check-fresh\" ] && exit 1\nexit 0\n"
    };
    tasty_test_support::write_executable(&bin, body).expect("껍데기 판정기");
}

#[test]
fn a_judge_that_reports_itself_stale_is_announced_and_widens() {
    let tmp = seed_repo();
    let d = tmp.path();
    plant_judge(d, false);
    let (_, text) = check(d, &["--staged"]);
    assert!(
        text.contains("지금 소스로 지어진 것이 아니다"),
        "현재 소스로 빌드되지 않은 도구를 안내 없이 사용했다:\n{text}"
    );
    assert!(
        text.contains("cargo build -p tasty-doc-guards"),
        "무엇을 하라는지가 없다:\n{text}"
    );
}

#[test]
fn a_judge_that_reports_itself_fresh_is_not_called_stale() {
    let tmp = seed_repo();
    let d = tmp.path();
    plant_judge(d, true);
    let (_, text) = check(d, &["--staged"]);
    assert!(
        !text.contains("지금 소스로 지어진 것이 아니다"),
        "현재 소스로 빌드된 도구를 낡았다고 판정했다:\n{text}"
    );
}

/// cargo tree가 읽을 수 있는 실제 매니페스트로 의존하는 크레이트와 의존하지 않는 크레이트를 함께 만든다.
fn seed_workspace() -> tempfile::TempDir {
    seed_workspace_under("crates")
}

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

/// 공유 크레이트 변경에서만 실행되는 플러그인 목록 수집도 확장한 루트를 따르는지 확인한다.
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
        "범위를 늘리기 전에도 extra를 검사해 확장 효과를 비교할 수 없다:\n{base_text}"
    );

    let gate = widened_script(d, "extra");
    let (code, text) = check_with(&gate, d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "확장한 루트의 플러그인이 목록에 포함되지 않았다. 매니페스트 수집 경로를 확인한다:\n{text}"
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
        "링크한 크레이트가 바뀌었지만 버전 증가를 요구하지 않았다:\n{text}"
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

// 워크스페이스 밖 path 의존도 대상이다. 이 합성 시험에는 실제 판정 도구가 없어 그 의존의 test 전용 변경 제외까지 검증하지는 않는다.

/// 루트 아래 path 의존이 자동 멤버가 되지 않도록 exclude에 등록한다.
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
        "워크스페이스 밖 path 의존이 바뀌었지만 버전 증가를 요구하지 않았다:\n{text}"
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
    assert_eq!(
        code, 1,
        "--range 모드에서 외부 의존의 변경을 찾지 못했다:\n{text}"
    );
}

#[test]
fn staged_mode_sees_an_outside_path_dependency_committed_after_the_base() {
    let tmp = seed_vendored_workspace();
    let d = tmp.path();
    let seed = rev_parse(d, "HEAD");
    write(
        d,
        "vendor/upstream/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "fix(upstream): change behaviour");
    write(d, "README.txt", "x\n");
    run_git(d, &["add", "README.txt"]);

    let (code, text) = check(d, &["--staged", "--base", &seed]);
    assert_eq!(
        code, 1,
        "base 뒤에 커밋된 워크스페이스 밖 path 의존 변경을 놓쳤다:\n{text}"
    );
    assert!(
        text.contains("vendor/upstream/src/lib.rs"),
        "어느 파일 때문인지 메시지에 없다:\n{text}"
    );
}

/// 어느 시험 호스트에서도 참이 아닌 target 전용 path 의존을 둔다. 결과가 호스트 OS와 무관하다.
fn seed_target_only_workspace(extra_dep: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("임시 디렉토리");
    let d = tmp.path();
    run_git(d, &["init", "--quiet"]);
    run_git(d, &["config", "user.email", "fixture@example.invalid"]);
    run_git(d, &["config", "user.name", "fixture"]);
    run_git(d, &["config", "core.hooksPath", "/dev/null"]);
    write(
        d,
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\"]\n\
         [workspace.package]\nedition = \"2024\"\n",
    );
    write(
        d,
        "crates/plug/Cargo.toml",
        &format!(
            "[package]\nname = \"plug\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
             [target.'cfg(target_os = \"haiku\")'.dependencies]\n\
             shared = {{ path = \"../shared\" }}\n{extra_dep}"
        ),
    );
    write(
        d,
        "crates/plug/tasty-plugin.toml",
        "id = \"com.tasty.plug\"\nversion = \"0.1.0\"\n",
    );
    write(d, "crates/plug/src/main.rs", "fn main() {}\n");
    write(
        d,
        "crates/shared/Cargo.toml",
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write(
        d,
        "crates/shared/src/lib.rs",
        "pub fn greet() {\n    let _ = 1;\n}\n",
    );
    run_git(d, &["add", "-A"]);
    run_git(d, &["commit", "--quiet", "-m", "seed"]);
    write(
        d,
        "crates/shared/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "fix(shared): change behaviour");
    tmp
}

#[test]
fn a_change_in_a_dependency_for_another_target_demands_a_bump() {
    let tmp = seed_target_only_workspace("");
    let (code, text) = check(tmp.path(), &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "다른 target 전용 의존의 변경을 폐포에서 놓쳤다:\n{text}"
    );
    assert!(
        text.contains("crates/shared/src/lib.rs"),
        "어느 파일 때문인지 메시지에 없다:\n{text}"
    );
}

#[test]
fn an_unfetched_dependency_for_another_target_is_undecidable_and_says_to_fetch() {
    let tmp = seed_target_only_workspace("itoa = \"1\"\n");
    let home = tempfile::tempdir().expect("빈 CARGO_HOME");
    let out = git_env::command("bash")
        .arg(script())
        .args(["--range", "HEAD^", "HEAD"])
        .current_dir(tmp.path())
        .env("CARGO_HOME", home.path())
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    assert_eq!(
        run.code, 2,
        "받지 않은 의존이 있는데 판정 불가가 아니다:\n{}",
        run.output
    );
    assert!(
        run.output.contains("cargo fetch --locked"),
        "cargo fetch 안내가 없다:\n{}",
        run.output
    );
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
    tasty_test_support::write_executable(
        &cargo,
        // tree는 성공시켜 metadata 실패 전에 다른 오류가 나지 않게 한다.
        format!(
            "#!/bin/sh\nif [ \"$1\" = metadata ]; then echo {MARK} >&2; exit 101; fi\nexec \"{real}\" \"$@\"\n",
            real = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
        ),
    )
    .expect("스텁 작성");

    let out = git_env::command("bash")
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
        "cargo metadata 실패를 구별하는 진단이 없다:\n{}",
        run.output
    );
    assert!(
        run.output.contains(MARK),
        "판정 불가의 사유(cargo 의 stderr)가 출력에 없다:\n{}",
        run.output
    );
}

/// 의존 계산은 서브셸에서 실행된다. 그 실패가 바깥 스크립트의 실패로도 전달되는지 확인한다.
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
    tasty_test_support::write_executable(
        &cargo,
        format!("#!/bin/sh\nif [ \"$1\" = tree ]; then echo {MARK} >&2; exit 101; fi\nexit 0\n"),
    )
    .expect("스텁 작성");

    let out = git_env::command("bash")
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
        "cargo tree 실패를 구별하는 진단이 없다:\n{}",
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

    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("출하 범위를 못 좁힌다"),
        "판정이 넓어진 것을 말하지 않는다 — 조용히 달라지면 다음 사람이 못 본다:\n{text}"
    );
}

#[test]
fn without_the_stripper_a_test_only_change_is_told_to_bump_and_that_is_deliberate() {
    // pre-commit은 제외 도구가 없는 새 저장소에서도 실행된다. 이 경우 원문을 넓게 검사해 test 변경에도 버전 증가를 요구한다.
    // 도구 부재를 무조건 성공이나 측정 실패로 바꾸지 않는 현재 예외를 확인한다.
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"a  b\";\n}\n\n#[cfg(test)]\nmod t {\n    #[test]\n    fn probe() {}\n}\n",
    );
    commit_all(d, "test(fixture): add a test-only block");

    let (code, text) = check_without_the_stripper(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "제외 도구가 없으면 원문을 검사해 이 test 변경에도 버전 증가를 요구해야 한다:\n{text}"
    );
    assert!(
        text.contains("출하 범위를 못 좁힌다"),
        "제외 도구를 사용하지 않았다는 안내가 없다:\n{text}"
    );
}

/// 판정 대상 수는 종료 코드와 별개로 출력도 확인한다. 두 플러그인 중 하나만 바꿔 전체 플러그인 수·변경 파일 수와 구별한다.
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
        "출력한 판정 대상 수가 실제 검사한 플러그인 수와 다르다:\n{text}"
    );
    assert!(
        text.contains("crates 파일 3 개 중"),
        "변경 파일 수가 플러그인 판정 대상 수와 구별되지 않는다:\n{text}"
    );
}

#[test]
fn a_missing_rustfmt_is_undecidable_not_a_pass() {
    let tmp = seed_repo();
    // git은 남기고 rustfmt만 PATH에서 제외한다. 다른 오류와 구별할 진단도 확인한다.
    let path = gate_env::only(&["bash", "git"]);
    let out = git_env::command("bash")
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
        "rustfmt가 없는데 측정 실패로 종료하지 않았다:\n{text}"
    );
    assert!(text.contains("rustfmt 가 없다"), "{text}");
}

#[test]
fn no_mode_argument_is_undecidable_not_a_pass() {
    let tmp = seed_repo();
    let (code, text) = check(tmp.path(), &[]);
    assert_eq!(
        code, 2,
        "모드를 지정하지 않았는데 측정 실패로 종료하지 않았다:\n{text}"
    );
    assert!(text.contains("--staged 또는 --range"), "{text}");
}

const FIXTURE_LOCK: &str =
    "schema = 1\n\n[[plugin]]\nid = \"com.tasty.fixture\"\nversion = \"9.9.9\"\n";

/// 잠금 파일을 둔 저장소에서 플러그인 소스를 바꾼 커밋을 만든다. 판정이 잠금 파일·매니페스트 판독까지 간다.
fn locked_change_repo(lock: &str, manifest: Option<&str>) -> tempfile::TempDir {
    let tmp = seed_repo();
    let d = tmp.path();
    if let Some(m) = manifest {
        write(d, &format!("{PLUGIN}/tasty-plugin.toml"), m);
    }
    write(d, "official-plugins.lock.toml", lock);
    commit_all(d, "seed lock");
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    commit_all(d, "change");
    tmp
}

fn assert_refused(what: &str, (code, text): (i32, String), words: &str) {
    assert_eq!(code, 2, "{what}: 판정 불가가 아니다:\n{text}");
    assert!(
        text.contains(words),
        "{what}: 거절 사유 `{words}` 가 출력에 없다:\n{text}"
    );
}

/// 잠금 파일과 매니페스트를 읽지 못한 사유는 서로 다른 문구로 나온다. 같은 종료 코드만 보면 다른 실패가 대신 통과한다.
#[test]
fn each_lock_and_manifest_read_failure_names_its_own_cause() {
    let unclosed_lock = format!("{FIXTURE_LOCK}note = \"\"\"\n열림\n");
    let cases: [(&str, &str, Option<&str>, &str); 4] = [
        (
            "닫히지 않은 잠금 파일",
            &unclosed_lock,
            None,
            "잠금 파일을 TOML 로 읽지 못했다",
        ),
        (
            "id 가 둘인 잠금 표",
            "[[plugin]]\nid = \"com.tasty.fixture\"\nid = \"com.tasty.other\"\n",
            None,
            "잠금 파일의 플러그인 id 를 받을 수 없다",
        ),
        (
            "닫히지 않은 매니페스트",
            FIXTURE_LOCK,
            Some("id = \"com.tasty.fixture\"\nnote = \"\"\"\n열림\n"),
            "매니페스트를 TOML 로 읽지 못했다",
        ),
        (
            "id 가 이스케이프인 매니페스트",
            FIXTURE_LOCK,
            Some("id = \"com.tasty.\\u0066ixture\"\n"),
            "매니페스트 id 를 받을 수 없다",
        ),
    ];
    for (what, lock, manifest, words) in cases {
        let tmp = locked_change_repo(lock, manifest);
        assert_refused(
            what,
            check(tmp.path(), &["--range", "HEAD^", "HEAD"]),
            words,
        );
    }
}

/// 스크립트 옆의 TOML 판독기가 없으면 잠금 파일을 읽지 않고 판정 불가로 멈춘다.
#[test]
fn a_missing_toml_reader_is_undecidable() {
    let tmp = locked_change_repo(FIXTURE_LOCK, None);
    let gate_dir = tempfile::tempdir().expect("사본 디렉토리");
    fs::create_dir_all(gate_dir.path().join("lib")).expect("사본 디렉토리 생성");
    fs::copy(
        format!("{}/scripts/lib/judge-bin.sh", env!("CARGO_MANIFEST_DIR")),
        gate_dir.path().join("lib/judge-bin.sh"),
    )
    .expect("공용 헬퍼 사본");
    let gate = gate_dir.path().join("check-plugin-version-bump.sh");
    fs::copy(script(), &gate).expect("게이트 사본");
    assert_refused(
        "판독기 없음",
        check_with(
            &gate.to_string_lossy(),
            tmp.path(),
            &["--range", "HEAD^", "HEAD"],
        ),
        "TOML 판독기가 없다",
    );
}

/// PATH 앞의 `git` 스텁이 `pattern`(sh case 패턴)과 맞는 호출만 실패시키고 나머지는 실제 git 으로 넘긴다.
fn check_with_failing_git(d: &Path, pattern: &str) -> (i32, String) {
    let stub = tempfile::tempdir().expect("스텁 디렉토리");
    let dir = stub.path().display().to_string();
    tasty_test_support::write_executable(
        &stub.path().join("git"),
        format!(
            "#!/bin/sh\ncase \"$*\" in\n  {pattern}) echo zz-git-refused >&2; exit 128 ;;\nesac\n\
             PATH=\"${{PATH#{dir}:}}\" exec git \"$@\"\n"
        ),
    )
    .expect("스텁 작성");
    let out = git_env::command("bash")
        .arg(script())
        .args(["--range", "HEAD^", "HEAD"])
        .current_dir(d)
        .env(
            "PATH",
            format!("{dir}:{}", std::env::var("PATH").unwrap_or_default()),
        )
        .output()
        .expect("게이트 스크립트 실행");
    let run = gate_env::GateRun::from_output(&out);
    (run.code, run.output)
}

/// 잠금 파일·매니페스트를 git 에서 꺼내는 단계가 실패하면 그 단계를 말하고 판정 불가로 멈춘다.
#[test]
fn a_git_failure_while_reading_the_lock_or_a_manifest_names_the_step() {
    for (what, pattern, words) in [
        (
            "트리 나열",
            "\"ls-tree -r --name-only \"*\" -- official-plugins.lock.toml\"",
            "비교 대상 트리를 나열하지 못했다",
        ),
        (
            "잠금 파일 꺼내기",
            "\"show \"*\":official-plugins.lock.toml\"",
            "잠금 파일을 꺼내지 못했다",
        ),
        (
            "매니페스트 꺼내기",
            "\"show \"*\":crates/tasty-plugin-fixture/tasty-plugin.toml\"",
            "매니페스트를 꺼내지 못했다",
        ),
    ] {
        let tmp = locked_change_repo(FIXTURE_LOCK, None);
        let (code, text) = check_with_failing_git(tmp.path(), pattern);
        assert!(
            text.contains("zz-git-refused"),
            "{what}: 스텁이 실패시킬 호출이 없었다:\n{text}"
        );
        assert_refused(what, (code, text), words);
    }
}
