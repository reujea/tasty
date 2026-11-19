//! 팝업 렌더링과 입력·표시 범위 계산.

use crate::adapters::ui::LayoutContext;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::elide_popup_title as elide_for_width;

use super::occlusion::{Occluder, PointOwnership, point_ownership};
use super::route::RoutedPress;
use super::{
    PopupCloseReason, PopupCx, PopupDrawResult, PopupId, PopupInstanceId, PopupManager, PopupScope,
    PopupShadow, PopupShell, ResizeEdges,
};

/// 리사이즈 테두리 밴드 폭(px). popup_rect 가장자리 안쪽 이 폭 안에서 누르면 리사이즈.
const RESIZE_BAND: LogicalPx = LogicalPx(6.0);

/// 셸 토큰 묶음의 색.
pub(super) struct ShellColors {
    pub(super) bg: egui::Color32,
    pub(super) border: egui::Color32,
    pub(super) title_edge: egui::Color32,
}

/// 선언한 셸 묶음의 배경·테두리·제목줄 아래 선 색. 팝업마다 이 묶음을 고른 이유는 정의(`defs.rs`)에 적는다.
pub(super) fn shell_colors(
    shell: PopupShell,
    th: &tasty_type_appearance::theme::Theme,
) -> ShellColors {
    let (bg, border, title_edge): (egui::Color32, egui::Color32, egui::Color32) = match shell {
        PopupShell::Raised => (
            th.surface_raised().into(),
            th.border_frame().into(),
            th.border_frame().into(),
        ),
        PopupShell::Panel => (
            th.bg_panel().into(),
            th.border_frame().into(),
            th.border_frame().into(),
        ),
        PopupShell::InfoPanel => (
            th.bg_panel().into(),
            th.border_frame().into(),
            th.info_modal_title_edge().into(),
        ),
        PopupShell::Dialog => (
            th.dialog_bg().into(),
            th.border_frame().into(),
            th.border_frame().into(),
        ),
        PopupShell::Menu => (
            th.surface_raised().into(),
            th.menu_border().into(),
            th.border_frame().into(),
        ),
        PopupShell::BannerMenu => (
            th.surface_raised().into(),
            th.banner_more_menu_border().into(),
            th.border_frame().into(),
        ),
        PopupShell::SearchBar => (
            th.surface_raised().into(),
            th.search_bar_border().into(),
            th.border_frame().into(),
        ),
        // 콘텐츠가 셸을 그린다. 관리자는 아무 색도 칠하지 않는다.
        PopupShell::Content => (
            egui::Color32::TRANSPARENT,
            egui::Color32::TRANSPARENT,
            egui::Color32::TRANSPARENT,
        ),
    };
    ShellColors {
        bg,
        border,
        title_edge,
    }
}

/// 선언한 그림자 토큰. scrim 여부와는 별개인 분류다(docs/architecture/ui-widgets-crate.md).
pub(super) fn shadow_token(
    shadow: PopupShadow,
    th: &tasty_type_appearance::theme::Theme,
) -> tasty_type_appearance::theme::ShadowToken {
    match shadow {
        PopupShadow::Modal => th.shadow_modal(),
        PopupShadow::Popover => th.shadow_popover(),
    }
}

/// 타이틀바 버튼 하나를 IconButton sm 규칙(호버·누름 배경, 글리프 색)으로 그린다.
/// hit-test는 매니저가 하므로 판정 결과만 받는다.
fn title_icon_button(
    ctx: &egui::Context,
    painter: &egui::Painter,
    th: &tasty_type_appearance::theme::Theme,
    rect: egui::Rect,
    icon: crate::adapters::ui::icons::Icon,
    hovered: bool,
    primary_down: bool,
) {
    let tint = tasty_ui_widgets::paint_icon_button_state(
        painter,
        th,
        rect,
        tasty_ui_widgets::IconButtonState::ghost(hovered, hovered && primary_down),
    );
    let glyph = tasty_ui_widgets::ControlSize::Sm.icon_glyph(th);
    let glyph_rect = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(glyph));
    if let Err(err) =
        tasty_ui_widgets::paint_popup_title_glyph(ctx, painter, icon, glyph_rect, tint)
    {
        tracing::warn!("popup title button glyph failed to load: {err}");
    }
}

/// 포인터가 있는 테두리를 반환한다. 계산은 egui 좌표를 사용한다.
pub(super) fn resize_edges_at(rect: egui::Rect, pos: egui::Pos2, band: f32) -> Option<ResizeEdges> {
    let left = pos.x <= rect.min.x + band;
    let right = pos.x >= rect.max.x - band;
    let top = pos.y <= rect.min.y + band;
    let bottom = pos.y >= rect.max.y - band;
    if left || right || top || bottom {
        Some(ResizeEdges {
            left,
            right,
            top,
            bottom,
        })
    } else {
        None
    }
}

/// 잡은 엣지 조합 → 리사이즈 커서. 모서리는 대각선, 단일 엣지는 수평/수직.
fn resize_cursor(e: ResizeEdges) -> egui::CursorIcon {
    use egui::CursorIcon as C;
    match (e.left, e.right, e.top, e.bottom) {
        (true, _, true, _) => C::ResizeNwSe, // top-left
        (_, true, _, true) => C::ResizeNwSe, // bottom-right
        (_, true, true, _) => C::ResizeNeSw, // top-right
        (true, _, _, true) => C::ResizeNeSw, // bottom-left
        (true, _, _, _) | (_, true, _, _) => C::ResizeHorizontal,
        (_, _, true, _) | (_, _, _, true) => C::ResizeVertical,
        _ => C::Default,
    }
}

impl PopupManager {
    /// Draw all open popups. The `content_fn` callback is invoked for each popup with its id.
    /// `draw_ctx` provides scope context for visibility and boundary clamping.
    /// Returns draw result including closed popup IDs and hover state for input layer.
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: egui 즉시모드 draw — 열린 popup별 content_fn 콜백 + 경계 clamp, 클로저 중첩이 구조적
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        content_fn: &mut dyn FnMut(&mut PopupCx<'_>, &mut egui::Ui),
        draw_ctx: Option<&LayoutContext>,
        plugin_occluders: &[Occluder],
    ) -> PopupDrawResult {
        let press = RoutedPress::from_frame_at_press(ctx, draw_ctx);
        self.draw_routed(ctx, content_fn, draw_ctx, plugin_occluders, press.as_ref())
    }

    /// [`Self::draw`]와 같고, 이번 프레임의 누름을 호출자가 정한다(메인 창은 winit 시점의 포커스를 담는다).
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: egui 즉시모드 draw — 열린 popup별 content_fn 콜백 + 경계 clamp, 클로저 중첩이 구조적
    pub(crate) fn draw_routed(
        &mut self,
        ctx: &egui::Context,
        content_fn: &mut dyn FnMut(&mut PopupCx<'_>, &mut egui::Ui),
        draw_ctx: Option<&LayoutContext>,
        plugin_occluders: &[Occluder],
        press: Option<&RoutedPress>,
    ) -> PopupDrawResult {
        let th = theme::theme();
        let screen_rect = ctx.screen_rect();
        let mut closed: Vec<(PopupInstanceId, PopupCloseReason)> = Vec::new();
        let mut bring_front: Option<PopupInstanceId> = None;
        let mut fullscreen_requested: Option<crate::adapters::ui::fullscreen::StageId> = None;
        let mut layers: Vec<egui::LayerId> = Vec::new();
        // 포커스를 받는 팝업의 레이어. 호출자가 plugin 팝업과의 순서를 정할 때 쓴다.
        let mut focus_layers: Vec<egui::LayerId> = Vec::new();
        let mut overlay_targets: Vec<crate::state::webview_overlay::OverlayTarget> = Vec::new();

        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let primary_pressed = press.is_some_and(RoutedPress::is_primary);
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let primary_released = ctx.input(|i| i.pointer.primary_released());

        // 그리기 함수가 보는 포커스는 이번 프레임의 클릭을 반영하기 전 값이다.
        let focused_at_start: Vec<PopupInstanceId> = self
            .popups
            .iter()
            .filter(|p| p.open && p.scope_visible && p.focused)
            .map(|p| p.instance)
            .collect();

        // Hidden scopes retain focus intent, but cannot hold the keyboard gate or
        // continue a pointer gesture started before the scope disappeared.
        for popup in &mut self.popups {
            popup.scope_visible = Self::is_scope_visible(&popup.scope, draw_ctx);
            if !popup.scope_visible {
                popup.dragging = false;
                popup.resizing = None;
            }
        }

        let mut open_indices: Vec<usize> = self
            .popups
            .iter()
            .enumerate()
            .filter(|(_, p)| p.open && Self::is_scope_visible(&p.scope, draw_ctx))
            .map(|(i, _)| i)
            .collect();
        // 포커스를 받지 않는 팝업은 다른 팝업 아래에 그린다. 그 안의 순서는 z 순서 그대로다.
        open_indices.sort_by_key(|&i| self.popups[i].arrange.takes_focus);
        self.judge_display(ctx, &open_indices, draw_ctx);
        let column_rect = self.place_column(&open_indices);
        let mut released: Vec<(PopupInstanceId, egui::Rect)> = Vec::new();

        // 이번 프레임의 영역을 뒤에 그릴 plugin 팝업에 전달한다.
        // 호출 순서는 egui_bridge::run_egui_frame과 source_guards::frame_draw_order에서 확인한다.
        let hit_rects: Vec<Occluder> = open_indices
            .iter()
            .filter_map(|&i| {
                self.popups[i].box_rect().map(|rect| Occluder {
                    rect,
                    z_seq: self.popups[i].z_seq,
                })
            })
            .collect();

        // 입력 우선순위: 닫기·전체화면 버튼 > 리사이즈 테두리 > 이동 손잡이 > 콘텐츠.
        let mut hovered_popup: Option<PopupInstanceId> = None;
        let mut hovered_handle: Option<PopupInstanceId> = None;
        let mut hovered_close: Option<PopupInstanceId> = None;
        let mut hovered_collapse: Option<PopupInstanceId> = None;
        let mut hovered_fullscreen: Option<(
            PopupInstanceId,
            crate::adapters::ui::fullscreen::StageId,
        )> = None;
        let mut hovered_resize: Option<(PopupInstanceId, ResizeEdges)> = None;
        let mut grip_pressed: Option<PopupInstanceId> = None;
        if let Some(pos) = pointer_pos {
            for &idx in open_indices.iter().rev() {
                let popup = &self.popups[idx];
                let Some(rect) = popup.box_rect() else {
                    continue;
                };
                // 위의 plugin 팝업이 덮으면 건너뛴다. 아래 host 팝업의 다른 영역도 검사해야 하므로 continue한다.
                if matches!(
                    point_ownership(rect, popup.z_seq, plugin_occluders, pos),
                    PointOwnership::OccludedByHigher
                ) {
                    continue;
                }
                if rect.contains(pos) {
                    hovered_popup = Some(popup.instance);
                    if popup.collapsed() {
                        // 접힌 프레임은 보기 버튼 하나뿐이다. 버튼은 그 Area의 egui 위젯이 받는다.
                    } else if !popup.headless && popup.close_btn_rect().contains(pos) {
                        hovered_close = Some(popup.instance);
                    } else if popup.collapse_btn_rect().is_some_and(|r| r.contains(pos)) {
                        hovered_collapse = Some(popup.instance);
                    } else if let Some((fs_rect, stage)) =
                        popup.fullscreen_btn_rect().zip(popup.fullscreen_stage)
                        && fs_rect.contains(pos)
                    {
                        hovered_fullscreen = Some((popup.instance, stage));
                    } else if popup.resizable
                        && let Some(edges) = popup.resize_edges_at(pos, RESIZE_BAND)
                    {
                        hovered_resize = Some((popup.instance, edges));
                    } else if let Some(handle) = popup.effective_drag_handle_rect(ctx)
                        && handle.contains(pos)
                    {
                        hovered_handle = Some(popup.instance);
                    }
                    break; // topmost popup wins
                } else if super::child_overlay_hit(ctx, popup.id, pos) {
                    // 드롭다운은 egui 별도 Area라 팝업 clip을 상속하지 않고 범위 밖에도 그려진다. 좌표 경로와
                    // 무관하게 그 팝업 내용의 하위 노드이므로 팝업 안 누름으로 취급한다.
                    hovered_popup = Some(popup.instance);
                    break;
                } else if popup
                    .cover_rect(draw_ctx, screen_rect)
                    .is_some_and(|r| r.contains(pos))
                {
                    // 덮는 팝업 아래로는 내려가지 않는다. 그 자리는 아래 팝업 것이 아니다.
                    break;
                }
            }
        }

        // 상자(와 그 드롭다운) 위 포인터. 아래 덮는 범위 판정과 구분해 누름 처리에 쓴다.
        let hovered_box = hovered_popup;
        // 덮는 팝업의 배치 범위 위 포인터는 그 팝업 것이다(상자 밖이면 scrim). 아래 노드로 내려가지 않는다.
        if hovered_popup.is_none()
            && let Some(pos) = pointer_pos
        {
            hovered_popup = self
                .cover_at(pos, &open_indices, draw_ctx, screen_rect)
                .map(|(iid, _)| iid);
        }

        // 닫기·포커스·맨 앞으로 올리기를 먼저 처리한다. 이동·리사이즈 시작은
        // 콘텐츠를 그린 뒤 위젯이 포인터를 사용했는지 확인하고 결정한다.
        if let Some(press) = press {
            if let Some(iid) = hovered_close.filter(|_| press.is_primary()) {
                closed.push((iid, PopupCloseReason::CloseButton));
            } else if let Some(iid) = hovered_collapse.filter(|_| press.is_primary()) {
                self.refold(iid);
            } else if let Some((_, stage)) = hovered_fullscreen.filter(|_| press.is_primary()) {
                // 무대는 별도 콘텐츠이므로 원본 팝업은 닫지 않는다.
                fullscreen_requested = Some(stage);
            } else if let Some(iid) = hovered_box {
                // 팝업 안 누름(어느 버튼이든 포커스 행위). 포커스를 받지 않는 팝업은 누름으로 포커스와
                // 순서를 바꾸지 않는다(give_focus·bring_to_front가 거른다).
                bring_front = Some(iid);
                self.give_focus(iid, Some(&press.path));
            } else if let Some(iid) = self.route_outside_press(
                press,
                &open_indices,
                draw_ctx,
                screen_rect,
                plugin_occluders,
                &mut closed,
            ) {
                bring_front = Some(iid);
                self.give_focus(iid, Some(&press.path));
            }
        }

        for popup in &mut self.popups {
            if !popup.dragging {
                continue;
            }
            if primary_released {
                popup.dragging = false;
                released.push((popup.instance, popup.popup_rect()));
            } else if primary_down && let Some(pos) = pointer_pos {
                let bounds =
                    Self::scope_rect(&popup.placement_scope(), draw_ctx).unwrap_or(screen_rect);
                let new_pos = pos - popup.drag_offset;
                popup.pos = egui::pos2(
                    new_pos.x.clamp(
                        bounds.min.x,
                        (bounds.max.x - popup.size.x).max(bounds.min.x),
                    ),
                    new_pos.y.clamp(
                        bounds.min.y,
                        (bounds.max.y - popup.size.y).max(bounds.min.y),
                    ),
                );
            }
        }

        // 잡은 테두리만 이동하고 최소 크기·범위 안으로 제한한다. 사용자가 바꾼 크기는 sizer로 덮지 않는다.
        for popup in &mut self.popups {
            let Some(edges) = popup.resizing else {
                continue;
            };
            if primary_released {
                popup.resizing = None;
                released.push((popup.instance, popup.popup_rect()));
                continue;
            }
            if primary_down && let Some(pos) = pointer_pos {
                let start = popup.resize_start_rect;
                let mut min = start.min;
                let mut max = start.max;
                if edges.left {
                    min.x = pos.x;
                }
                if edges.right {
                    max.x = pos.x;
                }
                if edges.top {
                    min.y = pos.y;
                }
                if edges.bottom {
                    max.y = pos.y;
                }
                let mw = popup.min_size.x;
                let mh = popup.min_size.y;
                if edges.left {
                    min.x = min.x.min(max.x - mw);
                }
                if edges.right {
                    max.x = max.x.max(min.x + mw);
                }
                if edges.top {
                    min.y = min.y.min(max.y - mh);
                }
                if edges.bottom {
                    max.y = max.y.max(min.y + mh);
                }
                let bounds =
                    Self::scope_rect(&popup.placement_scope(), draw_ctx).unwrap_or(screen_rect);
                let new_rect = egui::Rect::from_min_max(min, max).intersect(bounds);
                popup.pos = new_rect.min;
                popup.set_needed_size(new_rect.size());
                popup.size_user_overridden = true;
            }
        }

        for popup in &mut self.popups {
            if popup.request_center && popup.open {
                let center_rect =
                    Self::scope_rect(&popup.placement_scope(), draw_ctx).unwrap_or(screen_rect);
                popup.pos = egui::pos2(
                    center_rect.center().x - popup.size.x / 2.0,
                    center_rect.center().y - popup.size.y / 2.0,
                );
                popup.request_center = false;
            }
        }

        for popup in &mut self.popups {
            if popup.request_top && popup.open {
                let anchor_rect =
                    Self::scope_rect(&popup.placement_scope(), draw_ctx).unwrap_or(screen_rect);
                popup.pos = egui::pos2(
                    anchor_rect.center().x - popup.size.x / 2.0,
                    anchor_rect.min.y + th.spacing_sm.value(),
                );
                popup.request_top = false;
            }
        }

        // Set cursor. 진행 중인 리사이즈/드래그가 우선(포인터가 밴드 밖으로 나가도 유지),
        // 그 다음 hover 상태.
        let active_resize = self
            .popups
            .iter()
            .find_map(|p| if p.dragging { None } else { p.resizing });
        let active_drag = self.popups.iter().any(|p| p.dragging);
        if let Some(edges) = active_resize {
            ctx.set_cursor_icon(resize_cursor(edges));
        } else if active_drag {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if let Some((_, edges)) = hovered_resize {
            ctx.set_cursor_icon(resize_cursor(edges));
        } else if hovered_handle.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        } else if hovered_close.is_some()
            || hovered_collapse.is_some()
            || hovered_fullscreen.is_some()
        {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        } else if hovered_popup.is_some() {
            // Content area: set default cursor (arrow) to override terminal cursor
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }

        // 상위 팝업의 넓은 scrim이 하위 scrim을 덮을 수 있어 렌더링 전에 범위를 고른다.
        let scrim_candidates: Vec<(usize, egui::Rect)> = open_indices
            .iter()
            .filter(|&&i| !closed.iter().any(|(c, _)| *c == self.popups[i].instance))
            .filter(|&&i| self.popups[i].covers())
            .map(|&i| {
                (
                    i,
                    Self::scope_rect(&self.popups[i].placement_scope(), draw_ctx)
                        .unwrap_or(screen_rect),
                )
            })
            .collect();
        let scrim_rects: Vec<egui::Rect> = scrim_candidates.iter().map(|(_, r)| *r).collect();
        let scrims: Vec<(usize, egui::Rect)> = Self::pick_scrim_layers(&scrim_rects)
            .into_iter()
            .zip(scrim_candidates)
            .filter_map(|(paints, entry)| paints.then_some(entry))
            .collect();

        // 불투명 모드 WebView 페이지(직전 동기화 값). 비어 있으면 모든 팝업이 원래 모양이다.
        let pages = tasty_egui_theme::overlay_edge::opaque_pages(ctx);
        // 키 주인(맨 위 포커스 인스턴스). 포커스 가장자리와 접힌 버튼의 Enter·Space가 이 하나에만 간다.
        let key_owner = self.focused_dismissal_target().map(|(_, iid, _)| iid);
        let mut expand_requested: Option<PopupInstanceId> = None;
        for &popup_idx in &open_indices {
            let popup = &mut self.popups[popup_idx];
            if closed.iter().any(|(c, _)| *c == popup.instance) {
                continue;
            }

            let placement = popup.placement_scope();
            let scope_clip = Self::scope_rect(&placement, draw_ctx).unwrap_or(screen_rect);
            if popup.collapsed() {
                // 접힌 팝업은 버튼 프레임만 그린다. 위치·크기는 다시 펼칠 때를 위해 그대로 둔다.
                if let Some(frame) = popup.collapsed_frame() {
                    let layer_id = egui::LayerId::new(
                        egui::Order::Foreground,
                        area_id_of(popup.id, popup.instance),
                    );
                    if let Some(cover) = popup.cover_rect(draw_ctx, screen_rect) {
                        let block = super::press::block_pointer(
                            ctx,
                            layer_id.id.with("cover_block"),
                            cover,
                        );
                        layers.push(block);
                        focus_layers.push(block);
                    }
                    layers.push(layer_id);
                    focus_layers.push(layer_id);
                    let holds_keys = key_owner == Some(popup.instance);
                    let look = super::display::CollapsedDraw {
                        scrim: scrims
                            .iter()
                            .find(|(i, _)| *i == popup_idx)
                            .map(|(_, r)| *r),
                        on_scrim: popup.look.covers_scope,
                        focused: holds_keys,
                    };
                    if super::display::draw_collapsed(ctx, &th, layer_id, frame, scope_clip, look) {
                        expand_requested = Some(popup.instance);
                    }
                }
                continue;
            }
            let clamp_rect =
                Self::scope_bounds(&placement, draw_ctx, screen_rect, th.spacing_sm.value());
            popup.clamp_to_screen(clamp_rect);

            let popup_id = popup.id;
            let popup_iid = popup.instance;
            let is_headless = popup.headless;
            let popup_rect = popup.popup_rect();
            let content_rect = popup.content_rect();
            // 그립 자리는 콘텐츠 위에 덧대는 위젯으로 잡는다. 스크롤바처럼 그 밑에 있는 위젯이 누름을
            // 가져가면 크기 조절이 시작되지 않는다.
            let grip_rect = match popup.arrange.resize_handle {
                super::PopupResizeHandle::Grip { size } if popup.resizable => {
                    Some(egui::Rect::from_min_max(
                        popup_rect.max - egui::Vec2::splat(size.value()),
                        popup_rect.max,
                    ))
                }
                _ => None,
            };

            // 레이어·Area ID는 인스턴스로 정한다. 순서가 바뀌어도 스크롤·커서 같은 egui 상태가 따라간다.
            let layer_id =
                egui::LayerId::new(egui::Order::Foreground, area_id_of(popup_id, popup_iid));
            // 덮는 범위의 아래 위젯 입력은 팝업 바로 아래 막는 레이어가 받는다.
            if let Some(cover) = popup.cover_rect(draw_ctx, screen_rect) {
                let block =
                    super::press::block_pointer(ctx, layer_id.id.with("cover_block"), cover);
                layers.push(block);
                if popup.arrange.takes_focus {
                    focus_layers.push(block);
                }
            }
            layers.push(layer_id);
            if popup.arrange.takes_focus {
                focus_layers.push(layer_id);
            }

            // 그림자가 이웃 영역까지 어둡게 하지 않도록 팝업 소속 범위로 자른다.
            let painter = ctx.layer_painter(layer_id).with_clip_rect(scope_clip);

            // 페이지와 겹친 상자는 전체가 각진 모서리·그림자 없음이다. passive 팝업은 겹친 페이지를 숨기거나
            // (포커스를 받지 않는 팝업이면) 페이지에 구멍을 내므로 페이지 위에 그리지 않는다.
            let edge = if popup.passive {
                tasty_egui_theme::overlay_edge::BoxEdge::NORMAL
            } else {
                tasty_egui_theme::overlay_edge::edge_for(popup_rect, false, &pages)
            };
            let radius = edge.radius(&th, th.corner_radius).value();

            // scrim은 해당 팝업의 소속 범위에만, 셸보다 먼저 그린다. 범위 안에 불투명 모드 페이지가
            // 보이면 페이지만 밝게 남는 얼룩을 피하려고 그리지 않는다.
            let scrim = scrims
                .iter()
                .find(|(i, _)| *i == popup_idx)
                .map(|(_, r)| *r)
                .filter(|r| !tasty_egui_theme::overlay_edge::scrim_skipped(*r, &pages));
            if let Some(scrim_rect) = scrim {
                painter.rect_filled(scrim_rect, 0.0, th.scrim().to_egui());
            }

            let shell = shell_colors(popup.look.shell, &th);
            let bg_fill = shell.bg;
            let shell_by_content = popup.look.shell == PopupShell::Content;
            // 그림자는 scrim 위, 셸 아래에 그린다.
            let shadow = edge.shadow(&th, shadow_token(popup.look.shadow, &th));
            if !shell_by_content {
                painter.add(shadow.to_egui().as_shape(popup_rect, radius));
            }
            // 포커스를 받지 않는 팝업은 WebView에 구멍을 내서 보이므로 합성 대상이 아니다.
            if !popup.passive && popup.arrange.takes_focus {
                overlay_targets.push(crate::state::webview_overlay::OverlayTarget {
                    kind: crate::state::webview_overlay::TargetKind::HostPopup,
                    layer: layer_id,
                    rect: popup_rect,
                    opaque_rect: crate::state::webview_overlay::outlined_box(
                        popup_rect,
                        th.border_width,
                        scope_clip,
                        ctx.pixels_per_point(),
                    ),
                    paint_rect: (popup_rect + shadow.to_egui().margin()).intersect(scope_clip),
                    scrim,
                });
            }
            if !shell_by_content {
                painter.rect_filled(popup_rect, radius, bg_fill);
                painter.rect_stroke(
                    popup_rect,
                    radius,
                    egui::Stroke::new(th.border_width.value(), shell.border),
                    egui::StrokeKind::Outside,
                );
            }

            if popup.has_title_bar() {
                let title_rect = popup.title_rect();
                let close_btn_rect = popup.close_btn_rect();
                let fullscreen_btn_rect = popup.fullscreen_btn_rect();

                let cr = radius as u8;
                painter.rect_filled(
                    title_rect,
                    egui::CornerRadius {
                        nw: cr,
                        ne: cr,
                        sw: 0,
                        se: 0,
                    },
                    th.bg_sidebar(),
                );
                let title_edge = shell.title_edge;
                painter.line_segment(
                    [
                        egui::pos2(title_rect.min.x, title_rect.max.y),
                        egui::pos2(title_rect.max.x, title_rect.max.y),
                    ],
                    egui::Stroke::new(th.border_width.value(), title_edge),
                );

                // 버튼 수와 관계없이 양쪽에 같은 폭을 비워 제목을 스트립 가운데에 둔다.
                let title_font = tasty_ui_widgets::popup_title_font(&th);
                let title_avail_rect = popup.title_text_rect();
                // headless 팝업이 펼쳐 받은 host 제목줄에는 제목이 없다.
                let shown_title = if is_headless {
                    ""
                } else {
                    popup.title.as_str()
                };
                let elided_title = elide_for_width(
                    ctx,
                    shown_title,
                    title_font.clone(),
                    title_avail_rect.width(),
                );
                painter.with_clip_rect(title_avail_rect).text(
                    title_avail_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &elided_title,
                    title_font,
                    th.text_primary().into(),
                );
                // 잘린 제목만 호버 시 전체 제목을 보여 준다. 끌어 옮기는 중에는 띄우지 않는다.
                if elided_title != shown_title {
                    let tip_id = egui::Id::new("popup.title_tooltip").with(popup_id);
                    let band_hovered = hovered_popup == Some(popup_iid)
                        && !primary_down
                        && pointer_pos.is_some_and(|p| title_avail_rect.contains(p));
                    if tasty_ui_widgets::tooltip_hover_delay_elapsed(ctx, &th, tip_id, band_hovered)
                    {
                        tasty_ui_widgets::show_popup_title_tooltip(
                            ctx,
                            &th,
                            tip_id,
                            &popup.title,
                            title_avail_rect,
                        );
                    }
                }

                if let Some(rect) = fullscreen_btn_rect {
                    let hovered = matches!(hovered_fullscreen, Some((iid, _)) if iid == popup_iid);
                    title_icon_button(
                        ctx,
                        &painter,
                        &th,
                        rect,
                        crate::adapters::ui::icons::FIT,
                        hovered,
                        primary_down,
                    );
                    if hovered {
                        // painter로 그린 버튼에는 Response가 없어 툴팁을 직접 표시한다.
                        egui::show_tooltip_at(
                            ctx,
                            layer_id,
                            egui::Id::new("popup.fullscreen_tooltip").with(popup_id),
                            rect.left_bottom(),
                            |ui| ui.label(crate::i18n::t("popup.fullscreen_button.tooltip")),
                        );
                    }
                }

                if let Some(rect) = popup.collapse_btn_rect() {
                    let hovered = hovered_collapse == Some(popup_iid);
                    title_icon_button(
                        ctx,
                        &painter,
                        &th,
                        rect,
                        crate::adapters::ui::icons::SHRINK,
                        hovered,
                        primary_down,
                    );
                    if hovered {
                        egui::show_tooltip_at(
                            ctx,
                            layer_id,
                            egui::Id::new("popup.collapse_tooltip").with(popup_iid.0),
                            rect.left_bottom(),
                            |ui| ui.label(crate::i18n::t("popup.collapse_button.tooltip")),
                        );
                    }
                }
                if !is_headless {
                    title_icon_button(
                        ctx,
                        &painter,
                        &th,
                        close_btn_rect,
                        crate::adapters::ui::icons::CLOSE,
                        hovered_close == Some(popup_iid),
                        primary_down,
                    );
                }
            }
            // 키 주인인 좁은 범위 팝업은 셸 위쪽에 포커스 가장자리를 켠다. 창 범위(펼친 팝업 포함)는 켜지 않는다.
            if !shell_by_content && key_owner == Some(popup_iid) && placement != PopupScope::Window
            {
                tasty_ui_widgets::paint_popup_focus_edge(&painter, &th, popup_rect);
            }

            // egui가 스크롤·호버 영역으로 인식하도록 같은 layer ID의 Area에 콘텐츠를 그린다.
            // 이동은 이 매니저가 처리하고 내부 위젯에는 클릭·드래그를 남긴다.
            {
                egui::Area::new(layer_id.id)
                    .order(egui::Order::Foreground)
                    .fixed_pos(content_rect.min)
                    .movable(false)
                    .interactable(true)
                    .sense(egui::Sense::hover())
                    .constrain(false)
                    .show(ctx, |ui| {
                        if bg_fill == egui::Color32::from(th.surface_raised()) {
                            tasty_egui_theme::apply_raised_scrollbar(&th, ui.visuals_mut());
                        }
                        // 푸터와 빈 공간도 포인터 영역에 포함한다.
                        ui.set_min_size(content_rect.size());
                        ui.set_max_size(content_rect.size());
                        // 콘텐츠가 팝업 밖으로 넘치지 않게 자른다.
                        ui.set_clip_rect(content_rect);
                        // 이 팝업 안에서 연 드롭다운은 셸이 불투명이면 함께 불투명이다.
                        tasty_egui_theme::overlay_edge::with_opaque_parent(
                            ctx,
                            edge.opaque,
                            || {
                                let mut cx = PopupCx {
                                    id: popup.id,
                                    instance: popup.instance,
                                    focused: focused_at_start.contains(&popup.instance),
                                    payload: &mut popup.payload,
                                };
                                content_fn(&mut cx, ui)
                            },
                        );
                        if let Some(grip) = grip_rect {
                            let grip_id = egui::Id::new("popup.resize_grip").with(popup_iid);
                            if ui
                                .interact(grip, grip_id, egui::Sense::drag())
                                .is_pointer_button_down_on()
                            {
                                grip_pressed = Some(popup_iid);
                            }
                        }
                    });
            }
        }

        // 콘텐츠 위젯이 포인터를 사용하지 않았을 때만 리사이즈·이동을 시작한다. 그립은 이 매니저의
        // 위젯이 누름을 가져가므로 예외다.
        // 수동 드래그는 egui 위젯이 아니므로 is_using_pointer를 스스로 설정하지 않는다.
        let grip_owns_press =
            grip_pressed.is_some_and(|g| hovered_resize.is_some_and(|(iid, _)| iid == g));
        if primary_pressed && (!ctx.is_using_pointer() || grip_owns_press) {
            if let Some((iid, edges)) = hovered_resize {
                if let Some(popup) = self.instance_mut(iid) {
                    popup.resizing = Some(edges);
                    popup.resize_start_rect = popup.popup_rect();
                }
            } else if let Some(iid) = hovered_handle
                && let Some(popup) = self.instance_mut(iid)
            {
                popup.dragging = true;
                if let Some(pos) = pointer_pos {
                    popup.drag_offset = pos - popup.pos;
                }
            }
        }

        // 닫는 인스턴스를 지우면 색인이 바뀌므로 닫기 전에 모은다.
        let still_open: Vec<usize> = open_indices
            .iter()
            .copied()
            .filter(|&i| !closed.iter().any(|(c, _)| *c == self.popups[i].instance))
            .collect();
        let covers = self.cover_occluders(&still_open, draw_ctx, screen_rect);
        restack_layers(ctx, &layers);
        // egui는 누른 영역을 맨 위로 올린다. 포커스를 받지 않는 팝업을 눌렀으면 모든 팝업 레이어를 함께
        // 올려 지금 순서(그 팝업이 아래)를 지킨다. 함께 올린 레이어끼리는 egui가 기존 순서를 유지한다.
        // 덮는 범위를 막는 레이어를 누른 경우도 같다. 막는 레이어가 팝업 위로 올라가지 않는다.
        let pressed_cover = hovered_box.is_none() && hovered_popup.is_some();
        if press.is_some()
            && (pressed_cover || hovered_popup.is_some_and(|iid| self.never_focused(iid)))
        {
            for layer in &layers {
                ctx.move_to_top(*layer);
            }
        }

        let mut closed_ids = Vec::with_capacity(closed.len());
        for (iid, reason) in closed {
            if let Some(p) = self.instance(iid) {
                closed_ids.push(p.id);
            }
            self.close_instance(iid, reason);
        }

        if let Some(iid) = bring_front {
            self.bring_to_front(iid);
        }
        if let Some(iid) = expand_requested {
            self.expand(iid);
        }

        PopupDrawResult {
            closed: closed_ids,
            covers,
            hovered: hovered_popup.is_some(),
            layers: focus_layers,
            fullscreen_requested,
            hit_rects,
            overlay_targets,
            column_rect,
            released,
        }
    }

    /// 현재 그릴 팝업 중 z가 가장 높은 것을 찾는다. Escape 처리 대상을 정할 때 쓴다.
    pub fn topmost_visible_open(
        &self,
        draw_ctx: Option<&LayoutContext>,
    ) -> Option<(PopupId, PopupInstanceId, u64)> {
        self.popups
            .iter()
            .filter(|p| {
                p.open && p.arrange.takes_focus && Self::is_scope_visible(&p.scope, draw_ctx)
            })
            .max_by_key(|p| p.z_seq)
            .map(|p| (p.id, p.instance, p.z_seq))
    }

    pub(crate) fn is_scope_visible(scope: &PopupScope, ctx: Option<&LayoutContext>) -> bool {
        let Some(ctx) = ctx else { return true };
        match scope {
            PopupScope::Window => true,
            PopupScope::Workspace(ws_id) => *ws_id == ctx.active_workspace_id,
            PopupScope::Pane(pane_id) => ctx.pane_rects.iter().any(|(id, _)| *id == *pane_id),
            PopupScope::Tab(tab_id) => ctx.active_tab_ids.iter().any(|(_, tid)| *tid == *tab_id),
            PopupScope::Surface(surface_id) => {
                ctx.surface_rects.iter().any(|(id, _)| *id == *surface_id)
            }
        }
    }

    /// Get the bounding rect for a popup's scope.
    pub(crate) fn scope_rect(
        scope: &PopupScope,
        ctx: Option<&LayoutContext>,
    ) -> Option<egui::Rect> {
        let ctx = ctx?;
        match scope {
            // 창 범위는 호출자가 화면 사각형을 쓴다.
            PopupScope::Window => None,
            // 활성 workspace 의 콘텐츠 영역(탭 바와 pane). 사이드바·상태바는 범위 밖이다.
            PopupScope::Workspace(ws_id) => ctx
                .workspace_rect
                .filter(|_| *ws_id == ctx.active_workspace_id),
            PopupScope::Pane(pane_id) => ctx
                .pane_rects
                .iter()
                .find(|(id, _)| *id == *pane_id)
                .map(|(_, r)| *r),
            // 탭 범위는 탭 바 아래 콘텐츠 영역이다. 탭 바는 pane 범위에 들어간다.
            PopupScope::Tab(tab_id) => {
                let (pane_id, _) = ctx.active_tab_ids.iter().find(|(_, tid)| *tid == *tab_id)?;
                ctx.tab_content_rects
                    .iter()
                    .find(|(id, _)| *id == *pane_id)
                    .map(|(_, r)| *r)
            }
            PopupScope::Surface(surface_id) => ctx
                .surface_rects
                .iter()
                .find(|(id, _)| *id == *surface_id)
                .map(|(_, r)| *r),
        }
    }

    /// surface 팝업만 보더에서 inset만큼 안쪽에 둔다. 좁은 영역은 여백을 줄여 경계 역전을 막는다.
    /// 창·워크스페이스·pane·tab 범위에는 이 여백을 적용하지 않는다.
    pub(crate) fn scope_bounds(
        scope: &PopupScope,
        ctx: Option<&LayoutContext>,
        screen_rect: egui::Rect,
        inset: f32,
    ) -> egui::Rect {
        // 소속 영역이 없으면 화면을 경계로 쓰며 inset은 적용하지 않는다.
        let Some(rect) = Self::scope_rect(scope, ctx) else {
            return screen_rect;
        };
        if !matches!(scope, PopupScope::Surface(_)) {
            return rect;
        }
        let inset = inset
            .min(rect.width() / 2.0)
            .min(rect.height() / 2.0)
            .max(0.0);
        rect.shrink(inset)
    }

    /// z 오름차순의 팝업 영역을 받아 scrim을 그릴 위치를 고른다.
    /// 같은 범위에는 한 번만 그리며 넓은 영역이 포함하는 좁은 영역은 제외해 중복으로 어두워지지 않게 한다.
    pub(crate) fn pick_scrim_layers(rects: &[egui::Rect]) -> Vec<bool> {
        let mut paints = vec![false; rects.len()];
        let mut chosen: Vec<usize> = Vec::new();
        for (i, rect) in rects.iter().enumerate() {
            if chosen.iter().any(|&c| rects[c].contains_rect(*rect)) {
                continue;
            }
            chosen.retain(|&c| {
                let covered = rect.contains_rect(rects[c]);
                if covered {
                    paints[c] = false;
                }
                !covered
            });
            chosen.push(i);
            paints[i] = true;
        }
        paints
    }
}

/// 팝업 인스턴스의 egui 레이어·Area ID.
fn area_id_of(popup_id: PopupId, instance: PopupInstanceId) -> egui::Id {
    egui::Id::new("popup").with(popup_id).with(instance.0)
}

/// egui는 새로 보이거나 눌린 영역만 맨 위로 올리고, 올린 영역끼리는 기존 순서를 지킨다. 매니저가 순서를 바꾼
/// 프레임에는 직전 순서에서 제자리를 벗어난 레이어만 올려 그린 순서(아래에서 위)에 맞춘다. 순서가 그대로인
/// 프레임에는 건드리지 않아 팝업 안에서 연 드롭다운이 아래로 내려가지 않는다. 한 번에 맞출 수 없는 순서는
/// 다음 프레임에 이어서 맞춘다.
fn restack_layers(ctx: &egui::Context, layers: &[egui::LayerId]) {
    let key = egui::Id::new("popup.layer_order");
    let last: Vec<egui::LayerId> = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    // 직전 순서에서 사라진 레이어를 빼고, 처음 보이는 레이어는 egui가 맨 위에 둔다.
    let fresh: Vec<egui::LayerId> = layers
        .iter()
        .filter(|l| !last.contains(l))
        .copied()
        .collect();
    let mut now: Vec<egui::LayerId> = last.into_iter().filter(|l| layers.contains(l)).collect();
    now.extend(&fresh);
    let mut lifted = Vec::new();
    let mut highest_kept: Option<usize> = None;
    for layer in layers {
        let at = now.iter().position(|l| l == layer).unwrap_or(now.len());
        if highest_kept.is_none_or(|top| at > top) {
            highest_kept = Some(at);
        } else {
            lifted.push(*layer);
        }
    }
    for layer in &lifted {
        ctx.move_to_top(*layer);
    }
    let raised = |l: &egui::LayerId| lifted.contains(l) || fresh.contains(l);
    let mut next: Vec<egui::LayerId> = now.iter().filter(|l| !raised(l)).copied().collect();
    next.extend(now.iter().filter(|l| raised(l)));
    ctx.data_mut(|d| d.insert_temp(key, next));
}

#[cfg(test)]
#[path = "area_id_tests.rs"]
mod area_id_tests;

#[cfg(test)]
mod tests {
    use super::*;
    // 픽스처 폰트도 토큰에서 가져온다 — `Theme` 인스턴스가 없는 순수 유닛테스트라
    // zoom 적용 전 원본 상수(`SIZING`)를 쓴다.
    use tasty_type_appearance::theme::SIZING;

    /// `ctx.fonts()` 는 최소 한 프레임(`run`)이 지나야 폰트 정의가 로드된다.
    fn with_ctx<R>(f: impl FnOnce(&egui::Context) -> R) -> R {
        let ctx = egui::Context::default();
        let mut out = None;
        let mut f = Some(f);
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            out = Some((f.take().unwrap())(ctx));
        }));
        out.unwrap()
    }

    #[test]
    fn elide_for_width_keeps_short_text_untouched() {
        with_ctx(|ctx| {
            let font = egui::FontId::proportional(SIZING.font_size_max.value());
            let text = "짧은 제목";
            assert_eq!(elide_for_width(ctx, text, font, 1000.0), text);
        });
    }

    #[test]
    fn elide_for_width_truncates_long_text_with_ellipsis() {
        with_ctx(|ctx| {
            let font = egui::FontId::proportional(SIZING.font_size_max.value());
            let text =
                "파일 핸들러 선택: /Users/ljh/workspace/etc/teams-mcp-very-long-path/Cargo.toml";
            let result = elide_for_width(ctx, text, font, 40.0);
            assert!(result.chars().count() < text.chars().count());
            assert!(result.ends_with('…'));
        });
    }

    #[test]
    fn elide_for_width_zero_or_negative_width_returns_empty() {
        with_ctx(|ctx| {
            let font = egui::FontId::proportional(SIZING.font_size_max.value());
            assert_eq!(elide_for_width(ctx, "anything", font.clone(), 0.0), "");
            assert_eq!(elide_for_width(ctx, "anything", font, -5.0), "");
        });
    }

    /// 형식 Tag가 배경에 묻히지 않도록 셸과 다른 색을 사용한다.
    #[test]
    fn popup_shell_fill_keeps_the_default_tag_visible() {
        let th = theme::theme();
        let shell: egui::Color32 =
            declared_bg(super::super::file_handler_picker::PICKER_POPUP_ID, &th);
        let tag: egui::Color32 = th.tag_bg().into();
        assert_ne!(
            shell, tag,
            "핸들러 선택기 셸이 tag-bg 와 같은 색이면 format Tag 가 사라진다"
        );
        assert_eq!(shell, egui::Color32::from(th.bg_panel()));
    }

    /// rename 팝업의 읽기 전용 칸은 채움으로, 평소 이름 칸은 테두리로 바탕과 구분된다. 두 테마 모두 본다.
    #[test]
    fn the_rename_popup_ground_keeps_the_name_field_box_visible() {
        for text in [tasty_themes::MOCHA_TOML_TEXT, tasty_themes::LATTE_TOML_TEXT] {
            let file = tasty_themes::ThemeFile::parse(text).expect("built-in theme");
            let (partial, is_light) = file.to_partial();
            let mut colors = tasty_themes::mocha_fallback_colors();
            colors.apply_partial(&partial);
            let th =
                tasty_type_appearance::theme::Theme::with_colors(colors, is_light.unwrap_or(false));
            let shell = declared_bg("rename", &th);
            assert_eq!(shell, egui::Color32::from(th.dialog_bg()));
            for (what, fill) in [
                ("readonly", th.input_readonly_bg()),
                ("normal", th.input_border()),
            ] {
                assert_ne!(
                    shell,
                    egui::Color32::from(fill),
                    "{what} 칸이 바탕과 같은 색이다"
                );
            }
        }
    }

    /// DAG 목록 팝업의 검색 칸은 bg-panel 셸 위에서 테두리로 구분된다. Latte의 채움은 셸과 같은 색이다.
    #[test]
    fn the_dag_list_shell_keeps_the_search_field_box_visible() {
        for text in [tasty_themes::MOCHA_TOML_TEXT, tasty_themes::LATTE_TOML_TEXT] {
            let file = tasty_themes::ThemeFile::parse(text).expect("built-in theme");
            let (partial, is_light) = file.to_partial();
            let mut colors = tasty_themes::mocha_fallback_colors();
            colors.apply_partial(&partial);
            let th =
                tasty_type_appearance::theme::Theme::with_colors(colors, is_light.unwrap_or(false));
            let shell = declared_bg(super::super::dag_list::DAG_LIST_POPUP_ID, &th);
            assert_eq!(shell, egui::Color32::from(th.bg_panel()));
            assert_ne!(
                shell,
                egui::Color32::from(th.input_border()),
                "검색 칸 테두리가 바탕과 같은 색이다"
            );
        }
    }

    /// file picker 와 탐색기 Properties 오류 상태의 Retry·Reconnect(Secondary) 채움과 테두리가 셸과 달라야
    /// 버튼으로 보인다.
    #[test]
    fn file_picker_shell_keeps_the_secondary_button_visible() {
        let th = theme::theme();
        for id in [
            super::super::file_picker::FILE_PICKER_POPUP_ID,
            super::super::explorer_properties::EXPLORER_PROPERTIES_POPUP_ID,
        ] {
            let shell: egui::Color32 = declared_bg(id, &th);
            let fill: egui::Color32 = th.button_secondary_bg().into();
            let edge: egui::Color32 = th.button_secondary_border().into();
            assert_ne!(shell, fill, "{id}: 셸과 Secondary 버튼 채움이 같은 색이다");
            assert_ne!(
                shell, edge,
                "{id}: 셸과 Secondary 버튼 테두리가 같은 색이다"
            );
            assert_eq!(shell, egui::Color32::from(th.bg_panel()), "{id}");
        }
    }

    // 이웃 surface는 경계만 공유하고 내부 영역은 겹치지 않는다.
    const SURFACE_A: u32 = 11;
    const SURFACE_B: u32 = 22;

    fn screen() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1000.0, 800.0))
    }

    fn surface_a() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(32.0, 60.0), egui::pos2(500.0, 776.0))
    }

    fn surface_b() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(500.0, 60.0), egui::pos2(1000.0, 776.0))
    }

    fn two_surface_layout() -> LayoutContext {
        LayoutContext {
            active_workspace: 0,
            active_workspace_id: 0,
            pane_rects: vec![(
                1,
                egui::Rect::from_min_max(egui::pos2(32.0, 60.0), egui::pos2(1000.0, 776.0)),
            )],
            surface_rects: vec![(SURFACE_A, surface_a()), (SURFACE_B, surface_b())],
            active_tabs: vec![(1, 0)],
            active_tab_ids: vec![],
            workspace_rect: None,
            surface_owners: Vec::new(),
            tab_content_rects: Vec::new(),
        }
    }

    /// surface 범위의 scrim 자리는 **그 칸의 rect 그대로**다. 보더는 칸 안쪽에 그려지므로
    /// 이 rect 가 보더를 포함하고, 인접 칸은 변 하나만 공유할 뿐 안쪽이 안 겹친다.
    #[test]
    fn surface_scope_covers_its_own_surface_and_not_the_neighbour() {
        let layout = two_surface_layout();
        let rect = PopupManager::scope_rect(&PopupScope::Surface(SURFACE_A), Some(&layout))
            .expect("surface A 는 이 레이아웃에 있다");
        assert_eq!(rect, surface_a());
        // 인접 칸의 안쪽 점은 이 scrim 밖이다.
        assert!(!rect.contains(surface_b().center()));
        // 사이드바(x<32)·탭바(y<60)·상태바(y>776)도 밖이다.
        assert!(!rect.contains(egui::pos2(16.0, 400.0)));
        assert!(!rect.contains(egui::pos2(300.0, 40.0)));
        assert!(!rect.contains(egui::pos2(300.0, 790.0)));
    }

    #[test]
    fn window_scope_still_falls_back_to_the_whole_screen() {
        let layout = two_surface_layout();
        assert_eq!(
            PopupManager::scope_rect(&PopupScope::Window, Some(&layout)),
            None
        );
        assert_eq!(
            PopupManager::scope_bounds(&PopupScope::Window, Some(&layout), screen(), 8.0),
            screen()
        );
    }

    /// target 바인딩이 없는 호환 경로(레이아웃 컨텍스트 자체가 없는 프레임)도 화면 전체다.
    #[test]
    fn a_surface_scope_without_layout_context_falls_back_to_the_whole_screen() {
        assert_eq!(
            PopupManager::scope_rect(&PopupScope::Surface(SURFACE_A), None),
            None
        );
        assert_eq!(
            PopupManager::scope_bounds(&PopupScope::Surface(SURFACE_A), None, screen(), 8.0),
            screen()
        );
    }

    /// surface 범위 popup 은 칸에서 8pt 안쪽에 놓인다. 창 범위는 들이지 않는다.
    #[test]
    fn surface_bounds_inset_the_popup_and_window_bounds_do_not() {
        let layout = two_surface_layout();
        let bounds = PopupManager::scope_bounds(
            &PopupScope::Surface(SURFACE_A),
            Some(&layout),
            screen(),
            8.0,
        );
        assert_eq!(bounds, surface_a().shrink(8.0));
        assert_eq!(
            PopupManager::scope_bounds(&PopupScope::Window, Some(&layout), screen(), 8.0),
            screen()
        );
    }

    /// 칸이 inset 두 배보다 좁으면 경계가 뒤집히지 않고 폭이 0 으로 수렴한다.
    #[test]
    fn surface_bounds_never_invert_on_a_tiny_surface() {
        let tiny = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(6.0, 400.0));
        let layout = LayoutContext {
            active_workspace: 0,
            active_workspace_id: 0,
            pane_rects: vec![],
            surface_rects: vec![(SURFACE_A, tiny)],
            active_tabs: vec![],
            active_tab_ids: vec![],
            workspace_rect: None,
            surface_owners: Vec::new(),
            tab_content_rects: Vec::new(),
        };
        let bounds = PopupManager::scope_bounds(
            &PopupScope::Surface(SURFACE_A),
            Some(&layout),
            screen(),
            8.0,
        );
        assert!(bounds.width() >= 0.0 && bounds.height() >= 0.0);
        assert!(tiny.contains_rect(bounds));
    }

    /// 부모 popup 과 그것이 연 자식이 같은 scope 를 쓰면 scrim 은 **한 번만** 깔린다.
    #[test]
    fn a_parent_and_its_child_in_one_scope_paint_a_single_scrim() {
        let rects = [surface_a(), surface_a()];
        let paints = PopupManager::pick_scrim_layers(&rects);
        assert_eq!(
            paints.iter().filter(|p| **p).count(),
            1,
            "같은 scope 에 두 번 깔면 같은 알파가 곱해져 그 칸만 두 배로 어두워진다"
        );
        assert!(paints[0], "아래 깔린 쪽이 그린다");
    }

    /// 창 전체 scrim 이 있으면 그 안의 surface scrim 은 안 깐다 — 순서가 어느 쪽이든.
    #[test]
    fn a_window_scrim_absorbs_a_surface_scrim_inside_it() {
        let surface_first = PopupManager::pick_scrim_layers(&[surface_a(), screen()]);
        assert_eq!(surface_first, vec![false, true]);
        let window_first = PopupManager::pick_scrim_layers(&[screen(), surface_a()]);
        assert_eq!(window_first, vec![true, false]);
    }

    /// 서로 다른 칸에 하나씩 뜨면 각자 자기 칸을 덮는다 — 한쪽이 다른 쪽을 안 삼킨다.
    #[test]
    fn two_separate_surfaces_each_keep_their_own_scrim() {
        let paints = PopupManager::pick_scrim_layers(&[surface_a(), surface_b()]);
        assert_eq!(paints, vec![true, true]);
    }

    fn decl_of(id: &str) -> super::super::PopupDecl {
        super::super::defs::find(id)
            .unwrap_or_else(|| panic!("{id} 정의가 없다"))
            .decl
    }

    /// 셸 배경은 선언한 셸 묶음이 정한다.
    fn declared_bg(id: &str, th: &tasty_type_appearance::theme::Theme) -> egui::Color32 {
        shell_colors(decl_of(id).shell, th).bg
    }

    /// scrim 여부는 범위가 아니라 선언으로 정한다 — `search_bar` 는 surface 범위를 쓰지만
    /// anchored + scrim-less 갈래다(docs/architecture/ui-widgets-crate.md).
    #[test]
    fn the_anchored_search_bar_takes_no_scrim_although_it_is_surface_scoped() {
        assert!(!decl_of("search_bar").covers_scope);
        assert!(decl_of("convert_surface").covers_scope);
        assert!(decl_of("command_palette").covers_scope);
        assert!(
            decl_of(super::super::file_picker::FILE_PICKER_POPUP_ID).covers_scope,
            "picker 는 자기 범위를 덮는다. 부모와 같은 범위면 scrim 은 한 번만 칠한다"
        );
    }

    /// 범위 가운데가 아닌 곳에 붙는 팝업은 popover 그림자이고 scrim이 없다.
    #[test]
    fn no_anchored_popup_takes_a_scrim() {
        let anchored: Vec<_> = super::super::defs::all_defs()
            .iter()
            .filter(|d| d.decl.anchor != super::super::PopupAnchor::ScopeCenter)
            .collect();
        assert_eq!(
            anchored.len(),
            6,
            "tools_menu·search_bar·rail_category·배너 더보기 메뉴·단축키 도움말·복원 안내"
        );
        for def in anchored {
            assert!(
                !def.decl.covers_scope,
                "`{}` 는 anchored + scrim-less 갈래인데 scope를 덮는다고 선언했다 — 둘 중 \
                 하나가 틀렸다(docs/architecture/ui-widgets-crate.md)",
                def.id
            );
            assert_eq!(def.decl.shadow, PopupShadow::Popover, "{}", def.id);
        }
    }
}
