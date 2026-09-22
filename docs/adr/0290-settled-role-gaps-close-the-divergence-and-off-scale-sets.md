# ADR-0290: 2026-09-17 결정이 divergence alias 집합과 스케일 밖 값 집합을 닫았다

- **Status**: Accepted
- **Date**: 2026-09-19
- **Tags**: design-tokens, color, semantic, role, font-size, opacity, status-dot, theme, adr-0033, adr-0126
- **Group**: ui-theme-gallery

## Context

두 선행 ADR 이 같은 형태의 **미정 집합**을 남겼다.

- [ADR-0033](0033-ui-color-semantic-role-only.md) 은 "대응 role 이 없는 use 는 primitive 로
  돌아가지 않고 가장 가까운 role 로 alias + `// divergence:` 주석" 으로 정했다. 그 주석은
  "미래 design-request 후보" 표식이었고, 해소는 디자인 판단에 걸려 있었다.
- [ADR-0126](0126-off-scale-font-values-are-not-snapped-to-tokens.md) 은 어느 tier 에도
  없는 값(폰트 `.5` · 점 지름 · 반경)을 토큰으로 스냅하지 않고 사유를 적은 명명 const 로
  두기로 정했다. 어느 토큰으로 모을지 역시 디자인 판단이었다.

둘 다 **결정을 미룬 것이 아니라 결정권이 이쪽에 없다는 것을 기록한 것**이다. 그 판단이
2026-09-17 에 도착했다 — 색 role 공백 7 건(C1~C7) · 스케일 밖 폰트 8 건(T1~T8) · 흩어진
tinted 채움/테두리 계수 5 짝(P1~P5) · 인셋 1 건(I1) · 이름 없는 치수 7 건(D1~D7), 그리고
"값-보존을 위해 canonical 을 안 부르던" 세 자리(탭 hover 채움 · 탭 구분선 · toast 보더)가
의도된 시각 변화라는 확인.

## Decision

**미정 집합을 닫고, 그 결과로 생긴 규칙을 role 선택의 기준으로 삼는다.** 값이 같다는
것은 role 이 같다는 뜻이 아니므로, 새로 열린 role 은 값이 아니라 의미로 고른다.

- **잉크**: disabled 는 고유 잉크다 — 모든 disabled 라벨·글리프가 `text-disabled`(neutral-700)를
  읽는다. `glyph-dim`(neutral-600)은 *물러나야 하는 chrome* 전용이고 disabled 용이 아니다.
  `border-frame`(neutral-500)은 틀의 선이고 `surface-active` 와 값만 같다. **그 선은
  넷이다** — popup 프레임 · titlebar 아래 선 · pane divider · GPU 비활성 보더. popup
  **내부** 구분선은 이 role 이 아니라 `border-strong`(neutral-400)에 남는다: 틀과 칸막이는
  같은 회색 계열이지만 다른 역할이고, 둘을 한 role 로 묶으면 틀만 올리는 것이 불가능해진다.
  단계는 **명도가 아니라 대비로** 골랐다 — latte 에서는 neutral-500 이 neutral-400 보다
  어둡기 때문에 "한 단계 올린다" 를 명도로 적으면 두 테마 중 하나에서 반드시 틀린다.
  `accent-decorative`(peach)는 장식이고 `accent-attention` 과 값만 같다.
- **폰트**: 읽는 글은 위로, 숫자 micro 라벨은 아래로 스냅한다. 브랜드 30 은 스냅하지 않고
  이름을 얻었다(`font-size-brand-display`) — UI 14px 상한의 브랜딩 예외는 워드마크 17 과
  이것 **둘이 마지막**이다. 글리프 크기(clipboard 이미지 · spinner · toggle 체크)는 폰트
  스케일이 아니라 **아이콘 가족**으로 판정한다.
- **tint**: accent 를 옅게 깔고 같은 accent 로 두르는 관용구는 **채움 0.12 / 테두리 0.36**
  한 짝만 쓴다. 승인된 부분 사용 둘(채움만 · 테두리만)도 같은 계수를 쓴다.
- **점 가족**: 일반 8 · 24px 크롬 안 compact 6(`status-dot-size-compact`) · 활성 탭 마커 4
  (위치 표시라 다른 role). attached ring 은 굵기 2 + offset 2 이고 offset 은 **점 바깥
  edge → ring 안쪽 edge** 로 잰다.

**두 선행 ADR 의 결정 자체는 바뀌지 않는다.** alias + `divergence:` 규칙도, 스케일 밖 값을
스냅하지 않는 규칙도 그대로 유효하다 — 이번에 닫힌 것은 **그 규칙이 가리키던 미정 집합의
원소들**이지 규칙이 아니다. 그래서 Supersede 도 부분 개정도 아니다.

## Consequences

- **얻은 것**: `divergence:` 주석이 소스에서 **전부** 사라졌다 — 마지막 한 자리였던 상태바
  테마 표시는 색 점을 버리고 `sun`/`theme` 글리프를 `statusbar-theme-glyph` 로 그린다.
  네 갈래로 흩어져 있던 tint 계수가 한 짝으로 모였다. 점 지름이 "같은
  수로 수렴하는 이름 없는 역할" 이 아니라 이름 있는 세 role 이 됐다.
- **잃은 것**: C1 · C3/C4/C6 · `.5` 폰트 · D1/D3/D4/D5 는 **픽셀이 바뀐다.** 값-보존
  리팩터가 아니므로 화면별로 시각 확인이 필요하다.
- **C1 이 이 열거에 늦게 들어왔다.** 처음에는 픽셀이 안 바뀌는 쪽으로 적혀 있었고, 그것이
  틀렸다. 네 자리 중 **둘만** 값이 그대로다 — pane divider 와 GPU 비활성 보더는 이미
  `surface-active`(neutral-500)를 읽고 있어 role 이름만 바뀌었다. 나머지 둘, popup 프레임과
  titlebar 아래 선은 `border-strong`(neutral-400)에서 올라와 **실제로 한 단계 밝아진다**
  (latte 에서는 어두워진다 — 위 Decision 의 대비 기준). 패널 위 대비로 재면 1.80 → 2.46
  (mocha) · 1.61 → 1.91 (latte) 이고, 그 값은
  `crates/tasty-design-tokens/tests/color_drift.rs` 의
  `the_frame_line_outranks_the_strong_border_by_contrast_not_lightness` 가 고정한다.
- **운영 비용 / 유지 부담**: 새 role 넷(`border-frame` · `glyph-dim` · `accent-decorative` ·
  tint 짝)은 값이 기존 role 과 겹치므로, "값이 같으니 아무거나" 로 되돌아가는 것을 막는
  것은 문서와 리뷰다 — 값이 같은 두 role 을 갈라 읽는 가드는 원리적으로 세울 수 없다.
  **그 한계는 짝마다 따로 판정한다.** `border-frame` ↔ `surface-active` 는 값이 같아
  여전히 그렇지만, `border-frame` ↔ `border-strong` 은 값이 갈려 있어 가드를 세울 수 있고
  세웠다(바로 위 시험). 즉 "가드 불가" 는 이 결정 전체의 성질이 아니라 값이 겹치는 짝의
  성질이다.

## Alternatives Considered

- **A: divergence 주석만 지우고 alias 를 유지** — 안 고른 이유: 주석이 표식이었지 문제가
  아니었다. 표식만 지우면 다음 사람이 그 자리를 "이미 맞는 role" 로 읽는다.
- **B: 값이 같은 role 을 새로 열지 않고 기존 role 을 재사용** — 안 고른 이유: 디자인이
  "동일 숫자의 무관한 토큰에 묶지 않는다(값이 같다 ≠ 역할이 같다)" 를 명시적으로 요구했다.
  값이 갈리는 날 호출처를 다시 찾아야 하는 비용이 role 하나의 비용보다 크다.
- **C: 계수 짝을 역할별 component 토큰으로 쪼갠다** — 안 고른 이유: 디자인이 한 짝으로
  수렴시키고 부분 사용 둘만 승인했다. 역할별로 쪼개면 네 갈래가 이름만 바꿔 남는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- vendor 된 DTCG export 에서 `semantic.tint-fill-alpha` 또는 `semantic.tint-border-alpha` 의
  값이 바뀐다 — `crates/tasty-design-tokens/tests/sizing_parity.rs::tint_alphas_match_tokens`
  가 소스 사본과의 불일치로 그 순간 빨개진다.
- `component.status-dot-size-compact` 가 사라지거나 `tab-dot-size` 가 다시 일반 8 을
  가리킨다 — freshness 재생성이 접근자 본문을 바꾸므로 `committed_generated_files_are_fresh`
  가 잡는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- Latte 에서 C3/C4 의 disabled 대비가 4.5:1 목표에 못 미친다는 보고가 나온다.
  재는 법: Mocha/Latte 격리 인스턴스에서 해당 화면을 띄워 `text-disabled` 와 배경의
  대비를 잰다(`docs/ai-verification/visual-verification.md`).
- 디자인이 `.5` 스케일을 정식 tier 로 승인한다. 재는 법: 디자인 changelog 의 결정표를
  읽는다 — 레포에는 그 사실을 실어 나르는 좌변이 없다.

## References

- [ADR-0033](0033-ui-color-semantic-role-only.md) — alias + `divergence:` 규칙(그대로 유효)
- [ADR-0126](0126-off-scale-font-values-are-not-snapped-to-tokens.md) — 스케일 밖 값을 스냅하지 않는 규칙(그대로 유효)
- `docs/design/systems/theme.md` — 잉크 role · tint 짝 · 점 가족의 집행 요지
- `docs/design/systems/design-token-mapping.md` — 새 role 의 Theme 대응
- `docs/design/systems/token-crosswalk.md` — 값이 어긋나던 다섯 줄 중 넷이 닫힌 기록
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-type-appearance/src/semantic_color_generated.rs` 의 `border_frame`·`glyph_dim`·`accent_decorative`, `crates/tasty-type-appearance/src/theme.rs` 의 `TINT_FILL_ALPHA`·`TINT_BORDER_ALPHA`
