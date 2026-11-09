//! 앱 문구에만 UI 언어가 붙고 사용자 문서에는 붙지 않는지 정제 뒤 HTML로 본다.

use std::path::Path;

use tasty_plugin_sdk::Translator;
use tasty_type_appearance::theme::Theme;

use super::{DocumentInput, render_document, sanitize_html_for};

const UI: &str = "zh-Hans";

fn translator() -> Translator {
    Translator::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("lang"), UI)
}

fn render(source: &str) -> String {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let tr = translator();
    render_document(DocumentInput {
        find_bindings: &[],
        address_bindings: &[],
        theme: &theme,
        tr: &tr,
        file_path: "/a/current.md",
        source,
        load_error: None,
        base_dir: Some(Path::new("/a")),
        recent: &[],
        remote: None,
        large_deferred: false,
    })
}

/// 문서 본문 조각. 앱 문구가 있는 띠와 스크립트를 빼고 본다.
fn body_of(html: &str) -> &str {
    let start = html.find(r#"<div id="tasty-md-body">"#).expect("본문 시작");
    let end = html[start..].find("</div><script>").expect("본문 끝");
    &html[start..start + end]
}

/// `needle`로 시작하는 여는 태그 하나를 돌려준다.
fn tag_with<'a>(html: &'a str, needle: &str) -> &'a str {
    let at = html
        .find(needle)
        .unwrap_or_else(|| panic!("{needle}: {html}"));
    let start = html[..at].rfind('<').expect("태그 시작");
    let end = at + html[at..].find('>').expect("태그 끝");
    &html[start..=end]
}

#[test]
fn default_callout_label_keeps_the_ui_language_after_sanitizing() {
    let html = render("> [!NOTE]\n> 本文");
    let label = tag_with(&html, r#"class="tasty-callout-label""#);
    assert!(label.contains(r#"lang="zh-Hans""#), "{label}");
    assert!(label.contains(r#"data-label="Note""#), "{label}");
}

#[test]
fn callout_label_is_an_empty_span_so_it_adds_no_text_node() {
    let body = render("> [!NOTE]\n> 本文");
    let label = tag_with(&body, r#"class="tasty-callout-label""#);
    assert!(body.contains(&format!("{label}</span>")), "{body}");
}

#[test]
fn callout_body_does_not_inherit_the_ui_language() {
    let html = render("> [!NOTE]\n> 本文");
    let quote = tag_with(&html, r#"class="markdown-alert-note""#);
    assert!(!quote.contains("lang="), "{quote}");
    let body = body_of(&html);
    assert_eq!(
        body.matches("lang=").count(),
        1,
        "라벨 하나뿐이어야 한다: {body}"
    );
}

#[test]
fn custom_callout_title_carries_no_language() {
    let html = render("> [!info] 見出し\n> 本文");
    let body = body_of(&html);
    assert!(body.contains(r#"data-label="見出し""#), "{body}");
    assert!(!body.contains("lang="), "{body}");
}

#[test]
fn foldable_default_label_marks_only_the_summary_text() {
    let html = render("> [!NOTE]+\n> 本文");
    let body = body_of(&html);
    assert!(
        body.contains(r#"<summary><span lang="zh-Hans">Note</span></summary>"#),
        "{body}"
    );
    assert_eq!(body.matches("lang=").count(), 1, "{body}");
}

#[test]
fn foldable_custom_title_carries_no_language() {
    let html = render("> [!NOTE]+ 見出し\n> 本文");
    let body = body_of(&html);
    assert!(body.contains("<summary>見出し</summary>"), "{body}");
    assert!(!body.contains("lang="), "{body}");
}

#[test]
fn document_written_lang_is_removed_and_headings_stay_unmarked() {
    let html = render(
        "# 見出し\n\n<span lang=\"ja\">日本語</span>\n\n<p lang=\"ja\">段落</p>\n\n<blockquote lang=\"ja\">引用</blockquote>",
    );
    let body = body_of(&html);
    assert!(body.contains("見出し"), "{body}");
    assert!(body.contains("日本語"), "{body}");
    assert!(!body.contains("lang="), "{body}");
}

#[test]
fn html_element_carries_no_language() {
    let html = render("> [!NOTE]\n> 本文");
    let root = tag_with(&html, "<html");
    assert!(!root.contains("lang="), "{root}");
}

#[test]
fn sanitizer_keeps_only_the_ui_language_on_span() {
    let dirty =
        r#"<span lang="zh-Hans">a</span><span lang="ja">b</span><span lang="ZH-HANS">c</span>"#;
    assert_eq!(
        sanitize_html_for(dirty, UI),
        r#"<span lang="zh-Hans">a</span><span>b</span><span>c</span>"#
    );
}

#[test]
fn sanitizer_without_a_ui_language_removes_every_lang() {
    let dirty = r#"<span lang="zh-Hans">a</span><span lang="">b</span>"#;
    assert_eq!(sanitize_html_for(dirty, ""), "<span>a</span><span>b</span>");
}

#[test]
fn sanitizer_keeps_other_attributes_through_the_filter() {
    let dirty =
        r#"<a href="https://example.com" title="t">x</a><span class="math math-inline">y</span>"#;
    let clean = sanitize_html_for(dirty, UI);
    assert!(clean.contains(r#"href="https://example.com""#), "{clean}");
    assert!(clean.contains(r#"class="math math-inline""#), "{clean}");
}
