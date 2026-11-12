# Surface Highlight (주의 환기)

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent · 원격 사용자
- **설계**: 주의 환기 상태의 소유권
- **화면**: surface 테두리, 탭 제목, 워크스페이스 배지에 표시한다.

## 목적

작업이 끝났거나 사용자의 응답이 필요한 surface를 알려 준다.
완료 IPC, 셸 명령 종료, plugin 훅, 알림이 같은 상태를 사용한다.
상태 이름은 Attention이고, 이를 화면에 그린 결과를 Highlight라고 부른다.

## 내부 동작 (headless-valid)

`LiveDomainState.attention`의 `AttentionStore`는 surface마다 `{ kind, raised_at }` 하나를 보관한다.
`raise_attention`은 ID 0을 무시하고 기존 레코드를 새 kind와 시각으로 교체한다.
높은 우선순위의 kind도 다음 raise로 교체될 수 있다. 우선순위는 여러 surface를 함께 표시할 때 사용한다.

| 종류 | 의미 | 표시색 | 우선순위 |
|---|---|---|---|
| `Completion` | 작업 완료 | `surface_highlight_done_border` (attention-completion) | 10 |
| `NeedsInput` | 사용자 응답 대기 | `surface_highlight_input_border` (attention-needs-input) | 30 |

`effects_of`가 효과를 결정한다. 두 kind 모두 `panel_item=false`다.
attention만으로 알림 패널 항목을 만들지 않으며, 필요하면 발생원이 `NotificationStore::add`를 별도로 부른다.

### 표시

- 테두리는 Theme의 강조 테두리를 쓴다. 우선순위는 NeedsInput, 점유(soft/hard), Completion 순서다.
  따라서 점유는 완료 테두리를 가리지만 입력 요청 테두리는 가리지 않는다. 한 surface에는 선 하나만
  남는다. 입력 요청이 있는 점유 surface에는 2px 입력 요청선만 그리고 1px 점유선은 그리지 않는다.
  hard 점유의 강제 해제 버튼은 그대로 보인다.
- 탭 제목은 포함된 surface 중 가장 높은 kind의 색을 쓴다. attention이 없으면 활성·평상시 제목색을 쓴다.
  busy를 나타내는 녹색 점과는 별도다.
- 펼친 사이드바는 kind별 숫자 배지를 표시하고 99를 넘으면 `99+`로 표시한다.
  둘 다 있으면 NeedsInput이 왼쪽, Completion이 오른쪽이다. 하나뿐이면 기존 오른쪽 위치를 쓴다.
- 접힌 사이드바의 점은 NeedsInput, Completion, running 순서로 대표색을 고른다. 점은 6px이고 아바타와 겹쳐도 떨어져 보이도록 모든 점을 사이드바 배경색 1.5px 고리로 두른다.

### 해제

`clear_attention`은 실제 레코드를 지웠으면 true를 반환한다. 이미 비어 있으면 false다.
사용자 확인과 IPC 해제는 아래 규칙을 따른다.

| 경로 | 동작 |
|---|---|
| 실제 렌더에서 surface에 포커스 | 포커스 pane·현재 workspace의 surface이고 창이 OS 포커스를 가지면 kind와 관계없이 해제 |
| 알림 한 개 또는 모두 읽음 | 해당 surface에 안 읽은 알림이 남지 않았을 때 해제 |
| `surface.attention.clear` | 지정한 surface를 해제; 선택 kind가 있으면 정확히 일치할 때만 해제 |

창이 OS 포커스를 잃은 동안에는 보이는 포커스 탭도 attention을 지우지 않는다. webview 자식이 키보드 포커스를 쥔 경우는 창이 포커스를 가진 것으로 본다. 창이 포커스를 되찾으면 다음 렌더에서 해제한다. 그래서 화면에 보이는 포커스 탭은 attention 색을 띠지 않지만, 포커스가 없는 pane이나 다른 workspace의 활성 탭은 순위 사다리에 따라 attention 색을 유지한다.

hard 점유 중에는 서버 로컬 사용자가 보거나 알림을 읽어도 attention을 지우지 않는다.
`clear_attention_local`이 이 조건을 검사한다. 알림 자체는 읽음으로 처리하며 soft 점유에는 제한하지 않는다.
점유가 풀리면 다음 로컬 확인부터 다시 해제할 수 있다.

검증된 holder의 clear는 기본 `clear_attention`을 직접 부른다.
기본 함수 안에 hard 검사를 넣으면 holder 자신도 지울 수 없게 된다.
IPC는 같은 hard 조건을 더 앞에서 검사해 명시적인 오류를 반환한다.

### 원격 attach mirror 로의 전파 (server→client)

서버는 1Hz busy tick에서 attention 변화분을 holder에게 보낸다.
`Attention { surface_id, kind }`의 ID는 원격 ID이고 `kind:null`은 해제다.
캐시는 `(holder, kind)`를 기억하므로 값이 그대로면 보내지 않으며 holder가 바뀌면 초기값을 보낸다.
창이 있는 engine, parked engine, headless가 같은 처리를 한다.

client는 원격 ID를 로컬 mirror ID로 바꾸고 `set_mirror_surface_attention`으로 같은 AttentionStore에 적용한다.
로컬 `raise_attention`은 mirror를 무시한다. mirror도 OSC 133·Bell·OSC 9/777을 파싱하지만
attention을 중복 생성하지 않는다. 같은 이벤트의 알림 패널·toast·hook 처리까지 막는 것은 아니다.
mirror 대상으로 호출한 completion도 서버로 전달하지 않고 attention 변경 없이 성공한다.

mirror 사용자가 실제로 보거나 로컬 알림을 읽어 레코드를 지우면,
`clear_attention`이 `ClientAttentionClear`를 한 번 큐에 넣는다.
서버는 해당 workspace의 holder인지 검사한 뒤 지운다. 이미 지운 상태의 반복 호출은 전송하지 않는다.
서버 push와 mirror 제거는 전용 setter·forget 함수를 쓰므로 서버로 되돌아가는 반복 메시지가 생기지 않는다.

차분 전송은 유실된 같은 값을 다음 tick에 다시 보내지 않는다.
스트림 손실의 감지와 재동기화는 [attach 손실 처리](../../dev-guide/attach-behavior.md#통지를-받은-client-가-하는-일-재동기화-계약)를 따른다.
clear 전송 실패는 재시도하지 않는다. 오래된 확인을 나중에 적용해 새 attention을 지우지 않기 위해서다.

- 이미 포커스된 mirror에 새 attention이 오면 다음 렌더에서 바로 확인 처리한다. 로컬과 같은 규칙이다.
- surface 삭제와 세션 정리에서는 attention도 지운다. kind 변환은 같은 surface이므로 지우지 않는다.
- surface 단위 raw attach에는 workspace 기반 clear 전달과 GUI 확인 경로가 없다.
  이 경우 점유가 끝난 뒤 서버 로컬 확인으로 해제한다.
- 한 공유 레코드이므로 서버 사용자와 원격 사용자의 확인 상태를 따로 보관하지 않는다.

## Producer

| 발생원 | attention | 알림 패널 |
|---|---|---|
| 신규 toast 알림, Windows 절전 복귀 알림 | Completion | 별도 생성 |
| completion IPC/CLI | 요청 kind; 기본 Completion | 만들지 않음 |
| Claude stop, stop-failure, session-end | Completion | 훅별 알림 정책을 따름 |
| Claude notification(입력 대기 유형, [유형별 상태](../../plugins/claude/index.md#notification-유형별-상태)), AskUserQuestion pre-tool-use | NeedsInput | 훅별 알림 정책을 따름 |
| Codex PermissionRequest | NeedsInput | plugin 정책을 따름 |
| OSC 133 D 명령 종료 | 성공·실패와 관계없이 Completion | 만들지 않음 |
| 서버 Attention push | 서버 값 그대로 | 만들지 않음 |

Claude prompt-submit·session-start·active·post-tool-use·subagent-stop은 새 attention을 만들지 않는다.
Codex PostToolUse·Interrupt도 만들지 않으며 Codex Stop은 현재 Completion attention 대신 완료 알림 경로를 쓴다.
OSC 133은 셸 통합이 설치된 개별 명령 종료를 뜻한다. 같은 PTY 연결에서 프롬프트(A)를 그리기 전의 D는 셸 시작 보고라 attention을 만들지 않는다. 종료 코드는 memory 명령 기록과 CommandCompleted 훅에도 남긴다.
coalesce된 기존 toast는 신규 attention 발생으로 세지 않는다.

## 인터페이스

모든 요청은 surface ID를 명시하며 `Notification` 권한을 쓴다.

| CLI | IPC | 결과 |
|---|---|---|
| `tasty surface completion --surface <id> [--kind needs_input]` | `surface.completion` | attention 설정. needs_input 외 값은 호환상 Completion으로 읽음 |
| `tasty surface attention get --surface <id>` | `surface.attention.get` | `{ surface_id, kind }`; kind는 completion, needs_input 또는 null |
| `tasty surface attention clear --surface <id> [--kind <k>]` | `surface.attention.clear` | `{ ok, surface_id, cleared, previous_kind }` |

clear는 없는 surface, 모르는 kind, mirror, hard 점유 대상을 거절한다.
kind 불일치나 이미 비어 있는 상태는 성공이며 `cleared:false`다.
get은 mirror와 hard 점유도 허용한다. completion의 없는 대상 처리 호환과 clear의 검증은 서로 다르다.
clear는 알림 패널의 읽음 상태를 바꾸지 않는다.

## 비-목표 (Out of scope)

워크스페이스·탭 일괄 해제, 관측자별 확인 상태, 새 kind, 알림 읽음과 clear를 합친 API는 제공하지 않는다.
에이전트가 사용자 포커스를 바꾸어 해제하는 release API도 없다.

## Acceptance Criteria

- Completion·NeedsInput을 설정하고 get으로 같은 kind를 읽을 수 있다. headless도 동일하다.
- 정확한 kind 필터만 해제한다. 다른 kind와 빈 레코드는 `cleared:false`다.
- 같은 surface의 안 읽은 알림이 남으면 읽음 처리로 attention을 지우지 않는다.
- hard 점유는 서버 로컬 확인과 IPC clear를 막지만 holder의 확인은 허용한다. soft 점유는 막지 않는다.
- mirror 로컬 raise는 attention을 만들지 않고 서버 push는 적용한다.
- mirror에서 attention 레코드를 실제로 지운 경우에만 clear를 한 번 전송한다. 서버 push와 teardown은 전송하지 않는다.
- 같은 값은 다음 tick에 반복 전송하지 않는다. holder 교체는 같은 값이라도 전송한다.
- Given 창이 OS 포커스를 잃은 상태에서 포커스 surface에 attention이 올 때 When 화면을 그리면 Then attention이 남고, 창이 포커스를 되찾은 뒤의 렌더에서 해제된다.
- NeedsInput 테두리는 점유 중에도 보이고 Completion 테두리는 점유 표시 아래에 있다.
- Given hard 점유 surface에 NeedsInput이 있을 때 When 화면을 그리면 Then 가장자리에는 2px 입력 요청선만 있고 1px 점유선은 겹쳐 그려지지 않는다.

## 구현

상태와 정책은 `src/core/state/attention.rs`, IPC는 `src/adapters/ipc/handler/surface/{completion,attention}.rs`에 있다.
핸들러가 owner engine에 직접 적용해 headless에서도 동작하고 GUI 후처리는 redraw를 요청한다.
렌더 확인은 `src/gfx/gpu.rs`, 화면 표시는 divider·tab_bar·sidebar가 담당한다.
원격 전달은 `forward_attention`, holder 확인은 `apply_attached_attention_clear`를 사용한다.

검증은 상태 단위 테스트, `tests/e2e_tests.rs`의 IPC 왕복, `tests/attach_attention_loopback.rs`의 서버 wire 왕복으로 나뉜다.
raw socket loopback은 실제 GUI mirror의 큐→소켓 및 픽셀 렌더를 실행하지 않는다.
그 구간을 확인하려면 두 GUI 인스턴스를 연결해야 하며, 프로토콜 테스트 통과만으로 화면 검증을 대신하지 않는다.
