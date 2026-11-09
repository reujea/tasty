//! 비밀값 검사(A5). 저장·`mdedit` 쓰기 때 **새로 생기거나 바뀐 줄만** 본다 — 이미 있던 줄로
//! 저장을 막으면 그 문서는 다시는 저장되지 않는다.
//!
//! 보고에는 값을 싣지 않는다(종류와 줄 번호만). 라벨(`[pw:P1]`)은 허용 패턴으로 먼저 지운 뒤
//! 검사한다.

use std::sync::OnceLock;

use regex::Regex;

use super::diff::{diff_lines, split_lines};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecretsMode {
    #[default]
    Block,
    Warn,
    Off,
}

#[derive(Debug, Clone)]
pub struct SecretsConfig {
    pub mode: SecretsMode,
    allow: Vec<Regex>,
    extra: Vec<Regex>,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            mode: SecretsMode::Block,
            allow: Vec::new(),
            extra: Vec::new(),
        }
    }
}

impl SecretsConfig {
    pub fn from_parts(
        mode: Option<&str>,
        allow: &[String],
        extra: &[String],
        warnings: &mut Vec<String>,
    ) -> Self {
        let mode = match mode {
            None | Some("block") => SecretsMode::Block,
            Some("warn") => SecretsMode::Warn,
            Some("off") => SecretsMode::Off,
            Some(other) => {
                warnings.push(format!("secrets.mode '{other}' — using block"));
                SecretsMode::Block
            }
        };
        let mut compile = |list: &[String], what: &str| -> Vec<Regex> {
            list.iter()
                .filter_map(|p| match Regex::new(p) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        warnings.push(format!("secrets.{what} regex '{p}': {e}"));
                        None
                    }
                })
                .collect()
        };
        Self {
            mode,
            allow: compile(allow, "allow"),
            extra: compile(extra, "extra"),
        }
    }
}

/// 비밀값으로 보이는 자리 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 새 텍스트 기준 1 기준 줄 번호.
    pub line: usize,
    /// 패턴 종류(`password_assignment` 등).
    pub kind: &'static str,
}

/// 기본 라벨 형태 — `[pw:P1]`·`[pw:T3]` 같은 `[소문자:대문자숫자]`.
fn label_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[a-z]+:[A-Z]+\d+\]").expect("static regex"))
}

/// 영어 외 비밀값 낱말 — 데이터 파일에서 읽는다(탐지 어휘는 출력 문구가 아니다).
const SECRET_WORDS: &str = include_str!("../../assets/secret_words.txt");

fn local_words() -> String {
    SECRET_WORDS
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join("|")
}

struct Builtin {
    kind: &'static str,
    re: Regex,
    /// 캡처 1 이 있으면 그 값의 최소 길이.
    min_value: usize,
}

fn builtins() -> &'static [Builtin] {
    static B: OnceLock<Vec<Builtin>> = OnceLock::new();
    B.get_or_init(|| {
        let mk = |kind, pat: &str, min_value| Builtin {
            kind,
            re: Regex::new(pat).expect("static regex"),
            min_value,
        };
        vec![
            mk(
                "password_assignment",
                r"(?i)\b(?:password|passwd|passphrase|pwd|pw|secret|token|api[_-]?key|access[_-]?key|client[_-]?secret)\s*[:=]\s*([^\s,;)\]]+)",
                4,
            ),
            mk(
                "password_assignment",
                &format!(r"(?:{})\s*[:=]\s*([^\s,;)\]]+)", local_words()),
                4,
            ),
            mk(
                "account_password",
                r"(?:^|[\s(])(?:root|admin|administrator|ubuntu|centos|ec2-user|deploy\w*|postgres|mysql|sa)\s*/\s*([^\s/\[\](),]{6,})",
                6,
            ),
            mk("aws_access_key", r"\bAKIA[0-9A-Z]{16}\b", 0),
            mk(
                "github_token",
                r"\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{50,})",
                0,
            ),
            mk("gitlab_token", r"\bglpat-[A-Za-z0-9_-]{20,}", 0),
            mk("slack_token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}", 0),
            mk("api_secret_key", r"\bsk-[A-Za-z0-9_-]{20,}", 0),
            mk("private_key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----", 0),
            mk(
                "jwt",
                r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
                0,
            ),
        ]
    })
}

fn mask(line: &str, cfg: &SecretsConfig) -> String {
    let mut s = label_re().replace_all(line, " ").into_owned();
    for re in &cfg.allow {
        s = re.replace_all(&s, " ").into_owned();
    }
    s
}

/// 계정/비밀번호 꼴은 값에 숫자와 숫자 아닌 것이 섞였을 때만 센다 — `admin/dashboard` 같은
/// 경로 조각을 비밀로 보지 않기 위해서다.
fn value_looks_secret(kind: &str, v: &str) -> bool {
    if kind != "account_password" {
        return true;
    }
    v.chars().any(|c| c.is_ascii_digit()) && v.chars().any(|c| !c.is_ascii_digit())
}

/// 줄 하나의 발견 종류(없으면 `None`).
pub fn scan_line(line: &str, cfg: &SecretsConfig) -> Option<&'static str> {
    if cfg.mode == SecretsMode::Off {
        return None;
    }
    let masked = mask(line, cfg);
    for b in builtins() {
        for caps in b.re.captures_iter(&masked) {
            match caps.get(1) {
                Some(v) if v.as_str().chars().count() < b.min_value => {}
                Some(v) if !value_looks_secret(b.kind, v.as_str()) => {}
                _ => return Some(b.kind),
            }
        }
    }
    cfg.extra
        .iter()
        .any(|re| re.is_match(&masked))
        .then_some("custom_pattern")
}

/// `old` → `new` 에서 새로 생기거나 바뀐 줄의 발견 전부.
pub fn scan_added(old: &str, new: &str, cfg: &SecretsConfig) -> Vec<Finding> {
    if cfg.mode == SecretsMode::Off || old == new {
        return Vec::new();
    }
    let (ol, nl) = (split_lines(old), split_lines(new));
    let mut out = Vec::new();
    for h in diff_lines(&ol, &nl) {
        for i in h.b.clone() {
            if let Some(kind) = scan_line(nl[i], cfg) {
                out.push(Finding { line: i + 1, kind });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SecretsConfig {
        SecretsConfig::default()
    }

    #[test]
    fn labels_pass() {
        for l in [
            "- ssh: 22 (root/[pw:P1])",
            "- example-user / [pw:P5]",
            "- web: http://10.0.0.1:8080/login > admin/[pw:P1]",
            "- 개발서버 비번 바뀜 [pw:P11]",
            "- 비밀번호: [pw:P3]",
            "- 관리 화면: https:443 → /admin/v1/",
            "- token 만료 확인",
            "- admin/dashboard 화면",
        ] {
            assert_eq!(scan_line(l, &cfg()), None, "{l}");
        }
    }

    #[test]
    fn plaintext_is_caught() {
        for (l, k) in [
            ("- ssh: 22 (root/Qwer1234!)", "account_password"),
            ("- 비밀번호: hunter22", "password_assignment"),
            ("password=abcd1234", "password_assignment"),
            ("key AKIAABCDEFGHIJKLMNOP", "aws_access_key"),
            ("glpat-abcdefghijklmnopqrstu", "gitlab_token"),
            ("-----BEGIN OPENSSH PRIVATE KEY-----", "private_key"),
        ] {
            assert_eq!(scan_line(l, &cfg()), Some(k), "{l}");
        }
    }

    #[test]
    fn only_changed_lines_are_scanned_and_off_mode_skips() {
        let old = "- pw: legacy1234\n- a\n";
        let new = "- pw: legacy1234\n- b\n- 비번: newpass99\n";
        let f = scan_added(old, new, &cfg());
        assert_eq!(
            f,
            vec![Finding {
                line: 3,
                kind: "password_assignment"
            }]
        );
        let mut w = Vec::new();
        let off = SecretsConfig::from_parts(Some("off"), &[], &[], &mut w);
        assert!(scan_added(old, new, &off).is_empty());
    }

    #[test]
    fn allow_and_extra_patterns() {
        let mut w = Vec::new();
        let c = SecretsConfig::from_parts(
            Some("warn"),
            &[r"pw: demo\w+".into()],
            &[r"사내키-\d{4}".into()],
            &mut w,
        );
        assert!(w.is_empty());
        assert_eq!(c.mode, SecretsMode::Warn);
        assert_eq!(scan_line("pw: demo1234", &c), None);
        assert_eq!(scan_line("사내키-9876", &c), Some("custom_pattern"));
    }
}
