# ADR-0177: poison 복구가 금지되는 락은 이름이 아니라 프레임 경계 타입으로 판정한다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: poison, locks, error-handling, guards, measurement, false-negative, adr-0129, adr-0155
- **Group**: guard-design

## Context

`tasty-utils` 의 `poison` 모듈은 락 poison 을 **첫 1 회 보고하고 복구**하는 헬퍼
(`recover_mutex`/`recover_read`/`recover_write`)를 제공한다. 복구는 임계구역이 자료구조·값을
만질 뿐이라 락을 든 채 죽은 스레드가 불변식을 깨지 않을 때만 옳다. 반대로 임계구역이
**소켓/스트림에 프레임(한 줄=한 메시지)을 쓰는** 자리는, 반쪽 프레임을 남긴 채 죽을 수 있고
그 위에 `into_inner` 로 이어 쓰면 프레이밍이 깨져 상대가 쓰레기를 읽는다. 그런 락은 복구가
오답이고 skip(+보고) 또는 전파가 맞다.

이 "복구 금지" 를 소스 스캔 가드(`forbidden_lock_guard`)가 지킨다. 그 명부(`FORBIDDEN_LOCKS`)가
**원소 하나(`writer`)의 이름 목록**이었다. 이름 목록은 조용히 낡는다 — 같은 성질의 둘째 락
(`handle_writer`, `Mutex<HandleClient>`)이 이미 트리에 있었는데 이름이 달라 명부 밖이었고,
셋째(다음에 생길 스트림 락)는 어떤 이름으로 들어와도 안 걸린다. 이름이 같다는 것이 같은
부류라는 뜻이 아니고, 이름이 다르다는 것이 다른 부류라는 뜻도 아니다.

## Decision

명부의 근거를 **이름이 아니라 피보호 타입**으로 못박는다. 술어는 한 문장이다 —
**"임계구역이 프레임·레코드 경계를 갖는가."** 그 술어에 걸리는 타입은
`FORBIDDEN_STREAM_TYPES = [TcpStream, HandleStream, HandleClient]` 다(프레임 I/O 스트림).
`fs::File`(줄 단위 로그, 반쪽 줄 허용)·자료구조·값 슬롯은 경계가 없어 제외한다 — 그쪽은
복구가 옳고, "조용히 삼키지 말라" 는 별도 축이 지킨다.

매칭은 apparatus 재사용을 위해 이름(`FORBIDDEN_LOCKS`)으로 하되, 완전성은 타입이 진다:
`stream_typed_lock_names_are_all_listed` 가 선언(`이름: …Mutex<스트림타입>…`)을 전수해, 그
타입을 감싼 락의 바인딩 이름이 명부에 없으면 실패시킨다. 그래서 새 스트림 락이 어떤
이름으로 들어와도 가드-테스트 시점에 잡혀 명부에 편입된다.

## Consequences

- **얻은 것**: 명부가 "이름 둘짜리" 로 낡지 않는다. 프레임 경계 타입의 새 락은 이름과
  무관하게 완전성 테스트가 잡는다. 명부의 근거(술어)가 코드에 박혀, 다음 사람이 정당하게
  줄이거나 늘릴 수 있다.
- **잃은 것**: 완전성은 **선언에 바인딩 이름이 있는** 자리만 본다 — 익명 자리(enum variant
  `Ready(Arc<Mutex<HandleStream>>)` 등)는 이름이 없어 세지 않고, 그 자리는 사용처(`writer`
  이름 또는 skip-report)로 덮인다. 텍스트 스캔이라 타입 별칭(`type W = Mutex<TcpStream>`)은
  못 따라간다.
- **운영 비용**: 새 프레임 스트림 타입을 도입하면 `FORBIDDEN_STREAM_TYPES` 에 한 줄 더한다.

## Alternatives Considered

- **이름만 계속 쓰기(둘째 이름을 추가하고 끝)**: 셋째가 안 걸린다 — 기각.
- **매칭 자체를 타입으로 재작성(선언→사용처 타입 추론)**: `.lock()` 호출 자리엔 타입이 없어
  선언과 사용을 파일 넘어 잇는 추론이 필요하다. apparatus 를 통째로 갈아야 하고,
  이름 매칭 + 타입 완전성 테스트 조합이 같은 보증을 더 싸게 준다 — 기각.

## Reconsideration Triggers

- 타입 별칭이나 제네릭으로 감싼 프레임 스트림 락이 생겨 완전성 테스트의 텍스트 스캔이
  놓치는 사례가 관측되면.
- 익명 선언(enum variant 등)에서 프레임 스트림 락이 사용처 이름으로도 안 덮이는 자리가
  생기면.

## References

- 관련 ADR: [ADR-0129](0129-flaky-test-classes-and-standard-fixes.md) (flaky 테스트 부류와 표준 처방) · [ADR-0155](0155-global-state-race-prescription-by-parameterization.md) (전역 상태 경합 처방을 성질에 건다)
- 구현: `crates/tasty-utils/src/poison.rs` (`forbidden_lock_guard` — `FORBIDDEN_LOCKS`·`FORBIDDEN_STREAM_TYPES`·`stream_typed_lock_names_are_all_listed`)
- 방침: `docs/dev-guide/error-handling.md` "락 poison" 절
