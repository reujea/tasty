# ADR-0036: 플러그인 아이콘은 빌드타임 SVG 베이크 + `tasty-icons` 단일 소스로 그린다

- **Status**: Accepted
- **Date**: 2026-07-05
- **Tags**: plugin, icons, tasty-icons, single-source, build-time-bake, svg, vector, egui-mesh, design-parity, i18n, adr-0510, adr-0028, adr-0030
- **Group**: ui-theme-gallery

## Context

라인 아이콘(chevron·refresh·edit·plus·file·arrow 등)을 그리는 소비자가 셋으로 갈려 있었다.

1. **host**(`src/adapters/ui/icons.rs`) — 런타임 egui_extras SVG 로더로 `<svg>` 문자열을 텍스처로 올려 그린다.
2. **gallery**(`crates/tasty-gallery`) — host 와 같은 egui_extras 경로로 아이콘 카탈로그·specimen 을 그린다.
3. **plugin**(image·markdown) — egui-mesh 채널([ADR-0028](0028-plugin-egui-mesh-render-channel.md) · [ADR-0030](0030-image-egui-mesh-bitmap-texture.md))로 **자기 프로세스에서** 툴바/주소창 chrome 을 tessellate 해 host 가 합성한다.

문제:

- **아이콘 path 중복·drift**. host 와 gallery 가 같은 `<path d=…>` 를 각자 정의했고 일부(`DETAIL`/`IMAGE`/`STAR`/`CHEVRON_UP`)는 실제로 모양이 어긋나 있었다. 단일 소스가 없어 디자인 SoT 대비 정합을 파일 단위로 보증할 수 없었다.
- **plugin 은 raw 유니코드 글리프**(`◀ ▶ ↻ ✏ +`, `📄`, `→`)를 폰트 텍스트로 그렸다. host 라인 아이콘 셋과 시각이 다르고, 폰트에 없으면 tofu 가 나며, theme tint(색)를 줄 수 없고, i18n 중립 원칙(장식 문자 하드코딩 금지)에도 어긋났다.
- plugin 은 **out-of-process 로 mesh 를 그린다**. host 의 egui_extras 로더를 각 plugin 프로세스에 그대로 얹으면 DPI 마다 SVG→텍스처 래스터라이즈가 생기고 `TexturesDelta` 채널이 커진다. plugin chrome 은 얇고 아이콘 몇 개뿐이라 이 비용이 과하다.

요구: **아이콘 정의를 한 곳에 두고**, host/gallery 런타임 경로와 plugin 경로가 **바이트 동일한 소스**를 보게 하되, plugin 은 텍스처 없이 DPI 독립·theme tint 가능한 벡터로 그린다.

## Decision

아이콘의 canonical 소스를 **egui-free 단일 크레이트 `tasty-icons`** 로 만든다. 아이콘당 `pub const <NAME>: Icon` 하나가 완성 `<svg>` 문서·inner path·캐시 uri·filled 플래그를 전부 `&'static str`/`bool` 로 노출한다.

- **host/gallery** 는 `tasty-icons` 를 `features = ["egui"]` 로 링크해 `Icon::image()`(egui_extras 로더)로 런타임 렌더한다.
- **plugin** 은 `tasty-icons` 를 `[build-dependencies]`(egui off)로 링크하고, `build.rs` 가 같은 `Icon.svg` 문자열을 usvg 로 파싱·**베지에 평탄화**해 `pub const <NAME>: &[&[[f32; 2]]]`(viewBox 0..24 좌표 점배열)을 `OUT_DIR` 에 생성한다(**방식 B — 벡터**). 런타임엔 [`tasty_plugin_sdk::baked_icon::draw`] 가 이 점배열을 그릴 크기로 스케일해 egui stroke 로 그린다(텍스처 없음, DPI 독립, theme 색 tint).

두 경로가 `concat!` 로 합성한 **동일 `<svg>` 문자열**을 보므로 host↔gallery↔plugin 이 바이트 동일 소스를 공유한다. gallery specimen 도 같은 canonical 아이콘을 egui_extras 글리프로 렌더해 본체 베이크 룩을 미러한다(raw 유니코드 제거).

## Consequences

- **얻은 것**:
  - **단일 canonical 소스** — 아이콘 path 를 `tasty-icons` 한 곳에만 둔다. host/gallery 중복 정의와 drift 제거, 디자인 SoT 대비 정합을 크레이트 단위로 보증.
  - **plugin 툴바·주소창이 host 라인 아이콘과 시각 정합** — DPI 독립 벡터 stroke, theme 토큰으로 tint(disabled=muted 등), tofu 불가.
  - **i18n 중립** — 장식용 raw 유니코드 글리프를 코드에서 제거.
  - plugin 런타임에 egui_extras SVG 로더·텍스처 경로 불필요(build-dep 는 egui 미링크 — `default-features=false`).
- **잃은 것 / 제약**:
  - plugin 마다 **build.rs + usvg build-dep + 베이크 스텝** 이 붙는다(빌드 시간·복잡도 소폭 증가).
  - **방식 B 는 stroke-only 아이콘만** 베이크한다(평탄화가 stroke 전용). 채운/다색 아이콘은 이번 범위 밖(→ Alternatives C / 재검토 트리거).
- **운영 비용 / 유지 부담**:
  - 새 plugin 툴바 아이콘 = `tasty-icons` 에 const 추가 + 그 plugin `build.rs` 의 `ICONS` 목록에 한 줄 추가.
  - `tasty-icons` 의 egui 버전은 host/gallery/plugin 통일 인스턴스(`0.31`, `default-features=false`)와 맞춰야 `Image`/`Color32` 타입이 충돌하지 않는다.

## Alternatives Considered

- **A — 파일 자산(각 plugin `assets/icons/*.svg`)을 build.rs 가 파일로 읽기**: PoC 초기 방식(refresh.svg 1개). 소비자가 셋(host/gallery 런타임 + plugin 빌드)인데 파일은 plugin 마다 복사돼 **다시 drift** 나고, host/gallery 의 컴파일타임 `&'static str` 요건을 충족 못한다. 크레이트는 `concat!` 로 합성한 동일 문자열을 build-dep 와 egui feature 양쪽에 노출해 바이트 동일을 보증하므로 채택.
- **B — plugin 프로세스에 런타임 egui_extras SVG 로더 설치**(host 와 동일 경로): plugin 이 mesh 를 out-of-process 로 그리는데 런타임 SVG→텍스처 래스터가 DPI 마다 생기고 `TexturesDelta` 가 커진다. 얇은 chrome 에 과한 상시 비용. 빌드타임 베이크는 텍스처 0·DPI 독립이라 이 경로를 대체.
- **C — 방식 A(래스터 베이크)**: build.rs 가 아이콘을 비트맵으로 굽는다. 채운/다색 글리프엔 맞지만 stroke 라인 아이콘엔 텍스처 메모리·DPI 종속·tint 어려움을 안긴다. 현 대상 10개가 전부 stroke-only 라 방식 B(벡터)가 우세. **방식 A 는 미래 filled/다색 plugin 아이콘용으로 보류**.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- plugin 이 **채운/다색 아이콘**을 그려야 한다 → 방식 A(래스터 베이크)를 별도 경로로 도입.
- plugin 이 **빌드타임에 알 수 없는 동적 아이콘**(사용자 제공·원격 fetch 등)을 그려야 한다 → 런타임 로더 재검토.
- egui 가 in-process **저비용 벡터 SVG path 렌더**를 제공해 빌드 베이크 없이 텍스처 없는 렌더가 가능해진다.
- `tasty-icons` 의 egui 버전 고정이 host/plugin 업그레이드에 병목이 된다 → egui 의존을 더 얇게 격리.

## References

- [ADR-0028](0028-plugin-egui-mesh-render-channel.md) — plugin egui-mesh 렌더 채널(out-of-process tessellate → host 합성).
- [ADR-0030](0030-image-egui-mesh-bitmap-texture.md) — image surface mesh-only(비트맵=egui 텍스처).
- [ADR-0510](0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) — 갤러리 완전성(specimen cut 금지) — 아이콘 카탈로그·viewer specimen 미러 근거.
- `crates/tasty-icons/` — canonical `Icon` const + `stroke_icon!`/`fill_icon!` 매크로.
- `crates/tasty-plugin-sdk/src/baked_icon.rs` — 점배열 → egui stroke 벡터 렌더 helper.
- `crates/tasty-plugin-{image,markdown}/build.rs` — usvg 평탄화 베이크.
- [plugin-development](../dev-guide/plugin-development.md#4-plugin-ui-렌더-egui-mesh-채널) — plugin UI 렌더 개요.
