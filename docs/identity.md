# Tasty 정체성과 불가침 원칙

> Tasty 가 *무엇이며 왜 그렇게 만들어졌는지*, 그리고 그 정체성에서 필연적으로 나오는 **개발 시 절대 어기면 안 되는 원칙** 을 정의한다. 모든 문서·API·기능 설계는 이 문서 위에 얹힌다. **작업 전 가장 먼저 읽는다.**

## 1. Tasty 는 무엇인가

### 제작자 입맛에 맞춘 터미널

Tasty 는 "제작자가 자기 업무 환경에 딱 맞는 터미널이 없어서, 입맛에 맞게 직접 만든 터미널" 이다. 이름 그대로(*tasty* = 입맛에 맞는) **범용 합의가 아니라 제작자의 워크플로가 기본값의 기준** 이다. 기본 기능과 기본 플러그인은 제작자가 평소 쓰던 것들을 모은 것이다.

동시에 Tasty 는 **AI Agent 와 개인화 환경에 맞게 커스터마이징할 수 있도록 열려 있다** — 테마·단축키·플러그인으로 각자의 환경에 맞춘다. (이 커스터마이징을 떠받치는 코드 규칙 — 색·길이·단축키·문자열 하드코딩 금지 — 은 정체성이 아니라 *구현 정책* 이므로 `CLAUDE.md` 코드 정책 / dev-guide 에 둔다.)

### 동시성이 핵심 (가장 중요한 정체성)

Tasty 의 가장 중요한 요소는 **동시성** 이다. **기반 가정: 같은 tasty 를 세 종류의 주체가 동시에 사용한다 — ① 로컬 사용자, ② 여러 AI Agent, ③ (SSH 로 attach 한) 원격 접속 사용자.**

- **여러 AI Agent 가 여러 터미널에서 동시에 동작** 하고, 그것을 **오케스트레이션** 할 수 있다.
- **로컬 사용자는 독립적으로 자기 작업** 을 한다.
- **원격 접속 사용자** 는 SSH 너머에서 surface/workspace 를 **점유(attach)** 한 뒤 *그 점유 안에서만* 작업한다 — 점유 중 그 대상은 로컬 사용자·AI Agent 에게 readonly 가 되고, 로컬 사용자만 점유를 끊을 수 있다 (메커니즘: [`dev-guide/attach-behavior.md`](dev-guide/attach-behavior.md)).
- 즉 Tasty 는 *다중 에이전트 + 로컬/원격 사용자* 의 동시 작업에 초점을 맞춘 터미널이다.

이 동시성이 아래 모든 불가침 원칙의 뿌리다 — 한 주체가 다른 주체를 침범하면 동시성이 깨진다.

> **주체 분류**: ① 로컬 사용자 = *사용자 행동*(직접 입력, 포커스 주인), ② AI Agent = *에이전트 행동*(IPC/CLI). ③ 원격 접속 사용자는 사람이지만 **연결(attach)+점유 기반** 이라 분류상 에이전트에 가깝다 (직접 GUI 입력 아님, 로컬 포커스 비주인, 점유 필수). 각 주체의 canonical 정의는 [`concepts/actors.md`](concepts/actors.md).

### 크로스 플랫폼

Windows · macOS · Linux 모두 1급 지원.

## 2. 정체성에서 나오는 불가침 원칙

> 이 원칙이 깨지는 순간 Tasty 는 더 이상 "동시성에 초점을 맞춘, 신뢰할 수 있는 다중 에이전트 터미널" 이 아니게 된다.

### 2.1 사용자 행동 ↔ 에이전트 행동 분리 (soul)

동시성의 토대. 사용자와 에이전트가 같은 환경을 공유해도 서로를 침범하지 않아야 동시 작업이 성립한다.

- **사용자 행동** = 로컬 사용자의 직접 입력 (키보드/마우스/OS). **에이전트 행동** = IPC/CLI 호출 (에이전트가 자기 작업 수행). (원격 접속 사용자는 사람이지만 연결(attach)+점유 기반이라 분류상 에이전트 쪽 — [주체](concepts/actors.md).)
- **① 에이전트 행동의 부수효과가 사용자 상태에 닿지 않는다** — 포커스 / 닫은 항목 히스토리(Ctrl+Shift+T) / 선택·스크롤·커서. 에이전트가 surface 100개를 열었다 닫아도 사용자의 복원 스택은 *사용자가 닫은 것만* 복원한다.
- **② 사용자 입력 재현은 release 에 없다** — 키/마우스 주입, popup 강제 open/close, 메뉴 강제 invoke, 프로그래밍적 포커스 전환은 release IPC/CLI 표면에 존재하지 않는다. debug 빌드(`#[cfg(debug_assertions)]`)에서만 격리 제공한다 (격리의 판정은 *"이 코드를 통째로 지우고 컴파일 에러 몇 줄만 정리하면 디버그 기능이 깨끗이 사라지는가"* 이고, 그 집행은 debug 핸들러를 **모듈 선언에 cfg 가 붙은 별도 파일**로 모으는 것이다 — 상세 [`dev-guide/debug-ipc.md`](dev-guide/debug-ipc.md)).
- **②의 목적 — debug 에서는 사용자 전용 동작도 IPC 로 구동해 자기검증한다.** ②의 재현 기능을 *debug 에 제공하는 이유* 는 agent 가 자기가 만든 기능을 스스로 검증하기 위해서다. debug 빌드에서는 사용자에게만 허용되는 동작(키/마우스 주입 `debug.inject_key`/`inject_mouse`, popup 강제 open/close `debug.popup.*`, 도구 메뉴 클릭 `debug.tool.invoke` 등)을 **IPC 로 구동** 할 수 있어, 사용자 입력 흐름까지 포함한 기능을 release(= 사용자 환경)와 격리된 채 검증한다. → dev-guide [독립 검증](dev-guide/self-verification.md#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다).
- **판단 기준**: *에이전트가 자기 작업을 하기 위해 필요한가(→ release) vs 사용자가 직접 하는 조작을 재현하는가(→ debug)*.

### 2.2 AI 에이전트 조작 가능성

2.1 의 positive 면. 에이전트가 자기 작업에 필요한 기능은 **반드시 제공한다.**

- 에이전트 기능(surface/tab/workspace 생성·닫기·조회, 클립보드, 알림, 파일 열기, 메타데이터 등)은 **IPC + CLI 양면** 으로 동작해야 한다. GUI 전용 에이전트 기능 금지.
- 에이전트가 문제를 직접 확인·조작할 기능이 부족하면, 그건 *Tasty 가 에이전트가 자유롭게 조작할 수 있는 터미널이 아니라는 의미* 이므로 기능을 추가한다.
- **headless 동작-우선**: 기능의 진실은 내부 동작이고 화면은 그 투영이다 → [`documentation-model.md`](documentation-model.md).
- **headless 환경을 의식한다**: headless 인스턴스에는 로컬 사용자(GUI)가 없다 — AI Agent(IPC/CLI)와 원격 접속 사용자(attach)만 쓴다. 새 기능을 넣을 때 *로컬 GUI 사용자가 늘 존재한다* 고 가정하지 않는다.

### 2.3 포커스 독립성 — 포커스는 사용자의 것

tasty 의 기반 가정(로컬에서 한 명의 사용자 + 여러 AI Agent 동시 작업)에서, **포커스(활성 윈도우/탭/워크스페이스/Pane)는 "사용자의 것"** 이다 — 사용자가 지금 무엇을 보고 어디에 입력하는지의 시점. 로컬 사용자가 작업 중인데 agent 나 다른 IPC 동작이 포커스를 바꿔버리면 사용 경험이 망가진다. 그래서:

- agent/IPC 행동은 **포커스를 바꾸지 않는다.** release 빌드엔 포커스를 바꾸는 API 가 없다.
- 모든 명령은 대상을 **ID 로 직접 지정** 한다 (포커스에 의존하지 않음). list 는 **전 워크스페이스 순회**.
- 활성 상태 *조회* 는 허용(`focused` 필드 등), 활성 상태에 *의존* 하는 동작은 금지.
- 원격 접속 사용자는 대상을 **점유(attach)** 할 뿐 로컬 사용자의 포커스/시점을 옮기지 않는다 (점유는 로컬 사용자가 끊을 수 있다).
- 상세 [`design/policies/focus.md`](design/policies/focus.md).

### 2.4 크로스 플랫폼

- 모든 기능은 Windows · macOS · Linux 에서 동작해야 한다. 한 OS 전용 기능도 **다른 OS 의 컴파일을 깨지 않는다.** (구현상 `#[cfg(...)]` 분기 규칙은 CLAUDE.md 코드 정책 / dev-guide.)

## 관련

- [`documentation-model.md`](documentation-model.md) — 이 정체성(특히 headless 동작-우선)에서 도출된 문서 구조
- [`adr/0006-docs-taxonomy-behavior-first.md`](adr/0006-docs-taxonomy-behavior-first.md) — 문서 분류체계 결정
- 용어: [`concepts/actors.md`](concepts/actors.md)(주체) · [`concepts/hierarchy.md`](concepts/hierarchy.md)(구조 계층) · [`concepts/plugins.md`](concepts/plugins.md). 통합 용어집 [`concepts/ubiquitous-language.md`](concepts/ubiquitous-language.md).
- [`design/policies/focus.md`](design/policies/focus.md) — 포커스 독립성 운영 상세
