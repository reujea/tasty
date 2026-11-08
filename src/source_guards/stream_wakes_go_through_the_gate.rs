//! 스트림 수신 깨움(`StreamReady`·`AttachClientData`)은 `StreamWakeGate`를 거쳐서만 보낸다.
//! 읽기 스레드가 프레임마다 이 이벤트를 직접 보내면 winit 사용자 이벤트 큐가 비지 않아
//! 입력·redraw·IPC가 굶는다. 규칙은 docs/architecture/data-flows.md 의 스트림 회차 절에 있다.
//!
//! 이벤트를 만드는 줄을 찾고, 그 파일이 게이트에 보낼 함수를 넘기는 자리인지 확인한다.
//! match 갈래(`=>`)와 주석은 세지 않는다. 별칭이나 매크로로 만든 이벤트는 추적하지 않는다.

use tasty_doc_guards::source_text::mask_non_code;

const EVENTS: &[&str] = &["AppEvent::StreamReady", "AppEvent::AttachClientData"];

/// (파일, 그 파일에 있어야 하는 게이트 연결 표지, 사유)
const SENDERS: &[(&str, &str, &str)] = &[
    (
        "src/app/boot_machine.rs",
        "stream_wake.install(",
        "GUI 스트림 서버의 waker를 게이트에 연결한다",
    ),
    (
        "src/adapters/production/headless_waker.rs",
        "fn stream_waker(",
        "headless 의 원시 sender다. src/boot.rs 가 stream_wake.install 로 감싼다",
    ),
    (
        "src/app.rs",
        "attach_client_wake",
        "mirror 연결 읽기 스레드가 공유하는 게이트에 sender를 연결한다",
    ),
];

fn sends(raw: &str) -> Vec<(usize, String)> {
    mask_non_code(raw)
        .lines()
        .enumerate()
        .filter(|(_, line)| EVENTS.iter().any(|e| line.contains(e)) && !line.contains("=>"))
        .map(|(i, line)| (i + 1, line.trim().to_string()))
        .collect()
}

#[test]
fn stream_wakes_are_only_sent_through_the_gate() {
    let files = super::rust_sources();
    let mut offenders = Vec::new();
    let mut seen = Vec::new();
    for (rel_path, raw) in &files {
        let rel = rel_path.to_string_lossy().to_string();
        if !rel.starts_with("src/") || rel.starts_with("src/source_guards/") {
            continue;
        }
        let hits = sends(raw);
        if hits.is_empty() {
            continue;
        }
        match SENDERS.iter().find(|(f, _, _)| *f == rel) {
            Some((_, mark, _)) if raw.contains(mark) => seen.push(rel.clone()),
            _ => offenders.extend(hits.iter().map(|(l, t)| format!("  {rel}:{l}  {t}"))),
        }
    }
    assert!(
        offenders.is_empty(),
        "게이트를 거치지 않고 스트림 깨움을 보내는 곳이 있다:\n{}\nStreamWakeGate의 waker를 쓰고, 새 sender라면 SENDERS에 게이트 연결 표지와 사유를 적는다.",
        offenders.join("\n")
    );
    for (f, _, _) in SENDERS {
        assert!(
            seen.iter().any(|s| s == f),
            "{f} 에서 스트림 깨움 sender를 찾지 못했다. 옮겼다면 SENDERS를 고친다."
        );
    }
    let boot = files
        .iter()
        .find(|(p, _)| p.to_string_lossy() == "src/boot.rs")
        .map(|(_, raw)| raw.as_str())
        .unwrap_or_default();
    assert!(
        boot.contains("stream_wake.install(waker.stream_waker())"),
        "headless 의 stream waker가 게이트로 감싸지지 않았다(src/boot.rs)"
    );
}

#[test]
fn a_direct_send_is_reported() {
    let hits = sends(
        "fn f(p: &P) { p.send_event(AppEvent::StreamReady); }\nAppEvent::StreamReady => {}\n",
    );
    assert_eq!(hits.len(), 1);
}
