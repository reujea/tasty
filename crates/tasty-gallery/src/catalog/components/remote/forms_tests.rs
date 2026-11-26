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
