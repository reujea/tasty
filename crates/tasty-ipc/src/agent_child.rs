//! attach holder 연결로 원격 자식 에이전트를 만들고 관리하며, 그 완료를 부모 쪽으로 돌려보내는 제어 메시지.
//!
//! 부모(Claude/Codex 대화 또는 터미널)는 client 인스턴스에 있고 자식 프로세스는 서버 인스턴스에서 돈다.
//! client는 자기 mirror surface ID로 요청하고 서버는 자기 plugin으로 실행한다. 자식의 훅은 서버에서 처리되고
//! 그 결과는 [`AgentServerMsg::ChildNotice`]로 holder 연결을 거쳐 client에 간다. client는 자기 child
//! registry로 부모를 정해 기존 종점(완료 로그 훅)에 적용하고, plugin 고유 결과는 같은 namespace 의
//! 수신 메서드에 그대로 넘긴다. host 는 그 본문을 해석하지 않는다.
//!
//! 출처는 서버가 발급한 비공개 토큰([`AgentServerMsg::AgentHelloAck`])으로 정한다. 공개 값인 `instance_id`나
//! 재연결마다 바뀌는 연결 ID는 쓰지 않는다. 토큰은 holder 연결로만 오간다.
//!
//! 구 서버는 이 event 를 모르고 버린다. client 는 attach descriptor 에 [`CAPABILITY`] 가 있는 연결에만 보낸다.

use serde::{Deserialize, Serialize};

/// 서버가 원격 자식 에이전트 요청과 완료 통지를 지원한다는 capability 이름.
pub const CAPABILITY: &str = "ipc.stream.agent-children";
/// [`CAPABILITY`] 의 버전.
pub const VERSION: u32 = 1;

// 실행할 수 있는 plugin 과 메서드는 이 파일이 정하지 않는다. 각 plugin 매니페스트의
// `contributes.remote_attach` 선언(자기 namespace 메서드 이름)을 서버 host 가 읽어 정한다.
// host 는 메서드의 의미나 provider 별 훅 이름을 해석하지 않는다.

/// 원격 자식의 프로세스 종료. host 가 PTY 종료에서 만든다.
pub const PROCESS_EXIT: &str = "process-exit";

/// 원격 holder call 문맥을 host 가 넣는 plugin 인자 이름. 일반 namespace 요청은 이 이름을 쓸 수 없다.
pub const CONTEXT_PARAM: &str = "attach_context";
/// host 가 확인한 원격 자식 surface 를 싣는 plugin 인자 이름.
pub const TARGET_PARAM: &str = "attach_target_surface";

/// 일반 namespace 요청(공개 IPC·CLI·다른 plugin)에 실려 오면 안 되는 원격 출처 표지. 있으면 그 이름이다.
/// 원격 holder 의 call 만 host 가 이 표지를 넣는다. 호출자 plugin ID 만으로 원격 출처로 보지 않는다.
pub fn reserved_marker(params: &serde_json::Value) -> Option<&'static str> {
    let map = params.as_object()?;
    if map.contains_key(CONTEXT_PARAM) {
        return Some(CONTEXT_PARAM);
    }
    if map.contains_key(TARGET_PARAM) {
        return Some(TARGET_PARAM);
    }
    map.get("parent_external")
        .and_then(serde_json::Value::as_str)
        .filter(|key| key.starts_with("attach:"))
        .map(|_| "parent_external")
}

/// 서버 child registry 에서 원격 출처 자식의 부모 키 접두사. 뒤에 출처 번호가 붙는다(`attach:o3`).
/// 비공개 토큰이 아니라 공개 순번이므로 registry·응답에 실어도 된다.
pub const PARENT_PREFIX: &str = "attach:o";

/// 서버가 원격 출처 부모 키를 만든다.
pub fn parent_key(origin: u64) -> String {
    format!("{PARENT_PREFIX}{origin}")
}

/// 원격 출처 부모 키인지. 서버 로컬 호출이 이 접두사로 부모를 지어내지 못하게 막는 데 쓴다.
pub fn is_attach_parent(key: &str) -> bool {
    key.starts_with("attach:")
}

/// client → 서버.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentClientMsg {
    /// 이 연결의 출처를 정한다. 처음이면 `None`, 재연결이면 전에 받은 토큰을 보낸다.
    AgentHello {
        #[serde(default)]
        origin_token: Option<String>,
    },
    /// 서버 plugin 메서드 하나를 holder 권한으로 실행한다. anchor 는 대상 pane 의 원격 surface 다.
    /// 같은 출처가 같은 `call_id` 로 다시 보내면 서버는 다시 실행하지 않고 보관한 결과를 돌려준다
    /// (결과를 받지 못하고 끊긴 client 의 재조회).
    ///
    /// `namespace` 는 plugin IPC 접두사(`claude`)이고 `method` 는 접두사 없는 이름이다. 부모·위치·대상은
    /// host 가 정한다. 서버는 `params` 의 같은 이름 필드를 지우고 출처 부모 키·workspace·pane 과, 확인한
    /// `target_surface_id` 를 넣어 넘긴다. client 는 자기 쪽 문맥 키(`_tasty_context`·surface·parent)를 보내지 않는다.
    AgentCallRequest {
        call_id: u64,
        namespace: String,
        method: String,
        /// 대상 pane 에 있는 원격 surface. 실행 위치(workspace·pane)를 정한다.
        anchor_surface_id: u32,
        /// 다룰 기존 자식(원격 surface). 서버는 이 출처의 자식일 때만 넘긴다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_surface_id: Option<u32>,
        #[serde(default)]
        params: serde_json::Value,
        /// 새 자식 터미널의 cwd 상속 원본(원격 surface 또는 상속 안 함). 생략하면 서버가 자기 View
        /// 선택과 자기 설정을 쓴다. 서버는 이 값을 확인해 host 예약 인자로 넘긴다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inherit_cwd: Option<crate::stream::InheritCwd>,
    },
    /// 이 call 로 생긴 자식의 부모 경로를 client 가 다 준비했다. 서버는 그 전까지 그 자식의 통지를
    /// 보관만 하고 보내지 않는다. 준비 전에 온 완료가 수신처 없이 버려지지 않게 한다.
    AgentCallCommit {
        call_id: u64,
        remote_surface_ids: Vec<u32>,
    },
    /// 받은 통지의 처리 결과.
    ChildNoticeAck { key: String, outcome: AckOutcome },
}

/// 서버 → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentServerMsg {
    /// 이 연결에 묶은 출처. 받은 토큰을 재연결 때 다시 보낸다. 토큰은 서버 프로세스 단위이며 같은 client 의
    /// 여러 연결이 함께 쓴다. `resumed` 가 false 인데 토큰을 보냈다면 서버가 그 토큰을 모른다(서버 재시작).
    /// 그때 client 는 이전 출처의 자식을 결과불명으로 표시하고 이전 `call_id` 를 다시 보내지 않는다.
    AgentHelloAck {
        origin_token: String,
        origin: u64,
        resumed: bool,
    },
    /// [`AgentClientMsg::AgentCallRequest`] 의 결과. 성공이면 plugin 응답 그대로다.
    AgentCallResult {
        call_id: u64,
        outcome: CallOutcome,
        /// 이 call 동안 이 출처의 자식으로 새로 생긴 원격 surface. 서버 child registry 의 차분이며
        /// plugin 응답 본문에서 읽지 않는다. 실패한 call 도 자식을 남겼을 수 있다.
        #[serde(default)]
        spawned: Vec<u32>,
    },
    /// 원격 자식의 상태·이벤트·결과. 같은 `key` 는 같은 통지의 재전송이다. 서버는 자식마다 확인받은 순서대로
    /// 하나씩 보낸다. `seq` 는 서버 프로세스 안의 순서이며 client 는 [`Notice::State`] 에만 써서 오래된 상태가
    /// 새 상태를 덮지 않게 한다. 이벤트·결과는 key 중복 제거와 자식별 순서로만 처리하고 버리지 않는다.
    ChildNotice {
        key: String,
        seq: u64,
        remote_surface_id: u32,
        notice: Notice,
    },
}

/// call 의 결과. 성공이면 plugin 응답 그대로다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CallOutcome {
    Ok { result: serde_json::Value },
    Error(CallError),
}

/// 서버 plugin 이나 holder 검사가 낸 오류.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallError {
    pub code: i32,
    pub message: String,
}

/// 원격 자식에게 일어난 일. client 는 자기 mirror surface 에 같은 효과를 적용한다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Notice {
    /// child registry 상태(`idle`·`active`·`needs_input`).
    State { state: String },
    /// surface hook 이벤트 이름. 자식을 만든 plugin 이 매니페스트 `hook_events` 에 선언한 이름이거나 host 가 만든
    /// [`PROCESS_EXIT`] 다. 양쪽 host 가 선언으로 검사하며 의미는 해석하지 않는다.
    Event { name: String },
    /// 자식을 만든 plugin 이 `remote_attach.relay_meta` 에 선언한 surface 메타데이터. `None` 은 삭제다.
    Meta {
        key: String,
        #[serde(default)]
        value: Option<String>,
    },
    /// plugin 고유 결과. client 는 `namespace` plugin 이 선언한 수신 메서드에 `payload` 를 넘긴다.
    Plugin {
        namespace: String,
        payload: serde_json::Value,
    },
}

/// client 의 통지 처리 결과.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AckOutcome {
    /// 부모 종점에 기록했다.
    Recorded,
    /// 이미 기록한 통지다. 다시 기록하지 않았다.
    Duplicate,
    /// 지금은 처리할 수 없다(매핑 대기·종점 일시 실패). 서버가 나중에 다시 보낸다.
    RetryLater,
    /// 이 client 가 처리할 수 없는 통지다. 다시 보내지 않는다.
    Rejected { reason: String },
}

impl AckOutcome {
    /// 서버가 통지를 지워도 되는지.
    pub fn settles(&self) -> bool {
        !matches!(self, Self::RetryLater)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_attach_markers_are_reserved() {
        use serde_json::json;
        assert_eq!(
            reserved_marker(&json!({ "attach_context": "x" })),
            Some(CONTEXT_PARAM)
        );
        assert_eq!(
            reserved_marker(&json!({ "attach_target_surface": 3 })),
            Some(TARGET_PARAM)
        );
        assert_eq!(
            reserved_marker(&json!({ "parent_external": "attach:o1" })),
            Some("parent_external")
        );
        assert_eq!(
            reserved_marker(&json!({ "parent_external": "codex:abc" })),
            None
        );
        assert_eq!(
            reserved_marker(&json!({ "prompt": "attach_context" })),
            None
        );
    }
    use serde_json::json;

    #[test]
    fn messages_round_trip_with_their_event_tags() {
        let hello = AgentClientMsg::AgentHello { origin_token: None };
        assert_eq!(
            serde_json::to_value(&hello).unwrap(),
            json!({"event": "agent_hello", "origin_token": null})
        );
        let notice = AgentServerMsg::ChildNotice {
            key: "s:1".into(),
            seq: 1,
            remote_surface_id: 9,
            notice: Notice::Event {
                name: "claude-idle".into(),
            },
        };
        let v = serde_json::to_value(&notice).unwrap();
        assert_eq!(v["event"], "child_notice");
        assert_eq!(v["notice"], json!({"kind": "event", "name": "claude-idle"}));
        assert_eq!(serde_json::from_value::<AgentServerMsg>(v).unwrap(), notice);
        let ack = AgentClientMsg::ChildNoticeAck {
            key: "s:1".into(),
            outcome: AckOutcome::Rejected { reason: "x".into() },
        };
        let v = serde_json::to_value(&ack).unwrap();
        assert_eq!(v["outcome"], json!({"outcome": "rejected", "reason": "x"}));
        assert_eq!(serde_json::from_value::<AgentClientMsg>(v).unwrap(), ack);
    }

    /// 다른 기능의 raw event 와 섞여도 서로의 파서가 가져가지 않는다.
    #[test]
    fn other_raw_events_do_not_parse_as_agent_messages() {
        let other = json!({"event": "explorer_op_request", "op_id": 1, "op": {}});
        assert!(serde_json::from_value::<AgentClientMsg>(other.clone()).is_err());
        assert!(serde_json::from_value::<AgentServerMsg>(other).is_err());
    }

    #[test]
    fn only_retry_later_keeps_a_notice() {
        assert!(AckOutcome::Recorded.settles());
        assert!(AckOutcome::Duplicate.settles());
        assert!(AckOutcome::Rejected { reason: "r".into() }.settles());
        assert!(!AckOutcome::RetryLater.settles());
    }

    #[test]
    fn the_parent_key_is_public_and_recognizable() {
        assert_eq!(parent_key(3), "attach:o3");
        assert!(is_attach_parent("attach:o3"));
        assert!(!is_attach_parent("codex:t1"));
    }
}
