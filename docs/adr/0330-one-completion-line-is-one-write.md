# ADR-0330: 완료 알림 한 줄은 한 번의 write 다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: notify, concurrency, plugin, logging
- **Group**: agent-integration

## Context

완료 알림 로그(`<parent_home>/notify/<caller_surface>.log`)는 **writer 가 여럿인 파일**이다.
한 caller surface 밑에 claude child 와 codex child 가 함께 뜨면 서로 다른 두 프로세스가
각자의 plugin 훅에서 같은 파일에 append 한다. 읽는 쪽은 `tail -F` 라 **줄이 단위**다 —
줄 하나가 깨지면 그 완료 통지는 사라지는 것이 아니라 **잘못 읽힌다.**

`tasty-utils` 의 공유 append 헬퍼는 그 상황을 세 군데서 못 버텼다.

1. 크기 판정(`metadata`)과 파일 열기가 떨어져 있어 두 writer 가 **둘 다** 비우기로 판정할
   수 있었다.
2. 비우기를 **쓰기 핸들에 섞었다.** 파일이 cap 을 넘으면 그 핸들은 append 가 꺼진 채
   열려 offset 0 부터 쓰므로, 다른 writer 가 방금 append 한 줄의 앞부분을 덮어 **양쪽
   어느 줄도 아닌 잔해**를 남겼다.
3. 한 줄을 `writeln!` 로 보냈다. `O_APPEND` 가 보장하는 것은 **한 번의 `write` 가 끝에
   통째로 붙는 것**뿐인데 `writeln!` 은 인자와 개행을 나눠 보낼 수 있고, 그 사이에 다른
   프로세스의 append 가 끼면 두 줄이 섞였다.

3 번이 범위가 가장 넓다 — cap 근처가 아니어도, 비우기를 한 번도 안 지나도 깨진다.

두 프로세스를 같은 파일에 붙여 열 회차를 돌렸을 때 **682 줄**이 어느 쪽 줄도 아닌 잔해로
남았고, 겹친 회차 아홉 중 아홉에서 났다. 예외 상황이 아니라 **동시 writer 가 있는 내내**
나는 상태였다.

## Decision

**한 줄은 한 번의 `write` 로 보내고, 쓰기 핸들은 언제나 append 다.**

- 줄과 개행을 버퍼 하나로 합쳐 `write_all` 을 **한 번** 부른다.
- 비우기는 쓰기와 **분리된 단계**다. 별도 핸들에서 크기를 **다시 재고** 그때도 cap 을
  넘을 때만 `set_len(0)` 한다 — 바깥 판정과 여기 사이에 다른 writer 가 이미 비웠으면
  그 writer 가 새로 쓴 줄을 지우게 되기 때문이다.
- 이 헬퍼의 불변식을 **"한 줄은 통째로 남거나 통째로 없다"** 로 못박는다. 줄이 잘리거나
  섞이는 상태는 허용하지 않는다.

**손실의 범위를 정의한다.** 비우기 직전에 append 된 줄은 사라진다. 그 줄은 파일이 이미
cap 을 넘은 뒤에 쓰인 것이라 **애초에 이 비우기가 버릴 구간**이다. 즉 손실은 "cap 을 넘은
시점 이전" 으로 한정되고, 사라지는 단위는 **줄**이다.

## Consequences

- **얻은 것**: 동시 writer 상황에서 완료 통지가 잘못 읽히지 않는다. 같은 측정에서
  잔해가 682 → 0 으로 갔다. 사용자에게 보이는 동작은 그 외에 바뀌지 않는다 — cap 도,
  줄의 모양도, `tail -F` 가 받는 흐름도 그대로다.
- **잃은 것**: 한 줄마다 버퍼를 하나 만든다. 줄이 짧아 무시할 수 있는 비용이고, 대신
  포맷 조각 수만큼 나던 syscall 이 하나로 준다.
- **운영 비용 / 유지 부담**: `tasty-utils` 는 번들 plugin 전부의 의존 폐포 안에 있어
  이 변경은 plugin 아홉 개의 버전 bump 를 함께 요구한다.

## Alternatives Considered

- **파일 락(flock)** — writer 가 서로 다른 프로세스라 락 자체는 성립한다. 그러나 락은
  죽은 프로세스가 쥔 채로 남을 수 있고, 그때 완료 통지가 **전부** 멈춘다. 지금 구조는
  락 없이 커널의 `O_APPEND` 원자성만으로 충분하다 — 한 줄이 한 write 이기만 하면 된다.
- **줄마다 파일을 나눈다** — 잔해 문제는 사라지지만 `tail -F <파일 하나>` 라는 문서화된
  사용자 경로가 깨진다. 읽는 쪽 계약을 바꾸는 대가가 훨씬 크다.
- **비우기를 없애고 무한히 자라게 둔다** — cap 은 무한 성장 방어다. 없애면 잔해 대신
  디스크가 문제가 된다.
- **비우기 전에 잃은 줄 수를 파일에 적는다** — 사건 피드가 보존 밖 요청에 `skipped` 를
  함께 주는 것과 같은 모양이다. 이번 결정의 범위 밖으로 뒀다: 그것은 **읽는 쪽 계약을
  바꾸는 일**이고(`tail -F` 를 보는 사람이 그 줄을 완료 통지로 오독할 수 있다), 여기서
  고치려던 것은 줄이 깨지는 것이지 손실이 안 보이는 것이 아니다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `notify.rs` 의 `concurrent_writers_never_leave_a_partial_line` 이 깨진다. 그 시험은
  한 줄이 한 write 로 나가는지를 잰다.
- 같은 파일에 쓰는 writer 가 두 plugin 말고 더 생긴다 — 그때는 "한 줄 = 한 write" 로
  충분한지 다시 따져야 한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 비우기 갈래의 경합. 위 시험은 그 갈래를 안 지난다(지나게 하면 뒤의 비우기가 앞의
  증거를 지워 시험이 간헐적으로 통과한다). 재는 법: 공개 API `append_notify_line` 을
  부르는 writer 를 **OS 프로세스 둘 이상**으로 띄워 같은 caller surface 로 출하 cap
  (`NOTIFY_LOG_CAP_BYTES`)을 여러 번 넘기게 하고, 남은 파일에서 두 writer 중 어느 쪽
  모양도 아닌 줄을 센다. 0 이 아니면 이 결정이 깨진 것이다.
- 손실 범위가 "cap 을 넘은 시점 이전" 밖으로 벗어나는 것. 재는 법: 같은 harness 에서
  각 writer 가 쓴 줄 번호를 단조 증가로 두고, 남은 파일에 **cap 을 넘기 전 구간의
  번호가 빠진 채** 뒤 구간이 남아 있는지 본다.

## References

- 구현(현재 위치): `crates/tasty-utils/src/notify.rs` 의 `append_line_to` ·
  `truncate_over_cap`.
- 호출자(**셋**): `crates/tasty-plugin-claude/src/notifications.rs` 의
  `handle_notify_done`(완료) · `handle_notify_error`(에러 후 정지),
  `crates/tasty-plugin-codex/src/handlers.rs` 의 `handle_notify_caller`.
  **claude 쪽은 `handlers.rs` 가 아니라 `notifications.rs` 다** — 두 plugin 의 파일
  이름이 다르므로 중괄호로 묶어 한 경로처럼 쓰면 claude 쪽이 실재하지 않는 자리를
  가리킨다. 위 Decision 의 불변식("한 줄은 한 번의 write")은 이 **세 자리 전부**에 건다.
- 사용자 경로와 크기 관리: [dev-guide/external-interaction/child-completion-notify-log.md](../dev-guide/external-interaction/child-completion-notify-log.md)
- 같은 모양의 물음(보존 밖 요청에 건너뛴 수를 함께 준다): [ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md)
