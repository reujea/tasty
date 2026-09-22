# 헤드리스 IPC 표면 — 무엇이 답하고, 무엇이 왜 없는가

헤드리스는 **CLI 전용 실행 형태**다. [`docs/identity.md`](../identity.md) 원칙 2 는 에이전트
기능이 IPC + CLI 양면으로 동작할 것을 요구하므로, 헤드리스에서 메서드가 없다는 것은
성능이나 편의 문제가 아니라 **원칙의 문제**다. 다만 창이 없으면 답 자체가 정의되지 않는
메서드도 있다. 이 문서는 그 둘을 메서드별로 가른다 — "이 표면은 GUI 가 필요하다" 같은
뭉뚱그림을 두지 않는 것이 이 문서의 목적이다.

## 라우팅 구조

gui 는 5-step 라우터(`src/app/ipc.rs`)를 쓴다. 헤드리스 pump(`src/boot/headless_dispatch.rs`)
는 caller 해석 → **권한 경계** → engine handler 직결로 간소화하되, **`App` 층 상태를 읽어야만
답할 수 있는 것**만 그 앞에서 가로챈다. 현재 가로채는 것은 넷이다.

- `timer.list` — `App` 의 TimerHub 를 읽는다.
- 읽기 전용 `plugin.*` 조회 — `App.plugin_manager` 를 읽는다.
- `plugin.request_permission` — `state`·`engine` 만 읽는다(아래 "권한 경계").
- `events.fetch` — `App.plugin_manager` 가 소유한 사건 버스의 링을 읽는다.

### 권한 경계는 가로채기보다 **앞**이다

caller 인증 뒤 공통 `check_request`가 권한·cap·rate-limit을 검사하고 허용 호출을
한 번 관측한다. App 인터셉트·plugin namespace·일반 handler 모두 그 뒤에 온다.
통과한 `CheckedRequest`를 일반 handler에 전달하므로 예산과 관측을 중복 소비하지 않는다.
GUI 외부 IPC와 plugin host-call도 같은 경계를 사용한다.
[ADR-0277](../adr/0277-ipc-admission-and-observation-run-once.md).

권한 부족의 Agent 거부는 기존 capability elevation을 한 번 발행하고
`error.data`에 approval_id·permission·method를 싣는다. 거부된 요청 동작과 Allow 관측은
실행하지 않는다. Local 및 복구 메서드의 기존 예외는 유지한다.

그리고 engine handler 앞에 판정이 하나 더 있다 — **요청이 지목한 대상을 이 engine 이
가졌는가.** 헤드리스는 engine 이 하나라 라우팅할 곳이 없지만, 그 판정이 없으면 대상을
잘못 적은 요청이 그대로 실행된다(핸들러가 그 키를 안 읽으면 성공까지 돌아온다). gui 와
같은 코드를 쓰고, **호스트 예약 prefix 에 한정한다** — 예약되지 않은 prefix 는 plugin 이
답할 수 있어서 자르면 아래 forward 가 죽는다. 근거는
[ADR-0143](../adr/0143-a-named-target-is-checked-before-the-engine-in-headless.md).

두 가로채기 모두 **gui 와 같은 함수**를 부른다. 읽기 전용 plugin 조회의 라우팅 표는
`crate::adapters::ipc::handler::plugin::READONLY_METHODS` 하나뿐이고, gui 라우터도 헤드리스
pump 도 같은 `dispatch_readonly` 를 통과한다. 표를 두 벌로 두면 한쪽만 고쳐지는 순간
갈라지며, 이 저장소는 같은 실패형(같은 로직이 두 곳에 복제돼 서로 다르게 자란 것)을 이미
겪었다.

### 곁 — 등록된 kind 조회는 `plugin.*` 이 아니다

"어떤 surface kind 가 등록됐는가" 는 `surface.kinds` 가 답한다. 그 핸들러는 `CoreState`
하나만 읽어 **공용 engine handler** 에 있고, 헤드리스 pump 는 가로채지 않고 그대로
통과시킨다 — 즉 이 메서드는 위 두 가로채기 어디에도 등재되지 않는데 헤드리스에서 답한다.
그것이 이 문서가 가르는 축(창이 필요한가)에서 옳은 자리다: registry 는 `App` 도 창도
아니고 engine 의 것이다.

**한때 여기 "헤드리스에서 그 답이 GUI 와 다르다는 것이 이 조회의 값" 이라고 적혀
있었다 — 그 서술은 더 이상 사실이 아니다.** 그때 헤드리스는 `webview`/`remote` 선언을
등록하지 않았고, 그래서 같은 매니페스트가 조합에 따라 다른 kind 집합을 냈다. 그 차이는
[ADR-0255](../adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md) 로
근거가 사라졌다 — markdown mirror 가 나르는 것은 픽셀이 아니라 원문이고 그리는 것은
client 라, 서버는 창 없이도 그 kind 의 surface 를 가질 수 있다. 지금
`register_one_surface_kind`(`boot/headless_plugins.rs`)는 **세 rendering 을 전부**
등록하고, 그래서 이 조회의 답은 **같은 plugin 이 떠 있는 한 두 조합에서 같다.**

갈리는 것은 집합이 아니라 **언제 차는가**다. gui 는 첫 창을 만들 때 plugin 을 전부
띄우고(`app/window_lifecycle.rs` 의 `discover_and_start` — 코드 근거이며 이 회차에
gui 로 재지는 않았다), 헤드리스는 기동이 지연이라 아무 plugin 도 안 뜬 데몬에서는
host 내장 넷만 나온다. 실측(2026-09-09, 갓 만든 격리
홈 헤드리스 데몬): 부팅 직후 `dag_graph`·`empty`·`explorer`·`terminal` 넷 →
`plugin enable com.tasty.markdown` 뒤 `markdown` 이 더해져 다섯 → plugin 9 개가 전부
뜬 뒤 여덟(`html`·`image`·`mesh_demo` 가 더해진다). 그 시점의 `plugin.show` 는 넷 다
`registered: true` 이고 `effective_rendering` 이 선언과 같다.

**기동을 유발하는 자리가 하나 늘었다 — 그리고 그 자리는 하나만 띄운다.** plugin 이
선언한 kind 를 지목한 생성 요청(`tab.create`·`pane.split`·`workspace.create` 의 `type`)이
오면, 소속을 먼저 묻고 맞을 때만 **그 kind 의 소유자 하나**를 기동한다
(`headless_plugins::ensure_plugin_for_surface_kind`, namespace forward 와 같은 두 층 —
[ADR-0259](../adr/0259-a-kind-request-starts-the-owner-that-declares-it.md)).

소속은 **매니페스트 ∩ `plugins.toml`** 이다. 두 갈래가 거기서 끝난다(실측 2026-09-10,
갓 만든 격리 홈): 없는 이름(`--type nosuchkind`)은 0.09 s 에 예전과 같은
`unknown surface kind` 로 답하고, **`plugin disable` 한 plugin 의 kind** 도 같은 0.09 s
에 같은 답을 낸다 — 둘 다 `running` 이 그대로 0 이다. 뒤엣것을 안 거르면 그 요청이
영영 안 뜰 plugin 을 시한(5 초)까지 기다리고, 헤드리스 메인 루프가 하나라 **그동안
데몬 IPC 전체가 선다**(고치기 전 실측: 그 요청 5.34 s · 무관한 `list info` 5.04 s ·
덤으로 8 개 기동).

소속이 맞으면 그 하나만 뜬다 — `--type markdown` 첫 호출 0.14 s 뒤 `running` 이
`['com.tasty.markdown']` 이고, 이어서 `--type image` 를 부르면 둘이 된다. **namespace
forward도 owner만 준비한다**: namespace에 매칭 IPC hook이 있으면 해당 active extension을 함께 준비한다(ADR-0282).

## `plugin.*` — 19 개 메서드의 판정

### 답한다 — 읽기 (7)

`App.plugin_manager` 또는 `Core` 만 읽으면 답이 정해지는 것들이다. 창과 무관하다.

| 메서드 | 읽는 것 |
|--------|---------|
| `plugin.list` | `plugin_manager.packages` |
| `plugin.show` | `plugin_manager.packages` + config + `CoreState.surface_registry`(선언한 kind 가 등록됐는지) |
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
에이전트가 보는 계약이라 한 자리에만 둔다. gui 의 `App::plugin_enable` /
`App::plugin_disable` 도 같은 함수의 얇은 위임이다.

**갈리는 것은 낸 이벤트의 소비처 하나다.** gui 는 첫 main window 의 `PendingHostEvent`
큐에 넣어 `app/dispatch/host_events.rs` 가 다음 drain 에 event bus 로 보내고, 창이 하나도
없으면 **아무것도 발화하지 않는다**. 헤드리스는 창이 없어 그 큐를 못 쓰므로
`cascade_toggle_events_headless` 가 매니저를 직접 들고 그 자리에서 낸다. 발화하는 이벤트
키와 payload 자체(`plugin.enabled` / `plugin.disabled` / `plugin.unloaded`)는 두 경로가
같은 함수를 부른다. hook 이벤트 등록 해제도 gui 의 `cascade_plugin_unloaded` 와 같다.

근거·대안·재검토 조건은 [ADR-0253](../adr/0253-the-lifecycle-toggles-do-not-wait-for-the-app-split.md).

**매니저는 메타데이터 층까지만 세운다** (`ensure_plugin_manager_metadata`). 번들 설치는
부팅이 이미 했고(`src/boot.rs`), `PluginManager::enable` 은 그 package 표에서 **지목한
하나만** 찾아 기동한다. 여기서 `ensure_plugin_manager`(= `discover_and_start`)를 부르면
하나를 켜라는 명령이 설치된 전부를 띄운다 — 그것이 이 갈래를 나눈 이유다.

### 아직 없다 — 쓰기이지만 창은 필요 없다 (3)

`Core` 만 있으면 되므로 기술적 장벽은 없다. 읽기 표면과 **함께 열지 않은** 이유는
쓰기이기 때문이다. 감사 로그를 지우고 에이전트 권한을 바꾸는 것은 조회와 같은
판단으로 열 대상이 아니며, 권한 표면은 그 자체로 별도 결정을 요구한다.

`plugin.audit_clear` · `plugin.grant_agent_permission` · `plugin.revoke_agent_permission`

### 아직 없다 — `App` 이분이 선행이다 (6)

이어지는 `cascade_plugin_events` 는 `src/app/dispatch_domain.rs` 의 `App` 메서드이며
헤드리스 스텁(`dispatch_domain_stubs.rs`)에 대응물이 없다. 위 토글 둘은 그 cascade 중
자기 이벤트 둘만 헤드리스 형태로 대체해 열었지만, 나머지는 파일을 복사·삭제하거나
권한을 바꾸는 일이라 각각이 별도 결정이다. 이 경계를 여는 것은
[ADR-0127](../adr/0127-e2e-harness-binary-selection.md) 이 "`App` 이분이 선행" 이라고
적어 둔 그 자리다.

`plugin.install` · `plugin.remove` · `plugin.grant` · `plugin.revoke` ·
`plugin.upgrade_builtins` · `plugin.audit_follow`

`plugin.audit_follow` 는 `Core` 만 읽지만 구독을 여는 스트리밍 표면이라, 헤드리스에서
구독 수명을 무엇에 묶을지가 위 결정과 함께 정해져야 한다.

### 창이 없어도 답이 정의되는 것 (1)

`plugin.request_permission` 은 gui 에서 첫 main window 의 state 를 빌려 elevation popup 을
띄운다. 그래서 오래 "popup 을 보여 줄 창이 없으면 하는 일 자체가 없다" 로 분류돼 있었는데,
그 전제가 틀렸다 — **이 메서드가 만드는 것은 approval 레코드이고 popup 은 창이 있을 때
더해지는 표시다.** 레코드는 헤드리스에서도 `approval.await`·`approval.list`
·`approval.respond` 가 닿는다. 팝업만 `publish_capability_elevation` 안에서 gui feature 로
갈린다.

이것이 없으면 거부당한 헤드리스 agent 는 권한을 **요청할 자리**가 없다. 위 "권한 경계" 가
거부에 격상 레코드를 실어 주는 것과 같은 축의 나머지 절반이라 함께 열었다.

## 조회는 plugin 을 기동하지 않는다

헤드리스 데몬은 attach 세션이 없으면 plugin 을 하나도 띄우지 않는 것이 기본값이다. 그래서
`plugin.list` 에 답하려면 매니저를 세워야 하는데, 그 과정을 통째로 부르면 **조회가 자기
관측 대상을 바꾼다.** `src/boot/headless_plugins.rs` 는 그래서 둘로 갈라져 있다.

| 함수 | 하는 일 | 조회가 부르는가 |
|------|---------|-----------------|
| `ensure_plugin_manager_metadata` | 매니저 생성 + `refresh_packages`(디스크 스캔) | 예 |
| `ensure_plugin_manager` | 위 + `install_builtins_if_needed` + `discover_and_start` | 아니오 |

경계가 `install_builtins_if_needed` **위**인 것이 중요하다. 그 함수는 번들에서 파일을
복사하고 매니페스트 권한을 `plugins.toml` 에 자동 grant 한다 — 관측 대상을 정확하게 만드는
것이 아니라 **없던 설치를 만들어낸다.** 프로세스를 띄우는 것보다 앞서 배제된다.

그 결과 아무것도 설치되지 않은 홈에서는 목록이 빌 수 있다. 그것은 거짓이 아니라 그 시점의
사실이며, 매니저가 아예 없을 때의 응답과 구분된다.

| 응답 | 뜻 |
|------|-----|
| `-32000 plugin manager not initialized` | 매니저를 세우지 못했다(예: waker factory 부재) |
| `{"plugins": []}` | 매니저는 있고, 디스크에 설치된 plugin 이 없다 |

이 구분이 성립하려면 `Option<&PluginManager>` 를 받는 네 핸들러가 `None` 을 **같은 방식으로**
표현해야 한다. `handle_list` 만 빈 목록을 성공으로 돌려주던 이탈이 있었고, 지금은 넷이
같다. `src/adapters/ipc/handler/plugin.rs` 의 단위 테스트가 넷을 한 자리에서 비교한다.

## app 층 메서드 — 무엇이 답하고 무엇이 왜 없는가

gui 의 `app_methods` step(`src/app/ipc/app_methods.rs`)이 이름을 부르는 메서드를,
헤드리스가 답하는 것과 안 답하는 것으로 여기 가른다 — **그 수는 아래 두 절의 합**이고
여기 다시 적지 않는다. 두 절의 표가 소스와 정합인 것은 아래 가드가 강제하지만 이
문단은 그 가드 밖이라, 수를 여기 옮겨 적으면 표만 고쳐지고 이 줄은 조용히 낡는다.
`src/source_guards/headless_app_layer_coverage.rs` 가 이 표와 소스의 정합을 강제한다 —
**빈칸을 못 만들게 하는 것**이 그 가드의 목적이고, 초록은 "두 조합이 같다" 가 아니라
"차이가 전부 사유와 함께 적혀 있다" 는 뜻이다.

### 답한다 (7)

창이 없어도 답이 정의되는 것들이다. 본체는 두 조합이 **같은 함수**를 쓴다
(`src/core/app_surface.rs`, 승인·태스크 대기는 대기 본문 옆인 `handler/approval/read.rs` ·
`handler/agent/task.rs`) — `system.shutdown` 만 끊는 방식이 조합마다 달라 예외다.

| 메서드 | 읽는 것 / 하는 일 |
|--------|-------------------|
| `timer.list` | `App` 의 TimerHub — 무엇이 인스턴스를 깨우는가 |
| `clipboard.set_text` | `Core` 의 클립보드 포트. 없는 환경이면 포트가 실패를 돌려주고 그것이 사실이다 |
| `remote.workspaces` | 인자만 읽는다. App 상태를 하나도 안 본다 |
| `agent.task_await` | 이 engine 의 `task_waker_hub` + `agent_seq` |
| `approval.await` | 이 engine 의 `approval_store` |
| `events.fetch` | `App.plugin_manager` 의 사건 버스 링. `wait_ms` 대기는 워커로 나가 dispatch 루프를 안 막는다. **읽는 쪽만 있으면 답이 빈다** — `agent` 사건 큐를 버스로 옮기는 드레인이 데몬 루프(`run_headless`)의 맨 위에도 있어야 하고, 두 조합이 같은 함수를 쓴다(`app::agent_events`) |
| `system.shutdown` | 데몬을 멈춘다(debug 전용). 응답을 먼저 보내고 run loop 를 끊는다 |

### 없는 것이 정답 (11)

읽는 것이 `App.view` 인데 헤드리스에 그 필드가 없다(`src/app.rs` 에서 `gui` 게이트).
사유를 메서드마다 적는 이유는, "이 표면은 GUI 가 필요하다" 같은 뭉뚱그림이 **어느
것이 진짜 창을 요구하고 어느 것이 그냥 안 열린 것인지**를 지우기 때문이다.

| 메서드 | 왜 |
|--------|-----|
| `window.create` / `view.create` | winit 이벤트루프에 창 생성을 맡긴다. 헤드리스엔 그 루프가 없다 |
| `window.close` / `view.close` | `App.view.views` 에서 창을 닫는다. 그 레지스트리가 없다 |
| `window.focus` / `view.focus` | 포커스 전환이라 애초에 debug 격리(ADR-0115)이고, 대상도 창이다 |
| `window.list` / `view.list` | 빈 목록이 아니라 **개념이 없다** — `[]` 를 주면 "창이 0 개인 GUI" 로 읽혀 호출자가 `window.create` 를 시도한다 |
| `ui.screenshot` | 창 표면을 읽어 파일로 쓴다. 그릴 창이 없으면 하는 일 자체가 없다 |
| `remote.attach` | mirror workspace 를 띄울 창이 필요하다 |
| `system.gpu_stats` | 창마다의 GpuState 와 wgpu 전역 리포트를 센다. GPU 컨텍스트가 없다 |

`plugin.*` 는 위 "`plugin.*` — 19 개 메서드의 판정" 절이 따로 가른다 — 그 절 기준 **지금 9 건**이다.
아래 census 는 2026-09-05 에 12 건으로 셌고, 그 뒤 `plugin.request_permission` 과 토글 둘이
열려 그만큼 줄었다. 이 수는 census 를 다시 돌려 얻은 값이 아니라 그 절의 분류에서 따라오는
값이다 — 정본은 그 절이다.

## dispatch arm 이 `gui` 로 게이트된 표면

위 절이 다루는 `app_methods` step 과 **다른 축**이다. 이쪽은 `src/adapters/ipc/handler.rs`
의 dispatch arm 이 `#[cfg(feature = "gui")]` 인 경우와, gui 라우터의 debug step
(`src/app/ipc/debug_methods.rs`)에만 있는 경우 둘이다.

모수는 실행으로 세웠다 — 등재 `METHOD_TABLE` 276 + `DEBUG_METHODS` 50 = **326 건**을
두 조합에 각각 붙여 같은 인자로 부르고, **gui 는 답하는데 헤드리스가 답하지 않는 것**을
셌다(2026-09-05 실측).

> **이 문서에서 헤드리스의 거부 코드로 적힌 `-32601` 은 지금 `-32017` 이다.** 표에 등재된
> 이름이 이 빌드에 arm 이 없어 거절될 때의 코드를 오타(`-32601`)와 가른 것이
> [ADR-0167](../adr/0167-a-registered-name-answers-whether-it-is-in-this-binary.md) 이다.
> 위 census 수치는 그 변경 **이전**에 잰 것이고, 무엇이 답하고 무엇이 안 답하는가라는
> **판정 자체는 그대로다** — 바뀐 것은 안 답할 때 무슨 코드를 주느냐뿐이다. 두 표는 **겹치지 않는다** — 그 시점 교집합 0 이라 `+` 가 합집합과
같다. 같은 모수를 [ADR-0153](../adr/0153-a-bundled-namespace-hands-host-methods-back.md)
이 같은 말로 부른다(그쪽은 거기에 핸들러 트리 리터럴을 합집합해 361 로 넓힌다).

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
[ADR-0154](../adr/0154-a-platform-gated-dispatch-arm-answers-why-not-what.md)).

**★ 그런데 이 둘의 게이트는 축 하나가 아니다.** `src/adapters/ipc/handler.rs` 에서 실제 arm 은
`#[cfg(all(target_os = "macos", feature = "gui"))]` 이고 그 짝이 `not(all(…))` 이다 — 즉
**플랫폼과 조합이 한 코드로 접혀 있다.** 그래서 linux 헤드리스에서 오는 `-32015` 는 두
조건이 함께 실패한 결과이고, 이 census 가 세는 "gui 는 답하는데 헤드리스가 답하지 않는 것"
에 해당하는지를 그 코드만 보고는 **가를 수 없다**(가르려면 macOS gui 와 macOS 헤드리스를
같이 재야 한다 — 이 저장소에서 아직 안 쟀다).

그래서 두 이름은 census 의 어느 부류에도 넣지 않고 여기 이름과 실측 코드만 세워 둔다.
분류는 **미정**이며, 비어 있는 것은 재지 않았기 때문이다.

| 부류 | 건수 | 어디서 판정하나 |
|------|------|-----------------|
| 창 축(`window.*` · `view.*` · `ui.screenshot` · `remote.attach` · `system.gpu_stats`) | 11 | 위 "app 층 메서드" 절 |
| `plugin.*` | 9 | 위 "`plugin.*` — 19 개 메서드의 판정" 절 (census 시점 12 → 그 뒤 셋이 열렸다) |
| `debug.*` | 36 | 이 절 |
| 그 밖 | 6 | 이 절 |
| **합** | **62** | |

### 분류 미정 (그 밖 2)

`image.*` 를 이 census 밖으로 뺐던 근거가 틀렸다는 것은 "남은 표면" 절에 적혀 있다. 여기가 그 둘의 칸이다.

| 메서드 | 헤드리스 실측 | 분류 |
|--------|---------------|------|
| `image.open` | `-32017`(감싸짐) | **미정** — 핸들러가 `ConvertSurface` 를 발행하고 `image` kind 는 헤드리스에서도 **등록된다**(egui-mesh). 창이 없어서인지 경계가 안 열려서인지 재지 않았다 |
| `image.list` | `-32017`(감싸짐) | **미정** — surface 순회 조회다. 위와 같은 물음이 걸린다 |

분류를 비워 두는 것은 재지 않았기 때문이고, 그 사실을 적어 두는 쪽이 칸 자체를 없애는 것보다 낫다 — 칸이 없으면 물음이 있다는 것도 안 보인다.

### 답한다

| 메서드 | 읽는 것 |
|--------|---------|
| `theme.query` | 전역 Theme + `CoreState.settings` 뿐이다. 창도 surface 도 렌더러도 안 본다 |

`theme.query` 는 창을 하나도 안 읽는데 핸들러가 `gui` 게이트가 걸린 `webview` 모듈 안에
살고 있어 arm 까지 함께 게이트됐다. 핸들러를 `src/adapters/ipc/handler/theme.rs` 로 갈라
게이트 밖으로 냈다 — 두 조합이 **같은 함수**를 쓴다.

### 아직 없다 — `App` 이분이 선행이다 (그 밖 1)

| 메서드 | 왜 |
|--------|-----|
| `markdown.navigate` | host arm(`src/adapters/ipc/handler.rs` 의 `"markdown.navigate" =>`)이 `#[cfg(feature = "gui")]` 다 — `file_picker.trigger` 와 같은 구성이다. 핸들러 자체(`handler/markdown.rs::handle_navigate`)는 `AppState` 만 읽고 `ConvertSurface` intent 를 발행할 뿐 `App.view` 를 안 본다. [ADR-0153](../adr/0153-a-bundled-namespace-hands-host-methods-back.md) 이 세 세계를 실제로 호출해 이 이름을 갈랐고, 그 재검토 트리거가 **"`App` 이분이 착수되면 headless 에서 이 host arm 이 열릴 수 있다"** 고 적는다. 그래서 이것은 창이 없어서가 아니라 **경계가 아직 안 열려서** 없는 것이다 |

헤드리스에서 부르면 응답이 한 겹 감싸여 온다 — `-32017 host call 'call#N' failed: method
'markdown.navigate' is registered but this binary has no dispatch arm for it`. 번들
markdown plugin 이 그 namespace 를 점유해 host 로 되돌리기 때문이고, 그 모양의 근거도
같은 ADR 이다.

### 없는 것이 정답 (그 밖 2)

| 메서드 | 왜 |
|--------|-----|
| `file_picker.trigger` | 창 안 popup(`state.dialogs.file_picker`)을 연다. 그릴 창이 없다 |
| `webview.set_url` | 설정한 URL 을 소비하는 것이 매 프레임 도는 렌더러뿐이다. 값은 기록되겠지만 아무 일도 일어나지 않는다 |

### census 뒤에 게이트된 arm

위 표의 건수는 census 시점 값이라 그 뒤에 `gui` 로 게이트된 arm 은 거기 안 들어간다. 이름만 여기 세운다.

| 메서드 | 왜 |
|--------|-----|
| `file_handler.dispatch` | 요청을 적용할 identify worker 와 결과를 여는 창이 gui 에만 있다. arm 이 헤드리스에 있던 동안은 `{"accepted": true}` 로 답하고 요청을 버렸다 — `git_viewer.query` 와 같은 모양이다. 근거 [ADR-0425](../adr/0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md). 같은 namespace 의 `file_handler.reload` · `file_handler.detectors` 는 헤드리스에서도 답한다 |

### `debug.*` 36 건

debug 표면은 **에이전트가 자기 작업을 검증하는 자리**다(popup 이 떴는가, 훅이 발화했는가,
event bus 에 누가 붙었는가). 그래서 헤드리스에서만 사라지면 헤드리스 인스턴스는 자기
동작을 확인할 수단이 없다 — release 격리와는 다른 축이다. **여는 것은 "헤드리스 debug
빌드에서도 답한다" 이지 "release 에 노출한다" 가 아니다.** release 격리는 `DEBUG_METHODS`
가 release 에서 비는 것으로 유지되고, 실행으로 확인한다(release 헤드리스 실측: 아래 다섯
전부 `-32601`).

모수는 **호출마다 새 인스턴스를 띄우는** census 로 쟀다(2026-09-05). 한 인스턴스에서
순차로 부르면 앞쪽의 파괴적 호출이 뒤쪽 호출의 라우팅 대상을 없애고, 그러면 멀쩡한
메서드가 `Method not found` 로 보인다. 36 = 답한다 7 + 없는 것이 정답 29, 애매한 것 0.

#### 답한다 (7)

읽는 것이 `App` 의 `lua_engine` / `plugin_manager`, 그리고 gui 무관 정적 표뿐이다. 창·렌더러·egui 입력 큐를 하나도
안 본다. 자리가 없어서 사라졌던 것이라 헤드리스 pump 에 자리를 만들었고, 본체는 두 조합이
**같은 함수**를 쓴다(`src/core/app_surface_debug.rs` · `handler/debug_plugin.rs` ·
`handler/popup.rs`).

**판정은 갈래 단위가 아니라 이름 단위다.** `debug.popup.*` 가 그 실례다 — 셋이 한 갈래인데
`list` 는 여기 있고 `open`/`close` 는 아래에 있으며, 둘의 사유마저 서로 다르다(`open` 은
답이 정의되는데도 닫을 수단이 없어서, `close` 는 glue 가 gui 게이트 안이라서). 갈래로
묶어 한 줄로 적으면 그 차이가 안 보이고, 실제로 이 표에 한동안 그렇게 적혀 있었다.

| 메서드 | 읽는 것 |
|--------|---------|
| `debug.lua.eval` | `App.lua_engine` 워커에 스크립트를 던진다(fire-and-forget) |
| `debug.event_bus.list_subscribers` | `plugin_manager` 의 event bus 구독자 |
| `debug.event_bus.publish` | 같은 bus 에 이벤트를 넣는다 |
| `debug.event_bus.trace` | 같은 bus 의 trace |
| `debug.extension.invoke_hook` | `plugin_manager` 의 확장 훅을 수동 발화 |
| `debug.popup.list` | `plugin_manager` 의 popup contribute 목록과 열린 인스턴스. **조회만이다** — 같은 갈래의 `open`/`close` 는 아래 표에 있다 |
| `debug.fullscreen.list` | `src/fullscreen_stages.rs` 의 gui 무관 무대 메타(id·제목 키). **조회만이다** — 같은 갈래의 `open`/`close`/`state` 는 창을 지목해야 해서 아래 표에 있다 |

event bus 두 건은 매니저를 **메타데이터 층까지만** 세운다 — 조회가 plugin 프로세스를
띄우면 관측이 자기 대상을 바꾼다([ADR-0136](../adr/0136-a-query-does-not-create-what-it-observes.md)).
그래서 아무 plugin 도 안 뜬 데몬에서는 구독자가 0 으로 나오고, 그것이 그 시점의 사실이다.

#### 없는 것이 정답 (30)

| 메서드 | 왜 |
|--------|-----|
| `debug.info` · `debug.focused_surface` · `debug.selection` · `debug.pending_menu` | 창 하나의 렌더 상태(셀 크기·포커스·선택·대기 중 native 메뉴)를 읽는다 |
| `debug.settings.open` | `AppEvent::OpenSettings` 를 winit proxy 로 보낸다. 헤드리스엔 proxy 가 없다 |
| `debug.gpu.stall` | 렌더 스레드를 일부러 막아 stall 워치독을 시험한다. 막을 스레드가 없다 |
| `debug.banner.*` (4) · `debug.host_popup.*` (3) · `debug.modifier_hint.*` (2) | host 위젯의 표시 상태다. 그릴 창이 없으면 상태 자체가 없다 |
| `debug.tool.list` · `debug.tool.invoke` | 도구 메뉴는 창의 위젯이다 |
| `debug.fullscreen.open` · `close` · `state` (3) | 무대는 **창 단위**라 `pick_debug_window` 로 `self.view.views` 에서 창을 지목한다 |
| `debug.plugin_banner.*` (2) | 소유 view 의 BannerManager 와 host 매니저를 함께 다룬다 — `open` 도 `close` 도 `self.view.views` 를 순회한다. **재 봤고 갈래 안에서 판정이 안 갈린다**, 그래서 한 줄이 맞다 |
| `debug.inject_mouse` · `debug.inject_key` · `debug.inject_window_mouse` · `debug.inject_egui_mouse` · `debug.inject_egui_key` · `debug.inject_egui_text` (6) | 사용자 입력 재현이다. 앞 둘은 대상 surface 의 PTY 로, 뒤 넷은 winit·egui 입력 큐로 들어간다 — 그 큐가 창에 딸려 있다 |
| `debug.popup.open` | 매니저만 읽어 **답은 정의된다.** 그런데 헤드리스에는 그 인스턴스를 **닫는 경로가 하나도 없다** — debug close 도, plugin 자신의 release `popup.close` 도 gui 게이트 안의 `app::dispatch` 에 산다. 여는 것만 열면 그 빌드에서 닫을 수 없는 상태가 남는다 |
| `debug.popup.close` | 렌더가 수집하는 close 큐로 합류해야 `cancel_child_file_picker` 연쇄 정리가 돈다([ADR-0084](../adr/0084-plugin-triggered-host-popup-ownership.md)). 그 glue 가 gui 게이트 안이다 |

`src/source_guards/headless_app_layer_coverage.rs` 가 이 표와 두 라우터의 정합을 강제한다 —
app 층 step 과 debug step 두 쌍을 같은 규약으로 본다.

헤드리스 쪽에서 그 **텍스트를 읽는 창**은 dispatch 함수 명부다(`pump_ipc` ·
`intercept_app_layer` · `intercept_debug_app_layer`) — 파일 전체가 아니다. 답하지 않는
헬퍼가 이름을 문자열로 들면 파일 전체를 세는 창은 그것까지 답으로 세기 때문이다. 그
좁힘의 대가로 **명부 밖에 답하는 함수를 만들면 안 보이는** 사각이 생기므로, 같은 가드가
*출하 범위로 좁히고 주석을 걷어낸 파일 전체*의 이름이 명부 합집합과 같은지를 따로 잰다
(잔여 0). 출하 범위로 먼저 좁히는 것은 `#[cfg(test)]` 아래의 픽스처가 이름을 인용했을 때
**출하되지도 않는 코드에 면제를 요구하는 거짓 실패**가 나기 때문이다. 헤드리스
dispatch 에 새 헬퍼를 만들어 거기서 메서드 이름에 답하려면 **그 함수를 명부에 함께
등록한다** — 안 하면 그 가드가 그 자리에서 막는다.

### 이름은 리터럴로 적는다

위 가드는 dispatch 본문을 **텍스트로** 읽어 `"a.b"` 꼴 리터럴을 뽑는다. 그래서 이름이
리터럴이 아니면 — 매크로가 만들거나 상수와 맞대면 — 그 갈래는 표에도, 가드에도 안 보이고
답하지도 사유가 적혀 있지도 않은 메서드가 조용히 생긴다.

이 사각은 "몇 개 뽑혔나" 로 못 막는다. 이름 하나를 매크로 뒤로 숨기면 항목이 하나 줄 뿐이고,
매크로가 만든 이름으로 갈래를 더하면 항목 수는 아예 안 변한다 — 하한은 줄어드는 방향만
보므로 뒤쪽은 원리적으로 못 본다. 그래서 가드가 따로 재는 것은 **이름을 읽는 자리**다:
`request.method` 로 갈래를 칠 때 맞대는 값(`==` · `starts_with` · `match … as_str()` 의 팔)은
문자열 리터럴이어야 한다. 값을 위임 함수에 **넘기기만** 하는 자리는 대상이 아니다.

## 남은 표면

`debug.*` 36 건의 판정은 위 "`debug.*` 36 건" 절에 있다.

`image.open` · `image.list` 는 **닿는 자리가 같은 것이지 답이 같은 것이 아니다.**
[ADR-0153](../adr/0153-a-bundled-namespace-hands-host-methods-back.md) 이 잰 대로 번들 plugin 이
그 namespace 를 점유하고 self-call trampoline 로 host 에 돌려주므로 세 세계 모두 host arm 에
**닿는다.** 그러나 그 host arm 은 `src/adapters/ipc/handler.rs` 에서 `#[cfg(feature = "gui")]`
다 — 헤드리스에서 부르면 `-32017 host call 'call#N' failed: … gated out of this build
combination` 이 온다(실측 2026-09-07, `--no-default-features` 빌드).

**닿는 것과 답하는 것은 다르고, 이 census 의 술어는 뒤엣것이다**("gui 는 답하는데 헤드리스가
답하지 않는 것"). 그래서 이 둘은 census 밖이 아니라 그 안의 항목이다. 같은 ADR 이
`markdown.navigate` 에 대해 헤드리스를 별개 축으로 명시하는 것도 같은 이유다 — ADR 은
헤드리스를 봤고, 그것을 제외의 근거로 읽은 것이 이 문단이었다.

두 이름의 칸은 아래 "`그 밖` — 분류 미정 (2)" 에 있다.

두 칸의 분류를 비워 두는 것은 재지 않았기 때문이고, 그 사실을 적어 두는 쪽이 칸 자체를
없애는 것보다 낫다 — 칸이 없으면 물음이 있다는 것도 안 보인다.

## PTY 실행 종료

PTY drain은 output-match 훅과 함께 명시적인 process exit를 소비한다. GUI와 공유하는 host 처리에서 실행 구독 종료, process-exit 훅, no-snapshot surface close 및 soft 점유 정리를 수행한다. SessionEnd hook이 유실돼도 실제 PTY 종료가 수명 종료 근거가 된다. surface가 다른 window의 로컬 목록에 없다는 이유로 종료를 합성하지 않는다.
