# CLI / IPC API 규약 — 명명 + 안정성

IPC 메서드와 CLI 명령의 이름, 호환성, 버전 정책을 설명한다. release 메서드 목록은
`crates/tasty-ipc/src/method_meta.rs::METHOD_TABLE`에서 관리한다. 전체 메서드 카탈로그는 [reference/api](../reference/api.md).

## 형식

IPC 메서드 이름과 CLI 명령 경로는 서로 다른 규칙을 따른다. 둘을 1:1로 옮기지 않는다.

### IPC 메서드 이름

```
<namespace>.<verb>[_<modifier>]
```

- **namespace 단수형** (`surface`, NOT `surfaces`). list 반환 키는 복수 (`surfaces: [...]`).
- **root 예외**: `split`(pane 분할) · `tree`(surface tree)만 namespace 없이 root 에 등록(자주 쓰는 짧은 명령). 새 메서드는 namespace를 생략하지 않는다.
- **보조 도메인은 3단** `<namespace>.<sub>.<verb>` (예: `remote.profile.*`, `surface.meta.*` 점 표기).

### CLI 명령 경로

기존 기본 조작 명령은 동작이 앞에 오는 경로를 유지한다. 예: `surface.list` ↔ `tasty list surfaces`,
`surface.close` ↔ `tasty close surface`, `surface.send` ↔ `tasty send text`. `tasty surface list`라는 명령은 없다.
`list`·`new`·`close`·`set`·`unset`·`move`·`send`·`read`는 이 형태의 명시적 예외다.

새 도메인은 대상이 앞에 오는 `tasty <namespace> <verb>` 경로로 만든다. 예: `claude.spawn` ↔ `tasty claude spawn`,
`pty.spawn` ↔ `tasty pty spawn`. 기존 경로를 이 규칙에 맞추려고 옮기지 않는다.

CLI 명령 이름은 동작 방식을 드러낸다. 즉시 현재 값을 돌려주는 조회는 `state`를, 조건이 될 때까지 응답을 미루는 대기는 `await`를 쓴다.
IPC 메서드 이름과 CLI 이름이 다를 수 있다. 예: `pty.wait`는 즉시 조회라 CLI 추천 표기가 `tasty pty state`이고
`tasty pty wait`는 호환 별칭이다. `agent.barrier_await`도 즉시 조회라 CLI 추천 표기는 `tasty agent barrier-state`다.
`agent.task_await`는 실제로 기다리며, 결과의 `outcome`(`terminal`·`timed_out`·`not_found`)이 모두 성공 응답이므로
CLI 종료 코드 0이 작업 성공을 뜻하지 않는다.

namespace 별 메서드 수는 `tests/cli_naming_count_drift.rs` 가 강제한다 — 추가는 같은 minor 내 OK(테이블 동기화 필요). 제거·이름 변경을 낼지는 버전 단계와 관계없이 개발자가 판단한다([안정성 정책](#안정성-정책)). 낼 때는 `(BREAK)` 로 적고 같은 커밋에서 이 기록과 0.7.0 기준 목록(`crates/tasty-ipc/fixtures/method_baseline_0_7.txt`, 절차는 그 파일 머리 주석)을 고친다. 메서드 수는 테스트의 snapshot에서 관리하며 이 문서에 중복 기록하지 않는다.

## verb 화이트리스트

새 메서드는 적합한 카테고리의 verb 를 고르고, 밖이면 PR description 에서 이유를 설명한다(별도 ADR 파일은 필요 없다).

| 카테고리 | verb |
|----------|------|
| **Read**(부작용 없음) | `list`(컬렉션→array) · `info`(단일, id 필요) · `state`(스냅샷) · `get` · `count` · `read` |
| **Write** | `create` · `update` · `set`/`unset` · `move` · `close`(소프트) · `clear` · `remove`(closed 안 남김) · `destroy`(영구, 예약) |
| **Send/외부** | `send` · `paste` · `wait` · `wake` |
| **프로세스/세션** | `spawn` · `launch` · `kill` · `respawn` · `shutdown` |
| **권한/관리**(local-only) | `install`/`enable`/`disable`/`grant`/`revoke`/`permissions` |

**modifier 패턴** `<verb>_<modifier>` 로 변종 표현(`send_key`/`send_combo`/`read_since_mark`). 한 verb 에 modifier 5개 이상 누적되면 namespace 한 단계 분리 검토.

도메인 특수 verb(예: `claude.tell`/`broadcast`, telemetry `record`/`summary`, agent `task_*`/`barrier_*`, memory `bb_*`/`plan_*`/`cache_*`/`goal_*`)는 표준 밖이지만 도메인 의미가 명확해 채택된 것들이다. 새 영역은 표준 verb 를 우선 검토하고, 채택 시 PR 에서 사유를 남긴다.

## 인자 규칙

- 대상 식별은 `--<대상 종류> <id>` (`--surface 42`, `--tab 7`, `--workspace 3`). **활성 객체 의존 금지**(포커스 독립성 — [focus 정책](../design/policies/focus.md)).
- 명령이 다루는 객체 자신의 ID 는 `--id` 를 쓴다(`tasty approval get --id req_…`). `close` 처럼 한 명령이 여러 대상 종류를 다루면 대상 종류 옵션을 쓴다(아래 표의 `close workspace --workspace`·`close window --window`).
- 옵션은 kebab-case (`--strip-ansi`, `--since-mark`).
- 예전 표기를 추천 표기로 바꿀 때는 예전 표기를 clap 별칭으로 남긴다. 두 표기를 함께 주면 같은 인자를 두 번 준 것으로 파싱 단계에서 거절한다. 별칭은 타입·필수성·생략 의미·IPC 키를 바꾸지 않고 새 이름 조회나 암묵 대상도 추가하지 않는다.

### 대상 옵션 표

| 명령 | 추천 표기(호환 표기) | 타입·필수성 | 의미와 생략 | IPC 키 |
|---|---|---|---|---|
| `close workspace` | `--workspace`(`--id`) | u32, 필수 | workspace ID | `workspace.close`의 `id` |
| `close window` | `--window`(`--id`) | u64, 필수 | window ID | `window.close`의 `id` |
| `agent` 하위 명령 전체 | `--workspace`(`--workspace-id`) | u32. 명령마다 필수 또는 선택 | workspace ID. `dag-list` 등 선택인 명령은 생략 시 모든 live workspace | 각 메서드의 `workspace_id` |
| `approval request`·`list`·`history`·`summary set`·`summary get` | `--workspace`(`--workspace-id`) | u32. `summary set`·`get`은 필수, 나머지는 선택 | workspace ID. `request`는 생략 시 `--surface`의 workspace, 그다음 활성 workspace. `list`·`history`는 생략 시 필터 없음 | 각 메서드의 `workspace_id` |
| `approval request` | `--surface`(`--surface-id`) | u32, 선택 | 요청을 묶을 surface ID | `approval.request`의 `surface_id` |
| `split` | `--surface`(`--target-surface`) | 문자열, 선택 | 숫자 surface ID, `this`, nickname. 해석은 기존 resolver·서버 계약 | `split`의 `target_surface` |
| `split` | `--pane`(`--target-pane`) | u32, 선택 | pane 분할의 대상 pane ID | `split`의 `target_pane` |
| `pty attach-surface` | `--pty`(`--pty-id`) | u32, 필수 | standalone PTY ID | `pty.attach_surface`의 `id` |
| `pty attach-surface` | `--pane`(`--pane-id`) | u32, 필수 | 새 탭을 만들 pane ID | `pty.attach_surface`의 `pane_id` |
| `terminal spawn` | `--workspace` | 문자열, 필수 | workspace ID 또는 이름 | `terminal.spawn`의 `workspace` |
| `terminal kill` | `--surface` / `--child` | u32 선택 / u32 필수 | 부모 surface ID / 자식 번호(child index). 부모는 기존 환경변수·단일 부모 처리를 따른다 | `terminal.kill`의 `surface` / `child` |

필수·선택은 CLI 파싱 단계 기준이다. 어느 대상이 실제로 필요한지는 서버가 다시 검증한다.
child index와 surface ID는 서로 바꿔 넣지 않는다. `this`·nickname은 원래 받던 명령에서만 받으며 숫자만
받는 인자로 넓히지 않는다. plugin 인자는 매니페스트에 별칭 필드가 없어 이 표의 대상이 아니다.
`crates/tasty-cli/tests/target_aliases.rs`가 두 표기의 요청 동일성, 중복 거절, 타입 범위를 확인한다.

### 잘못된 인자는 거절한다 — 자르지도, 버리지도 않는다

대상 인자가 없을 때와 잘못됐을 때를 구분한다. 잘못된 값을 None으로 바꾸면 기본 대상에 작업이 실행될 수 있다.

- 정수 변환은 범위를 확인한다. `4_294_967_297 as u32`는 `1`이 되므로 ID 변환에는 `u32::try_from`을 쓴다.
- 선택 인자도 값이 왔으면 검증한다. `null`은 생략으로 처리한다.
- 값이 잘못됐으면 해당 값과 이유를 알리고, 값이 없다는 `missing` 오류로 바꾸지 않는다.

공용 스칼라 검사는 `src/core/param_bag.rs`에 있다. IPC 핸들러는 이를 재수출하고 오류를 감싸는 `handler/params.rs`를 사용한다. 원격 구조 변경도 같은 함수를 써야 같은 입력이 같은 결과를 받는다.

`params_chokepoint` 검사는 IPC 핸들러와 App IPC 코드에서 이 공용 함수를 우회한 숫자 읽기를 찾는다. 다만 `params` 계열 이름에서 직접 읽거나 한 번의 let 바인딩을 거치는 형태만 확인한다. 이름을 바꾸거나 여러 단계로 전달하거나 검사 범위 밖에서 읽는 경우까지 증명하지는 않는다. 정확한 범위는 검사 모듈 주석을 따른다.

## CLI vs IPC

`crates/tasty-cli` 의 plugin CLI 빌더는 **top/sub 2단만** 지원 — plugin 이 `x.meta.set` 을 노출하려면 `tasty <plugin> meta-set` 같은 2단으로 매핑. 호스트 본체 CLI 는 3단 직접 빌드 가능.

`attach.*` IPC namespace 는 `tasty attach` 로 노출되지 않고 용도별 CLI 로 갈린다: `tasty remote attach`/`remote check`(release, 원격 SSH), `tasty debug attach`(debug 전용, 로컬 loopback). 근거·동작은 [attach-behavior](attach-behavior.md), 격리는 [debug-ipc](debug-ipc.md).

<a id="cli--ipc-표면--무엇이-cli-로-닿고-무엇이-왜-안-닿는가"></a>

### IPC 메서드를 CLI로 제공하는 기준

에이전트가 자기 작업에 사용하는 기능은 **IPC 와 CLI 양면**으로 제공한다(저장소 `CLAUDE.md` 의 핵심 개발 규칙). 이 문서는 그 대조를 **어떻게 판정하고 어떻게 세는지**를 적는다.
결정의 근거·대안·재검토 조건은
[CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md).

**어느 메서드가 CLI 없이 남아 있고 그 사유가 무엇인지의 정본은
이 절 아래의 두 표**다([release 절반](#release-ipc-에-있는데-cli-가-없는-메서드) · [debug 절반](#debug-표에-있는데-cli-가-없는-메서드)).
`tests/cli_method_table_parity.rs` 가 그 표와 실제 집합을 **양방향으로** 대조하므로,
진입점이 생기면 행을 지워야 하고 새 메서드를 CLI 없이 얹으면 행을 넣어야 한다. 목록을
여기 옮겨 적지 않는다 — 두 벌이 되는 순간 한쪽만 고쳐진다.

#### 판별식

> **호출자가 누구인지가 응답의 일부인가.**

응답이 호출자의 신원(자기 배너·자기 팝업·자기 plugin 설정)이나 호출자에게 push 되는
이벤트 수신처에 매여 있으면 셸은 호출자가 될 수 없다 — 셸에는 plugin 신원도 이벤트
수신처도 없다. 매여 있지 않으면(전역 스냅샷 조회든 id 로 대상을 지정하는 쓰기든)
진입점이 있어야 한다.

호출자가 주로 plugin이라는 이유만으로 CLI를 생략하지 않는다. `surface_id`를 받아
처리하고 결과를 응답으로 돌려주는 메서드라면 셸에서도 호출할 수 있는지 확인한다.

#### 어떻게 세는가

CLI 하위 명령을 실제 실행하고 프록시에서 전송한 메서드를 확인한다. 이름만 비교하면
`message.clear`를 보내는 `tasty read queue --clear`와 `surface.send_wait_idle`을 보내는
`tasty send text --wait-idle`처럼 옵션으로 선택하는 요청을 놓친다.

IPC를 보내지 않는 명령도 있다. `tasty tool remote-profile add-ssh`는
`crates/tasty-cli/src/local/`에서 직접 처리한다. 요청이 관측되지 않았다는 이유만으로
CLI 진입점이 없다고 판단하지 않는다. 인자를 맞추지 못해 실행하지 못한 명령도 미측정으로
남긴다. 이런 명령이 포함된 미지원 개수는 확정값이 아니라 상한이다.

가드가 소스에서 판정할 때 쓰는 "CLI 로 닿는다" 의 정의는 세 갈래다
(`cli_reachable_methods`): CLI 의 요청 조립 자리에 있는 인자 값의 문자열 리터럴, 크레이트 전체의
`method: "…"` 필드, 그리고 **번들 plugin 매니페스트의 `ipc_method`**. 마지막 것이 없으면
`tasty image open` 처럼 plugin 이 기여하는 명령이 전부 "진입점 없음" 으로 잘못 잡힌다.

#### 선행 작업이 필요해 미룬 것

- **`markdown.navigate` 의 CLI 진입점** — namespace 를 번들 plugin 이 점유해 외부 호출이
  plugin 으로 forward 된다([플러그인 제작 가이드](plugin-development.md)).
  host CLI 명령을 만들면 plugin 설치 여부에 따라 흔들리므로, 진입점은 plugin 의 매니페스트
  `ipc_method` 기여로 가야 한다 — plugin 크레이트 수정 + 매니페스트/Cargo 버전 bump.

#### 관련 문서

- [CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md) — 이 규칙의 결정
- [headless-ipc-surface](headless-ipc-surface.md) — 같은 표를 조합(gui/headless) 축으로 가른 대조
- [debug-ipc](debug-ipc.md) — debug 격리 정책과 CLI 의 debug 트리

### release IPC 에 있는데 CLI 가 없는 메서드

[위 기준](#ipc-메서드를-cli로-제공하는-기준)에 따라 에이전트가 작업에 쓰는 기능은 IPC와 CLI로 제공한다. 모든 release IPC 메서드에 CLI가 필요한 것은 아니다. 플러그인이 호스트에 자기 자원을 요청하는 메서드는 CLI 호출자를 위한 기능이 아니다.

그래서 "release 표에 있는데 CLI 가 없다" 는 그 자체로 결함이 아니다. 아래가 현재 그런 메서드 전부이고, 각 행이 왜 그 기준 밖인지 또는 어떻게 이미 충족되는지를 적는다. **새로 그런 메서드를 만들면 여기에 행을 추가한다** — `tests/cli_method_table_parity.rs` 가 이 표와 실제 집합을 양방향으로 대조하므로, 빠뜨리면 테스트가 떨어진다. 아래 개수와, 사유 열이 "대신 이걸 쓰라" 고 든 명령이 실재하는지도 같은 가드가 본다. 개수는 표에서 파생되지 않는 값이라(마크다운 표는 스스로 세지 않는다) 행을 고칠 때 함께 고쳐야 하고, 안 고치면 그 가드가 실제 값을 알려준다.

총 38개.

| 이유 | 메서드 | 왜 CLI 가 없나 |
|---|---|---|
| plugin → host 서비스 †plugin-only | `banner.open` · `banner.close` · `popup.close` | plugin 이 **자기** contribute UI 인스턴스를 여닫는다. 대상 식별이 caller plugin 자신이라 CLI 호출자가 존재하지 않는다 |
| plugin → host 서비스 | `agent.task_turn_report` | provider 플러그인(namespace 를 소유하고 `agent.turn_report` 를 가진 플러그인. 번들은 claude·codex)이 자기 훅에서 agent task 의 턴 시작·끝을 알린다. 핸들러가 호출자가 그 provider namespace 를 소유한 플러그인인지 확인하므로 CLI 호출자는 쓸 수 없다. 에이전트가 결과를 내는 경로는 `tasty agent task-submit` 이다 |
| plugin → host 서비스 | `file_picker.trigger` | plugin 프로세스가 못 여는 host 소유 popup 을 대신 연다. 결과는 응답이 아니라 `event.dispatch` 로 그 plugin 에 push 된다. 외부 arm 은 있지만 CLI·agent 호출은 popup 을 안 열고 `-32016` 을 받는다(아래 †plugin-only 절 끝, [파일 핸들러 (File handler)](../features/file-handler/index.md)) |
| plugin → host 서비스 | `git_viewer.query` · `markdown.navigate` | 특정 plugin(git-viewer · markdown 주소창·파일열기 팝업)이 자기 surface 를 위해 부른다. `git_viewer.query` 는 `request_id` 만 회신하고 결과를 그 plugin 에 unicast push 하므로 셸이 결과를 받을 수 없고, `markdown.navigate` 는 그 namespace 를 번들 plugin 이 점유해 외부 호출이 plugin 으로 forward 된다([플러그인 제작 가이드](plugin-development.md)) |
| plugin → host 서비스 | `attach.agent_call` · `attach.agent_commit` · `attach.child_notice` · `attach.child_notices` | claude·codex 같은 provider plugin 이 attach mirror 에 원격 자식을 만들고(`agent_call`·`agent_commit`), 서버 쪽에서 그 자식의 결과를 맡긴다(`child_notice`). 호출 plugin 자신의 namespace 와 부모 경로가 대상이라 CLI 호출자가 없고, 핸들러가 plugin 이 아닌 호출을 거절한다. 사용자 진입점은 `tasty claude spawn`·`tasty codex spawn` 이다. `child_notices` 는 서버에 남은 통지를 보는 진단 조회이며 `attach.list` 와 같은 진단 용도다([Attach 메커니즘](attach-behavior.md#원격-자식-에이전트-ipcstreamagent-children)) |
| plugin → host 서비스 | `markdown_mirror.content_request` | markdown plugin 이 attach mirror 문서의 원격 원문을 요청한다. `git_viewer.query` 와 같은 비동기 accept 라 `request_id` 만 회신하고 원문은 그 plugin 에 unicast push 되므로 셸이 결과를 받을 수 없다([원격 attach (Remote attach)](../features/remote-attach/index.md)) |
| 열면 그 능력이 깨진다 | `surface.read_since_scan_mark` | 출력 스캐너 전용 커서라 **읽으면 커서가 전진한다.** CLI로 읽으면 스캐너보다 먼저 커서가 전진해 감시할 출력을 놓칠 수 있다. 에이전트가 출력을 읽을 때는 커서를 움직이지 않는 `tasty read since-mark`를 쓴다([터미널 (Terminal)](../surfaces/terminal/index.md)) |
| plugin → host 서비스 | `settings.get_keybinding` | markdown plugin 이 문서 페이지에 넣을 `find` 바인딩을 읽는다. 페이지는 host 설정을 직접 받지 못해서 생긴 plugin용 조회이며, 바뀌면 `command.host_shortcut_changed` 를 그 plugin 이 구독해 받는다. 셸에서는 같은 값을 config.toml 의 `[keybindings]` 에서 읽는다 |
| plugin → host 서비스 | `settings.get_plugin_setting` | `caller_plugin_id` 를 요청 파라미터가 아니라 `CallerContext` 에서 강제 도출한다 — CLI 호출자는 plugin 신원이 없어 호출할 수 없다 |
| plugin → host 서비스 †plugin-only | `webview.open_external` | plugin 이 **자기** webview surface 안에서 클릭된 외부 링크를 host 의 OS 열기 자리로 넘긴다. 대상이 caller plugin 소유 surface 여야 하고, 사용자 브라우저를 여는 것은 에이전트가 자기 작업에 쓰는 능력이 아니다([플러그인 제작 가이드](plugin-development.md)) |
| plugin → host 서비스 †plugin-only | `webview.context_menu` | plugin 이 **자기** webview 페이지의 우클릭 위치에 host 네이티브 메뉴를 연다. 대상이 caller plugin 소유이며 화면에 드러난 webview 여야 한다. 사용자 우클릭을 잇는 자리라 에이전트가 자기 작업에 쓰는 능력이 아니다([컨텍스트 메뉴 구현](context-menu.md)) |
| plugin → host 서비스 †plugin-only | `host.shared_buffer.create` | 응답이 main 채널 하나로 끝나지 않는다 — 공유 메모리 핸들(Unix fd / Windows HANDLE)이 그 plugin 프로세스의 **보조 채널**로 함께 전달되고, 받는 쪽은 그것을 자기 주소공간에 매핑한다. CLI 프로세스에는 그 채널도 매핑 대상도 없어 결과를 받을 수 없다 |
| CLI 는 있고 IPC 를 안 탄다 | `remote.workspaces` | `tasty remote workspaces` 가 SSH 터널을 직접 열고 클라이언트 주도로 실행한다. 이 IPC 는 같은 작업을 원격 호출자나 에이전트가 요청할 때 사용한다 |
| CLI 는 있고 IPC 를 안 탄다 | `remote.profile.add` · `remote.profile.get` · `remote.profile.list` · `remote.profile.list_local` · `remote.profile.detect` · `remote.profile.import` · `remote.profile.remove` | `tasty tool remote-profile …` 이 로컬 프로필 파일을 직접 다룬다(IPC 없음). 인스턴스가 떠 있지 않아도 되어야 하는 명령이라 그쪽이 옳다 |
| CLI 는 있고 IPC 를 안 탄다 | `remote.passkey.add` · `remote.passkey.get` · `remote.passkey.list` · `remote.passkey.remove` | `tasty tool passkey …` 가 같은 이유로 로컬 처리한다 |
| 다른 이름으로 이미 있다 | `view.create` · `view.close` · `view.list` | `window.*` 의 어휘 통일 alias 로 동작이 동등하다. CLI 는 `tasty new window` · `tasty close window` · `tasty list windows` 쪽 한 벌만 노출한다 |
| 같은 능력을 다른 명령이 준다 | `surface.send_combo` | `surface.send_key` 가 `"ctrl+c"` 형태를 파싱하므로 `tasty send key ctrl+c` 로 덮인다. 이쪽은 modifier 를 배열로 받는 JSON 친화 변종이다 |
| 같은 능력을 다른 명령이 준다 | `surface.send_to` | `surface.send` 와 동형이라 `tasty send text --surface <id>` 로 덮인다 |
| 연결 경계가 대신한다 | `attach.acquire` · `attach.release` · `attach.list` | 위 "CLI vs IPC" 의 `attach.*` 항목 참조. `client_id`는 `stream.open` 핸드셰이크에서 발급하므로 단발성 CLI 호출로 점유를 유지할 수 없다. CLI에서는 `tasty remote attach` / `tasty tool attach`가 세션 전체를 처리한다 |

#### † plugin-only — 외부 호출자는 무엇을 받는가

위 표에서 †plugin-only 로 표시한 여섯(`banner.open` · `banner.close` · `popup.close` ·
`webview.open_external` · `webview.context_menu` · `host.shared_buffer.create`)은 **CLI 명령뿐 아니라 외부 dispatch arm도
없다.** plugin host-call 진입부가 직접 인터셉트하기 때문이다. 나머지 행들은 사정이 다르다 —
`git_viewer.query` · `markdown.navigate` · `settings.get_plugin_setting` 같은 것은 외부에서
호출하면 실제로 라우팅되어 인자 오류나 plugin 의 답이 돌아온다. 두 부류가 같은 표에 있는 것은 이
표가 **CLI 진입점 유무**를 분류하기 때문이다.

이 다섯은 `METHOD_TABLE` 에 `plugin_only(&[…])` 로 등재되고, 외부 호출자는 `-32601`("그런
메서드 없다")이 아니라 다음을 받는다:

    -32016  method '<name>' is plugin-only: only the plugin host-call path dispatches it,
            so CLI and network IPC callers have no entry point

`plugin_only_dispatch_parity`가 메타데이터 표식과 plugin 진입부 처리를 양방향으로 대조한다. 새 메서드는 표식과 실제 caller 제한을 함께 확인한다.

**`-32016` 을 표식 없이 내는 메서드가 하나 있다 — `file_picker.trigger`.** 이 메서드는 외부
arm(gui 창 라우터)이 있어 `plugin_only` 표식을 달지 않는다. 그러나 핸들러가 첫 판정으로
`CallerContext::Plugin` 이 아닌 호출자를 거부한다:

    -32016  method 'file_picker.trigger' answers only a plugin caller: the picked path is
            pushed to the calling plugin, so a CLI or agent caller would only take the
            user's input focus

고른 경로는 호출한 plugin 에게만 push 되므로 CLI·agent 호출에는 받을 곳이 없고, popup 은 사용자
입력 포커스를 가져간다([사용자 통제와 데이터 원칙](../principles/control-and-data.md), [포커스 정책](../design/policies/focus.md)). 코드가 같은 것은 뜻이 같아서다 — 부를 수 있는 주체가
다르다. 근거 [파일 핸들러 (File handler)](../features/file-handler/index.md).

### 등재된 이름인데 이 바이너리에 arm 이 없을 때

응답은 이름 오류와 실행 조건을 구분한다.

| 조건 | 코드 | 확인할 것 |
|---|---|---|
| 등록되지 않은 이름 | `-32601` | 오타와 메서드 이름 |
| 플랫폼에서 지원하지 않음 | `-32015` | OS와 GUI 지원 조건 |
| plugin만 호출할 수 있음 | `-32016` | 호출자 종류 |
| 등록됐지만 현재 바이너리에 구현이 없음 | `-32017` | GUI·헤드리스·release 조합 |
| 설치된 소유 plugin이 실행 중이지 않음 | `-32002` | plugin의 enable·실행 상태 |

등록 여부는 `is_registered_name`으로 정확한 이름을 조회한다. namespace fallback까지 처리하는 `method_meta()`로 판정하면 plugin 고유 메서드나 오타까지 등록된 호스트 이름으로 잘못 취급한다. 플랫폼 전용 dispatch에는 반대 조건에서도 지원 불가 사유를 돌려주는 분기를 둔다. namespace 소유는 설치된 매니페스트에서 확인하며 실행 중인지와 구분한다.

근거: IPC 지원 조건과 오류.

### 도메인 오류 코드 — `-32003..-32059`

이름·지원 조건 계열(`-32002`·`-32015..-32017`)과 전송 대역(`-32060..-32069`) 밖의 코드는 도메인이 정한다. 표는 지금 응답에 쓰이는 코드와 그 뜻이다.

| 코드 | 쓰는 곳 | 뜻 |
|---|---|---|
| `-32003` | plugin.* | 대상 plugin이 설치돼 있지 않다 |
| `-32003` | plugin namespace 로 넘기는 호출 | plugin에 요청을 보내지 못했다 |
| `-32004` | agent.*, memory.* 등 | 대상이 없다(not found) |
| `-32005` | memory.* | 버전 비교 쓰기 충돌(cas_conflict) |
| `-32006` | memory.* | 다른 소유자의 값이다(owned_by_other) |
| `-32007` | memory.* | 용량·크기 상한 초과(quota_exceeded·value_too_large) |
| `-32007` | 권한 상승 게이트(전 메서드) | 정책이 막았다(cap_blocked) |
| `-32008` | agent.* | task 가 이미 끝났다(already terminal) |
| `-32009` | agent.* | lease 충돌 |
| `-32009` | memory.* | 이미 있다(already_exists) |
| `-32010` | agent.* | task 를 다른 task 가 참조한다(`data.referenced_by`) |
| `-32010` | rate limit 게이트(전 메서드) | 호출자의 `ipc_calls` 버킷이 비었다 |
| `-32011` | agent.* | task 가 실행 중이라 삭제할 수 없다 |
| `-32011` | approval.* | 요청자가 스스로 응답했다(self_response_forbidden) |
| `-32012` | agent.* | lease pool 소진 |
| `-32012` | approval.* | 응답 대기 시간 초과(timed_out) |
| `-32013` | approval.* | 요청이 취소됐다(cancelled) |
| `-32014` | approval.* | 승인 저장소 락이 poison 됐다(store_poisoned) |
| `-32018` | agent.* | 회차에 묶인 결과 쓰기(`task_set_result` 완료 보고·`task_submit_result` 제출)를 거절했다. `data` 에 `reason`·`attempt_id`·`current_attempt_id` |
| `-32019` | approval.* | host 가 발행한 권한 상승 요청에 사용자가 아닌 caller 가 응답·취소했다(user_response_required) |
| `-32020` | attach.*, terminal 점유 | 다른 소유자가 이미 점유했다 |
| `-32021` | attach.release | 해제하지 못했다(점유자가 아니다 등) |
| `-32040` | passkey.*, remote.profile.* | 이름으로 찾지 못했다 |
| `-32041` | remote.profile.* | 이름이 이미 쓰인다 |
| `-32022` | surface.html_script_allow · surface.html_remote_allow | html 문서 스크립트·원격 콘텐츠 허용을 거절했다. `data` 에 `reason`(`document_changed`·`loading`·`no_document`·`no_fingerprint`, 원격 허용은 `unsupported_view`·`globally_allowed` 도)·`surface_id`·`document` |
| `-32050` | remote 조회 | 원격 호출이 실패했다 |

- 새 도메인 오류를 만들 때는 이 표에서 뜻이 같은 코드를 쓰고, 같은 뜻이 없으면 표에 없는 코드를 새로 잡는다. 이미 다른 뜻으로 쓰이는 코드를 다시 쓰지 않는다.
- 표에서 같은 코드가 두 줄인 것(`-32003`·`-32007`·`-32009`·`-32010`·`-32011`·`-32012`)은 이 규칙 전에 생긴 겹침이다. 호출자는 코드와 함께 메서드 namespace 와 메시지로 구별한다. 겹침을 없애는 것은 배포된 응답을 바꾸므로 따로 정한다.
- 코드를 추가·변경하면 이 표를 같은 커밋에서 고친다.

### 전송 계층이 직접 내는 코드 — `-32060..-32069`

이 코드들은 수신·대기·멱등 처리에서 생긴다. 범위만 보고 실행 여부를 판단하지 말고 각 코드의 의미를 따른다.

| 사실 | 코드 | 호출자가 다음에 할 일 |
|------|------|----------------------|
| 요청 한 줄이 서버의 줄 상한을 넘었다 | `-32060` | 요청을 나눠 보낸다 (연결은 닫힌다) |
| 호출자가 실은 응답 대기 상한이 만료됐다 — 요청은 **이미 시작됐다** | `-32061` | **상태를 먼저 읽는다** (연결은 유지된다) |
| 서버가 동시 연결 상한에 닿아 안 받았다 | `-32062` | 그대로 다시 건다 (아무것도 실행 안 됐다) |
| 같은 멱등 키에 **다른 요청**이 붙었다 | `-32063` | 키 재사용을 고친다 (아무것도 실행 안 됐다) |
| 그 멱등 키의 요청은 **실행됐고** 답이 안 남았다 | `-32064` | **재전송하지 말고 상태를 읽는다** |
| 호스트의 명령 큐가 밀려 요청을 큐에 넣지 않았다 | `-32065` | 잠시 뒤 그대로 다시 건다 (아무것도 실행 안 됐다 · 연결은 유지된다) |
| 연결 뒤 첫 요청 줄이 기한 안에 안 왔다 | `-32066` | 연결 직후 바로 보낸다 (아무것도 실행 안 됐다 · 연결은 닫힌다) |
| 호출자가 실은 응답 대기 상한이 요청이 **큐에서 기다리는 동안** 지났다 | `-32067` | 그대로 다시 건다 (아무것도 실행 안 됐다 · 연결은 유지된다) |

`-32061`은 이미 시작된 작업의 결과를 모른다는 뜻이며 취소를 뜻하지 않는다. 이 구분은 서버의 `ipc.response-timeout.not-run` capability로 확인한다. 구 서버는 시작 전 만료도 결과 불명으로 답할 수 있다.

`-32060`·`-32066`은 완전한 요청을 읽지 못했으므로 응답 ID가 `null`이다. 연결 포화 응답 `-32062`는 accept를 막지 않는 최선 노력 전송이어서 EOF로 보일 수 있다. stream client는 이 JSON을 프레임 오류로 해석할 수도 있다. 큐 포화 `-32065`는 정상 요청을 지금 수용하지 못한 것이며 연결은 유지한다.

수신·쓰기 상한, 첫 줄 기한과 내부 주입 동작은 [IPC 서버](../architecture/ipc-server.md)에 정리한다.

#### CLI 응답 대기 옵션

CLI의 --response-timeout-ms는 서브커맨드 앞에 쓰는 루트 옵션이며 단발 RPC에만 적용한다. 0은 요청에 포함하지 않는다. 상대가 capability를 지원하지 않으면 sent:false 구조화 오류로 실행 전 거절한다. loop·stream·로컬 처리·SSH 조회·plugin 자동 대기 및 명령 없는 기동은 양의 값을 받으면 사용 오류 exit 2로 거절한다. 메서드 내부의 --timeout-ms와는 별개의 시간이다. 환경변수로 상속하거나 지원하지 않는 명령에서 조용히 무시하지 않는다.

CLI capability 확인과 본 요청은 하나의 응답 대기 예산을 나눠 쓴다. 확인에는 남은 시간의 socket read timeout과 올림한 요청 제한시간(ms)를 함께 적용해 구 서버에서도 끝난다. 본 요청은 남은 ms를 내림하고 1ms 미만이면 보내지 않는다. 확인 만료는 본 요청 미실행이므로 -32067이며, 확인 timeout 뒤 연결은 늦은 응답이 남을 수 있어 재사용하지 않는다. 1ms 옵션에서는 확인은 보내도 본 요청은 나가지 않는다. 연결·쓰기 시간까지 포함한 전체 CLI 실행 시간 보장은 아니다.

### 변경 명령의 재시도는 키로 구별한다

메서드 표는 두 번 전달됐을 때 결과를 기준으로 Read·Idempotent·Mutate를 필수 선언한다. message.read는 기본 소비 동작, screenshot은 파일 생성, set_mark는 시각·출력 위치 변화가 있어 이름만으로 읽기나 멱등으로 분류할 수 없다. 알 수 없는 plugin namespace는 재전달에 안전하다고 가정하지 않는다. 상태를 바꾸는 구현을 수정하면 같은 인자로 두 번 실행해 분류가 맞는지도 확인한다.

요청의 `idempotency_key`는 UTF-8 1~256바이트이며 요청 ID와 별개다.
저장 키는 Local·plugin ID·agent ID로 구분한 주체와 키의 조합이다.
연결이 바뀌어도 같은 주체의 재시도를 찾는다.
같은 키·같은 요청은 저장 응답을 idempotent_replay:true로 반환하고, 다른 요청이면 -32063으로 실행을 막는다.
권한·cap·rate 거절은 저장하지 않는다.
메모리 보존 경로의 시간·개수·개별 응답 크기·키 길이는 system.info에서 상수로부터 선언하며 그 경로는 재시작을 넘는 보장이 없다. 구조 저널로 연결된 이름은 별도 structure_journal 선언을 따른다.
항목 퇴출 후에는 다시 실행될 수 있지만 큰 응답만 버릴 때는 키를 남겨 -32064로 실행 완료·응답 없음 상태를 알린다.
응답 표지가 없는 것만으로 계약 밖이라고 판단하지 않는다.
client는 부수효과 전에 capability를 확인한다.
요청 비교용 digest는 전체 params 저장 비용을 줄이는 대신 충돌 가능성을 수용한다.

키 길이 검사는 공통 요청 게이트(`handler/gate.rs`)와 창 없는 Local check_without_engine에서 수행한다. 권한·cap·rate 검사와 허용된 요청의 사용량 집계 뒤, CheckedRequest를 만들기 전에 검사하여 기존 거절 순서를 유지한다. 목적지가 engine·App·plugin인지와 관계없이 잘못된 키는 -32602로 거절한다. 저장소 begin에서 같은 검사를 중복하지 않는다.

#### 어느 경로에 걸리나 — 호스트가 아는 이름은 전부 안, plugin 고유 이름만 밖

메서드별 KeyContract는 Kept{since}·Unneeded·Outside로 선언한다. Mutate 여부와 저장 보장 여부를 구분한다. Read·Idempotent의 기본값은 Unneeded이며, 구조 저널에 연결된 Idempotent는 최초 대상과 응답을 보존하기 위해 Kept를 선언한다. client는 Kept의 since 이상 capability를 요구하고 Unneeded도 최소 지원 버전을 확인한다. 서버 capability는 표가 요구하는 최대 버전에서 파생한다. 멱등 키 기능 버전 1은 engine, 버전 2는 App 경로의 보장을 뜻한다. 새 라우팅 계층을 열 때 실제 저장소 경로와 선언을 양방향으로 검증한다.

멱등 키 기능 버전 3은 호스트가 아는 Mutate 이름을 GUI debug와 namespace forward에서도 보호한다.
image·markdown처럼 plugin으로 전달되는 호스트 이름은 forward_keeping_the_key가 Kept를 확인하고 공용 relay를 사용한다.
plugin 고유 이름까지 같은 Mutate라는 이유로 저장하면 Outside 계약이 깨지므로 Kept 판정을 생략하지 않는다.
GUI debug 두 단계도 공용 저장 경로로 묶는다.
release 빌드의 같은 묶음 함수(`ipc_step_debug_layers`)는 보존소를 부르지 않는다. release의 창 필요 단계(`ipc_step_window_required`)는 아무 요청도 처리하지 않고, debug의 창 필요 이름은 release 메서드 표에 없어 `-32601`로 끝난다. 요청은 모두 routing으로 넘어가며 키는 거기서 `forward_keeping_the_key`(namespace 전달)와 `route_checked_request`의 `idempotency::begin`·`finish`(호스트 라우팅)가 보존한다. 전역 `notification.read`는 engine 핸들러를 거치지 않아 GUI routing에서 같은 `begin`·`finish`로 원응답을 보관하며 App 층 계약을 선언한다. `key_contract_by_layer`는 release 창 필요 단계가 요청을 처리하지 않는지와 release 묶음이 debug 단계를 부르지 않는지를 텍스트로 확인한다. release에서 창 필요 단계가 요청을 처리하게 되면 release 묶음도 보존소 뒤로 옮긴다.
원 요청 대신 키를 뗀 relay 인자를 전달하고 forward 완료 전 재시도도 합류시킨다.
host injector·plugin host-call·구조 stream op에는 현재 호출자 멱등 키 자체가 없다. plugin host-call의 공개 구조 요청도 plugin 주체로 같은 journal admission을 거치지만, 현재 plugin 프로토콜에서 키를 전달한다는 보장은 아니다.
plugin host-call은 IpcCall을 수집할 때 원 process binding을 고정한다. reload 뒤 새 process에 이전 call_id를 다시 결합하지 않으며 admission에서 현재 binding을 대조한다. 지연 구조·입력·namespace 결과와 오류도 원 binding에만 회신한다.
실제 GUI dispatch·plugin 왕복 및 루프 밖 조기 호출까지 검증됐다고 보장하지 않는다.
텍스트 가드가 확인하는 호출 모양을 벗어난 우회는 별도 행동 검증 대상이다.

호스트 표가 모르는 plugin 고유 이름은 Outside다. send_idempotent는 이런 이름을 연결에 쓰기 전에 KeyOutsideContract로 거절한다. 구 client가 직접 키를 실어 보내면 plugin 고유 호출의 중복 실행을 호스트가 막아주지는 않는다. 플러그인이 정확히 한 번의 실행을 요구하면 자체 요청 ID 계약이 필요하다. 낡은 client가 새 호스트 이름을 모를 때도 안전하게 거절하므로 이 경우 client 업데이트가 필요하다.

버전 4의 `structure_journal.methods`는 코드의 `kept_in_structure_journal` 선언에서 만든다. 카테고리·workspace metadata/순서 외에 workspace/tab/split 생성·닫기, tab 이동, preset 적용, terminal 자원 respawn, standalone 채택 및 child 생성·닫기를 포함한다. GUI·headless 요청에 키가 있으면 현재 권한 검사 뒤 원본 key를 조회하고, 처음 실행할 때만 ID·index를 해소한다. plugin host-call도 같은 admission을 거치지만 현재 caller key는 없다. 같은 key의 진행 중 요청은 ID 예약 전에도 합류한다. 최종 JSON-RPC 본문은 확정 기록으로 보존하며 재시도에서는 요청 id만 바꾼다. 대상이 이동·삭제되거나 worker가 재시작해도 최초 결과를 돌려준다. 도메인 거절도 Failed 응답으로 남고, admission 크기·큐 거절은 해당 요청만 종료한다. `terminal.respawn`은 cwd를 지정한 자원 교체만 같은 구조 admission을 사용하며, cwd 없는 interrupt·입력 분기는 기존 live/메모리 key 계약이다. 이 메서드 전체의 key 광고를 더 강한 구조 계약으로 올리지는 않는다. 나머지 이름은 선언된 기존 메모리/Outside 계약을 따른다.

mirror metadata와 혼합 목록의 mirror 위치는 비영속 표시 상태다. 로컬 구조 commit·projection 뒤 최초 완료에서만 표시 continuation을 적용하고 응답을 보낸다. delta·재연결로 같은 ID의 mirror 구조가 교체되면 이전 continuation을 버린다. 저장 응답 재시도는 과거 표시 변경이나 host 알림을 다시 내지 않는다. 이 process-local token은 원격 전송 큐의 connection epoch 보장을 대신하지 않는다.

#### 진행 중 요청과 보장 한계

App·forward의 지연 응답은 relay가 저장을 완료한 뒤 원 호출자와 합류자에게 전달한다.
진행 중인 같은 요청은 첫 실행에 합류하고 각자 ID로 replay 응답을 받는다.
다르면 충돌이다.
처리하지 않은 층은 항목과 실행 집계를 취소하고 다음 층으로 넘긴다.
응답 없이 채널이 닫히면 항목을 잊으며 늦은 완료가 새 항목을 덮지 않도록 ticket을 확인한다.
relay 생성 실패 시에는 warn을 남기고 키 없이 실행하는 현재 예외가 있어 중복 방지가 보장되지 않는다.
relay는 완료까지 살아 있으므로 저장 항목 수가 곧 스레드 수 상한은 아니다.
스레드 누적·재시작 보존 요구가 생기면 설계를 다시 검토한다.

새 경로를 추가하면 키 저장, 진행 중 합류, 호출자 범위, 실제 capability 버전을 함께 확인한다. 단위 테스트가 공용 함수를 검사하는 것과 실제 GUI·plugin 경로가 그 함수를 호출하는 것은 다른 검증이다. 설계 이유는 위 멱등 재시도 계약을 따른다.

### plugin 을 거쳐 온 실패도 호스트가 준 코드를 그대로 낸다

호스트 오류는 plugin을 왕복해도 error_code를 유지한다. ipc.result의 선택 필드는 구 SDK·구 호스트와 호환되며 코드가 없는 옛 응답만 -32000으로 해석한다. 호스트가 생성한 취소·만료·권한 오류와 post-hook을 거친 target plugin의 임의 코드도 전달한다. 기존 오류 표시 문구는 유지하고 문자열에 코드를 끼워 넣지 않는다. plugin 내부 버그의 인자가 외부 호출자가 고칠 수 없는 실패를 만드는 사례가 늘면 오류 책임을 구분하는 계약을 검토한다.

### debug 표에 있는데 CLI 가 없는 메서드

IPC·CLI 양면 기준은 debug 빌드에서 에이전트가 사용하는 기능에도 적용한다.
`debug.*`는 release에 포함하지 않지만, debug 빌드에서는 CLI 제공 여부를 확인해야 한다.
아래 표는 `DEBUG_METHODS`에 있으면서 `tasty debug …`로 호출할 수 없는 메서드다.
위 release 메서드 목록과는 빌드 조건이 달라 따로 정리한다.

debug 표 기준 총 3개.

| 이유 | debug 메서드 | 왜 CLI 가 없나 |
|---|---|---|
| 사용자 행동 | `system.shutdown` | 호스트 종료는 사용자가 직접 하는 동작이다. debug IPC 에 local 전용으로만 있고 CLI 진입점은 두지 않는다 |
| 사용자 행동 | `window.focus` · `view.focus` | 포커스 전환은 사용자의 단축키/마우스 영역이다([포커스 정책](../design/policies/focus.md)). debug IPC 에 재현 수단이 있는 것과, 그것을 CLI 한 줄로 상시 노출하는 것은 다르다 |

## 응답 계약 — mirror 워크스페이스로 간 구조 op

대상이 **mirror(원격 attach client) 워크스페이스**인 구조 op(`tab.create`/`split`/`tab.close`/`tab.move`/`pane.close`/`surface.close`/convert 등)는 로컬에서 실행되지 않고 원격으로 forward 된다([remote-attach](../features/remote-attach/index.md#mirror-워크스페이스-내-구조-변경)). 그 응답은 **fire-and-forget success** 다:

```json
{ "forwarded": true, "workspace_index": 2 }
```

즉 **생성된 id(surface/tab/pane)를 담지 않는다.** 원격 실행은 비동기라 응답 시점에 아직 아무것도 만들어지지 않았기 때문이다. 결과는 나중에 `StructuralDelta` 역반영으로 mirror 트리에 반영된다.

따라서 **구조 op 의 응답에서 생성된 id 를 동기로 꺼내 쓰는 호출자를 새로 만들지 않는다.** 그런 호출자는 mirror 워크스페이스에서 조용히 깨지고(응답에 필드가 없다), 게다가 forward 큐는 IPC 응답과 무관하게 드레인되므로 **로컬은 실패인데 원격에는 리소스가 남는** 고아를 만든다. 그 id 가 반드시 필요한 method 는 mirror 워크스페이스를 대상으로 **거부**해야 한다 — 실제 선례가 `terminal.spawn` 이며, 그 결정과 배경은 [Attach 메커니즘](attach-behavior.md).

## 권한 표 등재 (라우터 ↔ METHOD_TABLE)

**IPC 라우터에 dispatch 분기가 있는 메서드는 예외 없이 권한 표에 등재한다** — plugin 에 열 것이면 `plugin(&[..])`, local caller 전용으로 둘 것이면 `local_only()`. 표는 `METHOD_TABLE`(+ debug 빌드 전용 `DEBUG_METHODS`, prefix fallback `PREFIX_RULES`, `crates/tasty-ipc/src/method_meta.rs`).

미등재는 "닫혀 있음"으로 대충 넘어가지 않는다. `method_meta()` 가 `None` 이면 plugin/agent 호출자는 `UnknownMethod` 로 거부되긴 하지만, 그 거부가 **정책인지 등재 누락인지 표만 봐서는 구분되지 않는다** — 나중에 권한을 재검토하는 쪽이 "닫으려던 것"과 "잊은 것"을 판별할 수 없다. `local_only()` 등재는 그 판단을 코드에 남기는 선언이다(거부 자체는 `NotPluginCallable` 로 바뀔 뿐 동작은 같다).

`tests/ipc_router_table_parity.rs` 가 라우터 소스를 훑어 강제한다.
`"<method>" =>` 팔과 `… .method == "…"` 비교(`||` 로 이어진 다중 비교 포함, `src/app/ipc/app_methods.rs`·`window_required.rs` 가 그 형태다)를 **둘 다** 잡는다.
검사는 고정 목록(`ROUTER_SOURCES`)과 `src/app/ipc/` 디렉터리 전체(`ROUTER_DIRS`)를
읽는다. 그 디렉터리 밖에 새 라우터를 만들면 `ROUTER_SOURCES`에 추가한다.
검사는 debug 빌드에서 실행한다. release에서는 `DEBUG_METHODS`가 비어 있으므로
debug 메서드까지 대조할 수 없다([debug-ipc](debug-ipc.md)).


## plugin 점유 namespace

plugin 이 매니페스트로 contribute 하는 IPC namespace 는 호스트 예약어와 충돌 금지(`system surface tab pane workspace plugin hook global_hook webhook message tool notification window debug ui ime split tree memory output approval telemetry timer run` 등). 상세는 [plugin-development](plugin-development.md) "예약 prefix".

### 대상 surface 는 `surface` / `surface_id` 어느 이름으로 와도 같은 필드다

CLI 인자는 `--surface`(매니페스트의 `surface`)이고 호스트 IPC 의 표준 키는 `surface_id` 다. 그래서 CLI dynamic runner 는 **두 키를 모두 채워** 보낸다. agent plugin(`claude`/`codex`)의 핸들러는 그 두 이름을 **한 필드로** 읽고, 둘이 다른 값이면 고르지 않고 `-32602` 로 거절한다 — 어느 쪽을 골라도 절반의 호출자에게는 지목하지 않은 대상이 된다. 판정은 `tasty-plugin-agent-common` 에 한 벌만 있다.

아무 이름도 안 오면 아무것도 호스트로 넘기지 않는다. 그때 호스트는 **부모가 하나뿐이면 그것**으로 푸는데(`--surface` 생략의 정의), 그 폴백은 *이름을 안 준 호출* 을 위한 것이지 *이름을 줬는데 못 읽은 호출* 을 위한 것이 아니다. 대상을 읽고도 안 실어 보내면 실재하지 않는 id 를 지목한 호출이 남의 자식에 성공한다 — 호스트의 "named target is never resolved by focus" 가드가 그 자리를 지키는데, 이름이 어긋나면 그 가드에 애초에 닿지 않는다.

`claude.hook` 만 그 위에 폴백이 하나 더 있다 — 아무 이름도 안 오면 `TASTY_SURFACE_ID` 를 읽는다(훅 명령은 Claude Code 프로세스 안에서 돌고, 설치 문자열이 `--surface` 를 안 실었으면 그 env 가 유일한 지목 수단이다). 이 폴백도 **아무 이름도 안 온 경우에만** 탄다: 이름이 왔는데 못 읽으면 거절한다 — 폴백은 호출자 **자신**이라, 넘기면 잘못 지목한 훅이 자기에게 배달되고 종료코드는 0 이다. 어느 키가 틀렸는지도 위와 같은 한 벌이 댄다.

### auto_wait chain

일부 plugin 명령은 1차 IPC 응답 직후 wait IPC 를 자동 chain 해 대상이 terminal state(`idle`/`needs_input`/`exited`)에 도달할 때까지 block 할 수 있다.
child terminal 의 파생 상태 `stale`([child-terminal (호스트 내재화 자식 터미널 관리 · `tasty terminal`)](../features/child-terminal/index.md))은 **기본 terminal state 집합에 넣지 않는다** — 무출력 임계값 기반 판정은 휴리스틱이라 오탐 시 아직 일하는 자식을 종결 처리하게 된다.
다만 hook 유실로 영구 대기하는 것보다 조기 탈출이 나은 소비자는 `terminal_states` 에 직접 `"stale"` 을 추가해 선택할 수 있다.
매니페스트 `[[contributes.cli.subcommand]].auto_wait` 한 필드로 선언적으로 켠다(plugin 핸들러 미수정, CLI dynamic runner 가 chain). `map_from_response`(1차 응답→wait params, 우선) + `map_from_request`(요청→fallback) + `polling`(state_field/terminal_states/interval). `polling` 과 `auto_wait` 동시 선언은 validator 가 reject(직교 — 전자는 *이 명령 자체가 wait*, 후자는 *응답 직후 다른 method chain*). `surface`↔`surface_id` 키는 자동 alias.

`claude spawn`/`tell`과 `codex spawn`/`tell`은 auto_wait 대신 완료 알림 훅을 쓴다.
Claude는 `claude-idle`/`needs-input`/`process-exit` 훅에서 `claude.notify_done`을 호출한다
(`crates/tasty-plugin-claude/src/notifications.rs`의 `register_notify_hooks`).
Codex는 대응 훅에서 `codex notify-caller`를 호출한다([Codex](../plugins/codex/index.md)).
두 핸들러는 알림 뒤 `surface.locate`로 대상 생존을 확인하고, 살아 있으면 형제 훅을
다시 등록한다. 따라서 spawn/tell마다 한 번만 알리는 것이 아니라 자식이 종료할 때까지
상태가 바뀔 때마다 알린다.

현재 번들 plugin은 auto_wait/polling을 사용하지 않는다. 스키마는 외부 plugin을 위해 유지한다.

### 응답 출력 선언

매니페스트 서브커맨드의 `output = "child_table"`은 호스트 CLI가 그 응답을 JSON 대신 [부모 ID 표](cli-structure.md#부모-id-표-formatparent_tablers)로 찍게 한다. 응답은 `terminal.children` 항목의 배열이거나 `{"children": […]}`이다. 같은 그룹에 문자열 인자 `format`을 두면 `--format json`으로 JSON을 받는다. `table`과 `json` 밖의 값은 요청을 보내기 전에 거절한다. `polling`이나 `auto_wait`와 함께 선언할 수 없다. 번들 plugin에서는 `claude children`과 `codex children`이 쓴다.

---

## 안정성 정책

### 버전과 호환성 판단

[플랫폼과 호환성 원칙](../principles/platform-and-compatibility.md)에 따라 호환성 유지와 버전 부여는 개발자 재량이다. 0.x에만 유예 없는 변경을 허용하거나 안정선 이후 SemVer·deprecation 기간을 제품 의무로 두지 않는다.

메서드·필드 제거, 타입·단위·기본값 변경, 새 필수 조건은 기존 호출을 깨뜨릴 수 있다. 변경 영향은 검토하고 릴리스의 깨지는 변경은 CHANGELOG에 (BREAK)로 알린다. 변경 영향의 분류와 패키지 버전 선택은 구분한다.

### 호환 협상 — 무엇을 할 줄 아는지 묻는 자리

변경 영향 검토는 **무엇이 깨지는가**를 다루고, capability 는 **지금 이 서버가 무엇을 할 줄
아는가**를 답한다. 둘은 다른 물음이다 — 패키지 버전은 기능 목록이 아니다(같은 버전의 두
빌드가 feature 조합에 따라 다른 것을 한다).

- 선언 자리는 `system.info` 응답의 `capabilities` 키이고, 모양은 `{name, version}` 배열이다.
  목록은 `crates/tasty-ipc` 의 `capability::CAPABILITIES` 하나다.
- **이름을 더하는 것은 기능 추가**다. 패키지 버전 선택은 개발자 재량이다. 구 client 는 모르는 키·모르는 이름을 무시한다.
- **구현된 기능만 선언한다.** 선언이 "곧 할 것" 을 담으면 그 목록으로 분기한 client 가 깨진다.
- 기능 버전은 가능하면 **구현 상수에서 가져온다** — `ipc.stream`의 버전은 리터럴이 아니라 서버가
  handshake 에서 비교하는 `stream::STREAM_PROTO` 다.
- 뜻이 바뀌면 배열에서 빼지 말고 그 이름의 `version` 을 올린다.
- **메서드 인자도 이름이 필요하다.** 봉투 필드와 같은 이유로 — 인자 객체에도 모르는 키 거절이
  없어 구 서버는 새 인자를 조용히 버리고 성공으로 답한다. `surface.read_since_mark` 의 위치 인자가
  `ipc.output-cursor` 를 받은 것이 그 형태이고, 기능 이름·버전·인자 이름을 한 모듈(`tasty-ipc` 의
  `output_cursor`)에 둬 서버 파서·선언·CLI 가 같은 값을 쓴다
  ([터미널 출력 구조화 (Terminal output)](../features/terminal-output/index.md)).
- **CLI 는 요청이 요구하는 이름을 보내기 전에 묻는다**(`tasty-cli` 의 `contract` 모듈). 요구
  여부는 명령이 아니라 요청에서 판정하고, 새 계약을 안 쓰는 요청은 묻지 않는다. 없으면 요청을
  내보내지 않고 stderr 에 `{"error":{"kind":"unsupported_capability","capability":…,"required":…,"found":…,"sent":false,"message":…}}`
  한 줄을 쓴 뒤 종료 코드 1 로 끝난다. `found` 가 `null` 이면 이름이 없는 것이고, 수면 다른 버전으로
  선언된 것이다.
- ★ **스트림에 기능을 더할 때 `ipc.stream` 버전 대신 별도 capability를 추가한다.**
  바로 위 줄이 말하듯 그 버전은 `STREAM_PROTO` 이고, 서버는 그것을 handshake 에서 **동등
  비교**해 다르면 연결을 거절한다(`validate_stream_proto`). 그래서 그 값을 올리면 **구 peer의 attach 연결이 거절된다**. 버전은 프레임의
  *기존* 뜻이 바뀔 때만 움직이고, 더해지는 기능은 `ipc.stream.<기능>` 처럼 이름으로
  선언한다. 그 이름을 본 client 만 그 기능을 쓰고, 못 본 client 는 종전 동작을 받는다.
  본보기와 결정 근거는 [Attach 메커니즘](attach-behavior.md).

메서드 **이름**이 구 서버에 있는지는 별도 물음이고 표가 답한다 —
`method_meta::method_since` 가 0.7.0 동결 파일을 읽어 두 값(`FrozenBaseline` /
`AfterFrozenBaseline`)으로 답하고, 미등재 이름에는 `None` 을 준다. 그 값은 손으로 적지
않는다(동결 파일에서 대상 목록을 관리한다). 근거는
이 문서.

### Deprecation 절차

버전 간 호환성 유예 기간은 제품 의무가 아니다. 개발자 판단으로 break를 낼 수 있다
([플랫폼과 버전 호환성](../principles/platform-and-compatibility.md)).
이전 이름을 잠시 남기고 싶을 때만 아래처럼 표기한다.

1. 기존 API를 유지하면서 새 API를 추가한다.
2. 기존 API를 호출하면 `tracing::warn!("deprecated: <old>, use <new>")`(`crates/tasty-ipc/src/alias.rs`).
3. `CHANGELOG.md` `Deprecated` 절에 기록한다.
4. 제거할 때는 `Removed` 절에 적고, 깨지는 변경이면 `(BREAK)` 를 붙인다.

별칭이나 유예 기간을 둘지는 변경별로 개발자가 정하며, 특정 버전부터 반드시 도입하는 의무는 없다.

### plugin-protocol schema

`api_version` 메이저를 올리는 변경: 메시지 필드 의미 변경 · 메서드 제거(alias 없이) · 응답 형식 의미 변경 · handshake/auth 계약 변경. 추가만(새 메시지, optional+default 필드)은 같은 `api_version` 내 `crates/tasty-plugin-protocol/Cargo.toml` minor bump. 이력은 `crates/tasty-plugin-protocol/CHANGELOG.md`.

### 자동화 보조

`tests/changelog_unreleased.rs`(CHANGELOG `[Unreleased]` 절 존재 검증) + `cli_naming_count_drift.rs`(메서드 카운트 drift) + `ipc_router_table_parity.rs`(라우터 팔 ↔ 권한 표 등재 대조, 위 "권한 표 등재"). PR 템플릿·`git-cliff` 초안·경로 기반 규칙은 점진 도입 대상.

## 관련

- [reference/api](../reference/api.md) — 전체 IPC/CLI 메서드 카탈로그
- [plugin-development](plugin-development.md) · [plugin-packaging 생태계 정책](plugin-packaging.md#생태계-정책--자동-upgrade--호환성-분류) · [release](release.md)

### child 상태 전달의 채널

완료 알림 채널은 `<parent_home>/notify/<caller_surface>.log` 하나이며, notify 형제 hook의
surface 생존 판정·재무장이 그 경로를 채운다. `auto_wait`의 작업 성공 판정과는 별개다.
[완료 알림 로그](external-interaction.md#child-완료-알림--completion-log)를 따른다.

구조 명령이 실행 owner를 잃고 결과를 확정하지 못하면 같은 key 조회는 `-32068`(recovery required)을 반환한다. 이 응답은 원 명령의 durable InProgress를 실패·성공으로 바꾸지 않으며 외부 동작을 다시 보내지 않는다. 이후 정확한 owner 영수증으로 대조가 완료되면 같은 key로 확정 결과를 읽는다. 단순 응답 대기 만료(`-32061`)와 구분한다.
