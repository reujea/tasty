//! Painting and interaction for one tab slot.

use super::{PaneTabBarView, PaneTabBarsOutput, PaneTabBarsProps, TabBarAction};
use crate::adapters::ui::{icons, zoomed_px};
use tasty_type_geometry::length::LogicalPx;

/// 점 형태의 활성 표시 지름. 대응 역할 토큰이 없어 별도로 두며 밑줄 두께와 구분한다.
const TAB_ACTIVE_DOT_SIZE: LogicalPx = LogicalPx(4.0);

/// Inputs shared by the tab slots in one clipped pane strip.
pub(super) struct TabRenderContext<'a, 'props> {
    pub props: &'a PaneTabBarsProps<'props>,
    pub info: &'a PaneTabBarView,
    pub painter: &'a egui::Painter,
    pub clip_rect: egui::Rect,
    pub bg: tasty_type_appearance::color::HexColor,
    pub separator_w: LogicalPx,
}

pub(super) fn draw_tab(
    ui: &mut egui::Ui,
    context: &TabRenderContext<'_, '_>,
    output: &mut PaneTabBarsOutput,
    i: usize,
    start_x: LogicalPx,
) -> LogicalPx {
    let TabRenderContext {
        props,
        info,
        painter,
        clip_rect,
        bg,
        separator_w,
    } = *context;
    let th = props.theme;
    let name = &info.tab_names[i];
    let mut x = start_x.value();
    let separator_w = separator_w.value();
    let tab_w = props.tab_width;
    let bar_h = th.tab_bar_height.value();
    let label_font_size = props.tab_font_size;
    let h_padding: f32 = 8.0;
    let active_indicator_h = th.tab_indicator_width().value();
    if i > 0 {
        let sep = egui::Rect::from_min_size(
            egui::pos2(x, clip_rect.min.y),
            egui::vec2(separator_w, bar_h),
        );
        // separator는 알파가 이미 곱해진 색이므로 premultiplied로 읽는다.
        painter.rect_filled(sep, 0.0, th.tab_separator().to_egui_premultiplied());
        x += separator_w;
    }

    let pending = info.draft_tabs.get(i).copied().flatten().is_some();
    let is_active = i == info.active_tab;
    let tab_kind = info.tab_attention_kind.get(i).copied().flatten();
    let is_busy = info.tab_is_busy.get(i).copied().unwrap_or(false);
    // Fill 스타일만 활성 탭 배경을 채운다. Underline/Dot 은
    // 배경을 비활성과 동일하게 두고 별도 마커로 표시.
    let tab_id = egui::Id::new(("tab", info.pane_id, info.draft_pane, i));
    let fill_active =
        is_active && props.active_tab_indicator == crate::settings::ActiveTabIndicator::Fill;
    let (tab_bg, text_color) =
        tab_fill_and_ink(ui, th, tab_id, bg, is_active, fill_active, tab_kind);

    let tab_rect =
        egui::Rect::from_min_size(egui::pos2(x, clip_rect.min.y), egui::vec2(tab_w, bar_h));

    painter.rect_filled(tab_rect, 0.0, tab_bg);

    if is_active {
        use crate::settings::ActiveTabIndicator;
        match props.active_tab_indicator {
            ActiveTabIndicator::Underline => {
                let line_rect = egui::Rect::from_min_size(
                    egui::pos2(tab_rect.min.x, tab_rect.min.y),
                    egui::vec2(tab_w, active_indicator_h),
                );
                painter.rect_filled(line_rect, 0.0, th.tab_indicator());
            }
            ActiveTabIndicator::Fill => {}
            ActiveTabIndicator::Dot => {
                let r = zoomed_px(th, TAB_ACTIVE_DOT_SIZE).value() * 0.5;
                let center = egui::pos2(tab_rect.center().x, tab_rect.min.y + r * 2.0);
                painter.circle_filled(center, r, th.tab_indicator());
            }
        }
    }

    let marker = info.tab_html_script_marker.get(i).copied().flatten();
    let cluster = status_cluster(
        LogicalPx(tab_rect.max.x) - th.spacing_xs,
        ClusterSizes::of(th),
        ClusterItems {
            marker: marker.is_some(),
            move_glyph: info.move_mark == Some((i, super::TabMoveMark::Glyph)),
            busy: is_busy,
        },
    );
    let cy = tab_rect.center().y;
    let slot_rect = |x: egui::Rangef| {
        egui::Rect::from_x_y_ranges(
            x,
            egui::Rangef::new(cy - x.span() / 2.0, cy + x.span() / 2.0),
        )
    };
    if let Some(x) = cluster.busy {
        let color: egui::Color32 = th.accent_success_raised().into();
        painter.circle_filled(egui::pos2(x.center(), cy), x.span() / 2.0, color);
    }

    let icon_size = 14.0;
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            tab_rect.min.x + h_padding,
            tab_rect.center().y - icon_size / 2.0,
        ),
        egui::vec2(icon_size, icon_size),
    );
    // 포커스된 pane에서 슬롯 키가 있는 탭만 아이콘 대신 키캡을 표시한다. 슬롯 폭은 유지한다.
    let switch_digit = crate::adapters::ui::switch_overlay::tab_keycap_for(
        props.kb,
        props.switch_overlay_pane,
        info.pane_id,
        i,
    );
    // 키캡이 없는 프레임에도 페이드 상태를 갱신한다.
    let overlay_active = props.switch_overlay_pane == Some(info.pane_id);
    let fade = crate::adapters::ui::switch_overlay::appear_fade(
        ui.ctx(),
        th,
        info.pane_id,
        overlay_active,
    );
    if switch_digit.is_none() {
        let icon = info.tab_icons.get(i).copied().unwrap_or(icons::FILE);
        // 아이콘이 화살표·버튼 위로 넘치지 않게 그리는 동안 clip을 좁힌다.
        let prev_clip = ui.clip_rect();
        ui.set_clip_rect(clip_rect.intersect(prev_clip));
        icon.image(icon_size, th.text_secondary().into())
            .paint_at(ui, icon_rect);
        ui.set_clip_rect(prev_clip);
    }

    let loading = info.loading.get(i).copied().unwrap_or(false);
    if loading {
        let previous_clip = ui.clip_rect();
        ui.set_clip_rect(clip_rect.intersect(previous_clip));
        tasty_ui_widgets::paint_loading_overlay(
            ui,
            th,
            icon_rect,
            tasty_ui_widgets::LoadingPlacement::Tab,
            tab_bg.to_egui(),
        );
        ui.set_clip_rect(previous_clip);
    }
    // 전환 키캡은 같은 칸의 스피너 위에 그린다.
    if let Some(digit) = switch_digit {
        crate::adapters::ui::switch_overlay::paint_keycap(
            painter,
            th,
            icon_rect.center(),
            digit,
            is_active,
            fade,
        );
    }
    let text_x = icon_rect.max.x + 6.0;
    if let Some(x) = cluster.move_glyph {
        paint_move_glyph(ui, context, slot_rect(x));
    }
    let marker_slot = marker.zip(cluster.marker).map(|(m, x)| (slot_rect(x), m));
    let available_w = (cluster.label_right.value() - text_x).max(0.0);
    let font_id = egui::FontId::proportional(label_font_size);
    let final_galley = layout_tab_label(painter, name, font_id, text_color, LogicalPx(available_w));
    let text_y = tab_rect.center().y - final_galley.size().y / 2.0;
    painter.galley(egui::pos2(text_x, text_y), final_galley, text_color.into());

    let tab_clip = tab_rect.intersect(clip_rect);
    if !tab_clip.is_negative() {
        let resp = ui.interact(
            tab_clip,
            tab_id,
            if pending {
                egui::Sense::click()
            } else {
                egui::Sense::click_and_drag()
            },
        );
        // close 버튼 (active or hover) — 우측 끝. 클릭은
        // SwitchTab 보다 우선.
        let marker_hovered = marker_slot.is_some_and(|(slot, _)| ui.rect_contains_pointer(slot));
        let marker_clicked =
            marker_slot.and_then(|slot| paint_html_script_marker(ui, context, i, tab_rect, slot));
        if loading && !marker_hovered {
            paint_loading_tooltip(ui, context, &resp, name, tab_rect);
        }
        // lock 칸은 click만 받아 click_and_drag인 탭 응답의 hover를 빼앗지 않는다. lock hover에도 close가 보인다.
        let show_close = !pending && (is_active || resp.hovered());
        let close_clicked = if show_close {
            let close_rect = slot_rect(cluster.close);
            // 닫기 칸의 입력과 아이콘을 뷰포트 안으로 자른다. 스크롤로 가려진 탭의 닫기 칸이
            // 스크롤 화살표 밑에 놓여도 화살표 클릭을 가로채지 않는다.
            let prev_clip = ui.clip_rect();
            ui.set_clip_rect(clip_rect.intersect(prev_clip));
            let cr = ui.interact(
                close_rect,
                egui::Id::new(("tabclose", info.pane_id, i)),
                egui::Sense::click(),
            );
            if cr.hovered() {
                painter.rect_filled(
                    close_rect,
                    th.tab_close_radius().value(),
                    th.active_overlay.to_egui_premultiplied(),
                );
            }
            let cc: egui::Color32 = if cr.hovered() {
                th.text_primary().into()
            } else {
                th.text_secondary().into()
            };
            let glyph = th.icon_glyph_size_xs.value();
            icons::CLOSE.image(glyph, cc).paint_at(
                ui,
                egui::Rect::from_center_size(close_rect.center(), egui::vec2(glyph, glyph)),
            );
            ui.set_clip_rect(prev_clip);
            cr.clicked()
        } else {
            false
        };
        output.actions.extend(primary_click_action(
            info.pane_id,
            i,
            close_clicked,
            marker_clicked,
            resp.clicked(),
        ));
        if !pending && resp.secondary_clicked() {
            output.actions.push(TabBarAction::OpenContextMenu {
                pane_id: info.pane_id,
                tab_index: i,
                pos: resp.interact_pointer_pos().unwrap_or_default(),
            });
            painter.rect_stroke(
                tab_clip,
                0.0,
                egui::Stroke::new(th.focus_ring_width.value(), th.accent_success()),
                egui::StrokeKind::Inside,
            );
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            output.actions.push(TabBarAction::DragStart {
                pane_id: info.pane_id,
                tab_index: i,
            });
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = resp.interact_pointer_pos()
        {
            output.actions.push(TabBarAction::DragUpdate {
                pane_id: info.pane_id,
                mouse_x: pos.x,
            });
        }
        if resp.drag_stopped_by(egui::PointerButton::Primary) {
            output.actions.push(TabBarAction::DragEnd {
                pane_id: info.pane_id,
            });
        }
    }

    x += tab_w;
    LogicalPx(x)
}

/// 대상 서피스를 담은 비활성 탭의 묶음 칸에 move 글리프를 그린다.
fn paint_move_glyph(ui: &mut egui::Ui, context: &TabRenderContext<'_, '_>, slot: egui::Rect) {
    let prev_clip = ui.clip_rect();
    ui.set_clip_rect(context.clip_rect.intersect(prev_clip));
    tasty_ui_widgets::paint_move_source_glyph(ui, context.props.theme, slot);
    ui.set_clip_rect(prev_clip);
}

/// 탭 채움과 제목 색. 제목 우선순위는 NeedsInput > Completion > 활성 탭 > hover > 기본이다.
/// 응답은 라벨 뒤에 만들므로 지난 프레임의 hover를 읽는다.
fn tab_fill_and_ink(
    ui: &egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    tab_id: egui::Id,
    bg: tasty_type_appearance::color::HexColor,
    is_active: bool,
    fill_active: bool,
    kind: Option<crate::core::AttentionKind>,
) -> (
    tasty_type_appearance::color::HexColor,
    tasty_type_appearance::color::HexColor,
) {
    let hovered = !is_active
        && ui
            .ctx()
            .read_response(tab_id)
            .is_some_and(|response| response.hovered());
    let fill = match (fill_active, hovered) {
        (true, _) => th.bg_panel(),
        (false, true) => th.tab_bg_hover(),
        (false, false) => bg,
    };
    let ink = match kind {
        None if hovered => th.tab_fg_hover(),
        kind => tasty_ui_widgets::tab_title_color(th, kind.map(Into::into), is_active),
    };
    (fill, ink)
}

/// 로딩 중인 탭에 hover하면 제목 아래 둘째 줄로 로딩 문구를 보인다. 오버레이 자체에는 툴팁이 없다.
/// 탭 로딩은 attach 응답 대기에서만 켜지므로 `loading.remote_attach`를 쓴다.
fn paint_loading_tooltip(
    ui: &egui::Ui,
    context: &TabRenderContext<'_, '_>,
    resp: &egui::Response,
    name: &str,
    cell: egui::Rect,
) {
    let th = context.props.theme;
    if !tasty_ui_widgets::tooltip_hover_delay_elapsed(ui.ctx(), th, resp.id, resp.hovered()) {
        return;
    }
    let window = ui.ctx().screen_rect();
    tasty_ui_widgets::Tooltip::new(name)
        .second_line(crate::i18n::t("loading.remote_attach"))
        .id_source(("tab_loading_tooltip", resp.id))
        .placement_clear_of_native(
            ui.ctx(),
            th,
            cell,
            Some(cell),
            window,
            context.props.native_content,
        )
        .show(ui, th, cell);
}

/// 탭 한 칸의 왼쪽 클릭을 동작 하나로 정한다. 닫기, 스크립트 표지, 탭 전환 순으로 우선한다.
fn primary_click_action(
    pane_id: u32,
    tab_index: usize,
    close_clicked: bool,
    marker_clicked: Option<u32>,
    tab_clicked: bool,
) -> Option<TabBarAction> {
    if close_clicked {
        Some(TabBarAction::CloseTab { pane_id, tab_index })
    } else if let Some(surface_id) = marker_clicked {
        Some(TabBarAction::ShowHtmlScriptBanner { surface_id })
    } else if tab_clicked {
        Some(TabBarAction::SwitchTab { pane_id, tab_index })
    } else {
        None
    }
}

/// 탭 칸 오른쪽 묶음에 둘 항목.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ClusterItems {
    pub marker: bool,
    pub move_glyph: bool,
    pub busy: bool,
}

/// 묶음 항목의 폭과 간격.
#[derive(Clone, Copy, Debug)]
pub(super) struct ClusterSizes {
    pub close: LogicalPx,
    pub busy: LogicalPx,
    pub move_glyph: LogicalPx,
    pub marker: LogicalPx,
    /// 묶음 항목 사이 간격.
    pub gap: LogicalPx,
    /// 제목 오른쪽 끝과 묶음 사이 간격.
    pub label_gap: LogicalPx,
}

impl ClusterSizes {
    pub(super) fn of(th: &tasty_type_appearance::theme::Theme) -> Self {
        Self {
            close: th.tab_close_size(),
            busy: th.tab_dot_size(),
            move_glyph: LogicalPx(tasty_ui_widgets::move_source_glyph_size(th)),
            marker: th.html_script_marker_hit(),
            gap: th.tab_status_gap(),
            label_gap: th.tab_gap(),
        }
    }
}

/// 묶음 항목이 차지하는 x 범위와 제목에 남는 오른쪽 끝.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ClusterSlots {
    pub close: egui::Rangef,
    pub busy: Option<egui::Rangef>,
    pub move_glyph: Option<egui::Rangef>,
    pub marker: Option<egui::Rangef>,
    pub label_right: LogicalPx,
}

/// 탭 칸 오른쪽 끝 `right`에서 왼쪽으로 close · busy · move · 표지 순서로 칸을 잡는다.
/// close 칸은 보이지 않을 때도 자리를 지키고, 묶음은 제목 길이와 관계없이 줄지 않는다.
pub(super) fn status_cluster(
    right: LogicalPx,
    sizes: ClusterSizes,
    items: ClusterItems,
) -> ClusterSlots {
    let gap = sizes.gap.value();
    let mut edge = right.value();
    let mut take = |w: LogicalPx| {
        let range = egui::Rangef::new(edge - w.value(), edge);
        edge -= w.value() + gap;
        range
    };
    let close = take(sizes.close);
    let busy = items.busy.then(|| take(sizes.busy));
    let move_glyph = items.move_glyph.then(|| take(sizes.move_glyph));
    let marker = items.marker.then(|| take(sizes.marker));
    // 마지막 칸 뒤에 더한 항목 간격을 되돌린 곳이 묶음의 왼쪽 끝이다.
    let cluster_left = edge + gap;
    ClusterSlots {
        close,
        busy,
        move_glyph,
        marker,
        label_right: LogicalPx(cluster_left) - sizes.label_gap,
    }
}

/// 표지를 그리고, 사용자가 lock 을 눌렀으면 그 surface id 를 돌려준다.
/// `cell`은 표지가 든 탭 칸이며 툴팁의 스트립 안 후보 기준이다.
fn paint_html_script_marker(
    ui: &mut egui::Ui,
    context: &TabRenderContext<'_, '_>,
    i: usize,
    cell: egui::Rect,
    (slot, marker): (egui::Rect, super::TabScriptMarker),
) -> Option<u32> {
    use tasty_ui_widgets::HtmlScriptMarkerKind;
    let tooltip = crate::i18n::t(match marker.kind {
        HtmlScriptMarkerKind::Blocked => "banner.html_script.marker_blocked",
        HtmlScriptMarkerKind::Allowed if marker.web => "banner.html_script.marker_allowed_web",
        HtmlScriptMarkerKind::Allowed => "banner.html_script.marker_allowed",
    });
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(slot).id_salt((
        "html_script_marker",
        context.info.pane_id,
        i,
    )));
    child.set_clip_rect(context.clip_rect.intersect(ui.clip_rect()));
    let resp = tasty_ui_widgets::html_script_marker(
        &mut child,
        context.props.theme,
        marker.kind,
        tooltip,
        Some(cell),
        context.props.native_content,
    );
    resp.clicked().then_some(marker.surface_id)
}

/// Fit the label into the space left by the leading icon and the right-hand status cluster.
fn layout_tab_label(
    painter: &egui::Painter,
    name: &str,
    font_id: egui::FontId,
    text_color: tasty_type_appearance::color::HexColor,
    available_width: LogicalPx,
) -> std::sync::Arc<egui::Galley> {
    let available_w = available_width.value();
    let galley = painter.layout_no_wrap(name.to_owned(), font_id.clone(), text_color.into());
    if galley.size().x > available_w {
        let mut truncated = name.to_owned();
        loop {
            truncated.pop();
            let candidate = format!("{truncated}…");
            let g = painter.layout_no_wrap(candidate.clone(), font_id.clone(), text_color.into());
            if g.size().x <= available_w || truncated.is_empty() {
                break g;
            }
        }
    } else {
        galley
    }
}
