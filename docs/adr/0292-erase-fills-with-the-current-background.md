# ADR-0292: 소거는 현재 배경색으로 칸을 채우고 pen 을 안 건드린다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: terminal, vte, erase, bce, sgr, termwiz, adr-0002
- **Group**: terminal-input

## Context

ED/EL 여섯 분기가 **지운 칸에 무엇을 남기는가**와 **소거 뒤 pen 을 어떻게 두는가**에서
서로 달랐다. 두 축을 여섯 분기 × 커서 행(0 · 1) × 걸침 여부로 전수 측정했다
(10 열 4 행, `CSI 41m` 을 켠 뒤 각 소거를 먹이고 `cell_attrs()` 로 읽었다).

| 분기 | 지운 칸의 배경 | 소거 직후 찍은 글자의 배경 |
|---|---|---|
| EL0 `CSI 0K` | 기본 | 기본 |
| EL1 `CSI 1K` | 빨강 | 빨강 |
| EL2 `CSI 2K` | 기본 | 기본 |
| ED0 `CSI 0J` | 기본 | 기본 |
| ED1 `CSI 1J` | 커서 행 0 이면 빨강 · 행 1 이면 기본 | 같음 |
| ED2 `CSI 2J` | 기본 | 기본 |

세 무리였고, 그중 하나는 **같은 명령인데 커서 행 번호에 따라 갈렸다**. 원인은 구조다 —
termwiz `Surface` 의 `clear_*` 는 칸을 `CellAttributes::default().set_background(color)` 로
채우고 자기 `attributes` 를 그 값으로 리셋하는데, `Change::Text` 는 현재 pen 으로 찍는다.
`io.rs` 의 `mirror_pen` 이 앞쪽만 따라가므로, 같은 "소거" 를 두 수단으로 구현한 분기들이
서로 다른 부수효과를 냈다. ED1 은 한 명령 안에서 두 수단을 **순서대로** 쓴다 — 위 행들을
`ClearToEndOfLine` 로 지운 뒤 커서 행을 `Text` 로 찍으므로, 위에 행이 있으면 pen 이 이미
리셋된 뒤라 커서 행이 기본 배경이 된다.

무엇을 따를지 정하는 데 근거가 셋 있었다.

- **tasty 가 선언한 능력.** `crates/tasty-terminal/src/lib.rs` 가 자식 프로세스에
  `TERM=xterm-256color` 를 준다. 그 terminfo 엔트리는 `bce` 를 선언한다(측정:
  `infocmp -1 xterm-256color | grep bce` → `bce,`). 즉 애플리케이션에게 back color erase
  를 한다고 말해 놓고 여섯 중 넷에서 안 했다.
- **규격.** ECMA-48 은 소거된 칸을 "erased state" 로만 정하고 **색을 정하지 않는다.**
  그래서 색 축은 규격이 아니라 xterm 관례가 정하고, terminfo `bce` 가 그 관례를 선언하는
  자리다. 범위 축(어느 칸을 지우는가)만 규격이 정한다.
- **독립 구현.** tmux 3.4 를 격리 소켓의 10×4 패널로 띄우고 패널 tty 에 바이트를 직접 써
  넣어 `capture-pane -e` 로 읽었다. 여섯 분기 전부 **지운 칸이 현재 배경색이고 pen 이
  보존**됐다 — ED1 의 커서 행 의존은 tmux 에 **없었다**(커서 행 1 에서 위 행도 커서 행도
  빨강이고 그 뒤 글자도 빨강이다). 그리고 `CSI 4;41m` 으로 밑줄까지 켠 뒤 `CSI 1K` 를
  먹이면 지운 칸은 **빨강만 갖고 밑줄이 없다** — 옮겨지는 것은 배경색 하나다.

## Decision

여섯 분기를 하나의 규칙으로 통일한다.

1. **지운 칸은 기본 속성 + 소거 시점의 배경색을 갖는다.** 배경색 하나만 옮긴다 —
   밑줄·역상·굵기 같은 나머지 속성은 지워진 칸에 남지 않는다.
2. **소거가 끝난 뒤 pen 은 소거 전 그대로다.** 소거 명령은 렌더링 속성을 바꾸지 않는다.

구현은 `vte_handler/edit.rs` 한 자리에 모인다. 지우기 primitive 에는
`ColorAttribute::Default` 대신 `erase_color()`(현재 pen 의 배경)를 넘기고, `Change::Text`
로 지우는 갈래는 찍기 직전에 `Change::AllAttributes(erase_attrs())` 로 소거 pen 을 세우며,
모든 갈래가 끝에 `restore_pen()` 을 붙여 pen 을 되돌린다. `mirror_pen` 은 그대로 둔다 —
`Clear*` 가 미러를 리셋하는 것은 termwiz 의 실제 동작이라 미러가 따라가는 것이 맞고,
되돌리는 일은 뒤따르는 `AllAttributes` 가 한다.

**이 결정이 걸친 커서의 잔여도 함께 닫는다.** 걸친 자리(`cx == cols`)로 돌아가는 유일한
길은 마지막 열에 한 칸을 찍는 것인데, 그 칸의 pen 이 나머지와 달라지는 것이 한때 그 길을
막고 있었다. 소거 pen 이 하나로 정해지면 그 칸도 같은 pen 이라 막을 이유가 없다 — EL0·ED0
의 범위 결함(걸친 커서에서 한 칸도 안 지움)과 EL2·ED2 의 걸침 유실이 같은 변경에서 닫힌다.

## Consequences

- **얻은 것**: 여섯 분기 × 커서 행 × 걸침 = 24 조합이 하나의 규칙이다. `TERM` 이 선언한
  `bce` 와 실제 동작이 일치한다. 배경색을 켠 채 줄 끝을 지우는 TUI(진행바·상태줄)에서
  지운 구간만 배경이 끊기던 증상이 사라진다. 걸친 커서의 두 잔여가 함께 닫혔다.
- **잃은 것**: 소거마다 `Change` 가 하나(걸침 갈래는 셋) 늘어난다. 그리고 tmux 와
  **한 자리에서 갈라진다** — 걸친 커서의 EL0·ED0 에서 tmux 는 한 칸도 안 지우고 tasty 는
  커서가 올라앉은 칸을 지운다. 그 갈림은 색 축이 아니라 범위 축이고, 이 저장소가 EL1·ED1
  에서 이미 지키던 "걸친 커서는 마지막 열로 친다" 를 EL0·ED0 에도 적용한 결과다.
- **운영 비용 / 유지 부담**: `erase_pen.rs` 가 24 조합을 표로 재고, 음성 대조(배경을 안
  켜면 기본)와 밑줄 축(배경만 옮긴다)을 따로 단언한다. 새 소거 분기를 더하면 그 표에
  행을 더해야 한다.

## Alternatives Considered

- **A: non-BCE 로 통일하고 `TERM` 을 바꾼다** — `bce` 를 선언하지 않는 엔트리(예:
  `xterm` 계열이 아닌 것)를 주면 선언과 동작이 다시 맞는다. 안 골랐다: `TERM` 은 색 수·
  키 시퀀스·마우스 보고까지 함께 정하는 값이라 이 한 축 때문에 바꾸면 훨씬 넓은 것이
  따라 움직이고, 애플리케이션이 받는 기능이 줄어든다. 그리고 독립 구현이 반대 방향이다.
- **B: 현재 동작을 유지하고 분기별 규칙을 문서·시험에 명시한다** — 티켓이 유효한 결론으로
  열어 둔 갈래다. 안 골랐다: 명시해야 할 규칙이 "EL1 은 BCE, ED1 은 커서 행이 0 일 때만
  BCE, 나머지는 non-BCE" 가 되는데, 그 분기 조건(커서 행 번호)은 어떤 규격에서도 나오지
  않는 구현 부수효과다. 적는 순간 그것이 계약이 된다.
- **C: 지운 칸에 현재 pen 을 통째로 찍는다** — `Change::Text` 갈래가 이미 그렇게 동작하고
  있었으므로 가장 적은 변경이다. 안 골랐다: 그러면 밑줄을 켠 채 지운 칸에 밑줄이 남는다.
  두 규칙은 배경만 켠 상태에서 **같은 값을 내므로** 표만으로는 안 갈리고, tmux 실측이
  갈랐다(`CSI 4;41m` + `CSI 1K` → 지운 칸에 밑줄 없음).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 자식에게 주는 `TERM` 이 `bce` 를 선언하지 않는 값으로 바뀐다. 그러면 이 결정의 첫째
  근거가 사라진다 — 선언과 동작을 다시 맞춰야 한다.
- `DECSCNM`(역상 화면, `CSI ?5h`)이 구현된다. 그 모드에서는 "현재 배경색" 이 무엇인지가
  한 겹 더 갈리고, xterm 은 역상 화면에서 소거 색을 따로 다룬다. 지금 이 저장소에는 그
  모드가 없어 그 갈래를 정하지 않았다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 걸친 커서의 EL0·ED0 에서 tmux 와 갈라진 것이 실제 애플리케이션에서 눈에 보이는 차이를
  만드는 경우. 재는 법: 그 TUI 를 tasty 와 tmux 에서 나란히 띄우고 같은 출력 뒤 화면을
  비교한다. 갈림이 나는 조건은 "정확히 폭만큼 찍은 직후 `CSI 0K`/`CSI 0J`" 라 좁다.

## References

- 동작 명세와 표: [docs/features/terminal/index.md](../features/terminal/index.md)
- 파서·VTE 구조: [ADR-0002](0002-vte-parsing-off-input-thread.md)
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-terminal/src/vte_handler/edit.rs` 의
  `erase_attrs` · `erase_color` · `restore_pen` · `park_on_erased_last_cell`
- 시험: `crates/tasty-terminal/tests/erase_pen.rs`(색 축) ·
  `crates/tasty-terminal/tests/erase_boundaries.rs`(범위 축)
