//! 보조 창(설정·프리셋·플러그인·종료 확인)의 토스트 스택과 전역 수명 동기화.
//! 메인 창은 overlay가 draw 직전에 전역 수명을 맞추지만, 보조 창의 View는 engine을 받지 않는다.
//! 그래서 App이 이벤트를 넘기기 전에 같은 값을 넣는다. 값은 메인 창·엔진이 아니라 최신 적용 설정
//! snapshot에서 읽으므로 메인 창이 없어도(마지막 창 park, macOS 최소화) 전역 값을 쓴다.
//! docs/design/systems/toast.md 참고.

use winit::window::WindowId;

use crate::adapters::ui::ToastManager;
use crate::app::App;

/// 자기 토스트 스택을 가진 보조 창. 모달과 preset 편집기는 메인 창과 별개의 창 범위 스택을 쓴다.
pub(crate) fn auxiliary_toasts(
    view: &mut dyn std::any::Any,
) -> Option<(&'static str, &mut ToastManager)> {
    if view.is::<crate::view::settings::SettingsView>() {
        return view
            .downcast_mut::<crate::view::settings::SettingsView>()
            .map(|v| ("settings", &mut v.toasts));
    }
    if view.is::<crate::view::preset::PresetView>() {
        return view
            .downcast_mut::<crate::view::preset::PresetView>()
            .map(|v| ("preset", &mut v.toasts));
    }
    if view.is::<crate::view::QuitView>() {
        return view
            .downcast_mut::<crate::view::QuitView>()
            .map(|v| ("quit", &mut v.toasts));
    }
    view.downcast_mut::<crate::view::plugins::PluginsView>()
        .map(|v| ("plugins", &mut v.toasts))
}

impl App {
    /// `id` 창이 보조 창이면 그 토스트 수명을 전역 설정으로 맞춘다. hover 재시작도 이 값을 카드에 넣는다.
    pub(crate) fn sync_auxiliary_toast_lifetime(&mut self, id: WindowId) {
        let latest = self
            .services
            .latest_settings()
            .map(|s| s.overlay.toast_duration_ms);
        if let Some(view) = self.view.views.get_mut(&id)
            && let Some((_, toasts)) = auxiliary_toasts(view.as_any_mut())
        {
            apply_global_lifetime(toasts, latest);
        }
    }
}

/// 보조 창 수명은 최신 적용 snapshot 하나로 맞춘다. 부팅(`begin_boot`)과 설정 적용이 저장 성공과
/// 관계없이 채우고, 보조 창은 부팅 뒤에만 열리므로 비어 있으면 안 된다. 비어 있으면 기본 수명이다.
fn apply_global_lifetime(toasts: &mut ToastManager, latest: Option<u64>) {
    if latest.is_none() {
        tracing::warn!("no applied settings snapshot for an auxiliary window toast");
    }
    toasts.set_lifetime_ms_or_default(latest);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::apply_global_lifetime;
    use crate::adapters::ui::{ToastKind, ToastManager, ToastScope};

    fn pushed_lifetime(toasts: &mut ToastManager) -> Vec<Duration> {
        toasts.push("card", ToastKind::Info, ToastScope::Window);
        toasts.card_lifetimes()
    }

    /// Given 저장이 실패해 파일·새 엔진은 2000ms인데 최신 적용 snapshot은 5000ms,
    /// When 2000ms로 맞춰 둔 보조 창 관리자를 동기화한다, Then 새 카드는 5000ms다.
    #[test]
    fn the_latest_applied_snapshot_wins_over_a_stale_value() {
        let mut toasts = ToastManager::new();
        toasts.set_lifetime_ms(2000);
        apply_global_lifetime(&mut toasts, Some(5000));
        assert_eq!(
            pushed_lifetime(&mut toasts),
            vec![Duration::from_millis(5000)]
        );
    }

    /// Given 7000ms로 맞춘 관리자, When snapshot 없이(도달하면 안 되는 상태) 동기화한다,
    /// Then 이전 값이 아니라 기본 수명 2000ms다.
    #[test]
    fn without_a_snapshot_the_manager_uses_the_default_lifetime() {
        let mut toasts = ToastManager::new();
        toasts.set_lifetime_ms(7000);
        apply_global_lifetime(&mut toasts, None);
        assert_eq!(
            pushed_lifetime(&mut toasts),
            vec![Duration::from_millis(2000)]
        );
    }
}
