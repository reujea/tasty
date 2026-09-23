# 단축키 (Keybindings)

- **Status**: Implemented
- **주체**: 로컬 사용자
- **ADR**: [0256](../../adr/0256-the-binding-parser-lives-with-the-setting-it-parses.md) 파서 위치 · [0257](../../adr/0257-the-keybinding-bundle-is-a-toml-file-with-a-schema-tag.md) 이식 번들 · [0269](../../adr/0269-keybinding-import-applies-selected-rows-onto-the-draft.md) 가져오기 적용 단위 (그 밖의 정책은 [design/policies/key-mapping](../../design/policies/key-mapping.md))
- **코드**: `crates/tasty-settings/src/keybindings.rs` (+ `crud.rs` · `presets.rs` · `parse.rs`) · 이식 번들 `crates/tasty-host-plugin/src/keybinding_bundle.rs` · 가져오기/내보내기 화면 `src/view/settings/ui/keybindings_tab/import_export.rs`
- **화면**: [설정 창](../settings/screens/settings.md) Keybindings 탭

## 목적

tasty 의 **모든 단축키는 `KeybindingSettings` 한 곳에서 정의**되며 코드에 하드코딩되지 않는다([CLAUDE.md](../../../CLAUDE.md) "단축키" 필수 정책). 사용자가 Settings 의 Keybindings 탭에서 액션별 키 조합을 추가/삭제/변경한다. OS 메뉴(macOS NSMenu / Windows AcceleratorTable)의 key equivalent 도 이 binding 을 따라간다.

## 내부 동작

### 액션 ↔ 바인딩 목록

각 액션은 **바인딩 문자열의 `Vec`** 를 가진다(다중 바인딩 — 한 액션에 여러 키 조합 허용). 예: `copy`, `paste`, `enter_copy_mode`, `apply_workspace_preset` 등. 빈 `vec` 이면 그 액션엔 단축키가 없다(메뉴엔 단축키 없는 항목으로 표시).

바인딩 문자열은 **OS 독립 표기**다 — 위치 기반 추상화로 macOS 에선 `alt`→⌘ 등으로 매핑된다([key-mapping](../../design/policies/key-mapping.md)).

그 문자열을 축과 키 토큰으로 쪼개는 **파싱**(`parse_binding` · 축 modifier 조합의 `Combo::parse_modifiers`)은 값을 저장하는 크레이트인 `tasty_settings::keybindings::parse` 에 있고, 그 결과를 실제 키 이벤트와 대조하는 **매칭**만 `src/adapters/ui/input/shortcuts/` 에 남는다. 이식 판정처럼 UI 밖에서도 같은 규칙이 필요하기 때문이다([ADR-0256](../../adr/0256-the-binding-parser-lives-with-the-setting-it-parses.md)).

**사이드바 "도구" 메뉴의 빌트인 항목 일곱은 전부 대응 필드를 갖는다** — `toggle_command_palette` · `open_port_scanner` · `open_remote_tool` · `open_preset_window` · `open_tutorial` · `toggle_dag_list` · `open_file_picker`. plugin 이 기여하는 도구 항목은 매니페스트의 `[[contributes.commands]]` 로 이미 단축키를 갖고 Plugins 서브탭에서 관리되므로, 호스트 빌트인만 필드가 없으면 **plugin 도구는 되고 호스트 도구는 안 되는** 역전이 남는다. 다섯(팔레트·DAG 목록 제외)은 네 프리셋 모두 기본값이 비어 있다 — 새 기본값 다섯을 네 프리셋에 넣으면 기존 콤보와의 충돌을 통과해야 하고, 통과하더라도 그 콤보를 이미 쓰던 사용자는 첫 로드에서 `remove_conflicts_from_defaults` 가 새 기본값을 말없이 지운다.

`open_preset_window`(도구 메뉴의 **프리셋** 윈도우 열기)와 `apply_workspace_preset`/`apply_tab_preset`/`apply_pane_preset`(레이아웃 프리셋 **적용** picker)은 다른 것이다. 이름이 비슷해 섞기 쉬우므로 설정 화면에서도 서브탭이 갈린다 — 앞은 General, 뒤 셋은 대상 스코프를 따라 Workspace/Tab/Pane 이다.

### 탭/워크스페이스/카테고리 quick-switch (raw 키 + 축별 modifier 조합)

번호 전환·다음/이전 이동은 **콤보가 아니라 raw 키 하나**만 저장하는 별도 필드로 다룬다(단, "개별 지정" 모드 예외 — 아래 참조). modifier 는 세 축 각자의 독립 필드 `tab_switch_modifier`/`workspace_switch_modifier`/`category_switch_modifier`(각각 기본 `ctrl`/`alt`/`ctrl+shift`)에서 dispatch 시점에 조합되므로, modifier 드롭다운을 바꾸면 그 축의 모든 슬롯이 즉시 재조합된다. **각 modifier 는 단일 토큰(`"ctrl"`)뿐 아니라 조합(`"ctrl+shift"`)도 허용**하며, 매칭은 일반 바인딩과 동일한 4축 조합 파서(`Combo::parse_modifiers`)를 단일 소스로 쓴다. 카테고리도 1급 축으로 자기 modifier 필드를 갖는다. 이 필드들은 콤보 시스템(`GENERAL_BINDING_FIELDS`/`get_bindings`)과 분리되며 index 기반 accessor(`tab_slot_key`/`set_tab_slot_key` 등, `crud.rs`)로 접근한다. 세 축이 modifier·슬롯 배열·다음/이전을 대칭으로 갖는다는 사실 자체는 `SwitchAxis`/`SwitchStep`(`crud.rs`)이 타입으로 들고 있어, 축을 순회하는 쪽(설정 화면·이식 판정)이 필드 이름을 다시 나열하지 않는다 — 슬롯 수(탭 10 / 워크스페이스 9 / 카테고리 10)도 필드 타입이 쓰는 상수(`TAB_SWITCH_SLOT_COUNT` 등) 하나에서 나온다.

| 필드 | 타입 | 기본값 | 의미 |
|------|------|--------|------|
| `tab_switch_modifier` / `workspace_switch_modifier` / `category_switch_modifier` | `String` | `"ctrl"` / `"alt"` / `"ctrl+shift"` | 축별 modifier 조합. `KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER`(`"individual"`) sentinel 이면 그 축은 규칙 기반이 아니라 "개별 지정" 모드 |
| `tab_switch_slot_keys` | `[String; 10]` | `["1".."9","0"]` | 탭 1~10번 슬롯 |
| `workspace_switch_slot_keys` | `[String; 9]` | `["1".."9"]` | 워크스페이스 1~9번 슬롯(0번 없음) |
| `category_switch_slot_keys` | `[String; 10]` | `["1".."9","0"]` | 카테고리 1~10번 슬롯(reserved normal=1). `category_switch_modifier` 와 합성 |
| `tab_switch_next_key` / `tab_switch_prev_key` | `String` | `"l"` / `"h"` | 탭 다음/이전 |
| `workspace_switch_next_key` / `workspace_switch_prev_key` | `String` | `"j"` / `"k"` | 워크스페이스 다음/이전 |
| `category_switch_next_key` / `category_switch_prev_key` | `String` | `"j"` / `"k"` | 카테고리 다음/이전(다음/이전 **카테고리 자체**로 전환 — 카테고리 내부 워크스페이스 이동인 워크스페이스 축 next/prev 와 다름) |

세 축 모두 슬롯 + 다음/이전을 대칭으로 갖는다. 카테고리 modifier 기본값 `ctrl+shift` 는 macOS 스크린샷 예약(`⌘⇧3/4/5`, tasty 가 가로챌 수 없음)과 겹치지 않게 고른 값이고, 카테고리 next/prev 기본 raw 키 `j`/`k` 는 4 프리셋 전수 대조로 다른 액션과 무충돌임을 확인한 값이다(워크스페이스 축과 문자는 같지만 modifier 가 달라 합성 콤보는 겹치지 않는다 — `ctrl+shift+j` vs `alt+j`). slot/next/prev 필드는 모두 필드별 `#[serde(default = "…")]` 를 가져 신규 필드가 없는 구버전 config 를 읽어도 빈 값이 아니라 위 기본값으로 복원된다(`category_switch_modifier` 도 동일 — `"ctrl+shift"` default). 자유 콤보용 `next_tab`/`prev_tab` 필드와는 별개다(Command Palette·더블탭 경로 전용, quick-switch 가 건드리지 않음).

#### quick-switch 섹션 UI (Tab/Workspace 서브탭)

Tab 서브탭(탭 축)과 Workspace 서브탭(워크스페이스 축 + 카테고리 축)의 일반 콤보 목록 아래에 **quick-switch 섹션**이 있다(`keybindings_tab/quick_switch.rs`). 구성:

1. **modifier 드롭다운** — 해당 축 modifier 를 **OS-aware 허용 조합 리스트**(`tasty_settings::keybindings::parse::all_modifier_combos`, 비-macOS 7개·macOS option 축 포함 15개)와 **"개별 지정" sentinel 옵션** 중에서 고른다. 규칙 기반 값은 열거된 유효 조합만 노출해 쓰레기 값 저장을 원천 차단하고(표시는 `format_display`, `"ctrl+shift"` → `Ctrl+Shift`), "개별 지정"은 별도 번역 라벨로 표시된다.
2. **슬롯 1~N 버튼** — 탭 1~10번 / 워크스페이스 1~9번 / 카테고리 1~10번. 규칙 기반 축은 저장된 raw 키를 현재 modifier 조합과 **표시 시점에 합성**한 `"{Modifier}+{Key}"`(예: `Ctrl+Shift+1`) 라벨을 보여주고, 개별 지정 축은 슬롯 필드에 이미 저장된 **완전 콤보**를 그대로 표시한다.
3. **다음/이전 버튼 2개** — `*_next_key`/`*_prev_key`(세 축 모두).

버튼을 누르면 그 축이 규칙 기반이면 **bare-key 녹화**(`capture_bare_key` — modifier 가 하나라도 눌리면 무효, modifier 없는 순수 키 하나만 유효), 개별 지정이면 **일반 콤보 녹화**(`capture_winit_key_combo` — 일반 액션과 동일하게 modifier 포함 자유 조합)로 진입한다. Escape 는 두 경우 모두 슬롯을 비운다. 캡처 분기는 `RecordingSlot.field_kind`(`Combo`/`BareKey(BareTarget)`/`IndividualSlot(BareTarget)`)로 결정되고, `SettingsUiState::recording_is_bare_key()`(`FieldKind::BareKey(_)` 만 매치)가 winit 이벤트 캡처 경로를 가른다(`view/settings.rs`) — `IndividualSlot` 은 이 패턴에 안 걸려 자동으로 `capture_winit_key_combo` 경로를 탄다.

**충돌 검사**는 최종 콤보(규칙 기반은 `"{modifier}+{key}"` 합성값, 개별 지정은 저장값 그대로) 기준으로 두 가지를 본다: ① 일반 액션과의 충돌(`find_conflict` — `next_tab`/`prev_tab` 포함), ② 다른 quick-switch 슬롯과의 중복(슬롯 배열 자체 순회, 탭↔워크스페이스↔카테고리 교차 포함, 개별 지정 축도 동일하게 포함). 두 축이 같은 조합을 갖는 상태는 애초에 저장되지 않는다(`switch_target_for` 에는 우선순위 로직이 없다). 충돌 시 기존 확인 팝업(`PendingBinding`)을 재사용하며, accept 시 상대가 일반 필드면 그 바인딩을, 다른 슬롯이면 그 슬롯을 비운다. 또한 modifier 변경 등으로 현재 슬롯 콤보가 일반 액션과 겹치면 섹션 하단에 경고 라벨을 표시해 조용히 넘기지 않는다.

#### "개별 지정" 모드

modifier 드롭다운에서 **"개별 지정"**(sentinel `KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER = "individual"`)을 고르면 그 축은 규칙 기반(modifier + raw 키 1개)을 벗어나, 슬롯마다 완전히 독립된 콤보(모디파이어 포함 자유 조합)를 일반 액션처럼 녹화한다. 슬롯 필드의 "의미"가 modifier 값에 따라 갈리는 암묵적 불변식이다 — 규칙 기반이면 raw 키 하나, 개별 지정이면 이미 완성된 콤보 문자열(예: `"ctrl+alt+1"`). sentinel 문자열은 4축 조합 파서(`Combo::parse_modifiers`)가 인식하는 `ctrl`/`shift`/`alt`/`option` 토큰 중 어느 것과도 안 맞아 파싱 실패(`None`)하므로, 파서 수정 없이 "이 축은 규칙 기반이 아니다"를 안전하게 표현한다.

- **모드 전환 시 슬롯 이관/복원** — 규칙 기반 → 개별 지정으로 바꾸면 각 슬롯의 현재 합성 콤보(`구 modifier + raw`)가 그대로 슬롯 필드에 저장돼(`apply_modifier_transition`) 전환 직후 사용자 체감 동작이 100% 유지된다. 역방향(개별 지정 → 규칙 기반)은 개별 지정 콤보 문자열이 raw 로 역산 불가능(구조적 정보 유실)하므로 이 축을 기본값으로 복원한다(`reset_tab_switch_to_defaults` 등, `crud.rs`).
- **디스패치** — 개별 지정 축은 `switch_target_for` 가 그 축을 절대 반환하지 않으므로(sentinel 파싱 실패) 규칙 기반과 다른 별도 경로를 탄다. `input/shortcuts/numeric.rs` 가 축별 modifier 를 직접 확인해 개별 지정이면 그 축의 다음/이전 raw 값(=완전 콤보)과 슬롯 배열을 `matches_binding` 으로 순회 매칭한다(next/prev 우선순위는 규칙 기반과 동일).
- **switch-number 오버레이 소멸은 의도된 부작용** — 개별 지정 축은 `switch_target_for` 가 그 축을 절대 반환하지 않으므로 탭바/사이드바 숫자 키캡 오버레이가 자동으로 안 뜬다. 슬롯마다 콤보가 달라 통일된 숫자 힌트를 그릴 근거가 없기 때문.
- 프리셋(Tasty/Mac/Windows/Linux)은 항상 규칙 기반 값만 가지며 개별 지정으로 바뀌지 않는다.

#### dispatch — 키 입력 → 전환

실제 키 소비는 `input/shortcuts/numeric.rs`(`handle_numeric_switch_shortcuts`)가 담당한다. **개별 지정 축**은 규칙 기반 판정보다 먼저 검사한다(세 축 각각 modifier == sentinel 이면 그 축의 next/prev·슬롯을 `matches_binding` 으로 직접 매칭 — 위 "개별 지정 모드" 참조, 카테고리 축은 folders 게이트도 함께 적용). 규칙 기반 축은 `Key::Character` 이면(슬롯 키가 `"q"` 같은 문자일 수 있으므로 숫자 여부를 따지지 않는다) 대상(Tab/Workspace/Category)을 switch-number 오버레이와 **단일 소스**인 `switch_target_for(kb, ctrl, shift, alt, option)` 로 판정한다 — 세 축 각각의 modifier 조합(`Combo::parse_modifiers`)과 현재 눌린 조합이 **정확히 일치**할 때만 그 축이 잡힌다(단일 토큰은 조합의 부분집합이라 그대로 동작, `ctrl` 단독 ≠ `ctrl+shift`). 정확 일치라 축이 서로 새지 않고 우선순위 로직이 없다. `alt` 는 `"alt"` 토큰(macOS 물리 ⌘=super, 그 외 Alt), `option` 은 `"option"` 토큰(macOS 물리 ⌥, 그 외 항상 false)으로 플랫폼 정규화된 값을 받는다. 대상이 잡히면 **next/prev 키를 먼저**(커스텀 슬롯 키가 next/prev 키와 겹칠 때 next/prev 우선), 그 다음 슬롯 배열을 `position` 검색한다. 매칭 결과:

- 탭: next/prev → `next_tab_in_pane`/`prev_tab_in_pane`, 슬롯 index → `goto_tab_in_pane(index)`.
- 워크스페이스: next/prev → `next_workspace_in_active_category`/`prev_workspace_in_active_category`, 슬롯 local → 카테고리 토글 on 이면 `switch_workspace_in_active_category(local)`, off 면 `switch_workspace(local)`.
- 카테고리: next/prev → `next_category`/`prev_category`(`state/workspace.rs` — 카테고리 리스트 안에서 wrap-around ±1 이동한 뒤 `switch_to_category` 재사용, auto-expand + last-active 착지 포함), 슬롯 section → `switch_to_category(section)`. 세 매칭 모두 folders 토글 off 면 no-op·비소비. → [워크스페이스 카테고리](../workspace-category/index.md).

어느 것도 매칭 안 되면 조용히 `false` 를 돌려 다른 단축키 매칭으로 넘긴다. 이 메서드들은 focused pane 의 active_tab · active_workspace(= **사용자 포커스 상태**)를 바꾸므로 **사용자 키 입력 경로에서만** 호출되며 release IPC/CLI 로 노출되지 않는다(원칙 1/3). Command Palette·더블탭 파리티는 범위 밖.

#### switch-number 오버레이 — 표시 = 동작

modifier 홀드 중 탭바(`tab_bar/tab.rs`)·사이드바(`sidebar/view.rs`)에 뜨는 숫자 키캡은 고정 상수가 아니라 **설정된 슬롯 키**를 그린다(`switch_overlay::tab_digit(kb, index)`/`workspace_digit(kb, local_idx)`). 슬롯을 `"q"` 로 바꾸면 키캡도 `Q` 로 뜬다(눌러서 가는 곳 = 표시). 워크스페이스 사이드바는 카테고리 토글 on 시 **active 카테고리 내 로컬 인덱스**로 키캡을 매기고 **비활성 카테고리 행에는 키캡을 표시하지 않는다** — 슬롯 단축키가 active 카테고리 로컬 순서로 전환하기 때문(전역 인덱스로 표시하던 과거 불일치를 제거). 오버레이 modifier 상태는 egui raw_input(실제 사용자 키)만 반영하므로 IPC/에이전트로는 강제 표시할 수 없다. **개별 지정 축은 오버레이 대상에서 자동 제외**된다(위 "개별 지정 모드" 참조).

### 이식 번들 — 구성 전량을 파일 한 장으로

한 tasty 환경의 단축키 구성 **전량**을 파일 하나로 내보내고 다른 환경에서 읽어 들이는 코덱이 있다(`crates/tasty-host-plugin/src/keybinding_bundle.rs`). 옮기는 대상은 두 파일에 흩어져 있다 — `config.toml` 의 `[keybindings]`(`KeybindingSettings` 전량)와 `plugins.toml` 의 `keybindings`(plugin command override). 그래서 번들 타입은 두 타입이 모두 보이는 `tasty-host-plugin` 에 있다.

포맷은 최상위에 **스키마 태그와 버전**이 붙은 TOML 이다.

```toml
schema = "tasty.keybindings"
version = 1

[keybindings]
new_tab = ["alt+t", "ctrl+alt+t"]
tab_switch_modifier = "individual"
# … `KeybindingSettings` 의 필드 전량

[[keybindings.script_bindings]]
script_id = "s1"
combo = "ctrl+f9"

[plugin_keybindings."com.example.explorer"."explorer.refresh"]
mode = "key"
value = ["F6"]
```

번들은 액션 이름을 따로 열거하지 않고 `KeybindingSettings` 를 통째로 싣는다 — 번들에 있는 필드가 곧 그 타입에 있는 필드이므로, "모든 단축키는 `KeybindingSettings` 로 노출된다" 는 규칙이 번들 포맷에서도 그대로 성립한다.

**export 원본은 `PluginsConfig.keybindings` 자체**(`PluginsConfig::shortcut_overrides`)다. 설정 창이 가진 `PluginShortcutSnapshot` 은 `command_registry.iter_all()` 로 만들어져 **등록된 command 만** 담으므로, 그것을 원본으로 쓰면 비활성·미등록 plugin 의 override 가 조용히 빠진다.

import(`decode`)은 사용자가 고른 임의의 파일을 다루므로 세 갈래로 갈린다.

- **거절** — 최상위 `schema` 가 없거나 다르면 `NotABundle`, TOML 자체가 깨졌으면 `Toml`. 값을 하나도 복원할 수 없는 경우만 에러다(`config.toml` 을 골라도 패닉하지 않는다).
- **경고하고 계속** — 모르는 최상위 키·모르는 단축키 필드·값의 모양이 다른 필드(타입 불일치, 고정 배열 길이 차이)·읽히지 않는 override 항목·이 빌드보다 높은 버전. 전부 `BundleWarning` 목록으로 돌려준다.
- **버리고 경고** — 이 환경에 **설치되지 않은 plugin** 의 override 전부, 대상이 없는 `script_bindings` 항목. 설치 여부와 script 존재 여부는 `DecodeEnv` 로 받고, `known_script_ids: None` 이면 script 판정을 건너뛴다(레지스트리를 못 보는 호출자가 전량을 잃지 않게).

`[keybindings]` 는 **필드 단위로** 복원한다 — 기본값에서 출발해 번들의 필드를 하나씩 얹고, 얹은 뒤 전체가 역직렬화되지 않으면 그 필드만 되돌린다. 그래서 구버전·신버전 번들의 배열 길이 차이가 그 필드 하나만 기본값으로 만들고 나머지는 그대로 복원된다. 코덱에는 필드 명부가 없다.

import 한 구성이 macOS 에서 만들어졌으면 `option` 바인딩이 이 환경에서 조용히 죽는다. 그 자리를 전수로 찾아 대체 값을 적용하는 계산이 같은 자리에 있다(`keybinding_bundle::option_migration`) — 찾는 다섯 자리·대체 값의 종류·충돌 검사 규칙은 [key-mapping](../../design/policies/key-mapping.md) "이식 시 `option` 처리" 가 정본이다.

결정의 근거·대안·재검토 조건은 [ADR-0257](../../adr/0257-the-keybinding-bundle-is-a-toml-file-with-a-schema-tag.md).

### webview surface(markdown/html)에서의 단축키 — native 자식 창에서 host 로 포워딩

`rendering = "webview"` kind(`markdown`·`html`)는 host 의 wgpu 표면이 아니라 **OS 자식 창/뷰**
위에 그려진다(X11 child window + WebKitGTK / WKWebView subview / child HWND + WebView2).
그 자식이 키보드 입력을 받으면 winit 최상위 창은 `WindowEvent::KeyboardInput` 을 아예 받지
못하므로, 아무 조치가 없으면 그 상태에서 사용자가 설정한 단축키가 통째로 죽는다. 그래서 세
백엔드가 native 키를 가로채 host 로 올린다([ADR-0102](../../adr/0102-webview-key-forwarding.md)).

- **계약은 한 곳**: 백엔드는 자기 native 키 표현(GDK keyval / NSEvent
  `charactersIgnoringModifiers` / Win32 VK)을 winit `Key`+`ModifiersState` 로 정규화해
  `WebViewKeyEvent` 로 만들고 창마다 하나뿐인 `WebViewKeyBridge` 에 올린다
  (`src/host_api/webview/keys.rs`). "host 가 가져가는가" 판정은 그 브리지에서만 하고,
  판정 자체는 콜백 안에서 **동기**라 백엔드가 페이지 전파를 그 자리에서 막는다. 실제 액션은
  host 가 다음 프레임에 큐를 비우며 실행하므로 페이지와 host 가 같은 키를 이중 처리하지 않는다.
- **가져갈 콤보는 `KeybindingSettings` + plugin 명령 레지스트리에서 전량 도출**한다 —
  고정 액션 필드 + quick-switch 3 축 합성 콤보 + 스크립트 바인딩 + **활성** plugin
  명령의 effective binding(매니페스트 `default_keybinding` / 사용자 override / host 액션
  상속). 포워딩 계층에 키 리터럴은 없다. 도출은 단축키 계층
  (`src/adapters/ui/input/shortcuts/webview_claims.rs`)이 하고, 브리지의 정책은 그 결과인
  콤보 목록만 받는다([ADR-0385](../../adr/0385-webview-backends-receive-their-host-contract-by-injection.md)).
- **plugin 바인딩은 scope 로 미리 거르지 않되, 비활성 plugin 은 제외한다** — 스냅샷은
  활성 plugin 의 모든 명령을 담는 상위집합이다(키가 host 에 도착하는 시점의 모델 포커스를
  브리지가 claim 시점에는 알 수 없다). 비활성 plugin 명령은 발화 자체가 불가능하므로 claim
  하면 키가 페이지에도 host 에도 안 가고 사라진다 — `is_disabled` 로 거른다. 도달 이후의
  우선순위 판정은 winit 경로와 같은 `dispatch_plugin_shortcut_key` 가 그대로 한다 — 모델
  포커스가 어떤 plugin surface 에 있으면 그 plugin 의 명령만 후보다([key-mapping.md](../../design/policies/key-mapping.md)).
- **페이지 예약 콤보 대조는 콤보 동등성으로 한다** — plugin 매니페스트는 raw 문자열
  (`"Ctrl+F"`)이라 사용자 설정(`"ctrl+f"`)과 대소문자·modifier 순서가 다를 수 있어, 실제
  매칭과 같은 파싱 경로(`shortcuts::bindings_equivalent`)로 정규화해 비교한다. 원시 문자열
  비교면 표기만 다른 예약 콤보가 필터를 뚫어 페이지의 find/copy 가 죽는다.
- **스냅샷은 매 프레임 만들지 않는다** — `sync_webviews` 가 `KeybindingSettings` 값과 두
  epoch(`PluginCommandRegistry::revision()` / `PluginsConfig::shortcut_revision()`)를 직전
  스냅샷과 비교해 달라진 프레임에만 재생성한다. epoch 는 프로세스 전역 단조 증가
  `AtomicU64` 라, plugin 재적재로 레지스트리가 통째로 교체돼도 값이 되돌아가지 않는다.
- **비라틴 레이아웃은 physical key 폴백으로 맞춘다** — 백엔드가 하드웨어 키코드를 함께
  올리고(X11 `hardware_keycode − 8` / Win32 확장 스캔코드 / macOS `kVK_*`, 셋 다 winit
  `PhysicalKeyExtScancode::from_scancode` 표현), 브리지가 winit 경로와 **같은 함수**
  (`shortcuts::physical_key_to_logical`)로 치환한다. 치환은 `ctrl`/`super`/`alt` 중 하나
  이상이 눌린 경우에만 — 수식 없는 키는 페이지 타이핑·IME 조합 그대로다.
- **페이지에 남기는 것** 두 가지: ① `ctrl`/`alt`/`option` 중 하나도 없는 콤보(무수식·shift
  전용) — 문서 안 타이핑·IME 조합·폼 내비게이션·페이지의 Esc 를 건드리지 않기 위함.
  ② 페이지 예약 액션 `find`·`copy`·`cut`·`paste`·`select_all` — 페이지가 자체로 같은 의미를
  구현한다(markdown 의 문서 내 검색 등).
- **key-up 은 어느 백엔드도 올리지 않는다**(press 만). host 는 실려온 modifier 값만
  읽고 자기 `base.modifiers` 를 갱신하지 않으므로 modifier stuck 이 생기지 않는다.
- **auto-repeat 필터는 플랫폼이 갈린다** — macOS(`isARepeat`)·Windows(`WasKeyDown`)는 걸러
  내고, **Linux/GDK 는 걸러내지 않는다**(GTK `key-press-event` 에 repeat 플래그가 없고,
  press/release 를 직접 세는 대체 판정은 X 서버의 detectable auto-repeat 설정에 따라 정상
  press 를 삼킬 수 있다). host 단축키는 전부 edge 동작이라 반복 발화가 무해하고, winit
  터미널 경로도 repeat 를 걸러내지 않아 tasty 내 다른 경로와 일관된다.
- **모델 포커스는 클릭에만 따라간다** — 백엔드가 native 클릭/포커스 획득을 통지하면 host 가
  `focused_pane`/`focused_surface` 를 맞춘다. 키 도착은 근거로 쓰지 않는다(X11 은 포인터가
  자식 창 위이기만 해도 키를 넣으므로, 키를 근거로 삼으면 focus-follows-mouse 가 된다).
- **overlay 개폐**: egui overlay 가 열려 webview 를 숨길 때 키보드 포커스를 host 창으로
  회수한다(숨김과 포커스 해제는 세 OS 모두 별개). 닫힐 때 자동 복원은 하지 않는다.
  회수는 **창이 활성(`base.focused`)이고, 포커스가 실제로 그 webview 자식 안에 있을 때만**
  한다 — overlay 는 IPC 로도 열리므로 무조건 회수하면 tasty 가 다른 앱의 OS 키보드
  포커스를 빼앗는다(불가침 원칙 1).
- **폴링 tick 은 Linux 에서만 세워진다.** GDK 는 winit 과 다른 X 연결로 이벤트를 받아
  루프를 깨우지 못해, 드러난 webview 가 있고 창이 활성인 동안만 16ms tick 으로 GTK 를
  펌프한다. macOS/Windows 는 native 키 콜백이 winit 과 같은 이벤트 루프에서 발화해 폴링이
  필요 없다. **이 한정은 조건부 컴파일이 아니라 런타임 arm 분기다** — `Tick::WebviewKeyPoll`
  과 `WEBVIEW_KEY_POLL_INTERVAL`(`src/app/timers.rs`)은 `#[cfg(feature = "gui")]` 로만 게이트돼
  세 OS 모두 컴파일되고 (출처: 세 OS **컴파일** 채널 — `.github/workflows/crossplatform-check.yml` 의 `check-macos`·`check-windows`·`check-headless`. **런타임 동작은 그 채널 밖이다** — integration·e2e 는 Linux 뿐이다), `reschedule_webview_key_poll` 의
  `let arm = needs_poll && cfg!(target_os = "linux")` 가 Linux 에서만 tick 을 세운다(non-Linux 는
  `hub.cancel` 만 탄다). `cfg!` 은 컴파일타임 상수라 접히므로 다른 두 OS 의 폴링 비용은
  실질 0 이다. `#[cfg(target_os = "linux")]` 가 실제로 걸린 곳은 GTK 펌프 호출
  (`src/app/webview_keys.rs` 의 `pump_gtk_events()`) 하나뿐이다.
- **claim 되고도 소실되는 콤보가 있다** — host 가 가져간 뒤 그 프레임의 게이트(모달/무대/
  kind, plugin 명령이면 포커스된 plugin 게이트)가 거절하면 키는 페이지에도 가지 않는다.
  포워딩이 만든 손실이 아니라 기존 우선순위 규칙이 드러난 것이다(같은 상황을 wgpu 경로에서
  재현해도 결과가 같다).
- 실행 검증은 Linux/X11 만 됐다. Windows 는 `cargo check --target x86_64-pc-windows-gnu`
  타입 검증까지, macOS 는 로컬 크로스 툴체인이 없어 컴파일 검증도 하지 못했다 — 둘 다
  실기 미검증이다. 스캔코드 → `PhysicalKey` 변환도 같은 경계다(Linux 만 실측, 나머지 둘은
  winit 구현 대조까지). 비라틴 레이아웃 실기 전환 확인도 Linux/X11 에서만 했다.

### 오버레이가 열려 있을 때 — 단축키는 **전부** 막히고, Escape 가 푼다

설정 창 · 입력 dialog · **포커스된 host popup** · plugin popup 중 하나라도 해당하면 키는
단축키 매처에 **진입하지 않는다**(`view/main/keyboard.rs` 의 `keyboard_overlay_open`).
막히는 것은 어떤 부류가 아니라 **바인딩 테이블 전체**다 — 검색창에 글자를 치는데 `alt+w`
(기본 프리셋의 close_surface)가 탭을 닫아버리면 안 되기 때문이고, 어떤 액션이 "안전한가" 는
프리셋마다 달라 부류로 가를 수 없다. host 로 키를 포워딩하는 webview 경로도 같은 게이트를
먼저 본다.

게이트보다 **앞**에서 도는 것은 셋뿐이다: 전체화면 무대의 종료 키 · double-tap · **Escape**.

Escape 는 순서대로 세 가지를 본다.

1. 설정 창이 열려 있으면 닫는다.
2. 알림 패널이 열려 있으면 닫는다.
3. **포커스된 host popup 이 있으면 푼다** — 포커스를 놓고, `close_on_outside_click` 인
   popup 만 닫는다.

셋째는 새 정책이 아니라 **바깥 클릭에 이미 있던 의미의 두 번째 입구**다. 바깥을 클릭하면
non-sticky popup 의 포커스가 풀리고 `close_on_outside_click` 인 것은 닫히는데, 그 길이
마우스에만 있었다. 그래서 키보드만 쓰면 포커스된 popup 을 푸는 수단이 없었다.

범위는 바깥 클릭보다 좁다 — **포커스된 하나만** 본다. 바깥 클릭은 좌표를 가지므로 "그 점을
안 담은 popup 전부" 를 가리킬 수 있지만 Escape 에는 좌표가 없다. 좌표 없는 키를 같은 범위로
쓰면 사용자가 가리킨 적 없는 popup 까지 닫힌다. 열려 있어도 **포커스가 없는** popup 은 그대로
남는다.

앞의 둘이 먼저인 이유: 그 둘은 포커스와 무관하게 **열려만 있으면** 먹는다. 셋째를 앞으로
올리면 설정 창이 떠 있는 채로 다른 popup 이 포커스를 가질 때 Escape 가 설정을 안 닫는다.

### 편집 — 녹화 + 충돌

Settings Keybindings 탭에서 키 조합을 직접 **녹화**해 할당한다. 충돌(같은 조합이 다른 액션에 이미) 시 확인 팝업으로 수락/거부. 편집은 draft 에 쌓이고 Save 시 커밋(`crud.rs`). quick-switch 슬롯의 bare-key 녹화·충돌 흐름은 위 [quick-switch 섹션 UI](#quick-switch-섹션-ui-tabworkspace-서브탭) 참조.

### 프리셋

키바인딩 **프리셋**(기본 세트 전환)을 제공한다(`keybindings/presets.rs`). 레이아웃 프리셋(`tasty-presets` crate, Workspace/Tab/Pane)과는 별개 — 이름만 비슷한 다른 시스템이다.

Settings › Keybindings › **Preset** 서브탭은 **drill-down**(content-swap) 구조다
(`src/view/settings/ui/keybindings_tab/preset.rs`, 공용 위젯
`tasty_ui_widgets::{DrillDown, ListCtrl}` — 디자인 `settings_window.jsx` `PresetSubtab`):

- **List view** — 풀폭 `ListCtrl` 프리셋 목록(Tasty / Mac / Windows / Linux). 각 행: 이름 + 한 줄 설명 + drill-in chevron. 현재 draft 와 모든 일반 바인딩이 일치하는 **사용 중 프리셋**에 trailing "Active" Tag(success·dot) + selected 하이라이트(2px accent 바).
- **Detail view** — 행 클릭 시 콘텐츠 전체 교체(0ms): back bar(← + "{이름} preset" 제목 + **우측 Apply**) 아래 Action / Current / {프리셋} 3열 diff 테이블 — 변경 행은 accent-primary 강조. ← 로 목록 복귀.
- **Apply 범위** — Apply 는 선택 프리셋을 settings **draft** 에 기록(사용 중 프리셋이면 "Applied" 비활성 — 적용할 diff 없음), footer Save 가 draft 전체를 디스크에 커밋. 두 버튼은 물리적으로 분리(back bar vs footer).
- 이 서브탭은 표준 콘텐츠 패딩/스크롤 래퍼를 우회한 **full-bleed** — DrillDown 이 자체 패딩과 내부 스크롤을 소유한다.

### 프리셋 바인딩 — 네 프리셋의 기본값

tasty 는 4개 프리셋을 제공한다. 각 프리셋은 **바인딩 문자열의 집합**이고, 그 문자열이 실제 물리 키에 매핑되는 방식은 [key-mapping](../../design/policies/key-mapping.md) 의 OS별 레이어가 결정한다. 편집은 위 [편집 — 녹화 + 충돌](#편집--녹화--충돌), 프리셋 전환 화면은 바로 위 [프리셋](#프리셋).

| 프리셋 | 참고 | 특징 |
|--------|------|------|
| **Tasty**(기본) | 자체 | 자체 키 조합 |
| **Mac** | iTerm2/Terminal.app | `alt+`(=⌘) 중심 |
| **Windows** | Windows Terminal | `ctrl+shift+` 중심 |
| **Linux** | GNOME Terminal | `ctrl+shift+` 중심 |

프리셋은 **권장**이지 플랫폼 강제가 아니다 — Mac 프리셋의 `alt+c` 는 macOS 에서 ⌘C, Windows 에서 Alt+C 로 동작한다([key-mapping](../../design/policies/key-mapping.md) 의 위치 기반 매핑).

#### 전체 바인딩 비교 (빈 칸 = 바인딩 없음)

##### 생성/닫기
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| new_window | alt+shift+n | alt+shift+n | ctrl+shift+n | ctrl+shift+n |
| new_workspace | alt+n | alt+n | alt+n | alt+n |
| new_tab | alt+t | alt+t | alt+t | alt+t |
| close_active | ctrl+w | | ctrl+w | ctrl+w |
| close_pane | ctrl+shift+w | alt+shift+w | ctrl+shift+w | ctrl+shift+w |
| close_workspace | alt+shift+w | | alt+shift+w | alt+shift+w |
| restore_closed | ctrl+shift+t | ctrl+shift+t | ctrl+shift+t | ctrl+shift+t |

##### 분할 / 포커스
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

##### 클립보드 / 줌
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| copy | ctrl+c, alt+c, ctrl+shift+c | alt+c | ctrl+c | ctrl+shift+c |
| paste | ctrl+v, alt+v, ctrl+shift+v | alt+v | ctrl+v | ctrl+shift+v |
| zoom_in/out/reset | ctrl/alt 계열 다중 | alt 계열 | ctrl 계열 | ctrl 계열 |

> Tasty 프리셋 copy 는 세 관례를 다 바인딩 — `ctrl+c` 는 선택 있으면 복사, 없으면 SIGINT([clipboard](../clipboard/index.md)).

##### UI 토글 / 종료 / 변환
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
> 기본값으로 되돌리려면 프리셋을 재적용한다([key-mapping](../../design/policies/key-mapping.md#바인딩-문자열-문법)).

> `quit_immediate` 는 실수 방지로 전 프리셋 기본값 없음. `apply_*_preset`(레이아웃 프리셋 적용)도 사용자가 직접 배정하도록 기본값 없음. Windows 는 Alt+F4 가 OS 종료라 `quit` 불요.

> 이 표는 호스트 `KeybindingSettings` 프리셋 필드만 다룬다. 클립보드 뷰어(`toggle_clipboard_viewer`)·git
> viewer 등 플러그인 커맨드의 단축키는 각 플러그인 매니페스트의 `[[contributes.commands]]`
> `default_keybinding`으로 선언되며 프리셋과 무관하다 — [plugin-development](../../dev-guide/plugin-development.md#단축키-commands) 참조.

#### 설계 원칙

1. **수정자 계층** — Mac: `alt`(⌘)=주요 동작 / `alt+shift`(⌘⇧)=보조 / `ctrl`(⌃)=pane 포커스. Windows·Linux: `ctrl+shift`=주요 / `ctrl`=보조/줌 / `alt`=분할·surface 포커스.
2. **tasty 고유 기능**(Pane/Surface 분할·Workspace·변환)은 각 프리셋 수정자 계층에 맞추되 가능하면 문자 키 유지(`d`=분할, `e`=pane 분할).

### 가져오기 / 내보내기

Settings › Keybindings › **Import / Export** 는 위 [이식 번들](#이식-번들--구성-전량을-파일-한-장으로)을 파일로 쓰고 읽는 화면이다(`src/view/settings/ui/keybindings_tab/import_export.rs`, 행 모델 `import_export/model.rs` — 디자인 `kb_import_export.jsx`). L2 목록 **맨 끝**에 있고 위에 separator 가 붙는다(필터 검색 중에는 separator 를 숨긴다). Preset 과 같은 **full-bleed drill-down** 이다.

- **진입 화면** — 안내문 + 행 둘: **Export**(secondary) · **Import**(primary).
- **내보내기** — 설정 창의 로컬 파일 선택([native-file-picker](../native-file-picker/index.md#설정-창에서의-로컬-전용-재사용))을 저장 모드로 연다. 기본 파일명 `tasty-keybindings-<YYYY-MM-DD>.toml`, `.toml` 필터. 내보내는 원본은 **현재 draft**(저장 전 편집 포함)와 `plugins.toml` 의 override 전량(`PluginsConfig::shortcut_overrides`)이다. 쓰기에 성공하면 설정 창 자체 toast(Success)가 경로를 보여 준다. 쓰기에 실패하면 **Export 행 안**에 danger 알림 블록(경로 · 이유 · "Nothing was written.")이 뜨고 **Try again**(같은 경로로 다시 쓰기) · **Choose another location…**(블록을 닫고 저장 선택기를 다시 연다)을 싣는다. 블록이 떠 있는 동안 행의 **Export…** 버튼은 꺼진다 — 재시도 입구가 둘이 되지 않게. 실패는 toast 가 아니다: toast 는 저절로 사라지고 재시도를 못 싣는다. 이유 구절은 **고정 집합 넷**이다 — 읽기 전용 파일시스템(`ie_export_failure_read_only`) · 권한 없음(`_denied`) · 볼륨이 참(`_disk_full`) · 나머지를 접는 catch-all(`_unknown`, "the write didn't finish."). 집합이 고정인 이유는 그 구절이 세 언어에서 같은 문장으로 읽혀야 하기 때문이고, 그래서 **OS 가 낸 문장은 문장 안에 들어가지 않는다** — catch-all 갈래에서만 본문 아래 제 줄(mono caption · muted · 한 줄 말줄임 · 전문은 tooltip)로 따라붙는다. "볼륨이 참" 은 `io::ErrorKind::StorageFull` 이 아직 nightly 라 raw OS 코드로 본다(`db::disk_full_os_error`, db 초기화 쪽과 같은 상수). 로그(`tracing::error!`)도 함께 남는다.
- **가져오기** — 같은 선택기를 열기 모드로 열어 고른 파일을 `decode` 한다(`DecodeEnv` = 이 환경에 설치된 plugin id · 스크립트 레지스트리). 성공하면 detail 로 들어간다.
  - **파싱 실패**(`Toml`·`NotABundle`) — detail 에 danger 인라인 블록(파일 이름 · TOML span 에서 계산한 줄 번호)과 "다른 파일 고르기" 가 뜨고 back bar 액션은 숨는다. 줄 번호를 못 얻으면 줄 구절을 빼지 않고 "the file isn't TOML." 로 **바꾼다** — 가운데 문장은 언제나 이유를 말하고 앞뒤 문장은 두 형태에서 같다.
  - **버린 plugin override**(`DroppedUninstalledPlugin`) — 경고가 아니라 muted 정보 줄 하나(개수 · plugin id 목록).
  - **번들 경고** — 경고 톤 알림 블록 **하나**에 한 줄씩 쌓고 헤더에 개수를 단다. 개수가 **하나면 단수형**이다 — 헤더(`ie_notices_count_one`)도, 모르는 액션 줄(`ie_notice_unknown_actions_one`)도. 굴절하는 것은 영어뿐이지만 키는 세 언어에 다 있다: 한 자리가 언어마다 다른 갈래를 타면 문구가 갈린다. 순서는 원문 순서가 아니라 고정이다: 새 스키마(`NewerVersion`) → 모르는 액션(`UnknownKeybindingField`, 몇 개든 한 줄에 이름 목록) → 빈 그룹. 세 줄까지 보이고 나머지는 **Show {n} more** 뒤로 접힌다(`import_export/bundle_notices.rs`). 줄이 하나면 접을 것이 없어 그 링크가 서지 않는다. 정보 줄은 이 블록에 들어가지 않는다 — 정렬 키가 주제가 아니라 톤이다. 화면 문구가 정해지지 않은 경고(`UnknownTopLevelKey` · `KeybindingFieldShapeMismatch` · `KeybindingsUnreadable` · `UnreadablePluginOverride` · `PluginOverridesUnreadable` · `DroppedUnknownScriptBinding`)는 로그(`tracing::warn!`)로만 남는다. "빈 그룹" 줄은 디자인 문구가 있지만 그것을 내는 경고가 코덱에 없다.
- **diff 표** — 선두 **선택 열** + Action / Current / Imported. 행은 네 그룹이고 그룹마다 **헤더 행**(그룹 전체 선택 · 접기 chevron · 그룹명 · `N changed · M total`)이 붙는다.
  - **General** — 일반 콤보 필드 하나가 한 행(콤보 목록 전체).
  - **Quick switch** — 축(tab · workspace · category) 하나가 한 행. modifier · 슬롯 전부 · 다음/이전을 함께 옮긴다 — 슬롯은 raw 키라 modifier 와 떨어지면 뜻이 바뀐다.
  - **Scripts** — 현재와 번들의 **합집합**. 번들에 없는 현재 바인딩은 적용하면 사라지는 행으로 보인다.
  - **Plugin overrides** — **번들에 있는 명령만**. 이 환경에만 있는 override 는 표에 오르지 않고 바뀌지도 않는다.
  - 기본은 변경된 행만 보인다(back bar 의 **Show all {n}** / **Changed only** 토글). 선택은 해제한 행을 기억하는 방식이라 처음에는 모든 행이 선택돼 있다 — 변경 없는 행은 적용해도 값이 같다.
- **option 마이그레이션 카드** — 비-macOS 에서 번들에 `option` 바인딩이 있으면 표 위에 카드가 뜬다. 자리마다 대체 값을 정한다: 콤보 자리는 녹화 슬롯, 축 modifier 자리는 7 조합 중 선택(안 고른 상태는 값이 아니라 placeholder **Select a modifier** — UI 폰트 · `text_placeholder` 색, 고르면 목록에서 빠진다 — `select_or_placeholder`). 콤보 자리는 **Leave unbound**(비워 두기)도 해소로 센다 — 축 modifier 는 비울 수 없다. 대체 값이 새 충돌을 만들면 행 아래에 충돌 상대가 표시된다(행마다 언제나). 충돌 행이 **2 개 이상**이면 카드 설명 아래에 개수 줄(`{n} conflicts` danger 강조 + 적용 시 충돌 확인이 뜬다는 문장)이 먼저 선다 — 목록을 되풀이하지 않는다. **미해결이 하나라도 있으면 Apply 가 비활성**이고, back bar 에 `{n} unresolved` 가 뜬다 — 선택 여부와 무관하게 번들 전체에 대해 요구한다.
- **Apply** — 고른 행만 settings draft 와 `plugin_shortcuts_draft` 에 쓴다(`apply_rows`). 마이그레이션 해소는 `resolve_migration` 이 한다. 해소된 번들 안에서 새 충돌이 생기면 설정 창의 충돌 확인 popup 이 뜨고, **덮어쓰기**를 고르면 충돌 상대 중 계획 밖의 자리를 비우고 적용한다(`ConflictPolicy::UnbindOther`). 적용되면 toast 로 알린다. 디스크 커밋은 footer **Save** 가 한다(Preset 과 같은 2 단계).
- **Cancel** — 설정 draft 와 함께 `plugin_shortcuts_draft` 도 버린다. plugin draft 는 **Save 로 닫혔을 때만** 적용된다 — 창 닫기·`toggle_settings` 로 닫혀도 버린다. Plugins 서브탭 편집도 같은 규칙이다.

결정의 근거·대안·재검토 조건은 [ADR-0269](../../adr/0269-keybinding-import-applies-selected-rows-onto-the-draft.md).

### 설정 탭 구성 (서브탭·항목 순서)

Settings 의 Keybindings 탭은 액션을 서브탭으로 묶고, 그 순서는 **유비쿼터스 언어 계층**을 따른다. 서브탭 enum: `KeybindingsSubTab`(`src/view/settings/ui/keybindings_tab.rs`).

```
General → Workspace → Pane → Tab → Surface → Clipboard → Zoom → Explorer → Scripts → Preset → Plugins ─ Import / Export
          \________ 계층 순서 ________/
```

- **General / Clipboard / Zoom / Explorer**: 계층에 속하지 않는 전역·기능별 단축키.
- **Workspace → Pane → Tab → Surface**: [구조 계층](../../concepts/hierarchy.md) 순서.
- **Scripts / Preset / Plugins**: 스크립트 바인딩 · 프리셋 적용 · 플러그인 기여 단축키.
- **Import / Export**: 구성 전량의 파일 이식(항상 맨 끝, 위에 separator). [가져오기 / 내보내기](#가져오기--내보내기).

각 서브탭 *내부* 항목 순서: **① 생성/분할 → ② 탐색(next/prev/focus) → ③ 수정(rename/convert) → ④ 닫기 → ⑤ 수식키(modifier, separator 로 구분)**.

**어느 서브탭에 두는가** — 그 동작의 *대상 엔티티* 이름을 가진 서브탭에 둔다. `new_tab`→Tab, `split_pane_*`→Pane, `close_surface`→Surface. 수식키도 대상 엔티티 서브탭(`tab_switch_modifier`→Tab, `workspace_switch_modifier`·`category_switch_modifier`→Workspace). cascade 인 `close_active` 는 가장 먼저 닫히는 대상이 탭이라 Tab. `open_markdown` 은 새 탭으로 열려 Tab.

> explorer 는 host builtin kind 라 `open_explorer`(Tab)·`convert_to_explorer`(Surface) 호스트 키바인딩이 있다. 현재 Surface 의 convert 계열은 `convert_surface`·`convert_to_markdown`·`convert_to_explorer` 다.

## 인터페이스

- **사용자**: Settings → Keybindings 탭에서 녹화/편집. (단축키는 사용자 행동이라 release IPC/CLI 로 *발동* 하지 않는다 — 키 주입은 debug 전용, [debug-ipc](../../dev-guide/debug-ipc.md).)

## 비-목표

- 각 액션이 *무엇을 하는가* — 그 도메인 동작은 해당 기능 문서. 여기선 *키 ↔ 액션 매핑* 만.
- 위치 기반 modifier 매핑 규칙 — [design/policies/key-mapping](../../design/policies/key-mapping.md).

## 관련

- [ADR-0102](../../adr/0102-webview-key-forwarding.md) — webview 자식 창의 키를 host 로 포워딩하는 결정
- [design/policies/key-mapping](../../design/policies/key-mapping.md) — modifier 매핑(바인딩 문자열 → 물리 키 OS별 매핑)·OS 메뉴 key equivalent 정책
- [settings](../settings/index.md) — 편집 표면
