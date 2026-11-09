//! 대화 탭 claude 의 API 트래픽 조절 — loopback CONNECT 프록시와 방향별 토큰 버킷.
//!
//! # 왜
//!
//! 턴마다 대화 전체가 요청 본문으로 올라간다(프롬프트 캐시는 계산을 줄일 뿐 바이트를 줄이지 않는다).
//! 단말기 링크가 상단 장비의 policing · shaping 한도보다 빠르면 그 순간의 버스트가 버려지고, 연결이
//! 끊기면 claude 는 요청을 처음부터 다시 보낸다. 보내는 쪽에서 한도 아래로 흘리면 버려질 것이 없다.
//!
//! # 어떻게
//!
//! claude 는 `HTTPS_PROXY` 를 따른다. 설정 `traffic_on` 이 켜져 있으면 claude 프로필 세션을 띄울 때
//! 이 프로세스의 loopback 프록시 주소를 넣는다. 프록시는 `CONNECT` 만 받고 TLS 를 풀지 않은 채 바이트를
//! 중계한다. 올림(클라이언트 → 서버)과 내림은 각자 버킷 하나를 **이 프로세스 전체**가 나눠 쓴다 —
//! 상단 장비의 한도는 단말기(포트) 단위라서다.
//!
//! - 기존 `HTTPS_PROXY`(프로필 `env` → plugin 환경 순)가 있으면 그 뒤로 이어 보낸다(`CONNECT` 를 한 번 더).
//!   upstream 마다 리스너를 따로 두되 버킷은 같다. `NO_PROXY` 에 든 호스트는 claude 가 프록시를 안 거치므로
//!   조절 밖이다.
//! - upstream 이 `http://` 가 아니면(`https://` 프록시) 이어 보낼 수 없다 — 연결을 깨지 않으려고 그 세션은
//!   조절 없이 띄우고 경고를 남긴다.
//! - 한도는 바로 바뀐다(이미 열린 터널도). 켜는 것은 새로 뜨거나 재개되는 세션부터다 — 환경 변수는 띄울 때만
//!   넣을 수 있다. 끄면 이미 프록시를 쓰는 세션은 한도 없이 통과한다.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// 설정 키 — 매니페스트의 `storage_key` 와 같아야 한다(`manifest_defaults_match_the_code`).
pub const ON_KEY: &str = "traffic_on";
pub const UP_KEY: &str = "traffic_up_kbps";
pub const DOWN_KEY: &str = "traffic_down_kbps";
pub const DEFAULT_ON: bool = false;
pub const DEFAULT_KBPS: f64 = 0.0;
/// 한도의 위 끝(KB/s) — 10GbE 를 넘는 값은 막을 일이 없다.
pub const MAX_KBPS: f64 = 1_250_000.0;

/// 버킷이 한 번에 내줄 수 있는 양 — 한도의 이 비율(초). 상단 장비의 버스트 허용치는 보통 수십~수백 ms
/// 분량이라 그보다 작게 둔다.
const BURST_SECS: f64 = 0.125;
/// 중계 한 번에 읽는 양의 위 끝. 한도가 낮으면 [`chunk_for`] 가 더 줄인다.
const MAX_CHUNK: usize = 16 * 1024;
const MIN_CHUNK: usize = 1024;
/// 요청 머리의 상한 — `CONNECT` 줄과 몇 개의 헤더면 충분하다.
const MAX_HEAD: usize = 8 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// 설정 세 칸.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Limits {
    pub on: bool,
    /// 올림 한도(KB/s, 1 KB = 1,000 바이트) — 0 이면 그 방향은 무제한.
    pub up_kbps: f64,
    pub down_kbps: f64,
}

impl Limits {
    fn bytes_per_sec(kbps: f64) -> u64 {
        if kbps.is_finite() && kbps > 0.0 {
            (kbps.min(MAX_KBPS) * 1000.0) as u64
        } else {
            0
        }
    }
}

/// 방향 하나의 토큰 버킷 — 빚을 지는 방식이다. 가져간 만큼 빼고, 모자라면 그 빚을 갚을 시간만큼 잔다.
/// 여러 터널이 같은 버킷을 나눠 쓰므로 합이 한도를 넘지 않는다.
struct Bucket {
    rate: AtomicU64,
    state: Mutex<(f64, Instant)>,
}

impl Bucket {
    fn new() -> Self {
        Self {
            rate: AtomicU64::new(0),
            state: Mutex::new((0.0, Instant::now())),
        }
    }

    fn take(&self, n: usize) {
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 {
            return;
        }
        let wait = {
            let mut g = crate::adapters::driven::traffic::lock(&self.state);
            let now = Instant::now();
            let (tokens, wait) = debit(g.0, g.1, now, rate, n);
            *g = (tokens, now);
            wait
        };
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
}

/// 버킷 계산 — 지난 시각부터 채우고(버스트 상한까지) `n` 을 빼서, 남은 토큰과 기다릴 시간을 낸다.
fn debit(tokens: f64, last: Instant, now: Instant, rate: u64, n: usize) -> (f64, Duration) {
    let rate = rate as f64;
    let burst = (rate * BURST_SECS).max(MIN_CHUNK as f64);
    let filled = (tokens + now.saturating_duration_since(last).as_secs_f64() * rate).min(burst);
    let left = filled - n as f64;
    let wait = if left < 0.0 {
        Duration::from_secs_f64(-left / rate)
    } else {
        Duration::ZERO
    };
    (left, wait)
}

/// 한 번에 읽을 양 — 한도가 낮을수록 작게 읽어 버스트를 줄인다.
fn chunk_for(rate: u64) -> usize {
    if rate == 0 {
        return MAX_CHUNK;
    }
    ((rate as f64 * BURST_SECS / 2.0) as usize).clamp(MIN_CHUNK, MAX_CHUNK)
}

/// 이어 보낼 upstream 프록시(`http://[user:pass@]host[:port]`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Upstream {
    pub host: String,
    pub port: u16,
    /// `Proxy-Authorization: Basic` 값(이미 base64).
    pub auth: Option<String>,
}

impl Upstream {
    /// 환경 변수 값을 읽는다 — `http://` 만 이어 보낼 수 있다. 다른 스킴은 `Err`(조절 없이 띄운다).
    pub fn parse(url: &str) -> Result<Self, String> {
        let url = url.trim();
        let rest = match url.split_once("://") {
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => rest,
            Some((scheme, _)) => return Err(format!("unsupported proxy scheme {scheme}")),
            None => url,
        };
        let rest = rest.split('/').next().unwrap_or_default();
        let (auth, hostport) = match rest.rsplit_once('@') {
            Some((cred, hp)) => (Some(base64(cred.as_bytes())), hp),
            None => (None, rest),
        };
        // `[v6]:port` · `[v6]` · `host:port` · `host`.
        let (host, port) = match hostport.strip_prefix('[') {
            Some(v6) => {
                let (h, tail) = v6
                    .split_once(']')
                    .ok_or_else(|| format!("bad proxy host {hostport}"))?;
                (h, tail.strip_prefix(':'))
            }
            None => match hostport.rsplit_once(':') {
                Some((h, p)) => (h, Some(p)),
                None => (hostport, None),
            },
        };
        let port = match port {
            Some(p) => p
                .parse::<u16>()
                .map_err(|_| format!("bad proxy port {p}"))?,
            None => 80,
        };
        let host = host.to_string();
        if host.is_empty() {
            return Err("empty proxy host".into());
        }
        Ok(Self { host, port, auth })
    }
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for c in input.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= c.len() {
                out.push(T[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// 숫자 — `agent_session.traffic` 이 낸다.
#[derive(Default)]
struct Stats {
    up_bytes: AtomicU64,
    down_bytes: AtomicU64,
    open: AtomicU64,
    total: AtomicU64,
    failed: AtomicU64,
}

/// 프로세스 하나에 하나 — 버킷 · 리스너 · 숫자.
pub struct Traffic {
    on: AtomicBool,
    limits: Mutex<Limits>,
    up: Arc<Bucket>,
    down: Arc<Bucket>,
    /// upstream(없으면 직접) 마다 리스너 포트.
    listeners: Mutex<BTreeMap<Option<Upstream>, u16>>,
    stats: Arc<Stats>,
}

static GLOBAL: OnceLock<Traffic> = OnceLock::new();

/// 이 프로세스의 조절기.
pub fn global() -> &'static Traffic {
    GLOBAL.get_or_init(Traffic::new)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Traffic {
    fn new() -> Self {
        Self {
            on: AtomicBool::new(false),
            limits: Mutex::new(Limits::default()),
            up: Arc::new(Bucket::new()),
            down: Arc::new(Bucket::new()),
            listeners: Mutex::new(BTreeMap::new()),
            stats: Arc::new(Stats::default()),
        }
    }

    /// 설정을 반영한다 — 열린 터널에도 바로 걸린다. 꺼지면 두 방향 모두 무제한이다.
    pub fn apply(&self, l: Limits) {
        self.on.store(l.on, Ordering::Relaxed);
        let (up, down) = if l.on {
            (
                Limits::bytes_per_sec(l.up_kbps),
                Limits::bytes_per_sec(l.down_kbps),
            )
        } else {
            (0, 0)
        };
        self.up.rate.store(up, Ordering::Relaxed);
        self.down.rate.store(down, Ordering::Relaxed);
        *lock(&self.limits) = l;
    }

    /// 띄울 claude 에 줄 프록시 주소 — 꺼졌거나 이을 수 없는 upstream 이면 `None`(환경을 안 건드린다).
    /// `upstream` 은 그 자식이 원래 받을 `HTTPS_PROXY` 값이다.
    pub fn proxy_url(&self, upstream: Option<&str>) -> Option<String> {
        if !self.on.load(Ordering::Relaxed) {
            return None;
        }
        let up = match upstream
            .filter(|u| !u.trim().is_empty())
            .map(Upstream::parse)
        {
            None => None,
            Some(Ok(u)) => Some(u),
            Some(Err(why)) => {
                tracing::warn!("[agent_session] 트래픽 조절을 건너뛴다 — upstream 프록시: {why}");
                return None;
            }
        };
        match self.listener_for(up) {
            Ok(port) => Some(format!("http://127.0.0.1:{port}")),
            Err(e) => {
                tracing::warn!("[agent_session] 트래픽 조절 프록시를 못 열었다: {e}");
                None
            }
        }
    }

    fn listener_for(&self, upstream: Option<Upstream>) -> std::io::Result<u16> {
        let mut map = lock(&self.listeners);
        if let Some(port) = map.get(&upstream) {
            return Ok(*port);
        }
        let port = serve(
            TcpListener::bind(("127.0.0.1", 0))?,
            upstream.clone(),
            Arc::clone(&self.up),
            Arc::clone(&self.down),
            Arc::clone(&self.stats),
        )?;
        map.insert(upstream, port);
        Ok(port)
    }

    /// `agent_session.traffic` 의 답.
    pub fn report(&self) -> Value {
        let l = *lock(&self.limits);
        let s = &self.stats;
        let proxies: Vec<Value> = lock(&self.listeners)
            .iter()
            .map(|(u, port)| {
                json!({
                    "port": port,
                    "upstream": u.as_ref().map(|u| format!("{}:{}", u.host, u.port)),
                })
            })
            .collect();
        json!({
            "on": l.on,
            "up_kbps": l.up_kbps,
            "down_kbps": l.down_kbps,
            "up_bytes": s.up_bytes.load(Ordering::Relaxed),
            "down_bytes": s.down_bytes.load(Ordering::Relaxed),
            "tunnels_open": s.open.load(Ordering::Relaxed),
            "tunnels_total": s.total.load(Ordering::Relaxed),
            "tunnels_failed": s.failed.load(Ordering::Relaxed),
            "proxies": proxies,
        })
    }
}

/// 리스너를 받는 스레드를 띄우고 그 포트를 낸다.
fn serve(
    listener: TcpListener,
    upstream: Option<Upstream>,
    up: Arc<Bucket>,
    down: Arc<Bucket>,
    stats: Arc<Stats>,
) -> std::io::Result<u16> {
    let port = listener.local_addr()?.port();
    std::thread::Builder::new()
        .name(format!("traffic-proxy-{port}"))
        .spawn(move || {
            for conn in listener.incoming() {
                let Ok(client) = conn else { continue };
                let (upstream, up, down, stats) = (
                    upstream.clone(),
                    Arc::clone(&up),
                    Arc::clone(&down),
                    Arc::clone(&stats),
                );
                let spawned = std::thread::Builder::new()
                    .name("traffic-tunnel".into())
                    .spawn(move || tunnel(client, upstream.as_ref(), &up, &down, &stats));
                if let Err(e) = spawned {
                    tracing::warn!("[agent_session] 트래픽 터널 스레드를 못 띄웠다: {e}");
                }
            }
        })?;
    Ok(port)
}

/// 요청 머리(`\r\n\r\n` 까지)를 읽는다 — 그 뒤에 이미 온 바이트가 있으면 함께 낸다.
fn read_head(s: &mut TcpStream) -> std::io::Result<(String, Vec<u8>)> {
    let mut buf = Vec::with_capacity(512);
    let mut tmp = [0u8; 512];
    loop {
        let n = s.read(&mut tmp)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf.split_off(end + 4);
            return Ok((String::from_utf8_lossy(&buf).into_owned(), rest));
        }
        if buf.len() > MAX_HEAD {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
    }
}

/// `CONNECT host:port HTTP/1.x` 의 대상.
fn connect_target(head: &str) -> Option<&str> {
    let mut it = head.lines().next()?.split_whitespace();
    let (method, target) = (it.next()?, it.next()?);
    (method.eq_ignore_ascii_case("CONNECT") && target.contains(':')).then_some(target)
}

fn dial(target: &str) -> std::io::Result<TcpStream> {
    let mut last = std::io::Error::from(std::io::ErrorKind::NotFound);
    for addr in target.to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// upstream 프록시에 `CONNECT` 를 보내 터널을 연다. 거절이면 그 상태 줄을 낸다.
fn via_upstream(u: &Upstream, target: &str) -> std::io::Result<Result<TcpStream, String>> {
    let mut s = dial(&format!("{}:{}", u.host, u.port))?;
    let mut req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(a) = &u.auth {
        req.push_str(&format!("Proxy-Authorization: Basic {a}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())?;
    let (head, _) = read_head(&mut s)?;
    let status = head.lines().next().unwrap_or_default().to_string();
    if status.split_whitespace().nth(1) == Some("200") {
        Ok(Ok(s))
    } else {
        Ok(Err(status))
    }
}

fn tunnel(
    mut client: TcpStream,
    upstream: Option<&Upstream>,
    up: &Arc<Bucket>,
    down: &Arc<Bucket>,
    stats: &Arc<Stats>,
) {
    stats.total.fetch_add(1, Ordering::Relaxed);
    let fail = |client: &mut TcpStream, status: &str| {
        stats.failed.fetch_add(1, Ordering::Relaxed);
        let reply = format!("{status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        if let Err(e) = client.write_all(reply.as_bytes()) {
            tracing::debug!("[agent_session] 트래픽 프록시 거절을 못 보냈다: {e}");
        }
    };
    let (head, early) = match read_head(&mut client) {
        Ok(h) => h,
        Err(e) => {
            tracing::debug!("[agent_session] 트래픽 프록시 요청을 못 읽었다: {e}");
            stats.failed.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };
    let Some(target) = connect_target(&head).map(str::to_string) else {
        fail(&mut client, "HTTP/1.1 405 Method Not Allowed");
        return;
    };
    let server = match upstream {
        None => dial(&target).map(Ok),
        Some(u) => via_upstream(u, &target),
    };
    let server = match server {
        Ok(Ok(s)) => s,
        Ok(Err(status)) => {
            fail(&mut client, &status);
            return;
        }
        Err(e) => {
            tracing::info!("[agent_session] 트래픽 프록시가 {target} 에 못 닿았다: {e}");
            fail(&mut client, "HTTP/1.1 502 Bad Gateway");
            return;
        }
    };
    if let Err(e) = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n") {
        tracing::debug!("[agent_session] 트래픽 터널 응답을 못 보냈다: {e}");
        return;
    }
    stats.open.fetch_add(1, Ordering::Relaxed);
    let pair = (client.try_clone(), server.try_clone());
    let (Ok(client_r), Ok(server_r)) = pair else {
        stats.open.fetch_sub(1, Ordering::Relaxed);
        stats.failed.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let (up2, st2) = (Arc::clone(up), Arc::clone(stats));
    let mut server_w = server;
    let uploader = std::thread::Builder::new()
        .name("traffic-up".into())
        .spawn(move || {
            if !early.is_empty() {
                up2.take(early.len());
                if server_w.write_all(&early).is_err() {
                    return;
                }
                st2.up_bytes
                    .fetch_add(early.len() as u64, Ordering::Relaxed);
            }
            relay(client_r, &mut server_w, &up2, &st2.up_bytes);
            // 자식이 연결을 닫았다 — 서버가 연결을 열어 둔 채(keep-alive) 있어도 내림 쪽을 끝낸다. TLS 클라이언트는
            // 반만 닫고 답을 기다리지 않는다.
            close(&server_w);
        });
    let mut client_w = client;
    relay(server_r, &mut client_w, down, &stats.down_bytes);
    close(&client_w);
    if let Ok(h) = uploader
        && h.join().is_err()
    {
        tracing::warn!("[agent_session] 트래픽 올림 스레드가 비정상 종료했다");
    }
    stats.open.fetch_sub(1, Ordering::Relaxed);
}

/// 한쪽 방향을 끝까지 옮긴다 — 읽는 쪽이 끝나면 받는 쪽의 쓰기를 닫아 상대에게 끝을 알린다.
fn relay(mut from: TcpStream, to: &mut TcpStream, bucket: &Bucket, counter: &AtomicU64) {
    let mut buf = vec![0u8; MAX_CHUNK];
    loop {
        let chunk = chunk_for(bucket.rate.load(Ordering::Relaxed));
        let n = match from.read(&mut buf[..chunk]) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        bucket.take(n);
        if to.write_all(&buf[..n]).is_err() {
            break;
        }
        counter.fetch_add(n as u64, Ordering::Relaxed);
    }
    if let Err(e) = to.shutdown(Shutdown::Write) {
        tracing::trace!("[agent_session] 트래픽 터널 쓰기 닫기: {e}");
    }
    if let Err(e) = from.shutdown(Shutdown::Read) {
        tracing::trace!("[agent_session] 트래픽 터널 읽기 닫기: {e}");
    }
}

/// 터널 한쪽이 끝나면 소켓을 양쪽 다 닫아 반대 방향 중계도 끝나게 한다.
fn close(s: &TcpStream) {
    if let Err(e) = s.shutdown(Shutdown::Both) {
        tracing::trace!("[agent_session] 트래픽 터널 닫기: {e}");
    }
}

/// 자식이 원래 받을 `HTTPS_PROXY` — 프로필 `env` 가 먼저, 없으면 이 프로세스 환경.
pub fn inherited_proxy(profile_env: &BTreeMap<String, String>) -> Option<String> {
    ["HTTPS_PROXY", "https_proxy"]
        .iter()
        .find_map(|k| profile_env.get(*k).cloned())
        .or_else(|| {
            ["HTTPS_PROXY", "https_proxy"]
                .iter()
                .find_map(|k| std::env::var(k).ok())
        })
        .filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
#[path = "traffic_tests.rs"]
mod tests;
