//! 로딩 표본에서 move 글리프가 활성 칸에 그려지지 않는지, 레일 거절 칩이 연결 안 됨 원격 칩과만
//! 함께 그려지는지 그린 도형으로 잰다. 앱은 이동 원본이 활성 탭·활성 워크스페이스에 있으면 그 칸에
//! move 글리프를 두지 않고, 거절 칩은 연결되지 않은 저장 원격 행에만 둔다.

use crate::catalog::components::painted::{mocha_latte, paint};

/// 탭 표본(좁은 탭 포함)의 move 글리프는 활성 탭 칸 밖에 있다.
#[test]
fn the_loading_tab_samples_draw_no_move_glyph_on_an_active_tab() {
    // 표본은 받은 테마와 관계없이 Mocha·Latte를 함께 그린다.
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::tabs(ui, base));
    for (name, th) in mocha_latte() {
        let active = painted.active_tab_cells(&th);
        painted.assert_move_glyphs_outside(&th, &active, name);
    }
}

/// 레일 아바타 표본(네 모서리 표본 포함)의 move 칩 글리프는 활성 아바타(overlay-active 채움) 밖에 있다.
#[test]
fn the_loading_rail_samples_draw_no_move_chip_on_an_active_avatar() {
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::rail(ui, base));
    for (name, th) in mocha_latte() {
        let active = painted.rects_filled_all(th.overlay_active().to_egui_premultiplied());
        painted.assert_move_glyphs_outside(&th, &active, name);
    }
}

/// 레일 거절 칩(attach-refusal-glyph 잉크)마다 같은 아바타의 오른쪽 아래에 연결 안 됨 mirror 칩
/// 글리프(glyph-dim)가 있고, 연결된 mirror 칩 글리프(workspace-mirror-fg)는 없다.
#[test]
fn the_loading_rail_refusal_chip_comes_with_the_not_live_remote_chip() {
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::rail(ui, base));
    for (name, th) in mocha_latte() {
        let refusal = th.attach_refusal_glyph().to_egui();
        let offline: egui::Color32 = th.glyph_dim().into();
        let live: egui::Color32 = th.workspace_mirror_fg().into();
        assert_ne!(offline, live, "{name}: the two mirror inks must differ");
        let size = crate::catalog::components::rail_marks::avatar_size(&th);
        // 거절 칩은 왼쪽 위, mirror 칩은 오른쪽 아래 모서리다. 왼쪽 이웃 아바타의 칩은 왼쪽에 있다.
        let lower_right = |from: egui::Pos2, tint: egui::Color32| {
            painted.glyphs.iter().any(|(r, c)| {
                let d = r.center() - from;
                *c == tint && d.x > 0.0 && d.y > 0.0 && d.x <= size.x && d.y <= size.y
            })
        };
        let chips: Vec<egui::Rect> = painted
            .glyphs
            .iter()
            .filter(|(_, c)| *c == refusal)
            .map(|(r, _)| *r)
            .collect();
        assert!(!chips.is_empty(), "{name}: no refusal chip was painted");
        assert!(
            painted.glyphs.iter().any(|(_, c)| *c == live),
            "{name}: the live mirror control painted no glyph"
        );
        for chip in chips {
            assert!(
                lower_right(chip.center(), offline),
                "{name}: the refusal chip at {chip:?} has no not-live remote chip"
            );
            assert!(
                !lower_right(chip.center(), live),
                "{name}: the refusal chip at {chip:?} sits with a live remote chip"
            );
        }
    }
}

/// 점유 링(border-attached 테두리)을 두른 아바타에는 거절 칩도 move 칩도 없다. 링은 "attached ring"
/// 표본에만 있고 네 모서리 표본에는 없다.
#[test]
fn the_loading_rail_attached_ring_is_not_on_the_four_corner_avatar() {
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::rail(ui, base));
    for (name, th) in mocha_latte() {
        let ring: egui::Color32 = th.border_attached().into();
        let rings: Vec<egui::Rect> = painted
            .shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r) if r.stroke.width > 0.0 && r.stroke.color == ring => {
                    Some(r.rect)
                }
                _ => None,
            })
            .collect();
        assert!(!rings.is_empty(), "{name}: no attached ring was painted");
        let corners = [
            th.attach_refusal_glyph().to_egui(),
            th.move_source_glyph().into(),
        ];
        assert!(
            painted.glyphs.iter().any(|(_, c)| corners.contains(c)),
            "{name}: no corner chip was painted"
        );
        for r in rings {
            assert!(
                !painted
                    .glyphs
                    .iter()
                    .any(|(g, c)| corners.contains(c) && r.contains(g.center())),
                "{name}: the attached ring at {r:?} sits with a corner chip"
            );
        }
    }
}
