# ADR-0031: Lua 스크립트의 tasty 접근은 고정 호스트 API 표면으로만 — state 직접 접근 불가 + 워커 스레드 격리

- **Status**: Accepted
- **Date**: 2026-07-01
- **Tags**: lua, scripting, host-api, worker-thread, snapshot, command-queue, capability-boundary, sandbox, init-lua-removal, observe-only, adr-0009, adr-0028
- **Group**: plugin-system

## Context

현재 `crates/tasty-lua`(별칭 `ln`, `app.lua_engine`)는 다음 모델이다.

- **init.lua 부팅 자동로드.** `~/.tasty/init.lua` 1개를 부팅 시 실행해 그 안의 `tasty.on(event, cb)` 등록 코드를 돌린다(`crates/tasty-lua/src/engine.rs:92-106` `load_init`). Lua 를 쓰는 등록 경로가 이것뿐이다.
- **observe-only 훅.** 콜백은 이벤트를 보고(`fire`, `engine.rs:123-152`) 외부 동작만 한다. 반환값으로 tasty 흐름을 못 바꾼다(`docs/features/lua-hooks/index.md#설계-경계`).
- **호스트 API = `log`/`warn`/`run_cli`**(`crates/tasty-lua/src/host_api.rs`). `run_cli` 는 tasty 자기 CLI 를 **프로세스로 spawn** 해 간접 조작한다.
- **단일 VM, 메인 스레드.** VM 1 개를 메인 스레드 1 군데서만 호출한다.
- **샌드박스는 메모리 캡뿐.** `sandbox.rs` 는 32MB 메모리 한계 + `debug`/`load*`/`loadlib` 제거만 한다. `lib.rs:14` 주석은 "instruction cap" 을 주장하지만 **실제 구현이 없다**(무한 루프 미보호).

여기에 새 요구가 겹친다 — 사용자가 **단축키에 스크립트를 걸어 tasty 를 CRUD 조작**(1차: 트리 구조 조회)하고 싶다. 이때 두 축이 문제가 된다.

1. **능력 경계.** `run_cli` 간접 조작은 "무엇을 할 수 있는지" 가 CLI 표면 전체로 암묵 노출돼 경계가 흐리고, 매 호출 프로세스 spawn + 인자 직렬화 비용을 낸다. tasty 내부를 in-process 로 직접 조작하려면 "어디까지 노출하는가" 를 명시적으로 그어야 한다.
2. **스레드 안전.** 무거운/무한 스크립트가 메인 이벤트 루프를 막으면 UI 가 얼어붙는다(현재 메인 스레드 단일 VM). 이를 워커 스레드로 빼면, tasty 의 트리/surface/pane state 는 메인 스레드 소유이므로 워커가 그것을 직접 만질 수 없다.

## Decision

**Lua 가 tasty 를 건드리는 유일한 통로는 호스트가 명시적으로 등록한 _고정 API 표면_ 이다. Lua 는 tasty 의 내부 state 를 직접 참조·변형할 수 없고, 열거(enumerated)된 API 외의 어떤 능력도 tasty 에 대해 노출하지 않는다 — 생성·조회·수정·삭제(CRUD) 전부 이 API 로만. 그리고 Lua 엔진은 전용 워커 스레드에서 실행되며, 이 API 경계가 곧 유일한 마샬링 채널이다: 읽기는 메인 스레드가 발행한 불변 스냅샷을 읽고, 쓰기는 커맨드로 직렬화해 메인 스레드 큐로 보낸다. 워커는 메인 스레드 소유 state 를 절대 직접 만지지 않는다.**

이 방향은 사용자가 확정했다. 구현 착수와 함께 Accepted 로 전환한다.

세부 결정:

- **API 표면 = 열거된 것만.** host 가 Lua 전역에 등록하는 함수만 호출 가능하다. 첫 API 는 **트리 구조 조회(read) 하나**(`tasty list tree` 가 반환하는 구조를 in-process 로 반환). 이후 CRUD 는 이 표면에 명시 등록으로만 늘린다 — "CLI 전체가 공짜로 열리는" `run_cli` 식 암묵 노출과 반대다.
- **읽기 = 스냅샷.** 메인 스레드가 프레임 안전지점에서 읽기전용 트리 스냅샷을 발행하고, 워커의 read API 가 그 스냅샷을 읽는다. Lua 는 살아있는 state 핸들을 절대 쥐지 않는다.
- **쓰기 = 커맨드 큐.** mutation API 는 즉시 state 를 바꾸지 않고 커맨드로 직렬화해 메인 스레드 큐에 넣는다. 메인이 안전지점에서 적용한다.
- **워커 = 단일 스레드, 직렬 실행.** 스크립트는 워커 스레드 하나에서 직렬로 돈다(동시 실행 없음). 무한 루프/시간 초과는 주기적 VM 훅 + deadline 체크로 abort 한다(현재 미구현인 instruction cap 을 여기서 실체화). 구현상 `set_interrupt` 는 Luau 전용이라 Lua 5.4(`lua54`)에서는 instruction-count `set_hook`(every_nth_instruction) 로 동일 메커니즘을 구성한다.
- **init.lua 폐기.** 부팅 자동로드(`load_init`)와 그 재로드 IPC(`script.reload`)를 제거한다. 스크립트는 명시적 트리거(1차: 단축키, 설정 modal 에서 스크립트↔단축키 등록)로만 실행되고, 이후 도입할 이벤트-트리거 자동실행도 부팅 시 임의 Lua 실행이 아니라 **등록 목록(메타데이터) 로부터 배선**한다.
- **신뢰 모델은 유지.** 사용자가 자기 머신에서 자기 권한으로 짜는 스크립트이므로 `io`/`os.execute` 는 계속 안 막는다. 능력 제한의 목적은 plugin 식 샌드박스가 아니라 **① tasty 내부에 대한 접근을 API 로 좁히는 것 + ② 워커/메인 스레드 안전 보장** 이다. plugin(별 프로세스·권한 게이트, ADR-0009)과는 신뢰 카테고리가 다르다.
- **observe-only 와의 관계.** 이벤트 훅 콜백의 반환값을 무시하는 observe-only 는 유지된다. 본 ADR 이 추가하는 것은 반환값 기반 흐름 개입이 아니라 **명시 API 호출을 통한 active CRUD** 라는 직교 채널이다 — 흐름 소유권은 여전히 호스트에 있다(커맨드를 적용할지·언제 적용할지는 메인이 결정).

## Consequences

- **얻은 것**:
  - **스레드 안전** — 워커가 메인 state 를 물리적으로 못 만지므로(스냅샷 read / 커맨드 write 만) 데이터 레이스가 구조적으로 불가능하다.
  - **능력 경계 명확** — Lua 가 tasty 에 할 수 있는 일이 열거된 API 로 정확히 한정된다. 감사·문서화·권한 판단이 표면 하나로 수렴한다.
  - **UI 응답성** — 무겁거나 폭주하는 스크립트가 렌더/이벤트 루프를 막지 않는다.
  - **프로세스 비용 제거** — in-process 직접 호출이라 `run_cli` 의 spawn + 인자 직렬화 왕복이 사라진다.
  - **"자동 실행 금지" 정합** — init.lua 폐기로 부팅 시 임의 Lua 실행 경로가 없어진다.
- **잃은 것**:
  - **API 를 하나하나 설계·유지해야 함** — `run_cli` 처럼 "CLI 전체 공짜" 가 아니라, 노출할 CRUD 마다 명시 등록이 필요하다.
  - **인프라 신설** — 읽기 스냅샷 발행 + 쓰기 커맨드 큐 + 워커 스레드 + 크로스스레드 채널을 새로 깔아야 한다.
  - **스냅샷 최신성 지연** — 워커가 읽는 트리는 프레임 경계 스냅샷이라 실시간이 아니다.
- **운영 비용 / 유지 부담**:
  - 새 mutation API 마다 API 시그니처 + 커맨드 variant + 메인 적용 지점 3 중 배선.
  - `set_interrupt` deadline 튜닝(정상 스크립트 오탐 abort 방지).
  - init.lua 폐기에 따른 문서 정리 — `docs/features/lua-hooks/index.md#설계-경계`(observe-only/init.lua 서술), `docs/features/lua-hooks/`, `docs/features/lua-hooks/index.md#구현--발화-site--payload`, `docs/reference/api.md`(`script.reload`).

## Alternatives Considered

- **A. `run_cli` 로만(직접 API 미도입)**: 새 API 를 안 만들고 CLI 를 spawn 해 조작. 프로세스 비용 + 능력 경계가 CLI 전체로 암묵 노출 + 매 호출 직렬화 → **기각**(사용자가 in-process 직접 API 로 확정).
- **B. 메인 스레드에서 Lua 실행**: 크로스스레드 마샬링이 불필요해 단순하다. 그러나 무거운/무한 스크립트가 UI 를 프리즈시킨다. `set_interrupt` 로 무한 루프는 끊어도 정상 장시간 작업은 여전히 루프를 막는다 → **기각**(워커 정식 채택).
- **C. Lua 에 state 핸들 직접 노출(mlua userdata 로 tasty state 래핑)**: 표현력은 최대. 그러나 스레드 안전을 파괴하고(워커가 메인 state 참조), 능력 경계가 소멸하며, 소유권/수명 관리가 지옥이 된다 → **기각**.
- **D. init.lua 유지(관리 UI 와 공존)**: 기존 부팅 자동로드를 두고 목록 UI 를 추가. 부팅 자동실행이 "자동 실행 금지" 방침과 충돌하고 등록 경로가 이중화된다 → **기각**(목록 일원화).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `set_interrupt` deadline 실측이 정상 스크립트를 유의미하게 오탐 abort 한다(→ cap 전략 재고).
- 스냅샷 발행 비용이 프레임 예산을 침해한다(→ 증분 스냅샷 / lazy 발행 재고).
- 노출 API 표면이 과도하게 커져 열거 유지가 부담이 된다(→ 카테고리/네임스페이스 기반 노출 재고).
- mutation 커맨드 큐 라운드트립 지연이 사용자 체감 임계를 넘는다(→ 동기 경로 예외 설계).
- plugin 측 Lua 요구가 발생한다(현재 호스트 전용, ADR-0009 와 함께 재검토).

## References

- 코드 근거: `crates/tasty-lua/src/{engine.rs:92-106,123-152, host_api.rs:36-69, sandbox.rs:11-46}`; `src/app.rs`(`lua_engine`); `src/boot.rs:166,255`; `src/app/window_lifecycle.rs:302`; `src/app/event_handler.rs:790`; `src/app/dispatch/{surface_lifecycle.rs:28, host_events.rs:39}`; `src/app/ipc/app_methods.rs:33`(`script.reload`).
- 관련 문서: [features/lua-hooks 설계 경계](../features/lua-hooks/index.md#설계-경계)(observe-only/init.lua — 본 ADR 로 init.lua·자동로드 부분 supersede), [features/lua-hooks](../features/lua-hooks/index.md), [features/lua-hooks 구현](../features/lua-hooks/index.md#구현--발화-site--payload), [reference/api](../reference/api.md).
- 관련 ADR: [0009](0009-plugin-sandbox-deferred.md)(plugin sandbox 보류 — plugin 은 별 프로세스, Lua 는 in-process 워커라 신뢰 카테고리 상이), [0028](0028-plugin-egui-mesh-render-channel.md)(plugin 렌더 채널 — plugin in-process dylib 기각 근거와 대비).
