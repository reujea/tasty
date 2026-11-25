//! 워크플로의 push 트리거와 경로 필터, 자동 실행 대상으로 분류한 잡을 읽는다.
//! 주석의 키 이름을 실제 설정으로 세지 않도록 블록과 들여쓰기를 구분한다.
//! 실제 push에서 무엇이 실행됐는지는 판단하지 않는다. 실행 기록 확인은 docs/dev-guide/ci-gates.md를 따른다.

/// 한 워크플로의 `push:` 트리거 모양.
#[derive(Debug, PartialEq, Eq)]
pub struct PushTrigger {
    /// `on:` 블록에 `push:` 가 있는가.
    pub present: bool,
    /// 그 `push:` 아래에 `paths:` 나 `paths-ignore:` 가 붙었는가.
    pub path_filtered: bool,
    /// 태그 조건만 있고 브랜치 조건은 없는지 확인한다. 경로 필터가 없어도 태그 전용이면 일반 push 채널이 아니다.
    pub tags_only: bool,
}

/// 줄 첫 토큰이거나 공백 뒤인 #부터 주석으로 처리한다. 따옴표 안의 #는 별도로 해석하지 않는 단순 판독이다.
fn strip_yaml_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let bytes = line.as_bytes();
        let mut cut = line.len();
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
                cut = i;
                break;
            }
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// 최상위 on 블록부터 다음 최상위 키 직전까지 읽는다.
/// on: [push] 같은 인라인 표기는 지원하지 않는다. 이 경우 None을 판독 실패로 처리해야 한다.
fn on_block(text: &str) -> Option<Vec<&str>> {
    let mut lines = text.lines();
    // on 선언 다음 줄부터 읽는다. 선언이 없으면 None이다.
    lines.by_ref().find(|l| {
        let t = l.trim_end();
        t == "on:" || t == "\"on\":" || t == "'on':"
    })?;
    let mut block = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        // 다음 최상위 키에서 끝난다.
        if !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        block.push(line);
    }
    Some(block)
}

/// 한 줄의 들여쓰기 폭(공백 수).
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// push의 존재·경로 필터·태그 전용 여부를 읽는다. None은 지원하지 않는 on 표기이며 필터 없음과 다르다.
pub fn push_trigger(text: &str) -> Option<PushTrigger> {
    let stripped = strip_yaml_comments(text);
    let block = on_block(&stripped)?;
    let Some(at) = block
        .iter()
        .position(|l| indent(l) == 2 && l.trim_end().trim_start() == "push:")
    else {
        return Some(PushTrigger {
            present: false,
            path_filtered: false,
            tags_only: false,
        });
    };
    let mut path_filtered = false;
    let mut has_tags = false;
    let mut has_branches = false;
    for line in &block[at + 1..] {
        // 다음 트리거(같은 깊이)에서 끝난다.
        if indent(line) <= 2 {
            break;
        }
        let key = line.trim_start();
        if key.starts_with("paths:") || key.starts_with("paths-ignore:") {
            path_filtered = true;
        }
        if key.starts_with("tags:") || key.starts_with("tags-ignore:") {
            has_tags = true;
        }
        if key.starts_with("branches:") || key.starts_with("branches-ignore:") {
            has_branches = true;
        }
    }
    Some(PushTrigger {
        present: true,
        path_filtered,
        tags_only: has_tags && !has_branches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_mentioning_a_filter_is_not_a_filter() {
        let yaml = "\
# crossplatform-check.yml 은 paths-ignore: docs/** 뒤에 있다.
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  x:
";
        assert_eq!(
            push_trigger(yaml),
            Some(PushTrigger {
                present: true,
                path_filtered: false,
                tags_only: false
            })
        );
    }

    #[test]
    fn a_real_filter_under_push_is_seen() {
        let yaml = "\
on:
  push:
    branches: [main]
    paths-ignore:
      - 'docs/**'
  pull_request:
    branches: [main]
";
        assert!(push_trigger(yaml).unwrap().path_filtered);
    }

    #[test]
    fn a_filter_on_another_trigger_does_not_leak_into_push() {
        let yaml = "\
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
    paths:
      - 'src/**'
";
        assert_eq!(
            push_trigger(yaml),
            Some(PushTrigger {
                present: true,
                path_filtered: false,
                tags_only: false
            })
        );
    }

    #[test]
    fn a_workflow_without_push_says_so() {
        let yaml = "on:\n  workflow_dispatch:\n";
        assert_eq!(
            push_trigger(yaml),
            Some(PushTrigger {
                present: false,
                path_filtered: false,
                tags_only: false
            })
        );
    }

    #[test]
    fn a_tags_only_push_is_not_an_every_push_channel() {
        let yaml = "on:\n  push:\n    tags: ['v*']\n  workflow_dispatch:\n";
        let t = push_trigger(yaml).unwrap();
        assert!(t.present && !t.path_filtered && t.tags_only);
    }

    #[test]
    fn a_push_naming_branches_is_not_tags_only() {
        let yaml = "on:\n  push:\n    branches: [main]\n    tags: ['v*']\n";
        assert!(!push_trigger(yaml).unwrap().tags_only);
    }

    #[test]
    fn an_inline_trigger_is_unreadable_rather_than_unfiltered() {
        assert_eq!(push_trigger("on: [push]\njobs:\n"), None);
    }

    #[test]
    fn a_hash_inside_a_value_is_not_a_comment() {
        let yaml = "\
on:
  push:
    branches: [main]
    paths:
      - 'a#b/**'
";
        assert!(push_trigger(yaml).unwrap().path_filtered);
    }
}

/// 경로 필터 없는 push 채널이 실행하도록 설정된 테스트 범위.
/// 자동 트리거가 하나라도 있는지 묻는 검사와 다르며 태그 전용·경로 제한 채널은 포함하지 않는다.
#[derive(Debug, Default)]
pub struct FilterFreeCoverage {
    /// `--test <이름>` 으로 지목된 타깃 이름들.
    pub named: std::collections::BTreeSet<String>,
    /// -p/--package로 지정됐고 타깃 종류를 추가로 좁히지 않은 패키지.
    pub packages: std::collections::BTreeSet<String>,
    /// `--workspace` 를 좁힘 없이 부르는 잡이 있는가 — 있으면 전부 덮인다.
    pub whole_workspace: bool,
}

/// 호출을 특정 타깃/종류로 좁히는 플래그.
const NARROWING: &[&str] = &["--test", "--lib", "--bins", "--bin", "--doc", "--example"];

/// cargo test 호출이 특정 타깃이나 종류로 제한되는지 확인한다.
fn narrows(inv: &str) -> bool {
    inv.split_whitespace().any(|w| NARROWING.contains(&w))
}

/// 주석·스텝 이름을 제외하고 명령을 한 줄로 합친다.
/// 여러 줄 run 블록·접힌 스칼라·역슬래시 줄 이음의 뒤쪽 플래그도 읽기 위해 사용한다.
fn flatten(yaml: &str) -> String {
    yaml.replace("\r\n", "\n")
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with('#') && !t.starts_with("- name:") && !t.starts_with("name:")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 잡 하나 — 헤더에 적힌 이름과, 그 헤더부터 다음 헤더 직전까지의 본문.
#[derive(Debug, Clone)]
pub struct JobSpan {
    /// `jobs:` 아래 2 칸 헤더에 적힌 이름(뒤의 `:` 는 뗐다).
    pub name: String,
    /// 헤더 줄을 **포함한** 원문 본문. 다음 잡 머리 직전이나 `jobs:` 블록 끝(들여쓰기 없는 다음 줄)에서 끝난다.
    /// 주석이 그대로 남아 있다 — 아래 술어 설명 참조.
    pub body: String,
}

/// 공용 판독([`crate::workflow_yaml::parse`])으로 잡을 나눈다. 구조가 지원 밖 표기면 Err(파일:행 오류 줄)다.
fn parsed_jobs(file: &str, yaml: &str) -> Result<Vec<crate::workflow_yaml::Job>, Vec<String>> {
    let (jobs, errs) = crate::workflow_yaml::parse(file, yaml);
    if errs.is_empty() { Ok(jobs) } else { Err(errs) }
}

/// jobs 아래의 잡 이름과 본문을 나눈다. 자동/수동 분류는 별도로 한다.
/// 잡·스텝 구조는 [`crate::workflow_yaml`]의 좁은 표기만 받고, 그 밖의 표기는 Err로 돌려준다.
/// 본문은 주석을 포함한 원문이다. jobs와 첫 헤더 사이의 내용은 어느 잡에도 포함하지 않는다.
pub fn job_spans(file: &str, yaml: &str) -> Result<Vec<JobSpan>, Vec<String>> {
    Ok(parsed_jobs(file, yaml)?
        .into_iter()
        .map(|j| JobSpan {
            name: j.name,
            body: j.raw.iter().fold(String::new(), |mut acc, (_, l)| {
                acc.push_str(l);
                acc.push('\n');
                acc
            }),
        })
        .collect())
}

/// 자동·수동을 구분하지 않은 잡 이름들. 소비자는 파일별 수집 결과도 확인해야 한다.
pub fn job_headers(file: &str, yaml: &str) -> Result<Vec<String>, Vec<String>> {
    Ok(job_spans(file, yaml)?.into_iter().map(|j| j.name).collect())
}

/// [`job_headers`] 의 수. 이름이 필요 없는 자리에서 쓴다.
pub fn job_header_count(file: &str, yaml: &str) -> Result<usize, Vec<String>> {
    Ok(job_spans(file, yaml)?.len())
}

/// workflow_dispatch 조건 문자열이 있으면 수동 전용으로 분류한다.
/// 조건식을 평가하지 않아 OR로 연결된 자동 실행 조건이나 주석도 잘못 제외할 수 있다.
/// 자동 실행 채널 판정의 다른 소비자도 같은 제한을 공유한다.
fn is_manual_only(body: &str) -> bool {
    body.contains("github.event_name == 'workflow_dispatch'")
}

/// 수동 전용으로 분류한 잡을 제외한 이름들. 실제 실행 여부를 보장하지 않는다.
pub fn automatic_job_names(file: &str, yaml: &str) -> Result<Vec<String>, Vec<String>> {
    Ok(job_spans(file, yaml)?
        .into_iter()
        .filter(|j| !is_manual_only(&j.body))
        .map(|j| j.name)
        .collect())
}

/// 수동 전용으로 분류한 잡을 제외한 본문들. 트리거·경로 필터는 push_trigger에서 따로 확인한다.
/// Rust 밖에서도 같은 판독을 사용하려면 workflow-channels 바이너리를 호출한다.
pub fn automatic_job_bodies(file: &str, yaml: &str) -> Result<Vec<String>, Vec<String>> {
    Ok(job_spans(file, yaml)?
        .into_iter()
        .filter(|j| !is_manual_only(&j.body))
        .map(|j| j.body)
        .collect())
}

/// 평탄화된 본문에서 `cargo test` 호출을 하나씩 잘라낸다. 각 조각은 다음 `cargo ` 직전
/// 까지라, 한 스텝에 명령이 여럿이어도 플래그가 섞이지 않는다.
fn cargo_test_invocations(flat: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = flat[from..].find("cargo test") {
        let start = from + rel;
        let rest = &flat[start + "cargo test".len()..];
        let end = rest
            .find("cargo ")
            .map_or(flat.len(), |n| start + "cargo test".len() + n);
        out.push(&flat[start..end]);
        from = start + "cargo test".len();
    }
    out
}

/// 워크플로 디렉터리에서 경로 필터 없는 push 채널의 검사 범위를 읽는다. 판독하지 못한 파일은 Err로 반환한다.
/// Err 항목은 디렉터리 IO(`<디렉터리>: <오류>`), 파일 IO(`<파일>: 읽지 못했다: <오류>`), `on:` 판독 실패
/// (`<파일>: on: 을 못 읽었다`), 잡 구조 판독 실패(`<파일>:<행> 지원하지 않는 표기: …`)다. 잡 구조는 채널 선별 전에
/// 모든 파일에서 보고, 한 파일에서 `on:`과 잡 구조가 함께 실패하면 둘 다 낸다.
pub fn filter_free_coverage(
    workflows: &std::path::Path,
) -> Result<FilterFreeCoverage, Vec<String>> {
    let mut out = FilterFreeCoverage::default();
    let mut unreadable = Vec::new();
    let entries = match std::fs::read_dir(workflows) {
        Ok(e) => e,
        Err(e) => return Err(vec![format!("{}: {e}", workflows.display())]),
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .collect();
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                unreadable.push(format!("{name}: 읽지 못했다: {e}"));
                continue;
            }
        };
        let trigger = push_trigger(&text);
        if trigger.is_none() {
            unreadable.push(format!("{name}: on: 을 못 읽었다"));
        }
        let bodies = match automatic_job_bodies(&name, &text) {
            Ok(b) => b,
            Err(errs) => {
                unreadable.extend(errs);
                continue;
            }
        };
        let Some(trigger) = trigger else {
            continue;
        };
        if !trigger.present || trigger.path_filtered || trigger.tags_only {
            continue;
        }
        for body in bodies {
            let flat = flatten(&body);
            for inv in cargo_test_invocations(&flat) {
                let words: Vec<&str> = inv.split_whitespace().collect();
                for w in words.windows(2) {
                    if w[0] == "--test" {
                        out.named.insert(w[1].to_string());
                    }
                }
                if narrows(inv) {
                    continue;
                }
                for w in words.windows(2) {
                    if w[0] == "-p" || w[0] == "--package" {
                        out.packages.insert(w[1].to_string());
                    }
                }
                if words.contains(&"--workspace") {
                    out.whole_workspace = true;
                }
            }
        }
    }
    if unreadable.is_empty() {
        Ok(out)
    } else {
        Err(unreadable)
    }
}

impl FilterFreeCoverage {
    /// 이 타깃이 필터 없는 채널에 덮이는가. `stem` 은 `--test` 에 쓰는 타깃 이름,
    /// `package` 는 그 타깃이 속한 패키지 이름이다.
    pub fn covers(&self, stem: &str, package: &str) -> bool {
        self.whole_workspace || self.named.contains(stem) || self.packages.contains(package)
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn coverage() -> FilterFreeCoverage {
        filter_free_coverage(&crate::repo_root().join(".github/workflows"))
            .unwrap_or_else(|bad| panic!("판독하지 못한 워크플로: {bad:?}"))
    }

    /// 전체 결과가 비면 판독 실패나 채널 삭제를 확인한다. named와 packages 중 한쪽만 빈 것은 정상일 수 있다.
    #[test]
    fn the_reader_is_not_silently_empty() {
        let c = coverage();
        assert!(
            !c.named.is_empty() || !c.packages.is_empty() || c.whole_workspace,
            "필터 없는 채널을 하나도 못 읽었다 — 판독이 깨졌거나 채널이 전부 사라졌다"
        );
    }

    #[test]
    fn a_folded_scalar_narrowing_is_seen() {
        let c = coverage();
        assert!(
            c.named.contains("changelog_unreleased"),
            "접힌 스칼라 안의 `--test changelog_unreleased` 를 못 읽었다: {:?}",
            c.named
        );
        assert!(
            !c.packages.contains("tasty"),
            "좁혀진 호출을 패키지 전체 채널로 셌다: {:?}",
            c.packages
        );
    }

    /// 자동·수동 잡을 같은 파일에 두어 수동 조건이 다른 잡의 명령까지 제외하지 않는지 확인한다.
    #[test]
    fn two_jobs_split_and_a_manual_one_does_not_silence_the_automatic_one() {
        let yaml = "on:\n  push:\n    branches: [main]\njobs:\n  auto:\n    steps:\n      \
                    - run: cargo test -p alpha\n  manual:\n    if: github.event_name == \
                    'workflow_dispatch'\n    steps:\n      - run: cargo test -p beta\n";
        let bodies = automatic_job_bodies("fixture.yml", yaml).unwrap();
        assert_eq!(
            bodies.len(),
            1,
            "자동 잡 하나만 남아야 한다. 잡 헤더와 수동 조건의 분류를 확인한다: {bodies:?}"
        );
        assert!(
            bodies[0].contains("cargo test -p alpha"),
            "자동 잡의 명령이 사라졌다: {bodies:?}"
        );
        assert!(
            !bodies.iter().any(|b| b.contains("cargo test -p beta")),
            "수동 전용 잡의 명령이 자동 채널로 새어 들어왔다: {bodies:?}"
        );
    }

    /// 잡 헤더를 닮은 주석은 헤더에서 제외하되 반환 본문에는 남겨야 한다.
    #[test]
    fn a_comment_shaped_like_a_job_header_is_not_a_job() {
        let yaml = "on:\n  push:\n    branches: [main]\njobs:\n  alpha:\n    steps:\n      \
                    - run: cargo test -p alpha\n\n  # 아래 잡이 무엇을 닫는지 적는다:\n  \
                    #  - 하나\n  beta:\n    steps:\n      - run: cargo test -p beta\n";
        let bodies = automatic_job_bodies("fixture.yml", yaml).unwrap();
        assert_eq!(
            bodies.len(),
            2,
            "잡은 둘인데 {}개로 읽었다. 주석을 헤더로 읽지 않았는지 확인한다: {bodies:?}",
            bodies.len()
        );
        assert!(
            bodies.iter().any(|b| b.contains("아래 잡이 무엇을 닫는지")),
            "잡 본문에서 주석이 사라졌다. 소비자가 원문을 검사할 수 있어야 한다: {bodies:?}"
        );
        assert_eq!(
            job_header_count("fixture.yml", yaml).unwrap(),
            2,
            "잡 헤더 수와 본문 수의 분류 기준이 달라졌다."
        );
    }

    /// 잡이 여럿인 실제 워크플로에서도 파일 수보다 많은 잡 본문을 읽는지 확인한다.
    #[test]
    fn the_repo_has_more_automatic_job_bodies_than_workflow_files() {
        let dir = crate::repo_root().join(".github/workflows");
        let mut files = 0usize;
        let mut bodies = 0usize;
        for e in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .flatten()
        {
            let p = e.path();
            if !p.extension().is_some_and(|x| x == "yml" || x == "yaml") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            files += 1;
            bodies += automatic_job_bodies(&p.display().to_string(), &text)
                .unwrap_or_else(|e| panic!("{}", e.join("\n")))
                .len();
        }
        assert!(
            files >= 8,
            "워크플로를 {files} 개밖에 못 읽었다 — 모수가 깨졌다"
        );
        assert!(
            bodies > files,
            "잡 본문 {bodies}개가 워크플로 {files}개 이하다. 잡이 여럿인 파일의 헤더를 읽었는지 확인한다."
        );
    }

    /// 등록된 패키지와 등록되지 않은 타깃을 함께 확인한다.
    #[test]
    fn the_coverage_answers_both_yes_and_no() {
        let c = coverage();
        assert!(
            c.covers("no_checkbox_in_docs", "tasty-doc-guards"),
            "`tasty-doc-guards` 가 필터 없는 채널에 안 덮인다: {c:?}"
        );
        assert!(
            !c.covers("a_target_no_workflow_names", "tasty-plugin-mesh-demo"),
            "이 타깃은 지목되지 않았고 패키지 전체 실행도 없는데 포함된 것으로 보고했다. 새 --workspace 실행이 생겼거나 판독 범위가 잘못됐는지 확인한다: {c:?}"
        );
    }

    /// 각 제한 플래그를 독립된 입력으로 검사한다. 목록에서 입력을 생성하면 같은 오타를 공유할 수 있다.
    #[test]
    fn every_narrowing_flag_is_actually_read_as_narrowing() {
        let cases: [(&str, &str); 6] = [
            ("--test", "cargo test --workspace --locked --test e2e_tests"),
            ("--lib", "cargo test --workspace --locked --lib"),
            ("--bins", "cargo test --workspace --locked --bins"),
            ("--bin", "cargo test --workspace --locked --bin tasty"),
            ("--doc", "cargo test --workspace --locked --doc"),
            (
                "--example",
                "cargo test --workspace --locked --example demo",
            ),
        ];
        for (flag, inv) in cases {
            assert!(
                narrows(inv),
                "`{flag}`로 타깃을 제한한 호출을 전체 실행으로 읽었다. 제한 플래그 목록과 철자를 확인한다."
            );
        }
    }

    #[test]
    fn an_unnarrowed_invocation_is_not_read_as_narrowed() {
        assert!(
            !narrows("cargo test --workspace --locked --no-fail-fast"),
            "타깃 제한 플래그가 없는 호출을 제한된 실행으로 읽었다."
        );
        assert!(
            !narrows("cargo test --workspace --locked -p tasty-doc-guards"),
            "`-p` 는 패키지 선택이지 타깃 좁힘이 아니다 — 좁힘으로 읽으면 그 패키지를 \
             통째로 도는 잡이 커버리지에서 빠진다"
        );
    }
}

// 워크플로 스텝 판독.

/// 워크플로 잡 안의 스텝. 잡 결과와 각 검증 스텝의 실제 실행 여부는 다를 수 있다.
/// 이 타입은 선언을 읽으며 원격 실행 결과를 조회하지 않는다.
#[derive(Debug, Clone)]
pub struct Step {
    /// 이 스텝이 속한 잡 이름.
    pub job: String,
    /// 표시용 이름. `name` 값, 없으면 `uses` 값, 둘 다 없으면 `(이름 없음)`이다. 판정에 쓰지 않는다.
    pub name: String,
    /// 스텝 첫 줄(`- `가 있는 줄)의 파일 기준 줄 번호(1-based).
    pub line: usize,
    /// 잡 안 순번(1-based).
    pub ordinal: usize,
    /// 명시적인 if 조건이 있는지. 조건식의 의미나 앞 단계 실패 후 실행 여부는 평가하지 않는다.
    pub has_if: bool,
    /// `continue-on-error: true` 인가. 붙으면 이 스텝의 실패가 잡 결론에 안 들어간다.
    pub continue_on_error: bool,
    /// `name` 필드를 뺀 스텝 줄들(주석을 지웠다). 이름은 실행문이 아니므로 넣지 않는다.
    pub body: String,
}

/// 표시용 이름. 좁은 한 줄 값으로 읽히면 그 값, 아니면 주석을 지운 같은 줄 글자다.
fn display_value(file: &str, step: &crate::workflow_yaml::Step, key: &str) -> Option<String> {
    let f = step.fields.iter().find(|f| f.key == key)?;
    Some(
        crate::workflow_yaml::scalar(file, &step.fields, key)
            .ok()
            .flatten()
            .unwrap_or_else(|| strip_yaml_comments(&f.inline).trim().to_string()),
    )
}

/// 잡의 스텝들을 읽는다. 잡·스텝 구조는 [`crate::workflow_yaml`]의 좁은 표기이고(스텝은 6칸의 모든 `- `에서
/// 시작한다), 스텝마다 `if`·`continue-on-error`를 [`crate::workflow_yaml::step_conditions`]로 읽는다.
/// 지원 밖 표기가 하나라도 있으면 Err(파일:행 오류 줄)다. 소비자는 스텝을 하나도 읽지 못한 결과도 확인해야 한다.
pub fn job_steps(file: &str, yaml: &str) -> Result<Vec<Step>, Vec<String>> {
    let jobs = parsed_jobs(file, yaml)?;
    let mut out = Vec::new();
    let mut errs = Vec::new();
    for job in &jobs {
        for (k, s) in job.steps.iter().enumerate() {
            let cond = match crate::workflow_yaml::step_conditions(file, s) {
                Ok(c) => c,
                Err(e) => {
                    errs.extend(e);
                    continue;
                }
            };
            let name_lines: Vec<usize> = s
                .fields
                .iter()
                .filter(|f| f.key == "name")
                .flat_map(|f| std::iter::once(f.line).chain(f.children.iter().map(|(n, _)| *n)))
                .collect();
            let kept: String = s
                .lines
                .iter()
                .filter(|(n, _)| !name_lines.contains(n))
                .fold(String::new(), |mut acc, (_, l)| {
                    acc.push_str(l);
                    acc.push('\n');
                    acc
                });
            out.push(Step {
                job: job.name.clone(),
                name: display_value(file, s, "name")
                    .or_else(|| display_value(file, s, "uses"))
                    .unwrap_or_else(|| "(이름 없음)".to_string()),
                line: s.line,
                ordinal: k + 1,
                has_if: cond.if_expr.is_some(),
                continue_on_error: cond.continue_on_error == Some(true),
                body: strip_yaml_comments(&kept),
            });
        }
    }
    if errs.is_empty() { Ok(out) } else { Err(errs) }
}

/// 이 스텝이 검증을 돌리는가 — `cargo test` 또는 `cargo clippy`.
fn runs_verification(step: &Step) -> bool {
    let flat = step.body.replace('\n', " ");
    let mut words = flat.split_whitespace().peekable();
    while let Some(w) = words.next() {
        if w == "cargo" && matches!(words.peek(), Some(&"test") | Some(&"clippy")) {
            return true;
        }
    }
    false
}

/// 앞 스텝이 있고 명시적인 if·continue-on-error가 없는 cargo test/clippy 스텝을 찾는다.
/// 앞 스텝 실패로 실행되지 않을 수 있는 검증을 분류한다. 실패를 무시하는 스텝은 별도로 다룬다.
pub fn swallowable_verification_steps(file: &str, yaml: &str) -> Result<Vec<Step>, Vec<String>> {
    let steps = job_steps(file, yaml)?;
    let mut out = Vec::new();
    for (k, s) in steps.iter().enumerate() {
        if !runs_verification(s) || s.has_if || s.continue_on_error {
            continue;
        }
        if steps[..k].iter().any(|p| p.job == s.job) {
            out.push(s.clone());
        }
    }
    Ok(out)
}

/// 명시적인 if가 있는 검증 스텝을 따로 분류한다. 조건식 자체의 의미는 평가하지 않는다.
/// 정리·배포처럼 cargo test/clippy를 실행하지 않는 스텝은 포함하지 않는다.
pub fn protected_verification_steps(file: &str, yaml: &str) -> Result<Vec<Step>, Vec<String>> {
    Ok(job_steps(file, yaml)?
        .into_iter()
        .filter(|s| s.has_if && runs_verification(s))
        .collect())
}

#[cfg(test)]
mod step_tests {
    use super::*;

    /// 워크플로에서 생성하지 않은 합성 입력으로 각 조건을 독립적으로 확인한다.
    const FIXTURE: &str = r#"
name: fixture
on:
  push:
jobs:
  first:
    steps:
      - uses: actions/checkout@v4
      - name: cargo check
        run: cargo check --workspace --locked
      - name: swallowable
        run: cargo test --workspace --locked
      - name: protected
        if: ${{ !cancelled() }}
        run: cargo test --workspace --lib
      - name: swallowed failure is invisible
        continue-on-error: true
        run: cargo test --workspace --doc
  second:
    steps:
      - name: the first step of its job
        run: cargo test -p something
"#;

    #[test]
    fn the_step_reader_finds_steps_at_all() {
        let steps = job_steps("fixture.yml", FIXTURE).unwrap();
        assert_eq!(
            steps.len(),
            6,
            "픽스처에서 스텝을 {}개 읽었다(기대 6). 지원하는 헤더 들여쓰기를 확인한다.",
            steps.len()
        );
        let jobs: Vec<&str> = steps.iter().map(|s| s.job.as_str()).collect();
        assert!(
            jobs.contains(&"first") && jobs.contains(&"second"),
            "스텝의 잡 소속이 잘못됐다: {jobs:?}. 다른 잡의 선행 스텝을 섞지 않아야 한다."
        );
    }

    #[test]
    fn only_a_step_a_dead_neighbour_can_swallow_is_counted() {
        let got: Vec<String> = swallowable_verification_steps("fixture.yml", FIXTURE)
            .unwrap()
            .iter()
            .map(|s| s.name.trim().to_string())
            .collect();
        assert_eq!(
            got,
            vec!["swallowable".to_string()],
            "분류 결과가 다르다: {got:?}\n기대값은 swallowable 하나다. protected는 if가 있고, continue-on-error는 별도 분류이며, second는 잡의 첫 스텝이다."
        );
    }

    /// 두 분류가 같은 스텝을 중복 포함하지 않는지 확인한다.
    #[test]
    fn the_protected_and_the_swallowable_never_hold_the_same_step() {
        let prot: Vec<String> = protected_verification_steps("fixture.yml", FIXTURE)
            .unwrap()
            .iter()
            .map(|s| s.name.trim().to_string())
            .collect();
        assert_eq!(
            prot,
            vec!["protected".to_string()],
            "짝 술어가 고르는 것이 달라졌다: {prot:?} — 기대는 `protected` 하나다"
        );
        let swal: Vec<String> = swallowable_verification_steps("fixture.yml", FIXTURE)
            .unwrap()
            .iter()
            .map(|s| s.name.trim().to_string())
            .collect();
        for p in &prot {
            assert!(
                !swal.contains(p),
                "`{p}` 가 두 술어에 다 들어간다 — 두 수를 함께 읽는 전제가 깨진다"
            );
        }
    }

    /// 주석에서 다른 스텝의 cargo test를 설명해도 실제 명령으로 읽지 않는다.
    #[test]
    fn a_cargo_invocation_that_lives_only_in_a_comment_is_not_a_step_that_runs_it() {
        const COMMENTED: &str = r#"
jobs:
  only:
    steps:
      - uses: actions/checkout@v4
      - name: cargo check
        run: cargo check --locked
      - name: not a verification step
        run: echo hello
      # 아래 스텝은 cargo test --workspace 를 돌린다 — 이 줄은 주석이다
      - name: the real one
        run: cargo test --workspace
"#;
        let picked: Vec<String> = swallowable_verification_steps("fixture.yml", COMMENTED)
            .unwrap()
            .iter()
            .map(|s| s.name.trim().to_string())
            .collect();
        assert_eq!(
            picked,
            vec!["the real one".to_string()],
            "주석 안의 cargo test를 실제 명령으로 읽었다: {picked:?}. 실행문만 검사해야 한다."
        );
    }
}

/// 공용 판독(`workflow_yaml`)으로 바꾼 잡·스텝 경계와 지원 밖 표기 거절을 고정한다.
#[cfg(test)]
mod shared_reader_tests {
    use super::*;

    /// 이름 없는 스텝(`- run:`·`- id:`로 시작)은 앞 스텝에 합쳐지지 않고 제 순번을 받는다.
    #[test]
    fn a_nameless_step_is_its_own_step() {
        let yaml = "jobs:\n  j:\n    steps:\n      - name: a\n        run: echo\n      \
                    - run: cargo test --workspace\n      - id: x\n        run: cargo clippy\n";
        let steps = job_steps("fixture.yml", yaml).unwrap_or_else(|e| panic!("{e:?}"));
        let got: Vec<(usize, usize)> = steps.iter().map(|s| (s.ordinal, s.line)).collect();
        assert_eq!(
            got,
            vec![(1, 4), (2, 6), (3, 7)],
            "이름 없는 스텝이 제 스텝으로 읽히지 않았다(순번, 행): {steps:?}"
        );
        let swallowable: Vec<usize> = swallowable_verification_steps("fixture.yml", yaml)
            .unwrap_or_else(|e| panic!("{e:?}"))
            .iter()
            .map(|s| s.ordinal)
            .collect();
        assert_eq!(
            swallowable,
            vec![2, 3],
            "이름 없는 두 검증 스텝을 앞 스텝 뒤의 검증으로 세지 않았다"
        );
    }

    fn assert_rejected<T: std::fmt::Debug>(got: Result<T, Vec<String>>, want: &str) {
        match got {
            Ok(v) => panic!("지원 밖 표기를 받아들였다: {v:?}"),
            Err(errs) => assert!(
                errs.iter().any(|e| e.contains(want)),
                "`{want}`를 기대했다. 실제: {errs:?}"
            ),
        }
    }

    const STEP_HEAD: &str = "jobs:\n  j:\n    steps:\n      - name: first\n        run: echo\n      \
                             - name: probe\n";

    /// 스텝 조건이 블록 스칼라이거나 true/false가 아니면 스텝 판독 셋이 모두 실패한다.
    #[test]
    fn unsupported_step_conditions_fail_every_step_reader() {
        for (extra, want) in [
            (
                "        if: >-\n          ${{ !cancelled() }}\n",
                "fixture.yml:7 지원하지 않는 표기: if 값이 블록 스칼라다",
            ),
            (
                "        continue-on-error: yes\n",
                "fixture.yml:7 지원하지 않는 표기: continue-on-error 값이 true/false가 아니다",
            ),
        ] {
            let yaml = format!("{STEP_HEAD}{extra}        run: cargo test --workspace\n");
            assert_rejected(job_steps("fixture.yml", &yaml), want);
            assert_rejected(swallowable_verification_steps("fixture.yml", &yaml), want);
            assert_rejected(protected_verification_steps("fixture.yml", &yaml), want);
        }
    }

    /// 잡 머리 줄 끝의 주석(콜론으로 끝나도)은 잡 판독 전부가 거절한다.
    #[test]
    fn an_unsupported_job_header_fails() {
        for head in ["  build: # x\n", "  build: # x:\n"] {
            let yaml = format!("on:\n  push:\njobs:\n{head}    steps:\n      - run: cargo test\n");
            let want = "fixture.yml:4 지원하지 않는 표기: 잡 머리가 아니다";
            assert_rejected(job_spans("fixture.yml", &yaml), want);
            assert_rejected(automatic_job_bodies("fixture.yml", &yaml), want);
            assert_rejected(job_steps("fixture.yml", &yaml), want);
        }
    }

    /// `jobs:` 블록 뒤 최상위 키는 마지막 잡 본문에 들지 않는다. 들면 그 안의 수동 조건 글자가 잡을 수동으로 바꾼다.
    #[test]
    fn a_top_level_key_after_jobs_is_not_in_the_last_job() {
        let yaml = "on:\n  push:\njobs:\n  a:\n    steps:\n      - run: cargo test\nenv:\n  \
                    NOTE: \"github.event_name == 'workflow_dispatch'\"\n";
        let spans = job_spans("fixture.yml", yaml).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(spans.len(), 1);
        assert!(
            !spans[0].body.contains("NOTE"),
            "jobs 블록 뒤의 줄이 잡 본문에 들어갔다: {:?}",
            spans[0].body
        );
        assert_eq!(
            automatic_job_names("fixture.yml", yaml).unwrap_or_else(|e| panic!("{e:?}")),
            vec!["a".to_string()],
            "뒤 최상위 키의 글자로 자동 잡을 수동으로 분류했다"
        );
    }

    fn scratch_workflows(what: &str, files: &[(&str, &str)]) -> crate::temp_scratch::Scratch {
        let dir = crate::temp_scratch::Scratch::new(what);
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text)
                .unwrap_or_else(|e| panic!("{name}를 못 썼다: {e}"));
        }
        dir
    }

    /// 경로 필터가 있어 채널에서 빠질 파일도 잡 구조는 먼저 본다. `on:`과 잡 구조가 함께 틀리면 둘 다 알린다.
    #[test]
    fn filter_free_coverage_checks_the_structure_before_the_channel_filter() {
        let filtered = "on:\n  push:\n    paths:\n      - 'src/**'\njobs:\n  build: # x\n    \
                        steps:\n      - run: cargo test --workspace\n";
        let dir = scratch_workflows("ffc-structure", &[("filtered.yml", filtered)]);
        assert_rejected(
            filter_free_coverage(dir.path()),
            "filtered.yml:6 지원하지 않는 표기: 잡 머리가 아니다",
        );
        let both = "on: [push]\njobs:\n  build: # x\n    steps:\n      - run: cargo test\n";
        let dir = scratch_workflows("ffc-both", &[("both.yml", both)]);
        let errs = filter_free_coverage(dir.path()).expect_err("두 판독이 모두 실패했다");
        for want in [
            "both.yml: on: 을 못 읽었다",
            "both.yml:3 지원하지 않는 표기: 잡 머리가 아니다",
        ] {
            assert!(
                errs.iter().any(|e| e.contains(want)),
                "`{want}`를 기대했다. 실제: {errs:?}"
            );
        }
    }
}
