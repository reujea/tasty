# Lua 스크립트 (등록 + 단축키/이벤트 자동실행 트리거)

- **Status**: Implemented
- **주체**: 로컬 사용자 (자기 머신의 자기 스크립트)
- **ADR**: [0031](../../adr/0031-lua-host-api-only-worker-isolated.md) (설계 배경은 아래 [설계 경계](#설계-경계))
- **코드**: `tasty-lua` 크레이트(engine/host_api/sandbox/bridge), 스크립트 저장소 `tasty-settings`(`ScriptRegistry`), 단축키 바인딩 `KeybindingSettings.script_bindings`
- **화면**: 설정 modal 단축키 탭 › Scripts (단축키 바인딩) + 기타(Misc) 탭 › Scripts (관리 + 자동실행 트리거 편집)

## 목적

사용자가 Lua 스크립트를 **등록**하고 **단축키에 연결**하거나 **lifecycle 이벤트 트리거에 바인딩(자동실행)**해 실행한다. 스크립트는 tasty 를 **열거된 고정 호스트 API 로만** 조작하며(첫 API `tasty.tree()`), 전용 워커 스레드에서 격리 실행된다. 부팅 시 임의 Lua 자동로드(`init.lua`)는 폐기됐다 — 스크립트는 명시 트리거로만 실행된다.

> **경계 = 호스트 API.** Lua 는 tasty 내부 state 에 직접 접근할 수 없다. 읽기는 메인이 발행한 스냅샷, 쓰기는 메인 커맨드 큐를 경유한다. 이벤트 hook `tasty.on` 콜백은 **observe-only** — 호스트 흐름을 바꿀 수 없다(plugin Event Bus([reference/event-catalog](../../reference/event-catalog.md))와 별개 경로).

## 내부 동작

### 등록 · 트리거

설정에 스크립트를 등록하면 `{id, name, path, sha256, triggers}` 가 config(`~/.tasty/config.toml`)에 영속된다(`ScriptRegistry`). 트리거 채널은 둘:

- **단축키** — 단축키 탭에서 combo 바인딩(`script_bindings`, `KeybindingSettings` 소유). 누르면 워커에서 실행.
- **이벤트 자동실행** — 관리 창에서 lifecycle 이벤트를 트리거로 추가(`ScriptEntry.triggers`). host 가 그 이벤트를 fire 할 때 TOFU 재검 후 자동 실행된다. 등록 가능 이벤트는 host 가 실제 fire 하는 13종 화이트리스트(`AUTO_TRIGGER_EVENTS`): `tasty.startup.post` + window/workspace/tab/pane/surface 의 `create.post`/`delete.post` + workspace·tab 의 `change.post`. **release 는 사용자 키 입력에서만** 이 경로를 탄다(identity 원칙 1). 임의 Lua 주입은 debug 빌드 전용(`debug.lua.eval`).

**관리 창** — 설정 modal 기타(Misc) 탭 › **Scripts**(전 플랫폼·최상단, `src/view/settings/ui/tabs/misc.rs::draw_scripts_subtab`). 등록 스크립트를 행 목록으로 보여준다: script 글리프 · 표시 이름 · 중간생략 경로(디렉토리 tail 이 먼저 ellipsis, 파일명은 완전 표시) · 바운드 단축키 `Kbd` 또는 "Unbound". 행 액션은 **bind**(→ Keybindings › Scripts 진입만; 바인딩 편집은 단축키 탭 소유), **rename**(인라인), **remove**(인라인 확인 + 연결 단축키 자동 해제). 각 행 하단에 **자동실행 트리거** 행이 있다 — 등록된 트리거는 chip(이벤트명 + ✕, 클릭=제거)으로 표시하고, 화이트리스트 중 미등록 이벤트만 노출하는 콤보박스로 추가한다. 인라인 Add card(File + Browse… — OS 네이티브 다이얼로그가 아니라 설정 창 안의 로컬 파일 선택, `.lua` 필터 → [native-file-picker](../native-file-picker/index.md) "설정 창에서의 로컬 전용 재사용" / Display name)로 등록하며, 창을 열 때 디스크 해시를 저장 해시와 비교해 불일치 시 **changed** 배지 + 안내(TOFU 재확인 예고)를 표시한다. 등록이 없으면 빈 상태를 그린다.

### 실행 격리 · 안전 장치

VM 은 전용 워커 스레드가 소유하고 job 을 직렬 처리한다. 메모리 32MB cap · 텍스트 청크만(bytecode 거부) · `debug`/`load*`/`dofile`/`package.loadlib` 제거 · 무한 루프/시간 초과는 instruction-count deadline 훅으로 abort(워커만 종료, 메인 무영향) — 자동실행 경로도 동일 `Run` job 이라 deadline 이 그대로 적용된다. `os.execute`/`io.*` 는 사용 가능 — 사용자 자신의 스크립트라 권한 격리 안 함.

**자동실행 재진입 가드** — 자동실행 스크립트가 `tasty.run_cli` 로 자기 트리거 대상을 만들면(예: `surface.create.post` 바인딩 스크립트가 split 실행) 재발화 연쇄가 생긴다. deadline 은 1회 실행만 보므로, `AutofireGuard`(`src/host_api/hooks/autofire.rs`)가 자동실행 in-flight + 완료 직후 1 프레임 동안 신규 자동실행을 전역 억제해 연쇄를 유한하게 끊는다(억제 시 `tracing::warn`).

### 무결성 (TOFU)

등록 시 엔트리 파일의 SHA256 을 기록한다. 매 발화(단축키·자동실행)마다 현재 파일 해시와 비교한다:

- **단축키(수동)**: 불일치 시 실행 전 **확인 popup**(승인 시 해시 갱신 후 실행). 사용자가 계기이므로 popup 이 정당.
- **자동실행**: 불일치 시 **실행 차단 + `tracing::warn`** — 관리 창의 changed 배지로 확인하고 재승인해야 한다. 사용자 개입 없이 발화하므로 popup/배너를 띄우지 않고, 해시도 자동 갱신하지 않는다(자동 승인은 TOFU 무의미).

transitive `require` 의존 파일은 커버하지 않는다.

### 호스트 API

`tasty.on(event, cb)` · `tasty.log/warn(msg)`(tracing) · `tasty.run_cli(args)`(메인 커맨드 큐 경유 detached spawn) · `tasty.tree()`(워크스페이스 트리 read, 스냅샷 경유). 표면은 열거된 것만 — 필요한 CRUD 마다 명시 등록으로 늘린다.

### 이벤트 hook

`tasty.on` / `fire` 배관은 유지되며 observe-only 다. 발화 이벤트: `tasty.startup.post` + window/workspace/tab/pane/surface 의 `create.post`/`delete.post` + workspace·tab 의 `change.post`. payload 스키마는 `crates/tasty-lua/meta/tasty.lua`(EmmyLua stub)가 정답.

> `change` 이벤트는 **사용자가 GUI 다이얼로그로 직접 바꾼 경우만** 발화한다 — IPC/CLI rename 은 발화 안 함. 부팅 자동로드가 폐기돼 hook 을 부팅에 자동 등록하는 경로는 없다. 이벤트-트리거 **자동실행**은 이 observe-hook 과 직교하는 별도 채널이다 — 콜백을 깨우는 게 아니라 같은 fire 지점에서 등록 목록의 바인딩 스크립트를 실행한다.

## 인터페이스

- **사용자**: 설정 modal 에서 스크립트 등록·단축키 바인딩. 디버깅은 `TASTY_LOG=tasty_lua=debug`.

## 설계 경계

사용자가 등록한 Lua 스크립트로 tasty 를 조작·자동화하는 시스템의 *설계 근거*. 전체 결정은 [ADR-0031](../../adr/0031-lua-host-api-only-worker-isolated.md). 사용법은 위 절들, payload 매핑은 아래 [구현 — 발화 site · payload](#구현--발화-site--payload).

### 위치 결정 (ADR-0031)

| 항목 | 결정 |
|------|------|
| 사용 주체 | **호스트 전용.** plugin 은 Lua 미사용 |
| 등록·트리거 | 설정에 스크립트를 **등록**하고(SHA256 TOFU) **단축키 또는 이벤트 트리거(자동실행)로 실행**. 부팅 시 `~/.tasty/init.lua` 자동로드는 폐기 — 자동실행도 임의 로드가 아니라 등록 목록의 명시 트리거에서 배선. `require` 모듈 import 차단 |
| tasty 접근 | **열거된 고정 호스트 API 표면으로만.** state 직접 접근 불가, CRUD 전부 API. 첫 API 는 트리 조회 `tasty.tree()`(read) |
| 실행 격리 | **전용 워커 스레드.** 읽기=메인 발행 스냅샷, 쓰기=메인 커맨드 큐. 무한 루프/시간 초과는 instruction-count deadline 훅으로 abort |
| 무결성 | 등록 시 SHA256 기록 → **TOFU**. 수동 발화(단축키) 변경 시 확인 popup, 자동(이벤트) 발화 변경 시 **실행 차단 + `tracing::warn` + 관리창(Misc›Scripts) changed 배지**. 자동 경로는 popup/배너를 쓰지 않는다 — 발화에 사용자 계기가 없고, 배너는 사용자 직접 조작에서만 발사된다는 발화 정책과 충돌하기 때문. 해시 자동 갱신 금지(자동 승인은 TOFU 무의미) |
| 권한 | 이벤트 hook 콜백은 **observe-only** — 보고 외부 동작만. 명시 API 호출을 통한 active CRUD 는 직교 채널(흐름 소유권은 호스트) |
| 샌드박스 | 약 sandbox — `io`/`os.execute` 유지(자기 머신 자기 스크립트라 격리 무의미). 능력 제한 목적은 ① tasty 접근을 API 로 좁힘 ② 워커/메인 스레드 안전. DoS/무결성 보호(메모리 cap, `debug`/`loadlib`/`load*` 제거)만 |

> plugin 은 별 OS 프로세스로 격리돼 Rust 로 충분하므로 Lua 통로를 의도적으로 막았다. plugin 측 user-scripting 이 필요해지면 별도 채널을 새로 만든다(ADR-0009 와 함께 재검토).

### 이벤트 매트릭스 — post-only

`<entity>.<action>.<phase>` 형식, 1차는 **post-only**:

| 엔티티 | create.post | delete.post | change.post |
|--------|:-:|:-:|:-:|
| tasty | startup.post | — | — |
| window | ✓ | ✓ | — |
| workspace | ✓ | ✓ | ✓(rename) |
| tab | ✓ | ✓ | ✓(rename) |
| pane | ✓ | ✓ | — |
| surface | ✓ | ✓ | (보류 — GUI 경로 부재) |

#### `change` = 사용자 직접 변경만

`change.post` 는 **사용자가 GUI 다이얼로그로 직접 바꾼 경우에만** 발화. IPC/CLI rename 은 plugin 버스 이벤트(`workspace.renamed` 등)로는 가지만 Lua hook 으론 안 간다 — 자동화로 인한 변경까지 받으면 Slack 등에 중복 알림. 구현은 `PendingHostEvent` 의 `user_direct: bool`(GUI dialog=true, IPC handler=false)로 구분.

### 왜 post-only

현재 권한이 observe-only 라 pre/post 의미 차이가 없다. pre 는 intervention(cancel/transform) 권한이 생길 때 의미를 갖고, 그때 imperative call site 에 정밀 삽입하고 콜백 리턴으로 분기시킨다. `tasty.shutdown` 도 1차 미노출(polling 으론 부족, imperative fire 인프라 필요).

### 콜백 모델

`tasty.on(event, cb)`(동일 event 다중 등록, 순서대로). 인자는 단일 table(payload). 콜백 에러는 `tracing::warn!` 기록 후 다음 콜백 계속(한 ill-behaved hook 이 전체 dispatch 막지 않음). 리턴값 무시(observe-only). 호스트 API 표면(현재): `tasty.on`/`log`/`warn`/`run_cli`(커맨드 큐 경유)/`tree`(read).

이벤트 hook `fire`/`tasty.on` 배관은 유지되지만, 부팅 자동로드(init.lua)가 폐기되어 **hook 을 부팅에 자동 등록하는 경로는 없다.** 이벤트-트리거 **자동실행은 별도(직교) 채널로 구현되어 있다** — 콜백을 깨우는 것이 아니라, 등록 목록(`ScriptEntry.triggers`)에 바인딩된 스크립트 **소스를 트리거 발화 시 TOFU 재검 후 실행**한다(ADR-0031 의 "등록 목록에서 배선" 요구 충족).

### 자동실행 (autofire)

- **트리거**: host 가 실제 fire 하는 lifecycle 이벤트 13종 화이트리스트(`AUTO_TRIGGER_EVENTS`, `crates/tasty-settings/src/scripts.rs`)만 등록 가능. 저장은 `ScriptEntry.triggers`(단축키 combo 는 계속 `KeybindingSettings` 소유 — 이벤트 트리거는 combo 충돌 개념이 없어 scripts 소유).
- **identity 정합**: 자동실행은 사용자가 config 에 직접 바인딩한 "사용자 설정 행동" — 에이전트 행동이 아니므로 release 에 존재한다(단축키 트리거와 동일 논리). 트리거 바인딩을 조작하는 IPC API 는 만들지 않는다(설정 UI/config 경유만).
- **cascade 방어**: 자동실행 스크립트가 `run_cli` 로 자기 트리거 대상을 만들면 재발화 연쇄가 생긴다. per-job deadline 은 1회 실행만 보므로, **재진입 가드**(`AutofireGuard`, `src/host_api/hooks/autofire.rs`)가 in-flight + 완료 직후 1 프레임 동안 신규 자동실행을 전역 억제해 연쇄를 유한하게 끊는다. origin(user/agent) 게이트는 미배선 — create 계열 이벤트에 origin 판별자가 없어(아래 [이벤트 ↔ 발화 site](#이벤트--발화-site)) 게이트에 의존할 수 없다.

### 향후 확장

`pre.*`(intervention 권한 도입 시) · `tasty.shutdown.post`(shutdown fire 인프라) · surface `change.post`(GUI 타입 변경 경로 추가 시) · 호스트 API 표면 확대(mutation CRUD) · plugin Lua(미계획).

## 구현 — 발화 site · payload

호스트가 Lua hook 을 발화하는 코드 경로와 wire payload 스키마. 사용자 동작은 위 [내부 동작](#내부-동작), 설계 배경(observe-only)은 위 [설계 경계](#설계-경계).

### 구성

```
crates/tasty-lua/
  src/engine.rs    # LuaEngine — 워커 스레드 핸들 + job/커맨드 큐 + fire()
  src/host_api.rs  # tasty.log / warn / run_cli(커맨드 큐) / tree(스냅샷)
  src/sandbox.rs   # 메모리 cap + 위험 글로벌 제거
  src/bridge.rs    # LuaSnapshot / HostCommand (GUI 비의존 마샬링 타입)
  meta/tasty.lua   # EmmyLua stub (LuaLS 용)
```

`App` 가 `lua_engine: Option<LuaEngine>` 를 보유(`src/app.rs`). 부팅 시 `LuaEngine::new()` 로 VM 을 전용 워커 스레드에 기동한다 — 부팅 자동로드(init.lua)는 폐기됐다(ADR-0031). 메인은 `about_to_wait` 안전지점에서 읽기 스냅샷 발행(`publish_lua_snapshot`)과 워커 커맨드 drain(`dispatch_pending_lua_commands`)을 수행한다.

이벤트 발화는 `hooks::lua::fire` 헬퍼 한 곳을 거친다(`src/host_api/hooks/lua.rs`):

```rust
fn fire<T: Serialize>(
    lua: Option<&LuaEngine>,
    autofire: AutofireCtx<'_>,   // 등록 레지스트리 + 재진입 가드 — Option 아님
    event: &str,
    payload: &T,
)
```

`payload` 는 `serde_json::Value` 직렬화 후 Lua table 로 변환 — wire 필드 ↔ Lua table 필드 1:1(snake_case). 두 채널을 순서대로 태운다: ① observe-hook(`tasty.on` 콜백) fire, ② **자동실행**(`hooks::autofire::dispatch`) — `event` 를 트리거로 등록한 스크립트를 TOFU 재검 후 실행. `AutofireCtx` 가 필수 인자인 이유: 새 fire 지점을 추가할 때 자동실행 배선을 빠뜨릴 수 없게 시그니처로 강제한다.

### 자동실행 (autofire) 배선

`src/host_api/hooks/autofire.rs`:

- `dispatch(lua, scripts, guard, event)` — `ScriptRegistry::entries_for_event` 매칭 → 소스 read → `hash_bytes` 재검 → 일치 시 `run_script_tracked`(완료 추적) / 불일치 시 차단 + `tracing::warn`(해시 자동 갱신 금지). 단축키 경로(`try_dispatch_script_shortcut`)와 동형 시퀀스.
- `AutofireGuard` — cascade 재진입 방어. `App.lua_autofire` 가 소유하고 `about_to_wait` 시작에서 `checkpoint()` 1회. 완료 acknowledge 를 1 프레임 지연시켜, `run_cli` 가 유발한 이벤트(스크립트 완료보다 먼저 큐잉됨)가 자동실행을 재점화하지 못하게 한다. 워커의 완료 신호는 `tasty_lua::CompletionToken`(RAII — 큐 drop/abort 포함 어떤 경로로도 누락 없음).
- 트리거 저장 = `ScriptEntry.triggers`(`Vec<AutoTrigger>`, serde default). 등록 가능 이벤트 화이트리스트 = `AUTO_TRIGGER_EVENTS`(`crates/tasty-settings/src/scripts.rs`) — 아래 표의 이벤트와 1:1.

### 이벤트 ↔ 발화 site

대부분 `dispatch_pending_host_events`(`src/app/dispatch/host_events.rs`)가 `PendingHostEvent` 를 소비하며 발화한다:

| 이벤트 | Payload |
|--------|---------|
| `tasty.startup.post` | Null |
| `window.create.post` / `window.delete.post` | `WindowCreated` / `WindowClosed` |
| `workspace.{create,delete}.post` | `WorkspaceCreated` / `WorkspaceClosed` |
| `workspace.change.post` | `WorkspaceRenamed` (`user_direct==true` 일 때만) |
| `tab.{create,delete}.post` | `TabCreated` / `TabClosed` |
| `tab.change.post` | `TabRenamed` (`user_direct==true` 일 때만) |
| `pane.{create,delete}.post` | `PaneCreated` / `PaneClosed` |
| `surface.{create,delete}.post` | `SurfaceCreated` / `SurfaceClosed` |

> `surface.change.post` 는 발화 site 없음(GUI 에서 surface 타입 직접 변경 경로 추가 시 등록).

`.create/.delete` 이벤트는 **origin 무관 발화**가 계약이다 — GUI 경로와 IPC 경로가 같은 cascade 를 공유한다(`src/app/dispatch_domain.rs` 의 `cascade_*` 를 GUI dispatcher 와 `src/adapters/ipc/handler/*` 양쪽이 호출). `window.delete.post` 는 공통 helper `App::close_main_window`(`src/app/event_handler.rs`)에서 발화하며, GUI 닫기(`request_close_window`)와 IPC `window.close` 가 모두 이를 경유한다 (plugin payload 의 `reason` 만 `user`/`ipc` 로 갈린다). origin 게이팅은 아래 `change.post` 의 user-direct 분기가 유일하다.

#### change.post 의 user-direct 분기

`PendingHostEvent::{WorkspaceRenamed, TabRenamed}` 가 `user_direct: bool` 을 들고 다닌다. rename dialog(사용자 직접 GUI)는 `true`, IPC 경유(`workspace.update`/`move`)는 `false`. plugin 이벤트 버스는 구분 없이 발화하되 **Lua hook 은 `user_direct==true` 일 때만** fire.

### Payload 스키마

모든 payload 타입은 `crates/tasty-plugin-protocol/src/events/payloads.rs`. Lua 측 필드 이름 = Rust 필드(snake_case). 핵심 필드:

| Payload | 주요 필드 |
|---------|-----------|
| `WindowCreated` | `window_id`, `kind`, `modality: "modeless"\|"modal"` |
| `WorkspaceCreated` | `workspace_id`, `window_id`, `name` |
| `WorkspaceRenamed` | `workspace_id`, `name?`, `subtitle?`, `description?` |
| `TabCreated` | `tab_id`, `pane_id`, `workspace_id`, `kind` |
| `PaneCreated` | `pane_id`, `parent_pane_group?`, `workspace_id` |
| `SurfaceCreated` | `surface_id`, `kind`, `tab_id`, `pane_id`, `workspace_id`, `created_by: { kind: "user"\|"agent", source_plugin? }` |
| `*Closed` | `<id>`, `reason` (+ kind) |

EmmyLua 자동완성: 스크립트 파일 옆 `.luarc.json` 에 `"workspace.library": ["<TASTY_REPO>/crates/tasty-lua/meta"]`.

### 새 이벤트 추가

1. `PendingHostEvent` variant 확인/추가 + 발화 site 배치(polling lifecycle detection 또는 imperative push).
2. `dispatch_pending_host_events` 매치 절에 plugin 버스 emit + `hooks::lua::fire` 호출(`AutofireCtx` 필수 — 자동실행이 자동으로 함께 배선된다).
3. 자동실행 트리거로도 열 거면 `AUTO_TRIGGER_EVENTS`(`crates/tasty-settings/src/scripts.rs`)에 이벤트명 추가.
4. 이 문서의 [이벤트 hook](#이벤트-hook) 목록 · [이벤트 매트릭스](#이벤트-매트릭스--post-only) · `crates/tasty-lua/meta/tasty.lua` stub 갱신.

### 새 호스트 API 추가

`crates/tasty-lua/src/host_api.rs::install` 에 `tasty.create_function(...)` 등록 후 `tasty` global table 에 set. 강한 sandbox 아님 — 사용자 머신·사용자 스크립트라 OS 권한이 충분, 별도 권한 체크 불필요.

### 에러 / 실행

- 콜백 Lua 에러 → `tracing::warn!` + 같은 이벤트 다음 콜백 계속(dispatch 안 멈춤). payload 직렬화 실패 → warn + 이 이벤트 콜백 전부 skip.
- 스크립트 실행 = 단축키 트리거(release) / 이벤트 자동실행(release, TOFU 차단·재진입 가드 동반) / `debug.lua.eval`(debug). 워커 job 은 deadline 초과 시 abort(에러 반환) — 워커만 종료, 메인·다음 job 무영향. 자동실행 job 도 같은 `Run` 경로라 deadline 동일 적용. 부팅 자동로드(init.lua)·`script.reload` 는 ADR-0031 에서 제거됨.
- 디버그: `TASTY_LOG=tasty_lua=debug` (본체가 읽는 변수는 `TASTY_LOG` 다 — [crash-diagnostics](../../dev-guide/crash-diagnostics.md)).

## 관련

- [reference/event-catalog](../../reference/event-catalog.md) — plugin 용 Event Bus(별개 경로)
- [ADR-0031](../../adr/0031-lua-host-api-only-worker-isolated.md) — 결정 근거
