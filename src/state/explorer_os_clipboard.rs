//! Explorer 클립보드와 OS 파일 클립보드를 잇는다.
//!
//! 로컬 경로를 복사·잘라내면 OS 클립보드에도 올리고, 올린 직후 OS 클립보드를 한 번 읽어 둔다.
//! 붙여넣을 때 OS 클립보드가 그때와 같으면 Tasty 목록을, 달라졌으면 OS 쪽을 쓴다 — 나중에 한 복사가
//! 이긴다. 다른 앱이 글자 등으로 바꿨으면 오래된 목록은 버린다. 원격에서 복사한 경로는 OS 에 올리지
//! 않는다(같은 문자열의 로컬 파일로 읽히지 않도록).

use std::path::PathBuf;

use crate::clipboard::error::ClipboardError;
use crate::clipboard::files::{OsClip, OsFiles};

use super::{ExplorerClipboard, ExplorerPathSource, MainViewState};

/// Publication and internal sharing are separate outcomes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FileCopyOutcome {
    Published,
    InternalOnly(ClipboardError),
    RemoteSlot,
}

/// 붙여넣기가 쓸 목록.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PasteFrom {
    /// 앱 공용 Explorer 목록(원격 출처일 수 있다).
    Tasty,
    TastyUnavailable(ClipboardError),
    Unavailable(ClipboardError),
    /// OS 클립보드의 로컬 파일.
    Os(OsFiles),
    /// OS 클립보드에 로컬 파일이 아닌 항목이 있다.
    NotLocal,
    /// 다른 프로그램이 나중에 클립보드를 가졌지만 내용을 읽지 못했다.
    Unreadable,
    /// 붙여넣을 것이 없다.
    Nothing,
}

/// Tasty 목록과 OS 클립보드를 읽은 결과로 붙여넣을 목록을 정한다.
pub(crate) fn paste_from(
    tasty: Option<&ExplorerClipboard>,
    os: Result<OsClip, ClipboardError>,
) -> PasteFrom {
    let os = match os {
        Ok(os) => os,
        // OS 클립보드에 접근하지 못하면 Tasty 목록만 쓴다.
        Err(error) => {
            return if tasty.is_some() {
                PasteFrom::TastyUnavailable(error)
            } else {
                PasteFrom::Unavailable(error)
            };
        }
    };
    if let Some(clip) = tasty {
        match &clip.os_seen {
            // 복사한 뒤 OS 클립보드가 그대로다.
            Some(seen) if *seen == os => return PasteFrom::Tasty,
            // Without a baseline, a readable external offer must not revive a stale cut.
            _ => {}
        }
    }
    match os {
        OsClip::Files(files) => PasteFrom::Os(files),
        OsClip::NotLocal => PasteFrom::NotLocal,
        OsClip::Unreadable { .. } => PasteFrom::Unreadable,
        OsClip::Other => PasteFrom::Nothing,
    }
}

impl MainViewState {
    /// Explorer 의 복사·잘라내기 목록을 정한다. 로컬 경로는 OS 파일 클립보드에도 올린다.
    pub(crate) fn explorer_set_clipboard(
        &mut self,
        paths: Vec<PathBuf>,
        cut: bool,
        source: ExplorerPathSource,
    ) -> FileCopyOutcome {
        let outcome = if source == ExplorerPathSource::Local {
            let files = OsFiles {
                paths: paths.clone(),
                cut,
            };
            match self.file_clipboard.write(&files) {
                Ok(()) => FileCopyOutcome::Published,
                Err(error) => FileCopyOutcome::InternalOnly(error),
            }
        } else {
            FileCopyOutcome::RemoteSlot
        };
        let os_seen = match self.file_clipboard.read() {
            Ok(clip) => Some(clip),
            Err(error) => {
                tracing::debug!(kind = ?std::mem::discriminant(&error), "file clipboard snapshot unavailable after copy");
                // A successful write establishes an expected offer even if its readback fails.
                // Preserve the app cut identity on platforms that omit the cut mark on read.
                matches!(outcome, FileCopyOutcome::Published).then(|| {
                    OsClip::Files(self.file_clipboard.as_read(&OsFiles {
                        paths: paths.clone(),
                        cut,
                    }))
                })
            }
        };
        self.explorer_clipboard.set(Some(ExplorerClipboard {
            identity: Default::default(),
            paths,
            cut,
            source,
            os_seen,
        }));
        outcome
    }

    /// 붙여넣기 전에 Tasty 목록을 OS 클립보드와 맞춘다. 붙여넣을 것이 없으면 false.
    /// OS 쪽이 더 새로우면 그 목록을 Tasty 목록으로 삼아, 잘라내기 소비가 같은 길을 탄다.
    pub(crate) fn explorer_take_os_clipboard(&mut self, target: ExplorerPathSource) -> bool {
        let os = self.file_clipboard.read();
        match paste_from(self.explorer_clipboard.get().as_ref(), os) {
            PasteFrom::Tasty => true,
            PasteFrom::TastyUnavailable(error) => {
                if self
                    .explorer_clipboard
                    .get()
                    .is_some_and(|c| c.source == target && c.is_local())
                {
                    let (key, kind) =
                        error.files_feedback(crate::clipboard::error::FileClipboardOp::Paste);
                    self.toasts.push(
                        crate::i18n::t(key),
                        kind,
                        crate::adapters::ui::ToastScope::Window,
                    );
                }
                // Preserve the failure until the user action chooses the matching slot.
                tracing::debug!(kind = ?std::mem::discriminant(&error), "using app clipboard without OS access");
                true
            }
            PasteFrom::Unavailable(error) => {
                self.clipboard_error_toast(&error, crate::adapters::ui::ToastScope::Window);
                false
            }
            PasteFrom::Os(files) => {
                self.explorer_clipboard.set(Some(ExplorerClipboard {
                    identity: Default::default(),
                    paths: files.paths.clone(),
                    cut: files.cut,
                    source: ExplorerPathSource::Local,
                    os_seen: Some(OsClip::Files(files)),
                }));
                true
            }
            PasteFrom::NotLocal => {
                self.explorer_refuse_os_paste("explorer.state.os_paste_not_local")
            }
            PasteFrom::Unreadable => {
                self.explorer_refuse_os_paste("explorer.state.os_paste_unreadable")
            }
            PasteFrom::Nothing => {
                self.explorer_clipboard.set(None);
                false
            }
        }
    }

    /// OS 클립보드가 나중 것이지만 붙여넣을 수 없다. 오래된 목록을 버리고 이유를 알린다.
    fn explorer_refuse_os_paste(&mut self, key: &str) -> bool {
        self.explorer_clipboard.set(None);
        self.toasts.push(
            crate::i18n::t(key).to_string(),
            crate::adapters::ui::ToastKind::Info,
            crate::adapters::ui::ToastScope::Window,
        );
        false
    }

    /// 경로 출처가 `target` 인 칸에서 붙여넣기 메뉴를 켤 것인가. 붙여넣기가 쓸 목록(나중에 복사한 쪽)이
    /// 같은 컴퓨터의 경로일 때만 참이다. OS 클립보드 쪽은 늘 로컬 파일이다.
    pub(crate) fn explorer_can_paste_into(&self, target: ExplorerPathSource) -> bool {
        let tasty = self.explorer_clipboard.get();
        match paste_from(tasty.as_ref(), self.file_clipboard.read()) {
            PasteFrom::Tasty | PasteFrom::TastyUnavailable(_) => {
                tasty.is_some_and(|c| !c.paths.is_empty() && c.source == target)
            }
            PasteFrom::Unavailable(_) => target == ExplorerPathSource::Local,
            PasteFrom::Os(_) => target == ExplorerPathSource::Local,
            PasteFrom::NotLocal | PasteFrom::Unreadable | PasteFrom::Nothing => false,
        }
    }

    /// 로컬 칸의 붙여넣기 메뉴.
    #[cfg(test)]
    pub(crate) fn explorer_can_paste(&self) -> bool {
        self.explorer_can_paste_into(ExplorerPathSource::Local)
    }
}

/// 잘라낸 목록을 옮겼다. OS 클립보드가 아직 그 목록이면 비워 같은 파일을 다시 붙여넣지 못하게 한다.
pub(crate) fn cut_consumed(os: &crate::clipboard::files::Handle, clip: &ExplorerClipboard) {
    if clip.is_local() {
        os.clear_if_holding(&OsFiles {
            paths: clip.paths.clone(),
            cut: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(cut: bool) -> OsFiles {
        OsFiles {
            paths: vec![crate::test_support::abs_path("a/b.txt")],
            cut,
        }
    }

    fn tasty(os_seen: Option<OsClip>) -> ExplorerClipboard {
        ExplorerClipboard {
            identity: Default::default(),
            paths: vec![crate::test_support::abs_path("w/x.txt")],
            cut: true,
            source: ExplorerPathSource::Local,
            os_seen,
        }
    }

    #[test]
    fn successful_publication_keeps_cut_identity_when_readback_temporarily_fails() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        state.file_clipboard = handle;
        memory
            .drops_cut_mark
            .store(true, std::sync::atomic::Ordering::Relaxed);
        memory
            .read_unavailable
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            state.explorer_set_clipboard(files(true).paths, true, ExplorerPathSource::Local),
            FileCopyOutcome::Published
        );
        let identity = state.explorer_clipboard.get().unwrap().identity;
        memory
            .read_unavailable
            .store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        let clip = state.explorer_clipboard.get().unwrap();
        assert!(clip.cut);
        assert!(std::sync::Arc::ptr_eq(&identity, &clip.identity));
        memory.set(OsClip::NotLocal);
        assert!(!state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        assert!(state.explorer_clipboard.get().is_none());
    }

    #[test]
    fn unavailable_paste_is_reachable_but_opening_the_menu_is_silent() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        state.file_clipboard = handle;
        memory
            .unavailable
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(state.explorer_can_paste());
        assert!(!state.explorer_can_paste_into(ExplorerPathSource::Remote { workspace: 9 }));
        assert_eq!(state.toasts.len(), 0);
        assert!(!state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        assert_eq!(
            state.toasts.messages(),
            vec![crate::i18n::t("toast.clipboard_unavailable")]
        );
        memory
            .unavailable
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let before = state.toasts.len();
        assert!(!state.explorer_can_paste());
        assert!(!state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        assert_eq!(state.toasts.len(), before);
    }

    #[test]
    fn failed_publication_preserves_the_slot_identity_and_reports_internal_paste() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        state.file_clipboard = handle;
        memory
            .unavailable
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(matches!(
            state.explorer_set_clipboard(files(true).paths, true, ExplorerPathSource::Local),
            FileCopyOutcome::InternalOnly(ClipboardError::BackendUnavailable(_))
        ));
        let identity = state.explorer_clipboard.get().unwrap().identity;
        assert!(state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        assert!(std::sync::Arc::ptr_eq(
            &identity,
            &state.explorer_clipboard.get().unwrap().identity
        ));
        assert_eq!(
            state.toasts.messages(),
            vec![crate::i18n::t("toast.clipboard_files_internal_paste")]
        );
        // Access recovery must never resurrect the old cut over a later external offer.
        for external in [
            OsClip::Other,
            OsClip::NotLocal,
            OsClip::Unreadable { owner: 9 },
        ] {
            assert!(!matches!(
                paste_from(Some(&tasty(None)), Ok(external)),
                PasteFrom::Tasty | PasteFrom::TastyUnavailable(_)
            ));
        }
    }

    #[test]
    fn the_later_copy_wins() {
        let ours = OsClip::Files(files(false));
        // OS 클립보드가 복사한 뒤 그대로면 Tasty 목록이다.
        assert_eq!(
            paste_from(Some(&tasty(Some(ours.clone()))), Ok(ours.clone())),
            PasteFrom::Tasty
        );
        // 다른 앱이 파일을 복사했으면 그쪽이다.
        let theirs = files(true);
        assert_eq!(
            paste_from(
                Some(&tasty(Some(ours.clone()))),
                Ok(OsClip::Files(theirs.clone()))
            ),
            PasteFrom::Os(theirs.clone())
        );
        assert_eq!(
            paste_from(None, Ok(OsClip::Files(theirs.clone()))),
            PasteFrom::Os(theirs)
        );
        // 다른 앱이 글자를 복사했으면 오래된 잘라내기 목록을 쓰지 않는다.
        assert_eq!(
            paste_from(Some(&tasty(Some(ours.clone()))), Ok(OsClip::Other)),
            PasteFrom::Nothing
        );
        assert_eq!(
            paste_from(Some(&tasty(Some(ours))), Ok(OsClip::NotLocal)),
            PasteFrom::NotLocal
        );
    }

    #[test]
    fn without_os_access_the_tasty_list_stays() {
        assert_eq!(
            paste_from(Some(&tasty(Some(OsClip::Other))), Err("no display".into())),
            PasteFrom::TastyUnavailable("no display".into())
        );
        assert_eq!(
            paste_from(None, Err("no display".into())),
            PasteFrom::Unavailable("no display".into())
        );
        // 읽기 접근이 복구되면 외부 목록을 우선한다.
        assert_eq!(
            paste_from(Some(&tasty(None)), Ok(OsClip::Files(files(false)))),
            PasteFrom::Os(files(false))
        );
    }

    #[test]
    fn a_local_copy_reaches_the_os_and_a_remote_one_does_not() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        state.file_clipboard = handle;
        let paths = files(false).paths;
        state.explorer_set_clipboard(paths.clone(), true, ExplorerPathSource::Local);
        assert_eq!(
            memory.get(),
            OsClip::Files(OsFiles {
                paths: paths.clone(),
                cut: true
            })
        );
        assert!(state.explorer_can_paste());

        memory.set(OsClip::Other);
        state.explorer_set_clipboard(paths, false, ExplorerPathSource::Remote { workspace: 3 });
        // 원격 경로는 OS 에 올리지 않는다. Tasty 목록도 로컬에 붙여넣을 수 없다.
        assert_eq!(memory.get(), OsClip::Other);
        assert!(!state.explorer_can_paste());
    }

    #[test]
    fn an_os_cut_becomes_the_tasty_list_and_moving_it_clears_the_os() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        state.file_clipboard = handle;
        memory.set(OsClip::Files(files(true)));
        assert!(state.explorer_can_paste());
        assert!(state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        let clip = state.explorer_clipboard.get().expect("clipboard");
        assert_eq!((clip.paths.clone(), clip.cut), (files(true).paths, true));
        cut_consumed(&state.file_clipboard, &clip);
        assert_eq!(memory.get(), OsClip::Other);
    }

    #[test]
    fn a_text_copy_elsewhere_drops_the_stale_cut() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        state.file_clipboard = handle;
        state.explorer_set_clipboard(files(false).paths, true, ExplorerPathSource::Local);
        memory.set(OsClip::Other);
        assert!(!state.explorer_can_paste());
        assert!(!state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        assert!(state.explorer_clipboard.get().is_none());
    }

    #[test]
    fn a_non_local_os_list_is_refused_with_a_toast() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        state.file_clipboard = handle;
        state.explorer_set_clipboard(files(false).paths, true, ExplorerPathSource::Local);
        memory.set(OsClip::NotLocal);
        assert!(!state.explorer_can_paste());
        let before = state.toasts.len();
        assert!(!state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        assert_eq!(state.toasts.len(), before + 1);
        // 나중 복사가 이겼으므로 오래된 목록도 버린다.
        assert!(state.explorer_clipboard.get().is_none());
    }

    #[test]
    fn an_owner_that_does_not_answer_is_a_later_copy() {
        let hung = OsClip::Unreadable { owner: 7 };
        let ours = OsClip::Files(files(false));
        assert_eq!(
            paste_from(Some(&tasty(Some(ours))), Ok(hung.clone())),
            PasteFrom::Unreadable
        );
        // 복사할 때 읽지 못했어도 응답하지 않는 소유자는 나중 것이다.
        assert_eq!(
            paste_from(Some(&tasty(None)), Ok(hung.clone())),
            PasteFrom::Unreadable
        );
        // 복사 직후에도 같은 소유자가 응답하지 않았다면 바뀐 것이 없다.
        assert_eq!(
            paste_from(Some(&tasty(Some(hung.clone()))), Ok(hung.clone())),
            PasteFrom::Tasty
        );
        assert_eq!(paste_from(None, Ok(hung)), PasteFrom::Unreadable);

        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        state.file_clipboard = handle;
        state.explorer_set_clipboard(files(false).paths, true, ExplorerPathSource::Local);
        memory.set(OsClip::Unreadable { owner: 9 });
        assert!(!state.explorer_can_paste());
        let before = state.toasts.len();
        assert!(!state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        assert_eq!(state.toasts.len(), before + 1);
        assert!(state.explorer_clipboard.get().is_none());
    }

    /// macOS pasteboard 처럼 잘라내기 표지를 버리는 클립보드에서도 옮긴 뒤 클립보드를 비운다.
    #[test]
    fn a_moved_cut_is_cleared_even_where_the_clipboard_drops_the_cut_mark() {
        let (mut state, _engine) = crate::state::tests::test_state();
        let (handle, memory) = crate::clipboard::files::Handle::memory();
        memory
            .drops_cut_mark
            .store(true, std::sync::atomic::Ordering::Relaxed);
        state.file_clipboard = handle;
        state.explorer_set_clipboard(files(false).paths, true, ExplorerPathSource::Local);
        assert_eq!(
            memory.get(),
            OsClip::Files(OsFiles {
                paths: files(false).paths,
                cut: false
            })
        );
        // 표지가 없어진 채로 읽혀도 Tasty 목록은 잘라내기다.
        assert!(state.explorer_take_os_clipboard(ExplorerPathSource::Local));
        let clip = state.explorer_clipboard.get().expect("clipboard");
        assert!(clip.cut);
        cut_consumed(&state.file_clipboard, &clip);
        assert_eq!(memory.get(), OsClip::Other);
    }
}
