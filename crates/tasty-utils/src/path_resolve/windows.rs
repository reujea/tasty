//! Windows 경로 규칙. 문자열만 다루므로 다른 OS의 시험에서도 같은 결과를 확인할 수 있다.
//!
//! - 드라이브+루트(`C:\a`, `C:/a`), UNC(`\\srv\share`), verbatim(`\\?\…`)은 절대 경로다.
//!   verbatim은 벗긴 형태로 돌려준다.
//! - MSYS 드라이브 형식 `/c`, `/c/a`는 `C:\`, `C:\a`로 바꾼다.
//! - 드라이브 없는 루트(`\a`, `/a`)와 드라이브 상대(`C:a`)는 거부한다. 앞은 MSYS 마운트를 다른 파일로
//!   잘못 풀 수 있고, 뒤는 드라이브별 현재 폴더라는 고정할 수 없는 상태에 기대기 때문이다.
//! - 구분자는 `\`로 통일하고 `.`만 지운다. `..`와 대소문자는 그대로 둔다.

use super::PathResolveError;

/// `raw`를 `base`(실행 폴더, 없으면 원인 문구) 기준 절대 경로 문자열로 푼다.
pub(super) fn resolve(raw: &str, base: Result<&str, &str>) -> Result<String, PathResolveError> {
    let path = match msys_drive(raw) {
        Some(converted) => converted,
        None => strip_verbatim(&raw.replace('/', "\\")),
    };
    if path.starts_with(r"\\") {
        return Ok(normalize(&path));
    }
    if has_drive(&path) {
        return if path[2..].starts_with('\\') {
            Ok(normalize(&path))
        } else {
            Err(PathResolveError::DriveRelative)
        };
    }
    if path.starts_with('\\') {
        return Err(PathResolveError::RootRelative);
    }
    match base {
        Ok(base) => Ok(normalize(&format!(
            "{}\\{path}",
            strip_verbatim(&base.replace('/', "\\"))
        ))),
        Err(why) => Err(PathResolveError::LaunchDirUnavailable(why.to_owned())),
    }
}

/// `/c`, `/c/…`, `/c\…`(한 글자 드라이브)를 `C:\…`로 바꾼다. 그 밖에는 `None`.
fn msys_drive(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'/' || !bytes[1].is_ascii_alphabetic() {
        return None;
    }
    let rest = &raw[2..];
    if !(rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\')) {
        return None;
    }
    let drive = (bytes[1] as char).to_ascii_uppercase();
    Some(format!(
        "{drive}:\\{}",
        rest.trim_start_matches(['/', '\\']).replace('/', "\\")
    ))
}

fn strip_verbatim(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_owned();
    }
    path.to_owned()
}

fn has_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// 절대 경로 문자열의 접두(드라이브 또는 `\\server\share`) 뒤 세그먼트에서 빈 칸과 `.`을 지운다.
fn normalize(path: &str) -> String {
    let (prefix, rest) = if let Some(unc) = path.strip_prefix(r"\\") {
        let mut parts = unc.splitn(3, '\\');
        let server = parts.next().unwrap_or_default();
        let share = parts.next();
        let rest = parts.next().unwrap_or_default();
        let prefix = match share {
            Some(share) => format!(r"\\{server}\{share}"),
            None => format!(r"\\{server}"),
        };
        (prefix, rest)
    } else {
        (path[..2].to_owned(), &path[2..])
    };
    let segments: Vec<&str> = rest
        .split('\\')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    format!("{prefix}\\{}", segments.join("\\"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: Result<&str, &str> = Ok(r"D:\launch\dir");

    fn ok(raw: &str) -> String {
        resolve(raw, BASE).unwrap_or_else(|e| panic!("{raw}: {e}"))
    }

    #[test]
    fn absolute_forms_keep_their_location() {
        assert_eq!(ok(r"C:\a"), r"C:\a");
        assert_eq!(ok(r"C:/a\b"), r"C:\a\b");
        assert_eq!(ok(r"C:\a\.\b\..\c"), r"C:\a\b\..\c");
        assert_eq!(ok(r"\\srv\share\a"), r"\\srv\share\a");
        assert_eq!(ok(r"\\?\C:\a"), r"C:\a");
        assert_eq!(ok(r"\\?\UNC\srv\share"), r"\\srv\share\");
        assert_eq!(ok(r"\\?\UNC\srv\share\a"), r"\\srv\share\a");
    }

    #[test]
    fn msys_drive_form_becomes_a_drive_path() {
        assert_eq!(ok("/c/a"), r"C:\a");
        assert_eq!(ok("/C"), r"C:\");
        assert_eq!(ok("/d/x/y"), r"D:\x\y");
    }

    #[test]
    fn rooted_and_drive_relative_inputs_are_rejected() {
        assert_eq!(resolve(r"\a", BASE), Err(PathResolveError::RootRelative));
        assert_eq!(
            resolve("/usr/bin/bash", BASE),
            Err(PathResolveError::RootRelative)
        );
        assert_eq!(resolve("C:a", BASE), Err(PathResolveError::DriveRelative));
        assert_eq!(resolve("C:", BASE), Err(PathResolveError::DriveRelative));
    }

    #[test]
    fn relative_inputs_join_the_base() {
        assert_eq!(ok("a.md"), r"D:\launch\dir\a.md");
        assert_eq!(ok(r".\sh"), r"D:\launch\dir\sh");
        assert_eq!(ok("../y/x"), r"D:\launch\dir\..\y\x");
        assert_eq!(ok("."), r"D:\launch\dir");
        assert_eq!(ok(".."), r"D:\launch\dir\..");
    }

    #[test]
    fn a_relative_input_without_a_base_is_an_error() {
        assert_eq!(
            resolve("a", Err("gone")),
            Err(PathResolveError::LaunchDirUnavailable("gone".into()))
        );
        assert_eq!(resolve(r"C:\a", Err("gone")), Ok(r"C:\a".into()));
    }
}
