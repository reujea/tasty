//! explorer 파일 작업의 이름 충돌 질문. 작업을 요청한 칸에 scrim 과 함께 뜬다.
//! 답은 작업 상태(`Shared`)에 바로 넘기고, 작업은 답을 받을 때까지 그 항목에서 기다린다.

use std::time::SystemTime;

use tasty_ui_widgets::{ConflictPick, ConflictProps, conflict_card};

use super::PopupAction;
use crate::app::explorer_files::job::{Answer, Ask, Choice, EntryFacts};
use crate::core::fs_list::human_size;
use crate::i18n::{t, t_count, t_fmt};
use crate::state::MainViewState;
use crate::theme;

pub(crate) const EXPLORER_CONFLICT_POPUP_ID: super::PopupId = "explorer_conflict";

/// 답을 기다리는 칸. 파일 작업은 앱 전체에서 하나만 돌므로 질문하는 칸도 하나다.
/// draw 동안에는 popup 관리자가 state 밖으로 옮겨져 있어 열린 범위를 읽을 수 없다.
fn asking_surface(state: &MainViewState) -> Option<u32> {
    state.explorer_views.cached_surfaces().find(|sid| {
        state
            .explorer_views
            .get(*sid)
            .and_then(|v| v.ops.running.as_ref())
            .is_some_and(|r| r.shared.pending_ask().is_some())
    })
}

/// 첫 프레임 크기. 높이는 다음 프레임부터 잰 카드 높이를 쓴다.
pub fn default_size() -> egui::Vec2 {
    let th = theme::theme();
    egui::vec2(
        th.explorer_conflict_width().value(),
        th.item_height_interactive.value(),
    )
}

/// PopupDef.sizer — 폭은 explorer-conflict-width, 높이는 직전 프레임에 잰 카드 높이다.
pub fn sizer(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _payload: &crate::adapters::ui::popup::PopupPayload,
) -> egui::Vec2 {
    let measured = asking_surface(state)
        .and_then(|sid| state.explorer_views.get(sid))
        .map(|v| v.ops.conflict_height)
        .filter(|h| *h > 0.0);
    let base = default_size();
    egui::vec2(base.x, measured.unwrap_or(base.y))
}

fn when(modified: Option<SystemTime>) -> Option<String> {
    crate::clock::local_system_time(modified?, "%Y-%m-%d %H:%M")
}

/// 비교 줄의 값: 파일은 크기 · 수정 시각, 폴더는 항목 수.
fn facts_text(facts: &EntryFacts) -> String {
    if facts.is_dir {
        let items = facts
            .items
            .map_or_else(|| "—".to_owned(), |n| n.to_string());
        let n = facts.items.unwrap_or(0) as u64;
        return t_count("explorer.conflict.folder_items", n, &[&items]);
    }
    let size = facts.size.map(|s| human_size(false, s));
    match (size, when(facts.modified)) {
        (Some(size), Some(when)) => format!("{size} · {when}"),
        (Some(size), None) => size,
        (None, Some(when)) => when,
        (None, None) => "—".to_owned(),
    }
}

fn choice(pick: ConflictPick) -> Choice {
    match pick {
        ConflictPick::KeepBoth => Choice::KeepBoth,
        ConflictPick::Skip => Choice::Skip,
        ConflictPick::Replace => Choice::Replace,
        ConflictPick::CancelRest => Choice::CancelRest,
    }
}

/// 질문 하나의 카드 문구.
struct Texts {
    title: String,
    in_folder: String,
    existing: String,
    incoming: String,
    apply_all: String,
}

fn texts(ask: &Ask) -> Texts {
    let title = if ask.folder_conflict && ask.existing.is_dir && ask.incoming.is_dir {
        t_fmt("explorer.conflict.title_folder", &ask.name)
    } else {
        t_fmt("explorer.conflict.title", &ask.name)
    };
    Texts {
        title,
        in_folder: t_fmt(
            "explorer.conflict.in",
            &crate::explorer_ui::view::ops::home_tilde(&ask.folder),
        ),
        existing: facts_text(&ask.existing),
        incoming: facts_text(&ask.incoming),
        apply_all: t_count(
            "explorer.conflict.apply_all",
            ask.remaining as u64,
            &[&ask.remaining.to_string()],
        ),
    }
}

/// PopupDef::draw_fn entry.
pub fn draw(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    let Some(sid) = asking_surface(state) else {
        return PopupAction::Close;
    };
    let Some(view) = state.explorer_views.get_mut(sid) else {
        return PopupAction::Close;
    };
    let Some(shared) = view.ops.running.as_ref().map(|r| r.shared.clone()) else {
        return PopupAction::Close;
    };
    let Some(ask) = shared.pending_ask() else {
        return PopupAction::Close;
    };
    let text = texts(&ask);
    let props = ConflictProps {
        title: &text.title,
        in_folder: &text.in_folder,
        existing_label: t("explorer.conflict.existing"),
        existing: &text.existing,
        incoming_label: t("explorer.conflict.incoming"),
        incoming: &text.incoming,
        no_merge: ask.folder_conflict.then(|| t("explorer.conflict.no_merge")),
        apply_all: (ask.remaining > 0).then_some(text.apply_all.as_str()),
        cancel_rest: t("explorer.conflict.cancel_rest"),
        skip: t("explorer.conflict.skip"),
        replace: (!ask.folder_conflict).then(|| t("explorer.conflict.replace")),
        keep_both: t("explorer.conflict.keep_both"),
    };
    let th = theme::theme();
    let mut apply_all = view.ops.conflict_apply_all;
    let shown = ui.scope(|ui| conflict_card(ui, &th, &props, &mut apply_all));
    view.ops.conflict_height = shown.response.rect.height();
    view.ops.conflict_apply_all = apply_all;
    let Some(pick) = shown.inner else {
        return PopupAction::None;
    };
    view.ops.conflict_apply_all = false;
    view.ops.conflict_answered = true;
    shared.answer(Answer {
        choice: choice(pick),
        apply_all: apply_all && ask.remaining > 0,
    });
    PopupAction::Close
}

/// 답하지 않고 닫혔으면 남은 작업을 멈춘다. 닫힌 뒤에는 범위를 알 수 없어 작업을 가진 칸을 찾는다.
/// 답하고 닫은 경우는 그 사이 다음 충돌이 올라왔어도 건드리지 않는다.
pub fn on_close(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    for sid in state.explorer_views.cached_surfaces().collect::<Vec<_>>() {
        let Some(view) = state.explorer_views.get_mut(sid) else {
            continue;
        };
        if view.ops.running.is_none() || std::mem::take(&mut view.ops.conflict_answered) {
            continue;
        }
        if let Some(running) = &view.ops.running
            && running.shared.pending_ask().is_some()
        {
            running.shared.answer(Answer {
                choice: Choice::CancelRest,
                apply_all: true,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 비교 줄의 수정 시각은 로컬 시간대의 `YYYY-MM-DD HH:MM` 이다.
    #[test]
    fn the_modified_time_is_a_local_date_and_minute() {
        assert_eq!(when(None), None);
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_794_963_723);
        let at = jiff::Timestamp::from_second(1_794_963_723).unwrap();
        let local = crate::clock::format_in(at, &crate::clock::local_zone(), "%Y-%m-%d %H:%M");
        assert_eq!(when(Some(t)).as_deref(), Some(local.as_str()));
        assert_eq!(local.len(), 16, "{local}");
    }
}
