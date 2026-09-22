# ADR-0072: child terminal 상태를 hook push 캐시 단독에서 hook+관측 융합 판정으로 바꾼다

- **Status**: Accepted
- **Date**: 2026-08-18
- **Tags**: child-terminal, agent-collaboration, liveness, staleness, hook, observation, heuristic, self-heal, ipc
- **Group**: agent-integration

## Context

`terminal.children` / `terminal.state`(그 위의 `tasty claude children` · `tasty codex children`)가
보고하는 자식 상태는 `ChildTerminalRegistry` 의 bool 맵 두 개(`idle` / `needs_input`)를
되읽은 값이었다. 둘 다 false 면 `"active"` 를 반환하는데, 그 값의 실제 의미는
"작업 중" 이 아니라 **"idle 이라는 증거가 없음"** 이다.

registry 상태를 바꾸는 유일한 경로는 에이전트 hook push 단방향
(`terminal.set_state`)이다. 자식이 멈추거나 hook 이 한 번이라도 유실되면 마지막으로
찍힌 `active` 가 그대로 남고, 그것을 되돌릴 다른 신호원이 없다. `state_of` 위에
얹힌 self-heal(`reconcile_with_live_surfaces`)은 **surface 생존만** 보므로, 자식
에이전트 프로세스가 죽어도 셸 surface 는 살아 있어 항목이 남고 상태도 `active`
그대로다.

실제로 오케스트레이션이 정지했다 — 자식 4 개가 이미 작업을 끝냈는데 `children` 이
계속 `"active"` 를 반환했고, 부모 에이전트가 약 2 시간을 대기로 낭비했다.

판정 재료 자체는 이미 호스트 안에 있었다: 라이브 surface 집합, 1Hz 전경 프로세스
스냅샷, `is_surface_busy`, `Terminal::last_output_at`. 다만 어느 것도 자식 상태
판정에 연결돼 있지 않았고, `exited` 대조조차 단건 조회(`terminal.state`)에만 있고
목록(`terminal.children`)에는 없어 두 경로의 답이 갈려 있었다.

## Decision

hook push 축과 호스트 관측 축을 **융합**해 파생 상태를 만들고, `terminal.children` /
`terminal.state` 두 경로가 그 단일 헬퍼를 공유하게 한다.

- registry 에는 **"언제 보고받았나" 축**(`last_state_report_at`, unix epoch ms)만
  추가한다. hook push 마다 갱신되고 `register_child` 가 등록 시각으로 시딩한다.
  `state_of` 의 시그니처·동작·기존 테스트는 불변이다 — 필드 추가는 계약 변경이
  아니다. epoch 기반이라 호스트 재시작을 건너 살아남는다.
- **관측 축 합성은 상위 계층**(`src/core/state/child_liveness.rs`)에서 한다. registry
  는 host-IPC-free 단위 테스트 계층이라 터미널/프로세스에 접근할 수 없다. 판정
  함수 `derive_child_state(registry_state, &ChildObservation)` 는 순수 함수라
  관측 조합을 직접 주입해 테스트한다.
- 결과는 **`state` + `evidence` + `confidence` 3 축**으로 분리한다. `state` 에는
  기존 `idle`/`needs_input`/`active`/`exited` 에 `stale` 하나가 추가된다.
- **파생 상태는 출력 전용**이다. `terminal.set_state` 는 여전히
  `idle`/`needs_input`/`active` 세 값만 받는다 — hook 이 `stale`/`exited` 를 밀어넣을
  수 있으면 관측 축이 다시 push 캐시로 퇴화한다.
- **능동 프로빙은 배제**한다. 대상 surface 에 입력을 주입해 반응을 보는 방식은 사용자
  입력 재현이라 release 금지 대상이고(`docs/identity.md` 원칙 1) 자식 에이전트 상태도
  오염시킨다. 수동 관측만 쓴다.

판정 우선순위는 고정이다: surface 부재 → hook 보고(`needs_input`/`idle`) → busy →
PTY 미기동 게이트 → 전경 셸 복귀 → 관측 불가 게이트 → 무출력·hook 침묵 임계값.

## Consequences

- **얻은 것**:
  - hook 유실 시 자기치유 경로가 생겼다. 자식 에이전트가 종료돼 전경이 셸로
    되돌아오면 **확정** 판정으로 `stale` 이 나온다.
  - `terminal.children` 과 `terminal.state` 가 같은 헬퍼를 쓰므로 목록과 단건의 답이
    구조적으로 일치한다(개선 전에는 `exited` 판정이 단건에만 있었다).
  - `confidence` 축이 있어 소비자가 "확정 판정만 종결로 다룬다" 를 선택할 수 있다.
    휴리스틱 판정을 확정처럼 오해할 여지를 값 자체로 막는다.
  - `evidence` 축이 있어 "왜 이 값인가" 를 응답만 보고 판별할 수 있다.
- **잃은 것**:
  - `state` 값 집합이 늘었다. `stale` 을 모르는 기존 소비자는 그 값을 미지 상태로
    본다(기존 네 값의 의미·문자열은 그대로라 하위호환은 유지된다).
  - 무출력 기반 판정은 원리적으로 휴리스틱이다 — SIGSTOP 으로 멈춘 프로세스, 긴
    추론 중인 에이전트, 출력이 없는 긴 명령은 관측상 구별되지 않는다. 오탐이 0 이
    되지는 않는다.
- **운영 비용 / 유지 부담**:
  - 추가 프로세스 스냅샷은 없다. 전경 프로그램 이름은 1Hz 일괄 폴링이 이미 채우는
    `foreground_names` 캐시에서만 읽는다 — 자식마다 `foreground_process_info()` 를
    개별 호출하면 O(surfaces × processes) 회귀다.
  - 임계값 상수 2 개(`CHILD_OUTPUT_SILENCE` 120s, `CHILD_HOOK_SILENCE` 300s)가
    유지 대상이다. 에이전트 CLI 의 출력 패턴이 바뀌면 재조정이 필요하다.

## Alternatives Considered

- **관측 축만 쓰고 registry 는 전혀 건드리지 않는다** — `state_of` fallback 이 문서화된
  계약이고 `terminal.state` 가 이미 같은 패턴으로 `exited` 를 상위에서 판별하므로,
  registry 를 불변으로 두는 안. 기각: 이번 사고의 본질은 "출력이 없다" 가 아니라
  **"hook 이 N 시간째 안 온다"** 였고, 무출력은 그 대리 지표에 불과하다. hook 침묵은
  PTY 출력과 별개 축이라 registry 없이는 표현할 수 없다. 또 PTY `last_output_at` 은
  `Instant` 라 호스트 재시작 시 소멸하는 반면 registry 는 영속된다. "계약 보존"
  근거도 약하다 — 필드 추가는 `state_of` 의 시그니처·동작을 바꾸지 않는다.
- **registry 에 타임스탬프만 넣고 PTY 관측은 안 쓴다** — 기각: hook 침묵만으로는
  "정상적으로 오래 일하는 자식" 과 "멈춘 자식" 이 갈리지 않는다. busy 양성과 전경 셸
  복귀는 hook 과 무관한 **확정** 관측이라, 이 둘을 버리면 확정 판정 수단이 surface
  부재 하나만 남는다.
- **능동 프로빙(입력 주입 후 반응 관측)** — 기각: 사용자 입력 재현이라 release 금지
  대상이고(원칙 1), 자식 에이전트의 프롬프트 상태를 오염시킨다.
- **전경이 셸이면 `exited` 로 판정** — 기각: surface 는 살아 있으므로 `exited`(kill /
  respawn 대상)와 의미가 다르고, `terminal.adopt` 로 들어온 자식은 애초에 에이전트가
  아닌 일반 셸일 수 있어 종료로 단정할 수 없다. `stale` + `evidence` 로 관측한 사실만
  그대로 노출한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `stale` 오탐이 실사용에서 반복 관측된다 — 특히 정상적으로 긴 무출력 구간을 갖는
  에이전트 CLI 가 등장하면 임계값 또는 축 구성을 재조정한다.
- 자식 에이전트 프로세스의 생존을 PID 단위로 확정 판정할 수단이 생긴다(1Hz 전경
  스냅샷이 PID 까지 캐시하도록 확장되는 등). 그때는 이름 기반 셸 판정보다 강한
  확정 축으로 대체한다.
- mirror(remote attach) surface 가 실제로 child 로 등록될 수 있게 된다. 현재는
  `terminal.adopt` 가 hard 점유 대상을 거부해 발생 시나리오가 없어 "판정 불가" 로만
  처리하고 있다.
- hook 전달이 유실 없는 채널로 바뀐다(ack/재전송). 그러면 관측 축의 비중을 낮출 수
  있다.

## References

- [`docs/features/child-terminal/index.md`](../features/child-terminal/index.md) — registry·인터페이스 현재 계약
- [`docs/identity.md`](../identity.md) 원칙 1 — 능동 프로빙 배제 근거
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — child-terminal registry 와 soft 점유
- `src/core/child_terminal.rs` · `src/core/state/child_liveness.rs` · `src/adapters/ipc/handler/terminal.rs`
