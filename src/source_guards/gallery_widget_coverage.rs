//! **재수출된 공용 위젯을 갤러리가 한 번이라도 부르는가.**
//!
//! 자매 가드 [`super::gallery_specimen_parity`] 는 popup 과 무대를 덮으면서 **공용 위젯은
//! 못 덮는다**고 자기 문서에 적어 두었다 — 등록처가 없었기 때문이다. 여기가 그 자리다.
//! 왼쪽은 `crates/tasty-ui-widgets/src/lib.rs` 의 재수출 목록이고, 그 목록은 **세려고 만든
//! 표가 아니다**: 밖에서 그 위젯을 쓰려면 그 줄이 있어야 하고 빠지면 컴파일이 깨진다.
//! 다른 이유로 존재하고 없으면 시끄러운 좌변만 좌변으로 쓴다.
//!
//! # 명제는 좁다 — 이름표를 그보다 넓게 달지 않는다
//!
//! > **재수출된 모듈의 항목 중 적어도 하나를 갤러리 소스가 이름으로 부른다.**
//!
//! "그 위젯 전용 카드가 있다" 가 아니고, "갤러리가 그것을 **그린다**" 도 아니다. 판정기가
//! 보는 것은 마스킹한 갤러리 소스에 그 이름이 **코드로 나오는가**뿐이다. 그래서 테스트
//! 이름도 `..._is_named_by_the_gallery` 다 — "drawn" 이나 "완전성" 이라고 부르면 그 수는
//! 명제의 수가 아니게 된다.
//!
//! # 오늘 빚이 0 이라 **레포 실측만으로는 바늘이 죽어도 초록이다**
//!
//! 재수출 모듈 중 안 불리는 것이 **오늘 0** 이다(2026-09-06 실측). 그러니 이 파일의 무게는 레포 판정이 아니라
//! 아래 [`detector`] 의 합성 대조에 있다 — 판정기를 직접 쳐서 **빨개져야 할 때 빨개지는
//! 것**을 본다. 레포 쪽에는 하한과 대조 항목을 걸어, 추출이 죽어 0 개가 나오면 공허한
//! 초록 대신 빨강이 되게 한다.
//!
//! # 이 가드가 답하지 못하는 것 (산문으로 두지 않고 여기 박는다)
//!
//! - **"부른다" 와 "그린다" 사이의 틈은 안 닫힌다.** 갤러리가 `use` 만 해 두고 한 번도
//!   그리지 않아도 여기서는 참이다. 린트가 막아 주지도 않는다 — 워크스페이스는
//!   `unused_must_use`·`dead_code` 를 deny 로 박지만 `unused_imports` 는 목록에 없고
//!   CI 에 `-D warnings` 도 없다(확인함). 닫으려면 호출 그래프가 필요하고 텍스트
//!   판정기로는 못 한다.
//! - **항목 단위로는 안 묻는다.** 76 항목 중 21 개(11 모듈)가 안 불린다(2026-09-06 실측).
//!   전부
//!   출력/동작 타입(`ListCtrlOutput` · `TableOutput` · `StatusBarAction` 등)이다. 항목
//!   단위로 물으면 그 21 이 빚이 되는데, 그건 "갤러리가 그 위젯을 보여주는가" 가 아니라
//!   "그 타입 이름을 쓰는가" 라 명제가 아니다.
//! - **`tasty-ui-widgets` 하나만 본다.** 다른 크레이트가 공용 위젯을 내놓기 시작하면
//!   좌변이 하나 더 필요하다.
//!
//! # 찍는 표에는 자동 채널이 없다
//!
//! `println!` 은 통과한 테스트에서 삼켜지고 이 레포의 어느 회차 스텝도 `--nocapture` 를
//! 안 쓴다. 자동으로 지키는 것은 **단정**이고 표는 손으로 볼 때의 도구다.

use tasty_doc_guards::source_text::mask_non_code;

/// 좌변 — 재수출 목록이 사는 곳.
const WIDGETS_LIB: &str = "crates/tasty-ui-widgets/src/lib.rs";

/// 우변 — 갤러리 소스 전부.
const GALLERY_SRC: &str = "crates/tasty-gallery/src";

/// `pub mod` 로 노출된 것들의 **명부**.
///
/// 여기가 이 가드의 조용한 샛길이다. `pub use` 에서 빼고 `pub mod` 로 노출하면
/// `tasty_ui_widgets::foo::Bar` 로 여전히 쓸 수 있으면서 좌변에서는 사라진다 — 빨강을 끄는
/// 비용이 0 이 되고 보호만 없어진다. 그래서 `pub mod` 집합을 자리로 못 박는다. 새 `pub mod`
/// 가 생기면 그 자리에서 실패하고, 위젯이면 재수출로 내보내라고 요구한다.
const PUB_MODULES: &[(&str, &str)] = &[
    ("brand", "브랜드 자산(로고 등) 모듈 — 그릴 위젯이 아니다"),
    (
        "file_handler",
        "picker 의 모델 규칙(말줄임 · 문자 예산) — 순수 함수지 위젯이 아니다.          본체와 갤러리 specimen 이 같은 판정을 부르라고 여기 있다",
    ),
    (
        "crumb_alloc",
        "breadcrumb 의 폭 배분 계획기 — 잰 값으로 무엇을 접을지 정하는 순수 함수지 \
         위젯이 아니다. 본체 path bar 와 갤러리 specimen 이 같은 계획을 부르라고 \
         여기 있다(`file_handler` 와 같은 갈래다)",
    ),
    ("tokens", "레이아웃 상수 모듈 — 값이지 위젯이 아니다"),
];

/// 위젯이 아니어서 명제에서 뺀 재수출. **부류가 아니라 자리로 적는다.**
///
/// 오늘 비어 있다 — 헬퍼성 재수출(`ControlSize` · `hspace` 등)도 전부 갤러리가 부르기
/// 때문이다. 거짓이 되는 날 여기에 자리와 사유를 적고 아래 수를 함께 올린다.
const NOT_A_WIDGET: &[(&str, &str)] = &[];

/// 마스킹한 소스에서 `pub use <모듈>::{...};` / `pub use <모듈>::<항목>;` 을 모은다.
///
/// 순수 함수다 — 합성 입력을 그대로 먹여 대조한다.
fn exported_modules(masked_lib: &str) -> Vec<(String, Vec<String>)> {
    let flat: String = masked_lib.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut rest = flat.as_str();
    while let Some(at) = rest.find("pub use ") {
        rest = &rest[at + "pub use ".len()..];
        let Some(end) = rest.find(';') else { break };
        let decl = &rest[..end];
        rest = &rest[end + 1..];
        let Some((module, tail)) = decl.split_once("::") else {
            continue;
        };
        let module = module.trim();
        if module.is_empty()
            || !module
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
        {
            continue;
        }
        let items: Vec<String> = tail
            .trim()
            .trim_start_matches('{')
            .trim_end_matches('}')
            .split(',')
            // `X as Y` 로 내보내면 **밖에서 보이는 이름은 `Y`** 다. 별칭을 안 벗기면 항목이
            // "X as Y" 라는 한 덩어리가 되어 갤러리가 `Y` 를 그려도 못 찾는다 — 오늘 이
            // 형태가 0 이라 실측으로는 안 드러나고, 처음 쓰는 사람에게 거짓 빨강이 된다.
            .map(|s| s.rsplit(" as ").next().unwrap_or(s).trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if items.is_empty() {
            continue;
        }
        out.push((module.to_string(), items));
    }
    out
}

/// 마스킹한 갤러리 사본에 그 이름들 중 하나라도 **코드로** 나오는가.
fn named(items: &[String], masked_gallery: &str) -> bool {
    items.iter().any(|i| contains_word(masked_gallery, i))
}

/// 부분 문자열이 아니라 **낱말**로 나오는가 — `Button` 이 `ButtonVariant` 에 걸리지 않게.
fn contains_word(hay: &str, needle: &str) -> bool {
    let bytes = hay.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = hay[from..].find(needle) {
        let s = from + rel;
        let e = s + needle.len();
        let before_ok = s == 0 || !is_ident(bytes[s - 1]);
        let after_ok = e == bytes.len() || !is_ident(bytes[e]);
        if before_ok && after_ok {
            return true;
        }
        from = s + 1;
    }
    false
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `pub mod <이름>;` 을 모은다.
fn pub_modules(masked_lib: &str) -> Vec<String> {
    masked_lib
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub mod "))
        .filter_map(|r| r.split(';').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn masked(rel: &str) -> String {
    mask_non_code(&std::fs::read_to_string(super::repo_root().join(rel)).unwrap_or_default())
}

/// 갤러리 소스 전부를 마스킹해 한 덩어리로.
///
/// **자기 손으로 순회하지 않는다.** 직접 `read_dir` 을 돌면 그 순회가 조용히 좁아져도
/// (경로 오타 · 심볼릭 링크 · 권한) "안 불리는 위젯 0" 이 언제나 참이 된다 — 0 회 순회는
/// 0 건 발견과 구별되지 않는다. 공용 스캐너를 쓰면 그 모수가
/// [`super::scan_population`] 의 **집합 동등**(git 목록 대조)으로 이미 못 박혀 있어,
/// 파일이 조용히 빠지면 그쪽이 이름으로 말한다.
fn masked_gallery() -> String {
    let mut blob = String::new();
    let mut files = 0usize;
    for (rel, src) in super::rust_sources() {
        if !rel.starts_with(GALLERY_SRC) {
            continue;
        }
        blob.push_str(&mask_non_code(&src));
        blob.push('\n');
        files += 1;
    }
    // 공용 스캐너가 살아 있어도 **접두사가 틀리면** 여기만 0 이 된다. 그 0 은 조용하다.
    assert!(
        files >= 80,
        "갤러리 소스를 {files} 개밖에 못 골랐다(하한 80) — `{GALLERY_SRC}` 접두사가 \
         트리와 안 맞으면 스캐너가 성해도 이 판정만 공허해진다"
    );
    blob
}

#[test]
fn every_exported_widget_module_is_named_by_the_gallery() {
    let lib = masked(WIDGETS_LIB);
    let exported = exported_modules(&lib);

    // 바늘이 죽으면 "전부 참" 이 공허하게 성립한다. 그 0 은 초록보다 조용하다.
    assert!(
        exported.len() >= 20,
        "재수출 모듈이 {} 개다(하한 20) — `{WIDGETS_LIB}` 를 못 읽었거나 추출이 깨졌다. \
         아래 판정은 전부 공허하다",
        exported.len()
    );
    assert!(
        exported.iter().any(|(m, _)| m == "button"),
        "재수출 목록에 대조 항목 `button` 이 없다 — 추출이 맞는지부터 의심하라"
    );

    let blob = masked_gallery();
    assert!(
        // 파일 수 하한은 `masked_gallery` 가 든다. 여기는 **읽힌 내용**의 하한이다 —
        // 파일 수가 맞아도 내용이 비면(권한·인코딩) 이름 대조가 통째로 공허해진다.
        // 실측 1.23MB 의 절반 아래로 잡아 부분 읽기는 잡고 정상적인 축소엔 안 걸리게.
        blob.len() > 500_000,
        "갤러리 소스를 {} 바이트밖에 못 읽었다 — 경로가 옮겨졌으면 `{GALLERY_SRC}` 를 고쳐라",
        blob.len()
    );

    let mut missing = Vec::new();
    let mut listed = Vec::new();
    for (module, items) in &exported {
        if let Some((_, why)) = NOT_A_WIDGET.iter().find(|(m, _)| m == module) {
            listed.push(format!("  {module:<22} 위젯 아님 — {why}"));
            continue;
        }
        if named(items, &blob) {
            let hit = items
                .iter()
                .find(|i| contains_word(&blob, i))
                .cloned()
                .unwrap_or_default();
            listed.push(format!("  {module:<22} → {hit}"));
        } else {
            missing.push(format!("  {module:<22} 항목 {items:?}"));
        }
    }

    assert!(
        missing.is_empty(),
        "갤러리가 한 번도 부르지 않는 공용 위젯이 {} 개다:\n{}\n\n\
         gallery-first(ADR-0510)는 본체보다 갤러리가 먼저다 — 갤러리 카탈로그에서 그리거나, \
         위젯이 아니라는 **사유와 함께** `NOT_A_WIDGET` 명부에 그 자리를 올려라.\n\
         ★ `pub use` 에서 빼고 `pub mod` 로 옮기는 것은 **이행이 아니다** — 밖에서는 그대로 \
         쓸 수 있는데 이 가드만 못 보게 된다. 그 길은 아래 `pub mod` 명부가 막는다.",
        missing.len(),
        missing.join("\n")
    );

    println!(
        "[widgets] 재수출 {} 모듈 · 항목 {}\n{}",
        exported.len(),
        exported.iter().map(|(_, i)| i.len()).sum::<usize>(),
        listed.join("\n")
    );
}

#[test]
fn the_pub_module_escape_hatch_stays_closed() {
    let lib = masked(WIDGETS_LIB);
    let mods = pub_modules(&lib);
    assert!(
        !mods.is_empty(),
        "`pub mod` 을 하나도 못 찾았다 — 추출이 깨지면 이 명부가 공허해진다"
    );
    let unlisted: Vec<&String> = mods
        .iter()
        .filter(|m| !PUB_MODULES.iter().any(|(k, _)| *k == m.as_str()))
        .collect();
    assert!(
        unlisted.is_empty(),
        "명부에 없는 `pub mod` 이 있다: {unlisted:?}\n\
         `pub mod` 은 재수출 목록을 우회해 밖으로 나가는 길이라, 위젯이 그리로 나가면 \
         갤러리 판정이 통째로 비켜간다. **위젯이면 `pub use` 로 내보내고**, 위젯이 아니면 \
         사유와 함께 `PUB_MODULES` 에 올려라."
    );
    assert_eq!(
        mods.len(),
        PUB_MODULES.len(),
        "`pub mod` 이 {} 개인데 명부는 {} 행이다 — 줄었으면 명부에서도 빼라. \
         남는 행은 다음 사람에게 '여기는 이미 봤다' 는 거짓 신호가 된다",
        mods.len(),
        PUB_MODULES.len()
    );
}

/// 판정기를 합성 입력으로 직접 친다. 레포 빚이 0 이라 **여기가 이 가드의 본체다.**
#[cfg(test)]
mod detector {
    use super::*;

    const LIB: &str = "\
pub mod brand;
pub use button::{Button, ButtonVariant};
pub use ghost::{GhostWidget};
";

    #[test]
    fn a_widget_the_gallery_never_names_is_reported() {
        let ex = exported_modules(LIB);
        let blob = "fn demo() { Button::new(); }";
        let miss: Vec<&String> = ex
            .iter()
            .filter(|(_, items)| !named(items, blob))
            .map(|(m, _)| m)
            .collect();
        assert_eq!(
            miss,
            vec!["ghost"],
            "안 불리는 모듈을 못 짚으면 이 가드는 아무것도 안 본다"
        );
    }

    #[test]
    fn a_widget_the_gallery_names_is_not_reported() {
        let ex = exported_modules(LIB);
        let blob = "fn demo() { Button::new(); GhostWidget::show(); }";
        assert!(
            ex.iter().all(|(_, items)| named(items, blob)),
            "다 불리는데도 빨개지면 오탐이다"
        );
    }

    #[test]
    fn a_name_that_only_appears_in_a_comment_does_not_count() {
        // 마스킹을 안 하면 주석·문자열에 적힌 이름이 "부른다" 로 세어진다.
        let raw = "// GhostWidget 은 나중에 그린다\nlet s = \"GhostWidget\";\n";
        let blob = mask_non_code(raw);
        let ex = exported_modules(LIB);
        let ghost = ex
            .iter()
            .find(|(m, _)| m == "ghost")
            .expect("합성 입력이 깨졌다");
        assert!(
            !named(&ghost.1, &blob),
            "주석·문자열 속 이름이 코드로 세어졌다 — 마스킹이 죽으면 이 가드는 조용히 통과한다"
        );
        assert!(
            named(&ghost.1, raw),
            "마스킹 전에는 걸려야 한다 — 안 걸리면 이 대조가 마스킹을 안 재고 있다"
        );
    }

    /// `X as Y` 는 밖에서 `Y` 로 보인다 — 갤러리는 `Y` 를 부른다.
    #[test]
    fn an_alias_is_counted_by_the_name_it_exports() {
        let ex = exported_modules("pub use ghost::{Inner as GhostWidget};");
        assert_eq!(
            ex,
            vec![("ghost".to_string(), vec!["GhostWidget".to_string()])]
        );
        assert!(named(&ex[0].1, "fn demo() { GhostWidget::show(); }"));
    }

    #[test]
    fn a_prefix_is_not_a_name() {
        assert!(!contains_word("ButtonVariant::Ghost", "Button"));
        assert!(contains_word("let b = Button::new();", "Button"));
    }

    #[test]
    fn an_empty_left_side_yields_nothing_which_is_why_the_repo_test_has_a_floor() {
        assert!(exported_modules("").is_empty());
        assert!(pub_modules("").is_empty());
    }
}
