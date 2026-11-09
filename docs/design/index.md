# 설계 (Design)

여러 기능이 공유하는 정책과 시스템, 모듈 사이의 처리 흐름을 설명한다. 설계 선택의 이유·대안·재검토 조건은 [ADR](../adr/index.md)에 둔다. 시각 수치와 토큰 값은 복사하지 않고 디자인 시스템으로 연결한다([문서 작성 규칙](../documentation-model.md)).

## 정책 (policies/)

| 문서 | 내용 |
|------|------|
| [focus](policies/focus.md) | 포커스는 사용자의 것 — 에이전트 행동(IPC/CLI)은 포커스를 바꾸지 않고 release 엔 포커스 변경 API 가 없다 |
| [cwd](policies/cwd.md) | surface별 현재 폴더와 생성·분할·변환 시 상속 규칙. 링크 해석과 닫은 항목 복원에도 사용 |
| [busy-indicator](policies/busy-indicator.md) | 탭·워크스페이스의 "실행 중" 시각 표시 — focus 와 무관 |
| [key-mapping](policies/key-mapping.md) | 바인딩 문자열의 OS별 키 매핑과 위치 기반 추상화 — 설정 대상 단축키는 `KeybindingSettings`로 노출 |
| [gallery-completeness](policies/gallery-completeness.md) | 본체의 모든 UI 컴포넌트를 갤러리에 포함 |
| [system-tray](policies/system-tray.md) | 가능한 환경에서 트레이/상태 영역 사용. 없으면 태스크바·도크로 복귀 |

## 시스템 (systems/)

| 문서 | 내용 |
|------|------|
| [theme](systems/theme.md) | 색·타이포·간격의 단일 출처 `Theme` + UI 디자인 규칙·그림자 토큰과 검사 범위 |
| [design-token-mapping](systems/design-token-mapping.md) | Design System 토큰과 `Theme` 필드·호출처의 대응표 |
| [design-gallery-mapping](systems/design-gallery-mapping.md) | 디자인 JSX 컴포넌트·갤러리 예제·호스트 함수의 대응표. 원격 피커 행·pane 포함 |
| [design-parity-notes](systems/design-parity-notes.md) | 디자인(html/CSS) ↔ 구현(winit/egui) 의 구조적 차이와 전사 원칙 |
| [icons](systems/icons.md) | 라인/필 아이콘 세트 — SVG 지오메트리 단일 소스와 소비 구조 |
| [popup](systems/popup.md) | View 내부 가상 창 — `PopupManager` + `PopupDef` 로 관리, 포커스 비독점 · 자식 파일 피커 범위 상속/숨김 보존 |
| [toast](systems/toast.md) | 자동으로 사라지는 휘발성 피드백 UI — `ToastManager` |
| [banner](systems/banner.md) | 스코프 상단의 지속·인터랙티브 안내+조치 오버레이(4번째 오버레이 개념) |
| [modifier-hint](systems/modifier-hint.md) | modifier 홀드 동안 뜨는 단축키 목록 오버레이(5번째 오버레이 개념) |
| [fullscreen-stage](systems/fullscreen-stage.md) | 창 전체를 독점하는 독립 표면 — 기존 트리 밖의 무대 (기능 명세 포함) |
| [linux-display-backend](systems/linux-display-backend.md) | `DISPLAY`·`WAYLAND_DISPLAY`로 창과 GDK 백엔드를 고르는 규칙, XWayland 배율, 네이티브 Wayland 범위 |
| [webview](systems/webview.md) | OS 자식 창의 API·`Drop`, 부모 핸들·좌표·스레드·수명·탐색·키보드 포커스와 macOS HTTP context |
| [memory](systems/memory.md) | 에이전트 메모리 `memory.db` 의 가시성·소유권 모델 |
| [storage](systems/storage.md) | 영속 데이터의 텍스트(TOML/셸) ↔ SQLite 분할, 창 간 공유 최근 목록, 저장 기준선과 journal SQL 계측·테마 bootstrap·IPC 공개 barrier·세대별 정리 |
| [storage-contract](systems/storage-contract.md) | 저장 경로별 접수·commit·표시·실행 시점, memory.db 소비자 계약, 저장 실행기(접수·묶음·예산·flush·종료), 설정 부팅 barrier·CLI 파일 참여자 경계·child 관계 receipt·예약 확정 뒤 자식 이름 결정·점유/입력 barrier·늦은 ACK 병합·삭제된 상태 metadata 회수와 비정규 파일 거절·journal 논리 cut·ACK와 writer 명부, 고정 크기 시간 분포·미완료 요청 나이, 설정·파일 snapshot과 state 최근 목록의 미확정 revision·연속 dirty age, 공용·비밀 파일 교체 계측·중립 파일 명령 port·웹훅 ordered receipt와 보관 예산 |

## 흐름 (flows/)

모듈 사이에서 요청을 전달하고 처리하는 순서를 설명한다.

| 문서 | 내용 |
|------|------|
| [action-dispatch](flows/action-dispatch.md) | 원 origin·대상 binding을 가진 Intent와 journal admission/publication, View 후처리·직접 호출 시험의 개별 사유 |
