//! self-hosted 러너의 빌드 캐시 보존 배선을 워크플로 소스에서 고정한다.
//! 규칙의 설명은 docs/dev-guide/ci-gates.md의 "러너의 빌드 캐시" 절에 있다.
//! 조건식은 평가하지 않고 글자로 확인한다. 실제 실행 결과는 보지 않는다.

use std::path::PathBuf;

use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};
use tasty_doc_guards::workflow_triggers::{Step, job_spans, job_steps};

const PREPARE_OK: &str = "steps.prepare.outcome == 'success'";
const JUDGE_OK: &str = "steps.judge.outcome == 'success'";

/// 처음부터 빌드해야 하는 패키징 워크플로(사용자 결정 4·5). checkout 정리를 끄면 안 된다.
const COLD_WORKFLOWS: [&str; 2] = ["build-check.yml", "release.yml"];

/// script-gates에서 판정기(mask-source·strip-cfg-test)를 쓰는 스텝. 판정기 빌드가 실패하면 돌지 않아야 한다.
const JUDGE_USERS: [&str; 3] = [
    "Allow reason (ratchet)",
    "Shared walk (ratchet)",
    "Core writer (ratchet)",
];

/// 워크플로 파일 수집의 하한. 파일 하나를 못 읽는 것은 아래 개별 하한이 잡는다.
const WORKFLOW_FLOOR: Floor = Floor {
    min: 8,
    measured: 11,
    measured_on: "2026-11-24",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "ed95092f6d — git ls-tree -r --name-only HEAD .github/workflows에서 yml 파일 11개를 셌다.",
    ),
    why_this_gap: "워크플로 통폐합을 허용하는 하한이다. 보존 잡과 판정 스텝 수는 아래 하한이 따로 본다.",
};

/// `clean: false` checkout 수의 하한. 2026-11-24 ed95092f6d에서 7개 워크플로 12잡이었다.
const CACHED_CHECKOUT_FLOOR: usize = 12;
/// 앞 실패 뒤에도 도는 판정 스텝 수의 하한. 같은 커밋에서 crossplatform-check 10(관측용 gui e2e는
/// continue-on-error라 빠진다), script-gates 5, supply-chain-check 1이었다.
const AFTER_FAILURE_STEP_FLOOR: usize = 16;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("레포 루트")
        .to_path_buf()
}

/// (파일 이름, 원문) 목록.
fn workflows() -> Vec<(String, String)> {
    let dir = repo_root().join(".github/workflows");
    let walked = walk_with_floor(&dir, &dir, &WORKFLOW_FLOOR, Descend::Everything, &|w| {
        w.rel.ends_with(".yml")
    })
    .unwrap_or_else(|why| panic!("{why}"));
    walked
        .into_iter()
        .map(|w| {
            let text = std::fs::read_to_string(&w.path)
                .unwrap_or_else(|e| panic!("워크플로 {} 를 못 읽었다: {e}", w.rel));
            (w.rel, text)
        })
        .collect()
}

/// 스텝 본문에서 스텝 수준 `if:` 식을 꺼낸다.
fn if_expr(step: &Step) -> Option<&str> {
    step.body
        .lines()
        .find_map(|l| l.strip_prefix("        if:"))
        .map(str::trim)
}

fn has_line(step: &Step, want: &str) -> bool {
    step.body.lines().any(|l| l.trim() == want)
}

fn is_checkout(step: &Step) -> bool {
    step.name.starts_with("actions/checkout")
}

/// 앞 스텝이 실패해도 이 스텝을 돌리는 조건인가.
fn runs_after_failure(expr: &str) -> bool {
    expr.contains("!cancelled()")
        || expr.contains("always()")
        || expr.contains("success() || failure()")
}

/// GitHub 호스팅 러너 라벨. 작업 폴더가 잡마다 새로 만들어지므로 캐시 보존 대상이 아니다.
/// 자체 러너는 `self-hosted` 라벨 없이 고유 라벨만 쓸 수 있어(`[win-arm64]`) 호스팅 라벨이 아닌 것으로 판정한다.
const HOSTED_LABEL_PREFIXES: [&str; 3] = ["ubuntu-", "windows-", "macos-"];

fn is_hosted(job_body: &str) -> bool {
    job_body
        .lines()
        .filter_map(|l| l.trim().strip_prefix("runs-on:"))
        .any(|v| {
            v.trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
                .split(',')
                .any(|label| {
                    HOSTED_LABEL_PREFIXES
                        .iter()
                        .any(|p| label.trim().starts_with(p))
                })
        })
}

/// `clean: false`로 부른 checkout 바로 다음 스텝은 id `prepare`여야 하고, 그 잡은 자체 러너 잡이어야 한다.
/// 준비 없이 정리만 끄면 이전 잡의 미추적 파일 위에서 검사가 돈다.
#[test]
fn every_cached_checkout_is_followed_by_the_prepare_step() {
    let mut seen = 0usize;
    let mut bad = Vec::new();
    for (file, text) in workflows() {
        let steps = job_steps(&text);
        let hosted: Vec<String> = job_spans(&text)
            .into_iter()
            .filter(|j| is_hosted(&j.body))
            .map(|j| j.name)
            .collect();
        for (k, s) in steps.iter().enumerate() {
            if !is_checkout(s) || !has_line(s, "clean: false") {
                continue;
            }
            seen += 1;
            if hosted.contains(&s.job) {
                bad.push(format!(
                    "{file}:{} {} — GitHub 호스팅 러너 잡이 정리를 끈다",
                    s.line, s.job
                ));
            }
            let next = steps.get(k + 1).filter(|n| n.job == s.job);
            if !next.is_some_and(|n| has_line(n, "id: prepare")) {
                bad.push(format!(
                    "{file}:{} {} — clean: false checkout 다음 스텝이 id prepare가 아니다",
                    s.line, s.job
                ));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    assert!(
        seen >= CACHED_CHECKOUT_FLOOR,
        "clean: false checkout이 {seen}개다(하한 {CACHED_CHECKOUT_FLOOR}). 워크플로를 못 읽었거나 보존 잡이 줄었다"
    );
}

/// 준비 스텝이 있는 잡에서 앞 실패 뒤에도 도는 판정 스텝은 준비 성공을 조건으로 가져야 한다.
/// `continue-on-error: true`인 진단 스텝은 판정을 내지 않으므로 뺀다. `success() || failure()`는
/// `&&`가 먼저 묶이므로 괄호로 싸야 준비 조건이 두 갈래 모두에 걸린다.
#[test]
fn steps_that_run_after_failures_require_a_successful_prepare() {
    let mut seen = 0usize;
    let mut bad = Vec::new();
    for (file, text) in workflows() {
        let steps = job_steps(&text);
        for (k, s) in steps.iter().enumerate() {
            let after_prepare = steps[..k]
                .iter()
                .any(|p| p.job == s.job && has_line(p, "id: prepare"));
            if !after_prepare || s.continue_on_error {
                continue;
            }
            let Some(expr) = if_expr(s) else { continue };
            if !runs_after_failure(expr) {
                continue;
            }
            seen += 1;
            if !expr.contains(PREPARE_OK) {
                bad.push(format!(
                    "{file}:{} {} / {} — 준비 성공 조건이 없다: {expr}",
                    s.line, s.job, s.name
                ));
            }
            if expr.contains("success() || failure()") && !expr.contains("(success() || failure())")
            {
                bad.push(format!(
                    "{file}:{} {} / {} — OR 조건을 괄호로 묶지 않았다: {expr}",
                    s.line, s.job, s.name
                ));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    assert!(
        seen >= AFTER_FAILURE_STEP_FLOOR,
        "앞 실패 뒤에도 도는 판정 스텝이 {seen}개다(하한 {AFTER_FAILURE_STEP_FLOOR})"
    );
}

/// script-gates의 판정기 사용 스텝은 판정기 빌드(id `judge`)의 성공도 조건으로 가진다.
#[test]
fn judge_users_require_a_successful_judge_build() {
    let (_, text) = workflows()
        .into_iter()
        .find(|(f, _)| f == "script-gates.yml")
        .expect("script-gates.yml");
    let steps = job_steps(&text);
    assert!(
        steps.iter().any(|s| has_line(s, "id: judge")),
        "script-gates.yml에 id judge 스텝이 없다"
    );
    for want in JUDGE_USERS {
        let step = steps
            .iter()
            .find(|s| s.name == want)
            .unwrap_or_else(|| panic!("script-gates.yml에 스텝 {want}가 없다"));
        let expr = if_expr(step).unwrap_or("");
        assert!(
            expr.contains(JUDGE_OK) && expr.contains(PREPARE_OK),
            "script-gates.yml:{} {want} — 판정기·준비 성공 조건이 없다: {expr}",
            step.line
        );
    }
}

/// 패키징 워크플로는 checkout 정리를 끄지 않는다. build-check는 다섯 checkout 모두 `clean: true`를 적는다.
#[test]
fn packaging_workflows_never_disable_the_checkout_clean() {
    let all = workflows();
    for want in COLD_WORKFLOWS {
        let (_, text) = all
            .iter()
            .find(|(f, _)| f == want)
            .unwrap_or_else(|| panic!("{want}가 없다"));
        let steps = job_steps(text);
        let checkouts: Vec<&Step> = steps.iter().filter(|s| is_checkout(s)).collect();
        assert!(
            !checkouts.is_empty(),
            "{want}에서 checkout을 하나도 못 읽었다"
        );
        for s in &checkouts {
            assert!(
                !has_line(s, "clean: false"),
                "{want}:{} {} — 패키징 checkout이 정리를 끈다",
                s.line,
                s.job
            );
        }
        if want == "build-check.yml" {
            let explicit = checkouts
                .iter()
                .filter(|s| has_line(s, "clean: true"))
                .count();
            assert_eq!(explicit, 5, "build-check.yml의 clean: true checkout 수");
        }
    }
}
