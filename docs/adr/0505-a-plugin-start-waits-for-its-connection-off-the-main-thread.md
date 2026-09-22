# ADR-0505: plugin 기동은 연결을 메인 스레드 밖에서 기다리고, 연결 전의 요청은 쌓았다가 보낸다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: plugin, host-plugin, lifecycle, startup, handshake, main-thread, concurrency, adr-0457

## Context

`PluginProcess::spawn` 은 자식을 띄운 뒤 plugin 이 호스트 listener 에 연결(토큰 인증)하기를 **그 자리에서**
최대 `HANDSHAKE_TIMEOUT`(10 s) 기다렸다. 부르는 자리는 기동 창구 `start_plugin_internal` 하나이고, 그
창구에 닿는 호출자는 명시적 `enable`(IPC · CLI) · namespace 호출이 owner 를 띄우는 자리
(`start_one_enabled`) · 회수 뒤의 재기동(무응답 재시작 · `upgrade-builtins` 가 이어받은 예약) · swap ·
전체 기동(`discover_and_start`)이다. 앞의 넷은 호스트 메인 스레드에서 돈다. 메인 스레드가 IPC 응답과
프레임을 함께 처리하므로, 연결이 느린 plugin 하나가 그동안 **모든 IPC 와 화면**을 세웠다 — ADR-0457 이
종료 대기에서 없앤 것과 같은 종류의 정지다.

재어졌다(격리 `TASTY_HOME` 헤드리스 debug 인스턴스, 연결 전에 잠드는 probe plugin, 2026-09-23). 5 s 뒤에
연결하는 plugin 을 `plugin enable` 하자 CLI 가 5121 ms 걸렸고 같은 구간의 `list info` 최댓값이 5098 ms 였다.
끝내 연결하지 않는 plugin 은 enable 10122 ms · `list info` 최댓값 9954 ms 였다.

같은 자리에 경합이 하나 더 있었다. 연결을 받을 토큰은 자식을 띄운 **뒤에** 등록됐다. 자식을 띄우는
호출(`spawn_bound`)은 영속 spawner 스레드에 fork 를 맡기고 답을 기다리므로, 부하로 호출 스레드가 늦게
깨면 곧바로 연결하는 plugin 이 등록보다 먼저 인증해 "unknown token" 으로 거절됐고, 호스트는 한도(10 s)까지
기다린 뒤 기동 실패로 쳤다. 시각을 찍어 보면 거절이 `spawn_bound` 가 돌아오기 2.5 ms 전에 났다. 번들
시험 `an_upgrade_that_writes_nothing_does_not_wait_for_a_retirement` 가 부하에서 "다시 뜨지 않았다" 로
가끔 깨지던 것이 이것이다 — 같은 시험 12 개 병렬 + 바쁜 루프 40 개에서 36 번 중 21~23 번 깨졌고, 깨진
수와 거절 수가 같았다.
같은 틈은 plugin 채널의 Nagle 을 끈 변경(`set_nodelay`)이 인증 줄의 지연을 없애면서 GUI 부팅에서도 드러났고,
그 변경이 같은 순서(등록 → spawn)로 독립적으로 고쳤다. 두 구현은 이 결정의 형태(`HostListener::register` →
`PendingConnection`, 기다림은 연결 대기 스레드)로 합쳤다.

전체 기동은 이 문제의 바깥이다. GUI 부팅의 `discover_and_start` 는 부팅 워커 스레드(`tasty-boot-engine`)에서
돈다. 그리고 부팅은 그 뒤에 hello 를 짧은 시한(`PLUGIN_WAIT_DEADLINE` 300 ms)으로 기다리므로, 연결을 그
시한 안으로 밀어 넣으면 레이아웃 복원이 plugin kind 를 못 보고 지나갈 수 있다.

## Decision

**기동은 자식을 띄우자마자 돌아온다. 연결 대기와 송수신 스레드 기동은 `plugin-connect-<id>` 스레드가
맡고, 연결 전에 온 요청은 송신 큐에 쌓였다가 연결 뒤 순서대로 나간다. 연결의 결과는 매니저가 pump 에서
거둔다. 전체 기동만 연결의 결과까지 기다린다.**

- **기동은 기다리지 않는다.** `spawn` 은 연결을 받을 자리를 등록하고(자식을 띄우기 **전에** — 위 경합을
  닫는다), 자식을 띄우고, 채널을 만든 뒤 돌아온다. 프로세스는
  곧바로 `processes` 에 들어가므로 `is_running` 은 참이다.
- **연결 전의 요청은 버리지도 거절하지도 않는다.** 송신 큐가 spawn 시점에 이미 살아 있고, 연결되면
  송신 스레드가 그 큐를 처음부터 비운다. 그래서 "`enable` → 곧바로 namespace 호출" 과 namespace 호출이
  owner 를 띄우는 경로는 예전처럼 plugin 에 닿는다.
- **결과는 pump 가 한 번 거둔다(`settle_connections`).** 연결이 성사되면 연속 실패 기록을 지운다. 끝내 안
  오면(한도 10 s) 예전 spawn 실패와 같은 갈래로 보낸다 — 프로세스를 내리고, `plugin.error`
  (`spawn_failed`)를 내고, 연속 실패로 센다(자동 비활성 한도는 10 s 안에 3 회라, 한 건이 10 s 걸리는 연결
  실패만으로는 닿지 않고 빠른 spawn 실패와 섞일 때만 닿는다 — 예전과 같다). 내리는 것은 요청 없이
  곧바로 kill 하는 종료 핸들이고, kill 과 회수는 ADR-0457 의 회수 스레드가 한다. 연결을 기다리던 사이 쌓인
  namespace 호출은 caller 에 오류로 회신한다. 그 사이 그 extension 에 보낸 hook(pre/post × ipc/event)은
  **보낸 적 없는 것으로** 진행한다 — 예전에는 extension 기동이 먼저 실패해 hook 송신이 실패했고 원래
  흐름이 hook 없이 이어졌다(`bypass_hooks_sent_to`). pre-ipc hook 을 건너뛰고 target 에 보내는 요청에는 송신
  실패 갈래처럼 호출한 plugin id 를 싣는다(로컬 호출이면 없다). 실행된 적 없는 hook 이라 연속 실패로 세지 않는다.
- **연속 실패 기록은 연결 성사 때 지운다 — 자식을 띄운 때가 아니다.** 띄운 때 지우면 매번 연결에 실패하는
  plugin 의 누적이 기동마다 0 으로 돌아가 자동 비활성이 영영 안 걸린다.
- **전체 기동(`discover_and_start`)은 연결의 결과까지 기다린다.** 부팅의 hello 시한이 연결 시간에 먹히지
  않게 하려는 것이다. 연결 대기는 plugin 마다 스레드라 겹치므로, 이 대기는 예전의 합(Σ)이 아니라 가장
  느린 하나로 수렴한다.
- **요청의 시한은 받는 plugin 의 연결 성사부터 센다.** 예전에는 기동이 연결까지 막혔으므로 기동 직후 보낸
  요청의 시한은 연결 뒤에 섰다. 그 호환을 지키려고 연결 중인 plugin 에 보낸 요청은 아직 만료를 따지지
  않고, 연결이 성사되면 기다린 만큼 시한을 민다 — 시한의 길이는 그대로다(`deadline_from_connection`).
  pending 만료 판정 한 자리(`collect_expired_request_ids`)가 이 규칙을 쓰므로 extension pre-hook
  (`timeout_ms`, 수십 ms) · namespace 호출(`NAMESPACE_CALL_TIMEOUT`) · 나머지 hook 이 같은 규칙을 받는다.
  헤드리스 `--type <kind>` 의 hello 대기(ADR-0259 의 `KIND_REGISTRATION_WAIT`)는 메인 스레드의 동기 루프라
  같은 규칙을 그 자리에서 쓴다 — 먼저 연결 결과를 연결 한도(= 예전에 spawn 안에서 막히던 값)까지 기다리고,
  연결이 성사된 뒤부터 5 s 를 센다. 끝내 연결 안 하는 owner 는 연결 한도에서 끝난다.
- **스레드를 못 띄우면 그 자리에서 기다린다** — 예전 동작이다.
- **plugin 이 보는 것은 그대로다** — 환경변수 · 인증 한 줄 · 한도 · 그 뒤의 요청 순서. wire 도 그대로다.

## Consequences

- **얻은 것**: 5 s 뒤에 연결하는 plugin 의 `enable` 이 130 ms 로 끝났고 같은 구간의 `list info` 최댓값이
  164 ms(중앙값 129 ms)였다(전 5121 ms · 5098 ms). 호스트 로그의 `plugin connected … ms=5027` 이 연결이
  뒤에서 성사됐음을 보인다. 끝내 연결하지 않는 plugin 은 enable 189 ms · 14 s 구간 `list info` 최댓값
  170 ms(중앙값 135 ms)였고(전 10122 ms · 9954 ms), 10 s 에 `spawn failed … did not connect within 10s` 로
  기록된 뒤 50 ms 에 kill 로 회수됐다. 격리 헤드리스 debug 인스턴스, 이 결정이 착지한 코드의 빌드,
  2026-09-23. 머신에 다른 빌드가 돌던 때라 평시 중앙값도 올라 있다 — 견줄 것은 초 단위 정지가 사라졌다는
  것이다.
- **얻은 것**: 연결 실패로 끝난 자식이 남지 않는다. 예전에는 연결 한도를 넘긴 자식을 kill 하지 않고
  버렸다(`std::process::Child` 는 drop 에서 kill 하지 않는다 — 소스로 확인했고 옛 동작의 잔존은 관측하지 않았다).
- **얻은 것**: 전체 기동의 연결 대기가 plugin 수만큼 쌓이지 않는다(가장 느린 하나).
- **얻은 것**: 곧바로 연결하는 plugin 이 등록 전에 인증해 거절되지 않는다. 위 병렬 부하에서 이 결정의
  빌드는 36 번 중 0 번 깨졌고, 계수를 넣어 잰 48 번의 기동에서 거절 0 · 인증 48 이었다. 등록만 자식을 띄운
  뒤로 되돌린 변이는 같은 부하에서 48 번 중 17 번 거절됐다.
- **잃은 것**: 연결 전의 plugin 도 `plugin list` 에 `running: true` 로 보인다. 끝내 연결하지 않으면 최대
  10 s 뒤에 `false` 가 된다 — 예전에는 그 10 s 동안 메인 스레드가 서 있어 아무도 그 틈을 못 봤다.
- **잃은 것**: `enable` · swap · 재기동이 돌아온 순간은 "연결했다" 를 뜻하지 않는다. 연결 실패는 그 호출의
  응답이 아니라 뒤의 `plugin.error` 와 로그로 드러난다. swap(`upgrade-builtins --restart-running` ·
  auto-reload)의 "respawn failed" 는 이제 자식을 못 띄운 경우만 뜻한다.
- **잃은 것**: 연결을 기다리는 사이의 요청은 plugin 이 끝내 연결하지 않으면 한도(10 s)까지 답을 못 받는다.
  예전에는 그 요청이 오기 전에 기동이 이미 실패해 있었다.
- **유지한 것**: 헤드리스 `ensure_plugin_manager`(attach mesh mirror 세션)의 전체 기동은 메인 스레드에서
  연결 결과까지 기다린다. 뒤에 짧은 hello 시한이 없어 얻는 것은 "돌아온 뒤엔 연결돼 있다" 는 옛 보장
  하나이고, 그 보장을 깨지 않으려고 둔다 — 정지는 예전의 합(Σ)에서 가장 느린 하나로 줄었다.
- **운영 비용**: 기동마다 연결까지 사는 스레드 하나(최대 10 s).
- **운영 비용**: 연결 중인 plugin 에 보낸 요청은 연결 결과가 날 때까지 만료되지 않는다. 끝내 연결하지 않으면
  그 요청은 시한이 아니라 연결 실패(최대 10 s)에서 끝난다 — 짧은 시한의 hook 도 그때까지 기다린다. 예전에는
  그 10 s 를 기동이 메인 스레드에서 막혀 보냈다.

## Alternatives Considered

- **A: 연결 전에는 `processes` 에 넣지 않고, 그 사이의 namespace 호출은 `-32002 not running` 으로
  거절한다** — ADR-0457 의 회수 중 거절과 대칭이라 모양은 깔끔하다. 안 고른 이유는 호환이다. 예전에는
  `enable` 이 돌아오면 plugin 이 연결돼 있었고 "enable → 곧바로 호출" 이 성공했다. 거절하면 그 호출이
  성공 응답 뒤에 실패를 본다 — ADR-0457 이 명시적 `enable` 을 미루지 않은 이유(대안 F)와 같다. namespace
  호출이 owner 를 띄우는 경로는 더 나쁘다 — 띄운 바로 그 호출이 거절된다.
- **B: 명시적 `enable` 만 그대로 기다리고 나머지를 비동기로** — 재어진 정지(5 · 10 s)가 바로 그 `enable`
  이다. 요청을 쌓으면 기다리지 않고도 호환이 지켜지므로 남길 이유가 없다.
- **C: 전체 기동도 기다리지 않는다** — 부팅의 hello 시한(300 ms)이 연결 시간에 먹혀 레이아웃 복원이
  plugin kind 를 놓칠 수 있다. 그 자리는 GUI 에서 부팅 워커라 메인 스레드를 세우지 않는다.
- **D: 한도(10 s)를 줄인다** — 정지가 줄 뿐 없어지지 않고, 무거운 초기화를 하는 정상 plugin 의 기동이
  실패로 바뀐다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 기동이 다시 연결을 기다리면 `manager::tests_connect::a_start_returns_before_the_plugin_connects_and_queued_requests_reach_it`
  가 잡는다 — 연결을 시험이 파일로 쥐고 있어 시계와 무관하다. 연결 대기를 그 자리에서 하게 하는 변이로
  확인했다. 같은 시험이 연결 전에 보낸 요청이 연결 뒤 첫 줄로 닿는지도 단정한다.
- 연결을 받을 자리를 자식을 띄운 뒤에 열면 `the_connection_is_registered_before_the_child_starts` 가
  잡는다 — 시험 전용 hook(`process::AFTER_CHILD_SPAWN_DELAY`)이 자식을 띄운 직후 호출 스레드를 1 s 세워
  부하의 밀림을 결정적으로 만든다. 등록을 그 뒤로 옮기는 변이로 확인했다. 앞의 시험들은 이 회귀를 못
  잡는다 — 연결 전에도 `running` 이 참이라 거절돼도 통과한다(변이에서 거절 17 건에도 초록이었다).
- 연속 실패 기록을 자식을 띄운 때 지우면
  `a_plugin_that_never_connects_is_a_spawn_failure_and_repeats_auto_disable_it` 가, 연결 성사 때 안 지우면
  `a_connection_clears_the_failure_record` 가, pump 가 결과를 안 거두면 앞의 둘이, 전체 기동이 결과를
  안 기다리면 `discover_and_start_settles_connections_before_it_returns` 가 잡는다(넷 다 변이로 확인했다).
- 헤드리스의 전체 기동(`ensure_plugin_manager`, attach mesh mirror 세션)은 메인 스레드에서 돌고, 이 결정
  뒤에도 가장 느린 연결 하나만큼 선다. 그 경로가 주기적으로 오게 되면 다시 연다.
- 시한을 보낸 시각부터 다시 세면 `manager::connect::tests` 의
  `a_pre_hook_sent_before_the_extension_connects_is_timed_from_the_connection` 과
  `a_namespace_call_sent_before_the_owner_connects_is_timed_from_the_connection` 이, 연결 실패한 extension 의
  hook 을 건너뛰지 않으면 `a_pre_hook_to_an_extension_that_never_connects_is_bypassed` 가, 연결 성사를 안
  표시하면 `tests_connect` 의 첫 시험이, 헤드리스 kind 대기가 연결 시간을 등록 시한에 넣으면
  `boot::headless_plugins` 의 `a_kind_is_registered_when_its_owner_connects_after_the_registration_wait` 가
  잡는다(변이 넷으로 확인했다 — 시한 재기준을 빼는 변이 하나가 앞의 두 시험을 함께 깨웠다). 건너뛴 pre-ipc hook 뒤 target 에
  호출 plugin id 를 안 실으면 `a_bypassed_pre_hook_carries_the_calling_plugin_to_the_target` 가 잡는다(그 id 를
  `None` 으로 바꾸는 변이로 확인했다).

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 메인 스레드가 정말 안 서는가. 재는 법: 격리 `TASTY_HOME` 에 연결 전에 N 초 잠드는 probe plugin 을 두고
  `plugin enable` 하는 동안 `tasty list info` 왕복을 연속으로 재서 최댓값을 평시와 견준다. GUI 의 무응답
  재시작 경로는 같은 probe 를 멈춰 75 초 둔다.
- 연결 전 `running: true` 가 사용자에게 문제가 되는가. 재는 법: 연결 실패로 끝난 기동에서 `running: true`
  를 보고 호출한 요청이 몇 번 오류로 끝났는지 호스트 로그의 `plugin did not connect` 취소 줄로 센다.

## References

- 관련 ADR: [ADR-0457](0457-a-single-plugin-shutdown-is-reaped-off-the-main-thread.md) — 종료 대기를 메인 스레드 밖으로 뺀 결정. 연결 실패로 내리는 자식의 회수가 그 회수 스레드를 쓴다
- 관련 문서: [`plugin-development.md`](../dev-guide/plugin-development.md) §7 "생명주기" · "토큰 핸드셰이크" — 이 결정의 현재 운영 상태
- **코드 근거 (결정이 실현된 현재 위치)**: `tasty-host-plugin` 의 `process::connect` 모듈(`start` · `ConnectSlot`) ·
  `listener` 모듈의 `HostListener::register` · `PendingConnection`, 그리고 `manager::connect` 모듈의
  `PluginManager::settle_connections` · `wait_for_connections` · `deadline_from_connection` ·
  `bypass_hooks_sent_to` 와 `PluginProcess::abandon`, 헤드리스 `boot::headless_plugins` 의
  `wait_for_kind_registration`.
