use super::*;

#[test]
fn reload_with_surface_id_returns_ok() {
    let mut p = MarkdownPlugin::new(Translator::default());
    let resp = p
        .markdown_reload(&json!({ "surface": 42 }))
        .expect("reload should succeed");
    assert_eq!(resp["ok"], json!(true));
    assert_eq!(resp["surface_id"], json!(42));
}

#[test]
fn reload_without_surface_id_is_invalid_params() {
    let mut p = MarkdownPlugin::new(Translator::default());
    let err = p.markdown_reload(&json!({})).unwrap_err();
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("surface"));
}

#[test]
fn create_surface_loads_missing_file_as_error() {
    let mut p = MarkdownPlugin::new(Translator::default());
    // SDK 가 넘기는 envelope 형태(file 은 nested `params.file`)로 구성한다.
    p.create_surface(SurfaceCreateCtx {
        surface_id: 1,
        kind: "markdown".into(),
        cwd: None,
        params: json!({
            "surface_id": 1,
            "kind": "markdown",
            "params": { "file": "\0nonexistent-md-for-test" }
        }),
    });
    let doc = p.docs.get(&1).expect("doc inserted");
    assert!(doc.load_error.is_some());
    assert!(doc.content.is_empty());
}

// round-trip 회귀: create 는 file 을 snapshot 으로 실어야 layout/preset 저장에
// file 이 보존된다(그래야 host 가 snapshot_cache→SavedSurface::Generic.data 로
// round-trip). 이게 없으면(SDK 기본 = 빈 SurfaceResult) plugin surface 가 저장 시
// 내용을 잃는다.
#[test]
fn create_surface_carries_file_in_snapshot() {
    let mut p = MarkdownPlugin::new(Translator::default());
    let res = p.create_surface(SurfaceCreateCtx {
        surface_id: 1,
        kind: "markdown".into(),
        cwd: None,
        params: json!({
            "surface_id": 1,
            "kind": "markdown",
            "params": { "file": "/tmp/x.md" }
        }),
    });
    assert_eq!(
        res.snapshot,
        Some(json!({ "file": "/tmp/x.md" })),
        "create 는 file 을 snapshot 으로 실어야 한다"
    );
}

// layout 재시작 복원 경로: restore 는 create 가 실은 snapshot({file}) 을 그대로
// 받아 같은 문서를 열고, snapshot 을 재반환해 다음 저장에도 file 을 유지한다.
#[test]
fn restore_surface_reopens_from_snapshot_and_re_carries_it() {
    let mut p = MarkdownPlugin::new(Translator::default());
    let res = p.restore_surface(SurfaceRestoreCtx {
        surface_id: 2,
        kind: "markdown".into(),
        data: json!({ "file": "/tmp/x.md" }),
    });
    assert!(p.docs.contains_key(&2), "restore 가 문서를 연다");
    assert_eq!(
        res.snapshot,
        Some(json!({ "file": "/tmp/x.md" })),
        "restore 도 snapshot 을 재반환해야 다음 저장에 file 이 남는다"
    );
}

// file 없는 빈 markdown 은 저장할 snapshot 이 없어 None — 호스트는 기존 캐시 유지.
#[test]
fn create_without_file_yields_no_snapshot() {
    let mut p = MarkdownPlugin::new(Translator::default());
    let res = p.create_surface(SurfaceCreateCtx {
        surface_id: 3,
        kind: "markdown".into(),
        cwd: None,
        params: json!({ "surface_id": 3, "kind": "markdown", "params": {} }),
    });
    assert_eq!(res.snapshot, None);
}

/// 크기게이트 임계값 판정 — 초과만 게이트(경계값·이하·부재는 통과). host
/// `file/dispatch.rs` 의 `size_gate_boundary_and_over` 를 plugin in-process 로 이관.
#[test]
fn file_exceeds_limit_gates_over_only() {
    let dir = std::env::temp_dir();
    let big = dir.join(format!("tasty-md-big-{}.md", std::process::id()));
    let exact = dir.join(format!("tasty-md-exact-{}.md", std::process::id()));
    let small = dir.join(format!("tasty-md-small-{}.md", std::process::id()));
    std::fs::write(&big, vec![b'x'; LARGE_FILE_LIMIT_BYTES as usize + 1]).unwrap();
    std::fs::write(&exact, vec![b'x'; LARGE_FILE_LIMIT_BYTES as usize]).unwrap();
    std::fs::write(&small, vec![b'x'; 500 * 1024]).unwrap();

    assert_eq!(
        file_exceeds_limit(big.to_str().unwrap()),
        Some(LARGE_FILE_LIMIT_BYTES + 1)
    );
    // 정확히 임계값 → None (초과만 게이트).
    assert_eq!(file_exceeds_limit(exact.to_str().unwrap()), None);
    assert_eq!(file_exceeds_limit(small.to_str().unwrap()), None);
    // 없는 파일 → None (게이트 통과, 로드 시 error 표시).
    assert_eq!(file_exceeds_limit("\0nonexistent-md-for-test"), None);

    let _ = std::fs::remove_file(&big); // best-effort 정리 — 실패 무시(테스트 결과 무관).
    let _ = std::fs::remove_file(&exact); // best-effort 정리 — 실패 무시.
    let _ = std::fs::remove_file(&small); // best-effort 정리 — 실패 무시.
}

/// deferred 문서는 [열기] 확정(`resume_load`) 전까지 read 를 보류한다.
#[test]
fn deferred_doc_holds_read_until_resume() {
    let path = std::env::temp_dir().join(format!("tasty-md-deferred-{}.md", std::process::id()));
    std::fs::write(&path, b"# hello deferred").unwrap();
    let mut doc = MdDoc::new_deferred(Some(path.to_string_lossy().into_owned()));
    assert!(doc.pending_large);
    assert!(doc.content.is_empty());
    // 확정 후 실제 로드.
    doc.resume_load();
    assert!(!doc.pending_large);
    assert!(doc.content.contains("hello deferred"));
    // best-effort 정리 — 실패해도 테스트 결과에 영향 없음.
    let _ = std::fs::remove_file(&path);
}

/// `force_reload` 가 외부 삭제를 error 상태로 감지한다 — idle 감시(SDK `file_watch`)가
/// mtime 변경을 감지했을 때, 그리고 `markdown.reload` IPC 가 명시 호출됐을 때 모두
/// 이 경로 하나로 수렴한다(`file_watch` 모듈 문서 — 레이스를 없애는 단일 쓰기 경로).
#[test]
fn force_reload_detects_external_deletion_as_error() {
    let path = std::env::temp_dir().join(format!("tasty-md-delpoll-{}.md", std::process::id()));
    std::fs::write(&path, b"# hello poll").unwrap();
    let mut doc = MdDoc::new(Some(path.to_string_lossy().into_owned()));
    // 정상 로드 baseline.
    assert!(!doc.content.is_empty());
    assert!(doc.load_error.is_none());

    // 외부 삭제 → force_reload 가 read_now 실패를 load_error 로 남긴다.
    std::fs::remove_file(&path).unwrap();
    doc.force_reload();
    assert!(
        doc.load_error.is_some(),
        "삭제가 error 상태로 감지되어야 한다"
    );

    // best-effort 정리 — 이미 삭제되었을 수 있음.
    let _ = std::fs::remove_file(&path);
}

#[test]
fn format_size_and_basename_examples() {
    assert_eq!(format_size(2 * 1024 * 1024 + 200 * 1024), "2.2 MB");
    assert_eq!(format_size(12 * 1024 * 1024), "12 MB");
    #[cfg(not(windows))]
    assert_eq!(basename("/docs/big-notes.md"), "big-notes.md");
}

#[test]
fn parse_recent_extracts_paths_in_order() {
    let v = json!({ "recent": [
        { "path": "/a/first.md", "file_name": "first.md" },
        { "path": "/b/second.md", "file_name": "second.md" },
    ]});
    assert_eq!(parse_recent(&v), vec!["/a/first.md", "/b/second.md"]);
}

#[test]
fn parse_recent_tolerates_missing_or_malformed() {
    assert!(parse_recent(&json!({})).is_empty());
    assert!(parse_recent(&json!({ "recent": "nope" })).is_empty());
    // path 없는 항목은 건너뛴다.
    assert_eq!(
        parse_recent(&json!({ "recent": [{ "file_name": "x" }, { "path": "/ok.md" }] })),
        vec!["/ok.md"]
    );
}

#[test]
fn surface_param_file_reads_nested_and_flat() {
    assert_eq!(
        surface_param_file(&json!({ "params": { "file": "/a/b.md" } })).as_deref(),
        Some("/a/b.md")
    );
    assert_eq!(
        surface_param_file(&json!({ "file": "/c/d.md" })).as_deref(),
        Some("/c/d.md")
    );
    assert_eq!(surface_param_file(&json!({ "params": {} })), None);
}

/// `reload_webview` 는 `self.host` 가 없으면(on_start 전) 조용히 no-op 해야 한다 —
/// panic 하지 않고 경고 로그만 남긴다.
#[test]
fn reload_webview_without_host_is_noop() {
    let mut p = MarkdownPlugin::new(Translator::default());
    p.docs.insert(7, MdDoc::new(None));
    p.reload_webview(7); // host 없음 — panic 하지 않아야 한다.
}

fn mirror_create(p: &mut MarkdownPlugin, surface_id: u32, file: &str) -> SurfaceResult {
    p.create_surface(SurfaceCreateCtx {
        surface_id,
        kind: "markdown".into(),
        cwd: None,
        params: json!({
            "surface_id": surface_id,
            "kind": "markdown",
            "params": { "display_name": "notes.md", "remote": { "file": file } }
        }),
    })
}

fn content_result(
    surface_id: u32,
    request_id: u64,
    ok: bool,
    source: &str,
) -> MirrorContentResultWire {
    MirrorContentResultWire {
        surface_id,
        request_id,
        ok,
        source: ok.then(|| source.to_string()),
        reason: (!ok).then(|| source.to_string()),
    }
}

/// mirror 문서는 경로가 이 머신에 실재해도 읽지 않는다 — 그 경로는 원격 호스트의 것이다.
#[test]
fn remote_create_never_reads_the_local_file() {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("tasty-md-remote-{}-{seq}.md", std::process::id()));
    std::fs::write(&path, b"# local content").unwrap();
    let mut p = MarkdownPlugin::new(Translator::default());
    let res = mirror_create(&mut p, 5, &path.to_string_lossy());
    let doc = p.docs.get(&5).expect("doc inserted");
    assert!(doc.content.is_empty(), "로컬 파일을 읽으면 안 된다");
    assert!(doc.load_error.is_none());
    assert!(
        doc.base_dir.is_none(),
        "상대경로가 로컬 디렉토리로 풀리면 안 된다"
    );
    assert_eq!(
        doc.file_path.as_deref(),
        Some(path.to_string_lossy().as_ref())
    );
    assert!(doc.remote.is_some());
    // mirror 워크스페이스는 저장되지 않는다 — 원격 경로를 로컬 layout 에 남기지 않는다.
    assert_eq!(res.snapshot, None);

    // 재읽기 경로(idle 감시·markdown.reload)도 로컬 read 로 흘러가지 않는다.
    p.docs.get_mut(&5).unwrap().force_reload();
    assert!(p.docs[&5].content.is_empty());
    let resp = p.markdown_reload(&json!({ "surface": 5 })).unwrap();
    assert_eq!(resp["ok"], json!(true));
    assert!(p.docs[&5].content.is_empty());

    let _ = std::fs::remove_file(&path); // best-effort 정리 — 실패 무시(테스트 결과 무관).
}

#[test]
fn surface_param_remote_file_requires_the_remote_object() {
    assert_eq!(
        surface_param_remote_file(&json!({ "params": { "remote": { "file": "/r/a.md" } } }))
            .as_deref(),
        Some("/r/a.md")
    );
    // 원격이 파일 없이 연 문서도 mirror 문서다.
    assert_eq!(
        surface_param_remote_file(&json!({ "params": { "remote": {} } })).as_deref(),
        Some("")
    );
    assert_eq!(
        surface_param_remote_file(&json!({ "params": { "file": "/l/a.md" } })),
        None
    );
    assert_eq!(
        surface_param_remote_file(&json!({ "remote": { "file": "/r/a.md" } })),
        None
    );
}

/// restore data 는 envelope 이 아니라 params 객체 그 자체다 — kind 대기 placeholder 가
/// 실제화될 때 host 가 생성 params 와 같은 모양을 싣는다. 로컬 문서의 snapshot
/// (`{"file": ...}`)은 mirror 문서로 읽히면 안 된다.
#[test]
fn remote_file_of_reads_restore_data_and_ignores_local_snapshots() {
    assert_eq!(
        remote_file_of(&json!({ "display_name": "a.md", "remote": { "file": "/r/a.md" } }))
            .as_deref(),
        Some("/r/a.md")
    );
    assert_eq!(remote_file_of(&json!({ "file": "/l/a.md" })), None);
}

#[test]
fn remote_result_applies_only_to_the_pending_request() {
    let mut doc = MdDoc::new_remote("/r/notes.md".into());
    // 대기 중인 요청이 없으면 아무것도 반영하지 않는다.
    assert!(!doc.apply_remote_result(content_result(1, 7, true, "# a")));
    assert!(doc.content.is_empty());

    doc.remote.as_mut().unwrap().pending = Some(8);
    doc.remote.as_mut().unwrap().stale = true;
    // 옛 요청의 늦은 회신은 버린다.
    assert!(!doc.apply_remote_result(content_result(1, 7, true, "# old")));
    assert!(doc.content.is_empty());

    assert!(doc.apply_remote_result(content_result(1, 8, true, "# new")));
    assert_eq!(doc.content, "# new");
    let remote = doc.remote.as_ref().unwrap();
    assert_eq!(remote.pending, None);
    assert!(remote.loaded);
    assert!(!remote.stale, "최신 원문을 받으면 stale 표시가 풀린다");
    assert_eq!(
        doc.remote_view(),
        Some(render::RemoteView {
            loading: false,
            stale: false,
            disconnected: false,
        })
    );
}

#[test]
fn remote_failure_moves_reason_into_load_error() {
    let mut doc = MdDoc::new_remote("/r/notes.md".into());
    doc.remote.as_mut().unwrap().pending = Some(3);
    assert!(doc.apply_remote_result(content_result(1, 3, false, "permission denied")));
    assert_eq!(doc.load_error.as_deref(), Some("permission denied"));
    assert_eq!(doc.remote.as_ref().unwrap().pending, None);
}

/// 연결이 끊겨 회신이 영영 안 올 때 host 가 보내는 abandon 은 기다리던 요청을 끝내고 문서를
/// 끊김 상태로 둔다 — 이미 원문을 받은 문서도 그렇다. 받은 원문은 지우지 않고, 다음 성공
/// 회신이 끊김을 푼다.
#[test]
fn abandon_marks_the_document_disconnected_even_after_content_arrived() {
    let mut loading = MdDoc::new_remote("/r/notes.md".into());
    loading.remote.as_mut().unwrap().pending = Some(4);
    assert!(loading.apply_remote_result(content_result(
        1,
        MIRROR_ABANDON_REQUEST_ID,
        false,
        "gone"
    )));
    assert_eq!(loading.remote.as_ref().unwrap().pending, None);
    assert!(loading.remote_view().unwrap().disconnected);

    let mut loaded = MdDoc::new_remote("/r/notes.md".into());
    loaded.remote.as_mut().unwrap().pending = Some(5);
    assert!(loaded.apply_remote_result(content_result(1, 5, true, "# kept")));
    assert!(
        loaded.apply_remote_result(content_result(1, MIRROR_ABANDON_REQUEST_ID, false, "gone")),
        "원문을 받은 문서도 끊김을 그리러 다시 그린다"
    );
    assert!(loaded.remote_view().unwrap().disconnected);
    assert_eq!(loaded.content, "# kept", "받은 원문은 지우지 않는다");
    assert!(
        !loaded.apply_remote_result(content_result(1, MIRROR_ABANDON_REQUEST_ID, false, "gone")),
        "이미 끊김이면 다시 그리지 않는다"
    );

    loaded.remote.as_mut().unwrap().pending = Some(6);
    assert!(loaded.apply_remote_result(content_result(1, 6, true, "# back")));
    assert!(!loaded.remote_view().unwrap().disconnected);
    assert_eq!(loaded.content, "# back");
}

#[test]
fn change_signal_marks_a_shown_document_stale_once_and_ignores_local_documents() {
    let mut doc = MdDoc::new_remote("/r/notes.md".into());
    doc.remote.as_mut().unwrap().pending = Some(1);
    assert!(doc.apply_remote_result(content_result(1, 1, true, "# shown")));
    assert_eq!(doc.on_remote_changed(), RemoteChange::Redraw);
    assert_eq!(
        doc.on_remote_changed(),
        RemoteChange::Ignore,
        "이미 stale 이면 다시 그리지 않는다"
    );
    assert_eq!(doc.content, "# shown", "신호만으로 원문을 다시 받지 않는다");
    assert_eq!(MdDoc::new(None).on_remote_changed(), RemoteChange::Ignore);
}

/// 원문 대신 끊김·실패를 보여 주는 문서는 신호에 다시 요청한다 — 재연결 직후 host 가 보내는
/// 신호가 끊김 화면을 스스로 걷어내는 경로다. 이미 받는 중이면 그 회신을 기다린다.
#[test]
fn change_signal_refetches_a_document_that_shows_no_content() {
    let mut disconnected = MdDoc::new_remote("/r/notes.md".into());
    disconnected.remote.as_mut().unwrap().pending = Some(1);
    assert!(disconnected.apply_remote_result(content_result(1, 1, true, "# shown")));
    assert!(disconnected.apply_remote_result(content_result(
        1,
        MIRROR_ABANDON_REQUEST_ID,
        false,
        "gone"
    )));
    assert_eq!(disconnected.on_remote_changed(), RemoteChange::Refetch);
    assert!(
        !disconnected.remote.as_ref().unwrap().stale,
        "끊김 화면에는 stale 표시를 켜지 않는다"
    );

    let mut failed = MdDoc::new_remote("/r/notes.md".into());
    failed.remote.as_mut().unwrap().pending = Some(2);
    assert!(failed.apply_remote_result(content_result(1, 2, false, "permission denied")));
    assert_eq!(failed.on_remote_changed(), RemoteChange::Refetch);

    failed.remote.as_mut().unwrap().pending = Some(3);
    assert_eq!(failed.on_remote_changed(), RemoteChange::Ignore);
}

// ---- 찾아보기… 의 출발 폴더 ----

#[test]
fn picker_start_reads_local_observed_cwd_not_the_gated_cwd() {
    // `inherit_cwd` 가 꺼져 `cwd` 가 null 이어도 피커는 보고 있던 폴더에서 출발한다.
    let start = PickerStart::from_context(&json!({
        "cwd": null,
        "observed_cwd": "/work/proj",
        "origin_surface_id": 7,
    }));
    assert_eq!(start.dir.as_deref(), Some("/work/proj"));
    assert_eq!(start.origin_surface_id, Some(7));
}

#[test]
fn picker_start_on_mirror_reads_remote_cwd_only() {
    // mirror 면 원격 경로 키만 읽는다 — 로컬 키가 섞여 와도 원격 피커에 로컬 경로를 싣지 않는다.
    let start = PickerStart::from_context(&json!({
        "mirror": true,
        "local_surface_id": 9,
        "origin_surface_id": 9,
        "remote_cwd": "/srv/remote",
        "observed_cwd": "/should/not/be/used",
    }));
    assert_eq!(start.dir.as_deref(), Some("/srv/remote"));
    assert_eq!(start.origin_surface_id, Some(9));
}

#[test]
fn picker_start_tolerates_missing_keys() {
    // 구버전 host(`cwd` 만 싣는다) — 깨지지 않고 비어 있다.
    assert_eq!(
        PickerStart::from_context(&json!({ "cwd": "/old" })),
        PickerStart::default()
    );
}

#[cfg(any(unix, windows))]
#[test]
fn trigger_params_carry_start_dir_and_origin() {
    let start = PickerStart {
        dir: Some("/work/proj".to_string()),
        origin_surface_id: Some(7),
    };
    let params = popup::file_picker_trigger_params(42, &start);
    assert_eq!(params["owner_popup_instance"], json!(42));
    assert_eq!(params["start_dir"], json!("/work/proj"));
    assert_eq!(params["origin_surface_id"], json!(7));
    assert_eq!(params["filters"], json!(["md", "markdown"]));
}

/// 파일열기 팝업 [열기] 는 자기 팝업을 실어 보낸다 — host 가 이것으로 사용자 조작을 알아본다.
/// 빠지면 사용자가 연 탭이 에이전트 요청으로 분류된다(ADR-0526).
#[cfg(any(unix, windows))]
#[test]
fn file_open_dispatch_params_carry_the_owner_popup() {
    let params = popup::file_open_dispatch_params(42, "/work/a.md");
    assert_eq!(params["owner_popup_instance"], json!(42));
    assert_eq!(params["path"], json!("/work/a.md"));
    assert_eq!(params["depth"], json!("deep"));
    assert!(params.get("origin_surface_id").is_none());
}

/// 외부 링크는 host `webview.open_external` 로 간다 — host 가 읽는 두 키(`surface_id` · `url`)를
/// 싣는다. 이 plugin 은 OS 열기를 직접 하지 않는다(ADR-0527).
#[test]
fn external_link_params_name_the_clicked_surface_and_the_url() {
    let params = external_link_params(7, "https://example.com/a?b=1");
    assert_eq!(params["surface_id"], json!(7));
    assert_eq!(params["url"], json!("https://example.com/a?b=1"));
}

/// 원격 원문 요청은 에이전트가 건 것에만 `agent_origin` 을 싣는다 — host 는 그 회신의 잘림
/// toast 를 사용자에게 띄우지 않는다(ADR-0503). plugin 이 건 요청은 종전 모양 그대로다.
#[test]
fn only_an_agent_content_request_carries_the_agent_origin() {
    assert_eq!(
        mirror_content_request_params(3, RemoteRequester::Agent),
        json!({ "surface_id": 3, "agent_origin": true })
    );
    assert_eq!(
        mirror_content_request_params(3, RemoteRequester::Plugin),
        json!({ "surface_id": 3 })
    );
}
