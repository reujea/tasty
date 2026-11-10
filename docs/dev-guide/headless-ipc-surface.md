# 헤드리스 IPC 지원 범위

헤드리스는 GUI 없이 IPC·CLI와 attach 서버를 실행한다. 창 없이 처리할 수 있는 기능은 두 빌드에서 같은 계약을 따른다. 다음 표는 현재 지원하는 메서드와 지원하지 않는 이유를 구분한다.

`webview.set_url`은 두 빌드에서 surface의 요청 URL을 기록한다. HTML 원격 전송은 이 값과 source 세대를 사용하며, 생성·복원 snapshot이나 GUI commit 관측값과 구분한다. native 렌더링과 스크립트 조작은 여전히 GUI 전용이다. [HTML 전송 계약](html-transfer.md)을 참고한다.

## 조회와 개별 플러그인 실행

플러그인 조회는 매니저 메타데이터와 디스크 목록만 읽는다. 설치 파일 복사, 권한 부여, 프로세스 기동을 조회의 부수효과로 실행하지 않는다. 설치된 항목이 없는 상태와 매니저를 사용할 수 없는 오류를 구분한다. GUI와 헤드리스는 읽기 메서드의 표와 핸들러를 공유하며 등록된 표에서 구현을 찾지 못하면 내부 오류로 보고한다. 매니페스트를 직접 읽는 방식은 더 얇은 조회 계층이 실제로 필요해질 때 검토한다.

헤드리스도 명시된 호스트 대상 ID가 존재하는지 핸들러 실행 전에 확인한다. 검사는 호스트 예약 namespace에만 적용하고 GUI와 같은 대상 판정 함수를 쓴다. 플러그인 namespace의 ID 공간까지 호스트가 가정하지 않는다. 아직 설치되지 않은 플러그인 목록을 미리 읽어 전체 요청을 분류하는 방식은 깨끗한 홈에서 성립하지 않는다. 예약 namespace의 범위가 바뀌거나 헤드리스에 여러 engine이 생기면 대상 라우팅을 다시 검토한다.

헤드리스의 `plugin.enable`과 `plugin.disable`은 GUI와 공용 핸들러를 사용한다. metadata 초기화 후 지정한 플러그인만 켜며 전체 플러그인 기동 함수를 사용하지 않는다. 헤드리스는 토글이 만드는 `PluginEnableToggled`와 `PluginUnloaded`만 처리한다. 다른 설치·삭제·권한 변경 기능은 이 지원에 포함되지 않는다. 새 이벤트가 생기면 소비자를 먼저 정한다. 창이 없는 GUI에서는 토글 이벤트가 발행되지 않지만 헤드리스에서는 발행되는 차이가 있다. 구독 플러그인의 실제 사용에서 이 차이가 문제가 되면 양쪽을 비교해 수정 범위를 정한다.

## memory writer 응답

헤드리스의 `memory.*` · `memory.secret.*` · `surface.meta.*` · `approval.*` · `agent.barrier_*` · `agent.semaphore_*` · `agent.lease_*` · `agent.rate_limit_*` · `agent.task_*` · `agent.task_graph_*` · `agent.dag_*` · `agent.report_append` · `agent.task_submit` · telemetry 조회와 `telemetry.cap.*` · `plugin.audit_query` · `plugin.audit_summary` · `session.*` · `plugin.list_agent_permissions` · `surface.commands` · `surface.last_command` · `surface.command_at` 응답도 GUI와 같다. 핸들러가 memory writer에 요청을 넣고, headless 루프가 회차마다 결과를 거두어 IPC 연결이나 plugin 호출에 응답한다. writer가 끝나면 journal 깨우기 이벤트로 루프를 깨운다. 종료 때는 writer를 join하며 남은 응답을 보낸다. writer를 거치며 생기는 경우의 응답은 다음과 같다([저장 계약](../design/systems/storage-contract.md#ipc-응답)).

| 경우 | 코드 | 메시지 | `error.data` |
|---|---|---|---|
| 큐 포화(접수 거절, 다시 시도할 수 있다) | `-32603` | `memory writer queue is full (<요청 수> requests, <바이트> bytes held)` | `{"storage_failure":"busy","writer":"saturated"}` |
| 종료 중 접수 거절 | `-32603` | `memory writer is not accepting requests` | `{"storage_failure":"other","writer":"closed"}` |
| commit 결과 불명(원인 오류 있음) | 원인 오류의 코드 | 원인 오류의 기존 문구(`memory db error: …`) | 원인 오류의 `data`에 `"outcome":"unknown"`을 더한다 |
| worker 소실로 결과 불명 | `-32603` | `memory writer ended before the request finished; it may have been stored` | `{"storage_failure":"other","outcome":"unknown"}` |
| 요청 본문 panic | `-32603` | `memory request panicked` | `{"storage_failure":"other"}` |
| 저장 실패·busy 재시도 시한 초과·요청 거절 | 기존 매핑 그대로 | 기존 매핑 그대로 | 저장 실패만 `storage_failure` |

## 라우팅 구조

gui 는 5-step 라우터(`src/app/ipc.rs`)를 쓴다. 헤드리스 pump(`src/boot/headless_dispatch.rs`)
는 caller 해석 → **권한 경계** → engine handler 직결로 간소화하되, **`App` 층 상태를 읽어야만
답할 수 있는 것**만 그 앞에서 가로챈다. 현재 가로채는 것은 다섯이다.

- `timer.list` — `App` 의 TimerHub 를 읽는다.
- 읽기 전용 `plugin.*` 조회 — `App.plugin_manager` 를 읽는다.
- `plugin.enable` / `plugin.disable` — `App.plugin_manager` 를 쓴다(아래 "수명주기 토글").
- `plugin.request_permission` — `state`·`engine` 만 읽는다(아래 "권한 경계").
- `events.fetch` — `App.plugin_manager` 가 소유한 이벤트 버스의 링을 읽는다.

### 권한 경계는 가로채기보다 **앞**이다

caller 인증 뒤 공통 요청 게이트(`handler/gate.rs`)가 권한·cap·rate-limit을 검사하고 허용된 요청의 사용량을
한 번 집계한다. App 인터셉트·plugin namespace·일반 handler 모두 그 뒤에 온다.
통과한 `CheckedRequest`를 일반 handler에 전달하므로 예산을 이중으로 차감하거나 사용량을 중복 집계하지 않는다.
GUI 외부 IPC와 plugin host-call도 같은 경계를 사용한다.
[플러그인 권한 모델](plugin-permissions.md).

권한 부족의 Agent 거부는 기존 capability elevation을 한 번 발행하고
`error.data`에 approval_id·permission·method를 싣는다. 거부된 요청은 실행하지 않고 허용된 요청으로 집계하지 않는다. Local 및 복구 메서드의 기존 예외는 유지한다.

그리고 engine handler 앞에 판정이 하나 더 있다 — **요청이 지목한 대상을 이 engine 이
가졌는가.** 헤드리스는 engine 이 하나라 라우팅할 곳이 없지만, 그 판정이 없으면 대상을
잘못 적은 요청이 그대로 실행된다(핸들러가 그 키를 안 읽으면 성공까지 돌아온다). gui 와
같은 코드를 쓰고, **호스트 예약 prefix 에 한정한다** — 예약되지 않은 prefix 는 plugin 이
답할 수 있어서 제외하면 plugin으로 전달할 요청까지 거절된다. 근거는
이 문서.

읽기 전용 plugin 조회는 GUI와 헤드리스 모두
`crate::adapters::ipc::handler::plugin::READONLY_METHODS`와 `dispatch_readonly`를 쓴다.
메서드 목록이나 처리 함수를 두 빌드에 따로 복제하지 않는다.

### 곁 — 등록된 kind 조회는 `plugin.*` 이 아니다

"어떤 surface kind 가 등록됐는가" 는 `surface.kinds` 가 답한다. 그 핸들러는 `CoreState`
하나만 읽어 **공용 engine handler** 에 있고, 헤드리스 pump 는 가로채지 않고 그대로
통과시킨다 — 즉 이 메서드는 위 두 가로채기 어디에도 등재되지 않는데 헤드리스에서 답한다.
그것이 이 문서가 가르는 축(창이 필요한가)에서 옳은 자리다: registry 는 `App` 도 창도
아니고 engine 의 것이다.

`register_one_surface_kind`(`boot/headless_plugins.rs`)는 webview·remote·egui-mesh 선언을
모두 등록한다. markdown mirror는 서버의 픽셀이 아닌 원문을 client에서 그리므로,
서버에 창이 없어도 해당 kind의 surface를 가질 수 있다
([원격 attach (Remote attach)](../features/remote-attach/index.md)).
같은 plugin이 실행 중이면 두 빌드의 kind 집합도 같다.

등록 시점은 다르다. GUI는 첫 창을 만들 때 `discover_and_start`를 호출하고
(`app/window_lifecycle.rs`), 헤드리스는 필요한 plugin만 시작한다. 2026-09-09의
격리 홈 측정에서는 부팅 직후 내장 kind 넷(`dag_graph`·`empty`·`explorer`·`terminal`),
markdown 활성화 뒤 다섯, 번들 9개가 실행된 뒤 여덟(`html`·`image`·`mesh_demo` 추가)이었다.
이 측정에서 네 plugin kind는 `registered: true`였고 `effective_rendering`도 선언과 같았다.

`tab.create`·`pane.split`·`workspace.create`가 plugin kind를 요청하면
`headless_plugins::ensure_plugin_for_surface_kind`가 매니페스트와 `plugins.toml`을 확인하고
활성화된 소유 plugin 하나만 시작한다. 시작 전에 그 plugin의 번역 namespace를 등록하므로 새 탭의 기본 이름(`surface.kind.html` → `HTML`)은 attach 전에도 번역된 값으로 저장된다([i18n](i18n.md#plugin-네임스페이스)). 없는 kind나 비활성화된 plugin의 kind는
기동 대기 없이 `unknown surface kind`로 거절한다. 헤드리스 메인 루프가 하나이므로
시작할 수 없는 plugin을 기다리면 다른 IPC 요청도 지연된다.
namespace forward도 소유 plugin만 준비하되, 매칭 IPC hook이 있으면 해당 active
extension을 함께 준비한다([플러그인 제작 가이드](plugin-development.md)).

plugin이 host call로 다른 plugin의 namespace 메서드를 부르면(예: 한 plugin이 다른 plugin의
수신처 조회를 요청) 헤드리스도 GUI와 같은 규칙으로 소유자에게 전달한다. 두 런타임 모두
공통 게이트를 통과한 뒤 매니저의 `forward_plugin_call_to_other_namespace`를 부르고,
자기 namespace·소유자 없는 메서드·호스트가 직접 처리하는 메서드(GUI의 `image.open` 변환)는
호스트 처리기로 넘긴다. 전달할 때 소유자의 실행 준비와 `ipc.invoke:<prefix>` 권한 확인을 거친다.
params는 바꾸지 않고, 소유자의 답은 호출자에게 같은 call id의 `ipc.result`로 돌아간다.
이 전달이 없으면 호출이 호스트 처리기로 가서 `-32601`로 끝난다. 시험은 매니저의 전달 판정과
헤드리스 dispatch의 왕복을 stub plugin으로 잰다.

## `plugin.*` — 19 개 메서드의 판정

### 답한다 — 읽기 (7)

`App.plugin_manager` 또는 `Core` 만 읽으면 답이 정해지는 것들이다. 창과 무관하다.

| 메서드 | 읽는 것 |
|--------|---------|
| `plugin.list` | `plugin_manager.packages` |
| `plugin.show` | `plugin_manager.packages` + config + `EngineRuntime.surface_registry`(선언한 kind 가 등록됐는지) |
| `plugin.permissions` | `plugin_manager` config |
| `plugin.extension.list` | `plugin_manager.extensions` |
| `plugin.audit_query` | `Core` 의 audit store |
| `plugin.audit_summary` | `Core` 의 audit store |
| `plugin.list_agent_permissions` | `Core` 의 세션 권한 |

### 답한다 — 수명주기 토글 (2)

`plugin.enable` · `plugin.disable`

읽는 것도 쓰는 것도 `App.plugin_manager` 하나다. 창을 안 본다.

두 라우터가 **같은 함수**를 부른다 — `handler::plugin::dispatch_lifecycle_toggle`
(`src/adapters/ipc/handler/plugin.rs`). 파라미터 이름·응답 칸 이름·오류 문구는
에이전트가 보는 계약이라 한 자리에만 둔다. 이 함수는 메서드 분류·파라미터 decode·오류 코드
매핑만 하고, 매니저 변경과 surface kind 철회는 `src/app/plugin_lifecycle.rs`의 실행 함수가
맡는다. gui 의 `App::plugin_enable` / `App::plugin_disable` 도 같은 실행 함수의 얇은 위임이다.

**갈리는 것은 낸 이벤트의 소비처 하나다.** gui 는 첫 main window 의 `PendingHostEvent`
큐에 넣어 `app/dispatch/host_events.rs` 가 다음 drain 에 event bus 로 보내고, 창이 하나도
없으면 **아무것도 발행하지 않는다**. 헤드리스는 창이 없어 그 큐를 못 쓰므로
`cascade_toggle_events_headless` 가 매니저를 직접 들고 그 자리에서 낸다. 발행하는 이벤트
키와 payload 자체(`plugin.enabled` / `plugin.disabled` / `plugin.unloaded`)는 두 경로가
`src/app/plugin_events.rs`의 같은 함수를 부른다. hook 이벤트 등록 해제도 gui 의 `cascade_plugin_unloaded` 와 같다.

근거·대안·재검토 조건은 이 문서.

**매니저는 메타데이터 층까지만 세운다** (`ensure_plugin_manager_metadata`). 번들 설치는
부팅이 이미 했고(`src/boot.rs`), `PluginManager::enable` 은 그 package 표에서 **지목한
하나만** 찾아 기동한다. 여기서 `ensure_plugin_manager`(= `discover_and_start`)를 부르면
하나를 켜라는 명령이 설치된 전부를 띄운다 — 그것이 이 갈래를 나눈 이유다.

### 아직 없다 — 쓰기이지만 창은 필요 없다 (3)

이 세 메서드는 `Core`만 있으면 실행할 수 있지만, 헤드리스에서 제공하지 않는다.
감사 로그 삭제와 에이전트 권한 변경은 조회와 달리 상태를 바꾸므로,
어떤 호출자에게 허용할지 별도로 결정해야 한다.

`plugin.audit_clear` · `plugin.grant_agent_permission` · `plugin.revoke_agent_permission`

<a id="아직-없다--app-이분이-선행이다-6"></a>

### 아직 없다 — 별도 lifecycle과 권한 계약이 필요한 기능 (6)

plugin의 GUI lifecycle 통지는 `src/app/dispatch_domain.rs`가 처리하며 headless 일반 plugin bus는 제공하지 않는다. enable·disable은 지정 plugin의 실행 경계로 처리한다. 설치·삭제·권한 변경을 headless에 추가하는 일은 각각의 공개 권한·오류·완료 계약과 이 문서에 따라 판단한다.

`plugin.install` · `plugin.remove` · `plugin.grant` · `plugin.revoke` ·
`plugin.upgrade_builtins` · `plugin.audit_follow`

`plugin.audit_follow`는 `Core`의 데이터를 읽는 스트리밍 API다. 헤드리스에서 지원하려면
구독을 언제 종료할지도 위 결정과 함께 정해야 한다.

### 창이 없어도 답이 정의되는 것 (1)

`plugin.request_permission`은 approval 레코드를 만든다. GUI에서는 창이 있을 때
승인 popup을 함께 표시하고, 헤드리스에서는 `approval.await`·`approval.list`·
`approval.respond`로 레코드를 처리한다. `publish_capability_elevation`의 popup 표시만
GUI 조건에 따른다. 창이 없다는 이유로 권한 요청 자체를 막지 않는다.

## 조회는 plugin 을 기동하지 않는다

헤드리스 데몬은 attach 세션이 없으면 plugin 을 하나도 띄우지 않는 것이 기본값이다. 그래서
`plugin.list` 에 답하려면 매니저를 세워야 하는데, 설치·기동까지 함께 실행하면 조회가 상태를 변경하게 된다. `src/boot/headless_plugins.rs` 는 그래서 둘로 갈라져 있다.

| 함수 | 하는 일 | 조회가 부르는가 |
|------|---------|-----------------|
| `ensure_plugin_manager_metadata` | 매니저 생성 + `refresh_packages`(디스크 스캔) | 예 |
| `ensure_plugin_manager` | 위 + `install_builtins_if_needed` + `discover_and_start` | 아니오 |

경계가 `install_builtins_if_needed` **위**인 것이 중요하다. 그 함수는 번들에서 파일을
복사하고 매니페스트 권한을 `plugins.toml` 에 자동 grant 한다 — 관측 대상을 읽는 작업이 아니라 새 설치를 만드는 작업이다. 프로세스를 띄우는 것보다 앞서 배제된다.

그 결과 아무것도 설치되지 않은 홈에서는 목록이 빌 수 있다. 그것은 거짓이 아니라 그 시점의
사실이며, 매니저가 아예 없을 때의 응답과 구분된다.

| 응답 | 뜻 |
|------|-----|
| `-32000 plugin manager not initialized` | 매니저를 세우지 못했다(예: waker factory 부재) |
| `{"plugins": []}` | 매니저는 있고, 디스크에 설치된 plugin 이 없다 |

`Option<&PluginManager>`를 받는 네 핸들러는 `None`에 같은 오류를 반환한다.
`src/adapters/ipc/handler/plugin.rs`의 단위 테스트가 이를 함께 비교한다.

## app 층 메서드 — 무엇이 답하고 무엇이 왜 없는가

다음 표는 GUI의 `src/app/ipc/app_methods.rs`에 있는 메서드와 헤드리스의 지원 차이다.
`src/source_guards/headless_app_layer_coverage.rs`가 표와 소스를 대조한다.
검사 통과는 두 빌드가 같은 기능을 제공한다는 뜻이 아니라, 차이가 빠짐없이 사유와 함께
기록됐다는 뜻이다.

### 답한다 (7)

창이 없어도 답이 정의되는 것들이다. 본체는 두 조합이 **같은 함수**를 쓴다
(`src/app/services/surface.rs`, 승인·태스크 대기는 대기 본문 옆인 `handler/approval/read.rs` ·
`handler/agent/task.rs`) — `system.shutdown` 만 끊는 방식이 조합마다 달라 예외다.

| 메서드 | 읽는 것 / 하는 일 |
|--------|-------------------|
| `timer.list` | `App` 의 TimerHub — 무엇이 인스턴스를 깨우는가 |
| `clipboard.set_text` | `Core` 의 클립보드 포트. 없는 환경이면 포트가 실패를 돌려주고 그것이 사실이다 |
| `remote.workspaces` | 인자만 읽는다. App 상태를 하나도 안 본다 |
| `agent.task_await` | 이 engine 의 `TaskScope` 가 가진 완료 대기 허브 + task 순번 |
| `approval.await` | 이 engine 의 `approval_store` |
| `events.fetch` | `App.plugin_manager` 의 이벤트 버스 링. `wait_ms` 대기는 워커로 나가 dispatch 루프를 안 막는다. **이벤트를 넣는 경로도 필요하다** — `agent` 사건 큐를 버스로 옮기는 드레인이 데몬 루프(`run_headless`)의 맨 위에도 있어야 하고, 두 조합이 같은 함수를 쓴다(`app::agent_events`) |
| `system.shutdown` | 데몬을 멈춘다(debug 전용). 응답을 먼저 보내고 run loop 를 끊는다 |

### 없는 것이 정답 (12)

이 메서드들은 `App.view`를 사용하지만, 헤드리스에는 그 필드가 없다
(`src/app.rs`에서 `gui`로 제한). 아래 표는 각 메서드가 창이나 GUI 상태를 필요로 하는
이유다. 앞서 다룬, 창이 없어도 구현할 수 있지만 아직 지원하지 않는 메서드와 구분한다.

| 메서드 | 왜 |
|--------|-----|
| `window.create` / `view.create` | winit 이벤트루프에 창 생성을 맡긴다. 헤드리스엔 그 루프가 없다 |
| `window.close` / `view.close` | `App.view.views` 에서 창을 닫는다. 그 레지스트리가 없다 |
| `window.focus` / `view.focus` | 포커스 전환이라 애초에 debug 격리(docs/dev-guide/plugin-permissions.md)이고, 대상도 창이다 |
| `window.list` / `view.list` | 빈 목록이 아니라 **개념이 없다** — `[]` 를 주면 "창이 0 개인 GUI" 로 읽혀 호출자가 `window.create` 를 시도한다 |
| `ui.screenshot` | 창의 화면을 캡처해 파일로 쓴다. 캡처할 창이 필요하다 |
| `remote.attach` | mirror workspace 를 띄울 창이 필요하다 |
| `remote.refusals` | 거절을 기록하는 `remote.attach` 와 자동 attach 가 GUI 에만 있다 |
| `remote.disconnect` | 떼어 낼 mirror 를 만드는 `remote.attach` 와 attach 세션이 GUI 에만 있다 |
| `system.gpu_stats` | 창마다의 GpuState 와 wgpu 전역 리포트를 센다. GPU 컨텍스트가 없다 |

`attach.into_gui` 는 위 표의 메서드와 달리 창이 아니라 mirror 큐의 소비자가 GUI 에만 있다. GUI 는 app 층이 가로채 다른 포트를 거쳐 자기 자신에 닿는지 워커에서 확인한 뒤 응답한다. 헤드리스에는 큐를 비워 mirror 를 만드는 쪽이 없으므로 engine 처리기에 arm 을 두지 않는다. 접수하면 처리하지 않을 요청을 성공처럼 답하게 되므로 자기 포트를 포함한 모든 요청을 `-32017`(메서드 이름 포함)로 거절한다. plugin host-call 도 같은 engine 처리기에 닿아 같은 거절을 받는다. 소스 검사의 `NOT_IN_HEADLESS` 에는 이 사유로 등록돼 있다. 아래 "census 뒤에 게이트된 arm" 표에도 같은 거절 모양으로 적는다.

`plugin.*`의 현재 분류는 위 "`plugin.*` — 19 개 메서드의 판정" 절을 따른다.

<a id="dispatch-arm-이-gui-로-게이트된-표면"></a>

## GUI 빌드에서만 라우팅하는 메서드

위 절이 다루는 `app_methods` step 과 **다른 축**이다. 이쪽은 `src/adapters/ipc/handler.rs`
의 dispatch arm 이 `#[cfg(feature = "gui")]` 인 경우와, gui 라우터의 debug step
(`src/app/ipc/debug_methods.rs`)에만 있는 경우 둘이다.

아래에는 2026-09-05에 `METHOD_TABLE` 276개와 `DEBUG_METHODS` 50개, 총 326개를
GUI와 헤드리스에서 같은 인자로 호출한 조사와 이후 확인한 예외를 정리한다. 당시 두 표는
겹치지 않았다. 이 조사 수치를 현재 전체 메서드 수로 사용하지 않는다.

등록된 이름이 현재 빌드에 구현되지 않았을 때의 코드는 `-32017`, 알 수 없는 이름은
`-32601`이다([CLI / IPC API 규약 — 명명 + 안정성](api-conventions.md)).
구 조사에서는 두 경우를 `-32601`로 응답했으므로 과거 로그를 읽을 때 구분한다.

### 갈리는 축이 조합 하나가 아니다 — 플랫폼도 같은 자리에서 자른다

이 절의 census 는 `-32017`(이 조합에 arm 이 없다)과 `-32601`(이름이 틀렸다) 둘로만 갈린다.
그런데 dispatch 층에는 **세 번째 코드**가 있고, 그 코드가 붙는 이름은 이 문서의 어느 표에도
없었다. 헤드리스 실측(2026-09-07, linux):

| 메서드 | 응답 |
|--------|------|
| `surface.raw_key` | `-32015 input reproduction over the OS event stream is macOS-only …` |
| `surface.switch_input_source` | `-32015` (같은 문구) |

`-32015` 는 "이 플랫폼에서 안 된다" 이고, 호출자를 **조합이 아니라 플랫폼을 보는 쪽**으로
보낸다(코드 넷의 구분은 `crates/tasty-ipc/src/protocol.rs` 의 표와
[CLI / IPC API 규약 — 명명 + 안정성](api-conventions.md)).

두 메서드의 실제 조건은 `#[cfg(all(target_os = "macos", feature = "gui"))]`다.
따라서 Linux 헤드리스의 `-32015`만으로 OS와 GUI 중 어느 조건 때문인지 나눌 수 없다.
macOS GUI·헤드리스 비교 측정은 없으므로 아래 집계에 포함하지 않는다.

다음은 기존 조사 이후 plugin 지원 변경을 반영한 집계 기록이다. 현재 지원 여부는
각 메서드 표를 기준으로 확인한다.

| 부류 | 건수 | 어디서 판정하나 |
|------|------|-----------------|
| 창 축(`window.*` · `view.*` · `ui.screenshot` · `remote.attach` · `remote.refusals` · `system.gpu_stats`) | 12 | 위 "app 층 메서드" 절 |
| `plugin.*` | 9 | 위 "`plugin.*` — 19 개 메서드의 판정" 절 (census 시점 12 → 그 뒤 셋이 열렸다) |
| `debug.*` | 36 | 이 절 |
| 그 밖 | 6 | 이 절 |
| **합** | **62** | |

### 분류 미정 (그 밖 2)

image 요청은 host arm까지 전달되지만 헤드리스 구현이 없어 거절된다.
창이 반드시 필요한 기능인지, GUI 구현과 분리하면 지원할 수 있는지는 결정되지 않았다.

| 메서드 | 헤드리스 실측 | 분류 |
|--------|---------------|------|
| `image.open` | `-32017`(감싸짐) | **미정** — 핸들러가 `ConvertSurface` 를 발행하고 `image` kind 는 헤드리스에서도 **등록된다**(egui-mesh). 창이 없어서인지 경계가 안 열려서인지 재지 않았다 |
| `image.list` | `-32017`(감싸짐) | **미정** — surface 순회 조회다. 위와 같은 물음이 걸린다 |

이 표의 "미정"은 지원 여부가 아니라 향후 헤드리스 지원 가능성의 분류다.

### 답한다

| 메서드 | 읽는 것 |
|--------|---------|
| `theme.query` | 전역 Theme + `EngineRuntime.settings` 뿐이다. 창도 surface 도 렌더러도 안 본다 |
| `settings.get_keybinding` | `EngineRuntime.settings.keybindings` 의 한 동작 바인딩뿐이다 |

`theme.query`는 `src/adapters/ipc/handler/theme.rs`, `settings.get_keybinding`은 `src/adapters/ipc/handler/keybinding.rs`에서 두 빌드가 같은 함수를 사용한다.
전역 Theme와 설정만 읽으므로 창이 필요하지 않다.

### 아직 없다 — `App` 이분이 선행이다 (그 밖 1)

| 메서드 | 왜 |
|--------|-----|
| `markdown.navigate` | host arm(`src/adapters/ipc/handler.rs` 의 `"markdown.navigate" =>`)이 `#[cfg(feature = "gui")]` 다 — `file_picker.trigger` 와 같은 구성이다. 핸들러 자체(`handler/markdown.rs::handle_navigate`)는 `CoreState`(mirror 여부)와 `IpcWindow`(팝업의 사용자 입력 기록)만 읽고 `ConvertSurface` intent 를 발행할 뿐 `App.view` 를 안 본다. 이 문서의 역할 분리에 따라 GUI와 core의 책임을 나누면 headless에서 이 host arm을 제공할 여지가 있다. 그래서 이것은 창이 없어서가 아니라 **경계가 아직 안 열려서** 없는 것이다 |

헤드리스에서 부르면 응답이 한 겹 감싸여 온다 — `-32017 host call 'call#N' failed: method
'markdown.navigate' is registered but this binary has no dispatch arm for it`. 번들
markdown plugin 이 그 namespace 를 점유해 host 로 되돌리기 때문이고, 그 모양의 근거도
같은 ADR 이다.

### 없는 것이 정답 (그 밖 2)

| 메서드 | 왜 |
|--------|-----|
| `file_picker.trigger` | 창 안 popup(`state.dialogs.file_picker`)을 연다. 그릴 창이 없다 |

### census 뒤에 게이트된 arm

위 표의 건수는 census 시점 값이라 그 뒤에 `gui` 로 게이트된 arm 은 거기 안 들어간다. 이름만 여기 세운다.

| 메서드 | 왜 |
|--------|-----|
| `file_handler.dispatch` | 요청을 적용할 identify worker 와 결과를 여는 창이 gui 에만 있다. arm 이 헤드리스에 있던 동안은 `{"accepted": true}` 로 답하고 요청을 버렸다 — `git_viewer.query` 와 같은 모양이다. 근거 [파일 핸들러 (File handler)](../features/file-handler/index.md). 같은 namespace 의 `file_handler.reload` · `file_handler.detectors` 는 헤드리스에서도 답한다 |
| `git_viewer.query` · `markdown_mirror.content_request` | 요청을 큐에 넣고 `request_id` 만 답한 뒤 결과를 attach 채널로 받아 오는 비동기 accept 다. 큐를 비워 보내는 쪽이 gui 의 `about_to_wait` 에만 있어, arm 이 헤드리스에 있던 동안은 수락해 놓고 결과가 영영 안 왔다. `-32017` 문구가 메서드 이름을 실어 두 거절이 갈린다. 같은 줄의 셋째 forward(mirror 구조 op)는 메서드가 아니라 대상이 mirror 인지로 갈려 arm 을 못 뺀다 — headless의 명시 mirror 대상 해소가 거절한다(이 문서). 시험 `tests/e2e_tests.rs` 의 `mirror_forward_requests_are_refused_by_name_in_a_headless_daemon` |
| `attach.into_gui` | 접수한 요청을 비워 mirror 를 만드는 쪽이 gui 의 `about_to_wait` 에만 있다. arm 이 헤드리스에 있던 동안은 자기 포트만 거절하고 `{queued:true}` 로 답한 뒤 요청을 처리하지 않았다. 지금은 engine 처리기에 arm 이 없어 자기 포트를 포함해 `-32017` 로 거절한다. GUI 는 app 층이 먼저 답하므로 이 arm 에 닿지 않는다. 시험 `tests/attach_silent_disconnect.rs` 의 `a_headless_daemon_refuses_gui_attach_without_taking_occupancy` |
| `surface.html_back` · `surface.html_forward` · `surface.html_reload` | 이동을 실행하는 것이 gui 전용인 네이티브 WebView와 그것을 매 프레임 동기화하는 창이다. 헤드리스에는 html surface의 WebView가 없어 큐를 소비할 쪽이 없다 |
| `surface.html_nav` | 읽는 주소와 히스토리 정보가 gui 전용인 네이티브 WebView에서 프레임마다 옮겨진다. 헤드리스에는 html surface의 WebView가 없어 답할 상태가 없다 |
| `surface.html_script_allow` · `surface.html_remote_allow` | 허용 뒤 재로드를 실행하는 것이 gui 전용인 네이티브 WebView와 그것을 매 프레임 동기화하는 창이다. 헤드리스에는 html surface의 WebView가 없어 허용할 문서가 없다 |
| `surface.html_script` | 읽는 스크립트 상태가 gui 전용인 네이티브 WebView의 탐색 콜백에서만 채워진다. 헤드리스에는 html surface의 WebView가 없어 답할 상태가 없다 |

### `debug.*` 36 건

debug API는 에이전트가 팝업 표시, 훅 실행, 이벤트 버스 구독 등을 검증할 때 쓴다.
창 없이 확인할 수 있는 동작은 헤드리스 debug 빌드에서도 조회할 수 있어야 한다.
이는 release에 API를 제공한다는 뜻이 아니다. `DEBUG_METHODS`는 release에서 비어 있으며,
release 헤드리스 실행에서도 아래 다섯 메서드가 모두 `-32601`을 반환하는지 확인한다.

모수는 **호출마다 새 인스턴스를 띄우는** census 로 쟀다(2026-09-05). 한 인스턴스에서
순차로 부르면 앞쪽의 파괴적 호출이 뒤쪽 호출의 라우팅 대상을 없애고, 그러면 멀쩡한
메서드가 `Method not found` 로 보인다. 36 = 답한다 7 + 없는 것이 정답 29, 애매한 것 0.

#### 답한다 (7)

읽는 것이 `App` 의 `lua_engine` / `plugin_manager`, 그리고 gui 무관 정적 표뿐이다. 창·렌더러·egui 입력 큐를 하나도
안 본다. 자리가 없어서 사라졌던 것이라 헤드리스 pump 에 자리를 만들었고, 본체는 두 조합이
**같은 함수**를 쓴다(`src/app/services/surface_debug.rs` · `handler/debug_plugin.rs` ·
`handler/popup.rs`).

메서드별로 지원 여부를 판단한다. `debug.popup.list`는 조회만 하므로 지원하지만,
`open`과 `close`는 아래 표처럼 서로 다른 제약이 있다.

| 메서드 | 읽는 것 |
|--------|---------|
| `debug.lua.eval` | `App.lua_engine` 워커에 스크립트를 던진다(fire-and-forget) |
| `debug.event_bus.list_subscribers` | `plugin_manager` 의 event bus 구독자 |
| `debug.event_bus.publish` | 같은 bus 에 이벤트를 넣는다 |
| `debug.event_bus.trace` | 같은 bus 의 trace |
| `debug.extension.invoke_hook` | `plugin_manager` 의 확장 훅을 수동 실행 |
| `debug.popup.list` | `plugin_manager` 의 popup contribute 목록과 열린 인스턴스. **조회만이다** — 같은 갈래의 `open`/`close` 는 아래 표에 있다 |
| `debug.fullscreen.list` | `src/fullscreen_stages.rs` 의 gui 무관 무대 메타(id·제목 키). **조회만이다** — 같은 갈래의 `open`/`close`/`state` 는 창을 지목해야 해서 아래 표에 있다 |

event bus 두 건은 매니저를 **메타데이터 층까지만** 세운다 — 조회가 plugin 프로세스를
띄우면 관측이 자기 대상을 바꾼다(이 문서).
그래서 아무 plugin 도 안 뜬 데몬에서는 구독자가 0 으로 나오고, 그것이 그 시점의 사실이다.

#### 없는 것이 정답 (34)

| 메서드 | 왜 |
|--------|-----|
| `debug.info` · `debug.focused_surface` · `debug.surface_rect` · `debug.selection` · `debug.pending_menu` · `debug.pending_move` | 창 하나의 렌더 상태(셀 크기·포커스·화면 배치·선택·대기 중 native 메뉴·이동 대기 슬롯)를 읽는다 |
| `debug.window.list` | 메인 창과 보조 창을 `self.view.views` 에서 나열한다. 창이 없다 |
| `debug.native_menu.answer` | 창이 여는 native 메뉴의 응답이다. 메뉴를 여는 창이 없다 |
| `debug.settings.open` | `AppEvent::OpenSettings` 를 winit proxy 로 보낸다. 헤드리스엔 proxy 가 없다 |
| `debug.settings.close` | 활성 설정 모달을 `self.view.views` 에서 지운다. 창이 없다 |
| `debug.gpu.stall` | 렌더 스레드를 일부러 막아 stall 워치독을 시험한다. 막을 스레드가 없다 |
| `debug.banner.*` (4) · `debug.host_popup.*` (3) · `debug.modifier_hint.*` (2) | host 위젯의 표시 상태다. 그릴 창이 없으면 상태 자체가 없다 |
| `debug.tool.list` · `debug.tool.invoke` | 도구 메뉴는 창의 위젯이다 |
| `debug.fullscreen.open` · `close` · `state` (3) | 무대는 **창 단위**라 `pick_debug_window` 로 `self.view.views` 에서 창을 지목한다 |
| `debug.plugin_banner.*` (2) | 소유 view 의 BannerManager 와 host 매니저를 함께 다룬다 — `open` 도 `close` 도 `self.view.views` 를 순회한다. 두 메서드 모두 같은 창 의존성이 있다 |
| `debug.inject_mouse` · `debug.inject_key` · `debug.inject_window_mouse` · `debug.inject_egui_mouse` · `debug.inject_egui_key` · `debug.inject_egui_text` (6) | 사용자 입력 재현이다. 앞 둘은 대상 surface 의 PTY 로, 뒤 넷은 winit·egui 입력 큐로 들어간다 — 그 큐가 창에 딸려 있다 |
| `debug.popup.open` | 매니저만 읽어 **답은 정의된다.** 그런데 헤드리스에는 그 인스턴스를 **닫는 경로가 하나도 없다** — debug close 도, plugin 자신의 release `popup.close` 도 gui 게이트 안의 `app::dispatch` 에 산다. 여는 것만 열면 그 빌드에서 닫을 수 없는 상태가 남는다 |
| `debug.popup.close` | 렌더가 수집하는 close 큐로 합류해야 `cancel_child_file_picker` 연쇄 정리가 돈다([내부 팝업 시스템](../design/systems/popup.md)). 그 glue 가 gui 게이트 안이다 |

`src/source_guards/headless_app_layer_coverage.rs` 가 이 표와 두 라우터의 정합을 강제한다 —
app 층 step 과 debug step 두 쌍을 같은 규약으로 본다.

헤드리스 쪽에서 검사 범위는 dispatch 함수 명부다(`pump_ipc` ·
`intercept_app_layer` · `intercept_debug_app_layer`) — 파일 전체가 아니다. 답하지 않는
헬퍼가 이름을 문자열로 들면 파일 전체를 검사하면 그것까지 답으로 세기 때문이다. 명부 밖에 답하는 함수가 추가되는 것을 놓치지 않도록, 같은 가드가
*테스트 전용 코드와 주석을 제외한 파일 전체*의 이름이 명부 합집합과 같은지를 따로 잰다
(목록 밖 사용 0개). 테스트 전용 코드를 먼저 제외하는 것은 `#[cfg(test)]` 아래의 픽스처가 이름을 인용했을 때
**테스트 전용 코드에 면제를 요구하는 잘못된 실패**가 나기 때문이다. 헤드리스
dispatch 에 새 헬퍼를 만들어 거기서 메서드 이름에 답하려면 **그 함수를 명부에 함께
등록한다** — 안 하면 그 가드가 그 자리에서 막는다.

### 이름은 리터럴로 적는다

위 가드는 dispatch 본문을 **텍스트로** 읽어 `"a.b"` 꼴 리터럴을 뽑는다. 그래서 이름이
리터럴이 아니면 — 매크로가 만들거나 상수와 맞대면 — 그 갈래는 표에도, 가드에도 안 보이고
답하지도 사유가 적혀 있지도 않은 메서드가 조용히 생긴다.

이 사각은 "몇 개 뽑혔나" 로 못 막는다. 이름 하나를 매크로 뒤로 숨기면 항목이 하나 줄 뿐이고,
매크로가 만든 이름으로 갈래를 더하면 항목 수는 아예 안 변한다 — 수량 하한만으로는 새 분기가 검사에서 빠지는 문제를 찾을 수 없다. 그래서 가드가 따로 재는 것은 **이름을 읽는 자리**다:
`request.method` 로 갈래를 칠 때 맞대는 값(`==` · `starts_with` · `match … as_str()` 의 팔)은
문자열 리터럴이어야 한다. 값을 위임 함수에 **넘기기만** 하는 자리는 대상이 아니다.

<a id="남은-표면"></a>

## 나머지 메서드의 지원 여부

`debug.*` 36 건의 판정은 위 "`debug.*` 36 건" 절에 있다.

`image.open`·`image.list`는 plugin namespace를 거쳐 host arm까지 전달되지만,
그 arm은 GUI 전용이라 헤드리스에서 `-32017`로 거절된다. 라우팅 도달 여부와 기능 지원
여부를 구분한다. 분류와 미결정 사항은 위 "분류 미정 (그 밖 2)" 표에 모았다.

## PTY 실행 종료

PTY drain은 output-match 훅과 함께 명시적인 process exit를 소비한다. GUI와 공유하는 host 처리에서 실행 구독 종료, process-exit 훅, no-snapshot surface close 및 soft 점유 정리를 수행한다. SessionEnd hook이 유실돼도 실제 PTY 종료가 수명 종료 근거가 된다. surface가 다른 window의 로컬 목록에 없다는 이유로 종료를 합성하지 않는다.
