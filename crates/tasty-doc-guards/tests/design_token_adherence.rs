//! UI 소스의 인라인 간격·폰트·선 굵기·반경 값과 primitive 색 접근을 검사한다.
//! 간격은 typed 헬퍼와 Theme 값을, 폰트·색·선 굵기는 역할에 맞는 토큰을 사용한다.
//! 대응 토큰이 없는 구조값은 이유가 드러나는 이름의 상수로 둔다.
//! 색 생성자 호출은 clippy disallowed_methods도 검사한다.
//!
//! 이 크레이트는 GUI 의존성이 없으며 doc-guards.yml의 경로 필터 없는 develop·release·main push·PR와
//! crossplatform-check.yml의 Windows 잡에서 실행된다. 전체 구성은 docs/dev-guide/ci-gates.md를 따른다.
//! 컴파일만으로는 실행되지 않는다. 직접 확인하려면 cargo test -p tasty-doc-guards --test design_token_adherence --locked를 쓴다.
//!
//! 소스 문자열과 첫 숫자만 읽는 검사이므로 다음 형태를 모두 잡지는 못한다.
//! 괄호로 감싼 값, 인라인 주석, 다음 줄이 주석인 호출, 형변환·매크로·음수,
//! 상수·변수 경유 값, egui 기본 스타일의 값은 누락될 수 있다.
//! 상수의 역할이나 값의 출처는 추적하지 않는다. 토큰과 같은 값을 복제한 일부 명명 상수는
//! src/design_token_guard.rs가 별도로 검사한다.

use std::path::{Path, PathBuf};
use tasty_doc_guards::source_text::repo_relative;
use tasty_doc_guards::temp_scratch::Scratch;

/// 간격 스캔 대상 (repo-relative). host UI 계층 + 갤러리 + 위젯 크레이트.
const SCAN_ROOTS: &[&str] = &[
    "src/view",
    "src/adapters/ui",
    "src/gfx/gpu",
    "crates/tasty-gallery/src",
    "crates/tasty-ui-widgets/src",
    // 공용 egui 어댑터의 리터럴은 여러 UI 호출부에 영향을 주므로 함께 검사한다.
    "crates/tasty-egui-theme/src",
];

/// host UI와 위젯에서는 primitive 색 필드 대신 역할별 접근자를 사용한다(docs/design/systems/theme.md).
/// 팔레트 원색을 보여 주는 갤러리는 제외한다.
const COLOR_SCAN_ROOTS: &[&str] = &[
    "src/view",
    "src/adapters/ui",
    "src/gfx/gpu",
    "crates/tasty-ui-widgets/src",
];

/// host UI의 글리프만 검사한다. 기호 견본이 있는 갤러리와 위젯·플러그인은 제외한다.
const GLYPH_SCAN_ROOTS: &[&str] = &["src/view", "src/adapters/ui", "src/gfx/gpu"];

/// 갤러리는 egui 전역 zoom으로 리터럴도 확대하므로 길이 검사에서 제외한다(docs/concepts/typed-length.md).
const LENGTH_SETTER_SCAN_ROOTS: &[&str] = &[
    "src/view",
    "src/adapters/ui",
    "src/gfx/gpu",
    "crates/tasty-ui-widgets/src",
    "crates/tasty-egui-theme/src",
];

/// 본체는 egui zoom_factor가 1이라 Theme 밖에 직접 적은 길이에 UI 배율이 적용되지 않는다.
const LENGTH_SETTER_PREFIXES: &[&str] = &[
    "set_min_width(",
    "set_max_width(",
    "set_min_height(",
    "set_max_height(",
    "min_height(",
    "max_height(",
    "exact_width(",
    "exact_height(",
    "desired_width(",
    "desired_height(",
];

/// 대응 field_width 토큰이 없는 기존 리터럴의 한시 목록.
/// 값을 기존 토큰에 맞출지 새 토큰을 만들지는 디자인 판단이므로 검사에서 임의로 바꾸지 않는다.
/// 리터럴이 제거되면 항목도 제거한다. 범위 예외인 ALLOWLIST_PREFIXES와 구분한다.
const LENGTH_SETTER_BASELINE: &[(&str, &str, &str)] = &[];

/// 2026-09-07 실측 186파일에 하한 150을 뒀다. 여유 36보다 작은 누락은 잡지 못한다.
/// length_setter_literals_under는 각 루트가 비었는지도 별도로 확인한다.
const MIN_LENGTH_SETTER_SCANNED_FILES: usize = 150;

/// 역할별 접근자로 바꿔야 하는 primitive 필드. text_primary 같은 긴 이름은 경계로 구분한다.
const PRIMITIVE_COLOR_FIELDS: &[&str] = &[
    "crust",
    "mantle",
    "base",
    "surface0",
    "surface1",
    "surface2",
    "overlay0",
    "overlay1",
    "overlay2",
    "text",
    "subtext1",
    "subtext0",
    "blue",
    "green",
    "red",
    "yellow",
    "peach",
    "mauve",
    "teal",
    "sky",
    "lavender",
    "flamingo",
    "pink",
    "maroon",
    "rosewater",
];

/// 접두어 뒤 첫 숫자를 인라인 값으로 판정한다. 토큰·명명 상수는 이 검사에서 제외된다.
const FORBIDDEN_PREFIXES: &[&str] = &[
    "add_space(",
    "Margin::same(",
    "Margin::symmetric(",
    "inner_margin(",
    "FontId::proportional(",
    "FontId::monospace(",
    // 두 폰트 생성자의 공통 경로인 new도 포함한다.
    "FontId::new(",
    "Stroke::new(",
    // RichText 크기와 Spinner 지름이 같은 메서드명이라 지름 견본만 별도로 면제한다.
    ".size(",
    // 아이콘 크기도 토큰을 사용하고 스케일 밖 값은 이유를 적은 명명 상수로 둔다(docs/design/systems/theme.md).
    ".image(",
    // 직접 반경을 넘기는 형태와 CornerRadius 생성자로 감싼 형태를 모두 검사한다.
    // 스케일 밖 반경을 명명 상수로 빼면 그 값에는 Theme의 배율이 적용되지 않는다는 차이가 있다.
    ".corner_radius(",
    "CornerRadius::same(",
];

/// 구조체 필드는 숫자보다 이름이 먼저 나와 접두어 숫자 검사로 잡지 못한다.
/// Stroke·FontId는 토큰을 넘기는 생성자를 쓰도록 구조체 리터럴 자체를 금지한다.
/// 반환 타입 선언과는 앞의 ->로 구분한다.
const FORBIDDEN_FORMS: &[&str] = &["Stroke {", "FontId {"];

/// (파일, 접두어, 선행 문자열 표지)로 면제한다.
/// 표지가 있으면 같은 줄의 접두어 앞에 있어야 한다. 실제 수신자 타입을 판정하는 것은 아니다.
/// None은 해당 파일의 그 접두어 전체를 면제하므로 typed 헬퍼 구현에만 사용한다.
const ALLOWLIST_PREFIXES: &[(&str, &str, Option<&str>)] = &[
    // 간격 헬퍼 구현은 raw 간격 호출이 필요하다. 폰트·선 굵기는 면제하지 않는다.
    ("crates/tasty-ui-widgets/src/spacing.rs", "add_space(", None),
    (
        "crates/tasty-ui-widgets/src/spacing.rs",
        "Margin::same(",
        None,
    ),
    (
        "crates/tasty-ui-widgets/src/spacing.rs",
        "Margin::symmetric(",
        None,
    ),
    (
        "crates/tasty-ui-widgets/src/spacing.rs",
        "inner_margin(",
        None,
    ),
    // Spinner 지름 견본만 면제한다. 같은 파일의 RichText 크기는 계속 검사해야 한다.
    (
        "crates/tasty-gallery/src/catalog/components/prim_spinner.rs",
        ".size(",
        Some("Spinner::new()"),
    ),
];

#[test]
fn the_spinner_exemption_discriminates_receiver() {
    const SPIN: &str = "crates/tasty-gallery/src/catalog/components/prim_spinner.rs";

    assert!(exempt(
        SPIN,
        ".size(",
        "            Spinner::new().size(12.0).show(ui, theme);"
    ));
    assert!(!exempt(
        SPIN,
        ".size(",
        "            ui.label(RichText::new(\"x\").size(13.0));"
    ));
    assert!(!exempt(
        "crates/tasty-gallery/src/catalog/components/empty_surface.rs",
        ".size(",
        "    Spinner::new().size(30.0);"
    ));
    assert!(!exempt(
        SPIN,
        ".size(",
        "    x.size(13.0); // Spinner::new()"
    ));
    assert!(exempt(
        "crates/tasty-ui-widgets/src/spacing.rs",
        "add_space(",
        "    ui.add_space(8.0);"
    ));
}

#[test]
fn the_typed_helper_exemption_does_not_cover_other_axes() {
    const SP: &str = "crates/tasty-ui-widgets/src/spacing.rs";

    assert!(exempt(SP, "add_space(", "    ui.add_space(8.0);"));
    assert!(exempt(SP, "Margin::same(", "    Margin::same(12.0)"));

    for form in [
        ".size(",
        "FontId::proportional(",
        "Stroke::new(",
        "inner_margin(",
    ] {
        let exempted = exempt(SP, form, "    x");
        assert_eq!(
            exempted,
            form == "inner_margin(",
            "`{form}` 의 면제 여부가 뒤집혔다 — spacing.rs 면제는 등록된 네 접두에만 \
             걸려야 한다"
        );
    }

    assert_eq!(
        violating_prefix(SP, "    ui.label(RichText::new(x).size(13.0));", ""),
        Some(".size(")
    );
    assert_eq!(
        violating_prefix(SP, "    Stroke::new(2.0, c)", ""),
        Some("Stroke::new(")
    );
}

#[test]
fn the_return_signature_skip_only_covers_signatures() {
    assert_eq!(
        violating_prefix("src/view/x.rs", "fn s(..) -> egui::Stroke {", ""),
        None
    );
    assert_eq!(
        violating_prefix("src/view/x.rs", "fn f(..) -> egui::FontId {", ""),
        None
    );

    assert_eq!(
        violating_prefix(
            "src/view/x.rs",
            "    let s = egui::Stroke { width: 1.0 };",
            ""
        ),
        Some("Stroke {")
    );
    assert_eq!(
        violating_prefix(
            "src/view/x.rs",
            "fn s() -> egui::Stroke { egui::Stroke { width: 1.0 } }",
            ""
        ),
        Some("Stroke {")
    );
}

#[test]
fn the_corner_radius_axis_needs_both_prefixes() {
    const F: &str = "src/view/x.rs";

    assert_eq!(
        violating_prefix(
            F,
            "        .corner_radius(egui::CornerRadius::same(12))",
            ""
        ),
        Some("CornerRadius::same(")
    );
    assert_eq!(
        violating_prefix(F, "        .corner_radius(4.0)", ""),
        Some(".corner_radius(")
    );

    for ok in [
        "        .corner_radius(th.corner_radius.value())",
        "        .corner_radius(tasty_ui_widgets::tokens::BOOT_CARD_CORNER_RADIUS)",
        "        .corner_radius(egui::CornerRadius::ZERO)",
    ] {
        assert_eq!(
            violating_prefix(F, ok, ""),
            None,
            "처방 형태가 잡혔다: {ok}"
        );
    }
}

/// 개별 파일을 루트로 등록하면 같은 디렉터리에 추가된 UI 파일을 놓칠 수 있다.
#[test]
fn the_gpu_scan_root_is_a_directory_not_a_file() {
    assert!(
        SCAN_ROOTS.contains(&"src/gfx/gpu"),
        "gpu UI 계층의 스캔 루트가 디렉토리가 아니다 — 파일 단위로 되돌리면 그 \
         디렉토리에 새로 생기는 UI 파일이 기본 제외가 된다"
    );
    for roots in [SCAN_ROOTS, COLOR_SCAN_ROOTS, GLYPH_SCAN_ROOTS] {
        assert!(
            !roots.iter().any(|r| r.ends_with(".rs")),
            "스캔 루트에 개별 `.rs` 파일이 있다: {roots:?}"
        );
    }
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let mut files = Vec::new();
    gather_rs_files(&root.join("src/gfx/gpu"), &mut files);
    assert!(
        files.len() >= 2,
        "src/gfx/gpu 에서 걷은 .rs 가 {} 개다 — 경로를 확인할 것",
        files.len()
    );
}

/// 문자열 표지가 있으면 같은 줄에서 접두어 앞에 있는 경우만 면제한다.
fn exempt(rel: &str, form: &str, line: &str) -> bool {
    ALLOWLIST_PREFIXES.iter().any(|(path, prefix, marker)| {
        *path == rel
            && *prefix == form
            && match marker {
                None => true,
                Some(m) => match (line.find(m), line.find(form)) {
                    (Some(mi), Some(fi)) => mi < fi,
                    _ => false,
                },
            }
    })
}

/// `line` 에 금지 prefix + 숫자 인자가 있으면 매칭된 prefix 를 돌려준다.
/// `rel` 파일에 대해 그 접두가 [`ALLOWLIST_PREFIXES`] 에 있으면 건너뛴다.
fn violating_prefix(rel: &str, line: &str, next_line: &str) -> Option<&'static str> {
    for &form in FORBIDDEN_FORMS {
        if exempt(rel, form, line) {
            continue;
        }
        let mut from = 0;
        while let Some(idx) = line[from..].find(form) {
            let start = from + idx;
            // 반환 타입을 구조체 리터럴로 오해하지 않도록 타입 경로 앞의 ->를 확인한다.
            let head = line[..start]
                .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == ':');
            if !head.trim_end().ends_with("->") {
                return Some(form);
            }
            from = start + form.len();
        }
    }
    for &prefix in FORBIDDEN_PREFIXES {
        if exempt(rel, prefix, line) {
            continue;
        }
        let mut from = 0;
        while let Some(idx) = line[from..].find(prefix) {
            let after = &line[from + idx + prefix.len()..].trim_start();
            // 호출이 열린 채 행이 끝나면 바로 다음 한 줄만 검사한다.
            let probe = if after.is_empty() {
                next_line.trim_start()
            } else {
                after
            };
            if matches!(probe.chars().next(), Some(c) if c.is_ascii_digit()) {
                return Some(prefix);
            }
            from += idx + prefix.len();
        }
    }
    None
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `line` 에 `th.<primitive>` / `theme.<primitive>` 평면 필드 접근이 있으면 그 표현을 돌려준다.
/// 앞뒤 경계를 검사해 `th.text_primary()`(semantic) 나 `mytheme.blue` 오검출을 배제한다.
fn violating_color(_rel: &str, line: &str, _next: &str) -> Option<String> {
    for receiver in ["th.", "theme."] {
        let mut from = 0;
        while let Some(idx) = line[from..].find(receiver) {
            let start = from + idx;
            let before_ok =
                start == 0 || !is_word_char(line[..start].chars().next_back().unwrap_or(' '));
            let after = &line[start + receiver.len()..];
            if before_ok {
                for &field in PRIMITIVE_COLOR_FIELDS {
                    if let Some(rest) = after.strip_prefix(field) {
                        let next = rest.chars().next();
                        if !matches!(next, Some(c) if is_word_char(c)) {
                            return Some(format!("{receiver}{field}"));
                        }
                    }
                }
            }
            from = start + receiver.len();
        }
    }
    None
}

/// 루트 아래 Rust 파일에 판정을 적용한다. //로 시작하는 줄만 주석으로 제외한다.
fn collect_violations(
    root: &Path,
    target: &str,
    detect: &dyn Fn(&str, &str, &str) -> Option<String>,
    out: &mut Vec<String>,
) {
    let path = root.join(target);
    let mut files = Vec::new();
    gather_rs_files(&path, &mut files);
    assert!(
        !files.is_empty(),
        "스캔 루트 {target}에서 Rust 파일을 찾지 못했다. 경로와 읽기 오류를 확인한다."
    );
    for file in files {
        let rel = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let contents = std::fs::read_to_string(&file).expect("소스 파일 read 실패");
        let lines: Vec<&str> = contents.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            let next = lines.get(i + 1).copied().unwrap_or("");
            if let Some(hit) = detect(&rel, line, next) {
                out.push(format!("  {}:{} — `{}`", rel, i + 1, hit));
            }
        }
    }
}

fn gather_rs_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path.to_path_buf());
        }
        return;
    }
    let entries = std::fs::read_dir(path)
        .unwrap_or_else(|e| panic!("스캔 디렉터리를 읽지 못했다: {} — {e}", path.display()));
    for entry in entries.flatten() {
        gather_rs_files(&entry.path(), out);
    }
}

/// vec2 이후 같은 줄의 focus_ring_width를 바 용도로 의심해 보고한다.
/// 링과 바는 값이 같아도 배율 적용이 달라 바에는 selection_edge_width 역할을 사용한다.
/// 변수 경유나 좌표 산술은 찾지 못하며 인자·타입을 정밀하게 분석하지 않는다.
fn ring_token_used_as_bar(rel: &str, lines: &[&str], out: &mut Vec<String>) {
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let Some(v) = line.find("vec2(") else {
            continue;
        };
        if line[v..].contains("focus_ring_width") {
            out.push(format!(
                "  {}:{} — vec2 이후에 focus_ring_width가 있다. 바 용도라면 selection_edge_width 역할(탭은 tab_indicator_width)을 사용한다.",
                rel,
                i + 1
            ));
        }
    }
}

#[test]
fn no_literal_margin_fields_or_item_spacing() {
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let mut violations = Vec::new();
    for target in SCAN_ROOTS {
        let path = root.join(target);
        let mut files = Vec::new();
        gather_rs_files(&path, &mut files);
        assert!(
            !files.is_empty(),
            "스캔 루트 `{target}` 에서 .rs 파일을 찾지 못했다"
        );
        for file in files {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let contents = std::fs::read_to_string(&file).expect("소스 파일 read 실패");
            let lines: Vec<&str> = contents.lines().collect();
            margin_field_violations(&rel, &lines, &mut violations);
            item_spacing_violations(&rel, &lines, &mut violations);
            ring_token_used_as_bar(&rel, &lines, &mut violations);
        }
    }
    assert!(
        violations.is_empty(),
        "`Margin` 필드 / `item_spacing` 에 인라인 숫자 리터럴이 있다 — 토큰이나 명명 \
         const 로 바꿀 것:\n\
         · 4px 그리드 값 → `th.spacing_xs/sm/md/lg/xl.value() as i8`(Margin) 또는 \
         `.value()`(item_spacing)\n\
         · 1~4px 미세 간격 → `tasty_ui_widgets::tokens::STRUCT_GAP_1..4`\n\
         · 그리드 밖 값(9·10·11·14 등) → 사유를 적은 명명 const\n\
         · 0 은 그리드의 원점이라 규칙 안에 있다 — 다만 네 변이 전부 0 이면 \
         `Margin::ZERO`\n\
         · 링 토큰을 바에 쓰지 말 것 — 감싸는 획은 `focus_ring_width`, 한쪽 변에 \
         붙는 띠는 `selection_edge_width` 역할(둘 다 2 지만 zoom 거동이 다르다)\n{}",
        violations.join("\n")
    );
}

#[test]
fn no_inline_visual_token_literals() {
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    // 구조체 리터럴은 값과 무관하게 금지하므로 숫자만 바꾸면 된다고 안내하지 않는다.
    let detect = |rel: &str, line: &str, next: &str| {
        violating_prefix(rel, line, next).map(|p| {
            if FORBIDDEN_FORMS.contains(&p) {
                format!(
                    "{p}` — 구조체 리터럴 형태 자체가 금지(생성자 \
                     `Stroke::new(<토큰>, ..)` · `FontId::proportional(<토큰>)` 를 쓴다)`"
                )
            } else {
                format!("{p}<숫자>")
            }
        })
    };
    let mut violations = Vec::new();
    for target in SCAN_ROOTS {
        collect_violations(root, target, &detect, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "인라인 시각 토큰 리터럴이 재유입됨 — 각 축의 대체 수단으로 바꿀 것:\n\
         · 간격/마진 → typed 헬퍼(vspace/hspace/margin_all/margin_sym) + th.spacing_* / \
         STRUCT_GAP_*\n\
         · 폰트 크기 → th.font_size_micro/caption/body/heading/max, 또는 역할을 이름에 \
         담은 component 접근자(th.badge_font_size() · th.tag_font_size() · \
         th.kbd_font_size() 등)\n\
         · 선 굵기 → th.border_width(1) / th.focus_ring_width(2) / th.icon_stroke_width(1.5)\n\
         대응 토큰이 없는 구조값은 명명 const(`const NAME: LogicalPx = LogicalPx(N)`)로 \
         승격한다 — 그건 스코프 밖이다:\n{}",
        violations.join("\n")
    );
}

/// 숫자 리터럴 토큰인가 — `12` · `12.0` · `1.5f32` 등. 부호는 호출부가 뗀다.
fn numeric_literal(tok: &str) -> Option<f32> {
    let t = tok.trim().trim_end_matches("f32").trim_end_matches("f64");
    if t.is_empty() || !t.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    t.parse::<f32>().ok()
}

/// 0은 간격 없음으로 허용한다. 일부 필드가 0인 의도는 판별하지 못하고 네 변 모두 0인 경우는 Margin::ZERO를 안내한다.
fn is_zero(v: f32) -> bool {
    v == 0.0
}

/// 비대칭 마진은 구조체 리터럴이 필요하므로 형태를 금지하지 않고 필드 값을 검사한다.
fn margin_field_violations(rel: &str, lines: &[&str], out: &mut Vec<String>) {
    let mut depth = 0usize;
    let mut block_start = 0usize;
    let mut zero_fields = 0usize;
    let mut other_fields = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        if depth > 0 {
            if let Some((name, rest)) = t.split_once(':')
                && matches!(name.trim(), "left" | "right" | "top" | "bottom")
            {
                match numeric_literal(rest.trim().trim_end_matches(',')) {
                    Some(v) if is_zero(v) => zero_fields += 1,
                    Some(v) => {
                        other_fields += 1;
                        out.push(format!(
                            "  {}:{} — `Margin {{ {}: {v} }}`",
                            rel,
                            i + 1,
                            name.trim()
                        ));
                    }
                    None => other_fields += 1,
                }
            }
            if line.contains('}') {
                if zero_fields == 4 && other_fields == 0 {
                    out.push(format!(
                        "  {}:{} — 네 변이 전부 0 인 `Margin` 리터럴이다. `Margin::ZERO` 를 쓸 것",
                        rel,
                        block_start + 1
                    ));
                }
                depth = 0;
            }
            continue;
        }
        if line.contains("Margin {") && !line.trim_end().ends_with("Margin {") {
            for part in line.split(&['{', ',', '}'][..]) {
                if let Some((name, rest)) = part.split_once(':')
                    && matches!(name.trim(), "left" | "right" | "top" | "bottom")
                    && let Some(v) = numeric_literal(rest)
                    && !is_zero(v)
                {
                    out.push(format!(
                        "  {}:{} — `Margin {{ {}: {v} }}`",
                        rel,
                        i + 1,
                        name.trim()
                    ));
                }
            }
        } else if line.contains("Margin {") {
            depth = 1;
            block_start = i;
            zero_fields = 0;
            other_fields = 0;
        }
    }
}

/// item_spacing의 x·y 대입과 벡터 대입에서 숫자를 찾는다. 현재 줄과 이어진 최대 세 줄을 읽는다.
fn item_spacing_violations(rel: &str, lines: &[&str], out: &mut Vec<String>) {
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let Some(eq) = line.find("item_spacing").map(|p| p + "item_spacing".len()) else {
            continue;
        };
        let Some(rel_eq) = line[eq..].find('=') else {
            continue;
        };
        // == 비교를 대입으로 읽지 않도록 제외한다.
        if line[eq + rel_eq..].starts_with("==") {
            continue;
        }
        let mut stmt = line[eq + rel_eq + 1..].to_string();
        let mut j = i;
        while !stmt.contains(';') && j + 1 < lines.len() && j - i < 3 {
            j += 1;
            stmt.push(' ');
            stmt.push_str(lines[j].trim());
        }
        for tok in stmt.split(&['(', ')', ',', ';'][..]) {
            if let Some(v) = numeric_literal(tok)
                && !is_zero(v)
            {
                out.push(format!("  {}:{} — `item_spacing = {v}`", rel, i + 1));
                break;
            }
        }
    }
}

/// 이모지·픽토그래프와 딩뱃 범위를 검사한다. 화살표·기하 도형·기술 기호·경고 기호 등은 이 범위 밖이다.
fn is_forbidden_pictographic(cp: u32) -> bool {
    (0x1F000..=0x1FAFF).contains(&cp) || (0x2700..=0x27BF).contains(&cp)
}

/// 리터럴 문자와 Unicode 이스케이프를 같은 코드포인트 범위로 검사한다.
fn violating_glyph(_rel: &str, line: &str, _next: &str) -> Option<String> {
    for ch in line.chars() {
        let cp = ch as u32;
        if is_forbidden_pictographic(cp) {
            return Some(format!("U+{cp:04X} `{ch}`"));
        }
    }
    let needle = "\\u{";
    let mut from = 0;
    while let Some(rel) = line[from..].find(needle) {
        let start = from + rel + needle.len();
        let Some(close_rel) = line[start..].find('}') else {
            break;
        };
        let hex = &line[start..start + close_rel];
        if let Ok(cp) = u32::from_str_radix(hex, 16)
            && is_forbidden_pictographic(cp)
        {
            return Some(format!("\\u{{{hex}}}"));
        }
        from = start + close_rel + 1;
    }
    None
}

#[test]
fn no_raw_pictographic_glyph() {
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let mut violations = Vec::new();
    for target in GLYPH_SCAN_ROOTS {
        collect_violations(root, target, &violating_glyph, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "host UI 소스에 raw 픽토그래픽 글리프(이모지 U+1F000–1FAFF / 딩뱃 U+2700–27BF)가 \
         재유입됨 — SVG line-icon(`icons::*`)으로 바꿀 것. 리터럴·`\\u{{}}` 양형태 모두 금지:\n{}",
        violations.join("\n")
    );
}

#[test]
fn no_primitive_color_field_access_in_host_ui() {
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let mut violations = Vec::new();
    for target in COLOR_SCAN_ROOTS {
        collect_violations(root, target, &violating_color, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "host UI 에 primitive 색 필드 직접 접근이 재유입됨 — semantic 접근자\
         (accent_*/surface_*/text_*/border_* 등)로 바꿀 것:\n{}",
        violations.join("\n")
    );
}

/// 정책 예외는 경로의 존재만 확인한다. 현재 일치 건수가 없다고 필요 없어진 예외로 단정하지 않는다.
#[test]
fn allowlist_prefixes_point_at_paths_that_exist() {
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let missing = tasty_doc_guards::missing_referents(
        root,
        ALLOWLIST_PREFIXES.iter().map(|(rel, _, _)| *rel),
    );
    assert!(
        missing.is_empty(),
        "면제가 없는 경로를 가리킨다 — 옮겼으면 항목도 옮기고, 사라졌으면 항목을 지워라: {missing:?}"
    );
}

/// 무관한 줄 이동으로 기준이 바뀌지 않도록 (파일, 접두어, 값)으로 기록한다.
fn length_setter_literals(root: &Path) -> Vec<(String, &'static str, String)> {
    length_setter_literals_under(
        root,
        LENGTH_SETTER_SCAN_ROOTS,
        MIN_LENGTH_SETTER_SCANNED_FILES,
    )
}

/// 작은 합성 트리도 검사할 수 있도록 루트와 하한을 인자로 받는다.
fn length_setter_literals_under(
    root: &Path,
    scan_roots: &[&'static str],
    floor: usize,
) -> Vec<(String, &'static str, String)> {
    let mut out = Vec::new();
    let mut scanned = 0usize;
    for target in scan_roots {
        let path = root.join(target);
        let mut files = Vec::new();
        gather_rs_files(&path, &mut files);
        assert!(
            !files.is_empty(),
            "스캔 루트 `{target}` 에서 .rs 파일을 하나도 찾지 못했다 — 조용한 미스캔은 \
             위양성보다 나쁘다"
        );
        scanned += files.len();
        for file in files {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let contents = std::fs::read_to_string(&file).expect("소스 파일 read 실패");
            for line in contents.lines() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for &prefix in LENGTH_SETTER_PREFIXES {
                    let mut from = 0;
                    while let Some(idx) = line[from..].find(prefix) {
                        let start = from + idx;
                        from = start + prefix.len();
                        // `set_max_height(` 안의 `max_height(` 를 두 번 세지 않는다.
                        if line[..start].chars().next_back().is_some_and(is_word_char) {
                            continue;
                        }
                        let tail = &line[from..];
                        let value: String = tail
                            .chars()
                            .take_while(|c| c.is_ascii_digit() || *c == '.')
                            .collect();
                        // 접두어 바로 뒤가 숫자가 아니면 여기서는 판정하지 않는다.
                        if value.is_empty() || !value.starts_with(|c: char| c.is_ascii_digit()) {
                            continue;
                        }
                        out.push((rel.clone(), prefix, value));
                    }
                }
            }
        }
    }
    assert!(
        scanned >= floor,
        "스캔 파일이 {scanned}개로 하한 {floor}보다 적다. 각 루트가 빈 경우는 앞에서 검사하므로 전체 감소와 일부 누락을 확인한다. 루트가 이동했다면 LENGTH_SETTER_SCAN_ROOTS를 갱신한다. 수집 오류를 하한 변경으로 통과시키지 않는다."
    );
    out.sort();
    out
}

/// 리터럴 집합을 양방향으로 대조해 새 값과 제거된 값의 오래된 예외를 함께 찾는다.
#[test]
fn no_new_length_literal_at_call_sites() {
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let found = length_setter_literals(root);
    let mut expected: Vec<(String, &str, String)> = LENGTH_SETTER_BASELINE
        .iter()
        .map(|(rel, prefix, value)| ((*rel).to_string(), *prefix, (*value).to_string()))
        .collect();
    expected.sort();

    let show = |v: &[(String, &str, String)]| {
        v.iter()
            .map(|(r, p, val)| format!("  {r} — `{p}{val})`"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let added: Vec<_> = found
        .iter()
        .filter(|x| !expected.contains(x))
        .cloned()
        .collect();
    let gone: Vec<_> = expected
        .iter()
        .filter(|x| !found.contains(x))
        .cloned()
        .collect();

    assert!(
        added.is_empty(),
        "호출부에 길이 리터럴이 새로 들어왔다 — 본체는 egui zoom_factor 를 1.0 으로 \
         고정하므로 이 숫자는 `ui_scale` 을 안 탄다. `Theme` 의 `field_width_*` 등 \
         대응 토큰을 넘겨라(docs/concepts/typed-length.md):\n{}",
        show(&added)
    );
    assert!(
        gone.is_empty(),
        "한시 목록의 리터럴이 소스에서 사라졌다. 수정된 항목은 목록에서도 제거한다:\n{}",
        show(&gone)
    );
}

/// 접두어 목록에서 긴 이름이 짧은 이름을 포함하는 짝을 찾아, 같은 호출을 중복 수집하지 않는지 확인한다.
#[test]
fn the_walk_and_the_length_reader_answer_on_a_substituted_tree() {
    let (outer, inner) = LENGTH_SETTER_PREFIXES
        .iter()
        .find_map(|o| {
            LENGTH_SETTER_PREFIXES
                .iter()
                .find(|i| *i != o && o.ends_with(**i))
                .map(|i| (*o, *i))
        })
        .expect("서로 감싸는 접두 짝이 명부에 없다 — 이 대조가 겨냥하는 형태가 사라졌다");

    let probe = Scratch::new("design-token-reader");
    let root = probe.path();
    std::fs::create_dir_all(root.join("zone/deep")).expect("합성 트리를 만들지 못했다");

    std::fs::write(
        root.join("zone/a.rs"),
        format!(
            "fn build(ui: &mut Ui) {{\n    \
                 ui.{outer}12.5);\n    \
                 ui.{inner}8.0);\n    \
                 // ui.{outer}99.0);\n    \
                 ui.{outer}theme.gap);\n\
             }}\n"
        ),
    )
    .expect("합성 소스 실패");
    std::fs::write(
        root.join("zone/deep/b.rs"),
        format!("fn deep(ui: &mut Ui) {{\n    ui.{inner}120.0);\n}}\n"),
    )
    .expect("합성 하위 소스 실패");
    std::fs::write(
        root.join("zone/notes.md"),
        format!("문서가 `ui.{outer}42.0)` 를 인용한다\n"),
    )
    .expect("합성 문서 실패");

    let mut files = Vec::new();
    gather_rs_files(&root.join("zone"), &mut files);
    let mut rels: Vec<String> = files
        .iter()
        .map(|p| {
            repo_relative(p.strip_prefix(root).unwrap_or(p))
                .display()
                .to_string()
        })
        .collect();
    rels.sort();
    assert_eq!(
        rels,
        vec!["zone/a.rs".to_string(), "zone/deep/b.rs".to_string()],
        "순회가 합성 트리에서 다른 답을 냈다"
    );
    assert!(
        !rels.iter().any(|r| r.ends_with(".md")),
        "`.rs` 가 아닌 파일을 모았다 — 이 접두를 **인용만** 하는 문서가 판정에 들어온다"
    );

    // 실제 UI 파일 수가 아닌 합성 트리의 하한을 넘긴다.
    let got = length_setter_literals_under(root, &["zone"], 2);
    assert_eq!(
        got,
        vec![
            ("zone/a.rs".to_string(), inner, "8.0".to_string()),
            ("zone/a.rs".to_string(), outer, "12.5".to_string()),
            ("zone/deep/b.rs".to_string(), inner, "120.0".to_string()),
        ],
        "길이 추출이 합성 트리에서 다른 답을 냈다"
    );
    assert_eq!(
        got.iter()
            .filter(|(rel, _, v)| rel == "zone/a.rs" && v == "12.5")
            .count(),
        1,
        "긴 접두어 안의 짧은 접두어까지 같은 호출로 중복 집계했다"
    );
    assert!(
        !got.iter().any(|(_, _, v)| v == "99.0"),
        "주석 줄의 리터럴을 셌다"
    );
    assert!(
        !got.iter().any(|(_, _, v)| v.is_empty() || v == "42.0"),
        "토큰 인자(또는 문서의 인용)를 리터럴로 셌다 — 이 축이 원하는 형태가 토큰이다"
    );
    assert!(
        got.iter().any(|(_, _, v)| v == "12.5"),
        "소수점 뒤를 잘라 냈다 — 값이 달라지면 기준선과 영영 안 맞는다"
    );
}
