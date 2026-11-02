//! 사이트의 SVG 파일·Icon.jsx와 앱의 아이콘 사본을 대조한다.
//! 사이트는 SVG 파일 대신 Icon.jsx의 ICON_PATHS를 그리므로 두 사이트 사본도 서로 비교해야 한다.
//! 원격 디자인은 조회하지 않는다. 사본끼리 맞아도 최신 원본이라는 뜻은 아니다.
//!
//! 이름만으로 대응시킬 수 없어 PAIRS를 사용한다. 사이트 list는 앱 log에, listView는 앱 list에 대응한다.
//! 같은 경로 데이터를 가진 star와 starFill도 있어 채움 여부를 별도로 비교한다.
//!
//! 문자열을 정규화해 내부 마크업과 여는 SVG 태그의 속성을 대조한다. 실제 화면을 렌더하지 않으므로
//! 픽셀 일치나 사용 위치의 적절성을 보장하지 않는다. 표기만 달라진 경우에도 불일치가 날 수 있다.
//! viewBox·선 굵기·끝·연결 속성은 같아야 하고, 색은 사이트의 currentColor와 앱의 white+tint 차이를 허용한다.
//! xmlns는 SVG 문서 두 사본에만 있다. 새 속성과 더 이상 필요 없는 예외도 검사한다.
//! 사본을 갱신할 때는 site/vendor/README.md의 절차를 따른다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use tasty_doc_guards::floored_walk::{CountedOn, Descend, Floor, Walked, walk_with_floor};

/// 사이트 이름과 앱 이름. 이름이 겹치지만 다른 아이콘인 경우가 있어 자동 변환하지 않는다.
const PAIRS: &[(&str, &str)] = &[
    ("plus", "plus"),
    ("close", "close"),
    ("refresh", "refresh"),
    ("edit", "edit"),
    ("trash", "trash"),
    ("copy", "copy"),
    ("check", "check"),
    ("search", "search"),
    ("filter", "funnel"),
    ("swap", "swap"),
    ("more", "more"),
    ("download", "download"),
    ("star", "star"),
    ("starFill", "star_fill"),
    ("chevronRight", "chevron_right"),
    ("chevronDown", "chevron_down"),
    ("chevronUp", "chevron_up"),
    ("chevronLeft", "chevron_left"),
    ("chevronsLeft", "chevrons_left"),
    ("chevronsRight", "chevrons_right"),
    ("arrowRight", "arrow_right"),
    ("move", "move"),
    ("fit", "fit"),
    ("terminal", "terminal"),
    ("agent", "agent"),
    ("markdown", "markdown"),
    ("html", "html"),
    ("split", "split"),
    ("paneEmpty", "pane_empty"),
    ("splitH", "split_h"),
    ("folder", "folder"),
    ("folderOpen", "folder_open"),
    ("file", "file"),
    ("folderPlus", "folder_plus"),
    ("filePlus", "file_plus"),
    ("image", "image"),
    ("list", "log"),
    ("layoutGrid", "layout_grid"),
    ("layoutDetail", "layout_detail"),
    ("listView", "list"),
    ("layers", "layers"),
    ("columns", "columns"),
    ("clipboard", "clipboard"),
    ("textLeft", "text_left"),
    ("scriptFile", "script"),
    ("remote", "remote"),
    ("port", "port"),
    ("gitBranch", "git_branch"),
    ("gitTree", "git_tree"),
    ("eye", "eye"),
    ("eyeOff", "eye_off"),
    ("lock", "lock"),
    ("globe", "globe"),
    ("globeOff", "globe_off"),
    ("alertTriangle", "alert_triangle"),
    ("alertCircle", "alert_circle"),
    ("helpCircle", "help_circle"),
    ("info", "info"),
    ("link", "link"),
    ("shieldCheck", "shield_check"),
    ("bell", "bell"),
    ("tools", "tools"),
    ("settings", "settings"),
    ("plug", "plug"),
    ("rocket", "rocket"),
    ("command", "command"),
    ("theme", "theme"),
    ("keyboard", "keyboard"),
    ("mouse", "mouse"),
    ("sun", "sun"),
    ("hash", "hash"),
    ("cmdKey", "cmd_key"),
    ("optionKey", "option_key"),
    ("shiftKey", "shift_key"),
];

/// 앱에만 있는 아이콘. 사이트 사본에도 추가되면 PAIRS로 옮긴다.
// external은 공용 site Icon registry 대신 Explorer kit의 GIcon 원형을 쓴다. 아래에서 별도 대조한다.
const APP_ONLY: &[&str] = &["arrow_down", "external", "minus", "redo", "undo"];

/// 채운 글리프 — 사이트 `FILL_GLYPHS` 와 앱 `fill_icon!` 이 같은 집합을 가리켜야 한다.
/// 사이트 이름으로 적는다.
const FILLED: &[&str] = &["starFill"];

const VENDOR_ICON_DIR: &str = "site/vendor/icons";
const VENDOR_REGISTRY: &str = "site/vendor/components/core/Icon.jsx";
const APP_ICONS: &str = "crates/tasty-icons/src/lib.rs";

/// 빈 파싱 결과끼리 같다고 통과하지 않도록 세 사본에 같은 개수 하한을 적용한다.
const PARSE_FLOOR: usize = VENDOR_ICON_FLOOR.min;

/// 정확한 아이콘 집합은 별도로 비교하므로 이 하한에는 수집 실패를 찾을 여유를 둔다.
const VENDOR_ICON_FLOOR: Floor = Floor {
    min: 60,
    measured: 65,
    measured_on: "2026-09-20",
    counted_on: CountedOn::Tree(
        "9d1b15669 — site/vendor/icons의 추적 SVG 65개를 2026-09-24에 재확인했다. 기존 측정값과 같다.",
    ),
    why_this_gap: "정확한 아이콘 집합은 PAIRS와 APP_ONLY로 비교한다. 이 하한은 그 비교가 빈 수집 결과끼리 통과하지 않도록 하며 정상적인 개수 변화에는 여유를 둔다.",
};

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 를 못 읽었다: {e}", path.display()))
}

/// 비교용으로 자기닫기 태그를 통일하고 공백을 정리한다. 속성 안의 공백은 한 칸으로 합친다.
fn normalize(body: &str) -> String {
    let mut s = body.to_string();
    for tag in [
        "circle", "path", "rect", "line", "polyline", "polygon", "ellipse",
    ] {
        s = s.replace(&format!("></{tag}>"), "/>");
    }
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for ch in s.chars() {
        if ch.is_ascii_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    let out = out.replace("> <", "><");
    out.trim().to_string()
}

/// SVG를 한 번 수집해 내부 마크업과 여는 태그 속성 검사에서 함께 쓴다.
fn vendor_svg_sources() -> BTreeMap<String, String> {
    let root = repo_root();
    let walked = walk_with_floor(
        &root.join(VENDOR_ICON_DIR),
        &root,
        &VENDOR_ICON_FLOOR,
        Descend::Everything,
        &|w: &Walked| w.rel.ends_with(".svg"),
    )
    .unwrap_or_else(|why| panic!("{VENDOR_ICON_DIR} 순회: {why}"));

    let mut map = BTreeMap::new();
    for w in walked {
        let path = w.path;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("파일 이름")
            .to_string();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} 를 못 읽었다: {e}", path.display()));
        map.insert(name, text);
    }
    map
}

/// `site/vendor/icons/<name>.svg` → 이름 → inner 마크업.
fn vendor_files() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (name, text) in vendor_svg_sources() {
        let open_end = text
            .find('>')
            .unwrap_or_else(|| panic!("{name}.svg: `<svg …>` 를 못 찾았다"));
        let close = text
            .rfind("</svg>")
            .unwrap_or_else(|| panic!("{name}.svg: `</svg>` 를 못 찾았다"));
        map.insert(name, normalize(&text[open_end + 1..close]));
    }
    map
}

/// ICON_PATHS의 항목이 한 줄짜리 작은따옴표 문자열이라고 가정해 읽는다.
fn vendor_registry(src: &str) -> BTreeMap<String, String> {
    let start = src
        .find("export const ICON_PATHS = {")
        .expect("Icon.jsx: ICON_PATHS 선언을 못 찾았다");
    let rest = &src[start..];
    let end = rest
        .find("\n};")
        .expect("Icon.jsx: ICON_PATHS 의 끝을 못 찾았다");
    let mut map = BTreeMap::new();
    for line in rest[..end].lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let Some((name, rhs)) = line.split_once(':') else {
            continue;
        };
        let Some(open) = rhs.find('\'') else { continue };
        let Some(close) = rhs.rfind('\'') else {
            continue;
        };
        if close <= open {
            continue;
        }
        map.insert(name.trim().to_string(), normalize(&rhs[open + 1..close]));
    }
    map
}

/// `Icon.jsx` 의 `FILL_GLYPHS` 집합.
fn vendor_filled(src: &str) -> BTreeSet<String> {
    let start = src
        .find("FILL_GLYPHS = new Set([")
        .expect("Icon.jsx: FILL_GLYPHS 선언을 못 찾았다");
    let rest = &src[start..];
    let end = rest
        .find("])")
        .expect("Icon.jsx: FILL_GLYPHS 의 끝을 못 찾았다");
    let mut set = BTreeSet::new();
    let mut chunk = &rest[..end];
    while let Some(open) = chunk.find('"') {
        let after = &chunk[open + 1..];
        let Some(close) = after.find('"') else { break };
        set.insert(after[..close].to_string());
        chunk = &after[close + 1..];
    }
    set
}

/// 앱 전사본 → 이름 → (inner 마크업, 채움 여부).
///
/// `stroke_icon!(NAME, "uri", r#"…"#)` / `fill_icon!(…)` 두 형태를 읽는다. 매크로 호출이
/// 여러 줄에 걸쳐 있을 수 있어 줄 단위로는 못 읽고, 여는 자리부터 손으로 훑는다.
fn app_icons(src: &str) -> BTreeMap<String, (String, bool)> {
    let mut map = BTreeMap::new();
    for (macro_name, filled) in [("stroke_icon!(", false), ("fill_icon!(", true)] {
        let mut from = 0usize;
        while let Some(hit) = src[from..].find(macro_name) {
            let at = from + hit + macro_name.len();
            from = at;
            // 매크로 호출의 URI 문자열이 없는 형식은 건너뛴다.
            let Some(uri_open) = src[at..].find('"') else {
                continue;
            };
            let uri_start = at + uri_open + 1;
            let Some(uri_len) = src[uri_start..].find('"') else {
                continue;
            };
            let uri = &src[uri_start..uri_start + uri_len];
            let tail = &src[uri_start + uri_len..];
            let Some(body_open) = tail.find("r#\"") else {
                continue;
            };
            let body_start = body_open + 3;
            let Some(body_len) = tail[body_start..].find("\"#") else {
                continue;
            };
            let body = &tail[body_start..body_start + body_len];
            map.insert(uri.to_string(), (normalize(body), filled));
        }
    }
    map
}

/// 세 사본에서 같아야 하는 여는 태그 속성. SVG와 JSX의 속성 이름을 대응시킨다.
const SHARED_ENVELOPE: &[(&str, &str)] = &[
    ("viewBox", "viewBox"),
    ("stroke-width", "strokeWidth"),
    ("stroke-linecap", "strokeLinecap"),
    ("stroke-linejoin", "strokeLinejoin"),
];

/// SVG 파일과 앱의 SVG 문서에만 필요한 속성. JSX 인라인 태그에는 없어야 한다.
const DOCUMENT_ONLY_ENVELOPE: &[(&str, &str)] = &[(
    "xmlns",
    "독립 문서에만 필요한 네임스페이스 선언이다. `.svg` 파일은 파일로 열리고 앱 접두는 \
     `egui_extras` 로더와 `usvg` 가 문서로 읽는다. JSX 는 DOM 에 인라인되므로 브라우저가 \
     부모에서 상속한다",
)];

const DIVERGENT_ENVELOPE: &[(&str, &str)] = &[
    (
        "stroke",
        "사이트는 currentColor를 상속하고 앱은 white로 그린 뒤 egui tint로 색을 입힌다. 색 값 자체는 비교하지 않는다.",
    ),
    (
        "fill",
        "사이트와 앱의 색 적용 방식이 다르다. 채움 여부는 FILLED와 FILL_GLYPHS로 별도 비교한다.",
    ),
];

/// 여는 태그의 큰따옴표 속성 값을 읽는다. JSX 식은 값 대신 JSX_EXPR로 표시한다.
fn open_tag_attrs(tag: &str) -> BTreeMap<String, String> {
    let b = tag.as_bytes();
    let mut out = BTreeMap::new();
    let mut i = 0usize;
    while i < b.len() {
        if !(b[i].is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b':') {
            i += 1;
        }
        let name = &tag[start..i];
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() || b[j] != b'=' {
            continue;
        }
        j += 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < b.len() && b[j] == b'"' {
            let vs = j + 1;
            let Some(len) = tag[vs..].find('"') else {
                break;
            };
            out.insert(name.to_string(), tag[vs..vs + len].to_string());
            i = vs + len + 1;
        } else if j < b.len() && b[j] == b'{' {
            let mut depth = 0usize;
            let mut k = j;
            while k < b.len() {
                match b[k] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            out.insert(name.to_string(), JSX_EXPR.to_string());
            i = (k + 1).min(b.len());
        }
    }
    out
}

/// JSX 식이 있음을 나타내는 표지. 식의 내용은 비교하지 않는다.
const JSX_EXPR: &str = "{식}";

/// 큰따옴표 밖의 >까지 여는 SVG 태그를 읽는다.
fn svg_open_tag(src: &str, what: &str) -> String {
    let at = src
        .find("<svg")
        .unwrap_or_else(|| panic!("{what}: `<svg` 를 못 찾았다"));
    let rest = &src[at..];
    let b = rest.as_bytes();
    let mut i = 0usize;
    let mut in_quote = false;
    while i < b.len() {
        match b[i] {
            b'"' => in_quote = !in_quote,
            b'>' if !in_quote => return rest[..i].to_string(),
            _ => {}
        }
        i += 1;
    }
    panic!("{what}: 여는 `<svg …>` 태그가 안 닫힌다");
}

fn vendor_svg_envelopes() -> BTreeMap<String, BTreeMap<String, String>> {
    vendor_svg_sources()
        .into_iter()
        .map(|(name, text)| {
            let tag = svg_open_tag(&text, &format!("{name}.svg"));
            (name, open_tag_attrs(&tag))
        })
        .collect()
}

/// 사이트가 실제로 그리는 Icon.jsx의 여는 태그.
fn registry_envelope(src: &str) -> BTreeMap<String, String> {
    let at = src
        .find("export function Icon(")
        .expect("Icon.jsx: `export function Icon(` 을 못 찾았다");
    open_tag_attrs(&svg_open_tag(&src[at..], "Icon.jsx 의 컴포넌트"))
}

/// 앱 전사본 — 두 매크로가 붙이는 접두 `<svg …>`. 키는 채움 여부다.
fn app_envelopes(src: &str) -> BTreeMap<bool, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for (macro_def, filled) in [
        ("macro_rules! stroke_icon", false),
        ("macro_rules! fill_icon", true),
    ] {
        let at = src
            .find(macro_def)
            .unwrap_or_else(|| panic!("{APP_ICONS}: `{macro_def}` 을 못 찾았다"));
        let tag = svg_open_tag(&src[at..], macro_def);
        out.insert(filled, open_tag_attrs(&tag));
    }
    out
}

#[test]
fn the_envelope_readers_see_all_three_copies() {
    let files = vendor_svg_envelopes();
    assert!(
        files.len() >= PARSE_FLOOR,
        "`.svg` 를 {} 개만 읽었다 (바닥 {PARSE_FLOOR})",
        files.len()
    );
    let registry = registry_envelope(&read(VENDOR_REGISTRY));
    let app = app_envelopes(&read(APP_ICONS));
    assert_eq!(app.len(), 2, "앱 매크로 접두를 둘 다 못 읽었다: {app:?}");

    // 개수뿐 아니라 각 사본에서 공통 속성을 실제로 읽었는지도 확인한다.
    let (svg_axis, jsx_axis) = SHARED_ENVELOPE[0];
    for (what, attrs) in [
        (
            format!("{VENDOR_ICON_DIR} 의 첫 파일"),
            files.values().next().expect("파일 하나").clone(),
        ),
        (VENDOR_REGISTRY.to_string(), registry),
    ] {
        let axis = if what == VENDOR_REGISTRY {
            jsx_axis
        } else {
            svg_axis
        };
        assert!(
            attrs.contains_key(axis),
            "{what}의 여는 태그에서 {axis}를 읽지 못했다: {attrs:?}"
        );
    }
    for (filled, attrs) in &app {
        assert!(
            attrs.contains_key(svg_axis),
            "앱 매크로(채움={filled})의 접두에서 `{svg_axis}` 를 못 읽었다: {attrs:?}"
        );
    }
}

/// 값이 같아진 속성을 차이 허용 목록에 남기면 이후 차이를 놓칠 수 있다.
#[test]
fn the_divergence_roster_has_no_dead_weight() {
    let files = vendor_svg_envelopes();
    let app = app_envelopes(&read(APP_ICONS));
    let mut seen = BTreeSet::new();
    for (axis, why) in DIVERGENT_ENVELOPE {
        assert!(seen.insert(*axis), "`{axis}` 가 명부에 두 번 있다");
        assert!(!why.trim().is_empty(), "`{axis}` 에 사유가 없다");
        let mut values: BTreeSet<Option<&str>> = BTreeSet::new();
        for attrs in files.values().chain(app.values()) {
            values.insert(attrs.get(*axis).map(String::as_str));
        }
        assert!(
            values.len() > 1,
            "{axis}는 SVG 사본 사이에 값 차이가 없다({values:?}). 차이 허용 목록에서 SHARED_ENVELOPE로 옮긴다."
        );
    }
    for (svg_axis, _) in SHARED_ENVELOPE {
        assert!(
            !seen.contains(svg_axis),
            "`{svg_axis}` 가 두 명부에 다 있다 — 같아야 하면서 달라도 된다는 뜻이 된다"
        );
    }

    let registry = registry_envelope(&read(VENDOR_REGISTRY));
    for (axis, why) in DOCUMENT_ONLY_ENVELOPE {
        assert!(!why.trim().is_empty(), "`{axis}` 에 사유가 없다");
        assert!(
            !seen.contains(axis),
            "`{axis}` 가 갈려도 되는 축이면서 문서 전용 축이다 — 하나만 골라라"
        );
        let values: BTreeSet<Option<&str>> = files
            .values()
            .chain(app.values())
            .map(|a| a.get(*axis).map(String::as_str))
            .collect();
        assert_eq!(
            values.len(),
            1,
            "`{axis}` 가 두 문서 사본에서 갈린다: {values:?}"
        );
        assert!(
            values.iter().all(Option::is_some),
            "`{axis}` 가 일부 문서 사본에 없다 — 문서 전용 축이 아니다"
        );
        assert!(
            !registry.contains_key(*axis),
            "`{axis}` 가 {VENDOR_REGISTRY} 의 인라인 엘리먼트에도 생겼다 — 문서 전용이 \
             아니게 됐으므로 `SHARED_ENVELOPE` 로 옮기고 JSX 이름을 짝지어라"
        );
    }
}

/// SVG 문서에 추가된 속성은 같은 값·차이 허용·문서 전용 중 하나로 분류해야 한다.
#[test]
fn every_envelope_attribute_is_classified() {
    let classified: BTreeSet<&str> = SHARED_ENVELOPE
        .iter()
        .map(|(svg, _)| *svg)
        .chain(DOCUMENT_ONLY_ENVELOPE.iter().map(|(a, _)| *a))
        .chain(DIVERGENT_ENVELOPE.iter().map(|(a, _)| *a))
        .collect();

    let mut unclassified: BTreeSet<String> = BTreeSet::new();
    let mut missing: Vec<String> = Vec::new();
    let files = vendor_svg_envelopes();
    let app = app_envelopes(&read(APP_ICONS));

    for (what, attrs) in files
        .iter()
        .map(|(n, a)| (format!("{VENDOR_ICON_DIR}/{n}.svg"), a))
        .chain(
            app.iter()
                .map(|(f, a)| (format!("{APP_ICONS} 의 매크로(채움={f})"), a)),
        )
    {
        for name in attrs.keys() {
            if !classified.contains(name.as_str()) {
                unclassified.insert(format!("{name} ({what})"));
            }
        }
        for (svg_axis, _) in SHARED_ENVELOPE {
            if !attrs.contains_key(*svg_axis) {
                missing.push(format!("{svg_axis} 가 {what} 에 없다"));
            }
        }
    }

    assert!(
        unclassified.is_empty(),
        "여는 태그에 분류 안 된 속성이 있다: {unclassified:?}. 세 사본이 같아야 하면 \
         `SHARED_ENVELOPE` 에, 갈려도 되면 사유와 함께 `DIVERGENT_ENVELOPE` 에 넣어라"
    );
    assert!(
        missing.is_empty(),
        "공유 축이 빠진 자리가 있다 — 빠진 축은 브라우저·usvg 기본값으로 떨어져 \
         사본마다 다르게 그려진다: {missing:?}"
    );
}

/// 내부 마크업과 별개로 여는 SVG 태그의 속성을 비교한다.
#[test]
fn the_three_copies_share_one_render_envelope() {
    let files = vendor_svg_envelopes();
    let registry = registry_envelope(&read(VENDOR_REGISTRY));
    let app = app_envelopes(&read(APP_ICONS));

    let mut problems: Vec<String> = Vec::new();
    for (svg_axis, jsx_axis) in SHARED_ENVELOPE {
        let mut by_value: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, attrs) in &files {
            if let Some(v) = attrs.get(*svg_axis) {
                by_value
                    .entry(v.clone())
                    .or_default()
                    .push(format!("{name}.svg"));
            }
        }
        for (filled, attrs) in &app {
            if let Some(v) = attrs.get(*svg_axis) {
                by_value
                    .entry(v.clone())
                    .or_default()
                    .push(format!("앱 매크로(채움={filled})"));
            }
        }
        if let Some(v) = registry.get(*jsx_axis) {
            by_value
                .entry(v.clone())
                .or_default()
                .push(VENDOR_REGISTRY.to_string());
        } else {
            problems.push(format!(
                "`{jsx_axis}` 가 {VENDOR_REGISTRY} 의 여는 태그에 없다 — 사이트가 그리는 것은 \
                 이쪽이라, 여기서 빠지면 `.svg` 가 맞아도 화면이 다르다"
            ));
        }

        if by_value.len() > 1 {
            let spread: Vec<String> = by_value
                .iter()
                .map(|(v, wheres)| {
                    let head: Vec<&str> = wheres.iter().take(3).map(String::as_str).collect();
                    let more = wheres.len().saturating_sub(head.len());
                    if more > 0 {
                        format!("{v:?} ← {} 외 {more} 자리", head.join(" · "))
                    } else {
                        format!("{v:?} ← {}", head.join(" · "))
                    }
                })
                .collect();
            problems.push(format!("[{svg_axis}] {}", spread.join(" / ")));
        }
    }

    assert!(
        problems.is_empty(),
        "사이트와 앱 사본의 SVG 속성이 다르다. 내부 마크업만 같아도 이 속성 때문에 화면은 달라질 수 있다. 원격 디자인과 비교해 site/vendor/README.md의 절차로 사이트·앱 사본을 갱신한다:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn the_judge_actually_parses_all_three_copies() {
    let files = vendor_files();
    let registry = vendor_registry(&read(VENDOR_REGISTRY));
    let app = app_icons(&read(APP_ICONS));
    for (what, n) in [
        ("site/vendor/icons/*.svg", files.len()),
        ("Icon.jsx ICON_PATHS", registry.len()),
        ("crates/tasty-icons", app.len()),
    ] {
        assert!(
            n >= PARSE_FLOOR,
            "{what}에서 {n}개만 읽었다(하한 {PARSE_FLOOR}). 소스 형식과 파싱 범위를 확인한다."
        );
    }
    assert!(
        !vendor_filled(&read(VENDOR_REGISTRY)).is_empty(),
        "FILL_GLYPHS 를 빈 집합으로 읽었다 — 채움 축이 안 재진다"
    );
}

#[test]
fn every_roster_entry_names_a_real_glyph() {
    let files = vendor_files();
    let registry = vendor_registry(&read(VENDOR_REGISTRY));
    let app = app_icons(&read(APP_ICONS));

    let mut seen_site = BTreeSet::new();
    let mut seen_app = BTreeSet::new();
    for (site, app_name) in PAIRS {
        assert!(
            seen_site.insert(*site),
            "명부에 사이트 이름 {site} 가 두 번 있다"
        );
        assert!(
            seen_app.insert(*app_name),
            "명부에 앱 이름 {app_name} 가 두 번 있다"
        );
        assert!(
            files.contains_key(*site),
            "명부의 {site} 에 대응하는 {VENDOR_ICON_DIR}/{site}.svg 가 없다"
        );
        assert!(
            registry.contains_key(*site),
            "명부의 {site} 가 Icon.jsx ICON_PATHS 에 없다"
        );
        assert!(
            app.contains_key(*app_name),
            "명부의 앱 이름 {app_name} 가 {APP_ICONS} 에 없다"
        );
    }
    for name in APP_ONLY {
        assert!(
            app.contains_key(*name),
            "APP_ONLY 의 {name} 가 {APP_ICONS} 에 없다"
        );
        assert!(
            !seen_app.contains(name),
            "{name} 가 APP_ONLY 와 PAIRS 양쪽에 있다"
        );
    }
    for name in FILLED {
        assert!(
            seen_site.contains(name),
            "FILLED 의 {name} 가 PAIRS 에 없다"
        );
    }
}

#[test]
fn the_two_vendor_copies_carry_the_same_geometry() {
    let files = vendor_files();
    let registry = vendor_registry(&read(VENDOR_REGISTRY));

    let only_files: Vec<_> = files
        .keys()
        .filter(|k| !registry.contains_key(*k))
        .collect();
    let only_registry: Vec<_> = registry
        .keys()
        .filter(|k| !files.contains_key(*k))
        .collect();
    assert!(
        only_files.is_empty() && only_registry.is_empty(),
        "사이트 사본 둘의 글리프 **이름**이 어긋난다 — 파일에만: {only_files:?} · \
         ICON_PATHS 에만: {only_registry:?}"
    );

    let mismatched: Vec<_> = files
        .iter()
        .filter(|(name, body)| registry.get(*name) != Some(*body))
        .map(|(name, body)| {
            format!(
                "\n  {name}\n    icons/{name}.svg  {body}\n    ICON_PATHS        {}",
                registry.get(name).map(String::as_str).unwrap_or("<없음>")
            )
        })
        .collect();
    assert!(
        mismatched.is_empty(),
        "사이트 SVG와 ICON_PATHS의 내부 마크업이 다르다. 사이트는 ICON_PATHS를 그리므로 두 사본을 함께 확인한다:{}",
        mismatched.join("")
    );
}

#[test]
fn the_app_transcription_matches_the_vendored_kit() {
    let registry = vendor_registry(&read(VENDOR_REGISTRY));
    let app = app_icons(&read(APP_ICONS));

    let mismatched: Vec<_> = PAIRS
        .iter()
        .filter_map(|(site, app_name)| {
            let want = registry.get(*site)?;
            let (got, _) = app.get(*app_name)?;
            (want != got).then(|| format!("\n  {site} ↔ {app_name}\n    킷  {want}\n    앱  {got}"))
        })
        .collect();
    assert!(
        mismatched.is_empty(),
        "앱과 사이트 사본의 내부 마크업이 다르다. 어느 쪽이 최신인지 원격 디자인과 대조하고 site/vendor/README.md의 절차에 따라 갱신한다:{}",
        mismatched.join("")
    );

    let site_filled = vendor_filled(&read(VENDOR_REGISTRY));
    let want_filled: BTreeSet<String> = FILLED.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        site_filled, want_filled,
        "사이트의 FILL_GLYPHS 가 명부와 다르다"
    );
    let app_filled: BTreeSet<String> = PAIRS
        .iter()
        .filter(|(_, a)| app.get(*a).map(|(_, f)| *f).unwrap_or(false))
        .map(|(s, _)| (*s).to_string())
        .collect();
    assert_eq!(
        app_filled, want_filled,
        "앱 fill_icon과 사이트 FILL_GLYPHS가 다르다. 같은 경로 데이터라도 채움 여부는 같아야 한다."
    );
}

#[test]
fn the_app_has_exactly_the_recorded_extra_glyphs() {
    let app = app_icons(&read(APP_ICONS));
    let paired: BTreeSet<&str> = PAIRS.iter().map(|(_, a)| *a).collect();
    let extra: BTreeSet<&str> = app
        .keys()
        .map(String::as_str)
        .filter(|k| !paired.contains(k))
        .collect();
    let recorded: BTreeSet<&str> = APP_ONLY.iter().copied().collect();
    assert_eq!(
        extra, recorded,
        "앱 전용 아이콘 목록이 달라졌다. 새 아이콘의 디자인 근거를 확인하고, 사이트 사본에도 생긴 아이콘은 PAIRS로 옮긴다."
    );
}

#[test]
fn every_site_glyph_is_paired_with_the_app() {
    let files = vendor_files();
    let registry = vendor_registry(&read(VENDOR_REGISTRY));
    let paired: BTreeSet<&str> = PAIRS.iter().map(|(s, _)| *s).collect();
    let unpaired: BTreeSet<&str> = files
        .keys()
        .chain(registry.keys())
        .map(String::as_str)
        .filter(|k| !paired.contains(k))
        .collect();
    assert!(
        unpaired.is_empty(),
        "사이트 사본에만 있는 아이콘이 있다: {unpaired:?}. 앱에 전사하고 PAIRS에 추가한다. 앱의 같은 이름 아이콘이 APP_ONLY에 있으면 그 항목도 옮긴다."
    );
}

#[test]
fn external_matches_the_explorer_kit_glyph() {
    let kit = read("site/vendor/gallery/plugins.jsx");
    let row = kit
        .lines()
        .find(|line| line.trim_start().starts_with("external:"))
        .unwrap();
    let begin = row.find("<path ").unwrap();
    let end = row[begin..].find("/>").unwrap() + begin + 2;
    let icons = app_icons(&read(APP_ICONS));
    let (body, filled) = &icons["external"];
    // JSX의 자기닫기 태그 앞 공백은 도형 차이가 아니다.
    assert_eq!(
        normalize(body).replace(" />", "/>"),
        normalize(&row[begin..end]).replace(" />", "/>")
    );
    assert!(!filled);
    let shell = read("site/vendor/gallery/shell.jsx");
    let gicon = shell
        .split("function GIcon(")
        .nth(1)
        .unwrap()
        .split("\n}")
        .next()
        .unwrap();
    assert!(gicon.contains("strokeWidth=\"2\""));
    assert!(gicon.contains("viewBox=\"0 0 24 24\""));
    assert!(gicon.contains("strokeLinecap=\"round\""));
    assert!(gicon.contains("strokeLinejoin=\"round\""));
}
