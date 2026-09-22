# ADR-0012: tmux control mode(DCS) 및 DECRQSS 는 미지원

- **Status**: Deferred
- **Date**: 2026-06-18
- **Tags**: terminal, vte, dcs, tmux, decrqss, scope, deferred
- **Group**: terminal-input

## Context

termwiz 파서는 DCS(Device Control String) 계열 시퀀스를 별도 `Action` 으로 내보낸다.
이 중 두 가지가 VTE 커버리지 감사에서 미처리로 확인됐다:

- **tmux control mode** — tmux 가 `\x1bPtmux;…\x1b\\` 류 DCS 로 내보내는 제어 채널.
  외부 멀티플렉서(tmux)가 자신을 제어 모드로 구동할 때 쓰는 프로토콜로, 터미널이 이를
  파싱·중계해야 tmux control mode 통합이 성립한다.
- **DECRQSS**(`DCS $ q … ST`, Request Selection or Setting) — 현재 SGR/마진/커서 스타일 등
  **설정 상태를 문자열로 질의**하고 터미널이 `DCS 1 $ r … ST`(유효) / `DCS 0 $ r … ST`(무효)로
  회신해야 하는 응답 필수 query.

tasty 는 둘 다 catch-all 로 드롭한다. tmux 통합은 tasty 의 현재 범위(AI 코딩 에이전트용
터미널)에서 우선순위가 낮고, tasty 는 자체 멀티플렉싱(Workspace/Pane/Tab/Surface 계층)을
이미 제공한다. DECRQSS 는 표준상 응답 필수지만 실사용 빈도가 매우 낮다.

## Decision

**둘 다 미지원으로 둔다(현행 드롭 유지).** tmux control mode 는 tasty 자체 계층 구조와
역할이 겹치고 통합 수요가 확인되지 않았으므로 구현하지 않는다. DECRQSS 는 응답 필수지만
드물고, 무응답 시 앱이 기본값으로 폴백하므로 지금 구현하지 않는다.

"영구 거부"가 아니라 **보류(Deferred)** 다 — 우선순위가 낮을 뿐이며, 실수요가 확인되면
DECRQSS(소규모, 응답만)부터 착수할 수 있다.

## Consequences

- **얻은 것**: DCS 파싱·상태 직렬화·중계 로직을 들이지 않아 터미널 코어가 단순하게 유지된다.
  코어/에이전트 기능에 자원을 집중한다.
- **잃은 것**: tasty 안에서 `tmux -CC`(control mode)를 구동해도 통합되지 않는다(일반 tmux 는
  control mode 없이 정상 동작하므로 영향은 control mode 사용자에 한정). DECRQSS 로 설정을
  되묻는 일부 앱은 회신을 못 받아 기본값을 가정한다(드묾).
- **운영 비용 / 유지 부담**: 없음(현행 드롭 유지). 이 보류를 모르는 사람이 "tmux control mode
  가 안 붙는다"를 버그로 오인할 수 있어 본 ADR 이 그 근거가 된다.

## Alternatives Considered

- **tmux control mode 통합**: 대형 작업이고 tasty 의 자체 계층과 역할이 중복된다. 수요 미확인
  상태에서 투자 부적합.
- **DECRQSS 만 응답**: 비용은 작으나(설정 일부를 문자열로 회신) 실사용이 드물어 지금은 보류에
  포함. 수요 확인 시 가장 먼저 분리 착수 가능.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- tasty 안에서 외부 tmux 를 control mode 로 붙이려는 실제 사용 요구가 확인될 때.
- DECRQSS 무응답으로 특정 앱이 오동작하는 사례가 보고될 때.
- 코어/에이전트 우선 작업이 소진되어 응답 필수 query 정합성을 마저 채울 여력이 생길 때.

## References

- 영향 파일: `crates/tasty-terminal/src/vte_handler.rs`(최상위 `Action` 드롭)
- 관련: ADR-0002(VTE 파싱 구조), ADR-0008(인라인 그래픽 보류 — 같은 "범위 밖 보류" 계열)
