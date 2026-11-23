//! explorer 의 새 폴더·새 파일 명령. 컨텍스트 메뉴 행, 좁은 칸의 More 메뉴, 단축키가 같은 시작점을 쓴다.
//! More 메뉴는 툴바 view 묶음의 Find · Preview · 숨김 파일도 함께 보인다. 네이티브 메뉴에는 체크 표시가 없어
//! 이 행들은 지금 상태가 아니라 누르면 할 동작을 쓴다(Find ↔ Close find, Show ↔ Hide preview·hidden files).
//! 명령은 이름 입력을 열기만 한다. 실제 생성은 이름을 확정한 뒤 `ExplorerAction::Create` 로 한다.

use super::MainView;
use crate::platform::native_menu::MenuItem;
use crate::runtime::engine_read::EngineRead;

const NEW_FOLDER: u32 = 80;
const NEW_FILE: u32 = 81;
const FIND: u32 = 82;
const PREVIEW: u32 = 83;
const HIDDEN: u32 = 84;

impl MainView {
    /// 생성 행 두 개. 빈 영역 메뉴는 맨 앞 묶음이라 뒤에, 폴더 메뉴는 파일 조작 묶음이라 앞에 구분선을 둔다.
    pub(super) fn push_explorer_create_items(
        items: &mut Vec<MenuItem>,
        leading: bool,
        refusal: Option<&str>,
    ) {
        if !leading {
            items.push(MenuItem::separator());
        }
        for (id, key) in [
            (NEW_FOLDER, "explorer.command.new_folder"),
            (NEW_FILE, "explorer.command.new_file"),
        ] {
            items.push(match refusal {
                None => MenuItem::new(id, crate::i18n::t(key)),
                Some(reason) => MenuItem::disabled(id, crate::i18n::t(key)).with_tooltip(reason),
            });
        }
        if leading {
            items.push(MenuItem::separator());
        }
    }

    /// 컨텍스트 메뉴의 생성 행. 폴더 행이면 그 폴더 안에, 빈 영역이면 현재 폴더에 만든다.
    pub(super) fn explorer_menu_create(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        id: u32,
        paths: &[std::path::PathBuf],
        cwd: &std::path::Path,
    ) {
        let folder = match id {
            NEW_FOLDER => true,
            NEW_FILE => false,
            _ => return,
        };
        let dir = paths.first().cloned().unwrap_or_else(|| cwd.to_path_buf());
        self.state
            .start_explorer_create(engine, surface_id, Some(dir), folder);
    }

    /// 좁은 칸의 More 메뉴. 툴바에서 접은 명령을 같은 순서로 보인다.
    pub(super) fn handle_explorer_more_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        x: f32,
        y: f32,
    ) {
        let mut items = Vec::new();
        // 쓸 수 없는 원격 칸은 행을 꺼 두고 이유를 툴팁으로 단다.
        let refusal = engine.explorer_write_refusal(surface_id);
        let writable = self
            .state
            .explorer_views
            .get(surface_id)
            .is_none_or(|v| v.can_write_here());
        for (id, key) in [
            (NEW_FOLDER, "explorer.command.new_folder"),
            (NEW_FILE, "explorer.command.new_file"),
        ] {
            let label = crate::i18n::t(key);
            items.push(match &refusal {
                Some(reason) => MenuItem::disabled(id, label).with_tooltip(reason.as_str()),
                None if writable => MenuItem::new(id, label),
                None => MenuItem::disabled(id, label),
            });
        }
        if !items.is_empty() {
            items.push(MenuItem::separator());
        }
        let view = self.state.explorer_views.get(surface_id);
        let (find_open, preview_open, hidden_shown) = view.map_or((false, false, false), |v| {
            (v.find.is_some(), v.preview.open, v.hidden.show)
        });
        items.push(MenuItem::new(FIND, crate::i18n::t(find_row_key(find_open))));
        items.push(MenuItem::new(
            PREVIEW,
            crate::i18n::t(preview_row_key(preview_open)),
        ));
        items.push(MenuItem::new(
            HIDDEN,
            crate::i18n::t(crate::explorer_ui::view::hidden::action_key(hidden_shown)),
        ));
        self.open_native_menu(
            engine,
            x,
            y,
            &items,
            move |this, engine, result| match result {
                Some(id @ (NEW_FOLDER | NEW_FILE)) => {
                    this.state
                        .start_explorer_create(engine, surface_id, None, id == NEW_FOLDER);
                }
                Some(FIND) => {
                    if let Some(view) = this.state.explorer_views.get_mut(surface_id) {
                        view.toggle_find();
                    }
                }
                Some(PREVIEW) => {
                    if let Some(view) = this.state.explorer_views.get_mut(surface_id) {
                        view.preview.toggle();
                    }
                }
                Some(HIDDEN) => {
                    if let Some(view) = this.state.explorer_views.get_mut(surface_id) {
                        view.toggle_hidden();
                    }
                }
                _ => {}
            },
        );
    }
}

/// More 메뉴 Find 행의 문구 키. 바가 열려 있으면 닫는 동작을 쓴다.
fn find_row_key(open: bool) -> &'static str {
    if open {
        "explorer.more.find_close"
    } else {
        "explorer.command.find"
    }
}

/// More 메뉴 Preview 행의 문구 키.
fn preview_row_key(open: bool) -> &'static str {
    if open {
        "explorer.more.preview_hide"
    } else {
        "explorer.more.preview_show"
    }
}

impl crate::state::MainViewState {
    /// 탐색기의 Find 바를 열고 입력에 포커스를 준다.
    pub(crate) fn open_explorer_find(&mut self, surface_id: u32) {
        if let Some(view) = self.explorer_views.get_mut(surface_id) {
            view.open_find();
        }
    }

    /// 이름 입력을 연다. `dir` 이 없으면 지금 보는 폴더다.
    /// 원격 explorer 와 쓸 수 없다고 확인한 폴더는 입력을 열지 않고 이유를 알린다.
    pub(crate) fn start_explorer_create(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        dir: Option<std::path::PathBuf>,
        folder: bool,
    ) {
        if !engine.has_surface(surface_id) {
            return;
        }
        if self.refuse_remote_write(engine, surface_id) {
            return;
        }
        let Some(view) = self.explorer_views.get_mut(surface_id) else {
            return;
        };
        let Some(dir) = dir.or_else(|| view.shown_dir().map(std::path::Path::to_path_buf)) else {
            return;
        };
        if view.shown_dir() == Some(dir.as_path()) && !view.can_write_here() {
            self.toasts.push(
                crate::i18n::t("explorer.command.cannot_write").to_string(),
                crate::adapters::ui::ToastKind::Info,
                crate::adapters::ui::ToastScope::Surface(surface_id),
            );
            return;
        }
        view.request_create(dir, folder);
    }

    /// 대상 폴더에 쓸 수 없다고 확인되어 열지 않은 이름 입력을 토스트로 알린다. 알렸으면 true.
    pub(crate) fn report_explorer_create_refusals(&mut self) -> bool {
        let surfaces: Vec<_> = self.explorer_views.cached_surfaces().collect();
        let mut reported = false;
        for surface_id in surfaces {
            let refused = self
                .explorer_views
                .get_mut(surface_id)
                .is_some_and(|view| view.take_create_refused());
            if refused {
                self.toasts.push(
                    crate::i18n::t("explorer.command.cannot_write").to_string(),
                    crate::adapters::ui::ToastKind::Info,
                    crate::adapters::ui::ToastScope::Surface(surface_id),
                );
                reported = true;
            }
        }
        reported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_rows_name_the_action_they_take() {
        crate::i18n::init("en");
        assert_eq!(crate::i18n::t(find_row_key(false)), "Find");
        assert_eq!(crate::i18n::t(find_row_key(true)), "Close find");
        assert_eq!(crate::i18n::t(preview_row_key(false)), "Show preview");
        assert_eq!(crate::i18n::t(preview_row_key(true)), "Hide preview");
        assert_eq!(
            crate::i18n::t(crate::explorer_ui::view::hidden::action_key(false)),
            "Show hidden files"
        );
        assert_eq!(
            crate::i18n::t(crate::explorer_ui::view::hidden::action_key(true)),
            "Hide hidden files"
        );
    }

    /// 폴더 행 메뉴의 대상 폴더는 쓰기 가능 확인을 받은 뒤에 입력을 연다.
    #[cfg(unix)]
    #[test]
    fn a_folder_row_that_cannot_be_written_is_refused_before_the_input_opens() {
        use std::os::unix::fs::PermissionsExt;
        crate::i18n::init("en");
        let (mut state, engine) = crate::state::tests::test_state();
        let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        let open = dir.path().join("open");
        std::fs::create_dir(&locked).unwrap();
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let panel = crate::model::ExplorerPanel::new(sid, dir.path().into());
        state.explorer_views.get_or_init(&panel, None);
        let mut owner = crate::app::local_reads::LocalReads::default();
        let mut settle = |state: &mut crate::state::MainViewState| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            // holds_by 로 바꾸지 않는다. drive 는 새 읽기 worker 를 제출하는 진행이라 기한 뒤 관측에서
            // 부를 수 없고, 그것 없이는 끝난 결과가 view 에 들어오지 않는다.
            loop {
                owner.reap();
                owner.drive(|requests| {
                    state.explorer_views.poll_local_reads(requests);
                    true
                });
                let view = state.explorer_views.get(sid).unwrap();
                if !view.create_pending() {
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "check never finished");
                std::thread::yield_now();
            }
            state.report_explorer_create_refusals();
        };

        state.start_explorer_create(&engine.read(), sid, Some(locked.clone()), true);
        assert!(state.explorer_views.get(sid).unwrap().create.is_none());
        settle(&mut state);
        assert!(state.explorer_views.get(sid).unwrap().create.is_none());
        assert_eq!(
            state.toasts.messages(),
            vec![crate::i18n::t("explorer.command.cannot_write")]
        );

        state.start_explorer_create(&engine.read(), sid, Some(open.clone()), false);
        settle(&mut state);
        let view = state.explorer_views.get(sid).unwrap();
        assert_eq!(
            view.create.as_ref().map(|c| c.dir.clone()),
            Some(open.clone())
        );
        assert_eq!(state.toasts.messages().len(), 1);

        // 결과가 오기 전에 다른 폴더로 가면 입력을 열지 않는다.
        state.start_explorer_create(&engine.read(), sid, Some(open.clone()), true);
        let moved = crate::model::ExplorerPanel::new(sid, locked.clone());
        state.explorer_views.get_or_init(&moved, None);
        settle(&mut state);
        assert!(state.explorer_views.get(sid).unwrap().create.is_none());
        assert_eq!(state.toasts.messages().len(), 1);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
