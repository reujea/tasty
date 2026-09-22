# ADR-0519: 출력 observer 의 memory 레코드 키에 sink 순번을 붙인다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: output-observer, memory, storage, key, ring-buffer, data-loss

## Context

`output.observe_start` 의 memory sink(`src/core/output_observer.rs` 의 `run_memory_sink`)는 파싱된
항목을 `global` 스코프 `tasty.observer.<id>.<ms>` 키로 쓰고, `max_records` 를 넘으면 자기가 쓴 키를
기억한 순서대로 가장 오래된 것부터 지웠다. 같은 밀리초에 온 항목은 한 키로 겹쳐 덮어쓰이는데,
기억에는 그 키가 put 마다 들어갔다. 넘칠 때 옛 칸을 지우는 삭제가 같은 이름의 **최신** 레코드를
지웠다. 실측: `--max-records 2` 에 url 여섯 항목 → `total_out` 6 · 남은 키 0(격리 인스턴스), 단위
재현에서도 남은 레코드 0.

## Decision

키를 `tasty.observer.<id>.<ms>.<seq>` 로 바꾼다. `<seq>` 는 sink 가 쓴 순번(0 부터, `{:06}`)이다.
키가 sink 안에서 유일해지므로 같은 ms 항목도 각자 남고, 상한은 가장 최근 N 건을 남긴다. `<ms>` 를
앞에 둬 키 오름차순이 도착 순서가 되고, `tasty.observer.<id>.` 접두사로 읽는 소비자는 그대로 읽는다.

## Consequences

- **얻은 것**: 상한이 약속대로 최근 N 건을 남기고, 같은 ms 항목이 덮어쓰이지 않는다(상한이 없을
  때도 손실이 사라진다).
- **잃은 것**: 키의 마지막 마디를 밀리초로 읽던 소비자는 이제 순번을 읽는다. 시각은 레코드 값의
  `at_ms` 에 그대로 있다.
- **운영 비용 / 유지 부담**: 없음. 순번은 sink 스레드의 지역 변수다.

## Alternatives Considered

- **키 형식은 두고 기억(`written_keys`)에서 중복 키를 합친다** — 키 형식이 불변이라 호환은 더
  보존된다. 안 골랐다: 같은 ms 항목은 여전히 한 건만 남아 "같은 밀리초에 온 항목을 포함한 최근
  N 건" 을 못 지킨다. 손실의 한 갈래(살아 있는 레코드 삭제)만 닫고 다른 갈래(덮어쓰기)는 남긴다.
- **순번만 쓰고 밀리초를 뺀다** — 키가 짧아진다. 안 골랐다: 재시작 뒤 같은 observer id 가 다시
  쓰이면 순번이 0 부터라 옛 실행의 키와 겹친다. 밀리초가 실행을 가른다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 한 sink 가 같은 ms 에 10^6 건 이상을 쓰는 경로가 생긴다 — `{:06}` 의 자릿수가 넘쳐 같은 ms
  안의 키 오름차순이 도착 순서와 갈린다. 재는 법: sink 의 채널 용량(`bounded channel`)과 put
  처리율을 본다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 시계가 뒤로 가 같은 observer id 의 옛 ms 로 돌아가는 환경에서 재시작 뒤 키가 겹친다는 보고.
  재는 법: 같은 키에 서로 다른 `at_ms` 가 덮인 레코드를 찾는다.

## References

- 설계 문서: [storage](../design/systems/storage.md) "터미널 출력 observer 의 memory sink — 저장 계약"
- 코드 근거(결정이 실현된 현재 위치): `src/core/output_observer.rs` 의 `run_memory_sink`, 시험
  `the_sink_keeps_the_latest_records_even_within_one_millisecond`.
