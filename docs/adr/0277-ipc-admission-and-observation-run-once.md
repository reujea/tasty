# ADR-0277: IPC 진입 검사와 허용 관측은 요청마다 한 번 수행한다 — ADR-0152의 중첩 게이트·Allow 위치 개정

- **Status**: Accepted
- **Date**: 2026-09-15
- **Tags**: ipc, rate-limit, telemetry, permissions, headless
- **Group**: permission-security

## Context

외부 소켓의 GUI App 인터셉트·namespace forward·전 창 합산 응답과 headless App
인터셉트는 일반 handler 전에 끝난다. 권한 검사만 앞에 두면 rate-limit과 허용 호출의
ipc_calls 관측을 건너뛴다. 반대로 기존 plugin 진입점은 바깥과 안쪽에서 rate-limit을
중복 소비할 수 있다. rate-limit은 통과할 때도 토큰을 소비하므로 부수효과 없는 술어가 아니다.

ADR-0152의 “안쪽 게이트도 남긴다”와 “Allow 기록은 옮기지 않는다” 조항만 개정한다.
라우팅 전에 검사한다는 원칙, 권한 표·namespace 토큰·Local 예외, cap의 기존 적용 대상,
ADR-0085의 deny-only audit 보존과 ADR-0246의 telemetry 관측 정책은 개정하지 않는다.

## Decision

GUI·headless의 외부 IPC와 plugin host-call은 공통 check_request에서
권한 → cap → rate-limit을 검사하고, 통과한 요청만 telemetry와 Allow audit 호출을 한 번
수행한다. audit의 Allow 영속 여부는 기존 audit 정책이 결정한다. telemetry 관측을
조기 반환 경로의 예외로 두지 않는다.

공통 게이트는 요청과 caller를 빌린 CheckedRequest를 반환한다. 그 필드는 private이며
wire에서 역직렬화할 수 없다. App 라우팅과 일반 handler는 그 증거를 전달받아 실행하고,
검사·소비·관측을 반복하지 않는다. 다른 요청이나 plugin이 새로 보내는 host-call은 별도
진입이므로 새로 검사한다.

별칭은 게이트에서 canonical 이름으로 판정·관측한다. Local과 호스트 caller의 기존
면제, telemetry 및 rate-limit 복구 메서드의 예외를 유지한다. 거부는 요청 동작에
진입하지 않으며 audit Deny와 기존 capability elevation만 남긴다.

GUI에 main/parked engine이 전혀 없는 구간은 Local 부팅 명령만 허용한다. Agent는
관측 저장소 문맥 없이 우회시키지 않고 application-state 오류로 거절한다.
namespace의 기동 범위와 요청 대상·포커스 선택은 바꾸지 않는다.

## Consequences

- **얻은 것**: 조기 반환과 일반 handler가 같은 예산·관측 계약을 지키며 burst=1의 첫
  허용 요청은 통과하고 다음 요청은 throttled가 된다.
- **잃은 것**: 종전에 누락되던 App·합산·namespace 호출도 기존 설정된 한도를 소비하고
  ipc_calls에 포함된다. 그 요청들로도 cap·anomaly 입력이 늘어난다.
- **운영 비용 / 유지 부담**: 새 라우터는 검사 완료 객체를 전달해야 한다. engine 없는
  비-Local 호출자는 부팅 완료 후 재시도해야 한다. 기존 fail-open 저장소 오류 정책은 유지한다.

## Alternatives Considered

- **외부 진입부에 rate 검사만 더한다** — 일반 handler에서 두 번 소비하고 조기 반환
  Allow 관측은 계속 빠진다.
- **인터셉트의 Allow 관측을 정책적 예외로 만든다** — 같은 호출이 어떤 라우터에서
  답하는지에 따라 cap·anomaly 입력이 달라져 ADR-0246의 원칙에 맞지 않는다.
- **caller를 Local로 바꾸거나 wire에 checked 플래그를 둔다** — 실제 권한/신원과
  검사 상태를 혼동하고 외부 입력이 경계를 건너뛸 수 있다.

## Reconsideration Triggers

**채널이 붙는 것**

- 검사 완료 객체가 public 생성자나 역직렬화 경로를 갖게 된다. 그 경로가 요청·caller
  검사를 통하지 않고 증거를 만들 수 있는지 소스에서 검토한다.
- rate-limit의 소비 위치나 telemetry의 제외 대상이 바뀐다. handler/checked의
  단위 시험과 격리 IPC의 burst=1·summary·deny audit 대조를 다시 수행한다.

**원리적으로 안 붙는 것**

- 실제 외부 Agent가 engine 없는 상태에서 반드시 수행해야 하는 작업이 보고된다.
  재는 법: 격리 GUI에서 main/parked가 없는 상황을 만들고 해당 토큰·메서드를 호출해
  필요한 저장소와 관측 문맥을 확인한다.

## References

- 개정 대상: [ADR-0152](0152-gates-run-before-routing-not-inside-it.md) (중첩 게이트·Allow 기록 위치)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [ADR-0246](0246-telemetry-has-no-opt-out-and-its-token-is-a-declaration.md)
- [ADR-0085](0085-ipc-log-retention-bounded.md)
- 현재 구현: src/adapters/ipc/handler/checked.rs의 check_request·CheckedRequest,
  src/adapters/ipc/handler.rs의 handle_checked_request,
  src/app/dispatch/intents.rs의 gates_before_routing·dispatch_checked
- [headless IPC](../dev-guide/headless-ipc-surface.md) · [telemetry](../features/telemetry/index.md)
