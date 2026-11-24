# 터미널 출력 구조화 (Terminal output)

- **Status**: Implemented
- **주체**: AI Agent
- **설계**: [출력 스캐너 규칙](#출력-스캐너-전용-커서) · 이 문서
- **코드**: `tasty-output` 크레이트, `surface.parse_since_mark`/`surface.commands`/`output.observe_*` 핸들러 · `surface.read_since_scan_mark`(파서를 안 거치는 폴링 커서) · `surface.read_since_mark`(마크 또는 호출자가 지정한 cursor로 읽는 원문 조회)
- **화면**: 없음
- **메서드/파서**: [reference/api](../../reference/api.md#surface-상호작용) · [reference/output-parsers](../../reference/output-parsers.md)

## 목적

터미널 출력을 파싱해 에이전트가 사용할 JSON으로 제공한다. 파싱하지 않은 원문도 같은 `terminal.read` 권한으로 읽을 수 있다. 원문 조회에는 스캐너 전용 커서와 호출자가 직접 관리하는 cursor가 있다.

## 내부 동작 — 다섯 진입점

| 진입점 | 패턴 | 용도 |
|--------|------|------|
| `surface.parse_since_mark` | 일회성 batch | 마크 이후 출력을 한 번에 분해 |
| `surface.commands` (+`last_command`,`command_at`) | 일회성 batch | OSC 133 인덱싱된 **명령 단위** 메타데이터 |
| `output.observe_start` | 스트리밍 | PTY 라인마다 파서 → sink fan-out |
| `surface.read_since_scan_mark` | 주기 폴링 | 전진하는 전용 커서로 **새로 온 것만** 읽는다 (파서를 안 거친 raw) |
| `surface.read_since_mark` (`cursor`+`stream`) | 소비자별 이어 읽기 | 호출자가 지정한 위치부터 원문을 읽음. 소비자별 cursor는 서버에 저장하지 않음 |

**앞의 셋**이 같은 [파서 카탈로그](../../reference/output-parsers.md)를 공유한다. 뒤의 둘은 원문 텍스트를 주며 파싱은 호출자가 한다(아래 "출력 읽기 위치를 정하는 세 방식").

### parse_since_mark

`set mark` → 명령 실행 → `parse-since-mark --parsers path,url,compile_error,test_result`. `--parsers` 생략 시 기본 4종(`path,url,prompt_boundary,exit_code`). 고급 6 종은 명시 opt-in. 전체 block 을 받아 멀티라인 파서(`compile_error`/`stack_trace`)도 정확히 분해.

<a id="읽는-자리를-말하는-법은-셋이다"></a>

### 출력 읽기 위치를 정하는 세 방식

- **공유 마크**: `surface.set_mark`가 정하며 `parse_since_mark`와 cursor 없는 `read_since_mark`가 읽는다.
  surface마다 하나이므로 한 소비자가 마크를 바꾸면 다른 소비자의 다음 조회 범위도 바뀐다.
- **스캐너 커서**: `surface.read_since_scan_mark`가 읽을 때마다 전진한다.
  공유 마크와는 독립적이지만 surface마다 하나여서 스캐너 소비자 하나를 전제로 한다.
- **소비자별 cursor**: `surface.read_since_mark`에 `cursor`와 `stream`을 함께 보낸다.
  각 소비자가 자기 위치를 보관하므로 다른 소비자의 조회 범위를 바꾸지 않는다.
  서버에는 소비자별 cursor 상태를 저장하지 않는다.

독립적인 이어 읽기의 선택 근거는 출력 위치 조회,
사건에 같은 방식을 적용한 근거는 [이벤트 피드](../../reference/event-catalog.md)에 있다.

### attach mirror 에서 읽기

attach mirror 터미널의 출력 보관 버퍼는 attach 화면으로 다시 만든 사본이라 attach 이전 원문이 없고 유실을 `skipped` 로 알리지 못한다.
그래서 mirror 의 `read since-mark`·`set mark`·`read screen` 은 서버 원본 터미널을 읽는다. stream·cursor·보존 범위·`skipped` 는 서버 값이다.
mirror 의 마크는 client 가 보관하므로 서버 쪽 다른 소비자의 공유 마크를 바꾸지 않는다. 서버 터미널이 바뀌어 마크의 stream 이 맞지 않으면 `stream_mismatch` 로 거절하고 set mark 를 다시 하라고 알린다.
서버가 이 기능을 지원하지 않거나 연결이 끊긴 동안은 로컬 사본으로 답하지 않고 오류로 답한다. `parse_since_mark`·스캐너 커서·명령 인덱스는 mirror 의 로컬 사본 기준이다. 스캐너 커서 응답의 연속성 키도 로컬 사본 값이고, 출처 정보(`text_origins`)는 싣지 않는다.
전송 계약은 [Attach 메커니즘](../../dev-guide/attach-behavior.md#mirror-터미널-읽기-ipcstreamterminal-read)에 있다.

### 출력 스캐너 전용 커서

`surface.read_since_scan_mark`는 TerminalRead 권한으로 읽고 커서를 전진시킨다.
`OutputBuffer::take_scan_snapshot`이 상태 락 안에서 원문 복사와 전진을 한 번에 수행한다.
둘을 나누면 읽은 뒤 전진하기 전에 도착한 출력이 처리되지 않고 넘어갈 수 있다.
디코딩·ANSI 제거·출처 대응은 락을 놓은 뒤 한다(`ScanSnapshot::finish`). 그래서 PTY reader가 상태 락을 기다리는 시간은 원문 복사만큼이다.
첫 호출은 현재 보관한 전체 출력을 받을 수 있고 이후부터 새 출력만 받는다.

응답은 `surface.read_since_mark`와 같은 연속성 키(`stream`·`cursor`·`next_cursor`·`skipped` 등)를 싣는다.
같은 `stream`이면 이전 응답의 `next_cursor`를 이번 `cursor − skipped`와 견주어 응답을 잃었는지 안다.
보존 한도로 잘리기만 해도 `cursor`가 뛰므로 `cursor`만 견주면 잘림을 응답 유실로 잘못 본다.
`stream`은 터미널 교체(`cursor`가 0부터 다시) 외에 attach mirror의 재연결과 손실 통지 뒤 재동기화에서도 바뀐다.
뒤의 둘은 위치를 그대로 두고 표지만 새로 낸다(`OutputBuffer::renew_stream`, `Terminal::break_stream`).
`skipped`는 커서가 읽지 않은 원문이 보존 한도로 잘린 양이다.

응답의 `text_origins`는 `text`의 범위마다 그 원문이 일반 출력인지 OS 크기 적용 뒤 재그리기인지 알린다.
이 필드를 읽는 claude plugin과 함께일 때 그 plugin이 재그리기 출력을 정지 판단의 무출력 시계와 오류 기준에서 뺄 수 있게 하는 값이다.
호스트는 분류를 줄 뿐이다. 정지 알림에 효과가 나려면 실행 서버에 이 필드를 싣는 Tasty와 이 필드를 읽는 plugin이 둘 다 있어야 한다.
분류는 청크가 들어올 때 정해 보존 원문 옆에 둔다(`crates/tasty-terminal/src/output_origins.rs`).
읽을 때의 재그리기 구간으로 다시 가르지 않는 이유는 구간이 다음 크기 통지로 새로 열리며 시작이 옮겨 가기 때문이다.
그렇게 하면 앞 구간의 재그리기를 일반 출력으로 잘못 센다.
같은 시각의 청크는 투입 순서로 가른다. 구간을 열기 전에 들어온 청크는 시각이 구간 시작과 같아도 일반 출력이고, 연 뒤에 들어온 청크는 같은 시각이어도 재그리기 쪽이다.
OS 크기 적용이 진행 중일 때 들어온 청크는 잠정으로 두고 적용이 성공하면 재그리기, 실패하면 일반 출력으로 같은 락 안에서 정한다.
다만 열기 전부터 있던 구간이 이미 덮는 진행 중 청크는 바로 재그리기로 정하고 적용이 실패해도 그대로 둔다(`TerminalState::chunk_origin`).
스캔 읽기와 OS 적용은 같은 이벤트 루프에서 돌아 잠정 상태가 읽기에 보이지 않는다. 보이면 그 범위는 `unknown`이다.
`text` 대응은 원문 바이트를 `utf8_chunks`로 돌아 만들고 escape 제거는 `strip_ansi`와 같은 정규식의 매치 범위(`tasty_ansi::ansi_escape_ranges`)를 쓴다.
그래서 `text`는 출처 정보가 생기기 전과 같다.
값의 모양·생략 조건은 [API 레퍼런스](../../reference/api.md)에, 분류 판정의 정본은 `crates/tasty-terminal/src/scan_origins_tests.rs`에 있다.
분류의 한계(구간 안에서 시작해 끝난 진짜 출력도 재그리기다)는 [적용이 일으킨 출력](../../design/policies/render-on-draw.md#적용이-일으킨-출력) 규칙과 같다.

이 커서는 surface마다 하나다. 소비자가 여럿이면 서로의 데이터를 소비하므로 CLI 명령을 제공하지 않는다.
다만 plugin_only 전용 dispatch는 아니며 일반 로컬 IPC 클라이언트도 메서드를 호출할 수 있다.
응답을 받지 못해 재전송하면 이미 소비한 구간을 다시 받지 못하므로 메서드 효과는 Mutate다.
`surface.read_since_mark`의 읽기 효과와 혼동하지 않는다.

Claude plugin은 받은 델타를 자체 제한 버퍼에 모아 기존 에러 패턴·출력 지문·200자 중복 제거·정지 판단을 수행한다.
host의 OUTPUT_RETENTION_MAX_BYTES와 plugin의 수집 버퍼 상한은 일치해야 한다.
다른 스캐너를 추가하려면 같은 scan 커서를 공유하기 전에 소비자별 위치 읽기를 검토한다.

<a id="보존-밖으로-밀려난-것은-값으로-나온다"></a>

### 보존 범위와 유실 확인

출력 버퍼는 1 MiB를 넘으면 오래된 바이트부터 지운다. 절대 위치인 마크와 cursor는 그대로 두고
`retention_start`만 앞으로 이동한다. 지워진 위치에서 읽으면 `skipped`가 잃은 바이트 수를 알려준다.
공유 마크로 읽을 때도 같다.

응답에는 `retention_start`, `retention_end`, `cursor`, `next_cursor`, `raw_bytes`, `stream`이 있다.
다음 조회에는 `next_cursor`를 사용한다. 손실 디코딩과 ANSI 제거 때문에 `text` 길이는 원문 길이와 다르다.
stream이 다르거나 cursor가 현재 끝보다 크거나 대상에 터미널이 없으면 오류로 거절한다.
사유는 `error.data.reason`으로 구별한다.

CLI의 첫 조회는 cursor 없이 실행한다. 응답에서 받은 값으로
`tasty read since-mark --cursor <next_cursor> --stream <stream>`을 호출해 이어 읽는다.
`--cursor`에는 `--stream`이 필요하다. `--max-bytes`는 응답할 원문을 1바이트부터 보존 크기까지 제한한다.
0도 1바이트로 처리하며, 보존된 전체 범위를 읽으려면 이 인자를 생략한다.

옛 서버는 새 인자를 무시하고 공유 마크 범위로 답할 수 있다. CLI는 cursor·stream·max_bytes 중
하나라도 보내기 전에 `ipc.output-cursor` capability와 버전을 확인한다.
지원하지 않으면 요청을 보내지 않고 `unsupported_capability`와 `sent:false`를 포함한 JSON 한 줄을
stderr에 쓴 뒤 종료 코드 1로 끝난다. 새 인자가 없는 기존 호출에는 이 확인을 추가하지 않는다.
자세한 호환 이유는 출력 조회 ADR을 따른다.

### 명령 인덱싱 (OSC 133)

셸 통합이 OSC 133 을 보내면 각 명령의 prompt 시작/명령 시작/종료/exit code/명령 문자열을 `tasty-memory`(`surface:<id>` scope, `tasty.commands.<ms>`)에 기록. OSC 133 미지원 셸은 빈 배열.

**headless PTY 는 인덱싱 대상이 아니다.** 인덱서는 `TerminalStore` 키를 그대로 scope id 로 쓰는데 headless PTY([headless-pty](../headless-pty/index.md))의 `Terminal` 은 그 store 에 **pty id**(`>= 0x8000_0000`)로 등록돼 있다 — 그대로 기록하면 surface id 공간을 침범한 `Scope::Surface` 가 생겨 다음 부팅의 surface 카운터를 PTY 공간으로 밀어 올린다([포커스 정책 (운영 상세)](../../design/policies/focus.md)). headless PTY 의 종료코드는 `pty.wait` 가 별도로 제공한다.

**bash/zsh 셸 통합은 자동으로 주입한다.** 사용자의 `.bashrc`/`.zshrc`를 수정하지 않고
셸 시작 때 OSC 133의 A(prompt 시작), C(명령 실행 직전), D(종료와 exit code)를 보낸다.

| 셸 | 주입 방식 | 제한 |
|---|---|---|
| bash | `--rcfile <tasty 합성 rc>`, C는 `PS0`, A/D는 `PROMPT_COMMAND` 앞에서 `$?`를 캡처 | `PS0`가 있는 bash 4.4 이상 필요. 사용자 DEBUG trap과 충돌하지 않도록 구버전용 폴백은 두지 않음 |
| zsh | 관리 `.zshenv`를 `ZDOTDIR`로 지정하고 `add-zsh-hook precmd/preexec` 등록 | 사용자가 훅 배열을 통째로 덮거나 동명 함수를 재정의하면 tasty 훅이 사라질 수 있음 |
| fish/nu/pwsh(비-Windows) 등 | 자동 주입 미지원 | 사용자가 셸 통합을 설치해야 명령 인덱스가 생김 |

bash·zsh 프롬프트 함수는 D·A를 보낸 뒤 데이터 홈(`TASTY_PARENT_HOME`)의 `shell-integrations/`에 있는
`*.sh` 일반 파일을 C 로캘 바이트 순서로 매 프롬프트마다 `source`한다. 플러그인이 셸 함수 같은 통합을 넣는 확장점이며,
규칙은 [플러그인 개발 가이드](../../dev-guide/plugin-development.md#셸-통합-확장-스크립트)에 있다.

- Given 직전 명령이 23으로 끝났고 한 확장이 `return 7`로 끝날 때, When 프롬프트를 준비하면, Then D는 23을 보내고 나머지 확장과 프롬프트가 계속 동작한다.
- Given 사용자가 errexit(`set -e`·`setopt err_exit`)나 noglob(`set -f`·`setopt noglob`)을 켰을 때, When 프롬프트를 준비하면, Then 확장을 모두 읽고 실패한 확장 때문에 셸이 끝나지 않으며 사용자의 옵션은 그대로 남는다.
- Given 확장 디렉터리가 없거나 비어 있을 때, When 프롬프트를 준비하면, Then A·D만 보내며 bash의 `nullglob`·`failglob`과 zsh 옵션은 사용자 값 그대로다.
- 확장 안의 `exit`처럼 셸 자체를 끝내는 동작은 막지 않는다.

bash 훅 스크립트는 플랫폼 간에 공유한다(`crates/tasty-settings/src/general.rs`의
`BUILTIN_BASHRC_*`). Windows는 `--rcfile <path>`, 비-Windows는 `--rcfile <path> -i`로
실행한다. 로그인 셸에서는 `--rcfile`이 무시되므로 비-Windows에서는 명시적인 대화형 셸을
쓰고, `~/.bash_profile` → `~/.bash_login` → `~/.profile` 중 처음 존재하는 파일을 읽어
로그인 프로필 설정을 유지한다.

zsh는 `~/.tasty/zsh-integration/.zshenv`에서 훅을 등록한 뒤 `ZDOTDIR`를 원래 값으로
돌리고 원래 `.zshenv`를 읽는다. 원래 설정되지 않았으면 `ZDOTDIR`를 해제한다.

현재 OSC 133 C 파서는 payload를 버린다. `vte_handler/osc.rs`가
`FinalTermSemanticPrompt::MarkEndOfInputAndStartOfOutput`을 빈 payload로 바꾸므로
zsh가 보내는 `cmd=`도 명령 인덱스에 도달하지 않는다. bash는 이 필드를 보내지 않는다.
C payload를 보존하도록 바꿀 때에는 bash의 명령 문자열 추출도 함께 검토해야 한다.
PTY 확인에서 `history 1`은 실행할 명령을 가리켰지만 `$BASH_COMMAND`와 `fc -ln -1`은
이전 명령을 가리켰다. OSC 133 payload는 선택 사항이며 인덱서는 없는 경우도 처리한다.

### 스트리밍 옵저버

PTY 라인마다 파서를 돌려 sink 로 fan-out(**휘발성** — 호스트 재시작 시 소멸). sink: `memory`(ring buffer, `memory.list`/`query` 로 회수) / `file`(JSONL append). 필터: `parsers`(활성 파서) + `kinds`(출력 후 kind 필터) + `surface_id`(생략 시 전체 surface wildcard). **백압**: 옵저버별 bounded channel(256), 채워지면 drop + `info.dropped` 증가(PTY 스레드 절대 block 안 함). memory sink 는 채널에 쌓인 항목을 묶어 저장 실행기에 넣으며, 실행기가 접수하지 않은 항목은 `info.submit_rejected`, 저장되지 않았거나 저장 여부를 모르는 항목은 `info.store_failed` 로 센다([저장 계약](../../design/systems/storage.md#터미널-출력-observer-의-memory-sink--저장-계약)). surface 닫히면 매인 옵저버 자동 정리, wildcard 는 유지. **자동 정리는 sink 워커를 그 자리에서 join 하지 않는다** — 워크스페이스 close 가 surface 수만큼 이 경로를 렌더 스레드에서 반복하기 때문이다([종료 시퀀스 — 종료 cascade + Drop tail](../../architecture/shutdown-sequence.md)). channel 에 수락된 항목은 워커가 비우며 저장 실패·접수 거절은 별도 집계한다. 일반 세션 소유자 해제는 worker와 accepted 작업(확정 뒤 보존 삭제 포함)의 완료를 기다리고, 남은 워커는 앱 종료 시퀀스(S3b)가 완료를 폴링한다. 5초 뒤에도 남으면 handle을 미회수로 기록하고 새 memory 제출을 차단한다. 명시적 앱 종료를 시작한 라우터의 Drop은 다시 동기 대기하지 않으므로 프로세스 종료까지 미완료한 항목은 잃을 수 있다. 다만 **sink 파일에 마지막 항목이 도달하는 시점은 close 응답 이후로 밀릴 수 있다.** 명시 해제(`output.observe_stop`)는 종전대로 호출 복귀 시점에 sink 가 닫혀 있음을 보장한다.

> **멀티라인 파서는 줄 단위 옵저버에서 결과를 만들지 않는다**(라인별 dispatch). 컴파일 에러 수집은 `prompt_boundary` 옵저버로 종료 감지 후 `parse_since_mark` batch.

## 인터페이스

- **AI Agent / CLI**: `tasty read since-mark [--cursor N --stream S] [--max-bytes N]` · `tasty read parse-since-mark` · `tasty read commands/last-command/command-at` · `tasty output observe {start,list,info,stop}`. [reference/api](../../reference/api.md#surface-상호작용).

## 관련

- [reference/output-parsers](../../reference/output-parsers.md) — 파서 카탈로그 · [work-area](../work-area/index.md) — surface

## 위치 조회의 세부 계약

`error.data.reason`은 cursor_without_stream·stream_mismatch·cursor_ahead_of_stream·no_terminal로 구별한다.
명시 cursor는 지정한 surface만 조회하고 focus fallback을 하지 않는다.
stream은 버퍼 생성 시각과 counter를 함께 써 terminal 재생성과 교체를 구별한다.

max_bytes가 UTF-8 문자 중간에서 끝나면 최대 3 바이트 물러선다.
그 결과 0 바이트가 되면 읽기가 계속 멈추지 않도록 원래 범위를 읽는다. 음수·비정수 max_bytes는 인자 오류다.
원문은 메모리만 보존하며 화면 스크롤·선택이나 입력 기록을 바꾸지 않는다.

CLI는 --stream 단독도 허용한다. 기존 mark에서 읽으면서 stream 교체 여부를 확인하는 형태다.
협상 실패의 JSON은 capability·required·found·sent:false를 포함한다. found가 없으면 null이다.
인자 세 가지 중 하나라도 쓰는 외부 client는 CLI와 마찬가지로 ipc.output-cursor를 먼저 확인해야 한다.
새 독립 인자는 별도 capability 이름으로, 기존 인자의 의미를 좁히는 변경은 해당 기능 버전으로 구별한다.

일반 세션 retirement의 Router Drop은 새 제출을 차단하거나 worker를 분리하지 않고 accepted 작업 완료를 기다린다. 명시적 앱 종료의 polling·timeout 이후 Drop과 구분한다. 저장 오류나 writer의 접수 거절을 성공으로 보장하지 않는다.
