//! 모르는 형식 경고 배지 두 자리(프로필 행·프로토콜 필터)의 툴팁이 본체와 같은 문구 키를 읽는지
//! 포인터를 올려 그려진 툴팁 글자로 확인한다.

use crate::catalog::components::painted::{Painted, paint, paint_hovering};

/// `text`로 그려진 글자 중 가장 위의 것의 중심. Meta 칸의 같은 낱말 머리글보다 표본이 위에 있다.
fn topmost_center(p: &Painted, text: &str) -> egui::Pos2 {
    p.texts
        .iter()
        .filter(|t| t.text == text)
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap_or_else(|| panic!("{text:?} was not painted"))
        .rect
        .center()
}

/// 프로필 목록의 snb 행 경고 배지는 본체 행처럼 `remote_tool.type_unknown_hint`를 툴팁으로 보인다.
/// 그 행은 창 목록의 스크롤 아래에 있어 행 하나만 그려 포인터를 올린다.
#[test]
fn the_profile_row_unknown_type_tooltip_reads_the_runtime_hint_key() {
    let hint = crate::i18n::t("remote_tool.type_unknown_hint");
    let th = tasty_themes::mocha_fallback();
    let scratch = super::PROFILES
        .iter()
        .find(|p| p.tag == "snb")
        .expect("the snb profile in the specimen");
    let row = |ui: &mut egui::Ui| super::profile_row(ui, &th, scratch);
    let at = topmost_center(&paint(row), "snb");
    let p = paint_hovering(at, row);
    assert!(
        p.texts.iter().any(|t| t.text == hint),
        "hovering the snb badge paints the {hint:?} tooltip"
    );
}

/// 열린 프로토콜 필터의 unknown 배지는 본체 필터처럼 `remote_tool.type_unknown_hint`를 툴팁으로
/// 보인다.
#[test]
fn the_filter_unknown_badge_tooltip_reads_the_runtime_hint_key() {
    let hint = crate::i18n::t("remote_tool.type_unknown_hint");
    let th = tasty_themes::mocha_fallback();
    // 표본은 배지 글자를 고정 영어로 그리고, 내장 영어 값이 같은 글자를 낸다.
    let unknown = crate::i18n::t_lang("en", "remote_tool.filter_unknown");
    let at = topmost_center(&paint(|ui| super::draw_filter(ui, &th)), unknown);
    let p = paint_hovering(at, |ui| super::draw_filter(ui, &th));
    assert!(
        p.texts.iter().any(|t| t.text == hint),
        "hovering the unknown badge paints the {hint:?} tooltip"
    );
}
