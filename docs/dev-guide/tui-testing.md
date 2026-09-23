# TUI 테스트 가이드

터미널 에뮬레이션 버그를 결정적으로 재현·검증하는 방법. E2E 격리/timeout 정책은 [e2e-tests](e2e-tests.md).

## 원칙 — 재현 먼저

TUI 버그 발견 시: ① 최소 재현 VTE 시퀀스 특정 → ② `tasty-tui-sim` 에 시나리오 추가 → ③ `debug.cell_info`/`debug.screen_attrs` 로 정상 상태를 검증하는 E2E 테스트 작성 → ④ 버그 상태에서 테스트가 **실패하는지 확인** → ⑤ 수정 후 통과. "수정 먼저, 테스트 나중" 금지.

## tasty-tui-sim (VTE 시뮬레이터)

`crates/tasty-tui-simulator/` — 고수준 명령("cursor 5 3", "bold", "print hello")을 raw VTE escape 로 변환해 출력한다. 터미널 입장에선 실제 TUI 앱과 동일한 바이트 스트림. 테스트 전용이 아닌 독립 도구(`cargo build -p tasty-tui-simulator`).

로직은 `lib.rs` 에 있고 두 진입점이 공유한다(SoT 하나):

- 독립 바이너리 `tasty-tui-sim` — release 빌드 가능. PATH 에서 어느 surface 든 실행(부하 테스트는 보통 release 본체 대상이라 이쪽).
- `tasty debug sim <subcommand>` — debug 빌드 한정. **별도 빌드/PATH 설정 없이** 바로 호출 가능(이미 `tasty` 가 PATH 에 있으므로). surface 안에서 stdout 에 직접 VTE 를 뿜는 로컬 동작(IPC 미경유). `tasty debug sim flood` 가 화면 갱신 부하(full-screen truecolor redraw)를 거는 스트레스 모드.

### 인터랙티브 모드 (핵심)

서브커맨드 없이 실행하면 stdin REPL. E2E 가 `surface.send` 로 명령을 한 줄씩 보내 터미널 상태를 단계 구성한다. 시작 시 `READY`, 매 명령 후 `OK` 출력 → 테스트는 `wait_for_output("OK")` 로 동기화.

```bash
tasty-tui-sim   # = tasty-tui-sim interactive
```

### 명령 카테고리

raw escape 직접 출력이 목적이라 crossterm/ratatui 같은 추상화는 쓰지 않는다. 전체 목록은 `crates/tasty-tui-simulator/src/lib.rs`. 주요 카테고리:

- **화면/커서**: `clear` `reset` `cursor <r> <c>` `cursor-{up,down,left,right} [N]` `cursor-save`/`-restore`
- **텍스트**: `print`/`println <text>` `newline` `cr` `tab` `bell`
- **SGR**: `sgr <params>` `bold` `italic` `underline[-double|-curly|-dotted|-dashed]` `underline-color` `strikethrough` `inverse` `dim` `blink[-rapid]` `invisible` `overline` `fg <N|r;g;b>` `bg <…>` + `*-off`
- **지우기/스크롤**: `erase-display [N]` `erase-line [N]` `scroll-region <top> <bottom>` `scroll-{up,down} [N]`
- **모드**: `altscreen-enter`/`-exit` `decset/decrst <mode>` `mouse-track[-motion|-all]` `size`
- **raw**: `raw <hex>` `esc <seq>`
- **프리셋**: `scenario {cursor,colors,attrs,unicode,scroll-region}`
- **종료**: `quit`/`exit` (`BYE`) `exit-code <N>` `crash`(SIGABRT) `panic`

원샷(`tasty-tui-sim cursor --row 5 --col 10 --exit` 등)은 수동 눈 확인용 — `--exit` 없으면 키 대기. E2E 는 인터랙티브 모드 사용.

## 디버그 IPC (debug 빌드 전용, `#[cfg(debug_assertions)]`)

### `debug.cell_info` — termwiz 파싱 단계 셀 속성

`tasty debug cell-info --row 0 --col 0`. termwiz `CellAttributes` 단계를 그대로 노출 — **렌더러가 GPU 에 반영했는지는 `debug.glyph_color` 로 별도 확인.** 필드: `text` `fg`/`bg`(`default`/`palette:N`/`#rrggbb`) `bold` `italic` `underline` `strikethrough` `inverse` `width`(1/2) `intensity`(normal/bold/half) `underline_style` `underline_color` `blink` `invisible` `overline` `vertical_align`.

> `overline`/`underline_color`/`vertical_align` 은 termwiz `CellAttributes` 엔 있으나 `AttributeChange` enum 에 variant 가 없어 현재 SGR 파이프라인(`crates/tasty-terminal/src/vte_handler.rs`)이 셀에 전달하지 못한다 — SGR 로 입력해도 기본값 반환. 검증 인프라만 준비된 상태.

### `debug.glyph_color` — 렌더러가 GPU 에 push 하는 색

`tasty debug glyph-color --row 0 --col 0 [--bg-mode unfocused] [--surface 3]`. 응답: `in_bounds` `bg_mode`(focused/unfocused) `default_bg`/`bg`/`fg`(각 `{r,g,b,a,hex}`). `compute_cell_colors`(`src/cell_palette.rs`)가 GPU 인스턴스 색의 단일 출처 — 렌더러와 `debug.glyph_color` 가 같은 함수를 쓰므로 누락된 SGR 처리가 즉시 드러난다. (faint/dim 회귀: cell_info `intensity == "half"` → glyph_color 에서 fg 가 어둡게 적용됐는지 비교.)

### 입력 시뮬레이션 (debug + `--enable-input-simulation`)

2단계 게이트: `#[cfg(debug_assertions)]` + `--enable-input-simulation` 플래그(없으면 "input simulation not enabled" 거부). `debug.inject_mouse`(SGR 마우스: surface_id/col/row/button/event_type) · `debug.inject_key`(text 또는 hex bytes).

## E2E 테스트 패턴

```rust
tasty.set_mark(sid);
tasty.send_text(sid, "tasty-tui-sim\n");
tasty.wait_for_output(sid, "READY", Duration::from_secs(5));
tasty.send_text(sid, "print 한글\n");
tasty.wait_for_output(sid, "OK", Duration::from_secs(2));
let cell = tasty.call("debug.cell_info", json!({ "surface_id": sid, "row": 0, "col": 0 }));
assert_eq!(cell["text"], "한");
assert_eq!(cell["width"], 2);
tasty.send_text(sid, "quit\n");
tasty.wait_for_output(sid, "BYE", Duration::from_secs(2));
```

같은 프로세스에서 여러 단계 연속 수행 가능. 프리셋 시나리오의 기대 출력(cursor/colors/attrs/altscreen/unicode/scroll-region 의 행·열별 값)은 `tasty-tui-sim` 소스의 각 `scenario_*` 함수가 SoT — 검증값은 거기서 읽는다.

## 골든 셀 그리드 스냅샷 (`cargo test`, headless)

E2E 와 달리 **GUI surface 없이** 도는 결정적 회귀 가드. `crates/tasty-terminal/tests/golden_grid.rs` 가 production VTE ingest 경로(`Terminal::new_detached` → `feed_bytes`, 실제 PTY 와 동일한 핸들러)에 `tasty-tui-sim` 의 고수준 명령에 대응하는 raw escape 를 먹이고, 결과 셀 그리드의 **결정적 텍스트 표현**을 골든으로 고정한다. `cargo test --workspace` 에 포함되어 별도 인프라 없이 돈다 — 기본 조합의 그 잡은 수동 전용이고, 자동 실행은 `check-headless` 잡에서만 일어난다([ci-gates](ci-gates.md)).

- `grid_text()` — 가시 그리드를 행당 한 줄로(후행 공백 trim, 빈 행 보존) 덤프. 레이아웃/커서/줄바꿈/스크롤/지우기 회귀용.
- `styled_cells()` — populated 셀별 SGR 속성(bold/italic/underline/inverse/strike + palette fg/bg)을 `"{row},{col} {glyph} {flags}"` 로 덤프. SGR 적용 회귀의 정식 가드(예: bold 처리 누락 시 `0,0 B bold` → `0,0 B -` 로 골든이 깨짐).

**커버 범위(의도적으로 좁게):**

- 커버: 커서 절대 위치/레이아웃, autowrap 줄바꿈, scroll-region(DECSTBM) 스크롤업, erase-display(CSI J), per-cell SGR 속성.
- **미커버**: GPU 픽셀 렌더(환경의존 → 골든 부적합, `debug.glyph_color` 로 검증), chrome/위젯 레이아웃(`src/view` — GUI 하니스 필요). 둘 다 수동 시각 검증(`docs/ai-verification/screenshot-methods.md#시각-판정-체크리스트`) 영역으로 남는다.

픽셀이 아닌 **텍스트**를 고정하는 이유: 그리드의 텍스트 표현은 OS/GPU 무관하게 안정적이라, 로직 회귀가 골든 한 줄을 뒤집어 실패시키되 픽셀 골든처럼 flaky 하지 않다.

## 시나리오 추가

`crates/tasty-tui-simulator/src/lib.rs` 에 서브커맨드 + 함수 추가. `clear_and_setup()` 시작 → raw escape 직접 출력 → `finish(out, "{NAME}_TEST_DONE", exit)`.

## 주의

- **ratatui 금지** — raw escape 직접 출력해야 "터미널이 시퀀스를 올바로 해석하는가"를 검증할 수 있다.
- 디버그 IPC 는 release 에 없다 — E2E 는 debug 빌드 전용.
- **shell ZLE/readline 함정**: E2E 하네스는 격리 config 에 `shell = "/bin/sh"` 를 박고 `SHELL` 을 지우지만(`tests/common/mod.rs`), 셸을 직접 띄운 surface 나 수동 검증 인스턴스는 사용자 로그인 셸을 쓴다(`GeneralSettings::detect_shell`). `Alt+X`(execute-named-cmd), `Ctrl+R`(history-search), `Ctrl+X Ctrl+E`(edit-command-line) 등은 prompt 를 바꿔 후속 명령을 오염시킨다. shell-disruptive 키는 별도 임시 surface 에서 보내고 닫거나, `Ctrl+G`(abort)로 reset 후 진행. 증상: stripped 출력에 글자 사이 `_` 나 BEL 다수면 ZLE incremental 모드에 갇힌 것.
