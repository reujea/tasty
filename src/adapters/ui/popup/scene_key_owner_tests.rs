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

/// R pane의 탭 이름 바꾸기를 제품 열기 경로(`rename_popup::open`과 의도 적용)로 열고, 그 옆 R surface에 첫 항목을 고른
/// 변환 팝업을 키 없이 연다. 이름 바꾸기가 키 주인이다. (상태, 엔진, 변환, 이름 바꾸기)를 돌려준다.
fn rename_beside_convert() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
    PopupInstanceId,
    PopupInstanceId,
) {
    let (mut state, mut session) = scene(PANE_R);
    registered(&mut state);
    let convert = open_convert(&mut state, SURFACE_R, false);
    let read = session.read();
    assert!(crate::adapters::ui::rename_popup::open(
        &mut state,
        &read,
        crate::state::RenameTarget::TabName { tab_id: SURFACE_R },
        "old".into(),
        crate::intent::UiIntent::from_user_context_menu,
    ));
    drop(read);
    crate::intent::popup::apply_pending(&mut state, &mut session);
    let rename = state
        .popups
        .instances_of(crate::adapters::ui::rename_popup::RENAME_POPUP_ID)
        .map(|p| (p.instance, p.scope.clone()))
        .collect::<Vec<_>>();
    assert_eq!(rename.len(), 1, "이름 바꾸기 하나가 열린다: {rename:?}");
    let (rename, scope) = rename[0].clone();
    assert_eq!(
        scope,
        PopupScope::Pane(PANE_R),
        "탭 이름 바꾸기는 그 탭의 pane 범위다"
    );
    (state, session, convert, rename)
}

fn rename_buffers(state: &RequestContext) -> Vec<String> {
    crate::adapters::ui::rename_popup::instances(state)
        .map(|r| r.buffer.clone())
        .collect()
}

/// 이름 바꾸기 본문을 그린 표시를 지운다. 다음 프레임 뒤 다시 채워졌으면 그 프레임에 그려진 것이다.
fn forget_rename_height(state: &mut RequestContext) {
    for p in crate::adapters::ui::rename_popup::instances_mut(state) {
        p.content_height = None;
    }
}

fn rename_drawn(state: &RequestContext) -> bool {
    crate::adapters::ui::rename_popup::instances(state).all(|r| r.content_height.is_some())
}

/// 양성 대조: Given R pane 탭 이름 바꾸기가 키 주인이고 R surface에 변환 팝업이 열려 있을 때, When 글자가 오면,
/// Then 이름 바꾸기 입력이 egui 포커스를 얻고 글자가 입력에 들어간다. 아래 음성 시험과 같은 장면이다.
#[test]
fn the_rename_popup_takes_letters_while_it_owns_the_keys() {
    let (mut state, mut session, _convert, rename) = rename_beside_convert();
    let engine = session.borrow_mut();
    let ctx = Context::default();
    frame(&ctx, &mut state, &engine.read(), Vec::new());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(rename)),
        "이름 바꾸기가 키 주인이다"
    );
    assert!(
        ctx.memory(|m| m.focused()).is_some(),
        "이름 바꾸기 입력이 egui 포커스를 얻는다"
    );
    forget_rename_height(&mut state);
    frame(&ctx, &mut state, &engine.read(), letters());
    assert!(rename_drawn(&state), "이름 바꾸기 본문이 그려진다");
    let buffers = rename_buffers(&state);
    assert!(
        buffers.len() == 1 && buffers[0].contains("abcdefghijklmnopqrstuvwxyz"),
        "키 주인인 이름 바꾸기 입력에 글자가 들어간다: {buffers:?}"
    );
}

/// Given R pane 탭 이름 바꾸기와 R surface 변환 팝업이 열려 있고 키 주인이 L surface로 옮겨졌을 때, When 글자, Enter,
/// 화살표와 Esc가 각각 다른 프레임에 오면, Then 이름 바꾸기 입력은 egui 포커스를 잃고 글자를 받지 않으며 Enter로
/// 확정하지 않는다. 변환 선택도 그대로이고 두 팝업은 열린 채다.
#[test]
fn popups_of_another_scope_keep_their_state_while_a_surface_owns_the_keys() {
    let (mut state, mut session, convert, rename) = rename_beside_convert();
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let labels = convert_labels(&engine.read());
    frame(&ctx, &mut state, &engine.read(), Vec::new());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(rename)),
        "처음에는 이름 바꾸기가 키 주인이다"
    );
    let input = ctx.memory(|m| m.focused());
    assert!(input.is_some(), "이름 바꾸기 입력이 egui 포커스를 얻는다");

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
    let unchanged = Some((
        true,
        ConvertPayload {
            surface_id: SURFACE_R,
            selected: Some(0),
        },
    ));

    forget_rename_height(&mut state);
    frame(&ctx, &mut state, &engine.read(), letters());
    assert!(
        rename_drawn(&state),
        "글자 프레임에 이름 바꾸기 본문이 그려진다"
    );
    assert_eq!(
        ctx.memory(|m| m.focused()),
        None,
        "키 주인이 아닌 이름 바꾸기 입력은 egui 포커스를 내놓는다(앞 포커스 {input:?})"
    );
    assert_eq!(
        rename_buffers(&state),
        vec!["old".to_string()],
        "이름 바꾸기 입력이 글자를 받지 않는다"
    );

    forget_rename_height(&mut state);
    frame(&ctx, &mut state, &engine.read(), vec![press(Key::Enter)]);
    assert!(
        rename_drawn(&state),
        "Enter 프레임에 이름 바꾸기 본문이 그려진다"
    );
    assert!(
        state.popups.instance(rename).is_some(),
        "이름 바꾸기가 Enter로 확정하지 않고 열린 채다"
    );
    assert_eq!(rename_buffers(&state), vec!["old".to_string()]);

    forget_rename_height(&mut state);
    let texts = frame(
        &ctx,
        &mut state,
        &engine.read(),
        vec![
            press(Key::ArrowDown),
            press(Key::ArrowUp),
            press(Key::ArrowDown),
            press(Key::Escape),
        ],
    );
    assert!(
        convert_drawn(&texts, &labels),
        "변환 본문이 그려진다: 항목 {labels:?}, 칠한 글자 {texts:?}"
    );
    assert!(
        rename_drawn(&state),
        "화살표·Esc 프레임에 이름 바꾸기 본문이 그려진다"
    );
    assert_eq!(
        convert_of(&state, convert),
        unchanged,
        "변환 팝업은 열린 채 선택이 그대로다"
    );
    assert!(
        state.popups.instance(rename).is_some(),
        "이름 바꾸기는 Esc에도 열린 채다"
    );
}

/// 한 프레임 뒤의 (주제 선택, 시작 요청, 주제 목록이 열려 있는지, 변환 상태).
type TopicsSeen = (usize, Option<usize>, bool, Option<(bool, ConvertPayload)>);

/// 튜토리얼 주제 목록과 R 범위 변환 팝업을 함께 열고 `owner_is_topics` 쪽을 키 주인으로 둔다. ArrowDown 프레임과
/// Enter 프레임을 따로 넣고 각 프레임 뒤의 관측과, Enter 프레임까지 쌓인 surface 변환 요청(대상, 바꿀 kind)을 돌려준다.
fn topics_beside_convert(owner_is_topics: bool) -> (TopicsSeen, TopicsSeen, Vec<(u32, String)>) {
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
    let seen = |state: &RequestContext| -> TopicsSeen {
        (
            state.tutorial.popup_selected,
            state.tutorial.pending_start,
            state.popups.instance(topics).is_some(),
            convert_of(state, convert),
        )
    };
    let after_down = seen(&state);

    let texts = frame(&ctx, &mut state, &engine.read(), vec![press(Key::Enter)]);
    assert!(
        painted(&texts, t("tutorial.popup_title")),
        "Enter 프레임에도 주제 목록 본문이 그려진다: {texts:?}"
    );
    let converts = state
        .take_pending_intents()
        .into_iter()
        .filter_map(|i| match i.body {
            crate::intent::Intent::ConvertSurface { surface_id, target } => Some((
                surface_id,
                match target {
                    crate::intent::ConvertTarget::Terminal => "terminal".to_string(),
                    crate::intent::ConvertTarget::Kind { kind, .. } => kind,
                },
            )),
            _ => None,
        })
        .collect();
    (after_down, seen(&state), converts)
}

/// Given 튜토리얼 주제 목록과 R 범위 변환 팝업이 함께 열려 있을 때, When ArrowDown 프레임 뒤 Enter 프레임이 오면,
/// Then 키 주인 쪽만 반응한다. 주제 목록이 주인이면 주제 선택이 옮고 Enter로 시작을 요청하며 변환은 그대로다.
/// 변환이 주인이면 변환 선택이 옮고 Enter로 고른 kind로 R surface 변환을 요청하고 닫히며, 주제 목록은 두 프레임 뒤
/// 모두 열린 채 선택과 시작 요청이 그대로다.
#[test]
fn only_the_key_owner_reacts_when_tutorial_topics_share_the_screen_with_another_popup() {
    let convert_at = |selected| {
        Some((
            true,
            ConvertPayload {
                surface_id: SURFACE_R,
                selected: Some(selected),
            },
        ))
    };

    let (after_down, after_enter, converts) = topics_beside_convert(true);
    assert_eq!(
        after_down,
        (1, None, true, convert_at(0)),
        "주제 목록이 주인이면 ArrowDown으로 주제 선택만 옮는다"
    );
    assert_eq!(
        after_enter,
        (1, Some(1), false, convert_at(0)),
        "주제 목록이 주인이면 Enter로 고른 주제의 시작을 요청하고 닫힌다. 변환은 그대로다"
    );
    assert_eq!(
        converts,
        Vec::new(),
        "주제 목록이 주인이면 surface 변환 요청이 없다"
    );

    let (after_down, after_enter, converts) = topics_beside_convert(false);
    // Enter 프레임의 주제 쪽 관측을 먼저 본다. ArrowDown 단정이 먼저 실패하면 시험이 거기서 멈춰 Enter가 주제 목록에
    // 닿았는지를 보지 못한다.
    assert_eq!(
        (after_enter.1, after_enter.2),
        (None, true),
        "변환이 주인이면 Enter에 주제 시작 요청이 없고 주제 목록이 열린 채다"
    );
    assert_eq!(
        after_down,
        (0, None, true, convert_at(1)),
        "변환이 주인이면 ArrowDown으로 변환 선택만 옮고 주제 목록은 열린 채 그대로다"
    );
    assert_eq!(
        after_enter,
        (0, None, true, None),
        "변환이 주인이면 Enter로 변환이 확정해 닫히고 주제 목록은 열린 채 시작 요청이 없다"
    );
    // 이 장면의 변환 목록은 현재 kind(terminal)를 뺀 등록 kind이고, ArrowDown으로 고른 둘째 항목이 dag_graph다.
    assert_eq!(
        converts,
        vec![(SURFACE_R, "dag_graph".to_string())],
        "변환이 주인이면 Enter로 고른 항목(dag_graph)으로 R surface 변환을 요청한다"
    );
}
