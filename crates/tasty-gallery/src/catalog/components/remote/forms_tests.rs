//! 프로필 폼 표본의 안내 줄 자리와 잉크를 그려진 도형으로 확인한다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::components::painted::{PaintedText, paint};

use super::draw_profile_form;

fn themes() -> [Theme; 2] {
    [
        tasty_themes::mocha_fallback(),
        crate::host_shell::latte_theme(),
    ]
}

fn all<'a>(texts: &'a [PaintedText], text: &str) -> Vec<&'a PaintedText> {
    texts.iter().filter(|t| t.text == text).collect()
}

/// SSH 폼 두 장(add mode·validation error) 모두 shell=auto 안내를 Passkey 행 뒤에 그린다. 본체 폼
/// 순서(Shell 행 → Passkey 행 → 안내)와 같고, 안내는 text-muted이며 라벨 칸 오른쪽으로 들여쓴다.
#[test]
fn the_ssh_form_draws_the_shell_auto_hint_after_the_passkey_row() {
    let hint = crate::i18n::t("remote_tool.shell_auto_hint");
    for th in themes() {
        let p = paint(|ui| draw_profile_form(ui, &th));
        let hints = all(&p.texts, hint);
        // 표본은 라벨을 고정 영어로 그리고, 내장 영어 값이 같은 글자를 낸다.
        let shells = all(
            &p.texts,
            crate::i18n::t_lang("en", "remote_tool.field_shell"),
        );
        let passkeys = all(
            &p.texts,
            crate::i18n::t_lang("en", "remote_tool.field_passkey"),
        );
        assert_eq!(hints.len(), 2, "one shell=auto hint per SSH card");
        assert_eq!(shells.len(), 2, "one Shell label per SSH card");
        assert_eq!(passkeys.len(), 2, "one Passkey label per SSH card");
        for h in hints {
            // 같은 카드의 라벨: 안내 왼쪽에서 가장 가까운 열(라벨 칸)에 있고 안내보다 위인 가장 가까운 라벨.
            let above = |labels: &[&PaintedText]| -> egui::Rect {
                let left: Vec<&&PaintedText> = labels
                    .iter()
                    .filter(|l| l.rect.right() <= h.rect.left())
                    .collect();
                let gap = left
                    .iter()
                    .map(|l| h.rect.left() - l.rect.right())
                    .fold(f32::INFINITY, f32::min);
                left.into_iter()
                    .filter(|l| h.rect.left() - l.rect.right() <= gap + 0.5)
                    .filter(|l| l.rect.bottom() <= h.rect.top() + 0.5)
                    .min_by(|a, b| {
                        (h.rect.top() - a.rect.bottom())
                            .total_cmp(&(h.rect.top() - b.rect.bottom()))
                    })
                    .map(|l| l.rect)
                    .expect("a label above the hint in the same card")
            };
            let shell = above(&shells);
            let passkey = above(&passkeys);
            assert!(
                shell.bottom() < passkey.top(),
                "the Shell row {shell:?} comes before the Passkey row {passkey:?}"
            );
            assert!(
                passkey.bottom() <= h.rect.top(),
                "the hint {:?} sits below the Passkey row {passkey:?}",
                h.rect
            );
            assert!(
                h.rect.left() > passkey.right(),
                "the hint {:?} is indented past the label column",
                h.rect
            );
            assert_eq!(h.color, th.text_muted().to_egui(), "hint ink");
        }
    }
}

/// Generic 표본은 본체가 모르는 형식 snb를 두고, 배지 대신 본체 폼과 같은 키
/// `remote_tool.type_unknown_hint` 문구를 Type 행과 Name 행 사이에 accent-warning-raised 캡션으로
/// 들여써 그린다. 캡션은 줄바꿈 폭 안에서 감긴다.
#[test]
fn the_generic_form_draws_the_unknown_type_hint_caption_under_type() {
    let hint = crate::i18n::t("remote_tool.type_unknown_hint");
    let type_label = crate::i18n::t_lang("en", "remote_tool.field_type");
    let name_label = crate::i18n::t_lang("en", "remote_tool.field_name");
    for th in themes() {
        let p = paint(|ui| super::draw_generic_passkey_forms(ui, &th));
        let hints = all(&p.texts, hint);
        assert_eq!(hints.len(), 1, "one unknown-type hint (the generic card)");
        let h = hints[0];
        assert_eq!(h.color, th.accent_warning_raised().to_egui(), "hint ink");
        assert!(h.wrap_width.is_finite(), "the hint wraps inside the card");
        assert!(
            !p.texts.iter().any(|t| t.text == "Unknown type"),
            "no unknown type badge"
        );
        let snb = p
            .texts
            .iter()
            .find(|t| t.text == "snb")
            .expect("the generic Type field holds snb");
        let ty = all(&p.texts, type_label)
            .into_iter()
            .find(|t| (t.rect.center().y - snb.rect.center().y).abs() < snb.rect.height())
            .expect("the Type label on the snb row");
        let name = all(&p.texts, name_label)
            .into_iter()
            .filter(|t| t.rect.top() > ty.rect.bottom())
            .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .expect("the Name label under Type");
        assert!(
            ty.rect.bottom() <= h.rect.top() && h.rect.bottom() <= name.rect.top(),
            "the hint {:?} sits between Type {:?} and Name {:?}",
            h.rect,
            ty.rect,
            name.rect
        );
        assert!(
            h.rect.left() > ty.rect.right(),
            "the hint is indented past the label column"
        );
    }
}

/// Generic 표본 Meta는 unknown-type hint 칩을 accent-warning-raised 이름과 스와치로 보인다. Mocha는
/// 기본 accent-warning과 값이 같아 Latte가 가른다.
#[test]
fn the_generic_meta_names_the_unknown_type_hint_accent_warning_raised() {
    let latte = crate::host_shell::latte_theme();
    assert_ne!(latte.accent_warning(), latte.accent_warning_raised());
    for th in themes() {
        let p = paint(|ui| super::draw_generic_passkey_forms(ui, &th));
        let (chip, swatch) = p.token_chip(th.surface_raised().to_egui(), "— unknown-type hint");
        assert!(
            p.has_text_in("accent-warning-raised", chip),
            "the chip is named accent-warning-raised"
        );
        assert_eq!(swatch, th.accent_warning_raised().to_egui(), "chip swatch");
    }
}
