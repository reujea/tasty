//! 휘발성 인앱 알림(Toast) 시스템.
//!
//! 설계 문서: `docs/design/systems/toast.md`.
//!
//! - **사용자 행동에서만 발사**된다. CLI/IPC를 통한 에이전트 동작은 토스트를 만들지 않는다.
//! - 포커스를 받지 않으며 입력 이벤트를 소비하지 않는다 (마우스가 그대로 통과).
//! - 자동 소멸한다 (기본 2초).
//! - `LayoutContext`를 받아 스코프별 rect를 얻는다.
//!
//! ## Split: wrapper / view
//!
//! [`ToastManager`] 가 *상태 관리* (push / coalesce / 만료 정리 / scope rect lookup /
//! 캡 집행) 를 담당하고, **그리기 본문은 이 파일에 없다** — `tasty_ui_widgets::toast`
//! 가 소유하고 갤러리 specimen 이 같은 함수를 부른다. 여기 남은 [`draw_toast_view`] 는
//! 토스트가 떠야 할 레이어(`Order::Tooltip`)를 고르는 **얇은 래퍼**다.
//!
//! 형상을 두 벌 두지 않는 이유: 값이 같아 보여도 정의가 둘이면 갈리고, 갈린 뒤엔 어느
//! 쪽이 정본인지 알 수 없다. 예전에는 갤러리가 이 파일의 그리기를 손으로 되풀이했다.

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
    pub scope: ToastScope,
    pub spawned_at: Instant,
    pub lifetime: Duration,
}

const DEFAULT_LIFETIME: Duration = Duration::from_millis(2000);
/// 같은 스코프·같은 메시지가 이 시간 내에 다시 발사되면 새 토스트를 만들지 않고
/// 기존 토스트의 수명만 갱신한다.
const COALESCE_WINDOW: Duration = Duration::from_millis(500);
/// 스코프당 최대 동시 표시 개수.
const MAX_PER_SCOPE: usize = 5;
/// 토스트 본문 최대 문자 수(유니코드 char 기준, 바이트 아님). 초과 시 앞
/// `MAX_MESSAGE_CHARS` 자만 남기고 줄바꿈 + 안내 접미를 붙인다 — 비정상적으로
/// 긴 입력(경로/에러/plugin 텍스트)이 토스트를 세로로 폭주시키는 것을 막는다.
///
/// 값은 [`crate::i18n::TOAST_MAX_CHARS`] 에서 온다. 캡에 맞춰 문구를 **만드는** 쪽
/// (`tasty-i18n` 의 `fit_fragment`/`t_fmt_fit`)과 캡을 **집행하는** 여기가 서로 다른
/// 상수를 들면, 한쪽만 바뀐 순간 "맞췄는데 잘리는" 상태가 조용히 생긴다.
const MAX_MESSAGE_CHARS: usize = crate::i18n::TOAST_MAX_CHARS;
// 시각(카드 chrome · 스택 배치 · 페이드 곡선)은 위젯 크레이트가 소유한다. 여기서
// 다시 정의하면 갤러리와 갈릴 수 있는 구조가 되살아난다.
use tasty_ui_widgets::{TOAST_FADE_OUT_MS as FADE_OUT_MS, toast_fade_alpha};
pub use tasty_ui_widgets::{ToastEntryView, ToastScopeView, ToastViewProps};

/// 토스트 스택을 화면 최상단 레이어에 그린다.
///
/// 그리기 본문은 `tasty_ui_widgets::draw_toast_scopes` 에 있고 이 함수가 고르는
/// 것은 **어디에 그리는가** 하나다 — 토스트는 다른 UI 위에 떠야 하므로
/// `LayerId(Order::Tooltip, …)` 의 layer painter 를 넘긴다. 갤러리 specimen 은 같은
/// 함수에 무대 frame 의 painter 를 넘겨 같은 픽셀을 그린다.
///
/// 반환값 없음 — 토스트는 사용자 입력을 받지 않으며 (auto-dismiss) action 도 없다.
pub fn draw_toast_view(ctx: &egui::Context, props: &ToastViewProps<'_>) {
    if props.scopes.is_empty() {
        return;
    }

    let layer_id = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("toast_layer"));
    let painter = ctx.layer_painter(layer_id);
    tasty_ui_widgets::draw_toast_scopes(&painter, props);
}

pub struct ToastManager {
    toasts: Vec<ToastState>,
    next_id: u64,
    /// 새 토스트/coalesce 갱신에 부여할 수명. `Settings.overlay.toast_duration_ms`
    /// 에서 매 프레임 동기화된다(설정 미로드 시 [`DEFAULT_LIFETIME`]).
    lifetime: Duration,
}

impl ToastManager {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 1,
            lifetime: DEFAULT_LIFETIME,
        }
    }

    /// 토스트 수명을 설정값(ms)으로 동기화한다. 이미 떠 있는 토스트에는 소급하지
    /// 않고, 이후 push/coalesce 되는 토스트부터 적용된다. draw 직전에 호출한다.
    pub fn set_lifetime_ms(&mut self, ms: u64) {
        self.lifetime = Duration::from_millis(ms);
    }

    /// 토스트 발사. 사용자 행동에서만 호출되어야 한다.
    pub fn push(&mut self, message: impl Into<String>, kind: ToastKind, scope: ToastScope) {
        let message = truncate_message(message.into());
        let now = Instant::now();

        // Coalesce: 같은 스코프·같은 메시지가 짧은 시간 내에 또 오면 수명만 갱신한다.
        if let Some(existing) = self.toasts.iter_mut().rev().find(|t| {
            t.scope == scope
                && t.message == message
                && now.duration_since(t.spawned_at) < COALESCE_WINDOW
        }) {
            existing.spawned_at = now;
            existing.kind = kind;
            existing.lifetime = self.lifetime;
            return;
        }

        let id = self.next_id;
        self.next_id += 1;

        self.toasts.push(ToastState {
            id,
            message,
            kind,
            scope: scope.clone(),
            spawned_at: now,
            lifetime: self.lifetime,
        });

        // 스코프당 최대 개수 초과 시 가장 오래된 것 제거.
        let count_in_scope = self.toasts.iter().filter(|t| t.scope == scope).count();
        if count_in_scope > MAX_PER_SCOPE
            && let Some(idx) = self.toasts.iter().position(|t| t.scope == scope)
        {
            self.toasts.remove(idx);
        }
    }

    /// 지금 떠 있는 토스트 수. **테스트 전용** — 부수효과 게이트(예: 창 없는 parked
    /// engine 에는 토스트를 쌓지 않는다)를 단언하려면 개수를 볼 수 있어야 한다.
    /// 프로덕션 표면을 넓히지 않으려고 `cfg(test)` 로 묶는다.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.toasts.len()
    }

    /// 지금 떠 있는 토스트의 문구(발사 순). **테스트 전용** — 개수만으로는 사유가 문구에
    /// 그대로 실렸는지 못 본다. `len` 과 같은 이유로 `cfg(test)` 로 묶는다.
    #[cfg(test)]
    pub(crate) fn messages(&self) -> Vec<&str> {
        self.toasts.iter().map(|t| t.message.as_str()).collect()
    }

    /// 편의 헬퍼: Info 토스트.
    pub fn push_info(&mut self, message: impl Into<String>, scope: ToastScope) {
        self.push(message, ToastKind::Info, scope);
    }

    /// 매 프레임 호출. 만료된 토스트를 제거하고 살아있는 토스트를 그린다.
    /// `draw_ctx`는 PopupManager가 만든 것을 그대로 공유한다.
    ///
    /// `reduced_motion`이 true면 페이드 인/아웃을 0ms로 처리 (시각 자극 최소화).
    ///
    /// Wrapper: 상태 정리 + alpha 계산 + scope rect lookup → 순수
    /// [`draw_toast_view`] 로 위임.
    pub fn draw(&mut self, ctx: &egui::Context, draw_ctx: &LayoutContext, reduced_motion: bool) {
        let now = Instant::now();

        // 1) 만료된 토스트 제거.
        self.toasts.retain(|t| {
            let age = now.duration_since(t.spawned_at);
            age < t.lifetime + Duration::from_millis(FADE_OUT_MS as u64)
        });

        // 2) 스코프가 화면에서 사라진 토스트 제거.
        self.toasts
            .retain(|t| Self::scope_rect(&t.scope, draw_ctx, ctx).is_some());

        if self.toasts.is_empty() {
            return;
        }

        // 살아있는 동안에는 매 프레임 다시 그려야 페이드가 보인다.
        ctx.request_repaint();

        // 3) 스코프별로 그루핑하여 view 입력으로 변환.
        let mut by_scope: std::collections::HashMap<String, Vec<&ToastState>> =
            std::collections::HashMap::new();
        for t in &self.toasts {
            by_scope
                .entry(format!("{:?}", t.scope))
                .or_default()
                .push(t);
        }

        let mut scopes: Vec<ToastScopeView> = Vec::with_capacity(by_scope.len());
        for (_, mut group) in by_scope {
            group.sort_by_key(|t| t.id);
            let scope = &group[0].scope;
            let Some(scope_rect) = Self::scope_rect(scope, draw_ctx, ctx) else {
                continue;
            };
            let entries: Vec<ToastEntryView> = group
                .iter()
                .map(|t| ToastEntryView {
                    kind: t.kind,
                    message: t.message.clone(),
                    alpha: compute_alpha(t, now, reduced_motion),
                })
                .collect();
            scopes.push(ToastScopeView {
                scope_rect,
                entries,
            });
        }

        let th = theme::theme();
        let props = ToastViewProps {
            theme: &th,
            scopes: &scopes,
        };
        draw_toast_view(ctx, &props);
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

/// 페이드 인/아웃 알파 계산. pub — 테스트 + wrapper 가 view 입력 변환 시 호출.
///
/// 곡선 자체는 `tasty_ui_widgets::toast::fade_alpha` 가 소유한다(갤러리가 같은 곡선을
/// 쓴다). 여기는 `ToastState` 에서 그 함수가 읽는 두 값을 꺼내는 어댑터다 —
/// `ToastState` 가 `ToastScope`(도메인 모델)를 품어 위젯 크레이트로 못 넘어간다.
pub fn compute_alpha(t: &ToastState, now: Instant, reduced_motion: bool) -> f32 {
    toast_fade_alpha(now.duration_since(t.spawned_at), t.lifetime, reduced_motion)
}

/// 본문이 `MAX_MESSAGE_CHARS`(유니코드 char) 를 초과하면 앞부분만 남기고 줄바꿈
/// + 안내 접미(`toast.char_limit_notice`)를 붙인다.
///
/// - 길이는 `chars().count()`(문자 수), 자르기는 `chars().take(..)`(char 경계)로
///   처리해 멀티바이트(한글/일문 등)에서 바이트 슬라이싱 panic 을 피한다.
/// - 경계 정책: 원본이 `MAX_MESSAGE_CHARS` 를 *초과* 할 때만 자른다(정확히 같거나
///   이하는 변경 없음). 접미는 캡 *바깥* 에 추가로 붙는다.
/// - 접미가 말하는 숫자는 캡 상수에서 나온다 — 번역문에 숫자를 적어 두면 캡을 조정한
///   순간 세 로케일이 전부 거짓말을 한다.
/// - coalesce 비교 이전(push 진입부)에 적용되므로 같은 긴 메시지는 동일하게
///   잘려 정상 coalesce 된다.
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
        // 멀티바이트 250자 → 첫 줄 200 char + 줄바꿈 + 접미.
        let out = truncate_message("あ".repeat(250));
        let first_line = out.lines().next().unwrap();
        assert_eq!(first_line.chars().count(), MAX_MESSAGE_CHARS);
        assert!(out.contains('\n'));
        // 접미 줄(번역값/키)이 존재한다(로케일 의존이라 내용은 단정하지 않음).
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
        // 한글(자당 3바이트) 300자 — 바이트 슬라이싱이면 panic. char 경계라 안전.
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
        // kind 갱신됨
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
    fn view_empty_scopes_is_noop() {
        run_view(vec![]);
    }

    #[test]
    fn view_with_entries_does_not_panic() {
        let scopes = vec![ToastScopeView {
            scope_rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            entries: vec![
                ToastEntryView {
                    kind: ToastKind::Info,
                    message: "info".into(),
                    alpha: 1.0,
                },
                ToastEntryView {
                    kind: ToastKind::Error,
                    message: "long error message that may wrap into multiple lines".into(),
                    alpha: 0.5,
                },
            ],
        }];
        run_view(scopes);
    }

    #[test]
    fn view_skips_zero_alpha_entries() {
        // alpha=0 인 entry 만 있어도 panic 없이 통과.
        let scopes = vec![ToastScopeView {
            scope_rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            entries: vec![ToastEntryView {
                kind: ToastKind::Warning,
                message: "invisible".into(),
                alpha: 0.0,
            }],
        }];
        run_view(scopes);
    }
}
