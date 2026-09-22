# ADR-0274: 하네스 락은 선언 위치가 아니라 획득 형태를 검사한다

- **Status**: Accepted
- **Date**: 2026-09-15
- **Tags**: tests, mutex, poison, acquisition, scope
- **Group**: testing

## Context

공유 락을 쥔 채 테스트 단정이 패닉하면 뒤 테스트가 poison으로 실패하거나 stderr
수집기가 멈춘다. stderr 포착은 `tests/spawn_diag/mod.rs`의 `StderrCapture`로
모였지만 다른 목적의 공유 락도 존재한다. `tests/shared_instance_harness.rs`의
`OBSERVED`, `tests/gui_common/mod.rs`의 `SHARED_INSTANCE`,
`tests/attach_common/mod.rs`의 `WRITE_LOCK`은 stderr 포착이 아니다.

`Mutex` 선언을 `spawn_diag` 안으로 제한하면 이 정당한 용도를 면제해야 한다.
선언 위치만으로는 락을 쥔 구간에서 패닉하는지 알 수 없다.

## Decision

하네스 락의 선언 위치를 제한하는 추가 가드는 두지 않는다. 기존
`test_harness_lock_unwrap_ratchet`의 획득 형태 검사와 자리별 사유를 유지한다.
`MIN_ACQUISITIONS`는 검사 모집단의 하한 생존 검사이며, 정확한 획득 수의 양방향
래칫이 아니다. 획득 수가 늘어나는 것은 허용하고 비면제 생 `.unwrap()`은 별도로
거부한다. 면제 예산과 죽은 면제 검사는 획득 수 검사와 다른 조건이다.

## Consequences

- **얻은 것**: 정당한 공유 락을 위치 때문에 옮기거나 면제하는 부담이 없다.
- **잃은 것**: 새 하네스의 독자적인 락 선언 자체는 금지되지 않는다. 하한 이상의
  모집단 누락이나 검사기가 인식하지 않는 획득 형태는 하한으로 검출할 수 없다.
- **운영 비용 / 유지 부담**: 락의 보호 대상과 패닉 가능 구간은 코드 리뷰에서 확인한다.
  검사 뿌리는 루트 `tests/`이고 크레이트 내부 하네스는 이 검사의 대상이 아니다.

## Alternatives Considered

- **선언 위치 가드**: 서로 다른 목적의 락까지 면제 목록으로 관리하게 되어 채택하지 않는다.
- **획득 수를 정확한 값으로 고정**: 위험한 획득과 무해한 정리를 구별하지 못하며,
  기존 하한의 목적을 바꾸므로 채택하지 않는다.

## Reconsideration Triggers

**채널이 붙는 것**

- `spawn_diag` 밖의 하네스 Mutex 선언이 한 자리 이하로 줄면 위치 제한의 면제 비용을
  다시 검토한다. 자동 판정은 추가하지 않는다. 재는 법: 루트 `tests/`의 Rust 소스를
  마스킹한 뒤 `Mutex<` 선언을 찾아 `spawn_diag` 안팎으로 나누고 선언 심볼을 확인한다.
  현재 획득 검사와 그 하한은 선언 위치나 이 조건을 판정하지 않는다.

**원리적으로 안 붙는 것**

- 선언 위치 자체가 획득 형태와 무관하게 결함을 만든 사례가 확인되면 재검토한다.
  재는 법: 실패를 합성 락/하네스로 재현하고, 획득 동작을 유지한 채 선언 위치만
  바꾼 대조에서 실패가 사라지는지 확인한다.

## References

- 현재 검사: `crates/tasty-doc-guards/tests/test_harness_lock_unwrap_ratchet.rs`
- 현재 공유 포착: `tests/spawn_diag/mod.rs`의 `StderrCapture`
- [락 poison 처리 규약](../dev-guide/error-handling.md)
- [유닛 테스트 격리](../dev-guide/unit-test-isolation.md)
