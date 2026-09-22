# ADR-0013: 레거시·니치 입력 사설 모드는 미지원

- **Status**: Deferred
- **Date**: 2026-06-18
- **Tags**: terminal, vte, dec-private-mode, mouse, input, scope, deferred
- **Group**: terminal-input

## Context

DEC 사설 모드(DECSET/DECRST, `CSI ? Ps h/l`) 중 입력 인코딩·레거시 호환에 관한 여러 모드가
tasty 에서 미처리(catch-all 드롭)다. VTE 커버리지 감사에서 식별된 주요 항목:

- **Utf8Mouse(1005)** — 마우스 좌표를 UTF-8 로 확장 인코딩. SGR 마우스(1006)가 사실상 표준이
  되어 거의 안 쓰인다. tasty 는 이미 1006(SGRMouse)을 지원한다.
- **SGRPixelsMouse(1016)** — 마우스 좌표를 셀이 아닌 픽셀 단위로 회신. 셀 픽셀 메트릭이
  필요하고(렌더러 종속) 실사용이 드물다.
- **Win32InputMode(9001)** — ConPTY 의 Windows 전용 raw 입력 인코딩.
- 기타: DECCOLM(3, 80/132 컬럼 전환), ReverseWraparound(45),
  Meta/AltSendsEscape(1036/1039), GraphemeClustering(2027) 등.

이들은 대부분 "특정 앱/플랫폼의 입력 인코딩 변형"이며, 미지원 시 앱은 표준 인코딩
(예: 1006 SGR 마우스, 일반 ESC 입력)으로 폴백하므로 기능 상실이 아니라 인코딩 변형의 부재다.

## Decision

위 입력 사설 모드들을 **미지원으로 둔다(현행 드롭 유지).** 마우스는 표준 경로(1000/1002/1003
+ 1006 SGR)로 충분하고, 나머지는 레거시/플랫폼 변형이라 폴백이 동작한다. "영구 거부"가 아니라
**보류(Deferred)** 다 — 개별 항목은 실수요가 확인되면 독립적으로 추가할 수 있다.

특히 두 가지는 향후 확인 대상으로 명시해 둔다:

- **Win32InputMode(9001)**: Windows 1급 지원 원칙(크로스 플랫폼 불가침)과 맞닿아 있어, ConPTY
  환경에서 특정 키 입력(예: 일부 조합키·이벤트)이 누락된다는 사례가 나오면 우선 검토한다.
- **DECCOLM(3)**: 80/132 컬럼 전환은 리사이즈 정책과 엮여 단순 토글로 보기 어렵다. 별도 판단
  필요.

## Consequences

- **얻은 것**: 표준 마우스/입력 경로 하나로 동작이 수렴해 코어가 단순하고, 플랫폼·레거시 변형
  인코딩의 유지 부담이 없다.
- **잃은 것**: 1005/1016 마우스 인코딩을 강제하는 (드문) 앱, 또는 Win32 raw 입력에 의존하는
  Windows 콘솔 앱에서 일부 입력이 표준 폴백으로만 전달된다. 실측 영향은 낮음.
- **운영 비용 / 유지 부담**: 없음(현행 드롭 유지). 본 ADR 이 "왜 무시가 정당한가"의 근거다.

## Alternatives Considered

- **전부 구현**: 각 모드별 인코딩 분기를 들이는 비용 대비, 표준 폴백이 이미 동작하므로 효용이
  낮다.
- **Utf8Mouse(1005)만 추가**: 1006 SGR 마우스가 우월하고 호환성이 넓어 1005 를 따로 둘 이유가
  약하다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 (항목 단위로) 재검토한다.

- Windows(ConPTY)에서 Win32InputMode 미지원으로 특정 키/이벤트 입력이 누락된다는 사례가
  확인될 때.
- 픽셀 마우스(1016)나 132 컬럼(DECCOLM)에 의존하는 실제 워크플로 요구가 확인될 때.

## References

- 영향 파일: `crates/tasty-terminal/src/modes.rs`(`set_dec_mode` catch-all)
- 관련: ADR-0008(범위 밖 보류 계열), CLAUDE.md "크로스 플랫폼" 원칙(Win32InputMode 관련)
