# ADR-0511: debug 스위치 아래에서 OS 열기는 띄우지 않고 기록한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: debug, verification, self-verification, e2e, isolation, os-open, browser, user-agent-separation, identity
- **Group**: testing

## Context

검증 인스턴스는 격리 `TASTY_HOME` 과 전용 디스플레이(Xvfb)로 띄운다. 그 둘은 **tasty 의 상태**와
**창이 뜨는 화면**을 가른다. 그런데 tasty 가 OS 기본 핸들러에 무언가를 넘기는 자리는 그 둘로
갈리지 않는다.

- `terminal_link::open_uri` — 링크 클릭, `System` 파일 핸들러(`directory-system` · `html-system`),
  plugin 의 파일 열기 요청. `webbrowser` 크레이트를 부른다.
- `platform::reveal::open_path` — explorer "OS 기본 앱으로 열기". Linux `xdg-open` · macOS `open` ·
  Windows `explorer`.
- info modal 의 외부 링크(`open_external`), macOS 전체 디스크 접근 설정 패널.

브라우저는 이미 떠 있는 자기 인스턴스에 URL 을 넘기는 원격 제어 채널(DBus · 소켓 · macOS
LaunchServices)을 가진다. 실측(2026-09-21): 격리 홈 + 전용 Xvfb 로 띄운 debug 인스턴스에서 디렉토리를
dispatch 하자 `directory-system` 이 `firefox-bin file://…` 를 띄웠고, 그것이 **다른 디스플레이에 떠
있던 사용자 Firefox** 로 URL 을 넘겨 사용자 브라우저에 탭이 생겼다. 검증(에이전트 행동)의 부수효과가
사용자 상태에 닿은 것이다 — [정체성 원칙](../identity.md) 1.

그때까지 lane 들은 `PATH` 앞에 가짜 `xdg-open` 등을 두고 `BROWSER=` 로 비워 우회했다. 이 우회는
두 군데서 샌다.

- `webbrowser` 1.2.4(Linux)는 `BROWSER` 의 빈 항목을 건너뛰고, `xdg-settings get
  default-web-browser` 로 찾은 desktop entry 의 `Exec` 를 **직접** 실행한다 — `PATH` 앞의 가짜
  `xdg-open` 을 거치지 않는다. 사건의 `firefox-bin` 이 그 경로다.
- macOS(LaunchServices)·Windows(ShellExecute)에는 `PATH` 로 가로챌 자리가 없다.

e2e 하네스도 같다. 격리 HOME 과 `TASTY_E2E_DISPLAY` 가 이 채널을 막지 않는다.

## Decision

**debug 빌드는 `TASTY_DEBUG_OS_OPEN_LOG=<파일>` 이 있으면 host 의 OS 열기 자리 전부를 띄우지 않고
그 파일에 `<via>\t<대상>` 한 줄을 붙인다.**

- 판정은 `crates/tasty-platform/src/debug_os_open.rs` 의 `intercepted` 하나다. 모듈 선언에
  `#[cfg(debug_assertions)]` 가 붙어 release 에는 없다. 호출부 넷(`open_uri` · `open_path` ·
  `open_external` · `open_full_disk_access_settings`)이 각자 `#[cfg(debug_assertions)]` 한 줄로
  그것을 먼저 묻고, 가로챘으면 **성공으로** 돌아간다(뒤따르는 동작은 실제로 연 것과 같다).
- 변수가 **있기만 하면** 억제한다. 값이 비었거나 파일에 못 쓰면 경고 로그만 남기고 여전히 열지
  않는다(fail closed).
- 켜는 것은 opt-in 이다. 격리 `TASTY_HOME` 이 켜는 것이 아니다.
- e2e 하네스 셋은 격리 홈 아래 `os-open.log` 로 늘 켠다(`tests/spawn_diag` 의
  `apply_os_open_record`). 같은 자리가 받은 인자를 그 파일에 `BROWSER\t<인자>` 로 적기만 하는
  가짜 `BROWSER` 도 준다 — plugin 프로세스의 `webbrowser::open` 을 막는 몫이다. 문서 절차(self-verification)는 이 스위치와 가짜 브라우저
  `PATH`/`BROWSER` 를 함께 쓴다. 뒤쪽은 tasty 가 띄운 **다른 프로세스**(PTY 셸 · plugin)를 막는다.

## Consequences

- **얻은 것**: tasty 자신의 OS 열기가 플랫폼과 무관하게 한 자리에서 막힌다. "무엇이 열리려 했나"
  가 파일의 줄로 남아 시험이 단언할 수 있다 — `tests/e2e_tests.rs` 의
  `directory_dispatch_is_recorded_instead_of_opened_under_the_harness`(gui 조합 · debug 빌드)가 디렉토리
  dispatch 의 기록을 단언한다. 실측(2026-09-23): `open_uri` 의 가로채기를 끄는 변이에서 그 시험이
  FAILED 이고, 그때 가짜 `BROWSER` 가 실제 실행을 받았다.
- **잃은 것**: 없다 — release 동작은 그대로이고, 변수를 안 준 debug 동작도 그대로다.
- **운영 비용 / 유지 부담**: OS 열기 자리를 새로 만들면 그 자리도 `intercepted` 를 물어야 한다.
  그 누락을 잡는 채널은 없다 — 자리를 세는 가드는 만들지 않았다(자리가 넷이고 드물게 는다).
  plugin 프로세스의 열기(번들 markdown plugin 의 `webbrowser::open`)는 이 스위치 밖이다.
  하네스의 가짜 `BROWSER` 는 Linux·BSD 의 `webbrowser` 경로만 막는다(macOS·Windows 의 plugin 쪽 열기는 안 막힌다) —
  근본 해결(markdown plugin 의 외부 링크를 host 경유로 여는 것)은 별도 과제로 남아 있다.

## Alternatives Considered

- **문서 절차만(가짜 `xdg-open` · `BROWSER=`)** — 위 Context 의 두 누수(desktop entry 직접 실행,
  macOS·Windows)를 못 막는다. `BROWSER` 에 기록하는 가짜를 주면 Linux 의 `webbrowser` 경로는
  막히지만, 그것도 플랫폼마다 다른 수단을 사람이 기억해야 한다.
- **격리 `TASTY_HOME` 이면 자동으로 억제** — `TASTY_HOME` 은 다중 인스턴스 · 샌드박스 용도로도
  쓰이고, 거기서 링크를 실제로 열고 싶은 사용이 있다. 루트 override 에 다른 의미를 싣는 것이다.
- **plugin 도 같은 스위치를 읽게** — 번들 plugin 의 산출물이 바뀌어 plugin bump 가 따르고,
  plugin SDK 에 debug 분기를 들이는 일이다. 문서 절차와 e2e 하네스의 가짜 `BROWSER` 가 Linux 에서는 그 경로를 막는다.
- **debug IPC 로 켜고 끄기** — 부팅 직후의 열기(세션 복원 등)를 못 덮고, 스위치를 켜기 전의 창이
  남는다. 환경변수는 부팅 전에 정해진다.

## Reconsideration Triggers

**채널이 붙는 것**

- host 에 `intercepted` 를 안 묻는 OS 열기 자리가 생긴다. 재는 법: `webbrowser::` ·
  `Command::new("xdg-open"|"open"|"explorer")` · `cmd /c start` 를 `src/` · `crates/` 에서 세고,
  각 자리의 함수 첫 줄에 `debug_os_open::intercepted` 가 있는지 본다.

**원리적으로 안 붙는 것**

- plugin 이 스스로 여는 경로가 사고를 낸다(가짜 `BROWSER` 를 안 준 검증에서, 또는 가짜 `BROWSER` 가
  안 닿는 macOS·Windows 에서). 그때는 markdown plugin 의 외부 링크를 host 경유로 옮기는 근본 해결을
  당긴다. 재는 법: 검증 보고의 부수효과 절.

## References

- [debug-ipc.md](../dev-guide/debug-ipc.md) "OS 열기를 띄우지 않고 기록하기" — 스위치의 정본
- [self-verification.md](../dev-guide/self-verification.md) — 검증 인스턴스 절차(스위치 + 가짜 브라우저)
- [e2e-tests.md](../dev-guide/e2e-tests.md) §3 — 하네스 환경 격리 표
- 코드 근거(현재 위치): `tasty_platform::debug_os_open::intercepted` · `tests/spawn_diag` 의
  `apply_os_open_record`
- 부분 개정: [0527](0527-a-plugin-opens-external-links-through-the-host.md) (plugin 프로세스의 열기가 스위치 밖이라는 조항 개정 — markdown 외부 링크는 host `webview.open_external` 로 열려 스위치 안이다)
