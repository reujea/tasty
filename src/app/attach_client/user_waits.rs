//! User request admission and observable waits. View-only; never journals a preview.
use super::forward::find_mirror_session_and_remote_id;
use crate::app::App;
use crate::ipc::stream::StructuralOp;
use crate::runtime::engine_session::EngineId;
use crate::state::attach_pending::waits::{Wait, WaitKind};
use crate::state::attach_pending::{Query, QueryConsumer};
use std::collections::HashMap;
use tasty_remote::connection::ConnectionEpoch;

impl App {
    pub(super) fn begin_query_wait(
        &mut self,
        surface: u32,
        request: u64,
        admitted: std::time::Instant,
        consumer: QueryConsumer,
    ) {
        let Some((session, _)) =
            find_mirror_session_and_remote_id(&mut self.remote.sessions, surface, "query wait")
        else {
            return;
        };
        let workspace = session.state.local_workspace;
        let epoch = session.transport.frame_tx.epoch();
        for (_, main, engine) in self.engines_mut().window_pairs() {
            let Some(target) =
                crate::runtime::surface_binding::SurfaceBinding::capture(&engine.read(), surface)
            else {
                continue;
            };
            let queries = &mut main.state.attach_pending.queries;
            queries.retain(|query| query.consumer != consumer);
            queries.push(Query {
                consumer,
                request,
                workspace,
                admitted,
                epoch: epoch.clone(),
                target,
            });
            break;
        }
    }
    pub(crate) fn sync_attach_waits(&mut self) {
        use crate::view::ui::View as _;
        let (waits, epochs): (HashMap<_, _>, HashMap<_, _>) = self
            .remote
            .sessions
            .iter()
            .filter(|session| session.transport.frame_tx.epoch().is_active())
            .map(|session| {
                let mut times: Vec<_> = session
                    .state
                    .resize_sync
                    .pending_times()
                    .map(|time| Wait::new(WaitKind::Resize, time))
                    .collect();
                times.extend(
                    session
                        .transport
                        .frame_tx
                        .oldest_unprocessed_input()
                        .map(|time| Wait::new(WaitKind::Input, time)),
                );
                let workspace = session.state.local_workspace;
                (
                    (workspace, times),
                    (workspace, session.transport.frame_tx.began_at()),
                )
            })
            .unzip();
        // 대기는 요청한 plugin의 popup에만 걸었으므로 열린 popup인지만 본다.
        let git_consumers: std::collections::HashSet<_> = self
            .plugin_manager
            .as_ref()
            .into_iter()
            .flat_map(|manager| manager.popup_instances())
            .map(|(id, _)| id)
            .collect();
        for (_, main, engine) in self.engines_mut().window_pairs() {
            let mut waits = waits.clone();
            main.state.attach_pending.queries.retain(|query| {
                query.epoch.is_active()
                    && query.target.same_surface_in(&engine.read())
                    && match query.consumer {
                        QueryConsumer::Git(id) => git_consumers.contains(&id),
                        QueryConsumer::Markdown(_) => true,
                    }
            });
            for query in &main.state.attach_pending.queries {
                waits.entry(query.workspace).or_default().push(Wait::new(
                    match query.consumer {
                        QueryConsumer::Git(_) => WaitKind::Git,
                        QueryConsumer::Markdown(_) => WaitKind::Markdown,
                    },
                    query.admitted,
                ));
            }
            for (surface, since) in main.state.explorer_views.pending_remote_times() {
                if let Some((index, _)) = engine.find_workspace_index_for_surface(surface)
                    && let Some(workspace) = engine.workspace_at(index)
                    && waits.contains_key(&workspace.id)
                {
                    waits
                        .entry(workspace.id)
                        .or_default()
                        .push(Wait::new(WaitKind::Folder, since));
                }
            }
            if let Some(picker) = &main.state.dialogs.file_picker
                && let Some(workspace) = picker.mirror_ws_id
                && waits.contains_key(&workspace)
                && let crate::state::FpLoadState::Loading { sent_at, .. } = picker.load
            {
                waits
                    .entry(workspace)
                    .or_default()
                    .push(Wait::new(WaitKind::Folder, sent_at));
            }
            for (workspace, times) in &mut waits {
                if let Some(began) = epochs.get(workspace) {
                    times.retain(|wait| wait.admitted >= *began);
                }
                times.sort_by_key(|wait| wait.admitted);
            }
            main.state.attach_pending.retain(engine.core);
            if main.state.attach_waits != waits {
                main.state.attach_waits.clone_from(&waits);
                main.mark_dirty();
            }
        }
    }
    /// mirror로 보낼 구조 요청을 사용자 대기에 올린 뒤 journal에 들인다.
    pub(crate) fn admit_forwarded_request(
        &mut self,
        engine: EngineId,
        op: StructuralOp,
        origin: &crate::intent::IntentOrigin,
        admitted: std::time::Instant,
        mut continuation: Option<crate::app::journal::commands::IntentViewContinuation>,
        candidates: Vec<u32>,
    ) {
        let receipt = self.begin_attach_request(engine, &op, origin, admitted);
        if let Some(view) = continuation.as_mut() {
            view.remote_request = receipt;
        }
        self.journal
            .admit_remote_intent(engine, op, origin, continuation, candidates);
    }
    pub(crate) fn begin_attach_request(
        &mut self,
        engine: EngineId,
        op: &StructuralOp,
        origin: &crate::intent::IntentOrigin,
        admitted: std::time::Instant,
    ) -> Option<std::sync::Arc<tasty_remote::pending_request::PendingRequest>> {
        if !origin.is_user() {
            return None;
        }
        let (session, _) = find_mirror_session_and_remote_id(
            &mut self.remote.sessions,
            op.anchor_surface_id(),
            "user admission",
        )?;
        let epoch = session.transport.frame_tx.epoch();
        let context = self.engines_mut().resolve(engine)?;
        let view = context.view?;
        let receipt = context.state.attach_pending.begin(
            context.engine.core,
            &context.state.navigation,
            op,
            epoch,
            view.state.identity(),
            admitted,
        );
        view.mark_dirty();
        receipt
    }
    /// 사용자가 mirror surface에 보낸 입력은 원격이 받았다는 확인을 기다린다.
    pub(crate) fn user_input_receipt(
        &self,
        origin: &crate::intent::IntentOrigin,
        intent: &crate::app::command::DomainIntent,
        admitted: std::time::Instant,
    ) -> Option<UserInputReceipt> {
        let crate::app::command::DomainIntent::SendToSurface { surface_id, .. } = intent else {
            return None;
        };
        if !origin.is_user() {
            return None;
        }
        self.remote
            .sessions
            .iter()
            .find(|session| {
                session
                    .state
                    .remote_to_local
                    .values()
                    .any(|local| local == surface_id)
            })
            .map(|session| UserInputReceipt {
                sender: session.transport.frame_tx.clone(),
                admitted,
            })
    }
}

pub(crate) struct UserInputReceipt {
    sender: tasty_remote::client_session::SharedFrameSender,
    admitted: std::time::Instant,
}

impl UserInputReceipt {
    /// 입력이 실제로 surface에 전달됐을 때만 접수 시각을 연결에 기록한다.
    pub(crate) fn settle(self, events: &[crate::app::command::CoreEvent]) {
        if events.iter().any(|event| {
            matches!(
                event,
                crate::app::command::CoreEvent::SurfaceSent { sent: true, .. }
            )
        }) {
            self.sender.user_input_dispatched(self.admitted);
        }
    }
}

/// 원격 구조 요청의 실패를 그 run과 사용자 대기에 알린다.
pub(super) fn structural_failed(
    pending: &crate::state::attach_pending::AttachPending,
    epoch: &ConnectionEpoch,
    op: u64,
    reason: &Option<String>,
) {
    super::op_runs::failed(epoch, op, reason.as_deref());
    pending.result(epoch, op, None, false);
}

/// 결과 정산 전 안내 대상을 찾는다. 정산된 요청은 중복 회신이나 실패 뒤의 늦은 성공으로
/// 다시 안내하지 않으며, 사라진 View의 요청도 다른 View로 넘기지 않는다.
pub(super) fn unanswered_request_view(
    pending: &crate::state::attach_pending::AttachPending,
    epoch: &ConnectionEpoch,
    op: u64,
) -> Option<std::sync::Weak<()>> {
    pending
        .entries
        .iter()
        .find(|entry| {
            entry.epoch.same(epoch)
                && entry.receipt.op() == Some(op)
                && !entry.receipt.has_result()
                && !entry.receipt.finished()
                && entry.view.strong_count() > 0
        })
        .map(|entry| entry.view.clone())
}

/// 원격 구조 요청의 성공을 그 run과 사용자 대기에 알린다. 결과보다 먼저 적용된 생성을 사용자가
/// 선택해 두었으면 포커스할 local surface를 돌려준다.
pub(super) fn structural_succeeded(
    state: &mut crate::state::MainViewState,
    epoch: &ConnectionEpoch,
    (op, created): (u64, Option<u32>),
    view: Option<&std::sync::Weak<()>>,
    mapping: &HashMap<u32, u32>,
) -> Option<u32> {
    super::op_runs::succeeded(epoch, op, created, mapping);
    state.attach_pending.result(epoch, op, created, true);
    state
        .attach_pending
        .applied_before_result(epoch, op, &state.navigation, view, mapping)
}

/// 적용된 snapshot의 ID 매핑을 run과 사용자 대기에 알리고 포커스할 local surface를 돌려준다.
pub(super) fn structural_applied(
    state: &mut crate::state::MainViewState,
    core: &crate::core::CoreState,
    epoch: &ConnectionEpoch,
    view: Option<&std::sync::Weak<()>>,
    (mapping, new_remote): (&HashMap<u32, u32>, &[u32]),
) -> Option<u32> {
    super::op_runs::mapped(epoch, mapping);
    state
        .attach_pending
        .applied(core, &state.navigation, epoch, view, mapping, new_remote)
}

/// delta의 surface 중 아직 local에 매핑되지 않은 remote ID. 적용 전에 구해야 새로 생긴 것을 안다.
pub(super) fn unmapped_remote_ids(
    surfaces: &[serde_json::Value],
    mapping: &HashMap<u32, u32>,
) -> Vec<u32> {
    surfaces
        .iter()
        .filter_map(|surface| {
            surface
                .get("remote_id")?
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
        })
        .filter(|id| !mapping.contains_key(id))
        .collect()
}
