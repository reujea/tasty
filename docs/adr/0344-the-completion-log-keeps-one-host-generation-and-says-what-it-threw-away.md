# ADR-0344: 완료 알림 로그는 호스트 세대 하나를 들고, 버린 양을 말한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: notify, retention, logging, plugin, instance-identity, adr-0330
- **Group**: agent-integration

## Context

완료 알림 로그(`<parent_home>/notify/<caller_surface>.log`)는 `terminal.tell` 주입을
대체한 **실제 사용자 경로**다. 그런데 그 로그가 **무엇을 얼마나 보존하는지**가 어디에도
한 자리로 적혀 있지 않았다. 사실은 세 군데에 흩어져 있었다 — 바이트 상한은
`crates/tasty-utils/src/notify.rs` 의 상수 주석에, 부팅 시 디렉토리 전량 삭제는
`TcpIpcServer::clear_notify_then_publish_port` 의 주석에, 한 줄의 원자성은
[ADR-0330](0330-one-completion-line-is-one-write.md) 에.

그래서 이 로그를 읽는 쪽이 대답할 수 없는 물음이 셋 있었다.

1. **보존 범위** — 상한이 바이트 하나인가, 시간도 있는가, 파일 수는? 그리고 cap 에
   닿으면 "마지막 256 KiB 를 남기는" 것인가 "전량 버리는" 것인가.
2. **유실** — 비울 때 뒤처진 reader 의 미독분이 함께 사라진다. 정본 문서가 그 사실을
   **"사라졌다는 사실은 어디에도 남지 않는다"** 라고 적고 있었다. 즉 손실이 설계상
   보이지 않았다.
3. **인스턴스 정체성** — `surface_id` 는 호스트 실행마다 1 부터 다시 발급된다
   (`IdGenerator::next_surface`). 그래서 이전 실행의 파일과 이번 실행의 파일은 **이름이
   겹친다.** 겹침을 막는 것은 파일 안의 표식이 아니라 부팅 시 디렉토리 삭제 하나인데,
   그 삭제가 **데이터 루트 단위**라는 사실과 그것이 어떤 전제 위에 서 있는지가 안
   적혀 있었다.

3 번의 전제가 깨지는 자리를 실측으로 확인했다. `--port-file`(도움말: "for test
isolation")은 **포트 파일만** 옮기고 청소 대상은 `tasty_home()/notify` 로 남는다. 격리
홈에 호스트 A 를 띄우고 `notify/9.log` 를 남긴 뒤, **같은 홈**에 `--port-file` 만 다른
호스트 B 를 띄웠더니 A 가 살아 있는 채로 A 의 `notify/` 가 통째로 사라졌다
(2026-09-20, debug 빌드, Xvfb `:78`). 즉 "한 데이터 루트에 호스트 하나" 는 지켜지는
성질이 아니라 **지켜져야 하는 전제**였고, 그것이 어디에도 적혀 있지 않았다.

## Decision

**이 로그의 보존 범위를 세 축으로 못박고, 버리는 양을 로그에 남긴다.**

- **정체성은 경로다** — 한 완료 로그의 정체성은 **(데이터 루트, caller surface id)**
  이고, 그 외에 인스턴스를 가리키는 표식은 줄에도 파일에도 없다. 데이터 루트는
  `TASTY_PARENT_HOME`(없으면 `tasty_home()`)이 정한다. 줄 포맷과 경로 규약은 **안
  바꾼다** — `tail -n0 -F "$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log"` 가 문서화된
  사용자 경로이고, 정체성을 줄에 실으면 그 줄을 완료 통지로 읽는 계약이 깨진다.
- **세대는 부팅 삭제가 정한다** — 보존 범위는 **지금 호스트 세대 하나**다. 재시작을
  사이에 두고 과거 줄을 되읽을 방법은 없다. 이 보장은 **"한 데이터 루트에 호스트
  하나"** 를 전제하며, 이것을 암묵이 아니라 **계약으로 적는다.** 전제가 깨지면 나중에
  뜬 호스트가 먼저 뜬 호스트의 **살아 있는** 로그를 지운다.
- **크기 축은 바이트 하나다** — 시간 상한도 파일 수 상한도 **없다**. 닫힌 surface 의
  파일은 그 세대가 끝날 때까지 남고, 회수는 다음 부팅의 디렉토리 삭제뿐이다. 그리고
  cap 은 **전량 폐기**다: 실제 보존량은 0 과 cap 사이를 톱니로 오간다.
- **버린 양을 숨기지 않는다** — 비울 때마다 **버린 바이트 수**를 `tracing::warn!` 로
  남긴다. 이 파일 **자신에는 쓰지 않는다** — ADR-0330 이 같은 물음에서 기각한 대안이
  그것이고(읽는 쪽이 그 줄을 완료 통지로 읽는다), 기각 사유는 그대로 유효하다. 손실을
  적는 자리를 **읽는 쪽 계약 밖**으로 옮겨 두 요구("유실을 숨기지 않는다" 와 "한 줄 =
  완료 통지")를 동시에 만족시킨다.

**미리 정하지 않는 것**: 세그먼트 rotation, durable feed, 파일 수 상한, 세대 표식을
경로나 줄에 싣는 것. 넷 다 읽는 쪽 계약이나 디스크 모양을 바꾸고, 지금 필요한 것은
보존 범위를 **아는 것**이지 늘리는 것이 아니다.

## Consequences

- **얻은 것**: 읽는 쪽이 "이 파일이 답할 수 있는 물음의 크기" 를 한 자리에서 안다.
  비우기가 일어난 사실과 그 양이 값으로 남아, 완료 통지가 안 온 사고에서 "유실이었나"
  를 사후에 가를 수 있다 — 그 전에는 파일이 0 바이트라 아무도 되물을 수 없었다.
- **잃은 것**: cap 도달이 `warn` 로그 한 줄을 만든다. 한 caller surface 에 수천 건이
  쌓여야 닿는 지점이라 평시 소음은 0 이다.
- **운영 비용 / 유지 부담**: `tasty-utils` 는 번들 plugin 전부의 의존 폐포 안이라 이
  변경은 plugin 버전 bump 를 함께 요구한다.
- **안 고친 것**: `--port-file` 이 청소 대상을 안 옮기는 것은 그대로다. 위 Context 의
  실측이 그 자리를 가리키고, 고칠 자리는 `TcpIpcServer::clear_notify_then_publish_port`
  가 청소 대상을 고르는 한 줄이다.
  → [ADR-0416](0416-the-boot-cleanup-follows-the-port-file-root.md) 이 고쳤다. 포트 파일이
  데이터 루트 밖이면 청소를 건너뛴다. 이 ADR 의 세 축은 그대로다.

## Alternatives Considered

- **세대 id 를 경로에 넣는다**(`notify/<generation>/<surface>.log`) — 겹침이 원천
  차단되고 부팅 삭제도 세대 단위가 된다. 그러나 문서화된 reader 경로
  (`$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log`)가 그대로 깨진다. 읽는 쪽이
  에이전트 운영 관례에 박혀 있어 대가가 훨씬 크다.
- **세대 id 를 줄에 넣는다** — 경로는 그대로지만 줄이 바뀐다. 줄은 앱 언어를 타는
  사람 대상 문구이고, 읽는 쪽이 "한 줄 = 완료 통지 하나" 로 세고 있다.
- **비운 사실을 그 파일에 한 줄로 적는다** — ADR-0330 이 이미 기각했다. 기각 사유가
  여기서도 그대로다.
- **1 단 rotation**(`hook-failures.log` 가 쓰는 방식, `crates/tasty-cli/src/hook_failure.rs`)
  — 미독분이 `.log.1` 에 남아 사후 복구가 된다. 같은 데이터 루트 안에 선례가 있어
  자연스러운 선택이지만, 보존량이 2 배가 되고 `tail -F` 의 미독분 무손실을 보장하지도
  않는다. 이번 결정의 범위는 보존 범위를 **정의하는 것**이고, 늘리는 처방을 미리
  고정하지 않는다.
- **시간 상한을 더한다** — 이 로그의 수명이 호스트 세대 하나라 시간 축이 겹친다.
  세대보다 짧은 시간 상한은 살아 있는 세션의 완료 기록을 지우고, 긴 것은 아무 일도
  안 한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `notify.rs` 의 `truncate_reports_how_many_bytes_it_threw_away` 가 깨진다. 그 시험은
  비우기가 **버린 양을 값으로 돌려주는지**를 잰다 — 그 값이 없으면 손실을 적을 수 없다.
- `hitting_the_cap_discards_the_whole_file_not_just_the_excess` 가 깨진다. 그 시험은
  cap 이 **전량 폐기**인지를 잰다. 초과분만 잘라내도록 바뀌면 보존 범위를 "마지막
  비우기 이후" 로 말할 수 없게 된다.
- `notify_log_path_in` 이 `<home>/notify/<surface>.log` 말고 다른 모양을 만든다 —
  경로가 곧 정체성이라는 이 결정의 첫 축이 그때 바뀐다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **"한 데이터 루트에 호스트 하나" 전제가 깨지는 것.** 레포에는 호스트 수를 세는 자리가
  없다. 재는 법: 격리 홈 하나를 `TASTY_HOME` 으로 주고 호스트를 띄운 뒤
  `<home>/notify/<임의 번호>.log` 에 한 줄을 남기고, **같은 홈**으로 두 번째 호스트를
  (`--port-file` 을 다른 경로로) 띄운다. 그 파일이 사라지면 전제가 깨진 것이다.
  (정정 2026-09-21: [ADR-0416](0416-the-boot-cleanup-follows-the-port-file-root.md) 이후에는
  포트 파일이 데이터 루트 **밖**이면 청소를 건너뛰므로 이 절차로는 안 사라진다. 전제 위반을
  재려면 두 번째 호스트의 `--port-file` 을 **데이터 루트 안의 다른 이름**으로 준다.)
- **버린 양이 실제로 로그에 나가는 것.** 반환값에는 채널이 있으나 `tracing` 출력에는
  없다 — `tasty-utils` 는 구독자를 갖지 않는 leaf crate다. 재는 법: 호스트를 띄우고
  한 caller surface 의 완료 로그를 cap 위로 넘긴 뒤, 그 plugin 의 로그
  (`<home>/plugins-logs/<plugin id>.log`)에서 `hit the` 로 시작하는 줄과 거기 실린
  바이트 수를 찾는다.
- 닫힌 surface 의 파일이 세션 중 쌓여 디스크가 문제가 되는 것. 재는 법: 오래 뜬
  인스턴스에서 `<home>/notify/` 의 파일 수와 합계 크기를 잰다. 파일 수 상한을 두는
  것이 그때의 처방 후보다.

## References

- 구현(현재 위치): `crates/tasty-utils/src/notify.rs` 의 모듈 문서 "보존 범위" 절 ·
  `NOTIFY_LOG_CAP_BYTES` · `truncate_over_cap` · `append_line_to`.
- 세대를 정하는 자리(**다른 파일**): `TcpIpcServer::clear_notify_then_publish_port` 와
  `clear_notify_dir` — 청소 대상을 `tasty_home()` 으로 고르는 곳이 여기다. 포트 파일 뿌리와의
  비교는 `TcpIpcServer::notify_dir_to_clear`([ADR-0416](0416-the-boot-cleanup-follows-the-port-file-root.md)).
- 후속 확장: [ADR-0415](0415-a-resuming-completion-log-reader-learns-what-it-lost-from-a-sidecar.md) — 버린 양을 `tracing` 에 더해 옆 메타 파일의 누계 `retention_start` 로도 남긴다(재개 reader 용). 이 ADR 의 세 축은 바꾸지 않는다.
- 후속 해소: [ADR-0416](0416-the-boot-cleanup-follows-the-port-file-root.md) (Consequences 의 "안 고친 것").
- 한 줄의 원자성(이 ADR 이 바꾸지 않는 것): [ADR-0330](0330-one-completion-line-is-one-write.md).
- 같은 데이터 루트의 다른 파일 로그가 쓰는 1 단 rotation:
  `crates/tasty-cli/src/hook_failure.rs` · [ADR-0075](0075-agent-hook-delivery-failure-record.md).
- `memory.db` 쪽 관측 로그의 보존 정책(매체가 달라 표를 공유하지 않는다):
  `src/store/log_retention.rs` · [ADR-0085](0085-ipc-log-retention-bounded.md).
- 사용자 경로와 크기 관리: [dev-guide/external-interaction/child-completion-notify-log.md](../dev-guide/external-interaction/child-completion-notify-log.md)
