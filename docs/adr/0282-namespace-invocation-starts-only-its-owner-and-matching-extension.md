# ADR-0282: Namespace 호출은 owner와 필요한 활성 IPC hook extension만 시작한다

- **Status**: Accepted
- **Date**: 2026-09-15
- **Tags**: ipc, plugins, lifecycle, headless
- **Group**: plugin-system

## Context

ADR-0259는 kind 요청을 소유 plugin 하나의 기동으로 좁혔지만 namespace 경로는 별건으로
남겼다. ADR-0271은 namespace 권한을 닫았으나 허용 요청은 여전히 모든 활성 plugin을
시작했다. 다른 plugin의 on_start가 저장된 서버를 복원하는 부수효과까지 생긴다.
GUI는 부팅 때 기동하므로 요청의 추가 기동과 부팅 정책을 구분해야 한다.

## Decision

공통 manager forward는 manifest registry로 owner를 해소하고 caller의 기존 권한을
확인한 다음 start_one_enabled로 그 owner만 준비한다. disabled/auto-disabled는 유지하고,
이미 running이면 재시작하지 않는다. 미등록 prefix는 기동하지 않는다.

활성 extension 중 실제 매칭되는 pre/post IPC hook만 준비한다. self-loop 및 backoff로
우회되는 hook은 추가 기동을 만들지 않는다. hook의 실패/timeout/변환 규칙은 그대로다.
소유자 기동 실패 시 extension을 시작하지 않고 기존 not-running 오류로 답한다.

전체 IPC 메서드 명부는 계약에 없고 commands는 팔레트/단축키 기여 목록이다. 따라서 등록
prefix 안의 오타까지 기동 0으로 만들려고 이를 allowlist로 사용하지 않는다. owner는 하나
시작될 수 있고 오류는 plugin이 답한다. 다른 owner는 시작하지 않는다.

ADR-0277의 permission/cap/rate/관측 순서는 바꾸지 않는다. Local/Agent의 기동 범위를
가르지 않는다. GUI 부팅과 attach mirror의 기존 트리거는 별개로 유지한다. 이 결정은
ADR-0259/0271에서 남긴 namespace 기동 범위를 완결하며 그 문서의 당시 실측은 기록으로 보존한다.

## Consequences

- **얻은 것**: 허용 호출·실패 호출이 무관한 plugin의 수명주기를 깨우지 않는다.
- **잃은 것**: 다른 namespace 호출에 기대던 SSE·hook 복원은 더 이상 우연히 실행되지 않는다.
- **운영 비용 / 유지 부담**: 필요한 plugin은 명시적으로 사용하거나 Local enable로 시작해야 한다.
  extension 준비는 현재 manifest/config의 활성 선택을 사용하며 전체 plugin 시작으로 대체하지 않는다.

## Alternatives Considered

- 전량 기동 유지 또는 Local만 전량 기동 — 무관한 수명주기 효과와 caller 비대칭이 남는다.
- commands로 메서드 사전 거절 — 선언하지 않은 합법적 서드파티 IPC를 차단한다.
- owner만 시작하고 extension 무시 — 기존 pre/post hook 계약을 깨뜨린다.
- headless 부팅에서 저장된 SSE를 자동 복구 — 별도 제품 정책이며 이 요청의 범위가 아니다.

## Reconsideration Triggers

**채널이 붙는 것**: manifest에 완전한 IPC 메서드 명부 계약이 생기는 경우. manifest 타입과
namespace dispatch를 함께 대조해 사전 unknown 판정을 재검토한다.

**원리적으로 안 붙는 것**: 명시적 호출 전 SSE 복원이 필요하다는 사용자 요구. 격리 홈에 저장된
설정을 두고 재부팅 시 원하는 listener 수명주기를 재현해 별도 기동 정책을 결정한다.

## References

- [ADR-0259](0259-a-kind-request-starts-the-owner-that-declares-it.md)
- [ADR-0271](0271-a-plugin-namespace-is-invoked-with-its-token-from-every-gated-caller.md)
- [ADR-0277](0277-ipc-admission-and-observation-run-once.md)
- [Plugin permissions](../dev-guide/plugin-permissions.md)
