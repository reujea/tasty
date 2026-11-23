//! explorer 컨텍스트 메뉴의 "들어 있는 폴더에서 보기" 행. 하위 폴더 검색 결과나 링크 하나를 눌렀을 때
//! 메뉴 맨 앞에 둔다. 고르면 항목이 실제로 든 폴더로 옮기고 목록이 들어오면 그 항목을 고른다.
//! 대상이 사라진 링크에는 행을 끈 채 둔다(대상이 다시 생기면 쓸 수 있는 상태 조건이다).
//! 검색 결과에는 그 앞에 "열기" 행을 더 둔다. 고르면 더블클릭처럼 폴더는 들어가고 파일은 연다.
//! 컨텍스트 메뉴의 생성 행처럼 메뉴 구성(`redraw.rs`)과 따로 둔다.

use super::MainView;
use crate::platform::native_menu::MenuItem;
use crate::runtime::engine_read::EngineRead;

const SHOW_IN_FOLDER: u32 = 90;
const OPEN_RESULT: u32 = 91;

/// 들어 있는 폴더에서 보기 행의 모양.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShowInFolder {
    /// 행이 없다.
    Absent,
    /// 켠 행. 검색 결과면 그 앞에 열기 행을 둔다.
    Enabled { open: bool },
    /// 대상이 사라진 링크. 이유 없이 꺼진 행만 두고 열기 행은 두지 않는다.
    Disabled,
}

/// 메뉴 맨 앞에 붙일 행. 시안 검색 결과 메뉴처럼 열기 → 들어 있는 폴더에서 보기 → 구분선 순서다.
fn enclosing_head(row: ShowInFolder) -> Vec<MenuItem> {
    let label = crate::i18n::t("explorer.menu.show_in_folder");
    let mut head = Vec::with_capacity(3);
    match row {
        ShowInFolder::Absent => return head,
        ShowInFolder::Enabled { open } => {
            if open {
                head.push(MenuItem::new(OPEN_RESULT, crate::i18n::t("button.open")));
            }
            head.push(MenuItem::new(SHOW_IN_FOLDER, label));
        }
        ShowInFolder::Disabled => head.push(MenuItem::disabled(SHOW_IN_FOLDER, label)),
    }
    head.push(MenuItem::separator());
    head
}

/// 로컬 explorer 에서 대상 하나의 행 모양. 원격 칸과 대상이 여럿이면 행이 없다.
fn show_in_folder_row(
    view: Option<&crate::adapters::ui::surface::explorer::view::ExplorerView>,
    paths: &[std::path::PathBuf],
) -> ShowInFolder {
    match (paths, view) {
        ([path], Some(v)) if !v.is_remote() => {
            row_shape(v.enclosing_row(path), v.is_search_result())
        }
        _ => ShowInFolder::Absent,
    }
}

fn row_shape(
    row: crate::adapters::ui::surface::explorer::EnclosingRow,
    search_result: bool,
) -> ShowInFolder {
    use crate::adapters::ui::surface::explorer::EnclosingRow;
    match row {
        EnclosingRow::Target(..) => ShowInFolder::Enabled {
            open: search_result,
        },
        EnclosingRow::TargetGone => ShowInFolder::Disabled,
        EnclosingRow::None => ShowInFolder::Absent,
    }
}

impl MainView {
    /// 대상이 하나이고 들어 있는 폴더가 있으면 메뉴 맨 앞에 행과 구분선을 둔다.
    /// 검색 결과면 그 앞에 열기 행도 둔다. 대상이 사라진 링크면 꺼진 행과 구분선만 둔다.
    pub(super) fn with_show_in_folder(
        &self,
        surface_id: u32,
        paths: &[std::path::PathBuf],
        mut items: Vec<MenuItem>,
    ) -> Vec<MenuItem> {
        let row = show_in_folder_row(self.state.explorer_views.get(surface_id), paths);
        items.splice(0..0, enclosing_head(row));
        items
    }

    /// 이 칸에 붙여넣을 수 있는 목록이 있는가. 다른 컴퓨터의 경로는 붙여넣지 않는다.
    pub(super) fn explorer_menu_has_clip(&self, engine: &EngineRead<'_>, surface_id: u32) -> bool {
        crate::state::ExplorerPathSource::of_surface(engine, surface_id)
            .is_some_and(|target| self.state.explorer_can_paste_into(target))
    }

    /// 다른 행이 처리하지 않은 메뉴 결과. 들어 있는 폴더에서 보기가 아니면 생성 행이나 이력 행이다.
    pub(super) fn explorer_menu_more(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        id: u32,
        paths: &[std::path::PathBuf],
        cwd: &std::path::Path,
    ) {
        if id == OPEN_RESULT {
            return self.explorer_menu_open_result(engine, surface_id, paths);
        }
        if id != SHOW_IN_FOLDER {
            return self.explorer_menu_other(engine, surface_id, id, paths, cwd);
        }
        let Some((folder, item)) = paths.first().and_then(|p| {
            self.state
                .explorer_views
                .get(surface_id)
                .and_then(|v| v.enclosing_target(p))
        }) else {
            return;
        };
        let Some(target) =
            crate::runtime::surface_binding::SurfaceBinding::capture(engine, surface_id)
        else {
            return;
        };
        // 툴바·주소창 이동과 같은 경로라 히스토리(뒤로)에 남는다.
        self.state.dispatch_intent(
            crate::intent::Intent::Engine(crate::app::engine_action::EngineAction::Explorer(
                crate::app::requests::explorer::BoundExplorerCommand::new(
                    target,
                    crate::app::requests::explorer::ExplorerCommand::Navigate(folder.clone()),
                ),
            ))
            .from_user_context_menu(),
        );
        if let Some(view) = self.state.explorer_views.get_mut(surface_id) {
            view.reveal_after_load(folder, item);
        }
    }

    /// 검색 결과 하나를 연다.
    fn explorer_menu_open_result(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        paths: &[std::path::PathBuf],
    ) {
        if let [path] = paths {
            open_result(&mut self.state, engine, surface_id, path);
        }
    }
}

/// 더블클릭과 같은 액션을 같은 호스트 경로로 보낸다. 폴더는 그 폴더로 들어가고 파일은 연다.
/// 파일 열기는 엔진이 아니라 호스트가 처리하므로 엔진 액션으로 직접 보내면 아무 일도 일어나지 않는다.
fn open_result(
    state: &mut crate::state::MainViewState,
    engine: &EngineRead<'_>,
    surface_id: u32,
    path: &std::path::Path,
) {
    let action = if path.is_dir() {
        crate::explorer_ui::ExplorerAction::Navigate(path.to_path_buf())
    } else {
        crate::explorer_ui::ExplorerAction::OpenFile(path.to_path_buf())
    };
    crate::adapters::ui::apply_explorer_action(state, engine, surface_id, action);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (id, 활성). 구분선은 None.
    fn ids(items: &[MenuItem]) -> Vec<Option<(u32, bool)>> {
        items
            .iter()
            .map(|i| (!i.is_separator()).then_some((i.id, i.enabled)))
            .collect()
    }

    #[test]
    fn a_search_result_opens_first_then_shows_its_folder() {
        assert_eq!(
            ids(&enclosing_head(ShowInFolder::Enabled { open: true })),
            vec![
                Some((OPEN_RESULT, true)),
                Some((SHOW_IN_FOLDER, true)),
                None
            ]
        );
    }

    #[test]
    fn a_link_only_gets_the_folder_row() {
        assert_eq!(
            ids(&enclosing_head(ShowInFolder::Enabled { open: false })),
            vec![Some((SHOW_IN_FOLDER, true)), None]
        );
    }

    #[test]
    fn a_link_whose_target_is_gone_keeps_a_disabled_row_without_open() {
        assert_eq!(
            ids(&enclosing_head(ShowInFolder::Disabled)),
            vec![Some((SHOW_IN_FOLDER, false)), None]
        );
    }

    /// 메뉴 조립 경로: 일반 목록·검색 결과에서 깨진 링크, 살아 있는 링크, 일반 파일, 원격 칸.
    #[cfg(unix)]
    #[test]
    fn the_row_follows_the_link_target_and_the_listing() {
        use crate::adapters::ui::surface::explorer::view::ExplorerView;
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("plain.md");
        std::fs::write(&file, b"p").expect("write");
        let live = tmp.path().join("live.md");
        std::os::unix::fs::symlink(&file, &live).expect("symlink");
        let broken = tmp.path().join("broken.md");
        std::os::unix::fs::symlink(tmp.path().join("gone"), &broken).expect("symlink");

        let listing = ExplorerView::default();
        let row =
            |v: &ExplorerView, p: &std::path::Path| show_in_folder_row(Some(v), &[p.to_path_buf()]);
        assert_eq!(row(&listing, &broken), ShowInFolder::Disabled);
        assert_eq!(row(&listing, &live), ShowInFolder::Enabled { open: false });
        assert_eq!(row(&listing, &file), ShowInFolder::Absent);
        assert_eq!(
            show_in_folder_row(Some(&listing), &[broken.clone(), file.clone()]),
            ShowInFolder::Absent,
            "several targets"
        );

        // 검색 결과: 대상이 사라진 링크는 열기 없이 꺼진 행, 들어 있는 폴더가 있으면 열기와 켠 행.
        use crate::adapters::ui::surface::explorer::EnclosingRow;
        assert_eq!(
            row_shape(EnclosingRow::TargetGone, true),
            ShowInFolder::Disabled
        );
        assert_eq!(
            row_shape(EnclosingRow::Target(tmp.path().into(), file.clone()), true),
            ShowInFolder::Enabled { open: true }
        );
        assert_eq!(row_shape(EnclosingRow::None, true), ShowInFolder::Absent);

        let mut remote = ExplorerView::default();
        let mut panel = tasty_model::ExplorerPanel::new(1, std::path::PathBuf::from("/srv"));
        panel.active_tab_mut().navigate_to(tmp.path().to_path_buf());
        remote.sync(&panel, Some(7));
        assert_eq!(row(&remote, &broken), ShowInFolder::Absent);
    }

    /// 파일 결과의 열기는 더블클릭처럼 파일 dispatch가 되고, 폴더 결과는 그 폴더로 들어간다.
    #[test]
    fn opening_a_result_dispatches_a_file_or_enters_a_folder() {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let sid = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().to_path_buf();
        let file = dir.join("note.md");
        std::fs::write(&file, "x").expect("write the file");

        open_result(&mut state, &engine.read(), sid, &file);
        let intents = state.take_pending_intents();
        assert_eq!(intents.len(), 1);
        let crate::intent::Intent::Domain(crate::app::command::DomainIntent::DispatchFile {
            target,
            origin_surface_id,
            ..
        }) = &intents[0].body
        else {
            panic!("expected DispatchFile, got {:?}", intents[0].body);
        };
        assert_eq!(target.as_path(), file.as_path());
        assert_eq!(*origin_surface_id, Some(sid));

        open_result(&mut state, &engine.read(), sid, &dir);
        let intents = state.take_pending_intents();
        assert_eq!(intents.len(), 1);
        let crate::intent::Intent::Engine(crate::app::engine_action::EngineAction::Explorer(
            request,
        )) = &intents[0].body
        else {
            panic!("expected Navigate, got {:?}", intents[0].body);
        };
        assert_eq!(
            request.command(),
            &crate::app::requests::explorer::ExplorerCommand::Navigate(dir)
        );
    }

    #[test]
    fn no_enclosing_folder_adds_nothing() {
        assert!(enclosing_head(ShowInFolder::Absent).is_empty());
    }
}
