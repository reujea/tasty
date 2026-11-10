use serde_json::json;

use crate::adapters::ipc::handler::params::{self, p_try};
use crate::runtime::engine_access::EngineRef;
use tasty_ipc::protocol::JsonRpcResponse;

use super::require_surface_id;

/// 화면 읽기에서 요청보다 줄이 적게 반환됐을 때 보관량을 확인할 진단 정보.
pub(crate) struct ScreenDiag {
    pub scrollback_len: usize,
    pub alt_screen: bool,
    /// show_dim=false 일 때 dim 셀을 빼서 내용이 달라진 보이는 화면 행 수. show_dim=true 면 0.
    pub dim_omitted_rows: usize,
}

/// 터미널이 없는 상태를 스크롤백 0과 구분해 is_terminal:false와 null로 반환한다.
pub(crate) fn with_screen_diagnostics(
    mut obj: serde_json::Value,
    diag: Option<ScreenDiag>,
) -> serde_json::Value {
    let map = obj.as_object_mut().expect("응답은 객체다");
    match diag {
        Some(d) => {
            map.insert("is_terminal".into(), json!(true));
            map.insert("scrollback_len".into(), json!(d.scrollback_len));
            map.insert("alt_screen".into(), json!(d.alt_screen));
            map.insert("dim_omitted_rows".into(), json!(d.dim_omitted_rows));
        }
        None => {
            map.insert("is_terminal".into(), json!(false));
            map.insert("scrollback_len".into(), serde_json::Value::Null);
            map.insert("alt_screen".into(), serde_json::Value::Null);
            map.insert("dim_omitted_rows".into(), serde_json::Value::Null);
        }
    }
    obj
}

/// 마지막 N줄을 읽는다. 하단 빈 줄은 건너뛰고 부족하면 스크롤백에서 채운다.
/// show_dim은 기본 false이며 dim 셀을 제외한다. 터미널 여부와 보관량도 반환한다.
pub(crate) fn handle_screen_text(
    engine: &EngineRef<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let lines = p_try!(params::opt_int::<usize>(params, "lines", &id));
    let show_dim = params
        .get("show_dim")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let found = engine.find_terminal_by_id(surface_id);
    let (text, diag) = match found {
        Some(t) => t.with_content(|view| {
            (
                match lines {
                    Some(n) => view.screen_text_lines(n, show_dim),
                    None => view.screen_text(show_dim),
                },
                Some(ScreenDiag {
                    scrollback_len: view.scrollback_len(),
                    alt_screen: view.is_alternate_screen(),
                    dim_omitted_rows: if show_dim { 0 } else { view.dim_omitted_rows() },
                }),
            )
        }),
        None => (String::new(), None),
    };
    JsonRpcResponse::success(
        id,
        with_screen_diagnostics(json!({ "text": text, "surface_id": surface_id }), diag),
    )
}

pub(crate) fn handle_cursor_position(
    engine: &EngineRef<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    if let Some(terminal) = engine.find_terminal_by_id(surface_id) {
        let (x, y) = terminal.cursor_position();
        JsonRpcResponse::success(id, json!({ "x": x, "y": y, "surface_id": surface_id }))
    } else {
        JsonRpcResponse::invalid_params(id, format!("Surface {} not found", surface_id))
    }
}

/// 터미널의 마우스 모드와 실제 클릭 처리에 적용하는 모드를 함께 반환한다.
/// hard 점유나 캡처 차단 설정 때문에 둘이 다를 수 있다.
///
/// | 필드 | 무엇을 답하나 |
/// |---|---|
/// | `terminal_mode` · `terminal_tracking` | 터미널 레지스터의 실효 레벨(DECSET 1000/1002/1003) |
/// | `sgr` | 1006(SGR 확장 좌표) 여부 |
/// | `effective_click_mode` · `effective_click_tracking` | 클릭 축에서 핸들러가 실제로 존중할 레벨 |
/// | `degraded_by` | 격하 사유들. 빈 배열이면 두 축이 일치한다 |
///
/// 개별 DECSET 플래그나 실제 PTY 전송 여부는 알리지 않는다.
/// effective_click은 휠에 적용되지 않아 캡처 차단 상태에서도 휠 보고는 가능하다.
pub(crate) fn handle_mouse_tracking(
    engine: &EngineRef<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let Some(terminal) = engine.find_terminal_by_id(surface_id) else {
        return JsonRpcResponse::invalid_params(id, format!("Surface {surface_id} not found"));
    };
    JsonRpcResponse::success(
        id,
        mouse_tracking_report(
            surface_id,
            terminal.mouse_tracking(),
            terminal.sgr_mouse(),
            engine.live.occupancy.is_hard_occupied(surface_id),
            engine.read().is_surface_mouse_capture_disabled(surface_id),
        ),
    )
}

const NO_MOUSE_TRACKING: &str = "none";

/// 상태를 주입해 실제 모드와 적용 모드의 차이를 시험할 수 있도록 응답 조립을 분리한다.
fn mouse_tracking_report(
    surface_id: u32,
    terminal_mode: tasty_terminal::MouseTrackingMode,
    sgr: bool,
    hard_occupied: bool,
    capture_disabled: bool,
) -> serde_json::Value {
    let effective = crate::state::mouse::effective_click_tracking_decision(
        hard_occupied,
        capture_disabled,
        terminal_mode,
    );
    let mut degraded_by: Vec<&str> = Vec::new();
    if hard_occupied {
        degraded_by.push("hard_occupied");
    }
    if capture_disabled {
        degraded_by.push("mouse_capture_disabled");
    }
    let terminal_label = mouse_tracking_label(terminal_mode);
    let effective_label = mouse_tracking_label(effective);
    json!({
        "surface_id": surface_id,
        "terminal_mode": terminal_label,
        "terminal_tracking": terminal_label != NO_MOUSE_TRACKING,
        "sgr": sgr,
        "effective_click_mode": effective_label,
        "effective_click_tracking": effective_label != NO_MOUSE_TRACKING,
        "degraded_by": degraded_by,
    })
}

fn mouse_tracking_label(mode: tasty_terminal::MouseTrackingMode) -> &'static str {
    match mode {
        tasty_terminal::MouseTrackingMode::None => NO_MOUSE_TRACKING,
        tasty_terminal::MouseTrackingMode::Click => "click",
        tasty_terminal::MouseTrackingMode::CellMotion => "cell_motion",
        tasty_terminal::MouseTrackingMode::AllMotion => "all_motion",
    }
}

/// 터미널 PTY의 전경(foreground) 프로세스 이름/PID 조회.
/// 플러그인이 `claude` 같은 자식 프로세스가 살아있는지 판단하기 위해 사용한다.
/// `is_shell`은 그 전경이 터미널의 셸인지 알린다. 터미널이 없거나 조회할 수 없으면
/// `name`/`pid`/`is_shell`이 모두 `null`로 반환된다.
pub(crate) fn handle_foreground_process(
    engine: &EngineRef<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let shell_pid = engine
        .runtime
        .terminals
        .pty(surface_id)
        .and_then(|pty| pty.process_id());
    if shell_pid.is_none()
        && let Some(name) = engine.remote.mirror_foreground.name(surface_id)
    {
        return JsonRpcResponse::success(id, mirror_foreground_report(surface_id, name));
    }
    JsonRpcResponse::success(
        id,
        foreground_report(
            surface_id,
            shell_pid,
            tasty_terminal::foreground_process::get_foreground_process,
        ),
    )
}

/// mirror는 서버가 보낸 이름만 있다. 원격 PID는 이 머신의 프로세스가 아니므로 싣지 않고,
/// 셸 여부는 이름으로만 판정한다.
fn mirror_foreground_report(surface_id: u32, name: &str) -> serde_json::Value {
    json!({
        "surface_id": surface_id,
        "name": name,
        "pid": null,
        "is_shell": tasty_terminal::foreground_process::is_known_shell_name(name),
    })
}

/// 셸 PID 하나로 전경을 한 번 조회하고, 같은 결과로 이름·PID·셸 여부를 만든다.
fn foreground_report(
    surface_id: u32,
    shell_pid: Option<u32>,
    lookup: impl FnOnce(u32) -> Option<tasty_terminal::foreground_process::ForegroundProcessInfo>,
) -> serde_json::Value {
    let snapshot = shell_pid.and_then(|shell_pid| Some((shell_pid, lookup(shell_pid)?)));
    let (name, pid, is_shell) = match snapshot {
        Some((shell_pid, fg)) => {
            let is_shell = foreground_is_shell(shell_pid, &fg);
            (Some(fg.name), Some(fg.pid), Some(is_shell))
        }
        None => (None, None, None),
    };
    json!({
        "surface_id": surface_id,
        "name": name,
        "pid": pid,
        "is_shell": is_shell,
    })
}

/// `Terminal::busy_with_foreground`가 유휴로 보는 셸 조건과 같다.
fn foreground_is_shell(
    shell_pid: u32,
    info: &tasty_terminal::foreground_process::ForegroundProcessInfo,
) -> bool {
    info.pid == shell_pid || tasty_terminal::foreground_process::is_known_shell_name(&info.name)
}

/// surface ID를 유지하면서 터미널을 교체한다. 기존 Terminal은 drop으로 정리한다.
/// cwd가 있으면 새 프로세스의 작업 폴더로 사용한다.
/// surface가 속한 pane과 트리에 존재하는지를 반환한다.
pub(crate) fn handle_surface_locate(
    engine: &crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let pane_id = engine.find_pane_for_surface(surface_id);
    JsonRpcResponse::success(
        id,
        json!({
            "surface_id": surface_id,
            "pane_id": pane_id,
            "exists": pane_id.is_some(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_terminal_reports_null_not_zero() {
        let out = with_screen_diagnostics(json!({ "text": "", "surface_id": 7 }), None);
        assert_eq!(out["is_terminal"], json!(false));
        assert!(
            out["scrollback_len"].is_null(),
            "0 이 아니라 null 이어야 한다"
        );
        assert!(out["alt_screen"].is_null());
        assert!(out["dim_omitted_rows"].is_null());
        assert_eq!(out["surface_id"], json!(7));
        assert_eq!(out["text"], json!(""));
    }

    #[test]
    fn an_empty_scrollback_reports_zero_which_is_not_null() {
        let out = with_screen_diagnostics(
            json!({ "text": "hi", "surface_id": 7 }),
            Some(ScreenDiag {
                scrollback_len: 0,
                alt_screen: true,
                dim_omitted_rows: 0,
            }),
        );
        assert_eq!(out["is_terminal"], json!(true));
        assert_eq!(out["scrollback_len"], json!(0));
        assert!(!out["scrollback_len"].is_null());
        assert_eq!(out["alt_screen"], json!(true));
    }

    #[test]
    fn a_live_scrollback_is_reported_as_its_length() {
        let out = with_screen_diagnostics(
            json!({ "id": 3, "text": "x" }),
            Some(ScreenDiag {
                scrollback_len: 402,
                alt_screen: false,
                dim_omitted_rows: 3,
            }),
        );
        assert_eq!(out["scrollback_len"], json!(402));
        assert_eq!(out["alt_screen"], json!(false));
        assert_eq!(out["dim_omitted_rows"], json!(3));
        assert_eq!(out["id"], json!(3), "pty.read 의 키도 보존한다");
    }

    // 적용 제한이 있을 때와 없을 때를 함께 검사해야 항상 None을 반환하는 오류도 잡는다.
    #[test]
    fn the_two_axes_split_when_the_handler_degrades_and_agree_when_it_does_not() {
        use tasty_terminal::MouseTrackingMode as M;

        let plain = mouse_tracking_report(7, M::AllMotion, true, false, false);
        assert_eq!(plain["terminal_mode"], json!("all_motion"));
        assert_eq!(plain["effective_click_mode"], json!("all_motion"));
        assert_eq!(plain["effective_click_tracking"], json!(true));
        assert_eq!(plain["degraded_by"], json!([]), "격하가 없으면 사유도 없다");

        let blacklisted = mouse_tracking_report(7, M::AllMotion, true, false, true);
        assert_eq!(
            blacklisted["terminal_mode"],
            json!("all_motion"),
            "호스트의 입력 제한이 터미널의 트래킹 모드를 바꾸면 안 된다"
        );
        assert_eq!(
            blacklisted["effective_click_mode"],
            json!("none"),
            "호스트가 입력을 제한할 때 실제 적용 모드를 구분해 반환해야 한다"
        );
        assert_eq!(blacklisted["effective_click_tracking"], json!(false));
        assert_eq!(
            blacklisted["degraded_by"],
            json!(["mouse_capture_disabled"])
        );

        let occupied = mouse_tracking_report(7, M::CellMotion, false, true, false);
        assert_eq!(occupied["terminal_mode"], json!("cell_motion"));
        assert_eq!(occupied["effective_click_mode"], json!("none"));
        assert_eq!(
            occupied["degraded_by"],
            json!(["hard_occupied"]),
            "두 입력 제한 사유를 구분할 수 있어야 한다"
        );

        let both = mouse_tracking_report(7, M::Click, false, true, true);
        assert_eq!(
            both["degraded_by"],
            json!(["hard_occupied", "mouse_capture_disabled"])
        );

        // 원래 tracking이 꺼져 있으면 제한 사유가 있어도 두 모드는 같다.
        let off = mouse_tracking_report(7, M::None, false, false, true);
        assert_eq!(off["terminal_mode"], json!("none"));
        assert_eq!(off["effective_click_mode"], json!("none"));
        assert_eq!(off["terminal_tracking"], json!(false));
        assert_eq!(off["effective_click_tracking"], json!(false));
    }

    fn fg(name: &str, pid: u32) -> tasty_terminal::foreground_process::ForegroundProcessInfo {
        tasty_terminal::foreground_process::ForegroundProcessInfo {
            name: name.into(),
            pid,
        }
    }

    const SHELL_PID: u32 = 4100;

    #[test]
    fn a_known_shell_in_front_is_a_shell_even_under_another_pid() {
        let out = foreground_report(7, Some(SHELL_PID), |_| Some(fg("bash", 4200)));
        assert_eq!(out["name"], json!("bash"));
        assert_eq!(out["pid"], json!(4200));
        assert_eq!(out["is_shell"], json!(true), "중첩된 알려진 셸도 셸이다");
    }

    #[test]
    fn the_terminal_root_process_is_a_shell_even_with_an_unknown_name() {
        let out = foreground_report(7, Some(SHELL_PID), |_| Some(fg("mysh", SHELL_PID)));
        assert_eq!(out["name"], json!("mysh"));
        assert_eq!(out["pid"], json!(SHELL_PID));
        assert_eq!(
            out["is_shell"],
            json!(true),
            "터미널의 루트 프로세스는 셸이다"
        );
    }

    #[test]
    fn another_program_in_front_is_not_a_shell() {
        for name in ["codex", "vim", "node", "sleep"] {
            let out = foreground_report(7, Some(SHELL_PID), |_| Some(fg(name, 4300)));
            assert_eq!(out["name"], json!(name));
            assert_eq!(out["pid"], json!(4300));
            assert_eq!(out["is_shell"], json!(false), "{name} 는 셸이 아니다");
        }
    }

    #[test]
    fn a_missing_terminal_or_failed_lookup_reports_every_field_as_null() {
        let no_terminal = foreground_report(7, None, |_| -> Option<_> {
            panic!("셸 PID가 없으면 조회하지 않는다")
        });
        let failed = foreground_report(7, Some(SHELL_PID), |_| None);
        for out in [no_terminal, failed] {
            assert_eq!(out["surface_id"], json!(7));
            for key in ["name", "pid", "is_shell"] {
                assert!(
                    out[key].is_null(),
                    "{key} 는 false·0이 아니라 null이다: {out}"
                );
            }
        }
    }

    // 조회를 두 번 하면 이름·PID와 셸 여부가 서로 다른 순간을 볼 수 있다.
    #[test]
    fn name_pid_and_is_shell_come_from_one_lookup_of_the_same_shell_pid() {
        let mut calls = Vec::new();
        let out = foreground_report(7, Some(SHELL_PID), |pid| {
            calls.push(pid);
            Some(fg("codex", SHELL_PID))
        });
        assert_eq!(calls, vec![SHELL_PID], "판정한 셸 PID로 한 번만 조회한다");
        assert_eq!(out["name"], json!("codex"));
        assert_eq!(out["pid"], json!(SHELL_PID));
        assert_eq!(
            out["is_shell"],
            json!(true),
            "조회 결과의 PID가 셸 PID와 같으면 이름과 무관하게 셸이다"
        );
    }

    // 모든 모드 이름이 다르고 None만 tracking=false여야 한다.
    #[test]
    fn every_tracking_level_reports_a_name_of_its_own() {
        use tasty_terminal::MouseTrackingMode as M;
        let all = [M::None, M::Click, M::CellMotion, M::AllMotion];
        let labels: Vec<&str> = all.iter().copied().map(mouse_tracking_label).collect();

        let mut distinct = labels.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            all.len(),
            "트래킹 모드 네 종류의 표시 이름이 모두 달라야 한다: {labels:?}"
        );

        assert_eq!(mouse_tracking_label(M::None), NO_MOUSE_TRACKING);
        for mode in [M::Click, M::CellMotion, M::AllMotion] {
            assert_ne!(
                mouse_tracking_label(mode),
                NO_MOUSE_TRACKING,
                "{mode:?} 가 트래킹 꺼짐으로 보고된다"
            );
        }
    }

    /// mirror는 서버가 보낸 이름을 답하고, 이 머신의 프로세스가 아닌 원격 PID는 싣지 않는다.
    #[test]
    fn a_mirror_reports_the_pushed_name_without_a_pid() {
        let (_, mut session) = crate::state::tests::test_state();
        let mut e = session.borrow_mut();
        let ask = |e: &crate::runtime::engine_access::EngineMut<'_>| {
            handle_foreground_process(&e.as_ref(), json!(1), &json!({ "surface_id": 4242 }))
                .result
                .expect("success")
        };
        assert!(
            ask(&e)["name"].is_null(),
            "첫 값을 받기 전에는 지금처럼 null"
        );
        e.set_mirror_surface_foreground(4242, Some("claude".into()));
        let out = ask(&e);
        assert_eq!(out["name"], json!("claude"));
        assert!(out["pid"].is_null());
        assert_eq!(out["is_shell"], json!(false));
        e.set_mirror_surface_foreground(4242, Some("bash".into()));
        assert_eq!(ask(&e)["is_shell"], json!(true));
    }
}
