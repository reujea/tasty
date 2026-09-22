# ADR-0513: CLI 의 stderr 쓰기 실패는 버리고 명령의 종료 코드를 그대로 둔다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: cli, stderr, epipe, exit-code, crash-report, error-handling, adr-0101
- **Group**: cli-logging

## Context

[ADR-0101](0101-cli-stdout-broken-pipe-exit-zero.md) 은 stdout 쪽만 다뤘다. tasty-cli 는 stderr
에 `eprintln!` 으로 썼고, `eprintln!` 은 `println!` 과 같이 쓰기 실패를 panic 으로 승격한다
(`failed printing to stderr: Broken pipe`).

실측(2026-09-23, 격리 `TASTY_HOME` · headless debug 바이너리): stderr 의 읽는 쪽을 먼저 닫고
`tasty nosuchcmd` · `tasty window list` · 필수 인자가 빠진 `tasty read since-mark` 를 부르면
세 경우 모두 종료가 SIGABRT(`-6`)였고 `crash-reports/` 에 리포트가 하나씩 남았다. 셋 다
clap 파싱 오류를 여러 줄로 찍는 `help::format_parse_error` 경로다. `tasty window list 2>&1 | head -20`
처럼 오류 출력이 긴 명령을 파이프로 자르면 실사용에서 밟는다.

stdout 과 다른 점이 하나 있다. stdout 이 닫힌 것은 "더 쓸 곳이 없다" 는 신호라 ADR-0101 은
종료 코드 0 을 골랐다. stderr 에 쓰는 것은 대개 **이미 실패한 명령**이다.

## Decision

**tasty-cli 는 stderr 에 `crates/tasty-cli/src/out.rs` 의 `errln!` 으로만 쓰고, 그 쓰기의
실패는 버린다. 종료 코드는 호출자가 원래 내던 값(파싱 오류 2 · 실패 1 등) 그대로다.**

- stderr 는 CLI 가 실패를 알리는 마지막 채널이라 그 쓰기가 실패하면 더 알릴 곳이 없다.
  그래서 `errln!` 은 값을 돌려주지 않는다(`?` 로 올릴 이유가 없다).
- stdout 규칙(ADR-0101)은 바뀌지 않는다.
- 소스 스캔(`tests/cli_stdout_broken_pipe.rs` 의 `cli_crate_has_no_direct_stderr_print`)이
  `eprintln!`/`eprint!` 의 복귀를 막는다.

## Consequences

- **얻은 것**: `2>&1 | head` 로 잘린 오류 출력이 가짜 crash report 와 abort 가 되지 않는다.
  호출자는 stderr 를 닫든 말든 같은 종료 코드를 본다.
- **잃은 것**: stderr 가 EPIPE 가 아닌 이유(EIO 등)로 실패해도 조용하다. 알릴 채널이 없다는
  사정은 같다.
- **운영 비용 / 유지 부담**: 없음 — 강제는 기존 시험 파일의 스캔 하나다.

## Alternatives Considered

- **ADR-0101 처럼 종료 코드 0 으로 접는다** — stderr 에 쓰는 것은 대개 실패 보고다. 파이프가
  닫혔다는 이유로 실패가 성공이 되면 호출자가 오판한다.
- **panic hook 에서 `failed printing to stderr` 를 알아보고 조용히 끝낸다** — 모든 경로를 한
  자리에서 덮지만, 메시지 문자열에 기대고 panic 이 나는 시점의 종료 코드를 정할 방법이 없다.
- **stderr 쓰기도 `Result` 로 전파한다** — 전파받은 쪽이 할 수 있는 일이 없다(알릴 곳이 stderr
  였다). 호출부 75 곳에 `?` 만 늘어난다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- CLI 가 stderr 외의 보고 채널(파일 로그 등)을 갖는다. 그때 stderr 실패를 거기 남길지 다시 정한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- stderr 가 닫힌 채 "성공처럼 보이는" 실행이 문제로 보고된다. 재는 법: 이슈·사용자 보고.

## References

- [ADR-0101](0101-cli-stdout-broken-pipe-exit-zero.md) — stdout 쪽 결정. 이 ADR 은 그것을 바꾸지 않는다.
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-cli/src/out.rs` 의 `err_line` / `errln!`
- 시험: `tests/cli_stdout_broken_pipe.rs` 의 `parse_error_with_closed_stderr_keeps_its_exit_code` · `unreachable_host_with_closed_stderr_keeps_its_exit_code` · `cli_crate_has_no_direct_stderr_print`
- [`docs/dev-guide/error-handling.md`](../dev-guide/error-handling.md) "stdout 쓰기 (CLI 클라이언트)"
