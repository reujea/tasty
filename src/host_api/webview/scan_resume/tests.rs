use std::sync::{Arc, Mutex};

use tasty_model::html_script::HtmlScriptState;

use super::*;

/// 시험에서 deferral 대신 쓰는 표지.
type Token = u32;

struct Doc {
    gate: ScriptGate,
    state: Arc<Mutex<HtmlScriptState>>,
    dir: std::path::PathBuf,
    url: String,
}

impl Drop for Doc {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.dir) {
            tracing::warn!("temp dir cleanup failed: {e}");
        }
    }
}

fn wait<T>(mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(std::time::Instant::now() < deadline, "timed out");
        std::thread::yield_now();
    }
}

/// 허용한 로컬 문서 하나를 화면에 둔 게이트.
fn allowed_doc(tag: &str) -> Doc {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "tasty-scan-resume-{tag}-{}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("doc.html");
    std::fs::write(&path, "<script>a()</script>").expect("write");
    let url = format!("file://{}", path.to_string_lossy().replace('\\', "/"));
    let url = if url.starts_with("file:///") {
        url
    } else {
        url.replacen("file://", "file:///", 1)
    };
    let state = Arc::new(Mutex::new(HtmlScriptState::new(true)));
    let gate = ScriptGate::new(1, state.clone());
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&gate, &url, Some(1), StartKind::NewDocument, true),
        Start::Js(false)
    );
    gate.committed(Some(&url));
    gate.finished();
    wait(|| (!state.lock().expect("state").scan_pending()).then_some(()));
    state.lock().expect("state").allow_current().expect("allow");
    Doc {
        gate,
        state,
        dir,
        url,
    }
}

/// 재로드: 취소 → 스캔 → 같은 종류로 다시 시작 → 그 탐색은 읽어 둔 결과로 JS를 켠다.
#[test]
fn an_allowed_reload_is_cancelled_scanned_and_replayed_as_a_reload() {
    let doc = allowed_doc("reload");
    let mut resume = ScanResume::<Token>::new(1);
    resume.host_request(None);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    assert!(
        resume.completed(Some(10)).internal,
        "내부 취소의 완료는 로드 종료가 아니다"
    );
    assert!(!resume.completed(Some(10)).internal, "한 번만 소비한다");
    let (replay, url) = wait(|| resume.poll());
    assert_eq!((replay, url.as_str()), (Replay::Reload, doc.url.as_str()));
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Resumed(true)
    );
    assert!(
        !resume.completed(Some(11)).internal,
        "다시 시작한 탐색의 완료는 실제 종료다"
    );
    assert!(doc.gate.committed(Some(&doc.url)));
    // 재시작 결과는 한 번만 쓴다. 다음 재로드는 다시 취소하고 읽는다.
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(12), StartKind::Reload, true),
        Start::Cancel
    );
}

/// 내용이 바뀐 파일은 다시 시작해도 켜지 않는다.
#[test]
fn a_changed_file_is_replayed_without_scripts() {
    let doc = allowed_doc("changed");
    std::fs::write(doc.dir.join("doc.html"), "<script>b()</script>").expect("rewrite");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    wait(|| resume.poll());
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Resumed(false)
    );
}

/// 스캔 중 호스트가 새 로드를 요청하면 앞 스캔과 재시작을 버린다.
#[test]
fn a_host_request_during_the_scan_drops_the_replay() {
    let doc = allowed_doc("host");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    resume.host_request(None);
    assert!(!doc.state.lock().expect("state").scan_pending());
    assert!(!resume.waiting());
    assert_eq!(resume.poll(), None);
    assert!(
        resume.completed(Some(10)).internal,
        "취소한 탐색의 완료는 여전히 내부 것이다"
    );
}

/// 읽기가 끝난 뒤 다시 시작하기 전에 다른 탐색이 시작되면 표가 낡아 다시 시작하지 않는다.
#[test]
fn a_navigation_between_the_read_and_the_replay_makes_the_ticket_stale() {
    let doc = allowed_doc("stale");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    // 읽기가 끝나기 전후 어느 때든 다른 탐색이 끼어들면 재시작하지 않는다.
    doc.gate.load_started();
    wait(|| {
        assert_eq!(resume.poll(), None);
        (!resume.waiting()).then_some(())
    });
}

/// 재시작을 기다리는 동안 다른 주소의 탐색이 오면 재시작 결과를 버린다.
/// 그 뒤 원래 주소의 탐색이 와도 새 탐색으로 다시 취소하고 읽는다.
#[test]
fn another_url_while_waiting_for_the_replay_discards_it() {
    let doc = allowed_doc("other");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::NewDocument, true),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Navigate);
    let other = format!("{}.other.html", doc.url);
    assert_eq!(
        resume.start(&doc.gate, &other, Some(11), StartKind::NewDocument, true),
        Start::Js(false)
    );
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(12), StartKind::NewDocument, true),
        Start::Cancel
    );
}

/// 뒤로/앞으로는 호스트가 요청한 방향으로 다시 시작한다. 방향을 모르면 스크립트 없이 진행한다.
#[test]
fn history_steps_replay_in_the_requested_direction_only() {
    let doc = allowed_doc("history");
    let mut resume = ScanResume::<Token>::new(1);
    resume.host_request(Some(HistoryStep::Back));
    assert_eq!(
        resume.start(
            &doc.gate,
            &doc.url,
            Some(10),
            StartKind::BackOrForward,
            true
        ),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Back);
    assert_eq!(
        resume.start(
            &doc.gate,
            &doc.url,
            Some(11),
            StartKind::BackOrForward,
            true
        ),
        Start::Resumed(true)
    );
    doc.gate.committed(Some(&doc.url));
    // 페이지가 시작한 히스토리 이동은 방향을 모른다.
    assert_eq!(
        resume.start(
            &doc.gate,
            &doc.url,
            Some(12),
            StartKind::BackOrForward,
            true
        ),
        Start::Js(false)
    );
    assert!(!doc.state.lock().expect("state").scan_pending());
    assert!(!resume.completed(Some(12)).internal);
}

/// NavigationId를 모르면 내부 취소를 구별할 수 없어 취소하지 않는다.
#[test]
fn an_unknown_navigation_id_is_not_cancelled() {
    let doc = allowed_doc("noid");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, None, StartKind::Reload, true),
        Start::Js(false)
    );
    assert!(!doc.state.lock().expect("state").scan_pending());
}

/// 취소가 실패하면 그 ID를 내부 취소로 보지 않고 대기를 버린다.
#[test]
fn a_failed_cancel_forgets_the_id() {
    let doc = allowed_doc("cancelfail");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    resume.cancel_failed(Some(10));
    assert!(!resume.completed(Some(10)).internal);
    assert!(!resume.waiting());
}

/// 앞으로 이동을 다시 시작한다. 종류를 모르는 탐색(구형 런타임)은 재시작을 알아볼 수 없어
/// 취소하지 않고 스크립트 없이 진행한다.
#[test]
fn forward_replays_and_an_unknown_kind_is_not_cancelled() {
    let doc = allowed_doc("kinds");
    let mut resume = ScanResume::<Token>::new(1);
    resume.host_request(Some(HistoryStep::Forward));
    assert_eq!(
        resume.start(
            &doc.gate,
            &doc.url,
            Some(10),
            StartKind::BackOrForward,
            true
        ),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Forward);
    // 다시 시작하는 호출이 실패하면 기다리던 재시작을 버린다.
    resume.replay_failed();
    assert!(!resume.waiting());
    resume.host_request(None);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Unknown, true),
        Start::Js(false)
    );
    assert!(!resume.completed(Some(11)).internal);
}

/// 재시작을 기다리는 동안 같은 주소지만 종류가 다른 탐색이 먼저 오면 새 탐색이다.
/// 읽어 둔 결과를 쓰지 않고 다시 취소·스캔하며, 뒤에 온 실제 재시작도 새 스캔을 거친다.
#[test]
fn a_same_url_navigation_of_another_kind_does_not_take_the_replay_scan() {
    let doc = allowed_doc("samekind");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Reload);
    // 재시작(Reload) 신호 전에 같은 주소의 새 문서 탐색이 온다.
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::NewDocument, true),
        Start::Cancel,
        "읽어 둔 결과를 쓰지 않는다"
    );
    // 그 뒤에 온 실제 재시작 신호도 기다리는 재시작이 없으므로 새 탐색으로 취소된다.
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(12), StartKind::Reload, true),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Reload);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(13), StartKind::Reload, true),
        Start::Resumed(true)
    );
}

/// 다시 시작한 재로드의 `IsUserInitiated`가 거짓으로 와도 재시작으로 알아본다. Windows 실 기기에서
/// 이 값이 거짓인 회차마다 재시작을 알아보지 못해 상한에 걸리고 허용한 문서가 스크립트 없이
/// 열렸다. 같은 조건의 다른 탐색이 결과를 받아도 파일 판과 문서 요청 지문 확인이 내용을 묶는다.
#[test]
fn a_replay_reported_as_not_user_initiated_is_still_recognised() {
    let doc = allowed_doc("not-user");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, false),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Reload);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, false),
        Start::Resumed(true)
    );
    assert!(
        doc.gate.effective_js(),
        "the allowed document runs its scripts"
    );
}

/// 이 모듈이 취소한 탐색의 ID로 온 시작 신호는 재시작이 아니다.
#[test]
fn a_start_with_the_cancelled_id_is_not_the_replay() {
    let doc = allowed_doc("cancelled-id");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    wait(|| resume.poll());
    assert_ne!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Resumed(true)
    );
}

/// 주소·종류·사용자 시작 여부가 모두 같은 탐색은 재시작인지 알 수 없다(API 한계). 그래서 결과를
/// 파일 판에 묶는다. 판이 그대로면 그 탐색이 여는 내용이 스캔한 내용과 같아 결과를 받는다.
#[test]
fn an_identical_navigation_gets_the_scan_only_for_the_same_file_version() {
    let doc = allowed_doc("identical");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::NewDocument, true),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Navigate);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::NewDocument, true),
        Start::Resumed(true)
    );
}

/// 재시작을 기다리는 동안 같은 주소의 파일을 제자리에서 바꾸면, 그 뒤 오는 탐색은 재시작이든 같은
/// 조건의 사용자 탐색이든 앞 결과를 받지 않는다. 다시 취소하고 자기 스캔으로 JS를 정한다.
#[test]
fn a_file_rewritten_before_the_replay_starts_is_scanned_again() {
    let doc = allowed_doc("rewrite");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Reload);
    let path = doc.dir.join("doc.html");
    let before = FileStamp::of_path(&path).expect("original stamp");
    // mtime 해상도에 기대지 않고 길이를 바꿔 take_resume의 파일 판 차이를 확정한다.
    // 같은 stamp의 내용 변경은 a_same_stamp_rewrite_is_caught_at_the_document_request가 검사한다.
    std::fs::write(&path, "<script>changed_body()</script>").expect("rewrite");
    assert_ne!(FileStamp::of_path(&path), Some(before));
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Cancel,
        "바뀐 파일 판에는 앞 결과를 넘기지 않는다"
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Reload);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(12), StartKind::Reload, true),
        Start::Resumed(false),
        "자기 스캔은 허용한 지문과 달라 JS를 끈다"
    );
}

/// 이름 바꾸기로 같은 내용의 새 파일로 교체해도 다른 파일 판이다. 앞 결과를 넘기지 않고 다시 읽는다.
#[test]
fn a_file_replaced_by_rename_is_another_version() {
    let doc = allowed_doc("rename");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    wait(|| resume.poll());
    let tmp = doc.dir.join("next.html");
    std::fs::write(&tmp, "<script>a()</script>").expect("write");
    std::fs::rename(&tmp, doc.dir.join("doc.html")).expect("rename");
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Cancel
    );
    wait(|| resume.poll());
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(12), StartKind::Reload, true),
        Start::Resumed(true),
        "같은 내용이면 자기 스캔으로 다시 켠다"
    );
}

/// 경로가 사라지면 파일 판을 확인할 수 없어 결과를 넘기지 않는다.
#[test]
fn a_missing_file_gets_no_replay_scan() {
    let doc = allowed_doc("missing");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    wait(|| resume.poll());
    std::fs::remove_file(doc.dir.join("doc.html")).expect("remove");
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Cancel
    );
}

/// 다시 시작한 탐색을 계속 알아보지 못하면(예: 실제 종류가 예상과 다름) 재시작을 두 번까지만 하고
/// 그 뒤에는 취소하지 않고 스크립트 없이 진행한다. 취소와 재시작이 끝없이 이어지지 않는다.
#[test]
fn unrecognised_replays_stop_after_the_limit() {
    let doc = allowed_doc("limit");
    let mut resume = ScanResume::<Token>::new(1);
    // 재시작 신호가 매번 예상과 다른 종류로 오는 경우를 흉내 낸다.
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Reload);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::NewDocument, true),
        Start::Cancel
    );
    assert_eq!(wait(|| resume.poll()).0, Replay::Navigate);
    assert_eq!(MAX_UNMATCHED_REPLAYS, 2);
    let id = 12;
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(id), StartKind::Reload, true),
        Start::Js(false)
    );
    assert!(!doc.state.lock().expect("state").scan_pending());
    // 알아본 재시작이나 호스트 요청 뒤에는 다시 취소·재시작한다.
    resume.host_request(None);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(id + 1), StartKind::Reload, true),
        Start::Cancel
    );
}

/// 같은 길이로 내용을 바꾸고 수정 시각을 되돌린다. 크기·수정 시각·생성 시각·inode가 그대로인
/// 쓰기(Windows에서 쓰기 핸들이 열린 동안의 시각, FAT 2초 단위)를 흉내 낸다.
fn rewrite_keeping_stamp(path: &std::path::Path, body: &str) {
    let before = std::fs::metadata(path).expect("meta");
    std::fs::write(path, body).expect("rewrite");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open");
    file.set_modified(before.modified().expect("mtime"))
        .expect("set mtime");
    assert_eq!(
        FileStamp::of_path(path),
        Some(FileStamp::of(&before)),
        "식별값이 그대로인 쓰기"
    );
}

/// 재시작까지 진행해 결과를 기다리는 상태로 둔다.
fn replay_ready(doc: &Doc, resume: &mut ScanResume<Token>) {
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(10), StartKind::Reload, true),
        Start::Cancel
    );
    assert!(resume.completed(Some(10)).internal);
    wait(|| resume.poll());
}

/// 내용이 그대로면 결과를 받은 탐색의 문서 요청은 지문 확인 뒤 그대로 진행한다.
#[test]
fn the_document_request_of_a_resumed_navigation_proceeds_when_its_fingerprint_matches() {
    let doc = allowed_doc("verify-same");
    let mut resume = ScanResume::<Token>::new(1);
    replay_ready(&doc, &mut resume);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Resumed(true)
    );
    assert_eq!(
        resume.document_request(&doc.gate, &doc.url, || Some(7)),
        RequestAction::Held
    );
    let verified = wait(|| {
        let v = resume.poll_verify();
        (!v.complete.is_empty() || v.stop).then_some(v)
    });
    assert_eq!((verified.complete, verified.stop), (vec![7], false));
    let (stop, release) = resume.content_loading(Some(11));
    assert!(!stop && release.is_empty());
    assert!(doc.gate.committed(Some(&doc.url)));
    assert!(doc.state.lock().expect("state").current_is_allowed());
    let ended = resume.completed(Some(11));
    assert!(!ended.internal && ended.release.is_empty() && ended.restart.is_none());
}

/// 식별값이 같아도 내용이 다르면, 결과를 받은 탐색(재시작이든 같은 조건의 다른 탐색이든)의 문서
/// 요청을 붙잡은 채 탐색을 멈춘다. 그 탐색이 끝난 뒤에야 요청을 놓아 주고, 같은 종류로 다시
/// 시작해 새 내용의 스캔을 받는다. 바뀐 내용은 허용을 이어받지 않는다.
#[test]
fn a_same_stamp_rewrite_is_caught_at_the_document_request() {
    let doc = allowed_doc("verify-diff");
    let mut resume = ScanResume::<Token>::new(1);
    replay_ready(&doc, &mut resume);
    rewrite_keeping_stamp(&doc.dir.join("doc.html"), "<script>b()</script>");
    // 식별값만으로는 구별하지 못해 결과를 준다(이 확인이 막는 경로).
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Resumed(true)
    );
    assert_eq!(
        resume.document_request(&doc.gate, &doc.url, || Some(7)),
        RequestAction::Held
    );
    let verified = wait(|| {
        let v = resume.poll_verify();
        (!v.complete.is_empty() || v.stop).then_some(v)
    });
    assert!(
        verified.stop && verified.complete.is_empty(),
        "본문을 넘기지 않고 멈춘다"
    );
    let ended = resume.completed(Some(11));
    assert!(ended.internal, "멈춘 탐색의 완료는 로드 종료가 아니다");
    assert_eq!(ended.release, vec![7], "탐색이 끝난 뒤에 놓아 준다");
    assert_eq!(ended.restart, Some((Replay::Reload, doc.url.clone())));
    // 다시 시작한 탐색은 새 탐색으로 취소되어 자기 스캔을 받는다.
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(12), StartKind::Reload, true),
        Start::Cancel
    );
    assert!(resume.completed(Some(12)).internal);
    wait(|| resume.poll());
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(13), StartKind::Reload, true),
        Start::Resumed(false),
        "바뀐 내용은 허용을 이어받지 않는다"
    );
    assert_eq!(
        resume.document_request(&doc.gate, &doc.url, || Some(8)),
        RequestAction::Held
    );
    let verified = wait(|| {
        let v = resume.poll_verify();
        (!v.complete.is_empty() || v.stop).then_some(v)
    });
    assert_eq!((verified.complete, verified.stop), (vec![8], false));
    assert!(
        !doc.gate.committed(Some(&doc.url)),
        "새 내용은 JS 없이 commit한다"
    );
    assert!(!doc.state.lock().expect("state").current_is_allowed());
}

/// 확인 창의 탐색이 문서 요청 없이 commit하면 확인하지 못한 본문이므로 멈추고 재로드한다.
/// 요청을 붙잡지 못해도 멈춘다. 다른 파일의 요청이나 창이 없을 때의 요청은 건드리지 않는다.
#[test]
fn an_unchecked_resumed_document_is_stopped_and_reloaded() {
    let doc = allowed_doc("verify-none");
    let mut resume = ScanResume::<Token>::new(1);
    assert_eq!(
        resume.document_request(&doc.gate, &doc.url, || Some(1)),
        RequestAction::Ignore,
        "창이 없다"
    );
    replay_ready(&doc, &mut resume);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Resumed(true)
    );
    let other = doc.url.replace("doc.html", "other.html");
    assert_eq!(
        resume.document_request(&doc.gate, &other, || Some(2)),
        RequestAction::Ignore
    );
    let (stop, _) = resume.content_loading(Some(11));
    assert!(stop, "요청 없이 commit했다");
    let ended = resume.completed(Some(11));
    assert!(ended.internal);
    assert_eq!(ended.restart, Some((Replay::Reload, doc.url.clone())));

    replay_ready(&doc, &mut resume);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(14), StartKind::Reload, true),
        Start::Resumed(true)
    );
    assert_eq!(
        resume.document_request(&doc.gate, &doc.url, || None),
        RequestAction::Stop,
        "붙잡지 못하면 멈춘다"
    );
    assert!(resume.completed(Some(14)).restart.is_some());
}

/// 파일이 계속 바뀌어 지문이 연달아 다르면 상한 뒤 다음 로드를 스크립트 없이 둔다.
#[test]
fn a_file_that_keeps_changing_loads_without_scripts_after_the_limit() {
    let doc = allowed_doc("verify-loop");
    let mut resume = ScanResume::<Token>::new(1);
    let mut id = 10;
    for round in 0..=MAX_VERIFY_FAILURES {
        assert_eq!(
            resume.start(&doc.gate, &doc.url, Some(id), StartKind::Reload, true),
            Start::Cancel,
            "round {round}"
        );
        assert!(resume.completed(Some(id)).internal);
        wait(|| resume.poll());
        id += 1;
        // 읽은 뒤 내용이 바뀐다.
        let body = format!("<script>{round}()</script>");
        rewrite_keeping_stamp(&doc.dir.join("doc.html"), &body);
        assert!(matches!(
            resume.start(&doc.gate, &doc.url, Some(id), StartKind::Reload, true),
            Start::Resumed(_)
        ));
        resume.document_request(&doc.gate, &doc.url, || Some(round));
        wait(|| resume.poll_verify().stop.then_some(()));
        assert!(resume.completed(Some(id)).restart.is_some());
        id += 1;
    }
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(id), StartKind::Reload, true),
        Start::Js(false)
    );
}

/// `Stop`이 실패하면 재시작을 버린다. 지문이 다른 요청은 놓아 주지 않고, 다음 탐색이 commit할 때
/// 놓아 준다.
#[test]
fn a_failed_stop_drops_the_restart_and_keeps_the_request_until_another_commit() {
    let doc = allowed_doc("verify-stopfail");
    let mut resume = ScanResume::<Token>::new(1);
    replay_ready(&doc, &mut resume);
    rewrite_keeping_stamp(&doc.dir.join("doc.html"), "<script>b()</script>");
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Resumed(true)
    );
    resume.document_request(&doc.gate, &doc.url, || Some(7));
    wait(|| resume.poll_verify().stop.then_some(()));
    resume.stop_failed();
    let ended = resume.completed(Some(11));
    assert!(ended.internal && ended.restart.is_none());
    assert_eq!(ended.release, vec![7], "그 탐색이 끝났다면 놓아 준다");

    // 완료 신호가 오지 않은 채 다른 탐색이 commit하면 그때 놓아 준다.
    replay_ready(&doc, &mut resume);
    rewrite_keeping_stamp(&doc.dir.join("doc.html"), "<script>c()</script>");
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(21), StartKind::Reload, true),
        Start::Resumed(false),
        "허용하지 않은 내용의 스캔도 같은 요청 확인을 거친다"
    );
    resume.document_request(&doc.gate, &doc.url, || Some(8));
    wait(|| resume.poll_verify().stop.then_some(()));
    resume.stop_failed();
    resume.host_request(None);
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(22), StartKind::NewDocument, true),
        Start::Cancel
    );
    let (stop, release) = resume.content_loading(Some(30));
    assert!(!stop);
    assert_eq!(release, vec![8]);
}

/// 지문이 달라 멈춘 뒤로·앞으로 탐색은 같은 방향으로 다시 시작하고, 그 시작 신호는 방향을 알아
/// 다시 취소·스캔된다. 방향을 잃으면 스캔 없이 지문 없는 문서(`Js(false)`)가 된다.
#[test]
fn a_stopped_history_navigation_restarts_in_the_same_direction_and_is_scanned_again() {
    for (step, replay) in [
        (HistoryStep::Back, Replay::Back),
        (HistoryStep::Forward, Replay::Forward),
    ] {
        let doc = allowed_doc("verify-history");
        let mut resume = ScanResume::<Token>::new(1);
        resume.host_request(Some(step));
        assert_eq!(
            resume.start(
                &doc.gate,
                &doc.url,
                Some(10),
                StartKind::BackOrForward,
                true
            ),
            Start::Cancel
        );
        assert!(resume.completed(Some(10)).internal);
        assert_eq!(wait(|| resume.poll()), (replay, doc.url.clone()));
        rewrite_keeping_stamp(&doc.dir.join("doc.html"), "<script>b()</script>");
        assert_eq!(
            resume.start(
                &doc.gate,
                &doc.url,
                Some(11),
                StartKind::BackOrForward,
                true
            ),
            Start::Resumed(true)
        );
        assert_eq!(
            resume.document_request(&doc.gate, &doc.url, || Some(7)),
            RequestAction::Held
        );
        wait(|| resume.poll_verify().stop.then_some(()));
        let ended = resume.completed(Some(11));
        assert!(ended.internal);
        assert_eq!(ended.restart, Some((replay, doc.url.clone())), "{step:?}");
        // 재시작한 탐색은 방향을 알아 다시 취소되고, 같은 방향으로 재개된다.
        assert_eq!(
            resume.start(
                &doc.gate,
                &doc.url,
                Some(12),
                StartKind::BackOrForward,
                true
            ),
            Start::Cancel,
            "{step:?}"
        );
        assert!(resume.completed(Some(12)).internal);
        assert_eq!(wait(|| resume.poll()), (replay, doc.url.clone()));
        assert_eq!(
            resume.start(
                &doc.gate,
                &doc.url,
                Some(13),
                StartKind::BackOrForward,
                true
            ),
            Start::Resumed(false),
            "바뀐 내용은 자기 스캔을 받고 허용을 이어받지 않는다"
        );
    }
}

/// A의 문서 요청을 확인하는 동안 새 탐색 B가 시작되고 A가 끝나며 다른 문서가 commit한다. 그 뒤
/// 늦게 끝난 A의 불일치 결과는 이미 끝난 탐색의 것이므로 바로 놓아 준다. 다음 탐색을 기다리며
/// deferral을 붙잡아 두지 않는다.
#[test]
fn a_late_check_result_of_an_ended_navigation_is_released_at_once() {
    let doc = allowed_doc("verify-late");
    let mut resume = ScanResume::<Token>::new(1);
    replay_ready(&doc, &mut resume);
    rewrite_keeping_stamp(&doc.dir.join("doc.html"), "<script>b()</script>");
    assert_eq!(
        resume.start(&doc.gate, &doc.url, Some(11), StartKind::Reload, true),
        Start::Resumed(true)
    );
    assert_eq!(
        resume.document_request(&doc.gate, &doc.url, || Some(7)),
        RequestAction::Held
    );
    // 확인 결과를 처리하기 전에 B가 시작되고 A가 끝나며 다른 문서가 commit한다.
    let other = doc.url.replace("doc.html", "other.html");
    assert_eq!(
        resume.start(&doc.gate, &other, Some(12), StartKind::NewDocument, true),
        Start::Js(false)
    );
    assert!(
        resume.completed(Some(11)).release.is_empty(),
        "아직 읽는 요청"
    );
    let (stop, release) = resume.content_loading(Some(12));
    assert!(!stop && release.is_empty());
    let verified = wait(|| {
        let v = resume.poll_verify();
        (!v.complete.is_empty() || v.stop).then_some(v)
    });
    assert_eq!((verified.complete, verified.stop), (vec![7], false));
    let ended = resume.completed(None);
    assert!(ended.release.is_empty(), "남겨 둔 요청이 없다");
}
