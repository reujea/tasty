# ADR-0054: 원격 파일 바이트 전송(bulk)은 attach 스트림 위 native binary 채널로 구현한다 (sftp/SMB 등 외부 프로토콜은 이 기능에 미사용)

- **Status**: Accepted
- **Date**: 2026-07-23
- **Tags**: remote, attach, mirror, file-transfer, native-protocol, stream, bulk-channel, no-base64, ssh-delegation, cross-platform, adr-0007, adr-0032, adr-0045
- **Group**: remote-attach

## Context

tasty 의 원격 attach 는 **원격 tasty 인스턴스를 자체 프레임 프로토콜로 mirror** 하는 구조다([ADR-0032](0032-remote-attach-two-layer-split.md), [ADR-0007](0007-attach-targets-remote.md)). 전송 스택은:

- **베이스**: `127.0.0.1:<local_port>` 로의 평범한 TCP.
- **원격 암호화**: 그 포트가 `ssh -L 127.0.0.1:local:127.0.0.1:remote -N` 터널의 끝점(`crates/tasty-ssh/src/lib.rs`). SSH 는 **TCP 포트 포워딩만** 제공하고, sftp/scp/exec 같은 SSH 네이티브 채널은 쓰지 않는다.
- **애플리케이션 프로토콜**: tasty 자체 프레이밍(`crates/tasty-ipc/src/stream.rs`) — `[tag u8][len u32 BE][payload]`, `MAX_FRAME_LEN` 1 MiB. `Data`(raw 바이트) / `Control`(JSON) / `Ping` / `Detach` 태그, workspace attach 는 Data 앞에 4바이트 surface_id(`encode_mux`).

보안·인증은 전부 SSH 경계에 위임한다(tasty 자체 보안 계층 없음 — "SSH 로 들어올 수 있으면 attach 자격"). 원격 attach 대상은 **항상 tasty 인스턴스**다.

**필요**: mirror 로 **파일 바이트를 원격에 전송**하는 경로가 필요하다. 첫 소비자는 *attach 된 터미널(예: Claude Code CLI)에 이미지를 붙여넣을 때, 그 이미지를 원격에 올려 원격 파일시스템 경로를 회신받아 그 경로를 붙여넣는 것*이다. 로컬 PNG 저장 + 로컬 경로 붙여넣기는 mirror 에선 무의미하다(원격이 로컬 경로를 읽을 수 없다).

> **스코프 한정**: 본 ADR 은 **원격으로 파일 바이트를 나르는 전송 계층**의 결정만 다룬다. 원격 폴더 브라우징(explorer 원격 탐색)·원격↔로컬 복사 등 상위 파일시스템 기능은 **범위 밖**이며, 필요해지면 이 전송 계층 위에서 별도로 설계한다.

현재 유일한 "파일성" 전송은 스크린샷 캡처 업로드다 — 청크를 `Control`(JSON) 프레임에 `data_b64`(base64)로 실어 보내고 원격이 파일로 저장 후 경로를 회신한다(`src/app/attach_client.rs`). 이는 "작은 스크린샷이니 control 채널 재활용" 이라는 지름길이며 일반 파일 전송의 베이스로 삼기엔 base64(+33%)·control 채널 공유(HOL) 문제가 있다.

## Decision

원격 파일 바이트 전송을 **tasty 자체 프레임 프로토콜(attach 스트림) 위 native 로 구현**한다. control/data 를 분리한다:

- **메타데이터 — control plane**: 전송 시작(파일명·총 크기)·완료(commit)·원격 경로 회신은 구조화된 JSON `Control` 메시지로 주고받는다(기존 `StructuralDelta` 류 구조화 forward 의 연장선). 작고 문자열/숫자뿐이라 base64 무관.
- **파일 바이트 — data plane**: 실제 파일 바이트는 **전용 bulk 연결**(같은 `ssh -L` 터널 위 `127.0.0.1:<local_port>` 에 두 번째 `stream.open`, bulk 모드로 핸드셰이크)에서 **`Data`(binary) 프레임으로 raw 청킹**해 나른다. **base64 를 쓰지 않는다.** 대화형 attach 스트림과 소켓을 분리해 대량 전송이 대화형 I/O 를 head-of-line 블로킹하지 않게 한다.

tasty native(host)의 책임은 **이 전송 채널과, 그것을 소비자(내장 붙여넣기 경로 또는 plugin)가 호출할 수 있는 진입점을 제공**하는 데 그친다. "언제 무엇을 업로드할지"(예: Claude Code 이미지 붙여넣기 감지·경로 삽입 규칙)의 정책은 소비자 쪽에서 결정한다.

**이 결정은 "다른 프로토콜 금지" 가 아니다.** *이 기능*(attach 된 tasty 호스트로의 파일 바이트 전송)에 native 를 택한 것이며, 장래에 다른 목적·스코프(예: 임의 파일서버 브라우징을 위한 SMB/NFS/sftp 지원)를 tasty 에 별도 기능으로 추가하는 것은 이 ADR 이 배제하지 않는다. 그런 프로토콜은 이 기능의 베이스가 아니라 **병행 옵션**으로 붙을 수 있다.

## Consequences

- **얻은 것**:
  - **전 attach 모드 균일** — 원격 프로필/수동(`attach.into_gui`)/loopback 어디서나 동작한다(그냥 같은 포워딩 포트에 또 붙는 것이라). sftp 는 프로필 SSH attach 에서만 가능한 것과 대비된다.
  - **외부 의존성 0** — 원격 sftp 서브시스템(하드닝 서버는 비활성화하기도 함)·scp/sftp 바이너리·SMB 서버/마운트가 불필요. 크로스플랫폼 부담이 줄어든다.
  - **원격이 경로를 소유** — 저장 위치·권한·temp 관례를 가장 잘 아는 원격 tasty 가 경로를 정해 회신한다. 클라가 원격 목적지를 발명할 필요가 없다.
  - **base64/HOL 회피** — binary `Data` 프레임 + 전용 bulk 연결로 팽창과 대화형 스트림 간섭을 제거.
  - **선례 재사용** — 프레이밍·구조화 forward(`StructuralDelta`)·청크 업로드 골격이 이미 있어 신규 전송계층을 만들지 않는다.
- **잃은 것**:
  - sftp/scp 가 공짜로 주는 파일 전송 semantics(청킹·무결성·resume·에러 처리)를 **직접 구현**해야 한다(chunk + seq/offset + commit + 체크섬 수준). 구현량이 늘어난다.
- **운영 비용 / 유지 부담**:
  - bulk 연결의 인가(해당 워크스페이스 holder 와 결속)·수명·에러 처리를 attach 세션과 정합시켜야 한다. 다만 기존 attach 인가/heartbeat 모델([ADR-0052](0052-attach-heartbeat-ttl-hard-occupancy-release.md))을 재사용한다.

## Alternatives Considered

- **A: sftp/scp 로 파일 전송** — SSH 네이티브라 전송 semantics 가 공짜지만, (1) attach 세션이 원격 host/user 를 안 들고 있고(재-plumbing 필요) loopback/수동 attach 는 SSH 자체가 없어 **프로필 SSH attach 에서만** 가능, (2) 원격 sftp 서브시스템 활성 의존, (3) 별도 SSH 연결/바이너리의 크로스플랫폼 quirks. 인증은 blocker 가 아니다(터널이 이미 비대화식으로 붙은 이상 동일 키/agent 로 통과). 원격이 항상 tasty 라는 전제에선 native 가 상위호환이라 기각. (스코프가 "비-tasty SSH 호스트 전송" 으로 넓어지면 재고 — Reconsideration Triggers 참조.)
- **B: SMB / NFS** — 원격에 별도 파일서버(Samba/nfsd) + share export + 별도 auth 체계 + 포트(445 등)가 필요해 "sshd 만 있으면 된다" 는 SSH 위임 원칙을 깬다. 인터넷 노출 보안·특권포트 재바인딩·클라 마운트의 OS 별 상이함까지 겹쳐 이 기능의 베이스로 부적합. ("원격이 이미 파일서버인 별도 시나리오" 를 위한 병행 기능으로는 장래 가능 — 위 Decision 마지막 문단.)
- **C: 기존 캡처 업로드(Control+base64)를 그대로 일반화** — 청크를 JSON control 프레임에 base64 로 실으면 +33% 팽창 + 청크마다 JSON 봉투 + 대화형 control 채널 공유(HOL). data plane 을 binary `Data` 프레임 + 전용 연결로 다시 짜는 것이 일반화의 핵심이라 그대로 복사하지 않는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 전송 대상 스코프가 **attach 된 tasty 호스트를 넘어 임의 SSH/파일서버**로 확장될 때 — 원격이 tasty 를 서빙한다는 전제가 깨져 sftp 등 외부 프로토콜이 필요해진다.
- 매우 큰 파일 전송에서 native 구현(무결성/resume 등)이 sftp 대비 유의미하게 열위임이 실측될 때.
- SMB/NFS/sftp 등을 **별도 기능**으로 tasty 에 넣게 될 때 — 그 자체는 이 ADR 과 충돌하지 않으나(병행), 원격 파일 기능의 베이스를 어느 것으로 통일할지 재정리가 필요해진다.
- bulk 전용 연결의 인가/수명 모델이 기존 attach 점유·heartbeat 모델과 어긋나는 상황이 생길 때.

## References

- 원격 attach 2-레이어 분리: [`ADR-0032`](0032-remote-attach-two-layer-split.md)
- attach 대상·원격 지원: [`ADR-0007`](0007-attach-targets-remote.md)
- mirror geometry client-driven(구조화 forward 선례): [`ADR-0045`](0045-mirror-geometry-client-driven.md)
- 강한 점유 heartbeat TTL(인가/수명 모델): [`ADR-0052`](0052-attach-heartbeat-ttl-hard-occupancy-release.md)
- 스트림 프레이밍/태그/mux: `crates/tasty-ipc/src/stream.rs`
- 캡처 업로드 선례(현행 Control+base64): `src/app/attach_client.rs`
- attach 동작·heartbeat 메커니즘: [`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md)
