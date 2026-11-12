use super::settings_for_new_engine;
use crate::app::file_writer::SettingsSaves;
use crate::settings::Settings;
use std::cell::Cell;
use std::sync::Arc;

fn with_toast(ms: u64) -> Settings {
    let mut settings = Settings::default();
    settings.overlay.toast_duration_ms = ms;
    settings
}

#[test]
fn an_applied_snapshot_wins_without_reading_the_file() {
    let latest = with_toast(7_777);
    let loads = Cell::new(0);

    let chosen = settings_for_new_engine(Some(&latest), || {
        loads.set(loads.get() + 1);
        with_toast(1_111)
    });

    assert_eq!(chosen.overlay.toast_duration_ms, 7_777);
    assert_eq!(loads.get(), 0);
}

#[test]
fn the_file_is_read_only_when_nothing_was_applied() {
    let loads = Cell::new(0);

    let chosen = settings_for_new_engine(None, || {
        loads.set(loads.get() + 1);
        with_toast(1_111)
    });

    assert_eq!(chosen.overlay.toast_duration_ms, 1_111);
    assert_eq!(loads.get(), 1);
}

struct RefusingStorage;

impl tasty_settings::SettingsStorage for RefusingStorage {
    fn load(&self) -> Settings {
        Settings::load()
    }
    fn save(&self, _: &Settings) -> anyhow::Result<()> {
        anyhow::bail!("replacement refused")
    }
}

/// 저장이 실패해 파일은 옛 값으로 남아도 새 engine은 이번 실행에서 적용한 값으로 시작한다.
#[test]
fn a_new_engine_keeps_the_applied_value_after_the_save_fails() {
    let _home = crate::test_support::IsolatedHome::new();
    with_toast(1_111).save().expect("seed the settings file");
    let mut saves = SettingsSaves::new(None, Arc::new(RefusingStorage));
    saves.submit(with_toast(7_777), None, None);
    let settled = saves.reap();
    assert_eq!(settled.len(), 1);
    assert!(settled[0].saved.is_err(), "the save must fail");
    assert_eq!(Settings::load().overlay.toast_duration_ms, 1_111);

    let chosen = settings_for_new_engine(saves.latest(), Settings::load);
    let engine = crate::runtime::engine_session::EngineSession::new_with_ids_and_settings(
        crate::runtime::engine_session::EngineSessionSpec {
            cols: 80,
            rows: 24,
            waker: Arc::new(|| {}),
            shared_ids: None,
            layout_slot: None,
            memory: tasty_memory::writer::MemoryAccess::inline(Arc::new(std::sync::Mutex::new(
                tasty_memory::MemoryStore::open_in_memory().expect("memory"),
            ))),
            runner_registry: Arc::new(tasty_task_runtime::RunnerRegistry::new()),
        },
        chosen,
    )
    .expect("engine");

    assert_eq!(engine.runtime.settings.overlay.toast_duration_ms, 7_777);
}
