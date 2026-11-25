//! 모든 워크플로 잡에 정수 `timeout-minutes`가 있는지 확인하고, 그 값을 ci-gates.md의
//! "잡 시간 상한" 표와 양방향으로 대조한다. 실측값과 배수의 계산은 검사하지 않는다.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};
use tasty_doc_guards::workflow_triggers::job_spans;

const DOC: &str = "docs/dev-guide/ci-gates.md";
const SECTION: &str = "### 잡 시간 상한";

/// 파일별 잡 목록을 표와 대조하므로 순회 하한은 빈 결과만 막는다.
const LIVENESS: Floor = Floor {
    min: 1,
    measured: 11,
    measured_on: "2026-10-08",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "8b71d6256 — git ls-tree -r --name-only로 .github/workflows/의 yml 파일 11개를 셌다.",
    ),
    why_this_gap: "잡 누락은 표와의 차집합에서 검출한다. 순회 하한은 파일을 하나도 읽지 못한 경우만 막는다.",
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("레포 루트")
        .to_path_buf()
}

type Cell = (String, String);

/// 잡 본문에서 잡 수준(네 칸 들여쓰기)의 `timeout-minutes` 값을 모두 읽는다.
/// 단계 수준의 값과 주석 줄은 세지 않는다.
fn job_level_timeouts(body: &str) -> Vec<String> {
    body.lines()
        .filter(|l| l.starts_with("    ") && !l.starts_with("     "))
        .filter_map(|l| l.trim().strip_prefix("timeout-minutes:"))
        .map(|v| v.split('#').next().unwrap_or("").trim().to_string())
        .collect()
}

fn workflows() -> BTreeMap<Cell, Vec<String>> {
    let dir = repo_root().join(".github/workflows");
    let walked = walk_with_floor(&dir, &dir, &LIVENESS, Descend::Everything, &|w| {
        w.rel.ends_with(".yml")
    })
    .unwrap_or_else(|why| panic!("{why}"));
    let mut out = BTreeMap::new();
    for w in walked {
        let text = std::fs::read_to_string(&w.path)
            .unwrap_or_else(|e| panic!("워크플로 {} 를 못 읽었다: {e}", w.rel));
        for job in job_spans(&w.rel, &text).unwrap_or_else(|e| panic!("{}", e.join("\n"))) {
            out.insert((w.rel.clone(), job.name), job_level_timeouts(&job.body));
        }
    }
    out
}

/// 표 영역은 절 제목부터 다음 제목 직전까지다. 머리글과 구분선은 건너뛴다.
fn guide_table(doc: &str) -> BTreeMap<Cell, String> {
    let start = doc
        .find(SECTION)
        .unwrap_or_else(|| panic!("{DOC} 에 `{SECTION}` 절이 없다"));
    let rest = &doc[start + SECTION.len()..];
    let end = rest.find("\n#").unwrap_or(rest.len());
    let mut out = BTreeMap::new();
    for line in rest[..end].lines() {
        let cols: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if !line.trim_start().starts_with('|') || cols.len() != 7 {
            continue;
        }
        if cols[0] == "워크플로" || cols[0].starts_with("---") {
            continue;
        }
        let prev = out.insert(
            (cols[0].to_string(), cols[1].to_string()),
            cols[6].to_string(),
        );
        assert!(prev.is_none(), "표에 같은 잡이 두 번 있다: {line}");
    }
    out
}

#[test]
fn every_job_has_the_timeout_the_guide_lists() {
    let jobs = workflows();
    let doc = std::fs::read_to_string(repo_root().join(DOC))
        .unwrap_or_else(|e| panic!("{DOC} 를 못 읽었다: {e}"));
    let table = guide_table(&doc);
    assert!(
        !table.is_empty(),
        "{DOC} 의 `{SECTION}` 표에서 행을 하나도 읽지 못했다"
    );

    let mut problems = Vec::new();
    for ((file, job), values) in &jobs {
        match values.as_slice() {
            [v] if v.parse::<u32>().is_ok_and(|n| n > 0) => {}
            [] => problems.push(format!("{file} / {job}: timeout-minutes 가 없다")),
            other => problems.push(format!(
                "{file} / {job}: 잡 수준 timeout-minutes 가 양의 정수 하나가 아니다: {other:?}"
            )),
        }
        match (values.first(), table.get(&(file.clone(), job.clone()))) {
            (_, None) => problems.push(format!("{file} / {job}: {DOC} 표에 행이 없다")),
            (Some(v), Some(t)) if v != t => {
                problems.push(format!("{file} / {job}: 워크플로 {v} · 표 {t} 가 다르다"))
            }
            _ => {}
        }
    }
    for (file, job) in table.keys() {
        if !jobs.contains_key(&(file.clone(), job.clone())) {
            problems.push(format!(
                "{file} / {job}: 표에만 있고 워크플로에 없는 잡이다"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "잡 시간 상한이 워크플로와 {DOC} 표에서 어긋난다. 실측과 배수 규칙은 그 절을 따른다.\n{}",
        problems.join("\n")
    );
}
