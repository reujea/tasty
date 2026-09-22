# ADR-0092: 공유 로그 파일은 host 프로세스만 연다 — CLI 클라이언트는 stderr 전용

- **Status**: Accepted
- **Date**: 2026-08-30
- **Tags**: logging, tracing, diagnostics, cli, boot, crash-report
- **Group**: cli-logging

## Context

tasty 의 파일 tracing 은 데이터 루트 하나당 파일 하나를 쓴다 — dev 는 `$TASTY_HOME/debug-dev.log`(debug 레벨), release/dist 는 `$TASTY_HOME/debug.log`(warn 이상). 프로세스 시작 시 `fs::File::create` 로 열기 때문에 **매 실행 truncate** 다. 단일 프로세스만 그 파일을 연다는 전제에서는 파일 크기 상한을 공짜로 얻는 합리적인 선택이었다.

그 전제가 tasty 에서는 성립하지 않는다. **GUI(host)와 CLI 클라이언트가 같은 바이너리**이고, 파일 레이어를 만드는 `crash_report::init` 은 CLI/GUI 역할 판정(`cli_routing::parse_or_route()`)보다 **먼저** 불린다. 그래서 `tasty list info` 한 번이 실행 중인 host 의 로그를 통째로 지운다. tasty 는 에이전트가 CLI 를 상시 호출하는 것을 전제로 만든 터미널이라 이건 예외 상황이 아니라 정상 운용 상태다.

실측(dev 빌드, 격리된 `TASTY_HOME`):

- host 가 2.7MB 를 쌓은 뒤 `tasty list info` 1 회 → 파일에 남은 non-null 라인 **6 줄**(전부 그 CLI 프로세스가 찍은 것).
- 게다가 host 는 잘린 파일의 fd 를 그대로 들고 있어 원래 오프셋에 계속 쓴다 → 파일 앞부분이 **2MB 짜리 NUL 구멍**이 되고, host 로그와 CLI 로그가 같은 파일 안에서 오프셋만 다른 채 섞인다.

여러 `docs/dev-guide/` 문서가 이 파일을 사후 진단 수단으로 안내하는데, 진단이 필요한 상황일수록(에이전트가 CLI 로 상태를 캐묻는 상황) 더 빨리 지워졌다. [ADR-0091](0091-render-stall-watchdog-observation-only.md) 의 stall 워치독이 공유 로그 대신 `hang-<ts>.log` 를 따로 쓴 것도 이 함정을 우회한 것이다.

## Decision

**공유 로그 파일은 host 프로세스(GUI / headless)만 연다. CLI 클라이언트는 stderr 로만 로깅한다.**

파일 레이어는 모든 프로세스에 **설치**되지만, 그 writer 는 `OnceLock<Mutex<File>>` 이 채워지기 전까지 출력을 버린다. 파일을 실제로 여는 `crash_report::enable_host_file_log()` 는 `boot::run()` 의 `Routed::Gui` 분기 — 즉 역할이 host 로 확정된 뒤 — 에서만 불린다. host 는 데이터 루트당 하나이므로 시작 시 truncate 는 그대로 유지한다(rotation 불필요).

panic hook 설치와 stderr tracing 초기화는 **위치를 옮기지 않는다.** 둘 다 `main` 진입 직후 `crash_report::init` 에서 전과 똑같이 일어난다 — 부팅 첫 순간의 panic 과 로그를 놓치지 않는 것이 원래 설계 의도이고, 이 결정은 그 의도를 건드리지 않는다. 바뀌는 것은 "파일을 언제 여는가" 하나뿐이다.

## Consequences

- **얻은 것**: host 로그가 CLI 실행에 지워지지 않는다. CLI 프로세스의 로그가 host 로그에 섞이지도, NUL 구멍을 만들지도 않는다. 파일 로그를 사후 진단 수단으로 안내하는 문서들이 실제로 성립한다.
- **잃은 것**: CLI 클라이언트의 tracing 은 파일에 남지 않는다(stderr 전용). 대화형 CLI 실패는 사용자가 stderr 로 즉시 본다. 아무도 안 보는 곳에서 발화하는 agent hook 은 전용 append-only 채널(`hook-failures.log`, `crates/tasty-cli/src/hook_failure.rs`)을 가지지만 **그 채널이 덮는 범위는 전달 실패뿐이다** — `hook_failure::record` 는 IPC 전송/응답이 실패한 경로(`crates/tasty-cli/src/run.rs`)에서만 불린다. IPC 는 성공했는데 CLI 프로세스가 warn 을 찍는 경우(예: `crates/tasty-cli/src/dynamic/stdin.rs` 의 `read_stdin_json` — stdin payload 가 비어 hook params 가 덜 채워지는 상황)는 그 파일에도, 공유 로그에도 남지 않고 stderr 로만 나간다. 수정 전에도 이 경로가 실질적으로 보존된 적은 없다 — hook 은 턴당 여러 번 발화하고 매 발화가 파일을 truncate 했으므로 "마지막 한 번의 몇 줄" 만 남기면서 host 로그를 파괴했다. 즉 순수 손실이 아니라 **보존된 적 없는 것이 명시적으로 없어진 것**이지만, 사각지대인 것은 사실이므로 아래 재검토 트리거로 등록해 둔다.
- **운영 비용 / 유지 부담**: `enable_host_file_log()` 호출은 host 진입 경로 한 곳뿐이다. 새 host 진입 경로가 생기면 그때 함께 불러야 한다(빠뜨리면 파일 로그가 조용히 비는 것으로 드러난다 — 크래시나 오염은 없다). CLI 프로세스에서도 파일 레이어의 필터는 여전히 평가되지만, 통과한 이벤트는 포맷 후 버려질 뿐이라 수명이 짧은 CLI 에서 무시할 수 있는 비용이다.

## Alternatives Considered

- **A: tracing 초기화 자체를 `parse_or_route()` 뒤로 미룬다** — 역할을 알고 나서 초기화하면 분기 하나로 끝난다. 그러나 `parse_or_route()` 는 단순 판정이 아니라 plugin 동적 서브커맨드를 **실행까지** 하는 경로(`try_run_plugin_cli`)를 품고 있어, 그 구간의 stderr 로그가 통째로 사라진다. panic hook 도 함께 늦추면 더 나쁘고, 분리해서 hook 만 앞에 두면 "init 이 두 군데" 라는 순서 의존이 새로 생긴다. 채택한 안은 이 구간을 stderr 로 그대로 보존한다.
- **B: truncate 대신 append + rotation** — 보존은 확실해지지만 rotation 정책(크기 상한·세대 수·회전 시점)이 새로 필요하고, 여러 프로세스가 같은 파일에 append 하면 host 로그와 CLI 로그가 뒤섞이는 문제는 그대로다. 원인(역할 구분 없음)이 아니라 증상(지워짐)만 고친다.
- **C: 역할별로 파일을 나눈다(host 는 `debug.log`, CLI 는 `cli.log`)** — CLI 로그도 보존되지만, 에이전트가 CLI 를 연달아 돌리면 `cli.log` 는 "마지막 한 번" 만 남는 같은 함정을 그대로 물려받는다(또는 다시 rotation 이 필요하다). CLI 진단이 실제로 필요한 지점은 이미 전용 채널을 가지고 있어 값에 비해 비용이 크다.
- **D: 첫 write 시점에 파일을 여는 lazy `MakeWriter`** — CLI 는 대개 warn 이상을 안 찍으므로 평소엔 파일을 건드리지 않지만, **찍는 순간엔 여전히 truncate** 다. 완화일 뿐 해결이 아니다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 한 데이터 루트에 host 프로세스가 둘 이상 정상적으로 공존하는 구성이 생긴다 — 그러면 host 끼리 서로의 로그를 truncate 하므로 append + rotation(대안 B) 또는 프로세스별 파일이 필요해진다.
- CLI 클라이언트에서만 재현되는 결함을 stderr 와 `hook-failures.log` 로 진단할 수 없는 사례가 실제로 나온다 — 대안 C(역할별 파일)를 다시 검토한다.
- 파일 로그가 진단 근거로 쓰이는 빈도가 늘어 "직전 실행분만 남는다"(host 재시작 시 truncate)는 한계 자체가 병목이 된다.

## References

- `crates/tasty-platform/src/crash_report.rs` — `init(app_version)` / `init_tracing()` / `enable_host_file_log()`
- `src/boot.rs`, `src/boot/os.rs` — host 확정 후 파일 개방 호출 지점
- [dev-guide/crash-diagnostics.md](../dev-guide/crash-diagnostics.md) — 진단 파일 위치·필터 표
- [ADR-0091](0091-render-stall-watchdog-observation-only.md) — hang 리포트를 별도 파일로 남긴 결정. 그 근거 중 "공유 로그는 CLI 실행에 지워진다" 는 본 ADR 로 해소되지만, "사용자가 실제로 들여다보는 곳" 이라는 근거가 남아 별도 파일 결정 자체는 유효하다.
