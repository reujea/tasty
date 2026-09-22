//! `tasty-plugin-manifest` 의 두 표를 **텍스트로** 읽는다 — 권한 토큰과 contributes 게이트.
//!
//! 왜 링크하지 않고 읽나: 이 크레이트는 의존이 0 이라 콜드 빌드가 1 초 미만이고, 그래서
//! `doc-guards.yml` 이 **경로 필터 없이** 매 push 돌 수 있다(ADR-0138). 그 표들을 링크로
//! 열거하려면 `tasty-plugin-manifest` 를 끌어와야 하는데 그것만으로 전이 의존이 61 개다
//! (serde·toml·serde_json…) — 필터 없는 잡의 전제가 무너진다.
//!
//! 텍스트 판독의 대가는 **판독기가 진짜 표와 갈릴 수 있다**는 것이다. 그 위험은 본체
//! 패키지의 `tests/contributes_gate_readings_agree.rs` 가 받는다 — 거기서는 두 타입을
//! 링크해 런타임에 열거할 수 있으므로 판독본과 열거본을 직접 맞댄다.
//!
//! 판독기는 **모르는 형태를 만나면 panic 한다.** 조용히 건너뛰면 그 항목이 표에서
//! 사라져 소비자가 빈 쪽을 대조하며 통과한다.

use std::collections::BTreeMap;

/// `Permission::as_token` 의 팔 하나를 읽은 형태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenForm {
    /// 고정 토큰 — `Self::Network => "network".into()`.
    Literal(String),
    /// scoped — `Self::Extension(t) => format!("ext:{t}")`. `:` 로 끝나는 고정 prefix 다.
    Prefixed(String),
}

impl TokenForm {
    /// 문서 표에 적히는 형태. 고정은 토큰 그대로, scoped 는 prefix + 자리표시자.
    pub fn doc_form(&self, placeholder: &str) -> String {
        match self {
            TokenForm::Literal(t) => t.clone(),
            TokenForm::Prefixed(p) => format!("{p}{placeholder}"),
        }
    }
}

/// `Permission::as_token` 의 팔을 **variant 이름 → 토큰 형태** 로 읽는다.
///
/// 토큰 문자열의 단일 출처는 그 함수다 — exhaustive match 라 variant 를 늘리면 팔이
/// 강제로 추가된다. 팔은 공용 판정기([`crate::match_arms`])가 떼고, 모르는 모양(여러 variant
/// 를 `|` 로 묶은 팔, 리터럴도 `format!` 도 아닌 본문)은 panic 한다 — 줄 단위로 읽던
/// 판독은 그런 팔을 `continue` 로 건너뛰어, 본문이 여러 줄로 내려간 새 variant 의 토큰이
/// 문서 대조에서 조용히 빠졌다.
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
        "`as_token` 을 읽었는데 팔이 한 건도 안 나왔다 — 표가 빈 것이 아니라 판독기가 \
         형태를 못 맞춘 것이다."
    );
    out
}

/// `contributes_gates!` 표를 **(매니페스트 키, 문서 표기)** 로 읽는다.
///
/// 표기 해석에 토큰 표가 필요하므로 두 소스를 함께 받는다.
pub fn contributes_gates(gates_src: &str, types_src: &str) -> Vec<(String, String)> {
    let tokens = permission_tokens(types_src);
    let start = gates_src
        .find("contributes_gates! {")
        .expect("contributes_gates! 호출을 못 찾았다");
    let body = brace_block(&gates_src[start..]);
    let flat = crate::strip_line_comments(body);

    let mut out = Vec::new();
    let mut rest = flat.as_str();
    while let Some(at) = rest.find("GateToken::") {
        let key = last_string_literal(&rest[..at])
            .unwrap_or_else(|| panic!("게이트 항목 앞에 키 문자열이 없다: {:.80}", &rest[..at]));
        let after = &rest[at + "GateToken::".len()..];
        let doc_form = if let Some(a) = after.strip_prefix("Literal(Permission::") {
            let name = ident(a);
            match tokens.get(&name) {
                Some(f @ TokenForm::Literal(_)) => f.doc_form(""),
                other => panic!("Literal 게이트가 고정 토큰이 아니다: {name} → {other:?}"),
            }
        } else if let Some(a) = after.strip_prefix("Scoped") {
            let name = ident(after_marker(a, "make: Permission::"));
            let placeholder = string_after(a, "placeholder:");
            match tokens.get(&name) {
                Some(f @ TokenForm::Prefixed(_)) => f.doc_form(&placeholder),
                other => panic!("Scoped 게이트가 scoped 토큰이 아니다: {name} → {other:?}"),
            }
        } else {
            panic!("모르는 GateToken 형태: {:.60}", after);
        };
        out.push((key, doc_form));
        rest = after;
    }
    assert!(
        !out.is_empty(),
        "`contributes_gates!` 를 읽었는데 항목이 한 건도 안 나왔다 — 표가 빈 것이 아니라 \
         판독기가 형태를 못 맞춘 것이다."
    );
    out
}

/// `{` 부터 짝이 맞는 `}` 까지의 **안쪽** 을 돌려준다.
fn brace_block(src: &str) -> &str {
    let open = src.find('{').expect("여는 중괄호가 없다");
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open + 1..open + i];
                }
            }
            _ => {}
        }
    }
    panic!("중괄호가 안 닫혔다");
}

/// 앞에서부터 이어지는 식별자 문자만 취한다.
fn ident(src: &str) -> String {
    src.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// `marker` 뒤의 남은 문자열. 없으면 panic — 형태가 바뀐 것이다.
fn after_marker<'a>(src: &'a str, marker: &str) -> &'a str {
    let at = src
        .find(marker)
        .unwrap_or_else(|| panic!("`{marker}` 를 못 찾았다: {:.60}", src));
    &src[at + marker.len()..]
}

/// `marker` 뒤에 처음 나오는 문자열 리터럴의 내용.
fn string_after(src: &str, marker: &str) -> String {
    let rest = after_marker(src, marker);
    let open = rest
        .find('"')
        .unwrap_or_else(|| panic!("`{marker}` 뒤에 문자열이 없다"));
    let tail = &rest[open + 1..];
    let close = tail.find('"').expect("문자열이 안 닫혔다");
    tail[..close].to_string()
}

/// `src` 안에서 **마지막** 문자열 리터럴의 내용.
fn last_string_literal(src: &str) -> Option<String> {
    let mut spans: Vec<String> = Vec::new();
    let mut rest = src;
    while let Some(open) = rest.find('"') {
        let tail = &rest[open + 1..];
        let close = tail.find('"')?;
        spans.push(tail[..close].to_string());
        rest = &tail[close + 1..];
    }
    spans.pop()
}
