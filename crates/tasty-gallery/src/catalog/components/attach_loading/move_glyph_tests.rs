//! 로딩 표본에서 move 글리프가 활성 칸에 그려지지 않는지, 레일 거절 칩이 연결 안 됨 원격 칩과만
//! 함께 그려지는지 그린 도형으로 잰다. 앱은 이동 원본이 활성 탭·활성 워크스페이스에 있으면 그 칸에
//! move 글리프를 두지 않고, 거절 칩은 연결되지 않은 저장 원격 행에만 둔다.

use crate::catalog::components::painted::{Painted, mocha_latte, paint};
use tasty_type_appearance::theme::Theme;

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

/// `label` 표본의 off·on 아바타 바탕(rail bed, `th`의 bg-sidebar로 채운 아바타+여백 사각형).
/// 표본 글자 바로 아래에서 시작해 off·on 두 칸 폭 안에 놓인 바탕만 고른다. 다른 테마 카드의 같은
/// 글자 아래에는 이 테마 색 바탕이 없다.
fn rail_beds(painted: &Painted, th: &Theme, label: &str) -> Vec<egui::Rect> {
    let pad = th.spacing_sm.value();
    let size =
        crate::catalog::components::rail_marks::avatar_size(th) + egui::Vec2::splat(pad * 2.0);
    let beds: Vec<egui::Rect> = painted
        .rects_filled_all(th.bg_sidebar().into())
        .into_iter()
        .filter(|r| (r.size() - size).length() < 0.01)
        .collect();
    let mut found = Vec::new();
    for cap in painted.texts.iter().filter(|t| t.text == label) {
        let cap = cap.rect;
        for bed in &beds {
            let below = bed.min.y > cap.max.y && bed.min.y - cap.max.y < size.y;
            let in_pair = bed.min.x >= cap.min.x - 1.0
                && bed.min.x <= cap.min.x + size.x + th.spacing_md.value() + 1.0;
            if below && in_pair {
                found.push(*bed);
            }
        }
    }
    found
}

/// 글리프 중심이 `bed` 안에 있는 `tint` 글리프 수.
fn glyphs_in_bed(painted: &Painted, bed: egui::Rect, tint: egui::Color32) -> usize {
    painted
        .glyphs
        .iter()
        .filter(|(r, c)| *c == tint && bed.contains(r.center()))
        .count()
}

/// "remote refused"와 "idle with all four corners" 표본의 off·on 아바타마다 거절 칩(attach-refusal-glyph)과
/// 연결 안 됨 mirror 칩 글리프(glyph-dim)가 함께 그려지고, 연결된 mirror 칩 글리프(workspace-mirror-fg)는
/// 없다. 연결된 칩 판독은 같은 표본 묶음에 연결된 mirror 칩 글리프가 그려졌는지로 대조한다.
#[test]
fn the_loading_rail_refusal_chip_comes_with_the_not_live_remote_chip() {
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::rail(ui, base));
    for (name, th) in mocha_latte() {
        let refusal = th.attach_refusal_glyph().to_egui();
        let offline: egui::Color32 = th.glyph_dim().into();
        let live: egui::Color32 = th.workspace_mirror_fg().into();
        assert_ne!(offline, live, "{name}: the two mirror inks must differ");
        assert!(
            painted.glyphs.iter().any(|(_, c)| *c == live),
            "{name}: the live mirror control painted no glyph"
        );
        for label in ["remote refused", "idle with all four corners"] {
            let beds = rail_beds(&painted, &th, label);
            assert_eq!(beds.len(), 2, "{name}: {label} off·on beds");
            for bed in beds {
                assert_eq!(
                    glyphs_in_bed(&painted, bed, refusal),
                    1,
                    "{name}: {label} avatar at {bed:?} refusal chip"
                );
                assert_eq!(
                    glyphs_in_bed(&painted, bed, offline),
                    1,
                    "{name}: {label} avatar at {bed:?} not-live remote chip"
                );
                assert_eq!(
                    glyphs_in_bed(&painted, bed, live),
                    0,
                    "{name}: {label} avatar at {bed:?} live remote chip"
                );
            }
        }
    }
}

/// `bed` 안에 그린 점유 링(border-attached 테두리 사각형) 수.
fn rings_in_bed(painted: &Painted, th: &Theme, bed: egui::Rect) -> usize {
    let ring: egui::Color32 = th.border_attached().into();
    painted
        .shapes
        .iter()
        .filter(|s| match s {
            egui::Shape::Rect(r) => {
                r.stroke.width > 0.0 && r.stroke.color == ring && bed.contains(r.rect.center())
            }
            _ => false,
        })
        .count()
}

/// 네 모서리 표본의 off·on 아바타에는 점유 링이 없다. 링 판독은 "attached ring" 표본으로 대조한다.
#[test]
fn the_loading_rail_attached_ring_is_not_on_the_four_corner_avatar() {
    let (_, base) = &mocha_latte()[0];
    let painted = paint(|ui| super::matrix::rail(ui, base));
    for (name, th) in mocha_latte() {
        let ringed = rail_beds(&painted, &th, "attached ring");
        assert_eq!(ringed.len(), 2, "{name}: attached ring sample beds");
        for bed in ringed {
            assert_eq!(rings_in_bed(&painted, &th, bed), 1, "{name}: ring control");
        }
        let corners = rail_beds(&painted, &th, "idle with all four corners");
        assert_eq!(corners.len(), 2, "{name}: four-corner beds");
        for bed in corners {
            assert_eq!(
                rings_in_bed(&painted, &th, bed),
                0,
                "{name}: attached ring on the four-corner avatar at {bed:?}"
            );
        }
    }
}
