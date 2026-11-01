use super::*;
use sha2::{Digest, Sha256};

fn detect(src: &str) -> ScriptDetection {
    detect_scripts(src.as_bytes())
}

#[test]
fn inline_and_local_scripts_are_detected() {
    assert_eq!(
        detect("<p>x</p><script>a()</script>"),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<script src="app.js"></script>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<script type="module">a()</script>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<script type="">a()</script>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<script type="text/javascript; charset=utf-8">a()</script>"#),
        ScriptDetection::Scripts,
        "매개변수가 붙어도 보수적으로 스크립트로 본다"
    );
}

#[test]
fn data_blocks_are_not_scripts() {
    assert_eq!(
        detect(
            r#"<script type="application/json">{"onclick": "<a href='javascript:x'>"}</script>"#
        ),
        ScriptDetection::None
    );
    assert_eq!(
        detect(r#"<script type="text/template"><div onclick="x()"></div></script>"#),
        ScriptDetection::None
    );
}

#[test]
fn tag_and_attribute_names_are_case_insensitive() {
    assert_eq!(detect("<SCRIPT>a()</SCRIPT>"), ScriptDetection::Scripts);
    assert_eq!(
        detect(r#"<DIV ONCLICK="a()">x</DIV>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<script TYPE="Application/JSON">{}</script>"#),
        ScriptDetection::None
    );
}

#[test]
fn unquoted_attributes_are_read() {
    assert_eq!(detect("<body onload=init()>"), ScriptDetection::Scripts);
    assert_eq!(
        detect("<a href=javascript:go()>x</a>"),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect("<script type=application/json>{}</script>"),
        ScriptDetection::None
    );
}

#[test]
fn tags_inside_comments_are_ignored() {
    assert_eq!(
        detect("<!-- <script>a()</script> <div onclick=x> -->"),
        ScriptDetection::None
    );
    assert_eq!(
        detect("<!--[if IE]><script>a()</script><![endif]--><p>ok</p>"),
        ScriptDetection::None
    );
    assert_eq!(
        detect("<!-- note --><script>a()</script>"),
        ScriptDetection::Scripts
    );
}

#[test]
fn javascript_urls_ignore_case_and_whitespace() {
    assert_eq!(
        detect(r#"<a href="  JaVaScRiPt:alert(1)">x</a>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect("<a href=\"java\tscr\nipt:alert(1)\">x</a>"),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<form action="javascript:x()"></form>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<button formaction="javascript:x()">b</button>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<iframe src="javascript:x()"></iframe>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<a href="javascript-guide.html">x</a>"#),
        ScriptDetection::None
    );
    assert_eq!(
        detect(r#"<a title="javascript:x()">x</a>"#),
        ScriptDetection::None,
        "URL 속성만 본다"
    );
}

#[test]
fn remote_only_scripts_are_reported_separately() {
    assert_eq!(
        detect(r#"<script src="https://cdn.example/app.js"></script>"#),
        ScriptDetection::ScriptsRemoteOnly
    );
    assert_eq!(
        detect(
            r#"<script src="HTTP://cdn.example/a.js"></script><script src="http://x/b.js"></script>"#
        ),
        ScriptDetection::ScriptsRemoteOnly
    );
    assert_eq!(
        detect(r#"<script src="https://cdn.example/a.js"></script><script>b()</script>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<script src="//cdn.example/a.js"></script>"#),
        ScriptDetection::Scripts,
        "file:// 문서에서 //host 는 원격이 아니다"
    );
}

#[test]
fn inline_svg_scripts_are_detected() {
    assert_eq!(
        detect(r#"<svg><script>a()</script></svg>"#),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<svg><script xlink:href="https://x/a.js"/></svg>"#),
        ScriptDetection::ScriptsRemoteOnly
    );
    assert_eq!(
        detect(r#"<svg><set attributeName="x" onbegin="a()"/></svg>"#),
        ScriptDetection::Scripts
    );
}

#[test]
fn text_that_only_looks_like_a_tag_is_not_a_script() {
    assert_eq!(detect("<p>a < script b</p>"), ScriptDetection::None);
    assert_eq!(detect("<p>&lt;script&gt;</p>"), ScriptDetection::None);
    assert_eq!(
        detect(r#"<div title="<script>a()</script>">x</div>"#),
        ScriptDetection::None
    );
    assert_eq!(
        detect("<textarea><script>a()</script></textarea>"),
        ScriptDetection::None
    );
    assert_eq!(
        detect("<title><script>a()</script></title>"),
        ScriptDetection::None
    );
    assert_eq!(
        detect("<style>a::after{content:'<div onclick=x>'}</style>"),
        ScriptDetection::None
    );
    assert_eq!(detect("<p>plain</p>"), ScriptDetection::None);
}

#[test]
fn attributes_starting_with_on_are_treated_as_handlers() {
    assert_eq!(
        detect("<p one>x</p>"),
        ScriptDetection::Scripts,
        "on 으로 시작하는 속성은 보수적으로 핸들러로 본다"
    );
    assert_eq!(detect("<p on>x</p>"), ScriptDetection::None);
}

#[test]
fn a_raw_text_element_ends_at_its_own_closing_tag() {
    assert_eq!(
        detect("<textarea>x</textarea><script>a()</script>"),
        ScriptDetection::Scripts
    );
    assert_eq!(
        detect(r#"<script type="text/template"></scripts><div onclick=x></script>"#),
        ScriptDetection::None,
        "</scripts 는 닫는 태그가 아니다"
    );
}

#[test]
fn a_script_after_the_limit_is_not_detected_but_changes_the_fingerprint() {
    let head = "<p>".to_string() + &"x".repeat(100) + "</p>";
    let tail = "<script>document.title='TAIL'</script>";
    let short = scan_reader(head.as_bytes(), head.len()).expect("scan");
    let appended = format!("{head}{tail}");
    let long = scan_reader(appended.as_bytes(), head.len()).expect("scan");
    assert_eq!(short.detection, ScriptDetection::None);
    assert_eq!(
        long.detection,
        ScriptDetection::None,
        "상한 뒤는 감지하지 않는다"
    );
    assert_ne!(
        short.fingerprint, long.fingerprint,
        "지문은 파일 전체를 덮는다"
    );
    let expected: [u8; 32] = Sha256::digest(appended.as_bytes()).into();
    assert_eq!(long.fingerprint, Fingerprint(expected));
}

#[test]
fn a_script_inside_the_limit_is_detected_across_read_chunks() {
    let mut doc = "a".repeat(READ_CHUNK - 3);
    doc.push_str("<script>a()</script>");
    let s = scan_reader(doc.as_bytes(), SCAN_LIMIT_BYTES).expect("scan");
    assert_eq!(s.detection, ScriptDetection::Scripts);
}

fn remote(src: &str) -> bool {
    detect_remote_refs(src.as_bytes())
}

#[test]
fn each_fetching_element_with_a_remote_url_is_a_remote_ref() {
    let cases = [
        r#"<link rel="stylesheet" href="https://cdn.example/a.css">"#,
        r#"<link rel="Shortcut Icon" href="http://e.example/f.ico">"#,
        r#"<link rel="preload" as="image" imagesrcset="https://e.example/a.png 1x">"#,
        r#"<img src="https://e.example/a.png">"#,
        r#"<img src="a.png" srcset="a.png 1x, https://e.example/b.png 2x">"#,
        r#"<picture><source srcset="https://e.example/a.webp"></picture>"#,
        r#"<video src="https://e.example/a.mp4"></video>"#,
        r#"<video poster="https://e.example/p.png"></video>"#,
        r#"<audio src="https://e.example/a.mp3"></audio>"#,
        r#"<video><track src="https://e.example/t.vtt"></video>"#,
        r#"<iframe src="https://e.example/"></iframe>"#,
        r#"<embed src="https://e.example/a.swf">"#,
        r#"<object data="https://e.example/a.svg"></object>"#,
        r#"<input type="image" src="https://e.example/a.png">"#,
        r#"<svg><image href="https://e.example/a.png"/></svg>"#,
        r#"<body background="https://e.example/bg.png">"#,
        r#"<div style="background: url('https://e.example/bg.png')">x</div>"#,
        r#"<style>@import "https://e.example/a.css";</style>"#,
        r#"<style>@import url(https://e.example/a.css);</style>"#,
        r#"<style>p { background: url( "HTTPS://e.example/x.png" ) }</style>"#,
        r#"<script src="https://e.example/a.js"></script>"#,
    ];
    for c in cases {
        assert!(remote(c), "{c}");
    }
}

#[test]
fn local_data_and_non_fetching_urls_are_not_remote_refs() {
    let cases = [
        r#"<link rel="stylesheet" href="style.css">"#,
        r#"<link rel="canonical" href="https://e.example/">"#,
        r#"<link rel="alternate" href="https://e.example/feed">"#,
        r#"<a href="https://e.example/">link</a>"#,
        r#"<img src="data:image/png;base64,AAAA">"#,
        r#"<img src="//e.example/a.png">"#,
        r#"<img src="file:///tmp/a.png">"#,
        r##"<img src="#frag">"##,
        r#"<input type="text" src="https://e.example/a.png">"#,
        r#"<!-- <img src="https://e.example/a.png"> -->"#,
        r#"<textarea><img src="https://e.example/a.png"></textarea>"#,
        r#"<style>/* url(https://e.example/a.png) */ p { color: red }</style>"#,
        r#"<p>https://e.example/a.png</p>"#,
        r#"<script type="application/json">{"u": "<img src='https://e.example/a.png'>"}</script>"#,
    ];
    for c in cases {
        assert!(!remote(c), "{c}");
    }
}

#[test]
fn a_remote_base_makes_relative_and_protocol_relative_refs_remote() {
    assert!(remote(
        r#"<base href="https://e.example/"><img src="a.png">"#
    ));
    assert!(remote(
        r#"<base href="https://e.example/"><link rel="stylesheet" href="//cdn.example/a.css">"#
    ));
    assert!(
        !remote(r#"<base href="sub/"><img src="a.png">"#),
        "relative base stays local"
    );
    assert!(
        !remote(r#"<base href="https://e.example/"><img src="data:image/png;base64,AA">"#),
        "data: ignores the base"
    );
    assert!(
        !remote(r#"<base href="sub/"><base href="https://e.example/"><img src="a.png">"#),
        "only the first base counts"
    );
}

#[test]
fn script_detection_and_remote_refs_are_found_in_one_pass() {
    let d = super::detect(
        br#"<script>a()</script><link rel="stylesheet" href="https://e.example/a.css">"#,
    );
    assert_eq!(d.scripts, ScriptDetection::Scripts);
    assert!(d.remote_refs, "the pass keeps going after the first script");
    let d = super::detect(br#"<script src="https://e.example/a.js"></script>"#);
    assert_eq!(d.scripts, ScriptDetection::ScriptsRemoteOnly);
    assert!(d.remote_refs, "a remote script is a remote ref");
    let d = super::detect(br#"<base href="https://e.example/"><script src="a.js"></script>"#);
    assert_eq!(
        d.scripts,
        ScriptDetection::ScriptsRemoteOnly,
        "a relative script under a remote base loads from the network"
    );
}

#[test]
fn scan_reader_reports_remote_refs_within_the_limit_only() {
    let src = br#"<p>x</p><img src="https://e.example/a.png">"#;
    let full = scan_reader(&src[..], SCAN_LIMIT_BYTES).unwrap();
    assert!(full.remote_refs);
    let cut = scan_reader(&src[..], 8).unwrap();
    assert!(!cut.remote_refs, "refs after the limit are not seen");
    assert_eq!(
        full.fingerprint, cut.fingerprint,
        "the fingerprint covers the whole file"
    );
}
