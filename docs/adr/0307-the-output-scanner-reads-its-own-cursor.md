# ADR-0307: 출력 스캐너는 자기 커서로 읽는다 — 에이전트의 mark 를 공유하지 않는다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: terminal, output-buffer, ipc, plugin, claude, cursor, polling, adr-0085, adr-0266, adr-0306
- **Group**: terminal-input

## Context

`crates/tasty-terminal/src/output_buffer.rs` 는 처음부터 커서를 **둘** 선언했다. doc
주석이 `read_mark` 는 IPC mark API 의 것이고 `scan_mark` 은 출력 스캐너의 것이라고
적었다. 그런데 실측하면 `scan_mark` 을 읽고 쓰는 두 접근자(`output_since_scan_mark` ·
`set_output_scan_mark`)의 호출자가 `tasty-terminal` 크레이트 **밖에 한 곳도 없었다.**
선언된 분리가 성립하지 않는 상태였다.

스캐너가 사라진 것은 아니다. 호스트에 있던 ClaudeError 카탈로그가 plugin 으로 옮겨
가면서(`crates/tasty-plugin-claude/src/error_scan.rs`) 그 스캐너는 호스트 메모리를 직접
보지 못하게 됐고, 그래서 **IPC `surface.read_since_mark`** 로 텍스트를 받는 방식이 됐다.
추적 중인 surface 마다 `ERROR_SCAN_INTERVAL`(800 ms) 주기로 폴링한다. 그런데 그 plugin 은
`surface.set_mark` 을 **한 번도 부르지 않는다**(두 plugin 크레이트 전수 검색 0 건). 결과가
셋이다.

1. **아무도 mark 를 안 세운 surface 에서는 폴링마다 버퍼 전체를 다시 받는다.**
   `read_since_mark` 은 mark 가 없으면 `unwrap_or(0)` 으로 버퍼 처음부터 읽고, 그 버퍼의
   상한은 `OUTPUT_RETENTION_MAX_BYTES`(1 MiB) 다. 즉 최대 1 MiB 를 800 ms 마다 String 화 + ANSI
   strip + JSON 직렬화해 보낸다.
2. **에이전트가 `set_mark` 을 걸면 스캐너의 관측 창이 조용히 점프한다.** 간섭 방향은
   한쪽이다 — 읽기는 `&self` 라 mark 를 안 움직이고, **`set_mark` 하는 쪽이 안 하는 쪽을
   민다.**
3. **plugin 코드가 그 사실을 이미 우회하고 있었다.** `scan_one_at` 의 주석이 "prompt 가
   새로 그려지지 않는 한 mark 는 그대로 유지되므로 같은 chunk 가 반복 노출될 수 있다" 고
   적고, 앞 200 자 스니펫 dedupe 로 그 반복을 덮는다.

같은 `read_mark` 을 읽는 소비자는 둘이 아니라 **셋**이다 — `surface.read_since_mark` ·
`surface.parse_since_mark`(`src/adapters/ipc/handler/surface/mark.rs`) · 그리고 위 plugin
스캐너. 그리고 trim 이 mark 를 잘라내면 `read_mark` 은 `None` 이 되어 다음 읽기가 **버퍼
처음부터 돌아가는데, 응답에 그 사실을 말하는 칸이 없다.** 그 갈래를 재는 시험도 그 파일에
없었다(인파일 시험 0 개).

폴링 비용 자체가 이 저장소에서 이미 값으로 찍힌 적이 있다 —
[ADR-0085](0085-ipc-log-retention-bounded.md) 가 allow audit 를 끈 근거로 든 초당 14 건 ·
18 시간 371,936 행의 대부분이 이 종류의 주기 조회였고, 당시 처방은 **기록을 끈 것**이었다.
조회 자체는 남았다.

## Decision

**죽어 있던 `scan_mark` 을 되살려 출력 스캐너 전용 커서로 쓰고, 그 커서를 읽는 IPC
메서드를 하나 새로 더한다. 에이전트가 쓰는 세 메서드는 손대지 않는다.**

- `OutputBuffer::take_since_scan_mark(strip_ansi)` — scan 커서 이후의 출력을 돌려주고 그
  커서를 **읽은 자리 끝으로 전진시킨다.** 읽기와 전진은 **한 호출**이다. 둘로 쪼개면 그
  사이에 붙은 출력이 전진에 넘어가 **아무에게도 보고되지 않는** 창이 생긴다. 기존의
  분리된 짝(`output_since_scan_mark` + `set_scan_mark`)은 이 하나로 대체해 없앤다 — 짝이
  둘로 남아 있던 것이 애초에 그 창을 만들 수 있는 형태였고, 그 둘은 호출자가 없었다.
- IPC `surface.read_since_scan_mark` — `plugin(&[TerminalRead])`. `read_since_mark` 과
  같은 권한 버킷이다(같은 출력을 읽는다).
- **에이전트용 세 메서드(`surface.set_mark` · `surface.read_since_mark` ·
  `surface.parse_since_mark`)의 이름·파라미터·응답 모양을 바꾸지 않는다.** 그 셋은 0.7
  동결 baseline 에 등재돼 있고(`crates/tasty-ipc/fixtures/method_baseline_0_7.txt`),
  0.7.x 동안 추가만 가능하다. 이 결정은 **추가**다.
- **CLI 동사를 붙이지 않는다.** 이 커서의 읽기는 파괴적이다 — 부르는 쪽이 바이트를
  가져가고 커서가 전진한다. CLI 동사를 열면 사용자가 `tasty` 한 줄로 스캐너의 바이트를
  가로채 에러 감시에 구멍을 낼 수 있고, 그 구멍은 조용하다. 표에는 등재하므로 거부가
  **정책인지 등재 누락인지**는 구분된다(`METHOD_TABLE` 에 있고 CLI 진입점이 없는 선례가
  이미 둘 있다 — `host.shared_buffer.create` · `markdown_mirror.content_request`).
- **plugin 은 받은 델타를 자기 창에 누적한다.** 스캐너의 판정 휴리스틱(에러 패턴 매칭 ·
  출력 흐름 지문 · 200 자 dedupe · 정지 문턱)을 **하나도 바꾸지 않기** 위해서다. 창의
  상한은 `OutputBuffer` 의 상한과 같은 값을 **따로 적은 사본**이다 — plugin 은 호스트
  크레이트를 링크하지 않는다. 같은 크레이트 경계를 사이에 둔 사본이 이미 하나 있고
  (`error_scan.rs` 의 `STALL_QUIET_NO_ERROR` ↔ `src/core/state/child_liveness.rs` 의
  `CHILD_OUTPUT_SILENCE`), 그 값을 정한 것이
  [ADR-0266](0266-derived-stale-must-reach-the-push-channel.md) 결정 5 다. 사본으로 두는
  것을 그 주석이 명시하고 갈릴 때의 증상까지 적어 두었으므로 같은 형태를 따른다.

### 결정 당시 안 적은 두 가지 (2026-09-20 리뷰에서 채움)

- **`plugin_only` 로 등재하지 않은 것은 선택이 아니다.** 이 표에서 `plugin_only` 는 "부를 수
  있는 주체" 표식이 아니라 **"외부 dispatch arm 이 없다 — plugin host-call 진입부가 직접
  인터셉트한다"** 는 구조 주장이고, `src/source_guards/plugin_only_dispatch_parity.rs` 가 그
  표식과 진입부(`src/app/dispatch/plugin_ipc.rs` · `src/boot/headless_plugins.rs`)의 인터셉트
  집합을 **양방향으로** 대조한다. 이 메서드는 그 진입부가 아니라 일반 라우터 팔
  (`src/adapters/ipc/handler.rs`)이 처리하므로, `plugin_only` 로 적으면 표가 거짓이 되고 그
  가드가 그 자리에서 실패한다 — **변이로 확인했다**: 등재를 `plugin_only(&[TerminalRead])` 로
  바꾸면 그 시험이 `표는 plugin_only 라는데 plugin 진입부에 인터셉트가 없다` 로 죽는다. 그래서 위 CLI 항목이 말하는 것은 "표식으로 막는다" 가 아니라
  **"CLI 잎을 안 만든다"** 이고, 로컬 IPC 클라이언트는 여전히 이 이름을 부를 수 있다. 그것이
  아래 "잃은 것" 의 "소비자가 둘이 되면" 과 같은 사실이고, 이 결정의 e2e 왕복 검증이 성립하는
  이유이기도 하다.
- **첫 호출은 여전히 그 시점까지 쌓인 것을 통째로 나른다.** `scan_mark` 의 초기값이 0 이라
  아래 "얻은 것" 의 "폴링 1 회의 전송량" 은 **둘째 호출부터**의 값이다. surface 당 한 번이고
  스캐너가 켜지는 자리가 launch/spawn/respawn 이라 그 시점 버퍼는 실사용에서 거의 비어
  있지만, 이미 출력이 쌓인 surface 에 스캐너를 나중에 켜면 그 한 번은 상한까지 나를 수 있다.

### 재전달 분류: `Mutate` (2026-09-20, ADR-0306 이 표에 그 칸을 만든 뒤)

[ADR-0306](0306-a-method-declares-what-a-second-delivery-leaves-behind.md) 이 `MethodMeta` 에
`effect` 를 더해 모든 등재가 **"이 요청이 두 번 전달되면 관측 가능한 차이가 남는가"** 를
선언하게 했다. 이 메서드의 값은 **`Mutate`** 다.

**같은 자리의 `surface.read_since_mark` 는 `Read` 인데 왜 다른가.** 두 메서드는 같은 버퍼를
같은 권한(`TerminalRead`)으로 읽고 이름도 둘 다 read 계열이다. 갈리는 것은 **커서를 움직이는가**
하나다. `read_since_mark` 은 `&self` 로 읽어 아무 흔적도 안 남기므로 두 번째 전달이 같은
구간을 그대로 다시 준다 — `Read`. 이 메서드는 읽은 구간을 **소비한다**: 응답을 못 받은 호출자가
다시 보내면 그 사이 새로 온 두 번째 구간이 오고, **첫 응답이 나른 바이트는 어디에서도 다시
안 나온다.** 되돌릴 수단이 없다(커서를 뒤로 미는 API 가 없고, 있어도 그것은 다른 결정이다).

`Idempotent` 가 아닌 이유를 값으로 적는다. `Idempotent` 는 "두 번째 전달이 **같은 끝 상태로
수렴**한다" 다. 버퍼에 새 출력이 없는 순간만 보면 커서 위치는 두 번 다 끝이라 수렴처럼 보인다 —
그런데 그 관측은 **두 전달 사이에 상태가 안 바뀐 경우**만 본 것이고, ADR-0306 이 `set_mark` 에
대해 적은 재는 법이 정확히 그것을 금지한다("두 전달 사이에 상태를 바꾸고 그래도 끝 상태가
같은지 본다"). 출력이 한 바이트라도 도착하면 두 번째 전달은 그것을 먹고 사라지게 하므로 끝
상태가 다르다. 그래서 `Mutate` 다.

선례가 표 안에 이미 있다 — `message.read` 는 `peek` 기본값이 `false` 라 읽으면 소비하고
`Mutate` 로 등재돼 있다. ADR-0306 의 Context 가 그것을 "이름으로 추론하면 틀리는 반례" 로
들었고, 이 메서드는 같은 반례의 더 날카로운 형태다: **반례와 정례가 커서 하나만 다른 짝으로
표에 나란히 있다.** 그 짝을
`crates/tasty-ipc/src/method_meta_tests.rs` 의 `the_effect_axis_is_redelivery_not_the_verb` 에
두 줄로 박았다 — 변이로 확인했다(값을 `Read` 로 바꾸면 그 시험이 죽는다).

## Consequences

- **얻은 것**: 에이전트의 `set_mark` 이 스캐너의 관측 창을 못 민다 — 간섭 방향이 닫혔다.
  폴링 1 회의 전송량이 "버퍼 전체(최대 1 MiB)" 에서 "지난 800 ms 에 새로 온 바이트" 로
  바뀐다. 커서가 trim 에 걸려도 `saturating_sub` 로 보존 구간 앞끝에 붙을 뿐 무효화되지
  않으므로, `read_mark` 이 `None` 이 되어 처음부터 읽는 갈래가 스캐너에는 없다.
  그리고 `output_buffer.rs` 에 그 갈래들을 재는 인파일 시험이 생겼다 — trim 이 mark 를
  버리고 처음부터 읽는 것, 두 커서가 서로를 안 미는 것, 커서가 전진하는 것.
- **잃은 것**: 이 커서는 **하나**다. 두 번째 소비자가 같은 메서드를 부르기 시작하면 둘이
  서로의 바이트를 먹고, 그 손실은 조용하다. 소비자별 커서(응답이 `next_cursor` 와 보존
  구간을 함께 주는 형태)는 이 결정의 범위 밖이고 별 작업이다.
  plugin 쪽에는 surface 당 창 하나만큼의 상주 메모리가 생긴다 — 800 ms 마다 같은 양을
  직렬화해 보내던 것과 맞바꾼 것이다.
- **운영 비용 / 유지 부담**: 같은 상한값이 호스트와 plugin 에 **따로** 적힌다. 갈리면
  plugin 의 창이 호스트 버퍼보다 길거나 짧아지고, 짧아지는 쪽은 에러 문자열을 더 일찍
  잃는다. 그리고 `take_since_scan_mark` 의 호출자가 다시 0 이 되면 이 커서는 처음과 같은
  상태 — 선언만 있고 안 쓰이는 커서 — 로 돌아간다. **그 상태를 보는 채널은 없다**(pub 항목은
  dead_code 린트에 안 걸린다).

## Alternatives Considered

- **A. 두 번째 mark 를 주되 전진시키지 않는다** — 간섭은 없어지지만 폴링마다 창이 계속
  자라 1 MiB 재전송이 그대로 남는다. 티켓이 든 결함 둘 중 하나만 닫는다.
- **B. `read_since_mark` 에 `cursor` 같은 선택 파라미터를 더한다** — 이름을 안 늘려도
  되지만, 그 파라미터를 모르는 구 호스트는 **조용히 다른 창**(에이전트의 mark 기준)을
  돌려준다. 새 이름이면 구 호스트가 미등재 메서드로 `-32601` 을 주고 그것이 곧 "지원 안
  함" 신호가 된다. 조용한 오답보다 명시적 거절이 낫다.
- **C. 소비자별 커서 API** — 응답에 `next_cursor` · 보존 구간 시작/끝 · 최대 바이트를 실어
  소비자 수와 무관하게 성립시키는 형태. 이쪽이 최종 형태이지만 응답 모양과 호환 협상이
  함께 걸려 이 결정보다 크다. 여기서는 그 위에 얹을 수 있는 최소 조각만 놓는다.
- **D. 기존 `output.observe_*` 를 쓴다** — 그쪽은 파서를 거친 결과를 sink(memory/file)로
  **밀어 넣는** 구조라 당겨 읽는 커서가 아니고, 에러 카탈로그는 정규식이라 파서 목록에
  올릴 대상이 아니다. 스캐너를 통째로 다시 설계해야 한다.
- **E. 매칭을 호스트로 되돌린다** — 카탈로그를 plugin 으로 옮긴 것이 cutover 의 결정이고,
  그것을 되돌리는 것은 이 결함과 별개의 결정이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 이 결정이 더한 `surface.read_since_scan_mark` 가 `METHOD_TABLE` 에서 빠지는 것.
  `tests/cli_naming_count_drift.rs` 의 `surface` 칸이 그 수를 고정하고 있어, 빠지면 그
  자리에서 어긋난다. 빠진다는 것은 커서를 다시 공유로 되돌렸다는 뜻이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **두 번째 소비자가 같은 커서를 읽기 시작하는 것.** 파괴적 읽기라 둘이 서로의 바이트를
  먹고, 그때는 대안 C 의 소비자별 커서가 필요해진다. 재는 법:
  `git grep -n 'read_since_scan_mark'` 로 호출 자리를 세고, `crates/tasty-plugin-claude`
  밖에 호출자가 생겼는지 본다.
- **커서가 다시 죽는 것** — `take_since_scan_mark` 의 호출자가 0 이 되는 것. 이 ADR 이
  고친 상태로 되돌아간 것이고, pub 항목이라 컴파일러가 말해 주지 않는다. 재는 법: 같은
  grep 을 `take_since_scan_mark` 로 돌려 호출자가 정의 자신과 인파일 시험뿐인지 본다.
- **호스트와 plugin 의 창 상한이 갈리는 것.** 재는 법: `OUTPUT_RETENTION_MAX_BYTES` 와 plugin 쪽
  창 상한 상수를 함께 읽어 값이 같은지 본다.

## References

- 결정이 실현된 현재 위치: `crates/tasty-terminal/src/output_buffer.rs`
  (`OutputBuffer::take_since_scan_mark`) · `src/adapters/ipc/handler/surface/mark.rs`
  (`handle_read_since_scan_mark`) · `crates/tasty-plugin-claude/src/error_scan.rs`
  (`ErrorScanner::scan_one_at`)
- 폴링 비용이 값으로 찍힌 자리: [ADR-0085](0085-ipc-log-retention-bounded.md)
- 크레이트 경계를 사이에 둔 상수 사본의 선례와 그 값의 근거: `crates/tasty-plugin-claude/src/error_scan.rs` (`STALL_QUIET_NO_ERROR`) · [ADR-0266](0266-derived-stale-must-reach-the-push-channel.md) 결정 5
- 재전달 분류의 축과 그 칸을 만든 결정: [ADR-0306](0306-a-method-declares-what-a-second-delivery-leaves-behind.md)
- 이 메서드의 분류를 박은 시험: `crates/tasty-ipc/src/method_meta_tests.rs` 의
  `the_effect_axis_is_redelivery_not_the_verb`(`surface.read_since_mark` 바로 아래 줄) ·
  `the_output_scan_cursor_is_callable_with_the_permission_its_only_caller_holds`
- 메서드 추가·명명 규칙: [api-conventions](../dev-guide/api-conventions.md)
- 출력 읽기 기능 문서: [terminal-output](../features/terminal-output/index.md)
