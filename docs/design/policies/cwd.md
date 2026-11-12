# CWD 정책

surface의 현재 폴더(cwd)는 종류 전환, 새 탭·분할의 폴더 상속, 터미널 링크 해석, 닫은 항목 복원에 쓰인다. 이 문서는 종류별 cwd와 [생성·변환 중 전달 규칙](#surface-cwd-invariant)을 설명한다.

호스트는 `EngineRef::surface_cwd(sid)`(`src/core/state/surface_cwd.rs`)로 조회한다. 터미널은 `engine.runtime.terminals.get(sid).get_cwd()`, 나머지는 `Surface::source_cwd()`를 사용한다. 반환 타입 `SurfaceCwd`는 `Local`과 `Remote`를 구분한다. mirror workspace의 cwd는 항상 원격 값이며 [로컬 실행에는 사용하지 않는다](#3-2-원격-출처-cwd-는-로컬-실행-경로로-새지-않는다).

## Surface 별 cwd

| Surface | "현재 폴더" 정의 | 갱신 트리거 | host 가 아는 방법 |
|---------|------------------|-------------|------------------|
| **Terminal** | shell 의 `$PWD` | shell 이 OSC 7 송신 시 | termwiz parse → `cached_cwd`(store), `get_cwd()` 로 조회 |
| **Markdown** | 열린 파일의 부모 디렉터리 | *불변*. 생성·복원 때 파일에서 얻는다 | `RemoteSurface::source_cwd()`. 매니페스트의 `derive_cwd` 필드(`file`)를 생성 인자와 복원 자료에서 읽는다 |
| **Image** | 열린 파일의 부모 디렉터리 | *불변* | `source_cwd()` = 절대 경로 `file`의 `parent()` |
| **Empty** | 생성할 때 전달받은 cwd | 불변 | `source_cwd()` = `self.cwd` |
| **Explorer**(host 내장) | 생성 시 프로젝트 루트 | *불변* — 안에서 폴더를 옮겨도 안 바뀐다 | `source_cwd()` = 고정 cwd (`crates/tasty-model/src/explorer_panel.rs`) |
| **HTML** | 요청한 파일 source의 부모. HTTP(S)·inline은 상속 cwd를 보관하고 저장 자료에 함께 담아 복원한다 | 생성·복원·`html.open` source 갱신 | `RemoteSurface::source_cwd()` ([전송 source 계약](../../dev-guide/html-transfer.md)) |
| **기타 RemoteSurface** | 각 plugin의 정의 | 각 plugin | `surface.set_cwd` 권장, 미구현 시 None(기존 호환 유지) |

- **불변 surface**(Markdown/Image/Empty): 사용자가 내부에서 폴더를 바꾸지 않으므로 `source_cwd()`가 고정값을 반환한다.
- **동적 surface**(Terminal): 사용자가 `cd`로 이동하므로 cwd를 갱신해야 한다.

## 터미널 OSC 7

셸이 프롬프트마다 `\e]7;file://hostname/path\e\\` 를 보내면 이벤트 처리 중 `cached_cwd`를 갱신한다. 캐시가 비면 `get_cwd()` 는 OS 조회로 폴백한다 — Linux `/proc/<pid>/cwd` · macOS `proc_pidinfo` · Windows 없음(`crates/tasty-terminal/src/cwd.rs`). 코드: `crates/tasty-terminal/src/vte_handler/osc.rs`(수신), `accessors.rs`(`get_cwd`/`set_cached_cwd`).

| 셸 | OSC 7 |
|----|-------|
| zsh / fish | 기본 지원 |
| bash | 수동 (`PROMPT_COMMAND='printf "\033]7;file://%s%s\033\\" "$HOSTNAME" "$PWD"'`) |
| PowerShell 7+ | 수동 (`prompt` 함수) |

OSC 7을 받으면 소속 탭의 이름도 갱신한다. 탭에서 포커스된 surface의 cwd 마지막 경로 요소를 쓰며 홈은 `~`로 표시한다. 명시한 이름과 OSC 제목이 있으면 이를 우선한다. GUI는 `TerminalCwdChanged` → `SurfaceCwdChanged`, 헤드리스는 PTY 처리(`src/boot.rs`)의 `intent::headless::apply_terminal_cwd_changed`로 같은 engine 상태를 갱신한다([헤드리스 가이드](../../dev-guide/headless-build-boundaries.md)). `tests/e2e_tests.rs`의 `an_osc7_cwd_becomes_the_tab_name`이 두 빌드에서 같은 결과를 확인한다.

셸이 OSC 7 을 안 보내면 `cached_cwd` 가 비어, 새 분할의 부모 cwd 상속은 Linux/macOS 에선 위 OS 조회가 대신하고 Windows 에선 동작하지 않는다. Windows 의 터미널 셸은 Git Bash 뿐이고 합성 rcfile 이 OSC 7 을 보낸다([terminal](../../surfaces/terminal/index.md)).

## `surface.set_cwd` IPC (RemoteSurface)

plugin은 surface의 cwd가 바뀌면 호스트에 알린다.

```jsonc
{ "method": "surface.set_cwd", "params": { "surface_id": 42, "cwd": "/foo/bar" } }  // cwd: null 도 허용(해제)
```

권한은 `surface.write`다. plugin은 사용자가 보는 현재 폴더가 바뀌는 모든 경로에서 이 메서드를 호출한다. 공용 setter를 사용하면 누락을 줄일 수 있다. 호스트는 `RemoteSurface.cwd`에 저장하고 `source_cwd()`로 반환해 기존 상속 경로에 전달한다. 이 IPC를 사용하지 않는 옛 SDK는 None을 유지한다.

<a id="surface-cwd-invariant"></a>

## cwd 전달 규칙

Surface의 `cwd`는 생성·변환 경로 전체에서 전달해야 한다. 빠뜨린 값을 호스트 시작 디렉터리(예: `cargo run`을 실행한 폴더)로 대신하지 않는다.

### 동기

예를 들어 `/foo/bar`에서 작업하는 터미널을 Explorer로 바꾸면 같은 폴더를 표시해야 한다. cwd를 빠뜨리고 `std::env::current_dir()`로 대체하면 호스트를 시작한 폴더가 표시된다.

### 규칙

#### 1. `Surface::source_cwd()` 명시 의무 (default 없음)

`source_cwd()` 는 **default 본문이 없다**(`crates/tasty-model/src/surface_trait.rs`) — 모든 `impl Surface` 가 의미를 명시해야 한다(compile-time 강제).

| impl | source_cwd |
|------|-----------|
| `TerminalSurface` | `None` — cwd 는 terminal store(`get_cwd()`) 경유, `EngineRef::surface_cwd()` 가 분기 |
| `EguiMeshSurface`(plugin egui-mesh surface) | 자신이 연 절대 경로 파일의 부모 디렉터리. 파일이 없거나 상대 경로면 None |
| `EmptySurface` | 전달받은 `self.cwd` (없으면 None) |
| `ExplorerPanel` | 활성 탭의 **고정 cwd**(프로젝트 루트) — 현재 폴더(current)를 하위로 오가도 스폰 cwd 는 cwd 불변. cwd↔current 분리는 [explorer surface](../../surfaces/explorer/index.md) |
| `AttachMeshSurface` / `DagGraphSurface` | `None` |
| `RemoteSurface`(plugin surface) | HTML 파일 source는 파일 부모가 우선한다. 그 외는 생성·복원 때 매니페스트 `derive_cwd` 필드의 절대 경로 부모를 쓰고, 없으면 생성 시 전달한 값이나 저장한 cwd를 쓴다. `surface.set_cwd`로 갱신할 수 있다. 값이 없으면 `None` |

#### 2. carry 경로 강제 — `SurfaceKindDef::create` 시그니처

```rust
pub create: Arc<dyn Fn(SurfaceId, Option<&Path>, &serde_json::Value)
    -> anyhow::Result<Box<dyn Surface>> + Send + Sync>,
```

두 번째 인자 `Option<&Path>`가 cwd다. 기본 제공 종류와 plugin 종류 모두 이 인자를 받는다. 워크스페이스 첫 surface·새 탭·ConvertSurface·SplitPane·SplitSurface의 생성 요청이 cwd를 담고, 생성을 실행하는 `effect_runner::prepare`가 그 cwd를 kind의 `create`에 전달한다.

IPC 생성(`workspace.create` 의 `resolve_create_cwd`, `tab.create`·`split` 의 `create_spec`)은 명시 `cwd` 를 kind 와 관계없이 담는다. 명시값이 없을 때 지목한 surface·포커스 surface 에서 이어받는 것은 terminal 뿐이다. 다른 kind 는 cwd 없이 만들어 자기 기본값(explorer 는 홈)을 쓴다. `tab.create` 는 명시 `cwd` 가 있으면 kind 기본 params 의 `@home` 을 풀지 않는다. 그래야 explorer 의 `path = "@home"` 기본값이 명시 cwd 를 가리지 않는다. 없는 폴더를 명시하면 kind 와 관계없이 `cwd does not exist` 로 거절한다.

`tab.create`·`split`의 존재 검사는 상속한 cwd에도 적용된다. 그래서 surface의 cwd 폴더가 지워졌으면 IPC `new tab`은 cwd를 명시하지 않는 한 `cwd does not exist`로 거절된다. 복원된 markdown처럼 문서 폴더에서 cwd를 얻은 surface도 같고, 재시작 여부로 결과가 갈리지 않는다. 호출자는 있는 폴더를 `--cwd`로 명시한다. 탭바 `+`와 단축키는 이 IPC 검사를 지나지 않는다.

상대 `--file`은 CLI가 호출자 폴더를 `file_base`로 함께 보내고, 호스트가 대상이 로컬일 때만 절대 경로로 바꾼다(`create_spec::resolve_relative_file`). mirror 대상의 `file`은 원격 경로라 바꾸지 않는다. 기준 없이 들어온 상대 경로에서는 cwd를 얻지 않는다. 호스트 시작 폴더 기준으로 해석되기 때문이다.

`SurfaceKindDef::restore`도 cwd를 인자로 받는다. 세션 저장과 닫은 항목은 `RemoteSurface`가 보관한 cwd(`held_cwd`)를 저장 자료(`SurfaceData::Generic.cwd`)에 함께 담는다. 파일 source의 부모는 보관 cwd가 아니라서 담지 않고 복원 때 `url`에서 다시 계산한다. 그래서 재시작 여부로 source 이동 뒤의 cwd가 갈리지 않는다. 이 키가 없는 옛 저장본은 cwd 없이 읽는다. 비-deferred `EmptySurface`는 capture 대상이 아니어서 이 자료에 담기지 않는다. mirror는 저장 대상이 아니며 mirror의 복원 경로는 cwd를 넘기지 않는다.

#### 3. `ConvertSurfaceTarget::Kind` 에 cwd 동봉

```rust
pub(crate) enum ConvertSurfaceTarget {
    Terminal { cwd: Option<PathBuf> },
    Kind { cwd: Option<PathBuf>, kind: String, params: Value },
}
```

호출자가 `cwd: None`으로 보내면 App의 `src/app/creation_intent.rs`가 원본 surface에서 가져온다. 생략된 cwd의 대체값은 이 핸들러가 정하며, 호출자가 임의로 None에 고정하지 않는다.

##### 3-1. mirror(원격 attach) forward 경로도 같은 불변식 대상

mirror 워크스페이스의 새 탭·pane 분할·surface 분할·convert 는 로컬에서 실행되지 않고 `StructuralOp` 로 원격에 forward 된다([features/remote-attach](../../features/remote-attach/index.md)). 이 경로에서도 cwd 는 손실되지 않는다. 우선순위는 **op 의 `cwd` > op 의 `inherit_cwd` 가 정한 상속 원본 > 서버 View 선택(fallback)** 이다.

서버 View 의 선택과 client View 의 선택은 서로 독립이다([포커스 정책](focus.md)). 그래서 원격 생성 요청의 상속 원본은 client 가 요청 값 `inherit_cwd` 에 명시한다. 값은 셋이다.

| `inherit_cwd` | client 가 싣는 때 | 서버 동작 |
|---|---|---|
| 없음(또는 `null`) | client `inherit_cwd` 설정이 켜져 있는데 고를 surface 가 없을 때(예: 창 정보 없이 온 요청). 이 필드를 모르는 client 도 같다 | 서버 View 의 선택으로 상속 원본을 고른다. 게이트는 서버 자신의 `inherit_cwd` 설정이다. 일반 fallback 규칙이며 구버전 전용 분기가 아니다 |
| `{"surface": <원격 surface id>}` | client 설정이 켜져 있을 때 client View 가 고른 surface. 새 탭·pane 분할은 anchor pane 에서 client View 가 선택한 탭의 선택 surface, surface 분할·convert 는 대상 surface 자신 | 그 surface 의 실행 시점 위치를 서버 쪽 로컬 규칙(`local_surface_cwd`)으로 잇는다. 터미널은 서버 PTY 기준 cwd, 비터미널은 `source_cwd` 다. 서버 `inherit_cwd` 설정으로 막지 않는다. client 가 이미 판단했다 |
| `"none"` | client `inherit_cwd` 설정이 꺼져 있을 때 | 상속하지 않는다. 서버 선택도 쓰지 않는다. 명시 `cwd` 가 없으면 kind 기본(터미널은 셸 기본 위치) |

| 단계 | 담당 | 값 |
|------|------|----|
| client → wire (GUI) | `src/app/services/impl_mirror.rs` (`App::mirror_forward_op` → `build_mirror_forward_op`) | 요청을 낸 창의 View 선택으로 상속 원본을 정해 `inherit_cwd` 에 싣는다. intent 에 **명시된** cwd 만 `StructuralOp::ConvertSurface.cwd`(경로 문자열) 로 싣는다. mirror surface 에서 carry 한 cwd 는 원격 출처(§3-2)라 로컬 carry 헬퍼가 `None` 을 돌려주므로 싣지 않는다 — 실제 기준은 서버가 자기 PTY에서 읽은 값이며 클라이언트 값은 그 사본이다 |
| client → wire (IPC·CLI) | `src/app/journal/commands/forwarding.rs` (`local_op`) | mirror pane 을 지목한 `tab.create`·`split` 등은 그 pane 에서 client View 가 선택한 탭의 선택 surface 를 싣는다(로컬 요청과 같은 규칙) |
| 로컬 → 원격 ID | `src/app/attach_client/forward.rs` (`remote_structural_op`) | 고른 로컬 mirror surface 를 같은 세션 매핑으로 원격 ID 로 바꾼다. 매핑이 없으면 보내지 않고 요청을 실패로 끝낸다. 필드 없음으로 바꿔 보내지 않는다 |
| 원격 실행 | `src/app/journal/commands/inbound.rs`와 `src/app/journal/commands/create_spec.rs` (원 요청 해석) | `inherit_cwd` 를 검증한 뒤 상속 원본을 정하고, **실제 원격 PTY** 기준(OSC 7 캐시 → Linux `/proc`·macOS `proc_pidinfo`) cwd 를 직접 판정한다 |
| 관측 push (server → client) | `EngineMut::forward_surface_cwd` → `StreamControl::Cwd` → client `mirror_surface_cwd` 맵 | 서버가 1Hz 로 점유 surface 의 cwd 를 자기 트리에서 계산해 값이 바뀐 것만 holder 에 보낸다. 실행이 아니라 **관측**이다([원격 attach (Remote attach)](../../features/remote-attach/index.md)) |

**서버 검증**: 서버는 `{"surface": N}` 의 N 이 요청 연결이 점유한 workspace 에 있고 anchor(새 탭·pane 분할)나 대상 surface(surface 분할·convert)와 같은 pane 에 있는지 확인한다. 아니면 아무것도 만들지 않고 `StructuralResult { ok: false, reason }` 으로 거절한다. 이 빌드가 모르는 값(`"all"`, `{"pane": 1}` 등)도 `unsupported inherit_cwd value …` 로 거절한다. 모르는 값을 상속 안 함이나 필드 없음으로 읽지 않는다. 원본의 cwd 는 로컬 상속과 같은 `local_surface_cwd` 규칙으로 읽는다. 터미널은 셸 위치, 비터미널은 `source_cwd`(explorer root, 파일의 부모 폴더 등)다. cwd 를 모르면 거절하지 않고 상속하지 않는다.

**게이트**: `inherit_cwd` 설정 게이트는 요청을 만든 쪽(client)의 설정이다. 서버 자신의 설정은 필드가 없을 때(fallback)만 쓴다. **이 게이트는 실행 경로에 한정된다** — 관측 push 는 `inherit_cwd` 와 무관하게 raw cwd 를 보내고, 게이트는 소비 시점(client)이 건다. `inherit_cwd` 는 "새 surface 가 cwd 를 상속하는가" 이지 "cwd 를 아는가" 가 아니다.

**호환**: 서버는 이 필드를 이해하면 attach descriptor 의 `stream_capabilities` 에 `ipc.stream.inherit-cwd` 를 싣는다. 이 필드를 모르는 옛 서버는 필드를 무시해 fallback 규칙(서버 선택 + 서버 설정)과 같은 결과가 된다. 단 `"none"` 은 옛 서버에서 지켜지지 않는다. client 는 capability 가 없는 서버에도 요청을 그대로 보내고 info 로그만 남긴다.

**닫은 항목 복원**: 원격 `RestoreClosedItem` 은 서버 View 선택을 쓰지 않는다. 대상 pane 은 client 가 정해 anchor 로 싣고, 서버는 그 pane 과 점유 workspace 범위의 마지막 복원 기록을 저장된 스냅샷의 cwd 로 다시 만든다. 그래서 `inherit_cwd` 를 싣지 않는다.

##### 3-2. 원격 출처 cwd 는 로컬 실행 경로로 새지 않는다

두 인스턴스의 파일시스템은 다르다. surface cwd 의 판정은 `EngineRef::surface_cwd`(`src/core/state/surface_cwd.rs`) 하나이고 반환 타입 `SurfaceCwd` 가 출처를 가른다 — `Local(PathBuf)` / `Remote(RemoteCwd)`. mirror 워크스페이스에 속한 surface 의 값은 출처(OSC 7 캐시 · explorer root · plugin surface 의 `set_cwd`)와 무관하게 `Remote` 이고, `RemoteCwd` 에는 `Path` 로 가는 변환이 없다([원격 attach (Remote attach)](../../features/remote-attach/index.md)). 분류는 **값이 어디서 들어왔는가가 아니라 surface 가 어느 워크스페이스에 있는가**로 한다 — 그래서 cwd 가 들어오는 경로(`surface.set_cwd` IPC, `plugin_bridge/remote_kind.rs` 의 생성 시 `set_cwd`)를 따로 막지 않아도 나가는 쪽에서 한 번에 걸린다.

`MainViewState::resolve_inherit_cwd` / `resolve_inherit_cwd_from_surface` 는 `inherit_cwd` 게이트를 건 뒤 **`Local` 만** 돌려준다. 사용처별 처리 방식은 다음과 같다.

| 소비 지점 | cwd 가 가는 곳 | 판정 |
|---|---|---|
| `intent/workspace.rs` (새 워크스페이스 intent — GUI 경로) | 새 **로컬** 워크스페이스의 첫 PTY | 상속 원본은 그 창의 포커스 surface. 그것이 mirror 면 `None`(= 홈) |
| `adapters/ipc/handler/workspace.rs` (`workspace.create`) | 새 **로컬** 워크스페이스의 첫 surface | 상속(terminal 만) 원본은 `inherit_cwd` 가 명시한 surface, 없으면 지목한 `surface_id`, 그것도 없으면 그 창의 포커스 surface([CLI 크레이트 내부 구조 — 세 갈래 대칭](../../dev-guide/cli-structure.md)). 그것이 mirror 면 `None`(= 홈). `inherit_cwd` 의 surface 는 모든 창(parked 포함)의 engine 에서 찾고, 어디에도 없으면 거절한다. 그 surface 의 cwd 는 로컬 상속과 같은 `local_surface_cwd` 규칙으로 읽는다(비터미널은 `source_cwd`). 명시 원본은 이 인스턴스의 `inherit_cwd` 설정으로 막지 않고, `"none"` 이면 상속하지 않는다. 명시 `cwd` 는 kind 와 관계없이 그대로 존중 |
| `state/tab.rs` (`add_tab` · `add_kind_tab`) | mirror 면 cwd 를 읽기 **전에** forward 로 return | 로컬 워크스페이스에서만 값이 쓰인다 |
| `state/tab.rs` (`add_kind_tab_by_owner`) | owner surface 의 pane 에 `DomainIntent::CreateTab` 을 보낸다. 상속 원본은 owner surface 다. 그 창의 포커스는 결과에 영향을 주지 않는다 | owner surface 가 mirror 면 `None`. owner pane 이 mirror 면 App journal 이 `NewTab` 으로 forward 해 cwd 가 버려진다. 유일한 호출처인 explorer "새 탭으로 열기"는 `path` param 이 root 를 정하고 mirror 에서는 핸들러가 먼저 막는다 |
| `intent/pane.rs` · `intent/surface.rs` (split) · `intent/tab.rs` · `adapters/ipc/handler/{pane,tab}.rs` | App journal의 mirror 대상 해소가 forward — 그 op(`SplitPane`/`NewTab`/`SplitSurface`)는 cwd 경로 대신 상속 원본 `inherit_cwd`(원격 surface id)를 싣는다 | mirror 에서 계산된 로컬 cwd 는 `None` 이라 버려진다. 서버가 client 가 고른 원본의 서버 쪽 위치(터미널은 자기 PTY cwd, 비터미널은 `source_cwd`)로 결정(§3-1) |
| `intent/surface.rs` (convert) | mirror 면 `StructuralOp::ConvertSurface.cwd` | 명시값만 실린다(§3-1 표) |
| `app/journal/commands/inbound.rs` · `create_spec.rs` (원격 요청 해석) | **서버측** 로컬 실행 | 서버 자기 트리의 surface 라 `Local` — 이 규칙의 대상이 아니다 |
| `state.rs` (`enqueue_convert_input_popup`) · `adapters/ui/tools_menu.rs` | plugin popup context 의 `cwd` 키 | `cwd` 키에는 `Local` 만. 원격 값은 별도 키로만 나간다(docs/features/remote-attach/index.md) |
| `state/branch.rs` (StatusBar git 브랜치) | 로컬 디스크 상향 탐색 | `local_surface_cwd` — mirror surface 는 브랜치 미표시 |
| `intent/preset_capture.rs` (terminal cwd) | 영속 preset → 로컬 재실행 | `local_surface_cwd` — mirror terminal 은 cwd 없이 저장 |

`EngineRef::surface_cwd`를 거치지 않고 `TerminalStore::cwd`를 읽는 소비자도 원격 경로를 로컬 실행에 쓰지 않는다. `view/main/mouse.rs`의 링크 검출은 같은 항목에 Pty가 없으면 mirror로 표시해 로컬 검증을 건너뛴다. `view/main/redraw.rs`의 선택 경로 열기는 명시 Pty의 PID가 있어야 진행한다. journal snapshot과 호환 export는 mirror projection을 포함하지 않으며, `EngineMut::refresh_tab_display_name`은 표시 전용이라 로컬 fs를 건드리지 않는다.

#### 4. Plugin SDK 계약 — `SurfaceCreateCtx.cwd`

```rust
pub struct SurfaceCreateCtx { pub surface_id: u32, pub kind: String, pub cwd: Option<PathBuf>, pub params: Value }
```

호스트에서 plugin으로 보내는 `surface.create` 메시지의 최상위 `cwd` 키에 넣는다. plugin은 명시한 `params`, 전달받은 `ctx.cwd`, 자체 기본값(예: 홈) 순서로 선택한다. 옛 SDK는 모르는 cwd 키를 무시하므로 JSON-RPC 호환을 유지한다.

#### 5. Explorer root fallback (host builtin)

Explorer 는 plugin 이 아니라 본체 builtin surface 다(`register_explorer` — `src/runtime/surface_registry/builtins.rs`). **root 는 어떤 경로로 생성되든 항상 절대경로다.** 결정 순서:

1. `params["path"]`
2. `SurfaceKindDef::create` 의 carry cwd (§2)
3. `$HOME` / `%USERPROFILE%`
4. (홈 조회 실패 시) 프로세스 cwd 를 **절대경로로 확정**해서
5. (그것도 실패 시) 파일시스템 루트 — Windows 는 `%SystemDrive%\`(없으면 `C:\`). `"\"` 단독은 드라이브 문자가 없어 `Path::is_absolute()` 가 false 라 절대경로 보장이 깨진다

1·2의 값이 상대경로면 사용하지 않고 3단계로 넘어간다. 프로세스 cwd를 기준으로 절대경로를 만들면 사용자가 요청한 폴더 대신 호스트 시작 폴더를 쓰게 된다. `"."`를 그대로 저장해도 같은 문제가 생기며, 주소창·경로 복사·attach `list_dir` 응답에 상대경로가 노출된다.

4 단계(프로세스 cwd)는 홈 조회가 실패하는 환경(HOME 없는 컨테이너 등)의 최후 수단이다 — 생성 시점에 절대경로로 확정하므로 상대경로가 UI·wire 로 새지 않는다.

같은 규칙이 **snapshot 복원**(`explorer_tab_from_json`)에도 적용된다 — `root` 키가 없거나 값이 상대경로인 구 `layout.json`(과거 폴백이 저장한 `"."` 포함)은 복원 시 홈으로 교정된다. 상대 `cwd` 키는 (이미 절대로 확정된) current 를 따른다.

이 규칙은 `tasty_model::explorer_panel::{default_root, resolve_root}` 에 구현한다. 생성(`create`)·복원(`explorer_tab_from_json`)·빈 탭 목록 복원(`ExplorerPanel::from_tabs`)이 모두 이 함수를 호출한다.

홈 등의 대체값은 cwd가 주어지지 않았을 때만 사용한다. 상속할 cwd를 빠뜨려 홈이 선택됐다면 해당 생성 경로의 결함이다.

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
- [terminal](../../surfaces/terminal/index.md) · [terminal-link](../../features/terminal-link/index.md)(OSC 7 경로 해석)
