//! 저장된 원격 워크스페이스 행이 가리키는 대상. attach 프로필 이름과 원격 workspace id 두 값뿐이다.
//! 프로필 해석과 연결은 호스트가 맡는다. 프로필 이름이 바뀌거나 지워져도 이 값은 따라 바뀌지 않는다.

use serde::{Deserialize, Serialize};

/// 사용자가 GUI로 붙인 원격 워크스페이스 행의 연결 대상.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RemoteBinding {
    /// `tasty-attach` 프로필 이름. 정확히 같은 이름의 프로필로만 연결한다.
    pub profile: String,
    /// 원격 Tasty의 workspace id.
    pub remote_workspace: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binding_survives_a_json_roundtrip() {
        let binding = RemoteBinding {
            profile: "gx10".into(),
            remote_workspace: 7,
        };
        let json = serde_json::to_string(&binding).unwrap();
        assert_eq!(
            serde_json::from_str::<RemoteBinding>(&json).unwrap(),
            binding
        );
    }
}
