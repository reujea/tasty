//! 테스트 전용 코드를 빈 줄로 바꾼 사본을 만든다. 줄 번호를 보존하며 SLOC는 tokei로 센다.
//! 판독은 cfg_predicate::blank_gated_lines와 shipping_scope::test_only_files를 공유한다.
//!
//! 기본은 인라인 cfg(test) 범위를 비운다. test를 요구하는 cfg_attr은 속성 줄만 비우며
//! 항목 본문은 남긴다. 테스트 모듈 선언은 부모의 속성·mod 선언 줄도 함께 비운다.
//! --blank-test-only-files를 주면 테스트로만 선언된 별도 파일과 Cargo 통합 타깃도 비운다.
//!
//! --neutralize-char-literal-quotes는 문자 리터럴 안의 따옴표를 바꿔 tokei가 문자열
//! 시작으로 오독하지 않게 한다. 줄 수 측정에만 사용한다. 내용 비교에 사용하면
//! 서로 다른 문자 리터럴이 같아 보일 수 있으므로 플러그인 버전 검사에서는 사용하지 않는다.
//! 내용 비교 전에는 제거된 테스트가 남긴 빈 줄을 정규화한다.
//!
//! 종료코드 0은 사본 생성 완료, 2는 인자·루트·수집/쓰기 오류다. 파일이 없으면 실패한다.
//! 위반 여부는 이 도구를 호출한 검사가 판정한다.
//!
//! 외부 계약: tasty-official-plugins 레포의 플러그인 버전 검사가 이 bin을
//! `cargo install --git <tasty url> --rev <기준 커밋> --locked tasty-doc-guards --bin strip-cfg-test`로
//! 설치해 쓴다. 설치 확인은 `--check-fresh .`의 rc 0 또는 3이고, 호출 형태는
//! `--blank-test-only-files <out> <repo-root> <scan-root>...`이며 stdout은 버린다.
//! 다음을 유지한다.
//! - 패키지 이름 tasty-doc-guards와 bin 이름 strip-cfg-test.
//! - git 설치 가능성. 이 크레이트 밖의 파일을 빌드 입력으로 읽지 않는다. 의존을 더할 수는 있지만
//!   설치 시간이 늘고, 시스템 라이브러리가 필요한 의존은 그 레포 러너에서 설치를 깨뜨릴 수 있다.
//! - `[--blank-test-only-files] [--neutralize-char-literal-quotes] <out-dir> <repo-root> <scan-root>...` 형태,
//!   종료 코드 0·2, `--check-fresh <root>`의 0(일치)·1(재빌드 필요)·3(`<root>`에 이 크레이트 소스 없음).
//! - scan root 아래에 모듈 선언 체인이 있어야 테스트 전용 파일을 판정한다. 그 레포는 크레이트
//!   디렉터리 전체를 scan root로 넘긴다.
//!
//! 이 계약을 바꿀 때는 tasty-official-plugins/scripts/install-judge.sh와
//! tasty-official-plugins/scripts/check-plugin-version-bump.sh를
//! 함께 고친다. tests/strip_cfg_test_bin.rs의 계약 시험이 이름·종료 코드를 고정한다.

use std::path::{Path, PathBuf};

use tasty_doc_guards::cfg_predicate::blank_gated_lines;
use tasty_doc_guards::shipping_scope::test_only_files;
use tasty_doc_guards::source_text::neutralize_char_literal_quotes;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--check-fresh") {
        let root = args
            .get(1)
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        check_fresh(&root);
    }
    let blank_test_only = args.iter().any(|a| a == "--blank-test-only-files");
    args.retain(|a| a != "--blank-test-only-files");
    let quote_safe = args.iter().any(|a| a == "--neutralize-char-literal-quotes");
    args.retain(|a| a != "--neutralize-char-literal-quotes");
    if args.len() < 3 {
        eprintln!(
            "usage: strip-cfg-test [--blank-test-only-files] [--neutralize-char-literal-quotes] \
             <out-dir> <repo-root> <scan-root>...\n\
             test 전용인 줄을 빈 줄로 바꾼 사본을 <out-dir> 아래에 만든다.\n\
             --blank-test-only-files: `#[cfg(test)] mod x;` 로만 선언된 파일도 통째로 비운다.\n\
             --neutralize-char-literal-quotes: 문자 리터럴 안의 `\"` 를 안전한 글자로 바꾼다\n\
             (줄 수를 세는 계측기용 — 내용 동등을 묻는 소비자는 쓰면 안 된다)."
        );
        std::process::exit(2);
    }
    let out_dir = PathBuf::from(&args[0]);
    let root = PathBuf::from(&args[1]);
    if !root.is_dir() {
        eprintln!("레포 루트가 아니다: {}", root.display());
        std::process::exit(2);
    }

    let mut files = Vec::new();
    for scan in &args[2..] {
        let dir = root.join(scan);
        if !dir.is_dir() {
            eprintln!("스캔 루트가 없다: {}", dir.display());
            std::process::exit(2);
        }
        gather_rs(&dir, &mut files);
    }
    if files.is_empty() {
        eprintln!("`.rs` 를 하나도 못 찾았다 — 빈 모수는 측정 실패다");
        std::process::exit(2);
    }

    // 테스트 전용 파일 판정에는 다른 파일의 모듈 선언도 필요하다.
    let mut sources: Vec<(PathBuf, String)> = Vec::with_capacity(files.len());
    for path in &files {
        let Ok(rel) = path.strip_prefix(&root) else {
            eprintln!("스캔 결과가 루트 밖이다: {}", path.display());
            std::process::exit(2);
        };
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s.replace("\r\n", "\n"),
            Err(e) => {
                eprintln!("소스를 읽을 수 없다: {} — {e}", path.display());
                std::process::exit(2);
            }
        };
        sources.push((rel.to_path_buf(), src));
    }

    let whole_file_out = if blank_test_only {
        test_only_files(&root, &sources)
    } else {
        std::collections::BTreeSet::new()
    };

    for (rel, src) in &sources {
        let dst = out_dir.join(rel);
        if let Some(parent) = dst.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            eprintln!("사본 디렉토리를 만들 수 없다: {} — {e}", parent.display());
            std::process::exit(2);
        }
        let mut body = if whole_file_out.contains(rel) {
            blank_every_line(src)
        } else {
            blank_gated_lines(src, "test")
        };
        if quote_safe {
            body = neutralize_char_literal_quotes(&body);
        }
        if let Err(e) = std::fs::write(&dst, body) {
            eprintln!("사본을 쓸 수 없다: {} — {e}", dst.display());
            std::process::exit(2);
        }
    }
    println!("{}", sources.len());
}

/// 파일 전체를 비워도 원본의 줄 수는 유지한다.
fn blank_every_line(src: &str) -> String {
    let n = src.split('\n').count();
    "\n".repeat(n.saturating_sub(1))
}

/// target과 빌드 캐시 표식이 있는 디렉터리는 제외한다.
fn gather_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() == "target" || tasty_doc_guards::is_build_cache_dir(&path) {
                continue;
            }
            gather_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// 공통 라이브러리 지문과 함께 이 바이너리의 소스도 비교한다.
const OWN_SOURCE: &str = include_str!("strip-cfg-test.rs");
const OWN_REL: &str = "crates/tasty-doc-guards/src/bin/strip-cfg-test.rs";

/// --check-fresh: 0은 소스 일치, 1은 재빌드 필요, 3은 소스 부재로 비교할 수 없음.
fn check_fresh(root: &std::path::Path) -> ! {
    use tasty_doc_guards::freshness::{Freshness, check};
    match check(root, OWN_REL, OWN_SOURCE) {
        Freshness::Fresh => std::process::exit(0),
        Freshness::Stale(why) => {
            eprintln!("{why}");
            std::process::exit(1)
        }
        Freshness::Undecidable => std::process::exit(3),
    }
}
