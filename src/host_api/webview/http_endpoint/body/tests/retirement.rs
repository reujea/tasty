use super::*;
async fn request() -> (Arc<NativeRequest>, NativeBudget) {
    let (mut client, mut server) = tokio::io::duplex(1000);
    client
        .write_all(
            b"POST /post HTTP/1.1\r\nHost: remote.example:8080\r\nContent-Length: 3\r\n\r\n\0\xffx",
        )
        .await
        .unwrap();
    let budget = NativeBudget::new(1_000_000);
    let request = html_http_native::read_request(
        &mut server,
        &Destination::new("http", "remote.example", 8080).unwrap(),
        NativeLimits {
            upload_bytes: 1000,
            io_timeout: Duration::from_secs(1),
        },
        &budget,
        &|| true,
    )
    .await
    .unwrap();
    (Arc::new(request), budget)
}
#[tokio::test]
async fn retirement_wakes_pending_native_handler_and_drops_waiter_without_allocating_an_id() {
    let (sender, rx) = channel();
    let (mirror, view) = mirror(&sender, "http://remote.example:8080/post");
    let bridge = bridge(sender, &view);
    let source = BodySource::new(bridge.clone(), view, Arc::new(|| true));
    let (request, budget) = request().await;
    let handler = source.handler();
    let pending = tokio::spawn(handler(request.clone()));
    let wait_until = std::time::Instant::now() + Duration::from_secs(1);
    // 기한에 닿으면 한 번 더 읽고 그 값으로 판정한다. timeout 으로 감싸면 만료 때 다시 읽지 않는다.
    loop {
        if bridge.snapshot().receiving {
            break;
        }
        if std::time::Instant::now() >= wait_until {
            assert!(
                bridge.snapshot().receiving,
                "not reached within {:?}: bridge.snapshot().receiving",
                Duration::from_secs(1)
            );
            break;
        }
        tokio::task::yield_now().await;
    }
    source.retire();
    let result = tokio::time::timeout(Duration::from_secs(1), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(EndpointError::Retired)));
    bridge.drive(|| panic!("retired native waiter allocated an ID"), Err);
    assert!(!bridge.snapshot().receiving);
    assert!(rx.try_recv().is_err());
    drop(handler);
    drop(request);
    drop(source);
    drop(bridge);
    drop(mirror);
    assert_eq!(budget.used(), 0);
}
#[tokio::test]
async fn poisoned_replay_state_wakes_an_existing_native_waiter_without_allocating_an_id() {
    let (sender, rx) = channel();
    let (mirror, view) = mirror(&sender, "http://remote.example:8080/post");
    let bridge = bridge(sender, &view);
    let source = BodySource::new(bridge.clone(), view, Arc::new(|| true));
    let (request, budget) = request().await;
    let handler = source.handler();
    let pending = tokio::spawn(handler(request.clone()));
    let wait_until = std::time::Instant::now() + Duration::from_secs(1);
    // 기한에 닿으면 한 번 더 읽고 그 값으로 판정한다. timeout 으로 감싸면 만료 때 다시 읽지 않는다.
    loop {
        if bridge.snapshot().receiving {
            break;
        }
        if std::time::Instant::now() >= wait_until {
            assert!(
                bridge.snapshot().receiving,
                "not reached within {:?}: bridge.snapshot().receiving",
                Duration::from_secs(1)
            );
            break;
        }
        tokio::task::yield_now().await;
    }
    let poison = source.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = poison.replay.lock().unwrap();
            panic!("injected replay poison");
        })
        .join()
        .is_err()
    );
    assert!(matches!(
        handler(request.clone()).await,
        Err(EndpointError::Body)
    ));
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), pending)
            .await
            .unwrap()
            .unwrap(),
        Err(EndpointError::Retired)
    ));
    bridge.drive(|| panic!("poisoned native waiter allocated an ID"), Err);
    assert!(!bridge.snapshot().receiving);
    assert!(rx.try_recv().is_err());
    drop(handler);
    drop(request);
    drop(source);
    drop(bridge);
    drop(mirror);
    assert_eq!(budget.used(), 0);
}

#[tokio::test]
async fn typed_view_replacement_refuses_old_native_handler_before_model_admission() {
    let (sender, rx) = channel();
    let (mut mirror, view) = mirror(&sender, "http://remote.example:8080/post");
    let bridge = bridge(sender, &view);
    let source = BodySource::new(bridge.clone(), view, Arc::new(|| true));
    let next = mirror.http_source().unwrap().new_view().unwrap();
    assert!(next.is_current());
    let (request, budget) = request().await;
    assert!(matches!(
        (source.handler())(request.clone()).await,
        Err(EndpointError::Retired)
    ));
    assert!(bridge.document().is_none());
    assert!(rx.try_recv().is_err());
    drop(request);
    drop(source);
    drop(bridge);
    drop(mirror);
    assert_eq!(budget.used(), 0);
}
