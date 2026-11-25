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

/// 질문을 띄운 칸. 인스턴스 데이터가 가진다.
fn asking_surface(payload: &super::PopupPayload) -> Option<u32> {
    match payload {
        super::PopupPayload::ExplorerConflict { surface_id } => Some(*surface_id),
        _ => None,
    }
}

/// 충돌 질문을 그 칸에 띄우는 요청.
pub(crate) fn open_request(surface_id: u32) -> crate::intent::OpenRequest {
    crate::intent::OpenRequest::new(
        EXPLORER_CONFLICT_POPUP_ID,
        crate::model::popup_kind::PopupScope::Surface(surface_id),
    )
    .centered()
    .focused()
    .with_payload(super::PopupPayload::ExplorerConflict { surface_id })
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
    payload: &crate::adapters::ui::popup::PopupPayload,
) -> egui::Vec2 {
    let measured = asking_surface(payload)
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
    cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    let Some(sid) = asking_surface(cx.payload) else {
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
    // 카드는 키를 읽지 않는다. Enter 는 이 팝업의 키 통로가 판정한다(소비하지 않아 포커스된 버튼의 Enter 는 그 버튼이
    // 받는다). Esc 는 선언 Ignore 라 창이 받지 않는다.
    let confirm = cx.keys.pressed(super::keys::BasicKey::Confirm);
    let shown = ui.scope(|ui| conflict_card(ui, &th, &props, &mut apply_all, confirm));
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

/// 답하지 않고 닫혔으면 질문을 띄운 칸의 남은 작업을 멈춘다.
/// 답하고 닫은 경우는 그 사이 다음 충돌이 올라왔어도 건드리지 않는다.
pub fn on_close(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    let Some(view) =
        asking_surface(&closed.payload).and_then(|sid| state.explorer_views.get_mut(sid))
    else {
        return;
    };
    if view.ops.running.is_none() || std::mem::take(&mut view.ops.conflict_answered) {
        return;
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::app::explorer_files::job::{EntryFacts, Shared, Snapshot};

    /// `sid` 칸에 충돌 질문을 기다리는 작업을 둔다.
    fn waiting(state: &mut MainViewState, sid: u32) -> Arc<Shared> {
        let panel = crate::model::ExplorerPanel::new(sid, "/w".into());
        state.explorer_views.get_or_init(&panel, None);
        let shared = Arc::new(Shared::interactive(|| {}));
        let facts = EntryFacts {
            is_dir: false,
            size: Some(1),
            items: None,
            modified: None,
        };
        shared.mirror(
            &Snapshot {
                files: None,
                items_done: 0,
                items_total: 1,
                bytes_done: 0,
                bytes_total: 1,
                current: "a.txt".into(),
                removing: false,
            },
            Some(Ask {
                name: "a.txt".into(),
                folder: "/w".into(),
                existing: facts.clone(),
                incoming: facts,
                folder_conflict: false,
                remaining: 0,
            }),
        );
        let view = state.explorer_views.get_mut(sid).expect("explorer view");
        view.ops.running = Some(crate::explorer_ui::view::ops::Running {
            shared: shared.clone(),
            kind: crate::app::explorer_files::job::OpKind::Copy,
            dest: None,
            total: 1,
            in_history: false,
        });
        shared
    }

    /// 답하지 않고 닫은 질문은 그 질문을 띄운 칸의 남은 작업만 멈춘다.
    #[test]
    fn closing_an_unanswered_question_stops_only_its_own_job() {
        let (mut state, session) = crate::state::tests::test_state();
        let left = waiting(&mut state, 1);
        let right = waiting(&mut state, 2);
        let closed = crate::adapters::ui::popup::ClosedPopup {
            payload: super::super::PopupPayload::ExplorerConflict { surface_id: 1 },
            ..crate::adapters::ui::popup::ClosedPopup::test(EXPLORER_CONFLICT_POPUP_ID)
        };

        on_close(
            &egui::Context::default(),
            &mut state,
            &session.read(),
            &closed,
        );

        assert_eq!(
            left.take_answer(),
            Some(Answer {
                choice: Choice::CancelRest,
                apply_all: true
            })
        );
        assert_eq!(right.take_answer(), None);
        assert!(right.pending_ask().is_some());
    }

    /// T-6b-1: Given 포커스를 받은 충돌 질문(제품 정의, 선언 Ignore), When 수식키 없는 Esc 가 오면, Then 질문은
    /// Esc 수신자가 아니고 그 Esc 는 부모 경로로 가 egui 에 실리지 않는다. 사건 시점에 포커스가 그대로이고, 그린
    /// 뒤에도 남은 작업을 취소하지 않고 질문이 열린 채다.
    #[test]
    fn escape_does_nothing_on_the_conflict_question() {
        use crate::adapters::ui::popup::window_input_test_support::layer_frame;
        crate::i18n::init("en");
        let (mut state, mut session) = crate::state::tests::test_state();
        for def in super::super::defs::all_defs() {
            state.popups.register_def(def, 1.0);
        }
        let engine = session.borrow_mut();
        let read = engine.read();
        let sid = state.focused_surface_id(&read).expect("surface");
        let shared = waiting(&mut state, sid);
        let iid = state
            .popups
            .open_request(&open_request(sid), true)
            .expect("question opens");
        state.popups.set_instance_focused(iid, true);
        let ctx = egui::Context::default();
        layer_frame(&ctx, &mut state, &read, Vec::new());
        assert!(
            state.popups.is_instance_focused(iid),
            "질문이 포커스를 가진다"
        );
        let mut events = vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }];
        let egui_surface =
            crate::adapters::ui::input::key_nodes::focused_surface_takes_egui(&state, &read);
        let decision = crate::adapters::ui::input::key_nodes::bare_escape_decision(
            &state,
            &read,
            egui_surface,
        );
        assert!(
            matches!(
                decision,
                crate::adapters::ui::input::key_nodes::KeyDecision::Path(routed)
                    if !routed.receiver.is_egui()
            ),
            "Esc 는 질문을 지나 부모 경로로 간다: {decision:?}"
        );
        assert!(
            !crate::adapters::ui::popup::window_input_test_support::window_escape_step(
                &mut state,
                &read,
                &mut events,
                false
            ),
            "창의 Esc 적용은 처리하지 않고 단축키·끝 목적지 단계로 넘긴다"
        );
        assert!(
            state.popups.is_instance_focused(iid),
            "사건 시점에 포커스가 그대로다"
        );
        // egui 수신자가 아닌 Esc 는 egui 입력에 실리지 않는다(단계 8).
        layer_frame(&ctx, &mut state, &read, Vec::new());
        layer_frame(&ctx, &mut state, &read, Vec::new());
        assert_eq!(shared.take_answer(), None, "남은 작업을 취소하지 않는다");
        assert!(
            state.popups.instance(iid).is_some_and(|p| p.open),
            "질문이 열린 채다"
        );
    }

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
