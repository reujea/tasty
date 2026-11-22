//! 같은 홈으로 다시 실행한 두 번째 프로세스의 활성화 요청을 처리한다.
//!
//! 증거가 있는 요청만 처리한다. 사용자가 마지막으로 쓰던 MainView의 활성화를 OS에 요청하고, Windows·X11은
//! 그 전에 모든 View에 보이기와 최소화 해제를 요청한다. 포커스 이동 여부는 OS가 정하며 Tasty는
//! `focus_window()`를 부르지 않는다(docs/design/policies/focus.md). MainView가 없으면 새 창 경로로 간다.
//! 만드는 중인 창이 있으면 새로 만들지 않는다. Windows·X11은 그 창이 등록될 때 활성화를 요청하고,
//! 네이티브 Wayland는 이 요청을 남기지 않는다.
//!
//! 네이티브 Wayland에서는 winit이 보이기·최소화 해제를 무시하므로 Tasty는 창 상태를 바꾸지 못하고, 두 번째
//! 실행이 받은 xdg-activation 토큰으로 기존 창의 활성화만 요청한다. 토큰이 없으면 기존 창에 요청할 수
//! 없어 새 창 경로로 간다([`can_raise_existing_view`]). 컴포지터가 `xdg_activation_v1`을 제공하지 않아
//! 요청하지 못해도 토큰을 가진 채 새 창 경로로 간다.
//!
//! 요청을 받는 경로는 Windows·Linux 에만 있다(Linux D-Bus `Activate`, Windows 등록 메시지). macOS 는
//! 같은 앱을 다시 열면 OS 가 기존 프로세스에 전달하므로 받는 쪽 처리를 빌드하지 않는다. 만드는 중인 창에
//! 걸어 둔 요청을 적용하는 경로는 창 생성 흐름이 모든 OS 에서 부르므로 남긴다.

#[cfg(any(windows, target_os = "linux"))]
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::app::App;
#[cfg(any(windows, target_os = "linux"))]
use crate::app::event::WindowRequestOrigin;
use crate::boot::single_instance::ExternalActivation;

#[cfg(any(windows, target_os = "linux"))]
/// 외부 활성화 요청으로 할 일.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ActivationPlan<W> {
    /// 증거가 없다. 아무것도 바꾸지 않는다.
    Ignore,
    /// MainView가 없거나, Wayland에서 이 요청에 토큰이 없다. 새 창 경로로 간다.
    OpenWindow,
    /// `target`의 활성화를 OS에 요청한다. Windows·X11은 그 전에 모든 View에 보이기·최소화 해제를 요청한다.
    Restore { views: Vec<W>, target: W },
}

#[cfg(any(windows, target_os = "linux"))]
/// `views`는 (창, MainView 여부) 목록이다. 대상은 마지막으로 포커스를 가졌던 View이고, 그 값이 없거나
/// 이미 닫힌 창이면 가장 작은 id의 MainView다. `raise_existing`이 거짓이면(Wayland에서 토큰이 없는 요청)
/// 기존 창에 활성화를 요청할 수 없으므로 창이 있어도 새 창 경로를 고른다.
pub(crate) fn plan_activation<W: Copy + Ord>(
    has_evidence: bool,
    raise_existing: bool,
    views: &[(W, bool)],
    focused: Option<W>,
) -> ActivationPlan<W> {
    if !has_evidence {
        return ActivationPlan::Ignore;
    }
    if !raise_existing {
        return ActivationPlan::OpenWindow;
    }
    let Some(first_main) = views
        .iter()
        .filter(|(_, main)| *main)
        .map(|(id, _)| *id)
        .min()
    else {
        return ActivationPlan::OpenWindow;
    };
    let target = focused
        .filter(|f| views.iter().any(|(id, _)| id == f))
        .unwrap_or(first_main);
    let mut all: Vec<W> = views.iter().map(|(id, _)| *id).collect();
    all.sort();
    ActivationPlan::Restore { views: all, target }
}

#[cfg(any(windows, target_os = "linux"))]
/// 이 요청으로 이미 떠 있는 창을 활성화할 수 있는지. Wayland는 xdg-activation 토큰이 있어야 한다.
fn can_raise_existing_view(wayland: bool, request: &ExternalActivation) -> bool {
    raises_existing_view(wayland, request.evidence.wayland_token.is_some())
}

#[cfg(any(windows, target_os = "linux"))]
fn raises_existing_view(wayland: bool, has_wayland_token: bool) -> bool {
    !wayland || has_wayland_token
}

#[cfg(any(windows, target_os = "linux"))]
/// 새로 연 창이 등록된 뒤 OS 활성화를 한 번 더 요청할지. X11은 생성 때 startup id를 싣고 등록 뒤
/// `_NET_ACTIVE_WINDOW`를 보내는 두 단계를 쓴다. Wayland는 생성 속성에 실은 토큰 한 번으로 끝낸다. 그 요청은
/// 토큰이 있고 컴포지터가 `xdg_activation_v1`을 제공할 때만 나간다.
/// xdg-activation 토큰은 컴포지터가 한 번 쓰면 무효로 할 수 있다.
fn activates_after_creation(wayland: bool, raise_existing: bool) -> bool {
    raise_existing && !wayland
}

/// OS 활성화 요청의 결과 중 호출자가 다르게 처리할 것.
#[derive(Debug, PartialEq, Eq)]
enum OsActivation {
    /// 요청했거나, 요청하지 못한 이유를 로그로 남겼다.
    Done,
    /// Wayland에서 기존 창의 활성화를 요청할 수 없다.
    WaylandUnsupported,
}

impl App {
    #[cfg(any(windows, target_os = "linux"))]
    pub(crate) fn handle_external_activation(
        &mut self,
        event_loop: &ActiveEventLoop,
        request: ExternalActivation,
    ) {
        let views: Vec<(WindowId, bool)> = self
            .view
            .views
            .iter()
            .map(|(id, view)| (*id, view.as_main().is_some()))
            .collect();
        let wayland = crate::boot::single_instance::is_wayland(event_loop);
        let raise_existing = can_raise_existing_view(wayland, &request);
        match plan_activation(
            request.has_evidence(),
            raise_existing,
            &views,
            self.view.focused_view_id,
        ) {
            ActivationPlan::Ignore => {
                tracing::info!("external activation without launch evidence: nothing changed");
            }
            ActivationPlan::OpenWindow => {
                let activate_after = activates_after_creation(wayland, raise_existing);
                self.open_activated_window(event_loop, request, activate_after);
            }
            ActivationPlan::Restore { views, target } => {
                self.restore_and_activate(event_loop, wayland, &views, target, request);
            }
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    /// 트레이 보이기와 같은 `set_visible(true)`·`set_minimized(false)`를 부르되 `focus_window()`는 부르지
    /// 않는다. 포커스는 OS가 정한다. 네이티브 Wayland에서는 winit이 두 호출을 무시한다.
    fn restore_and_activate(
        &mut self,
        event_loop: &ActiveEventLoop,
        wayland: bool,
        views: &[WindowId],
        target: WindowId,
        request: ExternalActivation,
    ) {
        for id in views {
            if let Some(view) = self.view.views.get(id) {
                let window = &view.base().winit;
                window.set_visible(true);
                window.set_minimized(false);
            }
        }
        tracing::info!("{}", restore_log_line(wayland, views.len(), target));
        if self.request_os_activation(target, &request) == OsActivation::WaylandUnsupported {
            tracing::info!(
                "external activation: the existing window cannot be raised; taking the new-window path"
            );
            self.open_activated_window(event_loop, request, false);
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    /// 증거를 창 생성 속성에 실어 새 창을 만든다. Wayland는 그중 xdg-activation 토큰만 싣는다.
    /// `activate_after`가 참이면 등록 뒤 같은 증거로 OS 활성화를 한 번 더 요청한다
    /// ([`activates_after_creation`]). 만드는 중인 창이 있으면 싣기 전에 돌아가며, 요청은
    /// `activate_after`가 참일 때(Windows·X11)만 그 창에 남긴다.
    fn open_activated_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        request: ExternalActivation,
        activate_after: bool,
    ) {
        if self.pending_window.is_some() {
            if activate_after {
                tracing::info!(
                    "external activation: a window is being created; activating it when ready"
                );
                self.pending_external_activation = Some(request);
            } else {
                tracing::info!(
                    "external activation: a window is being created; native Wayland keeps neither this request nor its token"
                );
            }
            return;
        }
        self.next_window_activation = Some(request.evidence.clone());
        if activate_after {
            self.pending_external_activation = Some(request);
        }
        let outcome = self.create_new_window(event_loop, WindowRequestOrigin::User);
        if outcome.is_err() {
            self.next_window_activation = None;
            self.pending_external_activation = None;
        }
        tracing::info!("external activation: opening a new window: {outcome:?}");
    }

    /// 만드는 중이던 창이 등록되면 걸어 둔 외부 활성화를 그 창에 적용한다.
    pub(crate) fn apply_pending_external_activation(&mut self, window_id: WindowId) {
        if let Some(request) = self.pending_external_activation.take() {
            self.request_os_activation(window_id, &request);
        }
    }

    fn request_os_activation(&self, id: WindowId, request: &ExternalActivation) -> OsActivation {
        let Some(view) = self.view.views.get(&id) else {
            return OsActivation::Done;
        };
        let evidence = request.platform_evidence();
        let result =
            crate::platform::window_activation::request_activation(&view.base().winit, &evidence);
        log_os_activation(id, result)
    }
}

#[cfg(any(windows, target_os = "linux"))]
/// 기존 창 경로의 로그 한 줄. 네이티브 Wayland에서는 winit이 보이기·최소화 해제를 무시하므로
/// 복원했다고 적지 않는다.
fn restore_log_line<W: std::fmt::Debug>(wayland: bool, views: usize, target: W) -> String {
    if wayland {
        format!(
            "external activation: winit ignores unhide and unminimize for {views} view(s) on native Wayland; requesting compositor activation of {target:?}"
        )
    } else {
        format!(
            "external activation: restored {views} view(s), requesting OS activation of {target:?}"
        )
    }
}

fn log_os_activation(
    id: WindowId,
    result: Result<
        crate::platform::window_activation::ActivationRequested,
        crate::platform::window_activation::ActivationError,
    >,
) -> OsActivation {
    match result {
        Ok(kind) => {
            tracing::info!("external activation: requested {kind:?} for {id:?}");
            OsActivation::Done
        }
        Err(e) => log_os_activation_error(e),
    }
}

fn log_os_activation_error(
    error: crate::platform::window_activation::ActivationError,
) -> OsActivation {
    use crate::platform::window_activation::ActivationError;

    match error {
        ActivationError::WaylandUnsupported(why) => {
            tracing::info!("external activation: Wayland activation not requested: {why}");
            OsActivation::WaylandUnsupported
        }
        ActivationError::Other(why) => {
            tracing::warn!("external activation: OS activation not requested: {why}");
            OsActivation::Done
        }
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn a_request_without_evidence_changes_nothing() {
        assert_eq!(
            plan_activation(false, true, &[(1, true)], Some(1)),
            ActivationPlan::Ignore
        );
        assert_eq!(
            plan_activation::<u32>(false, true, &[], None),
            ActivationPlan::Ignore
        );
    }

    #[test]
    fn no_main_view_takes_the_new_window_path() {
        assert_eq!(
            plan_activation::<u32>(true, true, &[], None),
            ActivationPlan::OpenWindow
        );
        // 모달 View만 남은 경우도 MainView가 없는 것으로 본다.
        assert_eq!(
            plan_activation(true, true, &[(5, false)], Some(5)),
            ActivationPlan::OpenWindow
        );
    }

    #[test]
    fn the_last_focused_hidden_view_wins_over_an_agent_window() {
        // 1 = 트레이로 숨긴 마지막 포커스 View, 2 = 그 뒤 에이전트가 만든 보이는 창(내부 포커스 불변).
        assert_eq!(
            plan_activation(true, true, &[(2, true), (1, true)], Some(1)),
            ActivationPlan::Restore {
                views: vec![1, 2],
                target: 1
            }
        );
    }

    #[test]
    fn a_focused_modal_view_is_the_target_and_is_restored_with_the_main_views() {
        assert_eq!(
            plan_activation(true, true, &[(1, true), (9, false)], Some(9)),
            ActivationPlan::Restore {
                views: vec![1, 9],
                target: 9
            }
        );
    }

    #[test]
    fn a_stale_or_missing_focus_falls_back_to_the_lowest_main_view() {
        let expected = ActivationPlan::Restore {
            views: vec![3, 4, 8],
            target: 3,
        };
        assert_eq!(
            plan_activation(true, true, &[(4, true), (8, false), (3, true)], Some(7)),
            expected
        );
        assert_eq!(
            plan_activation(true, true, &[(4, true), (8, false), (3, true)], None),
            expected
        );
    }

    #[test]
    fn wayland_raises_an_existing_view_only_with_a_token() {
        assert!(raises_existing_view(true, true));
        assert!(!raises_existing_view(true, false));
        // X11·Windows 는 토큰과 무관하게 기존 창 경로를 쓴다.
        assert!(raises_existing_view(false, false));
        assert!(raises_existing_view(false, true));
    }

    #[test]
    fn only_x11_and_windows_activate_a_new_window_again_after_creation() {
        // Wayland: 등록 뒤 다시 요청하지 않는다. 토큰이 있어 기존 창을 올릴 수 있는 요청도 같다.
        assert!(!activates_after_creation(true, true));
        assert!(!activates_after_creation(true, false));
        // X11·Windows: 생성 뒤 OS 활성화를 한 번 더 요청한다.
        assert!(activates_after_creation(false, true));
    }

    #[test]
    fn a_wayland_request_without_a_token_takes_the_new_window_path_only_with_evidence() {
        // 토큰 없는 Wayland 요청: 증거가 있으면 MainView가 있어도 OpenWindow(새 창 경로)다.
        assert_eq!(
            plan_activation(true, false, &[(2, true), (1, true)], Some(1)),
            ActivationPlan::OpenWindow
        );
        assert_eq!(
            plan_activation::<u32>(true, false, &[], None),
            ActivationPlan::OpenWindow
        );
        // 증거가 없으면 백엔드와 무관하게 아무것도 바꾸지 않는다.
        assert_eq!(
            plan_activation(false, false, &[(1, true)], Some(1)),
            ActivationPlan::Ignore
        );
    }

    #[test]
    fn the_restore_log_says_restored_only_off_native_wayland() {
        let x11 = restore_log_line(false, 2, 7);
        assert_eq!(
            x11,
            "external activation: restored 2 view(s), requesting OS activation of 7"
        );
        let wayland = restore_log_line(true, 2, 7);
        assert!(!wayland.contains("restored"), "{wayland}");
        assert!(wayland.contains("native Wayland"), "{wayland}");
        assert!(wayland.contains("2 view(s)"), "{wayland}");
        assert!(wayland.contains("compositor activation of 7"), "{wayland}");
    }
}
