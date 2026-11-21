//! host·plugin·user가 선언하는 완료 판정 전략. 모두 poll과 push를 사용할 수 있다.
//! owner가 필요한 namespace·notify_via 검증은 레지스트리 병합 뒤에 한다.

use std::fmt;

use serde::Deserialize;

use super::types::{
    CompletionStrategyId, CompletionStrategyKind, CompletionStrategyOwner,
    is_valid_completion_strategy_short_name,
};
use crate::hook_handler::HookHandlerId;
use crate::runtime::agent::completion_strategy::completion_strategy_to_poll_spec;
/// CLI 자동 대기와 같은 poll 선언을 사용한다. 최상위 전략 선언과 이름이 겹쳐 별칭을 둔다.
use tasty_plugin_manifest::CompletionStrategyDecl as PollStrategyDecl;

#[derive(Debug, Clone, Deserialize)]
pub struct CompletionStrategyDecl {
    /// 전역 ID는 owner prefix와 이 짧은 이름을 /로 연결한다.
    pub id: String,
    pub priority: i32,
    #[serde(default)]
    pub display_name_i18n_key: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    /// 기본 전략으로 연결할 IPC 메서드. 비어 있으면 이름을 지정해야 한다.
    #[serde(default)]
    pub default_for_methods: Vec<String>,
    pub spec: CompletionStrategySpecDecl,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompletionStrategySpecDecl {
    /// CLI 자동 대기와 필드·기본값을 공유한다.
    Poll(PollStrategyDecl),
    /// 외부 완료 보고. notify_via는 보고를 받을 훅 핸들러 ID다.
    Push { notify_via: String, timeout_ms: u64 },
}

/// poll 선언 변환은 completion_strategy_to_poll_spec을 공유한다.
impl From<CompletionStrategySpecDecl> for CompletionStrategyKind {
    fn from(d: CompletionStrategySpecDecl) -> Self {
        match d {
            CompletionStrategySpecDecl::Poll(decl) => {
                CompletionStrategyKind::Poll(completion_strategy_to_poll_spec(&decl))
            }
            CompletionStrategySpecDecl::Push {
                notify_via,
                timeout_ms,
            } => CompletionStrategyKind::Push {
                notify_via: HookHandlerId::new(notify_via),
                timeout_ms,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionStrategyDeclError {
    InvalidShortName(String),
}

impl fmt::Display for CompletionStrategyDeclError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidShortName(s) => write!(
                f,
                "invalid completion strategy short-name '{s}' (must match [a-z0-9-]{{1,32}})"
            ),
        }
    }
}

impl std::error::Error for CompletionStrategyDeclError {}

pub fn validate_completion_strategy_decl(
    decl: &CompletionStrategyDecl,
) -> Result<(), CompletionStrategyDeclError> {
    if !is_valid_completion_strategy_short_name(&decl.id) {
        return Err(CompletionStrategyDeclError::InvalidShortName(
            decl.id.clone(),
        ));
    }
    Ok(())
}

pub fn global_id(owner: &CompletionStrategyOwner, short: &str) -> CompletionStrategyId {
    CompletionStrategyId(format!("{}/{}", owner.prefix(), short))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Debug)]
    struct Wrap {
        #[serde(rename = "strategy")]
        strategies: Vec<CompletionStrategyDecl>,
    }

    fn parse(s: &str) -> Result<Wrap, toml::de::Error> {
        toml::from_str(s)
    }

    #[test]
    fn poll_spec_parses() {
        let t = r#"
            [[strategy]]
            id = "spawn-wait"
            priority = 100
            default_for_methods = ["claude.spawn"]
            [strategy.spec]
            kind = "poll"
            poll_method = "claude.wait"
            state_field = "state"
            terminal_states = ["idle", "needs_input"]
        "#;
        let w = parse(t).expect("parse");
        assert_eq!(w.strategies.len(), 1);
        match &w.strategies[0].spec {
            CompletionStrategySpecDecl::Poll(decl) => {
                assert_eq!(decl.poll_method, "claude.wait");
                assert_eq!(decl.interval_ms, 500); // 기본값 (PollStrategyDecl 소유)
            }
            CompletionStrategySpecDecl::Push { .. } => panic!("expected poll"),
        }
    }

    #[test]
    fn push_spec_requires_timeout() {
        let t = r#"
            [[strategy]]
            id = "notify-done"
            priority = 100
            [strategy.spec]
            kind = "push"
            notify_via = "host/webhook-notify"
        "#;
        // push는 timeout_ms를 생략할 수 없다.
        assert!(parse(t).is_err());
    }

    #[test]
    fn push_spec_parses_with_timeout() {
        let t = r#"
            [[strategy]]
            id = "notify-done"
            priority = 100
            [strategy.spec]
            kind = "push"
            notify_via = "host/webhook-notify"
            timeout_ms = 30000
        "#;
        let w = parse(t).expect("parse");
        match &w.strategies[0].spec {
            CompletionStrategySpecDecl::Push {
                notify_via,
                timeout_ms,
            } => {
                assert_eq!(notify_via, "host/webhook-notify");
                assert_eq!(*timeout_ms, 30000);
            }
            CompletionStrategySpecDecl::Poll(_) => panic!("expected push"),
        }
    }

    /// 실제 매니페스트에서 완료 전략만 읽는다. 다른 contributes 항목은 무시한다.
    #[derive(Deserialize)]
    struct BundledManifestProbe {
        contributes: BundledContributesProbe,
    }

    #[derive(Deserialize)]
    struct BundledContributesProbe {
        #[serde(default)]
        completion_strategy: Vec<CompletionStrategyDecl>,
    }

    fn load_bundled_completion_strategies(plugin_id: &str) -> Vec<CompletionStrategyDecl> {
        let path = tasty_test_support::official_plugin_dir(plugin_id).join("tasty-plugin.toml");
        let s = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} read 실패: {e}", path.display()));
        let parsed: BundledManifestProbe =
            toml::from_str(&s).unwrap_or_else(|e| panic!("{} parse 실패: {e}", path.display()));
        parsed.contributes.completion_strategy
    }

    fn spawn_wait_poll_decl(strategies: &[CompletionStrategyDecl]) -> &PollStrategyDecl {
        let spawn_wait = strategies
            .iter()
            .find(|s| s.id == "spawn-wait")
            .expect("spawn-wait strategy declared in manifest");
        match &spawn_wait.spec {
            CompletionStrategySpecDecl::Poll(decl) => decl,
            CompletionStrategySpecDecl::Push { .. } => panic!("spawn-wait expected to be poll"),
        }
    }

    /// spawn 응답의 child_surface_id를 claude.state의 surface_id로 넘겨야 한다.
    #[test]
    fn claude_spawn_wait_manifest_maps_child_surface_id_to_surface_id() {
        let strategies = load_bundled_completion_strategies("com.tasty.claude");
        let decl = spawn_wait_poll_decl(&strategies);
        assert_eq!(
            decl.map_from_response
                .get("child_surface_id")
                .map(String::as_str),
            Some("surface_id")
        );
    }

    /// codex.state는 claude와 달리 대상 인자 이름이 surface다.
    #[test]
    fn codex_spawn_wait_manifest_maps_child_surface_id_to_surface() {
        let strategies = load_bundled_completion_strategies("com.tasty.codex");
        let decl = spawn_wait_poll_decl(&strategies);
        assert_eq!(
            decl.map_from_response
                .get("child_surface_id")
                .map(String::as_str),
            Some("surface")
        );
    }

    #[test]
    fn validate_rejects_bad_short_name() {
        let decl = CompletionStrategyDecl {
            id: "Bad/Name".into(),
            priority: 1,
            display_name_i18n_key: None,
            disabled: false,
            default_for_methods: vec![],
            spec: CompletionStrategySpecDecl::Push {
                notify_via: "host/x".into(),
                timeout_ms: 1000,
            },
        };
        assert!(matches!(
            validate_completion_strategy_decl(&decl),
            Err(CompletionStrategyDeclError::InvalidShortName(_))
        ));
    }
}
