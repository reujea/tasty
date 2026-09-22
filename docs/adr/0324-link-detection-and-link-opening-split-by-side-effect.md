# ADR-0324: 링크는 검출과 여는 것을 부수효과로 가른다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, crates, layering, terminal-link, headless, feature-gate, side-effect, adr-0319
- **Group**: architecture

## Context

터미널 링크 모듈은 렌더러가 색을 덮는 하이라이트 범위를 만든다. 그 타입이 UI adapter
모듈에 살아서 렌더러가 adapter 를 거꾸로 보는 모양이었고, 소속만 크레이트로 내리면 끝날
자리로 보였다 — 실측으로 그 파일은 `crate::` 를 **0 번** 쓰고 `#[cfg(feature = "gui")]`
도 **0 개**였다.

**그 둘이 결합 없음을 뜻하지 않았다.** `src/adapters/mod.rs` 가 `ui` 모듈 전체를
`#[cfg(feature = "gui")]` 로 감싸므로, 그 파일은 오늘 **gui 빌드에서만 컴파일된다.**
파일 안에 게이트가 없는 것은 게이트가 한 층 위에 있어서다. 그리고 그 안에 실제로 gui 에
매인 것이 하나 있었다 — `open_uri` 는 기본 브라우저를 부르고, 그 의존 `webbrowser` 는 루트
`Cargo.toml` 에서 `optional = true` 이며 `gui` feature 목록의 `"dep:webbrowser"` 로만
켜진다.

그래서 파일을 통째로 옮기면 **헤드리스 빌드가 브라우저 실행 의존을 떠안는다.** 반대로
옮기지 않으면 렌더러가 계속 adapter 를 거꾸로 본다. 자매 결정
[ADR-0319](0319-the-selection-model-drops-the-stale-gui-gate-instead-of-carrying-it.md)
는 게이트가 **낡아서** 버릴 수 있었지만, 여기 게이트는 낡지 않았다 — 브라우저를 여는 일은
헤드리스에서 성립하지 않는다.

## Decision

파일이 아니라 **부수효과로 가른다.** 검출과 그 결과 타입(순수 계산: 화면 텍스트에서 URL·
경로를 찾아 줄바꿈을 가로지르는 세그먼트와 하이라이트 범위를 만든다)은
`crates/tasty-terminal-link` 로 내려 gui 없이 컴파일된다. `open_uri`(OS 에 URI 를 넘기는
동작)는 `src/adapters/ui/terminal_link.rs` 에 남고, 그 파일은 크레이트를 `pub use` 로
재수출하는 shim 이 된다. 기존 `gui` 게이트 위치는 그대로다.

호출부는 계속 `terminal_link::` 하나로 둘 다 부른다 — 검출도 여는 것도 같은 이름으로
간다.

## Consequences

- **얻은 것**: 렌더러가 UI adapter 가 아니라 크레이트를 본다. 검출 로직이 헤드리스에서
  컴파일·시험되고, 브라우저 의존은 종전대로 `gui` 뒤에 있다. 이 모듈을 부르는 본체
  파일 열하나가 한 줄도 안 바뀌었다(선언 자리와 shim 자신은 뺀 수).
- **잃은 것**: 한 개념이 두 파일에 걸친다. "터미널 링크" 를 찾는 사람은 크레이트와 shim
  둘을 봐야 하고, 경로 인용도 둘로 갈린다. **이 갈래는 가드가 못 잡는다** — shim 파일이
  여전히 존재하므로 옛 경로 인용은 좌표가 해결되면서 엉뚱한 내용을 가리킨다(없어졌으면
  `cited_coordinates_exist` 가 잡았을 것이다). 실측으로 그런 자리가 둘이었다:
  기능 문서 쪽은 검출을 가리키고 있어 크레이트로 고쳤고, ADR 본문 쪽은 템플릿의 "좌표
  예외" 가 결정 시점 좌표의 갱신을 금지하므로 그대로 뒀다.
- **운영 비용 / 유지 부담**: 링크에 연산을 더할 때 "이것은 계산인가 부수효과인가" 를 먼저
  판정해야 한다. 그 판정을 대신해 주는 채널은 없다.

## Alternatives Considered

- **A: `open_uri` 까지 크레이트로 옮기고 `webbrowser` 를 크레이트 의존으로 선언한다** —
  크레이트가 워크스페이스 멤버라 헤드리스 조합에서도 컴파일되고, 그 의존이 헤드리스
  빌드로 들어온다. 헤드리스가 브라우저를 여는 일은 없으므로 순수한 손실이다. 기각.
- **B: 크레이트에 `gui` feature 를 만들어 `open_uri` 를 그 뒤에 둔다** — 의존 방향으로는
  된다. 그러나 잎 크레이트에 `gui` 라는 이름의 feature 가 생기는데 그 크레이트는 GUI 를
  모른다. ADR-0319 가 같은 이유로 같은 모양을 기각했다 — 이름이 뜻을 잃는 자리를 새로
  만들지 않는다.
- **C: 파일을 그대로 두고 타입만 크레이트에 복제한다** — 렌더러와 view 가 서로 변환해야
  하고, 두 벌이 조용히 갈라진다. 기각.
- **D: `ui` 모듈의 gui 게이트를 풀어 파일 전체를 헤드리스에서도 컴파일한다** — 이 티켓의
  범위를 훨씬 넘고, `ui` 트리의 나머지가 실제로 egui 에 매여 있다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `webbrowser` 가 루트 `Cargo.toml` 에서 `optional` 을 잃거나 `gui` feature 목록에서
  빠지면, 가른 이유가 사라진다. 그때는 `open_uri` 도 크레이트로 내린다.
- shim 에 `open_uri` 말고 다른 것이 쌓이기 시작하면 경계가 부수효과가 아닌 다른 것으로
  움직인 것이다. 그 파일의 `pub fn` 수가 좌변이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 크레이트 쪽에 부수효과가 섞여 들어갔는가. 재는 법: 크레이트의 의존 목록에 OS·프로세스·
  네트워크를 부르는 것이 있는지 본다(`cargo tree -p tasty-terminal-link --edges normal
  --prefix none`). 지금은 `regex` · `termwiz` 와 tasty 크레이트 셋(`tasty-cell-width` ·
  `tasty-terminal` · `tasty-type-appearance`)뿐이다. 컴파일이 통과하는
  것은 이 물음에 답하지 않는다.
- 크레이트에 GUI 가 들어왔는가. **직계 의존 목록을 읽는 것으로는 이 물음에 답하지 못한다**
  — 직계 의존이 자기 **기본 feature** 로 GUI 를 끌어오면 이름이 목록에 안 보인다. 좌변은
  직계가 아니라 **폐포**다. 재는 법:

  ```bash
  cargo tree -p tasty-terminal-link --edges normal --prefix none | sort -u \
    | grep -icE '^(egui|winit|wgpu|eframe|glow|epaint|ecolor|emath|tasty-ui-|tasty-egui|tasty-font|tasty-icons)'
  ```

  실측 2026-09-20: 이 크레이트 **0** · 자매 `tasty-file-handler` **0** ·
  `tasty-selection` **0**, 같은 술어를 본 바이너리 폐포에 걸면 **28** — 술어가 0 만 내는
  것은 증거가 아니므로 판별력을 그 대조로 보인다. 값이 0 인 것은 `tasty-type-appearance`
  를 `default-features = false` 로 받기 때문이다(그 플래그를 빼면 같은 명령이 **5**).

## References

- 자매 결정(같은 회차, 게이트가 **낡은** 경우): [ADR-0319](0319-the-selection-model-drops-the-stale-gui-gate-instead-of-carrying-it.md)
- 크레이트 분할이 의존 방향을 따른다는 결정: [ADR-0089](0089-crate-split-follows-dependency-direction.md)
- 기능 서술: [terminal-link](../features/terminal-link/index.md)
- 코드 근거(결정이 실현된 **현재 위치**): `tasty-terminal-link` 크레이트의
  `longest_existing_selection_path` · `LinkHighlight`, 그리고 shim 쪽의 `open_uri`
  (`src/adapters/ui/terminal_link.rs`)
