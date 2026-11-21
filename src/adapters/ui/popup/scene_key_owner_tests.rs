//! 키 주인이 아닌 팝업은 같은 키에 상태가 바뀌지 않는다. 창처럼 host 팝업 층(`draw_popup_layer`)을 그리며 키를 넣는다.
//! 장면은 좌우 surface L·R의 실제 엔진이다. 각 시험은 먼저 대상 본문이 그 프레임에 그려졌는지 확인한다.

use egui::{Context, Event, Key};

use super::repro::screen;
use super::{PANE_L, PANE_R, SURFACE_L, SURFACE_R, layout, scene, surface_rect_l};
use crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID;
use crate::adapters::ui::popup::convert::{CONVERT_POPUP_ID, ConvertPayload};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::frame::draw_popup_layer;
use crate::adapters::ui::popup::{PopupInstanceId, PopupPayload, PopupScope, defs};
use crate::adapters::ui::rename_popup::RenamePayload;
use crate::adapters::ui::tutorial::topic_popup::TUTORIAL_TOPICS_POPUP_ID;
use crate::i18n::t;
use crate::intent::OpenRequest;
use crate::runtime::engine_read::EngineRead;
use crate::state::RequestContext;
use crate::state::focus_node::FocusNode;

fn press(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 창의 host 팝업 층 한 프레임. 칠한 글자를 돌려준다.
fn frame(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    events: Vec<Event>,
) -> Vec<String> {
    fn walk(shape: &egui::epaint::Shape, texts: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(t) => texts.push(t.galley.text().to_owned()),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts)),
            _ => {}
        }
    }
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    let out = ctx.run(raw, |ctx| draw_popup_layer(ctx, state, engine, &layout()));
    state.close_input_pass(ctx);
    let mut texts = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut texts);
    }
    texts
}

fn painted(texts: &[String], wanted: &str) -> bool {
    texts.iter().any(|s| s.contains(wanted))
}

fn registered(state: &mut RequestContext) {
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
}

/// `surface`의 변환 팝업을 첫 항목을 고른 채 연다.
fn open_convert(state: &mut RequestContext, surface: u32, focused: bool) -> PopupInstanceId {
    let request = OpenRequest::new(CONVERT_POPUP_ID, PopupScope::Surface(surface))
        .centered()
        .with_payload(PopupPayload::Convert(ConvertPayload {
            surface_id: surface,
            selected: Some(0),
        }));
    let request = if focused { request.focused() } else { request };
    state
        .popups
        .open_request(&request, true)
        .expect("convert opens")
}

fn convert_of(state: &RequestContext, iid: PopupInstanceId) -> Option<(bool, ConvertPayload)> {
    let p = state.popups.instance(iid)?;
    match &p.payload {
        PopupPayload::Convert(c) => Some((p.open, c.clone())),
        _ => None,
    }
}

/// 변환 팝업이 나열할 항목 이름(현재 kind인 terminal과 숨김 kind를 뺀 등록 kind의 번역).
fn convert_labels(engine: &EngineRead<'_>) -> Vec<String> {
    engine
        .surface_registry
        .kinds_snapshot()
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| *k != "terminal" && *k != "empty")
        .map(|k| t(&format!("convert_popup.{k}")).to_string())
        .collect()
}

fn convert_drawn(texts: &[String], labels: &[String]) -> bool {
    labels.iter().any(|l| painted(texts, l))
}

fn letters() -> Vec<Event> {
    let mut out = Vec::new();
    for key in [
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
    ] {
        out.push(press(key));
        out.push(Event::Text(format!("{key:?}").to_lowercase()));
    }
    out
}

/// Given 팔레트가 키 주인이고 R 범위에 첫 항목을 고른 변환 팝업이 열려 있을 때, When Enter가 오면, Then 팔레트만
/// 명령을 확정하고 닫힌다. 변환 팝업은 열린 채 선택이 그대로다.
#[test]
fn enter_confirms_only_the_palette_and_leaves_the_convert_popup_of_another_scope() {
    let (mut state, mut session) = scene(PANE_R);
    let engine = session.borrow_mut();
    registered(&mut state);
    let convert = open_convert(&mut state, SURFACE_R, true);
    let palette = state
        .popups
        .open_request(
            &OpenRequest::new(COMMAND_PALETTE_POPUP_ID, PopupScope::Window)
                .centered()
                .focused(),
            true,
        )
        .expect("palette opens");
    let ctx = Context::default();
    let labels = convert_labels(&engine.read());
    // egui는 처음 나타난 영역을 크기만 재고 보이지 않게 그린다. 둘째 프레임부터 칠한다.
    frame(&ctx, &mut state, &engine.read(), Vec::new());
    let texts = frame(&ctx, &mut state, &engine.read(), Vec::new());
    assert!(
        painted(&texts, t("command_palette.hint_close")),
        "팔레트 본문이 그려진다: {texts:?}"
    );
    assert!(
        convert_drawn(&texts, &labels),
        "변환 본문이 그려진다: 항목 {labels:?}, 칠한 글자 {texts:?}"
    );
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(palette)),
        "팔레트가 키 주인이다"
    );

    let texts = frame(&ctx, &mut state, &engine.read(), vec![press(Key::Enter)]);
    assert!(
        convert_drawn(&texts, &labels),
        "Enter 프레임에도 변환 본문이 그려진다: {texts:?}"
    );
    assert!(
        !state.popups.instance(palette).is_some_and(|p| p.open),
        "팔레트가 Enter로 확정하고 닫힌다"
    );
    assert!(
        state.command_palette.pending_run.is_some(),
        "팔레트가 고른 명령을 실행 대기에 둔다"
    );
    assert_eq!(
        convert_of(&state, convert),
        Some((
            true,
            ConvertPayload {
                surface_id: SURFACE_R,
                selected: Some(0),
            }
        )),
        "변환 팝업은 열린 채 선택이 그대로다"
    );
}

/// Given R 범위에 변환 팝업과 이름 바꾸기가 열려 있고 키 주인이 L surface일 때, When Enter·화살표·Esc·글자가 오면,
/// Then 두 팝업은 열린 채 선택과 입력이 그대로다.
#[test]
fn popups_of_another_scope_keep_their_state_while_a_surface_owns_the_keys() {
    let (mut state, mut session) = scene(PANE_R);
    let engine = session.borrow_mut();
    registered(&mut state);
    let convert = open_convert(&mut state, SURFACE_R, false);
    let rename = crate::adapters::ui::rename_popup::open_for_test(
        &mut state,
        RenamePayload::new(crate::state::RenameTarget::NewCategory, "old".into()),
        PopupScope::Surface(SURFACE_R),
        None,
    )
    .expect("rename opens");
    let ctx = Context::default();
    let labels = convert_labels(&engine.read());
    frame(&ctx, &mut state, &engine.read(), Vec::new());

    let snapshot = state.take_press_snapshot(
        &engine.read(),
        surface_rect_l().center(),
        egui::PointerButton::Primary,
    );
    state.keep_press_snapshot(snapshot);
    {
        let read = engine.read();
        state
            .navigation
            .select_pane(read.workspace_at(0).unwrap(), PANE_L);
    }
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L)),
        "키 주인은 L surface다"
    );

    for p in crate::adapters::ui::rename_popup::instances_mut(&mut state) {
        p.content_height = None;
    }
    let mut events = vec![
        press(Key::ArrowDown),
        press(Key::ArrowUp),
        press(Key::Enter),
        press(Key::Escape),
    ];
    events.extend(letters());
    let texts = frame(&ctx, &mut state, &engine.read(), events);
    assert!(
        convert_drawn(&texts, &labels),
        "변환 본문이 그려진다: 항목 {labels:?}, 칠한 글자 {texts:?}"
    );
    assert!(
        crate::adapters::ui::rename_popup::instances(&state).all(|r| r.content_height.is_some()),
        "이름 바꾸기 본문이 그려진다"
    );
    assert_eq!(
        convert_of(&state, convert),
        Some((
            true,
            ConvertPayload {
                surface_id: SURFACE_R,
                selected: Some(0),
            }
        )),
        "변환 팝업은 열린 채 선택이 그대로다"
    );
    assert!(
        state.popups.instance(rename).is_some_and(|p| p.open),
        "이름 바꾸기는 열린 채다"
    );
    assert_eq!(
        crate::adapters::ui::rename_popup::instances(&state)
            .map(|r| r.buffer.clone())
            .collect::<Vec<_>>(),
        vec!["old".to_string()],
        "이름 바꾸기 입력이 그대로다"
    );
}

/// 튜토리얼 주제 목록과 R 범위 변환 팝업을 함께 열고 `owner_is_topics` 쪽을 키 주인으로 둔 뒤 ArrowDown·Enter를 넣는다.
/// (주제 선택, 시작 요청, 변환 상태)를 돌려준다.
fn topics_beside_convert(
    owner_is_topics: bool,
) -> (usize, Option<usize>, Option<(bool, ConvertPayload)>) {
    let (mut state, mut session) = scene(PANE_R);
    let engine = session.borrow_mut();
    registered(&mut state);
    let topics_request = OpenRequest::new(TUTORIAL_TOPICS_POPUP_ID, PopupScope::Window)
        .centered()
        .focused();
    let (topics, convert) = if owner_is_topics {
        let convert = open_convert(&mut state, SURFACE_R, true);
        let topics = state.popups.open_request(&topics_request, true);
        (topics, convert)
    } else {
        let topics = state.popups.open_request(&topics_request, true);
        (topics, open_convert(&mut state, SURFACE_R, true))
    };
    let topics = topics.expect("topics open");
    let owner = if owner_is_topics { topics } else { convert };
    let ctx = Context::default();
    let labels = convert_labels(&engine.read());
    frame(&ctx, &mut state, &engine.read(), Vec::new());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(owner)),
        "키 주인(주제 목록 {owner_is_topics})"
    );
    assert_eq!(state.tutorial.popup_selected, 0);
    assert_eq!(state.tutorial.pending_start, None);

    let texts = frame(
        &ctx,
        &mut state,
        &engine.read(),
        vec![press(Key::ArrowDown)],
    );
    assert!(
        painted(&texts, t("tutorial.popup_title")),
        "주제 목록 본문이 그려진다: {texts:?}"
    );
    assert!(
        convert_drawn(&texts, &labels),
        "변환 본문이 그려진다: 항목 {labels:?}, 칠한 글자 {texts:?}"
    );
    (
        state.tutorial.popup_selected,
        state.tutorial.pending_start,
        convert_of(&state, convert),
    )
}

/// Given 튜토리얼 주제 목록과 R 범위 변환 팝업이 함께 열려 있을 때, When ArrowDown이 오면, Then 키 주인 쪽만 선택을
/// 옮긴다. 주제 목록이 주인이면 변환 선택이 그대로이고, 변환이 주인이면 주제 선택이 그대로다.
#[test]
fn only_the_key_owner_reacts_when_tutorial_topics_share_the_screen_with_another_popup() {
    let (topic_selected, started, convert) = topics_beside_convert(true);
    assert_eq!(topic_selected, 1, "주제 목록이 주인이면 주제 선택이 옮는다");
    assert_eq!(started, None);
    assert_eq!(
        convert,
        Some((
            true,
            ConvertPayload {
                surface_id: SURFACE_R,
                selected: Some(0),
            }
        )),
        "주제 목록이 주인이면 변환 선택은 그대로다"
    );

    let (topic_selected, started, convert) = topics_beside_convert(false);
    assert_eq!(topic_selected, 0, "변환이 주인이면 주제 선택은 그대로다");
    assert_eq!(started, None, "변환이 주인이면 주제 시작 요청이 없다");
    assert_eq!(
        convert,
        Some((
            true,
            ConvertPayload {
                surface_id: SURFACE_R,
                selected: Some(1),
            }
        )),
        "변환이 주인이면 변환 선택이 옮는다"
    );
}
