//! Actual GUI/headless servers, with a raw loopback peer rather than a native HTML viewer.
mod attach_common;
mod common;
use attach_common::{
    AttachStream, try_open_workspace_attach_stream, wait_for_control_event, write_control_frame,
};
use base64::Engine;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tasty_ipc::html_transfer::{CAPABILITY, HtmlReply, HtmlRequest, TransferId, VIEWER_CAPABILITY};
fn server() -> &'static common::TastyInstance {
    let instance = common::shared();
    static NOTE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    NOTE.get_or_init(|| {
        tracing::info!(
            "HTML_SERVER pid={} home={} gui={}",
            instance.pid(),
            instance.tasty_home().display(),
            cfg!(feature = "gui")
        )
    });
    instance
}
fn open(server: &common::TastyInstance, ws: u64, url: &str) -> u64 {
    server.call(
        "tab.create",
        json!({"pane_id":server.first_pane_id_in_workspace(ws),"type":"html","url":url}),
    )["surface_id"]
        .as_u64()
        .unwrap()
}
fn attach(server: &common::TastyInstance, ws: u64, surface: u64) -> (AttachStream, TransferId) {
    let (mut stream, desc) =
        try_open_workspace_attach_stream(server.port(), ws).expect("workspace attaches");
    assert!(
        desc["stream_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == CAPABILITY)
    );
    assert!(
        desc["stream_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == VIEWER_CAPABILITY)
    );
    assert_eq!(
        desc["surfaces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["remote_id"].as_u64() == Some(surface))
            .unwrap()["role"],
        "placeholder"
    );
    write_control_frame(
        &mut stream,
        &serde_json::to_value(HtmlRequest::HtmlCapabilities {
            capabilities: vec![CAPABILITY.into()],
        })
        .unwrap(),
    );
    let id = TransferId {
        epoch: 1,
        surface_id: surface as u32,
        binding: 1,
        document: 1,
        request: 1,
        transfer: 1,
    };
    (stream, id)
}
fn request(stream: &mut AttachStream, msg: HtmlRequest) {
    write_control_frame(stream, &serde_json::to_value(msg).unwrap());
}
fn event(stream: &mut AttachStream, name: &str) -> Value {
    wait_for_control_event(stream, name)
}
#[test]
fn html_small_and_large_exact_bytes_metadata_and_placeholder_backwards_compatibility() {
    let server = server();
    let dir = tempfile::tempdir().unwrap();
    for size in [0usize, 100_000, 100_001, 1_500_000] {
        let body: Vec<_> = (0..size).map(|i| (i % 251) as u8).collect();
        let file = dir.path().join(format!("entry-{size}.html"));
        std::fs::write(&file, &body).unwrap();
        let ws = server.create_workspace("html-transport-size");
        let sid = open(server, ws.id, file.to_str().unwrap());
        let (mut stream, id) = attach(server, ws.id, sid);
        request(&mut stream, HtmlRequest::HtmlEntryRequest { id });
        let metadata = event(&mut stream, "html_metadata");
        assert_eq!(metadata["metadata"]["size"], size);
        assert_eq!(
            metadata["metadata"]["root"],
            dir.path().canonicalize().unwrap().to_str().unwrap()
        );
        let mut received = Vec::new();
        loop {
            request(
                &mut stream,
                HtmlRequest::HtmlRead {
                    id,
                    offset: received.len() as u64,
                    allow: size > 100_000,
                },
            );
            if size == 0 {
                break;
            }
            let payload = event(&mut stream, "html_chunk");
            assert!(
                serde_json::to_vec(&payload).unwrap().len()
                    <= tasty_ipc::stream::MAX_FRAME_LEN as usize
            );
            let HtmlReply::HtmlChunk {
                offset, data_b64, ..
            } = serde_json::from_value(payload).unwrap()
            else {
                unreachable!()
            };
            assert_eq!(offset, received.len() as u64);
            received.extend(
                base64::engine::general_purpose::STANDARD
                    .decode(data_b64)
                    .unwrap(),
            );
            if received.len() == size {
                break;
            }
        }
        assert_eq!(event(&mut stream, "html_complete")["size"], size);
        assert_eq!(Sha256::digest(received), Sha256::digest(body));
    }
}
#[test]
fn html_open_updates_headless_and_gui_requested_source_without_native_commit() {
    let server = server();
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.html");
    let b = dir.path().join("b.html");
    std::fs::write(&a, b"old").unwrap();
    std::fs::write(&b, b"new source").unwrap();
    let ws = server.create_workspace("html-source-update");
    let sid = open(server, ws.id, a.to_str().unwrap());
    let changed = server.call(
        "html.open",
        json!({"surface":sid,"url":b.to_str().unwrap()}),
    );
    assert_eq!(changed["ok"], true);
    let (mut stream, id) = attach(server, ws.id, sid);
    request(&mut stream, HtmlRequest::HtmlEntryRequest { id });
    let meta = event(&mut stream, "html_metadata");
    assert_eq!(meta["metadata"]["source"]["path"], b.to_str().unwrap());
    assert_eq!(meta["metadata"]["size"], 10);
    request(&mut stream, HtmlRequest::HtmlCancel { id });
    assert_eq!(event(&mut stream, "html_cancelled")["id"]["transfer"], 1);
}
#[test]
fn html_http_source_is_metadata_then_proxy_failure_without_client_fallback() {
    let server = server();
    let ws = server.create_workspace("html-http-metadata");
    let sid = open(server, ws.id, "http://127.0.0.1:9/no-file");
    let (mut stream, id) = attach(server, ws.id, sid);
    request(&mut stream, HtmlRequest::HtmlEntryRequest { id });
    let meta = event(&mut stream, "html_metadata");
    assert_eq!(meta["metadata"]["source"]["kind"], "http");
    assert_eq!(meta["metadata"]["regular"], false);
    assert!(
        event(&mut stream, "html_failure")["reason"]
            .as_str()
            .unwrap()
            .contains("proxy")
    );
}

#[test]
fn html_unattached_opt_in_peer_is_refused_and_cannot_select_a_path() {
    let server = server();
    let mut stream = attach_common::open_stream_without_attach(server.port());
    request(
        &mut stream,
        HtmlRequest::HtmlCapabilities {
            capabilities: vec![CAPABILITY.into()],
        },
    );
    let id = TransferId {
        epoch: 1,
        surface_id: 1,
        binding: 1,
        document: 1,
        request: 1,
        transfer: 1,
    };
    request(&mut stream, HtmlRequest::HtmlEntryRequest { id });
    assert!(
        event(&mut stream, "html_failure")["reason"]
            .as_str()
            .unwrap()
            .contains("workspace attach")
    );
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("safe.html");
    std::fs::write(&source, b"safe body").unwrap();
    let ws = server.create_workspace("html-no-client-path");
    let sid = open(server, ws.id, source.to_str().unwrap());
    let (mut stream, id) = attach(server, ws.id, sid);
    for (n, path) in ["/etc/passwd", "../secret", "%2e%2e/secret", "%2fetc/passwd"]
        .iter()
        .enumerate()
    {
        let id = TransferId {
            request: n as u64 + 1,
            transfer: n as u64 + 1,
            ..id
        };
        let mut msg = serde_json::to_value(HtmlRequest::HtmlEntryRequest { id }).unwrap();
        msg["path"] = json!(path);
        write_control_frame(&mut stream, &msg);
        let metadata = event(&mut stream, "html_metadata");
        assert_eq!(
            metadata["metadata"]["source"]["path"],
            source.to_str().unwrap()
        );
        request(
            &mut stream,
            HtmlRequest::HtmlRead {
                id,
                offset: 0,
                allow: false,
            },
        );
        let chunk = event(&mut stream, "html_chunk");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(chunk["data_b64"].as_str().unwrap())
                .unwrap(),
            b"safe body"
        );
        event(&mut stream, "html_complete");
    }
}

#[test]
fn html_resource_wire_keeps_original_root_generation_and_bounded_credit() {
    let server = server();
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.html");
    let expected: Vec<u8> = (0..300_001).map(|i| (i % 251) as u8).collect();
    std::fs::write(&index, b"entry").unwrap();
    std::fs::write(dir.path().join("large.bin"), &expected).unwrap();
    let ws = server.create_workspace("html-resource-root");
    let sid = open(server, ws.id, index.to_str().unwrap());
    let (mut stream, entry) = attach(server, ws.id, sid);
    request(&mut stream, HtmlRequest::HtmlEntryRequest { id: entry });
    let metadata = event(&mut stream, "html_metadata");
    let generation = metadata["metadata"]["source_generation"].as_u64().unwrap();
    request(
        &mut stream,
        HtmlRequest::HtmlRead {
            id: entry,
            offset: 0,
            allow: false,
        },
    );
    event(&mut stream, "html_chunk");
    event(&mut stream, "html_complete");
    let first = TransferId {
        request: 2,
        transfer: 2,
        ..entry
    };
    request(
        &mut stream,
        HtmlRequest::HtmlResourceRequest {
            id: first,
            entry,
            source_generation: generation,
            relative: "large.bin".into(),
            query: None,
        },
    );
    assert_eq!(
        event(&mut stream, "html_metadata")["metadata"]["size"],
        300_001
    );
    request(
        &mut stream,
        HtmlRequest::HtmlRead {
            id: first,
            offset: 0,
            allow: false,
        },
    );
    assert!(
        event(&mut stream, "html_failure")["reason"]
            .as_str()
            .unwrap()
            .contains("allowance")
    );
    let child = TransferId {
        request: 3,
        transfer: 3,
        ..entry
    };
    request(
        &mut stream,
        HtmlRequest::HtmlResourceRequest {
            id: child,
            entry,
            source_generation: generation,
            relative: "large.bin".into(),
            query: None,
        },
    );
    event(&mut stream, "html_metadata");
    let mut bytes = Vec::new();
    let mut chunks = 0;
    while bytes.len() < expected.len() {
        request(
            &mut stream,
            HtmlRequest::HtmlRead {
                id: child,
                offset: bytes.len() as u64,
                allow: true,
            },
        );
        let payload = event(&mut stream, "html_chunk");
        assert!(
            serde_json::to_vec(&payload).unwrap().len()
                <= tasty_ipc::stream::MAX_FRAME_LEN as usize
        );
        assert_eq!(payload["offset"], bytes.len());
        bytes.extend(
            base64::engine::general_purpose::STANDARD
                .decode(payload["data_b64"].as_str().unwrap())
                .unwrap(),
        );
        chunks += 1;
    }
    assert_eq!(event(&mut stream, "html_complete")["size"], expected.len());
    assert_eq!(chunks, 7);
    assert_eq!(Sha256::digest(&bytes), Sha256::digest(&expected));
    let mut sequence = 4;
    for relative in [
        "../large.bin",
        "/etc/passwd",
        "%2e%2e/etc/passwd",
        "sub\\..\\large.bin",
    ] {
        let id = TransferId {
            request: sequence,
            transfer: sequence,
            ..entry
        };
        sequence += 1;
        request(
            &mut stream,
            HtmlRequest::HtmlResourceRequest {
                id,
                entry,
                source_generation: generation,
                relative: relative.into(),
                query: None,
            },
        );
        event(&mut stream, "html_failure");
    }
    let stale = TransferId {
        request: sequence,
        transfer: sequence,
        ..entry
    };
    request(
        &mut stream,
        HtmlRequest::HtmlResourceRequest {
            id: stale,
            entry,
            source_generation: generation + 1,
            relative: "large.bin".into(),
            query: None,
        },
    );
    assert!(
        event(&mut stream, "html_failure")["reason"]
            .as_str()
            .unwrap()
            .contains("stale")
    );
    request(&mut stream, HtmlRequest::HtmlCancel { id: entry });
    let cancelled = TransferId {
        request: sequence + 1,
        transfer: sequence + 1,
        ..entry
    };
    request(
        &mut stream,
        HtmlRequest::HtmlResourceRequest {
            id: cancelled,
            entry,
            source_generation: generation,
            relative: "large.bin".into(),
            query: None,
        },
    );
    assert!(
        event(&mut stream, "html_failure")["reason"]
            .as_str()
            .unwrap()
            .contains("root")
    );
}

#[test]
fn html_nested_navigation_and_fresh_reload_preserve_root_and_provenance() {
    let server = server();
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.html");
    std::fs::write(&index, b"entry").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let body = b"<title>NESTED</title>";
    std::fs::write(dir.path().join("sub/next.html"), body).unwrap();
    let ws = server.create_workspace("html-nested-navigation");
    let sid = open(server, ws.id, index.to_str().unwrap());
    let (mut stream, entry) = attach(server, ws.id, sid);
    request(&mut stream, HtmlRequest::HtmlEntryRequest { id: entry });
    let metadata = event(&mut stream, "html_metadata");
    let generation = metadata["metadata"]["source_generation"].as_u64().unwrap();
    request(
        &mut stream,
        HtmlRequest::HtmlRead {
            id: entry,
            offset: 0,
            allow: false,
        },
    );
    event(&mut stream, "html_chunk");
    event(&mut stream, "html_complete");
    let moved = TransferId {
        document: entry.document + 1,
        request: 2,
        transfer: 2,
        ..entry
    };
    request(
        &mut stream,
        HtmlRequest::HtmlNavigateRequest {
            id: moved,
            entry,
            source_generation: generation,
            relative: "sub/next.html".into(),
            query: Some("q=1".into()),
            fragment: Some("anchor".into()),
        },
    );
    let metadata = event(&mut stream, "html_metadata");
    assert_eq!(metadata["metadata"]["file"]["mime"], "text/html");
    assert_eq!(metadata["metadata"]["file"]["relative"], "sub/next.html");
    assert_eq!(
        metadata["metadata"]["cwd"],
        dir.path().join("sub").to_str().unwrap()
    );
    assert!(
        metadata["metadata"]["source"]["url"]
            .as_str()
            .unwrap()
            .ends_with("/sub/next.html?q=1#anchor")
    );
    request(
        &mut stream,
        HtmlRequest::HtmlRead {
            id: moved,
            offset: 0,
            allow: false,
        },
    );
    let chunk = event(&mut stream, "html_chunk");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(chunk["data_b64"].as_str().unwrap())
            .unwrap(),
        body
    );
    event(&mut stream, "html_complete");
    let stale = TransferId {
        request: 3,
        transfer: 3,
        ..entry
    };
    request(
        &mut stream,
        HtmlRequest::HtmlResourceRequest {
            id: stale,
            entry,
            source_generation: generation,
            relative: "sub/next.html".into(),
            query: None,
        },
    );
    event(&mut stream, "html_failure");
    let replacement = dir.path().join("replacement");
    std::fs::write(&replacement, b"new original").unwrap();
    std::fs::rename(replacement, &index).unwrap();
    let reload = TransferId {
        document: moved.document + 1,
        request: 4,
        transfer: 4,
        ..entry
    };
    request(
        &mut stream,
        HtmlRequest::HtmlFileEntryRequest {
            id: reload,
            relative: "sub/next.html".into(),
            query: None,
            fragment: None,
        },
    );
    assert_eq!(
        event(&mut stream, "html_metadata")["metadata"]["size"],
        body.len()
    );
    request(
        &mut stream,
        HtmlRequest::HtmlRead {
            id: reload,
            offset: 0,
            allow: false,
        },
    );
    let chunk = event(&mut stream, "html_chunk");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(chunk["data_b64"].as_str().unwrap())
            .unwrap(),
        body
    );
    event(&mut stream, "html_complete");
}

/// Waits past the server's 30-second credit timeout; heartbeats keep the attach connection alive.
fn event_after_credit_timeout(stream: &mut AttachStream, name: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    // 기한 검사는 소켓 timeout 분기에만 있다. 무관한 프레임 소비와 소켓 대기 계약이 이 시험의 내용이라
    // 기한 뒤 재읽기 꼴로 바꾸지 않고 원래 루프를 둔다.
    loop {
        match attach_common::read_frame_result(stream) {
            Ok((attach_common::TAG_CONTROL, payload)) => {
                let value: Value = serde_json::from_slice(&payload).unwrap();
                if value["event"] == name {
                    return value;
                }
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && std::time::Instant::now() < deadline => {}
            Err(error) => panic!("waiting for {name}: {error:?}"),
        }
    }
}

#[test]
fn html_expired_size_question_is_answered_by_a_fresh_allowed_request() {
    let server = server();
    let dir = tempfile::tempdir().unwrap();
    let body: Vec<_> = (0..300_000).map(|i| (i % 251) as u8).collect();
    let file = dir.path().join("expired.html");
    std::fs::write(&file, &body).unwrap();
    let ws = server.create_workspace("html-transport-expired-question");
    let sid = open(server, ws.id, file.to_str().unwrap());
    let (mut stream, id) = attach(server, ws.id, sid);
    request(&mut stream, HtmlRequest::HtmlEntryRequest { id });
    assert_eq!(
        event(&mut stream, "html_metadata")["metadata"]["size"],
        300_000
    );
    // No credit while the size question is open: the server ends the worker on its own.
    let failure = event_after_credit_timeout(&mut stream, "html_failure");
    assert!(failure["reason"].as_str().unwrap().contains("timeout"));
    let retry = TransferId {
        request: 2,
        transfer: 2,
        ..id
    };
    request(&mut stream, HtmlRequest::HtmlEntryRequest { id: retry });
    assert_eq!(
        event(&mut stream, "html_metadata")["metadata"]["size"],
        300_000
    );
    let mut received = Vec::new();
    while received.len() < body.len() {
        request(
            &mut stream,
            HtmlRequest::HtmlRead {
                id: retry,
                offset: received.len() as u64,
                allow: true,
            },
        );
        let HtmlReply::HtmlChunk { data_b64, .. } =
            serde_json::from_value(event(&mut stream, "html_chunk")).unwrap()
        else {
            unreachable!()
        };
        received.extend(
            base64::engine::general_purpose::STANDARD
                .decode(data_b64)
                .unwrap(),
        );
    }
    assert_eq!(event(&mut stream, "html_complete")["size"], 300_000);
    assert_eq!(Sha256::digest(received), Sha256::digest(body));
}
