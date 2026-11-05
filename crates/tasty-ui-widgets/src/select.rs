//! 단일 선택 드롭다운. 트리거는 Theme로 그리고 열린 항목 목록은 egui 팝업 안의 공용 옵션 행이다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::Ground;

/// 드롭다운. `selected` 는 `options` 인덱스. 선택이 바뀌면 `true`.
pub fn select(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    selected: &mut usize,
    options: &[&str],
    width: f32,
    enabled: bool,
) -> bool {
    select_rect(ui, theme, id_salt, selected, options, width, enabled).0
}

/// [`select`]를 `ground` 바탕 위에 그린다. surface-raised 카드 위에서는 `Ground::Raised`로
/// 상자·쉬는 테두리·disabled 상자가 select-raised 토큰이다.
// reason: `select` 인자에 바탕 하나를 더한 것이다 — 묶으면 두 드롭다운의 호출 모양이 갈린다.
#[allow(clippy::too_many_arguments)]
pub fn select_on(
    ui: &mut egui::Ui,
    theme: &Theme,
    ground: Ground,
    id_salt: &str,
    selected: &mut usize,
    options: &[&str],
    width: f32,
    enabled: bool,
) -> bool {
    let (picked, _) = select_impl(
        ui,
        theme,
        ground,
        id_salt,
        Some(*selected),
        options,
        None,
        width,
        enabled,
    );
    match picked {
        Some(i) => {
            *selected = i;
            true
        }
        None => false,
    }
}

/// [`select`] 와 같고 트리거가 차지한 rect 도 돌려준다. 행 안 컨트롤의 높이·위치를 맞추는 view 가 쓴다.
pub(crate) fn select_rect(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    selected: &mut usize,
    options: &[&str],
    width: f32,
    enabled: bool,
) -> (bool, egui::Rect) {
    let (picked, rect) = select_impl(
        ui,
        theme,
        Ground::Panel,
        id_salt,
        Some(*selected),
        options,
        None,
        width,
        enabled,
    );
    match picked {
        Some(i) => {
            *selected = i;
            (true, rect)
        }
        None => (false, rect),
    }
}

/// 미선택 상태에서는 트리거만 placeholder 색과 문구를 사용한다. placeholder 는 목록의 행이 아니다 —
/// 열린 목록은 실제 옵션만 담고 아무것도 체크하지 않는다. 값을 지울 수 있어야 하면 호출하는 쪽이
/// "None" 같은 실제 옵션을 둔다. 실제 선택이 바뀌면 true를 반환한다.
// reason: `select` 와 같은 인자 모양에 placeholder 하나를 더한 것이다 — 묶으면 두 드롭다운의
// 호출 모양이 갈린다.
#[allow(clippy::too_many_arguments)]
pub fn select_or_placeholder(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    selected: &mut Option<usize>,
    options: &[&str],
    placeholder: &str,
    width: f32,
    enabled: bool,
) -> bool {
    select_or_placeholder_on(
        ui,
        theme,
        Ground::Panel,
        id_salt,
        selected,
        options,
        placeholder,
        width,
        enabled,
    )
}

/// [`select_or_placeholder`]를 `ground` 바탕 위에 그린다.
// reason: `select_or_placeholder` 인자에 바탕 하나를 더한 것이다.
#[allow(clippy::too_many_arguments)]
pub fn select_or_placeholder_on(
    ui: &mut egui::Ui,
    theme: &Theme,
    ground: Ground,
    id_salt: &str,
    selected: &mut Option<usize>,
    options: &[&str],
    placeholder: &str,
    width: f32,
    enabled: bool,
) -> bool {
    match select_impl(
        ui,
        theme,
        ground,
        id_salt,
        *selected,
        options,
        Some(placeholder),
        width,
        enabled,
    )
    .0
    {
        Some(i) => {
            *selected = Some(i);
            true
        }
        None => false,
    }
}

/// 두 공개 드롭다운이 공유하는 구현. 새로 선택한 인덱스(있으면)와 트리거 rect 를 반환한다.
// reason: 공개 함수 둘의 인자를 그대로 받는 내부 원문이다.
#[allow(clippy::too_many_arguments)]
fn select_impl(
    ui: &mut egui::Ui,
    theme: &Theme,
    ground: Ground,
    id_salt: &str,
    selected: Option<usize>,
    options: &[&str],
    placeholder: Option<&str>,
    width: f32,
    enabled: bool,
) -> (Option<usize>, egui::Rect) {
    let (rect, resp) = alloc_trigger(ui, theme, width, enabled);
    let popup_id = ui.make_persistent_id(("tasty_select", id_salt));
    if enabled && resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    // 목록이 열려 있으면 포커스 테두리와 ring을 그린다. 직접 키를 처리하지 않으므로
    // 키보드 포커스만으로는 포커스 모양을 그리지 않는다.
    let open = ui.memory(|m| m.is_popup_open(popup_id));
    let look = if open {
        SelectLook::Open
    } else if resp.hovered() {
        SelectLook::Hover
    } else {
        SelectLook::Normal
    };
    let label = match (selected, placeholder) {
        (None, Some(p)) => TriggerLabel::Placeholder(p),
        (s, _) => TriggerLabel::Value(s.and_then(|i| options.get(i)).copied().unwrap_or("")),
    };
    paint_trigger(ui, theme, ground, rect, look, enabled, label);

    let mut picked = None;
    tasty_egui_theme::with_popover_frame(ui, theme, |ui| {
        egui::popup_below_widget(
            ui,
            popup_id,
            &resp,
            egui::PopupCloseBehavior::CloseOnClick,
            |ui| {
                ui.set_min_width(width);
                // 옵션은 공용 MenuItem 옵션 행이다. 현재 값만 selected(글자 + 체크, 채움 없음)이고
                // 행은 다른 메뉴처럼 붙인다.
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, opt) in options.iter().enumerate() {
                    if crate::menu_option(ui, theme, opt, selected == Some(i)).clicked()
                        && selected != Some(i)
                    {
                        picked = Some(i);
                    }
                }
            },
        )
    });
    (picked, rect)
}

/// 트리거의 상호작용 모양. disabled이면 무시한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectLook {
    Normal,
    /// 포인터가 올라간 트리거 — border-strong 테두리.
    Hover,
    /// 목록이 열린 트리거 — select-border-focus 테두리와 같은 색 ring.
    Open,
}

enum TriggerLabel<'a> {
    Value(&'a str),
    Placeholder(&'a str),
}

/// 상호작용 없이 트리거 하나를 정해진 모양으로 그린다. [`select`]와 같은 칠을 쓰므로 갤러리가
/// hover·open처럼 포인터로만 생기는 상태를 고정해 보일 때 쓴다. `value`가 없으면 `placeholder`를 그린다.
// reason: 상태 견본은 실제 트리거의 모든 입력(바탕·값·모양·활성)을 그대로 받는다.
#[allow(clippy::too_many_arguments)]
pub fn select_pinned(
    ui: &mut egui::Ui,
    theme: &Theme,
    ground: Ground,
    value: Option<&str>,
    placeholder: &str,
    width: LogicalPx,
    look: SelectLook,
    enabled: bool,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), theme.select_height().value()),
        egui::Sense::hover(),
    );
    let label = value.map_or(TriggerLabel::Placeholder(placeholder), TriggerLabel::Value);
    paint_trigger(ui, theme, ground, rect, look, enabled, label);
    rect
}

/// 트리거 상자·글자·chevron을 그린다. disabled 글자·글리프는 opacity 없이 disabled ink를 쓴다.
fn paint_trigger(
    ui: &egui::Ui,
    theme: &Theme,
    ground: Ground,
    rect: egui::Rect,
    look: SelectLook,
    enabled: bool,
    label: TriggerLabel<'_>,
) {
    let pad_x = theme.select_padding_x().value();
    let body = theme.select_font_size().value();
    let chevron_room = theme.select_chevron_room().value();
    let border = match (look, ground) {
        (SelectLook::Open, _) => theme.select_border_focus(),
        (SelectLook::Hover, Ground::Raised) => theme.border_control_raised(),
        (SelectLook::Hover, _) => theme.border_control(),
        (SelectLook::Normal, Ground::Raised) => theme.select_raised_border(),
        (SelectLook::Normal, _) => theme.select_border(),
    };
    paint_trigger_box(ui.painter(), theme, ground, rect, border, enabled);
    if enabled && look == SelectLook::Open {
        let bw = theme.border_width.value();
        ui.painter().rect_stroke(
            rect.expand(bw),
            theme.select_radius().value(),
            egui::Stroke::new(bw, theme.select_border_focus().to_egui()),
            // 시안 box-shadow 0 0 0 1px처럼 테두리 바로 바깥 한 줄에 붙인다.
            egui::StrokeKind::Inside,
        );
    }
    let dim = |c: egui::Color32| {
        if enabled {
            c
        } else {
            theme.state_disabled_fg().to_egui()
        }
    };
    let (text, fg) = match label {
        TriggerLabel::Value(v) => (v, theme.select_fg()),
        TriggerLabel::Placeholder(p) => (p, theme.select_placeholder()),
    };
    // 문구가 테두리와 화살표를 넘지 않도록 줄인다.
    let text_max_width = (rect.right() - chevron_room - (rect.left() + pad_x)).max(0.0);
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_owned(),
        egui::FontId::proportional(body),
        egui::Color32::PLACEHOLDER,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_max_width);
    let galley = ui.fonts(|f| f.layout_job(job));
    let text_pos = egui::pos2(
        rect.left() + pad_x,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(text_pos, galley, dim(fg.to_egui()));
    let cx = rect.right() - chevron_room * 0.5;
    let ch = dim(theme.select_chevron_fg().to_egui());
    paint_chevron(ui.painter(), egui::pos2(cx, rect.center().y), ch, false);
}

/// 단일·다중 선택 필드가 같은 높이와 입력 영역 계산을 사용한다.
pub(crate) fn alloc_trigger(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: f32,
    enabled: bool,
) -> (egui::Rect, egui::Response) {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    ui.allocate_exact_size(egui::vec2(width, theme.select_height().value()), sense)
}

/// 단일·다중 선택 필드가 공유하는 배경·반경·선 굵기. 상태별 테두리색은 호출자가 정한다.
pub(crate) fn paint_trigger_box(
    painter: &egui::Painter,
    theme: &Theme,
    ground: Ground,
    rect: egui::Rect,
    border: HexColor,
    enabled: bool,
) {
    // disabled 트리거는 상태별 테두리 대신 disabled 상자 role을 쓴다.
    let (fill, edge) = match (enabled, ground) {
        (true, Ground::Raised) => (theme.select_raised_bg(), border),
        (true, _) => (theme.select_bg(), border),
        (false, Ground::Raised) => (
            theme.select_raised_disabled_bg(),
            theme.select_raised_disabled_border(),
        ),
        (false, _) => (theme.state_disabled_fill(), theme.state_disabled_border()),
    };
    painter.rect(
        rect,
        theme.select_radius().value(),
        fill.to_egui(),
        egui::Stroke::new(theme.border_width.value(), edge.to_egui()),
        egui::StrokeKind::Inside,
    );
}

pub(crate) fn paint_chevron(
    painter: &egui::Painter,
    center: egui::Pos2,
    color: egui::Color32,
    up: bool,
) {
    /// chevron 반폭(px).
    const HALF_W: f32 = 4.0;
    /// 꼭짓점이 중심선에서 내려가는 깊이(px).
    const DEPTH: f32 = 2.5;
    /// 양 끝이 중심선에서 올라가는 높이(px).
    const RISE: f32 = 2.0;
    /// chevron 선 굵기(px).
    const STROKE_W: f32 = 1.5;

    let dir = if up { -1.0 } else { 1.0 };
    painter.add(egui::Shape::line(
        vec![
            egui::pos2(center.x - HALF_W, center.y - RISE * dir),
            egui::pos2(center.x, center.y + DEPTH * dir),
            egui::pos2(center.x + HALF_W, center.y - RISE * dir),
        ],
        egui::Stroke::new(STROKE_W, color),
    ));
}
