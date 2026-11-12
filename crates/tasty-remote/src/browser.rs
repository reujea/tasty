//! Popup browse/create execution. UI owns only a request ID and copied progress values.
use crate::{
    browse::RemoteWorkspace,
    outbound::{AttemptToken, Remote},
};
use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
/// Browse timeout shared with the host's progress display.
pub const BROWSE_DEADLINE: Duration = Duration::from_secs(20);

pub type BrowserId = u64;
pub enum BrowserUpdate {
    Listed(Vec<RemoteWorkspace>),
    Created(u32),
    Failed { creating: bool, message: String },
}
struct Session {
    attempt: Option<AttemptToken>,
    port: Option<u16>,
    tunnel: Option<tasty_ssh::SshTunnel>,
    started: Instant,
    creating: bool,
}
enum ResultValue {
    Listed {
        port: u16,
        tunnel: Option<tasty_ssh::SshTunnel>,
        rows: Vec<RemoteWorkspace>,
    },
    Created(u32),
}
struct Outcome {
    id: BrowserId,
    attempt: AttemptToken,
    result: Result<ResultValue, String>,
}
pub(crate) struct Browsers {
    sessions: HashMap<BrowserId, Session>,
    tx: mpsc::SyncSender<Outcome>,
    rx: mpsc::Receiver<Outcome>,
}
impl Default for Browsers {
    fn default() -> Self {
        let (tx, rx) = mpsc::sync_channel(64);
        Self {
            sessions: HashMap::new(),
            tx,
            rx,
        }
    }
}
/// 엔드포인트를 해석하고 원격 workspace 목록을 받는다. 해석은 시험에서 바꿔 끼울 수 있게 받는다.
fn list_endpoint<T: crate::self_instance::TunnelVerdict>(
    resolve: impl FnOnce() -> anyhow::Result<(Option<T>, u16)>,
    token: &AttemptToken,
) -> anyhow::Result<(Option<T>, u16, Vec<RemoteWorkspace>)> {
    let (tunnel, port) = resolve()?;
    if !token.is_active() {
        anyhow::bail!("remote browser cancelled");
    }
    // 연결과 원격 workspace 생성이 모두 이 엔드포인트를 쓰므로 목록을 받기 전에 판정한다.
    crate::self_instance::refuse_this_instance_over(tunnel.as_ref(), port, Some(token))?;
    let rows = crate::browse::browse_via_port_bound(port, Some(token))?;
    Ok((tunnel, port, rows))
}

/// 팝업에 보일 실패 문구. 자기 자신 거절은 번역한 안내로 바꾼다. 리스너 없음 판정은 맥락으로
/// 붙으므로 원인까지 함께 보인다.
fn browser_error_text(error: anyhow::Error) -> String {
    if error
        .downcast_ref::<crate::self_instance::ThisInstance>()
        .is_some()
    {
        tasty_i18n::t("remote_attach.self_instance").to_owned()
    } else if error
        .downcast_ref::<tasty_ssh::RemoteListenerAbsent>()
        .is_some()
    {
        format!("{error:#}")
    } else {
        error.to_string()
    }
}

impl Remote {
    pub fn begin_browser(
        &mut self,
        id: BrowserId,
        profile: String,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), String> {
        self.cancel_browser(id);
        if self.browsers.sessions.len() >= 64 {
            return Err("remote browser capacity exhausted".into());
        }
        let attempt = self.begin_attempt(None, None).map_err(str::to_owned)?;
        let cancel = tasty_ssh::SshCancel::new();
        attempt.register_ssh(cancel.clone())?;
        let token = attempt.clone();
        let tx = self.browsers.tx.clone();
        self.browsers.sessions.insert(
            id,
            Session {
                attempt: Some(attempt.clone()),
                port: None,
                tunnel: None,
                started: Instant::now(),
                creating: false,
            },
        );
        let result = self.spawn_attempt(attempt, move || {
            let _scope = cancel.scope();
            let result = list_endpoint(
                || {
                    let (target, tasty, mode, file) =
                        crate::browse::resolve_connection_spec(Some(&profile), None, "", "")?;
                    crate::browse::resolve_endpoint(&target, &tasty, &mode, file.as_deref())
                },
                &token,
            )
            .map(|(tunnel, port, rows)| ResultValue::Listed { port, tunnel, rows })
            .map_err(browser_error_text);
            crate::outbound::send_attempt_result(
                &tx,
                &token,
                Outcome {
                    id,
                    attempt: token.clone(),
                    result,
                },
            );
            wake();
        });
        if result.is_err() {
            self.cancel_browser(id);
        }
        result
    }
    /// 원격 workspace 를 만든다. `inherit_cwd` 는 client 가 정한 첫 터미널의 cwd 상속 원본이다.
    /// None 이면 싣지 않고 원격이 자기 포커스와 자기 설정으로 정한다.
    pub fn create_in_browser(
        &mut self,
        id: BrowserId,
        wake: Arc<dyn Fn() + Send + Sync>,
        inherit_cwd: Option<tasty_ipc::stream::InheritCwd>,
    ) -> Result<(), String> {
        let session = self
            .browsers
            .sessions
            .get(&id)
            .ok_or("remote browser not found")?;
        if session.attempt.is_some() {
            return Err("remote browser operation in progress".into());
        }
        let port = session.port.ok_or("remote browser has no endpoint")?;
        let attempt = self.begin_attempt(None, None).map_err(str::to_owned)?;
        let token = attempt.clone();
        let tx = self.browsers.tx.clone();
        if let Some(session) = self.browsers.sessions.get_mut(&id) {
            session.attempt = Some(attempt.clone());
            session.started = Instant::now();
            session.creating = true;
        }
        let mut params = serde_json::json!({});
        if let Some(inherit_cwd) = &inherit_cwd {
            params["inherit_cwd"] = serde_json::json!(inherit_cwd);
        }
        let result = self.spawn_attempt(attempt, move || {
            let result =
                crate::browse::probe_method_bound(port, "workspace.create", params, Some(&token))
                    .map_err(|error| error.to_string())
                    .and_then(|value| {
                        value
                            .get("id")
                            .and_then(|id| id.as_u64())
                            .and_then(|id| u32::try_from(id).ok())
                            .map(ResultValue::Created)
                            .ok_or_else(|| {
                                tasty_i18n::t("remote_attach.create_failed_generic").to_owned()
                            })
                    });
            crate::outbound::send_attempt_result(
                &tx,
                &token,
                Outcome {
                    id,
                    attempt: token.clone(),
                    result,
                },
            );
            wake();
        });
        if result.is_err()
            && let Some(session) = self.browsers.sessions.get_mut(&id)
        {
            session.attempt = None;
        }
        result
    }
    pub fn cancel_browser(&mut self, id: BrowserId) {
        if let Some(session) = self.browsers.sessions.remove(&id) {
            if let Some(attempt) = session.attempt {
                self.cancel_attempt(&attempt);
            }
            self.retire_tunnel(session.tunnel);
        }
    }
    pub fn take_browser_connection(
        &mut self,
        id: BrowserId,
    ) -> Result<(u16, Option<tasty_ssh::SshTunnel>), String> {
        let session = self
            .browsers
            .sessions
            .get(&id)
            .ok_or("remote browser not found")?;
        if session.attempt.is_some() {
            return Err("remote browser operation in progress".into());
        }
        let port = session.port.ok_or("remote browser has no endpoint")?;
        let session = self
            .browsers
            .sessions
            .remove(&id)
            .ok_or("remote browser disappeared")?;
        Ok((port, session.tunnel))
    }
    pub fn poll_browsers(&mut self) -> Vec<(BrowserId, BrowserUpdate)> {
        self.reap_attempts();
        let mut updates = Vec::new();
        for _ in 0..64 {
            let Ok(outcome) = self.browsers.rx.try_recv() else {
                break;
            };
            let valid = self
                .browsers
                .sessions
                .get(&outcome.id)
                .and_then(|session| session.attempt.as_ref())
                .is_some_and(|attempt| *attempt == outcome.attempt);
            if !valid {
                self.discard_browser_outcome(outcome);
                continue;
            }
            if self.finish_attempt(&outcome.attempt).is_none() {
                self.discard_browser_outcome(outcome);
                continue;
            }
            let Some(session) = self.browsers.sessions.get_mut(&outcome.id) else {
                continue;
            };
            session.attempt = None;
            let update = match outcome.result {
                Ok(ResultValue::Listed { port, tunnel, rows }) => {
                    session.port = Some(port);
                    session.tunnel = tunnel;
                    BrowserUpdate::Listed(rows)
                }
                Ok(ResultValue::Created(id)) => BrowserUpdate::Created(id),
                Err(message) => BrowserUpdate::Failed {
                    creating: session.creating,
                    message,
                },
            };
            updates.push((outcome.id, update));
        }
        let expired: Vec<_> = self
            .browsers
            .sessions
            .iter()
            .filter(|(_, session)| {
                session.attempt.is_some()
                    && session.started.elapsed()
                        >= if session.creating {
                            Duration::from_secs(10)
                        } else {
                            BROWSE_DEADLINE
                        }
            })
            .map(|(id, session)| (*id, session.creating))
            .collect();
        for (id, creating) in expired {
            if let Some(attempt) = self
                .browsers
                .sessions
                .get_mut(&id)
                .and_then(|session| session.attempt.take())
            {
                self.cancel_attempt(&attempt);
            }
            updates.push((
                id,
                BrowserUpdate::Failed {
                    creating,
                    message: tasty_i18n::t(if creating {
                        "remote_attach.create_timeout"
                    } else {
                        "remote_attach.timeout"
                    })
                    .replace(
                        "{secs}",
                        &if creating {
                            10
                        } else {
                            BROWSE_DEADLINE.as_secs()
                        }
                        .to_string(),
                    ),
                },
            ));
        }
        updates
    }
    fn discard_browser_outcome(&mut self, outcome: Outcome) {
        if let Ok(ResultValue::Listed { tunnel, .. }) = outcome.result {
            self.retire_tunnel(tunnel);
        }
    }
    pub(crate) fn shutdown_browsers(&mut self) {
        let ids: Vec<_> = self.browsers.sessions.keys().copied().collect();
        for id in ids {
            self.cancel_browser(id);
        }
        while let Ok(outcome) = self.browsers.rx.try_recv() {
            self.discard_browser_outcome(outcome);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_instance::serve_once;

    fn token() -> (crate::outbound::Remote, AttemptToken) {
        let mut remote = crate::outbound::Remote::new();
        let token = remote.begin_attempt(None, None).expect("attempt");
        (remote, token)
    }

    #[test]
    fn the_popup_asks_a_tunnel_peer_before_listing_and_shows_the_notice() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 오류 문구를 만들기 전에 초기화한다.
        tasty_i18n::init("en");
        let (port, server) =
            serve_once(serde_json::json!({ "instance_id": tasty_ipc::instance::instance_id() }));
        let (_remote, token) = token();
        let error = list_endpoint(|| Ok((Some(()), port)), &token).expect_err("this instance");
        assert!(server.join().expect("server").contains("\"system.info\""));
        assert_eq!(
            browser_error_text(error),
            tasty_i18n::t("remote_attach.self_instance")
        );
    }

    /// 목록 전 자기 판정이 원격 포트의 리스너 없음으로 끊기면 팝업 문구에 그 안내가 나온다.
    #[test]
    fn the_popup_says_nothing_listens_on_the_remote_port() {
        tasty_i18n::init("en");
        let (port, server) = crate::self_instance::close_after_request();
        let (_remote, token) = token();
        let verdict = tasty_ssh::ListenerVerdict::with_observation(4321, true);
        let error = list_endpoint(|| Ok((Some(verdict), port)), &token).expect_err("dropped");
        server.join().expect("server");
        let expected = tasty_ssh::RemoteListenerAbsent { remote_port: 4321 }.to_string();
        let cause = error.root_cause().to_string();
        let text = browser_error_text(error);
        // 안내와 끊긴 원인이 함께 보여야 한다.
        assert!(text.contains(&expected), "{text}");
        assert!(text.contains(&cause), "{text}");
    }

    /// 채널 연결 실패를 보지 못한 끊김은 원래 문구 그대로 보인다.
    #[test]
    fn the_popup_keeps_the_original_text_when_a_listener_answered() {
        tasty_i18n::init("en");
        let (port, server) = crate::self_instance::close_after_request();
        let (_remote, token) = token();
        let verdict = tasty_ssh::ListenerVerdict::with_observation(4321, false);
        let error = list_endpoint(|| Ok((Some(verdict), port)), &token).expect_err("dropped");
        server.join().expect("server");
        let original = error.to_string();
        let text = browser_error_text(error);
        assert_eq!(text, original);
        let absent = tasty_ssh::RemoteListenerAbsent { remote_port: 4321 }.to_string();
        assert!(!text.contains(&absent), "{text}");
    }

    #[test]
    fn the_popup_lists_a_direct_endpoint_without_asking() {
        let (port, server) = serve_once(serde_json::json!([]));
        let (_remote, token) = token();
        let listed = list_endpoint(|| Ok((None::<()>, port)), &token);
        let first = server.join().expect("server");
        assert!(first.contains("\"workspace.list\""), "{first}");
        assert!(listed.is_ok(), "{listed:?}", listed = listed.err());
    }
}
