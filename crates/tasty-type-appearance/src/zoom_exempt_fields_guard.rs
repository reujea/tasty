//! Theme의 LogicalPx 필드 중 UI 배율을 적용하지 않는 이름을 EXEMPT와 대조한다.
//! 새 면제가 이름 없이 늘거나 기존 항목이 빠지는 것을 검사한다.
//! 필드 설명의 사유 표지도 대조하지만 그 사유가 디자인에 적합한지 판정하지는 않는다.
//! 파일 수집·초기화식 파싱이 비어 있는 경우는 하한과 양쪽 분류 검사로 거부한다.
//! 소스 경로는 컴파일 시점의 CARGO_MANIFEST_DIR를 기준으로 한다.

use std::fs;
use std::path::PathBuf;

/// 수집한 전체 LogicalPx 필드의 최소 수. 면제 건수와는 다르다.
const FIELD_FLOOR: usize = 40;

fn theme_source() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("theme.rs");
    fs::read_to_string(p).unwrap_or_default()
}

/// 배율을 적용하지 않는 사유. 실제 소비 위치만으로 분류할 수 없어 필드 설명과 표지를 대조한다.
/// 배율을 곱했을 때 값이 달라지는지는 별도의 반올림 계산으로 검사한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    /// UI 배율과 별도로 표시하는 콘텐츠 글꼴.
    Content,
    /// 고정 선 굵기.
    Hairline,
    /// 고정 치수를 사용하는 탭바.
    TabBar,
    /// 고정 치수를 사용하는 상태바.
    StatusBar,
    /// OS 창 장식과 맞추는 타이틀바.
    Titlebar,
}

/// 각 사유에 대응하는 필드 설명 표지.
impl Reason {
    /// 이 갈래의 필드 doc 에 반드시 들어 있어야 하는 표지.
    fn marker(self) -> &'static str {
        match self {
            Reason::Content => "콘텐츠",
            Reason::Hairline => "hairline",
            Reason::TabBar => "탭바 크롬",
            Reason::StatusBar => "상태바 크롬",
            Reason::Titlebar => "CSD 타이틀바 크롬",
        }
    }
}

const EXEMPT: &[(&str, Reason)] = &[
    ("font_size_prose_h1", Reason::Content),
    ("font_size_term_sm", Reason::Content),
    ("font_size_term", Reason::Content),
    ("font_size_term_lg", Reason::Content),
    ("border_width", Reason::Hairline),
    ("icon_stroke_width", Reason::Hairline),
    ("selection_edge_width", Reason::Hairline),
    ("tint_edge_width", Reason::Hairline),
    ("border_dash", Reason::Hairline),
    ("border_dash_gap", Reason::Hairline),
    ("status_dot_ring_width", Reason::Hairline),
    ("tab_width", Reason::TabBar),
    ("tab_bar_height", Reason::TabBar),
    ("tab_bar_label_font_size", Reason::TabBar),
    ("status_bar_height", Reason::StatusBar),
    ("titlebar_height", Reason::Titlebar),
    ("traffic_size", Reason::Titlebar),
    ("caption_width", Reason::Titlebar),
    ("window_button_size", Reason::Titlebar),
];

/// 필드 하나의 실측 결과: (이름, `zoomed()` 를 타는가).
type Field = (String, bool);

/// `Theme` 구조체 본문의 `LogicalPx` 필드를 생성자 초기화식과 짝지어 읽는다.
fn scan_fields(text: &str) -> Vec<Field> {
    let lines: Vec<&str> = text.lines().collect();
    let struct_head = concat!("pub struct ", "Theme {");
    let Some(s) = lines.iter().position(|l| l.starts_with(struct_head)) else {
        return Vec::new();
    };
    let Some(e) = lines
        .iter()
        .skip(s + 1)
        .position(|l| *l == "}")
        .map(|i| i + s + 1)
    else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for line in &lines[s + 1..e] {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("pub ")
            && let Some(name) = rest.strip_suffix(": LogicalPx,")
            && !name.contains(' ')
        {
            names.push(name.to_string());
        }
    }

    // 생성자의 `Self { .. }` 는 zoom 클로저 정의 직후에 온다.
    let ctor_anchor = concat!("let zoo", "med = |px: LogicalPx|");
    let Some(c) = lines.iter().position(|l| l.contains(ctor_anchor)) else {
        return Vec::new();
    };

    let zoom_call = concat!("zoo", "med(");
    let mut out = Vec::new();
    for name in names {
        let head = format!("{name}: ");
        let init = lines[c..]
            .iter()
            .find(|l| l.trim_start().starts_with(&head))
            .map(|l| l.trim_start().to_string());
        match init {
            // 초기화식을 못 읽은 항목은 누락시켜 수집·집합 검사가 실패하도록 한다.
            None => continue,
            Some(body) => {
                let zoomed = body.contains(zoom_call);
                out.push((name, zoomed));
            }
        }
    }
    out
}

/// `SIZING` 상수 블록에서 필드별 zoom 1 값을 읽는다.
fn scan_sizing_values(text: &str) -> Vec<(String, f32)> {
    let lines: Vec<&str> = text.lines().collect();
    let head = concat!("pub const SIZING: ", "ThemeSizing = ThemeSizing {");
    let Some(s) = lines.iter().position(|l| l.starts_with(head)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in lines.iter().skip(s + 1) {
        let t = line.trim();
        if t == "};" {
            break;
        }
        let Some((name, rest)) = t.split_once(": LogicalPx(") else {
            continue;
        };
        let Some((num, _)) = rest.split_once(')') else {
            continue;
        };
        if let Ok(v) = num.parse::<f32>() {
            out.push((name.to_string(), v));
        }
    }
    out
}

/// `Theme` 필드의 doc 주석 블록을 이름과 짝지어 읽는다.
fn scan_field_docs(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let struct_head = concat!("pub struct ", "Theme {");
    let Some(s) = lines.iter().position(|l| l.starts_with(struct_head)) else {
        return Vec::new();
    };
    let Some(e) = lines
        .iter()
        .skip(s + 1)
        .position(|l| *l == "}")
        .map(|i| i + s + 1)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in (s + 1)..e {
        let t = lines[i].trim();
        let Some(rest) = t.strip_prefix("pub ") else {
            continue;
        };
        let Some(name) = rest.strip_suffix(": LogicalPx,") else {
            continue;
        };
        if name.contains(' ') {
            continue;
        }
        let mut doc = String::new();
        let mut j = i;
        while j > s + 1 && lines[j - 1].trim_start().starts_with("///") {
            j -= 1;
            doc.insert_str(0, lines[j].trim_start().trim_start_matches("///"));
        }
        out.push((name.to_string(), doc));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn declared() -> BTreeSet<String> {
        EXEMPT.iter().map(|(n, _)| (*n).to_string()).collect()
    }

    fn measured(fields: &[Field]) -> BTreeSet<String> {
        fields
            .iter()
            .filter(|(_, z)| !*z)
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// 실제 면제 필드와 EXEMPT를 이름으로 비교한다.
    #[test]
    fn the_zoom_exempt_field_set_is_exactly_the_declared_one() {
        let fields = scan_fields(&theme_source());

        assert!(
            fields.len() >= FIELD_FLOOR,
            "LogicalPx 필드가 {} 개뿐이다 — 하한 {}. 스캔이 theme.rs 를 못 읽었을 \
             가능성이 높다(면제 집합 판정 이전의 문제다).",
            fields.len(),
            FIELD_FLOOR
        );
        let zoomed_count = fields.iter().filter(|(_, z)| *z).count();
        assert!(
            zoomed_count > 0,
            "zoom 을 타는 필드가 0 이다 — 초기화식을 파싱하지 못했다 (필드 {} 개는 읽혔다)",
            fields.len()
        );

        let m = measured(&fields);
        let d = declared();
        let joined: Vec<&String> = m.difference(&d).collect();
        let gone: Vec<&String> = d.difference(&m).collect();
        assert!(
            joined.is_empty() && gone.is_empty(),
            "zoom 면제 집합이 정본과 다르다.\n  새로 면제된 필드: {joined:?}\n  \
             목록에만 있고 소스엔 없는 필드: {gone:?}\n\
             새 필드를 면제하려면 EXEMPT에 사유와 함께 등록해야 한다."
        );
    }

    /// 합성 입력에서 배율 적용과 제외를 구분하는지 확인한다.
    #[test]
    fn the_scanner_tells_a_zoomed_initializer_from_a_bare_one() {
        let src = FIXTURE;
        let f = scan_fields(src);
        assert_eq!(f.len(), 3, "fixture 필드 3 개를 읽어야 한다: {f:?}");
        assert_eq!(
            f.iter().find(|(n, _)| n == "spacing_md").map(|(_, z)| *z),
            Some(true),
            "zoom 을 타는 초기화식을 못 알아봤다"
        );
        assert_eq!(
            f.iter().find(|(n, _)| n == "border_width").map(|(_, z)| *z),
            Some(false),
            "맨 초기화식을 zoom 으로 오인했다"
        );
    }

    /// 기존 접두사로 새 면제 필드를 추가해도 이름 대조가 검출하는지 확인한다.
    /// 하한이나 접두사 검사만으로는 잡히지 않는 경우를 함께 비교한다.
    #[test]
    fn a_new_field_that_borrows_an_exempt_prefix_is_caught_only_by_set_equality() {
        let mutant = FIXTURE.replace(
            "    pub border_width: LogicalPx,",
            "    pub border_width: LogicalPx,\n    pub tab_bar_close_size: LogicalPx,",
        );
        let mutant = mutant.replace(
            "            border_width: SIZING.border_width,",
            "            border_width: SIZING.border_width,\n            \
             tab_bar_close_size: SIZING.tab_bar_close_size,",
        );
        let fields = scan_fields(&mutant);
        assert_eq!(
            fields.len(),
            4,
            "합성 필드 추가 결과가 4개여야 한다: {fields:?}"
        );

        let exempt = measured(&fields);
        assert!(
            exempt.contains("tab_bar_close_size"),
            "추가한 합성 필드가 면제 집합에 포함돼야 한다"
        );

        // 개수가 늘어도 최소 하한은 통과한다.
        let floor_form = exempt.len() >= 2;
        assert!(
            floor_form,
            "하한 검사만으로는 추가한 면제를 검출하지 못한다"
        );

        // 기존 접두사를 사용하면 접두사 검사도 통과한다.
        let prefixes = [
            "tab_bar_",
            "titlebar_",
            "font_size_term",
            "border_",
            "icon_stroke_",
        ];
        let prefix_form = exempt
            .iter()
            .all(|n| prefixes.iter().any(|p| n.starts_with(p)));
        assert!(
            prefix_form,
            "접두사 검사만으로는 추가한 면제를 검출하지 못한다"
        );

        // 이름 집합을 비교하면 새 필드를 찾을 수 있다.
        let declared_here: BTreeSet<String> =
            ["border_width".to_string(), "font_size_term".to_string()].into();
        let joined: Vec<&String> = exempt.difference(&declared_here).collect();
        assert_eq!(
            joined,
            vec![&"tab_bar_close_size".to_string()],
            "집합 비교는 추가한 면제 필드의 이름을 찾아야 한다"
        );
    }

    /// 나중에 스캔 범위가 넓어져도 자기 소스를 구조체로 오인하지 않도록 검색 구문을 나눠 둔다.
    #[test]
    fn the_guard_does_not_carry_its_own_anchors() {
        let me = include_str!("zoom_exempt_fields_guard.rs");
        let struct_head = concat!("pub struct ", "Theme {");
        let ctor_anchor = concat!("let zoo", "med = |px: LogicalPx|");
        assert_eq!(
            me.matches(struct_head).count(),
            0,
            "검사 소스 안에 완전한 구조체 검색 구문이 있다. 문자열을 나눠야 한다."
        );
        assert_eq!(
            me.matches(ctor_anchor).count(),
            0,
            "검사 소스 안에 완전한 생성자 검색 구문이 있다. 문자열을 나눠야 한다."
        );
        // 소스 자체가 빈 문자열로 읽힌 경우와 구분한다.
        assert!(me.contains("LogicalPx"), "가드 소스를 못 읽었다");
    }

    #[test]
    fn the_scanner_finds_known_fields_in_the_real_source() {
        let fields = scan_fields(&theme_source());
        let by = |n: &str| fields.iter().find(|(f, _)| f == n).map(|(_, z)| *z);
        assert_eq!(
            by("border_width"),
            Some(false),
            "border_width 는 면제여야 한다"
        );
        assert_eq!(
            by("spacing_md"),
            Some(true),
            "spacing_md 는 zoom 을 타야 한다"
        );
        assert_eq!(
            by("focus_ring_width"),
            Some(true),
            "focus_ring_width 는 zoom 을 탄다"
        );
    }

    /// 중복 이름이 집합 변환 과정에서 사라지지 않도록 목록 중복을 검사한다.
    #[test]
    fn the_declared_list_has_no_duplicates() {
        assert_eq!(
            declared().len(),
            EXEMPT.len(),
            "EXEMPT 에 중복 이름이 있다 — 집합으로 접으면 {} 개로 줄어든다",
            declared().len()
        );
    }

    /// 각 면제 필드의 설명에 사유 표지가 있는지 확인한다. 디자인 사유의 타당성 검사는 아니다.
    #[test]
    fn every_exemption_repeats_its_category_in_the_field_doc() {
        let docs = scan_field_docs(&theme_source());
        assert!(
            docs.len() >= FIELD_FLOOR,
            "필드 doc 을 {} 개만 읽었다 — 하한 {}. 파일과 파싱 결과를 확인한다.",
            docs.len(),
            FIELD_FLOOR
        );
        let mut bad = Vec::new();
        for (name, reason) in EXEMPT {
            let Some((_, doc)) = docs.iter().find(|(n, _)| n == name) else {
                bad.push(format!("{name}: 필드를 못 찾았다"));
                continue;
            };
            if !doc.contains(reason.marker()) {
                bad.push(format!("{name}: doc 에 '{}' 가 없다", reason.marker()));
            }
        }
        assert!(
            bad.is_empty(),
            "면제 갈래와 필드 doc 이 어긋난다:\n  {}",
            bad.join("\n  ")
        );
    }

    /// 면제 아닌 필드가 같은 표지를 포함해 분류가 모호해지지 않는지 확인한다.
    #[test]
    fn the_category_markers_do_not_match_every_field() {
        let docs = scan_field_docs(&theme_source());
        let exempt: std::collections::BTreeSet<&str> = EXEMPT.iter().map(|(n, _)| *n).collect();
        let markers = [
            Reason::Content,
            Reason::Hairline,
            Reason::TabBar,
            Reason::StatusBar,
            Reason::Titlebar,
        ];
        let stray: Vec<&String> = docs
            .iter()
            .filter(|(n, _)| !exempt.contains(n.as_str()))
            .filter(|(_, d)| markers.iter().any(|m| d.contains(m.marker())))
            .map(|(n, _)| n)
            .collect();
        assert!(
            stray.is_empty(),
            "면제가 아닌 필드가 갈래 표지를 담고 있다 — 표지가 변별력을 잃었다: {stray:?}"
        );
        let hit = docs
            .iter()
            .filter(|(n, _)| exempt.contains(n.as_str()))
            .filter(|(_, d)| markers.iter().any(|m| d.contains(m.marker())))
            .count();
        assert_eq!(
            hit,
            EXEMPT.len(),
            "면제 {} 중 {hit} 만 표지를 담았다",
            EXEMPT.len()
        );
    }

    /// tasty-settings와 맞춰야 하는 지원 배율 사본. 원본 변경은 해당 크레이트의 검사가 알린다.
    const SUPPORTED_ZOOMS: [f32; 3] = [0.85, 1.0, 1.2];

    /// 지원 배율을 곱해도 반올림 결과가 같아 값만으로 적용 여부를 구분할 수 없는 필드.
    const UNOBSERVABLE: &[&str] = &["border_width", "selection_edge_width", "tint_edge_width"];

    /// 배율 적용 여부에 따라 값이 달라지지 않는 필드 집합을 대조한다.
    /// 토큰 값이나 지원 배율이 바뀌면 이 구분도 다시 확인해야 한다.
    #[test]
    fn the_unobservable_exemptions_are_exactly_the_pinned_ones() {
        let src = theme_source();
        let values: std::collections::BTreeMap<String, f32> =
            scan_sizing_values(&src).into_iter().collect();
        assert!(
            values.len() >= FIELD_FLOOR,
            "SIZING 값을 {} 개만 읽었다 — 하한 {}. 파싱 결과를 확인한다.",
            values.len(),
            FIELD_FLOOR
        );

        let mut measured: Vec<&str> = Vec::new();
        let mut missing: Vec<&str> = Vec::new();
        for (name, _) in EXEMPT {
            let Some(v) = values.get(*name) else {
                missing.push(name);
                continue;
            };
            if SUPPORTED_ZOOMS.iter().all(|z| (v * z).round() == *v) {
                measured.push(name);
            }
        }
        assert!(missing.is_empty(), "SIZING 에 없는 면제 필드: {missing:?}");

        let m: std::collections::BTreeSet<&str> = measured.into_iter().collect();
        let p: std::collections::BTreeSet<&str> = UNOBSERVABLE.iter().copied().collect();
        assert_eq!(
            m,
            p,
            "면제의 관측 가능성이 바뀌었다.\n  배율 적용 전후 값이 같아진 필드: {:?}\n               배율 적용 전후 값이 달라진 필드: {:?}\n             값이 바뀌었거나 지원 배율 집합이 바뀌었다. 어느 쪽이든 디자인 판단이 필요하다.",
            m.difference(&p).collect::<Vec<_>>(),
            p.difference(&m).collect::<Vec<_>>()
        );

        // 다른 면제 필드에는 배율을 곱했을 때 실제 차이가 있어야 한다.
        assert!(
            EXEMPT.len() > p.len(),
            "모든 면제 필드의 배율 적용 전후 값이 같다. 수집과 계산을 확인한다."
        );
    }

    /// 면제 여부와 별개로 값·배율 조합에 따라 반올림 결과의 차이를 구분하는지 확인한다.
    #[test]
    fn the_observability_test_discriminates_outside_the_exempt_set() {
        let values: std::collections::BTreeMap<String, f32> =
            scan_sizing_values(&theme_source()).into_iter().collect();
        let free = |n: &str| {
            let v = values
                .get(n)
                .unwrap_or_else(|| panic!("{n} 을 SIZING 에서 못 찾았다"));
            SUPPORTED_ZOOMS.iter().all(|z| (v * z).round() == *v)
        };
        assert!(
            free("focus_ring_width"),
            "focus_ring_width(2.0)는 지원 배율에서 반올림 결과가 같아야 한다"
        );
        assert!(
            !free("status_dot_size"),
            "status_dot_size(8.0)는 지원 배율에서 반올림 결과가 달라져야 한다"
        );
        assert!(
            !free("icon_stroke_width"),
            "icon_stroke_width(1.5)는 지원 배율에서 반올림 결과가 달라져야 한다"
        );
    }

    const FIXTURE: &str = concat!(
        "pub struct ",
        "Theme {\n",
        "    pub spacing_md: LogicalPx,\n",
        "    pub border_width: LogicalPx,\n",
        "    pub font_size_term: LogicalPx,\n",
        "}\n",
        "\n",
        "impl Theme {\n",
        "    fn build() -> Self {\n",
        "        let zoo",
        "med = |px: LogicalPx| LogicalPx((px.value() * z).round());\n",
        "        Self {\n",
        "            spacing_md: zoo",
        "med(SIZING.spacing_md),\n",
        "            border_width: SIZING.border_width,\n",
        "            font_size_term: SIZING.font_size_term,\n",
        "        }\n",
        "    }\n",
        "}\n",
    );
}
