# 번들 플러그인 (Bundled plugins)

tasty 에 **동봉되어 첫 부팅 시 자동 install** 되는 공식 플러그인들(배포 축 = bundled, [plugins 개념](../concepts/plugins.md)). 각 플러그인은 외부 플러그인과 동일한 라이프사이클(활성/비활성/제거/권한)을 따른다 — 관리 UI 는 [plugin-system](../features/plugin-system/index.md).

각 플러그인은 폴더 하나다 — `plugins/<id>/index.md`(동작) + 화면(UI 가 있으면 — 기획문서의 `## 화면` 절이 기본, 화면이 둘 이상이거나 자기 규칙을 가지면 `screens/` 파일. 기준은 [documentation-model §3](../documentation-model.md#3-폴더-구조-중첩)). 이 영역은 [features/](../features/index.md) 와 구조가 같되, **host 가 아니라 플러그인이 제공**하는 동작이라 분리되어 있다. 양식은 features 템플릿을 그대로 쓴다.

## 카탈로그 (`BUILTINS`)

| 플러그인 (id) | 무엇 | 주요 기여 |
|---------------|------|-----------|
| [markdown](markdown/index.md) — `com.tasty.markdown` | 마크다운 뷰어 | surface_kind(webview) · 파일 핸들러 · cli · settings_page |
| [image](image/index.md) — `com.tasty.image` | 이미지 뷰어 / 그림판 | surface_kind(egui-mesh) · 파일 핸들러 · cli |
| [html](html/index.md) — `com.tasty.html` | HTML 뷰어 | surface_kind(webview) · 파일 핸들러 · cli |
| [clipboard-viewer](clipboard-viewer/index.md) — `com.tasty.clipboard-viewer` | 클립보드 뷰어(현재 내용) | 도구 메뉴 · popup |
| [git-viewer](git-viewer/index.md) — `com.tasty.git-viewer` | git status/log/diff 뷰어 | 도구 메뉴 · popup |
| [claude](claude/index.md) — `com.tasty.claude` | Claude Code CLI 통합 | cli · ipc · 멀티에이전트 |
| [codex](codex/index.md) — `com.tasty.codex` | Codex CLI 통합 | cli · ipc · 멀티에이전트 · Codex 호환 훅 응답 · POSIX alias/function 우회 실행 |
| mesh-demo — `com.tasty.mesh-demo` (전용 문서 없음) | egui-mesh 채널 PoC(A1), `bundle=false`로 배포 제외 | surface_kind(egui-mesh) · popup |
| [agent-stream](agent-stream/index.md) — `com.tasty.agent-stream` | 에이전트 세션 transcript tail → 구조화 스트림 이벤트 수집 + SSE 엔드포인트 방출 · 요청별 턴 correlation · 인증 웹훅 배선(headless) · JSON 입력 상한과 연결 정리의 구분 · SSE 포트 소유권 시험, `bundle=false`로 배포 제외 | cli · ipc · sse |

> 각 플러그인은 **제작 가이드의 예제**이기도 하다 — 각 문서 상단의 "예제로서" 노트가 어떤 기여 패턴의 레퍼런스인지와 [dev-guide/plugin-development](../dev-guide/plugin-development.md) 해당 절을 가리킨다.

## 관련

- 개념·분류 축·권한: [concepts/plugins](../concepts/plugins.md)
- **제작 가이드(예제 인용)**: [dev-guide/plugin-development](../dev-guide/plugin-development.md) · [plugin-permissions](../dev-guide/plugin-permissions.md) · [plugin-development 민감 데이터](../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택)
- 관리/설치/권한 UI: [features/plugin-system](../features/plugin-system/index.md)
- 도구 메뉴 기여 항목의 진입점: [features/tools-menu](../features/tools-menu/index.md)
