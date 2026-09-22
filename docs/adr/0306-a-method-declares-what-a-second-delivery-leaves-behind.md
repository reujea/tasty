# ADR-0306: 메서드는 "두 번 전달되면 무엇이 남는가" 를 표에 선언한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: ipc, method-table, retry, contract
- **Group**: ipc-contract

## Context

`crates/tasty-ipc` 의 `METHOD_TABLE` 은 메서드마다 **누가 부를 수 있고 어떤 권한이
필요한가**를 답한다. 답하지 않는 것이 하나 있었다 — **같은 요청이 두 번 도착하면 무슨
일이 벌어지는가.**

그 물음이 필요한 자리가 생겼다. 응답을 못 받은 호출자가 다시 보내도 되는지 정하려면
메서드마다 그 답이 있어야 하는데, 표에도 핸들러에도 그것을 말하는 칸이 없었다. 실측
(2026-09-20, 표 등재 335 개): `mutating` · `read_only` · `side_effect` 어느 어휘도
0 건. 유일하게 가까운 것은 `crates/tasty-agent` 의 `lease` · `semaphore` 가 자기 doc 에
"같은 holder 면 idempotent" 라고 적어 둔 두 자리뿐이고, 그것은 표가 아니라 구현의 주석이다.

이름으로 추론하는 것은 안 된다. 실측으로 확인한 반례가 여럿이다.

- `message.read` — 이름은 읽기인데 `peek` 기본값이 `false` 라 **읽으면 소비한다.**
- `ui.screenshot` — 이름은 조회인데 **파일을 남긴다.**
- `surface.set_mark` — 이름은 `set` 인데 값이 "지금" 이라 재전달이 mark 를 **옮긴다.**
- `output.observe_start` — 이름은 `start` 인데 **새 observer id** 가 난다.
- `surface.read_since_mark` — 이름이 read 이고 실제로도 커서를 **안 옮긴다**(`&self`).

## Decision

`MethodMeta` 에 `effect: MethodEffect` 를 더하고, 축을 **"이 요청이 두 번 전달되면
관측 가능한 차이가 남는가"** 로 정한다. "읽기인가" 가 아니다.

- `Read` — 호스트 상태를 안 바꾼다. 두 번째 전달이 아무 흔적도 안 남긴다.
- `Idempotent` — 상태를 바꾸지만 두 번째 전달이 같은 끝 상태로 수렴한다.
- `Mutate` — 두 번째 전달이 두 번째 효과를 남긴다.

값은 **생성자가 요구한다**(`plugin` · `plugin_only` · `local_only` 가 첫 인자로 받는다).
그래서 새 메서드는 분류 없이는 표에 못 들어가고, 분류를 빠뜨린 상태가 존재할 수 없다.
plugin namespace 로 넘어가는 이름은 호스트가 뜻을 모르므로 `Mutate` 로 가정한다 —
모를 때 고를 값은 가장 조심스러운 쪽이다.

## Consequences

- **얻은 것**: "어떤 메서드가 변경 명령인가" 가 값으로 답해진다. 재전달 안전성을 다루는
  후속 작업이 대상 집합을 이름 규칙이 아니라 표에서 얻는다.
- **얻은 것**: 분류가 표와 같은 자리에 있어 **갈릴 수 없다.** 별도 목록으로 뒀다면 목록과
  표가 서로를 안 보는 두 사본이 됐다.
- **잃은 것**: 등재 335 자리가 전부 한 인자씩 길어졌다. 텍스트로 표를 읽는 판독기들은
  영향을 안 받는다 — 권한 목록을 `&[` 부터 찾으므로 앞에 인자가 붙어도 같은 것을 읽는다
  (변이로 확인했다).
- **운영 비용**: 새 메서드마다 사람이 한 번 판단해야 한다. 그것이 목적이다 — 기본값을 두면
  그 기본값이 답으로 굳는다.

## Alternatives Considered

- **A: 별도 테이블 `METHOD_EFFECT`** — 표를 안 건드려도 된다. 안 골랐다: 두 목록이
  갈리면 어느 쪽이 정본인지 알 수 없고, 갈렸을 때 조용하다. 표에 새 메서드를 더하면서
  이 목록을 빠뜨리는 것이 기본 동작이 된다.
- **B: 네 갈래 — `Read` / `Idempotent` / `Mutate` / "부수효과 있는 읽기"** — 안 골랐다:
  재전달 축에서 부수효과 있는 읽기는 `Mutate` 다(두 번째 호출이 두 번째 흔적을 남긴다).
  네 번째 칸을 두면 그 칸이 "읽기니까 다시 보내도 된다" 로 읽히는데, 그것이 정확히 틀린
  결론이다. 갈래는 그 값으로 무엇을 결정하느냐가 정한다.
- **C: 이름 규칙으로 유도(`*.list` 는 읽기, `*.create` 는 변경)** — 안 골랐다: 위 Context
  의 다섯 반례가 전부 이름 규칙을 어긴다. 규칙으로 유도하면 그 다섯이 조용히 틀린다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 세 갈래 중 하나가 비면 그 값은 계약이 아니라 장식이다.
  `method_meta_tests` 의 `every_branch_of_the_effect_classification_is_used` 가 본다.
- 앵커로 박은 반례들이 이름 규칙대로 다시 칠해지면
  `the_effect_axis_is_redelivery_not_the_verb` 가 빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **분류가 실제 핸들러 동작과 맞는가.** 표의 값과 핸들러 구현을 잇는 채널은 없다 — 핸들러를
  고쳐 멱등이 아니게 만들어도 표는 그대로다. 재는 법: 그 메서드의 핸들러를 같은 파라미터로
  두 번 태우고(격리 `TASTY_HOME` 의 headless 인스턴스) 두 번째 응답과 상태가 첫 번째와
  구별되는지 본다. 구별되면 `Mutate` 여야 한다.
- **`Idempotent` 로 적었지만 시간이 값에 섞이는 것.** `surface.set_mark` 가 그 형태라
  `Mutate` 로 뒀다. 같은 형태가 새로 생기면 이름이 `set` 이라는 이유로 `Idempotent` 가
  되기 쉽다. 재는 법: 두 전달 사이에 상태를 바꾸고(출력 도착 등) 그래도 끝 상태가 같은지 본다.

## References

- 결정이 실현된 현재 위치: `crates/tasty-ipc/src/method_meta.rs` 의 `MethodEffect` ·
  `MethodMeta::effect` · `plugin` / `plugin_only` / `local_only`
- 시험: `crates/tasty-ipc/src/method_meta_tests.rs` 의
  `every_branch_of_the_effect_classification_is_used` ·
  `the_effect_axis_is_redelivery_not_the_verb` ·
  `a_forwarded_namespace_name_is_assumed_unsafe_to_redeliver`
- 이미 멱등 계약을 자기 doc 에 적어 둔 선례: `crates/tasty-agent/src/lease.rs` ·
  `crates/tasty-agent/src/semaphore.rs`
- 표를 텍스트로 읽는 판독기: `crates/tasty-doc-guards/src/lib.rs` 의 `method_table` ·
  `KNOWN_CTORS`
