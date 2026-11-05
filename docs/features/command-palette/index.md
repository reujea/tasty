# 명령 팔레트 (Command palette)

- **Status**: Implemented
- **주체**: 로컬 사용자 (원격 접속 사용자는 mirror 로 봄)
- **설계**: 없음
- **코드**: `src/state/command_palette.rs`, `src/adapters/ui/popup/command_palette.rs`
- **화면**: [아래 절](#화면)

## 목적

VS Code 스타일 명령 팔레트. 모든 단축키 명령을 쿼리로 검색해 실행한다 — 단축키를 외우지 않아도 등록된 명령을 키보드로 실행할 수 있다. [도구 메뉴](../tools-menu/index.md) 항목이자 전용 단축키로 연다.

## 내부 동작

### 후보 목록

두 출처를 합친다(`PaletteCommand::Host` / `PaletteCommand::Plugin`):

- 호스트: `KeybindingSettings::GENERAL_BINDING_FIELDS` (단축키 설정 탭에 나타나는 모든 명령). `toggle_command_palette` 자신(이미 팔레트 안이므로)과 `fullscreen_stage_exit`(팔레트를 열 수 있는 시점엔 no-op), `copy_link`(대상이 마우스 포인터 아래 링크라 팔레트가 열린 동안 대상이 없다)는 제외(`PALETTE_EXCLUDED`).
- Plugin: `MainViewState.palette_plugin_commands` — `PluginManager::plugin_palette_commands()` 스냅샷. `[[contributes.commands]]` 로 선언된 명령 중 **`scope = "global"`만** 노출한다 — `surface` scope 는 owner plugin surface 가 포커스되어 있을 때만 의미가 있는데, 팔레트 실행 시점엔 그 컨텍스트를 보장할 수 없다(포커스 없이 매칭되는 키보드 단축키 경로 `match_global_shortcut` 과 동일 판단). 비활성 plugin 의 명령은 제외된다(`plugin_tool_items` = Tools 메뉴와 동일 필터 — 설정 UI 의 사전 키 바인딩 목적과 달리 팔레트는 "지금 실행 가능한" 명령만 보여줘야 하는 실행 UI).

### 매칭

쿼리 입력 → case-insensitive. 정확 substring(단어 시작 보너스) → 부분 시퀀스(gap 페널티) 순으로 점수화. 라벨은 호스트 `label_key` 또는 plugin `title_i18n_key` 를 `t()` 로 해석(plugin lang 네임스페이스는 discovery 시 같은 전역 resolver 에 등록되므로 별도 라우팅 불필요, tools_menu 와 동일 메커니즘).

### 탐색 / 실행

`↑/↓` 이동, `Enter` 실행, `Esc` 닫기, 클릭으로도 실행. 행 우측에 첫 번째 바인딩을 키캡으로 표시(plugin 명령은 override 해석에 `PluginManager` 접근이 필요해 — 팔레트 draw 함수는 접근 불가 — 키캡을 표시하지 않는다). 아이콘은 호스트 6개 명령만 전용, 나머지(동적 호스트 명령 + plugin 명령)는 `COMMAND` fallback.

### 실행 경로

Enter/클릭 시 `command_palette.pending_run` 에 선택된 `PaletteCommand` 적재 → `MainView::handle_redraw` 가 다음 프레임에 drain:

- `Host`: `dispatch_action_by_id` 호출 — **단축키와 정확히 같은 action body** 를 타므로 효과 동일.
- `Plugin`: `MainViewState.pending_plugin_command_invokes` 에 `(plugin_id, command_id)` 를 enqueue(팔레트 draw/redraw 경로는 `PluginManager` 에 접근할 수 없음 — `PopupDef` 고정 시그니처 제약). `App::dispatch_pending_palette_plugin_commands` 가 다음 IPC 처리 틱에 drain 해 `command_registry` 로 조회: `action` 이 있으면 `invoke_tool` 로 직접 실행(`try_plugin_shortcut` 의 action 분기와 동일 패턴), 없으면 `key_dispatch::dispatch_plugin_command(.., surface_id: None)` 으로 `command.invoked` 이벤트만 발생(포커스 없이 매칭된 global 단축키와 동일 — 옛 `command.invoke` IPC 는 대상 surface 가 없어 생략).

### MainViewState 동기화

`MainViewState.palette_plugin_commands` 는 `tool_registry` 와 동형 — 첫 창 조립(`assemble_app_state`) 시 1 회, 이후 plugin 라이프사이클 변경(`install`/`remove`/`enable`/`disable`/`grant`/`revoke`/`upgrade_builtins`) 시 `App::refresh_palette_plugin_commands`(`refresh_tool_registry` 와 동일 트리거·호출부)가 갱신한다.

## 인터페이스

- **사용자**: `toggle_command_palette` 단축키 + 도구 메뉴 `Command palette` 항목.
- **IPC/CLI**: 없음 — 의도된 것. 팔레트는 *사용자의 키보드 런처* 다. agent 는 자기 동작을 IPC/CLI 로 직접 하므로 팔레트가 불필요.

## 비-목표

- IPC/CLI 노출.
- 단축키 명령 자체의 정의 — 팔레트는 *실행 진입점* 일 뿐, 각 명령의 동작은 그 기능.
- Plugin `surface` scope 명령 노출 — 위 "후보 목록" 참고.

## Acceptance Criteria

- 단축키 또는 도구 메뉴로 팔레트가 열린다.
- 쿼리 입력 시 모든 단축키 명령(`PALETTE_EXCLUDED` 제외) + 활성 plugin 의 global 명령이 점수순으로 필터된다.
- `↑/↓`/Enter/Esc 와 클릭으로 탐색·실행·닫기가 된다.
- 항목 실행 결과가 해당 단축키 직접 실행과 동일하다(호스트) / 대응 도구 메뉴 클릭과 동일하다(plugin).
- plugin 이 하나도 활성화되지 않은 상태에서도 팔레트가 정상 동작한다(회귀 없음).

화면은 스크린샷으로, 매칭과 필터는 단위 시험으로 검증한다. 실제 plugin 명령의 검색·실행은
debug IPC(`debug.host_popup.open`/`debug.inject_egui_mouse`)로 확인할 수 있다.

## 구현

- 상태: `src/state/command_palette.rs` (`PaletteCommand::Host`/`Plugin`, `all_commands`, `match_score`, `pending_run`).
- popup: `src/adapters/ui/popup/command_palette.rs`.
- plugin 명령 snapshot 동기화: `src/app/plugin_glue/palette_commands.rs`(`refresh_palette_plugin_commands`), 초기 populate `src/app/window_lifecycle.rs`(`assemble_app_state`).
- plugin 명령 조회 필터: `crates/tasty-host-plugin/src/manager/queries.rs`(`plugin_palette_commands`).
- dispatch: `src/view/main/redraw.rs` 가 `pending_run` drain → 호스트는 `dispatch_action_by_id`, plugin 은 `MainViewState.pending_plugin_command_invokes` 로 enqueue. `src/app/dispatch/palette_plugin_commands.rs`(`dispatch_pending_palette_plugin_commands`)가 App 메인 루프에서 drain해 action 실행 또는 이벤트 발생.

## 화면

화면정의서 — **명령 팔레트 화면**.

- **트리거 위치**: `toggle_command_palette` 단축키 · [도구 메뉴](../tools-menu/index.md#화면) `Command palette`
- **시각 소스**: `site/vendor/ui_kits/terminal/overlays/command_palette.jsx` — Design System

### 트리거

단축키(`toggle_command_palette`) 또는 도구 메뉴 항목 → 화면 중앙에 팔레트 popup.

### 레이아웃

```
┌──────────────────────────────────┐
│ 🔍 (검색 입력)                     │
├──────────────────────────────────┤
│ ▸ New workspace       [Alt]+[N]   │  후보 행: 아이콘 + 라벨 + 키캡
│   Split pane          [Alt]+[E]   │
│   Toggle settings                 │
│   …                               │
├──────────────────────────────────┤
│ ↑↓ navigate  ↵ run  esc close     │  footer: 목록 바로 아래
└──────────────────────────────────┘
```

### UI 요소 인벤토리

- **검색 입력** (상단): 쿼리 입력. 즉시 필터. 공용 `Input`의 `Ground::Raised`를 써서 `input-raised-bg` 채움과 `input-raised-border` 휴지·호버 테두리를 그린다. 포커스는 `border-focus` 테두리와 1px 링으로 표시한다.
- **후보 리스트**: 각 행 = leading 아이콘(디자인 명시 명령은 전용 아이콘, 나머지는 fallback) + 명령 라벨 + 우측 키캡. 선택 행 강조, `↑/↓` 이동.
- **키캡**: 첫 바인딩을 키 하나마다 별도 키캡으로 그리고 사이에 `+` 를 둔다 — 상태바·메뉴와 같은 공용 `Kbd` 위젯이라 치수·색이 한 곳에서 나온다.
- **footer**: 네비게이션 힌트 한 줄. 목록 바로 아래에 붙는다.
- **여백**: 검색 구역은 사방 `space-md`, 목록은 사방 `space-sm`이다. 목록과 footer 사이에 별도 간격은 없다.

### 상태별 시각

- **빈 쿼리**: 전체 후보. **검색 중**: 점수순 필터. **결과 0**: 빈 목록.
- **카드 높이**: 표시 항목 수를 따라 매 프레임 달라진다 — 쿼리로 후보가 줄면 카드도 줄고 footer 가 마지막 행 바로 아래로 올라온다. 목록 상한은 `palette-list-max-height`(320)이고 행 높이(`control-height`)와 함께 UI 배율을 따르므로, 어느 배율에서도 11 행까지 보이고 12 행부터 스크롤한다. 카드 위쪽 가장자리는 열 때 정한 자리에 고정돼, 타이핑 중에 검색창이 움직이지 않는다.

### 시각 소스

`site/vendor/ui_kits/terminal/overlays/command_palette.jsx` — 팔레트 치수·행·아이콘·바인딩 표기의 단일 출처. 스크린샷: `site/vendor/screens/statusbar-palette-chip.png`.
