# 단축키 프리셋

tasty 는 4개 프리셋을 제공한다. 각 프리셋은 **바인딩 문자열의 집합**이고, 그 문자열이 실제 물리 키에 매핑되는 방식은 [key-mapping](key-mapping.md) 의 OS별 레이어가 결정한다. 편집·액션은 [keybindings](../../features/keybindings/index.md).

| 프리셋 | 참고 | 특징 |
|--------|------|------|
| **Tasty**(기본) | 자체 | 자체 키 조합 |
| **Mac** | iTerm2/Terminal.app | `alt+`(=⌘) 중심 |
| **Windows** | Windows Terminal | `ctrl+shift+` 중심 |
| **Linux** | GNOME Terminal | `ctrl+shift+` 중심 |

프리셋은 **권장**이지 플랫폼 강제가 아니다 — Mac 프리셋의 `alt+c` 는 macOS 에서 ⌘C, Windows 에서 Alt+C 로 동작한다([key-mapping](key-mapping.md) 의 위치 기반 매핑).

## 전체 바인딩 비교 (빈 칸 = 바인딩 없음)

### 생성/닫기
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| new_window | alt+shift+n | alt+shift+n | ctrl+shift+n | ctrl+shift+n |
| new_workspace | alt+n | alt+n | alt+n | alt+n |
| new_tab | alt+t | alt+t | alt+t | alt+t |
| close_active | ctrl+w | | ctrl+w | ctrl+w |
| close_pane | ctrl+shift+w | alt+shift+w | ctrl+shift+w | ctrl+shift+w |
| close_workspace | alt+shift+w | | alt+shift+w | alt+shift+w |
| restore_closed | ctrl+shift+t | ctrl+shift+t | ctrl+shift+t | ctrl+shift+t |

### 분할 / 포커스
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| split_pane_vertical | alt+e | alt+e | alt+shift+e | alt+shift+e |
| split_pane_horizontal | alt+shift+e | alt+shift+e | alt+shift+d | alt+shift+d |
| split_surface_vertical | alt+d | alt+d | alt+d | alt+d |
| split_surface_horizontal | alt+shift+d | alt+shift+d | alt+e | alt+e |
| focus_pane_next/prev | ctrl+] / ctrl+[ | (동일) | (동일) | (동일) |
| focus_surface_next/prev | alt+] / alt+[ | (동일) | (동일) | (동일) |
| tab_switch_modifier / workspace_switch_modifier | ctrl / alt | (동일) | (동일) | (동일) |
| category_switch_modifier | ctrl+shift | (동일) | (동일) | (동일) |

> Pane/Surface 포커스는 tasty 고유라 전 프리셋 동일(분할은 위 표대로 프리셋마다 다르다). `next_tab`/`prev_tab` 은 `ctrl+tab`/`alt+tab` 이 OS 단축키와 충돌해 기본값 없음.

### 클립보드 / 줌
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| copy | ctrl+c, alt+c, ctrl+shift+c | alt+c | ctrl+c | ctrl+shift+c |
| paste | ctrl+v, alt+v, ctrl+shift+v | alt+v | ctrl+v | ctrl+shift+v |
| zoom_in/out/reset | ctrl/alt 계열 다중 | alt 계열 | ctrl 계열 | ctrl 계열 |

> Tasty 프리셋 copy 는 세 관례를 다 바인딩 — `ctrl+c` 는 선택 있으면 복사, 없으면 SIGINT([clipboard](../../features/clipboard/index.md)).

### UI 토글 / 종료 / 변환
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| toggle_settings | ctrl+, | alt+, | ctrl+, | ctrl+, |
| toggle_notifications | ctrl+shift+i | alt+shift+i | ctrl+shift+i | ctrl+shift+i |
| toggle_dag_list | ctrl+shift+g | alt+shift+g | ctrl+shift+g | ctrl+shift+g |
| toggle_sidebar / _collapse | ctrl+shift+b / ctrl+b | alt+shift+b / alt+b | (ctrl 계열) | (ctrl 계열) |
| fullscreen_stage_exit | escape | escape | escape | escape |
| quit | | alt+q | | ctrl+q |
| quit_minimize | | alt+m | | |
| convert_surface | alt+' | alt+' | alt+' | alt+' |

> `fullscreen_stage_exit`(전체화면 무대 종료)은 4 프리셋 공통 `escape` 다 — 무대는
> 플랫폼 관습이 갈리는 영역이 아니고 "덮은 것을 ESC 로 걷는다" 는 관습이 세 OS 에 공통이다.
> modifier 가 없지만 이 바인딩은 **무대가 올라와 있을 때만** 조회되므로 평상시 ESC 동작
> (settings 모달·notifications 팝업 닫기, 터미널 `\x1b` 전달)을 가져가지 않는다. 설정 UI 의
> 녹화 버튼에서는 ESC 가 "슬롯 비우기" 로 예약돼 있어 ESC 를 **다시 지정할 수는 없다** —
> 기본값으로 되돌리려면 프리셋을 재적용한다([key-mapping](key-mapping.md#바인딩-문자열-문법)).

> `quit_immediate` 는 실수 방지로 전 프리셋 기본값 없음. `apply_*_preset`(레이아웃 프리셋 적용)도 사용자가 직접 배정하도록 기본값 없음. Windows 는 Alt+F4 가 OS 종료라 `quit` 불요.

> 이 표는 호스트 `KeybindingSettings` 프리셋 필드만 다룬다. 클립보드 뷰어(`toggle_clipboard_viewer`)·git
> viewer 등 플러그인 커맨드의 단축키는 각 플러그인 매니페스트의 `[[contributes.commands]]`
> `default_keybinding`으로 선언되며 프리셋과 무관하다 — [plugin-development](../../dev-guide/plugin-development.md#단축키-commands) 참조.

## 설계 원칙

1. **수정자 계층** — Mac: `alt`(⌘)=주요 동작 / `alt+shift`(⌘⇧)=보조 / `ctrl`(⌃)=pane 포커스. Windows·Linux: `ctrl+shift`=주요 / `ctrl`=보조/줌 / `alt`=분할·surface 포커스.
2. **tasty 고유 기능**(Pane/Surface 분할·Workspace·변환)은 각 프리셋 수정자 계층에 맞추되 가능하면 문자 키 유지(`d`=분할, `e`=pane 분할).

## 관련

- [key-mapping](key-mapping.md) — 바인딩 문자열 → 물리 키 OS별 매핑 · [keybindings](../../features/keybindings/index.md) — 편집
