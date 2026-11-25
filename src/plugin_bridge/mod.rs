//! 플러그인 매니저와 본체의 engine·파일·단축키·모델을 연결한다.
//! tasty-host-plugin이 본체에 역으로 의존하지 않도록 이곳에서 조합한다.

#[cfg(feature = "gui")]
pub mod banner_render;
#[cfg(feature = "gui")]
pub mod key_dispatch;
#[cfg(feature = "gui")]
pub mod manifest_validate;
pub mod mesh_forward;
#[cfg(feature = "gui")]
pub mod popup_render;
#[cfg(feature = "gui")]
pub(crate) mod popup_scope;
pub mod remote_kind;
pub mod remote_surface;
#[cfg(feature = "gui")]
pub(crate) mod user_navigation;
#[cfg(feature = "gui")]
pub mod wire_scroll;

#[cfg(feature = "gui")]
pub use tasty_host_plugin::host_actions;
pub use tasty_host_plugin::host_cmd;

/// egui의 논리 사각형을 GPU mesh 합성용 물리 사각형으로 변환한다.
#[cfg(feature = "gui")]
pub(crate) fn mesh_region_of(
    content_rect: egui::Rect,
    pixels_per_point: f32,
) -> crate::model::PhysicalRect {
    use crate::model::{LogicalPx, LogicalRect};
    LogicalRect {
        x: LogicalPx(content_rect.min.x),
        y: LogicalPx(content_rect.min.y),
        width: LogicalPx(content_rect.width()),
        height: LogicalPx(content_rect.height()),
    }
    .to_physical(pixels_per_point)
}

/// 콘텐츠 안의 논리 좌표인 IME 캐럿을 창의 물리 좌표로 변환한다.
/// 콘텐츠 origin은 이미 물리 좌표이므로 배율을 다시 곱하지 않는다.
/// 입력 위젯 전체 rect 대신 cursor_rect를 써야 후보창이 캐럿 옆에 놓인다.
#[cfg(feature = "gui")]
pub(crate) fn mesh_ime_cursor_area(
    content_origin: crate::model::PhysicalRect,
    ime: &tasty_plugin_protocol::ImeCursorWire,
    pixels_per_point: f32,
) -> crate::model::PhysicalRect {
    use crate::model::{LogicalPx, LogicalRect, PhysicalRect};
    let local = LogicalRect {
        x: LogicalPx(ime.cursor_rect.x),
        y: LogicalPx(ime.cursor_rect.y),
        width: LogicalPx(ime.cursor_rect.width),
        height: LogicalPx(ime.cursor_rect.height),
    }
    .to_physical(pixels_per_point);
    PhysicalRect {
        x: content_origin.x + local.x,
        y: content_origin.y + local.y,
        width: local.width,
        height: local.height,
    }
}

/// surface·popup·banner의 context 전송 상태.
/// 크기·테마 변경과 초기 렌더·전체 텍스처 재요청을 함께 관리한다.
/// 포커스·입력·강제 repaint는 요청 경로와 저장 위치가 달라 각 채널이 관리한다.
#[cfg(feature = "gui")]
#[derive(Default)]
pub(crate) struct MeshForwardCommon {
    /// 전송 기준으로 기록한 (width_px, height_px, ppp.to_bits()).
    pub(crate) last_geom: Option<(u32, u32, u32)>,
    /// 전송 기준으로 기록한 테마.
    pub(crate) last_theme: Option<tasty_plugin_protocol::ThemeWire>,
    /// 프레임을 받기 전 초기 context를 반복 요청하지 않도록 한다.
    /// 프레임을 확인하면 해제해 이후 프레임이 사라질 때 다시 요청할 수 있게 한다.
    pub(crate) bootstrap_sent: bool,
    /// 다음 context에 전체 텍스처 재전송 요청을 넣는다.
    pub(crate) pending_full: bool,
    /// 초기 context 요청 시각. 프레임이 없으면 BLANK_MESH_GRACE 후 경고한다.
    bootstrap_at: Option<std::time::Instant>,
    /// 같은 대상의 빈 화면 경고를 반복하지 않도록 한다. 프레임을 받으면 해제한다.
    blank_warned: bool,
    /// 포인터 hover·누름 기록. popup·banner 채널이 입력을 모을 때 쓴다.
    pub(crate) pointer: MeshPointerRoute,
    /// 키 누름 기록. popup 채널이 입력을 모을 때 쓴다.
    pub(crate) keys: MeshKeyRoute,
}

/// mesh 팝업 하나가 받은 키 누름 기록. 누름은 이 팝업이 키 주인일 때만 보내고 기록한다. 뗌은 egui 입력 순서 그대로
/// 이 팝업에서 누른 키일 때만 보낸다(지금 키 주인과 무관하게). 그래서 누름을 받은 팝업이 뗌을 한 번 받고 지금 키 주인은
/// 남의 누름의 뗌을 받지 않는다. Escape 는 Esc 몫의 기록(`PopupManager::note_unfed_escape_release`)이 맡으므로 키
/// 주인 여부만 본다.
#[cfg(feature = "gui")]
#[derive(Clone, Debug, Default)]
pub(crate) struct MeshKeyRoute {
    pressed: Vec<egui::Key>,
}

#[cfg(feature = "gui")]
impl MeshKeyRoute {
    /// 키 사건이 이 팝업에 가는지. `has_key_focus` 는 이 팝업이 지금 키 주인인지다.
    pub(crate) fn takes(&mut self, key: egui::Key, pressed: bool, has_key_focus: bool) -> bool {
        if key == egui::Key::Escape {
            return has_key_focus;
        }
        if pressed {
            if has_key_focus && !self.pressed.contains(&key) {
                self.pressed.push(key);
            }
            return has_key_focus;
        }
        let before = self.pressed.len();
        self.pressed.retain(|k| *k != key);
        self.pressed.len() != before
    }
}

/// mesh 콘텐츠 하나의 포인터 경로 기록. 이동·누름은 포인터 대상이 이 콘텐츠일 때만 보내고, 뗌은 누름이 간
/// 곳으로 보낸다(이 콘텐츠에서 누른 버튼만, 뗀 좌표와 무관하게). 포인터가 콘텐츠를 떠나거나 팝업에 덮이면
/// PointerGone 으로 hover 를 푼다.
#[cfg(feature = "gui")]
#[derive(Clone, Debug, Default)]
pub(crate) struct MeshPointerRoute {
    inside: bool,
    pressed: Vec<egui::PointerButton>,
}

#[cfg(feature = "gui")]
impl MeshPointerRoute {
    /// 누름이 이 콘텐츠에 가는지. 포인터 대상이 이 콘텐츠면(`mine`) 버튼을 기록하고 참이다.
    pub(crate) fn press(&mut self, button: egui::PointerButton, mine: bool) -> bool {
        if mine && !self.pressed.contains(&button) {
            self.pressed.push(button);
        }
        mine
    }

    /// 뗌이 이 콘텐츠에 가는지. 이 콘텐츠에서 누른 버튼이면 기록을 지우고 참이다.
    pub(crate) fn release(&mut self, button: egui::PointerButton) -> bool {
        let before = self.pressed.len();
        self.pressed.retain(|b| *b != button);
        self.pressed.len() != before
    }

    /// 이번 프레임 포인터가 이 콘텐츠의 대상인지 기록한다. 직전에 대상이었는데 아니게 됐으면 참이다(PointerGone
    /// 을 보낸다).
    pub(crate) fn hover(&mut self, inside: bool) -> bool {
        let gone = self.inside && !inside;
        self.inside = inside;
        gone
    }
}

/// 초기 context 요청 후 프레임 없이 기다리는 경고 유예 시간.
/// 느린 처리와 오류를 구별하는 기준은 아니며 원인은 플러그인 로그로 확인한다.
#[cfg(feature = "gui")]
const BLANK_MESH_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

#[cfg(feature = "gui")]
impl MeshForwardCommon {
    /// 프레임이 없으면 유예 시간 뒤 대상별로 한 번 경고한다.
    /// 프레임을 받으면 초기 요청과 경고 상태를 해제한다.
    pub(crate) fn watch_blank(
        &mut self,
        has_frame: bool,
        channel: std::fmt::Arguments<'_>,
        plugin_id: &str,
    ) {
        if has_frame {
            self.bootstrap_sent = false;
            self.bootstrap_at = None;
            self.blank_warned = false;
            return;
        }
        if self.blank_warned || !self.bootstrap_sent {
            return;
        }
        if self
            .bootstrap_at
            .is_none_or(|t| t.elapsed() < BLANK_MESH_GRACE)
        {
            return;
        }
        tracing::error!(
            "egui-mesh {channel} has received no frame {:.0}s after bootstrap. \
             Check the plugin's log: `tasty plugin logs {plugin_id}`",
            BLANK_MESH_GRACE.as_secs_f32(),
        );
        self.blank_warned = true;
    }

    pub(crate) fn geom_changed(&self, geom: (u32, u32, u32)) -> bool {
        self.last_geom != Some(geom)
    }

    pub(crate) fn theme_changed(&self, theme: &tasty_plugin_protocol::ThemeWire) -> bool {
        self.last_theme.as_ref() != Some(theme)
    }

    pub(crate) fn need_bootstrap(&self, has_frame: bool) -> bool {
        !has_frame && !self.bootstrap_sent
    }

    pub(crate) fn take_pending_full(&mut self) -> bool {
        std::mem::take(&mut self.pending_full)
    }

    /// context 전송 판단에 사용한 크기·테마와 초기 요청 시각을 기록한다.
    pub(crate) fn record_sent(
        &mut self,
        geom: (u32, u32, u32),
        theme: &tasty_plugin_protocol::ThemeWire,
        has_frame: bool,
    ) {
        self.last_geom = Some(geom);
        self.last_theme = Some(theme.clone());
        if !has_frame {
            self.bootstrap_sent = true;
            // 입력·리사이즈 때마다 시각을 갱신하면 경고가 계속 미뤄진다.
            self.bootstrap_at
                .get_or_insert_with(std::time::Instant::now);
        }
    }
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;
    use crate::model::{PhysicalPx, PhysicalRect};
    use tasty_plugin_protocol::{ImeCursorWire, RectWire};

    fn ime(cursor: RectWire) -> ImeCursorWire {
        ImeCursorWire {
            // 위젯 rect를 캐럿과 다르게 해 어느 값을 사용하는지 구분한다.
            rect: RectWire {
                x: 0.0,
                y: 0.0,
                width: 999.0,
                height: 999.0,
            },
            cursor_rect: cursor,
        }
    }

    #[test]
    fn ime_cursor_area_offsets_by_the_content_origin() {
        let origin = PhysicalRect {
            x: PhysicalPx(100.0),
            y: PhysicalPx(200.0),
            width: PhysicalPx(300.0),
            height: PhysicalPx(150.0),
        };
        let got = mesh_ime_cursor_area(
            origin,
            &ime(RectWire {
                x: 10.0,
                y: 20.0,
                width: 1.0,
                height: 18.0,
            }),
            1.0,
        );
        assert_eq!(got.x, PhysicalPx(110.0));
        assert_eq!(got.y, PhysicalPx(220.0));
        assert_eq!(got.width, PhysicalPx(1.0));
        assert_eq!(got.height, PhysicalPx(18.0));
    }

    // 이미 물리 좌표인 origin에 배율을 다시 곱하지 않는지 검사한다.
    #[test]
    fn ime_cursor_area_scales_only_the_local_rect() {
        let origin = PhysicalRect {
            x: PhysicalPx(100.0),
            y: PhysicalPx(200.0),
            width: PhysicalPx(600.0),
            height: PhysicalPx(300.0),
        };
        let got = mesh_ime_cursor_area(
            origin,
            &ime(RectWire {
                x: 10.0,
                y: 20.0,
                width: 2.0,
                height: 16.0,
            }),
            2.0,
        );
        assert_eq!(got.x, PhysicalPx(120.0));
        assert_eq!(got.y, PhysicalPx(240.0));
        assert_eq!(got.width, PhysicalPx(4.0));
        assert_eq!(got.height, PhysicalPx(32.0));
        assert_ne!(got.x, PhysicalPx(220.0));
    }

    #[test]
    fn ime_cursor_area_follows_the_caret_not_the_widget() {
        let origin = PhysicalRect {
            x: PhysicalPx(0.0),
            y: PhysicalPx(0.0),
            width: PhysicalPx(400.0),
            height: PhysicalPx(100.0),
        };
        let got = mesh_ime_cursor_area(
            origin,
            &ime(RectWire {
                x: 250.0,
                y: 8.0,
                width: 1.0,
                height: 18.0,
            }),
            1.0,
        );
        assert_eq!(got.x, PhysicalPx(250.0));
        assert_ne!(got.x, PhysicalPx(0.0));
        assert_ne!(got.width, PhysicalPx(999.0));
    }
}
