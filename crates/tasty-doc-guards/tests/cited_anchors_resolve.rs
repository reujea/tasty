//! Markdown 링크의 문서 내·문서 간 앵커를 검사한다.
//!
//! 제목에서 만든 슬러그와 본문의 명시 HTML 앵커를 대상 집합으로 사용한다.
//! 코드 블록과 인라인 코드에 든 앵커 예시는 대상이 아니다.
//!
//! 외부 URL, 참조식 링크, Markdown이 아닌 대상은 여기서 검사하지 않는다.
//! 없는 파일은 `cited_coordinates_exist`가 검사한다.
//! 경로는 슬래시로 정규화하며 앵커 규칙은 `docs/documentation-model.md`를 따른다.

// 이유: 테스트의 반환값 무시는 제품 코드의 lint 예외 명부에 포함하지 않는다.
#![allow(clippy::let_underscore_must_use)]

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use tasty_doc_guards::floored_walk::{Descend, Floor, Walked, walk_with_floor};

/// 저장소 Markdown 순회가 빈 목록이나 크게 누락된 목록으로 통과하지 않게 한다.
/// 점 디렉터리, 빌드 캐시, 의존성 디렉터리와 심볼릭 링크는 순회에서 제외한다.
const MD_FLOOR: Floor = Floor {
    min: 150,
    measured: 199,
    measured_on: "2026-11-08",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "acbddc755에 site/ 삭제를 더한 트리 — git ls-files로 점 디렉터리 밖 추적 Markdown 199개를 셌다.",
    ),
    why_this_gap: "추적 Markdown은 199개다. 하한은 로컬 파일에 의존하지 않으며 가장 큰 비-ADR 문서 분류인 \
                   docs/features의 49개만큼 여유를 둔다. 검사 범위가 바뀌면 다시 측정한다.",
};

/// 문서 수와 별개로 링크 추출 누락을 찾는 하한.
/// 2026-09-08 실측 160건(문서 내 33·문서 간 127)을 기준으로 뒀다.
const REFS_FLOOR: usize = 120;

/// 링크 표시 글자에서 강조·코드 마커를 제거하고 GitHub 방식의 제목 슬러그를 만든다.
fn slug(text: &str) -> String {
    let kept: String = link_text_only(text)
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == ' ')
        .collect();
    kept.trim().to_lowercase().replace(' ', "-")
}

/// 제목에서 링크 대상과 강조·코드 마커를 제거한다.
fn link_text_only(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '['
            && let Some(close) = (i + 1..bytes.len()).find(|&j| bytes[j] == ']')
            && bytes.get(close + 1) == Some(&'(')
            && let Some(end) = (close + 2..bytes.len()).find(|&j| bytes[j] == ')')
        {
            s.extend(&bytes[i + 1..close]);
            i = end + 1;
            continue;
        }
        s.push(bytes[i]);
        i += 1;
    }
    s.replace("**", "").replace(['`', '*'], "")
}

/// 인라인 코드 스팬을 지운다 — 백틱 안의 `](#x)` 는 링크가 아니라 예시다.
fn without_inline_code(mut line: &str) -> String {
    let mut out = String::new();
    while let Some(at) = line.find(['`', '<']) {
        out.push_str(&line[..at]);
        if line.as_bytes()[at] == b'<' {
            // HTML 속성의 백틱은 코드 구분자가 아니다. 유효한 태그 전체를 먼저 소비한다.
            let tail = &line[at + 1..];
            let end = html_tag_end(tail)
                .filter(|end| html_tag_name(&tail[..*end]).is_some())
                .map_or(at + 1, |end| at + end + 2);
            out.push_str(&line[at..end]);
            line = &line[end..];
            continue;
        }
        let run = line[at..].bytes().take_while(|b| *b == b'`').count();
        let after = &line[at + run..];
        let mut scan = after;
        let mut closed = false;
        while let Some(next) = scan.find('`') {
            let closing = scan[next..].bytes().take_while(|b| *b == b'`').count();
            scan = &scan[next + closing..];
            if closing == run {
                line = scan;
                closed = true;
                break;
            }
        }
        if !closed {
            out.push_str(&line[at..at + run]);
            line = after;
        }
    }
    out.push_str(line);
    out
}

/// 코드펜스 밖의 줄만 (1-based 줄번호와 함께) 낸다.
fn prose_lines(contents: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for (i, line) in contents.lines().enumerate() {
        let text = line.trim_start();
        let marker = text.as_bytes().first().copied().unwrap_or_default();
        let count = text.bytes().take_while(|b| *b == marker).count();
        if let Some((opened, width)) = fence {
            if marker == opened && count >= width && text[count..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if matches!(marker, b'`' | b'~') && count >= 3 {
            fence = Some((marker, count));
        } else {
            out.push((i + 1, line));
        }
    }
    out
}

fn anchors_of(contents: &str) -> HashSet<String> {
    anchors_with(contents, slug)
}

/// 같은 제목의 중복 앵커에는 문서 순서대로 -1·-2를 붙인다.
fn anchors_with(contents: &str, rule: fn(&str) -> String) -> HashSet<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = explicit_html_anchors(contents);
    for (_, line) in prose_lines(contents) {
        let Some(rest) = line.strip_prefix('#') else {
            continue;
        };
        let rest = rest.trim_start_matches('#');
        let Some(text) = rest.strip_prefix(' ') else {
            continue;
        };
        let base = rule(text);
        if base.is_empty() {
            continue;
        }
        let n = seen.entry(base.clone()).or_insert(0);
        let id = if *n == 0 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        *n += 1;
        out.insert(id);
    }
    out
}

/// 코드 예시와 HTML 주석 밖의 시작 태그에서 id와 a의 name을 읽는다.
/// 저장소에서 쓰는 한 줄 태그를 지원한다. 여러 줄 태그와 HTML entity 해석은 하지 않는다.
fn explicit_html_anchors(contents: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut in_comment = false;
    for (_, line) in prose_lines(contents) {
        if line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }
        let line = without_inline_code(line);
        let mut rest = line.as_str();
        while !rest.is_empty() {
            if in_comment {
                let Some((_, after)) = rest.split_once("-->") else {
                    break;
                };
                rest = after;
                in_comment = false;
                continue;
            }
            let Some((before, after)) = rest.split_once('<') else {
                break;
            };
            if before.bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1 {
                rest = after;
                continue;
            }
            if let Some(after) = after.strip_prefix("!--") {
                in_comment = true;
                rest = after;
                continue;
            }
            let Some(end) = html_tag_end(after) else {
                break;
            };
            let tag = &after[..end];
            rest = &after[end + 1..];
            let Some(name) = html_tag_name(tag) else {
                continue;
            };
            for (key, value) in html_attributes(&tag[name.len()..]) {
                if (key.eq_ignore_ascii_case("id")
                    || (name.eq_ignore_ascii_case("a") && key.eq_ignore_ascii_case("name")))
                    && !value.is_empty()
                {
                    out.insert(value.to_string());
                }
            }
        }
    }
    out
}

/// CommonMark의 시작 태그명: 영문자로 시작하고 영문자·숫자·하이픈만 이어진다.
fn html_tag_name(tag: &str) -> Option<&str> {
    let end = tag
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(tag.len());
    let name = &tag[..end];
    if tag[end..].starts_with('/') && &tag[end..] != "/" {
        return None;
    }
    let mut chars = name.chars();
    (chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-'))
    .then_some(name)
}

/// 따옴표로 감싼 속성값 안의 >는 태그를 닫지 않는다.
fn html_tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), c) if open == c => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None, '>') => return Some(at),
            _ => {}
        }
    }
    None
}

/// 속성값 전체를 먼저 소비하므로 title 안에 든 id= 예시를 앵커로 오해하지 않는다.
fn html_attributes(mut rest: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(rest.len());
        let key = &rest[..end];
        if key.is_empty() {
            break;
        }
        rest = rest[end..].trim_start();
        let Some(after) = rest.strip_prefix('=') else {
            continue;
        };
        rest = after.trim_start();
        let Some(first) = rest.chars().next() else {
            break;
        };
        let value;
        if first == '\'' || first == '"' {
            let Some((quoted, after)) = rest[1..].split_once(first) else {
                break;
            };
            value = quoted;
            rest = after;
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            value = &rest[..end];
            rest = &rest[end..];
        }
        out.push((key, value));
    }
    out
}

#[test]
fn explicit_anchors_resolve_but_examples_and_unrelated_attributes_do_not() {
    let body = r##"<a id="old-heading"></a>
<a name = 'legacy'></a><div id=section></div>
<a title="id='pretend'" data-id="wrong"></a>
<a title='><a id="inside-string">'></a>
\<a id="escaped"></a>
    <a id="indented-code"></a>
`<a id="inline"></a>`
``example ` <a id="long-inline"></a>``
~~~html
<a id="tilde-fenced"></a>
~~~
````html
```
<a id="long-fenced"></a>
````
<!-- <a id="comment"></a>
<a id="comment-next-line"></a> -->
```html
<a id="fenced"></a>
```
# Heading
[valid](#old-heading) [legacy](#legacy) [missing](#absent)
"##;
    assert_eq!(
        anchors_of(body),
        HashSet::from_iter(["old-heading", "legacy", "section", "heading"].map(str::to_string))
    );
    let corpus = Corpus::from_pairs(&[
        ("docs/a.md", body),
        ("docs/b.md", "[cross](a.md#old-heading)"),
    ]);
    let checked = audit(&corpus);
    assert_eq!(checked.violations.len(), 1, "{:?}", checked.violations);
    assert!(checked.violations[0].contains("absent"));
}

#[test]
fn punctuation_does_not_make_an_html_tag_name() {
    assert!(anchors_of(r#"<a! id="ghost"></a!>"#).is_empty());
}

#[test]
fn a_backtick_in_an_html_attribute_does_not_start_inline_code() {
    let text = "<span title=\"`\"><a id=\"real-after\"></a> `";
    assert_eq!(anchors_of(text), HashSet::from(["real-after".to_string()]));
}

/// 인라인 링크 대상과 1부터 시작하는 줄 번호.
fn links_of(contents: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (no, line) in prose_lines(contents) {
        let line = without_inline_code(line);
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i + 1 < chars.len() {
            if chars[i] == ']' && chars[i + 1] == '(' {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != ')' && !chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ')' {
                    out.push((no, chars[i + 2..j].iter().collect::<String>()));
                }
                i = j.max(i + 1);
                continue;
            }
            i += 1;
        }
    }
    out
}

/// 상대 Markdown 경로와 앵커를 분리한다. 검사 대상이 아니면 None이다.
fn split_anchor(target: &str) -> Option<(Option<&str>, &str)> {
    if target.contains("://") || target.starts_with("mailto:") || target.starts_with('/') {
        return None;
    }
    let (head, anchor) = target.split_once('#')?;
    if anchor.is_empty() {
        return None;
    }
    if head.is_empty() {
        return Some((None, anchor));
    }
    if !head.ends_with(".md") {
        return None;
    }
    Some((Some(head), anchor))
}

/// 점 디렉터리를 제외해 개발자의 로컬 작업 문서가 검사에 섞이지 않게 한다.
fn scanned_docs(root: &Path) -> Result<Vec<Walked>, String> {
    scanned_docs_under(root, root, &MD_FLOOR)
}

/// 작은 합성 트리도 같은 순회로 검증할 수 있도록 경로와 하한을 인자로 받는다.
fn scanned_docs_under(root: &Path, rel_base: &Path, floor: &Floor) -> Result<Vec<Walked>, String> {
    walk_with_floor(
        root,
        rel_base,
        floor,
        Descend::SkipBuildCachesAndDotDirs,
        &|found| found.rel.ends_with(".md"),
    )
}

/// 링크한 문서 자리에서 상대 경로를 푼다. `..` 를 문자로 접어 레포 밖으로 나가지 않게 한다.
fn resolve_rel(from_rel: &str, head: &str) -> Option<String> {
    let mut parts: Vec<&str> = from_rel.split('/').collect();
    parts.pop();
    for seg in head.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// 파일별 앵커와 원문을 보관한다. 판정은 디스크 접근 없이 이 자료로 수행한다.
struct Corpus {
    anchors_by_rel: BTreeMap<String, HashSet<String>>,
    contents_by_rel: BTreeMap<String, String>,
}

impl Corpus {
    /// 합성 입력도 실제 앵커 파서를 거친다.
    fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let mut anchors_by_rel = BTreeMap::new();
        let mut contents_by_rel = BTreeMap::new();
        for (rel, text) in pairs {
            anchors_by_rel.insert((*rel).to_owned(), anchors_of(text));
            contents_by_rel.insert((*rel).to_owned(), (*text).to_owned());
        }
        Self {
            anchors_by_rel,
            contents_by_rel,
        }
    }
}

fn corpus_of(docs: &[Walked]) -> Corpus {
    let mut anchors_by_rel = BTreeMap::new();
    let mut contents_by_rel = BTreeMap::new();
    for found in docs {
        if let Ok(text) = std::fs::read_to_string(&found.path) {
            anchors_by_rel.insert(found.rel.clone(), anchors_of(&text));
            contents_by_rel.insert(found.rel.clone(), text);
        }
    }
    Corpus {
        anchors_by_rel,
        contents_by_rel,
    }
}

struct Audit {
    intra: usize,
    cross: usize,
    violations: Vec<String>,
    violating_docs: std::collections::BTreeSet<String>,
}

/// 문서 내·문서 간 링크를 판정한다. 수집되지 않은 대상 파일은 제외한다.
fn audit(corpus: &Corpus) -> Audit {
    let mut out = Audit {
        intra: 0,
        cross: 0,
        violations: Vec::new(),
        violating_docs: std::collections::BTreeSet::new(),
    };
    for (rel, text) in &corpus.contents_by_rel {
        for (line_no, target) in links_of(text) {
            let Some((head, anchor)) = split_anchor(&target) else {
                continue;
            };
            let (target_rel, kind) = match head {
                None => (rel.clone(), "문서내"),
                Some(head) => {
                    let Some(resolved) = resolve_rel(rel, head) else {
                        continue;
                    };
                    (resolved, "크로스파일")
                }
            };
            let Some(anchors) = corpus.anchors_by_rel.get(&target_rel) else {
                // 대상 파일의 존재는 cited_coordinates_exist에서 검사한다.
                continue;
            };
            if head.is_none() {
                out.intra += 1;
            } else {
                out.cross += 1;
            }
            if !anchors.contains(anchor) {
                out.violations
                    .push(format!("  {rel}:{line_no} — [{kind}] `{target}`"));
                out.violating_docs.insert(rel.clone());
            }
        }
    }
    out
}

#[test]
fn cited_anchors_resolve_to_a_heading() {
    let root = &tasty_doc_guards::repo_root();
    let docs = scanned_docs(root).unwrap_or_else(|why| panic!("{why}"));

    let corpus = corpus_of(&docs);
    let Audit {
        intra,
        cross,
        violations,
        violating_docs,
    } = audit(&corpus);

    let judged = intra + cross;
    println!(
        "앵커 검사 범위: 문서 {} 개 · 판정 {judged} 건(문서내 {intra} · 크로스파일 {cross})",
        docs.len()
    );
    assert!(
        judged >= REFS_FLOOR,
        "앵커 참조를 {judged}건만 판정했다(문서 내 {intra}·문서 간 {cross}, 하한 {REFS_FLOOR}). 하한을 낮추기 전에 링크 추출과 수집 범위를 확인한다."
    );
    // 점 디렉터리 밖의 미추적 문서도 수집된다. 오류 시 추적 여부를 알려 잘못된 파일을 수정하지 않게 한다.
    let outside = if violations.is_empty() {
        String::new()
    } else {
        let rels: Vec<String> = violating_docs.into_iter().collect();
        tasty_doc_guards::tracked_scope::outside_repo_note(root, &rels)
    };
    assert!(
        violations.is_empty(),
        "해결되지 않는 앵커 {}건(문서 {}개·판정 {judged}건):\n{}\n제목 원문 검색만으로 판단하지 말고 마크업을 제거한 슬러그와 링크를 대조한다.{}",
        violations.len(),
        docs.len(),
        violations.join("\n"),
        outside
    );
}

#[test]
fn slug_is_a_function_of_heading_text_only() {
    assert_eq!(
        slug("미측정 구간의 **길이** — 이 문서가 가진 적 없던 축"),
        "미측정-구간의-길이--이-문서가-가진-적-없던-축"
    );
    assert_eq!(
        slug("훅이 **어느 OS 에서** 도는가 — 위 표에 없는 축"),
        "훅이-어느-os-에서-도는가--위-표에-없는-축"
    );
    assert_eq!(slug("주체 (→ [actors.md](actors.md))"), "주체--actorsmd");
}

#[test]
fn duplicate_heading_text_gets_an_ordered_suffix() {
    let anchors = anchors_of("# 같은 것\n\n## 같은 것\n\n### 같은 것\n");
    assert!(anchors.contains("같은-것"));
    assert!(anchors.contains("같은-것-1"));
    assert!(anchors.contains("같은-것-2"));
}

#[test]
fn fenced_and_inline_code_are_not_links() {
    let src = "```\n[a](#없는앵커)\n```\n\n`[b](#또없는앵커)` 는 예시다.\n";
    assert!(links_of(src).is_empty(), "{:?}", links_of(src));
}

/// 사이트 앵커 검사의 처리와 CI 호출이 모두 유지되는지 확인한다.
/// 렌더러 규칙을 복제하지 않고 생성된 HTML을 검사하는 스크립트에 맡기기 위한 조건이다.
/// 합성 문서도 anchors_of로 읽어 실제 오류 보고와 없는 대상 파일 제외를 확인한다.
#[test]
fn the_audit_reports_an_unresolved_anchor_and_skips_only_a_missing_target_file() {
    let corpus = Corpus::from_pairs(&[
        (
            "zone/guide.md",
            "# Alpha One\n\n[안으로](#alpha-one)\n[깨진 것](#no-such)\n\
             [옆으로](./other.md#beta-two)\n[없는 파일](./gone.md#whatever)\n",
        ),
        ("zone/other.md", "## Beta Two\n"),
        ("zone/rendered/a.md", "[사이트 안](./b.md#site-rule)\n"),
        ("zone/rendered/b.md", "## Site Rule\n"),
        (
            "zone/deep/into_site.md",
            "[밖에서 안으로](../rendered/b.md#site-rule)\n",
        ),
        (
            "zone/deep/into_site2.md",
            "[밖에서 안으로, 없는 앵커](../rendered/c.md#no-such-heading)\n",
        ),
        ("zone/rendered/c.md", "## Under_Score\n"),
    ]);

    let a = audit(&corpus);

    assert_eq!(
        a.violations.len(),
        2,
        "잘못된 앵커가 오류 목록에 없다: {:#?}",
        a.violations
    );
    assert!(
        a.violations.iter().any(|v| v.contains("zone/guide.md:4")
            && v.contains("문서내")
            && v.contains("#no-such")),
        "문서 내 오류의 경로·종류·대상이 진단에 없다: {:#?}",
        a.violations
    );
    assert!(
        a.violations
            .iter()
            .any(|v| v.contains("zone/deep/into_site2.md") && v.contains("크로스파일")),
        "다른 폴더의 없는 앵커가 판정에서 빠졌다: {:#?}",
        a.violations
    );
    assert_eq!(
        a.violating_docs.len(),
        2,
        "위반 문서 집합이 다르다: {:?}",
        a.violating_docs
    );

    // 대상 파일이 없는 링크만 제외된다. 문서 내·문서 간 어느 쪽에도 세지 않는다.
    assert_eq!(
        (a.intra, a.cross),
        (2, 4),
        "문서 내·문서 간 판정 수가 다르다. 대상 파일 제외 조건과 상대 경로 처리를 확인한다."
    );
}

#[test]
fn the_walk_skips_dot_directories_and_takes_only_markdown() {
    // 이유: 이 고유 접두어를 쓰는 테스트는 프로세스당 한 번만 실행한다.
    // PID로 동시 프로세스를 구분하고, 재사용된 PID의 이전 임시 경로는 생성 전에 정리한다.
    let root = std::env::temp_dir().join(format!(
        "tasty-cited-anchors-fixture-{}",
        std::process::id()
    ));
    // 이전 실행 잔여물 제거 — 없으면 `NotFound` 라 실패가 정상 경로다.
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("docs")).expect("합성 트리를 만들지 못했다");
    std::fs::create_dir_all(root.join(".hidden-work")).expect("합성 트리를 만들지 못했다");
    let write = |rel: &str, body: &str| {
        std::fs::write(root.join(rel), body).unwrap_or_else(|e| panic!("{rel}: {e}"));
    };
    write("top.md", "# Top\n");
    write("docs/one.md", "# One\n");
    write("docs/two.md", "# Two\n");
    write("docs/not-markdown.txt", "# Two\n");
    write(".hidden-work/note.md", "[깨진 것](#없는-제목)\n");

    // 작은 합성 트리에는 실제 저장소의 문서 수 하한을 적용할 수 없다.
    let floor = Floor {
        min: 2,
        measured: 3,
        measured_on: "2026-09-08",
        counted_on: tasty_doc_guards::floored_walk::CountedOn::SyntheticTree,
        why_this_gap: "이 합성 트리의 `.md` 수다. 새 조건을 시험하려고 파일을 더하는 것은 \
                       정상 변경이라 하한을 실제 개수에 맞추면 그 변경마다 실패한다.",
    };
    let found = scanned_docs_under(&root, &root, &floor).expect("합성 트리 순회가 하한에 걸렸다");
    let mut rels: Vec<String> = found.iter().map(|f| f.rel.clone()).collect();
    rels.sort();
    assert_eq!(
        rels,
        vec![
            "docs/one.md".to_owned(),
            "docs/two.md".to_owned(),
            "top.md".to_owned()
        ],
        "순회가 집은 것이 다르다 — 점 디렉토리를 내려갔거나 `.md` 아닌 것을 집었다"
    );

    // 이유: 검사 뒤의 임시 경로 정리는 결과 판정에 영향을 주지 않는다.
    let _ = std::fs::remove_dir_all(&root);
}
