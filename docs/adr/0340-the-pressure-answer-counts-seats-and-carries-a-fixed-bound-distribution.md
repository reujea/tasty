# ADR-0340: 압력 응답은 자리를 따로 세고 시간에는 고정 경계 분포를 단다 — ADR-0305 의 histogram 유보 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: telemetry, ipc, pressure, histogram, saturation, connections, observability, adr-0305, adr-0333
- **Group**: ipc-transport

## Context

[ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md) 가
요청 압력을 프로세스 게이지로 세우고
[ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md)
이 그것을 `system.pressure` 로 열었다. 그 응답이 답하지 못하는 물음이 둘 남아 있었고,
둘 다 같은 형태다 — **두 개의 다른 상태가 같은 수로 보인다.**

1. **자리가 없는 것이 느린 것으로 보인다.** 서버는 동시 IPC 연결을
   `MAX_CONCURRENT_CONNECTIONS`(256)으로 자르는데, 그 계수는 accept 스레드의 지역
   원자값이었다. 상한에 닿았다는 사실이 밖으로 나가는 통로는 포화로 들어가는 순간의
   `warn` 한 줄뿐이고, 그 줄이 지나가면 "지금 몇 개가 붙어 있나" 를 물어볼 자리가 없다.
   응답의 네 덩어리는 전부 시간이라 포화에 **아무것도 안 움직인다** — 거절된 연결은
   요청이 된 적이 없기 때문이다. 부르는 쪽에서는 느린 것과 자리가 없는 것이 똑같이
   "답이 안 온다" 로 보인다.

2. **꼬리가 있는 것이 전체가 느린 것으로 보인다.** 시간 덩어리 셋은 count·sum·max 를
   든다. ADR-0305 는 분위수를 **일부러 유보했다** — "버킷 경계는 실제 분포를 보고 정할
   일이라 지금 지어내지 않았다", 그리고 대안 "histogram 버킷을 지금 정한다" 를 "경계를
   분포 없이 고르면 그 경계가 곧 분포를 말하는 것처럼 읽힌다" 로 기각했다. 그 유보의
   전제(분포를 모른다)가 이제 성립하지 않는다: ADR-0333 이 격리 인스턴스에서 동시 60 건을
   걸어 큐 대기 max 25.4 ms · handler max 0.48 ms · 관측된 큐 깊이 18 을 쟀다. 그리고
   유보가 남긴 구멍은 실재한다 — 네 건 중 한 건이 1 s 이고 셋이 0 인 분포와, 넷이 전부
   250 ms 인 분포는 **count 도 sum 도 평균도 같다.** 처방은 반대다(뒤는 용량, 앞은 그 한
   건의 원인).

## Decision

압력 응답에 **자원 축 덩어리 하나**를 더하고, **시간 축 세 덩어리에 고정 경계 분포**를
단다.

**자원 축** — `connections` 덩어리가 `live` · `live_max` · `limit` · `accepted` ·
`refused_saturated` 를 싣는다. 계수는 `tasty_telemetry::ConnectionStats` 가 들고, 그
핸들을 **`Core` 가 낳아 부팅이 IPC 서버에 건넨다**(`db` 게이지와 방향이 반대인 이유는
서버가 `Core` 보다 뒤에 뜨기 때문이다). 세는 모수는 이 포트에 붙은 **TCP 연결 전부**이고
요청을 하나도 안 보내는 attach·mesh 스트림도 센다 — 자리를 먹는 것이 요청이 아니라
연결이다. `limit` 은 게이지가 아니라 서버가 집행하는 상수라 노출부가 함께 싣는다. 거절
로그는 포화 진입 순간만 `warn` 이고 그 뒤 `debug` 로 접히지만 **접히는 것은 로그뿐이고
`refused_saturated` 는 전부 센다.**

**포화 거절은 `ipc_calls` 에 안 싣고 [ADR-0277](0277-ipc-admission-and-observation-run-once.md)
을 개정하지 않는다.** 그 결정이 "요청마다 한 번" 으로 묶은 것은 게이트에 들어온 요청의
관측이고, accept 에서 거절된 연결은 **JSON-RPC 요청이 된 적이 없다.** 거기 실으면 TCP
사건이 요청 모수에 섞이고, 그 카운터가 cap 평가와 anomaly 검출의 입력이라 관측을
고치려다 집행이 바뀐다(ADR-0305 가 같은 이유로 거부를 안 실었다).

**시간 축** — `queue_before_gate.wait_us_hist` · `handler_after_gate.us_hist` ·
`plugin_round_trip.us_hist` 가 `bounds_us` 와 `counts` 로 나간다. 경계는 10 µs 부터 1 s
까지 **반-십진(√10 ≈ 3.16 배) 열한 칸 + 넘침 한 칸**이다. `counts` 는 `bounds_us` 보다 한
칸 길고 **누적이 아니다** — 칸끼리 겹치지 않아 합이 관측 수다(Prometheus 의 `le` 누적
버킷과 다르다). `bounds_us` 는 덩어리마다 되풀이한다: 값과 경계가 같은 자리에 있어야
응답을 그대로 덤프해도 칸의 뜻이 읽히고, 떼어 놓으면 소비자가 경계를 자기 쪽에 복제해
그 복제본이 갈린다. **분위수는 호스트가 계산하지 않는다** — 버킷 해상도 안에서만 답할 수
있는 값이라, 호스트가 한 수로 내놓으면 갖고 있지도 않은 정밀도를 말하게 된다.

`db` 와 `connections` 에는 분포를 안 단다. 앞은 게이지가 `tasty-memory` 에 살아 이
histogram 타입을 못 보고(의존이 `tasty-telemetry` → `tasty-memory` 방향이다), 뒤는 시간이
아니라 자리라 분포를 잴 축이 아니다.

**ADR-0305 에서 개정하는 것은 "분위수는 답하지 못한다" 한 조항뿐이다.** 개정하지 않는
것을 이름으로 적는다 — 프로세스 축(caller 로 나누지 않는다) · 고정 크기(호출 수와 무관) ·
저장소를 안 거친다 · 게이트를 기준으로 재는 자리가 갈린다(큐는 앞, handler 는 뒤) ·
`ipc_calls` 를 거부까지 넓히지 않는다 · `Clock` port 를 안 쓴다. 전부 그대로다.

## Consequences

- **얻은 것**: 운영자가 "느리다" 와 "자리가 없다" 를 한 응답에서 가른다. `live` 가
  `limit` 에 붙어 있거나 `refused_saturated` 가 오르는 중이면 시간 덩어리가 전부 작아도
  포화다 — 그 상태가 전에는 밖에서 안 보였다.
- **얻은 것**: 같은 평균·같은 최대를 내는 두 분포가 갈린다. 꼬리 몇 건과 전반적 적체는
  처방이 반대라, 그 구분이 없으면 진단이 답을 고르지 못한다.
- **잃은 것**: 분위수는 여전히 정확히 안 나온다. 버킷 안에서는 균등 가정 말고는 근거가
  없으므로, 소비자가 칸으로 추정하면 그 추정의 오차는 버킷 폭(3.16 배)이다.
- **잃은 것**: `bounds_us` 가 응답에 세 번 실린다. 경계가 열한 개라 실질 비용은 없지만
  응답이 그만큼 길어진다.
- **잃은 것**: `db` 덩어리만 분포가 없어 응답이 **비대칭**이다. 대칭으로 만들려면
  histogram 타입이 두 크레이트 아래의 공용 자리로 내려가야 하는데, 그 자리는 번들 plugin
  의 의존 폐포 안이라 계측 하나가 plugin 아홉의 버전 bump 를 끌고 온다(ADR-0333 이 같은
  이유로 집계 타입 중복을 받아들였다).
- **운영 비용**: 게이지가 원자값으로 `ConnectionStats` 넷 + histogram 세 개 × 열두 칸이다.
  관측 수와 무관하게 고정이고, 기록 한 번은 상한 열한 개의 선형 탐색 + `fetch_add` 하나다.

## Alternatives Considered

- **A: 연결 수를 `ipc_calls` 나 기존 시간 덩어리에 실는다** — 모수가 섞인다. 연결은
  요청이 아니고, 거절된 연결은 요청이 된 적조차 없다. 위 Decision 의 ADR-0277 문단이
  같은 논증이다.
- **B: `limit` 을 게이지 안에 둔다** — 응답이 자족적이 되지만 상한이 두 곳에 살게 된다.
  집행하는 값은 서버의 상수 하나여야 하고, 게이지가 그 사본을 들면 서버가 안 뜬 조립에서
  `limit: 0` 이 나가 "자리가 없다" 로 읽힌다.
- **C: `live` 만 싣고 누계 셋을 뺀다** — 순간값만 있으면 "지금은 비었지만 아까 꽉
  찼었다" 를 못 본다. 진단을 부르는 시점은 대개 문제가 지나간 뒤다.
- **D: 분위수(p50/p99)를 호스트가 계산해 낸다** — 읽기는 편하지만 버킷 해상도 안의
  추정을 정확한 수처럼 내놓는다. 넘침 칸에 든 관측은 상한이 없어 p99 를 아예 못 낸다 —
  그때 `null` 을 돌려주면 "관측이 없다" 와 구별되지 않는다.
- **E: 누적(`le`) 버킷으로 낸다** — Prometheus 관행과 맞지만, 칸의 합이 관측 수가
  아니게 되어 응답을 그대로 읽는 사람이 총계를 두 번 센다. 이 응답의 소비자는 스크래퍼가
  아니라 `tasty list pressure` 를 읽는 사람이다.
- **F: 버킷 경계를 설정값으로 연다** — 분포가 인스턴스마다 다르니 맞출 수 있지만, 경계가
  달라지면 두 인스턴스의 응답을 나란히 못 읽는다. 진단값의 값은 비교 가능성에 있다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 자리 계수가 **밖에서 읽는 게이지**로 가는가는
  `tcp_ipc_server::admission_tests::the_seat_count_lands_in_the_gauge_the_diagnostic_reads`
  가 잡는다. accept 루프가 지역 원자값으로 되돌아가면 게이지가 안 움직여 죽는다.
- 노출부가 **`Core` 의 그 게이지**를 읽는가는
  `pressure::tests::the_connection_block_reads_the_gauge_the_core_hands_to_the_server`
  가 잡는다. 핸들러가 새 기본값을 만들어 읽으면 0 이 와서 죽는다.
- 로그 접힘이 **계수를 접지 않는가**는
  `tcp_ipc_server::admission_tests::a_folded_log_line_does_not_fold_the_refusal_count`
  가 잡는다.
- 칸이 누적이 아닌가 · 경계가 `le` 인가 · 넘침 칸이 있는가는
  `tasty_telemetry::pressure` 의 `the_buckets_do_not_overlap` ·
  `an_observation_equal_to_a_bound_lands_in_that_bucket` ·
  `everything_past_the_last_bound_lands_in_the_overflow_bucket` 이 잡는다.
- 분포가 **자기 덩어리 안에 경계와 함께** 나가는가는
  `pressure::tests::each_distribution_ships_inside_its_own_block_with_its_bounds` 가
  잡는다. 경계를 응답 한 곳으로 빼거나 분포를 다른 덩어리로 옮기면 죽는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **부팅이 실제로 게이지를 서버에 건네는가.** 그 배선은 `Hub::start_ipc` 의 인자 하나이고,
  그것을 재려면 창과 accept 스레드를 띄워야 한다(ADR-0333 이 `plugin_round_trip` 의 주입
  자리에 대해 적은 것과 같은 구멍이다). 재는 법: 격리 `TASTY_HOME` 으로 인스턴스를 띄우고
  `tasty list pressure` 를 두 번 불러 `accepted` 가 오르는지 본다 — 안 오르면 배선이
  끊긴 것이다(CLI 호출 자체가 연결이므로 `accepted` 는 부를 때마다 오른다).
- **경계가 실제 분포에 맞는가.** 첫 칸이나 넘침 칸에 관측이 몰리면 그 범위 안에서는
  분포를 못 읽는다. 재는 법: 실사용 인스턴스에서 `tasty list pressure` 의 세 `counts` 를
  읽어, 관측의 대부분이 양 끝 칸에 있으면 경계를 그쪽으로 옮길 때다.
- **256 이 정상 인구의 상한인가.** `refused_saturated` 가 정상 사용에서 오르면 그것이
  보정 신호다(그 값이 이 물음에 답하려고 존재한다). 재는 법: 그 값이 0 이 아닌 보고가
  오면 그때 `live_max` 와 함께 읽는다.

## References

- 개정 대상: [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)
  (분위수 유보 조항 — "분위수는 답하지 못한다" 와 대안 "histogram 버킷을 지금 정한다")
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md) —
  노출 표면과 "덩어리는 모수마다 하나" 규칙. 그 ADR 의 재검토 조건 "연결 수 게이지가
  들어올 때 이 스키마가 맞는가" 에 이 ADR 이 답한다(맞았다 — 모수 경계로 읽히는 이름
  하나를 더했다).
- [ADR-0277](0277-ipc-admission-and-observation-run-once.md) —
  개정하지 **않는다.** 위 Decision 의 해당 문단이 근거다.
- [telemetry](../features/telemetry/index.md) — "요청 압력 게이지 (프로세스 축)" 절이
  현재 운영 상태를 기술한다.
- [reference/api](../reference/api.md) — `system.pressure` 의 표면.
- 코드 근거(결정이 실현된 **현재** 위치): `crates/tasty-telemetry/src/pressure.rs` 의
  `ConnectionStats`·`LatencyHistogram` · `src/adapters/production/tcp_ipc_server.rs` 의
  `ConnectionSlot` · `src/adapters/ipc/handler/pressure.rs` 의 `snapshot_json`·`hist_json`.
