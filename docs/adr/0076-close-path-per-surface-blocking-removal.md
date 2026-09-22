# ADR-0076: surface close 정리 루프에서 다른 프로세스/스레드를 기다리는 구간을 걷어낸다

- **Status**: Accepted
- **Date**: 2026-08-22
- **Tags**: close-sequence, pty, observer, blocking, render-thread, latency, cross-platform, adr-0002
- **Group**: window-workspace-lifecycle

## Context

워크스페이스 close 는 한 번의 사용자 조작으로 leaf surface 수만큼
`AppState::cleanup_surface` 를 **렌더 스레드에서 직렬 반복**시키는 유일한 경로다.
surface 1 개 close 에서 무시할 만한 대기도 N 배가 되면 프레임 예산을 넘긴다.

`tasty::close` 계측([close-sequence](../architecture/close-sequence.md))이 그
분해를 이미 제공하고 있었고, 실측은 정리 루프의 비용이 **거의 전부 C5b
(`Terminal` drop)** 임을 가리켰다 — 탭 30 개 close 에서 C5 1530ms 중 C5b 가
1519ms. surface 당 약 50ms 의 큰 상수였고, 스크롤백 유무와 무관했다.

`PtyBackend::drop` 을 단계별로 쪼개 재보니 그 50ms 는 통째로 `child.kill()` 안의
`thread::sleep` 이었다:

```
kill_ms=50.257  spawn_ms=0.157  master_ms=0.067   (12 회 전부 kill_ms 50.2~50.4)
```

`portable-pty-0.8.1` 의 `impl ChildKiller for std::process::Child` (unix) 는
SIGHUP 을 보낸 뒤 자체 유예 루프에서 `try_wait` 를 최대 5 회, 사이사이
`thread::sleep(50ms)` 로 폴링한다. **첫 `try_wait` 는 SIGHUP 직후라 거의 항상 아직
안 죽은 상태로 걸리고, 두 번째 `try_wait` 는 성공한다** — 즉 셸이 정상적으로
곧바로 죽는 경로에서도 매번 50ms 를 통째로 잔다.

tasty 는 같은 Drop 안에서 이미 detached reap 스레드를 띄워 `waitpid` 폴링
(5ms × 40 회 = 200ms 상한) → SIGKILL escalation 을 수행한다. 메인 스레드의 유예
루프는 그것과 **완전히 중복**이며, 더 성길 뿐이다(50ms vs 5ms granularity).

두 번째 구간은 `ObserverRouter::drop_surface` 의 sink 워커 `join()` 이다. observer
가 붙은 surface 마다 워커 스레드가 끝날 때까지 렌더 스레드가 대기한다(실측: 파일
sink 12 개에서 C5c 8.9ms).

세 번째로 지목됐던 `fs::remove_file`(C5a)은 실측 결과 스크롤백이 없을 때 surface
당 약 5µs 로, 탭 30 개에서도 0.13ms 였다. 스크롤백이 만재라 실제로 지울 파일이
있을 때는 surface 당 약 0.5ms(탭 30 개 15ms)까지 오르지만, 그래도 같은 조건
close_total 403ms 의 4% 수준이다.

## Decision

**"자식 프로세스/워커 스레드가 끝나기를 기다리는 일"은 close 루프에서 전부
빼낸다.** 이미 확립된 방침(`PtyBackend::drop` 이 unix reap 을 detached thread 로
넘긴 선례)을 남은 구간에 그대로 적용한다.

- **PTY 자식 종료 (unix)**: `child.kill()` 대신 `libc::kill(pid, SIGHUP)` 을 직접
  보낸다. 유예 대기와 SIGKILL escalation 은 이미 존재하는 detached reap 스레드가
  전담한다. `process_id()` 를 얻지 못하는 예외 경로에서만 blocking `kill()` 로
  폴백한다. **Windows 는 손대지 않는다** — 그쪽 `kill()` 은 `TerminateProcess`
  한 방이라 유예 루프가 없다.
- **observer sink 워커**: surface close 로 인한 자동 해제(`drop_surface`)는 sender
  만 떨어뜨리고 join 을 **뒤로 미룬다**(`retire`). 이미 끝난 워커는 다음 close 때
  논블로킹으로 걷고(`reap_finished`), 남은 것은 앱 종료 시퀀스가 한 번에 회수한다
  (`join_retired`, 마커 `S3b`). 명시 해제(`output.observe_stop`)는 호출 복귀
  시점에 sink 가 닫혀 있기를 기대하는 API 라 **join 을 유지**한다 — 그 경로는
  surface 수만큼 반복되지 않는다.
- **`fs::remove_file`(C5a)은 그대로 둔다.** 측정된 비중이 스크롤백 없음에서
  close 전체의 0.01%, 만재에서도 4% 라 백그라운드 삭제기를 도입할 근거가 없다.

## Consequences

- **얻은 것** (Linux / debug 빌드 / 격리 `TASTY_HOME` / `path="gui"`, 스크롤백 없음):

  | 탭 수 | C5 cleanup | C5b terminal_drop | close RPC wall |
  |-------|-----------|-------------------|----------------|
  | 12 (전) | 609.6 | 604.7 | — |
  | 12 (후) | **2.4** | **1.5** | — |
  | 30 (전) | 1524.7 | 1513.8 | 1637 |
  | 30 (후) | **6.2** | **4.3** | **117** |

  탭 수에 대한 큰 상수 항이 사라져 close 소요가 탭 수에 대해 완만해졌다.

  벌크 캡처(C1) · surface purge 중복 제거가 함께 들어간 뒤 같은 조건을 다시 재면
  탭 30 개 close_total 이 1541ms → **33ms**(스크롤백 없음), 1834ms → **403ms**
  (만재 300k 라인)다. 만재 쪽 남은 최대 항은 이제 C5 계열이 아니라
  **C2b(스크롤백 디스크 write) 234ms** 이고, 그다음이 C1 97ms 다 —
  [close-sequence](../architecture/close-sequence.md) 의 실측 기준선 참조.
- **잃은 것**:
  - 자식 셸이 실제로 죽었는지를 `cleanup_surface` 복귀 시점에 **아무도 확인하지
    않는다.** 확인 책임 전체가 reap 스레드로 넘어갔다. 회수 보장은 그대로다
    (200ms 유예 후 SIGKILL + blocking `waitpid`) — 실측에서 탭 30 개 close 후
    자식 35→5, 스레드 128→68, zombie 0 으로 0.5 초 안에 정리됐다.
  - observer sink 의 마지막 항목이 sink 파일에 도달하는 **시점**이 close 복귀
    이후로 밀린다. 유실은 아니다 — `try_send` 로 채널에 수락된 항목은 sender 가
    전부 drop 된 뒤에도 `Receiver::recv` 가 버퍼를 끝까지 비운 다음에야 `Err` 를
    돌려주고(std mpsc 계약), 파일 sink 는 `File` 에 직접 `writeln!` 하므로
    유저스페이스 버퍼도 없다. 유일한 유실 경로인 "워커가 다 쓰기 전 프로세스 종료"
    는 종료 시퀀스의 `join_retired`(S3b)가 막고, 그 호출을 타지 않는 경로로
    라우터가 드롭돼도 `ObserverRouter::drop` 이 같은 회수를 한 번 더 한다. 즉
    S3b 는 정확성 요건이 아니라 회수를 앞당기는 최적화다.
  - 종료 시퀀스에 단계가 하나 늘었다(S3b).
- **운영 비용 / 유지 부담**:
  - reap 스레드는 여전히 터미널 1 개당 1 개 생성된다. 탭 30 개 close 시 순간
    스레드 128 개까지 올랐다가 0.5 초 안에 68(기준선)로 복귀 — 수명이 짧아 별도
    풀링을 도입하지 않는다.
  - `portable-pty` 의 유예 루프를 우회하므로, 그 크레이트가 unix `kill` 의
    시맨틱을 바꾸면 이 결정을 재확인해야 한다.

## Alternatives Considered

- **`Terminal` drop 전체를 detached thread 로 넘긴다** — 기각: `Terminal` 은
  `TerminalState` 를 `Arc<Mutex<..>>` 로 파서 스레드와 공유하고 호스트 인덱스에서
  막 제거된 참조라, 해제 시점을 렌더 스레드 밖으로 옮기면 close 후 재생성
  (respawn/restore)과의 순서 보장이 흐려진다. 50ms 상수의 출처가 `kill()` 하나로
  특정됐으므로 그 한 지점만 걷어내는 편이 표면적이 훨씬 작다.
- **Windows `ClosePseudoConsole` 도 detached thread 로 넘긴다** — **보류**. Linux
  실측에서 `master_ms` 는 0.04~0.12ms 로 무시할 수준이고, Windows 에서 이 호출이
  실제로 자식 종료를 기다리는지·얼마나 걸리는지는 Windows 실행 환경에서만 측정할
  수 있다. 측정 없이 ConPTY 수명과 자식 정리 순서를 바꾸는 것은 위험 대비 근거가
  없다. 비용이 있다면 로그에 드러나도록 계측 구간에 `pty_master` 해제를 포함시켜
  둔 상태이므로(C5b/S5b), Windows 실측이 나오면 그때 같은 방침을 적용한다.
- **`fs::remove_file`(C5a)을 백그라운드 삭제 워커로 옮긴다** — 기각: 실측 5µs/
  surface(30 개 = 0.13ms, close 전체의 0.01%). 전역 워커 스레드와 그 실패 모드를
  더하는 대가가 이득보다 크다. 삭제 실패는 이미 다음 시작의
  `scrollback_store::gc_orphans` 가 회수한다.
- **observer `join()` 을 그냥 삭제하고 아무 데서도 회수하지 않는다** — 기각: 앱
  종료 시 아직 배수 중인 워커가 프로세스와 함께 죽어 sink 의 마지막 항목이
  잘린다. 회수 지점을 종료 시퀀스에 두는 비용은 실측상 0 에 가깝다(워커들이 그
  동안 병렬로 이미 배수를 끝낸다).
- **sink 워커를 join 하는 전용 reaper 스레드를 둔다** — 기각: 워커를 기다리기 위해
  또 스레드를 만드는 구조라, `Vec<JoinHandle>` + `is_finished()` 논블로킹 회수로
  같은 결과를 스레드 0 개로 얻을 수 있다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- Windows 실측에서 C5b 가 여전히 탭 수에 비례해 크게 나온다 — `ClosePseudoConsole`
  에 같은 방침(detached 해제)을 적용할지 그때 판정한다.
- `portable-pty` 가 unix `ChildKiller::kill` 의 유예 루프를 제거하거나 시맨틱을
  바꾼다 — 직접 SIGHUP 을 보내는 우회가 불필요해지거나 어긋난다.
- 탭 수가 수백 단위로 늘어 reap 스레드 생성 자체가 병목이 된다 — 그때는 스레드
  1 개가 pid 목록을 폴링하는 형태로 합친다.
- observer sink 에 `BufWriter` 나 네트워크/FIFO 처럼 **자체 버퍼를 가진 sink** 가
  추가된다 — "채널에 수락 = 곧 기록" 전제가 깨지므로 flush 계약을 다시 정의해야
  한다.
- close 계측에서 C5d(`purge_scope`, sqlite)가 새 지배 구간으로 올라온다 — 본
  ADR 범위 밖이며 별도 판정이 필요하다(탭 30 개에서 47.6ms 관측).

## References

- [close-sequence](../architecture/close-sequence.md) — `tasty::close` 계측과 실측 기준선
- [shutdown-sequence](../architecture/shutdown-sequence.md) — S3b / S5b
- [ADR-0002](0002-vte-parsing-off-input-thread.md) — 파싱을 메인 스레드 밖으로 낸 선례
- `crates/tasty-terminal/src/lib.rs` `PtyBackend::drop` · `src/core/output_observer.rs` · `src/app/shutdown_cascade.rs`
