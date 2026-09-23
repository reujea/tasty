# ADR-0416: 부팅 청소는 포트 파일과 같은 뿌리일 때만 돈다 — ADR-0344 의 "안 고친 것" 해소

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: notify, retention, instance-identity, port-file, boot, adr-0344
- **Group**: agent-integration

## Context

[ADR-0344](0344-the-completion-log-keeps-one-host-generation-and-says-what-it-threw-away.md)
는 완료 알림 로그의 보존 범위를 **호스트 세대 하나**로 정했다. 그 경계는 호스트가 부팅 때
`<데이터 루트>/notify/` 를 통째로 지우는 것 하나가 만들고, 그 보장은 **"한 데이터 루트에
호스트 하나"** 를 전제한다.

같은 ADR 이 그 전제가 깨지는 자리를 실측했다. `--port-file`(도움말: "for test isolation")은
포트 파일만 옮기고 청소 대상은 `tasty_home()/notify` 로 남는다. 그래서 격리 홈에 호스트 A 를
띄우고 같은 홈에 `--port-file` 만 다른 호스트 B 를 띄우면, A 가 살아 있는 채로 A 의 `notify/`
가 사라졌다. ADR-0344 는 이것을 **"안 고친 것"** 으로 남기고 고칠 자리를 지목했다 —
`TcpIpcServer::clear_notify_then_publish_port` 에 청소 대상을 넘기는 한 줄이다.

"포트 파일과 같은 뿌리로" 를 글자 그대로 읽으면 청소 대상을 `<포트 파일 디렉토리>/notify` 로
옮기는 것이다. 그런데 완료 로그의 **writer 는 그 자리에 안 쓴다.** writer(plugin)와
reader(conductor 셸)는 호스트가 주입한 `TASTY_PARENT_HOME` 을 보고, 그 값은 호스트의
`tasty_home()` 이다(`crates/tasty-host-plugin/src/process.rs` · `crates/tasty-terminal/src/lib.rs`).
포트 파일 위치는 거기에 영향을 주지 않는다. 그리고 `TASTY_PARENT_HOME` 은 완료 로그 경로만이
아니라 plugin 의 사용자 언어 파일 자리(`<루트>/lang`)도 정한다.

## Decision

**부팅 청소 대상은 데이터 루트의 `notify/` 그대로 두고, 포트 파일이 데이터 루트 밖에 있으면
청소를 건너뛴다.**

- **판정은 뿌리 비교 하나다** — `TcpIpcServer::notify_dir_to_clear` 가 포트 파일의 디렉토리와
  데이터 루트(`tasty_home()`)를 견준다. 같으면(기본 포트 파일은 언제나 `<루트>/tasty.port` 다)
  `<루트>/notify` 를 준다. 다르면 `None` 을 준다. 그러면 청소는 건너뛰고 포트 파일만 쓴다.
- **기본 부팅은 아무것도 안 바뀐다** — `--port-file` 이 없으면 예전과 같은 디렉토리를 같은
  순서(청소 → 포트 파일)로 지운다.
- **포트 파일을 데이터 루트 밖으로 옮긴 호스트는 그 루트의 주인임을 알리지 않은 것으로 본다.**
  같은 루트에 기본 포트 파일로 뜬 호스트가 따로 있을 수 있으므로 그 루트의 완료 로그를 지우지
  않는다. 건너뛴 사실은 `info` 로그로 남긴다.
- **writer 뿌리는 안 옮긴다** — `TASTY_PARENT_HOME` 도, 경로 규약
  `$TASTY_PARENT_HOME/notify/$TASTY_SURFACE_ID.log` 도 그대로다.
- **"한 데이터 루트에 호스트 하나" 는 여전히 전제다.** 이 결정은 그 전제가 깨졌을 때 **지우는
  사고**를 막을 뿐이다. 두 호스트가 한 루트를 공유할 때 **이름이 겹치는 문제**는 그대로
  남는다(surface id 가 둘 다 1 부터다).

## Consequences

- **얻은 것**: ADR-0344 가 실측한 사고가 닫힌다. `--port-file` 로 띄운 두 번째 호스트가 먼저
  뜬 호스트의 살아 있는 완료 로그를 지우지 않는다. 회귀 시험은
  `notify_cleanup_tests::a_port_file_outside_the_data_root_leaves_the_notify_dir_alone` 이다.
- **얻은 것**: 기본 부팅과, 포트 파일을 데이터 루트 안의 다른 이름으로 둔 부팅은 관측
  가능한 동작이 그대로다.
- **잃은 것 — 루트 밖 포트 파일로 뜬 호스트는 자기 세대 경계를 못 만든다.** 그 호스트의
  writer 는 공유 루트의 `notify/` 에 쓰는데, 그 호스트는 그 디렉토리를 지우지 않는다. 그래서
  이전 실행의 파일이 남을 수 있다. `tail -n0 -F` reader 는 arm 시점 이후만 읽으므로 영향이
  없다. 파일 처음부터 읽는 reader 는 과거 줄을 볼 수 있다. 옆 메타 파일의 누계
  (`retention_start`, [ADR-0415](0415-a-resuming-completion-log-reader-learns-what-it-lost-from-a-sidecar.md))도
  이전 실행 값에서 이어지므로, **청소 없는 루트에서 새로 붙는 reader 는 0 이 아니라 현재
  `retention_start` 에서 시작해야 한다** — 0 에서 시작하면 이번 세대에 잃은 것이 없는데도
  `truncated` 와 이전 세대 누계만큼의 `skipped` 를 받는다. 테스트 하네스(`tests/common` 이
  포트 파일을 임시 디렉토리에, 홈을 새 격리 디렉토리에 둔다)는 홈이 매번 새것이라 지울 것이
  애초에 없다.
- **잃은 것**: 두 호스트가 한 루트를 공유할 때 생기는 이름 겹침(같은 surface 번호의 줄이 한
  파일에 섞임)은 안 닫힌다. 그것을 닫으려면 writer 뿌리를 호스트마다 나눠야 한다(아래 대안 ⒝).
- **운영 비용**: 부팅 경로에 `canonicalize` 두 번이 더해진다.

## Alternatives Considered

- **⒜ 청소 대상을 `<포트 파일 디렉토리>/notify` 로 옮긴다** — 지시문 글자 그대로의 형태다.
  안 고른 이유: writer 가 그 디렉토리에 안 쓰므로, 루트 밖 포트 파일에서는 **아무도 안 쓰는
  디렉토리를 지우는 동작**이 된다. 그러면서 writer 가 실제로 쓰는 디렉토리의 청소는 멈춘다.
  포트 파일이 데이터 루트 안에 있으면 이 결정과 같은 결과다. 밖에 있으면 "안 지운다" 는 결과가
  같다. 그러면서 쓸모없는 삭제만 하나 더한다.
- **⒝ writer 뿌리까지 포트 파일을 따라가게 한다** — `--port-file` 호스트가 자식에게
  `TASTY_PARENT_HOME=<포트 파일 디렉토리>` 를 주입한다. 이름 겹침까지 닫힌다. 안 고른 이유:
  `TASTY_PARENT_HOME` 은 "호스트의 데이터 루트" 라는 뜻으로 plugin 의 사용자 언어 파일 자리도
  정한다. 거기에 데이터 루트가 아닌 값을 실으면 그 뜻이 거짓이 된다. 완료 로그 전용으로 새
  env 를 두면 문서화된 reader 경로(`$TASTY_PARENT_HOME/notify/...`)가 `--port-file` 호스트에서
  깨진다. 호환을 가장 많이 지키는 쪽을 골랐다.
- **⒞ 청소 전에 기본 포트 파일의 포트로 연결해 보고, 살아 있으면 건너뛴다** — 전제 위반을 직접
  감지한다. 안 고른 이유: 낡은 포트 파일의 번호를 다른 프로세스가 쓰고 있으면 거짓 "살아 있음"
  이 난다. 그리고 판정이 네트워크 상태에 걸려 시험으로 고정하기 어렵다. 경로 비교는 결정적이다.
- **⒟ 그대로 둔다(ADR-0344 의 상태)** — 사고가 재현되는 채로 남는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 루트 밖 포트 파일인데 청소 대상이 나오게 되면
  `notify_cleanup_tests::a_port_file_outside_the_data_root_leaves_the_notify_dir_alone` 이
  빨개진다.
- 기본 부팅이 청소를 안 하게 되면
  `notify_cleanup_tests::without_a_port_file_override_the_data_root_notify_dir_is_cleared` 가
  빨개진다.
- 호스트가 자식에 주입하는 `TASTY_PARENT_HOME` 이 `tasty_home()` 이 아닌 값을 갖게 되면 ⒝ 가
  착지한 것이다. 이 판정은 청소 뿌리와 writer 뿌리를 따로 고르는 것을 전제한다.
  재는 자리는 `crates/tasty-host-plugin/src/process.rs` 의 `inject_plugin_data_env` 와
  `crates/tasty-terminal/src/lib.rs` 의 `Terminal::new` 다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **한 루트를 두 호스트가 공유하는 운용이 실제로 생기는가**(이름 겹침이 문제가 되는가).
  재는 법: 한 데이터 루트의 `notify/<n>.log` 에서 서로 다른 호스트의 child 가 쓴 줄이 섞인
  보고가 나오는지 본다. 그때가 ⒝ 의 시점이다.

## References

- 개정 대상이 아니라 해소 대상: [ADR-0344](0344-the-completion-log-keeps-one-host-generation-and-says-what-it-threw-away.md)
  의 Consequences "안 고친 것". ADR-0344 의 세 축(정체성 · 세대 · 크기)은 바꾸지 않는다.
- 청소 순서(청소 → 포트 파일)는 바꾸지 않는다 — `clear_notify_then_publish` 의 계약.
- **코드 근거 (결정이 실현된 현재 위치)**: `src/adapters/production/tcp_ipc_server.rs` 의
  `TcpIpcServer::notify_dir_to_clear` · `start_with_port_file` · `clear_notify_then_publish_port`.
- 사용자 경로: [dev-guide/external-interaction.md](../dev-guide/external-interaction.md#child-완료-알림--completion-log).
