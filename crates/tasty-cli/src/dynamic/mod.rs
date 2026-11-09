//! 매니페스트 `contributes.cli`를 런타임에 clap 서브커맨드로 등록하고,
//! 매칭된 결과를 JSON-RPC 메서드+params로 변환한다.
//!
//! 호스트 정적 `Cli` 파싱이 `InvalidSubcommand`로 실패할 때 진입한다 — 정적 우선,
//! 정적이 모르는 이름만 plugin CLI에서 찾는다.
//!
//! 호스트 명령과 이름이 겹치는 플러그인 명령은 경고하고 등록하지 않는다.
//! 중복 등록은 clap의 debug 검증에서 전체 명령 트리를 실패시킬 수 있다.

mod build;
mod request;
mod stdin;

pub use build::{DiscoveredPlugins, build_augmented_cli, discover_plugin_clis, discover_plugins};
pub use request::{matches_to_request, wants_child_table};

use serde_json::{Map, Value};
use std::collections::HashMap;
use tasty_plugin_manifest::{CliCommandDecl, PollingDecl};

/// `spawn` / `tell` 같이 1 차 응답 후 chained wait 가 필요한 명령의 실행 계획.
/// `matches_to_request` 가 manifest `AutoWaitDecl` + 사용자 CLI 입력을 합쳐 빌드.
#[derive(Debug, Clone)]
pub struct AutoWaitPlan {
    pub method: String,
    pub polling: PollingDecl,
    pub map_from_response: HashMap<String, String>,
    pub map_from_request: HashMap<String, String>,
    pub timeout_field: String,
    /// 원 요청 params snapshot. wait params 구성 시 `map_from_request` 매핑과
    /// timeout 키 추출에 사용.
    pub request_params: Map<String, Value>,
    /// `--no-wait` 가 true 면 chain skip — caller 가 1 차 응답만 출력하고 종료.
    pub skipped: bool,
}

/// 한 plugin이 contribute한 CLI 묶음.
#[derive(Debug, Clone)]
pub struct PluginCliEntry {
    pub cli: CliCommandDecl,
    /// 이 명령이 호출자 값 형식 오류를 거절할 provider. 소유 플러그인이 선언한 provider와
    /// `requires_caller_context`를 합친 것이다.
    pub strict_caller_contexts: Vec<String>,
    /// 매니페스트 `contributes.remote_attach.methods` 의 전체 IPC 메서드 이름(`<namespace>.<method>`).
    /// 이 메서드만 attach mirror 대상의 경로 인자를 실행 머신에 맡긴다.
    pub remote_methods: Vec<String>,
}

#[cfg(test)]
mod tests;
