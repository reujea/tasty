# ADR-0267: mirror surface 의 cwd 는 서버가 push 하고, 그 경로는 원격 출처로 타입에서 구분한다

- **Status**: Accepted
- **Date**: 2026-09-13
- **Tags**: attach, mirror, remote, cwd, wire-format, provenance, newtype, inherit-cwd, file-picker, adr-0056, adr-0059, adr-0086, adr-0255
- **Group**: remote-attach

## Context

**mirror terminal 은 자기 cwd 를 스스로 모른다.** `Terminal::get_cwd`
(`crates/tasty-terminal/src/accessors.rs`)는 OSC 7 캐시(`cached_cwd`) → `process_id()` → OS 조회
(`cwd::get_cwd_of_pid` — Linux `/proc/<pid>/cwd`, macOS `proc_pidinfo`) 순으로 판정하는데, mirror
에는 로컬 PTY 가 없어 `process_id()` 가 항상 `None` 이다. 원격 셸이 OSC 7 을 방출해 원격 출력
바이트에 실려 올 때만 캐시가 채워진다. 별도 폴링 루프는 없다(`src/boot.rs` 의 "CWD는 OSC 7
시퀀스에만 의존한다").

**그러나 mirror 에 원격 절대경로가 없는 것은 아니다.** 서버가 mirror explorer 디스크립터에
원격 `root` 를 싣고(`src/core/attach_runtime.rs`), client 가 그 값으로 `ExplorerPanel` 을 만들며,
`ExplorerPanel::source_cwd()` 는 그 root 를 그대로 돌려준다. 즉 이 결정 시점에 원격 절대경로가
구분 없는 `Option<PathBuf>` 로 `AppState` 의 cwd 판정 함수(`cwd_from_surface`)를 통해 **로컬 소비자에게
이미 흘러간다** — mirror explorer 를 focus 한 채 새 워크스페이스를 만들면 로컬 PTY 가 원격 root 를
`working_dir` 로 받는다. 같은 함수의 둘째 사본(StatusBar git 브랜치 캐시 모듈의 `surface_cwd`)은
그 경로로 **로컬 디스크**를 뒤져 git 브랜치를 찾는다.

**소비자마다 우회로가 따로 났다.**

| 소비자 | 우회로 |
|---|---|
| git-viewer([ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md)) | `git_query_request` 에 원격 surface id 를 실어 서버가 cwd 를 resolve |
| Tools 메뉴 → plugin popup | popup context 에 `mirror`/`local_surface_id` 를 실어 plugin 이 원격 조회를 앵커 |
| convert forward | `StructuralOp::ConvertSurface.cwd` 에 client 사본을 싣고, 비면 서버가 `resolve_inherit_cwd_from_surface` 로 resolve |
| 파일 피커 | 우회로 없음 — mirror 면 빈 `dir` 로 보내 원격 홈에서 출발 |

고칠 대상은 소비자가 아니라 **mirror surface 가 자기 cwd 를 모른다는 사실 하나**다.

**같은 성질의 사실을 이미 두 개 push 한다.** `StreamControl::Activity`(busy) 의 doc 이 "mirror
terminals have no local PTY, so they can never compute this themselves — this push is the only
source" 라고 적고, `Attention` 도 같다. cwd 도 진실 원천이 PTY 를 소유한 인스턴스에만 있다.

**원격 경로를 로컬 fs 에 쓰지 않는다는 선례가 셋이다.** [ADR-0255](0255-markdown-attach-mirror-forwards-content-not-pixels.md)
— mirror markdown 의 `file` 은 표시 전용 opaque 문자열이고 client 는 그 값으로 자기 로컬 파일을
열지 않는다. [ADR-0059](0059-explorer-remote-attach-list-dir-reuse-browse-only.md) — mirror explorer
는 browse-only. `src/adapters/ui/terminal_link.rs` — mirror 판별 시 화면 경로의 로컬 `exists()`
검증을 건너뛴다. cwd 는 그 규칙이 아직 안 닿은 값이다.

**1Hz 조회 비용 실측(2026-09-13, Linux, 20 코어, load average 6.3)**: `std::fs::read_link("/proc/<pid>/cwd")`
을 20 만 회 연속 호출하는 벤치를 세 번 돌려 호출당 713 · 799 · 844 ns 였다. 점유 surface 100 개를
매 tick 조회해도 0.1 ms 미만이다. macOS `proc_pidinfo` 는 이 저장소 작업 환경에 macOS 가 없어
**재지 못했다**(아래 재검토 조건).

## Decision

1. **cwd 는 서버가 push 한다 — `Activity`/`Attention` 과 같은 축.** 새 wire variant
   `StreamControl::Cwd { surface_id, cwd: Option<String> }` (Direction: server→client). 서버는 1Hz
   `Tick::Busy` 마다 점유 중인 surface 의 cwd 를 **자기 트리 기준으로** 계산해(terminal 은
   `get_cwd()` — OSC 7 이 없어도 pid 폴백이 돈다, 그 외 kind 는 `source_cwd()`) 값이 바뀐 것만
   holder 에 보낸다. 배선 지점은 busy/attention 과 같은 셋(gui 포커스 window · gui parked engine
   · headless)이다.

2. **저장 위치는 `CoreState` 의 별도 맵이다** — `mirror_surface_cwd: HashMap<u32, RemoteCwd>`
   (로컬 mirror surface id → 원격 경로, 결정 3 의 newtype). `busy_surfaces`/`mirror_busy_surfaces` 분리와 같은
   형태이고, 정리는 `forget_mirror_surface_cwd` 가 busy 의 세 teardown 호출처에서 함께 돈다. cwd 는
   terminal 만의 값이 아니므로(explorer root · markdown 파일 부모) **kind 전환 전부**에서 정리한다
   — busy 의 "terminal 에서 출발한 전환만" 조건을 따르지 않는다. `Terminal::cached_cwd` 에 직접
   쓰지 않는다(Alternatives).

3. **원격 경로는 타입으로 출처가 구분된다 — newtype.** surface cwd 의 판정은
   `CoreState::surface_cwd(surface_id) -> Option<SurfaceCwd>` 하나로 모으고(두 사본을 합친다),
   반환 타입이

   ```rust
   pub enum SurfaceCwd { Local(PathBuf), Remote(RemoteCwd) }
   pub struct RemoteCwd(String); // Path 로 가는 변환(AsRef<Path>/Into<PathBuf>)을 두지 않는다
   ```

   다. mirror 워크스페이스에 속한 surface 의 값은 **출처가 무엇이든**(push 된 맵 · OSC 7 캐시 ·
   explorer root) `Remote` 로 나온다. 로컬 실행 경로가 쓰는 `AppState::resolve_inherit_cwd` /
   `resolve_inherit_cwd_from_surface` 는 계속 `Option<PathBuf>` 를 돌려주되 **`Local` 만** 통과시킨다.
   원격 값을 로컬 `PathBuf` 자리에 넣으려면 `RemoteCwd::as_str()` 을 꺼내 손으로 감싸야 하므로
   그 유입은 코드에 드러난다. 판정 규칙:
   - **로컬에서 실행되는 생성**(새 워크스페이스의 첫 PTY, 로컬 워크스페이스 안의 tab/split/convert,
     preset 에 저장되는 terminal cwd, StatusBar 의 로컬 git 브랜치 조회)은 `Local` 만 쓴다.
   - **mirror 안의 구조 변경**은 원격에서 실행되지만 client 사본을 싣지 않는다 — 서버가 자기
     PTY 에서 resolve 하는 값이 진실 원천이고 push 된 값은 그 사본이기 때문이다. 명시 cwd 가
     있는 경우만 op 에 실린다.
   - **plugin 에 넘기는 JSON 경계**에서는 원격 경로를 `cwd` 키에 싣지 않는다. 원격 값은 별도 키
     (`remote_cwd`)로만 나가고 `mirror: true` 와 함께 간다 — 이 구분을 모르는 plugin 이 원격
     경로를 로컬 경로로 쓰지 못하게 하는, 타입이 없는 경계에서의 같은 규칙이다.

4. **갱신 캐던스는 1Hz diff push 다.** OSC 7 edge 만으로는 OSC 7 을 안 쏘는 셸이 커버되지 않고,
   Linux 실측 비용(Context)이 1Hz 조회를 정당화한다. 값이 안 바뀌면 프레임이 나가지 않는다.
   diff 캐시는 **(holder, 값)** 을 함께 기억한다 — surface id 만 키로 잡으면 같은 tick 창 안에서
   holder 만 바뀌었을 때 새 holder 가 초기값을 못 받는다.

5. **`inherit_cwd` 설정은 push 를 게이트하지 않는다.** 서버는 raw cwd 를 보내고, 게이트는 소비
   시점(client)이 건다. `inherit_cwd` 는 "새 surface 가 cwd 를 상속하는가" 이지 "cwd 를 아는가" 가
   아니다. [surface-cwd §3-1](../architecture/invariants/surface-cwd.md) 의 "원격 인스턴스의
   `inherit_cwd` 게이트를 그대로 적용" 은 **실행 경로(서버측 resolve)에 한정**된다. 같은 이유로
   "지금 어느 폴더를 보고 있나" 를 알려 주는 소비자(파일 피커 시작 위치)는 설정과 무관하게 cwd 를
   받는다 — popup context 의 게이트 안 걸린 필드는 `cwd` 와 별개 키로 싣는다.

6. **wire 호환.** 구버전 client 는 모르는 `StreamControl` variant 를 파싱 실패로 무시한다. 구버전
   server 는 push 를 안 보내므로 맵이 비고, 서버측 resolve 폴백이 그대로 동작한다 — 이 채널은
   폴백을 **대체하지 않고 덮는다**(폴백 삭제는 범위 밖). **값이 사라지는 edge 는 `cwd: null` 로
   표현한다** — 원격이 cwd 미상이 되면 client 는 맵 엔트리를 지운다. 표현이 없으면 stale 원격
   경로가 mirror 에 영구히 남는다.

## Consequences

- **얻은 것**: mirror terminal 이 OSC 7 없이도 cwd 를 갖는다. 파일 피커 같은 소비자가 우회로 없이
  원격 cwd 에서 출발할 수 있다. 원격 경로가 로컬 PTY `working_dir` · 로컬 git 조회 · preset 으로
  새는 기존 유출(mirror explorer root)이 타입 경계에서 막힌다. `cwd_from_surface` 의 두 사본이
  하나로 합쳐져 한쪽만 고쳐지는 형태가 사라진다.
- **잃은 것**: mirror 에 **원격 절대경로가 상주하기 시작한다** — 이전에는 OSC 7 · explorer 에서만
  드물게 생기던 값이 상시 존재한다. 그 위험을 다루는 것이 결정 3 의 newtype 이다. mirror convert
  forward 가 client 사본 cwd 를 더는 싣지 않으므로, 서버 자신의 `inherit_cwd` 가 꺼져 있으면
  원격 convert 는 cwd 없이 만들어진다(실행 주체의 설정을 따르는 §3-1 의 원래 의미와 같다).
  mirror surface 에서 StatusBar git 브랜치가 표시되지 않는다(원격 경로로 로컬 디스크를 뒤지던
  잘못된 값이 사라진 것이다).
- **운영 비용 / 유지 부담**: wire variant 1 · 서버 diff 함수 1 · tick 배선 3 지점 · client teardown
  3 지점. `surface_cwd` 를 거치지 않고 `Terminal::get_cwd()` 를 직접 읽는 새 소비자는 이 구분을
  우회한다 — 그 형태는 리뷰로 잡아야 한다.

## Alternatives Considered

- **(a) push 값을 mirror terminal 의 `Terminal::set_cached_cwd` 로 직접 넣는다** — 소비자를 하나도
  안 고쳐도 `cwd_from_surface` 가 곧바로 참이 되지만, 로컬 경로와 원격 경로가 같은 필드에 섞여
  **구분할 수단이 사라진다**. 결정 3 이 막으려는 무구분 유입을 채널이 스스로 만든다. 또한
  비-terminal kind(explorer · markdown)는 담을 곳이 없다. `set_cached_cwd` 는 이 결정 시점에
  프로덕션 호출자가 없으며(테스트만 쓴다) 이 안을 고르지 않았으므로 그대로 남는다.
- **소비자별 질의 왕복**(git-viewer 방식을 파일 피커 등에 복제) — 같은 질문이 소비자 수만큼 늘고,
  각 소비자가 비동기 회신을 따로 기다려야 한다.
- **OSC 7 edge push 만** — OSC 7 을 안 쏘는 셸은 값이 영영 없다.
- **출처 구분을 호출 규약(주석·테스트)으로만** — 규약은 새 호출자에게 전달되지 않는다. newtype 은
  원격 값을 `PathBuf` 자리에 넣는 순간을 코드에 드러낸다.
- **별도 필드(`Option<PathBuf>` + `is_remote: bool`)** — bool 을 무시한 채 경로만 쓰는 호출이
  컴파일된다.
- **`inherit_cwd` 를 push 에도 적용** — 설정을 끈 사용자에게서 "원격 인지" 정보까지 빼앗는다.
  `src/adapters/ui/tools_menu.rs` 가 같은 이유로 mirror 판정을 설정과 무관하게 한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것**

- 원격 cwd 소비자가 **로컬 fs 를 건드려야 하는** 요구가 생긴다 — `RemoteCwd` 에서 `Path` 로 가는
  변환이 추가되는 순간이다. `rg 'impl .*AsRef<.*Path.*for RemoteCwd|From<RemoteCwd>'` 로 읽힌다.
- 서버측 resolve 폴백(`execute_forwarded_structural_op` 의 `resolve_inherit_cwd_from_surface`)이
  삭제된다 — 그때 결정 6 의 "덮는다" 가 "대체한다" 로 바뀌므로 구버전 server 호환을 다시 판정한다.

**원리적으로 안 붙는 것**

- 1Hz cwd 조회가 서버 tick 비용에서 문제가 된다. 특히 macOS `proc_pidinfo` 는 재지 않았다. 재는 법:
  Context 의 벤치(`read_link` 연속 호출)를 macOS 에서는 `cwd::get_cwd_of_pid` 연속 호출로 바꿔
  같은 조건으로 돌리고, 점유 surface 수 × 호출당 시간이 tick 예산의 의미 있는 비율인지 본다.

## References

- [docs/architecture/invariants/surface-cwd.md](../architecture/invariants/surface-cwd.md) — §1 표, §3-1 mirror forward
- [docs/dev-guide/attach-behavior.md](../dev-guide/attach-behavior.md) — mirror push 채널 목록
- [docs/features/remote-attach/index.md](../features/remote-attach/index.md)
- [ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md) — 원격 surface cwd 를 서버가 resolve 하는 첫 선례
- [ADR-0059](0059-explorer-remote-attach-list-dir-reuse-browse-only.md) — browse-only 선례
- [ADR-0086](0086-reject-terminal-spawn-into-mirror-workspace.md) — mirror 안 로컬 실행 거부(이미 닫힌 유출구)
- [ADR-0139](0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md) — Context 의 수치는 측정 시점 값이다
- [ADR-0255](0255-markdown-attach-mirror-forwards-content-not-pixels.md) — 원격 문자열을 opaque 로 둔 선례
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-ipc/src/stream.rs` 의 `StreamControl::Cwd` ·
  `src/core/state/surface_cwd.rs` 의 `SurfaceCwd`/`RemoteCwd`/`CoreState::surface_cwd`/`surface_cwd_forwards` ·
  `src/core/attach_runtime.rs` 의 `forward_surface_cwd` · `src/app/attach_client.rs` 의 `MirrorEvent::Cwd`
