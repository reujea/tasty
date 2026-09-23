# ADR-0566: 호스트가 아는 이름은 어느 경로로 끝나든 멱등 키를 지킨다 — ADR-0423 의 debug step 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: ipc, protocol, idempotency, capability, method-meta, plugin, namespace, debug, partial-amendment, adr-0361, adr-0421, adr-0423
- **Group**: ipc-contract

## Context

[ADR-0421](0421-the-app-layer-keeps-the-idempotency-contract-and-a-running-key-is-joined.md) 로 App 층이
보존소를 지나게 됐고, [ADR-0423](0423-each-method-declares-its-key-contract-and-the-version-that-keeps-it.md)
이 메서드마다 `KeyContract` 를 선언했다. 그 뒤 호스트가 키를 실은 요청을 끝내는 경로를 소스에서 다시
세면(2026-09-23, GUI `App::ipc_dispatch_command` 의 단계 순서와 헤드리스 `headless_dispatch::dispatch_command`
를 읽었다) 보존소를 안 지나는 경로가 둘 남아 있었다.

1. **plugin namespace forward 로 나가는 표의 이름.** 이름 표에는 prefix 가 예약되지 않은 호스트 메서드가
   있다 — 번들 plugin 이 그 prefix 를 점유했기 때문이다(`image.*` · `markdown.*`). 그중 `Mutate` 는 여섯이다:
   `image.open` · `image.export_png` · `image.next` · `image.prev` · `image.paste` · `markdown.navigate`.
   표 조회가 namespace fallback 보다 먼저라 이 여섯은 `Kept { since: 1 }` 로 선언돼 있었다. 그러나 plugin 이
   켜져 있으면 GUI 라우터 step 5 와 헤드리스 `forward_to_plugin_namespace` 는 **이름 표를 보지 않고 prefix
   만으로** forward 한다(`PluginManager::owns_namespace`). plugin 은 받아서 호스트로 되부르는데
   (`host.call`) 그 되부름에는 키가 없다. 그래서 선언은 "재시도가 재생이다" 인데 실제로는 두 번째
   실행이었고, client 는 그 선언을 믿고 키를 실어 보냈다 — 조용한 거짓이었다.
2. **GUI debug step.** app_methods step 뒤의 `debug_methods` · `window_required` step 은 App 층이 연
   자리가 닫힌 뒤에 돌아 보존소를 안 지났다. ADR-0423 은 거기의 `Mutate` 일곱을 `Outside` 로 선언해
   client 가 키를 싣지 않게 했다 — 선언은 참이었지만 계약 밖인 호스트 이름이 남았다.

나머지는 배선돼 있거나 선언이 참이었다: engine 라우터(`route_checked_request`), 두 조합의 App 층
(`run_app_layer`), 헤드리스 debug 이름(App 층 가로채기 안에서 끝나 이미 보존소를 지난다), 그리고 표가
모르는 plugin 고유 이름(ADR-0361 의 `Outside`). plugin → 호스트 호출(`plugin_call_request`)은 봉투에
키를 싣지 않아 우회할 키가 없다.

## Decision

**호스트가 아는 `Mutate` 이름은 어느 경로로 끝나든 보존소를 지난다. 남은 두 경로를 배선하고, 그 둘이
받기 시작한 판을 `ipc.idempotency-key` 판 3 으로 선언한다. 계약 밖(`Outside`)은 표가 모르는 이름 —
plugin 고유 이름 — 뿐이다.** `Read` · `Idempotent` 이름은 재전달이 원래 안전해 어느 경로에서도 보존소에
안 들어간다(`Unneeded`, ADR-0423 그대로).

- **forward 경로.** `idempotency::forward_keeping_the_key` 가 이름 표의 선언을 보고 `Kept` 면
  `run_app_layer` 로 보존소를 지나게 한 뒤, 키를 뗀 사본과 relay 통로로 forward 한다. plugin 의 답은
  나중에 오므로 App 층과 같은 진행 중 상태 · 합류가 그대로 쓰인다. `Kept` 가 아니면(plugin 고유 이름)
  개입하지 않고 원래 요청을 넘긴다. GUI 라우터와 헤드리스가 **같은 함수**를 부른다.
- **plugin 고유 이름을 계약 밖으로 지키는 것은 `Kept` 판정 하나뿐이다.** `forward_keeping_the_key` 는
  이름 표의 `KeyContract` 가 `Kept` 인지 보고, 그 뒤 `run_app_layer` 가 `MethodEffect::Mutate` 인지 본다.
  뒤 판정은 plugin 고유 이름을 거르지 않는다 — forward 는 prefix 가 점유됐을 때만 일어나고, 그때
  `method_meta` 는 그 이름을 namespace fallback 으로 `Mutate` · `Outside` 로 해소한다. 그래서 `Kept`
  판정이 빠지면 plugin 고유 이름이 보존소에 들어가 같은 키의 재시도가 재생이 되고, ADR-0361 의 `Outside`
  와 반대 결과가 된다. 그 갈래는 prefix 를 등록한 채 부르는 통제군 시험이 잰다(아래 재검토 트리거).
- **GUI debug step.** 두 step 을 한 함수(`App::ipc_step_debug_layers`)로 묶고 그 첫 줄에서
  `run_app_layer` 를 부른다. 따로 감싸면 첫 step 이 안 맡은 이름을 둘째가 또 연다.
- **판 3.** `KEY_KEPT_ON_EVERY_HOST_PATH = 3`. forward 로 나갈 수 있는 표 이름(예약 밖 prefix 의
  `Mutate`)과 debug step 의 `Mutate` 가 `.kept_on_every_host_path()` 로 판 3 을 선언한다. 서버가 선언하는
  판은 표의 최댓값이라 3 이 된다. 판 2 서버는 그 경로에서 키를 무시했으므로, 새 client 는 그 이름에
  판 3 을 요구하고 모자라면 보내지 않는다.

**ADR-0423 에서 개정하는 것**: "GUI debug step 의 `Mutate` 일곱은 `.outside_key_contract()`" 와 "잃은
것 — GUI debug step 의 `Mutate` 에 키를 못 싣는다", 그리고 판이 층을 말하는 목록("판 1 = engine 라우터,
판 2 = App 층")에 판 3 을 더한다.

**ADR-0423 에서 개정하지 않는 것**: `KeyContract` 의 세 값, 값 대부분을 `MethodEffect` 에서 유도하는
규칙, client 가 `since` 를 요구 판으로 쓰는 규칙, 표가 모르는 이름을 `Outside` 로 읽는 규칙, wire 가
capability 판 숫자 하나만 바뀐다는 조항.

**ADR-0361 은 개정하지 않는다** — plugin 고유 이름이 계약 밖이고 정확히 한 번은 target plugin 의
몫이라는 결정은 그대로다. 이 결정이 배선하는 것은 표의 이름뿐이다.

## Consequences

- **얻은 것**: 이름 표가 `Kept` 로 선언한 이름에서 선언과 동작이 갈리는 자리가 없어졌다. `image.open` 같은
  이름에 키를 실은 재시도가 plugin 을 두 번 거쳐 두 번 실행되던 형태가, 첫 답의 재생이 된다.
- **얻은 것**: 손으로 적던 `Outside` 무리가 없어졌다. 호스트 `Mutate` 이름의 선언은 판(1·2·3)만 다르고, 계약 밖은
  표의 값이 아니라 표가 모른다는 사실에서만 나온다.
- **잃은 것 — 판 2 서버에 그 이름들의 키를 못 싣는다.** 그 서버는 그 경로에서 키를 무시했다. 잃는 것은
  "키를 실어 보냈다" 는 착각뿐이다. 판 2 까지의 client 는 `image.open` 을 판 1 로 읽고 계속 키를 싣는데,
  이제 서버가 그것을 지킨다 — 옛 client 도 이 변경의 이득을 본다.
- **잃은 것 — 키를 실은 `Mutate` 가 GUI debug 빌드에서 relay 스레드를 한 번 더 잠깐 세운다.** debug
  묶음이 engine 라우터로 갈 이름도 열었다 닫기 때문이다(ADR-0421 의 App 층과 같은 비용). release 에는
  그 묶음이 보존소를 열지 않는다.
- **운영 비용**: 예약 밖 prefix 에 `Mutate` 를 더하거나 debug step 에 `Mutate` 를 더하면 표에
  `.kept_on_every_host_path()` 를 붙여야 한다 — 빠뜨리면 아래 시험이 그 자리에서 잡는다.

## Alternatives Considered

- **forward 로 나가는 표 이름을 `Outside` 로 선언한다** — 배선 없이 선언만 참으로 만든다. 그 이름은
  plugin 이 꺼져 있으면 engine 라우터로 가서 지켜지므로, 설치 상태에 따라 참이 달라지는 값에 보수적인
  쪽을 적게 된다. 그리고 호출자는 창 생성과 같은 성질의 surface 생성에서 재시도 안전성을 잃는다.
- **라우팅 step 전체를 감싼다** — engine 라우터까지 relay 스레드를 지나게 된다. engine 라우터는 이미
  동기 `begin`/`finish` 로 지켜지고 진행 중 상태가 닿지 않는다(ADR-0421).
- **plugin 고유 이름까지 forward 앞에서 보존한다**(ADR-0361 의 대안 ⒜) — 진행 중 상태는 이제 있으므로
  기술적 장애는 줄었다. 그러나 호스트는 그 이름의 뜻을 모르고, "정확히 한 번은 target 의 몫" 이라는
  plugin 쪽 계약을 바꾸는 별개의 결정이다.
- **debug step 은 `Outside` 로 둔다**(ADR-0423 그대로) — release 에 없는 표면이라 이득이 작다. 그러나
  배선은 함수 하나이고, 그것으로 손으로 유지하던 `Outside` 무리와 그 가드 한 갈래가 없어진다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- forward 경로가 `Kept` 이름을 보존하지 않게 되면
  `idempotency::tests::a_forwarded_host_method_runs_once_per_key_and_the_retry_is_a_replay` 가, plugin 고유
  이름까지 보존하게 되면 `a_plugin_name_is_forwarded_every_time_with_its_request_untouched` 가 빨개진다.
  뒤쪽은 그 이름의 prefix 를 시험 바이너리의 소유 표에 **등록한 채** 부른다 — 등록하지 않으면
  `method_meta` 가 `None` 이라 `run_app_layer` 가 먼저 빼 주고, `Kept` 판정을 지워도 통과한다(운영에서는
  등록 안 된 prefix 가 forward 되지 않으므로 그 상태는 운영에 없다).
- ADR-0361 의 재검토 트리거("호스트가 같은 forward 를 거르거나 재생하게 되면
  `manager::tests_forward_idempotency::the_same_forward_twice_runs_the_target_twice_and_is_never_replayed`
  가 잡는다")는 **이 결정이 배선한 층을 못 본다.** 그 시험은 `tasty-host-plugin` 의 `PluginManager` 층을
  재고, 그 크레이트는 루트 크레이트에 의존하지 않아 `forward_keeping_the_key` 가 그 시험 바이너리에 없다
  — `Kept` 판정을 지워 plugin 고유 이름이 재생되게 해도 그 시험은 통과한다(2026-09-23 실측). 그리고 이
  결정 뒤로 호스트 바이너리는 표 이름의 forward 를 실제로 거르고 재생한다. 호스트 바이너리의 forward 앞
  보존소가 plugin 고유 이름까지 거르게 되는 것은 위 통제군이 잡는다.
- 두 조합의 forward 자리가 `forward_keeping_the_key` 밖에서 `forward_namespace_call` 을 부르게 되면
  `source_guards::key_contract_by_layer::both_namespace_forwards_go_through_the_store` 가 잡는다.
- GUI dispatch 가 debug step 을 보존소 밖에서 부르게 되면 같은 모듈의
  `the_gui_debug_steps_run_behind_the_store` 가 잡는다.
- 세 자리(두 forward · debug 묶음)가 보존소에 넘기는 relay closure 가 자기 인자(키를 뗀 사본) 대신
  바깥 명령을 쓰게 되면 — 답이 원 통로로 나가 결말이 기록되지 않거나 키를 단 명령이 제 자리에 합류한다 —
  같은 모듈의 `the_relay_closure_uses_only_its_own_argument` 가, debug 묶음의 `handled` 판정이
  `IpcStep::Handled` 를 안 보게 되면 `the_debug_layers_judge_handled_by_the_step` 가 잡는다. 둘 다
  호출 자리의 모양을 재는 텍스트 가드이고, 그 closure 를 실제 dispatch 로 돌려 재는 행동 시험은 없다.
- 예약 밖 prefix 의 `Mutate` 와 판 3 선언이 갈리면
  `method_meta::tests::a_host_mutation_under_a_claimable_prefix_is_kept_from_version_three` 가, debug step
  의 `Mutate` 와 갈리면 `key_contract_by_layer::the_debug_step_mutations_are_exactly_the_debug_names_kept_from_version_three`
  가 잡는다.
- 번들 plugin 이 예약 prefix 를 점유하게 되거나(예약 목록이 바뀌면) 판 3 의 좌변이 바뀐다 — 위 시험이
  그 날의 좌변으로 다시 잰다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **실제 plugin 을 거친 forward 가 한 번만 실행되는가.** 단위 시험은 forward 를 흉내 낸다. 재는 법:
  image plugin 이 켜진 격리 `TASTY_HOME` 의 debug 인스턴스에 같은 키로 `image.open` 을 두 번 보내
  `surface.list` 의 이미지 surface 수와 둘째 답의 `idempotent_replay` 를 본다.

## References

- 개정 대상: [ADR-0423](0423-each-method-declares-its-key-contract-and-the-version-that-keeps-it.md)
  (GUI debug step 의 `Outside` 조항, 판 목록)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 선행 결정: [ADR-0421](0421-the-app-layer-keeps-the-idempotency-contract-and-a-running-key-is-joined.md)
  (`run_app_layer` 를 다른 두 경로에 적용한다) · [ADR-0361](0361-a-plugin-namespace-forward-is-declared-outside-the-idempotency-contract.md)
  (개정하지 않는다 — plugin 고유 이름) · [ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md)
  (예약 밖 prefix 가 생기는 이유). 탐색: `git grep -l 'outside_key_contract\|KEY_KEPT_BY_APP_LAYER' -- docs/adr/`
- 관련 dev-guide: [api-conventions](../dev-guide/api-conventions.md) 의 멱등 키 절.
- **코드 근거 (결정이 실현된 현재 위치)**: `idempotency::forward_keeping_the_key`, `App::ipc_step_debug_layers`,
  GUI `App::ipc_step_routing` · 헤드리스 `forward_to_plugin_namespace` 의 forward 자리, `tasty-ipc` 의
  `method_meta::KEY_KEPT_ON_EVERY_HOST_PATH` · `MethodMeta::kept_on_every_host_path` ·
  `capability::CAPABILITIES`.
