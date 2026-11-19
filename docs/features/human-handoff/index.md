# 휴먼 핸드오프 (Approval)

- **Status**: Implemented
- **주체**: AI Agent (요청) · 로컬 사용자 (응답)
- **설계**: 없음
- **코드**: `approval.*` 핸들러(`src/adapters/ipc/handler/approval/`), 기록 저장 `src/app/approval_records.rs`, 영속 `tasty-memory`
- **화면**: approval popup (pending 큐 head)
- **메서드 목록**: [reference/api](../../reference/api.md#휴먼-핸드오프-approval)

## 목적

에이전트가 위험한 동작 전에 사용자에게 결정을 요청하고 응답을 기다리는 기능. 단방향 [알림](../notifications/index.md)과 달리 **요청-응답 워크플로우**다.

## 내부 동작

### 흐름

`approval.request` → 큐 push + popup 노출 + `notification.create` 자동 생성 → 에이전트가 `approval.await(id)` 로 blocking 대기 → 사용자 응답 → await 반환. 응답 경로 3가지가 모두 같은 `approval.respond` 로 수렴: popup 버튼 클릭 · popup 단축키 `1..=9`(선택지 순서) · CLI `tasty approval respond`.

승인 팝업은 에이전트·플러그인·시스템 요청으로 열리므로 [passive 팝업](../../design/systems/popup.md#passive-팝업)이다. 큐에 넣을 때, 닫은 뒤 대기 요청으로 다시 열 때, 앞 요청에 답해 다음 요청으로 넘어갈 때 모두 키보드 포커스를 가져가지 않는다. 다음 요청으로 넘어갈 때는 팝업을 닫고 다시 열어 앞 요청에 준 포커스를 이어받지 않는다. 사용자가 터미널·explorer·dag_graph·native WebView 에 치던 키는 그대로 그곳으로 간다. 팝업을 누르면 포커스를 받는다.

숫자 키는 팝업이 키보드 포커스를 가졌고 코멘트 입력칸에 포커스가 없을 때만 선택이다. 포커스가 없는 팝업은 egui 입력을 받는 surface 로 간 숫자도 선택으로 읽지 않는다. 코멘트를 쓰는 동안 친 숫자(IME 조합 중 포함)는 코멘트 글자로 들어가고 응답을 보내지 않는다. egui `TextEdit`은 Key 이벤트를 소비하지 않으므로 뷰가 입력칸 포커스를 직접 확인한다. 코멘트를 쓴 뒤 키보드로 고르려면 Tab으로 입력칸을 벗어나거나 팝업의 다른 곳을 누른 뒤 숫자를 친다.

### severity 별 표시

| severity | 채널 |
|----------|------|
| `danger` | popup + notification |
| `warn` | popup + notification |
| `info` | notification only |

popup 은 창의 대기 목록(`MainViewState.approval_inbox`)의 맨 앞 요청을 그린다. 목록은 App 이 승인 저장소가 바뀔 때마다 맞추고, 끝난 요청은 바로 뺀다. 보이던 요청이 끝나면 팝업을 닫았다가 다음 요청으로 다시 열며, 코멘트는 인스턴스 데이터라 다음 요청으로 넘어가지 않는다. 닫힌 동안 끝난 요청으로는 다시 열지 않는다. **Esc 는 의도적으로 차단**(우회 응답 방지) — Esc 는 응답하거나 팝업을 닫지 않고 팝업의 키보드 포커스만 놓는다.

`destructive` 로 표시한 선택지는 위험 선택지로 그린다. 채움은 accent-danger 12% 를 bg-panel 에 섞은 불투명 색(`approval-danger-bg`)이고, 1px accent-danger 테두리(`approval-danger-border`)와 text-primary 글자(`approval-danger-fg`)를 쓴다. 뒤 배경과 관계없이 글자 대비는 Mocha 9.18 · Latte 5.80 이다. hover·누름은 일반 버튼 overlay 를 그 채움 위에 얹는다. 본체와 갤러리는 같은 `tasty_ui_widgets::approval_choice` 로 선택지를 그린다.

### 응답 규칙

self-response(같은 plugin 이 자기 요청에 응답)는 `-32011`, 이미 종료된 요청은 `-32010`. `approval.request` 의 metadata `kind` 가 host 전용 값 `capability_elevation` 이면 `-32602`(`invalid_request: …`)로 거절한다([capability-elevation](../capability-elevation/index.md)). host 가 발행한 권한 상승 요청에는 사용자(GUI popup·Local caller)만 응답·취소할 수 있고, agent·plugin 의 `approval.respond`·`approval.cancel` 은 `-32019`(`user_response_required`)로 거절한다. `approval.await` 는 local-only(plugin 호출 deadlock 방지) — `outcome ∈ {responded, timed_out, cancelled}`.

store 락이 poison 된 뒤(다른 스레드가 락을 든 채 패닉)의 `request`/`respond`/`cancel` 은 `-32014`(`store_poisoned`) 로 **거절**한다. 그 임계구역은 `state` → `history` → `waiters` 를 순서대로 갱신하므로 중간 상태가 남을 수 있고, 승인은 에이전트 행동의 관문이라 일관성을 확인할 수 없는 기록을 더 변경하지 않는다. 반면 `get`/`list` 는 표시용 읽기라 복구해서 계속 답한다. 어느 쪽도 패닉하지 않는다 — 이 store 는 승인 popup(메인 스레드)이 함께 쓰므로 패닉이 모든 창의 터미널 세션을 중단시킬 수 있다([error-handling](../../dev-guide/error-handling.md) "락 poison").

### 귀속 워크스페이스

요청이 묶일 워크스페이스는 **명시 `workspace_id` → `surface_id`가 속한 워크스페이스 → 그 창의 활성 워크스페이스** 순으로 정한다. surface를 지정한 요청은 사용자가 어느 워크스페이스를 보고 있든 같은 곳에 묶인다. 둘 다 안 준 요청만 활성 워크스페이스로 떨어지고 현재 화면에 따라 결과가 달라진다 — 재현이 필요하면 둘 중 하나를 준다([포커스 정책 (운영 상세)](../../design/policies/focus.md)). 이 값은 아래 영속 스코프와 함께 뜨는 알림의 워크스페이스를 정한다. popup 자체는 창 단위라 이 값에 안 걸린다.

### 영속

매 상태 전이마다 `tasty.approval.<id의 소문자 표기>` 키로 직렬화(workspace_id 있으면 `workspace:<id>`, 없으면 `global`, owner는 host). 응답/타임아웃/취소 후에도 보존 → `approval.history` 로 재시작 후 조회. 세션 요약은 별도 키(`tasty.approval.summary`, `approval.summary.set/get`, markdown)이고 history 에 섞이지 않는다.

승인 ID는 `req_` 와 Crockford base32 대문자 12자로 호스트가 만든다. memory key 는 소문자만 허용하므로 저장 key 에는 ID 를 소문자로 바꿔 쓴다. 이 형식 안에서는 대소문자 말고 다른 차이가 없어 서로 다른 ID 가 같은 key 로 모이지 않는다. 원래 ID 는 기록 값에 그대로 남고 `approval.history` 는 값에서 ID 를 읽어 원래 표기로 돌려준다. 이 형식이 아닌 ID 의 기록은 대소문자만 다른 ID 와 key 가 겹칠 수 있어 저장하지 않고 경고만 남긴다.

key·scope·직렬화와 기록 제출은 app 기능 모듈 `approval_records` 가 맡는다. 상태를 바꾸는 호출은 App 의 `request_approval`·`respond_approval`·`cancel_approval` 과 대기 워커의 `await_approval` 뿐이고, 이들이 전이를 적용한 직후 그 전이의 기록을 memory writer 에 제출한 뒤 접수 결과(접수·거절·제출 안 함)를 돌려준다. 진입점은 기록을 다시 제출하지 않고 완료를 어떻게 관측할지만 정한다.

| 경로 | 완료 관측 |
|---|---|
| IPC `approval.request`·`approval.respond`·`approval.cancel` | 기록이 commit 된 뒤 응답한다. writer 가 접수하지 않으면 그 거절로 응답한다 |
| GUI popup 응답, capability elevation 발행, cap `require_approval` | 기다리지 않는다. 메인 루프가 App 의 memory 핸들에서 결과를 거두므로 popup·View 수명과 무관하다 |
| `approval.await` 대기 워커(만료) | 그 대기가 만료 전이를 적용했을 때만 기록을 제출하고 워커 스레드에서 최대 30초 기다린다 |

응답·취소로 끝난 대기는 그 전이를 만든 쪽이 이미 기록을 제출했으므로 워커가 다시 쓰지 않는다. 같은 요청을 여러 워커가 기다려도 만료 전이와 그 기록은 하나다. capability elevation 권한 부여는 `respond_approval` 이 전이 직후, 기록 제출 전에 실행하므로 `approval.respond` 와 GUI popup 응답에 똑같이 적용된다.

저장 실패는 경고 로그로만 남긴다. 이미 끝난 메모리 상태 전이와 IPC 응답 내용은 바뀌지 않는다.

## 인터페이스

- **AI Agent**: `approval.request`(`Approval` 권한) → `approval.await`. capability elevation·cap require_approval 도 같은 메커니즘 위에 얹힌다([capability-elevation](../capability-elevation/index.md), [telemetry](../telemetry/index.md)).
- **사용자**: popup 응답 / `tasty approval {request,respond,await,get,list,history,summary}`.

## 관련

- [notifications](../notifications/index.md) · [capability-elevation](../capability-elevation/index.md) · [telemetry](../telemetry/index.md)
- [reference/api](../../reference/api.md#휴먼-핸드오프-approval)
