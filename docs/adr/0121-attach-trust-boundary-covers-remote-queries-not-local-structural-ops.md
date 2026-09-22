# ADR-0121: attach 신뢰경계는 원격 조회를 덮고 로컬 구조 op 는 덮지 않는다 — `remote.attach` 는 plugin 미개방

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: ipc, permissions, remote-attach, plugin, trust-boundary, user-agent-separation, identity-principle-1, method-table, asymmetry
- **Group**: remote-attach

## Context

`remote.*` IPC 메서드군을 권한 표(`crates/tasty-ipc/src/method_meta.rs`)에 등재하면서, 각 메서드를 plugin/agent 호출자에게 열지(`plugin(&[])`) local 전용으로 둘지(`local_only()`) 결정해야 했다.

이 레포의 확립된 정책은 **"attach 로 노출하는 기능은 SSH 로 이미 가능한 것의 부분집합인가만 판단한다 — 그러면 별도 permission gate 를 두지 않는다"** 였다(`docs/dev-guide/attach-behavior.md` 결정5, 연결 경계 = SSH + 127.0.0.1 loopback 에 위임). 이 정책은 조회·프로필 CRUD 계열(`remote.workspaces`, `remote.profile.*`, `remote.passkey.*`)에는 그대로 들어맞는다 — 그 데이터는 SSH 로 그 호스트에 들어갈 수 있는 사람이 이미 볼 수 있는 것이다.

그런데 `remote.attach` 는 성격이 다르다. 조회가 아니라 **호출자가 실행 중인 로컬 tasty 인스턴스에 mirror 워크스페이스를 만드는 구조 op** 다 — 사용자의 워크스페이스·창 구성(사용자 상태)을 바꾼다. 권한 게이트 구조상 이 구분이 중요하다: `CallerContext::Local`(세션 토큰 없는 CLI·로컬 스크립트)은 `caller.rs` 에서 권한 표를 거치지 않고 통과하지만, plugin/agent 세션 토큰 호출자는 표에 없으면 `UnknownMethod` 로 거부되고 `plugin(&[])` 로 등재하는 순간 새로 열린다. 즉 이 결정은 **설치된 plugin(요구 권한 0)이 사용자의 로컬 워크스페이스 목록을 바꿀 수 있게 되는가**를 가른다.

기존 정책 문구("SSH 로 가능한가")를 글자 그대로 적용하면 "SSH 로 원격에 붙을 수 있으니 attach 도 열어라" 로 읽혀 `remote.attach` 까지 `plugin(&[])` 이 될 수 있다. 그러나 SSH 자격이 실제로 부여하는 것은 **원격 셸 접근**이지, 그 사용자의 **로컬 창에 워크스페이스를 만들 권한**이 아니다. 경계가 한 칸 어긋난다.

## Decision

**attach 신뢰경계(SSH + loopback)는 *원격에 대해 이미 가능한 일*을 덮는다. 사용자의 로컬 tasty 구성(워크스페이스·창)을 바꾸는 *로컬 구조 op* 는 그 경계가 덮지 않는다 — 원격 접속 자격이 로컬 상태 변경 권한을 파생시키지 않는다.** 이는 기존 정책을 *뒤집는 것이 아니라 정밀화*하는 것이다: "SSH 로 가능한가" 는 여전히 조회·CRUD 계열의 기준이고, 여기에 "그 기능이 로컬 사용자 상태를 바꾸는 구조 op 인가" 라는 두 번째 관문을 명시한다.

적용 결과 (`method_meta.rs`):

- **조회·프로필·자격 CRUD → `plugin(&[])`**: `remote.workspaces`(원격 ws 브라우징), `remote.profile.*`, `remote.passkey.*`. SSH 로 이미 볼 수 있는 것이라 연결 경계 위임으로 충분, 추가 Permission 없음.
- **로컬 구조 op → `local_only()`**: `remote.attach`. Local·CLI(`tasty tool attach`)는 그대로 동작하고, plugin/agent 에는 열지 않는다.

**이 비대칭(`remote.workspaces` 는 열고 `remote.attach` 는 안 여는 것)은 의도된 것이다.** 두 메서드가 `tasty_remote` 코어를 공유하고 이름이 인접하다는 이유로 "일관성" 을 들어 `remote.attach` 를 함께 열지 않는다 — 그 정리는 위 경계를 지우는 회귀다.

## Consequences

- **얻은 것**: 요구 권한 0 인 plugin 이 사용자의 로컬 워크스페이스 구성을 바꾸는 경로가 닫힌다(불가침 원칙 1 — 에이전트 행동의 부수효과가 사용자 상태에 닿지 않는다). 조회 계열은 종전대로 열려 있어 에이전트가 원격 ws 를 브라우징하는 능력은 유지된다. 여는 방향은 non-breaking 이라, 필요가 확인되면 나중에 안전하게 열 수 있다.
- **잃은 것**: plugin/agent 가 소켓으로 직접 mirror attach 를 트리거하지는 못한다(현재 그 소비자는 없다). 필요해지면 재검토 트리거로 다룬다.
- **운영 비용 / 유지 부담**: `remote.*` 안에 열림/닫힘 비대칭이 생겨, 근거를 모르는 사람이 "일관성 정리" 로 지울 위험이 있다. 이 ADR 이 그 방지책이다 — `method_meta.rs` 의 해당 주석이 이 ADR 을 인용한다.

## Alternatives Considered

- **(a) `remote.attach` 도 `plugin(&[])`**: attach 계열 이름 일관성은 얻지만, 권한 0 plugin 이 사용자 워크스페이스 목록을 바꾸게 된다 — 원칙 1 이 말하는 "사용자 상태" 에 정확히 닿는다. SSH 신뢰경계가 덮지 못하는 범위라 근거가 서지 않아 기각.
- **(b) `remote.attach` 만 새 권한 토큰 요구**(예: `Remote`/`SurfaceWrite` 계열 Permission): 조회는 열되 구조 op 는 명시 권한 뒤에 두는 절충. 그러나 이 메서드를 필요로 하는 plugin 소비자가 **아직 없다**. 소비자 없는 메서드 하나를 위해 새 Permission 종류를 도입하는 비용이 이르다 — 닫아두는 것(`local_only()`)이 더 싸고, 여는 방향은 non-breaking 이라 필요 시점에 (b) 로 여는 것이 첫 후보다. 그래서 지금은 기각, 재검토 트리거로 보류.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- plugin/agent 가 `remote.attach` 를 정당하게 필요로 하는 사례가 생길 때 — 그때는 **(b)**(열되 명시 권한 뒤에)가 첫 후보다. `plugin(&[])`(무권한 개방)로 바로 가지 않는다.
- 다른 `remote.*`(또는 인접 IPC)에 **로컬 구조 op**(사용자 상태를 바꾸는 것)가 새로 추가될 때 — 같은 경계 판정을 적용한다(조회면 열고, 구조 op 면 닫는다).
- 불가침 원칙 1 의 "사용자 상태" 범위가 재정의될 때 — 이 경계 판정의 전제가 바뀐다.

## References

- `docs/dev-guide/attach-behavior.md` — attach 신뢰경계(연결 경계 위임) 결정5
- `crates/tasty-ipc/src/method_meta.rs` — `remote.*` 등재(이 ADR 의 적용 지점)
- `crates/tasty-ipc/src/caller.rs` — `CallerContext` 게이트(Local 은 표 미경유, Plugin/Agent 만 `check_permissions`)
- `docs/identity.md` — 불가침 원칙 1(사용자 행동 ↔ 에이전트 행동 분리)
