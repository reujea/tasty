# Surface cwd invariant

Surface 의 `cwd` 가 변환/생성 경로 전구간에서 **손실 없이 carry** 되어야 한다는 규칙. 사용자 의도와 무관한 *호스트 시작 cwd*(예: `cargo run` 시점의 working dir)가 새 surface 의 cwd 행세를 하지 않도록 한다.

## 동기

Terminal(`/foo/bar`) → Explorer 변환 시, 예전엔 변환 타깃이 cwd 를 carry 하지 않아 Explorer 가 `std::env::current_dir()` 로 fallback 했다. 결과적으로 사용자가 `cd /foo/bar` 한 터미널에서 변환해도 호스트 프로세스 시작 dir 이 root 로 표시되는 버그가 났다.

## 규칙

### 1. `Surface::source_cwd()` 명시 의무 (default 없음)

`source_cwd()` 는 **default 본문이 없다**(`crates/tasty-model/src/surface_trait.rs`) — 모든 `impl Surface` 가 의미를 명시해야 한다(compile-time 강제).

| impl | source_cwd |
|------|-----------|
| `TerminalSurface` | `None` — cwd 는 terminal store(`get_cwd()`) 경유, `CoreState::surface_cwd()` 가 분기 |
| `MarkdownPanel` / `ImagePanel` | 자기 file 의 parent (자체 의미 우선; file 없으면 None) |
| `EmptySurface` | carry 한 `self.cwd` (없으면 None) |
| `ExplorerPanel` | 활성 탭의 **고정 cwd**(프로젝트 루트) — 현재 폴더(current)를 하위로 오가도 스폰 cwd 는 cwd 불변. cwd↔current 분리는 [features/explorer](../../features/explorer/index.md) |
| `RemoteSurface`(plugin surface) | `None` — plugin 이 `ctx.cwd` 로 받아 자체 보유, host trait 에는 비노출. host carry 는 `SurfaceCreateCtx.cwd` 로 *한 번만* 전달 |

### 2. carry 경로 강제 — `SurfaceKindDef::create` 시그니처

```rust
pub create: Arc<dyn Fn(SurfaceId, Option<&Path>, &serde_json::Value)
    -> anyhow::Result<Box<dyn Surface>> + Send + Sync>,
```

두 번째 인자 `Option<&Path>` 가 cwd. 모든 builtin + remote plugin kind 등록자가 이 시그니처를 따라 host 가 cwd 를 *모든* 생성 경로에 일관 주입한다. `CoreState::create_surface_via_registry` 호출자(워크스페이스 첫 surface · 새 탭 · ConvertSurface · SplitPane · SplitSurface)가 cwd 를 받아 전달.

### 3. `ConvertSurfaceTarget::Kind` 에 cwd 동봉

```rust
pub(crate) enum ConvertSurfaceTarget {
    Terminal { cwd: Option<PathBuf> },
    Kind { cwd: Option<PathBuf>, kind: String, params: Value },
}
```

호출자가 명시 안 하면(`cwd: None`) intent handler(`src/intent/surface.rs::convert`)가 source surface 에서 carry 한다 — **fallback 결정은 항상 intent handler 가 담당**, 호출자가 임의로 `None` 고정 금지.

#### 3-1. mirror(원격 attach) forward 경로도 같은 불변식 대상

mirror 워크스페이스의 convert 는 로컬에서 실행되지 않고 `StructuralOp::ConvertSurface` 로 원격에 forward 된다([features/remote-attach](../../features/remote-attach/index.md)). 이 경로에서도 cwd 는 손실되지 않는다 — 우선순위는 **op 의 `cwd` > 원격 서버의 자체 resolve** 다.

| 단계 | 담당 | 값 |
|------|------|----|
| client → wire | `src/core/impl_mirror.rs` (`build_mirror_forward_op`) | intent 에 **명시된** cwd 만 `StructuralOp::ConvertSurface.cwd`(경로 문자열, `#[serde(default)]`) 로 실어 보낸다. mirror surface 에서 carry 한 cwd 는 원격 출처(§3-2)라 로컬 carry 헬퍼가 `None` 을 돌려주므로 싣지 않는다 — 서버가 자기 PTY 에서 resolve 하는 값이 진실 원천이고 client 가 가진 값은 그 사본이다 |
| 원격 실행 | `src/core/attach_runtime.rs` (`execute_forwarded_structural_op`) | op 의 `cwd` 가 비어 있으면 `AppState::resolve_inherit_cwd_from_surface` 로 **실제 원격 PTY** 기준(OSC 7 캐시 → Linux `/proc`·macOS `proc_pidinfo`) cwd 를 직접 판정한다 |
| 관측 push (server → client) | `CoreState::forward_surface_cwd` → `StreamControl::Cwd` → client `mirror_surface_cwd` 맵 | 서버가 1Hz 로 점유 surface 의 cwd 를 자기 트리에서 계산해 값이 바뀐 것만 holder 에 보낸다. 실행이 아니라 **관측**이다([ADR-0267](../../adr/0267-mirror-surface-cwd-is-pushed-by-the-server.md)) |

서버측 resolve 는 로컬 convert 와 같은 헬퍼를 쓰므로 **원격 인스턴스의 `inherit_cwd` 설정 게이트를 그대로 적용**한다(실행 주체의 설정 의미론을 따르는 쪽이 로컬 실행과 대칭). `cwd` 키가 없는 구버전 client 의 op 도 이 서버측 resolve 로 커버된다. **이 게이트는 실행 경로(서버측 resolve)에 한정된다** — 관측 push 는 `inherit_cwd` 와 무관하게 raw cwd 를 보내고, 게이트는 소비 시점(client)이 건다. `inherit_cwd` 는 "새 surface 가 cwd 를 상속하는가" 이지 "cwd 를 아는가" 가 아니다.

#### 3-2. 원격 출처 cwd 는 로컬 실행 경로로 새지 않는다

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

### 4. Plugin SDK 계약 — `SurfaceCreateCtx.cwd`

```rust
pub struct SurfaceCreateCtx { pub surface_id: u32, pub kind: String, pub cwd: Option<PathBuf>, pub params: Value }
```

host→plugin IPC `surface.create` payload 의 top-level `cwd` 키로 직렬화. plugin 은 ① `params` 명시 → ② `ctx.cwd` carry → ③ 자체 fallback(예: home) 순으로 결정. 옛 SDK(cwd 키 모름)는 무시 — JSON-RPC 호환.

### 5. Explorer root fallback (host builtin)

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

## 강제 / 위반 검출

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

- [features/work-area](../../features/work-area/index.md) — Surface 도메인 (`source_cwd` 가 Surface trait 핵심)
- [concepts/plugins](../../concepts/plugins.md) — RemoteSurface plugin
