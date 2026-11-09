//! 에이전트·plugin 설정 변경 요청의 실행 시점 적용과 완료 결과.
//!
//! 요청은 바꿀 값만 담고, 실행 시점의 최신 설정 사본에 적용한다. 결과는 원 요청에 한 번 돌아간다.
//! 접수(intent를 넣음)·메모리 적용·파일 저장을 구분하며 계약은 docs/architecture/data-flows.md의
//! "에이전트·plugin 설정 변경 요청의 완료 의미" 절에 있다.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};

use tasty_settings::Settings;

/// 실행 시점 설정에 적용할 변경.
#[derive(Debug, Clone)]
pub(crate) enum SettingsChange {
    /// 지정한 필드만 바꾼다.
    RemoteTransfer {
        dir: Option<String>,
        max_mb: Option<u64>,
    },
    InputRule {
        op: InputRuleOp,
        /// 기본값을 처음 넣는 호출자. `Initialize` 만 쓴다.
        owner: String,
        app: String,
        newline: bool,
    },
    WebhookAllowExternal(bool),
    /// debug 전용 전체 교체. 호출자가 이미 합친 설정이다.
    #[cfg(debug_assertions)]
    Replace(Box<Settings>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputRuleOp {
    Set,
    Remove,
    Initialize,
}

impl SettingsChange {
    /// `settings`에 적용하고 바뀐 것이 있는지 돌려준다. `false`면 저장하지 않는다.
    pub(crate) fn apply(&self, settings: &mut Settings) -> Result<bool, String> {
        match self {
            Self::RemoteTransfer { dir, max_mb } => {
                if let Some(dir) = dir {
                    settings.remote_transfer.dir = dir.clone();
                }
                if let Some(max_mb) = max_mb {
                    settings.remote_transfer.max_mb = *max_mb;
                }
                Ok(true)
            }
            Self::InputRule {
                op,
                owner,
                app,
                newline,
            } => {
                let input = &mut settings.terminal_input;
                match op {
                    InputRuleOp::Remove => {
                        input.remove_rule(app);
                        Ok(true)
                    }
                    InputRuleOp::Set => input.set_rule(app, *newline).map(|()| true),
                    InputRuleOp::Initialize => input.initialize_rule(owner, app, *newline),
                }
            }
            Self::WebhookAllowExternal(allow) => {
                settings.webhook.allow_external = *allow;
                Ok(true)
            }
            #[cfg(debug_assertions)]
            Self::Replace(new) => {
                let observation = settings.file_observation.clone();
                *settings = (**new).clone();
                settings.file_observation = observation;
                Ok(true)
            }
        }
    }
}

/// 설정 변경 요청의 결과.
#[derive(Debug, Clone)]
pub(crate) enum SettingsOutcome {
    /// 메모리에 적용됐다. `changed`가 거짓이면 바꿀 것이 없어 저장도 하지 않았다.
    /// `saved`가 오류면 이번 실행에만 적용되고 파일에는 남지 않았다. `settings`는 적용 뒤의 설정이다.
    Applied {
        changed: bool,
        saved: Result<(), String>,
        settings: Box<Settings>,
    },
    /// 검사·admission·category 초기화 실패. 설정은 바뀌지 않았다.
    Rejected(String),
    /// 끝나기 전에 더 새 설정 변경이 들어와 이 결과는 적용하지 않고 버렸다.
    // reason: 결과를 미루는 category 초기화 경로가 GUI에만 있다.
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    Superseded,
}

impl SettingsOutcome {
    pub(crate) fn unchanged(settings: Settings) -> Self {
        Self::Applied {
            changed: false,
            saved: Ok(()),
            settings: Box::new(settings),
        }
    }
}

/// 결과를 넣는 쪽. 처음 넣은 결과만 유효하다. 결과 없이 모든 사본이 버려지면 받는 쪽은 그 사실을 안다.
#[derive(Clone)]
pub(crate) struct SettingsDone {
    tx: SyncSender<SettingsOutcome>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for SettingsDone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SettingsDone")
    }
}

impl SettingsDone {
    pub(crate) fn finish(self, outcome: SettingsOutcome) {
        if self.tx.try_send(outcome).is_ok() {
            (self.wake)();
        }
    }
}

/// 결과를 기다리는 쪽.
pub(crate) struct SettingsWait {
    rx: Receiver<SettingsOutcome>,
}

/// 기다림의 상태.
pub(crate) enum SettingsPoll {
    Pending,
    Ready(SettingsOutcome),
    /// 결과 없이 요청이 버려졌다.
    Dropped,
}

impl SettingsWait {
    pub(crate) fn poll(&mut self) -> SettingsPoll {
        match self.rx.try_recv() {
            Ok(outcome) => SettingsPoll::Ready(outcome),
            Err(TryRecvError::Empty) => SettingsPoll::Pending,
            Err(TryRecvError::Disconnected) => SettingsPoll::Dropped,
        }
    }
}

/// `wake`는 결과를 넣은 뒤 메인 루프가 응답을 거두도록 깨운다.
pub(crate) fn pair(wake: Arc<dyn Fn() + Send + Sync>) -> (SettingsDone, SettingsWait) {
    let (tx, rx) = sync_channel(1);
    (SettingsDone { tx, wake }, SettingsWait { rx })
}

#[cfg(feature = "gui")]
impl crate::app::App {
    /// 메모리에 적용한 설정의 저장 결과를 기다리는 요청에 돌려주고, 사용자 출처면 저장 실패를 알린다.
    pub(crate) fn settle_settings(
        &mut self,
        origin: &crate::intent::IntentOrigin,
        done: Option<SettingsDone>,
        settings: Settings,
        saved: Result<(), String>,
    ) {
        if let Err(error) = &saved
            && origin.is_user()
        {
            self.toast_settings_result(
                crate::i18n::t_fmt_fit("toast.settings_not_saved", error),
                crate::adapters::ui::ToastKind::Warning,
            );
        }
        if let Some(done) = done {
            done.finish(SettingsOutcome::Applied {
                changed: true,
                saved,
                settings: Box::new(settings),
            });
        }
    }

    /// 끝난 설정 저장을 거두어 원 요청과 사용자 출처에 알린다. 메인 루프 회차마다 부른다.
    /// 출처가 없는 저장(종료 전 재저장)은 file writer가 로그로 남긴다.
    pub(crate) fn settle_settings_saves(&mut self) {
        for settled in self.services.take_settled_settings() {
            let crate::app::file_writer::SaveReply {
                origin,
                done,
                settings,
            } = settled.reply;
            if let Some(origin) = origin {
                self.settle_settings(&origin, done, *settings, settled.saved);
            } else if let Some(done) = done {
                done.finish(SettingsOutcome::Applied {
                    changed: true,
                    saved: settled.saved,
                    settings,
                });
            }
        }
    }

    pub(super) fn settle_settings_rejected(
        &mut self,
        origin: &crate::intent::IntentOrigin,
        done: Option<SettingsDone>,
        reason: String,
    ) {
        if origin.is_user() {
            self.toast_settings_result(
                crate::i18n::t_fmt_fit("toast.settings_not_applied", &reason),
                crate::adapters::ui::ToastKind::Error,
            );
        }
        if let Some(done) = done {
            done.finish(SettingsOutcome::Rejected(reason));
        }
    }

    /// 적용한 글꼴 값을 판정해 처음 거절된 값을 값마다 카드 한 장으로 알린다.
    pub(crate) fn report_rejected_fonts(
        &mut self,
        origin: &crate::intent::IntentOrigin,
        prev: Option<&crate::settings::AppearanceSettings>,
        next: &crate::settings::AppearanceSettings,
    ) {
        for value in self.font_rejections.after_apply(origin, prev, next) {
            self.toast_settings_result(
                crate::i18n::t_fmt_fit("toast.font_rejected", &value),
                crate::adapters::ui::ToastKind::Warning,
            );
        }
    }

    /// 설정 결과 토스트는 포커스된 메인 창에, 없으면 첫 메인 창에 띄운다.
    pub(super) fn toast_settings_result(
        &mut self,
        message: String,
        kind: crate::adapters::ui::ToastKind,
    ) {
        let focused = self.view.focused_view_id;
        let main = match focused.and_then(|id| self.view.views.get(&id)) {
            Some(view) if view.as_main().is_some() => focused,
            _ => None,
        }
        .or_else(|| {
            self.view
                .views
                .iter()
                .find_map(|(id, view)| view.as_main().map(|_| *id))
        });
        let Some(main) = main.and_then(|id| self.view.views.get_mut(&id)) else {
            tracing::warn!("settings result without a main window: {message}");
            return;
        };
        if let Some(main) = main.as_main_mut() {
            main.state
                .toasts
                .push(message, kind, crate::adapters::ui::ToastScope::Window);
            crate::view::ui::View::mark_dirty(main);
        }
    }
}

#[cfg(feature = "gui")]
/// webview 페이지에 넣은 조합은 plugin이 문서를 다시 만들어야 바뀐다.
pub(super) fn announce_shortcut_change(
    mgr: &mut crate::plugin::PluginManager,
    prev: Option<&crate::settings::KeybindingSettings>,
    next: &crate::settings::KeybindingSettings,
) {
    let changed = prev
        .map(|prev| crate::adapters::ipc::handler::keybinding::changed_actions(prev, next))
        .unwrap_or_default();
    if !changed.is_empty() {
        mgr.emit_host_event(
            "command.host_shortcut_changed",
            &serde_json::json!({ "action_ids": changed }),
            tasty_plugin_protocol::EventScope::System,
        );
    }
}

/// 핸들러 시험이 요청을 headless 적용 경로로 끝까지 실행할 때 쓴다.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tasty_settings::{Settings, SettingsStorage};

    /// 저장 횟수를 세고, `fail`이면 저장을 실패시킨다.
    #[derive(Default)]
    pub(crate) struct ProbeStorage {
        pub(crate) saves: AtomicUsize,
        pub(crate) fail: bool,
    }

    impl SettingsStorage for ProbeStorage {
        fn load(&self) -> Settings {
            Settings::default()
        }

        fn save(&self, _settings: &Settings) -> anyhow::Result<()> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                anyhow::bail!("config.toml is not writable");
            }
            Ok(())
        }
    }

    pub(crate) fn core_with(storage: Arc<ProbeStorage>) -> crate::app::services::AppServices {
        crate::ipc::handler::cli_entry_tests::test_core_builder()
            .with_settings_storage(storage)
            .build()
            .expect("test AppServices")
    }

    /// 핸들러가 넣은 intent를 headless 경로로 적용하고 미뤄 둔 응답을 만든다.
    pub(crate) fn settle(
        core: &mut crate::app::services::AppServices,
        state: &mut crate::state::RequestContext,
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
        intents: Vec<crate::intent::DispatchedIntent>,
    ) -> tasty_ipc::protocol::JsonRpcResponse {
        for intent in intents {
            crate::intent::headless::apply_one_for_test(core, state, engine, intent);
        }
        core.answer_parked_for_test()
    }
}

/// 저장이 막혀 있는 동안 결과는 대기이고, 저장이 끝나면 원 결과가 한 번 나오는지 실제 적용 경로로 본다.
#[cfg(test)]
mod boundary_tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, Sender, channel};

    /// `save`에 들어오면 알리고, 풀어 줄 때까지 기다린다.
    struct GatedStorage {
        entered: Mutex<Sender<Settings>>,
        release: Mutex<Receiver<()>>,
        fail: bool,
        files: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl tasty_settings::SettingsStorage for GatedStorage {
        fn load(&self) -> Settings {
            Settings::default()
        }

        fn save(&self, settings: &Settings) -> anyhow::Result<()> {
            self.entered
                .lock()
                .unwrap()
                .send(settings.clone())
                .expect("observer alive");
            self.release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("released");
            if self.fail {
                anyhow::bail!("config.toml is not writable");
            }
            self.files.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    fn run(fail: bool) {
        use std::sync::atomic::Ordering;
        let (entered_tx, entered_rx) = channel();
        let (release_tx, release_rx) = channel();
        let files = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let storage = Arc::new(GatedStorage {
            entered: Mutex::new(entered_tx),
            release: Mutex::new(release_rx),
            fail,
            files: files.clone(),
        });
        let writer = crate::app::file_writer::start(
            storage.clone(),
            tasty_utils::batch_writer::Limits::default(),
            Arc::new(tasty_utils::batch_writer::SystemClock),
            Arc::new(|| {}),
        )
        .expect("file writer");
        let mut core = crate::ipc::handler::cli_entry_tests::test_core_builder()
            .with_file_writer(writer)
            .with_settings_storage(storage)
            .build()
            .expect("test AppServices");
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = wakes.clone();
        let (done, wait) = pair(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        let intent = crate::app::command::DomainIntent::ChangeSettings {
            change: SettingsChange::RemoteTransfer {
                dir: None,
                max_mb: Some(4242),
            },
            done,
        }
        .from_agent_ipc();

        // The applying thread returns while the real file worker is still blocked.
        crate::intent::headless::apply_one_for_test(&mut core, &mut state, &mut engine, intent);
        let saving = entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("save entered");
        assert_eq!(saving.remote_transfer.max_mb, 4242);
        let mut wait = wait;
        assert!(matches!(wait.poll(), SettingsPoll::Pending));
        assert_eq!(files.load(Ordering::SeqCst), 0);
        core.poll_file_writer(); // Another event-loop turn can run before storage is released.
        release_tx.send(()).expect("storage waits");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while core.file_writer_stats().unwrap().1.replaced
            + core.file_writer_stats().unwrap().1.failed
            == 0
        {
            assert!(
                std::time::Instant::now() < deadline,
                "writer did not finish"
            );
            std::thread::yield_now();
        }
        let outcome = loop {
            crate::intent::headless::settle_settings_saves(&mut core);
            if let SettingsPoll::Ready(outcome) = wait.poll() {
                break outcome;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "receipt did not settle"
            );
            std::thread::yield_now();
        };

        assert_eq!(
            engine.runtime.settings.remote_transfer.max_mb, 4242,
            "applied to memory whether or not the save succeeded"
        );
        let response = crate::ipc::handler::deferred::settings_outcome_response(
            serde_json::json!(1),
            Some(outcome),
            |id, changed, settings| {
                tasty_ipc::protocol::JsonRpcResponse::success(
                    id,
                    serde_json::json!({ "changed": changed, "max_mb": settings.remote_transfer.max_mb }),
                )
            },
        );
        if fail {
            let error = response.error.expect("a failed save is not a success");
            assert_eq!(
                error.data.unwrap(),
                serde_json::json!({ "applied": true, "saved": false })
            );
            assert_eq!(files.load(Ordering::SeqCst), 0);
        } else {
            assert_eq!(
                response.result.unwrap(),
                serde_json::json!({ "changed": true, "max_mb": 4242 })
            );
            assert_eq!(files.load(Ordering::SeqCst), 1);
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1, "woken once");
        assert!(
            matches!(wait.poll(), SettingsPoll::Dropped),
            "answered once"
        );
    }

    #[test]
    fn a_held_save_keeps_the_answer_pending_then_reports_success_once() {
        run(false);
    }

    #[test]
    fn a_held_save_keeps_the_answer_pending_then_reports_the_failure_once() {
        run(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> (SettingsDone, SettingsWait) {
        pair(Arc::new(|| {}))
    }

    #[test]
    fn first_outcome_wins_and_wakes_once() {
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = wakes.clone();
        let (done, mut wait) = pair(Arc::new(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        let second = done.clone();
        done.finish(SettingsOutcome::Rejected("first".into()));
        second.finish(SettingsOutcome::Superseded);
        assert!(matches!(
            wait.poll(),
            SettingsPoll::Ready(SettingsOutcome::Rejected(reason)) if reason == "first"
        ));
        assert_eq!(wakes.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn dropping_every_sender_without_an_outcome_is_visible() {
        let (done, mut wait) = quiet();
        assert!(matches!(wait.poll(), SettingsPoll::Pending));
        drop(done);
        assert!(matches!(wait.poll(), SettingsPoll::Dropped));
    }

    #[test]
    fn initializing_an_already_seeded_rule_changes_nothing() {
        let mut settings = Settings::default();
        let change = |owner: &str| SettingsChange::InputRule {
            op: InputRuleOp::Initialize,
            owner: owner.into(),
            app: "claude".into(),
            newline: true,
        };
        assert_eq!(change("plugin.a").apply(&mut settings), Ok(true));
        assert_eq!(change("plugin.a").apply(&mut settings), Ok(false));
    }

    #[test]
    fn remote_transfer_keeps_the_field_it_was_not_given() {
        let mut settings = Settings::default();
        settings.remote_transfer.dir = "/latest".into();
        let change = SettingsChange::RemoteTransfer {
            dir: None,
            max_mb: Some(7),
        };
        assert_eq!(change.apply(&mut settings), Ok(true));
        assert_eq!(settings.remote_transfer.dir, "/latest");
        assert_eq!(settings.remote_transfer.max_mb, 7);
    }
}
