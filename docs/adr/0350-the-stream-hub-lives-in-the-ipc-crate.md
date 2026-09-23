# ADR-0350: 스트림 허브는 IPC 크레이트에 산다 — core 는 adapter 를 거치지 않는다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: architecture, layering, ipc, attach, stream, crate-split, hexagonal
- **Group**: architecture

## Context

attach 스트림의 서버측 레지스트리 `StreamHub`(연결마다 bounded push sink 하나, bulk 연결
분류, 입력 프레임 분류 `pump_inbound`, 누적 손실 계수)는 본체의
production adapter 폴더(`src/adapters/production/`)의 `stream_hub.rs` 에 있었다. 그런데 그것을 부르는 쪽의 절반은
adapter 가 아니라 **core** 였다 — `src/core/attach.rs`(점유 레지스트리의 detach 통지자),
`src/core/attach_runtime.rs`(스트림 tap·회신 push), `src/core/mod.rs`(git 조회 종류 타입).
결정 시점(`5dbb0787c`)에 `src/core/` 안의 `adapters::production` 참조는 9 줄이었고 **9 줄
전부가 이 파일 하나**를 가리켰다. core 가 concrete inbound adapter 의 파일 배치에 의존하는
유일한 자리였다.

그 파일이 본체에 남을 이유가 있는지를 재면 **없다**. 파일이 `std` 밖에서 부르는 것을 전수로
세면(`crate::` 경로와 외부 크레이트 경로 모두) 다섯 갈래였고, 본체 모듈은 시험 하나뿐이었다.

| 부르는 것 | 실체 | 자리 |
|---|---|---|
| `crate::ipc::stream`·`crate::ipc::server::IpcWaker` | `tasty-ipc` 의 재수출(`src/adapters/ipc.rs` 의 `pub use tasty_ipc::{…}`) | 47 |
| `crate::poison` | `tasty-utils` 의 재수출(`src/lib.rs`) | 8 |
| `tasty_plugin_protocol::protocol`(`ThemeWire`·`RawInputWire`, 시험의 `ModifiersWire`·`RawInputEventWire`) | 외부 크레이트 직접 경로 — `tasty-ipc` 가 이미 의존한다 | 3 (제품 2 · 시험 1) |
| `serde`(derive)·`serde_json` | 외부 크레이트 직접 경로 — `tasty-ipc` 가 이미 의존한다 | 6 · 26 |
| `crate::core::bulk_transfer::BulkTransferRegistry` | **시험 하나**(`ordered_batch_routes_to_intact_bytes`)만 쓴다 | 1 |

재는 법: 결정 시점 커밋 `5dbb0787c` 의 옛 파일(production adapter 폴더의 `stream_hub.rs`)을 `git show` 로 꺼내 `grep -oE '\b(crate::[a-z_]+|tasty_[a-z_]+|serde_json|serde)::' | sort | uniq -c` 로 센다.

전송 수단도 모른다 — sink 는 `std::sync::mpsc` 채널이고, 소켓을 읽고 쓰는 accept 스레드는
`src/adapters/production/tcp_ipc_server.rs` 에 따로 있다. 즉 허브는 "TCP 구현" 이 아니라 **wire
프레임(`tasty-ipc::stream`)과 같은 층의 공용 전송 계약**인데 adapter 폴더에 놓여 있었다.
같은 형태의 앞선 사례가 `HostIpcInjector` 였고, 그것은 `e28223450` 에서 `tasty-ipc` 로
옮겨졌다(본체 shim 없음).

## Decision

**`StreamHub` 와 그 동반 타입 전부(`PushResult`·`PumpOutcome`·`GitQueryKind`·`StreamInbound`·
`StreamContext`·`BulkEvent`·`CaptureUploadMsg`·`ListDirRequestMsg`·`GitQueryRequestMsg`·
`MarkdownContentRequestMsg`·`StreamLossSnapshot`·`StreamClientId`)를 `crates/tasty-ipc/src/stream_hub.rs`
로 파일째 옮긴다.** 본체의 호출부는 `tasty_ipc::stream_hub::…` 를 직접 부르고, 본체에
재수출 shim 은 두지 않는다. 소켓을 다루는 accept 스레드(`tcp_ipc_server.rs`)는 adapter 에
남는다.

동작은 바꾸지 않는다 — 코드 본문은 import 경로(`crate::ipc::` → `crate::`,
`crate::poison` → `tasty_utils::poison`)와 `GitQueryKind::as_wire_str` 의 가시성
(`pub(crate)` → `pub`, 크레이트 경계를 건너 부르므로)만 바뀐다.

**core 를 역참조하던 시험 하나는 core 쪽으로 옮긴다**(삭제하지 않는다).
`ordered_batch_routes_to_intact_bytes` 는 허브의 분류 결과를 `BulkTransferRegistry` 까지 이어
저장 bytes 가 온전한지 재는 end-to-end 시험이라, 두 계층 중 위쪽인
`src/core/bulk_transfer.rs` 의 시험 모듈로 간다. 허브 쪽에 남는
`pump_inbound_preserves_bulk_begin_chunk_commit_order` 가 분류 순서 보존 자체를 계속 잰다.

## Consequences

- **얻은 것**: `src/core/` 의 `adapters::production` 참조가 9 → 0 이다. 허브가 본체를 부를
  수 없다는 사실을 문장이 아니라 **크레이트 경계**가 강제한다 — `tasty-ipc` 의
  `Cargo.toml` 에 본체가 없다. 새 의존은 없다(`tasty-ipc` 는 이미 `serde`·`serde_json`·
  `tasty-utils`·`tasty-plugin-protocol` 을 가진다).
- **잃은 것**: 본체에서 허브를 부르는 18 파일(시험 이동처 `bulk_transfer.rs` 포함)의 경로가 바뀌어, 같은 파일을 동시에
  고치는 다른 작업과 기계적인 병합 충돌이 난다. 허브를 부르는 core 와 app 이 이제 크레이트
  이름을 직접 적는다.
- **운영 비용 / 유지 부담**: `tasty-ipc` 는 번들 plugin 의 의존 폐포 밖이다(plugin 크레이트
  중 `tasty-ipc` 를 의존하는 것이 없다) — 이 이동은 plugin 버전 bump 를 요구하지 않는다.
  허브 시험은 이제 `cargo test -p tasty-ipc` 에서 돈다. 본체 `cargo test -p tasty --lib` 가
  허브 시험을 더는 세지 않으므로, 두 타깃의 시험 수를 이동 전후로 견줄 때 그 차를 함께 본다.

## Alternatives Considered

- **A. 신설 크레이트(예: 스트림 전송 전용)** — 허브가 부르는 것이 `tasty-ipc::stream`·
  `tasty-ipc::server` 뿐이라 새 크레이트도 결국 `tasty-ipc` 를 의존한다. 크레이트 하나가
  lockstep 자리(아키텍처 목록·README 배지·크레이트 수)를 끌고 오는데 경계가 하나도 더
  생기지 않는다. 안 골랐다.
- **B. 본체 안에서 `src/core/` 나 `src/ports/` 로 옮긴다** — core 의 역참조는 사라지지만,
  허브가 본체 안에 있는 한 `crate::` 한 줄이면 다시 AppState·handler 에 닿는다. 앞으로
  headless core 를 크레이트로 뗄 때 한 번 더 옮겨야 한다. 안 골랐다.
- **C. 옛 경로에 재수출 shim 을 둔다**(`adapters::production::stream_hub` 가
  `pub use tasty_ipc::stream_hub::*`) — 호출부 diff 가 줄지만 core 가 그 shim 경로를 계속 쓰면
  "core → adapter" 가 이름만 남아 경계 판정이 안 된다. `HostIpcInjector` 이동의 선례도
  shim 을 두지 않았다. 안 골랐다.
- **D. end-to-end 시험을 허브 옆에 두고 `BulkTransferRegistry` 도 `tasty-ipc` 로 내린다** —
  레지스트리는 전송이 아니라 서버측 파일 조립 상태이고, 인가·저장을 하는
  `attach_runtime` 과 한 층이다. 시험 하나를 위해 도메인 상태를 전송 크레이트로 내리는 것은
  방향이 거꾸로다. 안 골랐다.

## Reconsideration Triggers

**채널이 붙는 것**

- `crates/tasty-ipc/src/stream_hub.rs` 가 본체(`tasty`) 타입을 필요로 하게 되면 — 컴파일이
  막으므로 그 순간 이 결정을 다시 연다(허브를 본체로 되돌릴지, 그 타입을 내릴지).
- `src/core/` 에 `adapters::production` 참조가 다시 생기면. 재는 법:
  `git grep -n "adapters::production" -- src/core` 가 0 이 아니다.

**원리적으로 안 붙는 것**

- 허브가 TCP 외의 전송(예: 유닉스 소켓·named pipe)에 따라 다르게 동작해야 하는 요구가
  생기면 — 그때는 sink 구현이 전송별로 갈리므로 허브 안의 채널 추상이 충분한지 다시 본다.
  재는 법: 새 전송을 넣는 작업이 `stream_hub.rs` 를 고치는지 본다.

## References

- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-ipc/src/stream_hub.rs` 의 `StreamHub` ·
  `src/adapters/production/tcp_ipc_server.rs` 의 accept 스레드 · `src/core/bulk_transfer.rs`
  의 `ordered_batch_routes_to_intact_bytes`
- [`docs/architecture/index.md`](../architecture/index.md) — `tasty-ipc` 크레이트 서술
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — 스트림 허브의 동작
- [ADR-0089](0089-crate-split-follows-dependency-direction.md) — 크레이트 분리는 의존 방향을 따른다
- [ADR-0054](0054-remote-filesystem-native-over-attach-stream.md) — bulk 연결 분류
