# ADR-0253: 수명주기 토글 둘은 `App` 이분을 기다리지 않는다 — cascade 전체가 아니라 그 둘이 내는 이벤트만 헤드리스 형태로 대체한다

- **Status**: Accepted
- **Date**: 2026-09-09
- **Tags**: ipc, headless, plugin, lifecycle, routing, agent-surface, adr-0127, adr-0173
- **Group**: architecture

## Context

헤드리스 데몬에서 plugin 을 켜는 길이 **없었다.** `plugin.enable` · `plugin.disable` 은
등재된 이름이지만 그 조합에 dispatch arm 이 없어 `-32017` 로 답했다. gui arm 은
`src/app/ipc/app_methods.rs` 에 있고 그 모듈을 여는 선언이 `#[cfg(feature = "gui")]`
이라 **모듈이 통째로 사라진다.** 그 자리는 [ADR-0127](0127-e2e-harness-binary-selection.md)
이 "`App` 이분이 선행" 이라고 적어 둔 여덟 중 둘이었고, 문서와 실제가 일치했다 —
불일치가 아니라 **없던 길**이었다.

그런데 없는 것이 아니었다. 켜는 경로가 **부작용으로만** 있었다:

    $ tasty image list     # 이 호출 자체는 -32017 로 실패한다
    $ tasty plugin list    # → 설치된 9 개가 전부 running=true

plugin namespace 로 forward 될 때 `ensure_plugin_manager` 가 `discover_and_start` 를
부르기 때문이다([ADR-0173](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md)
이 소속 판정을 매니페스트로 옮긴 뒤에도 **기동** 경로는 그대로다). 그 경로에는 개별
지목이 없어서 **하나만 띄울 수 없었고**, 그래서 헤드리스에서 plugin 하나를 관측하려면
관측 대상을 여덟 개 더 만들어야 했다. 그 우회는 어느 문서에도 절차로 적혀 있지 않았다.

`App::plugin_enable` / `plugin_disable` 의 본체가 실제로 만지는 것은 `App.plugin_manager`
하나다 — `view` 도 `parked_states` 도 안 읽는다. gui 를 요구하는 것은 그 뒤의
`cascade_plugin_events` 이고, 그것이 첫 main window 의 `PendingHostEvent` 큐를 쓴다.

## Decision

**토글 둘만 연다.** `cascade_plugin_events` 전체를 헤드리스로 옮기지 않고, 이 두
메서드가 실제로 내는 `CoreEvent` 둘(`PluginEnableToggled` · `PluginUnloaded`)만
헤드리스 형태로 대체한다(`cascade_toggle_events_headless`). 나머지 여섯
(`plugin.install` · `remove` · `grant` · `revoke` · `upgrade_builtins` · `audit_follow`)
은 파일을 복사·삭제하거나 권한을 바꾸는 일이라 **각각이 별도 결정**으로 남는다.

본체는 두 라우터가 **같은 함수 하나**를 부른다 —
`crate::ipc::handler::plugin::dispatch_lifecycle_toggle`. 파라미터 이름, 성공 응답의 칸
이름(`enabled` / `disabled`), 실패 문구의 접두어(`enable failed:`)는 **에이전트가 보는
계약**이라 사본을 두지 않는다. gui 의 `App::plugin_enable` / `plugin_disable` 도 같은
함수의 얇은 위임이 된다. 발화하는 이벤트 키와 payload(`plugin.enabled` /
`plugin.disabled` / `plugin.unloaded`)도 한 벌만 두고, gui 의 host event drain 이 그것을
위임해 부른다.

**헤드리스에서 매니저는 메타데이터 층까지만 세운다**(`ensure_plugin_manager_metadata`).
번들 설치는 부팅이 이미 했고, `PluginManager::enable` 은 그 package 표에서 **지목한
하나만** 찾아 기동한다. 여기서 `ensure_plugin_manager`(= `discover_and_start`)를 부르면
하나를 켜라는 명령이 설치된 전부를 띄운다 — 이 갈래를 나눈 이유가 그것이다.

## Consequences

- **얻은 것 — 개별 기동.** 실측(2026-09-09, 격리 홈 헤드리스 데몬): 부팅 직후 9 개 전부
  `running=false` → `plugin enable com.tasty.image` → `running` 이 `["com.tasty.image"]`
  하나. plugin 은 실제로 hello 를 마치고 egui-mesh kind `image` 를 등록했다. 같은 날 갓
  만든 홈에서 namespace 경로도 함께 쟀고 그쪽은 여전히 9 개 전부다 — **두 경로가 같은
  일을 다른 크기로 한다**는 것이 이제 값으로 갈린다.
- **얻은 것 — 관측이 대상을 덜 만든다.** 헤드리스에서 plugin 하나를 재려면 여덟을 더
  띄워야 했다. 그것은 [ADR-0136](0136-a-query-does-not-create-what-it-observes.md) 이
  조회에 대해 막은 것과 같은 형태가 **기동 쪽에** 남아 있던 자리다.
- **잃은 것 — 헤드리스 cascade 는 두 이벤트만 안다.** 토글이 세 번째 종류의
  `CoreEvent` 를 내게 되면 헤드리스는 그것을 발화하지 않고 `warn` 로그만 남긴다. gui 는
  `cascade_plugin_events` 가 일곱 종류를 받으므로 조합에 따라 하는 일이 갈린다. 그
  비대칭을 없애는 것이 `App` 이분이고, 이 ADR 은 그것을 앞당기지 않는다.
- **잃을 뻔한 것 — 반대 방향의 비대칭.** gui 는 창이 하나도 없으면 이벤트를 **아무것도
  발화하지 않는다**(`enqueue_plugin_host_event` 가 첫 main window 를 못 찾으면 조용히
  돌아간다). 헤드리스는 매니저를 직접 들고 있어 그 자리에서 낸다. 즉 헤드리스 쪽이 더
  많이 발화한다 — 이 차이는 없애지 않고 `docs/dev-guide/headless-ipc-surface.md` 에
  적었다. 없애려면 gui 가 창 없이도 발화하게 해야 하고, 그것은 이 회차의 물음이 아니다.
- **운영 비용**: 새 `plugin.*` 쓰기를 헤드리스에 열 때마다 "그 메서드가 내는 이벤트를
  누가 소비하나" 를 다시 답해야 한다. 값싸게 열리는 것은 소비처가 매니저 하나로 닫히는
  메서드뿐이다.

## Alternatives Considered

- **A. `cascade_plugin_events` 를 통째로 헤드리스 스텁까지 이분한다** — 기각. 그것이
  ADR-0127 이 "`App` 이분이 선행" 이라고 적은 일이고, `src/app.rs` 한 파일의 gui 게이트
  약 80 곳이 걸린다. 이 회차의 크기가 아니다. 대신 **필요분만** 대체했다.
- **B. 헤드리스 `enable` 이 `ensure_plugin_manager` 를 부른다** — 기각. 그러면 하나를
  켜라는 명령이 설치된 전부를 띄운다. 지금 있는 부작용 경로와 구별이 안 되므로 이
  회차가 열려는 것을 정확히 못 연다.
- **C. 부작용 경로를 절차로 문서화하고 끝낸다** — 기각. 그 경로는 개별 지목이 원리적으로
  불가능하고, "실패하는 호출을 일부러 쳐서 기동시킨다" 를 절차로 못 박으면 그 호출의
  실패 코드가 나중에 정확해질 때 절차가 조용히 깨진다(ADR-0173 이 같은 형태를 한 번
  겪었다).
- **D. 헤드리스는 이벤트를 아예 안 낸다** — 기각. 헤드리스가 host event 브로드캐스트를
  생략해 온 것은 사실이지만(`finalize_plugin_hello_headless`), 그것은 소비 주체가 없던
  자리의 이야기다. `plugin.enabled` 를 구독한 plugin 은 헤드리스에서도 살아 있고,
  안 내면 **같은 명령이 조합에 따라 다른 사실을 그 plugin 에게 준다.**

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 남은 여섯 중 하나가 헤드리스 dispatch 에 들어온다 — 그러면 "토글 둘만" 이라는 이
  결정의 범위가 낡는다. `tests/e2e_tests.rs` 의
  `the_remaining_lifecycle_methods_are_still_absent_in_a_headless_daemon` 이 그 자리에서
  빨개진다(`plugin.remove` · `plugin.grant` 가 `-32017` 이 아니게 되므로).
- `dispatch_lifecycle_toggle` 이 두 라우터 중 한쪽에서만 불리게 된다 — 그러면 계약이
  두 벌이 된다. `src/source_guards/headless_app_layer_coverage.rs` 의
  `a_shared_dispatch_is_called_by_both_routers` 가 그 자리에서 빨개진다: 호출자를
  **라우터 파일별로 나눠** 세고, `src/app/ipc/app_methods.rs` 와
  `src/boot/headless_dispatch.rs` 양쪽에 호출이 있어야 통과한다.
  같은 함수를 `DELEGATED_ROUTERS` 도 명부로 들고 있지만 **그것은 이 트리거의 채널이
  아니다** — 그 대조가 재는 것은 *호출자가 하나라도 있는가* 라, 한쪽이 자기 인라인
  사본으로 돌아가도 다른 쪽이 계속 부르는 한 초록이다(실측: gui 호출만 별칭으로
  우회시키면 새 시험만 빨개지고 `the_roster_is_reached_from_the_request_method` 는
  통과했다). 그 명부가 잡는 것은 이름이 통째로 사라지는 경우뿐이고, 그건 이 결정의
  트리거가 아니다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 위 "잃을 뻔한 것" 의 비대칭(창 없는 gui 는 발화하지 않고 헤드리스는 발화한다)이 실제
  문제로 관측되면. 재는 법: 창이 하나도 없는 gui 인스턴스에서 `plugin disable <id>` 를
  치고, `plugin.disabled` 를 구독한 plugin 이 그 이벤트를 받았는지 `debug.event_bus.*`
  로 확인한다 — 헤드리스에서 같은 절차를 밟은 결과와 대조한다.

## References

- `docs/dev-guide/headless-ipc-surface.md` — `plugin.*` 19 개의 메서드별 판정. 이 결정의
  현재 운영 상태가 그 문서의 "답한다 — 수명주기 토글" 절이다.
- `docs/dev-guide/self-verification.md` — 헤드리스에서 plugin 하나를 띄우는 절차와, 두
  경로(개별 지목 · namespace 부작용)의 크기 차이 실측.
- [ADR-0127](0127-e2e-harness-binary-selection.md) — "`App` 이분이 선행" 의 출처.
- [ADR-0173](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md) —
  소속은 매니페스트가, 기동은 그 뒤가 정한다.
- [ADR-0136](0136-a-query-does-not-create-what-it-observes.md) — 관측이 자기 대상을
  바꾸지 않는다. 이 결정은 그 원칙을 기동 쪽으로 넓힌다.
- [ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md) — `-32017`
  의 출처. 그 결정의 "잃은 것" 이 세는 e2e 단언 넷에 `plugin.enable` 이 들어 있는데,
  **이 결정이 그 자리를 옮겼다** — 그 단언은 이제 `plugin.remove` · `plugin.grant` 를
  잰다(`the_remaining_lifecycle_methods_are_still_absent_in_a_headless_daemon`).
  0167 본문은 그 시점의 기록이라 고치지 않는다.
- 코드 근거(결정이 실현된 현재 위치): `dispatch_lifecycle_toggle` ·
  `cascade_toggle_events_headless` · `emit_enable_toggled` · `emit_unloaded`
  (`src/adapters/ipc/handler/plugin.rs`).
