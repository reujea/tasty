//! 앱의 모든 창이 함께 쓰는 Explorer 복사·잘라내기 목록.
//!
//! 앱 서비스가 하나를 만들어 창마다 같은 슬롯을 넘긴다. 그래서 OS 클립보드에 접근하지 못해도 한 창에서
//! 복사한 목록을 다른 창이 붙여넣고, 원격 출처 표시와 잘라내기 소비도 창 사이에 그대로 이어진다.

use std::sync::{Arc, Mutex, MutexGuard};

use super::ExplorerClipboard;

/// 공용 슬롯. 복제하면 같은 슬롯을 가리킨다.
#[derive(Clone, Default)]
pub(crate) struct ExplorerClipboardSlot(Arc<Mutex<Option<ExplorerClipboard>>>);

impl ExplorerClipboardSlot {
    fn lock(&self) -> MutexGuard<'_, Option<ExplorerClipboard>> {
        // 슬롯은 값을 통째로 바꾸므로 다른 창의 패닉으로 독이 들어도 값은 온전하다.
        static POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::poison::recover_mutex(self.0.lock(), "explorer clipboard", &POISONED)
    }

    /// 지금 목록의 사본.
    pub(crate) fn get(&self) -> Option<ExplorerClipboard> {
        self.lock().clone()
    }

    pub(crate) fn set(&self, clip: Option<ExplorerClipboard>) {
        *self.lock() = clip;
    }

    pub(crate) fn take(&self) -> Option<ExplorerClipboard> {
        self.lock().take()
    }

    /// 지금 목록의 잘라내기 대기 경로. 창마다 흐리게 그린다.
    pub(crate) fn cut_paths(&self) -> std::collections::HashSet<std::path::PathBuf> {
        self.lock()
            .as_ref()
            .filter(|c| c.cut)
            .map(|c| c.paths.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 두 슬롯이 같은 공용 슬롯인가.
    #[cfg(test)]
    pub(crate) fn same_slot(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl super::MainViewState {
    /// 앱 창 하나의 View 상태. Explorer 복사·잘라내기 목록과 OS 파일 클립보드는 앱이 넘긴 것을 쓴다.
    pub(crate) fn for_window(
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        preset_store: Arc<Mutex<tasty_presets::PresetStore>>,
        explorer_clipboard: ExplorerClipboardSlot,
        file_clipboard: crate::clipboard::files::Handle,
    ) -> Self {
        let mut state = Self::new(engine, preset_store);
        state.explorer_clipboard = explorer_clipboard;
        state.file_clipboard = file_clipboard;
        state
    }
}

impl crate::app::services::AppServices {
    /// 새 창의 View 상태. 모든 창이 이 앱의 공용 목록과 OS 클립보드 핸들을 받는다.
    pub(crate) fn window_state(
        &self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) -> super::MainViewState {
        super::MainViewState::for_window(
            engine,
            self.preset_store.clone(),
            self.explorer_clipboard.clone(),
            self.file_clipboard.clone(),
        )
    }
}

impl std::fmt::Debug for ExplorerClipboardSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExplorerClipboardSlot")
            .field(&*self.lock())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ExplorerPathSource, MainViewState};

    /// 같은 engine 을 보는 두 번째 창. 첫 창의 슬롯을 넘겨받는다.
    fn second_window(
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        slot: &ExplorerClipboardSlot,
    ) -> MainViewState {
        let store = Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        MainViewState::for_window(engine, store, slot.clone(), Default::default())
    }

    /// OS 클립보드에 접근하지 못하는 창으로 만든다.
    fn without_os(state: &mut MainViewState) {
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        memory
            .unavailable
            .store(true, std::sync::atomic::Ordering::Relaxed);
        state.file_clipboard = handle;
    }

    #[test]
    fn a_copy_in_one_window_pastes_in_another_without_the_os_clipboard() {
        let (mut a, engine) = crate::state::tests::test_state();
        let mut b = second_window(&engine.read(), &a.explorer_clipboard);
        assert!(b.explorer_clipboard.same_slot(&a.explorer_clipboard));
        without_os(&mut a);
        without_os(&mut b);
        let paths = vec![crate::test_support::abs_path("w/a.txt")];
        a.explorer_set_clipboard(paths.clone(), true, ExplorerPathSource::Local);
        assert!(b.explorer_can_paste());
        assert!(b.explorer_take_os_clipboard(ExplorerPathSource::Local));
        let clip = b.explorer_clipboard.get().expect("shared list");
        assert_eq!((clip.paths, clip.cut), (paths.clone(), true));
        // 잘라내기 대기 표시도 두 창이 같다.
        assert_eq!(
            a.explorer_clipboard.cut_paths(),
            b.explorer_clipboard.cut_paths()
        );
        assert!(a.explorer_clipboard.cut_paths().contains(&paths[0]));
    }

    #[test]
    fn a_remote_copy_reaches_the_other_window_still_marked_remote() {
        let (mut a, engine) = crate::state::tests::test_state();
        let mut b = second_window(&engine.read(), &a.explorer_clipboard);
        without_os(&mut a);
        without_os(&mut b);
        let remote = ExplorerPathSource::Remote { workspace: 7 };
        a.explorer_set_clipboard(vec!["/same/on/both".into()], false, remote);
        // 다른 창도 원격 출처로 받는다. 로컬 칸의 붙여넣기 행은 꺼진다.
        assert_eq!(b.explorer_clipboard.get().map(|c| c.source), Some(remote));
        assert!(!b.explorer_can_paste());
        // 복사는 흐리게 그리지 않는다.
        assert!(b.explorer_clipboard.cut_paths().is_empty());
    }

    /// 앱이 만드는 창은 모두 앱의 공용 목록과 같은 OS 클립보드 핸들을 받는다.
    #[test]
    fn every_window_the_app_makes_shares_the_app_slot() {
        let services = crate::adapters::ipc::handler::cli_entry_tests::test_core();
        let (_state, engine) = crate::state::tests::test_state();
        let a = services.window_state(&engine.read());
        let b = services.window_state(&engine.read());
        assert!(a.explorer_clipboard.same_slot(&services.explorer_clipboard));
        assert!(b.explorer_clipboard.same_slot(&a.explorer_clipboard));
        assert!(a.file_clipboard.same(&services.file_clipboard));
        assert!(b.file_clipboard.same(&a.file_clipboard));
    }

    #[test]
    fn separate_slots_do_not_share() {
        let (a, engine) = crate::state::tests::test_state();
        let b = second_window(&engine.read(), &ExplorerClipboardSlot::default());
        assert!(!b.explorer_clipboard.same_slot(&a.explorer_clipboard));
    }
}
