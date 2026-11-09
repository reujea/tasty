//! agent-session 대화 탭의 GUI 시험(macOS). 번들 plugin 을 띄워야 해서 `gui_tests` 와 다른 바이너리다 — 번들 사용은
//! 바이너리 단위로 정한다(`spawn_diag::SUITES_THAT_CALL_BUNDLED_PLUGINS`).
//! 격리된 디스플레이에서 `cargo test --test agent_session_gui -- --ignored --test-threads=1 a_chat_tab a_surface_divider_drags a_right_click a_project_skill an_agent_log the_memory_saver`
//! 로 실행한다. 분할선 시험은 번들 markdown · mdedit 의 webview 옆에서 분할선을 실제로 끈다. 메모리 절약 시험은
//! 설정을 켠 `config.toml` 로 띄워 가려진 대화 탭의 WebView 가 내려가고 다시 만들어지는지 본다.
//!
//! agent-session 대화 탭(macOS) — 가짜 claude(stream-json)로 멀티턴, 입력칸 실입력, 권한 카드 실클릭을 돌린다.
//!
//! 대화 문서는 native WebView 라 egui · 터미널 주입이 닿지 않는다. 그래서 OS 수준 입력을 쓴다. 문서 아래쪽의
//! 높이(상태줄 줄 수 · webview 크기 반영 시점)가 실행마다 달라 고정 좌표로는 못 누른다 — surface 왼쪽 열을 아래에서
//! 위로 훑으며 누르고, plugin 쪽 결과(턴이 붙었나 · 허용을 받았나)로 맞는 자리를 가린다. 그 열에는 보내기 · 비우기 ·
//! 거부처럼 상태를 바꾸는 단추가 없다(모두 오른쪽이다). 끝내 결과가 없으면 실패한다.

#![cfg(target_os = "macos")]
// 이유: 공용 GUI 도우미(gui_common) 가운데 이 바이너리가 쓰는 것은 일부뿐이다.
#![allow(dead_code)]

mod gui_common;

use enigo::Key;
use serde_json::{Value, json};
use std::time::Duration;

/// 받은 말에 따라 답한다 — 표 · 권한 질문 · 나머지는 턴 번호를 붙여 되돌린다.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
sid=""
while [ $# -gt 0 ]; do case "$1" in --session-id|--resume) sid="$2"; shift ;; esac; shift; done
n=0
turn() {
  n=$((n+1))
  text=$(printf '%s' "$1" | sed -n 's/.*"text":"\([^"]*\)".*/\1/p')
  printf '{"type":"system","subtype":"init","session_id":"%s","model":"fake-model"}\n' "${FAKE_INIT_SID:-$sid}"
  case "$text" in
*perm*)
  printf '{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls -la"}}}\n'
  IFS= read -r ans
  case "$ans" in *'"behavior":"allow"'*) out="allowed (turn $n)" ;; *) out="denied (turn $n)" ;; esac ;;
*tool*)
  # 에이전트 내역 — 대화 기록(경계 앞 단계 · 경계 · 성공 · 실패 단계)과 하위 에이전트 기록을 남기고, 같은 단계와
  # 백그라운드 작업을 원문 줄로도 낸다.
  d="$HOME/.claude/projects/p"; mkdir -p "$d/$sid/subagents"; f="$d/$sid.jsonl"
  u='{"type":"assistant","timestamp":"2026-11-05T00:00:03Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"echo probe"}}]}}'
  r='{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"probe"}]}}'
  printf '%s\n' '{"type":"assistant","timestamp":"2026-11-05T00:00:00Z","message":{"content":[{"type":"tool_use","id":"t0","name":"Bash","input":{"command":"before compact"}}]}}' >> "$f"
  printf '%s\n' '{"type":"system","subtype":"compact_boundary","timestamp":"2026-11-05T00:00:01Z"}' >> "$f"
  printf '%s\n%s\n' "$u" "$r" >> "$f"
  printf '%s\n' '{"type":"assistant","timestamp":"2026-11-05T00:00:04Z","message":{"content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"/nope"}}]}}' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","is_error":true,"content":"missing"}]}}' >> "$f"
  printf '%s\n' '{"type":"user","timestamp":"2026-11-05T00:00:02Z","message":{"content":"look around"}}' > "$d/$sid/subagents/agent-probe.jsonl"
  printf '%s' '{"description":"probe helper","agentType":"Explore","finished":true}' > "$d/$sid/subagents/agent-probe.meta.json"
  printf '%s\n%s\n' "$u" "$r"
  printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"bg1","task_type":"local_bash","description":"probe server"}'
  out="tools done (turn $n)" ;;
*) out="echo: $text (turn $n)" ;;
  esac
  printf '{"type":"assistant","message":{"content":[{"type":"text","text":"%s"}]}}\n' "$out"
  printf '{"type":"result","subtype":"success","is_error":false,"duration_ms":1200,"total_cost_usd":0.01,"usage":{"input_tokens":1500,"output_tokens":42}}\n'
}
while IFS= read -r line; do case "$line" in *'"type":"user"'*) turn "$line" ;; esac; done
"#;

/// 입력칸 글자 열 · 권한 카드 Allow 열 — surface 왼쪽에서 잰 논리 pt. 높이는 훑어서 찾는다.
const INPUT_X: f64 = 106.0;
const ALLOW_X: f64 = 63.0;

fn install_fake_profile(inst: &gui_common::GuiTestInstance) {
    use std::os::unix::fs::PermissionsExt;
    let home = inst.home();
    let script = home.join("fake-claude");
    std::fs::write(&script, FAKE_CLAUDE).expect("가짜 claude 쓰기");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("실행권한");
    let profiles = home.join("plugin-data/com.tasty.agent-session/profiles");
    std::fs::create_dir_all(&profiles).expect("프로필 폴더");
    let body =
        json!({ "command": script.display().to_string(), "cwd": home.display().to_string() });
    std::fs::write(profiles.join("fake.json"), body.to_string()).expect("프로필 쓰기");
    // 대화 id 를 받은 id 와 다르게 내는 프로필 — 이어받기 · `/clear` 뒤처럼 기록 id ≠ 대화 id 가 된다.
    let mut fork = body;
    fork["env"] = json!({ "FAKE_INIT_SID": FORK_CONVERSATION_ID });
    std::fs::write(profiles.join("fork.json"), fork.to_string()).expect("프로필 쓰기");
}

/// `fork` 프로필의 가짜 claude 가 init 에 내는 대화 id.
const FORK_CONVERSATION_ID: &str = "c0ffee00-b107-4000-8000-000000000107";

/// surface 왼쪽 아래 기준 오프셋(논리 pt)을 화면 좌표로.
fn surface_point(inst: &gui_common::GuiTestInstance, sid: u64, at: (f64, f64)) -> (f64, f64) {
    let windows = inst.call("debug.window.list", json!({}));
    let main = windows["windows"]
        .as_array()
        .and_then(|w| w.iter().find(|w| w["kind"] == "main"))
        .cloned()
        .unwrap_or_else(|| panic!("메인 창이 없다: {windows}"));
    let scale = main["scale_factor"].as_f64().expect("scale_factor");
    let (px, py) = (
        main["inner_position"]["x"]
            .as_f64()
            .expect("inner_position.x"),
        main["inner_position"]["y"]
            .as_f64()
            .expect("inner_position.y"),
    );
    let r = inst.call("debug.surface_rect", json!({ "surface_id": sid }));
    let rect = &r["rect"];
    let (x, y, h) = (
        rect["x"].as_f64().expect("rect.x"),
        rect["y"].as_f64().expect("rect.y"),
        rect["height"].as_f64().expect("rect.height"),
    );
    ((px + x) / scale + at.0, (py + y + h) / scale - at.1)
}

/// 기다리는 동안 그 답이 오면 true.
fn answered(inst: &gui_common::GuiTestInstance, sid: u64, want: &str, within: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < within {
        if last_answer(inst, sid).contains(want) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn last_answer(inst: &gui_common::GuiTestInstance, sid: u64) -> String {
    inst.call("agent_session.output", json!({ "surface_id": sid }))
        .to_string()
}

#[test]
#[ignore]
fn a_chat_tab_runs_turns_typed_input_and_a_permission_click() {
    let mut inst = gui_common::GuiTestInstance::spawn();
    install_fake_profile(&inst);
    let pane = inst.first_pane_id();
    let created = inst.call(
        "tab.create",
        json!({ "pane_id": pane, "type": "agent_session" }),
    );
    let sid = created["surface_id"]
        .as_u64()
        .unwrap_or_else(|| panic!("대화 탭을 못 만들었다: {created}"));
    inst.call("debug.switch_tab", json!({ "index": 1 }));
    let started = inst.call(
        "agent_session.start",
        json!({ "surface_id": sid, "agent": "fake" }),
    );
    assert!(
        started["session_id"].is_string(),
        "세션을 못 열었다: {started}"
    );

    // 멀티턴 — 턴 번호가 이어진다.
    for (i, m) in ["hello", "second"].iter().enumerate() {
        let out: Value = inst.call(
            "agent_session.tell",
            json!({ "surface_id": sid, "text": m }),
        );
        let want = format!("echo: {m} (turn {})", i + 1);
        assert!(out.to_string().contains(&want), "{want} 이 아니다: {out}");
    }

    // 입력칸을 눌러 치고 Enter — 사용자가 보내는 길(webview navigation)이다. 아래에서 위로 훑는다.
    let mut input_y = None;
    for up in (2..=24).map(|k| f64::from(k) * 10.0) {
        let (x, y) = surface_point(&inst, sid, (INPUT_X, up));
        inst.click_screen_points(x, y);
        inst.type_text("typed in the box");
        inst.press_key(Key::Return);
        // 답이 올 때까지 기다린 뒤에 다음 자리로 넘어간다 — 서두르면 같은 말을 두 번 보낸다.
        if answered(&inst, sid, "echo: typed in the box", Duration::from_secs(4)) {
            input_y = Some(up);
            break;
        }
    }
    let input_y = input_y.expect("입력칸을 눌러 보낸 턴이 붙지 않았다 — 입력칸 자리를 못 찾았다");

    // 권한 질문 — 카드의 Allow 를 실제로 누르면 에이전트가 허용을 받는다. 카드는 입력칸보다 위에 있다.
    let (x, y) = surface_point(&inst, sid, (INPUT_X, input_y));
    inst.click_screen_points(x, y);
    inst.type_text("perm");
    inst.press_key(Key::Return);
    std::thread::sleep(Duration::from_secs(2));
    let allowed = (4..=36).map(|k| input_y + f64::from(k) * 10.0).any(|up| {
        let (x, y) = surface_point(&inst, sid, (ALLOW_X, up));
        inst.click_screen_points(x, y);
        answered(&inst, sid, "allowed (turn", Duration::from_millis(1500))
    });
    assert!(
        allowed,
        "Allow 를 눌러도 허용을 받지 못했다: {}",
        last_answer(&inst, sid)
    );
}

/// 기록 id ≠ 대화 id 인 세션 — init 을 받은 뒤에도 surface 로 `status` · `tell` 이 그 세션에 닿는다(B107).
/// 종전에는 init 의 대화 id 가 잇기 meta 를 덮어 "그런 세션이 없다" 가 났다.
#[test]
#[ignore]
fn a_chat_tab_keeps_its_link_when_the_conversation_id_differs() {
    let inst = gui_common::GuiTestInstance::spawn();
    install_fake_profile(&inst);
    let pane = inst.first_pane_id();
    let created = inst.call(
        "tab.create",
        json!({ "pane_id": pane, "type": "agent_session" }),
    );
    let sid = created["surface_id"].as_u64().expect("대화 탭");
    inst.call("debug.switch_tab", json!({ "index": 1 }));
    let started = inst.call(
        "agent_session.start",
        json!({ "surface_id": sid, "agent": "fork" }),
    );
    let record = started["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("세션을 못 열었다: {started}"))
        .to_string();
    assert_ne!(record, FORK_CONVERSATION_ID);
    let out = inst.call(
        "agent_session.tell",
        json!({ "surface_id": sid, "text": "hello" }),
    );
    assert!(out.to_string().contains("echo: hello (turn 1)"), "{out}");
    // init 은 원문으로 곧바로, 세션 행으로는 활동 폴링(2초)마다 온다 — 둘 다 지나가게 기다린다.
    std::thread::sleep(Duration::from_secs(5));
    let st = inst.call("agent_session.status", json!({ "surface_id": sid }));
    assert_eq!(st["session_id"], json!(record), "잇기가 바뀌었다: {st}");
    assert_eq!(
        st["status"]["agent_session_id"],
        json!(FORK_CONVERSATION_ID),
        "두 id 가 갈리지 않았다 — 시험이 아무것도 안 봤다: {st}"
    );
    let out = inst.call(
        "agent_session.tell",
        json!({ "surface_id": sid, "text": "second" }),
    );
    assert!(out.to_string().contains("echo: second (turn 2)"), "{out}");
}

/// 메인 창 안쪽 크기(물리 px)와 위치·배율.
fn main_window(inst: &gui_common::GuiTestInstance) -> (f64, f64, f64, f64, f64) {
    let windows = inst.call("debug.window.list", json!({}));
    let w = windows["windows"]
        .as_array()
        .and_then(|w| w.iter().find(|w| w["kind"] == "main"))
        .cloned()
        .unwrap_or_else(|| panic!("메인 창이 없다: {windows}"));
    let f = |v: &Value| v.as_f64().unwrap_or_else(|| panic!("숫자가 아니다: {w}"));
    (
        f(&w["inner_position"]["x"]),
        f(&w["inner_position"]["y"]),
        f(&w["inner_size"]["width"]),
        f(&w["inner_size"]["height"]),
        f(&w["scale_factor"]),
    )
}

fn surface_rect(inst: &gui_common::GuiTestInstance, sid: u64) -> (f64, f64, f64, f64) {
    let r = inst.call("debug.surface_rect", json!({ "surface_id": sid }));
    let f = |k: &str| {
        r["rect"][k]
            .as_f64()
            .unwrap_or_else(|| panic!("rect.{k}: {r}"))
    };
    (f("x"), f("y"), f("width"), f("height"))
}

/// 분할선 hit 띠는 webview 옆에서도 host 가 받는다(macOS 는 `KeyWebView` 의 `hitTest:` 가 띠를 뺀다).
/// 터미널 옆에 각 종류를 세로로 나눠 두고, 틈 가운데와 오른쪽(webview 쪽) 3pt 에서 분할선을 실제로 끈다.
#[test]
#[ignore]
fn a_surface_divider_drags_beside_every_webview_kind() {
    let mut inst = gui_common::GuiTestInstance::spawn();
    let doc = inst.home().join("divider-probe.md");
    std::fs::write(&doc, "# probe\n\nbody\n").expect("문서 쓰기");
    let left = inst.first_surface_id();
    let mut report = Vec::new();
    for kind in ["terminal", "markdown", "mdedit"] {
        let res = inst.call(
            "split",
            json!({ "level": "surface", "direction": "vertical", "target_surface": left,
                    "type": kind, "file": doc.display().to_string() }),
        );
        let right = res["new_surface_id"]
            .as_u64()
            .unwrap_or_else(|| panic!("{kind} 분할 실패: {res}"));
        // webview 가 자리를 잡을 때까지 기다린다.
        std::thread::sleep(Duration::from_secs(3));
        for (spot, off) in [("gap", 0.0), ("webview+3pt", 3.0)] {
            let (x, y, _, _, s) = main_window(&inst);
            let (lx, ly, lw, lh) = surface_rect(&inst, left);
            let (rx, _, _, _) = surface_rect(&inst, right);
            let px = (x + (lx + lw + rx) / 2.0) / s + off;
            let py = (y + ly + lh / 2.0) / s;
            inst.drag_screen_points((px, py), (px + 80.0, py));
            let (_, _, moved, _) = surface_rect(&inst, left);
            report.push((kind, spot, moved - lw));
            // 다음 자리를 같은 폭에서 재도록 되돌린다.
            let (rx2, _, _, _) = surface_rect(&inst, right);
            let back = (x + (lx + moved + rx2) / 2.0) / s;
            inst.drag_screen_points((back, py), (back - (moved - lw) / s, py));
        }
        inst.call("surface.close", json!({ "surface_id": right }));
        std::thread::sleep(Duration::from_secs(1));
    }
    for (kind, spot, moved) in &report {
        assert!(
            *moved > 50.0,
            "{kind} 옆 분할선을 {spot} 에서 끌어도 움직이지 않는다: {report:?}"
        );
    }
}

/// 페이지 우클릭은 WebKit 메뉴 대신 host 메뉴(`WebviewSurface`)를 연다 — 페이지 스크립트 → fragment → plugin →
/// `webview.context_menu` → host 를 모두 지난다. 시험 인스턴스는 native 메뉴를 띄우지 않고 요청을 잡아 둔다
/// (`TASTY_DEBUG_SUPPRESS_NATIVE_MENU`). 입력칸 안이나 고른 글 위의 우클릭은 기본 메뉴로 남으므로, 시작 화면의
/// 몇 자리를 차례로 눌러 입력칸이 아닌 자리에서 메뉴가 오는지 본다.
#[test]
#[ignore]
fn a_right_click_on_the_page_opens_the_host_menu() {
    let mut inst = gui_common::GuiTestInstance::spawn();
    let pane = inst.first_pane_id();
    let created = inst.call(
        "tab.create",
        json!({ "pane_id": pane, "type": "agent_session" }),
    );
    let sid = created["surface_id"]
        .as_u64()
        .unwrap_or_else(|| panic!("대화 탭을 못 만들었다: {created}"));
    inst.call("debug.switch_tab", json!({ "index": 1 }));
    std::thread::sleep(Duration::from_secs(3));
    let mut seen = Value::Null;
    let mut spot = (0.0, 0.0);
    'spots: for (fx, dy) in [(0.95, 12.0), (0.95, 60.0), (0.5, 12.0), (0.8, 200.0)] {
        spot = (fx, dy);
        let (x, y, _, _, s) = main_window(&inst);
        let (sx, sy, sw, _) = surface_rect(&inst, sid);
        inst.right_click_screen_points((x + sx + sw * fx) / s, (y + sy) / s + dy);
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            seen = inst.debug_pending_menu();
            if seen["kind"] == "WebviewSurface" {
                break 'spots;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    assert_eq!(
        seen["kind"], "WebviewSurface",
        "host 메뉴가 열리지 않았다: {seen}"
    );
    assert_eq!(seen["surface_id"].as_u64(), Some(sid), "{seen}");

    // 같은 자리에서 "터미널 ID 복사" 를 고른다 — 클립보드에 surface id 가 들어간다. 호스트 토스트는 겹친
    // webview 를 숨기므로 이 메뉴는 토스트 없이 plugin 에 결과를 넘긴다(`webview_menu.rs`).
    let armed = inst.call(
        "debug.native_menu.answer",
        json!({ "menu": "WebviewSurface", "item": 1 }),
    );
    assert_eq!(armed["armed"], json!(true), "응답을 걸지 못했다: {armed}");
    std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::null())
        .status()
        .expect("클립보드 비우기");
    let (x, y, _, _, s) = main_window(&inst);
    let (sx, sy, sw, _) = surface_rect(&inst, sid);
    inst.right_click_screen_points((x + sx + sw * spot.0) / s, (y + sy) / s + spot.1);
    let want = sid.to_string();
    let start = std::time::Instant::now();
    let mut got = String::new();
    while start.elapsed() < Duration::from_secs(5) {
        let out = std::process::Command::new("pbpaste")
            .output()
            .expect("pbpaste");
        got = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if got == want {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(got, want, "고른 뒤 클립보드에 surface id 가 없다");
}

/// 프로젝트 스킬이 `/` 목록에 있다 — 작업 폴더의 `.claude/skills` 를 연결할 때 훑는다(첫 말 전). `/probe` 를 치고
/// Tab 으로 채워 보내면 가짜 claude 가 받은 글을 되돌린다. 목록에 없으면 Tab 이 채우지 않아 `/probe` 만 간다.
#[test]
#[ignore]
fn a_project_skill_is_offered_in_the_slash_menu() {
    let mut inst = gui_common::GuiTestInstance::spawn();
    install_fake_profile(&inst);
    let skill = inst.home().join(".claude/skills/probe-skill");
    std::fs::create_dir_all(&skill).expect("스킬 폴더");
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: probe-skill\ndescription: probe\n---\nSay probe.\n",
    )
    .expect("SKILL.md");
    let pane = inst.first_pane_id();
    let created = inst.call(
        "tab.create",
        json!({ "pane_id": pane, "type": "agent_session" }),
    );
    let sid = created["surface_id"]
        .as_u64()
        .unwrap_or_else(|| panic!("대화 탭을 못 만들었다: {created}"));
    inst.call("debug.switch_tab", json!({ "index": 1 }));
    let started = inst.call(
        "agent_session.start",
        json!({ "surface_id": sid, "agent": "fake" }),
    );
    assert!(
        started["session_id"].is_string(),
        "세션을 못 열었다: {started}"
    );
    std::thread::sleep(Duration::from_secs(3));
    let mut input_y = None;
    for up in (2..=24).map(|k| f64::from(k) * 10.0) {
        let (x, y) = surface_point(&inst, sid, (INPUT_X, up));
        inst.click_screen_points(x, y);
        inst.type_text("/probe");
        inst.press_key(Key::Tab);
        inst.press_key(Key::Return);
        if answered(&inst, sid, "echo: /probe", Duration::from_secs(4)) {
            input_y = Some(up);
            break;
        }
    }
    let input_y =
        input_y.unwrap_or_else(|| panic!("입력칸에서 보낸 턴이 없다: {}", last_answer(&inst, sid)));
    let got = last_answer(&inst, sid);
    assert!(
        got.contains("echo: /probe-skill"),
        "Tab 이 스킬 이름으로 채우지 않았다 — 목록에 스킬이 없다: {got}"
    );

    // 연결된 채로 스킬을 더하고 `/reload-skills` 를 보내면 문서를 다시 싣지 않고 목록이 바뀐다(작업 스레드 →
    // `{type:"commands"}` 메시지). 새 이름은 앞 스킬과 접두가 달라 Tab 이 그것만 채운다.
    let second = inst.home().join(".claude/skills/second-probe");
    std::fs::create_dir_all(&second).expect("스킬 폴더");
    std::fs::write(
        second.join("SKILL.md"),
        "---\nname: second-probe\ndescription: added later\n---\nSay second.\n",
    )
    .expect("SKILL.md");
    let (x, y) = surface_point(&inst, sid, (INPUT_X, input_y));
    inst.click_screen_points(x, y);
    inst.type_text("/reload-skills");
    inst.press_key(Key::Return);
    assert!(
        answered(&inst, sid, "echo: /reload-skills", Duration::from_secs(6)),
        "/reload-skills 턴이 없다: {}",
        last_answer(&inst, sid)
    );
    std::thread::sleep(Duration::from_secs(1));
    inst.click_screen_points(x, y);
    inst.type_text("/second");
    inst.press_key(Key::Tab);
    inst.press_key(Key::Return);
    assert!(
        answered(&inst, sid, "echo: /second-probe", Duration::from_secs(6)),
        "다시 훑은 목록에 새 스킬이 없다: {}",
        last_answer(&inst, sid)
    );
}

/// 에이전트 내역 — 도구를 부른 턴 뒤 내역에 현재 에이전트(경계 뒤 단계만 · 성패) · 하위 에이전트 · 백그라운드 작업
/// 카드가 있고, 새 하위 에이전트가 패널을 연다. 패널 값은 plugin 이 페이지에 싣는 것과 같다(`agent_session.agent_log`).
#[test]
#[ignore]
fn an_agent_log_lists_main_steps_subagents_and_background_tasks() {
    let inst = gui_common::GuiTestInstance::spawn();
    install_fake_profile(&inst);
    let pane = inst.first_pane_id();
    let created = inst.call(
        "tab.create",
        json!({ "pane_id": pane, "type": "agent_session" }),
    );
    let sid = created["surface_id"]
        .as_u64()
        .unwrap_or_else(|| panic!("대화 탭을 못 만들었다: {created}"));
    inst.call("debug.switch_tab", json!({ "index": 1 }));
    let started = inst.call(
        "agent_session.start",
        json!({ "surface_id": sid, "agent": "fake" }),
    );
    assert!(
        started["session_id"].is_string(),
        "세션을 못 열었다: {started}"
    );
    let out = inst.call(
        "agent_session.tell",
        json!({ "surface_id": sid, "text": "tool please" }),
    );
    assert!(
        out.to_string().contains("tools done"),
        "도구 턴이 안 끝났다: {out}"
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let log = loop {
        let log = inst.call("agent_session.agent_log", json!({ "surface_id": sid }));
        let n = log["panel"]["cards"].as_array().map_or(0, Vec::len);
        if n >= 3 || std::time::Instant::now() > deadline {
            break log;
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    let panel = &log["panel"];
    let title = panel["title"].as_str().unwrap_or_default();
    assert!(
        ["에이전트 내역", "Agent log", "エージェント履歴"].contains(&title),
        "패널 제목이 에이전트 내역이 아니다: {log}"
    );
    assert_eq!(
        panel["hidden"], false,
        "새 하위 에이전트가 패널을 안 열었다: {log}"
    );
    let cards = panel["cards"].as_array().expect("카드");
    let keys: Vec<&str> = cards.iter().filter_map(|c| c["k"].as_str()).collect();
    assert_eq!(
        keys,
        ["main", "b:bg1", "s:agent-probe"],
        "카드 종류 · 순서가 다르다: {log}"
    );
    let steps: Vec<(&str, &str, &str)> = cards[0]["steps"]
        .as_array()
        .expect("단계")
        .iter()
        .map(|s| {
            (
                s["tool"].as_str().unwrap_or_default(),
                s["summary"].as_str().unwrap_or_default(),
                s["cls"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        steps,
        [("Bash", "echo probe", "ok"), ("Read", "/nope", "err")],
        "경계 뒤 단계 · 성패가 아니다: {log}"
    );
}

/// 그 탭의 문서를 WebView 생성 통지로 다시 실은 횟수 — plugin 로그에서 센다. 메모리 절약이 WebView 를 없앴다가
/// 다시 만들었는지는 이것으로 본다. 렌더링 프로세스 수는 WebKit 이 프로세스를 남겨 둘 수 있어 증거로 안 쓴다.
fn reloads(inst: &gui_common::GuiTestInstance, sid: u64) -> usize {
    let log = inst.home().join("plugins-logs/com.tasty.agent-session.log");
    let want = format!("surface={sid}: WebView 가 생겨 문서를 다시 싣는다");
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .matches(&want)
        .count()
}

/// 다시 실은 횟수가 그 수가 될 때까지 기다린다.
fn reloads_become(inst: &gui_common::GuiTestInstance, sid: u64, want: usize) -> usize {
    let start = std::time::Instant::now();
    loop {
        let n = reloads(inst, sid);
        if n == want || start.elapsed() >= Duration::from_secs(10) {
            return n;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// 이 기기의 WebKit 렌더링 프로세스 수.
fn web_content_count() -> usize {
    let out = std::process::Command::new("ps")
        .args(["-axo", "command"])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains("com.apple.WebKit.WebContent"))
        .count()
}

/// 렌더링 프로세스가 그 수가 될 때까지 기다린다 — 없앤 WebView 의 프로세스는 조금 늦게 내려간다.
fn web_content_becomes(want: usize) -> usize {
    let start = std::time::Instant::now();
    loop {
        let n = web_content_count();
        if n == want || start.elapsed() >= Duration::from_secs(10) {
            return n;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// 메모리 절약(`release_hidden_webview`) — 가려진 대화 탭의 WebView 를 없애고, 다시 보이면 새로 만들어 문서를
/// 다시 싣고, 쓰던 글이 입력칸에 돌아온다(돌아온 글을 Enter 로 보내 답으로 확인한다). 새로 만든 WebView 의
/// 입력칸은 비어 시작하므로 그 글이 가면 plugin 이 되살린 것이다.
///
/// 프로세스는 다른 대화 탭이 보이는 동안 센다 — 보이는 WebView 가 하나도 없으면 WebKit 이 마지막 렌더링
/// 프로세스를 다음 WebView 가 생길 때까지 남겨 두므로(VM 실측 2026-11-04: 40초 이상) 그때의 수는 증거가 아니다.
#[test]
#[ignore]
fn the_memory_saver_unloads_hidden_chat_tabs_and_restores_the_draft() {
    let mut inst = gui_common::GuiTestInstance::spawn_with_config(Some(
        "[plugin_settings.\"com.tasty.agent-session\"]\nrelease_hidden_webview = true\n",
    ));
    install_fake_profile(&inst);
    let pane = inst.first_pane_id();
    let created = inst.call(
        "tab.create",
        json!({ "pane_id": pane, "type": "agent_session" }),
    );
    let sid = created["surface_id"].as_u64().expect("대화 탭");
    inst.call("debug.switch_tab", json!({ "index": 1 }));
    let started = inst.call(
        "agent_session.start",
        json!({ "surface_id": sid, "agent": "fake" }),
    );
    let session = started["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("세션을 못 열었다: {started}"))
        .to_string();
    assert_eq!(
        reloads_become(&inst, sid, 1),
        1,
        "처음 보인 탭의 WebView 생성 통지가 없다"
    );

    // 입력칸에 쓰고 보내지 않는다 — plugin 이 받아 파일에 둔 것으로 자리를 찾는다.
    let file = inst
        .home()
        .join("plugin-data/com.tasty.agent-session/view-state")
        .join(format!("{session}.json"));
    let mut input_y = None;
    for up in (2..=24).map(|k| f64::from(k) * 10.0) {
        let (x, y) = surface_point(&inst, sid, (INPUT_X, up));
        inst.click_screen_points(x, y);
        inst.type_text("draft-x");
        std::thread::sleep(Duration::from_millis(900));
        if std::fs::read_to_string(&file).is_ok_and(|s| s.contains("draft-x")) {
            input_y = Some(up);
            break;
        }
    }
    let input_y =
        input_y.expect("쓰던 글이 파일에 안 남았다 — 입력칸을 못 찾았거나 보내기가 꺼졌다");

    // 다른 대화 탭으로 — 가려진 탭의 WebView 를 없앤다. 그 탭의 WebView 가 생긴 뒤에 센다(생기기 전에는 가려진
    // 탭의 프로세스 하나뿐이라 없애지 않아도 1 이다). 보이는 탭 하나의 프로세스만 남고, 다시 실은 횟수는 그대로다.
    let other = inst.call(
        "tab.create",
        json!({ "pane_id": pane, "type": "agent_session" }),
    )["surface_id"]
        .as_u64()
        .expect("둘째 대화 탭");
    inst.call("debug.switch_tab", json!({ "index": 2 }));
    assert_eq!(
        reloads_become(&inst, other, 1),
        1,
        "둘째 탭의 WebView 가 안 생겼다"
    );
    assert_eq!(
        web_content_becomes(1),
        1,
        "가려진 대화 탭의 렌더링 프로세스가 남았다"
    );
    assert_eq!(reloads(&inst, sid), 1, "가려진 동안 문서를 다시 실었다");

    // 돌아오면 새로 만들어 다시 싣는다 — 없애지 않았다면 생성 통지가 다시 오지 않는다.
    inst.call("debug.switch_tab", json!({ "index": 1 }));
    assert_eq!(
        reloads_become(&inst, sid, 2),
        2,
        "다시 보인 탭의 WebView 를 새로 만들지 않았다 — 가려질 때 안 없앴다"
    );
    std::thread::sleep(Duration::from_secs(2));
    let (x, y) = surface_point(&inst, sid, (INPUT_X, input_y));
    inst.click_screen_points(x, y);
    inst.press_key(Key::End);
    inst.press_key(Key::Return);
    assert!(
        answered(&inst, sid, "echo: draft-x", Duration::from_secs(8)),
        "되살린 글이 안 갔다: {}",
        last_answer(&inst, sid)
    );
    std::thread::sleep(Duration::from_millis(500));
    assert!(!file.exists(), "보낸 뒤에도 쓰던 글 파일이 남았다");
}
