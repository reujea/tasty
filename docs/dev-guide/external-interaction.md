# 외부 프로그램 구동 함정 (external-interaction)

tasty 가 PTY 로 **외부 프로그램**(child Claude Code / codex CLI / 기타 TUI)을 구동하고 입력을
주입·제어할 때, **외부 프로그램 측 동작**(bracketed-paste 감지, 입력 모드, read 타이밍 등)
때문에 생기는 **구조적 함정**을 검증된 사실로 모은다.

[`docs/design/systems/design-parity-notes.md`](../design/systems/design-parity-notes.md) 의
dev-guide 판이다 — 대상이 "디자인 렌더와 egui 의 구조 차이" 가 아니라 "tasty 가 구동하는
외부 프로그램의 동작" 이라는 점만 다르다.

## 기록 원칙

- **추측 금지.** 실측(재현 실험)·소스 코드로 확인된 것만 적는다.
- 항목 형식: **증상 / 원인 / 임계·근거 / 처방(현재 상태) / 일반 교훈 / 날짜(절대값)**.
- docs 는 현재 상태를 기술한다 — "수정 전/후" 는 함정을 설명하는 데 필요한 만큼만.

## 노트 — 아래 절 목록

| 노트 | 요지 |
|------|------|
| [tell-autosubmit-paste-threshold](#child-tell-단일라인-자동제출--63자-paste-임계) | child `tell` 단일라인이 63자(code point) 이상이면 자동제출 안 됨 — 본문+`\r` 한 burst 가 수신측 paste 휴리스틱에 흡수. 본문과 제출 `\r` 을 별도 PTY write 로 분리해 해결 |
| [child-completion-notify-log](#child-완료-알림--completion-log) | 완료는 부모 종류와 무관하게 로그 한 줄로 나가고 부모 Claude는 그것을 Monitor 로 tail 한다. 완료 이벤트의 PTY tell fallback은 없다. |
| [bash-resize-first-byte-loss](#bashmsys-resize-후-다음-입력의-첫-바이트-유실) | **Windows 전용**(ConPTY+MSYS bash). resize 후 다음 입력의 첫 1바이트를 지연 SIGWINCH 처리에 소모 (~25–33%, 시간 무관, cmd.exe 무결). 상류 버그 — 에이전트는 `\n` 프리픽스 또는 echo 검증으로 방어 |

## child tell 단일라인 자동제출 — 63자 paste 임계

`tasty claude tell` / `tasty codex tell` 로 child(외부 TUI: Claude Code / codex CLI)에 입력을
주입할 때, 단일라인 본문과 제출 Enter 를 **같은 PTY write burst** 에 섞으면 수신측의
bracketed-paste 휴리스틱이 끝의 `\r` 을 제출이 아닌 paste 본문으로 흡수하는 함정.

### 증상

- `tell` 단일라인 메시지가 **63자(code point) 이상**이면 자동제출(Enter)되지 않고 입력창(❯)에
  텍스트만 남는다. **62자 이하는 정상 제출.**
- **멀티라인(개행 포함) 은 길이 무관 제출**된다.
- 임계는 byte 도 표시폭도 아니라 **code point 수** 다 — 영문 63B 와 한글 189B 가 **같은 63자**
  에서 끊긴다(한글은 2배폭인데도 char 기준). 콘텐츠(영문/한글/공백/슬래시/경로/특수문자)·
  타이밍(즉시/지연 read) 모두 무관, 결정적.

### 원인

수신측 Claude Code / codex TUI 는 한 번의 read 로 들어온 입력 burst 가 일정 크기(~64 codepoint,
= 63자 본문 + `\r`) 이상이면 bracketed-paste 휴리스틱으로 **paste 로 판정**하고, burst 끝의
`\r` 을 제출 Enter 가 아니라 paste 본문 리터럴로 흡수한다.

tasty 가 단일라인 tell 을 본문과 제출 `\r` 을 **한 문자열(`"{msg}\r"`)로 만들어 한 번의
`surface.send` = 한 PTY write** 로 보내면, 본문이 길 때 그 write 전체가 paste 로 처리되어
미제출된다. 멀티라인은 원래부터 본문과 `\r` 을 **별도 PTY write** 로 분리해 보내므로 단독
`\r` 이 제출로 처리되어 길이와 무관하게 안정적이었다.

**근본 원인 소속**: 외부 TUI 의 paste 감지 자체는 합리적 동작이다. 함정은 tasty 가 "제출"
신호를 본문과 같은 write burst 에 섞어 보낸 데 있다 — tasty 측에서 고친다.

### 임계·근거

내부 재현 실험으로 확정(2026-06-25, idle child 에 `tell --no-wait` 후 화면 판정):

| 입력 | 문자수 | byte | 개행 | 결과 |
|------|-------|------|------|------|
| 영문 | 62 | 62 | 0 | 제출 |
| 영문 | 63 | 63 | 0 | **미제출** |
| 한글 | 62 | 186 | 0 | 제출 |
| 한글 | 63 | 189 | 0 | **미제출** |
| 슬래시/경로·한글+경로 혼합 | 62 / 63 | — | 0 | 62 제출 / 63 미제출 |
| 멀티라인 | 17·81 | — | 1 | 제출 (길이 무관) |

→ 경계는 콘텐츠 무관 **본문 63자**(본문+`\r` = 64 codepoint burst). 미제출 케이스는 텍스트가
입력창에 온전히 들어가 있고 수동 Enter 1회로 정상 제출됨.

### 처방 (현재 상태)

단일라인 tell 도 멀티라인과 동일하게 **본문과 제출 `\r` 을 별도 PTY write 로 분리 전송**한다.

- **호스트 `terminal.tell` 에 중앙화됐다** — `src/adapters/ipc/handler/terminal.rs` 의
  `build_tell_payload`(개행 유무와 무관하게 `\r` 미포함 본문만 만든다. 멀티라인은 bracketed
  paste 로 감싼다) + `handle_tell`(본문 `surface.send` → 별도 `\r` `surface.send`, 두 번의
  write 로 분리)이 유일한 구현이다.
- `claude`/`codex` 플러그인의 `tell` 은 자체 페이로드 조립 로직을 갖지 않고 `terminal.tell`
  IPC 를 그대로 호출해 위임한다(`crates/tasty-plugin-claude/src/handlers.rs`,
  `crates/tasty-plugin-codex/src/handlers.rs` 의 `handle_tell` — 둘 다 `host_call(host,
  "terminal.tell", …)` 한 줄). `terminal.spawn` 의 초기 command 주입도 같은
  `build_tell_payload` 를 거친다.
- 호스트 `terminal.broadcast`(`handle_broadcast`)도 동일 함정을 피한다 — `build_broadcast_payload`
  가 호출자가 넣은 trailing `\r`(제출 의도)을 본문에서 분리해, 본문은 `build_tell_payload` 로
  감싸 한 write, 제출 `\r` 은 별도 write 로 보낸다. tell 과 달리 broadcast 는 `\r` 이 없으면
  제출 write 를 생략해 "sent as-is" 주입 계약을 보존한다.
- **적용 범위 밖(주의)**: `claude`/`codex` 플러그인이 자기 프로세스를 처음 띄우는
  `start_claude_in_surface`류 launch 경로는 별도 `surface.send`(쉘 커맨드라인 + `\r` 한 덩어리)
  라 같은 사각지대다 — 다만 이쪽은 대상이 TUI 가 아니라 아직 아무것도 안 뜬 shell 프롬프트라
  bracketed-paste 휴리스틱이 걸릴 대상 자체가 없어 실질 위험은 없다.

결과: `tell`/`terminal.spawn`/`terminal.broadcast` 경로 모두 메시지 길이·콘텐츠와 무관하게
결정적으로 동작한다. 단위 테스트가 본문 payload 에 제출 `\r` 이 섞이지 않음(63자+ 회귀 가드
`broadcast_payload_splits_trailing_cr_for_submit`)과 멀티라인 본문 분리(`broadcast_payload_multiline_wraps_bracketed_and_submits`)를 검증한다.

### 일반 교훈

외부 TUI 에 입력을 주입할 때, **"제출(Enter)" 같은 제어 신호는 본문과 다른 write 로 분리**해야
외부의 paste/모드 감지에 흡수되지 않는다. 한 write burst 에 본문+제어를 섞으면 burst 크기에
따라 제어 신호가 비결정적으로 삼켜질 수 있다.

날짜: 2026-06-25 (근거는 내부 조사로 실측 확정 — 재현 매트릭스 핵심 수치를 위에 옮김).

## child 완료 알림 — completion-log

### 증상

child(claude/codex)가 작업을 끝내 caller(conductor)에게 "완료" 를 알릴 때, caller
세션이 **busy(다른 turn 을 생성 중)** 이면 완료 알림이 **씹힌다**. conductor 가 무거운
작업(빌드/리뷰/직접 코딩)을 도는 동안 child 완료 알림이 도착해도 놓치는 사고가 실제로
있었다.

### 원인

완료 알림의 원래 유일 경로는 `terminal.tell`(claude `notify-done` / codex
`notify-caller`)로 caller surface 의 PTY 에 텍스트+`\r` 을 강제 주입하는 것이다. 이는
**사용자 타이핑을 흉내내는 입력 주입**이라, 수신 세션이 다른 turn 을 처리 중이면 그
입력이 프롬프트 버퍼로 흡수되거나 turn 경계에서만 소비되어, 즉시 새 turn 을 일으키지
못한다. idle 세션이라도 tell 텍스트가 프롬프트에 얹힐 뿐 자동 제출/turn 시작을 보장하지
못하는 경우가 있다.

### 처방 (현재 상태)

완료 이벤트를 **파일에 한 줄씩 append** 한다. 부모가 Claude Code이면 이 파일을 Claude Code 내장
**Monitor tool** 로 tail 하면 busy/idle 여부와 무관하게 다음 turn 에 완료를 전달받는다 —
Monitor 가 뿜는 background-task notification 은 idle 세션도 깨우기 때문이다(상류 동작,
아래 근거).

원래는 이 append 와 함께 `terminal.tell` 도 발사했으나(즉시 눈에 보이는 fallback),
completion-log(Monitor) 채널이 안정적으로 검증된 뒤 **완료-알림 경로에서 `terminal.tell`
주입은 제거**했다. caller 가 Claude Code CLI 세션이면 주입된 텍스트가 **사람이 직접
타이핑해 제출한 발화와 구분되지 않는 형태**로 대화 트랜스크립트에 섞여 들어가는 부작용이
있었기 때문이다. 완료 수신은 completion-log가 담당한다. (일반 메시지
전달인 `tasty claude tell` / `tasty codex tell` 의 `terminal.tell` 은 완료 알림이 아니라
메시지 전달 그 자체이므로 그대로 유지된다.)

- **경로 규약**: `<parent_home>/notify/<caller_surface>.log`
  - `parent_home` = 호스트가 부팅 시 확정한 데이터 루트를 자식에 **`TASTY_PARENT_HOME`**
    env 로 내려준 값. 없으면 `tasty_home()`(= `~/.tasty` release / `~/.tasty-debug` debug /
    `TASTY_HOME` override)으로 fallback.
  - **한 머신에 `~/.tasty` 와 `~/.tasty-debug` 가 공존**할 수 있으므로, 어느 루트인지는
    호스트가 부팅 시 확정한 값을 **env 로 내려받아야만** 판별 가능하다(경로를 눈대중으로
    구성하면 안 됨).
  - plugin(writer)은 호스트가 주입한 `TASTY_PARENT_HOME` env 로 호스트와 **동일 루트**를
    본다 (`crates/tasty-host-plugin/src/process.rs` 가 plugin spawn 시 전파).
  - conductor(reader)가 떠 있는 터미널 PTY 도 동일하게 `TASTY_PARENT_HOME` 을 env 로 받는다
    (`crates/tasty-terminal/src/lib.rs` `Terminal::new` 가 `TASTY_SURFACE_ID` 와 함께
    주입). 따라서 conductor 는 **`$TASTY_PARENT_HOME` env 를 그대로 읽어**
    `$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log` 로 경로를 구성한다 — writer/reader 가
    같은 루트를 보장받는다.
  - **왜 `TASTY_HOME` 이 아니라 `TASTY_PARENT_HOME` 인가**: `TASTY_HOME` 은
    `tasty_home()`(자기 데이터 루트 결정, override 전용)의 1순위 입력이다. 정보성 부모-루트
    값을 `TASTY_HOME` 으로 주입하면, **release 앱이 spawn 한 터미널 안에서 debug 빌드를
    실행**했을 때 그 debug 프로세스가 부모의 release 루트(`~/.tasty`)를 자기 데이터 루트
    override 로 오인한다. 그러면 `cfg!(debug_assertions)` 로 `~/.tasty-debug` 에 격리돼야 할
    debug 인스턴스가 release 의 `~/.tasty/tasty.port` 를 자기 포트로 덮어써, release 앱에
    붙어있던 모든 `tasty` CLI 가 통째로 연결 불가가 되는 사고가 실제로 발생했다
    (2026-07-14). self-determination(`TASTY_HOME`)과 정보성 broadcast(`TASTY_PARENT_HOME`)를
    **환경변수 이름으로 분리**해 이 오인을 원천 차단한다.
- **라인 포맷**: 완료 메시지 **한 줄**. 문구는 그 plugin 의 `lang/{en,ko,ja}.toml` 을 거치므로
  **앱 언어를 따른다** — ko 는 `surface 42 작업 완료 (호출 방식: spawn)`, en 은
  `surface 42 task complete (via spawn)` 이다. 읽는 쪽이 문구를 문자열로 대조하면 로케일에
  따라 깨지므로, 대조가 필요하면 surface 번호처럼 언어에 안 실리는 조각으로 한다. 과거엔 `spawn 완료: surface 42` 형태였으나,
  `command_name`(spawn/tell)이 문장 맨 앞에서 완료의 주어처럼 읽혀 "spawn 이라는 호출이
  접수/완료됐다"로 오독되기 쉬웠다 — 실제 의미는 "그 child 가 맡은 작업이 끝났다"인데
  conductor 가 이를 spawn 접수 확인 정도로 여기고 실제 완료 알림을 계속 무시하는 사고로
  이어졌다(2026-07-17). 이제 "작업 완료"를 앞세우고 호출 방식은 괄호로 분리한다
  (`crates/tasty-plugin-claude/src/notifications.rs` 의 `notify_done_message` ·
  `crates/tasty-plugin-codex/src/handlers.rs` 의 `notify_caller_message`).
- **완료 외의 라인**: claude plugin 은 자식이 **에러 후 멈춘** 경우에도 같은 로그에 한 줄을
  append 한다(`tasty claude notify-error`, `claude-error-stalled` hook). 완료 신호 없이
  매달린 자식을 부모가 무한정 기다리지 않게 하는 push 경로다 — 판정 기준(무출력 지속 +
  호스트가 여전히 `active`)과 노이즈 상한은
  [plugins/claude](../plugins/claude/index.md) 의 "정지 알림" 절.
- **writer 는 여럿이다**: 한 caller surface 밑에 claude child 와 codex child 가 함께 뜨면
  **서로 다른 두 프로세스**가 같은 파일에 append 한다. 그래서 append 헬퍼의 불변식은
  **"한 줄은 통째로 남거나 통째로 없다"** 다 — 줄이 잘리거나 두 줄이 섞이는 상태는 없다.
  그것을 두 가지로 지킨다: 쓰기 핸들은 **언제나 append** 이고(비우기를 섞지 않는다),
  한 줄은 **한 번의 `write`** 로 나간다(`O_APPEND` 가 보장하는 것은 한 번의 write 가 끝에
  통째로 붙는 것뿐이다). 근거·측정·재검토 조건은
  [ADR-0330](../adr/0330-one-completion-line-is-one-write.md).
- **크기 관리**: append 전 파일이 256 KiB 이상이면 비우고 새로 쓴다(무한 성장 방어).
  `tail -F` 는 파일 축소를 감지해 재오픈하므로 arm 된 Monitor 는 비운 뒤의 라인을 계속
  받는다. **비우기는 파일 전체를 버린다** — "마지막 256 KiB 를 남긴다" 가 아니라 "256 KiB
  에서 0 으로 되돌린다" 이므로 실제 보존량은 0 과 256 KiB 사이를 톱니로 오간다. 뒤처진
  reader 의 미독분도 그 안에 있다.

  **버린 양은 기록된다 — 두 자리에.** 비울 때마다 `tracing::warn!` 로 경로와 **버린 바이트
  수**를 남긴다(plugin 이 writer 이므로 그 plugin 의 로그 파일로 간다). 그리고 같은 수를
  로그 **옆 메타 파일** `<caller_surface>.log.meta` 의 누계 `retention_start` 에 더한다(아래
  "재개하는 reader"). 로그 파일 **자신에는 안 쓴다** — 읽는 쪽 계약이 "한 줄 = 완료 통지"
  라 메타 줄을 끼우면 그것이 완료로 읽힌다
  ([ADR-0330](../adr/0330-one-completion-line-is-one-write.md) 이 그 대안을 기각한 자리).

  비우기는 쓰기와 **분리된 단계**이고, 별도 핸들에서 크기를 다시 재고 그때도 cap 을 넘을
  때만 실행한다. 재기 · 비우기 · 누계 갱신은 메타 파일의 **배타** advisory 잠금 아래에서,
  append 는 **공유** 잠금 아래에서 하므로 둘이 겹치지 않는다 — 비우기가 잰 크기가 곧 버린
  양이다. 배타 잠금은 **기다리지 않는다**: 200 ms 동안 다시 시도하고, 못 잡으면 잠금 없이
  비우되 누계는 **안 올린다**(공유 잠금을 쥔 reader 가 멈춰도 완료 통지가 서지 않게 한다).
  그 비우기와 잠금을 모르는 writer(메타 이전 plugin)의 비우기는 누계에 안 잡힌다 — 재개
  reader 에게는 "모른다" 로 보인다. 어느 경우든 사라지는 단위는 **줄**이다(줄 중간이 잘리지
  않는다).

- **재개하는 reader — `<caller_surface>.log.meta`**: `tail -F` 는 arm 된 동안만 비우기를
  따라간다. 멈췄다 다시 붙는 reader 는 그 사이 무엇을 잃었는지 옆 메타 파일로 안다. 정본은
  [ADR-0415](../adr/0415-a-resuming-completion-log-reader-learns-what-it-lost-from-a-sidecar.md).
  - 메타 파일은 `key=value` 줄이고 지금 키는 `retention_start` 하나다 — 이 세대에서 **버린
    바이트 누계**, 곧 지금 파일 첫 바이트의 논리 위치다. 값은 왼쪽 정렬 · 공백 채움 폭 20
    이다(셸 `$((…))` 이 그대로 읽는다). 파일이 없거나 비었으면 0 이다. 메타 파일은 첫
    append 때 빈 파일로 생기고 첫 비우기 때 값이 적힌다.
  - reader 는 **논리 오프셋** `next_offset` = `retention_start` + 파일 안 위치를 들고 있다가
    재개 때 견준다. `next_offset < retention_start` 면 `truncated` 이고
    `skipped = retention_start - next_offset` 바이트를 잃었다 — 파일 처음부터 읽는다.
    `next_offset - retention_start` 가 파일 길이보다 크면 누계에 안 잡힌 비우기가 있었다 —
    잃은 양은 **모른다**, 처음부터 읽는다. 아니면 그 파일 위치부터 읽는다. 마지막 개행까지만
    소비한다. 어휘는 `events.fetch` 와 같고 저장소는 따로다.
  - 정확한 수가 필요하면 누계와 로그를 메타 파일의 **공유** 잠금 아래에서 함께 읽는다
    (Linux: `flock -s "$f.meta" …`). 잠금 없이 읽어도 값이 깨지지는 않는다 — 메타 줄은 늘
    같은 폭이라 제자리 덮어쓰기가 파일을 줄이지 않는다.
  - **공유 잠금 구간은 짧게 한다.** 잠금 안에서는 누계와 로그를 **파일로 복사만** 하고, 풀고
    나서 소비한다. 잠금을 쥔 채 출력을 파이프로 흘리면 소비자가 멈출 때 잠금도 같이 잡혀
    있다. 그 동안 비우기는 200 ms 를 기다린 뒤 잠금 없이 진행하고, 그 비우기는 누계에 안
    잡혀 **이 reader 가 다음 재개에서 "모른다" 를 받는다.** 완료 통지 자체는 서지 않는다.
  - 예 — 셸 reader 한 번의 재개(Linux, `off` 는 직전에 들고 있던 `next_offset`):

    ```sh
    f="$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log"; chunk=$(mktemp)
    at=$(flock -s "$f.meta" sh -c '
      base=$(sed -n "s/^retention_start=//p" "$1.meta" 2>/dev/null); base=$((${base:-0}))
      len=$(stat -c %s "$1" 2>/dev/null || echo 0); off=$2
      if [ "$off" -lt "$base" ]; then echo "skipped=$((base - off))" >&2; pos=0
      elif [ $((off - base)) -gt "$len" ]; then echo "skipped=unknown" >&2; pos=0
      else pos=$((off - base)); fi
      tail -c +$((pos + 1)) "$1" | head -c $((len - pos)) > "$3"
      echo $((base + pos))' _ "$f" "$off" "$chunk")
    cat "$chunk"   # 공유 잠금은 이미 풀렸다 — 소비는 잠금 밖에서 한다
    ```

    `$chunk` 의 마지막 개행까지의 바이트 수를 `$at` 에 더한 값이 다음 `next_offset` 이다.
- **호스트 부팅 시 전량 삭제**: 호스트는 자기 데이터 루트의 주인이 되는 순간 `notify/`
  디렉토리를 **통째로 지운다.** surface_id 는 재시작마다 **1 부터 다시** 발급되므로
  (`IdGenerator::next_surface`) 이전 실행이 남긴 파일과 이번 실행의 파일은 **이름이
  겹친다** — 겹침을 막는 것은 파일 안의 표식이 아니라 이 삭제 하나다. 그래서 **호스트를
  재시작하면 이전 인스턴스의 완료 로그는 남지 않는다** — 재시작을 사이에 두고 과거 줄을
  되읽을 방법은 없다. 디렉토리는 다음 append 의 `create_dir_all` 이 다시 만든다.

  이 삭제는 **포트 파일을 쓰기 전에, 기다려서** 한다. 포트 파일이 인스턴스의 존재를 알리는
  유일한 통로이므로, 그 전에 삭제를 끝내 두면 새 인스턴스의 첫 append 가 삭제와 겹칠 수
  없다(`TcpIpcServer::clear_notify_then_publish_port`). 겹치면 방금 쓰인 줄이 지워진다.

  **청소가 실패해도 부팅은 이어지고, 다시 시도하지 않는다.** 삭제는 디렉토리를 훑어 파일을
  지운 뒤 마지막에 디렉토리 자신을 지운다. 그 사이 누군가 `notify/` 에 새 파일을 만들면
  마지막 단계가 `Directory not empty` 로 실패한다. 그때 호스트는 호스트 로그에 경고 한 줄
  (`failed to clear notify dir <경로>: …`)만 남기고 그대로 포트 파일을 쓴다 — 재시도도, IPC ·
  CLI 로 보이는 신호도 없다(`TcpIpcServer::clear_notify_dir`). 남는 것은 훑기가 지나간 뒤에
  생긴 파일이고, 훑을 때 있던 지난 세대 파일은 지워진다. 실측 2026-09-21: 격리 홈의 `notify/`
  에 쉬지 않고 append 하는 셸 writer 를 돌리는 중에 호스트를 부팅했더니 경고가 한 줄 났고,
  지난 세대로 심어 둔 `9.log`·`9.log.meta` 는 지워졌으며, writer 가 포트 파일 뒤에 쓴 줄은
  하나도 빠지지 않았다. 정상 부팅에서는 포트 파일 전에 writer 가 없으므로(위 순서) 이 갈래는
  아래 "한 데이터 루트에 호스트 하나" 전제가 깨졌을 때만 난다. 다시 시도하면 그 동시 writer
  가 방금 쓴 파일을 지우게 되므로 동작은 이대로 둔다.

- 구현: `crates/tasty-utils/src/notify.rs`(공유 append 헬퍼) + 이 파일에 쓰는 **세 자리** —
  `crates/tasty-plugin-claude/src/notifications.rs` 의 `handle_notify_done`(완료) ·
  `handle_notify_error`(위 "완료 외의 라인"), `crates/tasty-plugin-codex/src/handlers.rs` 의
  `handle_notify_caller`. **claude 쪽은 `handlers.rs` 가 아니라 `notifications.rs` 다.**
  위 불변식은 이 세 자리 전부에 걸린다 — 여기에 쓰기를 더하면 그 자리도 한 번의 write 여야 한다.

#### 보존 범위·유실·인스턴스 정체성 — 한 자리

정본은 [ADR-0344](../adr/0344-the-completion-log-keeps-one-host-generation-and-says-what-it-threw-away.md)
와 `crates/tasty-utils/src/notify.rs` 의 모듈 문서다. 요지는 셋이다.

- **정체성은 경로다.** 한 완료 로그의 정체성은 **(데이터 루트, caller surface id)** 이고,
  그 외에 인스턴스를 가리키는 표식은 줄에도 파일에도 없다. 데이터 루트가 다르면 같은
  surface 번호라도 다른 로그다.
- **보존 범위는 호스트 세대 하나.** 위 부팅 삭제가 그 경계를 만든다. 이 보장은 **"한
  데이터 루트에 호스트 하나"** 를 전제한다 — 전제이지 지켜지는 성질이 아니다.
- **크기 축은 바이트 하나.** 시간 상한도 **파일 수 상한도 없다.** 닫힌 surface 의 파일은
  그 세대가 끝날 때까지 남고, 회수는 다음 부팅의 디렉토리 삭제뿐이다.

★ **`--port-file` 은 이 디렉토리를 격리하지 않는다.** 그 플래그는 포트 파일만 옮기고,
writer 는 여전히 데이터 루트(`tasty_home()`)의 `notify/` 에 쓴다. 부팅 청소는 **포트 파일이
데이터 루트 안에 있을 때만** 돈다(`TcpIpcServer::notify_dir_to_clear`). 기본 포트 파일
(`<루트>/tasty.port`)이나 루트 안의 다른 이름이면 예전처럼 `<루트>/notify` 를 지운다. 루트
밖이면 청소를 건너뛴다 — 같은 루트에 기본 포트 파일로 뜬 호스트의 **살아 있는** 완료 로그를
지우지 않기 위해서다. 예전에는 루트 밖이어도 지웠다(실측 2026-09-20: 격리 홈에 호스트 A 를
띄우고 `notify/9.log` 를 남긴 뒤, 같은 홈에 `--port-file` 만 다른 호스트 B 를 띄웠더니 A 의
`notify/` 가 사라졌다). 근거·대안은
[ADR-0416](../adr/0416-the-boot-cleanup-follows-the-port-file-root.md).
그래도 두 호스트가 한 루트를 쓰면 **surface 번호가 겹쳐 줄이 한 파일에 섞인다** —
루트 밖 포트 파일로 뜬 호스트는 자기 세대 경계도 못 만든다. 격리는 **`TASTY_HOME` 으로** 한다.

#### 부모 Claude 운영 규약 — Monitor arm

부모 Claude는 child 를 dispatch 한 뒤 **한 번** 자기 surface 의 완료 로그를 arm 한다:

```
Monitor({ command: "tail -n0 -F \"$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log\"", persistent: true })
```

`$TASTY_PARENT_HOME` 과 `$TASTY_SURFACE_ID` 는 conductor 터미널 env 에 이미 주입돼 있으므로
경로를 하드코딩하거나 debug/release 루트를 추측할 필요가 없다.

이후 child 완료마다 append 된 라인이 Monitor 이벤트로 전달된다. `-n0` 은 기존 라인을 건너뛰고
arm 시점 이후만 받게 한다. `persistent: true` 로 세션 내내 열려 있어 재-arm 이 필요 없다.

#### Monitor 사용 시 주의 (상류 제약)

- **WebSocket 소스 금지**: Monitor 의 `ws` 소스는 loopback/사설 IP 를 거부하므로
  (`ws://127.0.0.1` 불가) tasty 완료 로그엔 쓸 수 없다. 반드시 `command`(파일 tail) 소스.
- Monitor tool 은 Amazon Bedrock / Google Cloud Agent Platform / Microsoft Foundry 에서
  미제공이고, `DISABLE_TELEMETRY` 또는 `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` 가
  설정되면 비활성이다. 이 경우 completion-log 는 여전히 append 되지만 자동 전달은 안 되므로
  수동 확인(로그 파일 직접 조회)에 의존한다 — `terminal.tell` fallback 은 위장 발화 부작용
  때문에 제거됐으므로 더는 대안 채널이 아니다.
- 간헐적 전달 지연(수십 초)이 보고돼 있다(아래 근거). 손실이 아니라 지연이다.

### 근거

- **tasty 측(append)**: 소스 확인 — `crates/tasty-utils/src/notify.rs`,
  `crates/tasty-plugin-claude/src/notifications.rs`,
  `crates/tasty-plugin-codex/src/handlers.rs`. 단위 테스트로 경로/포맷/비우기와
  **동시 writer 가 줄을 쪼개지 않는지**(`concurrent_writers_never_leave_a_partial_line`)
  검증. 그 시험은 스레드로 재므로 **비우기 갈래의 경합은 안 잰다** — 그 갈래를 재려면
  프로세스를 둘 이상 띄워야 한다(재는 법은 ADR-0330 의 재검토 조건 절).
  보존 범위 쪽은 같은 `mod tests` 의 셋이 잰다 —
  `hitting_the_cap_discards_the_whole_file_not_just_the_excess`(cap 이 전량 폐기인가) ·
  `truncate_reports_how_many_bytes_it_threw_away`(버린 양이 값으로 나오는가) ·
  `truncate_reports_nothing_when_another_writer_already_emptied_it`(안 버렸으면 손실을
  보고하지 않는가). 재개 reader 계약은 같은 `mod tests` 의 참조 reader `resume` 으로 잰다 —
  `a_resuming_reader_is_told_exactly_how_many_bytes_it_lost`(고유 표식으로 중지/재개 대조) ·
  `under_concurrent_writers_read_plus_skipped_equals_written`(동시 writer 에서 받은 바이트 +
  `skipped` = 쓴 바이트. 확률적 채널이라 잠금을 빼도 한 번 실행에 약 30~40% 만 깨진다 — 검출률
  실측과 재는 법은 [ADR-0415](../adr/0415-a-resuming-completion-log-reader-learns-what-it-lost-from-a-sidecar.md)
  의 Consequences) · `retention_start_accumulates_what_every_truncation_threw_away`
  · `an_unaccounted_truncation_is_reported_as_unknown_loss`. **`tracing` 출력 자체에는 채널이 없다** — `tasty-utils` 는 구독자를
  갖지 않는 leaf crate 라, 그 줄이 실제로 나가는지는 호스트를 띄워 plugin 로그에서
  확인한다(ADR-0344 의 재검토 조건 절).
- **Claude Code 측(Monitor 가 idle 세션을 깨움 / 채널이 idle 을 못 깨움)**: 상류
  `anthropics/claude-code` 이슈로 확인. background-task notification 이 idle 세션을 (오히려
  과하게) 깨운다: `#76331`. 반대로 MCP Channels(`notifications/claude/channel`) 는 idle
  세션 미wake 가 다수 OPEN 으로 남아(`#44380`/`#76330`/`#73381`) 완료 알림 주력으로 부적합 —
  그래서 채널이 아니라 **에이전트가 직접 arm 하는 Monitor** 방식을 택했다. Monitor 라인
  전달의 간헐 지연: `#76508`.

### 패키징된 macOS `.app` 의 PATH 제약 — hook 셸 커맨드가 자기 자신을 재호출할 때

완료 알림 hook(`register_notify_hooks` — claude 는
`crates/tasty-plugin-claude/src/notifications.rs`, codex 는
`crates/tasty-plugin-codex/src/handlers.rs`)이
등록하는 `command` 는 `tasty claude notify-done ...` / `tasty codex notify-caller ...` 형태로,
**`tasty` 자기 자신을 PATH 로 재호출**한다. 이 셸 커맨드는 `src/hook_handler/trigger.rs::spawn_shell`
이 `sh -c`(windows `cmd /C`)로 실행하며 **부모(host 앱) 프로세스의 환경을 상속**한다.

- **함정**: 패키징된 `.app` 을 macOS LaunchServices(Dock/Finder 더블클릭/`open Tasty.app`)로 띄우면
  host 프로세스의 PATH 가 `/usr/bin:/bin:/usr/sbin:/sbin` 로 제한된다. `tasty` 바이너리가 있는
  `.../Tasty.app/Contents/MacOS` 는 이 최소 PATH 에 없으므로, 상속받은 셸이 `tasty` 를 못 찾아
  `command not found`(exit 127)로 **조용히 실패**한다 — completion-log append 가 도착하지 않는다.
- **dev 에서 안 보이는 이유**: `cargo run` / 터미널에서 직접 띄운 바이너리는 그 터미널의 풍부한 PATH를
  상속하므로 재현되지 않는다. LaunchServices 로 띄운 `.app` 에서만 드러난다.
- **처방**: `spawn_shell` 이 자식 프로세스의 PATH 를 보강한다 — `std::env::current_exe()` 의 부모
  디렉토리(=실행 중인 `tasty` 바이너리가 있는 곳)를 PATH 맨 앞에 붙여, 자기 자신 재호출은 최소 PATH
  환경에서도 항상 해결된다. `current_exe()` 실패는 상속 PATH 그대로 두는 fallback(패닉 없음).
  이 보강은 `InlineShell`/`ShellCommand`(레지스트리) 양쪽이 공유하는 `spawn_shell` 한 곳에서 처리돼
  모든 hook 셸 커맨드에 적용된다. **스코프**: self-binary 디렉토리 하나만 추가하며, 로그인쉘(`$SHELL -lc`)
  이나 사용자 커스텀 PATH(nvm/rbenv/cargo bin 등)를 복제하지는 않는다.
- **공유 헬퍼**: 실제 PATH 계산은 `tasty_utils::process::path_prepending_self_dir` 하나로 통일돼,
  hook 셸(`spawn_shell`)과 PTY 셸(`crates/tasty-terminal/src/lib.rs::Terminal::new`, conductor 자신의
  인터랙티브 터미널이 `tasty` CLI 를 찾는 것도 이 경로 덕분)이 **동일 로직**을 쓴다. 구분자(`:`/`;`)는
  `std::env::{split_paths,join_paths}` 로 크로스플랫폼 처리.
- **회귀 방어**: `crates/tasty-utils/src/process.rs` 의 `prepends_self_binary_dir_to_minimal_path`
  (순수 함수, 최소 PATH prepend 검증) + `src/hook_handler/trigger.rs` 의
  `inline_shell_resolves_self_binary_via_augmented_path`(self-dir 없는 PATH 에서 basename 재호출 성공
  end-to-end 검증).

### 일반 교훈

- 수신 세션 상태(busy/idle)에 의존하는 PTY 입력 주입은 완료 알림의 **단일 경로로 부적합**하다.
  파일 append + 에이전트가 능동적으로 arm 하는 감시(Monitor tail)가 상태 독립적이다.
- writer(plugin)와 reader(conductor)가 같은 파일을 가리키려면 경로 SoT 를 하나로 통일하고,
  호스트가 **양쪽 프로세스 모두**에 부모 루트를 env 로 내려줘야 한다 — plugin spawn
  (`tasty-host-plugin`)뿐 아니라 conductor 가 사는 터미널 PTY spawn
  (`tasty-terminal` `Terminal::new`)에도. 한쪽만 전파하면 다른 쪽이 debug/release 루트를
  판별하지 못해 경로가 어긋난다.
- **정보성 broadcast 값은 self-determination 용 env(`TASTY_HOME`)와 이름을 겹치면 안 된다.**
  부모가 자기 루트를 자식에 알려주는 값을 `TASTY_HOME` 으로 주입하면, 그 자식이 다시 tasty
  바이너리(특히 다른 프로파일: release 안에서 debug)를 실행할 때 부모 루트를 자기 데이터
  루트 override 로 오인해 프로파일 격리가 깨진다. 그래서 broadcast 는 `TASTY_PARENT_HOME`
  으로 분리했다(위 "왜 `TASTY_HOME` 이 아니라" 참조).

### 날짜

2026-07-13 최초 작성.

## bash(MSYS): resize 후 다음 입력의 첫 바이트 유실

**⚠️ Windows 전용 이슈.** ConPTY(Windows 전용 PTY 백엔드) + MSYS bash(git-bash 등) 조합에서만 재현된다 — macOS/Linux 의 tasty PTY 백엔드는 ConPTY 를 쓰지 않으므로 이 문서의 증상과 무관하다(아래 실측의 "셸을 cmd.exe 로 교체 → 유실 0" 도 Windows 내부 비교이지, 크로스플랫폼 비교가 아님에 주의).

**증상.** ConPTY resize(레이아웃 변경 — split/unsplit, 창 크기 변경)가 일어난 surface 의 bash 프롬프트에 다음 입력을 주입하면, 첫 1바이트가 소리 없이 사라진다 — `surface.send "seq 1 5000\n"` 이 `eq 1 5000` 으로 도착해 `bash: eq: command not found`. 재현율 resize+입력 쌍당 ~25–33%.

**원인 (셸 측 — tasty 무죄).** bash 는 SIGWINCH 핸들러를 `SA_RESTART` 로 설치하고, readline 은 플래그만 세워뒀다가 **다음 입력이 read 를 깨울 때** 보류된 WINCH 를 처리한다(bash 5 동작, [fff#48](https://github.com/dylanaraps/fff/issues/48)). MSYS/Cygwin 의 시그널 에뮬레이션이 ConPTY 위에서 이 "read 를 깨운 바이트"를 소모한다.

**상태: 알려진 상류(msys2-runtime) 버그로 기록만 한다 — 상류 제보는 하지 않기로 결정(2026-07-12).** 상류 이슈 트래커에 기존 리포트 없음(당시 검색 기준). tasty 는 아래 처방으로 방어하고, 향후 마음이 바뀌면 아래 실측 표·원인 분석을 근거로 제보할 수 있다.

**임계·근거 (2026-07-12 실측, 격리 debug 인스턴스).**

| 실험 | 결과 |
|------|------|
| resize 없이 send 만 20회 | 유실 0 (resize 연루 확정) |
| settle 0.1 s / 0.6 s / 2 s 후 send | 모두 ~25–33% — **시간 무관**, "다음 입력"에 앵커 |
| 희생 개행(`\n`) 프리픽스 | 유실 0/15 — 정확히 첫 1바이트만 소모, `\n` 이 대신 먹힘 |
| 셸을 cmd.exe 로 교체 | 유실 0/27 — bash(MSYS) 전용 |
| **tasty 무관 단독 재현기** (portable-pty 단독, 단일 스레드) | **셸 기동 후 첫 resize 의 다음 입력에서 결정적 재현** (3회 연속 1/20, 항상 iter 0) — tasty 최종 무죄 확정 |

**처방 (현재 상태).**

- tasty 는 PTY 계층에서 셸 내부 상태를 알 수 없어 근본 수정 불가(상류 몫). 감시는 soak 하네스가 `input_incidents` 카운터로 계수한다 ([memory-leak-soak](memory-leak-soak.md)).
- **에이전트 처방**: 레이아웃 변경(split/close/resize) 직후 같은 surface 의 bash 에 명령을 주입해야 하면 **텍스트 앞에 `\n` 하나를 붙인다** — 유실돼도 개행이라 무해하고, 안 유실되면 빈 프롬프트 한 줄이 생길 뿐이다. 또는 주입 후 echo 를 검증하고 오염 시 재시도한다 (`tests/soak_memory.rs` 의 `cycle_heavy_output` 가 이 패턴).

**일반 교훈.** "주입한 입력이 도착했는가"는 시간 대기로 보장되지 않는다 — 셸이 시그널을 지연 처리하는 한 유실 창은 벽시계가 아니라 *다음 read* 에 붙는다. 입력 무결성이 필요한 자동화는 echo 검증 또는 무해한 선행 바이트로 방어한다.

날짜: 2026-07-12
