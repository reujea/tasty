# 기능 (Features)

각 기능은 폴더 하나다 — `features/<f>/index.md`(기획, 내부 동작 · 1순위)와 화면(투영 · 0..N). 화면은 기획문서의 `## 화면` 절이 기본이고, 화면이 둘 이상이거나 자기 규칙을 가지면 `screens/<s>.md` 파일이다(기준은 documentation-model §3). 모델·작성 규칙은 [documentation-model.md](../documentation-model.md), 작성 전 [identity.md](../identity.md) 필독.

양식: [`_feature.template.md`](_feature.template.md) (기획) · [`_screen.template.md`](_screen.template.md) (화면). 새 기능은 양식을 복사해 채운다.


## 카탈로그

| 기능 | 주체 | 화면 |
|------|------|------|
| [main-view](main-view/index.md) — MainView (메인 윈도우) | 로컬 사용자 · AI Agent · 원격 | [전체 레이아웃](main-view/index.md#화면) |
| [work-area](work-area/index.md) — 작업 영역 (Workspace/Pane/Tab/Surface 도메인) | 로컬 사용자 · AI Agent · 원격 | [화면](work-area/index.md#화면) |
| [terminal](terminal/index.md) — 터미널 (PTY·VTE·보조키 방향키·scrollback·GPU) | 로컬 사용자 · AI Agent · 원격 | GPU 그리드 |
| [terminal-search](terminal-search/index.md) — 터미널 검색 (스크롤백+화면) | 로컬 사용자 | 검색 바 popup |
| [terminal-link](terminal-link/index.md) — 링크 hover·수식키 좌클릭·좌클릭 열기 끄기 · 우클릭 메뉴 | 로컬 사용자 | hover 하이라이트 |
| [workspace-tabs](workspace-tabs/index.md) — 탭 스트립 (Pane 별 탭 바) | 로컬 사용자 | [화면](workspace-tabs/index.md#화면) · [구현](workspace-tabs/index.md#구현) |
| [workspace-category](workspace-category/index.md) — 워크스페이스 카테고리 (사이드바 폴더) | AI Agent · 로컬 사용자 | [사이드바](sidebar/index.md#화면) 카테고리 섹션 (토글 on 시 그룹 렌더) · 레일 `---` 팝업 · 생성/이름변경/삭제 다이얼로그 |
| [window-chrome](window-chrome/index.md) — 윈도우 크롬 (CSD 타이틀바) | 로컬 사용자 | [화면](window-chrome/index.md#화면) |
| [workspace-status-bar](workspace-status-bar/index.md) — 상태바 (작업영역 하단) | 로컬 사용자 | [화면](workspace-status-bar/index.md#화면) |
| [sidebar](sidebar/index.md) — 사이드바 (MainView 좌측 패널) | 로컬 사용자 | [화면](sidebar/index.md#화면) |
| [tools-menu](tools-menu/index.md) — 도구 메뉴 (사이드바 도구 버튼) | 로컬 사용자 | [메뉴](tools-menu/index.md#화면) |
| [settings](settings/index.md) — 설정 창 (사이드바 설정 버튼) | 로컬 사용자 | [창](settings/screens/settings.md) |
| [plugin-system](plugin-system/index.md) — 플러그인 관리 (설치 패키지 활성/비활성 · 사이드바 플러그인 버튼) | 로컬 사용자 · AI Agent | [창](plugin-system/index.md#화면) |
| [command-palette](command-palette/index.md) — 명령 팔레트 (도구 메뉴 항목) | 로컬 사용자 | [화면](command-palette/index.md#화면) |
| [fullscreen-stage](../design/systems/fullscreen-stage.md#기능-명세--상태--인터페이스--acceptance-criteria) — 전체화면 무대 (창 전체를 독점하는 독립 표면 · 기획은 design 시스템 문서의 절) | 로컬 사용자 | 없음 (무대 자체가 화면) |
| [tutorial](tutorial/index.md) — 튜토리얼 (화면 구조·분할 실습·명령 안내, 완료/재개 기록) | 로컬 사용자 | 마커+말풍선+주제 팝업 |
| [remote-profiles](remote-profiles/index.md) — 원격 접속 프로필 + Passkey (도구 메뉴 항목) | 로컬 사용자 · AI Agent | [창](remote-profiles/screens/remote-tool.md) |
| [remote-attach](remote-attach/index.md) — 원격 attach (점유/mirror) | 원격 · AI Agent · 로컬(force-detach) | [GUI mirror](remote-attach/index.md#화면) |
| [remote-screenshot-clipboard](remote-screenshot-clipboard/index.md) — 원격 스크린샷 → 클립보드 (mirror 포커스 시 원격 clipboard 반영) | 로컬 사용자 | 없음 (토스트만) |
| [listening-ports](listening-ports/index.md) — 리스닝 포트 뷰어 | 로컬 사용자 | [팝업](listening-ports/index.md#화면) |
| [keybindings](keybindings/index.md) — 단축키 (KeybindingSettings 도메인) | 로컬 사용자 | [설정 탭](settings/screens/settings.md) |
| [clipboard](clipboard/index.md) — 클립보드 (복사/붙여넣기/선택) | 로컬 사용자 | [뷰어 plugin](../plugins/clipboard-viewer/index.md) |
| [notifications](notifications/index.md) — 알림 (전역 IPC 목록·창별 패널/배지) | 로컬 사용자 · AI Agent | 패널 popup |
| [surface-highlight](surface-highlight/index.md) — Surface 주의 환기 (공유 상태·3채널·completion) | AI Agent · 로컬 사용자 | 없음 (테두리/탭/배지) |
| [file-handler](file-handler/index.md) — 파일 핸들러 (식별→디스패치, origin 소유권·탭 선택 보존) | 로컬 사용자 · AI Agent · plugin | [설정 탭](settings/screens/settings.md) · picker |
| [native-file-picker](native-file-picker/index.md) — 네이티브 파일 피커 (로컬+원격 겸용, Tools 메뉴 · plugin 트리거 · 설정 창 안의 로컬 전용 재사용 · 긴 파일명 말줄임 · 부모 범위 상속과 숨김 보존) | 로컬 사용자 · plugin | popup (갤러리 specimen) |
| [themes](themes/index.md) — 테마 추가/관리 (TOML) | 로컬 사용자 | [설정 탭](settings/screens/settings.md) |
| [language-packs](language-packs/index.md) — 사용자 언어팩 발견·선택, host/plugin CLI 도움말 및 번역 오버라이드 | 로컬 사용자 | [설정 탭](settings/screens/settings.md) General › Language · 부팅 경고 토스트 |
| [lua-hooks](lua-hooks/index.md) — Lua 스크립트(등록 + 단축키/이벤트 자동실행 트리거, host API) | 로컬 사용자 | [0031](../adr/0031-lua-host-api-only-worker-isolated.md) |
| [agent-collaboration](agent-collaboration/index.md) — 다중 에이전트 협업 (`agent.*`) | AI Agent | [DAG 그래프 surface](agent-collaboration/screens/dag-graph-surface.md) · [DAG 목록 popup](agent-collaboration/screens/dag-list-popup.md) |
| [child-terminal](child-terminal/index.md) — 자식 터미널 관리 (`tasty terminal`, soft 점유) | AI Agent | 없음 (headless) |
| [headless-pty](headless-pty/index.md) — Surface 없는 PTY primitive (`tasty pty`, exit-code·승격) | AI Agent | 없음 (headless) |
| [human-handoff](human-handoff/index.md) — 휴먼 핸드오프 (approval) | AI Agent · 로컬 사용자 | approval popup |
| [telemetry](telemetry/index.md) — 텔레메트리 (IPC 진입 단일 관측/비용/cap) | AI Agent · 로컬 사용자 | 없음 |
| [terminal-output](terminal-output/index.md) — 출력 구조화 (parse/commands/observe) | AI Agent | 없음 |
| [capability-elevation](capability-elevation/index.md) — 권한 상승 & 감사 | AI Agent · 로컬 사용자 | elevation popup |
| [hooks](hooks/index.md) — 훅 (surface/global, 자동 실행) | 로컬 사용자 · AI Agent | [설정 탭](settings/screens/settings.md) Hook Handlers |
| [webhook](webhook/index.md) — 인바운드 웹훅 리스너 (인증 · UTF-8 변환 전 JSON 입력 상한 · HTTP 연결 정리 비용 · 재시작 복원) | 로컬 사용자 · AI Agent | [설정 탭](settings/screens/settings.md) Hook Handlers (리스너는 headless) |
| [closed-tab-restore](closed-tab-restore/index.md) — 닫힌 항목 복원 (`Ctrl+Shift+T`) | 로컬 사용자 | 없음 |
| [convert-surface](convert-surface/index.md) — Surface 타입 전환 (`Alt+'`) | 로컬 사용자 · AI Agent | convert popup |
| [surface-move](surface-move/index.md) — Surface 위치 이동 (잘라내기/여기로 이동) | 로컬 사용자 | OS 컨텍스트 메뉴 |
| [explorer](explorer/index.md) — 내장 파일 관리자 surface (탐색/열기/뷰모드) | 로컬 사용자 · AI Agent | host surface |
| [layout-persistence](layout-persistence/index.md) — 레이아웃 영속화 (창별 슬롯 파일 `layouts/NN.json`·scrollback) | 로컬 사용자 | 없음 |
| [layout-presets](layout-presets/index.md) — 레이아웃 프리셋 (`preset.*`) | 로컬 사용자 · AI Agent | PresetView |
| [accessibility](accessibility/index.md) — 접근성 (reduced motion 등) | 로컬 사용자 | [설정 탭](settings/screens/settings.md) |
| [macos-permissions](macos-permissions/index.md) — macOS 권한 (TCC 파일·화면 기록 pre-warm, Full Disk Access 안내) | 로컬 사용자 | [설정 탭](settings/screens/settings.md) 일반 > 권한 |
