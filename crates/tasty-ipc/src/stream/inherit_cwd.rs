//! 원격 생성 요청이 싣는 cwd 상속 원본. client가 자기 `inherit_cwd` 설정과 자기 View의 선택으로 정한다.
//!
//! wire 값은 세 가지다. 필드 생략(또는 null)은 기준 없음으로, 서버가 자기 View 선택과 자기 설정을 쓴다.
//! `{"surface": N}`은 원격 surface N의 실행 시점 cwd를 잇는다는 뜻이고, `"none"`은 상속하지 않는다는 뜻이다.
//! 이 빌드가 모르는 값은 [`InheritCwd::Unknown`]으로 읽는다. 역직렬화가 실패하면 서버 hub가 op 전체를
//! 회신 없이 버리므로, 값을 보존해 받은 쪽이 요청을 거절하게 한다.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// [`InheritCwd::Surface`]의 wire 키.
const SURFACE_KEY: &str = "surface";
/// [`InheritCwd::NoInherit`]의 wire 값.
const NO_INHERIT: &str = "none";

#[derive(Debug, Clone, PartialEq)]
pub enum InheritCwd {
    /// 이 원격 surface의 실행 시점 cwd를 잇는다. wire `{"surface": N}`.
    Surface(u32),
    /// 상속하지 않는다. 서버 선택도 쓰지 않는다. wire `"none"`.
    NoInherit,
    /// 이 빌드가 모르는 값. 받은 원래 값을 그대로 보존해 다시 직렬화해도 같은 값이 된다.
    Unknown(Value),
}

impl InheritCwd {
    /// client가 싣는 값. client 설정(`inherit`)이 꺼져 있으면 "상속 안 함"이고, 켜져 있으면 client가 고른
    /// surface다. 고를 surface가 없으면 싣지 않는다(서버 fallback).
    pub fn of_client(inherit: bool, source: Option<u32>) -> Option<Self> {
        if !inherit {
            return Some(InheritCwd::NoInherit);
        }
        source.map(InheritCwd::Surface)
    }

    /// 상속 원본 surface. 다른 값은 None이다.
    pub fn surface(&self) -> Option<u32> {
        match self {
            InheritCwd::Surface(id) => Some(*id),
            _ => None,
        }
    }

    /// 거절 사유에 쓰는 원래 wire 값. 사용자 문구가 아니다.
    pub fn wire_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl Serialize for InheritCwd {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            InheritCwd::Surface(id) => {
                let mut object = serde_json::Map::new();
                object.insert(SURFACE_KEY.into(), Value::from(*id));
                Value::Object(object).serialize(serializer)
            }
            InheritCwd::NoInherit => serializer.serialize_str(NO_INHERIT),
            InheritCwd::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for InheritCwd {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(Self::of_value(value))
    }
}

impl InheritCwd {
    fn of_value(value: Value) -> Self {
        if value.as_str() == Some(NO_INHERIT) {
            return InheritCwd::NoInherit;
        }
        let surface = value
            .as_object()
            .filter(|object| object.len() == 1)
            .and_then(|object| object.get(SURFACE_KEY))
            .and_then(Value::as_u64)
            .and_then(|id| u32::try_from(id).ok());
        match surface {
            Some(id) => InheritCwd::Surface(id),
            None => InheritCwd::Unknown(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InheritCwd;
    use serde_json::json;

    #[test]
    fn known_values_round_trip_in_their_wire_shape() {
        for (value, wire) in [
            (InheritCwd::Surface(12), json!({"surface": 12})),
            (InheritCwd::NoInherit, json!("none")),
        ] {
            assert_eq!(serde_json::to_value(&value).unwrap(), wire);
            assert_eq!(serde_json::from_value::<InheritCwd>(wire).unwrap(), value);
        }
    }

    /// 모르는 값을 상속 안 함이나 생략으로 읽으면 client 의도와 다른 cwd로 만든다. 받은 쪽이 거절하도록
    /// 원래 값 그대로 Unknown 이 된다.
    #[test]
    fn an_unknown_value_is_kept_as_unknown() {
        for wire in [
            json!("all"),
            json!("None"),
            json!(5),
            json!(true),
            json!({"surface": "5"}),
            json!({"surface": -1}),
            json!({"surface": 4294967296u64}),
            json!({"surface": 1, "extra": 2}),
            json!({"pane": 1}),
            json!([1]),
        ] {
            let read = serde_json::from_value::<InheritCwd>(wire.clone()).unwrap();
            assert_eq!(read, InheritCwd::Unknown(wire.clone()), "{wire}");
            assert_eq!(serde_json::to_value(&read).unwrap(), wire);
        }
    }

    fn op_frame(inherit_cwd: serde_json::Value) -> String {
        let mut op = json!({
            "event": "structural_op",
            "op_id": 7,
            "op": {"kind": "new_tab", "anchor_surface_id": 3, "surface_kind": "terminal", "params": {}},
            "origin": "user",
        });
        if !inherit_cwd.is_null() {
            op["op"]["inherit_cwd"] = inherit_cwd;
        }
        op.to_string()
    }

    fn inherit_of(frame: &str) -> Option<InheritCwd> {
        match serde_json::from_str::<crate::stream::StreamControl>(frame).expect("op frame") {
            crate::stream::StreamControl::StructuralOp { op, .. } => op.inherit_cwd().cloned(),
            other => panic!("not an op: {other:?}"),
        }
    }

    /// 필드가 없는 옛 client의 op는 그대로 읽히고 기준 없음(fallback)이다. 모르는 값도 op 전체를 깨지 않는다.
    /// 깨지면 서버 hub가 그 프레임을 회신 없이 버려 client가 결과를 기다린다.
    #[test]
    fn creation_ops_read_every_inherit_cwd_shape() {
        assert_eq!(inherit_of(&op_frame(serde_json::Value::Null)), None);
        assert_eq!(
            inherit_of(&op_frame(json!({"surface": 9}))),
            Some(InheritCwd::Surface(9))
        );
        assert_eq!(
            inherit_of(&op_frame(json!("none"))),
            Some(InheritCwd::NoInherit)
        );
        assert_eq!(
            inherit_of(&op_frame(json!({"tab": 9}))),
            Some(InheritCwd::Unknown(json!({"tab": 9})))
        );
    }

    /// 새 필드를 모르는 옛 서버는 그 필드를 무시하고 op를 받는다(fallback 규칙과 같은 결과).
    #[test]
    fn an_older_server_ignores_the_field() {
        #[derive(Debug, serde::Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        #[allow(dead_code)] // 이유: 옛 서버의 op 모양만 흉내 낸다. 필드는 읽지 않는다.
        enum OlderOp {
            NewTab {
                anchor_surface_id: u32,
                surface_kind: String,
                params: serde_json::Value,
            },
        }
        let op = crate::stream::StructuralOp::NewTab {
            anchor_surface_id: 3,
            surface_kind: "terminal".into(),
            params: json!({}),
            inherit_cwd: Some(InheritCwd::Surface(4)),
        };
        let wire = serde_json::to_value(&op).unwrap();
        assert_eq!(wire["inherit_cwd"], json!({"surface": 4}));
        assert!(serde_json::from_value::<OlderOp>(wire).is_ok());
    }

    /// 기준이 없으면 필드를 싣지 않는다. 옛 서버와 같은 바이트를 보낸다.
    #[test]
    fn an_absent_source_is_not_written() {
        let op = crate::stream::StructuralOp::SplitPane {
            anchor_surface_id: 3,
            direction: crate::stream::SplitAxis::Vertical,
            surface_kind: "terminal".into(),
            params: json!({}),
            inherit_cwd: None,
        };
        assert!(
            serde_json::to_value(&op)
                .unwrap()
                .get("inherit_cwd")
                .is_none()
        );
    }

    /// 원격 자식 요청도 같은 값 형태를 쓰고, 모르는 값이 메시지 전체를 깨지 않는다.
    #[test]
    fn agent_calls_read_the_same_shapes() {
        let call = |inherit_cwd: serde_json::Value| {
            let mut msg = json!({
                "event": "agent_call_request", "call_id": 1, "namespace": "claude",
                "method": "spawn", "anchor_surface_id": 3, "params": {},
            });
            if !inherit_cwd.is_null() {
                msg["inherit_cwd"] = inherit_cwd;
            }
            match serde_json::from_value::<crate::agent_child::AgentClientMsg>(msg).expect("call") {
                crate::agent_child::AgentClientMsg::AgentCallRequest { inherit_cwd, .. } => {
                    inherit_cwd
                }
                other => panic!("not a call: {other:?}"),
            }
        };
        assert_eq!(call(serde_json::Value::Null), None);
        assert_eq!(call(json!({"surface": 5})), Some(InheritCwd::Surface(5)));
        assert_eq!(call(json!("none")), Some(InheritCwd::NoInherit));
        assert_eq!(call(json!(1)), Some(InheritCwd::Unknown(json!(1))));
    }
}
