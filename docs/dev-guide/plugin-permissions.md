# 호스트 호출 계약과 Agent 권한

## 제품 원칙과 현재 구현

[플러그인 원칙](../principles/plugins.md)은 인증과 미인증 설치 경고를 보안 정책으로 정한다. 설치 후 미인증 표시 외 제한을 추가하지 않는다. 그래서 플러그인에는 권한이 없다. 설치·활성된 플러그인은 `method_meta` 가 플러그인에 연 메서드(`plugin_callable`)를 토큰 없이 모두 부를 수 있고, 매니페스트로 선언한 기여(도구 메뉴 항목·팝업·배너·설정 페이지·파일 핸들러·훅·완료 전략·detector·extension)도 권한 토큰 없이 동작한다. `local_only` 메서드와 debug 메서드는 플러그인에 열지 않는다. 제품별 정합에서는 포커스 탈취 금지·원격 연결 강제 해제 인터페이스 제한과 자료 무결성 검사를 구분한다.

권한 토큰은 Agent 세션에만 쓴다. 이 문서는 호출자별 판정, Agent 권한 토큰 전체 목록(각 토큰이 실제로 무엇을 여는지), 새 IPC 메서드/토큰 추가 절차를 다룬다. 매니페스트가 무엇을 선언하는지는 [concepts/plugins](../concepts/plugins.md#매니페스트-선언), 제작 흐름은 [plugin-development](plugin-development.md).

## 구성 요소

| 위치 | 역할 |
|------|------|
| `crates/tasty-plugin-manifest/src/types.rs::Permission` | Agent 권한 enum + 토큰 매핑(`from_token`/`as_token`). 새 토큰은 여기 |
| `crates/tasty-ipc/src/method_meta.rs::method_meta` | IPC 메서드 → Agent 에게 필요한 권한 / plugin 호출 가능 여부 (기준 표) |
| `crates/tasty-ipc/src/caller.rs::CallerContext` | 호출자 종류 (Local / Internal / Plugin / Agent) + `ensure_allowed` |
| `src/adapters/ipc/handler/gate.rs::decide` | 라우터 진입에서 `ensure_allowed` + capability elevation 자동 발행 + audit |
| `crates/tasty-host-plugin/src/manager/ipc_dispatch.rs::validate_namespace_call` | plugin namespace 호출의 소유자 확인·자기 namespace 거절·owner 준비 |

## 호출자별 판정

`CallerContext::ensure_allowed` 가 호출자 종류에 따라 판정한다.

| 호출자 | 판정 |
|---|---|
| Local | 통과한다 |
| Plugin | `method_meta` 에 있고 `plugin_callable` 이면 통과한다. `local_only`·debug 메서드는 `NotPluginCallable`, 표에 없는 이름은 `UnknownMethod` 로 거절한다 |
| Agent | `method_meta` 의 `required` 토큰을 세션 권한이 모두 가져야 통과한다. 없으면 `MissingPermission`(-32001)과 capability elevation 요청이 생긴다 |

다른 plugin 의 namespace 메서드는 plugin 호출자가 토큰 없이 부를 수 있다. 자기 namespace 를 host-call 로 부르면 `-32001` 로 거절하고(self-loop 방지), 아무도 점유하지 않은 namespace 는 `-32601`, owner 가 실행 중이 아니면 `-32002` 다. Agent 는 plugin namespace 를 부를 때 `ipc.invoke:<prefix>` 가 필요하다. 다른 plugin 의 자료에 대한 소유 검사(예: 턴 보고의 provider 소유, 팝업 대상 소유)는 호출자 종류와 관계없이 그대로 적용한다.

## 요청 진입 검사

GUI·헤드리스의 외부 IPC와 plugin host-call은 요청 게이트(`handler/gate.rs`)에서 권한→cap→rate-limit을 한 번 검사한다.
통과한 요청의 telemetry와 Allow audit 호출도 한 번 수행하되 Allow는 보존 정책상 디스크에 남지 않는다.
private CheckedRequest가 요청·caller와 함께 검사 완료를 전달하고 wire checked 플래그나 Local 변환으로 우회하지 않는다.
alias는 canonical 이름으로 판정·집계한다.
새 host-call은 별도 요청이므로 다시 검사한다.
엔진 없는 GUI 구간은 Local 부팅 요청만 허용하며 Local 이외 호출자는 state 오류로 거절한다.
저장소 오류의 기존 fail-open 정책은 바꾸지 않는다.

## 토큰 형식 — `<name>[:<scope>]`

- **`.`** 는 *이름의 일부*다 (`surface.read` 의 `.` 는 분류용 — 호스트는 쪼개지 않음).
- **`:`** 는 *scope 구분자*다 — 권한이 적용되는 대상을 한정해야 의미가 생기는 권한에만 등장.

전체 토큰 목록은 아래 [토큰 전체 — 무엇을 여나](#토큰-전체--무엇을-여나). scoped 검증 규칙:

- `ipc.invoke:<prefix>` — `is_valid_ipc_prefix`(소문자 시작+소문자/숫자/`_`, ≤32) **그리고** 호스트 예약어 거부.
- `ext:<plugin_id>` — `is_valid_plugin_id`(reverse-DNS).
- `file_handler.extend:<id>` / `file_handler.handle:<id>` — `is_valid_detector_id`, `$unknown` 거부.
- `hook_handler.handle:<id>` — `is_valid_hook_handler_id`(소문자+숫자+`-`, ≤32). `$`-prefix reserved 개념 없음. `hook_handler.define` 은 scope 없는 base 토큰.

형식 위반 토큰은 `from_token` 이 `None` → `session.issue` 가 `unknown permission token` 으로 거부.

## 토큰 전체 — 무엇을 여나

`Permission` enum 의 `as_token` 이 토큰 문자열의 단일 출처다(exhaustive match — variant 를 늘리면 팔이 강제된다). 토큰은 Agent 세션에만 부여하며, 각 토큰이 Agent 에게 **실제로 여는 것**은 `method_meta` 가 그 권한을 요구하는 **호스트 IPC 메서드**다. 요구하는 메서드가 없는 토큰은 아무것도 열지 않는다 — 표에 그대로 적는다. 플러그인 호출자에게는 이 표가 적용되지 않는다([호출자별 판정](#호출자별-판정)).
한 메서드가 여러 권한을 **함께** 요구할 수 있으므로(`image.paste` 는 `surface.write` + `clipboard.read`) 같은 메서드가 여러 행에 나온다.

| 토큰 | 여는 호스트 IPC | 비고 |
|------|-----------------|----------------|
| `surface.read` | 트리·메타 조회 (`tree` · `workspace.list` · `tab.list` · `surface.list` · `surface.meta.get` · `hook.list` · `message.read` · `recent.query` 등) | — |
| `surface.write` | 트리 변경 (`workspace.create` · `tab.create` · `tab.close` · `split` · `surface.close` · `surface.set_cwd` · `surface.meta.set` · `hook.set` · `message.send` · html 문서 스크립트 허용 `surface.html_script_allow` · 원격 콘텐츠 허용 `surface.html_remote_allow` 등) | — |
| `notification` | `notification.create` · `notification.list` · `notification.counts` · `notification.read` · `surface.completion` · `surface.attention.get` · `surface.attention.clear` | — |
| `clipboard.read` | `image.paste` **하나뿐** | — |
| `clipboard.write` | `clipboard.set_text` | — |
| `fs.read` | `file_handler.dispatch` · `image.open` · `image.reload` · `markdown.navigate` · `git_viewer.query` · `markdown_mirror.content_request` · `file_picker.trigger` | — |
| `fs.write` | `image.save` · `image.export_png` | — |
| `process.spawn` | **없음** — `method_meta` 어느 메서드도 요구하지 않는다 | 없음 |
| `terminal.spawn` | `terminal.spawn` · `terminal.respawn` · `pty.spawn` · `pty.attach_surface` · `surface.wake` · `surface.respawn_terminal` · `attach.agent_call` · `attach.agent_commit` | — |
| `terminal.write` | `surface.send` · `surface.send_key` · `surface.send_to` · `terminal.tell` · `terminal.broadcast` · `pty.write` · `pty.kill` 등 | — |
| `terminal.read` | `surface.read_since_mark` · `surface.screen_text` · `surface.commands` · `output.observe_*` · `pty.read` · `pty.wait` 등 | — |
| `network` | `webhook.register` (아래 "network" 절) | — |
| `memory.read` | `memory.get/list/query/export` · `memory.exists/count/scopes/stats` · `memory.bb_*` 조회 · `memory.plan_*` 조회 · `memory.cache_get/cache_list` · `memory.goal_get` · `approval.summary.get` | — |
| `memory.write` | `memory.put/delete/import` · `memory.bb_*` 변경 · `memory.plan_*` 변경 · `memory.cache_*` · `memory.goal_set/goal_clear` · `approval.summary.set` | — |
| `memory.secret` | `memory.secret.*` 전부 | — |
| `approval` | `approval.*` 전부 · `plugin.request_permission`. `approval.summary.get/set` 은 `memory.read`/`memory.write` 를 함께 요구 | — |
| `telemetry` | `telemetry.*` 전부(기록·조회·cap·anomaly) | — |
| `agent` | `agent.*` 협업 primitive(턴 보고 하나만 아래 `agent.turn_report` 행) · `session.issue` · `session.revoke` | — |
| `agent.turn_report` | `agent.task_turn_report` **하나뿐** | 핸들러가 호출자가 보고하는 provider namespace 를 소유한 플러그인인지 확인하므로 Agent 호출은 이 토큰이 있어도 `-32001` 로 끝난다. provider 자격은 토큰이 아니라 namespace 소유와 활성 상태로 정한다([작업 러너 §agent task](agent-runner.md#agent-task)) |
| `ui.tool_item` | 없음 | 없음 |
| `ui.popup` | `popup.declare` · `popup.show` · `popup.close` | — |
| `ui.banner` | `banner.open` · `banner.close` | — |
| `ui.settings_page` | `settings.get_plugin_setting` · `settings.initialize_input_rule` | 입력 기본값은 호출자/앱별 최초 등록만 허용하며 기존 규칙·사용자 삭제를 덮어쓰지 않음 |
| `window.spawn` | 없음 | 없음 |
| `file_handler.define` | 없음 | 없음 |
| `hook_handler.define` | 없음 | 없음 |
| `completion_strategy.define` | 없음 | 없음 |
| `ipc.invoke:<prefix>` | 그 prefix 를 점유한 plugin 의 namespace 메서드 중 `METHOD_TABLE` 에 이름이 없는 것 전부 | `ensure_allowed` 가 Agent 에게 요구(없으면 `-32001` + elevation 발행). 플러그인 호출자에게는 요구하지 않는다 |
| `ext:<plugin_id>` | 없음 | 없음 |
| `file_handler.extend:<id>` | 없음 | 없음 |
| `file_handler.handle:<id>` | 없음 | 없음 |
| `hook_handler.handle:<id>` | 없음 | 없음 |

### 호스트 키 namespace 는 memory 권한으로 열리지 않는다

raw memory KV에서 tasty. 접두사는 호스트 전용이다. plugin·agent는 키 지정·import에서 거절되고 list/query/export에서는 필터되며 count에서도 제외된다. Local은 기존 접근을 유지한다. 정당한 blackboard·plan·cache는 전용 API로 사용한다. scope 이름과 합계 통계는 키·값을 노출하지 않는 범위에서 유지한다. 기존 row owner 검사만으로 신규 호스트 키 위조를 막을 수 없어 prefix 정책을 IPC에서 집행한다. owner 기반 읽기나 stats 세분화가 도입되면 예약 범위를 재검토한다.

### 선언 범위와 실제 개방 범위는 같지 않다

토큰 이름은 넓은 범주를 가리키지만, 그 토큰이 지금 여는 호스트 IPC 는 표의 가운데 열이 전부다. 특히:

- **`clipboard.read` 는 `image.paste` 만 연다.** 클립보드 내용을 읽는 번들 plugin(clipboard-viewer)은 호스트를 거치지 않고 자기 프로세스에서 직접 OS 클립보드를 읽는다([플러그인 제작 가이드](plugin-development.md)) — 그 읽기는 이 토큰이 통제하지 않는다.
- **`process.spawn` · `window.spawn` · `ui.tool_item` · `file_handler.*` · `hook_handler.*` · `completion_strategy.define` · `ext:<plugin_id>` 는 아무 IPC 도 열지 않는다.** 요구하는 메서드가 없어 Agent 세션에 부여해도 달라지는 것이 없다.

토큰을 넓은 이름 그대로 읽으면 Agent 세션 발급이 실제보다 큰 권한을 넘기는 것처럼 보인다. 반대 방향의 오해가 더 중요하다 — **Agent 프로세스가 자기 손으로 하는 일은 어느 토큰도 막지 못한다**(아래 [한계](#한계)).

### 토큰 없이도 열려 있는 메서드

위 표는 **토큰을 축으로** 하므로, 어느 토큰도 요구하지 않는 메서드는 어느 행에도 나타나지 않는다. `method_meta` 에는 그런 항목이 있다 — `plugin_callable` 이면서 `required` 가 빈 30개다. **권한 토큰을 하나도 받지 않은 Agent 세션도 이 가운데 `plugin_only` 가 아닌 것은 부를 수 있다.** `plugin_only` 인 `host.shared_buffer.create` 는 플러그인 진입부만 처리하므로 Agent 가 부르면 `-32016` 으로 거절된다. 앞 절이 "토큰이 이름보다 좁게 연다" 는 방향이라면 이것은 반대편이다. 플러그인 호출자는 토큰과 관계없이 `plugin_callable` 메서드를 모두 부르므로 이 표는 토큰 검사를 받는 Agent 기준이다.

근거가 **세 갈래**다. 한 덩어리로 적으면 "권한 없이 열린 건 전부 SSH 때문" 으로 잘못 일반화되므로 군을 나눈다. 특히 셋째 군은 앞의 둘과 성질이 다르다 — **결정이 아니라 미결**이다.

| 군 | 메서드 | 왜 토큰을 요구하지 않나 |
|----|--------|------------------------|
| 전역·자기 정보 조회 | `system.info` · `theme.query` · `settings.get_keybinding` · `plugin.list_agent_permissions` · `run.result` | 상태를 바꾸지 않고 특정 surface 의 내용도 노출하지 않는다. run.result는 호출자가 시작한 요청의 결과만 돌려준다(다른 plugin·에이전트의 run은 알 수 없는 run과 같다). 요청을 보낼 때 이미 그 메서드의 권한을 확인했으므로 결과 조회에 토큰을 다시 요구하지 않는다. settings.get_keybinding은 단축키 설정 한 동작의 전역 바인딩이다(webview 페이지가 직접 처리하는 동작의 조합을 문서에 넣을 때 쓴다). system.info의 version은 전역이고 count/index는 소유 workspace ID가 명시된 engine 관측값이다. theme.query는 테마의 전역 스냅샷이고(webview-kind surface 는 `set_context` 로 Theme 를 push 받지 못해 문서를 그릴 때마다 `theme.query` 로 대신한다 — [WebView 호스트 계약](../design/systems/webview.md)), 마지막은 **자기에게 지금 무슨 권한이 있는지**를 되읽는 self-introspection 이라 토큰을 요구하면 순환이 된다 |
| attach · 원격 프로필 · 패스키 | `attach.acquire` · `attach.release` · `attach.force_detach` · `attach.force_detach_workspace` · `attach.into_gui` · `attach.list` · `attach.agent_target` · `attach.child_notices` · `remote.profile.list` · `remote.profile.get` · `remote.profile.add` · `remote.profile.detect` · `remote.profile.remove` · `remote.profile.list_local` · `remote.profile.import` · `remote.workspaces` · `remote.attach` · `remote.refusals` · `remote.disconnect` · `remote.connect` · `remote.passkey.list` · `remote.passkey.get` · `remote.passkey.add` · `remote.passkey.remove` | 신뢰경계가 이 권한 모델이 아니라 **연결 경계(SSH + loopback)** 다 — 근거는 아래 [한계](#한계) 의 attach 문단에 있고 여기서 되풀이하지 않는다. 프로필은 비밀 없는 장비 인벤토리이고, 원격 워크스페이스 열거도 같은 조회이며, 패스키도 `list`/`get` 이 이름과 종류만 돌려주며 키 파일 내용은 반환하지 않는다([에이전트 메모리 시스템](../design/systems/memory.md)). 원격 자식 에이전트의 `attach.agent_call`·`attach.agent_commit`·`attach.child_notice` 는 이 군에 없다 — 터미널을 만들고 입력하는 일이라 Agent 호출자에게 `terminal.spawn`·`terminal.write` 권한을 요구한다([Attach 메커니즘](attach-behavior.md#원격-자식-에이전트-ipcstreamagent-children)). 조회인 `attach.agent_target`·`attach.child_notices` 는 이 군에 있다. `attach.into_gui` 는 GUI 에서 engine 처리기가 아니라 App 층이 처리한다 — plugin host-call 도 게이트를 통과한 뒤 외부 IPC 와 같은 자기 자신 판정을 거쳐 답을 받는다. `remote.attach` 는 `attach.into_gui` 와 같은 일을 하되 호출자가 연 터널을 받는 대신 호스트가 프로필·`ssh` 대상으로 터널을 직접 열고, 두 메서드 모두 mirror 를 만들 뿐 포커스를 옮기지 않는다. `remote.refusals` 는 `remote.attach` 응답의 `attempt` 로 비동기 거절을 찾는 조회다. `remote.disconnect` 는 mirror 의 연결(에이전트 mirror 와 저장된 원격 행)을 끊고 원격 workspace 는 남긴다. `remote.connect` 는 저장된 원격 행만 연결하고 포커스를 옮기지 않는다. `remote.attach`·`remote.refusals`·`remote.workspaces`·`remote.disconnect`·`remote.connect` 는 App 층이 외부 IPC 로만 처리한다. plugin 이 host-call 로 부르면 게이트는 통과하고 `-32017` 을 받는다. plugin 이 외부 IPC 로 부르면 토큰 없이 접속하므로 Local 로 처리된다 |
| 아직 정하지 않은 것 | `host.shared_buffer.create` | egui-mesh plugin이 프레임 버퍼를 얻는 통로다. `plugin_only` 라 Agent 는 부를 수 없고(`-32016`), 플러그인은 `plugin_callable` 검사만 받는다. `METHOD_TABLE`에 등재해 cap·rate·audit를 적용하지만 개수·총량 상한을 둘지는 아직 정하지 않았다. 이 상한은 별도 결정이라 이 문서의 열린 질문으로 남아 있다 |

앞의 두 분류는 토큰을 요구하지 않기로 정한 정책이다. 공유 버퍼는 개수·총량 제한을
아직 결정하지 않은 항목이므로 구분해서 읽는다.

위 표는 `crates/tasty-doc-guards/tests/permission_free_methods_docs_parity.rs` 가 `METHOD_TABLE` 을 `crates/tasty-ipc/src/method_meta.rs` 에서 읽어 양방향으로 강제한다 — 새 메서드를 `plugin(&[])` 로 등록하면 이 표에도 넣어야 통과한다.
이 검사는 `doc-guards.yml`에서 경로 필터 없이 매 push에 실행된다
([CI 가이드](ci-gates.md)). `tests/method_table_readings_agree.rs`는 텍스트로 읽은
메서드 표를 실제 런타임 목록과 대조한다. 검사기는 어느 근거 분류에 넣을지는 판단하지 않는다.

### `network` — 여는 것 하나

`network` 가 Agent 에게 여는 호스트 IPC 는 `webhook.register` 하나다. plugin 호출자는 토큰 없이 등록할 수 있지만 인라인 `sequence` 를 쓸 수 없고 자기 소유(`<plugin_id>/…`) hook 핸들러 id 만 바인딩할 수 있다. 임의 시퀀스 정의는 owner(Local) 전용 채널로 남는다.

plugin 프로세스가 자기 소켓을 여는 것은 호스트 IPC 가 아니므로 이 토큰과 무관하다. 번들 plugin agent-stream 의 SSE 엔드포인트가 그 예다 — 자기 프로세스에서 TCP 포트를 열고(`agent_stream.serve`), 노출 정책(loopback 기본 · 광역 bind 시 토큰 필수)은 호스트가 아니라 그 plugin 이 스스로 지키는 규약이다([Agent Stream (`com.tasty.agent-stream`)](https://github.com/zilhak/tasty-official-plugins/blob/develop/docs/plugins/agent-stream/index.md)).

## Scope 의 출처 — 동적 이름공간

`ipc.invoke:<X>` 의 `X` 는 호스트 enum 이 아니다. 각 플러그인이 `[[contributes.ipc_namespace]]` 로 prefix 를 선언함으로써 그 scope 이름이 시스템에 존재하기 시작한다. 호스트는 `session.issue` 에서 토큰의 **형식만 검증**하고 **owner 존재는 검증하지 않는다**:

| 검증한다 | 검증 안 한다 |
|----------|--------------|
| 형식 valid / 예약어 아님 | 그 namespace 를 점유한 플러그인이 설치/활성/running 인가 |

owner 미검증의 이유 — **disable/enable 견고성**(owner 가 잠시 꺼져 있어도 세션 발급이 실패하면 안 됨), dangling 호출은 runtime 에 명확히 실패 — 그 namespace 를 **아무도 설치하지 않았으면** `-32601 method not found`, **설치된 owner가 비활성·자동 비활성이거나 기동에 실패하면** `-32002 plugin '<id>' is not running` ([플러그인 제작 가이드](plugin-development.md)). 같은 prefix 는 두 플러그인이 동시에 점유 불가(두 번째 install 거부) — 임의 시점에 scope 는 정확히 한 플러그인에 귀속 또는 무소속.

plugin 은 다른 plugin 의 namespace 를 토큰 없이 부르고, 자기 namespace 를 host-call 로 부르면 self-loop 로 `-32001` 을 받는다. 자식 agent 에게 `ipc.invoke:<prefix>` 를 넘길 때도 쥐고 있을 필요가 없다(아래 [Agent caller](#agent-caller--session-token--temp-grants)).

## 새 IPC 메서드 추가 절차

핸들러를 추가하면 **반드시** `method_meta` 에 매핑 등록:

```rust
"surface.my_new_method" => plugin(&[Permission::SurfaceWrite]),
```

누락 메서드는 `method_meta` 가 `None` → plugin 호출 시 자동 `UnknownMethod` 거부(Local 은 fallthrough 통과). debug/호스트 자체 메서드(`plugin.*`/`window.*`)는 `local_only()`.

**권한은 "무엇을 실제로 건드리는가"로 정한다** — Surface 트리를 건드리면 `Surface*` 가 섞이고, 순수 PTY IO 만 하면 `Terminal*` 만 쓴다. headless PTY primitive(`pty.*`, [터미널 (Terminal)](../surfaces/terminal/index.md))가 이 규칙의 예시다 — 새 `Pty*` 토큰 없이 기존 `Terminal*` 3종만 재사용한다:

| 메서드 | 권한 | Surface 를 건드리는가 |
|--------|------|----------------------|
| `pty.spawn` | `TerminalSpawn` | 아니오 (Surface 없이 PTY 만) |
| `pty.write` / `pty.kill` | `TerminalWrite` | 아니오 |
| `pty.read` / `pty.wait` / `pty.list` | `TerminalRead` | 아니오 |
| `pty.attach_surface` | `SurfaceWrite, TerminalSpawn` | **예** (실제 Tab 생성 — `terminal.spawn` 과 동일 이유로 `SurfaceWrite` 추가) |

**`TerminalWrite` 의 범위는 "지정한 대상에 바이트를 쓴다" 까지다.** 이 토큰으로 열리는 것은 PTY 쓰기(`pty.write`/`pty.kill`)와 대상 surface ID 를 필수로 받는 `surface.send_key` 같은 메서드이며, **OS 전역 입력 조작은 포함하지 않는다.** macOS `surface.raw_key`(`CGEventPost`의 시스템 전역 키 주입)와 `surface.switch_input_source`는 debug 전용 `local_only()`이므로 plugin에서 호출할 수 없다(이 문서). 새 메서드에 `TerminalWrite` 를 붙일 때는 그 동작이 **대상을 ID 로 받는 쓰기** 인지 확인한다 — 아니면 토큰이 아니라 표(release/debug) 선택이 잘못된 것이다.

## 새 권한 토큰 추가

1. `Permission` enum 에 variant 추가(scoped 면 `<Name>(String)`).
2. `from_token`/`as_token` 매핑(scoped 면 `strip_prefix` + scope 검증 함수).
3. `is_valid_<x>` 검증 함수 — **형식만**, owner 존재는 검증 안 함.
4. runtime 게이트(`method_meta`) 연결.
5. 이 문서의 [토큰 전체](#토큰-전체--무엇을-여나) 표 갱신 — `crates/tasty-doc-guards/tests/permission_token_docs_parity.rs` 가 강제하고, `doc-guards.yml` 이 develop·release·main push · PR 마다 그것을 돌린다 — 그 잡에는 경로 필터가 없어 **이 문서만 고친 push 에서도 돈다**([가드 검증](guard-verification.md) · [ci-gates](ci-gates.md)).

`ipc.invoke`/`ext` 두 사례가 reference.

## 매니페스트 기여의 소유 검사

매니페스트 기여(`[[contributes.*]]`·`[extends]`)는 권한 토큰을 요구하지 않는다. 매니페스트 검증은 형식·참조·충돌·중복·예약 namespace 만 확인하고, 활성 plugin 이 선언한 기여는 그대로 동작한다.

`event_subscribe`·`event_publish` 는 권한이 아니라 계약 선언이다. 선언이 덮지 않는 구독·발행은 계약 위반으로 거절한다([구독·발행 선언 패턴](../reference/event-catalog.md#구독발행-선언-패턴)).

`scope = "surface"` 로 선언한 event trigger popup 은 별도 권한 토큰이 없다. 대신 host 가 대상 소유를 확인한다 — 이벤트가 가리킨 surface 가 그 popup 을 선언한 plugin 의 surface 여야 연다. plugin 이 발행한 이벤트라면 대상 유무와 관계없이 발행자도 그 plugin 이어야 한다(`event_publish` 선언은 plugin ID 에 묶이지 않는다). 그 밖에는 열지 않는다. 이 발행자 검사는 `scope = "surface"` popup 에만 적용한다 — `window`(기본) 선언 event trigger popup 은 다른 plugin 이 발행한 이벤트로도 열린다. **발행은 성공하고, 거절은 호스트 경고 로그에만 남는다** — 발행한 plugin 은 응답으로 거절을 알 수 없으므로 팝업이 뜨지 않으면 호스트 로그(`popup '<plugin>/<popup>' refused: …`)를 확인한다([내부 팝업 시스템](../design/systems/popup.md), 배치는 [popup 설계](../design/systems/popup.md#plugin-popup-의-스코프)).

## Agent caller — session token + temp grants

세션 토큰은 요청이 어느 agent 의 것인지 구분하는 표지이고, agent 권한은 협조적인 agent 의 실수를 막는 장치다. 보안 경계가 아니다. 커스텀 plugin에도 별도의 실행 보안 경계를 제품 원칙으로 요구하지 않는다.

Agent는 `session.issue`로 받은 토큰을 `TASTY_SESSION_TOKEN`과 IPC 봉투의 `session_token`으로 전달한다. 잘못됐거나 만료·취소된 토큰은 -32001로 거절하며 Local로 바꾸지 않는다. 권한은 발급 때 정한 base와 만료되지 않은 temp grant의 합이다. MissingPermission은 같은 agent·권한의 Pending 격상을 재사용해 승인 요청을 만든다.

호스트 표에 없는 plugin namespace 호출은 Agent caller에게 ipc.invoke:<prefix>를 요구한다.
agent_id 문자열이 plugin ID와 같아도 면제하지 않는다.
`session.issue`에서 Local과 plugin caller는 어떤 토큰이든 자식 세션에 넘길 수 있다.
agent의 재발급은 받은 권한의 부분집합이다.
표에 정확한 이름이 있는 image.open 같은 host 메서드는 표의 기존 권한을 따른다.
거절된 호출은 plugin 기동 전에 끝나야 한다.
세션 발급 실패 뒤에는 토큰 없이 자식을 띄우며, 그 자식은 Local 로 취급된다. 실수 방지 장치가 빠지는 경로이고 보안 결함은 아니다.

<a id="비-local-caller-가-유발할-수-있는-plugin-수명주기"></a>

## Local 이외 호출자가 plugin을 시작하는 경우

이 문서의 나머지는 "그 호출이 통과하는가" 를 다룬다. 이 절은 **통과한 호출의 부수효과가
plugin 프로세스를 띄우는가** 를 다룬다 — 권한 토큰이 아니라 각 경로가 무엇을 하느냐로
정해지는 축이라, 표를 따로 둔다. 현재 정책은 [플러그인 제작 가이드](plugin-development.md)다.

| 경로 | 필요한 권한 | 기동 범위 |
|---|---|---|
| `plugin.enable` / `plugin.disable` | Local 전용 | Local 이외 호출자 거부 시 0 |
| namespace 호출 권한·cap·rate 거부 | 기존 공통 게이트 | 0 |
| 허용된 namespace 호출 | 기존 METHOD_TABLE/PREFIX_RULES, Agent 는 `ipc.invoke:<prefix>` | 활성 owner, 그리고 실제 실행할 매칭 IPC hook의 active extension만 |
| 미등록 prefix | 기존 unknown-method 오류 | 0 |
| disabled/auto-disabled owner | 기존 not-running 오류 | 0 |
| plugin kind 지목 생성 | Agent 는 `surface.write`, plugin 은 토큰 없음 | 해당 kind의 활성 owner만 |

- namespace 소유는 설치된 manifest의 prefix registry에서 찾는다. `commands`는 전체 IPC 메서드 명부가 아니므로 사전 allowlist로 쓰지 않는다. **등록 prefix 안의 오타는 owner가 시작된 뒤 오류를 반환할 수 있다.** 무관한 plugin은 시작하지 않는다.
- owner를 찾거나 권한을 검사할 때는 기동하지 않는다. 이미 running인 owner/extension은 다시 시작하지 않는다. 매칭 hook이 없거나 self-loop/backoff로 우회되면 그 extension을 기동할 이유도 없다.
- 설치·enable·사용자 설정 저장은 namespace 준비 경로에 없다. 다른 namespace를 호출해 agent-stream의 저장된 SSE가 우연히 재개되는 동작에 의존하지 않는다. 재개하려는 plugin을 명시적으로 사용하거나 Local enable 경로로 시작한다.
- GUI의 첫 창에서 활성 plugin을 시작하는 기존 부팅 정책과 attach mesh mirror의 별도 기동 트리거는 유지한다. **호출로 인한 추가 기동**은 GUI/headless 공통 manager가 같은 범위로 처리한다.
- 권한 해소 순서는 기존 `METHOD_TABLE` → `DEBUG_METHODS` → 정적 `PREFIX_RULES` → 등록 plugin prefix다. 표에 있는 `image.list` 같은 이름의 권한을 `ipc.invoke`로 대체하지 않는다. 거부·cap·rate·허용된 요청의 사용량 집계는 이 문서의 공통 진입 검사를 유지한다.

## Audit log

입장 검사에서 허용된 요청의 사용량 집계와 거절 기록을 한 번씩 수행한다. 디스크에는 Deny만 보존한다. 보존 기간과 한도는 [저장소의 관측 로그 보존](../design/systems/storage.md#관측-로그-보존)을 따른다. 공통 게이트 뒤의 namespace 검증 거절까지 같은 감사 경로에 들어가는지는 별도 확인 대상이다.

### 텔레메트리 기록 정책

로컬 telemetry는 cap·이상 탐지 입력이므로 전체 기록을 끄는 옵션을 제공하지 않는다. 호스트는 이 기록을 외부로 전송하지 않으며 보존량은 제한한다. telemetry 권한 토큰은 Agent의 IPC 허용을 나타내지만 같은 OS 사용자 프로세스의 SQLite 직접 접근까지 차단하지 않는다. plugin 자체 외부 전송도 호스트가 막지 않는다. 기록 없이 cap이 동작하거나 실제 OS 격리·외부 전송 기능이 생기면 기록 및 권한 정책을 다시 검토한다.

### 사용자 입력 재현

사용자 키·마우스·IME·메뉴·포커스 재현은 release에 포함하지 않고 debug 전용 모듈과 표·라우터로 격리한다. OS 전역 raw_key·입력 소스 전환에는 debug에서도 --enable-input-simulation을 요구한다. 내부 IME 상태 시뮬레이션은 debug 격리로 제한한다. 반면 대상 surface ID에 PTY 바이트를 보내는 send_key는 release 에이전트 작업이다. ID만 추가해 실제 OS 수신자를 지정할 수 없는 입력 주입을 release로 승격하지 않는다. 새 플랫폼 API·정당한 기능 요구·이름 기반 검사 오탐이 생기면 원칙에 따라 범위를 다시 판단한다.

## 한계

Agent 권한 게이트는 **호스트 IPC 호출만** 막는다. Agent 나 플러그인이 자기 프로세스에서 `std::fs::write` 로 임의 경로에 쓰면 호스트는 모른다 — 진짜 격리는 OS 샌드박스(seccomp/sandbox-exec/WASM)가 필요하고 현재 범위 밖. 즉 Agent 권한 토큰은 **"호스트 API 호출 권한"** 이지 "OS 자원 권한"이 아니다 — UI/문서에서 권한 상승을 요청할 때 이 표현을 유지해 OS 자원까지 보호한다고 오해하게 하지 않는다.

같은 이유로 plugin·agent 프로세스는 로컬 IPC 포트에 세션 토큰 없이 직접 접속할 수 있고, 그 요청은 Local(사용자)로 처리된다. 현재의 호출 제한을 별도 실행 보안 경계로 설명하지 않는다. 제품 정책은 [플러그인 원칙](../principles/plugins.md)을 따른다.

호스트 호출 판정과 attach 접근 범위는 따로 관리한다. 이 문서의 판정은 호출자가
호스트 IPC를 호출할 수 있는지를 정한다. 플러그인이 그린 화면을 attach로 원격에
전달하는 것은 **SSH+loopback 연결 경계**를 따른다
([IPC 서버](../architecture/ipc-server.md),
[attach IPC](attach-behavior.md#ipc-표면-attach)). SSH 접속 권한으로 이미 원격 파일 등에
접근할 수 있으므로, 렌더 콘텐츠가 추가된다는 이유만으로 권한 토큰을 늘리지 않는다.

예를 들어 egui-mesh attach mirror는 다음 두 곳에서 범위를 제한한다.
markdown 본문은 [webview](../design/systems/webview.md)를 사용하므로 이 채널의 대상이 아니다.

- 서버가 attach 트리를 직렬화할 때 그 kind가 같은 plugin 소유의 egui-mesh로 등록돼 있는지
  `is_registered_egui_mesh_kind`로 다시 확인한다. plugin ID 목록이나 권한 검사는 아니다.
- 입력은 hard 점유를 가진 holder만 전달할 수 있다.
  `CoreState::apply_attached_mesh_input`이 holder를 검사한다.

`StreamControl::MeshContext`/`MeshInput`/`MeshFullResendRequest`/`MeshError`에는
별도의 `Permission::AttachMeshMirror` 토큰을 두지 않는다. `Permission`/`method_meta`의
호스트 호출 권한을 추가하지 않고 위 attach 규칙을 사용한다.
채널 동작은 [mesh mirror](attach-behavior.md#mesh-mirror-채널)와
[mesh 소비 경로](egui-mesh-channel.md#attach-mesh-mirror-소비-경로)를 참고한다.
