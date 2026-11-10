# Tasty

<img src="assets/icons/tasty-melon.svg" alt="Tasty 로고" width="96" height="96" />

English: [README.md](README.md)

> **Tasty** 는 AI 코딩 에이전트를 위해 설계된 크로스 플랫폼 GPU 가속 터미널 에뮬레이터다. Windows·macOS·Linux에서 여러 에이전트의 작업을 조율하고 화면 없이도 사용할 수 있다. 에이전트는 IPC·CLI로 대상 ID를 지정해 작업한다.

[![Version](https://img.shields.io/badge/version-0.14.0-blue)](CHANGELOG.md)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](#라이선스)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)](docs/installation.md)
[![Workspace](https://img.shields.io/badge/workspace-63%20crates-orange)](crates/)

Tasty에서는 사람이 키보드·마우스로 작업하는 동안 에이전트도 IPC·CLI로 자기 작업을 수행할 수 있다. 에이전트 기능은 현재 포커스에 의존하지 않는다.

## 사용자 행동과 에이전트 행동의 분리

Tasty 의 모든 API 는 **사용자 행동**(키보드/마우스/OS 네이티브 입력)과 **에이전트 행동**(IPC 메서드/CLI 서브커맨드)을 엄격히 분리한다. 에이전트 행동의 부수효과는 사용자의 포커스·히스토리·선택 상태에 닿지 않는다 — 사용자 입력을 *재현*하는 기능(키 주입/포커스 강제 전환 등)은 release 빌드의 IPC/CLI 표면에 존재하지 않는다(debug 격리). 자세한 원칙: [사용자 통제와 데이터](docs/principles/control-and-data.md), [포커스 정책](docs/design/policies/focus.md).

## 핵심 가치

- **크로스 플랫폼** — Windows / macOS / Linux 모두 네이티브 (winit + wgpu).
- **GPU 가속 렌더** — 셀 기반 셰이더, 10+ surface 환경에서도 prepare/draw 안정.
- **Hexagonal 아키텍처** — model + ports + adapters + view + host_api 분리, 63-crate workspace.
- **에이전트 조작** — IPC·CLI는 대상 ID를 사용하며 사용자 포커스를 바꾸지 않는다. 사용자 입력 재현은 debug에만 제공한다.

## 주요 시스템

터미널 위에 세 가지 시스템을 올렸다. 셋 다 GUI 와 CLI 양쪽에서 쓸 수 있다.

### task DAG 기반 에이전트 오케스트레이션

에이전트가 다른 에이전트에게 일을 맡기면 Tasty 가 순서대로 실행한다.

- 작업은 의존 관계 그래프와 상태 기계로 관리한다. 의존하는 작업이 모두 끝나야 다음 작업이 시작되고, 순환 의존은 작업을 만들 때 거부한다.
- 작업이 실패하면 뒤따르는 작업을 건너뛰거나, 그대로 진행시키거나, 대체 작업으로 넘길 수 있다.
- 끝난 작업의 출력을 뒤 작업의 입력으로 넘길 수 있다. reduce 작업은 여러 결과를 하나로 합친다(첫 성공, 전체 수집, JSON 병합, 텍스트 이어붙이기, 사용자 명령).
- semaphore 는 동시에 도는 에이전트 수를 제한하고, lease 는 자원을 사용 중으로 표시하고, barrier 는 여러 에이전트가 모일 때까지 기다리고, rate limit 은 호출 빈도를 제한한다.
- 자식 Claude·Codex도 그래프의 노드가 된다. 기본 완료 기준은 Claude의 idle·needs_input, Codex의 idle이다. Codex의 승인 대기는 완료로 처리하지 않는다. 자식이 종료되면 실패 정책을 적용한다.
- 진행 상황은 탭에서 실시간 그래프로 보고(`tasty new tab --type dag_graph`), CLI 에서는 JSON 이나 Graphviz dot 으로 받는다.

자세한 내용: [`docs/features/agent-collaboration/index.md`](docs/features/agent-collaboration/index.md)

### 별도 프로세스로 실행되는 플러그인

- 플러그인은 각각 별도의 OS 프로세스로 실행되고 로컬 TCP 위의 JSON 메시지로 호스트와 통신한다. 호스트는 플러그인마다 응답을 확인하고, 응답이 끊긴 플러그인은 플러그인 창의 "확인 필요" 에 표시한다.
- 호스트가 종료되면 플러그인도 종료하도록 Windows의 Job Object, Linux의 부모 종료 시그널, macOS SDK의 watchdog을 사용한다.
- 플러그인은 CLI 서브커맨드, IPC 네임스페이스, 자체 서피스 종류(플러그인이 직접 렌더하거나 웹뷰로 표시), 팝업과 도구 메뉴 항목, 파일 핸들러, 설정 페이지, 훅 이벤트, DAG 작업의 완료 판정 규칙을 추가할 수 있다.
- 파일 읽기와 쓰기, 프로세스 실행, 네트워크, 클립보드, 터미널 읽기와 쓰기 같은 권한은 매니페스트에 선언하고 설치할 때 부여한다. 매니페스트는 ed25519 로 서명하며, 서명 키를 모르거나 권한이 바뀐 플러그인은 다시 신뢰 확인을 받아야 한다.
- 기본으로 들어 있는 Markdown, Image, HTML, Git, Clipboard 뷰어와 Claude Code, Codex 연동은 모두 외부 플러그인과 같은 SDK 로 만든 플러그인이다.

자세한 내용: [`docs/features/plugin-system/index.md`](docs/features/plugin-system/index.md), [`docs/dev-guide/plugin-development.md`](docs/dev-guide/plugin-development.md)

### 원격 attach

- 다른 컴퓨터에서 이미 실행 중인 Tasty 에 연결해, 그쪽 워크스페이스에서 하던 작업을 내 창에서 이어 간다. 서피스 하나 또는 워크스페이스 전체를 attach 할 수 있다. attach 한 대상은 내 쪽에 mirror 로 나타나고 입력은 원격 PTY 로 전달된다.
- attach 된 서피스는 한 번에 한 클라이언트만 점유한다. 두 번째 attach 는 거부되면서 현재 점유자를 알려 주고, 점유 중에는 원격 컴퓨터 쪽의 키보드와 에이전트 입력이 차단된다.
- 연결이 닫히거나 heartbeat 가 끊기면(5초마다 전송, 기본 30초 동안 없으면 끊긴 것으로 판단, 설정으로 변경 가능) 점유가 풀린다. 원격 컴퓨터 앞의 사용자는 언제든 강제로 detach 할 수 있다.
- 네트워크 프로토콜, 인증, 암호화를 따로 만들지 않았다. 서버는 loopback 에서만 listen 하고, 클라이언트는 시스템 `ssh` 로 연 터널을 통해 접속한다.
- 로컬 워크스페이스를 원격 프로필과 원격 워크스페이스에 연결해 두면, 그 워크스페이스를 활성화할 때 자동으로 attach 된다.

자세한 내용: [`docs/features/remote-attach/index.md`](docs/features/remote-attach/index.md), [`docs/features/remote-profiles/index.md`](docs/features/remote-profiles/index.md)

## 설치

자세한 절차: [`docs/installation.md`](docs/installation.md).

**[GitHub Releases](https://github.com/zilhak/tasty/releases/latest)** 에서 macOS(DMG) / Windows(MSI) / Linux(AppImage 등) 배포 산출물을 바로 받을 수 있다. 소스 최신 커밋이 항상 최신 릴리스보다 앞서 있을 수 있으므로, 가장 최신 기능이 필요하면 아래 소스 빌드를 사용한다.

```bash
# 소스 빌드 (모든 플랫폼 공통)
git clone https://github.com/zilhak/tasty.git
cd tasty
cargo build --release
./target/release/tasty
```

## 핵심 기능

- **여러 AI 에이전트의 작업을 한 터미널에서 조율한다** — task DAG와 barrier·semaphore·lease·reduce·rate-limit으로 병렬 작업을 관리([`docs/features/agent-collaboration/index.md`](docs/features/agent-collaboration/index.md))
- **GUI 없이도 완전히 동작한다** — CLI/IPC 만으로 surface 를 만들고 끄고 입출력까지 다룰 수 있어 CI/서버 환경에 그대로 올라간다(headless 빌드: `cargo build --no-default-features` — gui 빌드에 `--headless` 를 줘도 headless 가 되지 않는다, [`docs/features/headless-pty/index.md`](docs/features/headless-pty/index.md))
- **키보드만으로 화면을 선택·복사한다** — vi 스타일 카피 모드(hjkl 이동·visual 선택·검색)와 GPU 커서 시각화([`docs/features/clipboard/index.md`](docs/features/clipboard/index.md))
- **배포용 설치 파일을 한 번에 뽑는다** — `cargo build --profile dist` + Justfile 로 DMG / MSI / AppImage 를 자동 빌드
- **플러그인으로 기능을 직접 확장한다** — 매니페스트 스키마 + 권한 시스템을 갖춘 SDK 제공([`docs/features/plugin-system/index.md`](docs/features/plugin-system/index.md))
- **에이전트끼리 정보를 공유한다** — Blackboard / Plan / Cache 로 여러 에이전트가 같은 작업 컨텍스트를 주고받음([`docs/design/systems/memory.md`](docs/design/systems/memory.md))
- **셸 명령 단위로 출력을 정확히 짚어낸다** — shell prompt 경계를 인식해 "이 명령의 출력"만 골라 캡처([`docs/features/terminal-output/index.md`](docs/features/terminal-output/index.md))
- **터미널 출력을 실시간으로 감시해 후속 작업을 건다** — PTY 출력 줄을 분석해 메모리나 파일로 전달([`docs/features/terminal-output/index.md`](docs/features/terminal-output/index.md))
- **에이전트 토큰 사용량을 재고 한도에서 자동으로 막는다** — 측정/집계 + cost cap 초과 시 자동 차단([`docs/features/telemetry/index.md`](docs/features/telemetry/index.md))
- **테마를 내 취향대로 바꾼다** — 4px 그리드/14px 폰트 상한 기반 사용자 정의 테마 시스템([`docs/features/themes/index.md`](docs/features/themes/index.md))
- **여러 자식 Claude를 동시에 실행하고 상태가 바뀌면 알림을 받는다** — spawn/tell 은 즉시 반환하고, idle/추가입력필요/종료 시점마다 호출자에게 완료 알림이 자동으로 온다([`docs/plugins/claude/index.md`](docs/plugins/claude/index.md))

## 문서

- 인덱스: [`docs/index.md`](docs/index.md)
- 사용자 가이드(설치해서 쓰는 사람): [Tasty 가이드](https://tasty.zilhak.org/ko/guide/) — 설치부터 화면 구성·설정까지
- 에이전트 입문: [tasty CLI 로 터미널 조작하기](https://tasty.zilhak.org/ko/guide/agents/cli/) — 작업용 터미널을 만들고 명령을 보내 출력을 읽는 첫 실습. 나머지 명령은 [CLI 명령 참조](https://tasty.zilhak.org/ko/guide/agents/cli-reference/)
- 기능 명세: [`docs/installation.md`](docs/installation.md), [`docs/features/`](docs/features/index.md)
- API·스키마 참조: [`docs/reference/`](docs/reference/index.md) (api / event-catalog / output-parsers / environments / plan.schema.json)
- 개발 가이드: [`docs/dev-guide/`](docs/dev-guide/)
- 안정성 정책: [`docs/dev-guide/api-conventions.md`](docs/dev-guide/api-conventions.md) 의 "안정성 정책" 절

## 아키텍처

Hexagonal 아키텍처(model + ports + adapters + view + host_api 분리)의 63-crate workspace. 자세한 구조: [`docs/architecture/`](docs/architecture/).

## 라이선스

MIT — [`LICENSE`](LICENSE). 번들하는 제3자 자산과 그 고지: [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md).
