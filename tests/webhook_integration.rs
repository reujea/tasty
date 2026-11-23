//! 실제 서버에서 웹훅 등록·HTTP 응답·인증·수명·남용 차단·CLI 매핑·재시작 복원을 확인한다.
//! 등록과 정상 HTTP 확인을 먼저 하고 출처별 쿨다운을 만드는 남용 검사는 마지막에 둔다.
//! 훅 환경변수 검사는 HTTP를 쓰지 않아 쿨다운 뒤에도 실행할 수 있다. 재시작은 같은 홈과 URL을 유지한다.

// 시험 본문은 제품 코드의 let _ 사유 주석 정책에서 제외된다.
#![allow(clippy::let_underscore_must_use)]

mod marker_wait;
mod webhook_common;

use std::time::{Duration, Instant};

use marker_wait::wait_file_content;

use serde_json::{Value, json};
use webhook_common::{WebhookInstance, stdout_str};

/// 정상 시나리오의 소수 실패와 남용 검사 요청을 구별하도록 임계치를 둔다.
const ABUSE_THRESHOLD: u32 = 40;

/// 두 실패 종류를 따로 검사하도록 앞의 쿨다운이 끝난 뒤 다음 종류를 요청한다.
const ABUSE_COOLDOWN_SECS: u64 = 3;

/// 큰 데이터를 매번 보내지 않고 경계를 확인하도록 이 시험의 본문 상한을 작게 지정한다.
const MAX_BODY_BYTES: usize = 4096;

const HOOK_HANDLERS_TOML: &str = r#"
[[handler]]
id = "user/hookonly"
source = "hook"
priority = 50
[handler.action]
kind = "ipc_sequence"
calls = [{ method = "notification.create", params = { title = "HookOnly", body = "dispatched" } }]

[[handler]]
id = "user/shelltest"
source = "hook"
priority = 50
[handler.action]
kind = "shell_command"
command = "echo"
args = ["hi"]
"#;

/// 연속 알림은 본문이 결합될 수 있어 전체 일치 대신 표지가 포함됐는지 확인한다.
fn has_notification(inst: &WebhookInstance, needle: &str) -> bool {
    let notifs = inst.call("notification.list", json!({}));
    notifs
        .as_array()
        .map(|arr| {
            arr.iter().any(|n| {
                n.get("body")
                    .and_then(|b| b.as_str())
                    .map(|b| b.contains(needle))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn wait_notification(inst: &WebhookInstance, needle: &str, timeout: Duration) -> bool {
    tasty_utils::test_wait::holds_by_every(
        Instant::now() + timeout,
        Duration::from_millis(50),
        |_| has_notification(inst, needle),
    )
}

fn register_notify_webhook(inst: &WebhookInstance, params_extra: Value) -> (String, String) {
    let mut params = json!({
        "methods": ["POST"],
        "sequence": [{
            "method": "notification.create",
            "params": { "title": "WH", "body": "${body.message}" }
        }]
    });
    if let (Some(obj), Some(extra)) = (params.as_object_mut(), params_extra.as_object()) {
        for (k, v) in extra {
            obj.insert(k.clone(), v.clone());
        }
    }
    let resp = inst.call("webhook.register", params);
    let id = resp["id"]
        .as_str()
        .expect("register returns id")
        .to_string();
    let url = resp["url"]
        .as_str()
        .expect("register returns url")
        .to_string();
    assert!(url.contains(&id), "url must contain id");
    (id, url)
}

#[allow(clippy::cognitive_complexity)] // complexity-exempt: 등록·인증·쿨다운을 순서대로 검증하는 통합 시나리오다. 같은 서버 상태를 물려받아야 하므로 한 함수에 둔다.
fn integration_flow(inst: &WebhookInstance) {
    {
        let (id, _url) = register_notify_webhook(inst, json!({}));
        let info = inst.call("webhook.info", json!({ "id": id }));
        assert_eq!(info["id"].as_str(), Some(id.as_str()));
        assert_eq!(info["methods"], json!(["POST"]));

        let (code, body) = inst.post(&id, r#"{"message":"MARKER_HELLO"}"#);
        assert_eq!(code, 200, "POST to registered webhook must ACK 200");
        assert_eq!(body, "received", "ACK body must be the fixed string");

        assert!(
            wait_notification(inst, "MARKER_HELLO", Duration::from_secs(8)),
            "webhook IpcSequence must create a notification with the substituted body"
        );
        inst.call("webhook.unregister", json!({ "id": id }));
    }

    {
        let (id, _url) = register_notify_webhook(inst, json!({}));

        let payloads = [
            r#"{"message":"P1","method":"system.shutdown"}"#,
            r#"{"message":"P2","params":{"method":"tab.close"}}"#,
            r#"{"message":"P3","body":{"nested":"x"}}"#,
        ];
        for p in payloads {
            let (code, body) = inst.post(&id, p);
            assert_eq!(code, 200, "payload {p} must still ACK 200");
            assert_eq!(
                body, "received",
                "ACK body must never carry payload/内부 data"
            );
        }
        let sysinfo = inst.call("system.info", json!({}));
        assert!(
            sysinfo.get("version").is_some(),
            "tasty must remain alive — payload method must not execute"
        );
        assert!(wait_notification(inst, "P1", Duration::from_secs(8)));
        inst.call("webhook.unregister", json!({ "id": id }));
    }

    {
        let (id, _url) = register_notify_webhook(inst, json!({}));
        let (code, body) = inst.http("GET", &id, "");
        assert_eq!(code, 405, "wrong method must be 405");
        assert_eq!(body, "method not allowed");
        inst.call("webhook.unregister", json!({ "id": id }));
    }

    {
        let (id, _url) = register_notify_webhook(inst, json!({ "count": 2 }));
        assert_eq!(inst.post(&id, r#"{"message":"C1"}"#).0, 200);
        let info = inst.call("webhook.info", json!({ "id": id }));
        assert_eq!(info["lifetime"]["remaining"].as_u64(), Some(1));
        assert_eq!(inst.post(&id, r#"{"message":"C2"}"#).0, 200);
        assert_eq!(inst.post(&id, r#"{"message":"C3"}"#).0, 404);
        assert!(
            inst.call_raw("webhook.info", json!({ "id": id }))
                .get("error")
                .is_some(),
            "exhausted webhook must be gone from registry"
        );
    }

    {
        let (id, _url) = register_notify_webhook(inst, json!({ "ttl_secs": 1 }));
        std::thread::sleep(Duration::from_secs(2));
        let (code, body) = inst.post(&id, r#"{"message":"expired"}"#);
        assert_eq!(code, 410, "expired time-limited webhook must be 410 Gone");
        assert_eq!(body, "gone");
        assert_eq!(inst.post(&id, r#"{"message":"again"}"#).0, 404);
    }

    {
        let resp = inst.call_raw("webhook.register", json!({ "handler": "user/hookonly" }));
        let err = resp
            .get("error")
            .expect("hook-only handler must be rejected");
        let msg = err["message"].as_str().unwrap_or("").to_lowercase();
        assert!(
            msg.contains("source") || msg.contains("hook"),
            "rejection must cite source/hook gate, got: {msg}"
        );

        let resp = inst.call_raw("webhook.register", json!({ "handler": "user/shelltest" }));
        assert!(
            resp.get("error").is_some(),
            "shell handler must never be webhook-bindable"
        );
    }

    {
        let resp = inst.call(
            "webhook.register",
            json!({ "handler": "host/webhook-notify" }),
        );
        let id = resp["id"].as_str().expect("host handler binds").to_string();
        let (code, _body) = inst.post(&id, r#"{"message":"HOSTDEFAULT"}"#);
        assert_eq!(code, 200);
        assert!(wait_notification(
            inst,
            "HOSTDEFAULT",
            Duration::from_secs(8)
        ));
        inst.call("webhook.unregister", json!({ "id": id }));
    }

    {
        let (id, _url) = register_notify_webhook(
            inst,
            json!({ "auth": { "location": "query", "key": "tok", "token": "s3cr3t" } }),
        );
        let (code, body) = inst.post(&id, r#"{"message":"NOAUTH"}"#);
        assert_eq!(code, 401, "missing token must be 401");
        assert_eq!(body, "unauthorized");
        assert!(
            !has_notification(inst, "NOAUTH"),
            "unauthorized must not execute"
        );

        let path = format!("{id}?tok=s3cr3t");
        let (code, _body) = inst.post(&path, r#"{"message":"AUTHED"}"#);
        assert_eq!(code, 200, "correct token must pass");
        assert!(wait_notification(inst, "AUTHED", Duration::from_secs(8)));

        let info = inst.call("webhook.info", json!({ "id": id }));
        let info_str = serde_json::to_string(&info).unwrap();
        assert!(
            !info_str.contains("s3cr3t"),
            "token must never leak in info"
        );
        inst.call("webhook.unregister", json!({ "id": id }));
    }

    {
        let (id, _url) = register_notify_webhook(inst, json!({}));
        assert_eq!(inst.post(&id, r#"{"message":"live"}"#).0, 200);
        let removed = inst.call("webhook.unregister", json!({ "id": id }));
        assert_eq!(removed["unregistered"].as_bool(), Some(true));
        assert_eq!(
            inst.post(&id, r#"{"message":"dead"}"#).0,
            404,
            "unregistered path must 404"
        );
    }

    {
        let out = inst.cli(&[
            "webhook",
            "register",
            "--method",
            "POST",
            "--sequence",
            r#"[{"method":"notification.create","params":{"body":"CLI_MARKER"}}]"#,
        ]);
        assert!(
            out.status.success(),
            "cli register must succeed: {}",
            webhook_common::stderr_str(&out)
        );
        let reg: Value = serde_json::from_str(&stdout_str(&out)).expect("cli register prints JSON");
        let cli_id = reg["id"].as_str().expect("cli register id").to_string();

        let out = inst.cli(&["webhook", "list"]);
        assert!(out.status.success());
        let list: Value = serde_json::from_str(&stdout_str(&out)).expect("cli list JSON");
        let found = list["webhooks"]
            .as_array()
            .map(|a| a.iter().any(|w| w["id"].as_str() == Some(cli_id.as_str())))
            .unwrap_or(false);
        assert!(found, "cli list must include the registered webhook");

        let out = inst.cli(&["webhook", "info", "--id", &cli_id]);
        assert!(out.status.success());
        let info: Value = serde_json::from_str(&stdout_str(&out)).expect("cli info JSON");
        assert_eq!(info["id"].as_str(), Some(cli_id.as_str()));

        let (code, _b) = inst.post(&cli_id, r#"{}"#);
        assert_eq!(code, 200);
        assert!(wait_notification(
            inst,
            "CLI_MARKER",
            Duration::from_secs(8)
        ));

        let out = inst.cli(&["webhook", "unregister", "--id", &cli_id]);
        assert!(out.status.success());
        assert_eq!(inst.post(&cli_id, r#"{}"#).0, 404);
    }

    {
        let out = inst.cli(&["hook-handler", "list"]);
        assert!(
            out.status.success(),
            "hook-handler list: {}",
            webhook_common::stderr_str(&out)
        );
        let list: Value = serde_json::from_str(&stdout_str(&out)).expect("hook-handler list JSON");
        let ids: Vec<String> = list["handlers"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|h| h["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            ids.iter().any(|i| i == "host/webhook-notify"),
            "host default handler must be listed"
        );
        assert!(
            ids.iter().any(|i| i == "user/hookonly"),
            "user handler must be merged in"
        );

        let out = inst.cli(&["hook-handler", "dispatch", "--id", "user/hookonly"]);
        assert!(
            out.status.success(),
            "dispatch: {}",
            webhook_common::stderr_str(&out)
        );
        assert!(
            wait_notification(inst, "dispatched", Duration::from_secs(8)),
            "dispatched hook handler must execute its IpcSequence"
        );
    }

    {
        let (id, _url) = register_notify_webhook(inst, json!({}));

        let small = format!(
            r#"{{"message":"SMALLBODY","pad":"{}"}}"#,
            "x".repeat(MAX_BODY_BYTES / 4)
        );
        assert!(small.len() < MAX_BODY_BYTES);
        assert_eq!(inst.post(&id, &small).0, 200, "상한 이하는 평소대로 200");
        assert!(wait_notification(inst, "SMALLBODY", Duration::from_secs(8)));

        let big = format!(
            r#"{{"message":"BIGBODY","pad":"{}"}}"#,
            "x".repeat(MAX_BODY_BYTES * 2)
        );
        let (code, body) = inst.post(&id, &big);
        assert_eq!(code, 413, "상한 초과 body 는 413");
        assert_eq!(body, "payload too large");
        assert!(
            !has_notification(inst, "BIGBODY"),
            "413 은 시퀀스를 실행하지 않는다"
        );

        // 경로 일치보다 본문 상한 판정이 먼저 적용되는지 확인한다.
        assert_eq!(inst.post("no-such-path-for-big-body", &big).0, 413);
        inst.call("webhook.unregister", json!({ "id": id }));
    }

    {
        // 쿨다운은 출처 IP 전체에 적용되므로 정상 요청 검사 뒤에 실행한다.
        let mut saw_429 = false;
        for i in 0..(ABUSE_THRESHOLD + 25) {
            let (code, _b) = inst.post("no-such-webhook-abuse", "");
            if code == 429 {
                saw_429 = true;
                break;
            }
            assert_eq!(code, 404, "pre-cooldown misses must be 404 (req {i})");
        }
        assert!(
            saw_429,
            "repeated 404s from one source must trip the abuse cooldown (429)"
        );
    }

    {
        // 앞의 쿨다운이 다음 인증 실패 검사를 가리지 않도록 해제 시간을 기다린다.
        std::thread::sleep(Duration::from_secs(ABUSE_COOLDOWN_SECS + 2));

        // 인증 실패는 웹훅 횟수 예산을 소비하지 않지만 반복 인증 시도 제한에는 집계돼야 한다.
        let (id, _url) = register_notify_webhook(
            inst,
            json!({ "auth": { "location": "query", "key": "tok", "token": "s3cr3t" } }),
        );
        let wrong_token_path = format!("{id}?tok=wrong");
        let mut saw_429 = false;
        for i in 0..(ABUSE_THRESHOLD + 25) {
            let (code, _b) = inst.post(&wrong_token_path, "");
            if code == 429 {
                saw_429 = true;
                break;
            }
            assert_eq!(
                code, 401,
                "pre-cooldown rejected tokens must be 401 (req {i})"
            );
        }
        assert!(
            saw_429,
            "repeated rejected tokens from one source must trip the abuse cooldown (429)"
        );
    }
}

/// 부팅 뒤 파일 재작성·reload는 레지스트리의 지연 저장과 경합할 수 있어 핸들러를 미리 작성한다.
struct HookEnvSetup {
    handlers_toml: String,
    hook_marker: std::path::PathBuf,
    dispatch_marker: std::path::PathBuf,
}

fn hook_env_setup() -> HookEnvSetup {
    // PID와 프로세스 내 카운터로 임시 경로를 구별한다.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let hook_marker = std::env::temp_dir().join(format!("tasty-hookenv-{unique}.txt"));
    let dispatch_marker = std::env::temp_dir().join(format!("tasty-dispenv-{unique}.txt"));

    // cmd가 > 앞 숫자를 파일 디스크립터로 해석하지 않도록 공백을 넣고 출력의 끝 공백은 trim한다.
    let (hook_cmd, dispatch_shell, dispatch_flag, dispatch_line) = if cfg!(windows) {
        (
            format!(
                "echo %TASTY_HOOK_EVENT%/%TASTY_HOOK_SOURCE%/%TASTY_HOOK_SURFACE_ID% > {}",
                hook_marker.display()
            ),
            "cmd",
            "/C",
            format!(
                "echo %TASTY_HOOK_EVENT%/%TASTY_HOOK_SOURCE%/%TASTY_HOOK_REPO% > {}",
                dispatch_marker.display()
            ),
        )
    } else {
        (
            format!(
                "echo \"$TASTY_HOOK_EVENT/$TASTY_HOOK_SOURCE/$TASTY_HOOK_SURFACE_ID\" > {}",
                hook_marker.display()
            ),
            "sh",
            "-c",
            format!(
                "echo \"$TASTY_HOOK_EVENT/$TASTY_HOOK_SOURCE/$TASTY_HOOK_REPO\" > {}",
                dispatch_marker.display()
            ),
        )
    };

    let handlers_toml = format!(
        r#"
[[handler]]
id = "user/envhook"
source = "hook"
priority = 50
[handler.action]
kind = "shell_command"
command = '{hook_cmd}'

[[handler]]
id = "user/envdispatch"
source = "hook"
priority = 50
[handler.action]
kind = "shell_command"
command = "{dispatch_shell}"
args = ["{dispatch_flag}", '{dispatch_line}']
"#
    );
    HookEnvSetup {
        handlers_toml,
        hook_marker,
        dispatch_marker,
    }
}

fn hook_env_flow(inst: &WebhookInstance, setup: &HookEnvSetup) {
    let hook_marker = &setup.hook_marker;
    let dispatch_marker = &setup.dispatch_marker;
    let sid = inst.first_surface_id();

    inst.call(
        "hook.set",
        json!({ "surface_id": sid, "event": "bell", "handler": "user/envhook" }),
    );
    let fired = inst.call(
        "surface.fire_hook",
        json!({ "surface_id": sid, "event": "bell" }),
    );
    assert_eq!(fired["fired"].as_u64(), Some(1), "hook should fire once");
    let content = wait_file_content(hook_marker, Duration::from_secs(10));
    assert_eq!(content, format!("bell/hook/{sid}"));

    let ack = inst.call(
        "hook_handler.dispatch",
        json!({ "id": "user/envdispatch", "body": { "repo": "tasty" } }),
    );
    assert_eq!(ack["accepted"].as_bool(), Some(true));
    let content = wait_file_content(dispatch_marker, Duration::from_secs(10));
    assert_eq!(content, "user/envdispatch/dispatch/tasty");

    std::fs::remove_file(hook_marker).ok();
    std::fs::remove_file(dispatch_marker).ok();
}

fn unique_home() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    std::env::temp_dir().join(format!("tasty-wh-family-home-{unique}"))
}

fn register_persistent(inst: &WebhookInstance, persistent: bool) -> String {
    let resp = inst.call(
        "webhook.register",
        json!({
            "methods": ["POST"],
            "persistent": persistent,
            "sequence": [{
                "method": "notification.create",
                "params": { "body": "${body.message}" }
            }]
        }),
    );
    resp["id"].as_str().expect("register id").to_string()
}

fn list_ids(inst: &WebhookInstance) -> Vec<String> {
    inst.call("webhook.list", json!({}))["webhooks"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|w| w["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 정상 HTTP 요청을 남용 검사 전에 실행하고, 같은 홈으로 재시작해 영속·임시 항목을 구별한다.
/// 출처 쿨다운은 메모리에만 있어 새 프로세스에는 남지 않는다.
#[test]
fn webhook_family() {
    // spawn 직전까지 예약을 유지하도록 PortLease 자체를 빌더에 넘긴다.
    let lease = webhook_common::free_port();
    let home = unique_home();
    let hook_env = hook_env_setup();
    let combined_handlers = format!(
        "{HOOK_HANDLERS_TOML}
{}",
        hook_env.handlers_toml
    );
    let (persistent_id, persistent_url, temp_id);

    {
        let inst = WebhookInstance::builder(lease)
            .home(home.clone())
            .env(
                "TASTY_WEBHOOK_ABUSE_THRESHOLD",
                &ABUSE_THRESHOLD.to_string(),
            )
            .env("TASTY_WEBHOOK_ABUSE_WINDOW_SECS", "3600")
            .env("TASTY_WEBHOOK_MAX_BODY_BYTES", &MAX_BODY_BYTES.to_string())
            .env(
                "TASTY_WEBHOOK_ABUSE_COOLDOWN_SECS",
                &ABUSE_COOLDOWN_SECS.to_string(),
            )
            .file("hook-handlers.toml", &combined_handlers)
            .spawn();
        inst.wait_webhook_ready();
        // 첫 등록 쓰기 전에 핸들러 파일을 읽어 미리 작성한 사용자 설정을 병합한다.

        persistent_id = register_persistent(&inst, true);
        temp_id = register_persistent(&inst, false);
        persistent_url = inst.call("webhook.info", json!({ "id": persistent_id }))["url"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(inst.post(&persistent_id, r#"{"message":"a"}"#).0, 200);
        assert_eq!(inst.post(&temp_id, r#"{"message":"b"}"#).0, 200);

        integration_flow(&inst);
        hook_env_flow(&inst, &hook_env);
    }

    std::thread::sleep(Duration::from_millis(500));

    {
        // URL 유지가 검증 대상이라 재시작에서는 기존 포트를 바꾸지 않는다.
        let inst2 = WebhookInstance::builder_for_restart()
            .home(home.clone())
            .spawn();
        inst2.wait_webhook_ready();

        let ids = list_ids(&inst2);
        assert!(
            ids.iter().any(|i| i == &persistent_id),
            "persistent webhook must be restored after restart (ids: {ids:?})"
        );
        assert!(
            !ids.iter().any(|i| i == &temp_id),
            "temporary webhook must NOT survive restart"
        );

        let restored_url = inst2.call("webhook.info", json!({ "id": persistent_id }))["url"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            restored_url, persistent_url,
            "restored webhook must keep the same URL (stable across restart)"
        );

        assert_eq!(
            inst2.post(&persistent_id, r#"{"message":"c"}"#).0,
            200,
            "restored persistent webhook must serve requests"
        );
        assert_eq!(
            inst2.post(&temp_id, r#"{"message":"d"}"#).0,
            404,
            "temporary webhook path must be gone (404)"
        );
    }

    let _ = std::fs::remove_dir_all(&home);
}
