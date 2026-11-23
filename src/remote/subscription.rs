//! Engine-owned attach receivers. No forwarding thread outlives its grant or physical terminal.
use crate::runtime::engine_access::EngineMut;
use std::sync::Weak;
use tasty_ipc::stream::{StreamControl, StreamFrame, StreamTag, encode_mux};
use tasty_ipc::stream_hub::{PushResult, StreamHub};

pub(crate) struct Subscription {
    surface: u32,
    client: u32,
    grant: u64,
    generation: tasty_terminal::ResourceGeneration,
    binding: Weak<()>,
    hub: StreamHub,
    mux: bool,
    events: tasty_terminal::AttachEventReceiver,
}
impl Subscription {
    /// 이 구독이 지금 출력을 보낼 수 있는지: 같은 holder의 ready grant, 같은 터미널 세대, 같은 연결.
    fn is_current(
        &self,
        grant: Option<&crate::core::attach::AttachLock>,
        terminals: &crate::runtime::terminal_store::TerminalStore,
    ) -> bool {
        grant.is_some_and(|grant| {
            grant.ready && grant.holder == self.client && grant.granted_seq == self.grant
        }) && terminals.matches_generation(self.surface, self.generation)
            && self.hub.matches_client_binding(self.client, &self.binding)
    }
}

impl EngineMut<'_> {
    pub(crate) fn subscribe_terminal(
        &mut self,
        surface: u32,
        client: u32,
        hub: &StreamHub,
        mux: bool,
        announce: bool,
    ) {
        let Some(binding) = hub.client_binding(client) else {
            return;
        };
        let Some((_, grant)) = self
            .live
            .occupancy
            .locks_snapshot()
            .into_iter()
            .find(|(id, grant)| *id == surface && grant.holder == client && grant.ready)
        else {
            return;
        };
        if self.remote.attach_subscriptions.len() >= 1024
            && !self
                .remote
                .attach_subscriptions
                .contains_key(&(surface, client))
        {
            tracing::warn!(surface, client, "attach subscription capacity exhausted");
            hub.unregister_bound(client, &binding);
            return;
        }
        let Some(terminal) = self.runtime.terminals.get_mut(surface) else {
            return;
        };
        let generation = terminal.resource_generation();
        let tasty_terminal::AttachStreamSubscription {
            snapshot,
            cols,
            rows,
            events,
        } = terminal.snapshot_and_stream();
        if announce {
            let descriptor = serde_json::json!({"event":"attached","surface_id":surface,"cols":cols,"rows":rows});
            if hub.push_bound(
                client,
                &binding,
                StreamFrame::new(
                    StreamTag::Control,
                    serde_json::to_vec(&descriptor).unwrap_or_default(),
                ),
            ) != PushResult::Sent
            {
                hub.unregister_bound(client, &binding);
                return;
            }
        }
        let payload = if mux {
            encode_mux(surface, &snapshot)
        } else {
            snapshot
        };
        if hub.push_bound(client, &binding, StreamFrame::new(StreamTag::Data, payload))
            != PushResult::Sent
        {
            hub.unregister_bound(client, &binding);
            return;
        }
        self.remote.attach_subscriptions.insert(
            (surface, client),
            Subscription {
                surface,
                client,
                grant: grant.granted_seq,
                generation,
                binding,
                hub: hub.clone(),
                mux,
                events,
            },
        );
    }
    pub(crate) fn poll_attach_subscriptions(&mut self) {
        self.poll_html_transfers();
        let grants: std::collections::HashMap<_, _> =
            self.live.occupancy.locks_snapshot().into_iter().collect();
        let mut budget = 128usize;
        let mut byte_budget = 1024 * 1024usize;
        let mut needs_poll = false;
        for (key, mut subscription) in std::mem::take(&mut self.remote.attach_subscriptions) {
            let current =
                subscription.is_current(grants.get(&subscription.surface), &self.runtime.terminals);
            if !current {
                continue;
            }
            let mut keep = true;
            while budget != 0 && byte_budget != 0 {
                let event = match subscription.events.try_recv() {
                    Ok(event) => event,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        keep = false;
                        break;
                    }
                };
                budget -= 1;
                let bytes = match &event {
                    tasty_terminal::AttachEvent::Output(bytes) => bytes.len(),
                    _ => 32,
                };
                byte_budget = byte_budget.saturating_sub(bytes);
                let frame = match event {
                    tasty_terminal::AttachEvent::Output(bytes) => StreamFrame::new(
                        StreamTag::Data,
                        if subscription.mux {
                            encode_mux(subscription.surface, &bytes)
                        } else {
                            bytes
                        },
                    ),
                    tasty_terminal::AttachEvent::Resize { cols, rows } => StreamFrame::new(
                        StreamTag::Control,
                        serde_json::to_vec(&StreamControl::Resize {
                            surface_id: subscription.surface,
                            cols,
                            rows,
                        })
                        .unwrap_or_default(),
                    ),
                    tasty_terminal::AttachEvent::Loss => {
                        // Loss-aware clients resnapshot; older clients still observe the forced EOF.
                        subscription.hub.push_bound(
                            subscription.client,
                            &subscription.binding,
                            StreamFrame::new(
                                StreamTag::Control,
                                serde_json::to_vec(&StreamControl::Loss { frames: 1 })
                                    .unwrap_or_default(),
                            ),
                        );
                        subscription
                            .hub
                            .unregister_bound(subscription.client, &subscription.binding);
                        keep = false;
                        break;
                    }
                };
                if matches!(
                    subscription
                        .hub
                        .push_bound(subscription.client, &subscription.binding, frame),
                    PushResult::Unknown | PushResult::Disconnected
                ) {
                    keep = false;
                    break;
                }
            }
            if keep {
                self.remote.attach_subscriptions.insert(key, subscription);
                needs_poll |= budget == 0 || byte_budget == 0;
            }
        }
        if needs_poll {
            (self.runtime.waker)();
        }
    }
}
