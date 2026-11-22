//! 버튼에 붙는 egui 팝오버는 `tasty_egui_theme::with_popover_frame`(또는 안쪽 둘레를 받는
//! `with_popover_frame_ring`) 클로저 안에서 열어야 메뉴 틀
//! (menu-bg · menu-border · menu-radius)로 그려진다. 전역 스타일은 tooltip 토큰을 담으므로 감싸지 않은
//! 팝오버는 tooltip 틀로 그려진다(docs/design/systems/theme.md).
//!
//! 주석을 가린 본체·크레이트 소스에서 세 진입 호출을 찾고, 바로 앞 코드가 래퍼 호출의 `|ui| {` 클로저
//! 시작인지 텍스트로 확인한다. 래퍼를 다른 함수로 한 번 더 감싼 호출, `Frame::popup`을 직접 그리는 경로,
//! 진입 호출을 `use`로 줄여 부른 형태는 보지 않는다.
//!
//! 열린 ComboBox 목록의 행은 공용 옵션 행(`menu_option` · `menu_option_value`)으로 그린다. egui 기본
//! 선택 행(`selectable_value` · `selectable_label`)은 현재 값을 accent 채움으로 칠하므로 디자인의 선택
//! 표현(글자 + 오른쪽 체크, 채움 없음)과 다르다. `show_ui` 클로저 본문을 중괄호 짝으로 잘라 확인한다.

use super::rust_sources;

const ENTRIES: &[&str] = &[
    "egui::popup_below_widget(",
    "egui::popup::popup_above_or_below_widget(",
    "egui::ComboBox::from_id_salt(",
];
/// 기본 둘레 래퍼와 둘레를 받는 래퍼. 어느 쪽이든 메뉴 틀을 정한다.
const WRAPPERS: &[&str] = &["with_popover_frame(", "with_popover_frame_ring("];
const GUARD_DIR: &str = "src/source_guards/";
/// 현재 감싼 호출부는 13곳이다. 순회가 비거나 진입 문자열이 낡아 0건으로 통과하는 것을 막는 하한이다.
const MIN_SITES: usize = 13;

/// `at`의 진입 호출이 래퍼 호출의 클로저 본문 첫 식인지 본다.
fn opened_inside_the_wrapper(code: &str, at: usize) -> bool {
    let Some(head) = code[..at].trim_end().strip_suffix('{') else {
        return false;
    };
    let Some(head) = head.trim_end().strip_suffix("|ui|") else {
        return false;
    };
    WRAPPERS.iter().any(|wrapper| {
        head.rfind(wrapper)
            .is_some_and(|w| !head[w..].contains([';', '{', '}']))
    })
}

/// 감싸지 않은 진입 호출의 줄 번호와 검사한 진입 호출 수.
fn judge(src: &str) -> (Vec<usize>, usize) {
    let code = tasty_doc_guards::source_text::mask_comments_aligned(src);
    let mut bare = Vec::new();
    let mut seen = 0;
    for entry in ENTRIES {
        for (at, _) in code.match_indices(entry) {
            seen += 1;
            if !opened_inside_the_wrapper(&code, at) {
                bare.push(code[..at].matches('\n').count() + 1);
            }
        }
    }
    (bare, seen)
}

#[test]
fn every_button_popover_opens_inside_the_menu_frame_wrapper() {
    let mut bare = Vec::new();
    let mut seen = 0;
    for (rel, src) in rust_sources() {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.starts_with(GUARD_DIR) {
            continue;
        }
        let (lines, n) = judge(&src);
        seen += n;
        bare.extend(lines.into_iter().map(|l| format!("{rel}:{l}")));
    }
    assert!(
        seen >= MIN_SITES,
        "팝오버 진입 호출을 {seen}곳만 찾았다(하한 {MIN_SITES}). 순회 범위와 진입 문자열을 확인한다."
    );
    assert!(
        bare.is_empty(),
        "`with_popover_frame` 밖에서 연 팝오버가 있다. 감싸지 않으면 메뉴 틀 대신 tooltip 틀로 그려진다:\n{}",
        bare.join("\n")
    );
}

#[test]
fn the_judge_tells_a_wrapped_popover_from_a_bare_one() {
    let wrapped = "tasty_egui_theme::with_popover_frame(ui, &th, |ui| {\n    egui::ComboBox::from_id_salt(\"a\").show_ui(ui, |ui| {});\n});";
    assert_eq!(judge(wrapped), (vec![], 1));
    let bare = "let x = 1;\negui::ComboBox::from_id_salt(\"a\").show_ui(ui, |ui| {});";
    assert_eq!(judge(bare), (vec![2], 1));
    // 래퍼 클로저 안이라도 앞에 다른 문장이 있으면 진입 호출이 래퍼의 첫 식이 아니다.
    let later = "with_popover_frame(ui, th, |ui| {\n    let y = 2;\n    egui::popup_below_widget(ui, id, &r, c, |ui| {});\n});";
    assert_eq!(judge(later), (vec![3], 1));
    // 다른 함수의 `|ui| {` 클로저는 래퍼로 치지 않는다.
    let other = "with_popover_frame(ui, th, |ui| {});\nui.horizontal(|ui| {\n    egui::popup::popup_above_or_below_widget(ui, id, &r, a, b, |ui| {});\n});";
    assert_eq!(judge(other), (vec![3], 1));
    // 둘레를 받는 래퍼도 래퍼다. 이름만 비슷한 다른 함수는 래퍼가 아니다.
    let ring = "tasty_egui_theme::with_popover_frame_ring(ui, th, egui::Margin::ZERO, |ui| {\n    egui::popup_below_widget(ui, id, &r, c, |ui| {});\n});";
    assert_eq!(judge(ring), (vec![], 1));
    let lookalike = "with_popover_frame_like(ui, th, |ui| {\n    egui::popup_below_widget(ui, id, &r, c, |ui| {});\n});";
    assert_eq!(judge(lookalike), (vec![2], 1));
    // 주석 안의 진입 문자열은 세지 않는다.
    assert_eq!(judge("// egui::popup_below_widget(\n"), (vec![], 0));
}

/// egui 기본 선택 행. 열린 ComboBox 목록 안에서는 쓰지 않는다.
const EGUI_SELECTABLE: &[&str] = &["selectable_value(", "selectable_label("];

/// `open` 위치의 `{` 와 짝이 되는 `}` 까지의 본문.
fn brace_body(code: &str, open: usize) -> &str {
    let mut depth = 0usize;
    for (i, c) in code[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &code[open..open + i];
                }
            }
            _ => {}
        }
    }
    &code[open..]
}

/// egui 기본 선택 행을 쓰는 ComboBox 의 줄 번호와 검사한 ComboBox 수.
fn combo_rows(src: &str) -> (Vec<usize>, usize) {
    let code = tasty_doc_guards::source_text::mask_comments_aligned(src);
    let mut offenders = Vec::new();
    let mut seen = 0;
    for (at, _) in code.match_indices("egui::ComboBox::from_id_salt(") {
        let Some(rel) = code[at..].find(".show_ui(ui, |ui| {") else {
            continue;
        };
        seen += 1;
        let open = at + rel + ".show_ui(ui, |ui| ".len();
        if EGUI_SELECTABLE
            .iter()
            .any(|e| brace_body(&code, open).contains(e))
        {
            offenders.push(code[..at].matches('\n').count() + 1);
        }
    }
    (offenders, seen)
}

#[test]
fn every_open_combo_box_list_draws_shared_option_rows() {
    let mut offenders = Vec::new();
    let mut seen = 0;
    for (rel, src) in rust_sources() {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.starts_with(GUARD_DIR) {
            continue;
        }
        let (lines, n) = combo_rows(&src);
        seen += n;
        offenders.extend(lines.into_iter().map(|l| format!("{rel}:{l}")));
    }
    // 현재 ComboBox 는 3곳이다(시험 하네스 1곳 포함). 순회가 비거나 진입 문자열이 낡아 0건으로 통과하는 것을 막는 하한이다.
    assert!(
        seen >= 3,
        "ComboBox 를 {seen}곳만 찾았다(하한 3). 순회 범위와 진입 문자열을 확인한다."
    );
    assert!(
        offenders.is_empty(),
        "열린 ComboBox 목록이 egui 기본 선택 행을 쓴다. 공용 menu_option · menu_option_value 로 그린다:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_combo_judge_tells_shared_rows_from_egui_selectable_rows() {
    let shared = "egui::ComboBox::from_id_salt(\"a\").show_ui(ui, |ui| {\n    menu_option_value(ui, th, &mut v, 1, \"x\");\n});\nui.selectable_value(&mut w, 2, \"y\");";
    assert_eq!(combo_rows(shared), (vec![], 1));
    let egui_row = "\negui::ComboBox::from_id_salt(\"a\").show_ui(ui, |ui| {\n    if ui.selectable_label(true, \"x\").clicked() {}\n});";
    assert_eq!(combo_rows(egui_row), (vec![2], 1));
}
