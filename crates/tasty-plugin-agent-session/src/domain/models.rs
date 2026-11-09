//! 시작 화면의 **모델 고르기** — 고른 프로필이 쓸 수 있는 모델을 보인다.
//!
//! 쓸 수 있는 모델은 프로필마다 다르다 — 인증(claude.ai 로그인 · 토큰 · 게이트웨이 API 키)에 따라
//! 목록이 갈린다. 목록을 물을 수단이 없으므로 종류별 별칭(`aliases`)을 보이고, 프로필에 `model` 이
//! 있으면 그것을 맨 앞에 둔다. 어느 경우든 직접 입력할 수 있다.
use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::domain::i18n;

/// 한 프로필의 고를 거리 — 데몬이 준 목록, 또는 그 종류의 별칭.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Choices {
    pub models: Vec<String>,
    /// 칸 아래 한 줄 — 목록이 어디서 왔는지 · 왜 별칭인지.
    pub note: String,
}

/// 종류별 별칭 — claude 는 `--model` 이 받는 별칭(`claude --help`, 2.1.304). 다른 종류는 목록을
/// 조회하기 전까지 직접 입력만 한다.
pub fn aliases(kind: &str) -> Vec<String> {
    match kind {
        "claude" => ["fable", "opus", "sonnet", "haiku"]
            .map(str::to_string)
            .to_vec(),
        _ => Vec::new(),
    }
}

/// 조회 전의 고를 거리 — 종류별 별칭.
pub fn fallback(kind: &str) -> Choices {
    Choices {
        models: aliases(kind),
        note: i18n::t("sup.form.model_note_aliases"),
    }
}

/// 프로필의 고를 거리 — 프로필이 정한 모델이 있으면 맨 앞에 둔다.
pub fn for_profile(p: &crate::adapters::driven::store::agents::AgentProfile) -> Choices {
    let mut c = fallback(&p.kind);
    if let Some(m) = p.model.as_ref().filter(|m| !m.trim().is_empty()) {
        c.models.retain(|x| x != m);
        c.models.insert(0, m.clone());
    }
    c
}

/// 시작 화면이 처음 싣는 표 — 프로필 이름 → `{l: 목록, n: 사유}`. 받은 것이 있으면 그것, 없으면 별칭.
pub fn initial_json(
    profiles: &[crate::adapters::driven::store::agents::AgentProfile],
    fetched: &BTreeMap<String, Choices>,
) -> String {
    let map: serde_json::Map<String, Value> = profiles
        .iter()
        .map(|p| {
            let c = fetched
                .get(&p.name)
                .cloned()
                .unwrap_or_else(|| for_profile(p));
            (p.name.clone(), entry(&c))
        })
        .collect();
    script_json(&Value::Object(map))
}

fn entry(c: &Choices) -> Value {
    json!({ "l": c.models, "n": c.note })
}

/// `<script>` 안에 싣는 JSON — `</script>` 로 끊기지 않게 `</` 를 벗긴다.
fn script_json(v: &Value) -> String {
    v.to_string().replace("</", "<\\/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_model_comes_first_without_duplicates() {
        let p = crate::adapters::driven::store::agents::AgentProfile {
            kind: "claude".into(),
            model: Some("opus".into()),
            ..Default::default()
        };
        let c = for_profile(&p);
        assert_eq!(c.models.first().map(String::as_str), Some("opus"));
        assert_eq!(c.models.iter().filter(|m| *m == "opus").count(), 1);
        let custom = crate::adapters::driven::store::agents::AgentProfile {
            kind: "claude".into(),
            model: Some("claude-opus-5-5".into()),
            ..Default::default()
        };
        assert_eq!(for_profile(&custom).models[0], "claude-opus-5-5");
    }

    #[test]
    fn claude_falls_back_to_aliases_and_others_to_typing() {
        assert!(aliases("claude").contains(&"opus".to_string()));
        assert!(aliases("pi").is_empty());
        assert_eq!(fallback("claude").models, aliases("claude"));
    }

    /// 표는 `</script>` 로 끊기지 않는다 — 프로필 이름과 모델은 사용자가 적은 글자다.
    #[test]
    fn script_payloads_cannot_close_the_script_tag() {
        let c = Choices {
            models: vec!["</script><img src=x>".into()],
            note: String::new(),
        };
        let p = crate::adapters::driven::store::agents::AgentProfile {
            name: "x</script>".into(),
            kind: "claude".into(),
            ..Default::default()
        };
        let mut fetched = BTreeMap::new();
        fetched.insert("x</script>".to_string(), c);
        let j = initial_json(&[p], &fetched);
        assert!(!j.contains("</") && j.contains("img src=x"), "{j}");
    }
}
