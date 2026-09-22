# ADR-0094: surface id 공간은 `PTY_ID_BASE` 미만으로 강제한다 — 경계에서 거부하고 부팅 시 침범분을 정리한다

- **Status**: Accepted
- **Date**: 2026-09-02
- **Tags**: surface-id, headless-pty, id-space, memory-db, boot, ipc, validation, invariant
- **Group**: window-workspace-lifecycle

## Context

Tasty 의 u32 id 키스페이스는 둘로 갈라져 있다. surface id 는 1 부터 증가하고, headless PTY id 는 `PTY_ID_BASE = 0x8000_0000`(2^31) 부터 발급한다. headless `Terminal` 은 surface 와 **같은** `TerminalStore` 에 자기 pty id 를 키로 등록하므로(ADR-0050), 두 공간이 겹치면 서로의 `Terminal` 을 조용히 덮어쓴다.

이 disjoint 는 오랫동안 "surface id 가 2^31 까지 자랄 일은 없다" 는 가정에만 기대고 있었고, 그 가정은 실환경에서 이미 깨져 있었다 — 실사용 인스턴스의 live surface id 가 전부 `2147484147`(= 2^31 + 499) 이상이었다. 붕괴 구조는 두 단계다.

1. **유입** — `Scope::Surface(id)` 를 쓰는 경로 어디에도 범위 검증이 없었다. 어느 경로가 실제로 최초 오염을 만들었는지는 **단정하지 않는다** — 남아 있는 memory.db 는 이미 여러 차례의 부팅 purge 를 거쳤으므로 최초 쓰기 시점을 복원할 수 없다. 다만 열려 있던 경로는 다음 둘이다.
   - **호스트 내부 경로 — 코드 경로상 가장 유력하다**: OSC 133 명령 인덱싱(`command_index`)이 `TerminalStore` 키를 그대로 surface id 로 받아 `Scope::Surface(key)` 에 `tasty.commands.*` 를 쓴다. headless PTY 의 `Terminal` 은 그 store 에 **pty id** 로 등록돼 있고, `pty.spawn` 은 surface 터미널과 동일한 `ShellConfig::from_settings` 로 셸을 띄워 OSC 133 자동 주입까지 그대로 받는다. 따라서 headless PTY 안에서 명령이 한 번 끝나면 `Scope::Surface(2^31+)` 가 생긴다. 이 경로는 코드로 성립이 확인되고 격리 인스턴스에서 재현되지만, **실사용 인스턴스의 오염 scope 가 실제로 이 경로에서 나왔는지는 실환경 관측으로 확증해야 한다**(오염 scope 의 키가 `tasty.commands.*` 뿐인지 확인).
   - **IPC 경계**: `require_surface_id` 는 `as_u64()` 뒤 무검사 `as u32` 캐스팅만 했고(2^32 이상은 조용히 wrap), `memory.*` 의 `scope` 파라미터는 `surface:<임의 u32>` 를 그대로 받았다. `surface.meta.set` / `memory.put` 에 PTY 공간 id 를 한 번만 넘겨도 같은 오염이 생긴다.
2. **고착(비가역 래칫)** — 복원 직전 surface 카운터 floor 를 `max(memory.db 의 Scope::Surface id) + 1` 로 올리는 로직이 있다. 이 로직 자체는 정당하다(surface id 는 매 실행 1 부터 재발급되지만 surface_meta 는 영속되므로, 올리지 않으면 새 surface 가 이전 실행의 stale 메타를 물려받는다). 문제는 오염된 scope 하나가 floor 를 PTY 공간으로 밀어 올리면, 그 실행이 발급한 surface 들이 다시 memory.db 에 기록되어 다음 부팅의 floor 를 유지한다는 점이다. 외부 개입 없이는 영원히 내려오지 않는다.

기존 회귀 테스트는 이걸 잡지 못했다. `ids_are_disjoint_from_surface_space` 와 `next_surface_starts_at_one` 은 둘 다 *갓 만든* 카운터를 쓰므로 래칫이 걸린 상태를 재현하지 않는다.

## Decision

`PTY_ID_BASE` 를 **강제되는 경계**로 승격한다. surface id 공간은 `[1, PTY_ID_BASE)` 이며, 이 불변식을 세 지점에서 집행한다.

1. **호스트 내부 쓰기에서 제외** — `command_index::on_boundary` 는 surface 공간 밖의 id 로 들어온 prompt boundary 를 인덱싱하지 않는다. headless PTY 는 Surface 가 없으므로 surface 스코프 명령 인덱스의 소비자도 없다.
2. **입력 경계에서 거부** — surface id 를 받는 모든 IPC 경계(`require_surface_id` 2 곳, `memory.*` 의 `scope=surface:<id>` 파싱)가 `PTY_ID_BASE` 이상을 `invalid_params` 로 거부한다. 무검사 `as u32` 캐스팅은 `u32::try_from` 으로 교체해 2^32 이상 값이 wrap 되지 않게 한다.
3. **부팅 시 floor 시딩에서 제외 + 침범분 purge** — floor 시딩(`impl_workspace::seed_surface_id_floor`, `src/core/impl_workspace.rs`)은 PTY 공간을 침범한 `Scope::Surface` 를 최대값 산정에서 제외하고, 같은 자리에서 `tracing::error!` 로 기록한 뒤 purge 한다.

세 지점 모두 `pty_registry::is_surface_id_space` 술어 하나를 공유해 경계 해석이 갈리지 않게 한다.

### 이미 오염된 인스턴스는 마이그레이션한다 (사용자 확정)

방치(오염 scope 를 그대로 두고 유입만 막는 안)와 마이그레이션(오염 scope 를 삭제하고 카운터를 정상 범위로 되돌리는 안) 중 **마이그레이션을 채택한다.** 코드 수정만으로는 이미 래칫이 걸린 인스턴스가 정상 범위로 돌아오지 않기 때문이다.

마이그레이션은 위 3 번 방어가 겸한다 — 오염된 scope 가 floor 를 밀어 올리지 못하므로 다음 부팅에서 surface id 가 정상 범위로 복귀하고, 그 scope 자체도 같은 자리에서 제거된다. **별도 마이그레이션 루틴은 두지 않는다.**

purge 로 인한 메타 손실은 없다고 본다. 부팅 시점의 `Scope::Surface` 는 정의상 전부 이전 실행의 잔재이고, 곧이어 도는 `purge_dead_surfaces` 가 live 아닌 scope 를 어차피 지운다. 재시작을 건너 살아남아야 하는 값(`restore.command`)은 capture 시점에 layout 슬롯 파일로 옮겨져 있고, `claude-session-id` 는 세션마다 hook 이 다시 쓴다.

**적용 조건**: 이 자가 치유는 floor 시딩이 도는 실행, 즉 **복원이 실제로 수행되는 부팅**에 한정된다. 복원 비활성 설정이나 headless engine 처럼 `pending_layout_restore` 가 없는 실행에서는 시딩·purge 가 돌지 않아 오염 scope 가 남는다. 그런 실행의 카운터는 1 부터 시작하므로 래칫 자체는 재발하지 않는다.

## Consequences

- **얻은 것**: disjoint 가 가정이 아니라 집행되는 불변식이 된다. 열려 있던 유입 경로(호스트 내부 명령 인덱싱 · IPC)가 모두 닫히고, 이미 오염된 인스턴스는 부팅 한 번으로 자가 치유된다. 경계 술어가 한 곳이라 새 진입점이 생겨도 같은 판정을 재사용한다.
- **잃은 것**: PTY 공간의 id 로 surface 메타/스코프를 쓰던 호출자는 이제 에러를 받는다. 정상 호출에는 그런 경로가 없으므로 실질 호환성 손실은 없다고 본다. headless PTY 안에서 끝난 명령은 `tasty.commands.*` 로 기록되지 않는다 — 조회 표면이 surface 스코프뿐이라 원래 읽을 수 없던 기록이다(exit-code 는 `pty.wait` 가 별도로 제공한다). 자가 치유 과정에서 오염 scope 에 남아 있던 값은 사라진다(위 근거대로 손실 없는 값으로 본다).
- **운영 비용 / 유지 부담**: 부팅마다 `scopes()` 를 한 번 더 훑는다 — floor 시딩이 이미 같은 목록을 읽으므로 추가 비용은 무시할 수준이다. surface id 를 받는 새 진입점(IPC 파라미터든 호스트 내부 `TerminalStore` 키 소비든)은 `is_surface_id_space` 를 통과시켜야 한다는 규약이 하나 생긴다.

## Alternatives Considered

- **(a) 방치 — 유입만 막고 이미 오염된 인스턴스는 그대로 둔다**: u32 헤드룸이 21 억 남아 실사용상 고갈되지 않는다는 논거. 채택하지 않았다 — 문제는 헤드룸이 아니라 **거리**다. 오염된 인스턴스는 PTY 카운터 시작점(`2^31`)과 surface 시작점(`2^31 + 499`) 사이가 500 밖에 안 돼, 장수명 세션에서 headless PTY 를 500 회 spawn 하면 실제로 같은 키에서 만난다. 증상이 `Terminal` 상호 덮어쓰기라 조용한 오동작이 되어 진단이 어렵다. 게다가 방치를 택하면 오염 인스턴스에서 surface 계열 IPC(입력 방어에 걸린다)가 전부 막히므로, 방치와 입력 방어는 애초에 양립하지 않는다.
- **(b′) 손실 없는 재키잉 마이그레이션**: 부팅 시 오염을 감지하면 별도 루틴으로 오염 scope 를 정상 id 로 rekey 해 메타를 보존하는 안. 채택하지 않았다 — 보존할 값이 실제로 없다(위 Decision 의 손실 분석). 재키잉은 보존 가치 없는 데이터를 위해 부팅 경로에 상태 변환을 하나 더 추가한다.
- **(c) PTY id 공간 재설계(별도 타입/네임스페이스)**: `TerminalStore` 키를 태그 유니온으로 바꿔 두 공간이 애초에 만나지 않게 하는 안. 채택하지 않았다 — 범위가 store/waker/attach 승격 경로 전반에 걸친다. 경계 집행만으로 불변식이 실제로 성립하므로 지금 지불할 비용이 아니다. 다만 아래 재검토 트리거가 걸리면 이 안이 1 순위 후보다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- surface id 나 PTY id 가 u32 헤드룸을 실제로 압박하는 사용 패턴이 생긴다(예: 단일 세션에서 수억 개 발급).
- `TerminalStore` 외의 제 3 의 id 공간이 같은 u32 키스페이스에 합류해야 한다 — 경계 상수 하나로 가르는 방식이 더 이상 확장되지 않는 시점. 이때는 (c) 재설계를 다시 검토한다.
- 재시작을 건너 보존돼야 하는 surface meta 키가 새로 생겨, 부팅 시 purge 가 실제 손실이 되는 경우 — 그때는 (b′) 재키잉을 다시 검토한다.
- 복원이 돌지 않는 실행이 상시화되어(복원 비활성이 기본이 되는 등) "부팅 한 번으로 자가 치유" 전제가 성립하지 않게 되는 경우 — floor 시딩과 무관한 별도 마이그레이션 지점이 필요해진다.
- 새 빌드로 재시작한 실사용 인스턴스에서 `sqlite3 ~/.tasty/memory.db "select scope from memory where scope like 'surface:%'"` 결과에 여전히 2^31 이상 id 가 남는 경우 — 여기서 다루지 않은 유입 경로가 있다는 뜻이다.

## References

- [`0050-headless-pty-primitive.md`](0050-headless-pty-primitive.md) — pty id 를 disjoint 고범위에서 발급하고 `TerminalStore` 를 공유한다는 원 결정.
- [`../features/headless-pty/index.md`](../features/headless-pty/index.md) — headless PTY 기능 문서(두 store 정합).
- [`../concepts/ubiquitous-language.md`](../concepts/ubiquitous-language.md) — Surface / PTY 용어.
