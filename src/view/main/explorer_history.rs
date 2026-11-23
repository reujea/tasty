//! explorer 빈 영역 메뉴의 Undo · Redo 행. 칸의 이력에서 맨 위 단계만 보인다.
//! 비활성 이유는 툴팁에, 항목 툴팁이 없는 Windows에서는 행 이름에 보인다.

use super::MainView;
use crate::app::explorer_files::history::{Entry, UndoOf};
use crate::platform::native_menu::MenuItem;
use crate::runtime::engine_read::EngineRead;
use crate::state::explorer_history::{redo_stale_text, stale_text};

const UNDO: u32 = 110;
const REDO: u32 = 111;

/// 메뉴 행의 작업 이름(예: "move 3 items", "rename"). 이름 변경·새 항목은 항목이 하나라 수를 적지 않는다.
fn op_label(entry: &Entry) -> String {
    let key = match entry.undo_of() {
        UndoOf::Move => "explorer.menu.op_move",
        UndoOf::Copy => "explorer.menu.op_copy",
        UndoOf::Rename => return crate::i18n::t("explorer.menu.op_rename").to_owned(),
        UndoOf::NewFolder => return crate::i18n::t("explorer.menu.op_new_folder").to_owned(),
        UndoOf::NewFile => return crate::i18n::t("explorer.menu.op_new_file").to_owned(),
    };
    let n = entry.count();
    crate::i18n::t_count(key, n as u64, &[&n.to_string()])
}

pub(crate) fn undo_label(entry: &Entry) -> String {
    crate::i18n::t_fmt("explorer.menu.undo", &op_label(entry))
}

pub(crate) fn redo_label(entry: &Entry) -> String {
    crate::i18n::t_fmt("explorer.menu.redo", &op_label(entry))
}

/// Undo · Redo는 이력이 없어도 두 자리를 유지한다.
pub(crate) fn history_items(
    undo: Option<&Entry>,
    redo: Option<&Entry>,
    busy: bool,
) -> Vec<MenuItem> {
    history_items_for_platform(undo, redo, busy, cfg!(windows))
}

fn history_items_for_platform(
    undo: Option<&Entry>,
    redo: Option<&Entry>,
    busy: bool,
    reason_in_label: bool,
) -> Vec<MenuItem> {
    [(UNDO, undo), (REDO, redo)]
        .into_iter()
        .map(|(id, entry)| {
            let Some(entry) = entry else {
                return MenuItem::disabled(
                    id,
                    crate::i18n::t(if id == UNDO {
                        "explorer.menu.undo_none"
                    } else {
                        "explorer.menu.redo_none"
                    }),
                );
            };
            let label = if id == UNDO {
                undo_label(entry)
            } else {
                redo_label(entry)
            };
            let why = if busy {
                Some(crate::i18n::t("explorer.menu.history_busy").to_owned())
            } else if id == UNDO {
                stale_text(entry)
            } else {
                redo_stale_text(entry)
            };
            match why {
                Some(why) if reason_in_label => MenuItem::disabled(id, why),
                Some(why) => MenuItem::disabled(id, label).with_tooltip(why),
                None => MenuItem::new(id, label),
            }
        })
        .collect()
}

/// 원격 칸의 Undo · Redo. 원격 작업은 이력에 쌓지 않으므로 이력이 없는 맨 이름으로 끄고 그 이유를 단다.
fn remote_history_items() -> Vec<MenuItem> {
    let why = crate::i18n::t("explorer.menu.undo_remote_reason");
    vec![
        MenuItem::disabled(UNDO, crate::i18n::t("explorer.menu.undo_none")).with_tooltip(why),
        MenuItem::disabled(REDO, crate::i18n::t("explorer.menu.redo_none")).with_tooltip(why),
    ]
}

/// 칸 이력으로 메뉴 행을 만들고, 고른 행이 이 이력을 실행하도록 지금 이력을 적어 둔다.
pub(super) fn history_rows(state: &mut crate::state::MainViewState, sid: u32) -> Vec<MenuItem> {
    let busy = state.explorer_history_busy(sid);
    let Some(history) = state
        .explorer_views
        .get_mut(sid)
        .map(|v| &mut v.ops.history)
    else {
        return history_items(None, None, busy);
    };
    history.show();
    history_items(history.peek_undo(), history.peek_redo(), busy)
}

/// 메뉴는 늘 "구분선 · Properties" 로 끝난다. 그 구분선 앞에 구분선과 행 묶음을 넣어
/// 묶음이 앞뒤 구분선 사이에 놓이게 한다.
fn insert_rows(items: &mut Vec<MenuItem>, rows: Vec<MenuItem>) {
    if rows.is_empty() {
        return;
    }
    let at = items.len().saturating_sub(2);
    items.splice(at..at, std::iter::once(MenuItem::separator()).chain(rows));
}

impl MainView {
    /// 빈 영역 메뉴의 Properties 묶음 앞에 Undo · Redo 묶음을 넣는다. mirror explorer 는 이력이 없어
    /// 두 행을 이유와 함께 끈다.
    pub(super) fn push_explorer_history_items(
        &mut self,
        surface_id: u32,
        is_mirror: bool,
        items: &mut Vec<MenuItem>,
    ) {
        let rows = if is_mirror {
            remote_history_items()
        } else {
            history_rows(&mut self.state, surface_id)
        };
        insert_rows(items, rows);
    }

    /// 생성 행과 이력 행을 처리한다. 둘 다 아니면 아무것도 하지 않는다.
    pub(super) fn explorer_menu_other(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        id: u32,
        paths: &[std::path::PathBuf],
        cwd: &std::path::Path,
    ) {
        match id {
            UNDO | REDO => {
                let origin = crate::intent::IntentOrigin::User {
                    source: crate::intent::UserSource::ContextMenu,
                };
                self.state
                    .explorer_history_step(engine, surface_id, id == REDO, origin, true);
            }
            _ => self.explorer_menu_create(engine, surface_id, id, paths, cwd),
        }
    }
}

#[cfg(test)]
#[path = "explorer_history_tests.rs"]
mod tests;
