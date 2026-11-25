//! 창 안 탭의 키보드 포커스 예제. 시안 `window-tabs-keyboard-focus` Spec(overlays-windows/window-tab-focus.jsx)을 옮긴다.
//!
//! 설정창 L1 탭·L2 행, 원격 창 탭 줄·segmented, 플러그인 창 탭 막대는 Tab·Shift+Tab으로 포커스를 받고 컨트롤 링을
//! 그린다. 예제는 키를 누르지 않으므로 칸마다 한 탭을 포커스 모양으로 고정한다. 각 창의 갤러리 그림이 [`look`]을
//! 읽어 그 탭에 링을 그린다.

use std::cell::Cell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 칸 사이 간격. 원격 탭·segment 규칙 예제(`segment_rule`)의 무대 간격과 같다.
const STAGE_GAP: LogicalPx = LogicalPx(18.0);

/// 포커스 모양으로 고정할 탭. 인덱스는 각 줄에서 왼쪽(위)부터 센다.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum FocusLook {
    /// 설정창 L1 탭.
    L1(usize),
    /// 설정창 L2 사이드바 행.
    L2(usize),
    /// 원격 창 탭 줄의 탭.
    RemoteTab(usize),
    /// 원격 폼 segmented의 칸.
    Segment(usize),
    /// 플러그인 창 탭 막대의 탭.
    PluginTab(usize),
}

/// 포커스 칸 하나가 그리는 창 그림.
type Frame = fn(&mut egui::Ui, &Theme);

thread_local! {
    static LOOK: Cell<Option<FocusLook>> = const { Cell::new(None) };
}

/// 지금 그리는 예제가 포커스 모양으로 고정한 탭. 이 예제 밖에서는 None이다.
pub(super) fn look() -> Option<FocusLook> {
    LOOK.with(Cell::get)
}

/// `look`을 고정한 채 `add`를 그린다.
fn showing(ui: &mut egui::Ui, look: FocusLook, add: impl FnOnce(&mut egui::Ui)) {
    let prev = LOOK.with(|l| l.replace(Some(look)));
    ui.push_id(look, add);
    LOOK.with(|l| l.set(prev));
}

/// 설정 구역 끝에 붙는 예제.
pub fn specs() -> Vec<crate::catalog::Spec> {
    vec![crate::catalog::Spec {
        id: "window-tabs-keyboard-focus",
        title: "Window tabs — keyboard focus",
        when: Some(
            "settings L1 tab (inset ring) and L2 row · remote tab strip and segment · plugins tab bar · Tab / Shift+Tab move focus, Space / Enter press · the pane tab strip is no keyboard stop",
        ),
        draw,
    }]
}

/// 예제 칸들: (캡션, 고정할 포커스 탭, 창 그림).
fn frames() -> [(&'static str, FocusLook, Frame); 8] {
    [
        (
            "settings · L1 tab",
            FocusLook::L1(0),
            super::settings::frame,
        ),
        (
            "settings · selected L1 tab",
            FocusLook::L1(1),
            super::settings::frame,
        ),
        (
            "settings · L2 row",
            FocusLook::L2(1),
            super::settings::frame,
        ),
        // 원격 탭 세 칸은 정본 remote-tab-focus·remote-tab-next-to-selected-focus·remote-selected-tab-focus
        // (tasty-page develop 80fa994 overlays-windows/window-tab-focus.jsx)다. 열린 탭은 Passkeys·Attach·Attach,
        // 포커스 탭은 Remote profiles·Passkeys·Attach다.
        (
            "remote · tab strip",
            FocusLook::RemoteTab(0),
            super::remote::passkeys_frame,
        ),
        (
            "remote · tab next to the selected one",
            FocusLook::RemoteTab(2),
            super::remote::attach_form_frame,
        ),
        (
            "remote · selected tab",
            FocusLook::RemoteTab(1),
            super::remote::attach_form_frame,
        ),
        (
            "remote · segmented",
            FocusLook::Segment(1),
            super::remote::attach_form_frame,
        ),
        (
            "plugins · tab bar",
            FocusLook::PluginTab(1),
            super::plugins_window::installed_frame,
        ),
    ]
}

fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(STAGE_GAP.value(), STAGE_GAP.value());
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                for (caption, look, frame) in frames() {
                    spec::wrap_item(ui, |ui| {
                        showing(ui, look, |ui| {
                            ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                            ui.label(
                                egui::RichText::new(caption)
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                            frame(ui, theme);
                        });
                    });
                }
            },
        );
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "tabs",
                "Settings L1 tabs and L2 rows · Remote tab strip and segments · Plugins tab bar",
            ),
            ("keys", "Tab / Shift+Tab move focus · Space / Enter press"),
            (
                "focus",
                "control ring: focus-ring-width 2 · border-focus · focus-ring-offset 1 off the L2 row, segment and plugins tab",
            ),
            (
                "settings L1 · remote tab",
                "the same ring inside the tab, 1 clear of the 2 active underline (−5 offset), selected or not",
            ),
            (
                "plugins tab",
                "the same ring on the header's mantle · border-focus",
            ),
            ("not here", "pane tab strip: no keyboard stop, no ring"),
        ],
        &[TokenChip::new(
            "border-focus",
            "focus ring",
            theme.border_focus().to_egui(),
        )],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `frame`을 `look`으로 고정해 한 패스 그리고 (포커스 링 사각형들, 활성 밑줄 x 중심)을 돌려준다.
    fn paint(theme: &Theme, look: FocusLook, frame: Frame) -> (Vec<egui::Rect>, Option<f32>) {
        let ctx = egui::Context::default();
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 1200.0),
                )),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default()
                    .show(c, |ui| showing(ui, look, |ui| frame(ui, theme)));
            },
        );
        let mut rings = Vec::new();
        let mut underline = None;
        for s in &out.shapes {
            match &s.shape {
                egui::Shape::Rect(r)
                    if r.stroke.width == theme.focus_ring_width.value()
                        && r.stroke_kind == egui::StrokeKind::Inside
                        && r.stroke.color == theme.border_focus().to_egui() =>
                {
                    rings.push(r.rect);
                }
                egui::Shape::LineSegment { points, stroke }
                    if stroke.width == theme.tab_indicator_width().value()
                        && stroke.color == theme.accent_primary().to_egui() =>
                {
                    underline = Some((points[0].x + points[1].x) * 0.5);
                }
                _ => {}
            }
        }
        (rings, underline)
    }

    /// 원격 탭 칸은 셋이고 (열린 탭, 포커스 탭)이 정본 순서대로 (Passkeys, Remote profiles), (Attach, Passkeys),
    /// (Attach, Attach)다. 열린 탭은 그린 밑줄이 놓인 탭으로, 탭 자리는 같은 그림의 각 탭 링으로 읽는다.
    #[test]
    fn the_remote_tab_cells_match_the_canon_three() {
        let theme = tasty_themes::mocha_fallback();
        let inset = tasty_ui_widgets::tab_ring_inset(&theme, theme.tab_indicator_width()).value();
        let mut got = Vec::new();
        for (_, look, frame) in frames() {
            let FocusLook::RemoteTab(focus) = look else {
                continue;
            };
            let tabs: Vec<egui::Rect> = (0..3)
                .map(|k| {
                    let (rings, _) = paint(&theme, FocusLook::RemoteTab(k), frame);
                    assert_eq!(rings.len(), 1, "tab {k}: {rings:?}");
                    rings[0].expand(inset)
                })
                .collect();
            let (rings, underline) = paint(&theme, look, frame);
            let x = underline.expect("the open tab draws its underline");
            let open = tabs
                .iter()
                .position(|t| t.x_range().contains(x))
                .expect("underline sits under a tab");
            assert_eq!(rings.len(), 1);
            assert_eq!(rings[0].expand(inset), tabs[focus], "ring on tab {focus}");
            got.push((open, focus));
        }
        assert_eq!(got, vec![(2, 0), (1, 2), (1, 1)]);
    }
}
