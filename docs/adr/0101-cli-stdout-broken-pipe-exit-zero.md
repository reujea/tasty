# ADR-0101: CLI 클라이언트의 stdout 파이프 조기 종료(EPIPE)는 종료 코드 0 으로 조용히 끝낸다 — SIGPIPE 복원은 채택하지 않는다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: cli, stdout, epipe, sigpipe, exit-code, crash-report, cross-platform, error-handling
- **Group**: cli-logging

## Context

`tasty <명령> | head -c 100`, `| true`, `| grep -m1` 처럼 **읽는 쪽이 파이프를 먼저 닫으면**
CLI 클라이언트가 `failed printing to stdout: Broken pipe (os error 32)` 로 panic 했다.
결과는 종료 코드 101, stderr 의 `Tasty crashed! Report saved to: …`, 그리고
`~/.tasty/crash-reports/crash-<ts>.log` 파일 하나였다.

원인은 언어 런타임의 고정 동작이다. Rust 는 프로세스 시작 시 SIGPIPE 를 `SIG_IGN` 으로
두므로 닫힌 파이프에 write 하면 (C 도구처럼 조용히 죽는 대신) `EPIPE` 가 반환되고,
`println!`/`print!` 는 그 `io::Result` 를 panic 으로 승격한다. tasty 의 panic hook 은 모든
panic 을 crash report 로 기록하므로 정상적인 파이프 조기 종료가 크래시로 둔갑했다.

문제가 되는 이유는 둘이다.

- 에이전트가 CLI 출력을 파이프로 자르는 것은 정상 사용 패턴이다(`docs/dev-guide/self-verification.md`
  등이 `tasty list tree | …` 조합을 안내한다). 종료 코드 101 은 에이전트가 명령 실패로
  오판하게 만든다 — [identity.md](../identity.md) 원칙 2 에서 CLI 는 에이전트 1 급 인터페이스다.
- `crash-reports/` 에 가짜 리포트가 쌓여 **실제 host 크래시 추적이 오염**된다.

제약도 둘이다.

- **host 에는 절대 적용 금지**: host 는 agent task 로 자식 프로세스 stdin 을 `Stdio::piped()`
  로 열고 `write_all` 한다. host 의 SIGPIPE 를 기본 동작으로 되돌리면 자식이 먼저 죽었을 때
  host 전체가 죽는다. 어떤 처리든 CLI 클라이언트 갈래(`Routed::AlreadyHandled` /
  `Subcommand` / `AugmentedHelp`) 안에 격리돼야 하고 `Routed::Gui` 는 건드리지 않는다.
- **크로스 플랫폼**: SIGPIPE 는 unix 전용 개념이다. Windows 에는 없고
  `io::ErrorKind::BrokenPipe` 만 온다(원칙 4).

## Decision

**stdout 쓰기를 `Result` 로 받아 `BrokenPipe` 를 CLI 경계까지 전파하고, 경계에서 종료 코드
`0` 으로 조용히 끝낸다.** SIGPIPE 기본 동작 복원(종료 코드 141)은 채택하지 않는다.

- tasty-cli 는 stdout 에 `crates/tasty-cli/src/out.rs` 의 `outln!`/`out!`(+ `flush`,
  외부 `io::Result` 용 `from_io`)로만 쓴다. `println!`/`print!` 는 쓰지 않는다.
- 쓰기 실패 중 `ErrorKind::BrokenPipe` 만 `StdoutClosed` 타입으로 구분해 `?` 로 올린다.
  그 외 stdout 오류(EIO / ENOSPC 등)는 일반 에러로 전파돼 `Error: …` + 종료 코드 1 이다.
- CLI 진입점 4 곳 — `run_client` / `print_augmented_help` / `print_command_tree` /
  `try_run_plugin_cli` — 이 `quiet_if_stdout_closed` 로 `StdoutClosed` 를 `Ok(())` 로 접는다.
  이 4 곳이 곧 CLI 클라이언트 3 갈래의 전부이고, `Routed::Gui` 는 stdout 에 쓰지 않으므로
  host 동작은 변하지 않는다.
- 종료 코드는 **0** 이다. "더 쓸 곳이 없어졌다" 는 명령 실패가 아니며, 종료 코드를 보고
  성패를 판정하는 에이전트가 오판하지 않아야 한다. crash report 는 어떤 경우에도 만들지
  않는다(panic 이 애초에 발생하지 않는다).
- panic hook 은 그대로 둔다. CLI 클라이언트에서 진짜 버그로 panic 이 나면 여전히 crash
  report 가 남아야 한다 — 역할 구분으로 hook 을 끄는 것은 증상만 가린다.
- `crates/tasty-cli/src/local/attach.rs` 의 raw bridge 는 예외다. `std::io::stdout()` 핸들에
  best-effort 로 미러하며 결과를 이미 무시하고, stdout 이 닫혀도 세션은 계속돼야
  한다(detach 승격 없음). `println!` 을 쓰지 않으므로 이 panic 의 대상도 아니다.

## Consequences

- **얻은 것**: 파이프 조기 종료가 세 OS 에서 같은 결과(조용한 종료 코드 0)로 끝난다.
  crash report 디렉토리에 실제 크래시만 남는다. 폴링 루프(`plugin audit-follow` 등)도
  읽는 쪽이 사라지면 다음 레코드 출력 시점에 빠져나온다(빈 버퍼 flush 는 write 를 내지
  않아 EPIPE 를 못 잡으므로 flush 는 탈출 지점이 아니다; 출력이 더 없으면 종전처럼 대기).
- **잃은 것**: 출력 함수들이 `-> Result<()>` 를 돌려주게 되어 호출부마다 `?` 가 붙는다.
  coreutils 관례(141)와는 다르므로 셸 파이프라인에서 `PIPESTATUS` 로 "잘렸음" 을 구분할
  수 없다 — tasty CLI 의 1 차 소비자는 셸 관례가 아니라 종료 코드로 성패를 읽는
  에이전트라 이쪽을 택했다.
- **운영 비용 / 유지 부담**: 새 CLI 출력은 반드시 `outln!`/`out!` 을 써야 한다.
  `tests/cli_stdout_broken_pipe.rs` 가 (a) 세 갈래의 EPIPE 동작과 (b) tasty-cli 소스에
  `println!`/`print!` 가 없음을 `cargo test --workspace` 로 강제한다.

## Alternatives Considered

- **SIGPIPE 를 `SIG_DFL` 로 복원(종료 코드 141)**: 코드 변경이 한 줄이지만 (1) Windows 가
  해결되지 않고(SIGPIPE 없음) `Result` 처리가 어차피 필요하며, (2) 종료 코드 141 은
  에이전트에게 실패로 읽히고, (3) host 프로세스에 새면 자식 stdin 쓰기에서 host 가 죽는다.
  CLI 갈래에만 격리한다 해도 (1)(2) 는 남는다.
- **panic hook 에서 프로세스 역할을 구분해 CLI panic 은 리포트하지 않기**: crash report 만
  사라질 뿐 종료 코드 101 과 stderr 의 panic 메시지는 그대로다. CLI 의 진짜 panic 까지
  리포트에서 빠진다.
- **래퍼 안에서 `BrokenPipe` 를 삼키고 계속 진행(`Ok(())`)**: 시그니처 변경이 필요 없지만
  폴링/follow 루프가 읽는 쪽이 사라진 뒤에도 영원히 돈다(`tasty plugin audit-follow | head -1`
  이 끝나지 않는다).
- **래퍼 안에서 `BrokenPipe` 시 `process::exit(0)`**: 루프는 끝나지만 `Drop` 이 실행되지
  않는다 — `remote workspaces | head -1` 처럼 SSH 터널(`SshTunnel::drop` 이 ssh 자식을
  kill)을 든 명령이 자식 ssh 를 고아로 남긴다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 셸 파이프라인 사용자가 잘린 출력을 종료 코드로 구분해야 하는 실제 사례가 보고된다(141
  또는 별도 코드 요구).
- Rust 안정 채널에 `#[unix_sigpipe]` 류 attribute 가 들어와 런타임 정책 자체를 바꿀 수
  있게 되고, 그것이 Windows 까지 포함해 같은 결과를 준다.
- host 프로세스가 stdout 에 쓰는 경로가 생긴다(현재는 없음 — 그때는 이 경계가 host 를
  덮지 않으므로 별도 판단이 필요하다).

## References

- [dev-guide/error-handling.md](../dev-guide/error-handling.md) "stdout 쓰기 (CLI 클라이언트)"
- [dev-guide/cli-structure.md](../dev-guide/cli-structure.md) "stdout 출력"
- [ADR-0092](0092-file-log-host-process-only.md) — 같은 바이너리 안에서 host 와 CLI
  클라이언트의 역할을 가르는 선례
- 구현: `crates/tasty-cli/src/out.rs`, 회귀 테스트 `tests/cli_stdout_broken_pipe.rs`
- 짝 결정: [ADR-0513](0513-cli-stderr-broken-pipe-keeps-the-exit-code.md) — stderr 쪽(쓰기 실패를 버리고 종료 코드 유지)
