# ADR-0089: 크레이트 분리 기준은 줄 수보다 의존 방향이 우선한다

- **Status**: Accepted
- **Date**: 2026-08-30
- **Tags**: build, crate-layout, dependency-direction, remote-attach
- **Group**: architecture

## Context

[`docs/dev-guide/build.md`](../dev-guide/build.md) §크레이트 분리 가이드는 본 바이너리의 leaf 모듈을 떼어낼 후보 조건을 셋으로 적어 뒀다 — **out-degree 작음** · **사이클 없음** · **충분히 큼(1000줄+)**. 이 조건은 "본 바이너리가 너무 커서 빌드가 느리다" 는 문제를 겨냥해 만들어졌고, 그 맥락에서 크기는 합리적인 문턱이었다.

그런데 원격 attach 계열을 정리하면서 크기 조건과 어긋나는 사례가 나왔다. 원격 인스턴스 조회/생성 코어(`browse` + `create`)는 **429줄**로 1000줄에 한참 못 미치지만, 소비자가 셋(CLI · 본체 GUI 팝업 · 로컬 IPC 핸들러)이고 의존 성격이 이웃 크레이트와 명확히 다르다:

- SSH 위임(`tasty-ssh`)은 **IPC 를 모른다** — 시스템 `ssh` 프로세스·터널·포트 발견만 안다.
- 원격 조회/생성은 그 터널 위에서 **JSON-RPC(`tasty-ipc`)를 호출**한다.

크기 조건만 보면 이 코드는 어딘가에 합쳐야 하고, 합칠 곳은 `tasty-ssh` 아니면 `tasty-ipc` 다. 어느 쪽으로 합치든 두 계층 사이에 없던 의존이 생긴다.

## Decision

**크레이트 분리 여부는 의존 방향으로 판정하고, 줄 수는 보조 지표로만 쓴다.** 어떤 코드 뭉치가 (a) 소비자가 둘 이상이고 (b) 합칠 후보 크레이트에 **그 크레이트가 원래 몰라도 되는 의존**을 새로 들이게 만든다면, 줄 수가 문턱에 못 미쳐도 별도 크레이트로 분리한다.

이 기준에 따라 `browse`/`create` 는 429줄이지만 `tasty-remote` 크레이트로 분리했다. 의존은 `tasty-remote → {tasty-ssh, tasty-ipc}` 한 방향으로만 흐르고, `tasty-ssh` 는 IPC 를 모르는 상태로 남는다.

`build.md` 의 "1000줄+" 은 삭제하지 않는다 — **빌드 시간 관점의 후보 발굴 힌트**로는 여전히 유효하다. 다만 그것은 분리를 *제안*하는 조건이지 *금지*하는 조건이 아니다.

## Consequences

- **얻은 것**: `tasty-ssh` 가 IPC 프로토콜을 모르는 상태로 유지된다. SSH 터널만 필요한 소비자가 JSON-RPC 전체를 빌드 그래프에 딸려오지 않는다. 의존 그래프가 계층 순서(ssh → remote → cli/host)로 읽힌다.
- **잃은 것**: 크레이트 수가 하나 늘어난다. 루트 `Cargo.toml` 의 `[profile.dev.package.*]` 항목과 workspace 멤버 관리 대상이 그만큼 늘어난다.
- **운영 비용 / 유지 부담**: 낮다. 신설 크레이트는 leaf 이고 공개 표면이 11개로 작다. 다만 "작으니까 합치자" 는 리팩터 제안이 앞으로도 반복될 수 있어, 그때마다 이 ADR 이 판정 근거가 된다.

## Alternatives Considered

- **`tasty-ssh` 에 합친다** — 429줄짜리 크레이트 신설을 피할 수 있다. 그 대가로 `tasty-ssh` 가 `tasty-ipc` 에 의존하게 되어, SSH 터널만 쓰려는 소비자가 IPC 프로토콜을 통째로 딸려온다. 분리의 목적(빌드 그래프 축소)과 정면으로 어긋나 기각.
- **`tasty-ipc` 에 합친다** — `browse`/`create` 가 결국 IPC 호출이라는 점에서 자연스러워 보인다. 그러나 `tasty-ipc` 는 서버(호스트)와 클라이언트가 모두 쓰는 wire 계층인데, 여기에 SSH 위임 의존이 들어가면 **SSH 를 전혀 모르는 IPC 서버**까지 `tasty-ssh` 를 딸려오게 된다([ADR-0007](0007-attach-targets-remote.md) 의 "원격성은 client 가 흡수한다" 와 어긋난다). 기각.
- **`build.md` 의 1000줄 조건을 삭제한다** — 크기 힌트 자체는 빌드 시간 관점에서 유효하고, 지우면 "작은 크레이트를 무제한으로 늘려도 된다" 로 읽힌다. 조건을 지우는 대신 **우선순위를 명시**하는 쪽을 택했다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 워크스페이스 크레이트 수가 늘어 `cargo build` 의 링크/메타데이터 오버헤드가 실측으로 유의미해질 때(크레이트당 고정비가 분리 이득을 넘어서는 지점).
- `tasty-ssh` 가 다른 이유로 IPC 에 의존하게 되어, 분리의 근거였던 방향 구분이 사라질 때.
- 분리한 크레이트가 소비자 하나로 줄어들어 "공유 코어" 라는 전제가 깨질 때.

## References

- [`docs/architecture/index.md`](../architecture/index.md) §도메인-IO — 이 결정이 그 절의 계층 규칙에 남긴 유일한 예외(`tasty-remote` → `tasty-ipc`)를 그 자리에 적어 두었다
- [`docs/dev-guide/build.md`](../dev-guide/build.md) §크레이트 분리 가이드 — 후보 조건 3종과 이 ADR 이 정한 우선순위
- [ADR-0007](0007-attach-targets-remote.md) — 원격성은 client 가 흡수한다(IPC 서버는 loopback 만 안다)
- [ADR-0032](0032-remote-attach-two-layer-split.md) — ssh / tasty-attach 프로필 2레이어
- [`docs/concepts/ubiquitous-language.md`](../concepts/ubiquitous-language.md) — "SSH 위임" 항목
