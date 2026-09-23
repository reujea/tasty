# 설정 (Settings)

- **Status**: Implemented
- **주체**: 로컬 사용자 (원격 접속 사용자는 mirror 로 봄)
- **ADR**: 없음
- **코드**: `src/view/settings.rs`, `src/view/settings/ui/`
- **화면**: [screens/settings.md](screens/settings.md)

## 목적

[사이드바](../sidebar/index.md) 설정 버튼이 여는 **설정 창**. tasty 의 환경설정을 2-level IA(상단 L1 탭 + 좌측 L2 섹션)로 편집한다. `SettingsView`(모달 계열 View, [구조 계층](../../concepts/hierarchy.md))이다.

## 내부 동작

### 2-level IA — L1 7탭 × L2 섹션

상단 L1 7탭, 각 탭은 좌측에 L2 섹션 목록을 가진다 (이 순서):

- **General** — L2: General(레이아웃 복원 · 카테고리 · 닫기 동작 · **언어** — 내장 `en`/`ko`/`ja` + `~/.tasty/lang/<code>/pack.toml` 언어팩을 한 콤보에 노출, 라벨은 `[meta] name` 없으면 코드, 설정값이 목록에 없으면 `<code> (not found)` 행으로 유지하고 덮어쓰지 않음, 변경은 재시작 후 반영 → [language-packs](../language-packs/index.md)) / Notifications / Accessibility / Overlay(오버레이류 표시 설정 — 현재는 토스트 자동 소멸 시간 `Toast duration` 1~10s 1행 → [toast](../../design/systems/toast.md)) / Remote transfer(원격 mirror 파일 전송 수신측 저장 정책 — `Save folder`(mono 경로 Input + Browse… native 폴더 피커, 기본 `~/.tasty/transfers/`) + `Maximum size`(정수 mono Input + 정적 `MiB` suffix, 기본 500 MiB) 2행, `RemoteTransferSettings{dir, max_mb}` 편집 → [remote-attach](../remote-attach/index.md)) / Display(macOS 전용 — Alt/Option/Shift 단축키 표시 스타일 드롭다운 3개. 텍스트("Alt"/"Option"/"Shift", 기본값)와 macOS 심볼("⌘"/"⌥"/"⇧") 중 독립 선택, `alt` 는 "Cmd" 텍스트도 선택 가능. 저장 포맷에는 영향 없음 → [key-mapping](../../design/policies/key-mapping.md)) / Permissions(macOS 전용 — 권한(TCC) 상태 표시 + 시스템 설정 바로가기 → [macos-permissions](../macos-permissions/index.md)).
- **Terminal** — L2: General(터미널 동작 설정 — 셸/스타트업/스크롤백/링크 수식키, macOS 빌드는 "Use Option as Meta" 토글 추가 → [terminal](../terminal/index.md) 키보드 입력) / Mouse Capture(마우스 캡처 안내 배너 토글 + Shift 우회 Note + 캡처 비활성화 블랙리스트 에디터) / TUI(OSC 52 클립보드 읽기 허용 토글 + 바로 아래 bordered warning callout → [clipboard](../clipboard/index.md)) / Performance.
- **Appearance** — L2: Theme / Colors / General / Display / Tasty / Terminal / Explorer + 플러그인 기여 페이지(동적). Display = UI 스케일(sm/md/lg) 전용. Tasty = 앱 크롬 색상(accent / sidebar bg / active tab indicator). Explorer는 내장 파일 관리자(T11) 전용 폰트 override 섹션 — 과거 `com.tasty.explorer` 플러그인이 기여하던 페이지였으나 host builtin 승격 후 고정 섹션이 됐다. HTML viewer 설정은 호스트 고정 탭이 아니라 `com.tasty.html` 플러그인이 기여하는 동적 페이지다.
- **Keybindings** — L2: General / Workspace / Pane / Tab / Surface / Clipboard / Zoom / Explorer / Scripts / Preset / Plugins / Import / Export(separator 아래). 단축키 편집 (아래).
- **FileHandler**(표시 라벨 **Handler** — 내부 enum 키는 `FileHandler` 유지) — L2: File Extension Mapping / File Detectors / File Handlers / Hook Handlers. Hook Handlers 는 공유 훅 핸들러 레지스트리(host 기본 + plugin 기여 + user 매핑) 편집 — 행별 enabled 토글(전 출처), ShellCommand 행 인라인 명령 편집, **user 행만** 제거, 인라인 추가 폼(신규 행 origin=user·max+10 priority·enabled). **출처가 지울 수 있는지를 정한다** — 레지스트리가 시작마다 host/plugin 기본값을 다시 심으므로 그 행에는 휴지통 대신 자물쇠 글리프가 온다(disabled 버튼이 아니다 — 보류된 것이 없다). 출처 Tag 는 모든 행이 달고 `host` · `you` · 그 plugin 의 id 를 찍는다. `IpcSequence` 행은 인라인 편집 없이 mono 한 줄 요약(스텝을 `→` 로 이음)이고, **GUI 시퀀스 편집기는 아직 없다** — 시퀀스 본문은 [`tasty hook-handler get`/`upsert`](../hooks/index.md#핸들러-레지스트리-hook_handler) 로 고친다(`~/.tasty/hook-handlers.toml` 손편집 + `tasty hook-handler reload` 도 그대로 된다). 웹훅 리스너(bind/port/secret) 설정은 여기 노출하지 않는다(CLI 전용).
- **Misc** — L2: Scripts (전 플랫폼·최상단 — Lua 스크립트 관리, [lua-hooks](../lua-hooks/index.md)) + Tastyrc (Windows 전용).
- **Plugins** — 플러그인 기여 설정 페이지 (동적).

L2 섹션은 좌측에 목록으로 뜨고 **필터 텍스트로 검색** 가능 (L1 전환 시 클리어).

### 플러그인 기여 설정 (generic 컨트롤)

플러그인이 `[[contributes.settings_pages]]` 로 기여한 페이지를 host 가 `draw_plugin_settings_page` 로 렌더한다. manifest item `kind` 별로 generic 컨트롤을 그린다 — `toggle` → Switch, `select` → Select(드롭다운), `number` → text Input(mono, + `suffix_key` 단위), `font_override` → surface 폰트 섹션. `toggle`/`select`/`number` 값은 `plugin_settings.<plugin_id>.<storage_key>` 슬롯(`PluginSettingValue` = Bool/Number/Text)에 저장·영속되며(`font_override` 의 전역 `plugin_font_overrides` 와 별개 네임스페이스), 변경 즉시 write + persist 된다. 첫 소비자는 `com.tasty.html` — Appearance 에 HTML viewer 설정(zoom / color scheme / allow remote content / sandbox scripts)을 이 방식으로 노출한다.

> **surface 폰트 override 저장소**: surface-kind 폰트 override 는 전부 `appearance.plugin_font_overrides.<kind>`(generic per-kind, host 는 live 경로에서 특정 kind 이름을 모른다)로 수렴한다. 단 레거시 top-level `[markdown_font]`/`[explorer_font]` 섹션은 **전환기 back-compat 로 유지**한다(`migrate_legacy_font_overrides` 가 load 시 `plugin_font_overrides` 로 일회성 승계 — 읽기 전용, write-back 없음). **후속 과제**: 이 migration 이 정식 릴리스에 배포된 뒤 다음 사이클에 두 레거시 필드를 제거한다(그전에 제거하면 migration 미포함 릴리스 사용자의 폰트 override 가 유실됨).

> **소비 배선**: host 가 `resolve_webview_settings` 로 `plugin_settings."com.tasty.html"` 을 읽어 네이티브 webview 에 직접 적용한다(별도 host→plugin IPC 없음 — `font_override` 호스트 적용과 같은 선례). 적용 현황:
> - **zoom · sandbox(JS on/off)**: 3 OS 모두 실효.
> - **color_scheme**(`prefers-color-scheme` 강제): macOS 실효(NSAppearance). Windows/Linux 는 no-op(후속).
> - **allow remote content**(원격 http/https 서브리소스 차단): 3 OS 모두 실효 — macOS 는 `WKContentRuleList`, Windows 는 `WebResourceRequested` 403, Linux 는 같은 스키마의 content filter 를 `WebKitUserContentFilterStore` 로 컴파일해 user content manager 에 붙인다(네비게이션은 decide-policy 가 따로 막는다). 근거·대안은 [ADR-0250](../../adr/0250-linux-blocks-remote-subresources-with-a-webkit-content-filter.md). 단 Windows/Linux 백엔드는 macOS 호스트에서 컴파일 불가라 CI(self-hosted Win / `test.yml` Linux)에서만 검증된다.

### draft / save 모델

편집은 **작업 사본(`draft`)** 에 쌓이고, Save 시 영속 `Settings` 로 커밋, Cancel 시 폐기. 일부 항목(FileHandler 의 파일 서브탭 → `~/.tasty/file-handlers.toml`, Hook Handlers → `~/.tasty/hook-handlers.toml`)은 Save 시 각 registry commit 후 user TOML 에 직접 atomic write.

**저장 실패는 화면에 도달한다.** Tastyrc(Windows) 편집 저장이 실패하면 그 사유를 모달이 들고 있다가, 창이 닫힐 때 host 가 회수해 main window 에 Error 토스트(`toast.bashrc_save_failed`)로 띄운다 — Save 는 곧바로 설정 창을 닫으므로 설정 창 안에 띄우면 보이지 않는다. 사유에는 대상 경로와 OS 에러가 들어 있어 문구에 함께 싣되, 토스트 200자 캡에 맞춰 **가운데를 생략**한다(`tasty_i18n::t_fmt_fit`) — 호스트 기본 잘림은 꼬리(=OS 에러)를 버린다. 성공 토스트는 없다(저장은 기본 기대 동작이라 매번 알리면 소음). → [toast](../../design/systems/toast.md)

### 단축키 탭 (Keybindings)

키 조합을 직접 녹화해 바인딩 할당. 충돌 시 확인 팝업으로 수락/거부. (모든 단축키는 `KeybindingSettings` 경유 — 코드 하드코딩 금지.)

### 플러그인 기여 페이지

플러그인이 설정 페이지를 contribute 하면 Appearance 의 sub-tab + Plugins 탭에 `(plugin_id, page_id)` 복합키로 나타난다. 등록된 plugin page 가 없으면 Plugins 탭/해당 sub-tab 이 비거나 사라진다.

## 인터페이스

- **사용자**: 사이드바 설정 버튼 → 모달, L1/L2 탐색, 편집 → Save/Cancel.
- **각 설정 도메인은 해당 기능으로 연결** (연결 개념 — 설정 창은 편집 UI, 도메인 규칙은 각 문서):
  - Keybindings → [`features/keybindings/`](../keybindings/index.md) / 키 매핑 정책 [`design/policies/key-mapping`](../../design/policies/key-mapping.md)
  - Appearance/Theme → [`design/systems/theme`](../../design/systems/theme.md)
  - Clipboard → [`features/clipboard/`](../clipboard/index.md) · Notifications → [`features/notifications/`](../notifications/index.md) · FileHandler(파일 서브탭) → [`features/file-handler/`](../file-handler/index.md) · Hook Handlers → [`features/webhook/`](../webhook/index.md)·[`features/hooks/`](../hooks/index.md)
  - Plugins → [`features/plugin-system/`](../plugin-system/index.md)
  - General › Language → [`features/language-packs/`](../language-packs/index.md) · 로더 규칙 [`dev-guide/i18n`](../../dev-guide/i18n.md)

## 비-목표

- 각 설정 항목의 *도메인 동작* (테마가 무엇을 바꾸나, 단축키가 무엇을 하나 등) — 설정 창은 *편집 표면* 일 뿐. 도메인은 각 기능/시스템 문서.

## Acceptance Criteria

- 사이드바 설정 버튼 클릭 시 설정 모달이 열린다 (L1 7탭).
- L1 탭 전환 시 좌측 L2 섹션 목록이 그 탭의 것으로 바뀌고 필터가 클리어된다.
- 편집 후 Save 시 영속 Settings 에 반영되고, Cancel 시 폐기된다.
- Keybindings 에서 키 조합 녹화 시 충돌이 있으면 확인 팝업이 뜬다.
- 플러그인이 설정 페이지를 contribute 하면 Plugins 탭/Appearance sub-tab 에 나타난다.

> 모달 창이라 시각 검증은 스크린샷, draft/save·plugin page 등록은 시나리오로 검증.

## 구현

- `src/view/settings.rs` — `SettingsView`, `SettingsUiState`(draft/active_tab/sub-tab 상태).
- `src/view/settings/ui.rs` — `SettingsTab`(L1 7탭: General / Terminal / Appearance / Keybindings / FileHandler / Misc / Plugins), L2 enum 군 `GeneralSubTab` / `TerminalSubTab` / `AppearanceSubTab`(Theme / Colors / General / Display / Tasty / Terminal / Explorer / Plugin) / `MiscSubTab` / `PluginSubTab`, L2 필터. FileHandler 의 L2 는 `FileHandlerSubTab`(ExtensionMapping / Detectors / Handlers / HookHandlers).
- 언어 콤보: `src/view/settings/ui/tabs/general.rs` → `tasty_ui_widgets::language_select`(갤러리 Settings specimen 공유), 목록은 `SettingsUiState.languages`(창 오픈 시 `tasty_i18n::available_languages()` 1회).
- 탭별: `src/view/settings/ui/tabs/*` + `keybindings_tab.rs` + `file_handler_tab.rs`(+ `file_handler_tab/hook_handlers.rs` — 훅 핸들러 레지스트리 편집).

## 화면

- [screens/settings.md](screens/settings.md) — 설정 창 레이아웃(L1 탭바 / L2 섹션 / 콘텐츠 / Save·Cancel)과 섹션별 연결.
