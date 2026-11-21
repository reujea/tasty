//! 잠금 파일(`official-plugins.lock.toml`) id인 매니페스트 크레이트를 판정 대상에서 빼는 규칙을 확인한다.
//! 공식 플러그인은 공식 플러그인 레포가 버전을 관리하고 tasty는 잠금 파일의 릴리스 산출물만 싣는다.

use super::*;

const SECOND: &str = "crates/tasty-plugin-second";
const LOCKED_NOTE: &str = "잠금 파일의 공식 플러그인 id라 대상에서 뺐다";

fn lock_body(ids: &[&str]) -> String {
    let mut body = String::from(
        "schema = 1\n\n[release]\nrepository = \"example/official\"\ntag = \"v0.0.1\"\n",
    );
    for id in ids {
        body.push_str(&format!(
            "\n[[plugin]]\nid = \"{id}\"\nversion = \"9.9.9\"\n\n  [[plugin.artifact]]\n  platform = \"linux-x64\"\n  file = \"{id}.tar.gz\"\n"
        ));
    }
    body
}

fn write_lock(d: &Path, ids: &[&str]) {
    write(d, "official-plugins.lock.toml", &lock_body(ids));
}

/// 잠금 파일에 없는 id의 두 번째 플러그인을 더한다. `deps`는 `[dependencies]` 본문이다.
fn add_second_plugin(d: &Path, deps: &str) {
    write(
        d,
        &format!("{SECOND}/Cargo.toml"),
        &format!(
            "[package]\nname = \"tasty-plugin-second\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
             [dependencies]\n{deps}"
        ),
    );
    write(
        d,
        &format!("{SECOND}/tasty-plugin.toml"),
        "id = \"com.tasty.second\"\nversion = \"0.1.0\"\n",
    );
    write(d, &format!("{SECOND}/src/main.rs"), "fn main() {}\n");
}

/// 루트 Cargo.toml의 members 앞에 두 번째 플러그인을 넣는다.
fn add_second_member(d: &Path) {
    let p = d.join("Cargo.toml");
    let src = fs::read_to_string(&p).expect("루트 Cargo.toml");
    assert_eq!(
        src.matches("members = [").count(),
        1,
        "members 선언이 하나여야 한다"
    );
    let out = src.replace("members = [", &format!("members = [\"{SECOND}\", "));
    fs::write(&p, out).expect("루트 Cargo.toml 쓰기");
}

/// 변경을 스테이징해 staged 모드로 재고, 커밋해 range 모드로 다시 잰다.
fn both_modes(d: &Path, what: &str, mut judge: impl FnMut(&str, i32, &str)) {
    run_git(d, &["add", "-A"]);
    let (code, text) = check(d, &["--staged"]);
    judge("staged", code, &text);
    run_git(d, &["commit", "--quiet", "-m", what]);
    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    judge("range", code, &text);
}

fn expect_only_second(mode: &str, code: i32, text: &str) {
    assert_eq!(
        code, 1,
        "{mode}: 잠금 파일에 없는 id의 플러그인 변경을 놓쳤다:\n{text}"
    );
    assert!(
        text.contains(&format!("✘ [plugin-version] {SECOND}:")),
        "{mode}: 위반 대상이 두 번째 플러그인이 아니다:\n{text}"
    );
    assert!(
        !text.contains(&format!("✘ [plugin-version] {PLUGIN}:")),
        "{mode}: 잠금 파일 id인 크레이트에 버전을 요구했다:\n{text}"
    );
    assert!(
        text.contains(LOCKED_NOTE) && text.contains(PLUGIN),
        "{mode}: 뺀 크레이트를 알리지 않는다:\n{text}"
    );
}

/// 같은 커밋에서 잠금 파일 id인 플러그인은 빠지고 잠금 파일에 없는 id의 플러그인은 여전히 위반이다.
#[test]
fn a_locked_plugin_changed_directly_is_skipped_while_an_unlocked_one_is_not() {
    let tmp = seed_repo();
    let d = tmp.path();
    add_second_plugin(d, "");
    write_lock(d, &["com.tasty.fixture"]);
    commit_all(d, "seed lock and second plugin");

    for dir in [PLUGIN, SECOND] {
        write(
            d,
            &format!("{dir}/src/main.rs"),
            "fn main() {\n    let msg = \"changed\";\n}\n",
        );
    }
    both_modes(d, "change both plugins", expect_only_second);
}

#[test]
fn a_shared_crate_change_skips_the_locked_plugin_and_flags_the_unlocked_one() {
    let tmp = seed_workspace();
    let d = tmp.path();
    add_second_plugin(d, "tasty-shared = { path = \"../tasty-shared\" }\n");
    add_second_member(d);
    write_lock(d, &["com.tasty.fixture"]);
    commit_all(d, "seed lock and second plugin");

    write(
        d,
        "crates/tasty-shared/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    both_modes(d, "change the shared crate", expect_only_second);
}

#[test]
fn an_outside_path_dependency_change_skips_the_locked_plugin_and_flags_the_unlocked_one() {
    let tmp = seed_vendored_workspace();
    let d = tmp.path();
    add_second_plugin(d, "upstream = { path = \"../../vendor/upstream\" }\n");
    add_second_member(d);
    write_lock(d, &["com.tasty.fixture"]);
    commit_all(d, "seed lock and second plugin");

    write(
        d,
        "vendor/upstream/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    both_modes(d, "change the outside dependency", |mode, code, text| {
        assert_eq!(
            code, 1,
            "{mode}: 잠금 파일에 없는 id의 플러그인이 링크한 외부 의존 변경을 놓쳤다:\n{text}"
        );
        assert!(
            text.contains(&format!("✘ [plugin-version] {SECOND}:")),
            "{mode}: 위반 대상이 두 번째 플러그인이 아니다:\n{text}"
        );
        assert!(
            !text.contains(&format!("✘ [plugin-version] {PLUGIN}:")),
            "{mode}: 잠금 파일 id인 크레이트에 버전을 요구했다:\n{text}"
        );
    });
}

/// 판정은 비교 대상 트리(index 또는 AFTER)의 잠금 파일을 읽는다. 작업 트리에만 있는 잠금 파일은 쓰지 않는다.
#[test]
fn a_lock_file_only_in_the_work_tree_is_not_used() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    run_git(d, &["add", "-A"]);
    write_lock(d, &["com.tasty.fixture"]);

    let (code, text) = check(d, &["--staged"]);
    assert_eq!(
        code, 1,
        "작업 트리에만 있는 잠금 파일로 판정을 뺐다:\n{text}"
    );
    run_git(d, &["commit", "--quiet", "-m", "change", "--", PLUGIN]);
    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        code, 1,
        "AFTER 트리에 없는 잠금 파일로 판정을 뺐다:\n{text}"
    );
}

/// 잠금 파일 본문(과 매니페스트)만 바꿔 같은 변경을 staged·range 두 모드로 잰다.
/// 두 모드의 종료 코드와 잠금 id 안내 유무가 같아야 하며, range 결과를 돌려준다.
fn judge_with_lock(lock: &str, manifest: Option<&str>) -> (i32, String) {
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
    run_git(d, &["add", "-A"]);
    let (staged_code, staged_text) = check(d, &["--staged"]);
    run_git(d, &["commit", "--quiet", "-m", "change"]);
    let (code, text) = check(d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(
        (staged_code, staged_text.contains(LOCKED_NOTE)),
        (code, text.contains(LOCKED_NOTE)),
        "staged·range 판정이 다르다:\n--- staged\n{staged_text}\n--- range\n{text}"
    );
    (code, text)
}

const LOCKED_TABLE: &str = "\n[[plugin]]\nid = \"com.tasty.fixture\"\nversion = \"9.9.9\"\n";

#[test]
fn only_ids_inside_plugin_tables_count_and_spaced_headers_are_read() {
    let outside = "schema = 1\n[release]\nid = \"com.tasty.fixture\"\n\n[[plugin]]\nid = \"com.tasty.other\"\n";
    let (code, text) = judge_with_lock(outside, None);
    assert_eq!(code, 1, "[[plugin]] 표 밖의 id 를 잠금 id 로 셌다:\n{text}");

    let spaced = "schema = 1\n\n[[ plugin ]] # 주석\nid = \"com.tasty.fixture\" # 주석\n\
                  \n  [[plugin.artifact]]\n  id = \"com.tasty.other\"\n";
    let (code, text) = judge_with_lock(spaced, None);
    assert_eq!(code, 0, "공백·주석이 있는 표 머리를 읽지 못했다:\n{text}");
    assert!(
        text.contains(LOCKED_NOTE),
        "잠금 id 판정을 알리지 않는다:\n{text}"
    );
}

#[test]
fn an_unreadable_lock_file_is_undecidable() {
    let cases = [
        (
            "빈 플러그인 목록",
            "schema = 1\n[release]\ntag = \"v0\"\n".to_owned(),
        ),
        ("id 없는 표", "[[plugin]]\nversion = \"1.0.0\"\n".to_owned()),
        (
            "표 안 id 두 개",
            "[[plugin]]\nid = \"com.tasty.fixture\"\nid = \"com.tasty.other\"\n".to_owned(),
        ),
        ("표 사이 중복 id", format!("{LOCKED_TABLE}{LOCKED_TABLE}")),
        ("문자열이 아닌 값", "[[plugin]]\nid = 7\n".to_owned()),
        (
            "따옴표 키",
            "[[plugin]]\n\"id\" = \"com.tasty.fixture\"\n".to_owned(),
        ),
        (
            "리터럴 문자열",
            "[[plugin]]\nid = 'com.tasty.fixture'\n".to_owned(),
        ),
        (
            "닫히지 않은 여러 줄 문자열",
            format!("{LOCKED_TABLE}note = \"\"\"\n열림\n"),
        ),
    ];
    for (what, lock) in cases {
        let (code, text) = judge_with_lock(&lock, None);
        assert_eq!(
            code, 2,
            "{what}: 잠금 파일을 읽지 못했는데 판정 불가가 아니다:\n{text}"
        );
        assert!(
            text.contains("잠금 파일"),
            "{what}: 원인이 잠금 파일임을 말하지 않는다:\n{text}"
        );
    }
}

#[test]
fn a_lock_id_outside_the_official_id_space_is_undecidable() {
    for id in [
        "com.example.demo",
        "com.tasty.bad_suffix",
        "comXtastyYdemo",
        "com.tasty.demo.extra",
    ] {
        let lock = format!("[[plugin]]\nid = \"{id}\"\n");
        let (code, text) = judge_with_lock(&lock, None);
        assert_eq!(
            code, 2,
            "{id}: 공식 플러그인 id 꼴이 아닌 잠금 id 를 받았다:\n{text}"
        );
    }
}

fn locked_manifest_judgement(manifest: &str) -> (i32, String) {
    judge_with_lock(&format!("schema = 1\n{LOCKED_TABLE}"), Some(manifest))
}

#[test]
fn quoted_keys_and_literal_strings_in_a_manifest_are_read() {
    for manifest in [
        "\"id\" = \"com.tasty.fixture\"\nversion = \"0.1.0\"\n",
        "id = 'com.tasty.fixture'\nversion = \"0.1.0\"\n",
        "'id' = 'com.tasty.fixture'\nversion = \"0.1.0\"\n",
    ] {
        let (code, text) = locked_manifest_judgement(manifest);
        assert_eq!(
            code, 0,
            "매니페스트 id 를 읽지 못했다 ({manifest:?}):\n{text}"
        );
        assert!(
            text.contains(LOCKED_NOTE),
            "잠금 id 로 판정하지 않았다:\n{text}"
        );
    }
}

#[test]
fn comments_and_nested_arrays_do_not_make_a_manifest_unreadable() {
    let manifest = "id = \"com.tasty.fixture\" # \"\"\" 따옴표 묶음\nversion = \"0.1.0\"\n\
                    # \"\"\" 주석 안 따옴표\nmatrix = [\n  [1, 2],\n  [3],\n]\n\
                    description = \"\"\"\"한 줄\"\"\"\"\ntitle = '''한 줄'''''\n\n[entry]\ncommand = \"x\"\n";
    let (code, text) = locked_manifest_judgement(manifest);
    assert_eq!(code, 0, "정상 매니페스트를 판정 불가로 만들었다:\n{text}");
    assert!(
        text.contains(LOCKED_NOTE),
        "잠금 id 로 판정하지 않았다:\n{text}"
    );
}

#[test]
fn a_manifest_id_the_scanner_cannot_be_sure_of_is_undecidable() {
    for (what, manifest) in [
        ("이스케이프 값", "id = \"com.tasty.\\u0066ixture\"\n"),
        (
            "이스케이프 따옴표 키",
            "\"i\\u0064\" = \"com.tasty.fixture\"\n",
        ),
        (
            "탭이 든 따옴표 키",
            "\"id\t\" = \"com.tasty.fixture\"\nid = \"com.tasty.fixture\"\n",
        ),
        (
            "id 두 개",
            "id = \"com.tasty.fixture\"\nid = \"com.tasty.other\"\n",
        ),
        (
            "id 없음",
            "version = \"0.1.0\"\n[entry]\nid = \"com.tasty.fixture\"\n",
        ),
    ] {
        let (code, text) = locked_manifest_judgement(manifest);
        assert_eq!(
            code, 2,
            "{what}: 확신할 수 없는 매니페스트 id 를 받았다:\n{text}"
        );
    }
}

#[test]
fn a_tab_inside_a_description_value_is_still_read() {
    let manifest = "id = \"com.tasty.fixture\"\ndescription = \"탭\t있음\"\nversion = \"0.1.0\"\n";
    let (code, text) = locked_manifest_judgement(manifest);
    assert_eq!(code, 0, "값의 탭 때문에 매니페스트를 읽지 못했다:\n{text}");
    assert!(
        text.contains(LOCKED_NOTE),
        "잠금 id 로 판정하지 않았다:\n{text}"
    );
}

/// 여러 줄 문자열 안의 가짜 `id = …`와 `[entry]` 줄을 실제 키·표 머리로 읽으면 잠금 id 로 오인해 위반을 놓친다.
#[test]
fn a_fake_id_inside_a_multiline_string_does_not_hide_an_unlocked_plugin() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/tasty-plugin.toml"),
        "description = \"\"\"\n[entry]\nid = \"com.tasty.locked\"\n\"\"\"\nid = \"com.tasty.fixture\"\nversion = \"0.1.0\"\n",
    );
    write_lock(d, &["com.tasty.locked"]);
    commit_all(d, "seed lock");
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    both_modes(d, "change", |mode, code, text| {
        assert_eq!(
            code, 1,
            "{mode}: 여러 줄 문자열 안의 가짜 id 로 판정을 뺐다:\n{text}"
        );
        assert!(
            !text.contains(LOCKED_NOTE),
            "{mode}: 잠금 id 로 판정했다:\n{text}"
        );
    });
}

/// 픽스처는 어떤 플러그인의 빌드 입력도 아니라 의존 폐포를 조회하지 않는다.
#[test]
fn a_fixture_only_change_does_not_query_the_dependency_closure() {
    const FIXTURE: &str = "crates/tasty-test-support/fixtures/official-plugins/com.tasty.fixture";
    let tmp = seed_workspace();
    let d = tmp.path();
    write(
        d,
        &format!("{FIXTURE}/tasty-plugin.toml"),
        "id = \"com.tasty.fixture\"\nversion = \"9.9.9\"\n",
    );
    write(d, &format!("{FIXTURE}/lang/en.toml"), "a = \"b\"\n");
    commit_all(d, "seed fixture");
    write(d, &format!("{FIXTURE}/lang/en.toml"), "a = \"c\"\n");
    commit_all(d, "change fixture");

    let stub = tempfile::tempdir().expect("스텁 디렉토리");
    tasty_test_support::write_executable(
        &stub.path().join("cargo"),
        "#!/bin/sh\necho zz-cargo-called >&2\nexit 101\n".to_owned(),
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
        run.code, 0,
        "픽스처만 바꿨는데 판정이 실패했다:\n{}",
        run.output
    );
    assert!(
        !run.output.contains("zz-cargo-called"),
        "픽스처 변경으로 cargo 를 불렀다:\n{}",
        run.output
    );
    assert!(
        run.output.contains("판정 대상 0 건"),
        "판정 대상이 0 이 아니다:\n{}",
        run.output
    );
}

/// 픽스처 경로는 잠금 파일을 읽을지 정하기 전에 빠진다. 읽을 수 없는 잠금 파일이 있어도 픽스처만 바꾼
/// 변경은 두 모드 모두 판정 대상 0건으로 끝난다.
#[test]
fn a_fixture_only_change_skips_the_lock_and_manifest_reads() {
    const FIXTURE: &str = "crates/tasty-test-support/fixtures/official-plugins/com.tasty.fixture";
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        "official-plugins.lock.toml",
        "[[plugins]]\nid = \"com.tasty.fixture\"\n",
    );
    write(
        d,
        &format!("{FIXTURE}/tasty-plugin.toml"),
        "id = \"com.tasty.fixture\"\nversion = \"9.9.9\"\n",
    );
    write(d, &format!("{FIXTURE}/lang/en.toml"), "a = \"b\"\n");
    commit_all(d, "seed fixture and an unreadable lock");
    write(d, &format!("{FIXTURE}/lang/en.toml"), "a = \"c\"\n");
    write(
        d,
        &format!("{FIXTURE}/tasty-plugin.toml"),
        "\"a\\u0062\" = 1\n",
    );
    both_modes(d, "change fixture", |mode, code, text| {
        assert_eq!(
            code, 0,
            "{mode}: 픽스처만 바꿨는데 잠금 파일을 읽었다:\n{text}"
        );
        assert!(
            text.contains("판정 대상 0 건"),
            "{mode}: 판정 대상이 0건이 아니다:\n{text}"
        );
    });
}

/// 수집 루트를 넓힌 사본도 스캐너를 찾아 잠금 id 판정을 한다.
#[test]
fn a_widened_root_still_reads_the_lock_file() {
    let tmp = seed_repo_under("extra");
    let d = tmp.path();
    write_lock(d, &["com.tasty.fixture"]);
    commit_all(d, "seed lock");
    write(
        d,
        "extra/tasty-plugin-fixture/src/main.rs",
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    commit_all(d, "change");
    let gate = widened_script(d, "extra");
    let (code, text) = check_with(&gate, d, &["--range", "HEAD^", "HEAD"]);
    assert_eq!(code, 0, "넓힌 루트에서 잠금 id 를 판정하지 않았다:\n{text}");
    assert!(
        text.contains(LOCKED_NOTE),
        "잠금 id 판정을 알리지 않는다:\n{text}"
    );
}

fn scan(body: &str) -> (i32, String) {
    let dir = tempfile::tempdir().expect("임시 디렉토리");
    let p = dir.path().join("in.toml");
    fs::write(&p, body).expect("입력 쓰기");
    let out = git_env::command("awk")
        .arg("-f")
        .arg(format!(
            "{}/scripts/lib/toml-scan.awk",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(&p)
        .output()
        .expect("awk 실행");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn the_scanner_rejects_unclosed_strings_and_brackets() {
    for body in ["a = \"\"\"\n열림\n", "a = '''\n열림\n", "a = [\n1,\n"] {
        let (code, out) = scan(body);
        assert_eq!(code, 3, "닫히지 않은 입력을 받았다 ({body:?}):\n{out}");
        assert!(
            out.starts_with("E\t") || out.contains("\nE\t"),
            "오류 레코드가 없다:\n{out}"
        );
    }
}

#[test]
fn the_scanner_reads_keys_after_closing_quote_runs_and_marks_tab_keys() {
    let (code, out) = scan("a = \"\"\"x\"\"\"\"\nb = '''y'''''\nid = \"z\"\n\"i\td\" = \"w\"\n");
    assert_eq!(code, 0, "정상 입력을 거부했다:\n{out}");
    assert!(
        out.contains("K\tbare\tid\tbasic\tz\tknown\tid\n"),
        "닫는 따옴표 묶음 뒤 키를 읽지 못했다:\n{out}"
    );
    assert!(
        out.contains("K\tunknown\t\tbasic\tw\tunknown\t\n"),
        "탭이 든 따옴표 키를 unknown 으로 내지 않았다:\n{out}"
    );
}

/// 점으로 이은 키(`"entry".type`)는 TOML에서 `[entry]` 표와 같다. id 와 무관한 정상 표기라 판정 불가가 아니어야 한다.
#[test]
fn dotted_keys_in_a_manifest_are_read_in_both_modes() {
    let tmp = seed_repo();
    let d = tmp.path();
    write(
        d,
        &format!("{PLUGIN}/tasty-plugin.toml"),
        "id = \"com.tasty.fixture\"\nversion = \"0.1.0\"\n\"entry\".type = \"process\"\nentry . command = \"tasty-plugin-fixture\"\n",
    );
    write_lock(d, &["com.tasty.fixture"]);
    commit_all(d, "seed lock");
    write(
        d,
        &format!("{PLUGIN}/src/main.rs"),
        "fn main() {\n    let msg = \"changed\";\n}\n",
    );
    both_modes(d, "change", |mode, code, text| {
        assert_eq!(
            code, 0,
            "{mode}: 점으로 이은 키 때문에 판정이 실패했다:\n{text}"
        );
        assert!(
            text.contains(LOCKED_NOTE),
            "{mode}: 잠금 id 로 판정하지 않았다:\n{text}"
        );
    });
}

#[test]
fn the_scanner_reads_dotted_keys_and_marks_escaped_segments() {
    let (code, out) =
        scan("\"entry\".type = \"exec\"\nentry . command = 'x'\n\"a\\u0062\".c = 1\n");
    assert_eq!(code, 0, "정상 입력을 거부했다:\n{out}");
    assert!(
        out.contains("K\tdotted\tentry.type\tbasic\texec\tknown\tentry\n"),
        "따옴표 조각이 든 점 구분 키를 읽지 못했다:\n{out}"
    );
    assert!(
        out.contains("K\tdotted\tentry.command\tliteral\tx\tknown\tentry\n"),
        "공백이 든 점 구분 키를 읽지 못했다:\n{out}"
    );
    assert!(
        out.contains("K\tunknown\t\tother\t\tunknown\t\n"),
        "이스케이프 조각을 unknown 으로 내지 않았다:\n{out}"
    );
}

/// 스캐너가 확신할 수 없는 표 머리나 generate 가 쓰지 않는 표 머리는 id 를 조용히 빼지 않고 판정 불가로 멈춘다.
/// 배포 도구는 `[[plugins]]` 를 거부하지만 줄바꿈 없는 공백은 받으므로, 어느 쪽이든 판정을 멈추는 것이 안전하다.
#[test]
fn a_lock_table_header_the_scanner_cannot_read_is_undecidable() {
    for (what, lock) in [
        (
            "줄바꿈 없는 공백",
            "[[\u{a0}plugin]]\nid = \"com.tasty.fixture\"\n".to_owned(),
        ),
        (
            "따옴표 표 이름",
            format!("{LOCKED_TABLE}[\"plugin\"]\nid = \"com.tasty.other\"\n"),
        ),
        (
            "점 앞뒤 공백",
            format!("{LOCKED_TABLE}\n  [[plugin . artifact]]\n  platform = \"x\"\n"),
        ),
        (
            "두 번 나온 [release]",
            format!("[release]\ntag = \"v1\"\n{LOCKED_TABLE}\n[release]\ntag = \"v2\"\n"),
        ),
        (
            "앞선 [[plugin]] 없는 artifact",
            format!("[[plugin.artifact]]\nplatform = \"x\"\n{LOCKED_TABLE}"),
        ),
        (
            "generate 가 쓰지 않는 표",
            format!("[[plugins]]\nid = \"com.tasty.other\"\n{LOCKED_TABLE}"),
        ),
    ] {
        let (code, text) = judge_with_lock(&lock, None);
        assert_eq!(code, 2, "{what}: 읽지 못한 표 머리를 건너뛰었다:\n{text}");
    }
}

/// 매니페스트의 따옴표 표 이름(`["entry"]`)은 TOML로 `[entry]`와 같다. 최상위 id 를 그대로 읽어 잠금 id 로 판정한다.
#[test]
fn a_quoted_table_name_in_a_manifest_is_read() {
    let (code, text) = locked_manifest_judgement(
        "id = \"com.tasty.fixture\"\nversion = \"0.1.0\"\n[\"entry\"]\ntype = \"process\"\n",
    );
    assert_eq!(
        code, 0,
        "따옴표 표 이름 매니페스트를 판정하지 못했다:\n{text}"
    );
    assert!(
        text.contains(LOCKED_NOTE),
        "잠금 id 로 판정하지 않았다:\n{text}"
    );
}

/// 판독기가 쓰는 표지와 같은 글자의 실제 키는 그 키로 읽는다.
#[test]
fn a_real_question_mark_key_is_not_the_unknown_marker() {
    let (code, text) = locked_manifest_judgement(
        "id = \"com.tasty.fixture\"\n\"?\" = \"ignored\"\nversion = \"0.1.0\"\n",
    );
    assert_eq!(code, 0, "\"?\" 키를 읽을 수 없는 키로 보았다:\n{text}");
    assert!(
        text.contains(LOCKED_NOTE),
        "잠금 id 로 판정하지 않았다:\n{text}"
    );
}

/// 잠금 id 플러그인만 링크하는 워크스페이스 밖 의존이 바뀌면 그 플러그인의 폐포를 조회하지 않고 뺐다고 알린다.
#[test]
fn an_outside_dependency_linked_only_by_a_locked_plugin_skips_its_closure() {
    let tmp = seed_vendored_workspace();
    let d = tmp.path();
    write_lock(d, &["com.tasty.fixture"]);
    commit_all(d, "seed lock");
    write(
        d,
        "vendor/upstream/src/lib.rs",
        "pub fn greet() {\n    let _ = 2;\n}\n",
    );
    commit_all(d, "change the outside dependency");

    let stub = tempfile::tempdir().expect("스텁 디렉토리");
    tasty_test_support::write_executable(
        &stub.path().join("cargo"),
        format!(
            "#!/bin/sh\nif [ \"$1\" = tree ]; then echo zz-cargo-tree-called >&2; exit 101; fi\nexec \"{real}\" \"$@\"\n",
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
    assert_eq!(
        run.code, 0,
        "잠금 id 플러그인의 폐포를 조회했다:\n{}",
        run.output
    );
    assert!(
        !run.output.contains("zz-cargo-tree-called"),
        "cargo tree 를 불렀다:\n{}",
        run.output
    );
    assert!(
        run.output.contains(LOCKED_NOTE) && run.output.contains(PLUGIN),
        "뺀 크레이트를 알리지 않는다:\n{}",
        run.output
    );
}

#[test]
fn the_scanner_rejects_headers_it_cannot_read_and_keeps_markers_apart() {
    for body in ["[[\u{a0}plugin]]\n", "[a b]\n", "[\"a\"\n"] {
        let (code, out) = scan(body);
        assert_eq!(code, 3, "읽지 못한 표 머리를 받았다 ({body:?}):\n{out}");
    }
    let (code, out) = scan("[\"a]\"]\n[ \"entry\" ]\n[[ plugin . artifact ]]\n");
    assert_eq!(code, 0, "따옴표 이름 표 머리를 거부했다:\n{out}");
    assert_eq!(
        out,
        "H\t[]\tunknown\tknown\ta]\nH\t[entry]\tquoted\tknown\tentry\nH\t[[plugin.artifact]]\tspaced\tknown\tplugin\n",
        "표 머리 레코드가 다르다"
    );
    let (code, out) = scan("\"?\" = \"x\"\n\"a\\u0062\" = \"y\"\nflag = false # 주석\n");
    assert_eq!(code, 0, "정상 입력을 거부했다:\n{out}");
    assert!(
        out.contains("K\tquoted\t?\tbasic\tx\tknown\t?\n"),
        "\"?\" 키를 읽지 못했다:\n{out}"
    );
    assert!(
        out.contains("K\tunknown\t\tbasic\ty\tunknown\t\n"),
        "이스케이프 키를 unknown 으로 내지 않았다:\n{out}"
    );
    assert!(
        out.contains("K\tbare\tflag\tbool\tfalse\tknown\tflag\n"),
        "불 값을 읽지 못했다:\n{out}"
    );
}
