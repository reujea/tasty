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

/// 한 번 큐를 비울 때 모은 surface 변환 요청(대상, 바꿀 kind).
type Converts = Vec<(u32, String)>;

/// 대기 인텐트를 모두 꺼내 그 가운데 surface 변환 요청만 돌려준다. 큐는 누적이므로 프레임마다 꺼내야 그 프레임의 요청이다.
fn take_converts(state: &mut RequestContext) -> Converts {
    state
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
        .collect()
}

/// 튜토리얼 주제 목록과 R 범위 변환 팝업을 함께 열고 `owner_is_topics` 쪽을 키 주인으로 둔다. ArrowDown 프레임과
/// Enter 프레임을 따로 넣고, 프레임마다 그 뒤의 관측과 그 프레임까지 새로 쌓인 surface 변환 요청을 돌려준다.
/// ArrowDown 쪽 요청에는 앞선 빈 프레임의 것도 들어간다.
fn topics_beside_convert(
    owner_is_topics: bool,
) -> ((TopicsSeen, Converts), (TopicsSeen, Converts)) {
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
    let after_down = (seen(&state), take_converts(&mut state));

    let texts = frame(&ctx, &mut state, &engine.read(), vec![press(Key::Enter)]);
    assert!(
        painted(&texts, t("tutorial.popup_title")),
        "Enter 프레임에도 주제 목록 본문이 그려진다: {texts:?}"
    );
    (after_down, (seen(&state), take_converts(&mut state)))
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

    let ((after_down, down_converts), (after_enter, enter_converts)) = topics_beside_convert(true);
    assert_eq!(
        after_down,
        (1, None, true, convert_at(0)),
        "주제 목록이 주인이면 ArrowDown으로 주제 선택만 옮는다"
    );
    assert_eq!(
        down_converts,
        Vec::new(),
        "주제 목록이 주인이면 ArrowDown 프레임까지 surface 변환 요청이 없다"
    );
    assert_eq!(
        after_enter,
        (1, Some(1), false, convert_at(0)),
        "주제 목록이 주인이면 Enter로 고른 주제의 시작을 요청하고 닫힌다. 변환은 그대로다"
    );
    assert_eq!(
        enter_converts,
        Vec::new(),
        "주제 목록이 주인이면 Enter 프레임에 surface 변환 요청이 없다"
    );

    let ((after_down, down_converts), (after_enter, enter_converts)) = topics_beside_convert(false);
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
        down_converts,
        Vec::new(),
        "변환이 주인이어도 ArrowDown 프레임까지는 surface 변환 요청이 없다"
    );
    assert_eq!(
        after_enter,
        (0, None, true, None),
        "변환이 주인이면 Enter로 변환이 확정해 닫히고 주제 목록은 열린 채 시작 요청이 없다"
    );
    // 이 장면의 변환 목록은 현재 kind(terminal)를 뺀 등록 kind이고, ArrowDown으로 고른 둘째 항목이 dag_graph다.
    assert_eq!(
        enter_converts,
        vec![(SURFACE_R, "dag_graph".to_string())],
        "변환이 주인이면 ArrowDown으로 고른 항목(dag_graph)으로 R surface 변환을 Enter 프레임에서 요청한다"
    );
}

const OVERLAY_OWNER: crate::adapters::ui::popup::PopupId = "scene_overlay_owner";
const MENU: &str = "menu";

/// 같은 정의(`PerScopeTarget`)의 인스턴스 A(L 범위)·B(R 범위)를 연 관리자. 한 번 그려 상자 위치를 정한다.
fn two_overlay_owners(
    ctx: &Context,
) -> (
    crate::adapters::ui::popup::PopupManager,
    PopupInstanceId,
    PopupInstanceId,
) {
    use crate::adapters::ui::popup::{PopupInstances, PopupManager, PopupState};
    use crate::model::LogicalPx;
    let mut mgr = PopupManager::new();
    mgr.register(
        PopupState::new(
            OVERLAY_OWNER,
            "overlay owner",
            egui::vec2(120.0, 80.0),
            PopupScope::Window,
        )
        .with_instances(PopupInstances::PerScopeTarget),
    );
    let a = mgr
        .open_request(
            &OpenRequest::new(OVERLAY_OWNER, PopupScope::Surface(SURFACE_L))
                .at(super::point(LogicalPx(40.0), LogicalPx(60.0))),
            false,
        )
        .expect("A opens");
    let b = mgr
        .open_request(
            &OpenRequest::new(OVERLAY_OWNER, PopupScope::Surface(SURFACE_R))
                .at(super::point(LogicalPx(490.0), LogicalPx(60.0))),
            false,
        )
        .expect("B opens");
    draw_overlay_owners(ctx, &mut mgr);
    (mgr, a, b)
}

fn draw_overlay_owners(ctx: &Context, mgr: &mut crate::adapters::ui::popup::PopupManager) {
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        ..Default::default()
    };
    drop(ctx.run(raw, |ctx| {
        mgr.draw_routed(
            ctx,
            &mut |_, _| crate::adapters::ui::PopupAction::None,
            Some(&layout()),
            &crate::adapters::ui::input::edit_key::EditBindings::default(),
            &[],
            None,
        );
    }));
}

/// A·B 상자 밖에 나온 두 메뉴 사각형. A의 것은 L, B의 것은 R 안에 있다.
fn menus() -> (egui::Rect, egui::Rect) {
    use crate::model::LogicalPx;
    let menu_a = super::to_egui(super::rect(
        LogicalPx(40.0),
        LogicalPx(400.0),
        LogicalPx(160.0),
        LogicalPx(120.0),
    ));
    let menu_b = super::to_egui(super::rect(
        LogicalPx(600.0),
        LogicalPx(400.0),
        LogicalPx(160.0),
        LogicalPx(120.0),
    ));
    (menu_a, menu_b)
}

fn pointer_boxes(
    ctx: &Context,
    mgr: &crate::adapters::ui::popup::PopupManager,
) -> Vec<crate::adapters::ui::popup::occlusion::Occluder> {
    let open: Vec<usize> = (0..mgr.popups.len())
        .filter(|&i| mgr.popups[i].open)
        .collect();
    mgr.pointer_boxes(ctx, &open, Some(&layout()), screen())
}

/// Given 같은 정의의 두 인스턴스가 같은 overlay 키로 자식 메뉴를 등록했을 때, Then 서로 덮어쓰지 않고, A 메뉴
/// 안의 점은 A의 안쪽 누름이며 B의 바깥 누름 판정이 가로채지 않는다.
#[test]
fn two_instances_keep_their_own_child_overlays() {
    use crate::adapters::ui::popup::{
        child_overlay_hit, child_overlay_rects, report_child_overlay_rect,
    };
    let ctx = Context::default();
    let (mgr, a, b) = two_overlay_owners(&ctx);
    let (menu_a, menu_b) = menus();
    report_child_overlay_rect(&ctx, a, MENU, Some((egui::Id::new("menu a"), menu_a)));
    report_child_overlay_rect(&ctx, b, MENU, Some((egui::Id::new("menu b"), menu_b)));

    assert_eq!(child_overlay_rects(&ctx, a), vec![menu_a]);
    assert_eq!(child_overlay_rects(&ctx, b), vec![menu_b]);
    assert!(child_overlay_hit(&ctx, a, menu_a.center()));
    assert!(!child_overlay_hit(&ctx, b, menu_a.center()));
    assert!(!child_overlay_hit(&ctx, a, menu_b.center()));

    let z = |iid| mgr.instance(iid).expect("open").z_seq;
    let boxes = pointer_boxes(&ctx, &mgr);
    let owners_of = |r: egui::Rect| -> Vec<u64> {
        boxes
            .iter()
            .filter(|o| o.rect == r)
            .map(|o| o.z_seq)
            .collect()
    };
    assert_eq!(owners_of(menu_a), vec![z(a)]);
    assert_eq!(owners_of(menu_b), vec![z(b)]);
}

/// Given 자식 메뉴를 등록한 인스턴스 A를 닫았을 때, Then 다음 그리기 전 포인터 상자 질의에 A 메뉴가 없고,
/// 다음 `draw_routed` 첫머리 뒤 등록부에 A 키가 없다. 열린 B의 등록은 남는다.
#[test]
fn a_closed_instance_drops_its_child_overlays() {
    use crate::adapters::ui::popup::{
        PopupCloseReason, child_overlay_keys, report_child_overlay_rect,
    };
    let ctx = Context::default();
    let (mut mgr, a, b) = two_overlay_owners(&ctx);
    let (menu_a, menu_b) = menus();
    report_child_overlay_rect(&ctx, a, MENU, Some((egui::Id::new("menu a"), menu_a)));
    report_child_overlay_rect(&ctx, b, MENU, Some((egui::Id::new("menu b"), menu_b)));

    mgr.close_instance(a, PopupCloseReason::Command);
    let boxes = pointer_boxes(&ctx, &mgr);
    assert!(boxes.iter().all(|o| o.rect != menu_a));
    assert!(boxes.iter().any(|o| o.rect == menu_b));

    draw_overlay_owners(&ctx, &mut mgr);
    let keys = child_overlay_keys(&ctx);
    assert!(!keys.contains(&(a, MENU)));
    assert!(keys.contains(&(b, MENU)));
}

/// 창의 host 팝업 층 한 프레임. 칠한 글자와 그 화면 사각형을 돌려준다.
fn frame_rects(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    events: Vec<Event>,
) -> Vec<(String, egui::Rect)> {
    fn walk(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::epaint::Shape::Text(t) => out.push((
                t.galley.text().to_owned(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
            )),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
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

fn text_rect(texts: &[(String, egui::Rect)], wanted: &str) -> egui::Rect {
    texts
        .iter()
        .find(|(s, _)| s == wanted)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{wanted} 이 칠해진다: {texts:?}"))
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

/// `pos`를 누른 프레임과 뗀 프레임을 그리고 뗀 프레임의 글자를 돌려준다.
fn click(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    pos: egui::Pos2,
) -> Vec<(String, egui::Rect)> {
    frame_rects(ctx, state, engine, pointer(pos, true));
    frame_rects(ctx, state, engine, pointer(pos, false))
}

/// remote_tool을 창 범위에 키 주인으로 열고 프로필 추가 양식까지 들어간다.
fn remote_tool_form(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
) -> PopupInstanceId {
    use crate::adapters::ui::popup::remote_tool::REMOTE_TOOL_POPUP_ID;
    registered(state);
    let iid = state
        .popups
        .open_request(
            &OpenRequest::new(REMOTE_TOOL_POPUP_ID, PopupScope::Window)
                .centered()
                .focused(),
            true,
        )
        .expect("remote_tool opens");
    frame_rects(ctx, state, engine, Vec::new());
    let texts = frame_rects(ctx, state, engine, Vec::new());
    let add = text_rect(&texts, t("remote_tool.profile_add"));
    click(ctx, state, engine, add.center());
    iid
}

const TYPE_SUGGEST: &str = "remote_tool.type_suggest";

/// Given 프로필 양식의 종류 제안 ComboBox를 열고 닫을 때, Then 등록부의 그 인스턴스 항목은 열려 있는 동안만 있고,
/// 등록한 popup id가 egui의 열린 팝업이다(`ComboBox::is_open`과 같은 규칙).
#[test]
fn a_registered_combo_reports_the_egui_open_state() {
    use crate::adapters::ui::popup::child_overlay_entry;
    let (mut state, mut session) = scene(PANE_R);
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let iid = remote_tool_form(&ctx, &mut state, &engine.read());
    let texts = frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    let arrow = text_rect(&texts, "▾");
    assert!(child_overlay_entry(&ctx, iid, TYPE_SUGGEST).is_none());

    click(&ctx, &mut state, &engine.read(), arrow.center());
    frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    let (popup, _) = child_overlay_entry(&ctx, iid, TYPE_SUGGEST).expect("열린 목록이 등록된다");
    assert!(
        ctx.memory(|m| m.is_popup_open(popup)),
        "등록한 id가 egui의 열린 팝업이다"
    );

    click(&ctx, &mut state, &engine.read(), arrow.center());
    frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    assert!(
        !ctx.memory(|m| m.any_popup_open()),
        "다시 누르면 목록이 닫힌다"
    );
    assert!(
        child_overlay_entry(&ctx, iid, TYPE_SUGGEST).is_none(),
        "닫힌 목록은 등록부에 없다"
    );
}

/// Given remote_tool 양식의 종류 제안 목록이 상자 오른쪽 밖·창 안에 열려 있을 때, When 그 목록의 상자 밖 부분을
/// 누르면, Then 누름은 remote_tool의 것이다. 포커스 노드가 remote_tool이고 그 점의 포인터 대상도 remote_tool이라
/// 아래 surface가 그 포인터를 받지 않는다. 덮는 사각형(창 전체) 때문에 등록 전에도 같다.
#[test]
fn a_combo_list_outside_the_remote_tool_keeps_its_focus() {
    use crate::model::LogicalPx;
    let (mut state, mut session) = scene(PANE_R);
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let iid = remote_tool_form(&ctx, &mut state, &engine.read());
    let texts = frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    click(
        &ctx,
        &mut state,
        &engine.read(),
        text_rect(&texts, "▾").center(),
    );
    let texts = frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    assert!(ctx.memory(|m| m.any_popup_open()), "egui 목록이 열렸다");
    let boxed = state
        .popups
        .instance(iid)
        .and_then(|p| p.box_rect())
        .expect("상자");
    let item = text_rect(&texts, "http");
    let pt = egui::pos2(boxed.right() + LogicalPx(8.0).value(), item.center().y);
    assert!(!boxed.contains(pt), "누를 점은 상자 밖이다");
    // after-only: 등록 rect 단정은 C4a-R 뒤에만 있다.
    assert!(
        crate::adapters::ui::popup::child_overlay_rects(&ctx, iid)
            .iter()
            .any(|r| r.contains(pt)),
        "등록한 목록 영역이 그 점을 포함한다"
    );

    // 포인터 대상은 직전 프레임의 배치로 정한다. 누르기 전 배치에서 본다.
    let z = state.popups.instance(iid).expect("열려 있다").z_seq;
    assert_eq!(state.popup_pointer_target(pt).map(|o| o.z_seq), Some(z));
    assert!(
        state.popup_owns_point(pt),
        "아래 surface는 그 포인터를 받지 않는다"
    );

    frame_rects(&ctx, &mut state, &engine.read(), pointer(pt, true));
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(iid)),
        "누름 대상과 포커스가 remote_tool이다"
    );
}

/// `depth`단 하위 폴더를 만든 임시 폴더. 가장 깊은 폴더 경로도 돌려준다.
fn deep_dir(depth: usize) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("임시 폴더");
    let mut deep = root.path().to_path_buf();
    for i in 0..depth {
        deep.push(format!("d{i:02}"));
    }
    std::fs::create_dir_all(&deep).expect("깊은 폴더");
    (root, deep)
}

/// `sid` 범위에 `start_dir`로 피커를 연다.
fn open_picker_at(
    state: &mut RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    sid: u32,
    start_dir: &std::path::Path,
) -> PopupInstanceId {
    let caller = tasty_ipc::caller::CallerContext::Plugin {
        plugin_id: "com.example.crumbs".into(),
    };
    let response = crate::adapters::ipc::handler::file_picker::handle_trigger(
        state,
        engine,
        &caller,
        serde_json::json!(1),
        &serde_json::json!({
            "origin_surface_id": sid,
            "location": "local",
            "mode": "folder",
            "start_dir": start_dir.to_string_lossy(),
        }),
    );
    assert!(response.result.is_some(), "{response:?}");
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(state, engine, &intent);
    }
    crate::adapters::ui::popup::file_picker::instances::open_pickers(state)
        .map(|(iid, _)| iid)
        .find(|iid| {
            state.popups.instance(*iid).map(|p| &p.scope) == Some(&PopupScope::Surface(sid))
        })
        .expect("picker opened")
}

/// Given 메인 창 피커의 숨은 경로 메뉴가 상자 밖까지 열려 있을 때, Then 그 메뉴 id는 피커 인스턴스의 자식 팝업 id
/// 목록에 있고 egui에서 열려 있다. rect 등록부에는 없고, 상자 밖 메뉴 영역의 점은 포인터 상자에 들지 않는다.
#[test]
fn the_hidden_crumbs_menu_is_a_child_of_its_picker() {
    use crate::adapters::ui::popup::file_picker::HIDDEN_CRUMBS_MENU_KEY;
    use crate::adapters::ui::popup::{child_overlay_keys, child_popup_ids};
    let (_root, deep) = deep_dir(40);
    let (mut state, mut session) = scene(PANE_R);
    let mut engine = session.borrow_mut();
    registered(&mut state);
    let picker = open_picker_at(&mut state, &mut engine, SURFACE_R, &deep);
    let ctx = Context::default();
    frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    let texts = frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    // 피커가 R보다 넓어 접힌 보기 버튼으로 그려진다. 눌러 창 위에 펼친다.
    let show = text_rect(&texts, t("popup.collapsed_button.label"));
    click(&ctx, &mut state, &engine.read(), show.center());
    frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    let texts = frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    click(
        &ctx,
        &mut state,
        &engine.read(),
        text_rect(&texts, "…").center(),
    );
    frame_rects(&ctx, &mut state, &engine.read(), Vec::new());

    let menu = child_popup_ids(&ctx)
        .into_iter()
        .find(|(key, _)| *key == (picker, HIDDEN_CRUMBS_MENU_KEY))
        .map(|(_, id)| id)
        .expect("메뉴 id가 피커 인스턴스로 보고된다");
    assert!(ctx.memory(|m| m.is_popup_open(menu)), "egui 메뉴가 열렸다");
    assert!(!child_overlay_keys(&ctx).contains(&(picker, HIDDEN_CRUMBS_MENU_KEY)));

    let menu_rect = ctx.memory(|m| m.area_rect(menu)).expect("메뉴 영역");
    let boxed = state
        .popups
        .instance(picker)
        .and_then(|p| p.box_rect())
        .expect("상자");
    let pt = egui::pos2(
        menu_rect.center().x,
        (boxed.bottom() + menu_rect.bottom()) / 2.0,
    );
    assert!(
        menu_rect.contains(pt) && !boxed.contains(pt),
        "메뉴가 상자 밖으로 나온 점이 있다: 메뉴 {menu_rect:?}, 상자 {boxed:?}"
    );
    assert!(
        pointer_boxes(&ctx, &state.popups)
            .iter()
            .all(|o| !o.rect.contains(pt)),
        "상자 밖 메뉴 영역은 포인터 상자가 아니다"
    );
}

// ---- 팝업 층 Esc(중앙 단계). 창 입력 큐(`WindowInput`)로 키를 넣고 팝업 층과 같은 순서로 그린다. ----

use crate::adapters::ui::PopupAction;
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
use crate::adapters::ui::popup::{PopupCloseReason, PopupCx, PopupState};

/// 시험 패스가 인스턴스마다 부르는 본문. `None` 이면 그 인스턴스 본문을 그리지 않는다.
type Body<'a> = dyn FnMut(
        &mut egui::Ui,
        &mut RequestContext,
        &EngineRead<'_>,
        &mut PopupCx<'_>,
    ) -> Option<PopupAction>
    + 'a;

/// 본문 함수의 이름.
type BodyFn =
    fn(&mut egui::Ui, &mut RequestContext, &EngineRead<'_>, &mut PopupCx<'_>) -> PopupAction;

/// `iid` 하나만 본문 함수 `body` 를 이름으로 불러 그리는 시험 본문. 정의 본문(`draw_fn`)은 제품 팝업 층에서만
/// 꺼내므로(본문 호출 가드) 결과 사유를 보는 시험은 본문을 이름으로 부른다.
fn by_name(
    iid: PopupInstanceId,
    body: BodyFn,
) -> impl FnMut(
    &mut egui::Ui,
    &mut RequestContext,
    &EngineRead<'_>,
    &mut PopupCx<'_>,
) -> Option<PopupAction> {
    move |ui, state, engine, cx| (cx.instance == iid).then(|| body(ui, state, engine, cx))
}

/// 창 입력 큐의 다음 패스로 host 팝업 층을 그린다. 순서는 `draw_popup_layer`와 같고(누름 → 범위·표시 → 포커스 정규화
/// → 키 주인 → 그리기 → 결과 적용) 닫기 훅만 부르지 않는다. 본문·중앙 단계 결과를 돌려준다. `content` 가 값을
/// 돌려주면 그 인스턴스는 정의 본문 대신 그 값을 쓴다.
fn layer_pass(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    input: &mut WindowInput,
    content: &mut Body<'_>,
) -> Vec<(PopupInstanceId, PopupAction)> {
    layer_pass_in(ctx, state, engine, input, layout(), content)
}

/// [`layer_pass`]와 같고 레이아웃이 `draw_ctx` 다.
fn layer_pass_in(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    input: &mut WindowInput,
    draw_ctx: crate::adapters::ui::LayoutContext,
    content: &mut Body<'_>,
) -> Vec<(PopupInstanceId, PopupAction)> {
    let events = input.take_pass(state, ctx);
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    let mut actions = Vec::new();
    drop(ctx.run(raw, |ctx| {
        let press =
            crate::adapters::ui::popup::frame::frame_press(ctx, state, engine, Some(&draw_ctx));
        state.popups.prepare_frame_for_draw(ctx, Some(&draw_ctx));
        state.normalize_focus(engine);
        state.decide_frame_key_owner(Some(&draw_ctx));
        let mut popups =
            std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());
        let mut result = popups.draw_routed(
            ctx,
            // 정의 본문은 제품 팝업 층에서만 부른다(본문 호출 가드). 이 패스는 시험 본문만 그린다.
            &mut |cx, ui| content(ui, state, engine, cx).unwrap_or(PopupAction::None),
            Some(&draw_ctx),
            &crate::adapters::ui::input::edit_key::EditBindings::default(),
            &[],
            press.as_ref(),
        );
        state.popups = popups;
        actions = std::mem::take(&mut result.actions);
        for (iid, action) in &actions {
            match action {
                PopupAction::Close => state.popups.close_instance(*iid, PopupCloseReason::Content),
                PopupAction::CloseBy(reason) => state.popups.close_instance(*iid, *reason),
                PopupAction::ReleaseFocus => state.popups.set_instance_focused(*iid, false),
                PopupAction::None => {}
            }
        }
    }));
    state.close_input_pass(ctx);
    actions
}

/// 정의 본문을 그대로 그리는 패스. 제품 팝업 층(`frame::draw_popup_layer`)이 그리고 결과를 처리한다.
fn body_pass(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    input: &mut WindowInput,
) {
    body_pass_in(ctx, state, engine, input, layout());
}

/// [`body_pass`]와 같고 레이아웃이 `draw_ctx` 다.
fn body_pass_in(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
    input: &mut WindowInput,
    draw_ctx: crate::adapters::ui::LayoutContext,
) {
    let events = input.take_pass(state, ctx);
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    drop(ctx.run(raw, |ctx| {
        crate::adapters::ui::popup::frame::draw_popup_layer(ctx, state, engine, &draw_ctx);
    }));
    // 창은 프레임 끝에 누름 스냅샷을 지운다(`egui_bridge`).
    state.press_snapshot = None;
    state.close_input_pass(ctx);
}

/// 결과 하나의 이름(`PopupAction` 은 비교·출력을 구현하지 않는다).
fn label(action: &PopupAction) -> String {
    match action {
        PopupAction::None => "None".into(),
        PopupAction::Close => "Close".into(),
        PopupAction::CloseBy(reason) => format!("CloseBy({reason:?})"),
        PopupAction::ReleaseFocus => "ReleaseFocus".into(),
    }
}

/// `iid` 가 받은 결과의 이름(None 은 뺀다).
fn action_of(actions: &[(PopupInstanceId, PopupAction)], iid: PopupInstanceId) -> Vec<String> {
    actions
        .iter()
        .filter(|(i, a)| *i == iid && !matches!(a, PopupAction::None))
        .map(|(_, a)| label(a))
        .collect()
}

fn is_open(state: &RequestContext, iid: PopupInstanceId) -> bool {
    state.popups.instance(iid).is_some_and(|p| p.open)
}

fn open_palette(state: &mut RequestContext) -> PopupInstanceId {
    state
        .popups
        .open_request(
            &OpenRequest::new(COMMAND_PALETTE_POPUP_ID, PopupScope::Window)
                .centered()
                .focused(),
            true,
        )
        .expect("palette opens")
}

/// 시험 정의 팝업 `id`(Esc 선언 `escape`)를 등록한다. 인스턴스는 범위·대상마다 하나다.
fn register_test_popup(
    state: &mut RequestContext,
    id: crate::adapters::ui::popup::PopupId,
    escape: crate::adapters::ui::popup::PopupEscape,
) {
    let mut p = PopupState::new(id, "test", egui::vec2(240.0, 160.0), PopupScope::Window)
        .with_instances(crate::adapters::ui::popup::PopupInstances::PerScopeTarget);
    p.escape = escape;
    state.popups.register(p);
}

fn open_test_popup(
    state: &mut RequestContext,
    id: crate::adapters::ui::popup::PopupId,
    scope: PopupScope,
) -> PopupInstanceId {
    state
        .popups
        .open_request(&OpenRequest::new(id, scope).centered().focused(), true)
        .expect("test popup opens")
}

/// T-4a3-1: Given 원격 닫기 확인(Esc 선언 Close)이 실제로 있는 원격 행을 기다리며 키 주인일 때(본문은 그려지고 결과는
/// None), When 수식키 없는 Esc 가 오면, Then 본문이 Esc 를 읽지 않아도 그 패스에 중앙 단계가 사유 Escape 로 닫는다.
#[test]
fn escape_closes_a_close_popup_whose_content_ignores_it() {
    use crate::adapters::ui::popup::confirm_close_remote::{
        CONFIRM_CLOSE_REMOTE_POPUP_ID, PendingCloseRemote,
    };
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    let row = 900_003;
    {
        let engine = session.borrow_mut();
        let mut ws =
            crate::model::Workspace::new_with_terminal_marker(row, "row".into(), row, row, row);
        ws.mirror = true;
        ws.remote_row = Some(crate::model::RemoteBinding {
            profile: "gx10".into(),
            remote_workspace: 4,
        });
        engine.core.push_mirror_workspace(ws);
    }
    let iid = state
        .popups
        .open_request(
            &OpenRequest::new(CONFIRM_CLOSE_REMOTE_POPUP_ID, PopupScope::Window)
                .centered()
                .focused()
                .with_payload(PopupPayload::ConfirmCloseRemote(Box::new(
                    PendingCloseRemote::new(
                        crate::app::attach_client::workspace_close::SavedRowClose {
                            row,
                            last: false,
                        },
                        crate::app::command::DomainIntent::CloseWorkspace { workspace_id: row },
                    ),
                ))),
            true,
        )
        .expect("confirmation opens");
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    for _ in 0..2 {
        let mut drawn = false;
        let mut body = by_name(
            iid,
            crate::adapters::ui::popup::confirm_close_remote::draw_confirm_close_remote,
        );
        let actions = layer_pass(
            &ctx,
            &mut state,
            &read,
            &mut input,
            &mut |ui, state, engine, cx| {
                drawn |= cx.instance == iid;
                body(ui, state, engine, cx)
            },
        );
        assert!(drawn, "본문이 그려졌다");
        assert!(action_of(&actions, iid).is_empty(), "본문 결과는 None 이다");
    }
    assert!(
        input.escape(&ctx, &mut state, &read, true),
        "창이 Esc 를 넘긴다"
    );
    let actions = layer_pass(
        &ctx,
        &mut state,
        &read,
        &mut input,
        &mut by_name(
            iid,
            crate::adapters::ui::popup::confirm_close_remote::draw_confirm_close_remote,
        ),
    );
    assert_eq!(
        action_of(&actions, iid),
        vec!["CloseBy(Escape)".to_string()],
        "중앙 단계가 사유 Escape 로 닫는다"
    );
    assert!(!is_open(&state, iid));
}

/// T-4a3-2: Given 키 주인 승인 팝업(Esc 선언 Unfocus), When Esc 가 오면, Then 포커스만 풀리고 열린 채이며 닫는 결과가
/// 없다.
#[test]
fn escape_only_unfocuses_an_unfocus_popup() {
    use crate::adapters::ui::popup::approval::APPROVAL_POPUP_ID;
    use tasty_approval::{
        ApprovalChoice, ApprovalId, ApprovalRecord, ApprovalRequest, ApprovalState, Requester,
        Severity,
    };
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    let record = ApprovalRecord {
        request: ApprovalRequest {
            id: ApprovalId("req-esc".into()),
            requester: Requester::Plugin { id: "test".into() },
            workspace_id: None,
            surface_id: None,
            title: "Test approval".into(),
            body: None,
            choices: vec![ApprovalChoice::approve(), ApprovalChoice::deny()],
            default_choice: None,
            timeout_ms: None,
            severity: Severity::Info,
            created_at: 1,
            metadata: serde_json::Value::Null,
            host_elevation: None,
        },
        state: ApprovalState::Pending,
        history: vec![],
    };
    {
        let read = session.read();
        crate::adapters::ui::popup::approval::enqueue_approval(&mut state, &read, &record);
    }
    crate::intent::popup::apply_pending(&mut state, &mut session);
    let iid = state
        .popups
        .instances_of(APPROVAL_POPUP_ID)
        .next()
        .expect("approval opens")
        .instance;
    state.popups.set_instance_focused(iid, true);
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    assert_eq!(
        state.focus_node(&read),
        FocusNode::Popup(PopupNode::Host(iid)),
        "승인 팝업이 키 주인이다"
    );
    assert!(input.escape(&ctx, &mut state, &read, true));
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(is_open(&state, iid), "닫는 결과가 없어 열린 채다");
    assert!(!state.popups.is_instance_focused(iid), "포커스가 풀렸다");
    assert_ne!(
        state.focus_node(&read),
        FocusNode::Popup(PopupNode::Host(iid))
    );
}

/// T-4a3-3: Given 키 주인 시험 팝업(Esc 선언 Ignore, 접히지 않음), When Esc 가 오면, Then 본문은 처리된 입력과 raw 입력
/// 어디에서도 Esc 를 보지 못하고 포커스를 유지한다.
#[test]
fn an_ignore_popup_body_never_sees_escape() {
    const IGNORE: &str = "scene_escape_ignore";
    let (mut state, mut session) = scene(PANE_L);
    register_test_popup(
        &mut state,
        IGNORE,
        crate::adapters::ui::popup::PopupEscape::Ignore,
    );
    let iid = open_test_popup(&mut state, IGNORE, PopupScope::Window);
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    let mut seen = Vec::new();
    let mut called = 0;
    let mut body =
        |ui: &mut egui::Ui, _: &mut RequestContext, _: &EngineRead<'_>, cx: &mut PopupCx<'_>| {
            (cx.id == IGNORE).then(|| {
                called += 1;
                seen.push(ui.input(|i| {
                    i.events
                        .iter()
                        .any(crate::adapters::ui::popup::body_keys::is_escape_press)
                        || i.raw
                            .events
                            .iter()
                            .any(crate::adapters::ui::popup::body_keys::is_escape_press)
                }));
                PopupAction::None
            })
        };
    layer_pass(&ctx, &mut state, &read, &mut input, &mut body);
    assert!(
        input.escape(&ctx, &mut state, &read, true),
        "창이 Esc 를 처리한다"
    );
    layer_pass(&ctx, &mut state, &read, &mut input, &mut body);
    assert_eq!(called, 2, "본문이 두 패스 모두 불렸다");
    assert_eq!(seen, vec![false, false], "본문은 Esc 를 보지 않는다");
    assert!(is_open(&state, iid));
    assert!(state.popups.is_instance_focused(iid), "포커스를 유지한다");
}

/// T-4a3-6: Given 키 주인 팔레트에 자식 사각형 등록만 남았고 그 egui 팝업은 닫혀 있을 때, When Esc 가 오면, Then 그
/// 등록은 열린 자식으로 치지 않아 팔레트가 닫힌다.
#[test]
fn a_stale_child_rect_does_not_count_as_open() {
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    let palette = open_palette(&mut state);
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    let stale = egui::Id::new("scene_escape_stale_child");
    crate::adapters::ui::popup::report_child_overlay_rect(
        &ctx,
        palette,
        "stale",
        Some((
            stale,
            egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(50.0, 50.0)),
        )),
    );
    assert!(
        !ctx.memory(|m| m.is_popup_open(stale)),
        "egui 팝업은 닫혀 있다"
    );
    assert!(!crate::adapters::ui::popup::child_popup_open(&ctx, palette));
    assert!(input.escape(&ctx, &mut state, &read, true));
    let actions = layer_pass(
        &ctx,
        &mut state,
        &read,
        &mut input,
        &mut by_name(
            palette,
            crate::adapters::ui::popup::command_palette::draw_command_palette_popup,
        ),
    );
    assert_eq!(
        action_of(&actions, palette),
        vec!["CloseBy(Escape)".to_string()]
    );
    assert!(!is_open(&state, palette), "팔레트가 닫힌다");
}

/// T-4a3-16: Given 키 주인 팔레트, When Shift+Esc 가 오면(창 Esc 가 아니다), Then 열린 채다.
#[test]
fn shift_escape_leaves_a_close_popup_open() {
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    let palette = open_palette(&mut state);
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(!input.escape_with(&ctx, &mut state, &read, true, egui::Modifiers::SHIFT));
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(is_open(&state, palette), "닫는 결과가 없어 열린 채다");
}

/// T-4a3-17: Given 펼친 DAG 목록 팝업(키 주인), When Shift+Esc 가 오면, Then 다시 접히지 않고 펼친 채다.
#[test]
fn shift_escape_does_not_fold_an_expanded_popup() {
    use crate::adapters::ui::popup::dag_list::DAG_LIST_POPUP_ID;
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    let iid = state
        .popups
        .open_request(
            &OpenRequest::new(DAG_LIST_POPUP_ID, PopupScope::Surface(SURFACE_L))
                .centered()
                .focused(),
            true,
        )
        .expect("dag list opens");
    if let Some(p) = state.popups.instance_mut(iid) {
        p.arrange.fit = crate::adapters::ui::popup::PopupFit::Collapse;
        p.size_user_overridden = true;
        p.set_needed_size(egui::vec2(800.0, 800.0));
    }
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(
        state.popups.instance(iid).unwrap().collapsed(),
        "좁아 접힌다"
    );
    state.popups.expand(iid);
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(state.popups.instance(iid).unwrap().expanded(), "펼쳤다");
    assert!(!input.escape_with(&ctx, &mut state, &read, true, egui::Modifiers::SHIFT));
    body_pass(&ctx, &mut state, &read, &mut input);
    let p = state.popups.instance(iid).unwrap();
    assert!(p.open && p.expanded(), "펼친 채다");
}

/// T-4a3-19: Given 원격 이름 바꾸기 확인을 기다리는 이름 바꾸기(키 주인), When Esc 가 오면, Then 열린 채다.
#[test]
fn escape_on_a_waiting_rename_keeps_it_open() {
    let (mut state, mut session, _convert, rename) = rename_beside_convert();
    for r in crate::adapters::ui::rename_popup::instances_mut(&mut state) {
        r.waiting = Some(crate::state::RenameWaiting {
            request: 7,
            host: "host".into(),
            done: false,
        });
    }
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    assert_eq!(
        state.focus_node(&read),
        FocusNode::Popup(PopupNode::Host(rename)),
        "이름 바꾸기가 키 주인이다"
    );
    assert!(input.escape(&ctx, &mut state, &read, true));
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(
        is_open(&state, rename),
        "닫는 결과가 없어 대기 중에는 열린 채다"
    );
}

/// T-4a3-20: Given Esc 선언 Close·바깥 누름 Unfocus 팝업이 좁은 범위에서 접혀 보기 버튼이 키 주인일 때, When Esc 가
/// 오면, Then 닫힌다.
#[test]
fn escape_closes_a_collapsed_close_popup_from_its_button() {
    const FOLD: &str = "scene_escape_collapsed_close";
    let (mut state, mut session) = scene(PANE_L);
    register_test_popup(
        &mut state,
        FOLD,
        crate::adapters::ui::popup::PopupEscape::Close,
    );
    let iid = open_test_popup(&mut state, FOLD, PopupScope::Surface(SURFACE_L));
    if let Some(p) = state.popups.instance_mut(iid) {
        p.close_on_outside_click = false;
        p.arrange.fit = crate::adapters::ui::popup::PopupFit::Collapse;
        p.size_user_overridden = true;
        p.set_needed_size(egui::vec2(800.0, 800.0));
    }
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    let p = state.popups.instance(iid).unwrap();
    assert!(
        p.collapsed() && state.popups.is_instance_focused(iid),
        "접힌 채 키 주인이다"
    );
    assert!(input.escape(&ctx, &mut state, &read, true));
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(!is_open(&state, iid), "닫힌다");
}

/// T-4a3-24: Given 같은 정의 인스턴스 둘(L·R 범위) 가운데 R 이 키 주인일 때, When Esc 가 오면, Then 키 주인만 닫힌다.
#[test]
fn escape_closes_only_the_owner_of_two_instances() {
    const TWO: &str = "scene_escape_two_instances";
    let (mut state, mut session) = scene(PANE_L);
    register_test_popup(
        &mut state,
        TWO,
        crate::adapters::ui::popup::PopupEscape::Close,
    );
    let left = open_test_popup(&mut state, TWO, PopupScope::Surface(SURFACE_L));
    let right = open_test_popup(&mut state, TWO, PopupScope::Surface(SURFACE_R));
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    assert_eq!(
        state.focus_node(&read),
        FocusNode::Popup(PopupNode::Host(right)),
        "R 인스턴스가 키 주인이다"
    );
    assert!(input.escape(&ctx, &mut state, &read, true));
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(!is_open(&state, right), "키 주인은 닫힌다");
    assert!(is_open(&state, left), "다른 인스턴스는 열린 채다");
}

/// T-4a3-30: Given 키 주인 팔레트, When Esc 가 오면, Then 그 패스에 닫히고 닫힌 뒤 두 패스 안에 egui 포커스가 풀린다.
#[test]
fn a_closed_palette_drops_its_egui_focus_within_two_passes() {
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    let palette = open_palette(&mut state);
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(
        ctx.memory(|m| m.focused()).is_some(),
        "팔레트 입력이 egui 포커스를 가졌다"
    );
    assert!(input.escape(&ctx, &mut state, &read, true));
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(!is_open(&state, palette), "그 패스에 닫힌다");
    body_pass(&ctx, &mut state, &read, &mut input);
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(
        ctx.memory(|m| m.focused()).is_none(),
        "두 패스 안에 egui 포커스가 풀린다"
    );
}

/// T-4a3-12: Given 저장 원격 행의 연결 팝업(Esc 선언 Unfocus)이 키 주인일 때, When Esc 가 오면, Then 포커스만 풀리고
/// 열린 채이며 닫는 결과가 없다.
#[test]
fn escape_on_a_remote_row_only_unfocuses_it() {
    use crate::adapters::ui::popup::remote_row::REMOTE_ROW_POPUP_ID;
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    let row = 900_004;
    {
        let engine = session.borrow_mut();
        let mut ws =
            crate::model::Workspace::new_with_terminal_marker(row, "row".into(), row, row, row);
        ws.mirror = true;
        ws.remote_row = Some(crate::model::RemoteBinding {
            profile: "gx10".into(),
            remote_workspace: 4,
        });
        engine.core.push_mirror_workspace(ws);
        state.set_active_workspace_index(&engine.read(), engine.workspaces().len() - 1);
    }
    // 연결 팝업은 활성 workspace(원격 행) 범위다. 형상 없는 행이라 덮지 않는 모양이다(덮는 모양은 정규화가 포커스를
    // 다시 준다).
    let row_layout = || {
        let mut l = layout();
        l.active_workspace_id = row;
        l
    };
    session.remote.row_states = std::collections::HashMap::from([(row, "not_connected")]);
    {
        let read = session.read();
        crate::adapters::ui::popup::remote_row::sync(&mut state, &read, &row_layout());
        crate::adapters::ui::popup::frame::sync_open_when(&mut state, &read);
    }
    let iid = state
        .popups
        .instances_of(REMOTE_ROW_POPUP_ID)
        .next()
        .expect("연결 팝업이 열린다")
        .instance;
    state.popups.set_instance_focused(iid, true);
    let engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass_in(&ctx, &mut state, &read, &mut input, row_layout());
    assert!(
        state.popups.is_instance_focused(iid),
        "연결 팝업이 키 주인이다"
    );
    assert!(input.escape(&ctx, &mut state, &read, true));
    body_pass_in(&ctx, &mut state, &read, &mut input, row_layout());
    assert!(is_open(&state, iid), "닫는 결과가 없어 열린 채다");
    assert!(!state.popups.is_instance_focused(iid), "포커스만 풀린다");
}

/// T-4a3-21: Given 메인 창 피커의 숨은 경로 메뉴가 열려 있을 때, When Esc 가 오면, Then 메뉴만 닫히고 피커는 열린 채다.
#[test]
fn escape_closes_only_the_hidden_crumbs_menu() {
    use crate::adapters::ui::popup::child_popup_ids;
    use crate::adapters::ui::popup::file_picker::HIDDEN_CRUMBS_MENU_KEY;
    let (_root, deep) = deep_dir(40);
    let (mut state, mut session) = scene(PANE_R);
    let mut engine = session.borrow_mut();
    registered(&mut state);
    let picker = open_picker_at(&mut state, &mut engine, SURFACE_R, &deep);
    // R 안에 들어가는 크기로 둔다. 펼친 팝업이면 창 Esc 가 다시 접기로 쓴다.
    if let Some(p) = state.popups.instance_mut(picker) {
        p.size_user_overridden = true;
        p.set_needed_size(egui::vec2(400.0, 480.0));
    }
    let ctx = Context::default();
    frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    let texts = frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    assert!(
        !state.popups.instance(picker).unwrap().collapsed(),
        "피커가 R 안에 그려진다"
    );
    click(
        &ctx,
        &mut state,
        &engine.read(),
        text_rect(&texts, "…").center(),
    );
    frame_rects(&ctx, &mut state, &engine.read(), Vec::new());
    let menu = child_popup_ids(&ctx)
        .into_iter()
        .find(|(key, _)| *key == (picker, HIDDEN_CRUMBS_MENU_KEY))
        .map(|(_, id)| id)
        .expect("메뉴 id");
    assert!(!state.popups.instance(picker).unwrap().expanded());
    assert!(ctx.memory(|m| m.is_popup_open(menu)), "메뉴가 열렸다");
    let read = engine.read();
    let mut input = WindowInput::default();
    assert!(input.escape(&ctx, &mut state, &read, true));
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(!ctx.memory(|m| m.is_popup_open(menu)), "메뉴가 닫힌다");
    assert!(is_open(&state, picker), "피커는 열린 채다");
}

/// 도구 메뉴를 열고 두 번 그린 뒤 명령 팔레트 행의 중심을 돌려준다.
fn tools_menu_scene(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &EngineRead<'_>,
) -> (PopupInstanceId, egui::Pos2) {
    registered(state);
    let tools = state
        .popups
        .open_request(
            &OpenRequest::new("tools_menu", PopupScope::Window)
                .centered()
                .focused(),
            true,
        )
        .expect("tools menu opens");
    frame_rects(ctx, state, engine, Vec::new());
    let texts = frame_rects(ctx, state, engine, Vec::new());
    let row = text_rect(&texts, t("command_palette.tools_menu_item")).center();
    // egui 는 직전 프레임 hover 로 누름 대상을 정한다.
    frame_rects(ctx, state, engine, vec![Event::PointerMoved(row)]);
    (tools, row)
}

/// 창이 사건 묶음을 받은 뒤 다음 그리기 전 단계(`about_to_wait`). 창 사건이 쌓은 Intent 를 그리기 전에 처리한다.
fn about_to_wait(
    state: &mut RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) {
    for intent in state.take_pending_intents() {
        crate::intent::popup::handle(state, engine, &intent);
    }
}

fn palette_requested(state: &mut RequestContext) -> bool {
    state.take_pending_intents().iter().any(|d| {
        matches!(
            &d.body,
            crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup(r))
                if r.id == COMMAND_PALETTE_POPUP_ID
        )
    })
}

fn button(pos: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// T-4a3-31: Given 도구 메뉴, When 한 창 사건 묶음 [행 누름, 행 뗌, Esc] 가 오면(몫 둘, 경계는 Esc 바로 앞), Then 앞 몫
/// 패스에서 행 동작(명령 팔레트 열기)이 일어나 도구 메뉴가 본문 결과로 닫히고 뒤 몫의 Esc 는 도구 메뉴에 닿지 않는다.
#[test]
fn a_click_and_escape_in_one_batch_run_the_tools_menu_row() {
    let (mut state, mut session) = scene(PANE_L);
    let mut engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let (tools, row) = tools_menu_scene(&ctx, &mut state, &read);
    let mut input = WindowInput::default();
    input.press(&mut state, &read, row);
    input.queue_mut().push(button(row, false));
    assert_eq!(input.cut_count(), 0, "누름까지는 끊긴 자리가 없다");
    input.escape(&ctx, &mut state, &read, true);
    assert_eq!(
        input.cut_count(),
        1,
        "Esc 가 몫을 끊는다: {}",
        input.cuts_text()
    );
    drop(read);
    about_to_wait(&mut state, &mut engine);
    let read = engine.read();
    let first = layer_pass(
        &ctx,
        &mut state,
        &read,
        &mut input,
        &mut by_name(tools, crate::adapters::ui::tools_menu::draw_tools_menu),
    );
    assert_eq!(
        action_of(&first, tools),
        vec!["Close".to_string()],
        "앞 몫: 행 동작"
    );
    assert!(palette_requested(&mut state), "명령 팔레트를 연다");
    let second = layer_pass(
        &ctx,
        &mut state,
        &read,
        &mut input,
        &mut by_name(tools, crate::adapters::ui::tools_menu::draw_tools_menu),
    );
    assert!(
        action_of(&second, tools).is_empty(),
        "뒤 몫의 Esc 는 도구 메뉴에 닿지 않는다"
    );
    assert!(!is_open(&state, tools));
}

/// T-4a3-31b: Given 도구 메뉴에 행 누름을 그린 뒤, When 한 몫 [행 뗌, Esc] 가 오면, Then 그 패스에 행 동작이 일어나고
/// 도구 메뉴는 본문 결과로 닫힌다(본문 결과가 있으면 Esc 를 쓴 것이다).
#[test]
fn a_release_and_escape_in_one_pass_run_the_tools_menu_row() {
    let (mut state, mut session) = scene(PANE_L);
    let mut engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let (tools, row) = tools_menu_scene(&ctx, &mut state, &read);
    let mut input = WindowInput::default();
    input.press(&mut state, &read, row);
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(is_open(&state, tools), "누름 패스 뒤 열린 채다");
    input.queue_mut().push(button(row, false));
    assert!(input.escape(&ctx, &mut state, &read, true));
    assert_eq!(input.cut_count(), 0, "한 몫이다: {}", input.cuts_text());
    drop(read);
    about_to_wait(&mut state, &mut engine);
    let read = engine.read();
    let actions = layer_pass(
        &ctx,
        &mut state,
        &read,
        &mut input,
        &mut by_name(tools, crate::adapters::ui::tools_menu::draw_tools_menu),
    );
    assert_eq!(
        action_of(&actions, tools),
        vec!["Close".to_string()],
        "본문 결과로 닫힌다"
    );
    assert!(palette_requested(&mut state), "명령 팔레트를 연다");
}

/// T-4a3-32: Given 진행 중 튜토리얼이 있을 때, When 제품 경로(`UiIntent::OpenPopup` 요청 처리기)로 주제 목록을 열고 첫
/// 그리기 전에 Esc 가 오면, Then 열기 훅이 목록을 불러온 뒤 그 패스에 닫히고 닫기 훅이 불러옴 표시를 지운다.
#[test]
fn the_tutorial_catalog_is_loaded_by_the_open_hook_before_escape() {
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    state.tutorial.request_start(0);
    state.tutorial.start_pending();
    assert!(state.tutorial.active.is_some(), "튜토리얼이 진행 중이다");
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup(
            OpenRequest::new(TUTORIAL_TOPICS_POPUP_ID, PopupScope::Window)
                .centered()
                .focused(),
        )
        .from_user_menu("test"),
    );
    crate::intent::popup::apply_pending(&mut state, &mut session);
    assert!(state.popups.is_open(TUTORIAL_TOPICS_POPUP_ID));
    assert!(
        state.tutorial.active.is_none(),
        "진행 중 튜토리얼은 중단됐다"
    );
    assert!(state.tutorial.catalog_loaded, "열기 훅이 목록을 불러왔다");
    assert!(state.tutorial.save_error, "저장 접수가 거절됐다");
    assert!(
        state.tutorial.progress[0].dirty,
        "그 주제 진행이 저장 대기다"
    );
    assert!(state.tutorial.pending_progress_is_empty());
    let mut engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    assert!(input.escape(&ctx, &mut state, &read, true));
    drop(read);
    about_to_wait(&mut state, &mut engine);
    let read = engine.read();
    let events = input.take_pass(&mut state, &ctx);
    frame(&ctx, &mut state, &read, events);
    assert!(
        !state.popups.is_open(TUTORIAL_TOPICS_POPUP_ID),
        "그 패스에 닫힌다"
    );
    assert!(
        !state.tutorial.catalog_loaded,
        "닫기 훅이 불러옴 표시를 지운다"
    );
}

/// T-4a3-33: Given 한 번 그린 주제 목록(선택 0, 진행 중 튜토리얼 없음), When 한 몫 [↓, Esc] 가 오면, Then 그 패스에 닫히고
/// 선택은 1 이다. 다시 열어 그려도 선택은 1 이다.
#[test]
fn a_down_and_escape_in_one_share_move_the_tutorial_selection() {
    let (mut state, mut session) = scene(PANE_L);
    registered(&mut state);
    let request = OpenRequest::new(TUTORIAL_TOPICS_POPUP_ID, PopupScope::Window)
        .centered()
        .focused();
    let topics = state
        .popups
        .open_request(&request, true)
        .expect("topics open");
    let mut engine = session.borrow_mut();
    let read = engine.read();
    let ctx = Context::default();
    let mut input = WindowInput::default();
    body_pass(&ctx, &mut state, &read, &mut input);
    assert_eq!(state.tutorial.popup_selected, 0);
    assert!(state.tutorial.active.is_none());
    assert!(
        input
            .key(&mut state, &read, false, press(Key::ArrowDown))
            .is_some()
    );
    assert!(input.escape(&ctx, &mut state, &read, true));
    assert_eq!(input.cut_count(), 0, "한 몫이다: {}", input.cuts_text());
    drop(read);
    about_to_wait(&mut state, &mut engine);
    let read = engine.read();
    let actions = layer_pass(
        &ctx,
        &mut state,
        &read,
        &mut input,
        &mut by_name(
            topics,
            crate::adapters::ui::tutorial::topic_popup::draw_tutorial_topics_popup,
        ),
    );
    assert_eq!(
        action_of(&actions, topics),
        vec!["CloseBy(Escape)".to_string()],
        "그 패스에 닫힌다"
    );
    assert_eq!(state.tutorial.popup_selected, 1, "↓ 가 선택을 옮겼다");
    let again = state.popups.open_request(&request, true).expect("reopen");
    body_pass(&ctx, &mut state, &read, &mut input);
    assert!(is_open(&state, again));
    assert_eq!(
        state.tutorial.popup_selected, 1,
        "다시 열어도 1번 행이 선택이다"
    );
}
