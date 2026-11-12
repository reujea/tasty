# 외부 프로그램 구동 함정 (external-interaction)

Tasty가 PTY로 Claude Code·Codex CLI 등 외부 프로그램을 실행할 때 생기는
입력·완료 알림 문제와 확인한 대응 방법을 정리한다. 외부 프로그램의 버전과 실행 환경에
영향을 받는 결과는 측정 조건 안에서 해석한다.

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
  에서 끊긴다(한글은 2배폭인데도 char 기준). 측정한 콘텐츠(영문/한글/공백/슬래시/경로/특수문자)와
  read 타이밍(즉시/지연)에서는 같은 결과였다.

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

- **호스트 `terminal.tell` 에 중앙화됐다** — `src/app/terminal_input.rs` 의 `tell_payload`(개행
  유무와 무관하게 `\r` 미포함 본문만 만든다. 멀티라인은 bracketed paste 로 감싼다)가 본문을 만들고,
  같은 파일의 `tell` 이 `TerminalInputAccess::write_then_submit`(`src/runtime/pending_submit.rs`)으로 본문을 한
  write 로 보낸 뒤 제출 `\r` 을 예약한다. engine poll 이 본문 write 완료(최대 5초)와 20ms 뒤 같은
  연결에만 별도 `\r` write 를 보낸다. 요청 decode 와 오류 매핑은
  `src/adapters/ipc/handler/terminal.rs` 의 `handle_tell` 이 맡는다. 이 경로가 유일한 구현이다.
- `claude`/`codex` 플러그인의 `tell` 은 자체 페이로드 조립 로직을 갖지 않고 `terminal.tell`
  IPC 를 그대로 호출해 위임한다(`crates/tasty-plugin-claude/src/handlers.rs`,
  `crates/tasty-plugin-codex/src/handlers.rs` 의 `handle_tell` — 둘 다 `host_call(host,
  "terminal.tell", …)` 한 줄). `terminal.spawn` 의 초기 command 주입도 같은 `tell_payload` 를
  거친다.
- 호스트 `terminal.broadcast` 도 동일 함정을 피한다 — `src/app/terminal_input.rs` 의
  `broadcast_payload` 가 호출자가 넣은 trailing `\r`(제출 의도)을 본문에서 분리해, 본문은
  `tell_payload` 로 감싸 한 write, 제출 `\r` 은 별도 write 로 보낸다. tell 과 달리 broadcast 는
  ack 를 기다리지 않고 `TerminalInputAccess::send`(`src/app/services/impl_attach.rs`, `surface.send` 와 같은 즉시 쓰기)로 두 write 를 바로 보내며, `\r` 이 없으면 제출 write 를 생략해 "sent as-is"
  주입 계약을 보존한다.
- **적용 범위 밖(주의)**: `claude`/`codex` 플러그인이 자기 프로세스를 처음 띄우는
  `start_claude_in_surface`류 launch 경로는 별도 `surface.send`(쉘 커맨드라인 + `\r` 한 덩어리)
  라 같은 사각지대다 — 다만 이쪽은 대상이 TUI 가 아니라 아직 아무것도 안 뜬 shell 프롬프트라
  bracketed-paste 휴리스틱이 걸릴 대상 자체가 없어 이 절에서 측정한 TUI paste 문제와는 구분한다.

이 분리 방식은 `tell`·`terminal.spawn`·`terminal.broadcast`에서 사용한다. 단위 테스트가 본문 payload 에 제출 `\r` 이 섞이지 않음(63자+ 회귀 가드
`broadcast_payload_splits_trailing_cr_for_submit`)과 멀티라인 본문 분리(`broadcast_payload_multiline_wraps_bracketed_and_submits`)를 검증한다.

### 일반 교훈

외부 TUI 에 입력을 주입할 때, **"제출(Enter)" 같은 제어 신호는 본문과 다른 write 로 분리**해야
외부의 paste/모드 감지에 흡수되지 않는다. 한 write burst 에 본문+제어를 섞으면 burst 크기에
따라 제어 신호가 비결정적으로 삼켜질 수 있다.

날짜: 2026-06-25 (근거는 내부 조사로 실측 확정 — 재현 매트릭스 핵심 수치를 위에 옮김).

## child 완료 알림 — completion-log

### 증상

부모가 다른 작업을 하는 동안 자식의 완료 메시지를 놓칠 수 있다.
완료 텍스트를 부모 터미널에 입력하면 사용자 발화와 섞이고, 즉시 제출되거나 새 턴을
시작한다는 보장도 없다.

### 원인

메시지 전달 명령인 `terminal.tell`과 비동기 상태 알림은 목적이 다르다.
완료 알림을 tell로 보내면 수신 에이전트의 입력 처리 상태에 따라 전달 결과가 달라진다.

### 처방 (현재 상태)

부모 종류와 무관하게 `<parent_home>/notify/<caller_surface>.log`에 한 줄씩 기록한다.
완료 경로에는 PTY 입력 fallback이나 Codex App Server sender가 없다.
일반 메시지 전달인 `tasty claude tell`과 `tasty codex tell`은 계속 사용한다.
`surface.completion`의 attention 표시와 작업 DAG의 completion strategy도 별개 기능이다.

부모 Claude는 사용 가능한 Monitor로 파일을 구독할 수 있다. Codex 대화에서 spawn한 자식의
턴 결과는 [Codex 부모에게 결과 반환](../plugins/codex/index.md#codex-부모에게-결과-반환)으로
부모 thread의 대기열에 참조로 들어간다. 이 송신은 completion-log writer와 별개다. 파일에
기록됐다는 사실만으로 부모가 읽었거나 새 턴을 시작했다고 보고하지 않는다. idle·입력 대기·중단·exit 알림은 작업 성공의 증명이 아니다.

parent_home은 호스트가 주입한 `TASTY_PARENT_HOME`이며 없으면 `tasty_home()`을 사용한다.
plugin writer와 부모의 PTY는 같은 값을 받는다. release·debug 경로를 추측하지 않는다.
`TASTY_HOME`은 프로세스 자신의 데이터 루트를 선택하는 override이므로 부모 경로를
전달하는 변수로 바꾸면 debug 프로세스가 release 홈을 사용할 수 있다.

메시지는 플러그인의 언어를 따르는 한 줄이다. 고정된 자연어 문구만으로 상태를 파싱하지 않는다.
줄은 번역하지 않는 타입 표지 `[info] `·`[warning] `·`[error] ` 중 하나로 시작한다. 상태 변경·결과 저장은 info, 출력이 없다는 알림은 warning, 오류가 섞이면 무조건 error다. 무출력 알림만 거르려면 `^\[warning\] `로 고른다. 종류별 대응은 [Claude 알림 타입 표지](../plugins/claude/index.md#알림-타입-표지)와 [Codex 완료 알림](../plugins/codex/index.md)에 있다.
Claude의 정지 알림도 같은 파일에 쓰며 기준은 [Claude 정지 알림](../plugins/claude/index.md)을 따른다.
기록 구현은 `tasty-utils::notify::append_notice`(타입을 받는다), 호출자는 Claude의 `handle_notify_done`·`handle_notify_error`·`handle_notify_stop_failure`·원격 결과 저장과
Codex의 `handle_notify_caller`·원격 결과 저장이다.

여러 writer가 같은 파일을 사용하므로 쓰기 핸들은 항상 append로 연다.
메시지와 개행을 버퍼 하나로 만든 뒤 `write_all`을 한 번 호출한다.
이는 `write_all` 내부가 언제나 한 번의 OS write로 끝난다는 뜻은 아니다.
부분 쓰기나 잠금 없는 실패 경로까지 무조건 줄 원자성이 보장된다고 확대 해석하지 않는다.

append 전 기존 크기가 256 KiB 이상이면 전체를 비운다. 마지막 256 KiB를 남기는 방식이
아니므로 미독 메시지도 사라질 수 있다. 한 번의 큰 기록이나 동시 append가 상한을 넘을 수 있어
파일 크기의 엄격한 최대값도 아니다. `tail -F`로 따라가도 이미 버린 미독 내용을 복구하지는 못한다.

비운 바이트 수는 plugin의 tracing 경고와 `<caller_surface>.log.meta`의 `retention_start`에
기록한다. 로그 본문에는 관리용 메시지를 섞지 않는다.
메타 파일의 공유 잠금은 append와 reader가, 배타 잠금은 크기 재확인·비우기·누계 갱신이 사용한다.
배타 잠금은 5ms 간격으로 최대 200ms 기다린다. 기다리는 동안 다른 writer가 먼저 비워 크기가
기준 아래로 내려가면 비우지 않고 그만둔다. 얻지 못하면 잠금 없이 비우고 누계는
갱신하지 않는다. 메타 열기·공유 잠금 실패에도 경고를 남기고 append하며, 누계 쓰기 실패가
크기 정리를 막지는 않는다.

#### 재개하는 reader — `<caller_surface>.log.meta`

메타는 `key=value` 줄이며 현재 키는 `retention_start`다. 모르는 키는 무시한다.
값은 왼쪽 정렬·공백 채움 폭 20의 바이트 누계다. 앞자리 0을 붙이지 않아 셸이 8진수로
해석하지 않는다. 파일·값·키가 없으면 0으로 읽는다.

reader는 `next_offset = retention_start + 파일 안 위치`를 저장한다.
재개할 때 메타의 공유 잠금 아래 누계와 로그 길이를 읽어 다음처럼 처리한다.

| 조건 | 처리 |
|---|---|
| next_offset이 retention_start보다 작음 | truncated, skipped는 두 값의 차이. 파일 처음부터 읽음 |
| next_offset - retention_start가 파일 길이보다 큼 | 누계에 반영되지 않은 축소. skipped는 unknown, 처음부터 읽음 |
| 그 밖 | next_offset - retention_start 위치부터 읽음 |

마지막 개행까지만 소비하고 읽은 바이트만큼 next_offset을 갱신한다.
잠금 안에서는 메타와 필요한 로그를 임시 파일로 복사하고, 출력 처리와 느린 소비자는 잠금 밖에서
실행한다. 고정 폭 메타만으로 잠금 없는 읽기의 일관성을 보장할 수는 없다.

옛 writer, 잠금 실패, 200ms 초과로 누계 없이 비운 뒤 파일이 이전 읽기 위치 이상으로 다시
자라면 손실을 감지하지 못할 수도 있다. unknown 분기가 모든 미집계 손실을 찾는 것은 아니다.
이 오프셋은 한 호스트 실행 안에서만 유효하다. 파일 두 개의 계약이며 전용 읽기 IPC·CLI는 없다.
구현과 참조 reader는 `crates/tasty-utils/src/notify.rs`의 `resume` 시험을 확인한다.

#### 보존 범위·유실·인스턴스 정체성 — 한 자리

로그의 식별자는 데이터 루트와 caller surface ID다. 별도 세대 표식은 없다.
기본 부팅은 `notify/`의 로그와 메타를 지운 뒤 포트 파일을 공개한다.
닫힌 서피스의 파일은 다음 청소까지 남으며 별도의 시간·파일 수 상한은 없다.
청소 실패는 경고만 남기고 부팅을 계속한다. 재시도하지 않으므로 모든 부팅에서 과거 파일이
반드시 사라진다고 보장하지 않는다.

포트 파일의 디렉터리와 데이터 루트가 같을 때만 청소한다.
루트 밖의 `--port-file`은 청소를 생략하고 info 로그를 남긴다. writer 경로는 그대로이므로
`--port-file`만으로 로그를 격리할 수 없다. 같은 루트의 다른 이름으로 포트 파일을 두면 청소한다.

서로 다른 호스트는 `TASTY_HOME`을 따로 사용한다. 같은 루트를 공유하면 surface 번호가
겹쳐 로그가 섞일 수 있다. 청소를 생략한 루트에 새 reader를 붙일 때 보관된 내용부터
읽으려면 현재 retention_start에서, 새 알림만 보려면 현재 파일 끝에서 시작한다.
이전 호스트의 next_offset을 새 실행에 그대로 사용하지 않는다.

#### 부모 Claude 운영 규약 — 자동 mod 수신

`tasty claude install`은 [완료 알림 mod](../plugins/claude/notify-mod.md)를 함께 설치한다. 지원되는 Claude에서 자기 `TASTY_CLAUDE_NOTIFY_MOD` 값이 실제 surface ID와 일치하면 세션 시작부터 완료 로그를 폴링하고 플러그인 출처로 `$.prompt.submit`한다. 같은 세션에 Monitor를 함께 켜면 중복 알림이 된다. 이 경로도 상태 변화만 전달하며 실제 결과를 검증하지 않는다. 세션 재개, 잠금 없는 snapshot, 재로딩 중 대기 중인 제출을 한 번만 받아들이는 별도 gate 플러그인과 그 한계는 mod 문서에 있다.

#### 부모 Claude 운영 규약 — Monitor arm

아래는 mod를 지원하지 않거나 활성화하지 못한 Claude의 대체 경로다. 구독은 첫 spawn/tell 전에 시작하고, 시작 직후에도 자식 목록을 확인한다.

새 알림을 놓치지 않도록 자식을 실행하기 전에 자기 서피스의 로그 구독을 준비한다.
이미 자식을 실행했다면 구독 이전에 기록된 내용도 먼저 확인한다.

```
Monitor({ command: "tail -n0 -F \"$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log\"", timeout_ms: 1800000 })
```

`-n0`은 구독 시작 이전의 줄을 건너뛴다. Monitor에는 `persistent` 같은 상시 유지 옵션이 없고
`timeout_ms`의 최댓값은 30분이다. 만료 알림을 받으면 같은 명령으로 다시 구독하고, 직후
`tasty claude children`으로 자식 상태를 한 번 조회해 만료와 재구독 사이에 끝난 자식을 놓치지 않는다.
두 plugin 의 `children` 이 모두 호스트 `terminal.children` 을 읽으므로 이 목록에는 Codex 자식도 나온다(`tasty codex children` 도 같은 자식을 보인다).
도구 제공 여부와 실제 전달은 아래 상류 제약을 확인한다.

#### Monitor 사용 시 주의 (상류 제약)

- **WebSocket 소스 금지**: Monitor 의 `ws` 소스는 loopback/사설 IP 를 거부하므로
  (`ws://127.0.0.1` 불가) tasty 완료 로그엔 쓸 수 없다. 반드시 `command`(파일 tail) 소스.
- Monitor tool 은 Amazon Bedrock / Google Cloud Agent Platform / Microsoft Foundry 에서
  미제공이고, `DISABLE_TELEMETRY` 또는 `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` 가
  설정되면 비활성이다. 이 경우 completion-log 는 여전히 append 되지만 자동 전달은 안 되므로
  수동 확인(로그 파일 직접 조회)에 의존한다 — `terminal.tell` fallback 은 사용자 입력과 섞이는 문제
  때문에 제거됐으므로 더는 대안 채널이 아니다.
- 간헐적 전달 지연(수십 초)이 보고돼 있다(아래 근거). 기록이 없어진 경우와 전달이 늦는 경우를 구분한다.

### 근거

`notify.rs`의 테스트는 경로·형식·전량 비우기·손실 누계·reader 복구를 확인한다.
동시 writer 시험은 경합을 매 실행마다 만들지는 못한다. 변이를 확인할 때는 대상 시험이
실제로 실행됐는지 보고 반복 실행하며, 한 번 통과했다고 모든 경합을 배제하지 않는다.
비우기까지 포함한 실제 프로세스 경합은 두 writer 이상으로 cap을 반복해서 넘기며 확인한다.
tracing 경고가 최종 파일에 기록되는지는 plugin 로그에서 별도로 확인한다.

Monitor의 전달은 외부 도구 동작에 의존한다. 기존 관측에는 idle 세션 깨움과 간헐 지연이
있었으며, 완료 로그가 남았다는 사실만으로 현재 도구에서도 자동 전달된다고 판단하지 않는다.

### 패키징된 macOS `.app` 의 PATH 제약 — hook 셸 커맨드가 자기 자신을 재호출할 때

완료 알림 hook(`register_notify_hooks` — claude 는
`crates/tasty-plugin-claude/src/notifications.rs`, codex 는
`crates/tasty-plugin-codex/src/handlers.rs`)이
등록하는 `command` 는 `tasty claude notify-done ...` / `tasty codex notify-caller ...` 형태로,
**`tasty` 자기 자신을 PATH 로 재호출**한다. 이 셸 커맨드는 `src/hook_runtime/trigger.rs::spawn_shell`
이 `sh -c`(windows `cmd /C`)로 실행하며 **부모(host 앱) 프로세스의 환경을 상속**한다.

- **함정**: 패키징된 `.app` 을 macOS LaunchServices(Dock/Finder 더블클릭/`open Tasty.app`)로 띄우면
  host 프로세스의 PATH 가 `/usr/bin:/bin:/usr/sbin:/sbin` 로 제한된다. `tasty` 바이너리가 있는
  `.../Tasty.app/Contents/MacOS` 는 이 최소 PATH 에 없으므로, 상속받은 셸이 `tasty` 를 못 찾아
  `command not found`(exit 127)로 **조용히 실패**한다 — completion-log append 가 도착하지 않는다.
- **dev 에서 안 보이는 이유**: `cargo run` / 터미널에서 직접 띄운 바이너리는 그 터미널의 풍부한 PATH를
  상속하므로 재현되지 않는다. LaunchServices 로 띄운 `.app` 에서만 드러난다.
- **처방**: `spawn_shell` 이 자식 프로세스의 PATH 를 보강한다 — `std::env::current_exe()` 의 부모
  디렉토리(=실행 중인 `tasty` 바이너리가 있는 곳)를 PATH 맨 앞에 붙여, 최소 PATH 환경에서도 자기 바이너리를 찾을 수 있게 한다. `current_exe()` 실패는 상속 PATH 그대로 두는 fallback(패닉 없음).
  이 보강은 `InlineShell`/`ShellCommand`(레지스트리) 양쪽이 공유하는 `spawn_shell` 한 곳에서 처리돼
  모든 hook 셸 커맨드에 적용된다. **스코프**: self-binary 디렉토리 하나만 추가하며, 로그인쉘(`$SHELL -lc`)
  이나 사용자 커스텀 PATH(nvm/rbenv/cargo bin 등)를 복제하지는 않는다.
- **공유 헬퍼**: 실제 PATH 계산은 `tasty_utils::process::path_prepending_self_dir` 하나로 통일돼,
  hook 셸(`spawn_shell`)과 PTY 셸(`crates/tasty-terminal/src/pty.rs::build_shell_command`, conductor 자신의
  인터랙티브 터미널이 `tasty` CLI 를 찾는 것도 이 경로 덕분)이 **동일 로직**을 쓴다. 구분자(`:`/`;`)는
  `std::env::{split_paths,join_paths}` 로 크로스플랫폼 처리.
- **회귀 방어**: `crates/tasty-utils/src/process.rs` 의 `prepends_self_binary_dir_to_minimal_path`
  (순수 함수, 최소 PATH prepend 검증) + `src/hook_runtime/trigger.rs` 의
  `inline_shell_resolves_self_binary_via_augmented_path`(self-dir 없는 PATH 에서 basename 재호출 성공
  end-to-end 검증).

### 일반 교훈

- 수신 세션 상태(busy/idle)에 의존하는 PTY 입력 주입은 완료 알림의 **단일 경로로 부적합**하다.
  파일 append + 에이전트가 직접 시작한 감시(Monitor tail)가 상태 독립적이다.
- writer(plugin)와 reader(conductor)가 같은 파일을 가리키려면 기준 경로를 하나로 정하고,
  호스트가 **양쪽 프로세스 모두**에 부모 루트를 env 로 내려줘야 한다 — plugin spawn
  (`tasty-host-plugin`)뿐 아니라 conductor를 실행한 터미널 PTY spawn
  (`tasty-terminal` `Terminal::new`)에도. 한쪽만 전파하면 다른 쪽이 debug/release 루트를
  판별하지 못해 경로가 어긋난다.
- **부모 경로를 알리는 변수와 자식 데이터 루트를 지정하는 `TASTY_HOME`을 구분한다.**
  부모가 자기 루트를 자식에 알려주는 값을 `TASTY_HOME` 으로 주입하면, 그 자식이 다시 tasty
  바이너리(특히 다른 프로파일: release 안에서 debug)를 실행할 때 부모 루트를 자기 데이터
  루트 override 로 오인해 프로파일 격리가 깨진다. 그래서 broadcast 는 `TASTY_PARENT_HOME`
  으로 분리했다(위 경로 규칙 참조).

### 날짜

2026-07-13 최초 작성.

## bash(MSYS): resize 후 다음 입력의 첫 바이트 유실

**⚠️ Windows 전용 이슈.** ConPTY(Windows 전용 PTY 백엔드) + MSYS bash(git-bash 등) 조합에서만 재현된다 — macOS/Linux 의 tasty PTY 백엔드는 ConPTY 를 쓰지 않으므로 이 문서의 증상과 무관하다(아래 실측의 "셸을 cmd.exe 로 교체 → 유실 0" 도 Windows 내부 비교이지, 크로스플랫폼 비교가 아님에 주의).

**증상.** ConPTY resize(레이아웃 변경 — split/unsplit, 창 크기 변경)가 일어난 surface 의 bash 프롬프트에 다음 입력을 주입하면, 첫 1바이트가 소리 없이 사라진다 — `surface.send "seq 1 5000\n"` 이 `eq 1 5000` 으로 도착해 `bash: eq: command not found`. 재현율 resize+입력 쌍당 ~25–33%.

**원인.** bash 는 SIGWINCH 핸들러를 `SA_RESTART` 로 설치하고, readline 은 플래그만 세워뒀다가 **다음 입력이 read 를 깨울 때** 보류된 WINCH 를 처리한다(bash 5 동작, [fff#48](https://github.com/dylanaraps/fff/issues/48)). MSYS/Cygwin 의 시그널 에뮬레이션이 ConPTY 위에서 이 "read 를 깨운 바이트"를 소모한다.

**상태:** MSYS/ConPTY 조합에서 확인된 제약으로 기록한다. 아래 표는 2026-07-12의
측정이며 모든 버전의 동작이나 현재 상류 이슈 상태를 뜻하지 않는다. 대응은 아래와 같다.

**임계·근거 (2026-07-12 실측, 격리 debug 인스턴스).**

| 실험 | 결과 |
|------|------|
| resize 없이 send 만 20회 | 유실 0 (resize 연루 확정) |
| settle 0.1 s / 0.6 s / 2 s 후 send | 모두 ~25–33% — **시간 무관**, "다음 입력"에 앵커 |
| 희생 개행(`\n`) 프리픽스 | 유실 0/15 — 정확히 첫 1바이트만 소모, `\n` 이 대신 먹힘 |
| 셸을 cmd.exe 로 교체 | 유실 0/27 — bash(MSYS) 전용 |
| **tasty 무관 단독 재현기** (portable-pty 단독, 단일 스레드) | **셸 기동 후 첫 resize 의 다음 입력에서 결정적 재현** (3회 연속 1/20, 항상 iter 0) — Tasty 앱 없이도 재현됨 |

**처방 (현재 상태).**

- tasty 는 PTY 계층에서 셸 내부 상태를 알 수 없어 근본 수정 불가(상류 몫). 감시는 soak 하네스가 `input_incidents` 카운터로 계수한다 ([memory-leak-soak](memory-leak-soak.md)).
- **에이전트 처방**: 레이아웃 변경(split/close/resize) 직후 같은 surface 의 bash 에 명령을 주입해야 하면 **텍스트 앞에 `\n` 하나를 붙인다** — 유실돼도 개행이라 무해하고, 안 유실되면 빈 프롬프트 한 줄이 생길 뿐이다. 또는 주입 후 echo 를 검증하고 오염 시 재시도한다 (`tests/soak_memory.rs` 의 `cycle_heavy_output` 가 이 패턴).

**자동화할 때 주의할 점.** 일정 시간 기다리는 것만으로 입력이 전달됐다고 보장할 수는 없다. 이 실험에서는 resize 후 얼마나 기다렸는지와 관계없이, 셸이 다음 입력을 읽을 때 첫 바이트가 사라졌다. 입력 전달을 확인해야 하는 자동화에서는 echo 결과를 검사하거나 앞에 무해한 바이트를 보낸다.

날짜: 2026-07-12
