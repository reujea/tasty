use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};
#[derive(Clone, Debug)]
pub(super) struct Seen {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) body: Vec<u8>,
    pub(super) headers: String,
}
pub(super) struct Server {
    addr: std::net::SocketAddr,
    pub(super) seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub(super) fn new() -> Self {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        l.set_nonblocking(true).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, t) = (seen.clone(), stop.clone());
        let join = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !t.load(Ordering::Acquire) {
                match l.accept() {
                    Ok((stream, _)) => {
                        let (s, t) = (s.clone(), t.clone());
                        workers.push(std::thread::spawn(move || serve(stream, s, t)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
            for w in workers {
                w.join().unwrap();
            }
        });
        Self {
            addr,
            seen,
            stop,
            join: Some(join),
        }
    }
    pub(super) fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
    pub(super) fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.join.take().unwrap().join().unwrap();
    }
}
fn serve(mut stream: TcpStream, seen: Arc<Mutex<Vec<Seen>>>, stop: Arc<AtomicBool>) {
    // macOS and Windows hand out accepted sockets that inherit the listener's nonblocking
    // mode, so reads would fail with WouldBlock instead of honoring the timeout below.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut raw = Vec::new();
    let mut b = [0; 4096];
    let end = loop {
        match stream.read(&mut b) {
            Ok(0) | Err(_) => return,
            Ok(n) => raw.extend_from_slice(&b[..n]),
        }
        if let Some(i) = raw.windows(4).position(|x| x == b"\r\n\r\n") {
            break i + 4;
        }
        assert!(raw.len() < 100_000)
    };
    let h = String::from_utf8(raw[..end].to_vec()).unwrap();
    let first = h
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    let method = first[0].to_string();
    let path = first[1].to_string();
    let len = h
        .lines()
        .find_map(|l| {
            l.to_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while raw.len() - end < len {
        match stream.read(&mut b) {
            Ok(0) | Err(_) => return,
            Ok(n) => raw.extend_from_slice(&b[..n]),
        }
    }
    let body = raw[end..end + len].to_vec();
    seen.lock().unwrap().push(Seen {
        method: method.clone(),
        path: path.clone(),
        body: body.clone(),
        headers: h,
    });
    if path == "/drop" {
        return;
    }
    let (status, headers, bytes, chunked) = if path == "/redirect" {
        (
            "302 Found",
            "Location: /target\r\n",
            b"redirect body".to_vec(),
            false,
        )
    } else if path == "/gzip" {
        use flate2::{Compression, write::GzEncoder};
        let mut g = GzEncoder::new(Vec::new(), Compression::default());
        g.write_all(&vec![b'x'; 200_000]).unwrap();
        (
            "200 OK",
            "Content-Encoding: gzip\r\n",
            g.finish().unwrap(),
            false,
        )
    } else if let Some(length) = path.strip_prefix("/chunk/") {
        ("200 OK", "", vec![b'x'; length.parse().unwrap()], true)
    } else if path == "/sse" {
        (
            "200 OK",
            "Content-Type: text/event-stream\r\n",
            b"data: x".to_vec(),
            true,
        )
    } else if path == "/slow" {
        ("200 OK", "", b"hello".to_vec(), true)
    } else if path == "/status" {
        (
            "418 Teapot",
            "X-M2: retained\r\nSet-Cookie: first=1\r\nSet-Cookie: second=2\r\nContent-Security-Policy: default-src 'self'\r\n",
            b"status-body".to_vec(),
            false,
        )
    } else {
        ("200 OK", "", body, false)
    };
    let framing = if chunked {
        "Transfer-Encoding: chunked\r\n".into()
    } else {
        format!("Content-Length: {}\r\n", bytes.len())
    };
    if write!(
        stream,
        "HTTP/1.1 {status}\r\n{headers}{framing}Connection: close\r\n\r\n"
    )
    .is_err()
    {
        return;
    }
    if method == "HEAD" {
        return;
    }
    if chunked {
        for c in bytes.chunks(8192) {
            if write!(stream, "{:x}\r\n", c.len())
                .and_then(|_| stream.write_all(c))
                .and_then(|_| stream.write_all(b"\r\n"))
                .is_err()
            {
                return;
            }
        }
        if path == "/slow" {
            while !stop.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(2));
            }
            return;
        }
        // The client may already have cancelled this fixture response.
        stream.write_all(b"0\r\n\r\n").unwrap_or(());
    } else {
        // Fault/cancellation fixtures intentionally close before the server finishes.
        stream.write_all(&bytes).unwrap_or(());
    }
}
