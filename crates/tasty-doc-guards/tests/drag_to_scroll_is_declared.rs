//! 스크롤 빌더에 drag_to_scroll 설정이 명시됐는지 검사한다. true·false 값은 판정하지 않는다.
//! 데스크톱의 텍스트·행 선택과 파일 드래그가 패닝과 충돌하지 않도록 기본 정책은 끄는 것이다.
//! 정책은 docs/design/systems/theme.md의 UI 디자인 규칙을 따른다.
//! ScrollArea·TableBuilder와 스크롤 설정 메서드를 호출한 Window를 대상으로 한다.
//! Window의 스크롤 설정 인자도 해석하지 않으므로 false 설정도 검사 대상이 된다.
//!
//! 생성 호출부터 40줄 안의 첫 종결 메서드까지 문자열을 읽으며 빌더의 소유 관계는 추적하지 않는다.
//! 변수에 담아도 이 범위 안에 종결자가 있으면 검사한다. 종결자를 못 찾으면 검사하지 않는다.
//! 무관한 show 호출이 앞에 끼면 실제 설정을 보지 못하고 오류로 보고할 수 있다.
//! 별칭·래퍼 호출, ComboBox처럼 라이브러리 내부에서 만드는 스크롤은 검사하지 못한다.
//! doc-guards.yml의 경로 필터 없는 develop·release·main push·PR 검사에서 실행된다. 전체 구성은 docs/dev-guide/ci-gates.md를 따른다.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tasty_doc_guards::cfg_predicate::cfg_gated_lines;
use tasty_doc_guards::shipping_scope::test_only_files;
use tasty_doc_guards::source_text::{mask_non_code, rust_sources};

const SCAN_ROOTS: &[&str] = &["src", "crates"];

/// ScrollArea 생성자를 철자로 찾는다.
const SCROLL_AREA_CTORS: &[&str] = &[
    "ScrollArea::vertical(",
    "ScrollArea::horizontal(",
    "ScrollArea::both(",
    "ScrollArea::neither(",
    "ScrollArea::new(",
];

/// `ScrollArea` 빌더 체인의 종결자.
const SCROLL_AREA_ENDS: &[&str] = &[".show(", ".show_viewport(", ".show_rows("];

/// `TableBuilder` 빌더 체인의 종결자.
const TABLE_ENDS: &[&str] = &[".header(", ".body("];

/// Window의 스크롤 설정 메서드. 인자 값과 무관하게 호출이 있으면 검사한다.
const WINDOW_SCROLL_ON: &[&str] = &[".scroll(", ".scroll2(", ".hscroll(", ".vscroll("];

/// `Window` 빌더 체인의 종결자.
const WINDOW_ENDS: &[&str] = &[".show(", ".open("];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<crate> 위가 레포 루트다")
        .to_path_buf()
}

/// 마스킹된 소스와 test 구간 표를 받아 위반의 줄 번호·생성자를 반환한다.
fn violations(
    masked: &str,
    gated: &[bool],
    ctors: &[&str],
    ends: &[&str],
    require: Option<&[&str]>,
) -> Vec<(usize, String)> {
    let lines: Vec<&str> = masked.split('\n').collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(ctor) = ctors.iter().find(|c| line.contains(**c)) else {
            continue;
        };
        if gated.get(i).copied().unwrap_or(false) {
            continue; // 인라인 #[cfg(test)] — test 전용
        }
        // 40줄 안에 종결자가 없으면 판정하지 않는다.
        let mut end = None;
        for (j, l) in lines.iter().enumerate().skip(i).take(40) {
            let after = if j == i {
                let at = l.find(*ctor).expect("방금 찾았다") + ctor.len();
                &l[at..]
            } else {
                l
            };
            if ends.iter().any(|e| after.contains(*e)) {
                end = Some(j);
                break;
            }
        }
        let Some(end) = end else { continue };
        let block = lines[i..=end].join("\n");
        if let Some(needles) = require
            && !needles.iter().any(|n| block.contains(*n))
        {
            continue;
        }
        if !block.contains("drag_to_scroll") {
            out.push((i + 1, (*ctor).to_string()));
        }
    }
    out
}

/// 출하되는 `.rs`의 `(경로, 마스킹된 본문, cfg(test) 줄 표)`.
type ShippingSource = (PathBuf, String, Vec<bool>);

/// 같은 테스트 프로세스의 검사들은 한 번 분석한 저장소 입력을 공유한다.
fn shipping_sources() -> &'static [ShippingSource] {
    static SOURCES: OnceLock<Vec<ShippingSource>> = OnceLock::new();
    SOURCES.get_or_init(collect_shipping_sources)
}

fn collect_shipping_sources() -> Vec<ShippingSource> {
    let root = repo_root();
    let sources = rust_sources(&root, SCAN_ROOTS);
    let test_only = test_only_files(&root, &sources);
    sources
        .into_iter()
        .filter(|(rel, _)| !test_only.contains(rel))
        .map(|(rel, text)| {
            let masked = mask_non_code(&text);
            let lines: Vec<&str> = masked.split('\n').collect();
            let gated = cfg_gated_lines(&lines, "test");
            (rel, masked, gated)
        })
        .collect()
}

fn report(kind: &str, hits: &[(PathBuf, usize, String)]) -> String {
    let mut s = format!(
        "{kind}에서 drag_to_scroll 설정이 없는 위치 {}:\n",
        hits.len()
    );
    for (rel, line, ctor) in hits {
        s.push_str(&format!("  {}:{line}  {ctor}\n", rel.display()));
    }
    s.push_str(
        "\n해당 빌더에 .drag_to_scroll(false)를 명시한다. 패닝을 유지할 이유가 있으면 true를 명시한다. 이 검사는 값이 아닌 설정의 존재만 확인한다. 정책: docs/design/systems/theme.md의 UI 디자인 규칙.\n",
    );
    s
}

#[test]
fn every_scroll_area_declares_drag_to_scroll() {
    let mut hits = Vec::new();
    for (rel, masked, gated) in shipping_sources() {
        for (line, ctor) in violations(masked, gated, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None) {
            hits.push((rel.clone(), line, ctor));
        }
    }
    hits.sort();
    assert!(hits.is_empty(), "{}", report("ScrollArea", &hits));
}

#[test]
fn every_table_builder_declares_drag_to_scroll() {
    let mut hits = Vec::new();
    for (rel, masked, gated) in shipping_sources() {
        for (line, ctor) in violations(masked, gated, &["TableBuilder::new("], TABLE_ENDS, None) {
            hits.push((rel.clone(), line, ctor));
        }
    }
    hits.sort();
    assert!(hits.is_empty(), "{}", report("TableBuilder", &hits));
}

/// 실제 Window 입력이 없어도 합성 입력에서 설정 유무 판정을 확인한다.
#[test]
fn every_scrolling_window_declares_drag_to_scroll() {
    let mut hits = Vec::new();
    for (rel, masked, gated) in shipping_sources() {
        for (line, ctor) in violations(
            masked,
            gated,
            &["Window::new("],
            WINDOW_ENDS,
            Some(WINDOW_SCROLL_ON),
        ) {
            hits.push((rel.clone(), line, ctor));
        }
    }
    hits.sort();
    assert!(
        hits.is_empty(),
        "{}",
        report("스크롤 켠 egui::Window", &hits)
    );
}

fn probe(src: &str, ctors: &[&str], ends: &[&str], require: Option<&[&str]>) -> Vec<usize> {
    let masked = mask_non_code(src);
    let lines: Vec<&str> = masked.split('\n').collect();
    let gated = cfg_gated_lines(&lines, "test");
    violations(&masked, &gated, ctors, ends, require)
        .into_iter()
        .map(|(l, _)| l)
        .collect()
}

#[test]
fn a_bare_scroll_area_is_a_violation_and_declaring_it_is_not() {
    let bare = "fn f(ui: &mut Ui) {\n    egui::ScrollArea::vertical().show(ui, |_| {});\n}\n";
    assert_eq!(
        probe(bare, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None),
        vec![2]
    );

    let off = "fn f(ui: &mut Ui) {\n    egui::ScrollArea::vertical()\n        \
               .drag_to_scroll(false)\n        .show(ui, |_| {});\n}\n";
    assert!(
        probe(off, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None).is_empty(),
        "끈 자리가 위반으로 잡혔다"
    );

    let on = off.replace("drag_to_scroll(false)", "drag_to_scroll(true)");
    assert!(
        probe(&on, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None).is_empty(),
        "명시한 true 가 위반으로 잡혔다 — 이 가드는 값을 보지 않는다"
    );
}

#[test]
fn a_scroll_area_named_in_a_comment_is_not_a_violation() {
    let doc = "/// 가로 스크롤. [`egui::ScrollArea::horizontal`] 로 감싼다.\n\
               /// 그 안에서 .show( 를 부른다.\nfn f() {}\n";
    assert!(
        probe(doc, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None).is_empty(),
        "doc 주석 안의 인용이 위반으로 잡혔다 — mask_non_code 가 안 걸렸다"
    );

    let string = "fn f() {\n    let s = \"egui::ScrollArea::vertical().show(\";\n}\n";
    assert!(
        probe(string, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None).is_empty(),
        "문자열 안의 철자가 위반으로 잡혔다"
    );
}

#[test]
fn an_inline_cfg_test_scroll_area_is_not_a_violation() {
    let src = "fn f() {}\n\n#[cfg(test)]\nmod tests {\n    fn g(ui: &mut Ui) {\n        \
               egui::ScrollArea::vertical().show(ui, |_| {});\n    }\n}\n";
    assert!(
        probe(src, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None).is_empty(),
        "인라인 cfg(test) 안의 자리가 위반으로 잡혔다 — test 코드 제외 판정이 안 걸렸다"
    );
}

#[test]
fn the_table_builder_axis_is_actually_wired() {
    let bare = "fn f(ui: &mut Ui) {\n    TableBuilder::new(ui)\n        .striped(true)\n        \
                .header(20.0, |_| {});\n}\n";
    assert_eq!(
        probe(bare, &["TableBuilder::new("], TABLE_ENDS, None),
        vec![2]
    );

    let off = bare.replace(".striped(true)", ".drag_to_scroll(false)");
    assert!(
        probe(&off, &["TableBuilder::new("], TABLE_ENDS, None).is_empty(),
        "끈 TableBuilder 가 위반으로 잡혔다"
    );
}

#[test]
fn the_window_axis_is_actually_wired() {
    let scrolling = "fn f(ctx: &Context) {\n    Window::new(\"x\")\n        \
                     .scroll(true)\n        .show(ctx, |_| {});\n}\n";
    assert_eq!(
        probe(
            scrolling,
            &["Window::new("],
            WINDOW_ENDS,
            Some(WINDOW_SCROLL_ON)
        ),
        vec![2],
        "축을 켠 Window 를 안 본다"
    );

    let plain = "fn f(ctx: &Context) {\n    Window::new(\"x\")\n        \
                 .show(ctx, |_| {});\n}\n";
    assert!(
        probe(
            plain,
            &["Window::new("],
            WINDOW_ENDS,
            Some(WINDOW_SCROLL_ON)
        )
        .is_empty(),
        "축을 안 켠 Window 가 위반으로 잡혔다 — 실재하지 않는 위반이다"
    );

    // 추가 줄을 원문으로 검사하는 egui 창 사용 가드가 합성 입력을 실제 사용으로 오해하지 않도록 경로를 조립한다.
    let qualified = scrolling.replace("Window::new(", concat!("egui", "::Window::new("));
    assert_eq!(
        probe(
            &qualified,
            &["Window::new("],
            WINDOW_ENDS,
            Some(WINDOW_SCROLL_ON)
        ),
        vec![2],
        "경로 수식이 붙은 실제 형태를 안 본다"
    );

    for m in WINDOW_SCROLL_ON {
        let src = scrolling.replace(".scroll(true)", &format!("{m}true)"));
        assert_eq!(
            probe(&src, &["Window::new("], WINDOW_ENDS, Some(WINDOW_SCROLL_ON)).len(),
            1,
            "`{m}` 를 축으로 안 센다"
        );
    }
}

/// 현재 판독이 놓치거나 오탐하는 형태도 검증해 범위를 바꿀 때 설명을 함께 재검토한다.
#[test]
fn the_documented_blind_spots_are_measured_not_guessed() {
    let var = "fn f(ui: &mut Ui) {\n    let mut sa = egui::ScrollArea::vertical();\n    \
               if x {\n        sa = sa.max_height(10.0);\n    }\n    sa.show(ui, |_| {});\n}\n";
    assert_eq!(
        probe(var, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None),
        vec![2],
        "변수에 담은 빌더의 범위 판정이 달라졌다. 검사 한계 설명을 함께 확인한다."
    );
    let var_ok = var.replace(".max_height(10.0)", ".drag_to_scroll(false)");
    assert!(probe(&var_ok, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None).is_empty());

    let truncated = "fn f(ui: &mut Ui) {\n    let mut sa = egui::ScrollArea::vertical();\n    \
                     other.show(ui);\n    sa = sa.drag_to_scroll(false);\n    \
                     sa.show(ui, |_| {});\n}\n";
    assert_eq!(
        probe(truncated, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None),
        vec![2],
        "무관한 종결자에서 구간이 끊기지 않았다. 기존 오탐 설명을 재검토한다."
    );

    let alias = "use egui::ScrollArea as SA;\nfn f(ui: &mut Ui) {\n    SA::vertical().show(ui, |_| {});\n}\n";
    assert!(
        probe(alias, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None).is_empty(),
        "별칭을 검출하도록 범위가 달라졌다. 한계 설명을 갱신한다."
    );

    let wrapper = "fn my_scroll() -> egui::ScrollArea {\n    egui::ScrollArea::vertical().drag_to_scroll(false)\n}\n\
                   fn f(ui: &mut Ui) {\n    my_scroll().show(ui, |_| {});\n}\n";
    assert!(
        probe(wrapper, SCROLL_AREA_CTORS, SCROLL_AREA_ENDS, None).is_empty(),
        "래퍼 경유 호출을 검출하도록 범위가 달라졌다. 한계 설명을 갱신한다."
    );
}

#[test]
fn the_scan_actually_reaches_the_repo() {
    let sources = shipping_sources();
    assert!(
        sources.len() > 500,
        "스캔이 {} 개만 훑었다 — SCAN_ROOTS 를 의심해라",
        sources.len()
    );
    let seen: usize = sources
        .iter()
        .map(|(_, masked, _)| {
            SCROLL_AREA_CTORS
                .iter()
                .map(|c| masked.matches(*c).count())
                .sum::<usize>()
        })
        .sum();
    assert!(
        seen > 40,
        "코드에서 ScrollArea 생성 호출을 {seen} 개만 봤다 — 마스킹이나 스캔이 과하다"
    );
}
