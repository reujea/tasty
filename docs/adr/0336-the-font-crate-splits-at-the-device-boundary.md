# ADR-0336: `tasty-font` 은 device 경계에서 갈린다 — wgpu 는 `gpu` feature 뒤로

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: build, cargo, features, headless, font, dependency-graph, wgpu, adr-0326
- **Group**: architecture

## Context

[ADR-0326](0326-egui-is-opt-in-for-the-crates-that-link-it.md) 이 egui 를 opt-in 으로 만든
뒤에도 헤드리스 의존 그래프에 GPU 스택이 남아 있었다. 실측(2026-09-20, `57336d169`):

```
헤드리스 342 · 기본(gui) 524      (cargo tree -e normal, 이름+major 유일)
남은 것: wgpu wgpu-core wgpu-hal wgpu-types naga glow ash gpu-alloc gpu-descriptor
        renderdoc-sys profiling · cosmic-text fontdb swash skrifa read-fonts …
```

입구는 하나다. `tasty-font` 이 `cosmic-text` 와 `wgpu` 를 **비-optional** 로 들었고
`[features]` 절 자체가 없었다. 루트는 그 크레이트를 비-optional 로 들고, 헤드리스 코드가
실제로 그것을 쓴다 — `src/boot/locale_font.rs` 의 `family_path` 가 `FontConfig::new` 로
시스템 폰트 DB 를 열어 family 이름을 경로로 바꾼다. 그 모듈에는 cfg 게이트가 없다.

**그 크레이트 안에서 두 의존의 처지가 다르다는 것이 이 결정의 출발점이다.**

- `wgpu` 를 보는 것은 `GlyphAtlas` 하나뿐이다. 그 타입은 `wgpu::Device` 없이 존재할 수 없고,
  그것을 쓰는 곳은 `src/gfx/` 뿐인데 그 모듈은 이미 `#[cfg(feature = "gui")]` 뒤에 있다.
- `cosmic-text` 는 **`FontConfig` 자신**이다 — 필드 넷 중 셋(`FontSystem` · `SwashCache` ·
  `FamilyOwned`)이 그 크레이트의 타입이다. 헤드리스가 부르는 것이 바로 그 타입이므로,
  이 의존을 그래프에서 빼려면 `family_path` 가 폰트 DB 를 **다른 수단으로** 열어야 한다.
  그것은 경계 이동이 아니라 동작 변경이다.

그래서 **이 결정이 다루는 것은 `wgpu` 뿐이고, `cosmic-text` 는 의도적으로 남긴다.**

한 가지 더가 형태를 정했다. 워크스페이스 lint 가 `dead_code = "deny"` 다. `GlyphAtlas` 만
cfg 로 가리면 그것만 쓰던 **비공개** 항목들(ASCII 캐시 · 내장 글리프 래스터라이저 ·
box-drawing 모듈)이 헤드리스에서 경고가 아니라 **컴파일 에러**가 된다. 경계를 항목마다
cfg 로 흩을 수도 있었지만, 그러면 "무엇이 device 쪽인가" 가 소스를 훑어야만 보인다.

## Decision

`tasty-font` 을 **device 경계에서 모듈로 가르고**, `wgpu` 를 `gpu` feature 뒤의 optional
의존으로 만든다.

- `crates/tasty-font/src/atlas.rs` — `GlyphAtlas` 와 그것만 쓰는 래스터라이저·ASCII 캐시·
  box-drawing. 이 모듈만 `wgpu` 를 본다. 선언이 `#[cfg(feature = "gpu")] mod atlas;` 다.
- 크레이트 루트 — `FontMetrics` · `FontConfig` · `GlyphKey` · `AtlasEntry` · `AtlasPage` ·
  `pick_lru_victim` · `FrameClock` · 번들 폰트 상수. **device 가 없어도 성립하는 것들이다.**
  아틀라스의 상태 기계(shelf 패킹 · LRU · 프레임 시계)가 여기 남는 것은 우연이 아니다 —
  그것들은 이미 device 없이 단위시험되고 있었고, 갈리기 전 그 시험 **18 개**가 `gpu` 를
  꺼도 그대로 돈다.
- `default = []`. 기본으로 켜면 새 소비자가 **묻지 않아도 되고**, 묻게 만드는 것이 이 갈림의
  목적이다(ADR-0326 이 `tasty-type-appearance` 에서 같은 이유로 같은 선택을 했다).
- 루트의 `gui` feature 가 `"tasty-font/gpu"` 를 켠다. 루트는 이미 `"dep:wgpu"` 를 거기서
  켜고 있었으므로 이 줄은 그 옆에 붙는다.

**`src/lib.rs` 의 `pub use tasty_font as font;` 는 그대로 둔다.** 그 재수출은 의존 그래프에
아무 영향이 없다 — 그래프는 매니페스트의 의존·feature 선언으로 정해진다. 헤드리스가
`FontConfig` 를 쓰는 한 `tasty → tasty-font` 간선은 남아야 하고, 빠져야 하는 것은
`tasty-font → wgpu` 간선이다.

## Consequences

- **얻은 것 — 헤드리스 그래프에서 27 크레이트가 빠진다.** 실측(2026-09-20):

  ```
  이름+major 유일:  헤드리스 342 → 314      기본(gui) 524 → 524 (불변)
  이름만 유일:      헤드리스 333 → 306
  ```

  빠진 것: `wgpu` `wgpu-core` `wgpu-hal` `wgpu-types` `naga` `glow` `ash` `gpu-alloc`
  `gpu-alloc-types` `gpu-descriptor` `gpu-descriptor-types` `renderdoc-sys` `profiling`
  `spirv` `khronos-egl` `libloading` `raw-window-handle` `codespan-reporting` `termcolor`
  `arrayvec` `document-features` `litrs` `hexf-parse` `static_assertions` `strum`
  `strum_macros` `unicode-xid`. 들어온 것은 **0** 이다.
  (두 규약이 1 만큼 다른 것은 `rustc-hash` 가 두 major 로 들어와 있어서다 — wgpu 쪽 v1 이
  빠지고 이 크레이트가 쓰는 v2 가 남는다. 수를 인용할 때 규약을 같이 적어야 하는 이유다.)

- **잃지 않은 것 — cosmic-text 는 남는다.** 위 목록에 `cosmic-text` · `fontdb` · `swash` ·
  `skrifa` · `read-fonts` · `ttf-parser` · `harfrust` · `font-types` · `yazi` · `ab_glyph` 가
  없는 것은 실패가 아니라 Context 가 말한 그대로다. 그것을 빼는 것은 `family_path` 의
  동작을 바꾸는 별개 결정이다.

- **잃은 것 — 조합마다 컴파일되는 코드가 달라진다.** `gpu` 를 끈 조합에서는 `atlas.rs` 가
  아예 타깃에 안 들어간다. 다만 **시험은 하나도 안 잃는다** — 실측: `gpu` on/off 두 조합
  모두 **19 건**이고 이름 목록이 같다(정렬 후 diff 0). **두 수를 갈라 읽어야 한다** —
  갈리기 전 18 건이 양쪽에서 그대로 돌고, 거기에 이 결정이 더한 전제 시험
  (`wgpu_stays_an_optional_dependency_of_this_crate`) 1 건이 얹혀 19 가 된다. 그 시험은
  매니페스트를 읽으므로 feature 와 무관하게 양쪽에 있다. 18 이 양쪽에서 도는 것은 그
  크레이트의 시험이 처음부터 전부 device-free 였기 때문이고, 그 사실이 이 자리를 가를 수
  있게 한 이유이기도 하다.

- **운영 비용**: 새 크레이트를 만들지 않았으므로 크레이트 수 lockstep 4 자리
  (`docs/architecture/index.md` · 두 README 배지·본문 ·
  `crates/tasty-doc-guards/tests/architecture_crate_list_complete.rs`)가 **안 움직인다.**

## Alternatives Considered

- **A: 설정/해석 타입을 새 크레이트로 뺀다.** 경계가 크레이트 경계가 되어 제일 단단하지만,
  **이 축에서는 아무것도 더 사지 못한다** — 빼야 할 것은 `tasty-font → wgpu` 간선이고 그것은
  feature 로 끊어진다. 반면 크레이트 수 lockstep 4 자리가 같은 커밋에 따라오고, 그 넷 중
  하나라도 빠지면 게이트가 빨개진다. 같은 결과에 더 많은 고정 자리를 사는 셈이라 안 골랐다.
- **B: `GlyphAtlas` 에만 `#[cfg(feature = "gpu")]` 를 단다.** 디프는 제일 작지만
  `dead_code = "deny"` 때문에 그것만 쓰던 비공개 항목 여덟 자리에도 같은 cfg 를 흩어야 한다.
  경계가 파일 안에 흩어지면 "무엇이 device 쪽인가" 를 훑어야만 알 수 있고, 다음 사람이 한
  자리를 빠뜨리면 그 자리가 조용히 양쪽에 남는다. 구조로 못 박는 쪽을 골랐다.
- **C: `cosmic-text` 까지 optional 로 만든다.** 헤드리스가 `FontConfig` 를 실제로 쓰므로
  동작을 바꾸지 않고는 불가능하다. 이 회차의 기본값은 행동 보존이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `tasty-font` 의 `wgpu` 가 다시 비-optional 이 되거나, `gpu` feature 선언이 사라지거나,
  `default` 가 비어 있지 않게 된다. **채널 있음**:
  `crates/tasty-font/src/lib.rs` 의 `wgpu_stays_an_optional_dependency_of_this_crate`.
  변이로 확인했다 — 세 형태 각각이 그 시험을 죽인다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **헤드리스 그래프에 GPU 스택이 다시 들어온다.** 위 시험은 이 크레이트의 **선언**을 보지
  그래프를 보지 않으므로, 다른 크레이트가 wgpu 를 새로 들이면 아무 신호도 안 난다.
  재는 법:

  ```bash
  cargo tree --no-default-features -e normal | grep -oE '[a-zA-Z0-9_-]+ v[0-9]' | sort -u | wc -l
  cargo tree --no-default-features -e normal --prefix none | awk '{print $1}' | sort -u \
    | grep -xE 'wgpu|wgpu-core|wgpu-hal|naga|glow|ash'
  ```

  첫 줄의 현재 값은 **314** 이고, 둘째 줄은 **아무것도 안 찍어야** 한다. 이름이 그래프에
  없으면 `cargo tree -i <이름>` 은 빈 출력이 아니라 `did not match any packages` 로
  **죽는다**(rc≠0) — 신호는 출력이 아니라 종료 코드다.

- **헤드리스가 `FontConfig` 보다 많은 것을 쓰게 된다.** 지금 `tasty-font` 를 코드에서 쓰는
  헤드리스 자리는 `src/boot/locale_font.rs` 의 `family_path` 하나뿐이다. 재는 법:
  `grep -rn 'tasty_font\|crate::font::' src/ --include='*.rs'` 로 세고, 나온 자리가
  `#[cfg(feature = "gui")]` 뒤인지 본다.

- **`family_path` 가 시스템 폰트 DB 를 cosmic-text 말고 다른 수단으로 열게 된다.** 그러면
  `cosmic-text` 도 optional 로 갈 수 있고 위 "잃지 않은 것" 의 10 크레이트가 함께 빠진다.
  재는 법: 위 첫 줄의 수가 314 에서 더 내려가는지.

## References

- 이 결정이 실현된 현재 위치: `crates/tasty-font/Cargo.toml` 의 `[features]` 와 `wgpu`
  선언, `crates/tasty-font/src/atlas.rs`, `crates/tasty-font/src/lib.rs` 의
  `mod atlas` 선언, 루트 `Cargo.toml` 의 `gui` feature 에 있는 `tasty-font/gpu`.
- [ADR-0326](0326-egui-is-opt-in-for-the-crates-that-link-it.md) — 같은 축의 앞 결정이고
  `default = []` 를 고른 근거를 이 ADR 이 그대로 따른다.
- [ADR-0325](0325-the-root-package-splits-into-a-lib-and-a-bin.md) — 헤드리스 소비자가
  붙을 수 있는 `lib` 타깃을 만든 결정.
- [`../dev-guide/build.md`](../dev-guide/build.md) 의 headless 절 — 그래프를 재는 두 명령과
  **채널이 없다는 사실**이 값과 함께 적혀 있는 자리.
