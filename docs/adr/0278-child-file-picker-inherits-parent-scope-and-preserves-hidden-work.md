# ADR-0278: 자식 파일 피커는 부모의 유효 범위를 상속하고 숨김 동안 작업을 보존한다

- **Status**: Accepted
- **Date**: 2026-09-15
- **Tags**: popup, file-picker, scope, ownership, input
- **Group**: ui-theme-gallery

## Context

plugin popup은 선언 종류와 host 바인딩으로 가시성을 정하지만, 그 popup의 찾아보기가 여는
host 파일 피커는 Window 범위였다. 부모 surface를 벗어나면 자식만 남아 무관한 화면 위에
그려지고 키보드 입력을 차단했다. 소유 관계의 close 정리는 이미 있으나 숨김의 수명은
정해지지 않았다. 범위 필드만 옮겨도 포커스 게이트가 가시성을 보지 않으면 입력 차단은 남는다.

## Decision

숨김은 close가 아니다. 부모·자식의 draft, 선택, 요청 상관관계를 보존한다. host는 자식의
요청자 plugin과 owner instance를 대조하고 부모의 선언 종류와 바인딩을 공통 함수로 해석한다.
Surface 선언과 target이 함께 있을 때만 Surface 범위를 상속한다. Window 부모, target 없는
부모, owner 없는 단독 피커는 Window 범위를 유지한다. plugin 입력 경로나 origin surface를
부모 범위로 해석하지 않는다.

host 프레임은 자식의 첫 paint/hit 판정 전에 범위를 적용한다. 숨은 범위는 paint, hit, Esc,
키/IME 게이트와 native WebView를 가리는 overlay 판정에서 빠진다. 포커스 의도는 유지하되 실제 포커스 조회는 최신 draw의 가시성을
함께 본다. 숨은 popup은 다른 화면의 클릭으로 dismiss되거나 포커스 의도를 잃지 않는다.
진행 중인 드래그·리사이즈 캡처는 숨김 때 해제한다. 복귀는 같은 상태와 z 순서를 사용한다.

부모 close는 자식 shell을 즉시 닫고 기존 취소·정확히 한 결과·이미 settled된 결과 보존 규약을 따른다. 숨은 자식의 draw를 기다리지 않는다.
피커의 로컬 확정은 기존 DispatchFile과 plugin 결과 이벤트를 각각 수행한다. markdown은
그 이벤트로 부모 경로 입력을 채우고, 부모의 Open이 context의 대상을 실행한다.

기존 surface 경계 clamp를 사용한다. 새 픽셀 값, breadcrumb 축소, scrim, 다른 surface에서의
재개방 정책은 이 결정이 바꾸지 않는다.

## Consequences

- **얻은 것**: 다른 workspace나 tab으로 이동해도 진행하던 선택과 요청이 취소되지 않으며,
  숨은 피커가 그 화면의 입력을 삼키지 않는다.
- **잃은 것**: 부모가 숨은 동안 자식만 따로 조작할 수 없다. 단일 피커 슬롯은 계속 점유된다.
- **운영 비용 / 유지 부담**: 키 게이트의 가시성은 렌더 프레임 기준이다. 부모 선언·바인딩의
  해석과 자식 paint 전 적용 순서를 유지해야 한다. 좁은 surface에서는 기존 clamp로 피커가 작아진다.

## Alternatives Considered

- **숨김 때 cancel** — 화면 전환만으로 초안과 선택 작업이 종료된다.
- **scope_surface만 복사** — Window 선언도 target을 가질 수 있어 부모와 자식의 범위가 갈린다.
- **plugin이 자식 scope를 지정** — host가 이미 가진 선언·바인딩을 별도 입력으로 중복한다.
- **focused를 false로 변경** — 복귀할 때 이전 상호작용 의도를 복원할 수 없다.

## Reconsideration Triggers

**원리적으로 안 붙는 것**

- 숨긴 작업의 유지가 사용자에게 잘못된 대상 선택을 유발한다는 보고가 반복된다. 재는 법:
  부모 입력·자식 선택을 만든 뒤 workspace/tab을 왕복하고 확정과 부모 Open의 실제 대상을 비교한다.
- 좁은 surface에서 피커의 필수 조작이 보이지 않는 디자인이 확정된다. 재는 법: 확정 시안과
  같은 크기의 surface에서 목록·경로·footer를 캡처해 대조한다.

## References

- [Popup 시스템](../design/systems/popup.md)
- [네이티브 파일 피커](../features/native-file-picker/index.md)
- [ADR-0084](0084-plugin-triggered-host-popup-ownership.md) — close 소유권
- [ADR-0273](0273-plugin-popup-declares-a-scope-kind-and-the-host-binds-the-target.md) — 선언 종류와 host target
- 구현 위치: `src/plugin_bridge/popup_scope.rs`의 `popup_scope`, `inherit_file_picker_scope`;
  `src/adapters/ui/popup/draw.rs`의 `PopupManager::draw`.
