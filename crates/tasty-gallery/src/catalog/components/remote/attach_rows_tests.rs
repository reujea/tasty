//! Attach 탭 표본의 네 행과 old-attach 행의 profile missing 배지를 그려진 도형으로 확인한다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::components::painted::{PaintedText, paint, paint_hovering};

use super::draw_attach;

fn themes() -> [Theme; 2] {
    [
        tasty_themes::mocha_fallback(),
        crate::host_shell::latte_theme(),
    ]
}

/// 행 이름 글자. 이름 뒤 `  (label)`은 떼고 비교한다.
fn row_name<'a>(texts: &'a [PaintedText], name: &str) -> &'a PaintedText {
    texts
        .iter()
        .find(|t| t.text.split("  (").next() == Some(name))
        .unwrap_or_else(|| panic!("attach row {name:?} was not painted"))
}

/// 표본은 정본 RemoteFrame 처럼 네 행을 위에서부터 host · edge-direct · legacy-attach · old-attach 순서로
/// 그린다. old-attach는 참조 프로필이 없어 이름을 text-primary로 두고, 대상 줄 오른쪽에 profile missing
/// 배지를 단다. inactive인 legacy-attach에는 그 배지가 없다.
#[test]
fn the_attach_specimen_draws_old_attach_with_a_profile_missing_badge() {
    let missing = crate::i18n::t("remote_tool.attach_profile_missing");
    for th in themes() {
        let p = paint(|ui| draw_attach(ui, &th));
        let names = ["host", "edge-direct", "legacy-attach", "old-attach"];
        let tops: Vec<f32> = names
            .iter()
            .map(|n| row_name(&p.texts, n).rect.top())
            .collect();
        assert!(
            tops.windows(2).all(|w| w[0] < w[1]),
            "attach rows are drawn top to bottom as {names:?}: {tops:?}"
        );

        let old = row_name(&p.texts, "old-attach");
        assert_eq!(
            old.color,
            th.text_primary().to_egui(),
            "old-attach keeps its name at text-primary"
        );
        let legacy = row_name(&p.texts, "legacy-attach");
        assert_eq!(
            legacy.color,
            th.text_muted().to_egui(),
            "legacy-attach is muted"
        );

        let badges: Vec<&PaintedText> = p.texts.iter().filter(|t| t.text == missing).collect();
        assert_eq!(badges.len(), 1, "one profile missing badge in the specimen");
        let badge = badges[0];
        let target = p
            .texts
            .iter()
            .find(|t| t.text == "→ retired-box")
            .expect("old-attach target line");
        assert!(
            badge.rect.left() > target.rect.right(),
            "the badge {:?} sits right of the target {:?}",
            badge.rect,
            target.rect
        );
        assert!(
            badge.rect.top() < target.rect.bottom() && target.rect.top() < badge.rect.bottom(),
            "the badge {:?} shares the target line {:?}",
            badge.rect,
            target.rect
        );
        assert!(
            badge.rect.top() > old.rect.bottom(),
            "the badge is below the old-attach title line"
        );

        // 같은 공용 경고 배지라 글자 잉크가 inactive 배지와 같다. 표본은 inactive를 고정 영어로 그리고,
        // 내장 영어 값이 같은 글자를 낸다.
        let inactive_text = crate::i18n::t_lang("en", "remote_tool.attach_inactive");
        let inactive = p
            .texts
            .iter()
            .find(|t| t.text == inactive_text)
            .expect("inactive badge");
        assert_eq!(
            badge.color, inactive.color,
            "the badge is the shared warn badge"
        );
    }
}

/// profile missing 배지에 포인터를 두면 본체 attach 행과 같은 키
/// `remote_tool.attach_profile_missing_hint` 문구가 툴팁으로 그려진다.
#[test]
fn the_profile_missing_badge_tooltip_reads_the_runtime_hint_key() {
    let missing = crate::i18n::t("remote_tool.attach_profile_missing");
    let hint = crate::i18n::t("remote_tool.attach_profile_missing_hint");
    let th = tasty_themes::mocha_fallback();
    let at = paint(|ui| draw_attach(ui, &th))
        .texts
        .iter()
        .find(|t| t.text == missing)
        .expect("profile missing badge")
        .rect
        .center();
    let p = paint_hovering(at, |ui| draw_attach(ui, &th));
    assert!(
        p.texts.iter().any(|t| t.text == hint),
        "hovering the profile missing badge paints the {hint:?} tooltip"
    );
}

/// 표본 하나를 그리는 함수.
type DrawFn = fn(&mut egui::Ui, &Theme);

/// 창 머리 띠는 정본 RemoteChromeG처럼 위아래 space-md와 sm 닫기 버튼(item_height_tab) 높이다.
/// 탭 띠(bg-sidebar 채움)의 위 끝이 창 안쪽 위에서 그만큼 떨어져 있는지 Attach 창과 프로필 창에서 잰다.
#[test]
fn the_window_header_is_space_md_around_a_small_close_button() {
    for th in themes() {
        let frames: [(&str, DrawFn); 2] =
            [("attach", super::attach_frame), ("profiles", super::draw)];
        for (name, frame) in frames {
            let p = paint(|ui| frame(ui, &th));
            let tabs = p.rect_filled_with(th.bg_sidebar().to_egui());
            // 탭 띠를 품는 가장 작은 창(bg-panel) 채움.
            let panel =
                p.rect_filled_around(crate::catalog::widgets::dialog::panel_fill(&th), tabs);
            let header = tabs.top() - (panel.top() + th.border_width.value());
            let want = th.spacing_md.value() * 2.0 + th.item_height_tab.value();
            assert!(
                (header - want).abs() < 0.5,
                "{name}: header band {header} is not space-md × 2 + item_height_tab = {want}"
            );
        }
    }
}
