//! 빈 입력 몫 뒤에 들어간 보류 편집(`EditKeyState` 의 붙여넣기)은 앞 빈 패스가 아니라 그 편집이 든 패스가 판정한다
//! (R2-15).
//!
//! 장면은 엔진에 실제 explorer surface 를 두고 제품 그리기(`draw_egui_panels`)로 그 Find 를 그린다. 패스 입력은 창과
//! 같은 분할로 꺼내고, 분할 직후·몫 필터 전에 창처럼 편집 보류에 패스 입력을 알리고 도착한 붙여넣기를 합친다
//! (`WindowInput::take_pass_observed`, `GpuState::take_pass_input`).

use std::time::Instant;

use egui::Event;

use super::super::repro::screen;
use super::super::{PANE_L, PANE_R, SURFACE_L, SURFACE_R, WORKSPACE, layout, rect};
use crate::adapters::ui::input::edit_key::{EditBindings, Release};
use crate::adapters::ui::popup::defs;
use crate::adapters::ui::popup::frame::draw_popup_layer;
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
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

/// 창의 egui 패스 하나. 입력을 창과 같은 분할로 꺼내고 분할 직후·몫 필터 전에 도착한 붙여넣기를 합친 뒤
/// (`GpuState::take_pass_input`), 창처럼 surface 본문(본문 키 게이트 안) → host 팝업 층을 그리거나 무대를 그린다.
/// 패스를 마치면 창처럼 몫을 닫고 그리기 중 전환을 처리한다(`WindowInput::close_pass`). `now` 는 붙여넣기 판정 시각이다.
fn pass(s: &mut FindScene, now: Instant) -> Seen {
    let engine = s.session.borrow_mut();
    let read = engine.read();
    let screen_drawn = s.state.fullscreen_stage_id();
    let focused = s.ctx.memory(|m| m.focused()).map_or(0, |id| id.value());
    let (held, edit) = (&mut s.held, &mut s.edit);
    let events = s
        .input
        .take_pass_observed(&mut s.state, &s.ctx, &mut |_, events| {
            edit.input_taken(events);
            let pastes = held.frame(events, focused, now);
            events.extend(pastes.into_iter().map(Event::Paste));
        });
    // 몫 필터 뒤: 버려진 몫의 편집은 대상이 사라진 것으로 끝난다(`run_main_pass`).
    if s.input.last_pass_dropped() {
        s.edit.input_dropped();
    }
    let take = s.state.surfaces_take_keys();
    let stage = s.state.pass_draws_stage();
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        events: events.clone(),
        ..Default::default()
    };
    let state = &mut s.state;
    let output = s.ctx.run(raw, |ctx| {
        if stage {
            crate::adapters::ui::draw_fullscreen_stage(ctx, state, &read);
        } else {
            let take = state.surfaces_take_keys();
            crate::adapters::ui::popup::body_keys::with_body_keys(ctx, take, || {
                crate::adapters::ui::draw_egui_panels(ctx, state, &read, &pane_rects(), 1.0);
            });
            draw_popup_layer(ctx, state, &read, &layout());
        }
    });
    s.input.close_pass(state, &s.ctx, screen_drawn);
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

fn has_paste(events: &[Event]) -> bool {
    events.iter().any(|e| matches!(e, Event::Paste(_)))
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
        done.queued,
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
