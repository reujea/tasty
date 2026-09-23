//! `clippy.toml` 의 `disallowed-methods` 목록과 `theme.md` 의 "색 생성 정책" 절 차단 함수 표가
//! **같은 집합**인지 본다.
//!
//! ## 왜 이 자리인가 — 이름 둘을 세다가 나왔다
//!
//! `disallowed_methods`(밑줄, `[workspace.lints.clippy]` 의 **레벨**)와
//! `disallowed-methods`(하이픈, `clippy.toml` 의 **목록**)가 같은 것을 가리키는지
//! 물었다. **아니다 — 서로 다른 두 자리다.** 세어 보면 지배하는 것이 다르다:
//! 레벨은 lint 하나를, 목록은 메서드 여섯을 지배한다. 같은 파일의 다른 키
//! `cognitive-complexity-threshold` 가 lint 이름 `cognitive_complexity` 와 **다르다**는
//! 것이 근거다 — config 키와 lint 이름은 별개의 이름공간이고, 이 둘이 스템을 공유하는
//! 것은 그 명명의 우연이다. (문서 표에는 이미 레벨 행 하나뿐이고, 목록은 별도 소절이
//! 든다. 그러니 거기엔 부분 사본이 없다.)
//!
//! **부분 사본은 한 층 아래에 있었다.** `theme.md` 의 "색 생성 정책" 절이 차단 함수를 표로 다시
//! 적는데, 그 표는 손으로 베낀 사본이고 어떤 채널도 안 본다. 지금은 여섯을 다 든다 —
//! 그러나 일곱째가 `clippy.toml` 에 추가되는 날 그 표는 **조용히 여섯에 머문다.**
//! 빠진 행은 틀린 값이 아니라 없는 값이라 읽는 사람이 못 본다.
//!
//! ## 좌표는 (타입, 메서드) 쌍이다
//!
//! 전체 경로로 맞대면 두 자리의 표기가 다르다 — `clippy.toml` 은
//! `tasty_type_appearance::color::HexColor::from_rgb` 로 크레이트 경로까지 쓰고,
//! 문서는 `HexColor::from_rgb` 로 줄여 쓴다. 마지막 **두** 마디를 좌표로 삼으면 둘이
//! 만난다. 메서드 이름만 쓰면 안 된다 — `from_rgb` 가 `HexColor` 와 `Color32` 양쪽에
//! 있어서 여섯이 다섯으로 접힌다.
//!
//! ## 문서 쪽 축약을 편다
//!
//! 문서 표는 사람이 읽기 좋게 줄여 쓴다. 판독기가 그 축약을 그대로 편다:
//! `` `A::x` / `y` `` 는 타입을 이어받고(`A::y`), `A::from_rgba_{u,p}` 는 중괄호를
//! 편다. 편 결과가 우변과 안 맞으면 그때가 진짜 어긋남이다. 축약을 못 펴면
//! **조용히 건너뛰지 않고 죽는다** — 건너뛰면 그 항목이 좌변에서 사라지고, 우변에도
//! 없으면 양쪽이 맞아 통과한다.
//!
//! ## 안 보는 것
//!
//! - 대체 방법 열(오른쪽 칸)의 내용. 그것은 산문이라 짝지을 우변이 없다.
//! - `reason` 문자열의 정합. `clippy.toml` 쪽에만 있고 문서는 다른 말로 쓴다.
//! - 그 목록이 **실제로 발동하는지**. 그건 clippy 가 본다(레벨이 `deny` 다).
//! - 같은 함수를 언급하는 다른 문서. 좌변은 그 절의 **첫 표** 하나다.

use std::collections::BTreeSet;

const MANIFEST: &str = "clippy.toml";
const DOC: &str = "docs/design/systems/theme.md";
const SECTION: &str = "\n### clippy 강제 — disallowed-methods";

/// 좌우변이 이 아래로 떨어지면 수집이 죽은 것이다.
///
/// 값의 근거: 2026-09-08 실측 **6**(`clippy.toml` 의 배열 항목 수). 그 아래는 감소가
/// 아니라 판독기가 깨진 것이다. **이 수를 내려서 초록을 만들지 마라** — 양쪽이 다
/// 비면 아래 집합 비교는 전부 통과한다.
const MIN_ENTRIES: usize = 4;

/// `(타입, 메서드)` — 두 표기가 만나는 좌표.
type Coord = (String, String);

fn read(rel: &str) -> String {
    let path = tasty_doc_guards::repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("읽을 수 없다: {} — {e}", path.display()))
}

/// 경로의 마지막 두 마디를 좌표로 만든다.
fn coord(path: &str) -> Coord {
    let seg: Vec<&str> = path.split("::").collect();
    assert!(
        seg.len() >= 2,
        "`{path}` 에 `::` 가 없다 — (타입, 메서드) 좌표를 만들 수 없다"
    );
    (
        seg[seg.len() - 2].to_string(),
        seg[seg.len() - 1].to_string(),
    )
}

/// `clippy.toml` 의 `disallowed-methods` 배열에서 `path = "..."` 를 읽는다.
///
/// 순수 함수다 — 변이 테스트가 파일을 안 고치고 찌를 수 있어야 한다.
fn manifest_coords(toml: &str) -> BTreeSet<Coord> {
    let body = toml
        .split_once("disallowed-methods = [")
        .unwrap_or_else(|| panic!("`{MANIFEST}` 에서 `disallowed-methods` 배열을 못 찾았다"))
        .1;
    let body = body
        .split_once("\n]")
        .unwrap_or_else(|| panic!("`{MANIFEST}` 의 `disallowed-methods` 배열이 안 닫혔다"))
        .0;
    let mut out = BTreeSet::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = trimmed.split_once("path = \"") else {
            panic!("`disallowed-methods` 안에서 모르는 줄을 만났다: `{trimmed}`");
        };
        let path = rest
            .1
            .split_once('"')
            .unwrap_or_else(|| panic!("`path = \"` 가 안 닫혔다: `{trimmed}`"))
            .0;
        out.insert(coord(path));
    }
    out
}

/// `A::from_rgba_{u,p}` 처럼 중괄호로 묶인 축약을 편다.
///
/// 순수 함수다.
fn expand_braces(spec: &str) -> Vec<String> {
    let Some((head, rest)) = spec.split_once('{') else {
        return vec![spec.to_string()];
    };
    let (inner, tail) = rest
        .split_once('}')
        .unwrap_or_else(|| panic!("중괄호가 안 닫혔다: `{spec}` — 축약을 못 펴면 판정 불가다"));
    inner
        .split(',')
        .map(|alt| format!("{head}{}{tail}", alt.trim()))
        .collect()
}

/// 문서 절의 **첫 표**에서 차단 함수 열을 읽는다.
///
/// 순수 함수다.
fn doc_coords(md: &str) -> BTreeSet<Coord> {
    let body = md
        .split_once(SECTION)
        .unwrap_or_else(|| panic!("`{SECTION}` 절을 못 찾았다 — 제목이 바뀌었으면 여기를 고쳐라"))
        .1;
    // 절의 끝은 다음 같은 깊이(###) 또는 더 얕은(##) 헤딩이다.
    let body = body.split("\n## ").next().unwrap_or(body);
    let body = body.split("\n### ").next().unwrap_or(body);

    let mut out = BTreeSet::new();
    let mut started = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            if started {
                break;
            }
            continue;
        }
        started = true;
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() < 2 || cells[0] == "차단 함수" || cells[0].starts_with("---") {
            continue;
        }
        // 한 칸 안에서 타입을 이어받는다 — `` `A::x` / `y` `` 의 `y` 는 `A::y` 다.
        let mut carried: Option<String> = None;
        for span in cells[0].split('`').skip(1).step_by(2) {
            for spec in expand_braces(span) {
                let full = if spec.contains("::") {
                    let c = coord(&spec);
                    carried = Some(c.0.clone());
                    c
                } else {
                    let ty = carried.clone().unwrap_or_else(|| {
                        panic!("`{spec}` 이 타입 없이 나왔는데 이어받을 것이 없다: `{trimmed}`")
                    });
                    (ty, spec.clone())
                };
                out.insert(full);
            }
        }
    }
    out
}

/// 양쪽을 맞대고 어긋난 것을 사람이 읽는 줄로 만든다. 순수 함수다.
fn mismatches(doc: &BTreeSet<Coord>, manifest: &BTreeSet<Coord>) -> Vec<String> {
    let mut out = Vec::new();
    for (ty, m) in manifest.difference(doc) {
        out.push(format!(
            "  `{MANIFEST}` 에만 있다: `{ty}::{m}` — 문서 표가 부분 사본이다"
        ));
    }
    for (ty, m) in doc.difference(manifest) {
        out.push(format!(
            "  `{DOC}` 에만 있다: `{ty}::{m}` — 차단되지 않는 함수를 차단된다고 적었다"
        ));
    }
    out
}

#[test]
fn the_doc_table_lists_exactly_the_disallowed_methods() {
    let manifest = manifest_coords(&read(MANIFEST));
    let doc = doc_coords(&read(DOC));

    assert!(
        manifest.len() >= MIN_ENTRIES,
        "`{MANIFEST}` 에서 {} 개만 읽었다(하한 {MIN_ENTRIES}) — 판독기가 깨졌는지 확인해라. \
         이 하한을 내려서 초록을 만들지 마라",
        manifest.len()
    );

    let wrong = mismatches(&doc, &manifest);
    assert!(
        wrong.is_empty(),
        "`{DOC}` 의 차단 함수 표가 `{MANIFEST}` 의 `disallowed-methods` 와 다르다. \
         그 표는 손으로 베낀 사본이고 컴파일에 안 먹으므로, 목록을 고치는 커밋에서 \
         표도 같이 고쳐라.\n실측: 문서 {} · 매니페스트 {}\n{}",
        doc.len(),
        manifest.len(),
        wrong.join("\n")
    );
}

/// 판정기가 실제로 무는지 확인하는 변이 — 파일은 안 고친다.
mod disallowed_mutations {
    use super::*;

    fn real() -> (BTreeSet<Coord>, BTreeSet<Coord>) {
        (doc_coords(&read(DOC)), manifest_coords(&read(MANIFEST)))
    }

    #[test]
    fn the_real_pair_agrees() {
        let (doc, manifest) = real();
        assert_eq!(mismatches(&doc, &manifest), Vec::<String>::new());
    }

    #[test]
    fn a_new_disallowed_method_missing_from_the_doc_is_caught() {
        let (doc, mut manifest) = real();
        manifest.insert(("Color32".into(), "from_additive".into()));
        let found = mismatches(&doc, &manifest);
        assert_eq!(found.len(), 1, "우변 추가를 못 물었다: {found:?}");
        assert!(found[0].contains("부분 사본"), "{found:?}");
    }

    #[test]
    fn a_row_deleted_from_the_doc_is_caught() {
        let (mut doc, manifest) = real();
        let key = doc.iter().next().expect("문서 표가 비었다").clone();
        doc.remove(&key);
        let found = mismatches(&doc, &manifest);
        assert_eq!(found.len(), 1, "좌변 삭제를 못 물었다: {found:?}");
    }

    #[test]
    fn a_doc_only_entry_is_caught() {
        let (mut doc, manifest) = real();
        doc.insert(("Color32".into(), "from_nothing".into()));
        let found = mismatches(&doc, &manifest);
        assert_eq!(found.len(), 1, "좌변 추가를 못 물었다: {found:?}");
        assert!(found[0].contains("차단되지 않는"), "{found:?}");
    }

    #[test]
    fn the_method_name_alone_would_fold_six_into_five() {
        // 좌표를 (타입, 메서드) 로 잡은 이유를 수로 못박는다.
        let (_, manifest) = real();
        let names: BTreeSet<&String> = manifest.iter().map(|(_, m)| m).collect();
        assert!(
            names.len() < manifest.len(),
            "메서드 이름만으로도 접히지 않는다 — 이 테스트의 전제가 바뀌었다"
        );
    }

    // ── 실패문의 양성 대조 ──────────────────────────────────────────────────
    //
    // 판독기가 죽는 갈래는 일곱이고 **처방이 서로 다르다**(배열이 어디 갔나 / 닫혔나 /
    // 줄 형태가 바뀌었나 / 따옴표가 안 닫혔나 / 경로에 `::` 가 없다 / 중괄호가 안 닫혔다 /
    // 절 제목이 바뀌었다). 합성 입력으로 발화시켜 **가르는 낱말**만 단정한다.

    #[test]
    #[should_panic(expected = "배열을 못 찾았다")]
    fn a_missing_array_says_the_key_moved() {
        manifest_coords("[lints]\n");
    }

    #[test]
    #[should_panic(expected = "배열이 안 닫혔다")]
    fn an_unclosed_array_says_so() {
        manifest_coords("disallowed-methods = [\n    { path = \"A::b\" },");
    }

    #[test]
    #[should_panic(expected = "모르는 줄을 만났다")]
    fn an_entry_without_a_path_key_is_not_silently_skipped() {
        manifest_coords("disallowed-methods = [\n    { paff = \"A::b\" },\n]\n");
    }

    #[test]
    #[should_panic(expected = "` 가 안 닫혔다")]
    fn an_unterminated_path_string_says_so() {
        manifest_coords("disallowed-methods = [\n    { path = \"A::b },\n]\n");
    }

    #[test]
    #[should_panic(expected = "`::` 가 없다")]
    fn a_path_without_a_type_segment_says_so() {
        coord("from_rgb");
    }

    #[test]
    #[should_panic(expected = "중괄호가 안 닫혔다")]
    fn an_unclosed_brace_shorthand_says_so() {
        expand_braces("A::from_rgba_{unmultiplied");
    }

    #[test]
    #[should_panic(expected = "절을 못 찾았다")]
    fn a_missing_doc_section_says_the_heading_moved() {
        doc_coords("## 다른 절\n\n| 차단 함수 | 대체 |\n");
    }

    #[test]
    fn braces_expand() {
        assert_eq!(
            expand_braces("egui::Color32::from_rgba_{unmultiplied,premultiplied}"),
            vec![
                "egui::Color32::from_rgba_unmultiplied".to_string(),
                "egui::Color32::from_rgba_premultiplied".to_string(),
            ]
        );
        assert_eq!(expand_braces("A::b"), vec!["A::b".to_string()]);
    }

    #[test]
    fn the_doc_reader_carries_the_type_across_a_slash() {
        let md = "## 색 생성 정책\n\n### clippy 강제 — disallowed-methods\n\n| 차단 함수 | 대체 |\n|---|---|\n\
                  | `HexColor::from_rgb` / `from_rgba` | x |\n\n### 다음\n";
        let got = doc_coords(md);
        assert!(
            got.contains(&("HexColor".into(), "from_rgba".into())),
            "{got:?}"
        );
    }

    #[test]
    #[should_panic(expected = "이어받을 것이 없다")]
    fn a_bare_method_with_no_type_to_carry_is_not_silently_skipped() {
        let md = "## 색 생성 정책\n\n### clippy 강제 — disallowed-methods\n\n| 차단 함수 | 대체 |\n|---|---|\n\
                  | `from_rgba` | x |\n\n### 다음\n";
        doc_coords(md);
    }
}
