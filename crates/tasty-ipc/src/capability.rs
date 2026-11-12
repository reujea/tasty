//! system.info에 지원 기능의 이름과 버전을 선언한다. 패키지 버전만으로는 빌드별 지원 여부를 알 수 없다.
//! 기존 client는 모르는 항목을 무시한다. 선언값은 해당 기능의 상수·메서드 표에서 가져온다.

/// 서버가 선언하는 협상 가능한 기능 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// 안정된 기능 이름. 의미가 바뀌면 기능 버전을 올린다.
    pub name: &'static str,
    /// 기능 버전. 낮아지지 않는다.
    pub version: u32,
}

/// 이 빌드의 지원 기능. 새 기능은 항목을 추가하고 기존 계약 변경은 해당 기능 버전에 반영한다.
pub const NOTIFICATION_LEVELS: &str = "notification.levels";
pub const NOTIFICATION_LEVELS_VERSION: u32 = 1;

pub const CAPABILITIES: &[Capability] = &[
    Capability {
        name: NOTIFICATION_LEVELS,
        version: NOTIFICATION_LEVELS_VERSION,
    },
    Capability {
        name: "ipc.capabilities",
        version: 1,
    },
    Capability {
        name: "ipc.method-effect",
        version: 1,
    },
    Capability {
        name: "ipc.method-since",
        version: 1,
    },
    // 구 서버는 response_timeout_ms를 무시하므로 호출 전에 지원 여부를 확인한다.
    Capability {
        name: RESPONSE_TIMEOUT,
        version: RESPONSE_TIMEOUT_VERSION,
    },
    // 큐에서 만료되면 실행하지 않고 -32067, 시작한 뒤 만료되면 -32061이다.
    // 기존 response-timeout을 요구하는 client의 호환을 위해 별도 기능 이름으로 선언한다.
    Capability {
        name: RESPONSE_TIMEOUT_NOT_RUN,
        version: 1,
    },
    // 멱등 키 버전 1은 engine 라우터, 2는 App, 3은 GUI debug step과
    // 등록된 호스트 메서드의 namespace forward까지 포함한다. plugin 고유 이름은 Outside다.
    // 메서드 표가 요구하는 가장 높은 버전을 선언해 구현 범위와 맞춘다.
    Capability {
        name: "ipc.idempotency-key",
        version: crate::method_meta::KEY_KEPT_IN_STRUCTURE_JOURNAL,
    },
    // 스트림 handshake가 실제로 비교하는 상수를 사용한다.
    Capability {
        name: "ipc.stream",
        version: crate::stream::STREAM_PROTO,
    },
    // STREAM_PROTO는 동등 비교하므로 올리면 기존 peer가 끊긴다. 추가 기능은 별도 이름을 쓴다.
    Capability {
        name: "ipc.stream.loss-notify",
        version: 1,
    },
    // client는 attach descriptor로 연결마다 확인한다. 여기 선언은 조회용 사본이다.
    Capability {
        name: crate::stream::RESIZE_ACK_CAPABILITY,
        version: crate::stream::RESIZE_ACK_VERSION,
    },
    Capability {
        name: crate::stream::INPUT_ACK_CAPABILITY,
        version: 1,
    },
    // client는 attach descriptor로 연결마다 확인한다. 원격 "+ 새 워크스페이스"는 조회 연결이라
    // descriptor가 없으므로 이 선언으로 확인할 수 있다.
    Capability {
        name: crate::stream::INHERIT_CWD_CAPABILITY,
        version: 1,
    },
    // client는 attach descriptor로 연결마다 확인한다. 여기 선언은 조회용 사본이다.
    Capability {
        name: crate::explorer_op::CAPABILITY,
        version: crate::explorer_op::VERSION,
    },
    // client는 attach descriptor로 연결마다 확인한다. 여기 선언은 조회용 사본이다.
    Capability {
        name: crate::agent_child::CAPABILITY,
        version: crate::agent_child::VERSION,
    },
    // client는 attach descriptor로 연결마다 확인한다. 여기 선언은 조회용 사본이다.
    Capability {
        name: crate::terminal_read::CAPABILITY,
        version: crate::terminal_read::VERSION,
    },
    // client는 attach descriptor로 연결마다 확인한다. 여기 선언은 조회용 사본이다.
    Capability {
        name: crate::file_download::CAPABILITY,
        version: crate::file_download::VERSION,
    },
    // 구 서버는 cursor 인자를 버리고 공유 마크를 읽는다. 전송 전에 지원 여부를 확인한다.
    Capability {
        name: crate::output_cursor::CAPABILITY,
        version: crate::output_cursor::VERSION,
    }, // 봉투의 caller_agent_id 를 Local 호출자의 텔레메트리·감사 표시 ID 로 쓴다. 구 서버는 필드를
    // 무시하고 `_host` 로 기록할 뿐 요청은 그대로 처리하므로 client 는 전송 전에 묻지 않는다.
    Capability {
        name: "ipc.caller-agent-id",
        version: 1,
    },
];

/// 서버 선언과 client의 지원 확인에 함께 쓰는 이름.
pub const RESPONSE_TIMEOUT: &str = "ipc.response-timeout";
/// 응답 대기 상한 기능의 버전.
pub const RESPONSE_TIMEOUT_VERSION: u32 = 1;
/// 큐에서 만료된 요청을 실행하지 않고 `-32067` 로 답한다는 선언의 이름.
pub const RESPONSE_TIMEOUT_NOT_RUN: &str = "ipc.response-timeout.not-run";

/// `system.info` 가 싣는 모양. `[{ "name": …, "version": … }, …]`.
pub fn capabilities_json() -> serde_json::Value {
    serde_json::Value::Array(
        CAPABILITIES
            .iter()
            .map(|c| serde_json::json!({ "name": c.name, "version": c.version }))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_capability_name_is_declared_twice() {
        let mut names: Vec<&str> = CAPABILITIES.iter().map(|c| c.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "중복된 capability 이름: {names:?}");
    }

    #[test]
    fn every_declared_version_starts_at_one() {
        for c in CAPABILITIES {
            assert!(c.version >= 1, "{} 의 판이 0 이다", c.name);
        }
    }

    #[test]
    fn the_stream_capability_carries_the_constant_the_server_compares() {
        let declared = CAPABILITIES
            .iter()
            .find(|c| c.name == "ipc.stream")
            .expect("stream capability 가 사라졌다");
        assert_eq!(declared.version, crate::stream::STREAM_PROTO);
    }

    #[test]
    fn every_name_a_client_requires_is_declared_at_the_version_it_requires() {
        for (name, required) in [
            (RESPONSE_TIMEOUT, RESPONSE_TIMEOUT_VERSION),
            (
                crate::client::IDEMPOTENCY_CAPABILITY,
                crate::client::IDEMPOTENCY_CAPABILITY_VERSION,
            ),
            (
                crate::client::IDEMPOTENCY_CAPABILITY,
                crate::method_meta::KEY_KEPT_BY_APP_LAYER,
            ),
            (
                crate::client::IDEMPOTENCY_CAPABILITY,
                crate::method_meta::KEY_KEPT_ON_EVERY_HOST_PATH,
            ),
            (
                crate::output_cursor::CAPABILITY,
                crate::output_cursor::VERSION,
            ),
        ] {
            let declared = CAPABILITIES
                .iter()
                .find(|c| c.name == name)
                .unwrap_or_else(|| panic!("{name} 가 선언돼 있지 않다"));
            assert!(declared.version >= required, "{name}");
        }
    }

    #[test]
    fn the_idempotency_version_is_the_highest_one_the_table_requires() {
        use crate::method_meta::{DEBUG_METHODS, KeyContract, METHOD_TABLE};
        let highest = METHOD_TABLE
            .iter()
            .chain(DEBUG_METHODS)
            .filter_map(|(_, m)| match m.key_contract {
                KeyContract::Kept { since } => Some(since),
                _ => None,
            })
            .max()
            .expect("보존소가 받는 메서드가 하나도 없다");
        let declared = CAPABILITIES
            .iter()
            .find(|c| c.name == crate::client::IDEMPOTENCY_CAPABILITY)
            .expect("멱등 키 capability 가 사라졌다");
        assert_eq!(declared.version, highest);
    }

    #[test]
    fn the_json_shape_is_an_array_of_name_and_version() {
        let v = capabilities_json();
        let arr = v.as_array().expect("배열이어야 한다");
        assert_eq!(arr.len(), CAPABILITIES.len());
        for item in arr {
            assert!(item["name"].is_string(), "{item}");
            assert!(item["version"].is_u64(), "{item}");
        }
    }
}
