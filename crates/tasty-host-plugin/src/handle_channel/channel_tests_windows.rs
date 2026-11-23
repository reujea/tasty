//! Windows Named Pipe 인증과 공유 메모리 핸들 전달·수신을 검증한다.
//! 같은 프로세스 안에서 핸들을 복제하고 두 매핑의 바이트를 비교한다.

#![cfg(all(test, windows))]
// 이유: 테스트는 let _ 사유 검사 대상이 아니다.
#![allow(clippy::let_underscore_must_use)]
use std::io;
use std::ptr;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tasty_plugin_protocol::{HandleChannelMessage, SharedBufferId};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING, ReadFile, WriteFile};

use super::HandleListener;

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;

/// 테스트용 raw 파이프 클라이언트(SDK PipeClientStream 최소 재현).
struct RawClient(HANDLE);

impl RawClient {
    fn connect(name: &str) -> Self {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // spawn 직후 인스턴스가 아직 없을 수 있어 짧게 재시도.
        // 루프 뒤 시도를 더하지 않는다. 시도마다 새 파이프 연결을 열고, 횟수 예산이라 검사 스레드가 밀려도
        // 시도 수는 줄지 않는다.
        for _ in 0..50 {
            // SAFETY: Win32 CreateFileW. wide 는 NUL 종단 UTF-16.
            let h = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    0,
                    ptr::null_mut(),
                )
            };
            if h != INVALID_HANDLE_VALUE && !h.is_null() {
                return Self(h);
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("RawClient: could not connect to {name}");
    }

    fn write_line(&self, line: &str) {
        let mut buf = line.as_bytes().to_vec();
        buf.push(b'\n');
        let mut off = 0usize;
        while off < buf.len() {
            let mut written: u32 = 0;
            // SAFETY: 유효 핸들, buf[off..] 유효, written out param.
            let rc = unsafe {
                WriteFile(
                    self.0,
                    buf[off..].as_ptr(),
                    (buf.len() - off) as u32,
                    &mut written,
                    ptr::null_mut(),
                )
            };
            assert!(rc != 0, "WriteFile failed: {}", io::Error::last_os_error());
            off += written as usize;
        }
    }

    /// 개행까지 1바이트씩 읽기(over-read 방지).
    fn read_line(&self) -> String {
        let mut out = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let mut read: u32 = 0;
            // SAFETY: 유효 핸들, byte 유효, read out param.
            let rc = unsafe { ReadFile(self.0, byte.as_mut_ptr(), 1, &mut read, ptr::null_mut()) };
            assert!(rc != 0, "ReadFile failed: {}", io::Error::last_os_error());
            if read == 0 || byte[0] == b'\n' {
                break;
            }
            out.push(byte[0]);
        }
        String::from_utf8(out).expect("utf8 line")
    }
}

impl Drop for RawClient {
    fn drop(&mut self) {
        // SAFETY: 유효 핸들 close.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[test]
fn windows_handle_channel_round_trip() {
    let listener = HandleListener::bind().expect("bind");
    let endpoint = listener.endpoint().to_string();
    let token = "tok-rt-win";

    // 실제 host 흐름과 동일하게 client connect *전에* mailbox 등록(race 방지).
    let rx = listener.register_token(token);

    let (done_tx, done_rx) = mpsc::channel::<(u8, u8, u8)>();
    let ep = endpoint.clone();
    let tok = token.to_string();
    let client = thread::spawn(move || {
        let c = RawClient::connect(&ep);
        c.write_line(&format!("{{\"plugin_id\":\"p\",\"token\":\"{tok}\"}}"));
        let ack = c.read_line();
        assert!(ack.contains("\"ok\":true"), "unexpected ack: {ack}");

        let line = c.read_line();
        let msg: HandleChannelMessage = serde_json::from_str(&line).expect("decode HandleAttach");
        let (handle, size) = match msg {
            HandleChannelMessage::HandleAttach { handle, size, .. } => {
                (handle.expect("windows handle present"), size)
            }
            other => panic!("expected HandleAttach, got {other:?}"),
        };
        // SAFETY: handle은 방금 host가 보낸 HandleAttach 메시지에서 받은, in-band
        // DuplicateHandle 복제 값 — 다른 곳에서 소유되지 않았다.
        let mem = unsafe {
            tasty_shm::receive(tasty_shm::ReceivedPayload::Handle {
                handle,
                size: size as usize,
            })
        }
        .expect("receive");
        // SAFETY: 방금 매핑한 공유 메모리. 단독 접근.
        let slice = unsafe { mem.as_mut_slice() };
        done_tx
            .send((slice[0], slice[1], slice[2]))
            .expect("send result");
        // mem 을 살려둔 채 잠시 대기 — host 가 먼저 drop 해도 매핑이 유효함을 보인다.
        thread::sleep(Duration::from_millis(50));
    });

    // host: 등록된 mailbox 로 stream 수령.
    let mut stream = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("client authenticated + handed off");

    // 실제 공유 메모리 생성 + 패턴 기록.
    let (mem, sendable) = tasty_shm::create(4096).expect("create shm");
    // SAFETY: 송신자가 단독 소유하는 매핑.
    unsafe {
        let s = mem.as_mut_slice();
        s[0] = 0xAB;
        s[1] = 0xCD;
        s[2] = 0xEF;
    }
    // same-process: DuplicateHandle 대상은 현재 프로세스.
    let payload =
        tasty_shm::prepare_send(sendable, tasty_shm::PeerPid::Same).expect("prepare_send");
    let handle = payload.serialized_handle();

    let msg = HandleChannelMessage::HandleAttach {
        request_id: 1,
        id: SharedBufferId(1),
        size: mem.len() as u64,
        handle: None, // send_handle 이 인자 handle 로 덮어씀
    };
    stream.send_handle(&msg, handle).expect("send_handle");

    let got = done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("client mapped + read");
    assert_eq!(got, (0xAB, 0xCD, 0xEF), "client read host-written bytes");

    client.join().expect("client thread");
    drop(payload);
    drop(mem);
}

/// reader가 대기하는 동안에도 write가 완료되는지 확인한다.
/// 같은 file object의 동기 I/O 직렬화를 피하도록 overlapped I/O를 사용한다.
#[test]
fn windows_handle_channel_concurrent_read_write_no_deadlock() {
    let listener = HandleListener::bind().expect("bind");
    let endpoint = listener.endpoint().to_string();
    let token = "tok-duplex";
    let rx = listener.register_token(token);

    // client: 인증 → HandleAttach 수신 확인 → (host reader 를 깨우기 위해) Dirty 송신.
    let (attached_tx, attached_rx) = mpsc::channel::<u64>();
    let ep = endpoint.clone();
    let tok = token.to_string();
    let client = thread::spawn(move || {
        let c = RawClient::connect(&ep);
        c.write_line(&format!("{{\"plugin_id\":\"p\",\"token\":\"{tok}\"}}"));
        assert!(c.read_line().contains("\"ok\":true"));
        let line = c.read_line();
        let msg: HandleChannelMessage = serde_json::from_str(&line).expect("decode");
        let handle = match msg {
            HandleChannelMessage::HandleAttach { handle, .. } => handle.expect("handle"),
            other => panic!("expected HandleAttach, got {other:?}"),
        };
        attached_tx.send(handle).expect("report attach");
        // host reader 가 blocking read 에서 풀리도록 Dirty 한 줄 보낸다.
        c.write_line("{\"kind\":\"dirty\"}");
        thread::sleep(Duration::from_millis(50));
    });

    let stream = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("client handed off");

    // host aux reader 스레드 — 실제 with_handle_stream 이 하는 것처럼 blocking read 진입.
    let reader = stream.reader().expect("split reader");
    let (read_done_tx, read_done_rx) = mpsc::channel::<()>();
    let reader_thread = thread::spawn(move || {
        let mut reader = reader;
        // Dirty 를 받을 때까지 blocking read. write 가 데드락하면 이건 영영 안 온다.
        let _ = reader.recv_message();
        read_done_tx.send(()).ok();
    });
    // reader가 읽기를 시작할 기회를 준다. 진입 완료를 확인하는 동기화는 아니다.
    thread::sleep(Duration::from_millis(100));

    // 이 write 는 reader 의 pending read 뒤에서 직렬화되면 데드락한다(동기 핸들). overlapped
    // 라면 즉시 완료. 별도 스레드에서 수행하고 타임아웃으로 데드락을 감지한다.
    let (write_done_tx, write_done_rx) = mpsc::channel::<()>();
    let writer_thread = thread::spawn(move || {
        let mut stream = stream;
        let (mem, sendable) = tasty_shm::create(4096).expect("create shm");
        let payload =
            tasty_shm::prepare_send(sendable, tasty_shm::PeerPid::Same).expect("prepare_send");
        let msg = HandleChannelMessage::HandleAttach {
            request_id: 1,
            id: SharedBufferId(1),
            size: mem.len() as u64,
            handle: None,
        };
        stream
            .send_handle(&msg, payload.serialized_handle())
            .expect("send_handle");
        write_done_tx.send(()).ok();
        drop(payload);
        drop(mem);
    });

    // 핵심 단언: reader 가 blocking read 중이어도 write 가 타임아웃 안에 완료된다.
    write_done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("send_handle deadlocked behind pending read (sync-IO serialization regression)");
    attached_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("client received HandleAttach");
    read_done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("host reader received Dirty");

    writer_thread.join().expect("writer thread");
    reader_thread.join().expect("reader thread");
    client.join().expect("client thread");
}
