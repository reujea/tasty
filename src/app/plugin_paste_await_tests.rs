//! 글자를 기다리는 플러그인 붙여넣기 시험. manager 의 회신 stub 으로 요청을 만든다.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tasty_plugin_protocol::{CopyTargetWire, EditRequestWire, EditResultWire, RawInputEventWire};
use tasty_terminal::waker_factory::NoopWakerFactory;
use winit::event::{ElementState, Ime, WindowEvent};
use winit::window::WindowId;

use super::super::super::edit_key_replay::ReplayStep;
use super::super::{PluginEditHolds, ResolvedPluginEdit};
use super::AwaitSettled;
use crate::adapters::ui::input::edit_key::EditKey;
use crate::gfx::gpu::edit_keys::AWAIT_TEXT_LIMIT;
use crate::gfx::gpu::plugin_paste_read::PluginPasteEnd;
use crate::plugin::manager::{PluginCopyTarget, PluginManager};

const DEMO: &str = "com.tasty.mesh-demo";
const TAG: u64 = 77;
const SURFACE: PluginCopyTarget = PluginCopyTarget::Surface(5);

fn window() -> WindowId {
    WindowId::from(1)
}

/// 원래 키(0)와 그 뒤의 키를 대신하는 창 이벤트.
fn key(n: usize) -> WindowEvent {
    WindowEvent::Ime(Ime::Commit(format!("key{n}")))
}

fn mgr() -> PluginManager {
    let mut mgr = PluginManager::with_registries(
        Arc::new(NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.attach_edit_outcome_stub_for_test(DEMO);
    mgr
}

/// surface 에 원래 키 `key(0)`(팔레트면 없음)을 붙잡은 기다림을 만들고 요청 id 를 돌려준다.
fn await_paste(
    holds: &mut PluginEditHolds,
    mgr: &mut PluginManager,
    keyed: bool,
    format_paste: bool,
    now: Instant,
) -> u64 {
    let request = mgr
        .begin_edit_request(DEMO, SURFACE)
        .expect("stub 은 회신을 약속했다");
    let id = request.edit_id;
    let original = keyed.then(|| key(0));
    holds.await_paste(window(), request, TAG, original, format_paste, now);
    id
}

fn settle(
    holds: &mut PluginEditHolds,
    mgr: &mut PluginManager,
    arrivals: Vec<(WindowId, u64, PluginPasteEnd)>,
    now: Instant,
    live: bool,
) -> AwaitSettled {
    let mut outcomes = mgr.take_edit_outcomes();
    let out = holds.settle_awaits(&mut outcomes, arrivals, now, &|_, _| live);
    assert!(outcomes.is_empty(), "기다림의 결과가 회신 보류로 넘어갔다");
    out
}

fn arrived(end: PluginPasteEnd) -> Vec<(WindowId, u64, PluginPasteEnd)> {
    vec![(window(), TAG, end)]
}

/// 재생할 원래 키와 줄 선 입력.
fn steps(item: &ResolvedPluginEdit) -> (Option<WindowEvent>, Vec<WindowEvent>) {
    let mut original = None;
    let mut queued = Vec::new();
    for step in &item.steps {
        match step {
            ReplayStep::Original(event, edit) => {
                assert_eq!(*edit, EditKey::Paste);
                original = Some(event.clone());
            }
            ReplayStep::Input(event) => queued.push(event.clone()),
            ReplayStep::View(_) => panic!("기다림 뒤 입력이 단축키 단계를 건너뛴다"),
        }
    }
    (original, queued)
}

fn only(out: &AwaitSettled) -> &ResolvedPluginEdit {
    let [item] = out.resolved.as_slice() else {
        panic!("끝난 기다림이 하나가 아니다: {}", out.resolved.len());
    };
    item
}

/// 글자가 오면 키 시점에 받은 id 로 글자 붙여넣기 요청을 그 surface 에 붙이고 회신 보류로 옮긴다. 기다리는 동안
/// 줄 선 입력은 회신 보류 뒤에 그대로 서서, 회신이 `not_acted`면 원래 키 다음에 재생된다.
#[test]
fn arrived_text_is_sent_and_the_key_moves_to_the_reply_hold_with_its_queue() {
    let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
    let id = await_paste(&mut holds, &mut mgr, true, false, now);
    assert!(
        holds.queue(window(), &key(1)),
        "기다리는 창의 키가 줄을 서지 않았다"
    );
    let out = settle(
        &mut holds,
        &mut mgr,
        arrived(PluginPasteEnd::Text("hi".into())),
        now,
        true,
    );
    assert!(out.resolved.is_empty() && out.cancelled.is_empty() && out.reads.is_empty());
    assert_eq!(
        out.sends,
        vec![(
            window(),
            SURFACE,
            RawInputEventWire::EditRequest {
                edit_id: id,
                edit: EditRequestWire::PasteText { text: "hi".into() },
            }
        )]
    );
    assert!(
        holds.queue(window(), &key(2)),
        "회신 보류가 입력을 줄 세우지 않는다"
    );
    mgr.receive_edit_outcome_for_test(
        DEMO,
        CopyTargetWire::Surface { surface_id: 5 },
        id,
        EditResultWire::NotActed,
        None,
    );
    let (resolved, _) = holds.settle(mgr.take_edit_outcomes(), now, |_, _| true);
    let [item] = resolved.as_slice() else {
        panic!("회신 보류가 풀리지 않았다");
    };
    let mut order = Vec::new();
    for step in &item.steps {
        match step {
            ReplayStep::Original(event, _) | ReplayStep::Input(event) => order.push(event.clone()),
            ReplayStep::View(_) => panic!("단계"),
        }
    }
    assert_eq!(order, vec![key(0), key(1), key(2)]);
}

/// 글자 없이 끝나면 원래 키를 한 번 재생하고 줄 선 입력이 그 뒤에 온다. 요청은 보내지 않았으므로 취소한다.
/// `egui_paste` surface 는 글자 없이도 붙일 수 있어 형식 붙여넣기를 보낸다.
#[test]
fn nothing_to_paste_replays_the_key_unless_the_surface_takes_formats() {
    let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
    let id = await_paste(&mut holds, &mut mgr, true, false, now);
    holds.queue(window(), &key(1));
    let out = settle(
        &mut holds,
        &mut mgr,
        arrived(PluginPasteEnd::Nothing),
        now,
        true,
    );
    assert_eq!(steps(only(&out)), (Some(key(0)), vec![key(1)]));
    assert_eq!(out.cancelled, vec![id]);
    assert!(out.reads.is_empty(), "끝난 읽기를 다시 취소한다");
    assert!(out.sends.is_empty());

    let id = await_paste(&mut holds, &mut mgr, true, true, now);
    let out = settle(
        &mut holds,
        &mut mgr,
        arrived(PluginPasteEnd::Nothing),
        now,
        true,
    );
    assert!(out.resolved.is_empty());
    assert_eq!(
        out.sends,
        vec![(
            window(),
            SURFACE,
            RawInputEventWire::EditRequest {
                edit_id: id,
                edit: EditRequestWire::Paste,
            }
        )]
    );
}

/// 다른 읽기가 대신했으면 실행 여부와 무관하게 원래 키를 재생하지 않고 줄 선 입력만 재생한다.
#[test]
fn a_replaced_read_drops_the_key_and_replays_the_queue() {
    let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
    await_paste(&mut holds, &mut mgr, true, true, now);
    holds.queue(window(), &key(1));
    let out = settle(
        &mut holds,
        &mut mgr,
        arrived(PluginPasteEnd::Replaced),
        now,
        true,
    );
    assert_eq!(steps(only(&out)), (None, vec![key(1)]));
    assert!(out.sends.is_empty());
}

/// 대상이 닫히거나 키를 잃으면 읽기를 취소하고 원래 키를 재생하지 않는다. 같은 회차에 읽기가 끝났어도 대상
/// 상실이 앞선다(글자를 보내지도, 키를 재생하지도 않는다).
#[test]
fn a_lost_target_ends_the_await_before_its_arrival() {
    let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
    let id = await_paste(&mut holds, &mut mgr, true, false, now);
    let out = settle(
        &mut holds,
        &mut mgr,
        arrived(PluginPasteEnd::Nothing),
        now,
        false,
    );
    assert_eq!(steps(only(&out)), (None, Vec::new()));
    assert_eq!(out.cancelled, vec![id]);
    assert_eq!(out.reads, vec![(window(), TAG)]);
    assert!(out.sends.is_empty());
}

/// 시한까지 읽기가 끝나지 않으면 읽기를 취소하고 원래 키를 재생한다(플러그인에 아무것도 보내지 않았다). 깨어날
/// 시각에 시한이 들어간다.
#[test]
fn an_await_past_its_limit_cancels_the_read_and_replays_the_key() {
    let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
    let id = await_paste(&mut holds, &mut mgr, true, false, now);
    assert_eq!(holds.deadline(), Some(now + AWAIT_TEXT_LIMIT));
    let early = now + AWAIT_TEXT_LIMIT - Duration::from_millis(1);
    let out = settle(&mut holds, &mut mgr, Vec::new(), early, true);
    assert!(out.resolved.is_empty());
    let out = settle(
        &mut holds,
        &mut mgr,
        Vec::new(),
        now + AWAIT_TEXT_LIMIT,
        true,
    );
    assert_eq!(steps(only(&out)), (Some(key(0)), Vec::new()));
    assert_eq!(out.cancelled, vec![id]);
    assert_eq!(out.reads, vec![(window(), TAG)]);
    assert_eq!(holds.deadline(), None);
}

/// 창 포커스 상실과 포인터 누름은 기다림을 끝낸다. 읽기를 취소하고 원래 키는 재생하지 않으며 줄 선 입력만 재생한다.
/// 포인터 이동은 줄 선 입력이 없으면 그대로 지나가고, 있으면 그 뒤에 선다.
#[test]
fn focus_loss_and_a_pointer_press_end_the_await_without_the_key() {
    let press = WindowEvent::MouseInput {
        device_id: winit::event::DeviceId::dummy(),
        state: ElementState::Pressed,
        button: winit::event::MouseButton::Left,
    };
    let moved = WindowEvent::CursorLeft {
        device_id: winit::event::DeviceId::dummy(),
    };
    for end in [WindowEvent::Focused(false), press] {
        let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
        let id = await_paste(&mut holds, &mut mgr, true, false, now);
        assert!(holds.interrupt_await(window(), &moved).is_none());
        assert!(
            !holds.queue(window(), &moved),
            "줄 선 것 없이 포인터가 섰다"
        );
        assert!(holds.queue(window(), &key(1)));
        assert!(
            holds.queue(window(), &moved),
            "포인터가 줄 선 키를 앞질렀다"
        );
        assert!(
            holds
                .interrupt_await(window(), &WindowEvent::Focused(true))
                .is_none()
        );
        let out = holds
            .interrupt_await(window(), &end)
            .expect("기다림이 끝나야 한다");
        assert_eq!(steps(only(&out)), (None, vec![key(1), moved.clone()]));
        assert_eq!(out.cancelled, vec![id]);
        assert_eq!(out.reads, vec![(window(), TAG)]);
        assert!(
            !holds.queue(window(), &key(2)),
            "끝난 기다림이 다시 줄 세웠다"
        );
        let out = settle(
            &mut holds,
            &mut mgr,
            arrived(PluginPasteEnd::Text("late".into())),
            now,
            true,
        );
        assert!(out.sends.is_empty(), "포커스를 되찾은 뒤 늦게 붙여넣었다");
    }
}

/// 기다리는 동안 플러그인 실행이 끝나면(종료·재시작) 그 요청의 `Gone`으로 기다림을 끝낸다. 새 실행에 이전
/// 붙여넣기를 보내지 않고 원래 키도 재생하지 않는다.
#[test]
fn a_plugin_run_that_ends_while_waiting_gets_nothing() {
    let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
    let id = await_paste(&mut holds, &mut mgr, true, false, now);
    mgr.disconnect_edit_outcome_stub_for_test(DEMO);
    mgr.attach_edit_outcome_stub_for_test(DEMO);
    let out = settle(
        &mut holds,
        &mut mgr,
        arrived(PluginPasteEnd::Text("hi".into())),
        now,
        true,
    );
    assert!(out.sends.is_empty(), "새 실행에 이전 붙여넣기를 보냈다");
    assert_eq!(steps(only(&out)), (None, Vec::new()));
    assert_eq!(out.cancelled, vec![id]);
}

/// 팔레트의 붙여넣기는 키 없이 기다린다. 기다리는 동안 그 창의 키는 줄을 서고, 글자가 오면 키 없는 요청으로
/// 기록한 뒤 줄 선 입력을 바로 순서대로 재생한다. 글자 없이 끝나면 줄 선 입력만 재생한다.
#[test]
fn a_keyless_await_queues_input_and_releases_it_when_it_ends() {
    let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
    let id = await_paste(&mut holds, &mut mgr, false, false, now);
    assert!(holds.queue(window(), &key(1)));
    let out = settle(
        &mut holds,
        &mut mgr,
        arrived(PluginPasteEnd::Text("hi".into())),
        now,
        true,
    );
    assert_eq!(out.sends.len(), 1);
    assert_eq!(steps(only(&out)), (None, vec![key(1)]));
    assert!(
        !holds.queue(window(), &key(2)),
        "키 없는 요청이 입력을 줄 세웠다"
    );
    mgr.receive_edit_outcome_for_test(
        DEMO,
        CopyTargetWire::Surface { surface_id: 5 },
        id,
        EditResultWire::Acted,
        None,
    );
    let (resolved, _) = holds.settle(mgr.take_edit_outcomes(), now, |_, _| true);
    assert_eq!(resolved.len(), 1, "키 없는 요청이 회신으로 풀리지 않았다");

    await_paste(&mut holds, &mut mgr, false, false, now);
    holds.queue(window(), &key(3));
    let out = settle(
        &mut holds,
        &mut mgr,
        arrived(PluginPasteEnd::Nothing),
        now,
        true,
    );
    assert_eq!(steps(only(&out)), (None, vec![key(3)]));
}

/// 다른 tag 의 끝난 읽기는 이 기다림의 것이 아니다.
#[test]
fn an_arrival_of_another_read_is_dropped() {
    let (mut mgr, mut holds, now) = (mgr(), PluginEditHolds::default(), Instant::now());
    await_paste(&mut holds, &mut mgr, true, false, now);
    let out = settle(
        &mut holds,
        &mut mgr,
        vec![(window(), TAG + 1, PluginPasteEnd::Text("x".into()))],
        now,
        true,
    );
    assert!(out.sends.is_empty() && out.resolved.is_empty());
    assert!(holds.queue(window(), &key(1)), "기다림이 끝났다");
}
