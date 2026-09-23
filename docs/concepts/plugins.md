# 플러그인 (Plugins)

tasty 의 많은 기능은 **플러그인**으로 제공된다 — 별도 프로세스가 매니페스트로 자기 기여(surface 종류·도구 항목·CLI·파일 핸들러 등)를 선언하고, host 가 권한 범위 안에서 그것을 통합한다. 이 문서는 *플러그인이란 무엇이고 어떤 축으로 분류·통합되는가* 의 단일 출처다. 개별 번들 플러그인의 동작은 [`plugins/`](../plugins/index.md), 제작 방법은 dev-guide(아래).

## 두 분류 축

플러그인은 **배포 축**과 **통합 축** 두 개로 독립적으로 분류된다. (예: `com.tasty.markdown` 은 배포=번들, 통합=webview surface kind + 파일 핸들러.)

### 배포 축 — 누가 설치/소유하나

| 카테고리 | 위치 | 목록 표시 | 설치 | 교체 |
|----------|------|-----------|------|------|
| **host-native** (기본 내장) | host 코드(본 바이너리) | ✗ | tasty 바이너리 자체 | 불가 |
| **bundled plugin** (기본 플러그인) | `~/.tasty/plugins/<id>/` | ✓ | 첫 부팅 시 `BUILTINS` 자동 install | ✓ disable/remove |
| **user plugin** (사용자 플러그인) | `~/.tasty/plugins/<id>/` | ✓ | `tasty plugin install <path>` | ✓ |

- **host-native** 는 플러그인 메커니즘을 거치지 않는 host 코드다. 대부분의 viewer 는 bundled 로 이전됐지만, `explorer` surface kind 는 plugin 에서 host-native 로 역이전됐다([hierarchy.md](hierarchy.md) 참고) — `dag_graph` surface kind 도 host-native 다. (사용자가 플러그인으로 인식하지 않아야 하고 교체 여지를 원천 차단할 때만 쓰는 카테고리.)
- **bundled plugin** 은 tasty 에 동봉되어 첫 부팅에 자동 install 되지만, 이후엔 외부 플러그인과 동일 라이프사이클(활성/비활성/제거/권한)을 따른다. remove 하면 `removed_builtins` 에 박혀 재설치되지 않는다.
- **user plugin** 은 사용자가 직접 install 한 외부 플러그인. host 가 자동 install 대상으로 인지하지 않을 뿐 디렉토리·라이프사이클은 동일.

신규 추가 시 판단이 어려우면 **bundled 를 기본값**으로 검토한다(disable 여지를 남기는 편이 안전). 자세한 카테고리 결정 기준·`BUILTINS` 자동 upgrade 절차는 dev-guide(아래).

### 통합 축 — host 에 무엇을 기여하나

플러그인은 매니페스트(`tasty-plugin.toml`)의 `[[surface_kinds]]` / `[contributes]` 로 기여를 선언한다. 주요 종류:

| 기여 | 무엇 | 예 |
|------|------|----|
| **surface_kind** | 새 Surface 종류 등록. `rendering` 으로 누가 그리나 결정 | markdown / image / html / mesh-demo |
| **tool** | [도구 메뉴](../features/tools-menu/index.md) 항목 추가 (`ui.tool_item`) | clipboard-viewer / git-viewer |
| **popup** | host popup 등록 (`ui.popup`) | clipboard-viewer / git-viewer |
| **cli / ipc_namespace** | `tasty <prefix> …` CLI + IPC 메서드 추가 | claude / codex / html / image / markdown |
| **detector / handler** | 파일 확장자 → surface 매핑(파일 열기) | markdown / image / html |
| **settings_pages** | [설정 창](../features/settings/index.md)에 플러그인 페이지 추가 (`ui.settings_page`) | markdown / html / claude / codex |
| **commands** | 명령 팔레트/단축키용 명령 | clipboard-viewer / git-viewer |
| **event_subscribe / hooks** | host 이벤트 구독 / pre·post 훅 | markdown (`theme.changed`) |

#### surface_kind 의 `rendering` — 누가 그리나

surface kind 는 콘텐츠를 **누가 렌더하느냐**로 다시 갈린다 (→ [work-area Surface 종류](../features/work-area/index.md#surface-종류)):

- **`rendering = "egui-mesh"`** — plugin 이 자기 프로세스에서 egui 를 tessellate 한 mesh 를 host 가 합성한다(ADR-0028). bundled 전용 화이트리스트 + api_version 게이트. 예: image, mesh-demo(+ markdown 의 대용량/파일열기 확인 팝업 2개만 — 본문은 아님).
- **`rendering = "webview"`** — host 의 네이티브 WebView 오버레이로 그린다. 예: html, markdown([ADR-0065](../adr/0065-markdown-webview-render-channel.md), Stage B — 본문).
- **(기본)** — 플러그인 프로세스가 직접 그린다. host 는 트리에 `RemoteSurface` marker 만 두고 plugin UI DSL 로 콘텐츠를 받는다. 현재 이 모드를 쓰는 번들 plugin 은 없다(과거 explorer 가 예시였으나 host-native 로 승격됨, [hierarchy.md](hierarchy.md) 참고) — 전부 `egui-mesh` 또는 `webview` 로 이전 완료.

## 권한 (Permissions)

플러그인은 매니페스트의 `permissions` 로 필요한 권한을 선언하고, host 가 grant 한 범위에서만 **호스트 IPC 호출**을 한다(권한 게이트는 OS 자원이 아니라 호스트 API 호출을 막는다 — [plugin-permissions](../dev-guide/plugin-permissions.md)). 토큰은 `crates/tasty-plugin-manifest/src/types.rs` `Permission` enum 이 단일 출처.

**Scope 없는 토큰**:
`surface.read` · `surface.write` · `fs.read` · `fs.write` · `clipboard.read` · `clipboard.write` · `notification` · `process.spawn` · `terminal.spawn` · `terminal.write` · `terminal.read` · `network` · `memory.read` · `memory.write` · `memory.secret` · `approval` · `telemetry` · `agent` · `ui.tool_item` · `ui.popup` · `ui.banner` · `ui.settings_page` · `window.spawn` · `file_handler.define` · `hook_handler.define` · `completion_strategy.define`

**Scope 있는 토큰** (`<name>:<scope>`):
`ipc.invoke:<prefix>`(다른 플러그인 namespace 호출 — agent 토큰에도 같은 뜻) · `ext:<plugin_id>`(다른 플러그인 확장) · `file_handler.extend:<id>` · `file_handler.handle:<id>` · `hook_handler.handle:<id>`

각 토큰이 **실제로 무엇을 여는지**(요구하는 호스트 IPC 메서드 · contribute 게이트)는 [dev-guide/plugin-permissions "토큰 전체"](../dev-guide/plugin-permissions.md#토큰-전체--무엇을-여나) — 토큰 이름이 가리키는 범주보다 좁은 것들이 있다.

권한 grant/표시 UI 와 관리는 [plugin-system](../features/plugin-system/index.md), 권한 모델·새 토큰 추가 절차는 [dev-guide/plugin-permissions](../dev-guide/plugin-permissions.md), 민감 데이터 취급은 [dev-guide/plugin-development 민감 데이터](../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택).

## 관련

- **관리/설치/권한 UI** (사용자 기능) → [`features/plugin-system/`](../features/plugin-system/index.md)
- **번들 플러그인 각각의 동작** → [`plugins/`](../plugins/index.md)
- **제작 가이드** → [dev-guide/plugin-development](../dev-guide/plugin-development.md) (기여 타입별 + 번들 플러그인을 예제로 인용)
- **권한 모델 / 민감 데이터** → [dev-guide/plugin-permissions](../dev-guide/plugin-permissions.md) · [dev-guide/plugin-development 민감 데이터](../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택)
- **surface 종류와 렌더 분기** → [`features/work-area/`](../features/work-area/index.md#surface-종류)
</content>
