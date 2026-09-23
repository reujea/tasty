//! `Hub` — 외부 통신 표면. IPC 서버, 포트 파일 등 *프로세스 외부* 와 주고받는
//! 인프라를 모은다.
//!
//! `ipc_server` 는 `Option<Box<dyn IpcServerPort>>` 로 보유. production
//! 은 `TcpIpcServer` (옛 `IpcServer` 의 type alias).

use crate::adapters::production::tcp_ipc_server::TcpIpcServer;
use crate::ipc::server::IpcWaker;
use crate::ports::ipc_server::IpcServerPort;
use tasty_ipc::host_call::HostIpcInjector;
use tasty_ipc::stream_hub::StreamContext;

pub(crate) struct Hub {
    pub ipc_server: Option<Box<dyn IpcServerPort>>,
    pub port_file: Option<String>,
}

impl Hub {
    pub(crate) fn new(port_file: Option<String>) -> Self {
        Self {
            ipc_server: None,
            port_file,
        }
    }

    /// IPC 서버 시작. `IpcWaker` 는 호출자가 직접 만든다 — gui 빌드는
    /// `EventLoopProxy<AppEvent>` 에서 변환, headless 는 `mpsc::Sender<AppEvent>`
    /// 에서 변환 (`adapters::production::headless_waker`).
    ///
    /// 반환: host→plugin sync dispatch 에 사용할 `HostIpcInjector` (서버 시작
    /// 실패 시 `None`). 호출자가 `Core::set_host_ipc_injector` 로 등록한다.
    ///
    /// `connections` 는 `Core` 가 들고 있는 연결 자리 게이지의 핸들이다. Hub 는
    /// `Core` 를 못 보므로 호출자가 건네준다 — 안 건네면 `system.pressure` 의
    /// `connections` 덩어리가 영영 0 으로 남는다.
    pub(crate) fn start_ipc(
        &mut self,
        ipc_waker: IpcWaker,
        stream_ctx: StreamContext,
        connections: std::sync::Arc<tasty_telemetry::ConnectionStats>,
    ) -> Option<HostIpcInjector> {
        match TcpIpcServer::start_with_port_file(
            self.port_file.take(),
            Some(ipc_waker.clone()),
            stream_ctx,
            connections,
        ) {
            Ok(ipc) => {
                tracing::info!("IPC server started on port {}", ipc.port());
                // 주입기는 서버와 **같은** 입장 장부를 든다 — 소켓 요청과 주입이 같은 큐의
                // 바이트를 나눠 쓰고, 주입 깊이는 그 장부가 따로 센다(ADR-0391).
                let injector = HostIpcInjector::new(ipc.command_sender(), ipc_waker)
                    .with_admission(ipc.admission());
                self.ipc_server = Some(Box::new(ipc));
                Some(injector)
            }
            Err(e) => {
                tracing::warn!("Failed to start IPC server: {}", e);
                None
            }
        }
    }
}
