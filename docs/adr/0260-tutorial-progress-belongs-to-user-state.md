# ADR-0260: 튜토리얼 진행은 사용자 상태 DB에 주제별로 저장한다

- **Status**: Accepted
- **Date**: 2026-09-09
- **Tags**: tutorial, persistence, concurrency
- **Group**: storage

## Context

튜토리얼은 호스트 GUI의 사용자 안내다. 완료·중단 위치는 앱 재시작과 여러 View에서 의미를 유지해야 하지만, 워크스페이스의 runtime ID나 레이아웃 복원 설정에 종속되면 안 된다. 기존 state.db는 사용자 사용 기록용 저장소이며, schema version 불일치를 자동 업그레이드하지 않는다.

## Decision

진행은 state.db의 별도 tutorial_progress 테이블에 주제 ID·콘텐츠 revision·완료·재개 단계·row version으로 저장한다. 테이블은 현재 스키마 버전 검증을 통과한 연결에 idempotent하게 추가하며, 버전 불일치 보호를 유지한다. 완료는 단조 갱신하고 재개 위치는 낙관적 버전 검사로 오래된 View의 쓰기를 거부한다. runtime 객체 ID를 저장하지 않고 실습 재개는 준비부터 수행한다.

## Consequences

- **얻은 것**: 레이아웃 복원 설정과 무관한 완료 기록, 기존 DB 상태 보존, stale snapshot 덮어쓰기 방지.
- **잃은 것**: 실습 도중 앱을 재시작하면 같은 객체에서 이어지는 경험은 제공하지 않는다.
- **운영 비용 / 유지 부담**: 단계 의미를 바꾸면 콘텐츠 revision 정책을 적용해야 한다. 저장 오류와 세션 진행은 구분해 표시한다.

## Alternatives Considered

- **config.toml**: 사용자가 편집하는 선호와 학습 이력이 섞이고 전체 설정 snapshot 갱신이 필요하다.
- **layout slot**: restore_layout 설정과 윈도우 슬롯 수명에 잘못 종속된다.
- **새 JSON 파일**: 기존 DB가 제공하는 잠금·트랜잭션을 다시 구현해야 한다.
- **state.db 버전 증가만 수행**: 현재 정책에서는 기존 DB를 mismatch로 거부하므로 추가 테이블 목적에 맞지 않는다.

## Reconsideration Triggers

- 외부 플러그인이 튜토리얼 콘텐츠를 기여하면 ID namespace와 revision 소유권을 재검토한다.
- DB가 정식 순차 migration을 도입하면 additive ensure를 그 경로로 수렴시킨다.
- 실습을 세션 복원 객체와 연결해야 한다는 사용자 요구가 생기면 persist ID 기반 재개를 실제 복원 실패 시나리오와 함께 평가한다.

## References

- [튜토리얼 동작](../features/tutorial/index.md)
- [사용자와 에이전트 분리](../identity.md)
- `src/store/tutorial_progress.rs`
- `src/db/migrations.rs`
