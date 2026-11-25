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

fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(STAGE_GAP.value(), STAGE_GAP.value());
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                let frames: [(&str, FocusLook, Frame); 6] = [
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
                    (
                        "remote · tab strip",
                        FocusLook::RemoteTab(0),
                        super::remote::passkeys_frame,
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
                ];
                for (caption, look, frame) in frames {
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
                "control ring: focus-ring-width 2 · border-focus · focus-ring-offset 1 off the tab",
            ),
            (
                "settings L1 tab",
                "the same ring inside the tab, 1 clear of the 2 active underline (−5 offset)",
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
