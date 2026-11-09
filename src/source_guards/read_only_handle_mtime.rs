//! File::open으로 연 읽기 전용 핸들의 mtime 변경은 Windows에서 권한 오류가 날 수 있다.
//! mtime을 바꿀 때는 쓰기 권한이 있는 핸들을 사용한다. 다른 플랫폼에서 통과해도 Windows 동작을 보장하지 않는다.
//!
//! 마스킹한 소스를 세미콜론으로 나눠 File::open과 set_modified/set_times가 함께 있는 구간을 찾는다.
//! 줄바꿈한 체인을 놓치지 않으려는 범위이며 실제로 같은 핸들인지까지 해석하지 않는다.
//! 변수에 받은 뒤 다음 구문에서 mtime을 바꾸는 형태는 추적하지 못한다.

use super::*;

/// mtime 변경이 모두 사라졌다면 검사의 필요성과 하한을 함께 검토한다.
const MIN_MTIME_WRITE_SITES: usize = 1;

/// 파일별 mtime 변경 수를 고정해 일부 파일의 누락·추가·건수 변화를 찾는다.
/// 같은 파일에서 한 사용처를 지우고 다른 곳을 추가해 수가 같으면 구별하지 못한다.
/// 타임스탬프와 무관한 내용 판정을 검증하려고 mtime을 바꾸는 시험 코드도 검사 대상이다.
/// 동작을 바꾸지 않는 시험 수정도 이 명부를 갱신해야 할 수 있다.
const EXPECTED_MTIME_SITES: &[(&str, usize)] = &[
    ("crates/tasty-host-plugin/src/builtin.rs", 1),
    (
        "crates/tasty-host-plugin/src/builtin/bundle_selection_tests.rs",
        1,
    ),
    ("crates/tasty-plugin-agent-common/src/prompt_file.rs", 1),
    // 원격 결과 정리 시험이 보존 기간·상한 판정을 보려고 결과 파일의 수정 시각을 쓰기 핸들(write(true))로 옛 시각으로 옮긴다.
    ("crates/tasty-plugin-agent-common/src/remote_results.rs", 2),
    // 계측 시험이 한 시간 전에 바뀐 대화 기록을 거르는지 보려고 쓰기 핸들로 수정 시각을 옮긴다.
    (
        "crates/tasty-plugin-agent-session/src/adapters/driven/rate_meter.rs",
        1,
    ),
    ("crates/tasty-plugin-claude/src/error_scan.rs", 1),
    ("crates/tasty-plugin-sdk/src/file_watch.rs", 1),
    // 복사한 파일에 원본의 수정 시각을 건다. 사본을 만든 쓰기 핸들(create_new)로 건다.
    ("src/app/file_jobs/copy_meta.rs", 1),
    // 사본 metadata 시험이 원본의 수정 시각을 옛 시각으로 쓰기 핸들에서 바꾼다.
    ("src/app/file_jobs/copy_meta_tests.rs", 1),
    // 썸네일·미리보기 시험이 같은 경로를 다른 수정 시각으로 바꾼 것처럼 쓰기 핸들로 수정 시각을 정한다.
    ("src/adapters/ui/surface/explorer/preview.rs", 1),
    ("src/adapters/ui/surface/explorer/thumbs.rs", 1),
    // 목록 시험이 크기·변경 표지 판정을 보려고 쓴 파일의 수정 시각을 쓰기 핸들(write(true))로 정한다.
    ("src/core/fs_list.rs", 1),
    // 이름 변경 이력 시험이 바꾼 뒤 항목을 고친 것처럼 쓰기 핸들로 수정 시각을 옮긴다.
    ("src/app/explorer_files/history_steps_tests.rs", 1),
    // 이력 시험이 옮겨 간 파일과 되돌린 뒤의 원본을 고친 것처럼 쓰기 핸들로 수정 시각을 옮긴다.
    ("src/app/explorer_files/history_tests.rs", 2),
    // 되돌리기 시험이 사본을 고친 것처럼 쓰기 핸들로 수정 시각을 옮긴다.
    ("src/app/file_jobs/job_tests.rs", 1),
    // 남은 원본 다시 지우기 시험이 같은 크기로 고친 원본의 수정 시각을 쓰기 핸들로 되돌린다.
    ("src/app/file_jobs/leftover_tests.rs", 1),
    // HTML 재개 시험이 같은 길이로 고친 문서의 수정 시각을 쓰기 핸들(write(true))로 되돌려 파일
    // 식별값이 그대로인 쓰기를 만든다.
    ("src/host_api/webview/scan_resume/tests.rs", 1),
];

fn scan_site_population() -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (path, text) in rust_sources() {
        let found = scan(&mask_non_code(&text));
        if found.sites > 0 {
            out.insert(path.to_string_lossy().replace('\\', "/"), found.sites);
        }
    }
    out
}

fn site_drift(actual: &BTreeMap<String, usize>) -> Vec<String> {
    let expected: BTreeMap<String, usize> = EXPECTED_MTIME_SITES
        .iter()
        .map(|(path, n)| ((*path).to_string(), *n))
        .collect();
    let mut drift = Vec::new();
    for (path, n) in &expected {
        match actual.get(path) {
            None => drift.push(format!("  사라짐: {path} (스냅샷 {n} 곳)")),
            Some(m) if m != n => {
                drift.push(format!("  개수 다름: {path} — 스냅샷 {n} · 실측 {m}"));
            }
            Some(_) => {}
        }
    }
    for (path, m) in actual {
        if !expected.contains_key(path) {
            drift.push(format!("  새로 생김: {path} ({m} 곳)"));
        }
    }
    drift
}

const READ_ONLY_OPEN: &str = "File::open(";
const MTIME_WRITES: &[&str] = &[".set_modified(", ".set_times("];

/// 줄 번호는 1부터 시작한다.
struct Scan {
    /// 세미콜론 구간별 set_modified/set_times의 첫 출현을 각각 센 수.
    sites: usize,
    violations: Vec<usize>,
}

fn scan(masked: &str) -> Scan {
    let mut out = Scan {
        sites: 0,
        violations: Vec::new(),
    };
    let mut stmt_start = 0usize;
    for (offset, _) in masked.match_indices(';').chain([(masked.len(), "")]) {
        let stmt = &masked[stmt_start..offset];
        for needle in MTIME_WRITES {
            let Some(rel) = stmt.find(needle) else {
                continue;
            };
            out.sites += 1;
            if stmt.contains(READ_ONLY_OPEN) {
                out.violations.push(line_of(masked, stmt_start + rel));
            }
        }
        stmt_start = offset + 1;
    }
    out
}

#[test]
fn mtime_is_never_written_through_a_read_only_handle() {
    let mut sites = 0usize;
    let mut violations = Vec::new();
    for (path, text) in rust_sources() {
        let found = scan(&mask_non_code(&text));
        sites += found.sites;
        for line in found.violations {
            violations.push(format!("{}:{line}", path.display()));
        }
    }
    assert!(
        sites >= MIN_MTIME_WRITE_SITES,
        "스캔 하한 미달: mtime 을 쓰는 호출을 {sites} 곳 찾았다(하한 \
         {MIN_MTIME_WRITE_SITES}). 정말 사라졌다면 이 하한을 함께 고쳐라"
    );
    let drift = site_drift(&scan_site_population());
    assert!(
        drift.is_empty(),
        "mtime 변경의 파일별 수가 명부와 다르다. 실제 사용처 변경과 수집 누락을 확인한다.\n{}",
        drift.join("\n")
    );
    assert!(
        violations.is_empty(),
        "`File::open` 은 읽기 접근만 얻는다 — 그 핸들로 mtime 을 쓰면 Windows 에서 \
         `PermissionDenied(os error 5)` 가 난다(Linux·macOS 는 통과해서 안 드러난다). \
         `std::fs::OpenOptions::new().write(true).open(..)` 로 열어라.\n  {}",
        violations.join("\n  ")
    );
}

mod exemption_mutations {
    //! 주석·문자열의 세미콜론이 호출 구간을 잘못 나누지 않는지 확인한다.

    use super::*;

    #[test]
    fn catches_a_chain_whose_semicolon_hides_in_a_string() {
        let src = "std::fs::File::open(&p.join(\"a;b\")).unwrap().set_modified(t).unwrap();\n";
        let found = scan(&mask_non_code(src));
        assert_eq!(found.sites, 1);
        assert_eq!(found.violations, vec![1]);
    }

    #[test]
    fn catches_a_chain_whose_semicolon_hides_in_a_comment() {
        let src = "std::fs::File::open(&p /* ; */).unwrap().set_modified(t).unwrap();\n";
        let found = scan(&mask_non_code(src));
        assert_eq!(found.sites, 1);
        assert_eq!(found.violations, vec![1]);
    }

    #[test]
    fn catches_a_chain_broken_across_lines_by_rustfmt() {
        let src =
            "std::fs::File::open(&stale)\n    .unwrap()\n    .set_modified(old)\n    .unwrap();\n";
        assert_eq!(scan(&mask_non_code(src)).violations, vec![3]);
    }

    #[test]
    fn does_not_flag_a_handle_opened_for_write() {
        let src =
            "std::fs::OpenOptions::new().write(true).open(&p).unwrap().set_modified(t).unwrap();\n";
        let found = scan(&mask_non_code(src));
        assert_eq!(found.sites, 1);
        assert!(found.violations.is_empty());
    }

    /// 세미콜론을 넘는 핸들 추적은 현재 지원하지 않는 범위다.
    #[test]
    fn intentionally_misses_a_handle_bound_across_statements() {
        let src = "let f = std::fs::File::open(&p).unwrap();\nf.set_modified(t).unwrap();\n";
        let found = scan(&mask_non_code(src));
        assert_eq!(found.sites, 1);
        assert!(
            found.violations.is_empty(),
            "세미콜론을 넘는 핸들 사용을 검출했다. 추적 범위를 넓혔다면 설명과 이 시험을 함께 갱신한다."
        );
    }

    #[test]
    fn catches_set_times_as_well_as_set_modified() {
        let src = "std::fs::File::open(&p).unwrap().set_times(times).unwrap();\n";
        assert_eq!(scan(&mask_non_code(src)).violations, vec![1]);
    }
}

mod population_mutations {
    use super::*;

    #[test]
    fn the_unmutated_site_map_has_no_drift() {
        let actual = scan_site_population();
        assert!(site_drift(&actual).is_empty(), "무변이인데 차분이 있다");
        let total: usize = actual.values().sum();
        assert!(
            total >= MIN_MTIME_WRITE_SITES,
            "mtime 변경을 {total}곳만 찾았다(하한 {MIN_MTIME_WRITE_SITES}). 수집 결과를 확인한다."
        );
    }

    /// 일부 파일의 누락이 전체 하한을 통과하더라도 파일별 명부 대조에서는 실패해야 한다.
    #[test]
    fn a_lost_file_is_named_not_just_counted() {
        let actual = scan_site_population();
        let victim = actual.keys().next().expect("대조군이 비었다").clone();
        let mut lost = actual.clone();
        lost.remove(&victim).expect("방금 고른 키다");

        let drift = site_drift(&lost);
        assert_eq!(drift.len(), 1, "잃은 파일 하나만 말해야 한다: {drift:?}");
        assert!(
            drift[0].contains(&victim),
            "이름으로 말하지 않는다: {drift:?}"
        );

        let left: usize = lost.values().sum();
        assert!(
            left >= MIN_MTIME_WRITE_SITES,
            "파일 하나를 뺀 뒤 {left}곳만 남아 전체 하한도 실패한다. 하한이 놓치는 일부 누락을 검증할 다른 입력이 필요하다."
        );
    }

    #[test]
    fn a_new_site_in_another_file_is_caught() {
        let mut grown = scan_site_population();
        grown.insert("crates/tasty-plugin-codex/src/handlers.rs".into(), 2);

        let drift = site_drift(&grown);
        assert_eq!(drift.len(), 1, "{drift:?}");
        assert!(drift[0].contains("새로 생김"), "{drift:?}");
        assert!(drift[0].contains("tasty-plugin-codex"), "{drift:?}");
    }

    #[test]
    fn a_single_lost_site_inside_a_file_is_caught() {
        let actual = scan_site_population();
        let victim = actual.keys().next().expect("대조군이 비었다").clone();
        let mut thinner = actual.clone();
        *thinner.get_mut(&victim).expect("방금 고른 키다") -= 1;

        let drift = site_drift(&thinner);
        assert_eq!(drift.len(), 1, "{drift:?}");
        assert!(drift[0].contains("개수 다름"), "{drift:?}");
    }

    /// 같은 파일의 사용처 교체는 개수가 유지되면 구별하지 못한다. 지원 범위를 넓히면 이 시험도 함께 갱신한다.
    #[test]
    fn a_same_file_swap_is_not_distinguished() {
        let together = "\
fn only_here() {
    let f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
    f.set_modified(t).unwrap();
    let g = std::fs::OpenOptions::new().write(true).open(p).unwrap();
    g.set_modified(t).unwrap();
}
";
        let apart = "\
fn first() {
    let f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
    f.set_modified(t).unwrap();
}

fn second() {
    let g = std::fs::OpenOptions::new().write(true).open(p).unwrap();
    g.set_modified(t).unwrap();
}
";
        let a = scan(&mask_non_code(together));
        let b = scan(&mask_non_code(apart));
        assert_ne!(together, apart, "비교할 두 합성 소스가 같다");
        assert!(
            a.violations.is_empty() && b.violations.is_empty(),
            "합성 소스에 위반이 있으면 아래 비교가 다른 이유로 갈린다"
        );
        assert_eq!(a.sites, 2, "합성 소스에서 사이트를 못 셌다");

        assert_eq!(
            a.sites, b.sites,
            "파일별 개수가 같은 사용처 교체를 구별했다. 지원 범위가 바뀌었다면 설명과 시험을 함께 갱신한다."
        );
        let same_map: BTreeMap<String, usize> =
            [("x.rs".to_string(), a.sites)].into_iter().collect();
        let also_same: BTreeMap<String, usize> =
            [("x.rs".to_string(), b.sites)].into_iter().collect();
        assert_eq!(same_map, also_same);
    }

    #[test]
    fn the_guard_file_does_not_count_itself() {
        let actual = scan_site_population();
        let me = "src/source_guards/read_only_handle_mtime.rs";
        assert!(
            !actual.contains_key(me),
            "가드 자신이 모수에 들어왔다: {actual:?}"
        );
        assert!(
            !actual.is_empty(),
            "mtime 변경 목록이 비어 검사 파일의 제외 여부를 확인할 수 없다"
        );
    }
}
