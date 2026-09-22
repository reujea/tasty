# ADR-0023: Shift+좌클릭 드래그 = 마우스 리포팅 우회 로컬 텍스트 선택 + 안내 toast 범용화

- **Status**: Accepted
- **Date**: 2026-06-26
- **Tags**: terminal, mouse, mouse-reporting, selection, modifier, clipboard, discoverability, ux
- **Group**: terminal-input

## Context

[ADR-0019](0019-mouse-button-reporting-app-delegation.md) 는 마우스 트래킹 앱(vim `:set mouse=a`,
htop, Claude Code 등)에서 좌클릭/드래그/릴리스를 포함한 모든 마우스를 앱에 **전면 위임**하기로
결정했다. 그 결과 트래킹 ON 인 surface 위에서는 화면 글자를 드래그해도 tasty 로컬 `text_selection`
이 **생성조차 되지 않아**, 복사 단축키를 눌러도 읽을 선택이 없어 무동작이 된다. 사용자 관점에선
"드래그해도 복사가 안 됨" 으로 보인다 — 트래킹 앱 위에서 텍스트를 클립보드로 옮기려면 키보드 vi
복사 모드밖에 길이 없었다.

ADR-0019 는 이 상황의 **Reconsideration Trigger** 로 *"트래킹 앱에서 로컬 텍스트 선택 ... 요구가
실제로 강하면 modifier 우회를 별도 ADR 로 추가한다(앱 위임을 깨지 않는 opt-in 형태)"* 를 명시해
두었고, Alternatives 에서도 *"Shift(또는 Option류) modifier 상시 우회(iTerm 모델)"* 를 후속 과제로
보류했다. 본 ADR 이 그 트리거의 발동(좌클릭 선택 축)이다.

우클릭 축은 이미 [ADR-0022](0022-shift-rightclick-context-menu-bypass.md) 가 동일 구조(ADR-0019 의
후속 과제 → 별도 ADR)로 `Shift+우클릭 = tasty 컨텍스트 메뉴 우회` 를 결정했다. 좌클릭 선택 축도
대칭으로 ADR 화한다. ADR-0022 는 또한 트래킹 진입 시 "마우스 캡처 중 — 메뉴는 Shift+우클릭" 안내
toast 를 우클릭 전용으로 두었는데, 좌클릭 우회가 추가되면 안내가 좌·우 두 우회를 모두 알려야 한다.

## Decision

ADR-0019 의 앱 위임 원칙을 **유지한 채**, 두 가지를 추가한다.

1. **Shift+좌클릭 드래그 = 마우스 리포팅 우회 로컬 텍스트 선택.** 트래킹 ON 인 surface 위에서도
   `Shift` 가 눌린 좌클릭은 앱에 보고하지 않고 tasty 로컬 텍스트 선택을 시작한다. 핵심 불변식:
   **Shift 여부는 press 시점에 1회만 판정하고 그 판정을 전용 상태 플래그(`left_select_bypass`)로
   release 까지 유지한다** — motion 마다 Shift 를 재검사하지 않으므로 드래그 도중 Shift 를 떼도
   선택이 깨지지 않는다(iTerm 동작과 동일). 멀티클릭(Shift+더블=word / 트리플=line)도 `dragging`
   여부와 무관하게 같은 플래그로 일관 라우팅되어, release 가 앱에 새지 않는다. 기본 좌클릭(Shift
   없음)은 ADR-0019 그대로 앱에 위임한다.
   - 표준 정합성: Shift 가 마우스 리포팅을 우회하는 것은 xterm/iTerm2 의 표준 관례다(새 발명 아님).
   - opt-in 성격: 앱 위임을 깨지 않는다 — 일반 좌클릭은 여전히 앱이 소유하고, `Shift` 라는 명시적
     modifier 가 있을 때만 tasty 가 로컬 선택으로 가로챈다. 선택이 생기면 기존 복사 단축키가 그대로
     동작한다(복사 경로 무변경).
2. **트래킹 안내 toast 범용화.** ADR-0022 의 안내 toast 를 우클릭 전용에서 일반화한다. 메시지를
   범용("텍스트 선택은 Shift+드래그, 메뉴는 Shift+우클릭")으로 바꾸고, 무장 플래그를 좌·우 보고
   경로가 **공유**해 **좌·우 클릭 중 먼저 발생한 캡처 상호작용 1회**에만 띄운다(먼저 소비한 쪽이
   disarm). 무장은 트래킹 `None→ON` 엣지, disarm 은 트래킹 OFF·RIS·소비 시 — 세션당 1회라는 기존
   메커니즘은 불변이다. 설정 토글(`mouse_capture_hint`, 기본 ON)로 끌 수 있다.

## Consequences

- **얻은 것**: 트래킹 앱(Claude Code, vim, htop 등) 위에서도 `Shift`+드래그로 텍스트를 선택·복사할
   수 있다 — 키보드 vi 복사 모드 외에 마우스 선택 경로가 열렸다. 안내 toast 가 좌·우 두 우회를 모두
   알려 "드래그해도 복사 안 됨" / "우클릭 먹통" 오인이 사라진다. ADR-0019 의 앱 위임 기본값은 그대로
   다(plain 좌클릭 회귀 없음).
- **잃은 것**: Shift+좌클릭이 앱에 보고되지 않는다 — 이전에는 인코더가 shift 비트(=4)를 실어 보고할
   수 있었다. 트래킹 앱이 Shift+좌클릭에 의존하는 경우는 드물고, 표준 관례상 Shift 우회가 우선이라
   [ADR-0022](0022-shift-rightclick-context-menu-bypass.md) 가 우클릭에 대해 받아들인 것과 **동일한
   트레이드오프**다.
- **불변**: 트래킹 OFF 일 때의 좌클릭은 기존처럼 동작한다 — Shift+click=범위 확장(extend),
   plain click=새 선택(start). 트래킹 ON 의 Shift 우회는 이전 로컬 앵커가 없으므로 extend 가 아니라
   항상 새 선택을 시작한다.
- **운영 비용 / 유지 부담**: `MainView` 에 상태 필드 하나(`left_select_bypass`)가 늘었다. 분기 결정은
   순수 함수(`left_click_local_select`)로 추출해 단위 테스트로 고정했다.

## Alternatives Considered

- **"마우스 리포팅 무시" surface 단위 토글** (ADR-0019·0022 가 거론한 대안) — 트래킹 중 모든 마우스를
  로컬로 되돌리는 토글. 강력하나 앱 마우스 기능 전체를 끄는 무거운 모델이라, 단발 선택에는 과하다.
  modifier 우회가 더 가볍고 표준적이며 우클릭 축(ADR-0022)과 대칭이다.
- **Option/Alt modifier 우회** (iTerm 기본) — macOS 관례엔 맞으나 Alt 는 터미널에서 meta 키로 더 자주
  쓰여 충돌 여지가 크다. `Shift` 가 3 OS 공통으로 안전하고 우클릭 우회와 동일 modifier 라 학습 비용이
  낮다.
- **press 시점 1회 판정 대신 매 motion 마다 Shift 재검사** — 구현은 단순하나 드래그 도중 Shift 를
  떼면 선택이 끊겨 사용성이 나쁘고 iTerm 동작과 어긋난다. 또 멀티클릭(word/line)은 `dragging=false`
  라 motion 가드만으로는 release 가 앱에 새는 버그가 난다. 전용 bypass 플래그(press set / release
  clear)로 일관 처리한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- Shift+좌클릭에 의존하는 트래킹 앱 사용 사례가 실제로 보고되면, 우회 modifier 를 설정으로 바꾸거나
  per-app 예외를 검토한다.
- 트래킹 중 로컬 선택/메뉴를 광범위하게 되살리려는 요구가 커지면 **"마우스 리포팅 무시" surface 토글**
  (ADR-0019·0022 가 보류한 대안)을 별도 설계 + ADR 로 추진한다.
- 안내 toast 가 거슬린다는 피드백이 많으면 기본값(ON)을 재고한다.

## References

- 영향 파일:
  - `src/view/main.rs`(`MainView.left_select_bypass` 상태 필드)
  - `src/view/main/mouse.rs`(좌클릭 press/motion/release 의 Shift 우회 분기 + `left_click_local_select`
    순수 함수 + 좌·우 공유 안내 toast 배선)
  - `src/view/main/selection.rs`(`start_selection` 재사용)
  - `crates/tasty-terminal/src/modes.rs`(`MouseTrackingMode`, `take_mouse_capture_hint`),
    `crates/tasty-terminal/src/lib.rs`(`mouse_capture_hint_armed`),
    `crates/tasty-terminal/src/vte_handler/esc.rs`(RIS disarm)
  - `crates/tasty-settings/src/general.rs`(`mouse_capture_hint` 설정, 구 `right_click_capture_hint`
    serde alias), `src/view/settings/ui/tabs/terminal.rs`(설정 UI)
  - `lang/{en,ko,ja}.toml`(`toast.mouse_capture_hint` 범용 메시지, `mouse_capture_hint_label`)
- 관련: [ADR-0019](0019-mouse-button-reporting-app-delegation.md) — 본 ADR 이 그 "후속 과제 /
  Reconsideration Trigger(트래킹 앱 로컬 선택 요구 → modifier 우회)" 의 **좌클릭 축**을 해소한다.
- 관련: [ADR-0022](0022-shift-rightclick-context-menu-bypass.md) — 우클릭 축의 동일 패턴 선례. 본 ADR
  이 그 안내 toast 를 좌·우 첫 상호작용 1회 + 범용 메시지로 확장한다.
- 표준 관례: xterm/iTerm2 의 Shift+마우스 리포팅 우회.
