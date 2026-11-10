//! Bounded worker completion pump and atomic installation before publication acknowledgement.
use super::*;

impl JournalApplication {
    pub(super) fn poll_initial(
        &mut self,
        sessions: &mut [&mut EngineSession],
        mut plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        #[cfg(feature = "gui")]
        self.poll_boot_release(plugins.as_deref_mut());
        const MAX_COMPLETIONS: usize = 16;
        for _ in 0..MAX_COMPLETIONS {
            if self.started {
                self.drive_pending_work(sessions, plugins.as_deref_mut())?;
            }
            let completion = match self.worker.try_recv() {
                Ok(value) => value,
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("structure journal worker stopped during bootstrap".into());
                }
            };
            match completion {
                Completion::Ready {
                    cut,
                    bootstrap,
                    runtime_epoch,
                    damage,
                    ..
                } => {
                    self.damaged_slots
                        .extend(damage.damaged.keys().filter_map(|stream| {
                            stream
                                .strip_prefix("structure:slot-")
                                .and_then(|slot| slot.parse::<u32>().ok())
                        }));
                    self.report_load_damage(&damage);
                    self.accept_bootstrap(cut, bootstrap, runtime_epoch, sessions)?;
                }
                Completion::StartupFailed(failure) => {
                    let detail = failure.to_string();
                    #[cfg(feature = "gui")]
                    {
                        self.startup_failure = Some(failure);
                    }
                    return Err(detail);
                }
                Completion::Halted(error) => return Err(error),
                Completion::Publish {
                    batch,
                    before,
                    engine_binding,
                    #[cfg(test)]
                        commit_statistics: _,
                } => {
                    self.publish_batch(
                        batch,
                        before,
                        engine_binding,
                        sessions,
                        plugins.as_deref_mut(),
                    )?;
                }
                Completion::Finished { ticket, result } => {
                    self.accept_finished(ticket, result, sessions, plugins.as_deref_mut())?;
                }
            }
        }
        // A continuously producing worker must not monopolize the event loop. Request another
        // turn even if its earlier wake was coalesced while this bounded batch was consumed.
        (self.wake)();
        Ok(())
    }

    fn drive_pending_work(
        &mut self,
        sessions: &mut [&mut EngineSession],
        mut plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        if self.pauses_observation()
            && let Some(plugins) = plugins.as_deref_mut()
        {
            plugins.poll_publication_retirements()?;
        }
        self.submit_openings()?;
        self.submit_commands()?;
        #[cfg(feature = "gui")]
        self.submit_forwards()?;
        self.poll_resource_cleanup(sessions, plugins)?;
        self.submit_captures(sessions)?;
        self.submit_preset_captures()?;
        self.refill_execution_ids(sessions)?;
        self.submit_restore_reads()?;
        self.restorations
            .submit_preservations(&self.worker, &mut self.next_ticket)?;
        #[cfg(feature = "gui")]
        self.submit_retirements()?;
        #[cfg(feature = "gui")]
        self.submit_view_writes()?;
        for ((engine, _), creation) in &mut self.creations {
            let session = sessions
                .iter_mut()
                .find(|session| session.id == *engine)
                .ok_or("materializing engine disappeared")?;
            let mirror_count = session.core_state.mirror_workspaces().len();
            let mut view = self
                .completion_views
                .get(engine)
                .cloned()
                .unwrap_or_default();
            view.mirror_count = mirror_count;
            creation.poll_cleanup(&self.worker, view, session)?;
        }
        self.finish_settled_creations();
        Ok(())
    }

    /// A settled creation leaves; its held command reply is read again. An unconfirmed retirement
    /// of the replaced owner is answered as recovery-required for requests already waiting on it.
    fn finish_settled_creations(&mut self) {
        let settled: Vec<_> = self
            .creations
            .iter()
            .filter(|(_, creation)| creation.is_settled())
            .map(|(key, _)| *key)
            .collect();
        if settled.is_empty() {
            return;
        }
        for key in settled {
            let creation = self.creations.remove(&key).expect("settled creation");
            self.note_first_workspace_failure(key.0, &creation);
            if let Some((command, reason)) = creation.unconfirmed_cleanup() {
                self.commands.note_unconfirmed_cleanup(command, reason);
            }
        }
        self.refresh_in_progress_commands();
        // The queued reads are submitted on the next turn. No reply or deadline is left to
        // request it, so ask for it here.
        (self.wake)();
    }

    #[cfg(feature = "gui")]
    fn note_first_workspace_failure(&mut self, engine: EngineId, creation: &creation::Creation) {
        if let Some(reason) = creation.first_workspace_failure() {
            tracing::error!(engine = ?engine, "the first workspace was not created: {reason}");
            self.first_workspace_failures.insert(engine, reason);
        }
    }

    /// Without a View no window waits for an engine's first workspace.
    #[cfg(not(feature = "gui"))]
    fn note_first_workspace_failure(&mut self, _engine: EngineId, _creation: &creation::Creation) {}

    fn accept_bootstrap(
        &mut self,
        cut: Option<u64>,
        mut bootstrap: tasty_core::StructureModels,
        runtime_epoch: u64,
        sessions: &mut [&mut EngineSession],
    ) -> Result<(), String> {
        self.runtime_epoch = Some(runtime_epoch);
        self.known_slots
            .extend(bootstrap.streams.iter().filter_map(|(stream, model)| {
                stream
                    .strip_prefix("structure:slot-")
                    .and_then(|slot| slot.parse::<u32>().ok())
                    .map(|slot| (slot, model.engine_retired))
            }));
        let occupied: Vec<_> = sessions
            .iter()
            .map(|session| (session.id, session.persistence.slot))
            .collect();
        // 이번 bootstrap에서 새로 배정한 슬롯. 같은 bootstrap의 다른 opening이 같은 번호를 받지 않게 한다.
        let mut assigned: Vec<u32> = Vec::new();
        for session in sessions.iter_mut() {
            let Some(opening) = self.opening.get_mut(&session.id) else {
                continue;
            };
            if opening.select_available_slot
                && let EngineSelection::Slot { slot, .. } = &mut opening.selection
                && let Some(available) = self
                    .known_slots
                    .iter()
                    .filter(|(_, retired)| !**retired)
                    .map(|(slot, _)| slot)
                    .find(|candidate| {
                        !occupied
                            .iter()
                            .any(|(id, used)| *id != session.id && *used == Some(**candidate))
                    })
            {
                *slot = *available;
                session.persistence.slot = Some(*available);
            }
            if let EngineSelection::Slot { slot, .. } = &mut opening.selection
                && self.damaged_slots.contains(slot)
            {
                // 손상된 슬롯 대신 어떤 기록에도 쓰이지 않은 번호로 새로 시작한다.
                let fresh = self
                    .known_slots
                    .keys()
                    .chain(&self.damaged_slots)
                    .copied()
                    .chain(occupied.iter().filter_map(|(_, used)| *used))
                    .chain(assigned.iter().copied())
                    .max()
                    .unwrap_or(0)
                    + 1;
                assigned.push(fresh);
                tracing::warn!(damaged = *slot, fresh, "starting a new layout slot instead");
                *slot = fresh;
                session.persistence.slot = Some(fresh);
            }
            let model = match opening.selection {
                EngineSelection::Slot { slot, resume: true } => bootstrap
                    .streams
                    .remove(&format!("structure:slot-{slot}"))
                    .unwrap_or_default(),
                _ => tasty_core::JournalModel::default(),
            };
            projection::bootstrap::initialize(&mut session.core_state, &model)?;
            crate::runtime::surface_restorer::initialize_instances(session, &model);
            opening.projected = matches!(
                opening.selection,
                EngineSelection::Slot { resume: true, .. }
            );
        }
        drop(bootstrap);
        #[cfg(feature = "gui")]
        self.forget_child_relations_of_unknown_slots(&occupied, sessions);
        self.worker
            .acknowledge(cut.unwrap_or(0), Ok(()))
            .map_err(|error| format!("bootstrap ACK: {error:?}"))?;
        self.started = true;

        Ok(())
    }

    /// 복원이 정한 슬롯으로 자식 관계 레지스트리를 다시 묶고, 다시 열 수 없는 슬롯의 관계를 지운다.
    /// 슬롯 표시가 없는 이전 형식의 항목은 첫 복원 engine 의 슬롯에 속한다.
    #[cfg(feature = "gui")]
    fn forget_child_relations_of_unknown_slots(
        &self,
        occupied: &[(crate::runtime::engine_session::EngineId, Option<u32>)],
        sessions: &mut [&mut EngineSession],
    ) {
        let mut keep: std::collections::HashSet<u32> = self
            .known_slots
            .iter()
            .filter(|(_, retired)| !**retired)
            .map(|(slot, _)| *slot)
            .collect();
        keep.extend(occupied.iter().filter_map(|(_, slot)| *slot));
        keep.extend(sessions.iter().filter_map(|s| s.persistence.slot));
        let mut first = true;
        for session in sessions.iter_mut() {
            let resumed = self.opening.get(&session.id).is_some_and(|opening| {
                matches!(
                    opening.selection,
                    EngineSelection::Slot { resume: true, .. }
                )
            });
            if !resumed {
                continue;
            }
            let registry = &mut session.runtime.child_terminals;
            registry.bind_slot(session.persistence.slot);
            if std::mem::take(&mut first) {
                registry.forget_slots_except(&keep);
            }
        }
    }

    fn publish_batch(
        &mut self,
        batch: tasty_core::StreamBatch,
        before: std::collections::BTreeMap<String, tasty_core::JournalModel>,
        engine_binding: Option<crate::runtime::journal_product::EngineBinding>,
        sessions: &mut [&mut EngineSession],
        mut plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        for session in sessions.iter_mut() {
            self.publish_session(
                session,
                &batch,
                &before,
                engine_binding.as_ref(),
                plugins.as_deref_mut(),
            )?;
        }
        self.worker
            .acknowledge(batch.batch_id, Ok(()))
            .map_err(|error| format!("bootstrap publication ACK: {error:?}"))?;
        for ((id, _), creation) in &self.creations {
            if let Some(session) = sessions.iter().find(|session| session.id == *id) {
                creation.acknowledge_publication(session);
            }
        }
        self.creations
            .retain(|_, creation| !creation.release_published());

        Ok(())
    }

    fn capture_batch_obligations(
        &mut self,
        session: &mut EngineSession,
        domain: &tasty_core::DomainBatch,
        stream: &str,
    ) -> Result<(), String> {
        for event in &domain.events {
            if let tasty_core::DomainEvent::OperationPrepared { operation } = &event.event
                && matches!(
                    operation.creation.as_ref().map(|plan| &plan.destination),
                    Some(tasty_core::CreationDestination::Assembly { .. })
                )
            {
                let ticket = self.next_ticket;
                self.next_ticket = self
                    .next_ticket
                    .checked_add(1)
                    .ok_or("journal ticket exhausted")?;
                let binding = session
                    .journal_binding
                    .clone()
                    .ok_or("assembly engine binding missing")?;
                let creation = creation::Creation::committed(
                    ticket,
                    binding,
                    &self.worker,
                    operation.id.clone(),
                )?;
                self.creations.insert((session.id, ticket), creation);
            }
            if let tasty_core::DomainEvent::OperationPrepared { operation } = &event.event
                && operation.retirement.is_some()
            {
                let retirement = crate::runtime::resource_retirement::ResourceRetirement::capture(
                    session, operation,
                )?;
                session
                    .pending_resource_retirements
                    .insert(operation.id.clone(), retirement);
                self.queue_resource_retirement(
                    session.id,
                    stream.to_owned(),
                    operation.id.clone(),
                )?;
            }
        }
        Ok(())
    }

    fn install_published_leaf(
        &mut self,
        session: &mut EngineSession,
        predecessor: &tasty_core::JournalModel,
        domain: &tasty_core::DomainBatch,
        leaf: crate::runtime::effect_runner::PreparedLeaf,
    ) -> Result<(), String> {
        let id = leaf
            .surface
            .surface_id()
            .ok_or("materialized kind has no ID")?;
        let descriptor = session
            .core_state
            .find_surface_by_id(id)
            .ok_or("materialized leaf has no committed descriptor")?;
        if descriptor.kind != leaf.logical_kind {
            return Err("materialized kind differs from committed descriptor".into());
        }
        let deferred = leaf
            .surface
            .as_any()
            .is::<crate::runtime::surface_restorer::JournalPlaceholder>();
        drop(session.runtime.surfaces.insert(id, leaf.surface));
        if deferred {
            let mut after = predecessor.clone();
            tasty_core::evolve(&mut after, domain).map_err(|error| error.to_string())?;
            if let Some(placeholder) = session.runtime.surfaces.get_mut(&id).and_then(|surface| {
                surface
                    .as_any_mut()
                    .downcast_mut::<crate::runtime::surface_restorer::JournalPlaceholder>()
            }) {
                let value = after
                    .surfaces
                    .get(&id)
                    .ok_or("deferred assembly leaf missing")?;
                placeholder.data = value.data;
                placeholder.creation_seed = value.creation_seed;
                placeholder.activation = value.activation;
            }
            if let Some(request) = crate::runtime::surface_restorer::describe(&session.as_ref())
                .into_iter()
                .find(|request| request.surface_id == id)
            {
                self.restorations.stage(session.id, request);
            }
        }

        Ok(())
    }

    fn apply_live_batch(
        &mut self,
        session: &mut EngineSession,
        predecessor: &tasty_core::JournalModel,
        domain: &tasty_core::DomainBatch,
        stream: &str,
        mut plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        let mut prepared = Vec::new();
        let mut installations = Vec::new();
        let session_id = session.id;
        for (key, creation) in self
            .creations
            .iter_mut()
            .filter(|((engine, _), _)| *engine == session_id)
        {
            if let Some(installation) = creation.authorize_installation(session, &domain.events)? {
                installations.push((*key, installation));
            }
            if let Some(leaf) = creation.leaf_for_publication(session, &domain.events)? {
                prepared.push(leaf);
            }
        }
        self.capture_batch_obligations(session, domain, stream)?;
        projection::apply(
            &mut session.core_state,
            predecessor,
            domain,
            &mut Vec::new(),
        )?;
        for leaf in prepared {
            self.install_published_leaf(session, predecessor, domain, leaf)?;
        }
        for (key, installation) in installations {
            let retiring = session
                .runtime
                .surfaces
                .get(&installation.surface_id())
                .map(|surface| {
                    crate::runtime::effect_runner::RetiringKind::capture(surface.as_ref())
                });
            self.creations
                .get_mut(&key)
                .expect("materialization request")
                .install_candidate(session, plugins.as_deref_mut(), retiring, installation)?;
        }

        Ok(())
    }

    fn record_published_session(
        &mut self,
        session: &mut EngineSession,
        predecessor: &tasty_core::JournalModel,
        events: &[tasty_core::RecordedEvent],
        stream: &str,
        batch_id: u64,
    ) -> Result<(), String> {
        for recorded in events {
            match &recorded.event {
                tasty_core::DomainEvent::StructureReplaced { replacement, .. } => {
                    self.replacements.push((session.id, *replacement))
                }
                // 새로 저장된 행은 토큰을 받고, 이름 등을 바꾼 행은 진행 중인 연결을 그대로 둔다.
                tasty_core::DomainEvent::RemoteRowSet { id, .. } => {
                    session
                        .remote
                        .row_tokens
                        .entry(*id)
                        .or_insert_with(|| Arc::new(()));
                }
                tasty_core::DomainEvent::RemoteRowRemoved { id } => {
                    session.remote.row_tokens.remove(id);
                }
                _ => {}
            }
        }
        if let Some(slot) = stream
            .strip_prefix("structure:slot-")
            .and_then(|slot| slot.parse::<u32>().ok())
        {
            let mut retired = predecessor.engine_retired;
            for event in events {
                match event.event {
                    tasty_core::DomainEvent::EngineRetired { .. } => retired = true,
                    tasty_core::DomainEvent::EngineIncarnationStarted { .. } => retired = false,
                    _ => {}
                }
            }
            self.known_slots.insert(slot, retired);
        }
        if events.iter().any(|recorded| {
            !matches!(
                recorded.event,
                tasty_core::DomainEvent::SurfaceDataRecorded { .. }
            )
        }) {
            session.persistence.dirty.mark_dirty();
        }
        self.changed_engines.insert(session.id);
        if let Some(binding) = session.journal_binding.as_mut() {
            binding.published_cut = Some(batch_id);
            if let Some(last) = events.last() {
                binding.revision = Some(last.revision);
            }
        }

        Ok(())
    }

    fn publish_session(
        &mut self,
        session: &mut EngineSession,
        batch: &tasty_core::StreamBatch,
        before: &std::collections::BTreeMap<String, tasty_core::JournalModel>,
        engine_binding: Option<&crate::runtime::journal_product::EngineBinding>,
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        let opening = self.opening.get_mut(&session.id);
        let stream = match opening.as_ref().map(|opening| &opening.selection) {
            Some(EngineSelection::Slot { slot, .. }) => Some(format!("structure:slot-{slot}")),
            Some(EngineSelection::ImportedSlot { source }) => {
                Some(format!("structure:slot-{}", source.destination_slot))
            }
            Some(EngineSelection::FreshHeadless) => engine_binding
                .as_ref()
                .map(|binding| binding.stream.clone()),
            None => session
                .journal_binding
                .as_ref()
                .map(|binding| binding.stream.clone()),
        };
        let Some(stream) = stream else {
            return Ok(());
        };
        if let Some(slot) = stream
            .strip_prefix("structure:slot-")
            .and_then(|slot| slot.parse::<u32>().ok())
        {
            self.known_slots.entry(slot).or_insert(false);
        }
        let Some(events) = batch.streams.get(&stream) else {
            return Ok(());
        };
        let predecessor = before.get(&stream).ok_or("bootstrap predecessor missing")?;
        let domain = tasty_core::DomainBatch {
            batch_id: batch.batch_id,
            events: events.clone(),
        };
        if let Some(opening) = opening
            && (!opening.projected
                || (session.core_state.local_workspaces().is_empty()
                    && predecessor.workspaces.is_empty()))
        {
            let mut after = predecessor.clone();
            tasty_core::evolve(&mut after, &domain).map_err(|error| error.to_string())?;
            projection::bootstrap::initialize(&mut session.core_state, &after)?;
            crate::runtime::surface_restorer::initialize_instances(session, &after);
            opening.projected = true;
        } else {
            self.apply_live_batch(session, predecessor, &domain, &stream, plugins)?;
        }
        session
            .borrow_mut()
            .observe_committed_structure(predecessor, events);
        self.record_published_session(session, predecessor, events, &stream, batch.batch_id)?;
        Ok(())
    }

    fn answer_finished_request(
        &mut self,
        ticket: u64,
        result: &Result<ResultValue, String>,
        sessions: &mut [&mut EngineSession],
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<bool, String> {
        #[cfg(feature = "gui")]
        if self.answer_forward(ticket, result)? {
            return Ok(true);
        }
        if self.answer_execution_ids(ticket, result, sessions)? {
            return Ok(true);
        }
        if self.answer_capture(ticket, result, sessions) {
            return Ok(true);
        }
        if self.answer_preset_capture(ticket, result) {
            return Ok(true);
        }
        if self.answer_resource_cleanup(ticket, result, sessions, plugins)? {
            return Ok(true);
        }
        #[cfg(feature = "gui")]
        if self.finish_retirement(ticket, result)? {
            return Ok(true);
        }
        #[cfg(feature = "gui")]
        if self.view_writes.complete(ticket, result)? {
            return Ok(true);
        }

        Ok(false)
    }

    fn complete_opening(
        &mut self,
        ticket: u64,
        result: Result<ResultValue, String>,
        sessions: &mut [&mut EngineSession],
    ) -> Result<(), String> {
        let id = self
            .opening
            .iter()
            .find_map(|(id, opening)| (opening.ticket == Some(ticket)).then_some(*id))
            .ok_or_else(|| self.unowned_completion(ticket, &result))?;
        let opening = self.opening.remove(&id).expect("matched opening");
        let session = sessions
            .iter_mut()
            .find(|session| session.id == id)
            .ok_or("opening engine disappeared")?;
        let bound = match result? {
            ResultValue::Bound(bound) => bound,
            // 기록을 남기기 전에 멈췄으므로 다른 engine의 journal은 계속 쓴다.
            ResultValue::OpeningRefused(reason) => {
                self.restorations.forget(id);
                self.refused_openings.insert(id, reason);
                return Ok(());
            }
            _ => return Err("bootstrap did not return an engine binding".into()),
        };
        for issue in &bound.issues {
            self.report_binding_issue(id, issue);
        }
        #[cfg(feature = "gui")]
        if bound.restore_legacy_metadata {
            // writer 순서를 따르므로 이 engine의 뒤 요청은 정리된 상태를 본다. 메인 루프는 기다리지 않는다.
            session.runtime.memory_access.submit_detached(
                tasty_memory::writer::MemoryRequest::unit(0, |memory| {
                    Ok(crate::surface_meta::SurfaceMetaStore::purge_out_of_range_surfaces(memory))
                }),
                "legacy surface scope purge",
                Some(Box::new(|output| {
                    let purged = output.into_value::<usize>().unwrap_or(0);
                    if purged > 0 {
                        tracing::error!(
                            purged,
                            "removed legacy surface scopes outside the surface ID range"
                        );
                    }
                })),
            );
        }
        if !opening.projected {
            projection::bootstrap::initialize(&mut session.core_state, &bound.model)?;
            crate::runtime::surface_restorer::initialize_instances(session, &bound.model);
        }
        self.restored_views.insert(
            session.id,
            crate::runtime::restored_presentation::presentation(
                &session.core_state,
                bound.imported_view,
            ),
        );
        for workspace in session.core_state.mirror_workspaces() {
            if workspace.is_saved_remote_row() {
                session
                    .remote
                    .row_tokens
                    .entry(workspace.id)
                    .or_insert_with(|| Arc::new(()));
            }
        }
        session.journal_binding = Some(bound.binding);
        if session.core_state.local_workspaces().is_empty() {
            let ticket = self.next_ticket;
            self.next_ticket = ticket
                .checked_add(1)
                .ok_or("journal ticket range exhausted")?;
            let creation = creation::Creation::default_workspace(ticket, session, &self.worker)?;
            self.creations
                .insert((session.id, creation.ticket), creation);
        } else {
            for restoration in crate::runtime::surface_restorer::describe(&session.as_ref()) {
                self.restorations.stage(session.id, restoration);
            }
        }

        Ok(())
    }

    fn accept_finished(
        &mut self,
        ticket: u64,
        result: Result<ResultValue, crate::runtime::journal_product::JournalError>,
        sessions: &mut [&mut EngineSession],
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        if self.answer_command(ticket, &result, sessions)? {
            return Ok(());
        }
        // Resource owners retain diagnostic text; only the command boundary maps IPC categories.
        let result = result.map_err(|error| error.to_string());
        if self.answer_finished_request(ticket, &result, sessions, plugins)? {
            return Ok(());
        }
        if self.restorations.complete(ticket, &result, sessions)? {
            self.report_payload_failures();
            return Ok(());
        }
        if self.restorations.complete_preservation(ticket, &result) {
            return Ok(());
        }
        if let Some(key) = self
            .creations
            .iter()
            .find_map(|(key, creation)| (creation.ticket == ticket).then_some(*key))
        {
            let id = key.0;
            let session = sessions
                .iter_mut()
                .find(|session| session.id == id)
                .ok_or("creating engine disappeared")?;
            let result = result.map_err(|error| {
                tracing::error!(ticket, engine = ?id, bound = session.journal_binding.is_some(),
                    %error, "journal creation completion failed");
                error
            })?;
            if self
                .creations
                .get_mut(&key)
                .expect("creation exists")
                .answered(&self.worker, session, &self.spawner, result)?
            {
                if let Some(mut creation) = self.creations.remove(&key) {
                    self.note_first_workspace_failure(id, &creation);
                    if let Some(request) = creation.failed_restore(session) {
                        self.restorations.return_ready(id, request);
                    }
                }
                self.refresh_in_progress_commands();
            }
            return Ok(());
        }
        self.complete_opening(ticket, result, sessions)?;
        Ok(())
    }
}
