//! winit user event 버스에 흐르는 host 자체 이벤트.
//!
//! 백그라운드 스레드 / IPC / OS-level 트리거가 `EventLoopProxy<AppEvent>` 로 push
//! 하면 App::user_event 가 분기 처리한다. variant 별 producer/consumer 매핑은
//! 각 doc-comment 참조.

/// 새 창 생성을 **누가** 요청했는지. 실패 안내 채널을 가르는 유일한 기준이다.
///
/// 핵심 원칙 1 은 에이전트 행동의 부수효과가 사용자 상태(포커스)에 닿는 것을 금지한다.
/// 실패 안내라도 예외가 아니다 — 사용자가 하지 않은 일의 실패 통지 때문에 하던 일의
/// 포커스를 잃어서는 안 된다. 그래서 `User` 는 방금 그 조작의 결과이므로 모달로 알리고,
/// `Agent` 발 실패는 요청자에게 IPC 응답으로 돌려주며 사용자 화면은 건드리지 않는다
/// (완료 채널 = [`IpcCompletion`]).
/// 근거: `docs/adr/0117-window-and-modal-creation-failure-policy.md`,
/// `docs/adr/0122-winit-scheduled-fallible-ipc-returns-outcome.md`.
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowRequestOrigin {
    /// 메뉴 · 단축키 · dock · tray — 사용자가 방금 요청했다.
    User,
    /// `window.create` / `view.create` IPC — 에이전트가 요청했다.
    Agent,
}

/// winit 이벤트 루프에 스케줄된 **실패 가능한** IPC op 의 결과를 요청자에게 돌려주는
/// 완료 채널. IPC 핸들러는 `{"scheduled": true}` 를 즉시 돌려주는 대신 이 채널을
/// `AppEvent` 에 실어 보내고 `IpcStep::Handled`(응답 defer)로 반환한다 — winit 핸들러가
/// op 를 실제로 수행한 뒤 성공/실패를 이 채널로 보낸다. 메서드마다 계약이 갈리지 않게
/// 한 벌로 공유한다. 근거: `docs/adr/0122-winit-scheduled-fallible-ipc-returns-outcome.md`.
#[cfg(feature = "gui")]
#[derive(Debug)]
pub(crate) struct IpcCompletion {
    response_tx: std::sync::mpsc::SyncSender<crate::ipc::protocol::JsonRpcResponse>,
    request_id: serde_json::Value,
}

#[cfg(feature = "gui")]
impl IpcCompletion {
    pub(crate) fn new(
        response_tx: std::sync::mpsc::SyncSender<crate::ipc::protocol::JsonRpcResponse>,
        request_id: serde_json::Value,
    ) -> Self {
        Self {
            response_tx,
            request_id,
        }
    }

    /// op 성공 — 결과 payload 를 요청자에게 돌려준다.
    pub(crate) fn reply_ok(self, result: serde_json::Value) {
        crate::ipc::server::send_response(
            &self.response_tx,
            crate::ipc::protocol::JsonRpcResponse::success(self.request_id, result),
        );
    }

    /// op 실패 — JSON-RPC 에러(원인 문자열 포함)를 요청자에게 돌려준다.
    pub(crate) fn reply_err(self, code: i32, message: impl Into<String>) {
        crate::ipc::server::send_response(
            &self.response_tx,
            crate::ipc::protocol::JsonRpcResponse::error(self.request_id, code, message.into()),
        );
    }

    /// 창 생성 결과를 요청자에게 돌려준다 — `window.create`/`view.create` 왕복의
    /// **핵심 계약**(ADR-0122). 성공은 `{"created": true, "window_id": <u64>}`,
    /// 실패는 `-32000` 에러에 원인 문자열을 실어 보낸다. 이 매핑이 이 lane 의
    /// 헤드라인 주장이다("window.create 가 생성 성공/실패를 응답에 싣는다") — winit
    /// 핸들러는 이 함수를 부르는 얇은 caller 다. 되돌리면(성공으로 즉답 등)
    /// `event::tests` 의 왕복 테스트가 깨진다.
    pub(crate) fn reply_window_create(self, outcome: Result<u64, String>) {
        match outcome {
            Ok(window_id) => self.reply_ok(serde_json::json!({
                "created": true,
                "window_id": window_id,
            })),
            Err(msg) => self.reply_err(-32000, msg),
        }
    }
}

/// Custom events sent to the winit event loop from background threads.
#[derive(Debug)]
pub(crate) enum AppEvent {
    /// PTY reader thread produced output. If targeted_pty_polling is enabled,
    /// contains the surface_id that has new data. Otherwise None (poll all).
    TerminalOutput(Option<u32>),
    /// IPC command arrived -- wake up and process.
    IpcReady,
    /// A streaming-channel client sent an inbound frame -- wake up and drain the
    /// stream inbound queue (StreamHub::pump_inbound).
    StreamReady,
    /// egui requested a repaint (new window, animation, cursor blink).
    /// `window_id` 는 어느 윈도우의 egui 컨텍스트가 repaint 를 요청했는지 식별 —
    /// 핸들러는 해당 윈도우만 dirty 로 표시한다. (모든 egui Context 는 root viewport
    /// 만 쓰므로 `viewport_id` 는 항상 `ROOT` 라 윈도우 구분에 못 쓴다 — window_id 로 라우팅.)
    /// delay-aware repaint (`Duration > 0`) 는 idle frame loop 방지 위해 callback 단계에서 drop 된다.
    #[cfg(feature = "gui")]
    EguiRepaint { window_id: winit::window::WindowId },
    /// Request to create a new window. 첫 페이로드는 **누가 요청했는지** — 실패 안내를
    /// 어느 채널로 낼지 가르는 기준이다([`WindowRequestOrigin`],
    /// `docs/adr/0117-window-and-modal-creation-failure-policy.md`). 둘째는 IPC 요청자에게
    /// 생성 성공/실패를 돌려줄 완료 채널 — `window.create`/`view.create` 는 `Some`,
    /// 메뉴·단축키·tray 등 사용자 경로는 `None`
    /// (`docs/adr/0122-winit-scheduled-fallible-ipc-returns-outcome.md`).
    #[cfg(feature = "gui")]
    CreateWindow(WindowRequestOrigin, Option<IpcCompletion>),
    /// CSD titlebar close 버튼이 발화하는 per-window 닫기 요청 (사용자 클릭).
    /// 네이티브 `WindowEvent::CloseRequested` 와 동일한 라이프사이클로 라우팅한다
    /// (단일 창이면 quit 흐름, 다중 창이면 해당 창만 닫음).
    #[cfg(feature = "gui")]
    CloseWindow(winit::window::WindowId),
    /// 사용자 스크립트 단축키가 눌려 Lua 워커에서 실행 요청 (ADR-0031).
    /// view 의 `handle_shortcut` 이 combo 매칭 후 스크립트 소스를 읽어 발행하고,
    /// App 이 소유한 `lua_engine` 워커로 실행한다(사용자 키 입력 경로에서만 — identity 원칙 1).
    ///
    /// 그 "사용자 키 입력 경로" 가 유일한 발신원이라 headless 에는 만드는 자리가 없다.
    #[cfg(feature = "gui")]
    RunLuaScript { source: String, name: String },
    /// Request to open settings modal.
    #[cfg(feature = "gui")]
    OpenSettings,
    /// Request to open plugins modal.
    #[cfg(feature = "gui")]
    OpenPlugins,
    /// Request to shut down the entire application.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "only the gui window lifecycle builds this variant; headless has no producer"
        )
    )]
    Shutdown,
    /// Request to minimize (park state, close windows).
    #[cfg(feature = "gui")]
    Minimize,
    /// Request quit following the close_behavior setting.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "only the gui window close path builds this variant; headless has no producer"
        )
    )]
    QuitRequested,
    /// Request to re-show hidden windows from the system tray (Windows / Linux,
    /// where windows persist while hidden). macOS restores via CreateWindow instead.
    #[cfg(all(any(windows, target_os = "linux"), feature = "gui"))]
    TrayShowWindow,
    /// OS 가 절전(suspend)에서 복귀했다 (Windows `WM_POWERBROADCAST`). resume
    /// 헬스 패스를 돌려 죽은 ConPTY 자식을 정리하고 살아있는 자식을 wake nudge
    /// 한다 (ADR-0017). Windows 전용 — Unix PTY 는 절전에 강건해 불필요하다.
    #[cfg(all(windows, feature = "gui"))]
    SystemResumed,
    /// 중앙 타이머 허브(`docs/dev-guide/timer-hub.md`)의 waker 스레드가 다음 데드라인에
    /// 도달했다는 wake 신호. **이 이벤트 자체는 아무 일도 하지 않는다** — 깨어난
    /// `about_to_wait` 가 `timers.drain_due` 로 due 한 키를 실행한다. headless 는
    /// 메인 루프가 `recv_timeout` 으로 직접 데드라인을 지키므로 이 이벤트가 없다.
    #[cfg(feature = "gui")]
    TimerTick,
    /// attach/detach 작업 J — client mirror reader thread 가 원격 출력을 받으면 보내는
    /// 실시간 wake 신호. 서버측 readonly 뷰의 3초 cadence(`Tick::AttachView`)와 달리, 내가 직접
    /// 다루는 client mirror 는 로컬 워크스페이스처럼 데이터가 오는 즉시 적용/repaint 한다.
    /// App 이 누적 출력 버퍼를 drain 해 mirror Terminal 에 feed 한다.
    #[cfg(feature = "gui")]
    AttachClientData,
    /// attach/detach 단계 7 — 자동 attach 워커 스레드가 SSH 터널 수립(또는 loopback
    /// 해석)을 마치면 보내는 wake 신호. App 이 결과 채널(`auto_attach_rx`)을 drain 해
    /// `start_gui_attach` 로 mirror 를 띄운다. 터널 핸들은 채널로 전달(AppEvent 는
    /// Debug 라 핸들을 싣지 않는다).
    #[cfg(feature = "gui")]
    AutoAttachReady,
    /// 스크린샷→클립보드 캡처 워커 스레드가 OS 인터랙티브 캡처를 마치면
    /// 보내는 wake 신호. App 이 결과 채널(`screenshot_capture_rx`)을 drain 해
    /// 로컬 클립보드에 기록하거나 mirror 세션으로 업로드한다.
    #[cfg(feature = "gui")]
    ScreenshotCaptureReady,
    /// mirror 이미지 paste 업로드 워커 스레드가 bulk 업로드를 마치면 보내는 wake
    /// 신호. App 이 결과 채널(`image_upload_rx`)을 drain 해 성공 시 원격 경로를 대상
    /// mirror surface 입력에 삽입하거나(실패 시 Warning toast) 처리한다.
    #[cfg(feature = "gui")]
    ImageUploadReady,
    /// mirror 파일 전송 업로드 워커의 진행 이벤트(청크 전송)가 도착했다는 wake 신호.
    /// App 이 진행 채널(`transfer_progress_rx`)을 drain 해 진행 팝업의 행(바이트/속도/
    /// determinate bar)을 갱신한다.
    #[cfg(feature = "gui")]
    TransferProgressTick,
    /// 비동기 파일 식별 결과. `IdentifyWorker::spawn` 의 worker thread 가 완료 시 송신.
    /// 각 요청은 독립적으로 완료되며 request_id는 진단용이다. 명시 origin이
    /// 사라진 결과는 다른 창으로 보내지 않고 폐기한다.
    #[cfg(feature = "gui")]
    IdentifyDone {
        request_id: crate::identify_worker::IdentifyRequestId,
        target: crate::file::format::FileTarget,
        detector: Option<crate::file::format::DetectorId>,
        /// `DispatchFile.origin_surface_id` 그대로 carry. Some 이면
        /// App routes completion to its owner, including parked engines, and
        /// retains it through picker selection. A vanished owner is not focus.
        origin_surface_id: Option<u32>,
        /// `DispatchFile.dispatch_origin` 그대로 carry — 사용자 경로는 결과 탭을 선택하고
        /// 에이전트 경로는 선택하지 않는다. `IntentOrigin` 은 이 왕복을 못 건넌다.
        dispatch_origin: crate::file::dispatch::FileDispatchOrigin,
        /// `DispatchFile.ignore_size_limit` 그대로 carry — 대용량 markdown 게이트를
        /// 건너뛴다(에이전트/IPC 강제 열기). 비동기 식별 왕복을 통과시키기 위함.
        ignore_size_limit: bool,
    },
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;

    /// `reply_ok` 은 요청 id 를 보존한 success 응답에 결과 payload 를 싣는다.
    /// 변이 검증: `reply_ok` 을 error 로 바꾸면 `resp.error.is_none()` 이 깨진다.
    #[test]
    fn reply_ok_carries_id_and_result() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        IpcCompletion::new(tx, serde_json::json!(7))
            .reply_ok(serde_json::json!({ "created": true, "window_id": 42 }));
        let resp = rx.recv().expect("response");
        assert_eq!(resp.id, serde_json::json!(7));
        assert!(resp.error.is_none());
        assert_eq!(
            resp.result.expect("result"),
            serde_json::json!({ "created": true, "window_id": 42 })
        );
    }

    /// `reply_err` 은 요청 id 를 보존한 error 응답에 code·원인 문자열을 싣는다.
    /// 변이 검증: `reply_err` 이 success 를 보내면 `resp.result.is_none()` 이 깨진다.
    #[test]
    fn reply_err_carries_id_code_and_message() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        IpcCompletion::new(tx, serde_json::json!("req-1")).reply_err(-32000, "boom");
        let resp = rx.recv().expect("response");
        assert_eq!(resp.id, serde_json::json!("req-1"));
        assert!(resp.result.is_none());
        let e = resp.error.expect("error");
        assert_eq!(e.code, -32000);
        assert_eq!(e.message, "boom");
    }

    /// 완료 채널이 응답 없이 drop 되면 요청자의 receiver 는 무한 대기하지 않고
    /// 즉시 disconnect 를 본다 — fire-and-forget 이 남긴 "영원히 안 오는 응답" 대신.
    #[test]
    fn dropping_without_reply_disconnects_the_receiver() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<crate::ipc::protocol::JsonRpcResponse>(1);
        drop(IpcCompletion::new(tx, serde_json::json!(1)));
        assert!(rx.recv().is_err(), "receiver must see disconnect, not hang");
    }

    /// **이 lane 의 헤드라인 계약** — 창 생성이 실패하면 `window.create` 왕복이
    /// 요청자에게 **JSON-RPC 에러를 원인 문자열과 함께** 돌려준다(성공 즉답이 아니라).
    /// winit 핸들러가 `create_new_window` 의 `Err` 를 이 매핑으로 흘려보내는 경로다.
    ///
    /// 변이 검증(conductor 수용 기준): `reply_window_create` 를 "만들되 성공으로
    /// 즉답"(예: outcome 무시하고 항상 `reply_ok`)으로 되돌리면 — `IpcCompletion` 은
    /// 그대로 쓰므로 dead-code 린트로는 안 잡히는 형태 — 이 테스트가 FAILED.
    /// 실패가 에러로 오는지(result 가 아니라)를 값으로 단정하기 때문이다.
    #[test]
    fn window_create_failure_returns_a_jsonrpc_error_with_the_cause() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        IpcCompletion::new(tx, serde_json::json!(9))
            .reply_window_create(Err("shell not found: /bad/path".to_string()));
        let resp = rx.recv().expect("response");
        assert_eq!(resp.id, serde_json::json!(9));
        assert!(
            resp.result.is_none(),
            "실패는 success 응답이면 안 된다 — 요청자가 실패를 성공으로 오독한다"
        );
        let e = resp.error.expect("실패는 JSON-RPC 에러로 와야 한다");
        assert_eq!(e.code, -32000);
        assert!(
            e.message.contains("shell not found: /bad/path"),
            "원인 문자열이 에러 메시지에 실려야 한다: {}",
            e.message
        );
    }

    /// 대칭 — 창 생성이 성공하면 같은 왕복이 `{"created": true, "window_id": <u64>}`
    /// 를 요청자에게 돌려준다. 변이(성공을 에러로)면 `resp.error.is_none()` 이 깨진다.
    #[test]
    fn window_create_success_returns_created_and_window_id() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        IpcCompletion::new(tx, serde_json::json!("req-ok")).reply_window_create(Ok(1234));
        let resp = rx.recv().expect("response");
        assert!(resp.error.is_none());
        assert_eq!(
            resp.result.expect("result"),
            serde_json::json!({ "created": true, "window_id": 1234 })
        );
    }
}
