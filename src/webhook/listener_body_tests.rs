//! Raw HTTP exercises the real tiny_http body readers, including incomplete chunks.

use super::{AckStatus, Screened};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const LIMIT: usize = 2048;

fn read_body(body: &[u8], framing: &str, finish: bool) -> Result<Value, ()> {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let worker = std::thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let mut request = Screened(request);
        let result = request.read_json_body(LIMIT).map_err(|_| ());
        request.respond(if result.is_err() {
            AckStatus::PayloadTooLarge
        } else {
            AckStatus::Received
        });
        result
    });
    let mut client = TcpStream::connect(addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        client,
        "POST / HTTP/1.1\r\nHost: localhost\r\n{framing}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    if framing.starts_with("Transfer-Encoding") {
        // An unfinished oversized chunk must not make the reader await its tail.
        let size = body.len() + if finish { 0 } else { 100 };
        write!(client, "{size:x}\r\n").unwrap();
        client.write_all(body).unwrap();
        if finish {
            client.write_all(b"\r\n0\r\n\r\n").unwrap();
        }
    } else {
        client.write_all(body).unwrap();
    }
    let mut response = [0; 128];
    assert!(client.read(&mut response).unwrap() > 0);
    // Release the peer after observing the response, before joining cleanup.
    drop(client);
    worker.join().unwrap()
}

#[test]
fn raw_chunked_utf8_boundaries() {
    for size in [LIMIT - 1, LIMIT, LIMIT + 1] {
        let mut valid = b"{}".to_vec();
        valid.resize(size, b' ');
        for invalid in [false, true] {
            let mut body = valid.clone();
            if invalid {
                body[0] = 0xff;
            }
            let actual = read_body(&body, "Transfer-Encoding: chunked", size <= LIMIT);
            let expected = if size > LIMIT {
                Err(())
            } else if invalid {
                Ok(Value::Null)
            } else {
                Ok(json!({}))
            };
            assert_eq!(actual, expected, "size={size}, invalid={invalid}");
        }
    }
}

#[test]
fn raw_chunked_cap_can_split_a_valid_multibyte_character() {
    let mut body = vec![b' '; LIMIT];
    body.extend_from_slice("한".as_bytes());
    assert!(read_body(&body, "Transfer-Encoding: chunked", false).is_err());
}

#[test]
fn raw_content_length_rejects_before_waiting_for_body() {
    assert!(read_body(&[], &format!("Content-Length: {}", LIMIT + 1), true).is_err());
}

#[test]
fn raw_content_length_preserves_json_and_invalid_utf8_policy() {
    assert_eq!(read_body(b"{}", "Content-Length: 2", true), Ok(json!({})));
    assert_eq!(
        read_body(b"\xff", "Content-Length: 1", true),
        Ok(Value::Null)
    );
}

/// The real `Screened` path over one connection: reject with 413 when the body is over
/// [`LIMIT`], read it and answer 200 otherwise. The handler reports each request once
/// `respond` returns — that return includes destroying the request, which is where an
/// unread `Content-Length` body used to be drained.
fn serve(requests: usize) -> (std::net::SocketAddr, std::sync::mpsc::Receiver<()>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for _ in 0..requests {
            let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(5)) else {
                return;
            };
            let mut request = Screened(request);
            let ack = match request.read_json_body(LIMIT) {
                Ok(_) => AckStatus::Received,
                Err(_) => AckStatus::PayloadTooLarge,
            };
            request.respond(ack);
            if done_tx.send(()).is_err() {
                return;
            }
        }
    });
    (addr, done_rx)
}

/// One connection answered by the abuse screen, which rejects a source in cooldown
/// with 429 before the body is read. The source is passed in, so this never touches
/// the tracker entry of the loopback address the other tests come from.
fn serve_blocked() -> (std::net::SocketAddr, std::sync::mpsc::Receiver<()>) {
    const SOURCE: &str = "198.51.100.9";
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for _ in 0..2048 {
            super::abuse::record_failure(SOURCE);
            if super::abuse::is_source_blocked(SOURCE) {
                break;
            }
        }
        assert!(super::abuse::is_source_blocked(SOURCE));
        let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(5)) else {
            return;
        };
        assert!(
            super::reject_if_abusive(request, Some(SOURCE)).is_none(),
            "a blocked source must be consumed by the screen"
        );
        let _ = done_tx.send(()); // 수신자가 이미 갔으면 알릴 곳이 없다.
    });
    (addr, done_rx)
}

/// Declares `declared` body bytes, sends none of them and keeps the socket open.
/// Everything below must happen without the client sending the rest or a FIN:
/// the full rejection with `Connection: close`, the server closing the connection,
/// the handler returning, and the server's receive side being gone.
fn assert_rejected_without_draining(extra_header: &str, declared: usize) {
    assert_rejection_closes(
        serve(1),
        extra_header,
        declared,
        "HTTP/1.1 413",
        "payload too large",
    );
}

fn assert_rejection_closes(
    server: (std::net::SocketAddr, std::sync::mpsc::Receiver<()>),
    extra_header: &str,
    declared: usize,
    status: &str,
    body: &str,
) {
    let (addr, done) = server;
    let mut client = TcpStream::connect(addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        client,
        "POST /hook HTTP/1.1\r\nHost: localhost\r\n{extra_header}Content-Length: {declared}\r\n\r\n"
    )
    .unwrap();

    // Response EOF alone is not the completion signal (a close request got it even
    // while the body was still being drained), but it must come without our help.
    let mut response = Vec::new();
    client.read_to_end(&mut response).expect(
        "the server must close the connection after rejecting without waiting for the body",
    );
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with(status), "{response}");
    assert!(response.contains("\r\nConnection: close\r\n"), "{response}");
    assert!(response.ends_with(body), "{response}");
    assert!(!response.contains("100 Continue"), "{response}");

    done.recv_timeout(Duration::from_secs(5))
        .expect("the handler must return without reading the rest of the body");

    // The receive side is gone when the peer's kernel refuses further body bytes. A
    // server still draining would accept every one of them.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if client.write(b"x").is_err() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the server kept accepting body bytes after the rejection"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn raw_oversize_keep_alive_is_closed_without_draining() {
    assert_rejected_without_draining("", LIMIT + 1);
}

#[test]
fn raw_oversize_connection_close_is_closed_without_draining() {
    assert_rejected_without_draining("Connection: close\r\n", LIMIT + 1);
}

#[test]
fn raw_oversize_expect_continue_is_closed_without_draining() {
    assert_rejected_without_draining("Expect: 100-continue\r\n", LIMIT + 1);
}

/// The drain used to allocate the whole remaining declared length at once, so a
/// declared terabyte is the allocation probe: it can only pass if nothing is drained.
#[test]
fn raw_oversize_huge_declared_length_allocates_nothing_for_the_rest() {
    assert_rejected_without_draining("", 1 << 40);
}

/// The abuse screen answers 429 before the body is read, so it closes the same way.
/// Without that, a declared length too large to allocate ends the process in the drain.
#[test]
fn raw_blocked_source_is_closed_without_draining() {
    assert_rejection_closes(
        serve_blocked(),
        "",
        1 << 40,
        "HTTP/1.1 429",
        "too many requests",
    );
}

/// Requests that are not rejected keep the keep-alive connection as before.
#[test]
fn raw_accepted_bodies_keep_the_connection_alive() {
    let (addr, done) = serve(2);
    let mut client = TcpStream::connect(addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut body = b"{}".to_vec();
    body.resize(1500, b' ');
    for _ in 0..2 {
        write!(
            client,
            "POST /hook HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        client.write_all(&body).unwrap();
        done.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    let mut response = String::new();
    let mut buf = [0; 1024];
    while response.matches("received").count() < 2 {
        let n = client.read(&mut buf).unwrap();
        assert!(n > 0, "the connection closed early: {response}");
        response.push_str(std::str::from_utf8(&buf[..n]).unwrap());
    }
    assert_eq!(response.matches("HTTP/1.1 200").count(), 2, "{response}");
    assert!(!response.contains("Connection: close"), "{response}");
}
