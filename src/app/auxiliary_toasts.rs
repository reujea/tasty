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
            let focused = self.focused_pair().map(|(_, engine)| engine);
            let any_engine = self.engines().windowed_and_parked().next();
            global_toast_duration_ms(
                focused.as_ref().map(|e| &e.runtime.settings),
                any_engine.as_ref().map(|e| &e.runtime.settings),
                crate::settings::Settings::load,
            )
        };
        if let Some(view) = self.view.views.get_mut(&id)
            && let Some((_, toasts)) = auxiliary_toasts(view.as_any_mut())
        {
            toasts.set_lifetime_ms(ms);
        }
    }
}

/// 보조 창 토스트 수명의 출처. 설정 적용이 parked 엔진까지 갱신하고 설정 창도 엔진이 없으면 파일을
/// 읽으므로, 포커스 메인 창 → 아무 엔진(parked 포함) → 설정 파일 순서로 같은 전역 값을 얻는다.
fn global_toast_duration_ms(
    focused: Option<&crate::settings::Settings>,
    any_engine: Option<&crate::settings::Settings>,
    load: impl FnOnce() -> crate::settings::Settings,
) -> u64 {
    match focused.or(any_engine) {
        Some(s) => s.overlay.toast_duration_ms,
        None => load().overlay.toast_duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::global_toast_duration_ms;
    use crate::settings::Settings;

    fn with_ms(ms: u64) -> Settings {
        Settings {
            overlay: tasty_settings::OverlaySettings {
                toast_duration_ms: ms,
            },
            ..Default::default()
        }
    }

    fn never_load() -> Settings {
        panic!("엔진이 있으면 설정 파일을 읽지 않는다")
    }

    /// Given 포커스 메인 창의 엔진이 전역 5000ms, Then 5000ms다.
    #[test]
    fn a_focused_engine_gives_the_global_duration() {
        let s = with_ms(5000);
        assert_eq!(
            global_toast_duration_ms(Some(&s), Some(&s), never_load),
            5000
        );
    }

    /// Given 마지막 메인 창을 park해 포커스 창이 없고 parked 엔진만 5000ms, Then 5000ms다.
    #[test]
    fn a_parked_engine_gives_the_global_duration_without_a_focused_window() {
        let s = with_ms(5000);
        assert_eq!(global_toast_duration_ms(None, Some(&s), never_load), 5000);
    }

    /// Given 엔진이 하나도 없고 설정 파일이 5000ms, Then 설정 창과 같이 파일 값 5000ms다.
    #[test]
    fn no_engine_falls_back_to_the_settings_file() {
        assert_eq!(global_toast_duration_ms(None, None, || with_ms(5000)), 5000);
    }
}
