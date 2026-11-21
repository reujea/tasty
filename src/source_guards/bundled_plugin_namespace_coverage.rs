//! 번들 플러그인의 namespace와 겹치는 호스트 메서드가 플러그인의 수신 dispatch에도 선언됐는지 확인한다.
//! namespace가 플러그인으로 전달되면 호스트 구현이 있어도 플러그인이 받지 않는 이름은 외부 호출에서 막힐 수 있다.
//! 발신 host.call의 문자열은 수신 처리의 증거가 아니므로 handle_ipc_method 본문의 match 패턴만 읽는다.
//! 이 검사는 패턴 존재를 확인하며 실제 분기의 실행 결과까지 검증하지는 않는다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use tasty_ipc::method_meta::METHOD_TABLE;

use super::{fn_body, repo_root};

const CRATES_DIR: &str = "crates";
const PLUGIN_CRATE_PREFIX: &str = "tasty-plugin-";
const MANIFEST_NAME: &str = "tasty-plugin.toml";

const DISPATCH_FN: &str = "fn handle_ipc_method";

/// 선언된 ipc_namespace 블록 수의 하한이다. 호스트 메서드와 겹치는 수가 아니다.
/// 2026-09-06 측정은 6개였다. description_i18n_key가 있는 3개·없는 3개 중
/// 한 형식을 놓치면 하한 4 미만이 되도록 정했다.
const MIN_DECLARED_NAMESPACES: usize = 4;

/// 2026-09-05 호스트 메서드 276개를 측정한 뒤 빈 파싱을 찾도록 둔 하한이다.
const MIN_HOST_METHODS: usize = 200;

/// 호스트와 플러그인 양쪽에 존재해 실제 비교한 prefix의 수다.
/// 2026-09-06에 image/markdown 둘이었다. 한쪽이 없어지는 정상 변경은 허용하되
/// 비교가 한 번도 실행되지 않으면 실패하도록 하한 1로 둔다.
const MIN_JOINED_PREFIXES: usize = 1;

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// `crates/tasty-plugin-*/` 중 매니페스트를 가진 디렉터리.
fn bundled_plugin_dirs() -> Vec<PathBuf> {
    let root = repo_root().join(CRATES_DIR);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root).expect("crates 디렉터리를 읽을 수 없다") {
        let entry = entry.expect("디렉터리 항목을 읽을 수 없다");
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(PLUGIN_CRATE_PREFIX) && path.join(MANIFEST_NAME).is_file() {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// 역직렬화와 유효성 검증은 별개다. 여기서는 매니페스트를 읽는다.
fn parse_manifest(dir: &std::path::Path) -> tasty_plugin_manifest::Manifest {
    let text = read(&dir.join(MANIFEST_NAME));
    toml::from_str(&text)
        .unwrap_or_else(|e| panic!("{}/{MANIFEST_NAME} 파싱 실패: {e}", dir.display()))
}

/// 매니페스트 파서로 namespace prefix를 읽어 주석·다른 표와 구분한다.
fn declared_prefixes(dir: &std::path::Path) -> Vec<String> {
    parse_manifest(dir)
        .contributes
        .ipc_namespace
        .iter()
        .map(|d| d.prefix.clone())
        .collect()
}

fn dispatch_bodies(dir: &std::path::Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("src")];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries {
            let entry = entry.expect("디렉터리 항목을 읽을 수 없다");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && let Some(body) = fn_body(&read(&path), DISPATCH_FN)
            {
                out.push((path, body));
            }
        }
    }
    out
}

/// 수신 함수의 match 패턴만 읽는다. 주석·로그·발신 호출에 쓰인 같은 이름은 제외한다.
fn handled_methods(body: &str, prefix: &str) -> BTreeSet<String> {
    use tasty_doc_guards::match_arms::{Source, matching_close};
    let src = Source::new(body);
    let code = src.code.as_str();
    let want = format!("{prefix}.");
    let mut out = BTreeSet::new();
    let mut from = 0usize;
    while let Some(at) = code[from..].find("match ") {
        let at = from + at;
        from = at + "match ".len();
        if code[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let Some(open) = code[from..].find('{').map(|k| from + k) else {
            break;
        };
        let Some(close) = matching_close(code, open) else {
            continue;
        };
        let arms = src
            .match_arms(open..close + 1)
            .unwrap_or_else(|e| panic!("inbound dispatch 의 팔을 못 읽었다 — {e}"));
        for arm in arms {
            for alt in src.alternatives(&arm.pattern) {
                if let Some(name) = src.plain_string(&alt)
                    && name.starts_with(&want)
                {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out
}

fn host_methods_by_prefix() -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, _) in METHOD_TABLE {
        if let Some((p, _)) = name.split_once('.') {
            out.entry(p.to_string()).or_default().insert((*name).into());
        }
    }
    out
}

#[test]
fn every_host_method_under_a_bundled_namespace_is_handled_by_that_plugin() {
    assert!(
        METHOD_TABLE.len() >= MIN_HOST_METHODS,
        "호스트 메서드를 {}개만 읽었다(하한 {MIN_HOST_METHODS}, 2026-09-05 측정 276개). 파싱 범위를 확인한다.",
        METHOD_TABLE.len()
    );
    let by_prefix = host_methods_by_prefix();

    // 실패 시 누락된 대상을 비교할 수 있도록 개수와 목록을 함께 남긴다.
    let mut declared: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    // 호스트와 실제로 겹쳐 비교한 prefix도 따로 기록한다.
    let mut joined: Vec<String> = Vec::new();
    for dir in bundled_plugin_dirs() {
        for prefix in declared_prefixes(&dir) {
            declared.push(format!(
                "{}::{prefix}",
                dir.file_name().unwrap_or_default().to_string_lossy()
            ));
            let Some(host) = by_prefix.get(&prefix) else {
                continue;
            };
            joined.push(format!("{prefix}(host {} 건)", host.len()));
            let bodies = dispatch_bodies(&dir);
            assert!(
                !bodies.is_empty(),
                "{}에서 {DISPATCH_FN} 본문을 찾지 못했다. SDK 메서드 이름과 소스 추출을 확인한다.",
                dir.display()
            );
            let handled: BTreeSet<String> = bodies
                .iter()
                .flat_map(|(_, b)| handled_methods(b, &prefix))
                .collect();
            for m in host.difference(&handled) {
                missing.push(format!(
                    "{m} — host 가 구현했는데 {} 의 inbound dispatch 에 arm 이 없다",
                    dir.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }

    assert!(
        joined.len() >= MIN_JOINED_PREFIXES,
        "실제로 비교한 prefix가 {}개뿐이다(하한 {MIN_JOINED_PREFIXES}). 비교 목록: {:?}, 선언 목록: {:?}. host_methods_by_prefix의 키와 선언을 대조한다. 호스트에 해당 이름이 있는데 연결하지 못했다면 파싱·문자열 형식을 고친다. 실제로 겹치는 메서드가 없어졌다면 검사의 필요성을 다시 검토한다. 비교 누락을 하한 0으로 숨기지 않는다.",
        joined.len(),
        joined,
        declared,
    );

    assert!(
        declared.len() >= MIN_DECLARED_NAMESPACES,
        "ipc_namespace 선언을 {}개만 읽었다(하한 {MIN_DECLARED_NAMESPACES}): {:?}. 실제 디렉터리·매니페스트의 선언과 수집 목록을 대조한다. 선언이 있는데 빠졌다면 파서를 고치고, 실제 선언이 줄었다면 누락을 찾을 수 있는 새 하한과 근거를 함께 정한다.",
        declared.len(),
        declared
    );
    assert!(
        missing.is_empty(),
        "플러그인 namespace와 겹치는 호스트 메서드가 플러그인의 수신 dispatch에 없다. self-call로 호스트에 전달하는 분기를 추가하거나 호스트의 해당 메서드 제공 여부를 검토한다:\n  {}",
        missing.join("\n  ")
    );
}

/// 함수 밖의 같은 prefix 문자열을 수신 메서드로 세지 않아야 한다.
#[test]
fn the_cut_is_the_dispatch_body_not_the_whole_file() {
    let src = "\
fn other_before() { emit(\"ns.before\"); }
fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> R {
    match ctx.method.as_str() {
        \"ns.inside\" => ok(),
        other => not_found(other),
    }
}
fn other_after() { host.call(\"ns.after\", p); }
";
    let body = fn_body(src, DISPATCH_FN).expect("본문을 잘라야 한다");
    let found = handled_methods(&body, "ns");
    assert!(found.contains("ns.inside"), "본문 안의 이름은 잡아야 한다");
    assert!(
        !found.contains("ns.before") && !found.contains("ns.after"),
        "본문 밖의 같은 prefix 리터럴을 집었다 — 자르기가 안 좁혀졌다: {found:?}"
    );
    assert!(
        !body.contains("other_after"),
        "본문이 함수 끝을 넘어 이어졌다"
    );
}

#[test]
fn braces_inside_string_literals_do_not_close_the_body() {
    let src = "\
fn handle_ipc_method(&mut self) -> R {
    emit(format!(\"{}\", x));
    emit(\"}\");
    match m { \"ns.inside\" => ok() }
}
fn after() { emit(\"ns.after\"); }
";
    let body = fn_body(src, DISPATCH_FN).expect("본문을 잘라야 한다");
    let found = handled_methods(&body, "ns");
    assert!(
        found.contains("ns.inside") && !found.contains("ns.after"),
        "문자열 안 중괄호에 속아 본문이 일찍 끊기거나 넘쳤다: {found:?}"
    );
}

/// 수신 함수 안에서도 match 패턴 외의 문자열은 처리 메서드가 아니다.
#[test]
fn only_arm_patterns_count_as_handled() {
    let src = "\
fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> R {
    // \"ns.commented\" 는 여기서 받지 않는다
    tracing::warn!(\"ns.logged 는 아직 없다\");
    let _ = host.call(\"ns.forwarded\", p);
    match ctx.method.as_str() {
        \"ns.inside\" | \"ns.also\" => ok(),
        \"ns.guarded\" if ready => ok(),
        other => not_found(other),
    }
}
";
    let body = fn_body(src, DISPATCH_FN).expect("본문을 잘라야 한다");
    let found = handled_methods(&body, "ns");
    let want: BTreeSet<String> = ["ns.inside", "ns.also", "ns.guarded"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(found, want, "팔이 아닌 자리의 이름을 셌거나 팔을 놓쳤다");
}

/// 2026-09-06 번들 매니페스트 9개를 측정한 뒤 빈 수집을 찾도록 둔 하한이다.
const MIN_BUNDLED_PLUGINS: usize = 6;

/// 번들 플러그인 매니페스트(workspace와 공식 플러그인 픽스처)에 실제 validate를 적용한다.
#[test]
fn every_bundled_manifest_passes_the_real_validation() {
    let dirs = tasty_test_support::bundled_plugin_manifest_dirs();
    assert!(
        dirs.len() >= MIN_BUNDLED_PLUGINS,
        "번들 매니페스트를 {}개만 읽었다(하한 {MIN_BUNDLED_PLUGINS}, 2026-09-06 측정 9개). 실제 플러그인 감소와 디렉터리 수집 실패를 구별한다. 하한 변경에는 측정 근거를 남긴다.",
        dirs.len()
    );
    for dir in &dirs {
        parse_manifest(dir).validate().unwrap_or_else(|e| {
            panic!(
                "{}/{MANIFEST_NAME}이 유효성 검증에 실패했다: {e}",
                dir.display()
            )
        });
    }
}

/// 검증된 실제 매니페스트의 prefix만 예약어로 바꿔 그 이유로 거절되는지 확인한다.
#[test]
fn the_validation_rejects_a_reserved_prefix() {
    let (dir, mut manifest) = tasty_test_support::bundled_plugin_manifest_dirs()
        .into_iter()
        .map(|d| {
            let m = parse_manifest(&d);
            (d, m)
        })
        .find(|(_, m)| !m.contributes.ipc_namespace.is_empty())
        .expect("예약 prefix 검사에 쓸 namespace 선언을 찾지 못했다");
    // 공식 플러그인의 계약은 잠금 파일이 가리키는 릴리스 산출물이다. 같은 id의 사본이 아니라 픽스처를 흔든다.
    assert_eq!(
        dir.parent(),
        Some(tasty_test_support::official_plugin_fixtures_root().as_path()),
        "namespace 선언을 공식 플러그인 픽스처가 아닌 곳에서 골랐다: {}",
        dir.display()
    );

    manifest
        .validate()
        .expect("prefix를 바꾸기 전 매니페스트부터 유효성 검사에 실패했다");

    let reserved = tasty_plugin_manifest::validators::RESERVED_IPC_PREFIXES
        .first()
        .expect("예약 목록이 비었다 — 흔들 값이 없다");
    manifest.contributes.ipc_namespace[0].prefix = (*reserved).to_string();
    let err = manifest
        .validate()
        .expect_err("예약된 prefix 를 선언했는데 검증이 통과했다");
    assert!(
        format!("{err}").contains(reserved),
        "거절은 했는데 이유가 그 prefix 가 아니다 — 다른 규칙에 먼저 걸렸다: {err}"
    );
}
