# ADR-0264: mirror 의 "닫은 항목 복원" 은 원격에서 실행하고, 복원 스택은 워크스페이스로 스코프한다

- **Status**: Accepted
- **Date**: 2026-09-12
- **Tags**: attach, mirror, remote, restore, closed-item, wire-format, focus, user-agent-separation, adr-0040, adr-0045, adr-0086
- **Group**: remote-attach

## Context

mirror(원격 attach) 워크스페이스를 보고 있는 상태에서 `restore_closed`(기본 `Ctrl+Shift+T`)를 누르면 **로컬 탭이 mirror 워크스페이스 안에 생긴다.** 로컬 PTY 까지 함께 뜬다.

- 단축키가 쏘는 `DomainIntent::RestoreClosedItem` 은 `Core::apply` 의 mirror 게이트(`CoreState::mirror_workspace_index_for_structural`)가 보는 구조 op 목록에 들어 있지 않아 게이트를 그냥 통과한다. 그 목록은 split/close/create-tab/move 계열 9 종뿐이고 나머지는 `None` 으로 떨어진다.
- 통과한 뒤 `Core::apply_restore_closed_item` 의 Surface/Tab 갈래는 `push_tab_to_pane` 로 전 workspace 를 순회해 pane 을 찾아 탭을 꽂고, Pane 갈래는 `insert_pane_beside` 로 같은 일을 한다 — 어느 쪽도 mirror 플래그를 보지 않는다. 복원은 `restore_rebuild` 가 **PTY 를 spawn** 하므로, 결과적으로 "workspace 전체가 remote" 불변식이 깨진다.

반대 방향은 이미 닫혀 있다. mirror 안에서 닫은 것은 클라이언트 복원 스택에 안 쌓인다 — `AppState::close_tab`/`close_active_tab`/`close_active_pane` 은 `forward_mirror_structural` 의 이른 return 이 스냅샷 캡처보다 앞에 있다. 즉 클라이언트 스택에 든 항목은 **항상 로컬 워크스페이스 것**이고, 그것이 그대로 mirror pane 에 복원된다 — 나가는 쪽만 막고 들어오는 쪽을 안 막은 형태다.

그리고 **원격에도 복원할 것이 없다.** forward 된 close 는 서버에서 기존 IPC 핸들러로 실행되는데 그 경로가 전부 에이전트 close 취급이라 스냅샷을 남기지 않는다 — `close_surface_via_intent` 는 `save_snapshot: false` 를 하드코딩하고, `Core::apply_close_tab`/`apply_close_pane` 에는 `save_snapshot` 인자 자체가 없다.

복원 스택 자체도 스코프가 없다. `ClosedItemStore` 는 `VecDeque<ClosedItem>` 하나뿐인 전역 LIFO 이고 workspace 귀속 정보가 없다. 그대로 두고 forward 를 붙이면 서버 앞에 앉은 로컬 사용자와 원격 mirror 사용자가 **같은 한 스택을 서로 뺏는다.**

## Decision

### 1. 복원 스택의 소유자는 서버(authoritative 인스턴스)다

mirror 에서의 복원은 서버가 실행한다. 근거 셋이 같은 방향을 가리킨다.

- **스크롤백이 서버에 있다.** `ClosedScrollback::Persisted` 가 쥐는 것은 그 인스턴스의 디스크 저장소를 가리키는 참조다. 클라이언트가 복원해도 그 참조를 읽을 수 없다.
- **PTY 가 서버에 있다.** 복원은 새 PTY spawn 이고, mirror 는 detached 터미널이라 PTY 를 만들 자격 자체가 없다 — [ADR-0086](0086-reject-terminal-spawn-into-mirror-workspace.md) 이 `terminal.spawn` 에 대해 내린 결정과 같은 축이다.
- **기존 구조 op 가 전부 그 모델이다.** split/new-tab/close/move 는 이미 원격에서 실행하고 `StructuralDelta` 로 되반영한다. 복원은 "새 탭을 만드는 op" 의 한 종류라 그 모델에 그대로 얹힌다.

### 2. mirror 에서의 복원 요청은 forward 하고, 로컬 폴백을 두지 않는다

원격 스택이 비어 있어도 로컬 스택을 대신 pop 하지 않는다 — 그러면 지금의 버그와 같은 결과(로컬 탭이 mirror 안에 생김)가 된다. 원격이 "복원할 것 없음" 을 회신하면 전용 사유로 회신하고 클라이언트가 그에 맞는 안내 toast 를 띄운다(일반 forward 실패 문구를 재사용하지 않는다 — 그 문구는 오류로 읽힌다).

클라이언트의 로컬 스택은 **그대로 남는다.** 사용자가 로컬 워크스페이스로 돌아가 같은 단축키를 누르면 로컬 항목이 복원된다. 스택이 둘(로컬/원격)이고 **보고 있는 워크스페이스가 어느 쪽을 쓸지 정한다** — 이것이 이 결정의 사용자 관점 요약이다.

### 3. 서버 복원 스택은 **워크스페이스로 스코프한다**

`ClosedItemStore` 의 각 엔트리는 닫힐 당시의 **출처 워크스페이스 id** 를 함께 싣는다. 워크스페이스 통째 항목(`ClosedItem::Workspace`)은 어느 워크스페이스에도 속하지 않으므로 출처가 `None` 이다. 출처는 push 시점에 항목 자신의 구조 id(surface/tab/pane)로 **저장소가 직접 판정한다** — push 는 전부 트리 재배치 **전**에 일어나므로 그 시점의 트리가 답을 갖고 있고, 이 방식이라 13 개 push 호출부가 손을 타지 않는다.

pop 은 스코프 술어를 받는다.

- **forward 된 복원**(원격 사용자): 출처가 **anchor 워크스페이스인 항목만** 꺼낸다. 다른 워크스페이스 항목은 애초에 후보가 아니다.
- **로컬 복원**(서버 앞의 사용자): **기존 전역 LIFO 를 그대로 둔다.** 스코프를 걸지 않는다.

이 비대칭이 의도적이고, 스코프를 **원격 쪽에만** 거는 데에는 세 가지 이유가 있다.

- 로컬 복원의 기존 관례 — "닫힐 당시 워크스페이스가 아니라 호출 시점 focused pane 을 현재 컨텍스트로 삼는다" — 는 `Core::apply_restore_closed_item` 이 주석으로 명시한 결정이다. 이 ADR 의 물음은 "복원이 어디서 실행되는가" 이지 "로컬 복원이 무엇을 고르는가" 가 아니다.
- forward 된 close 는 **서버 자신의 트리에서 탭을 없앤다.** 그 close 를 로컬 사용자가 되돌릴 수 없게 만들면, 자기 기계에서 벌어진 변경에 undo 가 없는 상태가 된다.
- 로컬 사용자가 원격 출처 항목을 되돌리는 것은 **자기 화면 안에서 벌어지는 가시적 동작**이다. 반대 방향(원격의 복원이 anchor 밖 항목을 가져가는 것)과 달리 조용히 일어나지 않는다.

세 가지가 이 스코프에서 함께 닫힌다.

- 원격 사용자의 `Ctrl+Shift+T` 가 서버 로컬 사용자가 방금 닫은 탭을 가져가지 않는다.
- 복원 결과가 항상 anchor 워크스페이스 안에 떨어지므로 `execute_forwarded_structural_op` 의 before/after diff 가 그것을 잡는다 — "서버에는 탭이 생겼는데 클라이언트에는 아무 일도 안 일어난 것처럼 보이는" 상태가 안 생긴다.
- `ClosedItem::Workspace` 갈래(출처 `None`)는 forward 스코프 술어에 **원리적으로 안 걸린다.** 그 갈래는 anchor 워크스페이스 밖에 새 워크스페이스를 만들어 delta 에 안 잡히는데, pop 한 뒤 거부하면 항목만 잃는다. 스코프가 그 갈래를 pop 이전에 배제한다.

### 4. forward 된 close 는 "복원 가능한 close" 다

forward 된 close 를 일으킨 것은 **원격 사용자의 손 조작**(단축키/버튼/컨텍스트 메뉴)이다. 그래서 서버는 그 close 에 스냅샷을 남긴다. IPC 전반의 정책(`save_snapshot=false`, 에이전트 경로 전제)은 그대로 두고 **forward 경로만** 갈라낸다 — 면제를 `params` 플래그가 아니라 **호출 경로**로 표현하는 `close_surface_for_attach_holder` 의 기존 규율과 같은 형태다. (현재 형태: 그 축은 도메인 함수 `core::structural_exec::close_surface` 의 `save_snapshot` 인자이고, forward 실행이 `true`, IPC 핸들러가 `false` 로 부른다 — [ADR-0395](0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md).)

`save_snapshot`(되돌리기 스택)과 `is_user_close`(plugin lifecycle 이벤트)는 **독립 축**이고 하나로 접지 않는다. 이 결정이 바꾸는 것은 `save_snapshot` 축뿐이다. forward 된 close 는 지금처럼 `is_user_close=false` 로 나간다.

### 5. wire 확장의 무음 실패는 감수 가능한 degrade 다

`StructuralOp` 에 새 variant 를 넣으면 구버전 서버는 역직렬화 실패로 그 프레임을 **통째로 무시한다** — 회신조차 오지 않는다. 이는 attach 채널이 이미 명시한 전방/후방 호환 정책("알 수 없는 event 는 역직렬화 실패로 무시")의 결과이고, 새 variant 를 위해 별도 협상 채널을 만들지 않는다. forward 는 fire-and-forget 이라 블로킹은 없고, 남는 비용은 그 세션 동안 회수되지 않는 `pending_op_focus` 엔트리 하나다.

## Consequences

- **얻은 것**: mirror 에서 누른 복원이 원격에서 실행돼 원격 PTY 를 가진 탭이 되살아나고, delta 로 mirror 에 나타난다. "workspace 전체가 remote" 불변식이 복원 경로에서도 유지된다. 두 사용자의 복원 히스토리가 서로 간섭하지 않는다.
- **잃은 것**: 간섭이 **한 방향 남는다.** 서버 앞의 사용자가 누른 복원은 여전히 전역 top 을 가져가므로, 그 top 이 원격 사용자가 방금 닫은 것이면 그것이 로컬 워크스페이스로 되살아나고 원격 사용자의 스택에서는 사라진다. 위 세 이유로 이 방향은 **감수한다** — 다만 조용한 쪽(원격이 로컬 것을 가져가는 방향)과 달리 감수 가능하다는 판단이지, 문제가 없다는 뜻이 아니다.
- **운영 비용 / 유지 부담**: forward 된 close 가 서버 스택에 쌓이기 시작하므로 **서버의 복원 스택 회전이 빨라진다** — `MAX_CLOSED_ITEMS` eviction 이 그만큼 자주 돌고, evict 된 항목의 디스크 스크롤백 해제도 그만큼 자주 일어난다. 이 해제는 `CoreState::push_closed_item` 만이 한다 — `closed_items.push` 를 직접 부르면 `~/.tasty/scrollback/*.bin` 이 샌다.
- 복원된 스크롤백은 **서버에서만** 완전하다. client mirror 는 기존 attach 규약대로 visible 화면 1 회 스냅샷 + 이후 delta 만 받으므로, 복원된 탭의 과거 스크롤백 전량이 mirror 로 가지는 않는다. mirror 의 기존 동작과 같고 이 결정의 결함이 아니다.

## Alternatives Considered

- **복원을 클라이언트가 실행한다** — 스크롤백 참조와 PTY 가 둘 다 서버에 있어 성립하지 않는다. 클라이언트가 만들 수 있는 것은 빈 로컬 터미널뿐이고, 그것이 지금의 버그다.
- **원격 스택이 비면 로컬 스택으로 폴백** — 폴백의 결과가 정확히 현재 버그(로컬 PTY 탭이 mirror 안에 생김)라, 고치려는 것을 조건부로 남기는 꼴이다.
- **스택 스코프 (b) — 점유 중 워크스페이스 항목만 holder 전용 별도 스택** — "닫은 항목 복원" 이라는 한 기능에 스택을 둘로 늘린다. 로컬 사용자가 자기 워크스페이스를 되돌릴 때의 규칙이 그 워크스페이스의 점유 여부에 따라 갈리고, 점유가 붙었다 떨어질 때 항목이 어느 스택에 있는지가 사용자에게 안 보인다.
- **스택 스코프 (c) — 스코프 없이 전역 pop** — 가장 단순하지만 두 사용자 간 간섭을 그대로 남긴다. 복원 결과가 anchor 워크스페이스 밖이면 delta 가 안 나가므로 그 경우를 감지해 `ok:false` 로 회신해야 하는데, 그 시점에 항목은 이미 pop 된 뒤다 — 되돌리려면 push 를 되감아야 하고, 되감기가 스크롤백 파일 수명과 얽힌다.
- **`DomainIntent::CloseTab`/`ClosePane` 에 `save_snapshot` 축을 추가** — 세 close 가 같은 모양이 된다는 장점이 있으나, 시그니처 변경이라 호출부가 전부 손을 타고 `apply_close_tab`/`apply_close_pane` 이 surface registry 와 terminal store 를 함께 받아야 해 Core 쪽 인자 배선이 늘어난다. 이 결정이 바꾸려는 것은 "IPC 가 스냅샷을 남기는가" 가 아니라 "**forward 경로**가 남기는가" 이고, 캡처를 forward 실행 지점에 두는 쪽이 그 구분을 코드 위치로 그대로 표현한다.
- **forward 된 복원의 cascade 를 로컬과 똑같이 재현** — `cascade_closed_item_restored` 의 두 갈래가 `AppState::active_workspace` 와 `focused_pane` 을 바꾼다. forward 경로에서 그것을 부르면 **원격 사용자의 조작이 서버 앞에 앉은 로컬 사용자의 화면을 움직인다** — 루트 `CLAUDE.md` 원칙 1(사용자 행동 ↔ 에이전트 행동 분리)·원칙 3(포커스 독립성)과 [`docs/design/policies/focus.md`](../design/policies/focus.md) 가 정확히 금지하는 형태다. client 쪽 focus 는 `PendingOpFocus::NewResource` 가 delta 적용 시점에 client-only 로 보정하므로 서버 상태를 만질 이유가 없다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `ClosedItemStore` 가 출처 워크스페이스를 더 이상 싣지 않게 되면(엔트리 타입에서 그 필드가 사라지면) 결정 3 의 전제가 없어진다.
- `StructuralOp` 의 복원 variant 가 사라지면 결정 1·2 가 실행 중이 아니다.
- forward 실행(`execute_forwarded_structural_op` 의 `CloseSurface` arm)이 `structural_exec::close_surface` 를 `save_snapshot=false` 로 부르면 결정 4 가 뒤집힌 것이다(옛 이름 `close_surface_for_attach_holder` 는 ADR-0395 에서 사라졌다).

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 복원 스택을 **워크스페이스가 아니라 사용자(attach client)별로** 나눠야 한다는 요구가 생길 때. 한 워크스페이스를 여러 holder 가 번갈아 점유하는 사용 패턴이 실제로 나오면 스코프의 좌변이 워크스페이스가 아니게 된다. 재는 법: 같은 워크스페이스의 holder 가 세션 중 교체된 사례가 보고되는지, 그때 이전 holder 가 닫은 항목이 새 holder 에게 보이는 것이 문제로 지적되는지.
- 위 "남는 한 방향" 이 실제로 물리는지 — 서버 앞 사용자가 복원을 눌렀는데 자기가 닫은 적 없는 원격 탭이 돌아오는 일. 재는 법: 그 형태의 보고가 나오는지. 나오면 로컬 복원에도 스코프를 거는 것이 다음 선택지이고, 그때는 "원격이 닫은 것을 서버 사용자가 되돌릴 수 없어진다" 는 위 두 번째 이유와 맞바꾸는 판단이 된다.

## References

- 코드 근거(이 결정이 실현된 현재 위치): `crates/tasty-model/src/closed_item.rs`(`ClosedItemStore`·출처 판정) · `src/core/impl_workspace.rs`(`apply_restore_closed_item`) · `src/core/impl_mirror.rs`(`build_mirror_forward_op`) · `src/core/state/finders.rs`(`mirror_workspace_index_for_structural`) · `src/core/attach_runtime.rs`(`execute_forwarded_structural_op`) · `crates/tasty-ipc/src/stream.rs`(`StructuralOp`).
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) "mirror 구조 변경 forward" — forward 결선 전체.
- [`docs/features/remote-attach/index.md`](../features/remote-attach/index.md) "mirror 워크스페이스 내 구조 변경" — 개념·정책.
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) 계열 — hard 점유 = 구조 변경 권한 모델.
- [ADR-0045](0045-mirror-geometry-client-driven.md) — client→server forward 큐 패턴의 원형.
- [ADR-0086](0086-reject-terminal-spawn-into-mirror-workspace.md) — mirror 워크스페이스에 로컬 터미널을 만들지 않는다는 같은 축의 선례. 그 ADR 이 열거한 라우터 가드 대상에는 복원이 없었다.
- 부분 개정: [0480](0480-a-forwarded-close-carries-who-asked-for-it.md) (결정 4 개정 — forward 된 close 가 전부 원격 사용자의 손 조작이라는 전제를 걷고, 에이전트가 요청한 close 는 서버 복원 스택에 남기지 않는다)
