# ADR-0019: 마우스 버튼/드래그 리포팅 — 트래킹 앱에 전면 위임, 로컬 선택 우회는 보류

- **Status**: Accepted
- **Date**: 2026-06-24
- **Tags**: terminal, vte, mouse, mouse-reporting, sgr, input, selection, scope
- **Group**: terminal-input

## Context

tasty 는 마우스 트래킹 모드(DECSET 1000/1002/1003 + 1006 SGR)를 **인식만** 했다 — 모드를
`MouseTrackingMode` 로 저장하고, 트래킹 중에는 로컬 텍스트 선택을 차단했다. 그러나 정작
**버튼 클릭/릴리스/드래그를 PTY 로 보고하는 구현이 없었다**(`encode_wheel_report` 로 휠만 보고).

그 결과 트래킹을 켜는 앱(vim `:set mouse=a`, htop, Claude Code 등)에서:
- 드래그가 **로컬 선택도 안 되고**(트래킹이라 차단), **앱에도 안 감**(버튼 보고 미구현) → 마우스가
  사실상 무동작.
- 클릭으로 위치 이동·항목 선택 같은 앱 마우스 기능이 동작하지 않음.

ADR-0013 이 "마우스는 표준 경로(1000/1002/1003 + 1006 SGR)로 충분"이라 전제했으나, 버튼 보고가
없어 그 전제가 실제로는 미충족이었다.

## Decision

**버튼 press/release/드래그 motion 리포팅을 구현하고, 트래킹 중 마우스를 앱에 전면 위임한다.**

- 인코딩: SGR(1006) `ESC [ < cb ; col ; row (M|m)` / legacy X10 `ESC [ M …` 폴백. modifier 비트
  (shift=4 · alt=8 · ctrl=16), 드래그 motion 비트(`|32`). 휠·버튼·debug 주입이 단일 함수
  `encode_mouse_report` 를 공유.
- 트래킹 ON 이면 **left / right / middle / wheel 을 전부 앱에 보고**. 드래그 motion 은 CellMotion(1002)
  /AllMotion(1003) 에서 셀 단위로 보고.
- 트래킹 중에는 tasty **로컬 텍스트 선택을 하지 않는다.** 우클릭 컨텍스트 메뉴도 트래킹 OFF
  에서만 띄운다(트래킹 ON 이면 우클릭도 보고).
- 트래킹 중 로컬 선택을 되살리는 **modifier 우회·"리포팅 무시" 토글은 두지 않는다(보류).** 앱이
  마우스를 켰으면 앱이 소유한다는 원칙을 우선한다.

## Consequences

- **얻은 것**: vim·htop·Claude Code 등에서 마우스 클릭/드래그가 표준대로 동작한다. 마우스 인코딩이
  휠·버튼·debug 주입 한 경로(`encode_mouse_report`)로 수렴해 드리프트가 없다.
- **잃은 것**: 트래킹 앱에서 **로컬 텍스트 선택과 우클릭 컨텍스트 메뉴를 (현재) 쓸 수 없다** —
  키보드 vi copy mode 로 대체한다. iTerm2 도 동일 트레이드오프이며(트래킹이 우클릭 메뉴/선택을
  가린다는 공식 회귀 이슈 존재), Option 같은 modifier 우회로 푼다.
- **미검증(macOS)**: **미들클릭 보고는 구현했으나** macOS 환경에서 가시적 동작(vim `<MiddleMouse>`
  = paste)이 확인되지 않았다. macOS 에는 X11 primary selection 이 없어 vim 이 붙일 내용이 없으면
  조용한 것이 정상일 수 있다 — tasty 의 `button 1` 보고 자체는 정상 발행된다. **추후 실측으로
  확인되면 본 ADR 과 docs 를 갱신한다.**

## Alternatives Considered

- **항상 로컬 선택(앱 마우스 포기)** — 트래킹 앱의 마우스 기능을 영구 상실. 임시 디버그 단계에서
  검토했으나 표준 위배라 기각.
- **Shift(또는 Option류) modifier 상시 우회** — iTerm 모델. 표준적이나 "tasty 만의 선택 동작"을
  지금 도입하지 않기로 하여 보류(후속 과제).
- **우클릭 컨텍스트 메뉴 항상 유지(부분 비준수)** — left/middle 만 보고. 동작 일관성이 깨지고
  앱이 우클릭을 못 받는다. 트래킹 중 메뉴는 후속 토글로 해결하는 편이 깔끔.
- **"마우스 리포팅 무시" 토글** — 트래킹 앱에서도 로컬 선택/메뉴를 되살리는 surface 단위 토글.
  유용하나 본 작업 범위 밖(별도 설계 + ADR).

## Reconsideration Triggers

- 트래킹 앱에서 로컬 텍스트 선택 또는 우클릭 메뉴 요구가 실제로 강하면, **"리포팅 무시" 토글**
  또는 **modifier 우회**를 별도 ADR 로 추가한다(앱 위임을 깨지 않는 opt-in 형태).
- **미들클릭 paste 가 macOS 에서 동작/미동작**임이 실측되면 동작과 문서를 확정한다.

## References

- 영향 파일: `crates/tasty-terminal/src/mouse_report.rs`(`encode_mouse_report`),
  `src/view/main/mouse.rs`(press/release/motion 보고 + 트래킹 분기),
  `src/adapters/ipc/handler/debug.rs`(`inject_mouse` 공용 인코딩),
  `crates/tasty-terminal/src/modes.rs`(`MouseTrackingMode`).
- 관련: ADR-0013(레거시·니치 입력 사설 모드 — "마우스 표준 경로로 충분" 전제를 본 ADR 이 실현),
  CLAUDE.md "크로스 플랫폼" 원칙.
- 후속(본 ADR 의 Reconsideration Trigger "modifier 우회" 해소): [ADR-0022](0022-shift-rightclick-context-menu-bypass.md)(우클릭 축), [ADR-0023](0023-shift-leftclick-selection-bypass.md)(좌클릭 선택 축).
