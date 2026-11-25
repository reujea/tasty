//! 빈 입력 몫 뒤의 붙여넣기와 편집 귀속.
//!
//! - 탐색기 Find 입력칸이 egui 포커스를 가진 채 그리기 중 무대 전환이 일어나면, 도착한 `HeldPastes` 항목이 키를 받지
//!   않는 패스(팝업이 키를 묶은 몫)에서 꺼내져 사라지지 않고 Find 가 키를 받는 일반 패스에서 한 번 들어간다(R2-12r).
//! - 빈 입력 몫 뒤에 들어간 보류 편집(`EditKeyState` 의 붙여넣기)은 앞 빈 패스가 아니라 그 편집이 든 패스가 판정한다
//!   (R2-15).
//!
//! 장면은 엔진에 실제 explorer surface 를 두고 제품 그리기(`draw_egui_panels`)로 그 Find 를 그린다. 패스는 제품 패스와
//! 같은 순서 함수로 돈다(`WindowInput::run_pass`). 분할 직후·몫 필터 전에 창처럼 편집 보류에 패스 입력을 알리고 도착한
//! 붙여넣기를 합친다(`GpuState::take_pass_input`). 창 키는 창처럼 egui 게이트를 거친다(`WindowInput::key_events`).

use std::time::Instant;

use egui::Event;

use super::super::repro::screen;
use super::super::{PANE_L, PANE_R, SURFACE_L, SURFACE_R, WORKSPACE, layout, rect};
use crate::adapters::ui::find_fields_seen;
use crate::adapters::ui::input::edit_key::{EditBindings, Release};
use crate::adapters::ui::popup::frame::draw_popup_layer;
use crate::adapters::ui::popup::window_input_test_support::{PassHook, WindowInput};
use crate::adapters::ui::popup::{PopupInstanceId, PopupScope, defs};
use crate::gfx::gpu::edit_keys::{ClipText, EditKeyOwner, EditKeyState, EguiKey, HeldEvent, KeyCx};
use crate::gfx::gpu::text_selection::HeldPastes;
use crate::model::LogicalPx;
use crate::model::PhysicalRect;
use crate::state::RequestContext;
use winit::keyboard::{Key, KeyCode, ModifiersState, PhysicalKey, SmolStr};

/// L 에 탐색기(apple.txt·banana.txt·cherry.txt), R 에 터미널이 있는 장면. L 탐색기의 Find 입력칸이 egui 포커스를 가진다.
struct FindScene {
    state: RequestContext,
    session: crate::runtime::engine_session::EngineSession,
    ctx: egui::Context,
    input: WindowInput,
    held: HeldPastes,
    /// 창의 편집 키 판정(주 창처럼 `EditKeyOwner::Window` 로 판정한다).
    edit: EditKeyState<Ev>,
    _dir: tempfile::TempDir,
    /// Find 입력칸의 붙여넣기 태그(egui 포커스 위젯 ID 의 `value()`, `GpuState::focus_tag` 와 같은 값).
    tag: u64,
    /// Find 입력칸의 자리.
    field: egui::Rect,
}

/// 판정에 넣는 창 사건: 키 누름.
#[derive(Clone, Debug, PartialEq)]
struct Ev(Key);

impl HeldEvent for Ev {
    fn press(&self) -> Option<(&Key, PhysicalKey)> {
        let code = match &self.0 {
            Key::Character(c) if c == "v" => KeyCode::KeyV,
            _ => KeyCode::KeyK,
        };
        Some((&self.0, PhysicalKey::Code(code)))
    }
    fn queues_behind_hold(&self) -> bool {
        true
    }
    fn is_pointer(&self) -> bool {
        false
    }
    fn is_key(&self) -> bool {
        true
    }
}

fn ev(c: &str) -> Ev {
    Ev(Key::Character(SmolStr::new(c)))
}

/// 패스 하나에서 본 것.
struct Seen {
    /// 몫 필터를 거쳐 egui 에 넣은 사건.
    events: Vec<Event>,
    /// 그 패스의 surface 본문이 키를 받았는지(`surfaces_take_keys`).
    take: bool,
    /// 그 패스가 무대를 그렸는지.
    stage: bool,
}

fn pane_rects() -> Vec<(u32, PhysicalRect)> {
    vec![
        (
            PANE_L,
            rect(
                LogicalPx(0.0),
                LogicalPx(0.0),
                LogicalPx(450.0),
                LogicalPx(680.0),
            )
            .to_physical(1.0),
        ),
        (
            PANE_R,
            rect(
                LogicalPx(450.0),
                LogicalPx(0.0),
                LogicalPx(450.0),
                LogicalPx(680.0),
            )
            .to_physical(1.0),
        ),
    ]
}

/// 엔진의 L 이 실제 explorer surface 인 좌우 두 pane 장면. 포커스는 L 이다(structure_tests 의 장면 준비와 같다).
fn engine_scene(
    dir: &std::path::Path,
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::DomainEvent as E;
    let mut events = vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: WORKSPACE,
            name: "workspace".into(),
            category: 0,
            index: 0,
            pane: PANE_L,
        },
        E::PaneSplit {
            target: PANE_L,
            pane: PANE_R,
            split: tasty_core::SplitSpec {
                direction: crate::model::SplitDirection::Vertical,
                ratio: tasty_core::Ratio::from_f32(0.5),
                placement: tasty_core::Placement::After,
            },
        },
    ];
    for (id, pane, kind) in [
        (SURFACE_L, PANE_L, "explorer"),
        (SURFACE_R, PANE_R, "terminal"),
    ] {
        events.push(E::TabCreated {
            id,
            pane,
            index: 0,
            name: kind.into(),
            surface: tasty_core::SurfaceSpec {
                id,
                kind: kind.into(),
                data: None,
            },
        });
    }
    let (mut state, mut session) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    session.runtime.surfaces.insert(
        SURFACE_L,
        Box::new(crate::model::ExplorerPanel::new(SURFACE_L, dir.into())),
    );
    state
        .navigation
        .select_pane(session.core_state.workspace_at(0).unwrap(), PANE_L);
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    (state, session)
}

/// 창의 egui 패스 하나. 제품 패스와 같은 순서 함수로 돈다(`WindowInput::run_pass`). 분할 직후·몫 필터 전에 편집 보류에
/// 패스 입력을 알리고 도착한 붙여넣기를 합치며(`GpuState::take_pass_input`), 몫 필터가 입력을 빼면 편집 보류를 끝낸다.
/// 그리기는 창처럼 surface 본문(본문 키 게이트 안) → host 팝업 층이거나 무대다. `now` 는 붙여넣기 판정 시각이다.
fn pass(s: &mut FindScene, now: Instant) -> Seen {
    let engine = s.session.borrow_mut();
    let read = engine.read();
    let focused = s.ctx.memory(|m| m.focused()).map_or(0, |id| id.value());
    let edit = &mut s.edit;
    let mut seen = None;
    let (events, output) = s.input.run_pass(
        &mut s.state,
        &s.ctx,
        screen(),
        &mut s.held,
        &mut |hook| match hook {
            PassHook::Taken(held, events) => {
                edit.input_taken(events);
                let pastes = held.frame(events, focused, now);
                events.extend(pastes.into_iter().map(Event::Paste));
            }
            PassHook::Dropped => edit.input_dropped(),
        },
        |ctx, state| {
            let take = state.surfaces_take_keys();
            let stage = state.pass_draws_stage();
            seen.get_or_insert((take, stage));
            if stage {
                crate::adapters::ui::draw_fullscreen_stage(ctx, state, &read);
            } else {
                crate::adapters::ui::popup::body_keys::with_body_keys(ctx, take, || {
                    crate::adapters::ui::draw_egui_panels(ctx, state, &read, &pane_rects(), 1.0);
                });
                draw_popup_layer(ctx, state, &read, &layout());
            }
        },
    );
    let (take, stage) = seen.expect("패스가 그렸다");
    // 창처럼 그 패스의 출력이 보류 편집을 정하고, 앞 입력을 기다리던 편집은 그 자리에서 다시 판정한다
    // (`GpuState::edit_keys_after_pass`).
    drop(s.edit.after_pass(output.platform_output, &s.ctx));
    if s.edit.needs_rejudge() {
        s.edit.set_cuts(s.input.cut_count());
        s.edit.rejudge(
            KeyCx {
                ctx: &s.ctx,
                events: s.input.queue_mut(),
                clipboard: &mut |_| ClipText::Pending,
            },
            now,
        );
    }
    Seen {
        events,
        take,
        stage,
    }
}

/// 남은 몫을 모두 그린다.
fn drain(s: &mut FindScene) -> Vec<Seen> {
    let mut seen = Vec::new();
    while !s.input.is_empty() {
        seen.push(pass(s, Instant::now()));
    }
    seen
}

/// L 탐색기의 지금 보이는 항목 이름. Find 검색어로 거른 결과다.
fn shown(s: &FindScene) -> Vec<String> {
    s.state
        .explorer_views
        .get(SURFACE_L)
        .expect("L 탐색기 view")
        .shown_entries()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

/// 탐색기 Find 를 열어 그 입력칸에 egui 포커스를 둔 장면.
fn find_scene() -> FindScene {
    crate::i18n::init("en");
    let dir = tempfile::tempdir().unwrap();
    for name in ["apple.txt", "banana.txt", "cherry.txt"] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    let (state, session) = engine_scene(dir.path());
    let mut s = FindScene {
        state,
        session,
        ctx: egui::Context::default(),
        input: WindowInput::default(),
        held: HeldPastes::default(),
        edit: EditKeyState::default(),
        _dir: dir,
        tag: 0,
        field: egui::Rect::NOTHING,
    };
    // 첫 그리기가 view 를 만들고 폴더 읽기를 요청한다.
    pass(&mut s, Instant::now());
    let mut reads = crate::app::local_reads::LocalReads::default();
    // 폴더 읽기는 worker 가 한다. 결과가 view 에 들 때까지 기다린다.
    for _ in 0..2000 {
        reads.drive(|requests| s.state.explorer_views.poll_local_reads(requests));
        if !shown(&s).is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    pass(&mut s, Instant::now());
    assert_eq!(shown(&s), ["apple.txt", "banana.txt", "cherry.txt"]);
    s.state
        .explorer_views
        .get_mut(SURFACE_L)
        .expect("L 탐색기 view")
        .open_find();
    for _ in 0..3 {
        pass(&mut s, Instant::now());
    }
    let focused = s
        .ctx
        .memory(|m| m.focused())
        .expect("Find 입력칸이 egui 포커스를 가진다");
    s.tag = focused.value();
    s.field = s.ctx.read_response(focused).expect("Find 입력칸 응답").rect;
    let settings = crate::settings::KeybindingSettings {
        paste: vec!["ctrl+v".into()],
        ..Default::default()
    };
    s.edit
        .set_bindings(Some(EditBindings::from_settings(&settings)));
    assert!(super::super::surface_rect_l().contains(s.field.center()));
    s
}

/// 알림 팝업을 단축키처럼 포커스 없이(passive) 열고 그 전체화면 버튼 자리를 돌려준다.
fn open_notifications(s: &mut FindScene) -> (PopupInstanceId, egui::Rect) {
    let iid = s
        .state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new("notifications", PopupScope::Window),
            true,
        )
        .expect("알림 팝업이 열린다");
    for _ in 0..2 {
        pass(s, Instant::now());
    }
    let button = s
        .state
        .popups
        .instance(iid)
        .and_then(|p| p.fullscreen_btn_rect())
        .expect("알림 팝업에 전체화면 버튼이 있다");
    assert!(!button.intersects(s.field));
    assert_eq!(
        s.ctx.memory(|m| m.focused()).map(|id| id.value()),
        Some(s.tag),
        "passive 로 연 팝업은 Find 의 egui 포커스를 가져가지 않는다"
    );
    (iid, button)
}

fn has_paste(events: &[Event]) -> bool {
    events.iter().any(|e| matches!(e, Event::Paste(_)))
}

/// R2-12r 의 제품 순서: 한 redraw 앞에 [알림 팝업 전체화면 버튼 누름(모델 포커스를 팝업으로 승격) → Find 안 대표 터치
/// 시작(egui 포인터와 포커스를 Find 에) → 원시 사건 없는 수식 키 뗌] 이 오고, 그 패스 A 가 그리기 중 무대를 연다. 첫 무대
/// 패스 전에 무대 종료 키(F8)가 와서 무대를 닫는다. 창 키는 모두 창처럼 egui 게이트를 거친다(L 탐색기는 egui 로 입력을
/// 받는 surface 라 게이트가 열리고, 그 자리의 키 주인을 묶는다). 그래서 수식 키 뗌은 누름이 포커스를 올린 알림 팝업을 그
/// 몫의 키 주인으로 묶고, F8 은 원시 `Key::F8` 사건을 큐에 넣는다. A 를 그리기 전 단계까지 진행한다.
fn press_then_touch_and_release(s: &mut FindScene) {
    let (_, button) = open_notifications(s);
    // 수식 키를 누른 채 앞선 몫의 노드·결속을 비운다.
    assert_eq!(
        modifier_release(s),
        Some(false),
        "앞선 몫에는 키 주인이 없다"
    );
    drain(s);
    {
        let engine = s.session.borrow_mut();
        let read = engine.read();
        s.input.press(&mut s.state, &read, button.center());
        let at = s.field.center();
        s.input.queue_mut().extend([
            Event::Touch {
                device_id: egui::TouchDeviceId(0),
                id: egui::TouchId(0),
                phase: egui::TouchPhase::Start,
                pos: at,
                force: None,
            },
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert_eq!(
        modifier_release(s),
        Some(true),
        "수식 키 뗌이 누름이 포커스한 알림 팝업을 키 주인으로 묶는다"
    );
}

/// 무대 종료 키 F8: 창처럼 끊기 → egui 게이트(원시 `Key::F8` 사건을 큐에 넣음) → 닫기 핸들러.
fn close_stage_with_f8(s: &mut FindScene) {
    let f8 = Event::Key {
        key: egui::Key::F8,
        physical_key: Some(egui::Key::F8),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    assert!(window_key_events(s, vec![f8]).is_some());
    assert!(s.state.close_fullscreen_stage());
}

/// 장면의 한 판: 응답이 `arrive` 번째 패스(0 이 A) 앞에 도착한다. 패스 순서는 A(키 받음) → B1·B2(수식 키 뗌이 끊은
/// 두 몫) → F8 몫 → X 이고, B1·B2 는 알림 팝업이 키를 묶어 surface 본문이 키를 받지 않는다. F8 몫은 무대가 받은
/// 경로 키의 몫이라 팝업을 묶지 않는다(K9 결속은 단계 4 수신자, `window_feeds_key`). 패스마다 (키 받음, 무대,
/// 붙여넣기 있음) 을 돌려준다.
fn run_with_arrival(s: &mut FindScene, arrive: usize) -> Vec<(bool, bool, bool)> {
    press_then_touch_and_release(s);
    assert_eq!(
        s.input.cuts_text(),
        "InputCuts([5, 5])",
        "수식 키 뗌이 같은 자리에서 두 번 끊는다"
    );
    let mut seen = Vec::new();
    for i in 0..6 {
        if i == arrive {
            s.held.add(vec![(s.tag, "banana".into())], Instant::now());
        }
        drop(find_fields_seen::take());
        let p = pass(s, Instant::now());
        if !p.take && !p.stage {
            assert_eq!(
                find_fields_seen::take(),
                [vec![], vec![s.tag]],
                "키가 가려진 패스 {i} 도 목록을 비운 뒤 그린 Find 입력칸(요청 태그)을 등록한다"
            );
        }
        if i == 0 {
            assert_eq!(
                s.state.fullscreen_stage_id(),
                Some(crate::fullscreen_stages::NOTIFICATIONS_STAGE_ID),
                "A 패스가 전체화면 버튼으로 무대를 연다"
            );
            assert_eq!(
                s.ctx.memory(|m| m.focused()).map(|id| id.value()),
                Some(s.tag),
                "대표 터치가 Find 의 egui 포커스를 지킨다"
            );
            close_stage_with_f8(s);
            assert_eq!(
                s.input.cuts_text(),
                "InputCuts([0, 0])",
                "B1 뒤 B2, 봉인한 F8 몫"
            );
        }
        seen.push((p.take, p.stage, has_paste(&p.events)));
        if i < FIRST_OPEN_PASS {
            assert_eq!(
                shown(s),
                ["apple.txt", "banana.txt", "cherry.txt"],
                "패스 {i}"
            );
            assert_eq!(s.held.is_empty(), i < arrive, "패스 {i} 뒤 보류 항목");
        }
    }
    seen
}

/// 팝업이 키를 묶지 않은 첫 패스(F8 몫). 그 앞 B1·B2 는 알림 팝업이 키를 묶었다.
const FIRST_OPEN_PASS: usize = 3;

/// Given 일반 화면의 탐색기 Find 입력칸이 egui 포커스를 가지고 알림 팝업이 passive 로 열려 있을 때(R2-12r), When 위
/// 제품 순서의 응답이 A 앞, A·B1 사이, B1·B2 사이, B2·F8 사이, F8·X 사이 어디에 도착해도, Then 붙여넣기는 알림 팝업이
/// 키를 묶은 B1·B2 패스에서 꺼내져 사라지지 않고, 키를 묶지 않은 패스(F8 몫, 응답이 그 뒤에 왔으면 X)에서 Find 에 한
/// 번 들어간다. 그 뒤 패스는 다시 넣지 않는다.
#[test]
fn a_find_paste_arriving_before_the_passes_whose_keys_a_popup_holds_reaches_the_find_once() {
    for arrive in 0..5 {
        let mut s = find_scene();
        let seen = run_with_arrival(&mut s, arrive);
        let paste_at = arrive.max(FIRST_OPEN_PASS);
        let expected = (0..6)
            .map(|i| (i == 0 || i >= FIRST_OPEN_PASS, false, i == paste_at))
            .collect::<Vec<_>>();
        assert_eq!(
            seen, expected,
            "도착 {arrive}: (키 받음, 무대, 붙여넣기) 패스 순서"
        );
        assert_eq!(
            shown(&s),
            ["banana.txt"],
            "도착 {arrive}: 결속 없는 패스에서 Find 에 한 번 들어간다"
        );
        assert!(s.held.is_empty());
        assert!(s.input.is_empty());
    }
}

/// 창 키 하나: 창처럼 키 분류와 끊기(`key_focus`)를 거친 뒤 편집 판정을 받는다. `clip` 은 그 키가 붙여넣기면 읽을
/// 클립보드다. 판정 결과를 돌려준다.
fn window_key(s: &mut FindScene, event: Ev, modifiers: ModifiersState, clip: ClipText) -> EguiKey {
    {
        let engine = s.session.borrow_mut();
        s.input.key_without_event(&mut s.state, &engine.read());
    }
    s.edit.set_modifiers(modifiers);
    s.edit.set_cuts(s.input.cut_count());
    let mut clip = Some(clip);
    s.edit.judge(
        &event,
        EditKeyOwner::Window,
        Instant::now(),
        KeyCx {
            ctx: &s.ctx,
            events: s.input.queue_mut(),
            clipboard: &mut |_| clip.take().unwrap_or(ClipText::Empty),
        },
    )
}

/// 붙여넣기 키(Ctrl+V). 클립보드가 Wayland 읽기 중이면 편집을 `AwaitText` 로 보류한다.
fn paste_key(s: &mut FindScene, clip: ClipText) -> EguiKey {
    window_key(s, ev("v"), ModifiersState::CONTROL, clip)
}

/// Wayland 읽기가 끝났다(`GpuState::edit_keys_text_arrived`).
fn text_arrived(s: &mut FindScene, text: &str) {
    s.edit.set_cuts(s.input.cut_count());
    s.edit.text_arrived(
        Some(text.into()),
        &s.ctx,
        s.input.queue_mut(),
        Instant::now(),
    );
}

/// 엔진 포커스를 `pane` 으로 옮긴다(egui 포커스는 그대로 Find 에 남는다).
fn activate(s: &mut FindScene, pane: u32) {
    let engine = s.session.borrow_mut();
    let read = engine.read();
    s.state
        .navigation
        .select_pane(read.workspace_at(0).unwrap(), pane);
    s.state.normalize_focus(&read);
}

/// R 의 활성화 좌누름과 같은 redraw 앞의 A Find 안 대표 터치 시작. 터치는 누름 입구를 거치지 않는다.
fn press_r_then_touch_find(s: &mut FindScene) {
    {
        let engine = s.session.borrow_mut();
        s.input.press(
            &mut s.state,
            &engine.read(),
            super::super::surface_rect_r().center(),
        );
    }
    activate(s, PANE_R);
    let at = s.field.center();
    s.input.queue_mut().extend([
        Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(0),
            phase: egui::TouchPhase::Start,
            pos: at,
            force: None,
        },
        Event::PointerMoved(at),
        Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
}

/// 보류 편집이 끝나 창에 돌아온 것.
fn release(s: &mut FindScene) -> Option<Release<Ev>> {
    s.edit.take_release(EditKeyOwner::Window)
}

/// 붙여넣기가 Find 에 한 번 들어가고 원래 키 재생 없이 끝났는지, 뒤에 선 키 K 가 순서대로 돌아왔는지.
fn assert_pasted_once(s: &mut FindScene, last: &Seen) {
    assert!(has_paste(&last.events), "붙여넣기는 이 패스에 든다");
    assert_eq!(shown(s), ["banana.txt"], "Find 에 한 번 들어간다");
    let done = release(s).expect("붙여넣기를 실행한 패스가 보류를 끝낸다");
    assert!(
        done.replay.is_none(),
        "실행한 편집은 원래 키를 재생하지 않는다"
    );
    assert_eq!(
        done.queued_events(),
        [ev("k")],
        "보류 뒤에 선 키는 순서대로 돌아온다"
    );
}

/// 앞 빈 패스는 편집을 판정하지 않는다: 그 패스에는 붙여넣기도 보류 끝도 없다.
fn assert_empty_pass_leaves_the_edit(s: &mut FindScene, empty: &Seen) {
    assert!(
        empty.events.is_empty() && empty.take,
        "결속 없는 빈 준비 몫 패스"
    );
    assert!(release(s).is_none(), "빈 패스가 편집을 미리 끝내지 않는다");
    assert_eq!(shown(s), ["apple.txt", "banana.txt", "cherry.txt"]);
}

/// 보류한 붙여넣기 뒤에 [R 활성화 좌누름 → A Find 안 대표 터치 → 창 키 K] 가 오면 창 키가 노드 차이로 같은 자리에서
/// 두 번 끊는다. 앞 몫에는 두 포인터 입력이 든다.
fn hold_then_pointer_and_key(s: &mut FindScene) -> usize {
    assert_eq!(paste_key(s, ClipText::Pending), EguiKey::Taken);
    assert!(s.edit.awaits_text());
    press_r_then_touch_find(s);
    let at = s.input.queue_mut().len();
    assert_eq!(
        window_key(s, ev("k"), ModifiersState::empty(), ClipText::Empty),
        EguiKey::Taken
    );
    assert_eq!(s.input.cuts_text(), format!("InputCuts([{at}, {at}])"));
    at
}

/// Given 두 surface 가 같은 일반 화면에 있고 A(L 탐색기 Find)로 Wayland 붙여넣기를 요청해 보류했을 때(R2-15), When
/// [R 활성화 좌누름 → A Find 안 대표 터치 → 창 키 K] 뒤 포인터 패스를 그린 다음 응답이 와서 편집이 큐 끝에 들어가면,
/// Then 남은 빈 R 준비 몫 패스는 편집을 판정하지 않고, 편집이 든 다음 패스가 붙여넣기를 실행해 원래 키 없이 끝낸다.
#[test]
fn a_paste_arriving_after_the_pointer_pass_is_judged_in_the_pass_that_holds_it() {
    let mut s = find_scene();
    let at = hold_then_pointer_and_key(&mut s);
    let pointer = pass(&mut s, Instant::now());
    assert_eq!(
        pointer.events.len(),
        at,
        "포인터 패스는 앞 몫의 두 포인터 입력을 그린다"
    );
    assert!(release(&mut s).is_none());
    text_arrived(&mut s, "banana");
    let empty = pass(&mut s, Instant::now());
    assert_empty_pass_leaves_the_edit(&mut s, &empty);
    let last = pass(&mut s, Instant::now());
    assert_pasted_once(&mut s, &last);
    assert!(s.input.is_empty());
}

/// Given 같은 장면에서, When 응답이 포인터 패스 전에 와서 먼저 앞 입력의 패스를 기다리고(`Settle`) 포인터 패스 뒤 재판정이
/// 편집을 큐 끝에 넣으면, Then 남은 빈 R 준비 몫 패스는 편집을 판정하지 않고 편집이 든 다음 패스가 실행한다.
#[test]
fn a_paste_arriving_before_the_pointer_pass_is_judged_in_the_pass_that_holds_it() {
    let mut s = find_scene();
    let at = hold_then_pointer_and_key(&mut s);
    text_arrived(&mut s, "banana");
    assert!(!s.edit.awaits_text(), "응답은 앞 입력의 패스를 기다린다");
    let pointer = pass(&mut s, Instant::now());
    assert_eq!(pointer.events.len(), at);
    assert!(release(&mut s).is_none());
    let empty = pass(&mut s, Instant::now());
    assert_empty_pass_leaves_the_edit(&mut s, &empty);
    let last = pass(&mut s, Instant::now());
    assert_pasted_once(&mut s, &last);
}

/// Given 보류한 붙여넣기 뒤에 포인터 입력 없이 엔진 포커스만 R 로 옮기고 창 키 K 가 와서, 원시 사건 없는 같은 자리에
/// 경계가 둘 남았을 때, When 응답이 바로 편집을 큐에 넣으면, Then 빈 패스 두 개는 편집을 판정하지 않고 셋째 패스가
/// 실행한다(경계 수를 줄이기 전에 0 을 본다).
#[test]
fn a_paste_behind_two_empty_segments_is_judged_in_the_third_pass() {
    let mut s = find_scene();
    assert_eq!(paste_key(&mut s, ClipText::Pending), EguiKey::Taken);
    activate(&mut s, PANE_R);
    assert_eq!(
        window_key(&mut s, ev("k"), ModifiersState::empty(), ClipText::Empty),
        EguiKey::Taken
    );
    assert_eq!(s.input.cuts_text(), "InputCuts([0, 0])");
    text_arrived(&mut s, "banana");
    for _ in 0..2 {
        let empty = pass(&mut s, Instant::now());
        assert_empty_pass_leaves_the_edit(&mut s, &empty);
    }
    let last = pass(&mut s, Instant::now());
    assert_pasted_once(&mut s, &last);
}

/// Given 엔진 포커스가 R 로 옮겨 간 뒤 동기 클립보드로 붙여넣기 키가 와서, 키의 노드 차이가 같은 자리에 빈 경계 둘을
/// 남기고 판정이 곧바로 편집을 큐에 넣었을 때(최초 판정의 `Injected`), When 패스를 그리면, Then 빈 패스는 편집을
/// 판정하지 않고 편집이 든 패스가 실행한다.
#[test]
fn a_paste_injected_at_key_time_behind_empty_segments_is_judged_in_the_pass_that_holds_it() {
    let mut s = find_scene();
    assert_eq!(
        window_key(&mut s, ev("k"), ModifiersState::empty(), ClipText::Empty),
        EguiKey::Free
    );
    activate(&mut s, PANE_R);
    assert_eq!(
        paste_key(&mut s, ClipText::Text("banana".into())),
        EguiKey::Taken
    );
    assert_eq!(s.input.cuts_text(), "InputCuts([0, 0])");
    assert!(
        has_paste(s.input.queue_mut()),
        "판정이 편집을 곧바로 큐에 넣는다"
    );
    for _ in 0..2 {
        let empty = pass(&mut s, Instant::now());
        assert_empty_pass_leaves_the_edit(&mut s, &empty);
    }
    let last = pass(&mut s, Instant::now());
    assert!(has_paste(&last.events));
    assert_eq!(shown(&s), ["banana.txt"]);
    let done = release(&mut s).expect("붙여넣기를 실행한 패스가 보류를 끝낸다");
    assert!(done.replay.is_none());
}

/// Given 경계가 없을 때, When 보류한 붙여넣기의 응답이 편집을 큐에 넣으면, Then 다음 패스가 바로 실행한다(대조).
#[test]
fn a_paste_with_no_segment_ahead_is_judged_in_the_next_pass() {
    let mut s = find_scene();
    assert_eq!(paste_key(&mut s, ClipText::Pending), EguiKey::Taken);
    assert_eq!(s.input.cuts_text(), "InputCuts([])");
    text_arrived(&mut s, "banana");
    let last = pass(&mut s, Instant::now());
    assert!(has_paste(&last.events));
    assert_eq!(shown(&s), ["banana.txt"]);
    let done = release(&mut s).expect("다음 패스가 보류를 끝낸다");
    assert!(done.replay.is_none());
    assert!(done.queued.is_empty());
}

/// Given 붙여넣기 편집이 큐에 들어간 뒤 그 몫의 노드(L)가 없는 무대가 열렸을 때, When 그 몫을 그리면 몫 필터가 편집을
/// 빼므로, Then 보류는 원래 키를 재생하지 않고 끝난다(대상 몫 폐기, `TargetGone`).
#[test]
fn a_paste_whose_segment_is_dropped_ends_without_replaying_the_key() {
    let mut s = find_scene();
    assert_eq!(paste_key(&mut s, ClipText::Pending), EguiKey::Taken);
    text_arrived(&mut s, "banana");
    assert!(
        s.state
            .open_fullscreen_stage(crate::fullscreen_stages::NOTIFICATIONS_STAGE_ID)
    );
    let stage = pass(&mut s, Instant::now());
    assert!(stage.stage);
    assert!(!has_paste(&stage.events), "몫 필터가 편집을 뺀다");
    let done = release(&mut s).expect("버려진 몫이 보류를 끝낸다");
    assert!(
        done.replay.is_none(),
        "버려진 편집은 원래 키를 재생하지 않는다"
    );
}

/// 알림 팝업을 포커스를 주어 열고, 키 하나가 그 팝업을 이번 몫의 키 주인으로 묶게 한다(그 몫의 패스는 surface 본문이
/// 키를 받지 않는다). Find 의 egui 포커스는 그대로다.
fn bind_keys_to_a_focused_popup(s: &mut FindScene) {
    s.state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new("notifications", PopupScope::Window).focused(),
            true,
        )
        .expect("알림 팝업이 열린다");
    for _ in 0..3 {
        pass(s, Instant::now());
    }
    assert_eq!(
        s.ctx.memory(|m| m.focused()).map(|id| id.value()),
        Some(s.tag),
        "Find 가 egui 포커스를 지킨다"
    );
}

/// 다음 키 하나(원시 사건 없는 수식 키 뗌)를 창처럼 egui 게이트에 넣어 포커스 팝업에 묶는다.
fn bound_key(s: &mut FindScene) {
    assert_eq!(modifier_release(s), Some(true), "포커스 팝업이 키 주인이다");
}

/// 원시 사건 없는 수식 키 뗌. 창처럼 끊기 → egui 게이트(L 탐색기는 egui 로 입력을 받는 surface 라 게이트가 열리고 키
/// 주인을 묶는다)를 거친다(`MainView::feed_egui_keyboard`).
fn modifier_release(s: &mut FindScene) -> Option<bool> {
    window_key_events(s, Vec::new())
}

/// 창이 받은 키 사건 하나와 그 원시 사건들. egui 게이트의 surface 판정은 창과 같은 값이다
/// (`feed_egui_keyboard` 의 `consumes_egui_input`).
fn window_key_events(s: &mut FindScene, events: Vec<Event>) -> Option<bool> {
    let engine = s.session.borrow_mut();
    let read = engine.read();
    let egui_surface = s
        .state
        .focused_surface_type(&read)
        .kind_capability(&read, |d| d.consumes_egui_input);
    assert!(egui_surface, "L 탐색기는 egui 로 입력을 받는다");
    s.input
        .key_events(&mut s.state, &read, egui_surface, events)
}

/// Given Find 가 egui 포커스를 가지고 그 태그의 붙여넣기가 정착한 뒤, When 팝업이 키를 묶은 패스가 먼저 오고 그다음
/// 키를 묶지 않은 패스가 오면, Then 붙여넣기는 묶인 패스에서 꺼내지지 않고 다음 열린 패스에서 Find 에 한 번 들어간다.
#[test]
fn a_settled_find_paste_waits_out_a_pass_whose_keys_a_popup_holds() {
    let mut s = find_scene();
    bind_keys_to_a_focused_popup(&mut s);
    let arrived = Instant::now();
    s.held.add(vec![(s.tag, "banana".into())], arrived);
    bound_key(&mut s);
    let closed = pass(&mut s, arrived);
    assert!(!closed.take, "팝업이 이 패스의 키를 묶었다");
    assert!(!has_paste(&closed.events));
    assert!(
        !s.held.is_empty(),
        "묶인 패스는 Find 붙여넣기를 꺼내지 않는다"
    );
    let open = pass(&mut s, arrived);
    assert!(open.take);
    assert!(has_paste(&open.events));
    assert_eq!(shown(&s), ["banana.txt"]);
    assert!(s.held.is_empty());
}

/// Given 같은 장면에서, When 묶인 패스가 붙여넣기의 기다림 한도(`PASTE_LIMIT`)를 넘긴 시각에 오면, Then 그 패스에서
/// 버려지고 다음 열린 패스에도 들어가지 않는다(만료는 보류보다 먼저다).
#[test]
fn a_find_paste_past_its_limit_is_dropped_in_the_pass_whose_keys_a_popup_holds() {
    let mut s = find_scene();
    bind_keys_to_a_focused_popup(&mut s);
    let arrived = Instant::now();
    s.held.add(vec![(s.tag, "banana".into())], arrived);
    bound_key(&mut s);
    let late = arrived + crate::file::drag::PASTE_LIMIT;
    pass(&mut s, late);
    assert!(
        s.held.is_empty(),
        "한도를 넘긴 붙여넣기는 묶인 패스에서 버려진다"
    );
    let open = pass(&mut s, late);
    assert!(!has_paste(&open.events));
    assert_eq!(shown(&s), ["apple.txt", "banana.txt", "cherry.txt"]);
}

/// Given 같은 장면에서, When 정착한 뒤 묶인 패스의 egui 포커스가 Find 가 아니고 다음 열린 패스에서 Find 로 돌아오면,
/// Then 붙여넣기는 묶인 패스에서 버려지고 돌아온 Find 에 뒤늦게 들어가지 않는다(대조).
#[test]
fn a_find_paste_dropped_while_focus_was_elsewhere_does_not_come_back_with_the_focus() {
    let mut s = find_scene();
    bind_keys_to_a_focused_popup(&mut s);
    let arrived = Instant::now();
    s.held.add(vec![(s.tag, "banana".into())], arrived);
    bound_key(&mut s);
    let field = s.ctx.memory(|m| m.focused()).expect("Find 포커스");
    s.ctx.memory_mut(|m| m.surrender_focus(field));
    pass(&mut s, arrived);
    assert!(
        s.held.is_empty(),
        "포커스가 다른 곳인 정착 패스에서 버려진다"
    );
    s.ctx.memory_mut(|m| m.request_focus(field));
    for _ in 0..3 {
        let open = pass(&mut s, arrived);
        assert!(!has_paste(&open.events));
    }
    assert_eq!(shown(&s), ["apple.txt", "banana.txt", "cherry.txt"]);
}

/// Given 명령 팔레트(host 검색창)가 포커스를 가지고 그 입력칸이 egui 포커스를 가질 때, When 그 태그의 붙여넣기가
/// 도착하면, Then 지금처럼 정착한 패스에서 붙여넣기 사건으로 들어간다(Find 가 아닌 항목의 대조).
#[test]
fn a_paste_for_the_command_palette_field_goes_in_as_before() {
    let mut s = find_scene();
    s.state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(
                crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID,
                PopupScope::Window,
            )
            .focused(),
            true,
        )
        .expect("팔레트가 열린다");
    for _ in 0..3 {
        pass(&mut s, Instant::now());
    }
    let palette = s
        .ctx
        .memory(|m| m.focused())
        .map(|id| id.value())
        .expect("팔레트 입력칸이 egui 포커스를 가진다");
    assert_ne!(palette, s.tag);
    s.held.add(vec![(palette, "banana".into())], Instant::now());
    let seen = pass(&mut s, Instant::now());
    assert!(has_paste(&seen.events), "정착한 패스에서 들어간다");
    assert!(s.held.is_empty());
}

/// Given Find 가 egui 포커스를 가지고 팝업 결속이 없을 때, When 그 태그의 정착한 붙여넣기가 있는 채 무대 패스를 그리면,
/// Then 무대 패스는 Find 를 그리지 않으므로 키 게이트가 열려 있어도 항목을 넣지 않고 남긴다. When 재포커스 없이 일반
/// 화면으로 돌아오면, Then egui 가 무대 패스 끝에 Find 의 포커스를 지웠으므로 첫 일반 패스가 태그 불일치로 항목을 버린다.
#[test]
fn a_stage_pass_keeps_a_settled_find_paste_and_the_lost_focus_drops_it() {
    let mut s = find_scene();
    let arrived = Instant::now();
    s.held.add(vec![(s.tag, "banana".into())], arrived);
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(s.state.surfaces_take_keys(), "무대 패스 입구의 결속 없음");
    assert_eq!(
        s.ctx.memory(|m| m.focused()).map(|id| id.value()),
        Some(s.tag),
        "무대 패스 입구의 태그 일치"
    );
    let stage = pass(&mut s, arrived);
    assert!(stage.stage);
    assert!(!has_paste(&stage.events));
    assert!(!s.held.is_empty(), "무대 패스 뒤에도 항목이 남는다");
    assert_eq!(
        s.ctx.memory(|m| m.focused()),
        None,
        "무대 패스가 Find 포커스를 지웠다"
    );
    assert!(s.state.close_fullscreen_stage());
    let back = pass(&mut s, arrived);
    assert!(!back.stage);
    assert!(!has_paste(&back.events));
    assert!(s.held.is_empty(), "태그 불일치로 버린다");
    assert_eq!(shown(&s), ["apple.txt", "banana.txt", "cherry.txt"]);
}
