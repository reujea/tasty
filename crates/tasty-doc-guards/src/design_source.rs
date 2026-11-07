//! 앱이 따르는 디자인 정본(tasty-page 레포)의 체크아웃을 찾는다.
//!
//! 정본 커밋은 `crates/tasty-design-tokens/dtcg/SOURCE`에 한 줄로 기록한다.
//! 체크아웃 경로는 환경 변수 `TASTY_PAGE_DIR`로 받는다. 변수가 없거나 체크아웃의 HEAD가
//! 기록한 커밋과 다르면 panic한다. 조용히 건너뛰면 다른 판과 대조하거나 대조 없이 통과한다.
//! CI는 `.github/workflows/doc-guards.yml`이 기록한 커밋으로 tasty-page를 받아 이 변수를 넘긴다.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 디자인 정본 체크아웃 경로를 넘기는 환경 변수.
pub const ENV: &str = "TASTY_PAGE_DIR";

/// 앱이 따르는 tasty-page 커밋을 기록한 파일(저장소 루트 기준).
pub const SOURCE_FILE: &str = "crates/tasty-design-tokens/dtcg/SOURCE";

/// `SOURCE` 본문에서 커밋 해시를 읽는다. 40자 소문자 16진수 한 줄만 허용한다.
pub fn parse_source(text: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let [hash] = lines.as_slice() else {
        return Err(format!(
            "{SOURCE_FILE}에는 커밋 해시 한 줄만 있어야 한다. 줄 {}개를 읽었다.",
            lines.len()
        ));
    };
    if hash.len() != 40 || !hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        return Err(format!(
            "{SOURCE_FILE}의 값 {hash:?}가 40자 소문자 16진수 커밋 해시가 아니다."
        ));
    }
    Ok((*hash).to_string())
}

/// 체크아웃의 HEAD가 기록한 커밋과 같은지 판정한다.
pub fn check_head(recorded: &str, head: &str) -> Result<(), String> {
    if recorded == head.trim() {
        Ok(())
    } else {
        Err(format!(
            "{ENV}의 HEAD {}가 {SOURCE_FILE}의 {recorded}와 다르다. 기록한 커밋으로 체크아웃한 tasty-page를 넘긴다.",
            head.trim()
        ))
    }
}

/// 기록한 tasty-page 커밋.
pub fn recorded_commit() -> String {
    let path = crate::repo_root().join(SOURCE_FILE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 를 못 읽었다: {e}", path.display()));
    parse_source(&text).unwrap_or_else(|why| panic!("{why}"))
}

fn head_of(dir: &Path) -> String {
    let out = crate::git_env::command("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "git rev-parse HEAD를 실행하지 못했다({}): {e}",
                dir.display()
            )
        });
    assert!(
        out.status.success(),
        "{}에서 git rev-parse HEAD가 실패했다: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// 기록한 커밋으로 체크아웃한 tasty-page의 루트. 한 번만 확인한다.
pub fn root() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let Some(dir) = std::env::var_os(ENV) else {
            panic!(
                "{ENV}가 없다. {SOURCE_FILE}의 커밋으로 체크아웃한 tasty-page 경로를 넘긴다. \
                 디자인 정본과 앱을 대조하는 시험은 건너뛰지 않는다. 받는 명령: \
                 export {ENV}=\"$(bash scripts/fetch-design-source.sh)\""
            );
        };
        let dir = PathBuf::from(dir);
        assert!(
            dir.join("vendor").is_dir(),
            "{ENV}={}에 vendor/가 없다. tasty-page 체크아웃을 넘긴다.",
            dir.display()
        );
        check_head(&recorded_commit(), &head_of(&dir)).unwrap_or_else(|why| panic!("{why}"));
        dir
    })
    .clone()
}

/// tasty-page 루트 기준 경로의 파일을 읽는다.
pub fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("tasty-page의 {rel}를 못 읽었다({}): {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "2ce2d0b74938dde8032ba377bd32617dccac022a";

    #[test]
    fn the_source_file_takes_exactly_one_full_hash() {
        assert_eq!(parse_source(&format!("{H}\n")).as_deref(), Ok(H));
        assert!(parse_source("").is_err(), "빈 파일을 받아들였다");
        assert!(parse_source("2ce2d0b\n").is_err(), "짧은 해시를 받아들였다");
        assert!(
            parse_source(&format!("{H}\n{H}\n")).is_err(),
            "두 줄을 받아들였다"
        );
        assert!(
            parse_source(&H.to_ascii_uppercase()).is_err(),
            "대문자를 받아들였다"
        );
    }

    #[test]
    fn a_checkout_at_another_commit_is_refused() {
        assert!(check_head(H, &format!("{H}\n")).is_ok());
        let other = "013ed71000000000000000000000000000000000";
        assert!(check_head(H, other).is_err(), "다른 커밋을 받아들였다");
    }
}
