//! workflow-channels 출력의 트리거·자동 잡 수·검사 범위가 공유 라이브러리 결과와 같은지 확인한다.
//! 두 구현이 같은 오류를 내면 일치하므로 파서의 정확성 자체를 증명하지는 않는다.
//! 출력 행 하한과 경로 필터 유무의 두 결과를 확인하지만 일부 행의 누락까지 모두 보장하지는 않는다.

// 이유: 테스트의 반환값 무시는 제품 코드의 lint 예외 명부에 포함하지 않는다.
#![allow(clippy::let_underscore_must_use)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use tasty_doc_guards::workflow_triggers::{
    automatic_job_bodies, filter_free_coverage, push_trigger,
};

/// 2026-09-07 de0572359에서 워크플로 파일·출력 행 모두 11개였고 하한은 8이다.
/// 빈 디렉터리는 바이너리가 rc2로 거부하며, 이 하한은 실행 성공 후 출력이 크게 줄었는지 확인한다.
/// 여유 안의 누락은 통과할 수 있다. 워크플로를 의도적으로 삭제할 때 파일 수와 출력 행을 함께 확인한다.
const MIN_ROWS: usize = 8;

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_workflow-channels"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("판정기를 실행할 수 없다: {e}"));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_exposed_judge_reports_the_same_population_as_the_library() {
    let root = repo_root();
    let (rc, stdout, stderr) = run(&[&root.to_string_lossy()]);
    assert_eq!(rc, 0, "판정기가 실패했다(rc {rc}): {stderr}");

    let mut rows = 0usize;
    let mut filtered = 0usize;
    let mut unfiltered = 0usize;
    for line in stdout.lines() {
        if !line.contains('\t') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 6, "행 형태가 바뀌었다: {line}");
        let (name, present, path_filtered, tags_only, auto, manual) =
            (f[0], f[1], f[2], f[3], f[4], f[5]);
        rows += 1;

        let text = std::fs::read_to_string(root.join(".github/workflows").join(name))
            .unwrap_or_else(|e| panic!("read {name}: {e}"));
        let t = push_trigger(&text)
            .unwrap_or_else(|| panic!("{name}: 라이브러리가 `on:` 을 못 읽었다"));
        assert_eq!(present == "yes", t.present, "{name}: push 판정이 갈렸다");
        assert_eq!(
            path_filtered == "yes",
            t.path_filtered,
            "{name}: 실행 파일과 라이브러리의 경로 필터 판정이 다르다"
        );
        assert_eq!(
            tags_only == "yes",
            t.tags_only,
            "{name}: 태그전용 판정이 갈렸다"
        );

        let lib_auto = automatic_job_bodies(name, &text)
            .unwrap_or_else(|e| panic!("{}", e.join("\n")))
            .len();
        assert_eq!(
            auto.parse::<usize>().unwrap_or(usize::MAX),
            lib_auto,
            "{name}: 실행 파일과 라이브러리의 자동 잡 수가 다르다"
        );
        // 수동 분류 수는 정수 형식만 확인한다. 전체 잡 수와의 차이까지 대조하지는 않는다.
        assert!(
            manual.parse::<usize>().is_ok(),
            "{name}: 수동 전용 잡 수가 정수가 아니다: {manual}"
        );

        if t.path_filtered {
            filtered += 1;
        } else if t.present {
            unfiltered += 1;
        }
    }

    println!("[노출 판정기] 워크플로 행 {rows} · 하한 {MIN_ROWS}");
    assert!(
        rows >= MIN_ROWS,
        "워크플로 출력이 {rows}행으로 하한 {MIN_ROWS}보다 적다. 파일 수와 출력 수집을 확인한다."
    );
    assert!(
        filtered > 0 && unfiltered > 0,
        "경로 필터 유무 중 한쪽 결과가 없다(있음 {filtered}·없음 {unfiltered}). 워크플로 구성과 판독을 확인한다."
    );
}

#[test]
fn the_exposed_judge_reports_the_same_coverage_as_the_library() {
    let root = repo_root();
    let (rc, stdout, stderr) = run(&[&root.to_string_lossy()]);
    assert_eq!(rc, 0, "판정기가 실패했다(rc {rc}): {stderr}");

    let field = |key: &str| -> String {
        stdout
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap_or_else(|| panic!("출력에 `{key}` 줄이 없다"))
            .to_string()
    };
    let split = |v: &str| -> BTreeSet<String> {
        v.split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    };

    let lib = filter_free_coverage(&root.join(".github/workflows"))
        .unwrap_or_else(|bad| panic!("판독하지 못한 워크플로: {bad:?}"));

    assert_eq!(
        split(&field("named=")),
        lib.named,
        "--test로 선택된 타깃 목록이 실행 파일과 라이브러리에서 다르다"
    );
    assert_eq!(
        split(&field("packages=")),
        lib.packages,
        "-p로 선택된 패키지 목록이 실행 파일과 라이브러리에서 다르다"
    );
    assert_eq!(
        field("whole_workspace=") == "yes",
        lib.whole_workspace,
        "`--workspace` 판정이 갈렸다"
    );
    assert!(
        !lib.named.is_empty() || !lib.packages.is_empty() || lib.whole_workspace,
        "필터 없는 실행 경로를 수집하지 못했다"
    );
}

/// --check-fresh가 현재 소스로 빌드한 실행 파일을 유효하다고 판정하는지 확인한다.
#[test]
fn the_freshness_probe_answers() {
    let root = repo_root();
    let (rc, _, stderr) = run(&["--check-fresh", &root.to_string_lossy()]);
    assert_eq!(
        rc, 0,
        "현재 소스로 빌드한 실행 파일을 오래된 것으로 판정했다: {stderr}"
    );
}

/// 행 수 하한과 필터 유무 확인은 독립된 조건이다. 합성 입력에서 각각만 실패하는 경우와 빈 디렉터리 거부를 구별한다.
#[test]
fn the_row_floor_sees_a_collapsed_collection() {
    let root =
        std::env::temp_dir().join(format!("tasty-rowfloor-{}-{}", std::process::id(), line!()));
    let dir = root.join(".github/workflows");

    // 앞선 실행의 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let reset = || {
        // 지우고 다시 만든다. 없어서 나는 실패는 정보가 아니다.
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&dir).expect("임시 디렉토리를 못 만들었다");
    };

    let plant = |name: &str, filtered: bool| {
        let on = if filtered {
            "  push:\n    paths:\n      - 'src/**'\n"
        } else {
            "  push:\n    branches: [main]\n"
        };
        std::fs::write(
            dir.join(name),
            format!(
                "name: {name}\non:\n{on}jobs:\n  a:\n    runs-on: ubuntu-latest\n    \
                 steps:\n      - run: cargo test --workspace\n"
            ),
        )
        .expect("쓰기 실패");
    };

    // 실제 출력 검사와 같은 열 형식으로 읽는다.
    let measure = || -> (i32, usize, usize, usize) {
        let (rc, stdout, _) = run(&[&root.to_string_lossy()]);
        let (mut rows, mut filtered, mut unfiltered) = (0usize, 0usize, 0usize);
        for line in stdout.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 6 {
                continue;
            }
            rows += 1;
            if f[2] == "yes" {
                filtered += 1;
            } else if f[1] == "yes" {
                unfiltered += 1;
            }
        }
        (rc, rows, filtered, unfiltered)
    };

    reset();
    let (rc, rows, _, _) = measure();
    assert_eq!(rc, 2, "빈 워크플로 디렉터리를 오류로 처리해야 한다");
    assert_eq!(
        rows, 0,
        "거절했는데 행이 나오면 rc 와 출력이 다른 말을 하는 것이다"
    );

    reset();
    plant("unfiltered.yml", false);
    plant("filtered.yml", true);
    let (rc, rows, filtered, unfiltered) = measure();
    assert_eq!(
        rc, 0,
        "필터 유무가 하나씩 있는 코퍼스는 판정기가 답해야 한다"
    );
    assert_eq!(
        rows, 2,
        "놓은 만큼 행이 나와야 한다 — 아니면 이 대조가 판정기를 안 본다"
    );
    assert!(
        filtered > 0 && unfiltered > 0,
        "필터 유무 확인은 통과해야 행 수 하한만 실패하는 경우를 검증할 수 있다"
    );
    assert!(
        rows < MIN_ROWS,
        "작은 합성 입력의 행 수가 하한 미달이어야 한다"
    );

    reset();
    for i in 1..=9 {
        plant(&format!("f{i}.yml"), true);
    }
    let (rc, rows, filtered, unfiltered) = measure();
    assert_eq!(rc, 0, "판정기가 답해야 한다");
    assert!(
        rows >= MIN_ROWS,
        "행 수 하한은 통과해야 필터 종류 확인만 실패하는 경우를 검증할 수 있다"
    );
    assert!(
        filtered > 0 && unfiltered == 0,
        "필터 있는 워크플로만 넣었는데 다른 결과가 나왔다(있음 {filtered}·없음 {unfiltered})"
    );

    // 뒷정리 실패는 무시한다 — 임시 디렉토리라 남아도 다음 실행이 먼저 지우고, 여기서
    // 죽으면 위 단정의 결과가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&root);
}
