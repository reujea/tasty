# Modifier-hint 오버레이

보조키를 누르고 기다리면 그 조합으로 사용할 수 있는 단축키를 보여 준다.
설계 이유는 [단축키 결정](../policies/key-mapping.md),
시각 값은 [토큰 매핑](design-token-mapping.md)의 modifier-hint 절을 따른다.

## 정체성 — 왜 별도 개념인가

키를 누르는 동안만 유지되고 키보드 포커스를 가져가지 않는다.
메시지가 일정 시간 뒤 사라지는 toast나 클릭으로 여닫는 popup과 수명이 다르다.

## 표시와 소멸

일반 보조키 조합은 500ms, Shift 단독은 1200ms 뒤에 나타난다. 표시에는 200ms fade를 쓰고 키를 놓으면 즉시 사라진다.
시간은 Theme 접근자 `modhint_hold_delay`와 `motion_hold_reveal_shift`에서 읽는다.
Shift의 1200ms는 코드 접근자의 값이며 같은 이름의 duration primitive가 있다고 가정하지 않는다.

처음 키를 누를 때 타이머를 시작한다. Ctrl에 Shift를 추가하는 조합 변경은 타이머를 재시작하지 않는다.
현재 조합으로 지연을 다시 계산하므로 Shift 단독 대기 중 다른 보조키를 누르면 일반 지연을 적용한다.

아직 표시되지 않은 동안 등록된 단축키를 실제 키 입력에서 소비하면 타이머를 다시 시작한다.
단축키 처리(`handle_shortcut_after_edit_judgment`, WebView 키의 `handle_received_shortcut`) 성공과 modifier 더블탭 성공이 대상이다. 이미 보이는 패널, X로 닫은 상태,
드래그 상태는 바꾸지 않는다. 명령 팔레트 실행, 일반 PTY 키, vi copy mode 내부 키, Escape는 이 타이머를 재설정하지 않는다.

## 입력 — 포커스와 마우스

키 입력은 계속 원래 대상에 전달한다. 패널은 이동·크기 조절·X 닫기에 필요한 마우스만 소비한다.
패널은 창(Window) 범위 팝업 인스턴스(`modifier_hint`)다. 선언이 키를 받지 않음(`keyboard = None`)이라
열거나 눌러도 포커스와 순서가 바뀌지 않고 다른 팝업 아래에 그린다. 위쪽 띠(X 폭 제외)로 옮기고
오른쪽 아래 그립으로만 크기를 바꾸며, 둘 다 팝업 관리자가 처리한다. 그립을 끌면 오른쪽·아래 변이 포인터 위치를 따라간다.
셸·그립·X와 페이드는 콘텐츠가 그린다(셸 `Content`).
native WebView(HTML·Markdown surface)와 겹쳐도 WebView를 숨기거나 키보드 포커스를 회수하지 않는다. WebView는
bounds·표시·포커스를 그대로 두고 패널 사각형만 그림과 포인터에서 빼서 패널이 페이지 위에 보인다
([팝업 §passive 팝업](popup.md#passive-팝업)의 `keyboard: None` 규칙).
설정 `enabled`를 끄면 표시하지 않는다. 실제 키 상태는 winit의 ModifiersChanged에서 갱신한다.

## 목록 — 무엇을 보이나

현재 누른 modifier를 모두 포함하는 조합을 보여 준다. Ctrl에서 Ctrl+Shift로 좁히면 목록도 즉시 좁힌다.
정확히 같은 조합뿐 아니라 추가로 누를 수 있는 조합도 보여 준다. 헤더와 섹션은 같은 `combo_keycaps`를 사용한다.

섹션은 조합 키캡 헤더, 특수 역할 행, 단축키 행 순서로 그린다. 조합을 누르고 있으면 무엇이 되는지(탭 번호 전환,
마우스 캡처 우회 등)를 개별 단축키보다 먼저 보이기 위해서다.

바인딩과 역할이 모두 없어도 섹션은 유지하고 `modifier_hint.empty` 문구를 보여 준다.
미할당을 고장이나 비활성으로 오해하지 않게 하기 위해서다.
예외로 macOS 밖에서 Win·Super(`option`)를 누른 동안 `option` 이 든 바인딩·역할이 하나도 없으면 패널을 띄우지 않는다. 타일링 WM 조작이나 Super+드래그 창 이동으로 이 키를 오래 누르는 일이 흔하기 때문이다. 빈 행은 키캡·아이콘·배경 없이 약하게 표시한 텍스트만 그린다.
`modhint_empty_fg`, `modhint_empty_row_gap`, `modhint_empty_row_min_height`를 사용하며
값과 디자인 출처는 토큰 매핑 문서에서 관리한다. 행에는 hover·focus·click 동작을 붙이지 않는다.

## 지오메트리 영속

사용자가 옮기거나 크기를 바꾼 결과는 `Settings::modifier_hint`에 저장한다.
표시 타이머의 변경이 편집 중 위치와 크기를 초기화해서는 안 된다.

## 발화 정책

사용자가 실제로 보조키를 누르고 있을 때 표시를 시작한다. release IPC로 강제 표시할 수 없다.
검증은 GUI debug 전용 `debug.modifier_hint.hold`와 `.state`를 사용한다.
상태 응답의 visible·empty는 실제 draw와 같은 모델을 사용한다.
새 단축키 소비 경로는 표시 전 타이머를 재설정해야 하는지 확인한다.

## 관련

- [단축키 기능](../../features/keybindings/index.md)
- [키 매핑](../policies/key-mapping.md)
- [banner](banner.md) · [toast](toast.md) · [popup](popup.md)
