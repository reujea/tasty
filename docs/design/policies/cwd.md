# CWD 정책

각 surface 가 자기 "현재 폴더"(cwd)를 **정의·갱신**하는 방식. 이 cwd 는 surface 전환·새 탭 cwd 상속·split carry·터미널 링크 해석·닫힌 항목 복원에 쓰인다. 앞 절들은 *각 surface 가 자기 cwd 를 어떻게 정의/갱신하는가* 이고, 생성·변환 시 cwd 가 *손실 없이 carry* 되는 invariant 는 아래 [Surface cwd invariant](#surface-cwd-invariant) 절이다.

호스트는 `CoreState::surface_cwd(sid)`(`src/core/state/surface_cwd.rs`)로 조회 — terminal kind 면 `engine.terminals.get(sid).get_cwd()`(store 경유), 그 외는 `Surface::source_cwd()`. 반환값은 출처가 구분된 `SurfaceCwd`(`Local` / `Remote`)이고, mirror 워크스페이스의 surface 는 `Remote` 다 — 원격 경로가 로컬 실행 자리로 새지 않게 하는 규칙은 아래 [§3-2](#3-2-원격-출처-cwd-는-로컬-실행-경로로-새지-않는다).

## Surface 별 cwd

| Surface | "현재 폴더" 정의 | 갱신 트리거 | host 가 아는 방법 |
|---------|------------------|-------------|------------------|
| **Terminal** | shell 의 `$PWD` | shell 이 OSC 7 송신 시 | termwiz parse → `cached_cwd`(store), `get_cwd()` 로 조회 |
| **Markdown** / **Image** | 열린 파일의 parent | *불변* | `source_cwd()` = `file.parent()` |
| **Empty** | 생성 시 carry 된 cwd | 불변 | `source_cwd()` = `self.cwd` |
| **Explorer**(host 내장) | 생성 시 프로젝트 루트 | *불변* — 안에서 폴더를 옮겨도 안 바뀐다 | `source_cwd()` = 고정 cwd (`crates/tasty-model/src/explorer_panel.rs`) |
| **기타 RemoteSurface** | 각 plugin 의미 | 각 plugin | `surface.set_cwd` 권장, 미구현 시 None(호환 OK) |

- **불변 surface**(Markdown/Image/Empty): 사용자가 그 안에서 위치를 이동할 수단이 없어 갱신 메커니즘 불요 — `source_cwd()` 정적 반환.
- **동적 surface**(Terminal): 사용자가 안에서 이동(`cd`) → 갱신 메커니즘 필수.

## 터미널 OSC 7

셸이 프롬프트마다 `\e]7;file://hostname/path\e\\` 를 보내면 즉시 `cached_cwd` 반영(비용 0, 이벤트 기반). 캐시가 비면 `get_cwd()` 는 OS 조회로 폴백한다 — Linux `/proc/<pid>/cwd` · macOS `proc_pidinfo` · Windows 없음(`crates/tasty-terminal/src/cwd.rs`). 코드: `crates/tasty-terminal/src/vte_handler/osc.rs`(수신), `accessors.rs`(`get_cwd`/`set_cached_cwd`).

| 셸 | OSC 7 |
|----|-------|
| zsh / fish | 기본 지원 |
| bash | 수동 (`PROMPT_COMMAND='printf "\033]7;file://%s%s\033\\" "$HOSTNAME" "$PWD"'`) |
| PowerShell 7+ | 수동 (`prompt` 함수) |

OSC 7 이 오면 그 surface 가 속한 탭의 이름도 다시 매긴다 — 탭의 focused surface 의 cwd 마지막 이름(홈이면 `~`)이고, 명시 이름과 OSC 제목이 있으면 그쪽이 이긴다. 두 빌드 조합이 같다: gui 는 `TerminalCwdChanged` → `SurfaceCwdChanged` cascade 가, headless 는 PTY drain(`src/boot.rs`)이 `intent::headless::apply_terminal_cwd_changed` 로 같은 engine 갱신을 한다([headless-build-boundaries](../../dev-guide/headless-build-boundaries.md) "두 조합이 같게 하는 것"). 시험 `tests/e2e_tests.rs` 의 `an_osc7_cwd_becomes_the_tab_name` 이 두 조합에서 같은 단언을 본다.

셸이 OSC 7 을 안 보내면 `cached_cwd` 가 비어, 새 분할의 부모 cwd 상속은 Linux/macOS 에선 위 OS 조회가 대신하고 Windows 에선 동작하지 않는다(프롬프트 설정으로 해결). (Windows 는 합성 rcfile 로 bash 의 OSC 7 emit 강제 — [terminal](../../features/terminal/index.md).)

## `surface.set_cwd` IPC (RemoteSurface)

동적 cwd plugin surface 가 host 에 통보:

```jsonc
{ "method": "surface.set_cwd", "params": { "surface_id": 42, "cwd": "/foo/bar" } }  // cwd: null 도 허용(해제)
```

- 권한 `surface.write`. plugin 은 *사용자 인지 "현재 폴더" 가 바뀐 모든 path* 에 발사(단일 setter 로 모으는 패턴 권장 — 누락 방지). host 는 `RemoteSurface.cwd` 에 보관, `source_cwd()` 가 반환 → 기존 carry 경로에 자동 합류. 옛 SDK(이 IPC 모름)는 None 으로 남음(추가만 — 호환 유지).

## Surface cwd invariant

Surface 의 `cwd` 가 변환/생성 경로 전구간에서 **손실 없이 carry** 되어야 한다는 규칙. 사용자 의도와 무관한 *호스트 시작 cwd*(예: `cargo run` 시점의 working dir)가 새 surface 의 cwd 행세를 하지 않도록 한다.

### 동기

Terminal(`/foo/bar`) → Explorer 변환 시, 예전엔 변환 타깃이 cwd 를 carry 하지 않아 Explorer 가 `std::env::current_dir()` 로 fallback 했다. 결과적으로 사용자가 `cd /foo/bar` 한 터미널에서 변환해도 호스트 프로세스 시작 dir 이 root 로 표시되는 버그가 났다.

### 규칙

#### 1. `Surface::source_cwd()` 명시 의무 (default 없음)

`source_cwd()` 는 **default 본문이 없다**(`crates/tasty-model/src/surface_trait.rs`) — 모든 `impl Surface` 가 의미를 명시해야 한다(compile-time 강제).

| impl | source_cwd |
|------|-----------|
| `TerminalSurface` | `None` — cwd 는 terminal store(`get_cwd()`) 경유, `CoreState::surface_cwd()` 가 분기 |
| `EguiMeshSurface`(plugin egui-mesh surface) | 자기 file 의 parent (자체 의미 우선; file 없으면 None) |
| `EmptySurface` | carry 한 `self.cwd` (없으면 None) |
| `ExplorerPanel` | 활성 탭의 **고정 cwd**(프로젝트 루트) — 현재 폴더(current)를 하위로 오가도 스폰 cwd 는 cwd 불변. cwd↔current 분리는 [features/explorer](../../features/explorer/index.md) |
| `AttachMeshSurface` / `DagGraphSurface` | `None` |
| `RemoteSurface`(plugin surface) | `None` — plugin 이 `ctx.cwd` 로 받아 자체 보유, host trait 에는 비노출. host carry 는 `SurfaceCreateCtx.cwd` 로 *한 번만* 전달 |

#### 2. carry 경로 강제 — `SurfaceKindDef::create` 시그니처

```rust
pub create: Arc<dyn Fn(SurfaceId, Option<&Path>, &serde_json::Value)
    -> anyhow::Result<Box<dyn Surface>> + Send + Sync>,
```

두 번째 인자 `Option<&Path>` 가 cwd. 모든 builtin + remote plugin kind 등록자가 이 시그니처를 따라 host 가 cwd 를 *모든* 생성 경로에 일관 주입한다. `CoreState::create_surface_via_registry` 호출자(워크스페이스 첫 surface · 새 탭 · ConvertSurface · SplitPane · SplitSurface)가 cwd 를 받아 전달.

#### 3. `ConvertSurfaceTarget::Kind` 에 cwd 동봉

```rust
pub(crate) enum ConvertSurfaceTarget {
    Terminal { cwd: Option<PathBuf> },
    Kind { cwd: Option<PathBuf>, kind: String, params: Value },
}
```

호출자가 명시 안 하면(`cwd: None`) intent handler(`src/intent/surface.rs::convert`)가 source surface 에서 carry 한다 — **fallback 결정은 항상 intent handler 가 담당**, 호출자가 임의로 `None` 고정 금지.

##### 3-1. mirror(원격 attach) forward 경로도 같은 불변식 대상

mirror 워크스페이스의 convert 는 로컬에서 실행되지 않고 `StructuralOp::ConvertSurface` 로 원격에 forward 된다([features/remote-attach](../../features/remote-attach/index.md)). 이 경로에서도 cwd 는 손실되지 않는다 — 우선순위는 **op 의 `cwd` > 원격 서버의 자체 resolve** 다.

| 단계 | 담당 | 값 |
|------|------|----|
| client → wire | `src/core/impl_mirror.rs` (`build_mirror_forward_op`) | intent 에 **명시된** cwd 만 `StructuralOp::ConvertSurface.cwd`(경로 문자열, `#[serde(default)]`) 로 실어 보낸다. mirror surface 에서 carry 한 cwd 는 원격 출처(§3-2)라 로컬 carry 헬퍼가 `None` 을 돌려주므로 싣지 않는다 — 서버가 자기 PTY 에서 resolve 하는 값이 진실 원천이고 client 가 가진 값은 그 사본이다 |
| 원격 실행 | `src/core/attach_runtime.rs` (`execute_forwarded_structural_op`) | op 의 `cwd` 가 비어 있으면 `AppState::resolve_inherit_cwd_from_surface` 로 **실제 원격 PTY** 기준(OSC 7 캐시 → Linux `/proc`·macOS `proc_pidinfo`) cwd 를 직접 판정한다 |
| 관측 push (server → client) | `CoreState::forward_surface_cwd` → `StreamControl::Cwd` → client `mirror_surface_cwd` 맵 | 서버가 1Hz 로 점유 surface 의 cwd 를 자기 트리에서 계산해 값이 바뀐 것만 holder 에 보낸다. 실행이 아니라 **관측**이다([ADR-0267](../../adr/0267-mirror-surface-cwd-is-pushed-by-the-server.md)) |

서버측 resolve 는 로컬 convert 와 같은 헬퍼를 쓰므로 **원격 인스턴스의 `inherit_cwd` 설정 게이트를 그대로 적용**한다(실행 주체의 설정 의미론을 따르는 쪽이 로컬 실행과 대칭). `cwd` 키가 없는 구버전 client 의 op 도 이 서버측 resolve 로 커버된다. **이 게이트는 실행 경로(서버측 resolve)에 한정된다** — 관측 push 는 `inherit_cwd` 와 무관하게 raw cwd 를 보내고, 게이트는 소비 시점(client)이 건다. `inherit_cwd` 는 "새 surface 가 cwd 를 상속하는가" 이지 "cwd 를 아는가" 가 아니다.

##### 3-2. 원격 출처 cwd 는 로컬 실행 경로로 새지 않는다

두 인스턴스의 파일시스템은 다르다. surface cwd 의 판정은 `CoreState::surface_cwd`(`src/core/state/surface_cwd.rs`) 하나이고 반환 타입 `SurfaceCwd` 가 출처를 가른다 — `Local(PathBuf)` / `Remote(RemoteCwd)`. mirror 워크스페이스에 속한 surface 의 값은 출처(OSC 7 캐시 · explorer root · plugin surface 의 `set_cwd`)와 무관하게 `Remote` 이고, `RemoteCwd` 에는 `Path` 로 가는 변환이 없다([ADR-0267](../../adr/0267-mirror-surface-cwd-is-pushed-by-the-server.md) 결정 3). 분류는 **값이 어디서 들어왔는가가 아니라 surface 가 어느 워크스페이스에 있는가**로 한다 — 그래서 cwd 가 들어오는 경로(`surface.set_cwd` IPC, `plugin_bridge/remote_kind.rs` 의 생성 시 `set_cwd`)를 따로 막지 않아도 나가는 쪽에서 한 번에 걸린다.

`AppState::resolve_inherit_cwd` / `resolve_inherit_cwd_from_surface` 는 `inherit_cwd` 게이트를 건 뒤 **`Local` 만** 돌려준다. 소비 지점 판정(판정 근거는 그 자리의 mirror 게이트 위치다):

| 소비 지점 | cwd 가 가는 곳 | 판정 |
|---|---|---|
| `intent/workspace.rs` (새 워크스페이스 intent — GUI 경로) | 새 **로컬** 워크스페이스의 첫 PTY | 상속 원본은 그 창의 포커스 surface. 그것이 mirror 면 `None`(= 홈) |
| `adapters/ipc/handler/workspace.rs` (`workspace.create`) | 새 **로컬** 워크스페이스의 첫 PTY | 상속 원본은 지목한 `surface_id`, 없으면 그 창의 포커스 surface([ADR-0532](../../adr/0532-workspace-create-inherits-cwd-from-the-surface-that-names-its-window.md)). 그것이 mirror 면 `None`(= 홈). 명시 `cwd` 는 그대로 존중 |
| `state/tab.rs` (`add_tab` · `add_kind_tab`) | mirror 면 cwd 를 읽기 **전에** forward 로 return | 로컬 워크스페이스에서만 값이 쓰인다 |
| `state/tab.rs` (`add_kind_tab_by_owner`) | owner surface 의 pane 에 로컬 생성 | mirror owner 면 `None` — 원격 경로를 로컬 surface 에 심지 않는다 |
| `intent/pane.rs` · `intent/surface.rs` (split) · `intent/tab.rs` · `adapters/ipc/handler/{pane,tab}.rs` | cwd 계산 **후** `Core::apply` 의 mirror 게이트가 forward — 그 op(`SplitPane`/`NewTab`/`SplitSurface`)는 cwd 필드가 없다 | mirror 에서는 `None` 이 계산돼 버려진다. 서버가 자기 트리에서 결정 |
| `intent/surface.rs` (convert) | mirror 면 `StructuralOp::ConvertSurface.cwd` | 명시값만 실린다(§3-1 표) |
| `core/attach_runtime.rs` (`execute_forwarded_structural_op`) | **서버측** 로컬 실행 | 서버 자기 트리의 surface 라 `Local` — 이 규칙의 대상이 아니다 |
| `state.rs` (`enqueue_convert_input_popup`) · `adapters/ui/tools_menu.rs` | plugin popup context 의 `cwd` 키 | `cwd` 키에는 `Local` 만. 원격 값은 별도 키로만 나간다(ADR-0267 결정 3) |
| `core/state/branch.rs` (StatusBar git 브랜치) | 로컬 디스크 상향 탐색 | `local_surface_cwd` — mirror surface 는 브랜치 미표시 |
| `intent/preset_capture.rs` (terminal cwd) | 영속 preset → 로컬 재실행 | `local_surface_cwd` — mirror terminal 은 cwd 없이 저장 |

`CoreState::surface_cwd` 를 거치지 않고 `Terminal::get_cwd()` 를 직접 읽는 자리는 각자 mirror 를 배제한다: `adapters/ui/terminal_link.rs` 와 `view/main/redraw.rs` 의 선택 경로 열기는 `process_id()` 가 없으면(= mirror) 로컬 검증을 건너뛰거나 빠지고, `core/layout_persistence/capture.rs` 는 mirror 워크스페이스를 저장하지 않으며, `CoreState::refresh_tab_display_name` 은 표시 전용이라 로컬 fs 를 건드리지 않는다.

#### 4. Plugin SDK 계약 — `SurfaceCreateCtx.cwd`

```rust
pub struct SurfaceCreateCtx { pub surface_id: u32, pub kind: String, pub cwd: Option<PathBuf>, pub params: Value }
```

host→plugin IPC `surface.create` payload 의 top-level `cwd` 키로 직렬화. plugin 은 ① `params` 명시 → ② `ctx.cwd` carry → ③ 자체 fallback(예: home) 순으로 결정. 옛 SDK(cwd 키 모름)는 무시 — JSON-RPC 호환.

#### 5. Explorer root fallback (host builtin)

Explorer 는 plugin 이 아니라 본체 builtin surface 다(`register_explorer` — `src/core/surface_registry/builtins.rs`). **root 는 어떤 경로로 생성되든 항상 절대경로다.** 결정 순서:

1. `params["path"]`
2. `SurfaceKindDef::create` 의 carry cwd (§2)
3. `$HOME` / `%USERPROFILE%`
4. (홈 조회 실패 시) 프로세스 cwd 를 **절대경로로 확정**해서
5. (그것도 실패 시) 파일시스템 루트 — Windows 는 `%SystemDrive%\`(없으면 `C:\`). `"\"` 단독은 드라이브 문자가 없어 `Path::is_absolute()` 가 false 라 절대경로 보장이 깨진다

1·2 의 값이 **상대경로면 채택하지 않고** 3 단계로 내려간다. 상대 root 를 프로세스 cwd 기준으로 절대화하는 선택지는 이 불변식이 금지한 "호스트 시작 cwd 가 root 행세" 를 그대로 되살리므로 채택하지 않았다. `"."` 를 root 로 두는 것은 `std::env::current_dir()` 폴백을 **지연 평가**하는 것과 동작상 같으면서, 그 문자열이 주소창·경로 복사·attach `list_dir` wire 로 새어나가므로 더 나쁘다.

4 단계(프로세스 cwd)는 홈 조회가 실패하는 환경(HOME 없는 컨테이너 등)의 최후 수단이다 — 생성 시점에 절대경로로 확정하므로 상대경로가 UI·wire 로 새지 않는다.

같은 규칙이 **snapshot 복원**(`explorer_tab_from_json`)에도 적용된다 — `root` 키가 없거나 값이 상대경로인 구 `layout.json`(과거 폴백이 저장한 `"."` 포함)은 복원 시 홈으로 교정된다. 상대 `cwd` 키는 (이미 절대로 확정된) current 를 따른다.

구현의 단일 진실원천은 `tasty_model::explorer_panel::{default_root, resolve_root}` 이고, 생성(`create`)·복원(`explorer_tab_from_json`)·빈 탭 목록 복원(`ExplorerPanel::from_tabs`) 세 경계가 모두 이를 호출한다.

주의: 이 폴백은 "cwd 가 애초에 주어지지 않았을 때" 의 방어선이지 cwd carry(§2·§3)의 대체가 아니다. carry 할 cwd 가 있는데 전달하지 않아 홈으로 떨어지는 것은 여전히 해당 생성 경로의 버그다.

### 강제 / 위반 검출

| 메커니즘 | 효과 |
|----------|------|
| trait default 제거(`source_cwd`) | 새 Surface impl 추가 시 cwd 의미 명시 강제 |
| `SurfaceKindDef.create` cwd 인자 | 모든 builtin + plugin 등록자 강제 |
| `ConvertSurfaceTarget::Kind.cwd` 필드 | 변환 경로 cwd 누락 컴파일 차단 |
| `StructuralOp::ConvertSurface.cwd` 필드 | mirror forward 경로 cwd 누락 컴파일 차단 |
| `SurfaceCwd` / `RemoteCwd` newtype | 원격 출처 cwd 를 로컬 `PathBuf` 자리에 넣는 순간을 코드에 드러낸다. 회귀 테스트 `src/state/tests.rs` 의 `mirror_explorer_cwd_is_remote_and_not_inherited_locally` |
| explorer root 회귀 테스트 | `builtins.rs` 의 `explorer_create_*` / `explorer_restore_normalizes_relative_snapshot_root` · `explorer_panel.rs` 의 `default_root_is_always_absolute` / `resolve_root_*` — 상대 root 가 생성·복원 경계를 통과하면 실패 |

검출: `rg 'ConvertSurfaceTarget::Kind \{ cwd: None'` · `rg 'PathBuf::from\("\."\)' src/core/surface_registry crates/tasty-model/src/explorer_panel.rs` · 새 impl 의 source_cwd 누락은 컴파일 실패.

## 관련

- [`features/work-area` split 명령](../../features/work-area/index.md#split-명령) — split/새 탭 상속
- [features/work-area](../../features/work-area/index.md) — Surface 도메인 (`source_cwd` 가 Surface trait 핵심)
- [concepts/plugins](../../concepts/plugins.md) — RemoteSurface plugin
- [terminal](../../features/terminal/index.md) · [terminal-link](../../features/terminal-link/index.md)(OSC 7 경로 해석)
