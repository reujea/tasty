# 개발 가이드

tasty 를 **개발하는** AI 에이전트용 가이드. tasty 를 *사용하는* 에이전트용 표면은 [reference/](../reference/index.md).

> 핵심 원칙 — **독립 검증**: tasty 개발 환경이 곧 tasty(dogfooding)다. debug 빌드는 별도 루트(`~/.tasty-debug/`)로 release(`~/.tasty/`)와 격리돼, agent 가 release tasty 안에서 동작 중이어도 자기 debug 빌드를 따로 띄워 충돌 없이 검증할 수 있다. [independent-verification](independent-verification.md).

## 시작 / 검증

| 문서 | 내용 |
|------|------|
| [git-hooks](git-hooks.md) | clone 직후 `./scripts/dev-setup.sh` + pre-commit/pre-push 검사 |
| [shell-scripts](shell-scripts.md) | `scripts/`·`.githooks/`·`Justfile`·워크플로 `run:` 규약 — 조기에 끝나는 소비자를 파이프 오른쪽에 두지 않는다(SIGPIPE) |
| [ci-gates](ci-gates.md) | CI·훅 게이트 매트릭스 — 어떤 검증 명령이 어디서(자동/수동/훅) 실제로 도는지, 자동 채널이 없는 검사는 없다고 명시 |
| [self-verification](self-verification.md) | 커밋 전 직접 검증(사용자에게 떠넘기지 않기), 길이 가드 사각 계수의 도입 추적·변이 대조 |
| [independent-verification](independent-verification.md) | dogfooding · debug↔release 격리 |
| [linux](linux.md) | Linux dev 환경(실행/재시작/스크린샷) |

## 코드 정책

| 문서 | 내용 |
|------|------|
| [commit-convention](commit-convention.md) | Conventional Commits |
| [adr-landing](adr-landing.md) | lane 착지 때 새 ADR 끼리·기존 ADR 과 같은 조항을 결정하지 않는지 대조 — 보고 도구 `scripts/adr-landing-report.sh`(겹침은 판정하지 않는다) |
| [error-handling](error-handling.md) | Result 처리·락 poison 복구와 관측 범위 |
| [clippy-policy](clippy-policy.md) | 위치별 allow 선호, 워크스페이스 끄기 지양 |
| [complexity-gate](complexity-gate.md) | 복잡도 게이트(cognitive deny + 파일 SLOC), 예외 컨벤션 |
| [duplicated-sets](duplicated-sets.md) | 같은 집합이 여러 곳에 적힐 때 — 자리로 셀 수 있는 것, 합칠 곳과 남길 곳을 가르는 기준 |
| [unsafe-checklist](unsafe-checklist.md) | `// SAFETY:` 작성 + 자가검토 5문 |
| [color-policy](color-policy.md) | 색 생성 newtype + clippy 강제 |
| [i18n](i18n.md) | 생성 help 트리·설명 없는 인자의 안내 보존; `t()` / lang 파일, host/plugin CLI 도움말·파싱 오류, plugin 사용자 오버라이드와 부모 홈 경로 확정, 공용 폰트 검사, 하드코딩 허용 예외, 강제 테스트 |

## 빌드 / 릴리스

| 문서 | 내용 |
|------|------|
| [build](build.md) | 워크스페이스·빌드 프로필 · debug 전용 번들 자동 동기화 · 공용 모듈의 GUI 정의 경계 |
| [dist-build](dist-build.md) | 로컬 dist 산출물 명령 |
| [release](release.md) | 릴리스 워크플로(버전 bump → 태그 → CI) |
| [release-runners](release-runners.md) | self-hosted runner 인벤토리·운영 |
| [dep-issues](dep-issues.md) | 의존성 future-incompat 모니터링 |
| [site](site.md) | 공개 사이트(GitHub Pages) 생성·배포 — `site/` 생성기, 사용자 가이드 `site/content/`(docs/ 는 발행 안 함), 집필 규칙과 사이트 어조, URL 구조, 영어 번역 모델(`site/content/en/` + 폴백 + 스탬프) |

## 구현 패턴

| 문서 | 내용 |
|------|------|
| [model-view-split](model-view-split.md) | Model + Host View 분리 |
| [gpu-rendering](gpu-rendering.md) | GPU 렌더링 구조 |
| [egui-mesh-channel](egui-mesh-channel.md) | plugin egui mesh → host 합성 렌더 채널 (ADR-0028) |
| [perf-benchmarks](perf-benchmarks.md) | GPU 성능 측정 |
| [design-change-workflow](design-change-workflow.md) | 디자인 변경 루프 — 요청문서→Claude design 시안→갤러리/본체/사이트 사본 정합 |
| [gallery-first](gallery-first.md) | 새 UI 컴포넌트는 디자인→갤러리→본체 순서 (cut 금지), specimen 기하의 역할 명명 |
| [popup-implementation](popup-implementation.md) | Popup(`PopupDef` 시스템) |
| [dag-layout](dag-layout.md) | Task DAG 좌표 계산(`tasty-dag-layout`) — 레이어 배치·엣지 라우팅·어댑터 경계 |
| [context-menu](context-menu.md) | OS 네이티브 컨텍스트 메뉴 |
| [timer-hub](timer-hub.md) | 중앙 타이머 허브 — 메인 루프 시간축 폴링 등록/실행, Strict·Lax, 대기 전략·waker poison 관측 |
| [crash-diagnostics](crash-diagnostics.md) | 크래시 진단·로그 위치 · GUI/headless 진단 범위 |
| [timer-hub](timer-hub.md) | 중앙 타이머 허브 — 메인 루프 시간축 폴링 등록/실행, Strict·Lax, 대기 전략 |
| [crash-diagnostics](crash-diagnostics.md) | 크래시 진단·로그 위치·WebView 로드/배치/표시 실패 |
| [memory-leak-soak](memory-leak-soak.md) | 메모리 누수 soak 테스트 — 4계층 지표·판정·플랫폼별 attribution |

## IPC / Agent

| 문서 | 내용 |
|------|------|
| [api-conventions](api-conventions.md) | CLI/IPC 명명 + 안정성/버전 정책 |
| [cli-structure](cli-structure.md) | CLI 크레이트 내부 세 갈래(commands/ · request/ · local/)와 `Dispatch` |
| [debug-ipc](debug-ipc.md) | debug 전용 IPC + 격리 |
| [headless-ipc-surface](headless-ipc-surface.md) | 헤드리스 IPC의 단일 진입 검사·관측·PTY 종료 수명과 메서드별 제공 범위 |
| [headless-build-boundaries](headless-build-boundaries.md) | 헤드리스 빌드의 정의 경계 세 갈래와 그것을 재는 여덟 칸 |
| [app-state-ownership](app-state-ownership.md) | `AppState` 필드마다 도메인 사실·사용자 view 상태·실행 자원 분류와 수명·소유자·headless 유무 |
| [cli-ipc-surface](cli-ipc-surface.md) | CLI 진입점 유무를 가르는 판별식과 그것을 실행으로 세는 법 |
| [attach-behavior](attach-behavior.md) | attach(서버=loopback / 로컬-원격=클라이언트) · self-attach connector 진입/완료 검증 |
| [agent-runner](agent-runner.md) | task DAG executor + 동기화 primitive |
| [agent-identification](agent-identification.md) | `AgentId` 도출(잠정 모델) |
| [lua-hooks](lua-hooks.md) | Lua hook 호스트 측 매핑 |

## 외부 프로그램 구동

| 문서 | 내용 |
|------|------|
| [external-interaction](external-interaction/index.md) | PTY 로 구동하는 외부 TUI(child Claude Code / codex 등)의 동작 때문에 생기는 함정 모음 |

## 테스트

| 문서 | 내용 |
|------|------|
| [e2e-tests](e2e-tests.md) | E2E 인스턴스 공유·환경 격리·timeout 정책 + 부팅 이정표와 포트 공개 동기화 검증·GUI stderr 수명과 실패 진단 |
| [unit-test-isolation](unit-test-isolation.md) | 유닛 테스트를 로컬 상태(홈 `config.toml` · env · 이 머신에만 있는 파일시스템 경로)로부터 격리하는 규칙 — 설정 주입 지점 + env RAII 가드 + 파일시스템 픽스처 직접 생성·PID와 호출별 counter 격리·시계 nonce 한계 + feature 별 테스트 게이팅 + 병렬 경합(flake) 처방과 temp 바인딩 범위와 가드 검증(§7) + 공유 픽스처 `test_state()` 가 진짜 셸을 fork 한다는 것과 그 횟수를 세는 법(§8) |
| [e2e-tests](e2e-tests.md) | E2E 인스턴스 공유·환경 격리·timeout 정책 + GUI stderr 수명과 실패 진단 |
| [unit-test-isolation](unit-test-isolation.md) | 유닛 테스트를 로컬 상태(홈 `config.toml` · env · 이 머신에만 있는 파일시스템 경로)로부터 격리하는 규칙 — 설정 주입 지점 + env RAII 가드 + 파일시스템 픽스처는 테스트가 직접 생성 + feature 별 테스트 게이팅 + 병렬 경합(flake) 처방과 가드 검증(§7) + 공유 픽스처 `test_state()` 가 진짜 셸을 fork 한다는 것과 그 횟수를 세는 법(§8) |
| [tui-testing](tui-testing.md) | tui-simulator + debug 셀 검증 |
| [guard-population](guard-population.md) | 모수가 걷기가 아니라 `const` 배열·fixture 에서 오는 가드 — drift 가 위험이고 비면 조용히 통과한다. 목록을 소스 추출로 바꾸는 절차(ADR-0133 이 못 다루는 갈래) + 직접 `read_dir` 자리의 갈래(순회의 뿌리로 가른다)와 세는 법 · 판정값이 0 이어도 정상인 뿌리는 판정값이 아니라 「봤다」로 잰다 |
| [guard-verification](guard-verification.md) | 가드가 **자기가 주장하는 것을 실제로 판정하는가** — 변이를 죽인 것이 컴파일러/타입/고아 파일일 수 있다, 술어가 대리(키 이름)를 본다, 눈멂의 방향이 한쪽이면 그 수는 상한, 지표를 목적함수로 삼지 않는다 |
| [guard-relocation](guard-relocation.md) | 가드 파일을 **옮길 때** 깨지는 것 — 옛 경로 인용은 시끄럽고 새 뿌리에 들어가는 것은 조용하다. 축 여섯(새 뿌리가 세기 시작한다 · 그 내용이 위반의 모양이다 · 뿌리 표현식이 위치 의존이다 · 파일이 자기 채널을 주장한다 · 뿌리별 하한은 합이 불변인 채로 뚫린다 · 이동 뒤에 생긴 인용)와 이동 전에 재는 도구, 줄어드는 뿌리의 하한을 낮추지 않는 법 · 이동이 브랜치들에 걸쳐 있는 동안 어느 패키지의 타깃인지 한 줄로 묻는 법과 잔여 위험을 받는 자리 · 「바뀐 파일 → 그것을 보는 가드」 역방향 색인이 어디까지 되는가(뿌리 단위·루트 tests/ 한정)와 왜 래칫을 못 거는가(정확도를 답을 세서 검증할 수 없다) · 가드는 통합 타깃 밖 인라인 `#[cfg(test)]` 에도 산다 — 목록 대신 세는 명령, 그리고 비용은 검사가 아니라 부르는 형태의 성질이라는 것 |

## Plugin

| 문서 | 내용 |
|------|------|
| [plugin-development](plugin-development.md) | 상대 설치 루트의 실행·CWD·자산 경계, plugin 제작 + 호스트 런타임 계약 (실행 중 tasty 에 플러그인만 반복 갱신 §9.1 — 호스트 재빌드 불필요) |
| [paired-agent-handlers](paired-agent-handlers.md) | Claude/Codex 짝 핸들러의 공개 응답·번역·완료 알림 호환 경계 |
| [plugin-permissions](plugin-permissions.md) | namespace owner 기동 범위 · 권한 모델 |
| [plugin-sensitive-data](plugin-sensitive-data.md) | 민감 데이터 |
| [plugin-packaging](plugin-packaging.md) | 서명 + staging 동기화 |
| [plugin-ecosystem](plugin-ecosystem.md) | 생태계 정책 + 자동 upgrade |
