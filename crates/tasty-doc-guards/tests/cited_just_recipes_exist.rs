//! Markdown에서 명령으로 인용한 just recipe가 Justfile에 있는지 확인한다.
//! 코드 펜스와 인라인 코드에서 선택적 $ 프롬프트·환경 변수 대입 뒤에 just가 오는 경우만 읽는다.
//! 영어 부사로 쓴 just와 구별하기 위한 제한이다. recipe 인자와 직접 호출한 스크립트는 검사하지 않는다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tasty_doc_guards::floored_walk::{Descend, Floor, Walked, walk_with_floor};
use tasty_doc_guards::temp_scratch::Scratch;

const JUSTFILE: &str = "Justfile";

/// 저장소의 Markdown 파일을 읽는다. 점 디렉터리와 빌드 캐시는 제외한다.
/// 로컬 전용 문서가 없는 clone에서도 같은 기준을 적용한다.
const DOC_FLOOR: Floor = Floor {
    min: 150,
    measured: 199,
    measured_on: "2026-11-08",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "acbddc755에 site/ 삭제를 더한 트리 — git ls-files로 점 디렉터리 밖 추적 Markdown 199개를 셌다.",
    ),
    why_this_gap: "추적 Markdown 199개를 기준으로 한다. 가장 큰 비-ADR 분류인 docs/features 49개만큼 여유를 둔 하한 150이다. 작은 분류 하나의 누락까지 검출하지는 못하므로 검사 범위가 바뀌면 다시 측정한다.",
};

/// recipe 수 하한. 2026-09-07 실측 16.
const MIN_RECIPES: usize = 10;
/// 명령 자리 인용 하한. 2026-09-07 실측 33.
const MIN_CITATIONS: usize = 15;

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

/// 변수 선언(:=)을 recipe로 오해하지 않도록 제외한다.
fn recipes(root: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(root.join(JUSTFILE))
        .unwrap_or_else(|e| panic!("{JUSTFILE} 을 읽지 못했다 — {e}"));
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let head = line.split('#').next().unwrap_or("");
        if head.contains(":=") {
            continue;
        }
        let Some(colon) = head.find(':') else {
            continue;
        };
        let name: String = head[..colon]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && name.starts_with(|c: char| c.is_ascii_alphabetic())
        {
            out.insert(name);
        }
    }
    out
}

/// 선택적 셸 프롬프트와 환경 변수 대입 뒤의 just 명령에서 recipe 이름을 추출한다.
fn just_target(fragment: &str) -> Option<String> {
    let mut rest = fragment.trim_start();
    if let Some(r) = rest.strip_prefix("$ ") {
        rest = r.trim_start();
    }
    loop {
        let word = rest.split_whitespace().next()?;
        if word == "just" {
            let after = rest[word.len()..].trim_start();
            let name: String = after
                .chars()
                .take_while(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_'
                })
                .collect();
            return (!name.is_empty()).then_some(name);
        }
        let is_env = word.contains('=')
            && word.split('=').next().is_some_and(|k| {
                !k.is_empty()
                    && k.chars()
                        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
            });
        if !is_env {
            return None;
        }
        rest = rest[word.len()..].trim_start();
    }
}

/// 로컬 작업 문서가 섞이지 않도록 점 디렉터리를 제외한다. 점 없는 미추적 문서는 포함될 수 있다.
fn citations(root: &Path) -> Vec<(String, usize, String)> {
    citations_under(root, &DOC_FLOOR)
}

/// 저장소와 작은 합성 트리에 같은 판독을 쓰도록 경로·하한을 인자로 받는다.
fn citations_under(root: &Path, floor: &Floor) -> Vec<(String, usize, String)> {
    let docs = walk_with_floor(
        root,
        root,
        floor,
        Descend::SkipBuildCachesAndDotDirs,
        &|w: &Walked| w.rel.ends_with(".md") && !w.rel.starts_with("target/"),
    )
    .unwrap_or_else(|e| panic!("문서 순회가 실패했다 — {e}"));

    let mut out = Vec::new();
    for w in docs {
        let Ok(text) = std::fs::read_to_string(&w.path) else {
            continue;
        };
        let mut in_fence = false;
        for (i, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence && let Some(name) = just_target(line) {
                out.push((w.rel.clone(), i + 1, name));
            }
            let mut rest = line;
            while let Some(open) = rest.find('`') {
                let after = &rest[open + 1..];
                let Some(close) = after.find('`') else { break };
                if let Some(name) = just_target(&after[..close]) {
                    out.push((w.rel.clone(), i + 1, name));
                }
                rest = &after[close + 1..];
            }
        }
    }
    out
}

#[test]
fn every_cited_just_recipe_exists() {
    let root = repo_root();
    let known = recipes(&root);
    let cited = citations(&root);
    assert!(
        known.len() >= MIN_RECIPES,
        "Justfile에서 recipe를 {}개만 읽었다(2026-09-07 실측 16). recipe 판독을 확인한다.",
        known.len()
    );
    assert!(
        cited.len() >= MIN_CITATIONS,
        "명령 자리의 `just` 인용을 {} 개만 찾았다 (2026-09-07 실측 33) — 추출이 깨졌다",
        cited.len()
    );
    let missing: Vec<String> = cited
        .iter()
        .filter(|(_, _, n)| !known.contains(n))
        .map(|(f, l, n)| format!("  {f}:{l}  just {n}"))
        .collect();
    // 미추적 문서의 오류를 저장소 문서 수정으로 오해하지 않도록 추적 여부를 알린다.
    let outside = if missing.is_empty() {
        String::new()
    } else {
        let rels: Vec<String> = cited
            .iter()
            .filter(|(_, _, n)| !known.contains(n))
            .map(|(f, _, _)| f.clone())
            .collect();
        tasty_doc_guards::tracked_scope::outside_repo_note(&root, &rels)
    };
    assert!(
        missing.is_empty(),
        "Justfile에 없는 recipe를 인용했다:\n{}\n현재 명령으로 문서를 고치거나 누락된 recipe를 복원한다.{}",
        missing.join("\n"),
        outside
    );
}

#[test]
fn the_extractor_reads_command_position_only() {
    assert_eq!(
        just_target("just build-plugins").as_deref(),
        Some("build-plugins")
    );
    assert_eq!(just_target("$ just run").as_deref(), Some("run"));
    assert_eq!(
        just_target("PROFILE=debug just build-plugins").as_deref(),
        Some("build-plugins")
    );
    assert_eq!(just_target("이것은 just the 예시다"), None);
    assert_eq!(
        just_target(r#"  --prompt "Review the diff that was just committed""#),
        None
    );
    assert_eq!(
        just_target("just build-plugin claude").as_deref(),
        Some("build-plugin")
    );
}

/// 실제 Justfile에 의존하지 않는 합성 입력으로 변수 제외와 문서 인용 수집을 함께 확인한다.
#[test]
fn the_recipe_and_citation_readers_answer_on_a_substituted_tree() {
    let probe = Scratch::new("just-citation");
    let dir = probe.path();
    std::fs::create_dir_all(dir.join("sub")).expect("합성 트리를 만들지 못했다");

    std::fs::write(
        dir.join(JUSTFILE),
        "zeta-build:\n    cargo build\n\
         omega_probe: zeta-build\n    echo hi\n\
         SIGMA := \"변수라 recipe 가 아니다\"\n\
         tau-run: # 꼬리 주석이 붙은 recipe\n    echo run\n\
         # kappa-commented: 주석 줄 전체\n\
         phi-decoy #주석 안의 콜론: 이 줄은 recipe 선언이 아니다\n\
         4nope:\n    echo 숫자로 시작하면 이름이 아니다\n",
    )
    .expect("합성 Justfile 실패");

    let got = recipes(dir);
    let mut names: Vec<&str> = got.iter().map(|s| s.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["omega_probe", "tau-run", "zeta-build"],
        "recipe 판독이 합성 Justfile 에서 다른 답을 냈다"
    );
    assert!(
        !got.contains("SIGMA"),
        "`:=` 변수 줄을 recipe 로 셌다 — 없는 recipe 인용이 통과하게 된다"
    );
    assert!(
        !got.iter().any(|n| n.starts_with("kappa")),
        "주석 줄에서 recipe 를 만들어 냈다"
    );
    assert!(
        !got.contains("phi-decoy"),
        "주석 안의 콜론을 recipe 콜론으로 읽었다 — `#` 를 안 벗긴 것이다"
    );
    assert!(
        !got.contains("4nope"),
        "숫자로 시작하는 이름을 recipe 로 셌다"
    );

    std::fs::write(
        dir.join("top.md"),
        "산문에서 just 는 부사다.\n\
         인라인은 `just zeta-build` 로 적는다.\n\
         ```sh\n\
         just omega_probe\n\
         ```\n",
    )
    .expect("합성 문서 실패");
    std::fs::write(dir.join("sub/deep.md"), "```sh\n$ just tau-run\n```\n")
        .expect("합성 하위 문서 실패");
    std::fs::write(dir.join("notes.txt"), "```sh\njust sigma-decoy\n```\n")
        .expect("합성 잡파일 실패");

    let fixture_floor = Floor {
        min: 2,
        measured: 2,
        measured_on: "2026-09-08",
        counted_on: tasty_doc_guards::floored_walk::CountedOn::SyntheticTree,
        why_this_gap: "합성 트리라 문서 수를 이 시험이 직접 정한다 — 간격이 0 인 것이 맞다.",
    };
    let cites = citations_under(dir, &fixture_floor);
    let mut seen: Vec<(&str, usize, &str)> = cites
        .iter()
        .map(|(f, l, n)| (f.as_str(), *l, n.as_str()))
        .collect();
    seen.sort_unstable();

    assert!(
        seen.iter()
            .any(|(f, l, n)| *f == "top.md" && *l == 2 && *n == "zeta-build"),
        "펜스 밖 인라인 코드 스팬의 인용을 못 읽었다(또는 줄 번호가 1 기반이 아니다): {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(f, l, n)| *f == "top.md" && *l == 4 && *n == "omega_probe"),
        "펜스 안의 인용을 못 읽었다: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(f, _, n)| *f == "sub/deep.md" && *n == "tau-run"),
        "하위 디렉토리 문서로 안 내려갔다: {seen:?}"
    );
    assert!(
        !seen.iter().any(|(_, _, n)| *n == "sigma-decoy"),
        "Markdown이 아닌 파일에서 인용을 수집했다: {seen:?}"
    );
    assert!(
        !seen.iter().any(|(_, l, _)| *l == 1),
        "산문의 just를 명령으로 판독했다: {seen:?}"
    );

    let missing: Vec<&(&str, usize, &str)> =
        seen.iter().filter(|(_, _, n)| !got.contains(*n)).collect();
    assert!(
        missing.is_empty(),
        "합성 문서의 인용과 Justfile recipe가 다르다: {missing:?}"
    );
}
