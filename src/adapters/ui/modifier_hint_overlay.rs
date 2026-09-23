//! Modifier-hint 오버레이 **본체** — 홀드→표시 수명주기 + 드래그/리사이즈/영속 + 커스텀 draw.
//!
//! 4분류(Popup/Toast/Banner/Modal) 어디에도 안 맞는 **신규 오버레이 요소**다:
//! **키보드 포커스 없음 + 마우스 인터랙티브(드래그 이동·테두리/코너 리사이즈·X) + 홀드 수명**.
//! modifier 를 500ms 이상 홀드하면 200ms 페이드(opacity 0.2→1.0)로 등장하고, 키를 떼면
//! 즉시 소멸한다. 콘텐츠 모델은 [`super::input::shortcuts::modifier_hint`](modifier-hint-02),
//! 시각 토큰은 `Theme::modhint_*()`(modifier-hint-01 슬롯 + design-token-mapping).
//!
//! ## 불가침 원칙
//! - **원칙1(사용자↔에이전트 분리)**: 홀드 상태는 winit `ModifiersChanged`(실제 사용자
//!   입력)만 반영한다. IPC/CLI 로 강제 표시할 수 없다. X dismiss·드래그·리사이즈는 사용자
//!   마우스만.
//! - **원칙3(포커스 독립성)**: 키보드 포커스를 **절대** 취득하지 않는다. 마우스만 소비하며,
//!   `AppState::modifier_hint_hovered` 가드가 `mouse.rs` 4지점에서 하위 surface 로의 전파
//!   (click-to-activate/휠/드래그)를 막고, `gfx/gpu.rs` 커서 결정부에서도 같은 플래그로
//!   패널 아래 surface 의 커서(I-beam 등)가 덮어써지지 않게 막는다(입력 소비가 아니라
//!   프레임 후처리 시각 갱신이라 mouse.rs 4지점과는 별도 카테고리).
//!
//! ## Model / Runtime / View 분리
//! - [`hold_reveal_alpha`] / [`default_rect`] / [`clamp_rect`] / [`resize_to`] — 순수 함수,
//!   테스트로 고정.
//! - [`ModifierHintRuntime`] — `AppState` 에 사는 홀드 상태 + 진행 중 드래그 working rect.
//! - [`draw_modifier_hint`] — 매 프레임 draw. `overlay::draw_overlays` 가 toast/banner 인접에서 호출한다.

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

/// modifier-hint 오버레이의 런타임 상태. `AppState` 필드(gui 전용).
///
/// 홀드 상태(시작 시각·눌린 조합·세션 dismiss)와 진행 중 드래그 working rect 를 담는다.
/// 지오메트리 영속값은 `Settings::modifier_hint`(pos/size)에 있고, working 은 드래그 중의
/// 임시 실시간 rect 다(놓는 시점에 Settings 로 커밋).
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
    /// 홀드 상태를 갱신한다. `ModifiersChanged` 훅이 플랫폼 정규화된 축 bool 로 호출한다.
    ///
    /// - 하나도 안 눌림 → 전부 clear(+ dismissed 리셋). 반환 `true`(표시 갱신 필요).
    /// - 하나 이상 눌림 → 현재 눌린 4축을 그대로 `held` 조합으로 저장. **조합이 바뀌면 항상
    ///   dirty(=`true`)** 를 반환해 즉시 콘텐츠를 좁힌다(예: Ctrl→Ctrl+Shift). 타이머
    ///   (`hold_since`)는 최초 press 에만 시작하고 조합이 바뀌어도 **리셋하지 않는다**.
    ///
    /// 반환 = 상태가 바뀌어 redraw 가 필요한지.
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

    /// 등록된 단축키가 실제로 소비된 시점에 호출한다. 아직 표시 지연 게이트를 통과하지
    /// 않은 홀드라면(=패널이 아직 안 뜬 상태) 타이머를 지금부터 다시 잰다 — 단축키를
    /// 계속 쓰는 동안에는 패널이 뜨지 않게 하기 위함(오버레이의 목적은 발견성이고, 등록
    /// 단축키를 실제로 실행한 사용자에겐 그 시점에 도움말이 불필요하다).
    ///
    /// 이미 게이트를 통과한(=표시 중인) 홀드는 건드리지 않는다 — 뜬 패널을 숨겼다 다시
    /// 띄우지 않는다. 홀드 중이 아니면(`held`/`hold_since` 가 `None`) no-op — modifier
    /// 없이 실행된 단축키가 새 홀드를 만들면 안 된다. `dismissed`/`working`/`held` 는
    /// 건드리지 않는다(전부 리셋 대상 아님).
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

/// Debug 전용 홀드 상태 조작·관찰 — `debug.modifier_hint.*` IPC 표면이 쓴다.
///
/// 원칙1상 오버레이는 실 modifier 홀드(`ModifiersChanged`)로만 뜨고 IPC 로 강제 표시할 수
/// 없다. 이 접근자들은 `host_popup.open`(사용자 클릭 우회 force-open)과 동일 성격의 debug
/// 격리 표면으로, 오버레이 내부 홀드 상태만 세팅/덤프한다(PTY raw 주입 아님). release
/// 미노출. 테스트도 같은 손잡이로 홀드 시각을 당기므로 `test` 에도 산다 — 테스트는 release
/// 산출물이 아니다.
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

    /// `hold_since` 를 `elapsed` 만큼 과거로 백데이트 → 표시 지연 게이트를 즉시 통과시킨다
    /// (스크린샷·상태 검증용). `Instant::checked_sub` 로 플랫폼별 하한을 안전 처리한다.
    pub fn debug_backdate(&mut self, elapsed: std::time::Duration) {
        if let Some(s) = self.hold_since {
            self.hold_since = Some(s.checked_sub(elapsed).unwrap_or(s));
        }
    }
}

/// Debug 전용 — 오버레이 렌더 상태를 draw 경로와 동일 로직으로 재평가해 JSON 덤프.
///
/// `debug.modifier_hint.state` 가 쓴다. `reveal_delay_ms`(Shift 단독 판정) · `hold_reveal_alpha`
/// · `build_hint_sections`(좁힘) · `combo_keycaps` 를 그대로 재사용하므로, 스크린샷 없이도
/// "무엇이 어떻게 표시되는가" 를 자동 단정할 수 있다. release 미노출(테스트는 위와 같은 이유로
/// 부른다).
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
                "rows": s.rows.iter().map(|r| prettify_binding(binding_leaf(&r.binding))).collect::<Vec<_>>(),
                "roles": s.roles.iter().map(|r| r.desc_key()).collect::<Vec<_>>(),
                // ADR-0038: 빈 조합 섹션은 draw 가 "바인딩 없음" 플레이스홀더로 렌더한다.
                // 렌더 픽셀 없이 빈-플레이스홀더 표시를 자동 단정할 수 있게 플래그를 노출.
                "empty": s.is_empty(),
            })
        })
        .collect();
    // draw 의 방어 가드(`if sections.is_empty() { return }`)와 정확히 동일한 조건.
    // ADR-0038 이후 빈 조합 홀드도 빈 섹션(플레이스홀더)이 남아 sections 가 비지 않으므로,
    // 이전과 달리 미할당 조합에서도 visible:true 로 뜬다.
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

/// 홀드 경과시간(ms) → 오버레이 alpha. `None` = 아직 표시 안 함(지연 전).
///
/// - `held_ms < delay_ms` → `None`(홀드 지연 게이트 통과 전 — 실수 스침 억제).
/// - `reduced_motion` → 게이트 통과 즉시 `Some(1.0)`(페이드 생략, **지연은 유지**).
/// - 그 외 → `[delay, delay+fade]` 구간에서 opacity **0.2→1.0** 선형 페이드.
///
/// 디자인 확정: 등장은 투명도 80%→0%(alpha 0.2→1.0). `delay_ms`/`fade_ms` 는 Theme 토큰
/// (`modhint_hold_delay` · `motion_hold_reveal_shift` / `modhint_fade`)에서 주입 — 순수 함수라 테스트로 고정한다.
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
        // 0.2 → 1.0 선형.
        Some((0.2 + 0.8 * (t / fade_ms)).clamp(0.2, 1.0))
    }
}

/// 홀드 조합별 표시 지연(ms). **Shift 단독**(shift 만 눌리고 ctrl/alt/option 모두 미눌림)이면
/// 1200ms, 그 외 조합은 500ms.
///
/// 타이핑 중 Shift 로 팝업이 튀는 문제를 완화한다(Shift 는 대문자·기호 입력에 상시 쓰여
/// 스침이 잦음). Shift 를 포함하되 다른 modifier 도 눌린 조합(Ctrl+Shift 등)은 의도적
/// 단축키 조합이라 기본 지연을 유지한다. 매 프레임 현재 조합으로 재평가되므로, Shift 단독
/// 1.5초 뒤 Ctrl 을 추가하면 지연이 500ms 로 떨어지고 경과(1.5s) > 500ms 라 즉시 표시된다.
/// `delay_ms` 는 Theme 토큰(`modhint_hold_delay`/`motion_hold_reveal_shift`)에서 온다.
fn reveal_delay_ms(held: Combo, theme: &Theme) -> f32 {
    // `hold_reveal_alpha` 가 단위 없는 ms 산술을 하는 순수 함수라 여기서 벗겨 넘긴다 —
    // `Millis` 의 범위는 `Theme` 경계까지이고, 그 바깥의 순수 함수는 f32 ms 로 둔다.
    if held.shift && !held.ctrl && !held.alt && !held.option {
        theme.motion_hold_reveal_shift().to_millis_f32()
    } else {
        theme.modhint_hold_delay().to_millis_f32()
    }
}

/// 저장된 위치/크기가 없을 때의 기본 지오메트리 — **사이드바 하단 anchor**(접힘/펼침 무관
/// 동일). 화면 좌하단에 margin 을 두고 180×400(min 아님, 기본값)으로 배치한다.
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

// ── View (draw) ──────────────────────────────────────────────────────────────

use std::time::Duration;

use crate::i18n::t;
use tasty_settings::Settings;
use tasty_ui_widgets::{ControlSize, IconButton, IconButtonVariant, kbd, kbd_parts};

use crate::adapters::ui::icons;

/// `draw_modifier_hint` 결과 — 입력 레이어 배선 + 지오메트리 영속.
#[derive(Default, Clone, Copy)]
pub struct HintDrawResult {
    /// 마우스가 패널 위 → 하위 surface 전파 차단(`AppState::modifier_hint_hovered`).
    pub hovered: bool,
    /// 드래그/리사이즈를 놓은 시점의 (pos, size). `Some` 이면 호출자가 `UpdateSettings` 로
    /// 영속한다(사용자 행동 → `from_user_menu`). `None` = 이번 프레임 변경 없음.
    pub persist: Option<((LogicalPx, LogicalPx), (LogicalPx, LogicalPx))>,
    /// 이번 프레임에 `modhint_layer` Area 를 실제로 그렸으면 그 `LayerId`(안 그렸으면
    /// `None` — 표시 조건 미충족으로 조기 반환한 프레임). 중앙 집중식 z-order
    /// 강제(`enforce_foreground_z_order`, `src/gfx/gpu/egui_bridge.rs`)가 쓴다.
    pub layer: Option<egui::LayerId>,
}

/// modifier-hint 오버레이를 매 프레임 그린다. `overlay::draw_overlays` 가 toast/banner 인접에서 호출.
///
/// 표시 조건: `enabled` && 홀드 중 && 홀드 500ms 경과 && !dismissed && 섹션 비어있지 않음.
/// 표시 안 하는 프레임엔 필요한 만큼만 `request_repaint(_after)` 를 예약해 유휴 CPU 낭비를
/// 막는다(지연 도달 시점 1회 예약 · 페이드 중에만 매 프레임).
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
        // 아직 지연 게이트 전 — 500ms 도달 시점에 깨어나도록 정확히 예약(busy-loop 아님).
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
    // 방어적 가드 — ADR-0038 이후 홀드 가능한 실경로에선 항상 최소 홀드 조합 자신의
    // 섹션이 남아 비지 않는다(빈 섹션도 유지되고 플레이스홀더로 렌더). 만일의 빈 목록엔
    // 빈 셸을 그리지 않고 조용히 빠진다.
    if sections.is_empty() {
        return result;
    }

    let screen = ctx.screen_rect();
    let base = rt
        .working
        .map(|(r, _)| r)
        .unwrap_or_else(|| rect_from_settings(settings, screen, theme));
    let render_rect = clamp_rect(base, screen);

    // 오버레이를 `egui::Area` 로 등록해 `Memory::Areas::order`(z-stack)에 편입시킨다
    // — bare layer(`ctx.layer_painter`/미등록 `Ui::new`)는 같은 `Order::Foreground`
    // 타이어 안에서도 Area 등록 레이어보다 항상 나중(=위)에 그려진다
    // (`egui::layers::GraphicLayers::drain`) — banner 가 같은 이유로 Area 등록으로
    // 바뀐 것(`banner.rs`)과 동일 근거. 등록 안 하면 Popup(Area 등록됨)이 먼저 열려
    // 있어도 이 오버레이가 항상 그 위에 그려져 [입력 계층 정책](../../../docs/architecture/input-layer.md)
    // (Popup=2 > Modifier-hint=2b) 을 위반한다. 프레임 종료 시점의 중앙 집중식
    // `enforce_foreground_z_order`(`src/gfx/gpu/egui_bridge.rs`) 가 이 레이어를 부모로,
    // 열린 popup 레이어들을 자식으로 `Context::set_sublayer` 로 묶어 Popup 이 항상 이
    // 레이어 바로 위에 오도록 고정한다(`move_to_top` 반복 호출로는 두 레이어의 상대
    // 순서를 못 만든다는 이유는 `enforce_foreground_z_order` 자신의 doc 참고).
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

            // ★ **이 순서가 계약이다.** 테두리를 셸에서 떼어 **맨 뒤에** 다시 그린다 —
            // `draw_content` 의 첫 동작이 드래그 스트립 배경을 `radius 0.0` 불투명으로
            // 채우는 것이라, 테두리가 먼저 그려져 있으면 그 구간이 지워진다(CSS 박스
            // 모델의 border-on-top 재현). 앞으로 당기면 오버레이 위쪽 몇 px 의 테두리만
            // 사라지고 컴파일도 시험도 멀쩡하다.
            // 지키는 것은 `source_guards::modifier_hint_paint_order` 다.
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

            // ── 인터랙션: 드래그 스트립(이동) · 코너 그립(리사이즈) · X(dismiss) ──
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

            // X 버튼 — 우상단 strip 안. 클릭 시 이번 홀드 세션 dismiss.
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

    // hover 판정(입력 레이어). 렌더 rect 전체를 소비 zone 으로.
    result.hovered = ctx
        .pointer_hover_pos()
        .is_some_and(|p| render_rect.contains(p));

    // 페이드 중에만 매 프레임 재그리기(완전 표시 후엔 입력 구동 repaint 에 맡겨 유휴 CPU 절약).
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

/// 셸(그림자 + 불투명 fill)을 painter 로 그린다 — 고정 크기라 Frame(콘텐츠 맞춤)
/// 대신 painter 직접 사용. 색은 `alpha` 곱(페이드).
///
/// 테두리는 여기서 안 그린다 — [`draw_shell_border`] 가 따로 그리고, **그 순서 계약의
/// 자리는 셋을 잇달아 부르는 [`draw_modifier_hint`] 안**이다. 계약 본문과 무엇이 그것을
/// 지키는지는 거기 적혀 있다.
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

/// 패널 테두리만 그린다 — 셸에서 테두리만 떼어낸 짝이라 `radius` 를 그대로 재사용해
/// 둥근 모서리가 별도 처리 없이 복원된다.
///
/// **`draw_content` 이후에 불러야 한다.** 왜 그런지와 무엇이 그것을 지키는지는 순서를
/// 실제로 정하는 자리 — [`draw_modifier_hint`] 의 세 호출 위 — 에 적혀 있다.
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

    // 드래그 스트립 배경 + 하단 separator.
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
            theme.modhint_separator().to_egui().gamma_multiply(alpha),
        ),
    );

    // 스트립 내용: held 조합 Kbd + "held" 라벨 (좌). X 는 호출측.
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

    // 스크롤 리스트.
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

    // 이 오버레이는 **modifier 를 홀드한 채** 떠 있는 특수 팝업이다. egui 는 Ctrl+휠을 zoom,
    // Shift+휠을 가로 스크롤로 재해석하므로, 홀드 상태에서 세로 `ScrollArea` 가 안 움직인다.
    // 따라서 포인터가 패널 위일 때는 modifier 를 무시한 **순수 세로 휠량**을 직접 계산해
    // ScrollArea 에 주입한다. alt/option 단독은 egui 가 이미 세로로 처리하므로 이중 스크롤을
    // 피해 zoom/가로로 전용되는 modifier(Ctrl/Cmd/Shift) 홀드 시에만 주입한다.
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
            // 좌우/상하 패딩은 Frame inner_margin 으로(ScrollArea 뷰포트 내부 → 절대 rect
            // 수동 배치 대신 idiomatic flow). 세로 섹션 간격은 spacing.
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

/// 포인터가 `rect`(패널) 위일 때, **modifier 를 무시한** 이번 프레임의 순수 세로 휠량(포인트)을
/// 돌려준다. 그 외(포인터가 밖이거나 전용 modifier 없음)엔 `0.0`.
///
/// egui 는 `Ctrl/Cmd+휠`을 zoom, `Shift+휠`을 가로 스크롤로 바꾸므로([`InputState::begin_pass`]
/// 의 MouseWheel 처리) 홀드 상태의 세로 `ScrollArea` 가 갱신되지 않는다. 여기서는 raw
/// `Event::MouseWheel` 을 modifier 무관하게 다시 읽어 egui 와 **동일한 단위 스케일**(Point 그대로 /
/// Line×`line_scroll_speed` / Page×화면높이)로 세로 성분만 합산한다. 부호는 egui 의 `smooth_scroll_delta`
/// 와 같은 규약이라 [`egui::Ui::scroll_with_delta`] 에 그대로 넣으면 무-modifier 휠과 동일하게 스크롤된다.
///
/// alt/option 단독은 egui 가 이미 세로로 처리하므로, 이중 스크롤을 피하려 zoom/가로로 전용되는
/// modifier(Ctrl·Cmd·Shift) 가 있을 때만 값을 낸다.
pub(crate) fn modifier_free_wheel_y(ctx: &egui::Context, rect: egui::Rect) -> f32 {
    let pointer_over = ctx.pointer_hover_pos().is_some_and(|p| rect.contains(p));
    if !pointer_over {
        return 0.0;
    }
    let line_speed = ctx.options(|o| o.line_scroll_speed);
    let page = ctx.screen_rect().height();
    ctx.input(|i| {
        let m = i.modifiers;
        // egui 가 세로 휠을 전용해 버리는 modifier 만 대상(그 외는 egui 세로 스크롤이 정상 동작).
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

/// 한 조합 섹션 — ChordHead(Kbd + separator) + HintRow* + RoleRow*.
fn draw_section(
    ui: &mut egui::Ui,
    theme: &Theme,
    sec: &HintSection,
    general: &tasty_settings::GeneralSettings,
) {
    // ChordHead.
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
            theme.modhint_separator().to_egui(),
        ),
    );
    // 빈 조합 섹션은 내부 간격을 좁게(3px vs 6px) 잡아, 항상 표시되는 빈 섹션이
    // 리스트를 과하게 늘어뜨리지 않게 한다(ADR-0038, 디자인 §6-5). 섹션 간 간격은 불변.
    let content_gap = if sec.is_empty() {
        theme.modhint_empty_row_gap().value()
    } else {
        theme.modhint_row_gap().value()
    };
    ui.add_space(content_gap);

    ui.spacing_mut().item_spacing.y = content_gap;
    if sec.is_empty() {
        // 바인딩·역할이 모두 없는 조합 → "바인딩 없음" 플레이스홀더 한 줄(ADR-0038).
        draw_empty_row(ui, theme);
    } else {
        for row in &sec.rows {
            draw_row(ui, theme, row);
        }
        for role in &sec.roles {
            draw_role_row(ui, theme, *role);
        }
    }
}

/// 빈 조합 플레이스홀더 행 — muted 텍스트("바인딩 없음") 한 줄. 부재 신호라 리스트에서
/// 가장 조용하다: 키캡 없음 · wash 없음 · leading 글리프 없음 · 호버/포커스 없음(정적).
/// 디자인 `explorations/modifier-hint-empty-section.html` `.mh-empty` 전사.
fn draw_empty_row(ui: &mut egui::Ui, theme: &Theme) {
    ui.horizontal(|ui| {
        // 키캡 행(24px)보다 타이트한 20px 최소 높이(디자인 §6-5).
        ui.set_min_height(theme.modhint_empty_row_min_height().value());
        ui.add(
            egui::Label::new(
                egui::RichText::new(t("modifier_hint.empty"))
                    .size(theme.font_size_body.value())
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
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let (label, is_plugin) = row_label(&row.source);
        if is_plugin {
            let d = theme.status_dot_size().value();
            let (r, _) = ui.allocate_exact_size(egui::vec2(d, d), egui::Sense::hover());
            ui.painter()
                .circle_filled(r.center(), d * 0.5, theme.modhint_agent_dot().to_egui());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            kbd(ui, theme, &prettify_binding(binding_leaf(&row.binding)));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(label)
                            .size(theme.font_size_body.value())
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
                    // 카테고리 전환 → folder 글리프(디자인 E, mhIc.folder).
                    HintRole::CategorySwitch => {
                        icons::FOLDER.image(gsz, col).paint_at(ui, r);
                    }
                    // tab/workspace/link 역할 → 숫자 오버레이 의미의 "#" 글리프.
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
                            .size(theme.font_size_body.value())
                            .color(theme.modhint_row_fg().to_egui()),
                    )
                    .wrap(),
                );
            });
        });
}

/// 조합 → `"Ctrl+Shift"` 형태 키캡 문자열(우선순위 순서). Alt/Option/Shift 토큰 텍스트는
/// `general`(`GeneralSettings::{alt,option,shift}_display_style`,
/// `docs/design/policies/key-mapping.md` 참고)을 따른다 —
/// `KeybindingSettings::format_display_parts` 와 동일한 스타일 규약(alt: Alt/Cmd/⌘,
/// option: Option/⌥, shift: Shift/⇧)이며, 여기 하드코딩됐던 `cfg!(target_os = "macos")`
/// alt→"Cmd" 치환은 이 설정으로 대체됐다(기본값 "alt" — 표시 스타일을 만지지 않은
/// 사용자는 업그레이드 후 Alt/Cmd 표기가 바뀔 수 있음, 설정 > 일반 > 표시에서 다시
/// 선택 가능). **JSON 직렬화 경로 전용**(`debug_state_json`) — 화면 렌더링(스트립
/// 헤더·섹션 헤더)은 tofu box 위험이 있는 심볼 스타일을 텍스트가 아니라 아이콘으로
/// 그려야 해서 [`combo_keycap_parts`] 로 분리됐다. 호출처가 `debug_state_json` 하나뿐이라
/// release 빌드에는 포함하지 않는다(그 함수와 같은 cfg).
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
        parts.push(match general.option_display_style.as_str() {
            "symbol" => "⌥",
            _ => "Option",
        });
    }
    if c.shift {
        parts.push(match general.shift_display_style.as_str() {
            "symbol" => "⇧",
            _ => "Shift",
        });
    }
    parts.join("+")
}

/// [`combo_keycaps`] 의 화면 렌더링 경로 버전 — `"symbol"` 스타일(⌘/⌥/⇧)은 텍스트가
/// 아니라 [`icons::CMD_KEY`]/[`OPTION_KEY`]/[`SHIFT_KEY`] 벡터 아이콘으로 대체한다.
/// [`combo_keycaps`](JSON 직렬화 경로, `debug_state_json`)는 그대로 텍스트를 유지 —
/// 두 경로가 갈리는 이유는 픽셀을 그리지 않는 JSON 덤프엔 tofu box 위험이 없어서다.
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
        match general.option_display_style.as_str() {
            "symbol" => parts.push(KbdKey::Icon(icons::OPTION_KEY)),
            _ => parts.push(KbdKey::Text("Option")),
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

/// 바인딩 문자열(`"ctrl+shift+t"`) → 키캡 표기(`"Ctrl+Shift+T"`). 세그먼트별 첫 글자 대문자.
///
/// 표시 스타일 파라미터가 없다 — 두 호출부(`debug_state_json`/`draw_row`) 모두
/// `binding_leaf(&row.binding)` 로 modifier 를 이미 전부 벗겨낸 leaf 키만 넘긴다
/// (`binding_leaf` 테스트 참고). modifier 토큰을 볼 일이 없어 alt/option/shift 표시
/// 스타일과 무관하다.
fn prettify_binding(binding: &str) -> String {
    binding
        .split('+')
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// 행 출처 → (표시 라벨, plugin 여부). 라벨 해석은 이 오버레이 책임(모델은 키만 반환).
fn row_label(source: &HintRowSource) -> (String, bool) {
    match source {
        HintRowSource::Host { label_key } => (t(label_key).to_string(), false),
        // ScriptRegistry 이름 해석은 draw 경로에 미도달 → script_id 표기(후속 배선 대상).
        HintRowSource::Script { script_id } => (script_id.clone(), false),
        HintRowSource::Plugin {
            plugin_id,
            title_i18n_key,
        } => (format!("{plugin_id}: {}", t(title_i18n_key)), true),
    }
}

/// `modhint_layer` Area 안에 주어진 rect 로 자식 Ui 를 만든다 — `banner.rs::ui_at`
/// 와 동일 패턴(부모 Area 의 `Memory::Areas::order` 편입을 그대로 물려받는다).
/// clip_rect 를 rect 로 좁혀 Area 기본 clip(화면 전체) 밖으로 넘치지 않게 한다.
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
    fn alpha_hidden_before_delay() {
        assert_eq!(hold_reveal_alpha(0.0, DELAY, FADE, false), None);
        assert_eq!(hold_reveal_alpha(499.0, DELAY, FADE, false), None);
    }

    #[test]
    fn alpha_starts_at_0_2_and_reaches_1_0() {
        // t=500 → 0.2, t=600 → 0.6, t=700 → 1.0, 이후 1.0 고정.
        assert_eq!(hold_reveal_alpha(500.0, DELAY, FADE, false), Some(0.2));
        let mid = hold_reveal_alpha(600.0, DELAY, FADE, false).unwrap();
        assert!((mid - 0.6).abs() < 1e-4, "t=600 alpha={mid}");
        assert_eq!(hold_reveal_alpha(700.0, DELAY, FADE, false), Some(1.0));
        assert_eq!(hold_reveal_alpha(1000.0, DELAY, FADE, false), Some(1.0));
    }

    #[test]
    fn alpha_reduced_motion_snaps_at_delay_boundary() {
        // 지연은 유지(499 → None), 게이트 통과 즉시 1.0(페이드 생략).
        assert_eq!(hold_reveal_alpha(499.0, DELAY, FADE, true), None);
        assert_eq!(hold_reveal_alpha(500.0, DELAY, FADE, true), Some(1.0));
        assert_eq!(hold_reveal_alpha(600.0, DELAY, FADE, true), Some(1.0));
    }

    #[test]
    fn update_hold_stores_combo_and_dirties_on_change_keeping_timer() {
        let mut rt = ModifierHintRuntime::default();
        // Ctrl 누름 → held={ctrl}, 타이머 시작, dirty.
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
        // Ctrl 유지하며 Shift 추가 → 조합이 바뀌므로 dirty=true(즉시 좁힘), 타이머 리셋 안 함.
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
        // 같은 조합 재입력 → dirty=false(불필요 redraw 억제).
        assert!(!rt.update_hold(true, false, false, true));
    }

    #[test]
    fn update_hold_follows_combo_when_axis_released() {
        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(true, false, false, true); // Ctrl+Shift
        let t0 = rt.hold_since;
        // Ctrl 뗌, Shift 유지 → held={shift} 로 즉시 따라감(anchor 개념 없음), 타이머 유지.
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
        // Shift 단독 → 1200ms.
        assert_eq!(reveal_delay_ms(shift, &theme), 1200.0);
        // Shift + 다른 축 → 기본 500ms.
        let ctrl_shift = Combo {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(reveal_delay_ms(ctrl_shift, &theme), 500.0);
        // Shift 없는 조합 → 500ms.
        let ctrl = Combo {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(reveal_delay_ms(ctrl, &theme), 500.0);
    }

    #[test]
    fn shift_only_1200ms_gate_hides_before_and_shows_after() {
        // 순수함수 hold_reveal_alpha 를 1200ms 지연으로 평가: 500ms→None, 1300ms→Some.
        assert_eq!(hold_reveal_alpha(500.0, 1200.0, FADE, false), None);
        assert!(hold_reveal_alpha(1300.0, 1200.0, FADE, false).is_some());
    }

    #[test]
    fn update_hold_clears_and_resets_dismiss_on_full_release() {
        let mut rt = ModifierHintRuntime::default();
        rt.update_hold(true, false, false, false);
        rt.dismissed = true;
        // 전부 뗌 → clear + dismissed 리셋.
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
        // 우하단 밖으로 나간 rect → 안쪽으로 이동, 크기 유지.
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
        // 크게 줄이는 delta → min 200×240 로 클램프.
        let shrunk = resize_to(r, egui::vec2(-500.0, -500.0), 200.0, 240.0);
        assert_eq!(shrunk.size(), egui::vec2(200.0, 240.0));
        assert_eq!(shrunk.min, r.min, "좌상단 고정");
        // 늘리는 delta → 그대로 반영.
        let grown = resize_to(r, egui::vec2(30.0, 20.0), 200.0, 240.0);
        assert_eq!(grown.size(), egui::vec2(250.0, 420.0));
    }

    // 빈 섹션 억제·정렬은 `build_hint_sections` 쪽 테스트(modifier_hint.rs)가 이미 본다. 여기서는
    // 오버레이가 그 결과를 그대로 소비함만 확인(계약 회귀 방지).
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

    /// ADR-0038: 바인딩·역할이 전무한 조합을 홀드해도 오버레이는 visible 이고, debug 덤프의
    /// 해당 섹션은 `empty:true` 로 표시된다(draw 가 "바인딩 없음" 플레이스홀더로 렌더).
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

        // 미할당 조합이라도 패널은 뜬다(이전엔 visible:false 였음).
        assert_eq!(v["visible"], serde_json::json!(true), "dump={v}");
        let sections = v["sections"].as_array().expect("sections 배열");
        // 홀드 조합(Ctrl+Alt) 섹션이 존재하고 empty:true. alt 축 라벨은 타깃 OS 가
        // 아니라 `general.alt_display_style`(기본 "alt") 이 정하므로, 기본 설정인
        // 여기서는 모든 플랫폼에서 "Alt" 다.
        let held_combo = "Ctrl+Alt";
        let ctrl_alt = sections
            .iter()
            .find(|s| s["combo"] == serde_json::json!(held_combo))
            .unwrap_or_else(|| panic!("{held_combo} 섹션 존재. dump={v}"));
        assert_eq!(ctrl_alt["empty"], serde_json::json!(true));
        assert!(ctrl_alt["rows"].as_array().unwrap().is_empty());
        assert!(ctrl_alt["roles"].as_array().unwrap().is_empty());
        // 모든 섹션이 빈 섹션이어야 한다(전부 미할당).
        assert!(
            sections
                .iter()
                .all(|s| s["empty"] == serde_json::json!(true)),
            "dump={v}"
        );
    }
}
