//! e2e 시나리오 — **시나리오 하나에 `#[test]` 하나**.
//!
//! 인스턴스는 여전히 test binary 당 1 개다(`common::shared()`, ADR-0090). 격리
//! 단위는 프로세스가 아니라 workspace 이므로 각 시나리오는 [`scenario`] 로 자기
//! workspace 를 잡고 그 안의 surface/pane 만 건드린다 — 그래서 전역 목록
//! (`pane.list` / `workspace.list` / `hook.list` / `pty.list` / notification) 위에서는
//! 길이 산술 대신 "내 것이 있는가/없는가" 로 판정한다(`tests/common/mod.rs` 의
//! `shared()` doc 경고).
//!
//! **왜 한 함수가 아닌가.** 전체를 한 `#[test]` 에 직렬로 두면 앞에서 하나가 죽는
//! 순간 뒤의 전부가 실행되지 않고, CI 는 그 하나를 위해 파일 전체를 `--skip` 하게
//! 된다 — 그러면 GUI 를 요구하지 않는 시나리오까지 같이 사라진다. 실제로 헤드리스
//! 조합에서 이 파일은 통째로 skip 되어 있었고, 벽은 마지막 시나리오의
//! `window.create` 하나뿐이었다. 지금은 창을 요구하는 단언이
//! [`multi_window_owner_routing`] 한 곳에 모여 있어 그것만 skip 하면 된다.

mod common;

use common::{TastyInstance, TestWorkspace};
use serde_json::json;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

/// 창을 만드는 시나리오와 나머지를 갈라 놓는 차선(lane) 잠금. 나머지는 read 로
/// 서로 병렬이고, 창을 만드는 하나만 write 로 단독이다.
///
/// **왜 생겼나 — 실측(기본 gui 조합, Xvfb).** `multi_window_owner_routing` 이 두
/// 번째 창을 만들면 그 창이 포커스를 가져가는데, **owner 를 params 에서 못 찾는
/// 메서드는 포커스된 창으로 라우팅된다**. (그 앞쪽 전제 — IPC 로 만든 창이 포커스를
/// 가져간다 — 는 ADR-0497 로 사라졌다: 에이전트 창은 focused 를 옮기지 않는다.) 그때 근거로 든 것은 `Kind` 가
/// surface / workspace / pane 셋뿐이라 `tab.close {tab_id}` 와 `pty.*` 의 headless
/// pty id 가 owner 를 못 찾고 `focused_view_id` 로 떨어진다는 것이었다(3 건 실패).
///
/// **그 근거 셋은 이제 하나도 살아 있지 않다(2026-09-06 재측정).**
/// `src/core/request_target.rs` 의 `Kind` 는 여덟이고(surface · workspace · pane ·
/// tab · headless · hook · observer · category), `params_resource_id` 가 `tab_id` 를,
/// `method_scoped_resource_id` 가 `pty.kill`/`read`/`wait`/`write` 의 `"id"` 를 푼다.
/// 지목한 대상이 어느 창에도 없을 때 모호성 오류가 참인 거절을 덮던 것도
/// `src/app/request_owner.rs` 에서 닫혔다.
///
/// **그래도 이 잠금을 지금 걷지 않는다** — 걷어도 되는지는 **안 재봤다.** 위
/// 기전 자체("owner 를 못 찾는 요청은 포커스로 간다")는 살아 있고, 그런 요청이
/// 이 파일에 더 없다는 것을 확인하지 않았다. 걷으려면 **부하 아래에서** 걷고
/// 돌려 봐야 한다 — 이 형태는 단독 실행에서 안 난다.
///
/// **★ 잠금은 동시성을 막지 잔여를 막지 않는다.** write 차선은 창 만드는
/// 시나리오가 남들과 *겹치지* 않게 할 뿐, 그것이 **남긴 창**은 뒤에 도는 read
/// 차선 전부가 본다. 그래서 그 시나리오는 자기가 만든 창을 스스로 닫는다.
/// 창 수를 읽는 단언을 새로 넣을 때 이 성질을 먼저 보라.
static WINDOW_EXCLUSIVE: RwLock<()> = RwLock::new(());

/// 창을 만들지 않는 시나리오의 차선. poison 은 무시한다 — 다른 테스트가 panic 한
/// 사실이 이 테스트의 판정을 바꾸지 않고, 바꾸면 실패 원인이 뒤바뀐다.
fn lane() -> RwLockReadGuard<'static, ()> {
    WINDOW_EXCLUSIVE.read().unwrap_or_else(|e| e.into_inner())
}

/// 창을 만드는 시나리오의 차선 — 나머지가 다 끝난 뒤 단독으로 돈다.
fn exclusive_lane() -> RwLockWriteGuard<'static, ()> {
    WINDOW_EXCLUSIVE.write().unwrap_or_else(|e| e.into_inner())
}

/// 시나리오 하나의 격리 단위를 잡는다 — 차선 + 공유 인스턴스 + 전용 workspace +
/// 그 workspace 의 surface/pane. 반환 순서는 `(instance, workspace, surface, pane, lane)`.
/// 마지막 값은 테스트가 끝날 때까지 살아 있어야 하므로 `_lane` 으로 받아 둔다.
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

// ========== Read-only queries ==========

#[test]
fn read_only_queries() {
    let (tasty, _ws, sid, pid, _lane) = scenario("e2e-read-only");

    // system.info
    let info = tasty.call("system.info", json!({}));
    assert_eq!(
        info.get("version").and_then(|v| v.as_str()),
        Some(env!("CARGO_PKG_VERSION")),
    );
    assert!(info["workspace_count"].as_u64().unwrap() >= 1);

    // tree
    let tree = tasty.call("tree", json!({}));
    let tree_arr = tree.as_array().unwrap();
    assert!(!tree_arr.is_empty());
    assert!(tree_arr[0].get("name").is_some());

    // ui.state
    let ui = tasty.call("ui.state", json!({}));
    assert_eq!(ui["settings_open_requested"], false);
    // 모달이 안 떠 있는 부팅 직후라 둘 다 "없음" 이다. `active_modal_kind` 를 함께 보는
    // 이유는 그 키가 **있는지**를 여기서만 재기 때문이다 — 키가 사라지면 `as_str()` 가
    // `None` 을 내고, 그것은 소비자 쪽에서 "모달이 없다" 와 같은 모양이 된다.
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

    // surface.list / pane.list — 전역 목록이므로 "내 것이 보이는가" 로 본다.
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

    // screen_text
    let text = tasty.screen_text_of(sid);
    assert!(!text.trim().is_empty());

    // cursor_position
    let cursor = tasty.call("surface.cursor_position", json!({"surface_id": sid}));
    assert!(cursor.get("x").is_some());
    assert!(cursor.get("y").is_some());

    // tab.list — pane 스코프라 그대로 안전하다.
    let tabs = tasty.call("tab.list", json!({"pane_id": pid}));
    assert!(!tabs["tabs"].as_array().unwrap().is_empty());
}

/// 셸이 OSC 7 로 알린 cwd 가 그 탭의 이름이 된다 — **두 조합에서 같다.** gui 는
/// `App::cascade_terminal_pty_cwd_changed` 가, 헤드리스는 PTY drain 의
/// `intent::headless::apply_terminal_cwd_changed` 가 한다(docs/dev-guide/headless-build-boundaries.md
/// "두 조합이 같게 하는 것"). 헤드리스에 그 배선이 없던 동안은 이름이 안 바뀌었다.
///
/// 하네스 셸은 `/bin/sh` 라 스스로 OSC 7 도 제목(OSC 0/2)도 안 쏜다 — 제목이 이름을 덮는 경우가
/// 없어 이 시험은 cwd 갈래만 잰다. `printf` 의 `\033` 해석이 셸마다 달라 Unix 로 한정한다
/// (`dim_sgr2_survives_to_the_renderer` 와 같은 이유).
#[cfg(not(windows))]
#[test]
fn an_osc7_cwd_becomes_the_tab_name() {
    // `tab.list` 는 표시 이름이 아니라 원본 `name` 을 싣는다 — 표시 이름(명시 이름 → OSC 제목 →
    // cwd → 원본)을 싣는 것은 `tree` 의 탭 행이다.
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
    // workspace.list 는 mirror(원격 attach client 인지) 를 함께 실어야 한다 — GUI
    // 사이드바만 알던 정보라 에이전트가 조작 전에 판별할 수단이 없었다. 로컬 인스턴스
    // 에는 mirror 워크스페이스가 없으므로 전부 false 다(true 케이스는 실제 attach 가
    // 필요해 두 인스턴스 실측으로 확인한다).
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

    // markdown.recent — 최근 markdown 목록 조회(읽기 전용, 주소창 드롭다운 공급원).
    // 격리 HOME 이라 초기 목록은 비어 있어도 무방 — 왕복 success + `recent` 배열 shape 검증.
    let recent = tasty.call("markdown.recent", json!({}));
    let recent_arr = recent["recent"]
        .as_array()
        .expect("markdown.recent returns { recent: [...] }");
    assert!(recent_arr.len() <= 10, "recent 은 최대 10개");
    // 조회가 사용자 상태(포커스 등)를 바꾸지 않았는지: 재조회가 여전히 성공.
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
    // 전역 목록이라 길이가 아니라 "내가 만든 것이 보이는가" 로 본다.
    let notifs = tasty.call("notification.list", json!({}));
    let rows = notifs.as_array().cloned().unwrap_or_default();
    assert!(!rows.is_empty(), "notification.list 가 비었다: {created:?}");
    assert!(
        rows.iter().any(|n| n["surface_id"].as_u64() == Some(sid)),
        "notification.list 가 내 surface={sid} 의 알림을 빠뜨림: {rows:?}"
    );
}

// ========== Terminal I/O ==========

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

    // mark_and_read
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

/// 출력 스캐너 커서(`surface.read_since_scan_mark`)가 에이전트의 mark 와 **다른**
/// 커서인지, 그리고 읽을 때 전진하는지를 실제 IPC 왕복으로 잰다.
///
/// 인파일 단위시험(`crates/tasty-terminal/src/output_buffer.rs`)이 같은 두 성질을 버퍼
/// 수준에서 재지만, 그것만으로는 **라우터 팔과 권한 표 등재가 살아 있는지** 알 수 없다 —
/// 이름이 등재되지 않았거나 팔이 없으면 요청은 `-32601` 로 돌아오고 버퍼는 그 사실을
/// 모른다. 여기서는 그 왕복을 지난다(ADR-0307).
///
/// 세 단계가 다 **순서에 의존한다.** 어느 단계든 출력과 커서 조작의 순서를 뒤집으면 잃을
/// 것이 없어져, 재려던 회귀에서도 통과한다. 각 단계의 ★ 주석이 그 순서를 적는다.
#[test]
fn terminal_scan_cursor_is_separate_from_the_agent_mark() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-scan-cursor");

    // 등재·라우팅 대조군 — 이름이 표에 없거나 팔이 없으면 여기서 죽는다. 아래 단정들은
    // "빈 문자열" 로도 초록이 될 수 있는 형태가 있으므로 이 줄이 먼저 서야 한다.
    let first = tasty.call(
        "surface.read_since_scan_mark",
        json!({"surface_id": sid, "strip_ansi": true}),
    );
    assert!(
        first.get("text").and_then(|v| v.as_str()).is_some(),
        "surface.read_since_scan_mark 가 text 를 안 줬다 — 등재나 라우터 팔이 없다: {first:?}"
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
    // 커서가 전진하므로 한 번에 다 오지 않을 수 있다 — 본 만큼 이어 붙여 판정한다.
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

    // 1. 커서가 전진한다 — 같은 구간이 두 번 오지 않는다.
    echo("scan_marker_one");
    wait_scan("scan_marker_one");
    assert!(
        !scan(true).contains("scan_marker_one"),
        "커서가 전진하지 않았다 — 같은 구간이 다시 왔다"
    );

    // 2. 에이전트의 set_mark 이 scan 커서를 밀지 않는다.
    //
    // ★ **순서가 판정을 만든다.** 출력을 먼저 내고 그 다음에 mark 를 세운다. 거꾸로 하면
    // `set_mark` 이 scan 커서를 버퍼 끝으로 밀어도 그 사이에 잃을 출력이 없어서, 그
    // 회귀에서도 이 단계가 통과한다 — 이 자리가 실제로 그 모양이었고 변이로 드러났다.
    echo("scan_marker_two");
    // 버퍼에 닿은 것은 **mark 를 안 움직이는 읽기**로 확인한다. scan 으로 확인하면 그
    // 읽기가 커서를 전진시켜 아래 판정이 재는 것이 없어진다.
    tasty.wait_for_output(sid, "scan_marker_two", Duration::from_secs(10));
    tasty.set_mark(sid);
    let seen = wait_scan("scan_marker_two");
    assert!(
        seen.contains("scan_marker_two"),
        "set_mark 이 scan 커서를 밀었다 — 그 앞에 이미 와 있던 출력이 사라졌다: {seen}"
    );

    // 3. 반대 방향 — scan 읽기가 에이전트의 mark 를 안 움직인다.
    //
    // 위 2 의 mark 뒤에 출력을 내고, 그것을 **scan 이 먼저 먹은 뒤** mark 기준으로 읽는다.
    // scan 읽기가 `read_mark` 까지 밀면 그 구간이 mark 쪽에서 사라진다.
    //
    // 2 와 달리 이 단계는 순서를 바꿔도 그 회귀에서 죽는다 — 실측으로 확인했다. 다만 옛
    // 순서에서는 대기 헬퍼의 timeout 으로 죽어서 실패문이 이 성질을 안 말했다. 지금은 이
    // 단계 자신의 단정에서 죽는다.
    echo("scan_marker_three");
    wait_scan("scan_marker_three");
    let since_mark = tasty.read_since_mark(sid);
    assert!(
        since_mark.contains("scan_marker_three"),
        "scan 읽기가 에이전트의 mark 를 밀었다 — mark 이후 구간에서 출력이 사라졌다: \
         {since_mark}"
    );
}

/// 소비자가 든 위치로 읽는 형태(`surface.read_since_mark` 의 `cursor`/`stream`)를
/// 실제 IPC 왕복으로 잰다.
///
/// 인파일 단위시험(`crates/tasty-terminal/src/output_buffer.rs`)이 버퍼 수준의 성질을
/// 재고 핸들러 단위시험이 인자 규칙을 재지만, **응답의 칸들이 실제로 wire 에 실리는지**
/// 와 **두 소비자가 IPC 경계를 건너서도 서로를 안 미는지** 는 그 둘 어디에도 없다.
/// 여기서 그 왕복을 지난다(ADR-0341).
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

    // 1. 위치를 안 주는 첫 읽기가 이어 읽을 좌표를 전부 준다. 칸 하나라도 안 실리면
    //    여기서 죽는다 — 아래 단계들은 빈 값으로도 초록이 될 수 있는 형태가 있다.
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

    // 2. 두 소비자가 각자 위치를 들고 번갈아 읽어도 서로를 안 민다.
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

    // B 는 A 와 같은 자리에서 아직 안 읽었다. A 의 읽기가 B 의 자리를 소비했으면
    // 여기서 빈다.
    let b_read = read(json!({"cursor": b, "stream": stream}));
    assert!(
        b_read["text"]
            .as_str()
            .unwrap_or_default()
            .contains("cursor_marker_one"),
        "A 의 읽기가 B 의 위치를 움직였다: {b_read:?}"
    );

    // 그리고 A 는 이미 본 것을 다시 안 받는다 — B 가 읽었다고 A 가 밀리지도 않는다.
    let a_again = read(json!({"cursor": a, "stream": stream}));
    assert!(
        !a_again["text"]
            .as_str()
            .unwrap_or_default()
            .contains("cursor_marker_one"),
        "B 의 읽기가 A 의 위치를 되돌렸다: {a_again:?}"
    );

    // 3. `set_mark` 은 소비자가 든 위치를 안 건드린다.
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

    // 4. 거절 셋이 **사유를 값으로** 돌려준다. 문구가 아니라 `error.data.reason` 으로
    //    갈려야 호출자의 다음 동작이 문구에 안 묶인다.
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

    // 5. `max_bytes` 가 **원문** 바이트를 자르고 `next_cursor` 가 그만큼만 전진한다.
    //
    //    ★ 이 단계는 "전진이 `text` 길이가 아니라 원문 길이를 센다" 를 **못 잰다.**
    //    셸이 낸 출력이 ASCII 면 둘이 같은 수라, `next_cursor` 를 `text` 길이로
    //    바꿔치기해도 여기서 안 죽는다(변이로 확인했다). 그 성질을 가르는 자리는
    //    `output_buffer.rs` 의 인파일 시험과 `handler/surface/mark.rs::answered` 의
    //    시험이고, 둘 다 길이가 갈리는 입력을 손으로 만든다.
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

    // send_key (enter)
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

    // send_key: navigation keys
    for key in &["up", "down"] {
        let result = tasty.call("surface.send_key", json!({"surface_id": sid, "key": key}));
        assert_eq!(result["sent"], true, "Failed to send key: {}", key);
    }

    // send_to specific surface
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
    // zsh ZLE 의 경우 Alt+X 는 execute-named-cmd 위젯을 호출하여 prompt 가
    // "execute: " 로 바뀐다. 후속 단언이 일반 명령으로 동작하도록 Ctrl+G 로
    // mode 를 abort 시킨다. (사용자 dotfile 이 ^G 를 rebind 할 수 있으나,
    // 본 테스트는 HOME/ZDOTDIR 을 격리해 stock 셸 동작을 보장한다.)
    tasty.call(
        "surface.send_combo",
        json!({"surface_id": sid, "key": "g", "modifiers": ["ctrl"]}),
    );
    // sentinel echo 로 abort 가 실제로 풀려 prompt 가 명령을 받을 수 있는
    // 상태인지 deterministic 하게 확인.
    tasty.set_mark(sid);
    tasty.send_text(sid, "echo __abort_ok__\n");
    tasty.wait_for_output(sid, "__abort_ok__", Duration::from_secs(3));
}

// ========== surface.completion (highlight producer) ==========

#[test]
fn surface_completion_reaches_the_pipeline() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-completion");

    // completion IPC 가 CLI→핸들러→intent→cascade 전 경로로 라우팅되어 success 를
    // 돌려주는지(=method_not_found 아님) 확인. highlight 발동 자체는 host 렌더라
    // 헤드리스로 관측 불가 — 여기선 파이프라인 도달만 검증한다.
    let completion = tasty.call("surface.completion", json!({ "surface_id": sid }));
    assert_eq!(completion["ok"], true);
    assert_eq!(completion["surface_id"].as_u64().unwrap(), sid);
}

// ========== surface.attention.{get,clear} (해제 표면 왕복) ==========

#[test]
fn surface_attention_raise_and_clear() {
    // raise 는 되지만 해제가 없던 비대칭의 회귀 가드. 해제 producer 두 개(실 렌더
    // 포커스·알림 읽음)는 전부 GUI 로컬 사건이라 IPC 로 관측/구동할 수 없어, 이
    // 왕복이 해제 축을 프로토콜 레벨에서 실행하는 유일한 경로다.
    //
    // 대상은 포커스 surface 가 아니라 **IPC 로 새로 만든 워크스페이스의 surface** 다
    // — `gpu.rs` 가 매 렌더 프레임 실-포커스 surface 의 attention 을 지우므로 포커스
    // surface 위에서는 raise 가 프레임 하나를 못 넘긴다. IPC 로 만든 워크스페이스는
    // active 를 전환하지 않아(원칙 1·3) 그 surface 는 렌더 포커스를 얻지 않는다.
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

    // (1) raise → 조회로 kind 가 보인다.
    raise_kind("needs_input");
    assert_eq!(attention_kind(), "needs_input");

    // (2) kind 필터 불일치는 지우지 않는다 — 그 사이 더 급한 kind 로 재발동한 신호를
    //     늦게 도착한 해제가 덮지 않게 하는 계약.
    let mismatched = tasty.call(
        "surface.attention.clear",
        json!({ "surface_id": att_sid, "kind": "completion" }),
    );
    assert_eq!(mismatched["ok"], true);
    assert_eq!(mismatched["cleared"], false);
    assert_eq!(mismatched["previous_kind"], "needs_input");
    assert_eq!(attention_kind(), "needs_input");

    // (3) kind 생략 = kind 무관 해제.
    let cleared = tasty.call("surface.attention.clear", json!({ "surface_id": att_sid }));
    assert_eq!(cleared["cleared"], true);
    assert_eq!(cleared["previous_kind"], "needs_input");
    assert!(attention_kind().is_null());

    // (4) 이미 없는 상태의 재호출도 성공(idempotent).
    let again = tasty.call("surface.attention.clear", json!({ "surface_id": att_sid }));
    assert_eq!(again["ok"], true);
    assert_eq!(again["cleared"], false);
    assert!(again["previous_kind"].is_null());

    // (5) 해제 후 재발동이 정상 동작하고, 일치하는 kind 필터는 실제로 지운다.
    raise_kind("completion");
    assert_eq!(attention_kind(), "completion");
    let matched = tasty.call(
        "surface.attention.clear",
        json!({ "surface_id": att_sid, "kind": "completion" }),
    );
    assert_eq!(matched["cleared"], true);
    assert!(attention_kind().is_null());

    // (6) 존재하지 않는 surface / 알 수 없는 kind 는 명시적 에러.
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
    // surface_id 필수 (포커스 독립, 불가침 원칙 1).
    assert!(
        tasty
            .call_raw("surface.attention.clear", json!({}))
            .get("error")
            .is_some()
    );
}

// ========== Dim (SGR 2) renderer regression ==========

// printf is a posix builtin; shell on Windows is cmd.exe by default which does not
// interpret \033 escapes the same way, so we restrict to Unix.
#[cfg(not(windows))]
#[test]
fn dim_sgr2_survives_to_the_renderer() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-dim-sgr2");

    tasty.set_mark(sid);
    tasty.send_text(sid, "clear; printf '\\033[2mD\\033[0mN\\n'\n");
    tasty.wait_for_output(sid, "DN", Duration::from_secs(5));
    // Allow the renderer one frame to apply the SGR before querying cell state.
    std::thread::sleep(Duration::from_millis(200));

    // `surface.screen_text` 는 기본(`show_dim:false`)으로 dim 셀을 걸러낸다
    // (에이전트가 ghost suggestion 을 실제 입력으로 오인하지 않게 하는 기본값).
    // 여기서 찾는 "D" 가 바로 그 dim 셀이므로 명시적으로 켜서 조회한다.
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

// ========== Hooks ==========

#[test]
fn hook_set_list_unset() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-hooks");

    let hook_result = tasty.call(
        "hook.set",
        json!({"surface_id": sid, "event": "bell", "command": "echo hooked"}),
    );
    let hook_id = hook_result["hook_id"].as_u64().unwrap();
    assert!(hook_id > 0);

    // hook.list 는 전역이라 길이 산술 대신 멤버십으로 본다.
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

// ========== Structural mutations ==========

#[test]
fn structural_mutations_within_one_workspace() {
    let (tasty, ws, _sid, pid, _lane) = scenario("e2e-structural");

    // 이 workspace 안의 pane 수만 센다 — 전역 `pane.list` 길이는 다른 시나리오가
    // 동시에 pane 을 만들고 닫으므로 산술이 성립하지 않는다.
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

    // split pane
    let panes_before = panes_in_ws();
    let split_result = tasty.call(
        "split",
        json!({"level": "pane", "direction": "vertical", "target_pane": pid}),
    );
    let new_pane_id = split_result["new_pane_id"].as_u64().unwrap();
    assert_eq!(panes_in_ws(), panes_before + 1);

    // create tab — tab.list 는 pane 스코프라 그대로 안전하다.
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

    // close tab
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

    // close pane (the one we split off)
    let close_pane_result = tasty.call("pane.close", json!({"pane_id": new_pane_id}));
    assert_eq!(close_pane_result["closed"], true);
    assert_eq!(panes_in_ws(), panes_before);

    // close last pane → should refuse (sole-pane 판정은 workspace 레이아웃 단위다)
    let result = tasty.call("pane.close", json!({"pane_id": pid}));
    assert_eq!(result["closed"], false);

    // close last tab → should refuse
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
    // 전역 길이 델타(`before + 1`)는 병렬 시나리오가 동시에 workspace 를 만들면
    // 깨진다 — 만든 workspace 가 목록에 있는지로 본다.
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

// ========== tab.close self-protection guard is tab-scoped, not pane-scoped ==========

#[test]
fn tab_close_guard_is_tab_scoped_not_pane_scoped() {
    // Regression: the guard used to check "does caller belong to the same PANE as the
    // target tab", which wrongly blocked closing a sibling tab. tab.close only affects
    // that tab (and its own SurfaceGroup), so the guard must match that blast radius.
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

    // caller (sid) lives in own_tab_id, not sibling_tab_id → closing the sibling must succeed.
    let result = tasty.call(
        "tab.close",
        json!({"tab_id": sibling_tab_id, "caller_surface_id": sid}),
    );
    assert_eq!(result["closed"], true);

    // Re-create the sibling so own_tab_id is no longer the last tab, then confirm closing
    // the tab that actually contains the caller is still refused (the guard's real purpose).
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

// ========== Renderer color resolution (debug.glyph_color) ==========

#[test]
fn renderer_resolves_dim_and_plain_glyph_colors() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-glyph-color");

    // Inject deterministic VTE: clear screen, home cursor, plain Z, then dim Z.
    // - col 0 row 0: plain Z (intensity = normal)
    // - col 1 row 0: dim Z (intensity = half)
    //
    // Sequence: ESC[2J ESC[H Z ESC[2m Z ESC[0m
    // Hex:      1b5b324a 1b5b48 5a 1b5b326d 5a 1b5b306d
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

    // The plain cell's fg must be the default fg (no SGR fg color was set).
    let pr = plain_fg["r"].as_f64().unwrap();
    let pg = plain_fg["g"].as_f64().unwrap();
    let pb = plain_fg["b"].as_f64().unwrap();
    assert!(
        pr > 0.5 && pg > 0.5 && pb > 0.5,
        "plain fg should be bright"
    );

    // palette::compute_cell_colors blends fg toward bg for Intensity::Half,
    // so the dim cell's fg must differ from a plain cell's fg on the same row.
    assert_ne!(
        plain_fg, dim_fg,
        "dim cell fg should differ from plain cell fg (SGR 2 must dim)",
    );

    // Each channel of the dim fg should sit between the plain fg and the bg
    // (i.e. moved toward the background, not past it or in some other direction).
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

// ========== Error paths ==========

#[test]
fn error_paths_reject_malformed_calls() {
    let (tasty, _ws, sid, _pid, _lane) = scenario("e2e-error-paths");

    // method_not_found
    let resp = tasty.call_raw("nonexistent.method", json!({}));
    assert!(resp.get("error").is_some());
    assert_eq!(resp["error"]["code"].as_i64().unwrap(), -32601);

    // send_combo missing key
    let resp = tasty.call_raw(
        "surface.send_combo",
        json!({"surface_id": sid, "modifiers": ["ctrl"]}),
    );
    assert!(resp.get("error").is_some());

    // send_to nonexistent surface
    let resp = tasty.call_raw(
        "surface.send_to",
        json!({"surface_id": 99999, "text": "hello"}),
    );
    assert!(resp.get("error").is_some());

    // missing required params
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
    // completion 은 surface_id 필수 (포커스 독립, 불가침 원칙 1).
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

// ========== headless PTY (pty.*) — 6+1 메서드 통합 흐름 ==========

#[test]
fn headless_pty_spawn_write_wait_kill() {
    // spawn → list → write → read → wait(exit_code) → kill. Surface 없이 돌던 PTY 가
    // 진짜 exit-code 를 회수하는지 end-to-end 로 검증한다.
    let _lane = lane();
    let tasty = common::shared();

    // spawn: bare shell(command 없음). pty id 는 disjoint 고범위(>= 0x8000_0000).
    let spawned = tasty.call("pty.spawn", json!({}));
    let pty_id = spawned["pty_id"]
        .as_u64()
        .expect("pty.spawn returns pty_id");
    assert!(
        pty_id >= 0x8000_0000,
        "pty id 는 surface id 와 disjoint 한 고범위여야: {pty_id}"
    );

    // list: 방금 만든 headless PTY 가 필터 없이 전체 목록에 등장.
    let listed = tasty.call("pty.list", json!({}));
    assert!(
        listed["ptys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"].as_u64() == Some(pty_id)),
        "pty.list 가 방금 spawn 한 pty 를 빠뜨림: {listed:?}"
    );

    // write → read: 셸에 echo 를 보내고 화면 텍스트에 반영되는지 폴링.
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

    // write(exit 5) → wait: watcher 가 잡은 진짜 exit_code 를 폴링으로 확인.
    tasty.call("pty.write", json!({ "id": pty_id, "text": "exit 5\n" }));
    let wait_start = std::time::Instant::now();
    let exited = loop {
        let w = tasty.call("pty.wait", json!({ "id": pty_id }));
        if w["exited"].as_bool() == Some(true) {
            break w;
        }
        if wait_start.elapsed() > Duration::from_secs(10) {
            // 첫째 자리(pty.rs exit_wait_failure)와 같은 수준으로 말하게 한다 — IPC 경계라
            // 그 함수를 공유하진 못하므로 pty.read 로 같은 관측(화면·scrollback·alt)을 꺼낸다.
            // 상한(10s)·폴링·단정은 안 건드리고, 실패했을 때 무엇을 말하는가만 넓힌다.
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
                    "빈 채 — 셸이 아무것도 안 뱉음(exec 미기동 쪽)"
                } else {
                    "내용 있음 — 셸은 떴다(우리가 쓴 것이 안 들어갔거나 안 죽는 쪽)"
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

    // kill: 이미 종료된 PTY 도 두 store 에서 회수 → list 에서 사라진다.
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
    // 살아있는 headless PTY 를 실제 Tab 으로 승격 — 승격 시 실제 surface 로 등장하고
    // headless 목록에서는 빠지는지 확인한다.
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
    // 승격된 surface 는 실제 surface.list 에 등장하고, headless 목록에선 사라진다.
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

// ========== Multi-window: owner-based routing + list 전체 순회 ==========

/// X11 창이 화면에 보이는가(map state 가 `IsViewable`). 에이전트 창은 숨긴 채 만들어 등록
/// 뒤에 보이는데(ADR-0497) `window.list` 에는 보임 필드가 없어, 보이게 하는 호출이 빠지는
/// 회귀가 IPC 로는 안 보인다 — 그래서 X 서버에 직접 묻는다.
///
/// 하네스가 자식에게 준 디스플레이(`TASTY_E2E_DISPLAY`, `inherit` 이면 `DISPLAY`)를 연다.
/// `Ok(None)` 은 물을 수 없는 경우다 — `inherit` 에 Wayland 세션이면 창이 X 창이 아닐 수
/// 있고, 그때 X 에 그 id 를 물으면 Xlib 기본 에러 처리가 프로세스를 끝낸다.
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
    // SAFETY: cname 은 NUL 종단 문자열이고 반환값은 아래에서 null 을 검사한다.
    let dpy = unsafe { (x.XOpenDisplay)(cname.as_ptr()) };
    if dpy.is_null() {
        return Err(format!("XOpenDisplay({name}) failed"));
    }
    // SAFETY: XWindowAttributes 는 정수·포인터만 담은 C 구조체라 0 이 유효한 초기값이다.
    let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
    // SAFETY: dpy 는 위에서 연 연결, xid 는 이 디스플레이에 창을 만든 데몬이 돌려준 X 창
    // id(winit 의 X11 `WindowId` 는 X 창 id 다)이고 attrs 는 쓰기 가능한 지역 변수다.
    let status = unsafe { (x.XGetWindowAttributes)(dpy, xid as xlib::Window, &mut attrs) };
    // SAFETY: 위에서 연 연결을 닫는다. 이후 dpy 를 쓰지 않는다.
    unsafe { (x.XCloseDisplay)(dpy) };
    if status == 0 {
        return Err(format!("XGetWindowAttributes(0x{xid:x}) failed"));
    }
    Ok(Some(attrs.map_state == xlib::IsViewable))
}

/// `xid` 가 5 초 안에 보이게(`IsViewable`) 되지 않으면 panic 한다. 물을 수 없는 경우
/// (`x11_window_is_viewable` 의 `Ok(None)`)는 경고만 남기고 넘어간다.
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

/// **이 파일에서 유일하게 창을 요구하는 시나리오다.** `window.create` 는 gui 라우터의
/// `app_methods` step 에만 있어 헤드리스 데몬에서는 `-32017`("표에는 있는데 이 바이너리에
/// arm 이 없다")이 난다 — 배선 결함이 아니라 창이 없다는 사실 그 자체이므로, 헤드리스
/// 조합 CI 는 **이 이름 하나만**
/// `--skip` 한다(`.github/workflows/crossplatform-check.yml`,
/// `crates/tasty-doc-guards/tests/headless_skip_names_are_exact.rs` 가 그 이름의 정확성을 강제한다).
#[test]
fn multi_window_owner_routing() {
    // 두 번째 main window 를 IPC 로 생성하고, 두 윈도우의 surface 가 모두 IPC 로
    // 접근 가능한지 검증. CLAUDE.md "포커스 독립". 에이전트 창은 focused 를 옮기지
    // 않으므로(ADR-0497) focused 는 첫 윈도우에 남는다 — 그래서 owner 라우팅을
    // 실제로 재는 것은 focused 가 아닌 새 윈도우 surface 로의 send 다.
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
        "window.create 전에 focused 창이 있어야 아래 단언이 무언가를 잰다"
    );

    let create_resp = tasty.call("window.create", json!({}));
    // window.create 는 더 이상 fire-and-forget(`{"scheduled": true}`)이 아니라 완료
    // 채널로 생성 성공/실패를 왕복시킨다 — 성공은 `{"created": true, "window_id": …}`
    // (ADR-0122). 옛 `scheduled` 계약을 보면 Null 이 잡힌다.
    assert_eq!(
        create_resp["created"], true,
        "window.create 성공 응답이 created=true 를 실어야 한다: {create_resp:?}"
    );
    assert!(
        create_resp["window_id"].as_u64().is_some(),
        "window.create 성공 응답에 window_id 가 있어야 한다: {create_resp:?}"
    );
    // 에이전트가 만든 창은 사용자가 보던 창의 focused 를 가져가지 않는다(ADR-0497).
    assert_eq!(
        focused_window(tasty),
        focused_before,
        "window.create 뒤 window.list 의 focused 는 원래 창이어야 한다: {create_resp:?}"
    );
    // 에이전트 창은 숨긴 채 만들어 등록 뒤에 보인다(ADR-0497) — 결국 화면에 보여야 한다.
    #[cfg(all(target_os = "linux", feature = "gui"))]
    wait_x11_window_viewable(
        create_resp["window_id"]
            .as_u64()
            .expect("window.create 성공 응답에 window_id 가 있어야 한다"),
    );

    // 새 윈도우의 PTY shell 이 surface.list 에 등장할 때까지 polling.
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
        if let Some(&id) = new_ids.first() {
            // pty_ready 까지 기다리기.
            if arr
                .iter()
                .any(|s| s["id"].as_u64() == Some(id) && s["pty_ready"].as_bool() == Some(true))
            {
                break id;
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            panic!("second window surface did not appear in 10s. surface.list = {arr:?}");
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    // surface.list 전체 순회 — 두 surface 모두 보여야.
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

    // workspace.list 도 전체 순회.
    let workspaces = tasty
        .call("workspace.list", json!({}))
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        workspaces.len() >= 2,
        "workspace.list 가 모든 engine 의 workspace 를 합쳐 반환해야: {workspaces:?}"
    );

    // `tree` 도 전체 순회 — 이름이 `*.list` 가 아니라서 오래 빠져 있던 자리다.
    // 판정을 수로 하지 않고 **`workspace.list` 와 같은 id 집합**인지로 한다: 둘이
    // 같은 물음에 답하므로, 한쪽만 창을 건너면 그 자리에서 갈린다.
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
        "tree 와 workspace.list 의 workspace 집합이 달라졌다 — 한쪽이 포커스된 창만 \
         보고 있다. tree={tree:?} workspace.list={workspaces:?}"
    );

    // owner-based routing: focused 는 첫 윈도우다(에이전트 창은 focused 를 옮기지
    // 않는다, ADR-0497). 첫 윈도우 surface 로의 send 는 focused 폴백과 owner 가 같은
    // 창이고, 아래 두 번째 윈도우 surface 로의 send 가 owner 라우팅을 잰다.
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

    // 두 번째 윈도우 surface 에도 send 동작 — focused 가 아닌 창이라 owner 라우팅이
    // 아니면 닿지 않는다.
    tasty.set_mark(new_sid);
    let send_second = tasty.call(
        "surface.send",
        json!({"surface_id": new_sid, "text": "echo W2_owner_route\n"}),
    );
    assert_eq!(send_second["sent"], true);
    let out2 = tasty.wait_for_output(new_sid, "W2_owner_route", Duration::from_secs(5));
    assert!(out2.contains("W2_owner_route"));

    // 만든 창을 닫는다 — 위 단언이 전부 끝난 뒤라 검증은 그대로 남는다.
    //
    // **차선 잠금이 이것까지 해 주지 않는다.** write 차선은 이 시나리오가 남들과
    // 겹치지 않게 할 뿐이고, 잠금을 놓는 순간 **남긴 창**은 뒤에 도는 read 차선
    // 전부에게 보인다. 실제로 그 잔여가 형제 테스트의 전제를 바꿔 회차에서만
    // 빨개진 적이 있다 — 단독 실행에서는 순서가 반대라 안 났다.
    //
    // 닫힘을 **단언한다**. 조용히 실패하면 잔여가 그대로 남아 같은 형태가 다시
    // 나는데, 그때 이 자리는 정리한 것처럼 보인다.
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

// ========== plugin 읽기 표면 — 창 없이 답하는가 ==========

/// `plugin.list` 가 창 없이 답한다.
///
/// 이전에는 헤드리스에서 `-32601`(그런 메서드 없다)이었다. 그것은 `plugin.*`
/// 관리 표면이 통째로 없다는 뜻이었고, CLI 전용 실행 형태인 헤드리스에서
/// `docs/identity.md` 원칙 2(에이전트 기능은 IPC + CLI 양면)에 정면으로 걸렸다.
///
/// **왜 통합 테스트여야 하는가.** 이 경로의 단위 테스트
/// (`src/adapters/ipc/handler/plugin.rs`)는 매니저가 **있을 때** 에러가 아니라는 것을
/// 못 잰다 — `PluginManager` 는 waker 와 registry port 를 요구해 단위 테스트가 만들 수
/// 없고, `tasty-host-plugin` 의 테스트용 생성자는 그 크레이트의 `#[cfg(test)]` 라 여기서
/// 보이지 않는다. 그래서 단위 테스트만 두면 "모든 응답이 에러" 여도 통과한다. 라우팅
/// 표(`READONLY_METHODS`)와 dispatch arm 이 실제로 이어져 있다는 것도 여기서만 잰다.
///
/// 헤드리스 조합에서는 테스트가 띄우는 바이너리 자체가 헤드리스라, 이 단언은 그
/// 조합에서 **창이 없는 데몬**을 상대로 돈다. gui 조합에서도 성립해야 한다 — 두
/// 라우터가 같은 함수를 쓰기 때문이다.
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

    // 개수는 홈 상태에 달렸으므로 세지 않는다. 배열이라는 것과, 있다면 각 항목이
    // 매니페스트에서 나온 모양이라는 것만 본다.
    for p in plugins {
        assert!(
            p.get("id").and_then(|v| v.as_str()).is_some(),
            "plugin 항목에 id 가 없다: {p}"
        );
    }
}

/// `plugin.show` 는 매니저가 있어도 **없는 plugin 이름**에는 다른 에러를 준다.
///
/// 이것이 위 테스트의 대조다 — 위만 있으면 "모든 plugin.* 가 무조건 성공" 이어도
/// 통과한다. 여기서 요구하는 것은 성공/실패가 **입력에 따라 갈린다**는 것이다.
/// `-32000`(매니저 없음)이 아니라 `-32003`(그 plugin 이 설치돼 있지 않음)이어야
/// 매니저가 실제로 세워져 조회가 수행됐다는 뜻이 된다.
///
/// **두 코드의 차이가 이 축의 계측기다.** `-32000` 은 "물어볼 대상 자체가 없다",
/// `-32003` 은 "물어봤고 그런 것이 없더라" 다. 앞의 것으로 느슨하게 고치면 이
/// 테스트는 매니저가 한 번도 안 세워져도 통과한다 — 즉 검증하려던 것을 정확히
/// 놓친다. 코드를 바꿔야 한다면 무엇이 계측되는지 먼저 다시 세워라.
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

/// 수명주기 **토글 둘**은 창 없이 답한다 — `-32017`(이 빌드에 arm 이 없다)이 아니다.
///
/// 파라미터를 **일부러 빼고** 부른다. 그러면 응답이 arm 안의 파싱에서 나므로
/// (`-32602 Missing 'id' parameter`), 이 단언은 매니저 상태도 홈 상태도 안 보고
/// **arm 이 이 바이너리에 있는가** 하나만 잰다. 공유 데몬을 상대로 도는 테스트라
/// plugin 을 실제로 켜고 끄면 다른 테스트의 전제를 바꾼다 — 그래서 부수효과가
/// 0 인 갈래로 잰다.
///
/// 두 코드의 차이가 계측기다: `-32017` 은 "이 조합에 안 들어 있다", `-32602` 는
/// "들어 있고 인자가 틀렸다" 다. 뒤엣것이 와야 배선됐다는 뜻이 된다.
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

/// 나머지 수명주기 메서드는 **여전히 없다** — 위 축이 연 것은 토글 둘뿐이다.
///
/// **이 테스트는 위 셋이 빨개질 때 초록으로 남아야 한다.** 배선
/// (`src/boot/headless_dispatch.rs` 의 읽기 전용 · 토글 가로채기)을 죽이면 위 세
/// 테스트는 실패하는데, 이것은 그대로 통과한다. 넷이 함께 빨개지면 그 변이는
/// "무언가 깨졌다" 만 말하고, 배선이 **정확히 그 셋을 만든다**는 것은 말하지 못한다.
/// 이 비대칭이 이 네 테스트의 판정력이므로, 넷을 한 조건으로 묶도록 고치지 마라.
///
/// `plugin.remove` 는 파일을 지우고 `plugin.grant` 는 권한을 바꾼다 — 토글과 달리
/// 헤드리스에서 열지 여부가 각각 별도 결정이라 아직 없다
/// (`docs/dev-guide/headless-ipc-surface.md`).
#[cfg(not(feature = "gui"))]
#[test]
fn the_remaining_lifecycle_methods_are_still_absent_in_a_headless_daemon() {
    let _lane = lane();
    let tasty = common::shared();
    for method in ["plugin.remove", "plugin.grant"] {
        let resp = tasty.call_raw(method, json!({"id": "anything"}));
        let code = resp
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_i64());
        assert_eq!(
            code,
            Some(-32017),
            "헤드리스에서 {method} 는 아직 arm 이 없는 메서드여야 한다. `-32601` 이 \
             왔다면 표에서 이름이 빠진 것이고, 그러면 호출자가 오타와 구분할 수 없다: {resp}"
        );
    }
}

/// 헤드리스에서 `file_handler.dispatch` 는 **수락했다고 답하지 않는다** — `-32017` 로 거절한다.
///
/// 그 intent 를 적용할 identify worker 와 결과를 여는 창이 gui 에만 있다. 예전에는 arm 이
/// 헤드리스에도 있어 `{"accepted": true}` 로 답하고 요청을 로그 한 줄과 함께 버렸다 —
/// 에이전트는 성공으로 읽었다(docs/adr/0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md).
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

/// 헤드리스는 attach mirror 로 나가는 두 plugin 요청을 **수락하지 않는다** — 큐를 비워 attach
/// 채널로 보내는 쪽(`App::about_to_wait`)이 gui 에만 있어, 수락하면 `request_id` 만 받은 plugin 이
/// 결과를 영영 기다린다. 라우터 끝의 `-32017` 이 답하고, 문구가 메서드 이름을 실어 **무엇이
/// 거절됐는지** 두 자리가 갈린다(docs/dev-guide/headless-ipc-surface.md "census 뒤에 게이트된 arm").
/// 셋째 자리(mirror 구조 op)는 헤드리스에 mirror workspace 가 없어 여기서 못 부른다 — 그 거절은
/// `core::attach_runtime` 의 `dispatch_refuses_tab_create_in_mirror_workspace_in_headless` 가 본다.
#[cfg(not(feature = "gui"))]
#[test]
fn mirror_forward_requests_are_refused_by_name_in_a_headless_daemon() {
    let _lane = lane();
    let tasty = common::shared();
    let mut messages = Vec::new();
    for (method, params) in [
        (
            "git_viewer.query",
            json!({"kind": "status", "local_surface_id": 1}),
        ),
        ("markdown_mirror.content_request", json!({"surface_id": 1})),
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

/// 헤드리스는 레이아웃을 영속하지 않는다 — 슬롯을 점유하지 않고, 그 사실을 `system.info` 의
/// `layout_slot: null` 로 답한다(docs/adr/0539-headless-does-not-persist-layouts.md). 에이전트가
/// "재시작하면 워크스페이스가 돌아오는가" 를 이 값 하나로 판정하므로, 슬롯을 잡기 시작하면(저장·
/// 복원 배선이 생기면) 이 시험이 먼저 알린다.
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

/// `general.restore_layout` 을 켠 헤드리스는 그 설정이 이 빌드에서 아무 일도 안 한다는 것을
/// 부팅 때 **경고로** 말한다(docs/adr/0539-headless-does-not-persist-layouts.md 결정 ①). 문구는
/// `src/boot.rs` 의 단위 시험이 재지만, 그 시험은 함수의 반환값만 본다 — 부팅이 그것을 어느
/// 레벨로 내보내는지는 이 시험만 본다. stderr 기본 필터가 warn 이라, 레벨이 info 이하로 내려가면
/// 사람은 아무것도 못 보는데 단위 시험은 초록으로 남는다. 그래서 자식의 stderr 를 **제품 기본
/// 필터 그대로**(하네스가 `spawn_diag::LOG_FILTER` 로 박는다) 읽어 줄이 있는지와 그 줄의 레벨을
/// 함께 단언한다.
#[cfg(not(feature = "gui"))]
#[test]
fn a_headless_daemon_warns_at_boot_that_restore_layout_is_ignored() {
    let _lane = lane();
    let tasty = common::TastyInstance::spawn_with_restore_layout();
    // 고지는 포트 파일 **뒤**에 나간다 — `run_headless` 가 IPC 를 먼저 열어 포트 파일을 쓰고
    // (`start_ipc_and_seed`), 그 다음 `bootstrap_engine` 이 고지를 warn 한다. 그래도 하네스 spawn 은
    // 포트 파일 뒤에 `surface.list`·셸 대기로 메인 루프를 왕복한 다음 돌아오므로, 이 시점에는
    // 고지가 이미 stderr 로 나갔다. 아래 폴링은 배출 스레드가 링에 넣는 지연만 흡수한다.
    // `system.info` 호출은 지금은 중복이다 — 하네스 spawn 이 나중에 포트 파일만 보고 돌아오게
    // 바뀌어도 이 시험이 안 흔들리게 하는 보험이다(메인 루프가 응답했으면 고지는 나갔다). 이
    // 보험은 문다 — 하네스를 영구히 바꿀 필요 없이 임시 변이로 잰다: `src/boot.rs` 의 고지 직전에
    // 6 초 sleep 을 넣고(초록), `tests/common/mod.rs` 의 `wait_for_shell` 호출을 빼 하네스가 포트
    // 파일만 보고 돌아오게 하면 여전히 초록이며, 거기서 이 호출까지 빼면 아래 단언이 FAILED 가 된다.
    // 6 초는 아래 폴링 창(`from_secs(5)`)보다 길게 잡은 값이다. 창보다 짧으면 호출을 뺀 마지막
    // 변이에서도 폴링이 지연을 흡수해 초록이 나와 "보험이 안 문다" 로 거꾸로 읽힐 것이다 — 이것은
    // 창 길이에서 끌어낸 추론이고, 더 짧은 sleep 으로 돌려 본 적은 없다.
    tasty.call("system.info", json!({}));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let line = loop {
        if let Some(line) = tasty.find_stderr(|l| l.contains("general.restore_layout is on")) {
            break line;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "restore_layout 을 켠 헤드리스가 부팅 고지를 stderr 에 안 냈다 — 레벨이 기본 필터 아래로 \
             내려갔거나 호출이 사라졌다:\n{}",
            tasty.startup_diagnostics()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    // fmt 레이어는 파이프에도 ANSI 색을 붙인다 — 레벨 토큰을 가르기 전에 벗긴다.
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

/// 디렉토리 dispatch 는 host 기본 핸들러 `directory-system`(OS 열기)으로 간다 — 하네스가 켠
/// `TASTY_DEBUG_OS_OPEN_LOG` 아래에서는 그것이 **띄워지지 않고 기록된다.**
///
/// 이 스위치가 없으면 시험 인스턴스의 OS 열기가 실행자의 이미 떠 있는 브라우저로 URL 을
/// 넘긴다 — 격리 홈도 전용 디스플레이도 그 채널을 못 막는다(ADR-0511). 기록 줄이 안 생기면
/// 그 열기는 실제로 실행됐다는 뜻이다.
///
/// `debug_assertions` 로 막는다 — 스위치는 debug 격리라 release 로 지은 자식에는 없고, 그때 이
/// 시험은 실패하기 **전에** 실제로 OS 열기를 띄운다. 막으려던 바로 그 부수효과다.
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

/// `file_handler.reload` 는 적용하지 않은 user 항목을 `rejected` 에 사유와 함께 싣는다 — 기존 필드는
/// 그대로다. 예전에는 `{path, exists}` 뿐이라 설정이 무시된 것이 로그에만 남았다
/// (docs/adr/0426-file-handler-reload-reports-the-entries-it-dropped.md).
///
/// 공유 인스턴스의 user 설정을 바꾸므로 단독 차선에서 돌고, 끝에 파일을 지우고 다시
/// reload 해 원래 상태(user 설정 없음)로 돌려 놓는다. 두 조합 모두에서 돈다.
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

    // 파일을 먼저 지워 두어 아래 단언이 실패해도 다음 시나리오가 이 설정을 안 물려받게 한다.
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

/// `file_handler.detectors` 는 finalize 된 detector 와 그것을 만든 출처별 원본을 함께 싣는다 —
/// 병합 결과만으로는 어느 출처가 이겼는지 밖에서 재현할 수 없다.
///
/// user 설정으로 host 기본 detector(`html`)에 표시명 patch 를 얹고 reload 한 뒤, 병합 결과와
/// contribution 두 줄(host · user)을 본다. 공유 인스턴스의 user 설정을 바꾸므로 단독 차선에서
/// 돌고, 끝에 파일을 지우고 reload 해 원래 상태로 돌려 놓는다. 두 조합 모두에서 돈다.
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

/// 대상을 **지목했는데 아무 창도 안 가진** 요청은 거절된다 — 포커스된 창으로 안 샌다.
///
/// 지우기 전의 폴백은 이 요청을 포커스된 창에 넘겼고, 그래서 **존재하지 않는
/// `workspace_id` 를 실은 `workspace.create` 가 조용히 성공했다**(실측 2026-09-05:
/// 포커스된 창에 워크스페이스를 만들고 그 id 를 돌려줬다). 호출자는 자기가 지목한
/// 곳에 만들어진 줄 안다. `docs/design/policies/focus.md` 의 "silent fallback 금지".
///
/// **두 조합 모두**에서 돈다. 헤드리스는 engine 이 하나라 라우팅할 곳이 없지만
/// 판정은 같아야 한다 — 한쪽만 거절하면 같은 요청이 조합에 따라 다르게 끝난다.
#[test]
fn a_request_naming_an_unowned_target_is_rejected() {
    let _lane = lane();
    let tasty = common::shared();

    // 지목했고 아무도 안 가졌다 → 에러. 핸들러가 그 키를 무시하더라도 그렇다.
    let resp = tasty.call_raw(
        "workspace.create",
        json!({ "workspace_id": 999_999, "name": "unowned-target-probe" }),
    );
    assert!(
        resp.get("error").is_some(),
        "지목한 대상을 아무도 안 가졌으면 거절해야 한다(성공하면 포커스된 창에 \
         만들어진 것이다): {resp}"
    );
    let msg = resp["error"]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("999999") && msg.contains("workspace"),
        "에러가 무엇을 못 찾았는지 말해야 고칠 수 있다: {resp}"
    );

    // plugin → host call 경로도 같은 판정을 받는다. 라우팅 사본이 둘이라
    // (IPC 라우터 / intent 디스패처) 한쪽만 고치면 다른 쪽이 조용히 옛 동작을 남긴다.
    // `image.*` 는 image plugin 의 namespace 이고 그 plugin 이 `image.open` 을 호스트로
    // 되던지므로, 이 호출이 그 사본을 탄다 — 응답의 `host call ... failed` 감싸기가
    // 경로를 드러낸다.
    //
    // 이 단언만 gui 인 이유는 이 축과 무관하다: 헤드리스에는 `image.open` 의 호스트
    // arm 자체가 없어서(`#[cfg(feature = "gui")]`) 되던진 호출이 소유 검사에 닿기 전에
    // "Method not found" 로 끝난다. 그건 headless 가 app 층 메서드를 떨어뜨리는
    // 별개 축이고, 그쪽이 닫히면 이 게이트도 없어진다.
    #[cfg(feature = "gui")]
    {
        let via_plugin = tasty.call_raw(
            "image.open",
            json!({ "surface_id": 999_999, "path": "/tmp/does-not-exist.png" }),
        );
        let via_msg = via_plugin
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or_default();
        assert!(
            via_msg.contains("surface 999999"),
            "plugin 을 경유한 호출도 지목한 대상이 없으면 같은 이유로 거절돼야 한다: \
             {via_plugin}"
        );
    }

    // plugin 이 점유한 namespace 의 메서드는 id 를 실었어도 **안 잘린다.**
    // 그 prefix 는 호스트 예약이 아니라서(번들 plugin 이 갖고 있다) plugin 이 답할 수
    // 있고, 헤드리스는 소유 검사가 engine handler 앞이라 여기서 자르면 forward 될
    // 호출을 불러 보기도 전에 죽인다. 이 단언이 그 경계를 지킨다.
    let forwarded = tasty.call_raw("markdown.recent", json!({ "surface_id": 999_999 }));
    assert!(
        forwarded.get("result").is_some(),
        "plugin namespace 의 메서드는 id 를 실어도 forward 돼야 한다: {forwarded}{}",
        common::bundle_staging_note()
    );

    // 지목 안 한 같은 메서드는 그대로 동작한다 — 폴백을 통째로 없앤 것이 아니다.
    let ok = tasty.call_raw("workspace.create", json!({ "name": "no-target-probe" }));
    assert!(
        ok.get("result").is_some(),
        "대상을 지목하지 않은 생성 요청은 계속 동작해야 한다: {ok}"
    );
}

/// app 층 표면 중 **창이 없어도 답이 정의되는 것**은 두 조합에서 같이 답한다.
///
/// 양방향으로 본다 — 연 것이 실제로 라우팅되는 것과, 안 연 것이 여전히 `-32601` 인
/// 것을 같은 회차에서. 한쪽만 보면 "전부 열었다" 와 "아무것도 안 열었다" 가 둘 다
/// 통과하는 판정이 된다.
///
/// 판정 기준은 성공이 아니라 **`-32601` 이 아님**이다. 인자 없이 부르므로 라우팅된
/// 메서드는 `-32602`(인자 오류)로 답하고, 그것이 "핸들러에 닿았다" 는 증거다. 성공을
/// 요구하면 클립보드·SSH 같은 환경 의존이 판정에 섞인다.
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
        // 라우팅 실패의 두 얼굴을 **함께** 배제한다. `-32601` 만 보면 이름이 표에서
        // 빠졌을 때만 잡히고, 표에 남은 채 arm 만 사라지면 `-32017` 로 조용히 통과한다.
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

    // 반대편. `window.list` 가 읽는 것은 `App.view` 라 헤드리스에 대응물이 없다.
    // gui 에서는 답하고 헤드리스에서는 **`-32017`("이 바이너리에 arm 이 없다")** 인 것이
    // 의도된 상태이며, 그것을 여기서 못 박아 둔다 — 안 그러면 위 루프만 남아 "전부 열어도
    // 통과" 가 된다.
    let resp = tasty.call_raw("window.list", json!({}));
    let code = resp["error"]["code"].as_i64();
    #[cfg(feature = "gui")]
    assert_ne!(code, Some(-32601), "gui 는 window.list 에 답한다: {resp}");
    #[cfg(not(feature = "gui"))]
    assert_eq!(
        code,
        Some(-32017),
        "헤드리스에 창이 없으므로 `window.list` 는 이 조합에 arm 이 없는 것이 정답이다. \
         `-32601` 이 왔다면 호출자가 오타와 구분할 수 없고, 다른 코드가 왔다면 \
         `App.view` 없이 답하는 길이 생긴 것이니 판정을 다시 세워라: {resp}"
    );
}

/// 플랫폼이 못 하는 debug 메서드는 **"없다" 가 아니라 "여기선 못 한다" 로** 답한다.
///
/// `surface.raw_key` 는 `DEBUG_METHODS` 에도 CLI 서브커맨드에도 플랫폼 조건 없이 있다.
/// 그런데 dispatch arm 만 macOS gui 게이트라, 상보 arm 이 없으면 다른 조합에서 `match` 의
/// `_` 가 받아 `-32601`("그런 메서드 없음")로 끝난다 — 이름은 맞고 표에도 있으므로 그 답은
/// 거짓이고, 그것을 받은 호출자는 오타를 의심하는 **틀린 수리**로 간다.
///
/// 소스 짝 맞춤은 `src/source_guards/platform_gated_dispatch_complement.rs` 가 본다.
/// 여기서는 그 짝이 실제로 **응답을 바꾸는지**를 실행으로 못 박는다 — 소스에 arm 이
/// 있다는 것과 그것이 라우터에 닿는다는 것은 다른 사실이다. 근거는 ADR-0154.
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
                "이 조합은 실행하지 못하지만 메서드는 **있다** — 상보 arm 이 사유와 함께 \
                 `-32015` 로 답해야 한다. `-32601` 이 왔다면 상보 arm 이 사라진 것이고, \
                 그러면 `tasty debug raw-key` 가 도움말에 뜨는데 '그런 메서드 없음' 으로 \
                 끝난다: {resp}"
            );
            let msg = resp["error"]["message"].as_str().unwrap_or_default();
            assert!(
                msg.contains("macOS-only"),
                "코드만으로는 무엇이 부족한지 알 수 없다 — 사유가 함께 와야 한다: {resp}"
            );
        }
    }
}

/// engine 층 조회 중 **창을 하나도 안 읽는 것**은 두 조합에서 같이 답한다.
///
/// `theme.query` 가 읽는 것은 전역 Theme 과 `CoreState.settings` 둘뿐이다. 그런데 그
/// 핸들러가 `gui` 게이트가 걸린 `webview` 모듈 안에 살고 있어서 arm 까지 함께 게이트됐고,
/// 헤드리스에서는 `-32601`(그런 메서드 없음)로 끝났다 — 읽을 것이 다 있는데도.
/// 헤드리스는 CLI 전용 실행 형태라 그 부재는 `docs/identity.md` 원칙 2 의 구멍이다.
///
/// 반대편도 같은 회차에서 본다. `webview.set_url` 이 쓰는 값을 소비하는 것은 렌더러뿐이라
/// 헤드리스에서 없는 것이 정답이고, 그것을 여기 못 박아 두지 않으면 위 단언만 남아
/// "전부 열어도 통과" 가 된다.
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
    #[cfg(feature = "gui")]
    assert_ne!(
        code,
        Some(-32601),
        "gui 는 webview.set_url 에 답한다: {resp}"
    );
    #[cfg(not(feature = "gui"))]
    assert_eq!(
        code,
        Some(-32017),
        "webview 의 URL 을 소비하는 것은 렌더러뿐이라 헤드리스엔 arm 이 없는 것이 \
         정답이다 — 다만 이름은 있으므로 오타(`-32601`)와는 다르게 답한다: {resp}"
    );
}
/// 지목한 대상이 **없으면** 부류를 가리지 않고 거절하고, **있으면** 그 검사가 안 걸린다.
///
/// 앞선 자리(`a_request_naming_an_unowned_target_is_rejected`)는 이 판정을 `workspace_id`
/// **한 키**로만 고정했다. 그런데 판정기가 보는 키는 열하나이고 부류는 일곱이다
/// (`core::request_target::params_resource_id`) — 한 키만 박아 두면 나머지 열이 조용히
/// 빠져도 초록이다. 실제로 이 저장소에서 같은 형태가 났다: 같은 판정을 워크스페이스
/// 단위에서는 하고 surface 단위에서는 안 하던 자리가 있었다(ADR-0156).
///
/// **두 방향을 짝으로 본다.** 거절만 세면 "전부 거절" 과 구별이 안 되므로, 같은 메서드에
/// **살아 있는** id 를 실었을 때 이 검사가 걸리지 않는 것을 같은 회차에서 확인한다.
///
/// 그리고 **두 조합에서 같은 몸통이 돈다.** gui 는 라우터 앞에서, 헤드리스는 engine
/// handler 앞에서 판정하는데 — 경로가 다르므로 결과가 같은지는 재야 안다.
#[test]
fn an_unowned_target_is_rejected_for_every_resource_kind() {
    let _lane = lane();
    let tasty = common::shared();
    const MISSING: u64 = 999_999;

    // (부류/키, 메서드, 그 키를 뺀 나머지 params). 메서드는 그 키를 실제로 받는
    // **호스트 예약 prefix** 로 고른다 — plugin prefix 는 애초에 이 판정을 안 지난다.
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

    // ── 방향 ①: 없는 대상은 부류를 가리지 않고 거절된다.
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
            "{method} 가 없는 메서드다 — 이 줄은 판정이 아니라 미측정이다. 이름을 고쳐라: {resp}"
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

    // ── 방향 ②: 살아 있는 대상에는 이 검사가 안 걸린다.
    // 거절만 세면 "전부 거절" 과 구별이 안 된다. 여기서 다른 이유의 실패는 허용한다
    // (자기 surface 를 닫으려 한다든지) — 보는 것은 **소유 검사가 걸렸는가** 하나다.
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

/// debug 표면 중 **창을 안 읽는 것**은 두 조합의 debug 빌드에서 같이 답한다.
///
/// 이 다섯이 읽는 것은 `App` 의 `lua_engine` / `plugin_manager` 뿐이다. 그런데 dispatch 가
/// gui 라우터의 debug step 에만 있어서 헤드리스에서는 `-32601` 이었다 — 창을 안 보는데
/// 자리가 없어서 사라진 것이라, 에이전트가 자기 작업을 검증하는 표면(event bus 관측 ·
/// 확장 훅 발화 · Lua 주입)이 헤드리스에서만 없는 형태였다.
///
/// **여는 것은 "헤드리스 debug 빌드에서도 답한다" 이지 "release 에 노출한다" 가 아니다.**
/// release 격리는 `DEBUG_METHODS` 가 debug 빌드에서만 비지 않는 것과, 이 arm 들이
/// `#[cfg(debug_assertions)]` 안에 있는 것 둘로 유지된다. 이 테스트 자체는 debug 로
/// 돌므로 그 격리를 여기서 못 본다 — release 격리는
/// `tests/ipc_release_table_excludes_input_reproduction.rs` 와 실행 대조가 본다.
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
        // 조회만이다. 같은 갈래의 `debug.popup.open` 은 아래 음성 대조에 있다 —
        // 헤드리스에 닫는 경로가 없어 여는 것만 열면 안 된다.
        "debug.popup.list",
        // 같은 형태. 무대 표를 gui 무관 메타와 그리기 함수로 가른 뒤 조회만 연다.
        "debug.fullscreen.list",
    ] {
        let resp = tasty.call_raw(method, json!({}));
        let code = resp["error"]["code"].as_i64();
        assert_ne!(
            code,
            Some(-32601),
            "`{method}` 가 읽는 것은 `App` 의 lua_engine/plugin_manager, 또는 gui 무관 정적 \
             표뿐이다 — 두 조합의 debug 빌드에서 라우팅돼야 한다: {resp}"
        );
        // 위와 같은 이유. 표에 남은 채 arm 만 게이트 뒤로 들어가면 `-32017` 이 된다.
        assert_ne!(
            code,
            Some(-32017),
            "`{method}` 의 arm 이 이 조합에서 사라졌다: {resp}"
        );
    }

    // 음성 대조. 헤드리스에 대응물이 없어 **없는 것이 정답**인 것들이다. 같은 회차에서
    // 이것을 못 박지 않으면 위 루프만 남아 "debug step 을 통째로 옮겨도 통과" 가 된다.
    //
    // 사유가 둘로 갈린다 — 한 갈래 안에서도 갈린다는 것이 이 대조의 요점이다:
    //   `debug.tool.list`  창·egui 입력 큐를 읽는다. 헤드리스에 그 상태 자체가 없다.
    //   `debug.popup.open` 매니저만 읽어 **답은 정의되지만** 헤드리스엔 그 인스턴스를
    //                      닫는 경로가 하나도 없다(debug close 도, plugin 자신의
    //                      release `popup.close` 도 gui 게이트 안이다). 여는 것만
    //                      열면 닫을 수 없는 상태가 남는다.
    //   `debug.fullscreen.open` 같은 갈래의 `list` 는 위에서 답하는데 이쪽은 아니다 —
    //                      `pick_debug_window` 로 창을 지목한다. 무대는 창 단위다.
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
            "헤드리스엔 arm 이 없는 것이 정답이다 — 이름은 표에 있으므로 오타와 \
             같은 코드로 답하면 안 된다: {resp}"
        );
    }
}

/// 한 회차가 큐를 다 못 비워도 요청이 삼켜지지 않는다.
///
/// `tcp_ipc_server::DRAIN_BUDGET_PER_ROUND`(= `MAX_CONCURRENT_CONNECTIONS`) 가 한 회차에 집어 드는 명령 수를
/// 막는다. 남은 것이 다음 회차에 다시 불리는 근거는 headless 에서는 **게이트가 wake 를
/// 하나로 접고, 잘린 회차가 입장 장부를 보고 루프를 다시 깨우는 것**(ADR-0465)이고, gui
/// 에서는 잘린 회차의 재깨움(ADR-0413)이다. 그 성질이 깨지면 예산을 넘긴 요청이 응답 없이
/// 남는다. 클라이언트가 응답을 기다리며 블록하므로 그 사고는 **행이 아니라 멈춤**
/// 으로 나타난다.
///
/// ★ **이 시험은 기본 예산에서는 회차를 자르지 않는다** — 동시 연결 16 개는 수 예산(256)
/// 아래이고, `workspace.list` 16 개는 시간 예산(16 ms) 안에 끝난다. 수 예산은 낮춰 볼 수도
/// 없다(`INJECTED_DEPTH_LIMIT <= DRAIN_BUDGET_PER_ROUND` 컴파일 단언). 그래서 이 시험은
/// 재깨움 갈래를 지나지 않는다 — 그 갈래는 아래
/// `concurrent_requests_are_all_answered_when_every_round_is_cut` 가 시간 예산을 줄여 잰다.
#[test]
fn concurrent_requests_are_all_answered() {
    let _lane = lane();
    let tasty = common::shared();
    const CLIENTS: usize = 16;

    // 연결마다 스레드 하나 — `call` 이 자기 TcpStream 을 열고 응답까지 블록하므로
    // 이 순간 큐에 최대 CLIENTS 개의 명령이 동시에 들어간다. 한 요청씩 순서대로
    // 보내면 큐 깊이가 늘 1 이라 회차 예산을 건드릴 수 없다.
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

/// 회차가 **매번** 잘려도 동시 요청이 전부 답을 받는다 — 잘린 회차가 루프를 다시 깨우는
/// 갈래(headless ADR-0465 · gui ADR-0413)를 실제로 지나는 시험이다.
///
/// 위 시험은 기본 예산에서 회차를 안 자르므로 그 갈래를 안 지난다. 여기서는 debug 전용
/// `TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS=0` 으로 따로 띄워 회차마다 첫 명령 하나만 꺼내게
/// 한다(`src/app/ipc_round.rs` 의 `round_time_budget`). 그러면 동시에 든 나머지는 전부 재깨움
/// 으로만 진척한다 — headless 는 채널에 wake 가 하나뿐이라 재깨움이 빠지면 명령이 큐에 선다.
///
/// 응답을 기다리는 상한을 짧게 둔다. 선 명령은 영원히 안 오므로, 상한이 길면 이 시험이
/// 실패가 아니라 긴 대기로 보인다.
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
        "잘린 회차 뒤에 남은 요청이 답을 못 받았다 — 재깨움이 안 섰다: {unanswered:?}"
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
        "회차가 한 번도 시간 예산에서 안 잘렸다 — 예산 주입이 안 먹어 재깨움 갈래를 안 지났다: {}",
        rounds["queue_dispatch"]
    );
}
