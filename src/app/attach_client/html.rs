//! HTML transport wiring. Native viewer installation is intentionally a separate capability.
use tasty_ipc::{
    html_transfer::HtmlReply,
    stream_hub::{PumpOutcome, StreamHub},
};
use tasty_remote::client_session::{AttachClientSession, MirrorEvent};
impl crate::app::App {
    pub(in crate::app) fn apply_html_batch(&mut self, outcome: &mut PumpOutcome, hub: &StreamHub) {
        for (client, msg) in outcome.html_requests.drain(..) {
            if let Some(mut engine) = crate::app::event_handler::find_workspace_holder_engine_mut(
                self.engines_mut(),
                client,
            ) {
                engine.handle_html_control(hub, client, msg);
            } else {
                crate::remote::html_transfer::refuse(hub, client, msg);
            }
        }
    }
}
pub(super) fn parse(payload: &[u8]) -> Option<MirrorEvent> {
    if let Ok(reply) = serde_json::from_slice::<HtmlReply>(payload) {
        return Some(MirrorEvent::Html(reply));
    }
    let reply = serde_json::from_slice::<tasty_ipc::html_http::HttpReply>(payload).ok()?;
    reply.validate().ok()?;
    Some(MirrorEvent::Http(reply))
}

pub(super) fn receive(sess: &mut AttachClientSession, reply: HtmlReply) {
    let remote = reply.id().surface_id;
    if let Some(mirror) = sess.transport.html.mirrors.get(&remote) {
        lock_mirror(mirror).receive(reply);
        return;
    }
    if sess.state.remote_to_local.contains_key(&remote)
        && let Some(document) = sess.transport.html.documents.get_mut(&remote)
    {
        document.receive(reply);
    }
}
use crate::remote::mirror_role::VIEWER;
pub(super) fn retain(sess: &mut AttachClientSession, surfaces: &[serde_json::Value]) {
    sess.transport
        .html
        .http_documents
        .retain_with_sender(&sess.transport.frame_tx, |remote| {
            surfaces.iter().any(|surface| {
                surface["remote_id"].as_u64() == Some(remote as u64) && surface["role"] == VIEWER
            })
        });
    sess.transport.html.mirrors.retain(|remote, mirror| {
        let keep = surfaces.iter().any(|surface| {
            surface["remote_id"].as_u64() == Some(*remote as u64) && surface["role"] == VIEWER
        });
        if !keep {
            lock_mirror(mirror).disconnect();
        }
        keep
    });
    sess.transport.html.documents.retain(|remote, _| {
        surfaces.iter().any(|surface| {
            surface["remote_id"].as_u64() == Some(*remote as u64) && surface["role"] == VIEWER
        })
    });
}

#[cfg(all(
    feature = "gui",
    any(target_os = "linux", target_os = "macos", windows)
))]
pub(super) mod http;
mod viewer;
pub(super) use viewer::synchronize;
static MIRROR_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(crate) fn lock_mirror(
    mirror: &std::sync::Mutex<tasty_remote::html_mirror::HtmlMirror>,
) -> std::sync::MutexGuard<'_, tasty_remote::html_mirror::HtmlMirror> {
    crate::poison::recover_mutex(mirror.lock(), "remote HTML mirror", &MIRROR_POISONED)
}
pub(super) fn viewer_available() -> bool {
    cfg!(all(
        feature = "gui",
        any(target_os = "linux", target_os = "macos", windows)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_ipc::html_http::{CommandKind, FailureCode, HttpReply};
    #[test]
    fn http_session_parser_preserves_rejection_and_terminal_reply_kinds_without_file_fallback() {
        let id = tasty_ipc::html_transfer::TransferId {
            epoch: 1,
            surface_id: 2,
            binding: 3,
            document: 4,
            request: 5,
            transfer: 6,
        };
        let rejection = HttpReply::CommandRejected {
            id,
            source_generation: 7,
            response: None,
            command: CommandKind::Credit,
            code: FailureCode::InProgress,
            reason: "Pending".into(),
        };
        assert!(matches!(
            parse(&rejection.frame().unwrap().payload),
            Some(MirrorEvent::Http(HttpReply::CommandRejected {
                command: CommandKind::Credit,
                ..
            }))
        ));
        let terminal = HttpReply::Failure {
            id,
            source_generation: 7,
            response: None,
            code: FailureCode::Network,
            reason: "Network".into(),
        };
        assert!(matches!(
            parse(&terminal.frame().unwrap().payload),
            Some(MirrorEvent::Http(HttpReply::Failure { .. }))
        ));
        let mut malformed = serde_json::to_value(&terminal).unwrap();
        malformed["reason"] = "bad\nreason".into();
        assert!(parse(&serde_json::to_vec(&malformed).unwrap()).is_none());
        assert!(parse(br#"{"event":"future_html_http_reply"}"#).is_none());
    }
}
