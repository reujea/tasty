# Event Bus 1.0 카탈로그

호스트와 plugin이 주고받는 이벤트의 공개 형식과 호환성 규칙이다. 구독·발행 방법은
[plugins 개념](../concepts/plugins.md)을 참고한다. 이 문서는 구조를 요약하며,
정확한 payload는 `tasty_plugin_protocol::events::payloads`의 Rust 타입을 기준으로 한다.

## Envelope

```jsonc
{
  "key": "surface.focused",
  "payload": { "surface_id": 42, "prev_surface_id": 7 },
  "meta": { "trace_id": "trace-abc", "hop": 0, "origin": { "kind": "host" }, "scope": "surface" }
}
```

| 필드 | 의미 |
|------|------|
| `key` | `<namespace>.<event_name>`. 예약 네임스페이스는 호스트만 publish |
| `payload` | 이벤트별(아래 카탈로그) |
| `meta.trace_id` | chain 전체 공유 opaque id. 호스트가 이벤트를 만들 때 생성. 재발행에서 이어지는 것은 plugin 이 받은 값을 실어 보낼 때다 — 호스트는 이 값을 고치지 않는다 |
| `meta.hop` | host는 0. plugin publish는 미응답 dispatch의 최대 hop+1과 보낸 값 중 큰 값을 사용하며 16 초과는 거절한다. 아래 재발행 규칙 참조 |
| `meta.origin` | `{kind:host}` 또는 `{kind:plugin, plugin_id}` |
| `meta.scope` | `system`(전역) 또는 `surface`(대상 id 는 payload 필드로) |

- **Lifecycle `reason`** (종료 계열 `*.closed` / `plugin.unloaded`): `user`(사용자 직접) / `ipc`(CLI·plugin 자동화) / `crash`(비정상). cascade 별도 분류 없음 — 부모를 닫은 주체가 자식 reason 에 전파.
- **쓰로틀**: `surface.resized`(키별), `split.ratio_changed`(group 별)는 150ms leading+trailing. drag 시작·종료엔 무조건 1 회.

### 재발행과 응답

host는 성공적으로 보낸 event.dispatch의 request ID·hop을 응답까지 보관한다.
publish 도착 때 하한을 적용하고 hook 처리 뒤까지 미루지 않는다. trace_id는 plugin이 전달한 값을 유지한다.
응답은 기록을 한 번 소비하며 plugin 정지·재시작은 전부 정리한다.
plugin당 최대 1024 건을 유지하고 넘으면 오래된 기록부터 버린다.

SDK는 on_event 뒤에 응답한다. callback 안 재발행은 이 하한을 적용받지만 먼저 응답하고 나중에 발행하는 루프는 막지 못한다.
처리 중 무관한 publish도 hop이 올라갈 수 있고, 응답하지 않으면 높은 하한이 오래 남아 정상 publish도 거절될 수 있다.
plugin은 event.dispatch에 응답해야 한다. SDK callback·응답 순서와 host pump의 사건 우선 처리를 함께 유지한다.

## 지나간 사건 — 위치로 읽는다

버스는 debug·release에서 같은 메모리 ring을 사용한다.
보존은 EVENT_RING_CAPACITY(1024 건)와 EVENT_RING_BYTES_LIMIT(16MiB 직렬화 길이) 중 먼저 닿는 쪽으로 제한한다.
단 가장 새 사건 하나는 크기와 무관하게 남기므로 16MiB를 절대 메모리 상한으로 보면 안 된다.
JSON Value 메모리와 직렬화 길이도 다르다. 길이는 버스 lock 밖에서 buffer 없이 센다.
이 보존과 plugin subscription fan-out은 별개이며 fan-out은 승인된 사건을 모두 전달한다.

| 응답 | 의미 |
|------|------|
| events·next_offset | 각 사건의 offset과 다음 조회 위치 |
| epoch | 버스 세대. 재시작하면 기록과 offset이 초기화되므로 함께 보존한다 |
| truncated·skipped | 요청 위치보다 앞선 사건이 이미 제거돼 건너뛴 개수 |
| ahead_of_stream·stream_end | 요청 offset이 현재 끝보다 큼. 끝과 같으면 정상 대기 위치 |

서버는 consumer별 cursor를 보관하지 않는다. 같은 인자로 반복해도 cursor 부작용은 없지만
새 사건과 eviction 때문에 답은 달라질 수 있다. 사건은 디스크에 저장하지 않는다.
epoch는 정상 시계에서는 버스 생성의 wall-clock nanos, UNIX_EPOCH 이전이면 OS random-seeded hash와 PID·시간 차로 만든다.
영구 고유성을 보장하는 ID는 아니다.

`events.fetch {offset,max,filter,wait_ms}`는 Local 전용이다. token Agent·Plugin은 사용할 수 없고 Plugin은 subscription을 쓴다.
필터는 정확 key 또는 namespace.*이며 조건변수로 최대 60 초 대기한다. 관심 없는 사건으로 깨어나면 남은 시간만 기다린다.
미래 위치도 요청한 대기를 유지하고 next_offset을 임의 변경하지 않는다. 즉시 점검하려면 `wait_ms=0`으로 조회한다.
개별 대기 상한은 동시 요청 thread 수 상한이 아니다.

### CLI follow와 재연결

`tasty events follow`는 offset과 epoch를 함께 관리한다. --epoch로 이전 세대를 지정할 수 있다.
연결마다 첫 요청은 `wait_ms=0`이며 epoch 변경·ahead_of_stream이면 그 답의 사건을 출력하지 않고 0부터 다시 읽는다.
전송 실패 때는 기본적으로 재부착할 --offset·--epoch 인자를 stderr에 쓰고 종료한다.
--reconnect일 때만 1 초마다 discovery 파일을 다시 읽고 연결한다. host가 반환한 오류는 전송 단절과 구별한다.
stdout은 사건 JSON 줄만, 유실·세대·연결 통지는 stderr로 쓴다.

epoch 없이 새 세대의 끝이 이미 옛 offset을 넘었다면 재시작을 구별하지 못한다.
같은 세대에서 손으로 미래 offset을 지정해도 초기화 후 과거 사건이 다시 나올 수 있다.
consumer가 cursor와 중복 처리를 책임진다. feed로 사용자 key·mouse 원문이나 화면 복원을 구현하지 않는다.

<a id="예약-네임스페이스-호스트만-발화"></a>

## 예약 네임스페이스 (호스트만 발행)

```
system, surface, tab, pane, workspace, window, command, ime, split,
notification, hook, tool, plugin, extension, process, clipboard, theme, language, memory,
agent
```

이 외는 plugin 자유 — 관례상 자기 `id`(`com.tasty.claude.*`)를 네임스페이스로.

## 안정성 등급

- **Stable** — 키·필수 필드는 바꾸지 않고 옵션 필드만 추가하는 것이 기본이다. 깨야 할 때는 아래 [후속 변경 정책](#후속-변경-정책)을 따른다.
- **Experimental** — 개발자 판단에 따라 변경 가능. **경고일 뿐 구독 조건이 아니다** — 구독 조건은 등급과 무관하게 매니페스트 `event_subscribe` 패턴이 요청 패턴을 덮는가 하나다.
- **Internal** — debug 빌드 전용.

## 카탈로그

### Surface (scope=surface)
| 키 | 시점 | payload | 등급 |
|----|------|---------|------|
| `surface.created` | 생성 직후 | `surface_id, kind, tab_id, pane_id, workspace_id, created_by` | Stable |
| `surface.closed` | 종료 직전 | `surface_id, kind, reason` | Stable |
| `surface.focused` | 포커스 | `surface_id, prev_surface_id?` | Stable |
| `surface.resized` | 크기 변경(쓰로틀) | `surface_id, width_px, height_px` | Stable |
| `surface.title_changed` | 표시 이름 변경 | `surface_id, title` | Stable |
| `surface.refresh_requested` | 사용자가 공통 새로고침 실행 | `surface_id, kind, trigger(shortcut/palette)` | Experimental |
| `surface.webview_created` | `lazy_webview` kind 의 WebView 를 만든 직후, kind 를 등록한 plugin 에만(unicast) | `surface_id` | Experimental |

`created_by`: `{kind:user}` 또는 `{kind:agent, source_plugin}`.

`surface.refresh_requested` 는 사용자가 [공통 새로고침](../features/keybindings/index.md#공통-새로고침--refresh)을 실행했을 때 키를 누른 시점의 대상 surface 로 한 번 발행한다. 에이전트·IPC 경로는 만들지 않으므로 항상 사용자 요청이다. `kind` 는 키를 누른 시점의 kind 이고 host 는 전달 직전에 같은 세대·kind 인지 확인해 닫기·변환·재생성된 대상에는 보내지 않는다. scope=surface 라도 소유 plugin 에만 가는 것이 아니라 `event_subscribe` 가 덮는 모든 구독자에게 가므로 수신자는 `surface_id`(필요하면 `kind`)가 자기 문서인지 확인한다. 발행은 요청 접수이며 갱신 성공을 뜻하지 않는다.

`surface.webview_created` 는 구독 없이 kind 를 등록한 plugin 에 간다. 그 surface 에 만들기 전 보낸 `webview.post_message` 는 버려졌으므로 받은 plugin 은 문서 전체를 다시 싣는다([WebView 생성 시점](../design/systems/webview.md#생성-시점--미리-만들기와-처음-보일-때-만들기)).

### Tab / Pane / Split (scope=system)
| 키 | payload |
|----|---------|
| `tab.created` | `tab_id, pane_id, workspace_id, kind` |
| `tab.closed` | `tab_id, pane_id, reason` |
| `tab.focused` | `tab_id, pane_id, prev_tab_id?` |
| `tab.moved` | `tab_id, from_pane, to_pane` |
| `tab.renamed` | `tab_id, title` |
| `pane.created` | `pane_id, parent_pane_group?, workspace_id` |
| `pane.closed` | `pane_id, reason` |
| `pane.split` | `original_pane, new_pane, direction` |
| `split.ratio_changed` (쓰로틀) | `group_id, level(pane/surface), ratio` |

### Workspace / Window (scope=system)
| 키 | payload |
|----|---------|
| `workspace.created` | `workspace_id, window_id, name` |
| `workspace.closed` | `workspace_id, reason` |
| `workspace.activated` | `workspace_id, prev_workspace_id?` |
| `workspace.renamed` | `workspace_id, name?, subtitle?, description?` |
| `window.created` | `window_id, kind, modality` |
| `window.closed` | `window_id, reason` |
| `window.focused` | `window_id` |

`window.kind`/`modality` 는 [hierarchy](../concepts/hierarchy.md) 와 일치.

`workspace.created` 의 `window_id` 는 발행 시점에 그 workspace 가 속한 창의 id 다. 창이 없는 engine(창을 닫아 보관 중인 engine 등)에서 만들었거나 발행 전에 창에서 사라졌으면 `0` 이며, `0` 은 "창 없음"을 뜻한다. 같은 payload 를 받는 Lua `workspace.create.post` 도 같다.

### Process (scope 표시)
| 키 | scope | payload |
|----|-------|---------|
| `process.started` | surface | `surface_id, pid, command` |
| `process.exited` | surface | `surface_id, exit_code?` |

### Plugin / Extension / Tool (scope=system)
| 키 | payload |
|----|---------|
| `plugin.loaded` | `plugin_id, version` |
| `plugin.unloaded` | `plugin_id, reason` |
| `plugin.error` | `plugin_id, error_kind, message` |
| `plugin.enabled` / `plugin.disabled` | `plugin_id` |
| `plugin.surface_kind_registered` | `plugin_id, kind, rendering` |
| `plugin.window_declared` | `plugin_id, window_id` |
| `extension.activated` | `extension_id, target_id` |
| `extension.pending` | `extension_id, target_id, reason` |
| `extension.conflict` | `extension_id, target_id, conflicting_id` |
| `tool.invoked` | `tool_id, source(builtin/plugin)` |

<a id="command-option-d--plugin-은-단축키를-보지-않음"></a>

### Command — plugin은 실행할 명령을 받는다
| 키 | 전달 | payload |
|----|------|---------|
| `command.invoked` | **owner unicast**(broadcast 아님) | `plugin_id, command_id, scope, source_surface_id?, trigger(shortcut/menu/ipc)` |
| `command.shortcut_changed` | broadcast | `plugin_id, command_id, shortcut?, prev_shortcut?` |
| `command.host_shortcut_changed` | broadcast | `action_ids` — 설정 저장으로 바인딩이 달라진 host 단축키 동작 ID 목록. webview 페이지에 조합을 넣은 plugin 이 문서를 다시 만든다 |

scope=global command 단축키는 조합키만, scope=surface 는 단일 키도 허용.

### Memory (scope=system, Stable)
`memory.changed`: regular entry 의 put/delete/expire/cleanup 이 commit 된 뒤 — `scope, key, kind∈{created,updated,deleted,expired}, version?`. **secret 영역은 발행하지 않음**(owner/key 노출 방지). 1 변경 = 1 envelope. 정리되지 않은 만료 행을 덮어쓴 put 은 `expired`(version 없음) 다음 `created`(version 1)를 같은 commit 순서로 낸다 — 주기 정리가 먼저 지운 경우와 구독자가 보는 순서가 같다. 구독 권한 `memory.read`. GUI 와 headless 모두 메인 루프가 회차마다 발행한다.

### Agent (scope=system, Experimental)

협업 기능의 **종결 사실**만 싣는다. 대상 workspace 는 `meta.scope` 가 아니라 payload 의 `workspace_id` 로 온다 — envelope 의 scope 축은 `system`/`surface` 둘뿐이고 `workspace.*` 계열이 이미 같은 방식이다.

| 키 | 시점 | payload | 등급 |
|----|------|---------|------|
| `agent.task_finished` | task 가 종결 상태에 들어간 직후 | `workspace_id, task_id, state`, 선택 `attempt_id, revision, skip, terminal_id` | Experimental |
| `agent.barrier_closed` | barrier 가 요구 수를 채워 닫힌 직후 | `workspace_id, name, count_required` | Experimental |

- `state` 는 `succeeded` · `failed` · `cancelled` · `skipped` 넷 중 하나다. **비종결 전이(`waiting`/`ready`/`running`)는 발행하지 않는다** — 종결에는 모든 진입 경로가 지나는 공통 처리 지점이 있고(`agent.task_await` 가 그것으로 깨어난다) 비종결에는 없다. 종결 쓰기의 응답을 잃어도 후속 확인 읽기나 재시작에서 영속 종결 의무를 확인하면 발행한다. 복구 범위는 [agent-runner §실행 회차와 완료](../dev-guide/agent-runner.md#실행-회차와-완료)에 있다.
- `terminal_id`는 영속 종결 의무의 UUID다. 같은 허브에서 같은 ID는 한 번 발행한다. 전달 뒤 확인 전에 프로세스가 종료되면 재시작 후 같은 ID가 다시 올 수 있어 소비자는 이 필드로 재전달을 구분한다. 다른 회차와 삭제 후 같은 task ID의 재생성은 새 UUID를 받는다. 영속 의무가 없는 내부 직접 통지는 생략할 수 있다. 피드는 휘발성 bounded 큐이며 수신 확인까지 영속 보장하지 않는다.
- `attempt_id` 는 v2 task 의 실행 회차 id(`agent.task_get` 의 `attempt.id`)이고 회차가 없는 task 는 생략한다. `revision` 은 종결을 기록한 뒤 읽은 레코드 revision 으로 `agent.task_get`·`agent.task_await` 의 `revision` 과 같은 값이다. 레코드는 쓸 때마다 revision 이 커지므로, 다시 읽은 레코드의 `revision` 이 사건의 값보다 작지 않으면 이 사건 이후의 레코드다. 이 비교는 같은 레코드의 수명 안에서만 성립한다. task 를 지우고 같은 id 로 다시 제출하면 revision 은 1부터 다시 시작하므로, 그 사이에 삭제·재제출이 있었을 수 있으면 `attempt_id`·`state` 도 함께 대조한다. 읽지 못했으면 생략한다. `skip` 은 건너뛴 이유가 기록된 v2 task 에만 실리며 `agent.task_get` 의 `skip` 과 같은 모양이다(`{"reason": "branch_not_selected"}` 또는 `{"reason": "upstream_unavailable", "source", "source_state"}`). 경로가 선택되지 않아 끝난 task 와 선행 실패로 건너뛴 task 를 이 필드로 구분한다.
- **실패 사유·task 결과·명령 출력을 안 싣는다.** 사건은 종결 사실과 레코드를 다시 읽을 좌표(`task_id`·`attempt_id`·`revision`)만 알리고, 실패 사유·결과는 레코드가 정본이다. 필요하면 `task_id` 로 `agent.task_get` 을 불러 읽고, `revision` 으로 그 레코드가 이 사건 이후의 것인지 확인한다.
- **`agent.barrier_closed` 에 시간 초과는 안 온다.** barrier 의 `timed_out` 은 전이가 일어나는 순간이 없고 조회할 때 현재 시각으로 판단한다.
- **lease 만료는 사건이 아니다.** 같은 이유다 — 만료는 읽을 때 확인하는 조건이고, 그것을 사건으로 내면 발생 시점이 "누가 언제 조회했나" 에 달린다.
- 등급이 Experimental 이라 minor 에서 키·payload 가 바뀔 수 있다. 구독 조건은 다른 키와 같다 — 매니페스트 `event_subscribe` 가 그 키를 덮으면 받는다.

### IME / Theme / Language / Notification / Hook / System
| 키 | scope | 등급 | payload |
|----|-------|------|---------|
| `ime.composition_start` / `_end` | surface | Experimental | `surface_id` / `surface_id, committed_text` |
| `theme.changed` | system | Stable | `theme_id` |
| `language.changed` | system | Stable | `language_code` |
| `notification.created` | system | Stable | `id, level, title, body, source` |
| `notification.dismissed` | system | **Planned**(예약, 아직 발행하지 않음) | `id` |
| `hook.fired` | surface/system | Experimental | `hook_id, event_kind, surface_id?, payload` |
| `system.startup_complete` | system | Stable | `{}` |
| `system.shutdown_initiated` | system | Stable | `reason` |
| `debug.*` | system | Internal | (가변, debug 빌드만) |

`theme.changed` 는 전역 Theme 의 색 세트나 UI 배율을 정하는 외관 값 — 테마 ID(`appearance.theme`)·기본 색(`appearance.theme_base`)·색 override(`appearance.theme_overrides`)·라이트 여부(`appearance.theme_is_light`)·UI 배율(`appearance.ui_scale`) — 중 하나라도 바뀐 설정 적용에서 새 전역 Theme 를 설치한 뒤에 한 번 발행한다. 한 번의 설정 적용에서 여러 값이 함께 바뀌어도 한 번이다. 테마 ID 외의 값만 바뀐 경우(같은 ID 의 테마 파일을 고쳐 다시 적용한 경우 포함) `theme_id` 는 직전과 같으므로, 구독자는 `theme_id` 가 달라졌는지로 재적용 여부를 정하지 않는다. payload 에 색·`is_light`·zoom 이 없으므로 구독자는 수신 후 `theme.query` 로 새 테마를 읽는다. `accessibility.reduced_motion` 은 전역 Theme 에 들어가지만 `theme.query` 응답에 없고 색이 아니므로 발행 조건이 아니다. 발행 조건을 이렇게 넓힌 것은 Stable 이벤트의 키·필드를 바꾸지 않는 발행 시점 확장이다.

> `composition_update`·`process.output_match`·`settings.changed` 는 1.0 제외. 알림 읽음 처리는 표시 상태만 바꾸며 host 이벤트를 발행하지 않는다.

<a id="구독발화-권한-패턴"></a>

## 구독·발행 권한 패턴

매니페스트 `event_subscribe` / `event_publish` 가 권한 게이트다.

| 형식 | 예 | 매칭 |
|------|-----|------|
| 정확 키 | `surface.closed` | 그 키만 |
| 네임스페이스 와일드카드(끝만) | `surface.*` | `surface.` prefix 전부 |
| plugin id 네임스페이스 | `com.tasty.claude.*` | 그 plugin publish 전부(대상 매니페스트 `[[events_emitted]]` 선언 필요) |

거부: `"*"`(전체), `"*.bar"`/`"foo*"`(중간/시작 와일드카드). `event_publish` 는 예약 네임스페이스 키 거부.

## 후속 변경 정책

Stable 키·필수 필드 제거는 기존 소비자를 깨뜨릴 수 있다. 호환성 유지·버전 부여·유예 여부는 [제품 원칙](../principles/platform-and-compatibility.md)에 따라 개발자가 정한다. 릴리스의 깨지는 변경은 CHANGELOG에 (BREAK)로 알린다. Stable·Experimental은 현재 계약의 성격을 알리는 표시이며 특정 major/minor 증가나 영구 호환을 보장하지 않는다.

## 관련

- [concepts/plugins](../concepts/plugins.md) — plugin 통합 축(events 포함)
- [reference/api](api.md) — IPC/CLI 표면
