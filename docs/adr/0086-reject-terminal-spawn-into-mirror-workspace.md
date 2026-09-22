# ADR-0086: mirror 워크스페이스로의 `terminal.spawn` 은 거부하고, 대상 판정을 최종 pane 으로 옮긴다

- **Status**: Accepted
- **Date**: 2026-08-25
- **Tags**: attach, mirror, terminal-spawn, structural-forward, orphan-resource, ipc, adr-0060
- **Group**: remote-attach

## Context

mirror(원격 attach client) 워크스페이스를 대상으로 `tasty claude spawn` / `tasty codex spawn` /
`tasty terminal spawn` 을 부르면 내부 필드 이름을 그대로 노출하는 오류가 나왔다.

```
tab.create response missing 'surface_id': {"forwarded":true,"workspace_index":2}
```

그리고 **실패했는데 부작용이 남았다** — 같은 호출이 원격 인스턴스에 `childN` 이름의 탭을 실제로
하나 만들어 놓는다. child registry 등록도, soft 점유 등록도 되지 않은 고아다.

### 왜 이렇게 되는가

mirror 워크스페이스 안의 구조 변경은 로컬에서 실행하지 않고 원격으로 forward 하는 것이 정상
설계다("workspace 전체가 remote" 불변식). 그 forward 는 **fire-and-forget** 이라, 로컬 호출자가
받는 응답은 `{"forwarded":true,"workspace_index":N}` 이고 **생성된 id 를 담지 않는다** — 응답
시점에는 원격이 아직 아무것도 만들지 않았기 때문이다. 결과는 나중에 `StructuralDelta` 역반영으로
mirror 트리에 들어온다.

`terminal.spawn` 만 이 모델 위에 얹힐 수 없다. `handle_spawn` 은 내부적으로 `tab.create` 핸들러를
직접 호출한 뒤 그 응답에서 새 surface id 를 **동기로** 꺼내, child registry 등록 → soft 점유 →
(선택) 초기 command 주입까지 이어가야 한다. 그래서 forward 응답을 받으면 id 를 찾지 못하고
`internal_error` 로 떨어진다.

부작용은 그 에러와 **무관하게** 진행된다. 구조 op 는 차단 시점에 이미 forward 큐
(`pending_structural_forward`)에 들어가 있고, 그 큐는 IPC 응답이 아니라 이벤트 루프가 드레인해
원격으로 보낸다. 탭 이름(`childN`)도 그 전에 확정된다. 즉 로컬은 실패를 반환하고 원격에는 탭이
생기는, 호출자가 인지하지 못하는 리소스가 남는다.

### 같은 증상을 가진 다른 method 는 없다

`{forwarded:true}` success 를 받는 구조 op 는 여럿이지만(`tab.create`/`split`/`tab.close`/
`tab.move`/`pane.close`/`surface.close`/`image.open` 등), 그 응답에서 **생성된 id 를 다시 꺼내
쓰는 호출자는 `terminal.spawn` 뿐**이다. 나머지는 응답을 그대로 호출자에게 돌려주는
fire-and-forget 이라 설계대로 동작한다. 따라서 "내부 필드명 오류 + 원격 고아 생성" 조합은 이 한
경로에 국한된다.

### 판정 기준이 파라미터와 어긋나 있다

`terminal.spawn` 의 실제 대상은 `workspace` 파라미터가 아니다. `pane` 오버라이드가 주어지면 탭은
그 pane 에 생기고, 그 pane 은 다른 워크스페이스에 속할 수 있다 — `handle_spawn` 은 pane 의
workspace 소속을 검증하지 않는다. 반면 라우터 가드(`hard_occupied_structural_guard`)는 `workspace`
문자열만 resolve 할 수 있다.

이 어긋남은 mirror 만의 문제가 아니다. **[ADR-0060](0060-block-terminal-spawn-into-hard-occupied-workspace.md)
이 세운 hard-occupied 차단도 같은 구멍으로 뚫린다**: `--workspace <비점유 ws>` +
`--pane <hard-occupied ws 의 pane>` 이면 가드는 무해한 워크스페이스를 보고 통과시키고, 탭은 점유된
워크스페이스에 생긴다. `handle_spawn` 이 `tab.create` 를 **함수로 직접 호출**하므로 라우터를 두 번째로
타지도 않는다. 응답이 돌려주는 `workspace_id` 도 같은 이유로 실제 생성 위치와 어긋나 있었다.

## Decision

**mirror 워크스페이스를 대상으로 한 `terminal.spawn` 은 tab/surface 를 전혀 만들지 않고
`invalid_params` 로 즉시 거부한다.** 에러 메시지는 mirror 워크스페이스라는 사유와 대안(다른
워크스페이스 사용, 또는 원격 인스턴스에서 직접 spawn)을 담고, 내부 필드 이름을 노출하지 않는다.
이는 ADR-0060 이 서버(hard-occupied) 축에서 내린 결정("생성 전 거부 + 사유 명시")을 client(mirror)
축에 그대로 적용한 것이다.

**그리고 `terminal.spawn` 의 대상 판정을 `workspace` 파라미터에서 최종 확정된 `pane_id` 로 옮긴다.**
집행 지점은 라우터 가드가 아니라 `handle_spawn` 안(`spawn_target_guard`, pane 확정 직후 ·
`tab.create` 호출 전)이다. 한 번의 "이 pane 이 어느 워크스페이스에 속하는가" 조회로 mirror 와
hard-occupied 를 함께 판정하므로, 위에 적은 `--pane` 우회가 **두 축 모두** 같은 자리에서 닫힌다.
응답의 `workspace_id` 도 같은 기준으로 맞춘다.

두 가지를 함께 못박는다.

1. **mirror 판정은 `terminal.spawn` 에만 적용한다.** 라우터 가드가 다루는 나머지 8종
   (`split`/`tab.create`/`tab.close`/`tab.move`/`pane.close`/`surface.close`/`markdown.navigate`/
   `image.open`)의 mirror forward 는 정상 설계이므로 라우터 가드에는 mirror 판정을 넣지 않는다.
2. **핸들러 내부 집행이 forward 회귀를 만들지 않는다.** 라우터 가드가 "핸들러 내부에 두면 안 된다"
   고 못박은 이유는 holder 의 forward 실행 경로(`execute_forwarded_structural_op`)가 그 핸들러들을
   직접 함수 호출하기 때문인데, `handle_spawn` 은 그 재사용 목록(6종)에 **포함되지 않는다**
   (ADR-0060 이 확인한 사실). `terminal.spawn` 에 한해서만 핸들러 내부가 안전한 집행 지점이다.
   (현재 형태: forward 실행은 이제 핸들러를 부르지 않고 IPC 핸들러와 같은 도메인 실행 함수
   `core::structural_exec` 를 부른다 —
   [ADR-0395](0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md). 그래서 이
   근거는 "핸들러 안" 이 아니라 "도메인 실행 함수 안이나 `Core::apply`" 에 가드를 두면 안 된다는
   말로 읽는다. 라우터 가드 위치는 그대로 맞다.)

ADR-0060 의 결정 자체는 유지된다 — 정책(hard-occupied 워크스페이스로의 spawn 차단)은 그대로이고,
집행 지점만 더 정확한 곳으로 옮겨 그 정책이 실제로 지켜지도록 만든다.

## Consequences

- **얻은 것**:
  - 실패 응답과 실제 부작용이 어긋나던 상태가 사라진다. 거부된 spawn 은 forward 큐에 아무것도
    쌓지 않으므로 **원격에 고아 탭이 생기지 않는다.**
  - 오류가 원인을 말한다. mirror 여부 자체는 `workspace.list`/`tasty list workspaces` 로 사전
    판별할 수 있지만(`mirror` 필드 · `[mirror]` 행 마커), **사전 판별은 선택이고 거부는 필수다** —
    묻지 않은 호출자가 원격에 고아를 남기는 것이 이 결정이 막는 것이고, 그 호출자에게는 거부
    문구가 유일한 안내다.
  - ADR-0060 의 `--pane` 우회가 함께 닫힌다. 그 정책은 지금까지 `workspace` 파라미터를 정직하게
    쓰는 호출자에게만 적용되고 있었다.
  - 응답의 `workspace_id` 가 실제 생성 위치와 일치한다.
- **잃은 것**:
  - mirror 워크스페이스에 자식 터미널을 만드는 경로가 없어진다. 대안은 다른 워크스페이스를 쓰거나
    원격 인스턴스에서 직접 spawn 하는 것이다. 원격에 자식을 만들고 그 핸들을 로컬에서 받는 기능은
    forward 가 생성 id 를 회신하지 않는 한 성립하지 않는다.
  - 거부는 `terminal.spawn` 이라는 method 단위라, 미래에 "id 를 동기로 요구하지 않는" spawn 변종이
    생기면 그 변종도 함께 막힌다.
- **운영 비용 / 유지 부담**:
  - 같은 정책을 집행하는 지점이 둘로 나뉜다 — 8종은 라우터 가드, `terminal.spawn` 은
    `spawn_target_guard`. 거부 문구는 `hard_occupied_denial` 하나를 공유해 문구 drift 는 막았지만,
    "hard-occupied 차단 대상"을 셀 때 두 곳을 봐야 한다는 사실은 남는다.
  - `resolve_workspace_id` 는 라우터가 더 이상 쓰지 않아 `terminal.rs` 내부로 가시성이 좁아졌다.

## Alternatives Considered

- **A: 응답의 `forwarded` 를 보고 에러 메시지만 갈아끼운다** — 원 제보가 지목한 위치
  (`handle_spawn` 의 `surface_id` 추출 실패 지점). 안 고른 이유: 메시지는 고치지만 **원격 고아 탭은
  그대로 남는다.** ADR-0060 이 같은 형태("에러 메시지 개선만 하는 안")를 이미 기각했고, 그 기각
  근거("쓸 수 없는 것을 만들어주는 동작 자체가 문제")가 여기서는 한층 강하다 — 여기선 만들어진
  것이 로컬도 아닌 **원격**에 남아 호출자가 회수할 수단조차 없다.
- **B: 라우터 가드(`hard_occupied_structural_guard`)에 mirror 판정을 더한다** — 가드에 이미
  `terminal.spawn` 케이스가 있어 변경이 가장 작다. 안 고른 이유: 라우터는 `workspace` 파라미터만
  resolve 할 수 있어 `--pane` 우회가 그대로 남고, 그 우회는 mirror 뿐 아니라 ADR-0060 의
  hard-occupied 차단에도 이미 뚫려 있던 구멍이다. 같은 한 지점으로 둘 다 닫을 수 있는데 얕은 쪽을
  고를 이유가 없다.
- **C: `terminal.spawn` 을 forward 대상에 추가하고 원격이 생성 id 를 회신하게 한다** — mirror 에서도
  자식 spawn 이 되게 만드는 안. 안 고른 이유: forward 는 fire-and-forget 요청/응답이고, 생성된 id 를
  동기로 회신하려면 op 별 응답 페이로드와 대기 모델을 새로 정의해야 한다(현재는 `ok/reason` 뿐).
  게다가 그렇게 받은 id 는 **원격 surface id** 라, child registry·soft 점유·`surface.send` 가 모두
  로컬 id 를 전제로 하는 지금 구조와 맞지 않는다. 범위가 이 결함의 크기에 비해 지나치게 크다.
- **D: forward 큐 push 를 IPC 응답 성공 시점까지 미룬다** — 고아의 직접 원인(응답과 큐의 비동조)을
  없애는 안. 안 고른 이유: fire-and-forget 은 forward 설계의 전제이지 버그가 아니다. 응답에 큐를
  묶으면 mirror 구조 변경 전체가 동기 왕복으로 바뀐다. 문제는 큐가 아니라 "동기 id 를 요구하는
  호출자가 그 위에 얹혀 있는 것" 이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **forward 응답이 생성된 리소스의 id 를 회신하게 된다** — 대안 C 의 전제가 성립하므로, 거부 대신
  mirror 에서도 spawn 을 지원하는 쪽을 다시 검토한다.
- **사전 판별이 강제된다**(예: spawn 호출자가 mirror 여부를 반드시 확인하도록 만드는 상위 계약이
  생긴다) — 지금은 `workspace.list` 로 확인할 수 **있을** 뿐 확인하지 않아도 호출이 성립하므로
  호스트측 거부가 필요하다. 확인이 강제되면 그 필요가 약해진다(고아 방지라는 본질은 남는다).
- **id 를 동기로 요구하지 않는 spawn 변종이 생긴다** — method 단위 거부가 과하게 넓어지므로, 판정을
  method 이름이 아니라 "동기 id 를 요구하는가" 라는 성질로 옮기는 것을 검토한다.
- **`pty.attach_surface` 가 mirror 워크스페이스에서 같은 증상을 보인다** — ADR-0060 이 남긴 갭과
  같은 축이다. 그때는 이 가드의 대상을 그 경로까지 확장할지 판정한다.

## References

- [ADR-0060](0060-block-terminal-spawn-into-hard-occupied-workspace.md) — 서버(hard-occupied) 축의
  대칭 결정. 정책은 유지되고 집행 지점만 본 ADR 이 옮긴다.
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — 점유 모델(유지).
- [features/remote-attach](../features/remote-attach/index.md) — mirror 구조 변경 forward 설계와
  거부 대상 서술. mirror 여부의 조회 표면(`workspace.list` 의 `mirror`, `list workspaces` 의
  `[mirror]` 마커)도 같은 문서에 있다.
- [dev-guide/attach-behavior](../dev-guide/attach-behavior.md) — 가드 메커니즘 상세.
- [dev-guide/api-conventions](../dev-guide/api-conventions.md) — mirror forward 응답 계약
  (생성 id 를 담지 않는다).
- 영향 파일: `src/adapters/ipc/handler.rs`(`spawn_target_guard`, `hard_occupied_denial`),
  `src/adapters/ipc/handler/terminal.rs`(`handle_spawn`),
  `src/core/attach_runtime.rs`(`forward_exec_tests` 회귀 테스트).
