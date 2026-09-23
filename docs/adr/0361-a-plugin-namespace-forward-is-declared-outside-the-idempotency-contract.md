# ADR-0361: plugin namespace forward 는 멱등 키 계약 밖이라고 선언된다 — 호스트는 정확히 한 번을 약속하지 않는다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, protocol, idempotency, plugin, namespace, retry, adr-0338
- **Group**: ipc-contract

## Context

[ADR-0338](0338-a-mutation-retry-is-told-apart-by-a-caller-key-and-the-peer-is-asked-before-the-effect.md)
이 요청 봉투에 `idempotency_key` 를 더하고, 호스트가 같은 키의 재시도를 보관된 답으로 돌려주게
했다. 그 보존소는 engine 라우터 한 자리에 있고, 그 앞에서 끝나는 층이 둘이다 — App 층이 직접
끝내는 메서드와 **plugin namespace forward**(plugin 이 점유한 prefix 아래의 모든 이름)다. ADR-0338
은 그 두 층의 배선을 후속 조각으로 남겼고, 그때까지 호출자가 요청마다 그 차이를 아는 값은 응답의
`idempotent_replay` 하나뿐이었다. 그 표지는 한 방향으로만 답한다(붙으면 걸린 것, 안 붙은 것은
아무 뜻도 아님).

그래서 지금 plugin namespace 이름에 키를 실은 호출자는 **계약이 걸린 줄 안다.** 응답을 놓치고
같은 키로 다시 보내면 호스트는 보존소를 안 보고 요청을 그대로 owner plugin 에게 넘기고, plugin 은
두 번 실행한다. 아무 신호도 안 난다 — `send_idempotent` 의 capability 확인은 "서버가 이 필드를
읽는가" 만 묻고, 서버는 읽는다.

길은 둘이었다.

- **⒜ forward 를 계약 안으로 넣는다** — 보존소를 forward 앞에도 건다.
- **⒝ forward 를 계약 밖이라고 선언한다** — 이름 표에 그 사실을 적고, 호출자가 키를 실어 보내기
  전에 거절한다.

제약이 둘 있었다. ⒜ 가 필요로 하는 보존소 파일(`src/adapters/ipc/handler/idempotency.rs`)은 이
결정을 실행한 작업의 소유 밖이다(같은 회차에서 다른 작업이 그 파일을 고친다). 그리고 **서버의 관측
가능한 동작을 바꾸지 않는다** — 이미 배포된 client 가 plugin 이름에 키를 실어 보내고 있을 수 있다.

## Decision

**⒝ 를 고른다. plugin namespace forward 는 멱등 키 계약 밖이라고 이름 표(`method_meta`)에 선언하고,
호스트는 그 호출이 정확히 한 번 실행된다고 약속하지 않는다.**

- **선언은 값이다.** `MethodMeta` 에 `key_contract: KeyContract` 를 더한다. 값은 둘이다 —
  `Outside`(계약 밖: 키를 실어도 호스트가 보존소를 안 거친다) 와 `Undeclared`(이 표가 아무것도
  선언하지 않는다). namespace fallback 이 만드는 메타는 `Outside`, 표의 모든 호스트 메서드는
  `Undeclared` 다.
- **"걸린다" 를 뜻하는 값은 두지 않는다.** 호스트 메서드가 보존소에 걸리는지는 메서드마다 다르고
  (App 층이 끝내는 것은 안 걸린다) 그 목록은 아직 표에 없다. 확정되지 않은 것을 선언하면 "착지한 것만
  적는다" 를 어긴다 — 그래서 확정된 한 갈래(`Outside`)만 선언한다.
- **표가 모르는 이름은 `Outside` 다** (`key_contract(method)`). client 프로세스에는 namespace 소유
  표가 없어서 plugin 이름은 그쪽에서 "표에 없는 이름" 으로 보인다. 그것을 `Undeclared` 로 읽으면
  이 결정이 막으려는 호출이 그대로 통과한다. 모를 때는 조심스러운 쪽 — `effect` 가 모르는 이름을
  `Mutate` 로 두는 것과 같은 판단이다.
- **client 가 보내기 전에 거절한다.** `IpcConnection::send_idempotent` 가 capability 확인 **앞에서**
  `key_contract` 를 보고 `Outside` 면 `KeyOutsideContract` 로 끝난다. 연결은 한 바이트도 안 쓴다.
  그 거절은 `UnsupportedCapability` 와 같은 성질이다 — 호스트가 답한 실패와 다른 타입이라는 사실이
  "아무것도 일어나지 않았다" 를 뜻한다.
- **서버는 바꾸지 않는다.** 키를 실은 forward 는 지금처럼 키를 무시하고 넘긴다. 거절(새 에러 코드)로
  바꾸면 이미 키를 싣고 있는 client 의 호출이 실패로 돌아선다.
- **정확히 한 번은 target 의 몫이다.** 같은 forward 가 두 번 오면 target plugin 이 두 번 부름을
  받고, 호출자는 두 답을 받으며, 어느 답에도 재생 표지가 없다. 이 사실은 시험으로 고정한다 — 누가
  호스트에 조용한 중복 제거를 넣으면 선언과 동작이 갈리기 때문이다.

## Consequences

- **얻은 것**: plugin 이름에 키를 실은 재시도가 **조용한 두 번째 실행**이 되던 자리가, 요청이 나가기
  전의 타입 있는 거절이 됐다.
- **얻은 것**: "계약 밖" 이 산문이 아니라 표의 값이라, 다른 소비자(CLI · 문서 생성)가 같은 판정을
  읽을 수 있다.
- **잃은 것 — 정확히 한 번이 필요한 plugin 호출에 호스트가 줄 수 있는 도구가 없다.** plugin 은
  자기 params 안에 요청 id 를 받아 스스로 걸러야 한다.
- **★ 잃은 것 — 버전 차이의 비용.** client 의 표는 그 client 가 빌드된 시점의 것이다. 호스트에 새
  메서드가 생기고 client 가 그것을 모르면, 그 이름에 키를 실은 호출은 client 에서 `Outside` 로
  거절된다. 방향은 안전하다(두 번째 실행이 아니라 안 보냄) — 그 대가로 새 메서드에서 키를 쓰려면
  client 를 올려야 한다.
- **잃은 것**: 구 client 는 이 선언을 모르므로 여전히 키를 실어 보낸다. 서버가 동작을 안 바꿨으니
  그 호출은 예전처럼 두 번 실행될 수 있다 — 이 결정이 닫는 것은 새 client 쪽이다.
- **유지 부담**: `MethodMeta` 를 만드는 세 const fn 과 fallback 이 필드를 하나 더 채운다.

## Alternatives Considered

- **⒜ forward 앞에도 보존소를 건다** — 호출자에게는 가장 좋은 답이다. 안 고른 이유는 셋이다.
  보존소 파일이 이 작업의 소유 밖이다. `forward_namespace_call` 은 키를 받지 않아 호스트 층을 가로질러
  새 인자가 흘러야 한다. 그리고 forward 의 답은 **비동기로** 온다(plugin 이 나중에 답한다) — 보존소의
  "진행 중 상태를 두지 않는다" 는 전제(호스트 실행이 직렬이라 둘째 요청이 볼 때 첫째는 끝나 있다)가
  forward 에서는 안 선다. 진행 중 상태를 새로 설계해야 하는 일이라 회차 크기가 아니다.
- **서버가 키를 실은 forward 를 거절한다** — 선언을 서버가 강제하는 형태라 구 client 까지 막는다.
  그 대가로 이미 동작하던 호출이 실패로 바뀐다. 외부 호환을 가장 많이 지키는 쪽을 골랐다.
- **표가 모르는 이름을 `Undeclared` 로 둔다** — 새 호스트 메서드의 버전 차이 비용이 없어진다. 대신
  client 프로세스에서 plugin 이름이 전부 통과해 이 결정이 무효가 된다.
- **`Covered`(계약 안) 값을 같이 둔다** — engine 라우터로 가는 `Mutate` 에 붙일 수 있다. 그러나 App
  층이 끝내는 메서드와 가르는 목록이 표에 없고, 그 목록을 세는 것은 다른 작업의 몫이다. 틀린 선언은
  선언 없음보다 나쁘다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 호스트가 같은 forward 를 거르거나 재생하게 되면
  `manager::tests_forward_idempotency::the_same_forward_twice_runs_the_target_twice_and_is_never_replayed`
  가 빨개진다 — 그때는 ⒜ 가 착지한 것이므로 이 ADR 을 Supersede 한다.
- namespace fallback 의 선언이 `Outside` 가 아니게 되면
  `method_meta::tests::a_forwarded_namespace_name_is_declared_outside_the_key_contract` 가 잡는다.
  표의 호스트 메서드에 선언이 붙는 것은 ADR-0423 으로 이미 일어났고, 그 값은
  `a_host_method_declaration_follows_its_effect` 가 본다.
- client 가 계약 밖 이름에 키를 실어 보내게 되면
  `client::tests::a_refused_key_never_touches_the_connection` 이 잡는다.
- `forward_namespace_call` 이 멱등 키를 인자로 받게 되면 — ⒜ 의 둘째 장애가 없어진 것이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **버전 차이 거절이 실제로 나는가.** 재는 법: `KeyOutsideContract` 를 받은 호출자의 보고에서 그
  메서드가 plugin 이름이 아니라 호스트 메서드인지 본다. 그렇다면 client 가 낡은 것이다.
- **정확히 한 번이 필요한 plugin 호출이 생기는가.** 재는 법: plugin 이 params 에 자체 요청 id 를
  받아 거르기 시작하는지(같은 코드가 둘 이상의 plugin 에 생기면 호스트 도구가 필요하다는 신호다).

## References

- 관련 ADR: [ADR-0338](0338-a-mutation-retry-is-told-apart-by-a-caller-key-and-the-peer-is-asked-before-the-effect.md)
  — 멱등 키 계약. 이 ADR 은 그 결정이 후속으로 남긴 두 층 가운데 namespace forward 를 계약 밖으로
  확정한다. App 층은 다루지 않는다.
- 관련 dev-guide: [api-conventions](../dev-guide/api-conventions.md) 의 멱등 키 절.
- 부분 개정: [0423](0423-each-method-declares-its-key-contract-and-the-version-that-keeps-it.md) (`key_contract` 의 값 집합 개정)
- 후속 결정: [ADR-0566](0566-every-host-path-keeps-the-idempotency-key-and-only-a-plugin-name-is-outside.md)
  — forward 로 나가는 표의 이름(`image.open` 등)도 보존소를 지나게 한다. plugin 고유 이름은 이 결정
  그대로 계약 밖이다.
- **코드 근거 (결정이 실현된 현재 위치)**: `tasty-ipc` 의 `method_meta::KeyContract` ·
  `MethodMeta::key_contract` · `method_meta::key_contract`, `client::KeyOutsideContract` ·
  `IpcConnection::send_idempotent`, 그리고 `tasty-host-plugin` 의
  `PluginManager::forward_namespace_call`.
