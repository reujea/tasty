# ADR-0423: 메서드마다 멱등 키 계약과 그것을 지키는 판을 선언한다 — ADR-0361 의 선언 값 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, protocol, idempotency, capability, method-meta, compatibility, adr-0312, adr-0338, adr-0361, adr-0421
- **Group**: ipc-contract

## Context

[ADR-0361](0361-a-plugin-namespace-forward-is-declared-outside-the-idempotency-contract.md) 은 이름
표(`method_meta`)에 `key_contract` 칸을 더하고 값을 둘로 뒀다 — `Outside`(plugin namespace forward)와
`Undeclared`(호스트 메서드 전부). "걸린다" 를 뜻하는 값은 **일부러** 두지 않았다: 호스트 메서드
가운데 App 층이 끝내는 것은 보존소에 안 걸렸고, 그 목록이 표에 없었다. 틀린 선언은 선언 없음보다
나쁘다는 판단이었다.

그 결과 호출자가 "이 메서드에 키를 실으면 지켜지나" 를 **사전에** 알 길이 없었다. 사후 표지인
응답의 `idempotent_replay` 는 한 방향으로만 답했다 — 붙으면 계약이 걸린 것이지만, 안 붙은 것은 계약
밖일 수도 계약이 걸린 채 실행을 막은 답(`-32063`)일 수도 있었다. 규칙을 문자 그대로 읽은 호출자가
`-32063` 을 "계약 밖" 으로 읽고 키 없이 재전송하면, 계약이 막아 준 두 번째 효과를 스스로 낸다.

이제 전제가 바뀌었다. [ADR-0421](0421-the-app-layer-keeps-the-idempotency-contract-and-a-running-key-is-joined.md)
로 App 층도 보존소를 지나고, 남는 층은 plugin namespace forward(계약 밖)와 GUI debug step(release 에
없는 표면) 둘이다. "어느 호스트 메서드가 걸리나" 를 적을 수 있게 됐다.

남는 문제가 하나 있다 — **판 차이**. client 의 표는 그 client 가 빌드된 시점의 것이고, 서버는 다른
판일 수 있다. App 층 보존소가 없는 서버(판 1)에 새 client 가 창 생성 키를 실으면 서버는 키를 무시하고
재시도가 두 번째 실행이 된다. `ipc.idempotency-key` capability 는 그때까지 판 1 하나였다.

## Decision

**`KeyContract` 를 세 값으로 바꾼다 — `Kept { since }` · `Unneeded` · `Outside`. `Undeclared` 는
없앤다. `ipc.idempotency-key` 의 판을 2 로 올리고, client 는 메서드의 `since` 를 요구 판으로 쓴다.**

- **"라우터 보존소에 걸리는가" 는 `key_contract` 와 같은 물음이다** — 둘 다 "같은 키로 다시 보내면 두
  번째 효과가 나는가" 에 답한다. 그래서 칸을 따로 두지 않고 ADR-0361 의 칸에 값을 더한다.
  `MethodEffect` 와는 다른 물음이다: 같은 `Mutate` 라도 보존소를 지나는 층과 안 지나는 층이 있다.
- **값은 대부분 유도한다.** 표의 생성자가 `effect` 에서 정한다 — `Mutate` → `Kept { since: 1 }`,
  `Read`·`Idempotent` → `Unneeded`. 손으로 적는 것은 층이 다른 이름뿐이다: App 층 `Mutate` 여섯은
  `.kept_by_app_layer()`(판 2), GUI debug step 의 `Mutate` 일곱은 `.outside_key_contract()`.
  `kept_by_app_layer` 는 `Mutate` 가 아니면 컴파일 시점에 실패한다.
- **손으로 적은 두 무리는 실제 배선과 맞댄다.** 본체의 `source_guards::key_contract_by_layer` 가 GUI
  app_methods step 본문 · debug step 파일 · 헤드리스 App 층 가로채기가 부르는 이름을 읽어, 판 2 선언의
  집합과 `Outside` 선언의 집합이 **정확히** 그 층의 `Mutate` 와 같은지 양방향으로 잰다.
- **판은 층을 말한다.** 판 1 = engine 라우터가 받는다, 판 2 = App 층까지 받는다. 서버가 선언하는 판은
  표가 요구하는 가장 높은 판이다(`capability.rs` 가 상수에서 유도하고, 시험이 그 둘을 맞댄다).
  `IpcConnection::send_idempotent` 는 `Kept { since }` 면 그 판을, `Unneeded` 면 최소 판(1)을 요구한다.
  모자라면 `UnsupportedCapability` 로 끝나고 요청은 안 나간다.
- **표지의 단방향성은 선언이 푼다.** 계약 안인지 밖인지는 보내기 전에 선언으로 안다. 그래서 응답의
  `idempotent_replay` 는 "이 답이 재생인가" 하나만 답하면 된다 — `Kept` 메서드에서 표지 없는 성공은
  **이번에 실행한 것**이고, `-32063`·`-32064` 는 계약이 개입한 답이다. 표지의 부재를 "계약 밖" 으로
  읽을 자리가 없어진다.
- **wire 는 안 바뀐다.** 선언은 client 프로세스의 표로 읽는다(`ipc.method-effect` 와 같은 방식). 바뀌는
  wire 값은 capability 목록의 판 숫자 하나다.

**ADR-0361 에서 개정하는 것**: `key_contract` 의 값 집합("값은 둘이다 — `Outside` 와 `Undeclared`",
"'걸린다' 를 뜻하는 값은 두지 않는다", "표의 모든 호스트 메서드는 `Undeclared`").

**ADR-0361 에서 개정하지 않는 것**: plugin namespace forward 가 계약 밖이라는 결정, 표가 모르는 이름을
`Outside` 로 읽는 규칙, client 가 capability 확인 **앞에서** 계약 밖 이름을 `KeyOutsideContract` 로
거절하는 순서, 서버가 키를 실은 forward 를 거절하지 않는다는 결정, 정확히 한 번은 target plugin 의
몫이라는 결정.

## Consequences

- **얻은 것**: 호출자가 보내기 **전에** 안다 — 이 메서드에 키를 실으면 보존소가 받는가(`Kept`),
  원래 안전한가(`Unneeded`), 계약 밖인가(`Outside`).
- **얻은 것**: 판 1 서버에 App 층 키를 실어 재시도가 두 번째 실행이 되는 경로가 요청 전 거절이 된다.
- **잃은 것 — 판 1 서버에 App 층 키를 못 싣는다.** 그 서버는 어차피 그 키를 무시했다. 잃는 것은 "키를
  실어 보냈다" 는 착각뿐이다.
- **잃은 것 — GUI debug step 의 `Mutate` 에 키를 못 싣는다.** 헤드리스에서는 그 이름이 App 층
  가로채기 안이라 실제로는 보존소를 지나지만, 선언은 조합마다 갈리지 않으므로 보수적인 쪽을 적는다.
- **운영 비용**: 새 App 층 `Mutate` 를 더하면 표에 `.kept_by_app_layer()` 를, debug step 에 `Mutate` 를
  더하면 `.outside_key_contract()` 를 붙여야 한다 — 빠뜨리면 `key_contract_by_layer` 가 그 자리에서
  잡는다.

## Alternatives Considered

- **`method_meta` 에 "라우터 보존소에 걸리는가" 칸(`bool`)을 따로 둔다** — `key_contract` 와 같은 물음을
  두 칸에 적게 된다. 두 칸이 갈리면(forward 가 `Outside` 인데 `true`) 어느 쪽을 믿을지 정할 규칙이 또
  필요하다.
- **판을 안 올리고 `Kept` 하나만 둔다** — 새 client 가 판 1 서버에 창 생성 키를 싣고, 서버는 무시한다.
  ADR-0338 이 capability 를 둔 이유(보내기 전에 상대가 읽는지 안다)가 층 단위에서 다시 무너진다.
- **판 대신 새 capability 이름(`ipc.idempotency-key.app-layer`)을 더한다** — 스트림 기능이 이름을 더한
  이유는 그 판을 서버가 **동등 비교**해서였다(`ipc.stream.loss-notify`). 이 이름은 이상 비교(`>=`)라
  판을 올려도 구 client 가 거절되지 않는다. 이름이 둘이면 client 가 둘을 조합해 읽어야 한다.
- **선언을 `system.info` 로 내려 서버 표를 읽게 한다** — 판 차이가 사라진다. 대신 메서드 수만큼의
  목록이 `system.info` 에 실리고, 연결마다 한 번 받는 응답이 커진다. `ipc.method-effect` 가 이미
  client 표로 읽는 방식을 골랐고, 판 숫자가 층을 말하므로 그것으로 충분하다.
- **`Unneeded` 를 두지 않고 `Read`·`Idempotent` 도 `Kept` 로 적는다** — 그 메서드는 보존소에 안
  들어가고 재생 표지가 안 붙는다. "`Kept` 에서 표지 없는 성공 = 이번 실행" 이라는 읽기가 거짓이 된다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- App 층 · debug step 의 `Mutate` 와 선언이 갈리면 `source_guards::key_contract_by_layer` 의 세 시험이
  빨개진다(변이 확인은 References 의 커밋에 적었다).
- 표의 `Kept` 판 최댓값과 capability 판이 갈리면
  `capability::tests::the_idempotency_version_is_the_highest_one_the_table_requires` 가 잡는다.
- `Mutate` 가 아닌데 `Kept`/`Outside` 인 이름이 생기면 `method_meta::tests::a_host_method_declaration_follows_its_effect`
  가 잡는다.
- client 가 App 층 메서드에 판 1 서버로 키를 싣게 되면
  `client::tests::an_app_layer_key_needs_the_version_that_keeps_it` 가 잡는다.
- GUI debug step 이 app_methods step 앞으로 옮겨져 보존소를 지나게 되면 — debug `Mutate` 의 `Outside`
  를 걷을 날이다. 그때 `key_contract_by_layer` 의 둘째 시험이 판 2 쪽으로 옮겨야 한다고 알린다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **판 1 서버를 쓰는 호출자가 `UnsupportedCapability` 를 받는가.** 재는 법: 그 보고의 `required` 가 2
  이고 메서드가 App 층 이름인지 본다 — 그렇다면 서버를 올리는 것이 답이다.

## References

- 개정 대상: [ADR-0361](0361-a-plugin-namespace-forward-is-declared-outside-the-idempotency-contract.md)
  (`key_contract` 의 값 집합)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 부분 개정: [0566](0566-every-host-path-keeps-the-idempotency-key-and-only-a-plugin-name-is-outside.md) (GUI debug step 의 `Outside` 조항 개정, 판 3 추가)
- 관련 ADR: [ADR-0312](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md) —
  capability 선언. [ADR-0338](0338-a-mutation-retry-is-told-apart-by-a-caller-key-and-the-peer-is-asked-before-the-effect.md)
  — 멱등 키 계약. [ADR-0421](0421-the-app-layer-keeps-the-idempotency-contract-and-a-running-key-is-joined.md)
  — App 층 배선.
- 관련 dev-guide: [api-conventions](../dev-guide/api-conventions.md) 의 멱등 키 절.
- **코드 근거 (결정이 실현된 현재 위치)**: `tasty-ipc` 의 `method_meta::KeyContract` ·
  `KEY_KEPT_BY_ROUTER` · `KEY_KEPT_BY_APP_LAYER` · `MethodMeta::kept_by_app_layer`,
  `client::required_key_version` · `IpcConnection::send_idempotent`,
  `capability::CAPABILITIES`; 본체의 `source_guards::key_contract_by_layer`.
