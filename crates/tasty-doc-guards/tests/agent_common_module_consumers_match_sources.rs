//! 아키텍처 문서가 적은 `tasty-plugin-agent-common` 모듈별 소비자가 실제 소스와 맞는지 검사한다.
//! 공용 크레이트로 옮긴 사실과 "두 plugin 이 같은 기능을 함께 쓴다" 는 설명은 다르다 —
//! 옮기기만 하고 한쪽만 쓰는 모듈을 공용 소비처럼 적는 회귀를 막는다.
//!
//! 소비는 `tasty_plugin_agent_common::<module>` 문자열 등장으로 센다. 주석·문자열 안의
//! 언급도 함께 세므로 "쓰지 않는데 쓴다고 적힌" 쪽만 확실히 잡고, 반대 방향은 느슨하다.

use std::path::{Path, PathBuf};

const DOC: &str = "docs/architecture/index.md";

/// 문서가 단독 소비자라고 적은 모듈과 그 소비 크레이트.
/// 다른 agent plugin 이 같은 모듈을 쓰기 시작하면 문서를 함께 고치라는 뜻으로 실패한다.
const SOLE_CONSUMERS: &[(&str, &str)] = &[
    ("transcript", "tasty-plugin-agent-stream"),
    ("tail", "tasty-plugin-agent-stream"),
];

/// 같은 계층의 agent plugin 들 — 이 중 소비자로 적히지 않은 쪽이 쓰면 문서가 낡은 것이다.
const AGENT_PLUGINS: &[&str] = &[
    "tasty-plugin-claude",
    "tasty-plugin-codex",
    "tasty-plugin-agent-stream",
];

fn root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

/// 크레이트 src/ 아래 .rs 전부를 이어 붙인다. 비면 panic 한다 — 빈 본문에 대고
/// "소비 없음" 으로 읽으면 검사가 통째로 vacuous 해진다.
fn crate_sources(crate_name: &str) -> String {
    let dir = root().join("crates").join(crate_name).join("src");
    let mut out = String::new();
    collect(&dir, &mut out);
    assert!(
        !out.is_empty(),
        "{} 아래에서 읽은 소스가 비었다 — 경로가 바뀌었거나 순회가 죽었다. \
         이 값을 그대로 쓰면 모든 검사가 '소비 없음' 으로 통과한다",
        dir.display()
    );
    out
}

fn collect(dir: &Path, out: &mut String) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
            );
            out.push('\n');
        }
    }
}

fn uses(sources: &str, module: &str) -> bool {
    sources.contains(&format!("tasty_plugin_agent_common::{module}"))
}

#[test]
fn documented_sole_consumer_actually_consumes_the_module() {
    for (module, consumer) in SOLE_CONSUMERS {
        let sources = crate_sources(consumer);
        assert!(
            uses(&sources, module),
            "{DOC} 는 `{module}` 의 소비자로 {consumer} 를 적지만 그 크레이트에 \
             `tasty_plugin_agent_common::{module}` 소비가 없다"
        );
    }
}

#[test]
fn no_other_agent_plugin_consumes_a_sole_consumer_module() {
    for (module, consumer) in SOLE_CONSUMERS {
        for plugin in AGENT_PLUGINS {
            if plugin == consumer {
                continue;
            }
            let sources = crate_sources(plugin);
            assert!(
                !uses(&sources, module),
                "{plugin} 가 `{module}` 을 쓰는데 {DOC} 는 {consumer} 만 소비자로 적는다 — \
                 모듈별 소비자 서술을 함께 고쳐라"
            );
        }
    }
}

/// 문서 문장 자체가 모듈별 소비자를 적고 있는지 — 다시 "공용으로 함께 쓴다" 로
/// 뭉뚱그려 돌아가면 실패한다.
#[test]
fn doc_states_per_module_consumers() {
    let path = root().join(DOC);
    let doc =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    for (module, _) in SOLE_CONSUMERS {
        let needle = format!("`{module}`");
        assert!(
            doc.contains(&needle),
            "{DOC} 의 agent-common 설명이 모듈 {needle} 을 모듈별 소비자와 함께 적어야 한다"
        );
    }
}
