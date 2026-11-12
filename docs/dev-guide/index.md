# 개발 가이드

Tasty 개발자를 위한 가이드다. Tasty를 사용하는 에이전트용 명령은 [reference/](../reference/index.md).

> 핵심 원칙 — **독립 검증**: tasty 개발 환경이 곧 tasty(dogfooding)다. debug 빌드는 별도 루트(`~/.tasty-debug/`)로 release(`~/.tasty/`)와 격리돼, agent 가 release tasty 안에서 동작 중이어도 자기 debug 빌드를 따로 띄워 충돌 없이 검증할 수 있다. [self-verification 독립 검증](self-verification.md#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다).

## 시작 / 검증

| 문서 | 내용 |
|------|------|
| [branching](branching.md) | `develop`·`release`·`main` 브랜치의 뜻, 작업 브랜치 반영·release 승격·태그·hotfix 역병합, merge 커밋 허용 범위 |
| [git-hooks](git-hooks.md) | Git 훅 설치, Clippy로 통합한 개발 컴파일 검사, 검사 이름별 진행·시간 표시와 오류 로그 확인 |
| [shell-scripts](shell-scripts.md) | `scripts/`·`.githooks/`·`Justfile`·워크플로 `run:` 규약 — 조기에 끝나는 소비자를 파이프 오른쪽에 두지 않는다(SIGPIPE) |
| [ci-gates](ci-gates.md) | CI·Git 훅의 실제 검사 명령과 실행 조건 |
| [self-verification](self-verification.md) | 격리 인스턴스에서 직접 검증하는 절차와 결과를 해석하는 기준 |
| [종료 숫자 계측](../architecture/shutdown-sequence.md#writer-종료-시간과-콜백-원시-표본) | writer cut/join/timeout과 debug GUI 콜백 raw 표본의 범위 |

## 코드 정책

| 문서 | 내용 |
|------|------|
| [commit-convention](commit-convention.md) | Conventional Commits |
| [adr-migration](adr-migration.md) | 이전 ADR 번호에서 현재 원칙·설계·기능 정본 찾기 |
| [adr-index](adr-index.md) | ADR 헤더로 인덱스 생성, 충돌 해결, 병합할 결정의 중복 검토 |
| [adr-renumber](adr-renumber.md) | ADR 을 새 번호로 옮기고 레포 전체의 인용을 한 번에 고치는 도구(`adr-renumber` bin) — 매핑 파일 형식, 고치는 형태와 보고만 하는 형태, 삭제되는 ADR 을 부르는 자리가 쓰기를 막는 이유, 도구가 못 보는 것 |
| [error-handling](error-handling.md) | Result 처리·종료 시 반환값·락 poison 복구와 관측 범위 |
| [clippy-policy](clippy-policy.md) | 위치별 allow·expect와 플랫폼별 타입 차이, 워크스페이스 끄기 지양 · unsafe `// SAFETY:` 작성 + 자가검토 7문 |
| [complexity-gate](complexity-gate.md) | 복잡도 게이트(cognitive deny + 파일 SLOC), 오류 분기·역할별 모듈 분리와 예외 컨벤션 |
| [duplicated-sets](duplicated-sets.md) | 같은 집합이 여러 곳에 적힐 때 — 자리로 셀 수 있는 것, 합칠 곳과 남길 곳을 가르는 기준 |
| [theme › 색 생성 정책](../design/systems/theme.md#색-생성-정책) | 색 생성 newtype + clippy 강제 (design/systems/theme 의 절) |
| [i18n](i18n.md) | 번역 키·갤러리 공용 문구·CLI 도움말과 고정 허용값·사용자 오버라이드·폰트·하드코딩 예외 |

## 빌드 / 릴리스

| 문서 | 내용 |
|------|------|
| [build](build.md) | 워크스페이스·빌드 프로필·문서 가드 최적화 · debug 전용 번들 자동 동기화 · 공용 모듈의 GUI 정의 경계 · Windows 파일 identity 경계 · 로컬 dist 산출물 명령 |
| [release](release.md) | 버전·lockfile·README 배지 정합 · 릴리스 워크플로(태그 → CI) · self-hosted 러너 인벤토리·운영 |
| [dep-issues](dep-issues.md) | 의존성 future-incompat 모니터링과 포크 핀 |
| [x11-ime-recovery](x11-ime-recovery.md) | 원격 XIM 응답 기한, 서버 종료·교체 복구와 입력 수명 |

## 구현 패턴

| 문서 | 내용 |
|------|------|
| [model-view-split](model-view-split.md) | Model + Host View 분리와 Runtime 요청 계약 |
| [gpu-rendering](gpu-rendering.md) | View 분할 비율 미리보기·확정 경계, GPU 렌더링 구조 · 성능 측정 · 중복 표면 설정 억제 · surface 오류 지연 재시도 |
| [egui-mesh-channel](egui-mesh-channel.md) | plugin egui mesh → host 합성 렌더 채널 (docs/dev-guide/egui-mesh-channel.md) |
| [design-change-workflow](design-change-workflow.md) | 디자인 변경 루프 — 요청문서→디자이너 세션의 tasty-page 갤러리 반영→tasty 갤러리/본체 정합, 앱이 따르는 디자인 판(`dtcg/SOURCE`) |
| [gallery-first](gallery-first.md) | 새 UI 컴포넌트는 디자인→갤러리→본체 순서 (cut 금지), specimen 기하의 역할 명명 |
| [popup-implementation](popup-implementation.md) | Popup(`PopupDef` 시스템) |
| [dag-layout](dag-layout.md) | Task DAG 좌표 계산(`tasty-dag-layout`) — 레이어 배치·엣지 라우팅·어댑터 경계 |
| [context-menu](context-menu.md) | OS 네이티브 컨텍스트 메뉴 |
| [timer-hub](timer-hub.md) | 메인 루프의 주기 작업 등록·취소·데드라인과 대기 방식 |
| [crash-diagnostics](crash-diagnostics.md) | 크래시 진단·로그 위치 · 콜백 안팎과 대기 IPC 정지 관측 · GUI/headless 진단 범위 · WebView 로드/배치/표시 실패 |
| [memory-leak-soak](memory-leak-soak.md) | 메모리 누수 soak 테스트 — 4계층 지표·판정·플랫폼별 attribution |

## IPC / Agent

| 문서 | 내용 |
|------|------|
| [api-conventions](api-conventions.md) | CLI/IPC 명명·버전 정책, 구조 요청의 영속 재시도와 응답 계약, CLI 진입점 검증 |
| [cli-structure](cli-structure.md) | CLI 크레이트 내부 세 갈래(commands/ · request/ · local/), `Dispatch`, Codex 호출·실행 문맥 |
| [debug-ipc](debug-ipc.md) | debug 전용 IPC + 격리 |
| [headless-ipc-surface](headless-ipc-surface.md) | 헤드리스 IPC의 단일 진입 검사·관측·PTY 종료 수명과 메서드별 제공 범위 |
| [headless-build-boundaries](headless-build-boundaries.md) | GUI·헤드리스 컴파일 경계, 시험 helper의 OS 조건과 여덟 빌드 조합 검사 |
| [app-state-ownership](app-state-ownership.md) | App·Engine·View 상태 소유권, EngineRead와 실행 대여, receipt 기반 정리와 headless 문맥 |
| [attach-behavior](attach-behavior.md) | GUI client 모듈·접근 범위·대기 수명, attach(서버=loopback / 로컬-원격=클라이언트) · self-attach connector 진입/완료 검증 · raw 출력 오류 처리 |
| [agent-runner](agent-runner.md) | task DAG executor + 동기화 primitive · 결과 불명 뒤 종결 의무 확인·알림 복구 |

## 외부 프로그램 구동

| 문서 | 내용 |
|------|------|
| [external-interaction](external-interaction.md) | PTY 로 구동하는 외부 TUI(child Claude Code / codex 등)의 동작 때문에 생기는 함정 모음 |

## 테스트

| 문서 | 내용 |
|------|------|
| [e2e-tests](e2e-tests.md) | E2E 인스턴스 공유·환경 격리·timeout·진단·스냅샷 검증 |
| [unit-test-isolation](unit-test-isolation.md) | 홈·환경변수·파일·프로세스·시간 격리, 공용 writer 계측 기준값과 부팅 인계·descriptor 명부, feature별 helper 범위·winit EventLoop의 새 프로세스 격리와 자식 CLI argv 검증·빈 profile admission |
| [guard-population](guard-population.md) | 검사 대상 수집, 형식별 인용 주석과 Objective-C 줄 주석 판독, 분석 전 후보 필터·실행 내 결과 공유, 빈 결과 검출, 픽스처의 전체 엔트리 열거, 직렬화 락·플랫폼 선언의 정확한 명부와 delegate 반환 변이 |
| [guard-verification](guard-verification.md) | 실제 검사 경로와 합성 반례·변이 결과 확인 |
| [guard-relocation](guard-relocation.md) | 검사 파일 이동·제품 모듈 분리 전후의 경로·대상·실행 조건 확인 |

## Plugin

| 문서 | 내용 |
|------|------|
| [plugin-development](plugin-development.md) | 상대 설치 루트의 실행·CWD·자산 경계, plugin 제작·미수신 spawn 결과 회수 + 민감 데이터(regular · secret · keyring) + 호스트 런타임 계약 (실행 중 tasty 에 플러그인만 반복 갱신 §9.1 — 호스트 재빌드 불필요) |
| [paired-agent-handlers](paired-agent-handlers.md) | Claude/Codex 짝 핸들러의 공개 응답·번역·완료 알림 호환 경계 |
| [plugin-persistence](plugin-persistence.md) | Plugin 설정·신뢰 기록의 잠금 병합·파일 확정과 런타임 효과 경계 |
| [plugin-runtime](plugin-runtime.md) | 호스트가 plugin 프로세스에게 주는 런타임 계약 — 수명주기 · namespace · 채널 |
| [plugin-permissions](plugin-permissions.md) | namespace owner 기동 범위 · 권한 모델 |
| [plugin-packaging](plugin-packaging.md) | 서명 + staging 동기화 + 생태계 정책(자동 upgrade · 호환성 분류) |

DAG 화면의 읽기 요청과 App 워커 소유 경계는 [Model + Host View 분리](model-view-split.md)에 설명한다.

- [HTML entry 전송](html-transfer.md) — source 메타데이터, opt-in 협상, 같은 root의 파일 worker, Windows root fixture, native 응답 대기와 문서 세대.

- [HTTP transport bridge](html-http-bridge.md) — public Linux X11 App/native renderer, GUI/headless HTTP/HTTPS/Reload server protocol, bounded encoded receiving and consent, exact-body verification/replay, native source/owner fences, authenticated endpoints and TLS-only UNKNOWN_CA proof, singleton probe pool, bounded typed profile-lock admission, selected-clear and retained-view Reload; component-test scope is separate from actual native observations.

- [Windows HTTP context](windows-http-context.md) — WebView2 remote HTTP view: authenticated HTTP proxy front, exact-leaf certificate check, response-header script policy and Reload limits.
- [macOS HTTP context](macos-http-context.md) — WebKit remote HTTP view on macOS 27+: per-store HTTP CONNECT proxy, loopback aliases and content rule, exact-leaf certificate check and Reload.
