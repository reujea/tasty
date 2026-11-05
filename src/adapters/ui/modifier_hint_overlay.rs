//! modifier를 누르고 있을 때 표시하는 단축키 도움말. 키보드 포커스는 받지 않으며
//! 마우스로 이동·크기 조절·닫기를 할 수 있다. Shift 단독은 1200ms, 나머지는 500ms 뒤
//! 표시하고 200ms 동안 alpha 0.2→1.0으로 바꾼다. 키를 모두 떼면 닫는다.
//!
//! 홀드 상태는 실제 ModifiersChanged 입력으로 갱신한다. 강제 조작 IPC는 debug 전용이다.
//! modifier_hint_hovered는 아래 surface의 마우스 입력과 커서 덮어쓰기를 막는다.
//! 위치·크기는 드래그를 놓을 때 저장한다.

use std::time::Instant;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::input::shortcuts::modifier_hint::{
    Combo, HintRole, HintRowSource, HintSection, binding_leaf, build_hint_sections,
};

/// 진행 중 드래그/리사이즈 모드.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    /// 드래그 스트립 → 패널 이동.
    Move,
    /// 우하단 코너 그립 → 리사이즈(min 클램프).
    Resize,
}

/// 홀드 상태와 드래그 중의 임시 영역. 저장된 위치·크기는 Settings::modifier_hint에 있다.
#[derive(Debug, Clone, Default)]
pub struct ModifierHintRuntime {
    /// 홀드 시작 시각. `None` = 홀드 아님. **최초 press 에만 시작**하고 조합이 바뀌어도 유지.
    hold_since: Option<Instant>,
    /// 현재 눌린 modifier **조합**(4축). 이 조합을 포함하는 조합만 노출한다. `None` = 홀드 아님.
    held: Option<Combo>,
    /// 이번 홀드 세션 X dismiss 여부. **전 modifier 를 떼면** `false` 로 리셋된다.
    dismissed: bool,
    /// 드래그/리사이즈 진행 중 실시간 rect(logical, 미클램프 원본) + 모드. `None` = 유휴.
    working: Option<(egui::Rect, DragMode)>,
}

impl ModifierHintRuntime {
    /// modifier 조합을 갱신하고 화면을 다시 그려야 하면 true를 반환한다.
    /// 조합이 바뀌어도 최초 홀드 시각은 유지한다. 모두 떼면 닫기 상태까지 초기화한다.
    pub fn update_hold(&mut self, ctrl: bool, alt: bool, option: bool, shift: bool) -> bool {
        let any = ctrl || alt || option || shift;
        if !any {
            let changed = self.hold_since.is_some() || self.held.is_some() || self.dismissed;
            self.hold_since = None;
            self.held = None;
            self.dismissed = false;
            self.working = None;
            return changed;
        }
        let new = Combo {
            ctrl,
            alt,
            option,
            shift,
        };
        let changed = self.held != Some(new);
        self.held = Some(new);
        if self.hold_since.is_none() {
            self.hold_since = Some(Instant::now());
        }
        changed
    }

    /// 창 포커스 상실 등에서 홀드 상태를 전부 비운다(switch-overlay clear 와 동반).
    pub fn clear(&mut self) {
        self.hold_since = None;
        self.held = None;
        self.dismissed = false;
        self.working = None;
    }

    /// 단축키를 실행했을 때 아직 표시 전이면 대기 시간을 다시 잰다.
    /// 이미 표시 중이거나 홀드 중이 아니면 아무것도 바꾸지 않는다.
    /// 닫기·드래그·조합 상태는 유지한다.
    pub fn reset_reveal_timer_if_not_shown(&mut self, theme: &Theme) {
        let (Some(held), Some(since)) = (self.held, self.hold_since) else {
            return;
        };
        let held_ms = since.elapsed().as_secs_f32() * 1000.0;
        if held_ms < reveal_delay_ms(held, theme) {
            self.hold_since = Some(Instant::now());
        }
    }
}

/// debug IPC와 테스트에서만 홀드 상태를 조작·조회한다. release에는 노출하지 않는다.
#[cfg(any(debug_assertions, test))]
impl ModifierHintRuntime {
    /// 현재 홀드 상태 스냅샷 — `(눌린 조합, 경과시간, dismissed)`.
    pub fn debug_snapshot(&self) -> (Option<Combo>, Option<std::time::Duration>, bool) {
        (
            self.held,
            self.hold_since.map(|s| s.elapsed()),
            self.dismissed,
        )
    }

    /// 캡처·테스트를 위해 홀드 시작을 앞당긴다. Instant 하한은 checked_sub로 처리한다.
    pub fn debug_backdate(&mut self, elapsed: std::time::Duration) {
        if let Some(s) = self.hold_since {
            self.hold_since = Some(s.checked_sub(elapsed).unwrap_or(s));
        }
    }
}

/// debug.modifier_hint.state용 상태 덤프. 표시 지연·투명도·섹션 계산은 렌더링과 공유한다.
/// 픽셀을 검사하는 함수는 아니며 release에는 노출하지 않는다.
#[cfg(any(debug_assertions, test))]
pub fn debug_state_json(
    rt: &ModifierHintRuntime,
    settings: &Settings,
    theme: &Theme,
    reduced_motion: bool,
) -> serde_json::Value {
    use serde_json::{Value, json};
    let (held, elapsed, dismissed) = rt.debug_snapshot();
    let Some(held) = held else {
        return json!({
            "held": Value::Null,
            "hold_elapsed_ms": Value::Null,
            "dismissed": dismissed,
            "reveal_delay_ms": Value::Null,
            "visible": false,
            "alpha": Value::Null,
            "header_combo": "",
            "sections": [],
        });
    };
    let elapsed_ms = elapsed.map(|d| d.as_secs_f32() * 1000.0).unwrap_or(0.0);
    let delay = reveal_delay_ms(held, theme);
    let fade = theme.modhint_fade().to_millis_f32();
    let alpha = hold_reveal_alpha(elapsed_ms, delay, fade, reduced_motion);
    let sections = build_hint_sections(
        held,
        &settings.keybindings,
        &settings.general.link_click_modifier,
        settings.general.workspace_categories_enabled,
        &[],
    );
    let sections_json: Vec<Value> = sections
        .iter()
        .map(|s| {
            json!({
                "combo": combo_keycaps(s.combo, &settings.general),
                "rows": s.rows.iter().map(|r| row_key_text(&r.binding)).collect::<Vec<_>>(),
                "roles": s.roles.iter().map(|r| r.desc_key()).collect::<Vec<_>>(),
                // 빈 섹션도 화면에는 "바인딩 없음"으로 표시한다.
                "empty": s.is_empty(),
            })
        })
        .collect();
    // draw와 같은 표시 조건. 빈 조합도 플레이스홀더 섹션은 남는다.
    let visible =
        settings.modifier_hint.enabled && !dismissed && alpha.is_some() && !sections.is_empty();
    json!({
        "held": { "ctrl": held.ctrl, "alt": held.alt, "option": held.option, "shift": held.shift },
        "hold_elapsed_ms": elapsed_ms,
        "dismissed": dismissed,
        "reveal_delay_ms": delay,
        "visible": visible,
        "alpha": alpha,
        "header_combo": combo_keycaps(held, &settings.general),
        "sections": sections_json,
    })
}

/// 표시 지연 전에는 None, 이후에는 alpha 0.2→1.0을 반환한다.
/// reduced_motion이면 지연은 유지하고 페이드만 생략한다. 시간은 Theme 토큰에서 받는다.
pub fn hold_reveal_alpha(
    held_ms: f32,
    delay_ms: f32,
    fade_ms: f32,
    reduced_motion: bool,
) -> Option<f32> {
    if held_ms < delay_ms {
        return None;
    }
    if reduced_motion {
        return Some(1.0);
    }
    let t = held_ms - delay_ms;
    if t >= fade_ms {
        Some(1.0)
    } else {
        Some((0.2 + 0.8 * (t / fade_ms)).clamp(0.2, 1.0))
    }
}

/// Shift 단독은 타이핑 중 오표시를 줄이기 위해 1200ms, 나머지는 500ms를 기다린다.
/// 현재 조합으로 매번 다시 계산하며 Theme의 시간 값을 ms로 변환한다.
fn reveal_delay_ms(held: Combo, theme: &Theme) -> f32 {
    if held.shift && !held.ctrl && !held.alt && !held.option {
        theme.motion_hold_reveal_shift().to_millis_f32()
    } else {
        theme.modhint_hold_delay().to_millis_f32()
    }
}

/// 저장값이 없으면 Theme의 기본 크기로 화면 좌하단에 배치한다.
pub fn default_rect(screen: egui::Rect, theme: &Theme) -> egui::Rect {
    let w = theme.modhint_width().value();
    let h = theme.modhint_height().value();
    let margin = theme.spacing_sm.value();
    let left = screen.left() + margin;
    let bottom = screen.bottom() - margin;
    egui::Rect::from_min_size(egui::pos2(left, bottom - h), egui::vec2(w, h))
}

/// rect 를 화면 안으로 클램프한다 — **저장값은 불변, 렌더만 클램프**(윈도우 축소 대응).
/// 크기가 화면보다 크면 크기도 줄인다. 위치는 화면 안쪽으로 평행이동.
pub fn clamp_rect(rect: egui::Rect, screen: egui::Rect) -> egui::Rect {
    let w = rect.width().min(screen.width());
    let h = rect.height().min(screen.height());
    let x = rect
        .left()
        .clamp(screen.left(), (screen.right() - w).max(screen.left()));
    let y = rect
        .top()
        .clamp(screen.top(), (screen.bottom() - h).max(screen.top()));
    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
}

/// 코너 리사이즈 — 좌상단(min)을 고정하고 우하단을 `delta` 만큼 이동, min 크기로 클램프.
pub fn resize_to(rect: egui::Rect, delta: egui::Vec2, min_w: f32, min_h: f32) -> egui::Rect {
    let w = (rect.width() + delta.x).max(min_w);
    let h = (rect.height() + delta.y).max(min_h);
    egui::Rect::from_min_size(rect.min, egui::vec2(w, h))
}

use std::time::Duration;

use crate::i18n::t;
use tasty_settings::Settings;
use tasty_ui_widgets::{
    ControlSize, IconButton, IconButtonVariant, kbd, kbd_parts, modhint_resize_grip,
};

use crate::adapters::ui::icons;

/// 마우스 입력 차단, 위치·크기 저장, 레이어 정렬에 필요한 렌더링 결과.
#[derive(Default, Clone, Copy)]
pub struct HintDrawResult {
    /// 마우스가 패널 위 → 하위 surface 전파 차단(`MainViewState::modifier_hint_hovered`).
    pub hovered: bool,
    /// 드래그/리사이즈를 놓은 시점의 (pos, size). `Some` 이면 호출자가 `UpdateSettings` 로
    /// 영속한다(사용자 행동 → `from_user_menu`). `None` = 이번 프레임 변경 없음.
    pub persist: Option<((LogicalPx, LogicalPx), (LogicalPx, LogicalPx))>,
    /// 실제 그린 Area의 LayerId. enforce_foreground_z_order에서 팝업과의 순서를 정한다.
    pub layer: Option<egui::LayerId>,
}

/// 설정이 켜져 있고 홀드 지연이 지났으며 닫지 않은 경우 도움말을 그린다.
/// 지연이 끝날 때 다시 그리기를 예약하고, 페이드 중에는 매 프레임 갱신한다.
pub fn draw_modifier_hint(
    ctx: &egui::Context,
    rt: &mut ModifierHintRuntime,
    settings: &Settings,
    theme: &Theme,
    reduced_motion: bool,
) -> HintDrawResult {
    let mut result = HintDrawResult::default();

    if !settings.modifier_hint.enabled {
        rt.working = None;
        return result;
    }
    let (Some(held), Some(since)) = (rt.held, rt.hold_since) else {
        return result;
    };
    if rt.dismissed {
        return result; // 이번 홀드 세션은 X 로 닫힘 — 재홀드(전 modifier release) 전까지 미표시.
    }

    let held_ms = since.elapsed().as_secs_f32() * 1000.0;
    let delay = reveal_delay_ms(held, theme);
    let fade = theme.modhint_fade().to_millis_f32();
    let Some(alpha) = hold_reveal_alpha(held_ms, delay, fade, reduced_motion) else {
        // 해당 조합의 표시 지연이 끝날 때 다시 그린다.
        let remain = (delay - held_ms).max(1.0);
        ctx.request_repaint_after(Duration::from_millis(remain as u64));
        return result;
    };

    let sections = build_hint_sections(
        held,
        &settings.keybindings,
        &settings.general.link_click_modifier,
        settings.general.workspace_categories_enabled,
        &[], // plugin_bindings: PluginManager 는 App 소유라 draw 경로 미도달 → 후속 배선(open).
    );
    // 바인딩이 없는 조합도 섹션은 남는다. 목록 자체가 없으면 그리지 않는다.
    if sections.is_empty() {
        return result;
    }

    let screen = ctx.screen_rect();
    let base = rt
        .working
        .map(|(r, _)| r)
        .unwrap_or_else(|| rect_from_settings(settings, screen, theme));
    let render_rect = clamp_rect(base, screen);

    // Area에 등록해야 egui 레이어 정렬에 참여한다.
    // enforce_foreground_z_order에서 열린 팝업을 이 레이어 바로 위에 둔다.
    // 입력 순서: ../../../docs/architecture/input-layer.md.
    let layer_id = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("modhint_layer"));
    result.layer = Some(layer_id);
    egui::Area::new(layer_id.id)
        .order(egui::Order::Foreground)
        .fixed_pos(egui::Pos2::ZERO)
        .movable(false)
        .interactable(true)
        .sense(egui::Sense::hover())
        .constrain(false)
        .show(ctx, |area_ui| {
            let mut ui = ui_at(area_ui, render_rect);

            // 내용의 불투명 배경이 테두리를 덮지 않도록 테두리를 마지막에 그린다.
            // source_guards::modifier_hint_paint_order가 호출 순서를 검사한다.
            draw_shell(&ui, theme, render_rect, alpha);
            draw_content(
                &mut ui,
                theme,
                render_rect,
                held,
                &sections,
                alpha,
                &settings.general,
            );
            draw_shell_border(&ui, theme, render_rect, alpha);
            // 오른쪽 아래 리사이즈 그립. 아래 상호작용 영역(`modhint_resize`)과 같은 모서리다.
            let mut grip_painter = ui.painter().clone();
            grip_painter.multiply_opacity(alpha);
            modhint_resize_grip(&grip_painter, theme, render_rect);

            let strip_h = theme.modhint_strip_height().value();
            let x_zone = strip_h; // 우측 X 버튼 폭 만큼 드래그에서 제외.
            let strip_drag = egui::Rect::from_min_max(
                render_rect.min,
                egui::pos2(render_rect.right() - x_zone, render_rect.top() + strip_h),
            );
            let move_resp = ui
                .interact(
                    strip_drag,
                    egui::Id::new("modhint_move"),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::Move);
            if move_resp.drag_started() {
                rt.working = Some((render_rect, DragMode::Move));
            }
            if move_resp.dragged()
                && let Some((r, DragMode::Move)) = rt.working.as_mut()
            {
                *r = clamp_rect(r.translate(move_resp.drag_delta()), screen);
            }

            let grip = theme.modhint_grip_size().value();
            let grip_rect = egui::Rect::from_min_max(
                egui::pos2(render_rect.right() - grip, render_rect.bottom() - grip),
                render_rect.max,
            );
            let resize_resp = ui
                .interact(
                    grip_rect,
                    egui::Id::new("modhint_resize"),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
            if resize_resp.drag_started() {
                rt.working = Some((render_rect, DragMode::Resize));
            }
            if resize_resp.dragged()
                && let Some((r, DragMode::Resize)) = rt.working.as_mut()
            {
                let resized = resize_to(
                    *r,
                    resize_resp.drag_delta(),
                    theme.modhint_min_width().value(),
                    theme.modhint_min_height().value(),
                );
                *r = clamp_rect(resized, screen);
            }

            if (move_resp.drag_stopped() || resize_resp.drag_stopped())
                && let Some((r, _)) = rt.working.take()
            {
                let c = clamp_rect(r, screen);
                result.persist = Some((
                    (LogicalPx(c.left()), LogicalPx(c.top())),
                    (LogicalPx(c.width()), LogicalPx(c.height())),
                ));
            }

            let x_rect = egui::Rect::from_min_size(
                egui::pos2(render_rect.right() - x_zone, render_rect.top()),
                egui::vec2(x_zone, strip_h),
            );
            let mut x_ui = ui.new_child(egui::UiBuilder::new().max_rect(x_rect).layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            ));
            let x_clicked = IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(&mut x_ui, theme, &|ui, r, c| {
                    icons::CLOSE.image(r.height(), c).paint_at(ui, r);
                })
                .on_hover_text(t("modifier_hint.hide_tooltip"))
                .clicked();
            if x_clicked {
                rt.dismissed = true;
                rt.working = None;
            }
        });

    result.hovered = ctx
        .pointer_hover_pos()
        .is_some_and(|p| render_rect.contains(p));

    // 페이드가 끝나면 입력에 따른 다시 그리기만 사용한다.
    if alpha < 1.0 {
        ctx.request_repaint();
    }

    result
}

/// 저장된 pos/size(있으면) 또는 기본 anchor 로 base rect 를 만든다.
fn rect_from_settings(settings: &Settings, screen: egui::Rect, theme: &Theme) -> egui::Rect {
    let def = default_rect(screen, theme);
    let size = settings
        .modifier_hint
        .size
        .map(|(w, h)| egui::vec2(w.value(), h.value()))
        .unwrap_or_else(|| def.size());
    let min = settings
        .modifier_hint
        .pos
        .map(|(x, y)| egui::pos2(x.value(), y.value()))
        .unwrap_or(def.min);
    egui::Rect::from_min_size(min, size)
}

/// 고정 크기 패널의 그림자와 배경. 테두리는 내용 뒤에 draw_shell_border로 그린다.
fn draw_shell(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, alpha: f32) {
    let radius = theme.corner_radius.value();
    let painter = ui.painter();
    let mut shadow = theme.shadow_popover().to_egui();
    shadow.color = shadow.color.gamma_multiply(alpha);
    painter.add(shadow.as_shape(rect, radius));
    painter.rect_filled(
        rect,
        radius,
        theme.modhint_bg().to_egui().gamma_multiply(alpha),
    );
}

/// 내용이 테두리를 가리지 않도록 draw_content 뒤에 호출한다.
fn draw_shell_border(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, alpha: f32) {
    let radius = theme.corner_radius.value();
    let bw = theme.border_width.value();
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, theme.modhint_border().to_egui().gamma_multiply(alpha)),
        egui::StrokeKind::Inside,
    );
}

/// 스트립("held" 조합) + 스크롤 리스트(섹션) 를 그린다. X 버튼은 호출측이 인터랙션과 함께 그린다.
fn draw_content(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    held: Combo,
    sections: &[HintSection],
    alpha: f32,
    general: &tasty_settings::GeneralSettings,
) {
    let w = rect.width();
    let strip_h = theme.modhint_strip_height().value();
    let bw = theme.border_width.value();

    let strip_rect = egui::Rect::from_min_size(rect.min, egui::vec2(w, strip_h));
    ui.painter().rect_filled(
        strip_rect,
        0.0,
        theme.modhint_strip_bg().to_egui().gamma_multiply(alpha),
    );
    ui.painter().hline(
        strip_rect.x_range(),
        strip_rect.bottom() - bw * 0.5,
        egui::Stroke::new(
            bw,
            theme
                .modhint_separator()
                .to_egui_premultiplied()
                .gamma_multiply(alpha),
        ),
    );

    let pad_l = theme.modhint_pad().value();
    let strip_inner = egui::Rect::from_min_max(
        egui::pos2(strip_rect.left() + pad_l, strip_rect.top()),
        egui::pos2(strip_rect.right() - strip_h, strip_rect.bottom()),
    );
    let mut strip_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip_inner));
    strip_ui.set_opacity(alpha);
    strip_ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        kbd_parts(ui, theme, &combo_keycap_parts(held, general));
        ui.label(
            egui::RichText::new(t("modifier_hint.held"))
                .size(theme.font_size_caption.value())
                .color(theme.modhint_held_fg().to_egui()),
        );
    });

    let list_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), strip_rect.bottom()),
        egui::pos2(rect.right(), rect.bottom()),
    );
    let mut list_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(list_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    list_ui.set_opacity(alpha);

    // Ctrl/Cmd/Shift가 있으면 egui가 휠을 줌·가로 스크롤로 처리하므로 세로 양을 직접 전달한다.
    // Alt/Option만 있으면 egui가 처리하므로 중복 적용하지 않는다.
    let wheel_y = modifier_free_wheel_y(ui.ctx(), rect);
    let pad = theme.modhint_pad().value();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .show(&mut list_ui, |ui| {
            if wheel_y != 0.0 {
                ui.scroll_with_delta(egui::vec2(0.0, wheel_y));
                ui.ctx().request_repaint();
            }
            egui::Frame::new()
                .inner_margin(egui::Margin::same(pad as i8))
                .show(ui, |ui| {
                    ui.set_width((list_rect.width() - pad * 2.0).max(0.0));
                    ui.spacing_mut().item_spacing.y = theme.modhint_section_gap().value();
                    for (i, sec) in sections.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(theme.modhint_section_gap().value());
                        }
                        draw_section(ui, theme, sec, general);
                    }
                });
        });
}

/// 포인터가 패널 위에 있고 Ctrl/Cmd/Shift가 눌렸을 때 세로 휠 양을 반환한다.
/// egui가 줌·가로 스크롤로 바꾸기 전 이벤트를 읽는다. 단위 변환과 부호는 egui와 같다
/// (Point 그대로, Line×line_scroll_speed, Page×화면 높이). 나머지는 0으로 중복 처리를 피한다.
pub(crate) fn modifier_free_wheel_y(ctx: &egui::Context, rect: egui::Rect) -> f32 {
    let pointer_over = ctx.pointer_hover_pos().is_some_and(|p| rect.contains(p));
    if !pointer_over {
        return 0.0;
    }
    let line_speed = ctx.options(|o| o.line_scroll_speed);
    let page = ctx.screen_rect().height();
    ctx.input(|i| {
        let m = i.modifiers;
        if !(m.ctrl || m.mac_cmd || m.command || m.shift) {
            return 0.0;
        }
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                    egui::MouseWheelUnit::Point => delta.y,
                    egui::MouseWheelUnit::Line => line_speed * delta.y,
                    egui::MouseWheelUnit::Page => page * delta.y,
                }),
                _ => None,
            })
            .sum()
    })
}

/// 한 조합 섹션 — ChordHead(Kbd + separator) + RoleRow* + HintRow*. 시안처럼 조합이 하는 일을 먼저 보인다.
fn draw_section(
    ui: &mut egui::Ui,
    theme: &Theme,
    sec: &HintSection,
    general: &tasty_settings::GeneralSettings,
) {
    kbd_parts(ui, theme, &combo_key_parts(sec, general));
    ui.add_space(theme.modhint_row_gap().value());
    let w = ui.available_width();
    let (hr, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        hr.x_range(),
        hr.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.modhint_separator().to_egui_premultiplied(),
        ),
    );
    // 빈 섹션의 내부 간격만 좁혀 불필요한 높이를 줄인다.
    let content_gap = if sec.is_empty() {
        theme.modhint_empty_row_gap().value()
    } else {
        theme.modhint_row_gap().value()
    };
    ui.add_space(content_gap);

    ui.spacing_mut().item_spacing.y = content_gap;
    if sec.is_empty() {
        draw_empty_row(ui, theme);
    } else {
        for role in &sec.roles {
            draw_role_row(ui, theme, *role);
        }
        for row in &sec.rows {
            draw_row(ui, theme, row);
        }
    }
}

/// 빈 조합에는 키캡·배경·상호작용 없이 "바인딩 없음"만 표시한다.
fn draw_empty_row(ui: &mut egui::Ui, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.set_min_height(theme.modhint_empty_row_min_height().value());
        ui.add(
            egui::Label::new(
                egui::RichText::new(t("modifier_hint.empty"))
                    .size(theme.modhint_row_font_size().value())
                    .color(theme.modhint_empty_fg().to_egui()),
            )
            .selectable(false),
        );
    });
}

/// 액션 행 — (plugin 이면 agent dot) + 라벨 + 우측 Kbd.
fn draw_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    row: &super::input::shortcuts::modifier_hint::HintRow,
) {
    ui.horizontal(|ui| {
        ui.set_min_height(theme.modhint_row_min_height().value());
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let (label, is_plugin) = row_label(&row.source);
        if is_plugin {
            let d = theme.status_dot_size().value();
            let (r, _) = ui.allocate_exact_size(egui::vec2(d, d), egui::Sense::hover());
            ui.painter()
                .circle_filled(r.center(), d * 0.5, theme.modhint_agent_dot().to_egui());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            kbd(ui, theme, &row_key_text(&row.binding));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(label)
                            .size(theme.modhint_row_font_size().value())
                            .color(theme.modhint_row_fg().to_egui()),
                    )
                    .wrap(),
                );
            });
        });
    });
}

/// 특수 역할 행 — washed 배경 + leading 글리프(role-fg) + 설명.
fn draw_role_row(ui: &mut egui::Ui, theme: &Theme, role: HintRole) {
    egui::Frame::new()
        .fill(theme.modhint_role_bg().to_egui())
        .corner_radius(theme.corner_radius_sm.value())
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_sm.value() as i8,
            theme.modhint_row_gap().value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                let gsz = theme.icon_glyph_size_xs.value();
                let (r, _) = ui.allocate_exact_size(egui::vec2(gsz, gsz), egui::Sense::hover());
                let col = theme.modhint_role_fg().to_egui();
                match role {
                    HintRole::MouseCaptureBypass => {
                        icons::MOUSE.image(gsz, col).paint_at(ui, r);
                    }
                    HintRole::CategorySwitch => {
                        icons::FOLDER.image(gsz, col).paint_at(ui, r);
                    }
                    HintRole::TabSwitch | HintRole::WorkspaceSwitch | HintRole::LinkClick => {
                        ui.painter().text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            "#",
                            egui::FontId::monospace(gsz),
                            col,
                        );
                    }
                }
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(t(role.desc_key()))
                            .size(theme.modhint_row_font_size().value())
                            .color(theme.modhint_role_text_fg().to_egui()),
                    )
                    .wrap(),
                );
            });
        });
}

/// debug JSON용 키 조합 문자열. Alt/Option/Shift 표기는 GeneralSettings를 따른다.
/// 화면에서는 글꼴의 기호 누락을 피하기 위해 combo_keycap_parts의 벡터 아이콘을 쓴다.
#[cfg(any(debug_assertions, test))]
fn combo_keycaps(c: Combo, general: &tasty_settings::GeneralSettings) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if c.ctrl {
        parts.push("Ctrl");
    }
    if c.alt {
        parts.push(match general.alt_display_style.as_str() {
            "cmd" => "Cmd",
            "symbol" => "⌘",
            _ => "Alt",
        });
    }
    if c.option {
        parts.push(tasty_settings::KeybindingSettings::option_display_text(
            general,
        ));
    }
    if c.shift {
        parts.push(match general.shift_display_style.as_str() {
            "symbol" => "⇧",
            _ => "Shift",
        });
    }
    parts.join("+")
}

/// 화면용 키캡. symbol 스타일은 글꼴에 없는 기호가 생기지 않도록 벡터 아이콘으로 그린다.
fn combo_keycap_parts(
    c: Combo,
    general: &tasty_settings::GeneralSettings,
) -> Vec<tasty_ui_widgets::KbdKey<'static>> {
    use tasty_ui_widgets::KbdKey;
    let mut parts = Vec::new();
    if c.ctrl {
        parts.push(KbdKey::Text("Ctrl"));
    }
    if c.alt {
        match general.alt_display_style.as_str() {
            "cmd" => parts.push(KbdKey::Text("Cmd")),
            "symbol" => parts.push(KbdKey::Icon(icons::CMD_KEY)),
            _ => parts.push(KbdKey::Text("Alt")),
        }
    }
    if c.option {
        // 표시 방식(Option·⌥)은 macOS 설정이다. 다른 OS 는 같은 위치의 키 이름(Win·Super)을 쓴다.
        if cfg!(target_os = "macos") && general.option_display_style == "symbol" {
            parts.push(KbdKey::Icon(icons::OPTION_KEY));
        } else {
            parts.push(KbdKey::Text(
                tasty_settings::KeybindingSettings::option_display_text(general),
            ));
        }
    }
    if c.shift {
        match general.shift_display_style.as_str() {
            "symbol" => parts.push(KbdKey::Icon(icons::SHIFT_KEY)),
            _ => parts.push(KbdKey::Text("Shift")),
        }
    }
    parts
}

/// 섹션 조합 → 키캡 파트. [`combo_keycap_parts`] 를 섹션 헤더 draw 경로에서 재사용.
fn combo_key_parts(
    sec: &HintSection,
    general: &tasty_settings::GeneralSettings,
) -> Vec<tasty_ui_widgets::KbdKey<'static>> {
    combo_keycap_parts(sec.combo, general)
}

/// 행 바인딩의 키캡 문자열. modifier 는 섹션 헤더가 보여 주므로 키만 남기고, 설정 화면과 같은
/// [`KeybindingSettings::format_key_display`] 로 표시한다(`ctrl+plus` → `+`).
fn row_key_text(binding: &str) -> String {
    tasty_settings::KeybindingSettings::format_key_display(binding_leaf(binding))
}

/// 행 출처 → (표시 라벨, plugin 여부). 라벨 해석은 이 오버레이 책임(모델은 키만 반환).
fn row_label(source: &HintRowSource) -> (String, bool) {
    match source {
        HintRowSource::Host { label_key } => (t(label_key).to_string(), false),
        // 여기서는 ScriptRegistry를 조회하지 않으므로 script_id를 표시한다.
        HintRowSource::Script { script_id } => (script_id.clone(), false),
        HintRowSource::Plugin {
            plugin_id,
            title_i18n_key,
        } => (format!("{plugin_id}: {}", t(title_i18n_key)), true),
    }
}

/// Area의 레이어를 사용하는 자식 Ui를 만들고 clip_rect를 지정 영역으로 좁힌다.
fn ui_at(parent: &mut egui::Ui, rect: egui::Rect) -> egui::Ui {
    let mut child = parent.new_child(
        egui::UiBuilder::new()
            .id_salt("modhint_overlay")
            .max_rect(rect),
    );
    child.set_clip_rect(rect);
    child
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: f32 = 500.0;
    const FADE: f32 = 200.0;

    #[test]
    fn row_keys_use_the_settings_key_display() {
        assert_eq!(row_key_text("ctrl+plus"), "+");
        assert_eq!(row_key_text("ctrl++"), "+");
        assert_eq!(row_key_text("ctrl+minus"), "-");
        assert_eq!(row_key_text("ctrl+shift+t"), "T");
        assert_eq!(row_key_text("ctrl+,"), ",");
    }

    #[test]
    fn alpha_hidden_before_delay() {
        assert_eq!(hold_reveal_alpha(0.0, DELAY, FADE, false), None);
        assert_eq!(hold_reveal_alpha(499.0, DELAY, FADE, false), None);
    }

    #[test]
    fn alpha_starts_at_0_2_and_reaches_1_0() {
        assert_eq!(hold_reveal_alpha(500.0, DELAY, FADE, false), Some(0.2));
        let mid = hold_reveal_alpha(600.0, DELAY, FADE, false).unwrap();
        assert!((mid - 0.6).abs() < 1e-4, "t=600 alpha={mid}");
        assert_eq!(hold_reveal_alpha(700.0, DELAY, FADE, false), Some(1.0));
        assert_eq!(hold_reveal_alpha(1000.0, DELAY, FADE, false), Some(1.0));
    }

    #[test]
    fn alpha_reduced_motion_snaps_at_delay_boundary() {
        assert_eq!(hold_reveal_alpha(499.0, DELAY, FADE, true), None);
        assert_eq!(hold_reveal_alpha(500.0, DELAY, FADE, true), Some(1.0));
        assert_eq!(hold_reveal_alpha(600.0, DELAY, FADE, true), Some(1.0));
    }

    #[test]
    fn update_hold_stores_combo_and_dirties_on_change_keeping_timer() {
        let mut rt = ModifierHintRuntime::default();
        assert!(rt.update_hold(true, false, false, false));
        assert_eq!(
            rt.held,
            Some(Combo {
                ctrl: true,
                ..Default::default()
            })
        );
        let t0 = rt.hold_since;
        assert!(t0.is_some());
        assert!(
            rt.update_hold(true, false, false, true),
            "조합 변경 시 dirty"
        );
        assert_eq!(
            rt.held,
            Some(Combo {
                ctrl: true,
                shift: true,
                ..Default::default()
            })
        );
        assert_eq!(rt.hold_since, t0, "조합 변경 시 타이머 리셋 금지");
        assert!(!rt.update_hold(true, false, false, true));
    }

    #[test]
    fn update_hold_follows_combo_when_axis_released() {
        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(true, false, false, true); // Ctrl+Shift
        let t0 = rt.hold_since;
        assert!(rt.update_hold(false, false, false, true));
        assert_eq!(
            rt.held,
            Some(Combo {
                shift: true,
                ..Default::default()
            })
        );
        assert_eq!(rt.hold_since, t0);
    }

    #[test]
    fn reveal_delay_shift_only_is_1200_else_500() {
        let theme = tasty_themes::mocha_fallback();
        let shift = Combo {
            shift: true,
            ..Default::default()
        };
        assert_eq!(reveal_delay_ms(shift, &theme), 1200.0);
        let ctrl_shift = Combo {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(reveal_delay_ms(ctrl_shift, &theme), 500.0);
        let ctrl = Combo {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(reveal_delay_ms(ctrl, &theme), 500.0);
    }

    #[test]
    fn shift_only_1200ms_gate_hides_before_and_shows_after() {
        assert_eq!(hold_reveal_alpha(500.0, 1200.0, FADE, false), None);
        assert!(hold_reveal_alpha(1300.0, 1200.0, FADE, false).is_some());
    }

    #[test]
    fn update_hold_clears_and_resets_dismiss_on_full_release() {
        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(true, false, false, false);
        rt.dismissed = true;
        assert!(rt.update_hold(false, false, false, false));
        assert!(rt.held.is_none());
        assert!(rt.hold_since.is_none());
        assert!(!rt.dismissed, "전 modifier release 시 dismiss 리셋");
    }

    /// 표시 지연 게이트 통과 전(400ms < 500ms delay)에 단축키가 실행되면 타이머가
    /// 다시 시작돼, 리셋 직후 경과시간 기준으로는 여전히 게이트 미통과(alpha=None)다.
    #[test]
    fn reset_reveal_timer_rewinds_before_gate() {
        let theme = tasty_themes::mocha_fallback();
        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(true, false, false, false); // Ctrl 홀드 시작(delay 500ms).
        rt.debug_backdate(std::time::Duration::from_millis(400));

        rt.reset_reveal_timer_if_not_shown(&theme);

        let (held, elapsed, _) = rt.debug_snapshot();
        let held = held.expect("held 는 리셋 대상 아님 — 유지돼야 한다");
        let elapsed_ms = elapsed.expect("홀드 중이므로 Some").as_secs_f32() * 1000.0;
        let delay = reveal_delay_ms(held, &theme);
        assert_eq!(
            hold_reveal_alpha(elapsed_ms, delay, FADE, false),
            None,
            "리셋 직후이므로 여전히 지연 게이트 전이어야 한다 (elapsed={elapsed_ms}ms, delay={delay}ms)"
        );
    }

    /// 이미 지연 게이트를 통과한(700ms > 500ms delay = 표시 중인) 홀드에서 단축키가
    /// 실행돼도 타이머는 건드리지 않는다 — 뜬 패널을 숨겼다 다시 띄우지 않는다.
    #[test]
    fn reset_reveal_timer_keeps_timer_after_gate() {
        let theme = tasty_themes::mocha_fallback();
        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(true, false, false, false);
        rt.debug_backdate(std::time::Duration::from_millis(700));

        rt.reset_reveal_timer_if_not_shown(&theme);

        let (held, elapsed, _) = rt.debug_snapshot();
        let held = held.expect("held 는 리셋 대상 아님");
        let elapsed_ms = elapsed.expect("홀드 중이므로 Some").as_secs_f32() * 1000.0;
        let delay = reveal_delay_ms(held, &theme);
        assert!(
            hold_reveal_alpha(elapsed_ms, delay, FADE, false).is_some(),
            "이미 표시 중인 홀드는 리셋되면 안 된다 (elapsed={elapsed_ms}ms, delay={delay}ms)"
        );
    }

    /// 홀드 중이 아닐 때(`hold_since`/`held` 가 `None`) 호출되면 아무 것도 하지 않는다 —
    /// modifier 없이 실행된 단축키가 새 홀드를 만들면 안 된다.
    #[test]
    fn reset_reveal_timer_is_noop_without_hold() {
        let theme = tasty_themes::mocha_fallback();
        let mut rt = ModifierHintRuntime::default();

        rt.reset_reveal_timer_if_not_shown(&theme);

        assert!(rt.hold_since.is_none());
        assert!(rt.held.is_none());
    }

    /// Shift 단독 홀드는 자기 조합의 delay(1200ms) 기준으로 게이트 통과 여부를 판정한다
    /// — 500ms 는 넘었지만 1200ms 는 아직인 900ms 시점은 여전히 리셋 대상이다.
    #[test]
    fn reset_reveal_timer_uses_shift_only_delay() {
        let theme = tasty_themes::mocha_fallback();
        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(false, false, false, true); // Shift 단독(delay 1200ms).
        rt.debug_backdate(std::time::Duration::from_millis(900));

        rt.reset_reveal_timer_if_not_shown(&theme);

        let (held, elapsed, _) = rt.debug_snapshot();
        let held = held.expect("held 는 리셋 대상 아님");
        let elapsed_ms = elapsed.expect("홀드 중이므로 Some").as_secs_f32() * 1000.0;
        let delay = reveal_delay_ms(held, &theme);
        assert_eq!(delay, 1200.0, "Shift 단독은 1200ms delay");
        assert_eq!(
            hold_reveal_alpha(elapsed_ms, delay, FADE, false),
            None,
            "900ms 는 500ms 는 넘지만 Shift 단독 delay(1200ms) 는 아직 — 리셋돼야 한다"
        );
    }

    /// X 로 dismiss 된 세션에서 단축키가 실행돼도 `dismissed` 는 건드리지 않는다.
    #[test]
    fn reset_reveal_timer_does_not_touch_dismissed() {
        let theme = tasty_themes::mocha_fallback();
        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(true, false, false, false);
        rt.dismissed = true;
        rt.debug_backdate(std::time::Duration::from_millis(200));

        rt.reset_reveal_timer_if_not_shown(&theme);

        assert!(rt.dismissed, "dismissed 는 리셋 대상 아님");
    }

    #[test]
    fn clamp_keeps_rect_inside_screen() {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let r = egui::Rect::from_min_size(egui::pos2(700.0, 500.0), egui::vec2(220.0, 400.0));
        let c = clamp_rect(r, screen);
        assert!(c.max.x <= screen.right() + 0.01);
        assert!(c.max.y <= screen.bottom() + 0.01);
        assert_eq!(c.size(), egui::vec2(220.0, 400.0));
    }

    #[test]
    fn clamp_shrinks_rect_bigger_than_screen() {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(150.0, 150.0));
        let r = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(220.0, 400.0));
        let c = clamp_rect(r, screen);
        assert_eq!(c.size(), egui::vec2(150.0, 150.0));
    }

    #[test]
    fn resize_clamps_to_min() {
        let r = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(220.0, 400.0));
        let shrunk = resize_to(r, egui::vec2(-500.0, -500.0), 200.0, 240.0);
        assert_eq!(shrunk.size(), egui::vec2(200.0, 240.0));
        assert_eq!(shrunk.min, r.min, "좌상단 고정");
        let grown = resize_to(r, egui::vec2(30.0, 20.0), 200.0, 240.0);
        assert_eq!(grown.size(), egui::vec2(250.0, 420.0));
    }

    #[test]
    fn consumes_build_hint_sections() {
        use tasty_settings::KeybindingSettings;
        let kb = KeybindingSettings::preset_tasty();
        let ctrl = Combo {
            ctrl: true,
            ..Default::default()
        };
        let sections: Vec<HintSection> = build_hint_sections(ctrl, &kb, "ctrl", false, &[]);
        assert!(!sections.is_empty());
    }

    /// 바인딩이 없어도 조합 섹션은 표시하며 debug 덤프에는 empty:true가 남는다.
    #[test]
    fn debug_state_marks_empty_combo_and_stays_visible() {
        use tasty_settings::{KeybindingSettings, Settings};
        let mut settings = Settings::default();
        // 모든 바인딩을 비워 어떤 조합에도 행이 안 붙게 한다.
        for (field_id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
            settings.keybindings.clear_field(field_id);
        }
        settings.keybindings.script_bindings.clear();
        // 역할도 안 붙게: switch modifier 를 Ctrl/Alt 아닌 값으로, link 는 none.
        settings.keybindings.tab_switch_modifier = "shift".into();
        settings.keybindings.workspace_switch_modifier = "shift".into();
        settings.general.link_click_modifier = "none".into();
        settings.modifier_hint.enabled = true;

        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(true, true, false, false); // Ctrl+Alt 홀드
        rt.debug_backdate(std::time::Duration::from_millis(5000)); // 표시 지연 게이트 통과

        let theme = tasty_themes::mocha_fallback();
        let v = debug_state_json(&rt, &settings, &theme, false);

        assert_eq!(v["visible"], serde_json::json!(true), "dump={v}");
        let sections = v["sections"].as_array().expect("sections 배열");
        // Alt 표기는 OS가 아니라 general.alt_display_style의 기본값을 따른다.
        let held_combo = "Ctrl+Alt";
        let ctrl_alt = sections
            .iter()
            .find(|s| s["combo"] == serde_json::json!(held_combo))
            .unwrap_or_else(|| panic!("{held_combo} 섹션 존재. dump={v}"));
        assert_eq!(ctrl_alt["empty"], serde_json::json!(true));
        assert!(ctrl_alt["rows"].as_array().unwrap().is_empty());
        assert!(ctrl_alt["roles"].as_array().unwrap().is_empty());
        assert!(
            sections
                .iter()
                .all(|s| s["empty"] == serde_json::json!(true)),
            "dump={v}"
        );
    }
}
