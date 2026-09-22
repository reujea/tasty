//! Memory scope. SQLite의 `memory.scope` 컬럼에 들어가는 문자열 표현 정의 +
//! 파서.
//!
//! 형식: `global` | `account:<userid>` | `window:<id>` | `workspace:<id>` |
//! `surface:<id>`. 숫자 ID는 u32 또는 u64. account는 OS userid 문자열.

use std::fmt;

/// 스코프 ID. window 외에는 u32 (tasty-core의 WorkspaceId/SurfaceId와 일치).
/// window는 winit::WindowId(불투명) → u64 ABI에 매칭.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    Global,
    Account(String),
    Window(u64),
    Workspace(u32),
    Surface(u32),
}

impl Scope {
    /// 직렬화된 표현 (DB 저장용).
    pub fn as_token(&self) -> String {
        match self {
            Scope::Global => "global".to_string(),
            Scope::Account(id) => format!("account:{id}"),
            Scope::Window(id) => format!("window:{id}"),
            Scope::Workspace(id) => format!("workspace:{id}"),
            Scope::Surface(id) => format!("surface:{id}"),
        }
    }

    /// 문자열로부터 파싱. invalid면 `Err(원본)` — caller가 `MemoryError::InvalidScope`로 wrap.
    pub fn parse(s: &str) -> Result<Self, String> {
        if s == "global" {
            return Ok(Scope::Global);
        }
        let (kind, value) = s.split_once(':').ok_or_else(|| s.to_string())?;
        match kind {
            "account" => {
                if value.is_empty() {
                    return Err(s.to_string());
                }
                // account userid는 OS-derived 문자열. 길이 1..128로 제한해서
                // pathological 값 차단. 문자종류는 OS마다 다르므로 광범위 허용.
                if value.len() > 128 {
                    return Err(s.to_string());
                }
                Ok(Scope::Account(value.to_string()))
            }
            "window" => value
                .parse::<u64>()
                .map(Scope::Window)
                .map_err(|_| s.to_string()),
            "workspace" => value
                .parse::<u32>()
                .map(Scope::Workspace)
                .map_err(|_| s.to_string()),
            "surface" => value
                .parse::<u32>()
                .map(Scope::Surface)
                .map_err(|_| s.to_string()),
            _ => Err(s.to_string()),
        }
    }

    /// 카테고리 문자열 (`global`, `account`, `window`, ...). 통계/필터 용.
    pub fn kind(&self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Account(_) => "account",
            Scope::Window(_) => "window",
            Scope::Workspace(_) => "workspace",
            Scope::Surface(_) => "surface",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_token())
    }
}

/// 키 길이 상한(바이트). [`validate_key`] 가 이 값으로 자른다.
pub const MAX_KEY_LEN: usize = 256;

/// [`validate_key`] 가 허용하는 문자를 사람에게 설명하는 문구. 판정 규칙을
/// 바꾸면 이 문구도 같이 바꾼다 — 둘은 이 파일 안에서만 짝을 이룬다.
pub const KEY_ALLOWED_CHARS: &str = "lowercase a-z, 0-9, '.', '_', '-'";

/// 키 검증. 1..=[`MAX_KEY_LEN`]자, `[a-z0-9._-]+`만 허용.
pub fn validate_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("empty key".to_string());
    }
    if key.len() > MAX_KEY_LEN {
        return Err(format!("key too long: {} > {MAX_KEY_LEN}", key.len()));
    }
    for (i, c) in key.bytes().enumerate() {
        let ok =
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'_' || c == b'-';
        if !ok {
            return Err(format!("invalid char at {i}: {:?}", c as char));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_roundtrip() {
        let cases = [
            Scope::Global,
            Scope::Account("zilhak".into()),
            Scope::Window(42),
            Scope::Workspace(7),
            Scope::Surface(123),
        ];
        for s in cases {
            let token = s.as_token();
            let parsed = Scope::parse(&token).unwrap();
            assert_eq!(parsed, s);
        }
    }

    #[test]
    fn scope_parse_rejects_invalid() {
        for bad in [
            "",
            "garbage",
            "surface:",
            "surface:abc",
            "window:-1",
            "account:",
        ] {
            assert!(Scope::parse(bad).is_err(), "should reject: {bad}");
        }
    }

    #[test]
    fn scope_parse_rejects_long_account() {
        let long = format!("account:{}", "a".repeat(129));
        assert!(Scope::parse(&long).is_err());
    }

    #[test]
    fn key_validation_accepts_normal() {
        for k in [
            "a",
            "task.123.plan",
            "tasty.role",
            "plugin.codex.last-run",
            "x-y_z.0",
        ] {
            validate_key(k).expect(k);
        }
    }

    #[test]
    fn key_validation_rejects() {
        validate_key("").unwrap_err();
        validate_key(&"a".repeat(257)).unwrap_err();
        validate_key("UPPER").unwrap_err();
        validate_key("with space").unwrap_err();
        validate_key("emoji😀").unwrap_err();
        validate_key("slash/key").unwrap_err();
    }
}
