# ADR-0136: 조회는 자기가 관측하는 것을 만들지 않는다 — headless plugin 조회 표면

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: headless, plugin, ipc, identity-principle-2, agent-surface, observability
- **Group**: architecture

## Context

headless 는 **CLI 전용 실행 형태**다. [`docs/identity.md`](../identity.md) 원칙 2 는
에이전트 기능이 IPC + CLI 양면으로 동작할 것을 요구하는데, headless 에서 `plugin.*`
관리 표면이 통째로 없어 `plugin.list` 조차 `-32601` 이었다. 성능이나 편의가 아니라
원칙의 문제다.

기술적 원인은 라우팅이다. gui 는 5-step 라우터(`src/app/ipc.rs`)를 쓰고 그 step 2 가
`App` 층 상태를 읽는 메서드를 처리하는데, headless pump(`src/boot/headless_dispatch.rs`)
는 caller 해석 → engine handler 직결로 간소화하면서 그 step 을 통째로 생략한다.
`plugin.*` 는 `App.plugin_manager` 를 읽으므로 engine handler 가 닿지 못한다.

그런데 그 필드는 headless 에 **있다**. `src/boot/headless_plugins.rs` 가 채운다. 다만
**lazy** 다 — headless 데몬은 attach 세션이 없으면 plugin 을 하나도 띄우지 않는 것이
기본값이고, 매니저는 호스트가 모르는 메서드가 plugin namespace 로 forward 될 때 비로소
선다. 그래서 조회에 답하려면 그 매니저를 세워야 하는데, 세우는 과정을 통째로 부르면
**조회가 자기 관측 대상을 바꾼다.**

## Decision

### 1. 조회가 부르는 층과 기동하는 층을 가른다

headless 부트스트랩을 두 함수로 나눈다.

- `ensure_plugin_manager_metadata` — 매니저 객체 생성 + 디스크 스캔(`refresh_packages`).
  **쓰기가 없다.** 조회 메서드가 부르는 층이다.
- `ensure_plugin_manager` — 위에 번들 설치와 plugin 프로세스 기동을 얹는다. 조회는
  부르지 않는다.

**경계는 번들 설치 위다.** 처음에는 "프로세스를 띄우는 것" 만 조회에서 배제하고 번들
설치는 "관측 대상을 정확하게 만드는 일" 로 분류하려 했으나, 그 함수가 실제로 하는 일이
다르다: 사용자 plugin 디렉터리에 **번들에서 파일을 복사하고, 매니페스트가 선언한 권한을
설정 파일에 자동으로 grant 한다.** 없던 설치를 만들고 권한까지 부여하는 것은 관측 대상을
정확하게 만드는 것이 아니라 **만들어내는** 것이며, 프로세스를 띄우는 것보다 먼저 배제된다.

판별 기준은 이렇게 선다 — **부수효과가 관측 대상 자체를 바꾸는가.** 디스크를 읽어
목록을 채우는 것은 관측이고, 설치·권한 부여·프로세스 기동은 관측 대상의 변경이다.

그 결과 아무것도 설치되지 않은 홈에서는 목록이 빌 수 있다. 그것은 거짓이 아니라 그
시점의 사실이며, 매니저가 아예 없을 때의 응답(`-32000`)과 구분된다.

### 2. 라우팅 표는 한 벌만 둔다

읽기 전용 `plugin.*` 의 메서드 표와 dispatch 는
`crate::adapters::ipc::handler::plugin` 한 곳에 있고, gui 라우터와 headless pump 가
**같은 함수**를 부른다.

표를 양쪽에 두면 한쪽만 고쳐지는 순간 갈라진다. 그 형태의 사고가 이 저장소에 이미
있었다 — `Option<&PluginManager>` 를 받는 네 핸들러 중 셋은 매니저 부재를
`-32000` 으로 답하는데 `handle_list` 하나만 빈 목록을 성공으로 돌려주고 있었다. 관례가
둘이었던 것이 아니라 하나였고 이탈이 하나였으며, 그 이탈이 호출자에게서 "설치된 plugin
이 없다" 와 "아직 매니저를 안 띄웠다" 를 구별할 수단을 뺏었다. 표를 복제하면 같은 일이
메서드 단위로 다시 일어난다.

표에 이름이 있는데 dispatch arm 이 없으면 조용히 "그런 메서드 없음" 으로 새지 않고
그 자리에서 internal error 로 실패한다 — 누락과 미구현이 같은 응답이 되지 않게.

### 3. 무엇을 열고 무엇을 안 여는지 메서드별로 적는다

읽기 전용 7 건만 연다. 나머지는 이유가 셋으로 갈리며, "이 표면은 GUI 가 필요하다"
같은 뭉뚱그림을 두지 않는다. 메서드별 판정표는
[headless-ipc-surface](../dev-guide/headless-ipc-surface.md).

- **쓰기** — `Core` 만 읽으므로 기술 장벽은 없으나, 감사 로그를 지우고 에이전트 권한을
  바꾸는 것은 조회와 같은 판단으로 열 대상이 아니다.
- **`App` 이분 선행** — plugin 수명주기 헬퍼가 `gui` feature 로 게이트된 모듈에 있고,
  이어지는 cascade 는 headless 스텁에 대응물이 없다.
  [ADR-0127](0127-e2e-harness-binary-selection.md) 이 같은 선행 조건을 적어 둔 자리다.
- **없는 것이 정답** — elevation popup 을 띄우는 메서드는 popup 을 보여 줄 창이 없으면
  하는 일 자체가 없다.

## Consequences

- headless 에서 에이전트가 설치된 plugin 을 조회할 수 있다. 원칙 2 가 읽기 축에서는
  닫힌다. 쓰기·수명주기 축은 열리지 않았다.
- 조회는 plugin 프로세스를 띄우지 않으므로 `running` 은 실제로 뜬 것만 참이다.
- headless 데몬이 조회를 받으면 매니저 객체가 서고 이후 유지된다. 그 전후로
  "매니저 없음" 과 "설치된 것 없음" 의 응답이 달라지는데, 둘이 **구분되는 답**인 것이
  이 결정의 의도다.
- 호스트가 모르는 메서드를 한 번이라도 부르면(오타 포함) 데몬이 plugin 을 기동하는
  기존 동작은 그대로다. 이 ADR 은 그 경로를 바꾸지 않는다 — forward 되는 메서드는
  기동이 답변의 구성요소이기 때문이다.

## Alternatives Considered

- **조회 시 매니저를 통째로 기동한다.** `plugin.list` 가 실질로 답하지만 관찰이
  관찰 대상을 바꾼다. 되돌릴 방법도 없다 — 한 번 뜬 매니저를 조회가 내리지 않는다.
- **매니저 없이 매니페스트를 직접 읽어 답한다.** `plugin.list` 가 답하는 항목 대부분은
  디스크의 매니페스트에 있고 매니저가 필요한 것은 실행 상태뿐이라, 이쪽이 더 정확하다.
  구현이 크고 이 결정과 독립이므로 지금 고르지 않았다. 열려 있는 선택지다.
- **headless 전용 dispatch 표를 따로 둔다.** 구현은 가장 짧지만 위 2 의 이유로 기각.

## Reconsideration Triggers

- **`App` 이분이 착수되면** — `gui` feature 로 게이트된 plugin 수명주기 경계가 열리므로,
  지금 "열지 않는다" 로 분류한 수명주기 메서드들이 이 경계의 **수용 기준**이 된다.
- **매니페스트 직독이 필요해지면** — 매니저를 아예 세우지 않고 답하는 것이 요구되면
  위 대안 2 를 다시 연다. 이 결정의 층 분할은 그때도 유효하다(조회 층이 더 얇아질 뿐).
- **쓰기 표면을 열기로 하면** — 감사·권한 메서드는 조회와 다른 판단이므로 별도 결정으로
  다룬다. 이 ADR 은 그것을 미룬 상태를 기록한 것이지 금지한 것이 아니다.

## References

- [headless-ipc-surface](../dev-guide/headless-ipc-surface.md) — 메서드별 판정표
- [ADR-0127](0127-e2e-harness-binary-selection.md) — `App` 이분 선행 조건
- [ADR-0111](0111-headless-drains-the-intent-queue.md) — headless 가 gui 라우터의 계약을
  따로 이행해야 하는 같은 형태
- [ADR-0134](0134-headless-drains-host-events-but-applies-only-hook-fired.md) — 같은 축의
  직전 결정
