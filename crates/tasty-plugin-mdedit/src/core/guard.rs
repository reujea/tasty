//! 섹션 가드(A4)와 그 설정을 담는 사이드카 `.<파일명>.tasty.toml`(docs/surfaces/mdedit/index.md#섹션-가드는-사이드카-파일에-둔다).
//!
//! 판정은 "옛 텍스트 → 새 텍스트" 한 쌍만 본다. 사람의 저장과 에이전트의 `mdedit` 쓰기가
//! 같은 함수를 지난다.

use std::path::{Path, PathBuf};

use chrono::{Duration, NaiveDate};
use regex::Regex;
use serde::Deserialize;

use super::diff::{diff_lines, split_lines};
use super::leak::SecretsConfig;
use super::ops::task_char_in_line;
use super::outline::{Section, sections};

/// 문서 `<dir>/<name>` 의 사이드카 경로 `<dir>/.<name>.tasty.toml`.
pub fn sidecar_path(doc: &Path) -> Option<PathBuf> {
    let name = doc.file_name()?.to_str()?;
    Some(doc.with_file_name(format!(".{name}.tasty.toml")))
}

#[derive(Debug, Deserialize, Default)]
struct RawSidecar {
    #[serde(default)]
    guard: Vec<RawGuard>,
    #[serde(default)]
    secrets: Option<RawSecrets>,
}

#[derive(Debug, Deserialize)]
struct RawGuard {
    heading: String,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    allow_suffixes: Vec<String>,
    /// 헤딩 제목에서 날짜를 읽을 형식(chrono `strftime`). `older_than_days` 와 함께 쓴다.
    date_format: Option<String>,
    older_than_days: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct RawSecrets {
    mode: Option<String>,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    extra: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GuardRule {
    pub heading: Regex,
    pub allow_checkbox: bool,
    pub allow_suffixes: Vec<String>,
    /// 날짜 상대 조건 — 있으면 헤딩 제목의 날짜가 기준일보다 이 조건만큼 오래된 섹션만 가드한다.
    pub date: Option<DateCond>,
}

/// `date_format` 으로 헤딩 제목의 날짜를 읽고, 오늘에서 `older_than_days` 일을 뺀 날보다
/// **앞선** 날짜의 섹션만 가드 대상으로 본다. `older_than_days = 1` 이면 오늘·어제는 자유,
/// 그제부터 가드다. 날짜를 못 읽는 헤딩은 가드 대상이 아니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateCond {
    pub format: String,
    pub older_than_days: i64,
}

impl DateCond {
    fn applies(&self, title: &str, today: NaiveDate) -> bool {
        let Ok((date, _rest)) = NaiveDate::parse_and_remainder(title.trim(), &self.format) else {
            return false;
        };
        date < today - Duration::days(self.older_than_days)
    }
}

/// 사이드카 하나를 읽은 결과. 읽기·해석 실패는 저장을 막지 않는다 — `warnings` 로 알리고 그
/// 항목만 뺀다(설정 오류 때문에 문서를 못 고치는 상태를 만들지 않는다).
#[derive(Debug, Clone, Default)]
pub struct DocConfig {
    pub guards: Vec<GuardRule>,
    pub secrets: SecretsConfig,
    pub warnings: Vec<String>,
}

impl DocConfig {
    pub fn load(doc: &Path) -> Self {
        let Some(path) = sidecar_path(doc) else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => Self {
                warnings: vec![format!("{}: {e}", path.display())],
                ..Self::default()
            },
        }
    }

    pub fn parse(text: &str) -> Self {
        let raw: RawSidecar = match toml::from_str(text) {
            Ok(r) => r,
            Err(e) => {
                return Self {
                    warnings: vec![format!("sidecar parse error: {e}")],
                    ..Self::default()
                };
            }
        };
        let mut warnings = Vec::new();
        let mut guards = Vec::new();
        for g in raw.guard {
            match Regex::new(&g.heading) {
                Ok(re) => {
                    for a in &g.allow {
                        if a != "checkbox" {
                            warnings.push(format!("guard '{}': unknown allow '{a}'", g.heading));
                        }
                    }
                    let date = match (g.date_format, g.older_than_days) {
                        (None, None) => None,
                        (Some(format), Some(days)) if days >= 0 => Some(DateCond {
                            format,
                            older_than_days: days,
                        }),
                        _ => {
                            // 반쪽 설정은 의도를 모른다 — 그 규칙을 통째로 빼고 알린다(날짜를 무시하고
                            // 전부 가드하면 오늘 섹션까지 못 고치게 된다).
                            warnings.push(format!(
                                "guard '{}': date_format and older_than_days (>= 0) go together — rule skipped",
                                g.heading
                            ));
                            continue;
                        }
                    };
                    guards.push(GuardRule {
                        heading: re,
                        allow_checkbox: g.allow.iter().any(|a| a == "checkbox"),
                        allow_suffixes: g.allow_suffixes,
                        date,
                    });
                }
                Err(e) => warnings.push(format!("guard heading regex '{}': {e}", g.heading)),
            }
        }
        let secrets = match raw.secrets {
            Some(s) => {
                SecretsConfig::from_parts(s.mode.as_deref(), &s.allow, &s.extra, &mut warnings)
            }
            None => SecretsConfig::default(),
        };
        Self {
            guards,
            secrets,
            warnings,
        }
    }

    fn rule_for(&self, sec: &Section, today: NaiveDate) -> Option<&GuardRule> {
        self.guards.iter().find(|g| {
            g.heading.is_match(&sec.heading.text)
                && g.date
                    .as_ref()
                    .is_none_or(|d| d.applies(&sec.heading.title, today))
        })
    }
}

/// 가드 위반 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// 가드에 걸린 헤딩 줄.
    pub heading: String,
    /// 옛 텍스트 기준 1 기준 줄 번호(헤딩이 사라진 경우 헤딩 줄).
    pub line: usize,
    pub reason: ViolationReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationReason {
    /// 가드 섹션의 헤딩이 사라졌거나 바뀌었다.
    HeadingRemoved,
    /// 허용되지 않은 내용 변경.
    Modified,
}

/// 한 줄 변경이 허용 목록에 드는가 — 체크박스 문자만 바뀌었거나, 허용된 꼬리만 붙었다.
fn line_change_allowed(old: &str, new: &str, rule: &GuardRule) -> bool {
    if rule.allow_checkbox
        && old.len() == new.len()
        && let (Some((oi, _)), Some((ni, _))) = (task_char_in_line(old), task_char_in_line(new))
        && oi == ni
        && old[..oi] == new[..ni]
        && old[oi + 1..] == new[ni + 1..]
    {
        return true;
    }
    let (o, n) = (
        old.trim_end_matches(['\n', '\r']),
        new.trim_end_matches(['\n', '\r']),
    );
    rule.allow_suffixes
        .iter()
        .any(|s| !s.is_empty() && n.strip_suffix(s.as_str()) == Some(o))
}

/// `old` → `new` 가 가드를 어기는 자리 전부. `restore_from` 이 있으면, 새 섹션이 그 텍스트의
/// 같은 섹션과 똑같은 경우(검토에서 외부 변경을 되돌린 결과)는 허용한다. 날짜 조건의 기준일은
/// 로컬 날짜다.
pub fn check(old: &str, new: &str, cfg: &DocConfig, restore_from: Option<&str>) -> Vec<Violation> {
    check_at(
        old,
        new,
        cfg,
        restore_from,
        chrono::Local::now().date_naive(),
    )
}

/// [`check`] 의 기준일을 주입하는 형태 — 날짜 경계 시험용이자 판정의 본체.
pub fn check_at(
    old: &str,
    new: &str,
    cfg: &DocConfig,
    restore_from: Option<&str>,
    today: NaiveDate,
) -> Vec<Violation> {
    if cfg.guards.is_empty() || old == new {
        return Vec::new();
    }
    let new_secs = sections(new);
    let restore_secs = restore_from.map(sections).unwrap_or_default();
    let mut out = Vec::new();
    for sec in sections(old) {
        let Some(rule) = cfg.rule_for(&sec, today) else {
            continue;
        };
        let Some(ns) = new_secs.iter().find(|s| s.heading.text == sec.heading.text) else {
            out.push(Violation {
                heading: sec.heading.text.clone(),
                line: sec.heading.line + 1,
                reason: ViolationReason::HeadingRemoved,
            });
            continue;
        };
        let (os_text, ns_text) = (&old[sec.whole.clone()], &new[ns.whole.clone()]);
        if os_text == ns_text {
            continue;
        }
        if let Some(r) = restore_from
            && restore_secs
                .iter()
                .find(|s| s.heading.text == sec.heading.text)
                .is_some_and(|s| &r[s.whole.clone()] == ns_text)
        {
            continue;
        }
        let (ol, nl) = (split_lines(os_text), split_lines(ns_text));
        for h in diff_lines(&ol, &nl) {
            let ok = h.a.len() == h.b.len()
                && ol[h.a.clone()]
                    .iter()
                    .zip(&nl[h.b.clone()])
                    .all(|(o, n)| line_change_allowed(o, n, rule));
            if !ok {
                out.push(Violation {
                    heading: sec.heading.text.clone(),
                    line: sec.heading.line + 1 + h.a.start,
                    reason: ViolationReason::Modified,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: &str = r#"
[[guard]]
heading = '^# 참조$'
allow = ["checkbox"]

[[guard]]
heading = '^# 2026\.09\.'
allow = ["checkbox"]
allow_suffixes = [" → todo"]
"#;
    const DOC: &str =
        "# 참조\n- [ ] a\n- web\n\n# 2026.10.22\n- x\n\n# 2026.09.30\n- [ ] old\n- note\n";

    #[test]
    fn sidecar_name_keeps_extension() {
        assert_eq!(
            sidecar_path(Path::new("/d/일지.md")).unwrap(),
            PathBuf::from("/d/.일지.md.tasty.toml")
        );
    }

    #[test]
    fn guarded_section_rejects_edits_but_allows_checkbox_and_suffix() {
        let cfg = DocConfig::parse(SIDE);
        assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
        let edit = DOC.replace("- web", "- web2");
        let v = check(DOC, &edit, &cfg, None);
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].line, v[0].reason), (3, ViolationReason::Modified));
        assert!(check(DOC, &DOC.replace("- [ ] a", "- [x] a"), &cfg, None).is_empty());
        assert!(check(DOC, &DOC.replace("- note", "- note → todo"), &cfg, None).is_empty());
        // 꼬리는 그 가드에만 허용된다.
        assert_eq!(
            check(DOC, &DOC.replace("- web", "- web → todo"), &cfg, None).len(),
            1
        );
        // 가드 밖은 자유.
        assert!(check(DOC, &DOC.replace("- x", "- y"), &cfg, None).is_empty());
    }

    #[test]
    fn removing_guarded_heading_or_adding_lines_is_rejected() {
        let cfg = DocConfig::parse(SIDE);
        let v = check(DOC, &DOC.replace("# 참조\n", "# 참조2\n"), &cfg, None);
        assert_eq!(v[0].reason, ViolationReason::HeadingRemoved);
        assert_eq!(
            check(DOC, &DOC.replace("- web\n", "- web\n- more\n"), &cfg, None).len(),
            1
        );
    }

    #[test]
    fn restoring_a_guarded_section_is_allowed() {
        let cfg = DocConfig::parse(SIDE);
        let tampered = DOC.replace("- web", "- hacked");
        assert_eq!(check(&tampered, DOC, &cfg, None).len(), 1);
        assert!(check(&tampered, DOC, &cfg, Some(DOC)).is_empty());
    }

    #[test]
    fn broken_sidecar_warns_and_guards_nothing() {
        let cfg = DocConfig::parse("[[guard]]\nheading = '('\n");
        assert!(cfg.guards.is_empty());
        assert_eq!(cfg.warnings.len(), 1);
        let cfg = DocConfig::parse("not toml ===");
        assert_eq!(cfg.warnings.len(), 1);
    }

    /// 날짜 상대 가드 — 시계를 주입해 오늘·어제·그제 경계를 잰다. 합성 fixture 만 쓴다.
    const DATED_SIDE: &str = r#"
[[guard]]
heading = '^# \d{4}\.\d{2}\.\d{2}'
date_format = '%Y.%m.%d'
older_than_days = 1
allow = ["checkbox"]
allow_suffixes = [" → todo"]
"#;
    const DATED: &str = "# 메모\n- free\n\n# 2030.01.10\n- today\n\n# 2030.01.09\n- yesterday\n\n# 2030.01.08 회의\n- [ ] old\n- note\n";

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2030, 1, 10).unwrap()
    }

    #[test]
    fn dated_guard_frees_today_and_yesterday_only() {
        let cfg = DocConfig::parse(DATED_SIDE);
        assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
        let edit = |a: &str, b: &str| check_at(DATED, &DATED.replace(a, b), &cfg, None, today());
        assert!(
            edit("- today", "- today!").is_empty(),
            "today must be editable"
        );
        assert!(
            edit("- yesterday", "- yesterday!").is_empty(),
            "yesterday must be editable"
        );
        let v = edit("- note", "- note!");
        assert_eq!(v.len(), 1, "the day before yesterday is guarded");
        assert_eq!(v[0].heading, "# 2030.01.08 회의");
        assert!(
            edit("- [ ] old", "- [x] old").is_empty(),
            "checkbox stays allowed"
        );
        assert!(
            edit("- note", "- note → todo").is_empty(),
            "suffix stays allowed"
        );
        assert!(
            edit("- free", "- anything").is_empty(),
            "undated heading is not guarded"
        );
        // 같은 문서도 하루가 지나면 어제였던 섹션이 가드된다.
        let next_day = NaiveDate::from_ymd_opt(2030, 1, 11).unwrap();
        let v = check_at(
            DATED,
            &DATED.replace("- yesterday", "- y!"),
            &cfg,
            None,
            next_day,
        );
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn dated_guard_zero_days_and_half_config() {
        let cfg =
            DocConfig::parse(&DATED_SIDE.replace("older_than_days = 1", "older_than_days = 0"));
        assert_eq!(
            check_at(
                DATED,
                &DATED.replace("- yesterday", "- y!"),
                &cfg,
                None,
                today()
            )
            .len(),
            1,
            "with 0 days, yesterday is already guarded"
        );
        assert!(
            check_at(
                DATED,
                &DATED.replace("- today", "- t!"),
                &cfg,
                None,
                today()
            )
            .is_empty()
        );
        let half = DocConfig::parse(&DATED_SIDE.replace("older_than_days = 1\n", ""));
        assert!(half.guards.is_empty());
        assert_eq!(half.warnings.len(), 1);
    }
}
