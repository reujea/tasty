//! 확인 필요 탭의 예약 id 폴더 제거. 등록된 적이 없어 활성 상태·프로세스·명령을 건드리지 않으므로
//! `enqueue_plugin_remove` 를 쓰지 않는다. 그 경로는 id 로 폴더를 찾아 다른 폴더를 지울 수 있다.

use std::path::{Path, PathBuf};

use serde_json::json;
use tasty_host_plugin::reserved_removal::{ReservedRemovalError, remove_reserved_folder};

use crate::app::{App, plugin_file_lifecycle::Failure};
use crate::{plugin, window};

type Finish = Box<dyn FnOnce(&mut App, Result<(), Failure>)>;

/// 토스트와 확인 문구에 쓰는 폴더 이름. UTF-8 이 아닌 이름은 손실 변환한다.
pub(crate) fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string_lossy().into_owned())
}

fn to_failure(error: ReservedRemovalError) -> Failure {
    let mut failure = Failure::before(error.message);
    if error.deletion_started {
        failure.details = json!({
            "directory_cleanup_started": true,
            "directory_outcome": "unknown",
            "outcome": "unknown",
        });
    }
    failure
}

/// 목록에 없는 폴더는 지우지 않는다. 이미 없으면 목표 상태이므로 성공이고, 앞서 받은 같은 요청이
/// 먼저 지운 경우가 여기에 든다. 남아 있으면 이 창이 본 뒤로 상태가 바뀐 것이므로 실패다.
fn unlisted_outcome(dir: &Path) -> Result<(), Failure> {
    match std::fs::symlink_metadata(dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Failure::before(error)),
        Ok(_) => Err(Failure::before("the folder is not waiting for removal")),
    }
}

/// 폴더 이름과 실패 사유는 길이가 정해지지 않으므로 토스트 제한 안에서 가운데를 줄인다. 잘리면 사유
/// 끝의 원인 문장이 사라진다.
fn removal_toast(
    name: &str,
    result: Result<(), Failure>,
) -> (String, crate::adapters::ui::ToastKind) {
    match result {
        Ok(()) => (
            crate::i18n::t_fmt_fit("plugins.attn_removed", name),
            crate::adapters::ui::ToastKind::Success,
        ),
        Err(error) => (
            crate::i18n::t_fmt_fit("plugins.attn_remove_failed", &error.message),
            crate::adapters::ui::ToastKind::Error,
        ),
    }
}

impl App {
    pub(crate) fn enqueue_reserved_remove(
        &mut self,
        dir: PathBuf,
        finish: impl FnOnce(&mut App, Result<(), Failure>) + 'static,
    ) -> Result<(), Failure> {
        let root =
            plugin::plugin_root().ok_or_else(|| Failure::before("plugin directory unavailable"))?;
        self.plugin_files.enqueue_sized(
            dir.as_os_str().len().saturating_add(root.as_os_str().len()),
            move |app| app.start_reserved_remove(dir, root, Box::new(finish)),
        )?;
        (self.services.reply_wake())();
        Ok(())
    }

    /// 화면 값은 식별에만 쓴다. 차례가 온 시점에 매니저 목록에서 그 경로를 다시 찾는다.
    fn start_reserved_remove(&mut self, dir: PathBuf, root: PathBuf, finish: Finish) {
        let listed = self.plugin_manager.as_ref().is_some_and(|mgr| {
            mgr.reserved_blocked()
                .iter()
                .any(|blocked| blocked.dir == dir)
        });
        let bytes = dir.as_os_str().len().saturating_add(root.as_os_str().len());
        let target = dir.clone();
        self.run_plugin_file(
            (bytes, 0),
            move || {
                if listed {
                    remove_reserved_folder(&root, &target).map_err(to_failure)
                } else {
                    unlisted_outcome(&target)
                }
            },
            |result| result.as_ref().err().map_or(0, |e| e.message.len()),
            crate::app::plugin_file_lifecycle::classify_failure,
            move |app, result| {
                let result = result.and_then(|outcome| outcome);
                if result.is_ok() {
                    if let Some(mgr) = app.plugin_manager.as_mut() {
                        mgr.forget_reserved(&dir);
                    }
                    app.refresh_plugin_window_after_file();
                }
                finish(app, result);
            },
        );
    }

    pub(super) fn handle_remove_reserved(&mut self, dir: PathBuf) {
        let modal = self.view.active_modal_id();
        let name = folder_name(&dir);
        let shown = name.clone();
        let admitted = self.enqueue_reserved_remove(dir, move |app, result| {
            app.finish_reserved_remove(modal, &shown, result);
        });
        if let Err(error) = admitted {
            self.finish_reserved_remove(modal, &name, Err(error));
        }
    }

    /// 창이 열려 있으면 그 창에 결과를 띄운다. 닫혔으면 실패만 메인 창에 알린다.
    fn finish_reserved_remove(
        &mut self,
        modal: Option<winit::window::WindowId>,
        name: &str,
        result: Result<(), Failure>,
    ) {
        let ok = result.is_ok();
        let (message, kind) = removal_toast(name, result);
        if let Some(modal) = modal.and_then(|id| self.view.views.get_mut(&id))
            && let Some(view) = modal.as_any_mut().downcast_mut::<window::PluginsView>()
        {
            view.finish_remove_reserved();
            view.push_toast(message, kind);
        } else if !ok {
            self.toast_settings_result(message, kind);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::TOAST_MAX_CHARS;

    #[test]
    fn an_unlisted_folder_is_never_deleted() {
        let root = tempfile::tempdir().expect("root");
        let kept = root.path().join("kept");
        std::fs::create_dir(&kept).expect("dir");
        assert!(unlisted_outcome(&kept).is_err());
        assert!(kept.exists());
        assert!(unlisted_outcome(&root.path().join("gone")).is_ok());
    }

    /// 깨진 매니페스트의 오류에는 전체 경로가 들어가 제한을 넘는다. 번역 문장과 끝의 원인이 남아야 한다.
    #[test]
    fn a_long_failure_reason_keeps_its_cause_inside_the_toast_limit() {
        crate::i18n::init("en");
        let root = tempfile::tempdir().expect("root");
        let dir = root.path().join("a".repeat(180));
        std::fs::create_dir(&dir).expect("dir");
        std::fs::write(dir.join("tasty-plugin.toml"), "not toml [").expect("manifest");
        let error = remove_reserved_folder(root.path(), &dir).expect_err("refused");
        assert!(dir.exists());
        let reason = error.message.clone();
        let raw = crate::i18n::t_fmt("plugins.attn_remove_failed", &reason);
        assert!(raw.chars().count() > TOAST_MAX_CHARS, "{raw}");

        let (message, kind) = removal_toast("x", Err(to_failure(error)));
        assert_eq!(kind, crate::adapters::ui::ToastKind::Error);
        assert!(message.chars().count() <= TOAST_MAX_CHARS, "{message}");
        assert!(message.starts_with("Remove failed: "), "{message}");
        let cause: String = reason
            .chars()
            .rev()
            .take(30)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        assert!(message.ends_with(&cause), "{message}");
    }

    #[test]
    fn a_long_folder_name_keeps_the_sentence_inside_the_toast_limit() {
        crate::i18n::init("en");
        let (message, kind) = removal_toast(&"a".repeat(255), Ok(()));
        assert_eq!(kind, crate::adapters::ui::ToastKind::Success);
        assert!(message.chars().count() <= TOAST_MAX_CHARS, "{message}");
        assert!(message.starts_with("Removed '"), "{message}");
        assert!(message.ends_with("'."), "{message}");
    }

    #[test]
    fn a_partial_removal_reports_an_unknown_outcome() {
        let failure = to_failure(ReservedRemovalError {
            message: "denied".into(),
            deletion_started: true,
        });
        assert_eq!(failure.message, "denied");
        assert_eq!(failure.details["outcome"], "unknown");
    }
}
