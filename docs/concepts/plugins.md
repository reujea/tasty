# 플러그인 (Plugins)

플러그인은 별도 프로세스에서 실행되며 매니페스트로 Surface 종류·도구 항목·CLI·파일 핸들러 등을 선언한다. 호스트는 활성 플러그인이 선언한 기능을 연결한다. 이 문서는 플러그인의 분류와 통합 방식을 설명한다. 번들 플러그인의 동작은 [플러그인 문서](../plugins/index.md), 제작 방법은 아래 개발 가이드를 참고한다.

## 두 분류 축

플러그인은 **배포 축**과 **통합 축** 두 개로 독립적으로 분류된다. (예: `com.tasty.markdown` 은 배포=번들, 통합=webview surface kind + 파일 핸들러.)

### 배포 축 — 누가 설치/소유하나

| 카테고리 | 위치 | 목록 표시 | 설치 | 교체 |
|----------|------|-----------|------|------|
| **host-native** (기본 내장) | host 코드(본 바이너리) | ✗ | tasty 바이너리 자체 | 불가 |
| **bundled plugin** (기본 플러그인) | `~/.tasty/plugins/<id>/` | ✓ | 첫 부팅 시 `BUILTINS` 자동 install | ✓ disable/remove |
| **user plugin** (사용자 플러그인) | `~/.tasty/plugins/<id>/` | ✓ | `tasty plugin install <path>` | ✓ |

- **host-native**는 플러그인 메커니즘을 거치지 않는 호스트 코드다. `explorer`와 `dag_graph`가 여기에 속한다([구조 계층](hierarchy.md)). 사용자가 교체할 수 없는 내장 기능으로 제공해야 할 때 선택한다.
- **bundled plugin**은 Tasty에 동봉되어 첫 부팅에 자동 설치된다. 이후 활성·비활성·제거는 외부 플러그인과 같다. 제거한 번들은 `removed_builtins`에 기록해 자동으로 다시 설치하지 않는다.
- **user plugin**은 사용자가 직접 설치한다. 자동 설치 대상이 아니라는 점 외에는 번들 플러그인과 같은 디렉터리·수명 관리를 사용한다.

신규 추가 시 판단이 어려우면 **bundled 를 기본값**으로 검토한다(disable 여지를 남기는 편이 안전). 자세한 카테고리 결정 기준·`BUILTINS` 자동 upgrade 절차는 dev-guide(아래).

### 통합 축 — host 에 무엇을 기여하나

플러그인은 매니페스트(`tasty-plugin.toml`)의 `[[surface_kinds]]` / `[contributes]` 로 기여를 선언한다. 주요 종류:

| 기여 | 무엇 | 예 |
|------|------|----|
| **surface_kind** | 새 Surface 종류 등록. `rendering` 으로 누가 그리나 결정 | markdown / image / html / mesh-demo |
| **tool** | [도구 메뉴](../features/tools-menu/index.md) 항목 추가 | clipboard-viewer / git-viewer |
| **popup** | host popup 등록 | clipboard-viewer / git-viewer |
| **cli / ipc_namespace** | `tasty <prefix> …` CLI + IPC 메서드 추가 | claude / codex / html / image / markdown |
| **detector / handler** | 파일 확장자 → surface 매핑(파일 열기) | markdown / image / html |
| **settings_pages** | [설정 창](../features/settings/index.md)에 플러그인 페이지 추가 | markdown / html / claude / codex |
| **commands** | 명령 팔레트/단축키용 명령 | clipboard-viewer / git-viewer |
| **event_subscribe / hooks** | host 이벤트 구독 / pre·post 훅 | markdown (`theme.changed`) |

#### surface_kind 의 `rendering` — 누가 그리나

surface kind 는 콘텐츠를 **누가 렌더하느냐**로 다시 갈린다 (→ [work-area Surface 종류](../features/work-area/index.md#surface-종류)):

- **`rendering = "egui-mesh"`** — plugin 이 자기 프로세스에서 egui 를 tessellate 한 mesh 를 host 가 합성한다(docs/dev-guide/egui-mesh-channel.md). plugin ID 와 무관하게 api_version 일치와 kind 이름 충돌만 검사하므로 커스텀 plugin 도 쓸 수 있다. 번들 예: image, mesh-demo(+ markdown 의 대용량/파일열기 확인 팝업 2개만 — 본문은 아님).
- **`rendering = "webview"`** — host 의 네이티브 WebView 오버레이로 그린다. WebView 설정은 kind 를 등록한 plugin 의 설정에서 읽으므로 커스텀 plugin 도 자기 설정과 기본값을 갖는다. 예: html, markdown([WebView 호스트 계약](../design/systems/webview.md), 본문).
- **(기본)** — 플러그인 프로세스가 직접 그린다. 호스트 트리에는 `RemoteSurface`를 두고 UI DSL로 콘텐츠를 받는다. 현재 이 모드를 사용하는 번들 플러그인은 없다.

## 매니페스트 선언

플러그인은 매니페스트(`tasty-plugin.toml`)에 자기 정체(id·버전·실행 파일)와 기여(`[[surface_kinds]]`·`[[contributes.*]]`·`[extends]`), 이벤트 구독·발행(`event_subscribe`·`event_publish`)을 선언한다. 호스트는 설치·로드 때 형식·참조·충돌·중복·예약 namespace 를 검사하고, 활성 플러그인이 선언한 기여를 그대로 연결한다. 기여마다 따로 허락받는 단계는 없다. 매니페스트는 호스트와의 계약이고, 선언으로 충돌·중복·계약 위반을 설치·로드 때 미리 잡아 런타임 오류를 막는 것이 목적이다([매니페스트의 목적](../principles/plugins.md#매니페스트의-목적)). 보안 수단이 아니다. 선언하지 않은 이벤트 구독·발행은 계약 위반으로 거절하고 호스트 로그에 고칠 방법을 남긴다.

설치·활성된 플러그인은 호스트 IPC 중 플러그인에 연 메서드(`plugin_callable`)를 모두 부를 수 있다. 설치·제거·활성 전환 같은 Local 전용 메서드와 debug 메서드는 부를 수 없다. 호출자별 판정과 Agent 세션의 권한 토큰은 [dev-guide/plugin-permissions](../dev-guide/plugin-permissions.md), 민감 데이터 취급은 [dev-guide/plugin-development 민감 데이터](../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택).

## 관련

- **관리/설치 UI** (사용자 기능) → [`features/plugin-system/`](../features/plugin-system/index.md)
- **번들 플러그인 목록** → [`plugins/`](../plugins/index.md). surface kind 를 제공하는 플러그인(markdown·image·html)의 동작은 [`surfaces/`](../surfaces/index.md)에 있다
- **제작 가이드** → [dev-guide/plugin-development](../dev-guide/plugin-development.md) (기여 타입별 + 번들 플러그인을 예제로 인용)
- **호스트 호출 계약 / 민감 데이터** → [dev-guide/plugin-permissions](../dev-guide/plugin-permissions.md) · [dev-guide/plugin-development 민감 데이터](../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택)
- **surface 종류와 렌더 분기** → [`features/work-area/`](../features/work-area/index.md#surface-종류) · kind별 명세 [`surfaces/`](../surfaces/index.md)
