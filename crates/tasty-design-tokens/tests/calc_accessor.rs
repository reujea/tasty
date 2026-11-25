//! `calc()` 치수 토큰이 항 접근자의 배수 합으로 생성되고, 받지 않는 식은 스킵되는지 확인한다.

use tasty_design_tokens::dtcg;

fn set(component: &str) -> dtcg::TokenSet {
    let json = format!(
        r#"{{
  "primitive": {{ "size-8": {{ "$type": "dimension", "$value": "8px" }},
                  "size-56": {{ "$type": "dimension", "$value": "56px" }},
                  "size-1": {{ "$type": "dimension", "$value": "1px" }} }},
  "semantic": {{ "space-sm": {{ "$type": "dimension", "$value": "{{primitive.size-8}}" }},
                 "border-width": {{ "$type": "dimension", "$value": "{{primitive.size-1}}" }} }},
  "component": {{ "cell-width": {{ "$type": "dimension", "$value": "{{primitive.size-56}}" }},
                  {component} }}
}}"#
    );
    dtcg::parse(&json).expect("시험 토큰 파싱")
}

fn accessors(component: &str) -> (String, Vec<String>) {
    let g = dtcg::generate(&set(component));
    let file = g
        .type_appearance_files
        .iter()
        .find(|(n, _)| *n == "generated_component.rs")
        .map(|(_, f)| f.clone())
        .expect("component 접근자 파일");
    (file, g.skips)
}

#[test]
fn a_calc_token_sums_its_term_accessors() {
    let (file, skips) = accessors(
        r#""box-min": { "$type": "dimension", "$value": "calc(4 * var(--tasty-cell-width) + 2 * var(--tasty-space-sm) + 2 * var(--tasty-border-width))" }"#,
    );
    // 한 줄이 100칸을 넘어 rustfmt 와 같은 줄바꿈으로 나온다.
    assert!(
        file.contains(
            "LogicalPx(\n            self.cell_width().value() * 4.0\n                + self.spacing_sm.value() * 2.0\n                + self.border_width.value() * 2.0,\n        )"
        ),
        "{file}"
    );
    assert!(
        file.contains("= 242px"),
        "배율 1 값이 문서 주석에 있다: {file}"
    );
    assert!(
        skips
            .iter()
            .any(|s| s.starts_with("component.box-min: calc 식")),
        "상수는 만들지 않는다: {skips:?}"
    );
}

#[test]
fn an_unsupported_calc_is_skipped_not_guessed() {
    for value in [
        "calc(var(--tasty-cell-width) - 2px)",
        "calc(2 * var(--tasty-missing))",
        "calc(100% - var(--tasty-cell-width))",
    ] {
        let (file, skips) = accessors(&format!(
            r#""box-min": {{ "$type": "dimension", "$value": "{value}" }}"#
        ));
        assert!(!file.contains("fn box_min"), "{value}: {file}");
        assert!(
            skips
                .iter()
                .any(|s| s.starts_with("component.box-min:") && s.ends_with("생성 스킵")),
            "{value}: {skips:?}"
        );
    }
}

#[test]
fn a_negative_calc_term_is_subtracted_not_multiplied_by_minus_one() {
    let (file, _) = accessors(
        r#""box-pad": { "$type": "dimension", "$value": "calc(var(--tasty-space-sm) + -1 * var(--tasty-border-width))" }"#,
    );
    // `x * -1.0` 은 clippy neg_multiply 에 걸려 생성 파일이 -D warnings 를 통과하지 못한다.
    assert!(
        file.contains("LogicalPx(self.spacing_sm.value() * 1.0 - self.border_width.value())"),
        "{file}"
    );
    assert!(!file.contains("* -1.0"), "{file}");
}
