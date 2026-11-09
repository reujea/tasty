use super::*;

fn at(base: Instant, ms: u64) -> Instant {
    base + Duration::from_millis(ms)
}

/// 버킷은 버스트(한도의 1/8초)까지만 모으고, 모자라면 빚을 갚을 시간만큼 기다리게 한다.
#[test]
fn the_bucket_caps_the_burst_and_charges_the_debt() {
    let t0 = Instant::now();
    // 100 KB/s — 버스트 12,500 바이트.
    let (left, wait) = debit(0.0, t0, at(t0, 10_000), 100_000, 10_000);
    assert_eq!(left, 2_500.0, "오래 쉬어도 버스트 이상 모으면 안 된다");
    assert!(wait.is_zero());
    // 버스트를 넘는 요청은 넘친 만큼 기다린다: 12,500 - 62,500 = -50,000 → 0.5초.
    let (left, wait) = debit(0.0, t0, at(t0, 10_000), 100_000, 62_500);
    assert_eq!(left, -50_000.0);
    assert_eq!(wait, Duration::from_millis(500));
    // 빚은 다음 요청이 갚는다 — 0.1초 뒤 -50,000 + 10,000 = -40,000 에서 1,000 을 더 가져가면 0.41초.
    let (left, wait) = debit(-50_000.0, t0, at(t0, 100), 100_000, 1_000);
    assert!((left + 41_000.0).abs() < 1e-6, "{left}");
    assert!((wait.as_secs_f64() - 0.41).abs() < 1e-6, "{wait:?}");
}

#[test]
fn the_chunk_shrinks_with_the_rate() {
    assert_eq!(chunk_for(0), MAX_CHUNK);
    assert_eq!(chunk_for(10_000_000), MAX_CHUNK);
    assert_eq!(chunk_for(10_000), MIN_CHUNK);
    assert_eq!(chunk_for(100_000), 6_250);
}

#[test]
fn an_upstream_url_is_read_with_its_port_and_credentials() {
    let u = Upstream::parse("http://proxy.corp:3128").unwrap();
    assert_eq!(
        (u.host.as_str(), u.port, u.auth.as_deref()),
        ("proxy.corp", 3128, None)
    );
    let u = Upstream::parse("http://user:p%40ss@proxy.corp:8080/").unwrap();
    assert_eq!(u.port, 8080);
    assert_eq!(u.auth.as_deref(), Some("dXNlcjpwJTQwc3M="));
    let u = Upstream::parse("proxy.corp").unwrap();
    assert_eq!(u.port, 80);
    let u = Upstream::parse("http://[::1]:3128").unwrap();
    assert_eq!((u.host.as_str(), u.port), ("::1", 3128));
    assert!(
        Upstream::parse("https://proxy.corp:443").is_err(),
        "TLS 프록시는 못 잇는다"
    );
    assert!(Upstream::parse("socks5://proxy.corp:1080").is_err());
    assert!(Upstream::parse("http://proxy.corp:notaport").is_err());
}

#[test]
fn base64_matches_the_standard_alphabet() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
}

#[test]
fn only_connect_requests_open_a_tunnel() {
    assert_eq!(
        connect_target("CONNECT api.anthropic.com:443 HTTP/1.1\r\nHost: x\r\n\r\n"),
        Some("api.anthropic.com:443")
    );
    assert_eq!(
        connect_target("GET http://example.com/ HTTP/1.1\r\n\r\n"),
        None
    );
    assert_eq!(connect_target("CONNECT nohostport HTTP/1.1\r\n\r\n"), None);
}

#[test]
fn a_switched_off_controller_leaves_the_child_environment_alone() {
    let t = Traffic::new();
    t.apply(Limits {
        on: false,
        up_kbps: 100.0,
        down_kbps: 100.0,
    });
    assert_eq!(t.proxy_url(None), None);
    assert_eq!(t.up.rate.load(Ordering::Relaxed), 0, "꺼지면 한도도 없다");
    t.apply(Limits {
        on: true,
        up_kbps: 100.0,
        down_kbps: 0.0,
    });
    assert_eq!(t.up.rate.load(Ordering::Relaxed), 100_000);
    assert_eq!(
        t.down.rate.load(Ordering::Relaxed),
        0,
        "0 은 그 방향 무제한"
    );
    assert_eq!(
        t.proxy_url(Some("https://tls.proxy:443")),
        None,
        "못 잇는 upstream 이면 조절 없이 띄운다"
    );
    let url = t.proxy_url(None).expect("켜졌으면 주소를 준다");
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    assert_eq!(t.proxy_url(None), Some(url), "같은 upstream 은 리스너 하나");
}

#[test]
fn the_inherited_proxy_prefers_the_profile() {
    let mut env = BTreeMap::new();
    env.insert("https_proxy".to_string(), "http://p:1".to_string());
    assert_eq!(inherited_proxy(&env).as_deref(), Some("http://p:1"));
    env.insert("HTTPS_PROXY".to_string(), "http://q:2".to_string());
    assert_eq!(inherited_proxy(&env).as_deref(), Some("http://q:2"));
}

/// 받은 것을 그대로 돌려주는 서버.
fn echo_server() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut w = c.try_clone().unwrap();
                let mut r = c;
                if let Err(e) = std::io::copy(&mut r, &mut w) {
                    tracing::warn!("echo server: {e}");
                }
            });
        }
    });
    port
}

/// 프록시를 거쳐 `bytes` 를 보내고 다 돌려받는 데 걸린 시간.
fn round_trip(proxy: u16, target: u16, bytes: usize) -> Duration {
    let mut s = TcpStream::connect(("127.0.0.1", proxy)).unwrap();
    s.write_all(format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\n\r\n").as_bytes())
        .unwrap();
    let (head, _) = read_head(&mut s).unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let started = Instant::now();
    let mut w = s.try_clone().unwrap();
    let writer = std::thread::spawn(move || {
        w.write_all(&vec![7u8; bytes]).unwrap();
    });
    let mut got = vec![0u8; bytes];
    s.read_exact(&mut got).unwrap();
    writer.join().unwrap();
    assert!(got.iter().all(|b| *b == 7));
    started.elapsed()
}

fn controller(up_kbps: f64, down_kbps: f64) -> Traffic {
    let t = Traffic::new();
    t.apply(Limits {
        on: true,
        up_kbps,
        down_kbps,
    });
    t
}

fn port_of(url: &str) -> u16 {
    url.rsplit(':').next().unwrap().parse().unwrap()
}

/// 실제 소켓으로 — 올림 한도 200 KB/s 에 400 KB 는 1.5초 남짓(버스트 25 KB 를 빼고) 걸린다. 한도가 없으면
/// 순식간이다. 내림 한도는 같은 바이트가 돌아오는 길에 따로 걸린다.
#[test]
fn a_tunnel_holds_each_direction_to_its_limit() {
    let echo = echo_server();
    let fast = controller(0.0, 0.0);
    let fast_port = port_of(&fast.proxy_url(None).unwrap());
    assert!(round_trip(fast_port, echo, 400_000) < Duration::from_millis(700));

    let slow = controller(200.0, 0.0);
    let slow_port = port_of(&slow.proxy_url(None).unwrap());
    let took = round_trip(slow_port, echo, 400_000);
    assert!(
        took >= Duration::from_millis(1_700),
        "올림 한도를 안 지켰다: {took:?}"
    );
    assert!(took < Duration::from_secs(5), "너무 느리다: {took:?}");

    let down = controller(0.0, 200.0);
    let down_port = port_of(&down.proxy_url(None).unwrap());
    let took = round_trip(down_port, echo, 400_000);
    assert!(
        took >= Duration::from_millis(1_700),
        "내림 한도를 안 지켰다: {took:?}"
    );

    // 마지막 조각의 숫자는 쓰기 뒤에 오른다 — 받는 쪽이 먼저 끝날 수 있다.
    std::thread::sleep(Duration::from_millis(100));
    let r = slow.report();
    assert_eq!(r["up_bytes"], 400_000);
    assert_eq!(r["down_bytes"], 400_000);
    assert_eq!(r["tunnels_total"], 1);
}

/// 기존 프록시 뒤로 잇는다 — 이 조절기를 upstream 으로 두고 그 앞에 또 하나를 세운다.
#[test]
fn a_tunnel_chains_through_the_inherited_proxy() {
    let echo = echo_server();
    let upstream = controller(0.0, 0.0);
    let up_url = upstream.proxy_url(None).unwrap();
    let front = controller(0.0, 0.0);
    let front_port = port_of(&front.proxy_url(Some(&up_url)).unwrap());
    round_trip(front_port, echo, 10_000);
    assert_eq!(
        upstream.report()["tunnels_total"],
        1,
        "upstream 을 안 거쳤다"
    );
    assert_eq!(
        front.report()["proxies"][0]["upstream"],
        up_url.trim_start_matches("http://")
    );
}

/// upstream 이 거절하면(예: 407 인증 요구) 그 상태 줄을 그대로 돌려준다 — 자식이 원인을 본다.
#[test]
fn an_upstream_refusal_reaches_the_child() {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(String::new()));
    let seen2 = Arc::clone(&seen);
    std::thread::spawn(move || {
        if let Some(Ok(mut c)) = l.incoming().next() {
            let (head, _) = read_head(&mut c).unwrap();
            *seen2.lock().unwrap() = head;
            c.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                .unwrap();
        }
    });
    let front = controller(0.0, 0.0);
    let front_port = port_of(
        &front
            .proxy_url(Some(&format!("http://user:pass@127.0.0.1:{port}")))
            .unwrap(),
    );
    let mut s = TcpStream::connect(("127.0.0.1", front_port)).unwrap();
    s.write_all(b"CONNECT api.example:443 HTTP/1.1\r\n\r\n")
        .unwrap();
    let (head, _) = read_head(&mut s).unwrap();
    assert!(head.starts_with("HTTP/1.1 407"), "{head}");
    let sent = seen.lock().unwrap().clone();
    assert!(
        sent.starts_with("CONNECT api.example:443 HTTP/1.1"),
        "{sent}"
    );
    assert!(
        sent.contains("Proxy-Authorization: Basic dXNlcjpwYXNz"),
        "{sent}"
    );
    assert_eq!(front.report()["tunnels_failed"], 1);
}

#[test]
fn a_plain_request_is_refused() {
    let front = controller(0.0, 0.0);
    let port = port_of(&front.proxy_url(None).unwrap());
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(b"GET http://example.com/ HTTP/1.1\r\n\r\n")
        .unwrap();
    let (head, _) = read_head(&mut s).unwrap();
    assert!(head.starts_with("HTTP/1.1 405"), "{head}");
}

/// 서버가 연결을 끝까지 열어 둬도(keep-alive) 자식이 닫으면 터널이 닫힌다 — 남은 중계 스레드가 쌓이지 않는다.
#[test]
fn a_tunnel_closes_when_the_child_hangs_up() {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let target = l.local_addr().unwrap().port();
    let held = Arc::new(Mutex::new(Vec::new()));
    let held2 = Arc::clone(&held);
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            held2.lock().unwrap().push(c);
        }
    });
    let t = controller(0.0, 0.0);
    let port = port_of(&t.proxy_url(None).unwrap());
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\n\r\n").as_bytes())
        .unwrap();
    let (head, _) = read_head(&mut s).unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(t.report()["tunnels_open"], 1);
    drop(s);
    let deadline = Instant::now() + Duration::from_secs(2);
    while t.report()["tunnels_open"] != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        t.report()["tunnels_open"],
        0,
        "자식이 닫았는데 터널이 남았다"
    );
}
