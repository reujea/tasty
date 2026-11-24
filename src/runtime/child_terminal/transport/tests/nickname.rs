use super::*;
use crate::runtime::child_terminal::{ChildEntry, NicknameRefused, ParentKey};

fn named(child: u32, index: u32, nickname: &str) -> ChildEntry {
    let mut value = entry(child, index);
    value.nickname = Some(nickname.into());
    value
}

/// 같은 값을 두 번 정할 때 첫 쓰기만 실패해도 두 번째 성공은 그 값이 디스크에 있다는 뜻이다.
/// 필드 차이만 옮기면 두 번째의 차이가 비어 디스크에 옛 값이 남은 채 성공한다.
#[test]
fn a_repeated_nickname_is_written_even_when_the_earlier_write_failed() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let parent = ParentKey::Surface(10);
    client.register_child(10, named(100, 0, "old"));
    wait(&client.submit().unwrap()).unwrap();
    let (release, gate) = f.gate();
    f.hub
        .panic_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let first = client
        .set_nickname(&parent, 0, 100, Some("new".into()))
        .unwrap();
    let second = client
        .set_nickname(&parent, 0, 100, Some("new".into()))
        .unwrap();
    gate_done(release, gate);
    assert!(wait(&first).is_err());
    assert_eq!(wait(&second), Ok(None));
    let disk = f.disk();
    let saved = disk.find_child(&parent, 0).expect("child on disk");
    assert_eq!(saved.nickname.as_deref(), Some("new"));
    f.drain();
}

/// 확인한 자식과 같은 번호의 다른 자식이 조회 값에 들어와 있으면 고치지 않는다.
#[test]
fn a_nickname_change_refuses_an_index_now_held_by_another_child() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let parent = ParentKey::External("codex:r".into());
    client.register_child(parent.clone(), entry(100, 0));
    wait(&client.submit().unwrap()).unwrap();
    let checked = client.find_child(&parent, 0).expect("child");
    client.remove_child(&parent, 0);
    client.register_child(parent.clone(), named(200, 0, "other"));
    let refused = client.set_nickname(&parent, 0, checked.child_surface_id, Some("x".into()));
    assert!(matches!(refused, Err(NicknameRefused::ChildChanged)));
    let now = client.find_child(&parent, 0).expect("child");
    assert_eq!(now.child_surface_id, 200);
    assert_eq!(now.nickname.as_deref(), Some("other"));
    f.drain();
}

/// 조회 값은 옛 자식을 가리켜도 디스크에서 그 번호가 다른 자식이면 저장하지 않고 실패한다.
#[test]
fn a_nickname_save_fails_when_the_index_moved_on_disk() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let parent = ParentKey::External("codex:r".into());
    client.register_child(parent.clone(), entry(100, 0));
    wait(&client.submit().unwrap()).unwrap();
    let mut moved = f.disk();
    moved.remove_child(&parent, 0);
    moved.register_child(parent.clone(), named(200, 0, "other"));
    std::fs::write(f.path(), serde_json::to_vec(&moved).unwrap()).unwrap();
    let ticket = client
        .set_nickname(&parent, 0, 100, Some("x".into()))
        .unwrap();
    let error = wait(&ticket).unwrap_err();
    assert!(error.contains("no longer surface 100"), "{error}");
    let disk = f.disk();
    let kept = disk.find_child(&parent, 0).expect("child on disk");
    assert_eq!(kept.child_surface_id, 200);
    assert_eq!(kept.nickname.as_deref(), Some("other"));
    f.drain();
}

/// 접수 뒤 쓰기가 실패한 nickname은 조회 값에서 거둔다. 장애가 풀린 뒤 같은 창의 다른 자식 저장에
/// 실패한 값이 섞여 파일에 쓰이지 않는다.
#[test]
fn a_failed_nickname_write_leaves_no_value_for_a_later_save() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let parent = ParentKey::Surface(10);
    client.register_child(10, named(100, 0, "a-old"));
    client.register_child(10, named(200, 1, "b-old"));
    wait(&client.submit().unwrap()).unwrap();
    f.hub
        .panic_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let failed = client
        .set_nickname(&parent, 0, 100, Some("a-bad".into()))
        .unwrap();
    assert!(wait(&failed).is_err());
    let shown = client.find_child(&parent, 0).expect("child");
    assert_eq!(shown.nickname.as_deref(), Some("a-old"));
    let saved = client
        .set_nickname(&parent, 1, 200, Some("b-new".into()))
        .unwrap();
    assert_eq!(wait(&saved), Ok(None));
    let disk = f.disk();
    let a = disk.find_child(&parent, 0).expect("a on disk");
    assert_eq!(a.nickname.as_deref(), Some("a-old"));
    let b = disk.find_child(&parent, 1).expect("b on disk");
    assert_eq!(b.nickname.as_deref(), Some("b-new"));
    f.drain();
}

/// (가) 같은 자식에 실패할 대입 두 건 사이에 다른 자식의 저장이 접수돼도, 실패한 값은 그 저장에 실려
/// 파일에 쓰이지 않고 조회 값에도 남지 않는다.
#[test]
fn a_save_admitted_between_two_failed_nicknames_does_not_carry_them() {
    // 묶음마다 요청 하나: 첫 대입의 결과가 뒤 명령을 기다리지 않고 따로 전해진다.
    let mut f = Fixture::limits(Limits {
        max_items: 1,
        ..Limits::default()
    });
    let mut client = f.client();
    let parent = ParentKey::Surface(10);
    client.register_child(10, named(100, 0, "a-old"));
    client.register_child(10, named(200, 1, "b-old"));
    wait(&client.submit().unwrap()).unwrap();
    let panic_next = || {
        f.hub
            .panic_next
            .store(true, std::sync::atomic::Ordering::SeqCst)
    };
    // writer 순서: 첫 대입, 막는 명령, 둘째 대입. 둘째가 접수된 채 첫 실패만 거둔 때 B 저장이 접수된다.
    let (release_first, first_gate) = f.gate();
    panic_next();
    let first = client
        .set_nickname(&parent, 0, 100, Some("a-bad".into()))
        .unwrap();
    let (entered, between) = mpsc::sync_channel(1);
    let (release_second, hold) = mpsc::sync_channel::<()>(1);
    let second_gate = command::submit(
        Some(&f.parts.writer.handle()),
        1,
        move || {
            entered.send(()).unwrap();
            hold.recv_timeout(Duration::from_secs(5)).unwrap();
        },
        |_| 0,
    )
    .unwrap();
    panic_next();
    let second = client
        .set_nickname(&parent, 0, 100, Some("a-bad".into()))
        .unwrap();
    gate_done(release_first, first_gate);
    between.recv_timeout(Duration::from_secs(5)).unwrap();
    let other = client
        .set_nickname(&parent, 1, 200, Some("b-new".into()))
        .unwrap();
    assert!(wait(&first).is_err());
    gate_done(release_second, second_gate);
    assert!(wait(&second).is_err());
    assert_eq!(wait(&other), Ok(None));
    let shown = client.find_child(&parent, 0).expect("a");
    assert_eq!(shown.nickname.as_deref(), Some("a-old"));
    let disk = f.disk();
    let a = disk.find_child(&parent, 0).expect("a on disk");
    assert_eq!(a.nickname.as_deref(), Some("a-old"));
    let b = disk.find_child(&parent, 1).expect("b on disk");
    assert_eq!(b.nickname.as_deref(), Some("b-new"));
    f.drain();
}

/// (나) 대입이 대기 중일 때 같은 자식에 다른 요청(respawn 설치)이 같은 값을 명시해 저장하면, 앞선 대입이
/// 실패해도 그 값은 조회 값과 파일에 남는다.
#[test]
fn a_failed_nickname_does_not_erase_a_value_a_later_install_named() {
    let mut f = Fixture::new();
    let mut client = f.client();
    let parent = ParentKey::Surface(10);
    client.register_child(10, named(100, 0, "old"));
    wait(&client.submit().unwrap()).unwrap();
    let (release, gate) = f.gate();
    f.hub
        .panic_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let failed = client
        .set_nickname(&parent, 0, 100, Some("new".into()))
        .unwrap();
    // respawn --cwd --nickname new 의 설치: 관계 항목에 명시 값을 대입하고 저장한다.
    client.update_child(&parent, 0, |entry| entry.nickname = Some("new".into()));
    let install = client.submit().unwrap();
    gate_done(release, gate);
    assert!(wait(&failed).is_err());
    assert_eq!(wait(&install), Ok(None));
    let shown = client.find_child(&parent, 0).expect("child");
    assert_eq!(shown.nickname.as_deref(), Some("new"));
    let disk = f.disk();
    let saved = disk.find_child(&parent, 0).expect("child on disk");
    assert_eq!(saved.nickname.as_deref(), Some("new"));
    f.drain();
}
