//! 보조 창(설정·프리셋·플러그인)의 토스트 스택과 전역 수명 동기화.
//! 메인 창은 overlay가 draw 직전에 전역 수명을 맞추지만, 보조 창의 View는 engine을 받지 않는다.
//! 그래서 App이 이벤트를 넘기기 전에 같은 값을 넣는다. 메인 창이 없어도(마지막 창 park, macOS 최소화)
//! 전역 값을 쓴다. docs/design/systems/toast.md 참고.

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
    view.downcast_mut::<crate::view::plugins::PluginsView>()
        .map(|v| ("plugins", &mut v.toasts))
}

impl App {
    /// `id` 창이 보조 창이면 그 토스트 수명을 전역 설정으로 맞춘다. hover 재시작도 이 값을 카드에 넣는다.
    pub(crate) fn sync_auxiliary_toast_lifetime(&mut self, id: WindowId) {
        let is_auxiliary = self
            .view
            .views
            .get_mut(&id)
            .is_some_and(|view| auxiliary_toasts(view.as_any_mut()).is_some());
        if !is_auxiliary {
            return;
        }
        let ms = {
            let latest = self.services.latest_settings();
            let focused = self.focused_pair().map(|(_, engine)| engine);
            let any_engine = self.engines().windowed_and_parked().next();
            global_toast_duration_ms(
                latest,
                focused.as_ref().map(|e| &e.runtime.settings),
                any_engine.as_ref().map(|e| &e.runtime.settings),
            )
        };
        if let Some(view) = self.view.views.get_mut(&id)
            && let Some((_, toasts)) = auxiliary_toasts(view.as_any_mut())
        {
            match ms {
                Some(ms) => toasts.set_lifetime_ms(ms),
                None => toasts.set_lifetime_ms_once(|| {
                    crate::settings::Settings::load().overlay.toast_duration_ms
                }),
            }
        }
    }
}

/// 보조 창 토스트 수명의 출처. 설정 적용은 저장이 실패해도 최신 snapshot을 남기므로 그 값이 우선이다.
/// snapshot은 이번 실행에서 처음 적용하기 전까지 비어 있고, 그동안 엔진 설정은 파일에서 읽은 값이다.
/// 엔진도 없으면 `None`이고, 호출자가 창마다 한 번만 설정 파일을 읽는다.
fn global_toast_duration_ms(
    latest: Option<&crate::settings::Settings>,
    focused: Option<&crate::settings::Settings>,
    any_engine: Option<&crate::settings::Settings>,
) -> Option<u64> {
    latest
        .or(focused)
        .or(any_engine)
        .map(|s| s.overlay.toast_duration_ms)
}

#[cfg(test)]
mod tests {
    use super::global_toast_duration_ms;
    use crate::adapters::ui::ToastManager;
    use crate::settings::Settings;

    fn with_ms(ms: u64) -> Settings {
        Settings {
            overlay: tasty_settings::OverlaySettings {
                toast_duration_ms: ms,
            },
            ..Default::default()
        }
    }

    /// Given 저장이 실패해 파일·새 엔진은 2000ms, 최신 적용 snapshot은 5000ms, Then 5000ms다.
    #[test]
    fn the_latest_applied_snapshot_wins_over_a_stale_engine() {
        let (latest, stale) = (with_ms(5000), with_ms(2000));
        assert_eq!(
            global_toast_duration_ms(Some(&latest), Some(&stale), Some(&stale)),
            Some(5000)
        );
        assert_eq!(
            global_toast_duration_ms(Some(&latest), None, Some(&stale)),
            Some(5000)
        );
    }

    /// Given 아직 적용한 적이 없고 포커스 메인 창의 엔진이 5000ms, Then 5000ms다.
    #[test]
    fn before_any_apply_a_focused_engine_gives_the_global_duration() {
        let s = with_ms(5000);
        assert_eq!(
            global_toast_duration_ms(None, Some(&s), Some(&s)),
            Some(5000)
        );
    }

    /// Given 아직 적용한 적이 없고 마지막 메인 창을 park해 parked 엔진만 5000ms, Then 5000ms다.
    #[test]
    fn before_any_apply_a_parked_engine_gives_the_global_duration() {
        let s = with_ms(5000);
        assert_eq!(global_toast_duration_ms(None, None, Some(&s)), Some(5000));
    }

    /// Given snapshot도 엔진도 없음, Then 출처가 없다(호출자가 파일을 읽는다).
    #[test]
    fn without_snapshot_or_engine_there_is_no_in_memory_source() {
        assert_eq!(global_toast_duration_ms(None, None, None), None);
    }

    /// Given 출처가 없는 이벤트가 여러 번, Then 설정 파일 대신 쓰는 값은 창마다 한 번만 읽는다.
    #[test]
    fn the_file_fallback_is_read_once_per_window() {
        let mut toasts = ToastManager::new();
        let mut reads = 0;
        for _ in 0..3 {
            toasts.set_lifetime_ms_once(|| {
                reads += 1;
                5000
            });
        }
        assert_eq!(reads, 1);
        toasts.set_lifetime_ms(7000);
        toasts.set_lifetime_ms_once(|| unreachable!("이미 맞춘 수명은 다시 읽지 않는다"));
    }
}
