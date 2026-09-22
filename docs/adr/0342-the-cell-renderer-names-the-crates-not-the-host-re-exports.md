# ADR-0342: 셀 렌더러는 크레이트를 직접 부르고, 본체 재수출을 거치지 않는다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, layering, renderer, selection, terminal-link, naming, headless, adr-0319, adr-0324
- **Group**: architecture

## Context

선택·링크 타입은 이미 잎 크레이트로 내려가 있다 — 선택은 `tasty-selection`
([ADR-0319](0319-the-selection-model-drops-the-stale-gui-gate-instead-of-carrying-it.md)),
링크 검출은 `tasty-terminal-link`
([ADR-0324](0324-link-detection-and-link-opening-split-by-side-effect.md)). 두 결정은
"렌더러가 앱 상태 모듈이 아니라 크레이트를 본다" 를 얻은 것으로 적었다.

**타입은 그렇게 됐는데 표기는 안 그랬다.** 두 결정 모두 호출부를 안 건드리려고 본체에
이름을 이어 뒀고(`src/state.rs` 의 `pub use tasty_selection as selection;` ·
`src/adapters/ui/terminal_link.rs` 의 `pub use tasty_terminal_link::*;`), 크레이트
루트가 그것을 다시 `crate::selection` · `crate::terminal_link` 로 재수출한다. 그래서
셀 렌더러가 소스에 적고 있던 것은 여전히

```rust
use crate::selection::{NormalizedSelection, SelectionPoint};   // = state::selection
use crate::terminal_link::LinkHighlight;                       // = adapters::ui::terminal_link
```

였다. 컴파일되는 타입은 크레이트의 것이지만, **소스를 읽어서는 렌더러가 앱 상태와 UI
어댑터를 보는 것으로 읽힌다** — 그리고 그 두 재수출 중 하나가 사라지면 렌더러가 깨진다.
의존 방향을 재는 사람도 도구도 표기를 본다.

링크 쪽은 표기만의 문제도 아니었다. `crate::terminal_link` 는 `src/lib.rs` 에서
`#[cfg(feature = "gui")]` 뒤에 있다(실체인 `adapters::ui` 가 그 게이트다 — 링크를 **여는**
쪽이 `webbrowser` 를 들기 때문이고, [ADR-0324](0324-link-detection-and-link-opening-split-by-side-effect.md)
가 그래서 검출과 열기를 갈랐다). 즉 렌더러는 **게이트 없는 타입을 게이트 뒤의 이름으로**
부르고 있었다. 렌더러 자신이 `gui` 뒤라 컴파일은 통과했지만, 그 이름으로는 헤드리스
소비자가 같은 타입을 부를 수 없다.

같은 형태가 폭 표(`tasty-cell-width`)·글리프 아틀라스(`tasty-font`)·사각형
(`tasty-model`)에도 있었다. 셋 다 본체 모듈 파일 한 줄짜리 재수출을 거치고 있었다.

## Decision

`src/gfx/renderer.rs` 와 `src/gfx/renderer/` 는 **워크스페이스 크레이트를 직접 이름으로
부른다** — `tasty_selection` · `tasty_terminal_link` · `tasty_cell_width` ·
`tasty_font` · `tasty_model`. 본체의 `crate::selection` · `crate::terminal_link` ·
`crate::font` · `crate::model` 재수출은 **렌더러 안에서 쓰지 않는다.** 타입도 동작도
바뀌지 않는다 — 같은 타입을 원래 이름으로 부를 뿐이다.

렌더 입력 타입의 표기는 호출부와 같아야 뜻이 하나로 남으므로, 그 두 타입
(`tasty_selection::*` · `tasty_terminal_link::LinkHighlight`)에 한해 `src/gfx/gpu.rs` ·
`src/gfx/gpu/render_pass.rs` 의 시그니처도 함께 옮긴다. 그 둘의 나머지 본체 참조
(`crate::state` · `crate::core` · `crate::search_state` · `crate::model`)는 **그대로
둔다** — `gpu` 는 `AppState` · `CoreState` · `PluginManager` 를 받는 호스트 접착층이고,
그 의존은 거짓이 아니라 사실이다.

그 결과 렌더러가 부르는 본체 경로는 **`crate::cell_palette` 하나**로 남는다. 그것은
지우지 않는다 — 셀 색 해석은 `gui` 게이트 밖에 있어야 하고(헤드리스
`debug.glyph_color` 핸들러가 같은 함수로 답한다), 새 크레이트를 만들지 않는 한 렌더러와
헤드리스 핸들러가 공유할 자리는 본체 모듈뿐이다.

## Consequences

- **얻은 것**: 렌더러의 의존이 **표기에서도** 크레이트다. 렌더러 두 파일에서 주석 줄을
  뺀 `crate::` 언급이 두 줄(둘 다 `cell_palette`)이고, 그 두 줄이 곧 "렌더러를 본체
  밖으로 옮기려면 무엇이 남았는가" 의 전량이다 — 전에는 그 물음에 답하려면 재수출
  사슬을 넷 따라가야 했다. 세는 명령은 아래 재검토 조건에 있다.
- **잃은 것**: 본체 재수출 이름과 렌더러의 이름이 **갈렸다.** `state::selection` 을 다른
  크레이트로 갈아끼우면 렌더러는 따라오지 않는다 — 그것이 이 결정이 의도한 바지만,
  "한 곳만 고치면 전부 따라온다" 는 성질은 그 자리에서 사라진다.
- **운영 비용 / 유지 부담**: 없다. 새 타입·새 인자·새 크레이트·새 feature 가 없고,
  `Cargo.toml` 도 안 바뀐다(다섯 크레이트 전부 이미 무조건 의존이다).

## Alternatives Considered

- **A: 그대로 둔다 — 타입이 이미 크레이트니 표기는 취향이다** — 컴파일 결과는 같지만
  의존 방향을 재는 모든 수단(사람의 grep, 이동 계획, 재수출 삭제)이 표기를 본다.
  ADR-0319 가 "얻은 것" 으로 적은 문장이 소스에서 확인되지 않는 상태가 남는다. 기각.
- **B: `cell_palette` 까지 잎 크레이트로 내려 렌더러의 본체 참조를 0 으로 만든다** —
  가장 깨끗하지만 새 크레이트가 필요하고, 그러면 크레이트 수 lockstep 네 자리
  (`docs/architecture/index.md` · 두 README 의 배지와 본문 · `architecture_crate_list_complete.rs`)
  가 같은 커밋에 딸려 온다. 그 네 자리는 크레이트를 만드는 쪽이 한 번에 움직여야
  어긋나지 않으므로, 표기를 고치는 이 작업과 같이 하지 않는다. 보류 — 좌변이 한
  모듈뿐이라 나중에 해도 비용이 같다.
- **C: 반대로 본체 재수출을 지우고 모든 호출부를 크레이트 이름으로 바꾼다** — 렌더러 밖
  호출부가 열 자리가 넘고, `src/state.rs` · `src/adapters/ui/` · `src/view/` 처럼 이
  결정과 무관한 층에 흩어져 있다. 이 결정이 묻는 것은 "렌더러가 무엇을 보는가" 하나라
  거기까지 넓힐 이유가 없다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `crate::cell_palette` 가 렌더러에서 사라진다 — 대안 B 가 실행됐다는 뜻이고, 그러면 이
  ADR 이 남긴 "하나" 라는 수가 0 이 된다. 그때 이 문서의 Decision 마지막 문단을 다시 본다.
- 본체가 `tasty-selection` · `tasty-terminal-link` 를 optional 의존으로 바꾼다. 그러면
  렌더러가 직접 부르는 이름이 feature 뒤로 들어가고, 재수출을 거치던 때와 게이트가 달라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 렌더러가 다시 본체 모듈을 이름으로 부르게 되는가. **이 사실을 재는 자동 채널은 없다** —
  새 `use crate::…` 한 줄은 컴파일되고 아무 시험도 안 깬다. 재는 법:

  ```bash
  grep -rn 'crate::' src/gfx/renderer.rs src/gfx/renderer/ | grep -v '^[^:]*:[0-9]\+:[[:space:]]*//'
  ```

  **두 줄**(둘 다 `cell_palette`)이 아니면 무엇이 더 나왔는지 본다. 뒤의 `grep -v` 는
  생략할 수 없다 — 좌변이 `.rs` 원문이라 앞 명령만 쓰면 이 규칙을 적은 `renderer.rs`
  머리 주석 자신을 두 줄 세고, 그 상시 잡음이 회귀 한 줄을 덮는다. 좌변을
  `use crate::` 로 좁히는 대안은 **기각한다**: 이 결정이 지운 형태 중 둘이 `use` 없이
  본문에 박힌 `crate::selection::is_selected(...)` 였고 좁힌 좌변은 그것을 못 본다
  (base `63a777ecc` 에서 위 좌변 전수 — `renderer.rs` + `renderer/` 아래 **다섯
  파일** — 로 재면 `use crate::` 는 10, 위 두 단계는 12 다. 차가 정확히 그 두
  자리다. 좌변을 두 파일로 줄여 세면 `renderer/pipeline.rs` 의 한 줄이 빠져 9·11 이
  나오는데, 차이 2 가 그대로라 그 오차는 결론을 안 흔들고 그래서 안 들킨다).
  자동 가드를 붙이지 않은 이유는 이 결정이 **표기 규약**
  이라, 강제하려면 소스 텍스트를 읽는 가드가 필요한데 그 판정기의 유지 비용이 좌변
  두 줄보다 크다고 봤기 때문이다.

  **이 재는 법의 사각도 같이 적는다 — "조용한 통과가 없다" 는 말은 할 수 없다.**
  `grep -v` 의 패턴은 줄머리에 고정돼 있고(`^[^:]*:[0-9]\+:` 뒤에서만 `//` 를 본다),
  고정 전에는 진짜 코드 줄이라도 후행 주석에 `파일:줄:` 인용이 있으면 통째로 버려졌다
  (탐침 `pub const _P: bool = crate::state::FLAG; // see <파일>:99: // gui gate` 가
  필터 뒤 0 이었다. 고정 뒤엔 남는다). 고정으로도 **못 닫는 것**이 하나 남는다 —
  `crate::` 라는 글자가 없는 본체 의존은 어떤 텍스트 술어로도 안 보인다. 탐침 둘
  (`use super::super::super::state::AppState;` · `use crate as c;` + `c::state::…`)이
  둘 다 필터 뒤 목록에 안 나왔다. 반대로 블록 주석·문자열 리터럴 안의 `crate::` 는
  그대로 세어 **더 많이 잡는다** — 이 재는 법에는 상한도 면제 목록도 자동 실패문도
  없고 처방이 "무엇이 더 나왔는지 사람이 본다" 하나라, 잉여 한 줄의 비용은 눈으로 한
  번 보는 것이고 그 자리가 영구히 헐거워지지 않는다.
- 표기를 갈라 둔 값이 실제로 있었는가 — 즉 렌더러를 본체 밖으로 옮기는 작업이 이 결정
  덕에 짧아졌는가. 재는 법: 그 이동을 실제로 시도할 때 렌더러 쪽에서 고칠 본체 경로
  줄이 몇이었는지 세어, 이 ADR 이 말한 "두 줄" 과 대조한다.

## References

- 타입 소속을 내린 두 결정: [ADR-0319](0319-the-selection-model-drops-the-stale-gui-gate-instead-of-carrying-it.md) ·
  [ADR-0324](0324-link-detection-and-link-opening-split-by-side-effect.md)
- 크레이트 분할이 의존 방향을 따른다는 결정: [ADR-0089](0089-crate-split-follows-dependency-direction.md)
- 코드 근거(결정이 실현된 **현재 위치**): `src/gfx/renderer.rs` 머리 주석과 그 모듈의
  `use` 목록, `src/cell_palette.rs` 의 `compute_cell_colors`
- 렌더 구조: [gpu-rendering](../dev-guide/gpu-rendering.md)
