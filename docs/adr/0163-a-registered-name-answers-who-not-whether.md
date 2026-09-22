# ADR-0163: 등재된 이름은 "없다" 가 아니라 "부를 수 있는 주체가 다르다" 로 답한다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: ipc, error-codes, plugin, agent-surface, guards, adr-0154, adr-0140, adr-0152
- **Group**: ipc-contract

## Context

`banner.open` · `banner.close` · `popup.close` · `host.shared_buffer.create` 넷은
plugin 이 host 에게 자기 자원을 요청하는 메서드다. host 는 이들을 **plugin host-call
진입부에서 직접 인터셉트**하고(`src/app/dispatch/plugin_ipc.rs`, 헤드리스는
`src/boot/headless_plugins.rs`), 외부(CLI · 네트워크 IPC) 라우터에는 arm 을 두지 않는다.
그 설계 자체는 옳다 — 대상 식별이 caller plugin 자신이거나(배너·팝업 인스턴스), 응답이
보조 채널을 함께 써야 해서(공유 메모리 핸들) 셸 호출자가 성립하지 않는다.

문제는 **외부에서 그 이름을 부르면 무엇을 듣는가**였다.

### ① 지금 응답은 사실이 아닌 것을 말한다

외부 호출자는 `-32601 Method not found` 를 받았다. 그 코드의 뜻은 "그런 메서드는 없다"
인데, 그 메서드는 **있고, `METHOD_TABLE` 에 등재돼 있고, 구현돼 있다.** 없는 것은 그
호출자를 위한 dispatch arm 하나뿐이다.

이 거짓은 호출자를 틀린 방향으로 보낸다. `-32601` 을 받은 사람은 **이름을 의심한다** —
오타를 고치거나 표를 다시 읽는다. 그 방향에는 고칠 것이 없다. 사실을 들었다면 그는
"이건 plugin 이 부르는 것" 이라는 **다른 결론**에 도달한다. 표가 읽는 사람을 없는 곳으로
보내는 형태를 이 저장소는 이미 여러 번 잡았고, 이번엔 **응답이** 그렇게 한다.

플랫폼 축에서 같은 거짓을 고친 것이
[ADR-0154](0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) 다("여기선 못 한다"
를 `-32015` 로 답한다). 이 ADR 은 그것의 **caller 축** 대응이다.

### ② `plugin_callable` 이 이미 있는데 왜 부족한가

`MethodMeta.plugin_callable` 은 이미 표에 있다. 즉 **호스트는 이미 안다.** 부족한 것은
그 앎이 **응답에 안 실린다**는 것이다 — 외부 호출자는 `-32601` 만 받고 `plugin_callable`
을 볼 방법이 없다(그 표를 노출하는 IPC 메서드도 CLI 도 없다. 실측 2026-09-05: 표의
런타임 소비처는 `crates/tasty-ipc/src/caller.rs` 의 plugin 게이트 하나뿐이다).

게다가 그 필드는 **방향이 반쪽**이다. `local_only()` 는 "plugin 은 못 부른다" 를 말할 수
있는데, 그 거울인 "plugin 만 부를 수 있다" 를 말할 수단이 없었다. 그래서 이 넷이
`plugin(&[…])` 로 적혔고, 그 표기는 "plugin 이 부를 수 있다" 는 참을 말하면서
"그러니 남들도 부를 수 있다" 는 **거짓을 함의**했다.

그리고 게이트의 한쪽 방향은 이미 올바르게 답하고 있었다 — plugin 이 `local_only` 메서드를
부르면 `UnknownMethod` 가 아니라 `NotPluginCallable` 을 받는다. 즉 이 저장소는 **한쪽
방향에서는 이미 "없다" 로 답하기를 거부하고 있었다.** 반대 방향에 그 답이 없었던 것은
결정이 아니라 표에 칸이 없어서였다.

### 이 넷이 "두 집 문제" 가 흔들린 그 자리다

같은 날 `host.shared_buffer.create` 행이 `docs/dev-guide/api-conventions.md` 에서 지워졌다가
가드가 반대 방향으로 걸려 되돌아왔다. 그때는 "어느 문서가 정본인가" 로 풀었다. 지금 보면
그 행이 흔들린 진짜 이유는 다른 데 있다 — **그 메서드가 "없다" 도 "있다" 도 아닌 제3의
상태였고, 그 상태를 부를 이름이 없었다.** 이름이 없으면 문서는 그것을 있는 쪽으로 적었다
지웠다 하게 된다. 이 ADR 이 그 상태에 이름을 준다.

### 모수 — 작은 축이다

실측(2026-09-05, gui debug 인스턴스 · plugin 설치된 세계 · 외부에서 전수 프로브):
`plugin_callable = true` 인 **231** 개 중 외부 호출이 `-32601` 로 끝난 것은 **4** 개다.
나머지는 `-32602` 188 · 실행 성공 37 · `-32000` 2(plugin 으로 forward 된 뒤의 답)였다.
표 설계 문제가 아니라 **말할 수단이 없던 한 칸**이다.

같은 집합을 "외부 라우터 소스에 이름이 안 보이는 것" 으로 세면 **14** 개가 나온다 —
`window.*` · `view.*` · `ui.screenshot` 처럼 match 팔이 아니라 명부로 라우팅되는 것이 섞여
들어온다. 이 부류는 소스 텍스트가 아니라 **실행**으로만 정해진다.

## Decision

표에 방향을 적을 칸을 만든다. `MethodMeta.plugin_only` 와 그 생성자 `plugin_only(&[…])` —
`local_only()` 의 거울이다. 그리고 **그 표식을 응답이 싣게 한다**: 외부 dispatch 의 종단
(`src/adapters/ipc/handler.rs`)은 `method_not_found` 대신
`JsonRpcResponse::unrouted_for_external_caller` 를 부르고, 그 함수가 표를 보고 갈라 답한다.

    -32016  method '<name>' is plugin-only: only the plugin host-call path dispatches it,
            so CLI and network IPC callers have no entry point

등재되지 않은 이름은 그대로 `-32601` 이다 — 그 갈래가 무너지면 오타가 "주체가 다르다" 로
보고돼 호출자가 영영 못 고친다.

**표식을 하중 부담(load-bearing)으로 둔 것이 핵심이다.** 표식이 문서에만 있으면 조용히
낡지만, 응답을 바꾸면 낡는 순간 관측된다. 그 위에 `src/source_guards/plugin_only_dispatch_parity.rs`
가 표식과 plugin 진입부 인터셉트를 **양방향**으로 대조한다.

## Consequences

- **얻은 것**: 외부 호출자가 "이름이 틀렸다" 가 아니라 "주체가 다르다" 를 듣는다. 표가
  방향을 말할 수 있게 됐고(`local_only` 의 거울이 생겼다), 그 방향이 응답으로 관측된다.
- **잃은 것**: 에러 코드가 하나 늘었다(`-32016`). `-32601` 을 기대하던 외부 호출자가 이
  넷에 대해서만 다른 코드를 받는다 — 넷 다 CLI 잎이 없어 셸에서 부를 길이 원래 없었고,
  등재되지 않은 이름의 `-32601` 은 그대로다.
- **운영 비용**: 새 메서드가 이 부류가 되면 표식을 붙여야 한다. 안 붙이면 가드가
  "인터셉트는 있는데 표식이 없다" 로 잡는다(반대 방향도 잡는다).
- 헤드리스의 namespace forward 는 `-32601` 을 신호로 쓰는데, 이 넷의 prefix(`banner` ·
  `popup` · `host`)는 전부 예약돼 있어([ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md))
  plugin 이 점유할 수 없다. 그래서 forward 가 성립할 수 없었던 자리이고, 코드가 바뀌어도
  잃는 경로가 없다.

## Alternatives Considered

- **별도 표로 뺀다** — 기각. 집합이 하나 더 생기면 두 집합이 갈라지는 자리가 하나 더
  생긴다. 이 저장소는 그 형태를 이미 겪었다(같은 명제의 두 집). 게다가 표를 나눠도
  **응답은 그대로 `-32601`** 이라 호출자가 듣는 거짓이 안 고쳐진다 — 이 축의 결함은 표가
  아니라 답에 있다.
- **지금이 맞다(문서로 충분하다)** — 기각. 사실은 `api-conventions.md` 에 적혀 있지만,
  그것을 읽는 것은 **문서를 여는 사람**이고 `-32601` 을 받는 것은 **호출 중인 에이전트**다.
  둘은 같은 시점에 있지 않다. 읽는 사람과 겪는 사람이 다른 시점에 있는 결함은 "어딘가에 적혀 있다" 로 닫히지 않는다.
- **CLI 잎을 만들어 준다** — 기각. 대상 식별이 caller plugin 자신이거나(배너·팝업)
  응답이 보조 채널을 요구해서(공유 버퍼) 셸 호출자가 **원리적으로** 성립하지 않는다.
  ADR-0152 가 그 이유를 이미 적었다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `plugin_only` 표식이 두 자리 수가 될 때 — 그때는 "말할 수단이 없던 한 칸" 이 아니라 표
  설계의 축이 하나 빠진 것이므로 표의 형태 자체를 다시 본다.
- 메서드 표를 외부에 노출하는 표면(예: `system.methods`)이 생겼을 때 — 그러면 호출자가
  부르기 전에 방향을 읽을 수 있어 응답 코드의 역할이 줄어든다.
- 외부 호출자에게 `-32601` 을 기대하는 소비자가 생겼을 때(예: 코드로 능력 탐지를 하는
  클라이언트) — 그때는 탐지 규약을 먼저 정한다.

## References

- [ADR-0154](0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) — 같은 거짓의 플랫폼 축(`-32015`)
- [ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md) — 호스트 prefix 예약
- [ADR-0152](0152-gates-run-before-routing-not-inside-it.md) — plugin 진입부의 게이트 순서
- [api-conventions](../dev-guide/api-conventions.md) — CLI 진입점이 없는 메서드의 사유 표와 † 절
- `src/source_guards/plugin_only_dispatch_parity.rs` — 표식 ↔ 인터셉트 양방향 대조
