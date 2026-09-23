//! `empty-surface` specimen — 빈 surface 본문 (Layouts).
//!
//! 본체 `src/adapters/ui/surface/empty.rs::draw_empty` 의 구조 전사. 그 함수는
//! surface 본문 전체를 `bg-app` 으로 한 번 칠하고(crust/base 색 불일치 방지),
//! 남은 높이의 절반만큼 띄운 뒤 `vertical_centered` 로 **버튼 하나**만 그린다.
//! 클릭하면 그 surface 를 대상으로 convert popup 이 열린다.
//!
//! **토큰 이관 2건** (구조는 동일, 값은 보존):
//! - 본체의 버튼 높이 리터럴 `28.0` → `item_height_interactive`(28) 로 읽는다.
//! - 본체는 egui 기본 `ui.button` 을 쓴다 → specimen 은 공용
//!   `tasty_ui_widgets::Button`(Secondary). 보편 컴포넌트는 공용 위젯으로
//!   그린다는 정책(`docs/architecture/ui-widgets-crate.md#무엇을-공용-위젯으로`)의 목표 상태다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant};

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// surface 무대 1칸 — 본체의 pane 본문에 대응.
fn surface_body(ui: &mut egui::Ui, theme: &Theme) {
    let size = egui::vec2(theme.measure_md.value(), theme.measure_sm.value() * 0.6);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());

    // 본체와 같은 순서: 배경을 먼저 전면 칠한다(radius 0 — pane 은 사각).
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_app().to_egui());

    // 세로 중앙에 버튼 하나. `new_child` 는 부모 레이아웃을 상속하므로(스테이지가
    // 가로 컨텍스트면 `add_space` 가 가로로 먹는다) 세로 스택을 명시한다.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let btn_h = theme.item_height_interactive.value();
    child.add_space(((rect.height() - btn_h) * 0.5).max(0.0));
    child.vertical_centered(|ui| {
        Button::new("Surface Type")
            .variant(ButtonVariant::Secondary)
            .show(ui, theme);
    });
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "Empty surface", |ui| surface_body(ui, theme));
    });

    spec::meta(
        ui,
        theme,
        &[
            ("background", "bg-app, 전면 · radius 0"),
            ("button", "item-height-interactive(28) · 세로 중앙"),
            ("action", "convert popup 을 이 surface 대상으로 연다"),
        ],
        &[
            TokenChip::new("bg-app", "surface body", theme.bg_app().to_egui()),
            TokenChip::new(
                "text-primary",
                "button label",
                theme.text_primary().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "빈 surface 는 안내 문구를 두지 않는다 — 다음 수가 하나뿐이라 버튼 자체가 안내다. \
         split 으로 새로 생긴 자리와 kind 를 비운 자리가 같은 화면을 쓴다.",
    );
}
