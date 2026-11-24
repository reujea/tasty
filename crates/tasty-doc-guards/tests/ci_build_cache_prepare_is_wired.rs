//! self-hosted 러너의 빌드 캐시 보존 배선을 워크플로 소스에서 고정한다.
//! 규칙의 설명은 docs/dev-guide/ci-gates.md의 "러너의 빌드 캐시" 절에 있다.
//! 조건식은 평가하지 않고 글자로 확인한다. 실제 실행 결과는 보지 않는다.
//!
//! YAML 전체를 해석하지 않는다. 읽을 수 있는 좁은 표기만 받고, 판정에 쓰는 값이 그 밖의 표기면
//! "지원하지 않는 표기"로 실패한다(fail closed).
//! - 잡 머리는 `jobs:` 아래 2칸, 잡 키는 4칸, `steps:`는 4칸이다.
//! - 스텝은 6칸의 `- `로 시작하고(첫 키가 무엇이든 새 스텝), 스텝 키는 8칸, `with:` 아래 키는 10칸이다.
//! - 판정에 쓰는 값(`uses`·`id`·`if`·`continue-on-error`·`with.clean`·`runs-on`)은 한 줄 값이다. 작은·큰따옴표는
//!   벗겨 비교한다. 블록 스칼라(`|`·`>`)·여러 줄 값·줄 끝 주석은 받지 않는다.
//! - `runs-on`은 한 줄 값, 흐름 목록 `[a, b]`, 블록 목록(`- a`)을 받는다. 식(`${{ }}`)과 매핑은 받지 않는다.

use std::path::PathBuf;

use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};

const PREPARE_OK: &str = "steps.prepare.outcome == 'success'";
const JUDGE_OK: &str = "steps.judge.outcome == 'success'";
const UNSUPPORTED: &str = "지원하지 않는 표기";

/// prepare 스텝이 준비 스크립트를 부르는 줄. Linux·macOS는 bash, Windows는 git 옆의 Git Bash(`$bash`)다.
const PREPARE_CALLS: [&str; 2] = [
    "bash scripts/ci-prepare-workspace.sh",
    "& $bash scripts/ci-prepare-workspace.sh",
];

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
// 좁은 표기 읽기
// ---------------------------------------------------------------------------

/// 키 하나. `inline`은 `키:` 뒤 같은 줄의 글자, `children`은 더 깊이 들여쓴 다음 줄들(원문 그대로)이다.
#[derive(Debug, Clone)]
struct Field {
    key: String,
    line: usize,
    inline: String,
    children: Vec<(usize, String)>,
}

#[derive(Debug)]
struct Step {
    job: String,
    line: usize,
    fields: Vec<Field>,
}

#[derive(Debug)]
struct Job {
    name: String,
    line: usize,
    fields: Vec<Field>,
    steps: Vec<Step>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// 빈 줄과 전체 주석 줄은 구조에서 뺀다.
fn is_blank(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with('#')
}

/// `키: 값`을 나눈다. 키는 영문·숫자·`-`·`_`만 받는다.
fn split_key(text: &str) -> Option<(String, String)> {
    let (k, v) = text.split_once(':')?;
    if k.is_empty()
        || !k
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    if !(v.is_empty() || v.starts_with(' ')) {
        return None;
    }
    Some((k.to_string(), v.trim().to_string()))
}

/// 들여쓰기 `indent`의 키 줄들을 Field로 묶는다. 첫 줄은 이미 `- `를 뗀 상태로 줄 수 있다.
/// 같은 깊이에 키가 아닌 줄이 있으면 오류다.
fn fields_at(
    lines: &[(usize, String)],
    indent: usize,
    file: &str,
    errs: &mut Vec<String>,
) -> Vec<Field> {
    let mut out: Vec<Field> = Vec::new();
    for (no, raw) in lines {
        if is_blank(raw) {
            if let Some(f) = out.last_mut() {
                f.children.push((*no, raw.clone()));
            }
            continue;
        }
        let ind = indent_of(raw);
        if ind > indent {
            match out.last_mut() {
                Some(f) => f.children.push((*no, raw.clone())),
                None => errs.push(format!("{file}:{no} {UNSUPPORTED}: 키 없이 들여쓴 줄")),
            }
            continue;
        }
        if ind < indent {
            errs.push(format!("{file}:{no} {UNSUPPORTED}: 들여쓰기 {ind}칸"));
            continue;
        }
        match split_key(raw.trim_start()) {
            Some((key, inline)) => out.push(Field {
                key,
                line: *no,
                inline,
                children: Vec::new(),
            }),
            None => errs.push(format!(
                "{file}:{no} {UNSUPPORTED}: 키 줄이 아니다: {}",
                raw.trim()
            )),
        }
    }
    for f in &mut out {
        while f.children.last().is_some_and(|(_, l)| is_blank(l)) {
            f.children.pop();
        }
    }
    out
}

/// 워크플로를 잡과 스텝으로 나눈다. 구조 오류는 `errs`에 쌓는다.
fn parse(file: &str, text: &str) -> (Vec<Job>, Vec<String>) {
    let mut errs = Vec::new();
    let text = text.replace("\r\n", "\n");
    let lines: Vec<(usize, String)> = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.to_string()))
        .collect();
    let Some(start) = lines.iter().position(|(_, l)| l == "jobs:") else {
        errs.push(format!(
            "{file} {UNSUPPORTED}: 들여쓰기 없는 `jobs:` 줄이 없다"
        ));
        return (Vec::new(), errs);
    };
    let mut jobs = Vec::new();
    let mut k = start + 1;
    while k < lines.len() {
        let (no, raw) = &lines[k];
        if is_blank(raw) {
            k += 1;
            continue;
        }
        if indent_of(raw) == 0 {
            break;
        }
        let head = raw.trim();
        if indent_of(raw) != 2 || !head.ends_with(':') || split_key(head).is_none() {
            errs.push(format!(
                "{file}:{no} {UNSUPPORTED}: 잡 머리가 아니다: {head}"
            ));
            k += 1;
            continue;
        }
        let mut body = Vec::new();
        k += 1;
        while k < lines.len() {
            let (_, l) = &lines[k];
            if !is_blank(l) && indent_of(l) <= 2 {
                break;
            }
            body.push(lines[k].clone());
            k += 1;
        }
        let name = head.trim_end_matches(':').to_string();
        let fields = fields_at(&body, 4, file, &mut errs);
        let steps = fields
            .iter()
            .find(|f| f.key == "steps")
            .map(|f| parse_steps(&name, f, file, &mut errs))
            .unwrap_or_default();
        jobs.push(Job {
            name,
            line: *no,
            fields,
            steps,
        });
    }
    (jobs, errs)
}

fn parse_steps(job: &str, steps: &Field, file: &str, errs: &mut Vec<String>) -> Vec<Step> {
    if !steps.inline.is_empty() {
        errs.push(format!(
            "{file}:{} {UNSUPPORTED}: steps 값이 같은 줄에 있다",
            steps.line
        ));
        return Vec::new();
    }
    let mut groups: Vec<(usize, Vec<(usize, String)>)> = Vec::new();
    for (no, raw) in &steps.children {
        if !is_blank(raw) && indent_of(raw) == 6 {
            match raw.strip_prefix("      - ") {
                Some(rest) => {
                    groups.push((*no, vec![(*no, format!("{}{rest}", " ".repeat(8)))]));
                    continue;
                }
                None => {
                    errs.push(format!(
                        "{file}:{no} {UNSUPPORTED}: 스텝 깊이의 줄이 `- `로 시작하지 않는다"
                    ));
                    continue;
                }
            }
        }
        if !is_blank(raw) && indent_of(raw) < 6 {
            errs.push(format!(
                "{file}:{no} {UNSUPPORTED}: 스텝 들여쓰기가 6칸이 아니다"
            ));
            continue;
        }
        match groups.last_mut() {
            Some((_, g)) => g.push((*no, raw.clone())),
            None if is_blank(raw) => {}
            None => errs.push(format!("{file}:{no} {UNSUPPORTED}: 스텝 밖의 줄")),
        }
    }
    groups
        .into_iter()
        .map(|(line, g)| Step {
            job: job.to_string(),
            line,
            fields: fields_at(&g, 8, file, errs),
        })
        .collect()
}

/// 작은·큰따옴표를 벗긴다. 작은따옴표 안의 `''`는 `'` 하나다.
fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        return v[1..v.len() - 1].replace("''", "'");
    }
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return v[1..v.len() - 1].to_string();
    }
    v.to_string()
}

/// 판정에 쓰는 한 줄 값. 없으면 None, 받지 않는 표기면 Err.
fn scalar(file: &str, fields: &[Field], key: &str) -> Result<Option<String>, String> {
    let Some(f) = fields.iter().find(|f| f.key == key) else {
        return Ok(None);
    };
    let bad = |why: &str| format!("{file}:{} {UNSUPPORTED}: {key} {why}", f.line);
    if f.inline.starts_with('|') || f.inline.starts_with('>') {
        return Err(bad("값이 블록 스칼라다"));
    }
    if f.inline.is_empty() || f.children.iter().any(|(_, l)| !is_blank(l)) {
        return Err(bad("값이 여러 줄이다"));
    }
    if f.inline.contains(" #") {
        return Err(bad("값 뒤에 주석이 있다"));
    }
    Ok(Some(unquote(&f.inline)))
}

/// true/false만 받는 값.
fn boolean(file: &str, fields: &[Field], key: &str) -> Result<Option<bool>, String> {
    match scalar(file, fields, key)?.as_deref() {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(v) => {
            let line = fields.iter().find(|f| f.key == key).map_or(0, |f| f.line);
            Err(format!(
                "{file}:{line} {UNSUPPORTED}: {key} 값이 true/false가 아니다: {v}"
            ))
        }
    }
}

/// `with:` 아래의 키들(10칸).
fn with_fields(file: &str, step: &Step, errs: &mut Vec<String>) -> Vec<Field> {
    let Some(w) = step.fields.iter().find(|f| f.key == "with") else {
        return Vec::new();
    };
    if !w.inline.is_empty() {
        errs.push(format!(
            "{file}:{} {UNSUPPORTED}: with 값이 같은 줄에 있다",
            w.line
        ));
        return Vec::new();
    }
    fields_at(&w.children, 10, file, errs)
}

/// 스텝의 `run` 본문(한 줄 값 또는 블록의 각 줄, 앞뒤 공백 제거).
fn run_lines(step: &Step) -> Vec<String> {
    let Some(f) = step.fields.iter().find(|f| f.key == "run") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if !(f.inline.starts_with('|') || f.inline.starts_with('>')) {
        out.push(unquote(&f.inline));
    }
    out.extend(f.children.iter().map(|(_, l)| l.trim().to_string()));
    out
}

/// `${{ … }}`를 벗긴 식을 최상위 `&&` 항으로 나눈다. 최상위 `||`가 있으면 Err다.
fn top_level_and_terms(expr: &str) -> Result<Vec<String>, String> {
    let mut e = expr.trim();
    if let Some(inner) = e.strip_prefix("${{").and_then(|x| x.strip_suffix("}}")) {
        e = inner.trim();
    }
    if e.contains("${{") {
        return Err(format!("{UNSUPPORTED}: 식 안에 `${{{{`가 또 있다"));
    }
    let chars: Vec<char> = e.chars().collect();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut terms = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if in_str {
            cur.push(c);
            if c == '\'' {
                if next == Some('\'') {
                    cur.push('\'');
                    i += 2;
                    continue;
                }
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' => in_str = true,
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return Err(format!("{UNSUPPORTED}: 괄호가 맞지 않는다"));
        }
        if depth == 0 && c == '|' && next == Some('|') {
            return Err("최상위에 ||가 있다".to_string());
        }
        if depth == 0 && c == '&' && next == Some('&') {
            terms.push(cur.trim().to_string());
            cur.clear();
            i += 2;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if depth != 0 || in_str {
        return Err(format!("{UNSUPPORTED}: 괄호나 따옴표가 맞지 않는다"));
    }
    terms.push(cur.trim().to_string());
    Ok(terms)
}

/// 앞 스텝이 실패해도 이 스텝을 돌릴 수 있는 조건인가. 상태 함수 하나라도 있으면 대상이다.
fn runs_after_failure(expr: &str) -> bool {
    ["failure()", "always()", "cancelled()"]
        .iter()
        .any(|f| expr.contains(f))
}

/// `runs-on` 라벨. 한 줄 값, 흐름 목록, 블록 목록만 받는다.
fn runs_on_labels(file: &str, job: &Job) -> Result<Vec<String>, String> {
    let Some(f) = job.fields.iter().find(|f| f.key == "runs-on") else {
        return Err(format!("{file}:{} {} — runs-on이 없다", job.line, job.name));
    };
    let bad = |why: &str| format!("{file}:{} {UNSUPPORTED}: runs-on {why}", f.line);
    let labels: Vec<String> = if f.inline.is_empty() {
        let mut out = Vec::new();
        for (_, l) in f.children.iter().filter(|(_, l)| !is_blank(l)) {
            match l.trim().strip_prefix("- ") {
                Some(v) => out.push(unquote(v)),
                None => return Err(bad("블록 값이 목록이 아니다")),
            }
        }
        out
    } else {
        if f.children.iter().any(|(_, l)| !is_blank(l)) {
            return Err(bad("값이 여러 줄이다"));
        }
        let v = f.inline.trim();
        match v.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            Some(list) => list.split(',').map(unquote).collect(),
            None => vec![unquote(v)],
        }
    };
    if labels.is_empty() || labels.iter().any(|l| l.is_empty()) {
        return Err(bad("라벨이 비었다"));
    }
    if labels
        .iter()
        .any(|l| l.contains("${{") || l.contains(['[', ']', '{', '}', ':']))
    {
        return Err(bad("라벨이 식이나 매핑이다"));
    }
    Ok(labels)
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

fn is_checkout(file: &str, step: &Step, errs: &mut Vec<String>) -> bool {
    match scalar(file, &step.fields, "uses") {
        Ok(v) => v.is_some_and(|u| u.starts_with("actions/checkout")),
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

/// prepare 스텝은 준비 스크립트를 부른다.
fn check_prepare_body(file: &str, s: &Step, r: &mut Report) {
    let lines = run_lines(s);
    if lines.iter().any(|l| l == PREPARE_CALLS[1]) {
        r.windows_prepares += 1;
    }
    if !lines.iter().any(|l| PREPARE_CALLS.contains(&l.as_str())) {
        r.problems.push(format!(
            "{file}:{} {} — prepare 스텝이 준비 스크립트를 부르지 않는다",
            s.line, s.job
        ));
    }
}

/// prepare 뒤에서 앞 실패에도 도는 판정 스텝은 최상위 && 항으로 준비 성공을 둔다.
/// `continue-on-error: true`인 스텝은 판정을 내지 않으므로 뺀다.
fn check_after_prepare(file: &str, s: &Step, r: &mut Report) {
    let p = &mut r.problems;
    let coe = boolean(file, &s.fields, "continue-on-error").unwrap_or_else(|e| {
        p.push(e);
        None
    });
    if coe == Some(true) {
        return;
    }
    let expr = match scalar(file, &s.fields, "if") {
        Ok(Some(e)) => e,
        Ok(None) => return,
        Err(e) => return p.push(e),
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
            let mut prepared = false;
            for (k, s) in job.steps.iter().enumerate() {
                if is_checkout(file, s, &mut r.problems) {
                    match checkout_clean(file, s, &mut r.problems) {
                        Some(true) => explicit_clean_true += 1,
                        Some(false) => check_cached_checkout(file, job, k, &mut r),
                        None => {}
                    }
                }
                if is_prepare(file, s, &mut r.problems) {
                    prepared = true;
                    check_prepare_body(file, s, &mut r);
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
    assert_caught(&all, "prepare 스텝이 준비 스크립트를 부르지 않는다");
    let all = mutated(
        "crossplatform-check.yml",
        "          & $bash scripts/ci-prepare-workspace.sh\n",
        "          & $bash scripts/other.sh\n",
    );
    assert_caught(&all, "prepare 스텝이 준비 스크립트를 부르지 않는다");
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
