# ADR-0022: Shift+우클릭 modifier 우회 + 트래킹 안내 toast

- **Status**: Accepted
- **Date**: 2026-06-25
- **Tags**: terminal, mouse, mouse-reporting, context-menu, modifier, discoverability, ux
- **Group**: terminal-input

## Context

[ADR-0019](0019-mouse-button-reporting-app-delegation.md) 는 마우스 트래킹 앱(vim `:set mouse=a`,
htop, Claude Code 등)에서 우클릭을 포함한 모든 마우스를 앱에 **전면 위임**하기로 결정했다. 그
결과 트래킹 ON 인 surface 위에서는 tasty 자체 컨텍스트 메뉴(텍스트 복사 / 개행 없이 복사 /
surface ID 복사)를 띄울 방법이 전혀 없다 — 우클릭이 모두 앱으로 가기 때문이다.

사용자 관점에서는 tasty 메뉴를 띄우려 우클릭했는데 트래킹 앱이 그 입력을 먹어버려 **"우클릭이
먹통"** 처럼 보인다. ADR-0019 는 이 상황의 **Reconsideration Trigger** 로 *"트래킹 앱에서 우클릭
메뉴 요구가 실제로 강하면 modifier 우회를 별도 ADR 로 추가한다(앱 위임을 깨지 않는 opt-in
형태)"* 를 명시해 두었다. 본 ADR 이 그 트리거의 발동이다.

## Decision

ADR-0019 의 앱 위임 원칙을 **유지한 채**, 두 가지를 추가한다.

1. **Shift+우클릭 = tasty 컨텍스트 메뉴 우회.** 트래킹 ON 인 surface 위에서도 `Shift` 가 눌린
   우클릭은 앱에 보고하지 않고 tasty 네이티브 컨텍스트 메뉴를 띄운다. press·release 양쪽 모두
   `report_mouse_event` 경로로 새지 않는다(인코더의 shift 비트로 가지 않음 — Shift 분기가 보고
   *이전* 에 메뉴 경로로 빠진다). 기본 우클릭(Shift 없음)은 ADR-0019 그대로 앱에 위임한다.
   - 표준 정합성: Shift 가 마우스 리포팅을 우회하는 것은 xterm/iTerm2 의 표준 관례다(새 발명 아님).
   - opt-in 성격: 앱 위임을 깨지 않는다 — 일반 우클릭은 여전히 앱이 소유하고, Shift 라는 명시적
     modifier 가 있을 때만 tasty 가 가로챈다.
2. **트래킹 안내 toast.** 트래킹 세션마다 1회, 사용자가 기본 우클릭(메뉴가 안 뜨는 경우)을 했을
   때 "마우스 캡처 중 — 컨텍스트 메뉴는 Shift+우클릭" 류의 안내 toast 를 띄운다. 설정 토글로
   끌 수 있으며 기본 ON. 우클릭이 "먹통"이 아니라 Shift 로 우회 가능함을 발견하게 돕는다.
   - **갱신(Shift+좌클릭 우회 도입 후)**: 좌클릭 선택 우회([ADR-0019] Reconsideration Trigger
     발동)가 추가되면서 안내가 좌·우 2개 우회를 모두 알려야 하므로, 메시지를 범용("텍스트 선택은
     Shift+드래그, 메뉴는 Shift+우클릭")으로 일반화하고, 무장 플래그를 좌·우 보고 경로가 공유해
     **좌·우 클릭 중 먼저 발생한 캡처 상호작용 1회**에만 띄운다. 설정 키는 `mouse_capture_hint`
     (구 `right_click_capture_hint` 는 serde alias 로 호환). 무장/disarm 메커니즘 자체는 불변.

## Consequences

- **얻은 것**: 트래킹 앱 위에서도 Shift+우클릭으로 tasty 컨텍스트 메뉴(복사 / surface ID 복사 등)
  를 쓸 수 있다. 안내 toast 로 우회 경로의 발견성(discoverability)이 확보돼 "우클릭 먹통" 오인이
  사라진다. ADR-0019 의 앱 위임 기본값은 그대로다(회귀 없음).
- **잃은 것**: Shift+우클릭이 앱에 보고되지 않는다 — 이전에는 인코더가 shift 비트(=4)를 실어
  보고했다. 트래킹 앱이 Shift+우클릭에 의존하는 경우는 드물고, 표준 관례상 Shift 우회가 우선이라
  허용 가능한 트레이드오프다.
- **불변**: 트래킹 OFF 일 때의 우클릭은 Shift 유무와 무관하게 기존처럼 컨텍스트 메뉴를 띄운다.

## Alternatives Considered

- **"마우스 리포팅 무시" surface 단위 토글** (ADR-0019 가 거론한 대안) — 트래킹 중 모든 마우스를
  로컬로 되돌리는 토글. 강력하나 앱 마우스 기능 전체를 끄는 무거운 모델이라, 단발 메뉴 호출에는
  과하다. modifier 우회가 더 가볍고 표준적.
- **Option/Alt modifier 우회** (iTerm 기본) — macOS 관례엔 맞으나 Alt 는 터미널에서 meta 키로 더
  자주 쓰여 충돌 여지가 크다. Shift 가 3 OS 공통으로 안전.
- **안내 toast 없음** — 우회만 추가. 그러나 사용자가 Shift 우회의 존재를 알 길이 없어 "먹통"
  오인이 남는다. 발견성 위해 toast 를 함께 둔다.

## Reconsideration Triggers

- Shift+우클릭에 의존하는 트래킹 앱 사용 사례가 실제로 보고되면, 우회 modifier 를 설정으로
  바꾸거나 per-app 예외를 검토한다.
- 안내 toast 가 거슬린다는 피드백이 많으면 기본값(ON)을 재고한다.

## References

- 영향 파일: `src/view/main/mouse.rs`(우클릭 분기의 Shift 우회),
  `src/view/main/redraw.rs`(`PendingNativeMenu::TerminalSurface` 렌더 — 기존),
  안내 toast 배선(연계 작업 11).
- 관련: [ADR-0019](0019-mouse-button-reporting-app-delegation.md) — 본 ADR 이 그 "후속 과제 /
  Reconsideration Trigger(트래킹 앱 우클릭 메뉴 요구 → modifier 우회)" 를 해소한다.
- 관련: [ADR-0023](0023-shift-leftclick-selection-bypass.md) — 좌클릭 선택 우회를 대칭으로 추가하며,
  본 ADR 의 안내 toast 를 좌·우 첫 상호작용 1회 + 범용 메시지로 확장한다.
- 관련: [ADR-0055](0055-mouse-capture-banner-suppress-list.md) — 본 ADR 의 Reconsideration
  Trigger("per-app 예외 검토")를 해소해, 캡처는 유지하고 안내 배너만 앱별로 억제하는 별도
  블랙리스트를 추가한다.
- 표준 관례: xterm/iTerm2 의 Shift+마우스 리포팅 우회.
