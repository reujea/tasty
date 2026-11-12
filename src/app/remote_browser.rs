//! App binds Remote browse tickets to the requesting engine and View incarnation.
use super::App;
use crate::view::ui::View as _;
#[derive(Clone, Debug)]
pub(crate) enum BrowserRequest {
    Browse {
        id: u64,
        profile: String,
        view: std::sync::Weak<()>,
    },
    Create {
        id: u64,
    },
    Connect {
        id: u64,
        workspace: u32,
    },
    Cancel {
        id: u64,
    },
}
pub(crate) struct BrowserTarget {
    engine: crate::runtime::engine_session::EngineId,
    window: winit::window::WindowId,
    view: std::sync::Weak<()>,
    /// 이 창에서 고른 attach 프로필 이름. 연결한 mirror의 호스트 글자가 된다.
    profile: String,
}
impl App {
    pub(crate) fn remote_browser_request(
        &mut self,
        engine: crate::runtime::engine_session::EngineId,
        request: BrowserRequest,
    ) -> Result<(), String> {
        let id = match &request {
            BrowserRequest::Browse { id, .. }
            | BrowserRequest::Create { id }
            | BrowserRequest::Connect { id, .. }
            | BrowserRequest::Cancel { id } => *id,
        };
        if matches!(request, BrowserRequest::Cancel { .. }) {
            self.remote.cancel_browser(id);
            self.state.remote_browsers.remove(&id);
            return Ok(());
        }
        let proxy = self.view.proxy.clone();
        let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            if proxy.send_event(crate::AppEvent::AutoAttachReady).is_err() {
                tracing::debug!("remote browser wake receiver closed");
            }
        });
        match request {
            BrowserRequest::Browse { id, profile, view } => {
                let target = self
                    .engines()
                    .window_pairs()
                    .find_map(|(window, main, _)| {
                        (self.engines.of_window(window) == Some(engine)
                            && main.base.state.matches_identity(&view))
                        .then(|| BrowserTarget {
                            engine,
                            window,
                            view: main.base.state.identity(),
                            profile: profile.clone(),
                        })
                    })
                    .ok_or("remote browser View disappeared")?;
                self.state.remote_browsers.insert(id, target);
                if let Err(message) = self.remote.begin_browser(id, profile, wake) {
                    self.display_browser_update(
                        id,
                        tasty_remote::browser::BrowserUpdate::Failed {
                            creating: false,
                            message,
                        },
                    );
                }
            }
            BrowserRequest::Create { id } => {
                if !self.browser_target_is_current(id, engine) {
                    return Err("remote browser target retired".into());
                }
                let inherit_cwd = self.browser_inherit_cwd(id);
                if let Err(message) = self.remote.create_in_browser(id, wake, inherit_cwd) {
                    self.display_browser_update(
                        id,
                        tasty_remote::browser::BrowserUpdate::Failed {
                            creating: true,
                            message,
                        },
                    );
                }
            }
            BrowserRequest::Connect { id, workspace } => {
                if !self.browser_target_is_current(id, engine) {
                    self.remote.cancel_browser(id);
                    self.state.remote_browsers.remove(&id);
                    return Ok(());
                }
                let profile = self
                    .state
                    .remote_browsers
                    .get(&id)
                    .map(|browser| browser.profile.clone());
                // 같은 대상의 저장된 원격 행이 있으면 새 행을 만들지 않고 그 행으로 연결한다.
                let existing = profile.as_ref().and_then(|profile| {
                    self.saved_row_for_binding(&crate::model::RemoteBinding {
                        profile: profile.clone(),
                        remote_workspace: workspace,
                    })
                });
                if let Some(row) = existing
                    && self
                        .remote
                        .sessions
                        .iter()
                        .any(|session| session.state.saved_row == Some(row))
                {
                    // 형상이 남은 행은 그 행의 재연결 경로로 붙인다. 팝업이 연 연결은 쓰지 않는다.
                    let (_, tunnel) = self.remote.take_browser_connection(id)?;
                    self.remote.retire_tunnel(tunnel);
                    self.state.remote_browsers.remove(&id);
                    self.remote.user_disconnected.remove(&row);
                    self.connect_saved_row(row, true);
                    self.focus_mirror_workspace(row);
                    return Ok(());
                }
                if let Some(row) = existing {
                    self.take_over_saved_row(row);
                }
                let queued = self.queue_popup_mirror(id, engine, existing, profile, workspace);
                if let (Err(error), Some(row)) = (&queued, existing) {
                    self.end_taken_over_row(row, error);
                }
                queued?;
            }
            BrowserRequest::Cancel { .. } => {}
        }
        Ok(())
    }
    /// 팝업 연결을 그 창(저장 행이면 그 행)의 mirror 로 설치하도록 넘긴다.
    fn queue_popup_mirror(
        &mut self,
        id: u64,
        engine: crate::runtime::engine_session::EngineId,
        existing: Option<u32>,
        profile: Option<String>,
        workspace: u32,
    ) -> Result<(), String> {
        let mut target = self
            .mirror_install_target(existing.is_none().then_some(engine), existing, None, true)
            .map_err(|error| error.to_string())?;
        target.user = true;
        target.profile = profile;
        let (port, tunnel) = self.remote.take_browser_connection(id)?;
        self.state.remote_browsers.remove(&id);
        self.queue_browser_mirror(target, port, workspace, tunnel)
            .map_err(|error| error.to_string())
    }

    /// 형상 없는 저장 행을 팝업 연결로 붙이기 전에 그 행의 자동 시도를 취소하고 표지를 지운다.
    /// 두 시도가 같은 원격 workspace 를 두고 겨루면 늦은 쪽이 occupied 로 끝나 연결된 행에 안내가 남는다.
    /// 넘겨받은 팝업 연결은 그 행의 진행 중 시도다. 연결이 끝날 때까지 행은 연결 중이고, 보류한 요청과
    /// 합류한 run 은 그 연결의 결과로 끝난다(실패는 `fail_pending_mirror`, 성공은 설치된 세션).
    pub(crate) fn take_over_saved_row(&mut self, row: u32) {
        self.abandon_row_attempts(row, "replaced by a connection from the remote popup");
        self.ensure_row_token(row);
        self.remote.user_disconnected.remove(&row);
        self.remote.pending_reactivation.remove(&row);
        self.remote.active.insert(row);
    }

    /// 넘겨받은 팝업 연결이 설치 대기열에 들기 전에 실패하면 그 행의 첫 연결 실패로 끝낸다.
    fn end_taken_over_row(&mut self, row: u32, error: &str) {
        self.remote.active.remove(&row);
        if let Some(binding) = self.saved_row_binding(row) {
            self.on_first_connect_failed(row, &binding, &anyhow::anyhow!(error.to_owned()));
        }
    }

    /// 원격 "+ 새 워크스페이스"가 싣는 cwd 상속 원본. 팝업을 연 창의 포커스가 같은 원격(같은 attach 프로필)의
    /// mirror 터미널이면 그 원격 ID 를 싣는다. client `inherit_cwd`가 꺼져 있으면 "상속 안 함"이다. 같은
    /// 서버인지 판정할 수 없으면(포커스가 로컬 surface, 프로필 없이 붙은 mirror, 다른 프로필) 싣지 않아
    /// 원격 Tasty가 자기 포커스와 자기 설정으로 정한다.
    fn browser_inherit_cwd(&mut self, id: u64) -> Option<tasty_ipc::stream::InheritCwd> {
        let (engine, window, profile) = self
            .state
            .remote_browsers
            .get(&id)
            .map(|target| (target.engine, target.window, target.profile.clone()))?;
        let (main, session) = self.engines().window_pair(window)?;
        if self.engines.of_window(window) != Some(engine) {
            return None;
        }
        let inherit = session.runtime.settings.general.inherit_cwd;
        let focused = main
            .state
            .focused_surface_id(session.core)
            .filter(|surface| {
                session
                    .core
                    .find_surface_by_id(*surface)
                    .is_some_and(|found| found.kind() == "terminal")
            });
        let remote = focused.and_then(|local| {
            remote_surface_of_profile(
                self.remote
                    .sessions
                    .iter()
                    .map(|s| (s.state.profile.as_deref(), &s.state.remote_to_local)),
                &profile,
                local,
            )
        });
        tasty_ipc::stream::InheritCwd::of_client(inherit, remote)
    }

    fn browser_target_is_current(
        &self,
        id: u64,
        engine: crate::runtime::engine_session::EngineId,
    ) -> bool {
        self.state.remote_browsers.get(&id).is_some_and(|target| {
            target.engine == engine
                && self.engines.of_window(target.window) == Some(engine)
                && self
                    .view
                    .views
                    .get(&target.window)
                    .and_then(|view| view.as_main())
                    .is_some_and(|view| view.base.state.matches_identity(&target.view))
        })
    }
    fn display_browser_update(&mut self, id: u64, update: tasty_remote::browser::BrowserUpdate) {
        let Some(target) = self.state.remote_browsers.get(&id) else {
            return;
        };
        if !self.browser_target_is_current(id, target.engine) {
            return;
        }
        if let Some(main) = self
            .view
            .views
            .get_mut(&target.window)
            .and_then(|view| view.as_main_mut())
        {
            crate::adapters::ui::popup::remote_attach::receive_update(
                &main.base.gpu.egui_ctx,
                id,
                update,
            );
            main.mark_dirty();
        }
    }
    pub(crate) fn poll_remote_browsers(&mut self) {
        let retired: Vec<_> = self
            .state
            .remote_browsers
            .iter()
            .filter(|(id, target)| !self.browser_target_is_current(**id, target.engine))
            .map(|(id, _)| *id)
            .collect();
        for id in retired {
            self.remote.cancel_browser(id);
            self.state.remote_browsers.remove(&id);
        }
        for (id, update) in self.remote.poll_browsers() {
            self.display_browser_update(id, update);
        }
    }
}

/// 로컬 mirror surface 의 원격 ID. 그 surface 를 가진 mirror 세션이 같은 attach 프로필로 연결됐을 때만 준다.
/// 세션에는 서버 `instance_id` 가 없으므로 같은 서버인지는 연결 때 고른 프로필로 판정한다.
fn remote_surface_of_profile<'a>(
    sessions: impl IntoIterator<Item = (Option<&'a str>, &'a std::collections::HashMap<u32, u32>)>,
    profile: &str,
    local: u32,
) -> Option<u32> {
    sessions
        .into_iter()
        .filter(|(session_profile, _)| *session_profile == Some(profile))
        .find_map(|(_, remote_to_local)| {
            remote_to_local
                .iter()
                .find(|(_, mapped)| **mapped == local)
                .map(|(remote, _)| *remote)
        })
}

#[cfg(test)]
mod tests {
    use super::remote_surface_of_profile;
    use std::collections::HashMap;

    /// 같은 프로필의 세션만 원격 ID 를 준다. 프로필 없는 mirror 와 다른 프로필은 같은 서버로 보지 않는다.
    #[test]
    fn only_a_mirror_of_the_same_profile_names_its_remote_surface() {
        let same: HashMap<u32, u32> = [(40, 3)].into_iter().collect();
        let other: HashMap<u32, u32> = [(50, 4)].into_iter().collect();
        let unnamed: HashMap<u32, u32> = [(60, 5)].into_iter().collect();
        let sessions = || {
            [
                (Some("other"), &other),
                (None, &unnamed),
                (Some("gx10"), &same),
            ]
        };
        assert_eq!(remote_surface_of_profile(sessions(), "gx10", 3), Some(40));
        assert_eq!(remote_surface_of_profile(sessions(), "gx10", 4), None);
        assert_eq!(remote_surface_of_profile(sessions(), "gx10", 5), None);
        assert_eq!(remote_surface_of_profile(sessions(), "gx10", 9), None);
    }
}
