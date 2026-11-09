//! 세션 목록의 **남은 일** — 결정 대기 · 미커밋 · 마지막 활동.
//!
//! 여러 탭을 돌리면 어느 탭에 사람이 정할 것이 남았고 어느 폴더에 커밋 안 한 변경이 있는지를
//! 탭마다 열어 봐야 알았다. 목록 한 줄에 모은다.
//!
//! | 칸 | 출처 |
//! |---|---|
//! | 결정 대기 | 그 폴더 `spec/resume.md` 의 `열린 질문` 절 항목 수 + 세션이 물음에 답을 기다리는가 |
//! | 미커밋 | `git status --short` 줄 수 — git 이 아니면 모른다(`None`) |
//! | 마지막 활동 | 세션이 마지막으로 이벤트를 낸 뒤 지난 초 |
//!
//! 폴더 값은 [`TTL`] 동안 캐시한다 — 목록을 다시 그릴 때마다 git 을 돌리면 탭 수만큼 프로세스가 뜬다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 폴더 값 캐시 수명 `[초기값]`.
pub const TTL: Duration = Duration::from_secs(30);

/// 한 세션의 남은 일. 모르는 칸은 `None` — 0 과 가른다(0 은 "없다", `None` 은 "못 봤다").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Remaining {
    /// `spec/resume.md` 의 열린 질문 항목 수.
    pub questions: Option<usize>,
    /// 세션이 물음(권한·질문)에 답을 기다린다.
    pub asking: bool,
    /// 커밋 안 한 파일 수.
    pub dirty: Option<usize>,
    /// 마지막 이벤트 뒤 지난 초.
    pub idle_secs: Option<u64>,
}

/// 세션 행 하나의 남은 일 — 폴더 칸은 캐시를 거친다.
pub fn of_row(cwd: Option<&str>, raw: &serde_json::Value) -> Remaining {
    let (questions, dirty) = cwd.map_or((None, None), |c| folder(Path::new(c)));
    Remaining {
        questions,
        asking: raw.get("needs_input").is_some_and(|n| !n.is_null()),
        dirty,
        idle_secs: raw.get("idle_secs").and_then(serde_json::Value::as_u64),
    }
}

/// 폴더별 `(잰 시각, 열린 질문 수, 미커밋 파일 수)`.
type FolderCache = HashMap<PathBuf, (Instant, Option<usize>, Option<usize>)>;

fn folder(cwd: &Path) -> (Option<usize>, Option<usize>) {
    static CACHE: Mutex<Option<FolderCache>> = Mutex::new(None);
    let mut g = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let map = g.get_or_insert_with(HashMap::new);
    if let Some((at, q, d)) = map.get(cwd)
        && at.elapsed() < TTL
    {
        return (*q, *d);
    }
    let q = std::fs::read_to_string(cwd.join("spec/resume.md"))
        .ok()
        .map(|t| open_questions(&t));
    let d = dirty_files(cwd);
    map.insert(cwd.to_path_buf(), (Instant::now(), q, d));
    (q, d)
}

/// 열린 질문 절의 목록 항목 수. 절이 없으면 0.
///
/// 제목은 지금 로케일의 `sup.form.rem_heading` 또는 영어 `Open questions` 다 — 재개 문서는 사용자의 언어로
/// 쓰이므로 UI 로케일과 다른 언어로 쓴 문서의 절은 못 찾는다.
///
/// 절은 같은 깊이 이상의 다음 제목까지다. 항목은 `- ` · `* ` · `1. ` 으로 시작하는 줄이다 —
/// 들여쓴 하위 항목은 세지 않는다(질문 하나의 부연이다).
pub fn open_questions(text: &str) -> usize {
    let local = crate::domain::i18n::t("sup.form.rem_heading").to_lowercase();
    let mut depth = None;
    let mut n = 0;
    for line in text.lines() {
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && line[hashes..].starts_with(' ') {
            if let Some(d) = depth
                && hashes <= d
            {
                break;
            }
            let title = line[hashes..].trim().to_lowercase();
            if depth.is_none() && (title.contains(&local) || title.contains("open questions")) {
                depth = Some(hashes);
            }
            continue;
        }
        if depth.is_some() && is_item(line) {
            n += 1;
        }
    }
    n
}

fn is_item(line: &str) -> bool {
    if line.starts_with("- ") || line.starts_with("* ") {
        return true;
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(". ")
}

/// `git status --short` 줄 수 — git 이 없거나 저장소가 아니면 `None`.
fn dirty_files(cwd: &Path) -> Option<usize> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["status", "--short"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).lines().count())
}

/// 목록 칸의 글 — 아는 것만 ` · ` 로 잇는다. 아무것도 모르면 `-`.
pub fn label(r: &Remaining) -> String {
    let mut parts = Vec::new();
    if let Some(q) = r.questions.filter(|q| *q > 0) {
        parts.push(crate::domain::i18n::t1(
            "sup.form.rem_questions",
            &q.to_string(),
        ));
    }
    if r.asking {
        parts.push(crate::domain::i18n::t("sup.form.rem_asking"));
    }
    if let Some(d) = r.dirty.filter(|d| *d > 0) {
        parts.push(crate::domain::i18n::t1(
            "sup.form.rem_dirty",
            &d.to_string(),
        ));
    }
    if let Some(s) = r.idle_secs {
        parts.push(match s {
            0..60 => crate::domain::i18n::t("sup.form.rem_idle_now"),
            60..3600 => crate::domain::i18n::t1("sup.form.rem_idle_min", &(s / 60).to_string()),
            _ => crate::domain::i18n::t1("sup.form.rem_idle_hour", &(s / 3600).to_string()),
        });
    }
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_questions_counts_items_of_that_section_only() {
        let doc = "# 재개\n## 다음 할 일\n- a\n## 열린 질문\n- q1\n  - 부연\n- q2\n1. q3\n### 하위\n- q4\n## 다음 절\n- x\n";
        assert_eq!(
            open_questions(doc),
            4,
            "하위 절은 포함, 다음 같은 깊이 절에서 멈춘다"
        );
        assert_eq!(open_questions("## 다음 할 일\n- a\n"), 0, "절이 없으면 0");
        assert_eq!(open_questions("## Open questions\n* one\n"), 1);
    }

    #[test]
    fn the_label_shows_only_what_is_known() {
        assert_eq!(label(&Remaining::default()), "-");
        let r = Remaining {
            questions: Some(2),
            asking: true,
            dirty: Some(5),
            idle_secs: Some(180),
        };
        let l = label(&r);
        for want in ["2", "5", "3"] {
            assert!(l.contains(want), "{want} 가 없다: {l}");
        }
        assert_eq!(l.matches(" · ").count(), 3, "{l}");
        // 0 은 "없다" — 칸을 내지 않는다.
        let quiet = Remaining {
            questions: Some(0),
            dirty: Some(0),
            ..Default::default()
        };
        assert_eq!(label(&quiet), "-");
    }

    /// git 저장소가 아닌 폴더는 미커밋을 모른다 — 0 이라고 하지 않는다.
    #[test]
    fn a_folder_outside_git_has_no_dirty_count() {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tasty-as-rem-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("spec")).expect("폴더");
        std::fs::write(dir.join("spec/resume.md"), "## 열린 질문\n- q\n").expect("쓰기");
        let r = of_row(dir.to_str(), &serde_json::json!({ "idle_secs": 5 }));
        assert_eq!(r.questions, Some(1));
        assert_eq!(r.dirty, None);
        assert_eq!(r.idle_secs, Some(5));
        assert!(!r.asking);
        std::fs::remove_dir_all(&dir).expect("정리");
    }
}
