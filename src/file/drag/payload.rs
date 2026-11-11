//! Immutable local paths; never canonicalize symlinks or read a remote filesystem.
use std::path::PathBuf;

pub(super) const MAX_BYTES: usize = 1024 * 1024;

pub(super) fn validate(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty()
        || paths
            .iter()
            .any(|p| !p.is_absolute() || p.as_os_str().as_encoded_bytes().contains(&0))
    {
        return Err("file drag requires nonempty absolute local paths without NUL".into());
    }
    if paths
        .iter()
        .map(|p| {
            p.as_os_str()
                .as_encoded_bytes()
                .len()
                .saturating_mul(3)
                .saturating_add(9)
        })
        .sum::<usize>()
        > MAX_BYTES
    {
        return Err("file drag path payload exceeds 1 MiB".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(super) fn uri_list(paths: &[PathBuf]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::new();
    for path in paths {
        out.push_str("file://");
        for &byte in path.as_os_str().as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'.' | b'_' | b'~') {
                out.push(byte as char);
            } else {
                use std::fmt::Write;
                // Writing to a String cannot fail.
                write!(out, "%{byte:02X}").expect("String formatting");
            }
        }
        out.push_str("\r\n");
    }
    out.into_bytes()
}

/// Decodes an RFC 2483 `text/uri-list` into local paths, keeping non-UTF-8 bytes. Comment and
/// empty lines are skipped, and so is any entry that is not a local `file://` URI with an absolute
/// path free of NUL. A list with no usable entry is an error.
#[cfg(target_os = "linux")]
pub(super) fn parse_uri_list(bytes: &[u8]) -> Result<Vec<PathBuf>, String> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    if bytes.len() > MAX_BYTES {
        return Err("file drag path payload exceeds 1 MiB".into());
    }
    let paths: Vec<PathBuf> = bytes
        .split(|&byte| byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.is_empty() && !line.starts_with(b"#"))
        .filter_map(local_file_path)
        .map(|path| PathBuf::from(OsString::from_vec(path)))
        .collect();
    if paths.is_empty() {
        return Err("file drag list has no local file".into());
    }
    validate(&paths)?;
    Ok(paths)
}

/// `file:///p` and `file://localhost/p` only. Other hosts name a remote filesystem.
#[cfg(target_os = "linux")]
fn local_file_path(uri: &[u8]) -> Option<Vec<u8>> {
    let rest = uri.strip_prefix(b"file://")?;
    let slash = rest.iter().position(|&byte| byte == b'/')?;
    if !matches!(&rest[..slash], b"" | b"localhost") {
        return None;
    }
    let mut out = Vec::with_capacity(rest.len() - slash);
    let mut encoded = rest[slash..].iter();
    while let Some(&byte) = encoded.next() {
        if byte == b'%' {
            let hex = [*encoded.next()?, *encoded.next()?];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            out.push(byte);
        }
    }
    (!out.contains(&0)).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_and_oversized_payloads() {
        assert!(validate(&[]).is_err());
        assert!(validate(&[PathBuf::from("relative")]).is_err());
        assert!(validate(&[PathBuf::from("/a\0b")]).is_err());
        assert!(validate(&[PathBuf::from(format!("/{}", "a".repeat(MAX_BYTES)))]).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn preserves_bytes_and_link_names_without_filesystem_access() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let paths = [
            PathBuf::from(OsString::from_vec(b"/space #\n%\xff".to_vec())),
            PathBuf::from("/broken-link"),
        ];
        assert!(validate(&paths).is_ok());
        assert_eq!(
            uri_list(&paths),
            b"file:///space%20%23%0A%25%FF\r\nfile:///broken-link\r\n"
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn decodes_local_file_lists() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let list = b"# from a file manager\r\nfile:///a%20b/%ED%95%9C%25\r\n\r\nfile://localhost/c\nhttps://example.com/x\r\nfile://host/remote\r\nfile:///bad%2\r\nfile:///nul%00\r\nfile:relative\r\n";
        assert_eq!(
            parse_uri_list(list).unwrap(),
            [PathBuf::from("/a b/한%"), PathBuf::from("/c")]
        );
        let paths = [
            PathBuf::from(OsString::from_vec(b"/space #\n%\xff".to_vec())),
            PathBuf::from("/broken-link"),
        ];
        assert_eq!(parse_uri_list(&uri_list(&paths)).unwrap(), paths);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn rejects_lists_without_a_local_file_or_too_large() {
        assert!(parse_uri_list(b"").is_err());
        assert!(parse_uri_list(b"# only a comment\r\n").is_err());
        assert!(parse_uri_list(b"file://host/a\r\nsmb://x/y\r\n").is_err());
        let mut big = b"file:///".to_vec();
        big.resize(MAX_BYTES + 1, b'a');
        assert!(parse_uri_list(&big).is_err());
    }
}
