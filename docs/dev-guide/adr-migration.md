# ADR 문서 이동표

제품 원칙과 현재 설계·운영 계약의 정본을 찾기 위한 이전 번호 대응표다. 이전 0001~0078은 현재 예외 ADR 번호가 아니다. 새 예외 기록은 0001부터 시작한다.

원본은 Git 커밋 d636a92837653e162605bc1bfce7930b58100454의 docs/adr 디렉터리에 있다. 현재 내용은 아래 정본을 따르며, 같은 번호의 미래 ADR을 과거 선택으로 해석하지 않는다.

| 이전 번호 | 현재 주제 | 현재 정본 |
|---|---|---|
| 0001 | 아키텍처 개요 | [아키텍처 개요](../architecture/index.md) |
| 0002 | App·Engine·View 상태 소유권 | [App·Engine·View 상태 소유권](app-state-ownership.md) |
| 0003 | 헤드리스 IPC 지원 범위 | [헤드리스 IPC 지원 범위](headless-ipc-surface.md) |
| 0004 | CLI / IPC API 규약 — 명명 + 안정성 | [CLI / IPC API 규약 — 명명 + 안정성](api-conventions.md) |
| 0005 | CLI / IPC API 규약 — 명명 + 안정성 | [CLI / IPC API 규약 — 명명 + 안정성](api-conventions.md) |
| 0006 | IPC 서버 | [IPC 서버](../architecture/ipc-server.md) |
| 0007 | IPC 서버 | [IPC 서버](../architecture/ipc-server.md) |
| 0008 | IPC 서버 | [IPC 서버](../architecture/ipc-server.md) |
| 0009 | 저장소 시스템 (Storage System) | [저장소 시스템 (Storage System)](../design/systems/storage.md) |
| 0010 | 저장 계약과 writer 명부 | [저장 계약과 writer 명부](../design/systems/storage-contract.md) |
| 0011 | 에이전트 메모리 시스템 | [에이전트 메모리 시스템](../design/systems/memory.md) |
| 0012 | 플러그인 권한 모델 | [호스트 호출 계약](plugin-permissions.md) |
| 0013 | 터미널 (Terminal) | [터미널 (Terminal)](../surfaces/terminal/index.md) |
| 0014 | 터미널 (Terminal) | [터미널 (Terminal)](../surfaces/terminal/index.md) |
| 0015 | 터미널 (Terminal) | [터미널 (Terminal)](../surfaces/terminal/index.md) |
| 0016 | 종료 시퀀스 — 종료 cascade + Drop tail | [종료 시퀀스 — 종료 cascade + Drop tail](../architecture/shutdown-sequence.md) |
| 0017 | 포커스 정책 (운영 상세) | [포커스 정책 (운영 상세)](../design/policies/focus.md) |
| 0018 | 스크린샷 방법 | [스크린샷 방법](../ai-verification/screenshot-methods.md) |
| 0019 | 키 매핑 설계 (운영 상세) | [키 매핑 설계 (운영 상세)](../design/policies/key-mapping.md) |
| 0020 | 원격 접속 프로필 + Passkey (remote_tool) | [원격 접속 프로필 + Passkey (remote_tool)](../features/remote-profiles/index.md) |
| 0021 | Attach 메커니즘 | [Attach 메커니즘](attach-behavior.md) |
| 0022 | 원격 attach (Remote attach) | [원격 attach (Remote attach)](../features/remote-attach/index.md) |
| 0023 | Attach 메커니즘 | [Attach 메커니즘](attach-behavior.md) |
| 0024 | Surface Highlight (주의 환기) | [Surface Highlight (주의 환기)](../features/surface-highlight/index.md) |
| 0025 | 플러그인 권한 모델 | [호스트 호출 계약](plugin-permissions.md) |
| 0026 | 플러그인 제작 가이드 | [플러그인 제작 가이드](plugin-development.md) |
| 0027 | 훅 (Surface / Global hooks) | [훅 (Surface / Global hooks)](../features/hooks/index.md) |
| 0028 | egui-mesh 렌더 채널 | [egui-mesh 렌더 채널](egui-mesh-channel.md) |
| 0029 | WebView 호스트 계약 | [WebView 호스트 계약](../design/systems/webview.md) |
| 0030 | 플러그인 제작 가이드 | [플러그인 제작 가이드](plugin-development.md) |
| 0031 | 파일 핸들러 (File handler) | [파일 핸들러 (File handler)](../features/file-handler/index.md) |
| 0032 | 웹훅 (Inbound webhook listener) | [웹훅 (Inbound webhook listener)](../features/webhook/index.md) |
| 0033 | Event Bus 1.0 카탈로그 | [Event Bus 1.0 카탈로그](../reference/event-catalog.md) |
| 0034 | 터미널 출력 구조화 (Terminal output) | [터미널 출력 구조화 (Terminal output)](../features/terminal-output/index.md) |
| 0035 | 테마 시스템 (운영 상세) | [테마 시스템 (운영 상세)](../design/systems/theme.md) |
| 0036 | 내부 팝업 시스템 | [내부 팝업 시스템](../design/systems/popup.md) |
| 0037 | `tasty-ui-widgets` — 본체·갤러리 공유 UI primitive | [`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../architecture/ui-widgets-crate.md) |
| 0038 | 레이아웃 프리셋 (Layout presets) | [레이아웃 프리셋 (Layout presets)](../features/layout-presets/index.md) |
| 0039 | 타입 있는 길이 (Typed length) | [타입 있는 길이 (Typed length)](../concepts/typed-length.md) |
| 0040 | 국제화 (i18n) | [국제화 (i18n)](i18n.md) |
| 0041 | child-terminal (호스트 내재화 자식 터미널 관리 · `tasty terminal`) | [child-terminal (호스트 내재화 자식 터미널 관리 · `tasty terminal`)](../features/child-terminal/index.md) |
| 0042 | 다중 에이전트 협업 (Agent collaboration) | [다중 에이전트 협업 (Agent collaboration)](../features/agent-collaboration/index.md) |
| 0043 | CLI 크레이트 내부 구조 — 세 갈래 대칭 | [CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md) |
| 0044 | Agent Stream (`com.tasty.agent-stream`) | [Agent Stream (`com.tasty.agent-stream`)](../plugins/agent-stream/index.md) |
| 0045 | 유닛 테스트 격리 — 사용자 환경을 읽지 않는다 | [유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md) |
| 0046 | 수정 후 자체 검증 (Self-verification) | [수정 후 자체 검증 (Self-verification)](self-verification.md) |
| 0047 | CI · 훅 게이트 매트릭스 — 어떤 검사가 어디서 도는가 | [CI · 훅 게이트 매트릭스 — 어떤 검사가 어디서 도는가](ci-gates.md) |
| 0048 | 가드 검증 | [가드 검증](guard-verification.md) |
| 0049 | Tasty 문서 모델 (Documentation Model) | [Tasty 문서 모델 (Documentation Model)](../documentation-model.md) |
| 0050 | ADR-XXXX: <짧고 구체적인 선택> | [ADR-XXXX: <짧고 구체적인 선택>](../adr/template.md) |
| 0051 | 릴리스 절차 | [릴리스 절차](release.md) |
| 0052 | macOS 권한 (TCC) | [macOS 권한 (TCC)](../features/macos-permissions/index.md) |
| 0053 | HTML Viewer (`com.tasty.html`) | [HTML Viewer (`com.tasty.html`)](../surfaces/html/index.md) |
| 0054 | App·Engine·View 상태 소유권 | [App·Engine·View 상태 소유권](app-state-ownership.md) |
| 0055 | 이벤트 저장소 (`tasty-event-store`) | [이벤트 저장소 (`tasty-event-store`)](../architecture/event-store.md) |
| 0056 | 아키텍처 개요 | [아키텍처 개요](../architecture/index.md) |
| 0057 | CLI / IPC API 규약 — 명명 + 안정성 | [CLI / IPC API 규약 — 명명 + 안정성](api-conventions.md) |
| 0058 | 헤드리스 IPC 지원 범위 | [헤드리스 IPC 지원 범위](headless-ipc-surface.md) |
| 0059 | 포커스 정책 (운영 상세) | [포커스 정책 (운영 상세)](../design/policies/focus.md) |
| 0060 | 터미널 (Terminal) | [터미널 (Terminal)](../surfaces/terminal/index.md) |
| 0061 | Attach 메커니즘 | [Attach 메커니즘](attach-behavior.md) |
| 0062 | Agent task runner | [Agent task runner](agent-runner.md) |
| 0063 | 이벤트 저장소 (`tasty-event-store`) | [이벤트 저장소 (`tasty-event-store`)](../architecture/event-store.md) |
| 0064 | 이벤트 저장소 (`tasty-event-store`) | [이벤트 저장소 (`tasty-event-store`)](../architecture/event-store.md) |
| 0065 | 플랫폼과 버전 호환성 | [플랫폼과 버전 호환성](../principles/platform-and-compatibility.md) |
| 0066 | 릴리스 절차 | [릴리스 절차](release.md) |
| 0067 | Agent task runner | [Agent task runner](agent-runner.md) |
| 0068 | Agent task runner | [Agent task runner](agent-runner.md) |
| 0069 | Agent task runner | [Agent task runner](agent-runner.md) |
| 0070 | Agent task runner | [Agent task runner](agent-runner.md) |
| 0071 | 닫기와 실행 자원 회수 | [닫기와 실행 자원 회수](../architecture/close-sequence.md) |
| 0072 | Agent task runner | [Agent task runner](agent-runner.md) |
| 0073 | 플러그인 권한 모델 | [호스트 호출 계약](plugin-permissions.md) |
| 0074 | macOS 권한 (TCC) | [macOS 권한 (TCC)](../features/macos-permissions/index.md) |
| 0075 | Agent task runner | [Agent task runner](agent-runner.md) |
| 0076 | Explorer (내장 파일 관리자 surface) | [Explorer (내장 파일 관리자 surface)](../surfaces/explorer/index.md) |
| 0077 | Explorer 파일 작업 — 대상과 결과의 계약 | [Explorer 파일 작업 — 대상과 결과의 계약](../surfaces/explorer/file-operations.md) |
| 0078 | 저장 계약과 writer 명부 | [저장 계약과 writer 명부](../design/systems/storage-contract.md) |

한 원본에 여러 주제가 있으면 위 문서는 진입점이며 연결된 기능별 정본이 세부 계약을 소유한다. 전체화면·캡처, 트레이·종료, 이미지·마크다운·Git 도구 등은 각각의 기능 문서로 나눈다. 제품 원칙과 다른 현행 동작은 해당 정본에서 미충족 범위를 명시하며 예외로 승인한 것으로 취급하지 않는다.
