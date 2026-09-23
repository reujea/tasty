# IPC 로 tasty 조작·검증

tasty 는 AI 가 조작 가능한 터미널이다. 정상 모드면 IPC 서버가 자동으로 뜨고 포트 파일(`~/.tasty/tasty.port`)로 접속한다 — tasty 터미널 안에서 도는 AI 도 IPC 로 자신을 제어할 수 있다. 전체 메서드는 [reference/api](../reference/api.md).

```python
import socket, json, os
port = int(open(os.path.expanduser("~/.tasty/tasty.port")).read().strip())
s = socket.socket(); s.connect(('127.0.0.1', port))
def call(method, params=None):
    s.sendall((json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params or {}})+'\n').encode())
    return json.loads(s.makefile('rb').readline())
```

```bash
tasty list workspaces
tasty send text "hello" && tasty send key enter
```

## 함정 1 — `\r`(Enter) 전송

| 전송 경로 | `"text\r"` 의미 | Enter 전송법 |
|-----------|----------------|-------------|
| CLI `tasty send text "..."` | CR (0x0D) ✅ — CLI 가 `\r` `\n` `\t` `\\` `\0` 을 해석한다 | `"text\r"` |
| Python/JSON-RPC | CR (0x0D) ✅ | `{"text": "...\r"}` |
| CLI `tasty send text $'...'` | CR (0x0D) ✅ | `$'text\r'` |

**Bash 도구에서 `tasty send text "cat\r"` 는 CR 을 보낸다** — CLI 가 전송 전에 C 식 이스케이프를 풀기 때문이다(`crates/tasty-cli/src/request.rs` 의 `unescape`).

## 함정 2 — 응답은 `read_line` 으로 (`read_to_end` 금지)

IPC 서버는 응답을 한 줄(`\n` 종료)로 보낸 뒤 **connection 을 즉시 닫지 않는다.** `read_to_end`/TCP EOF 대기로 읽으면 응답을 받고도 read timeout 만큼 더 기다린다(10초 설정이면 정확히 10초). "모든 IPC 호출이 10초씩 걸린다 → throttling 이다" 같은 잘못된 가설로 시간을 낭비하는 위험한 함정이다.

```rust
// ❌ server close 까지 read timeout 만큼 대기
stream.read_to_end(&mut buf).ok();
// ✅ 한 줄만
BufReader::new(&stream).read_line(&mut line).ok();
```

```python
line = s.makefile('rb').readline()   # ✅ 줄 단위
```

`tests/common/mod.rs::call()` 가 표준 구현 — 새 테스트/디버그 repro 도구는 이것을 따른다.

## 대화형 작업 수행 — 도구 한계 우회

위의 함정들이 "tasty 를 검증한다" 축이라면, 이 섹션은 다른 축이다 — **에이전트가 평소 못 하던 대화형 작업을 tasty surface 로 수행한다.**

### 핵심 명제

tasty surface 뒤에는 **진짜 PTY**(Windows ConPTY) 가 있다. 그래서 stdout 을 파이프로 캡처하는 일반 자식 프로세스(Bash/PowerShell 도구)로는 **구조적으로 불가능한 대화형 프로그램 구동**을 surface 안에서 할 수 있다. `send` → `read screen` → 판단 → `send` 를 반복하면 읽고-쓰는 대화 루프가 된다.

### 왜 되는가 — 두 장벽을 실 PTY 가 둘 다 뚫는다

에이전트가 대화형 입력을 못 하는 이유는 두 장벽이 겹쳐서다:

1. **하니스 장벽** — Bash/PowerShell 도구는 "명령 1회 던지고 종료까지 대기 후 출력 수신" 모델이라, 실행 *도중* 프롬프트에 키를 끼워넣는 실시간 양방향 채널이 없다.
2. **TTY 장벽** — OpenSSH 등은 비밀번호를 stdin 이 아니라 제어 터미널(`/dev/tty`)에서 읽는다. 도구는 자식에 PTY 를 안 붙이므로(stdin=pipe/null) `no tty present...` 로 실패한다.

tasty surface 는 둘 다 뚫는다:

- 장벽1 → `send` / `read screen` 를 **각각 별개 도구 호출**로 반복하면 사실상 폴링 기반 대화 루프가 된다(한 번의 도구 호출 안에서 실시간 주고받을 필요가 없다).
- 장벽2 → surface 에 실 PTY 가 있으니, 프로그램의 `/dev/tty` 읽기가 `send key`/`send text` 로 보낸 키를 실제 터미널 입력으로 받는다.

### 적용 예

ssh 비밀번호/`sudo` 암호/OTP 프롬프트, 대화형 REPL, 설치 마법사, `git rebase -i` 같은 풀스크린 에디터 등 — 파이프 도구로는 손댈 수 없던 대화형 절차를 surface 안에서 진행할 수 있다.

> 단, ssh 비밀번호는 **키 인증이 정답**이다(키-경로 수렴: [remote-attach](../features/remote-attach/index.md), [ADR-0016](../adr/0016-passkey-store-path-convergence.md)). 여기서는 "비밀번호 프롬프트에도 응답할 수 있다"는 **능력의 존재만** 기술하며 비밀번호 자동화를 권장하지 않는다.

### 한계

- **실시간이 아니다(폴링)** — `send` 와 `read` 사이에 프로그램이 아직 프롬프트를 안 띄웠을 수 있다. `send` 전후로 `read screen` 해 상태를 확인하고, 화면 상태와 보낸 키가 어긋나는 desync 를 경계한다.
- **ghost-suggestion 혼동 주의** — Claude Code CLI 등은 빈 입력창에 아직 아무도 타이핑하지 않은 자동완성 제안(dim 텍스트)을 그려둔다. `read screen`(`surface.screen_text`/`pty.read`)은 이런 dim 셀을 **기본 제외**해 실제 입력된 텍스트만 반환하므로, child 의 입력 버퍼 상태를 오독할 위험이 낮다. 제안 텍스트 자체를 보고 싶을 때만 `--show-dim`.
- **비밀번호는 에코되지 않는다** — 비번 입력은 surface 에 표시되지 않으므로 입력이 들어갔는지 화면으로 확인할 수 없다. 다음 화면 상태(프롬프트 통과/재요청)로만 역추론한다.
- **자격증명을 만들어내지는 못한다** — 능력 ≠ 비밀 우회. 모르는 비밀번호/OTP 를 surface 가 대신 알아내 주지 않는다.

### 제약 (불가침 — 반드시 지킨다)

대화형 작업도 "에이전트 행동"이므로 [사용자 행동 ↔ 에이전트 행동 분리](../identity.md) 원칙([프로젝트 CLAUDE.md §1](../../CLAUDE.md))의 적용을 받는다.

- **사용자 활성 쉘 금지** — `foreground_process` 가 사용자 작업이거나 `busy:true` 인 surface 는 건드리지 않는다.
- **전용 테스트 surface 에만** — `tasty new tab --pane <pane_id>` 로 만든 surface 에서만 입력 주입을 한다.
- **끝나면 정리** — 작업이 끝난 테스트 surface 는 닫는다.

> `send`/`read` 는 release 정식 명령이다(debug 전용이 아니다). 입력 *시뮬레이션* 류의 `debug.*` 명령만 `#[cfg(debug_assertions)]` 게이트로 debug 빌드에 한정된다 — 혼동하지 않는다.
