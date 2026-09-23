# ADR-0167: 등재된 이름은 "없다" 가 아니라 "이 바이너리에 안 들어 있다" 로 답한다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: ipc, error-codes, headless, build-combination, guards, adr-0154, adr-0163
- **Group**: ipc-contract

## Context

`window.create` · `window.list` · `webview.set_url` 처럼 `#[cfg(feature = "gui")]` 뒤에
있는 메서드를 헤드리스(`--no-default-features`) 데몬에서 부르면, `match` 팔이 컴파일에서
통째로 사라져 호출이 `_` 로 떨어지고 외부 dispatch 의 종단이 답했다. 그 답이 `-32601`
이었다.

### 오타와 구분되지 않았다

실측(2026-09-05, 격리 `TASTY_HOME` 헤드리스 데몬, 327 이름 전수 프로브):

    window.creat    -32601 Method not found: window.creat      ← 오타
    window.create   -32601 Method not found: window.create     ← 표에 있고 이 빌드엔 없다

두 응답은 코드·메시지·`data` 가 전부 같았다. 그 프로브에서 `-32601` 로 끝난 것은 54 개였고
(release 표 22 · debug 표 31 · 추출 artifact 1), **54/54 가 같은 한 형태**였다 — `data` 가
실린 것은 하나도 없었다. 호출자는 "이름을 잘못 썼다" 와 "이 조합에는 없다" 를 응답만으로
가를 수 없었고, `-32601` 이 보내는 방향(이름을 의심한다)에는 고칠 것이 없었다.

닿는 범위는 좁다. agent 토큰으로 부르면 같은 이름이 `-32001`("not callable from …")로
먼저 걸리므로, 이 구분 불가는 **Local / CLI 호출자**에게만 닿는다.

### 같은 거짓을 이 저장소가 이미 두 번 고쳤다

[ADR-0154](0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) 가 플랫폼 축을
`-32015`("여기선 못 한다")로, [ADR-0163](0163-a-registered-name-answers-who-not-whether.md)
가 caller 축을 `-32016`("주체가 다르다")로 갈랐다. 둘 다 "`-32601` 을 유지하고 사유는 다른
자리에 싣자" 를 명시적으로 기각했다 — ADR-0163 의 표현으로 "이 축의 결함은 표가 아니라
답에 있다". 남은 것이 **빌드 조합 축**이고, 이 ADR 이 셋째다.

### 이 결함이 라우팅 층 결함과 같은 가족인가 — 아니다

같은 날 잡힌 두 결함(지목한 대상이 라우팅에 안 실리던 것)과 층이 같은지 변이 둘로 갈랐다.

| 변이 | 거부 형태(`-32601`/`-32016`) | named-target 가드 |
|------|------------------------------|-------------------|
| 종단(`unrouted_for_external_caller`)의 `plugin_only` 갈래 무력화 | **바뀐다** 4/4 | 안 바뀐다 |
| 라우팅(`request_resource_id`)을 항상 `None` | 안 바뀐다 6/6 | **죽는다** 3/3 |

뒤쪽 변이가 살아 있다는 것은 pristine 바이너리를 나란히 띄운 A–B–A 로 확인했다
(`surface.close{surface_id:999999}` 가 `-32602 no live surface …` 에서 `ok {closed:false}` 로
바뀌었다). 두 층은 직교한다 — 한 자리에서 풀리지 않는다.

## Decision

외부 dispatch 종단(`JsonRpcResponse::unrouted_for_external_caller`)에 셋째 갈래를 둔다.
이름이 **표에 그 이름 그대로 등재돼 있으면** 다음으로 답한다.

    -32017  method '<name>' is registered but this binary has no dispatch arm for it:
            it is gated out of this build combination (headless / release)

**갈래의 술어는 `method_meta::is_registered_name`** — `METHOD_TABLE` 과 `DEBUG_METHODS` 를
그 이름 그대로 조회하는 것뿐이다. `method_meta()` 로 물으면 안 된다.

그리고 **헤드리스의 plugin namespace forward 는 두 코드를 함께 신호로 본다**
(`src/boot/headless_dispatch.rs` 의 `is_unrouted_here`). 그 자리가 묻는 것은 "표에 있나" 가
아니라 "engine 이 못 답했나" 하나이기 때문이다.

### 왜 술어를 그렇게 좁히는가 — 재서 정했다

`method_meta()` 는 4 단계다: `METHOD_TABLE` → `DEBUG_METHODS` → 정적 `PREFIX_RULES` →
**런타임 등록 plugin prefix**. 마지막 단계 때문에 그 함수는 설치된 plugin 의 이름과 그 아래
**임의의 오타**에도 `Some` 을 준다. 그 술어로 갈래를 태워 실행한 결과(실측 2026-09-05,
plugin 이 기동된 헤드리스 데몬):

    claude.children        -32017   ← plugin 이 답해야 하는데 host 가 삼켰다
    agent_stream.list      -32017   ← 원래 성공하던 호출
    markdown.no_such_thing -32017   ← 오타인데 "빌드에 없다" 로 답한다(새 거짓)

**설치된 plugin 의 표면이 통째로 죽는다.** 모수도 유한하지 않다 — 그 prefix 아래 어떤
이름이든 해당된다. 이것은 소스를 읽어서는 나오지 않았고 돌려서 나왔다.

## Consequences

- **얻은 것**: 호출자가 오타와 조합 부재를 응답만으로 가른다. 실측으로 `window.creat` 는
  `-32601`, `window.create` 는 `-32017` 이다. 그리고 `-32015` · `-32016` 과 함께 종단이
  답하는 사실이 네 갈래로 닫힌다(주체 / 플랫폼 / 조합 / 이름).
- **잃은 것**: 에러 코드가 하나 늘었다(`-32017`). 헤드리스에서 `-32601` 을 기대하던 e2e
  단언 넷이 `-32017` 로 바뀐다(`plugin.enable` · `window.list` · `webview.set_url` ·
  debug 삼총사) — 그 자리들은 원래 "없는 것이 정답" 을 못 박던 곳이고, 지금은 더 정확한
  사실을 못 박는다.
- **잃을 뻔한 것 — 재서 0 으로 만들었다**: 헤드리스의 plugin forward 가 `-32601` 을 신호로
  쓰고 있었다. 그래서 **표에 등재된 채 번들 plugin namespace 아래 있는 여덟**(`image.*` 7 ·
  `markdown.navigate`)이 새 코드를 받으면 forward 를 못 타고 plugin 이 답하던 호출이 host 의
  거절로 바뀐다. 실측으로 확인했고(`image.save` 가 `-32602 missing 'surface'` 에서
  `-32017` 로), `is_unrouted_here` 가 두 코드를 함께 보게 해 잔여를 0 으로 만들었다.
- **운영 비용**: 새 메서드가 조합 게이트 뒤로 들어가면 그 이름의 헤드리스 응답이 `-32601`
  에서 `-32017` 로 바뀐다. 표에서 이름을 빼면 반대로 돌아간다 — 즉 **표 등재가 이 답의
  하중을 진다.** 그 어긋남은 e2e 가 잡는다(위 단언들이 코드를 정확히 못 박는다).

## Alternatives Considered

- **`-32601` 을 유지하고 `error.data` 에 사유를 싣는다** — 기각. 헤드리스 forward 가
  코드만 보므로 이 선택은 확실히 안전해 보였고, 실제로 그 이유로 한 번 제안했다. 그러나
  같은 축을 이미 두 번 결정한 ADR-0154 · ADR-0163 이 정확히 이 형태를 기각했다 —
  "응답은 그대로 `-32601` 이라 호출자가 듣는 거짓이 안 고쳐진다". 사유를 곁들여도 코드가
  거짓이면 코드로 분기하는 호출자는 여전히 속는다. 그리고 forward 안전성은 이 ADR 이
  `is_unrouted_here` 한 줄로 얻었으므로, 그 이점은 대안의 전유물이 아니다.
- **`method_meta()` 를 그대로 술어로 쓴다** — 기각. 위 실측대로 plugin 표면을 삼킨다.
- **표를 외부에 노출해(예: `system.methods`) 호출자가 미리 읽게 한다** — 기각(지금은).
  부르기 전에 읽을 수 있으면 응답 코드의 역할이 줄지만, 그 표면은 별도 결정이고 그것이
  생겨도 **이미 부른 호출**에는 답이 필요하다. ADR-0163 이 같은 대안을 재검토 트리거로
  남겼고 여기서도 그렇게 둔다.
- **헤드리스 forward 를 gui 처럼 namespace 해소로 바꾼다** — 이 ADR 에서는 안 한다. 위
  여덟의 대가는 전적으로 그 비대칭에서 나오므로 근본은 그쪽이 맞다. 다만 그것은 두 조합의
  라우팅 재료를 통일하는 별개 축이고, 이 축의 대가는 한 줄로 0 이 되어 급하지 않다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 헤드리스 forward 가 오류 코드가 아니라 namespace 해소로 판정하게 됐을 때 — 그러면
  `is_unrouted_here` 의 두 코드 병기가 불필요해지고, 종단 코드가 forward 와 완전히 분리된다.
- 조합 게이트 뒤에 있는 이름이 표의 절반을 넘을 때 — 그때는 "예외를 코드로 말한다" 가 아니라
  조합별로 표 자체를 갈라야 한다는 신호다.
- 메서드 표를 외부에 노출하는 표면이 생겼을 때 — ADR-0163 과 같은 트리거다.
- `-32017` 을 제어 흐름에 쓰는 소비자가 `is_unrouted_here` 말고 더 생겼을 때 — 코드가
  제어 신호가 되면 그 의미가 고정되어 이 축의 다음 정정이 다시 막힌다.

## References

- [ADR-0163](0163-a-registered-name-answers-who-not-whether.md) — 같은 거짓의 caller 축(`-32016`)
- [ADR-0154](0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) — 플랫폼 축(`-32015`)
- [ADR-0143](0143-a-named-target-is-checked-before-the-engine-in-headless.md) — 헤드리스 종단 앞뒤 순서
- [api-conventions](../dev-guide/api-conventions.md) — 네 코드의 관계 표
- [headless-ipc-surface](../dev-guide/headless-ipc-surface.md) — 두 조합의 메서드별 판정
- 부분 개정: [0173](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md) (헤드리스 forward 신호 조항 개정 — `is_unrouted_here` 를 지우고 소속을 매니페스트로 묻는다)
