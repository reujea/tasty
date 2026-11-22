//! 변환 팝업이 surface마다 인스턴스를 따로 열고 대상·선택을 인스턴스에 두는지 확인한다.

use super::{CONVERT_POPUP_ID, ConvertPayload, open_request};
use crate::adapters::ui::popup::{PopupCloseReason, PopupInstanceId, PopupPayload};
use crate::intent::UiIntent;
use crate::state::RequestContext;

fn reopen(
    state: &mut RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
    surface_id: u32,
) {
    let mut engine = session.borrow_mut();
    crate::intent::popup::handle(
        state,
        &mut engine,
        &UiIntent::ReopenPopup(open_request(surface_id)).from_user_menu("test"),
    );
}

/// 열린 변환 인스턴스의 (인스턴스 id, 대상 데이터)를 연 순서대로 모은다.
fn opened(state: &RequestContext) -> Vec<(PopupInstanceId, ConvertPayload)> {
    let mut found: Vec<_> = state
        .popups
        .instances_of(CONVERT_POPUP_ID)
        .map(|p| match &p.payload {
            PopupPayload::Convert(c) => (p.instance, c.clone()),
            other => panic!("convert instance without its data: {other:?}"),
        })
        .collect();
    found.sort_by_key(|(iid, _)| *iid);
    found
}

fn target(surface_id: u32, selected: Option<usize>) -> ConvertPayload {
    ConvertPayload {
        surface_id,
        selected,
    }
}

fn select(state: &mut RequestContext, iid: PopupInstanceId, selected: usize) {
    let p = state.popups.instance_mut(iid).expect("open instance");
    let PopupPayload::Convert(c) = &mut p.payload else {
        panic!("convert instance without its data");
    };
    c.selected = Some(selected);
}

/// 왼쪽·오른쪽 surface에서 연 변환 팝업은 각자 자기 surface를 대상으로 한다.
#[test]
fn each_surface_opens_its_own_convert_instance() {
    let (mut state, mut session) = crate::state::tests::test_state();
    reopen(&mut state, &mut session, 1);
    reopen(&mut state, &mut session, 2);

    let found = opened(&state);
    assert_eq!(
        found.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
        vec![target(1, None), target(2, None)]
    );
}

/// 같은 surface에서 다시 열면 그 인스턴스의 선택만 처음으로 돌아가고 다른 surface의 선택은 남는다.
#[test]
fn reopening_on_one_surface_resets_only_its_own_selection() {
    let (mut state, mut session) = crate::state::tests::test_state();
    reopen(&mut state, &mut session, 1);
    reopen(&mut state, &mut session, 2);
    let found = opened(&state);
    let (left, right) = (found[0].0, found[1].0);
    select(&mut state, left, 2);
    select(&mut state, right, 1);

    reopen(&mut state, &mut session, 1);

    assert_eq!(
        opened(&state),
        vec![(left, target(1, None)), (right, target(2, Some(1)))]
    );
}

/// 한 surface의 변환 팝업을 닫아도 다른 surface의 인스턴스와 대상은 그대로다.
#[test]
fn closing_one_surface_instance_keeps_the_other_target() {
    let (mut state, mut session) = crate::state::tests::test_state();
    reopen(&mut state, &mut session, 1);
    reopen(&mut state, &mut session, 2);
    let found = opened(&state);
    let (left, right) = (found[0].0, found[1].0);
    select(&mut state, right, 1);

    state.popups.close_instance(left, PopupCloseReason::Escape);

    assert_eq!(opened(&state), vec![(right, target(2, Some(1)))]);
}

/// 두 surface의 변환 인스턴스를 한 프레임에 그리며 키 하나를 넣는다. 인스턴스마다 그리기 결과를 돌려준다.
fn draw_with_key(key: egui::Key, focused: [bool; 2]) -> [&'static str; 2] {
    let (mut state, session) = crate::state::tests::two_pane_fixture();
    let engine = session.read();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        events: vec![egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut acted = ["none"; 2];
    drop(egui::Context::default().run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            for (i, sid) in [101, 201].into_iter().enumerate() {
                let mut payload = PopupPayload::Convert(target(sid, Some(0)));
                let mut cx = crate::adapters::ui::popup::PopupCx {
                    id: CONVERT_POPUP_ID,
                    instance: PopupInstanceId::NONE,
                    focused: focused[i],
                    keys: crate::adapters::ui::popup::keys::PopupKeys::new(
                        ctx,
                        focused[i],
                        crate::adapters::ui::popup::PopupKeyboard::Keys,
                        &crate::adapters::ui::input::edit_key::EditBindings::default(),
                    ),
                    payload: &mut payload,
                };
                let action = super::draw_convert_popup(ui, &mut state, &engine, &mut cx);
                acted[i] = match action {
                    crate::adapters::ui::PopupAction::None => "none",
                    crate::adapters::ui::PopupAction::CloseBy(PopupCloseReason::Escape) => "escape",
                    _ => "acted",
                };
            }
        });
    }));
    acted
}

/// 인스턴스마다 키에 반응했는지.
fn press_with_focus(key: egui::Key, focused: [bool; 2]) -> [bool; 2] {
    draw_with_key(key, focused).map(|a| a != "none")
}

/// 첫 항목의 글자 단축키.
fn first_shortcut_key() -> egui::Key {
    let (state, session) = crate::state::tests::two_pane_fixture();
    let ch = super::listed_kinds(&state, &session.read(), Some(101))
        .into_iter()
        .find_map(|item| item.shortcut)
        .expect("a convert item with a letter shortcut");
    egui::Key::from_name(&ch.to_string()).expect("letter key")
}

/// Given 두 surface에 변환 팝업이 보이고 한쪽만 포커스를 가졌을 때, When Enter·글자 단축키를 누르면,
/// Then 포커스를 가진 인스턴스만 반응한다. Esc 는 본문이 읽지 않는다(아래 시험).
#[test]
fn only_the_focused_convert_instance_reads_keys() {
    for key in [egui::Key::Enter, first_shortcut_key()] {
        assert_eq!(
            press_with_focus(key, [false, true]),
            [false, true],
            "{key:?}"
        );
        assert_eq!(
            press_with_focus(key, [true, false]),
            [true, false],
            "{key:?}"
        );
    }
}

/// When 포커스를 가진 변환 팝업에서 Escape를 누르면, Then 두 본문 모두 반응하지 않는다. 본문은 Esc 를 읽지 않는다. 닫기는 팝업 층 중앙 단계가 선언대로 정한다(T-4a3-25).
#[test]
fn the_convert_body_leaves_escape_to_the_popup_layer() {
    assert_eq!(
        draw_with_key(egui::Key::Escape, [true, false]),
        ["none", "none"]
    );
}

/// 변환 내용 한 장면. 문맥을 이어 써서 클릭처럼 여러 프레임에 걸친 입력을 넣는다. 대상은 왼쪽 surface(101)다.
struct Content {
    ctx: egui::Context,
    state: RequestContext,
    session: crate::runtime::engine_session::EngineSession,
    target: ConvertPayload,
    edits: crate::adapters::ui::input::edit_key::EditBindings,
}

/// 고른 동작의 kind.
fn chosen_kind(action: super::ConvertAction) -> String {
    match action {
        super::ConvertAction::Terminal => "terminal".to_string(),
        super::ConvertAction::RequiresInput(kind) | super::ConvertAction::Kind(kind) => kind,
    }
}

impl Content {
    fn new(selected: Option<usize>) -> Self {
        crate::i18n::init("en");
        let (state, session) = crate::state::tests::two_pane_fixture();
        Self {
            ctx: egui::Context::default(),
            state,
            session,
            target: target(101, selected),
            edits: Default::default(),
        }
    }

    /// 나열 항목의 (kind, 라벨, 단축키 글자).
    fn items(&self) -> Vec<(String, String, Option<char>)> {
        super::listed_kinds(&self.state, &self.session.read(), Some(101))
            .into_iter()
            .map(|it| (it.kind.to_string(), it.label, it.shortcut))
            .collect()
    }

    /// 단축키가 있는 앞 두 항목의 (kind, 라벨, 글자 키). 항목 순서다.
    fn two_shortcuts(&self) -> [(String, String, egui::Key); 2] {
        let found: Vec<_> = self
            .items()
            .into_iter()
            .filter_map(|(kind, label, ch)| {
                let key = egui::Key::from_name(&ch?.to_string())?;
                Some((kind, label, key))
            })
            .take(2)
            .collect();
        found
            .try_into()
            .expect("two convert items with letter shortcuts")
    }

    /// 키 주인 `Keys` 통로로 내용을 한 프레임 그린다. 고른 kind 와 칠한 글자를 돌려준다.
    fn frame(&mut self, events: Vec<egui::Event>) -> (Option<String>, Vec<(String, egui::Rect)>) {
        let engine = self.session.read();
        let (state, target, edits) = (&mut self.state, &mut self.target, &self.edits);
        let mut chosen = None;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let out = self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut keys = crate::adapters::ui::popup::keys::PopupKeys::new(
                    ctx,
                    true,
                    crate::adapters::ui::popup::PopupKeyboard::Keys,
                    edits,
                );
                chosen = super::draw_convert_content(ui, state, &engine, target, &mut keys)
                    .map(|super::ConvertResult::Action(a)| chosen_kind(a));
            });
        });
        (
            chosen,
            crate::adapters::ui::popup::window_input_test_support::painted_texts(&out),
        )
    }
}

fn press(ctx: &egui::Context, key: egui::Key, held: Option<Held>) -> egui::Event {
    crate::adapters::ui::popup::window_input_test_support::held_press(ctx, key, held)
}

use crate::adapters::ui::popup::window_input_test_support::Held;

/// T-9a: Given 첫 항목을 고른 변환 팝업, When 수식키 없는 Enter 를 누르면, Then 그 항목으로 바꾼다.
#[test]
fn bare_enter_converts_the_selection() {
    let mut c = Content::new(Some(0));
    let first = c.items()[0].0.clone();
    let press = press(&c.ctx, egui::Key::Enter, None);
    assert_eq!(c.frame(vec![press]).0, Some(first));
}

/// T-9a: Given 첫 항목을 고른 변환 팝업, When Shift·Ctrl·Alt·Cmd(비-macOS Super)+Enter 를 누르면, Then 바꾸지 않는다.
#[test]
fn modified_enter_is_ignored_by_convert() {
    for held in Held::ALL {
        let mut c = Content::new(Some(0));
        let press = press(&c.ctx, egui::Key::Enter, Some(held));
        assert_eq!(c.frame(vec![press]).0, None, "{held:?}");
    }
}

/// T-9a: Given 첫 항목을 고른 변환 팝업, When 수식키 없는 ↓ 를 누르면, Then 다음 항목을 고른다.
#[test]
fn bare_down_moves_the_convert_selection() {
    let mut c = Content::new(Some(0));
    let press = press(&c.ctx, egui::Key::ArrowDown, None);
    c.frame(vec![press]);
    assert_eq!(c.target.selected, Some(1));
}

/// T-9a: Given 첫 항목을 고른 변환 팝업, When Shift·Ctrl·Alt·Cmd(비-macOS Super)+↓ 를 누르면, Then 선택이 그대로다.
#[test]
fn modified_down_is_ignored_by_convert() {
    for held in Held::ALL {
        let mut c = Content::new(Some(0));
        let press = press(&c.ctx, egui::Key::ArrowDown, Some(held));
        c.frame(vec![press]);
        assert_eq!(c.target.selected, Some(0), "{held:?}");
    }
}

/// T-9a-c1: Given 변환 팝업, When 논리 키는 단축키가 아닌 글자이고 물리 키는 한 항목의 단축키인 누름이 오면,
/// Then 물리 키로 그 항목을 고른다.
#[test]
fn a_letter_shortcut_reads_the_physical_key() {
    let mut c = Content::new(None);
    let [(kind, _, key), _] = c.two_shortcuts();
    let used: Vec<char> = c.items().into_iter().filter_map(|(_, _, ch)| ch).collect();
    let logical = ('a'..='z')
        .find(|ch| !used.contains(ch))
        .and_then(|ch| egui::Key::from_name(&ch.to_string()))
        .expect("a letter that is no shortcut");
    let event = egui::Event::Key {
        key: logical,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    assert_eq!(c.frame(vec![event]).0, Some(kind));
}

/// T-9a-c2: Given 변환 팝업, When 한 패스에 단축키 둘이 항목 순서와 반대로 오면, Then 사건 순서상 마지막 것을 고른다.
#[test]
fn the_last_of_two_letter_shortcuts_wins() {
    let mut c = Content::new(None);
    let [(first, _, a), (_, _, b)] = c.two_shortcuts();
    let events = vec![press(&c.ctx, b, None), press(&c.ctx, a, None)];
    assert_eq!(c.frame(events).0, Some(first));
}

/// T-9a-c3: Given 변환 팝업, When 한 패스에 단축키와 다른 항목 클릭이 오면, Then 클릭한 항목을 고른다.
#[test]
fn a_click_overrides_a_letter_shortcut() {
    let mut c = Content::new(None);
    let [(_, _, a), (second, label, _)] = c.two_shortcuts();
    let (_, texts) = c.frame(Vec::new());
    let at = crate::adapters::ui::popup::window_input_test_support::painted_at(&texts, &label)
        .expect("second item label is painted")
        .center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    c.frame(vec![egui::Event::PointerMoved(at)]);
    c.frame(vec![button(true)]);
    let shortcut = press(&c.ctx, a, None);
    assert_eq!(c.frame(vec![button(false), shortcut]).0, Some(second));
}

/// T-9a-c4: Given 변환 팝업, When Cmd(비-macOS Super)+단축키 글자를 누르면, Then 고르지 않는다.
#[test]
fn a_super_letter_shortcut_is_ignored() {
    let mut c = Content::new(None);
    let [(_, _, a), _] = c.two_shortcuts();
    let press = press(&c.ctx, a, Some(Held::Command));
    assert_eq!(c.frame(vec![press]).0, None);
}

/// T-9a-c5: Given 편집 바인딩 하나를 수식키 없는 단축키 글자로 둔 통로, When 그 글자를 누르면, Then 통로가 `Keys`
/// 집합 밖으로 보아 고르지 않는다.
#[test]
fn a_letter_bound_as_an_edit_key_is_left_to_the_text_set() {
    let mut c = Content::new(None);
    let [(_, _, a), _] = c.two_shortcuts();
    let kb = crate::settings::KeybindingSettings {
        copy: vec![a.name().to_lowercase()],
        ..Default::default()
    };
    c.edits = crate::adapters::ui::input::edit_key::EditBindings::from_settings(&kb);
    let press = press(&c.ctx, a, None);
    assert_eq!(c.frame(vec![press]).0, None);
}
