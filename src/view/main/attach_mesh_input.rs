//! 원격 mesh에 사용자 입력·컨텍스트를 전달한다. 좌표·modifier 변환은 로컬 mesh와 공유한다.
//! MainView에서 CoreState 큐에 넣고 App이 attach 세션으로 전송한다.
//! 원격 surface는 이미 존재하므로 생성하지 않으며 전체 텍스처 복구 요청은 별도 경로에서 처리한다.

use winit::event::MouseButton;

use tasty_plugin_protocol::protocol::{RawInputEventWire, RawInputWire, ThemeWire};

use crate::core::AttachMeshContextForward;
use crate::model::PhysicalRect;
use crate::state::mesh_frames::MeshSide;

use super::MainView;
use super::egui_mesh::{key_wire_event, map_button};

/// 원격 mesh에 마지막으로 보낸 컨텍스트와 다음에 보낼 입력.
#[derive(Default)]
pub(crate) struct AttachMeshForwardState {
    /// 마지막으로 보낸 (width_px, height_px, ppp.to_bits()). 변경 감지에 사용.
    last_geom: Option<(u32, u32, u32)>,
    /// 마지막으로 보낸 focused 상태.
    last_focused: Option<bool>,
    /// 마지막으로 보낸 Theme 스냅샷.
    last_theme: Option<ThemeWire>,
    /// 다음 forward 에 실어 보낼 누적 입력 이벤트(순서 보존).
    events: Vec<RawInputEventWire>,
    admitted: Option<std::time::Instant>,
}

impl MainView {
    /// 포인터 위치의 원격 mesh surface와 영역. 버튼을 누른 채면 마지막으로 누른 leaf가 원격일 때만 그 leaf다.
    pub(super) fn attach_mesh_target_at(
        &self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        x: f32,
        y: f32,
    ) -> Option<(u32, PhysicalRect)> {
        let regions = self.mesh_regions(engine);
        let presses = &self.state.mesh_press;
        crate::state::mesh_frames::motion_target(&regions, presses, MeshSide::Attach, x, y)
            .map(|r| (r.id, r.rect))
    }

    /// 버튼 사건의 원격 mesh 대상. 누른 leaf를 기억해 뗌을 그 leaf로 보낸다.
    pub(super) fn attach_mesh_button(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        x: f32,
        y: f32,
        (button, state): (winit::event::MouseButton, winit::event::ElementState),
    ) -> Option<(u32, PhysicalRect)> {
        let regions = self.mesh_regions(engine);
        crate::state::mesh_frames::button_target(
            &regions,
            &mut self.state.mesh_press,
            MeshSide::Attach,
            button,
            (x, y),
            super::egui_mesh::is_pressed(state),
        )
        .map(|r| (r.id, r.rect))
    }

    /// 포인터 버튼 누름/뗌을 attach mesh surface 에 누적.
    pub(super) fn attach_mesh_push_pointer_button(
        &mut self,
        surface_id: u32,
        rect: PhysicalRect,
        x: f32,
        y: f32,
        button: MouseButton,
        pressed: bool,
    ) {
        let Some(button) = map_button(button) else {
            return;
        };
        let (lx, ly) = self.mesh_local_point(rect, x, y);
        let modifiers = self.mesh_modifiers();
        let st = self.attach_mesh_input.entry(surface_id).or_default();
        st.admitted.get_or_insert_with(std::time::Instant::now);
        st.events.push(RawInputEventWire::PointerButton {
            x: lx,
            y: ly,
            button,
            pressed,
            modifiers,
        });
    }

    /// 포인터 이동을 attach mesh surface 에 누적.
    pub(super) fn attach_mesh_push_pointer_moved(
        &mut self,
        surface_id: u32,
        rect: PhysicalRect,
        x: f32,
        y: f32,
    ) {
        let (lx, ly) = self.mesh_local_point(rect, x, y);
        let st = self.attach_mesh_input.entry(surface_id).or_default();
        st.admitted.get_or_insert_with(std::time::Instant::now);
        st.events
            .push(RawInputEventWire::PointerMoved { x: lx, y: ly });
    }

    /// 포인터가 대상 밖으로 나갔음을 다음 전송에 넣는다.
    pub(super) fn attach_mesh_push_pointer_gone(&mut self, surface_id: u32) {
        let st = self.attach_mesh_input.entry(surface_id).or_default();
        st.admitted.get_or_insert_with(std::time::Instant::now);
        st.events.push(RawInputEventWire::PointerGone);
    }

    /// 스크롤 델타(논리 포인트)를 attach mesh surface 에 누적.
    pub(super) fn attach_mesh_push_scroll(&mut self, surface_id: u32, dx: f32, dy: f32) {
        let st = self.attach_mesh_input.entry(surface_id).or_default();
        st.admitted.get_or_insert_with(std::time::Instant::now);
        st.events.push(RawInputEventWire::Scroll { x: dx, y: dy });
    }

    /// 눌린 키를 누적한다. 뗌은 키보드 진입부에서 제외한다.
    pub(super) fn attach_mesh_push_key(&mut self, surface_id: u32, event: &winit::event::KeyEvent) {
        let modifiers = self.mesh_modifiers();
        let Some(ev) = key_wire_event(
            &event.logical_key,
            event.physical_key,
            matches!(event.state, winit::event::ElementState::Pressed),
            event.repeat,
            modifiers,
        ) else {
            return;
        };
        let st = self.attach_mesh_input.entry(surface_id).or_default();
        st.admitted.get_or_insert_with(std::time::Instant::now);
        st.events.push(ev);
    }

    /// 텍스트 입력을 attach mesh surface 에 누적.
    pub(super) fn attach_mesh_push_text(&mut self, surface_id: u32, text: &str) {
        if text.is_empty() {
            return;
        }
        let st = self.attach_mesh_input.entry(surface_id).or_default();
        st.admitted.get_or_insert_with(std::time::Instant::now);
        st.events.push(RawInputEventWire::Text {
            text: text.to_string(),
        });
    }

    /// IME 조합 이벤트를 attach mesh surface 에 누적.
    pub(super) fn attach_mesh_push_ime(
        &mut self,
        surface_id: u32,
        event: tasty_plugin_protocol::protocol::ImeWire,
    ) {
        let st = self.attach_mesh_input.entry(surface_id).or_default();
        st.admitted.get_or_insert_with(std::time::Instant::now);
        st.events.push(RawInputEventWire::Ime { event });
    }

    /// 변경된 영역·테마·포커스와 누적 입력을 App의 네트워크 전송 큐에 넣는다.
    pub(super) fn forward_attach_mesh_context(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        let terminal_rect = self.compute_terminal_rect();
        let ppp = self.base.gpu.scale_factor();
        let focused = self.state.focused_surface_id(engine);
        let modifiers = self.mesh_modifiers();
        let current_theme = self.mesh_theme_snapshot(engine);

        let targets: Vec<(u32, PhysicalRect)> = self
            .state
            .mesh_frame_regions(engine, terminal_rect, self.base.gpu.scale_factor())
            .into_iter()
            .filter(|r| r.side == MeshSide::Attach)
            .map(|r| (r.id, r.rect))
            .collect();

        let existing = self.state.attach_mesh_surfaces_existing(engine);
        let live: std::collections::HashSet<u32> = existing.into_iter().collect();
        self.attach_mesh_input.retain(|sid, _| live.contains(sid));

        for (sid, rect) in targets {
            let w = rect.width.value().round().max(1.0) as u32;
            let h = rect.height.value().round().max(1.0) as u32;
            let geom = (w, h, ppp.to_bits());
            let is_focused = focused == Some(sid);

            let st = self.attach_mesh_input.entry(sid).or_default();
            let geom_changed = st.last_geom != Some(geom);
            let theme_changed = st.last_theme.as_ref() != Some(&current_theme);
            let focus_changed = st.last_focused != Some(is_focused);
            let has_input = !st.events.is_empty();

            let mut context = None;
            let mut input = None;
            if geom_changed || theme_changed || focus_changed {
                st.last_geom = Some(geom);
                st.last_theme = Some(current_theme.clone());
                st.last_focused = Some(is_focused);
                context = Some(AttachMeshContextForward {
                    width_px: w,
                    height_px: h,
                    pixels_per_point: ppp,
                    theme: Some(current_theme.clone()),
                    focused: is_focused,
                });
            }

            if has_input {
                let events = std::mem::take(&mut st.events);
                input = Some(RawInputWire {
                    time: None,
                    focused: is_focused,
                    modifiers,
                    events,
                });
            }
            if (context.is_some() || input.is_some())
                && let Some(target) =
                    crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
            {
                self.state.dispatch_intent(
                    crate::intent::Intent::Frame(
                        crate::app::frame_effect::InputEffect::RemoteMesh {
                            target,
                            context,
                            input,
                            admitted: st.admitted.take().unwrap_or_else(std::time::Instant::now),
                        }
                        .into(),
                    )
                    .from_user_shortcut("remote-mesh-frame"),
                );
            }
        }
    }
}
