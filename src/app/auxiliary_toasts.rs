//! 보조 창(설정·프리셋·플러그인)의 토스트 스택과 전역 수명 동기화.
//! 메인 창은 overlay가 draw 직전에 전역 수명을 맞추지만, 보조 창의 View는 engine을 받지 않는다.
//! 그래서 App이 이벤트를 넘기기 전에 같은 값을 넣는다. docs/design/systems/toast.md 참고.

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
    /// `id` 창이 보조 창이면 그 토스트 수명을 포커스 메인 창 engine의 전역 설정으로 맞춘다.
    /// 메인 창이 없으면 지난 값을 둔다. hover 재시작도 이 값을 카드에 넣는다.
    pub(crate) fn sync_auxiliary_toast_lifetime(&mut self, id: WindowId) {
        let Some(ms) = self
            .focused_pair()
            .map(|(_, engine)| engine.runtime.settings.overlay.toast_duration_ms)
        else {
            return;
        };
        if let Some(view) = self.view.views.get_mut(&id)
            && let Some((_, toasts)) = auxiliary_toasts(view.as_any_mut())
        {
            toasts.set_lifetime_ms(ms);
        }
    }
}
