use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
pub(super) struct Fixture {
    url: String,
    count: Arc<AtomicUsize>,
    upload: Arc<Mutex<Vec<u8>>>,
    release: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Fixture {
    pub(super) fn new() -> Self {
        Self::with_navigation(false)
    }
    pub(super) fn navigating() -> Self {
        Self::with_navigation(true)
    }
    fn with_navigation(allow_final: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let upload = Arc::new(Mutex::new(vec![]));
        let release = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let (c, u, r, s) = (count.clone(), upload.clone(), release.clone(), stop.clone());
        let join = std::thread::spawn(move || {
            'accept: while !s.load(Ordering::Acquire) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                // macOS and Windows hand out accepted sockets that inherit the listener's nonblocking
                // mode, so reads would fail with WouldBlock instead of honoring the timeout below.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut data = vec![];
                let mut chunk = [0; 1024];
                // This thread is the accept loop: a connection that fails or ends early ends only
                // itself, so the requests after it still get an answer.
                let end = loop {
                    let Ok(n) = stream.read(&mut chunk) else {
                        continue 'accept;
                    };
                    if n == 0 {
                        continue 'accept;
                    }
                    data.extend_from_slice(&chunk[..n]);
                    if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                    assert!(data.len() < 65536);
                };
                let headers = String::from_utf8_lossy(&data[..end]);
                assert!(
                    headers.starts_with("POST / HTTP/1.1\r\n")
                        || (allow_final && headers.starts_with("POST /final HTTP/1.1\r\n"))
                );
                let head = headers.to_string();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while data.len() - end < length {
                    let Ok(n) = stream.read(&mut chunk) else {
                        continue 'accept;
                    };
                    if n == 0 {
                        continue 'accept;
                    }
                    data.extend_from_slice(&chunk[..n]);
                }
                *u.lock().unwrap() = data[end..end + length].to_vec();
                captured
                    .lock()
                    .unwrap()
                    .push((head, data[end..end + length].to_vec()));
                c.fetch_add(1, Ordering::AcqRel);
                if stream.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 10\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n").is_err(){continue;}
                while !r.load(Ordering::Acquire) && !s.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(1));
                }
                if !s.load(Ordering::Acquire) {
                    // Cancellation cases deliberately close the socket before this fixture writes.
                    if let Err(error) = stream.write_all(b"owned body") {
                        tracing::debug!(
                            ?error,
                            "HTTP fixture response body write ended after cancellation"
                        );
                    }
                }
            }
        });
        Self {
            url,
            count,
            upload,
            release,
            requests,
            stop,
            join: Some(join),
        }
    }
    pub(super) fn requests(&self) -> Vec<(String, Vec<u8>)> {
        self.requests.lock().unwrap().clone()
    }
    pub(super) fn url(&self) -> String {
        self.url.clone()
    }
    pub(super) fn count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }
    pub(super) fn upload(&self) -> Vec<u8> {
        self.upload.lock().unwrap().clone()
    }
    pub(super) fn release(&self) {
        self.release.store(true, Ordering::Release);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.release();
        self.join.take().unwrap().join().unwrap();
    }
}

/// An empty connection and one cut off in its body each end only themselves: the POST after them
/// is still answered and is the only one counted.
#[test]
fn a_connection_that_ends_early_does_not_end_the_accept_loop() {
    let fixture = Fixture::new();
    let address = fixture
        .url
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();
    drop(TcpStream::connect(&address).unwrap());
    let mut cut_off = TcpStream::connect(&address).unwrap();
    cut_off
        .write_all(b"POST / HTTP/1.1\r\nContent-Length: 4\r\n\r\nab")
        .unwrap();
    drop(cut_off);
    let mut request = TcpStream::connect(&address).unwrap();
    request
        .write_all(b"POST / HTTP/1.1\r\nContent-Length: 4\r\n\r\nbody")
        .unwrap();
    // The fixture answers with the status line and headers, then holds the body until release.
    let mut reply = vec![];
    let mut chunk = [0; 1024];
    while !reply.windows(4).any(|window| window == b"\r\n\r\n") {
        let n = request.read(&mut chunk).unwrap();
        assert!(n > 0, "no answer: {}", String::from_utf8_lossy(&reply));
        reply.extend_from_slice(&chunk[..n]);
    }
    assert!(reply.starts_with(b"HTTP/1.1 201 Created\r\n"));
    assert_eq!(fixture.count(), 1);
    assert_eq!(fixture.upload(), b"body");
}
