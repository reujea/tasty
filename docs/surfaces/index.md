# Surface 종류 (Surfaces)

Surface 종류(kind)마다 명세를 한 곳에 둔다. host 내장인지 번들 plugin이 등록하는지와 관계없이 이 폴더에서 찾는다. Surface는 화면만이 아니라 콘텐츠 타입, 수명, 저장·복원, IPC·CLI 계약을 함께 가진다. 배치 이유는 [문서 구조 결정](../documentation-model.md)에 있다.

여러 kind가 공유하는 구조는 이 폴더에 두지 않는다.

- Workspace › Pane › Tab › Surface 트리, 분할, 생성 경계, kind 등록 조회: [작업 영역](../features/work-area/index.md)
- 계층 용어: [구조 계층](../concepts/hierarchy.md)
- plugin 배포·통합 축과 렌더 분기 개념: [plugins 개념](../concepts/plugins.md)
- 새 surface의 cwd 상속: [cwd 정책](../design/policies/cwd.md#surface-cwd-invariant)
- 다른 kind로 바꾸기: [surface 변환](../features/convert-surface/index.md)
- 원격 attach의 공통 점유·mirror 규칙: [remote-attach](../features/remote-attach/index.md)

## 카탈로그

| kind | 문서 | 제공 주체 | 렌더 경로(`surface.kinds`의 `rendering`) |
|---|---|---|---|
| `terminal` | [terminal](terminal/index.md) | host 내장 | GPU 셀 그리드 (`host-egui` 로 등록, 콘텐츠는 GPU 셰이더) |
| `empty` | [empty](empty/index.md) | host 내장 | `host-egui` |
| `explorer` | [explorer](explorer/index.md) — 내부 탭별 선택·스크롤·Find·X11 이름 변경 IME, 미리보기 옆 목록 테두리, 원격 진행·호스트 툴팁·작은 칸 안 대기열 버튼, 보이는 행 조회, [OS별 확인 범위와 Wayland 한계](explorer/file-operations.md#확인된-linux-범위와-한계) | host 내장 | `host-egui` |
| `dag_graph` | [dag-graph](dag-graph/index.md) | host 내장 | `host-egui` |
| `markdown` | [markdown](markdown/index.md) | 번들 plugin `com.tasty.markdown` | `webview` |
| `image` | [image](image/index.md) | 번들 plugin `com.tasty.image` | `egui-mesh` |
| `html` | [html](html/index.md) — 로컬 문서·Linux 원격 파일 entry | 번들 plugin `com.tasty.html` | `webview` |

- host 내장 4종은 `register_builtin_kinds`(`src/runtime/surface_registry/builtins.rs`)가 부팅 때 등록한다.
- plugin kind는 매니페스트(`crates/tasty-plugin-<id>/tasty-plugin.toml`)의 `[[surface_kinds]]`가 선언하고, plugin이 연결된 뒤 host가 등록한다. 실제로 등록된 kind와 렌더 경로는 `tasty list surface-kinds`(IPC `surface.kinds`)가 런타임 기준으로 알려 준다. 헤드리스도 `webview`·`remote`·`egui-mesh` kind를 GUI와 같이 등록한다([작업 영역](../features/work-area/index.md#surface-종류)).
- `mdedit`(`com.tasty.mdedit`)은 사용자와 에이전트가 같은 로컬 마크다운 파일을 함께 고치는 편집기다(렌더 경로 `webview`). 매니페스트가 `bundle = false`라 이 카탈로그에 넣지 않고, 명세는 [mdedit](mdedit/index.md)에 있다.
- `mesh_demo`(`com.tasty.mesh-demo`)는 egui-mesh 채널 검증용 개발 kind다. 매니페스트가 `bundle = false`라 배포 패키지에 들어가지 않으므로 이 카탈로그에 넣지 않는다. 동작은 [egui-mesh 채널](../dev-guide/egui-mesh-channel.md)에 있다.

번들 plugin의 다른 기여(파일 핸들러, CLI, 설정 페이지, popup)는 그 plugin이 제공하는 surface 문서에 함께 적는다. plugin 목록과 surface가 없는 plugin은 [번들 플러그인](../plugins/index.md)에 있다.

## 타입 문서의 공통 항목

각 kind 문서는 내부 동작을 먼저 설명하고 화면을 연결한다([문서 모델 §1](../documentation-model.md#1-핵심-원칙--동작이-1순위-화면은-2순위)). 다음 항목을 다룬다. 해당하지 않는 항목은 지우지 않고 이유를 한 줄로 적는다. 항목마다 파일을 나누지 않는다.

1. **목적·지원 범위** — 이 kind가 보여 주는 콘텐츠와 지원하지 않는 범위.
2. **상태 소유자** — 구조 트리의 leaf(`SurfaceDescriptor`), 실행 자원(PTY, plugin 문서, WebView 등), View 상태(스크롤·선택 등)가 각각 어디에 있는지.
3. **생성·갱신·종료** — 만드는 경로와 필수 params, 콘텐츠 갱신, 닫을 때 정리하는 자원.
4. **저장·복원** — 레이아웃에 저장하는 값, 저장하지 않는 값, 복원 순서와 실패 시 상태.
5. **IPC·CLI** — 이 kind 전용 명령과 공통 명령에서 다른 점.
6. **headless·원격 제약** — 헤드리스 등록 여부, attach mirror에서의 동작.
7. **화면** — 단일 화면이면 `## 화면` 절, 화면이 둘 이상이거나 고유 규칙이 있으면 `screens/` 파일([문서 모델 §3](../documentation-model.md#3-폴더-구조-중첩)).
8. **검증 기준** — Given/When/Then 평문 불릿.

새 kind 문서는 [surface 템플릿](_surface.template.md)으로 시작한다.

## 관련

- [작업 영역](../features/work-area/index.md) — 구조 트리와 공통 생성 경계
- [번들 플러그인](../plugins/index.md) — plugin 카탈로그
- [plugin 개발](../dev-guide/plugin-development.md#surface-kind--rendering-3-종) — plugin이 surface kind를 등록하는 방법
