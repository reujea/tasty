use crate::runtime::engine_read::EngineRead;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::window::CursorIcon;

use super::{DividerDrag, HoveredLink, MainView, MeshHoverTarget};
use crate::app::command::{DomainIntent, SendPayload};
use crate::settings::LinkModifier;
use crate::terminal_link::{self, LinkHighlight};
use crate::theme;
use crate::view::ui::View;
use tasty_type_geometry::length::PhysicalPx;

impl MainView {
    /// 현재 마우스 좌표와 수식키 상태로 hovered_link를 갱신한다.
    /// 변경이 있으면 true를 반환 (렌더 dirty 플래그를 켜기 위함).
    pub(crate) fn update_hovered_link(&mut self, engine: &EngineRead<'_>) -> bool {
        let prev = self
            .hovered_link
            .as_ref()
            .map(|h| (h.surface_id, h.highlight.segments.clone()));

        let modifier = LinkModifier::parse(&engine.settings.general.link_click_modifier);
        let mods = &self.base.state.modifiers;
        let matches_mods = modifier.matches(mods.control_key(), mods.alt_key(), mods.super_key());

        // 수식키 변경 때도 호출되므로 무대 중에는 배경 링크를 조회하지 않는다.
        let new_link = if !matches_mods
            || self.mouse_overlay_open()
            || self.state.popup_hovered
            || self.state.banner_hovered
        {
            None
        } else {
            self.compute_hovered_link(&engine.as_ref())
        };

        let changed = prev
            != new_link
                .as_ref()
                .map(|h| (h.surface_id, h.highlight.segments.clone()));
        self.hovered_link = new_link;
        changed
    }

    pub(super) fn compute_hovered_link(&self, engine: &EngineRead<'_>) -> Option<HoveredLink> {
        let pos = self.cursor_position?;
        let terminal_rect = self.compute_terminal_rect();
        let x = pos.x as f32;
        let y = pos.y as f32;
        if !terminal_rect.contains(PhysicalPx(x), PhysicalPx(y)) {
            return None;
        }
        // 마우스 아래 surface id를 구하고 그 surface의 terminal을 사용.
        // focused 기반이 아니라 실제 hover 위치의 surface로 판별해야 여러 pane 중
        // 어느 곳이든 동작한다.
        let scale_factor = self.base.gpu.scale_factor();
        let surface_id =
            self.state
                .surface_id_at_position(engine, x, y, terminal_rect, scale_factor)?;
        if engine.live.occupancy.is_hard_occupied(surface_id) {
            return None;
        }
        let terminal = engine.visible_terminal(surface_id)?;
        let surface_rect =
            self.state
                .surface_rect_by_id(engine, surface_id, terminal_rect, scale_factor)?;

        let cwd = engine.terminals.cwd(surface_id);
        let mirror = !engine.terminals.has_pty(surface_id);
        let (span, cut, top_row) =
            terminal.with_view(&self.state.terminal_views.get(engine, surface_id), |view| {
                let point = crate::selection::pixel_to_grid(
                    x,
                    y,
                    &surface_rect,
                    self.base.gpu.cell_width(),
                    self.base.gpu.cell_height(),
                    view.viewport(),
                );
                terminal_link::link_at_view(
                    &view,
                    point.col,
                    point.absolute_row,
                    cwd.as_deref(),
                    mirror,
                )
                .map(|span| (span, view.cut(), view.viewport().top_row))
            })?;
        let th = theme::theme();
        let highlight = LinkHighlight {
            epoch: cut.epoch,
            segments: span.segments,
            fg: th.text_on_active().to_gpu_rgba(),
            bg: th.selection_bg.to_gpu_rgba(),
            underline: th.accent_primary_raised().to_gpu_rgba(),
        };
        // mirror 의 파일 경로만 원격 위치를 알린다. scheme 이 있는 URL 은 로컬 브라우저가 연다.
        let remote_tip = (mirror
            && matches!(
                crate::file_dispatch::parse_link(&span.uri),
                crate::file_dispatch::LinkKind::FileTarget(_)
            ))
        .then(|| {
            crate::adapters::ui::link_remote_tip::LinkRemoteAnchor::new(
                &highlight.segments,
                top_row,
                &surface_rect,
                PhysicalPx(self.base.gpu.cell_width()),
                PhysicalPx(self.base.gpu.cell_height()),
                engine.mirror_host_text(surface_id),
            )
        })
        .flatten();
        Some(HoveredLink {
            revision: cut.revision,
            surface_id,
            uri: span.uri,
            highlight,
            remote_tip,
        })
    }

    /// 커서가 창을 벗어나면 상태를 비우고 이전 mesh 대상에 PointerGone을 보낸다.
    pub(super) fn handle_cursor_left(&mut self) {
        self.cursor_position = None;
        self.base.winit.set_cursor(CursorIcon::Default);
        self.update_mesh_hover(None);
        // 창 밖으로 나갔다 같은 셀로 돌아오면 그건 새 hover 다 — dedup 키를 비워
        // 첫 보고가 삼켜지지 않게 한다.
        self.last_mouse_report_cell = None;
    }

    /// 전체화면 무대는 좌표에 관계없이 배경 마우스 입력을 막는다.
    /// 이동·클릭·휠, click-to-activate, copy_link 단축키가 같은 판정을 사용한다.
    pub(super) fn mouse_overlay_open(&self) -> bool {
        self.state.settings_open_requested || self.state.fullscreen_stage_active()
    }

    /// hover 대상이 바뀌면 이전 mesh surface에 PointerGone을 보낸다.
    /// 그렇지 않으면 plugin에 마지막 hover 표시가 남을 수 있다.
    pub(super) fn update_mesh_hover(&mut self, target: Option<MeshHoverTarget>) {
        let (next, gone) = mesh_hover_transition(self.mesh_pointer_hover, target);
        self.mesh_pointer_hover = next;
        if let Some(prev) = gone {
            match prev {
                MeshHoverTarget::Local(sid) => self.egui_mesh_push_pointer_gone(sid),
                MeshHoverTarget::Attach(sid) => self.attach_mesh_push_pointer_gone(sid),
            }
        }
    }

    pub(super) fn handle_cursor_moved(
        &mut self,
        engine: &EngineRead<'_>,
        position: winit::dpi::PhysicalPosition<f64>,
        egui_consumed: bool,
    ) {
        self.cursor_position = Some(position);
        let overlay_open = self.mouse_overlay_open();
        if cursor_moved_should_short_circuit(
            self.dragging_divider.is_some(),
            egui_consumed,
            overlay_open,
            self.state.popup_hovered,
            self.state.banner_hovered,
            self.state.modifier_hint_hovered,
        ) {
            // 콘텐츠/오버레이 위에서는 리사이즈 커서를 띄우지 않는다(콘텐츠 우선).
            // early-return 경로에서도 반드시 리셋해야 가장자리→콘텐츠 이동 시 ↔ 커서가
            // 남지 않는다.
            self.state.pending_resize_cursor = None;
            if self.hovered_link.take().is_some() {
                self.mark_dirty();
            }
            // 아래 mesh 처리 전에 반환하므로 이전 대상의 hover를 여기서 해제한다.
            self.update_mesh_hover(None);
            self.mark_dirty();
            return;
        }

        // egui가 커서를 덮어쓰므로 리사이즈 방향을 저장해 렌더 프레임에서 적용한다.
        // macOS는 native 장식이 처리한다.
        #[cfg(not(target_os = "macos"))]
        {
            let size = self.base.gpu.size();
            self.state.pending_resize_cursor =
                if super::fullscreen_window::window_size_is_locked(&self.base.winit) {
                    None
                } else {
                    crate::platform::window_chrome::resize_direction_at(
                        position.x,
                        position.y,
                        f64::from(size.width),
                        f64::from(size.height),
                        crate::platform::window_chrome::RESIZE_EDGE_MARGIN,
                    )
                };
        }

        let terminal_rect = self.compute_terminal_rect();
        let x = position.x as f32;
        let y = position.y as f32;

        // divider 드래그 중에는 mesh로 전달하지 않아야 아래 드래그 갱신이 실행된다.
        // 입력 우선순위: docs/architecture/input-layer.md.
        if self.dragging_divider.is_none()
            && let Some((sid, _plugin_id, rect)) = self.egui_mesh_target_at(engine, x, y)
        {
            self.update_mesh_hover(Some(MeshHoverTarget::Local(sid)));
            self.egui_mesh_push_pointer_moved(sid, rect, x, y);
            self.mark_dirty();
            return;
        }
        // attach mesh mirror surface 위 포인터 이동 forward(`docs/dev-guide/egui-mesh-channel.md`
        // 의 "attach mesh mirror 소비 경로" 참고) — 위와 동형이되 목적지가 원격.
        if let Some((sid, rect)) = self.attach_mesh_target_at(engine, x, y) {
            self.update_mesh_hover(Some(MeshHoverTarget::Attach(sid)));
            self.attach_mesh_push_pointer_moved(sid, rect, x, y);
            self.mark_dirty();
            return;
        }
        // 어느 mesh surface 위도 아니다 — divider 드래그로 위 두 분기를
        // 건너뛴 경우 포함. 직전까지 hover 중이던 mesh surface 가 있었다면
        // `PointerGone` 을 1 회 forward 한다(mesh→mesh, mesh→non-mesh 전환 공통 처리).
        self.update_mesh_hover(None);

        if self.update_hovered_link(engine) {
            self.mark_dirty();
        }

        // 로컬 선택은 좌버튼만, 앱 보고는 press를 보낸 모든 버튼의 드래그를 처리한다.
        let dragging_for_report = !self.report_buttons_down.is_empty();
        if (self.left_mouse_down || dragging_for_report) && self.dragging_divider.is_none() {
            // press 때 선택한 Shift 우회 상태를 motion과 release까지 유지한다.
            if !self.left_select_bypass {
                // 트래킹 앱에 셀 이동을 보고한다. 커서가 surface 밖으로 나가도
                // press 때 정한 대상을 유지하고 좌표를 해당 surface에 맞춰 제한한다.
                let track = self
                    .report_buttons_down
                    .last()
                    .copied()
                    .and_then(|(button, sid)| {
                        engine
                            .find_terminal_by_id(sid)
                            .map(|t| (sid, button, t.mouse_tracking()))
                    });
                // 1002와 1003 모두 버튼 드래그를 보고한다. 버튼 없는 1003 이동은 아래에서 처리한다.
                if let Some((sid, button, mode)) = track
                    && matches!(
                        self.effective_click_tracking(engine, sid, mode),
                        tasty_terminal::MouseTrackingMode::CellMotion
                            | tasty_terminal::MouseTrackingMode::AllMotion
                    )
                {
                    let (col, row) = self.mouse_cell_for_report(&engine.as_ref(), sid, x, y);
                    if self.last_mouse_report_cell != Some((sid, col, row)) {
                        self.report_mouse_event(engine, sid, x, y, button, true, false);
                    }
                    return;
                }
            }
            // 우·미들 버튼 드래그로 로컬 선택을 바꾸지 않는다.
            let is_dragging =
                self.left_mouse_down && self.text_selection.as_ref().is_some_and(|s| s.dragging);
            if is_dragging
                && let Some((point, _)) = self.mouse_to_grid(&engine.as_ref(), x, y, &terminal_rect)
            {
                if let Some(sel) = &mut self.text_selection {
                    if sel.anchor.epoch != point.epoch {
                        sel.anchor = point;
                    }
                    sel.cursor = point;
                }
                self.mark_dirty();
            }
        } else {
            // 버튼 없는 DECSET 1003 이동. 위 드래그 보고와 중복하지 않는다.
            self.report_hover_motion(engine, x, y, &terminal_rect);
        }

        if let Some(drag) = self.dragging_divider {
            let cell_w = self.base.gpu.cell_width();
            let cell_h = self.base.gpu.cell_height();
            let scale_factor = self.base.gpu.scale_factor();
            let changed = {
                let ratio = match drag.info.direction {
                    crate::model::SplitDirection::Vertical => {
                        (PhysicalPx(x) - drag.info.split_rect.x).value()
                            / drag.info.split_rect.width.value()
                    }
                    crate::model::SplitDirection::Horizontal => {
                        (PhysicalPx(y) - drag.info.split_rect.y).value()
                            / drag.info.split_rect.height.value()
                    }
                };
                let changed = self.state.update_divider_preview(
                    engine,
                    drag.sequence,
                    ratio,
                    terminal_rect,
                    scale_factor,
                );
                if changed {
                    self.state
                        .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
                }
                changed
            };
            if changed {
                self.mark_dirty();
            }
        }
        // Cursor icon is determined in the egui render cycle (gfx/gpu.rs)
    }

    /// native 메뉴를 닫는 클릭의 press/release를 egui에 전달하지 않는다.
    /// PointerGone도 보내 아래 위젯이 같은 클릭을 처리하거나 hover를 유지하지 않게 한다.
    pub(super) fn begin_menu_dismiss_swallow(&mut self, event: &winit::event::WindowEvent) -> bool {
        let winit::event::WindowEvent::MouseInput {
            state: button_state,
            button,
            ..
        } = event
        else {
            return false;
        };
        if !self.take_menu_dismiss_swallow(*button_state, *button) {
            return false;
        }
        self.base.gpu.push_egui_pointer_gone();
        true
    }

    /// 메뉴를 닫는 클릭인지 egui 전달 전에 한 번 판정한다.
    /// 결과는 egui 입력 차단과 handle_mouse_input이 함께 사용한다.
    pub(super) fn take_menu_dismiss_swallow(
        &mut self,
        button_state: ElementState,
        button: MouseButton,
    ) -> bool {
        let menu_open = self.pending_menu.is_some();
        menu_dismiss_swallow_step(
            &mut self.menu_dismiss_swallow,
            menu_open,
            button_state,
            button,
        )
    }

    /// OS 리사이즈, 활성화 클릭, 오버레이, mesh 입력을 순서대로 처리한 뒤
    /// 남은 클릭을 버튼별 핸들러에 전달한다.
    pub(super) fn handle_mouse_input(
        &mut self,
        engine: &EngineRead<'_>,
        button_state: ElementState,
        button: MouseButton,
        egui_consumed: bool,
        menu_dismiss_swallow: bool,
    ) {
        // native 메뉴 바깥 클릭의 소비 여부는 egui 전달 전에 이미 결정됐다.
        // 물리 버튼을 놓으면 어느 경로로 반환하든 보고 스택에서 먼저 제거한다.
        // 그렇지 않으면 이후 이동을 계속 드래그로 해석한다.
        if button_state == ElementState::Released
            && let Some(code) = report_button_code(button)
        {
            pop_report_button(&mut self.report_buttons_down, code);
        }

        if menu_dismiss_swallow {
            if button_state == ElementState::Pressed {
                self.dismiss_pending_native_menu();
            }
            // release 는 삼키기만 하면 된다 — 짝이 되는 press 를 라우팅하지 않았으니
            // 정리할 드래그/선택 상태도 없다.
            return;
        }

        #[cfg(not(target_os = "macos"))]
        if self.try_begin_os_resize(button, button_state) {
            return;
        }

        let overlay_open = self.mouse_overlay_open();
        // 복원 안내 카드 위의 press는 안내가 받는다. 아래 surface를 활성화하거나 선택·분할선
        // 드래그를 시작하지 않는다. 바깥에서 시작한 드래그의 release는 그대로 마무리한다.
        let notice_hit = button_state == ElementState::Pressed && self.restore_notice_hit();
        // 분할선 hit 띠의 press 는 포커스를 바꾸지 않고 곧바로 드래그를 시작한다. 비활성 surface 쪽
        // 띠도 첫 press 부터 분할선을 잡아야 하므로 click-to-activate 보다 먼저 본다.
        if self.try_press_divider_first(
            engine,
            button,
            button_state,
            egui_consumed || notice_hit,
            overlay_open,
        ) {
            return;
        }
        if !notice_hit && self.try_click_to_activate(engine, button, button_state, overlay_open) {
            return;
        }

        if egui_consumed
            || notice_hit
            || overlay_open
            || self.state.popup_hovered
            || self.state.banner_hovered
            || self.state.modifier_hint_hovered
        {
            // 비-좌클릭/Release/egui-크롬(사이드바·탭바) 클릭의 소비. 활성 surface 안
            // pane 포커스 갱신은 위 click-to-activate 단계가 흡수하므로 여기서는
            // Release 정리와 egui 소비 repaint 만 남긴다.
            if button_state == ElementState::Released {
                self.finish_divider_drag(engine);
                self.left_mouse_down = false;
                self.left_select_bypass = false;
            }
            if egui_consumed {
                self.mark_dirty();
            }
            return;
        }

        // divider 드래그의 release는 mesh로 보내지 않는다. 아래 핸들러가
        // 크기 변경을 확정하고 드래그 상태를 해제해야 한다.
        if self.dragging_divider.is_none()
            && self.try_forward_egui_mesh_button(engine, button, button_state)
        {
            return;
        }
        if self.try_forward_attach_mesh_button(engine, button, button_state) {
            return;
        }

        match button {
            MouseButton::Right => self.handle_right_button(engine, button_state),
            MouseButton::Middle => self.handle_middle_button(engine, button_state),
            MouseButton::Left => self.handle_left_button(engine, button_state),
            _ => {}
        }
    }

    /// 현재 포인터가 직전 프레임에 그린 복원 안내 카드 위인지.
    fn restore_notice_hit(&self) -> bool {
        self.cursor_position.is_some_and(|pos| {
            self.state.restore_notices.covers_physical(
                PhysicalPx(pos.x as f32),
                PhysicalPx(pos.y as f32),
                self.base.gpu.egui_pixels_per_point(),
            )
        })
    }

    /// 창 가장자리에서 좌버튼을 누르면 OS 리사이즈를 시작한다. macOS는 제외한다.
    /// egui 패널 전체가 아니라 실제 위젯의 hover를 확인해 빈 여백에서도 시작할 수 있게 한다.
    #[cfg(not(target_os = "macos"))]
    fn try_begin_os_resize(&mut self, button: MouseButton, button_state: ElementState) -> bool {
        if button == MouseButton::Left
            && button_state == ElementState::Pressed
            && !resize_should_yield_to_content(
                self.state.resize_edge_widget_hovered,
                self.mouse_overlay_open(),
                self.state.popup_hovered,
                self.state.banner_hovered,
                super::fullscreen_window::window_size_is_locked(&self.base.winit),
            )
            && let Some(pos) = self.cursor_position
        {
            let size = self.base.gpu.size();
            if let Some(dir) = crate::platform::window_chrome::resize_direction_at(
                pos.x,
                pos.y,
                f64::from(size.width),
                f64::from(size.height),
                crate::platform::window_chrome::RESIZE_EDGE_MARGIN,
            ) {
                if let Err(e) = self.base.winit.drag_resize_window(dir) {
                    tracing::warn!("window resize drag failed: {e}");
                }
                return true;
            }
        }
        false
    }

    /// click-to-activate swallow: 비활성 surface 를 좌클릭(press)하면 첫 클릭을
    /// "surface 전환" 이 통째로 소비한다(macOS 모델). 전환하면 `true`(클릭 소비).
    /// modal/popup 은 상위 레이어라 전환보다 먼저 배제(호출부 `overlay_open`).
    /// docs/architecture/input-layer.md.
    fn try_click_to_activate(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        button: MouseButton,
        button_state: ElementState,
        overlay_open: bool,
    ) -> bool {
        if button == MouseButton::Left
            && button_state == ElementState::Pressed
            && !overlay_open
            && !self.state.popup_hovered
            && !self.state.modifier_hint_hovered
            && let Some(pos) = self.cursor_position
        {
            let terminal_rect = self.compute_terminal_rect();
            let scale_factor = self.base.gpu.scale_factor();
            let (x, y) = (pos.x as f32, pos.y as f32);
            if let Some(sid) =
                self.state
                    .surface_id_at_position(engine, x, y, terminal_rect, scale_factor)
                && self.state.focused_surface_id(engine) != Some(sid)
            {
                let changed_pane =
                    self.state
                        .focus_pane_at_position(engine, x, y, terminal_rect, scale_factor);
                let changed_surf =
                    self.state
                        .focus_surface_at_position(engine, x, y, terminal_rect, scale_factor);
                if changed_pane || changed_surf {
                    self.base.state.dirty = true;
                }
                self.mark_dirty();
                return true;
            }
        }
        false
    }

    /// egui-mesh surface 입력 forward (A1-S7): 포인터가 egui-mesh surface 위면 버튼
    /// 이벤트를 surface-local 좌표로 누적해 다음 set_context 로 보내고 소비(`true`).
    fn try_forward_egui_mesh_button(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        button: MouseButton,
        button_state: ElementState,
    ) -> bool {
        if let Some(pos) = self.cursor_position {
            let (x, y) = (pos.x as f32, pos.y as f32);
            if let Some((sid, _plugin_id, rect)) = self.egui_mesh_target_at(engine, x, y) {
                let pressed = super::egui_mesh::is_pressed(button_state);
                if !pressed {
                    self.left_mouse_down = false;
                }
                self.egui_mesh_push_pointer_button(sid, rect, x, y, button, pressed);
                self.mark_dirty();
                return true;
            }
        }
        false
    }

    /// attach mesh mirror surface 입력 forward — 위와 동형이되 목적지가 원격.
    fn try_forward_attach_mesh_button(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        button: MouseButton,
        button_state: ElementState,
    ) -> bool {
        if let Some(pos) = self.cursor_position {
            let (x, y) = (pos.x as f32, pos.y as f32);
            if let Some((sid, rect)) = self.attach_mesh_target_at(engine, x, y) {
                let pressed = super::egui_mesh::is_pressed(button_state);
                if !pressed {
                    self.left_mouse_down = false;
                }
                self.attach_mesh_push_pointer_button(sid, rect, x, y, button, pressed);
                self.mark_dirty();
                return true;
            }
        }
        false
    }

    /// 우클릭 라우팅: 트래킹 ON+Shift없음이면 앱 위임(docs/surfaces/terminal/index.md), 아니면 tasty 컨텍스트
    /// 메뉴(terminal/비-terminal 별도). 결정은 순수 `right_click_delegates_to_app`.
    fn handle_right_button(&mut self, engine: &EngineRead<'_>, button_state: ElementState) {
        // 링크 메뉴 스냅샷은 한 클릭 사이클의 것이다 — press 는 이전 값을 버리고, release 는
        // 아래 early return 보다 먼저 회수해 다음 사이클로 새지 않게 한다.
        let released_link = match button_state {
            ElementState::Pressed => {
                self.right_link_press = None;
                None
            }
            ElementState::Released => self.right_link_press.take(),
        };
        let terminal_rect = self.compute_terminal_rect();
        let Some(pos) = self.cursor_position else {
            return;
        };
        let (x, y) = (pos.x as f32, pos.y as f32);
        if !terminal_rect.contains(PhysicalPx(x), PhysicalPx(y)) {
            return;
        }
        let Some(surface_id) = self.state.surface_id_at_position(
            engine,
            x,
            y,
            terminal_rect,
            self.base.gpu.scale_factor(),
        ) else {
            return;
        };
        let tracking = engine
            .find_terminal_by_id(surface_id)
            .map(|t| t.mouse_tracking());
        let Some(tracking) = tracking else {
            // 비터미널 surface의 메뉴는 egui 프레임에서 만든다.
            // 여기서는 터미널의 트래킹 보고와 메뉴만 처리한다.
            return;
        };
        // 링크 위 우클릭은 tracking 위임보다 먼저 로컬 링크 메뉴로 간다 — 좌클릭이
        // `try_handle_link_click` 을 위임 판정보다 먼저 부르는 것과 같은 게이트다.
        let link = match button_state {
            ElementState::Pressed => {
                self.right_link_press =
                    self.terminal_link_menu_target(&engine.as_ref(), surface_id);
                self.right_link_press.clone()
            }
            ElementState::Released => released_link,
        };
        if let Some(link) = link {
            self.queue_terminal_link_menu(link, button_state, x, y);
            return;
        }
        // 블랙리스트면 None 으로 격하 → 우클릭이 tasty 컨텍스트 메뉴로 빠진다.
        let tracking = self.effective_click_tracking(engine, surface_id, tracking);
        let shift = self.base.state.modifiers.shift_key();
        if right_click_delegates_to_app(tracking, shift) {
            // 트래킹 앱이 마우스를 캡처 중이라 우클릭이 앱으로 간다 — Shift+드래그/
            // Shift+우클릭 우회 안내를 트래킹 세션당 1회(Pressed, 설정 ON, docs/surfaces/terminal/index.md).
            if button_state == ElementState::Pressed {
                self.report_left_press_capture(engine, surface_id);
                // Shift+우클릭(docs/surfaces/terminal/index.md)은 이 분기에 들어오지 않으므로 스택에도 안
                // 오른다 — press 를 안 보낸 드래그의 motion 이 새지 않는다.
                push_report_button(&mut self.report_buttons_down, 2, surface_id);
            }
            self.report_mouse_event(
                engine,
                surface_id,
                x,
                y,
                2,
                false,
                button_state == ElementState::Released,
            );
            return;
        }
        // Linux는 버튼을 놓은 뒤 열어야 GTK popup_at_rect가 메뉴를 표시한다.
        // macOS/Windows는 press에서 동기 처리한다.
        if button_state == terminal_menu_open_state() {
            let sf = self.base.gpu.scale_factor();
            self.state.dialogs.pending_native_menu =
                Some(crate::state::PendingNativeMenu::TerminalSurface {
                    surface_id,
                    // 네이티브 메뉴 좌표는 logical — 물리 마우스 좌표를 변환 API 로 내린다.
                    x: PhysicalPx(x).to_logical(sf).value(),
                    y: PhysicalPx(y).to_logical(sf).value(),
                });
            self.mark_dirty();
        }
    }

    /// 미들클릭 라우팅: 트래킹 ON 에서만 앱에 보고 (트래킹 OFF 는 무동작 유지).
    fn handle_middle_button(&mut self, engine: &EngineRead<'_>, button_state: ElementState) {
        let terminal_rect = self.compute_terminal_rect();
        if let Some(pos) = self.cursor_position {
            let (x, y) = (pos.x as f32, pos.y as f32);
            if terminal_rect.contains(PhysicalPx(x), PhysicalPx(y))
                && let Some(surface_id) = self.state.surface_id_at_position(
                    engine,
                    x,
                    y,
                    terminal_rect,
                    self.base.gpu.scale_factor(),
                )
                && engine
                    .find_terminal_by_id(surface_id)
                    .map(|t| {
                        self.effective_click_tracking(engine, surface_id, t.mouse_tracking())
                            != tasty_terminal::MouseTrackingMode::None
                    })
                    .unwrap_or(false)
            {
                self.report_mouse_event(
                    engine,
                    surface_id,
                    x,
                    y,
                    1,
                    false,
                    button_state == ElementState::Released,
                );
                if button_state == ElementState::Pressed {
                    push_report_button(&mut self.report_buttons_down, 1, surface_id);
                }
            }
        }
    }

    /// 좌클릭 라우팅: 상태 갱신(left_mouse_down·vi_copy 종료) 후 링크클릭 →
    /// press(divider/selection) → release 로 위임.
    fn handle_left_button(&mut self, engine: &EngineRead<'_>, button_state: ElementState) {
        if button_state == ElementState::Pressed {
            self.left_mouse_down = true;
            // 이전 클릭의 링크 실행 여부를 비운다.
            self.link_click_consumed = false;
            // mouse drag 시작은 vi copy mode 와 충돌 — 자동 종료. (R7)
            if self.vi_copy.is_some() {
                self.vi_copy = None;
                self.base.state.dirty = true;
            }
        } else {
            self.left_mouse_down = false;
        }

        let terminal_rect = self.compute_terminal_rect();
        let Some(pos) = self.cursor_position else {
            return;
        };
        let (x, y) = (pos.x as f32, pos.y as f32);
        if self.try_handle_link_click(engine, x, y, &terminal_rect, button_state) {
            return;
        }
        if button_state == ElementState::Pressed {
            self.handle_left_press(engine, x, y, &terminal_rect);
        } else if button_state == ElementState::Released {
            self.handle_left_release(engine, x, y, &terminal_rect);
        }
    }

    /// 수식키+좌클릭 링크 라우팅. 매치되면 focus 갱신 후 링크 위면 열고(파일/외부 URL),
    /// 아니면 아무것도 안 함 — 어느 쪽이든 `true`(selection 경로로 안 샘).
    fn try_handle_link_click(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &crate::model::PhysicalRect,
        button_state: ElementState,
    ) -> bool {
        let modifier = LinkModifier::parse(&engine.settings.general.link_click_modifier);
        let mods = &self.base.state.modifiers;
        let link_mods_match = !matches!(modifier, LinkModifier::None)
            && modifier.matches(mods.control_key(), mods.alt_key(), mods.super_key());
        if !(link_mods_match && button_state == ElementState::Pressed) {
            return false;
        }
        // hard 점유 화면은 지연된 스냅샷이므로 링크를 열지 않는다(docs/dev-guide/attach-behavior.md).
        // false를 반환해 로컬 텍스트 선택은 계속 허용한다.
        if let Some(sid) = self.state.surface_id_at_position(
            engine,
            x,
            y,
            *terminal_rect,
            self.base.gpu.scale_factor(),
        ) && engine.live.occupancy.is_hard_occupied(sid)
        {
            return false;
        }
        if terminal_rect.contains(PhysicalPx(x), PhysicalPx(y)) {
            let scale_factor = self.base.gpu.scale_factor();
            let changed_pane =
                self.state
                    .focus_pane_at_position(engine, x, y, *terminal_rect, scale_factor);
            let changed_surf =
                self.state
                    .focus_surface_at_position(engine, x, y, *terminal_rect, scale_factor);
            if changed_pane || changed_surf {
                self.base.state.dirty = true;
            }
        }
        if let Some(hovered) = self.compute_hovered_link(&engine.as_ref()) {
            // 링크를 연 press는 앱에 보내지 않았으므로 release도 보내지 않는다.
            self.link_click_consumed = true;
            // 자식 PTY가 없는 mirror의 파일 경로는 원격 호스트 경로다.
            let is_mirror = engine.terminals.contains(hovered.surface_id)
                && !engine.terminals.has_pty(hovered.surface_id);
            match crate::file_dispatch::parse_link(&hovered.uri) {
                crate::file_dispatch::LinkKind::FileTarget(path) => {
                    // mirror의 경로는 원격 경로라 mirror origin으로 원격 열기 판정을 탄다.
                    self.state.dispatch_intent(
                        crate::file::dispatch::remote::terminal_link_file_intent(
                            crate::file::format::FileTarget::new(path),
                            hovered.surface_id,
                            is_mirror,
                        ),
                    );
                }
                crate::file_dispatch::LinkKind::External(uri) => {
                    // 외부 URL(http:// 등)은 mirror 여부와 무관하게 기존대로 처리.
                    let opened = terminal_link::open_uri(&uri);
                    if let Some(message) =
                        terminal_link::open_failure_message(opened, "toast.open_link_failed", &uri)
                    {
                        self.state.toasts.push(
                            message,
                            crate::adapters::ui::ToastKind::Error,
                            crate::adapters::ui::ToastScope::Surface(hovered.surface_id),
                        );
                    }
                }
            }
        }
        self.mark_dirty();
        true
    }

    /// 좌클릭 press: divider 히트 시 드래그 시작, 아니면 selection 시작으로 위임.
    fn handle_left_press(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &crate::model::PhysicalRect,
    ) {
        if !self.begin_divider_drag_at(engine, x, y, terminal_rect) {
            self.begin_left_selection(engine, x, y, terminal_rect);
        }
    }

    /// (`x`, `y`)가 pane·surface 분할선 hit 띠 안이면 그 분할선의 드래그를 시작하고 `true`.
    fn begin_divider_drag_at(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &crate::model::PhysicalRect,
    ) -> bool {
        let scale_factor = self.base.gpu.scale_factor();
        if let Some(info) =
            self.state
                .find_pane_divider_at(engine, x, y, *terminal_rect, scale_factor)
        {
            self.dragging_divider = self
                .state
                .begin_pane_divider(engine, info, *terminal_rect, scale_factor)
                .map(|sequence| DividerDrag { info, sequence });
            true
        } else if let Some(info) =
            self.state
                .find_surface_divider_at(engine, x, y, *terminal_rect, scale_factor)
        {
            self.dragging_divider = self
                .state
                .begin_surface_divider(engine, info, *terminal_rect, scale_factor)
                .map(|sequence| DividerDrag { info, sequence });
            true
        } else {
            false
        }
    }

    /// 좌버튼 press 가 분할선 hit 띠에 떨어지면 포커스를 바꾸지 않고 드래그를 시작한다(`true`).
    /// egui·오버레이·popup·배너·수식키 안내가 받은 press 는 그쪽이 우선이라 보지 않는다.
    /// 띠 안이면 수식키 링크 클릭보다도 분할선이 먼저다(일반 press 경로와 같은 우선순위다).
    fn try_press_divider_first(
        &mut self,
        engine: &EngineRead<'_>,
        button: MouseButton,
        button_state: ElementState,
        egui_consumed: bool,
        overlay_open: bool,
    ) -> bool {
        if button != MouseButton::Left
            || button_state != ElementState::Pressed
            || egui_consumed
            || overlay_open
            || self.state.popup_hovered
            || self.state.banner_hovered
            || self.state.modifier_hint_hovered
        {
            return false;
        }
        let Some(pos) = self.cursor_position else {
            return false;
        };
        let terminal_rect = self.compute_terminal_rect();
        if !self.begin_divider_drag_at(engine, pos.x as f32, pos.y as f32, &terminal_rect) {
            return false;
        }
        // handle_left_button 의 press 준비와 같다. release 는 일반 경로가 드래그를 끝낸다.
        self.left_mouse_down = true;
        self.link_click_consumed = false;
        if self.vi_copy.is_some() {
            self.vi_copy = None;
            self.base.state.dirty = true;
        }
        true
    }

    /// 좌클릭 로컬/보고 선택 시작. focus 전환 + IME flush 후, 순수
    /// `left_click_local_select` 결정에 따라 로컬 선택 시작 / 앱 보고 / Shift extend.
    fn begin_left_selection(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &crate::model::PhysicalRect,
    ) {
        let scale_factor = self.base.gpu.scale_factor();
        let (need_flush, mouse_tracking) = {
            let old_surface = self.state.focused_surface_id(engine);
            let changed_pane =
                self.state
                    .focus_pane_at_position(engine, x, y, *terminal_rect, scale_factor);
            let changed_surf =
                self.state
                    .focus_surface_at_position(engine, x, y, *terminal_rect, scale_factor);
            if changed_pane || changed_surf {
                self.base.state.dirty = true;
            }
            let ime_active = self.ime_preedit.is_some();
            let need_flush = ime_active && self.state.focused_surface_id(engine) != old_surface;
            // Start text selection (only if not mouse-tracking or Shift held)
            let mouse_tracking = self
                .state
                .focused_terminal(&engine.as_ref())
                .map(|t| t.mouse_tracking())
                .unwrap_or(tasty_terminal::MouseTrackingMode::None);
            (need_flush, mouse_tracking)
        };
        if need_flush {
            self.flush_ime_preedit(engine);
        }
        // 블랙리스트면 None 으로 격하 → 좌클릭이 로컬 텍스트 선택으로 빠지고 앱 보고/
        // 캡처 안내 배너 경로엔 진입하지 않는다.
        let mouse_tracking = self
            .state
            .focused_surface_id(engine)
            .map(|sid| self.effective_click_tracking(engine, sid, mouse_tracking))
            .unwrap_or(mouse_tracking);
        let shift = self.base.state.modifiers.shift_key();
        if mouse_tracking != tasty_terminal::MouseTrackingMode::None {
            if left_click_local_select(mouse_tracking, shift, false) {
                // Shift 우회는 press에서 결정하고 release까지 유지한다.
                // 트래킹 중에는 이전 로컬 앵커가 없어 선택을 새로 시작한다.
                self.left_select_bypass = true;
                self.start_selection(engine, x, y, terminal_rect);
            } else {
                // 트래킹 ON + Shift 없음: 버튼 press 를 앱에 보고 (docs/surfaces/terminal/index.md 앱 위임). 단,
                // 트래킹 진입 후 첫 캡처 상호작용이면 캡처 안내를 1회 띄운다.
                if let Some(sid) = self.state.focused_surface_id(engine) {
                    self.report_left_press_capture(engine, sid);
                    self.report_mouse_event(engine, sid, x, y, 0, false, false);
                    // press 를 보고했으니 이후 motion 도 좌버튼으로 보고한다.
                    push_report_button(&mut self.report_buttons_down, 0, sid);
                }
            }
        } else if shift {
            self.extend_selection(engine, x, y, terminal_rect);
        } else {
            self.start_selection(engine, x, y, terminal_rect);
        }
    }

    /// 트래킹 세션의 첫 캡처 조작에 Shift 우회 안내를 표시한다(docs/surfaces/terminal/index.md).
    /// 설정이 꺼져 있거나 배너 억제 목록에 해당하면 표시하지 않는다.
    /// 억제된 앱에서는 첫 조작 표지를 남겨 이후 다른 앱에서 안내할 수 있게 한다.
    fn report_left_press_capture(&mut self, engine: &EngineRead<'_>, surface_id: u32) {
        if mouse_capture_banner_suppressed(engine, surface_id) {
            return;
        }
        if engine.settings.general.mouse_capture_hint
            && let Some(target) =
                crate::runtime::surface_binding::SurfaceBinding::capture(engine, surface_id)
        {
            self.state.dispatch_intent(
                crate::intent::Intent::MouseCaptureHint {
                    target,
                    foreground_generation: engine.foreground_generation(surface_id),
                    view: self.base.state.identity(),
                }
                .from_user_shortcut("mouse_capture_hint"),
            );
        }
    }

    /// 좌클릭 release: divider 드래그 확정(resize) 후, 트래킹 ON 이면 앱 보고,
    /// 아니면 로컬 선택 확정(빈 클릭은 커서 이동 + 선택 클리어). bypass 는 앱 보고 스킵.
    /// press 가 링크오픈으로 소비됐으면(`link_click_consumed`) 마찬가지로 앱 보고 스킵
    /// (press/release 비대칭으로 인한 mouse-tracking 앱의 링크 중복 오픈 방지).
    fn handle_left_release(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &crate::model::PhysicalRect,
    ) {
        if self.finish_divider_drag(engine) {
            let cell_w = self.base.gpu.cell_width();
            let cell_h = self.base.gpu.cell_height();
            let scale_factor = self.base.gpu.scale_factor();
            self.state
                .resize_all(engine, *terminal_rect, cell_w, cell_h, scale_factor);
            self.base.state.dirty = true;
        }
        // 트래킹 ON 이면 release 를 앱에 보고, 아니면 로컬 선택 완료. 단, Shift+좌클릭
        // 우회 시퀀스(left_select_bypass)면 — dragging 여부와 무관하게(멀티클릭 word/line
        // 은 dragging=false) — 앱 보고를 스킵하고 로컬 선택을 확정한다.
        let bypass = self.left_select_bypass;
        let link_click_consumed = self.link_click_consumed;
        let report_surface = if bypass {
            None
        } else {
            self.state.focused_surface_id(engine).filter(|sid| {
                engine
                    .find_terminal_by_id(*sid)
                    .map(|t| {
                        should_report_release_to_app(
                            self.effective_click_tracking(engine, *sid, t.mouse_tracking()),
                            link_click_consumed,
                        )
                    })
                    .unwrap_or(false)
            })
        };
        if let Some(sid) = report_surface {
            self.report_mouse_event(engine, sid, x, y, 0, false, true);
        } else {
            let empty = if let Some(sel) = &mut self.text_selection {
                sel.dragging = false;
                sel.is_empty()
            } else {
                false
            };
            if empty {
                // bypass 단일(빈) 클릭은 커서 이동 없이 선택만 클리어한다. 일반 단일
                // 클릭은 클릭 위치로 커서 이동 후 클리어.
                if !bypass {
                    self.move_cursor_to_click(engine, x, y, terminal_rect);
                }
                self.text_selection = None;
            }
        }
        self.left_select_bypass = false;
        self.link_click_consumed = false;
        self.mark_dirty();
    }

    /// 클릭/드래그 픽셀 좌표를 해당 surface 의 viewport 1-based `(col, row)` 로 변환
    /// (마우스 리포팅 전송용). surface 를 못 찾으면 `(1, 1)`.
    fn mouse_cell_for_report(
        &self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        x: f32,
        y: f32,
    ) -> (usize, usize) {
        let terminal_rect = self.compute_terminal_rect();
        let cell_w = self.base.gpu.cell_width();
        let cell_h = self.base.gpu.cell_height();
        let Some(terminal) = engine.visible_terminal(surface_id) else {
            return (1, 1);
        };
        let Some(rect) = self.state.surface_rect_by_id(
            engine,
            surface_id,
            terminal_rect,
            self.base.gpu.scale_factor(),
        ) else {
            return (1, 1);
        };
        terminal.with_view(&self.state.terminal_views.get(engine, surface_id), |view| {
            let point =
                crate::selection::pixel_to_grid(x, y, &rect, cell_w, cell_h, view.viewport());
            let row = point
                .absolute_row
                .saturating_sub(view.viewport().top_row)
                .min(view.rows().saturating_sub(1))
                + 1;
            let col = point.col.min(view.cols().saturating_sub(1)) + 1;
            (col, row)
        })
    }

    /// 캡처 제외 목록에 해당하거나 hard 점유 중이면 클릭 트래킹을 None으로 처리한다.
    /// 앱에 보고하는 대신 로컬 선택·메뉴를 허용한다(docs/dev-guide/attach-behavior.md).
    /// 휠은 이 함수를 쓰지 않고 별도로 hard 점유를 차단한다.
    fn effective_click_tracking(
        &self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface_id: u32,
        actual: tasty_terminal::MouseTrackingMode,
    ) -> tasty_terminal::MouseTrackingMode {
        crate::state::mouse::effective_click_tracking_decision(
            engine.live.occupancy.is_hard_occupied(surface_id),
            engine.is_surface_mouse_capture_disabled(surface_id),
            actual,
        )
    }

    /// DECSET 1003의 버튼 없는 셀 이동을 보고한다.
    /// OS 창과 대상 surface 모두 포커스되어야 하며 포커스를 옮기지는 않는다.
    /// divider와 창 리사이즈 영역에서는 보고하지 않는다(docs/surfaces/terminal/index.md).
    fn report_hover_motion(
        &mut self,
        engine: &EngineRead<'_>,
        x: f32,
        y: f32,
        terminal_rect: &crate::model::PhysicalRect,
    ) {
        let Some(sid) = self.state.focused_surface_id(engine) else {
            return;
        };
        let tracking = engine
            .find_terminal_by_id(sid)
            .map(|t| t.mouse_tracking())
            .unwrap_or(tasty_terminal::MouseTrackingMode::None);
        // 클릭과 같은 제외 목록·hard 점유 조건을 적용한다.
        let tracking = self.effective_click_tracking(engine, sid, tracking);
        if tracking != tasty_terminal::MouseTrackingMode::AllMotion {
            // 여기서 끊어야 아래 hit-test 들이 매 프레임 헛돌지 않는다.
            return;
        }
        let threshold =
            crate::state::mouse::divider_hit_threshold_physical(self.base.gpu.scale_factor());
        let on_divider_band = self
            .state
            .find_pane_divider_at(engine, x, y, *terminal_rect, threshold)
            .or_else(|| {
                self.state
                    .find_surface_divider_at(&*engine, x, y, *terminal_rect, threshold)
            })
            .is_some();
        let over_focused_surface = self.state.surface_id_at_position(
            engine,
            x,
            y,
            *terminal_rect,
            self.base.gpu.scale_factor(),
        ) == Some(sid);
        if !should_report_hover_motion(HoverReportInput {
            window_focused: self.base.state.focused,
            over_focused_surface,
            on_divider_band,
            // macOS 는 네이티브 데코가 가장자리를 처리해 이 값이 항상 None 이다 —
            // 그쪽에서는 이 가드가 무동작이고, 그게 의도한 플랫폼 차이다.
            resize_edge_active: self.state.pending_resize_cursor.is_some(),
            dragging_divider: self.dragging_divider.is_some(),
        }) {
            return;
        }
        let (col, row) = self.mouse_cell_for_report(&engine.as_ref(), sid, x, y);
        if self.last_mouse_report_cell == Some((sid, col, row)) {
            return;
        }
        self.report_mouse_event(engine, sid, x, y, MOUSE_BUTTON_NONE, true, false);
    }

    /// 마우스 버튼/드래그 이벤트를 트래킹 앱(PTY)에 보고한다. `button` 0=left /
    /// 1=middle / 2=right, `motion` 드래그 여부, `release` 버튼 떼기. 좌표/SGR 여부는
    /// 보고 시점에 해당 surface 에서 조회한다.
    #[allow(clippy::too_many_arguments)] // reason: 인자가 마우스 이벤트 그 자체라 묶으면 호출부 6곳이 전부 구조체 리터럴이 된다
    fn report_mouse_event(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        x: f32,
        y: f32,
        button: u8,
        motion: bool,
        release: bool,
    ) {
        let (col, row) = self.mouse_cell_for_report(&engine.as_ref(), surface_id, x, y);
        let sgr = engine
            .find_terminal_by_id(surface_id)
            .map(|t| t.sgr_mouse())
            .unwrap_or(false);
        let m = &self.base.state.modifiers;
        let cb = mouse_report_cb(button, motion, m.shift_key(), m.alt_key(), m.control_key());
        let bytes = tasty_terminal::encode_mouse_report(sgr, cb, col, row, release);
        self.state.dispatch_intent(
            DomainIntent::SendToSurface {
                surface_id,
                payload: SendPayload::Bytes(bytes),
            }
            .from_user_shortcut("mouse_report"),
        );
        self.last_mouse_report_cell = Some((surface_id, col, row));
    }

    pub(super) fn handle_mouse_wheel(
        &mut self,
        engine: &EngineRead<'_>,
        delta: MouseScrollDelta,
        egui_consumed: bool,
    ) {
        crate::gfx::perf::wheel_received();
        let overlay_open = self.mouse_overlay_open();
        if egui_consumed {
            self.mark_dirty();
        }
        if !egui_consumed
            && !overlay_open
            && !self.state.popup_hovered
            && !self.state.banner_hovered
            && !self.state.modifier_hint_hovered
        {
            // egui-mesh surface 휠 forward (A1-S7): 포인터가 egui-mesh surface 위면
            // 스크롤 델타를 논리 포인트로 변환해 누적하고 소비한다.
            if let Some(pos) = self.cursor_position {
                let (x, y) = (pos.x as f32, pos.y as f32);
                // 노치 거리는 host egui 옵션이 런타임 단일 출처다 — host 위젯이 스크롤하는
                // 거리와 plugin 표면이 받는 거리를 같게 유지한다(docs/surfaces/terminal/index.md).
                let line_scroll =
                    crate::plugin_bridge::wire_scroll::line_scroll(&self.base.gpu.egui_ctx);
                if let Some((sid, _plugin_id, _rect)) = self.egui_mesh_target_at(engine, x, y) {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(lx, ly) => {
                            ((line_scroll * lx).value(), (line_scroll * ly).value())
                        }
                        MouseScrollDelta::PixelDelta(p) => {
                            let ppp = self.base.gpu.scale_factor().max(f32::EPSILON);
                            (
                                PhysicalPx(p.x as f32).to_logical(ppp).value(),
                                PhysicalPx(p.y as f32).to_logical(ppp).value(),
                            )
                        }
                    };
                    self.egui_mesh_push_scroll(sid, dx, dy);
                    self.mark_dirty();
                    return;
                }
                // attach mesh mirror surface 휠 forward — 위와 동형이되
                // 목적지가 원격.
                if let Some((sid, _rect)) = self.attach_mesh_target_at(engine, x, y) {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(lx, ly) => {
                            ((line_scroll * lx).value(), (line_scroll * ly).value())
                        }
                        MouseScrollDelta::PixelDelta(p) => {
                            let ppp = self.base.gpu.scale_factor().max(f32::EPSILON);
                            (
                                PhysicalPx(p.x as f32).to_logical(ppp).value(),
                                PhysicalPx(p.y as f32).to_logical(ppp).value(),
                            )
                        }
                    };
                    self.attach_mesh_push_scroll(sid, dx, dy);
                    self.mark_dirty();
                    return;
                }
            }

            // Find the surface under the cursor, falling back to the focused surface
            let terminal_rect = self.compute_terminal_rect();
            let target_id = self
                .cursor_position
                .and_then(|pos| {
                    let (x, y) = (pos.x as f32, pos.y as f32);
                    self.state.surface_id_at_position(
                        &*engine,
                        x,
                        y,
                        terminal_rect,
                        self.base.gpu.scale_factor(),
                    )
                })
                .or_else(|| self.state.focused_surface_id(&*engine));

            if let Some(surface_id) = target_id {
                // hard 점유에서는 휠을 막는다. 표시 중인 mirror 대신 live 터미널에
                // 보고하거나 스크롤 위치를 바꾸면 점유 해제 뒤 화면이 달라질 수 있다.
                if engine.live.occupancy.is_hard_occupied(surface_id) {
                    return;
                }
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as i32,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as i32,
                };
                if lines == 0 {
                    return;
                }
                let viewport = self.state.terminal_views.get(engine, surface_id);
                let info = engine.as_ref().visible_terminal(surface_id).map(|t| {
                    t.with_view(&viewport, |view| {
                        (
                            view.is_alternate_screen(),
                            view.mouse_tracking(),
                            view.sgr_mouse(),
                            view.viewport(),
                        )
                    })
                });
                let Some((is_alt, tracking, sgr, content)) = info else {
                    return;
                };
                let (cols, rows) = (content.cut.cols, content.cut.rows);

                if tracking != tasty_terminal::MouseTrackingMode::None {
                    // 마우스 추적이 켜져 있으면 휠을 마우스 이벤트로 전송한다 (표준
                    // 동작). alt screen 이라고 무조건 arrow 로 바꾸면, 앱(예: Claude
                    // Code)이 그 arrow 를 history 이동으로 해석해 스크롤이 깨진다.
                    let cell_w = self.base.gpu.cell_width();
                    let cell_h = self.base.gpu.cell_height();
                    let (col, row) = self
                        .cursor_position
                        .and_then(|pos| {
                            let (x, y) = (pos.x as f32, pos.y as f32);
                            let rect = self.state.surface_rect_by_id(
                                &*engine,
                                surface_id,
                                terminal_rect,
                                self.base.gpu.scale_factor(),
                            )?;
                            let point = crate::selection::pixel_to_grid(
                                x, y, &rect, cell_w, cell_h, content,
                            );
                            // viewport 기준 1-based (col, row). alt screen 은 scrollback
                            // 이 없어 absolute_row 가 곧 viewport row.
                            let viewport_top = content.top_row;
                            let row = point
                                .absolute_row
                                .saturating_sub(viewport_top)
                                .min(rows.saturating_sub(1))
                                + 1;
                            let col = point.col.min(cols.saturating_sub(1)) + 1;
                            Some((col, row))
                        })
                        .unwrap_or((1, 1));
                    // xterm wheel button: 64 = up, 65 = down.
                    let btn = if lines > 0 { 64 } else { 65 };
                    let count = lines.unsigned_abs() as usize;
                    let bytes = encode_wheel_report(sgr, btn, col, row, count);
                    self.state.dispatch_intent(
                        DomainIntent::SendToSurface {
                            surface_id,
                            payload: SendPayload::Bytes(bytes),
                        }
                        .from_user_shortcut("mouse_wheel"),
                    );
                } else if is_alt {
                    // 마우스 추적 OFF + alt screen — alternate scroll mode: 휠을 arrow
                    // 키로 변환 (vim/less 등에서 휠 스크롤). lines 만큼 한 Vec 에 concat
                    // 후 1 Intent (큐 폭증 회피).
                    let seq: &[u8] = if lines > 0 { b"\x1b[A" } else { b"\x1b[B" };
                    let count = lines.unsigned_abs() as usize;
                    let mut bytes = Vec::with_capacity(seq.len() * count);
                    for _ in 0..count {
                        bytes.extend_from_slice(seq);
                    }
                    self.state.dispatch_intent(
                        DomainIntent::SendToSurface {
                            surface_id,
                            payload: SendPayload::Bytes(bytes),
                        }
                        .from_user_shortcut("mouse_wheel"),
                    );
                } else {
                    // 일반 화면 — scrollback (UI 자체 mutate, PTY 와 무관).
                    self.state.terminal_views.update(
                        &engine.as_ref(),
                        surface_id,
                        |viewport, cut| {
                            if lines > 0 {
                                viewport.scroll_up(cut, lines as usize);
                            } else if lines < 0 {
                                viewport.scroll_down(cut, (-lines) as usize);
                            }
                        },
                    );
                    self.base.state.dirty = true;
                }
            }
        }
    }
}

/// 앱에 press 를 보고한 버튼을 스택 맨 위로 올린다. 이미 있으면 최신 위치로
/// 옮긴다 — motion cb 는 "가장 최근에 누른 버튼" 을 싣는다(xterm 관례).
fn push_report_button(stack: &mut Vec<(u8, u32)>, button: u8, surface_id: u32) {
    stack.retain(|(b, _)| *b != button);
    stack.push((button, surface_id));
}

/// 보고 스택에서 버튼을 내린다. 남은 버튼이 있으면 그다음 최근 press 가 motion
/// 버튼이 된다(여러 버튼을 겹쳐 눌렀다 하나만 뗀 경우).
fn pop_report_button(stack: &mut Vec<(u8, u32)>, button: u8) {
    stack.retain(|(b, _)| *b != button);
}

/// winit 버튼 → 마우스 리포팅 버튼 코드(0=left/1=middle/2=right). 그 외 버튼은
/// 보고 대상이 아니다.
fn report_button_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

/// "버튼 없음" 을 뜻하는 마우스 리포팅 버튼 코드. xterm 규약상 하위 2 비트가 `3` 이면
/// 버튼이 눌리지 않은 상태이고, motion 비트를 얹으면 `3|32 = 35` 가 된다.
const MOUSE_BUTTON_NONE: u8 = 3;

/// 트래킹 모드를 확인한 뒤 hover 보고 여부를 결정하는 입력.
#[derive(Debug, Clone, Copy)]
struct HoverReportInput {
    /// tasty 창 자체가 포커스를 갖고 있는가.
    window_focused: bool,
    /// 커서 아래 surface 가 focused surface 인가.
    over_focused_surface: bool,
    /// divider 히트 밴드(분할선 ±threshold) 안인가.
    on_divider_band: bool,
    /// OS 창 리사이즈 가장자리 밴드 안인가.
    resize_edge_active: bool,
    /// divider 드래그가 진행 중인가.
    dragging_divider: bool,
}

/// 버튼 없는 hover motion 을 앱에 보고할지. 호출자가 트래킹이 `AllMotion` 임을
/// 확인한 뒤의 위치 판정만 담당한다 — 하나라도 걸리면 보고하지 않는다.
fn should_report_hover_motion(i: HoverReportInput) -> bool {
    i.window_focused
        && i.over_focused_surface
        && !i.on_divider_band
        && !i.resize_edge_active
        && !i.dragging_divider
}

/// winit 버튼(0=left / 1=middle / 2=right, 3=버튼 없음) + 드래그/modifier → 마우스 리포팅 `cb` 코드.
/// shift=4 · alt(meta)=8 · ctrl=16, 드래그 motion=32. (xterm 표준 비트)
fn mouse_report_cb(button: u8, motion: bool, shift: bool, alt: bool, ctrl: bool) -> u8 {
    let mut cb = button;
    if motion {
        cb |= 32;
    }
    if shift {
        cb |= 4;
    }
    if alt {
        cb |= 8;
    }
    if ctrl {
        cb |= 16;
    }
    cb
}

fn encode_wheel_report(sgr: bool, btn: u32, col: usize, row: usize, count: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..count {
        bytes.extend_from_slice(&tasty_terminal::encode_mouse_report(
            sgr, btn as u8, col, row, false,
        ));
    }
    bytes
}

/// terminal 우클릭 메뉴를 세우는 버튼 상태. Linux 는 release, 그 외는 press — 이유는
/// `handle_right_button` 의 주석(GTK `popup_at_rect` 가 버튼이 눌린 채로는 no-op).
pub(super) fn terminal_menu_open_state() -> ElementState {
    if cfg!(target_os = "linux") {
        ElementState::Released
    } else {
        ElementState::Pressed
    }
}

/// 우클릭을 앱(PTY)에 위임할지 결정한다. 트래킹 ON 이고 Shift 가 없을 때만 위임하고
/// (docs/surfaces/terminal/index.md), 트래킹 OFF 이거나 Shift+우클릭이면 tasty 컨텍스트 메뉴로 우회한다 (docs/surfaces/terminal/index.md).
fn right_click_delegates_to_app(tracking: tasty_terminal::MouseTrackingMode, shift: bool) -> bool {
    tracking != tasty_terminal::MouseTrackingMode::None && !shift
}

/// 앱에 press를 보냈고 트래킹이 켜져 있을 때만 release도 보고한다.
/// 링크 실행으로 소비한 press의 release를 단독 전송하지 않는다.
fn should_report_release_to_app(
    tracking: tasty_terminal::MouseTrackingMode,
    link_click_consumed: bool,
) -> bool {
    tracking != tasty_terminal::MouseTrackingMode::None && !link_click_consumed
}

/// 좌클릭을 tasty 로컬 텍스트 선택으로 처리할지(true), 아니면 앱(PTY)에 보고할지(false)
/// 결정한다. 트래킹 OFF 면 항상 로컬. 트래킹 ON 이면 press 시점 Shift 우회이거나
/// (`shift`), 이미 우회 시퀀스가 활성(`bypass_active`)일 때만 로컬 — 그 외엔 앱에 보고.
/// `bypass_active` 는 press 에서 set 된 `left_select_bypass` 로, motion/release 가
/// Shift 재검사 없이 이 플래그만으로 같은 결정을 유지하게 한다 (멀티클릭 dragging=false 포함).
fn left_click_local_select(
    tracking: tasty_terminal::MouseTrackingMode,
    shift: bool,
    bypass_active: bool,
) -> bool {
    tracking == tasty_terminal::MouseTrackingMode::None || shift || bypass_active
}

/// 실제 위젯이나 오버레이가 입력을 받는 위치에서는 창 리사이즈를 시작하지 않는다.
/// 패널 전체 영역을 제외하면 타이틀바·상태바의 빈 가장자리도 사용할 수 없게 된다.
/// macOS는 native 장식을 사용하므로 호출하지 않는다.
// 이유: macOS 는 네이티브 데코라 호출부 `try_begin_os_resize` 가 그 빌드에서 빠진다(위).
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn resize_should_yield_to_content(
    resize_edge_widget_hovered: bool,
    overlay_open: bool,
    popup_hovered: bool,
    banner_hovered: bool,
    window_size_locked: bool,
) -> bool {
    resize_edge_widget_hovered
        || overlay_open
        || popup_hovered
        || banner_hovered
        || window_size_locked
}

/// 커서 이동을 오버레이나 egui가 처리했으면 mesh 조회를 생략한다.
/// 호출자는 반환 전에 update_mesh_hover(None)으로 이전 hover를 해제한다.
/// host 분할선 드래그 중이면 생략하지 않는다. 분할선 띠의 press 를 egui 위젯도 클릭 후보로
/// 잡으면(`is_using_pointer`) 이후 이동을 egui 가 소비했다고 알린다. 배율 2 에서 html pane 아래
/// 분할선을 잡을 때 관측했다. 그 이동을 건너뛰면 드래그한 분할선이 움직이지 않는다.
fn cursor_moved_should_short_circuit(
    dragging_divider: bool,
    egui_consumed: bool,
    overlay_open: bool,
    popup_hovered: bool,
    banner_hovered: bool,
    modifier_hint_hovered: bool,
) -> bool {
    !dragging_divider
        && (egui_consumed
            || overlay_open
            || popup_hovered
            || banner_hovered
            || modifier_hint_hovered)
}

/// 메뉴를 닫는 클릭의 press와 release를 모두 소비한다.
/// press를 소비한 버튼을 기억해 메뉴가 먼저 닫혀도 release를 egui에 보내지 않는다.
/// 그렇지 않으면 메뉴 아래 위젯이 같은 클릭을 처리할 수 있다.
fn menu_dismiss_swallow_step(
    swallowed: &mut Vec<MouseButton>,
    menu_open: bool,
    button_state: ElementState,
    button: MouseButton,
) -> bool {
    match button_state {
        ElementState::Pressed => {
            if !menu_open {
                return false;
            }
            if !swallowed.contains(&button) {
                swallowed.push(button);
            }
            true
        }
        ElementState::Released => {
            let Some(idx) = swallowed.iter().position(|b| *b == button) else {
                return false;
            };
            swallowed.remove(idx);
            true
        }
    }
}

/// `update_mesh_hover`(MainView 메서드)의 순수 결정 로직. 슬롯의 다음 값과,
/// `PointerGone` 을 보내야 할 이전 대상(있다면)을 반환한다. 대상이 안 바뀌면(같은
/// surface 에 머무르거나 계속 `None`) `PointerGone` 을 보내지 않는다.
fn mesh_hover_transition(
    current: Option<MeshHoverTarget>,
    new: Option<MeshHoverTarget>,
) -> (Option<MeshHoverTarget>, Option<MeshHoverTarget>) {
    if current == new {
        (current, None)
    } else {
        (new, current)
    }
}

/// 메뉴를 닫는 클릭의 상태 전이와 egui 위젯 반응을 확인한다.
/// GPU가 필요한 MainView 대신 egui::Context에 동일한 입력을 전달한다.
/// 입력을 그대로 보낸 경우와 소비한 경우를 비교한다.
#[cfg(test)]
mod menu_dismiss_tests {
    use super::menu_dismiss_swallow_step;
    use winit::event::{ElementState, MouseButton};

    const OPEN: bool = true;
    const CLOSED: bool = false;

    #[test]
    fn press_while_menu_open_starts_swallowing_the_cycle() {
        let mut sw = Vec::new();
        assert!(menu_dismiss_swallow_step(
            &mut sw,
            OPEN,
            ElementState::Pressed,
            MouseButton::Left
        ));
        assert_eq!(sw, vec![MouseButton::Left]);
    }

    #[test]
    fn matching_release_is_swallowed_even_after_menu_slot_cleared() {
        // press 로 dismiss 를 건 뒤 release 전에 poll 이 메뉴를 회수하는 것이 정상
        // 경로다. 그래도 짝이 되는 release 는 삼켜져야 egui 가 쌍을 완성하지 못한다.
        let mut sw = Vec::new();
        assert!(menu_dismiss_swallow_step(
            &mut sw,
            OPEN,
            ElementState::Pressed,
            MouseButton::Left
        ));
        assert!(menu_dismiss_swallow_step(
            &mut sw,
            CLOSED,
            ElementState::Released,
            MouseButton::Left
        ));
        assert!(sw.is_empty(), "사이클이 끝나면 삼킴 상태가 남지 않는다");
    }

    #[test]
    fn release_without_swallowed_press_passes_through() {
        // 메뉴가 떠 있는 동안 눌린 적 없는 버튼의 release(예: 메뉴가 뜨기 전에
        // 시작된 드래그의 release)는 정상 라우팅되어야 한다.
        let mut sw = Vec::new();
        assert!(!menu_dismiss_swallow_step(
            &mut sw,
            OPEN,
            ElementState::Released,
            MouseButton::Left
        ));
    }

    #[test]
    fn press_without_menu_passes_through() {
        let mut sw = Vec::new();
        assert!(!menu_dismiss_swallow_step(
            &mut sw,
            CLOSED,
            ElementState::Pressed,
            MouseButton::Left
        ));
        assert!(sw.is_empty());
    }

    #[test]
    fn other_button_release_does_not_end_the_swallowed_cycle() {
        let mut sw = Vec::new();
        assert!(menu_dismiss_swallow_step(
            &mut sw,
            OPEN,
            ElementState::Pressed,
            MouseButton::Right
        ));
        assert!(!menu_dismiss_swallow_step(
            &mut sw,
            CLOSED,
            ElementState::Released,
            MouseButton::Left
        ));
        assert!(menu_dismiss_swallow_step(
            &mut sw,
            CLOSED,
            ElementState::Released,
            MouseButton::Right
        ));
    }

    /// 버튼 하나의 클릭 사이클을 egui 에 흘려 위젯이 클릭으로 인식하는지 본다.
    /// `swallow=true` 면 `handle_event` 의 삼킴 경로와 동일하게 press/release 를 **넣지
    /// 않고** `PointerGone` 만 넣는다. 반환값 = (primary clicked, secondary clicked).
    fn click_cycle_reaches_widget(button: egui::PointerButton, swallow: bool) -> (bool, bool) {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 100.0));
        let mut center = egui::Pos2::ZERO;
        let mut primary = false;
        let mut secondary = false;
        let frame = |events: Vec<egui::Event>,
                     center: &mut egui::Pos2,
                     primary: &mut bool,
                     secondary: &mut bool| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            // 프레임 출력(텍스처/셰이프)은 이 테스트의 관심사가 아니다 — 위젯 응답만 본다.
            let _frame_output = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let resp = ui.button("target");
                    *center = resp.rect.center();
                    *primary |= resp.clicked();
                    *secondary |= resp.secondary_clicked();
                });
            });
        };
        // 1) 레이아웃 확보 → 위젯 rect 를 얻는다. 2) 그 위로 포인터를 올린다.
        frame(Vec::new(), &mut center, &mut primary, &mut secondary);
        frame(
            vec![egui::Event::PointerMoved(center)],
            &mut center,
            &mut primary,
            &mut secondary,
        );
        let btn = |pressed: bool| egui::Event::PointerButton {
            pos: center,
            button,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        let (press_frame, release_frame) = if swallow {
            (vec![egui::Event::PointerGone], Vec::new())
        } else {
            (vec![btn(true)], vec![btn(false)])
        };
        frame(press_frame, &mut center, &mut primary, &mut secondary);
        frame(release_frame, &mut center, &mut primary, &mut secondary);
        frame(Vec::new(), &mut center, &mut primary, &mut secondary);
        (primary, secondary)
    }

    #[test]
    fn unswallowed_cycle_does_fire_the_widget() {
        // 입력을 그대로 보내면 egui 위젯이 클릭을 처리한다.
        assert_eq!(
            click_cycle_reaches_widget(egui::PointerButton::Primary, false),
            (true, false)
        );
        assert_eq!(
            click_cycle_reaches_widget(egui::PointerButton::Secondary, false),
            (false, true)
        );
    }

    #[test]
    fn swallowed_cycle_never_completes_a_click_in_egui() {
        // 본 검증 — 메뉴를 닫는 클릭은 그 밑의 위젯을 실행시키지 않는다.
        assert_eq!(
            click_cycle_reaches_widget(egui::PointerButton::Primary, true),
            (false, false)
        );
        // 우클릭이 새 컨텍스트 메뉴를 큐에 넣는 경로도 함께 막힌다.
        assert_eq!(
            click_cycle_reaches_widget(egui::PointerButton::Secondary, true),
            (false, false)
        );
    }
}

#[cfg(test)]
mod wheel_tests {
    use super::encode_wheel_report;

    #[test]
    fn sgr_encodes_button_col_row() {
        assert_eq!(encode_wheel_report(true, 64, 3, 5, 1), b"\x1b[<64;3;5M");
        assert_eq!(encode_wheel_report(true, 65, 10, 20, 1), b"\x1b[<65;10;20M");
    }

    #[test]
    fn x10_encodes_with_32_offset() {
        // btn=64 → 96, col=1 → 33, row=1 → 33.
        assert_eq!(
            encode_wheel_report(false, 64, 1, 1, 1),
            vec![0x1b, b'[', b'M', 96, 33, 33]
        );
    }

    #[test]
    fn count_repeats_sequence() {
        assert_eq!(
            encode_wheel_report(true, 64, 1, 1, 3),
            b"\x1b[<64;1;1M\x1b[<64;1;1M\x1b[<64;1;1M"
        );
    }

    #[test]
    fn x10_clamps_large_coords() {
        // 32 + 300 = 332 → clamp 255.
        let out = encode_wheel_report(false, 64, 300, 1, 1);
        assert_eq!(out[4], 255);
    }
}

#[cfg(test)]
mod right_click_tests {
    use super::right_click_delegates_to_app;
    use tasty_terminal::MouseTrackingMode;

    #[test]
    fn tracking_on_no_shift_delegates_to_app() {
        // docs/surfaces/terminal/index.md: 트래킹 ON + Shift 없음 → 앱에 위임 (tasty 메뉴 안 뜸).
        assert!(right_click_delegates_to_app(
            MouseTrackingMode::Click,
            false
        ));
        assert!(right_click_delegates_to_app(
            MouseTrackingMode::CellMotion,
            false
        ));
        assert!(right_click_delegates_to_app(
            MouseTrackingMode::AllMotion,
            false
        ));
    }

    #[test]
    fn tracking_on_with_shift_bypasses_to_menu() {
        // docs/surfaces/terminal/index.md: 트래킹 ON + Shift → 앱에 보고 안 하고 tasty 컨텍스트 메뉴로 우회.
        assert!(!right_click_delegates_to_app(
            MouseTrackingMode::Click,
            true
        ));
        assert!(!right_click_delegates_to_app(
            MouseTrackingMode::AllMotion,
            true
        ));
    }

    #[test]
    fn tracking_off_always_shows_menu() {
        // 트래킹 OFF: Shift 유무와 무관하게 메뉴 (위임 안 함).
        assert!(!right_click_delegates_to_app(
            MouseTrackingMode::None,
            false
        ));
        assert!(!right_click_delegates_to_app(MouseTrackingMode::None, true));
    }
}

#[cfg(test)]
mod left_click_tests {
    use super::left_click_local_select;
    use tasty_terminal::MouseTrackingMode;

    #[test]
    fn tracking_on_shift_press_starts_local_select() {
        // 트래킹 ON + Shift+press → 로컬 선택 시작 (앱에 보고 안 함).
        assert!(left_click_local_select(
            MouseTrackingMode::AllMotion,
            true,
            false
        ));
        assert!(left_click_local_select(
            MouseTrackingMode::CellMotion,
            true,
            false
        ));
        assert!(left_click_local_select(
            MouseTrackingMode::Click,
            true,
            false
        ));
    }

    #[test]
    fn tracking_on_no_shift_reports_to_app() {
        // 트래킹 ON + Shift 없음 + bypass 비활성 → 앱에 보고 (로컬 선택 안 함).
        assert!(!left_click_local_select(
            MouseTrackingMode::AllMotion,
            false,
            false
        ));
        assert!(!left_click_local_select(
            MouseTrackingMode::CellMotion,
            false,
            false
        ));
    }

    #[test]
    fn tracking_on_bypass_active_stays_local() {
        // 트래킹 ON + bypass 활성(press 에서 set) → motion/release 는 로컬 경로 유지.
        // shift 가 false 여도(드래그 중 Shift 해제) bypass 가 결정을 유지한다.
        // 멀티클릭 word/line(dragging=false)도 같은 플래그로 로컬 유지 — 앱에 안 샌다.
        assert!(left_click_local_select(
            MouseTrackingMode::AllMotion,
            false,
            true
        ));
    }

    #[test]
    fn tracking_off_always_local() {
        // 트래킹 OFF → 항상 로컬 (Shift/bypass 무관).
        assert!(left_click_local_select(
            MouseTrackingMode::None,
            false,
            false
        ));
        assert!(left_click_local_select(
            MouseTrackingMode::None,
            true,
            false
        ));
        assert!(left_click_local_select(
            MouseTrackingMode::None,
            false,
            true
        ));
    }
}

#[cfg(test)]
mod link_click_release_tests {
    use super::should_report_release_to_app;

    #[test]
    fn release_report_skipped_when_press_consumed_by_link_click() {
        // tracking ON + 이번 클릭의 press가 링크오픈으로 소비됐다면 release도 보고 안 함
        assert!(!should_report_release_to_app(
            tasty_terminal::MouseTrackingMode::Click,
            true,
        ));
    }

    #[test]
    fn release_report_still_sent_for_normal_click_with_tracking_on() {
        // 기존 동작 보존: 링크오픈이 아닌 일반 클릭은 tracking ON 이면 그대로 보고
        assert!(should_report_release_to_app(
            tasty_terminal::MouseTrackingMode::Click,
            false,
        ));
    }

    #[test]
    fn release_report_skipped_when_tracking_off_regardless() {
        assert!(!should_report_release_to_app(
            tasty_terminal::MouseTrackingMode::None,
            false,
        ));
    }
}

#[cfg(test)]
mod mesh_hover_tests {
    use super::{MeshHoverTarget, mesh_hover_transition};

    #[test]
    fn same_target_is_not_a_transition() {
        // 같은 local surface 에 계속 머무르면 PointerGone 을 보내지 않는다.
        let (next, gone) = mesh_hover_transition(
            Some(MeshHoverTarget::Local(1)),
            Some(MeshHoverTarget::Local(1)),
        );
        assert_eq!(next, Some(MeshHoverTarget::Local(1)));
        assert_eq!(gone, None);
    }

    #[test]
    fn none_to_none_is_not_a_transition() {
        let (next, gone) = mesh_hover_transition(None, None);
        assert_eq!(next, None);
        assert_eq!(gone, None);
    }

    #[test]
    fn entering_a_surface_from_none_sends_no_gone() {
        // 이전에 아무것도 hover 하지 않았으면 보낼 PointerGone 대상이 없다.
        let (next, gone) = mesh_hover_transition(None, Some(MeshHoverTarget::Local(1)));
        assert_eq!(next, Some(MeshHoverTarget::Local(1)));
        assert_eq!(gone, None);
    }

    #[test]
    fn leaving_window_sends_gone_for_previous_local_surface() {
        // CursorLeft 등으로 target 이 None 이 되면 이전 local surface 에 PointerGone.
        let (next, gone) = mesh_hover_transition(Some(MeshHoverTarget::Local(1)), None);
        assert_eq!(next, None);
        assert_eq!(gone, Some(MeshHoverTarget::Local(1)));
    }

    #[test]
    fn switching_between_local_surfaces_sends_gone_for_the_old_one() {
        // 창을 벗어나지 않고 다른 mesh surface 로 바로 넘어가도 이전 surface 에
        // PointerGone 을 보낸다.
        let (next, gone) = mesh_hover_transition(
            Some(MeshHoverTarget::Local(1)),
            Some(MeshHoverTarget::Local(2)),
        );
        assert_eq!(next, Some(MeshHoverTarget::Local(2)));
        assert_eq!(gone, Some(MeshHoverTarget::Local(1)));
    }

    #[test]
    fn switching_from_local_to_attach_sends_gone_for_local() {
        let (next, gone) = mesh_hover_transition(
            Some(MeshHoverTarget::Local(1)),
            Some(MeshHoverTarget::Attach(9)),
        );
        assert_eq!(next, Some(MeshHoverTarget::Attach(9)));
        assert_eq!(gone, Some(MeshHoverTarget::Local(1)));
    }
}

/// early-return 조건과 mesh hover 해제 결과를 각각 확인한다.
/// MainView의 GPU·winit 이벤트 처리를 직접 실행하는 검사는 아니다.
#[cfg(test)]
mod cursor_moved_early_return_tests {
    use super::{MeshHoverTarget, cursor_moved_should_short_circuit, mesh_hover_transition};

    #[test]
    fn case_a_egui_consumed_short_circuits() {
        // Case A: mesh surface 에서 host UI chrome(사이드바 등)으로 넘어가는 전환
        // 이벤트 자체가 egui_consumed=true 다.
        assert!(cursor_moved_should_short_circuit(
            false, true, false, false, false, false
        ));
    }

    #[test]
    fn case_b_overlay_open_short_circuits() {
        // Case B: 설정창 등 오버레이가 열려 있는 동안은 좌표와 무관하게 항상 참.
        assert!(cursor_moved_should_short_circuit(
            false, false, true, false, false, false
        ));
    }

    #[test]
    fn popup_banner_modifier_hint_each_short_circuit() {
        assert!(cursor_moved_should_short_circuit(
            false, false, false, true, false, false
        ));
        assert!(cursor_moved_should_short_circuit(
            false, false, false, false, true, false
        ));
        assert!(cursor_moved_should_short_circuit(
            false, false, false, false, false, true
        ));
    }

    #[test]
    fn a_divider_drag_keeps_moves_even_when_egui_claims_them() {
        assert!(!cursor_moved_should_short_circuit(
            true, true, false, false, false, false
        ));
    }

    #[test]
    fn no_flag_set_does_not_short_circuit() {
        assert!(!cursor_moved_should_short_circuit(
            false, false, false, false, false, false
        ));
    }

    #[test]
    fn short_circuit_frame_transitions_hovered_mesh_target_to_none_with_pointer_gone() {
        // early-return 전에 hover를 해제하면 이전 대상에 PointerGone을 한 번 보낸다.
        assert!(cursor_moved_should_short_circuit(
            false, true, false, false, false, false
        ));
        let (next, gone) = mesh_hover_transition(
            Some(MeshHoverTarget::Local(7)),
            None, /* update_mesh_hover(None) */
        );
        assert_eq!(next, None);
        assert_eq!(gone, Some(MeshHoverTarget::Local(7)));
    }

    #[test]
    fn short_circuit_frame_is_idempotent_when_already_none() {
        // 이미 해제한 상태에서는 PointerGone을 중복 전송하지 않는다.
        let (next, gone) = mesh_hover_transition(None, None);
        assert_eq!(next, None);
        assert_eq!(gone, None);
    }
}

/// 실제 위젯에 입력을 양보할 조건을 확인한다. OS 리사이즈 동작 자체는 검사하지 않는다.
#[cfg(test)]
mod resize_gate_tests {
    use super::resize_should_yield_to_content;

    #[test]
    fn empty_chrome_yields_nothing_so_resize_wins() {
        // 타이틀바/상태바의 버튼 없는 빈 여백 — 모든 플래그가 false 여야
        // 리사이즈가 항상 이긴다 — 이 조합이 깨지면 빈 여백에서 창 가장자리를
        // 잡을 수 없다.
        assert!(!resize_should_yield_to_content(
            false, false, false, false, false
        ));
    }

    #[test]
    fn chrome_widget_hovered_yields_to_content() {
        // 타이틀바 창 버튼/Windows 캡션/상태바 클릭 요소 위 — egui_consumed 가 아니라
        // 이 위젯 단위 플래그로만 리사이즈를 양보해야 한다.
        assert!(resize_should_yield_to_content(
            true, false, false, false, false
        ));
    }

    #[test]
    fn sidebar_widget_hovered_also_yields_to_content() {
        // 사이드바도 같은 위젯 hover 플래그를 사용하므로 리사이즈보다 우선한다.
        assert!(resize_should_yield_to_content(
            true, false, false, false, false
        ));
    }

    #[test]
    fn overlay_popup_banner_size_locked_each_yield() {
        assert!(resize_should_yield_to_content(
            false, true, false, false, false
        ));
        assert!(resize_should_yield_to_content(
            false, false, true, false, false
        ));
        assert!(resize_should_yield_to_content(
            false, false, false, true, false
        ));
        assert!(resize_should_yield_to_content(
            false, false, false, false, true
        ));
    }
}

#[cfg(test)]
mod hover_motion_tests {
    use super::{HoverReportInput, MOUSE_BUTTON_NONE, mouse_report_cb, should_report_hover_motion};

    /// 모든 조건을 통과하는 기본 입력. 각 테스트에서 조건 하나씩 바꾼다.
    fn ok() -> HoverReportInput {
        HoverReportInput {
            window_focused: true,
            over_focused_surface: true,
            on_divider_band: false,
            resize_edge_active: false,
            dragging_divider: false,
        }
    }

    #[test]
    fn reports_when_every_guard_passes() {
        assert!(should_report_hover_motion(ok()));
    }

    /// 확정 정책: 비포커스 대상에는 어떤 hover 도 보내지 않는다 — 배경 TUI 로 마우스
    /// 입력이 새지 않게 한다(포커스 전환도 하지 않는다, docs/surfaces/terminal/index.md).
    #[test]
    fn never_reports_to_a_non_focused_target() {
        assert!(!should_report_hover_motion(HoverReportInput {
            over_focused_surface: false,
            ..ok()
        }));
        assert!(!should_report_hover_motion(HoverReportInput {
            window_focused: false,
            ..ok()
        }));
    }

    /// 입력 z-order 상 divider(순서 6) 가 surface 콘텐츠(순서 7) 보다 위다 — 밴드
    /// 안에서는 커서가 ↔/↕ 이므로 그 아래 TUI 가 hover 를 받으면 안 된다.
    #[test]
    fn boundary_bands_and_divider_drag_block_hover() {
        assert!(!should_report_hover_motion(HoverReportInput {
            on_divider_band: true,
            ..ok()
        }));
        assert!(!should_report_hover_motion(HoverReportInput {
            resize_edge_active: true,
            ..ok()
        }));
        assert!(!should_report_hover_motion(HoverReportInput {
            dragging_divider: true,
            ..ok()
        }));
    }

    /// 버튼 없음(3) + motion(32) = 35. 드래그(좌버튼 0)의 32 와 구분된다.
    #[test]
    fn hover_cb_is_thirty_five() {
        assert_eq!(
            mouse_report_cb(MOUSE_BUTTON_NONE, true, false, false, false),
            35
        );
        assert_eq!(mouse_report_cb(0, true, false, false, false), 32);
    }
}

/// 캡처 동작은 유지하면서 안내 배너만 숨기는 설정인지 확인한다.
/// busy 폴링이 마지막으로 관측한 전경 이름을 현재 설정과 대조하며, 이름을 모르면 숨기지 않는다.
fn mouse_capture_banner_suppressed(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    surface_id: u32,
) -> bool {
    engine.foreground_name(surface_id).is_some_and(|name| {
        engine
            .settings
            .general
            .mouse_capture_banner_disabled_for(name)
    })
}

#[cfg(test)]
mod mouse_capture_banner_tests {
    use super::mouse_capture_banner_suppressed;

    #[test]
    fn banner_is_suppressed_only_for_a_listed_foreground() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut e_session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine");
        let e = e_session.borrow_mut();
        e.runtime.settings.general.mouse_capture_banner_blacklist = vec!["vim".to_string()];
        assert!(!mouse_capture_banner_suppressed(&e.read(), 42));
        e.live.foreground_names.insert(42, "vim".to_string());
        e.live.foreground_names.insert(43, "htop".to_string());
        assert!(mouse_capture_banner_suppressed(&e.read(), 42));
        assert!(!mouse_capture_banner_suppressed(&e.read(), 43));
    }
}

#[cfg(test)]
mod drag_button_tests {
    use super::{mouse_report_cb, pop_report_button, push_report_button, report_button_code};
    use winit::event::MouseButton;

    /// motion 대상은 "가장 최근에 보고된 press" 다 — 버튼 코드와 surface 를 함께 싣는다.
    fn target(stack: &[(u8, u32)]) -> Option<(u8, u32)> {
        stack.last().copied()
    }

    #[test]
    fn most_recent_press_owns_the_motion_report() {
        let mut stack = Vec::new();
        push_report_button(&mut stack, 0, 7);
        assert_eq!(target(&stack), Some((0, 7)));
        // 좌버튼을 누른 채 우버튼을 추가로 누르면 motion 은 우버튼으로 나간다.
        push_report_button(&mut stack, 2, 7);
        assert_eq!(target(&stack), Some((2, 7)));
        // 우버튼만 떼면 아직 눌려 있는 좌버튼이 다시 motion 주인이 된다.
        pop_report_button(&mut stack, 2);
        assert_eq!(target(&stack), Some((0, 7)));
        pop_report_button(&mut stack, 0);
        assert_eq!(target(&stack), None);
    }

    /// 우클릭은 click-to-activate 를 타지 않아 비포커스 surface 에 press 가 갈 수 있다 —
    /// motion 도 그 surface 로 가야 press/motion/release 가 한 앱에서 짝을 이룬다.
    #[test]
    fn motion_follows_the_press_time_surface() {
        let mut stack = Vec::new();
        push_report_button(&mut stack, 2, 42);
        assert_eq!(target(&stack), Some((2, 42)));
    }

    /// 같은 버튼 재press 는 중복 쌓지 않고 최신 surface 로 갱신된다.
    #[test]
    fn repeated_press_does_not_stack_up() {
        let mut stack = Vec::new();
        push_report_button(&mut stack, 1, 3);
        push_report_button(&mut stack, 1, 5);
        assert_eq!(stack.len(), 1);
        assert_eq!(target(&stack), Some((1, 5)));
    }

    #[test]
    fn winit_buttons_map_to_report_codes() {
        assert_eq!(report_button_code(MouseButton::Left), Some(0));
        assert_eq!(report_button_code(MouseButton::Middle), Some(1));
        assert_eq!(report_button_code(MouseButton::Right), Some(2));
        assert_eq!(report_button_code(MouseButton::Back), None);
    }

    /// 우 34(`2|32`) / 미들 33(`1|32`) / 좌 32 — 좌버튼 값은 기존과 같아야 한다(회귀).
    #[test]
    fn drag_motion_cb_carries_the_button_bits() {
        assert_eq!(mouse_report_cb(2, true, false, false, false), 34);
        assert_eq!(mouse_report_cb(1, true, false, false, false), 33);
        assert_eq!(mouse_report_cb(0, true, false, false, false), 32);
    }
}
