# 번들 플러그인 (Bundled plugins)

Tasty가 관리하는 공식 플러그인 목록이다([plugins 개념](../concepts/plugins.md)). 배포 패키지에 동봉된 플러그인은 첫 부팅 때 자동 설치하며, `bundle=false`인 개발용 항목은 패키지에서 제외한다. 각 플러그인은 외부 플러그인과 동일한 라이프사이클(활성/비활성/제거/권한)을 따른다 — 관리 UI 는 [plugin-system](../features/plugin-system/index.md).

surface kind 를 제공하는 플러그인(markdown·image·html·agent-session)은 그 kind 의 명세가 정본이므로 [`surfaces/`](../surfaces/index.md)의 kind 문서에 둔다. 파일 핸들러·CLI·설정 페이지·popup 처럼 같은 플러그인의 다른 기여도 그 문서에 함께 적는다. surface 를 제공하지 않는 플러그인은 `plugins/<id>/index.md`(동작) + 화면(UI 가 있으면 — 기획문서의 `## 화면` 절이 기본, 화면이 둘 이상이거나 자기 규칙을 가지면 `screens/` 파일. 기준은 [documentation-model §3](../documentation-model.md#3-폴더-구조-중첩))에 둔다. 이 영역은 [features/](../features/index.md) 와 구조가 같되, **host 가 아니라 플러그인이 제공**하는 동작이라 분리되어 있다. 양식은 features 템플릿을 그대로 쓴다.

Claude의 훅 설치는 [완료 알림 mod](claude/notify-mod.md)도 관리한다. 실제 모델 수신과 로컬 시험의 범위는 구분한다.

claude·codex 는 attach 로 연결한 원격 워크스페이스에도 자식을 만들고 관리한다. 두 plugin 이 같은 방식을 쓰므로 [원격 attach 자식](remote-attach-children.md)에 함께 적는다. 자식 쪽 Tasty 에 설치한 기존 훅이 그대로 완료를 보고한다.

## 카탈로그 (`BUILTINS`)

| 플러그인 (id) | 무엇 | 주요 기여 |
|---------------|------|-----------|
| [markdown](../surfaces/markdown/index.md) — `com.tasty.markdown` | 마크다운 뷰어 | surface_kind(webview) · 파일 핸들러 · cli · settings_page |
| [image](../surfaces/image/index.md) — `com.tasty.image` | 이미지 뷰어 / 그림판 | surface_kind(egui-mesh) · 파일 핸들러 · cli |
| [html](../surfaces/html/index.md) — `com.tasty.html` | HTML 뷰어 | surface_kind(webview) · 파일 핸들러 · cli |
| [clipboard-viewer](clipboard-viewer/index.md) — `com.tasty.clipboard-viewer` | 클립보드 뷰어(현재 내용) | 도구 메뉴 · popup |
| [git-viewer](git-viewer/index.md) — `com.tasty.git-viewer` | git status/log/diff 뷰어 | 도구 메뉴 · popup |
| [claude](claude/index.md) — `com.tasty.claude` | Claude Code CLI 통합 · spawn 전용 모델·effort 선택과 같은 세션 복원 · Shift+Enter 입력 기본값 | cli · ipc · 멀티에이전트 |
| [codex](codex/index.md) — `com.tasty.codex` | Codex CLI 통합 | cli · ipc · 멀티에이전트 · 부모 대화 자동 반환 · 호출자 실행 경로 보존 · 일반 명령·선택기 실행의 터미널별 세션 복원 · 부팅 IPC 준비 대기 · 큰 Codex 응답의 원문 중계 · spawn 자식별 model/effort 지정 |
| mesh-demo — `com.tasty.mesh-demo` (전용 문서 없음 — [egui-mesh 채널](../dev-guide/egui-mesh-channel.md)) | egui-mesh 채널 검증 예제, `bundle=false`로 배포 제외 | surface_kind(egui-mesh) · popup |
| [agent-stream](agent-stream/index.md) — `com.tasty.agent-stream` | Claude transcript 수집·SSE 전송·요청과 턴 매칭, `bundle=false`로 배포 제외 | cli · ipc · sse |
| [agent-session](../surfaces/agent-session/index.md) — `com.tasty.agent-session` | Claude Code 를 stream-json 으로 띄우는 대화 탭 · agent 프로필, `bundle=false`로 배포 제외 | surface_kind(webview) · cli · ipc · settings_page |

> 각 플러그인은 **제작 가이드의 예제**이기도 하다 — 각 문서 상단의 "예제로서" 노트가 어떤 기여 패턴의 레퍼런스인지와 [dev-guide/plugin-development](../dev-guide/plugin-development.md) 해당 절을 가리킨다.

## 관련

- 개념·분류 축·권한: [concepts/plugins](../concepts/plugins.md)
- **제작 가이드(예제 인용)**: [dev-guide/plugin-development](../dev-guide/plugin-development.md) · [plugin-permissions](../dev-guide/plugin-permissions.md) · [plugin-development 민감 데이터](../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택)
- 관리/설치/권한 UI: [features/plugin-system](../features/plugin-system/index.md)
- 도구 메뉴 기여 항목의 진입점: [features/tools-menu](../features/tools-menu/index.md)
