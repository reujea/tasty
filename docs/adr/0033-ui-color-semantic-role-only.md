# ADR-0033: UI 색은 semantic role 접근자로만 — primitive 필드 직접 접근 전면 금지(위젯 포함)

- **Status**: Accepted
- **Date**: 2026-07-03
- **Tags**: design-tokens, color, semantic, primitive, theme, ui-widgets, guard, enforcement, adr-0510
- **Group**: ui-theme-gallery

## Context

design-tokens 시리즈는 "UI 는 primitive 색(Catppuccin 원색: `crust`/`surface0`/`subtext0`/`blue` …)을 직접 읽지 않고 **semantic role 접근자**(`bg_app()`/`surface_raised()`/`text_muted()`/`accent_primary()`/`border_default()` …)만 읽는다"는 디자인 계약을 세웠다. 이유는 role 토큰이 SoT 가 되면, 디자인이 나중에 role 별로 색을 갈라도(예: `border-focus` ≠ `accent-primary`) 호출처 수정 없이 전파되고, "같은 색·다른 의미"의 다의성이 코드에 표현되기 때문이다.

접근자는 `crates/tasty-type-appearance/src/semantic_color_generated.rs` 에 DTCG(`dtcg/tasty.tokens.json`)에서 codegen 된다. host UI 계층(`src/view`, `src/adapters/ui`, `src/gfx/gpu/shell_setup.rs`)은 전수 이식 완료 + 소스 스캔 가드(`crates/tasty-doc-guards/tests/design_token_adherence.rs::no_primitive_color_field_access_in_host_ui`)로 재유입을 CI 차단했다(design-tokens-05-C).

마지막 잔여는 **재사용 위젯 크레이트 `tasty-ui-widgets`** 였다 — chip·status_dot·menu_item·two_depth·tree_row·table 에 `theme.subtext0`/`theme.surface0`/`theme.crust`/`theme.text`/`theme.surface1` 직접 접근 9곳. 여기서 갈렸다: **"범용 위젯은 앱의 semantic role 을 몰라야 하니 primitive 접근이 정당한 레이어"인가, 아니면 위젯도 이식 대상인가.**

## Decision

**위젯 크레이트를 포함한 모든 UI 코드는 색을 semantic role 접근자로만 읽는다. primitive Catppuccin 필드 직접 접근(`th.<field>`/`theme.<field>`)은 전면 금지한다.** `tasty-ui-widgets` 도 예외가 아니다 — 이 위젯들은 순수 범용 라이브러리가 아니라 tasty 전용 host chrome 조각이고, 여기에 구멍을 두면 그 틈으로 primitive 가 다시 샌다.

- 9곳 전부 값-보존(pixel diff 0) alias 로 이식: `subtext0 → text_muted()`, `crust → bg_app()`, `surface0(fill) → surface_raised()`·`surface0(stroke) → border_default()`, `surface1(stroke) → border_strong()`, `text → text_primary()`.
- **대응 role 토큰이 없는 use** 는 primitive 로 되돌아가지 않는다. 의미가 가장 가까운 role 접근자로 alias 하고 `// divergence:` 주석으로 불일치를 문서화한다(향후 design-token request 후보로 표시). 예: status-dot idle·chip "+"·table active-header 는 대응 role 부재로 `text_muted()`/`text_primary()` 로 alias + divergence 주석.
- **집행 채널 = 소스 스캔 가드 테스트**(`cargo test --workspace`. 그 잡은 수동 전용이라 자동으로 돌지 않는다 — [ci-gates](../dev-guide/ci-gates.md)). clippy 가 아니다 — clippy 는 `pub` 구조체 **필드 접근**을 검출하는 lint 이 없고(`disallowed-methods` 는 메서드 경로 전용), CI 에 `-D warnings` 도 없다. 필드 접근을 실제로 잡을 수 있는 건 정규식 소스 스캔뿐이므로 가드 테스트가 유일한 강제 장치다. 스캔 스코프에 `crates/tasty-ui-widgets/src` 를 편입한다.

**금지에서 제외(primitive 접근이 본질인 곳)**: 테마 시스템 내부(`tasty-type-appearance`·`tasty-themes` 자신), 색상 픽커(46색 flat 편집 UI), ANSI 팔레트·터미널 색 경로(GPU 렌더러), 갤러리 팔레트 데모(raw primitive 를 의도적으로 노출).

## Consequences

- **얻은 것**: "UI 는 semantic/component 만 읽는다"는 디자인 계약이 위젯 계층까지 완성. role 재매핑이 호출처 수정 없이 전파. 재유입이 CI 로 봉인(위젯 포함). 다의성 핫스팟이 코드에 명시(divergence 주석).
- **잃은 것**: 일부 use 가 의미상 완벽히 맞지 않는 role 로 alias 됨(divergence 주석으로 표식). clippy 로는 못 막음 — 필드 접근 lint 부재.
- **운영 비용 / 유지 부담**: 가드 테스트 스코프·allowlist 유지. 새 위젯은 처음부터 접근자만 쓴다. divergence 주석 목록은 향후 role 토큰 신설 시 해소 대상.

## Alternatives Considered

- **A: 위젯 크레이트는 primitive 접근 허용 레이어로 남긴다** — "범용 위젯은 앱 role 을 몰라야 한다"는 관점. 안 고른 이유: 이 위젯들은 tasty 전용 chrome 이고, 예외를 두면 그 틈으로 drift 가 재유입된다. 대상이 9곳뿐이고 전부 값-보존 alias 가 이미 존재해 이식 비용이 사실상 0.
- **B: clippy `disallowed-methods` 로 집행** — 안 고른 이유: clippy 는 `pub` 필드 접근을 못 잡는다(메서드 전용). 잡으려면 Theme 필드를 메서드로 바꿔야 하는데 픽커·테마 내부가 필드 직접 접근을 정당히 필요로 해 파탄. CI 에 `-D warnings` 도 없어 clippy 는 애초에 hard gate 가 아니다.
- **C: divergence 마다 role 토큰을 지금 신설** — 안 고른 이유: DTCG + codegen 을 타는 design-gated 작업이고, 일부는 픽셀이 바뀐다(diff ≠ 0). divergence 주석이 "미래 design-request 후보" 표식 역할을 하므로 지금 막을 필요 없이 기록만 남긴다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 외부(vendored) 위젯 크레이트를 도입하는데 그게 raw 팔레트를 정당히 필요로 하는 경우(제외 목록 확장 검토).
- `pub` 필드 접근을 검출하는 clippy/dylint lint 이 생겨 가드 테스트를 보완/대체할 수 있게 된 경우.
- 디자인이 현재 divergence alias 를 해소하는 role 토큰을 신설한 경우(alias → 정식 role 접근자 교체 + 주석 제거).

## References

- `docs/design/systems/theme.md` — "Semantic 접근자 우선"(집행 체계·보류 해제 기록)
- `crates/tasty-type-appearance/src/semantic_color_generated.rs` — role 접근자 codegen 산출물
- `crates/tasty-design-tokens/dtcg/tasty.tokens.json` · `crates/tasty-design-tokens/src/dtcg.rs` — 토큰 SoT + 매핑표
- `crates/tasty-doc-guards/tests/design_token_adherence.rs` — `no_primitive_color_field_access_in_host_ui` 가드(스코프에 ui-widgets 편입)
- [ADR-0510](0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) — 갤러리 = 컴포넌트 완전 출처(팔레트 데모 제외 근거)
- [ADR-0290](0290-settled-role-gaps-close-the-divergence-and-off-scale-sets.md) — 이 ADR 이 남긴 divergence alias 집합이 2026-09-17 디자인 결정으로 닫힌 기록(규칙 자체는 그대로 유효)
