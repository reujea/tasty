//! 이전 실행이 실행기에 남아 있는 전역 훅의 건너뜀. 시험용 소형 실행기에 붙잡는 셸 명령을 넣어 본다.

use super::*;
use crate::hook_runtime::shell_pool::ShellPool;
use std::path::Path;

/// interval 시험 훅의 주기. 틱 사이에는 [`past_period`]만큼 잔다.
const PERIOD: Duration = Duration::from_millis(10);

fn past_period() {
    std::thread::sleep(PERIOD + Duration::from_millis(5));
}

/// 시작하면 `order`에 `name`을 적고 `release` 파일이 생길 때까지 도는 명령.
/// 임시 폴더가 지워지거나 60초가 지나면 스스로 끝난다.
fn held(dir: &Path, name: &str) -> String {
    let d = dir.display();
    format!(
        "echo {name} >> '{d}/order'; i=0; \
         while [ ! -e '{d}/release' ] && [ -d '{d}' ] && [ $i -lt 6000 ]; do sleep 0.01; i=$((i+1)); done"
    )
}

/// 시험이 끝나거나 실패해도 붙잡은 명령을 놓아준 뒤 임시 폴더를 지운다.
struct Held(Option<tempfile::TempDir>);

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(tmp) = self.0.take() {
            if let Err(e) = std::fs::write(tmp.path().join("release"), "") {
                tracing::warn!("could not release the held commands: {e}");
            }
            drop(tmp);
        }
    }
}

fn setup() -> (Held, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    (Held(Some(tmp)), dir)
}

fn release(dir: &Path) {
    std::fs::write(dir.join("release"), "").expect("release");
}

/// 다음 실행을 다시 붙잡는다. 앞 실행이 모두 끝난 뒤에만 부른다.
fn hold_again(dir: &Path) {
    std::fs::remove_file(dir.join("release")).expect("remove release");
}

fn order(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("order"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn wait_order(dir: &Path, expected: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(20);
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || order(dir).len() >= expected.len()),
        "order never reached {expected:?}: {:?}",
        order(dir)
    );
    assert_eq!(order(dir), expected);
}

/// 슬롯과 대기열이 모두 빌 때까지 기다린다. 토큰은 슬롯 반납 전에 풀리므로 이 뒤에는 표시가 없다.
fn wait_idle(pool: &Arc<ShellPool>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || pool.running() == 0 && pool.waiting() == 0),
        "pool still busy: running {}, waiting {}",
        pool.running(),
        pool.waiting()
    );
}

/// 이전 실행이 슬롯을 차지한 동안 주기가 지난 틱은 그 훅을 제출하지 않고 그 회차의 시각만 기록한다.
/// 끝난 뒤 다음 주기에는 다시 제출한다.
#[test]
fn an_interval_hook_is_skipped_while_its_previous_run_holds_a_slot() {
    let (_tmp, dir) = setup();
    let pool = ShellPool::new(1, 4);
    let mut mgr = GlobalHookManager::new();
    let id = mgr.add(HookCondition::Interval(PERIOD), held(&dir, "a"), None);
    past_period();
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["a"]);
    assert!(mgr.has_pending_run(id));
    for _ in 0..2 {
        let before = mgr.last_fired[&id];
        past_period();
        run_due(&mut mgr, &pool);
        assert_eq!(pool.waiting(), 0, "a skipped run entered the queue");
        assert!(
            mgr.last_fired[&id] > before,
            "the skipped period was not recorded"
        );
    }
    release(&dir);
    wait_idle(&pool);
    assert!(!mgr.has_pending_run(id));
    assert_eq!(order(&dir), ["a"], "a skipped period ran later");
    hold_again(&dir);
    past_period();
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["a", "a"]);
}

/// 이전 실행이 대기열에 있는 동안에도 건너뛰어 대기열이 늘지 않는다.
#[test]
fn an_interval_hook_is_skipped_while_its_previous_run_waits_in_the_queue() {
    let (_tmp, dir) = setup();
    let pool = ShellPool::new(1, 4);
    submit_command_in(&pool, &held(&dir, "blocker"), None);
    wait_order(&dir, &["blocker"]);
    let mut mgr = GlobalHookManager::new();
    let id = mgr.add(HookCondition::Interval(PERIOD), held(&dir, "a"), None);
    past_period();
    run_due(&mut mgr, &pool);
    assert_eq!(pool.waiting(), 1);
    assert!(mgr.has_pending_run(id));
    for _ in 0..2 {
        past_period();
        run_due(&mut mgr, &pool);
        assert_eq!(pool.waiting(), 1, "a skipped run entered the queue");
    }
    release(&dir);
    wait_idle(&pool);
    assert_eq!(order(&dir), ["blocker", "a"]);
}

/// 실행기가 거절하면 표시가 남지 않아 다음 주기에 다시 제출한다.
#[test]
fn a_refused_interval_run_leaves_no_mark_and_is_submitted_next_period() {
    let (_tmp, dir) = setup();
    let pool = ShellPool::new(1, 0);
    submit_command_in(&pool, &held(&dir, "blocker"), None);
    wait_order(&dir, &["blocker"]);
    let mut mgr = GlobalHookManager::new();
    let id = mgr.add(HookCondition::Interval(PERIOD), held(&dir, "a"), None);
    past_period();
    run_due(&mut mgr, &pool);
    assert!(!mgr.has_pending_run(id), "a refused run left a mark");
    release(&dir);
    wait_idle(&pool);
    past_period();
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["blocker", "a"]);
}

/// 같은 명령이라도 다른 훅의 이전 실행은 막지 않는다.
#[test]
fn a_pending_run_does_not_block_another_hook_with_the_same_command() {
    let (_tmp, dir) = setup();
    let pool = ShellPool::new(2, 4);
    let mut mgr = GlobalHookManager::new();
    let a = mgr.add(HookCondition::Interval(PERIOD), held(&dir, "x"), None);
    past_period();
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["x"]);
    let b = mgr.add(HookCondition::Interval(PERIOD), held(&dir, "x"), None);
    past_period();
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["x", "x"]);
    assert!(mgr.has_pending_run(a) && mgr.has_pending_run(b));
    assert_eq!(pool.waiting(), 0);
}

/// 실행 중인 훅을 지우고 같은 명령을 다시 등록하면 새 id는 다음 주기에 제출된다.
#[test]
fn a_hook_registered_again_after_unset_is_not_blocked_by_the_old_run() {
    let (_tmp, dir) = setup();
    let pool = ShellPool::new(2, 4);
    let mut mgr = GlobalHookManager::new();
    let old = mgr.add(HookCondition::Interval(PERIOD), held(&dir, "x"), None);
    past_period();
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["x"]);
    assert!(mgr.remove(old));
    let new = mgr.add(HookCondition::Interval(PERIOD), held(&dir, "x"), None);
    assert_ne!(old, new);
    past_period();
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["x", "x"]);
    assert!(!mgr.has_pending_run(old));
    assert!(mgr.has_pending_run(new));
}

/// once 훅은 표시와 무관하게 한 번 제출되고 제거된다.
#[test]
fn a_once_hook_is_submitted_once_and_removed() {
    let (_tmp, dir) = setup();
    release(&dir);
    let pool = ShellPool::new(1, 4);
    let mut mgr = GlobalHookManager::new();
    mgr.add(HookCondition::Once(PERIOD), held(&dir, "once"), None);
    past_period();
    run_due(&mut mgr, &pool);
    assert!(mgr.list().is_empty());
    past_period();
    run_due(&mut mgr, &pool);
    wait_idle(&pool);
    assert_eq!(order(&dir), ["once"]);
}

/// file 훅은 이전 실행 중의 변경을 건너뛰되 mtime 기준을 두어, 끝난 뒤 첫 틱에 한 번 제출한다.
#[test]
fn a_file_change_during_a_pending_run_is_submitted_once_after_it_ends() {
    let (_tmp, dir) = setup();
    let watched = dir.join("watched.txt");
    std::fs::write(&watched, "v1").expect("v1");
    let pool = ShellPool::new(1, 4);
    let mut mgr = GlobalHookManager::new();
    let id = mgr.add(HookCondition::File(watched.clone()), held(&dir, "f"), None);
    run_due(&mut mgr, &pool);
    std::thread::sleep(Duration::from_millis(10));
    std::fs::write(&watched, "v2").expect("v2");
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["f"]);
    let base = mgr.last_mtime[&id];
    std::thread::sleep(Duration::from_millis(10));
    std::fs::write(&watched, "v3").expect("v3");
    assert_ne!(file_mtime(&watched), base, "the test write kept the mtime");
    run_due(&mut mgr, &pool);
    assert_eq!(pool.waiting(), 0, "a skipped change entered the queue");
    assert_eq!(mgr.last_mtime[&id], base, "a skipped change moved the base");
    release(&dir);
    wait_idle(&pool);
    assert_eq!(order(&dir), ["f"]);
    run_due(&mut mgr, &pool);
    wait_order(&dir, &["f", "f"]);
    wait_idle(&pool);
    run_due(&mut mgr, &pool);
    wait_idle(&pool);
    assert_eq!(order(&dir), ["f", "f"], "one change ran twice");
}
