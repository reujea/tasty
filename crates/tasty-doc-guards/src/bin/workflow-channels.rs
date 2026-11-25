//! workflow_triggers의 자동 실행 판정을 셸에서도 같은 구현으로 사용하도록 제공한다.
//!
//! 파일별 출력은 탭으로 구분한다:
//! <파일> <push> <path_filtered> <tags_only> <자동잡> <수동전용잡>
//! 마지막에는 필터 없는 채널의 named=, packages=, whole_workspace=를 출력한다.
//! yes/no와 정수·목록을 읽어 위반 여부를 판단하는 것은 호출자의 역할이다.
//!
//! 종료코드 0은 결과 출력, 2는 루트 부재·워크플로 없음·파일 읽기 실패·on 선언 판독 실패·잡 구조 판독 실패다.
//! 2이면 stdout에 아무것도 내지 않고, 판독하지 못한 파일(잡 구조는 파일:행)을 stderr에 적는다.

use std::path::{Path, PathBuf};

use tasty_doc_guards::workflow_triggers::{
    automatic_job_bodies, filter_free_coverage, job_header_count, push_trigger,
};

const OWN_SOURCE: &str = include_str!("workflow-channels.rs");
const OWN_REL: &str = "crates/tasty-doc-guards/src/bin/workflow-channels.rs";

fn check_fresh(root: &Path) -> ! {
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

fn yn(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--check-fresh") {
        let root = args
            .get(1)
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        check_fresh(&root);
    }
    let root = args
        .first()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let dir = root.join(".github/workflows");

    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("워크플로 디렉토리를 못 읽었다: {}", dir.display());
        std::process::exit(2);
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        eprintln!("워크플로가 0 개다 — 빈 모수를 0 으로 돌려주지 않는다");
        std::process::exit(2);
    }

    // 모든 파일을 본 뒤 하나라도 판독하지 못했으면 결과를 내지 않는다. 한 파일의 `on:`과 잡 구조 오류는 함께 알린다.
    let mut out = String::new();
    let mut bad: Vec<String> = Vec::new();
    for path in &paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                bad.push(format!("{name}: 못 읽었다: {e}"));
                continue;
            }
        };
        let trigger = push_trigger(&text);
        if trigger.is_none() {
            bad.push(format!(
                "{name}: `on:` 을 못 읽었다 — 판정 불가는 통과가 아니다"
            ));
        }
        let counts = automatic_job_bodies(&name, &text)
            .and_then(|auto| job_header_count(&name, &text).map(|all| (auto.len(), all)));
        let (auto, all) = match counts {
            Ok(c) => c,
            Err(errs) => {
                bad.extend(errs);
                continue;
            }
        };
        let Some(t) = trigger else {
            continue;
        };
        out.push_str(&format!(
            "{name}\t{}\t{}\t{}\t{auto}\t{}\n",
            yn(t.present),
            yn(t.path_filtered),
            yn(t.tags_only),
            all.saturating_sub(auto),
        ));
    }
    if !bad.is_empty() {
        for line in &bad {
            eprintln!("{line}");
        }
        std::process::exit(2);
    }

    match filter_free_coverage(&dir) {
        Ok(c) => {
            out.push_str(&format!(
                "named={}\n",
                c.named.iter().cloned().collect::<Vec<_>>().join(",")
            ));
            out.push_str(&format!(
                "packages={}\n",
                c.packages.iter().cloned().collect::<Vec<_>>().join(",")
            ));
            out.push_str(&format!("whole_workspace={}\n", yn(c.whole_workspace)));
        }
        Err(bad) => {
            eprintln!("판독하지 못한 워크플로: {}", bad.join(", "));
            std::process::exit(2);
        }
    }
    print!("{out}");
}
