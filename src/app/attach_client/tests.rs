//! Shared fixtures for attach regression scenarios.

use crate::model::Workspace;
use crate::runtime::engine_session::{EngineId, EngineSession};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tasty_remote::client_session::{
    AttachClientSession, ClientSessionState, ClientTransport, MirrorOutbox, SessionState,
};
use tasty_terminal::Terminal;
pub(super) fn supply_ids(engine: &crate::runtime::engine_access::EngineMut<'_>) {
    engine
        .runtime
        .ids
        .supply(
            [
                tasty_core::IdKind::Workspace,
                tasty_core::IdKind::Pane,
                tasty_core::IdKind::Tab,
                tasty_core::IdKind::Surface,
            ]
            .map(|kind| tasty_event_store::IdRange {
                kind: kind.label().into(),
                start: 20000,
                end: 30000,
            })
            .to_vec(),
        )
        .unwrap();
}

pub(super) fn test_ids() -> crate::runtime::id_reservations::ReservedIds {
    let bank = crate::runtime::id_reservations::IdReservations::default();
    let kinds = [
        tasty_core::IdKind::Workspace,
        tasty_core::IdKind::Pane,
        tasty_core::IdKind::Tab,
        tasty_core::IdKind::Surface,
    ];
    bank.supply(
        kinds
            .iter()
            .map(|kind| tasty_event_store::IdRange {
                kind: kind.label().into(),
                start: 10000,
                end: 20000,
            })
            .collect(),
    )
    .unwrap();
    bank.lease(&kinds.map(|kind| (kind, 10000))).unwrap()
}

/// 순수 함수 시험용 parked 항목. registry 없이 id·View 복원 자료·engine만 묶는다.
pub(super) struct ParkedEngine {
    pub(super) view_restore: crate::state::MainViewState,
    pub(super) session: EngineSession,
}

impl ParkedEngine {
    pub(super) fn from_test_state(
        (view_restore, core_state): (crate::state::MainViewState, EngineSession),
    ) -> Self {
        Self {
            view_restore,
            session: core_state,
        }
    }
}

/// writer 없는 시험 세션. 입력 전송은 실패해도 forwarder가 다음 입력을 기다린다.
pub(super) fn test_session(
    local_workspace: u32,
    remote_to_local: HashMap<u32, u32>,
) -> AttachClientSession {
    test_session_with_frames(local_workspace, remote_to_local).0
}

/// 보낸 프레임을 받는 쪽을 함께 돌려주는 시험 세션. 받는 쪽을 버리면 전송이 실패한다.
pub(crate) fn test_session_with_frames(
    local_workspace: u32,
    remote_to_local: HashMap<u32, u32>,
) -> (
    AttachClientSession,
    std::sync::mpsc::Receiver<tasty_remote::connection::QueuedFrame>,
) {
    let (tx, rx) = tasty_remote::connection::channel();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let control = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (_peer, _) = listener.accept().unwrap();
    let session = AttachClientSession {
        transport: ClientTransport {
            workers: tasty_remote::transport::ConnectionWorkers::new(control, Vec::new()),
            output: MirrorOutbox::new(tx.epoch()),
            disconnected: Arc::new(AtomicBool::new(false)),
            frame_tx: tx,
            tunnel: None,
            explorer_write: true,
            workspace_close: true,
            agent_children: true,
            terminal_read: true,
            file_download: true,
            agent_hello_sent: false,
            explorer_inbox: Vec::new(),
            html: Default::default(),
        },
        state: ClientSessionState {
            structure_ids: Default::default(),
            local_workspace,
            remote_to_local,
            phase: SessionState::Connected,
            client_id: 1,
            remote_workspace: 7,
            bulk_port: 0,
            anchor_ws_id: None,
            op_seq: 0,
            pending_op_focus: HashMap::new(),
            agent_requests: Default::default(),
            next_delta_focus: None,
            resize_sync: Default::default(),
            remote_label: "127.0.0.1:0".to_string(),
            profile: None,
            pending_list_dir_consumers: HashMap::new(),
            capture_uploads: HashMap::new(),
            html_locals: Default::default(),
            markdown_locals: HashSet::new(),
            resync_pending: None,
            resync_awaiting_window: false,
            workspace_close: Default::default(),
        },
    };
    (session, rx)
}

/// 실제 kind 등록 경로를 사용하며 플러그인 프로세스 대신 채널 수신자로 명령을 확인한다.
pub(super) fn register_markdown_kind(
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    plugin_id: &str,
) -> std::sync::mpsc::Receiver<crate::plugin_bridge::host_cmd::HostCmd> {
    let decl: crate::plugin::manifest::SurfaceKindDecl =
        serde_json::from_value(serde_json::json!({
            "kind": "markdown",
            "display_name_i18n_key": "surface.kind.markdown",
            "rendering": "webview",
        }))
        .expect("test SurfaceKindDecl");
    let (tx, rx) = std::sync::mpsc::channel();
    crate::plugin_bridge::remote_kind::register_remote_kind(
        &engine.runtime.surface_registry,
        plugin_id,
        &decl,
        tx,
    );
    rx
}

pub(super) fn created_surfaces(
    rx: &std::sync::mpsc::Receiver<crate::plugin_bridge::host_cmd::HostCmd>,
) -> Vec<(u32, Value)> {
    rx.try_iter()
        .filter_map(|cmd| match cmd {
            crate::plugin_bridge::host_cmd::HostCmd::RemoteSurfaceCreated {
                surface_id,
                params,
                ..
            } => Some((surface_id, params)),
            _ => None,
        })
        .collect()
}

pub(super) fn markdown_descriptor(remote_id: u32) -> Value {
    serde_json::json!({
        "remote_id": remote_id,
        "role": "markdown",
        "file": "/remote/docs/README.md",
        "display_name": "README.md",
    })
}

pub(super) fn single_leaf_tree(remote_id: u32) -> Value {
    serde_json::json!({
        "id": 9, "name": "mirror", "focused_pane": 7,
        "panes": [ {
            "id": 7,
            "tabs": [ {
                "id": 3, "name": "README.md", "active": true, "focused_surface": remote_id,
                "layout": { "type": "Leaf", "id": remote_id, "kind": "markdown" }
            } ]
        } ]
    })
}

pub(super) fn parked_ids(
    parked: &[ParkedEngine],
) -> impl Iterator<Item = (EngineId, &crate::core::CoreState)> {
    parked.iter().map(|p| (p.session.id, &p.session.core_state))
}

/// 첫 항목만 보는 오류를 잡도록 mirror는 두 번째 parked engine에만 둔다.
pub(super) fn parked_with_mirror(ws_id: u32, local_surface: u32) -> Vec<ParkedEngine> {
    let mut parked: Vec<ParkedEngine> = (0..2)
        .map(|_| ParkedEngine::from_test_state(crate::state::tests::test_state()))
        .collect();
    let mut mirror_ws = Workspace::new_with_terminal_marker(
        ws_id,
        "mirror".to_string(),
        9_001,
        9_002,
        local_surface,
    );
    mirror_ws.mirror = true;
    let mut engine = parked[1].session.borrow_mut();
    engine.push_mirror_workspace(mirror_ws);
    engine.runtime.surfaces.insert(
        local_surface,
        Box::new(crate::model::TerminalSurface { id: local_surface }),
    );
    engine
        .runtime
        .terminals
        .insert(local_surface, Terminal::new_detached(80, 24), None);
    parked
}

pub(super) const MARKDOWN_PLUGIN_ID: &str = "com.tasty.markdown";
