//! 사용자 행동의 결과를 잠시 보여 주는 토스트. 호출부는 에이전트 동작에 토스트를 만들지 않아야 한다.
//! 입력·포커스를 받지 않으며 관리자는 중복 합치기·만료·범위·개수 제한을 처리한다.
//! 렌더링은 갤러리와 같은 공용 위젯을 사용한다. docs/design/systems/toast.md 참고.

use std::time::{Duration, Instant};

use crate::theme;

use super::layout_context::LayoutContext;

pub use crate::model::toast_kind::{ToastKind, ToastScope};

/// 단일 토스트 인스턴스 상태.
#[derive(Debug, Clone)]
pub struct ToastState {
    pub id: u64,
    pub message: String,
    pub kind: ToastKind,
    /// 알림을 낸 동작의 단축키 키캡. 메뉴·마우스로 실행했고 binding이 있을 때만 채운다.
    pub hint: Vec<String>,
    pub scope: ToastScope,
    pub spawned_at: Instant,
    pub lifetime: Duration,
}

const DEFAULT_LIFETIME: Duration = Duration::from_millis(2000);
/// 같은 범위·문구가 이 시간 안에 다시 오면 수명만 갱신한다.
const COALESCE_WINDOW: Duration = Duration::from_millis(500);
/// 스코프당 최대 동시 표시 개수.
const MAX_PER_SCOPE: usize = 5;
/// 문자 수가 이 상한을 넘으면 앞부분과 안내 접미만 표시한다.
/// 번역문을 만들 때와 같은 TOAST_MAX_CHARS를 사용한다.
const MAX_MESSAGE_CHARS: usize = crate::i18n::TOAST_MAX_CHARS;
use tasty_ui_widgets::{
    TOAST_FADE_IN_MS as FADE_IN_MS, TOAST_FADE_OUT_MS as FADE_OUT_MS, toast_fade_alpha,
};
pub use tasty_ui_widgets::{ToastEntryView, ToastScopeView, ToastStackBottom, ToastViewProps};

/// 공용 위젯에 Tooltip 레이어 painter를 전달해 다른 UI 위에 토스트를 그린다.
/// 그린 카드마다 토스트 id와 egui 사각형을 돌려준다.
pub fn draw_toast_view(ctx: &egui::Context, props: &ToastViewProps<'_>) -> Vec<(u64, egui::Rect)> {
    if props.scopes.is_empty() {
        return Vec::new();
    }

    let layer_id = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("toast_layer"));
    let painter = ctx.layer_painter(layer_id);
    tasty_ui_widgets::draw_toast_scopes(&painter, props)
}

/// 토스트 hint에 넣을 `binding_id` 동작의 첫 단축키 키캡. binding이 비었으면 빈 목록이다.
/// 메뉴·마우스로 실행한 동작에만 쓴다. 키를 눌러 실행했다면 사용자가 이미 키를 안다.
pub fn binding_hint(settings: &tasty_settings::Settings, binding_id: &str) -> Vec<String> {
    settings
        .keybindings
        .get_bindings(binding_id)
        .and_then(|b| b.first())
        .map(|b| tasty_settings::KeybindingSettings::format_display_parts(b, &settings.general))
        .unwrap_or_default()
}

pub struct ToastManager {
    toasts: Vec<ToastState>,
    next_id: u64,
    /// 새 토스트/coalesce 갱신에 부여할 수명. `Settings.overlay.toast_duration_ms`
    /// 에서 매 프레임 동기화된다(설정 미로드 시 [`DEFAULT_LIFETIME`]).
    lifetime: Duration,
    /// 마지막 draw에서 그린 카드 영역(egui 논리 좌표). 토스트가 없으면 비어 있다.
    /// WebView 동기화가 이 영역과 겹치는 native 화면만 숨긴다.
    card_rects: Vec<crate::model::LogicalRect>,
    /// 마지막 draw에서 그린 카드의 토스트 id와 egui 사각형. 다음 draw가 포인터 아래 카드를 찾는다.
    drawn_cards: Vec<(u64, egui::Rect)>,
    /// 마지막 draw에서 포인터 아래에 있던 카드의 토스트 id. 그 뒤 프레임이 쉬는 동안에도
    /// 포인터는 그 자리에 있었으므로 다음 draw에서도 붙잡는다.
    hovered_last: Vec<u64>,
    /// 설정값으로 수명을 맞춘 적이 있는지. 보조 창이 설정 파일을 창마다 한 번만 읽게 한다.
    lifetime_synced: bool,
    /// Window·Workspace 범위 스택의 하단 기준. 메인 창은 상태바, Settings 창은 하단 버튼 줄을 피한다.
    window_bottom: ToastStackBottom,
}

impl ToastManager {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 1,
            lifetime: DEFAULT_LIFETIME,
            card_rects: Vec::new(),
            drawn_cards: Vec::new(),
            hovered_last: Vec::new(),
            lifetime_synced: false,
            window_bottom: ToastStackBottom::Window,
        }
    }

    /// Settings 창의 관리자. 창 범위 스택을 하단 버튼 줄(Cancel · Save) 위에 쌓는다.
    pub fn for_settings_window() -> Self {
        Self {
            window_bottom: ToastStackBottom::SettingsWindow,
            ..Self::new()
        }
    }

    /// 마지막 draw에서 그린 카드 영역. 카드가 떠 있는 동안만 비어 있지 않다.
    pub fn card_rects(&self) -> &[crate::model::LogicalRect] {
        &self.card_rects
    }

    /// 토스트 수명을 설정값(ms)으로 동기화한다. 이미 떠 있는 토스트에는 소급하지
    /// 않고, 이후 push/coalesce 되거나 포인터에 붙잡힌 토스트부터 적용된다. draw 직전에 호출한다.
    pub fn set_lifetime_ms(&mut self, ms: u64) {
        self.lifetime = Duration::from_millis(ms);
        self.lifetime_synced = true;
    }

    /// 아직 설정값으로 수명을 맞춘 적이 없을 때만 `ms()`로 맞춘다. 이미 맞췄으면 `ms`를 부르지 않는다.
    pub fn set_lifetime_ms_once(&mut self, ms: impl FnOnce() -> u64) {
        if !self.lifetime_synced {
            self.set_lifetime_ms(ms());
        }
    }

    #[cfg(test)]
    pub(crate) fn card_lifetimes(&self) -> Vec<Duration> {
        self.toasts.iter().map(|t| t.lifetime).collect()
    }

    /// 사용자 행동의 결과로 토스트를 추가한다.
    pub fn push(&mut self, message: impl Into<String>, kind: ToastKind, scope: ToastScope) {
        self.push_with_hint(message, kind, Vec::new(), scope);
    }

    /// `hint`(동작의 단축키 키캡)를 붙여 토스트를 추가한다. 키보드로 실행한 동작이나
    /// binding이 빈 동작에는 빈 목록을 넘긴다. docs/design/systems/toast.md 참고.
    pub fn push_with_hint(
        &mut self,
        message: impl Into<String>,
        kind: ToastKind,
        hint: Vec<String>,
        scope: ToastScope,
    ) {
        let message = truncate_message(message.into());
        let now = Instant::now();

        // 포인터 아래 카드는 draw가 쉬는 동안에도 붙잡힌 상태라 나이와 관계없이 합친다.
        let held = &self.hovered_last;
        if let Some(existing) = self.toasts.iter_mut().rev().find(|t| {
            t.scope == scope
                && t.message == message
                && (held.contains(&t.id) || now.duration_since(t.spawned_at) < COALESCE_WINDOW)
        }) {
            // 등장 중인 카드는 그대로 둔다. 이미 불투명한 카드를 `now`로 되돌리면 alpha가 0이 되어
            // 한 프레임 사라졌다가 다시 페이드인한다.
            if now.duration_since(existing.spawned_at) >= fade_in() {
                existing.spawned_at = full_opacity_start(now);
            }
            existing.kind = kind;
            existing.hint = hint;
            existing.lifetime = self.lifetime;
            return;
        }

        let id = self.next_id;
        self.next_id += 1;

        self.toasts.push(ToastState {
            id,
            message,
            kind,
            hint,
            scope: scope.clone(),
            spawned_at: now,
            lifetime: self.lifetime,
        });

        let count_in_scope = self.toasts.iter().filter(|t| t.scope == scope).count();
        if count_in_scope > MAX_PER_SCOPE
            && let Some(idx) = self.toasts.iter().position(|t| t.scope == scope)
        {
            self.toasts.remove(idx);
        }
    }

    /// 테스트에서 현재 토스트 수를 확인한다.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.toasts.len()
    }

    /// 테스트에서 생성 순서의 문구를 확인한다.
    #[cfg(test)]
    pub(crate) fn messages(&self) -> Vec<&str> {
        self.toasts.iter().map(|t| t.message.as_str()).collect()
    }

    /// 편의 헬퍼: Info 토스트.
    pub fn push_info(&mut self, message: impl Into<String>, scope: ToastScope) {
        self.push(message, ToastKind::Info, scope);
    }

    /// 만료·표시 범위를 정리하고 공용 위젯으로 그린다. 모션 감소이면 페이드를 생략한다.
    pub fn draw(&mut self, ctx: &egui::Context, draw_ctx: &LayoutContext, reduced_motion: bool) {
        self.draw_at(ctx, draw_ctx, reduced_motion, Instant::now());
    }

    /// `now` 시각으로 [`Self::draw`]를 한다. 시험이 시간을 정할 수 있게 분리했다.
    fn draw_at(
        &mut self,
        ctx: &egui::Context,
        draw_ctx: &LayoutContext,
        reduced_motion: bool,
        now: Instant,
    ) {
        // 만료 판정 전에 붙잡아야 페이드아웃이 끝나기 직전 카드도 되살아난다. 포인터 좌표와
        // 지난 프레임 사각형만 비교하므로 클릭·드래그·포커스는 아래로 그대로 간다.
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let hovered = hovered_ids(&self.drawn_cards, pointer);
        // 모든 카드가 붙잡히면 프레임을 쉬므로 지난 draw 뒤의 시간도 hover로 친다. 그래야
        // 포인터가 떠나는 프레임에서 수명이 처음부터 흐른다.
        let mut held = std::mem::replace(&mut self.hovered_last, hovered.clone());
        held.extend(hovered.iter().copied());
        hold_hovered(&mut self.toasts, &held, now, self.lifetime);

        self.toasts.retain(|t| {
            let age = now.duration_since(t.spawned_at);
            age < t.lifetime + Duration::from_millis(FADE_OUT_MS as u64)
        });

        self.toasts
            .retain(|t| Self::scope_rect(&t.scope, draw_ctx, ctx).is_some());

        if self.toasts.is_empty() {
            self.card_rects.clear();
            self.drawn_cards.clear();
            self.hovered_last.clear();
            return;
        }

        if self.toasts.iter().all(|t| hovered.contains(&t.id)) {
            // 붙잡힌 카드는 불투명으로 멈춰 있다. 포인터가 움직이거나 창을 벗어나면 입력이
            // 프레임을 깨우므로 매 프레임 다시 그리지 않는다.
            ctx.request_repaint_after(self.lifetime);
        } else {
            // 살아있는 동안에는 매 프레임 다시 그려야 페이드가 보인다.
            ctx.request_repaint();
        }

        let th = theme::theme();
        self.toasts.sort_by_key(|t| t.id);
        let members: Vec<StackMember> = self
            .toasts
            .iter()
            .filter_map(|t| {
                Self::scope_rect(&t.scope, draw_ctx, ctx).map(|rect| StackMember {
                    id: t.id,
                    rect,
                    bottom: stack_bottom(&t.scope, self.window_bottom),
                })
            })
            .collect();
        let columns = merge_columns(&th, &members);

        // 한 열의 상한은 범위가 달라도 합쳐서 센다. 넘친 가장 오래된 카드는 바로 버린다.
        let dropped: Vec<u64> = columns
            .iter()
            .flat_map(|c| c.ids[..c.ids.len().saturating_sub(MAX_PER_SCOPE)].to_vec())
            .collect();
        self.toasts.retain(|t| !dropped.contains(&t.id));

        let scopes: Vec<ToastScopeView> = columns
            .iter()
            .map(|c| ToastScopeView {
                scope_rect: c.rect,
                bottom: c.bottom,
                entries: c
                    .ids
                    .iter()
                    .filter_map(|id| self.toasts.iter().find(|t| t.id == *id))
                    .map(|t| ToastEntryView {
                        id: t.id,
                        kind: t.kind,
                        message: t.message.clone(),
                        hint: t.hint.clone(),
                        alpha: compute_alpha(t, now, reduced_motion),
                    })
                    .collect(),
            })
            .collect();

        let props = ToastViewProps {
            theme: &th,
            scopes: &scopes,
        };
        self.drawn_cards = draw_toast_view(ctx, &props);
        self.card_rects = self
            .drawn_cards
            .iter()
            .map(|(_, r)| crate::model::LogicalRect {
                x: crate::model::LogicalPx(r.min.x),
                y: crate::model::LogicalPx(r.min.y),
                width: crate::model::LogicalPx(r.width()),
                height: crate::model::LogicalPx(r.height()),
            })
            .collect();
    }

    /// 스코프의 rect를 얻는다. Window/Workspace는 screen rect를 사용한다.
    fn scope_rect(
        scope: &ToastScope,
        draw_ctx: &LayoutContext,
        ctx: &egui::Context,
    ) -> Option<egui::Rect> {
        match scope {
            ToastScope::Window => Some(ctx.screen_rect()),
            ToastScope::Workspace(ws_idx) => {
                if *ws_idx == draw_ctx.active_workspace {
                    Some(ctx.screen_rect())
                } else {
                    None
                }
            }
            ToastScope::Pane(pane_id) => draw_ctx
                .pane_rects
                .iter()
                .find(|(id, _)| id == pane_id)
                .map(|(_, r)| *r),
            ToastScope::Surface(surface_id) => draw_ctx
                .surface_rects
                .iter()
                .find(|(id, _)| id == surface_id)
                .map(|(_, r)| *r),
        }
    }
}

/// 스택 맨 아래 카드의 기준. 창 전체 rect에 그리는 Window·Workspace는 창의 하단 기준
/// (`window_bottom`) 위에 쌓고, 화면 일부를 덮는 Pane·Surface는 가장자리 여백만 둔다.
fn stack_bottom(scope: &ToastScope, window_bottom: ToastStackBottom) -> ToastStackBottom {
    match scope {
        ToastScope::Window | ToastScope::Workspace(_) => window_bottom,
        ToastScope::Pane(_) | ToastScope::Surface(_) => ToastStackBottom::ScopeMargin,
    }
}

/// 열에 넣을 토스트 하나. `id` 오름차순으로 넘긴다.
struct StackMember {
    id: u64,
    rect: egui::Rect,
    bottom: ToastStackBottom,
}

/// 같은 모서리에 놓이는 스택을 합친 한 열. `ids`는 오래된 것부터다.
struct StackColumn {
    rect: egui::Rect,
    bottom: ToastStackBottom,
    ids: Vec<u64>,
}

/// 맨 아래 카드의 기준점이 같은 범위들을 한 열로 합친다. 오른쪽 아래 pane의 스택은
/// workspace 스택과 같은 점에 놓이므로 따로 그리면 카드가 겹친다. 합친 열은 가장 넓은
/// 범위의 영역을 써서 위쪽 경계가 좁은 범위에 잘리지 않게 한다.
fn merge_columns(th: &theme::Theme, members: &[StackMember]) -> Vec<StackColumn> {
    const SAME_POINT: f32 = 0.5;
    let area = |r: egui::Rect| r.width() * r.height();
    let mut columns: Vec<(egui::Pos2, StackColumn)> = Vec::new();
    for m in members {
        let anchor = tasty_ui_widgets::toast_stack_anchor(th, m.rect, m.bottom);
        match columns.iter_mut().find(|(a, _)| {
            (a.x - anchor.x).abs() < SAME_POINT && (a.y - anchor.y).abs() < SAME_POINT
        }) {
            Some((_, col)) => {
                if area(m.rect) > area(col.rect) {
                    col.rect = m.rect;
                    col.bottom = m.bottom;
                }
                col.ids.push(m.id);
            }
            None => columns.push((
                anchor,
                StackColumn {
                    rect: m.rect,
                    bottom: m.bottom,
                    ids: vec![m.id],
                },
            )),
        }
    }
    columns.into_iter().map(|(_, c)| c).collect()
}

/// 등장 페이드 시간.
fn fade_in() -> Duration {
    Duration::from_secs_f32(FADE_IN_MS / 1000.0)
}

/// 등장 페이드를 막 끝낸 시각. `spawned_at`을 이 값으로 두면 카드가 완전 불투명이 되고
/// 수명이 처음부터 흐른다. `now`로 두면 alpha가 0이라 카드가 그려지지 않는다.
fn full_opacity_start(now: Instant) -> Instant {
    now.checked_sub(fade_in()).unwrap_or(now)
}

/// 포인터가 위에 있는 카드의 토스트 id. 포인터가 창 밖이면 없다.
fn hovered_ids(drawn: &[(u64, egui::Rect)], pointer: Option<egui::Pos2>) -> Vec<u64> {
    let Some(pos) = pointer else {
        return Vec::new();
    };
    drawn
        .iter()
        .filter(|(_, r)| r.contains(pos))
        .map(|(id, _)| *id)
        .collect()
}

/// 포인터 아래 카드의 수명을 멈춘다. 매 프레임 생성 시각을 등장 페이드 끝으로 당겨 두므로
/// 카드는 불투명으로 남고, 포인터가 떠나면 전역 수명 `lifetime`이 처음부터 흐른다.
fn hold_hovered(toasts: &mut [ToastState], hovered: &[u64], now: Instant, lifetime: Duration) {
    for t in toasts.iter_mut().filter(|t| hovered.contains(&t.id)) {
        t.spawned_at = full_opacity_start(now);
        t.lifetime = lifetime;
    }
}

/// 상태의 시간 값을 공용 위젯의 fade_alpha에 전달한다.
pub fn compute_alpha(t: &ToastState, now: Instant, reduced_motion: bool) -> f32 {
    toast_fade_alpha(now.duration_since(t.spawned_at), t.lifetime, reduced_motion)
}

/// 문자 경계를 지켜 본문을 상한까지 줄이고, 상한 밖에 번역한 안내 접미를 붙인다.
/// 상한과 같으면 자르지 않는다. 중복 비교 전에 적용해 같은 긴 문구도 합칠 수 있게 한다.
fn truncate_message(message: String) -> String {
    if message.chars().count() <= MAX_MESSAGE_CHARS {
        return message;
    }
    let truncated: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
    let notice = crate::i18n::t_fmt("toast.char_limit_notice", &MAX_MESSAGE_CHARS.to_string());
    format!("{truncated}\n{notice}")
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn test_theme() -> Theme {
        tasty_themes::mocha_fallback()
    }

    #[test]
    fn truncate_over_limit_keeps_200_chars_and_appends_notice() {
        let out = truncate_message("あ".repeat(250));
        let first_line = out.lines().next().unwrap();
        assert_eq!(first_line.chars().count(), MAX_MESSAGE_CHARS);
        assert!(out.contains('\n'));
        assert!(!out.lines().last().unwrap().is_empty());
    }

    #[test]
    fn truncate_at_or_under_limit_unchanged() {
        let exact = "x".repeat(MAX_MESSAGE_CHARS);
        assert_eq!(truncate_message(exact.clone()), exact);
        let under = "y".repeat(10);
        assert_eq!(truncate_message(under.clone()), under);
    }

    #[test]
    fn truncate_no_panic_on_multibyte_boundary() {
        let out = truncate_message("한".repeat(300));
        assert_eq!(
            out.lines().next().unwrap().chars().count(),
            MAX_MESSAGE_CHARS
        );
    }

    fn mk_state(id: u64, kind: ToastKind, msg: &str) -> ToastState {
        ToastState {
            id,
            message: msg.to_string(),
            kind,
            hint: Vec::new(),
            scope: ToastScope::Window,
            spawned_at: Instant::now(),
            lifetime: DEFAULT_LIFETIME,
        }
    }

    #[test]
    fn compute_alpha_full_during_lifetime() {
        let t = mk_state(1, ToastKind::Info, "hello");
        // 100ms past spawn (FADE_IN_MS = 80) — full opacity
        let now = t.spawned_at + Duration::from_millis(100);
        let a = compute_alpha(&t, now, false);
        assert!((a - 1.0).abs() < 1e-3);
    }

    #[test]
    fn compute_alpha_fade_in() {
        let t = mk_state(1, ToastKind::Info, "hi");
        // 40ms past spawn → 40/80 = 0.5
        let now = t.spawned_at + Duration::from_millis(40);
        let a = compute_alpha(&t, now, false);
        assert!((a - 0.5).abs() < 1e-3);
    }

    #[test]
    fn compute_alpha_fade_out() {
        let t = mk_state(1, ToastKind::Info, "hi");
        // 2080ms past spawn → past lifetime, mid fade-out (80/160 = 0.5 done)
        let now = t.spawned_at + Duration::from_millis(2080);
        let a = compute_alpha(&t, now, false);
        assert!((a - 0.5).abs() < 1e-3);
    }

    #[test]
    fn compute_alpha_reduced_motion_no_fade() {
        let t = mk_state(1, ToastKind::Info, "hi");
        // 40ms past spawn — would be 0.5 with fade, but reduced_motion → 1.0
        let now_in = t.spawned_at + Duration::from_millis(40);
        assert!((compute_alpha(&t, now_in, true) - 1.0).abs() < 1e-3);
        // past lifetime → 0.0
        let now_out = t.spawned_at + Duration::from_millis(3000);
        assert_eq!(compute_alpha(&t, now_out, true), 0.0);
    }

    #[test]
    fn manager_push_assigns_unique_ids() {
        let mut mgr = ToastManager::new();
        mgr.push("a", ToastKind::Info, ToastScope::Window);
        mgr.push("b", ToastKind::Info, ToastScope::Window);
        assert_eq!(mgr.toasts.len(), 2);
        assert_ne!(mgr.toasts[0].id, mgr.toasts[1].id);
    }

    #[test]
    fn manager_coalesce_same_message_same_scope() {
        let mut mgr = ToastManager::new();
        mgr.push("dup", ToastKind::Info, ToastScope::Window);
        mgr.push("dup", ToastKind::Warning, ToastScope::Window);
        assert_eq!(mgr.toasts.len(), 1);
        assert_eq!(mgr.toasts[0].kind, ToastKind::Warning);
    }

    /// Given 등장 페이드를 지난 카드, When 같은 범위·문구가 합쳐진다, Then 카드는 불투명으로
    /// 남고 수명이 처음부터 흐른다.
    #[test]
    fn coalesce_keeps_an_opaque_card_opaque() {
        let mut mgr = ToastManager::new();
        mgr.push("dup", ToastKind::Info, ToastScope::Window);
        let shown = Instant::now() - Duration::from_millis(300);
        mgr.toasts[0].spawned_at = shown;
        mgr.push("dup", ToastKind::Info, ToastScope::Window);
        assert_eq!(mgr.toasts.len(), 1);
        let t = &mgr.toasts[0];
        let now = Instant::now();
        assert!((compute_alpha(t, now, false) - 1.0).abs() < 1e-3);
        assert!(t.spawned_at > shown, "수명이 갱신되지 않았다");
        // 남은 수명은 전역 수명에서 등장 페이드 몫만 뺀 값이다.
        let left = (t.spawned_at + t.lifetime).saturating_duration_since(now);
        assert!(
            left + fade_in() + Duration::from_millis(50) >= t.lifetime,
            "{left:?}"
        );
    }

    /// Given 등장 페이드 중인 카드, When 합쳐진다, Then 등장 시각을 바꾸지 않아 페이드가 이어진다.
    #[test]
    fn coalesce_leaves_a_fading_in_card_alone() {
        let mut mgr = ToastManager::new();
        mgr.push("dup", ToastKind::Info, ToastScope::Window);
        let spawned = mgr.toasts[0].spawned_at;
        mgr.push("dup", ToastKind::Warning, ToastScope::Window);
        assert_eq!(mgr.toasts[0].spawned_at, spawned);
        assert_eq!(mgr.toasts[0].kind, ToastKind::Warning);
    }

    #[test]
    fn manager_max_per_scope_evicts_oldest() {
        let mut mgr = ToastManager::new();
        for i in 0..(MAX_PER_SCOPE + 2) {
            mgr.push(format!("m-{i}"), ToastKind::Info, ToastScope::Window);
        }
        assert_eq!(mgr.toasts.len(), MAX_PER_SCOPE);
        assert!(mgr.toasts.iter().all(|t| t.message != "m-0"));
        assert!(mgr.toasts.iter().all(|t| t.message != "m-1"));
    }

    fn run_view(scopes: Vec<ToastScopeView>) {
        let ctx = egui::Context::default();
        let theme = test_theme();
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            let props = ToastViewProps {
                theme: &theme,
                scopes: &scopes,
            };
            draw_toast_view(ctx, &props);
        }));
    }

    #[test]
    fn window_wide_scopes_stack_above_the_window_offset_and_regions_keep_the_margin() {
        use ToastStackBottom::{ScopeMargin, SettingsWindow, Window};
        for window_bottom in [Window, SettingsWindow] {
            assert_eq!(
                stack_bottom(&ToastScope::Window, window_bottom),
                window_bottom
            );
            assert_eq!(
                stack_bottom(&ToastScope::Workspace(0), window_bottom),
                window_bottom
            );
            assert_eq!(
                stack_bottom(&ToastScope::Pane(1), window_bottom),
                ScopeMargin
            );
            assert_eq!(
                stack_bottom(&ToastScope::Surface(1), window_bottom),
                ScopeMargin
            );
        }
        assert_eq!(ToastManager::new().window_bottom, Window);
        assert_eq!(
            ToastManager::for_settings_window().window_bottom,
            SettingsWindow
        );
    }

    /// 오른쪽 아래 pane의 기준점은 상태바 위 workspace 기준점과 같다. 두 스택은 한 열이 되고
    /// 더 넓은 workspace 영역을 쓴다. 다른 모서리의 pane은 따로 남는다.
    #[test]
    fn stacks_on_the_same_corner_merge_into_one_column() {
        use tasty_ui_widgets::tokens::{TOAST_SCOPE_MARGIN, TOAST_STACK_OFFSET_BOTTOM};
        let th = test_theme();
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 800.0));
        let corner_pane = egui::Rect::from_min_max(
            egui::pos2(600.0, 400.0),
            egui::pos2(
                1200.0,
                800.0 - TOAST_STACK_OFFSET_BOTTOM + TOAST_SCOPE_MARGIN,
            ),
        );
        let left_pane = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(600.0, 776.0));
        let m = |id, rect, bottom| StackMember { id, rect, bottom };
        let columns = merge_columns(
            &th,
            &[
                m(1, corner_pane, ToastStackBottom::ScopeMargin),
                m(2, screen, ToastStackBottom::Window),
                m(3, left_pane, ToastStackBottom::ScopeMargin),
                m(4, corner_pane, ToastStackBottom::ScopeMargin),
            ],
        );
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].ids, vec![1, 2, 4]);
        assert_eq!(columns[0].rect, screen);
        assert_eq!(columns[0].bottom, ToastStackBottom::Window);
        assert_eq!(columns[1].ids, vec![3]);
    }

    /// 합친 열의 상한 5는 범위를 가리지 않고 센다. 넘치면 가장 오래된 카드부터 버린다.
    #[test]
    fn a_merged_column_keeps_the_newest_five_across_scopes() {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 800.0));
        let layout = LayoutContext {
            active_workspace: 0,
            active_workspace_id: 0,
            pane_rects: vec![(
                7,
                egui::Rect::from_min_max(egui::pos2(600.0, 400.0), egui::pos2(1200.0, 776.0)),
            )],
            surface_rects: Vec::new(),
            active_tabs: Vec::new(),
            active_tab_ids: Vec::new(),
            workspace_rect: None,
        };
        let mut mgr = ToastManager::new();
        for i in 0..4 {
            mgr.push_info(format!("ws {i}"), ToastScope::Workspace(0));
        }
        for i in 0..3 {
            mgr.push_info(format!("pane {i}"), ToastScope::Pane(7));
        }
        drop(ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| mgr.draw(ctx, &layout, true),
        ));
        assert_eq!(
            mgr.messages(),
            vec!["ws 2", "ws 3", "pane 0", "pane 1", "pane 2"]
        );
    }

    fn screen() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 800.0))
    }

    fn window_layout() -> LayoutContext {
        LayoutContext {
            active_workspace: 0,
            active_workspace_id: 0,
            pane_rects: Vec::new(),
            surface_rects: Vec::new(),
            active_tabs: Vec::new(),
            active_tab_ids: Vec::new(),
            workspace_rect: None,
        }
    }

    /// 포인터 이벤트 하나로 한 프레임을 그린다.
    fn frame(
        ctx: &egui::Context,
        mgr: &mut ToastManager,
        event: Option<egui::Event>,
        now: Instant,
        reduced_motion: bool,
    ) {
        let layout = window_layout();
        drop(ctx.run(
            egui::RawInput {
                screen_rect: Some(screen()),
                events: event.into_iter().collect(),
                ..Default::default()
            },
            |ctx| mgr.draw_at(ctx, &layout, reduced_motion, now),
        ));
    }

    fn card_of(mgr: &ToastManager, id: u64) -> egui::Rect {
        mgr.drawn_cards
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, r)| *r)
            .expect("card drawn")
    }

    fn alpha_of(mgr: &ToastManager, id: u64, now: Instant) -> f32 {
        let t = mgr.toasts.iter().find(|t| t.id == id).expect("toast alive");
        compute_alpha(t, now, false)
    }

    fn after(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// 카드 하나를 그려 두고 그 중심과 id를 돌려준다. 생성 시각은 `t0`이다.
    fn shown_card(ctx: &egui::Context, mgr: &mut ToastManager, t0: Instant) -> (u64, egui::Pos2) {
        mgr.push_info("held card", ToastScope::Window);
        let id = mgr.toasts[0].id;
        mgr.toasts[0].spawned_at = t0;
        frame(ctx, mgr, None, after(t0, 100), false);
        frame(ctx, mgr, None, after(t0, 100), false);
        (id, card_of(mgr, id).center())
    }

    /// Given 모든 카드가 붙잡혀 프레임이 쉰다, When 마지막 hover 프레임 1900ms 뒤 포인터가
    /// 떠난다, Then 떠난 프레임부터 전역 수명이 처음부터 흘러 수명 - 등장 페이드 동안 불투명이다.
    #[test]
    fn leaving_after_an_idle_hover_restarts_the_lifetime() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        let t0 = Instant::now() - Duration::from_secs(60);
        let (id, center) = shown_card(&ctx, &mut mgr, t0);
        let hover = after(t0, 500);
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerMoved(center)),
            hover,
            false,
        );
        let left = hover + Duration::from_millis(1900);
        frame(&ctx, &mut mgr, Some(egui::Event::PointerGone), left, false);
        let late = left + DEFAULT_LIFETIME - fade_in() - Duration::from_millis(20);
        frame(&ctx, &mut mgr, None, late, false);
        assert_eq!(mgr.len(), 1);
        assert!((alpha_of(&mgr, id, late) - 1.0).abs() < 1e-3);
        let gone = left + DEFAULT_LIFETIME + Duration::from_millis(FADE_OUT_MS as u64);
        frame(&ctx, &mut mgr, None, gone, false);
        assert_eq!(mgr.len(), 0);
    }

    /// Given hover draw 뒤 프레임이 600ms 쉰다, When 같은 범위·문구를 push한다, Then 새 카드 없이
    /// 붙잡힌 카드에 합쳐지고 불투명으로 남는다.
    #[test]
    fn an_idle_held_card_still_absorbs_the_same_message() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        let t0 = Instant::now() - Duration::from_secs(60);
        let (id, center) = shown_card(&ctx, &mut mgr, t0);
        let hover = Instant::now() - Duration::from_millis(600);
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerMoved(center)),
            hover,
            false,
        );
        mgr.push_info("held card", ToastScope::Window);
        assert_eq!(mgr.len(), 1);
        assert!((alpha_of(&mgr, id, Instant::now()) - 1.0).abs() < 1e-3);
    }

    /// Given 보조 창이 만드는 관리자(설정 창·프리셋·플러그인)와 전역 5000ms, When App이 이벤트 전에
    /// 넣는 값으로 맞춘 뒤 hover했다가 떠난다, Then 카드 수명은 기본 2000ms가 아니라 5000ms다.
    /// 보조 창 View는 GPU 없이 만들 수 없어 App의 배선 자체는 실제 실행으로 잰다.
    #[test]
    fn auxiliary_window_managers_restart_with_the_synced_global_lifetime() {
        let global = tasty_settings::Settings {
            overlay: tasty_settings::OverlaySettings {
                toast_duration_ms: 5000,
            },
            ..Default::default()
        };
        for mut mgr in [ToastManager::for_settings_window(), ToastManager::new()] {
            let ctx = egui::Context::default();
            assert_eq!(mgr.lifetime, DEFAULT_LIFETIME);
            mgr.set_lifetime_ms(global.overlay.toast_duration_ms);
            let t0 = Instant::now() - Duration::from_secs(60);
            let (_, center) = shown_card(&ctx, &mut mgr, t0);
            frame(
                &ctx,
                &mut mgr,
                Some(egui::Event::PointerMoved(center)),
                after(t0, 500),
                false,
            );
            frame(
                &ctx,
                &mut mgr,
                Some(egui::Event::PointerGone),
                after(t0, 600),
                false,
            );
            assert_eq!(mgr.card_lifetimes(), vec![Duration::from_millis(5000)]);
            frame(&ctx, &mut mgr, None, after(t0, 600 + 4000), false);
            assert_eq!(mgr.len(), 1);
        }
    }

    /// Given 2000ms 카드를 hover 중이다, When 전역 수명을 5000ms로 바꾸고 포인터가 떠난다,
    /// Then 떠난 뒤 새 전역 수명 5000ms를 기준으로 만료된다.
    #[test]
    fn a_held_card_takes_the_new_global_lifetime() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        mgr.set_lifetime_ms(2000);
        let t0 = Instant::now() - Duration::from_secs(60);
        let (id, center) = shown_card(&ctx, &mut mgr, t0);
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerMoved(center)),
            after(t0, 500),
            false,
        );
        mgr.set_lifetime_ms(5000);
        frame(&ctx, &mut mgr, None, after(t0, 600), false);
        let left = after(t0, 700);
        frame(&ctx, &mut mgr, Some(egui::Event::PointerGone), left, false);
        assert_eq!(mgr.card_lifetimes(), vec![Duration::from_millis(5000)]);
        let late = left + Duration::from_millis(4000);
        frame(&ctx, &mut mgr, None, late, false);
        assert_eq!(mgr.len(), 1);
        assert!((alpha_of(&mgr, id, late) - 1.0).abs() < 1e-3);
        let gone = left + Duration::from_millis(5000 + FADE_OUT_MS as u64);
        frame(&ctx, &mut mgr, None, gone, false);
        assert_eq!(mgr.len(), 0);
    }

    /// 모션 감소도 같다. 마지막 hover 1950ms 뒤 떠나도 떠난 프레임에서 불투명이다.
    #[test]
    fn reduced_motion_leaving_after_an_idle_hover_restarts_the_lifetime() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        let t0 = Instant::now() - Duration::from_secs(60);
        let (id, center) = shown_card(&ctx, &mut mgr, t0);
        let hover = after(t0, 500);
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerMoved(center)),
            hover,
            true,
        );
        let left = hover + Duration::from_millis(1950);
        frame(&ctx, &mut mgr, Some(egui::Event::PointerGone), left, true);
        let t = mgr.toasts.iter().find(|t| t.id == id).expect("alive");
        assert_eq!(compute_alpha(t, left, true), 1.0);
        let late = left + DEFAULT_LIFETIME - fade_in() - Duration::from_millis(20);
        assert_eq!(compute_alpha(t, late, true), 1.0);
    }

    /// 포인터 아래 카드만 붙잡는다. 페이드아웃 중이던 카드도 불투명으로 돌아온다.
    #[test]
    fn hold_hovered_pins_only_the_hovered_card_at_full_opacity() {
        let now = Instant::now() + Duration::from_secs(10);
        let fading = now - DEFAULT_LIFETIME - Duration::from_millis(80);
        let mut toasts = vec![
            mk_state(1, ToastKind::Info, "a"),
            mk_state(2, ToastKind::Info, "b"),
        ];
        toasts[0].spawned_at = fading;
        toasts[1].spawned_at = fading;
        assert!(compute_alpha(&toasts[0], now, false) < 0.6);
        hold_hovered(&mut toasts, &[1], now, DEFAULT_LIFETIME);
        assert!((compute_alpha(&toasts[0], now, false) - 1.0).abs() < 1e-3);
        let age = now.duration_since(toasts[0].spawned_at);
        assert!(age < toasts[0].lifetime + Duration::from_millis(FADE_OUT_MS as u64));
        assert_eq!(toasts[1].spawned_at, fading);
        // 모션 감소에서도 수명 안이다.
        assert_eq!(compute_alpha(&toasts[0], now, true), 1.0);
    }

    #[test]
    fn hovered_ids_needs_a_pointer_inside_a_card() {
        let r = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(50.0, 20.0));
        let drawn = [(4, r)];
        assert_eq!(hovered_ids(&drawn, Some(r.center())), vec![4]);
        assert!(hovered_ids(&drawn, Some(egui::pos2(0.0, 0.0))).is_empty());
        assert!(hovered_ids(&drawn, None).is_empty());
    }

    /// Given 떠 있는 토스트, When 포인터를 카드 위에 수명보다 오래 둔다, Then 불투명으로 남는다.
    /// When 포인터가 창을 벗어난다, Then 그때부터 전역 수명과 소멸 페이드 뒤 사라진다.
    #[test]
    fn a_hovered_toast_outlives_its_lifetime_and_restarts_when_left() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        mgr.set_lifetime_ms(2000);
        mgr.push_info("hold me", ToastScope::Window);
        let id = mgr.toasts[0].id;
        let t0 = mgr.toasts[0].spawned_at;
        frame(&ctx, &mut mgr, None, after(t0, 100), false);
        frame(&ctx, &mut mgr, None, after(t0, 100), false);
        let center = card_of(&mgr, id).center();

        for ms in [500, 1500, 2500, 4000, 6000] {
            frame(
                &ctx,
                &mut mgr,
                Some(egui::Event::PointerMoved(center)),
                after(t0, ms),
                false,
            );
            assert_eq!(mgr.len(), 1, "{ms}ms");
            assert!(
                (alpha_of(&mgr, id, after(t0, ms)) - 1.0).abs() < 1e-3,
                "{ms}ms"
            );
        }

        let left = after(t0, 6000);
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerGone),
            after(t0, 6010),
            false,
        );
        // 떠난 뒤 전역 수명 안에서는 남는다.
        frame(
            &ctx,
            &mut mgr,
            None,
            left + Duration::from_millis(1800),
            false,
        );
        assert_eq!(mgr.len(), 1);
        assert!((alpha_of(&mgr, id, left + Duration::from_millis(1800)) - 1.0).abs() < 1e-3);
        // 수명 + 소멸 페이드가 지나면 사라진다.
        frame(
            &ctx,
            &mut mgr,
            None,
            left + Duration::from_millis(2000 + 160 + 20),
            false,
        );
        assert_eq!(mgr.len(), 0);
    }

    /// Given 페이드아웃 중인 카드, When 포인터를 올린다, Then 완전 불투명으로 되살아난다.
    #[test]
    fn hovering_a_fading_card_revives_it() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        mgr.push_info("fading", ToastScope::Window);
        let id = mgr.toasts[0].id;
        let t0 = mgr.toasts[0].spawned_at;
        frame(&ctx, &mut mgr, None, after(t0, 2060), false);
        frame(&ctx, &mut mgr, None, after(t0, 2080), false);
        assert!(alpha_of(&mgr, id, after(t0, 2080)) < 0.6);
        let center = card_of(&mgr, id).center();
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerMoved(center)),
            after(t0, 2150),
            false,
        );
        assert_eq!(mgr.len(), 1);
        assert!((alpha_of(&mgr, id, after(t0, 2150)) - 1.0).abs() < 1e-3);
    }

    /// Given 같은 열에 카드 두 장, When 위 카드에만 포인터를 둔다, Then 아래 카드만 사라진다.
    #[test]
    fn only_the_hovered_card_of_a_column_is_held() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        mgr.push_info("older on top", ToastScope::Window);
        mgr.push_info("newer below", ToastScope::Window);
        let (top, below) = (mgr.toasts[0].id, mgr.toasts[1].id);
        let t0 = mgr.toasts[0].spawned_at;
        frame(&ctx, &mut mgr, None, after(t0, 100), false);
        frame(&ctx, &mut mgr, None, after(t0, 100), false);
        assert!(card_of(&mgr, top).bottom() < card_of(&mgr, below).top());
        let center = card_of(&mgr, top).center();
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerMoved(center)),
            after(t0, 3000),
            false,
        );
        assert_eq!(mgr.messages(), vec!["older on top"]);
        // 아래 카드가 사라지면 붙잡힌 카드가 앵커 쪽으로 내려와 포인터에서 벗어난다.
        assert!(card_of(&mgr, top).center().y > center.y);
        assert!(hovered_ids(&mgr.drawn_cards, Some(center)).is_empty());
        assert!(mgr.drawn_cards.iter().all(|(id, _)| *id != below));
    }

    /// Given 모션 감소, When hover한다, Then 남고, 떠나면 수명 뒤 페이드 없이 사라진다.
    #[test]
    fn reduced_motion_hover_holds_and_then_ends_without_fade() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        mgr.push_info("rm", ToastScope::Window);
        let id = mgr.toasts[0].id;
        let t0 = mgr.toasts[0].spawned_at;
        frame(&ctx, &mut mgr, None, after(t0, 10), true);
        frame(&ctx, &mut mgr, None, after(t0, 10), true);
        let center = card_of(&mgr, id).center();
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerMoved(center)),
            after(t0, 5000),
            true,
        );
        assert_eq!(mgr.len(), 1);
        let t = &mgr.toasts[0];
        assert_eq!(compute_alpha(t, after(t0, 5000), true), 1.0);
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerGone),
            after(t0, 5000),
            true,
        );
        let left = after(t0, 5000);
        let t = &mgr.toasts[0];
        assert_eq!(
            compute_alpha(t, left + Duration::from_millis(2000), true),
            0.0
        );
    }

    /// Given hover로 붙잡은 카드, When 같은 범위·문구가 다시 온다, Then 그 카드에 합쳐지고
    /// 불투명으로 남는다.
    #[test]
    fn a_held_card_absorbs_the_same_message_without_flicker() {
        let ctx = egui::Context::default();
        let mut mgr = ToastManager::new();
        mgr.push_info("dup", ToastScope::Window);
        let id = mgr.toasts[0].id;
        let t0 = mgr.toasts[0].spawned_at;
        frame(&ctx, &mut mgr, None, after(t0, 100), false);
        frame(&ctx, &mut mgr, None, after(t0, 100), false);
        let center = card_of(&mgr, id).center();
        frame(
            &ctx,
            &mut mgr,
            Some(egui::Event::PointerMoved(center)),
            Instant::now(),
            false,
        );
        mgr.push_info("dup", ToastScope::Window);
        assert_eq!(mgr.len(), 1);
        assert!((alpha_of(&mgr, id, Instant::now()) - 1.0).abs() < 1e-3);
    }

    /// Given 카드 아래 버튼, When 카드 위에서 누르고 뗀다, Then 버튼이 클릭을 받고 토스트는
    /// 포인터 아래 레이어가 되지 않는다. 대조군은 토스트 없이 같은 클릭이다.
    #[test]
    fn a_click_on_a_held_card_reaches_the_widget_below() {
        fn click_through(with_toast: bool) -> (bool, bool) {
            let ctx = egui::Context::default();
            let mut mgr = ToastManager::new();
            if with_toast {
                mgr.push_info("over the button", ToastScope::Window);
            }
            let layout = window_layout();
            let mut card = None;
            let mut clicked = false;
            let mut over_area = false;
            let mut run = |events: Vec<egui::Event>, card_out: &mut Option<egui::Rect>| {
                let mut hit = false;
                drop(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen()),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let b = ui.add_sized(ui.available_size(), egui::Button::new("below"));
                            hit = b.clicked();
                        });
                        mgr.draw(ctx, &layout, false);
                        *card_out = mgr.drawn_cards.first().map(|(_, r)| *r);
                    },
                ));
                hit
            };
            run(Vec::new(), &mut card);
            run(Vec::new(), &mut card);
            let pos = card.map_or(egui::pos2(1100.0, 700.0), |r| r.center());
            let press = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(vec![egui::Event::PointerMoved(pos)], &mut card);
            over_area |= ctx
                .layer_id_at(pos)
                .is_some_and(|l| l.order == egui::Order::Tooltip);
            run(vec![press(true)], &mut card);
            clicked |= run(vec![press(false)], &mut card);
            if with_toast {
                assert_eq!(mgr.len(), 1, "card held under the pointer");
            }
            (clicked, over_area)
        }
        assert_eq!(click_through(false), (true, false), "control");
        assert_eq!(click_through(true), (true, false));
    }

    #[test]
    fn view_empty_scopes_is_noop() {
        run_view(vec![]);
    }

    #[test]
    fn view_with_entries_does_not_panic() {
        let scopes = vec![ToastScopeView {
            scope_rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            bottom: ToastStackBottom::ScopeMargin,
            entries: vec![
                ToastEntryView {
                    id: 2,
                    kind: ToastKind::Info,
                    message: "info".into(),
                    hint: Vec::new(),
                    alpha: 1.0,
                },
                ToastEntryView {
                    id: 3,
                    kind: ToastKind::Error,
                    message: "long error message that may wrap into multiple lines".into(),
                    hint: Vec::new(),
                    alpha: 0.5,
                },
            ],
        }];
        run_view(scopes);
    }

    #[test]
    fn view_skips_zero_alpha_entries() {
        let scopes = vec![ToastScopeView {
            scope_rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            bottom: ToastStackBottom::ScopeMargin,
            entries: vec![ToastEntryView {
                id: 4,
                kind: ToastKind::Warning,
                message: "invisible".into(),
                hint: Vec::new(),
                alpha: 0.0,
            }],
        }];
        run_view(scopes);
    }
}
