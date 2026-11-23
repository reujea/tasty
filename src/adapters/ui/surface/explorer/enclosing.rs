//! "들어 있는 폴더에서 보기". 하위 폴더 검색 결과와 링크에서 항목이 실제로 든 폴더를 열고,
//! 그 목록이 들어오면 항목을 고르고 보이는 곳으로 스크롤한다. 깜박임 같은 강조는 없다.

use std::path::{Component, Path, PathBuf};

use super::view::{ExplorerView, LoadState};

/// 컨텍스트 메뉴의 들어 있는 폴더에서 보기 행 상태.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EnclosingRow {
    /// 행을 켠다: (열 폴더, 고를 항목).
    Target(PathBuf, PathBuf),
    /// 링크의 대상이 사라졌다. 대상이 다시 생기면 쓸 수 있으므로 행을 끈 채 둔다. 값은 없는 대상 경로다.
    TargetGone(PathBuf),
    /// 행이 없다: 링크도 검색 결과도 아니거나, 대상을 풀 수 없는 링크(권한·순환·부모 없는 대상).
    None,
}

impl ExplorerView {
    /// 지금 목록이 하위 폴더 검색 결과인가. 컨텍스트 메뉴가 결과에 열기 행을 붙일 때 쓴다.
    pub(crate) fn is_search_result(&self) -> bool {
        self.search_root().is_some()
    }

    /// 메뉴 행의 대상: (열 폴더, 고를 항목). 링크는 대상이 든 폴더를, 하위 폴더 검색 결과는 결과가 든
    /// 폴더를 연다. 그 밖의 항목과 대상이 없는 링크는 `None` 이다.
    pub(crate) fn enclosing_target(&self, path: &Path) -> Option<(PathBuf, PathBuf)> {
        match self.enclosing_row(path) {
            EnclosingRow::Target(folder, item) => Some((folder, item)),
            EnclosingRow::TargetGone(_) | EnclosingRow::None => None,
        }
    }

    /// 들어 있는 폴더에서 보기 행의 상태. 대상이 사라진 링크(`metadata` 가 `NotFound`)만 꺼진 행이다.
    pub(crate) fn enclosing_row(&self, path: &Path) -> EnclosingRow {
        let is_link = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
        let item = if is_link {
            match std::fs::metadata(path) {
                Ok(_) => link_target(path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let target = link_target(path)
                        .or_else(|| std::fs::read_link(path).ok())
                        .unwrap_or_else(|| path.to_path_buf());
                    return EnclosingRow::TargetGone(target);
                }
                Err(_) => None,
            }
        } else if self.search_root().is_some() {
            Some(path.to_path_buf())
        } else {
            None
        };
        match item.and_then(|item| Some((item.parent()?.to_path_buf(), item))) {
            Some((folder, item)) => EnclosingRow::Target(folder, item),
            None => EnclosingRow::None,
        }
    }

    /// 폴더 `dir` 로 옮길 때 부른다. Find 를 닫아 그 폴더의 전체 목록을 보이고, 목록에 `item` 이 나타나면
    /// 고른다. 고르기 전에 그 폴더에 도착했다가 떠나거나 그 폴더를 읽지 못하면 버린다(`settle_reveal`).
    pub(crate) fn reveal_after_load(&mut self, dir: PathBuf, item: PathBuf) {
        self.find = None;
        self.reveal = Some(item);
        self.reveal_select = true;
        self.reveal_in = Some((dir, false));
    }

    /// 매 동기화 첫머리에 부른다. 고를 항목이 기다리는 폴더에 도착했는지 기록하고, 도착한 뒤 다른 폴더로
    /// 갔거나(뒤로·위로·탭 전환 포함) 그 폴더 목록이 오류로 끝났으면 기다리던 고르기를 버린다.
    /// 상태는 지난 동기화의 것이므로 지금 보이는 폴더가 그 폴더일 때만 오류를 그 폴더의 것으로 본다.
    pub(super) fn settle_reveal(&mut self, root: &Path) {
        let shown_failed = self.shown_dir() == Some(root)
            && matches!(self.state, LoadState::Error(_) | LoadState::NoPermission);
        let Some((dir, arrived)) = self.reveal_in.as_mut() else {
            return;
        };
        let here = dir.as_path() == root;
        let left = *arrived && !here;
        *arrived |= here;
        let failed = here && shown_failed;
        if left || failed {
            self.forget_reveal();
        }
    }

    /// 기다리던 고르기를 버린다. 이미 고른 항목은 그대로 둔다.
    pub(super) fn forget_reveal(&mut self) {
        if self.reveal_in.take().is_some() {
            self.reveal = None;
            self.reveal_select = false;
        }
    }
}

/// 링크 자신만 한 번 푼 대상 경로. 상대 대상은 링크가 든 폴더에 이어 붙이고 `.`·`..` 만 글자로
/// 정리한다. 상위 경로의 링크는 풀지 않아 사용자가 보던 경로 체계를 지킨다. 대상이 다시 링크면
/// 그 링크가 대상이다(체인을 끝까지 따라가지 않는다).
fn link_target(link: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(link).ok()?;
    let joined = link.parent()?.join(target);
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fs_list::DirEntryInfo;

    fn listed(view: &mut ExplorerView, dir: &Path, names: &[&str]) {
        view.entries = names
            .iter()
            .map(|n| DirEntryInfo {
                path: dir.join(n),
                name: (*n).into(),
                is_dir: false,
                size: 0,
                modified: None,
                ext: String::new(),
                ..Default::default()
            })
            .collect();
    }

    #[test]
    fn only_links_and_search_results_have_an_enclosing_folder() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real = tmp.path().join("real");
        std::fs::create_dir(&real).expect("mkdir");
        let target = real.join("notes.md");
        std::fs::write(&target, b"n").expect("write");
        let plain = tmp.path().join("plain.txt");
        std::fs::write(&plain, b"p").expect("write");
        let view = ExplorerView::default();
        assert_eq!(view.enclosing_target(&plain), None);
        #[cfg(unix)]
        {
            let broken = tmp.path().join("broken.md");
            std::os::unix::fs::symlink(tmp.path().join("gone"), &broken).expect("symlink");
            assert_eq!(view.enclosing_target(&broken), None);
            assert_eq!(
                view.enclosing_row(&broken),
                EnclosingRow::TargetGone(tmp.path().join("gone"))
            );
        }
        assert_eq!(view.enclosing_row(&plain), EnclosingRow::None);
    }

    /// 대상을 풀 수 없지만 사라진 것은 아닌 링크(순환, 부모 없는 대상)는 행이 없다.
    #[cfg(unix)]
    #[test]
    fn only_a_link_whose_target_is_gone_gets_a_disabled_row() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let view = ExplorerView::default();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        std::os::unix::fs::symlink(&b, &a).expect("symlink");
        std::os::unix::fs::symlink(&a, &b).expect("symlink");
        assert_eq!(view.enclosing_row(&a), EnclosingRow::None, "a loop");
        let root = tmp.path().join("root");
        std::os::unix::fs::symlink("/", &root).expect("symlink");
        assert_eq!(view.enclosing_row(&root), EnclosingRow::None, "no parent");
    }

    /// 링크를 만든 뒤 `enclosing_target` 이 돌려준 (폴더, 항목).
    #[cfg(unix)]
    fn target_of(link: &Path, to: impl AsRef<Path>) -> Option<(PathBuf, PathBuf)> {
        std::os::unix::fs::symlink(to, link).expect("symlink");
        ExplorerView::default().enclosing_target(link)
    }

    #[cfg(unix)]
    #[test]
    fn a_link_opens_its_target_folder_without_resolving_the_path_above() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let real = tmp.path().join("store");
        std::fs::create_dir_all(real.join("real/deep")).expect("mkdir");
        std::fs::write(real.join("real/notes.md"), b"n").expect("write");
        // 사용자는 링크된 폴더 `proj`(→ store) 안에서 본다. 결과는 `proj` 아래 경로여야 한다.
        let proj = tmp.path().join("proj");
        std::os::unix::fs::symlink(&real, &proj).expect("symlink");
        let seen = proj.join("real");

        let rel = target_of(&proj.join("rel.md"), "real/./deep/../notes.md");
        assert_eq!(rel, Some((seen.clone(), seen.join("notes.md"))));

        let up = target_of(&seen.join("deep/up.md"), "../notes.md");
        assert_eq!(up, Some((seen.clone(), seen.join("notes.md"))));

        let abs = target_of(&proj.join("abs.md"), proj.join("real/notes.md"));
        assert_eq!(abs, Some((seen.clone(), seen.join("notes.md"))));

        // 체인은 한 단계만 푼다: chain → rel.md(링크) 이면 rel.md 가 든 폴더에서 rel.md 를 고른다.
        let chain = target_of(&seen.join("chain.md"), "../rel.md");
        assert_eq!(chain, Some((proj.clone(), proj.join("rel.md"))));
    }

    #[test]
    fn the_item_is_selected_once_the_folder_lists_it() {
        let dir = Path::new("/srv/real");
        let item = dir.join("notes.md");
        let mut view = ExplorerView::default();
        view.reveal_after_load(dir.to_path_buf(), item.clone());
        assert_eq!(view.take_reveal(), None, "not listed yet");
        assert!(view.selected.is_empty());
        listed(&mut view, dir, &["a.txt", "notes.md"]);
        assert_eq!(view.take_reveal(), Some(item.clone()));
        assert_eq!(view.selected.len(), 1);
        assert!(view.selected.contains(&item));
        assert_eq!(view.take_reveal(), None, "revealed once");
    }

    /// mirror view 가 `dir` 을 보도록 동기화한다. 요청이 나가면 그 ID 를 돌려준다.
    fn show_remote(
        view: &mut ExplorerView,
        panel: &mut tasty_model::ExplorerPanel,
        dir: &str,
    ) -> Option<u64> {
        panel.active_tab_mut().navigate_to(PathBuf::from(dir));
        view.sync(panel, Some(7));
        view.drain_outbox().pop().map(|r| r.request_id)
    }

    #[test]
    fn a_waiting_pick_survives_the_move_there_and_is_dropped_once_left() {
        let mut view = ExplorerView::default();
        let mut panel = tasty_model::ExplorerPanel::new(1, PathBuf::from("/srv"));
        show_remote(&mut view, &mut panel, "/srv");
        view.reveal_after_load("/srv/a".into(), "/srv/a/n.md".into());
        // 아직 옮기기 전(이동 요청을 낸 프레임)에는 버리지 않는다.
        show_remote(&mut view, &mut panel, "/srv");
        assert!(view.reveal.is_some());
        let id = show_remote(&mut view, &mut panel, "/srv/a").expect("요청");
        assert!(
            view.reveal.is_some(),
            "도착만으로 기다리던 고르기를 지우면 안 된다"
        );
        // 목록이 오기 전에 떠나면(뒤로·위로·탭 전환) 버린다.
        show_remote(&mut view, &mut panel, "/srv");
        assert_eq!(view.reveal, None);
        assert!(!view.reveal_select);
        // 다시 들어가 목록에 항목이 있어도 고르지 않는다.
        // 떠나기 전에 보낸 요청의 응답이 다시 들어간 뒤에 온다.
        show_remote(&mut view, &mut panel, "/srv/a");
        let listed = Ok(vec![DirEntryInfo {
            path: "/srv/a/n.md".into(),
            name: "n.md".into(),
            ..Default::default()
        }]);
        assert!(view.apply_remote_list_dir_result(id, &panel, listed));
        assert_eq!(view.take_reveal(), None);
        assert!(view.selected.is_empty());
    }

    #[test]
    fn a_waiting_pick_is_dropped_when_its_folder_fails_to_list() {
        let mut view = ExplorerView::default();
        let mut panel = tasty_model::ExplorerPanel::new(1, PathBuf::from("/srv"));
        let id = show_remote(&mut view, &mut panel, "/srv/locked").expect("요청");
        view.reveal_after_load("/srv/locked".into(), "/srv/locked/f.md".into());
        assert!(view.apply_remote_list_dir_result(id, &panel, Err("permission denied".into())));
        show_remote(&mut view, &mut panel, "/srv/locked");
        assert_eq!(view.reveal, None, "읽지 못한 폴더의 고르기는 버린다");
    }

    #[test]
    fn a_failure_of_the_folder_left_behind_does_not_drop_the_pick() {
        let mut view = ExplorerView::default();
        let mut panel = tasty_model::ExplorerPanel::new(1, PathBuf::from("/srv"));
        let id = show_remote(&mut view, &mut panel, "/srv/locked").expect("요청");
        assert!(view.apply_remote_list_dir_result(id, &panel, Err("permission denied".into())));
        view.reveal_after_load("/srv/a".into(), "/srv/a/n.md".into());
        show_remote(&mut view, &mut panel, "/srv/locked");
        show_remote(&mut view, &mut panel, "/srv/a");
        assert!(view.reveal.is_some());
    }
}
