use std::{
    io::{Read, Write},
    net::TcpListener,
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
            while !s.load(Ordering::Acquire) {
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
                let end = loop {
                    let Ok(n) = stream.read(&mut chunk) else {
                        return;
                    };
                    if n == 0 {
                        return;
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
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
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
