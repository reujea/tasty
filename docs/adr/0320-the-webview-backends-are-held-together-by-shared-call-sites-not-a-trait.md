# ADR-0320: webview 백엔드 셋은 trait 이 아니라 공유 호출부가 묶는다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, webview, host-api, cross-platform, trait, cfg, contract
- **Group**: plugin-render

## Context

`PlatformWebView` 는 하나의 타입이 아니라 **셋**이다 — `linux.rs` · `macos.rs` ·
`windows.rs` 가 각각 정의하고, `src/host_api/webview.rs` 의 `cfg` 가 빌드마다 하나를
`pub use` 한다. 호출부는 어느 것이 골라졌는지 모른 채 이름으로 부른다. 공통 trait 은 없다.

이 모양을 보면 "trait 으로 묶어야 계약이 강제된다" 는 제안이 자연스럽게 나온다. 실제로
이 레포에서 그 물음이 문서로 답해진 적이 없어서, 다음 사람이 처음부터 재게 된다.

**먼저 재야 할 것은 지금 무엇이 강제되고 있는가다.** 실측 2026-09-20: 세 파일이 노출하는
`pub fn` 은 각각 **정확히 열둘**이고, 이름을 정렬해 비교하면 **세 집합이 완전히 같다**
(`diff` 출력 0 줄). 그 일치는 우연이 아니라 강제된 것이다 — 호출부가 셋 모두에 공유되므로
한 백엔드에 메서드가 없거나 시그니처가 다르면 **그 OS 의 컴파일이 깨진다**. 그 컴파일은
`crossplatform-check` 의 `check-macos` · `check-windows` · `check-headless` 세 잡이 main
push · PR 마다 돌린다.

즉 trait 이 추가로 살 수 있는 것은 **이름·시그니처 일치가 아니다.** 그것은 이미 강제된다.

## Decision

trait 을 도입하지 않는다. 세 백엔드는 지금처럼 같은 이름을 노출하고 `cfg` 가 하나를 고르며,
일치는 공유 호출부와 세 OS 컴파일이 강제한다. 대신 그 계약이 **무엇인지**를
[`docs/design/systems/webview.md`](../design/systems/webview.md) 가 한 자리에 적는다 —
표면 전량, 부모 handle 종류, 좌표 변환, 스레드 친화성, teardown, 탐색 상태 소유.

그 문서는 강제되지 않는 것도 이름으로 적는다: **뜻 수준의 일치**에는 채널이 없다.

## Consequences

- **얻은 것**: dyn 디스패치도 제네릭 전파도 없이 지금의 정적 디스패치를 유지한다. 한 번에
  한 백엔드만 컴파일되므로 trait 이 있어도 런타임 다형성은 쓰이지 않았을 것이다.
  계약이 문서 한 자리에 모여, 백엔드를 고칠 때 셋을 대조할 기준이 생겼다.
- **잃은 것**: 세 백엔드가 **뜻**으로 갈라지는 것을 잡는 것이 아무것도 없다. trait 이
  있어도 그건 마찬가지였겠지만, trait 이 있으면 최소한 doc 주석 한 자리가 셋을 내려다봤을
  것이다. 그 자리를 문서가 대신한다 — 문서는 컴파일러가 안 읽는다.
- **운영 비용 / 유지 부담**: 백엔드에 연산을 더할 때 세 파일을 모두 고쳐야 하고, 문서의
  표도 함께 움직여야 한다. 앞의 둘은 컴파일이 강제하고 뒤는 사람이 한다.

## Alternatives Considered

- **A: `WebViewBackend` trait 을 정의하고 셋이 구현한다** — 이름·시그니처 일치는 이미
  강제되므로 새로 얻는 것이 없고, `new` 가 `Result<Self, _>` 를 돌려주는 생성자라
  object-safe 하지 않아 trait 을 쪼개거나 생성을 밖으로 빼야 한다. 계약을 적는 자리를
  얻으려고 타입 구조를 바꾸는 값이 안 선다.
- **B: 세 백엔드의 표면이 같은지 검사하는 가드를 짓는다** — 소스를 파싱해 `pub fn` 이름
  집합 셋을 비교하는 것은 가능하다. 그러나 그 집합이 갈라지면 **컴파일이 이미 깨지므로**
  가드는 컴파일보다 먼저 울리지 못하고, 잡는 것도 같은 것이다. 뜻의 갈라짐은 이 가드로도
  못 잡는다.
- **C: 계약을 세 파일의 머리 주석에 각각 적는다** — 셋이 서로 다르게 낡는다. 한 자리에
  적고 세 파일이 그 자리를 가리키는 쪽을 골랐다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 한 빌드에서 **둘 이상의** 백엔드가 동시에 컴파일돼야 하게 되면(런타임 백엔드 선택, 원격
  백엔드 추가 등) 정적 디스패치 전제가 깨지고 trait 이 필요해진다.
- 네 번째 백엔드가 생기면 다시 본다. 셋일 때는 공유 호출부가 실용적인 강제였지만, 늘어날수록
  "어느 OS 에서 깨지는가" 를 기다리는 비용이 커진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 세 백엔드가 뜻으로 갈라졌는가. 재는 법: 한 연산에 대해 세 파일의 해당 메서드 본문을 모두
  열어 대조한다. 이름 집합의 일치는 다음으로 잰다 —
  `for f in linux macos windows; do grep -oE '^    pub fn [a-z_]+' src/host_api/webview/$f.rs | awk '{print $3}' | sort; done`
  — 세 출력이 같아야 한다. 이 명령은 **이름만** 답하고 뜻은 답하지 않는다.

## References

- 계약 본문: [webview](../design/systems/webview.md)
- 키보드 계약의 결정: [ADR-0102](0102-webview-key-forwarding.md)
- 반대 방향(백엔드 → 호스트) 접점의 trait — 이 결정과 별개: [ADR-0385](0385-webview-backends-receive-their-host-contract-by-injection.md)
- 생성 실패 분류의 결정: [ADR-0159](0159-a-null-gdk-window-is-a-value-not-a-crash.md)
- teardown 순서의 결정: [ADR-0248](0248-webview-teardown-lets-gdk-finish-before-the-x-window-is-destroyed.md)
- 코드 근거(결정이 실현된 **현재 위치**): `src/host_api/webview.rs` 의 `cfg` 재수출 셋과
  각 백엔드의 `PlatformWebView`
