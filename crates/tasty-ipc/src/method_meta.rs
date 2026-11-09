//! IPC 메서드의 호출자·권한·재시도 계약을 정의한다. 새 호스트 메서드는 이 표에 등록한다.
//! Plugin/Agent는 이 메타데이터로 권한을 검사한다. Local의 권한 검사는 별도로 면제된다.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock, RwLock};

use tasty_plugin_manifest::Permission;

use crate::ipc_namespace::IpcNamespaceRegistry;
// PREFIX_RULES를 줄 단위로 읽는 검사가 있어 짧은 이름으로 한 줄 형식을 유지한다.
// 이 이름은 debug 표에서만 사용하므로 import에도 같은 cfg를 적용한다.
#[cfg(debug_assertions)]
use self::MethodEffect::Idempotent;

/// 같은 요청을 다시 전달했을 때의 부수효과. 조회처럼 보여도 파일을 쓰거나
/// 커서를 옮기는 메서드는 Mutate다. 새 메서드는 생성자에서 반드시 분류한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodEffect {
    /// 호스트 상태를 안 바꾼다. 두 번째 전달은 아무 흔적도 안 남긴다.
    ///
    /// 바깥을 조회하는 것(원격 probe 등)도 호스트 상태를 안 바꾸면 여기다.
    Read,
    /// 상태를 바꾸지만 두 번째 전달이 **같은 끝 상태로 수렴**한다. 값을 호출자가 주는
    /// 쓰기(`*.set` · 이름을 지정한 삭제)와, 이미 멱등 계약을 문서로 든 것
    /// (`agent.lease_acquire` 의 같은 holder 재획득)이 여기다.
    Idempotent,
    /// 두 번째 전달이 **두 번째 효과**를 남긴다. 새 id 를 내는 생성, 누적되는 기록,
    /// 밖으로 나가는 전송, 그리고 시점이 값이 되는 것(`surface.set_mark`)이 여기다.
    Mutate,
}

/// 전송 전에 확인할 멱등 키 계약. Mutate와 journal 구조 명령은 Kept이며, 나머지 read/idempotent 요청은 Unneeded다.
/// 표에 없는 플러그인 고유 이름은 Outside다. 필요한 기능 버전은 실제 실행 경로에 따라 다르다.
/// App과 GUI debug/namespace forward의 버전은 별도로 지정하며 key_contract_by_layer가 구현과 대조한다.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContract {
    /// 선언된 보존 범위 안에서 같은 키·요청의 응답을 재사용한다.
    /// 재사용 응답에는 idempotent_replay가 붙는다. since 이상의 서버 기능 버전이 필요하다.
    Kept {
        /// 필요한 ipc.idempotency-key 최소 버전.
        since: u32,
    },
    /// **보존소가 필요 없다** — 재전달이 원래 안전하다([`MethodEffect::Read`] ·
    /// [`MethodEffect::Idempotent`]). 키는 봉투 검사만 받고 보존소에 안 들어가며, 재생 표지는
    /// 붙지 않는다. 다시 보내도 두 번째 효과가 없다.
    Unneeded,
    /// **계약 밖이다** — 키를 실어도 호스트가 보존소를 거친다고 선언하지 않는다. 재시도는 두
    /// 번째 실행일 수 있다. 호스트는 그 실행이 정확히 한 번이라고 약속하지 않는다.
    Outside,
}

/// Engine 라우터의 멱등 키 기능 버전.
pub const KEY_KEPT_BY_ROUTER: u32 = 1;
/// App에서 처리하는 메서드의 멱등 키 기능 버전.
pub const KEY_KEPT_BY_APP_LAYER: u32 = 2;
/// GUI debug step과 정적 호스트 이름의 namespace forward에 필요한 기능 버전.
pub const KEY_KEPT_ON_EVERY_HOST_PATH: u32 = 3;
/// Journal-backed structure results preserve their original response across engine resumes.
pub const KEY_KEPT_IN_STRUCTURE_JOURNAL: u32 = 4;

/// 효과 분류에서 기본 키 계약을 만든다. 다른 실행 경로는 전용 생성자로 버전을 지정한다.
const fn key_contract_of(effect: MethodEffect) -> KeyContract {
    match effect {
        MethodEffect::Mutate => KeyContract::Kept {
            since: KEY_KEPT_BY_ROUTER,
        },
        MethodEffect::Read | MethodEffect::Idempotent => KeyContract::Unneeded,
    }
}

/// 한 IPC 메서드에 대한 권한 메타.
#[derive(Debug, Clone, Copy)]
pub struct MethodMeta {
    /// Plugin/Agent 호출 허용 여부. false면 권한을 추가해도 호출할 수 없다.
    pub plugin_callable: bool,
    /// 플러그인 host-call에서만 처리하며 외부 CLI·네트워크 dispatch 경로는 없는 메서드.
    pub plugin_only: bool,
    /// plugin이 호출하려면 매니페스트에 이 권한들이 모두 선언돼 있어야 함.
    pub required: &'static [Permission],
    /// 정적 표 대신 등록된 플러그인 prefix로 찾은 이름이다.
    /// required가 비어 있어도 호출 게이트가 ipc.invoke:<prefix>를 요구한다.
    pub namespace_forward: bool,
    /// 이 메서드를 두 번 전달했을 때 무엇이 남는가. [`MethodEffect`] 참조.
    pub effect: MethodEffect,
    /// 멱등 키에 대해 무엇을 선언하는가. [`KeyContract`] 참조.
    pub key_contract: KeyContract,
}

const fn plugin(effect: MethodEffect, required: &'static [Permission]) -> MethodMeta {
    MethodMeta {
        plugin_callable: true,
        plugin_only: false,
        required,
        namespace_forward: false,
        effect,
        key_contract: key_contract_of(effect),
    }
}

/// 플러그인 host-call 전용 메서드를 선언한다. 외부 dispatch 경로는 없다.
const fn plugin_only(effect: MethodEffect, required: &'static [Permission]) -> MethodMeta {
    MethodMeta {
        plugin_callable: true,
        plugin_only: true,
        required,
        namespace_forward: false,
        effect,
        key_contract: key_contract_of(effect),
    }
}

impl MethodMeta {
    const fn kept_in_structure_journal(self) -> Self {
        Self {
            key_contract: KeyContract::Kept {
                since: KEY_KEPT_IN_STRUCTURE_JOURNAL,
            },
            ..self
        }
    }

    /// App에서 처리하는 Mutate의 최소 멱등 키 기능 버전을 지정한다.
    const fn kept_by_app_layer(self) -> Self {
        assert!(
            matches!(self.effect, MethodEffect::Mutate),
            "App 층 보존소 선언은 Mutate 에만 뜻이 있다"
        );
        Self {
            key_contract: KeyContract::Kept {
                since: KEY_KEPT_BY_APP_LAYER,
            },
            ..self
        }
    }

    /// GUI debug step 또는 정적 호스트 이름의 namespace forward 경로에 필요한 버전이다.
    const fn kept_on_every_host_path(self) -> Self {
        assert!(
            matches!(self.effect, MethodEffect::Mutate),
            "보존소 선언은 Mutate 에만 뜻이 있다"
        );
        Self {
            key_contract: KeyContract::Kept {
                since: KEY_KEPT_ON_EVERY_HOST_PATH,
            },
            ..self
        }
    }
}

const fn local_only(effect: MethodEffect) -> MethodMeta {
    MethodMeta {
        plugin_callable: false,
        plugin_only: false,
        required: &[],
        namespace_forward: false,
        effect,
        key_contract: key_contract_of(effect),
    }
}

/// 호스트 메서드의 등록 목록. prefix 기반 처리는 PREFIX_RULES와 런타임 namespace 표를 따른다.
pub const METHOD_TABLE: &[(&str, MethodMeta)] = {
    use MethodEffect::*;
    use Permission::*;
    &[
        // ── 호스트 system ─────────────────────────────────────────────
        ("system.info", plugin(Read, &[])),
        // 호스트 내부 렌더러 진단은 Local에만 공개한다.
        ("system.gpu_stats", local_only(Read)),
        // 다른 호출자의 부하까지 포함하는 프로세스 통계이므로 Local 전용이다.
        // 플러그인은 자기 사용량을 telemetry로 조회한다.
        ("system.pressure", local_only(Read)),
        // ── plugin 보조 채널 ──────────────────────────────────────────
        // 보조 fd/HANDLE 채널을 다루는 plugin 진입부가 직접 처리한다.
        // 별도 권한 토큰·할당량 정책은 미결정이며 현재 토큰 없이 cap·rate·audit 검사를 받는다.
        ("host.shared_buffer.create", plugin_only(Mutate, &[])),
        // ── 타이머 관측 ───────────────────────────────────────────────
        // 호스트 내부 주기 작업의 진단 목록이므로 Local 전용이다.
        ("timer.list", local_only(Read)),
        // ── workspace (read/write) ────────────────────────────────────
        ("workspace.list", plugin(Read, &[SurfaceRead])),
        (
            "workspace.create",
            plugin(Mutate, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "workspace.update",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "workspace.move",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "workspace.close",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        // ── workspace category (사이드바 폴더 CRUD) ──────────────────
        ("workspace_category.list", plugin(Read, &[SurfaceRead])),
        (
            "workspace_category.create",
            plugin(Mutate, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "workspace_category.rename",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "workspace_category.delete",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "workspace_category.move",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        // ── pane / split ──────────────────────────────────────────────
        ("pane.list", plugin(Read, &[SurfaceRead])),
        (
            "pane.close",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "split",
            plugin(Mutate, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        // ── tab ───────────────────────────────────────────────────────
        ("tab.list", plugin(Read, &[SurfaceRead])),
        (
            "tab.create",
            plugin(Mutate, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "tab.close",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "tab.move",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        // ── preset (layout preset CRUD + apply) ───────────────────────
        ("preset.list", plugin(Read, &[SurfaceRead])),
        ("preset.get", plugin(Read, &[SurfaceRead])),
        ("preset.save", plugin(Idempotent, &[SurfaceWrite])),
        ("preset.delete", plugin(Idempotent, &[SurfaceWrite])),
        ("preset.rename", plugin(Idempotent, &[SurfaceWrite])),
        ("preset.capture", plugin(Mutate, &[SurfaceWrite])),
        (
            "preset.apply",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        // ── surface (구조 조작) ───────────────────────────────────────
        ("surface.list", plugin(Read, &[SurfaceRead])),
        // 생성 없이 등록 여부를 확인한다. 종류·번역 키·렌더 경로·출처만 공개한다.
        ("surface.kinds", plugin(Read, &[SurfaceRead])),
        (
            "surface.close",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "surface.close_self",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        ("tree", plugin(Read, &[SurfaceRead])),
        ("webview.set_url", plugin(Idempotent, &[SurfaceWrite])),
        // 실린 문서를 다시 싣지 않고 JSON 메시지를 보낸다. set_url과 같은 권한이다.
        // 같은 메시지를 두 번 보내면 페이지가 두 번 처리하므로 Idempotent가 아니다.
        ("webview.post_message", plugin(Mutate, &[SurfaceWrite])),
        // webview 플러그인은 문서를 만들 때 전역 테마를 조회한다. surface별 정보가 아니므로 추가 토큰은 없다.
        ("theme.query", plugin(Read, &[])),
        // webview 플러그인이 페이지가 처리할 동작의 단축키를 문서에 넣을 때 조회한다. 전역 설정이다.
        ("settings.get_keybinding", plugin(Read, &[])),
        ("surface.set_cwd", plugin(Idempotent, &[SurfaceWrite])),
        ("surface.meta.get", plugin(Read, &[SurfaceRead])),
        ("surface.meta.list", plugin(Read, &[SurfaceRead])),
        ("surface.meta.set", plugin(Idempotent, &[SurfaceWrite])),
        ("surface.meta.unset", plugin(Idempotent, &[SurfaceWrite])),
        // ── terminal I/O ──────────────────────────────────────────────
        ("surface.send", plugin(Mutate, &[TerminalWrite])),
        ("surface.send_key", plugin(Mutate, &[TerminalWrite])),
        ("surface.send_combo", plugin(Mutate, &[TerminalWrite])),
        ("surface.send_to", plugin(Mutate, &[TerminalWrite])),
        ("surface.send_wait_idle", plugin(Mutate, &[TerminalWrite])),
        (
            "surface.wake",
            plugin(Mutate, &[TerminalSpawn]).kept_in_structure_journal(),
        ),
        ("surface.set_mark", plugin(Mutate, &[TerminalRead])),
        // 완료 신호는 attention 상태를 바꾸므로 알림 권한을 요구한다.
        ("surface.completion", plugin(Mutate, &[Notification])),
        // 같은 attention 상태의 조회·해제도 Notification 권한으로 묶는다.
        ("surface.attention.get", plugin(Read, &[Notification])),
        (
            "surface.attention.clear",
            plugin(Idempotent, &[Notification]),
        ),
        ("surface.read_since_mark", plugin(Read, &[TerminalRead])),
        // 스캐너 전용 커서는 읽은 구간을 소비하므로 Read가 아닌 Mutate다.
        // TerminalRead를 요구하되 다른 소비자가 위치를 바꾸지 않도록 CLI에는 노출하지 않는다.
        (
            "surface.read_since_scan_mark",
            plugin(Mutate, &[TerminalRead]),
        ),
        ("surface.mouse_tracking", plugin(Read, &[TerminalRead])),
        ("surface.parse_since_mark", plugin(Read, &[TerminalRead])),
        ("surface.commands", plugin(Read, &[TerminalRead])),
        ("surface.last_command", plugin(Read, &[TerminalRead])),
        ("surface.command_at", plugin(Read, &[TerminalRead])),
        // ── output observer ─────────────────────────────────────────
        ("output.observe_start", plugin(Mutate, &[TerminalRead])),
        ("output.observe_stop", plugin(Idempotent, &[TerminalRead])),
        ("output.observe_list", plugin(Read, &[TerminalRead])),
        ("output.observe_info", plugin(Read, &[TerminalRead])),
        ("surface.screen_text", plugin(Read, &[TerminalRead])),
        ("surface.cursor_position", plugin(Read, &[TerminalRead])),
        ("surface.foreground_process", plugin(Read, &[TerminalRead])),
        ("surface.locate", plugin(Read, &[SurfaceRead])),
        ("surface.html_script", plugin(Read, &[SurfaceRead])),
        ("surface.html_script_allow", plugin(Mutate, &[SurfaceWrite])),
        ("surface.html_remote_allow", plugin(Mutate, &[SurfaceWrite])),
        ("surface.html_nav", plugin(Read, &[SurfaceRead])),
        ("surface.html_back", plugin(Mutate, &[SurfaceWrite])),
        ("surface.html_forward", plugin(Mutate, &[SurfaceWrite])),
        ("surface.html_reload", plugin(Mutate, &[SurfaceWrite])),
        (
            "surface.respawn_terminal",
            plugin(Mutate, &[TerminalSpawn]).kept_in_structure_journal(),
        ),
        ("surface.is_typing", plugin(Read, &[TerminalRead])),
        // ── child-terminal 관리 (docs/features/child-terminal/index.md) ─────────────
        // 자식 터미널 작업이 사용하는 생성·입력·닫기 권한을 함께 요구한다.
        (
            "terminal.spawn",
            plugin(Mutate, &[SurfaceWrite, TerminalWrite, TerminalSpawn])
                .kept_in_structure_journal(),
        ),
        ("terminal.tell", plugin(Mutate, &[TerminalWrite])),
        ("terminal.children", plugin(Read, &[SurfaceRead])),
        ("terminal.parent", plugin(Read, &[SurfaceRead])),
        ("terminal.state", plugin(Read, &[SurfaceRead])),
        (
            "terminal.kill",
            plugin(Idempotent, &[SurfaceWrite]).kept_in_structure_journal(),
        ),
        (
            "terminal.respawn",
            plugin(Mutate, &[TerminalWrite, TerminalSpawn]),
        ),
        ("terminal.broadcast", plugin(Mutate, &[TerminalWrite])),
        ("terminal.set_state", plugin(Idempotent, &[SurfaceWrite])),
        // surface 생성 없이 자식 관계와 soft 점유만 등록한다.
        ("terminal.adopt", plugin(Idempotent, &[SurfaceWrite])),
        // 자식 관계·soft 점유만 해제하며 surface를 닫지 않는다.
        ("terminal.release", plugin(Idempotent, &[SurfaceWrite])),
        // ── headless PTY primitive (docs/features/headless-pty/index.md#내부-동작-headless-valid /
        // pty_registry) ────────────────────────────────────────────────
        // surface 트리를 바꾸지 않는 PTY 작업에는 Terminal 권한만 요구한다.
        ("pty.spawn", plugin(Mutate, &[TerminalSpawn])),
        ("pty.write", plugin(Mutate, &[TerminalWrite])),
        ("pty.read", plugin(Read, &[TerminalRead])),
        ("pty.wait", plugin(Read, &[TerminalRead])),
        ("pty.kill", plugin(Idempotent, &[TerminalWrite])),
        ("pty.list", plugin(Read, &[TerminalRead])),
        // PTY를 실제 surface로 만들므로 생성 권한에 SurfaceWrite를 추가한다.
        (
            "pty.attach_surface",
            plugin(Mutate, &[SurfaceWrite, TerminalSpawn]).kept_in_structure_journal(),
        ),
        ("surface.fire_hook", plugin(Mutate, &[SurfaceWrite])),
        // ── hooks ─────────────────────────────────────────────────────
        ("hook.set", plugin(Idempotent, &[SurfaceWrite])),
        ("hook.list", plugin(Read, &[SurfaceRead])),
        ("hook.unset", plugin(Idempotent, &[SurfaceWrite])),
        ("global_hook.set", plugin(Idempotent, &[SurfaceWrite])),
        ("global_hook.list", plugin(Read, &[SurfaceRead])),
        ("global_hook.unset", plugin(Idempotent, &[SurfaceWrite])),
        // ── webhook (인바운드 웹훅 리스너 — lifetime 6종/영속화 포함) ──────
        // 플러그인은 Network 권한으로 자기 hook handler만 등록할 수 있다.
        // 인라인 sequence는 거절하며 조회·해제·설정은 Local 전용이다.
        ("webhook.register", plugin(Mutate, &[Network])),
        ("webhook.list", local_only(Read)),
        ("webhook.info", local_only(Read)),
        ("webhook.unregister", local_only(Idempotent)),
        ("webhook.sweep", local_only(Idempotent)),
        ("webhook.config", local_only(Idempotent)),
        // ── message (surface 간 메시지 큐) ─────────────────────────────
        ("message.send", plugin(Mutate, &[SurfaceWrite])),
        ("message.read", plugin(Mutate, &[SurfaceRead])),
        ("message.count", plugin(Read, &[SurfaceRead])),
        ("message.clear", plugin(Idempotent, &[SurfaceWrite])),
        // ── image surface ─────────────────────────────────────────────
        // image prefix가 플러그인에 등록돼도 정적 메서드는 이 표의 권한을 적용한다.
        // 외부 요청이 namespace로 전달될 수 있어 Mutate에는 해당 경로의 키 기능 버전이 필요하다.
        (
            "image.open",
            plugin(Mutate, &[SurfaceWrite, FsRead]).kept_on_every_host_path(),
        ),
        ("image.save", plugin(Idempotent, &[FsWrite])),
        (
            "image.export_png",
            plugin(Mutate, &[FsWrite]).kept_on_every_host_path(),
        ),
        (
            "image.next",
            plugin(Mutate, &[SurfaceWrite]).kept_on_every_host_path(),
        ),
        (
            "image.prev",
            plugin(Mutate, &[SurfaceWrite]).kept_on_every_host_path(),
        ),
        (
            "image.paste",
            plugin(Mutate, &[SurfaceWrite, ClipboardRead]).kept_on_every_host_path(),
        ),
        ("image.reload", plugin(Idempotent, &[SurfaceWrite, FsRead])),
        ("image.list", plugin(Read, &[SurfaceRead])),
        // ── clipboard ──────────────────────────────────────────────────
        ("clipboard.set_text", plugin(Idempotent, &[ClipboardWrite])),
        // ── memory: regular (공유 네임스페이스, owner enforcement) ────
        ("memory.put", plugin(Idempotent, &[MemoryWrite])),
        ("memory.get", plugin(Read, &[MemoryRead])),
        ("memory.delete", plugin(Idempotent, &[MemoryWrite])),
        ("memory.list", plugin(Read, &[MemoryRead])),
        ("memory.exists", plugin(Read, &[MemoryRead])),
        ("memory.count", plugin(Read, &[MemoryRead])),
        ("memory.scopes", plugin(Read, &[MemoryRead])),
        ("memory.stats", plugin(Read, &[MemoryRead])),
        ("memory.query", plugin(Read, &[MemoryRead])),
        ("memory.export", plugin(Mutate, &[MemoryRead])),
        ("memory.import", plugin(Mutate, &[MemoryWrite])),
        // ── memory: secret (plugin 별 사전 분할) ──────────────────────
        ("memory.secret.put", plugin(Idempotent, &[MemorySecret])),
        ("memory.secret.get", plugin(Read, &[MemorySecret])),
        ("memory.secret.delete", plugin(Idempotent, &[MemorySecret])),
        ("memory.secret.list", plugin(Read, &[MemorySecret])),
        ("memory.secret.exists", plugin(Read, &[MemorySecret])),
        ("memory.secret.count", plugin(Read, &[MemorySecret])),
        ("memory.secret.scopes", plugin(Read, &[MemorySecret])),
        ("memory.secret.stats", plugin(Read, &[MemorySecret])),
        // ── memory: 유지 보수 (host 전용) ─────────────────────────────
        ("memory.gc", local_only(Idempotent)),
        // ── memory: blackboard (workspace-scoped) ─────────────────────
        ("memory.bb_create", plugin(Idempotent, &[MemoryWrite])),
        ("memory.bb_put", plugin(Idempotent, &[MemoryWrite])),
        ("memory.bb_get", plugin(Read, &[MemoryRead])),
        ("memory.bb_get_all", plugin(Read, &[MemoryRead])),
        ("memory.bb_get_meta", plugin(Read, &[MemoryRead])),
        ("memory.bb_delete_field", plugin(Idempotent, &[MemoryWrite])),
        ("memory.bb_delete", plugin(Idempotent, &[MemoryWrite])),
        ("memory.bb_list", plugin(Read, &[MemoryRead])),
        ("memory.bb_exists", plugin(Read, &[MemoryRead])),
        // ── memory: bb snapshot ────────────────────────────────────────
        ("memory.bb_snapshot", plugin(Idempotent, &[MemoryWrite])),
        ("memory.bb_snapshot_get", plugin(Read, &[MemoryRead])),
        ("memory.bb_snapshot_list", plugin(Read, &[MemoryRead])),
        (
            "memory.bb_snapshot_delete",
            plugin(Idempotent, &[MemoryWrite]),
        ),
        (
            "memory.bb_snapshot_restore",
            plugin(Idempotent, &[MemoryWrite]),
        ),
        // ── memory: plan (workspace-scoped) ───────────────────────────
        ("memory.plan_create", plugin(Idempotent, &[MemoryWrite])),
        ("memory.plan_get", plugin(Read, &[MemoryRead])),
        ("memory.plan_list", plugin(Read, &[MemoryRead])),
        ("memory.plan_delete", plugin(Idempotent, &[MemoryWrite])),
        ("memory.plan_add_step", plugin(Mutate, &[MemoryWrite])),
        (
            "memory.plan_remove_step",
            plugin(Idempotent, &[MemoryWrite]),
        ),
        (
            "memory.plan_update_step",
            plugin(Idempotent, &[MemoryWrite]),
        ),
        // ── memory: cache (TTL 캐시) ───────────────────────────────────
        ("memory.cache_put", plugin(Idempotent, &[MemoryWrite])),
        ("memory.cache_get", plugin(Read, &[MemoryRead])),
        (
            "memory.cache_invalidate",
            plugin(Idempotent, &[MemoryWrite]),
        ),
        ("memory.cache_clear", plugin(Idempotent, &[MemoryWrite])),
        ("memory.cache_list", plugin(Read, &[MemoryRead])),
        // ── memory: goal (surface-scoped 단일 목표 문장) ──────────────
        ("memory.goal_set", plugin(Idempotent, &[MemoryWrite])),
        ("memory.goal_get", plugin(Read, &[MemoryRead])),
        ("memory.goal_clear", plugin(Idempotent, &[MemoryWrite])),
        // ── approval (휴먼 핸드오프) ──────────────────────────────────
        ("approval.request", plugin(Mutate, &[Approval])),
        ("approval.respond", plugin(Idempotent, &[Approval])),
        // await 는 blocking + timeout 이라 main thread 가 막히면 안 됨.
        // process_ipc 에서 worker thread 로 분리 처리되며, plugin 호출은 미지원.
        ("approval.await", local_only(Read)),
        ("approval.cancel", plugin(Idempotent, &[Approval])),
        ("approval.list", plugin(Read, &[Approval])),
        ("approval.get", plugin(Read, &[Approval])),
        ("approval.history", plugin(Read, &[Approval])),
        (
            "approval.summary.set",
            plugin(Idempotent, &[Approval, MemoryWrite]),
        ),
        (
            "approval.summary.get",
            plugin(Read, &[Approval, MemoryRead]),
        ),
        // ── telemetry (관측 / 비용) ───────────────────────────────────
        ("telemetry.record", plugin(Mutate, &[Telemetry])),
        ("telemetry.record_batch", plugin(Mutate, &[Telemetry])),
        ("telemetry.summary", plugin(Read, &[Telemetry])),
        ("telemetry.timeseries", plugin(Read, &[Telemetry])),
        ("telemetry.top", plugin(Read, &[Telemetry])),
        ("telemetry.cap.set", plugin(Idempotent, &[Telemetry])),
        ("telemetry.cap.list", plugin(Read, &[Telemetry])),
        ("telemetry.cap.remove", plugin(Idempotent, &[Telemetry])),
        ("telemetry.cap.status", plugin(Read, &[Telemetry])),
        ("telemetry.cap.reset", plugin(Idempotent, &[Telemetry])),
        ("telemetry.anomaly.list", plugin(Read, &[Telemetry])),
        ("telemetry.session_summary", plugin(Read, &[Telemetry])),
        // ── events (사건 피드 조회) ──────────────────────────────────
        // 커서는 소비자가 보관하며 서버의 읽기 위치를 바꾸지 않는다.
        // 피드가 갱신되므로 같은 요청의 답이 항상 같다는 뜻은 아니다.
        // 긴 대기가 SDK 워커를 막을 수 있어 Local 전용이며 플러그인은 이벤트 버스를 구독한다.
        ("events.fetch", local_only(Read)),
        // ── agent (협업 primitive) ────────────────────────────────────
        ("agent.task_create", plugin(Mutate, &[AgentManage])),
        ("agent.task_list", plugin(Read, &[AgentManage])),
        ("agent.task_get", plugin(Read, &[AgentManage])),
        // SDK 단일 워커를 막는 대기는 Local 전용이다. 플러그인은 완료 전략이나 task_get 폴링을 쓴다.
        ("agent.task_await", local_only(Read)),
        ("agent.task_cancel", plugin(Idempotent, &[AgentManage])),
        ("agent.task_retry", plugin(Mutate, &[AgentManage])),
        ("agent.task_graph", plugin(Read, &[AgentManage])),
        ("agent.task_graph_validate", plugin(Read, &[AgentManage])),
        ("agent.task_graph_submit", plugin(Mutate, &[AgentManage])),
        ("agent.dag_list", plugin(Read, &[AgentManage])),
        ("agent.dag_get", plugin(Read, &[AgentManage])),
        ("agent.dag_report", plugin(Read, &[AgentManage])),
        // 실행 중인 회차가 받은 주소(토큰 포함)로 그 블록에만 쓴다. agent task 의 세션은 세션 토큰으로
        // 부르므로 Local 전용이면 쓸 수 없다. 주소의 토큰이 대상 회차를 묶는다.
        ("agent.report_append", plugin(Mutate, &[AgentManage])),
        // Custom 작업의 상태는 러너가 관리한다. 플러그인의 별도 상태 변경과 경합하지 않도록 Local 전용이다.
        ("agent.task_set_result", local_only(Idempotent)),
        // agent task 회차에 결과를 제안한다. 종결은 러너가 한다. 세션 토큰의 agent 는 자기 세션의
        // 회차에만 낼 수 있고(핸들러 검사), task_set_result 의 Local 전용은 그대로 둔다.
        (
            "agent.task_submit_result",
            plugin(Idempotent, &[AgentManage]),
        ),
        // provider 플러그인의 턴 시작·종료 보고. 다른 agent.* 를 열지 않는 전용 권한을 요구하고,
        // 핸들러가 호출자가 provider namespace 를 소유한 플러그인인지도 확인한다(그 밖은 -32001).
        (
            "agent.task_turn_report",
            plugin(Idempotent, &[AgentTurnReport]),
        ),
        // 러너는 자동 재시작하지 않으므로 플러그인도 명시적으로 시작·중지할 수 있다.
        ("agent.task_run", plugin(Mutate, &[AgentManage])),
        ("agent.task_delete", plugin(Idempotent, &[AgentManage])),
        ("agent.task_purge", plugin(Idempotent, &[AgentManage])),
        ("agent.barrier_create", plugin(Idempotent, &[AgentManage])),
        ("agent.barrier_signal", plugin(Mutate, &[AgentManage])),
        ("agent.barrier_await", plugin(Read, &[AgentManage])),
        ("agent.barrier_state", plugin(Read, &[AgentManage])),
        ("agent.semaphore_create", plugin(Idempotent, &[AgentManage])),
        (
            "agent.semaphore_set_permits",
            plugin(Idempotent, &[AgentManage]),
        ),
        (
            "agent.semaphore_acquire",
            plugin(Idempotent, &[AgentManage]),
        ),
        (
            "agent.semaphore_release",
            plugin(Idempotent, &[AgentManage]),
        ),
        ("agent.barrier_list", plugin(Read, &[AgentManage])),
        ("agent.barrier_delete", plugin(Idempotent, &[AgentManage])),
        ("agent.semaphore_list", plugin(Read, &[AgentManage])),
        ("agent.semaphore_delete", plugin(Idempotent, &[AgentManage])),
        ("agent.lease_acquire", plugin(Idempotent, &[AgentManage])),
        ("agent.lease_release", plugin(Idempotent, &[AgentManage])),
        ("agent.lease_list", plugin(Read, &[AgentManage])),
        ("agent.task_reduce", plugin(Mutate, &[AgentManage])),
        ("agent.rate_limit_set", plugin(Idempotent, &[AgentManage])),
        ("agent.rate_limit_list", plugin(Read, &[AgentManage])),
        (
            "agent.rate_limit_remove",
            plugin(Idempotent, &[AgentManage]),
        ),
        ("agent.rate_limit_status", plugin(Read, &[AgentManage])),
        // ── session.* (자식 agent 신원 토큰) ──────────────────────────
        // 플러그인이 자식 세션을 발급·철회할 수 있다. 전체 목록은 운영자용으로 제한한다.
        ("session.issue", plugin(Mutate, &[AgentManage])),
        ("session.revoke", plugin(Idempotent, &[AgentManage])),
        ("session.list", local_only(Read)),
        // attach 점유 제어
        // 스트림 점유는 handshake에서 처리한다. JSON-RPC attach 제어는 여기 등록한다.
        // SSH와 loopback 연결을 신뢰 경계로 사용하며 추가 권한 토큰은 요구하지 않는다.
        // 남의 점유를 강제로 푸는 메서드는 두지 않는다. 원격 사용자를 끊는 것은 로컬
        // GUI 조작이며(docs/principles/control-and-data.md), 그 재현은 DEBUG_METHODS에 있다.
        ("attach.acquire", plugin(Idempotent, &[])),
        ("attach.release", plugin(Idempotent, &[])),
        ("attach.into_gui", plugin(Mutate, &[]).kept_by_app_layer()),
        ("attach.list", plugin(Read, &[])),
        // 원격 자식 에이전트. client 쪽 plugin 이 자기 namespace 로 holder 연결을 거쳐 서버 plugin 을 부르고,
        // 서버 쪽 plugin 이 그 자식의 고유 결과를 보낸다. 호출 plugin 의 기존 터미널 권한을 그대로 요구한다.
        // agent_target 은 읽기 전용이라 CLI 도 경로 인자를 어느 머신 기준으로 다룰지 정하려고 묻는다.
        // call·commit·notice 는 외부 arm 이 있고 핸들러가 plugin 이 아닌 호출을 거절한다(file_picker.trigger 와 같다).
        ("attach.agent_target", plugin(Read, &[])),
        (
            "attach.agent_call",
            plugin(Mutate, &[TerminalSpawn, TerminalWrite]),
        ),
        ("attach.agent_commit", plugin(Idempotent, &[TerminalSpawn])),
        ("attach.child_notice", plugin(Idempotent, &[TerminalWrite])),
        ("attach.child_notices", plugin(Read, &[])),
        // ── remote.profile.* (원격 접속 프로필 CRUD) ─────────────────────
        // 프로필은 비밀 없는 장비 인벤토리(passkey 를 이름으로 참조만). attach.* 와 동일하게
        // 연결 경계(소켓 도달)에 신뢰를 위임 — 추가 Permission 불요.
        // (구 tool.ssh.* / ssh.profile.* 는 alias.rs 로 한시 호환.)
        ("remote.profile.list", plugin(Read, &[])),
        ("remote.profile.get", plugin(Read, &[])),
        ("remote.profile.add", plugin(Idempotent, &[])),
        ("remote.profile.detect", plugin(Read, &[])),
        ("remote.profile.remove", plugin(Idempotent, &[])),
        // SSH config에서는 alias와 경로 정보를 조회하며 키 내용은 반환하지 않는다.
        ("remote.profile.list_local", plugin(Read, &[])),
        ("remote.profile.import", plugin(Mutate, &[])),
        // ── remote.workspaces (원격 ws 브라우징) ──────────────────────────
        // CLI와 같은 원격 조회 기능. 연결·SSH 신뢰 경계에 따르며 추가 권한 토큰은 없다.
        ("remote.workspaces", plugin(Read, &[])),
        // 호출자가 연 터널을 받는 attach.into_gui 와 달리 호스트가 터널을 직접 연다. 연결 경계는 같아
        // 권한 토큰을 더 요구하지 않고, mirror 를 만들어도 포커스는 옮기지 않는다.
        ("remote.attach", plugin(Mutate, &[]).kept_by_app_layer()),
        // remote.attach 응답의 attempt 로 비동기 거절을 찾으므로 호출자 범위를 같이 둔다.
        ("remote.refusals", plugin(Read, &[])),
        // remote.attach 로 붙인 mirror 만 뗀다. 원격 workspace 는 남고, 두 번째 호출은 같은 끝 상태에서
        // mirror 가 없다는 오류로 답한다. 연결 경계는 remote.attach 와 같아 권한 토큰을 요구하지 않는다.
        ("remote.disconnect", plugin(Idempotent, &[])),
        // 저장된 원격 행만 연결한다. 사용자가 끊은 행을 다시 붙이는 명시적 요청이라 그 표지를 지운다.
        // 연결 경계는 remote.attach 와 같아 권한 토큰을 요구하지 않고 포커스도 옮기지 않는다.
        ("remote.connect", plugin(Idempotent, &[])),
        // ── remote.passkey.* (자격증명 CRUD) ─────────────────────────────
        // 값 마스킹은 핸들러가 보장(list/get 은 name+kind 만, 파일 내용 미반환). 등록은
        // 쓰기라 허용. 권한은 프로필과 동일 — 연결 경계 위임(docs/design/systems/memory.md#passkey-저장과-열람).
        ("remote.passkey.list", plugin(Read, &[])),
        ("remote.passkey.get", plugin(Read, &[])),
        ("remote.passkey.add", plugin(Idempotent, &[])),
        ("remote.passkey.remove", plugin(Idempotent, &[])),
        // ── notification ──────────────────────────────────────────────
        ("notification.list", plugin(Read, &[Notification])),
        ("notification.counts", plugin(Read, &[Notification])),
        (
            "notification.read",
            plugin(Mutate, &[Notification]).kept_by_app_layer(),
        ),
        ("notification.create", plugin(Mutate, &[Notification])),
        // ── settings (plugin 이 자기 plugin_settings 값을 read-back) ──────
        // [[contributes.settings_pages]] 를 선언하려면 이미 UiSettingsPage 권한이
        // 필요하므로(위 permission variant 재사용), 그 값을 다시 읽는 IPC 도
        // 동일 권한으로 게이트한다. caller_plugin_id 는 요청 파라미터가 아니라
        // CallerContext 에서 강제 도출 — 다른 plugin 값 조회 불가.
        (
            "settings.get_plugin_setting",
            plugin(Read, &[UiSettingsPage]),
        ),
        // ── settings.remote_transfer (원격 전송 저장 정책 get/set) ──────
        // 전역 저장 정책은 Local 전용이다. set은 호스트의 UpdateSettings 경로로 저장한다.
        ("settings.get_remote_transfer", local_only(Read)),
        ("settings.get_input_rules", local_only(Read)),
        ("settings.set_input_rule", local_only(Idempotent)),
        ("settings.remove_input_rule", local_only(Idempotent)),
        // Settings contributors may seed absent input defaults once; they cannot
        // overwrite user rules or restore a rule the user has removed.
        (
            "settings.initialize_input_rule",
            plugin(Idempotent, &[UiSettingsPage]),
        ),
        ("settings.set_remote_transfer", local_only(Idempotent)),
        // ── file_handler.* (host config 관리 — local-only) ───────────
        ("file_handler.reload", local_only(Idempotent)),
        // 사용자 설정의 출처별 원문까지 반환하므로 Local 전용이다.
        ("file_handler.detectors", local_only(Read)),
        // 지정 경로를 핸들러로 열므로 FsRead가 필요하다.
        ("file_handler.dispatch", plugin(Mutate, &[FsRead])),
        // ── hook_handler.* (공유 훅 핸들러 레지스트리 — local-only) ───────
        // 공유 훅 설정의 조회·변경·실행은 현재 Local 전용이다.
        ("hook_handler.list", local_only(Read)),
        ("hook_handler.get", local_only(Read)),
        // IpcSequence는 Local 권한으로 실행하므로 플러그인이 수정하면 권한 범위를 우회할 수 있다.
        ("hook_handler.upsert", local_only(Idempotent)),
        ("hook_handler.remove", local_only(Idempotent)),
        ("hook_handler.reload", local_only(Idempotent)),
        ("hook_handler.dispatch", local_only(Mutate)),
        // dispatch가 돌려준 run의 진행·최종 결과 조회.
        ("hook_handler.result", local_only(Read)),
        // 접수 응답의 run으로 진행·최종 결과를 조회한다. 플러그인·에이전트는 자기가 시작한 run만 읽는다.
        ("run.result", plugin(Read, &[])),
        // ── completion_strategy.* (완료 판정 전략 레지스트리 — local-only) ──
        // 비활성 항목을 포함한 전략 목록만 제공한다. 재로드·직접 실행 명령은 없다.
        ("completion_strategy.list", local_only(Read)),
        // markdown surface 제자리 이동 — 주어진 surface 를 새 파일의 markdown
        // 으로 교체한다. 임의 path 를 읽으므로 FsRead. markdown 주소창 플러그인이 caller.
        (
            "markdown.navigate",
            plugin(Mutate, &[FsRead]).kept_on_every_host_path(),
        ),
        // 이미 연 항목의 목록만 반환하며 파일을 읽지 않아 SurfaceRead를 요구한다.
        ("recent.query", plugin(Read, &[SurfaceRead])),
        // ── git_viewer.* (docs/dev-guide/attach-behavior.md#커스텀-이벤트-확장-streamcontrol-밖-raw-json-event-태그
        // — 원격 attach mirror git 조회 트리거) ─
        // 원격 Git 조회를 비동기로 요청한다. request_id 뒤 실제 결과는 event.dispatch로 받는다.
        ("git_viewer.query", plugin(Read, &[FsRead])),
        // ── markdown_mirror.* (docs/dev-guide/attach-behavior.md#markdown-content-채널
        // — 원격 attach mirror markdown 원문 조회 트리거) ─
        // 원격 문서 원문을 요청하며 실제 결과는 event.dispatch로 받는다. 파일 읽기라 FsRead를 요구한다.
        ("markdown_mirror.content_request", plugin(Mutate, &[FsRead])),
        // ── file_picker.* (plugin 트리거 host 소유 file_picker popup) ─
        // 팝업을 열고 request_id를 반환한다. 선택/취소 결과는 file_picker.result 이벤트로 보낸다.
        // 비플러그인 호출은 핸들러가 -32016으로 거절한다. 외부 arm이 있어 plugin_only와는 구분한다.
        ("file_picker.trigger", plugin(Mutate, &[FsRead])),
        // ── popup (plugin → host) ─────────────────────────────────────
        // 자신의 팝업 인스턴스만 닫을 수 있다. 소유권은 호스트가 검사한다.
        ("popup.close", plugin_only(Idempotent, &[UiPopup])),
        // 플러그인 배너
        // 자기 배너를 소유 surface에 표시한다. App에서 소유권을 확인한다.
        ("banner.open", plugin_only(Idempotent, &[UiBanner])),
        // 자기 배너 인스턴스를 명시적으로 닫는다.
        ("banner.close", plugin_only(Idempotent, &[UiBanner])),
        // ── webview 외부 열기 (plugin → host) ──────────────────────────
        // 자신의 webview 외부 링크를 호스트가 연다. 소유권은 App에서 검사하며 외부 IPC 경로는 없다.
        (
            "webview.open_external",
            plugin_only(Mutate, &[SurfaceWrite]),
        ),
        // ── 호스트 자체 메서드 (plugin/window 관리) — local-only ──────
        ("plugin.list", local_only(Read)),
        ("plugin.show", local_only(Read)),
        ("plugin.extension.list", local_only(Read)),
        ("plugin.install", local_only(Mutate).kept_by_app_layer()),
        ("plugin.remove", local_only(Idempotent)),
        ("plugin.enable", local_only(Idempotent)),
        ("plugin.disable", local_only(Idempotent)),
        // 번들 바이너리 교체는 운영자·업그레이드 경로가 맡으며 플러그인 자신에게 열지 않는다.
        ("plugin.upgrade_builtins", local_only(Idempotent)),
        ("plugin.permissions", local_only(Read)),
        ("plugin.grant", local_only(Idempotent)),
        ("plugin.revoke", local_only(Idempotent)),
        // agent 임시 grant. grant/revoke 는 user/operator 만, list 는
        // readonly 라 plugin/agent 도 self-introspection 가능.
        ("plugin.grant_agent_permission", local_only(Idempotent)),
        ("plugin.revoke_agent_permission", local_only(Idempotent)),
        ("plugin.list_agent_permissions", plugin(Read, &[])),
        // 추가 권한 요청은 사용자 승인이 필요하므로 Approval 권한을 요구한다.
        (
            "plugin.request_permission",
            plugin(Mutate, &[Approval]).kept_by_app_layer(),
        ),
        // audit log 조회/집계/삭제. 운영자 전용.
        ("plugin.audit_query", local_only(Read)),
        ("plugin.audit_summary", local_only(Read)),
        ("plugin.audit_follow", local_only(Read)),
        ("plugin.audit_clear", local_only(Idempotent)),
        ("window.create", local_only(Mutate).kept_by_app_layer()),
        ("window.close", local_only(Idempotent)),
        ("window.list", local_only(Read)),
        // ui.screenshot — 정식 focus-독립 캡처. 대상 window/surface 를 ID 로 지정하며
        // focused 창에 의존하지 않는다(docs/design/policies/focus.md). 임의 경로 파일 쓰기 표면이라 local_only
        // (plugin 미노출) — CLI/로컬 client 만 호출.
        ("ui.screenshot", local_only(Mutate).kept_by_app_layer()),
        // view.*는 window.*의 호환 이름이다. 외부 payload의 window_id도 유지한다.
        ("view.create", local_only(Mutate).kept_by_app_layer()),
        ("view.close", local_only(Idempotent)),
        ("view.list", local_only(Read)),
        // window.focus / view.focus 는 debug 빌드 전용 (DEBUG_METHODS 참조).
        // CLAUDE.md: 포커스 전환은 사용자 단축키/마우스 입력 영역.
        // script.reload (init.lua 재로드)는 제공하지 않는다(docs/features/lua-hooks/index.md#실행-격리--안전-장치) — 스크립트는
        // 등록 목록 + 명시 트리거(단축키)로만 실행되고 부팅 자동로드가 폐기됐다.
    ]
};

/// debug 빌드의 로컬 진단·입력 재현 메서드. 핸들러와 라우터도 같은 cfg로 제한한다.
/// GUI debug step에서 끝나는 Mutate는 kept_on_every_host_path로 기능 버전을 선언한다.
/// 실제 dispatch와의 일치는 key_contract_by_layer가 검사한다.
#[cfg(debug_assertions)]
pub const DEBUG_METHODS: &[(&str, MethodMeta)] = &[
    ("system.shutdown", local_only(MethodEffect::Idempotent)),
    ("ui.state", local_only(MethodEffect::Read)),
    ("debug.info", local_only(MethodEffect::Read)),
    ("debug.cell_info", local_only(MethodEffect::Read)),
    ("debug.screen_attrs", local_only(MethodEffect::Read)),
    ("debug.glyph_color", local_only(MethodEffect::Read)),
    ("debug.feed_bytes", local_only(MethodEffect::Mutate)),
    // GPU 결함 주입 — 이벤트 루프를 실제로 멎게 만드는 파괴적 표면이라 plugin 미노출.
    ("debug.gpu.stall", local_only(MethodEffect::Mutate)),
    ("debug.inject_mouse", local_only(MethodEffect::Mutate)),
    ("debug.inject_key", local_only(MethodEffect::Mutate)),
    // 텍스트는 키와 별도 이벤트이므로 egui-key만으로 TextEdit에 입력할 수 없다.
    (
        "debug.inject_window_mouse",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    (
        "debug.inject_egui_mouse",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    (
        "debug.inject_egui_key",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    (
        "debug.inject_egui_text",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    // 임의 Lua 주입(docs/features/lua-hooks/index.md#실행-격리--안전-장치) — release 에는 이 경로가 없다(사용자 조작 재현). local 전용.
    (
        "debug.lua.eval",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    // 사용자 조작 재현(워크스페이스 닫기 / 워크스페이스·탭 전환) — 위 inject_*
    // 와 같은 계열이라 같은 debug 격리.
    (
        "debug.close_workspace",
        local_only(MethodEffect::Idempotent),
    ),
    (
        "debug.switch_workspace",
        local_only(MethodEffect::Idempotent),
    ),
    ("debug.switch_tab", local_only(MethodEffect::Idempotent)),
    // 로컬 사용자의 강제 끊기(점유 테두리 버튼·사이드바 확인 팝업) 재현. release에는
    // 원격 점유를 강제로 푸는 공개 메서드가 없다.
    (
        "debug.attach.force_detach",
        local_only(MethodEffect::Idempotent),
    ),
    (
        "debug.attach.force_detach_workspace",
        local_only(MethodEffect::Idempotent),
    ),
    // 마우스 라우팅 회귀 안전망용 read-only dump — 관찰 전용(사용자 상태 불변). release 미노출.
    ("debug.selection", local_only(MethodEffect::Read)),
    ("debug.pending_menu", local_only(MethodEffect::Read)),
    // native 메뉴 항목 선택 재현(사용자 우클릭 조작) — inject_* 와 같은 debug 격리.
    (
        "debug.native_menu.answer",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    ("debug.pending_move", local_only(MethodEffect::Read)),
    ("debug.focused_surface", local_only(MethodEffect::Read)),
    ("debug.surface_rect", local_only(MethodEffect::Read)),
    ("debug.tool.list", local_only(MethodEffect::Read)),
    ("debug.tool.invoke", local_only(MethodEffect::Mutate)),
    ("debug.popup.list", local_only(MethodEffect::Read)),
    ("debug.popup.open", local_only(MethodEffect::Idempotent)),
    ("debug.popup.close", local_only(MethodEffect::Idempotent)),
    ("debug.host_popup.list", local_only(MethodEffect::Read)),
    (
        "debug.host_popup.open",
        local_only(MethodEffect::Idempotent),
    ),
    (
        "debug.host_popup.close",
        local_only(MethodEffect::Idempotent),
    ),
    // modifier-hint 오버레이 홀드 주입/상태 덤프 — 사용자 modifier 홀드 재현. release 미노출.
    ("debug.modifier_hint.hold", local_only(MethodEffect::Mutate)),
    ("debug.modifier_hint.state", local_only(MethodEffect::Read)),
    // 설정 모달 강제 open — 사용자 조작 재현. release 미노출. 시각 검증 자동화용.
    ("debug.settings.open", local_only(MethodEffect::Idempotent)),
    // 설정 모달만 닫는다(저장하지 않음) — 사용자 창 닫기 재현. release 미노출.
    ("debug.settings.close", local_only(MethodEffect::Idempotent)),
    // 런타임 설정 patch 적용 — 사용자 "설정 저장" 재현. release 미노출.
    ("debug.settings.apply", local_only(MethodEffect::Idempotent)),
    // 활성 모달에 창 닫기 요청을 전달한다. release의 window.close는 메인 창만 대상으로 한다.
    (
        "debug.modal.close_request",
        local_only(MethodEffect::Idempotent),
    ),
    // 보조 창까지 포함한 창 목록. release window.list 는 메인 창만 다룬다(docs/ai-verification/screenshot-methods.md).
    ("debug.window.list", local_only(MethodEffect::Read)),
    // 범위·대상을 지정한 토스트 발화 — 토스트는 사용자 행동의 결과라 release 미노출. 위치 실측용.
    (
        "debug.toast",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    // 원격 파일 "다운로드 후 실행" 시작·1 MiB 초과 확인·취소 — 사용자 조작 재현. 버튼 UI 는 디자인 대기. release 미노출.
    (
        "debug.remote_download.start",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    (
        "debug.remote_download.confirm",
        local_only(MethodEffect::Idempotent),
    ),
    (
        "debug.remote_download.cancel",
        local_only(MethodEffect::Idempotent),
    ),
    // 배너 직접 발화/조회/닫기/카운트다운 — 사용자 조작 재현. release 미노출.
    ("debug.banner.list", local_only(MethodEffect::Read)),
    ("debug.banner.show", local_only(MethodEffect::Mutate)),
    ("debug.banner.close", local_only(MethodEffect::Idempotent)),
    (
        "debug.banner.set_countdown",
        local_only(MethodEffect::Idempotent),
    ),
    (
        "debug.plugin_banner.open",
        local_only(MethodEffect::Idempotent),
    ),
    (
        "debug.plugin_banner.close",
        local_only(MethodEffect::Idempotent),
    ),
    // html 문서의 사용자 탐색(주소 입력·뒤로·앞으로·재로드·중지) 재현. release 미노출.
    ("debug.webview.history", local_only(MethodEffect::Mutate)),
    // native WebView 오버레이 모드 조회와 backend 지원 모드 강제. release 미노출.
    ("debug.webview.overlay", local_only(MethodEffect::Mutate)),
    (
        "debug.event_bus.list_subscribers",
        local_only(MethodEffect::Read),
    ),
    (
        "debug.event_bus.publish",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    ("debug.event_bus.trace", local_only(MethodEffect::Read)),
    (
        "debug.extension.invoke_hook",
        local_only(MethodEffect::Mutate).kept_on_every_host_path(),
    ),
    // 전체화면 무대 강제 진입/종료/조회 — 사용자 조작(popup 타이틀바 전체화면 버튼)
    // 재현. release 미노출. 자기검증(무대 렌더 스크린샷)의 진입점.
    ("debug.fullscreen.list", local_only(MethodEffect::Read)),
    (
        "debug.fullscreen.open",
        local_only(MethodEffect::Idempotent),
    ),
    (
        "debug.fullscreen.close",
        local_only(MethodEffect::Idempotent),
    ),
    ("debug.fullscreen.state", local_only(MethodEffect::Read)),
    // 외부 포커스 조작은 debug 전용이다. view.focus는 window.focus의 호환 이름이다.
    ("window.focus", local_only(MethodEffect::Idempotent)),
    ("view.focus", local_only(MethodEffect::Idempotent)),
    // ── OS 전역 입력 상태 조작 (macOS) — 사용자 입력 재현 ──────────
    // OS 전역 키 주입·입력기 변경은 debug 및 --enable-input-simulation으로 제한한다.
    // release의 surface.send_key는 대상 ID가 있는 터미널 입력 경로다.
    (
        "surface.switch_input_source",
        local_only(MethodEffect::Idempotent),
    ),
    ("surface.raw_key", local_only(MethodEffect::Mutate)),
    // IME 계열은 아래 prefix 규칙에 같은 debug 제한을 적용한다.
];
#[cfg(not(debug_assertions))]
pub const DEBUG_METHODS: &[(&str, MethodMeta)] = &[];

/// prefix 기반 fallback. METHOD_TABLE에 없는 메서드를 prefix로 매칭한다.
/// - `surface.ime_*` — 창 IME 조합 상태를 강제로 세팅/조회하는 시뮬레이션
///   메서드. 사용자 입력기 조합의 재현이고 대상을 ID 로 받지 못한 채 포커스된
///   창에 작용하므로 [`DEBUG_METHODS`] 와 같은 `#[cfg(debug_assertions)]` 격리
///   대상이다 — release 에서는 이 규칙 자체가 사라져 빈 슬라이스가 된다.
///
/// plugin 이 매니페스트 `[[contributes.ipc_namespace]]` 로 점유한 prefix 는
/// host 가 [`install_namespace_table`] 로 건넨 표에 올라 `method_meta()` 의 마지막
/// fallback 단계에서 해소된다. 정적 `PREFIX_RULES` 는 host 자체 메서드의
/// prefix-fallback 전용.
#[cfg(debug_assertions)]
pub const PREFIX_RULES: &[(&str, MethodMeta)] = &[("surface.ime_", local_only(Idempotent))];
#[cfg(not(debug_assertions))]
pub const PREFIX_RULES: &[(&str, MethodMeta)] = &[];

/// 호스트와 같은 Arc로 공유하는 namespace 소유 표. 여기서는 읽기만 한다.
/// 설치 전에는 등록된 prefix가 없는 것으로 처리한다.
static NAMESPACE_TABLE: OnceLock<Arc<RwLock<IpcNamespaceRegistry>>> = OnceLock::new();

/// 부팅 때 한 번 설치한다. 이미 설치됐으면 표를 바꾸지 않고 false를 반환한다.
pub fn install_namespace_table(table: Arc<RwLock<IpcNamespaceRegistry>>) -> bool {
    NAMESPACE_TABLE.set(table).is_ok()
}

/// 시험은 한 번 설치한 전역 표를 공유하고 test_lock으로 변경을 직렬화한다.
#[cfg(test)]
pub(crate) fn test_namespace_table() -> &'static Arc<RwLock<IpcNamespaceRegistry>> {
    NAMESPACE_TABLE.get_or_init(|| Arc::new(RwLock::new(IpcNamespaceRegistry::new())))
}

/// 읽기 전용 임계구역의 poison을 보고하고 복구한다. 읽기 실패를 미등록으로 취급하지 않는다.
const NAMESPACES_WHAT: &str = "the plugin IPC namespace table";
static NAMESPACES_POISON_REPORTED: AtomicBool = AtomicBool::new(false);

/// prefix 등록 여부를 읽는다. 호출자가 결과를 반대로 써 차단할 수 있어
/// poison을 false로 바꾸지 않고 실제 표를 복구해 확인한다. 설치 전에는 false다.
pub fn is_registered_plugin_prefix(prefix: &str) -> bool {
    let Some(table) = NAMESPACE_TABLE.get() else {
        return false;
    };
    let guard = tasty_utils::poison::recover_read(
        table.read(),
        NAMESPACES_WHAT,
        &NAMESPACES_POISON_REPORTED,
    );
    guard.owns_prefix(prefix)
}

/// 특정 플러그인이 prefix를 소유하는지 확인한다. 자기 namespace 호출 면제와
/// 자식에게 namespace 권한을 부여하는 검사가 사용한다.
pub fn plugin_owns_prefix(plugin_id: &str, prefix: &str) -> bool {
    let Some(table) = NAMESPACE_TABLE.get() else {
        return false;
    };
    let guard = tasty_utils::poison::recover_read(
        table.read(),
        NAMESPACES_WHAT,
        &NAMESPACES_POISON_REPORTED,
    );
    guard.prefixes_of(plugin_id).iter().any(|p| p == prefix)
}

/// 정적 호스트 표에 정확히 등재된 이름인지 확인한다. prefix fallback은 제외한다.
/// 이를 섞으면 플러그인 고유 이름과 그 아래 오타까지 호스트가 처리할 이름으로 오인한다.
pub fn is_registered_name(method: &str) -> bool {
    METHOD_TABLE.iter().any(|(name, _)| *name == method)
        || DEBUG_METHODS.iter().any(|(name, _)| *name == method)
}

/// 0.7.0 시점에 동결된 메서드 이름 목록. `METHOD_TABLE` 의 스냅샷이고 의도한 제거가
/// 있을 때만 이름이 빠진다 — 갱신 절차는 파일 머리 주석과 `docs/dev-guide/api-conventions.md`.
const FROZEN_BASELINE_0_7: &str = include_str!("../fixtures/method_baseline_0_7.txt");

/// 0.7.0 기준 목록에 포함됐는지 구분한다. 이후 추가된 이름의 개별 도입 버전은 기록하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodSince {
    /// 0.7.0 동결 baseline 에 있던 이름. 그 버전 이상이면 어느 서버에나 있다.
    FrozenBaseline,
    /// 0.7.0 이후에 더해진 이름. 구 서버에는 없을 수 있다.
    AfterFrozenBaseline,
}

/// 동결 목록을 이름 집합으로 한 번만 푼다.
fn frozen_baseline_names() -> &'static std::collections::HashSet<&'static str> {
    static NAMES: OnceLock<std::collections::HashSet<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        FROZEN_BASELINE_0_7
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    })
}

/// 정적 등록 여부와 동결 목록으로 분류한다. 미등록 이름은 None이다.
/// debug 이름은 release 표에서 빠지므로 release에서는 None이 된다.
pub fn method_since(method: &str) -> Option<MethodSince> {
    if !is_registered_name(method) {
        return None;
    }
    if frozen_baseline_names().contains(method) {
        Some(MethodSince::FrozenBaseline)
    } else {
        Some(MethodSince::AfterFrozenBaseline)
    }
}

/// 알려진 메서드의 메타. 미등록 메서드는 `None`.
pub fn method_meta(method: &str) -> Option<MethodMeta> {
    for (name, meta) in METHOD_TABLE {
        if *name == method {
            return Some(*meta);
        }
    }
    for (name, meta) in DEBUG_METHODS {
        if *name == method {
            return Some(*meta);
        }
    }
    for (prefix, meta) in PREFIX_RULES {
        if method.starts_with(prefix) {
            return Some(*meta);
        }
    }
    if let Some(dot) = method.find('.')
        && is_registered_plugin_prefix(&method[..dot])
    {
        // 등록 prefix 아래 임의 이름은 ipc.invoke:<prefix>를 호출 게이트에서 확인한다.
        return Some(MethodMeta {
            plugin_callable: true,
            plugin_only: false,
            required: &[],
            namespace_forward: true,
            // 플러그인 메서드의 부수효과는 알 수 없어 Mutate로 분류한다.
            effect: MethodEffect::Mutate,
            // forward 는 engine 라우터의 보존소 앞에서 plugin 으로 나간다 — 키를 실어도
            // 재생되지 않는다(docs/dev-guide/api-conventions.md#어느-경로에-걸리나--호스트가-아는-이름은-전부-안-plugin-고유-이름만-밖).
            key_contract: KeyContract::Outside,
        });
    }
    None
}

/// 이 프로세스의 정적 표가 선언한 키 계약. 모르는 이름은 Outside다.
/// 호스트의 런타임 namespace 표 유무와 관계없이 플러그인 고유 이름은 Outside다.
/// 서버가 더 새 메서드를 지원해도 옛 client는 전송 전에 거절할 수 있다.
pub fn key_contract(method: &str) -> KeyContract {
    method_meta(method).map_or(KeyContract::Outside, |m| m.key_contract)
}

/// Single declaration used by the host admission adapter and its advertised durable key scope.
pub fn has_structure_journal_contract(method: &str) -> bool {
    matches!(
        key_contract(method),
        KeyContract::Kept {
            since: KEY_KEPT_IN_STRUCTURE_JOURNAL
        }
    )
}

#[cfg(test)]
#[path = "method_meta_tests.rs"]
mod tests;
