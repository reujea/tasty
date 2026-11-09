//! 백그라운드 작업·IPC·OS의 요청을 winit 이벤트 루프로 전달한다.

/// 사용자의 창 생성 실패는 화면에 알리고 에이전트 실패는 IPC로 돌려준다.
/// 에이전트 작업의 실패 때문에 사용자 포커스를 바꾸지 않는다.
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowRequestOrigin {
    User,
    Agent,
}

/// 예약 접수와 실제 성공을 구별하도록 이벤트 처리 뒤 IPC 결과를 돌려주는 채널.
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

    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + serde_json::to_vec(&self.request_id)
                .expect("request ID serializes")
                .len()
    }

    pub(crate) fn reply_ok(self, result: serde_json::Value) {
        crate::ipc::server::send_response(
            &self.response_tx,
            crate::ipc::protocol::JsonRpcResponse::success(self.request_id, result),
        );
    }

    pub(crate) fn reply_err(self, code: i32, message: impl Into<String>) {
        crate::ipc::server::send_response(
            &self.response_tx,
            crate::ipc::protocol::JsonRpcResponse::error(self.request_id, code, message.into()),
        );
    }

    /// 생성 성공은 created·window_id, 실패는 원인을 담은 -32000 응답으로 전달한다.
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

#[derive(Debug)]
pub(crate) enum AppEvent {
    #[cfg(all(feature = "gui", any(target_os = "windows", target_os = "macos")))]
    OutboundDragEnded {
        window: winit::window::WindowId,
        id: u64,
        result: Result<crate::file::drag::Outcome, String>,
    },
    /// PTY reader thread produced output. If targeted_pty_polling is enabled,
    /// contains the surface_id that has new data. Otherwise None (poll all).
    TerminalOutput(Option<u32>),
    IpcReady,
    JournalReady,
    /// 들어온 스트림 프레임 큐를 비우도록 깨운다.
    StreamReady,
    /// egui viewport는 모두 ROOT여서 창 ID로 repaint 대상을 구별한다.
    #[cfg(feature = "gui")]
    EguiRepaint {
        window_id: winit::window::WindowId,
    },
    /// egui의 지연 repaint 요청. `at`에 타이머로 그 창을 다시 그린다.
    #[cfg(feature = "gui")]
    EguiRepaintAfter {
        window_id: winit::window::WindowId,
        at: std::time::Instant,
    },
    /// 요청 origin은 실패 안내 방법을 정한다. IPC는 완료 채널을, 사용자 요청은 None을 전달한다.
    #[cfg(feature = "gui")]
    CreateWindow(WindowRequestOrigin, Option<IpcCompletion>),
    /// CSD 닫기 버튼도 OS 닫기 이벤트와 같은 종료 경로를 사용한다.
    #[cfg(feature = "gui")]
    CloseWindow(winit::window::WindowId),
    /// 사용자 단축키로 읽은 스크립트를 Lua 워커에 전달한다. 실패는 `window`의 토스트로만 보인다.
    #[cfg(feature = "gui")]
    RunLuaScript {
        source: String,
        name: String,
        window: winit::window::WindowId,
    },
    #[cfg(feature = "gui")]
    OpenSettings,
    #[cfg(feature = "gui")]
    OpenPlugins,
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "only the gui window lifecycle builds this variant; headless has no producer"
        )
    )]
    Shutdown,
    #[cfg(feature = "gui")]
    Minimize,
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "only the gui window close path builds this variant; headless has no producer"
        )
    )]
    QuitRequested,
    /// Windows·Linux의 숨긴 창을 다시 표시한다. macOS는 창 생성 경로로 복원한다.
    #[cfg(all(any(windows, target_os = "linux"), feature = "gui"))]
    TrayShowWindow,
    /// 같은 홈으로 다시 실행한 두 번째 프로세스의 활성화 요청(Linux D-Bus `Activate`, Windows 등록 메시지).
    /// 증거가 없으면 아무것도 바꾸지 않는다. macOS 는 이 요청을 받는 경로가 없다.
    #[cfg(all(any(windows, target_os = "linux"), feature = "gui"))]
    ExternalActivate(crate::boot::single_instance::ExternalActivation),
    /// Windows 절전 복귀 뒤 ConPTY 자식 상태를 확인하고 살아 있는 자식을 깨운다.
    #[cfg(all(windows, feature = "gui"))]
    SystemResumed,
    /// 직접 작업하지 않고 about_to_wait가 실행 기한이 된 타이머를 처리하도록 깨운다.
    #[cfg(feature = "gui")]
    TimerTick,
    /// mirror 수신 버퍼를 적용하도록 깨운다. 서버 readonly 화면의 주기 갱신과는 별개다.
    #[cfg(feature = "gui")]
    AttachClientData,
    /// 자동 attach 워커의 결과 채널을 확인한다. 터널 핸들은 별도 채널로 전달한다.
    #[cfg(feature = "gui")]
    AutoAttachReady,
    /// 완료한 화면 캡처를 결과 채널에서 받아 클립보드에 넣거나 원격으로 올린다.
    #[cfg(feature = "gui")]
    ScreenshotCaptureReady,
    /// 원격 업로드 결과를 받아 처음 지정한 mirror surface에 경로를 붙여넣는다.
    #[cfg(feature = "gui")]
    ImageUploadReady,
    /// 업로드 진행 채널을 읽어 표시를 갱신한다.
    #[cfg(feature = "gui")]
    TransferProgressTick,
    /// plugin webview 우클릭 메뉴에서 고른 항목을 소유 plugin 에 알린다(`webview.context_menu_selected`).
    /// 메뉴 뒤처리는 창 View 에서 돌아 plugin 매니저를 쥔 App 으로 넘긴다.
    #[cfg(feature = "gui")]
    WebviewMenuChosen {
        plugin_id: String,
        payload: serde_json::Value,
    },
    /// 파일 식별 결과. 명시한 origin이 사라졌으면 다른 창으로 보내지 않고 폐기한다.
    #[cfg(feature = "gui")]
    IdentifyDone {
        request_id: crate::identify_worker::IdentifyRequestId,
        target: crate::file::format::FileTarget,
        detector: Option<crate::file::format::DetectorId>,
        /// 원래 surface의 소유 engine으로 전달하며 parked 상태도 포함한다.
        origin_surface_id: Option<u32>,
        /// IntentOrigin은 비동기 왕복에 남지 않아 사용자·에이전트 선택 동작을 별도로 보존한다.
        dispatch_origin: crate::file::dispatch::FileDispatchOrigin,
        /// 요청의 대용량 파일 허용 여부를 비동기 식별 뒤에도 유지한다.
        ignore_size_limit: bool,
        /// 최종 결과를 받을 요청의 run.
        run: Option<u64>,
    },
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;

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

    #[test]
    fn dropping_without_reply_disconnects_the_receiver() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<crate::ipc::protocol::JsonRpcResponse>(1);
        drop(IpcCompletion::new(tx, serde_json::json!(1)));
        assert!(rx.recv().is_err(), "receiver must see disconnect, not hang");
    }

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
