//! 앵커 주변에 도움말 버블을 그린다. 위치는 위·아래·왼쪽·오른쪽 중에서 선택한다.
//! 호버와 대기 시간은 호출자가 판단하며 egui 전역 도움말 설정은 바꾸지 않는다.

use tasty_type_appearance::theme::Theme;

/// 버블이 앵커의 어느 쪽에 뜨는지 — 앵커 rect 중앙 기준.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TooltipPlacement {
    /// 앵커 위(기본).
    #[default]
    Top,
    /// 앵커 아래.
    Bottom,
    /// 앵커 왼쪽.
    Left,
    /// 앵커 오른쪽.
    Right,
}

/// Tooltip 버블 빌더.
pub struct Tooltip<'a> {
    text: &'a str,
    /// 본문 아래 muted 둘째 줄. 로딩 중인 탭·카드·아바타의 상태 문구 등.
    second_line: Option<&'a str>,
    placement: TooltipPlacement,
    /// 버블 `Area` 의 고유 id — 한 프레임에 여러 버블(specimen 4 placement)을 그릴 때
    /// 충돌을 막는다. 기본값은 단일 사용을 가정한 고정 id.
    id: egui::Id,
    /// [`Tooltip::placement_clear_of_native`]가 정한 버블 사각형. 있으면 그 자리에 그린다.
    resolved: Option<egui::Rect>,
    /// 본문을 mono 로 그린다. 경로 같은 값을 보일 때 쓴다.
    mono: bool,
}

impl<'a> Tooltip<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            second_line: None,
            placement: TooltipPlacement::default(),
            id: egui::Id::new("tasty_tooltip"),
            resolved: None,
            mono: false,
        }
    }

    /// 본문 아래에 `text-muted` 둘째 줄을 더한다. 줄 높이·글꼴은 본문과 같다.
    pub fn second_line(mut self, line: &'a str) -> Self {
        self.second_line = Some(line);
        self
    }

    /// 본문을 mono 로 그린다.
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }

    /// 앵커 기준 배치.
    pub fn placement(mut self, placement: TooltipPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// 버블 `Area` id 의 출처 — 한 페이지에 여러 버블을 동시에 그릴 때 고유화한다.
    pub fn id_source(mut self, source: impl std::hash::Hash) -> Self {
        self.id = egui::Id::new(source);
        self
    }

    /// 위에 들어가면 위, 아니면 아래에 들어가면 아래, 둘 다 안 되면 위(창 안으로 당김)로 배치한다.
    /// `bounds`는 버블이 들어가야 하는 영역(보통 창 전체)이다.
    pub fn placement_top_then_bottom(
        self,
        ctx: &egui::Context,
        theme: &Theme,
        anchor: egui::Rect,
        bounds: egui::Rect,
    ) -> Self {
        let offset = theme.spacing_xs.value();
        let height = self.bubble_height(ctx, theme);
        let placement = if anchor.top() - offset - height >= bounds.top()
            || anchor.bottom() + offset + height > bounds.bottom()
        {
            TooltipPlacement::Top
        } else {
            TooltipPlacement::Bottom
        };
        self.placement(placement)
    }

    /// pane 탭 스트립·pane 머리에 붙은 툴팁의 배치 규칙이다.
    /// 위 → 아래 → 스트립 안 순으로, `window` 안에 들어가고 `native`의 어느 사각형과도 겹치지 않는
    /// 첫 후보를 쓴다. `native`는 egui보다 위에 그려지는 네이티브 콘텐츠(WebView) 영역이며 호출자가 넘긴다.
    /// `cell`은 앵커가 속한 스트립 칸이고 세로 범위가 스트립 행이다. 있으면 "스트립 안" 후보를 시도한다.
    /// 그 후보는 행 세로 가운데, 칸 오른쪽(다음 왼쪽)에 `tooltip-offset`만큼 띄운다. 이웃 탭을 덮을 수 있다.
    /// 스트립 안 후보는 네이티브 영역과 변마다 `border-width`까지 겹쳐도 통과한다. 한 줄 버블(25)이 24 행보다
    /// 1px 남짓 크기 때문이다.
    /// 모두 안 되면 `cell`이 있으면 스트립 안 배치(오른쪽, 창을 벗어나면 왼쪽, 둘 다 벗어나면 가로를 창 안으로
    /// 당긴 오른쪽)를 쓴다. `cell`이 없으면 위에 두고 창 안으로 당기며, 세로도 창 가장자리에서 `tooltip-offset`만큼 띄운다.
    /// 디자인의 최후 배치 "스트립 안"과 다른 이 동작은 놓을 행이 없는 호출에만 남긴 의도된 차이이며, 버블이
    /// WebView에 일부 가려질 수 있다.
    /// 가로는 위·아래 후보와 위 최후 배치 모두 창 가장자리에서 `tooltip-offset`만큼 안쪽으로 당긴다.
    pub fn placement_clear_of_native(
        self,
        ctx: &egui::Context,
        theme: &Theme,
        anchor: egui::Rect,
        cell: Option<egui::Rect>,
        window: egui::Rect,
        native: &[egui::Rect],
    ) -> Self {
        let size = self.bubble_size(ctx, theme);
        let (placement, rect) = native_clear_rect(anchor, cell, size, theme, window, native);
        Self {
            placement,
            resolved: Some(rect),
            ..self
        }
    }

    /// 버블의 왼쪽 위 모서리를 `min`에 둔다. 창 밖으로 나가면 egui가 안으로 당긴다.
    pub fn at(self, min: egui::Pos2) -> Self {
        Self {
            resolved: Some(egui::Rect::from_min_size(min, egui::Vec2::ZERO)),
            ..self
        }
    }

    /// `anchor` rect 를 기준으로 버블을 그린다(강제 표시). hover/delay 판정은 호출부 몫.
    pub fn show(self, ui: &egui::Ui, theme: &Theme, anchor: egui::Rect) {
        self.show_in(ui.ctx(), theme, anchor);
    }

    /// `Ui` 없이 painter만 쓰는 호출부용 [`Tooltip::show`].
    pub fn show_in(self, ctx: &egui::Context, theme: &Theme, anchor: egui::Rect) {
        if let Some(rect) = self.resolved {
            self.paint(ctx, theme, rect.min, egui::Align2::LEFT_TOP);
            return;
        }
        let offset = theme.spacing_xs.value();
        // 앵커 rect 중앙 기준 앵커 포인트 + 버블 pivot(버블에서 앵커에 붙는 변).
        let (anchor_pos, pivot) = match self.placement {
            TooltipPlacement::Top => (
                egui::pos2(anchor.center().x, anchor.top() - offset),
                egui::Align2::CENTER_BOTTOM,
            ),
            TooltipPlacement::Bottom => (
                egui::pos2(anchor.center().x, anchor.bottom() + offset),
                egui::Align2::CENTER_TOP,
            ),
            TooltipPlacement::Left => (
                egui::pos2(anchor.left() - offset, anchor.center().y),
                egui::Align2::RIGHT_CENTER,
            ),
            TooltipPlacement::Right => (
                egui::pos2(anchor.right() + offset, anchor.center().y),
                egui::Align2::LEFT_CENTER,
            ),
        };

        self.paint(ctx, theme, anchor_pos, pivot);
    }

    fn paint(self, ctx: &egui::Context, theme: &Theme, pos: egui::Pos2, pivot: egui::Align2) {
        let pad_x = theme.spacing_sm.value();
        let pad_y = theme.spacing_xs.value();
        let job = self.layout_job(theme);

        egui::Area::new(self.id)
            .order(egui::Order::Tooltip)
            .fixed_pos(pos)
            .pivot(pivot)
            .constrain(true) // 화면/모달 밖으로 나가면 egui 기본 constrain 이 안으로 당김.
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme.surface_raised().to_egui())
                    .stroke(egui::Stroke::new(
                        theme.border_width.value(),
                        theme.border_strong().to_egui(),
                    ))
                    .corner_radius(theme.corner_radius.value())
                    .shadow(theme.shadow_popover().to_egui())
                    .inner_margin(egui::Margin::symmetric(pad_x as i8, pad_y as i8))
                    .show(ui, |ui| {
                        ui.set_max_width(theme.tooltip_max_width.value());
                        ui.label(job);
                    });
            });
    }

    fn layout_job(&self, theme: &Theme) -> egui::text::LayoutJob {
        // 텍스트는 border-box 240 을 넘지 않도록 padding(x=space-sm ×2)을 뺀 폭에서 wrap.
        let pad_x = theme.spacing_sm.value();
        let text_wrap = (theme.tooltip_max_width.value() - pad_x * 2.0).max(0.0);

        let caption = theme.font_size_caption.value();
        let mut job = egui::text::LayoutJob {
            halign: egui::Align::LEFT,
            ..Default::default()
        };
        job.wrap.max_width = text_wrap;
        job.append(
            self.text,
            0.0,
            egui::TextFormat {
                font_id: if self.mono {
                    egui::FontId::monospace(caption)
                } else {
                    egui::FontId::proportional(caption)
                },
                color: theme.tooltip_fg().to_egui(),
                // 줄 높이는 caption 글꼴 크기에 line_height_ui를 곱한다.
                line_height: Some(caption * theme.line_height_ui),
                ..Default::default()
            },
        );
        if let Some(line) = self.second_line {
            job.append(
                &format!("\n{line}"),
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::proportional(caption),
                    color: theme.text_muted_raised().to_egui(),
                    line_height: Some(caption * theme.line_height_ui),
                    ..Default::default()
                },
            );
        }
        job
    }

    /// 버블 높이 — 텍스트 + 위아래 padding + 위아래 테두리.
    fn bubble_height(&self, ctx: &egui::Context, theme: &Theme) -> f32 {
        self.bubble_size(ctx, theme).y
    }

    /// 버블 크기 — 텍스트 + padding + 테두리.
    fn bubble_size(&self, ctx: &egui::Context, theme: &Theme) -> egui::Vec2 {
        let text = ctx.fonts(|f| f.layout_job(self.layout_job(theme)).size());
        let border = theme.border_width.scaled(2.0).value();
        egui::vec2(
            text.x + theme.spacing_sm.scaled(2.0).value() + border,
            text.y + theme.spacing_xs.scaled(2.0).value() + border,
        )
    }
}

/// 위·아래 후보 버블의 사각형. 가로는 창 가장자리에서 `tooltip-offset`만큼 안쪽으로 당긴다.
fn bubble_rect(
    placement: TooltipPlacement,
    anchor: egui::Rect,
    size: egui::Vec2,
    theme: &Theme,
    window: egui::Rect,
) -> egui::Rect {
    let offset = theme.tooltip_offset().value();
    let top = match placement {
        TooltipPlacement::Bottom => anchor.bottom() + offset,
        TooltipPlacement::Top | TooltipPlacement::Left | TooltipPlacement::Right => {
            anchor.top() - offset - size.y
        }
    };
    let left = (anchor.center().x - size.x / 2.0)
        .min(window.right() - offset - size.x)
        .max(window.left() + offset);
    egui::Rect::from_min_size(egui::pos2(left, top), size)
}

/// [`Tooltip::placement_clear_of_native`]의 후보를 차례로 시험해 배치와 버블 사각형을 정한다.
fn native_clear_rect(
    anchor: egui::Rect,
    cell: Option<egui::Rect>,
    size: egui::Vec2,
    theme: &Theme,
    window: egui::Rect,
    native: &[egui::Rect],
) -> (TooltipPlacement, egui::Rect) {
    let offset = theme.tooltip_offset().value();
    let tolerance = theme.border_width.value();
    // `tolerance`만큼 줄인 사각형으로 네이티브 영역과 겹치는지 본다. 위·아래 후보는 0이다.
    let clears = |rect: &egui::Rect, tolerance: f32| {
        window.contains_rect(*rect)
            && native
                .iter()
                .all(|n| !rect.shrink(tolerance).intersect(*n).is_positive())
    };
    let vertical = [TooltipPlacement::Top, TooltipPlacement::Bottom]
        .map(|p| (p, bubble_rect(p, anchor, size, theme, window)));
    let in_strip = cell.map(|cell| {
        let top = cell.center().y - size.y / 2.0;
        [
            (
                TooltipPlacement::Right,
                egui::Rect::from_min_size(egui::pos2(cell.right() + offset, top), size),
            ),
            (
                TooltipPlacement::Left,
                egui::Rect::from_min_size(egui::pos2(cell.left() - offset - size.x, top), size),
            ),
        ]
    });
    if let Some(found) = vertical.into_iter().find(|(_, rect)| clears(rect, 0.0)) {
        return found;
    }
    if let Some(strip) = in_strip {
        if let Some(found) = strip.into_iter().find(|(_, rect)| clears(rect, tolerance)) {
            return found;
        }
        // 최후 배치도 스트립 안이다. Tasty가 그리는 행이라 버블이 보인다.
        return strip
            .into_iter()
            .find(|(_, rect)| rect.left() >= window.left() && rect.right() <= window.right())
            .unwrap_or_else(|| {
                let (_, right) = strip[0];
                let left = right
                    .left()
                    .min(window.right() - offset - size.x)
                    .max(window.left() + offset);
                (
                    TooltipPlacement::Right,
                    egui::Rect::from_min_size(egui::pos2(left, right.top()), size),
                )
            });
    }
    // 칸 없는 호출: 놓을 스트립 행이 없어 디자인의 스트립 안 최후 배치 대신 위 clamp를 쓴다.
    let rect = bubble_rect(TooltipPlacement::Top, anchor, size, theme, window);
    let top = rect
        .top()
        .min(window.bottom() - offset - size.y)
        .max(window.top() + offset);
    (
        TooltipPlacement::Top,
        egui::Rect::from_min_size(egui::pos2(rect.left(), top), size),
    )
}

/// `hovered`가 이어진 시간이 `tooltip-delay`를 넘었는지 판정한다. 벗어나면 초기화한다.
/// 다른 egui 도움말의 대기 시간을 바꾸지 않도록 `id`별 자체 타이머를 사용한다.
pub fn tooltip_hover_delay_elapsed(
    ctx: &egui::Context,
    theme: &Theme,
    id: egui::Id,
    hovered: bool,
) -> bool {
    let key = id.with("tooltip_hover_started_at");
    if hovered {
        let now = ctx.input(|i| i.time);
        let start = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(key, || now));
        if now - start < theme.tooltip_delay().to_secs_f64() {
            // delay 경과 후 자동으로 다시 판정되도록 repaint 예약.
            ctx.request_repaint();
            false
        } else {
            true
        }
    } else {
        ctx.data_mut(|d| d.remove::<f64>(key));
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    /// 창 안의 한 줄 앵커가 받는 배치.
    fn placement_for(anchor_top: f32) -> TooltipPlacement {
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let anchor =
            egui::Rect::from_min_size(egui::pos2(100.0, anchor_top), egui::vec2(200.0, 28.0));
        let ctx = egui::Context::default();
        let mut placement = TooltipPlacement::Left;
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            placement = Tooltip::new("Surface Type")
                .placement_top_then_bottom(ctx, &theme(), anchor, bounds)
                .placement;
        });
        placement
    }

    #[test]
    fn top_is_used_when_the_bubble_fits_above() {
        assert_eq!(placement_for(300.0), TooltipPlacement::Top);
    }

    #[test]
    fn bottom_is_used_when_the_bubble_does_not_fit_above() {
        assert_eq!(placement_for(0.0), TooltipPlacement::Bottom);
    }

    /// 800×600 창에서 `anchor_top`의 16×16 앵커와 `native` 영역으로 고른 배치.
    fn native_placement_for(anchor_top: f32, native: &[egui::Rect]) -> TooltipPlacement {
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let anchor =
            egui::Rect::from_min_size(egui::pos2(100.0, anchor_top), egui::vec2(16.0, 16.0));
        let ctx = egui::Context::default();
        let mut placement = TooltipPlacement::Left;
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            placement = Tooltip::new("Scripts blocked. Click to show the notice again.")
                .placement_clear_of_native(ctx, &theme(), anchor, None, window, native)
                .placement;
        });
        placement
    }

    /// 앵커 바로 아래(탭 바 아래)부터 시작하는 WebView 영역.
    fn webview_below(anchor_top: f32) -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(0.0, anchor_top + 20.0), egui::pos2(800.0, 600.0))
    }

    #[test]
    fn native_rule_opens_top_over_a_webview_below() {
        assert_eq!(
            native_placement_for(60.0, &[webview_below(60.0)]),
            TooltipPlacement::Top
        );
    }

    #[test]
    fn native_rule_falls_back_to_bottom_when_top_leaves_the_window() {
        assert_eq!(native_placement_for(2.0, &[]), TooltipPlacement::Bottom);
    }

    #[test]
    fn native_rule_opens_bottom_when_a_webview_is_above() {
        let above = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(800.0, 290.0));
        assert_eq!(
            native_placement_for(300.0, &[above]),
            TooltipPlacement::Bottom
        );
    }

    #[test]
    fn native_rule_keeps_top_when_neither_side_clears() {
        let above = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(800.0, 290.0));
        assert_eq!(
            native_placement_for(300.0, &[above, webview_below(300.0)]),
            TooltipPlacement::Top
        );
        assert_eq!(
            native_placement_for(2.0, &[webview_below(2.0)]),
            TooltipPlacement::Top
        );
    }

    const WINDOW: egui::Rect = egui::Rect {
        min: egui::Pos2::ZERO,
        max: egui::pos2(800.0, 600.0),
    };

    /// 창 안의 24px 스트립 행에 놓인 탭 칸과 그 행 위·아래를 덮는 WebView 두 개.
    fn stacked(cell_left: f32, strip_top: f32) -> (egui::Rect, [egui::Rect; 2]) {
        let cell =
            egui::Rect::from_min_size(egui::pos2(cell_left, strip_top), egui::vec2(140.0, 24.0));
        let above = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(800.0, cell.top()));
        let below = egui::Rect::from_min_max(egui::pos2(0.0, cell.bottom()), WINDOW.max);
        (cell, [above, below])
    }

    /// 칸 오른쪽 끝의 16×16 표지.
    fn marker_in(cell: egui::Rect) -> egui::Rect {
        egui::Rect::from_min_size(
            egui::pos2(cell.right() - 20.0, cell.center().y - 8.0),
            egui::vec2(16.0, 16.0),
        )
    }

    #[test]
    fn stacked_webviews_put_the_bubble_inside_the_strip_right_of_the_cell() {
        let theme = theme();
        let offset = theme.tooltip_offset().value();
        let (cell, native) = stacked(100.0, 300.0);
        let size = egui::vec2(120.0, 22.0);
        let (placement, rect) =
            native_clear_rect(marker_in(cell), Some(cell), size, &theme, WINDOW, &native);
        assert_eq!(placement, TooltipPlacement::Right);
        assert_eq!(rect.left(), cell.right() + offset);
        assert_eq!(rect.center().y, cell.center().y);
    }

    #[test]
    fn the_strip_candidate_moves_left_when_the_right_side_leaves_the_window() {
        let theme = theme();
        let offset = theme.tooltip_offset().value();
        let (cell, native) = stacked(660.0, 300.0);
        let size = egui::vec2(120.0, 22.0);
        let (placement, rect) =
            native_clear_rect(marker_in(cell), Some(cell), size, &theme, WINDOW, &native);
        assert_eq!(placement, TooltipPlacement::Left);
        assert_eq!(rect.right(), cell.left() - offset);
        assert_eq!(rect.center().y, cell.center().y);
    }

    #[test]
    fn without_a_cell_the_strip_candidate_is_not_tried() {
        let (cell, native) = stacked(100.0, 300.0);
        let (placement, _) = native_clear_rect(
            marker_in(cell),
            None,
            egui::vec2(120.0, 22.0),
            &theme(),
            WINDOW,
            &native,
        );
        assert_eq!(placement, TooltipPlacement::Top);
    }

    /// 위·아래 WebView가 스트립 행에 바로 붙어 있어도 한 줄 버블(25 남짓)은 변마다 `border-width`까지의 겹침이
    /// 허용돼 스트립 안 후보로 통과한다. 오른쪽 이웃 pane의 WebView가 오른쪽 후보를 막으면 왼쪽 후보가 통과한다.
    /// 둘째 줄은 본문과 같은 글꼴·줄 높이로 한 줄 아래에 놓이고 `text-muted`로 칠한다.
    #[test]
    fn the_second_line_sits_below_in_muted_ink() {
        let theme = theme();
        let one = Tooltip::new("build-server");
        let two = Tooltip::new("build-server").second_line("Attaching…");
        let job = two.layout_job(&theme);
        assert_eq!(job.sections.len(), 2);
        assert_eq!(job.sections[0].format.color, theme.tooltip_fg().to_egui());
        assert_eq!(
            job.sections[1].format.color,
            theme.text_muted_raised().to_egui()
        );
        assert_eq!(
            &job.text[job.sections[1].byte_range.clone()],
            "\nAttaching…"
        );
        let ctx = egui::Context::default();
        let mut sizes = None;
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            sizes = Some((one.bubble_size(ctx, &theme), two.bubble_size(ctx, &theme)));
        });
        let (one, two) = sizes.expect("sized");
        let line = theme.font_size_caption.value() * theme.line_height_ui;
        assert!((two.y - one.y - line).abs() < 1.0, "{one:?} {two:?}");
    }

    #[test]
    fn a_one_line_bubble_passes_the_strip_row_within_the_border_width() {
        let theme = theme();
        let offset = theme.tooltip_offset().value();
        let (cell, [above, below]) = stacked(300.0, 300.0);
        let neighbour =
            egui::Rect::from_min_max(egui::pos2(cell.right() + offset + 10.0, 0.0), WINDOW.max);
        let size = egui::vec2(120.0, 25.4);
        let (placement, rect) = native_clear_rect(
            marker_in(cell),
            Some(cell),
            size,
            &theme,
            WINDOW,
            &[above, below, neighbour],
        );
        assert_eq!(placement, TooltipPlacement::Left);
        assert_eq!(rect.right(), cell.left() - offset);
        assert_eq!(rect.center().y, cell.center().y);
    }

    /// 앱 기하: html pane 위에 html pane이 쌓이고 두 WebView가 스트립 행 위·아래에 간격 없이 붙는다.
    /// 실제 표지 문안의 한 줄 버블은 스트립 행보다 크지만 스트립 안 오른쪽에 놓인다. 허용치와 최후 배치가 같은
    /// 자리를 주므로 두 규칙의 구분은 위·아래 시험이 맡는다.
    #[test]
    fn app_geometry_marker_tooltip_opens_inside_the_strip() {
        let theme = theme();
        let offset = theme.tooltip_offset().value();
        let (cell, native) = stacked(100.0, 300.0);
        let ctx = egui::Context::default();
        let mut placed = None;
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            let t = Tooltip::new("Scripts blocked. Click to show the notice again.");
            let size = t.bubble_size(ctx, &theme);
            let t = t.placement_clear_of_native(
                ctx,
                &theme,
                marker_in(cell),
                Some(cell),
                WINDOW,
                &native,
            );
            placed = Some((t.placement, t.resolved, size));
        });
        let (placement, rect, size) = placed.expect("frame ran");
        let rect = rect.expect("resolved");
        assert!(
            size.y > cell.height(),
            "bubble {} vs strip {}",
            size.y,
            cell.height()
        );
        assert!(size.y <= cell.height() + 2.0 * theme.border_width.value());
        assert_eq!(placement, TooltipPlacement::Right);
        assert_eq!(rect.left(), cell.right() + offset);
        assert_eq!(rect.center().y, cell.center().y);
    }

    /// 겹침이 `border-width`를 넘으면 그 후보는 통과하지 않는다. 모두 막히면 최후 배치로 스트립 안 오른쪽을 쓴다.
    #[test]
    fn when_every_candidate_fails_the_bubble_stays_inside_the_strip() {
        let theme = theme();
        let offset = theme.tooltip_offset().value();
        // 창 맨 위 스트립: 위 후보는 창 밖, 아래·스트립 안 후보는 자기 WebView와 1px 넘게 겹친다.
        let (cell, [above, below]) = stacked(300.0, 2.0);
        let neighbour =
            egui::Rect::from_min_max(egui::pos2(cell.right() + offset + 10.0, 0.0), WINDOW.max);
        let size = egui::vec2(200.0, 40.0);
        let (placement, rect) = native_clear_rect(
            marker_in(cell),
            Some(cell),
            size,
            &theme,
            WINDOW,
            &[above, below, neighbour],
        );
        assert_eq!(placement, TooltipPlacement::Right);
        assert_eq!(rect.left(), cell.right() + offset);
        assert_eq!(rect.center().y, cell.center().y);
        assert_eq!(rect.size(), size);
    }

    /// 최후 배치에서 양옆이 모두 창을 벗어나면 오른쪽 배치를 창 안으로 당긴다.
    #[test]
    fn the_last_strip_placement_is_clamped_into_a_narrow_window() {
        let theme = theme();
        let offset = theme.tooltip_offset().value();
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 600.0));
        let cell = egui::Rect::from_min_size(egui::pos2(80.0, 300.0), egui::vec2(140.0, 24.0));
        let above = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(300.0, cell.top()));
        let below = egui::Rect::from_min_max(egui::pos2(0.0, cell.bottom()), window.max);
        let size = egui::vec2(200.0, 40.0);
        let (placement, rect) = native_clear_rect(
            marker_in(cell),
            Some(cell),
            size,
            &theme,
            window,
            &[above, below],
        );
        assert_eq!(placement, TooltipPlacement::Right);
        assert_eq!(rect.right(), window.right() - offset);
        assert_eq!(rect.center().y, cell.center().y);
    }

    /// 스트립 칸이 없으면 예전처럼 위에 두고 창 안으로 당기며 세로도 `tooltip-offset`만큼 띄운다.
    #[test]
    fn without_a_cell_the_last_resort_is_top_with_a_vertical_margin() {
        let theme = theme();
        let offset = theme.tooltip_offset().value();
        let (cell, [_, below]) = stacked(100.0, 2.0);
        let size = egui::vec2(200.0, 40.0);
        let (placement, rect) =
            native_clear_rect(marker_in(cell), None, size, &theme, WINDOW, &[below]);
        assert_eq!(placement, TooltipPlacement::Top);
        assert_eq!(rect.top(), WINDOW.top() + offset);
        assert_eq!(rect.size(), size);
    }

    #[test]
    fn a_bubble_taller_than_the_window_keeps_the_top_margin() {
        let theme = theme();
        let offset = theme.tooltip_offset().value();
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 30.0));
        let anchor = egui::Rect::from_min_size(egui::pos2(100.0, 8.0), egui::vec2(16.0, 16.0));
        let size = egui::vec2(200.0, 40.0);
        let (placement, rect) = native_clear_rect(anchor, None, size, &theme, window, &[]);
        assert_eq!(placement, TooltipPlacement::Top);
        // 창보다 큰 버블은 위 여백을 우선한다.
        assert_eq!(rect.top(), window.top() + offset);
    }

    #[test]
    fn bubble_is_clamped_to_the_window_edges_by_the_tooltip_offset() {
        let theme = theme();
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let size = egui::vec2(200.0, 24.0);
        let offset = theme.tooltip_offset().value();
        let at_right = egui::Rect::from_min_size(egui::pos2(790.0, 100.0), egui::vec2(8.0, 8.0));
        let rect = bubble_rect(TooltipPlacement::Top, at_right, size, &theme, window);
        assert_eq!(rect.right(), window.right() - offset);
        let at_left = egui::Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(8.0, 8.0));
        let rect = bubble_rect(TooltipPlacement::Bottom, at_left, size, &theme, window);
        assert_eq!(rect.left(), window.left() + offset);
        assert_eq!(rect.top(), at_left.bottom() + offset);
    }

    #[test]
    fn top_is_kept_when_neither_side_fits() {
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 30.0));
        let anchor = egui::Rect::from_min_size(egui::pos2(100.0, 1.0), egui::vec2(200.0, 28.0));
        let ctx = egui::Context::default();
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            let t = Tooltip::new("Surface Type").placement_top_then_bottom(
                ctx,
                &theme(),
                anchor,
                bounds,
            );
            assert_eq!(t.placement, TooltipPlacement::Top);
        });
    }
}
