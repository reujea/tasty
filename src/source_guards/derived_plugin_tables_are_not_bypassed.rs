//! 플러그인 상태의 직접 변경이 등록된 갱신 파일 밖에 있는지 검사한다.
//! packages를 바꾼 뒤 namespace·extensions를 갱신하지 않으면 제거한 플러그인이 호출을 가릴 수 있다.
//! 관련 설계는 [플러그인 제작 가이드](../../docs/dev-guide/plugin-development.md)에 있다.
//!
//! 필드는 비공개라는 전제 아래 소유 크레이트 안에서만 검색한다. namespace 쓰기는
//! namespaces_write 이름으로 찾으며, 공유 표 타입의 외부 사용과 표를 돌려주는 함수도 별도로 검사한다.
//! 공유 Arc나 읽을 때마다 계산하는 값처럼 별도 갱신 없이 따라오는 값은 낡은 사본으로 분류하지 않는다.
//!
//! 파일 이름이 아니라 shipping_scope의 선언·타깃 분류로 테스트 전용 파일을 제외한다.
//! 변경 검색은 주석만 지운 줄의 이름·호출 형태를 비교한다. 수신자 타입과 실행 순서는 분석하지 않는다.
//! 갱신 순서의 정합은 lifecycle 끝의 debug_assert_extensions_fresh와 그 실행 시험에서 확인한다.

use std::path::PathBuf;

use tasty_doc_guards::shipping_scope;

use super::{repo_root, strip_comments};

struct Derived {
    what: &'static str,
    /// 필드 이름은 점을 포함한다. 함수는 이름 전체를 field에 두고 verbs에 호출 구분자를 둔다.
    field: &'static str,
    verbs: &'static [&'static str],
    /// 직접 변경을 허용하는 파일.
    home: &'static str,
}

const HOST_PLUGIN_LIFECYCLE: &str = "crates/tasty-host-plugin/src/manager/lifecycle.rs";

const DERIVED: &[Derived] = &[
    Derived {
        // 외부에는 packages()만 공개되지만 같은 크레이트 안의 직접 변경은 검사해야 한다.
        what: "설치 목록(디스크에서 재발견된다)",
        field: ".packages",
        verbs: &[
            ".retain(", ".push(", ".clear(", ".remove(", ".insert(", " =",
        ],
        home: HOST_PLUGIN_LIFECYCLE,
    },
    Derived {
        // 공유 표를 바꾸는 접근 함수의 호출을 찾는다.
        what: "namespace 소유 표(packages 에서 유도)",
        field: "namespaces_write",
        verbs: &["("],
        home: HOST_PLUGIN_LIFECYCLE,
    },
    Derived {
        what: "확장 집합(packages + config 에서 유도)",
        field: ".extensions",
        verbs: &[".recompute(", " ="],
        home: "crates/tasty-host-plugin/src/manager/queries.rs",
    },
];

/// 테스트 전용 파일을 제외한 수집 수의 하한. 2026-09-06 측정 1083개.
const MIN_SHIPPING_SCANNED: usize = 900;

/// 테스트 제외 판정이 비지 않았는지 확인한다. 2026-09-06 제외 파일 126개.
const MIN_TEST_ONLY: usize = 60;

/// 소유 크레이트에서 갱신 파일을 제외하고 검사한 수의 하한이다. 2026-09-06 측정 30개.
/// manager 디렉터리로 잘못 좁히면 9개, 갱신 파일만 보면 0개였다.
/// 하한 20은 두 축소를 찾으면서 정상적인 파일 감소에는 10개의 여유를 둔다.
const MIN_PEERS_IN_SCOPE: usize = 20;

const FIELD_DECL_FILE: &str = "crates/tasty-host-plugin/src/manager.rs";

const TABLE_TYPE: &str = "IpcNamespaceRegistry";

const TABLE_TYPE_HOMES: &[&str] = &["crates/tasty-ipc/", "crates/tasty-host-plugin/"];

const TABLE_CUSTODY_FILE: &str = "crates/tasty-ipc/src/method_meta.rs";

const NAME_BLIND_TEST_ONLY_FILE: &str =
    "crates/tasty-doc-guards/tests/filtered_guards_are_not_totally_blind.rs";

/// 소유 크레이트 밖을 검사하지 않는 전제인 필드 비공개 여부를 확인한다.
#[test]
fn the_narrowed_scan_rests_on_the_fields_being_private() {
    let src = std::fs::read_to_string(repo_root().join(FIELD_DECL_FILE))
        .expect("필드 선언 파일을 읽지 못했다 — 옮겼으면 이 상수도 함께 고쳐라");
    let masked = super::mask_non_code(&src);
    // namespaces_write는 필드가 아니라 접근 함수이므로 이 선언 검사에서 제외한다.
    for field in ["packages", "ipc_namespaces", "extensions"] {
        let decls: Vec<&str> = masked
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with(&format!("{field}:"))
                    || (t.starts_with("pub") && t.contains(&format!(" {field}:")))
            })
            .collect();
        assert_eq!(
            decls.len(),
            1,
            "`{field}` 선언을 {}개 찾았다. 정확히 1개여야 하므로 이동 여부와 파서를 확인한다.",
            decls.len()
        );
        assert!(
            !decls[0].trim_start().starts_with("pub"),
            "`{field}`가 공개돼 크레이트 밖에서도 직접 변경할 수 있다. 비공개로 되돌리거나 검사 범위를 함께 넓힌다: {}",
            decls[0].trim()
        );
    }
}

fn mutates(d: &Derived, line: &str) -> bool {
    let Some(at) = line.find(d.field) else {
        return false;
    };
    // 함수 정의를 호출로 세지 않는다.
    if line[..at].contains("fn ") {
        return false;
    }
    let after = &line[at + d.field.len()..];
    if after.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return false;
    }
    d.verbs.iter().any(|v| after.starts_with(v))
}

/// 갱신 파일은 검사에 포함되고, 이름과 무관하게 테스트 전용 파일은 제외되는지 대조한다.
#[test]
fn the_exemption_is_pinned_on_both_sides_by_real_files() {
    let root = repo_root();
    let sources = super::rust_sources();
    let test_only = shipping_scope::test_only_files(&root, &sources);

    for d in DERIVED {
        assert!(
            !test_only.contains(&PathBuf::from(d.home)),
            "갱신 파일 {}가 테스트 전용으로 제외돼 실제 변경을 비교할 수 없다",
            d.home
        );
    }

    let by_name_not_exempt = PathBuf::from(NAME_BLIND_TEST_ONLY_FILE);
    assert!(
        sources.iter().any(|(p, _)| *p == by_name_not_exempt),
        "{NAME_BLIND_TEST_ONLY_FILE}이 스캔에 없다. 파일 이동 여부를 확인하고 선언 기반 제외를 검증할 대상을 갱신한다."
    );
    let name = NAME_BLIND_TEST_ONLY_FILE
        .rsplit('/')
        .next()
        .unwrap_or(NAME_BLIND_TEST_ONLY_FILE);
    assert!(
        !name.starts_with("tests") && !name.contains("_tests.") && name != "tests.rs",
        "{NAME_BLIND_TEST_ONLY_FILE}의 이름에도 tests가 있어 선언 기준과 이름 기준을 비교할 수 없다. 이름으로는 제외되지 않는 테스트 파일을 고른다."
    );
    assert!(
        test_only.contains(&by_name_not_exempt),
        "{NAME_BLIND_TEST_ONLY_FILE} 이 면제 밖이다 — 선언 판정이 이 형태를 놓쳤다"
    );
}

/// 공유 표를 다른 크레이트가 직접 받으면 namespaces_write를 거치지 않고 바꿀 수 있다.
/// 타입 이름을 찾는 검색이 작동하는지 테스트 전용 사용처와 함께 대조한다.
/// 타입 별칭이나 추론된 값의 전달까지 추적하는 검사는 아니다.
#[test]
fn the_shared_table_type_is_named_only_where_it_is_owned() {
    let root = repo_root();
    let sources = super::rust_sources();
    let test_only = shipping_scope::test_only_files(&root, &sources);

    let mut shipping_outside: Vec<String> = Vec::new();
    let mut test_only_outside: Vec<String> = Vec::new();
    for (path, src) in &sources {
        let rel = path.to_string_lossy().replace('\\', "/");
        if TABLE_TYPE_HOMES.iter().any(|h| rel.starts_with(h)) {
            continue;
        }
        // 이 검사의 문자열 상수가 실제 타입 사용처로 세어지지 않도록 리터럴도 가린다.
        if !super::mask_non_code(src).contains(TABLE_TYPE) {
            continue;
        }
        if test_only.contains(path) {
            test_only_outside.push(rel);
        } else {
            shipping_outside.push(rel);
        }
    }

    assert!(
        !test_only_outside.is_empty(),
        "두 크레이트 밖에서 `{TABLE_TYPE}` 사용을 찾지 못했다. 타입 이름·마스킹·스캔을 확인하고, 비교하던 테스트가 사라졌다면 다른 테스트 사용처로 대체한다. 빈 결과만으로 제품 코드에 사용이 없다고 판단하지 않는다."
    );
    assert!(
        shipping_outside.is_empty(),
        "소유 크레이트 밖의 제품 코드에서 `{TABLE_TYPE}`을 쓴다. 공유 표를 직접 바꾸면 namespaces_write 검색으로 찾지 못한다. 접근 경로를 제한하거나 검사 방식을 갱신한다:\n  {}",
        shipping_outside.join("\n  ")
    );
}

/// 공유 표를 반환하는 함수를 test 전용으로 제한한다.
/// fn·->·타입 이름이 한 줄에 있는 형태만 읽으며 별칭과 여러 줄 선언은 추적하지 않는다.
#[test]
fn the_custody_crate_does_not_hand_the_handle_back_out_in_release() {
    let src = std::fs::read_to_string(repo_root().join(TABLE_CUSTODY_FILE))
        .expect("보관 파일을 읽지 못했다 — 옮겼으면 이 상수도 함께 고쳐라");
    let masked = super::mask_non_code(&src).replace("\r\n", "\n");
    let lines: Vec<&str> = masked.lines().collect();
    let gated = tasty_doc_guards::cfg_predicate::cfg_gated_lines(&lines, "test");

    let mut returning: Vec<(usize, bool)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if (t.starts_with("fn ") || t.contains(" fn "))
            && let Some(at) = line.find("->")
            && line[at..].contains(TABLE_TYPE)
        {
            returning.push((i, gated[i]));
        }
    }

    assert!(
        !returning.is_empty(),
        "{TABLE_CUSTODY_FILE}에서 `{TABLE_TYPE}` 반환 함수를 찾지 못했다. fn·->·타입 이름의 매칭과 실제 선언을 확인한다. 함수가 모두 사라졌다면 다른 반환 함수로 파서를 검증하면서 이 타입의 반환이 0개인지 검사하도록 바꾼다."
    );
    let ungated: Vec<String> = returning
        .iter()
        .filter(|(_, g)| !g)
        .map(|(i, _)| format!("{TABLE_CUSTODY_FILE}:{} — {}", i + 1, lines[*i].trim()))
        .collect();
    assert!(
        ungated.is_empty(),
        "test 전용이 아닌 함수가 공유 표를 반환한다. 외부에서 직접 변경하지 못하도록 반환을 없애거나 테스트 전용으로 제한한다:\n  {}",
        ungated.join("\n  ")
    );
}

#[test]
fn derived_plugin_state_is_only_mutated_where_it_is_derived() {
    let root = repo_root();
    let sources = super::rust_sources();
    let test_only = shipping_scope::test_only_files(&root, &sources);
    assert!(
        test_only.len() >= MIN_TEST_ONLY,
        "테스트 전용 파일을 {}개만 찾았다(하한 {MIN_TEST_ONLY}, 전체 수집 {}). 실제 파일 수와 제외 판정을 확인한다. 이 하한은 제외된 파일 하나하나의 정확성까지 보장하지 않는다.",
        test_only.len(),
        sources.len()
    );

    let mut offenders: Vec<String> = Vec::new();
    let mut homes_seen = vec![false; DERIVED.len()];
    // 갱신 파일만 검사하도록 범위가 줄어도 homes_seen은 통과하므로 다른 파일 수도 센다.
    let mut peers_in_scope = vec![0usize; DERIVED.len()];
    let mut scanned = 0usize;
    for (path, src) in &sources {
        if test_only.contains(path) {
            continue;
        }
        scanned += 1;
        let rel = path.to_string_lossy().replace('\\', "/");
        let stripped = strip_comments(src);
        for (n, d) in DERIVED.iter().enumerate() {
            // 필드는 비공개이므로 소유 크레이트만 검사한다. 함수 이름으로 찾는 항목은 전체 범위를 유지한다.
            if d.field.starts_with('.') {
                let owner = d.home.rsplit_once("/src/").map(|(c, _)| c);
                if let Some(owner) = owner
                    && !rel.starts_with(owner)
                {
                    continue;
                }
            }
            if rel != d.home {
                peers_in_scope[n] += 1;
            }
            let hit = stripped.lines().enumerate().filter(|(_, l)| mutates(d, l));
            if rel == d.home {
                if hit.count() > 0 {
                    homes_seen[n] = true;
                }
                continue;
            }
            for (i, line) in hit {
                offenders.push(format!("{rel}:{} [{}] — {}", i + 1, d.what, line.trim()));
            }
        }
    }

    assert!(
        scanned >= MIN_SHIPPING_SCANNED,
        "테스트 제외 후 검사한 파일이 {scanned}개뿐이다(하한 {MIN_SHIPPING_SCANNED}, 전체 수집 {}). 실제 파일 감소와 과도한 제외를 구별해 원인을 수정한다.",
        sources.len()
    );

    for (n, peers) in peers_in_scope.iter().enumerate() {
        if !DERIVED[n].field.starts_with('.') {
            continue; // 자유 함수 부류는 애초에 안 좁힌다 — 좁힘이 없으면 잴 것도 없다.
        }
        assert!(
            *peers >= MIN_PEERS_IN_SCOPE,
            "{}의 검사 범위에서 갱신 파일 외에 {peers}개만 찾았다(하한 {MIN_PEERS_IN_SCOPE}). 소유 크레이트 {}의 실제 test 전용이 아닌 파일 수와 비교한다. 검사 범위가 잘못 줄었다면 home과 소유 prefix 계산을 고친다. 실제 파일이 줄었을 때만 근거를 남겨 하한을 조정하며 0으로 낮추지 않는다.",
            DERIVED[n].what,
            DERIVED[n]
                .home
                .rsplit_once("/src/")
                .map(|(c, _)| c)
                .unwrap_or(DERIVED[n].home),
        );
    }

    for (n, seen) in homes_seen.iter().enumerate() {
        assert!(
            *seen,
            "{}의 갱신 파일 {}에서 변경 형태를 찾지 못했다. 파일 이동·이름 변경을 확인하고 DERIVED를 갱신한다.",
            DERIVED[n].what, DERIVED[n].home
        );
    }

    assert!(
        offenders.is_empty(),
        "등록된 갱신 파일 밖에서 플러그인 상태를 직접 바꾼다. namespace나 extensions가 낡지 않도록 해당 갱신 함수를 사용한다:\n  {}",
        offenders.join("\n  ")
    );
}

/// 주석만 지우는 검색에 이 검사의 입력 문자열이 걸리지 않도록 대상 이름을 조립한다.
#[test]
fn the_mutation_shapes_are_recognised_and_reads_are_not() {
    let pkgs = &DERIVED[0];
    let f = pkgs.field;
    assert!(mutates(
        pkgs,
        &format!("        mgr{f}.retain(|p| p.id != x);")
    ));
    assert!(mutates(pkgs, &format!("    self{f} = packages;")));
    assert!(
        !mutates(pkgs, &format!("    for pkg in &mgr{f} {{")),
        "읽기를 쓰기로 셌다"
    );
    assert!(
        !mutates(pkgs, &format!("    let n = mgr{f}.len();")),
        "읽기를 쓰기로 셌다"
    );
    assert!(
        !mutates(pkgs, &format!("    mgr{f}_of(id).clear();")),
        "더 긴 이름을 이 필드로 셌다"
    );

    for d in DERIVED.iter().filter(|d| d.field.starts_with('.')) {
        let f = d.field;
        let verb = d.verbs[0];
        assert!(
            mutates(d, &format!("        mgr{f}{verb});")),
            "{}의 첫 변경 형태를 검출하지 못했다",
            d.what
        );
        assert!(
            !mutates(d, &format!("        let x = mgr{f}.len();")),
            "{} 에서 읽기를 쓰기로 셌다",
            d.what
        );
    }
}

#[test]
fn a_mutation_inside_a_comment_is_not_counted() {
    let d = &DERIVED[0];
    let f = d.field;
    let src = format!("fn f() {{\n    // mgr{f}.retain(|p| true);\n    ok();\n}}\n");
    let stripped = strip_comments(&src);
    assert!(
        !stripped.lines().any(|l| mutates(d, l)),
        "주석의 변경 예시를 실제 코드로 판단했다"
    );
}

/// 필드와 함수 항목은 검사 범위가 다르므로 각각 비지 않았는지 확인한다. 2026-09-06 측정은 필드 2개·함수 1개였다.
#[test]
fn the_roster_still_carries_both_kinds() {
    let fields = DERIVED.iter().filter(|d| d.field.starts_with('.')).count();
    let free = DERIVED.len() - fields;
    assert!(
        fields >= 1 && free >= 1,
        "명부에 필드 {fields}개·함수 {free}개가 있다(전체 {}). 두 종류는 검사 범위가 달라 각각 필요하다. 대상이 실제로 없어졌다면 관련 검사도 함께 정리하고, 명부에서만 빠졌다면 복원한다.",
        DERIVED.len()
    );
}

#[test]
fn every_derivation_home_is_a_real_file() {
    for d in DERIVED {
        assert!(
            repo_root().join(d.home).is_file(),
            "{} 의 유도 자리 {} 가 파일이 아니다",
            d.what,
            d.home
        );
    }
}

#[test]
fn this_file_carries_no_whole_mutation_literal() {
    let me = repo_root().join("src/source_guards/derived_plugin_tables_are_not_bypassed.rs");
    let src = std::fs::read_to_string(&me).expect("이 파일을 읽어야 한다");
    let stripped = strip_comments(&src);
    for d in DERIVED {
        let n = stripped.lines().filter(|l| mutates(d, l)).count();
        assert_eq!(
            n, 0,
            "이 파일이 자기 판정({})에 {n} 줄 걸린다 — 대조군을 리터럴로 적었다면 \
             조립으로 바꿔라",
            d.what
        );
    }
}
