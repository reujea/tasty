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
                if let Err(message) = self.remote.create_in_browser(id, wake) {
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
                    self.ensure_row_token(row);
                }
                let mut target = self
                    .mirror_install_target(
                        existing.is_none().then_some(engine),
                        existing,
                        None,
                        true,
                    )
                    .map_err(|error| error.to_string())?;
                target.user = true;
                target.profile = profile;
                if let Some(row) = existing {
                    self.remote.user_disconnected.remove(&row);
                    self.remote.pending_reactivation.remove(&row);
                }
                let (port, tunnel) = self.remote.take_browser_connection(id)?;
                self.state.remote_browsers.remove(&id);
                self.queue_browser_mirror(target, port, workspace, tunnel)
                    .map_err(|error| error.to_string())?;
            }
            BrowserRequest::Cancel { .. } => {}
        }
        Ok(())
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
