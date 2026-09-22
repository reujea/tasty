# ADR-0011: XTWINOPS 창 조작·창 상태 질의는 미지원 (크기/타이틀 스택만 응답)

- **Status**: Accepted
- **Date**: 2026-06-18
- **Tags**: terminal, xtwinops, vte, window, user-agent-separation, security, scope
- **Group**: terminal-input

## Context

XTWINOPS(`CSI Ps ; ... t`, termwiz `CSI::Window`)는 한 시퀀스 패밀리 안에 성격이
크게 다른 연산을 섞어 담는다:

1. **창 조작** — Iconify/DeIconify, MoveWindow, ResizeWindowPixels/Cells,
   Maximize/FullScreen 계열, Raise/Lower 등. PTY 쪽 프로그램이 **사용자 창의 위치·
   크기·표시 상태를 직접 바꾼다.**
2. **리포트 질의(응답 필수)** — ReportTextAreaSize(Cells/Pixels), ReportScreenSize(Cells/Pixels),
   ReportCellSizePixels, ReportWindowPosition/State, ReportWindowTitle/IconLabel 등.
   터미널이 PTY 로 현재 값을 회신한다.
3. **타이틀 스택** — Push/Pop WindowTitle·IconTitle. 앱이 자기 타이틀로 바꿨다가
   종료 시 원복하는 용도(vim/tmux).

tasty 의 불가침 원칙은 **사용자 행동과 에이전트 행동의 분리**다(identity.md). PTY 안에서
실행되는 프로그램(= 에이전트/원격 프로세스)이 보낸 이스케이프가 사용자 창 상태를 조작하거나
사용자 환경을 탐침하는 것은 이 경계를 침범한다. 동시에 (2)·(3) 중 일부는 무해하고 호환성에
도움이 된다.

## Decision

XTWINOPS 를 **선별 처리**한다.

- **응답한다**: 셀 단위 크기 리포트 — `ReportTextAreaSizeCells`(`CSI 18 t` → `CSI 8;rows;cols t`),
  `ReportScreenSizeCells`(`CSI 19 t` → `CSI 9;rows;cols t`). 그리고 **타이틀 스택**
  (`CSI 22/23 t`, push/pop, 단일 타이틀 기준, 64 entry 로 bound).
- **무시한다(미지원)**: 창 조작 전체(Move/Resize/Maximize/FullScreen/Iconify/Raise/Lower),
  창 위치·상태·타이틀 **탐침**(`ReportWindowPosition/State`, `ReportWindowTitle/IconLabel`),
  그리고 **픽셀 단위 크기 리포트**(`CSI 14/16 t` = TextAreaSizePixels/CellSizePixels).

픽셀 리포트를 뺀 이유는 정체성 위반과 별개다: 셀의 픽셀 크기는 폰트·DPI 스케일에 종속되어
**렌더러에만 존재**하고 터미널 모델에는 없다. 이를 터미널로 끌어오면 프레임마다/리사이즈마다
host→terminal 플러밍이 필요해 ADR-0002(프레임당 전 터미널 락 회피)와 충돌한다. 게다가
픽셀 셀 크기의 주 소비자는 인라인 이미지 프로토콜인데 그건 ADR-0008 로 보류 상태라 효용도 낮다.

## Consequences

- **얻은 것**: 에이전트가 보낸 이스케이프로 사용자 창이 움직이거나 최대화/최소화되거나
  창 위치·타이틀이 새어 나가는 일이 구조적으로 불가능하다. 동시에 TUI 가 흔히 묻는 셀 크기
  (`18 t`)와 vim/tmux 의 타이틀 원복(`22/23 t`)은 정상 동작해 호환성을 확보한다.
- **잃은 것**: 창 크기를 픽셀로 묻는 앱(주로 이미지 도구)은 응답을 못 받아 기본값으로 폴백한다
  (실측 영향 낮음 — 이미지 미지원이라 그 경로 자체가 비활성). 창 조작 이스케이프에 의존하는
  희귀한 자동화는 동작하지 않는다(의도된 비지원).
- **운영 비용 / 유지 부담**: 없음. 처리하는 항목은 터미널 상태(rows/cols, 타이틀)만으로 닫혀
  추가 플러밍이 없다.

## Alternatives Considered

- **전체 무시(드롭 유지)**: 셀 크기 리포트·타이틀 스택까지 버리면 일부 TUI·vim/tmux 호환성이
  떨어진다. 이 둘은 무해+유용하므로 처리하는 편이 낫다.
- **리포트 질의 전부 응답(창 위치·타이틀 포함)**: 위치·타이틀 회신은 사용자 환경 정보를
  PTY 프로세스에 흘리는 것이라 정체성상 부적절. 효용도 낮아 제외.
- **픽셀 리포트까지 응답**: 셀 픽셀 크기를 host 에서 터미널로 플러밍해야 하는데 ADR-0002 와
  충돌하고, 주 소비자(이미지)가 ADR-0008 로 보류라 비용 대비 효용이 없다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 인라인 이미지(ADR-0008)를 지원하게 되어 셀 픽셀 크기 리포트(`16 t`)가 실제로 필요해질 때.
- 픽셀 셀 크기를 ADR-0002 를 위반하지 않고 터미널 모델에 노출하는 깔끔한 경로가 생길 때.
- 창 위치/타이틀 탐침을 정체성 위반 없이 안전하게 회신할 수 있는 합의된 정책이 생길 때.

## References

- 영향 파일: `crates/tasty-terminal/src/vte_handler/osc.rs`(`handle_window`),
  `crates/tasty-terminal/src/vte_handler/control.rs`(`CSI::Window` 라우팅)
- 관련: ADR-0002(프레임당 락 회피), ADR-0008(인라인 그래픽 보류), `docs/identity.md`(사용자/에이전트 분리)
