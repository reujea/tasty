# ADR-0318: 핸들러 레지스트리는 크레이트로 내려가고, 번들 기본값은 상대 경로가 아니라 크레이트 상수가 된다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, crates, layering, file-handler, include-str, plugin-protocol, orphan-rule, adr-0308
- **Group**: architecture

## Context

본 바이너리 안의 파일 모듈 아래에 파일 핸들러의 **정책과 레지스트리**가 있었다 — detector→handler
매핑, 사용자 오버라이드, plugin 기여, 최근 선택, 그리고 그 셋을 patch semantics 로 병합하는
규칙. 호스트·GUI·IPC 를 한 줄도 부르지 않는데 본 바이너리 안에 있었고, 그래서 바이너리가
바이너리와 무관한 도메인 로직을 소유했다.

한 가지가 잘못 세어지기 쉬웠다. 이 모듈은 `crate::poison::` 을 쓰는데 그것은 **GUI 가 아니다**
— `src/lib.rs` 의 `pub(crate) use tasty_utils::poison;` 재수출이다. 의존으로 세면 잎
크레이트 하나다.

번들 기본값 `default-file-handlers.toml` 은 **두 자리에서** 상대 경로로
`include_str!` 되고 있었다 — 모듈 자신과 `src/core/state.rs`. 상대 경로는 파일이 움직이는
순간 자리마다 따로 깨지고, 한쪽만 고쳐진 상태가 컴파일된다.

## Decision

핸들러 정책·레지스트리를 `crates/tasty-file-handler` 로 내리고, 본체는 `src/file.rs` 에서
`pub use tasty_file_handler as handler;` 로 이름만 이어 `crate::file::handler::…` 호출부를
건드리지 않는다. 번들 기본값은 크레이트 안으로 함께 옮기고 `pub const HOST_DEFAULTS_TOML`
로 **한 번만** 박아, 소비처가 경로가 아니라 이름을 부르게 한다 — 자매 크레이트
`tasty-file-format` 이 이미 쓰는 모양이다.

새 크레이트의 `tasty-plugin-protocol` 의존은 [ADR-0308](0308-the-format-registry-port-impl-stays-with-the-type.md) 의
**같은 결정을 적용한 것이지 새 결정이 아니다** — plugin 이 레지스트리를 조회하는 port
trait 이 wire 크레이트에 살고, 고아 규칙이 impl 을 타입 소유자 쪽으로 강제한다. layer
예외 명부와 아키텍처 문서의 도메인-IO 절에 그 형태로 등록한다.

### ADR-0308 의 재검토 트리거를 여기서 소진한다 (착지 직후 보강)

위 문장만으로는 부족하다. ADR-0308 의 재검토 조건은 "같은 절의 예외가 **셋 이상**이 된다"
이고 좌변은 "`EXCEPTIONS` 중 도메인-IO 절 소속 항목 수" 다. 이 결정이 셋째를 더하므로
**그 트리거는 이 ADR 과 함께 발화한다.** 그 처방은 "절 경계 자체를 다시 그어야 한다" 이고,
"개별 예외인가 절 경계 문제인가" 는 바로 위 문장이 단정한 명제다 — 단정으로 대신할 수
없으므로 여기서 실제로 본다.

**절 경계를 다시 그을 수 있는가 — 실측 2026-09-20: 그릴 수 있고, 그래프 비용은 0 이다.**
두 예외의 대상인 `tasty-plugin-protocol` 의 워크스페이스 내부 의존은 `tasty-type-appearance`
**하나뿐**이고 그것은 type-\* 절이다. 소비자는 전부 도메인-IO 이상이다(`tasty-file-format` ·
`tasty-file-handler` · `tasty-ipc` · `tasty-host-plugin` · `tasty-cli` · `tasty-plugin-sdk` ·
`tasty-plugin-manifest` · 번들 plugin 넷). 그러므로 이 크레이트를 도메인-IO **아래**
(type-\* / primitive) 로 옮기면 들어오는 간선은 전부 아래로 향하고 자기 간선은 절 안에
머문다 — **새로 생기는 역전 0, 사라지는 예외 둘.** 남는 것은 뿌리가 다른
`tasty-remote` → `tasty-ipc`(ADR-0089) 하나다.

**그런데도 안 옮긴다.** "plugin protocol / SDK" 는 층이 아니라 **경계 진술**이고
(`docs/architecture/index.md` 의 그 절 — "이 계층은 도메인-IO 에 직접 의존하지 않는다
(sandbox 경계) — protocol/sdk 만 통과"), 그 진술의 **주어가 바로 이 크레이트**다. 옮기면
경계를 정의하는 wire 크레이트가 그 경계를 적는 절 밖에 놓이고, 진술은 SDK 만 남긴 채
읽힌다. 같은 저울을 아키텍처 문서가 `tasty-shm` 자리에서 이미 한 번 달았고 결론도 같은
방향이었다 — **경계 진술에 안 적힌 예외를 두는 대가가 가장 크다.** 여기서는 예외 둘이
명부와 본문 양쪽에 이름과 이유로 적혀 있으므로, 적힌 예외 둘과 안 적힌 예외 하나를 맞바꾸는
거래가 된다.

그래서 이 ADR 의 결정은 둘이다. ① 절 경계를 다시 그리지 않는다. ② **문턱을 셋에서 넷으로
옮기는 것을 이 ADR 이 명시적으로 결정한다** — 아래 재검토 조건이 0308 과 **같은 좌변**을
쓰되 발화점만 다르다. 값이 3 인 지금 두 술어의 답이 갈리지 않게 하려는 것이고, 문턱이 조용히
움직였다는 인상을 남기지 않으려는 것이다.

## Consequences

- **얻은 것**: 도메인 로직이 바이너리 밖으로 나왔고, GUI 없이 링크된다. 번들 기본값이
  한 자리에만 있으므로 파일이 다시 움직여도 자리마다 따로 깨지지 않는다.
- **잃은 것**: layer 예외가 둘에서 셋이 됐다. 그 셋이 전부 같은 뿌리(고아 규칙)라는 것이
  문서에 적혀야 값이 유지된다 — 안 적으면 다음 사람이 세 번 같은 조사를 한다.
- **운영 비용 / 유지 부담**: 크레이트 수 lockstep 아홉 자리가 함께 움직인다. 그 자리는
  README 배지 둘 · README 본문 넷 · 아키텍처 문서 둘 · `CLAUDE.md` 빌드 절 하나다.

## Alternatives Considered

- **A: port trait impl 을 `tasty-plugin-protocol` 쪽으로 옮겨 예외를 없앤다** — 고아 규칙이
  막는다. 타입을 소유하지 않은 크레이트에 impl 을 둘 수 없다. ADR-0308 이 같은 자리에서
  같은 결론을 냈다.
- **B: 번들 기본값을 크레이트 밖에 두고 양쪽이 상대 경로로 계속 박는다** — 지금 동작하지만
  이번 이동이 그 경로를 크레이트 밖으로 밀어낸다. 같은 함정을 한 칸 옮기는 것뿐이다.
- **C: 핸들러와 dispatch 를 함께 내린다** — dispatch 는 파일을 실제로 여는 실행 경로이고
  호스트 동작에 닿는다. 정책만 내리는 경계가 "GUI 없이 쓸 수 있는가" 와 일치한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 도메인-IO 절의 예외가 **넷째**로 늘어나면, 개별 예외가 아니라 port trait 의 배치 자체를
  다시 본다. 좌변은 ADR-0308 과 **같다** — `architecture_layer_order_holds.rs` 의
  `EXCEPTIONS` 중 **도메인-IO 절 소속 항목 수**다(전체 길이가 아니다. 지금은 두 수가 같지만
  다른 절에 예외가 생기면 갈린다). 발화점만 셋이 아니라 넷이고, 그 이동은 위 Decision 이
  근거와 함께 명시한다.
- 그 넷째의 **뿌리가 고아 규칙이 아니면** 수와 무관하게 즉시 다시 본다. 지금 셋을 묶어 주는
  것은 수가 작다는 사실이 아니라 **둘이 같은 뿌리**라는 사실이고, 뿌리가 갈리는 순간 위
  Decision 의 저울이 성립하지 않는다.
- `HOST_DEFAULTS_TOML` 을 부르지 않는 새 `include_str!` 이 같은 파일을 가리키면 이
  결정이 무너진 것이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 이 크레이트가 정말 호스트 결합 0 인가. 좌변은 **의존 폐포**다 — 직계 의존 목록은 이
  물음에 답하지 못한다(직계가 자기 기본 feature 로 GUI 를 끌어오면 이름이 목록에 안 보인다.
  자매 크레이트에서 실제로 그렇게 새어 들어온 적이 있다: [ADR-0324](0324-link-detection-and-link-opening-split-by-side-effect.md)).
  재는 법은 자매 ADR 과 **같은 술어**를 쓴다:

  ```bash
  cargo tree -p tasty-file-handler --edges normal --prefix none | sort -u \
    | grep -icE '^(egui|winit|wgpu|eframe|glow|epaint|ecolor|emath|tasty-ui-|tasty-egui|tasty-font|tasty-icons)'
  ```

  실측 2026-09-20: 이 크레이트 **0**, 같은 술어를 본 바이너리 폐포에 걸면 **28** — 0 만 내는
  술어는 증거가 아니므로 판별력을 그 대조로 보인다. 컴파일이 통과하는 것은 이 물음에 답하지
  않는다.

## References

- 같은 형태의 선행 결정, 그리고 이 ADR 이 **재검토 트리거를 소진한** 상대:
  [ADR-0308](0308-the-format-registry-port-impl-stays-with-the-type.md)
- 크레이트 분할이 의존 방향을 따른다는 결정: [ADR-0089](0089-crate-split-follows-dependency-direction.md)
- 코드 근거(결정이 실현된 **현재 위치**): `tasty-file-handler` 크레이트의
  `HOST_DEFAULTS_TOML` 과 `FileHandlerRegistry`, 본체 쪽 이름 잇기는 `src/file.rs` 의
  `handler` 재수출
- 절 소속·크레이트 수: [architecture](../architecture/index.md)
