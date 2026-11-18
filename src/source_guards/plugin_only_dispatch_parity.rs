//! plugin_only 표식과 플러그인 host-call 처리에서 비교하는 메서드 이름을 대조한다.
//! 외부 호출을 허용하지 않는 메서드를 명시해 존재하지 않는 이름과 구별하기 위한 검사다(docs/dev-guide/api-conventions.md).
//!
//! GUI·헤드리스 파일에서 call.method == 형태의 이름을 합쳐 비교한다. 두 조합에 각각 존재하는지나
//! 실제 응답은 검증하지 않는다. 표에 없는 비교 이름도 이 대조에서는 제외한다.
//! 프로토콜 상수로 비교한 이름은 정의에서 값을 읽고, 상수 해석을 별도 시험한다.

use std::collections::BTreeSet;
use std::path::PathBuf;

use tasty_ipc::method_meta::METHOD_TABLE;

use super::{mask_non_code, repo_root};

const DISPATCH_SOURCES: &[&str] = &[
    "src/app/dispatch/plugin_ipc.rs",
    "src/boot/headless_plugins.rs",
];

/// 처리 분기가 비교하는 메서드 이름 상수를 정의한 파일들이다.
const PROTOCOL_CONSTS: &[&str] = &[
    "crates/tasty-plugin-protocol/src/protocol.rs",
    "crates/tasty-plugin-protocol/src/popup_spec.rs",
];

fn protocol_consts() -> Vec<(String, String)> {
    PROTOCOL_CONSTS
        .iter()
        .flat_map(|f| string_consts(&read(f)))
        .collect()
}

/// 2026-09-05 호스트 메서드 276개를 측정한 뒤 빈 표를 찾도록 둔 하한이다.
const MIN_HOST_METHODS: usize = 200;

/// 2026-09-05 플러그인 전용 메서드 4개를 실행 확인한 뒤 둔 하한이다.
const MIN_PLUGIN_ONLY: usize = 4;

fn read(rel: &str) -> String {
    let p: PathBuf = repo_root().join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n")
}

fn string_consts(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(':') else {
            continue;
        };
        let Some((_, value)) = tail.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_end_matches(';').trim();
        let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
            continue;
        };
        out.push((name.trim().to_string(), inner.to_string()));
    }
    out
}

/// 코드에서 call.method ==를 찾고 원문의 리터럴·상수 값을 읽는다. 한 줄 비교만 지원한다.
fn intercepted(src: &str, consts: &[(String, String)]) -> BTreeSet<String> {
    let masked = mask_non_code(src);
    let mut out = BTreeSet::new();
    for (raw, code) in src.lines().zip(masked.lines()) {
        let Some(at) = code.find("call.method ==") else {
            continue;
        };
        let after_raw = &raw[at.min(raw.len())..];
        let Some(rhs) = after_raw.split_once("==").map(|(_, r)| r.trim()) else {
            continue;
        };
        if let Some(lit) = rhs.strip_prefix('"').and_then(|v| v.split('"').next()) {
            out.insert(lit.to_string());
            continue;
        }
        let ident: String = rhs
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
            .next()
            .unwrap_or("")
            .rsplit("::")
            .next()
            .unwrap_or("")
            .to_string();
        if let Some((_, v)) = consts.iter().find(|(n, _)| *n == ident) {
            out.insert(v.clone());
        }
    }
    out
}

fn dispatched_names() -> BTreeSet<String> {
    let consts = protocol_consts();
    let mut out = BTreeSet::new();
    for f in DISPATCH_SOURCES {
        out.extend(intercepted(&read(f), &consts));
    }
    out
}

fn marked_plugin_only() -> BTreeSet<String> {
    METHOD_TABLE
        .iter()
        .filter(|(_, m)| m.plugin_only)
        .map(|(n, _)| (*n).to_string())
        .collect()
}

#[test]
fn the_plugin_only_mark_and_the_intercepts_are_the_same_set() {
    assert!(
        METHOD_TABLE.len() >= MIN_HOST_METHODS,
        "호스트 메서드가 {}개로 하한 {MIN_HOST_METHODS} 미만이다. 표 수집을 확인한다.",
        METHOD_TABLE.len()
    );
    let marked = marked_plugin_only();
    assert!(
        marked.len() >= MIN_PLUGIN_ONLY,
        "plugin_only 메서드가 {}개로 하한 {MIN_PLUGIN_ONLY} 미만이다(2026-09-05 측정 4개). 실제 제공 범위와 표식을 확인한다.",
        marked.len()
    );
    let dispatched = dispatched_names();

    let unmarked: Vec<&String> = dispatched
        .iter()
        .filter(|n| !marked.contains(*n))
        .filter(|n| METHOD_TABLE.iter().any(|(m, _)| *m == n.as_str()))
        .collect();
    assert!(
        unmarked.is_empty(),
        "플러그인 처리에 있고 메서드 표에도 있으나 plugin_only 표식이 없다. 외부 호출 허용 여부를 확인하고 표식을 맞춘다: {unmarked:?}"
    );

    let undispatched: Vec<&String> = marked.iter().filter(|n| !dispatched.contains(*n)).collect();
    assert!(
        undispatched.is_empty(),
        "plugin_only 메서드의 처리 이름을 두 플러그인 진입 파일에서 찾지 못했다. 실제 처리 경로와 추출 형식을 확인한다: {undispatched:?}"
    );
}

#[test]
fn the_constant_resolution_is_alive() {
    let consts = protocol_consts();
    assert!(
        consts.iter().any(|(_, v)| v == "host.shared_buffer.create")
            && consts.iter().any(|(_, v)| v == "popup.show"),
        "프로토콜 상수에서 `host.shared_buffer.create` 나 `popup.show` 를 못 찾았다 — 상수 정의 자리가 \
         옮겨졌으면 `PROTOCOL_CONSTS` 를 고쳐라"
    );
    let src = "if call.method == tasty_plugin_protocol::METHOD_HOST_SHARED_BUFFER_CREATE {";
    let got = intercepted(src, &consts);
    assert!(
        got.contains("host.shared_buffer.create"),
        "상수 경로를 값으로 못 풀었다: {got:?}"
    );
}

#[test]
fn the_extractor_reads_literals_and_skips_comments() {
    let src = "\
// if call.method == \"ns.commented\" { }
if call.method == \"ns.real\" {
";
    let got = intercepted(src, &[]);
    assert!(got.contains("ns.real"), "리터럴 비교를 놓쳤다: {got:?}");
    assert!(
        !got.contains("ns.commented"),
        "주석 안의 비교를 집었다 — 마스킹이 안 걸렸다: {got:?}"
    );
}
