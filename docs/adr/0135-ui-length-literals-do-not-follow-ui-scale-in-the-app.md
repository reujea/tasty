# ADR-0135: 본체의 UI 길이 리터럴은 배율을 안 탄다 — 갤러리는 탄다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: theme, design-tokens, ui-scale, zoom, egui, gallery, guards, adr-0126, adr-0033
- **Group**: typed-length-dpi

## Context

UI 배율(`AppearanceSettings.ui_scale`)이 픽셀에 닿는 경로가 본체와 갤러리에서 **서로 다르다.**

| | 배율을 어디에 거는가 | 리터럴 길이는 |
|---|---|---|
| 본체 (`src/`) | `install_global_with_zoom` → `Theme::with_colors_and_zoom` 의 `zoomed()`. egui `zoom_factor` 는 `Gpu::update_scale_factor` 가 **1.0 으로 고정**한다 | **안 커진다** |
| 갤러리 (`crates/tasty-gallery/`) | `ctx.set_zoom_factor(state.ui_scale)` 로 egui 전역에 건다. `Theme` 는 zoom 1 로 만든다 | **커진다** |

본체가 egui 전역 zoom 을 안 쓰는 데에는 이유가 있다 — `update_scale_factor` 주석대로
`set_pixels_per_point` 은 `zoom = ppp / native_ppp` 를 계산하는데 native ppp 가 아직
갱신되지 않은 시점(예: macOS 자동 복원)에 0.5 같은 낡은 값이 박혀 영구히 남는다.

그 결과 본체에서는 **토큰만 배율을 타고 호출부 리터럴은 안 탄다.** 컨테이너 치수를
리터럴로 적으면 상자는 고정인데 그 안의 폰트·간격·글리프는 전부 토큰이라 커진다 —
1.2 에서 내용이 잘리고 0.85 에서 빈 공간이 남는다. zoom 1 에서는 어떤 값 테스트도 울지
않으므로 이 결함에는 자동 채널이 없었다.

ADR-0126 이 이미 "명명 const 는 `zoomed()` 밖이라 배율을 안 탄다" 를 대가로 기록했지만,
그 ADR 의 축은 폰트·반경(값 3~17)이다. 길이 축의 값은 26~340 이라 **대가의 크기가 10 배**고
형태도 다르다 — 값 불일치가 아니라 **내용 손실**이다.

## Decision

**본체 UI 코드는 길이를 리터럴로 적지 않는다. 대응 디자인 토큰이 없어도 `Theme` 접근자로
뺀다** — 접근자 본문이 `LogicalPx((N * self.ui_zoom).round())` 형태이므로 배율을 탄다.
이것은 `modhint_*` · `multiselect_*` 가 이미 쓰던 형태이고, 디자인 export 에 해당 토큰이
들어오면 생성물로 넘어간다. **값은 바꾸지 않는다** — 스케일 밖 값을 가까운 토큰으로
스냅하지 않는 ADR-0126 을 그대로 지킨다. 바뀌는 것은 배율 추종뿐이고 zoom 1 픽셀은 동일하다.

**갤러리의 같은 형태 리터럴은 결함이 아니다.** 갤러리는 egui 전역 zoom 을 쓰므로 리터럴과
토큰이 함께 커진다. 따라서 이 축의 가드는 갤러리를 모수에 넣지 않는다 — 넣으면 결함이
아닌 것을 위반으로 세게 되고, 그것이 바로 allowlist 를 부풀리는 경로다.

## Consequences

- **얻은 것**: 컨테이너 치수가 배율을 따라간다. 자동 채널도 생겼다 —
  `crates/tasty-type-appearance/src/zoom_coverage_guard.rs` 가 "숫자를 담은 길이 접근자는
  `self.ui_zoom` 을 곱한다" 를 강제한다(편입 시점 접근자 80 · 위반 0 · 면제 0). lib 유닛이라
  Windows·headless 두 `--lib --bins` 잡에서 자동 실행된다.
- **얻은 것**: 두 경로의 비대칭이 문서에 남았다. 이 사실은 소스 어디에도 안 적혀 있어서
  이 축을 건드릴 때마다 다시 발견해야 했고, 모르면 갤러리 자리를 위반으로 오판한다.
- **잃은 것**: 대응 토큰이 없는 치수가 `Theme` 접근자로 올라온다 — 컴포넌트 하나짜리 값이
  전역 타입의 표면이 된다. 디자인 export 가 갱신되기 전까지의 임시 거처다.
- **운영 비용**: 새 컨테이너 치수를 넣을 때 접근자를 하나 추가해야 한다. 가드가 접근자
  **안**만 보므로, 호출부에 리터럴을 직접 적는 새 자리는 이 가드가 못 잡는다 —
  그쪽은 `crates/tasty-doc-guards/tests/design_token_adherence.rs` 의 접두 목록이 맡을 축이고 지금은 열려 있다.

## Alternatives Considered

- **A: 본체도 egui `zoom_factor` 에 `ui_scale` 을 건다(갤러리와 통일)** — 리터럴 문제가
  통째로 사라진다. 안 고른 이유는 `update_scale_factor` 가 기록한 실제 사고다: native ppp
  갱신 전에 zoom 이 계산되면 낡은 값이 영구히 박힌다. 그 위험을 이 축을 위해 다시 열지 않는다.
  갤러리는 자체 창 하나뿐이고 DPI 전환 경로가 없어 사정이 다르다.
- **B: 스케일 밖 값을 가장 가까운 토큰으로 스냅한다** — 배율 문제도 같이 풀린다. 안 고른
  이유는 픽셀이 실제로 바뀌기 때문이다(ADR-0126 의 결정을 이 축에서 뒤집는 것이 된다).
  값 판단은 디자인의 몫이고 리터럴 정리가 곁다리로 할 일이 아니다.
- **C: 값을 명명 `const` 로 빼고 호출부에서 배율을 곱한다** — 접근자를 안 늘려도 된다.
  안 고른 이유 둘: const 는 `zoomed()` 밖이라 곱을 빠뜨리면 원래 결함으로 즉시 돌아가고,
  호출부마다 곱을 반복하면 그 곱을 검사할 자리가 없다. 접근자는 검사 지점이 하나다.
- **D: 범용 `Theme::zoom_px(f32)` 헬퍼 하나로 끝낸다** — 접근자 증가가 없다. 안 고른 이유는
  그것이 raw px 를 호출부에 두는 것을 정당화하는 API 라서다. "색·폰트·굵기·간격은 `Theme`
  에서 가져온다" 는 기존 규칙과 정면으로 어긋난다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 본체가 egui `zoom_factor` 를 쓰게 된다(대안 A). 그 순간 리터럴도 배율을 타므로 이 결정의
  전제가 사라지고, 가드와 접근자 다수가 불필요해진다. 전제는
  `Gpu::update_scale_factor` 의 `set_zoom_factor(1.0)` 한 줄이다.
- 갤러리가 egui 전역 zoom 을 그만두고 본체와 같은 경로로 바꾼다. 그때는 갤러리 리터럴이
  **결함이 되므로** 가드의 모수에 갤러리를 넣어야 한다.
- 디자인 export 에 이 컴포넌트 치수들의 토큰이 들어온다 — 수기 접근자를
  `generated_component.rs` 로 넘기고 이 ADR 의 "임시 거처" 서술을 갱신한다.
- 호출부 리터럴을 막는 접두 가드가 생긴다 — 그때 이 ADR 의 "운영 비용" 에 적힌 구멍이 닫힌다.

## References

- [ADR-0126](0126-off-scale-font-values-are-not-snapped-to-tokens.md) — 스케일 밖 값을
  토큰으로 스냅하지 않는 규약. 이 ADR 은 그 규약을 길이 축에 적용하면서, 그 ADR 이 대가로
  적어 둔 "명명 const 는 배율을 안 탄다" 를 길이 축에서는 접근자로 회피한다
- [ADR-0033](0033-ui-color-semantic-role-only.md) — 값은 `Theme` 경유라는 상위 규칙
- [`docs/design/systems/theme.md`](../design/systems/theme.md) — UI 디자인 규칙
- `crates/tasty-type-appearance/src/zoom_coverage_guard.rs` — 이 결정의 집행 장치
- `crates/tasty-type-appearance/src/theme.rs` `with_colors_and_zoom` — `zoomed()` 적용 범위
