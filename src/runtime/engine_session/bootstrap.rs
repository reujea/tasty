use super::{EngineId, EngineSession};
use crate::core::CoreState;
use crate::runtime::counters::RuntimeCounters;
use crate::settings::Settings;
use std::sync::Arc;
use tasty_terminal::Waker;

/// engine 생성 경로가 공통으로 함께 옮기는 자원 묶음.
pub(crate) struct EngineSessionSpec {
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) waker: Waker,
    pub(crate) shared_ids: Option<RuntimeCounters>,
    pub(crate) layout_slot: Option<crate::core::layout_persistence::LayoutSlotId>,
    /// memory.db 요청 핸들. 제품은 호스트의 memory writer 핸들, 시험은 바로 실행하는 핸들을 넘긴다.
    pub(crate) memory: tasty_memory::writer::MemoryAccess,
    pub(crate) runner_registry: Arc<tasty_task_runtime::RunnerRegistry>,
}

impl EngineSession {
    /// Resource-only test owner with default Settings and in-memory storage.
    /// Structure fixtures must publish a committed journal model explicitly.
    #[cfg(test)]
    pub fn new(cols: usize, rows: usize, waker: Waker) -> anyhow::Result<Self> {
        let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
            std::sync::Arc::new(std::sync::Mutex::new(
                tasty_memory::MemoryStore::open_in_memory()?,
            ));
        let runner_registry = std::sync::Arc::new(tasty_task_runtime::RunnerRegistry::new());
        Self::new_with_ids_and_settings(
            EngineSessionSpec {
                cols,
                rows,
                waker,
                shared_ids: None,
                layout_slot: None,
                memory: tasty_memory::writer::MemoryAccess::inline(memory),
                runner_registry,
            },
            Settings::default(),
        )
    }

    /// 다른 engine과 발급기를 공유할 수 있다. 슬롯이 있고 restore_layout이 켜져 있을 때만 읽는다.
    /// runner 등록부는 TaskService가 가진 Arc를 넘긴다. 설정은 호출자가 고른다. 이번 실행에서 적용한
    /// 값이 저장되지 않았을 수 있으므로 여기서 파일을 읽지 않는다.
    #[cfg(feature = "gui")]
    pub fn new_with_ids(
        spec: EngineSessionSpec,
        settings: Settings,
        registries: super::super::registries::RuntimeRegistries,
    ) -> anyhow::Result<Self> {
        let state = Self::for_journal(spec, settings, registries)?;
        Ok(state)
    }

    /// Allocate services and the resource owner without creating a local structure or PTY.
    pub(crate) fn for_journal(
        spec: EngineSessionSpec,
        settings: Settings,
        registries: super::super::registries::RuntimeRegistries,
    ) -> anyhow::Result<Self> {
        Self::assemble(spec, settings, Some(registries))
    }

    #[cfg(test)]
    pub(crate) fn new_with_ids_and_settings(
        spec: EngineSessionSpec,
        settings: Settings,
    ) -> anyhow::Result<Self> {
        Self::assemble(spec, settings, None)
    }

    fn assemble(
        spec: EngineSessionSpec,
        settings: Settings,
        registries: Option<super::super::registries::RuntimeRegistries>,
    ) -> anyhow::Result<Self> {
        // CoreState와 실행 자원이 파일을 읽기 전에 검사 홈을 설정한다.
        #[cfg(test)]
        let isolated_home = Some(crate::test_support::IsolatedHome::new());
        let EngineSessionSpec {
            cols,
            rows,
            waker,
            shared_ids,
            layout_slot,
            memory,
            runner_registry,
        } = spec;
        let next_ids = shared_ids.unwrap_or_default();
        #[cfg(test)]
        let standalone_fixture = registries.is_none();
        let registries =
            registries.unwrap_or_else(|| super::super::registries::RuntimeRegistries::new(None));
        #[cfg(test)]
        if standalone_fixture {
            registries.child_terminals.test_attach_writer();
        }
        let mut session = Self {
            id: EngineId::issue(),
            remote: crate::remote::state::RemoteState::new(),
            live: crate::core::live::LiveDomainState::with_notifications(
                next_ids.notification_counter(),
                settings.notification.coalesce_ms,
            ),
            journal_binding: None,
            engine_release: None,
            pending_materializations: Default::default(),
            pending_resource_retirements: Default::default(),
            persistence: super::EnginePersistence::new(layout_slot),
            core_state: CoreState::new_base(),
            hooks: crate::hook_runtime::HookRuntimeState::with_counters(
                next_ids.hook_counter(),
                next_ids.global_hook_counter(),
            ),
            task_scope: tasty_task_runtime::TaskScope::new(runner_registry),
            runner_stop: None,
            observer_router: crate::output_observer::ObserverRouter::with_counter(
                next_ids.observer_counter(),
            ),
            runtime: crate::runtime::engine_runtime::EngineRuntime::new(
                next_ids.clone(),
                waker.clone(),
                memory,
                settings,
                cols,
                rows,
                registries,
            ),
            #[cfg(test)]
            _isolated_home: isolated_home,
            #[cfg(all(test, feature = "gui"))]
            test_host_commands: None,
        };
        session.runtime.child_terminals.bind_slot(layout_slot);

        Ok(session)
    }
}
