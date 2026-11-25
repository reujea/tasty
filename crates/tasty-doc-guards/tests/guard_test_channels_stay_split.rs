//! doc-guards의 Windows 컴파일 경로와 Ubuntu·Windows 실행 경로를 워크플로 소스에서 확인한다.
//! 러너 라벨과 명령 문자열을 검색하며 실제 러너 OS나 실행 성공을 검증하지 않는다.
//! 자동 잡 분류도 공유 파서의 모델을 따른다. 워크플로의 변경은 docs/dev-guide/ci-gates.md와 함께 검토한다.

use std::path::{Path, PathBuf};

use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};
use tasty_doc_guards::workflow_triggers::automatic_job_bodies;

const DOC: &str = "docs/dev-guide/ci-gates.md";

/// 워크플로 파일 수집의 하한.
const WORKFLOW_FLOOR: Floor = Floor {
    min: 8,
    measured: 11,
    measured_on: "2026-09-07",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::NEVER_COUNTED,
    why_this_gap: "워크플로 yml 수집에서 소수 파일의 통폐합을 허용하는 하한 8이다. 미달하면 실제 파일 감소와 수집 범위를 확인한다.",
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("레포 루트")
        .to_path_buf()
}

fn automatic_jobs() -> Vec<(String, String)> {
    automatic_jobs_under(&repo_root().join(".github/workflows"), &WORKFLOW_FLOOR)
}

/// 작은 합성 디렉터리도 같은 수집을 사용하도록 경로·하한을 인자로 받는다. .yml만 읽는다.
fn automatic_jobs_under(dir: &Path, floor: &Floor) -> Vec<(String, String)> {
    let walked = walk_with_floor(dir, dir, floor, Descend::Everything, &|w| {
        w.rel.ends_with(".yml")
    })
    .unwrap_or_else(|why| panic!("{why}"));

    let mut out = Vec::new();
    for w in walked {
        // 순회 하한은 파일을 찾았는지만 보므로 본문 읽기 실패도 별도로 보고한다.
        let text = std::fs::read_to_string(&w.path).unwrap_or_else(|e| {
            panic!(
                "워크플로 {}를 읽지 못했다: {e}. 파일 수집 후 본문 읽기도 성공해야 한다.",
                w.rel
            )
        });
        for body in
            automatic_job_bodies(&w.rel, &text).unwrap_or_else(|e| panic!("{}", e.join("\n")))
        {
            out.push((w.rel.clone(), body));
        }
    }
    out
}

/// 설명에 적힌 플래그를 명령으로 오해하지 않도록 첫 # 뒤를 제거한다. YAML 문자열은 정밀하게 구분하지 않는다.
fn commands_only(body: &str) -> String {
    body.lines()
        .map(|l| match l.find('#') {
            Some(at) => &l[..at],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn runs_on_windows(body: &str) -> bool {
    body.lines()
        .filter(|l| l.trim_start().starts_with("runs-on:"))
        .any(|l| l.contains("Windows") || l.contains("windows"))
}

fn runs_on_ubuntu(body: &str) -> bool {
    body.lines()
        .filter(|l| l.trim_start().starts_with("runs-on:"))
        .any(|l| l.contains("ubuntu"))
}

/// Windows 러너 라벨과 주석 제거 후 --all-targets 문자열을 함께 찾는다.
fn windows_compile_jobs(jobs: &[(String, String)]) -> Vec<&str> {
    jobs.iter()
        .filter(|(_, b)| runs_on_windows(b) && commands_only(b).contains("--all-targets"))
        .map(|(n, _)| n.as_str())
        .collect()
}

/// doc-guards 패키지 테스트 호출을 찾되 특정 bin만 선택한 경우는 통합 가드 실행으로 세지 않는다.
fn guard_test_jobs(jobs: &[(String, String)]) -> Vec<&(String, String)> {
    jobs.iter()
        .filter(|(_, b)| {
            let cmds = commands_only(b);
            cmds.contains("cargo test")
                && cmds.contains("-p tasty-doc-guards")
                && !cmds.contains("--bin ")
        })
        .collect()
}

/// 빈 잡 수집을 오류로 처리한다. 일부 워크플로 누락까지 전부 검출하는 조건은 아니다.
#[test]
fn the_compile_channel_for_guard_tests_still_exists_on_windows() {
    let jobs = automatic_jobs();
    assert!(
        !jobs.is_empty(),
        "자동으로 분류된 잡을 읽지 못했다. 파일 수집과 automatic_job_bodies의 헤더·조건 판독을 확인한다."
    );

    let compilers = windows_compile_jobs(&jobs);
    assert!(
        !compilers.is_empty(),
        "Windows 러너와 --all-targets가 함께 있는 잡을 찾지 못했다. 실제 컴파일 경로를 확인하고 {DOC}의 설명을 갱신한다."
    );
}

#[test]
fn the_execution_channel_for_guard_tests_spans_ubuntu_and_windows() {
    let jobs = automatic_jobs();
    let runners = guard_test_jobs(&jobs);
    assert!(
        !runners.is_empty(),
        "doc-guards 패키지 테스트 호출을 찾지 못했다. 판독과 실행 구성을 확인하고 {DOC}와 맞춘다."
    );

    assert!(
        runners.iter().any(|(_, b)| runs_on_ubuntu(b)),
        "Ubuntu 러너의 doc-guards 실행 호출을 찾지 못했다. 실제 실행 경로와 {DOC}를 확인한다."
    );

    assert!(
        runners.iter().any(|(_, b)| runs_on_windows(b)),
        "Windows 러너의 doc-guards 실행 호출을 찾지 못했다. 경로 구분자·줄 끝 처리를 Windows에서도 검증할 경로와 {DOC}를 함께 확인한다."
    );
}

/// 실제 파일 없이 잡 이름과 본문을 합성해 러너·명령 판독을 검증한다.
fn jobs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(n, b)| ((*n).to_string(), (*b).to_string()))
        .collect()
}

#[test]
fn the_readers_look_at_commands_and_at_the_runs_on_line_only() {
    let roster = jobs(&[
        (
            "zone-alpha.yml",
            "    runs-on: [self-hosted, Windows]\n    steps:\n      - name: parity with the ubuntu job\n        run: cargo clippy --workspace --all-targets --locked\n",
        ),
        (
            "zone-beta.yml",
            "    runs-on: windows-latest\n    steps:\n      - run: cargo check   # --all-targets 는 여기선 안 쓴다\n",
        ),
        (
            "zone-gamma.yml",
            "    runs-on: ubuntu-latest\n    steps:\n      - name: mirror of the Windows job\n        run: cargo clippy --all-targets\n",
        ),
    ]);

    assert_eq!(
        windows_compile_jobs(&roster),
        vec!["zone-alpha.yml"],
        "주석 안의 낱말과 스텝 이름 속 러너 이름은 둘 다 안 세야 한다"
    );

    assert!(
        runs_on_windows(&roster[1].1),
        "runs-on 줄의 windows 를 못 셌다"
    );
    assert!(
        !runs_on_windows(&roster[2].1),
        "스텝 이름 속 `Windows` 를 러너로 셌다"
    );
    assert!(
        runs_on_ubuntu(&roster[2].1),
        "runs-on 줄의 ubuntu 를 못 셌다"
    );
    assert!(
        !runs_on_ubuntu(&roster[0].1),
        "runs-on 줄이 아닌 곳의 ubuntu 를 러너로 셌다"
    );

    assert!(
        !commands_only(&roster[1].1).contains("--all-targets"),
        "주석을 안 뗐다 — 규칙을 *설명하는* 주석이 명령으로 읽힌다"
    );
    assert!(
        commands_only(&roster[0].1).contains("--all-targets"),
        "주석을 떼면서 명령까지 지웠다"
    );
}

/// 특정 bin 테스트는 통합 가드를 실행하지 않으므로 패키지 실행과 구분한다.
#[test]
fn the_runner_roster_excludes_the_judge_build_and_the_other_package() {
    let roster = jobs(&[
        (
            "zone-ubuntu.yml",
            "    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test -p tasty-doc-guards --locked\n",
        ),
        (
            "zone-windows.yml",
            "    runs-on: [self-hosted, Windows]\n    steps:\n      - run: cargo test -p tasty-doc-guards --locked --no-fail-fast\n",
        ),
        (
            "zone-judge.yml",
            "    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test -p tasty-doc-guards --bin mask-source\n",
        ),
        (
            "zone-other.yml",
            "    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test -p tasty --locked\n",
        ),
    ]);

    let names: Vec<&str> = guard_test_jobs(&roster)
        .iter()
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["zone-ubuntu.yml", "zone-windows.yml"],
        "판정기 빌드와 다른 크레이트는 실행 채널이 아니다"
    );

    let runners = guard_test_jobs(&roster);
    assert!(
        runners.iter().any(|(_, b)| runs_on_ubuntu(b)),
        "ubuntu 채널을 못 골랐다"
    );
    assert!(
        runners.iter().any(|(_, b)| runs_on_windows(b)),
        "Windows 채널을 못 골랐다"
    );

    assert!(
        windows_compile_jobs(&roster).is_empty(),
        "실행 채널을 컴파일 채널로 셌다"
    );
}
