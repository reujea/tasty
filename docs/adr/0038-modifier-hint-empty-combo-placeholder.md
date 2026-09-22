# ADR-0038: modifier-hint 빈 조합 섹션은 "바인딩 없음" 플레이스홀더로 표시한다

- **Status**: Accepted
- **Date**: 2026-07-06
- **Tags**: modifier-hint, overlay, keybindings, empty-state, placeholder, design-token, i18n, accessibility, debug-ipc, adr-0035, adr-0510

## Context

modifier-hint 오버레이는 modifier 를 홀드하면 "그 조합을 포함하는 조합의 단축키·역할"을 섹션 목록으로 보여준다([ADR-0035](0035-modifier-hint-combo-narrowing-and-shift-delay.md)). 초기 설계(디자인 SoT `2026-07-02` 결정 #1)는 **바인딩·역할이 모두 없는 조합 섹션을 통째로 생략**했다 — 콘텐츠 모델 `build_hint_sections()` 끝에서 `sections.retain(|s| !s.is_empty())` 로 빈 섹션을 버렸다.

이 생략 설계는 한 가지 사각을 낳았다. 눌린 조합의 상위집합이 **전부 미할당**이면(예: `Ctrl+Alt+Shift` 홀드) 모든 섹션이 비어 `retain` 후 목록이 완전히 비고, draw 는 `if sections.is_empty() { return }` 로 **오버레이 자체를 안 그린다**. 사용자에겐 "홀드해도 아무 반응 없음" 으로 읽혀, 오버레이가 꺼졌는지·고장났는지·정말 미할당인지 구분할 수 없다.

동시에 CLAUDE.md UI 규칙(색·치수는 Theme 토큰, raw px 금지)과 i18n 규칙(문자열은 `t()` 키)이 걸려 있어, 플레이스홀더를 넣더라도 새 색/치수/문구를 토큰·키로 노출해야 했다.

## Decision

**빈 조합 섹션을 생략하지 않고 유지하며, ChordHead 아래에 muted "바인딩 없음" 플레이스홀더 한 줄을 그린다.** `build_hint_sections()` 의 빈 섹션 `retain` 을 제거해 빈 `HintSection` 이 살아남게 하고(`is_empty()` 는 draw 의 빈-판정용으로 pub 유지), 오버레이 `draw_section()` 은 `rows`·`roles` 가 모두 비면 `draw_empty_row()` 로 플레이스홀더를 렌더한다. 미할당 조합을 홀드하면 이제 패널이 정상적으로 뜨고 "여기 바인딩 없음" 을 명시한다. 빈 섹션은 **항상** 표시되며 채워진 섹션과 나란히도 뜬다(예: `Ctrl` 홀드 → 바인딩 있는 `Ctrl`/`Ctrl+Shift` 옆에 빈 `Ctrl+Alt`/`Ctrl+Alt+Shift` 가 플레이스홀더로). 항상 표시되는 빈 섹션의 노이즈는 수용하되, 밀도로 완화한다.

**플레이스홀더는 리스트에서 가장 조용한 행이다 — 부재 신호이지 역할 행이 아니다.** 텍스트만(우측 키캡 없음 — 키캡은 있지도 않은 바인딩을 암시), 톤은 신규 토큰 `--tasty-modhint-empty-fg → text-muted`(키캡 행 `text-secondary` 보다 한 단계 절제), 배경 wash 없음(wash 는 role-row = "이 조합이 무언가를 한다"로 오독), leading 글리프 없음(dash/아이콘은 리스트 불릿으로 읽혀 노이즈). 빈 섹션의 내부 간격은 3px(채워진 6px 보다 좁게), 행 최소 높이 20px(키캡 행 24px 보다 타이트)로 잡아 항상 표시되는 빈 섹션이 리스트를 늘어뜨리지 않게 한다. 섹션 간 간격(`--tasty-modhint-section-gap`)은 불변. 문구는 `modifier_hint.empty` i18n 키 — en `"No shortcuts bound"` · ko `"지정된 단축키 없음"` · ja `"割り当てなし"`(220px 폭에서 세 언어 모두 한 줄). 정적·비상호작용(호버·포커스·클릭 없음 — 오버레이 전 행 공통).

**신규 토큰은 색 1개 + 치수 2개.** 색 `modhint_empty_fg()`(→ `text_muted`)는 디자인 SoT `tokens/components.css` 의 `--tasty-modhint-empty-fg` 미러다. 치수(3px 간격·20px 최소높이)는 디자인이 시안에서 인라인 px(`.mh-section--empty{gap:3px}`, `.mh-empty{min-height:20px}`)로만 둔 값을 코드에서 토큰화한 것 — raw px 하드코딩 금지 정책상 `modhint_empty_row_gap()`·`modhint_empty_row_min_height()` 슬롯을 손추가한다. modhint 계열은 codegen(dtcg) 밖의 손작성 토큰이라 `theme.rs` 직접 추가가 맞다.

**검증은 debug 격리 IPC + 단위테스트로 한다.** `debug.modifier_hint.state` 덤프의 각 섹션에 `empty` 플래그를 노출해 스크린샷 없이 빈-플레이스홀더 표시를 자동 단정한다(`empty_sections_are_retained` · `mixed_hold_keeps_filled_and_empty_sections` · `consumes_build_hint_sections`). `visible` 은 draw 의 방어 가드와 동일 조건이며, 빈 섹션이 유지되므로 미할당 조합 홀드에서도 `visible:true` 가 된다.

## Consequences

- **얻은 것**:
  - 미할당 조합을 홀드해도 패널이 뜨고 "바인딩 없음" 을 명시 — "반응 없음/고장" 과 "정말 미할당" 이 구분된다.
  - 플레이스홀더가 실제 행보다 조용하게(muted·wash 없음·키캡 없음) 스타일링돼 실제 바인딩·역할 행과 경쟁하지 않는다.
  - `build_hint_sections()` 는 순수 함수라 빈 섹션 유지·혼재 공존을 단위테스트(`empty_sections_are_retained` · `mixed_hold_keeps_filled_and_empty_sections`)로 고정하고, `debug.modifier_hint.state` 의 `empty` 플래그로 렌더 픽셀 없이 자동 단정한다.
- **잃은 것**:
  - 항상 표시되는 빈 섹션이 리스트에 노이즈를 더한다(비-macOS 최대 2개·macOS 최대 6개). 3px 타이트 간격·20px 행높이로 밀도를 완화했으나 완전 제거는 아니다(의도적 트레이드).
  - 신규 치수 토큰 2개는 디자인이 인라인 px 로만 둔 값을 코드에서 토큰화한 것이라, 디자인 `components.css` 에는 대응 치수 토큰 미러가 없다(색 토큰만 미러 존재).
- **운영 비용 / 유지 부담**:
  - blast radius 는 콘텐츠 모델(`modifier_hint.rs`, `retain` 제거) + 오버레이(`modifier_hint_overlay.rs`, `draw_empty_row`/debug 플래그) + 토큰(`theme.rs`) + i18n 3파일 + 갤러리 specimen + 문서. draw 게이트(`sections.is_empty()`)는 방어 가드로만 남는다(실경로에선 홀드 조합 자신의 섹션이 항상 남아 비지 않음).
  - debug IPC 표면 증가 없음(`empty` 필드 추가뿐, release 미컴파일).

## Alternatives Considered

- **빈 섹션 생략 유지(현행)**: 미할당 조합 홀드 시 "반응 없음" 사각이 남는다 — 본 ADR 이 반전하는 대상.
- **플레이스홀더를 role-row 처럼 washed 배경 + 글리프로**: "이 조합이 무언가를 한다"로 오독된다. 부재는 조용할수록 좋아 채택 안 함(디자인 §6-3/§6-4).
- **text-secondary 톤(실제 행과 동일)**: 실제 항목과 무게가 같아 "부재" 로 안 읽힌다. 한 단계 절제된 text-muted 채택(디자인 §6-2).
- **긴 문구("No shortcuts assigned to this combination")**: 220px 폭에서 3줄 wrap 으로 빈 섹션을 늘어뜨린다. 짧은 "No shortcuts bound" 채택(디자인 §6-1).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 사용자 피드백에서 "항상 표시되는 빈 섹션이 목록을 너무 시끄럽게 한다" 는 불만이 유의미하게 반복될 때(예: 빈 섹션을 접거나 옵션으로 끄는 절충 검토).
- plugin 단축키 wiring 완료로 조합별 바인딩 밀도가 크게 바뀌어 "미할당 조합" 의 빈도 자체가 달라질 때.
- 디자인 SoT 가 플레이스홀더 톤/밀도/문구 결정을 다시 바꿀 때.

## References

- [ADR-0035](0035-modifier-hint-combo-narrowing-and-shift-delay.md) — modifier-hint 조합 좁힘 + Shift 지연(이 오버레이의 직전 결정).
- [docs/features/accessibility/index.md](../features/accessibility/index.md) — Modifier key hints 기능 서술.
- [docs/design/systems/design-token-mapping.md](../design/systems/design-token-mapping.md) — modhint Tier-3 토큰 매핑(신규 empty-fg/empty-row-gap/empty-row-min-height 포함).
- [docs/dev-guide/debug-ipc.md](../dev-guide/debug-ipc.md) — `debug.modifier_hint.state` 덤프(`empty` 플래그).
