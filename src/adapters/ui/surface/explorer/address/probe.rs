//! 주소창 확정을 이번 프레임의 탐색기 동작으로 바꾼다. 파일 경로면 그 파일이 든 폴더로 가서 파일을 고른다.
//! 원격 경로는 로컬에서 볼 수 없으므로 이동하기 전에 부모 폴더의 원격 목록을 한 번 받아 파일인지 확인한다.
//! 규칙은 docs/surfaces/explorer/index.md의 "주소 입력" 절에 있다.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::super::ExplorerAction;
use super::super::view::{ExplorerListRequest, ExplorerView, LIST_DIR_SOFT_TIMEOUT};
use super::{AddressTarget, remote, resolve};
use crate::core::fs_list::DirEntryInfo;

/// 응답을 기다리는 원격 주소 확인. 확인하는 동안 사용자가 다른 폴더로 가면 버린다.
#[derive(Debug)]
pub(crate) struct AddressProbe {
    request_id: u64,
    /// 입력을 확정할 때 보던 폴더.
    from: PathBuf,
    /// 입력이 가리키는 원격 경로.
    target: PathBuf,
    /// `target` 의 부모 폴더와 마지막 이름.
    dir: PathBuf,
    name: String,
    sent_at: Instant,
    /// 응답. 파일이면 목록이 알려 준 그 파일의 경로, 폴더이거나 확인하지 못했으면 None이다.
    answer: Option<Option<PathBuf>>,
}

impl AddressProbe {
    pub(crate) fn owns_request(&self, request_id: u64) -> bool {
        self.request_id == request_id && self.answer.is_none()
    }
    pub(crate) fn pending_since(&self) -> Option<Instant> {
        self.answer.is_none().then_some(self.sent_at)
    }
}

impl ExplorerView {
    /// 주소창에서 확정한 입력이나 원격 확인의 응답을 동작으로 바꾼다. `input` 은 이번 프레임에 확정한
    /// 입력이고, 없으면 기다리던 원격 확인의 결과만 본다.
    /// 확인을 기다리는 동안에는 다른 입력이 없어도 제한 시간에 깨어나도록 `ctx` 에 다시 그리기를 예약한다.
    pub(crate) fn address_action(
        &mut self,
        ctx: &egui::Context,
        input: Option<&str>,
        current: &Path,
        remote: bool,
    ) -> Option<ExplorerAction> {
        let action = match input {
            Some(input) => self.address_input(input, current, remote),
            None => self.probe_outcome(current),
        };
        if let Some(probe) = self.addr_probe.as_ref().filter(|p| p.answer.is_none()) {
            ctx.request_repaint_after(
                LIST_DIR_SOFT_TIMEOUT.saturating_sub(probe.sent_at.elapsed()),
            );
        }
        action
    }

    /// 원격 확인을 기다린 시간. 기다리는 확인이 없거나 응답이 왔으면 None 이다.
    pub(crate) fn address_waiting(&self) -> Option<std::time::Duration> {
        self.addr_probe
            .as_ref()
            .filter(|p| p.answer.is_none())
            .map(|p| p.sent_at.elapsed())
    }

    /// 시험: 응답을 기다리는 원격 주소 확인 하나를 둔다.
    #[cfg(test)]
    pub(crate) fn wait_address_for_test(&mut self) {
        self.addr_probe = Some(AddressProbe {
            request_id: 0,
            from: PathBuf::new(),
            target: PathBuf::new(),
            dir: PathBuf::new(),
            name: String::new(),
            sent_at: Instant::now(),
            answer: None,
        });
    }

    /// 기다리던 원격 확인을 버린다. 주소창은 다음 `sync` 에서 현재 폴더로 돌아간다.
    pub(crate) fn cancel_address_probe(&mut self) {
        self.addr_probe = None;
    }

    /// 확정한 입력을 동작으로 바꾼다. 기다리던 확인은 어떤 입력이든(거부·빈 입력 포함) 버린다.
    fn address_input(
        &mut self,
        input: &str,
        current: &Path,
        remote: bool,
    ) -> Option<ExplorerAction> {
        self.addr_probe = None;
        match resolve(input, current, self.address_host(remote))? {
            Ok(AddressTarget::Folder(dir)) => Some(ExplorerAction::Navigate(dir)),
            Ok(AddressTarget::File { dir, file }) => {
                self.reveal_after_load(dir.clone(), file);
                Some(ExplorerAction::Navigate(dir))
            }
            Ok(AddressTarget::Remote(path)) => self.start_probe(path, current),
            Err(why) => Some(ExplorerAction::AddressRejected(why)),
        }
    }

    /// 부모 폴더의 원격 목록을 요청한다. 부모가 없는 경로(루트)거나 원격이 아니면 바로 그 경로로 간다.
    fn start_probe(&mut self, target: PathBuf, current: &Path) -> Option<ExplorerAction> {
        let split = remote::parent_and_name(&target.to_string_lossy());
        let (Some(local_ws_id), Some((dir, name))) = (self.mirror_ws_id, split) else {
            return Some(ExplorerAction::Navigate(target));
        };
        let request_id = crate::core::next_list_dir_request_id();
        let dir = PathBuf::from(dir);
        self.outbox.push(ExplorerListRequest {
            local_ws_id,
            request_id,
            dir: dir.clone(),
        });
        self.addr_probe = Some(AddressProbe {
            request_id,
            from: current.to_path_buf(),
            target,
            dir,
            name,
            sent_at: Instant::now(),
            answer: None,
        });
        None
    }

    /// 원격 확인이 끝났으면 이동한다. 파일이면 부모 폴더로 가서 고르고, 폴더이거나 목록을 받지 못했으면
    /// 입력한 경로로 간다(그 결과는 목록 자리에 보인다). 제한 시간을 넘기면 받지 못한 것으로 본다.
    fn probe_outcome(&mut self, current: &Path) -> Option<ExplorerAction> {
        let probe = self.addr_probe.as_ref()?;
        if probe.from != current {
            self.addr_probe = None;
            return None;
        }
        if probe.answer.is_none() && probe.sent_at.elapsed() <= LIST_DIR_SOFT_TIMEOUT {
            return None;
        }
        let probe = self.addr_probe.take()?;
        match probe.answer.flatten() {
            Some(file) => {
                self.reveal_after_load(probe.dir.clone(), file);
                Some(ExplorerAction::Navigate(probe.dir))
            }
            None => Some(ExplorerAction::Navigate(probe.target)),
        }
    }

    /// 원격 확인 요청의 응답이면 기록하고 true를 돌려준다. 같은 이름의 폴더가 아닌 항목이 있으면 파일이다.
    /// server 가 링크 항목의 종류를 대상으로 정하므로 폴더 링크는 폴더, 파일 링크와 대상이 없는 링크는 파일이다.
    /// 로컬과 같이 링크는 풀지 않고 링크가 든 폴더에서 고른다. 링크 구분을 싣지 않는 옛 server 도 종류는
    /// 같은 규칙으로 실으므로 결과가 같다.
    pub(in super::super) fn answer_address_probe(
        &mut self,
        request_id: u64,
        result: &Result<Vec<DirEntryInfo>, String>,
    ) -> bool {
        let Some(probe) = self
            .addr_probe
            .as_mut()
            .filter(|p| p.request_id == request_id)
        else {
            return false;
        };
        let entries = result.as_ref().ok();
        let file = entries
            .and_then(|es| es.iter().find(|e| e.name == probe.name && !e.is_dir))
            .map(|e| e.path.clone());
        let dir = probe.dir.clone();
        probe.answer = Some(file.clone());
        // 파일이면 이 폴더로 옮기므로 받은 목록을 그 폴더의 목록으로 쓴다.
        if let (Some(_), Some(entries)) = (file, entries) {
            self.cache_remote_listing(dir, entries.clone());
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn entry(dir: &str, name: &str, is_dir: bool) -> DirEntryInfo {
        DirEntryInfo {
            path: Path::new(dir).join(name),
            name: name.into(),
            is_dir,
            ..Default::default()
        }
    }

    fn navigate(to: &str) -> Option<PathBuf> {
        Some(to.into())
    }

    /// 동작이 이동이면 그 폴더. 이동이 아닌 동작은 시험 실패다.
    fn nav(action: Option<ExplorerAction>) -> Option<PathBuf> {
        match action? {
            ExplorerAction::Navigate(dir) => Some(dir),
            _ => panic!("이동이 아닌 동작"),
        }
    }

    /// mirror view 에서 `/srv` 를 보다가 주소를 확정하고 보낸 확인 요청 ID 를 돌려준다.
    fn remote_view_typed(input: &str) -> (ExplorerView, Option<ExplorerAction>, Option<u64>) {
        let mut view = ExplorerView::default();
        view.mirror_ws_id = Some(7);
        let action = view.address_action(
            &egui::Context::default(),
            Some(input),
            Path::new("/srv"),
            true,
        );
        let request = view.drain_outbox().pop();
        let id = request.as_ref().map(|r| r.request_id);
        if let Some(r) = request {
            assert_eq!(r.local_ws_id, 7);
            assert_eq!(r.dir, view.addr_probe.as_ref().unwrap().dir);
        }
        (view, action, id)
    }

    #[test]
    fn a_local_file_opens_its_folder_and_is_picked_once_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("notes.md"), b"n").unwrap();
        let mut view = ExplorerView::default();
        let action = view.address_action(
            &egui::Context::default(),
            Some("sub/notes.md"),
            tmp.path(),
            false,
        );
        assert_eq!(nav(action), Some(sub.clone()));
        assert!(view.drain_outbox().is_empty());
        view.set_entries(vec![
            entry(&sub.to_string_lossy(), "a.md", false),
            entry(&sub.to_string_lossy(), "notes.md", false),
        ]);
        assert_eq!(view.take_reveal(), Some(sub.join("notes.md")));
        assert_eq!(
            view.selected.iter().collect::<Vec<_>>(),
            [&sub.join("notes.md")]
        );
    }

    #[test]
    fn a_local_folder_moves_without_picking_anything() {
        let tmp = tempfile::tempdir().unwrap();
        let mut view = ExplorerView::default();
        let action = view.address_action(&egui::Context::default(), Some("."), tmp.path(), false);
        assert_eq!(nav(action), Some(tmp.path().into()));
        assert_eq!(view.reveal, None);
    }

    #[test]
    fn a_waiting_remote_check_keeps_the_typed_path_until_it_is_cancelled() {
        let panel = tasty_model::ExplorerPanel::new(1, PathBuf::from("/srv"));
        let (mut view, action, _) = remote_view_typed("a/notes.md");
        assert!(action.is_none());
        assert!(view.address_waiting().is_some());
        view.addr_buffer = "a/notes.md".into();
        view.sync(&panel, Some(7));
        assert_eq!(
            view.addr_buffer, "a/notes.md",
            "the field does not snap back"
        );
        view.cancel_address_probe();
        assert!(view.address_waiting().is_none());
        view.sync(&panel, Some(7));
        assert_eq!(view.addr_buffer, "/srv", "Esc restores the current path");
    }

    #[test]
    fn a_remote_file_waits_for_its_folder_list_then_opens_the_folder_and_picks_it() {
        let (mut view, action, id) = remote_view_typed("a/notes.md");
        assert!(action.is_none());
        let id = id.expect("부모 폴더 목록을 요청해야 한다");
        assert!(
            view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)
                .is_none()
        );

        let listed = Ok(vec![
            entry("/srv/a", "notes.md", false),
            entry("/srv/a", "other", true),
        ]);
        let panel = tasty_model::ExplorerPanel::new(1, PathBuf::from("/srv"));
        assert!(view.apply_remote_list_dir_result(id, &panel, listed));
        assert_eq!(
            nav(view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)),
            navigate("/srv/a")
        );
        assert_eq!(view.reveal, Some(PathBuf::from("/srv/a/notes.md")));
        assert!(view.reveal_select);
        assert!(view.addr_probe.is_none());
    }

    #[test]
    fn a_remote_folder_or_a_failed_list_goes_to_the_typed_path() {
        let (mut view, _, id) = remote_view_typed("/srv/a/docs");
        assert!(view.answer_address_probe(id.unwrap(), &Ok(vec![entry("/srv/a", "docs", true)])));
        assert_eq!(
            nav(view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)),
            navigate("/srv/a/docs")
        );
        assert_eq!(view.reveal, None);

        let (mut view, _, id) = remote_view_typed("/srv/a/missing.md");
        assert!(view.answer_address_probe(id.unwrap(), &Ok(Vec::new())));
        assert_eq!(
            nav(view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)),
            navigate("/srv/a/missing.md")
        );

        let (mut view, _, id) = remote_view_typed("/locked/x.md");
        assert!(view.answer_address_probe(id.unwrap(), &Err("permission denied".into())));
        assert_eq!(
            nav(view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)),
            navigate("/locked/x.md")
        );
    }

    /// 링크 상태를 실은 목록에서도 로컬과 같다. 대상이 없는 링크는 링크가 든 폴더에서 고르고 그 목록을 쓰므로
    /// 옮긴 뒤 끊긴 링크로 보인다. 폴더 링크는 그 경로로 들어간다.
    #[test]
    fn a_remote_broken_link_is_picked_in_its_folder_and_a_folder_link_is_entered() {
        use crate::core::fs_list::EntryLink;
        let linked = |name: &str, is_dir, link| DirEntryInfo {
            link,
            ..entry("/srv/a", name, is_dir)
        };
        let (mut view, _, id) = remote_view_typed("a/gone.md");
        let listed = vec![
            linked("gone.md", false, EntryLink::Broken),
            linked("docs", true, EntryLink::Valid),
        ];
        assert!(view.answer_address_probe(id.unwrap(), &Ok(listed)));
        assert_eq!(
            nav(view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)),
            navigate("/srv/a")
        );
        assert_eq!(view.reveal, Some(PathBuf::from("/srv/a/gone.md")));
        let panel = tasty_model::ExplorerPanel::new(1, PathBuf::from("/srv/a"));
        view.sync(&panel, Some(7));
        assert!(view.is_broken_link(Path::new("/srv/a/gone.md")));

        let (mut view, _, id) = remote_view_typed("a/docs");
        let listed = vec![linked("docs", true, EntryLink::Valid)];
        assert!(view.answer_address_probe(id.unwrap(), &Ok(listed)));
        assert_eq!(
            nav(view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)),
            navigate("/srv/a/docs")
        );
        assert_eq!(view.reveal, None);
    }

    #[test]
    fn a_remote_root_has_no_folder_to_check_and_goes_at_once() {
        let (_, action, id) = remote_view_typed("/");
        assert_eq!(nav(action), navigate("/"));
        assert_eq!(id, None);
    }

    #[test]
    fn only_the_waiting_request_answers_and_leaving_the_folder_drops_it() {
        let (mut view, _, id) = remote_view_typed("/srv/a/notes.md");
        let id = id.unwrap();
        assert!(!view.answer_address_probe(id + 1, &Ok(Vec::new())));
        assert!(view.addr_probe.as_ref().unwrap().answer.is_none());

        assert!(view.answer_address_probe(id, &Ok(vec![entry("/srv/a", "notes.md", false)])));
        assert!(
            view.address_action(
                &egui::Context::default(),
                None,
                Path::new("/elsewhere"),
                true
            )
            .is_none()
        );
        assert!(view.addr_probe.is_none());
        assert_eq!(view.reveal, None);
    }

    #[test]
    fn a_new_input_replaces_the_waiting_check() {
        let (mut view, _, id) = remote_view_typed("/srv/a/notes.md");
        let action = view.address_action(
            &egui::Context::default(),
            Some("/srv/b/c.md"),
            Path::new("/srv"),
            true,
        );
        assert!(action.is_none());
        assert!(!view.answer_address_probe(id.unwrap(), &Ok(Vec::new())));
        assert_eq!(view.addr_probe.as_ref().unwrap().name, "c.md");
    }

    #[test]
    fn no_reply_within_the_list_timeout_goes_to_the_typed_path() {
        let (mut view, _, _) = remote_view_typed("/srv/a/notes.md");
        let probe = view.addr_probe.as_mut().unwrap();
        probe.sent_at = Instant::now()
            .checked_sub(LIST_DIR_SOFT_TIMEOUT + Duration::from_secs(1))
            .unwrap();
        assert_eq!(
            nav(view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)),
            navigate("/srv/a/notes.md")
        );
    }

    #[test]
    fn any_confirmed_input_drops_the_waiting_check_even_a_rejected_or_empty_one() {
        for input in ["~/x", "   "] {
            let (mut view, _, id) = remote_view_typed("/srv/a/notes.md");
            let action = view.address_action(
                &egui::Context::default(),
                Some(input),
                Path::new("/srv"),
                true,
            );
            assert!(
                view.addr_probe.is_none(),
                "{input:?} 가 기다리던 확인을 버려야 한다"
            );
            if input == "~/x" {
                assert!(matches!(action, Some(ExplorerAction::AddressRejected(_))));
            } else {
                assert!(action.is_none());
            }
            let reply = Ok(vec![entry("/srv/a", "notes.md", false)]);
            assert!(!view.answer_address_probe(id.unwrap(), &reply));
            assert!(
                view.address_action(&egui::Context::default(), None, Path::new("/srv"), true)
                    .is_none()
            );
            assert_eq!(view.reveal, None);
        }
    }

    #[test]
    fn a_waiting_check_wakes_itself_by_the_list_timeout() {
        let delays = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let ctx = egui::Context::default();
        let seen = delays.clone();
        ctx.set_request_repaint_callback(move |info| seen.lock().unwrap().push(info.delay));
        let mut view = ExplorerView::default();
        view.mirror_ws_id = Some(7);
        assert!(
            view.address_action(&ctx, Some("/srv/a/notes.md"), Path::new("/srv"), true)
                .is_none()
        );
        let first = delays.lock().unwrap().clone();
        assert!(
            first
                .iter()
                .any(|d| *d > Duration::ZERO && *d <= LIST_DIR_SOFT_TIMEOUT),
            "확인을 기다리는 동안 제한 시간 안의 다시 그리기를 예약해야 한다: {first:?}"
        );

        // 응답을 받았으면 더 기다릴 일이 없으므로 예약하지 않는다.
        delays.lock().unwrap().clear();
        let id = view.drain_outbox().pop().unwrap().request_id;
        assert!(view.answer_address_probe(id, &Ok(Vec::new())));
        assert!(
            view.address_action(&ctx, None, Path::new("/srv"), true)
                .is_some()
        );
        assert!(delays.lock().unwrap().iter().all(|d| *d == Duration::ZERO));
    }

    #[test]
    fn the_checked_folder_list_is_reused_when_moving_there() {
        let (mut view, _, id) = remote_view_typed("/srv/a/notes.md");
        let listed = Ok(vec![
            entry("/srv/a", "notes.md", false),
            entry("/srv/a", "other", true),
        ]);
        assert!(view.answer_address_probe(id.unwrap(), &listed));
        let to = nav(view.address_action(&egui::Context::default(), None, Path::new("/srv"), true));
        let panel = tasty_model::ExplorerPanel::new(1, to.unwrap());
        view.sync(&panel, Some(7));
        assert!(
            view.drain_outbox().is_empty(),
            "받은 목록을 다시 요청하지 않는다"
        );
        assert_eq!(view.state, super::super::super::view::LoadState::Ok);
        assert_eq!(view.take_reveal(), Some(PathBuf::from("/srv/a/notes.md")));
    }

    #[test]
    fn a_folder_answer_is_not_kept_as_a_listing() {
        let (mut view, _, id) = remote_view_typed("/srv/a/docs");
        assert!(view.answer_address_probe(id.unwrap(), &Ok(vec![entry("/srv/a", "docs", true)])));
        let panel = tasty_model::ExplorerPanel::new(1, PathBuf::from("/srv/a"));
        view.sync(&panel, Some(7));
        assert_eq!(
            view.drain_outbox().len(),
            1,
            "폴더로 판단한 확인 목록은 캐시로 쓰지 않는다"
        );
    }
}
