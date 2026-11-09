//! 문서 읽기·쓰기. 편집기는 원문을 **바이트 그대로** 돌려줘야 한다(E1) — 웹뷰의 `<textarea>`
//! 는 `\r\n` 을 `\n` 으로 바꾸고 BOM 을 보이지 않게 다루므로, 그 둘을 여기서 떼었다가 저장 때
//! 되붙인다.
//!
//! 저장은 같은 폴더의 임시 파일에 쓰고 rename 한다(E2: 도중에 죽어도 반쯤 쓰인 파일이 없다).
//! `tasty_utils::path::write_atomic` 과 같은 방식에 **원래 권한 보존**을 더했다 — 임시 파일은
//! 0600 으로 만들어지므로 그대로 rename 하면 0644 문서가 0600 으로 바뀐다.

use std::io::Write as _;
use std::path::Path;

use sha2::{Digest, Sha256};

/// 원문의 줄바꿈 형태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Lf,
    Crlf,
    /// 섞여 있다 — 편집기는 LF 로 다루고 저장하면 LF 로 통일된다(배너로 알린다).
    Mixed,
}

/// 편집기가 다루는 형태(LF, BOM 없음)와 되돌릴 정보.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub eol: Eol,
    pub bom: bool,
}

pub fn decode(raw: &str) -> Decoded {
    let (bom, body) = match raw.strip_prefix('\u{feff}') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    let lf = body.matches('\n').count();
    let crlf = body.matches("\r\n").count();
    let eol = if crlf == 0 {
        Eol::Lf
    } else if crlf == lf {
        Eol::Crlf
    } else {
        Eol::Mixed
    };
    let text = if eol == Eol::Lf {
        body.to_string()
    } else {
        body.replace("\r\n", "\n")
    };
    Decoded { text, eol, bom }
}

pub fn encode(text: &str, eol: Eol, bom: bool) -> String {
    let body = if eol == Eol::Crlf {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    if bom { format!("\u{feff}{body}") } else { body }
}

/// 내용 지문 — sha256 앞 16 hex. `--if-match` 와 화면 표시에 쓴다.
pub fn content_hash(raw: &str) -> String {
    let d = Sha256::digest(raw.as_bytes());
    d.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// 원자적 저장 + 기존 권한 보존.
pub fn save_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let perms = std::fs::metadata(path).ok().map(|m| m.permissions());
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    if let Some(p) = perms {
        tmp.as_file().set_permissions(p)?;
    }
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_encode_round_trip() {
        for raw in ["a\nb\n", "a\r\nb\r\n", "\u{feff}a\r\nb", "x", ""] {
            let d = decode(raw);
            assert!(!d.text.contains('\r'));
            assert_eq!(encode(&d.text, d.eol, d.bom), raw, "{raw:?}");
        }
        let mixed = decode("a\r\nb\n");
        assert_eq!(mixed.eol, Eol::Mixed);
        assert_eq!(mixed.text, "a\nb\n");
    }

    #[test]
    fn hash_is_stable_and_short() {
        assert_eq!(content_hash("a"), content_hash("a"));
        assert_ne!(content_hash("a"), content_hash("b"));
        assert_eq!(content_hash("").len(), 16);
    }

    #[cfg(unix)]
    #[test]
    fn save_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("d.md");
        std::fs::write(&p, "old").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        save_atomic(&p, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new");
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }
}
