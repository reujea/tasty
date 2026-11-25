use super::*;
use winit::keyboard::SmolStr;

fn ch(c: &str) -> Key {
    Key::Character(SmolStr::new(c))
}

/// 설정의 `alt` 토큰. macOS에서는 Command다.
fn alt() -> ModifiersState {
    if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        ModifiersState::ALT
    }
}

fn kb(copy: &[&str], cut: &[&str], paste: &[&str]) -> crate::settings::KeybindingSettings {
    let v = |s: &[&str]| s.iter().map(|c| (*c).to_string()).collect();
    crate::settings::KeybindingSettings {
        copy: v(copy),
        cut: v(cut),
        paste: v(paste),
        ..Default::default()
    }
}

/// 바인딩을 옮기면 판정도 따라가고, 지운 키는 편집이 아니다.
#[test]
fn the_binding_follows_the_settings() {
    let settings = kb(&["alt+c"], &["alt+x"], &["alt+v"]);
    assert_eq!(
        edit_binding(&settings, &ch("c"), alt()),
        Some(EditKey::Copy)
    );
    assert_eq!(edit_binding(&settings, &ch("x"), alt()), Some(EditKey::Cut));
    assert_eq!(
        edit_binding(&settings, &ch("v"), alt()),
        Some(EditKey::Paste)
    );
    assert_eq!(edit_binding(&settings, &ch("q"), alt()), None);
    let removed = kb(&[], &[], &["alt+v"]);
    assert_eq!(edit_binding(&removed, &ch("c"), alt()), None);
}

/// 한 키를 여러 편집에 묶으면 복사 → 잘라내기 → 붙여넣기 순서로 하나만 고른다.
#[test]
fn an_overlapping_key_picks_one_edit_in_order() {
    let settings = kb(&["alt+k"], &["alt+k"], &["alt+k"]);
    assert_eq!(
        edit_binding(&settings, &ch("k"), alt()),
        Some(EditKey::Copy)
    );
    let settings = kb(&[], &["alt+k"], &["alt+k"]);
    assert_eq!(edit_binding(&settings, &ch("k"), alt()), Some(EditKey::Cut));
}

/// (바인딩 유무) × (능력) × (편집)의 판정표. 능력은 바인딩이 있을 때만 묻는다.
#[test]
fn verdict_table() {
    assert_eq!(
        verdict(None, |_| panic!("no binding asks nothing")),
        Verdict::NoBinding
    );
    for edit in EditKey::ORDER {
        assert_eq!(verdict(Some(edit), |_| Capability::Yes), Verdict::Act(edit));
        assert_eq!(verdict(Some(edit), |_| Capability::No), Verdict::Pass(edit));
        assert_eq!(
            verdict(Some(edit), |_| Capability::Unknown),
            Verdict::Hold(edit)
        );
        assert_eq!(
            verdict(Some(edit), |asked| {
                assert_eq!(asked, edit, "능력은 고른 편집으로 묻는다");
                Capability::No
            }),
            Verdict::Pass(edit)
        );
    }
}

/// 복사와 붙여넣기를 같은 키에 묶고 선택이 없으면 복사만 판정해 넘기고 붙여넣기를 다시 묻지 않는다.
#[test]
fn copy_and_paste_on_one_key_judges_copy_once() {
    let settings = kb(&["alt+k"], &[], &["alt+k"]);
    let mut asked = Vec::new();
    let v = verdict(edit_binding(&settings, &ch("k"), alt()), |edit| {
        asked.push(edit);
        Capability::No
    });
    assert_eq!(v, Verdict::Pass(EditKey::Copy));
    assert_eq!(asked, vec![EditKey::Copy]);
}

fn held() -> EditKeyHold<&'static str> {
    let mut hold = EditKeyHold::new("press", EditKey::Paste, true);
    hold.queue("release", std::time::Instant::now());
    hold.queue("next", std::time::Instant::now());
    hold
}

/// 동작했으면 원래 키를 버리고, 뒤에 온 이벤트는 받은 순서대로 돌려준다.
#[test]
fn acted_consumes_the_key_and_keeps_the_queue_order() {
    let release = held().resolve(Outcome::Acted);
    assert!(release.replay.is_none());
    assert_eq!(release.queued_events(), vec!["release", "next"]);
}

/// 줄 선 이벤트는 받은 때의 도착 시각을 들고 풀린다. 재생이 늦어도 그 시각은 그대로다.
#[test]
fn the_queue_keeps_each_arrival_time() {
    let start = std::time::Instant::now();
    let later = start + std::time::Duration::from_millis(30);
    let mut hold = EditKeyHold::new("press", EditKey::Paste, true);
    hold.queue("release", start);
    hold.queue("next", later);
    let release = hold.resolve(Outcome::NotActed);
    assert_eq!(release.replay, Some(("press", EditKey::Paste)));
    assert_eq!(
        release.queued_arrivals(),
        vec![("release", start), ("next", later)]
    );
}

/// 동작하지 않았으면 원래 키를 한 번, 뒤 이벤트보다 먼저 재생한다.
#[test]
fn not_acted_replays_the_key_once_before_the_queue() {
    let release = held().resolve(Outcome::NotActed);
    assert_eq!(release.replay, Some(("press", EditKey::Paste)));
    assert_eq!(release.queued_events(), vec!["release", "next"]);
}

/// 시간 초과는 실행하지 않았음이 보장될 때만 재생하고, 대상이 사라지면 재생하지 않는다.
#[test]
fn timeout_and_lost_target_follow_the_rule() {
    assert!(held().resolve(Outcome::TimedOut).replay.is_some());
    let mut consume = EditKeyHold::new("press", EditKey::Copy, false);
    consume.queue("next", std::time::Instant::now());
    let release = consume.resolve(Outcome::TimedOut);
    assert!(release.replay.is_none());
    assert_eq!(release.queued_events(), vec!["next"]);
    let release = held().resolve(Outcome::TargetGone);
    assert!(release.replay.is_none());
    assert_eq!(release.queued_events(), vec!["release", "next"]);
}
