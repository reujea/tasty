# 접근성 (Accessibility)

- **Status**: Implemented — 수동 설정
- **주체**: 로컬 사용자
- **설계**: [`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md) — Theme의 모션 감소 설정
- **코드**: `AccessibilitySettings` · `ModifierHintSettings` · `Settings::theme_runtime()`(`tasty-settings`), `ThemeRuntime`(`tasty-themes`), `Theme.reduced_motion`(`tasty-type-appearance`), `crates/tasty-ui-widgets/src/{toast,spinner}.rs`, `src/adapters/ui/switch_overlay.rs`, `src/app/modal/shake.rs`, `src/adapters/ui/input/shortcuts/modifier_hint.rs`, `src/adapters/ui/modifier_hint_overlay.rs`
- **화면**: [설정 창](../settings/screens/settings.md)의 Accessibility 탭

## 목적

사용자가 모션을 줄이거나 보조키 단축키 안내를 켤 수 있게 한다. OS 설정 자동 감지, AccessKit, 색맹 팔레트, 스크린 리더 라벨은 현재 제공하지 않는다.

## 내부 동작

### Reduced motion

`accessibility.reduced_motion`의 기본값은 false다. 켜면 다음과 같이 동작한다.

| 대상 | 모션 감소 시 동작 |
|---|---|
| 토스트 | 등장·소멸 페이드 0ms. 수명 동안 100%, 만료 즉시 0% |
| Spinner | 회전을 멈추고 정적인 3개 점 표시 |
| switch-number 오버레이 | 페이드 없이 즉시 표시·제거 |
| 모달 흔들기 | 시작하지 않음 |
| modifier-hint | 페이드만 생략하고 표시 지연 유지 |
| 터미널 콘텐츠 | 원래 모션을 쓰지 않으므로 변화 없음 |

`Settings::theme_runtime()`이 설정값을 만들고 전역 Theme 설치 경로가 `Theme.reduced_motion`에 전달한다. 위젯은 기본적으로 이 값을 읽는다. `Spinner::reduced_motion` 같은 개별 override는 갤러리에서 회전·정지 상태를 함께 보여 줄 때 사용한다.

현재 `ThemeWire`에는 이 값이 없어 plugin 프로세스로 전달되지 않는다. plugin에 모션 위젯을 추가할 때는 호스트의 모션 감소 설정을 전달하는 방법도 함께 정해야 한다. 자세한 단위와 적용 범위는 [테마 가이드](../../design/systems/theme.md#모션-설정과-시간-단위)를 따른다.

### Modifier key hints

`modifier_hint.enabled`의 기본값은 true다. 보조키를 누르고 기다리면 현재 누른 키를 모두 포함하는 단축키 조합을 보여 준다. 기본 위치는 사이드바 하단이다. 키를 놓으면 즉시 사라지고 Ctrl에서 Ctrl+Shift로 바꾸면 목록도 즉시 좁혀진다.

표시 지연은 500ms이며 Shift 단독만 1200ms다. 타이핑 중 자주 누르는 Shift 때문에 안내가 뜨는 것을 줄이기 위한 구분이다([키 매핑 설계 (운영 상세)](../../design/policies/key-mapping.md)). 처음 누를 때 타이머를 시작하고 조합 변경만으로는 다시 시작하지 않는다. 현재 조합으로 지연을 계산하므로 Shift 대기 중 다른 보조키를 추가하면 500ms 기준을 적용한다.

아직 표시 전일 때 실제 키 입력으로 등록된 단축키나 보조키 더블탭을 실행하면 타이머를 다시 시작한다. 명령 팔레트 같은 다른 실행 경로에는 적용하지 않는다. 기본 등장 페이드는 200ms(opacity 0.2→1.0)이며 모션 감소를 켜도 표시 지연은 유지한다.

패널은 키보드 포커스를 받지 않는 창 범위 팝업이다. 키 입력은 기존 대상에 전달하고, 이동(위쪽 띠)·크기 조절(오른쪽 아래 그립)·X 닫기·목록 스크롤에 필요한 마우스만 소비한다. 눌러도 다른 팝업의 포커스는 바뀌지 않고, 다른 팝업과 겹치면 그 아래에 그린다. X는 현재 보조키를 누르고 있는 동안만 패널을 숨긴다. 패널 위 마우스는 다른 팝업처럼 포인터 대상 판정(`popup_owns_point`)으로 하위 surface에 전달되지 않는다([입력 계층](../../architecture/input-layer.md)).

휠은 보조키와 관계없이 세로 스크롤한다. egui는 Ctrl·Cmd 휠을 확대, Shift 휠을 가로 이동으로 해석하므로 `modifier_free_wheel_y()`가 패널 위의 raw MouseWheel을 같은 단위로 다시 읽어 세로 성분을 `scroll_with_delta`에 전달한다. Alt·Option만 누른 경우에는 egui의 세로 처리를 사용해 중복 스크롤을 피한다.

목록은 `build_hint_sections`가 `combos_containing_all`·`Combo::contains_all`로 만든다. 호스트 액션, 사용자 스크립트, 탭·workspace 전환·마우스 캡처 우회·링크 열기 역할을 조합 크기와 우선순위로 정렬한다. 여러 키를 누르면 첫 섹션은 현재 조합과 같다. plugin 단축키는 아직 이 목록에 포함하지 않는다.

바인딩과 역할이 없는 조합도 남겨 `modifier_hint.empty`로 표시한다. 단 macOS 밖에서 Win·Super(`option`)를 누른 동안 `option` 바인딩·역할이 하나도 없으면 패널을 띄우지 않는다(`silent_option_hold`). 키캡·아이콘·배경 없이 약한 텍스트를 사용하며 최소 높이는 20px, 내부 간격은 3px다. `combo_keycap_parts`는 표시 스타일이 symbol인 Alt·Option·Shift를 `CMD_KEY`·`OPTION_KEY`·`SHIFT_KEY` 벡터 아이콘으로 그린다. 해당 글리프가 폰트에 없을 때 빈 사각형이 되는 문제를 피한다([키 매핑](../../design/policies/key-mapping.md)).

실제 키 상태는 winit `ModifiersChanged`에서만 읽는다. `held: Option<Combo>`는 네 보조키 상태를 보관하고 `update_hold`는 조합이 바뀌면 다시 그리도록 알린다. 창 포커스를 잃으면 비운다. `reveal_delay_ms`는 Theme의 `modhint_hold_delay()`와 `motion_hold_reveal_shift()`를 사용하며, 후자의 1200ms는 대응 디자인 토큰이 없어 수기 접근자에 남아 있다.

위치와 크기는 `ModifierHintSettings`의 `pos`·`size`(`Option<(LogicalPx, LogicalPx)>`, 기본 None)에 저장한다. 기본 크기는 180×400, 최소는 180×240이다. 사용자가 옮기거나 크기를 바꿀 때 UpdateSettings로 저장하며 여러 창에서는 마지막 저장값을 사용한다. 창 축소로 화면 밖이 돼도 저장값은 바꾸지 않고 그릴 때 경계를 제한한다.

## 인터페이스

- **사용자**: Settings › Accessibility의 토글(`settings.accessibility.modifier_hint*`). 안내 문구는 `modifier_hint.*`를 사용한다.
- **에이전트**: release에는 강제 표시·조작 API가 없다. GUI debug 전용 `debug.modifier_hint.hold`는 보조키와 타이머를 설정하고 `.state`는 표시 상태를 조회한다. `#[cfg(all(debug_assertions, feature = "gui"))]`로 격리한다([debug IPC](../../dev-guide/debug-ipc.md)).

## 관련

- [설정](../settings/index.md)
- [modifier-hint](../../design/systems/modifier-hint.md)
- [토스트](../../design/systems/toast.md) · [테마](../../design/systems/theme.md)
