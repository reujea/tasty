# ADR-0179: 해소하는 crate 에 표를 넘긴다 — 주입하는 것은 함수가 아니라 데이터다

- **Status**: Accepted
- **Date**: 2026-09-06
- **Tags**: plugins, ipc, derived-state, encapsulation, global-state, adr-0173, adr-0178
- **Group**: plugin-system

## Context

`tasty-ipc::method_meta::method_meta()` 는 IPC 메서드 이름을 받아 "이것을 어떻게 다뤄야
하나"(권한·plugin 호출 가능 여부)를 답한다. 마지막 단계에서 `<prefix>.<method>` 의
prefix 가 **어느 plugin 의 것인지** 물어야 하는데, 그 소유 표(`IpcNamespaceRegistry`)는
위층인 `tasty-host-plugin` 의 `PluginManager` 가 들고 있었다. `tasty-ipc` 는 그 crate 를
의존할 수 없다(의존 방향).

그래서 아래층이 **사본**을 들었다: `static PLUGIN_PREFIXES` 미러와, 위층이 그것을
채우라고 열어 둔 `pub` 쓰기 함수 셋(`register_plugin_prefix` ·
`unregister_plugin_prefix` · `doc(hidden) pub clear_plugin_prefixes_for_tests`).

표가 둘이면 **한쪽만 갱신하는 결함**이 생긴다. 실제로 났다: `plugin.remove` 가 설치
목록만 손으로 지워 제거된 plugin 의 prefix 가 소유 표에 남았고, 그 이름의 호출이
`-32002 plugin '<id>' is not running` 으로 거절됐다 — 설치조차 안 돼 있는데. 호스트가
같은 이름에 구현을 가진 메서드(`image.list`·`image.open`·`markdown.navigate`)는 그
상태에서 통째로 가려졌다.

그때의 처방은 텍스트 가드였다. 그러나 가드는 "누가 쓰는가" 는 보지만 **"언제 쓰는가"
(순서)** 는 못 본다. 그리고 이 항목만은 구조로 닫을 수가 없었다 — 쓰기 함수가 다른
crate 에서 불려야 해서 `pub` 이어야 하고, **러스트에는 "이 crate 에만 공개" 가 없다.**

## Decision

**닫는 방법은 가시성이 아니라 사본을 없애는 것이다.** 소유 표의 타입을 해소하는
crate(`tasty-ipc`)로 내리고, `PluginManager` 가 든 표의 `Arc` 를 부팅 때 그대로
넘긴다(`install_namespace_table`, 1 회, 덮어쓰지 않음). 표가 하나면 "두 표가 어긋난다"
는 결함이 존재할 자리가 없고, 미러 쓰기 함수 셋이 함께 사라진다.

**그리고 주입하는 것은 함수가 아니라 데이터다.** 아래층이 위층의 상태를 보게 하는 데는
두 형태가 있다 — 조회 **클로저**를 주입하거나, 표 **핸들**을 주입하거나. 클로저를
주입하면 `method_meta()` 안에서 host 코드가 돌고, 그것이 유도 자리(`&mut self`)와 겹칠
수 있다(재진입). 핸들을 주입하면 `method_meta()` 안에서 도는 host 코드가 **없다.**

> **재진입은 소유 이전의 성질이 아니라 콜백의 성질이었다.**

유도도 같은 원리로 바꿨다: 차분(낡은 것을 골라 지우고 새 것을 더하기) 대신 **락 밖에서
표를 통째로 계산하고 한 번 대입**한다. 임계구역이 대입 한 줄이면 그 안에서 도는 코드가
없고, "어디서 왔는지 모르는 항목" 이 남을 수 없다 — 그것이 위 결함의 형태였다.

## Consequences

- **얻은 것**: 미러가 없으므로 동조 결함이 불가능하다. `pub` 쓰기 함수 셋과 테스트 전용
  전역 변형자(`doc(hidden) pub`)가 운영 표면에서 사라졌다. 유도가 "계산 후 대입" 이라
  재진입이 구조적으로 불가능하다. `tasty-host-plugin` 의 테스트는 각자 자기 매니저의
  표만 만지므로 그 crate 의 직렬화 락이 필요 없어졌다 — **락을 지운 것이 아니라 경합을
  없앤 것이다.**
- **잃은 것**: 표가 여전히 프로세스 전역으로 **설치**된다. 전역 자체가 없어진 것은
  아니다(그러려면 인자로 넘겨야 하는데 — 아래 대안 B). 그리고 읽기 창구의 모양이
  바뀌었다: 표가 락 뒤에 있어 소유자 `&str` 을 빌려 나올 수 없으므로 물음을 둘로
  쪼갰다(`owns_namespace` · `namespace_belongs_to_other`).
- **운영 비용 / 유지 부담**: 부팅 경로에 설치 한 줄이 는다. 그 자리는
  [ADR-0178](0178-a-job-whose-need-is-independent-of-the-trigger-is-anchored-at-boot.md)
  의 명부(`src/source_guards/jobs_anchored_at_boot.rs`)에 조합별로 등록돼 있어, 한
  조합에만 있으면 가드가 잡는다. 설치가 빠지면 plugin namespace 메서드가 권한 검사에서
  "모르는 메서드" 가 되므로 **조용한 실패가 아니다**.

## Alternatives Considered

- **A — 조회 클로저(resolver)를 주입한다**: 아래층이 `Fn(&str) -> …` 하나를 부팅 때
  받는다. 미러는 같이 사라진다. 안 고른 이유는 **재진입**이다 — `method_meta()` 안에서
  host 코드가 돌고, 유도 자리는 `&mut self` 안에서 표를 쓴다. 클로저가 원본을 직접 읽게
  하려면 weak 참조와 그 수명 관리가 따라오고, `Arc<Mutex<…>>` 를 잡으면 소유가 다시
  둘이 된다. **문제를 옮길 뿐 없애지 않는다.**
- **B — 전역을 없애고 인자로 넘긴다**: `method_meta(method, &dyn NamespaceOwners)`.
  유일하게 전역까지 없앤다. 실측 fan-out 도 작았다 — `CallerContext::ensure_allowed`
  외부 소비자 5, `JsonRpcResponse::unrouted_for_external_caller` 외부 1, 여섯 자리 전부
  이미 `plugin_manager` 를 들고 있다. 안 고른 이유는 **자리 수 밖의 무게**다: 바뀌는 것이
  권한 검사의 시그니처라, 그 뒤로는 **정책의 출처를 호출자가 고르게 된다.** 타입은 맞는데
  의미가 틀린 표를 넘길 수 있고, 위험은 오늘의 여섯이 아니라 **내일의 일곱 번째**다.
- **C — 미러를 두고 텍스트 가드로 지킨다(현행 유지)**: 비용 0. 안 고른 이유는 그 대가가
  **이미 현실화됐기** 때문이다(위 `-32002`). 가드는 "누가 쓰는가" 만 보고 순서를 못 본다.

## 경계 — 이 형태가 안 맞는 자리

이 ADR 은 "아래층이 위층의 상태를 봐야 할 때 사본 대신 핸들을 넘긴다" 를 말한다. 다음
자리에는 그대로 적용되지 않는다.

- **한 프로세스에 소유자가 둘 이상일 수 있는 상태.** 여기서는 `PluginManager` 가
  프로세스에 하나라 설치도 하나면 됐다(그래서 설치는 덮어쓰지 않는다). 소유자가 여럿이면
  "어느 표인가" 가 호출 시점에 달리게 되므로 이 형태를 그대로 쓰면 안 된다.
- **읽기 임계구역 안에서 위층 코드가 돌아야 하는 상태.** 핸들 주입이 재진입을 없애는
  근거는 "아래층이 자료구조만 읽는다" 이다. 조회가 계산을 요구하면 그 계산이 다시 위층에
  들어가고, 그때는 대안 A 의 문제가 그대로 돌아온다.
- **유도가 차분이어야 하는 상태.** "락 밖에서 통째로 계산" 은 재료가 작고 재계산이 싼
  경우의 처방이다. 재계산이 비싸면 차분이 필요하고, 그러면 임계구역이 다시 넓어진다.

그 밖에 이 형태가 깨지는 자리는 **찾지 않았다.** 없다는 뜻이 아니다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 한 프로세스에 `PluginManager` 가 둘 이상 필요해진다(설치가 1 회라는 전제가 깨진다).
- `method_meta()` 의 해소가 표 조회를 넘어 **계산**을 요구하게 된다(재진입이 돌아온다).
- 권한 검사의 인자에 정책 출처를 실어야 할 다른 이유가 생긴다 — 그때는 대안 B 를
  다시 재되, "내일의 일곱 번째" 를 닫는 수단(인자 타입을 좁히거나 넘기는 자리를 하나로
  모으는 것)을 함께 정한다.
- 소유 표가 `packages` 말고 다른 재료를 갖게 된다(유도의 정의가 바뀐다).

## References

- [ADR-0173](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md) — 소유 표의 재료가 설치된 매니페스트라는 결정. 이 ADR 은 그 표를 **어디에 두는가** 를 정한다.
- [ADR-0178](0178-a-job-whose-need-is-independent-of-the-trigger-is-anchored-at-boot.md) — 설치를 부팅에 거는 근거와 그 명부.
- `docs/architecture/boot-sequence.md` — 부팅에 걸린 일의 조합별 자리.
- `src/source_guards/derived_plugin_tables_are_not_bypassed.rs` — 유도 상태를 우회하는 자리를 보는 가드(미러가 사라지며 항목 하나가 빠졌다).
- `crates/tasty-host-plugin/src/manager/tests_namespace_table.rs` — 유도와 신선도 단정의 테스트.
