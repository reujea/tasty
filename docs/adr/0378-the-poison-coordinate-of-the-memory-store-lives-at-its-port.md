# ADR-0378: memory store 락의 poison 보고 좌표는 store 의 port 에 둔다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: memory, storage, poison, boundary, output-observer
- **Group**: storage

## Context

`memory.db` 의 store 는 호스트가 하나 열어 `Arc<Mutex<dyn MemoryStorage>>` 로 여러 모듈에
나눠 준다. 락이 하나이므로 poison 복구의 "첫 1 회만 보고" 플래그도 하나여야 하고, 그 좌표
(`MEMORY_WHAT` · `MEMORY_POISONED`)는 본체 `src/core/mod.rs` 에 있었다.

터미널 출력 observer 의 memory sink(`src/core/output_observer.rs` 의 `run_memory_sink`)는
port 만 받아 쓰는 자리인데, poison 복구 때문에 `crate::core::MEMORY_WHAT` ·
`&crate::core::MEMORY_POISONED` 를 직접 참조했다. observer 를 도메인에서 떼어 낼 수 있는
leaf 로 보던 과거 서술은 이 한 결합 때문에 틀렸고, 도메인 경계를 옮기는 후속 작업(headless
core 라이브러리 추출)이 이 결합을 먼저 풀기를 기다렸다.

## Decision

좌표의 실체를 **store 의 port** 로 올린다 — `tasty_memory::STORE_LOCK_WHAT`(이름) ·
`tasty_memory::STORE_LOCK_POISONED`(첫-1 회 플래그), 정의는 `crates/tasty-memory/src/port.rs`.

- observer 의 memory sink 는 port 의 좌표를 바로 쓴다. `crate::core` 참조가 0 이 된다.
- 본체 `core` 의 `MEMORY_WHAT` · `MEMORY_POISONED` 는 **이름을 잇는 재수출**로 남긴다. 같은
  static 을 가리키므로 플래그는 프로세스에 하나다. 그 좌표를 쓰는 나머지 자리(도메인 · IPC
  핸들러 · 부팅)는 고치지 않는다.
- 복구 자체는 여전히 호출자가 `tasty_utils::poison::recover_mutex` 로 한다. `tasty-memory` 는
  좌표만 내고 그 헬퍼를 부르지 않는다 — 헬퍼의 소비자 목록(그 모듈의 머리말이 센다)이 바뀌지
  않고, 번들 plugin 전부의 의존 폐포인 `tasty-utils` 를 건드리지 않는다.
- 로그에 나가는 이름(`"memory store"`)은 그대로다.

## Consequences

- **얻은 것**: observer 가 port 와 `tasty-utils` 만 알면 된다. 도메인 추출이 이 파일을 도메인
  쪽 의존 없이 옮길 수 있다.
- **얻은 것**: 좌표가 락을 나눠 주는 쪽(port)에 있어, 새 소비자가 도메인을 안 보고도 같은
  플래그를 쓴다.
- **잃은 것**: 좌표에 이름이 둘 생긴다(port 의 이름과 `core` 의 재수출 이름). 두 이름이 같은
  static 이라는 것은 시험이 본다.
- **운영 비용**: 없다. 부팅 wiring 의 별도 좌표(`memory store (db latency handle)`)는 이 결정의
  대상이 아니고 그대로다.

## Alternatives Considered

- **A: 좌표를 복구된 guard 를 돌려주는 함수로 port 에 둔다(`lock_store(&mutex)`)** — 호출이
  한 줄로 준다. 안 골랐다: `tasty-memory` 가 poison 헬퍼의 새 소비자가 되어 그 헬퍼의 머리말이
  세는 소비자 수가 틀려지고, 그것을 고치려면 번들 plugin 9 개의 bump 를 부르는 `tasty-utils`
  편집이 필요하다.
- **B: sink 에 좌표를 인자로 주입한다** — 결합은 풀리지만 `register` 의 시그니처와 모든 호출부가
  바뀌고, 결국 호출부가 `core` 좌표를 넘기므로 좌표의 주인은 그대로 `core` 다.
- **C: sink 에 자기 좌표를 따로 둔다** — 같은 poison 이 두 번 보고되고 어느 쪽도 첫 1 회가 아니다.
- **D: 나머지 열두 자리도 port 이름으로 옮긴다** — 이번 단위의 범위를 넘는 기계적 이동이다.
  재수출이 같은 static 이라 동작 차이가 없다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 두 이름이 다른 static 이 되면 `the_memory_sink_shares_one_poison_coordinate_with_the_core` 가
  빨개진다.
- sink 가 put 실패에서 멈추면 `a_failed_put_drops_that_record_and_the_sink_keeps_going` 이 빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **memory store 가 둘 이상이 되면**(예: 워크스페이스별 store) 플래그 하나라는 전제가 깨진다.
  재는 법: 호스트 부팅이 `MemoryStore` 를 여는 자리 수를 센다.

## References

- 결정이 실현된 현재 위치: `crates/tasty-memory/src/port.rs` 의 `STORE_LOCK_WHAT` ·
  `STORE_LOCK_POISONED`, 재수출은 `src/core/mod.rs`, 소비자는 `src/core/output_observer.rs` 의
  `run_memory_sink`
- 저장 계약 정본: [storage](../design/systems/storage.md) "터미널 출력 observer 의 memory sink"
- poison 방침: [error-handling](../dev-guide/error-handling.md)
