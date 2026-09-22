# ADR-0265: 자식 Claude 의 승인 정책은 호출자가 고르고, 아무도 안 고르면 사용자 자신의 설정이 남는다

- **Status**: Accepted
- **Date**: 2026-09-12
- **Tags**: plugin, claude, codex, permissions, cli, ipc, defaults, safety, profile
- **Group**: agent-integration

## Context

`tasty claude launch` / `spawn` / `respawn` / `reboot` 으로 띄운 자식 Claude Code 는
승인(permission) 정책을 **호출자가 지정할 수단 없이** 뜬다. 기동 명령을 만드는 지점이
셋인데 셋 다 권한 축을 갖고 있지 않다 —
`crates/tasty-plugin-claude/src/handlers.rs` 의 `build_launch_command`(`claude`
+ 선택적 `--task` + 선택적 `--settings`) 와 `start_claude_in_surface`(inline env
prefix + `claude` + 선택적 `--settings`), `crates/tasty-plugin-claude/src/reboot.rs`
의 `resume_command_line`(`claude -r <session_id>` + 선택적 `--settings`). 결정 시점에
레포의 **코드**에는 `--permission-mode` 도 `--dangerously-skip-permissions` 도 한 건도
없고, 유일한 등장은 `docs/plugins/claude/index.md` 의 "승인 정책 플래그 없음(미확인
상태)" 문단이다. 이 ADR 이 그 문단을 닫는다.

같은 레포의 codex plugin 은 같은 자리에 정책 축을 이미 갖고 있다 —
`crates/tasty-plugin-codex/src/handlers.rs` 의 `resolve_policy_args` 가
`approval`/`sandbox`/`full_auto` params 를 `-a <policy>` / `-s <mode>` /
`--dangerously-bypass-approvals-and-sandbox` 로 번역하고,
`crates/tasty-plugin-codex/tasty-plugin.toml` 은 `launch`/`spawn`/`respawn`/`reboot`
**네 arg_group 전부**에 그 플래그 셋을 선언하며 설정 페이지에
`default_approval_policy`/`default_sandbox_mode` 를 둔다. 즉 두 에이전트 plugin 이
"자식이 승인을 요구하면 어떻게 되는가" 라는 같은 물음에 서로 다른 답을 하고 있고,
claude 쪽 답은 **결정된 적이 없다.**

**실측 — Claude Code 가 받는 값 집합** (`claude --help`, 2026-09-12 확인):

- `--permission-mode <mode>` — choices: `acceptEdits`, `auto`, `bypassPermissions`,
  `manual`, `dontAsk`, `plan`
- `--dangerously-skip-permissions` — 모든 권한 검사 우회
- `--allow-dangerously-skip-permissions` — 그 우회를 **선택지로만** 켠다
- `--permission-prompts <target>` — `host` | `none` (`--print` 조합에서 누가 답하는가)

`--permission-mode` 가 `--dangerously-skip-permissions` 를 값 하나
(`bypassPermissions`)로 포함하는 **상위 축**이고, 승인 대기를 피하는 방법도 전부
아니면 전무가 아니다(`dontAsk`/`acceptEdits` 같은 중간 값이 실재한다).

claude plugin 이 정책 대신 이미 갖고 있는 축은 `--profile`/`--profile-file` 이다
(`crates/tasty-plugin-claude/src/handlers.rs` 의 `resolve_profile_file_param`) — 세션 settings JSON 을 `--settings
<path>` 로 주입하는 경로이고, 그 JSON 에 `permissions` 를 손으로 넣으면 정책이
바뀐다. 다만 그 축은 파일을 **미리 만들어 등록해야** 하고 호출 단위 override 라는
의미를 갖지 못한다. 그리고 그 축에는 이미 안전 규율이 하나 박혀 있다 —
`crates/tasty-plugin-claude/src/profile_merge.rs` 는 프로필을 조합할 때
`permissions.defaultMode` 값이 갈리면 last-wins 로 조용히 정하지 않고 **거부**한다.
권한 모드가 조합의 부산물로 약해지는 것을 막기 위한 규칙이고, 이 ADR 은 그 규율을
새 축에도 그대로 잇는다.

두 plugin 을 가르는 사실이 하나 더 있다. **codex 는 축이 둘이고 claude 는 하나다.**
codex 는 승인을 `never`(안 묻는다)로 떨어뜨려도 `-s <sandbox>` 라는 **독립된 봉쇄
축**이 남아 자식이 할 수 있는 일의 범위가 따로 정해진다. Claude Code 에는 그런 두
번째 축이 없어서, "안 묻는다" 를 기본으로 고르는 순간 그것이 곧 **봉쇄 없는 전권
실행**이 된다. 두 plugin 의 기본값이 갈리는 근거가 여기 있다.

## Decision

claude plugin 은 **`--permission-mode` 하나**를 호출자의 축으로 노출하고, 아무도
고르지 않으면 **아무 플래그도 붙이지 않는다** — 그 자리는 사용자 자신의 Claude Code
설정이 그대로 정한다. 구체적으로 일곱 가지를 정한다.

1. **노출하는 축은 `--permission-mode` 하나다.** `--dangerously-skip-permissions`
   를 따로 불리언으로 내보내지 않는다 — 그 우회는 `--permission-mode bypassPermissions`
   라는 값으로 이미 이 축 안에 있고, 같은 것을 두 창구로 노출하면 둘이 동시에 온
   경우를 또 정해야 한다. CLI 플래그 `--permission-mode` 와 IPC params
   `permission_mode` 가 같은 축의 양면이다(`docs/identity.md` 원칙 2 — 한쪽만 붙이는
   선택지는 이 원칙이 이미 막는다).
2. **기본값은 "플래그 미부착"이다.** params 도 설정도 비어 있으면 기동 명령에
   `--permission-mode` 가 붙지 않고, 자식은 사용자 자신의 설정
   (`~/.claude/settings.json` 의 `permissions.defaultMode` 등)이 정한 정책으로 뜬다.
   codex 가 고른 "미지정이면 비대화형(`never`)" 을 claude 는 고르지 않는다 — 위
   Context 의 축 개수 차이 때문이다. 이 선택의 대가는 아래 Consequences 에 적는다.
3. **설정 페이지에 기본값 항목을 둔다.** `default_permission_mode`(select,
   기본 `inherit`). `inherit` 은 "플래그를 안 붙인다" 와 같은 뜻이다. 전 자식에
   같은 정책을 걸고 싶은 사용자가 매 호출에 플래그를 적지 않아도 되게 하되,
   그 선택은 **사용자가** 한다.
4. **우선순위는 `params` > 설정 > 미부착이다.** params 는 이 호출에 한해
   설정 기본값을 덮는다(codex 와 같은 규칙).
5. **`--profile`/`--profile-file` 이 주입하는 settings JSON 에
   `permissions.defaultMode` 가 있고 `permission_mode` 도 함께 오면 거부한다.**
   어느 쪽이 이기는지 조용히 정하지 않는다 — `resolve_profile_file_param` 이
   `profile_file`+`profile` 조합에서, `profile_merge` 가 `defaultMode` 충돌에서 이미
   같은 선택을 했다. `permissions.allow`/`deny` 는 규칙 목록이지 모드가 아니므로
   충돌로 보지 않고 그대로 공존시킨다.
6. **적용 범위는 기동 경로 넷 전부다** — `launch`/`spawn`/`respawn`/`reboot`
   (`child_profile` 은 reboot 경로를 그대로 타므로 같이 받는다). **단 그 값은
   호출을 넘어 남지 않는다**: 레이아웃 복원이 셸에 그대로 타이핑하는
   `restore.command` surface meta(`crates/tasty-plugin-claude/src/hook.rs` 가 쓴다)
   에는 정책 플래그를 싣지 않는다. 복원은 호출자가 없는 자리에서 일어나므로, 한 번의
   호출에서 고른 정책이 그 뒤 모든 재부팅에 조용히 눌러앉으면 안 된다.
7. **값 검증은 실측 집합 멤버십이다.** 위 여섯 값만 통과시키고 모르는 값은
   `invalid_params` 로 거부한다(codex 의 `validate_choice` 와 동형). 각 값이 무엇을
   뜻하는지는 Claude Code 의 계약이므로 이 plugin 이 해석하거나 재정의하지 않는다 —
   전달만 한다.

## Consequences

- **얻은 것**: 호출자가 자식의 승인 정책을 호출 단위로 고를 수 있다. 무인 자동화
  흐름은 `--permission-mode` 를 명시해 승인 대기를 피할 수 있고, 반대로 감독이 필요한
  자식은 대화형 값을 명시할 수 있다. 두 에이전트 plugin 이 같은 물음에 같은 모양의
  창구(호출 params + 설정 기본값 + 우선순위)로 답하게 된다.
- **잃은 것 (기본값의 대가)**: 기본이 우회가 아니므로, 사용자의 설정이 승인을 요구하는
  값이면 **무인 spawn 자식이 승인 프롬프트에서 멈춘다.** 그 대가를 감수하는 이유는
  두 가지다 — ① 그 정지는 관측된다: tasty 가 설치하는 `Notification`/`PreToolUse`
  훅(`crates/tasty-plugin-claude/src/install.rs` 의 `MANAGED_HOOKS`)이 `needs_input`
  상태와 부모 알림을 내므로 부모는 깨어나 `tasty claude tell` 로 답할 수 있고, 훅이
  유실된 갈래는 [ADR-0266](0266-derived-stale-must-reach-the-push-channel.md) 이 닫는다.
  ② 반대 방향의 대가는 관측되지 않는다: 기본을 우회로 두면 사용자가 모르는 채 모든
  자식이 봉쇄 없이 도구를 실행하고, 그 사실은 아무 신호도 내지 않는다. 되돌릴 수 있는
  쪽(멈춘 자식)을 기본으로 골랐다.
- **운영 비용 / 유지 부담**: 값 집합이 Claude Code 버전에 의존하므로 하드코드한
  목록이 낡을 수 있다(아래 재검토 조건). 설정 항목 하나와 그 lang 키 셋이 늘고,
  프로필 충돌 검사가 settings JSON 을 한 번 읽는다 — 기동 경로에서만 일어나는
  1 회 I/O 다.

## Alternatives Considered

- **A: 축을 안 넣고 `--profile` settings JSON 으로만 둔다** — 이미 가능한 경로이고
  새 코드가 0 이다. 그러나 호출 단위 override 가 안 된다(파일을 미리 만들어 등록해야
  한다), 승인 정책 하나 때문에 프로필이라는 더 큰 개념을 끌어와야 하며, 무엇보다
  두 plugin 의 비대칭이 그대로 남아 "claude 자식은 왜 정책을 못 고르나" 가 매번
  재논의된다. 결정을 안 하는 것이 선택지가 될 수 없어서 기각.
- **B: `--dangerously-skip-permissions` 불리언만 노출한다** — 구현이 가장 작다.
  그러나 실측상 그것은 상위 축의 값 하나일 뿐이라, 중간 정책(`dontAsk`/`acceptEdits`/
  `plan`)을 고를 길이 영영 막힌다. 노출하는 유일한 정책이 **전권 우회**가 되므로
  "정책을 고른다" 가 사실상 "우회한다" 와 동의어가 된다. 기각.
- **C: codex 와 동형의 다축(policy + sandbox)** — 대칭이 가장 예쁘다. 그러나 Claude
  Code 에 대응하는 두 번째 축이 **없다** — 없는 플래그를 만들어 낼 수 없고, plugin 이
  스스로 샌드박스를 흉내 내는 것은 이 plugin 의 일이 아니다. 대칭은 창구의 모양
  (params + 설정 + 우선순위)에서 맞추고 축의 개수는 각 도구가 실제로 받는 것을
  따른다. 기각.
- **D: 기본을 `bypassPermissions` 로 둔다(codex 의 `never` 와 같은 취지)** — 무인
  자동화가 절대 안 멈춘다. 그러나 claude 에는 봉쇄 축이 없어 그 기본이 곧 전권이고,
  사용자는 자기 설정이 무시된 것을 알 방법이 없다. 위 Consequences 의 비대칭(멈춤은
  보이고 과권한은 안 보인다) 때문에 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- codex plugin 의 하드코드 기본값이 `never` 가 아니게 된다. 기본값을 가르는 논거
  (축이 둘 vs 하나)가 그 값을 전제로 서 있다. 이 값은
  `crates/tasty-plugin-codex/src/handlers.rs` 의
  `resolve_policy_args_defaults_to_never_approval_when_nothing_set` 테스트가 이미
  고정하고 있어, 바뀌면 그 테스트가 먼저 빨개진다.
- `crates/tasty-plugin-claude/src/install.rs` 의 `MANAGED_HOOKS` 에서
  `Notification` 또는 `PreToolUse` 가 빠진다. 기본값을 안전하다고 판단한 근거가
  "승인 대기가 훅으로 관측된다" 이므로, 그 훅이 사라지면 근거가 사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- Claude Code 가 `--permission-mode` 의 값 집합을 바꾼다(값 추가·삭제·개명).
  재는 법: `claude --help | grep -A3 -- '--permission-mode'` 의 choices 를
  `crates/tasty-plugin-claude/src/handlers.rs` 의 허용 값 상수와 눈으로 대조한다.
  값이 늘었는데 상수가 그대로면 새 값이 `invalid_params` 로 거부된다 — 조용한
  통과가 아니라 명시적 거부라, 사용자가 신고하는 형태로 드러난다.
- Claude Code 가 `--permission-mode` 와 `--settings` 의 `permissions` 를 동시에
  받았을 때의 우선순위를 문서화하거나 바꾼다. 결정 5 는 그 우선순위를 **알 수 없다**
  는 전제 위에서 "조용히 정하지 않고 거부" 를 고른 것이므로, 공식 규칙이 생기면
  거부 대신 그 규칙을 따르는 선택지가 열린다. 재는 법: 두 인자를 함께 준 세션에서
  실제 적용된 모드를 `/status` 등으로 확인한다.

## References

- [ADR-0072](0072-child-state-hook-observation-fusion.md) — 자식 상태 판정(관측 융합).
  이 ADR 의 "정지는 관측된다" 전제가 그 판정 위에 선다.
- [ADR-0266](0266-derived-stale-must-reach-the-push-channel.md) — 훅이 유실된 갈래에서
  정지가 push 채널까지 도달하게 하는 짝 결정.
- [`docs/plugins/claude/index.md`](../plugins/claude/index.md) — claude plugin 기능 문서.
  이 ADR 이 그 문서의 "승인 정책 플래그 없음(미확인 상태)" 문단을 닫는다.
- [`docs/plugins/codex/index.md`](../plugins/codex/index.md) — 대칭의 기준이 된 선례
  (승인/샌드박스 정책 플래그 절).
- [`docs/identity.md`](../identity.md) 원칙 2 — 에이전트 기능은 IPC + CLI 양면.
- 코드 근거(결정 시점의 현재 위치): `crates/tasty-plugin-claude/src/handlers.rs`
  (`build_launch_command`·`start_claude_in_surface`·`resolve_profile_file_param`),
  `crates/tasty-plugin-claude/src/reboot.rs`(`resume_command_line`),
  `crates/tasty-plugin-claude/src/profile_merge.rs`(`permissions.defaultMode` 충돌 거부),
  `crates/tasty-plugin-codex/src/handlers.rs`(`resolve_policy_args`).
- `claude --help` (2026-09-12 실측) — 위 값 집합의 출처.
