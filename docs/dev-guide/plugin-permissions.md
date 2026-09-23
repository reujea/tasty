# 플러그인 권한 모델

플러그인이 호스트 IPC 를 호출할 때 적용되는 권한 게이트의 동작 원리 + 토큰 전체 목록(각 토큰이 실제로 무엇을 여는지) + 새 IPC 메서드/토큰 추가 절차. 권한이 plugin 모델 어디에 놓이는지는 [concepts/plugins](../concepts/plugins.md#권한-permissions), 제작 흐름은 [plugin-development](plugin-development.md).

## 구성 요소

| 위치 | 역할 |
|------|------|
| `crates/tasty-plugin-manifest/src/types.rs::Permission` | 권한 enum + 토큰 매핑(`from_token`/`as_token`). 새 토큰은 여기 |
| `crates/tasty-ipc/src/method_meta.rs::method_meta` | IPC 메서드 → 필요 권한 / plugin 호출 가능 여부 (단일 진실원) |
| `crates/tasty-ipc/src/caller.rs::CallerContext` | 호출자 종류 (Local / Internal / Plugin / Agent) + `ensure_allowed` |
| `src/adapters/ipc/handler/checked.rs::check_request` | 라우터 진입에서 `ensure_allowed` + capability elevation 자동 발행 + audit |
| `crates/tasty-host-plugin/src/manager.rs::plugin_permissions` | plugin id → `Arc<HashSet<Permission>>` 캐시 |
| `crates/tasty-host-plugin/src/registry_state.rs` | `plugins.toml` 의 grant 영속화 |

## 토큰 형식 — `<name>[:<scope>]`

- **`.`** 는 *이름의 일부*다 (`surface.read` 의 `.` 는 분류용 — 호스트는 쪼개지 않음).
- **`:`** 는 *scope 구분자*다 — 권한이 적용되는 대상을 한정해야 의미가 생기는 권한에만 등장.

전체 토큰 목록은 아래 [토큰 전체 — 무엇을 여나](#토큰-전체--무엇을-여나). scoped 검증 규칙:

- `ipc.invoke:<prefix>` — `is_valid_ipc_prefix`(소문자 시작+소문자/숫자/`_`, ≤32) **그리고** 호스트 예약어 거부.
- `ext:<plugin_id>` — `is_valid_plugin_id`(reverse-DNS).
- `file_handler.extend:<id>` / `file_handler.handle:<id>` — `is_valid_detector_id`, `$unknown` 거부.
- `hook_handler.handle:<id>` — `is_valid_hook_handler_id`(소문자+숫자+`-`, ≤32). `$`-prefix reserved 개념 없음. `hook_handler.define` 은 scope 없는 base 토큰.

형식 위반 토큰은 `from_token` 이 `None` → 매니페스트 로드 단계에서 거부.

## 토큰 전체 — 무엇을 여나

`Permission` enum 의 `as_token` 이 토큰 문자열의 단일 출처다(exhaustive match — variant 를 늘리면 팔이 강제된다). 각 토큰이 **실제로 여는 것**은 두 종류다: `method_meta` 가 그 권한을 요구하는 **호스트 IPC 메서드**, 그리고 매니페스트 로드·런타임이 그 토큰 없이는 거부하는 **contribute/호출 게이트**. 둘 다 없는 토큰은 아직 아무것도 열지 않는다 — 표에 그대로 적는다. 한 메서드가 여러 권한을 **함께** 요구할 수 있으므로(`image.paste` 는 `surface.write` + `clipboard.read`) 같은 메서드가 여러 행에 나온다.

| 토큰 | 여는 호스트 IPC | 그 밖의 게이트 |
|------|-----------------|----------------|
| `surface.read` | 트리·메타 조회 (`tree` · `workspace.list` · `tab.list` · `surface.list` · `surface.meta.get` · `hook.list` · `message.read` · `recent.query` 등) | — |
| `surface.write` | 트리 변경 (`workspace.create` · `tab.create` · `tab.close` · `split` · `surface.close` · `surface.set_cwd` · `surface.meta.set` · `hook.set` · `message.send` 등) | — |
| `notification` | `notification.create` · `notification.list` · `surface.completion` · `surface.attention.get` · `surface.attention.clear` | — |
| `clipboard.read` | `image.paste` **하나뿐** | — |
| `clipboard.write` | `clipboard.set_text` | — |
| `fs.read` | `file_handler.dispatch` · `image.open` · `image.reload` · `markdown.navigate` · `git_viewer.query` · `markdown_mirror.content_request` · `file_picker.trigger` | — |
| `fs.write` | `image.save` · `image.export_png` | — |
| `process.spawn` | **없음** — `method_meta` 어느 메서드도 요구하지 않는다 | 없음 |
| `terminal.spawn` | `terminal.spawn` · `terminal.respawn` · `pty.spawn` · `pty.attach_surface` · `surface.wake` · `surface.respawn_terminal` | — |
| `terminal.write` | `surface.send` · `surface.send_key` · `surface.send_to` · `terminal.tell` · `terminal.broadcast` · `pty.write` · `pty.kill` 등 | — |
| `terminal.read` | `surface.read_since_mark` · `surface.screen_text` · `surface.commands` · `output.observe_*` · `pty.read` · `pty.wait` 등 | — |
| `network` | `webhook.register` (아래 "network" 절) | — |
| `memory.read` | `memory.get/list/query/export` · `memory.exists/count/scopes/stats` · `memory.bb_*` 조회 · `memory.plan_*` 조회 · `memory.cache_get/cache_list` · `memory.goal_get` · `approval.summary.get` | — |
| `memory.write` | `memory.put/delete/import` · `memory.bb_*` 변경 · `memory.plan_*` 변경 · `memory.cache_*` · `memory.goal_set/goal_clear` · `approval.summary.set` | — |
| `memory.secret` | `memory.secret.*` 전부 | — |
| `approval` | `approval.*` 전부 · `plugin.request_permission`. `approval.summary.get/set` 은 `memory.read`/`memory.write` 를 함께 요구 | — |
| `telemetry` | `telemetry.*` 전부(기록·조회·cap·anomaly) | — |
| `agent` | `agent.*` 협업 primitive 전부 · `session.issue` · `session.revoke` | — |
| `ui.tool_item` | 없음 | `[[contributes.tool]]` 매니페스트 게이트 + 도구 메뉴 노출 조건(grant 없으면 항목이 뜨지 않음) |
| `ui.popup` | `popup.close` | `[[contributes.popup]]` · `[[contributes.commands]]`(`action.kind = "open_popup"`) 매니페스트 게이트 + popup contribute 노출 조건 |
| `ui.banner` | `banner.open` · `banner.close` | `[[contributes.banner]]` 매니페스트 게이트 |
| `ui.settings_page` | `settings.get_plugin_setting` | `[[contributes.settings_pages]]` 매니페스트 게이트 |
| `window.spawn` | 없음 | `[[contributes.window]]` 매니페스트 게이트. spawn 핸들러 자체가 아직 schema + stub 이라 이 토큰이 여는 실행 경로는 없다 |
| `file_handler.define` | 없음 | `[[contributes.detector]]` 로 **신규** detector id 를 선언할 때 요구 |
| `hook_handler.define` | 없음 | `[[contributes.hook_handler]]` 매니페스트 게이트 |
| `completion_strategy.define` | 없음 | `[[contributes.completion_strategy]]` 매니페스트 게이트 |
| `ipc.invoke:<prefix>` | 그 prefix 를 점유한 plugin 의 namespace 메서드 중 `METHOD_TABLE` 에 이름이 없는 것 전부 — plugin 과 agent 에게 똑같이 | `ensure_allowed` 가 요구(없으면 `-32001`, agent 면 elevation 발행) + plugin→plugin 은 `manager/ipc_dispatch.rs` 가 한 번 더 본다. 소유 plugin 자신의 호출은 면제 |
| `ext:<plugin_id>` | 없음 | `[extends]` 매니페스트 게이트 + grant 되어야 extension 이 활성(`recompute_extensions`) |
| `file_handler.extend:<id>` | 없음 | 기존 detector id 재선언(rule 추가) 시 요구 |
| `file_handler.handle:<id>` | 없음 | `[[contributes.handler]]` 가 그 detector 에 붙을 때 요구 |
| `hook_handler.handle:<id>` | 없음 | **없음** — 형식 검증만 있고 강제하는 지점이 아직 없다(핸들러 id 단위 grant 가시성을 위해 예약된 토큰) |

### 호스트 키 namespace 는 memory 권한으로 열리지 않는다

`memory.read` / `memory.write` 는 regular memory 를 연다. regular memory 는 **설계상 공유
네임스페이스**라 읽기 API 에 owner 차원이 아예 없다 — plugin 별 비공개가 필요하면
`memory.secret` 을 쓴다.

호스트도 자기 상태를 그 공유 네임스페이스에 둔다(감사 로그·telemetry·agent primitive·
approval 등, 키 접두 `tasty.`). 그래서 **접두 `tasty.` 로 시작하는 키는 예약**이고, 권한을
받는 caller(plugin / agent)의 raw kv 표면에서는 존재하지 않는 것으로 다룬다 — 지목하면
거부, 열거하면 결과에서 빠지고, 세는 수에도 안 들어간다. `Local`(CLI·사용자)은 제외라
`tasty memory list --prefix tasty.audit.` 은 그대로 동작한다.

이 예약이 없으면 전용 메서드로 잠근 데이터가 옆문으로 열린다. 감사 로그는
`plugin.audit_*` 가 넷 다 plugin 에게 닫혀 있는데, 같은 행이 `tasty.audit.` 키로 앉아 있어
`memory.read` 로 전부 읽히고 `memory.write` 로 날조됐다. 근거와 대안은
[ADR-0141](../adr/0141-host-key-namespace-is-reserved-in-raw-memory-kv.md).

`memory.bb_*` · `memory.plan_*` · `memory.cache_*` 는 영향받지 않는다 — 자기 접두로만 키를
조립하는 전용 메서드라 raw kv 를 거치지 않는다.

### 선언 범위와 실제 개방 범위는 같지 않다

토큰 이름은 넓은 범주를 가리키지만, 그 토큰이 지금 여는 호스트 IPC 는 표의 가운데 열이 전부다. 특히:

- **`clipboard.read` 는 `image.paste` 만 연다.** 클립보드 내용을 읽는 번들 plugin(clipboard-viewer)은 호스트를 거치지 않고 자기 프로세스에서 직접 OS 클립보드를 읽는다([ADR-0026](../adr/0026-clipboard-history-removal-plugin-direct-read.md)) — 그 읽기는 이 토큰이 통제하지 않는다.
- **`process.spawn` 과 `window.spawn` 은 아무 IPC 도 열지 않는다.** 전자는 요구하는 메서드가 없고, 후자는 매니페스트 게이트만 있다.
- **`hook_handler.handle:<id>` 는 강제 지점이 없다.** 매니페스트에 적을 수 있고 형식 검증도 받지만, 그 토큰 유무로 갈리는 동작이 아직 없다.

토큰을 넓은 이름 그대로 읽으면 grant 화면이 실제보다 큰 권한을 넘기는 것처럼 보인다. 반대 방향의 오해가 더 중요하다 — **plugin 프로세스가 자기 손으로 하는 일은 어느 토큰도 막지 못한다**(아래 [한계](#한계)).

### 토큰 없이도 열려 있는 메서드

위 표는 **토큰을 축으로** 하므로, 어느 토큰도 요구하지 않는 메서드는 어느 행에도 나타나지 않는다. `method_meta` 에는 그런 항목이 있다 — `plugin_callable` 이면서 `required` 가 빈 22개다. **매니페스트에 `permissions` 를 하나도 적지 않은 plugin 도 이것들은 부를 수 있다.** 앞 절이 "토큰이 이름보다 좁게 연다" 는 방향이라면 이것은 반대편이다.

근거가 **세 갈래**다. 한 덩어리로 적으면 "권한 없이 열린 건 전부 SSH 때문" 으로 잘못 일반화되므로 군을 나눈다. 특히 셋째 군은 앞의 둘과 성질이 다르다 — **결정이 아니라 미결**이다.

| 군 | 메서드 | 왜 토큰을 요구하지 않나 |
|----|--------|------------------------|
| 전역·자기 정보 조회 | `system.info` · `theme.query` · `plugin.list_agent_permissions` | 상태를 바꾸지 않고 특정 surface 의 내용도 노출하지 않는다. system.info의 version은 전역이고 count/index는 소유 workspace ID가 명시된 engine 관측값이다. theme.query는 테마의 전역 스냅샷이고(webview-kind surface 는 `set_context` 로 Theme 를 push 받지 못해 문서를 그릴 때마다 `theme.query` 로 대신한다 — [ADR-0065](../adr/0065-markdown-webview-render-channel.md)), 마지막은 **자기에게 지금 무슨 권한이 있는지**를 되읽는 self-introspection 이라 토큰을 요구하면 순환이 된다 |
| attach · 원격 프로필 · 패스키 | `attach.acquire` · `attach.release` · `attach.force_detach` · `attach.force_detach_workspace` · `attach.into_gui` · `attach.list` · `remote.profile.list` · `remote.profile.get` · `remote.profile.add` · `remote.profile.detect` · `remote.profile.remove` · `remote.profile.list_local` · `remote.profile.import` · `remote.workspaces` · `remote.passkey.list` · `remote.passkey.get` · `remote.passkey.add` · `remote.passkey.remove` | 신뢰경계가 이 권한 모델이 아니라 **연결 경계(SSH + loopback)** 다 — 근거는 아래 [한계](#한계) 의 attach 문단에 있고 여기서 되풀이하지 않는다. 프로필은 비밀 없는 장비 인벤토리이고, 원격 워크스페이스 열거도 같은 조회이며(구조를 만드는 `remote.attach` 는 이 군에 **없다** — 사용자의 로컬 창에 워크스페이스를 만드는 것은 SSH 가 주는 권한이 아니라서 local 전용이다, [ADR-0121](../adr/0121-attach-trust-boundary-covers-remote-queries-not-local-structural-ops.md)), 패스키도 `list`/`get` 이 이름과 종류만 돌려주며 키 파일 내용은 반환하지 않는다([ADR-0016](../adr/0016-passkey-store-path-convergence.md)) |
| 아직 정하지 않은 것 | `host.shared_buffer.create` | **정책이 아니다.** egui-mesh 로 그리는 plugin 이 프레임 버퍼를 얻는 통로이고, 요구 토큰이 없는 것은 그렇게 정해서가 아니라 **정한 적이 없어서**다. 원래는 `METHOD_TABLE` 에 등재조차 없어 게이트가 이름을 못 찾았고, 그래서 권한뿐 아니라 cap·rate·audit 도 통째로 건너뛰었다. 지금은 현재 동작 그대로 등재해 최소한 그 셋은 걸리게 해 둔 상태다. 어떤 토큰을 요구할지와 개수·총량 상한을 함께 둘지는 매니페스트 호환성이 걸린 별도 결정이라 [ADR-0152](../adr/0152-gates-run-before-routing-not-inside-it.md) 의 열린 질문으로 남아 있다 |

**앞의 두 군은 구멍이 아니라 정책이다.** 그 둘은 "게이트를 빠뜨렸다" 가 아니라 그 자리에 게이트를 두지 않기로 한 결정이고, 특히 아래쪽 군은 SSH 접속 권한이 이미 그 이상을 허용하므로 별도 토큰을 만들지 않는다는 이 레포의 확립된 판단이다([ADR-0004](../adr/0004-ipc-transport-tcp.md)). 다만 그 사실이 **문서에 있어야** grant 화면을 보는 사용자와 매니페스트를 쓰는 plugin 작성자가 실제 개방 범위를 안다. 셋째 군은 그 반대다 — 결정을 기다리는 자리이므로, 같은 표에 있다는 것이 같은 근거를 갖는다는 뜻이 되지 않게 군을 갈라 둔다.

위 표는 `crates/tasty-doc-guards/tests/permission_free_methods_docs_parity.rs` 가 `METHOD_TABLE` 을 `crates/tasty-ipc/src/method_meta.rs` 에서 읽어 양방향으로 강제한다 — 새 메서드를 `plugin(&[])` 로 등록하면 이 표에도 넣어야 통과한다. 이 가드는 의존이 0 인 크레이트에 살아 **`doc-guards.yml` 이 경로 필터 없이 매 push 실행한다** — 이 표를 고치는 것이 곧 이 가드를 위반하는 유일한 방법이라, 문서만 바뀐 push 에서도 도는 것이 요점이다([ci-gates](ci-gates.md) · ADR-0138). 그 판독이 실제 표와 갈리지 않는지는 본체 패키지의 `tests/method_table_readings_agree.rs` 가 런타임 열거와 대조해 붙박는다. 다만 **어느 군에 넣을지는 가드가 판정하지 않는다**(근거의 분류라 기계가 고를 값이 아니다).

### `network` — 여는 것 하나 + 정직한 선언

`network` 가 여는 호스트 IPC 는 하나다.

- `webhook.register` — plugin 은 이 권한으로 인바운드 웹훅을 등록하되 인라인 `sequence` 는 쓸 수 없고 자기 소유(`<plugin_id>/…`) hook 핸들러 id 만 바인딩할 수 있다 — 임의 시퀀스 정의는 owner(Local) 전용 채널로 남는다.

그와 별개로 이 토큰은 **호스트가 강제할 수 없는 네트워크 사용을 사용자에게 알리는 선언**으로도 쓴다. 권한 게이트는 호스트 IPC 호출만 막으므로, plugin 프로세스가 자기 소켓을 여는 것은 어느 토큰으로도 통제되지 않는다. 그래도 매니페스트에 `network` 를 적으면 사용자가 **grant 시점에** 그 사실을 본다. 번들 plugin agent-stream 이 이 용법이다 — SSE 엔드포인트가 자기 프로세스에서 TCP 포트를 열고(`agent_stream.serve`), 노출 정책(loopback 기본 · 광역 bind 시 토큰 필수)은 호스트가 아니라 그 plugin 이 스스로 지키는 규약이다([ADR-0100](../adr/0100-agent-stream-sse-endpoint-exposure.md)). `fs.read` / `fs.write` 를 직접 파일 접근에 대해 정직하게 선언하는 것과 같은 관례다.

즉 `network` 는 두 의미를 겸한다: **호스트가 강제하는 것**(`webhook.register` 호출 자격)과 **호스트가 강제하지 못해 선언으로만 남는 것**(plugin 프로세스의 소켓). 표의 가운데 열은 전자만 센다.

## Scope 의 출처 — 동적 이름공간

`ipc.invoke:<X>` 의 `X` 는 호스트 enum 이 아니다. 각 플러그인이 `[[contributes.ipc_namespace]]` 로 prefix 를 선언함으로써 그 scope 이름이 시스템에 존재하기 시작한다. 호스트는 매니페스트 parse 시 **형식만 검증**하고 **owner 존재는 검증하지 않는다**:

| 검증한다 | 검증 안 한다 |
|----------|--------------|
| 형식 valid / 예약어 아님 | 그 namespace 를 점유한 플러그인이 설치/활성/running 인가 |

owner 미검증의 이유 — **install 순서 무관성**(B 가 A 보다 늦게 깔려도 A 매니페스트가 거부되면 안 됨), **disable/enable 견고성**, dangling 호출은 runtime 에 명확히 실패 — 그 namespace 를 **아무도 설치하지 않았으면** `-32601 method not found`, **설치된 owner가 비활성·자동 비활성이거나 기동에 실패하면** `-32002 plugin '<id>' is not running` ([ADR-0173](../adr/0173-namespace-resolution-reads-the-manifest-not-the-process-table.md)). 같은 prefix 는 두 플러그인이 동시에 점유 불가(두 번째 install 거부) — 임의 시점에 scope 는 정확히 한 플러그인에 귀속 또는 무소속.

**자기 namespace `ipc.invoke:<self>` 는 매니페스트에 두지 않는다.** plugin 자신의 호출에는 필요 없고(소유자 면제 — plugin→plugin forward 경로의 self-loop 는 `-32001` 로 차단), 자식 agent 에게 넘길 때도 쥐고 있을 필요가 없다(아래 [Agent caller](#agent-caller--session-token--temp-grants)).

## 새 IPC 메서드 추가 절차

핸들러를 추가하면 **반드시** `method_meta` 에 매핑 등록:

```rust
"surface.my_new_method" => plugin(&[Permission::SurfaceWrite]),
```

누락 메서드는 `method_meta` 가 `None` → plugin 호출 시 자동 `UnknownMethod` 거부(Local 은 fallthrough 통과). debug/호스트 자체 메서드(`plugin.*`/`window.*`)는 `local_only()`.

**권한은 "무엇을 실제로 건드리는가"로 정한다** — Surface 트리를 건드리면 `Surface*` 가 섞이고, 순수 PTY IO 만 하면 `Terminal*` 만 쓴다. headless PTY primitive(`pty.*`, [ADR-0050](../adr/0050-headless-pty-primitive.md))가 이 규칙의 예시다 — 새 `Pty*` 토큰 없이 기존 `Terminal*` 3종만 재사용한다:

| 메서드 | 권한 | Surface 를 건드리는가 |
|--------|------|----------------------|
| `pty.spawn` | `TerminalSpawn` | 아니오 (Surface 없이 PTY 만) |
| `pty.write` / `pty.kill` | `TerminalWrite` | 아니오 |
| `pty.read` / `pty.wait` / `pty.list` | `TerminalRead` | 아니오 |
| `pty.attach_surface` | `SurfaceWrite, TerminalSpawn` | **예** (실제 Tab 생성 — `terminal.spawn` 과 동일 이유로 `SurfaceWrite` 추가) |

**`TerminalWrite` 의 범위는 "지정한 대상에 바이트를 쓴다" 까지다.** 이 토큰으로 열리는 것은 PTY 쓰기(`pty.write`/`pty.kill`)와 대상 surface ID 를 필수로 받는 `surface.send_key` 같은 메서드이며, **OS 전역 입력 조작은 포함하지 않는다.** 한때 macOS `surface.raw_key`(`CGEventPost` 로 시스템 전역 키 주입)와 `surface.switch_input_source` 가 같은 토큰으로 열려 있어 토큰 이름이 함의하는 것보다 능력이 넓었으나, 두 메서드는 debug 전용 `local_only()` 로 옮겨져 plugin 에서 호출 자체가 불가능하다([ADR-0115](../adr/0115-input-reproduction-ipc-debug-isolation.md)). 새 메서드에 `TerminalWrite` 를 붙일 때는 그 동작이 **대상을 ID 로 받는 쓰기** 인지 확인한다 — 아니면 토큰이 아니라 표(release/debug) 선택이 잘못된 것이다.

## 새 권한 토큰 추가

1. `Permission` enum 에 variant 추가(scoped 면 `<Name>(String)`).
2. `from_token`/`as_token` 매핑(scoped 면 `strip_prefix` + scope 검증 함수).
3. `is_valid_<x>` 검증 함수 — **형식만**, owner 존재는 검증 안 함.
4. runtime 게이트(`method_meta` 또는 manager) 배선.
5. 이 문서의 [토큰 전체](#토큰-전체--무엇을-여나) 표 + [concepts/plugins](../concepts/plugins.md#권한-permissions) 나열 갱신 — `crates/tasty-doc-guards/tests/permission_token_docs_parity.rs` 가 둘 다 강제하고, `doc-guards.yml` 이 main push · PR 마다 그것을 돌린다 — 그 잡에는 경로 필터가 없어 **이 문서만 고친 push 에서도 돈다**([ADR-0138](../adr/0138-doc-guards-live-in-a-dependency-free-crate.md) · [ci-gates](ci-gates.md)).

`ipc.invoke`/`ext` 두 사례가 reference.

## contributes 권한 게이트

IPC 외에 일부 contribute 는 권한을 강제(매니페스트 로드 단계 거부):

이 표는 위 [토큰 전체](#토큰-전체--무엇을-여나) 와 **방향이 반대인 색인**이다 — 위는 `토큰 → 여는 것`,
여기는 `contributes 항목 → 요구 토큰`.

게이트 목록의 단일 출처는 `crates/tasty-plugin-manifest/src/gates.rs` 의 `ContributesGate` 표다.
`validate.rs` 의 검증 코드는 토큰 문자열을 그 표에서만 가져오고, `crates/tasty-doc-guards/tests/contributes_gate_docs_parity.rs`
가 표와 아래 행을 **양방향으로** 대조한다 — 코드에만 있는 게이트도, 문서에만 남은 행도 깨진다. 채널은 [ci-gates](ci-gates.md) 가 정본이다.

| contributes | 요구 권한 |
|-------------|-----------|
| `[[contributes.tool]]` | `ui.tool_item` |
| `[[contributes.popup]]` | `ui.popup` |
| `[[contributes.commands]]` (`action.kind = "open_popup"`) | `ui.popup` |
| `[[contributes.banner]]` | `ui.banner` |
| `[[contributes.settings_pages]]` | `ui.settings_page` (카테고리 무관) |
| `[[contributes.window]]` | `window.spawn` |
| `[extends]` | `ext:<target>` |
| `[[contributes.detector]]` (신규 id) | `file_handler.define` |
| `[[contributes.detector]]` (기존 id 재선언) | `file_handler.extend:<id>` |
| `[[contributes.handler]]` | `file_handler.handle:<detector>` |
| `[[contributes.hook_handler]]` | `hook_handler.define` |
| `[[contributes.completion_strategy]]` | `completion_strategy.define` |

`event_subscribe` 는 별도 권한 없음 — 패턴 자체가 게이트.

### 새 게이트 추가

1. `gates.rs` 의 `contributes_gates!` 에 행 추가 — `enum` · `ALL` · `contributes_key` · `token` 이
   그 한 행에서 함께 생성되므로 넷이 어긋날 수 없다.
2. `validate.rs` 의 해당 검증 함수에서 `ContributesGate::<Variant>.required(...)` 로 토큰을 얻어
   `permissions[]` 대조 — 토큰 리터럴을 그 자리에 다시 적지 않는다.
3. 위 표에 행 추가. 빠뜨리면 parity 가드가 깨진다.

## Builtin 자동 grant

번들 플러그인 매니페스트 권한은 `install_builtins_if_needed` 가 자동 grant — 최초는 전체, 기존 사용자에 새 버전이 토큰을 추가하면 `apply_builtin_permission_diff` 가 **신규 토큰만 증분** grant(기존 deny 보존).

## 권한 변경 즉시 반영

grant/revoke → `plugins.toml` 저장 → `refresh_plugin_permissions` 가 (매니페스트 ∩ granted)를 새 `Arc<HashSet>` 로 교체. `CallerContext::Plugin` 이 호출 시점에 `Arc::clone` 을 쥐므로 호출 도중 갱신돼도 일관(snapshot semantics).

## Agent caller — session token + temp grants

`claude.spawn` 같은 호스트-launched 자식은 `session.issue` 로 64-char hex 토큰을 받아 `TASTY_SESSION_TOKEN` 으로 전달, IPC envelope 의 `session_token` 으로 첨부 → `CallerContext::Agent`. invalid/expired/revoked 는 `-32001` 즉시 거부(Local fallback 안 함 — 환경변수 위조 방어).

- **base_permissions**: `session.issue` 시점 고정. caller 권한의 부분집합만(escalation 방지). **예외 하나** — plugin 프로세스 caller 는 **자기가 점유한 namespace** 의 `ipc.invoke:<prefix>` 를 쥐지 않고도 넘긴다. 자기 namespace 의 주인이 입장권을 나눠 주는 것이고, agent 는 이 예외를 받지 않는다(`agent_id` 는 발급자가 고른 문자열이라 plugin id 로 지을 수 있다). [ADR-0271](../adr/0271-a-plugin-namespace-is-invoked-with-its-token-from-every-gated-caller.md)
- **plugin namespace 는 토큰으로 연다.** 표에 없는 plugin namespace 이름은 agent 에게도 `ipc.invoke:<prefix>` 를 요구한다. 번들 claude plugin 이 자식 Claude 에게 주는 토큰은 `surface.read` · `surface.write` · `terminal.read` · `terminal.write` · `notification` · `telemetry` · `agent` · `ipc.invoke:claude`(완료 훅 · 손자 spawn·tell) · `ipc.invoke:codex`(교차 검증)다. 뒤엣것은 남의 namespace 라 claude plugin 매니페스트가 선언해 쥐고 넘긴다.
- **temp_grants**: runtime `plugin.grant_agent_permission` 추가, 만료 lazy evict. `effective = base ∪ non-expired temp`.

**Capability elevation 자동 발행**: Agent 가 `MissingPermission` 으로 거부되면 `approval.request{kind:capability_elevation}` 발행(같은 (agent,permission) Pending 은 approval_id 재사용). `approve`(TTL grant) / `approve_permanently`(무기한) / `deny`.

## 비-Local caller 가 유발할 수 있는 plugin 수명주기

이 문서의 나머지는 "그 호출이 통과하는가" 를 다룬다. 이 절은 **통과한 호출의 부수효과가
plugin 프로세스를 띄우는가** 를 다룬다 — 권한 토큰이 아니라 각 경로가 무엇을 하느냐로
정해지는 축이라, 표를 따로 둔다. 현재 정책은 [ADR-0282](../adr/0282-namespace-invocation-starts-only-its-owner-and-matching-extension.md)다.

| 경로 | 필요한 권한 | 기동 범위 |
|---|---|---|
| `plugin.enable` / `plugin.disable` | Local 전용 | 비-Local 거부 시 0 |
| namespace 호출 권한·cap·rate 거부 | 기존 공통 게이트 | 0 |
| 허용된 namespace 호출 | 기존 METHOD_TABLE/PREFIX_RULES 또는 `ipc.invoke:<prefix>` | 활성 owner, 그리고 실제 실행할 매칭 IPC hook의 active extension만 |
| 미등록 prefix | 기존 unknown-method 오류 | 0 |
| disabled/auto-disabled owner | 기존 not-running 오류 | 0 |
| plugin kind 지목 생성 | `surface.write` | 해당 kind의 활성 owner만 |

- namespace 소유는 설치된 manifest의 prefix registry에서 찾는다. `commands`는 전체 IPC 메서드 명부가 아니므로 사전 allowlist로 쓰지 않는다. **등록 prefix 안의 오타는 owner가 시작된 뒤 오류를 반환할 수 있다.** 무관한 plugin은 시작하지 않는다.
- owner를 찾거나 권한을 검사할 때는 기동하지 않는다. 이미 running인 owner/extension은 다시 시작하지 않는다. 매칭 hook이 없거나 self-loop/backoff로 우회되면 그 extension을 기동할 이유도 없다.
- 설치·enable·grant·사용자 설정 저장은 namespace 준비 경로에 없다. 다른 namespace를 호출해 agent-stream의 저장된 SSE가 우연히 재개되는 동작에 의존하지 않는다. 재개하려는 plugin을 명시적으로 사용하거나 Local enable 경로로 시작한다.
- GUI의 첫 창에서 활성 plugin을 시작하는 기존 부팅 정책과 attach mesh mirror의 별도 기동 트리거는 유지한다. **호출로 인한 추가 기동**은 GUI/headless 공통 manager가 같은 범위로 처리한다.
- 권한 해소 순서는 기존 `METHOD_TABLE` → `DEBUG_METHODS` → 정적 `PREFIX_RULES` → 등록 plugin prefix다. 표에 있는 `image.list` 같은 이름의 권한을 `ipc.invoke`로 대체하지 않는다. 거부·cap·rate·허용 관측은 [ADR-0277](../adr/0277-ipc-admission-and-observation-run-once.md)의 공통 진입 검사를 유지한다.

## Audit log

공통 `check_request`가 거부 audit과 허용 관측을 요청마다 한 번 수행한다. 허용 호출은 telemetry의 `ipc_calls`에 기록된다. `audit::record`는 기존 [ADR-0085](../adr/0085-ipc-log-retention-bounded.md) 정책에 따라 Deny만 `tasty.audit.{ts}.{seq}` Global scope에 영속한다. `plugin.audit_query/summary/follow/clear`로 조회한다. 라우터 조기 반환도 관측을 생략하지 않는다([ADR-0277](../adr/0277-ipc-admission-and-observation-run-once.md)).

## 한계

권한 게이트는 **호스트 IPC 호출만** 막는다. 플러그인이 자기 프로세스에서 `std::fs::write` 로 임의 경로에 쓰면 호스트는 모른다 — 진짜 격리는 OS 샌드박스(seccomp/sandbox-exec/WASM)가 필요하고 현재 범위 밖. 즉 매니페스트 `permissions[]` 는 **"호스트 API 호출 권한"** 이지 "OS 자원 권한"이 아니다 — UI/문서에서 grant 요청 시 이 표현을 유지해 false security 를 만들지 않는다.

**이 문서의 권한 모델과 attach 의 신뢰 모델은 서로 다른 축이라 섞지 않는다.** 이 문서는 "플러그인이 호스트 IPC 를 호출할 수 있는가"만 통제한다. 플러그인이 그린 화면(렌더 결과)이 attach 로 원격에 얼마나 노출되는지는 이 권한 모델과 무관하게 **SSH+loopback 연결 경계**([ADR-0004](../adr/0004-ipc-transport-tcp.md), [attach-behavior "IPC 표면"](attach-behavior.md#ipc-표면-attach))에 이미 위임돼 있다 — attach 로 새 콘텐츠(예: 플러그인 렌더)를 노출하는 기능을 설계할 때, "더 민감해 보이니 이 권한모델에 신규 토큰을 추가해야 한다"고 판단하지 않는다. SSH 접속 권한은 이미 그 이상(임의 파일 접근 등)을 허용하기 때문이다.

**구현 사례 — mesh mirror**: bundled egui-mesh surface(image/mesh_demo — markdown 은 [ADR-0065](../adr/0065-markdown-webview-render-channel.md) 로 webview 전환되어 이 채널 대상에서 제외됨)의 attach mirror([attach-behavior "mesh mirror 채널"](attach-behavior.md#mesh-mirror-채널), [egui-mesh-channel "attach mesh mirror 소비 경로"](egui-mesh-channel.md#attach-mesh-mirror-소비-경로))는 위 원칙을 그대로 따른 결과다 — 렌더 콘텐츠를 원격으로 흘려보내는 새 채널(`StreamControl::MeshContext`/`MeshInput`/`MeshFullResendRequest`/`MeshError`)을 추가하면서도 이 문서의 `Permission`/`method_meta`엔 어떤 신규 토큰도 추가하지 않았다. 노출 범위 통제는 오직 **①** 기존 화이트리스트(`is_egui_mesh_allowed` — 이 채널 자체의 개방 정책, plugin permission 과 무관)를 서버가 attach 트리 직렬화 시점에 재검증하는 것과 **②** attach 의 holder 점유 모델(hard 점유 = 입력 forward 수신 자격, `CoreState::apply_attached_mesh_input` 의 holder 검증)뿐이다 — "화면을 그리는 콘텐츠니 더 민감하다"는 이유로 별도 `Permission::AttachMeshMirror` 류 토큰을 만들지 않았다.
</content>
