# 설정 시작 경로 전수 조사

저장한 설정마다 시작 경로와 실행 중 적용 경로가 같은 값을 쓰는지 조사한다. 표는 321행이고 칸마다 코드 위치가 있다. 이 문서는 조사 결과이며 제품 동작을 바꾸지 않는다. 결함은 기록만 하고 고치는 방향은 후보로만 적는다.

| 표 | 행 수 | 내용 |
|----|-------|------|
| [일반 설정](table-settings.md) | 89 | 테마 색과 단축키를 뺀 설정 키 |
| [테마 색](table-theme.md) | 104 | `theme_overrides.*`와 `theme_base.*` |
| [단축키](table-keybindings.md) | 111 | `KeybindingSettings` 필드 |
| [플러그인 설정](table-plugins.md) | 17 | 매니페스트가 선언한 값과 `plugin_settings` 형태 |

## 결과 요약

| 판정 | 행 수 | 대상 |
|------|-------|------|
| 결함 | 52 | `theme_overrides.*` 48행(D1), 글꼴 4행(D2) |
| 재시작 필요(안내 없음) | 4 | `notification.coalesce_ms`, `memory.*` 3행 |
| 재시작 필요(안내 문서 있음) | 2 | `general.language`, `webhook.allow_external` |
| 소비자 없음 | 2 | `general.confirm_close_running`, `appearance.ligatures` |
| 해당 없음 | 10 | `markdown_font`, `explorer_font`의 호환 필드 |
| 일치 | 251 | 나머지 |
| 합계 | 321 | |

D3과 D4는 특정 키가 아니라 값을 읽는 경로의 결함이다. 어느 키에서든 일어나므로 표의 행 판정에는 세지 않았다.

## 범위

대상은 `crates/tasty-settings`의 `Settings`가 `config.toml`에 저장하는 값과 번들 플러그인 매니페스트가 선언한 `plugin_settings` 값이다. `webhooks.toml`의 포트, `PluginsConfig.keybindings`, `remote-profiles.toml`은 별도 저장소라 범위 밖이다. 측정은 Linux만 한다.

칸의 뜻은 다음과 같다.

- 시작 경로: 앱을 시작할 때 값을 읽는 위치다. `resumed`에서 `boot_load_normalized_settings`가 파일을 읽어 정규화하고, `begin_boot`이 테마를 적용한 뒤 그 값을 저장한다. 그다음 엔진이 `new_with_ids`에서 파일을 다시 읽는다.
- 새 창 경로: 실행 중 새 창을 만들 때 값을 읽는 위치다. GPU와 테마는 메모리의 `latest_settings`를 쓰고, 새 엔진은 파일을 읽는다(D3). 표의 parked는 엔진을 남겨 둔 창의 복원 경로이고 미측정이다.
- 적용 경로: 설정 화면과 IPC 저장이 지나는 `apply_settings_after_structure`다.
- 소비 지점: 값을 실제로 쓰는 코드다.
- 헤드리스: `--no-default-features` 빌드의 읽기와 적용 위치다. 코드로만 판정했다.

코드 위치는 파일, 행 번호, 심볼 이름으로 적는다. 행 번호는 측정 커밋 `0459df0c8` 당시 값이며 이후 코드와 맞지 않을 수 있다. 이후에는 심볼 이름으로 찾는다. 사용자 목록(`terminal_input.rules`, `scripts.scripts`, `script_bindings`)은 항목별 동작이 같으므로 형태별로 한 행을 둔다.

## 판정 어휘

- 일치: 시작과 적용이 같은 값을 쓴다. 적용 값이 새 객체부터 반영되는 경우도 시작과 같은 값이면 여기에 넣는다.
- 결함: 시작과 적용의 값이나 효과가 다르고, 안내도 그런 동작을 말하지 않는다.
- 재시작 필요(안내 문서 있음): 재시작해야 반영되고 그 사실을 화면이나 문서가 알린다.
- 재시작 필요(안내 없음): 재시작해야 반영되는데 알리는 곳이 없다.
- 소비자 없음: 저장은 되지만 값을 읽어 동작을 바꾸는 코드가 없다.
- 해당 없음: 저장되지 않는 호환 필드라 비교할 값이 없다.
- 미측정: 재지 못한 항목이다. 판정 수단 칸에 코드로만 판정했다는 뜻으로도 적는다.

판정 수단 칸의 "측정(왕복)"은 E2로 저장과 재시작을 거친 파일 값이 유지됨을 잰 것이다. 소비자의 동작까지 잰 것이 아니며 소비자 쪽은 코드로 판정했다. "측정(개별)"은 그 키의 효과를 따로 잰 것이다. "측정(왕복, 파일만)"은 플러그인 값의 파일 왕복만 잰 것이다.

## 모수 대조

표가 모든 필드를 덮는지 두 방법으로 세어 표의 키 열과 비교했다.

- 방법 A: `crates/tasty-settings/src`의 구조체 타입을 파싱해 말단 필드를 센다. 299개다.
- 방법 B: `Settings::default()`를 TOML로 직렬화해 키를 센다. 240개이고 빈 테이블 4개(`plugin_font_overrides`, `terminal_font`, `theme_overrides`, `plugin_settings`)를 포함한다.
- 표: 키 열 321개이고 중복이 없다.

A에만 있는 69개는 다음과 같다.

| 항목 | 수 | 이유 |
|------|----|------|
| `theme_overrides.*` | 48 | `None`이면 직렬화하지 않는다 |
| `terminal_font.*`, `markdown_font.*`, `explorer_font.*` | 15 | 앞은 기본값일 때, 뒤 둘은 항상 직렬화하지 않는다 |
| `theme_base.surface_themes` | 1 | 맵이라 B에서는 하위 키로 나온다 |
| `general.option_as_meta` | 1 | 다른 OS의 `cfg` 뒤에 있다 |
| `modifier_hint.pos`, `modifier_hint.size` | 2 | `None`이면 직렬화하지 않는다 |
| `origin`, `file_observation` | 2 | 저장하지 않는 필드다 |

B에만 있는 10개는 `surface_themes`의 `terminal`, `markdown` 하위 4필드씩 8개와 빈 테이블 `terminal_font`, `theme_overrides` 2개다.

A와 B의 합집합은 309개다. 이 중 표에 키 그대로 없는 것은 8개다.

| 항목 | 표에서의 처리 |
|------|---------------|
| `plugin_font_overrides`, `theme_base.surface_themes`, `theme_overrides.surface_themes` | `<kind>.<필드>` 형태 행 3개로 대체 |
| `plugin_settings` | `<plugin_id>.<storage_key>` 형태 행과 매니페스트 행으로 대체 |
| `terminal_font`, `theme_overrides` | 빈 테이블이다. 하위 필드를 개별 행으로 적었다 |
| `origin`, `file_observation` | 저장하지 않으므로 제외 |

표에만 있는 것은 20개다. 형태 행 4개와 매니페스트 값 16개(`com.tasty.claude` 7, `com.tasty.codex` 3, `com.tasty.html` 4, `com.tasty.markdown` 2)다. 이 중 `com.tasty.markdown`의 하나는 `plugin_settings`가 아니라 `plugin_font_overrides.markdown`에 저장된다. 다른 번들 플러그인 매니페스트 5개에는 선언이 없다.

계산: 309 - 8 + 20 = 321. 표의 행 수와 같다.

## 측정

격리된 `TASTY_HOME`의 debug 인스턴스로 저장 뒤 새 창과 재시작을 쟀다. 값 변경은 debug IPC의 설정 적용을 썼고, 재시작 전후의 `config.toml`과 화면 값을 비교했다.

| 번호 | 대상 | 결과 |
|------|------|------|
| E1 | `theme_overrides.crust` | 아래 D1 |
| E1 | `default_font`의 글꼴 파일과 행 높이 | 아래 D2 |
| E2 | 모든 키를 기본값과 다르게 저장한 왕복 | 적용과 재시작 사이의 파일 차이는 5개뿐이다. 아래에 적는다 |
| E2 | `terminal_font` | 아래 D2 |
| E3 | `memory.entry_max_mb`, `webhook.allow_external` | 아래 재시작 필요 절 |
| E4 | `notification.coalesce_ms` | 아래 재시작 필요 절 |
| E5 | 저장 거절, 잘못된 값, 직접 쓴 파일 | D3과 아래 관찰 절 |
| E6 | 저장 실패 상태의 quit 모달 | D4 |
| E7 | 글꼴 파일만 지정하고 family가 빈 경우 | D2 |
| F1, F2, F3 | 파일로만 찾을 수 있는 family의 글꼴 파일 | D2 |

E2에서 적용 직후와 재시작 뒤 파일이 다른 값은 `theme_base.crust`(`#010203`에서 `#11111b`로), `theme_is_light`(true에서 false로), `theme_overrides` 테이블, `theme_overrides.crust`, `theme_overrides.surface_themes.terminal.focused_bg`다. 뒤의 세 값은 D1이다. 앞의 두 값은 비우기가 아니라 `crates/tasty-themes/src/state.rs` 79행 `apply_inner`의 테마 파일 병합에서 온다. [테마 시스템](../../design/systems/theme.md)이 `theme_base`를 앱 소유로 정한 동작이라 결함이 아니고, D1을 고쳐도 이 차이는 남는다. 기본값과 적용 직후의 비교에서는 저장한 값이 전부 반영됐다. `background_opacity`는 0.9가 `0.8999999761581421`로 기록돼 부동소수 표현만 달라졌다.

## 결함 목록

### D1. 시작과 새 창이 사용자 색을 지운다

대상: `appearance.theme_overrides.*` 48행.

`apply_theme`(`crates/tasty-themes/src/state.rs` 65행 `apply_theme`)은 `theme_overrides`를 비운다. [테마 시스템](../../design/systems/theme.md)은 이것을 테마를 바꿀 때의 동작으로 정의하고, 동기화해도 override는 보존된다고 적는다. 그런데 이 함수는 테마 전환이 아닌 경로에서도 돈다.

- 시작: `src/app/window_lifecycle.rs` 23행 `boot_apply_theme`. 이 값이 `src/app/boot_machine.rs` 116행 `begin_boot`에서 저장되므로 파일에서도 사라진다.
- 새 창: `src/app/window_lifecycle.rs` 698행 `normalize_window_settings`가 `src/app/window_lifecycle.rs` 31행 `apply_cached_theme`을 부른다. 비운 복제본으로 전역 테마를 다시 설치하므로 화면 색이 바뀐다. 보통은 복제본만 비우므로 파일에 값이 남는다. 없는 테마 ID나 값 복구로 `changed`가 참이면 `src/app/window_lifecycle.rs` 584행 `create_new_window`가 비운 복제본을 저장하므로 재시작 없이 파일에서도 사라진다. 이 분기는 코드로만 판정했다.

측정 E1(`crust`를 `#112233`으로 저장):

| 시점 | `list theme`의 `crust` | `config.toml`의 `theme_overrides.crust` |
|------|------------------------|---------------|
| 저장 전 | `#11111b` | 없음 |
| 적용 직후 | `#112233` | `#112233` |
| 새 창을 연 뒤 | `#11111b` | `#112233` |
| 재시작 뒤 | `#11111b` | 사라짐 |

E2에서는 `surface_themes.terminal.focused_bg`도 같이 사라졌다.

고치는 방향 후보: 재적용(비우지 않음)과 테마 전환(비움)을 다른 함수로 나눈다. 또는 비우는 동작을 전환 호출자로 옮긴다. 전환 호출자는 `src/view/settings/ui/tabs/appearance.rs` 100행 `apply_theme`을 부르는 설정 화면, `src/adapters/ui/status_bar.rs` 104행 `apply_theme`을 부르는 상태 바, debug 재적용이다.

### D2. 글꼴 파일과 행 높이가 시작 때 적용되지 않는다

대상: `appearance.default_font`와 `appearance.terminal_font`의 `custom_font_path`, `line_height` 4행.

`src/gfx/renderer/pipeline.rs` 14행의 `FontConfig::with_options(font_size, font_family, "", 1.0)`이 두 값을 고정값으로 생성한다. `src/gfx/gpu.rs` 269행 `last_term_font_sig`는 저장값으로 서명을 계산한다. 그래서 `src/gfx/gpu/egui_bridge.rs` 236행의 비교가 달라진 것이 없다고 보고 재구성을 건너뛴다. 값은 파일에 남지만 렌더러에 들어가지 않는다.

측정 E1(글꼴 파일 `DejaVuSansMono.ttf`, 행 높이 1.5, 측정 환경의 칸 값). 이 글꼴은 기본 글꼴과 칸 폭이 7로 같아서 아래 값은 행 높이의 소실만 보여 준다. 파일의 소실은 F1과 F2로 구분한다.

| 시점 | 칸 | 격자 |
|------|----|------|
| 저장 전 | 7x14 | 157x45 |
| 적용 직후 | 7x21 | 157x30 |
| 새 창 | 새 창의 칸은 재지 않음 | 새 창 157x45, 첫 창 157x30 |
| 재시작 뒤 | 7x14 | 157x45 |

측정 F1~F3. 시스템 글꼴 목록에 없는 고정폭 파일 `KaTeX_Typewriter`를 썼다. family가 같은 이름이므로 파일이 없으면 이름을 찾지 못하고 대체 글꼴로 그린다. `family만`은 파일 없이 family만 준 대조다.

| 번호 | 입력 | 적용 직후 | 새 창 격자(첫 창은 적용 직후 값) | 재시작 뒤 |
|------|------|-----------|----------------------------------|-----------|
| F1 | `default_font` family, 파일, 행 높이 1.5 | 8x21, 137x30 | 84x45 | 13x14, 84x45 |
| F2 | `terminal_font` family, 파일, 크기 14, 행 높이 1.0 | 8x14, 137x45 | 84x45 | 13x14, 84x45 |
| F3 | `default_font` family만 | 13x14, 84x45 | 84x45 | 13x14, 84x45 |

F1과 F2의 재시작 뒤 값이 F3과 같으므로 파일이 렌더러에 들어가지 않았다. 파일에는 값이 그대로 남아 있었다. 칸 폭 13은 이 컴퓨터의 대체 글꼴 값이며 다른 컴퓨터에서는 다를 수 있다. 처음에 쓴 다른 파일(`decker.ttf`)은 파일을 주어도 칸 폭이 대체 글꼴과 같은 13이어서 구분하지 못해 결과로 쓰지 않았다.

측정 E2(`terminal_font` 크기 16, 행 높이 1.25): 적용 직후 10x20, 106x31이고 재시작 뒤 10x16, 106x39다. family와 크기는 유지되고 행 높이만 빠진다.

측정 E7(글꼴 파일 `DejaVuSans-Bold`만 지정하고 family는 빈 값): 칸과 격자가 7x14, 157x45로 변하지 않았다. `crates/tasty-font/src/lib.rs` 94행 `reconfigure`는 파일을 등록만 하고 family가 비면 묶음 D2Coding을 고른다. family `DejaVu Sans`를 함께 주면 13x14, 84x45가 된다. 칸 폭은 글꼴에 따라 다르다. 같은 입력의 설정 화면 미리보기는 재지 않았다. 코드로는 `src/adapters/ui/font_registry.rs` 154행 `register_custom_font`가 파일을 읽어 미리보기에 쓰므로 터미널과 다른 글자가 나올 것으로 본다. 미리보기와 터미널의 글자 차이는 미측정이다.

고치는 방향 후보: 렌더러를 만들 때 재구성과 같은 입력(family, 파일, 행 높이)을 넘긴다. 파일만 지정한 경우의 결과는 별도로 정한다. 측정 커밋에는 이 수정이 들어 있지 않다.

### D3. 새 엔진이 메모리 값 대신 파일 값을 읽는다

대상: 키와 무관한 읽기 경로.

새 창의 GPU와 테마는 `src/app/window_lifecycle.rs` 578행 `create_new_window`의 `latest_settings`를 쓴다. 엔진은 `src/app/window_lifecycle.rs` 261행 `ensure_engine_and_plugins`에서 `src/runtime/engine_session/bootstrap.rs` 51행 `new_with_ids`를 불러 파일을 읽는다. 저장에 실패했으면 한 창 안에서 두 값이 갈린다.

측정 E5: 외부에서 파일을 바꾼 뒤 `coalesce_ms`를 적용했다. 응답은 `Error (-32603): settings applied for this run but not saved`이고 데이터는 `{"applied":true,"saved":false}`다. 파일 값은 500으로 남았고, 새 창의 엔진은 메모리 값이 아니라 파일 값 500으로 동작했다(2초 간격 알림 두 건이 따로 표시). 저장이 성공한 경우의 대조에서는 병합됐다. 저장 대기 중 경합은 미측정이다.

고치는 방향 후보: 새 창 경로가 `src/runtime/engine_session/bootstrap.rs` 56행 `for_journal`로 메모리의 최신 값을 넘긴다. `new_with_ids`는 이 함수에 `Settings::load()`를 넘길 뿐이다. 같은 파일의 `new_with_ids_and_settings`는 `cfg(test)`라 제품 코드에서 부를 수 없다.

### D4. quit 모달이 파일 값으로 전역 테마를 다시 설치한다

대상: 키와 무관한 읽기 경로.

`src/app/modal/quit.rs` 94행 `open_quit_modal`의 `Settings::load().appearance`가 GPU를 만들면서 전역 테마를 파일 값으로 덮어쓴다. 다른 모달은 포커스된 엔진의 값을 먼저 쓰고 파일은 대체 경로로만 쓴다(`src/app/modal/settings.rs` 97행, `src/app/modal/plugins.rs` 42행, `src/app/modal/preset.rs` 73행).

측정 E6: 저장 실패 상태에서 `crust`가 `#445566`일 때 quit 모달을 열면 `#11111b`가 된다.

고치는 방향 후보: 다른 모달처럼 포커스된 엔진의 값을 먼저 쓴다.

## 재시작이 필요한 값

### 안내 없음

- `notification.coalesce_ms`: `src/runtime/engine_session/bootstrap.rs` 103행 `assemble`에서 저장소에 복사하고 적용 경로는 갱신하지 않는다. 측정 E4: 60000을 적용한 뒤 같은 엔진에서는 2초 간격 두 건이 따로 표시됐다. 새 창의 엔진과 재시작 뒤에는 병합됐다. 방향 후보: 적용 때 저장소를 갱신하거나 안내를 추가한다.
- `memory.entry_max_mb`, `memory.secret_quota_mb_per_plugin`, `memory.regular_quota_mb_total`: `src/boot.rs` 231행과 `src/boot/headless_memory.rs` 10행에서 한 번 읽는다. 측정 E3: `entry_max_mb`를 2로 적용한 뒤 1.5MiB 저장은 `value_too_large: 1572864 bytes > 1048576`으로 거절됐고 재시작 뒤에는 성공했다. 나머지 두 키는 코드로 판정하고 왕복만 쟀다. [메모리 설계](../../design/systems/memory.md)에 재시작 안내가 없다. 방향 후보: 적용 때 저장소 한도를 갱신하거나 안내를 추가한다.

### 안내 있음

- `general.language`: 설정 화면 안내 `settings.general.language_restart_notice`가 있다. 화면 효과는 미측정이다.
- `webhook.allow_external`: 설정 화면 안내 `webhook_allow_external_notice`가 있다. 측정 E3: 적용 직후 `127.0.0.1:28430`이고 E2 재시작 뒤 `0.0.0.0:28430`이다.

## 소비자 없는 값

`general.confirm_close_running`과 `appearance.ligatures`는 설정 화면(`src/view/settings/ui/tabs/terminal.rs` 107행, `src/view/settings/ui/tabs/appearance.rs` 271행)만 쓴다. 동작을 바꾸는 읽기가 없다. 방향 후보: 동작에 연결하거나 설정을 없앤다.

## 조사 단서별 판정

| 단서 | 판정 |
|------|------|
| 메모리 저장소 | 재시작 필요(안내 없음). 시작 때 한 번 읽는다 |
| 웹훅 리스너 | 재시작 필요(안내 있음) |
| GUI와 헤드리스의 정규화 | GUI의 `boot_settings`(`src/boot.rs` 214행)는 정규화 전 값이지만 거기서 읽는 webhook, memory, scripts는 정규화 대상이 아니라 갈리지 않는다. 헤드리스는 `src/boot.rs` 858행 `load_headless_settings`가 정규화한다. 헤드리스는 코드로만 판정했다 |
| 로드 복구와 적용 거절 | 범위가 다르다. 아래 관찰에 적는다 |
| 메모리 값과 파일 값 | D3, D4 |
| 이전 값과 비교해 발화하는 경로 | `last_term_font_sig`만 결함이다(D2). `appearance_changed`는 첫 설치가 시작 때 끝나서 영향이 없다. `prev_language`와 `prev_keybindings`는 적용 때 한 번 비교해 플러그인 이벤트를 낼지만 정한다(`src/app/dispatch_domain.rs` 726행 `announce_settings_change`, `src/app/settings_change.rs` 259행 `announce_shortcut_change`). 값 자체를 정하지 않으므로 시작 때 발화하지 않아도 값은 달라지지 않는다. webview 정책과 글꼴 레지스트리 서명은 프레임 비교다. `src/adapters/ui/font_registry.rs` `refresh_surface_fonts`는 첫 호출에서 반드시 적용한다 |
| macOS 메뉴 | 코드로만 판정했다. `src/app/dispatch_domain.rs` 690행 `apply_settings_after_structure`의 `rebuild_main_menu`가 적용 때 다시 만든다. 미측정이다 |

## 파일을 다시 읽는 그 밖의 경로

| 경로 | 판정 |
|------|------|
| `src/boot/home_in_use.rs` 80행 `create_gpu` | 영향 없음. 정규화 없이 파일을 읽지만 엔진이 없는 오류 안내 창의 GPU에만 쓰고 저장하지 않는다. `apply_theme`도 돌지 않는다. 코드로만 판정했다 |
| `src/app/event_handler.rs` 993행 `finish_shell_setup_confirmed` | 일치. 파일을 읽어 정규화하고 확인한 셸 경로만 바꿔 `begin_boot`으로 넘긴다. 일반 시작과 같은 경로라 D1도 같이 적용된다. 코드로만 판정했다 |
| `src/app/modal/preset.rs` 79행 `focused_keybindings_or_disk` | 포커스된 엔진이 없을 때만 파일을 읽는다. 저장에 실패한 상태와 겹칠 때만 메모리 값과 달라진다. 같은 파일의 73행 `focused_appearance_or_disk`, `src/app/modal/settings.rs` 97행 `resolve_settings_init_data`, `src/app/modal/plugins.rs` 42행도 같다. 코드로만 판정했고 그 겹침은 미측정이다 |
| `crates/tasty-platform/src/macos_delegate.rs` 175행 `inject_delegate_methods` | 시작 때 파일에서 메뉴를 한 번 만들고 적용 때는 메모리 값으로 다시 만든다. 코드로만 판정했고 미측정이다 |

## 관찰

표의 판정을 바꾸지는 않지만 시작과 적용의 차이로 확인한 사실이다.

- 정규화와 거절: 측정 E5에서 debug IPC로 잘못된 선택 값을 저장하면 파일에 남고, 재시작 때 `general.close_behavior`는 ask, `general.link_click_modifier`는 ctrl, `general.explorer_view_mode`는 detail, `appearance.ui_scale`은 medium, `performance.scrollback_disk_lines`는 10000으로 돌아간다. 실행 중 `font_scale_mode`의 잘못된 값은 fixed처럼 동작하지만(`crates/tasty-settings/src/appearance.rs` 90행 `effective_font_size`) 재시작 뒤에는 auto다. debug 경로로만 닿는다. `remote_attach.heartbeat_timeout_secs` 5의 적용은 `-32602`로 거절된다.
- 직접 쓴 파일: 측정 E5에서 범위 밖 값은 `remote_attach.heartbeat_timeout_secs` 5에서 30으로, `task_pipeline.report_append_bytes` 8에서 1024로, `anomaly_detection.rss_surge_samples` 1에서 5로 복구됐다. 같은 섹션의 유효한 `anomaly_detection.slow_loop_threshold` 값도 기본값으로 돌아간다. 섹션 전체를 초기화하는 동작이고 시험이 이 동작을 의도로 고정한다. 이전 값 `catppuccin-latte`는 `latte`로, `markdown_font`는 `plugin_font_overrides.markdown`으로 옮겨진다.
- `performance.targeted_pty_polling`: `crates/tasty-settings/src/types.rs` 9행 필드 주석은 재시작이 필요하다고 적지만 `src/runtime/engine_access.rs` 208행 `make_waker`는 새 터미널마다 값을 읽는다. 표는 코드 기준으로 일치로 판정한다.
- 프리셋 창 단축키: `src/app/modal/preset.rs` 29행 `focused_keybindings_or_disk`로 열 때 복사하므로 열린 동안의 적용은 반영되지 않는다. 코드로만 판정했다.
- 창을 닫고 새로 연 경우(IPC, 엔진을 남기지 않음): 슬롯을 복원한 새 엔진은 파일 값을 쓴다. E4 값에서 병합됐고 격자는 157x45로 첫 창의 157x30과 달랐다(D2).

## 미측정 항목

| 항목 | 이유 | 판정 근거 |
|------|------|-----------|
| `general.option_as_meta`, macOS 메뉴 | macOS는 범위 밖이다 | 코드 |
| parked 창 복원 | Linux에서 IPC로 닿는 경로가 없다. macOS 최소화와 내부 창 닫기 요청만 parked를 만든다. debug 경로가 있어야 잰다 | 코드 |
| 헤드리스 바이너리 | `--no-default-features`로 빌드하지 않았다 | 코드 |
| 플러그인 값의 플러그인 쪽 효과 | 파일 왕복만 쟀다. 플러그인을 빌드해 올리지 않았다 | 코드 + 파일 왕복 |
| 대부분 키의 소비자 동작 | 왕복은 파일 재기록만 잰다 | 코드 + 파일 왕복 |
| 저장 대기 중 새 창을 여는 경합 | 재현 수단을 만들지 않았다 | 코드 |
| `general.language`의 화면 효과 | 재지 않았다 | 코드 |
| 글꼴 파일만 지정했을 때 설정 화면 미리보기의 글자 | 터미널 칸만 쟀다 | 코드 |
| 시작 때 저장이 실패한 경로 | 재현하지 않았다. 코드상 엔진은 정규화 전 파일 값을 읽는다 | 코드 |
| 창이 없을 때의 `create_gpu_state` | 첫 창은 `boot_apply_theme`이 덮는다. 그 밖의 경우는 재지 않았다 | 코드 |

## 측정 커밋과 환경

- 커밋: `0459df0c8`, 측정일 2026-11-09.
- 빌드: debug `cargo build --locked`, 종료 코드 0.
- 환경: Linux, Xvfb 1280x800x24(뷰포트 1280x720, 배율 1.0), 격리 `TASTY_HOME`. 인스턴스마다 저장한 PID로 종료를 확인했다.

## 한계

- 일치의 대부분은 코드 판정과 파일 왕복이다. 키마다 소비자 동작을 재지 않았다.
- 표의 시작 경로와 새 창 경로는 공통 경로를 키마다 반복해 적은 것이다. 소비 지점 칸은 키 이름 검색으로 찾았으므로 간접 사용을 빠뜨렸을 수 있다.
- 사용자 목록은 형태별 한 행이라 항목별 차이를 재지 않았다.
- 칸 크기는 측정 환경의 글꼴과 크기에서 나온 값이다. 다른 환경에 그대로 옮기지 않는다.
- D3의 새 창은 IPC로 연 창이다.
