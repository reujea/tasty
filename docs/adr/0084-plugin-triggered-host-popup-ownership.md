# ADR-0084: plugin 이 트리거한 host popup 은 자진 신고한 부모 instance 로 스택을 이룬다

- **Status**: Accepted
- **Date**: 2026-08-24
- **Tags**: popup, plugin, ipc, lifecycle, ownership
- **Group**: ui-theme-gallery

## Context

[ADR-0058](0058-plugin-triggered-host-popup-async-ack-push.md)가 세운 `file_picker.trigger` 는 plugin 이 host 소유 popup 을 열고 결과를 이벤트로 돌려받는 일반 메커니즘이다. 그 ADR 은 ack/result **배관**만 정의했고 두 popup 사이의 z-order·dismiss·수명 상호작용은 다루지 않았다.

그 결과 화면상 명백한 부모-자식(자식이 부모의 입력 필드를 채우러 열림)인 두 popup 이 시스템에서는 아무 관계 없는 형제로 취급된다. 관측된 귀결:

- 두 popup **모두의 바깥**을 클릭하면 부모(plugin popup)만 닫히고 자식(host `file_picker`)이 고아로 남는다. `file_picker` 는 `close_on_outside_click: false` 라 스크림 클릭으로 닫히지 않고, host 는 부모가 죽은 사실을 모르므로 정리 트리거도 없다.
- 고아 피커에서 파일을 골라도 결과를 받을 popup 이 이미 없어 조용히 버려진다.
- 고아가 떠 있는 동안 부모를 다시 열어 다시 트리거하면 "이미 열려 있음" 으로 거부(`-32000`)되어 버튼이 죽은 것처럼 보인다. 거부 정책 자체는 ADR-0058 의 "모든 트리거는 정확히 하나의 결과를 받는다" 계약을 지키는 의도된 설계이고, 문제는 고아가 생긴다는 사실 쪽이다.
- Esc 한 번에 열린 plugin popup 전체와 host `file_picker` 가 동시에 반응한다. 두 경로가 같은 프레임의 같은 `ctx.input` 을 각자 읽기 때문이다.

호스트에는 소유 관계를 담을 자리가 없었다. `CallerContext::Plugin` 은 `plugin_id` 만 나르고, "어느 popup instance 가 이 피커를 열었는가" 를 아는 쪽은 plugin 뿐이다.

한편 popup 은 "포커스를 독점하지 않는 다중 창" 으로 정의된다([popup.md](../design/systems/popup.md) §Modal 과의 차이). 따라서 "자식이 뜬 동안 부모를 모달로 잠근다" 는 이 시스템의 모델과 맞지 않는다 — 필요한 것은 **스택 유지**(부모가 자식보다 먼저 사라지지 않음)와 **수명 연동**(한쪽이 사라지면 다른 쪽이 정리되고 결과가 유실되지 않음) 두 가지다.

## Decision

**트리거하는 plugin 이 `file_picker.trigger` 파라미터에 자기 popup instance_id 를 `owner_popup_instance` 로 자진 신고하고, host 는 그 값을 자식 popup 의 요청자 기록(`FilePickerRequester`)에 보관해 부모-자식 스택으로 다룬다.** popup 밖(surface 위젯 등)에서 호출하는 경우를 위해 이 파라미터는 선택이며, 없으면 지금까지와 똑같이 관계 없는 단독 popup 이다.

관계가 성립하면 네 가지가 따라온다.

1. **스택 유지** — 자식이 열려 있는 동안 부모는 outside-click dismiss 대상에서 빠진다. 모달화가 아니라 dismiss 목록에서만 제외하는 최소 개입이다.
2. **Esc 소유권** — host/plugin 을 통틀어 이번 프레임 최상단 popup 하나만 Esc 를 소비한다. z_seq 가 공유 전역 시퀀스([ADR-0068](0068-host-plugin-popup-shared-z-seq.md))라 두 진영을 한 축에서 비교할 수 있다.
3. **연쇄 정리** — 부모가 어떤 경로로 닫히든 자식 피커에 **취소 결과를 채운다**. 그러면 기존 result drain 이 평소 경로 그대로 돌아 plugin 에 `file_picker.result { cancelled: true }` 를 보내고 피커도 닫힌다 — ADR-0058 의 "정확히 하나의 결과" 계약이 이 정리 경로에서도 유지된다. 사용자가 이미 확정한 결과는 덮지 않는다.

   "어떤 경로로 닫히든" 은 **close 처리 초크포인트를 하나로 유지**해서 얻는다. popup close 는 사유(사용자 Esc·바깥 클릭·plugin 의 `popup.close`·debug 강제)와 무관하게 전부 `AppState.plugin_popup_closes` 큐에 들어가고, 그 큐를 drain 하는 자리에서만 `PluginManager::close_popup_instance` 를 부른다. 매니저를 직접 치는 호출처가 하나라도 생기면 그 경로만 조용히 정리를 건너뛰므로(실제로 그렇게 새어 자식이 고아로 남았다), 초크포인트 밖 직접 호출은 `crates/tasty-doc-guards/tests/plugin_popup_close_chokepoint.rs` 가 소스 수준에서 막는다. 정리를 마치면 자식이 들고 있던 부모 링크(`owner_popup_instance`)를 끊는다 — 아래 4의 유실 경고가 "정리가 실패했을 때" 에만 나오도록.
4. **결과 유실 감지** — result push 시점에 소유 popup 이 이미 사라졌으면 `warn` 을 남긴다. 연쇄 정리가 제대로 돌면(3의 링크 끊기까지 마치면) 나오지 않아야 하는 조합이라 조용히 넘기지 않는다. 예외는 **사용자가 이미 파일을 확정한 뒤 부모가 죽은** 경우로, 이때는 링크를 그대로 두어 경고가 나오게 한다 — 고른 결과를 받을 popup 이 사라진 것이라 진짜 신호다. 이벤트 자체는 그대로 보낸다 — "정확히 하나의 결과" 는 popup 생사와 무관한 계약이고, plugin 이 popup 밖에서 상관관계를 유지하고 있을 수도 있다.

소유 관계는 **자식 쪽에만** 기록한다. 부모 쪽에 사본을 두지 않으므로 둘이 어긋날 수 없고, 피커가 사라지면 관계도 함께 사라진다.

host 는 plugin id/kind 로 분기하지 않는다 — 자식이 스스로 기록한 부모 instance_id 를 대조할 뿐이라 `file_picker.trigger` 를 쓰는 모든 plugin 에 그대로 적용되는 generic 계약이다(핵심 원칙 2, [identity](../identity.md)).

## Consequences

- **얻은 것**: 고아 피커·조용한 결과 유실·죽은 재시도 버튼이 구조적으로 사라진다. Esc 가 스택을 한 단계씩 벗긴다. 관계 표현이 wire 에 명시되어 `file_picker.trigger` 를 쓰는 다음 plugin 도 같은 보장을 그대로 받는다.
- **잃은 것**: `file_picker.trigger` 의 wire 표면이 한 필드 늘었다. 신고는 자발적이라 plugin 이 안 실으면 예전과 같은 형제 취급으로 남는다 — host 가 강제할 수단은 없다(popup 밖 호출을 구분할 수 없으므로 강제하면 정당한 호출을 막게 된다).
- **운영 비용 / 유지 부담**: `file_picker` 외의 host popup 을 plugin 이 트리거하게 되면 같은 필드·같은 판정을 그 경로에도 붙여야 한다. 소유 관계 보관처가 popup 별 요청자 구조체라 그때마다 한 줄씩 늘어난다.
- **범위 밖으로 남긴 것**: host popup 끼리의 Esc 중재. 각 popup 의 view 가 자기 Esc 를 직접 소비하고 그 의미가 제각각(팔레트는 상태 리셋을 겸함)이라, 24 개 view 를 일괄 개조하는 대신 이번 스택에 실제로 참여하는 `file_picker` 에만 소유권 게이트를 붙였다. host popup 이 서로 겹쳐 열리는 조합이 실제로 생기면 그때 `PopupManager` 차원의 일괄 중재로 올린다.

## Alternatives Considered

- **`CallerContext::Plugin` 에 instance_id 를 담는다**: 호출자가 아무것도 안 실어도 host 가 알게 되어 더 견고하다. 그러나 `CallerContext` 는 **모든** plugin IPC 호출에 걸린 타입이라 파급이 넓고, "popup 안에서의 호출" 이라는 개념이 popup 과 무관한 호출 전반에 스며든다. 필요한 지점 하나(`file_picker.trigger`)의 파라미터로 좁히는 편이 비용 대비 효과가 낫다.
- **host 는 무지한 채로 두고 plugin 이 `dismiss_on_outside_click` 을 런타임 토글한다**: 현재 이 값은 매니페스트 정적 값이라 토글 수단을 새로 만들어야 하고, 만들어도 Esc·연쇄 정리·결과 유실은 그대로 남는다. 부모를 지키는 책임을 plugin 마다 반복 구현하게 되는 것도 generic 계약과 어긋난다.
- **결과 유실만 막는 최소 수정**(부모가 닫힐 때 plugin 이 피커 close 를 요청): 고아와 죽은 버튼은 해소되지만 "부모가 자식보다 먼저 닫힌다" 는 사용자에게 보이는 현상 자체가 남는다. 왕복이 한 번 더 늘어 정리 사이에 프레임 틈도 생긴다.
- **자식이 열린 동안 부모를 모달로 잠근다**: popup 시스템의 정의(포커스를 독점하지 않는 다중 창)와 정면으로 어긋난다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `file_picker` 외의 host popup 을 plugin 이 트리거하는 경로가 둘 이상 생겨, 소유 관계를 popup 별 요청자 구조체에 반복 기록하는 비용이 공용 레지스트리보다 커질 때.
- host popup 두 개 이상이 상시 겹쳐 열리는 조합이 생겨 host↔host Esc 중재가 필요해질 때.
- plugin 이 한 popup 에서 자식 popup 을 **여러 개** 동시에 여는 요구가 생길 때(현재 `file_picker` 는 단일 인스턴스 전제).
- `CallerContext` 가 다른 이유로 호출 주체 컨텍스트를 이미 나르게 되어, 파라미터 자진 신고가 중복이 될 때.

## References

- [ADR-0058 — plugin-triggered host popup 즉시 ack + 이벤트 push](0058-plugin-triggered-host-popup-async-ack-push.md)
- [ADR-0053 — native file picker / remote attach 채널](0053-native-file-picker-remote-attach-channel.md)
- [ADR-0068 — host/plugin 공유 popup z-order 시퀀스](0068-host-plugin-popup-shared-z-seq.md)
- [design/systems/popup.md](../design/systems/popup.md) — 8대 규칙, §Host ↔ Plugin popup z-order, §수명 계약
- [dev-guide/popup-implementation.md](../dev-guide/popup-implementation.md) — `on_close` 훅과 닫힘 정리
