//! 팝업 렌더링과 입력·표시 범위 계산.

use crate::adapters::ui::LayoutContext;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::elide_popup_title as elide_for_width;

use super::occlusion::{Occluder, PointOwnership, point_ownership};
use super::{PopupDrawResult, PopupId, PopupManager, PopupScope, ResizeEdges};

/// 리사이즈 테두리 밴드 폭(px). popup_rect 가장자리 안쪽 이 폭 안에서 누르면 리사이즈.
const RESIZE_BAND: LogicalPx = LogicalPx(6.0);

/// scrim 없이 콘텐츠 위에 붙는 팝업 목록. 트리거 옆 또는 범위 상단에 표시한다.
/// 위치는 열 때 지정되므로 PopupDef에서 읽을 수 없어 별도 목록으로 관리한다.
const ANCHORED_POPUPS: &[PopupId] = &[
    "tools_menu",
    "search_bar",
    super::rail_category::RAIL_CATEGORY_POPUP_ID,
    crate::adapters::ui::mouse_capture_menu::MOUSE_CAPTURE_BANNER_MENU_POPUP_ID,
];

/// popover 목록에 없는 팝업은 modal 그림자를 사용한다.
/// scrim 여부와는 별개인 분류다(docs/architecture/ui-widgets-crate.md).
fn popup_shadow(popup_id: PopupId) -> Option<tasty_type_appearance::theme::ShadowToken> {
    let th = theme::theme();
    if ANCHORED_POPUPS.contains(&popup_id) {
        Some(th.shadow_popover())
    } else {
        Some(th.shadow_modal())
    }
}

/// 팝업 종류별 배경. file_handler_picker의 배경은 default Tag 채움색과 구분해야 한다.
/// popup_shell_fill_keeps_the_default_tag_visible에서 두 색을 비교한다.
/// file picker(설정 창 파일 선택기 포함)는 오류 상태의 Secondary 버튼(`button-secondary-bg` = surface-raised)이 셸에
/// 묻히지 않도록 시안 창 배경 bg-panel 을 쓴다. 탐색기 Properties 도 시안 YProps 바탕이 bg-panel 이고
/// 읽기 실패 화면의 Retry 가 같은 Secondary 버튼이다. rename 팝업(workspace·탭·카테고리·탐색기 항목·
/// 즐겨찾기 추가)은 dialog 라 dialog-bg 이며, 그 위라야 읽기 전용 이름 칸의 상자가 보인다.
/// DAG 목록 팝업은 시안 `DagWindow` 바탕이 bg-panel 이고 검색 칸은 기본 바탕 Input 이다. surface-raised
/// 위에서는 input-bg·input-border 가 바탕과 같은 색이 되어 칸이 사라진다.
fn popup_bg_fill(popup_id: PopupId, th: &tasty_type_appearance::theme::Theme) -> egui::Color32 {
    match popup_id {
        "notifications" | "remote_tool" | "port_scanner" | "tutorial_topics" | "remote_attach"
        | "convert_surface" => th.bg_panel().into(),
        super::transfer::TRANSFER_PROGRESS_POPUP_ID
        | super::transfer::TRANSFER_ERROR_POPUP_ID
        | super::file_handler_picker::PICKER_POPUP_ID
        | super::file_picker::FILE_PICKER_POPUP_ID
        | crate::view::file_chooser::FILE_CHOOSER_POPUP_ID
        | super::explorer_properties::EXPLORER_PROPERTIES_POPUP_ID
        | super::dag_list::DAG_LIST_POPUP_ID
        | crate::adapters::ui::info_modal::INFO_MODAL_ID => th.bg_panel().into(),
        "rename" => th.dialog_bg().into(),
        _ => th.surface_raised().into(),
    }
}

/// 셸 테두리 색. 메뉴형 팝업(Tools 메뉴·레일 카테고리 메뉴)은 egui 팝오버와 같은 `menu-border`,
/// 검색 바는 `search-bar-border`, 마우스 캡처 배너 메뉴는 `banner-more-menu-border` 다(셋 다
/// → border-strong). 나머지는 공통 frame 색이다.
fn popup_border(popup_id: PopupId, th: &tasty_type_appearance::theme::Theme) -> egui::Color32 {
    match popup_id {
        crate::adapters::ui::mouse_capture_menu::MOUSE_CAPTURE_BANNER_MENU_POPUP_ID => {
            th.banner_more_menu_border().into()
        }
        "tools_menu" | super::rail_category::RAIL_CATEGORY_POPUP_ID => th.menu_border().into(),
        "search_bar" => th.search_bar_border().into(),
        _ => th.border_frame().into(),
    }
}

/// 타이틀바 아래 선 색. 안내 모달은 디자인의 info modal 셸 규칙(`info-modal-title-edge`)을
/// 따르고 나머지는 공통 값을 쓴다.
fn popup_title_edge(popup_id: PopupId, th: &tasty_type_appearance::theme::Theme) -> egui::Color32 {
    if popup_id == crate::adapters::ui::info_modal::INFO_MODAL_ID {
        th.info_modal_title_edge().into()
    } else {
        th.border_frame().into()
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
fn resize_edges_at(rect: egui::Rect, pos: egui::Pos2, band: f32) -> Option<ResizeEdges> {
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
        content_fn: &mut dyn FnMut(&str, &mut egui::Ui),
        draw_ctx: Option<&LayoutContext>,
        plugin_occluders: &[Occluder],
    ) -> PopupDrawResult {
        let th = theme::theme();
        let screen_rect = ctx.screen_rect();
        let mut closed: Vec<PopupId> = Vec::new();
        let mut bring_front: Option<PopupId> = None;
        let mut fullscreen_requested: Option<crate::adapters::ui::fullscreen::StageId> = None;
        let mut layers: Vec<egui::LayerId> = Vec::new();
        let mut overlay_targets: Vec<crate::state::webview_overlay::OverlayTarget> = Vec::new();

        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let primary_released = ctx.input(|i| i.pointer.any_released());

        // Hidden scopes retain focus intent, but cannot hold the keyboard gate or
        // continue a pointer gesture started before the scope disappeared.
        for popup in &mut self.popups {
            popup.scope_visible = Self::is_scope_visible(&popup.scope, draw_ctx);
            if !popup.scope_visible {
                popup.dragging = false;
                popup.resizing = None;
            }
        }

        let open_indices: Vec<usize> = self
            .popups
            .iter()
            .enumerate()
            .filter(|(_, p)| p.open && Self::is_scope_visible(&p.scope, draw_ctx))
            .map(|(i, _)| i)
            .collect();

        // 이번 프레임의 영역을 뒤에 그릴 plugin 팝업에 전달한다.
        // 호출 순서는 egui_bridge::run_egui_frame과 source_guards::frame_draw_order에서 확인한다.
        let hit_rects: Vec<Occluder> = open_indices
            .iter()
            .map(|&i| Occluder {
                rect: self.popups[i].popup_rect(),
                z_seq: self.popups[i].z_seq,
            })
            .collect();

        // 입력 우선순위: 닫기·전체화면 버튼 > 리사이즈 테두리 > 이동 손잡이 > 콘텐츠.
        let mut hovered_popup: Option<PopupId> = None;
        let mut hovered_handle: Option<PopupId> = None;
        let mut hovered_close: Option<PopupId> = None;
        let mut hovered_fullscreen: Option<(PopupId, crate::adapters::ui::fullscreen::StageId)> =
            None;
        let mut hovered_resize: Option<(PopupId, ResizeEdges)> = None;
        if let Some(pos) = pointer_pos {
            for &idx in open_indices.iter().rev() {
                let popup = &self.popups[idx];
                let rect = popup.popup_rect();
                // 위의 plugin 팝업이 덮으면 건너뛴다. 아래 host 팝업의 다른 영역도 검사해야 하므로 continue한다.
                if matches!(
                    point_ownership(rect, popup.z_seq, plugin_occluders, pos),
                    PointOwnership::OccludedByHigher
                ) {
                    continue;
                }
                if rect.contains(pos) {
                    hovered_popup = Some(popup.id);
                    if !popup.headless && popup.close_btn_rect().contains(pos) {
                        hovered_close = Some(popup.id);
                    } else if let Some((fs_rect, stage)) =
                        popup.fullscreen_btn_rect().zip(popup.fullscreen_stage)
                        && fs_rect.contains(pos)
                    {
                        hovered_fullscreen = Some((popup.id, stage));
                    } else if popup.resizable
                        && let Some(edges) = resize_edges_at(rect, pos, RESIZE_BAND.value())
                    {
                        hovered_resize = Some((popup.id, edges));
                    } else if let Some(handle) = popup.effective_drag_handle_rect(ctx)
                        && handle.contains(pos)
                    {
                        hovered_handle = Some(popup.id);
                    }
                    break; // topmost popup wins
                } else if super::child_overlay_hit(ctx, popup.id, pos) {
                    // 부모 밖으로 나온 자식 드롭다운도 안쪽 클릭으로 취급한다.
                    hovered_popup = Some(popup.id);
                    break;
                }
            }
        }

        // 닫기·포커스·맨 앞으로 올리기를 먼저 처리한다. 이동·리사이즈 시작은
        // 콘텐츠를 그린 뒤 위젯이 포인터를 사용했는지 확인하고 결정한다.
        if primary_pressed {
            if let Some(id) = hovered_close {
                closed.push(id);
            } else if let Some((_, stage)) = hovered_fullscreen {
                // 무대는 별도 콘텐츠이므로 원본 팝업은 닫지 않는다.
                fullscreen_requested = Some(stage);
            } else if let Some(id) = hovered_popup {
                bring_front = Some(id);
                for popup in &mut self.popups {
                    if popup.scope_visible {
                        popup.focused = popup.id == id;
                        popup.passive &= !popup.focused;
                    }
                }
            } else {
                // 위의 plugin 팝업이 덮은 곳은 바깥 클릭으로 처리하지 않는다.
                // host를 먼저 그리므로 plugin 영역은 직전 프레임 값이다. 방금 닫힌
                // plugin 팝업이 클릭을 한 번 막을 수 있지만 가려진 팝업을 잘못 닫는 것을 피한다.
                for popup in &mut self.popups {
                    if !popup.scope_visible {
                        continue;
                    }
                    let occluded = pointer_pos.is_some_and(|p| {
                        matches!(
                            point_ownership(popup.popup_rect(), popup.z_seq, plugin_occluders, p),
                            PointOwnership::OccludedByHigher
                        )
                    });
                    if occluded {
                        continue;
                    }
                    if popup.open && popup.close_on_outside_click {
                        closed.push(popup.id);
                    }
                    // sticky_focus popups keep keyboard focus when clicking outside.
                    if !popup.sticky_focus {
                        popup.focused = false;
                    }
                }
            }
        }

        for popup in &mut self.popups {
            if !popup.dragging {
                continue;
            }
            if primary_released {
                popup.dragging = false;
            } else if primary_down && let Some(pos) = pointer_pos {
                let bounds = Self::scope_rect(&popup.scope, draw_ctx).unwrap_or(screen_rect);
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
                let bounds = Self::scope_rect(&popup.scope, draw_ctx).unwrap_or(screen_rect);
                let new_rect = egui::Rect::from_min_max(min, max).intersect(bounds);
                popup.pos = new_rect.min;
                popup.size = new_rect.size();
                popup.size_user_overridden = true;
            }
        }

        for popup in &mut self.popups {
            if popup.request_center && popup.open {
                let center_rect = Self::scope_rect(&popup.scope, draw_ctx).unwrap_or(screen_rect);
                popup.pos = egui::pos2(
                    center_rect.center().x - popup.size.x / 2.0,
                    center_rect.center().y - popup.size.y / 2.0,
                );
                popup.request_center = false;
            }
        }

        for popup in &mut self.popups {
            if popup.request_top && popup.open {
                let anchor_rect = Self::scope_rect(&popup.scope, draw_ctx).unwrap_or(screen_rect);
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
        } else if hovered_close.is_some() || hovered_fullscreen.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        } else if hovered_popup.is_some() {
            // Content area: set default cursor (arrow) to override terminal cursor
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }

        // 상위 팝업의 넓은 scrim이 하위 scrim을 덮을 수 있어 렌더링 전에 범위를 고른다.
        let scrim_candidates: Vec<(usize, egui::Rect)> = open_indices
            .iter()
            .filter(|&&i| !closed.contains(&self.popups[i].id))
            .filter(|&&i| Self::popup_has_scrim(self.popups[i].id))
            .map(|&i| {
                (
                    i,
                    Self::scope_rect(&self.popups[i].scope, draw_ctx).unwrap_or(screen_rect),
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
        for (z_idx, &popup_idx) in open_indices.iter().enumerate() {
            let popup = &mut self.popups[popup_idx];
            if closed.contains(&popup.id) {
                continue;
            }

            let scope_clip = Self::scope_rect(&popup.scope, draw_ctx).unwrap_or(screen_rect);
            let clamp_rect =
                Self::scope_bounds(&popup.scope, draw_ctx, screen_rect, th.spacing_sm.value());
            popup.clamp_to_screen(clamp_rect);

            let popup_id = popup.id;
            let is_headless = popup.headless;
            let popup_rect = popup.popup_rect();
            let content_rect = popup.content_rect();

            let layer_id = egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("popup").with(popup_id).with(z_idx),
            );
            layers.push(layer_id);

            // 그림자가 이웃 영역까지 어둡게 하지 않도록 팝업 소속 범위로 자른다.
            let painter = ctx.layer_painter(layer_id).with_clip_rect(scope_clip);

            // 페이지와 겹친 상자는 전체가 각진 모서리·그림자 없음이다. passive 팝업은 페이지를 숨기는
            // 기존 규칙을 따르므로 겹칠 페이지가 없다.
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

            let bg_fill: egui::Color32 = popup_bg_fill(popup_id, &th);
            // 그림자는 scrim 위, 셸 아래에 그린다.
            let shadow = popup_shadow(popup_id).map(|s| edge.shadow(&th, s));
            if let Some(shadow) = shadow {
                painter.add(shadow.to_egui().as_shape(popup_rect, radius));
            }
            if !popup.passive {
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
                    paint_rect: shadow
                        .map_or(popup_rect, |s| popup_rect + s.to_egui().margin())
                        .intersect(scope_clip),
                    scrim,
                });
            }
            painter.rect_filled(popup_rect, radius, bg_fill);
            painter.rect_stroke(
                popup_rect,
                radius,
                egui::Stroke::new(th.border_width.value(), popup_border(popup_id, &th)),
                egui::StrokeKind::Outside,
            );

            if !is_headless {
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
                let title_edge = popup_title_edge(popup_id, &th);
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
                let elided_title = elide_for_width(
                    ctx,
                    &popup.title,
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
                if elided_title != popup.title {
                    let tip_id = egui::Id::new("popup.title_tooltip").with(popup_id);
                    let band_hovered = hovered_popup == Some(popup_id)
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
                    let hovered = matches!(hovered_fullscreen, Some((id, _)) if id == popup_id);
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

                title_icon_button(
                    ctx,
                    &painter,
                    &th,
                    close_btn_rect,
                    crate::adapters::ui::icons::CLOSE,
                    hovered_close == Some(popup_id),
                    primary_down,
                );
            }

            // egui가 스크롤·호버 영역으로 인식하도록 같은 layer ID의 Area에 콘텐츠를 그린다.
            // 이동은 이 매니저가 처리하고 내부 위젯에는 클릭·드래그를 남긴다.
            {
                let area_id = egui::Id::new("popup").with(popup_id).with(z_idx);
                egui::Area::new(area_id)
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
                            || content_fn(popup_id, ui),
                        );
                    });
            }
        }

        // 콘텐츠 위젯이 포인터를 사용하지 않았을 때만 리사이즈·이동을 시작한다.
        // 수동 드래그는 egui 위젯이 아니므로 is_using_pointer를 스스로 설정하지 않는다.
        if primary_pressed && !ctx.is_using_pointer() {
            if let Some((id, edges)) = hovered_resize {
                if let Some(popup) = self.popups.iter_mut().find(|p| p.id == id) {
                    popup.resizing = Some(edges);
                    popup.resize_start_rect = popup.popup_rect();
                }
            } else if let Some(id) = hovered_handle
                && let Some(popup) = self.popups.iter_mut().find(|p| p.id == id)
            {
                popup.dragging = true;
                if let Some(pos) = pointer_pos {
                    popup.drag_offset = pos - popup.pos;
                }
            }
        }

        for id in &closed {
            self.close(id);
        }

        if let Some(id) = bring_front {
            self.bring_to_front(id);
        }

        PopupDrawResult {
            closed,
            hovered: hovered_popup.is_some(),
            layers,
            fullscreen_requested,
            hit_rects,
            overlay_targets,
        }
    }

    /// 현재 그릴 팝업 중 z가 가장 높은 것을 찾는다. Escape 처리 대상을 정할 때 쓴다.
    pub fn topmost_visible_open(&self, draw_ctx: Option<&LayoutContext>) -> Option<(PopupId, u64)> {
        self.popups
            .iter()
            .filter(|p| p.open && Self::is_scope_visible(&p.scope, draw_ctx))
            .max_by_key(|p| p.z_seq)
            .map(|p| (p.id, p.z_seq))
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
            PopupScope::Window => None,       // use screen_rect (caller default)
            PopupScope::Workspace(_) => None, // workspace fills window
            PopupScope::Pane(pane_id) => ctx
                .pane_rects
                .iter()
                .find(|(id, _)| *id == *pane_id)
                .map(|(_, r)| *r),
            PopupScope::Tab(tab_id) => {
                let (pane_id, _) = ctx.active_tab_ids.iter().find(|(_, tid)| *tid == *tab_id)?;
                ctx.pane_rects
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

    /// scrim은 범위가 아니라 팝업 ID로 정한다. surface 소속인 search_bar도 scrim은 사용하지 않는다.
    pub(crate) fn popup_has_scrim(id: PopupId) -> bool {
        matches!(
            id,
            "remote_tool"
                | "remote_attach"
                | "command_palette"
                | "port_scanner"
                | "convert_surface"
                | super::transfer::TRANSFER_PROGRESS_POPUP_ID
                | super::transfer::TRANSFER_ERROR_POPUP_ID
                | super::explorer_conflict::EXPLORER_CONFLICT_POPUP_ID
                | super::confirm_close_remote::CONFIRM_CLOSE_REMOTE_POPUP_ID
        )
    }
}

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
            popup_bg_fill(super::super::file_handler_picker::PICKER_POPUP_ID, &th);
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
            let shell = popup_bg_fill("rename", &th);
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
            let shell = popup_bg_fill(super::super::dag_list::DAG_LIST_POPUP_ID, &th);
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
            let shell: egui::Color32 = popup_bg_fill(id, &th);
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

    /// scrim 여부는 범위가 아니라 id 로 정한다 — `search_bar` 는 surface 범위를 쓰지만
    /// anchored + scrim-less 갈래다(docs/architecture/ui-widgets-crate.md).
    #[test]
    fn the_anchored_search_bar_takes_no_scrim_although_it_is_surface_scoped() {
        assert!(!PopupManager::popup_has_scrim("search_bar"));
        assert!(PopupManager::popup_has_scrim("convert_surface"));
        assert!(PopupManager::popup_has_scrim("command_palette"));
        assert!(
            !PopupManager::popup_has_scrim(super::super::file_picker::FILE_PICKER_POPUP_ID),
            "자식 picker 는 부모의 scrim 위에 얹힌다 — 자기 것을 덧그리지 않는다"
        );
    }

    /// popover 그림자를 사용하는 팝업에는 scrim이 없는지 두 목록을 대조한다.
    #[test]
    fn no_anchored_popup_takes_a_scrim() {
        for id in ANCHORED_POPUPS {
            assert!(
                !PopupManager::popup_has_scrim(id),
                "`{id}` 는 anchored + scrim-less 갈래인데 scrim 명부에 있다 — 둘 중 \
                 하나가 틀렸다(docs/architecture/ui-widgets-crate.md)"
            );
        }
    }
}
