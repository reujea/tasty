# child-terminal (호스트 내재화 자식 터미널 관리 · `tasty terminal`)

- **Status**: Implemented
- **주체**: AI Agent
- **ADR**: [ADR-0040](../../adr/0040-occupancy-soft-hard-tiers-agent-occupant.md) (soft 점유 소비자)
- **코드**: `src/core/child_terminal.rs` (registry) · `src/adapters/ipc/handler/terminal.rs` (IPC) · `crates/tasty-cli/src/commands/terminal.rs` (CLI)
- **화면**: 없음 — headless 전용 (자식은 일반 terminal surface 로 렌더; soft 점유 테두리는 [surface-highlight](../surface-highlight/index.md)/점유 계약)

## 목적

에이전트가 자식 터미널 surface 를 spawn/tell/kill 하는 **범용 기계**를 호스트 1급으로 제공한다. 같은 기계가 codex/claude 플러그인에 각각 중복돼 있었는데, 특정 에이전트 바이너리에 묶이지 않는 부분(자식 registry·spawn 조합·self-heal)을 호스트로 끌어올려 단일 SoT 로 수렴한다 (CLAUDE.md 원칙 2: 에이전트 기능은 IPC+CLI 양면·호스트 1급).

## 내부 동작 (headless-valid)

- **registry**(`ChildTerminalRegistry`): `parent_surface → 자식 목록`, `child_surface → parent`, parent별 다음 index, child별 idle/needs_input 상태, 그리고 그 상태를 **마지막으로 보고받은 시각**(`last_state_report_at`, unix epoch ms)을 보관한다. 앞의 두 bool 맵이 "무엇을 보고받았나"라면 마지막 값은 "언제 보고받았나"로 별개 축이며, hook push(`terminal.set_state`)마다 갱신되고 `register_child` 가 등록 시각으로 시딩한다 — 파생 상태 판정의 **hook 침묵 축**이다(아래 "상태 판정"). epoch 기반이라 호스트 재시작을 건너 살아남는다. `~/.tasty/child-terminals.json` 에 영속(등록/제거마다 즉시 save; 이 필드 도입 이전에 영속된 파일은 `serde(default)` 로 빈 맵이 된다). session token 추적(`session.rs`)·agent shell 서브프로세스 추적(`runner_host` `shell_children`)과는 **다른 서브시스템**이다.
- **needs_input 이 화면에 표시된다**: `terminal.set_state --state needs_input`(에이전트 hook
  진입점) 자체는 이 registry 만 갱신하고 화면에 아무 효과도 없다 — registry 는 에이전트의
  완료 판정 입력이지 사용자 UI 상태가 아니다(불가침 원칙 1). 화면 표시는 별개 채널인
  [surface-highlight](../surface-highlight/index.md) 의 `AttentionKind::NeedsInput` 이
  담당하며, Claude 플러그인은 `terminal.set_state` 와 `surface.completion { kind:
  "needs_input" }` 를 **같은 훅 이벤트에서 둘 다** 호출해 두 SoT 를 함께 갱신한다
  (`crates/tasty-plugin-claude/src/hook.rs::apply_hook`). 포커스로 해제되는 것은
  `AttentionStore` 레코드뿐 — 사용자가 탭을 쳐다본 것만으로 `tasty terminal state` 결과
  (이 registry 조회)가 바뀌지는 않는다.
- **spawn**: 대상 workspace 의 pane 에 `terminal` 탭을 만들고(호출자 지정 `--command` 를 그대로 전송) child 를 registry 에 등록한 뒤, 그 child 를 **soft 점유**로 표시한다(주체 = spawn 을 발동한 parent surface). command 는 임의 문자열 — 에이전트 특화 command 빌더는 플러그인에 잔류. **`command` 는 optional**: 생략하면 tab 생성·registry 등록·soft 점유·`child_surface_id` 반환만 하고 아무것도 전송하지 않는다 — codex/claude 플러그인이 이 **2단계 spawn** 을 소비한다(먼저 command 없이 호출해 host registry 에 등록하고 받은 surface_id 를 박은 에이전트 특화 command 를 `surface.send` 로 별도 전송; surface_id inline env·session token 등이 필요하기 때문).
- **soft 점유 연결**: spawn 성공 시 `occupy_soft(child, parent)`, kill 시 `release_occupancy(child)`, release 시 `release_soft_occupancy(child, parent)` 를 **in-process core 함수**로 호출한다 (`occupancy.*` IPC method 는 없다 — ADR-0040 경계). soft 점유는 표시만 하고 입력을 차단하지 않으며(`attached=false`), parent 가 죽으면 focus 지연 청소로 풀린다.
- **self-heal**: 호스트가 라이브 surface 트리를 직접 소유하므로, 접근 시점마다 라이브 집합과 대조해(reconcile) 죽은 자식을 registry 에서 정리한다(이벤트 구독 없이 동기). 부팅 후 첫 접근이 이전 세션 잔재를 회수한다.
- **adopt**: `terminal.spawn`(새 탭 생성 시에만 자동)과 달리, 이미 존재하는 임의의 surface(과거에 child였든 아니든)를 지금 시점에 명시적으로 등록한다 — PTY 생성 없이 `handle_spawn`의 관계등록+점유 블록과 동일한 시퀀스를 수행한다. 검증 순서: 대상 존재 → 자기입양 거부(`parent == target`) → 중복 등록 거부(이미 다른 parent 의 child) → hard 점유(원격 attach) 거부 → `occupy_soft` 시도. `occupy_soft` 가 실패하면(다른 parent 가 이미 soft 점유 중) registry 는 전혀 건드리지 않고 즉시 에러를 반환한다(`register_child`+`occupy_soft` 순서가 spawn 과 반대 — "children 목록 = 점유 목록" 동치성 보존).
- **release**: adopt 의 대칭 — child 관계와 soft 점유만 해제하고 surface(탭)는 닫지 않는다. `handle_kill`과 동일하게 `remove_child`+`save`를 수행하지만, 점유 해제에 tier-무관 `release_occupancy` 대신 주체 검증판 `release_soft_occupancy(child, parent)`를 쓴다 — hard 점유(원격 attach)는 `self.soft` 맵을 보지 않으므로 구조적으로 손대지 않는다. `release_soft_occupancy`가 desync(예: 점유만 먼저 풀린 상태)로 실패해도 `tracing::warn!`만 남기고 registry 관계 제거는 그대로 성공 처리한다. `surface.close` 호출이 없는 것이 kill 과의 유일한 차이.
- **관계 존재를 소비하는 플러그인 신호**: claude 플러그인의 PTY 에러 스캐너는 자식 surface 의 추적 여부를 `terminal.parent` 조회(관계 존재)로 판정한다 — surface 존재(`surface.locate`)로 판정하면 위 **release** 가 surface 를 남기므로 영원히 정리되지 않는다. 즉 `terminal.release` 는 그 child 에 대한 `claude-error` 발화를 끊는 경계이기도 하다([claude plugin](../../plugins/claude/index.md) "PTY 에러 스캔 범위").

## 상태 판정 (hook + 관측 융합 — [ADR-0072](../../adr/0072-child-state-hook-observation-fusion.md))

`terminal.children` / `terminal.state` 가 보고하는 `state` 는 registry 의 hook push
캐시를 그대로 되읽은 값이 **아니다**. hook 축과 호스트 관측 축을 합성한 파생 값이며,
두 IPC 경로가 같은 헬퍼(`CoreState::child_liveness{,_with_live}` —
`src/core/state/child_liveness.rs`)를 공유한다.

**왜 필요한가**: registry 의 `state_of` 는 `idle`/`needs_input` 두 bool 이 모두 false 면
`"active"` 를 반환하는데, 그 의미는 "작업 중" 이 아니라 **"idle 이라는 증거가 없음"**
이다. 상태를 바꾸는 유일한 경로가 hook push 단방향이라, hook 이 유실되거나 자식이
멈추면 마지막으로 찍힌 `active` 가 영구히 남고 되돌리는 경로가 없다.

### 판정 우선순위

위에서 아래로 먼저 맞는 규칙이 이긴다.

| # | 조건 | `state` | `confidence` | `evidence` |
|---|---|---|---|---|
| 1 | surface 가 라이브 트리에 없음 | `exited` | `confirmed` | `surface_gone` |
| 2 | hook 이 `needs_input` 보고 | `needs_input` | `reported` | `hook_needs_input` |
| 3 | hook 이 `idle` 보고 | `idle` | `reported` | `hook_idle` |
| 4 | PTY busy | `active` | `confirmed` | `pty_busy` |
| 5 | PTY 미기동(deferred) | `active` | `unobserved` | `pty_not_started` |
| 6 | 전경 프로그램이 셸로 복귀 | `stale` | `confirmed` | `foreground_is_shell` |
| 7 | 무출력 경과시간 관측 불가(mirror 등) | `active` | `unobserved` | `observation_unavailable` |
| 8 | 무출력 < 임계값 | `active` | `heuristic` | `recent_output` |
| 9 | 무출력 ≥ 임계값 && hook 침묵 < 임계값 | `active` | `heuristic` | `recent_hook_report` |
| 10 | 무출력 ≥ 임계값 && hook 침묵 ≥ 임계값 | `stale` | `heuristic` | `output_and_hook_silent` |

- **2·3 이 관측보다 위**인 것은 의도다 — hook 은 거짓 `idle` 을 만들지 않으므로 관측이
  이 둘을 덮어쓰지 않는다.
- **5 가 6~10 보다 위**인 것도 의도다 — deferred terminal 은 출력을 낸 적이 자체가
  없어, 게이트하지 않으면 spawn 직후 전부 `stale` 로 오판정된다.
- 임계값: 무출력 `CHILD_OUTPUT_SILENCE` = 120s, hook 침묵 `CHILD_HOOK_SILENCE` = 300s.
  `BUSY_OUTPUT_WINDOW`(2s)를 그대로 쓸 수 없다 — 그 창은 "지금 화면이 갱신되는 중인가"
  용도라 사람이 프롬프트를 읽는 몇 초만으로도 넘어간다.
- hook 침묵 기준점이 없으면(이 기능 도입 전에 영속된 항목) 침묵으로 간주한다 —
  무출력 축이 이미 임계값을 넘긴 상태라 두 축 모두 반증이 없다.

### 미등록 surface

`terminal.state` 는 registry 에 없는 surface 도 조회를 거부하지 않는다(`state_of` 의
미등록 fallback 계약 유지). 다만 응답은 registry 원값이 아니라 파생 판정이므로,
**PTY 가 떠 있고 셸 프롬프트에 머무는** 임의의 live surface 는 `active` 가 아니라
`stale`(`foreground_is_shell`)로 나온다 — "이 surface 에서 도는 프로그램이 없다" 는
관측 사실 그대로다. PTY 미기동(deferred) surface 는 게이트에 걸려 `active`
(`pty_not_started`)로 남는다.

### `stale` 의 의미와 한계

`stale` 은 **`exited` 가 아니다.** surface 는 살아 있고, "이 surface 에서 에이전트
프로세스가 돌고 있지 않거나, 돌고 있다는 증거가 없다" 는 뜻이다. `terminal.adopt` 로
들어온 자식은 애초에 에이전트가 아닌 일반 셸일 수 있으므로 종료로 단정해선 안 된다.

무출력 기반 정지 판정은 **원리적으로 휴리스틱**이다 — SIGSTOP 으로 멈춘 프로세스,
긴 추론 중인 에이전트, 출력이 없는 긴 명령은 관측상 구별되지 않는다. 확정으로 취급
가능한 관측은 **surface 부재**와 **전경 셸 복귀** 두 가지뿐이며, 나머지는
`confidence: heuristic` 으로 표시된다. 소비자는 confidence 를 보고 확정 판정만 종결로
다룰 수 있다.

### 출력 전용

`stale`/`exited` 는 호스트가 관측으로만 만들어내는 값이다. `terminal.set_state` 는
여전히 `idle`/`needs_input`/`active` 세 값만 받는다 — hook 이 파생 상태를 registry 에
밀어넣을 수 있으면 관측 축이 다시 push 캐시로 퇴화한다.

### 조회만이 소비처가 아니다 — push 축

판정된 값은 `terminal.children`/`terminal.state` 응답으로만 나가는 것이 아니다.
claude plugin 의 정지 감시(`crates/tasty-plugin-claude/src/error_scan.rs`)가 자식 출력이
문턱 이상 멎은 것을 보면 이 판정을 조회해 보고, 그 값이 **`active` 또는 `stale`** 이면
부모의 완료 알림 로그에 한 줄을 싣는다(`claude-error-stalled` hook →
`tasty claude notify-error` →
[external-interaction 완료 알림](../../dev-guide/external-interaction.md#child-완료-알림--completion-log)).
`stale` 이 그 대상인 이유는 **그 값에 대응하는 완료 알림 경로가 없기 때문**이다 — `stale` 이
나온다는 것 자체가 훅이 유실됐다는 뜻이라, 부모가 묻지 않으면 그 사실이 아무 데도 도달하지
않는다. 결정과 오탐 대가는
[ADR-0266](../../adr/0266-derived-stale-must-reach-the-push-channel.md).

이 축은 위 "출력 전용" 과 어긋나지 않는다 — 알림은 판정을 **읽어서** 나가는 것이고,
`terminal.set_state` 를 부르지 않으므로 파생 상태가 registry 로 되밀려 들어가지 않는다.
조회 축(`spawn_census` 의 respawn 후보 집계)은 지금대로 **확정 `stale` 만** 센다: 두 축이
묻는 것이 다르다 — 조회는 "이 자식을 재사용해도 되나", push 는 "부모가 더 기다려도
소용없나" 다.

### 능동 프로빙 배제

대상 surface 에 입력을 주입해 반응을 보는 능동 프로빙은 사용자 입력 재현이라 release
금지 대상이고([`docs/identity.md`](../../identity.md) 원칙 1), 자식 에이전트의 프롬프트
상태도 오염시킨다. 판정은 **수동 관측만** 쓴다.

### 비용

추가 프로세스 스냅샷은 없다. 전경 프로그램 이름은 1Hz 일괄 폴링이 이미 채우는
`foreground_names` 캐시에서만 읽는다 — 자식마다 `Terminal::foreground_process_info()`
를 개별 호출하면 O(surfaces × processes) 를 되살리는 회귀다
(`src/core/state/busy.rs` 폴링 주석).

## 인터페이스

- **AI Agent (IPC/CLI)** — 모든 대상은 ID 로 직접 지정(포커스 독립, 원칙 3):
  - `tasty terminal spawn --workspace <ws> --command "<cmd>" [--surface <parent>] [--pane] [--cwd] [--role] [--nickname]` ↔ `terminal.spawn`
  - `tasty terminal tell "<text>" [--surface]` ↔ `terminal.tell`
  - `tasty terminal children [--surface]` ↔ `terminal.children`
  - `tasty terminal parent --surface <child>` ↔ `terminal.parent`
  - `tasty terminal state --surface <child>` ↔ `terminal.state` — 자식 단건 상태(`idle`/`needs_input`/`active`/`stale`/`exited`) 조회. `terminal.children` 의 항목별 `state` 와 **같은 판정 헬퍼**(`CoreState::child_liveness`)를 쓰므로 목록과 단건의 답이 갈리지 않는다. 이미 registry 에서 정리된(reconcile 로 사라진) surface 도 라이브 트리와 직접 대조해 `"exited"` 로 판별한다 — `ChildTerminalRegistry::state_of` 자체의 미등록 surface `"active"` fallback 계약은 그대로 둔 채, 상위 판정 계층이 그 위에서 죽은 surface 를 걸러낸다
  - `tasty terminal kill [--surface] --child <n>` ↔ `terminal.kill`
  - `tasty terminal respawn [--surface] --child <n> [--cwd] [--command] [--role] [--nickname]` ↔ `terminal.respawn`
  - `tasty terminal broadcast "<text>" [--surface] [--role]` ↔ `terminal.broadcast`
  - `tasty terminal set-state --surface <child> --state <idle|needs_input|active>` ↔ `terminal.set_state` (에이전트 hook 진입점). **파생 상태(`stale`/`exited`)는 입력으로 받지 않는다** — 출력 전용이다(아래 "상태 판정")
  - `tasty terminal adopt --target <surface> [--surface <parent>] [--cwd] [--role] [--nickname]` ↔ `terminal.adopt` — 새 탭을 만들지 않고, 이미 존재하는 임의의 surface 를 지금 시점에 명시적으로 child 로 등록(soft 점유)한다
  - `tasty terminal release [--surface <parent>] --child <n>` ↔ `terminal.release` — child 관계와 soft 점유만 해제한다. surface(탭) 자체는 닫지 않는다(`terminal.kill`과 달리)

### 판정 응답 필드

`terminal.children` 의 각 항목과 `terminal.state` 단건 응답은 판정 3 축을 **모두**
싣는다. 두 경로가 같은 직렬화 지점(`liveness_fields`,
`src/adapters/ipc/handler/terminal.rs`)을 거치므로 키 집합과 값이 구조적으로 일치한다.

| 필드 | 값 |
|---|---|
| `state` | `exited` \| `needs_input` \| `idle` \| `active` \| `stale` |
| `evidence` | `surface_gone` \| `hook_needs_input` \| `hook_idle` \| `pty_busy` \| `pty_not_started` \| `foreground_is_shell` \| `observation_unavailable` \| `recent_output` \| `recent_hook_report` \| `output_and_hook_silent` |
| `confidence` | `confirmed` \| `reported` \| `heuristic` \| `unobserved` |

나오는 조합은 위 "판정 우선순위" 표의 행 그대로다 — 임의 조합은 생기지 않는다.

`state` 하나만 보면 **같은 값의 근거가 갈리는 것을 구분할 수 없다.** 예를 들어
`active` 는 "PTY 가 지금 출력 중"(`pty_busy`/`confirmed`)일 수도 있고 "판정할 관측
축이 없어서 그대로 둔 것"(`observation_unavailable`/`unobserved`)일 수도 있다.
소비자는 `confidence` 를 보고 **확정 판정만 종결로 다룰 수 있다** — `heuristic` 인
`stale` 은 SIGSTOP·긴 추론·무출력 명령과 구별되지 않으므로(위 "`stale` 의 의미와
한계") 그것만으로 자식을 종결 처리하면 일하는 자식을 죽인다.

원시 관측값(`busy`, 무출력 경과시간 등)은 싣지 않는다 — `evidence` 가 "어느 축이
판정을 결정했나" 를 이미 알려주므로 목적이 달성되고, 원시값을 계약으로 굳히면
임계값 조정이 소비자 계약 변경이 된다.

**소비자 정합**: claude plugin 의 `claude.children` remap 은 화이트리스트라 세 필드를
명시적으로 옮긴다(`crates/tasty-plugin-claude/src/handlers.rs`). codex plugin 의
`codex.children` 은 passthrough 라 자동 반영된다.

### `--child <n>` 은 index 지 surface_id 가 아니다

`--child` 는 부모별로 0 부터 발급되는 **child index**(`ChildTerminalRegistry::next_index_for`)를
받는다. `terminal.children` 이 반환하는 `surface_id` 와는 다른 번호 공간이며, 둘 다 정수라
혼동하기 쉽다. 두 공간은 **실제로 겹칠 수 있다** — 새 인스턴스는 surface id 도 1, 2, 3… 이라
`--child 2` 가 index 2 인지 surface 2 인지 구조적으로 구분되지 않는다. 그래서 넘어온 값을
surface_id 로 자동 해석하지 않고, **인자 의미는 index 로 고정한 채 에러 메시지가 안내**한다:

| 넘긴 값 | 응답 |
|---|---|
| 같은 부모의 `child_surface_id` | `… 4 is a child_surface_id, not a child index — use \`--child 2\`` |
| 다른 부모의 `child_surface_id` | `… under a different parent — use \`--surface 9000 --child 4\`` |
| 그 외(오타·범위 밖·이미 정리됨) | `… (valid child indices: 0, 2; 2 children)` |
| 그 외 + 등록된 child 가 0 | `… (no children registered under surface 9000)` |

kill/release/respawn 세 경로가 같은 메시지를 쓴다. 실패는 `exit=1` + stderr 이므로, 일괄
처리 스크립트는 **호출당 종료코드를 확인해야 한다** — 버리면 전건 실패를 성공으로 오인한다.

### `--surface` 생략과 다중 윈도우

`kill`/`release`/`respawn`/`broadcast` 는 `--surface`(parent) 를 생략할 수 있다 — host 가
현재 engine 의 `child_terminals.single_parent()` 로 폴백한다(parent 가 정확히 1개일 때만
성공, 0 개·2 개 이상이면 에러). 이 폴백은 **그 engine(= 하나의 main window) 안에서만**
유일성을 본다 — main window 가 2 개 이상 열린 세션에서는 애초에 어느 window 를 봐야
하는지가 정해지지 않는다. 그래서 이 4 개 메서드가 `--surface` 없이(그리고 라우팅 가능한
다른 리소스 id 도 없이) 호출됐는데 main window 가 2 개 이상이면, focused window 로 조용히
새지 않고 라우팅 단계(`App::find_request_owner`)에서 명시적 에러로 거부한다(단일 윈도우
세션은 기존처럼 생략 가능 — 하위 호환). 구현: `src/app/request_owner.rs`
`ambiguous_parent_fallback_requires_surface`.

## 비-목표 (Out of scope)

- **에이전트 특화 로직** — codex/claude 바이너리 command 빌더, hook/trust, telemetry 는 플러그인에 잔류한다. 호스트는 임의 command 만 붙인다.
- **`terminal launch`(새 workspace 생성)** — 에이전트 편의 명령이라 범위 밖.
- **soft 입력 독립성(잔여 입력 비오염)** — 현재는 단순 `surface.send` 로 붙인다(ADR 방향성만).

## Acceptance Criteria

- Given workspace `<ws>` When `terminal.spawn{parent=P, command}` Then 자식 터미널이 생성·registry 등록되고 `occupancy_of(child)==Soft`·`holder.parent==P`·`attached=false`.
- Given 점유된 child C When `terminal.kill` Then `occupancy_of(C)==None` + surface 닫힘.
- Given 죽은 자식이 남은 registry When `terminal.children` Then reconcile 로 목록에서 제거.
- Given 이미 존재하는 임의의 surface(spawn 으로 만들지 않은 일반 터미널 탭 포함) When `terminal.adopt{surface=P, target}` Then `occupancy_of(target)==Soft`·`holder.parent==P`·`terminal.children` 목록에 나타남.
- Given 이미 등록된 child 또는 hard 점유 중인 대상 When `terminal.adopt` Then 에러 반환 + registry 불변.
- Given 점유된 child C When `terminal.release` Then `occupancy_of(C)==None` + `terminal.children` 목록에서 사라짐 + surface(탭)는 여전히 열려있음(닫히지 않음).
- Given 등록되지 않은 child index When `terminal.release` Then 에러 반환.
- Given C 와 무관한 다른 surface 가 hard 점유 중 When `terminal.release{child=C}` Then 그 hard 점유는 영향받지 않음.
- Given 실행 중인 child C When `terminal.state{surface=C}` Then 응답이 `state`·`evidence`·`confidence`·`surface_id:C` 를 싣고 `state` 는 `exited` 가 아니다(값은 위 판정 우선순위 표).
- Given `terminal.kill`로 종료된 child C When `terminal.state{surface=C}` Then `state` 가 `"exited"` 다 (`"active"`가 아님).

## spawn 준비와 실제 PTY 종료

spawn은 registry 등록·soft 점유 준비 후 command를 전송한다. 준비나 동기 전송 단계에서 오류가 나면 이번 호출이 만든 surface만 표준 agent close 경로로 정리하고 자신의 registry/점유도 회수한다. adopt의 실패는 기존 surface나 같은 부모가 이미 갖고 있던 soft 점유를 변경하지 않는다.

headless의 실제 PTY 종료도 GUI와 같은 host process-exit 경로를 사용한다. SessionEnd 없이 종료해도 process-exit 훅을 발화하고 soft 점유를 정리한다. headless도 HookFired의 task waiter를 처리하며 view 전용 ProcessExited broadcast는 GUI에 남는다. 종료 원인만으로 작업 성공을 추론하지 않는다.
