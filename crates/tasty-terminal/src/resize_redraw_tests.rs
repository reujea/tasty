//! OS 크기 통지 뒤 재그리기 출력: busy를 새로 켜지 않고 침묵을 깨지 않는다.
use super::*;
use crate::foreground_process::ForegroundProcessInfo;

const SHELL: u32 = 1;

fn tui() -> ForegroundProcessInfo {
    ForegroundProcessInfo {
        name: "codex".into(),
        pid: 4242,
    }
}

fn idle_terminal() -> Terminal {
    let t = Terminal::new_detached(80, 24);
    let past = Instant::now() - Duration::from_secs(300);
    let mut st = t.lock_state();
    st.last_output_at = past;
    st.last_activity_at = past;
    st.last_activity_chunk_at = past;
    st.last_input_at = past;
    drop(st);
    t
}

fn busy(t: &mut Terminal) -> bool {
    let fg = tui();
    t.busy_with_foreground(SHELL, Some(&fg))
}

fn notify(t: &Terminal) {
    // 적용 성공을 흉내 낸다. 성공이면 되돌릴 상태가 필요 없다.
    let _applied = t.open_resize_window();
    t.confirm_resize_window();
}

/// Given idle인 TUI, When OS 크기 통지 뒤 다시 그리면, Then busy가 아니다.
#[test]
fn output_right_after_os_resize_does_not_enter_busy() {
    let mut t = idle_terminal();
    assert!(!busy(&mut t));
    notify(&t);
    t.process_bytes(b"\x1b[H\x1b[2Jredraw");
    assert!(!busy(&mut t));
}

/// Given 이미 busy(latch)인 프로그램, When 크기 통지 뒤 재그리기, Then busy를 유지한다.
#[test]
fn latched_busy_survives_os_resize_redraw() {
    let mut t = idle_terminal();
    t.process_bytes(b"working");
    assert!(busy(&mut t));
    notify(&t);
    t.process_bytes(b"\x1b[Hredraw");
    assert!(busy(&mut t));
}

/// Given 통지 전에 나온 출력, When 통지 뒤 판정, Then 억제하지 않는다.
#[test]
fn output_before_os_resize_is_not_suppressed() {
    let mut t = idle_terminal();
    t.process_bytes(b"real work");
    notify(&t);
    assert!(busy(&mut t));
}

/// Given 통지 → 재그리기 → 다음 통지(아직 재그리기 없음), When 판정, Then busy가 아니다.
#[test]
fn output_between_consecutive_resizes_is_suppressed() {
    let mut t = idle_terminal();
    notify(&t);
    t.process_bytes(b"redraw 1");
    notify(&t);
    assert!(!busy(&mut t));
}

/// Given 첫 구간이 끝난 뒤(2초 안) 다음 통지가 오고, When 두 번째 재그리기 전에 판정, Then 첫
/// 재그리기는 이어받은 구간 안이라 busy가 아니다.
#[test]
fn a_later_notification_inherits_the_window_of_the_last_redraw() {
    let mut t = idle_terminal();
    notify(&t);
    t.process_bytes(b"redraw 1");
    {
        let mut st = t.lock_state();
        let shift = RESIZE_REDRAW_WINDOW + Duration::from_millis(300);
        let r = st.resize_redraw.as_mut().expect("window");
        r.activity_start -= shift;
        r.output_start -= shift;
        r.end -= shift;
        st.last_activity_chunk_at -= shift;
        st.last_activity_at -= shift;
        st.last_output_at -= shift;
    }
    notify(&t);
    assert!(!busy(&mut t));
}

/// Given 구간 끝을 넘어 이어지는 출력, When 판정, Then 일반 활동이라 busy다.
#[test]
fn output_past_resize_window_enters_busy() {
    let mut t = idle_terminal();
    notify(&t);
    t.process_bytes(b"redraw");
    {
        let mut st = t.lock_state();
        let end = st.resize_redraw.expect("window").end;
        st.last_activity_chunk_at = end + Duration::from_millis(1);
        st.last_output_at = end + Duration::from_millis(1);
    }
    assert!(busy(&mut t));
}

/// Given 200초 전 마지막 출력, When 통지 뒤 재그리기, Then 침묵 기준은 200초 전 그대로이고 원시
/// `last_output_at`만 새 출력이다. 버스트 안 여러 통지에도 기준이 이어진다.
#[test]
fn silence_uses_the_pre_apply_output_time_inside_the_window() {
    let mut t = idle_terminal();
    let before = t.silence_since();
    notify(&t);
    t.process_bytes(b"redraw 1");
    notify(&t);
    t.process_bytes(b"redraw 2");
    assert_eq!(t.silence_since(), before);
    assert!(t.last_output_at() > before);
}

/// Given 실패한 OS 적용, When 되돌리면, Then 구간이 없어 뒤 출력은 일반 활동이다.
#[test]
fn a_failed_apply_restores_the_previous_state() {
    let mut t = idle_terminal();
    let token = t.open_resize_window();
    t.restore_resize_window(token);
    t.process_bytes(b"real");
    assert!(busy(&mut t));
    assert!(t.silence_since() > Instant::now() - Duration::from_secs(1));
}

/// Given 재연결로 닫힌 구간, When 새 연결의 출력이 오면, Then 침묵 기준이 그 출력이다. 이미 억제한
/// 출력의 기준은 새 출력 전까지 남는다.
#[test]
fn reconnect_closes_the_window_for_new_output() {
    let mut t = idle_terminal();
    let before = t.silence_since();
    notify(&t);
    t.process_bytes(b"redraw");
    t.lock_state().close_resize_window_at(Instant::now());
    assert_eq!(t.silence_since(), before);
    std::thread::sleep(Duration::from_millis(2));
    t.process_bytes(b"new connection");
    assert!(t.silence_since() > before);
}

/// Given 실제 PTY, When spawn 크기로 다시 flush하면 구간을 열지 않고, 다른 크기면 연다.
#[cfg(unix)]
#[test]
fn flush_opens_the_window_only_when_the_os_size_changes() {
    let (terminal, mut pty) = crate::spawn_terminal(
        crate::TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some("/bin/sh"),
            args: &["-c", "sleep 30"],
            surface_id: 0,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        std::sync::Arc::new(|| {}),
    )
    .expect("terminal/PTY creation");
    assert!(pty.schedule_resize(terminal.resource_generation(), 80, 24));
    assert!(!flush_os_resize(&terminal, &mut pty, false));
    assert!(terminal.lock_state().resize_redraw.is_none());

    assert!(pty.schedule_resize(terminal.resource_generation(), 100, 30));
    assert!(flush_os_resize(&terminal, &mut pty, false));
    assert!(terminal.lock_state().resize_redraw.is_some());
}
