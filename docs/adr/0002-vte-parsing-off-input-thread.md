# ADR-0002: VTE 파싱을 입력(winit) 스레드 밖의 per-terminal 파서 스레드로 분리

- **Status**: Accepted
- **Date**: 2026-06-15
- **Tags**: performance, terminal, threading, input-latency, vte
- **Group**: terminal-input

## Context

winit 이벤트 루프는 단일 스레드다. 키 입력(`WindowEvent::KeyboardInput`)과 PTY 출력
wake(`AppEvent::TerminalOutput`)가 같은 메인 스레드에서 직렬 처리된다.

기존 구조에서 PTY reader 스레드는 raw 8KB 청크만 채널로 보내고, 실제 VTE 파싱
(`parser.parse_as_vec` + `action_to_changes` + grid 갱신)은 메인 스레드의
`Terminal::process()` 에서 일어났다. `process()` 는 한 호출에 버퍼된 모든 청크를
drain 하므로 최대 256KB(sync_channel(32)×8KB)의 escape sequence 를 메인 스레드에서
동기 파싱했다. 따라서 백그라운드 surface(안 보이는 Claude 등) 개수에 비례해
포그라운드 키 입력 반응이 선형으로 느려졌다.

측정(별도 격리 인스턴스, IPC 에코 왕복 지연 중앙값): flood 0/4/8/16 개에서
208 → 313 → 328 → 389ms 로 단조 증가 — 메인 스레드 직렬화가 입증됨.

## Decision

**접근법 (A): reader 스레드에서 파싱.** 각 terminal 의 reader 스레드를 *파서 스레드* 로
승격해, PTY raw 바이트를 읽는 즉시 그 스레드에서 ingest(파싱+grid 갱신)를 수행한다.
메인 스레드는 더 이상 파싱하지 않는다.

공유 상태 동기화는 **per-terminal `Arc<Mutex<TerminalState>>` + 청크 단위 락 해제** 로
처리한다. `Terminal` 은 PTY I/O 핸들만 보유하는 thin handle 이 되고, VTE 상태
(surface grid, parser, modes, scrollback, output buffer, events)는 `TerminalState`
로 옮겨 락 뒤에 둔다. 파서 스레드는 8KB 청크마다 락을 잡아 ingest 하고 즉시 해제하므로,
메인 스레드가 렌더/IPC/이벤트 수집을 위해 같은 terminal 을 락하려 할 때 최대 1 청크
파싱 시간(수십 µs)만 대기한다. 안 보이는 flood terminal 은 렌더되지 않으므로 그 락은
메인 스레드와 거의 경합하지 않는다 — 포그라운드 probe 의 락은 비경합 상태로 유지된다.

## Consequences

- **얻은 것**: 백그라운드 terminal 수가 늘어도 메인 스레드의 파싱 부하가 사라져 포그라운드
  입력/IPC 응답이 평탄해진다. 파싱이 멀티코어로 분산된다(터미널당 1 스레드). 스크롤백
  디스크 flush 등 부수 I/O 도 메인 스레드에서 빠진다.
- **잃은 것**: `Terminal` 의 외부 API 중 `surface() -> &Surface` 처럼 grid 참조를 직접
  반환하던 것은 락 가드 밖으로 참조를 빼낼 수 없어 `with_surface(|s| …)` / owned 접근자
  (`dimensions`/`cursor_position`/`screen_lines`)로 대체했다. 렌더 경로는 terminal 당 락을
  한 번 잡는다.
- **운영 비용 / 유지 부담**: terminal 당 스레드 1 개(기존 reader 스레드를 대체 — 순증
  없음). `TerminalState` 접근은 항상 락을 거치므로, 새 grid 접근 API 추가 시 handle 에
  delegating wrapper 를 둬야 한다. Mutex poisoning 은 `into_inner()` 로 복구한다.

## Alternatives Considered

- **(B) surface 별 워커 풀**: 파싱 전용 스레드 풀에서 surface 를 스케줄. terminal-스레드
  매핑/스케줄링 복잡도가 늘고 per-terminal 락 격리의 단순함(probe 락 비경합)을 잃는다.
  terminal 수가 수백을 넘는 상황이 아니므로 풀의 이점이 없다.
- **메인 스레드 협력적 yield(청크 budget + re-wake)**: 파싱을 메인 스레드에 둔 채 청크
  단위로 잘라 이벤트 루프에 양보. 구현은 작지만 파싱이 여전히 입력 스레드 CPU 를 쓰므로
  부하 포화 시 입력 지연이 terminal 수에 비례해 남는다. "입력 스레드에서 분리" 라는 목표를
  충족하지 못한다(P4 throttle 의 영역에 가까움).
- **ArcSwap 스냅샷 + actor 커맨드**: 파서 스레드가 grid 를 단독 소유하고 렌더 스냅샷만
  publish. `read_since_mark`/`screen_text`/`cell_info` 등 다수의 동기 read API 가
  request/response 로 바뀌어 busy 파서 스레드에 블록된다 — 측정 경로(`read_since_mark`)와
  이벤트 수집이 다시 직렬화되어 본말전도.

## Reconsideration Triggers

- terminal 당 스레드 수가 자원 문제(수백~수천 terminal)가 되면 (B) 워커 풀로 전환.
- per-chunk 락 경합이 프로파일에서 유의미해지면 grid 더블버퍼(스냅샷 교체)로 read 경로
  분리 재검토.
- termwiz 파서가 내부적으로 스레드 안전/병렬 파싱을 제공하게 되면 구조 단순화 재검토.

## References

- `docs/dev-guide/build.md`, `docs/concepts/hierarchy.md` (Surface 계층)
- 영향 파일: `crates/tasty-terminal/src/{lib,io,resize,accessors}.rs`,
  `src/gfx/renderer.rs`, selection/IME/mouse/IPC 의 `surface()` 접근자
