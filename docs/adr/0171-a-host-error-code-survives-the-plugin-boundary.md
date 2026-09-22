# ADR-0171: 호스트가 준 오류 코드는 plugin 경계를 넘어 살아남는다 — ADR-0153 의 "한 겹 감싸짐" 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: ipc, error-codes, plugin, sdk, wire-protocol, partial-amendment, adr-0153, adr-0154, adr-0163, adr-0167
- **Group**: ipc-contract

## Context

외부 호출자가 plugin namespace 의 메서드를 부르면, 그 호출은 owner plugin 으로 forward 되고
plugin 이 자기 일을 하려고 호스트 메서드를 되부른다(`claude.parent` → `terminal.parent`).
그 되부름을 호스트가 거절하면 거절 사유가 plugin 을 거쳐 원래 호출자에게 돌아간다.

**그 왕복에서 오류 코드가 사라졌다.** 호스트는 `-32602`("인자가 틀렸다, 고쳐서 다시 걸어라")
로 답했는데 외부 호출자는 `-32000`("서버 사정")을 받았다. 두 코드는 호출자가 다음에 할 일이
반대다 — 앞엣것은 인자를 고쳐 재시도하고, 뒤엣것은 재시도를 포기한다.

### 코드는 버리는 그 자리에 손에 들려 있었다

`crates/tasty-host-plugin/src/manager/response.rs` 의 한 `match` 안에서, namespace 갈래는
`resp.error_code` 를 **쓰고 있었고** plugin 갈래는 같은 값을 **버렸다**. 없어서 못 넘긴 것이
아니다. 그리고 반대 방향(plugin → 호스트)의 `PluginResponse` 에는 `error_code` 가 처음부터
있었다 — 한 방향만 비어 있었다.

버리는 자리는 일곱이었고 한 축이다: gui 종단(`src/app/dispatch/plugin_ipc.rs`) · 헤드리스
쌍둥이(`src/boot/headless_plugins.rs`, 두 곳) · `send_ipc_result` 의 시그니처 ·
`response.rs` 의 plugin 갈래 · 와이어의 `IpcCallResult` · SDK 의 `deliver_ipc_result` ·
`From<PluginError> for IpcMethodError` 의 하드코딩 `-32000`.

### 실측 (2026-09-05, 격리 gui 인스턴스)

claude · codex 의 안전한 inbound 13 개씩 26 건에 `{"surface_id":999}`:

    고치기 전   감싸짐(-32000) 11 · -32602 5 · ok 5 · -32601 5
    고친 뒤     감싸짐 0        · -32602 16 · ok 5 · -32601 5

감싸진 11 은 전부 호스트가 `-32602` 로 답한 것이었다. 표시 문구는 바이트 단위로 그대로다
(`host call 'call#N' failed: <호스트 사유>`) — 그 문구를 읽는 소비자가 이미 있기 때문이다.

### 이 축이 왜 "같은 잘못에 답이 여럿" 인가

같은 잘못된 대상이 **owner plugin 이 떠 있느냐에 따라** 다른 코드를 받았다. A–B–A 로 확정:

    codex enabled    codex.parent{surface_id:999} → -32000  (plugin 이 실행되고 그 outbound 가 잘렸다)
    codex disabled   같은 호출                    → -32602  (forward 가 안 돼 호스트가 직접 답했다)
    codex enabled    같은 호출                    → -32000

즉 코드가 **대상의 잘못**이 아니라 **plugin 의 기동 상태**를 보고했다. 고친 뒤에는 두 국면이
같은 `-32602` 로 답한다.

## Decision

`ipc.result`(호스트 → plugin) 와이어에 `error_code: Option<i32>` 를 더하고, 위 일곱 자리가
호스트가 준 코드를 그대로 흘린다. `From<PluginError> for IpcMethodError` 는 호스트가 코드를
준 실패(`PluginError::HostCall { code: Some(_) }`)에 그 코드를 쓰고, 그 외에는 종전대로
server error(`-32000`)를 쓴다.

**표시 문구는 안 바꾼다.** `PluginError::HostCall` 의 `#[error]` 에 코드를 넣지 않는다 —
그 문구를 읽는 판정이 이미 있다(`crates/tasty-plugin-agent-stream/src/resolve.rs` 의
"그런 surface 는 없다" 판정). 코드는 변환 시점에만 쓰인다.

### ADR-0153 의 무엇을 개정하나

[ADR-0153](0153-a-bundled-namespace-hands-host-methods-back.md) 의 Consequences 중
**"plugin 을 거쳐 가므로 응답이 한 겹 감싸진다 — `-32000 host call 'call#N' failed: …`"**
한 조항만 개정한다. 응답이 한 겹 감싸지는 것(문구가 `host call '…' failed:` 로 시작하는 것)은
**그대로다.** 바뀌는 것은 그 겹이 **코드까지 뭉개지는가**뿐이다.

**개정하지 않는 것을 이름으로 적는다:** 번들 namespace 아래 host 구현을 plugin 이 되돌려
주는 결정(self-call trampoline), `markdown.navigate` 가 plugin 설치 여부와 무관하게 같은 곳에
닿는다는 결정, `src/source_guards/bundled_plugin_namespace_coverage.rs` 가 그 정합을 양방향으로
보는 것, 서드파티 plugin 이 그 가드의 대상이 아니라는 것 — 넷 다 그대로 유효하다.

### 2026-09-20 보강 — 호스트가 *스스로 내는* 코드도 같은 축이다

위 "버리는 자리 일곱" 은 전부 호스트가 **되받은** 코드(`resp.error_code`)를 흘리는
자리였다. 호스트가 plugin 을 기다리다 **스스로 내는** 코드는 그 셈에 없었고, 그 자리는
네 개였다 — `send_final_error` 의 plugin 갈래(호출자 넷이 `-32001` · `-32003` ·
취소 `-32004` · 만료 `-32004` 를 준다) · `forward_namespace_call_from_plugin` 의 검증
실패 갈래(`validate_namespace_call` 이 코드를 손에 쥐여 주는데 `_code` 로 버렸다) ·
`cancel_pending_namespace_calls` 의 plugin caller 갈래 ·
`expire_pending_request` 의 plugin caller 갈래. 뒤 둘은 **바로 옆 local caller 갈래가
`-32004` 를 싣는데** 같은 사건의 plugin 쪽만 `None` 이었다.

그래서 같은 사건이 **누가 물었는가**에 따라 다른 코드로 나갔다 — CLI 가 물으면
`-32004`("plugin 이 네 호출을 끝내지 못했다"), 다른 plugin 이 물으면 `-32000`("서버
사정"). 이 ADR 의 Context 가 든 결함 — 코드가 *대상의 잘못*이 아니라 *plugin 의 기동
상태*를 보고했다 — 와 같은 모양이고, 축도 같다. 그래서 새 결정이 아니라 **이 결정이
안 센 네 자리를 같은 규칙 아래로 넣는 것**이다. 네 자리 모두 손에 든 코드를 그대로
싣는다.

[ADR-0311](0311-a-namespace-call-expires-into-an-error-not-a-fail-open.md) 이 그
비대칭을 사실로 적고 "없어지면 그 문단을 지워라" 를 재검토 조건으로 달아 두었으므로,
그 문단과 조건을 같은 커밋에서 갈아끼웠다.

**그 넷을 고치면 다섯째 갈래가 따라온다 — 그쪽은 호스트가 *되받은* 코드다.**
`send_final_error` 는 `finalize_target_outcome` 에서도 불리고, 거기로 오는 `code` 는
`handle_target_response_with_post_hook` 의 `resp.error_code.unwrap_or(-32000)` —
**target plugin 이 낸 코드**다. 그래서 plugin A 가 plugin B 의 메서드를 부르고 그
메서드에 extension 의 post-IPC hook 이 걸려 있으면, B 가 낸 `-32602` 가 종전에는
`-32000` 으로 뭉개졌다가 이제 그대로 A 에게 간다. 이 자리는 위 "버리는 자리 일곱" 에도
빠져 있었다 — 같은 한 줄이 두 축을 한 번에 고친 셈이고, 그래서 **동작 변경의 모수는
"호스트가 내는 네 값" 이 아니라 "그 네 값 + target plugin 이 내는 임의의 JSON-RPC 코드"**
다. 이 ADR 의 축("호출자가 대상의 판단을 그대로 듣는다")에는 둘 다 맞다.

## Consequences

- **얻은 것**: 호출자가 호스트의 판단을 그대로 듣는다. 같은 잘못된 대상이 plugin 기동 상태와
  무관하게 같은 코드를 받는다. CLI 의 hook 실패 기록([ADR-0164](0164-hook-failure-locale-invariance-rests-on-fields.md))
  이 싣는 `code=` 도 이제 참값이다 — 그 필드가 로케일 무관성을 지는데, 종전에는 plugin 을
  거친 실패에서 전부 `-32000` 이라 아무 것도 안 갈랐다.
- **잃은 것**: `-32000` 을 기대하고 재시도를 포기하던 호출자가 이제 `-32602` 를 받아
  재시도한다. 그게 의도지만 **동작 변경**이다. 이 저장소 안에 오류 코드 값으로 분기하는
  소비자는 전수에서 하나뿐이고(`src/boot/headless_dispatch.rs` 의 forward 신호,
  `-32601`/`-32017`), `-32000` 으로 분기하는 자리는 **0** 이다(2026-09-05 실측).
- **와이어 호환**: 필드는 `#[serde(default, skip_serializing_if)]` 라 양방향이다. 구버전 SDK
  로 빌드된 plugin 은 낯선 키를 안 보고, 구버전 호스트가 보낸 모양은 `None` 으로 읽힌다.
  두 방향을 `crates/tasty-plugin-protocol/src/protocol_tests.rs` 가 못 박는다.
- **2026-09-20 보강이 바꾼 동작**: plugin caller 가 받던 `-32000` 이 이제 실제 코드가
  된다. 그 값은 **둘 중 하나**다 — 호스트가 스스로 낸 넷(`-32001`·`-32002`·`-32003`·
  `-32004`), 또는 post-IPC hook 이 걸린 경로에서 **target plugin 이 낸 임의의 JSON-RPC
  코드**(`finalize_target_outcome` 갈래). 즉 모수는 닫힌 네 값이 아니다.
  `PluginError::HostCall { code }` 로 분기하는 소비처는 SDK 의
  `From<PluginError> for IpcMethodError` 하나고, 값으로 분기하는 자리는 이 저장소에
  없다(이 ADR 의 위 항이 센 것과 같은 좌변). 표시 문구는 여전히 안 바뀐다.
- **운영 비용**: `send_ipc_result` 에 인자가 하나 늘었다. **2026-09-20 보강 뒤로 그
  인자에 `None` 을 주는 실패 경로는 없다** — 호스트 내부 실패 다섯 자리가 전부 코드를
  싣고, `None` 이 남은 곳은 성공 경로 하나뿐이다(코드를 실을 것이 없다). 새 실패 경로를
  더하는 사람은 `None` 이 아니라 그 자리의 코드를 준다: `None` 은 SDK 기본값 `-32000`
  으로 떨어져 이 ADR 과 ADR-0311 이 막은 비대칭을 그 자리에 다시 만든다.

## Alternatives Considered

- **코드를 표시 문구에 넣는다** — 기각. 문구를 읽는 판정이 이미 있고, 그 판정은 "없다는 답"
  과 "모름" 을 가른다. 문구가 바뀌면 그 판정이 조용히 한쪽으로 떨어진다.
- **plugin 마다 자기 오류 코드를 정하게 둔다** — 기각. 같은 호스트 거절이 plugin 마다 다른
  코드로 나오면 이 축의 결함이 이름만 바꿔 남는다.
- **그냥 둔다(ADR-0153 이 수용했다)** — 기각. 그 조항은 감싸짐을 **관측된 결과**로 적었지
  "이게 맞다" 고 결정한 것이 아니다. 그리고 이 저장소는 같은 축을 세 번 다른 방향으로
  결정했다([ADR-0154](0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) ·
  [ADR-0163](0163-a-registered-name-answers-who-not-whether.md) ·
  [ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md)) — 답은 호출자에게
  **다음에 할 일**을 말해야 한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 호스트 코드를 그대로 흘리는 것이 **틀린 답**이 되는 사례가 나올 때 — plugin 자신의 버그로
  잘못된 인자를 호스트에 보내면 호출자가 못 고칠 것을 `-32602` 로 듣는다. 지금은 문구가
  안쪽 메서드 이름을 밝히므로 구분 가능하다고 보지만, 그 구분이 부족하다는 사례가 나오면
  plugin 이 코드를 갈아끼울 수단을 준다.
- 오류 코드 값으로 분기하는 외부 소비자가 생겼을 때 — 그때는 탐지 규약을 먼저 정한다
  (ADR-0163 과 같은 트리거).
- "그 대상이 없다" 전용 코드가 생길 때 — 그러면 agent-stream 의 문자열 판정이 코드로 옮겨갈
  수 있다.

## References

- 개정 대상: [ADR-0153](0153-a-bundled-namespace-hands-host-methods-back.md) (Consequences 의 "한 겹 감싸짐" 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [ADR-0154](0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) · [ADR-0163](0163-a-registered-name-answers-who-not-whether.md) · [ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md) — 같은 축의 앞선 세 결정
- [ADR-0164](0164-hook-failure-locale-invariance-rests-on-fields.md) — 실패 기록의 `code=` 필드
- [api-conventions](../dev-guide/api-conventions.md) — 오류 코드 표
