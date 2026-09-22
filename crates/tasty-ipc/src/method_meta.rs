//! IPC 메서드별 메타데이터 — plugin이 호출 가능한지, 어떤 권한이 필요한지.
//!
//! 이 테이블이 **단일 진실 원천**이다. 새 IPC 메서드를 추가할 때 반드시
//! 여기에도 등록한다. 매핑되지 않은 메서드는 [`method_meta`]가 `None`을 반환하며,
//! `CallerContext::ensure_allowed`가 plugin 호출을 거부한다.
//!
//! Local caller(CLI/사용자)는 권한 검사를 거치지 않는다. 이 테이블은 **plugin이
//! 호출했을 때**의 권한 요구사항이다.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock, RwLock};

use tasty_plugin_manifest::Permission;

use crate::ipc_namespace::IpcNamespaceRegistry;
// `PREFIX_RULES` 한 줄을 100 칸 안에 두려고 이름을 짧게 든다. 그 정의는
// `ipc_release_table_excludes_input_reproduction` 이 **줄 단위로** 읽으므로, 줄이
// 접히면 규칙이 사라진 것과 구별이 안 돼 빨개진다.
//
// cfg 가 붙은 이유: 이 이름을 쓰는 자리가 debug 쪽 `PREFIX_RULES` 하나뿐이라,
// release 에서는 아무 데도 안 쓰여 `unused_import` 가 뜬다. 그 조합을 보는 잡이
// `check-release` 이고 debug 빌드에서는 조용하다.
#[cfg(debug_assertions)]
use self::MethodEffect::Idempotent;

/// 이 메서드를 **두 번 전달하면 관측 가능한 차이가 남는가**.
///
/// 축이 "읽기인가" 가 아니라 **재전달**인 이유는 이 값을 쓰는 쪽이 그것만 묻기 때문이다 —
/// 응답을 못 받은 호출자가 다시 보내도 되는지. 그래서 파일을 쓰는 조회
/// (`ui.screenshot`)는 이름이 읽기 계열이어도 [`MethodEffect::Mutate`] 다. 두 번째 호출이
/// 두 번째 파일을 남긴다.
///
/// 판정 불가를 뜻하는 값은 없다 — 생성자가 이 값을 **요구**하므로 새 메서드는 분류 없이는
/// 표에 못 들어간다. 별도 목록으로 뒀다면 그 목록과 표가 갈릴 수 있었다.
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

/// 멱등 키([`crate::protocol::JsonRpcRequest::idempotency_key`])를 실으면 이 메서드에서
/// **무엇이 일어나는가** — 호출자가 보내기 **전에** 읽는 선언.
///
/// 이 값이 답하는 물음은 하나다: *같은 키로 다시 보내면 두 번째 효과가 나는가.* 호스트의
/// 보존소가 "이 메서드에 걸리는가" 도 같은 물음이라 칸을 따로 두지 않는다 — 걸리면
/// [`KeyContract::Kept`], 원래 걸 필요가 없으면 [`KeyContract::Unneeded`], 걸리지 않으면
/// [`KeyContract::Outside`] 다. [`MethodEffect`] 와 다른 물음인 것은 층 때문이다: 같은
/// `Mutate` 라도 보존소를 지나는 이름(호스트가 아는 이름 — 어느 층이 끝내든)이 있고 안 지나는
/// 이름(plugin 이 뜻을 정하는 namespace 고유 이름)이 있으며, 지나는 이름도 층마다 **받기 시작한
/// 판**이 다르다.
///
/// 값은 대부분 [`MethodEffect`] 에서 **유도**된다(`Mutate` → `Kept`, 나머지 → `Unneeded`).
/// 손으로 적는 것은 판이 다른 이름뿐이다 — App 층이 끝내는 `Mutate` 는
/// [`KEY_KEPT_BY_APP_LAYER`], 그 뒤의 두 경로(GUI debug step · plugin namespace forward 로 나가는
/// 표 이름)가 끝내는 `Mutate` 는 [`KEY_KEPT_ON_EVERY_HOST_PATH`] 다. 그 둘이 실제 배선과
/// 맞는지는 본체의 source guard(`key_contract_by_layer`)가 dispatch 본문을 읽어 양방향으로 잰다.
///
/// `Outside` 는 표가 쓰지 않는다 — 표가 모르는 이름(plugin 고유 이름)에만 나온다.
///
/// 근거·대안은 [ADR-0423] · [ADR-0566], plugin namespace forward 의 `Outside` 는 [ADR-0361].
///
/// [ADR-0361]: ../../../docs/adr/0361-a-plugin-namespace-forward-is-declared-outside-the-idempotency-contract.md
/// [ADR-0423]: ../../../docs/adr/0423-each-method-declares-its-key-contract-and-the-version-that-keeps-it.md
/// [ADR-0566]: ../../../docs/adr/0566-every-host-path-keeps-the-idempotency-key-and-only-a-plugin-name-is-outside.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContract {
    /// **호스트 보존소가 키를 받는다.** 같은 키·같은 요청의 재시도는 실행되지 않고 보관된 답을
    /// `idempotent_replay: true` 로 받는다(선언된 보존 범위 안에서). 그래서 성공 응답에 표지가
    /// 없으면 그 답은 **이번에 실행한 것**이다.
    ///
    /// `since` 는 이 보장을 선언하는 `ipc.idempotency-key` 의 판이다. 판 1 서버는 engine
    /// 라우터에서만 키를 받았으므로, App 층 메서드에 키를 실으려면 상대가 판 2 이상이어야 한다
    /// — 판이 낮은 서버에서는 키가 조용히 무시되고 재시도가 두 번째 실행이 된다.
    Kept {
        /// 요구하는 `ipc.idempotency-key` 의 최소 판.
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

/// engine 라우터의 보존소가 키를 받기 시작한 `ipc.idempotency-key` 판.
pub const KEY_KEPT_BY_ROUTER: u32 = 1;
/// App 층도 키를 받기 시작한 판.
pub const KEY_KEPT_BY_APP_LAYER: u32 = 2;
/// App 층 **뒤**의 두 경로도 키를 받기 시작한 판 — GUI debug step, 그리고 plugin namespace
/// forward 로 나가는 **표의** 이름(prefix 를 번들 plugin 이 점유한 `image.open` 등). 판 2
/// 서버는 그 둘에서 키를 무시했다. 서버가 선언하는 판이 이것이다(`crate::capability`).
pub const KEY_KEPT_ON_EVERY_HOST_PATH: u32 = 3;

/// [`MethodEffect`] 에서 유도한 기본 선언. 판이 다른 이름만 [`MethodMeta::kept_by_app_layer`] ·
/// [`MethodMeta::kept_on_every_host_path`] 로 고친다.
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
    /// plugin이 이 메서드를 호출할 수 있는지. false면 plugin은 어떤 경우에도 호출 불가.
    pub plugin_callable: bool,
    /// **plugin 만** 부를 수 있는지 — 즉 이 이름에 외부(CLI/네트워크 IPC) dispatch
    /// arm 이 없다. plugin host-call 진입부가 직접 인터셉트하는 메서드들이다.
    ///
    /// 이 표는 원래 caller **게이트**만 담았고 라우팅은 담지 않았다. 그런데 게이트의
    /// 한쪽 방향은 이미 말할 수 있었다 — `local_only()` 가 "plugin 은 못 부른다" 다.
    /// 반대 방향을 말할 수단이 없어서, plugin 전용 메서드가 `plugin(&[…])` 로 적히고
    /// 외부 호출자는 `-32601`("그런 메서드 없다")을 받았다. 이름은 맞고 표에도 있는데
    /// 없다고 답한 것이라, 플랫폼 축에서 같은 거짓을 고친 ADR-0154 와 같은 형태다(ADR-0163).
    pub plugin_only: bool,
    /// plugin이 호출하려면 매니페스트에 이 권한들이 모두 선언돼 있어야 함.
    pub required: &'static [Permission],
    /// 이 이름이 **표가 아니라 plugin 이 점유한 prefix 로** 해소됐는가.
    ///
    /// 참이면 요구 권한이 정적 `required` 로 적히지 않는다 — 필요한 것은
    /// `ipc.invoke:<prefix>` 이고 그 prefix 는 이름에서만 나온다. 그래서 게이트
    /// (`CallerContext::ensure_allowed`)가 이 표시를 보고 그 토큰을 직접 요구한다.
    /// 이 칸이 없던 때에는 이 갈래가 `required: []` 로만 답해, 권한을 하나도 안 가진
    /// agent 토큰이 설치된 plugin 의 namespace 전체를 부를 수 있었다
    /// ([ADR-0271](../../../docs/adr/0271-a-plugin-namespace-is-invoked-with-its-token-from-every-gated-caller.md)).
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

/// plugin 만 부를 수 있는 메서드 — 외부 dispatch arm 이 없다.
///
/// `local_only()` 의 거울이다. 표에 등재하는 이유도 같다: 거부가 **정책인지 누락인지**
/// 구분되게 하려고. 다른 것은 그 거부를 누가 받느냐뿐이다.
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
    /// App 층이 끝내는 `Mutate` — 보존소가 받지만 판 [`KEY_KEPT_BY_APP_LAYER`] 부터다.
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

    /// App 층 뒤의 경로가 끝내는 `Mutate` — GUI debug step 과, plugin namespace forward 로 나가는
    /// 표 이름. 보존소가 받지만 판 [`KEY_KEPT_ON_EVERY_HOST_PATH`] 부터다.
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

/// 등록된 IPC 메서드 — 단일 진실 원천. lint/검증 테스트가 이 테이블 위에서
/// 동작한다. 새 메서드는 여기에 추가한다.
///
/// prefix-기반 fallback(`surface.ime_*` 등)은 [`PREFIX_RULES`] 참조.
pub const METHOD_TABLE: &[(&str, MethodMeta)] = {
    use MethodEffect::*;
    use Permission::*;
    &[
        // ── 호스트 system ─────────────────────────────────────────────
        ("system.info", plugin(Read, &[])),
        // GPU 리소스 카운트 read-only 스냅샷 (메모리 누수 soak 검증). 순수 조회지만
        // 내부 렌더러 구조를 노출하는 진단 표면이라 local_only — plugin 미노출.
        ("system.gpu_stats", local_only(Read)),
        // 프로세스 압력 게이지(큐 대기·깊이·handler 실행 시간)의 누계 조회.
        //
        // `local_only` 인 이유는 `system.gpu_stats` 와 같다 — 순수 조회지만 **호스트
        // 내부의 스케줄링 상태**를 노출하는 진단 표면이다. 그리고 여기 값은
        // ADR-0305 대로 caller 로 나누지 않는 프로세스 게이지라, plugin 에게 주면
        // 자기 몫이 아닌 다른 caller 의 부하까지 읽는 것이 된다. plugin 이 자기
        // 사용량을 보는 축은 `telemetry.*`(caller 별)이고 그쪽은 그대로 열려 있다.
        ("system.pressure", local_only(Read)),
        // ── plugin 보조 채널 ──────────────────────────────────────────
        // egui-mesh 프레임용 공유 메모리 생성. main 채널 + 보조 채널(fd/HANDLE 송신)을
        // 함께 다뤄야 해서 라우터가 아니라 plugin 진입부가 가로채 처리하지만, **등재는
        // 여기 있어야 한다** — 표에 없으면 표를 읽는 어떤 감사도 이 메서드를 보지 못하고,
        // 게이트도 이름을 못 찾아 태울 수 없다.
        //
        // 지금 요구하는 토큰이 없는 것은 **결정이 아니라 미결**이다. 어떤 권한을
        // 요구할지(그리고 개수·총량 상한을 함께 둘지)는 매니페스트 호환성이 걸린
        // 별도 결정이고, ADR-0152 의 "이 ADR 이 안 정한 것" 에 열린 질문으로 있다.
        // 그때까지는 현재 동작 그대로 등재해 최소한 cap·rate·audit 는 걸리게 한다.
        ("host.shared_buffer.create", plugin_only(Mutate, &[])),
        // ── 타이머 관측 ───────────────────────────────────────────────
        // 등록된 주기 작업의 read-only 스냅샷("지금 무엇이 이 인스턴스를 깨우는가").
        // local_only — plugin 이 호스트 내부 스케줄을 알아야 할 이유가 없고, 조회
        // 전용이라 등록/취소 경로 자체가 없다. 진단·회귀검증 목적이므로 새 권한
        // 토큰을 신설해 기존 plugin 재승인을 유발할 값어치도 없다.
        ("timer.list", local_only(Read)),
        // ── workspace (read/write) ────────────────────────────────────
        ("workspace.list", plugin(Read, &[SurfaceRead])),
        ("workspace.create", plugin(Mutate, &[SurfaceWrite])),
        ("workspace.update", plugin(Idempotent, &[SurfaceWrite])),
        ("workspace.move", plugin(Idempotent, &[SurfaceWrite])),
        // workspace.create 와 대칭 — 만들 수 있는 주체는 닫을 수도 있어야 한다(원칙 2).
        // 새 권한 토큰을 만들지 않는 이유: pane.close / tab.close 도 같은 SurfaceWrite
        // 이고, 닫기만 따로 승인받게 하면 기존 plugin 이 전부 재승인 대상이 된다.
        ("workspace.close", plugin(Idempotent, &[SurfaceWrite])),
        // ── workspace category (사이드바 폴더 CRUD) ──────────────────
        ("workspace_category.list", plugin(Read, &[SurfaceRead])),
        ("workspace_category.create", plugin(Mutate, &[SurfaceWrite])),
        (
            "workspace_category.rename",
            plugin(Idempotent, &[SurfaceWrite]),
        ),
        (
            "workspace_category.delete",
            plugin(Idempotent, &[SurfaceWrite]),
        ),
        (
            "workspace_category.move",
            plugin(Idempotent, &[SurfaceWrite]),
        ),
        // ── pane / split ──────────────────────────────────────────────
        ("pane.list", plugin(Read, &[SurfaceRead])),
        ("pane.close", plugin(Idempotent, &[SurfaceWrite])),
        ("split", plugin(Mutate, &[SurfaceWrite])),
        // ── tab ───────────────────────────────────────────────────────
        ("tab.list", plugin(Read, &[SurfaceRead])),
        ("tab.create", plugin(Mutate, &[SurfaceWrite])),
        ("tab.close", plugin(Idempotent, &[SurfaceWrite])),
        ("tab.move", plugin(Idempotent, &[SurfaceWrite])),
        // ── preset (layout preset CRUD + apply) ───────────────────────
        ("preset.list", plugin(Read, &[SurfaceRead])),
        ("preset.get", plugin(Read, &[SurfaceRead])),
        ("preset.save", plugin(Idempotent, &[SurfaceWrite])),
        ("preset.delete", plugin(Idempotent, &[SurfaceWrite])),
        ("preset.rename", plugin(Idempotent, &[SurfaceWrite])),
        ("preset.capture", plugin(Mutate, &[SurfaceWrite])),
        ("preset.apply", plugin(Idempotent, &[SurfaceWrite])),
        // ── surface (구조 조작) ───────────────────────────────────────
        ("surface.list", plugin(Read, &[SurfaceRead])),
        // 등록된 surface kind 조회. 만드는 것이 유일한 확인이던 자리를 읽기 전용으로
        // 대체한다 — plugin 도 자기 kind 가 실제로 등록됐는지 물을 수 있어야 해서
        // `local_only` 가 아니다. 노출값은 kind 이름·i18n 키·렌더 경로·출처뿐이라
        // `SurfaceRead` 보다 넓은 권한을 요구하지 않는다.
        ("surface.kinds", plugin(Read, &[SurfaceRead])),
        ("surface.close", plugin(Idempotent, &[SurfaceWrite])),
        ("surface.close_self", plugin(Idempotent, &[SurfaceWrite])),
        // tree/meta는 read 권한
        ("tree", plugin(Read, &[SurfaceRead])),
        // webview — plugin 이 webview-enabled surface 의 URL 설정. SurfaceWrite 권한.
        ("webview.set_url", plugin(Idempotent, &[SurfaceWrite])),
        // theme.query — 현재 resolved 전역 Theme 스냅샷 조회. webview-kind surface(예:
        // markdown)는 `set_context` 를 받지 않아 Theme 이 자동 push 되지 않으므로, 문서를
        // (재)생성할 때마다 이 read-only 조회로 대신한다(ADR-0065). surface 별 데이터가
        // 아닌 전역 정보라 별도 권한 없이 노출(`system.info` 와 같은 근거).
        ("theme.query", plugin(Read, &[])),
        // surface.set_cwd — plugin 이 자기 RemoteSurface 의 cwd 를 host 에 통보.
        // 예: explorer 가 root 변경 시 carry 후보 cwd 갱신.
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
        ("surface.wake", plugin(Mutate, &[TerminalSpawn])),
        ("surface.set_mark", plugin(Mutate, &[TerminalRead])),
        // completion 은 read 가 아니라 attention(주의 환기) 발동 — PushNotification
        // 계열이므로 notification.* 와 동일한 Notification 권한.
        ("surface.completion", plugin(Mutate, &[Notification])),
        // attention 조회/해제 — completion 의 역방향(해제)과 그 관측 표면.
        // raise 와 같은 상태를 읽고 되돌리는 것이라 같은 `Notification` 버킷에 둔다:
        // 발동 권한만 주고 해제 권한을 빼면 자기가 켠 신호를 못 끄는 비대칭이 된다.
        ("surface.attention.get", plugin(Read, &[Notification])),
        (
            "surface.attention.clear",
            plugin(Idempotent, &[Notification]),
        ),
        ("surface.read_since_mark", plugin(Read, &[TerminalRead])),
        // 출력 스캐너 전용 커서 — 읽으면 커서가 전진한다. 같은 출력을 읽으므로
        // read_since_mark 과 같은 권한 버킷이고, CLI 진입점은 없다(파괴적 읽기라
        // 사용자가 한 줄로 스캐너의 바이트를 가로챌 수 있게 열지 않는다 —
        // ADR-0307).
        //
        // effect 가 위 read_since_mark(`Read`)와 갈리는 이유: 이 읽기는 커서를
        // 전진시켜 **읽은 구간을 소비한다.** 재전달은 그때까지 새로 온 두 번째
        // 구간을 먹고, 첫 응답이 나른 바이트는 어디에서도 다시 못 읽는다. 응답을
        // 못 받은 호출자가 다시 보내면 안 되는 형태라 `message.read`(peek 기본값
        // false)와 같은 `Mutate` 다 — ADR-0306 의 축이 "읽기인가" 가 아니라
        // "두 번 전달하면 관측 가능한 차이가 남는가" 인 것이 여기서 갈린다.
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
        ("surface.respawn_terminal", plugin(Mutate, &[TerminalSpawn])),
        ("surface.is_typing", plugin(Read, &[TerminalRead])),
        // ── child-terminal 관리 (ADR-0040 / occupancy-04) ─────────────
        // 호스트가 내재화한 자식 터미널 registry. codex/claude plugin 이
        // 자체 registry 를 걷어내고 이 method 들로 위임한다. 권한은 각 method 가
        // 내부에서 조합하는 sibling 핸들러(tab.create=SurfaceWrite, surface.send=
        // TerminalWrite, surface.respawn_terminal=TerminalSpawn, surface.close=
        // SurfaceWrite)의 요구를 합집합으로 반영한다.
        (
            "terminal.spawn",
            plugin(Mutate, &[SurfaceWrite, TerminalWrite, TerminalSpawn]),
        ),
        ("terminal.tell", plugin(Mutate, &[TerminalWrite])),
        ("terminal.children", plugin(Read, &[SurfaceRead])),
        ("terminal.parent", plugin(Read, &[SurfaceRead])),
        // 자식 단건 상태 조회. children/parent 와 동일하게 순수 조회라
        // SurfaceRead 단독.
        ("terminal.state", plugin(Read, &[SurfaceRead])),
        ("terminal.kill", plugin(Idempotent, &[SurfaceWrite])),
        (
            "terminal.respawn",
            plugin(Mutate, &[TerminalWrite, TerminalSpawn]),
        ),
        ("terminal.broadcast", plugin(Mutate, &[TerminalWrite])),
        // hook 이 idle/needs_input 신호를 호스트 registry 에 주입. 자식 상태 write.
        ("terminal.set_state", plugin(Idempotent, &[SurfaceWrite])),
        // 임의의 기존 surface 를 명시적으로 child 로 등록(soft 점유) —
        // `docs/features/child-terminal/index.md`("adopt" 절). sibling IPC 핸들러를
        // 호출하지 않고 순수 in-process core 함수(register_child/occupy_soft)만
        // 쓰므로 "child 관계 write" 성격의 SurfaceWrite 단독으로 충분
        // (terminal.kill/terminal.set_state 와 동일 컨벤션).
        ("terminal.adopt", plugin(Idempotent, &[SurfaceWrite])),
        // child 관계·soft 점유만 해제하고 surface 는 닫지 않음 —
        // `docs/features/child-terminal/index.md`("release" 절). adopt 와 대칭으로
        // 순수 in-process core 함수(remove_child/release_soft_occupancy)만 쓰므로
        // SurfaceWrite 단독.
        ("terminal.release", plugin(Idempotent, &[SurfaceWrite])),
        // ── headless PTY primitive (docs/adr/0050-headless-pty-primitive.md /
        // pty_registry) ────────────────────────────────────────────────
        // Surface 가 없는 백그라운드 PTY. child-terminal 과 달리 Surface 트리를
        // 전혀 건드리지 않으므로 Surface* 토큰이 섞이지 않는다 — 기존 Terminal* 3종만
        // 사용한다(위 ADR Decision 참고). spawn 은 Surface 를 안 만들어 SurfaceWrite
        // 불필요, wait 는 라이브 트리 대신 PtyEntry exit cell 로 판정해 SurfaceRead
        // 불필요, kill 은 Surface 를 닫지 않고 프로세스만 종료해 SurfaceWrite 불필요.
        ("pty.spawn", plugin(Mutate, &[TerminalSpawn])),
        ("pty.write", plugin(Mutate, &[TerminalWrite])),
        ("pty.read", plugin(Read, &[TerminalRead])),
        ("pty.wait", plugin(Read, &[TerminalRead])),
        ("pty.kill", plugin(Idempotent, &[TerminalWrite])),
        ("pty.list", plugin(Read, &[TerminalRead])),
        // 승격 경로: headless PTY 를 실제 Surface 로 만든다 — spawn/write/... 와
        // 달리 Surface 트리를 새로 만들므로 terminal.spawn 과 동일하게 SurfaceWrite 를
        // 더한다(TerminalSpawn 은 새 터미널 surface 생성 권한).
        (
            "pty.attach_surface",
            plugin(Mutate, &[SurfaceWrite, TerminalSpawn]),
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
        // register 만 plugin 호출 가능(Network 권한, S11). plugin 은 인라인
        // sequence 를 못 쓰고 자기 소유 hook 핸들러 id 만 바인딩할 수 있다(핸들러
        // 측 caller 게이트). 나머지 조회/해제/설정은 local_only(CLI/로컬 client).
        // register/unregister/sweep 는 웹훅 lifecycle 의미가 create/remove/clear
        // 보다 명확 — api-conventions "verb 화이트리스트" 정당화. sweep = 만료 정리.
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
        // com.tasty.image plugin이 namespace를 점유하지만, 호스트 어댑터는
        // plugin 비활성 상태에서도 동작한다. plugin은 ipc.invoke:image 권한으로
        // 위 메서드들을 호출한다.
        //
        // plugin 이 켜져 있으면 외부 호출은 namespace forward 로 plugin 에 먼저 가고
        // plugin 이 호스트로 되부른다 — 그래서 `Mutate` 는 forward 경로의 판이다.
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
        // 세션 요약 — workspace 별 markdown 텍스트. memory.* 와 분리된 표면.
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
        //
        // 읽으면서 서버 쪽 커서가 전진하지 **않는다** — 커서는 소비자가 들고 매번
        // 가져온다. 그래서 같은 인자로 두 번 불러도 같은 답이 오고, 두 번째 전달이
        // 남기는 관측 가능한 차이가 없다(ADR-0306 의 축). 이 점이
        // `surface.read_since_scan_mark` 와 갈리는 자리다 — 그쪽은 읽으면서 서버
        // 커서를 밀어 같은 구간을 다시 못 읽는다.
        //
        // `local_only` 인 이유는 권한이 아니라 **모양**이다. plugin 은 버스 구독으로
        // 이미 push 를 받고, `wait_ms` 대기는 plugin SDK 의 단일 워커를 막는다
        // (`agent.task_await`·`approval.await` 가 같은 이유로 local 전용이다).
        ("events.fetch", local_only(Read)),
        // ── agent (협업 primitive) ────────────────────────────────────
        ("agent.task_create", plugin(Mutate, &[AgentManage])),
        ("agent.task_list", plugin(Read, &[AgentManage])),
        ("agent.task_get", plugin(Read, &[AgentManage])),
        // approval.await(:265)와 대칭 — 진짜 blocking(worker thread 위임) 이라 plugin
        // 이 호출하면 단일 워커 스레드가 막혀 다른 host→plugin 요청을 못 받는다.
        // plugin 은 완료 판정 전략 선언(러너가 대신 기다림) 또는 task_get 폴링으로
        // 우회한다 — docs/dev-guide/agent-runner.md "완료 판정 전략 레지스트리".
        ("agent.task_await", local_only(Read)),
        ("agent.task_cancel", plugin(Idempotent, &[AgentManage])),
        ("agent.task_retry", plugin(Mutate, &[AgentManage])),
        ("agent.task_graph", plugin(Read, &[AgentManage])),
        // DAG 그룹 조회 — task_graph 와 같은 읽기 표면이라 같은 권한.
        ("agent.dag_list", plugin(Read, &[AgentManage])),
        ("agent.dag_get", plugin(Read, &[AgentManage])),
        // 외부 task(Custom kind) 의 완료 신호 — 러너가 그 생명주기를 단독으로
        // 소유한다. plugin 이 같은 task 를 별도로 전이시키면 쓰기 주체가 둘이 되어
        // 러너의 완료 판정과 경합하고, 결과가 어느 쪽 것인지 추적할 수 없게 된다.
        // plugin 은 완료 판정 전략 선언(러너가 그 전략으로 판정)으로 우회한다 —
        // docs/dev-guide/agent-runner.md "완료 판정 전략 레지스트리".
        ("agent.task_set_result", local_only(Idempotent)),
        // 자동 시작이 없으므로(재시작 정화는 부팅 경로 전용) plugin 이 자기
        // workspace 의 runner 를 스스로 되살릴 수단이 필요하다 — start/stop 은
        // idempotent, status 는 순수 조회.
        ("agent.task_run", plugin(Mutate, &[AgentManage])),
        // 참조 검사(cascade/force) + 상태 제약(Running 거부)을 지키는
        // 단건/일괄 삭제.
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
        // issue/revoke 는 plugin 도 호출 가능 (claude plugin 등이 자식에게
        // 토큰을 발급해야 하므로). list 는 host 전용 — 감사/디버깅 목적이라
        // plugin 노출 불필요.
        ("session.issue", plugin(Mutate, &[AgentManage])),
        ("session.revoke", plugin(Idempotent, &[AgentManage])),
        ("session.list", local_only(Read)),
        // ── attach.* (배타 attach 점유 제어 — attach/detach 단계 3·4) ──────
        // surface 단위 배타 점유 lock 제어. acquire/release 는 주로 stream 핸드셰이크
        // (stream.open{target})로 일어나 method_meta 게이트를 거치지 않는다.
        // force_detach/list 는 JSON-RPC 요청-응답 경로라 여기 등록이 필요하다.
        //
        // 권한: decision 5 — attach 보안은 **연결 경계(SSH + 127.0.0.1 loopback)**에
        // 위임한다. 자체 권한 레이어를 두지 않으므로 추가 Permission 을 요구하지
        // 않는다(`plugin(&[])`). Local(별도 인스턴스 client)·인증된 agent 모두
        // 소켓에 도달했다면 attach 제어를 호출할 수 있다.
        ("attach.acquire", plugin(Idempotent, &[])),
        ("attach.release", plugin(Idempotent, &[])),
        ("attach.force_detach", plugin(Idempotent, &[])),
        ("attach.force_detach_workspace", plugin(Idempotent, &[])),
        ("attach.into_gui", plugin(Mutate, &[])),
        ("attach.list", plugin(Read, &[])),
        // ── remote.profile.* (원격 접속 프로필 CRUD) ─────────────────────
        // 프로필은 비밀 없는 장비 인벤토리(passkey 를 이름으로 참조만). attach.* 와 동일하게
        // 연결 경계(소켓 도달)에 신뢰를 위임 — 추가 Permission 불요.
        // (구 tool.ssh.* / ssh.profile.* 는 alias.rs 로 한시 호환.)
        ("remote.profile.list", plugin(Read, &[])),
        ("remote.profile.get", plugin(Read, &[])),
        ("remote.profile.add", plugin(Idempotent, &[])),
        ("remote.profile.detect", plugin(Read, &[])),
        ("remote.profile.remove", plugin(Idempotent, &[])),
        // 로컬 ssh config 열거/가져오기. 읽는 파일이 `~/.ssh/config` 로 넓어지지만
        // 노출하는 것은 alias 이름과 표시용 hint 뿐이고(키·비밀 없음), 같은 OS 유저의
        // FS read 는 신뢰모델 범위 밖이라 프로필 CRUD 와 같은 메타를 쓴다.
        ("remote.profile.list_local", plugin(Read, &[])),
        ("remote.profile.import", plugin(Mutate, &[])),
        // ── remote.workspaces (원격 ws 브라우징) ──────────────────────────
        // profile CRUD 와 같은 `tasty_remote` 코어를 공유하는 release 경로다
        // (`app/ipc/app_methods.rs`, 원칙 2: 에이전트가 CLI 없이 소켓으로도 수행 가능).
        // 조회(browse)라 remote.profile.* 와 같은 신뢰경계 — 연결 경계(소켓 도달 + SSH)에
        // 위임하고 추가 Permission 을 두지 않는다. CLI `remote workspaces` 와 대칭.
        // 주의(호출자별로 의미가 다르다): 표는 Local 호출자에게 게이트가 아니다
        // (`caller.rs` Local => Ok). Local(세션 토큰 없는 tasty CLI·로컬 스크립트)은 라우터
        // 팔만 있으면 표와 무관하게 이미 도달 가능했다 — 이쪽은 재등재. 반면 plugin/agent
        // 세션 토큰 호출자는 표에 없으면 UnknownMethod 로 거부됐고, 이 항목이 그들에게
        // 처음 연다 — 이쪽은 **release 표면 확장**이다.
        ("remote.workspaces", plugin(Read, &[])),
        // remote.attach 는 조회가 아니라 로컬에 mirror 워크스페이스를 만드는 구조 op 라
        // 사용자 상태(불가침 원칙 1)에 닿는다. SSH 신뢰경계는 원격 셸 접근만 주고 그
        // 사용자의 로컬 tasty 창에 워크스페이스를 만들 권한은 주지 않으므로, 다른
        // remote.* 조회와 달리 연결경계 위임만으로 plugin 에 열 근거가 서지 않는다 —
        // local caller 전용으로 등재한다(CLI `tool attach` 는 그대로 동작). 위 조회는 열고
        // 이건 안 여는 비대칭은 의도된 것이다("일관성 정리" 로 지우지 말 것). 근거·재검토
        // 트리거는 ADR-0121(docs/adr/0121-attach-trust-boundary-covers-remote-queries-not-local-structural-ops.md).
        ("remote.attach", local_only(Mutate).kept_by_app_layer()),
        // ── remote.passkey.* (자격증명 CRUD) ─────────────────────────────
        // 값 마스킹은 핸들러가 보장(list/get 은 name+kind 만, 파일 내용 미반환). 등록은
        // 쓰기라 허용. 권한은 프로필과 동일 — 연결 경계 위임(ADR-0016 / decision 7).
        ("remote.passkey.list", plugin(Read, &[])),
        ("remote.passkey.get", plugin(Read, &[])),
        ("remote.passkey.add", plugin(Idempotent, &[])),
        ("remote.passkey.remove", plugin(Idempotent, &[])),
        // ── notification ──────────────────────────────────────────────
        ("notification.list", plugin(Read, &[Notification])),
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
        // general settings 전역 read/write 라 plugin 권한 모델에 대응 variant 가
        // 없다 — memory.gc / system.gpu_stats 처럼 local_only 로 두어 plugin 에는
        // 노출하지 않고 로컬 IPC(CLI·에이전트)만 조작한다. focus 독립(전역 설정,
        // 대상 ID 불요). set 은 핸들러가 UpdateSettings intent 로 태워 collapse/save
        // 파이프라인을 재사용한다.
        ("settings.get_remote_transfer", local_only(Read)),
        ("settings.set_remote_transfer", local_only(Idempotent)),
        // ── file_handler.* (host config 관리 — local-only) ───────────
        // user TOML 변경 후 재로드. plugin 이 호출할 일은 없으며 (자기 manifest
        // 도 reload 영향 밖이라) local 전용.
        ("file_handler.reload", local_only(Idempotent)),
        // finalize 된 detector 와 출처별 contribution 조회. 읽기 전용이라 사용자 상태에
        // 닿지 않는다. plugin 에는 노출하지 않는다 — user 설정의 원본(표시명·아이콘·
        // 켜기/끄기 patch)까지 싣는데 그것에 대응하는 plugin 권한 variant 가 없다
        // (`hook_handler.list` 와 같은 판단).
        ("file_handler.detectors", local_only(Read)),
        // 임의 경로를 file_handler dispatch 흐름에 진입시킨다. 임의 path 를
        // 읽고 (handler 가 OpenSurface 면 surface 의 param 으로, System 이면 OS
        // opener 가 읽음) 처리하므로 FsRead 권한 요구. explorer plugin 더블클릭
        // 같은 사용처가 주된 caller.
        ("file_handler.dispatch", plugin(Mutate, &[FsRead])),
        // ── hook_handler.* (공유 훅 핸들러 레지스트리 — local-only) ───────
        // 웹훅/훅이 공유하는 핸들러 레지스트리 조회(list)/user config 재로드(reload)/
        // id 로 수동 발화(dispatch). webhook.* 와 동일하게 지금은 전부 local_only —
        // plugin 이 HookHandler 권한으로 list/dispatch 를 호출하는 실배선은 후속(S11).
        // reload 는 user config 변경 후 재읽기라 애초에 plugin 무관(local 전용).
        ("hook_handler.list", local_only(Read)),
        ("hook_handler.get", local_only(Read)),
        // get/upsert/remove 도 같은 이유로 local 전용이되 근거가 하나 더 있다 —
        // IpcSequence 는 **Local 권한으로 실행된다.** plugin 이 시퀀스를 고칠 수 있으면
        // 자기 권한 집합을 넘어선 IPC escalation 이 되므로, `webhook.register` 가
        // plugin 의 인라인 sequence 를 거부하는 것과 같은 자리에서 막는다.
        ("hook_handler.upsert", local_only(Idempotent)),
        ("hook_handler.remove", local_only(Idempotent)),
        ("hook_handler.reload", local_only(Idempotent)),
        ("hook_handler.dispatch", local_only(Mutate)),
        // ── completion_strategy.* (완료 판정 전략 레지스트리 — local-only) ──
        // hook_handler.list 미러 — 등록된 전략(비활성 포함) 조회만.
        // reload/dispatch 대응물 없음: "발화" 개념이 없고(판정 함수일 뿐),
        // user config 재로드는 아직 노출하지 않는다(Settings UI CRUD 표면 없음).
        ("completion_strategy.list", local_only(Read)),
        // markdown surface 제자리 이동 — 주어진 surface 를 새 파일의 markdown
        // 으로 교체한다. 임의 path 를 읽으므로 FsRead. markdown 주소창 플러그인이 caller.
        (
            "markdown.navigate",
            plugin(Mutate, &[FsRead]).kept_on_every_host_path(),
        ),
        // generic per-kind 최근목록 조회 — 주소창 드롭다운 데이터 공급원(plugin 이
        // kind 를 채워 호출). 임의 파일 read 가 아니라 이미 열었던 목록 반환뿐이라 더
        // 약한 SurfaceRead 권한. host 는 특정 kind 이름을 모른다(generic).
        ("recent.query", plugin(Read, &[SurfaceRead])),
        // ── fs.* (native 파일시스템 자원 위임 — host 프로세스 전용) ─────
        // ── git_viewer.* (docs/adr/0056-git-viewer-remote-attach-git-query-channel.md
        // — 원격 attach mirror git 조회 트리거) ─
        // git-viewer plugin 이 mirror workspace 에서 status/log/worktrees snapshot
        // 또는 diff 를 요청. host 는 즉시 request_id 만 회신하고(비동기 accept), 실제
        // 조회는 attach Control 채널 왕복 후 `event.dispatch` unicast 로 plugin 에
        // push 된다(popup.set_context 는 이 결과 전달에 쓰지 않는다 — context 필드가
        // 없음). 임의 원격 경로 read 라 FsRead(파일을 고르는 read 관심사, `file_picker.trigger` 와 동일 근거).
        ("git_viewer.query", plugin(Read, &[FsRead])),
        // ── markdown_mirror.* (docs/adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md
        // — 원격 attach mirror markdown 원문 조회 트리거) ─
        // markdown plugin 이 mirror 문서의 원격 원문을 요청한다. host 는 즉시 request_id 만
        // 회신하고(비동기 accept), 원문은 attach Control 채널 왕복 후 `event.dispatch`
        // unicast 로 plugin 에 push 된다. 원격 파일 read 라 FsRead(`git_viewer.query` 와
        // 동일 근거).
        ("markdown_mirror.content_request", plugin(Mutate, &[FsRead])),
        // ── file_picker.* (plugin 트리거 host 소유 file_picker popup) ─
        // plugin(현재는 markdown Browse)이 host 소유 `file_picker` popup(ADR-0053)을
        // 열도록 트리거한다. host 는 즉시 request_id 만 회신하고(비동기 accept,
        // ADR-0058), 실제 확정/취소 결과는 확정 지점에서 `event.dispatch` unicast
        // `"file_picker.result"` 로 plugin 에 push 된다. 파일을 고르는 read 관심사라
        // FsRead(`git_viewer.query` 와 동일 근거). 비-plugin 호출자(CLI·agent)는 이 표가
        // 아니라 핸들러가 `-32016` 으로 거부한다 — 외부 arm 이 있어 `plugin_only` 를 못 단다
        // (docs/adr/0504-the-file-picker-trigger-answers-only-a-plugin-caller.md).
        ("file_picker.trigger", plugin(Mutate, &[FsRead])),
        // ── popup (plugin → host) ─────────────────────────────────────
        // 자기 contribute popup 인스턴스를 명시적으로 닫는다. METHOD_POPUP_CLOSED
        // (host → plugin)와는 다른 방향. plugin은 자기 instance_id만 닫을 수 있다 —
        // 다른 plugin의 인스턴스 close 요청은 만들어진 응답에서 거부.
        ("popup.close", plugin_only(Idempotent, &[UiPopup])),
        // ── banner (plugin → host, A3) ────────────────────────────────
        // 자기 contribute banner 를 자기 surface 에 띄운다(D1 소유권 검증은 App).
        ("banner.open", plugin_only(Idempotent, &[UiBanner])),
        // 자기 배너 인스턴스를 명시적으로 닫는다.
        ("banner.close", plugin_only(Idempotent, &[UiBanner])),
        // ── webview 외부 열기 (plugin → host) ──────────────────────────
        // 자기 webview surface 안의 외부 링크를 OS 기본 핸들러로 연다. plugin 프로세스가
        // OS 열기를 직접 하지 않게 host 한 자리로 모은다(ADR-0527). 소유권(자기 surface)
        // 검증은 App. 외부 IPC 호출자에게는 arm 이 없다 — 사용자 브라우저를 여는 것은
        // 에이전트가 자기 작업에 쓰는 능력이 아니다(원칙 1).
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
        // 번들 plugin 을 실행 중 인스턴스에 재sync — install/remove/enable/disable
        // 과 같은 lifecycle 계열이라 같은 근거로 닫는다. 호출자는 개발 중 재빌드를
        // 반영하는 사람이나 dist 업그레이드 경로이지 plugin 자신이 아니고, plugin
        // 이 자기(또는 남의) 번들 바이너리를 교체할 수 있으면 lifecycle 소유가
        // 뒤집힌다 — docs/dev-guide/plugin-development.md §9.1.
        ("plugin.upgrade_builtins", local_only(Idempotent)),
        ("plugin.permissions", local_only(Read)),
        ("plugin.grant", local_only(Idempotent)),
        ("plugin.revoke", local_only(Idempotent)),
        // agent 임시 grant. grant/revoke 는 user/operator 만, list 는
        // readonly 라 plugin/agent 도 self-introspection 가능.
        ("plugin.grant_agent_permission", local_only(Idempotent)),
        ("plugin.revoke_agent_permission", local_only(Idempotent)),
        ("plugin.list_agent_permissions", plugin(Read, &[])),
        // agent 가 자기 권한 부족을 미리 알고 elevation 을 명시
        // 발행할 entry point. approval.request 와 동일한 의미이므로 Approval
        // 권한이 필요.
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
        // focused 창에 의존하지 않는다(원칙 3). 임의 경로 파일 쓰기 표면이라 local_only
        // (plugin 미노출) — CLI/로컬 client 만 호출.
        ("ui.screenshot", local_only(Mutate).kept_by_app_layer()),
        // E.C.e (D1=b) — Tasty 내부 어휘 통일에 따른 view.* alias. 동작은 window.* 와 동등.
        // wire format 호환을 위해 양쪽 메서드 명 모두 살림. payload 의 `window_id`
        // 필드는 외부 wire format 이라 변경 X.
        ("view.create", local_only(Mutate).kept_by_app_layer()),
        ("view.close", local_only(Idempotent)),
        ("view.list", local_only(Read)),
        // window.focus / view.focus 는 debug 빌드 전용 (DEBUG_METHODS 참조).
        // CLAUDE.md: 포커스 전환은 사용자 단축키/마우스 입력 영역.
        // script.reload (init.lua 재로드) 는 ADR-0031 에서 제거됨 — 스크립트는
        // 등록 목록 + 명시 트리거(단축키)로만 실행되고 부팅 자동로드가 폐기됐다.
    ]
};

/// debug 빌드에서만 등록되는 메서드. release에서는 [`method_meta`]가 `None`을
/// 반환해 IPC 표면에서 완전히 사라진다. 핸들러 함수 본체와 라우터 분기는
/// 이미 `#[cfg(debug_assertions)]`로 보호되어 있으므로, 표 등록만 게이트하면
/// 일관된 release 표면이 된다.
///
/// 카테고리:
/// - `system.shutdown` — 호스트 종료 (사용자가 직접 종료해야 하는 동작)
/// - `ui.state` — UI 상태 dump (디버깅용)
/// - `debug.*` — 사용자 입력 재현 / 디버그 dump
///
/// (`ui.screenshot` 은 focus-독립 리팩토링으로 [`METHOD_TABLE`] 로 승격됨.)
///
/// `.kept_on_every_host_path()` 가 붙은 `Mutate` 는 GUI 의 debug step(app_methods step **뒤**)이
/// 끝내는 이름이다. 그 step 도 보존소를 지나지만 판 [`KEY_KEPT_ON_EVERY_HOST_PATH`] 부터다. 그
/// 목록이 debug step 의 실제 dispatch 와 맞는지는 본체의 `source_guards::key_contract_by_layer`
/// 가 잰다.
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
    // window/egui 입력 주입(마우스·키·문자) — 위 inject_* 와 같은 사용자 입력 재현
    // 계열이라 같은 debug 격리(원칙 1·3). release 미노출. 문자는 키와 다른 이벤트라
    // (`Event::Text`) 따로 있다 — 키 주입으로는 `TextEdit` 에 글자가 안 들어간다.
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
    // 임의 Lua 주입(ADR-0031) — release 에는 이 경로가 없다(원칙 1). local 전용.
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
    // 마우스 라우팅 회귀 안전망용 read-only dump — 관찰 전용(사용자 상태 불변). release 미노출.
    ("debug.selection", local_only(MethodEffect::Read)),
    ("debug.pending_menu", local_only(MethodEffect::Read)),
    ("debug.focused_surface", local_only(MethodEffect::Read)),
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
    // 런타임 설정 patch 적용 — 사용자 "설정 저장" 재현. release 미노출.
    ("debug.settings.apply", local_only(MethodEffect::Idempotent)),
    // 활성 모달에 창 닫기 **요청**을 흘린다 — 사용자가 창 닫기 버튼을 누른 것의 재현.
    // release `window.close` 가 main view 만 대상으로 두고 모달을 뺀 것과 같은 선이다.
    // WM 없는 Xvfb 에는 `WM_DELETE_WINDOW` 를 보낼 손이 없어(실측: `xdotool windowclose`
    // 는 `XDestroyWindow` 를 불러 winit 이 패닉하고, `wmctrl -i -c` 는 WM 이 없으면
    // 아무도 처리하지 않는다) 자동 검증에는 이 경로가 유일하다.
    (
        "debug.modal.close_request",
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
    // 사용자 입력 재현 — 포커스 전환은 단축키/마우스 영역.
    // view.focus 는 window.focus 의 alias (E.C.e, D1=b). debug 빌드 only.
    ("window.focus", local_only(MethodEffect::Idempotent)),
    ("view.focus", local_only(MethodEffect::Idempotent)),
    // ── OS 전역 입력 상태 조작 (macOS) — 사용자 입력 재현 ──────────
    // `surface.raw_key` 는 CGEventPost 로 **OS 이벤트 스트림에** 키를 주입한다.
    // 대상 surface 를 받을 수단이 없고(그 순간 OS 포커스를 가진 무엇이든 받는다),
    // `surface.switch_input_source` 는 TISSelectInputSource 로 **시스템 입력 소스**
    // 를 바꾼다 — 둘 다 사용자가 키보드/입력기 메뉴로 하는 조작의 재현이라
    // release 표면에 두지 않는다. 에이전트가 자기 작업으로 터미널에 키를 넣는
    // 경로는 대상 ID 를 받는 `surface.send_key`(release) 다.
    // 런타임 `--enable-input-simulation` 게이트가 추가로 걸린다(inject_* 와 동일).
    (
        "surface.switch_input_source",
        local_only(MethodEffect::Idempotent),
    ),
    ("surface.raw_key", local_only(MethodEffect::Mutate)),
    // `surface.ime_*` 도 같은 계열(창 IME 조합 상태 강제 세팅 — 사용자 입력기
    // 조합 재현)이지만 개별 등재가 아니라 prefix 로 해소되므로 아래
    // [`PREFIX_RULES`] 쪽에 같은 cfg 격리를 걸어 뒀다.
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
/// [`register_plugin_prefix`] 로 *runtime* 등록되어 `method_meta()` 의 마지막
/// fallback 단계에서 해소된다. 정적 `PREFIX_RULES` 는 host 자체 메서드의
/// prefix-fallback 전용.
#[cfg(debug_assertions)]
pub const PREFIX_RULES: &[(&str, MethodMeta)] = &[("surface.ime_", local_only(Idempotent))];
#[cfg(not(debug_assertions))]
pub const PREFIX_RULES: &[(&str, MethodMeta)] = &[];

/// plugin 이 `[[contributes.ipc_namespace]]` 로 점유한 prefix 의 **소유 표**.
///
/// **사본이 아니라 원본이다.** 예전에는 이 자리에 `HashMap` 미러가 따로 있었고
/// host 가 자기 표를 고칠 때마다 같은 값을 여기에 한 번 더 썼다 — 표가 둘이면
/// 갱신을 한쪽만 하는 결함이 생기고, 실제로 났다(제거된 plugin 의 prefix 가 남아
/// `-32002` 로 거절하던 것). 지금은 host 가 든 것과 **같은 `Arc`** 를 부팅 때 한 번
/// 받는다. 쓰는 쪽은 하나뿐이고 여기는 읽기만 한다.
///
/// 결정·대안·경계는
/// [ADR-0179](../../../docs/adr/0179-the-resolver-is-handed-the-table-not-a-callback.md).
///
/// 설치 전에는 "등록된 prefix 가 없다" 로 답한다 — 옛 미러가 그 시점에 비어 있던 것과
/// 같은 답이다. 부팅 전 의미를 바꾸지 않는다.
static NAMESPACE_TABLE: OnceLock<Arc<RwLock<IpcNamespaceRegistry>>> = OnceLock::new();

/// 부팅 때 **1 회** 설치한다. 이미 설치돼 있으면 아무것도 안 하고 `false`.
///
/// 덮어쓰기를 허용하지 않는 이유: 표를 바꿔 끼우는 것은 소유 관계를 통째로 갈아치우는
/// 일이라 진행 중인 라우팅이 어느 표를 봤는지가 호출 시점에 달리게 된다. 한 프로세스에
/// `PluginManager` 는 하나이므로 설치도 한 번이면 된다.
pub fn install_namespace_table(table: Arc<RwLock<IpcNamespaceRegistry>>) -> bool {
    NAMESPACE_TABLE.set(table).is_ok()
}

/// 테스트가 쓰는 표. 설치는 1 회뿐이라 **바꿔 끼울 수 없고**, 그래서 테스트는 첫 번째가
/// 설치한 표를 함께 쓰고 내용만 직렬화해서 비운다(`test_lock`). 운영 코드에는 이 경로가
/// 없다 — 예전의 `clear_plugin_prefixes_for_tests` 가 `doc(hidden) pub` 으로 운영 표면에
/// 뚫려 있던 것을 `cfg(test)` 로 닫은 것이다.
#[cfg(test)]
pub(crate) fn test_namespace_table() -> &'static Arc<RwLock<IpcNamespaceRegistry>> {
    NAMESPACE_TABLE.get_or_init(|| Arc::new(RwLock::new(IpcNamespaceRegistry::new())))
}

/// 이 락의 임계구역은 표 조회뿐이라 패닉이 지나가도 남는 값이 성립한다 — 그래서 복구가
/// 답이다. 조용히 건너뛰면 poison 이 sticky 인 탓에 소유 판정이 **영구히 "아무도 소유하지
/// 않는다"** 가 되고, 그러면 plugin namespace 로 갈 호출이 전부 host 로 샌다.
const NAMESPACES_WHAT: &str = "the plugin IPC namespace table";
static NAMESPACES_POISON_REPORTED: AtomicBool = AtomicBool::new(false);

/// `prefix` 를 어떤 plugin 이 `[[contributes.ipc_namespace]]` 로 점유하고 있는지 조회.
///
/// 호출부가 이 값을 `!` 로 뒤집어 쓰므로 **여기서 `false` 로 물러나면 차단이 뚫린다**
/// — 표를 못 읽었다는 사정이 "등록된 적 없는 prefix" 와 같은 답이 되어, 막으려던
/// 우회가 그대로 열린다. 그래서 락이 poison 이어도 복구해서 실제 값을 본다.
/// 표가 아직 설치되지 않은 것은 다른 사실이고(부팅 전), 그때는 `false` 가 참이다.
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

/// `plugin_id` 가 `prefix` 를 `[[contributes.ipc_namespace]]` 로 점유하고 있는가.
///
/// [`is_registered_plugin_prefix`] 는 "누군가 점유했나" 를 묻고, 여기는 "**이 plugin 이**
/// 점유했나" 를 묻는다. 게이트가 둘을 가르는 자리는 둘이다 — plugin 이 자기 namespace 를
/// 부르는 trampoline 은 `ipc.invoke:<자기>` 를 요구하지 않고, `session.issue` 는 소유자가
/// 자기 namespace 의 토큰을 자식에게 넘기는 것을 허용한다. 락 poison 처리는
/// [`is_registered_plugin_prefix`] 와 같다.
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

/// 이 이름이 **표에 그 이름 그대로 적혀 있는가**. prefix fallback 은 보지 않는다.
///
/// `method_meta()` 와 다른 물음이다. 저쪽은 "이 이름을 어떻게 다뤄야 하나" 를 묻고
/// 그래서 정적 `PREFIX_RULES` 와 **런타임 등록 plugin prefix** 까지 4 단계로 해소한다.
/// 여기서 묻는 것은 "**우리가 이 이름을 우리 것으로 적어 뒀나**" 하나뿐이다.
///
/// 두 물음을 섞으면 설치된 plugin 의 표면이 통째로 host 것으로 오인된다. 실측
/// 2026-09-05: `method_meta(...).is_some()` 을 "표에 있다" 로 읽고 종단 응답을 가르면
/// `claude.children` · `agent_stream.list` 는 물론 `markdown.no_such_thing` 같은 **오타까지**
/// 마지막 단계(런타임 prefix)에 걸려 host 의 답을 받는다 — plugin 으로 갈 호출이 안 간다.
/// 그 모수는 유한하지도 않다(그 prefix 아래 임의의 이름이 전부 해당된다).
pub fn is_registered_name(method: &str) -> bool {
    METHOD_TABLE.iter().any(|(name, _)| *name == method)
        || DEBUG_METHODS.iter().any(|(name, _)| *name == method)
}

/// 0.7.0 시점에 동결된 메서드 이름 목록. `METHOD_TABLE` 의 스냅샷이고 major bump
/// 전까지 안 바뀐다 — 갱신 절차는 파일 머리 주석과 `docs/dev-guide/release.md`.
const FROZEN_BASELINE_0_7: &str = include_str!("../fixtures/method_baseline_0_7.txt");

/// 이 이름이 **0.7.0 표면에 이미 있었는가.**
///
/// 두 값뿐인 이유는 답할 수 있는 것이 그것뿐이기 때문이다. 동결 파일은 0.7.0 시점의
/// 이름 집합 하나이고, 그 뒤에 더해진 이름들이 **각각 언제** 들어왔는지는 어디에도 안
/// 적혀 있다. 그 값을 지금 손으로 채우면 커밋 로그에서 추정한 수가 표의 값이 되고,
/// 그것은 재현되지 않는다. 그래서 더 잘게 나누지 않는다.
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

/// 등재된 이름이 언제부터 있었는가. **미등록 이름은 `None`** — "없다" 와 "예전부터
/// 있었다" 를 같은 값으로 답하지 않는다.
///
/// 값은 손으로 안 적는다. `METHOD_TABLE` 옆의 동결 파일이 유일한 모수이고, 이 함수는
/// 그것을 읽을 뿐이다 — 표에 이름을 더하면 이 답이 **자동으로** 따라온다. 둘째 사본을
/// 두면 표에 더하면서 이쪽을 빠뜨리는 것이 기본 동작이 된다([ADR-0306] 이 같은 이유로
/// `effect` 를 별도 테이블로 두지 않았다).
///
/// **답이 빌드 조합에 따라 갈리는 이름이 있다.** `DEBUG_METHODS` 는 debug 빌드에만 있으므로
/// `debug.` 로 시작하는 이름은 debug 에서 `AfterFrozenBaseline`, release 에서 `None` 이다.
/// 등록 여부를 먼저 보는 `is_registered_name` 의 성질을 그대로 물려받은 것이고, 동결
/// baseline 에 그 접두사가 하나도 없어 **분류 자체는 안 흔들린다.** 다만 이 답을 내보내는
/// capability 선언은 조합과 무관하므로, client 가 이 값으로 분기하면 그때 갈린다.
///
/// [ADR-0306]: ../../../docs/adr/0306-a-method-declares-what-a-second-delivery-leaves-behind.md
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
        // plugin namespace 아래의 이름은 **무엇이든** plugin 이 받는다. 요구 권한은
        // 이름에서 나오는 `ipc.invoke:<prefix>` 하나라 정적 칸에 못 적는다 — 표시만
        // 남기고 게이트가 그 토큰을 요구한다(`CallerContext::ensure_allowed`).
        return Some(MethodMeta {
            plugin_callable: true,
            plugin_only: false,
            required: &[],
            namespace_forward: true,
            // 이 이름을 구현하는 것은 plugin 이고 호스트는 그 뜻을 모른다. 그래서
            // 재전달이 안전한지도 모르고, 모를 때 고를 값은 **가장 조심스러운 쪽**이다 —
            // `Read` 로 두면 소비자가 마음대로 다시 보내도 된다고 읽는다.
            effect: MethodEffect::Mutate,
            // forward 는 engine 라우터의 보존소 앞에서 plugin 으로 나간다 — 키를 실어도
            // 재생되지 않는다(ADR-0361).
            key_contract: KeyContract::Outside,
        });
    }
    None
}

/// 이 이름에 멱등 키를 실으면 지켜지는가 — **이 프로세스가 아는 표로** 답한다.
///
/// 표가 이름을 모르면 [`KeyContract::Outside`] 다. 호스트의 보존소는 호스트가 아는 이름에만
/// 닿을 수 있고, 표가 모르는 이름은 둘 중 하나다 — 존재하지 않는 메서드(`-32601`)이거나
/// plugin namespace. 뒤쪽이 이 판정의 요점이다: **client 프로세스에는 namespace 소유 표가
/// 없어서**(그 표는 호스트가 부팅 때 설치한다) 거기서 plugin 메서드는 `method_meta` 가 `None`
/// 이 된다. 호스트 프로세스에서는 같은 이름이 namespace fallback 으로 해소돼 `Outside` 가
/// 나온다 — 두 프로세스의 답이 같다.
///
/// 비용이 한 방향으로만 틀린다: 표보다 **새로운** 서버에 더해진 호스트 메서드는 옛 client 가
/// `Outside` 로 읽어 키를 안 싣는다. 그때 호출자는 보내기 전에 거절을 받으므로 두 번째 효과는
/// 안 난다.
pub fn key_contract(method: &str) -> KeyContract {
    method_meta(method).map_or(KeyContract::Outside, |m| m.key_contract)
}

#[cfg(test)]
#[path = "method_meta_tests.rs"]
mod tests;
