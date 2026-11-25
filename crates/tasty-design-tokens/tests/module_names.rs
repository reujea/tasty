//! component 모듈명이 Rust 키워드가 되는 토큰도 컴파일되는 이름으로 생성되는지 확인한다.

use tasty_design_tokens::dtcg;

#[test]
fn a_keyword_module_name_is_emitted_as_a_raw_identifier() {
    let set = dtcg::parse(
        r#"{
  "primitive": { "size-8": { "$type": "dimension", "$value": "8px" } },
  "semantic": {},
  "component": {
    "mod-hint-width": { "$type": "dimension", "$value": "{primitive.size-8}" },
    "zz-width": { "$type": "dimension", "$value": "{component.mod-hint-width}" }
  }
}"#,
    )
    .expect("시험 토큰 파싱");
    let g = dtcg::generate(&set);
    let component = g
        .files
        .iter()
        .find(|(n, _)| *n == "component.rs")
        .map(|(_, f)| f.as_str())
        .expect("component 상수 파일");
    assert!(component.contains("pub mod r#mod {"), "{component}");
    assert!(
        component.contains("super::r#mod::HINT_WIDTH"),
        "{component}"
    );
    assert!(!component.contains("pub mod mod {"), "{component}");
}
