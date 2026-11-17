//! 추가 의존성 없이 매니페스트 소스의 Agent 권한 토큰을 읽는다.
//! 지원하지 않는 형태를 만나면 항목을 누락시키지 않고 panic한다.

use std::collections::BTreeMap;

/// `Permission::as_token` 의 팔 하나를 읽은 형태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenForm {
    /// 고정 토큰 — `Self::Network => "network".into()`.
    Literal(String),
    /// scoped — `Self::Extension(t) => format!("ext:{t}")`. `:` 로 끝나는 고정 prefix 다.
    Prefixed(String),
}

/// Permission::as_token의 match를 variant별 토큰 형태로 읽는다.
/// 여러 variant를 |로 묶거나 지원하지 않는 본문 형식이면 panic한다.
pub fn permission_tokens(src: &str) -> BTreeMap<String, TokenForm> {
    use crate::match_arms::{Source, matching_close};
    let source = Source::new(src);
    let bodies = source.fn_bodies("as_token");
    assert_eq!(
        bodies.len(),
        1,
        "`fn as_token` 정의가 {} 개다 — 판독기는 `Permission` 의 것 하나를 가정한다",
        bodies.len()
    );
    let body = &bodies[0];
    let block = source
        .code_slice(body)
        .find("match self")
        .and_then(|k| {
            let from = body.start + k;
            source.code[from..].find('{').map(|o| from + o)
        })
        .and_then(|open| matching_close(&source.code, open).map(|close| open..close + 1))
        .expect("`as_token` 의 `match self {{ … }}` 를 못 찾았다");
    let arms = source
        .match_arms(block)
        .unwrap_or_else(|e| panic!("`as_token` 의 팔을 못 읽었다 — {e}"));
    let mut out = BTreeMap::new();
    for arm in arms {
        let pattern = source.slice(&arm.pattern);
        let alts = source.alternatives(&arm.pattern);
        let variant = pattern
            .strip_prefix("Self::")
            .filter(|_| alts.len() == 1 && arm.guard.is_none())
            .unwrap_or_else(|| panic!("`as_token` 팔 패턴을 못 읽었다: `{pattern}`"));
        // `Extension(target)` 처럼 payload 가 붙은 팔은 이름만 남긴다.
        let name = variant
            .split(['(', ' ', '{'])
            .next()
            .unwrap_or(variant)
            .to_string();
        let rhs = source.slice(&arm.body).trim();
        // rustfmt 가 긴 팔을 `=> { … }` 블록으로 내린 형태 — 문장 없는 식 하나면 그 식이다.
        let rhs = match rhs.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
            Some(inner) if !source.code_slice(&arm.body).contains(';') => inner.trim(),
            _ => rhs,
        };
        let form = if let Some(rest) = rhs.strip_prefix("format!(\"") {
            let (prefix, _) = rest
                .split_once('{')
                .unwrap_or_else(|| panic!("scoped 팔을 못 읽었다: {rhs}"));
            assert!(
                prefix.ends_with(':'),
                "scoped 토큰 prefix 는 ':' 로 끝나야 한다: {prefix}"
            );
            TokenForm::Prefixed(prefix.to_string())
        } else if let Some(rest) = rhs.strip_prefix('"') {
            let (tok, tail) = rest
                .split_once('"')
                .unwrap_or_else(|| panic!("팔을 못 읽었다: {rhs}"));
            assert!(
                matches!(tail, ".into()" | ".to_string()" | ".to_owned()"),
                "고정 토큰 팔의 본문이 `\"…\".into()` 꼴이 아니다: {rhs}"
            );
            TokenForm::Literal(tok.to_string())
        } else {
            panic!("`as_token` 팔 본문이 리터럴도 `format!` 도 아니다: {rhs}");
        };
        out.insert(name, form);
    }
    assert!(
        !out.is_empty(),
        "as_token에서 match 분기가 한 건도 나오지 않았다. 함수 내용과 지원하는 문법을 확인한다."
    );
    out
}
