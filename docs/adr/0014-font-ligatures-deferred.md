# ADR-0014: 폰트 ligature 는 보류 (현재 미지원, 추후 지원 계획 있음)

- **Status**: Deferred
- **Date**: 2026-06-18
- **Tags**: font, ligatures, appearance, settings, rendering, cell-grid, scope, deferred
- **Group**: terminal-input

## Context

디자인은 `Appearance › General` 폰트 설정 목록에 **ligatures** 항목(프로그래밍 합자 토글)을
포함한다. 즉 디자인 의도상 폰트 설정에 ligatures on/off 가 있다.

그러나 소스의 `FontSettings`(`crates/tasty-settings/src/appearance.rs`)는
`font_family / font_size / custom_font_path / line_height / font_scale_mode` 만 갖고
**ligatures 필드 자체가 없다.** 폰트 설정 이전 작업(General 로 이동)도 기존 필드만 옮긴다.

판단에 영향을 준 사실:

- tasty 터미널은 **셀-격자(cell-grid) 렌더**다. 프로그래밍 합자(`=>`, `!=`, `->` 등)는
  **인접 셀을 가로지르는 cross-cell shaping** 이 필요하다 — 두 칸 이상을 한 글리프로 묶고,
  커서가 그 위에 오거나 선택/색 경계가 합자 중간을 지날 때는 다시 **합자를 해제**해 칸 단위로
  되돌려야 한다. 자유 흐름 텍스트의 합자보다 구현·유지 부담이 크다.
- shaping 라이브러리(`cosmic-text 0.18`, `crates/tasty-font`)는 합자 자체는 처리할 수 있으나,
  현재 터미널 글리프 경로는 셀 단위로 글리프를 배치할 뿐 cross-cell 합자를 적용하지 않는다.
- 주 용도(AI 코딩 에이전트용 터미널)에서 합자의 가치 대비 비용이 낮다 — 지금 들일 만한 우선순위가
  아니라는 것이 사용자 확정 판단이다.

참고로 번들 폰트 이름이 "D2Coding **ligature**"(`crates/tasty-font`, `docs/features/terminal`)
인 것은 **폰트 파일이 합자 글리프를 포함**한다는 뜻이며, 본 ADR 이 보류하는
**사용자 설정 가능한 ligatures 토글/적용** 과는 별개다(폰트 자원 ≠ 설정 기능).

## Decision

**보류(Deferred)한다.** 폰트 ligature 는 **현재 미지원**이며, `Appearance › General` 폰트
설정에 ligatures 토글을 추가하지 않는다. `FontSettings` 에 ligatures 필드도 넣지 않는다.

단 "영구 비지원(rejected)"으로 못박지 않는다 — 우선순위가 낮을 뿐 거부가 아니다. 미래에 지원할
의향이 있으며, 아래 재검토 트리거가 충족되면 착수한다.

## Consequences

- **얻은 것**: cross-cell shaping 이라는 큰 렌더링 작업을 효용이 확인될 때까지 미룬다. 코어
  터미널/에이전트 기능에 자원을 집중한다. 디자인(있음)과 소스(없음)의 불일치를 본 ADR 이 명시적
  근거로 흡수해, "왜 디자인엔 ligatures 가 있는데 설정에 없나" 라는 혼선을 막는다.
- **잃은 것**: 디자인 #6 의 ligatures 항목이 당장 구현되지 않는다. 합자 글꼴을 쓰는 사용자는
  프로그래밍 합자(`=>` 등)가 칸 단위 그대로 렌더된다.
- **운영 비용 / 유지 부담**: 없음(필드/UI/렌더 추가 없음). 디자인의 ligatures 항목은 본 ADR 로
  **미구현(deferred)** 상태임을 명시한다.

## Alternatives Considered

- **지금 풀 지원(cell-grid cross-cell 합자 + 해제)**: 인접 셀 묶기 + 커서/선택/색 경계 시 합자
  해제까지 필요해 구현·유지 비용이 크다. 주 용도 효용 대비 우선순위 부적합.
- **markdown/explorer 같은 자유 텍스트 surface 에만 부분 적용**: 셀-격자가 아닌 surface 는
  cross-cell 제약이 없어 비용이 낮다. 다만 "터미널은 안 되는데 일부 surface 만 된다" 는 부분
  지원은 사용자 멘탈 모델을 복잡하게 만든다 — 현재는 전체 미지원으로 단순화해 함께 보류한다.
- **명시적 비지원(rejected) 선언**: 못박으면 향후 수요 시 결정을 번복(새 ADR supersede)해야
  한다. 낮은 우선순위가 "영구 거부"까지 정당화하지 않으므로 과하다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 터미널 글리프 경로가 **cross-cell shaping**(인접 셀을 가로지르는 합자 + 경계 시 해제)을
  지원하게 되어 합자 적용 비용이 크게 낮아질 때.
- 합자 토글에 대한 **실제 사용자 수요**가 확인될 때.
- 지원하기로 하면: `FontSettings` 에 ligatures 필드 추가 + `tab_bar`/surface 렌더 소비 +
  i18n(`lang/{en,ko,ja}.toml`) 키 추가 + `Appearance › General` UI 노출까지 함께 진행한다.

## References

- 디자인: `Appearance › General` 폰트 설정 목록의 ligatures 항목 (본 ADR 로 deferred)
- 관련 소스: `crates/tasty-settings/src/appearance.rs`(`FontSettings` — ligatures 필드 없음),
  `crates/tasty-font`(cosmic-text shaping, 번들 "D2Coding ligature" 폰트)
- 관련 ADR: ADR-0008(인라인 그래픽 보류 — 같은 "deferred" 포맷 선례)
