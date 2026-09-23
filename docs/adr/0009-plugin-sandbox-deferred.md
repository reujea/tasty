# ADR-0009: Plugin sandbox 는 보류 — OS-level opt-in 을 우선 후보로

- **Status**: Deferred
- **Date**: 2026-06-17
- **Tags**: plugin, sandbox, security, wasm, seccomp, trust-boundary, deferred
- **Group**: plugin-system

## Context

tasty plugin 의 권한 모델은 **IPC method 단위 게이트** 다([plugin-permissions](../dev-guide/plugin-permissions.md)). 매니페스트 `permissions[]` 는 *호스트 API 호출 권한* 이지 *OS 자원 시스템 권한* 이 아니다 — plugin 이 자기 프로세스에서 `std::fs::*`/`std::net::*`/`Command::new` 를 직접 호출하는 것은 OS process privilege 로만 결정되고 호스트는 알 수 없다. `tasty-host-plugin` 의 plugin spawn 은 sandbox wrapper(seccomp/sandbox-exec/Job Object) 없이 OS process 로 직접 띄운다.

현재 번들 plugin 8 종은 모두 first-party 이고 외부 plugin 0 개라 적대적 plugin 시나리오가 현실화되지 않았다. 즉 sandbox 가 *지금* 필요한 정도는 낮다.

## Decision

**0.x 동안 plugin sandbox 를 도입하지 않는다.** 현 상태(process, sandbox 0)를 유지하고, 권한 모델의 한계를 false security 보다 투명하게 명시한다. 도입이 필요해지면 **OS-level sandbox 를 opt-in(매니페스트 토글)으로 우선** 검토한다 — WASM 은 그 다음 단계.

## Consequences

- **얻은 것**: plugin 도구체인·빌드·디버깅 인프라 무변경. FFI 부담 0(native process 유지).
- **잃은 것**: plugin 의 OS 자원 직접 접근을 막을 수단 없음 — first-party 신뢰에 의존.
- **운영 비용**: 도입 시점으로 미룸. 본 ADR 이 그 trade-off 의 기록.

## Alternatives Considered

- **WASM (wasmtime, WASI Preview 2)** — capability-based 강제 sandbox. POC(`tasty-plugin-sdk-wasm`, workspace-exclude, clipboard-history 변환)에서 측정: cold-start ~115–142ms(process 와 유사/우월), host_call ~3–7µs(process TCP+JSON 보다 우월), wasi-p2 capability injection 으로 격리 강제. **그러나** linear-memory 복사 FFI(현 `tasty-shm` 공유메모리 모델과 충돌), wasm32 debug 도구체인 재설계, WASI preopen 의 임의경로(explorer/git-viewer) 비호환, 동적 권한 prefix(`file_handler.extend:<id>`)→정적 capability 매핑 비용이 1.0 전 부담 한도 초과. POC 측정으로 *기술적 viable* 은 확인됐으나 정식 채택은 후속 spike(sandbox 실증 + RSS + typed-record marshal) 후 의사결정으로 미룸.
- **OS-level sandbox**(macOS sandbox-exec/Seatbelt · Linux seccomp+landlock · Windows AppContainer) — FFI 부담 0, 기존 plugin 코드 무변경, opt-in 토글 가능. 도입 시 1순위. 비용: 플랫폼별 분기 + profile/syscall 매핑 테이블 + plugin 작성자 호환 검증 절차.
- **현 상태 유지(채택)** — 외부 plugin 0 시점에 sandbox 비용 대비 가치 음수.

매니페스트 `sandbox = "process"|"os-strict"|"wasm"` enum 은 *제안만* — 기본값 `"process"`, 정식 도입은 `manifest_version` bump 시점.

## Reconsideration Triggers

- 외부에서 WASM/비-Rust plugin 작성 요청 2건 이상
- 권한 게이트 한계에서 비롯한 보안 이슈 1건
- 첫 외부 plugin 출시 후 1년 운영 데이터
- **marketplace 도입**(비-trusted plugin 일상화) — [ADR-0010](0010-plugin-marketplace-deferred.md) 과 상호 trigger. marketplace ↔ sandbox 는 묶음 의사결정(부분 도입은 false security 위험).

## References

- [dev-guide/plugin-packaging 생태계 정책](../dev-guide/plugin-packaging.md#생태계-정책--자동-upgrade--호환성-분류) §정책(WASM 보류) · [plugin-permissions](../dev-guide/plugin-permissions.md) 「한계」
- [ADR-0010](0010-plugin-marketplace-deferred.md) — marketplace 보류(연동)
- 코드: `crates/tasty-host-plugin/src/process.rs`(spawn) · `crates/tasty-plugin-manifest/src/types.rs`(`Permission`) · `crates/tasty-plugin-sdk-wasm/`(WASM POC, workspace exclude)
