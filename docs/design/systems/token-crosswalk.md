# 토큰 크로스워크 (DTCG ↔ Rust Theme)

디자인 시스템의 DTCG 토큰과 Rust `Theme` 필드, 그리고 실제 `th.*`/`theme.*` 호출처를 잇는 매핑 참조. [theme.md](theme.md) 의 토큰 구조를 호출처 관점에서 보충한다.

> **vendor 상태**: DTCG 토큰 파일은 `crates/tasty-design-tokens/dtcg/tasty.tokens.json` 으로 **vendor 되어 있다** (832 토큰 = primitive 123 / semantic 143 / component 566 — 수는 `crates/tasty-design-tokens/tests/freshness.rs` 가 고정). 치수 계열은 `crates/tasty-design-tokens/src/generated/` 에 const 로 생성되고 freshness·정합·색 드리프트 테스트가 CI 에서 일치를 강제한다. vendor 갱신 절차는 `crates/tasty-design-tokens/README.md`. **component tier(치수+색)는 `&Theme` 접근자로 생성돼**(`tasty-type-appearance/src/generated_component.rs`, [theme.md](theme.md) "Component tier 접근자") `tasty-ui-widgets` 위젯과 host chrome(`src/adapters/ui/`)이 소비 중.

## 구조 모델

```
DTCG:  primitive ─▶ semantic ─▶ component ─▶ UI        (3-tier)
Rust:  ThemeColors(평면 primitive) ─▶ Theme(펼친 필드 + 도출 overlay + 생성 tier 접근자) ─▶ UI
                    ▲ 색 저장은 평면 primitive, tier 는 &Theme 접근자로 재구성
```

- **색 저장은 평면 primitive.** `ThemeColors`(`crates/tasty-type-appearance/src/theme.rs`)는 catppuccin 평면 primitive(neutral ramp 12 + `placeholder` + accent hue 13 + 터미널 색 4 + ansi 16 + `surface_themes` map)만 저장한다.
- **tier 는 `&Theme` 접근자로 노출된다**: semantic 색 중 단순 primitive alias(`accent_primary()`/`surface_raised()`/`border_default()` 등)는 **생성 접근자**(`semantic_color_generated.rs` — DTCG semantic 색 토큰에서 생성), is_light 분기·도출 overlay·합성·리터럴은 수기(`text_on_accent()`/`overlay_hover()`/`scrim()` 등). component tier(치수+색)는 생성 접근자(`generated_component.rs` — `button_primary_bg()`/`button_height_lg()` 등, [theme.md](theme.md) "Component tier 접근자"). 즉 저장은 평면이나 소비는 tier 를 경유한다.
- 아직 접근자로 못 옮긴 **primitive 직접 참조**(`th.<field>`)에서는 *"이 primitive 가 지금 어떤 의미(role)로 쓰이나"* 를 코드 호출처가 들고 있다 — 같은 필드가 여러 role 로 갈린다(아래 핫스팟).
- 반투명 의미색(`hover_overlay`/`active_overlay`/`separator`)만 `is_light` 에서 **도출**된다(`derive_overlays`), primitive 가 아니다.

## 다의성 핫스팟 (Rust 필드 → 겹치는 role)

한 primitive 필드가 여러 의미 role 을 겸한다. 의미 기반 접근자([theme.md](theme.md) "Semantic 접근자 우선")로 옮길 때, 호출처별로 어느 role 인지 가려야 하는 지점이다. (필드명은 `theme.rs`, 실제 색값은 `crates/tasty-themes/src/fallback.rs` 가 출처. 현재 호출처는 `rg '\bth\.<field>\b'` 로 확인.)

| Rust 필드 | 겹치는 role | 갈래 판단 포인트 |
|-----------|-------------|------------------|
| `blue` | accent-primary · border-focus · ansi-blue | selection·hyperlink=primary, focus ring stroke=border-focus, 터미널 팔레트=ansi |
| `yellow` | accent-warning · ansi-yellow · search-match | 경고=warning, 검색 하이라이트=search-match, 팔레트=ansi |
| `red` | accent-danger · ansi-red | error/danger 버튼=danger, 팔레트=ansi |
| `green` | accent-success · ansi-green | 성공 표시=success, 팔레트=ansi |
| `mauve` | accent-agent · ansi-magenta | 에이전트 강조=agent, 팔레트=ansi |
| `surface0` | surface-raised · border-default | 채움 배경 vs 1px 선 |
| `surface1` | surface-hover · border-strong · ansi-black | hover 배경 vs 강조 선 vs 팔레트 |
| `surface2` | surface-active · selection-bg | active 배경 vs 터미널 선택 (동일값) |
| `subtext0` | text-muted (+ caption 혼용) | muted 본문 vs 보조/caption 라벨 (최다 호출) |
| `overlay1` | text-disabled (+ recording 강조) | 비활성 텍스트 vs keybinding 녹화 강조 |
| `text` | text-primary · ansi-bright-white | UI 본문 vs 팔레트 |
| `subtext1` | text-secondary · ansi-white | 보조 텍스트 vs 팔레트 |

## ANSI 팔레트는 배열로 한 번에 전달

ANSI 16색은 개별 `th.*` 호출이 아니라 `theme.ansi_palette()` 배열로 GPU 렌더러(`src/gfx/gpu/render_pass.rs`)에 한 번에 넘어간다. 다수가 neutral/accent 필드와 **동일값**이지만 별도 필드다(`ansi_black`=`surface1`, `ansi_blue`=`blue` 등 — 위 핫스팟의 "ansi-*" role).

## surface kind 색은 `surface_themes` map

터미널/마크다운의 focused/unfocused × bg/fg 색은 `ThemeColors.surface_themes: BTreeMap<String, SurfaceTheme>` 에 들어가 `theme.surface("terminal")` / `theme.surface("markdown")` 헬퍼로 읽는다. `focused_bg` 만 black/white role-remap(light/dark).

## neutral ramp 12단 ↔ ThemeColors 필드

DTCG `primitive.color-neutral-*` 는 **elevation role 기준 넘버링**(0 = 최심 배경, 1100 = 최강 전경 — TOKENS.md)이며, catppuccin 평면 필드와 아래처럼 1:1 대응한다. 이 표가 색 드리프트 테스트(`crates/tasty-design-tokens/tests/color_drift.rs`)의 전거다. (`placeholder` 필드는 ramp 밖 — DTCG primitive 미대응.)

| DTCG primitive | ThemeColors 필드 | | DTCG primitive | ThemeColors 필드 |
|---|---|---|---|---|
| `color-neutral-0` | `crust` | | `color-neutral-600` | `overlay0` |
| `color-neutral-100` | `mantle` | | `color-neutral-700` | `overlay1` |
| `color-neutral-200` | `base` | | `color-neutral-800` | `overlay2` |
| `color-neutral-300` | `surface0` | | `color-neutral-900` | `subtext0` |
| `color-neutral-400` | `surface1` | | `color-neutral-1000` | `subtext1` |
| `color-neutral-500` | `surface2` | | `color-neutral-1100` | `text` |

accent hue 13종(`color-blue` … `color-rosewater`)은 동명 필드와 1:1 (테마당 hue 별 1값, ramp 없음).

## vendor 후 남은 것 (후속 시리즈)

vendor·치수 codegen·드리프트 테스트는 완료됐다 (`crates/tasty-design-tokens`). 남은 것:

- DTCG semantic **색** 토큰 ↔ Rust 접근자 전수표. 과거 이 문서가 "Rust 미대응"으로 꼽았던 것 중 `text-on-accent` → `Theme::text_on_accent()`, `radius-sm` → `SIZING.corner_radius_sm` 은 **이미 구현되어 있다** (stale 정정). `radius-pill`/`motion-*`(semantic — component 모션은 `banner_fade()` 등 생성 접근자로 있다)/`ui-scale-*`/`brand-*`(`brand-melon-flesh` → `brand_melon_flesh()` 만 있다) 등은 여전히 Theme 표면 부재 — 색은 시리즈 05, component 색 접근자는 시리즈 04 에서 결정.
- component tier(버튼/입력/탭/토스트…) ↔ 호출처 매핑, SIZING 소비처의 토큰 참조 전환 (시리즈 02).

### ★ 시리즈 02 착수 전 필독 — 그 전환에는 **픽셀 변경이 섞여 있다**

**이 시리즈는 "연결" 이 아니라 부분적으로 재디자인이다.** vendor 된 값과 그 자리가 지금
그리는 값이 다르면 연결하는 순간 화면이 바뀌고, 그건 전환이 아니라 **디자인 결정**이라
전환 커밋이 곁다리로 할 수 없다. **지금까지 다섯이 확인됐고(아래에서 모두 닫혔다) 나머지는 미측정이다** — 그 둘을
같이 읽어라. 다섯은 상한이 아니라 지금까지 연 자리의 수다.

**같은 형태를 이 레포는 이미 한 번 겪었다** — 폰트 축에서 "값이 바뀌는 치환은 하나도
없다" 는 주장과 함께 올라온 묶음에 실제로는 ±0.5~1.0 변경이 10 자리 섞여 있었고,
그 결과가 [ADR-0126](../../adr/0126-off-scale-font-values-are-not-snapped-to-tokens.md) 이다.
그 ADR 의 결론(스케일 밖 값은 스냅하지 않고 사유를 적은 명명 const 로 둔다)은 폰트 ·
코너 반경 · 점 치수 세 축에 적용돼 있다.

**전환 전에 값 대조부터 한다.** 접근자를 부르기 전에 그 자리가 지금 그리는 값을 재고,
토큰 값과 다르면 연결하지 말고 디자인 판단을 받는다.

실측(2026-09-05, 치수 접근자 208): 접근자 이름으로 불리는 것 125 · **base 필드 이름으로
불리는 것 53** · 자체 계산이면서 아무 이름으로도 안 불리는 것 30.

★ **"안 불리는 접근자" 를 세는 것으로는 이 위험을 못 잰다.** 접근자가 안 불려도 같은 값이
base 필드 이름으로 이미 불리고 있을 수 있고(예: `titlebar-caption-width` ↔ `caption_width`),
반대로 **base 이름으로 불리고 있어도 그 컴포넌트 자리는 다른 값을 그리고 있을 수 있다.**
**호출 여부와 값 일치는 독립이다.**

값이 어긋나는 것으로 **확인된** 자리는 **한 줄도 안 남았다** — 2026-09-17 디자인 결정
(D1~D7)이 다섯을 모두 닫았다.

닫힌 다섯은 이렇게 갈렸다 — `tab-dot-size` 는 토큰이 `status-dot-size-compact`(6)를
가리키도록 바뀌어 자리가 맞았고(D2), `toast-gap`(D1) · `status-dot-attached-ring-width`(D4) ·
`-offset`(D5) · 상태바 점(D3, 7→`statusbar-dot-size` 6)은 **자리가 토큰 값으로 옮겨 갔다**
(6→8 · 1.5→2 · 1.5→2 · 7→6, 전부 의도된 시각 변화).

**확인된 자리가 다 닫혔다는 것이 "어긋난 자리가 없다" 는 뜻은 아니다** — 열린 확인 자리가 없다는
뜻이다. 아래 "미측정" 문단이 그 차이를 든다. 새로 확인되면 그 자리의 상수 doc 이 같은
사실을 적어야 한다: 값이 그리드 스케일 밖이라는 것과, 겨냥하는 토큰이 실재하되 값이
어긋난다는 것은 다른 물음이라 갈라 적는다. 어느 쪽이 맞는지는 디자인이 정하고, 그때까지
상수는 값을 지키고 이름만 남긴다(ADR-0126).

나머지는 **미측정**이다 — 0 이 아니라 안 잰 것이다. 이름이 같은 const 를 기계로 대조해
봤지만 `GAP` · `POPUP_WIDTH` · `MIN_HEIGHT` 같은 흔한 이름이 무관한 토큰에 무더기로 걸려
(예: `component.transfer-popup-width` ↔ 파일 피커의 `POPUP_WIDTH`) 그 수는 위험의 크기가
아니라 이름 충돌의 크기였다. **이 축의 판정은 이름이 아니라 정의를 열어야 선다.**

**남은 줄은 소스에 기록돼 있다 — 그 상수의 doc 주석에.** 그런데 전환을 하는 쪽은 상수
주석이 아니라 이 문서를 읽는다. 그래서 여기 옮겨 적는다. 반대 방향도 성립한다 — 이 표에
줄을 더할 때 그 상수의 doc 도 같이 적어야 두 자리가 갈리지 않는다.

## 관련

- [theme.md](theme.md) — Theme 2계층 모델 + UI 디자인 규칙
- `crates/tasty-design-tokens/` — vendor json + 치수 codegen + 드리프트 가드 (갱신 절차는 crate README)
- 코드: `crates/tasty-type-appearance/src/theme.rs` (필드) · `crates/tasty-themes/src/fallback.rs` (mocha 색값)
