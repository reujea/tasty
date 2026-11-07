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
    body: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Fixture {
    pub(super) fn new(length: usize, chunked: bool) -> Self {
        Self::encoded(length, chunked, false, false)
    }
    pub(super) fn gzip(length: usize) -> Self {
        Self::encoded(length, false, true, false)
    }
    pub(super) fn bad_gzip() -> Self {
        Self::encoded(1, false, true, true)
    }
    fn encoded(length: usize, chunked: bool, gzip: bool, bad_gzip: bool) -> Self {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        let url = format!("http://{}/", l.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let body = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (c, b, s) = (count.clone(), body.clone(), stop.clone());
        let join = std::thread::spawn(move || {
            while !s.load(Ordering::Acquire) {
                match l.accept() {
                    Ok((stream, _)) => serve(stream, length, chunked, gzip, bad_gzip, &c, &b),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url,
            count,
            body,
            stop,
            join: Some(join),
        }
    }
    pub(super) fn url(&self) -> String {
        self.url.clone()
    }
    pub(super) fn count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }
    pub(super) fn body(&self) -> Vec<u8> {
        self.body.lock().unwrap().clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.join.take().unwrap().join().unwrap();
    }
}
fn serve(
    mut s: TcpStream,
    length: usize,
    chunked: bool,
    gzip: bool,
    bad_gzip: bool,
    count: &AtomicUsize,
    body: &Mutex<Vec<u8>>,
) {
    // macOS and Windows hand out accepted sockets that inherit the listener's nonblocking
    // mode, so reads would fail with WouldBlock instead of honoring the timeout below.
    s.set_nonblocking(false).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut data = Vec::new();
    let mut buf = [0; 8192];
    let end = loop {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => data.extend_from_slice(&buf[..n]),
        };
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        assert!(data.len() < 100000);
    };
    let headers = String::from_utf8_lossy(&data[..end]);
    let n = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|n| n.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while data.len() - end < n {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => data.extend_from_slice(&buf[..n]),
        }
    }
    *body.lock().unwrap() = data[end..end + n].to_vec();
    count.fetch_add(1, Ordering::AcqRel);
    let bytes = if bad_gzip {
        vec![b'x']
    } else if gzip {
        use flate2::{Compression, write::GzEncoder};
        let mut g = GzEncoder::new(Vec::new(), Compression::default());
        g.write_all(&vec![b'x'; length]).unwrap();
        g.finish().unwrap()
    } else {
        vec![b'x'; length]
    };
    let representation = if gzip {
        "Content-Encoding: gzip\r\n"
    } else {
        ""
    };
    let framing = if chunked {
        "Transfer-Encoding: chunked\r\n".into()
    } else {
        format!("Content-Length: {}\r\n", bytes.len())
    };
    if write!(
        s,
        "HTTP/1.1 200 OK\r\n{framing}{representation}Connection: close\r\n\r\n"
    )
    .is_err()
    {
        return;
    }
    if chunked {
        for c in bytes.chunks(8192) {
            if write!(s, "{:x}\r\n", c.len())
                .and_then(|_| s.write_all(c))
                .and_then(|_| s.write_all(b"\r\n"))
                .is_err()
            {
                return;
            }
        }
        s.write_all(b"0\r\n\r\n").unwrap_or(());
    } else {
        s.write_all(&bytes).unwrap_or(());
    }
}
