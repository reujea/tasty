# ADR-0261: busy 는 순간값이 아니라 상태다 — 입력은 진입만 막고, 유지는 못 끊는다

- **Status**: Accepted
- **Date**: 2026-09-11
- **Tags**: busy-indicator, terminal, sidebar, osc133, shell-integration, input-echo, adr-0002
- **Group**: terminal-input

## Context

surface 의 busy 판정은 `crates/tasty-terminal/src/accessors.rs` 의 `busy_with_foreground`
하나다. 세 조건의 AND 로 답한다.

1. PTY foreground 가 셸 자신도, 알려진 셸 이름도 아니다
   (`crates/tasty-terminal/src/foreground_process.rs` 의 `is_known_shell_name`).
2. 마지막 PTY 출력이 `BUSY_OUTPUT_WINDOW`(2 초) 안이다.
3. 그 출력이 사용자 입력의 에코가 아니다 — `last_output_at <= last_input_at +
   INPUT_ECHO_WINDOW`(200 밀리초) 이면 busy 가 아니라고 답한다.

`last_input_at` 을 갱신하는 것은 `crates/tasty-terminal/src/io.rs` 의 `write_input`
하나이고, 그 경로로 들어오는 것은 키 입력·`send_key`·`send_bytes`·마우스 리포트다.
판정 결과는 `src/core/state/busy.rs` 의 `refresh_busy_surfaces` 가 1Hz 로 모아
`busy_surfaces` 에 담고, 거기서 사이드바 워크스페이스 dot(`src/adapters/ui/sidebar/view.rs`)
과 탭 dot, `surface.list` 의 `busy`, `annotate_tree_busy` 가 붙이는 `busy_count` 로
그대로 나간다. 즉 조건 (3) 하나가 사용자가 보는 모든 실행 표시를 정한다.

조건 (3) 의 부등식에는 **시간 축의 방향이 없다.** 읽히는 뜻은 "출력이 입력 *직후*의
창 안에 있다" 인데, 실제로 비교하는 것은 "출력 시각이 입력 시각 + 200 밀리초 이하" 다.
그래서 결과가 둘로 갈린다.

**갈래 A — 입력 이전의 출력이 통째로 무효화된다.** `last_output_at` 이 `last_input_at`
보다 **앞선 경우에도** 부등식이 참이다. 프로그램이 0.5 초 전에 출력을 냈다면 조건 (2)
의 2 초 창 안이라 busy 여야 하는데, 사용자가 키를 하나 누르는 순간 `과거출력 <= 지금 +
200 밀리초` 가 참이 되어 **에코가 하나도 발생하지 않아도 즉시 idle** 이 된다.

**갈래 B — 연속 타이핑이 억제 창을 영구히 열어둔다.** 타자 간격이 200 밀리초보다
짧으면 `last_input_at` 이 계속 앞으로 밀려 부등식이 항상 참이다. 그 사이 프로그램이
아무리 출력을 뱉어도 타이핑하는 내내 idle 이다.

갈래 B 는 이 레포가 **다른 write 경로에서 이미 결함으로 인정하고 고친 형태**다. 같은
파일의 `send_terminal_response` 가 `last_input_at` 을 일부러 안 건드리는 이유가 그
함수의 doc 주석에 그대로 적혀 있다 — 200 밀리초보다 짧은 주기로 질의를 쏘는 TUI 가
억제 창을 영구히 열어둬, 출력이 끊임없이 흐르는데도 실행 내내 `busy == false` 가 되기
때문이다. 같은 논리가 사람의 연속 타이핑에 그대로 적용되는데 그쪽만 막혀 있지 않다.

### 이것은 코드가 문서를 어긴 것이 아니다

[`design/policies/busy-indicator.md`](../design/policies/busy-indicator.md) 의 조건 (3) 은
"마지막 PTY 출력이 마지막 사용자 입력 + `INPUT_ECHO_WINDOW` 이후" 라고 적는다. busy
조건으로 풀면 `last_output_at > last_input_at + 200 밀리초` — 코드와 글자 그대로 같다.
같은 문서가 갈래의 귀결도 예시로 적어둔다: `claude` 프롬프트에 타이핑 중이면 idle,
마우스 트래킹 TUI 위에서 마우스만 움직여도 idle.

그러니 이 ADR 이 여는 것은 버그 수정이 아니라 **정책 전환**이다. 어긋난 것은 코드와
문서가 아니라, 조건 (3) 의 명분("에코가 아니다")과 그 조건이 실제로 하는 일("입력이
있었으면 그 이전 출력까지 무효")이다. ADR 없이 코드만 고치면 그때 비로소 문서와 코드가
갈린다.

### 조건 (3) 이 실제로 일하는 자리

조건 (1) 이 먼저 `foreground == 셸` 을 걸러내므로, **셸 프롬프트에서 타이핑하는 경우는
(3) 에 도달조차 하지 않는다.** (3) 이 일하는 순간은 non-shell foreground 가 떠 있을 때
키를 치는 경우다 — `vim`·`claude` 같은 TUI 가 대표적이지만 그것만이 아니다. `cat` 처럼
stdin 을 읽으며 그대로 에코하는 평범한 프로그램도 같은 자리에 있다.

뒤집으면 **(3) 을 통째로 없앴을 때 생기는 유일한 회귀는 "non-shell 프로그램을 띄워두고
타이핑하면 dot 이 켜진다"** 하나이고, 이 결정이 답해야 하는 것은 정확히 그 하나를 어떻게
다루느냐다.

## Decision

**busy 는 순간값이 아니라 상태다. 입력은 idle → busy 진입만 막고, busy → idle 로는 밀지
못한다.** 조항 넷으로 못 박는다.

**조항 1 — 해제 권한은 조건 (1)·(2) 에만 있다.** busy 인 surface 가 idle 로 돌아가는
사건은 둘뿐이다: 출력이 `BUSY_OUTPUT_WINDOW` 동안 정적이거나(조건 2), foreground 가
셸로 복귀하거나(조건 1). 입력은 해제 사유가 아니다.

**조항 2 — 진입 억제는 방향을 갖는다.** 에코라면 출력이 입력 **이후**여야 하므로, 억제는
`last_input_at <= last_output_at <= last_input_at + INPUT_ECHO_WINDOW` 인 구간에만
적용한다. 이 조항을 빼면 "idle 에서 시작한 빌드가 타이핑 때문에 영영 busy 로 진입하지
못하는" 경로(갈래 A)가 조항 1 아래에서도 그대로 남는다.

**조항 3 — 판정 순서를 고정한다.** 조건 (1) → 조건 (2) → 입력 억제 순으로 보고, **앞의
둘이 먼저 해제 권한을 갖는다.** 입력 억제를 출력 만료보다 먼저 보면, 유지 상태가 "출력이
이미 죽은 surface" 를 입력만으로 붙잡아 둘 수 있다.

**조항 4 — 추정 갈래는 유지 상태를 만들지 않는다.** `busy_with_foreground` 의 `try_lock`
이 `WouldBlock` 이면 락 없이 `true` 로 끝난다. 그 `true` 는 관측이 아니라 **추정**이다 —
파서가 락을 쥐고 있으니 ingest 중일 것이라고 미루어 보는 값이고, 그 처리의 근거는
[ADR-0002](0002-vte-parsing-off-input-thread.md) 다. 이 값은 **그 tick 의 보고로만** 쓰고,
"직전이 busy 였는가" 라는 유지 상태의 근거로는 쓰지 않는다. 그러지 않으면 한 번 락 경합에
걸린 surface 가 이후 타이핑 에코만으로 busy 를 이어갈 수 있다.

이 결정이 **동시에** 만족시키는 두 케이스가 채택 이유다.

- `vim` 을 열어두고 타이핑을 시작한다 → 직전이 idle 이므로 진입이 막혀 **idle 유지**
  (기존 정책이 지키려던 것).
- `claude` 가 응답을 흘리는 중에 다음 질문을 타이핑한다 → 직전이 busy 이므로 **busy 유지**
  (지금 깨져 있는 것).

검토한 후보 중 이 둘을 동시에 만족시키는 것은 이 안뿐이다 — 아래 대안 A~D 는 모두 한쪽을
포기한다.

이 결정은 [`design/policies/busy-indicator.md`](../design/policies/busy-indicator.md) 의
판정 절 조건 (3) 과 그 절의 예시 두 줄(`claude` 프롬프트 타이핑 · 마우스 트래킹 TUI 위
마우스 이동)을 **무효화한다.** 그 문서의 갱신은 이 결정을 코드로 옮기는 후속 작업이 같은
회차에 한다.

## Consequences

- **얻은 것**: 응답을 흘리는 에이전트·빌드 위에서 타이핑해도 dot 이 꺼지지 않는다. 갈래
  A(입력 하나가 과거 출력을 무효화)와 갈래 B(연속 타이핑이 억제 창을 영구히 연다)가 함께
  닫힌다. 조건 (3) 의 명분과 실제 동작이 일치하게 되어, 정책 문서가 코드를 읽지 않고도
  옳게 읽힌다.
- **잃은 것**: 직전이 busy 였던 surface 위에서 타이핑하면 타이핑하는 동안에도 dot 이 켜져
  있다. 이것은 오탐이 아니라 결정의 내용이다(직전 busy 는 프로그램이 실제로 출력을 냈다는
  관측이다). 다만 사용자가 보는 그림은 "내가 치는 동안 켜져 있다" 로 같으므로, **유지
  구간의 길이가 곧 체감 오탐의 길이**가 된다 — 그 길이를 정하는 것은 `BUSY_OUTPUT_WINDOW`
  이고, 그래서 그 상수가 아래 재검토 조건에 들어간다.
- **운영 비용 / 유지 부담**: 판정이 상태를 갖게 되므로 "직전이 busy 였는가" 를 담을 자리가
  하나 생긴다. 저장 위치·형태·수명(예: surface 소멸 시 정리)은 이 ADR 이 정하지 않고 구현이
  정한다. 대신 조항 4 가 그 자리에 들어가면 안 되는 값(추정 갈래의 `true`)을 못 박는다.
- **영향 범위**: `last_input_at` 의 소비자는 이 판정 하나뿐이다 — 정의는 `Terminal` 의 상태
  구조체, 쓰기는 `write_input`, 읽기는 `busy_with_foreground` 로 각각 한 자리씩이다. 이름이
  비슷한 `should_suppress_cursor_during_output` 은 `last_screen_control_at` 만 보고
  `last_input_at` 은 읽지 않는다. 그러므로 이 필드의 뜻을 "마지막 사용자 입력 시각" 에서
  "진입 억제 창의 시작점" 으로 좁혀도 다른 기능에 파급이 없다.

## Alternatives Considered

- **A — 사건 축: 입력 이후의 "출력 횟수/바이트" 로 에코를 가른다.** 출력 진입점이
  `crates/tasty-terminal/src/lib.rs` 의 `ingest` 하나뿐이라 `outputs_since_input` 류의
  카운터를 두는 것 자체는 쉽다. 기각 사유: chunk 경계가 PTY read 크기에 좌우돼
  **비결정적**이고, TUI 는 키 하나에 전체 화면 repaint 를 수 KB·여러 chunk 로 낸다. 그래서
  임계값이 재현 가능한 근거를 갖지 못한다 — 같은 타이핑이 머신·부하에 따라 다른 쪽으로
  판정된다.
- **B — 내용 매칭: 보낸 입력 바이트와 나온 출력을 대조한다.** `write_input` 이 바이트를
  들고 있어 링버퍼 보관은 가능하다. 기각 사유: 위 "조건 (3) 이 실제로 일하는 자리" 때문에
  대상이 non-shell foreground 뿐인데, TUI 는 입력을 원문 에코로 돌려주지 않는다(커서 이동
  + 부분 재그리기 + SGR). 대조가 성립하는 케이스가 거의 안 남아, 비용을 들여도 판정이 거의
  안 바뀐다.
- **C — 액션 종류 축: `is_screen_repaint_action` 을 재사용한다.** 이미 있는 축이고
  (`crates/tasty-terminal/src/lib.rs`), Windows 커서 억제가 쓰고 있다. 기각 사유: **방향이
  반대다.** TUI 는 타이핑에도 커서 제어를 낸다 — 이 축을 쓰면 타이핑이 오히려 busy 로
  분류된다.
- **D — OSC 133 명령 구간을 권위 판정으로 쓴다.** 셸 통합이 emit 하는 OSC 133 은 이미 끝까지
  파싱되고(`crates/tasty-terminal/src/vte_handler/osc.rs` → `src/core/impl_pty.rs` →
  `src/core/command_index.rs` 의 `on_boundary`), 명령 시작(`C`) 에서 채워져 종료(`D`) 에서
  리셋되는 자리가 있어 **"명령이 실행 중인가" 라는 정보가 이미 호스트에 있다.** 설치 여부
  판별도 `src/core/state/shell_integration_hint.rs` 의 `shell_integration_boundary_seen`
  으로 이미 존재한다. "셸이 직접 알려주는 사실이니 휴리스틱보다 앞선다" 는 논리로 채택
  직전까지 갔다.

  기각 사유: **그 구간은 `cargo build` 와 `vim` 을 구분하지 못한다.** 셸에서 `vim` 을 띄우면
  셸 통합이 `C` 를 내고 vim 이 도는 내내 `D` 는 오지 않는다 — 대화형 프로그램도 **구간 안**
  이다. 그 구간에서 입력 억제를 면제하면 vim 안에서 타이핑한 에코가 조건 (2) 를 충족시켜
  **idle 이어야 할 surface 가 busy 로 진입한다.** 이 결정이 지키려는 첫째 케이스를 정면으로
  깨는 것이다. "조건 (2) 를 AND 로 유지하면 막힌다" 는 반론은 성립하지 않는다 — 타이핑
  에코 자체가 조건 (2) 를 충족시키기 때문이다. `C` 페이로드의 명령 텍스트로 대화형 여부를
  가르는 변형도 결국 **이름 기반 분류**라 조건 (1) 이 이미 하는 일과 같고, 새 정보를 주지
  않는다.

  **다시 파지 않도록 결론을 남긴다**: OSC 133 이 이 문제에 주는 유일한 새 정보는 "프롬프트로
  돌아왔다"(`D` 도착) 이고, 그것은 조건 (1)("foreground 가 셸") 과 거의 동치다. 1Hz 폴링
  지연 없이 즉시 알 수 있다는 점과, Windows 의 근사 판정(ConPTY 가 foreground PGID 를
  미노출 — 정책 문서의 플랫폼 표)을 보완할 수 있다는 점에서 **별개 작업으로서의 가치는
  남는다.** 그러나 이 ADR 이 다루는 문제(타이핑이 busy 를 끄는 것)의 해법은 아니다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `last_input_at` 을 읽는 자리가 `busy_with_foreground` 말고 생긴다. 위 "영향 범위" 는
  소비자가 하나라는 관측 위에 서 있고, 둘째 소비자가 생기는 순간 "이 필드의 뜻을 좁혀도
  파급이 없다" 는 전제가 깨진다. 읽는 법: `crates/tasty-terminal/src` 아래에서 그 식별자를
  훑어 쓰기 한 자리(`write_input`)와 읽기 한 자리(`busy_with_foreground`) 말고 더 있는지
  센다. **판정기는 안 지었다** — 좌변이 0 에서 1 로 움직이는 사건이고, 이 필드를 새로 읽는
  코드를 쓰는 사람은 그 이름을 훑으며 반드시 이 자리를 지난다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 이 결정 아래에서는 `BUSY_OUTPUT_WINDOW`(2 초)가 **유지 구간의 길이**를 정한다. 출력이
  드문드문한 프로그램에서 dot 이 깜빡인다는 관측이 모이면 그 상수를 다시 잰다. 재는 법:
  출력 간격이 2 초를 넘나드는 프로그램(링크 단계에 들어간 빌드, 느린 네트워크 스트림)을
  띄워놓고 `surface.list` 의 `busy` 를 1Hz 로 30 초 샘플링해 idle↔busy 전이 횟수를 센다.
  같은 프로그램의 한 번의 실행이 여러 번 전이하면 상수가 짧다는 뜻이다.
- 기각한 대안 D 는 **"대화형 여부를 알 수 있는 신호가 생기면"** 재검토 대상이다. 기각
  사유가 "OSC 133 구간이 `cargo build` 와 `vim` 을 못 가른다" 하나뿐이라, 그 구분이 가능해지는
  순간 사유가 통째로 사라진다. 재는 법: 셸 통합 스크립트가 내는 OSC 133 `C` 페이로드의 키를
  수집해, 이름이 아닌 축(TTY 소유권 이전·raw mode 전환 같은 것)이 실려 오는지 본다.

## References

- 코드 근거(결정 시점의 기록 — 이 결정이 바꾸려는 자리다):
  `crates/tasty-terminal/src/accessors.rs` 의 `busy_with_foreground`,
  `crates/tasty-terminal/src/io.rs` 의 `write_input`·`send_terminal_response`,
  `crates/tasty-terminal/src/lib.rs` 의 `BUSY_OUTPUT_WINDOW`·`INPUT_ECHO_WINDOW`,
  `src/core/state/busy.rs` 의 `refresh_busy_surfaces`
- 이 결정이 무효화하는 운영 서술: [`design/policies/busy-indicator.md`](../design/policies/busy-indicator.md)
  의 "판정 — 세 조건 모두 (AND)" 절
- `try_lock` WouldBlock 을 busy 로 읽는 근거: [ADR-0002](0002-vte-parsing-off-input-thread.md)
- OSC 133 셸 통합의 현재 범위: [`features/terminal-output/index.md`](../features/terminal-output/index.md)
