//! 매니페스트 거절 사유를 사용자에게 보일 문장으로 바꾼다.
//! 로그는 영어 원문을 그대로 쓰고, CLI 경고·설치 오류·Add 미리보기만 이 함수를 쓴다.

use tasty_plugin_manifest::TastyVersionTooOld;

/// 오류 사슬에 최소 Tasty 버전 거절이 있으면 현재 언어의 문장을, 없으면 원문을 돌려준다.
pub fn describe(error: &anyhow::Error) -> String {
    match error.downcast_ref::<TastyVersionTooOld>() {
        Some(old) => {
            tasty_i18n::t_fmt2("plugins.requires_newer_tasty", &old.required, &old.current)
        }
        None => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_refusal_is_described_with_both_versions() {
        let error = anyhow::Error::new(TastyVersionTooOld {
            required: "9.8.7".into(),
            current: "0.14.0".into(),
        });
        let text = describe(&error);
        assert!(text.contains("9.8.7") && text.contains("0.14.0"), "{text}");
        assert_ne!(text, "plugins.requires_newer_tasty", "key must resolve");
    }

    #[test]
    fn other_errors_keep_their_original_text() {
        let error = anyhow::anyhow!("plugin api_version '2' incompatible with host '1'");
        assert_eq!(describe(&error), error.to_string());
    }
}
