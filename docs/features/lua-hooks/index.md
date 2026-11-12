# Lua 스크립트 (등록 + 단축키/이벤트 자동실행 트리거)

- **Status**: Implemented
- **주체**: 로컬 사용자 (자기 머신의 자기 스크립트)
- **설계**: [훅 (Surface / Global hooks)](../hooks/index.md) (설계 배경은 아래 [설계 경계](#설계-경계))
- **코드**: `tasty-lua` 크레이트(engine/host_api/sandbox/bridge), 스크립트 저장소 `tasty-settings`(`ScriptRegistry`), 단축키 바인딩 `KeybindingSettings.script_bindings`
- **화면**: 설정 modal 단축키 탭 › Scripts (단축키 바인딩) + 기타(Misc) 탭 › Scripts (관리 + 자동실행 트리거 편집)

## 목적

사용자가 Lua 스크립트를 **등록**하고 **단축키에 연결**하거나 **lifecycle 이벤트 트리거에 바인딩(자동실행)**해 실행한다. 스크립트는 tasty 를 **열거된 고정 호스트 API 로만** 조작하며(첫 API `tasty.tree()`), 전용 워커 스레드에서 격리 실행된다. 부팅 시 `init.lua`를 자동으로 읽지 않는다 — 스크립트는 명시 트리거로만 실행된다.

> **경계 = 호스트 API.** Lua 는 tasty 내부 state 에 직접 접근할 수 없다. 읽기는 메인이 발행한 스냅샷, 쓰기는 메인 커맨드 큐를 경유한다. 이벤트 hook `tasty.on` 콜백은 **observe-only** — 호스트 흐름을 바꿀 수 없다(plugin Event Bus([reference/event-catalog](../../reference/event-catalog.md))와 별개 경로).

## 내부 동작

### 등록 · 트리거

설정에 스크립트를 등록하면 `{id, name, path, sha256, triggers}` 가 config(`~/.tasty/config.toml`)에 영속된다(`ScriptRegistry`). 트리거 채널은 둘:

- **단축키** — 단축키 탭에서 combo 바인딩(`script_bindings`, `KeybindingSettings` 소유). 누르면 워커에서 실행.
- **이벤트 자동실행** — 관리 창에서 lifecycle 이벤트를 트리거로 추가(`ScriptEntry.triggers`). host 가 그 이벤트를 fire 할 때 TOFU 재검 후 자동 실행된다. 등록 가능 이벤트는 host 가 실제 fire 하는 13 종 화이트리스트(`AUTO_TRIGGER_EVENTS`): `tasty.startup.post` + window/workspace/tab/pane/surface 의 `create.post`/`delete.post` + workspace·tab 의 `change.post`. release에서는 등록된 단축키와 이벤트 트리거로 실행한다. 임의 Lua 주입은 debug 빌드 전용(`debug.lua.eval`).

**관리 창** — 설정 modal 기타(Misc) 탭 › **Scripts**(전 플랫폼·최상단, `src/view/settings/ui/tabs/misc.rs::draw_scripts_subtab`). 등록 스크립트를 행 목록으로 보여준다: script 글리프 · 표시 이름 · 중간생략 경로(디렉토리 tail 이 먼저 ellipsis, 파일명은 완전 표시) · 바운드 단축키 `Kbd` 또는 "Unbound". 행 액션은 **bind**(→ Keybindings › Scripts 진입만; 바인딩 편집은 단축키 탭 소유), **rename**(인라인), **remove**(인라인 확인 + 연결 단축키 자동 해제). 각 행 하단에 **자동실행 트리거** 행이 있다 — 등록된 트리거는 chip(이벤트명 + ✕, 클릭=제거)으로 표시하고, 화이트리스트 중 미등록 이벤트만 노출하는 콤보박스로 추가한다. 인라인 Add card(File + Browse… — OS 네이티브 다이얼로그가 아니라 설정 창 안의 로컬 파일 선택, `.lua` 필터 → [native-file-picker](../native-file-picker/index.md) "설정 창에서의 로컬 전용 재사용" / Display name)로 등록하며, 창을 열 때 디스크 해시를 저장 해시와 비교해 불일치 시 **changed** 배지 + 안내(TOFU 재확인 예고)를 표시한다. 등록이 없으면 빈 상태를 그린다.

### 실행 격리 · 안전 장치

VM은 전용 워커가 소유하고 job을 순서대로 실행한다. 메모리 한도는 32MiB다.
호스트가 읽은 스크립트는 텍스트 모드로 실행하고 `debug`, `load`, `loadstring`, `loadfile`, `dofile`,
`package.loadlib`와 `package.searchers`를 제거하며 cpath를 비운다.
`require` 이름 자체는 남지만 일반 모듈 검색 경로는 제거된다. `os.execute`와 `io`는 사용 가능하다.
이 설정은 불신 코드의 OS 접근을 격리하는 샌드박스가 아니다.

Lua5.4의 instruction-count hook이 기한을 확인해 초과 job을 오류로 끝낸다. 워커는 살아 있어 다음 job을 처리한다.
외부 OS 호출 안에 머무는 동안까지 강제로 취소하는 기능으로 해석하지 않는다.
읽기는 메인 스냅샷, 변경 요청은 메인 명령 큐를 사용한다.

자동실행은 `AutofireGuard`가 실행 중과 완료 직후 한 프레임 동안 새 자동실행을 억제한다.
스크립트가 자기 trigger 대상을 만드는 연쇄를 막으며 억제는 경고 로그와 경고 알림으로 남긴다([실행 결과 전달](#실행-결과-전달)).

### 무결성 (TOFU)

등록 시 엔트리 파일의 SHA256 을 기록한다. 실행(단축키·자동실행)할 때마다 현재 파일 해시와 비교한다:

- **단축키(수동)**: 불일치 시 실행 전 **확인 popup**(승인 시 해시 갱신 후 실행). 사용자가 계기이므로 popup 이 정당. popup 폭은 360(UI 배율 적용)이다. changed 태그는 단독 줄이고, 안내문은 그 아래 전폭 문단으로 폭 안에서 줄바꿈한다. 본문과 버튼(sm) 사이는 space-md 다. 콘텐츠 여백은 위아래 space-md · 좌우 14(시안 Padding 12/14)이며 공용 위젯이 넣는다. 그래서 이 popup 에는 셸 공통 내부 여백을 두지 않는다. 높이는 직전 프레임에 잰 콘텐츠에 맞추며 최소값이 없다. 높이 식은 타이틀바 + 잰 콘텐츠(여백 포함)다. 아직 재지 않은 첫 프레임만 기본 높이 152 를 쓴다(`script_confirm_sizer`). 콘텐츠는 갤러리 예제(Overlays › Script changed confirm)와 같은 `tasty_ui_widgets::script_confirm` 이 그린다.
- **자동실행**: 불일치 시 **실행 차단 + 경고 로그 + 경고 알림**(`lua_run.autofire_blocked_title`, [실행 결과 전달](#실행-결과-전달)) — 관리 창의 changed 배지로 확인하고 재승인해야 한다. 사용자 개입 없이 발생하므로 결과는 알림 채널로만 보내고 popup/배너를 띄우지 않는다. 해시도 자동 갱신하지 않는다(자동 승인은 TOFU 무의미).

- **갱신한 해시의 저장**: 설정은 윈도우(engine)마다 같은 사본을 둔다. 확인 popup 에서 승인하면 `dispatch_pending_script_confirm` 이 모든 engine 의 설정 사본(창·parked)에 새 해시를 넣은 뒤 저장한다. 그래서 다른 윈도우가 다른 스크립트를 승인해 자기 사본을 저장해도 이 해시가 지워지지 않고, 다른 윈도우에서 같은 스크립트를 실행할 때 다시 묻지 않는다. 설정 modal 의 Save 는 스크립트 목록을 id 로 맞춰 필드 단위로 병합하므로([설정 Save 병합](../settings/index.md)), modal 이 해시를 바꾸지 않은 스크립트는 저장 시점의 해시를 쓰고 modal 에서 추가·삭제·이름 변경한 것은 modal 결과를 따른다. 사용자 입력으로는 modal 이 열린 동안 메인 윈도우의 확인 popup 을 누를 수 없다(모든 입력 차단).
- Given 윈도우 두 개 When 각 윈도우에서 다른 스크립트의 변경을 승인한다 Then 재시작 뒤 두 해시가 모두 남는다(`lua_commands.rs` 의 `script_hashes_approved_in_two_windows_both_survive_a_restart`).
- Given 설정 modal 이 열린 동안 스크립트 해시가 승인됐다 When modal 에서 다른 스크립트를 이름 변경·삭제·추가하고 Save 한다 Then 승인한 해시는 남고 modal 의 변경도 반영된다(`tasty-settings` `merge.rs` 의 `scripts_merge_per_id_and_keep_hashes_approved_meanwhile`).

`require`로 간접 로드하는 파일의 해시는 확인하지 않는다.

### 호스트 API

`tasty.on(event, cb)` · `tasty.log/warn(msg)`(tracing) · `tasty.run_cli(args)`(메인 커맨드 큐 경유 detached spawn) · `tasty.tree()`(워크스페이스 트리 read, 스냅샷 경유). API는 열거된 것만 — 필요한 CRUD 마다 명시 등록으로 늘린다. 이 목록 밖의 함수(예: `tasty.notify`)는 없다. 정의 파일 `crates/tasty-lua/meta/tasty.lua`도 같은 목록을 정의한다.

#### `tasty.tree()` 반환 구조

메인 스레드는 이벤트 루프를 한 번 돌 때마다 트리를 만들어 스냅샷으로 발행한다. `tasty.tree()`는 호출한 시점에 마지막으로 발행된 스냅샷을 복사해 Lua table로 돌려준다. 돌려받은 table을 바꿔도 Tasty 상태는 바뀌지 않는다. 각 workspace의 필드는 IPC `list tree`와 같다. 두 경로가 같은 함수(`build_engine_tree`)로 JSON을 만들기 때문이다. 반환값은 workspace 배열이다. 세션(engine)마다 그 workspace를 차례로 이어 붙인다. 세션에는 창이 있는 세션과 창 없이 보관된 parked 세션(마지막 창을 닫았을 때, macOS에서 창을 최소화했을 때)이 있다. 창이 있는 세션이 먼저 오고(창 사이 순서는 고정돼 있지 않다) parked 세션이 보관 순서대로 뒤에 온다.

| 단계 | 필드 |
|---|---|
| workspace | `id`, `name`, `active`(그 세션에서 선택된 workspace), `busy_count`(workspace 안 surface 중 busy 수), `panes` |
| pane | `id`, `focused`(그 workspace의 포커스 pane), `busy_count`(pane 안 모든 tab의 surface 중 busy 수. 선택되지 않은 tab 포함), `tabs` |
| tab | `id`, `name`, `active`(그 pane에서 선택된 tab), `busy_count`(tab 안 surface 중 busy 수), `busy`(tab 안 surface 중 하나라도 busy), `surface` |
| `tab.surface`(나뉜 tab) | `type = "SplitLayout"`, `focused_surface`(그 tab의 포커스 surface id), `surfaces`(tab 안 surface id 배열), `layout` |
| `layout` 노드 | `Leaf`: `type`, `id`, `kind`, `busy`. `Split`: `type`, `direction`(`horizontal`·`vertical`), `ratio`, `focus_second`, `first`, `second`. `focus_second`는 분할을 만들거나 복원할 때의 hint라 현재 포커스와 다를 수 있다. 포커스는 `focused_surface`로 판단한다 |
| `tab.surface`(나뉘지 않은 tab) | surface 하나의 JSON. 모든 surface에 `id`, `type`, `busy`가 있고 나머지는 종류마다 다르다. 터미널은 `type = "Terminal"`, `pty_ready`, `cols`, `rows`(이 트리에서는 항상 0)이다. 종류별 필드 전체는 정의 파일의 `TastyTreeSurface`에 있다. |

세션 하나의 포커스 surface id는 다음 순서로 찾는다.

1. `active`가 참인 workspace
2. 그 workspace에서 `focused`가 참인 pane
3. 그 pane에서 `active`가 참인 tab
4. `tab.surface.type`이 `"SplitLayout"`이면 `tab.surface.focused_surface`, 아니면 `tab.surface.id`

```lua
local function focused_surface_id()
  for _, ws in ipairs(tasty.tree()) do
    if ws.active then
      for _, pane in ipairs(ws.panes) do
        if pane.focused then
          for _, tab in ipairs(pane.tabs) do
            if tab.active then
              local s = tab.surface
              return s.type == "SplitLayout" and s.focused_surface or s.id
            end
          end
        end
      end
    end
  end
end
```

한계는 다음과 같다.

- `active`와 `focused`는 세션(창, 또는 창 없이 보관된 parked 세션) 안에서만 판정한다. 트리에는 창이나 세션을 나타내는 필드가 없다. 세션이 여러 개면 `active`가 참인 workspace가 세션마다 하나씩 나오고, OS 포커스를 가진 창이나 화면에 보이는 세션을 트리로 구분할 수 없다. 위 예처럼 첫 번째 `active` workspace를 고르면 다른 창의 surface가 나올 수 있다. 창을 모두 닫았거나 macOS에서 창을 최소화한 상태에서는 화면에 없는 parked 세션의 surface가 나올 수 있다.
- 스냅샷은 발행 시점의 값이다. `tasty.run_cli`는 CLI 실행을 요청만 하고 완료를 기다리지 않으므로, 바로 뒤의 `tasty.tree()`에 그 결과가 반영됐다고 가정할 수 없다.

### 이벤트 hook

`tasty.on` / `fire`는 이벤트 관찰용이다. 발생 이벤트: `tasty.startup.post` + window/workspace/tab/pane/surface 의 `create.post`/`delete.post` + workspace·tab 의 `change.post`. payload 스키마는 `crates/tasty-lua/meta/tasty.lua`(EmmyLua stub)를 따른다.

> `change` 이벤트는 **사용자가 GUI 다이얼로그로 직접 바꾼 경우만** 발생한다 — IPC/CLI rename 은 발생 안 함. hook을 부팅에 자동 등록하지 않는다. 이벤트-트리거 **자동실행**은 이 콜백과 별개다 — 콜백을 깨우는 게 아니라 같은 fire 지점에서 등록 목록의 바인딩 스크립트를 실행한다.

## 인터페이스

- **사용자**: 설정 modal 에서 스크립트 등록·단축키 바인딩. 디버깅은 `TASTY_LOG=tasty_lua=debug`.

## 설계 경계

사용자가 등록한 Lua 스크립트로 tasty 를 조작·자동화하는 시스템의 *설계 근거*. 전체 결정은 [훅 (Surface / Global hooks)](../hooks/index.md). 사용법은 위 절들, payload 매핑은 아래 [구현 — 이벤트 발생 위치와 payload](#구현--발화-site--payload).

<a id="위치-결정-docs/features/file-handler/index.md"></a>

### 실행 모델

호스트만 Lua를 사용한다. plugin은 별도 OS 프로세스로 실행되며 Lua API를 제공하지 않는다.
plugin에서 사용자 스크립트를 지원하려면 이 경계를 별도로 검토해야 한다(docs/dev-guide/plugin-permissions.md).

실행·무결성·메모리 제한은 위 [실행 격리](#실행-격리--안전-장치)와 [TOFU](#무결성-tofu)를
따른다. 콜백 반환값으로 이벤트를 취소하거나 내용을 바꿀 수는 없다. 변경 작업은 명시적인
호스트 API를 호출해야 한다. OS 접근은 격리하지 않으며 `io`와 `os.execute`를 허용한다.

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

`change.post` 는 **사용자가 GUI 다이얼로그로 직접 바꾼 경우에만** 발생. IPC/CLI rename 은 plugin 버스 이벤트(`workspace.renamed` 등)로는 가지만 Lua hook 으론 안 간다 — 자동화로 인한 변경까지 받으면 Slack 등에 중복 알림. 구현은 `PendingHostEvent` 의 `user_direct: bool`(GUI dialog=true, IPC handler=false)로 구분.

### 왜 post-only

현재 권한이 observe-only 라 pre/post 의미 차이가 없다. pre 는 intervention(cancel/transform) 권한이 생길 때 의미를 갖고, 그때 imperative call site 에 정밀 삽입하고 콜백 리턴으로 분기시킨다. `tasty.shutdown` 도 1차 미노출(polling 으론 부족, imperative fire 인프라 필요).

### 콜백 모델

`tasty.on(event, cb)`(동일 event 다중 등록, 순서대로). 인자는 단일 table(payload). 콜백 에러는 `tracing::warn!` 기록 후 다음 콜백 계속(한 콜백의 오류가 다른 콜백을 막지 않음). 리턴값 무시(observe-only). 호스트 API 목록(현재): `tasty.on`/`log`/`warn`/`run_cli`(커맨드 큐 경유)/`tree`(read, 반환 구조는 [`tasty.tree()` 반환 구조](#tastytree-반환-구조)).

이벤트 자동실행은 콜백 등록과 별개다. `ScriptEntry.triggers`에 연결한 스크립트 소스를
이벤트가 발생할 때 읽고 해시를 다시 확인한 뒤 실행한다.

### 자동실행 (autofire)

- **트리거**: host 가 실제 fire 하는 lifecycle 이벤트 13 종 화이트리스트(`AUTO_TRIGGER_EVENTS`, `crates/tasty-settings/src/scripts.rs`)만 등록 가능. 저장은 `ScriptEntry.triggers`(단축키 combo 는 계속 `KeybindingSettings` 소유 — 이벤트 트리거는 combo 충돌 개념이 없어 scripts 소유).
- **identity 정합**: 자동실행은 사용자가 config 에 직접 바인딩한 "사용자 설정 행동" — 에이전트 행동이 아니므로 release 에 존재한다(단축키 트리거와 동일 논리). 트리거 바인딩을 조작하는 IPC API 는 만들지 않는다(설정 UI/config 경유만).
- **cascade 방어**: 자동실행 스크립트가 `run_cli` 로 자기 트리거 대상을 만들면 재발생 연쇄가 생긴다. per-job deadline 은 1 회 실행만 보므로, **재진입 가드**(`AutofireGuard`, `src/host_api/hooks/autofire.rs`)가 in-flight + 완료 직후 1 프레임 동안 신규 자동실행을 전역 억제해 연쇄를 유한하게 끊는다. origin(user/agent) 필터는 사용하지 않는다 — create 계열 이벤트에 origin 판별자가 없어(아래 [이벤트 발생 위치](#이벤트--발화-site)) 게이트에 의존할 수 없다.

### 향후 확장

`pre.*`(intervention 권한 도입 시) · `tasty.shutdown.post`(shutdown fire 인프라) · surface `change.post`(GUI 타입 변경 경로 추가 시) · 호스트 API 목록 확대(mutation CRUD) · plugin Lua(미계획).

<a id="구현--발화-site--payload"></a>

## 구현 — 이벤트 발생 위치와 payload

호스트가 Lua hook을 발생시키는 코드 경로와 wire payload 스키마. 사용자 동작은 위 [내부 동작](#내부-동작), 설계 배경(observe-only)은 위 [설계 경계](#설계-경계).

### 구성

```
crates/tasty-lua/
  src/engine.rs    # LuaEngine — 워커 스레드 핸들 + job/커맨드 큐 + fire()
  src/host_api.rs  # tasty.log / warn / run_cli(커맨드 큐) / tree(스냅샷)
  src/sandbox.rs   # 메모리 cap + 위험 글로벌 제거
  src/bridge.rs    # LuaSnapshot / HostCommand (GUI 비의존 마샬링 타입)
  meta/tasty.lua   # EmmyLua stub (LuaLS 용)
```

`App` 가 `lua_engine: Option<LuaEngine>` 를 보유(`src/app.rs`). 부팅 시 `LuaEngine::new()` 로 VM 을 전용 워커 스레드에 기동한다. 메인은 `about_to_wait` 안전지점에서 읽기 스냅샷 발행(`publish_lua_snapshot`)과 워커 커맨드 drain(`dispatch_pending_lua_commands`)을 수행한다.

이벤트 발생은 `hooks::lua::fire` 헬퍼 한 곳을 거친다(`src/host_api/hooks/lua.rs`):

```rust
fn fire<T: Serialize>(
    lua: Option<&LuaEngine>,
    autofire: AutofireCtx<'_>,   // 등록 레지스트리 + 재진입 가드 — Option 아님
    event: &str,
    payload: &T,
)
```

`payload` 는 `serde_json::Value` 직렬화 후 Lua table 로 변환 — wire 필드 ↔ Lua table 필드 1:1(snake_case). 두 처리를 순서대로 실행한다: ① observe-hook(`tasty.on` 콜백) fire, ② **자동실행**(`hooks::autofire::dispatch`) — `event` 를 트리거로 등록한 스크립트를 TOFU 재검 후 실행. `AutofireCtx` 가 필수 인자인 이유: 새 fire 지점을 추가할 때 자동실행 연결을 빠뜨릴 수 없게 시그니처로 강제한다.

<a id="자동실행-autofire-배선"></a>

### 자동실행 연결

`src/host_api/hooks/autofire.rs`:

- `dispatch(lua, scripts, guard, runs, event)` — `ScriptRegistry::entries_for_event` 매칭 → 소스 read → `hash_bytes` 재검 → 일치 시 `submit_script`(완료 추적 token 동반)로 접수하고 ticket을 `LuaRuns`에 `Autofire` 경로로 기록한다. 불일치·읽기 실패·접수 거절·엔진 없음·억제는 경고 로그와 함께 경고 알림(`SystemNotice`)을 돌려주고 `hooks::lua::fire`가 알림 대기열에 넣는다(해시 자동 갱신 금지). 단축키 경로(`try_dispatch_script_shortcut`)와 동형 시퀀스.
- `AutofireGuard` — cascade 재진입 방어. `App.lua_autofire` 가 소유하고 `about_to_wait` 시작에서 `checkpoint()` 1 회. 완료 acknowledge 를 1 프레임 지연시켜, `run_cli` 가 유발한 이벤트(스크립트 완료보다 먼저 큐잉됨)가 자동실행을 다시 실행하지 못하게 한다. 워커의 완료 신호는 `tasty_lua::CompletionToken`(RAII — 결과를 보낸 뒤, 또는 접수 거절·큐 drop/abort 때 job과 함께 한 번 drop된다). 제출 수는 token을 내줄 때(`issue_token`) 함께 센다. 거절된 token도 완료 수를 올리므로, 접수 성공 때만 세면 두 checkpoint 뒤 인정한 완료 수가 제출 수를 넘어 다음 재시도가 실행 중인 동안 같은 이벤트를 억제하지 못한다.
- Given 자동실행이 job 큐 포화로 거절됐다 When 두 checkpoint 뒤 재시도가 접수되고 그 실행 중에 같은 이벤트가 다시 온다 Then 재시도는 실행되고 두 번째는 억제된다(`autofire.rs`의 `a_rejected_autofire_keeps_the_reentry_guard_balanced`).

<a id="실행-결과-전달"></a>

### 실행 결과 전달

단축키·자동실행 모두 `LuaEngine::submit_script`로 실행을 맡기고 결과는 시작 경로로 돌아간다([결과 피드백 원칙](../../principles/feedback-and-recovery.md#시작-경로에-따른-결과-전달)). 메인 루프와 워커 어느 쪽도 막히지 않는다.

- **접수와 결과 슬롯**: `submit_script`는 접수할 때 결과 슬롯 하나를 잡는다(256개, `OUTCOME_QUEUE_CAP`). 슬롯은 메인이 `drain_run_outcomes`로 결과를 꺼낼 때 돌아온다. 결과 채널 용량이 슬롯 수와 같으므로 접수한 결과는 버려지지 않는다. job 큐(256)가 가득 찼거나 메인이 결과를 꺼내지 않아 슬롯이 모두 찼으면 접수하지 않고 곧바로 `RunRejected::QueueFull`을 돌려준다. 워커가 없으면 `WorkerGone`이다. 거절은 결과가 아니라 접수 실패로 즉시 시작 경로에 보인다. 결과 소비자가 사라진 경우(엔진 종료)에만 결과를 버린다.
- **결과**: 워커는 실행이 끝나면 `RunOutcome { ticket, result }`를 보내고 IPC와 같은 waker로 메인 루프를 깨운다. 실패는 `RunFailure::Error`(Lua 오류의 첫 줄, traceback은 로그에만)와 `RunFailure::TimedOut`(실행 기한 초과)이다. 시간 초과는 기한 hook이 실제로 실행을 중단했을 때만이다. 느린 호스트 호출로 예산을 넘긴 뒤 난 일반 오류는 원래 사유를 그대로 싣는다. 결과를 보낸 뒤 완료 token을 drop한다. debug 전용 `run_script`(`debug.lua.eval`)는 결과를 보내지 않는다.
- **귀속**: `App.lua_runs`(`src/host_api/hooks/lua_runs.rs`)가 ticket과 시작 경로를 보관한다. GUI는 `about_to_wait`의 `settle_lua_runs`, headless는 메인 루프가 결과를 꺼내 짝을 맞춘다. 결과가 오지 않은 항목은 512개를 넘으면 오래된 것부터 버린다.
- **단축키(사용자 입력)**: 실패하면 단축키를 누른 창(`AppEvent::RunLuaScript.window`)에 `ToastKind::Error`·`ToastScope::Window` 토스트를 띄운다. 대상은 등록 없음, 읽기 큐(8건) 포화, 파일 읽기 실패, 엔진 없음, 접수 거절, 실행 오류, 시간 초과다. 변경 확인 popup에서 승인한 실행도 승인한 창으로 돌아온다. 그 창이 닫혔으면 다른 창에 옮기지 않고 버린다. 문구는 `lua_run.toast.*`이며 길이를 모르는 사유는 가운데를 줄인다.
- **자동실행(입력에서 시작하지 않은 동작)**: 읽기 실패, TOFU 차단, 억제, 엔진 없음, 접수 거절, 실행 오류, 시간 초과를 `level: warning` 알림으로 남긴다. `src/system_notice.rs` 대기열을 거쳐 `NotificationMerge::Separate`로 저장하며 받는 곳은 활성 workspace(`surface_id: 0`)다. GUI `notification.created` 이벤트의 `source`는 `lua.autofire`다. 저장소와 `notification.list`에는 source를 저장하지 않는다. headless는 이벤트를 보내지 않는다. popup·배너는 띄우지 않는다. 문구는 `lua_run.autofire_*`이며 본문에 스크립트 이름과 이벤트를 넣는다.
- **반복 합치기**: 같은 source·레벨·제목·본문의 알림은 10초(`system_notice::REPEAT_WINDOW`) 안에 한 번만 남긴다. 억제 알림은 스크립트가 아니라 이벤트당 하나이므로 연쇄 중 같은 이벤트가 반복돼도 한 건이다. 다른 스크립트·이벤트·사유는 합치지 않는다.
- **성공**: 토스트·알림을 띄우지 않는다. 스크립트가 `tasty.run_cli`로 직접 보낸 알림은 일반 알림 규칙을 그대로 따른다.
- Given 접수한 스크립트가 `error()`를 던진다 When 결과를 꺼낸다 Then 오류 사유가 그 ticket의 결과로 돌아온다(`tasty-lua` `engine.rs`의 `submitted_runs_report_success_and_the_error_reason`, 시간 초과는 `a_run_over_the_deadline_reports_timed_out`).
- Given 느린 호스트 호출로 예산을 넘긴 뒤 `error()`를 던진다 When 실패 사유를 나눈다 Then 시간 초과가 아니라 원래 오류 사유가 남는다(`an_error_after_a_slow_host_call_keeps_its_reason`).
- Given 결과 receiver가 사라졌거나 엔진을 drop한다 When 실행 중·대기 중 job이 끝나거나 버려진다 Then 각 완료 token은 한 번씩 해제된다(`a_dropped_outcome_receiver_still_releases_the_token_once`, `dropping_the_engine_releases_every_pending_token_once`).
- Given 메인이 결과를 꺼내지 않아 결과 슬롯이 모두 찼다 When 새 실행을 요청한다 Then 접수하지 않고 `QueueFull`을 돌려주며 접수한 결과는 모두 순서대로 남는다. token은 접수·거절 모두 한 번씩 해제된다(`a_slow_consumer_gets_rejections_instead_of_lost_results`, job 큐 포화는 `a_full_job_queue_rejects_and_releases_the_token_once`, 워커 부재는 `a_gone_worker_rejects_and_releases_the_token_once`).
- Given 자동실행 스크립트가 실패한다 When 결과를 정산한다 Then `lua.autofire` 경고 알림이 생기고 토스트로 섞이지 않으며 다음 트리거는 억제되지 않고 실행된다(`autofire.rs`의 `a_failing_autofire_reports_a_warning_and_the_next_trigger_still_runs`, 단축키 결과가 알림으로 섞이지 않는 쪽은 `lua_runs.rs`의 `shortcut_failures_go_back_to_the_caller_not_to_the_notice_channel`).
- Given 자동실행 스크립트 파일이 바뀌었거나 사라졌다 When 트리거 이벤트가 온다 Then 실행하지 않고 경고 알림을 돌려주며 해시는 바꾸지 않는다(`dispatch_blocks_on_tofu_mismatch`, `a_missing_script_file_reports_a_read_warning`).
- 트리거 저장 = `ScriptEntry.triggers`(`Vec<AutoTrigger>`, serde default). 등록 가능 이벤트 화이트리스트 = `AUTO_TRIGGER_EVENTS`(`crates/tasty-settings/src/scripts.rs`) — 아래 표의 이벤트와 1:1.

<a id="이벤트--발화-site"></a>

### 이벤트 발생 위치

대부분 `dispatch_pending_host_events`(`src/app/dispatch/host_events.rs`)가 `PendingHostEvent` 를 처리하며 이벤트를 발생시킨다:

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

> `surface.change.post` 는 이벤트 발생 위치 없음(GUI 에서 surface 타입 직접 변경 경로 추가 시 등록).

`.create/.delete` 이벤트는 **origin과 관계없이 발생한다** — GUI 경로와 IPC 경로가 같은 cascade 를 공유한다(`src/app/dispatch_domain.rs` 의 `cascade_*` 를 GUI dispatcher 와 `src/adapters/ipc/handler/*` 양쪽이 호출). `window.delete.post` 는 공통 helper `App::close_main_window`(`src/app/event_handler.rs`)에서 발생하며, GUI 닫기(`request_close_window`)와 IPC `window.close` 가 모두 이를 경유한다 (plugin payload 의 `reason` 만 `user`/`ipc` 로 갈린다). origin 게이팅은 아래 `change.post` 의 user-direct 분기가 유일하다.

#### change.post 의 user-direct 분기

`PendingHostEvent::{WorkspaceRenamed, TabRenamed}` 가 `user_direct: bool` 을 들고 다닌다. rename dialog(사용자 직접 GUI)는 `true`, IPC 경유(`workspace.update`/`move`)는 `false`. plugin 이벤트 버스는 구분 없이 전달하되 **Lua hook 은 `user_direct==true` 일 때만** fire.

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

1. `PendingHostEvent` variant 확인/추가 + 이벤트 발생 위치 배치(polling lifecycle detection 또는 imperative push).
2. `dispatch_pending_host_events` 매치 절에 plugin 버스 emit + `hooks::lua::fire` 호출(`AutofireCtx` 필수 — 자동실행이 자동으로 함께 연결된다).
3. 자동실행 트리거로도 열 거면 `AUTO_TRIGGER_EVENTS`(`crates/tasty-settings/src/scripts.rs`)에 이벤트명 추가.
4. 이 문서의 [이벤트 hook](#이벤트-hook) 목록 · [이벤트 매트릭스](#이벤트-매트릭스--post-only) · `crates/tasty-lua/meta/tasty.lua` stub 갱신.

### 새 호스트 API 추가

`crates/tasty-lua/src/host_api.rs::install` 에 `tasty.create_function(...)` 등록 후 `tasty` global table 에 set. 강한 sandbox 아님 — 사용자 머신·사용자 스크립트라 OS 권한이 충분, 별도 권한 체크 불필요.

### 에러 / 실행

- 콜백 Lua 에러 → `tracing::warn!` + 같은 이벤트 다음 콜백 계속(dispatch 안 멈춤). payload 직렬화 실패 → warn + 이 이벤트 콜백 전부 skip.
- 스크립트 실행 = 단축키 트리거(release) / 이벤트 자동실행(release, TOFU 차단·재진입 가드 동반) / `debug.lua.eval`(debug). 워커 job 은 deadline 초과 시 abort(에러 반환) — 해당 job만 중단하며 워커는 다음 job을 처리. 자동실행 job 도 같은 `Run` 경로라 deadline 동일 적용. 단축키·자동실행의 실패는 [실행 결과 전달](#실행-결과-전달)에 따라 토스트나 알림으로 보인다. `script.reload`는 제공하지 않는다.
- 디버그: `TASTY_LOG=tasty_lua=debug` (본체가 읽는 변수는 `TASTY_LOG` 다 — [crash-diagnostics](../../dev-guide/crash-diagnostics.md)).

## 관련

- [reference/event-catalog](../../reference/event-catalog.md) — plugin 용 Event Bus(별개 경로)
- [훅 (Surface / Global hooks)](../hooks/index.md) — 결정 근거
