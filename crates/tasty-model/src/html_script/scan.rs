//! 문서 원본을 한 번 읽어 스크립트 감지와 파일 전체 지문을 함께 구한다.
//! 바이트 입력만 다룬다. 파일을 열고 정규 파일인지 확인하는 일은 호스트가 맡는다.
//!
//! 감지는 상한까지만 하고 지문은 파일 끝까지 구한다(docs/surfaces/html/index.md). 상한 뒤에만 있는 스크립트와
//! 원격 참조는 감지하지 못하며, 이때는 차단된 채 배너가 뜨지 않는다.

use std::io::Read;

use sha2::{Digest, Sha256};

use super::{Fingerprint, ScriptDetection, ScriptScan};

/// 감지 스캔의 상한. 지문에는 적용하지 않는다.
pub const SCAN_LIMIT_BYTES: usize = 4 * 1024 * 1024;

const READ_CHUNK: usize = 64 * 1024;

/// `reader`를 끝까지 읽어 지문을 구하고, 앞의 `limit` 바이트로 스크립트를 감지한다.
pub fn scan_reader(mut reader: impl Read, limit: usize) -> std::io::Result<ScriptScan> {
    let mut hasher = Sha256::new();
    let mut head = Vec::new();
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        let chunk = &buf[..n];
        hasher.update(chunk);
        if head.len() < limit {
            let take = (limit - head.len()).min(n);
            head.extend_from_slice(&chunk[..take]);
        }
    }
    let found = detect(&head);
    Ok(ScriptScan {
        fingerprint: Fingerprint(hasher.finalize().into()),
        detection: found.scripts,
        remote_refs: found.remote_refs,
        remote_scripts: found.remote_scripts,
    })
}

/// HTML 원본에서 실행 가능한 스크립트를 찾는다. 주석과 원문 텍스트 요소의 내용은 보지 않는다.
pub fn detect_scripts(src: &[u8]) -> ScriptDetection {
    detect(src).scripts
}

/// HTML 원본에서 원격(`http(s)`) 리소스 참조를 찾는다. 결과는 "참조가 있다"이며 실제 차단 여부가 아니다.
pub fn detect_remote_refs(src: &[u8]) -> bool {
    detect(src).remote_refs
}

/// 한 번의 순회로 구한 감지 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub scripts: ScriptDetection,
    pub remote_refs: bool,
    /// 원격에서 받는 실행 가능한 스크립트가 있다. 로컬 스크립트와 섞여 있어도 선다.
    pub remote_scripts: bool,
}

/// 스크립트와 원격 참조를 함께 감지한다. 원격 참조를 놓치지 않도록 원본(상한까지) 끝까지 순회한다.
pub fn detect(src: &[u8]) -> Detection {
    let mut local_scripts = false;
    // 기존 판정(`ScriptsRemoteOnly`)은 `<base href>`를 보지 않는다. 원격 스크립트 비트는 base를 반영한다.
    let mut legacy_remote_scripts = false;
    let mut remote_scripts = false;
    let mut remote_refs = false;
    // 첫 `<base href>`만 상대 URL 해석에 쓰인다. 원격이면 상대 경로 참조도 원격이다.
    let mut base: Option<bool> = None;
    // 열린 `<template>` 깊이. template 내용은 문서에 붙기 전에는 리소스를 받지 않으므로 원격 감지에서만
    // 뺀다. 스크립트 판정은 template을 보통 태그로 읽던 원래 판정 그대로다. 재귀하지 않고 한 번 순회한다.
    let mut template_depth: usize = 0;
    let mut i = 0;
    while i < src.len() {
        if src[i] != b'<' {
            i += 1;
            continue;
        }
        if src[i..].starts_with(b"<!--") {
            i = find(src, i + 4, b"-->").map_or(src.len(), |e| e + 3);
            continue;
        }
        let Some(tag) = parse_tag(src, i) else {
            i += 1;
            continue;
        };
        i = tag.end;
        if tag.closing {
            if tag.name == "template" {
                template_depth = template_depth.saturating_sub(1);
            }
            continue;
        }
        let inert = template_depth > 0;
        if tag.name == "template" {
            template_depth += 1;
        }
        if tag.name == "base"
            && !inert
            && base.is_none()
            && let Some(href) = attr(&tag.attrs, "href")
        {
            base = Some(is_remote_url(href));
        }
        let base_remote = base.unwrap_or(false);
        if tag.attrs.iter().any(|a| is_event_handler(&a.name)) {
            local_scripts = true;
        }
        if tag
            .attrs
            .iter()
            .any(|a| is_url_attribute(&a.name) && a.value.as_deref().is_some_and(is_javascript_url))
        {
            local_scripts = true;
        }
        if !remote_refs && !inert {
            remote_refs = tag_has_remote_ref(&tag, base_remote);
        }
        if tag.name == "script" {
            if is_executable_script(&tag.attrs) {
                let src = script_src(&tag.attrs);
                if src.is_some_and(is_remote_url) {
                    legacy_remote_scripts = true;
                } else {
                    local_scripts = true;
                }
                if !inert && src.is_some_and(|url| resolves_remote(url, base_remote)) {
                    remote_scripts = true;
                }
            }
            i = skip_raw_text(src, i, b"script");
        } else if tag.name == "style" {
            let end = skip_raw_text(src, i, b"style");
            if !remote_refs && !inert {
                remote_refs = css_has_remote_ref(&src[i..end], base_remote);
            }
            i = end;
        } else if let Some(name) = RAW_TEXT_ELEMENTS.iter().find(|n| **n == tag.name) {
            i = skip_raw_text(src, i, name.as_bytes());
        }
    }
    let scripts = if local_scripts {
        ScriptDetection::Scripts
    } else if legacy_remote_scripts {
        ScriptDetection::ScriptsRemoteOnly
    } else {
        ScriptDetection::None
    };
    Detection {
        scripts,
        remote_refs: remote_refs || remote_scripts,
        remote_scripts,
    }
}

fn attr<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| a.value.as_deref())
}

/// `link`의 `rel` 중 WebView가 리소스를 받아 오는 값.
const FETCHING_LINK_RELS: &[&str] = &[
    "stylesheet",
    "icon",
    "apple-touch-icon",
    "preload",
    "prefetch",
    "modulepreload",
    "manifest",
];

/// 태그가 리소스로 불러오는 URL 속성 가운데 원격으로 해석되는 것이 있는지. `style` 속성의 CSS도 본다.
fn tag_has_remote_ref(tag: &Tag, base_remote: bool) -> bool {
    let remote =
        |name: &str| attr(&tag.attrs, name).is_some_and(|u| resolves_remote(u, base_remote));
    let remote_srcset =
        |name: &str| attr(&tag.attrs, name).is_some_and(|v| srcset_has_remote(v, base_remote));
    let by_element = match tag.name.as_str() {
        "link" => {
            let rel = attr(&tag.attrs, "rel").unwrap_or("").to_ascii_lowercase();
            rel.split_ascii_whitespace()
                .any(|r| FETCHING_LINK_RELS.contains(&r))
                && (remote("href") || remote_srcset("imagesrcset"))
        }
        "img" => remote("src") || remote_srcset("srcset"),
        "source" => remote("src") || remote_srcset("srcset"),
        "video" => remote("src") || remote("poster"),
        "audio" | "track" | "iframe" | "frame" | "embed" => remote("src"),
        "input" => {
            attr(&tag.attrs, "type").is_some_and(|t| t.trim().eq_ignore_ascii_case("image"))
                && remote("src")
        }
        "object" => remote("data"),
        "image" | "feimage" | "use" => remote("href") || remote("xlink:href"),
        "body" | "table" | "td" | "th" => remote("background"),
        _ => false,
    };
    by_element
        || attr(&tag.attrs, "style")
            .is_some_and(|css| css_has_remote_ref(css.as_bytes(), base_remote))
}

/// `srcset`의 각 후보 URL. HTML 명세처럼 공백까지를 URL로 읽어 `data:` URL 안의 쉼표를 후보 구분으로 보지 않는다.
fn srcset_has_remote(value: &str, base_remote: bool) -> bool {
    let mut rest = value;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if rest.is_empty() {
            return false;
        }
        let end = rest
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let url = &rest[..end];
        rest = &rest[end..];
        // URL 끝의 쉼표는 후보 구분이다. 그때는 설명자가 없다.
        let (url, described) = match url.strip_suffix(',') {
            Some(u) => (u.trim_end_matches(','), false),
            None => (url, true),
        };
        if resolves_remote(url, base_remote) {
            return true;
        }
        if described {
            // 설명자(`2x`, `100w`)는 괄호 밖의 다음 쉼표까지다.
            let mut depth = 0u32;
            let cut = rest
                .char_indices()
                .find(|&(_, c)| {
                    match c {
                        '(' => depth += 1,
                        ')' => depth = depth.saturating_sub(1),
                        ',' if depth == 0 => return true,
                        _ => {}
                    }
                    false
                })
                .map_or(rest.len(), |(at, _)| at + 1);
            rest = &rest[cut..];
        }
    }
}

/// CSS 텍스트의 `url(...)`과 `@import "..."`에서 원격 URL을 찾는다. CSS 주석은 건너뛴다.
fn css_has_remote_ref(css: &[u8], base_remote: bool) -> bool {
    let text = String::from_utf8_lossy(css);
    let mut stripped = String::with_capacity(text.len());
    let mut rest = text.as_ref();
    while let Some(start) = rest.find("/*") {
        stripped.push_str(&rest[..start]);
        rest = rest[start + 2..]
            .find("*/")
            .map_or("", |e| &rest[start + 2 + e + 2..]);
    }
    stripped.push_str(rest);
    let lower = stripped.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("url(") {
        let start = from + pos + 4;
        let end = lower[start..].find(')').map_or(lower.len(), |e| start + e);
        let raw = stripped[start..end]
            .trim()
            .trim_matches(|c| c == '"' || c == '\'');
        if resolves_remote(raw, base_remote) {
            return true;
        }
        from = end;
    }
    let mut from = 0;
    while let Some(pos) = lower[from..].find("@import") {
        let start = from + pos + "@import".len();
        let tail = stripped[start..].trim_start();
        if let Some(q) = tail.chars().next().filter(|c| *c == '"' || *c == '\'')
            && let Some(end) = tail[1..].find(q)
            && resolves_remote(&tail[1..1 + end], base_remote)
        {
            return true;
        }
        from = start;
    }
    false
}

/// 문서 기준으로 해석했을 때 `http(s)` URL인지. 문서는 `file://`이고 `base_remote`면 `<base href>`가 원격이다.
/// 스킴이 있는 다른 URL(`data:` 등)과 fragment만 있는 값은 원격이 아니다.
fn resolves_remote(url: &str, base_remote: bool) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    if is_remote_url(trimmed) {
        return true;
    }
    if has_scheme(trimmed) {
        return false;
    }
    // 상대 경로와 `//host`는 기준 URL의 스킴을 따른다. `file://` 문서에서는 `file://host`다.
    base_remote
}

/// URL이 `scheme:`으로 시작하는지(RFC 3986 scheme 문자).
fn has_scheme(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// 내용을 태그로 해석하지 않는 요소. `script`는 따로 처리한다.
const RAW_TEXT_ELEMENTS: &[&str] = &["style", "textarea", "title", "xmp", "noembed", "noframes"];

/// HTML 명세가 JavaScript로 해석하는 MIME 형식.
const JAVASCRIPT_MIME_TYPES: &[&str] = &[
    "application/ecmascript",
    "application/javascript",
    "application/x-ecmascript",
    "application/x-javascript",
    "text/ecmascript",
    "text/javascript",
    "text/javascript1.0",
    "text/javascript1.1",
    "text/javascript1.2",
    "text/javascript1.3",
    "text/javascript1.4",
    "text/javascript1.5",
    "text/jscript",
    "text/livescript",
    "text/x-ecmascript",
    "text/x-javascript",
];

const URL_ATTRIBUTES: &[&str] = &["href", "src", "action", "formaction", "xlink:href"];

struct Attr {
    name: String,
    value: Option<String>,
}

struct Tag {
    name: String,
    closing: bool,
    attrs: Vec<Attr>,
    /// `>` 다음 위치. 태그가 닫히지 않았으면 원본 끝이다.
    end: usize,
}

fn find(src: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= src.len() {
        return None;
    }
    src[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn is_name_byte(b: u8) -> bool {
    !(b.is_ascii_whitespace() || b == b'/' || b == b'>' || b == b'=')
}

/// `start`의 `<`에서 태그 하나를 읽는다. 태그 이름이 영문자로 시작하지 않으면 `None`.
fn parse_tag(src: &[u8], start: usize) -> Option<Tag> {
    let mut i = start + 1;
    let closing = src.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    if !src.get(i).is_some_and(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let name_start = i;
    while i < src.len() && is_name_byte(src[i]) {
        i += 1;
    }
    let name = String::from_utf8_lossy(&src[name_start..i]).to_ascii_lowercase();
    let mut attrs = Vec::new();
    loop {
        while i < src.len() && (src[i].is_ascii_whitespace() || src[i] == b'/') {
            i += 1;
        }
        if i >= src.len() {
            break;
        }
        if src[i] == b'>' {
            i += 1;
            break;
        }
        let attr_start = i;
        // `=`로 시작하는 이상한 이름도 한 글자로 소비해 진행을 보장한다.
        i += 1;
        while i < src.len() && is_name_byte(src[i]) {
            i += 1;
        }
        let attr_name = String::from_utf8_lossy(&src[attr_start..i]).to_ascii_lowercase();
        let mut j = i;
        while j < src.len() && src[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut value = None;
        if src.get(j) == Some(&b'=') {
            j += 1;
            while j < src.len() && src[j].is_ascii_whitespace() {
                j += 1;
            }
            let (v, next) = read_attr_value(src, j);
            value = Some(v);
            i = next;
        }
        attrs.push(Attr {
            name: attr_name,
            value,
        });
    }
    Some(Tag {
        name,
        closing,
        attrs,
        end: i,
    })
}

fn read_attr_value(src: &[u8], start: usize) -> (String, usize) {
    match src.get(start) {
        Some(&q) if q == b'"' || q == b'\'' => {
            let end = src[start + 1..]
                .iter()
                .position(|&b| b == q)
                .map_or(src.len(), |p| start + 1 + p);
            let value = String::from_utf8_lossy(&src[start + 1..end]).into_owned();
            (value, (end + 1).min(src.len()))
        }
        _ => {
            let mut end = start;
            while end < src.len() && !src[end].is_ascii_whitespace() && src[end] != b'>' {
                end += 1;
            }
            (String::from_utf8_lossy(&src[start..end]).into_owned(), end)
        }
    }
}

/// `</name`이 나올 때까지 건너뛴다. 닫는 태그가 없으면 원본 끝까지다.
fn skip_raw_text(src: &[u8], from: usize, name: &[u8]) -> usize {
    let mut i = from;
    while let Some(lt) = find(src, i, b"</") {
        let after = lt + 2;
        let candidate = src.get(after..after + name.len());
        let boundary = src.get(after + name.len()).copied();
        if candidate.is_some_and(|c| c.eq_ignore_ascii_case(name))
            && boundary.is_none_or(|b| !is_name_byte(b))
        {
            return lt;
        }
        i = after;
    }
    src.len()
}

fn is_event_handler(name: &str) -> bool {
    name.len() > 2 && name.starts_with("on")
}

fn is_url_attribute(name: &str) -> bool {
    URL_ATTRIBUTES.contains(&name)
}

/// 브라우저처럼 앞뒤 공백·제어 문자와 중간의 탭·줄바꿈을 무시하고 스킴을 비교한다.
fn is_javascript_url(value: &str) -> bool {
    let cleaned: String = value
        .trim_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .take("javascript:".len())
        .collect();
    cleaned.eq_ignore_ascii_case("javascript:")
}

fn is_executable_script(attrs: &[Attr]) -> bool {
    let Some(ty) = attrs.iter().find(|a| a.name == "type") else {
        return true;
    };
    let ty = ty
        .value
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if ty.is_empty() || ty == "module" {
        return true;
    }
    // 명세상 매개변수가 붙은 형식은 실행되지 않지만, 놓치는 쪽보다 배너가 뜨는 쪽을 택해 매개변수를 뺀다.
    let essence = ty.split(';').next().unwrap_or("").trim();
    JAVASCRIPT_MIME_TYPES.contains(&essence)
}

fn script_src(attrs: &[Attr]) -> Option<&str> {
    attrs
        .iter()
        .find(|a| a.name == "src" || a.name == "href" || a.name == "xlink:href")
        .and_then(|a| a.value.as_deref())
}

/// `file://` 문서에서 `//host` 형태는 `file://host`로 해석되므로 원격으로 보지 않는다.
fn is_remote_url(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("http://") || url.starts_with("https://")
}

#[cfg(test)]
mod tests;
