# 설계 (Design)

*현재 시스템이 어떻게 동작하나* 를 기술한다 — 여러 기능을 가로지르는 **정책**(무엇을 지키나), 공용 **시스템**(무엇이 있나), 모듈 경계를 넘는 **흐름**(어떻게 흐르나). 결정의 *근거·대안·재검토 조건* 은 여기가 아니라 [adr/](../adr/index.md) 에 둔다. 시각 수치·토큰 값은 재서술하지 않고 디자인 시스템을 링크한다([documentation-model](../documentation-model.md)).

## 정책 (policies/)

| 문서 | 내용 |
|------|------|
| [focus](policies/focus.md) | 포커스는 사용자의 것 — 에이전트 행동(IPC/CLI)은 포커스를 바꾸지 않고 release 엔 포커스 변경 API 가 없다 |
| [cwd](policies/cwd.md) | surface 가 자기 cwd 를 정의·갱신하는 방식 — 새 탭 상속·split carry·링크 해석·닫힌 항목 복원의 기준, 그리고 생성·변환 경로에서 cwd 가 손실 없이 carry 되는 invariant |
| [busy-indicator](policies/busy-indicator.md) | 탭·워크스페이스의 "실행 중" 시각 표시 — focus 와 무관 |
| [key-mapping](policies/key-mapping.md) | 바인딩 문자열의 OS별 키 매핑과 위치 기반 추상화 — 모든 단축키는 `KeybindingSettings` 로 노출 |
| [gallery-completeness](policies/gallery-completeness.md) | 갤러리는 본체의 모든 UI 컴포넌트를 노출한다 — cut 금지 |
| [system-tray](policies/system-tray.md) | 백그라운드 진입 시 전 OS 트레이/상태 영역 best-effort, 없으면 최소화 폴백 |

## 시스템 (systems/)

| 문서 | 내용 |
|------|------|
| [theme](systems/theme.md) | 색·타이포·간격의 단일 출처 `Theme` + UI 디자인 규칙·그림자 토큰과 검사 범위 |
| [design-token-mapping](systems/design-token-mapping.md) | claude design semantic 토큰을 `Theme` 필드로 옮기는 매핑 + 반대 방향(DTCG 토큰 ↔ Rust `Theme` 필드 ↔ 호출처) 크로스워크 |
| [design-gallery-mapping](systems/design-gallery-mapping.md) | 디자인 jsx 하위 컴포넌트 ↔ 갤러리 specimen ↔ 호스트 함수 3자 매핑 (원격 picker 행·pane 포함) |
| [design-parity-notes](systems/design-parity-notes.md) | 디자인(html/CSS) ↔ 구현(winit/egui) 의 구조적 차이와 전사 원칙 |
| [icons](systems/icons.md) | 라인/필 아이콘 세트 — SVG 지오메트리 단일 소스와 소비 구조 |
| [popup](systems/popup.md) | View 내부 가상 창 — `PopupManager` + `PopupDef` 로 관리, 포커스 비독점 · 자식 파일 피커 범위 상속/숨김 보존 |
| [toast](systems/toast.md) | 자동으로 사라지는 휘발성 피드백 UI — `ToastManager` |
| [banner](systems/banner.md) | 스코프 상단의 지속·인터랙티브 안내+조치 오버레이(4번째 오버레이 개념) |
| [modifier-hint](systems/modifier-hint.md) | modifier 홀드 동안 뜨는 단축키 목록 오버레이(5번째 오버레이 개념) |
| [fullscreen-stage](systems/fullscreen-stage.md) | 창 전체를 독점하는 독립 표면 — 기존 트리 밖의 무대 (기능 명세 포함) |
| [webview](systems/webview.md) | 창 안의 native 자식 창 — 표면 열둘+`Drop`, 부모 handle·좌표·스레드 친화성·수명·탐색 상태·키보드 포커스 회수의 입출력 계약 |
| [memory](systems/memory.md) | 에이전트 메모리 `memory.db` 의 가시성·소유권 모델 |
| [storage](systems/storage.md) | 영속 데이터의 텍스트(TOML/셸) ↔ SQLite 분할, 창 간 공유 최근 목록 |

## 흐름 (flows/)

모듈 경계를 넘는 동작의 *처리 흐름* — 정책(무엇을)이나 시스템(무엇이 있나)이 아니라 *어떻게 흐르나*.

| 문서 | 내용 |
|------|------|
| [action-dispatch](flows/action-dispatch.md) | Intent 큐 — 호스트 내부 동작 디스패치 모델 |
