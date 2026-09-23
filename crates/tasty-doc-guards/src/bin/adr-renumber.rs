//! ADR 을 새 번호로 옮기고, 레포 전체에서 그 번호를 부르는 자리를 함께 고친다. 판정 규칙은
//! [`tasty_doc_guards::adr_renumber`] 에 있고, 여기는 파일 · git 부수효과와 보고다.
//!
//! ```text
//! cargo run -p tasty-doc-guards --bin adr-renumber -- <매핑 파일> [--report <경로>]           # dry-run(기본)
//! cargo run -p tasty-doc-guards --bin adr-renumber -- <매핑 파일> --write [--report <경로>]   # 쓴다
//! ```
//!
//! `--root <디렉토리>` 로 레포 루트를 줄 수 있다(생략하면 현재 디렉토리). 매핑 형식 · 형태
//! 목록 · 못 보는 것은 `docs/dev-guide/adr-renumber.md`.
//!
//! ## 종료코드
//!
//! 0 = dry-run 이 계획을 세웠고 막는 것이 없다, 또는 `--write` 를 마쳤다.
//! 1 = **삭제(DELETE)되는 ADR 을 고치는 형태로 부르는 자리가 남아 있다.** dry-run 은 그 목록을
//! 내고 1 로 끝나고, `--write` 는 아무것도 안 쓰고 1 로 끝난다 — 그 자리를 남긴 채 번호를
//! 옮기면 옛 번호가 **다른 ADR 을 조용히 가리킨다.** 사람이 그 자리를 내용으로 바꾼 뒤 다시
//! 돌린다.
//! 2 = 할 수 없다 — 모르는 인자 · 매핑 오류 · ADR 수집 실패 · 번호 중복 · git 실패 ·
//! `--write` 인데 작업 트리가 깨끗하지 않다 · 파일을 못 씀.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use tasty_doc_guards::adr_index::{INDEX, collect, duplicate_numbers, render_index};
use tasty_doc_guards::adr_renumber::{Form, Hit, Target, apply, line_of, parse_mapping, scan};

/// 고치지 않고 보고만 하는 vendored 사본. 고칠 곳은 원격 디자인 킷이다(`site/vendor/README.md`).
/// 그 README 자신은 손으로 쓰는 파일이라 범위 안이다.
const VENDOR: &str = "site/vendor/";
const VENDOR_OWN: &str = "site/vendor/README.md";

/// 문자열 픽스처에 **가짜 ADR** 을 담은 파일. 파일명 형태(slug 가 실재 파일과 끝까지 같아야
/// 한다 — 픽스처의 가짜 slug 는 안 걸린다)만 고치고, 나머지 형태는 보고만 한다. 접두 형태를
/// 고치면 픽스처 안의 짝(`[ADR-NNNN](NNNN-x.md)` 과 기대값 `"NNNN"`)이 한쪽만 바뀐다.
const FIXTURES: &[&str] = &[
    "crates/tasty-doc-guards/src/adr_index.rs",
    "crates/tasty-doc-guards/tests/adr_index_parity.rs",
    "crates/tasty-doc-guards/tests/adr_renumber_bin.rs",
    "crates/tasty-doc-guards/src/adr_renumber.rs",
    "crates/tasty-doc-guards/src/bin/adr-renumber.rs",
];

fn fail(msg: &str) -> ! {
    eprintln!("[adr-renumber] {msg}");
    std::process::exit(2)
}

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap_or_else(|e| fail(&format!("git {} 를 못 돌렸다: {e}", args.join(" "))));
    if !out.status.success() {
        fail(&format!(
            "git {} 실패: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    out.stdout
}

#[derive(Default)]
struct Plan {
    /// 형태 → (매핑에 있는 번호를 본 자리, 그중 고칠 자리).
    seen: BTreeMap<Form, (usize, usize)>,
    edits: BTreeMap<String, String>,
    blocking: Vec<String>,
    bare: Vec<String>,
    stale_slug: Vec<String>,
    collisions: Vec<String>,
    vendor: Vec<String>,
    fixture: Vec<String>,
    non_utf8: usize,
    files: usize,
}

fn target_label(t: Option<&Target>) -> String {
    match t {
        Some(Target::Num(n)) => n.clone(),
        Some(Target::Delete) => "DELETE".into(),
        None => "매핑 밖".into(),
    }
}

fn entry(path: &str, text: &str, h: &Hit, t: Option<&Target>) -> String {
    let (n, line) = line_of(text, h.start);
    format!(
        "{path}:{n}: [{} {}→{}] {}",
        h.form.label(),
        h.num,
        target_label(t),
        line.trim()
    )
}

fn build_plan(
    root: &Path,
    files: &[String],
    stems: &BTreeMap<String, String>,
    map: &BTreeMap<String, Target>,
    deleted_files: &BTreeSet<String>,
) -> Plan {
    let targets: BTreeSet<&str> = map
        .values()
        .filter_map(|t| match t {
            Target::Num(n) => Some(n.as_str()),
            Target::Delete => None,
        })
        .collect();
    let mut plan = Plan::default();
    for path in files {
        if deleted_files.contains(path) {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(path)) else {
            // 추적되지만 작업 트리에 없는 파일(삭제 후 미커밋)은 볼 본문이 없다.
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            plan.non_utf8 += 1;
            continue;
        };
        plan.files += 1;
        let vendor = path.starts_with(VENDOR) && path != VENDOR_OWN;
        let fixture = FIXTURES.contains(&path.as_str());
        let hits = scan(&text, stems, path == INDEX);
        let mut to_apply = Vec::new();
        for h in &hits {
            let t = map.get(&h.num);
            let changed = matches!(t, Some(Target::Num(n)) if *n != h.num);
            let deleted = matches!(t, Some(Target::Delete));
            // 맨 네 자리는 겹침으로 안 센다 — 매핑 밖 네 자리는 대부분 ADR 이 아니다.
            let colliding = t.is_none() && h.form != Form::Bare && targets.contains(h.num.as_str());
            let line = || entry(path, &text, h, t);
            if colliding {
                plan.collisions.push(line());
                continue;
            }
            match h.form {
                Form::Bare => {
                    if changed || deleted {
                        plan.bare.push(line());
                    }
                }
                Form::UnknownFile => {
                    if changed || deleted {
                        plan.stale_slug.push(line());
                    }
                }
                form => {
                    if t.is_none() {
                        continue;
                    }
                    let slot = plan.seen.entry(form).or_default();
                    slot.0 += 1;
                    if !(changed || deleted) {
                        continue;
                    }
                    if vendor {
                        plan.vendor.push(line());
                    } else if fixture && form != Form::FileName {
                        plan.fixture.push(line());
                    } else if deleted {
                        plan.blocking.push(line());
                    } else {
                        slot.1 += 1;
                        to_apply.push(h.clone());
                    }
                }
            }
        }
        if !to_apply.is_empty() {
            plan.edits
                .insert(path.clone(), apply(&text, &to_apply, map));
        }
    }
    plan
}

fn section(out: &mut String, title: &str, why: &str, rows: &[String]) {
    out.push_str(&format!("\n## {title} — {} 자리\n\n{why}\n\n", rows.len()));
    for r in rows {
        out.push_str(&format!("- {r}\n"));
    }
}

fn render_report(plan: &Plan, moves: &[(String, String)], deleted: &BTreeSet<String>) -> String {
    let mut out = String::from("# ADR 재번호 보고\n\n");
    out.push_str(&format!(
        "훑은 파일 {} · UTF-8 아님(건너뜀) {} · 옮길 ADR {} · 지울 ADR {} · 고칠 파일 {}\n",
        plan.files,
        plan.non_utf8,
        moves.len(),
        deleted.len(),
        plan.edits.len()
    ));
    out.push_str("\n| 형태 | 매핑에 있는 번호를 본 자리 | 고칠 자리 |\n|---|---|---|\n");
    for (f, (seen, fix)) in &plan.seen {
        out.push_str(&format!("| {} | {seen} | {fix} |\n", f.label()));
    }
    section(
        &mut out,
        "삭제되는 ADR 을 부르는 자리 (막는다)",
        "이 자리가 하나라도 있으면 `--write` 는 아무것도 안 쓴다. 옛 번호를 남긴 채 옮기면 그 번호가 \
         다른 ADR 을 조용히 가리킨다. 내용으로 바꾼 뒤 다시 돌린다.",
        &plan.blocking,
    );
    section(
        &mut out,
        "매핑 밖 번호가 새 번호와 겹치는 자리",
        "지금 없는 ADR(이미 죽은 인용 · 픽스처)을 부르는데, 그 번호를 재번호 뒤 다른 ADR 이 받는다. \
         도구는 안 고친다 — 고치기 전까지 그 자리는 **엉뚱한 ADR 을 가리킨다.**",
        &plan.collisions,
    );
    section(
        &mut out,
        "맨 네 자리 (안 고친다)",
        "옮기거나 지우는 번호와 같은 네 자리인데 좌표를 스스로 들고 있지 않은 자리. 날짜 · 측정값 · \
         산문 속 ADR 인용이 섞여 있고 도구는 가르지 않는다. 사람이 읽고 고친다.",
        &plan.bare,
    );
    section(
        &mut out,
        "slug 가 실재 파일과 다른 파일명 (안 고친다)",
        "`NNNN-<slug>.md` 인데 그 파일이 없다. 이미 죽은 링크거나 픽스처다.",
        &plan.stale_slug,
    );
    section(
        &mut out,
        "vendored 사본 (안 고친다)",
        "`site/vendor/` 는 원격 디자인 킷의 사본이다. 원격을 고치고 다시 받는다.",
        &plan.vendor,
    );
    section(
        &mut out,
        "픽스처 파일의 파일명 밖 형태 (안 고친다)",
        "가짜 ADR 을 담은 파일이라 파일명 형태만 고쳤다. 주석 속 실제 인용이 섞여 있으면 사람이 고친다.",
        &plan.fixture,
    );
    out
}

fn main() {
    let mut write = false;
    let mut root = PathBuf::from(".");
    let mut report: Option<PathBuf> = None;
    let mut mapping: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--write" => write = true,
            "--dry-run" => write = false,
            "--root" => {
                root = args
                    .next()
                    .map_or_else(|| fail("--root 뒤에 경로가 없다"), PathBuf::from)
            }
            "--report" => {
                report = Some(
                    args.next()
                        .map_or_else(|| fail("--report 뒤에 경로가 없다"), PathBuf::from),
                );
            }
            other if other.starts_with("--") => fail(&format!("모르는 인자: {other}")),
            other => mapping = Some(PathBuf::from(other)),
        }
    }
    let mapping = mapping.unwrap_or_else(|| fail("매핑 파일을 주지 않았다"));
    let adrs = collect(&root).unwrap_or_else(|e| fail(&e));
    if adrs.is_empty() {
        fail("ADR 을 하나도 못 찾았다 — 루트가 틀렸다");
    }
    let dups = duplicate_numbers(&adrs);
    if !dups.is_empty() {
        fail(&format!(
            "번호가 겹치는 ADR 이 있다 — 먼저 푼다:\n  {}",
            dups.join("\n  ")
        ));
    }
    let stems: BTreeMap<String, String> = adrs
        .iter()
        .map(|a| (a.num.clone(), a.file.trim_end_matches(".md").to_string()))
        .collect();
    let existing: BTreeSet<String> = stems.keys().cloned().collect();
    let text = std::fs::read_to_string(&mapping)
        .unwrap_or_else(|e| fail(&format!("매핑 파일을 못 읽었다: {e}")));
    let map = parse_mapping(&text, &existing)
        .unwrap_or_else(|errs| fail(&format!("매핑이 틀렸다:\n  {}", errs.join("\n  "))));

    let dir = tasty_doc_guards::adr_index::ADR_DIR;
    let mut moves = Vec::new();
    let mut deleted = BTreeSet::new();
    for a in &adrs {
        match &map[&a.num] {
            Target::Num(n) if *n != a.num => moves.push((
                format!("{dir}/{}", a.file),
                format!("{dir}/{n}{}", &a.file[4..]),
            )),
            Target::Num(_) => {}
            Target::Delete => {
                deleted.insert(format!("{dir}/{}", a.file));
            }
        }
    }
    let files: Vec<String> = String::from_utf8_lossy(&git(&root, &["ls-files", "-z"]))
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if files.is_empty() {
        fail("git ls-files 가 0 개다 — 빈 모수는 측정 실패다");
    }
    let plan = build_plan(&root, &files, &stems, &map, &deleted);
    let body = render_report(&plan, &moves, &deleted);
    match &report {
        Some(p) => {
            std::fs::write(p, &body).unwrap_or_else(|e| fail(&format!("보고 파일을 못 썼다: {e}")))
        }
        None => print!("{body}"),
    }
    let fixes: usize = plan.seen.values().map(|v| v.1).sum();
    eprintln!(
        "[adr-renumber] {} · 훑은 파일 {} · 옮길 ADR {} · 지울 ADR {} · 고칠 자리 {fixes} (파일 {}) · \
         막는 자리 {} · 겹침 {} · 맨 네 자리 {} · vendor {} · 픽스처 {}",
        if write { "write" } else { "dry-run" },
        plan.files,
        moves.len(),
        deleted.len(),
        plan.edits.len(),
        plan.blocking.len(),
        plan.collisions.len(),
        plan.bare.len(),
        plan.vendor.len(),
        plan.fixture.len(),
    );
    if !plan.blocking.is_empty() {
        eprintln!(
            "[adr-renumber] 삭제되는 ADR 을 부르는 자리가 남았다 — 보고의 첫 절. 아무것도 안 썼다."
        );
        std::process::exit(1);
    }
    if !write {
        return;
    }
    if !git(&root, &["status", "--porcelain"]).is_empty() {
        fail(
            "작업 트리가 깨끗하지 않다 — 되돌릴 수 있고 diff 가 이 도구의 것만이도록 커밋부터 한다",
        );
    }
    for (path, new) in &plan.edits {
        std::fs::write(root.join(path), new)
            .unwrap_or_else(|e| fail(&format!("{path} 를 못 썼다: {e}")));
    }
    for d in &deleted {
        git(&root, &["rm", "-q", "--", d]);
    }
    // 늘 두 단계로 옮긴다 — 새 경로가 아직 옮기기 전의 다른 ADR 자리일 수 있다(같은 slug 의
    // 교환). 한 단계로 옮기면 그 순서를 도구가 풀어야 하고, 틀리면 git mv 가 중간에 멈춘다.
    for (from, _) in &moves {
        git(&root, &["mv", "--", from, &format!("{from}.renumber-tmp")]);
    }
    for (from, to) in &moves {
        git(&root, &["mv", "--", &format!("{from}.renumber-tmp"), to]);
    }
    let adrs = collect(&root).unwrap_or_else(|e| fail(&e));
    let index_path = root.join(INDEX);
    let index = std::fs::read_to_string(&index_path)
        .unwrap_or_else(|e| fail(&format!("{INDEX} 를 못 읽었다: {e}")))
        .replace("\r\n", "\n");
    let rendered = render_index(&index, &adrs)
        .unwrap_or_else(|errs| fail(&format!("인덱스 마커가 깨졌다:\n  {}", errs.join("\n  "))));
    std::fs::write(&index_path, rendered)
        .unwrap_or_else(|e| fail(&format!("{INDEX} 를 못 썼다: {e}")));
    eprintln!("[adr-renumber] 썼다. 다음: adr-index --check · cargo test -p tasty-doc-guards");
}
