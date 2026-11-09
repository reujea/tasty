//! Claude Code 대화 기록(`<session_id>.jsonl`)의 자리를 찾는다.
//! 외부 도구의 프로젝트 디렉터리 이름 규칙을 추측하지 않고 루트와 바로 아래 디렉터리를 탐색한다.

use std::path::{Path, PathBuf};

/// transcript 루트 디렉토리. `CLAUDE_CONFIG_DIR` 이 설정돼 있으면 그 아래 `projects`,
/// 아니면 홈의 `.claude/projects`.
pub fn transcript_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("projects"));
    }
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    Some(home.join(".claude").join("projects"))
}

/// 루트와 바로 아래 프로젝트 디렉터리에서 처음 발견한 <session_id>.jsonl을 반환한다.
pub fn find_transcript(root: &Path, session_id: &str) -> Option<PathBuf> {
    let file_name = format!("{session_id}.jsonl");
    let direct = root.join(&file_name);
    if direct.is_file() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let candidate = entry.path().join(&file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_transcript_by_scanning_project_dirs_without_computing_the_slug() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // slug 규칙을 흉내 낼 수 없는 이름들 — 탐색 방식이라 상관없다.
        std::fs::create_dir_all(root.join("-home-user-a")).expect("mkdir");
        std::fs::create_dir_all(root.join("-home-user-b-worktree-wt-1")).expect("mkdir");
        let wanted = root.join("-home-user-b-worktree-wt-1").join("sess-1.jsonl");
        std::fs::write(&wanted, b"{}\n").expect("write");

        assert_eq!(find_transcript(root, "sess-1"), Some(wanted));
    }

    #[test]
    fn finds_a_transcript_placed_directly_under_the_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wanted = dir.path().join("sess-2.jsonl");
        std::fs::write(&wanted, b"{}\n").expect("write");
        assert_eq!(find_transcript(dir.path(), "sess-2"), Some(wanted));
    }

    #[test]
    fn returns_none_when_the_session_file_does_not_exist_yet() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("-proj")).expect("mkdir");
        assert_eq!(find_transcript(dir.path(), "not-written-yet"), None);
    }

    #[test]
    fn a_directory_named_like_the_transcript_is_not_accepted() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("-proj").join("sess-3.jsonl")).expect("mkdir");
        assert_eq!(find_transcript(dir.path(), "sess-3"), None);
    }
}
