//! 출하되지 않는 줄을 **지운 사본**을 만든다 — 파일 SLOC 게이트가 잴 대상.
//!
//! ## 왜 사본인가
//!
//! 게이트의 계측기는 `tokei` 다. tokei 는 파일을 통째로 세고 "이 줄은 빼라" 를 모른다.
//! 그래서 판정(무엇이 출하되는가)과 계측(몇 줄인가)을 갈라, 판정은 이 경로가 하고 계측은
//! tokei 가 그대로 한다. **계측기를 하나 더 만들지 않으려는 것**이다 — 줄 수를 세는
//! 두 번째 구현이 생기면 게이트가 무엇을 재는지가 둘로 갈린다.
//!
//! 지운 줄은 **빈 줄로 남긴다.** 줄 번호가 보존돼야 tokei 의 보고를 원본 좌표로 읽을 수
//! 있고, 사본이 원본보다 짧아지면 "지운 결과" 와 "안 읽힌 결과" 가 구분되지 않는다.
//!
//! ## 지우는 것
//!
//! 기본은 파일 안의 인라인 `#[cfg(test)]` 범위와, 술어가 `test` 를 요구하는
//! `cfg_attr` **속성 줄**이다. 뒤쪽은 범위가 다르다 — `cfg_attr` 은 붙는 속성만
//! 조건부이고 항목 자신은 출하되므로, 속성 줄만 지운다. 파일 **전체**가 출하 밖인 경우
//! (`#[cfg(test)] mod x;` 로 선언된 별도 파일 · cargo 통합 타깃)는 기본값에서 건드리지
//! 않고, 아래 `--blank-test-only-files` 를 줄 때만 지운다 — 파일 SLOC 게이트도 그 플래그를
//! 넘긴다.
//!
//! **줄 판정 자체는 이 파일에 없다** — [`tasty_doc_guards::cfg_predicate::blank_gated_lines`]
//! 가 하고, 이 프로그램은 그 사본을 파일로 쓸 뿐이다. 소비자가 둘이기 때문이다:
//! `src/source_guards/headless_app_layer_coverage.rs` 가 "명부 밖에 이름이 사는가" 의
//! 좌변을 만들 때 같은 판정을 **함수로** 부른다(그쪽은 사본 파일이 필요 없다).
//! 그 가드가 자기 판정을 따로 두면 같은 물음의 답이 둘로 갈리고, 갈린 쪽은 조용하다.
//!
//! **그 문장은 *선언된 파일* 의 몫만 말한다 — 선언 줄 자신은 다르다.** 부모 파일에 남는
//! `#[cfg(test)]` 속성 줄과 바로 다음의 `mod x;` 선언 줄은 인라인 범위로 잡혀 **기본
//! 동작에서 공백화된다**(`src/lib.rs` 의 `#[cfg(test)] mod design_token_guard;` 가 그
//! 자리다 — 사본에서 두 줄 다 빈 줄이다). 그래서 한 모듈을 테스트 전용으로 돌릴 때
//! 그 크레이트에서 줄어드는 몫은 옮겨간 파일의 본문만이 아니라 **부모의 두 줄까지**다.
//! 이 두 줄을 빼먹으면 이동 전후의 SLOC 차가 안 맞고, 그 차이는 파일마다 달라 상수로
//! 보정되지 않는다.
//!
//! 아래 "빈 줄에 주의" 가 세는 `#[cfg(test)] mod x;` 두 줄이 바로 이것이다 — 그 절은
//! 이 사실을 이미 전제하고 있었고, 여기에 적기 전까지 두 절을 잇는 문장이 없었다.
//!
//! `--blank-test-only-files` 를 주면 그 축까지 지운다. 소비자가 둘로 늘었고 둘의 물음이
//! 다르기 때문이다 — SLOC 게이트는 "이 파일이 몇 줄인가" 를 묻고, plugin 버전 게이트는
//! "이 크레이트의 **산출물**이 달라졌는가" 를 묻는다. 뒤쪽에서는 전체-테스트 파일이
//! 통째로 산출물 밖이라 그 차이를 세면 안 된다. 판정 자체는 하나다 —
//! [`tasty_doc_guards::shipping_scope::test_only_files`] 를 부를 뿐 여기서 다시 세지 않는다.
//!
//! ## 계측기가 읽을 수 있는 사본 — `--neutralize-char-literal-quotes`
//!
//! 이 사본을 세는 것은 `tokei` 이고, 그 계측기는 문자 리터럴 `'"'` 의 따옴표를
//! **문자열의 시작**으로 읽는다. 그러면 그 뒤 파일 끝까지가 문자열 안으로 보이고,
//! 문자열 안의 빈 줄은 code 로 세어진다 — 즉 **지운 줄이 다시 세어진다.** 실측
//! (tokei 14.0.0, 2026-09-09): `src/core/attach_runtime.rs` 에 그 형태 한 자리가
//! 들어오자 출하 SLOC 이 1240 → 3046 이 됐는데 같은 구간의 원시 순증은 +364 였다.
//!
//! 플래그를 주면 그 자리만 안전한 글자로 바꾼 사본을 만든다
//! ([`tasty_doc_guards::source_text::neutralize_char_literal_quotes`]). 리터럴 **안의
//! 한 글자**만 바뀌므로 줄 수도 code 수도 안 움직인다.
//!
//! **기본값이 아닌 이유**: 내용 동등을 묻는 소비자(plugin 버전 게이트)에게는
//! `'"'` 와 `'x'` 가 같아 보인다. 그 게이트가 놓치면 산출물이 달라졌는데 bump 를
//! 안 요구한다. 그래서 세는 소비자만 켠다.
//!
//! ## 빈 줄에 주의 — 소비자에 따라 접어야 한다
//!
//! 아래 "줄 번호 보존" 때문에 지운 자리는 빈 줄로 남는다. **내용 동등을 묻는 소비자는
//! 비교 전에 빈 줄을 접어야 한다.** 안 접으면 `#[cfg(test)] mod x;` 두 줄이 는 것이
//! "내용이 달라졌다" 로 읽힌다(실측으로 밟았다).
//!
//! ## 종료코드
//!
//! 0 = 사본을 만들었다. 2 = 만들지 못했다(인자 부족·루트 없음·파일 0 개).
//! **1 은 쓰지 않는다** — 이 프로그램은 위반을 판정하지 않는다. 파일 0 개를 0 으로
//! 돌려주면 게이트가 빈 모수를 재고 조용히 초록이 된다.

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
             출하되지 않는 줄을 빈 줄로 바꾼 사본을 <out-dir> 아래에 만든다.\n\
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

    // 읽기를 판정보다 먼저 끝낸다 — 전체-테스트 파일 판정은 **다른 파일의 선언**을
    // 봐야 하므로 파일 하나씩 훑는 순회로는 답이 안 나온다.
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

/// 파일 전체가 출하 밖일 때. 줄 수는 그대로 남긴다 — 위 "줄 번호 보존" 과 같은 이유다.
fn blank_every_line(src: &str) -> String {
    let n = src.split('\n').count();
    "\n".repeat(n.saturating_sub(1))
}

/// 빌드 캐시는 세지 않는다 — 이름이 아니라 표식으로 가른다(`CARGO_TARGET_DIR` 로 다른
/// 이름을 주면 이름 가지치기는 통째로 새어 들어간다).
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

/// 이 파일의 내용. 게이트가 "이 판정기가 지금 소스로 지어졌나" 를 물을 때 라이브러리
/// 지문과 함께 대조한다 — 자세한 이유는 [`tasty_doc_guards::freshness`].
const OWN_SOURCE: &str = include_str!("strip-cfg-test.rs");
const OWN_REL: &str = "crates/tasty-doc-guards/src/bin/strip-cfg-test.rs";

/// `--check-fresh <repo-root>`: 0 = 지금 소스로 지어졌다, 1 = 낡았다, 3 = 물을 수 없다.
/// **1 과 3 을 가른다** — 소스가 없는 트리(배포 tarball · 합성 픽스처)에서 정상 상황이
/// 경고가 되면 안 된다.
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
