//! 플러그인 편집 보류 시험. manager 의 회신 stub 으로 요청을 만들고 회신을 넣는다.

use std::sync::Arc;

use tasty_plugin_protocol::{CopyTargetWire, EditResultWire};
use tasty_terminal::waker_factory::NoopWakerFactory;
use winit::event::Ime;

use super::*;
use crate::plugin::manager::{PluginCopyTarget, PluginManager};

const MD: &str = "com.tasty.markdown";

fn window(n: u64) -> WindowId {
    WindowId::from(n)
}

/// 원래 키를 대신하는 창 이벤트. 보류는 이벤트 종류를 보지 않는다.
fn key(n: usize) -> WindowEvent {
    WindowEvent::Ime(Ime::Commit(format!("key{n}")))
}

fn mgr() -> PluginManager {
    let mut mgr = PluginManager::with_registries(
        Arc::new(NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.attach_edit_outcome_stub_for_test(MD);
    mgr
}

/// `window` 에 원래 키 `key(0)` 을 붙잡은 Copy 요청을 만든다.
fn hold(
    holds: &mut PluginEditHolds,
    mgr: &mut PluginManager,
    window: WindowId,
    edit: EditKey,
    now: Instant,
) -> u64 {
    let request = mgr
        .begin_edit_request(MD, PluginCopyTarget::Popup(1))
        .expect("stub 은 회신을 약속했다");
    let id = request.edit_id;
    holds.hold(window, request, key(0), edit, now);
    id
}

fn reply(mgr: &mut PluginManager, id: u64, result: EditResultWire, copied: Option<&str>) {
    mgr.receive_edit_outcome_for_test(
        MD,
        CopyTargetWire::Popup { instance_id: 1 },
        id,
        result,
        copied,
    );
}

use super::super::edit_key_replay::ReplayStep;

fn replayed(item: &ResolvedPluginEdit) -> Option<(WindowEvent, EditKey)> {
    item.steps.iter().find_map(|step| match step {
        ReplayStep::Original(event, edit) => Some((event.clone(), *edit)),
        _ => None,
    })
}

/// 줄 선 입력. 플러그인 보류의 줄은 플러그인 단축키 단계 앞에서 붙잡혔으므로 그 단계부터 다시 거친다.
fn queued(item: &ResolvedPluginEdit) -> Vec<WindowEvent> {
    item.steps
        .iter()
        .filter_map(|step| match step {
            ReplayStep::Original(..) => None,
            ReplayStep::Input(event) => Some(event.clone()),
            ReplayStep::View(_) => panic!("플러그인 보류 뒤 입력이 단축키 단계를 건너뛴다"),
        })
        .collect()
}

/// 실행 안 함 회신은 원래 키를 판정한 편집으로 한 번 재생하고, 보류 중 줄 선 입력이 그 뒤에 온다.
#[test]
fn a_not_acted_reply_replays_the_key_before_the_queued_input() {
    let (mut holds, mut mgr, now) = (PluginEditHolds::default(), mgr(), Instant::now());
    let id = hold(&mut holds, &mut mgr, window(1), EditKey::Paste, now);
    assert!(holds.queue(window(1), &key(1)));
    assert!(holds.queue(window(1), &WindowEvent::HoveredFile("a".into())));
    assert!(
        !holds.queue(window(1), &WindowEvent::Moved((0, 0).into())),
        "창 상태는 줄 서지 않는다"
    );
    assert!(!holds.queue(window(2), &key(2)), "다른 창은 보류가 없다");
    reply(&mut mgr, id, EditResultWire::NotActed, Some("x"));
    let resolved = holds.resolve(mgr.take_edit_outcomes());
    assert_eq!(resolved.len(), 1);
    let item = &resolved[0];
    assert_eq!(item.window, window(1));
    assert_eq!(replayed(item), Some((key(0), EditKey::Paste)));
    assert_eq!(
        queued(item),
        vec![key(1), WindowEvent::HoveredFile("a".into())]
    );
    assert_eq!(item.copied, None, "일어나지 않은 편집의 복사는 없다");
    assert!(!holds.queue(window(1), &key(3)), "보류가 풀렸다");
}

/// 실행 회신은 키를 소비하고 복사 글자를 넘긴다. 모름과 대상 사라짐도 재생하지 않는다.
#[test]
fn acted_unknown_and_gone_replies_consume_the_key() {
    let (mut holds, mut mgr, now) = (PluginEditHolds::default(), mgr(), Instant::now());
    let acted = hold(&mut holds, &mut mgr, window(1), EditKey::Cut, now);
    let unknown = hold(&mut holds, &mut mgr, window(2), EditKey::Paste, now);
    hold(&mut holds, &mut mgr, window(3), EditKey::Paste, now);
    reply(&mut mgr, acted, EditResultWire::Acted, Some("hi"));
    reply(&mut mgr, unknown, EditResultWire::Unknown, None);
    mgr.disconnect_edit_outcome_stub_for_test(MD);
    let mut resolved = holds.resolve(mgr.take_edit_outcomes());
    resolved.sort_by_key(|r| r.result.map(|r| r as u8));
    let got: Vec<_> = resolved
        .iter()
        .map(|r| (r.window, r.result, replayed(r).is_some(), r.copied.clone()))
        .collect();
    let mut want = vec![
        (
            window(1),
            Some(PluginEditResult::Acted),
            false,
            Some("hi".to_string()),
        ),
        (window(2), Some(PluginEditResult::Unknown), false, None),
        (window(3), Some(PluginEditResult::Gone), false, None),
    ];
    want.sort_by_key(|r| r.1.map(|r| r as u8));
    assert_eq!(got, want);
}

/// 회신이 상한 안에 오지 않으면 키를 소비하고(실행됐을 수 있다) 줄 선 입력을 돌려주며, 그 요청은 더 기다리지
/// 않는다. 늦게 온 회신은 보류가 없어 버린다.
#[test]
fn a_hold_without_a_reply_times_out_without_replaying() {
    let (mut holds, mut mgr, now) = (PluginEditHolds::default(), mgr(), Instant::now());
    let id = hold(&mut holds, &mut mgr, window(1), EditKey::Paste, now);
    assert!(holds.queue(window(1), &key(1)));
    assert_eq!(holds.deadline(), Some(now + PLUGIN_HOLD_LIMIT));
    let (early, cancelled) = holds.expire(now + PLUGIN_HOLD_LIMIT - Duration::from_millis(1));
    assert!(early.is_empty() && cancelled.is_empty());

    let (late, cancelled) = holds.expire(now + PLUGIN_HOLD_LIMIT);
    assert_eq!(cancelled, vec![id]);
    assert_eq!(late.len(), 1);
    assert_eq!(replayed(&late[0]), None);
    assert_eq!(queued(&late[0]), vec![key(1)]);
    assert_eq!(holds.deadline(), None);

    mgr.cancel_edit_request(id);
    reply(&mut mgr, id, EditResultWire::NotActed, None);
    assert!(holds.resolve(mgr.take_edit_outcomes()).is_empty());
}

/// 대상 팝업이 닫히면 시한 전에 보류가 끝난다. 원래 키는 재생하지 않고 줄 선 입력은 바로 재생하며, 시한에 다시
/// 풀리지 않으므로 시간 초과 경고도 없다. 열린 대상의 보류는 그대로 둔다.
#[test]
fn a_closed_target_ends_its_hold_before_the_time_limit() {
    let (mut holds, mut mgr, now) = (PluginEditHolds::default(), mgr(), Instant::now());
    let id = hold(&mut holds, &mut mgr, window(1), EditKey::Paste, now);
    assert!(holds.queue(window(1), &key(1)));

    let (ended, cancelled) = holds.end_lost_targets(|_, _| true);
    assert!(ended.is_empty() && cancelled.is_empty());
    assert!(holds.deadline().is_some());

    let (ended, cancelled) =
        holds.end_lost_targets(|_, target| *target != PluginCopyTarget::Popup(1));
    assert_eq!(cancelled, vec![id]);
    assert_eq!(ended.len(), 1);
    assert_eq!(replayed(&ended[0]), None);
    assert_eq!(queued(&ended[0]), vec![key(1)]);
    assert_eq!(holds.deadline(), None);
    let (late, expired) = holds.expire(now + PLUGIN_HOLD_LIMIT);
    assert!(late.is_empty() && expired.is_empty());
    assert!(
        !holds.queue(window(1), &key(2)),
        "끝난 보류 뒤에 줄 서지 않는다"
    );
}

/// 같은 pump 에 대상 팝업이 닫히고 회신도 왔으면 닫힘이 먼저다. 실행 안 함 회신도 원래 키를 재생하지 않고, 실행
/// 회신의 복사 글자도 쓰지 않는다. 줄 선 입력만 재생한다.
#[test]
fn a_reply_from_a_target_closed_in_the_same_pump_is_dropped() {
    let (mut holds, mut mgr, now) = (PluginEditHolds::default(), mgr(), Instant::now());
    let not_acted = hold(&mut holds, &mut mgr, window(1), EditKey::Copy, now);
    assert!(holds.queue(window(1), &key(1)));
    let acted = hold(&mut holds, &mut mgr, window(2), EditKey::Copy, now);
    reply(&mut mgr, not_acted, EditResultWire::NotActed, None);
    reply(&mut mgr, acted, EditResultWire::Acted, Some("hi"));

    let (resolved, mut cancelled) = holds.settle(mgr.take_edit_outcomes(), now, |_, _| false);
    cancelled.sort();
    let mut want = vec![not_acted, acted];
    want.sort();
    assert_eq!(cancelled, want);
    assert_eq!(resolved.len(), 2);
    for item in &resolved {
        assert_eq!(replayed(item), None, "닫힌 대상에 보낸 키를 재생했다");
        assert_eq!(item.copied, None, "닫힌 대상의 복사 글자를 썼다");
        assert_eq!(item.result, None, "회신이 아니라 닫힘으로 끝난다");
        assert_eq!(item.target, PluginCopyTarget::Popup(1));
    }
    let first = resolved
        .iter()
        .find(|r| r.window == window(1))
        .expect("창 1");
    assert_eq!(queued(first), vec![key(1)]);
}

/// 회신을 기다리는 동안 다른 팝업이 그 창의 키를 받게 됐으면, 대상이 열려 있어도 그 회신은 재생·복사 없이 끝난다.
/// 키 소유자가 그대로면 회신대로 재생한다.
#[test]
fn a_reply_after_another_popup_took_the_keys_is_dropped() {
    let open: HashSet<u64> = [1, 2].into_iter().collect();
    for (owner, replays) in [(Some(2), false), (None, false), (Some(1), true)] {
        let (mut holds, mut mgr, now) = (PluginEditHolds::default(), mgr(), Instant::now());
        let id = hold(&mut holds, &mut mgr, window(1), EditKey::Copy, now);
        reply(&mut mgr, id, EditResultWire::NotActed, None);
        let (resolved, _) = holds.settle(mgr.take_edit_outcomes(), now, |_, target| match target {
            PluginCopyTarget::Popup(instance) => popup_target_live(Some(&open), owner, *instance),
            PluginCopyTarget::Surface(_) => true,
        });
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            replayed(&resolved[0]).is_some(),
            replays,
            "키 소유자 {owner:?}"
        );
    }
}

/// 팝업 대상은 열려 있고 그 창의 키를 받을 때만 살아 있다.
#[test]
fn a_popup_target_lives_only_while_open_and_owning_the_keys() {
    let open: HashSet<u64> = [1].into_iter().collect();
    assert!(popup_target_live(Some(&open), Some(1), 1));
    assert!(
        !popup_target_live(Some(&open), Some(2), 1),
        "다른 팝업이 키를 받는다"
    );
    assert!(
        !popup_target_live(Some(&open), None, 1),
        "호스트 팝업이 키를 받는다"
    );
    assert!(
        !popup_target_live(Some(&HashSet::new()), Some(1), 1),
        "닫혔다"
    );
    assert!(
        !popup_target_live(None, Some(1), 1),
        "플러그인 관리자가 없다"
    );
}

/// 편집 결과 복사의 토스트는 surface 의 편집이면 그 surface 에, 팝업의 편집이면 창에 뜬다.
#[test]
fn a_surface_copy_toast_stays_in_that_surface() {
    use crate::adapters::ui::ToastScope;
    assert_eq!(
        copy_scope(PluginCopyTarget::Surface(5)),
        ToastScope::Surface(5)
    );
    assert_eq!(copy_scope(PluginCopyTarget::Popup(1)), ToastScope::Window);
}

/// 키 없이 보낸 편집(팔레트)은 창 입력을 줄 세우지 않는다. 회신이 오면 재생할 것 없이 복사 글자만 돌려주고,
/// 대상을 잃거나 시간이 지나면 결과 없이 요청을 끝낸다. 시간 초과 시각에도 든다.
#[test]
fn a_keyless_edit_queues_nothing_and_returns_only_the_copy() {
    let mut mgr = mgr();
    let mut holds = PluginEditHolds::default();
    let now = Instant::now();
    let request = mgr
        .begin_edit_request(MD, PluginCopyTarget::Popup(1))
        .expect("stub 은 회신을 약속했다");
    let id = request.edit_id;
    holds.track(window(1), request, EditKey::Copy, now);
    assert!(
        !holds.queue(window(1), &key(1)),
        "키 없는 요청이 입력을 줄 세웠다"
    );
    assert_eq!(holds.deadline(), Some(now + PLUGIN_HOLD_LIMIT));
    reply(&mut mgr, id, EditResultWire::Acted, Some("abc"));
    let (resolved, cancelled) = holds.settle(mgr.take_edit_outcomes(), now, |_, _| true);
    assert!(cancelled.is_empty());
    let [item] = resolved.as_slice() else {
        panic!("풀린 요청이 하나가 아니다: {}", resolved.len());
    };
    assert_eq!(item.window, window(1));
    assert!(item.steps.is_empty());
    assert_eq!(item.copied.as_deref(), Some("abc"));
    assert_eq!(holds.deadline(), None);

    for lost in [true, false] {
        let request = mgr
            .begin_edit_request(MD, PluginCopyTarget::Popup(1))
            .expect("stub 은 회신을 약속했다");
        let id = request.edit_id;
        holds.track(window(2), request, EditKey::Cut, now);
        let at = if lost { now } else { now + PLUGIN_HOLD_LIMIT };
        let (resolved, cancelled) = holds.settle(Vec::new(), at, |_, _| !lost);
        assert!(resolved.is_empty());
        assert_eq!(cancelled, vec![id], "lost={lost}");
        assert_eq!(holds.deadline(), None);
    }
}
