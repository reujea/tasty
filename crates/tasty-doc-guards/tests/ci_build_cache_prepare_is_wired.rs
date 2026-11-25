//! self-hosted 러너의 빌드 캐시 보존 배선을 워크플로 소스에서 고정한다.
//! 규칙의 설명은 docs/dev-guide/ci-gates.md의 "러너의 빌드 캐시" 절에 있다.
//! 조건식은 평가하지 않고 글자로 확인한다. 실제 실행 결과는 보지 않는다.
//!
//! 워크플로는 tasty_doc_guards::workflow_yaml의 좁은 표기 판독으로 읽는다. 받는 표기는 그 모듈 설명에 있고,
//! 판정에 쓰는 값이 그 밖의 표기면 "지원하지 않는 표기"로 실패한다(fail closed).

use std::path::PathBuf;

use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};
use tasty_doc_guards::workflow_yaml::{
    Job, Step, UNSUPPORTED, boolean, parse, runs_on_labels, scalar, step_conditions,
    top_level_and_terms, with_fields,
};

const PREPARE_OK: &str = "steps.prepare.outcome == 'success'";
const JUDGE_OK: &str = "steps.judge.outcome == 'success'";

/// Linux·macOS prepare 스텝의 `run` 값. 이 한 줄과 정확히 같아야 한다.
const PREPARE_RUN_BASH: &str = "bash scripts/ci-prepare-workspace.sh";
/// Windows prepare 스텝(`shell: pwsh`)의 `run: |` 블록. 들여쓰기 10칸을 뗀 줄이 이 목록과 정확히 같아야 한다.
/// git 옆의 Git Bash를 찾아 준비 스크립트를 부르고 그 종료 코드를 스텝 결과로 넘긴다.
const PREPARE_RUN_PWSH: [&str; 9] = [
    "$gitRoot = Split-Path (Split-Path (Get-Command git -ErrorAction Stop).Source -Parent) -Parent",
    "$bash = @(",
    "  (Join-Path $gitRoot 'bin\\bash.exe'),",
    "  (Join-Path (Split-Path $gitRoot -Parent) 'bin\\bash.exe'),",
    "  'C:\\Program Files\\Git\\bin\\bash.exe'",
    ") | Where-Object { Test-Path $_ } | Select-Object -First 1",
    "if (-not $bash) { throw \"Git Bash not found next to $gitRoot\" }",
    "& $bash scripts/ci-prepare-workspace.sh",
    "if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }",
];
/// 정리를 끈 checkout이 가질 수 없는 키. 붙으면 checkout을 건너뛰거나 실패를 숨길 수 있다.
const CACHED_CHECKOUT_DENIED_KEYS: [&str; 2] = ["if", "continue-on-error"];
/// prepare 스텝이 가질 수 있는 키. `if`·`continue-on-error`·`env` 등은 준비를 건너뛰거나 실패를 숨길 수 있어 받지 않는다.
const PREPARE_KEYS: [&str; 4] = ["name", "id", "shell", "run"];

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
/// Windows Git Bash로 준비 스크립트를 부르는 prepare 수의 하한. 같은 커밋에서 crossplatform-check의 두 Windows 잡이었다.
const WINDOWS_PREPARE_FLOOR: usize = 2;
/// 앞 실패 뒤에도 도는 판정 스텝 수의 하한. 같은 커밋에서 crossplatform-check 10(관측용 gui e2e는
/// continue-on-error라 빠진다), script-gates 5, supply-chain-check 1이었다.
const AFTER_FAILURE_STEP_FLOOR: usize = 16;

/// GitHub 호스팅 러너 라벨. 작업 폴더가 잡마다 새로 만들어지므로 캐시 보존 대상이 아니다.
/// 자체 러너는 `self-hosted` 라벨 없이 고유 라벨만 쓸 수 있어(`[win-arm64]`) 호스팅 라벨이 아닌 것으로 판정한다.
const HOSTED_LABEL_PREFIXES: [&str; 3] = ["ubuntu-", "windows-", "macos-"];

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

// ---------------------------------------------------------------------------
// 좁은 표기 읽기는 tasty_doc_guards::workflow_yaml에 있다. 아래는 이 가드의 규칙만 쓴다.
// ---------------------------------------------------------------------------

/// 상태 함수. 식에 하나라도 있으면 GitHub가 기본 `success()`를 붙이지 않는다.
const STATUS_FUNCTIONS: [&str; 4] = ["success", "failure", "always", "cancelled"];

/// 앞 스텝이 실패해도 이 스텝을 돌릴 수 있는 조건인가. 상태 함수 이름(대소문자 무시) 뒤에 공백과 `(`가
/// 오는 곳이 하나라도 있으면 대상이다. 부정·OR·문자열 안의 이름도 대상으로 보아 준비 조건을 요구한다(fail closed).
fn runs_after_failure(expr: &str) -> bool {
    let lower = expr.to_ascii_lowercase();
    STATUS_FUNCTIONS.iter().any(|name| {
        lower
            .match_indices(name)
            .any(|(at, _)| lower[at + name.len()..].trim_start().starts_with('('))
    })
}

fn is_hosted(labels: &[String]) -> bool {
    labels
        .iter()
        .any(|l| HOSTED_LABEL_PREFIXES.iter().any(|p| l.starts_with(p)))
}

// ---------------------------------------------------------------------------
// 규칙
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Report {
    problems: Vec<String>,
    cached_checkouts: usize,
    windows_prepares: usize,
    after_failure_steps: usize,
}

/// checkout 스텝인가. `actions/checkout`은 소문자 꼴만 받는다. 대소문자만 다른 꼴은 checkout일 수 있으므로
/// 분류에서 빼지 않고 지원하지 않는 표기로 실패한다.
fn is_checkout(file: &str, step: &Step, errs: &mut Vec<String>) -> bool {
    match scalar(file, &step.fields, "uses") {
        Ok(Some(u)) if u.starts_with("actions/checkout") => true,
        Ok(Some(u)) if u.to_ascii_lowercase().starts_with("actions/checkout") => {
            let line = step
                .fields
                .iter()
                .find(|f| f.key == "uses")
                .map_or(step.line, |f| f.line);
            errs.push(format!(
                "{file}:{line} {UNSUPPORTED}: uses의 actions/checkout은 소문자로 쓴다: {u}"
            ));
            false
        }
        Ok(_) => false,
        Err(e) => {
            errs.push(e);
            false
        }
    }
}

/// with.clean. 없으면 None.
fn checkout_clean(file: &str, step: &Step, errs: &mut Vec<String>) -> Option<bool> {
    let with = with_fields(file, step, errs);
    boolean(file, &with, "clean").unwrap_or_else(|e| {
        errs.push(e);
        None
    })
}

fn is_prepare(file: &str, step: &Step, errs: &mut Vec<String>) -> bool {
    match scalar(file, &step.fields, "id") {
        Ok(v) => v.as_deref() == Some("prepare"),
        Err(e) => {
            errs.push(e);
            false
        }
    }
}

fn step_name(file: &str, step: &Step) -> String {
    scalar(file, &step.fields, "name")
        .ok()
        .flatten()
        .unwrap_or_else(|| "(이름 없음)".to_string())
}

/// 정리를 끈 checkout: 패키징 워크플로가 아니고, 자체 러너 잡이고, 바로 다음 스텝이 prepare다.
fn check_cached_checkout(file: &str, job: &Job, k: usize, r: &mut Report) {
    let s = &job.steps[k];
    let p = &mut r.problems;
    r.cached_checkouts += 1;
    // prepare처럼 조건 키를 받지 않는다. 이 checkout만 건너뛰면 이전 작업 트리의 소스로 뒤 스텝이 돈다.
    for f in s
        .fields
        .iter()
        .filter(|f| CACHED_CHECKOUT_DENIED_KEYS.contains(&f.key.as_str()))
    {
        p.push(format!(
            "{file}:{} {UNSUPPORTED}: clean: false checkout에 {}가 있다",
            f.line, f.key
        ));
    }
    if COLD_WORKFLOWS.contains(&file) {
        p.push(format!(
            "{file}:{} {} — 패키징 checkout이 정리를 끈다",
            s.line, job.name
        ));
    }
    match runs_on_labels(file, job) {
        Ok(labels) if is_hosted(&labels) => p.push(format!(
            "{file}:{} {} — GitHub 호스팅 러너 잡이 정리를 끈다: {labels:?}",
            s.line, job.name
        )),
        Ok(_) => {}
        Err(e) => p.push(e),
    }
    if !job.steps.get(k + 1).is_some_and(|n| is_prepare(file, n, p)) {
        p.push(format!(
            "{file}:{} {} — clean: false checkout 다음 스텝이 id prepare가 아니다",
            s.line, job.name
        ));
    }
}

/// 잡의 OS가 Windows인가. runs-on 라벨에 `Windows`나 `win-`으로 시작하는 라벨이 있으면 Windows,
/// `Linux`나 `macOS`가 있으면 아니다. 둘 다 있거나 둘 다 없으면 읽지 못한 것으로 None이다.
fn job_is_windows(labels: &[String]) -> Option<bool> {
    let lower: Vec<String> = labels.iter().map(|l| l.to_ascii_lowercase()).collect();
    let windows = lower
        .iter()
        .any(|l| l == "windows" || l.starts_with("win-"));
    let unix = lower.iter().any(|l| l == "linux" || l == "macos");
    (windows != unix).then_some(windows)
}

/// prepare 스텝은 좁은 꼴 하나만 받는다. 키는 name·id·shell·run뿐이다. 잡의 OS(runs-on)가 Linux·macOS면
/// `shell: bash`와 [`PREPARE_RUN_BASH`] 한 줄, Windows면 `shell: pwsh`와 [`PREPARE_RUN_PWSH`] 블록이다.
fn check_prepare_body(file: &str, job: &Job, s: &Step, r: &mut Report) {
    let p = &mut r.problems;
    let at = format!("{file}:{} {}", s.line, s.job);
    let windows = match runs_on_labels(file, job).map(|l| (job_is_windows(&l), l)) {
        Ok((Some(w), _)) => w,
        Ok((None, labels)) => {
            return p.push(format!(
                "{at} — {UNSUPPORTED}: prepare 잡의 OS를 runs-on에서 읽지 못했다: {labels:?}"
            ));
        }
        Err(e) => return p.push(e),
    };
    for f in &s.fields {
        if !PREPARE_KEYS.contains(&f.key.as_str()) {
            p.push(format!(
                "{at} — {UNSUPPORTED}: prepare 스텝에 받지 않는 키 {}가 있다",
                f.key
            ));
        }
    }
    let shell = match scalar(file, &s.fields, "shell") {
        Ok(v) => v,
        Err(e) => return p.push(e),
    };
    let Some(run) = s.fields.iter().find(|f| f.key == "run") else {
        return p.push(format!("{at} — {UNSUPPORTED}: prepare 스텝에 run이 없다"));
    };
    let body: Vec<Option<&str>> = run
        .children
        .iter()
        .map(|(_, l)| l.strip_prefix(&" ".repeat(10)))
        .collect();
    let want_shell = if windows { "pwsh" } else { "bash" };
    if shell.as_deref() != Some(want_shell) {
        return p.push(format!(
            "{at} — {UNSUPPORTED}: prepare 스텝의 꼴이 잡의 OS와 다르다: shell {shell:?}, 기대 {want_shell}"
        ));
    }
    let exact = match shell.as_deref() {
        Some("bash") => run.inline == PREPARE_RUN_BASH && run.children.is_empty(),
        Some("pwsh") => {
            let ok = run.inline == "|"
                && body.len() == PREPARE_RUN_PWSH.len()
                && body
                    .iter()
                    .zip(PREPARE_RUN_PWSH)
                    .all(|(l, want)| *l == Some(want));
            if ok {
                r.windows_prepares += 1;
            }
            ok
        }
        _ => {
            return p.push(format!(
                "{at} — {UNSUPPORTED}: prepare 스텝의 shell은 bash나 pwsh다: {shell:?}"
            ));
        }
    };
    if !exact {
        p.push(format!(
            "{at} — {UNSUPPORTED}: prepare 스텝의 run이 정해진 준비 스크립트 호출과 다르다"
        ));
    }
}

/// 모든 스텝의 `if`·`continue-on-error` 표기를 본다. 아래 판정의 예외(진단 스텝)와 따로 두어,
/// 예외에 해당하는 스텝도 지원 밖 표기면 실패한다.
fn check_step_notation(file: &str, s: &Step, p: &mut Vec<String>) {
    if let Err(e) = step_conditions(file, s) {
        p.extend(e);
    }
}

/// prepare 뒤에서 앞 실패에도 도는 판정 스텝은 최상위 && 항으로 준비 성공을 둔다.
/// `continue-on-error: true`인 스텝은 판정을 내지 않으므로 뺀다. 표기 오류는 [`check_step_notation`]이 낸다.
fn check_after_prepare(file: &str, s: &Step, r: &mut Report) {
    let p = &mut r.problems;
    if boolean(file, &s.fields, "continue-on-error") == Ok(Some(true)) {
        return;
    }
    let Ok(Some(expr)) = scalar(file, &s.fields, "if") else {
        return;
    };
    if !runs_after_failure(&expr) {
        return;
    }
    r.after_failure_steps += 1;
    let name = step_name(file, s);
    match top_level_and_terms(&expr) {
        Ok(terms) if terms.iter().any(|t| t == PREPARE_OK) => {}
        Ok(_) => p.push(format!(
            "{file}:{} {} / {name} — 최상위 && 항에 준비 성공 조건이 없다: {expr}",
            s.line, s.job
        )),
        Err(e) => p.push(format!(
            "{file}:{} {} / {name} — {e}: {expr}",
            s.line, s.job
        )),
    }
}

/// 모든 워크플로에 규칙을 적용한다.
fn check(workflows: &[(String, String)]) -> Report {
    let mut r = Report::default();
    for (file, text) in workflows {
        let (jobs, errs) = parse(file, text);
        r.problems.extend(errs);
        let mut explicit_clean_true = 0usize;
        for job in &jobs {
            // runs-on은 모든 잡에서 좁은 꼴인지 본다. 읽지 못한 라벨을 자체 러너로 분류하지 않기 위해서다.
            if job.fields.iter().any(|f| f.key == "runs-on")
                && let Err(e) = runs_on_labels(file, job)
            {
                r.problems.push(e);
            }
            let mut prepared = false;
            for (k, s) in job.steps.iter().enumerate() {
                check_step_notation(file, s, &mut r.problems);
                if is_checkout(file, s, &mut r.problems) {
                    match checkout_clean(file, s, &mut r.problems) {
                        Some(true) => explicit_clean_true += 1,
                        Some(false) => check_cached_checkout(file, job, k, &mut r),
                        None => {}
                    }
                }
                if is_prepare(file, s, &mut r.problems) {
                    prepared = true;
                    check_prepare_body(file, job, s, &mut r);
                } else if prepared {
                    check_after_prepare(file, s, &mut r);
                }
            }
        }
        if file == "build-check.yml" && explicit_clean_true != 5 {
            r.problems.push(format!(
                "build-check.yml의 clean: true checkout이 {explicit_clean_true}개다(기대 5)"
            ));
        }
        if file == "script-gates.yml" {
            check_judge_users(file, &jobs, &mut r.problems);
        }
    }
    r
}

/// script-gates의 판정기 사용 스텝은 최상위 && 항으로 준비·판정기 빌드(id `judge`) 성공을 둔다.
fn check_judge_users(file: &str, jobs: &[Job], p: &mut Vec<String>) {
    let mut notation = Vec::new();
    let steps: Vec<&Step> = jobs.iter().flat_map(|j| &j.steps).collect();
    if !steps
        .iter()
        .any(|s| scalar(file, &s.fields, "id").ok().flatten().as_deref() == Some("judge"))
    {
        p.push(format!("{file}에 id judge 스텝이 없다"));
    }
    for want in JUDGE_USERS {
        let Some(step) = steps.iter().find(|s| step_name(file, s) == want) else {
            p.push(format!("{file}에 스텝 {want}가 없다"));
            continue;
        };
        let expr = scalar(file, &step.fields, "if")
            .unwrap_or_else(|e| {
                notation.push(e);
                None
            })
            .unwrap_or_default();
        match top_level_and_terms(&expr) {
            Ok(t) if t.iter().any(|x| x == PREPARE_OK) && t.iter().any(|x| x == JUDGE_OK) => {}
            Ok(_) => p.push(format!(
                "{file}:{} {want} — 최상위 && 항에 판정기·준비 성공 조건이 없다: {expr}",
                step.line
            )),
            Err(e) => p.push(format!("{file}:{} {want} — {e}: {expr}", step.line)),
        }
    }
    p.extend(notation);
}

// ---------------------------------------------------------------------------
// 시험
// ---------------------------------------------------------------------------

/// 실제 워크플로는 규칙을 모두 지키고 하한 이상을 읽는다.
#[test]
fn the_workflows_keep_the_build_cache_wiring() {
    let r = check(&workflows());
    assert!(r.problems.is_empty(), "{}", r.problems.join("\n"));
    assert!(
        r.cached_checkouts >= CACHED_CHECKOUT_FLOOR,
        "clean: false checkout이 {}개다(하한 {CACHED_CHECKOUT_FLOOR}). 워크플로를 못 읽었거나 보존 잡이 줄었다",
        r.cached_checkouts
    );
    assert!(
        r.windows_prepares >= WINDOWS_PREPARE_FLOOR,
        "Git Bash로 준비 스크립트를 부르는 prepare가 {}개다(하한 {WINDOWS_PREPARE_FLOOR})",
        r.windows_prepares
    );
    assert!(
        r.after_failure_steps >= AFTER_FAILURE_STEP_FLOOR,
        "앞 실패 뒤에도 도는 판정 스텝이 {}개다(하한 {AFTER_FAILURE_STEP_FLOOR})",
        r.after_failure_steps
    );
}

/// `file`의 원문에서 `from`의 첫 출현을 `to`로 바꾼 워크플로 목록. `from`이 없으면 시험이 낡은 것이다.
fn mutated(file: &str, from: &str, to: &str) -> Vec<(String, String)> {
    let mut all = workflows();
    let (_, text) = all
        .iter_mut()
        .find(|(f, _)| f == file)
        .unwrap_or_else(|| panic!("{file}가 없다"));
    assert!(text.contains(from), "{file}에 바꿀 글자가 없다: {from}");
    *text = text.replacen(from, to, 1);
    all
}

fn assert_caught(all: &[(String, String)], want: &str) {
    let r = check(all);
    assert!(
        r.problems.iter().any(|p| p.contains(want)),
        "`{want}`를 기대했다. 실제:\n{}",
        r.problems.join("\n")
    );
}

const LINUX_PREPARE: &str = "      - name: prepare the working tree (build cache)\n        id: prepare\n        shell: bash\n        run: bash scripts/ci-prepare-workspace.sh\n";
const SUPPLY_IF: &str =
    "        if: ${{ (success() || failure()) && steps.prepare.outcome == 'success' }}\n";
const TEST_CACHED_CHECKOUT: &str =
    "      - uses: actions/checkout@v4\n        with:\n          clean: false\n";

/// X1: 패키징 워크플로의 checkout에 따옴표 친 `clean: 'false'`를 넣으면 잡는다.
#[test]
fn a_quoted_clean_false_in_a_packaging_checkout_is_caught() {
    let all = mutated(
        "release.yml",
        "      - uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n",
        "      - uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n          clean: 'false'\n",
    );
    assert_caught(&all, "패키징 checkout이 정리를 끈다");
}

/// 이름을 먼저 쓴 checkout도 checkout으로 읽는다.
#[test]
fn a_name_first_checkout_with_clean_false_is_caught() {
    let all = mutated(
        "release.yml",
        "      - uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n",
        "      - name: checkout\n        uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n          clean: false\n",
    );
    assert_caught(&all, "패키징 checkout이 정리를 끈다");
}

/// clean 값은 true/false만 받는다.
#[test]
fn a_clean_value_other_than_true_or_false_is_rejected() {
    let all = mutated(
        "test.yml",
        TEST_CACHED_CHECKOUT,
        "      - uses: actions/checkout@v4\n        with:\n          clean: off\n",
    );
    assert_caught(&all, "clean 값이 true/false가 아니다");
}

/// X2: 상태 함수 순서와 무관하게 최상위 `||`는 잡는다.
#[test]
fn a_failure_or_success_condition_is_caught() {
    let all = mutated(
        "supply-chain-check.yml",
        SUPPLY_IF,
        "        if: failure() || success()\n",
    );
    assert_caught(&all, "최상위에 ||가 있다");
}

/// X3: 블록 스칼라 if는 받지 않는다.
#[test]
fn a_block_scalar_if_is_rejected() {
    let all = mutated(
        "supply-chain-check.yml",
        SUPPLY_IF,
        "        if: >-\n          ${{ (success() || failure()) && steps.prepare.outcome == 'success' }}\n",
    );
    assert_caught(&all, "if 값이 블록 스칼라다");
}

/// 여러 줄로 이어 쓴 if도 받지 않는다.
#[test]
fn a_multi_line_if_is_rejected() {
    let all = mutated(
        "supply-chain-check.yml",
        SUPPLY_IF,
        "        if: ${{ (success() || failure())\n          && steps.prepare.outcome == 'success' }}\n",
    );
    assert_caught(&all, "if 값이 여러 줄이다");
}

/// 이름 없는 run 스텝을 checkout과 prepare 사이에 끼우면 잡는다.
#[test]
fn a_nameless_step_between_checkout_and_prepare_is_caught() {
    let all = mutated(
        "test.yml",
        TEST_CACHED_CHECKOUT,
        "      - uses: actions/checkout@v4\n        with:\n          clean: false\n      - run: echo before-prepare\n",
    );
    assert_caught(&all, "다음 스텝이 id prepare가 아니다");
}

/// `- id: prepare`로 시작하는 prepare도 독립 스텝으로 읽는다. 그대로면 통과하고, 그 뒤 판정 스텝의 준비
/// 조건을 빼면 잡는다(앞 스텝 본문에 합쳐 읽으면 뒤 스텝을 prepare 뒤로 보지 못한다).
#[test]
fn an_id_first_prepare_is_read_as_its_own_step() {
    let id_first = "      - id: prepare\n        name: prepare the working tree (build cache)\n        shell: bash\n        run: bash scripts/ci-prepare-workspace.sh\n";
    let ok = check(&mutated("supply-chain-check.yml", LINUX_PREPARE, id_first));
    assert!(ok.problems.is_empty(), "{}", ok.problems.join("\n"));
    let mut all = mutated("supply-chain-check.yml", LINUX_PREPARE, id_first);
    let (_, text) = all
        .iter_mut()
        .find(|(f, _)| f == "supply-chain-check.yml")
        .expect("supply-chain-check.yml");
    *text = text.replacen(SUPPLY_IF, "        if: ${{ success() || failure() }}\n", 1);
    assert_caught(&all, "최상위에 ||가 있다");
}

/// prepare 스텝 본문이 준비 스크립트를 부르지 않으면 잡는다.
#[test]
fn a_prepare_step_without_the_script_call_is_caught() {
    let all = mutated(
        "supply-chain-check.yml",
        "        run: bash scripts/ci-prepare-workspace.sh\n",
        "        run: echo skipped\n",
    );
    assert_caught(
        &all,
        "prepare 스텝의 run이 정해진 준비 스크립트 호출과 다르다",
    );
    let all = mutated(
        "crossplatform-check.yml",
        "          & $bash scripts/ci-prepare-workspace.sh\n",
        "          & $bash scripts/other.sh\n",
    );
    assert_caught(
        &all,
        "prepare 스텝의 run이 정해진 준비 스크립트 호출과 다르다",
    );
}

/// 따옴표 친 호스팅 라벨과 블록 목록 라벨도 읽는다.
#[test]
fn a_quoted_or_listed_hosted_label_is_caught() {
    let all = mutated(
        "test.yml",
        "    runs-on: [self-hosted, Linux]\n",
        "    runs-on: 'ubuntu-latest'\n",
    );
    assert_caught(&all, "GitHub 호스팅 러너 잡이 정리를 끈다");
    let all = mutated(
        "test.yml",
        "    runs-on: [self-hosted, Linux]\n",
        "    runs-on:\n      - self-hosted\n      - \"ubuntu-24.04\"\n",
    );
    assert_caught(&all, "GitHub 호스팅 러너 잡이 정리를 끈다");
    let all = mutated(
        "test.yml",
        "    runs-on: [self-hosted, Linux]\n",
        "    runs-on: ${{ matrix.os }}\n",
    );
    assert_caught(&all, "runs-on 라벨이 식이나 매핑이다");
}

/// 준비 조건 앞의 `&&`를 `||`로 바꾸면 잡는다(supply-chain-check 63행, script-gates 54행의 judge 앞).
#[test]
fn an_or_before_the_prepare_or_judge_term_is_caught() {
    let all = mutated(
        "supply-chain-check.yml",
        "(success() || failure()) && steps.prepare.outcome",
        "(success() || failure()) || steps.prepare.outcome",
    );
    assert_caught(&all, "최상위에 ||가 있다");
    let all = mutated(
        "script-gates.yml",
        "steps.prepare.outcome == 'success' && steps.judge.outcome",
        "steps.prepare.outcome == 'success' || steps.judge.outcome",
    );
    assert_caught(&all, "Allow reason (ratchet) — 최상위에 ||가 있다");
}

/// test.yml semver-guards의 prepare 바로 뒤에 `if: <cond>`인 판정 스텝을 끼운다.
fn probe_after_prepare(cond: &str) -> Vec<(String, String)> {
    mutated(
        "test.yml",
        LINUX_PREPARE,
        &format!(
            "{LINUX_PREPARE}\n      - name: probe\n        if: {cond}\n        run: cargo test --locked\n"
        ),
    )
}

/// M1: 상태 함수는 이름(대소문자 무시)과 `(` 사이 공백까지 읽는다. success()를 부정하거나 OR로 묶어도 대상이다.
#[test]
fn any_status_function_makes_the_step_need_the_prepare_term() {
    for (cond, want) in [
        ("${{ !success() }}", "최상위 && 항에 준비 성공 조건이 없다"),
        ("${{ success() || true }}", "최상위에 ||가 있다"),
        ("${{ Always() }}", "최상위 && 항에 준비 성공 조건이 없다"),
        ("${{ always () }}", "최상위 && 항에 준비 성공 조건이 없다"),
        ("${{ failure( ) }}", "최상위 && 항에 준비 성공 조건이 없다"),
    ] {
        assert_caught(&probe_after_prepare(cond), want);
    }
}

/// M2: 대상 식의 대괄호나 `&&`·`||`가 든 문자열 리터럴은 최상위 분해를 믿을 수 없으므로 받지 않는다.
#[test]
fn brackets_or_operators_inside_literals_are_rejected() {
    for cond in [
        "${{ always() && github.event['' && steps.prepare.outcome == 'success' && ''] == null }}",
        "${{ always() && steps.prepare.outcome == 'success' && github.ref != 'a || b' }}",
    ] {
        assert_caught(&probe_after_prepare(cond), "지원하지 않는 표기");
    }
}

/// M3: 값 뒤 주석이나 큰따옴표 이스케이프는 분류에서 빼지 않고 실패한다.
#[test]
fn unsupported_scalars_fail_instead_of_being_skipped() {
    let all = mutated(
        "test.yml",
        "    runs-on: [self-hosted, Linux]\n",
        "    runs-on: 'ubuntu-latest' # hosted\n",
    );
    assert_caught(&all, "test.yml:20 지원하지 않는 표기");
    let all = mutated(
        "release.yml",
        "      - uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n",
        "      - uses: \"actions/check\\u006fut@v4\"\n        with:\n          fetch-depth: 0\n          clean: false\n",
    );
    assert_caught(&all, "release.yml:22 지원하지 않는 표기");
}

/// M4: prepare는 if·continue-on-error 없이 정해진 한 줄(Linux·macOS) 또는 정해진 블록(Windows)만 받는다.
#[test]
fn a_prepare_step_that_can_be_skipped_or_short_circuited_is_caught() {
    let call = "        run: bash scripts/ci-prepare-workspace.sh\n";
    let with = |extra: &str| {
        mutated(
            "test.yml",
            LINUX_PREPARE,
            &LINUX_PREPARE.replace(call, &format!("{extra}{call}")),
        )
    };
    assert_caught(&with("        continue-on-error: true\n"), "prepare 스텝");
    assert_caught(&with("        if: false\n"), "prepare 스텝");
    let all = mutated(
        "test.yml",
        call,
        "        run: |\n          exit 0\n          bash scripts/ci-prepare-workspace.sh\n",
    );
    assert_caught(&all, "prepare 스텝");
    let all = mutated(
        "crossplatform-check.yml",
        "          & $bash scripts/ci-prepare-workspace.sh\n",
        "          exit 0\n          & $bash scripts/ci-prepare-workspace.sh\n",
    );
    assert_caught(&all, "prepare 스텝");
}

/// R1: 진단 스텝(continue-on-error: true)도 if 표기는 검사한다.
#[test]
fn a_diagnostic_step_with_a_block_scalar_if_is_rejected() {
    let all = mutated(
        "crossplatform-check.yml",
        "        if: ${{ !cancelled() }}\n        continue-on-error: true\n",
        "        if: >-\n          ${{ !cancelled() }}\n        continue-on-error: true\n",
    );
    assert_caught(&all, "crossplatform-check.yml:128 지원하지 않는 표기");
}

/// R2: 대상 식의 문자열 리터럴 안 대괄호도 받지 않는다.
#[test]
fn a_bracket_inside_a_literal_is_rejected() {
    let all = mutated(
        "supply-chain-check.yml",
        SUPPLY_IF,
        "        if: ${{ always() && steps.prepare.outcome == 'success' && github.ref != '[' }}\n",
    );
    assert_caught(&all, "지원하지 않는 표기: 식에 대괄호가 있다");
}

/// R3: Windows prepare 블록 끝의 `#` 줄도 블록 본문이므로 비교에 넣는다.
#[test]
fn a_comment_line_inside_the_windows_prepare_block_is_caught() {
    let all = mutated(
        "crossplatform-check.yml",
        "          if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n",
        "          if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n          #Requires -Version 99.0\n",
    );
    assert_caught(
        &all,
        "prepare 스텝의 run이 정해진 준비 스크립트 호출과 다르다",
    );
}

/// R4: prepare의 꼴은 잡의 OS(runs-on)로 고른다. Linux 잡에 Windows 블록이면 실패다.
#[test]
fn a_windows_prepare_block_in_a_linux_job_is_caught() {
    let block: String = PREPARE_RUN_PWSH
        .iter()
        .map(|l| format!("          {l}\n"))
        .collect();
    let all = mutated(
        "test.yml",
        LINUX_PREPARE,
        &format!(
            "      - name: prepare the working tree (build cache)\n        id: prepare\n        shell: pwsh\n        run: |\n{block}"
        ),
    );
    assert_caught(&all, "prepare 스텝의 꼴이 잡의 OS와 다르다");
}

/// R5: 지원 밖 표기 오류는 모든 경로에서 파일:행과 `지원하지 않는 표기`를 낸다.
#[test]
fn every_notation_error_names_the_line_and_the_phrase() {
    let text = workflows()
        .into_iter()
        .find(|(f, _)| f == "test.yml")
        .expect("test.yml")
        .1;
    let no = text.lines().position(|l| l == "jobs:").expect("jobs:") + 1;
    let all = mutated("test.yml", "\njobs:\n", "\njobs: # comment\n");
    assert_caught(&all, &format!("test.yml:{no} 지원하지 않는 표기"));
    let call = "        run: bash scripts/ci-prepare-workspace.sh\n";
    let all = mutated(
        "test.yml",
        LINUX_PREPARE,
        &LINUX_PREPARE.replace(call, &format!("        if: false\n{call}")),
    );
    assert_caught(
        &all,
        "지원하지 않는 표기: prepare 스텝에 받지 않는 키 if가 있다",
    );
    let all = mutated("test.yml", call, "        run: echo skipped\n");
    assert_caught(&all, "지원하지 않는 표기: prepare 스텝의 run이");
}

/// R6: 최상위 키(jobs 등)를 두 번 쓰면 실패한다. 두 번째 블록을 조용히 빼지 않는다.
#[test]
fn a_repeated_top_level_key_is_rejected() {
    let mut all = workflows();
    let (_, text) = all
        .iter_mut()
        .find(|(f, _)| f == "test.yml")
        .expect("test.yml");
    text.push_str("jobs:\n  extra:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          clean: false\n");
    assert_caught(&all, "지원하지 않는 표기: 최상위 키 jobs가 두 번 있다");
}

/// R7: checkout은 소문자 `actions/checkout`만 받는다. 대소문자를 바꾼 꼴은 실패다.
#[test]
fn a_checkout_in_another_case_is_rejected() {
    let all = mutated(
        "release.yml",
        "      - uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n",
        "      - uses: Actions/Checkout@v4\n        with:\n          fetch-depth: 0\n          clean: false\n",
    );
    assert_caught(&all, "release.yml:22 지원하지 않는 표기");
}

/// K1: 정리를 끈 checkout은 prepare처럼 if·continue-on-error 없이 쓴다. 조건 하나로 그 checkout만 건너뛰면
/// 이전 작업 트리의 소스로 시험이 돈다.
#[test]
fn a_cached_checkout_with_a_condition_is_rejected() {
    for extra in [
        "        if: always()\n",
        "        continue-on-error: true\n",
    ] {
        let all = mutated(
            "test.yml",
            TEST_CACHED_CHECKOUT,
            &TEST_CACHED_CHECKOUT.replacen(
                "      - uses: actions/checkout@v4\n",
                &format!("      - uses: actions/checkout@v4\n{extra}"),
                1,
            ),
        );
        assert_caught(
            &all,
            "test.yml:25 지원하지 않는 표기: clean: false checkout",
        );
    }
}

/// S1: 채널 가드(`workflow_triggers`)가 거절하는 지원 밖 표기를 이 가드도 같은 파일:행으로 거절한다.
/// 합성 파일 `probe.yml`이라 파일별 규칙과 저장소 하한이 닿지 않는다.
#[test]
fn the_notation_the_channel_guard_rejects_is_rejected_here_too() {
    let steps = "    runs-on: [self-hosted, Linux]\n    steps:\n      - name: first\n        run: echo\n      - name: probe\n";
    for (yaml, want) in [
        (
            format!(
                "on:\n  push:\njobs:\n  j:\n{steps}        if: >-\n          ${{{{ !cancelled() }}}}\n        run: cargo test\n"
            ),
            "probe.yml:10 지원하지 않는 표기: if 값이 블록 스칼라다",
        ),
        (
            format!(
                "on:\n  push:\njobs:\n  j:\n{steps}        continue-on-error: yes\n        run: cargo test\n"
            ),
            "probe.yml:10 지원하지 않는 표기: continue-on-error 값이 true/false가 아니다",
        ),
        (
            format!("on:\n  push:\njobs:\n  j: # x\n{steps}        run: cargo test\n"),
            "probe.yml:4 지원하지 않는 표기: 잡 머리가 아니다",
        ),
        (
            format!("on:\n  push:\njobs:\n  j: # x:\n{steps}        run: cargo test\n"),
            "probe.yml:4 지원하지 않는 표기: 잡 머리가 아니다",
        ),
    ] {
        assert_caught(&[("probe.yml".to_string(), yaml)], want);
    }
}
