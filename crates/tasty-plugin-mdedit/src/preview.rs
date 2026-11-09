//! 미리보기 렌더. 본문 HTML 은 markdown 뷰어의 렌더러를 **그 소스 파일 그대로** 컴파일해
//! 만든다(docs/surfaces/mdedit/index.md#렌더러-제자리-컴파일) — 사본을 두지 않는다. 편집기 고유 요소는 렌더 결과에 덧붙인다.
//!
//! 렌더러가 바뀌면 이 plugin 의 산출물도 바뀐다. 버전 게이트는 cargo 의존 폐포만 보므로 이
//! 결합을 모른다 — `shared_renderer.pin.txt` 와 그 시험이 대신 잡는다.

// 이유: render.rs 는 markdown plugin 의 모듈이라 그쪽 main.rs 가 쓰는 항목(attach mirror 의
// `RemoteView` 생성 등)을 이 크레이트는 쓰지 않는다. 그 파일을 고치지 않는 것이 결정이므로
// (docs/surfaces/mdedit/index.md#렌더러-제자리-컴파일) 쓰지 않는 항목의 경고를 모듈 단위로 끈다 — 그 항목들의 사용은 markdown 크레이트가
// 자기 빌드에서 검사한다.
// render.rs 는 하위 모듈(render/*.rs)을 둔다 — 파일 하나를 #[path] 로 싣으면 하위 모듈을 이 크레이트
// 안에서 찾는다. 그래서 markdown 의 src 폴더를 가리키는 인라인 모듈 안에 선언해 원래 자리에서 찾게 한다.
#[path = "../../tasty-plugin-markdown/src"]
mod shared {
    // 이유: 렌더러는 markdown 뷰어 전용 항목(찾기·주소 결합 등)도 함께 실려 오는데 미리보기는 쓰지 않는다.
    #[allow(dead_code, unused_imports)]
    pub(crate) mod render;
}
pub(crate) use shared::render;

use std::path::Path;

use pulldown_cmark::{Event, Parser};
use tasty_plugin_sdk::Translator;
use tasty_type_appearance::theme::Theme;

/// render.rs 와 그것이 굽는 자산의 내용 지문(docs/surfaces/mdedit/index.md#렌더러-제자리-컴파일). 시험이 현재 내용과 견준다.
#[cfg(test)]
const SHARED_RENDERER_PIN: &str = include_str!("shared_renderer.pin.txt");

/// markdown plugin 설치본의 번역 카탈로그 — 렌더러가 쓰는 `markdown.*` 문자열을 같은 번역으로
/// 읽는다. 설치본이 없으면 키가 그대로 보인다.
pub fn markdown_translator(own_plugin_dir: Option<&Path>, locale: &str) -> Translator {
    let Some(dir) = own_plugin_dir.and_then(Path::parent) else {
        return Translator::default();
    };
    let user_lang = std::env::var_os("TASTY_PARENT_HOME")
        .filter(|r| !r.is_empty())
        .map(|r| std::path::PathBuf::from(r).join("lang"));
    Translator::load_with_overrides(
        &dir.join("com.tasty.markdown").join("lang"),
        locale,
        "com.tasty.markdown",
        user_lang.as_deref(),
    )
}

/// 미리보기에 그려지는 체크박스의 체크 문자 바이트 오프셋 — 문서 순서. 미리보기의 n 번째
/// `<input type=checkbox>` 가 이 목록의 n 번째다.
pub fn task_offsets(source: &str) -> Vec<usize> {
    Parser::new_ext(source, crate::core::blocks::parser_options())
        .into_offset_iter()
        .filter_map(|(ev, range)| match ev {
            Event::TaskListMarker(_) => {
                source[range.clone()].find('[').map(|i| range.start + i + 1)
            }
            _ => None,
        })
        .collect()
}

/// 렌더러가 일반 인용으로 내보낸 `> [!agent] HH:MM 제목` 블록에 배지를 단다(A6). 원문은
/// 그대로다 — 기존 뷰어에서는 여전히 일반 인용으로 보인다.
///
/// 입력은 sanitize 를 거친 HTML 이라 제목은 이미 이스케이프돼 있다. 바꾸는 것은 여는
/// `<blockquote>` 와 첫 `<p>` 머리뿐이다.
pub fn decorate_agent_notes(html: &str, badge_label: &str) -> String {
    const OPEN: &str = "<blockquote>\n<p>[!agent]";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(OPEN) {
        out.push_str(&rest[..i]);
        let after = &rest[i + OPEN.len()..];
        let line_end = after.find(['\n', '<']).unwrap_or(after.len());
        let head = after[..line_end].trim();
        let (time, title) = match head.split_once(' ') {
            Some((t, rest)) if is_hhmm(t) => (Some(t), rest.trim()),
            _ if is_hhmm(head) => (Some(head), ""),
            _ => (None, head),
        };
        out.push_str(r#"<blockquote class="mdedit-agent-note"><p class="mdedit-agent-head"><span class="mdedit-agent-badge">"#);
        out.push_str(badge_label);
        out.push_str("</span>");
        if let Some(t) = time {
            out.push_str(r#"<span class="mdedit-agent-time">"#);
            out.push_str(t);
            out.push_str("</span>");
        }
        out.push_str(r#"<span class="mdedit-agent-title">"#);
        out.push_str(title);
        out.push_str("</span>");
        rest = &after[line_end..];
        // 제목 줄이 문단의 끝이 아니면(같은 문단에 줄이 이어지면) 새 문단으로 연다.
        if let Some(r) = rest.strip_prefix('\n') {
            out.push_str("</p>\n<p>");
            rest = r;
        }
    }
    out.push_str(rest);
    out
}

fn is_hhmm(s: &str) -> bool {
    let b = s.as_bytes();
    matches!(b.len(), 4 | 5)
        && b[b.len() - 3] == b':'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == b.len() - 3 || c.is_ascii_digit())
}

/// 렌더러 입력.
pub struct PreviewInput<'a> {
    pub theme: &'a Theme,
    pub md_tr: &'a Translator,
    pub file_path: &'a str,
    pub source: &'a str,
    pub load_error: Option<&'a str>,
    pub badge_label: &'a str,
}

/// 완성 HTML 문서(편집기 요소는 아직 없다).
pub fn render(input: PreviewInput) -> String {
    let base_dir = Path::new(input.file_path).parent();
    let html = render::render_document(render::DocumentInput {
        theme: input.theme,
        tr: input.md_tr,
        file_path: input.file_path,
        source: input.source,
        load_error: input.load_error,
        base_dir,
        recent: &[],
        remote: None,
        // 편집기 미리보기는 찾기·주소창 단축키와 대용량 보류를 쓰지 않는다 — 편집기가 파일 전체를 쥔다.
        large_deferred: false,
        find_bindings: &[],
        address_bindings: &[],
    });
    decorate_agent_notes(&html, input.badge_label)
}

/// 링크 클릭을 렌더러의 규칙대로 해석한다(파일 → host 로 열기, URL → OS).
pub fn classify(dest: &str, base_dir: Option<&Path>) -> Option<render::LinkClick> {
    render::classify_link(dest, base_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_offsets_point_at_the_check_char() {
        let src = "# t\n- [ ] a\n- [x] b\n\n```\n- [ ] not\n```\n> - [ ] quoted\n";
        let offs = task_offsets(src);
        assert_eq!(offs.len(), 3);
        for o in &offs {
            assert!(matches!(&src[*o..*o + 1], " " | "x"), "offset {o}");
            assert_eq!(&src[o - 1..*o], "[");
        }
    }

    #[test]
    fn agent_note_gets_a_badge_and_keeps_body() {
        let html = "<blockquote>\n<p>[!agent] 15:10 일지 구조 개편</p>\n<ul>\n<li>a</li>\n</ul>\n</blockquote>";
        let out = decorate_agent_notes(html, "AGENT");
        assert!(
            out.starts_with(r#"<blockquote class="mdedit-agent-note">"#),
            "{out}"
        );
        assert!(out.contains(r#"<span class="mdedit-agent-time">15:10</span>"#));
        assert!(out.contains(r#"<span class="mdedit-agent-title">일지 구조 개편</span></p>"#));
        assert!(out.contains("<li>a</li>"));
        let html = "<blockquote>\n<p>[!agent] 제목\n둘째 줄</p>\n</blockquote>";
        let out = decorate_agent_notes(html, "A");
        assert!(out.contains("제목</span></p>\n<p>둘째 줄</p>"), "{out}");
        assert_eq!(
            decorate_agent_notes("<blockquote>\n<p>x</p>", "A"),
            "<blockquote>\n<p>x</p>"
        );
    }

    #[test]
    fn rendered_agent_note_is_decorated_end_to_end() {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let tr = Translator::default();
        let html = render(PreviewInput {
            theme: &theme,
            md_tr: &tr,
            file_path: "/tmp/x.md",
            source: "# 2026.10.22\n> [!agent] 15:10 정리\n> - 검증 0건\n",
            load_error: None,
            badge_label: "AGENT",
        });
        assert!(
            html.contains("mdedit-agent-badge"),
            "agent note not decorated"
        );
        assert!(html.contains("검증 0건"));
    }

    /// docs/surfaces/mdedit/index.md#렌더러-제자리-컴파일 — 렌더러 소스·자산이 바뀌면 이 plugin 의 산출물도 바뀐다. 그 결합을 버전
    /// 게이트가 못 보므로 여기서 잡는다.
    #[test]
    fn shared_renderer_pin() {
        let now = super::super::pin::current_pin();
        assert_eq!(
            SHARED_RENDERER_PIN.trim(),
            now,
            "\nmarkdown plugin 의 render.rs 또는 assets/ 가 바뀌었다 — mdedit 산출물도 바뀐다.\n\
             같은 커밋에서: (1) crates/tasty-plugin-mdedit/src/shared_renderer.pin.txt 를 아래 값으로\n\
             갱신 (2) mdedit Cargo.toml·tasty-plugin.toml version patch +1 (3) Cargo.lock 갱신.\n\
             새 pin: {now}\n"
        );
    }
}
