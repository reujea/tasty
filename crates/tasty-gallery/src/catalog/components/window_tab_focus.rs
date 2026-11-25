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

#[cfg(test)]
thread_local! {
    static REMOTE_STRIP: Cell<Option<egui::Id>> = const { Cell::new(None) };
}

/// 원격 탭 줄을 그린 ui의 id를 적어 둔다. 시험이 위젯이 탭마다 등록한 응답을 이 id로 읽는다.
#[cfg(test)]
pub(super) fn note_remote_strip(id: egui::Id) {
    REMOTE_STRIP.with(|s| s.set(Some(id)));
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
            "remote · tab next to the open one",
            FocusLook::RemoteTab(2),
            super::remote::attach_frame,
        ),
        (
            "remote · selected tab",
            FocusLook::RemoteTab(1),
            super::remote::attach_frame,
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

/// 칸 하나: 캡션과 그 아래 창 그림. `look`을 고정해 그린다.
fn cell(ui: &mut egui::Ui, theme: &Theme, caption: &str, look: FocusLook, frame: Frame) {
    showing(ui, look, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.label(
            egui::RichText::new(caption)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        frame(ui, theme);
    });
}

fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(STAGE_GAP.value(), STAGE_GAP.value());
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                for (caption, look, frame) in frames() {
                    spec::wrap_item(ui, |ui| cell(ui, theme, caption, look, frame));
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
    use crate::catalog::components::painted::PaintedText;

    /// 원격 창 탭 줄의 탭 수(Remote profiles·Attach·Passkeys).
    const REMOTE_TABS: usize = 3;

    /// 한 패스를 그린 결과.
    struct Painted {
        /// 포커스 링 사각형들.
        rings: Vec<egui::Rect>,
        /// 활성 밑줄의 x 중심.
        underline: Option<f32>,
        /// 원격 탭 줄 위젯이 탭마다 등록한 응답의 사각형. 원격 탭 줄을 그리지 않은 그림이면 비어 있다.
        remote_tabs: Vec<egui::Rect>,
    }

    /// `frame`을 `look`으로 고정해 한 패스 그린다.
    fn paint(theme: &Theme, look: FocusLook, frame: Frame) -> Painted {
        REMOTE_STRIP.with(|s| s.set(None));
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
        // 탭 줄 위젯(`draw_tab_strip_with_focus_look`)이 `ui.interact`에 쓰는 id와 같다.
        let remote_tabs = REMOTE_STRIP.with(Cell::get).map_or_else(Vec::new, |strip| {
            (0..REMOTE_TABS)
                .map(|i| {
                    ctx.read_response(strip.with((i, "rt_tab")))
                        .expect("each tab registers its rect")
                        .rect
                })
                .collect()
        });
        Painted {
            rings,
            underline,
            remote_tabs,
        }
    }

    /// 원격 탭 칸은 셋이고 (열린 탭, 포커스 탭)이 정본 순서대로 (Passkeys, Remote profiles), (Attach, Passkeys),
    /// (Attach, Attach)다. 탭 자리는 위젯이 탭마다 등록한 응답으로 읽고, 열린 탭은 그린 밑줄이 놓인 탭, 포커스
    /// 탭은 그린 링이 안쪽에 놓인 탭이다.
    #[test]
    fn the_remote_tab_cells_match_the_canon_three() {
        let theme = tasty_themes::mocha_fallback();
        let inset = tasty_ui_widgets::tab_ring_inset(&theme, theme.tab_indicator_width()).value();
        let mut got = Vec::new();
        for (caption, look, frame) in frames() {
            let FocusLook::RemoteTab(_) = look else {
                continue;
            };
            let p = paint(&theme, look, frame);
            assert_eq!(p.remote_tabs.len(), REMOTE_TABS, "{caption}");
            let x = p.underline.expect("the open tab draws its underline");
            let open = p
                .remote_tabs
                .iter()
                .position(|t| t.x_range().contains(x))
                .expect("underline sits under a tab");
            assert_eq!(p.rings.len(), 1, "{caption}: {:?}", p.rings);
            let focus = p
                .remote_tabs
                .iter()
                .position(|t| t.shrink(inset) == p.rings[0])
                .unwrap_or_else(|| panic!("{caption}: ring {:?} inside no tab", p.rings[0]));
            got.push((open, focus));
        }
        assert_eq!(got, vec![(2, 0), (1, 2), (1, 1)]);
    }

    /// 원격 탭 세 칸의 정본 캡션과 그 아래 창의 목록 표지: (캡션, add-bar 버튼 글자, 목록 행 이름).
    /// 정본 tasty-page develop 80fa994 overlays-windows/window-tab-focus.jsx의 세 View가 그리는
    /// `RemoteFrame tab="passkeys"`·`tab="attach"`·`tab="attach"`다.
    const CANON_REMOTE_TAB_CELLS: [(&str, &str, &[&str]); 3] = [
        (
            "remote · tab strip",
            "Add passkey",
            &["ed25519-main", "vault-token", "legacy"],
        ),
        (
            "remote · tab next to the open one",
            "Add attach",
            &["host", "edge-direct", "legacy-attach"],
        ),
        (
            "remote · selected tab",
            "Add attach",
            &["host", "edge-direct", "legacy-attach"],
        ),
    ];

    /// 원격 탭 세 칸은 정본 캡션을 그리고, 그 아래에 정본 탭의 목록 창(add-bar 버튼과 목록 행 이름)을
    /// 그린다. Attach 폼 창은 Add attach 버튼과 목록 행을 그리지 않는다. 칸은 예제가 그리는 칸 그리기
    /// (`cell`)로 하나씩 그린다(예제 전체는 시험 화면보다 길어 아래 칸 글자가 그려지지 않는다).
    #[test]
    fn the_remote_tab_cells_draw_the_canon_captions_over_the_list_frames() {
        let theme = tasty_themes::mocha_fallback();
        let remote: Vec<(&str, FocusLook, Frame)> = frames()
            .into_iter()
            .filter(|(_, look, _)| matches!(look, FocusLook::RemoteTab(_)))
            .collect();
        assert_eq!(remote.len(), CANON_REMOTE_TAB_CELLS.len());
        for ((caption, look, frame), (canon, add_bar, rows)) in
            remote.into_iter().zip(CANON_REMOTE_TAB_CELLS)
        {
            let p = crate::catalog::components::painted::paint(|ui| {
                cell(ui, &theme, caption, look, frame)
            });
            let cap = p
                .texts
                .iter()
                .find(|t| t.text == canon)
                .unwrap_or_else(|| panic!("canon caption {canon:?} was not painted"));
            let below: Vec<&PaintedText> = p
                .texts
                .iter()
                .filter(|t| t.rect.top() > cap.rect.bottom())
                .collect();
            assert!(
                below.iter().any(|t| t.text == add_bar),
                "{canon}: the cell draws the {add_bar:?} list frame"
            );
            for name in rows {
                assert!(
                    below
                        .iter()
                        .any(|t| t.text.split("  (").next() == Some(name)),
                    "{canon}: list row {name:?} is drawn"
                );
            }
        }
    }
}
