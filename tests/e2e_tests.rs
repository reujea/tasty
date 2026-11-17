//! 시나리오별 테스트는 인스턴스를 공유하고 전용 워크스페이스로 격리한다.
//! 전역 목록은 다른 시나리오와 함께 바뀌므로 전체 길이 대신 자기 항목의 존재를 확인한다.

mod common;

use common::{TastyInstance, TestWorkspace};
use serde_json::json;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

/// 창이나 공용 설정을 바꾸는 시나리오는 write 잠금으로 단독 실행한다. 나머지는 read 잠금으로 병렬 실행한다.
/// 잠금 제거의 안전성은 확인하지 않았다. 잠금은 동시 실행만 막으므로 만든 창과 공용 설정은 별도로 정리해야 한다.
static WINDOW_EXCLUSIVE: RwLock<()> = RwLock::new(());

/// 다른 테스트의 panic이 이 테스트까지 실패시키지 않도록 poison은 무시한다.
fn lane() -> RwLockReadGuard<'static, ()> {
    WINDOW_EXCLUSIVE.read().unwrap_or_else(|e| e.into_inner())
}

fn exclusive_lane() -> RwLockWriteGuard<'static, ()> {
    WINDOW_EXCLUSIVE.write().unwrap_or_else(|e| e.into_inner())
}

/// 반환한 잠금 가드는 시나리오가 끝날 때까지 _lane에 보관한다.
fn scenario(
    name: &str,
) -> (
    &'static TastyInstance,
    TestWorkspace,
    u64,
    u64,
    RwLockReadGuard<'static, ()>,
) {
    let lane = lane();
    let tasty = common::shared();
    let ws = tasty.create_workspace(name);
    let sid = ws.surface_id;
    tasty.wait_for_shell(sid);
    let pid = tasty.first_pane_id_in_workspace(ws.id);
    (tasty, ws, sid, pid, lane)
}

#[test]
fn read_only_queries() {
    let (tasty, _ws, sid, pid, _lane) = scenario("e2e-read-only");

    let info = tasty.call("system.info", json!({}));
    assert_eq!(
        info.get("version").and_then(|v| v.as_str()),
        Some(env!("CARGO_PKG_VERSION")),
    );
    assert!(info["workspace_count"].as_u64().unwrap() >= 1);

    let tree = tasty.call("tree", json!({}));
    let tree_arr = tree.as_array().unwrap();
    assert!(!tree_arr.is_empty());
    assert!(tree_arr[0].get("name").is_some());

    let ui = tasty.call("ui.state", json!({}));
    assert_eq!(ui["settings_open_requested"], false);
    // 키가 빠진 경우와 명시적인 null을 구별한다.
    assert_eq!(ui["modal_open"], false);
    assert!(
        ui.get("active_modal_kind").is_some(),
        "`ui.state` 가 `active_modal_kind` 키를 아예 안 냈다 — 소비자 쪽에서는 이것이 \
         '모달이 없다' 와 구별되지 않는다"
    );
    assert_eq!(ui["active_modal_kind"], serde_json::Value::Null);
    assert_eq!(ui["notification_panel_open"], false);
    assert!(ui["workspace_count"].as_u64().unwrap() >= 1);
    assert!(ui["pane_count"].as_u64().unwrap() >= 1);
    assert!(ui["tab_count"].as_u64().unwrap() >= 1);

    let surfaces = tasty.call("surface.list", json!({}));
    assert!(
        surfaces
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"].as_u64() == Some(sid)),
        "surface.list 가 내 workspace 의 surface={sid} 를 빠뜨림: {surfaces:?}"
    );
    let panes = tasty.call("pane.list", json!({}));
    assert!(
        panes
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"].as_u64() == Some(pid)),
        "pane.list 가 내 workspace 의 pane={pid} 를 빠뜨림: {panes:?}"
    );

    let text = tasty.screen_text_of(sid);
    assert!(!text.trim().is_empty());

    let cursor = tasty.call("surface.cursor_position", json!({"surface_id": sid}));
    assert!(cursor.get("x").is_some());
    assert!(cursor.get("y").is_some());

    let tabs = tasty.call("tab.list", json!({"pane_id": pid}));
    assert!(!tabs["tabs"].as_array().unwrap().is_empty());
}

/// OSC 7의 cwd가 표시 이름에 반영되는지 GUI와 헤드리스에서 확인한다.
/// 하네스의 /bin/sh는 OSC 제목을 자동 전송하지 않으므로 제목 우선순위와 섞이지 않는다.
/// printf의 이스케이프 해석이 다른 Windows는 제외한다.
#[cfg(not(windows))]
#[test]
fn an_osc7_cwd_becomes_the_tab_name() {
    // 표시 이름은 tree에서 읽는다. tab.list는 원본 name을 반환한다.
    fn tab_names(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(o) => {
                if o.contains_key("surface")
                    && let Some(n) = o.get("name").and_then(|n| n.as_str())
                {
                    out.push(n.to_string());
                }
                o.values().for_each(|c| tab_names(c, out));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|c| tab_names(c, out)),
            _ => {}
        }
    }
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-osc7-tab-name");
    let dir = format!("tasty-osc7-{}", std::process::id());
    tasty.send_text(
        sid,
        &format!("printf '\\033]7;file://localhost/tmp/{dir}\\007'\r"),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let mut names = Vec::new();
        tab_names(&tasty.call("tree", json!({})), &mut names);
        if names.iter().any(|n| n == &dir) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "OSC 7 cwd({dir}) 가 탭 이름이 되지 않았다: {names:?} / 화면: {}",
            tasty.screen_text_of(sid)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn workspace_list_rows_carry_mirror_and_id() {
    let (tasty, ws, _sid, _pid, _lane) = scenario("e2e-workspace-list-shape");

    let ws_rows = tasty
        .call("workspace.list", json!({}))
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!ws_rows.is_empty(), "workspace.list 가 비었다");
    for row in &ws_rows {
        assert_eq!(
            row.get("mirror").and_then(|v| v.as_bool()),
            Some(false),
            "workspace.list 행에 mirror:false 가 없다: {row:?}"
        );
        assert!(
            row.get("id").and_then(|v| v.as_u64()).is_some(),
            "workspace.list 행에 id 가 없다: {row:?}"
        );
    }
    assert!(
        ws_rows.iter().any(|row| row["id"].as_u64() == Some(ws.id)),
        "workspace.list 가 방금 만든 workspace={} 를 빠뜨림: {ws_rows:?}",
        ws.id
    );
}

#[test]
fn markdown_recent_is_read_only() {
    let _lane = lane();
    let tasty = common::shared();

    let recent = tasty.call("markdown.recent", json!({}));
    let recent_arr = recent["recent"]
        .as_array()
        .expect("markdown.recent returns { recent: [...] }");
    assert!(recent_arr.len() <= 10, "recent 은 최대 10개");
    let recent2 = tasty.call("markdown.recent", json!({}));
    assert!(recent2["recent"].is_array());
}

#[test]
fn notification_create_then_list() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-notification");

    let created = tasty.call(
        "notification.create",
        json!({"title": "Test", "body": "Hello", "surface_id": sid}),
    );
    let notifs = tasty.call("notification.list", json!({}));
    let rows = notifs.as_array().cloned().unwrap_or_default();
    assert!(!rows.is_empty(), "notification.list 가 비었다: {created:?}");
    assert!(
        rows.iter().any(|n| n["surface_id"].as_u64() == Some(sid)),
        "notification.list 가 내 surface={sid} 의 알림을 빠뜨림: {rows:?}"
    );
}

#[test]
fn terminal_echo_and_mark_read() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-terminal-echo");

    tasty.set_mark(sid);
    let echo_cmd = if cfg!(windows) {
        "echo hello\r\n"
    } else {
        "echo hello\n"
    };
    tasty.send_text(sid, echo_cmd);
    let output = tasty.wait_for_output(sid, "hello", Duration::from_secs(5));
    assert!(output.contains("hello"));

    tasty.set_mark(sid);
    let echo_cmd = if cfg!(windows) {
        "echo test_marker\r\n"
    } else {
        "echo test_marker\n"
    };
    tasty.send_text(sid, echo_cmd);
    let output = tasty.wait_for_output(sid, "test_marker", Duration::from_secs(5));
    assert!(output.contains("test_marker"));
}

/// 실제 IPC로 출력 스캐너 커서의 전진과 에이전트 mark와의 독립성을 확인한다.
#[test]
fn terminal_scan_cursor_is_separate_from_the_agent_mark() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-scan-cursor");

    let first = tasty.call(
        "surface.read_since_scan_mark",
        json!({"surface_id": sid, "strip_ansi": true}),
    );
    assert!(
        first.get("text").and_then(|v| v.as_str()).is_some(),
        "surface.read_since_scan_mark 응답에 text가 없다: {first:?}"
    );
    assert_eq!(first["surface_id"].as_u64(), Some(sid));

    let scan = |strip_ansi: bool| -> String {
        tasty.call(
            "surface.read_since_scan_mark",
            json!({"surface_id": sid, "strip_ansi": strip_ansi}),
        )["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    // 커서가 전진하므로 출력이 나뉘어 오면 읽은 조각을 이어 붙인다.
    let wait_scan = |needle: &str| -> String {
        let start = std::time::Instant::now();
        let mut seen = String::new();
        loop {
            seen.push_str(&scan(true));
            if seen.contains(needle) {
                return seen;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "scan 커서에서 '{needle}' 를 못 봤다. 본 것:\n{seen}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let echo = |marker: &str| {
        let cmd = if cfg!(windows) {
            format!("echo {marker}\r\n")
        } else {
            format!("echo {marker}\n")
        };
        tasty.send_text(sid, &cmd);
    };

    echo("scan_marker_one");
    wait_scan("scan_marker_one");
    assert!(
        !scan(true).contains("scan_marker_one"),
        "커서가 전진하지 않았다 — 같은 구간이 다시 왔다"
    );

    // 출력이 도착한 뒤 mark를 세워야 set_mark가 scan 커서까지 옮기는 결함을 찾을 수 있다.
    echo("scan_marker_two");
    // scan으로 기다리면 커서가 전진하므로 mark를 움직이지 않는 조회로 출력 도착을 확인한다.
    tasty.wait_for_output(sid, "scan_marker_two", Duration::from_secs(10));
    tasty.set_mark(sid);
    let seen = wait_scan("scan_marker_two");
    assert!(
        seen.contains("scan_marker_two"),
        "set_mark 이 scan 커서를 밀었다 — 그 앞에 이미 와 있던 출력이 사라졌다: {seen}"
    );

    // scan으로 먼저 읽은 뒤에도 에이전트 mark에서는 같은 출력을 읽을 수 있어야 한다.
    echo("scan_marker_three");
    wait_scan("scan_marker_three");
    let since_mark = tasty.read_since_mark(sid);
    assert!(
        since_mark.contains("scan_marker_three"),
        "scan 읽기가 에이전트의 mark 를 밀었다 — mark 이후 구간에서 출력이 사라졌다: \
         {since_mark}"
    );
}

/// 소비자별 커서와 응답 필드가 실제 IPC에서도 유지되는지 확인한다.
#[test]
fn terminal_output_reads_from_a_consumer_held_position() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-output-cursor");

    let read = |params: serde_json::Value| -> serde_json::Value {
        let mut p = json!({"surface_id": sid, "strip_ansi": true});
        for (k, v) in params.as_object().expect("object") {
            p[k.as_str()] = v.clone();
        }
        tasty.call("surface.read_since_mark", p)
    };
    let echo = |marker: &str| {
        let cmd = if cfg!(windows) {
            format!("echo {marker}\r\n")
        } else {
            format!("echo {marker}\n")
        };
        tasty.send_text(sid, &cmd);
    };

    let first = read(json!({}));
    let stream = first["stream"]
        .as_str()
        .unwrap_or_else(|| panic!("응답에 stream 칸이 없다: {first:?}"))
        .to_string();
    for key in [
        "cursor",
        "next_cursor",
        "raw_bytes",
        "retention_start",
        "retention_end",
        "skipped",
    ] {
        assert!(
            first[key].as_u64().is_some(),
            "응답에 {key} 칸이 없다: {first:?}"
        );
    }
    assert_eq!(first["skipped"].as_u64(), Some(0));

    let mut a = first["next_cursor"].as_u64().expect("next_cursor");
    let b = a;
    echo("cursor_marker_one");
    tasty.wait_for_output(sid, "cursor_marker_one", Duration::from_secs(10));

    let a_read = read(json!({"cursor": a, "stream": stream}));
    assert!(
        a_read["text"]
            .as_str()
            .unwrap_or_default()
            .contains("cursor_marker_one"),
        "A 가 자기 위치 이후를 못 받았다: {a_read:?}"
    );
    a = a_read["next_cursor"].as_u64().expect("next_cursor");

    let b_read = read(json!({"cursor": b, "stream": stream}));
    assert!(
        b_read["text"]
            .as_str()
            .unwrap_or_default()
            .contains("cursor_marker_one"),
        "A 의 읽기가 B 의 위치를 움직였다: {b_read:?}"
    );

    let a_again = read(json!({"cursor": a, "stream": stream}));
    assert!(
        !a_again["text"]
            .as_str()
            .unwrap_or_default()
            .contains("cursor_marker_one"),
        "B 의 읽기가 A 의 위치를 되돌렸다: {a_again:?}"
    );

    echo("cursor_marker_two");
    tasty.wait_for_output(sid, "cursor_marker_two", Duration::from_secs(10));
    tasty.set_mark(sid);
    let after_mark = read(json!({"cursor": a, "stream": stream}));
    assert!(
        after_mark["text"]
            .as_str()
            .unwrap_or_default()
            .contains("cursor_marker_two"),
        "set_mark 이 소비자의 위치를 밀었다 — 그 앞에 이미 와 있던 출력이 사라졌다: \
         {after_mark:?}"
    );

    // 호출자가 번역 가능한 메시지에 의존하지 않도록 구조화된 reason을 비교한다.
    let refusal = |params: serde_json::Value| -> String {
        let mut p = json!({"surface_id": sid});
        for (k, v) in params.as_object().expect("object") {
            p[k.as_str()] = v.clone();
        }
        let resp = tasty.call_raw("surface.read_since_mark", p);
        resp["error"]["data"]["reason"]
            .as_str()
            .unwrap_or_else(|| panic!("거절에 사유 칸이 없다: {resp:?}"))
            .to_string()
    };
    assert_eq!(
        refusal(json!({"cursor": 0})),
        "cursor_without_stream",
        "표지 없는 위치를 받아들이면 재사용된 surface id 위에서 남의 출력을 잇는다"
    );
    assert_eq!(
        refusal(json!({"cursor": 0, "stream": "not-this-stream"})),
        "stream_mismatch"
    );
    let end = read(json!({}))["retention_end"].as_u64().expect("end");
    assert_eq!(
        refusal(json!({"cursor": end + 1_000_000, "stream": stream})),
        "cursor_ahead_of_stream"
    );

    // 원문 바이트와 커서 증가량을 비교한다. 이 ASCII 출력만으로는 원문 길이와 text 길이를 구별할 수 없다.
    // 길이가 다른 입력은 버퍼와 응답 생성 함수의 단위 시험에서 검사한다.
    let capped = read(json!({"cursor": a, "stream": stream, "max_bytes": 4}));
    assert!(capped["raw_bytes"].as_u64().is_some_and(|n| n <= 4));
    assert_eq!(
        capped["next_cursor"].as_u64(),
        Some(a + capped["raw_bytes"].as_u64().expect("raw_bytes")),
        "next_cursor 는 원문 바이트로 전진한다"
    );
}

#[test]
fn terminal_send_key_and_send_to() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-terminal-keys");

    tasty.set_mark(sid);
    tasty.call(
        "surface.send",
        json!({"surface_id": sid, "text": "echo key_test"}),
    );
    tasty.call(
        "surface.send_key",
        json!({"surface_id": sid, "key": "enter"}),
    );
    let output = tasty.wait_for_output(sid, "key_test", Duration::from_secs(5));
    assert!(output.contains("key_test"));

    for key in &["up", "down"] {
        let result = tasty.call("surface.send_key", json!({"surface_id": sid, "key": key}));
        assert_eq!(result["sent"], true, "Failed to send key: {}", key);
    }

    tasty.set_mark(sid);
    tasty.call(
        "surface.send_to",
        json!({"surface_id": sid, "text": "echo targeted\n"}),
    );
    let output = tasty.wait_for_output(sid, "targeted", Duration::from_secs(5));
    assert!(output.contains("targeted"));
}

#[test]
fn terminal_send_combo_and_abort() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-terminal-combo");

    let result = tasty.call(
        "surface.send_combo",
        json!({"surface_id": sid, "key": "x", "modifiers": ["alt"]}),
    );
    assert_eq!(result["sent"], true);
    // zsh에서 Alt+X가 명령 입력 모드를 바꿀 수 있어 Ctrl+G로 빠져나온다. 사용자 dotfile은 격리돼 있다.
    tasty.call(
        "surface.send_combo",
        json!({"surface_id": sid, "key": "g", "modifiers": ["ctrl"]}),
    );
    tasty.set_mark(sid);
    tasty.send_text(sid, "echo __abort_ok__\n");
    tasty.wait_for_output(sid, "__abort_ok__", Duration::from_secs(3));
}

#[test]
fn surface_completion_reaches_the_pipeline() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-completion");

    // IPC 응답만 확인하며 실제 highlight 렌더링은 검사하지 않는다.
    let completion = tasty.call("surface.completion", json!({ "surface_id": sid }));
    assert_eq!(completion["ok"], true);
    assert_eq!(completion["surface_id"].as_u64().unwrap(), sid);
}

#[test]
fn surface_attention_raise_and_clear() {
    // 렌더링 중 포커스된 서피스의 attention이 지워질 수 있으므로 활성화하지 않은 새 워크스페이스를 사용한다.
    let _lane = lane();
    let tasty = common::shared();
    let att_ws = tasty.create_workspace("attention-clear-e2e");
    let att_sid = att_ws.surface_id;
    let raise_kind = |kind: &str| {
        let r = tasty.call(
            "surface.completion",
            json!({ "surface_id": att_sid, "kind": kind }),
        );
        assert_eq!(r["ok"], true);
    };
    let attention_kind =
        || tasty.call("surface.attention.get", json!({ "surface_id": att_sid }))["kind"].clone();

    raise_kind("needs_input");
    assert_eq!(attention_kind(), "needs_input");

    // 늦게 도착한 해제가 새 attention을 지우지 않도록 kind 불일치를 확인한다.
    let mismatched = tasty.call(
        "surface.attention.clear",
        json!({ "surface_id": att_sid, "kind": "completion" }),
    );
    assert_eq!(mismatched["ok"], true);
    assert_eq!(mismatched["cleared"], false);
    assert_eq!(mismatched["previous_kind"], "needs_input");
    assert_eq!(attention_kind(), "needs_input");

    let cleared = tasty.call("surface.attention.clear", json!({ "surface_id": att_sid }));
    assert_eq!(cleared["cleared"], true);
    assert_eq!(cleared["previous_kind"], "needs_input");
    assert!(attention_kind().is_null());

    let again = tasty.call("surface.attention.clear", json!({ "surface_id": att_sid }));
    assert_eq!(again["ok"], true);
    assert_eq!(again["cleared"], false);
    assert!(again["previous_kind"].is_null());

    raise_kind("completion");
    assert_eq!(attention_kind(), "completion");
    let matched = tasty.call(
        "surface.attention.clear",
        json!({ "surface_id": att_sid, "kind": "completion" }),
    );
    assert_eq!(matched["cleared"], true);
    assert!(attention_kind().is_null());

    assert!(
        tasty
            .call_raw("surface.attention.clear", json!({ "surface_id": 999_999 }))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("surface.attention.get", json!({ "surface_id": 999_999 }))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw(
                "surface.attention.clear",
                json!({ "surface_id": att_sid, "kind": "bogus" })
            )
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("surface.attention.clear", json!({}))
            .get("error")
            .is_some()
    );
}

// Windows 기본 셸은 printf 이스케이프를 동일하게 해석하지 않아 Unix에서만 실행한다.
#[cfg(not(windows))]
#[test]
fn dim_sgr2_survives_to_the_renderer() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-dim-sgr2");

    tasty.set_mark(sid);
    tasty.send_text(sid, "clear; printf '\\033[2mD\\033[0mN\\n'\n");
    tasty.wait_for_output(sid, "DN", Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(200));

    // 기본 조회는 dim 셀을 숨기므로 검사할 D가 포함되도록 show_dim을 켠다.
    let text = tasty.call(
        "surface.screen_text",
        json!({"surface_id": sid, "show_dim": true}),
    )["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let row = text
        .lines()
        .position(|l| l.starts_with("DN"))
        .unwrap_or_else(|| panic!("DN row not found in screen_text:\n{text}")) as u64;

    let dim = tasty.call(
        "debug.cell_info",
        json!({"surface_id": sid, "row": row, "col": 0}),
    );
    assert_eq!(dim["text"], "D");
    assert_eq!(dim["intensity"], "half");

    let normal = tasty.call(
        "debug.cell_info",
        json!({"surface_id": sid, "row": row, "col": 1}),
    );
    assert_eq!(normal["text"], "N");
    assert_eq!(normal["intensity"], "normal");

    let dim_glyph = tasty.call(
        "debug.glyph_color",
        json!({"surface_id": sid, "row": row, "col": 0}),
    );
    let normal_glyph = tasty.call(
        "debug.glyph_color",
        json!({"surface_id": sid, "row": row, "col": 1}),
    );
    assert_ne!(
        dim_glyph["fg"]["hex"], normal_glyph["fg"]["hex"],
        "renderer must dim fg distinctly from normal fg",
    );
}

#[test]
fn hook_set_list_unset() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-hooks");

    let hook_result = tasty.call(
        "hook.set",
        json!({"surface_id": sid, "event": "bell", "command": "echo hooked"}),
    );
    let hook_id = hook_result["hook_id"].as_u64().unwrap();
    assert!(hook_id > 0);

    let hooks = tasty.call("hook.list", json!({}));
    assert!(
        hooks
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["id"].as_u64() == Some(hook_id) || h["hook_id"].as_u64() == Some(hook_id)),
        "hook.list 가 방금 만든 hook={hook_id} 를 빠뜨림: {hooks:?}"
    );

    tasty.call("hook.unset", json!({"hook_id": hook_id}));
    let hooks_after = tasty.call("hook.list", json!({}));
    assert!(
        hooks_after
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["id"].as_u64() != Some(hook_id) && h["hook_id"].as_u64() != Some(hook_id)),
        "unset 후에도 hook={hook_id} 가 남아있다: {hooks_after:?}"
    );
}

#[test]
fn structural_mutations_within_one_workspace() {
    let (tasty, ws, _sid, pid, _lane) = scenario("e2e-structural");

    // 다른 시나리오가 동시에 페인을 바꾸므로 이 워크스페이스의 페인만 센다.
    let panes_in_ws = || {
        tasty
            .call("pane.list", json!({}))
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter(|p| p["workspace_id"].as_u64() == Some(ws.id))
            .count()
    };

    let panes_before = panes_in_ws();
    let split_result = tasty.call(
        "split",
        json!({"level": "pane", "direction": "vertical", "target_pane": pid}),
    );
    let new_pane_id = split_result["new_pane_id"].as_u64().unwrap();
    assert_eq!(panes_in_ws(), panes_before + 1);

    let tabs_before = tasty.call("tab.list", json!({"pane_id": pid}))["tabs"]
        .as_array()
        .unwrap()
        .len();
    tasty.call("tab.create", json!({"pane_id": pid}));
    let tabs_after = tasty.call("tab.list", json!({"pane_id": pid}))["tabs"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(tabs_after, tabs_before + 1);

    let tab_list = tasty.call("tab.list", json!({"pane_id": pid}));
    let last_tab_id = tab_list["tabs"].as_array().unwrap().last().unwrap()["id"]
        .as_u64()
        .unwrap();
    let close_result = tasty.call("tab.close", json!({"tab_id": last_tab_id}));
    assert_eq!(close_result["closed"], true);
    let tabs_final = tasty.call("tab.list", json!({"pane_id": pid}))["tabs"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(tabs_final, tabs_before);

    let close_pane_result = tasty.call("pane.close", json!({"pane_id": new_pane_id}));
    assert_eq!(close_pane_result["closed"], true);
    assert_eq!(panes_in_ws(), panes_before);

    let result = tasty.call("pane.close", json!({"pane_id": pid}));
    assert_eq!(result["closed"], false);

    let tab_list = tasty.call("tab.list", json!({"pane_id": pid}));
    let sole_tab_id = tab_list["tabs"].as_array().unwrap()[0]["id"]
        .as_u64()
        .unwrap();
    let result = tasty.call("tab.close", json!({"tab_id": sole_tab_id}));
    assert_eq!(result["closed"], false);
}

#[test]
fn workspace_create_appears_in_the_list() {
    let _lane = lane();
    let tasty = common::shared();
    let created = tasty.call("workspace.create", json!({"name": "e2e-ws-create"}));
    let ws_id = created["id"].as_u64().expect("workspace.create returns id");
    let rows = tasty
        .call("workspace.list", json!({}))
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        rows.iter().any(|w| w["id"].as_u64() == Some(ws_id)),
        "workspace.list 가 방금 만든 workspace={ws_id} 를 빠뜨림: {rows:?}"
    );
}

#[test]
fn tab_close_guard_is_tab_scoped_not_pane_scoped() {
    let (tasty, _ws, sid, pid, _lane) = scenario("e2e-tab-close-guard");

    let tab_list = tasty.call("tab.list", json!({"pane_id": pid}));
    let own_tab_id = tab_list["tabs"].as_array().unwrap()[0]["id"]
        .as_u64()
        .unwrap();

    tasty.call("tab.create", json!({"pane_id": pid}));
    let tab_list = tasty.call("tab.list", json!({"pane_id": pid}));
    let sibling_tab_id = tab_list["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_u64().unwrap() != own_tab_id)
        .unwrap()["id"]
        .as_u64()
        .unwrap();

    let result = tasty.call(
        "tab.close",
        json!({"tab_id": sibling_tab_id, "caller_surface_id": sid}),
    );
    assert_eq!(result["closed"], true);

    // 마지막 탭 보호와 구별하기 위해 형제 탭을 다시 만든 뒤 자기 탭 닫기를 확인한다.
    tasty.call("tab.create", json!({"pane_id": pid}));
    let result = tasty.call_raw(
        "tab.close",
        json!({"tab_id": own_tab_id, "caller_surface_id": sid}),
    );
    assert!(
        result.get("error").is_some(),
        "expected tab.close to refuse closing the caller's own tab, got {result:?}"
    );
}

#[test]
fn renderer_resolves_dim_and_plain_glyph_colors() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-glyph-color");

    // 화면을 지우고 첫 행에 일반 Z와 dim Z를 순서대로 주입한다.
    let dim_seq = "1b5b324a1b5b485a1b5b326d5a1b5b306d";
    let fed = tasty.call(
        "debug.feed_bytes",
        json!({"surface_id": sid, "bytes": dim_seq}),
    );
    assert!(fed["fed"].as_u64().unwrap() > 0);

    let plain = tasty.call(
        "debug.cell_info",
        json!({"surface_id": sid, "row": 0, "col": 0}),
    );
    assert_eq!(plain["text"], "Z", "plain cell text mismatch");
    assert_eq!(
        plain["intensity"], "normal",
        "plain cell should have intensity=normal"
    );

    let dim = tasty.call(
        "debug.cell_info",
        json!({"surface_id": sid, "row": 0, "col": 1}),
    );
    assert_eq!(dim["text"], "Z", "dim cell text mismatch");
    assert_eq!(
        dim["intensity"], "half",
        "dim cell should have intensity=half (SGR 2 reached termwiz)"
    );

    let plain_color = tasty.call(
        "debug.glyph_color",
        json!({"surface_id": sid, "row": 0, "col": 0}),
    );
    let dim_color = tasty.call(
        "debug.glyph_color",
        json!({"surface_id": sid, "row": 0, "col": 1}),
    );
    assert_eq!(plain_color["in_bounds"], true);
    assert_eq!(dim_color["in_bounds"], true);

    let plain_fg = &plain_color["fg"];
    let dim_fg = &dim_color["fg"];

    let pr = plain_fg["r"].as_f64().unwrap();
    let pg = plain_fg["g"].as_f64().unwrap();
    let pb = plain_fg["b"].as_f64().unwrap();
    assert!(
        pr > 0.5 && pg > 0.5 && pb > 0.5,
        "plain fg should be bright"
    );

    assert_ne!(
        plain_fg, dim_fg,
        "dim cell fg should differ from plain cell fg (SGR 2 must dim)",
    );

    // dim 전경색은 각 채널에서 일반 전경색과 배경색 사이에 있어야 한다.
    let plain_bg = &plain_color["bg"];
    let br = plain_bg["r"].as_f64().unwrap();
    let bg = plain_bg["g"].as_f64().unwrap();
    let bb = plain_bg["b"].as_f64().unwrap();
    let dr = dim_fg["r"].as_f64().unwrap();
    let dg = dim_fg["g"].as_f64().unwrap();
    let db = dim_fg["b"].as_f64().unwrap();
    let between = |a: f64, b: f64, x: f64| (a.min(b)..=a.max(b)).contains(&x);
    assert!(
        between(pr, br, dr) && between(pg, bg, dg) && between(pb, bb, db),
        "dim fg ({dr},{dg},{db}) must lie between plain fg ({pr},{pg},{pb}) and bg ({br},{bg},{bb})",
    );
}

#[test]
fn error_paths_reject_malformed_calls() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-error-paths");

    let resp = tasty.call_raw("nonexistent.method", json!({}));
    assert!(resp.get("error").is_some());
    assert_eq!(resp["error"]["code"].as_i64().unwrap(), -32601);

    let resp = tasty.call_raw(
        "surface.send_combo",
        json!({"surface_id": sid, "modifiers": ["ctrl"]}),
    );
    assert!(resp.get("error").is_some());

    let resp = tasty.call_raw(
        "surface.send_to",
        json!({"surface_id": 99999, "text": "hello"}),
    );
    assert!(resp.get("error").is_some());

    assert!(
        tasty
            .call_raw("surface.send_to", json!({"text": "hello"}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("surface.send_to", json!({"surface_id": 1}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("surface.send", json!({"text": "hello"}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("surface.set_mark", json!({}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("surface.completion", json!({}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("surface.screen_text", json!({}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("surface.cursor_position", json!({}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("pane.close", json!({}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("tab.close", json!({}))
            .get("error")
            .is_some()
    );
    assert!(tasty.call_raw("tab.list", json!({})).get("error").is_some());
    assert!(
        tasty
            .call_raw("tab.create", json!({}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("split", json!({"direction": "vertical"}))
            .get("error")
            .is_some()
    );
    assert!(
        tasty
            .call_raw("split", json!({"level": "pane", "direction": "vertical"}))
            .get("error")
            .is_some()
    );
}

#[test]
fn headless_pty_spawn_write_wait_kill() {
    let _lane = lane();
    let tasty = common::shared();

    let spawned = tasty.call("pty.spawn", json!({}));
    let pty_id = spawned["pty_id"]
        .as_u64()
        .expect("pty.spawn returns pty_id");
    assert!(
        pty_id >= 0x8000_0000,
        "pty id 는 surface id 와 disjoint 한 고범위여야: {pty_id}"
    );

    let listed = tasty.call("pty.list", json!({}));
    assert!(
        listed["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"].as_u64() == Some(pty_id)),
        "pty.list 가 방금 spawn 한 pty 를 빠뜨림: {listed:?}"
    );

    tasty.call(
        "pty.write",
        json!({ "id": pty_id, "text": "echo PTY_E2E_MARK\n" }),
    );
    let read_start = std::time::Instant::now();
    loop {
        let r = tasty.call("pty.read", json!({ "id": pty_id }));
        if r["text"].as_str().unwrap_or("").contains("PTY_E2E_MARK") {
            break;
        }
        if read_start.elapsed() > Duration::from_secs(5) {
            panic!("pty.read 가 echo 출력을 반영하지 못함: {r:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    tasty.call("pty.write", json!({ "id": pty_id, "text": "exit 5\n" }));
    let wait_start = std::time::Instant::now();
    let exited = loop {
        let w = tasty.call("pty.wait", json!({ "id": pty_id }));
        if w["exited"].as_bool() == Some(true) {
            break w;
        }
        if wait_start.elapsed() > Duration::from_secs(10) {
            let r = tasty.call("pty.read", json!({ "id": pty_id }));
            let screen = r["text"].as_str().unwrap_or("<pty.read 실패>");
            let tail: String = screen
                .chars()
                .rev()
                .take(48)
                .collect::<Vec<char>>()
                .into_iter()
                .rev()
                .collect();
            panic!(
                "pty.wait 가 종료를 감지하지 못함: {w:?} — 관측(pty.read): 화면 {}(scrollback={}, \
                 alt_screen={}), 꼬리=\"{}\"",
                if screen.trim().is_empty() {
                    "빈 화면"
                } else {
                    "화면 내용 있음"
                },
                r["scrollback_len"],
                r["alt_screen"],
                tail.escape_default(),
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(exited["exit_code"].as_i64(), Some(5), "진짜 exit-code 회수");
    assert_eq!(exited["success"], false);

    tasty.call("pty.kill", json!({ "id": pty_id }));
    let listed2 = tasty.call("pty.list", json!({}));
    assert!(
        listed2["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["id"].as_u64() != Some(pty_id)),
        "kill 후 pty.list 에 남아있으면 안 됨: {listed2:?}"
    );
}

#[test]
fn headless_pty_attach_surface_promotes_to_a_tab() {
    let (tasty, _ws, _sid, pid, _lane) = scenario("e2e-pty-promote");

    let promo = tasty.call("pty.spawn", json!({}));
    let promo_id = promo["pty_id"].as_u64().expect("pty.spawn returns pty_id");
    let attached = tasty.call(
        "pty.attach_surface",
        json!({ "id": promo_id, "pane_id": pid }),
    );
    let new_surface = attached["surface_id"]
        .as_u64()
        .expect("attach_surface returns surface_id");
    assert_eq!(attached["pane_id"].as_u64(), Some(pid));
    let surfaces_now = tasty.call("surface.list", json!({}));
    assert!(
        surfaces_now
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"].as_u64() == Some(new_surface)),
        "승격된 surface 가 surface.list 에 없음: {surfaces_now:?}"
    );
    let listed = tasty.call("pty.list", json!({}));
    assert!(
        listed["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["id"].as_u64() != Some(promo_id)),
        "승격된 pty 는 headless 목록에서 빠져야 함: {listed:?}"
    );
}

/// window.list에 표시 여부가 없어 X 서버의 IsViewable을 직접 조회한다.
/// 하네스 디스플레이를 사용한다. inherit + Wayland에서는 Xlib로 다른 종류의 ID를 조회하면 프로세스가 종료될 수 있어 None을 반환한다.
#[cfg(all(target_os = "linux", feature = "gui"))]
fn x11_window_is_viewable(xid: u64) -> Result<Option<bool>, String> {
    use x11_dl::xlib;
    let declared = std::env::var("TASTY_E2E_DISPLAY").unwrap_or_default();
    let name = if declared.trim() == "inherit" {
        if std::env::var("WAYLAND_DISPLAY").is_ok_and(|v| !v.is_empty()) {
            return Ok(None);
        }
        std::env::var("DISPLAY").map_err(|_| "DISPLAY is not set".to_string())?
    } else {
        declared.trim().to_string()
    };
    let x = xlib::Xlib::open().map_err(|e| format!("Xlib::open: {e}"))?;
    let cname = std::ffi::CString::new(name.clone()).map_err(|e| e.to_string())?;
    // SAFETY: cname은 NUL 종단 문자열이며 반환된 포인터가 null인지 확인한다.
    let dpy = unsafe { (x.XOpenDisplay)(cname.as_ptr()) };
    if dpy.is_null() {
        return Err(format!("XOpenDisplay({name}) failed"));
    }
    // SAFETY: XWindowAttributes는 정수·포인터로 구성된 C 구조체이며 0으로 초기화할 수 있다.
    let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
    // SAFETY: dpy는 열린 연결이고 xid는 같은 디스플레이에서 만든 X11 창 ID다. attrs는 쓰기 가능한 지역 변수다.
    let status = unsafe { (x.XGetWindowAttributes)(dpy, xid as xlib::Window, &mut attrs) };
    // SAFETY: 열린 연결을 닫고 이후 dpy를 사용하지 않는다.
    unsafe { (x.XCloseDisplay)(dpy) };
    if status == 0 {
        return Err(format!("XGetWindowAttributes(0x{xid:x}) failed"));
    }
    Ok(Some(attrs.map_state == xlib::IsViewable))
}

/// 5초 안에 창이 보이는지 확인한다. Wayland로 조회를 생략하면 경고를 남긴다.
#[cfg(all(target_os = "linux", feature = "gui"))]
fn wait_x11_window_viewable(xid: u64) {
    let start = std::time::Instant::now();
    loop {
        match x11_window_is_viewable(xid) {
            Ok(None) => {
                tracing::warn!("inherit + Wayland — skipping the agent window map state check");
                return;
            }
            Ok(Some(true)) => return,
            Ok(Some(false)) if start.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(Some(false)) => {
                panic!("에이전트 창 0x{xid:x} 이 5 초 안에 보이지(IsViewable) 않았다")
            }
            Err(e) => panic!("에이전트 창 0x{xid:x} 의 map state 를 못 읽었다: {e}"),
        }
    }
}

/// 새 창의 첫 workspace가 셸을 시작하지 못하면 workspace 없는 창을 등록하지 않고 창 생성이
/// 실패한다. 앱은 계속 응답하고 기존 창만 남는다. 셸 설정을 바꾸는 동안 다른 시험이 터미널을
/// 만들지 않도록 단독 lane에서 실행하고, 끝나면 격리 설정의 셸로 되돌린다.
#[cfg(feature = "gui")]
#[test]
fn a_new_window_whose_first_workspace_fails_is_refused_and_the_app_keeps_running() {
    struct RestoreShell(&'static common::TastyInstance);
    impl Drop for RestoreShell {
        fn drop(&mut self) {
            // 실패한 시험에서는 인스턴스가 응답하지 않을 수 있어 복원 호출로 다시 panic하지 않는다.
            if !std::thread::panicking() {
                self.0.call(
                    "debug.settings.apply",
                    json!({ "settings": { "general": { "shell": common::ISOLATED_SHELL } } }),
                );
            }
        }
    }
    let _lane = exclusive_lane();
    let tasty = common::shared();
    let before = tasty.call("window.list", json!({}));
    let _restore = RestoreShell(tasty);
    // 없는 경로는 실행 시 탐지한 셸로 대신하므로, 셸 이름을 가진 실행 불가 파일을 둔다.
    let shell_dir = tempfile::tempdir().expect("shell dir");
    let shell = shell_dir.path().join("bash");
    std::fs::write(&shell, "").expect("non-executable shell file");
    tasty.call(
        "debug.settings.apply",
        json!({ "settings": { "general": { "shell": shell.to_string_lossy() } } }),
    );
    let resp = tasty.call_raw("window.create", json!({}));
    assert!(
        resp.get("result").is_none(),
        "첫 workspace가 없는 창을 성공으로 답하면 안 된다: {resp}"
    );
    assert!(
        resp["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("first workspace")),
        "실패 사유가 첫 workspace를 가리켜야 한다: {resp}"
    );
    let windows = tasty.call("window.list", json!({}));
    assert_eq!(
        windows.as_array().map(Vec::len),
        before.as_array().map(Vec::len),
        "앱이 계속 응답하고 기존 창만 남아야 한다: {windows}"
    );
}

/// 창 생성이 필요해 GUI 조합에서만 컴파일한다. headless에는 창 생성 핸들러와 window.list가 없다.
/// 헤드리스 CI의 제외 이름 일치는 headless_skip_names_are_exact 검사에서 확인한다.
#[cfg(feature = "gui")]
#[test]
fn multi_window_owner_routing() {
    // 포커스가 첫 창에 남은 상태에서 새 창의 서피스에 요청해 소유 창 라우팅을 확인한다.
    let _lane = exclusive_lane();
    let tasty = common::shared();
    let ws = tasty.create_workspace("e2e-multi-window");
    let sid = ws.surface_id;
    tasty.wait_for_shell(sid);

    let ids_before: std::collections::HashSet<u64> = tasty
        .call("surface.list", json!({}))
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|s| s["id"].as_u64())
        .collect();

    let focused_window = |tasty: &common::TastyInstance| -> Option<u64> {
        tasty
            .call("window.list", json!({}))
            .as_array()
            .and_then(|ws| ws.iter().find(|w| w["focused"] == true))
            .and_then(|w| w["id"].as_u64())
    };
    let focused_before = focused_window(tasty);
    assert!(
        focused_before.is_some(),
        "창 생성 전 포커스된 창을 찾지 못해 포커스 유지 여부를 비교할 수 없다"
    );

    let create_resp = tasty.call("window.create", json!({}));
    assert_eq!(
        create_resp["created"], true,
        "window.create 성공 응답이 created=true 를 실어야 한다: {create_resp:?}"
    );
    assert!(
        create_resp["window_id"].as_u64().is_some(),
        "window.create 성공 응답에 window_id 가 있어야 한다: {create_resp:?}"
    );
    assert_eq!(
        focused_window(tasty),
        focused_before,
        "window.create 뒤 window.list 의 focused 는 원래 창이어야 한다: {create_resp:?}"
    );
    #[cfg(target_os = "linux")]
    wait_x11_window_viewable(
        create_resp["window_id"]
            .as_u64()
            .expect("window.create 성공 응답에 window_id 가 있어야 한다"),
    );

    let start = std::time::Instant::now();
    let new_sid = loop {
        let arr = tasty
            .call("surface.list", json!({}))
            .as_array()
            .cloned()
            .unwrap_or_default();
        let new_ids: Vec<u64> = arr
            .iter()
            .filter_map(|s| s["id"].as_u64())
            .filter(|id| !ids_before.contains(id))
            .collect();
        if let Some(&id) = new_ids.first()
            && arr
                .iter()
                .any(|s| s["id"].as_u64() == Some(id) && s["pty_ready"].as_bool() == Some(true))
        {
            break id;
        }
        if start.elapsed() > Duration::from_secs(10) {
            panic!("second window surface did not appear in 10s. surface.list = {arr:?}");
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let surfaces = tasty
        .call("surface.list", json!({}))
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        surfaces.iter().any(|s| s["id"].as_u64() == Some(sid)),
        "surface.list 가 첫 윈도우의 surface={sid} 를 빠뜨림: {surfaces:?}"
    );
    assert!(
        surfaces.iter().any(|s| s["id"].as_u64() == Some(new_sid)),
        "surface.list 가 두번째 윈도우의 surface={new_sid} 를 빠뜨림: {surfaces:?}"
    );

    let workspaces = tasty
        .call("workspace.list", json!({}))
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        workspaces.len() >= 2,
        "workspace.list 가 모든 engine 의 workspace 를 합쳐 반환해야: {workspaces:?}"
    );

    let ws_ids: std::collections::HashSet<u64> =
        workspaces.iter().filter_map(|w| w["id"].as_u64()).collect();
    let tree = tasty
        .call("tree", json!({}))
        .as_array()
        .cloned()
        .unwrap_or_default();
    let tree_ids: std::collections::HashSet<u64> =
        tree.iter().filter_map(|w| w["id"].as_u64()).collect();
    assert_eq!(
        tree_ids, ws_ids,
        "tree와 workspace.list의 워크스페이스 집합이 다르다. 창별 수집 범위를 확인한다. tree={tree:?} workspace.list={workspaces:?}"
    );

    tasty.set_mark(sid);
    let send_first = tasty.call(
        "surface.send",
        json!({"surface_id": sid, "text": "echo W1_owner_route\n"}),
    );
    assert_eq!(
        send_first["sent"], true,
        "owner-based routing 으로 첫 윈도우 surface 에 send 가능해야: {send_first:?}"
    );
    let out = tasty.wait_for_output(sid, "W1_owner_route", Duration::from_secs(5));
    assert!(
        out.contains("W1_owner_route"),
        "첫 윈도우 surface 가 명령을 실행하지 못함: {out:?}"
    );

    tasty.set_mark(new_sid);
    let send_second = tasty.call(
        "surface.send",
        json!({"surface_id": new_sid, "text": "echo W2_owner_route\n"}),
    );
    assert_eq!(send_second["sent"], true);
    let out2 = tasty.wait_for_output(new_sid, "W2_owner_route", Duration::from_secs(5));
    assert!(out2.contains("W2_owner_route"));

    // 잠금은 남은 창을 정리하지 않는다. 뒤의 시나리오에 영향을 주지 않도록 닫힘까지 확인한다.
    let win_id = create_resp["window_id"]
        .as_u64()
        .expect("window.create 가 window_id 를 줬다");
    let close_resp = tasty.call_raw("window.close", json!({ "id": win_id }));
    assert!(
        close_resp.get("error").is_none(),
        "만든 창을 닫지 못하면 잔여가 남는다: {close_resp}"
    );
    let closing = std::time::Instant::now();
    loop {
        let n = tasty
            .call("window.list", json!({}))
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        if n <= 1 {
            break;
        }
        assert!(
            closing.elapsed() < Duration::from_secs(5),
            "창을 닫으라고 했는데 5 초가 지나도 window.list 가 {n} 이다"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// 플러그인 매니저를 실제로 초기화한 인스턴스에서 목록 조회 응답을 확인한다.
#[test]
fn plugin_list_answers_without_a_window() {
    let _lane = lane();
    let tasty = common::shared();
    let resp = tasty.call_raw("plugin.list", json!({}));

    assert!(
        resp.get("error").is_none(),
        "plugin.list 가 에러로 답했다: {resp}"
    );
    let plugins = resp
        .get("result")
        .and_then(|r| r.get("plugins"))
        .and_then(|p| p.as_array())
        .unwrap_or_else(|| panic!("plugin.list 응답에 plugins 배열이 없다: {resp}"));

    // 설치 개수는 홈 상태에 따라 달라지므로 배열과 항목 형태만 확인한다.
    for p in plugins {
        assert!(
            p.get("id").and_then(|v| v.as_str()).is_some(),
            "plugin 항목에 id 가 없다: {p}"
        );
    }
}

/// 미설치 플러그인 오류와 매니저 부재 오류를 구별한다. 매니저 초기화를 빠뜨려도 통과하지 않아야 한다.
#[test]
fn plugin_show_distinguishes_an_unknown_plugin_from_a_missing_manager() {
    let _lane = lane();
    let tasty = common::shared();
    let resp = tasty.call_raw(
        "plugin.show",
        json!({"id": "no-such-plugin-in-any-installation"}),
    );

    let err = resp
        .get("error")
        .unwrap_or_else(|| panic!("없는 plugin 인데 성공으로 답했다: {resp}"));
    let code = err.get("code").and_then(|c| c.as_i64());
    assert_eq!(
        code,
        Some(-32003),
        "없는 plugin 은 -32003 이어야 한다(-32000 이면 매니저 자체가 안 세워진 것): {resp}"
    );
}

/// 공유 인스턴스의 플러그인 상태를 바꾸지 않도록 id 없이 호출한다. 인자 오류 응답으로 핸들러가 있는지 확인한다.
#[test]
fn lifecycle_toggles_answer_without_a_window() {
    let _lane = lane();
    let tasty = common::shared();
    for method in ["plugin.enable", "plugin.disable"] {
        let resp = tasty.call_raw(method, json!({}));
        let code = resp
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_i64());
        assert_eq!(
            code,
            Some(-32602),
            "{method} 는 arm 이 있어 인자 오류로 답해야 한다. `-32017` 이면 그 조합에 \
             arm 이 없는 것이고, `-32601` 이면 표에서 이름이 빠진 것이다: {resp}"
        );
    }
}

/// 헤드리스의 remove는 지원하지 않는다. 조회·토글 허용 범위와 구별해 검사한다.
#[cfg(not(feature = "gui"))]
#[test]
fn the_remaining_lifecycle_methods_are_still_absent_in_a_headless_daemon() {
    let _lane = lane();
    let tasty = common::shared();
    let resp = tasty.call_raw("plugin.remove", json!({"id": "anything"}));
    let code = resp
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64());
    assert_eq!(
        code,
        Some(-32017),
        "헤드리스에서 plugin.remove 는 아직 arm 이 없는 메서드여야 한다. `-32601` 이 \
         왔다면 표에서 이름이 빠진 것이고, 그러면 호출자가 오타와 구분할 수 없다: {resp}"
    );
}

/// 헤드리스에는 파일 식별 결과를 열 창이 없으므로 dispatch를 수락하면 안 된다.
#[cfg(not(feature = "gui"))]
#[test]
fn file_dispatch_is_refused_rather_than_accepted_in_a_headless_daemon() {
    let _lane = lane();
    let tasty = common::shared();
    let resp = tasty.call_raw(
        "file_handler.dispatch",
        json!({"path": "/definitely/not/a/tasty/test/page.html", "depth": "cheap"}),
    );
    let code = resp
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64());
    assert_eq!(
        code,
        Some(-32017),
        "헤드리스는 파일을 열 수 없으니 dispatch 를 수락했다고 답하면 안 된다: {resp}"
    );
    assert!(
        resp.get("result").is_none(),
        "거절 응답에 result 가 같이 실리면 호출자가 성공으로 읽는다: {resp}"
    );
}

/// 헤드리스에는 창이라는 개념이 없다. 빈 목록은 "창이 0개인 GUI"로 읽혀 호출자가 창을 만들려 하므로
/// 창 조회·생성은 조합에 없는 메서드(-32017)로 답하고, engine 조회는 system.info가 맡는다.
#[cfg(not(feature = "gui"))]
#[test]
fn window_methods_are_absent_rather_than_empty_in_a_headless_daemon() {
    let _lane = lane();
    let tasty = common::shared();
    for method in ["window.list", "view.list", "window.create"] {
        let resp = tasty.call_raw(method, json!({}));
        assert_eq!(
            resp.get("error")
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_i64()),
            Some(-32017),
            "헤드리스에서 {method} 는 조합에 없는 메서드로 답해야 한다. 빈 목록이나              `-32601` 이면 호출자가 창이 없는 GUI 나 이름 오타로 읽는다: {resp}"
        );
        assert!(
            resp.get("result").is_none(),
            "거절 응답에 result 가 같이 실리면 호출자가 성공으로 읽는다: {resp}"
        );
    }
    let info = tasty.call("system.info", json!({}));
    assert_eq!(
        info["scope"], "engine",
        "창 없이 engine 을 조회하는 경로는 system.info 다: {info}"
    );
}

/// 헤드리스에는 mirror 요청 큐를 attach로 보내는 GUI 경로가 없어 수락하면 결과를 돌려줄 수 없다.
/// mirror 구조 변경 거절은 core::attach_runtime의 별도 단위 시험에서 확인한다.
#[cfg(not(feature = "gui"))]
#[test]
fn mirror_forward_requests_are_refused_by_name_in_a_headless_daemon() {
    let _lane = lane();
    let tasty = common::shared();
    // Named-target validation precedes the unsupported headless dispatch fallback.
    let workspace = tasty.create_workspace("headless-mirror-forward-refusal");
    let surface_id = workspace.surface_id;
    let mut messages = Vec::new();
    for (method, params) in [
        (
            "git_viewer.query",
            json!({"kind": "status", "local_surface_id": surface_id}),
        ),
        (
            "markdown_mirror.content_request",
            json!({"surface_id": surface_id}),
        ),
    ] {
        let resp = tasty.call_raw(method, params);
        let error = resp.get("error");
        assert_eq!(
            error.and_then(|e| e.get("code")).and_then(|c| c.as_i64()),
            Some(-32017),
            "헤드리스는 {method} 를 수락하면 안 된다 — 결과를 보낼 쪽이 없다: {resp}"
        );
        assert!(
            resp.get("result").is_none(),
            "거절 응답에 result 가 같이 실리면 호출자가 성공으로 읽는다: {resp}"
        );
        let message = error
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_string();
        assert!(
            message.contains(method),
            "거절 문구가 무엇이 거절됐는지 말해야 한다: {message}"
        );
        messages.push(message);
    }
    assert_ne!(
        messages[0], messages[1],
        "두 자리의 거절 사유가 같은 문자열이면 무엇이 거절했는지 못 가른다"
    );
}

/// 헤드리스는 레이아웃 슬롯을 점유하지 않는다. 필드 누락과 명시적인 null을 구별한다.
#[cfg(not(feature = "gui"))]
#[test]
fn a_headless_daemon_answers_that_it_holds_no_layout_slot() {
    let _lane = lane();
    let tasty = common::shared();
    let info = tasty.call("system.info", json!({}));
    assert!(
        info.as_object()
            .is_some_and(|o| o.contains_key("layout_slot")),
        "system.info 가 layout_slot 칸을 아예 안 냈다 — 소비자 쪽에서 null 과 구별되지 않는다: {info}"
    );
    assert_eq!(
        info["layout_slot"],
        serde_json::Value::Null,
        "헤드리스는 레이아웃 슬롯을 잡지 않는다: {info}"
    );
}

/// 실제 부팅 로그가 기본 warn 필터에서 복원 미지원 안내를 내보내는지 확인한다.
/// 메시지 반환값만 검사하면 호출 누락이나 로그 레벨 변경을 찾지 못한다.
#[cfg(not(feature = "gui"))]
#[test]
fn a_headless_daemon_warns_at_boot_that_restore_layout_is_ignored() {
    let _lane = lane();
    let tasty = common::TastyInstance::spawn_with_restore_layout();
    // 메인 루프 응답을 기다려 부팅 안내 이후에 검사한다. 이어지는 폴링은 stderr 수집 지연을 기다린다.
    tasty.call("system.info", json!({}));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let line = loop {
        if let Some(line) = tasty.find_stderr(|l| l.contains("general.restore_layout is on")) {
            break line;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "헤드리스 부팅 로그에서 restore_layout 미지원 안내를 찾지 못했다. 호출·로그 레벨·수집 상태를 확인한다:\n{}",
            tasty.startup_diagnostics()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    // 파이프에도 ANSI 색이 들어올 수 있어 레벨 토큰 비교 전에 제거한다.
    let mut plain = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    assert!(
        plain.split_whitespace().any(|token| token == "WARN"),
        "부팅 고지가 warn 이 아니다 — 기본 필터가 warn 이라 이 줄이 보인 것은 필터가 바뀐 탓일 수 있다: {plain}"
    );
    assert!(
        plain.contains("does not save or restore layouts"),
        "고지가 무엇을 안 하는지 말하지 않는다: {plain}"
    );
}

/// OS 열기는 기존 브라우저 등 사용자 프로세스에 영향을 줄 수 있어 debug 기록 모드로만 검사한다.
/// 이 모드가 없는 release 빌드에서는 실행하지 않는다. 격리 홈과 디스플레이만으로 OS 열기를 막을 수는 없다.
#[cfg(all(feature = "gui", debug_assertions))]
#[test]
fn directory_dispatch_is_recorded_instead_of_opened_under_the_harness() {
    let _lane = lane();
    let tasty = common::shared();
    let dir = tasty.tasty_home().join("os-open-probe-dir");
    std::fs::create_dir_all(&dir).expect("create probe dir");
    let resp = tasty.call_raw(
        "file_handler.dispatch",
        json!({"path": dir.to_str().unwrap(), "depth": "cheap"}),
    );
    assert_eq!(
        resp.pointer("/result/accepted"),
        Some(&json!(true)),
        "gui 는 디렉토리 dispatch 를 접수해야 한다: {resp}"
    );
    let log = tasty.os_open_log();
    let needle = dir.to_str().unwrap().to_string();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text
            .lines()
            .any(|l| l.starts_with("open_uri\t") && l.contains(&needle))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "디렉토리 OS 열기가 {} 에 기록되지 않았다 — 기록 대신 실제로 띄웠을 수 있다. 기록 내용: {text:?}",
            log.display()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// 공유 user 설정을 바꾸므로 단독 실행하고 파일 삭제·reload로 복구한다.
#[test]
fn file_handler_reload_reports_the_entries_it_dropped() {
    let _lane = exclusive_lane();
    let tasty = common::shared();
    let path = tasty.tasty_home().join("file-handlers.toml");
    assert!(
        !path.exists(),
        "격리 홈에 user 설정이 이미 있으면 복원 기준이 없다: {}",
        path.display()
    );
    std::fs::write(
        &path,
        r#"
[[handler]]
id = "md-as-html"
detector = "markdown"
[handler.action]
kind = "system"

[[handler]]
id = "user/no-action"
detector = "markdown"

[[handler]]
id = "user/good"
detector = "markdown"
[handler.action]
kind = "system"

[[handler]]
id = "com.example.absent/viewer"
priority = 10
"#,
    )
    .expect("user 설정 쓰기");

    let resp = tasty.call("file_handler.reload", json!({}));

    // 응답 단정에 실패해도 설정이 남지 않도록 먼저 복구한다.
    std::fs::remove_file(&path).expect("user 설정 지우기");
    let restored = tasty.call("file_handler.reload", json!({}));

    assert_eq!(resp["exists"], json!(true), "기존 필드는 그대로다: {resp}");
    assert!(resp["path"].is_string(), "기존 필드는 그대로다: {resp}");
    assert_eq!(
        resp["rejected"],
        json!([
            {"id": "md-as-html", "reason": "missing_owner_prefix"},
            {"id": "user/no-action", "reason": "missing_detector_or_action"},
            {"id": "com.example.absent/viewer", "reason": "target_not_contributed"},
        ]),
        "적용하지 않은 항목과 사유가 응답에 있어야 한다: {resp}"
    );
    assert_eq!(
        restored["rejected"],
        json!([]),
        "적용하지 않은 것이 없으면 빈 배열이다: {restored}"
    );
}

/// 병합 결과뿐 아니라 출처별 원본도 확인한다. 공용 설정을 바꾸므로 단독 실행하고 원래 상태로 복구한다.
#[test]
fn file_handler_detectors_reports_the_merged_state_and_each_source() {
    let _lane = exclusive_lane();
    let tasty = common::shared();
    let path = tasty.tasty_home().join("file-handlers.toml");
    assert!(
        !path.exists(),
        "격리 홈에 user 설정이 이미 있으면 복원 기준이 없다: {}",
        path.display()
    );
    let find = |resp: &serde_json::Value, id: &str| -> serde_json::Value {
        resp["detectors"]
            .as_array()
            .and_then(|a| a.iter().find(|d| d["id"] == json!(id)).cloned())
            .unwrap_or_else(|| panic!("`{id}` detector 가 목록에 없다: {resp}"))
    };

    let before = tasty.call("file_handler.detectors", json!({}));
    let html = find(&before, "html");
    assert_eq!(
        html["contributions"]
            .as_array()
            .map(|a| a.iter().map(|c| c["origin"].clone()).collect::<Vec<_>>()),
        Some(vec![json!("host")]),
        "user 설정이 없으면 host 한 출처다: {html}"
    );
    assert_eq!(html["disabled"], json!(false), "{html}");
    assert!(
        html["rules"].as_array().is_some_and(|r| r
            .iter()
            .any(|r| r["kind"] == json!("extension") && r["origin"] == json!("host"))),
        "rule 은 설정 파일 키와 출처로 적힌다: {html}"
    );

    std::fs::write(
        &path,
        r#"
[[detector]]
id = "html"
display_name_i18n_key = "user.html"
"#,
    )
    .expect("user 설정 쓰기");
    tasty.call("file_handler.reload", json!({}));
    let after = tasty.call("file_handler.detectors", json!({}));

    std::fs::remove_file(&path).expect("user 설정 지우기");
    tasty.call("file_handler.reload", json!({}));
    let restored = tasty.call("file_handler.detectors", json!({}));

    let html = find(&after, "html");
    assert_eq!(
        html["display_name_i18n_key"],
        json!("user.html"),
        "user patch 가 병합 결과에 반영된다: {html}"
    );
    let contribs = html["contributions"].as_array().expect("contributions");
    assert_eq!(contribs.len(), 2, "host · user 두 출처: {html}");
    assert!(
        contribs.iter().any(|c| c["origin"] == json!("user")
            && c["display_name_i18n_key"] == json!("user.html")
            && c["disabled"].is_null()),
        "user 원본이 적은 그대로 실린다(켜기/끄기는 안 적어 null): {html}"
    );
    assert_eq!(
        find(&restored, "html")["contributions"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "user 설정을 지우고 reload 하면 host 한 출처로 돌아간다: {restored}"
    );
}

/// 없는 대상을 지정한 요청은 다른 창으로 넘기지 않고 두 빌드 조합에서 모두 거절해야 한다.
#[test]
fn a_request_naming_an_unowned_target_is_rejected() {
    let _lane = lane();
    let tasty = common::shared();

    let resp = tasty.call_raw(
        "workspace.create",
        json!({ "workspace_id": 999_999, "name": "unowned-target-probe" }),
    );
    assert!(
        resp.get("error").is_some(),
        "없는 대상을 지정한 요청이 거절되지 않았다: {resp}"
    );
    let msg = resp["error"]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("999999") && msg.contains("workspace"),
        "에러가 무엇을 못 찾았는지 말해야 고칠 수 있다: {resp}"
    );

    // image namespace에 있지만 GUI 호스트가 직접 변환하는 호출도 확인한다. 헤드리스에는 해당 host 핸들러가 없어 이 경우는 GUI에서만 검사한다.
    #[cfg(feature = "gui")]
    {
        let converted = tasty.call_raw(
            "image.open",
            json!({ "surface_id": 999_999, "path": "/tmp/does-not-exist.png" }),
        );
        let via_msg = converted
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or_default();
        assert!(
            via_msg.contains("surface 999999"),
            "호스트가 변환하는 plugin namespace 호출도 지목한 대상이 없으면 같은 이유로 거절돼야 한다: \
             {converted}"
        );
    }

    // 플러그인 소유 namespace는 호스트 대상 검사로 차단하지 않고 플러그인에 전달한다.
    let forwarded = tasty.call_raw("markdown.recent", json!({ "surface_id": 999_999 }));
    assert!(
        forwarded.get("result").is_some(),
        "plugin namespace 의 메서드는 id 를 실어도 forward 돼야 한다: {forwarded}{}",
        common::bundle_staging_note()
    );

    let ok = tasty.call_raw("workspace.create", json!({ "name": "no-target-probe" }));
    assert!(
        ok.get("result").is_some(),
        "대상을 지목하지 않은 생성 요청은 계속 동작해야 한다: {ok}"
    );
}

/// `system.pressure` 의 plugin 왕복 수가 한동안 그대로일 때 그 값을 돌려준다.
/// 늦게 도착한 plugin 응답까지 센 뒤에 비교하려고 기다린다.
fn settled_plugin_round_trips(tasty: &TastyInstance) -> u64 {
    let read = || {
        tasty.call("system.pressure", json!({}))["plugin_round_trip"]["matched"]
            .as_u64()
            .expect("system.pressure 에 plugin_round_trip.matched 가 있어야 한다")
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut last = read();
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let now = read();
        if now == last {
            return now;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "plugin 왕복 수가 10초 동안 멈추지 않았다: {last} → {now}"
        );
        last = now;
    }
}

/// 실제 인스턴스에서 plugin namespace 메서드 IPC 하나가 plugin 에 정확히 한 번 전달되는지
/// 응답이 매칭된 plugin 왕복 수의 차분으로 잰다. 단위 시험이 지나지 않는 GUI
/// `ipc_step_routing` 의 namespace 단계 밖 추가 전달도 여기서 드러난다.
/// `markdown` namespace 에 IPC pre/post hook 을 건 활성 extension 이 없다는 것이 전제다.
/// 그런 extension 이 있으면 호출 하나에 hook 왕복이 더해져 차분이 2 이상이 된다.
#[test]
fn a_plugin_namespace_call_is_forwarded_to_the_plugin_exactly_once() {
    let _lane = exclusive_lane();
    let tasty = common::shared();
    // 첫 호출은 plugin 기동·연결 지연을 흡수한다. 그 뒤 기준값을 읽는다.
    let warm = tasty.call_raw("markdown.recent", json!({}));
    assert!(
        warm.get("result").is_some(),
        "markdown.recent 가 plugin 에서 답해야 한다: {warm}{}",
        common::bundle_staging_note()
    );
    let before = settled_plugin_round_trips(tasty);
    let resp = tasty.call_raw("markdown.recent", json!({}));
    assert!(resp.get("result").is_some(), "{resp}");
    let after = settled_plugin_round_trips(tasty);
    assert_eq!(
        after - before,
        1,
        "namespace 메서드 하나에 plugin 왕복이 {} 번 기록됐다 (전: {before}, 후: {after})",
        after - before
    );
}

/// 환경 의존적인 성공 대신 메서드 부재·빌드 미지원 오류가 아닌지 검사해 라우팅을 확인한다.
#[test]
fn app_layer_methods_that_need_no_window_answer_in_both_combos() {
    let _lane = lane();
    let tasty = common::shared();

    for method in [
        "clipboard.set_text",
        "remote.workspaces",
        "agent.task_await",
        "approval.await",
    ] {
        let resp = tasty.call_raw(method, json!({}));
        let code = resp["error"]["code"].as_i64();
        // 메서드 표 누락과 핸들러 누락은 다른 코드이므로 둘 다 확인한다.
        assert_ne!(
            code,
            Some(-32601),
            "`{method}` 는 창이 없어도 답이 정의된다 — 두 조합에서 라우팅돼야 한다: {resp}"
        );
        assert_ne!(
            code,
            Some(-32017),
            "`{method}` 의 arm 이 이 조합에서 사라졌다 — 창을 안 보는데 게이트 뒤로 \
             들어갔다는 뜻이다: {resp}"
        );
    }

    let resp = tasty.call_raw("window.list", json!({}));
    let code = resp["error"]["code"].as_i64();
    #[cfg(feature = "gui")]
    assert_ne!(code, Some(-32601), "gui 는 window.list 에 답한다: {resp}");
    #[cfg(not(feature = "gui"))]
    assert_eq!(
        code,
        Some(-32017),
        "헤드리스의 window.list는 빌드 미지원 오류여야 한다. 메서드 표 누락과 구별한다: {resp}"
    );
}

/// 플랫폼 미지원과 없는 메서드를 구별하는 실제 응답을 확인한다.
#[test]
fn a_platform_gated_debug_method_says_why_not_that_it_is_missing() {
    let _lane = lane();
    let tasty = common::shared();

    for method in ["surface.raw_key", "surface.switch_input_source"] {
        let resp = tasty.call_raw(method, json!({}));
        let code = resp["error"]["code"].as_i64();

        #[cfg(all(target_os = "macos", feature = "gui"))]
        assert_ne!(
            code,
            Some(-32601),
            "macOS gui 에서는 실제 핸들러가 받는다: {resp}"
        );

        #[cfg(not(all(target_os = "macos", feature = "gui")))]
        {
            assert_eq!(
                code,
                Some(-32015),
                "지원하지 않는 플랫폼에서는 메서드 부재 대신 -32015와 사유를 반환해야 한다: {resp}"
            );
            let msg = resp["error"]["message"].as_str().unwrap_or_default();
            assert!(
                msg.contains("macOS-only"),
                "코드만으로는 무엇이 부족한지 알 수 없다 — 사유가 함께 와야 한다: {resp}"
            );
        }
    }
}

/// 창 없이 가능한 theme 조회와 렌더러가 필요한 webview 변경을 구별한다.
#[test]
fn an_engine_query_that_reads_no_window_answers_in_both_combos() {
    let _lane = lane();
    let tasty = common::shared();

    let resp = tasty.call_raw("theme.query", json!({}));
    assert!(
        resp.get("result").is_some(),
        "`theme.query` 는 전역 Theme 과 settings 만 읽는다 — 두 조합에서 답해야 한다: {resp}"
    );
    assert!(
        resp["result"].get("colors").is_some(),
        "색상표가 실려야 한다 — 라우팅만 되고 빈 답이면 호출자에게 쓸모가 없다: {resp}"
    );

    let resp = tasty.call_raw("webview.set_url", json!({}));
    let code = resp["error"]["code"].as_i64();
    assert_eq!(
        code,
        Some(-32602),
        "webview.set_url records source in both builds and validates parameters: {resp}"
    );

    let resp = tasty.call_raw("surface.html_script", json!({}));
    let code = resp["error"]["code"].as_i64();
    #[cfg(feature = "gui")]
    assert_eq!(
        code,
        Some(-32602),
        "gui 는 surface.html_script 에 답하고 빠진 surface_id 를 거절한다: {resp}"
    );
    #[cfg(not(feature = "gui"))]
    assert_eq!(
        code,
        Some(-32017),
        "헤드리스의 surface.html_script는 빌드 미지원 오류여야 한다: {resp}"
    );
}
/// 여러 대상 종류에 없는 ID를 지정했을 때 거절하는지 확인하고, 있는 ID의 일부 요청도 함께 검사한다.
#[test]
fn an_unowned_target_is_rejected_for_every_resource_kind() {
    let _lane = lane();
    let tasty = common::shared();
    const MISSING: u64 = 999_999;

    // 플러그인 namespace는 호스트 소유 검사를 거치지 않으므로 호스트 메서드를 선택한다.
    let cases: Vec<(&str, &str, &str, serde_json::Value)> = vec![
        (
            "workspace",
            "workspace_id",
            "workspace.create",
            json!({ "name": "unowned-kind-probe" }),
        ),
        (
            "workspace",
            "target_workspace_id",
            "workspace.move",
            json!({}),
        ),
        ("surface", "surface_id", "surface.close", json!({})),
        ("surface", "surface", "terminal.children", json!({})),
        ("surface", "parent", "terminal.kill", json!({})),
        ("surface", "target", "surface.split", json!({})),
        (
            "surface",
            "to_surface_id",
            "message.send",
            json!({ "message": "x" }),
        ),
        ("tab", "tab_id", "tab.close", json!({})),
        ("pane", "pane_id", "pane.close", json!({})),
        ("pane", "pane", "tab.new", json!({})),
        (
            "pane",
            "target_pane_id",
            "split",
            json!({ "level": "pane" }),
        ),
        ("headless pty", "id", "pty.write", json!({ "data": "x" })),
        ("surface hook", "hook_id", "hook.unset", json!({})),
        (
            "output observer",
            "observer_id",
            "output.observe_stop",
            json!({}),
        ),
    ];

    for (kind, key, method, base) in &cases {
        let mut params = base.clone();
        params[*key] = json!(MISSING);
        let resp = tasty.call_raw(method, params);
        let msg = resp
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or_default();
        assert!(
            !msg.contains("Method not found"),
            "{method}를 찾지 못해 대상 ID 검사에 도달하지 않았다: {resp}"
        );
        assert!(
            msg.contains(&MISSING.to_string()),
            "{key}({kind}) 를 없는 id 로 실었는데 그것을 말하는 거절이 안 나왔다 \
             — 지목한 대상이 없는데 조용히 성공했을 수 있다: {resp}"
        );
        assert!(
            msg.contains(kind),
            "거절이 **무엇을** 못 찾았는지 말해야 고칠 수 있다({kind} 를 기대): {resp}"
        );
    }

    // 있는 대상의 요청은 다른 이유로 실패해도 허용한다. 여기서는 대상 소유 검사 오류만 확인한다.
    let tree = tasty.call("tree", json!({}));
    let live_ws = tree[0]["id"].as_u64().expect("살아 있는 workspace id");
    let surfaces = tasty.call("surface.list", json!({}));
    let live_surface = surfaces
        .as_array()
        .and_then(|a| a.first())
        .and_then(|s| s["id"].as_u64())
        .expect("살아 있는 surface id");
    let panes = tasty.call("pane.list", json!({}));
    let live_pane = panes
        .as_array()
        .and_then(|a| a.first())
        .and_then(|p| p["id"].as_u64())
        .expect("살아 있는 pane id");

    let live: Vec<(&str, &str, serde_json::Value, u64)> = vec![
        (
            "workspace_id",
            "workspace.create",
            json!({ "name": "live-kind-probe" }),
            live_ws,
        ),
        ("surface", "terminal.children", json!({}), live_surface),
        ("pane", "tab.new", json!({}), live_pane),
    ];
    for (key, method, base, id) in live {
        let mut params = base.clone();
        params[key] = json!(id);
        let resp = tasty.call_raw(method, params);
        let msg = resp
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or_default();
        assert!(
            !msg.contains("no live"),
            "{key}={id} 는 살아 있는데 소유 검사가 걸렸다 — 검사가 너무 넓다: {resp}"
        );
    }
}

/// 닫힌 surface에는 metadata를 쓸 수 없다(`surface.meta.set`·`memory.put` 모두). 닫기 전에 쓴
/// 값도 닫기와 함께 지워진다.
/// metadata 저장소는 surface 소유 검사를 거치지 않는 `memory.list`로 직접 확인한다.
#[test]
fn a_closed_surface_takes_no_metadata() {
    let _lane = lane();
    let tasty = common::shared();
    let anchor = tasty.first_surface_id();
    let split = tasty.call(
        "split",
        json!({ "level": "surface", "target_surface": anchor, "direction": "horizontal" }),
    );
    let closed = split["new_surface_id"]
        .as_u64()
        .expect("split 이 새 surface 를 만든다");
    tasty.call(
        "surface.meta.set",
        json!({ "surface_id": closed, "key": "role", "value": "live" }),
    );
    tasty.call("surface.close", json!({ "surface_id": closed }));

    let resp = tasty.call_raw(
        "surface.meta.set",
        json!({ "surface_id": closed, "key": "role", "value": "after-close" }),
    );
    assert_eq!(
        resp["error"]["code"].as_i64(),
        Some(-32602),
        "닫힌 surface 에 쓰기가 다른 surface 를 못 찾은 거절과 같은 코드로 거절돼야 한다: {resp}"
    );
    let msg = resp["error"]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains(&closed.to_string()) && msg.contains("surface"),
        "거절이 어느 surface 를 못 찾았는지 말해야 한다: {resp}"
    );

    // memory.put 으로 같은 surface scope 에 쓰는 길도 같은 이유로 거절한다.
    let via_memory = tasty.call_raw(
        "memory.put",
        json!({ "scope": format!("surface:{closed}"), "key": "role", "value": "via-memory" }),
    );
    assert_eq!(
        via_memory["error"]["code"].as_i64(),
        Some(-32602),
        "닫힌 surface scope 에 memory.put 이 거절돼야 한다: {via_memory}"
    );
    assert!(
        via_memory["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains(&closed.to_string())),
        "거절이 어느 surface 를 못 찾았는지 말해야 한다: {via_memory}"
    );

    let left = tasty.call(
        "memory.list",
        json!({ "scope": format!("surface:{closed}"), "prefix": "role" }),
    );
    assert_eq!(
        left["count"].as_u64(),
        Some(0),
        "닫힌 surface 의 metadata 가 저장소에 남았다: {left}"
    );
}

/// 창을 사용하지 않는 debug 메서드의 라우팅을 확인한다. 이 debug 실행으로 release 격리까지 검증하지는 않는다.
#[test]
fn debug_surfaces_that_read_no_window_answer_in_both_combos() {
    let _lane = lane();
    let tasty = common::shared();

    for method in [
        "debug.lua.eval",
        "debug.event_bus.list_subscribers",
        "debug.event_bus.publish",
        "debug.event_bus.trace",
        "debug.extension.invoke_hook",
        "debug.popup.list",
        "debug.fullscreen.list",
    ] {
        let resp = tasty.call_raw(method, json!({}));
        let code = resp["error"]["code"].as_i64();
        assert_ne!(
            code,
            Some(-32601),
            "창을 사용하지 않는 debug 메서드 {method}가 라우팅되지 않았다: {resp}"
        );
        assert_ne!(
            code,
            Some(-32017),
            "`{method}` 의 arm 이 이 조합에서 사라졌다: {resp}"
        );
    }

    // tool 조회와 fullscreen 열기는 창이 필요하다. popup은 헤드리스에 닫는 경로가 없어 열기도 허용하지 않는다.
    for method in [
        "debug.tool.list",
        "debug.popup.open",
        "debug.fullscreen.open",
    ] {
        let resp = tasty.call_raw(method, json!({}));
        let code = resp["error"]["code"].as_i64();
        #[cfg(feature = "gui")]
        assert_ne!(code, Some(-32601), "gui 는 `{method}` 에 답한다: {resp}");
        #[cfg(not(feature = "gui"))]
        assert_eq!(
            code,
            Some(-32017),
            "헤드리스에서는 이 메서드가 빌드 미지원 오류를 반환해야 한다: {resp}"
        );
    }
}

/// 여러 클라이언트의 응답을 확인한다. 기본 예산에서는 처리 중단이 보장되지 않으므로 재깨움 검증은 아래 별도 시험에서 한다.
#[test]
fn concurrent_requests_are_all_answered() {
    let _lane = lane();
    let tasty = common::shared();
    const CLIENTS: usize = 16;

    let rows: Vec<serde_json::Value> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..CLIENTS)
            .map(|_| s.spawn(|| tasty.call("workspace.list", json!({}))))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("동시 요청 스레드가 패닉했다"))
            .collect()
    });

    assert_eq!(rows.len(), CLIENTS, "응답 수가 요청 수와 다르다");
    for (i, row) in rows.iter().enumerate() {
        assert!(
            row.as_array().is_some_and(|a| !a.is_empty()),
            "{i} 번째 동시 요청의 workspace.list 가 비었다: {row:?}"
        );
    }
}

/// debug 시간 예산을 0으로 주어 한 명령 뒤 회차를 중단하고 남은 요청의 응답을 확인한다.
/// 응답에 상한을 두어 재깨움이 누락되면 무한히 기다리지 않게 한다.
#[test]
fn concurrent_requests_are_all_answered_when_every_round_is_cut() {
    let _lane = lane();
    let tasty =
        common::TastyInstance::spawn_with_env(&[("TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS", "0")]);
    const CLIENTS: usize = 16;
    const ANSWER_BOUND: Duration = Duration::from_secs(10);
    let port = tasty.port();

    let answers: Vec<Result<serde_json::Value, String>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..CLIENTS)
            .map(|i| {
                s.spawn(move || {
                    use std::io::{BufRead, BufReader, Write};
                    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))
                        .map_err(|e| format!("connect: {e}"))?;
                    stream
                        .set_read_timeout(Some(ANSWER_BOUND))
                        .map_err(|e| format!("timeout: {e}"))?;
                    let line = json!({"jsonrpc": "2.0", "id": i, "method": "workspace.list", "params": {}});
                    writeln!(stream, "{line}").map_err(|e| format!("write: {e}"))?;
                    let mut got = String::new();
                    BufReader::new(stream)
                        .read_line(&mut got)
                        .map_err(|e| format!("no answer within {ANSWER_BOUND:?}: {e}"))?;
                    serde_json::from_str(got.trim()).map_err(|e| format!("json: {e}: {got}"))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("동시 요청 스레드가 패닉했다"))
            .collect()
    });

    let unanswered: Vec<String> = answers
        .iter()
        .enumerate()
        .filter_map(|(i, a)| a.as_ref().err().map(|e| format!("{i}: {e}")))
        .collect();
    assert!(
        unanswered.is_empty(),
        "처리 회차 중단 뒤 응답이 없는 요청이다. 재깨움과 연결 오류를 확인한다: {unanswered:?}"
    );
    for (i, a) in answers.iter().enumerate() {
        let resp = a.as_ref().expect("checked above");
        assert!(
            resp["result"].as_array().is_some_and(|a| !a.is_empty()),
            "{i} 번째 요청의 workspace.list 가 비었다: {resp}"
        );
    }
    let rounds = tasty.call("system.pressure", json!({}));
    assert!(
        rounds["queue_dispatch"]["rounds_stopped_by_time"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "시간 예산으로 중단된 회차가 없어 재깨움 경로를 확인할 수 없다: {}",
        rounds["queue_dispatch"]
    );
}

#[test]
fn concurrent_creations_keep_their_creation_route_after_binding_the_engine() {
    let _lane = exclusive_lane();
    let tasty = common::shared();
    let before = tasty.call("ui.state", json!({}))["active_workspace"].clone();
    const CLIENTS: usize = 8;
    let barrier = std::sync::Barrier::new(CLIENTS);
    let workspaces: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..CLIENTS)
            .map(|index| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    tasty.create_workspace(&format!("bound-creation-{index}"))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("concurrent create"))
            .collect()
    });
    let ids: std::collections::HashSet<_> =
        workspaces.iter().map(|workspace| workspace.id).collect();
    assert_eq!(ids.len(), CLIENTS);
    assert_eq!(
        tasty.call("ui.state", json!({}))["active_workspace"],
        before
    );
    for workspace in workspaces {
        assert!(
            tasty
                .call("surface.list", json!({}))
                .as_array()
                .unwrap()
                .iter()
                .any(|surface| surface["id"].as_u64() == Some(workspace.surface_id))
        );
        tasty.call("workspace.close", json!({"id":workspace.id}));
    }
}

/// 하네스 디스플레이 이름. inherit + Wayland 이면 Xlib 로 조회할 수 없어 None 이다.
#[cfg(all(target_os = "linux", feature = "gui"))]
fn x11_harness_display() -> Option<String> {
    let declared = std::env::var("TASTY_E2E_DISPLAY").unwrap_or_default();
    if declared.trim() != "inherit" {
        return Some(declared.trim().to_string());
    }
    if std::env::var("WAYLAND_DISPLAY").is_ok_and(|v| !v.is_empty()) {
        return None;
    }
    std::env::var("DISPLAY").ok()
}

/// native WebView 창과 그것을 품은 메인 창을 한 번에 읽은 결과.
/// native WebView 는 Linux 에서 메인 창의 X 자식 창이다(`src/host_api/webview/linux.rs`).
#[cfg(all(target_os = "linux", feature = "gui"))]
#[derive(Debug)]
struct X11WebViewCapture {
    /// 메인 창 기준 자식 창 위치·크기(물리 px).
    child: (i32, i32, i32, i32),
    /// 자식 창 왼쪽 위의 화면(root) 좌표. XTest 포인터 좌표의 기준이다.
    child_root: (i32, i32),
    child_xid: u64,
    child_pixels: Vec<u32>,
}

#[cfg(all(target_os = "linux", feature = "gui"))]
impl X11WebViewCapture {
    fn child_at(&self, px: i32, py: i32) -> u32 {
        self.child_pixels[(py * self.child.2 + px) as usize]
    }

    /// 자식 창 네 변에서 한 칸 안쪽 가운데 픽셀. 순서는 왼쪽·오른쪽·위·아래다.
    fn child_edges(&self) -> [u32; 4] {
        let (_, _, w, h) = self.child;
        [
            self.child_at(1, h / 2),
            self.child_at(w - 2, h / 2),
            self.child_at(w / 2, 1),
            self.child_at(w / 2, h - 2),
        ]
    }
}

/// `parent` 창의 보이는 자식 하나의 위치와 픽셀을 읽는다. 자식이 없으면 None 이다.
#[cfg(all(target_os = "linux", feature = "gui"))]
fn x11_capture_webview(display: &str, parent: u64) -> Result<Option<X11WebViewCapture>, String> {
    use x11_dl::xlib;
    let x = xlib::Xlib::open().map_err(|e| format!("Xlib::open: {e}"))?;
    let cname = std::ffi::CString::new(display).map_err(|e| e.to_string())?;
    // SAFETY: cname 은 NUL 종단 문자열이며 반환된 포인터가 null 인지 확인한다.
    let dpy = unsafe { (x.XOpenDisplay)(cname.as_ptr()) };
    if dpy.is_null() {
        return Err(format!("XOpenDisplay({display}) failed"));
    }
    let parent = parent as xlib::Window;
    let mut root = 0;
    let mut parent_out = 0;
    let mut children: *mut xlib::Window = std::ptr::null_mut();
    let mut count = 0;
    // SAFETY: dpy 는 열린 연결이고 출력 인자는 모두 쓰기 가능한 지역 변수다.
    let ok = unsafe {
        (x.XQueryTree)(
            dpy,
            parent,
            &mut root,
            &mut parent_out,
            &mut children,
            &mut count,
        )
    };
    let kids: Vec<xlib::Window> = if ok != 0 && !children.is_null() {
        // SAFETY: XQueryTree 가 count 개의 창 ID 배열을 돌려줬다. 복사한 뒤 XFree 로 돌려준다.
        let v = unsafe { std::slice::from_raw_parts(children, count as usize).to_vec() };
        // SAFETY: XQueryTree 가 할당한 배열이며 이후 쓰지 않는다.
        unsafe { (x.XFree)(children.cast()) };
        v
    } else {
        Vec::new()
    };
    let attributes = |win: xlib::Window| {
        // SAFETY: XWindowAttributes 는 0 으로 초기화할 수 있는 C 구조체다.
        let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
        // SAFETY: dpy 는 열린 연결이고 win 은 같은 디스플레이의 창 ID 다.
        let ok = unsafe { (x.XGetWindowAttributes)(dpy, win, &mut attrs) } != 0;
        ok.then_some(attrs)
    };
    // 화면 밖으로 나간 부분을 XGetImage 로 읽으면 BadMatch 로 프로세스가 끝난다. 먼저 잰다.
    let read = |win: xlib::Window, w: i32, h: i32| -> Result<(Vec<u32>, (i32, i32)), String> {
        let (mut ax, mut ay, mut through) = (0, 0, 0);
        // SAFETY: dpy 는 열린 연결이고 win·root 는 같은 디스플레이의 창이다. 출력은 지역 변수다.
        unsafe { (x.XTranslateCoordinates)(dpy, win, root, 0, 0, &mut ax, &mut ay, &mut through) };
        let screen = attributes(root).ok_or("root attributes")?;
        if ax < 0 || ay < 0 || ax + w > screen.width || ay + h > screen.height {
            return Err(format!(
                "창 {w}x{h}+{ax}+{ay} 이 화면 {}x{} 밖으로 나간다. Xvfb 화면을 키운다",
                screen.width, screen.height
            ));
        }
        // SAFETY: 화면 안에 있는 창의 전체 영역을 ZPixmap 으로 읽는다. 실패하면 null 이다.
        let image = unsafe { (x.XGetImage)(dpy, win, 0, 0, w as u32, h as u32, !0, xlib::ZPixmap) };
        if image.is_null() {
            return Err("XGetImage returned null".into());
        }
        let mut pixels = Vec::with_capacity((w * h) as usize);
        for py in 0..h {
            for px in 0..w {
                // SAFETY: image 는 w×h 이미지이고 좌표는 그 안이다.
                pixels.push((unsafe { (x.XGetPixel)(image, px, py) } & 0x00ff_ffff) as u32);
            }
        }
        // SAFETY: XGetImage 가 만든 이미지이며 이후 쓰지 않는다.
        unsafe { (x.XDestroyImage)(image) };
        Ok((pixels, (ax, ay)))
    };
    let result = (|| {
        let Some((kid, attrs)) = kids.iter().find_map(|&kid| {
            attributes(kid)
                .filter(|a| a.map_state == xlib::IsViewable && a.width >= 8 && a.height >= 8)
                .map(|a| (kid, a))
        }) else {
            return Ok(None);
        };
        let (child_pixels, child_root) = read(kid, attrs.width, attrs.height)?;
        Ok(Some(X11WebViewCapture {
            child: (attrs.x, attrs.y, attrs.width, attrs.height),
            child_root,
            child_xid: kid,
            child_pixels,
        }))
    })();
    // SAFETY: 열린 연결을 닫고 이후 dpy 를 쓰지 않는다.
    unsafe { (x.XCloseDisplay)(dpy) };
    result
}

/// GTK 배율이 2 인 X11 세션(`GDK_SCALE=2` 와 winit 배율 2 로 만든다. GNOME 의 XSETTINGS 배율
/// 경로는 재지 않는다)에서 html surface 의 페이지 viewport 가 native WebView 창과 같은
/// 크기인지 잰다. 페이지는 viewport 전체에 고정한 3 CSS px 파란 테두리를 그리므로, viewport 가
/// 창과 같으면 창의 네 변 모두에 테두리 색이 보인다. host 가 GTK 크기·allocation 에 물리 px 를
/// 그대로 주면 viewport 가 창의 두 배로 잡혀 오른쪽·아래 변에 페이지 바탕색이 나오고 실패한다.
/// 실행: 창이 배율 2 로 2560x1440 이므로 화면이 그보다 큰 격리 Xvfb(예:
/// `Xvfb :<n> -screen 0 2600x1600x24`)와 번들 plugin 준비 뒤
/// `TASTY_E2E_DISPLAY=:<n> cargo test --locked --test e2e_tests -- --ignored --exact webview_page_viewport_fills_its_native_window_under_gtk_scale_two`.
#[cfg(all(target_os = "linux", feature = "gui"))]
#[test]
#[ignore = "2560x1440 보다 큰 Linux X11 디스플레이와 번들 html plugin 이 필요해 기본 실행·CI 에서 돌리지 않는다"]
fn webview_page_viewport_fills_its_native_window_under_gtk_scale_two() {
    const BLUE: u32 = 0x0000ff;
    let display = x11_harness_display()
        .expect("X11 디스플레이가 필요하다(inherit + Wayland 는 측정하지 않는다)");
    let tasty =
        TastyInstance::spawn_with_env(&[("GDK_SCALE", "2"), ("WINIT_X11_SCALE_FACTOR", "2")]);
    let page = tasty.tasty_home().join("webview-viewport-marker.html");
    std::fs::write(
        &page,
        "<!doctype html><html><head><style>\
         html,body{margin:0;height:100%;background:#00ff00;overflow:hidden}\
         #f{position:fixed;left:0;top:0;right:0;bottom:0;border:3px solid #0000ff;box-sizing:border-box}\
         </style></head><body><div id=f></div></body></html>",
    )
    .expect("marker page");
    let sid = tasty.first_surface_id();
    let split = tasty.call(
        "split",
        json!({
            "level": "pane",
            "target_surface": sid.to_string(),
            "direction": "vertical",
            "type": "html",
            "url": format!("file://{}", page.display()),
        }),
    );
    assert!(
        split["new_surface_id"].as_u64().is_some(),
        "html split: {split}"
    );
    let window = tasty
        .call("window.list", json!({}))
        .as_array()
        .and_then(|ws| ws.first().and_then(|w| w["id"].as_u64()))
        .expect("window.list 에 창이 있어야 한다");

    // 페이지가 그려질 때까지 왼쪽 변의 파랑을 기다린다. 왼쪽 변은 배율과 무관하게 창 안에 있다.
    let start = std::time::Instant::now();
    let edges = loop {
        match x11_capture_webview(&display, window).map(|c| c.map(|c| c.child_edges())) {
            Ok(Some(e)) if e[0] == BLUE => break e,
            Ok(_) if start.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(250));
            }
            Ok(other) => panic!("30 초 안에 WebView 창에 페이지가 그려지지 않았다: {other:?}"),
            Err(e) => panic!("WebView 창을 읽지 못했다: {e}"),
        }
    };
    let names = ["left", "right", "top", "bottom"];
    let missing: Vec<_> = names
        .iter()
        .zip(edges)
        .filter(|(_, c)| *c != BLUE)
        .map(|(n, c)| format!("{n}=#{c:06x}"))
        .collect();
    assert!(
        missing.is_empty(),
        "페이지 viewport 의 테두리가 WebView 창의 변에 없다({}). viewport 가 창보다 크거나 작다",
        missing.join(", ")
    );
}

/// `window` 의 입력 영역(X input shape)을 사각형 목록으로 읽는다. shape 가 없으면 창 전체다.
/// x11-dl 에 SHAPE 확장이 없어 libXext 를 실행 시점에 연다(빌드에 개발 패키지를 요구하지 않는다).
#[cfg(all(target_os = "linux", feature = "gui"))]
fn x11_input_region(display: &str, window: u64) -> Result<Vec<(i32, i32, i32, i32)>, String> {
    type GetRectangles = unsafe extern "C" fn(
        *mut x11_dl::xlib::Display,
        x11_dl::xlib::Window,
        std::os::raw::c_int,
        *mut std::os::raw::c_int,
        *mut std::os::raw::c_int,
    ) -> *mut x11_dl::xlib::XRectangle;
    const SHAPE_INPUT: std::os::raw::c_int = 2;
    // SAFETY: NUL 종단 이름으로 공유 라이브러리를 연다. 실패하면 null 이다. 핸들은 닫지 않는다
    // (시험 프로세스 수명 동안 같은 라이브러리를 여러 번 연다 — 참조 수만 늘어난다).
    let lib = unsafe { libc::dlopen(c"libXext.so.6".as_ptr(), libc::RTLD_NOW) };
    if lib.is_null() {
        return Err("dlopen(libXext.so.6) failed".into());
    }
    // SAFETY: 열린 라이브러리에서 NUL 종단 이름의 심볼을 찾는다.
    let sym = unsafe { libc::dlsym(lib, c"XShapeGetRectangles".as_ptr()) };
    if sym.is_null() {
        return Err("dlsym(XShapeGetRectangles) failed".into());
    }
    // SAFETY: libXext 의 XShapeGetRectangles 시그니처와 같은 함수 포인터 타입이다.
    let get_rectangles: GetRectangles = unsafe { std::mem::transmute(sym) };
    let x = x11_dl::xlib::Xlib::open().map_err(|e| format!("Xlib::open: {e}"))?;
    let cname = std::ffi::CString::new(display).map_err(|e| e.to_string())?;
    // SAFETY: cname 은 NUL 종단 문자열이며 반환된 포인터가 null 인지 확인한다.
    let dpy = unsafe { (x.XOpenDisplay)(cname.as_ptr()) };
    if dpy.is_null() {
        return Err(format!("XOpenDisplay({display}) failed"));
    }
    let (mut count, mut ordering) = (0, 0);
    // SAFETY: dpy 는 열린 연결이고 window 는 같은 디스플레이의 창이다. 출력은 지역 변수다.
    let rects = unsafe { get_rectangles(dpy, window as _, SHAPE_INPUT, &mut count, &mut ordering) };
    let out = if rects.is_null() {
        Vec::new()
    } else {
        // SAFETY: count 개의 XRectangle 배열이다. 복사한 뒤 XFree 로 돌려준다.
        let v = unsafe { std::slice::from_raw_parts(rects, count as usize) }
            .iter()
            .map(|r| (r.x as i32, r.y as i32, r.width as i32, r.height as i32))
            .collect();
        // SAFETY: XShapeGetRectangles 가 할당한 배열이며 이후 쓰지 않는다.
        unsafe { (x.XFree)(rects.cast()) };
        v
    };
    // SAFETY: 열린 연결을 닫고 이후 dpy 를 쓰지 않는다.
    unsafe { (x.XCloseDisplay)(dpy) };
    Ok(out)
}

#[cfg(all(target_os = "linux", feature = "gui"))]
fn region_contains(region: &[(i32, i32, i32, i32)], px: i32, py: i32) -> bool {
    region
        .iter()
        .any(|&(x, y, w, h)| px >= x && px < x + w && py >= y && py < y + h)
}

/// 창 관리자 없는 하네스 디스플레이에서 X 창 크기를 바로 바꾼다(물리 px).
#[cfg(all(target_os = "linux", feature = "gui"))]
fn x11_resize(display: &str, window: u64, width: u32, height: u32) -> Result<(), String> {
    let x = x11_dl::xlib::Xlib::open().map_err(|e| format!("Xlib::open: {e}"))?;
    let cname = std::ffi::CString::new(display).map_err(|e| e.to_string())?;
    // SAFETY: cname 은 NUL 종단 문자열이며 반환된 포인터가 null 인지 확인한다.
    let dpy = unsafe { (x.XOpenDisplay)(cname.as_ptr()) };
    if dpy.is_null() {
        return Err(format!("XOpenDisplay({display}) failed"));
    }
    // SAFETY: dpy 는 열린 연결이고 window 는 같은 디스플레이의 최상위 창이다.
    unsafe { (x.XResizeWindow)(dpy, window as _, width, height) };
    // SAFETY: 위와 같은 연결. 요청이 서버에 닿을 때까지 기다린다.
    unsafe { (x.XSync)(dpy, 0) };
    // SAFETY: 열린 연결을 닫고 이후 dpy 를 쓰지 않는다.
    unsafe { (x.XCloseDisplay)(dpy) };
    Ok(())
}

/// XTest 로 하네스 디스플레이의 포인터를 움직인다. 좌표는 화면(root) 기준이다.
#[cfg(all(target_os = "linux", feature = "gui"))]
enum Pointer {
    Move(i32, i32),
    Press,
    Release,
}

#[cfg(all(target_os = "linux", feature = "gui"))]
fn x11_pointer(display: &str, steps: &[Pointer]) -> Result<(), String> {
    let x = x11_dl::xlib::Xlib::open().map_err(|e| format!("Xlib::open: {e}"))?;
    let xtest = x11_dl::xtest::Xf86vmode::open().map_err(|e| format!("Xtst open: {e}"))?;
    let cname = std::ffi::CString::new(display).map_err(|e| e.to_string())?;
    // SAFETY: cname 은 NUL 종단 문자열이며 반환된 포인터가 null 인지 확인한다.
    let dpy = unsafe { (x.XOpenDisplay)(cname.as_ptr()) };
    if dpy.is_null() {
        return Err(format!("XOpenDisplay({display}) failed"));
    }
    for step in steps {
        match *step {
            // SAFETY: dpy 는 열린 연결이다. 화면 0 기준 좌표로 포인터를 옮긴다.
            Pointer::Move(px, py) => unsafe { (xtest.XTestFakeMotionEvent)(dpy, 0, px, py, 0) },
            // SAFETY: dpy 는 열린 연결이다. 버튼 1 을 누른다.
            Pointer::Press => unsafe { (xtest.XTestFakeButtonEvent)(dpy, 1, 1, 0) },
            // SAFETY: dpy 는 열린 연결이다. 버튼 1 을 뗀다.
            Pointer::Release => unsafe { (xtest.XTestFakeButtonEvent)(dpy, 1, 0, 0) },
        };
        // SAFETY: dpy 는 열린 연결이다. 보낸 가짜 이벤트를 서버로 내보낸다.
        unsafe { (x.XFlush)(dpy) };
        std::thread::sleep(Duration::from_millis(150));
    }
    // SAFETY: 열린 연결을 닫고 이후 dpy 를 쓰지 않는다.
    unsafe { (x.XCloseDisplay)(dpy) };
    Ok(())
}

/// WebView 창이 surface 사각형(`debug.surface_rect`)과 물리 px 단위로 같은지. 반올림 차이만 둔다.
#[cfg(all(target_os = "linux", feature = "gui"))]
fn assert_webview_fills_surface(tasty: &TastyInstance, sid: u64, capture: &X11WebViewCapture) {
    let reply = tasty.call("debug.surface_rect", json!({ "surface_id": sid }));
    let r = &reply["rect"];
    let surface =
        ["x", "y", "width", "height"].map(|k| r[k].as_f64().expect("surface rect") as f32);
    let (cx, cy, cw, ch) = capture.child;
    let child = [cx, cy, cw, ch].map(|v| v as f32);
    assert!(
        surface.iter().zip(child).all(|(s, c)| (s - c).abs() < 1.0),
        "WebView 창 {:?} 이 surface 사각형 {surface:?} 과 다르다",
        capture.child
    );
}

/// 화면 좌표 (`px`, `py`)가 WebView 창 기준으로 입력 영역 안인지.
#[cfg(all(target_os = "linux", feature = "gui"))]
fn webview_takes_input_at(display: &str, capture: &X11WebViewCapture, px: i32, py: i32) -> bool {
    let region = x11_input_region(display, capture.child_xid).expect("input region");
    region_contains(&region, px, py)
}

/// html WebView 는 surface 를 꽉 채우고, 분할선 hit 띠와 창 가장자리 리사이즈 밴드만 입력에서
/// 빼서 host 에 남긴다(`state::webview_edges`). 분할 배치와 단일 pane 배치를 차례로 잰다.
/// - 두 배치 모두 WebView X 창 = `debug.surface_rect`.
/// - 분할: WebView 왼쪽 끝(분할선 hit 띠)과 오른쪽 8 px(창 리사이즈 밴드)은 입력 영역 밖이고 가운데는 안.
///   XTest 로 왼쪽 끝을 눌러 끌면 분할선이 움직이고, 가운데 링크를 누르면 페이지가 바뀐다.
/// - 단일 pane(왼쪽 pane 을 닫는다): 왼쪽 끝은 사이드바와 닿아 입력 영역 안, 오른쪽 밴드는 밖.
///
/// 창 리사이즈 자체는 창 관리자가 필요해 이 시험이 재지 않는다. 실행: 격리 Xvfb 와 번들 plugin(html)
/// 준비 뒤 `TASTY_E2E_DISPLAY=:<n> cargo test --locked --test e2e_tests -- --ignored --exact webview_fills_its_surface_and_leaves_host_input_bands_to_the_host`.
#[cfg(all(target_os = "linux", feature = "gui"))]
#[test]
#[ignore = "Linux X11 디스플레이와 번들 html plugin 이 필요하고 포인터를 움직여 기본 실행·CI 에서 돌리지 않는다"]
fn webview_fills_its_surface_and_leaves_host_input_bands_to_the_host() {
    const LINK: u32 = 0x808000;
    const LINK_VISITED: u32 = 0x008080;
    let display = x11_harness_display()
        .expect("X11 디스플레이가 필요하다(inherit + Wayland 는 측정하지 않는다)");
    let tasty = TastyInstance::spawn_with_env(&[]);
    let page = tasty.tasty_home().join("webview-input.html");
    std::fs::write(
        &page,
        "<!doctype html><html><head><style>html,body{margin:0;height:100%;background:#000080}\
         #a{display:block;position:fixed;left:30%;top:30%;width:40%;height:40%;background:#808000}\
         #a:target{background:#008080}</style></head><body><a id=a href=\"#a\"></a></body></html>",
    )
    .expect("page");
    let first_pane = tasty.first_pane_id();
    let sid = tasty.first_surface_id();
    let split = tasty.call(
        "split",
        json!({
            "level": "pane",
            "target_surface": sid.to_string(),
            "direction": "vertical",
            "type": "html",
            "url": format!("file://{}", page.display()),
        }),
    );
    let html = split["new_surface_id"].as_u64().expect("html split");
    let window = tasty
        .call("window.list", json!({}))
        .as_array()
        .and_then(|ws| ws.first().and_then(|w| w["id"].as_u64()))
        .expect("window.list 에 창이 있어야 한다");
    let wait_for = |what: &str, pred: &dyn Fn(&X11WebViewCapture) -> bool| {
        let start = std::time::Instant::now();
        loop {
            match x11_capture_webview(&display, window) {
                Ok(Some(c)) if pred(&c) => break c,
                Ok(_) if start.elapsed() < Duration::from_secs(30) => {
                    std::thread::sleep(Duration::from_millis(250));
                }
                Ok(c) => panic!("30 초 안에 {what}: {:?}", c.map(|c| c.child)),
                Err(e) => panic!("WebView 창을 읽지 못했다: {e}"),
            }
        }
    };

    // 분할 배치
    wait_for("페이지가 그려지지 않았다", &|c| {
        let (_, _, w, h) = c.child;
        c.child_at(w / 2, h / 2) == LINK
    });
    std::thread::sleep(Duration::from_millis(500));
    let c = x11_capture_webview(&display, window).unwrap().unwrap();
    assert_webview_fills_surface(&tasty, html, &c);
    let (cx, _, w, h) = c.child;
    assert!(
        !webview_takes_input_at(&display, &c, 1, h / 2),
        "분할선 hit 띠가 WebView 입력에 남았다"
    );
    assert!(
        !webview_takes_input_at(&display, &c, w - 2, h / 2),
        "창 오른쪽 리사이즈 밴드가 WebView 입력에 남았다"
    );
    assert!(
        !webview_takes_input_at(&display, &c, w - 8, h / 2),
        "창 오른쪽 리사이즈 밴드 안쪽 끝이 WebView 입력에 남았다"
    );
    assert!(
        webview_takes_input_at(&display, &c, w / 2, h / 2),
        "WebView 가운데가 입력 영역 밖이다"
    );

    // 띠 밖 클릭은 페이지로 간다.
    let (rx, ry) = c.child_root;
    x11_pointer(
        &display,
        &[
            Pointer::Move(rx + w / 2, ry + h / 2),
            Pointer::Press,
            Pointer::Release,
        ],
    )
    .expect("click");
    wait_for("페이지 클릭이 페이지에 닿지 않았다", &|c| {
        let (_, _, w, h) = c.child;
        c.child_at(w / 2, h / 2) == LINK_VISITED
    });

    // WebView 위 분할선 hit 띠를 끌면 분할선이 움직인다.
    let mut drag = vec![Pointer::Move(rx + 1, ry + h / 2), Pointer::Press];
    drag.extend((1..=5).map(|i| Pointer::Move(rx + 1 - i * 20, ry + h / 2)));
    drag.push(Pointer::Release);
    x11_pointer(&display, &drag).expect("drag");
    wait_for(
        "WebView 위 분할선 드래그가 분할선을 옮기지 않았다",
        &|c| c.child.0 < cx - 50,
    );
    std::thread::sleep(Duration::from_millis(500));
    assert_webview_fills_surface(
        &tasty,
        html,
        &x11_capture_webview(&display, window).unwrap().unwrap(),
    );

    // 단일 pane 배치
    tasty.call("pane.close", json!({ "pane_id": first_pane }));
    wait_for(
        "왼쪽 pane 을 닫은 뒤 WebView 가 넓어지지 않았다",
        &|c| c.child.0 < cx - 200,
    );
    std::thread::sleep(Duration::from_millis(500));
    let c = x11_capture_webview(&display, window).unwrap().unwrap();
    assert_webview_fills_surface(&tasty, html, &c);
    let (_, _, w, h) = c.child;
    assert!(
        webview_takes_input_at(&display, &c, 1, h / 2),
        "분할선이 없는 왼쪽 끝이 입력 영역 밖이다"
    );
    assert!(
        !webview_takes_input_at(&display, &c, w - 2, h / 2),
        "창 오른쪽 리사이즈 밴드가 WebView 입력에 남았다"
    );
}

/// 배율 2 에서 html pane 바로 아래의 가로 pane 분할선을 WebView 쪽 hit 띠에서 첫 press 로 잡는다.
/// 배치는 터미널 / html / 터미널 세로 3단이고 html 은 포커스가 없다.
/// - WebView 창 = surface 사각형, 입력 영역 아래쪽 7 행(host 판정 `|y - d| < 8` 중 WebView 안쪽)만
///   구멍이고 그 위 행은 페이지가 받는다.
/// - 분할선 위 6 행을 눌러 아래로 끌면 html 높이가 늘고, 포커스된 surface 는 바뀌지 않는다
///   (분할선 hit 판정이 click-to-activate 보다 먼저다).
///
/// 실행: 2600x1600 이상 Xvfb 와 번들 plugin(html) 준비 뒤
/// `TASTY_E2E_DISPLAY=:<n> cargo test --locked --test e2e_tests -- --ignored --exact webview_lets_the_first_press_drag_the_pane_divider_below_it_at_scale_two`.
#[cfg(all(target_os = "linux", feature = "gui"))]
#[test]
#[ignore = "Linux X11 디스플레이와 번들 html plugin 이 필요하고 포인터를 움직여 기본 실행·CI 에서 돌리지 않는다"]
fn webview_lets_the_first_press_drag_the_pane_divider_below_it_at_scale_two() {
    let display = x11_harness_display()
        .expect("X11 디스플레이가 필요하다(inherit + Wayland 는 측정하지 않는다)");
    let tasty =
        TastyInstance::spawn_with_env(&[("GDK_SCALE", "2"), ("WINIT_X11_SCALE_FACTOR", "2")]);
    let page = tasty.tasty_home().join("webview-stack.html");
    std::fs::write(
        &page,
        "<!doctype html><html><head><style>html,body{margin:0;height:100%;background:#808000}\
         </style></head><body></body></html>",
    )
    .expect("page");
    let top = tasty.first_surface_id();
    let split = tasty.call(
        "split",
        json!({
            "level": "pane",
            "target_surface": top.to_string(),
            "direction": "horizontal",
            "type": "html",
            "url": format!("file://{}", page.display()),
        }),
    );
    let html = split["new_surface_id"].as_u64().expect("html split");
    let below = tasty.call(
        "split",
        json!({ "level": "pane", "target_surface": html.to_string(), "direction": "horizontal" }),
    );
    assert!(
        below["new_surface_id"].as_u64().is_some(),
        "terminal split: {below}"
    );
    let window = tasty
        .call("window.list", json!({}))
        .as_array()
        .and_then(|ws| ws.first().and_then(|w| w["id"].as_u64()))
        .expect("window.list 에 창이 있어야 한다");
    let wait_for = |what: &str, pred: &dyn Fn(&X11WebViewCapture) -> bool| {
        let start = std::time::Instant::now();
        loop {
            match x11_capture_webview(&display, window) {
                Ok(Some(c)) if pred(&c) => break c,
                Ok(_) if start.elapsed() < Duration::from_secs(30) => {
                    std::thread::sleep(Duration::from_millis(250));
                }
                Ok(c) => panic!("30 초 안에 {what}: {:?}", c.map(|c| c.child)),
                Err(e) => panic!("WebView 창을 읽지 못했다: {e}"),
            }
        }
    };
    wait_for("페이지가 그려지지 않았다", &|c| {
        let (_, _, w, h) = c.child;
        c.child_at(w / 2, h / 2) == 0x808000
    });
    // 리뷰에서 결함을 재현한 창 크기다. 2560x1440 에서는 egui 가 press 를 잡지 않아 드러나지 않았다.
    x11_resize(&display, window, 1600, 1000).expect("resize");
    wait_for(
        "창 크기를 바꾼 뒤 WebView 가 따라오지 않았다",
        &|c| c.child.2 < 1300,
    );
    std::thread::sleep(Duration::from_millis(800));
    let c = x11_capture_webview(&display, window).unwrap().unwrap();
    assert_webview_fills_surface(&tasty, html, &c);
    let (_, _, w, h) = c.child;
    for row in h - 7..h {
        assert!(
            !webview_takes_input_at(&display, &c, w / 2, row),
            "분할선 hit 띠 행 {row}(높이 {h})가 WebView 입력에 남았다"
        );
    }
    assert!(
        webview_takes_input_at(&display, &c, w / 2, h - 8),
        "host 가 분할선으로 보지 않는 행 {}(높이 {h})이 WebView 입력에서 빠졌다",
        h - 8
    );

    let focused =
        |t: &TastyInstance| t.call("debug.focused_surface", json!({}))["surface_id"].clone();
    let before = focused(&tasty);
    assert_ne!(
        before,
        json!(html),
        "시험 전제: html 은 포커스가 없어야 한다"
    );
    let (rx, ry) = c.child_root;
    let press_y = ry + h - 6;
    let mut drag = vec![Pointer::Move(rx + w / 2, press_y), Pointer::Press];
    drag.extend((1..=4).map(|i| Pointer::Move(rx + w / 2, press_y + i * 10)));
    drag.push(Pointer::Release);
    x11_pointer(&display, &drag).expect("drag");
    wait_for(
        "WebView 아래 분할선 hit 띠의 첫 press 드래그가 분할선을 옮기지 않았다",
        &|c| c.child.3 > h + 20,
    );
    assert_eq!(
        focused(&tasty),
        before,
        "분할선 hit 띠의 press 가 포커스를 바꿨다"
    );
}

const ROUNDTRIP_INTS: [i64; 3] = [i64::MIN, i64::MAX, 9_007_199_254_740_993];

/// v2 int64 wire 값(10진 문자열)을 읽는다. 정수 토큰도 받아, 문자열화가 빠졌을 때
/// 값이 바뀌어서 실패하도록 한다(형식만 달라서 실패하지 않게).
fn wire_int64s(v: &serde_json::Value) -> Vec<Option<i64>> {
    v.as_array()
        .unwrap_or_else(|| panic!("not an array: {v}"))
        .iter()
        .map(|x| match x {
            serde_json::Value::String(s) => s.parse().ok(),
            serde_json::Value::Number(n) => n.as_i64(),
            _ => None,
        })
        .collect()
}

/// JavaScript 소비자처럼 숫자를 f64 로 읽고 다시 쓴다. node 가 있으면 `JSON.parse` →
/// `JSON.stringify` 를 그대로 거치고, 없으면 같은 IEEE double 변환을 Rust 로 흉내 낸다.
fn through_javascript(v: &serde_json::Value) -> serde_json::Value {
    use std::io::Write;
    let text = serde_json::to_string(v).expect("serialize");
    let spawned = std::process::Command::new("node")
        .args([
            "-e",
            "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>process.stdout.write(JSON.stringify(JSON.parse(s))))",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn();
    match spawned {
        Ok(mut child) => {
            // node 가 입력을 읽기 전에 실패로 끝나면 쓰기가 파이프 끊김으로 끝난다. 그때는 아래
            // 종료 상태 검사가 실패 이유를 보여 준다(node 의 stderr 는 시험 출력에 그대로 나온다).
            let written = child
                .stdin
                .take()
                .expect("stdin")
                .write_all(text.as_bytes());
            if let Err(e) = written {
                assert_eq!(
                    e.kind(),
                    std::io::ErrorKind::BrokenPipe,
                    "write node stdin: {e}"
                );
            }
            let out = child.wait_with_output().expect("node");
            assert!(out.status.success(), "node failed: {}", out.status);
            serde_json::from_slice(&out.stdout).expect("node output is json")
        }
        Err(e) => {
            tracing::warn!("node unavailable ({e}); emulating JSON.parse number handling");
            fn as_double(v: &serde_json::Value) -> serde_json::Value {
                match v {
                    serde_json::Value::Number(n) => {
                        let f = n.as_f64().expect("finite");
                        // JSON.stringify 는 정수 모양의 double 을 소수점 없이 쓴다.
                        if f.fract() == 0.0 && f.abs() < 1e21 {
                            serde_json::from_str(&format!("{f:.0}")).expect("number")
                        } else {
                            serde_json::Value::from(f)
                        }
                    }
                    serde_json::Value::Array(a) => a.iter().map(as_double).collect(),
                    serde_json::Value::Object(o) => o
                        .iter()
                        .map(|(k, v)| (k.clone(), as_double(v)))
                        .collect::<serde_json::Map<_, _>>()
                        .into(),
                    other => other.clone(),
                }
            }
            as_double(v)
        }
    }
}

/// CLI 출력에서 `<label>: ` 로 시작하는 블록(다음 최상위 항목 전까지)을 JSON 으로 읽는다.
fn cli_json_block(stdout: &str, label: &str) -> serde_json::Value {
    let marker = format!("\n{label}: ");
    let start = stdout
        .find(&marker)
        .unwrap_or_else(|| panic!("no {label} block:\n{stdout}"));
    let rest = &stdout[start + marker.len()..];
    let end = rest
        .lines()
        .scan(0usize, |offset, line| {
            let at = *offset;
            *offset += line.len() + 1;
            Some((at, line))
        })
        .skip(1)
        .find(|(_, line)| !line.starts_with(' ') && !line.starts_with('}'))
        .map(|(at, _)| at)
        .unwrap_or(rest.len());
    serde_json::from_str(rest[..end].trim()).unwrap_or_else(|e| panic!("{e}:\n{stdout}"))
}

/// v2 task 의 int64 출력(최소·최대·2^53+1)이 IPC 응답과 CLI 출력에서 JavaScript 의
/// `JSON.parse` 를 지나도 같은 정수로 복원되는지 본다. v2 를 IPC 로 만드는 경로가 아직
/// 없어 레코드는 memory 로 심고, 결과 확정은 실제 `agent.task_set_result` 경로를 쓴다.
/// 같은 v2 task 를 v1 출력 placeholder 나 v1 reduce 입력으로 쓰는 요청은 거절돼야 한다.
#[test]
fn typed_int64_outputs_survive_javascript_through_ipc_and_cli() {
    let _lane = lane();
    let tasty = common::shared();
    let ws = tasty.create_workspace("int64-wire");
    let id = "t-int64-wire";
    tasty.call(
        "memory.put",
        json!({
            "scope": format!("workspace:{}", ws.id),
            "key": format!("tasty.agent.typed_task.{id}"),
            "value": {"record_format": "tasty.task/v2", "task": {
                "id": id, "workspace_id": ws.id, "name": "int64-wire",
                "command": {"kind": "custom", "ipc_method": "system.ping"},
                "depends_on": [], "state": {"kind": "running"},
                "created_at": 0, "started_at": 0,
                "on_failure": {"kind": "abort"}, "metadata": null,
                "reserved_for_fallback": false,
                "contract": {"contract_version": 2,
                    "output_schema": {"type": "list", "items": {"type": "int64"}}}
            }}
        }),
    );
    // 제출은 JSON 정수 토큰으로 한다. 확정된 wire 값은 문자열이어야 한다.
    let settled = tasty.call(
        "agent.task_set_result",
        json!({"workspace_id": ws.id, "id": id, "state": "succeeded",
               "output": ROUNDTRIP_INTS}),
    );
    assert_eq!(
        settled["task"]["state"]["kind"],
        json!("succeeded"),
        "{settled}"
    );
    let expected: Vec<Option<i64>> = ROUNDTRIP_INTS.iter().copied().map(Some).collect();

    let fetched = tasty.call("agent.task_get", json!({"workspace_id": ws.id, "id": id}));
    let output = &fetched["typed_result"]["output"];
    assert_eq!(wire_int64s(output), expected, "{fetched}");
    let via_js = through_javascript(&fetched);
    assert_eq!(wire_int64s(&via_js["typed_result"]["output"]), expected);
    assert_eq!(wire_int64s(&via_js["result"]["output"]), expected);

    // CLI 전용 TASTY_HOME 에 하네스 인스턴스의 port 를 적어 같은 인스턴스에 붙인다.
    // 인스턴스를 새로 띄우지 않으므로 부팅용 port 파일 인자는 쓰지 않는다.
    let cli_home = tempfile::tempdir().expect("cli home");
    std::fs::write(cli_home.path().join("tasty.port"), tasty.port().to_string())
        .expect("write port file");
    let out = std::process::Command::new(common::spawn_diag::instance_bin())
        .env("TASTY_HOME", cli_home.path())
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        .args([
            "agent",
            "task-get",
            "--workspace-id",
            &ws.id.to_string(),
            "--id",
            id,
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run tasty cli");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "cli failed: {}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let cli_result = cli_json_block(&stdout, "result");
    assert_eq!(wire_int64s(&cli_result["output"]), expected);
    assert_eq!(
        wire_int64s(&through_javascript(&cli_result)["output"]),
        expected
    );

    // v1 출력 placeholder 는 v2 결과를 읽지 못한다.
    let rejected = tasty.call_raw(
        "agent.task_create",
        json!({
            "workspace_id": ws.id,
            "name": "v1-reads-v2",
            "command": {"kind": "custom", "ipc_method": "system.ping",
                        "params": {"x": format!("${{task.{id}.output}}")}},
            "depends_on": [id],
        }),
    );
    assert_eq!(rejected["error"]["code"], json!(-32602), "{rejected}");
    assert_eq!(
        rejected["error"]["data"]["task_id"],
        json!(id),
        "{rejected}"
    );

    // v1 reduce 도 v2 결과를 입력으로 받지 않는다. 생성과 단발 reduce 모두 거절한다.
    let reduce_rejected = tasty.call_raw(
        "agent.task_create",
        json!({
            "workspace_id": ws.id,
            "name": "v1-reduce-of-v2",
            "command": {"kind": "reduce", "inputs": [id], "strategy": {"kind": "all"}},
        }),
    );
    assert_eq!(
        reduce_rejected["error"]["code"],
        json!(-32602),
        "{reduce_rejected}"
    );
    assert_eq!(
        reduce_rejected["error"]["data"]["task_id"],
        json!(id),
        "{reduce_rejected}"
    );
    let one_shot = tasty.call_raw(
        "agent.task_reduce",
        json!({"workspace_id": ws.id, "inputs": [id], "strategy": {"kind": "all"}}),
    );
    assert_eq!(one_shot["error"]["code"], json!(-32602), "{one_shot}");
    assert_eq!(
        one_shot["error"]["data"]["task_id"],
        json!(id),
        "{one_shot}"
    );

    tasty.call_raw(
        "agent.task_delete",
        json!({"workspace_id": ws.id, "id": id, "force": true}),
    );
    tasty.call("workspace.close", json!({"id": ws.id}));
}

/// 작업 상태가 종결될 때까지 기다린다.
#[cfg(unix)]
fn await_task_state(tasty: &TastyInstance, ws: u64, id: &str) -> serde_json::Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let t = tasty.call("agent.task_get", json!({"workspace_id": ws, "id": id}));
        let kind = t["state"]["kind"].as_str().unwrap_or_default().to_string();
        if ["succeeded", "failed", "cancelled", "skipped"].contains(&kind.as_str()) {
            return t;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "task {id} did not finish: {t}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
fn task_count(tasty: &TastyInstance, ws: u64) -> usize {
    tasty.call("agent.task_list", json!({"workspace_id": ws}))["tasks"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or_default()
}

#[cfg(unix)]
fn graph_run(ws: u64, argv: serde_json::Value) -> serde_json::Value {
    json!({"kind": "run", "workspace_id": ws, "command": argv})
}

/// 정수를 내는 producer, argv 로 받는 소비자, stdin 으로 받는 소비자로 된 그래프.
#[cfg(unix)]
fn typed_graph(
    ws: u64,
    args_log: &std::path::Path,
    stdin_log: &std::path::Path,
    label_binding: serde_json::Value,
) -> serde_json::Value {
    let args_script = format!("printf '%s\\n' \"$1\" >> '{}'", args_log.display());
    let stdin_script = format!("cat >> '{}'", stdin_log.display());
    json!({"contract_version": 2, "tasks": [
        {"id": "use.args",
         "command": graph_run(ws, json!(["sh", "-c", args_script, "sh"])),
         "input_schema": {"type": "object", "fields": {"n": {"type": "int64"}}},
         "bindings": {"n": {"from_task": "count.src"}},
         "input_mapping": {"args": ["/n"]}},
        {"id": "use.stdin",
         "command": graph_run(ws, json!(["sh", "-c", stdin_script])),
         "input_schema": {"type": "object", "fields": {
             "label": {"type": "string"}, "n": {"type": "int64"}}},
         "bindings": {"label": label_binding, "n": {"literal": "9007199254740993"}},
         "input_mapping": {"stdin": true}},
        {"id": "count.src", "command": graph_run(ws, json!(["sh", "-c", "exit 3"])),
         "allowed_exit_codes": [0, 3]}
    ]})
}

/// 격리 홈에서 CLI 로 그래프를 제출한다. stdout 과 stderr 를 합쳐 돌려준다.
#[cfg(unix)]
fn cli_graph_submit(
    tasty: &TastyInstance,
    ws: u64,
    graph: &serde_json::Value,
    dry_run: bool,
) -> (bool, String) {
    let cli_home = tempfile::tempdir().expect("cli home");
    std::fs::write(cli_home.path().join("tasty.port"), tasty.port().to_string())
        .expect("write port file");
    let mut cmd = std::process::Command::new(common::spawn_diag::instance_bin());
    cmd.env("TASTY_HOME", cli_home.path())
        .env_remove("TASTY_SURFACE_ID")
        .env_remove("TASTY_SESSION_TOKEN")
        .args(["agent", "task-graph-submit", "--workspace-id"])
        .arg(ws.to_string())
        .arg("--graph")
        .arg(graph.to_string())
        .stdin(std::process::Stdio::null());
    if dry_run {
        cmd.arg("--dry-run");
    }
    let out = cmd.output().expect("run tasty cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// 잘못된 그래프는 실행 중인 러너가 있어도 아무것도 남기지 않고, 오류에 위치가 실린다.
#[cfg(unix)]
fn refused_graphs_leave_nothing(tasty: &TastyInstance, ws: u64, bad: &serde_json::Value) {
    let refused = tasty.call_raw(
        "agent.task_graph_submit",
        json!({"workspace_id": ws, "graph": bad}),
    );
    assert_eq!(refused["error"]["code"], json!(-32602), "{refused}");
    let data = &refused["error"]["data"];
    assert_eq!(
        data["location"],
        json!("/tasks/1/bindings/label"),
        "{refused}"
    );
    assert_eq!(data["task_id"], json!("use.stdin"));
    assert_eq!(data["type_error"]["expected"], json!("string"));
    assert_eq!(data["type_error"]["actual"], json!("int64"));
    let cycle = tasty.call_raw(
        "agent.task_graph_submit",
        json!({"workspace_id": ws, "graph": {"contract_version": 2, "tasks": [
            {"id": "a", "command": graph_run(ws, json!(["true"])), "depends_on": ["b"]},
            {"id": "b", "command": graph_run(ws, json!(["true"])), "depends_on": ["a"]}]}}),
    );
    assert_eq!(
        cycle["error"]["data"]["location"],
        json!("/tasks"),
        "{cycle}"
    );
    let (ok, text) = cli_graph_submit(tasty, ws, bad, true);
    assert!(!ok, "{text}");
    assert!(text.contains("/tasks/1/bindings/label"), "{text}");
    assert_eq!(
        task_count(tasty, ws),
        0,
        "a refused graph leaves no task behind"
    );
}

/// 정지한 러너: 그래프는 저장·활성화되지만 러너를 시작하기 전에는 실행되지 않는다.
#[cfg(unix)]
fn stopped_runner_holds_an_active_graph(tasty: &TastyInstance, ws: u64, dir: &std::path::Path) {
    tasty.call(
        "agent.task_run",
        json!({"workspace_id": ws, "action": "stop"}),
    );
    let later_log = dir.join("later.log");
    let script = format!("echo once >> '{}'", later_log.display());
    tasty.call(
        "agent.task_graph_submit",
        json!({"workspace_id": ws, "graph": {"contract_version": 2, "tasks": [
            {"id": "later", "command": graph_run(ws, json!(["sh", "-c", script]))}]}}),
    );
    std::thread::sleep(Duration::from_millis(500));
    let later = tasty.call("agent.task_get", json!({"workspace_id": ws, "id": "later"}));
    assert_eq!(later["state"]["kind"], json!("ready"), "{later}");
    assert!(!later_log.exists());
    tasty.call(
        "agent.task_run",
        json!({"workspace_id": ws, "action": "start"}),
    );
    let later = await_task_state(tasty, ws, "later");
    assert_eq!(later["state"]["kind"], json!("succeeded"), "{later}");
    assert_eq!(std::fs::read_to_string(&later_log).unwrap(), "once\n");
}

/// v2 그래프 제출의 IPC·CLI 경계. 실행 중인 러너에 잘못된 그래프를 내도 task 가 하나도 남지
/// 않고, 오류에는 task 와 제출 정의 안의 위치가 실린다. 올바른 그래프는 활성화 뒤에만 실행되며
/// 입력은 값으로 한 번만 전달된다. 정지한 러너에서는 저장만 되고 러너를 시작해야 실행된다.
#[cfg(unix)]
#[test]
fn typed_task_graphs_submit_atomically_and_pass_inputs_by_value() {
    let _lane = lane();
    let tasty = common::shared();
    let ws = tasty.create_workspace("typed-graph").id;
    let out = tempfile::tempdir().expect("out dir");
    let args_log = out.path().join("args.log");
    let stdin_log = out.path().join("stdin.log");
    let started = tasty.call(
        "agent.task_run",
        json!({"workspace_id": ws, "action": "start"}),
    );
    assert!(started.is_object(), "{started}");

    let bad = typed_graph(ws, &args_log, &stdin_log, json!({"from_task": "count.src"}));
    refused_graphs_leave_nothing(tasty, ws, &bad);
    let good = typed_graph(
        ws,
        &args_log,
        &stdin_log,
        json!({"from_task": "count.src", "convert": "to_string"}),
    );
    let (ok, text) = cli_graph_submit(tasty, ws, &good, true);
    assert!(ok, "{text}");
    assert_eq!(task_count(tasty, ws), 0, "a dry run stores nothing");

    let submitted = tasty.call(
        "agent.task_graph_submit",
        json!({"workspace_id": ws, "graph": good}),
    );
    assert_eq!(submitted["activated"], json!(true), "{submitted}");
    assert_eq!(submitted["tasks"].as_array().map(|a| a.len()), Some(3));
    for id in ["use.args", "use.stdin"] {
        let t = await_task_state(tasty, ws, id);
        assert_eq!(t["state"]["kind"], json!("succeeded"), "{t}");
    }
    // 입력은 값으로 한 번만 전달된다. int64 는 정확하고 stdin 은 wire 형식이다.
    assert_eq!(std::fs::read_to_string(&args_log).unwrap(), "3\n");
    assert_eq!(
        std::fs::read_to_string(&stdin_log).unwrap(),
        r#"{"label":"3","n":"9007199254740993"}"#
    );
    let consumer = tasty.call(
        "agent.task_get",
        json!({"workspace_id": ws, "id": "use.args"}),
    );
    assert_eq!(consumer["input_snapshot"]["value"], json!({"n": "3"}));
    assert_eq!(
        consumer["input_snapshot"]["sources"][0]["from_task"],
        json!("count.src")
    );
    let edges = tasty.call("agent.task_graph", json!({"workspace_id": ws}));
    assert!(edges.to_string().contains("\"binding\""), "{edges}");

    stopped_runner_holds_an_active_graph(tasty, ws, out.path());
    tasty.call(
        "agent.task_run",
        json!({"workspace_id": ws, "action": "stop"}),
    );
    tasty.call("workspace.close", json!({"id": ws}));
}
