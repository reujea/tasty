//! DomainIntent를 적용하고 CoreEvent에 따른 후속 처리를 실행한다.
//! 구조 변경은 journal 명령에 합류하고 비저널 결과와 사용자 표시 후속을 원 engine으로 전달한다.
//! 설정·테마·플러그인 등 App 자원이 필요한 처리는 여기에 둔다.
//! [계층 경계](../../docs/dev-guide/app-state-ownership.md)를 따른다.

mod terminal;
mod theme_order;

use tasty_settings::Settings;

use crate::app::App;
use crate::app::command::CoreEvent;
use crate::app::window_access::{DispatchCtx, engines_mut};
use crate::core::AttentionKind;
use crate::intent::{DispatchedIntent, Intent, IntentOrigin};
use crate::view::ui::View as _;

/// 요청이 시작된 engine. 도메인 변경과 후속 처리가 같은 engine을 사용한다.
/// 창·parked 어느 관계든 engine id로 가리켜 사이에 창을 열거나 닫아도 대상이 바뀌지 않는다.
/// 창 View와 MainViewState는 처리 시점에 [`resolve`](crate::app::window_access::EngineScanMut::resolve)로 찾는다.
#[derive(Debug, Clone, Copy)]
pub(crate) enum DispatchSource {
    Engine(crate::runtime::engine_session::EngineId),
}

impl DispatchSource {
    pub(crate) fn engine(self) -> crate::runtime::engine_session::EngineId {
        let Self::Engine(id) = self;
        id
    }
}

impl App {
    /// Core가 반환한 이벤트를 같은 source·origin으로 처리한다. 결과를 받을 run이 있는 명령은
    /// journal 완료가 그 run을 끝내며, journal에 들어가지 못했으면 여기서 실패로 끝낸다.
    pub(crate) fn dispatch_domain_intent(
        &mut self,
        source: DispatchSource,
        dispatched: DispatchedIntent,
    ) -> anyhow::Result<()> {
        let Some(run) = dispatched.completion else {
            return self.apply_domain_intent(source, dispatched);
        };
        self.journal.set_intent_completion(Some(run));
        let applied = self.apply_domain_intent(source, dispatched);
        if let Some(run) = self.journal.take_intent_completion() {
            crate::request_runs::fail_unapplied(run, &applied);
        }
        applied
    }

    fn apply_domain_intent(
        &mut self,
        source: DispatchSource,
        mut dispatched: DispatchedIntent,
    ) -> anyhow::Result<()> {
        if let Intent::RemoteBrowser(request) = &dispatched.body {
            return self
                .remote_browser_request(source.engine(), request.clone())
                .map_err(anyhow::Error::msg);
        }
        if let Intent::AttachSizeSync(action) = &dispatched.body {
            self.apply_attach_size_sync_action(*action, std::time::Instant::now());
            return Ok(());
        }
        if let Intent::DisconnectMirror { workspace_id } = &dispatched.body {
            self.disconnect_mirror_workspace(*workspace_id);
            return Ok(());
        }
        if self.take_saved_row_intent(&dispatched.body) {
            return Ok(());
        }
        if let Intent::CapturePreset {
            kind,
            source: target,
            presentation,
        } = &dispatched.body
        {
            return self
                .queue_preset_capture_intent(
                    source.engine(),
                    *kind,
                    *target,
                    presentation,
                    &dispatched.origin,
                )
                .map_err(anyhow::Error::msg);
        }
        let tutorial_preparation = match &dispatched.body {
            Intent::PrepareTutorial { ticket } => Some(ticket.clone()),
            _ => None,
        };
        if let Some(ticket) = tutorial_preparation.as_ref()
            && self
                .engines_mut()
                .resolve(source.engine())
                .is_none_or(|context| !context.state.tutorial.matches_preparation(ticket))
        {
            return Ok(());
        }
        let after_create = match &dispatched.body {
            Intent::NewTabWithFollowup { followup, .. } => Some(followup.clone()),
            _ => None,
        };
        if let Some(context) = self.engines_mut().resolve(source.engine())
            && let Some(intent) = crate::app::creation_intent::resolve(
                context.state,
                &context.engine.as_ref(),
                &dispatched.body,
                &dispatched.origin,
            )?
        {
            dispatched.body = Intent::Domain(intent);
        }
        if let Intent::CommitDivider(commit) = &dispatched.body {
            let id = source.engine();
            let result = self
                .engines
                .session_mut(id)
                .ok_or_else(|| "divider engine disappeared".to_owned())
                .and_then(|session| {
                    self.journal
                        .admit_divider(session, commit.clone(), &dispatched.origin)
                });
            if result.is_err()
                && let Some(context) = self.engines_mut().resolve(id)
            {
                context.state.layout_previews.cancel(commit.sequence);
                if let Some(view) = context.view {
                    view.mark_dirty();
                }
            }
            return result.map_err(anyhow::Error::msg);
        }
        if let Intent::ForwardMirror {
            op,
            close_focus_candidates,
        } = &dispatched.body
        {
            let id = source.engine();
            let continuation = self.view_continuation(id, after_create.clone(), None);
            self.admit_forwarded_request(
                id,
                op.clone(),
                &dispatched.origin,
                dispatched.admitted,
                continuation,
                close_focus_candidates.clone(),
            );
            return Ok(());
        }
        if let Intent::DirectRename(rename) = &dispatched.body {
            self.journal
                .admit_direct_rename(source.engine(), rename, &dispatched.origin);
            return Ok(());
        }
        let Intent::Domain(intent) = dispatched.body else {
            anyhow::bail!("dispatch_domain_intent: non-Domain Intent");
        };
        if let crate::app::command::DomainIntent::RetireExitedSurface {
            surface_id,
            generation,
        } = &intent
            && self.engines.get(source.engine()).is_none_or(|engine| {
                !engine
                    .runtime
                    .terminals
                    .matches_generation(*surface_id, *generation)
            })
        {
            return Ok(());
        }
        let origin = dispatched.origin;
        let id = source.engine();
        let continuation =
            self.view_continuation(id, after_create.clone(), tutorial_preparation.clone());
        if self.hold_saved_row_close(id, &origin, &intent) {
            return Ok(());
        }
        if let Some(session) = self.engines.get(id)
            && session
                .mirror_workspace_index_for_structural(&intent)
                .is_some()
            && let Some(op) =
                crate::app::services::impl_mirror::build_mirror_forward_op(session.core, &intent)
        {
            if let Some(refusal) = self.workspace_close_refusal(&op) {
                self.report_workspace_close_refusal(id, &origin, refusal);
                return Ok(());
            }
            self.admit_forwarded_request(
                id,
                op,
                &origin,
                dispatched.admitted,
                continuation,
                Vec::new(),
            );
            return Ok(());
        }
        if let Some(session) = self.engines.session_mut(id)
            && self.journal.admit_metadata_intent(
                session.id,
                &session.core_state,
                &intent,
                &origin,
                continuation,
            )
        {
            return Ok(());
        }
        let input_receipt = self.user_input_receipt(&origin, &intent, dispatched.admitted);
        let core = &mut self.services;
        let Some(DispatchCtx {
            state,
            mut engine,
            view,
            ..
        }) = engines_mut!(self).resolve(id)
        else {
            anyhow::bail!("dispatch_domain_intent: engine {id:?} not found");
        };
        let applied = core.apply_live(&mut engine, intent);
        let failed = applied.is_err();
        let events = events_or_report(view.as_ref().map(|_| &mut *state), &origin, applied);
        if failed && let Some(view) = view {
            view.mark_dirty();
        }
        if let Some(receipt) = input_receipt {
            receipt.settle(&events);
        }
        for event in events {
            self.handle_core_event(source, &origin, event);
        }
        Ok(())
    }

    /// journal 완료 뒤 요청한 창에서 이어 할 일. 창이 없는 engine이면 없다.
    fn view_continuation(
        &mut self,
        id: crate::runtime::engine_session::EngineId,
        after_create: Option<crate::intent::CreateFollowup>,
        tutorial_preparation: Option<std::sync::Weak<()>>,
    ) -> Option<crate::app::journal::commands::IntentViewContinuation> {
        let context = self.engines_mut().resolve(id)?;
        context.view.map(
            |view| crate::app::journal::commands::IntentViewContinuation {
                remote_request: None,
                view: view.state.identity(),
                selection: context.state.navigation.generation(),
                activate_surface: None,
                close_empty_engine: false,
                preset_apply: false,
                after_create,
                tutorial: None,
                tutorial_preparation,
                tutorial_surface: None,
            },
        )
    }

    /// PTY 출력은 사용자·에이전트 요청이 아니므로 System origin으로 처리한다.
    pub(crate) fn handle_core_event_system(&mut self, source: DispatchSource, event: CoreEvent) {
        let origin = IntentOrigin::System;
        self.handle_core_event(source, &origin, event);
    }

    /// 창별 변경은 source·origin을 사용하고 전역 변경은 공용 상태에 적용한다.
    fn handle_core_event(
        &mut self,
        source: DispatchSource,
        origin: &IntentOrigin,
        event: CoreEvent,
    ) {
        if let Some(DispatchCtx { engine, .. }) = engines_mut!(self).resolve(source.engine())
            && let Some((surface, generation)) = event.terminal_binding()
            && !engine
                .runtime
                .terminals
                .matches_generation(surface, generation)
        {
            return;
        }
        match event {
            CoreEvent::SettingsUpdated { settings, done } => {
                self.cascade_settings_updated(settings, origin, done);
            }
            CoreEvent::NotificationPushRequested {
                level,
                ws_id,
                surface_id,
                title,
                body,
                source,
                merge,
            } => {
                self.cascade_notification_pushed(crate::app::command::NotificationPush {
                    level,
                    ws_id,
                    surface_id,
                    title,
                    body,
                    source,
                    merge,
                });
            }
            CoreEvent::NotificationReadRequested { id } => {
                self.cascade_notification_read(id);
            }
            CoreEvent::AllNotificationsReadRequested { ids } => {
                self.cascade_all_notifications_read(&ids);
            }
            CoreEvent::SurfaceCwdChanged { surface_id, .. } => {
                self.cascade_surface_cwd_changed(surface_id);
            }
            CoreEvent::TerminalMarkSet { surface_id } => {
                self.cascade_terminal_mark_set(surface_id);
            }
            CoreEvent::SurfaceCompletionRequested { surface_id, kind } => {
                self.cascade_surface_completion(surface_id, kind);
            }
            CoreEvent::SurfaceAttentionClearRequested { surface_id, kind } => {
                self.cascade_surface_attention_clear(surface_id, kind);
            }
            CoreEvent::SurfaceSent { .. } => {}
            CoreEvent::TerminalNotification {
                surface_id,
                title,
                body,
                ..
            } => {
                self.cascade_terminal_notification(source, surface_id, title, body);
            }
            CoreEvent::TerminalBellRing { surface_id, .. } => {
                self.cascade_terminal_bell_ring(source, surface_id);
            }
            CoreEvent::TerminalOutputMatch {
                surface_id, text, ..
            } => {
                self.cascade_terminal_output_match(source, surface_id, text);
            }
            CoreEvent::TerminalTitleChanged {
                surface_id,
                title,
                generation,
            } => {
                self.cascade_terminal_title_changed(source, surface_id, title, generation);
            }
            CoreEvent::TerminalCwdChanged {
                surface_id,
                generation,
            } => {
                self.cascade_terminal_pty_cwd_changed(source, surface_id, generation);
            }
            CoreEvent::TerminalCommandCompleted {
                surface_id,
                exit_code,
                ..
            } => {
                self.cascade_terminal_command_completed(source, surface_id, exit_code);
            }
            CoreEvent::TerminalShellIntegrationHint { surface_id, .. } => {
                self.cascade_terminal_shell_integration_hint(source, surface_id);
            }
            CoreEvent::TerminalClipboardSet {
                surface_id, result, ..
            } => {
                self.cascade_terminal_clipboard_set(source, surface_id, result);
            }
            CoreEvent::TerminalProcessExited {
                surface_id,
                generation,
            } => {
                self.cascade_terminal_process_exited(source, surface_id, generation);
            }
            CoreEvent::TabNameUpdated { .. } => {
                // OSC 제목은 저장 대상이 아니므로 레이아웃 저장 대신 화면 갱신만 요청한다.
                self.mark_source_window_dirty(source);
            }
            plugin @ (CoreEvent::PluginLoaded { .. }
            | CoreEvent::PluginEnableToggled { .. }
            | CoreEvent::PluginUnloaded { .. }
            | CoreEvent::PluginError { .. }
            | CoreEvent::PluginSurfaceKindRegistered { .. }
            | CoreEvent::PluginRegistryChanged { .. }
            | CoreEvent::PluginWindowDeclared { .. }) => self.cascade_plugin_event(plugin),
        }
    }

    // 플러그인 이벤트는 첫 MainView의 큐에 넣는다. 창이 없으면 이 경로는 큐에 넣지 않는다.

    /// 플러그인 매니저는 App 전체에서 공유하므로 창 source 없이 이벤트를 처리한다.
    pub(crate) fn cascade_plugin_events(&mut self, events: Vec<CoreEvent>) {
        for ev in events {
            self.cascade_plugin_event(ev);
        }
    }

    fn cascade_plugin_event(&mut self, ev: CoreEvent) {
        match ev {
            CoreEvent::PluginLoaded { plugin_id, version } => {
                self.cascade_plugin_loaded(plugin_id, version)
            }
            CoreEvent::PluginEnableToggled { plugin_id, enabled } => {
                self.cascade_plugin_enable_toggled(plugin_id, enabled)
            }
            CoreEvent::PluginUnloaded { plugin_id, reason } => {
                self.cascade_plugin_unloaded(plugin_id, reason)
            }
            CoreEvent::PluginError {
                plugin_id,
                error_kind,
                message,
            } => self.cascade_plugin_error(plugin_id, error_kind, message),
            CoreEvent::PluginSurfaceKindRegistered {
                plugin_id,
                kind,
                rendering,
            } => self.cascade_plugin_surface_kind_registered(plugin_id, kind, rendering),
            CoreEvent::PluginRegistryChanged { plugin_id, change } => {
                self.cascade_plugin_registry_changed(plugin_id, change)
            }
            CoreEvent::PluginWindowDeclared {
                plugin_id,
                window_id,
            } => self.cascade_plugin_window_declared(plugin_id, window_id),
            other => {
                tracing::warn!(
                    "cascade_plugin_events: non-plugin CoreEvent received: {:?}",
                    std::mem::discriminant(&other)
                );
            }
        }
    }

    fn enqueue_plugin_host_event(&mut self, ev: crate::state::PendingHostEvent) {
        self.state.pending_host_events.push(ev);
    }

    fn cascade_plugin_loaded(&mut self, plugin_id: String, version: String) {
        self.enqueue_plugin_host_event(crate::state::PendingHostEvent::PluginLoaded {
            plugin_id,
            version,
        });
    }

    fn cascade_plugin_enable_toggled(&mut self, plugin_id: String, enabled: bool) {
        self.enqueue_plugin_host_event(crate::state::PendingHostEvent::PluginEnableToggled {
            plugin_id,
            enabled,
        });
    }

    fn cascade_plugin_unloaded(
        &mut self,
        plugin_id: String,
        reason: tasty_plugin_protocol::events::LifecycleReason,
    ) {
        let reason_str = match reason {
            tasty_plugin_protocol::events::LifecycleReason::User => "user",
            tasty_plugin_protocol::events::LifecycleReason::Ipc => "ipc",
            tasty_plugin_protocol::events::LifecycleReason::Crash => "crash",
        };
        // 비활성 플러그인이 선언한 훅 이벤트는 새 등록에 사용할 수 없게 한다.
        self.services
            .registries
            .plugin_hook_events
            .unregister(&plugin_id);
        self.enqueue_plugin_host_event(crate::state::PendingHostEvent::PluginUnloaded {
            plugin_id,
            reason: reason_str.to_string(),
        });
    }

    fn cascade_plugin_error(&mut self, plugin_id: String, error_kind: String, message: String) {
        self.enqueue_plugin_host_event(crate::state::PendingHostEvent::PluginError {
            plugin_id,
            error_kind,
            message,
        });
    }

    fn cascade_plugin_surface_kind_registered(
        &mut self,
        plugin_id: String,
        kind: String,
        rendering: String,
    ) {
        self.enqueue_plugin_host_event(
            crate::state::PendingHostEvent::PluginSurfaceKindRegistered {
                plugin_id,
                kind,
                rendering,
            },
        );
    }

    fn cascade_plugin_registry_changed(
        &mut self,
        plugin_id: String,
        change: crate::app::command::PluginRegistryChange,
    ) {
        use crate::app::command::PluginRegistryChange;
        let (change_kind, detail) = match change {
            PluginRegistryChange::Installed { version } => {
                ("installed", serde_json::json!({ "version": version }))
            }
            PluginRegistryChange::Removed => ("removed", serde_json::Value::Null),
            PluginRegistryChange::PermissionGranted { permission } => (
                "permission_granted",
                serde_json::json!({ "permission": permission }),
            ),
            PluginRegistryChange::PermissionRevoked { permission } => (
                "permission_revoked",
                serde_json::json!({ "permission": permission }),
            ),
        };
        self.enqueue_plugin_host_event(crate::state::PendingHostEvent::PluginRegistryChanged {
            plugin_id,
            change_kind: change_kind.to_string(),
            detail,
        });
    }

    fn cascade_plugin_window_declared(&mut self, plugin_id: String, window_id: String) {
        self.enqueue_plugin_host_event(crate::state::PendingHostEvent::PluginWindowDeclared {
            plugin_id,
            window_id,
        });
    }

    fn cascade_terminal_mark_set(&mut self, surface_id: u32) {
        for mut engine in self.engines_mut().windowed_and_parked() {
            if let Some(t) = engine.find_terminal_by_id_mut(surface_id) {
                t.set_mark();
                return;
            }
        }
    }

    fn cascade_surface_completion(&mut self, surface_id: u32, kind: AttentionKind) {
        self.apply_on_first_engine(|engine| {
            if !engine.has_surface(surface_id) {
                return false;
            }
            engine.raise_attention(surface_id, kind);
            engine.mark_layout_dirty();
            true
        });
    }

    /// 요청 후 다른 kind의 attention이 생겼으면 kind_filter로 남겨 둔다.
    /// IPC 핸들러가 이미 해제했을 수도 있어 필터 결과와 무관하게 화면 갱신을 요청한다.
    fn cascade_surface_attention_clear(
        &mut self,
        surface_id: u32,
        kind_filter: Option<AttentionKind>,
    ) {
        self.apply_on_first_engine(|engine| {
            if !engine.has_surface(surface_id) {
                return false;
            }
            if kind_filter.is_none_or(|k| engine.attention_kind(surface_id) == Some(k)) {
                engine.clear_attention(surface_id);
            }
            engine.mark_layout_dirty();
            true
        });
    }

    fn cascade_surface_cwd_changed(&mut self, surface_id: u32) {
        self.apply_on_first_engine(|engine| {
            if !engine.has_surface(surface_id) {
                return false;
            }
            engine.refresh_tab_display_name(surface_id);
            engine.mark_layout_dirty();
            true
        });
    }

    /// 검사를 통과한 설정만 journal 에 들인다. 거절하면 이전 설정을 유지하고 사유를 돌려준다.
    fn admit_settings(&mut self, new_settings: &Settings) -> Result<u64, String> {
        if let Err(e) = new_settings.validate_runtime() {
            tracing::error!("settings not applied: {e}");
            return Err(e);
        }
        self.journal.note_settings_intent().map_err(|error| {
            tracing::error!("settings admission failed: {error}");
            error
        })
    }

    /// 창과 parked 상태의 설정을 모두 갱신해야 복원된 창이 옛 설정을 쓰지 않는다.
    /// 결과는 `done`(기다리는 요청)과 사용자 출처의 토스트로 알린다. category 초기화가 필요하면
    /// journal 완료 뒤 `finish_settings_results`가 같은 일을 한다.
    pub(crate) fn cascade_settings_updated(
        &mut self,
        new_settings: Settings,
        origin: &IntentOrigin,
        done: Option<crate::app::settings_change::SettingsDone>,
    ) {
        let generation = match self.admit_settings(&new_settings) {
            Ok(generation) => generation,
            Err(reason) => {
                self.settle_settings_rejected(origin, done, reason);
                return;
            }
        };
        let turning_off = self
            .engines()
            .windowed_and_parked()
            .next()
            .is_some_and(|engine| engine.runtime.settings.general.workspace_categories_enabled)
            && !new_settings.general.workspace_categories_enabled;
        if turning_off {
            let ids: Vec<_> = self
                .engines()
                .windows()
                .filter_map(|(window, _)| self.engines.of_window(window))
                .chain(self.engines().parked_with_ids().map(|(id, _)| id))
                .collect();
            let mut changes = Vec::new();
            for id in ids {
                let Some(binding) = self.engines.journal_binding(id) else {
                    tracing::error!("category reset engine has no journal binding");
                    self.settle_settings_rejected(
                        origin,
                        done,
                        "category reset engine has no journal binding".into(),
                    );
                    return;
                };
                changes.push(crate::runtime::journal_product::StreamCommand {
                    stream: binding.stream.clone(),
                    command: tasty_core::StructuralCommand::ResetCategories,
                });
            }
            self.journal
                .admit_settings_reset(generation, changes, new_settings, origin, done);
            return;
        }
        self.apply_settings_after_structure(new_settings, origin, done);
    }

    /// 창과 parked 상태에 새 설정을 적용하고 저장을 file writer에 넣는다. 저장을 기다리지 않는다.
    /// 저장 결과는 메인 루프가 거두어 `done`과 사용자 출처의 토스트로 알린다([`Self::settle_settings_saves`]).
    pub(crate) fn apply_settings_after_structure(
        &mut self,
        new_settings: Settings,
        origin: &IntentOrigin,
        done: Option<crate::app::settings_change::SettingsDone>,
    ) {
        crate::app::task_report::apply(&self.services, &new_settings);
        crate::app::anomaly_settings::apply(&self.services, &new_settings);
        crate::app::attach_heartbeat::apply(&self.services, &new_settings);
        let prev_settings = self
            .engines()
            .windowed_and_parked()
            .next()
            .map(|e| &e.runtime.settings);
        let prev_appearance = prev_settings.map(|s| s.appearance.clone());
        let prev_language = prev_settings.map(|s| s.general.language.clone());
        let content_restore_turned_off = prev_settings
            .is_some_and(|s| s.general.restore_surface_content)
            && !new_settings.general.restore_surface_content;
        let prev_keybindings = prev_settings.map(|s| s.keybindings.clone());

        for session in self.engines.all_sessions_mut() {
            session.runtime.settings = new_settings.clone();
            // 구조가 바뀌지 않아도 다음 capture가 생겨야 이전 scrollback이 든 payload가
            // 현재 복원 원본에서 빠진다. 종료 강제 capture는 내용 복원이 꺼지면 dirty를 요구한다.
            if content_restore_turned_off {
                session.persistence.dirty.mark_dirty();
            }
        }
        for main in self.main_windows_iter_mut() {
            main.mark_dirty();
        }
        self.services
            .submit_settings_save(new_settings.clone(), Some(origin.clone()), done);

        let appearance_changed =
            theme_order::appearance_changed(prev_appearance.as_ref(), &new_settings.appearance);
        theme_order::install_theme_then(&new_settings, || {
            self.announce_settings_change(
                &new_settings,
                appearance_changed,
                prev_language,
                prev_keybindings,
            )
        });

        // 바뀐 단축키가 macOS 메뉴 표시에도 반영되도록 재구성한다.
        #[cfg(target_os = "macos")]
        crate::macos_delegate::rebuild_main_menu(&new_settings.keybindings);
        self.report_rejected_fonts(origin, prev_appearance.as_ref(), &new_settings.appearance);
        // 접수 거절이나 writer 없는 구성의 결과는 바로 나와 있다.
        self.settle_settings_saves();
    }

    /// 전역 Theme 설치 뒤에 창·터미널 색을 갱신하고 plugin에 설정 변경 이벤트를 발행한다.
    /// `theme.changed`는 [`theme_order::appearance_changed`]가 참일 때 한 번 발행한다.
    fn announce_settings_change(
        &mut self,
        new_settings: &Settings,
        appearance_changed: bool,
        prev_language: Option<String>,
        prev_keybindings: Option<crate::settings::KeybindingSettings>,
    ) {
        if appearance_changed {
            for view in self.view.views.values_mut() {
                view.base_mut().gpu.refresh_theme();
                view.mark_dirty();
            }
            for session in self.engines.all_sessions_mut() {
                session.borrow_mut().resync_terminal_palettes();
            }
        }

        if let Some(mgr) = self.plugin_manager.as_mut() {
            use tasty_plugin_protocol::EventScope;
            use tasty_plugin_protocol::events::payloads::{LanguageChanged, ThemeChanged};
            if appearance_changed {
                mgr.emit_host_event(
                    "theme.changed",
                    &ThemeChanged {
                        theme_id: new_settings.appearance.theme.clone(),
                    },
                    EventScope::System,
                );
            }
            if prev_language.as_deref() != Some(new_settings.general.language.as_str()) {
                mgr.emit_host_event(
                    "language.changed",
                    &LanguageChanged {
                        language_code: new_settings.general.language.clone(),
                    },
                    EventScope::System,
                );
            }
            crate::app::settings_change::announce_shortcut_change(
                mgr,
                prev_keybindings.as_ref(),
                &new_settings.keybindings,
            );
        }
    }

    /// 서로 다른 호스트 사건은 `merge: Separate`로 넣어 앞 알림과 합치지 않는다.
    pub(crate) fn cascade_notification_pushed(
        &mut self,
        push: crate::app::command::NotificationPush,
    ) {
        let crate::app::command::NotificationPush {
            level,
            ws_id,
            surface_id,
            title,
            body,
            source,
            merge,
        } = push;
        let Some((_, mut engine)) = engines_mut!(self)
            .sessions()
            .find(|(_, engine)| engine.has_workspace(ws_id))
        else {
            tracing::warn!(
                ws_id,
                "cascade NotificationPushRequested: workspace not found"
            );
            return;
        };
        let created_id = merge.add(
            &mut engine.live.notifications,
            ws_id,
            surface_id,
            title.clone(),
            body.clone(),
            level,
        );
        if let Some(nid) = created_id {
            engine.raise_attention(surface_id, AttentionKind::Completion);
            // OS 벨과의 중복을 피하려는 TerminalBellRing 표지는 사운드에서 제외한다.
            if engine.runtime.settings.notification.sound && source != "TerminalBellRing" {
                self.services.sound_player().play();
            }
            engine.enqueue_host_event(crate::state::PendingHostEvent::NotificationCreated {
                id: nid,
                level,
                title,
                body,
                source,
            });
        }
    }

    /// Shared IDs find the owning store; its existing attention policy applies.
    fn cascade_notification_read(&mut self, id: u64) {
        for (_, main, mut engine) in self.engines_mut().window_pairs() {
            engine.mark_notification_read(id);
            main.mark_dirty();
        }
        for mut engine in self.engines_mut().parked() {
            engine.mark_notification_read(id);
        }
    }

    fn cascade_all_notifications_read(&mut self, ids: &[u64]) {
        for (_, main, mut engine) in self.engines_mut().window_pairs() {
            for id in ids {
                engine.mark_notification_read(*id);
            }
            main.mark_dirty();
        }
        for mut engine in self.engines_mut().parked() {
            for id in ids {
                engine.mark_notification_read(*id);
            }
        }
    }
}

/// workspace 생성의 창별 후속 처리. 사용자 요청일 때만 활성 workspace를 옮긴다.
/// apply 오류는 요청한 창의 state로 알린다. mirror 차단 toast도 이 경로로 뜬다.
/// 오류를 여기서 처리하므로 후속 처리할 이벤트가 없다.
fn events_or_report(
    state: Option<&mut crate::state::MainViewState>,
    origin: &IntentOrigin,
    applied: anyhow::Result<Vec<CoreEvent>>,
) -> Vec<CoreEvent> {
    applied.unwrap_or_else(|err| {
        crate::intent::report_apply_error(
            state,
            origin,
            crate::intent::FailureContext::General,
            &err,
        );
        Vec::new()
    })
}
