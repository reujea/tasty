# 원격 attach 자식 (claude·codex 공통)

attach 로 연결한 원격 워크스페이스(mirror)에 `tasty claude spawn` / `tasty codex spawn` 으로 자식 에이전트를 만들고, 그 자식을 이 Tasty 의 자식처럼 tell·children·state·kill 로 다룬다. 자식의 상태 변경은 부모가 있는 Tasty 의 기존 수신처(완료 로그·Codex 대화)에 기록되고, 턴 본문은 부모가 Codex 대화면 그 대화로, 물리 surface 면 부모 Tasty 에 저장한 뒤 완료 로그의 결과 ID 와 조회 명령으로 전달된다.

- C: 부모가 있는 Tasty(attach client, mirror 보유). L: C 의 mirror surface. S: 자식이 실제로 도는 Tasty(attach 서버). R: S 의 surface.
- host 쪽 전달(출처 토큰·holder 검사·통지 outbox·ACK·tombstone)은 [attach 동작](../dev-guide/attach-behavior.md)과 [원격 attach](../features/remote-attach/index.md)가 정본이다. 이 문서는 provider plugin 이 그 위에서 하는 일만 적는다.
- 공용 코드: `crates/tasty-plugin-agent-common/src/remote_attach.rs`·`remote_results.rs`(물리 부모의 결과 보관). claude 는 `crates/tasty-plugin-claude/src/remote.rs`·`crates/tasty-plugin-claude/src/codex_return/attach.rs`. codex 는 `crates/tasty-plugin-codex/src/handlers/remote.rs`·`crates/tasty-plugin-codex/src/delivery/attach_out.rs`.

## 매니페스트 선언

두 plugin 모두 `[contributes.remote_attach]` 를 선언한다. host 는 provider 목록을 갖지 않고 이 선언으로만 허용한다.

| plugin | `methods`(S 에서 holder 대신 실행) | `receive`(C 에서 원격 결과 수신) | `relay_meta`(S→C 로 옮기는 메타) |
|---|---|---|---|
| claude | `spawn`·`tell`·`kill` | `remote_receive` | `claude-last-stop-failure`·`claude-background-wait`·`claude-background-quiet`·`claude-stall-notice` |
| codex | `spawn`·`tell`·`kill` | `remote_receive` | 없음 |

`relay_meta` 는 C 의 완료 알림 문구(`notify-done`·`notify-stop-failure`)가 L 에서 읽는 메타다. 중계되는 훅 이벤트는 각 plugin 의 `[[contributes.hook_events]]` 와 host 가 만드는 `process-exit` 뿐이다.

## 누가 무엇을 하나

| 일 | 어디서 |
|---|---|
| 부모 해석(물리 surface / Codex 대화), Codex 결과 수신처, 완료 알림 훅 | C plugin. 기존 경로 그대로 L 에 묶는다 |
| 자식 실행 명령·세션 토큰·settings·프로필·권한 모드·CLI·cwd | S plugin. S 에 설치된 CLI·설정·토큰으로 실행한다 |
| 훅 처리·턴 확정 | S plugin. 자식은 S 의 `TASTY_SURFACE_ID` 로 S 에 보고한다. 훅 명령·install 절차는 바뀌지 않는다 |
| 상태·이벤트·메타 중계 | host. plugin 은 할 일이 없다 |
| 턴 결과 | S plugin 이 `attach.child_notice` 로 맡기고, C plugin 이 `remote_receive` 로 받아 기록한다. Codex 대화 부모는 Codex 원장, 물리 surface 부모는 결과 보관과 완료 로그다 |

훅에는 부모 Tasty 주소나 비밀이 들어가지 않는다. 결과는 기존 attach holder 연결로만 오간다.

## client(C) plugin

- **spawn**: `--workspace`(숫자 id)나 `--pane` 이 mirror 면 `attach.agent_target` 으로 확인한 뒤 `attach.agent_call{method: spawn}` 을 보낸다. mirror 가 아니면 기존 로컬 spawn 이다. workspace 이름으로는 mirror 를 고르지 않는다.
  - 서버가 기능을 모르면(`capable=false`) 아무것도 만들지 않고 오류로 끝난다.
  - 보내는 인자는 허용 목록뿐이다. claude: `cwd`·`prompt`·`profile`·`profile_file`·`permission_mode`·`model`·`effort`. codex: `cwd`·`prompt`·spawn 옵션·승인 정책. 부모가 있으면(Codex 대화든 물리 surface 든) 둘 다 `return_route`(route 토큰)를 붙인다. `_tasty_context`·부모·수신처·`caller_surface` 는 보내지 않는다.
  - route 토큰은 C plugin 이 spawn·tell 하나마다 새로 발급하는 불투명 값이다. C 는 토큰과 그때의 수신처를 영속 기록하고, 서버에는 토큰만 보낸다. 수신처는 Codex 대화(receiver id)이거나 물리 surface 부모(surface id 와 발급 때 그 surface 의 `claude-session-id` 메타)다. 이 기록은 경로를 풀거나 다른 부모로 다시 묶어도 지우지 않는다. claude 는 발급 뒤 7일이 지난 기록을 지우고 4096개를 넘으면 오래된 것부터 지운다. codex 는 수정 시각으로 30일이 지난 기록을 지우며 개수 상한은 없다. 지워진 토큰의 결과는 `rejected` 다.
  - 사용자가 명시한 `--cwd`·`--profile-file` 경로와 `--profile` 이름은 서버가 해석하고 확인한다. 원격 spawn 의 두 경로는 서버 기준 절대 경로만 받는다. 상대 경로를 서버 프로세스의 작업 위치로 풀면 의도하지 않은 파일을 실행할 수 있기 때문이다. 서버 plugin 은 터미널을 만들기 전에 절대 경로인지, `cwd` 는 디렉터리로·`profile_file` 은 파일로 서버에 있는지 확인하고 아니면 거절한다. C 에서 해석한 설정 경로(기본 프로필·plugin 설정)를 대신 넣어 보내지는 않는다. 로컬 spawn 의 상대 경로 처리는 그대로다.
  - 응답을 받으면 L 에 기존 경로를 묶는다(물리 부모: 완료 알림 훅, Codex 부모: 결과 경로). 묶기에 성공한 뒤에만 `attach.agent_commit` 을 부른다. commit 전에는 서버가 그 자식의 통지를 보내지 않으므로 첫 완료가 수신처 없이 지나가지 않는다. 묶기에 실패하면 commit 하지 않고 그 자식을 원격 kill 로 닫은 뒤 오류로 끝난다.
  - 응답의 `child_surface_id`·`child_index` 는 C 기준(L)이다. `remote: true`, `remote_child_surface_id`(R)를 덧붙인다.
  - 결과를 알 수 없으면(-32010, 35초 상한·재연결 실패·서버 재시작) 성공이나 실패로 단정하지 않고 결과불명으로 알린다. 그 call 의 부모는 plugin 메모리에 15분 남긴다. host 가 늦은 결과를 `remote_receive{kind: late_spawn}` 으로 주면 그 부모로 경로를 묶고 `recorded` 로 답한다(host 가 commit). 기다리는 부모가 없거나(plugin 재시작·만료) 서버가 오류를 냈으면 그 자식을 원격 kill 로 닫고 `rejected` 로 답한다.
- **tell**: 대상이 C 에 등록된 원격 자식이면 다음 순서로 보낸다.
  1. 조회와 검사: Codex 호출자의 자식인지, 물리 부모의 Claude 세션, 지금 경로. 여기서 실패하면 아무 기록도 바꾸지 않는다.
  2. [결과 소유자](#결과-소유자) 판정: 자식을 만든 부모가 아니면 `child_of_other_parent`, 소유자 기록이 없거나 읽을 수 없으면 `child_without_owner` 로 거절한다.
  3. 결과 경로를 정한다(Codex 호출자: 경로 묶기와 그 대화의 토큰, 그 밖: 이전 Codex 경로 해제와 `--caller-surface` 의 토큰). 입력은 `attach.agent_call{method: tell}` 로 보낸다.
  4. 경로 변경·토큰 저장·전송의 실패가 확정되면 경로와 이 tell 로 맡은 소유를 이전 상태로 되돌린다. 결과불명이면 서버에서 실행됐을 수 있으므로 새 경로·토큰·소유를 그대로 둔다.
  - `--caller-surface` 의 완료 알림 훅은 L 에 다시 건다.
- **kill**: index 로 고른 자식이 원격 자식이면 `attach.agent_call{method: kill}` 로 닫는다. 결과 경로는 바로 지우지 않고 끝난 경로로 남겨(24시간) 서버에 남은 결과가 늦게 와도 기록한다. 소유자 기록은 지운다.
- **children·state·parent**: host child registry 가 L 을 일반 자식처럼 답한다. claude children --format json 은 host 의 `remote`·`liveness` 필드를 그대로 싣는다. 연결이 끊긴 동안 liveness 는 `unknown` 이다.
- **remote_receive**(`kind: notice`): payload 의 결과를 그 토큰의 수신처로 기록한다. 물리 surface 수신처는 아래 [물리 surface 부모의 결과](#물리-surface-부모의-결과)를 따른다. Codex 대화 수신처는 다음과 같다. 완료 키는 서버 키에 `r.` 를 붙인다. 재전송은 같은 키라 Codex 원장이 `duplicate` 로 거른다. 토큰이 없거나 모르는 토큰이면 `rejected`, 일시 실패는 JSON-RPC 오류(host 가 다시 보낸다)다.
  - claude 는 `codex.deliver` 를 부른다(claude→codex 단방향). codex 는 자기 namespace 를 다시 부르지 않고 내부 `delivery::deliver` 를 직접 부른다(같은 worker 재진입 교착 방지).
  - 수신처는 결과에 실린 route 토큰으로 찾는다. 서버가 싣는 토큰은 그 턴이 끝날 때 자식에 묶인 마지막 토큰이다. 원격 자식에는 [결과 소유자](#결과-소유자)만 지시할 수 있으므로 그 토큰의 수신처는 언제나 소유 부모다. 같은 부모의 겹친 지시는 마지막 토큰(같은 부모)으로 돌아온다. 서버는 C 의 수신처를 모른다.

## 결과 소유자

서버는 자식마다 결과 경로를 하나만 두고, 턴이 끝날 때 그 경로의 마지막 토큰을 결과에 싣는다. 같은 C 의 부모들은 서버에서 같은 출처 키를 쓰므로 서버는 부모를 구분하지 못한다. 그래서 한 부모가 지시한 턴이 진행되는 동안 다른 부모가 tell 하면 앞 턴의 결과가 뒤 부모에게 정상 결과처럼 간다. 입력이 합쳐지거나 서버 plugin 이 스스로 턴을 열 수 있어 지시와 결과의 수를 맞춰 볼 수도 없다. 원격 자식에게 가는 지시는 모두 C 를 거치므로 C 가 소유자를 고정한다.

- 소유자는 자식을 만든 부모다. Codex 대화는 receiver id, 물리 부모는 surface 와 발급 때의 `claude-session-id` 메타(셸은 없음)다. 같은 surface 에서 세션이 바뀌면 다른 부모다.
- C plugin 은 데이터 폴더의 `attach-owners/<L>.json` 에 소유자를 영속 기록한다(폴더 잠금, fsync 후 이름 바꾸기). 새 자식임이 확인된 곳(spawn·늦은 spawn 의 묶기)에서만 새로 쓰며, 같은 surface id 에 남은 기록은 덮어쓴다. 소유자가 아직 없는 새 자식도 `owner: null` 로 기록한다. 원격 kill 이 성공하면 파일을 지운다.
- 부모 없이 만든 자식은 prompt 로 이미 턴을 열었으면 소유자 없음으로 고정한다. prompt 가 없으면 첫 tell 의 부모(호출 surface 가 없으면 소유자 없음)가 맡는다. 그 tell 이 확정 실패하면 `owner: null` 로 되돌린다.
- 기록 파일이 없거나 해석할 수 없는 자식(소유자 기록 이전에 만든 자식, 기록 유실)은 소유자를 알 수 없다. 첫 tell 이 가져가지 않도록 누구의 tell 이든 `child_without_owner`(invalid params)로 거절하며, 원격 호출·토큰 발급·경로 변경을 하지 않는다. 새 자식을 만든다.
- 다른 부모의 tell 은 결과가 왔든, 자식이 idle 이든 관계없이 `child_of_other_parent`(invalid params, 자식·소유자)로 거절한다. 원격 호출·토큰 발급·경로 변경을 하지 않는다. 시간으로 풀지 않는다. 다른 부모는 새 자식을 만든다. 부모를 넘기는 기능은 없다.
- 같은 부모의 후속 tell 은 그대로 보낸다.
- 서버 plugin 이 스스로 연 턴(auto_resume 등)의 결과도 마지막 토큰, 곧 소유 부모에게 간다.

## 물리 surface 부모의 결과

부모가 Claude 세션이나 셸 surface 면 받을 Codex 원장이 없다. C plugin 은 결과를 자기 데이터 폴더의 `attach-results/<결과 ID>.json` 에 보관하고 부모의 완료 로그(`<TASTY_PARENT_HOME>/notify/<부모 surface>.log`, notify mod·Monitor 가 읽는 곳)에 한 줄을 남긴다.

- 순서: 본문 저장(fsync 후 이름 바꾸기) → 완료 로그 한 줄 → 기록에 `notified` 표시 → `recorded` 응답(host ACK). 저장이나 로그 쓰기가 실패하면 JSON-RPC 오류로 답해 host 가 다시 보내게 한다. 다시 오면 저장분을 그대로 두고 알림만 다시 시도한다.
- 줄 문구는 plugin 의 `remote_result_message` 번역 키로 만들며 설정 언어에 따라 번역된다. 영어는 `surface <L>: turn result <ID> (<status>) is stored. Read it with: tasty claude remote-results --id <ID>`, 한국어는 `surface <L>: 턴 결과 <ID> (<status>)을(를) 저장했습니다. 읽기: tasty claude remote-results --id <ID>` 다(codex 자식은 `tasty codex remote-results`). 줄은 `[info] ` 표지로 시작한다. 번역과 관계없이 줄은 조회 명령 `tasty <claude|codex> remote-results --id <ID>` 로 끝나고 ID 는 16자리 소문자 hex 다. 자동화는 `turn result` 같은 자연어나 줄 안의 임의 hex 가 아니라 이 명령 부분에서 provider 와 ID 를 함께 뽑는다(ERE: `(^| )tasty (claude|codex) remote-results --id ([0-9a-f]{16})$`). status 는 `turn_ended`·`turn_failed`·`exited` 만 그대로 쓰고 그 밖은 `unknown` 이다. 본문·세션 이름 같은 자식이 만든 문자열은 줄에 넣지 않는다. 이 줄은 상태 변경 줄(`state changed`)과 따로 쓰이므로 부모는 상태 줄 뒤에 결과 줄을 기다려야 한다.
- notify mod 는 완료 로그의 줄을 고친 데 없이 부모 Claude 입력으로 넣는다. mod 가 없으면 Monitor 로 같은 로그를 읽는다.
- 결과 정체는 (route 토큰, client 완료 키)다. mirror surface 는 재연결로 바뀔 수 있어 정체에 넣지 않는다. 결과 ID 는 정체의 FNV-1a 64비트 해시(16 hex)이고, 같은 ID 칸의 기록과 정체를 대조해 다르면 다음 칸(최대 16칸)에 둔다. 같은 결과를 찾을 때는 빈 칸이 있어도 16칸을 모두 본다. 받기는 결과 폴더의 잠금 파일로 프로세스 사이까지 직렬화한다.
- 같은 결과가 다시 오면 `duplicate` 이며 다시 알리지 않는다. 다만 알림 줄을 쓴 뒤 `notified` 표시를 저장하지 못하면 다음 재전송에서 같은 ID 의 줄이 한 번 더 간다. 사용자 결정에 따라 장애 때의 중복 알림을 허용한다(결과 유실보다 낫다). 줄에 결과 ID 가 있으므로 같은 ID 를 다시 읽어도 해가 없다.
- 부모 surface 가 없거나(`parent_gone`), 발급 때 기록한 부모 Claude 세션과 지금 세션이 다르면(`parent_session_changed`) 본문은 저장하되 알리지 않고 `undelivered` 에 사유를 남긴 뒤 `recorded` 로 답한다(서버가 더 묻지 않는다). 같은 surface 에서 새로 시작한 다른 대화가 남의 결과를 받지 않게 하기 위해서다. 세션 메타가 없는 부모(셸)는 surface 존재만 본다. 부모 조회 자체가 실패하면 오류로 답해 다시 받는다. 발급 때 부모 세션 조회가 실패하면 세션 보호가 빠진 토큰을 보내지 않도록 spawn·tell 을 원격에 보내기 전에 오류로 끝낸다.
- 결과는 자식을 닫아도 남는다. 마지막으로 쓴 시각(알린 결과는 알린 시각)부터 7일이 지나거나 1024개를 넘으면 결론 난 결과(알렸거나 `undelivered`) 중 오래된 것부터 지운다. 아직 알리지 못한 결과는 지우지 않으므로 그때는 상한을 넘을 수 있다. 쓰다 끊긴 tmp 파일은 1시간 뒤 지운다. 조회로는 지우지 않는다.
- 조회: `claude.remote_results`·`codex.remote_results`(CLI `tasty claude remote-results [--id <ID>]`·`tasty codex remote-results [--id <ID>]`). `--id` 면 본문을 포함한 한 건(`{"result": …}`), 없으면 본문을 뺀 목록(`{"results": […]}`, `body_bytes` 포함, 받은 순서)이다. 목록은 모든 부모의 결과를 보이며 권한 경계가 아니다. 형식이 틀린 ID 와 없는 ID 는 invalid params 오류다.

## 서버(S) plugin

host 는 원격 출처 호출에 `parent_external = attach:o<n>`·`attach_context`·`workspace`·`pane`(spawn), `attach_target_surface`(tell·kill)를 넣는다. plugin 은 `attach:` 부모와 `attach_context` 가 함께 있을 때만 원격 출처로 본다.

- 이 부모에게는 완료 알림 훅을 걸지 않고 `codex.parent`·Codex 호출 문맥도 쓰지 않는다. 상태·이벤트·메타는 host 가 L 로 중계한다.
- spawn 은 `terminal.spawn` 에 받은 부모·위치·문맥을 그대로 싣고, 기존 로컬 spawn 과 같은 기동 경로(세션 토큰·settings·프로필·권한 모드·오류 감시)를 쓴다. claude 는 S 의 Claude Code 설정에 Tasty Stop·StopFailure 훅이 없으면 응답에 경고를 싣는다.
- 서버의 공개 호출(서버 사용자의 CLI·다른 도구)은 attach 원격 자식을 바꾸지 못한다. `tell`·`reboot` 의 대상 surface 부모가 `attach:` 이거나, `kill`·`respawn`·`child_profile`(claude)·`broadcast` 가 `attach:` 부모 키를 지정하면 plugin 이 거절한다. host 는 그 자식의 attach 점유 예외를 생성 plugin 단위로 주므로, 이 검사가 없으면 서버 로컬 호출이 같은 plugin 을 거쳐 점유를 우회한다. plugin 의 내부 동작(auto_resume·훅 처리)은 공개 호출이 아니어서 그대로다.
- 원격 출처 호출의 대상은 `attach_target_surface` 가 지금도 그 출처 부모(`terminal.parent` 의 `parent_external`)의 자식일 때만 쓴다. 공개 호출이 출처 키를 꾸며 넣어도 다른 surface 에는 닿지 않는다. 같은 부모의 자식인지까지 꾸민 호출을 가려내는 것은 host 의 문맥 토큰 확인 몫이다.
- tell 은 `attach_target_surface` 에만 보낸다. kill 은 `terminal.children{parent_external}` 에서 그 surface 의 index 를 찾아 `terminal.kill` 한다.
- `return_route` 가 있는 자식은 결과 경로를 attach 부모로 두고, 수신처 자리에 그 토큰을 둔다. 로컬과 같이 Stop 때 그 턴의 토큰을 고정하므로, 확정 전에 다음 지시가 새 토큰으로 묶어도 앞 턴의 결과는 앞 토큰을 싣는다. 결과 본문의 `route` 필드에 토큰을 싣는다.
  - claude: 경로의 `attach` 표지. 확정된 턴은 기존처럼 claim 해 영속 요청으로 남기고, 송신 스레드가 `codex.deliver` 대신 `attach.child_notice{surface_id: R, key: completion_key, payload: {completion}}` 를 부른다.
  - codex: 반환 기록의 `attach_parent`. 소유 서버로 턴을 확정한 뒤 원장 대신 `attach-out/` 영속 기록으로 옮기고 `attach.child_notice` 를 부른다.
  - `attach.child_notice` 호출 하나의 상한은 15초다. codex 는 이 호출을 worker 에서 하므로, 호출이 실패하면 그 회차에는 남은 기록을 묻지 않고 다음 깨우기로 미룬다. claude 는 송신 스레드에서 부른다.
  - `pending` 이나 호출 실패면 영속 기록을 두고 같은 키로 늘어나는 간격을 두어 다시 묻는다. `delivered`(C 가 recorded·duplicate)를 받은 뒤에만 지운다. 같은 키를 다시 맡기면 C 에 같은 결과가 다시 갈 수 있고, C 원장이 같은 키로 거른다(장애 재전달 허용).
  - `rejected(reason)`(host 가 영구히 전달할 수 없다고 판정했거나 C 가 거절)를 받거나, 첫 실패부터 24시간이 지나도 전달되지 않으면 결과를 지우지 않고 사유와 함께 포기한 기록으로 남긴 뒤 더 묻지 않는다. claude 는 outbox 의 `gave_up` 요청(`tasty claude codex-returns` 로 보고 `--retry` 로 다시 보낼 수 있다), codex 는 plugin 데이터 폴더 `attach-out/` 의 기록(`gave_up`·`error`)과 경고 로그로 남는다.

## 한계

- 원격 spawn 의 `--cwd`·`--profile-file` 은 CLI 가 C 에서 확인하지 않고 그대로 넘기며 서버가 확인한다([Attach 메커니즘](../dev-guide/attach-behavior.md#cli-경로-인자)). `--cwd` 를 생략하면 서버의 기본 위치에서 시작한다.
- respawn·broadcast·launch 는 원격 자식에 대해 지원하지 않는다.
- 결과불명 call 의 부모 기록은 plugin 메모리에만 있다. plugin 이 다시 시작한 뒤 온 늦은 자식은 닫는다.
- 서버 host 가 다시 시작하면 attach 출처가 사라진다. 그때 진행 중이던 call 은 결과불명이고, 그 출처의 자식 결과는 host 가 `rejected` 로 답해 서버 plugin 에 포기한 기록으로 남는다. 서버 재시작 뒤 출처를 복구해 결과를 다시 전달하는 기능은 없다.
- `remote_receive` 는 `codex.deliver` 와 같이 공개 IPC 로도 부를 수 있다(별도 권한 경계가 아니다).
- 물리 부모의 완료 로그 쓰기는 fsync 하지 않는다. `notified` 표시와 응답 뒤 전원이 끊겨 줄이 사라지면 다시 알리지 않는다. 부모가 재시작하는 동안 쓰인 줄은 새 세션의 notify mod 가 받지 않는다(시작 때 로그 끝부터 읽는다). 두 경우 모두 `remote-results` 목록으로 찾는다.
- 원격 자식은 만든 부모만 지시할 수 있다. 같은 surface 에서 Claude 대화를 새로 시작한 경우도 다른 부모라 새 자식을 만들어야 한다.
- 소유자 기록과 결과 경로는 mirror 기준이다. 수동 attach 는 끊기면 세션과 출처를 버리고, 다시 attach 하면 새 세션·새 mirror 가 된다. 이전 세션의 지시와 서버에 남은 결과는 새 mirror 로 이어지지 않는다(같은 세션의 자동 재연결만 이어진다). 옛 자식은 새 출처에서 지시할 수 없다.
- 로컬 자식의 물리 부모는 이 결과 보관을 쓰지 않는다. 로컬 부모는 상태 변경 줄과 화면 읽기(`tasty read screen`, 필요하면 `--show-dim`)로 자식을 본다. Claude Code 가 alt screen 으로 그리는 동안에는 지난 턴 본문이 scrollback 에 없을 수 있다.
