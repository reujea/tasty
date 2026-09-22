# ADR-0460: 셀 강조색의 alpha 는 CPU 에서 그 셀 배경 위에 합성한다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: renderer, gpu, theme, search, selection, alpha, blending, compatibility
- **Group**: ui-theme-gallery

## Context

테마의 `[terminal]` 강조색은 8 자리 hex 로 alpha 를 가질 수 있고, 빌트인 두 테마는
검색 강조에 그것을 쓴다(`search_match_bg = "#f9e2af4d"` · `search_match_active_bg =
"#f9e2afb3"`, latte 도 같은 형태). 문서(`docs/design/systems/theme.md`, 사용자 가이드의
테마 페이지)도 "마지막 두 자리가 투명도" 라고 적는다.

그런데 화면에서는 alpha 가 버려지고 있었다. 셀 렌더러는 셀 하나를 bg quad 하나로 그리고,
강조는 별도 quad 가 아니라 **그 셀의 bg 색을 강조색으로 바꾸는** 방식이다(선택 → vi 커서
→ 링크 → 검색 순으로 뒤엣것이 앞엣것을 덮는다). 그리고 bg 파이프라인의 블렌드 상태는
`BlendState::REPLACE` 다. 그래서 강조색의 RGB 가 그대로 쓰이고 alpha 는 프레임버퍼
alpha 채널에만 들어갔다 — 창이 불투명 합성이므로 보이지 않는다. 결과로 active 매치와
inactive 매치가 **같은 불투명 `#f9e2af`** 로 칠해져 구분이 사라졌고, 밝은 노랑 위에서
기본 전경색 글자가 거의 안 읽혔다. 실측(mocha, 셀 배경 `#000000`): 두 매치 모두
`#f9e2af`. 이 형태는 셀 격자 렌더링 도입 때부터 있었다.

bg 파이프라인에 들어가는 색을 전수로 세면 다음과 같다(`src/gfx/renderer.rs` ·
`src/gfx/renderer/line_render.rs`).

| 출처 | 값 | 빌트인 테마의 alpha |
|---|---|---|
| 셀 기본 bg · gutter · 빈 줄 · 행 끝 빈 칸 | `surfaces.terminal.focused_bg` / `unfocused_bg` (DECSCNM 이면 fg 와 교환) | 1 |
| 셀 속성 bg (SGR · 256 색 · truecolor · 반전) | `compute_cell_colors` | 1 (truecolor 는 termwiz 가 준 값) |
| Block 커서 | 셀 fg 와 교환 | 1 |
| 선택 | `terminal.selection_bg` | 1 |
| vi copy 커서 | `terminal.vi_cursor_bg` | 1 |
| 링크 hover | 호출부가 만든 `LinkHighlight.bg` | 1 |
| 검색 inactive / active | `terminal.search_match_bg` / `search_match_active_bg` | **0x4d / 0xb3** |
| IME preedit 상자 | `accent_primary()` (별도 quad) | 1 |

빌트인 테마에서 alpha < 1 인 값은 검색 둘뿐이다. 사용자 테마(`~/.tasty/themes/*.toml`)와
`theme_overrides` 는 어느 키에든 8 자리를 쓸 수 있다.

## Decision

강조(선택 · vi 커서 · 링크 · 검색)는 **CPU 에서 그 셀의 현재 bg 위에 source-over 로
합성한 색**을 셀 bg 로 쓴다(`src/gfx/renderer/overlay.rs` 의 `composite_over`). 적용
순서는 종전과 같다 — 선택 위에 vi 커서, 그 위에 링크, 그 위에 검색이 차례로 얹힌다.
bg 파이프라인은 `REPLACE` 로 둔다.

- 불투명 강조색은 합성 결과가 강조색 그대로다(비트 단위로 같다). 그래서 빌트인 테마에서
  화면이 바뀌는 자리는 검색 강조 하나다.
- 셀 기본 bg · 셀 속성 bg · Block 커서는 **합성 대상이 아니다** — 종전대로 그 값을 그대로
  쓴다. 반투명 강조가 셀 기본 bg 를 반투명하게 만들지 않는다: 합성 결과의 alpha 는
  `ta + ba·(1−ta)` 라 셀 bg 가 불투명이면 결과도 불투명이다.
- IME preedit 상자는 이 결정 밖이다. 셀 quad 와 별개로 얹는 quad 이고, 그 아래 셀 색을
  그 자리에서 모른다. 빌트인 테마에서 불투명이라 보이는 차이가 없다.

## Consequences

- **얻은 것**: 테마·문서가 약속한 검색 강조 alpha 가 화면에 반영된다. mocha 에서
  inactive `#4b4435` · active `#af9f7b`(검은 셀 bg 위, 실측)로 둘이 갈린다. 강조 아래
  셀 색(선택된 칸 위의 검색 매치 등)이 비쳐 보인다.
- **잃은 것**: 사용자 테마가 선택 · vi 커서 · 링크 강조색에 alpha < 1 을 써 두었다면
  그 강조가 종전과 달리 반투명하게 보인다. 이는 테마 형식이 약속한 동작으로 돌아가는
  것이다. 검색 강조 위 글자는 종전의 밝은 노랑보다 어두운 배경 위에 놓인다(가독성은
  나아진다).
- **운영 비용 / 유지 부담**: 강조를 적용하는 자리가 두 곳(`fill_surface` 와
  `render_cell`)이라 합성 호출도 두 곳에 있다. 새 강조를 더할 때 두 곳 모두
  `composite_over` 를 거쳐야 한다. 강조 셀마다 부동소수 연산 몇 번이 더해진다(불투명이면
  바로 반환).

## Alternatives Considered

- **bg 파이프라인을 `ALPHA_BLENDING` 으로 바꾼다** — 셀 quad 가 하나뿐이라 반투명 강조가
  **셀 bg 가 아니라 그 아래 clear 색**(`bg_panel`)과 섞인다. 색 있는 셀(SGR bg) 위 매치가
  틀린 색이 되고, 셀 기본 bg 에 alpha 를 준 테마는 clear 색과 섞여 종전 모습이 바뀐다.
  강조를 별도 quad 로 쪼개 두 번 그리면 맞출 수 있지만 강조 셀의 인스턴스가 두 배가 되고
  wide 셀 · 행 끝 경로를 모두 고쳐야 한다. 외부 동작 변화가 더 크다.
- **검색 강조만 합성한다** — 같은 테마 형식이 다른 강조 키에서는 계속 alpha 를 버린다.
  불투명 값에서는 결과가 같으므로 범위를 좁혀 얻는 호환이 없다.
- **테마 값을 불투명으로 바꾼다** — 디자인 값(색)을 로컬에서 정하는 일이고, 사용자 테마의
  8 자리 값은 여전히 버려진다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 셀 강조가 셀 bg 교체가 아니라 별도 인스턴스로 그려지게 바뀐다 — 그때는 GPU 블렌딩이
  같은 결과를 더 싸게 낸다. 재는 법: `src/gfx/renderer.rs` 에서 강조 판정 뒤
  `bg_instances.push` 가 셀당 한 번인지 본다.
- 창 투명도가 terminal 셀 영역까지 닿게 바뀐다(surface 를 투명 합성으로 만들고 셀 bg 가
  `background_opacity` 를 따른다) — 합성 결과 alpha 가 화면에 나타나므로 이 공식을 다시
  본다. 재는 법: `src/gfx/gpu.rs` 의 `alpha_mode` 선택과 `render_clear_pass` 외에
  `background_opacity` 를 읽는 자리가 늘었는지 grep.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 검색 강조 위 글자의 가독성이 떨어진다는 사용자 보고. 재는 법: 해당 테마로 검색 강조
  캡처(`screenshot --window`)에서 매치 칸의 bg 와 글자 색 대비를 잰다.

## References

- 코드(결정이 실현된 현재 위치): `src/gfx/renderer/overlay.rs` 의 `composite_over`,
  `src/gfx/renderer.rs` 의 `fill_surface`, `src/gfx/renderer/line_render.rs` 의
  `render_cell`, `src/gfx/renderer/pipeline.rs` 의 bg 파이프라인.
- `docs/dev-guide/gpu-rendering.md`, `docs/design/systems/theme.md`
- 렌더러 입력 경계: [ADR-0342](0342-the-cell-renderer-names-the-crates-not-the-host-re-exports.md)
