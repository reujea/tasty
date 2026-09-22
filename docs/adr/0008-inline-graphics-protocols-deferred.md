# ADR-0008: 인라인 그래픽 프로토콜(Sixel / Kitty / iTerm)은 보류

- **Status**: Deferred
- **Date**: 2026-06-17
- **Tags**: terminal, graphics, sixel, kitty, image, vte, scope, deferred
- **Group**: terminal-input

## Context

termwiz 파서는 인라인 래스터 이미지 프로토콜을 파싱해 별도 `Action`/OSC 로 내보낸다:
`Action::Sixel`, `Action::KittyImage`, 그리고 OSC 1337 `OperatingSystemCommand::ITermProprietary`.
tasty 는 현재 이 셋을 **모두 드롭**한다 — `action_to_changes`(`crates/tasty-terminal/src/vte_handler.rs`)
의 `_ => vec![]`, `map_osc`(`crates/tasty-terminal/src/vte_handler/osc.rs`)의 `_ => {}`.

VTE 커버리지 감사 중 "이걸 지원해야 하는가"가 제기됐다. 세 프로토콜은 인코딩만 다를 뿐
**터미널 셀 그리드 위에 비트맵 이미지를 그리는** 같은 목적이다(Sixel=DEC DCS 비트맵,
Kitty=APC+base64 PNG/RGBA, iTerm=OSC 1337 base64 파일).

판단에 영향을 준 사실:

- 구현 비용이 크다 — 이미지 디코드 + GPU 텍스처화 + 셀 그리드 위 위치 배치 + 스크롤/리사이즈/
  스크롤백·placement 생명주기 관리까지 필요하다(단순 핸들러 추가가 아님).
- tasty 의 주 용도(AI 코딩 에이전트용 터미널)에서 이 프로토콜은 **거의 쓰이지 않는다.**
  코딩 에이전트(Claude Code 등) 자신은 이 프로토콜을 전혀 사용하지 않고(텍스트/마크다운만 출력),
  실사용처는 이미지 뷰어(`imgcat`/`icat`/`viu`)·파일매니저 썸네일·일부 플로팅 도구로 좁다.
- `docs/` 에 그래픽 지원 정책이 없어 "의도적 비지원"으로 단정할 근거도 없다.

## Decision

**보류(Deferred)한다.** 당장 구현하지 않고 현행 드롭을 유지하되, "비지원(rejected)"으로
못박지 않는다. 우선순위가 낮을 뿐 거부가 아니다 — 다른 우선 작업이 소진되어 여력이 생기거나
실제 수요가 확인되면 구현을 검토한다. 구현하게 될 경우 표현력이 가장 높고 현대 터미널에서
세를 넓히는 **Kitty 그래픽 프로토콜을 1순위 후보**로 본다.

같은 맥락의 능력 질의 응답(Sixel `XTSMGRAPHICS`, Kitty `QueryKittySupport`)도 함께 보류한다 —
이미지 렌더 없이 "미지원" 회신만 하는 선택지는 비용은 작지만, 정작 그 도구들을 쓰지 않는
환경에서는 질의 자체가 오지 않아 효용이 미미하므로 지금 따로 구현하지 않는다.

## Consequences

- **얻은 것**: 큰 렌더링 작업을 주 용도에 효용이 확인될 때까지 미룬다. 코어 터미널/에이전트
  기능에 자원을 집중한다. "비지원 못박기"가 아니라 열린 상태라, 수요가 생기면 ADR 교체 없이
  Status 전환만으로 착수할 수 있다.
- **잃은 것**: 이미지 뷰어/플로팅 도구를 tasty 터미널에서 쓰면 이미지가 안 뜨고 원시 시퀀스가
  소리 없이 사라진다(앱에 따라 깨진 텍스트가 보일 수 있음). 능력 질의 무응답으로 일부 앱의
  폴백이 살짝 느려질 수 있으나 실측 영향은 낮음.
- **운영 비용 / 유지 부담**: 없음(현행 드롭 유지). 단, 이 보류 결정을 모르는 사람이 "이미지가
  안 보인다"를 버그로 오인할 수 있어 본 ADR 이 그 근거가 된다.

## Alternatives Considered

- **명시적 비지원(rejected) 선언**: 못박으면 향후 수요가 생겼을 때 결정을 번복(새 ADR supersede)
  해야 한다. 비지원으로 단정할 근거(낮은 빈도)는 "영구 거부"까지 정당화하지 않으므로 과하다.
- **지금 풀 지원**: 주 용도 효용 대비 렌더링 구현 비용이 크다 — 우선순위 부적합.
- **렌더 없이 "응답만" 처리**: 능력 질의에 미지원 회신만 추가. 비용은 작으나, 이 도구들을
  안 쓰는 환경에선 질의가 오지 않아 효용이 거의 없어 지금은 제외(보류에 포함).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- tasty 터미널에서 이미지/플롯/썸네일을 보고 싶다는 실제 사용 요구가 확인될 때.
- 코어/에이전트 우선 작업이 소진되어 그래픽에 투자할 여력이 생길 때.
- Kitty 그래픽 프로토콜이 사실상 표준이 되어 미지원이 호환성 결함으로 체감될 때.

## References

- 영향 파일: `crates/tasty-terminal/src/vte_handler.rs`(최상위 `Action` 드롭),
  `crates/tasty-terminal/src/vte_handler/osc.rs`(`ITermProprietary` 드롭)
- 관련: ADR-0002(VTE 파싱 구조)
