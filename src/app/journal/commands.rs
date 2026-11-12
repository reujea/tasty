//! Accepted structural requests retain their original identity until a committed wire reply exists.
mod assembly;
mod category;
mod child;
mod close;
mod completion;
mod create;
mod create_spec;
mod display;
#[cfg(feature = "gui")]
mod divider;
#[cfg(feature = "gui")]
mod forwarding;
#[cfg(not(feature = "gui"))]
mod headless;
pub(crate) mod inbound;
mod intents;
pub(crate) mod live_resume;
mod notification;
mod replacement;
mod scheduling;
pub(crate) mod split_floor;
mod tab;
#[cfg(feature = "gui")]
mod view_completion;
mod wake;
mod workspace;

use super::*;
use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::runtime::journal_product::JournalError;
use crate::runtime::journal_product::{Admission, ResponsePlan, StreamCommand};
use std::sync::mpsc::SyncSender;

const MAX_PENDING: usize = 64;

struct CategoryReservation {
    stream: String,
    name: String,
}

/// Live View continuation, never serialized with a structural command or replayed response.
#[derive(Clone)]
pub(crate) struct IntentViewContinuation {
    #[cfg(feature = "gui")]
    pub(crate) remote_request:
        Option<std::sync::Arc<tasty_remote::pending_request::PendingRequest>>,
    #[cfg(feature = "gui")]
    pub(crate) view: std::sync::Weak<()>,
    #[cfg(feature = "gui")]
    pub(crate) selection: std::sync::Weak<()>,
    pub(crate) activate_surface: Option<u32>,
    pub(crate) close_empty_engine: bool,
    #[cfg(feature = "gui")]
    pub(crate) preset_apply: bool,
    #[cfg(feature = "gui")]
    pub(crate) after_create: Option<crate::intent::CreateFollowup>,
    #[cfg(feature = "gui")]
    pub(crate) tutorial: Option<create::TutorialCreated>,
    #[cfg(feature = "gui")]
    pub(crate) tutorial_preparation: Option<std::sync::Weak<()>>,
    #[cfg(feature = "gui")]
    pub(crate) tutorial_surface: Option<(u32, crate::runtime::surface_binding::SurfaceBinding)>,
}

/// 끝난 category 초기화와 그 뒤에 적용할 설정.
#[cfg(feature = "gui")]
pub(crate) struct SettingsCompletion {
    pub(crate) generation: u64,
    pub(crate) settings: Option<Box<crate::settings::Settings>>,
    pub(crate) origin: crate::intent::IntentOrigin,
    pub(crate) done: Option<crate::app::settings_change::SettingsDone>,
    pub(crate) response: JsonRpcResponse,
}

#[cfg(feature = "gui")]
impl SettingsCompletion {
    /// 성공이면 적용할 설정과 결과를 기다리는 요청을 돌려준다. 초기화가 실패했으면 설정을 적용하지 않고
    /// 기다리는 요청에 거절을 넣은 뒤, 공통 오류 보고에 쓸 출처와 사유를 돌려준다.
    pub(crate) fn into_settings(
        self,
    ) -> Result<
        (
            crate::settings::Settings,
            crate::intent::IntentOrigin,
            Option<crate::app::settings_change::SettingsDone>,
        ),
        (crate::intent::IntentOrigin, String),
    > {
        if let Some(error) = self.response.error {
            if let Some(done) = self.done {
                done.finish(crate::app::settings_change::SettingsOutcome::Rejected(
                    format!("category reset failed: {}", error.message),
                ));
            }
            return Err((self.origin, error.message));
        }
        let settings = *self.settings.expect("successful settings continuation");
        Ok((settings, self.origin, self.done))
    }
}

#[allow(clippy::large_enum_variant)] // reason: 처리 중 요청만 담는 맵이라 MAX_PENDING(64)으로 상한이 걸려 있다 — 낭비는 몇 KB로 묶이고 Box 는 요청마다 붙는다
enum Reply {
    Resume(live_resume::Resume),
    Remote(inbound::RemoteReply),
    #[cfg(feature = "gui")]
    Divider {
        engine: EngineId,
        sequence: u64,
        origin: crate::intent::IntentOrigin,
    },
    #[cfg(feature = "gui")]
    Settings {
        settings: Option<Box<crate::settings::Settings>>,
        generation: u64,
        origin: crate::intent::IntentOrigin,
        /// 이 설정 변경을 기다리는 요청. 결과는 `finish_settings_results`가 넣는다.
        done: Option<crate::app::settings_change::SettingsDone>,
    },
    Ipc(SyncSender<JsonRpcResponse>),
    Plugin {
        plugin_id: String,
        call_id: u64,
        binding: std::sync::Weak<()>,
    },
    Intent {
        engine: EngineId,
        origin: crate::intent::IntentOrigin,
        view: Option<IntentViewContinuation>,
        /// 최종 결과를 받을 요청의 run([`crate::request_runs`]).
        completion: Option<u64>,
    },
}

struct IntentResult {
    #[cfg(feature = "gui")]
    view: Option<IntentViewContinuation>,
    engine: EngineId,
    origin: crate::intent::IntentOrigin,
    response: JsonRpcResponse,
    completion: Option<u64>,
}

impl IntentResult {
    /// 응답을 요청의 run에 최종 결과로 남긴다. 만든 surface가 있으면 그 ID를 싣는다.
    fn finish_completion(&self) {
        let Some(run) = self.completion else {
            return;
        };
        if let Some(error) = &self.response.error {
            crate::request_runs::finish(
                run,
                crate::request_runs::RunState::Failed,
                error.message.clone(),
                serde_json::json!({ "code": error.code }),
            );
            return;
        }
        let surface = self.response.result.as_ref().and_then(|value| {
            ["surface_id", "new_surface_id"]
                .iter()
                .find_map(|key| value.get(*key).and_then(serde_json::Value::as_u64))
        });
        let result = surface.map_or_else(
            || "completed".to_string(),
            |surface| format!("surface {surface} is ready"),
        );
        crate::request_runs::finish(
            run,
            crate::request_runs::RunState::Succeeded,
            result,
            serde_json::json!({ "surface_id": surface }),
        );
    }
}

struct Pending {
    request: JsonRpcRequest,
    engine_scope: Option<EngineId>,
    /// Fixed local metadata patches may follow earlier fixed patches before their ACK.
    ordered_metadata: bool,
    reply: Reply,
    queued: Option<Work>,
    /// A worker request for this ticket awaits its Finished. At most one is outstanding, because
    /// a second reply would reach the pump after `answer_command` removed its owner.
    in_flight: bool,
    needs_resolution: bool,
    category: Option<CategoryReservation>,
    resource: Option<create::Request>,
    child_spawn: Option<child::Spawn>,
    #[cfg(feature = "gui")]
    forward: Option<super::forward::Draft>,
    /// Reservation follows the original request until its final reply, including effect-owned copies.
    #[cfg(feature = "gui")]
    forward_reserved: usize,
    one_shot_reserved: usize,
    closing: Option<close::Request>,
    replacing: Option<replacement::Request>,
    close_cause: close::Cause,
    waiting_command: Option<String>,
    /// Operations a committed close is still retiring. While every one of them only awaits its
    /// resource receipts, the close no longer pauses observation.
    closing_operations: Vec<tasty_core::OperationId>,
    activation_wait: Option<super::creation::ActivationReceipt>,
    created: Option<create::Completed>,
    bytes: usize,
    replay: bool,
    admitted: bool,
    retry_counted: bool,
    fixed: Option<Vec<StreamCommand>>,
    display: Option<display::DisplayContinuation>,
}

impl Pending {
    fn take_retry_outcome(
        &mut self,
        result: &Result<ResultValue, JournalError>,
    ) -> Option<crate::ipc::handler::idempotency::RetryOutcome> {
        if self.retry_counted || self.request.idempotency_key.is_none() {
            return None;
        }
        let outcome = retry_outcome(result)?;
        self.retry_counted = true;
        Some(outcome)
    }
}

/// Whether one pending command pauses observation. A replacement always does. A close does until
/// it is committed, and again once any of its operations leaves the receipt wait (Finish, or the
/// gap before its reply), so its final facts and reply stay ordered before later inputs.
fn close_pauses(
    closing: bool,
    replacing: bool,
    committed: bool,
    operations: &[tasty_core::OperationId],
    awaiting_receipts: impl Fn(&tasty_core::OperationId) -> bool,
) -> bool {
    replacing
        || (closing
            && !(committed && !operations.is_empty() && operations.iter().all(awaiting_receipts)))
}

#[derive(Default)]
pub(super) struct Commands {
    pending: std::collections::BTreeMap<u64, Pending>,
    #[cfg(feature = "gui")]
    completed_dividers: Vec<(EngineId, u64, crate::intent::IntentOrigin, JsonRpcResponse)>,
    completed_plugins: Vec<(String, u64, std::sync::Weak<()>, JsonRpcResponse)>,
    completed_intents: Vec<IntentResult>,
    /// 다음 `admit_intent_request`가 가져갈 run. 호출자가 admission 앞뒤로 넣고 비운다.
    intent_completion: Option<u64>,
    completed_live: Vec<(live_resume::Resume, JsonRpcResponse)>,
    completed_remote: Vec<(inbound::RemoteReply, JsonRpcResponse)>,
    completed_host_events: Vec<(EngineId, notification::Notification)>,
    #[cfg(feature = "gui")]
    settings_generation: u64,
    #[cfg(feature = "gui")]
    completed_settings: Vec<SettingsCompletion>,
    /// Commands whose published creation could not confirm the replaced owner's retirement,
    /// kept only while a request still waits on them.
    unconfirmed_cleanup: std::collections::HashMap<String, String>,
}

impl Commands {
    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn note_unconfirmed_cleanup(&mut self, command: &str, reason: &str) {
        if self
            .pending
            .values()
            .any(|pending| pending.waiting_command.as_deref() == Some(command))
        {
            self.unconfirmed_cleanup
                .insert(command.to_owned(), reason.to_owned());
        }
    }

    fn forget_unconfirmed_cleanup(&mut self) {
        let pending = &self.pending;
        self.unconfirmed_cleanup.retain(|command, _| {
            pending
                .values()
                .any(|pending| pending.waiting_command.as_deref() == Some(command.as_str()))
        });
    }

    #[cfg(feature = "gui")]
    pub(super) fn has_remote_request(&self, engine: EngineId) -> bool {
        self.pending
            .values()
            .any(|pending| matches!(&pending.reply,Reply::Remote(remote) if remote.engine==engine))
            || self
                .completed_remote
                .iter()
                .any(|(remote, _)| remote.engine == engine)
    }
    /// A close pauses observation until it is committed and after its retirements finish. In
    /// between, while `awaiting_receipts` holds for every operation it prepared, its structure is
    /// already published and only resource receipts remain, so other inputs may proceed.
    pub(super) fn has_closing(
        &self,
        awaiting_receipts: impl Fn(&tasty_core::OperationId) -> bool,
    ) -> bool {
        self.pending.values().any(|pending| {
            close_pauses(
                pending.closing.is_some(),
                pending.replacing.is_some(),
                pending.waiting_command.is_some(),
                &pending.closing_operations,
                &awaiting_receipts,
            )
        })
    }
    #[cfg(feature = "gui")]
    pub(super) fn has_resource_request(&self, engine: EngineId) -> bool {
        self.completed_live
            .iter()
            .any(|(resume, _)| resume.engine == engine)
            || self.pending.values().any(|pending| {
                matches!(&pending.reply,Reply::Resume(resume) if resume.engine==engine)
                    || pending
                        .closing
                        .as_ref()
                        .is_some_and(|request| request.engine == engine)
                    || pending
                        .resource
                        .as_ref()
                        .is_some_and(|request| request.engine == engine)
                    || pending
                        .replacing
                        .as_ref()
                        .is_some_and(|request| request.engine == engine)
            })
    }

    fn deliver(&mut self, reply: Reply, response: JsonRpcResponse) {
        match reply {
            Reply::Resume(resume) => {
                if response.error.is_some() {
                    let (reply, response) = resume.error_reply(response);
                    self.deliver(reply, response);
                } else {
                    self.completed_live.push((resume, response));
                }
            }
            Reply::Remote(reply) => self.completed_remote.push((reply, response)),
            #[cfg(feature = "gui")]
            Reply::Divider {
                engine,
                sequence,
                origin,
            } => self
                .completed_dividers
                .push((engine, sequence, origin, response)),
            #[cfg(feature = "gui")]
            Reply::Settings {
                settings,
                generation,
                origin,
                done,
            } => {
                let settings = if response.error.is_some() {
                    None
                } else {
                    settings
                };
                self.completed_settings.push(SettingsCompletion {
                    generation,
                    settings,
                    origin,
                    done,
                    response,
                });
            }
            Reply::Ipc(sender) => crate::ipc::server::send_response(&sender, response),
            Reply::Plugin {
                plugin_id,
                call_id,
                binding,
            } => self
                .completed_plugins
                .push((plugin_id, call_id, binding, response)),
            Reply::Intent {
                engine,
                origin,
                view,
                completion,
            } => {
                #[cfg(feature = "gui")]
                if response.error.is_some()
                    && let Some(request) =
                        view.as_ref().and_then(|view| view.remote_request.as_ref())
                {
                    request.finish();
                }
                // gui 가 꺼지면 IntentResult 가 view 연속을 보관하지 않는다.
                #[cfg(not(feature = "gui"))]
                let _ = view;
                self.completed_intents.push(IntentResult {
                    #[cfg(feature = "gui")]
                    view,
                    engine,
                    origin,
                    response,
                    completion,
                })
            }
        }
    }
}

/// A host-converted method inside a plugin namespace. The GUI host admits it directly and never
/// sends it to the namespace owner: the conversion waits for that owner's worker to acknowledge
/// the old surface's destruction, and a worker relaying the request back through `host.call`
/// cannot answer until its call times out.
pub(crate) fn host_converts(method: &str) -> bool {
    cfg!(feature = "gui") && method == "image.open"
}

pub(crate) fn handles(method: &str) -> bool {
    tasty_ipc::method_meta::has_structure_journal_contract(method)
}

fn handles_request(request: &JsonRpcRequest) -> bool {
    handles(&request.method)
        || (request.method == "terminal.respawn"
            && request.params.get("cwd").is_some_and(|cwd| cwd.is_string()))
}

impl JournalApplication {
    /// Authentication and current permissions precede this call. No implicit target has been read.
    pub(crate) fn admit_ipc(
        &mut self,
        command: &crate::ipc::server::IpcCommand,
        caller: &crate::ipc::caller::CallerContext,
    ) -> bool {
        if !handles_request(&command.request) {
            return false;
        }
        self.admit_request(
            &command.request,
            Reply::Ipc(command.response_tx.clone()),
            crate::ipc::handler::idempotency::caller_scope(caller),
            "ipc",
        );
        true
    }

    fn admit_request(
        &mut self,
        request: &JsonRpcRequest,
        reply: Reply,
        scope: String,
        origin: &str,
    ) {
        let admission = Admission {
            key: request
                .idempotency_key
                .as_ref()
                .map(|key| tasty_event_store::CommandKey {
                    caller_scope: scope.clone(),
                    idempotency_key: key.clone(),
                }),
            original_digest: request_digest(request),
            actor: scope,
            origin: origin.into(),
            causation_id: None,
        };
        let work = Work::Admit(admission);
        let bytes = serde_json::to_vec(request)
            .expect("JSON request serializes")
            .len()
            + crate::runtime::journal_product::request_size(&work)
            + reply_weight(&reply);
        let held: usize = self
            .commands
            .pending
            .values()
            .map(|pending| pending.bytes)
            .sum::<usize>()
            .saturating_add(
                self.commands
                    .completed_live
                    .iter()
                    .map(|(resume, _)| resume.weight())
                    .sum::<usize>(),
            );
        if self.commands.pending.len() + self.commands.completed_live.len() >= MAX_PENDING
            || held.saturating_add(bytes) > tasty_ipc::admission::QUEUED_BYTES_LIMIT
        {
            self.commands.deliver(
                reply,
                JsonRpcResponse::error(
                    request.id.clone().unwrap_or_default(),
                    crate::ipc::protocol::ERR_COMMAND_QUEUE_FULL,
                    "structure admission queue is full",
                ),
            );
            (self.wake)();
            return;
        }
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        self.commands.pending.insert(
            ticket,
            Pending {
                request: request.clone(),
                engine_scope: reply.engine_scope(),
                ordered_metadata: false,
                reply,
                queued: Some(work),
                in_flight: false,
                needs_resolution: false,
                child_spawn: None,
                category: None,
                resource: None,
                #[cfg(feature = "gui")]
                forward: None,
                #[cfg(feature = "gui")]
                forward_reserved: 0,
                one_shot_reserved: 0,
                closing: None,
                replacing: None,
                close_cause: Default::default(),
                waiting_command: None,
                closing_operations: Vec::new(),
                activation_wait: None,
                created: None,
                bytes,
                replay: false,
                admitted: false,
                retry_counted: false,
                fixed: None,
                display: None,
            },
        );
        (self.wake)();
    }

    pub(super) fn submit_commands(&mut self) -> Result<(), String> {
        let allowed: std::collections::HashSet<_> = self
            .commands
            .pending
            .iter()
            .filter(|(ticket, pending)| self.commands.submittable(**ticket, pending))
            .map(|(ticket, _)| *ticket)
            .collect();
        let mut oversized = Vec::new();
        for (ticket, pending) in self.commands.pending.iter_mut().take(MAX_PENDING) {
            if pending.in_flight {
                continue;
            }
            let Some(work) = pending.queued.take() else {
                continue;
            };
            if !matches!(work, Work::Admit(_)) && !allowed.contains(ticket) {
                pending.queued = Some(work);
                continue;
            }
            let kind = work.kind();
            // The worker returns the original work on pressure; no resource or input is cloned.
            match self.worker.submit_owned(Request {
                ticket: *ticket,
                work,
            }) {
                Ok(()) => {
                    pending.in_flight = true;
                    self.command_submissions.record(*ticket, kind);
                }
                Err((crate::runtime::journal_product::SubmitError::Busy, request)) => {
                    pending.queued = Some(request.work);
                    break;
                }
                Err((crate::runtime::journal_product::SubmitError::TooLarge, _)) => {
                    if pending.admitted {
                        pending.queued = Some(Work::CancelAdmission);
                    } else {
                        oversized.push(*ticket);
                    }
                }
                Err((error, _)) => return Err(format!("structure request submission: {error:?}")),
            }
        }
        for ticket in oversized {
            let pending = self
                .commands
                .pending
                .remove(&ticket)
                .expect("oversized request");
            self.commands.deliver(
                pending.reply,
                oversized_response(pending.request.id.unwrap_or_default()),
            );
        }
        Ok(())
    }

    fn refresh_command_weight(&mut self, ticket: u64) {
        let Some(pending) = self.commands.pending.get_mut(&ticket) else {
            return;
        };
        if let Some(Work::Resolve { changes, .. }) = &pending.queued
            && changes
                .first()
                .is_some_and(|first| changes.iter().any(|change| change.stream != first.stream))
        {
            pending.engine_scope = None;
        }
        pending.bytes = pending_weight(pending);
        let total: usize = self
            .commands
            .pending
            .values()
            .map(|pending| pending.bytes)
            .sum::<usize>()
            .saturating_add(
                self.commands
                    .completed_live
                    .iter()
                    .map(|(resume, _)| resume.weight())
                    .sum::<usize>(),
            );
        if total > tasty_ipc::admission::QUEUED_BYTES_LIMIT {
            let response = JsonRpcResponse::error(
                serde_json::Value::Null,
                crate::ipc::protocol::ERR_COMMAND_QUEUE_FULL,
                "resolved structure request exceeds the pending byte budget",
            );
            if !self.commands.pending[&ticket].admitted {
                let pending = self
                    .commands
                    .pending
                    .remove(&ticket)
                    .expect("sized command");
                let mut response = response;
                response.id = pending.request.id.unwrap_or_default();
                self.commands.deliver(pending.reply, response);
                (self.wake)();
                return;
            }
            let pending = self
                .commands
                .pending
                .get_mut(&ticket)
                .expect("sized command");
            pending.category = None;
            pending.resource = None;
            pending.fixed = None;
            pending.display = None;
            pending.request.params = serde_json::Value::Null;
            #[cfg(feature = "gui")]
            if let Reply::Settings { settings, .. } = &mut pending.reply {
                *settings = None;
            }
            pending.queued = Some(Work::Resolve {
                changes: Vec::new(),
                response: Some(ResponsePlan::Fixed(response)),
            });
            pending.bytes = pending_weight(pending);
        }
    }

    #[cfg(not(feature = "gui"))]
    pub(crate) fn creation_requests_needing_kind(&self) -> Vec<JsonRpcRequest> {
        self.commands
            .pending
            .values()
            .take(1)
            .filter(|pending| {
                pending.needs_resolution
                    && matches!(
                        pending.request.method.as_str(),
                        "workspace.create" | "tab.create" | "split"
                    )
            })
            .map(|pending| pending.request.clone())
            .collect()
    }

    pub(crate) fn requests_needing_resolution(&self) -> Vec<(u64, JsonRpcRequest)> {
        self.commands
            .pending
            .iter()
            .filter(|(_, pending)| pending.needs_resolution)
            .map(|(ticket, pending)| (*ticket, pending.request.clone()))
            .collect()
    }

    pub(crate) fn reject_resolved_request(&mut self, ticket: u64, mut response: JsonRpcResponse) {
        response.id = serde_json::Value::Null;
        if let Some(pending) = self.commands.pending.get_mut(&ticket) {
            pending.needs_resolution = false;
            pending.request.params = serde_json::Value::Null;
            pending.queued = Some(Work::Resolve {
                changes: Vec::new(),
                response: Some(ResponsePlan::Fixed(response)),
            });
            (self.wake)();
        }
        self.refresh_command_weight(ticket);
    }

    pub(crate) fn resolve_ipc_for_engine(&mut self, ticket: u64, session: &EngineSession) {
        if !self.bind_ipc_command_engine(ticket, session.id, &session.core_state) {
            return;
        }
        if self.commands.pending.get(&ticket).is_some_and(|pending| {
            matches!(
                pending.request.method.as_str(),
                "surface.wake" | "intent.wake"
            )
        }) {
            self.resolve_wake(ticket, session);
            return;
        }
        if let Some(pending) = self.commands.pending.get(&ticket)
            && pending.request.method == "tab.move"
            && !matches!(pending.close_cause, close::Cause::RemoteHolder { .. })
        {
            let workspace = pending.request.params["pane_id"]
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
                .and_then(|pane| session.core_state.find_workspace_index_for_pane(pane))
                .and_then(|index| session.core_state.workspace_at(index));
            if let Some(workspace) = workspace
                && session
                    .live
                    .occupancy
                    .workspace_holder(workspace.id)
                    .is_some()
            {
                self.reject_resolved_request(
                    ticket,
                    crate::ipc::handler::hard_occupied_denial(
                        workspace.id,
                        &serde_json::Value::Null,
                    ),
                );
                return;
            }
        }
        if self.commands.pending.get(&ticket).is_some_and(|pending| {
            matches!(
                pending.request.method.as_str(),
                "intent.replace" | "intent.move-surface"
            )
        }) {
            self.resolve_replacement(ticket, session);
            return;
        }
        if self
            .commands
            .pending
            .get(&ticket)
            .is_some_and(|pending| pending.request.method == "intent.restore-closed")
        {
            self.resolve_undo(ticket, session);
            return;
        }
        if self
            .commands
            .pending
            .get(&ticket)
            .is_some_and(|pending| close::is_close_method(&pending.request.method))
        {
            self.resolve_close(ticket, session);
            return;
        }
        if self
            .commands
            .pending
            .get(&ticket)
            .is_some_and(|pending| pending.request.method == "intent.create")
        {
            self.resolve_fixed_creation(ticket, session);
            return;
        }
        let Some(pending) = self.commands.pending.get_mut(&ticket) else {
            return;
        };
        let Some(binding) = session.journal_binding.as_ref() else {
            self.reject_resolved_request(
                ticket,
                JsonRpcResponse::internal_error(
                    serde_json::Value::Null,
                    "engine has no journal binding",
                ),
            );
            return;
        };
        if matches!(
            pending.request.method.as_str(),
            "workspace.update"
                | "workspace.move"
                | "intent.remote-row"
                | "intent.workspace-rename"
                | "intent.tab-name"
                | "intent.tab-move"
                | "tab.move"
        ) {
            match if pending.request.method == "tab.move" {
                tab::move_public(&pending.request, session)
            } else if pending.request.method == "intent.tab-name" {
                tab::rename(&pending.request, session)
            } else if pending.request.method == "intent.tab-move" {
                tab::move_tab(&pending.request)
            } else {
                workspace::resolve(&pending.request, session, &binding.stream)
            } {
                Ok(resolved) => {
                    pending.display = resolved.display;
                    pending.queued = Some(Work::Resolve {
                        changes: resolved
                            .changes
                            .into_iter()
                            .map(|command| StreamCommand {
                                stream: binding.stream.clone(),
                                command,
                            })
                            .collect(),
                        response: Some(resolved.response),
                    });
                }
                Err(response) => self.reject_resolved_request(ticket, response),
            }
        } else {
            match category::resolve(&pending.request, &session.core_state) {
                Ok(category::Resolved::Create(name)) => {
                    pending.category = Some(CategoryReservation {
                        stream: binding.stream.clone(),
                        name,
                    });
                    pending.queued = Some(Work::Reserve(vec![(tasty_core::IdKind::Category, 1)]));
                }
                Ok(category::Resolved::Apply(command, response)) => {
                    pending.queued = Some(Work::Resolve {
                        changes: vec![StreamCommand {
                            stream: binding.stream.clone(),
                            command,
                        }],
                        response: Some(ResponsePlan::Fixed(response)),
                    });
                }
                Err(response) => self.reject_resolved_request(ticket, response),
            }
        }
        if let Some(pending) = self.commands.pending.get_mut(&ticket) {
            // After the miss fixes its input, the original digest belongs to the worker. Keeping
            // another params clone here would duplicate the resolved command and frozen reply.
            pending.request.params = serde_json::Value::Null;
        }
        self.refresh_command_weight(ticket);
        (self.wake)();
    }

    pub(super) fn fail_pending_commands(&mut self, reason: &str) {
        for (_, pending) in std::mem::take(&mut self.commands.pending) {
            self.commands.deliver(
                pending.reply,
                JsonRpcResponse::internal_error(pending.request.id.unwrap_or_default(), reason),
            );
        }
        #[cfg(feature = "gui")]
        self.fail_unsent_forwards(reason);
    }
}

impl JournalApplication {
    pub(crate) fn take_changed_engines(&mut self) -> Vec<EngineId> {
        self.changed_engines.drain().collect()
    }
}

fn retry_outcome(
    result: &Result<ResultValue, JournalError>,
) -> Option<crate::ipc::handler::idempotency::RetryOutcome> {
    use crate::ipc::handler::idempotency::RetryOutcome as O;
    match result {
        Ok(ResultValue::NeedsResolution) => Some(O::Executed),
        Ok(ResultValue::JoinedAdmission { .. }) => Some(O::InFlight),
        Ok(ResultValue::Stored(_) | ResultValue::RecoveryRequired { replay: true, .. }) => {
            Some(O::Replayed)
        }
        Err(JournalError::KeyConflict) => Some(O::Conflicted),
        _ => None,
    }
}

fn oversized_response(id: serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::error(
        id,
        crate::ipc::protocol::ERR_REQUEST_LINE_TOO_LONG,
        "structural request exceeds the accepted payload or queue byte limit",
    )
}

impl JournalApplication {
    pub(crate) fn admit_plugin(
        &mut self,
        request: &JsonRpcRequest,
        caller: &crate::ipc::caller::CallerContext,
        call: &tasty_host_plugin::manager::PendingPluginCall,
        manager: Option<&crate::plugin::PluginManager>,
    ) -> bool {
        // This runs only after namespace routing; a host conversion is never sent to its owner.
        if !handles_request(request) && !host_converts(&request.method) {
            return false;
        }
        if !manager.is_some_and(|manager| manager.plugin_call_is_current(call)) {
            tracing::warn!(plugin=%call.plugin_id,"discarding structural call from retired plugin");
            return true;
        }
        self.admit_request(
            request,
            Reply::Plugin {
                plugin_id: call.plugin_id.clone(),
                call_id: call.call_id,
                binding: call.binding.clone(),
            },
            crate::ipc::handler::idempotency::caller_scope(caller),
            "plugin",
        );
        true
    }

    #[cfg(feature = "gui")]
    pub(crate) fn admit_host_fallback_ipc(
        &mut self,
        command: &crate::ipc::server::IpcCommand,
        caller: &crate::ipc::caller::CallerContext,
    ) -> bool {
        if !host_converts(&command.request.method) {
            return false;
        }
        self.admit_request(
            &command.request,
            Reply::Ipc(command.response_tx.clone()),
            crate::ipc::handler::idempotency::caller_scope(caller),
            "ipc",
        );
        true
    }

    pub(crate) fn deliver_plugin_replies(
        &mut self,
        manager: Option<&mut crate::plugin::PluginManager>,
    ) {
        let replies = std::mem::take(&mut self.commands.completed_plugins);
        let Some(manager) = manager else {
            return;
        };
        for (plugin, call, binding, response) in replies {
            if !manager.send_bound_ipc_result(&plugin, &binding, call, response) {
                tracing::warn!("discarding structural reply for retired plugin {plugin}");
            }
        }
    }
}

fn pending_weight(pending: &Pending) -> usize {
    #[cfg(feature = "gui")]
    let forward_bytes = pending.forward_reserved.max(
        pending
            .forward
            .as_ref()
            .map_or(0, super::forward::Draft::weight),
    );
    #[cfg(not(feature = "gui"))]
    let forward_bytes: usize = 0;
    forward_bytes
        .saturating_add(pending.child_spawn.as_ref().map_or(0, child::Spawn::weight))
        .saturating_add(pending.one_shot_reserved)
        .saturating_add(
            serde_json::to_vec(&pending.request)
                .expect("request JSON serializes")
                .len()
                .saturating_add(
                    pending
                        .queued
                        .as_ref()
                        .map_or(0, crate::runtime::journal_product::request_size),
                )
                .saturating_add(pending.fixed.as_ref().map_or(0, |commands| {
                    serde_json::to_vec(commands)
                        .expect("fixed commands serialize")
                        .len()
                }))
                .saturating_add(
                    pending
                        .display
                        .as_ref()
                        .map_or(0, display::DisplayContinuation::weight),
                )
                .saturating_add(pending.resource.as_ref().map_or(0, create::Request::weight))
                .saturating_add(pending.closing.as_ref().map_or(0, close::Request::weight))
                .saturating_add(
                    pending
                        .replacing
                        .as_ref()
                        .map_or(0, replacement::Request::weight),
                )
                .saturating_add(pending.waiting_command.as_ref().map_or(0, String::len))
                .saturating_add(
                    pending
                        .created
                        .as_ref()
                        .map_or(0, create::Completed::weight),
                )
                .saturating_add(reply_weight(&pending.reply))
                .saturating_add(
                    pending
                        .category
                        .as_ref()
                        .map_or(0, |category| category.name.len() + category.stream.len()),
                ),
        )
}

fn reply_weight(_reply: &Reply) -> usize {
    if let Reply::Resume(resume) = _reply {
        return resume.weight();
    }
    if let Reply::Remote(_) = _reply {
        return std::mem::size_of::<inbound::RemoteReply>();
    }
    #[cfg(feature = "gui")]
    if let Reply::Settings { settings, .. } = _reply {
        return serde_json::to_vec(settings)
            .expect("settings serialize")
            .len();
    }
    0
}

#[cfg(test)]
mod close_history_tests;
#[cfg(test)]
mod close_pause_tests;
#[cfg(test)]
mod close_remote_tests;
#[cfg(test)]
mod remote_cwd_tests;
// The selection follows a completed intent only in the GUI; headless keeps no View activation.
#[cfg(all(test, feature = "gui"))]
mod remote_selection_tests;
#[cfg(test)]
mod tests;

// Stream JSON through a hash so raw terminal input is never retained as a command digest.
fn request_digest(request: &JsonRpcRequest) -> Vec<u8> {
    // Existing keys retain their original comparison representation. Only the newly journaled
    // raw-input composite uses a hash; upgrading must not turn old Stored replies into conflicts.
    if !matches!(
        request.method.as_str(),
        "terminal.spawn" | "terminal.respawn"
    ) {
        return serde_json::to_vec(&(&request.method, &request.params))
            .expect("JSON request serializes");
    }
    use sha2::Digest;
    struct HashWriter(sha2::Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(sha2::Sha256::new());
    serde_json::to_writer(&mut writer, &(&request.method, &request.params))
        .expect("JSON request serializes");
    writer.0.finalize().to_vec()
}
