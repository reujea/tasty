# ADR-0173: namespace 해소는 프로세스 표가 아니라 매니페스트를 읽는다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: plugin, ipc, routing, headless, namespace, error-codes, adr-0167, adr-0136, adr-0171
- **Group**: plugin-system

## Context

`<prefix>.<method>` 호출을 owner plugin 으로 넘길지 정하는 판정이 두 조합에서 **다른
자리**에 있었다. gui 는 라우터 step 5 에서 `mgr.ipc_namespaces.resolve()` 로 정하고,
헤드리스는 engine 응답의 **오류 코드**(`-32601` / `-32017`)를 보고 "engine 이 못
답했나" 를 물은 뒤에야 같은 `resolve()` 를 불렀다.

**재료가 다른 것이 아니었다** — 최종 판정은 양쪽 다 `resolve()` 다. 다른 것은
**물을 수 있는 시점**이었다. `ipc_namespaces` 는 `on_plugin_spawn_success` 에서만
채워졌으므로, plugin 을 안 띄우는 헤드리스 데몬은 **소속을 묻기 위해 먼저 띄워야**
했다. 오류 코드는 그 비용을 낼지 말지의 문턱이었다.

그 구조가 만든 것이 둘이다.

1. **정적 사실을 묻는 대가가 프로세스 아홉이었다.** 실측(2026-09-05, 설치 끝난 홈,
   헤드리스 데몬): 라우팅되는 호출(`surface.list`)은 35~72 ms 에 프로세스 0 인데,
   **호스트가 모르는 이름을 한 번 부르면**(오타 포함) 첫 응답이 1272 ms(기동 후
   92 ms)로 늘고 plugin 프로세스 9 개 · 자식 RSS 합 약 47 MB 가 **데몬 수명 내내**
   남았다.
2. **종단이 내는 코드가 라우팅 신호로 고정됐다.** [ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md)
   이 `-32017` 을 신설하자, 표에 등재된 채 번들 plugin namespace 아래 있던 여덟
   (`image.*` 7 · `markdown.navigate`)이 forward 를 못 타게 됐다. 한 줄로 닫았지만
   (`is_unrouted_here` 가 두 코드를 함께 본다), 그 한 줄은 `-32601` 이 호출자에게는
   "없다" 를, 라우팅에게는 "plugin 으로 넘겨라" 를 뜻한다는 사실을 고정했다.

그리고 같은 표가 **두 물음을 겹쳐 지고 있었다**: "누가 소유하는가" 와 "지금 살아
있는가". 그래서 disable 된 plugin 의 메서드가 `-32601`("그런 메서드 없다")로
답했는데, 그것은 **거짓**이다 — 있고, 꺼져 있다.

## Decision

**라우팅 해소의 재료를 실행 상태에서 매니페스트로 옮긴다.**

`IpcNamespaceRegistry` 는 이제 **설치된** plugin 의 선언을 담는다.
`refresh_packages()`(디스크 스캔) 가 채우고, 패키지가 사라질 때만 지운다. spawn ·
disable · swap · 무응답 재시작은 이 표를 건드리지 않는다.

"지금 살아 있는가" 는 **이미 있던 다른 술어**가 답한다 —
`validate_namespace_call` 의 `processes.contains_key` 가 내는
`-32002 plugin '<id>' is not running`. 그 분기는 코드에 이미 쓰여 있었고 프로덕션에서
도달할 수 없었다(해소가 실행 중 집합이라 언제나 `-32601` 이 먼저 났다). 이 결정이
그 분기를 되살린다. **호출부를 두 함수로 가를 필요가 없었던 이유가 이것이다** —
`resolve()` 호출부 넷(gui 라우터 · 헤드리스 dispatch · plugin→plugin 경로 ·
`validate_namespace_call`)은 전부 "누가 소유하는가" 를 물었고, 생존은 그 옆에서
따로 물어지고 있었다.

따라오는 두 가지:

- **헤드리스는 forward 판정을 engine 호출 앞으로 옮긴다**(gui 와 같은 순서).
  `is_unrouted_here` 를 지운다 — 오류 코드는 라우팅에 더 쓰이지 않는다.
  소속은 디스크만 읽는 층(`ensure_plugin_manager_metadata`)으로 묻고, **소속이 맞을
  때만** 기동한다.
- **헤드리스는 번들 plugin 설치를 부팅에서 한다**(gui 가 창 생성에서 하는 것과 같은
  자리). 예전에는 그 설치가 "호스트가 모르는 이름을 처음 부를 때" 딸려 왔다 — 즉
  오타 하나가 설치와 기동을 함께 시켰다. 그 우연한 트리거가 사라지므로 설치를 제
  자리로 옮긴다. **기동은 여전히 지연이다**: 부팅 시 프로세스는 하나도 안 뜬다.

## Consequences

- **얻은 것**: 오타는 이제 아무것도 안 띄운다. 실측 대조 — 같은 오타
  (`zz.no_such_thing`)가 이전 1272 ms · 프로세스 9, 이후 **98 ms · 프로세스 0**.
- **얻은 것**: 꺼진 plugin 의 메서드가 참을 답한다. gui 실측 —
  `plugin.disable com.tasty.markdown` 뒤 `markdown.zz_probe_no_such` 와
  `markdown.navigate` 둘 다 `-32002 plugin 'com.tasty.markdown' is not running`.
  이전에는 각각 `-32601 Method not found`(거짓)와 `-32602 missing field surface_id`
  (**host arm 이 답했다** — 설치 상태에 따라 답하는 주체가 바뀌던 형태,
  `src/source_guards/bundled_plugin_namespace_coverage.rs` 가 경계하는 그 표면)였다.
  재-enable 하면 둘 다 원래 응답으로 돌아온다(A–B–A 확인).
- **얻은 것**: 종단(`unrouted_for_external_caller`)이 코드를 하나 더 늘려도 forward 가
  안 깨진다. ADR-0167 이 치른 여덟의 대가는 이 구조에서는 발생하지 않는다.
- **잃은 것 / 바뀐 것**: `method_meta` 의 해소도 같은 집합을 본다(당시에는 사본이었고
  지금은 host 가 든 표를 그대로 본다).
  즉 **설치돼 있으면 꺼져 있어도** 그 prefix 가 등록된 것으로 보인다. 이것이 위
  `-32002` 를 가능하게 하는 바로 그 변화다.
- **운영 비용**: 헤드리스 부팅이 번들 설치(파일 복사 + 매니페스트 권한 grant)를
  한다. 첫 부팅에만 실제 복사가 일어나고 이후는 `copy_if_newer` 가 no-op 다.
  프로세스는 안 뜬다. 부수 효과로 **갓 만든 헤드리스 홈에서 `plugin.list` 가
  0 이 아니라 실제 개수를 답한다**(이전에는 누군가 오타를 칠 때까지 0 이었다).

### 실측 (2026-09-05)

세 상태에서 "매니페스트가 선언한 namespace" ∖ "실행 중 namespace" 의 크기:

| 상태 | 선언 | 실행 중 | 차 | 그 차가 오늘 받던 답 |
|---|---|---|---|---|
| 차가운 헤드리스 데몬 | 6 | 0 | 6 | **관측 불가** — 묻는 순간 기동돼 차가 0 이 된다 |
| 전부 enable(gui) | 6 | 6 | 0 | — |
| 하나 disable(gui) | 6 | 5 | 1 | `-32601 Method not found`(거짓) |

선언 6 = `agent_stream` · `claude` · `codex` · `html` · `image` · `markdown`.

프로세스 수를 세는 방법에는 양성 대조를 붙였다 — 같은 세는 방법
(`ps --ppid <데몬> | grep -c plugin`)이 plugin 소속 이름 호출 뒤에 **9** 를 냈다.
그러니 위 0 들은 "안 세는 0" 이 아니다.

forward 의존 여덟은 변경 뒤에도 전부 plugin 에 닿는다(헤드리스 실행 확인, 응답이
`host call 'call#N' failed:` 로 감싸여 온다 = plugin 이 host 로 되부른 자취).

## Alternatives Considered

- **A: 헤드리스도 `ipc_namespaces` 로 판정하되 표는 그대로 spawn 이 채운다** — 소속을
  묻기 위해 매 호출마다 기동해야 하므로 위 1.2 s · 프로세스 9 가 **아무 IPC 호출
  하나** 시점으로 당겨진다. plugin 을 영영 안 쓰는 데몬까지 문다.
- **B: 비대칭을 의도로 못 박고 기록만 한다** — 그러면 `is_unrouted_here` 가 신호로
  보는 코드 목록이 하중을 지는 명부가 되고, 종단이 코드를 늘릴 때 그 목록도 같이
  늘어야 한다는 것을 강제할 채널이 새로 필요하다. 고칠 수 있는 것을 지키기로
  바꾸는 선택이라 안 골랐다.
- **C: 해소 함수를 둘로 가른다**(라우팅용=설치됨 / 생존용=실행 중) — `resolve()`
  호출부를 전수해 보니 **생존을 묻는 자리가 하나도 없었다.** 가를 것이 없다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 헤드리스 부팅의 번들 설치가 체감될 만큼 비싸진다 — 재는 명령:
  갓 만든 `TASTY_HOME` 으로 데몬을 띄우고 `plugin.list` 가 처음 답할 때까지의 시간.
  설치를 다시 지연시키되 **오타가 아닌 트리거**를 찾아야 한다.
- `resolve()` 호출부 중 "지금 살아 있는가" 를 뜻해서 부르는 자리가 생긴다 — 그때는
  대안 C(두 함수로 가르기)로 간다.
- disable 된 plugin 의 메서드가 `-32002` 를 받는 것이 소비자를 깨뜨린다는 관측이
  나온다 — 재는 자리는 그 소비자의 오류 처리 분기다.

## References

- [ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md) — 이
  결정이 없애는 결합(종단 코드 ↔ 라우팅 신호)을 만든 변경
- [ADR-0136](0136-a-query-does-not-create-what-it-observes.md) — 조회가 관측 대상을
  바꾸지 않는다. 설치를 조회 층이 아니라 부팅으로 올린 이유가 이 규약이다
- [ADR-0171](0171-a-host-error-code-survives-the-plugin-boundary.md) — 위 실측에서
  plugin 을 거쳐 온 응답이 원래 코드를 유지하는 근거
- `src/source_guards/bundled_plugin_namespace_coverage.rs` — 설치 상태에 따라 답하는
  주체가 바뀌던 표면을 경계하는 가드
