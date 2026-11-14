//! strip-cfg-test의 전체 테스트 파일 제거·문자 리터럴 중화 옵션을 확인한다.
//! 기본값은 인라인 테스트 범위만 제거하며 파일 전체 제거와 문자 중화는 명시적으로 켜야 한다.
//! SLOC 검사는 문자 중화를 쓰지만 내용 변경을 보는 플러그인 버전 검사는 쓰면 안 된다.
//! cfg_attr의 test 전용 속성 제거도 실제 바이너리 출력으로 확인한다.
//! 외부 계약: tasty-official-plugins 레포의 install-judge가 이 bin을 이름으로 설치하고
//! 종료 코드로 설치 성공을 판정한다. 계약 시험의 기대값을 바꾸려면 bin 머리 주석의 계약과
//! tasty-official-plugins/scripts/install-judge.sh·tasty-official-plugins/scripts/check-plugin-version-bump.sh를
//! 함께 고친다.

// 이유: 테스트의 값 무시를 제품 코드의 lint 목록에서 제외한다.
#![allow(clippy::let_underscore_must_use)]

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_strip-cfg-test");

/// PID와 테스트별 태그로 임시 디렉터리를 구분한다. 같은 태그의 재호출은 구분하지 못한다.
struct Tmp(PathBuf);
impl Tmp {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("tasty-stripbin-{}-{tag}", std::process::id()));
        // 이전 실행의 임시 경로를 정리한다. 없어도 정상이다.
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("임시 디렉토리");
        Self(d)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        // 정리 실패가 원래 테스트 실패를 가리지 않게 한다.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `lib.rs` 가 `helper` 는 그냥, `guard` 는 test 게이트로 선언한 최소 크레이트.
fn fixture(root: &Path) {
    let src = root.join("crates/demo/src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("lib.rs"),
        "mod helper;\n#[cfg(test)]\nmod guard;\n",
    )
    .unwrap();
    std::fs::write(
        src.join("helper.rs"),
        "pub fn ship() {}\npub fn also() {}\n",
    )
    .unwrap();
    std::fs::write(
        src.join("guard.rs"),
        "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n",
    )
    .unwrap();
    std::fs::write(src.join("attrs.rs"), attrs_fixture()).unwrap();
    // 계측기(`tokei`)가 문자열의 시작으로 오독하는 형태 — test 전용이 아닌 코드다.
    std::fs::write(
        src.join("quotes.rs"),
        "pub fn q(c: char) -> u8 {\n    match c {\n        '\"' => 2,\n        _ => 1,\n    }\n}\n",
    )
    .unwrap();
}

/// 줄 단위 사유 검사가 합성 allow를 실제 억제로 세지 않도록 이름을 조립한다.
fn attrs_fixture() -> String {
    let a = "allow";
    format!(
        "#![cfg_attr(test, {a}(clippy::x))]\n\
         pub fn ship_one() {{}}\n\
         #[cfg_attr(not(test), {a}(clippy::y))]\n\
         pub fn ship_two() {{}}\n"
    )
}

/// tokei는 따옴표 문자 리터럴 뒤의 빈 줄을 코드로 잘못 셀 수 있어 계측용 사본만 중화한다.
/// 내용 비교에도 중화를 적용하면 서로 다른 문자 리터럴을 같게 보므로 기본값은 그대로여야 한다.
#[test]
fn the_flag_makes_char_literal_quotes_safe_for_the_line_counter() {
    for (i, flag) in [None, Some("--neutralize-char-literal-quotes")]
        .into_iter()
        .enumerate()
    {
        let root = Tmp::new(&format!("quote-root-{i}"));
        let out = Tmp::new(&format!("quote-out-{i}"));
        fixture(root.path());
        run(root.path(), out.path(), flag);
        let got = std::fs::read_to_string(out.path().join("crates/demo/src/quotes.rs")).unwrap();
        if flag.is_some() {
            assert!(
                got.contains("'x' => 2,"),
                "켰는데 문자 리터럴이 안 바뀌었다 — 계측기가 그 뒤를 문자열로 읽는다: {got:?}"
            );
        } else {
            assert!(
                got.contains("'\"' => 2,"),
                "기본값이 문자 리터럴을 바꿨다 — 내용 동등을 묻는 소비자가 차이를 놓친다: {got:?}"
            );
        }
        assert!(
            got.contains("pub fn q(c: char)"),
            "제품 코드가 사라졌다: {got:?}"
        );
        let original =
            std::fs::read_to_string(root.path().join("crates/demo/src/quotes.rs")).unwrap();
        assert_eq!(
            got.split('\n').count(),
            original.split('\n').count(),
            "줄 수가 달라졌다 (flag={flag:?})"
        );
    }
}

fn run(root: &Path, out: &Path, flag: Option<&str>) {
    let mut cmd = Command::new(BIN);
    if let Some(f) = flag {
        cmd.arg(f);
    }
    // 자식 stderr를 수집해 병렬 실행에서도 해당 실패와 함께 보고한다.
    let result = cmd
        .arg(out)
        .arg(root)
        .arg("crates")
        .output()
        .expect("strip-cfg-test 를 실행할 수 없다");
    assert!(
        result.status.success(),
        "종료코드 {:?}\nstderr:\n{}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn the_flag_blanks_a_file_that_is_declared_test_only() {
    let root = Tmp::new("blank-root");
    let out = Tmp::new("blank-out");
    fixture(root.path());
    run(root.path(), out.path(), Some("--blank-test-only-files"));

    let guard = std::fs::read_to_string(out.path().join("crates/demo/src/guard.rs")).unwrap();
    assert!(
        guard.trim().is_empty(),
        "전체-테스트 파일이 안 비워졌다: {guard:?}"
    );
    let original = std::fs::read_to_string(root.path().join("crates/demo/src/guard.rs")).unwrap();
    assert_eq!(
        guard.split('\n').count(),
        original.split('\n').count(),
        "줄 수가 달라졌다"
    );
}

#[test]
fn without_the_flag_that_same_file_is_left_alone() {
    let root = Tmp::new("keep-root");
    let out = Tmp::new("keep-out");
    fixture(root.path());
    run(root.path(), out.path(), None);

    let guard = std::fs::read_to_string(out.path().join("crates/demo/src/guard.rs")).unwrap();
    assert!(
        guard.contains("fn a()"),
        "기본값이 전체-테스트 파일을 건드렸다 — 플래그 없이 부르는 쪽의 계약(인라인 범위만 지운다)이 깨졌다: {guard:?}"
    );
}

#[test]
fn a_shipping_file_survives_either_way() {
    for (i, flag) in [None, Some("--blank-test-only-files")]
        .into_iter()
        .enumerate()
    {
        let root = Tmp::new(&format!("ship-root-{i}"));
        let out = Tmp::new(&format!("ship-out-{i}"));
        fixture(root.path());
        run(root.path(), out.path(), flag);
        let helper = std::fs::read_to_string(out.path().join("crates/demo/src/helper.rs")).unwrap();
        assert!(
            helper.contains("pub fn ship()"),
            "test 전용이 아닌 파일이 지워졌다 (flag={flag:?}): {helper:?}"
        );
    }
}

/// test 전용 cfg_attr만 지우고 not(test) 속성과 속성이 붙은 test 전용이 아닌 항목은 남겨야 한다.
#[test]
fn a_cfg_attr_that_requires_test_is_stripped_and_its_opposite_is_not() {
    for (i, flag) in [None, Some("--blank-test-only-files")]
        .into_iter()
        .enumerate()
    {
        let root = Tmp::new(&format!("attr-root-{i}"));
        let out = Tmp::new(&format!("attr-out-{i}"));
        fixture(root.path());
        run(root.path(), out.path(), flag);
        let got = std::fs::read_to_string(out.path().join("crates/demo/src/attrs.rs")).unwrap();
        assert!(
            !got.contains("cfg_attr(test"),
            "`test` 를 요구하는 속성이 test 코드를 제외한 사본에 남았다 (flag={flag:?}): {got:?}"
        );
        assert!(
            got.contains("cfg_attr(not(test)"),
            "프로덕션 전용 속성을 지웠다 — 제품 코드가 판정 밖으로 나간다 (flag={flag:?}): {got:?}"
        );
        assert!(
            got.contains("pub fn ship_one()") && got.contains("pub fn ship_two()"),
            "속성이 붙은 항목까지 지웠다 (flag={flag:?}): {got:?}"
        );
    }
}

// 외부 계약 시험: tasty-official-plugins 레포의 install-judge가 기대는 이름과 종료 코드다.

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn exit_code(args: &[&std::ffi::OsStr]) -> Option<i32> {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("strip-cfg-test 를 실행할 수 없다")
        .status
        .code()
}

#[test]
fn the_package_name_is_the_one_outside_consumers_install() {
    assert_eq!(
        env!("CARGO_PKG_NAME"),
        "tasty-doc-guards",
        "plugins 레포가 `cargo install ... tasty-doc-guards --bin strip-cfg-test`로 설치한다"
    );
}

#[test]
fn check_fresh_without_the_library_source_is_undecidable_for_outside_consumers() {
    let root = Tmp::new("fresh-absent");
    let rc = exit_code(&["--check-fresh".as_ref(), root.path().as_os_str()]);
    assert_eq!(
        rc,
        Some(3),
        "plugins 레포 install-judge가 rc 0·3만 설치 성공으로 본다"
    );
}

#[test]
fn check_fresh_on_this_repository_is_fresh() {
    let root = repo_root();
    let rc = exit_code(&["--check-fresh".as_ref(), root.as_os_str()]);
    assert_eq!(
        rc,
        Some(0),
        "방금 빌드한 bin이 자기 소스와 다르다고 판정했다"
    );
}

#[test]
fn no_arguments_is_a_usage_error() {
    assert_eq!(
        exit_code(&[]),
        Some(2),
        "인자 오류는 rc 2다. plugins 레포는 이 값으로 호출 실패를 구별한다"
    );
}
