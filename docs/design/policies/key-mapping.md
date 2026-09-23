# 키 매핑 설계 (운영 상세)

tasty 의 모든 단축키는 [`KeybindingSettings`](../../features/settings/index.md) 로 노출되며 코드에 하드코딩되지 않는다. 본 문서는 바인딩 문자열의 OS별 키 매핑과 위치 기반 추상화를 기술한다.

## 핵심 원칙: 물리적 키 위치 일관성

Windows/macOS/Linux 에서 **같은 물리적 키 조합 → 같은 기능**. 표준 키보드 하단 수정자 배치:

```
Windows/Linux:  [Ctrl] [Win/Super] [Alt]  ──  [Alt] [Win/Super] [Ctrl]
macOS:          [Ctrl] [Option]    [Cmd]  ──  [Cmd] [Option]    [Ctrl]
```

macOS 의 **Cmd** 는 Windows/Linux 의 **Alt** 와 같은 물리적 위치다. 사용자가 키보드를 바꿔 써도 같은 손가락 위치에서 같은 동작을 기대하므로, tasty 는 이 물리적 위치를 기준으로 매핑한다.

## 바인딩 토큰 ↔ 실제 키

| 토큰 | Windows | Linux | macOS |
|------|---------|-------|-------|
| `ctrl` | Ctrl | Ctrl | Control (⌃) |
| `alt` | Alt | Alt | **Command (⌘)** |
| `shift` | Shift | Shift | Shift |
| `option` | (미사용) | (미사용) | Option (⌥) |

macOS 에서만 `alt` 토큰이 Cmd(⌘)에 매핑된다(물리 위치가 Win/Linux 의 Alt 와 동일하므로). 예: 프리셋 `new_tab = "alt+t"` 는 Win/Linux 에서 Alt+T, macOS 에서 ⌘+T 로 눌린다. 프리셋은 **하나의 바인딩 문자열 집합**을 쓰지만 OS별 매핑으로 각 OS 에서 자연스러운 조합으로 느껴진다.

### 캡처(설정 UI) / 매칭(런타임)

- **캡처**: winit `ModifiersState` → 토큰. macOS `super_key() → "alt"` · `alt_key() → "option"`, 기타 `alt_key() → "alt"`; `control_key() → "ctrl"`, `shift_key() → "shift"`. macOS 에서 ⌘+N 도, Windows 에서 Alt+N 도 동일하게 `"alt+n"` 저장.
- **매칭**: winit `ModifiersState` 와 비교. `"ctrl" → control_key()`, `"alt" → macOS super_key() / 기타 alt_key()`, `"shift" → shift_key()`.

## 바인딩 문자열 문법

`+` 는 구분자이자 키 이름이다. 파서는 왼쪽부터 `ctrl+`·`shift+`·`alt+` 프리픽스를 벗기고 남은 전체를 키 토큰으로 본다 — `"ctrl++"` = "Ctrl + `+` 키".

| 바인딩 | 해석 |
|--------|------|
| `ctrl++` / `ctrl+plus` | Ctrl + `+` (문자/이름 별칭 양방향 매칭) |
| `ctrl+-` / `ctrl+minus` | Ctrl + `-` |
| `ctrl+=` / `ctrl+equals` | Ctrl + `=` |
| `ctrl+shift+=` | Shift 까지 함께 요구 |
| `ctrl+` / `ctrl` | 무효 (키 부분 없음 / 모디파이어 단독) |

- **modifier 없는 일반 키 등록 방지**: 설정 캡처 시 알파벳/숫자/스페이스 등 타이핑 키는 수정자 1개 이상과 함께여야 등록된다(`w` 단독 무시). F1~F12·Tab·Enter 등 비타이핑 키는 수정자 없이 가능.
- **모디파이어 단독 입력(Ctrl/Shift/Alt/Super/Meta/Fn)은 어떤 바인딩과도 매칭 안 됨** — 매처가 구조적으로 차단.
- **Escape 는 설정 UI 녹화에서 "슬롯 비우기"로 예약** — 녹화 중 ESC 를 누르면 그 슬롯이
  지워지고 녹화가 끝난다. 따라서 `escape` 를 값으로 갖는 바인딩은 프리셋/설정 파일로만
  들어오고 녹화 버튼으로는 재지정할 수 없다(현재 해당: `fullscreen_stage_exit`). 기본값으로
  되돌리려면 프리셋을 재적용한다.
- **modifier 없는 바인딩은 modifier-hint 오버레이에 뜨지 않는다** — 오버레이는 홀드 중인
  modifier 조합에 속한 바인딩만 나열하므로, 조합이 없는 바인딩은 속할 섹션이 없다.

## 복사/붙여넣기 키 정책

세 방식을 독립적으로 on/off:

| 방식 | 복사 / 붙여넣기 | macOS 실제 키 |
|------|----------------|---------------|
| macOS | `alt+c` / `alt+v` | ⌘+C / ⌘+V |
| Linux | `ctrl+shift+c` / `ctrl+shift+v` | Ctrl+Shift+C/V |
| Windows | `ctrl+c` / `ctrl+v` | Ctrl+C/V |

## OS 고유 키 이름 혼용 금지

바인딩 토큰 `ctrl`/`alt`/`shift` 는 **물리적 위치를 추상화한 이름**이며 OS 가 인식하는 키 이름과 다를 수 있다. tasty 는 각 키를 OS 고유 이름으로 인식하고 바인딩 문자열과의 변환만 OS별로 다르게 한다:

- macOS **Command(⌘)** 은 Command 다. Alt 가 아니다. **Option(⌥)** 은 Option 이다. Alt 가 아니다.
- Windows **Alt** 는 Alt, **Win** 은 Win 이다.

토큰 `"alt"` 가 macOS 에서 Command 에 매핑되는 것은 OS 고유 키를 다른 이름으로 부르는 게 아니라 **물리적 위치 기반 추상화**다.

## 설정 파일 이식성

`~/.tasty/config.toml` 의 바인딩 문자열은 **OS 독립**이다 — `"alt+n"` 은 어디서든 `"alt+n"`. 변환은 런타임 OS별 매핑 레이어가 한다. **저장(항상 추상 토큰) ↔ 표시(OS별/사용자 선택 표기)** 를 분리해 이식성을 유지한다.

macOS 사용자를 위한 표시 커스터마이징: `GeneralSettings::{alt,option,shift}_display_style` 3 개 필드(설정 > 일반 > 표시, mac 전용 UI)로 Alt/Option/Shift 토큰의 화면 표기를 텍스트("Alt"/"Option"/"Shift", 기본값)와 macOS 심볼("⌘"/"⌥"/"⇧") 사이에서 독립적으로 고를 수 있다(`alt` 는 추가로 "Cmd" 텍스트도 선택 가능). `KeybindingSettings::format_display`/`format_display_parts` 가 이 설정을 받아 표시 문자열을 만든다 — 필드는 크로스플랫폼으로 존재하지만(직렬화 단순성), 값을 바꿀 수 있는 UI 는 macOS 에서만 노출된다. 저장 포맷(바인딩 문자열)에는 전혀 영향을 주지 않는다.

**"symbol" 표시가 실제 화면에 그려지는 방식은 위치마다 다르다.** `format_display`/`format_display_parts` 가 만드는 문자열은 "⌘"/"⌥"/"⇧" 을 그대로 담은 텍스트다(커맨드 팔레트·상태바·키바인딩 탭 등에서 소비) — egui 폰트 fallback 체인에 U+2325(⌥) glyph 가 없어 이 경로는 tofu box 로 깨질 수 있는 리스크를 안고 있다(알려진 이슈, 아직 미해결). 반면 설정 > 일반 > 표시 탭의 3 개 드롭다운과 modifier-hint 오버레이의 keycap 칩(`combo_keycap_parts`, `src/adapters/ui/modifier_hint_overlay.rs`)은 "symbol" 스타일을 텍스트로 타이핑하지 않고 벡터 아이콘(`tasty_icons::{CMD_KEY,OPTION_KEY,SHIFT_KEY}`, `tasty_ui_widgets::{KbdKey,kbd_parts}`)으로 그려 이 문제를 원천 차단한다.

### 이식 시 `option` 처리

이식이 깨지는 토큰은 `option` **하나뿐**이다. 나머지는 저장이 이미 OS 독립이고 `alt` 는 저장이 하나이며 macOS 에서만 ⌘ 로 매핑된다. 반면 `option` 은 macOS 물리 ⌥ 이라 **비-macOS 에서는 매칭 자체가 항상 불일치**로 접힌다(`crates/tasty-key-match/src/lib.rs` 의 winit·egui 두 경로 모두 `option_matches = !parsed.option`). 그래서 macOS 에서 만든 구성을 Windows/Linux 로 가져오면 그 바인딩들은 **화면에는 그대로 보이는데 눌러도 아무 일이 없다**.

네 프리셋에는 `option` 문자열이 하나도 없다 — `option` 바인딩은 사용자가 macOS 에서 직접 만든 것뿐이다(녹화이거나 quick-switch modifier 선택).

판정과 대체는 `tasty_host_plugin::keybinding_bundle::option_migration` 이 한다.

- **판정은 문자열 검색이 아니라 실제 파서**로 한다(`parse_binding` / `Combo::parse_modifiers`). `"option"` 이라는 이름의 키가 있을 가능성과 대소문자·토큰 순서 변형을 문자열 매칭으로 다루면 틀리고, 틀리는 방향이 거짓 음성이라 조용하다.
- **찾는 자리는 다섯이다**: ① 일반 콤보 필드의 각 원소 ② quick-switch 축 modifier 셋 ③ quick-switch 슬롯·다음/이전 — **그 축이 "개별 지정" 일 때만**(규칙 기반 축의 슬롯은 raw 키 하나라 콤보가 아니다) ④ `script_bindings[].combo` ⑤ plugin override 의 `Key { value }`(`Inherit`/`None` 은 콤보를 안 담는다).
- **대체 값을 받는 수단이 자리마다 다르다**(`ReplacementKind`). 콤보 자리는 기존 녹화로 받고, **축 modifier 자리는 녹화로 못 받는다** — 녹화가 modifier 단독 입력을 무시하기 때문이라 `all_modifier_combos()`(비-macOS 7개) 중에서 고르게 해야 한다. sentinel `"individual"` 은 modifier 조합으로 파싱되지 않아 거절되므로, 마이그레이션이 축의 모드를 바꾸지는 않는다.
- **대체 값이 다시 `option` 을 담으면 거절**한다.
- **충돌은 적용 전후의 차분으로 본다.** 축 modifier 를 바꾸면 그 축의 슬롯 전부와 다음/이전의 합성 콤보가 한꺼번에 움직이므로, 검사 대상은 값 하나가 아니라 적용 결과의 **발화 콤보 명부 전체**다. 원래부터 있던 중복까지 거절하면 사용자가 이번 이식과 무관한 이유로 막히므로, 새로 생긴 충돌만 에러가 된다.
- **충돌 네임스페이스는 둘**이다. 호스트 액션·quick-switch·스크립트가 한 묶음이고, plugin 은 plugin 마다 별도다 — 겹치는 키에서 plugin 이 호스트보다 항상 우선하고(아래 "Plugin 커맨드 단축키 우선순위") 어느 plugin 의 명령이 후보인지는 포커스가 가르므로, 호스트↔plugin 과 plugin↔plugin 은 충돌이 아니라 규정된 우선순위다.

- **자리를 비우는 것도 해소다**(`Resolution::Unbind`). 이 환경에서 그 단축키를 안 쓰기로 하는 것은 대체 조합을 고르는 것과 같은 무게의 선택이다. 일반 콤보는 그 원소만, plugin override 는 남은 키가 없으면 `None` 이 되고, quick-switch 슬롯·다음/이전은 빈 값이 된다. **축 modifier 는 비울 수 없다** — 축은 조합 하나를 반드시 갖는다(`CannotUnbind`).
- **새 충돌의 처리는 호출자가 고른다**(`ConflictPolicy`). `Reject` 는 충돌 목록을 돌려주고 아무것도 안 쓴다. `UnbindOther` 는 충돌 상대 중 **계획 밖의 자리**를 비우고 적용한다 — 설정 창 가져오기에서 사용자가 충돌 확인을 수락한 경로다. 충돌 양쪽이 모두 계획 안이면 비울 쪽을 정할 수 없어 `UnbindOther` 에서도 거절한다.
- **덜 채워진 계획도 미리 볼 수 있다**(`introduced_conflicts` · `preview_resolution`). 화면이 자리마다 충돌 표시를 붙이려면 모든 자리가 정해지기 전에 판정해야 하므로, 이 둘은 정해진 자리만 쓰고 나머지는 원래 값으로 둔 채 차분을 본다. 실제 적용(`resolve_migration`)은 계획이 완전해야 한다.

대상 플랫폼 판정은 컴파일 타임(`cfg!(target_os = "macos")`)으로 충분하다 — 비-macOS 바이너리는 매칭이 항상 불일치라 런타임에 갈릴 여지가 없다. 대상이 macOS 면 목록은 비어 있다.

## OS 메뉴 key equivalent

tasty 가 직접 소유하는 OS 메뉴의 key equivalent 도 **`KeybindingSettings` 의 대응 binding 에서 가져온다 — 가져올 수 없으면 비운다.** 지금 key equivalent 를 배선한 메뉴는 macOS NSMenu 하나이고, Windows AcceleratorTable · Linux Wayland 메뉴도 등록하게 되면 같은 규칙을 따른다. selector 가 OS 표준(`cut:` / `performClose:` 등)이라는 사실이 단축키 하드코딩을 정당화하지 않는다(selector 와 key equivalent 는 독립 결정). binding 이 빈 vec 이면 key equivalent 도 비워 단축키 없는 메뉴 항목으로 둔다.

**예외**: OS 자체가 박아 tasty 가 무력화/덮어쓰기/가로채기 모두 불가능한 단축키(macOS Spotlight `Cmd+Space`, OS 전역 윈도우 전환 등)는 정책 범위 밖 — tasty 가 등록할 수도 끌 수도 없다.

**라벨은 `t()` 다** — key equivalent 와 독립으로, OS 메뉴 항목의 **제목**은 `lang/*.toml` 번역 키에서 온다: macOS NSMenu `menu.macos.*`(앱 이름을 결합하는 About / Hide / Quit 은 `{}` placeholder + `t_fmt` 로 언어별 어순 대응), 시스템 트레이 메뉴·툴팁 `tray.*`, Windows Jump List `jump_list.*`. OS 네이티브 표면도 [i18n](../../dev-guide/i18n.md) 예외가 아니다. macOS 메뉴는 `KeybindingSettings` 변경 시 통째로 rebuild 되면서 라벨도 다시 조회하므로 키바인딩 변경 후에도 현재 언어가 유지된다.

## Plugin 커맨드 단축키 우선순위

Plugin 이 `[[contributes.commands]]` 로 선언한 단축키(`CommandDecl`)는 호스트
`KeybindingSettings` 와 별도의 매칭 경로를 거치며, 겹치는 키에 대해 **항상 plugin
이 우선**한다.

- **디스패치 순서**: `App::try_plugin_shortcut` 이 매 키 입력마다 호스트 단축키
  디스패치(`dispatch_window_event_to_view`)보다 **먼저** 호출된다
  (`src/app/event_handler.rs`). Plugin command 가 매칭되면 이벤트가 그 자리에서
  소모되어 호스트 디스패치로 흘러가지 않는다 — 즉 같은 키를 호스트
  `KeybindingSettings` 에도 지정했다면 plugin 쪽이 이긴다. 이 순서는 의도적
  설계 결정이다: plugin 은 사용자가 명시적으로 활성화·설정한 확장이므로, 사용자가
  같은 키를 plugin 커맨드에도 지정했다면 그 의도를 존중한다.
- **scope 별 매칭 대상**:
  - 포커스된 surface 가 어떤 plugin 이 만든 `RemoteSurface` 이면, **그 plugin 의
    커맨드만**(scope 무관 — `Global`/`Surface` 둘 다) 후보가 된다. "그 plugin 의
    surface 가 포커스되어 있다"는 조건 자체가 `Surface` scope 의 발화 조건을
    이미 만족하기 때문.
  - 포커스된 plugin surface 가 없으면(순수 터미널 tab 등), 등록된 **모든**
    plugin 의 `CommandScope::Global` 커맨드가 후보가 된다. `CommandScope::Surface`
    커맨드는 이 경로에 나타나지 않는다 — owner surface 가 실제로 포커스되어
    있을 때만 발화한다(문서상 "어디서나 동작"이 아니라 "그 plugin surface
    포커스 시에만 동작"이 `Surface` scope 의 계약).
- **여러 plugin 이 같은 키를 Global 로 등록한 경우**: 먼저 발견되는 plugin(등록
  순서 — `PluginCommandRegistry` 내부 순회 순서, 결정론적이지만 사용자에게
  노출되는 우선순위 규칙은 아님)이 이긴다. 여러 plugin 이 같은 Global 키를
  등록하는 상황 자체가 plugin 작성 시점의 설계 실수에 가까우므로, 이 경우를 위한
  전용 충돌 감지 UI 는 아직 없다(설정 UI 의 host-vs-host 중복 키 감지처럼 plugin
  용도 확장은 후속 과제).
- **동작 종류 우선순위(`action` vs `handle_command`)**: `CommandDecl.action`
  이 선언되어 있으면 호스트가 그 액션(`ToolAction::Event`/`OpenSurface`/`OpenPopup`)
  을 `[[contributes.tool]]` 과 동일하게 직접 실행하고, 옛 `command.invoke` IPC
  (`handle_command`)는 이 커맨드에 대해 **발사되지 않는다** — 두 경로가 동시에
  실행되면 popup 이 중복으로 열리는 등의 부작용이 있어 `action` 이 있으면
  `handle_command` 는 완전히 스킵한다. `action` 이 없으면 기존 `handle_command`
  IPC 왕복 경로를 그대로 쓴다. Event Bus `command.invoked` owner-unicast 통지는
  `action` 유무·대상 surface 유무와 무관하게 매칭될 때마다 항상 발사되는
  informational 통지다.

## 합성 키 이벤트 (winit)

winit 은 창이 포커스를 **얻는** 순간 그때 물리적으로 눌려 있던 모든 키에 대해 `Pressed`
이벤트를, **잃는** 순간 같은 키들에 대해 `Released` 이벤트를 합성해 보낸다
(`WindowEvent::KeyboardInput { is_synthetic: true, .. }`). X11 과 Windows 에서만 동작하며
macOS·Wayland 에서는 합성하지 않는다.

**tasty 는 합성 키 이벤트를 사용자 입력으로 취급하지 않고 전부 버린다.** 사용자가 그 창
안에서 누른 적이 없는 키이기 때문이다. 예: 다른 앱을 `Alt+F4` 로 닫으면 그 앱과 함께
`F4` 의 keyup 이 배달될 곳을 잃어 OS 키보드 상태에 눌린 채 남고, 이어서 tasty 가 포커스를
받는 순간 합성 `F4` Pressed 가 들어와 `rename_tab` 바인딩이 저절로 발화한다.

### 차단 지점 — 이벤트 진입부 단 한 곳

판정은 `is_synthetic_key_event`, 게이트는 `App::window_event` 진입부다. shell setup /
종료 / 부팅 / 모달 / plugin 단축키 가로채기 / View 위임보다 **앞**에 둔다 — 이 분기들은
전부 조기 return 하는 배타적 경로라, 하나라도 게이트보다 앞서면 그 모드에서만 합성 키가
새는 부분 회귀가 된다.

지점별로 막지 않는 이유는 유입 경로가 두 축이기 때문이다.

- **직접 해석 경로** — `WindowEvent::KeyboardInput` 을 패턴 매칭해 단축키·PTY·녹화로
  보내는 곳. **결함이 실제로 재현되는 축이다.** 게이트가 없으면 포커스 획득 시 합성 `F4`
  가 `rename_tab` 팝업을 열고, `Escape` 를 누른 채 종료 확인 창에 포커스를 넘기면 그 창이
  저절로 닫힌다(`src/view/quit.rs` 의 `Escape` 처리). X11 대조군 실측에서 게이트 전에는
  둘 다 재현되고 게이트 후에는 발생하지 않으며, 창 안에서 직접 누른 같은 키는 그대로
  동작한다.
- **egui feed 경로** — 키 이벤트를 해석하지 않고 `handle_egui_event` 로 통째로 넘기는 곳.
  `WindowEvent::KeyboardInput` grep 으로 **잡히지 않는다.** View 5 종
  (`MainView`/`SettingsView`/`PresetView`/`PluginsView`/`QuitView`)이 전부 이 경로를 갖고,
  `PresetView`·`PluginsView` 는 이 경로**뿐**이다.

  이 축은 tasty 게이트가 없어도 합성 `Pressed` 에 한해 **우연히** 막혀 있다 — `egui-winit`
  이 `is_synthetic && state == Pressed` 인 `KeyboardInput` 을 egui 에 넘기기 전에 스스로
  버리기 때문이다(0.31.1 기준). 그래서 "합성 `Enter` 가 종료 확인 창의 확인 버튼을 누른다"
  는 실제로는 발생하지 않는다 — 그 버튼에 egui 키보드 포커스가 살아 있어도 그렇다(egui 는
  창이 blur 돼도 위젯 키보드 포커스를 놓지 않으므로, 안전의 근거는 위젯 포커스가 아니라
  이 필터다). 다만 이건 상위 의존성의 구현 세부이지 계약이 아니고, 같은 필터가 합성
  `Released` 는 걸러내지 않는다. 버전이 바뀌면 조용히 사라질 보호막이므로 tasty 는 이 축도
  자기 진입부에서 직접 끊는다.

진입부 한 곳에서 버리면 두 축이 동시에 덮이고, View 가 새로 늘어도 자동으로 덮인다.
배선 위치는 `crates/tasty-doc-guards/tests/synthetic_key_event_guard.rs` 가 강제한다.

### 버려도 modifier 상태가 깨지지 않는 이유

양쪽 백엔드 모두 합성 키와 **별개로** `ModifiersChanged` 를 보낸다 — X11 은 포커스 획득
처리 말미의 `update_mods_from_query`, Windows 는 `gain_active_focus` 의 `update_modifiers`
가 담당한다. 따라서 합성 modifier Pressed 를 버려도 포커스 획득 직후의 modifier 조합
단축키는 정상 매칭된다.

### double-tap detector 는 포커스 전환마다 초기화한다

합성 이벤트를 버리면 modifier 의 down/up 짝이 포커스 경계를 넘을 때 완결되지 않는다.
`Alt+Tab` 으로 빠져나가면 `Alt` 의 press 만 들어오고 짝이 되는 release 는 합성이라
버려지므로, 그대로 두면 돌아와서 `Alt` 를 떼는 것이 "clean release" 로 오인돼 first tap
으로 기록되고 다음 실제 탭 한 번에 double-tap 이 오발화한다. `DoubleTapDetector::reset`
을 `WindowEvent::Focused` 양방향에서 호출한다. `MainView` 와 `SettingsView` 는 **별개
인스턴스**라 두 곳 모두 배선한다.

### `#[cfg]` 분기를 두지 않는 이유

`is_synthetic` 은 플랫폼 중립 필드고 발현 원인도 winit 의 공통 계약이다. 합성을 하지 않는
macOS·Wayland 에서는 항상 `false` 로 들어와 동작이 바뀌지 않으므로, OS 별 근본 원인이
다를 때 요구되는 분기 케이스가 아니다.

## 코드 위치

- `KeybindingSettings`(바인딩 필드·`format_display`), 캡처/매칭 레이어, 프리셋 기본 바인딩.
- OS 메뉴 배선: macOS NSMenu(`crates/tasty-platform/src/native_menu/macos.rs`).
- 합성 키 차단: `src/adapters/ui/input/synthetic.rs`(`is_synthetic_key_event`),
  `src/app/event_handler.rs`(`App::window_event` 진입부 게이트),
  `src/adapters/ui/input/double_tap.rs`(`DoubleTapDetector::reset`).
- Plugin 커맨드: `crates/tasty-host-plugin/src/command_registry.rs`(`PluginCommandRegistry`,
  `effective_binding`), `src/plugin_bridge/key_dispatch.rs`(`match_plugin_shortcut`/
  `match_global_shortcut`/`dispatch_plugin_command`), `src/app/plugin_glue/shortcut.rs`
  (`App::try_plugin_shortcut`).
</content>
