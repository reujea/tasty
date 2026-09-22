# Event Bus 1.0 카탈로그

호스트와 plugin 이 공유하는 사건의 wire 계약. **plugin 이 의존하는 공개 API** 이자 호환성 정책의 단일 출처(SoT). plugin 이 구독·발화 API 를 *어떻게* 쓰는지는 [concepts/plugins](../concepts/plugins.md) · dev-guide; 여기는 사건의 *구조* 만 다룬다. 페이로드 Rust 타입은 `tasty_plugin_protocol::events::payloads` 가 정답이며, 본 표는 사람이 읽기 위한 요약이다.

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
| `meta.trace_id` | chain 전체 공유 opaque id. 호스트 발화 시 생성. 재발화에서 전파되는 것은 plugin 이 받은 값을 실어 보낼 때다 — 호스트는 이 값을 고치지 않는다 |
| `meta.hop` | 호스트 발화 시 `0`. plugin 의 publish 는 **호스트가 정한다**: 그 plugin 에게 보낸 `event.dispatch` 중 아직 응답이 안 온 것이 있으면 `max(보낸 값, 그 dispatch 들의 hop 최댓값 + 1)`, 없으면 보낸 값 그대로. SDK 는 `on_event` 를 마친 뒤 응답하므로 콜백 안의 publish 가 곧 재발화이고 `+1` 이 된다. **올린 값이 `hop > 16`(MAX_HOP) 이면 dispatcher 차단** — 서로의 사건에 hop 0 으로 반응하는 두 plugin 도 16 번 안에 끊긴다. 그래서 plugin 은 `event.dispatch` 에 **응답해야 한다** — 응답하지 않은 dispatch 의 hop 은 재시작 전까지 하한으로 남아, 이후 publish 가 `MAX_HOP` 으로 거절될 수 있다. 근거·한계는 [ADR-0406](../adr/0406-the-host-raises-the-hop-of-a-publish-made-while-a-dispatch-is-unanswered.md) |
| `meta.origin` | `{kind:host}` 또는 `{kind:plugin, plugin_id}` |
| `meta.scope` | `system`(전역) 또는 `surface`(대상 id 는 payload 필드로) |

- **Lifecycle `reason`** (종료 계열 `*.closed` / `plugin.unloaded`): `user`(사용자 직접) / `ipc`(CLI·plugin 자동화) / `crash`(비정상). cascade 별도 분류 없음 — 부모를 닫은 주체가 자식 reason 에 전파.
- **쓰로틀**: `surface.resized`(키별), `split.ratio_changed`(group 별)는 150ms leading+trailing. drag 시작·종료엔 무조건 1회.

## 지나간 사건 — 위치로 읽는다

버스는 발화한 envelope 을 **메모리 링**에 들고 있다 — 1024 건과 16 MiB(직렬화 바이트) 중 먼저 닿는 쪽까지이고, 넘으면 가장 오래된 것부터 밀려난다. 가장 새 사건 하나는 크기와 무관하게 남는다. 구독하지 않고 있던 소비자가 나중에 붙어 그 위치부터 읽을 수 있다. 바이트로 밀려난 사건도 개수로 밀려난 것과 같은 `truncated` / `skipped` 로 드러난다.

| 개념 | 뜻 |
|------|-----|
| 위치(offset) | 발화 순서대로 0 부터 매겨진다. **링에서 밀려나도 되돌아가지 않는다** — 옛 위치가 새 사건을 가리키는 일이 없다 |
| 세대(epoch) | 호스트가 선 순간의 표지. **재시작하면 위치가 0 부터 다시 매겨지므로**, 소비자가 옛 위치를 들고 와도 이 값이 다르면 그것이 옛 세대다 |
| `truncated` / `skipped` | 요청한 위치가 이미 밀려났을 때. **조용히 처음부터 주지 않고** 몇 개를 건너뛰었는지 함께 답한다 |
| `ahead_of_stream` / `stream_end` | 요청한 위치가 링의 끝(`stream_end`, 다음 발화가 받을 위치)보다 **뒤**일 때 — 이 세대에 아직 없는 위치다. 흔한 원인은 재시작 전 세대의 위치다. **조용히 기다리지 않고** 표지를 단다. 나머지 필드는 표지가 없던 때와 같다 |

- **커서는 소비자가 든다.** 서버는 소비자별 상태를 두지 않으므로 같은 인자로 두 번 물으면 같은 답이 오고, 느린 소비자가 호스트 쪽에 아무것도 쌓지 않는다.
- **사건은 디스크에 안 남는다.** 재시작하면 링이 비는 것이 정상이고, 그 보존 수준은 완료 알림 로그가 부팅 때 지워지는 것과 같다.
- 근거·용량 단위·대안은 [ADR-0322](../adr/0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md), 바이트 상한은 [ADR-0456](../adr/0456-the-event-ring-evicts-by-count-or-bytes-whichever-comes-first.md). 끝보다 뒤인 위치의 표지는 [ADR-0405](../adr/0405-a-position-past-the-end-of-the-feed-is-marked-not-waited-on-silently.md).
- **읽는 자리는 `events.fetch` 다**(local 전용). `{offset, max, filter, wait_ms}` 를 받아 `{events, next_offset, epoch, truncated, skipped, ahead_of_stream, stream_end}` 로 답하고, 각 봉투에 자기 `offset` 이 실린다. `filter` 는 아래 구독과 **같은 문법**이다 — 정확 일치 또는 `<ns>.*`. `wait_ms` 를 주면 그 시간까지 새 사건을 기다렸다 답한다(상한 60초). CLI 는 `tasty events fetch` / `tasty events follow`. plugin 은 이 메서드 대신 구독을 쓴다 — 대기가 SDK 의 단일 워커를 막기 때문이다. 근거는 [ADR-0323](../adr/0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md).

## 예약 네임스페이스 (호스트만 발화)

```
system, surface, tab, pane, workspace, window, command, ime, split,
notification, hook, tool, plugin, extension, process, clipboard, theme, language, memory,
agent
```

이 외는 plugin 자유 — 관례상 자기 `id`(`com.tasty.claude.*`)를 네임스페이스로.

## 안정성 등급

- **Stable** — major 전까지 키·필수 필드 불변, 옵션 필드 추가만.
- **Experimental** — minor 마다 변경 가능. **경고일 뿐 구독 조건이 아니다** — 구독 조건은 등급과 무관하게 매니페스트 `event_subscribe` 패턴이 요청 패턴을 덮는가 하나다. 근거는 [ADR-0501](../adr/0501-the-experimental-event-grade-is-a-warning-not-a-subscription-gate.md).
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

`created_by`: `{kind:user}` 또는 `{kind:agent, source_plugin}`.

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
| `extension.activated` | `extension_id, target_id` |
| `extension.pending` | `extension_id, target_id, reason` |
| `extension.conflict` | `extension_id, target_id, conflicting_id` |
| `tool.invoked` | `tool_id, source(builtin/plugin)` |

### Command (Option D — plugin 은 단축키를 보지 않음)
| 키 | 전달 | payload |
|----|------|---------|
| `command.invoked` | **owner unicast**(broadcast 아님) | `plugin_id, command_id, scope, source_surface_id?, trigger(shortcut/menu/ipc)` |
| `command.shortcut_changed` | broadcast | `plugin_id, command_id, shortcut?, prev_shortcut?` |

scope=global command 단축키는 조합키만, scope=surface 는 단일 키도 허용.

### Memory (scope=system, Stable)
`memory.changed`: regular entry 의 put/delete/expire/cleanup 직후 — `scope, key, kind∈{created,updated,deleted,expired}, version?`. **secret 영역은 발화 안 함**(owner/key 노출 방지). 1 변경 = 1 envelope. 구독 권한 `memory.read`.

### Agent (scope=system, Experimental)

협업 primitive 의 **종결 사실**만 싣는다. 대상 workspace 는 `meta.scope` 가 아니라 payload 의 `workspace_id` 로 온다 — envelope 의 scope 축은 `system`/`surface` 둘뿐이고 `workspace.*` 계열이 이미 같은 방식이다.

| 키 | 시점 | payload | 등급 |
|----|------|---------|------|
| `agent.task_finished` | task 가 종결 상태에 들어간 직후 | `workspace_id, task_id, state` | Experimental |
| `agent.barrier_closed` | barrier 가 요구 수를 채워 닫힌 직후 | `workspace_id, name, count_required` | Experimental |

- `state` 는 `succeeded` · `failed` · `cancelled` · `skipped` 넷 중 하나다. **비종결 전이(`waiting`/`ready`/`running`)는 발화하지 않는다** — 종결에는 모든 진입 경로가 지나는 단일 깔때기가 있고(`agent.task_await` 가 그것으로 깨어난다) 비종결에는 없다.
- **실패 사유·task 결과·명령 출력을 안 싣는다.** 그 문자열은 task 가 돌린 명령의 출력을 담을 수 있고 피드는 구독 권한만 있으면 받는다. 필요하면 `task_id` 로 `agent.task_get` 을 부른다.
- **`agent.barrier_closed` 에 시간 초과는 안 온다.** barrier 의 `timed_out` 은 전이가 일어나는 순간이 없고 읽는 쪽이 시계를 견줄 때 도장이 찍힌다.
- **lease 만료는 사건이 아니다.** 같은 이유다 — 만료는 읽을 때 평가되는 술어이고, 그것을 사건으로 내면 발화 시점이 "누가 언제 조회했나" 에 달린다.
- 등급이 Experimental 이라 minor 에서 키·payload 가 바뀔 수 있다. 구독 조건은 다른 키와 같다 — 매니페스트 `event_subscribe` 가 그 키를 덮으면 받는다.

### IME / Theme / Language / Notification / Hook / System
| 키 | scope | 등급 | payload |
|----|-------|------|---------|
| `ime.composition_start` / `_end` | surface | Experimental | `surface_id` / `surface_id, committed_text` |
| `theme.changed` | system | Stable | `theme_id` |
| `language.changed` | system | Stable | `language_code` |
| `notification.created` | system | Stable | `id, title, body, source` |
| `notification.dismissed` | system | **Planned**(예약, 미발화) | `id` |
| `hook.fired` | surface/system | Experimental | `hook_id, event_kind, surface_id?, payload` |
| `system.startup_complete` | system | Stable | `{}` |
| `system.shutdown_initiated` | system | Stable | `reason` |
| `debug.*` | system | Internal | (가변, debug 빌드만) |

> `composition_update`·`process.output_match`·`settings.changed` 는 1.0 제외. 알림 *read* 처리는 host event 미발화(표시 상태일 뿐).

## 구독·발화 권한 패턴

매니페스트 `event_subscribe` / `event_publish` 가 권한 게이트다.

| 형식 | 예 | 매칭 |
|------|-----|------|
| 정확 키 | `surface.closed` | 그 키만 |
| 네임스페이스 와일드카드(끝만) | `surface.*` | `surface.` prefix 전부 |
| plugin id 네임스페이스 | `com.tasty.claude.*` | 그 plugin publish 전부(대상 매니페스트 `[[events_emitted]]` 선언 필요) |

거부: `"*"`(전체), `"*.bar"`/`"foo*"`(중간/시작 와일드카드). `event_publish` 는 예약 네임스페이스 키 거부.

## 후속 변경 정책

Stable 키/필수 필드 제거 → major bump. 옵션 필드 추가·새 이벤트 추가·Experimental→Stable 승격 → minor 이하(plugin 호환 유지). 새 예약 네임스페이스 추가는 충돌 가능 → major/마이그레이션 안내.

`agent` 는 그 규칙의 예외로 minor 에 들어갔다. 판정은 "이름이 충돌하는가" 로 했고, 충돌할 수 있는 자리 둘을 실측해 **0 건**이었다 — 번들 plugin 아홉의 매니페스트 어디에도 `agent.*` 발화 선언이 없고, 그 이름은 IPC 메서드 prefix 예약 목록(`RESERVED_IPC_PREFIXES`)에는 **처음부터 있었다**. 즉 사건 쪽 목록에만 빠져 있던 것이라, 더하는 것이 새 자리를 뺏는 것이 아니라 두 목록을 맞추는 일이다. 근거는 [ADR-0321](../adr/0321-agent-domain-events-publish-only-at-the-funnel-that-already-exists.md).

## 관련

- [concepts/plugins](../concepts/plugins.md) — plugin 통합 축(events 포함)
- [reference/api](api.md) — IPC/CLI 표면
