//! `system.pressure` — 요청 압력 게이지 조회.
//!
//! 재는 자리는 [ADR-0305](../../../../docs/adr/0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)
//! 가 정했고 이 모듈은 그것을 **밖에서 읽을 수 있게** 한다. 그 ADR 이 "잃은 것" 으로
//! 적어 둔 것이 바로 노출 경로의 부재였다 — 값은 프로세스 안에만 있었고 읽는 것은
//! 시험뿐이었다.
//!
//! ## 덩어리는 **모수마다 하나**다
//!
//! 응답은 재는 모수마다 한 덩어리로 갈린다 — 오늘 열둘이고, 아래에 그 열둘이 한
//! 절씩 있다(큐의 두 덩어리는 한 절에 함께 있다). **덩어리 이름 자체에 그 모수의 경계를 넣는다**: 응답을 그대로 덤프해도
//! 어느 수가 무엇을 센 것인지 갈린다.
//!
//! ★ 이 수를 세는 문장은 이 파일에만 두고 절 제목에는 서수를 쓰지 않는다. 한때
//! "두 덩어리" · "세 번째 덩어리" 로 적혀 있었고, 덩어리를 더한 커밋이 **두 번
//! 연속** 다른 문서만 고치고 여기와 CLI 도움말을 지나쳤다. 서수를 쓰면 덩어리를
//! 더할 때마다 고쳐야 할 자리가 절 수만큼 늘어난다.
//!
//! ## `queue_before_gate` / `handler_after_gate` — 게이트가 가르는 둘
//!
//! 두 값의 **모수가 다르다.** 큐 대기·깊이는 명령이 큐에서 나온 직후, 게이트보다
//! **앞**에서 재므로 뒤에 거부될 요청도 센다. handler 실행 시간은 게이트를 통과한
//! 뒤 `handle_checked_request` 에서 재므로 통과한 것만 센다. 둘을 같은 이름 아래
//! 묶으면 운영자가 "큐에 34 건이 앉았는데 handler 는 30 번 돌았다" 를 읽고도 그 차이가
//! 무엇인지 고를 수 없다.
//!
//! ## `plugin_round_trip` — **남을 기다린** 시간이다
//!
//! 위 둘은 호스트가 자기 큐와 자기 handler 에서 보낸 시간이다. `plugin_round_trip` 은
//! 호스트가 **plugin 프로세스의 답을 기다린** 시간이고, 그래서 이것이 같은 응답에
//! 있어야 운영자가 원인을 고를 수 있다 — 큐도 handler 도 빠른데 응답이 느리면 그
//! 시간은 plugin 안에 있었던 것이다.
//!
//! 그 덩어리의 모수는 위 둘과 또 다르다: **응답이 실제로 매칭된 요청만** 센다. 끝내
//! 답이 안 온 요청(취소 · deadline 만료 · plugin 종료)은 끝점이 없어 못 잰다.
//!
//! ## `db` — **디스크가 받아준** 시간이다
//!
//! `db` 는 `MemoryStore` 가 트랜잭션 commit 과 WAL checkpoint 에 쓴 시간이다. 위
//! 셋과 겹치지 않는 축이다 — commit 은 handler 시간 **안에** 들어 있으므로, 그 둘을
//! 나란히 두면 "handler 가 느리다" 와 "handler 안의 쓰기가 느리다" 가 갈린다.
//!
//! 그 덩어리 안이 다시 둘이다. `commits` 는 **성공한** 쓰기만 세고(거부는 롤백이라
//! 디스크에 남긴 것이 없다), `checkpoints` 는 부팅 때의 WAL 되감기다. 되감기는
//! 다른 커넥션이 읽는 중이면 못 끝내고 돌아오므로 `busy` 를 따로 센다 — 시간만
//! 봐서는 느린 것과 경합한 것이 안 갈린다.
//!
//! ## `connections` — **시간이 아니라 자리**다
//!
//! 앞의 넷은 전부 "얼마나 걸렸나" 이고 이것만 "자리가 남았나" 다. 요청이 하나도 안
//! 느려도 연결 자리가 차면 새 client 는 **붙지도 못한다** — 그 거절은 요청이 되기
//! 전에 일어나므로 앞의 네 덩어리 어디에도 안 남고 `ipc_calls` 에도 안 남는다
//! (JSON-RPC 요청이 아니라 TCP 연결이다). 그래서 이 덩어리가 없으면 "느리다" 와
//! "자리가 없다" 가 밖에서 같은 관측(응답 없음)으로 보인다.
//!
//! 그 덩어리의 모수도 앞과 다르다: 세는 것은 **이 포트에 붙은 TCP 연결 전부**이고,
//! 그 안에는 요청을 하나도 안 보내는 연결 — attach·mesh 스트림처럼 오래 붙어 있는
//! 것 — 도 들어간다. 자리를 먹는 것이 요청이 아니라 연결이라 그것이 맞는 모수다.
//!
//! `live` 만 **누계가 아니다.** 나머지 셋(`live_max` · `accepted` ·
//! `refused_saturated`)과 `limit` 은 이 응답의 다른 모든 수처럼 안 내려간다.
//! `limit` 은 게이지가 아니라 서버가 집행하는 상수라 게이지 밖에서 온다
//! ([`crate::adapters::production::tcp_ipc_server::MAX_CONCURRENT_CONNECTIONS`]) —
//! 그것이 같이 나가야 `live` 가 얼마나 상한에 가까운지가 한 응답 안에서 읽힌다.
//!
//! 이 덩어리에 시간이 **하나** 있다 — `accept_wait_bound_us_*` 와 그 기록 수 `accept_waits`. 새 연결이
//! OS 의 accept 큐에서 기다렸을 수 있는 시간의 **상한**이다(accept 루프가 큐를 마지막으로 비어 있다고
//! 본 뒤 지난 시간). 모수가 자리 계수와 같아서(루프가 꺼낸 TCP 연결 전부 = `accepted +
//! refused_saturated`) 여기 둔다. 그 대기는 요청 줄을 읽기 전이라 큐 대기(`queue_before_gate`)에 안
//! 잡히고, 루프가 빈 큐에서 100 ms 자므로 호출마다 연결을 여는 client 에게는 그만큼이 된다
//! ([ADR-0467](../../../../docs/adr/0467-the-accept-wait-is-reported-as-a-bound-in-the-connection-block.md)).
//!
//! ## `db_pragmas` — **누계가 아니라 열 때 한 번 되읽은 설정**이다
//!
//! 앞의 다섯은 전부 프로세스 수명 동안 자라는 수이고, 이것만 **안 자란다.** 두 SQLite
//! DB 가 열릴 때 건 연결 pragma(`journal_mode` · `synchronous` · `foreign_keys` ·
//! `journal_size_limit`)의 요청값과 **되읽은 실제값**을 DB 마다 한 덩어리로 싣는다
//! (`memory_db` · `state_db`). 여기 있는 이유는 `db` 덩어리와 같은 DB 를 말하기 때문이다
//! — "commit 이 느리다" 를 읽은 자리에서 "WAL 이 안 섰다" 가 함께 보여야 원인이 갈린다.
//!
//! 요청값을 같이 싣는 이유는 소스의 `"WAL"` 이 runtime 보장이 아니어서다(in-memory DB
//! 는 요청을 조용히 거절한다 — `tasty_memory::pragma` 의 doc). `degraded` 는 하나라도
//! 그 DB 모드의 허용 결과로 안 섰다는 뜻이고, 오류가 아니라 **열린 채로 쓰이는 상태**다
//! (ADR-0376).
//!
//! `state_db` 가 `null` 이면 "그 DB 가 이 프로세스에 열려 있지 않다" 이다 — 헤드리스는
//! 늘 그렇고 GUI 도 열기에 실패한 창의 안내 구간에서는 그렇다(`crate::db` 머리말). 두
//! 출처는 이 값으로 안 갈린다. `memory_db` 가 `null` 이면 스토어가 없는 조립(단위 시험)이다.
//!
//! ## `stream_push` — **요청이 아니라 밀어내기**다
//!
//! 앞의 덩어리들은 전부 client 가 **물어본** 것(요청·연결·쓰기)을 잰다. 이것은 서버가
//! attach·mesh 스트림 연결로 **밀어낸** 프레임을 잰다 — 모수는 스트림 허브의 push 이고,
//! 요청 하나 없이도 자란다. 그래서 `connections` 와 겹치지 않는다: 그 덩어리는 자리를
//! 세고, 이것은 그 자리 중 스트림 연결에 무엇이 쌓이고 무엇이 버려졌는가를 센다.
//!
//! 안의 세 수는 성질이 둘로 갈린다(ADR-0400). `frames_dropped` · `clients_lagged_out` 는
//! **누계**라 안 내려가고, `backlog` 만 지금 살아 있는 연결들의 sink 에 쌓인 양이라
//! 내려간다. `sink_capacity` 는 연결 **하나**의 sink 상한이다 — `connections.limit` 과 같은
//! 이유로(서버가 집행하는 상수) 게이지 밖에서 와서, `backlog` 이 얼마나 찼는지가 한
//! 응답에서 읽힌다. 연결별 **연속** drop 수(`lag`)는 여기 없다 — 성공 한 번에 0 이 되는
//! 강제분리의 좌변이라, 뽑는 시점에 따라 같은 사건이 0 으로도 보인다.
//!
//! 허브가 이 프로세스의 엔진에 주입되지 않은 조립(단위 시험)이면 `null` 이다.
//!
//! ## `queue_admission` / `queue_dispatch` — 큐에 **든** 쪽과 **꺼낸** 쪽
//!
//! `queue_before_gate` 는 큐에서 나온 명령이 얼마나 기다렸는가다. 이 둘은 그 큐의 양 끝을
//! 잰다(ADR-0435). `queue_admission` 은 입장 장부(ADR-0391)다 — 지금 든 바이트·명령 수·주입
//! 명령 수는 **내려가는 값**이고, `peak_bytes` 와 거절 누계 둘(`refused_bytes` ·
//! `refused_depth`)은 안 내려간다. 모수는 큐에 **들어오려던** 요청이라 거절된 것도 센다 —
//! 거절은 큐에 한 번도 안 들어가므로 뒤의 어느 덩어리에도 안 남는다. 상한 둘(`limit_bytes` ·
//! `limit_injected_depth`)은 `connections.limit` 과 같은 이유로 게이지 밖에서, 그 장부가
//! 집행하는 값으로 온다. 장부는 IPC 서버가 만들고 주입기가 들어서, 서버가 안 뜬 조립(단위
//! 시험)이면 이 덩어리가 `null` 이다.
//!
//! `queue_dispatch` 는 큐에서 **꺼낸** 쪽의 누계다(ADR-0412) — 꺼낸 회차와 그 회차가 예산
//! (명령 수 · 시간)에 닿아 멈춘 수, 기한이 큐에서 지나 실행하지 않은 수, 실행을 시작한 수,
//! 그리고 `in_flight`(지금 실행 중이고 응답을 기다리는 쪽이 아직 기다리는 요청)와 그 최댓값.
//! `in_flight` 만 내려간다. 둘을 한 덩어리로 묶지 않는 이유는 모수가 달라서다 —
//! `queued_commands` 와 `started` 의 차는 "아직 큐에 있다" 가 아니다(큐 안에서 만료된 것과
//! 기다리던 쪽이 물러난 것이 섞인다). 아래 "두 수의 차이를 여기서 빼지 않는 이유" 와 같은 함정이다.
//!
//! ## `keyed_requests` — **키를 실은 요청만** 센다
//!
//! 멱등 키를 실은 요청이 보존소에서 받은 판정이 갈래마다 한 칸이다(ADR-0422) — `executed`
//! (처음 보는 키, 실행했다) · `replayed`(같은 요청, 보관된 답을 냈다) · `conflicted`(다른 요청,
//! 아무것도 안 했다) · `discarded`(실행은 됐고 답은 버려졌다) · `in_flight`(같은 요청이 진행
//! 중이었다 — 합류했다). 한 요청은 자기를 맡은 층의 판정으로 **한 번**만 세진다. 전부 누계다.
//! `executed` 는 재시도가 아니지만 모수다 — 재생 수만으로는 그것이 키 실은 실행 열 건 중
//! 하나인지 만 건 중 하나인지 모른다. `queue_dispatch.in_flight` 와 이름이 같고 뜻이 다르다
//! — 덩어리를 가른 이유 중 하나다.
//!
//! ## `slow_requests` — 분포가 아니라 **느린 요청 한 건씩**이다
//!
//! 앞의 덩어리들은 전부 집계다. 집계는 "100 ms 를 넘은 것이 몇 건" 까지 말하고, **그 한 건의
//! 시간이 어느 단계에 있었나** · **그 plugin 대기가 어느 요청의 것이었나** 는 못 말한다
//! (ADR-0436). 이 덩어리는 문턱(`threshold_us`)을 넘은 요청을 한 줄씩 싣는다 — 호스트가 발급한
//! `request_seq`(JSON-RPC `id` 도 Event Bus `trace_id` 도 아니다) · `method`(canonical 이름, 모르는 이름은 받은 그대로 —
//! `MAX_METHOD_BYTES` 에서 자른다) ·
//! `caller`(봉투가 말한 local/agent) · `queue_wait_us` · `host_us`(꺼낸 뒤 호스트가 다 다루기까지,
//! 게이트 포함 — `handler_after_gate` 와 모수가 다르다) · `outcome` · `error_code`(호출자가 실제로
//! 받은 답 — 읽는 순간 아직 안 나갔으면 둘 다 `null`, ADR-0468) · plugin 으로 넘겼으면 `plugin_hops`(hop
//! 마다 `plugin_id` · `host_request_id` · `wait_us` · `outcome`) · `total_us`.
//!
//! 모수는 **호스트 IPC 큐를 지난 요청 중 문턱을 넘은 것**이다. plugin 이 부른 host-call 은 큐를
//! 안 지나 번호가 없어 여기 안 든다. IPC `file_handler.dispatch` 가 파일 핸들러 큐를 거쳐 plugin
//! 으로 넘긴 forward 도 큐에서 번호를 잃어 그 hop 이 원 요청 줄에 안 붙는다. `system.pressure` 자신은 넣지 않는다 — 조회가 링을 밀어내면
//! 조회할 때마다 원인 요청이 사라진다. 링은 메모리 안의 고정 용량(`capacity`)이라 넘치면 오래된
//! 줄부터 밀려나고, `admitted` 는 켜진 뒤 링에 든 누계라 줄 수와의 차가 밀려난 수다. `host` 가
//! `null` 인 줄은 열린 자리가 밀려난 뒤 plugin hop 만 온 것이다.
//!
//! ## `gate_refusals` — 게이트가 **돌려보낸** 요청이다
//!
//! 진입 게이트(`handler/checked.rs` 의 `check_request`)에 든 요청의 판정 누계다
//! (ADR-0548). `judged` 가 모수 — 게이트에 든 요청 전부(Local 도, 큐를 안 지나는 plugin
//! host-call 도 센다) — 이고, 거절이 게이트마다 한 칸이다: `permission_denied`(권한, `-32001`) · `cap_blocked`(텔레메트리 cap, `-32007`) ·
//! `throttled`(rate limit, `-32010`). 게이트는 이 차례로 보고 앞에서 돌려보낸 요청은 뒤를 안 지나므로
//! 한 요청은 많아야 한 칸이다. 셋을 합치지 않는 이유는 처방이 달라서다(권한을 청한다 · cap 을 푼다 ·
//! 기다린다). `permission_denied` 는 권한 게이트가 돌려보낸 `-32001` 만 센다 — 그 거절은 모두
//! `-32001` 이지만 역은 아니다. 권한이 모자란 거절 말고도 plugin · agent 가 부른 없는 메서드 이름과
//! 그들에게 열리지 않은 메서드를 함께 센다 — 그 둘은 권한을 청해도 안 풀린다. Local 호출은 이
//! 게이트에서 돌려보내지지 않으므로 없는 메서드여도 이 칸에 안 든다. `-32001` 인데 안 드는 것이 두
//! 부류다: 봉투 토큰 거절(`resolve_caller_from_envelope` — 게이트 앞이라 `judged` 에도 안 든다)과
//! 게이트를 지난 뒤 핸들러가 내는 `-32001`(통과 쪽에 든다).
//!
//! 전부 이 프로세스가 뜬 뒤의 누계다. 창 단위가 아니고 재시작하면 0 이다 — 영속되는
//! `agent.rate_limit_status` 의 `throttled_count`(버킷 하나의, 재시작을 넘는 누계)와 모수가 다르다.
//! 판정 뒤의 봉투 검사(멱등 키 길이)에서 돌려보낸 요청은 거절 칸에 안 들고, 엔진이 없는 부팅·종료
//! 구간에서 Local 이 아닌 호출자를 돌려보내는 것(`check_without_engine`)은 `judged` 에도 안 든다.
//!
//! ## 아직 안 재는 값의 자리는 미리 비워 두지 않는다
//!
//! 위 `connections` 덩어리는 재는 자리가 생겼을 때 함께 생겼다. 빈 덩어리를 미리
//! 넣지 않는 규칙은 그대로다 — 값이 0 인 덩어리는 "관측된 0" 으로 읽히고, 그것은 이
//! 파일이 평균을 `null` 로 두는 것과 정확히 같은 함정이다.
//!
//! ## 두 수의 차이를 여기서 빼지 않는 이유
//!
//! `commands - calls` 는 "거부된 수" 가 **아니다.** 게이트를 통과하고도
//! `handle_checked_request` 를 안 지나는 갈래가 있다 — gui 의 app 층 메서드
//! (`App::ipc_step_app_methods`)가 그 자리에서 답하고 돌아간다. 그 차를 이름 붙여
//! 내보내면 실재하지 않는 양을 재는 것이 되므로, 두 모수를 **나란히** 두고 뺄셈은
//! 하지 않는다. 게이트가 돌려보낸 수는 뺄셈이 아니라 그 자리에서 센 값으로 `gate_refusals` 에
//! 따로 있다.
//!
//! ## `*_hist` — 평균·최대가 못 답하는 것
//!
//! 시간을 재는 세 덩어리에는 분포가 하나씩 더 있다(`queue_before_gate.wait_us_hist` ·
//! `handler_after_gate.us_hist` · `plugin_round_trip.us_hist`). 평균과 최대만 있으면
//! **"전부 조금씩 느린가, 대부분 빠른데 꼬리가 몇 건인가"** 가 안 갈린다 — 두 상태는
//! 같은 평균과 같은 최대를 낼 수 있고 처방이 반대다(앞은 용량, 뒤는 그 몇 건의 원인).
//!
//! 각 분포는 `bounds_us` 와 `counts` 로 나간다. **`bounds_us` 를 세 번 되풀이하는 것은
//! 의도다** — 위 "덩어리 이름 자체에 경계를 넣는다" 와 같은 이유로, 응답을 그대로
//! 덤프해도 어느 칸이 무엇을 센 것인지 한 자리에서 읽혀야 한다. 값과 경계를 떼어 놓으면
//! 소비자가 경계를 자기 쪽에 복제하고 그 복제본이 갈린다.
//!
//! **`counts` 는 `bounds_us` 보다 한 칸 길고 누적이 아니다.** 칸끼리 겹치지 않으므로
//! 합이 관측 수이고(Prometheus 의 `le` 누적 버킷과 다르다), 마지막 칸은 마지막 상한을
//! 넘은 것들이라 상한이 없다 — 그 칸이 차면 "그 상한을 넘었다" 까지만 알 수 있고 얼마나
//! 넘었는지는 같은 덩어리의 `us_max` 가 답한다.
//!
//! `db` 와 `connections` 에는 분포가 없다. 앞은 게이지가 다른 크레이트(`tasty-memory`)에
//! 살아 이 histogram 타입을 못 보고(의존이 그 방향이다), 뒤는 자리가 시간이 아니고 하나 있는
//! 시간(accept 대기)도 잰 값이 아니라 상한이라 분포를 잴 축이 아니다.
//!
//! ## 평균이 `null` 일 수 있는 이유
//!
//! 관측이 없으면 `None` 이다. 0 을 돌려주면 "기다림이 없었다" 와 "잰 적이 없다" 가
//! 같은 값이 된다 — 그 구분은 `PressureSnapshot` 이 이미 `Option` 으로 들고 있고
//! 여기서 무너뜨리지 않는다.

use serde_json::json;
use tasty_ipc::protocol::JsonRpcResponse;

/// `system.pressure` — 프로세스 수명 누계를 읽는다.
///
/// `&Core` 로 충분하다. 안이 전부 원자값이라 읽기가 요청 처리의 가변 빌림과 다투지
/// 않는다 — `Core::pressure` 가 `&self` 인 것과 같은 이유다. `engine` 은 스트림 허브를
/// 꺼내는 데만 쓴다 — 허브는 부팅이 attach 레지스트리에 주입한 것 하나뿐이다.
pub(super) fn handle_system_pressure(
    core: &crate::core::Core,
    engine: &crate::core::CoreState,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let mut body = snapshot_json(
        &core.pressure().snapshot(),
        &core.plugin_wait().snapshot(),
        &core.db_latency().snapshot(),
        &core.connections().snapshot(),
    );
    let state_db = crate::db::with_state_db(|db| db.applied_pragmas.clone());
    body["db_pragmas"] = db_pragmas_json(
        core.memory_pragmas(),
        core.memory_init_fallback(),
        state_db.as_ref(),
    );
    body["stream_push"] = stream_push_json(engine.attach.notifier().map(|hub| hub.loss()));
    // 입장 장부는 서버가 만들고 주입기가 같은 것을 든다(ADR-0391) — `Core` 가 닿는 길은 그
    // 주입기뿐이다. 서버가 안 뜬 조립이면 주입기나 장부가 없고, 그때 `queue_admission` 은 `null`.
    let ledger = core
        .host_ipc_injector
        .get()
        .and_then(|injector| injector.admission());
    let queue =
        tasty_ipc::dispatch::CommandQueueSnapshot::read(ledger.map(|a| &**a), core.dispatch());
    body["queue_admission"] = queue_admission_json(queue.admission.zip(ledger.map(|a| a.limits())));
    body["queue_dispatch"] = queue_dispatch_json(&queue.dispatch);
    body["keyed_requests"] = keyed_requests_json(&super::idempotency::retry_counts());
    body["slow_requests"] = slow_requests_json(&core.slow_requests().snapshot());
    body["gate_refusals"] = gate_refusals_json(&core.gate().snapshot());
    JsonRpcResponse::success(id, body)
}

/// 명령 큐에 **든** 쪽 — 입장 장부의 지금 값 · 최고 바이트 · 거절 누계와, 그것을 집행하는
/// 상한. 장부가 없으면 `null` 이다(이 파일 머리말 "아직 안 재는 값의 자리는 미리 비워 두지
/// 않는다").
///
/// 아래 세 함수는 원천 구조체를 **`..` 없이** 분해한다. 원천에 필드가 더해지면 여기서
/// E0027 로 빌드가 멈춘다 — 필드를 이름으로 옮기는 응답이 새 필드를 조용히 빠뜨리지 않게
/// 하는 채널이다(ADR-0435).
pub(super) fn queue_admission_json(
    ledger: Option<(
        tasty_ipc::admission::AdmissionSnapshot,
        tasty_ipc::admission::QueueLimits,
    )>,
) -> serde_json::Value {
    match ledger {
        Some((a, limits)) => {
            let tasty_ipc::admission::AdmissionSnapshot {
                queued_bytes,
                queued_commands,
                queued_injected,
                peak_bytes,
                refused_bytes,
                refused_depth,
            } = a;
            json!({
                "queued_bytes": queued_bytes,
                "queued_commands": queued_commands,
                "queued_injected": queued_injected,
                "peak_bytes": peak_bytes,
                "refused_bytes": refused_bytes,
                "refused_depth": refused_depth,
                "limit_bytes": limits.queued_bytes,
                "limit_injected_depth": limits.injected_depth,
            })
        }
        None => serde_json::Value::Null,
    }
}

/// 명령 큐에서 **꺼낸** 쪽 — 꺼낸 회차와 그 끝, 실행 전 만료, 시작한 요청과 지금 실행 중인
/// 요청(ADR-0412). 누계는 `Core` 가 늘 들고 있어 `null` 이 되지 않는다.
pub(super) fn queue_dispatch_json(d: &tasty_ipc::dispatch::DispatchSnapshot) -> serde_json::Value {
    let tasty_ipc::dispatch::DispatchSnapshot {
        rounds,
        rounds_stopped_by_count,
        rounds_stopped_by_time,
        expired_before_run,
        started,
        in_flight,
        in_flight_max,
    } = *d;
    json!({
        "rounds": rounds,
        "rounds_stopped_by_count": rounds_stopped_by_count,
        "rounds_stopped_by_time": rounds_stopped_by_time,
        "expired_before_run": expired_before_run,
        "started": started,
        "in_flight": in_flight,
        "in_flight_max": in_flight_max,
    })
}

/// 멱등 키를 실은 요청이 보존소에서 받은 판정 — 칸마다 한 갈래(ADR-0422). 보존소는 프로세스에
/// 하나라 `null` 이 되지 않는다.
pub(super) fn keyed_requests_json(r: &super::idempotency::RetryCounts) -> serde_json::Value {
    let super::idempotency::RetryCounts {
        executed,
        replayed,
        conflicted,
        discarded,
        in_flight,
    } = *r;
    json!({
        "executed": executed,
        "replayed": replayed,
        "conflicted": conflicted,
        "discarded": discarded,
        "in_flight": in_flight,
    })
}

/// 진입 게이트의 판정 누계 — 판정한 수와 게이트마다의 거절 수(ADR-0548). 누계는 `Core` 가 늘 들고
/// 있어 `null` 이 되지 않는다. **`..` 없이** 분해하는 것은 위 세 함수와 같은 이유다.
pub(super) fn gate_refusals_json(g: &tasty_telemetry::GateSnapshot) -> serde_json::Value {
    let tasty_telemetry::GateSnapshot {
        judged,
        permission_denied,
        cap_blocked,
        throttled,
    } = *g;
    json!({
        "judged": judged,
        "permission_denied": permission_denied,
        "cap_blocked": cap_blocked,
        "throttled": throttled,
    })
}

/// 느린 요청 링(ADR-0436) — 문턱을 넘은 요청 한 건씩. 링은 `Core` 가 늘 들고 있어 `null` 이 되지
/// 않는다. 줄과 hop 을 **`..` 없이** 분해하는 것은 위 세 함수와 같은 이유다.
pub(super) fn slow_requests_json(
    s: &tasty_telemetry::slow_requests::SlowRequestsSnapshot,
) -> serde_json::Value {
    use tasty_telemetry::slow_requests::{
        HostPart, PluginHop, SLOW_REQUEST_CAPACITY, SLOW_REQUEST_THRESHOLD, SlowRequest,
        SlowRequestsSnapshot,
    };
    let SlowRequestsSnapshot { rows, admitted } = s;
    let rows: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            let SlowRequest {
                request_seq,
                host,
                plugin_hops,
            } = row;
            let hops: Vec<serde_json::Value> = plugin_hops
                .iter()
                .map(|hop| {
                    let PluginHop {
                        plugin_id,
                        host_request_id,
                        wait_us,
                        outcome,
                    } = hop;
                    json!({
                        "plugin_id": plugin_id,
                        "host_request_id": host_request_id,
                        "wait_us": wait_us,
                        "outcome": outcome.as_str(),
                    })
                })
                .collect();
            let host = host.as_ref().map(|h| {
                let HostPart {
                    method,
                    caller,
                    queue_wait_us,
                    host_us,
                    outcome,
                } = h;
                // 답이 아직 안 나갔으면(plugin 으로 넘긴 요청이 기다리는 중) 두 칸 다 null 이다.
                let outcome = outcome.get();
                json!({
                    "method": method,
                    "caller": caller.as_str(),
                    "queue_wait_us": queue_wait_us,
                    "host_us": host_us,
                    "outcome": outcome.map(|o| o.as_str()),
                    "error_code": outcome.and_then(|o| o.error_code()),
                })
            });
            json!({
                "request_seq": request_seq,
                "host": host,
                "plugin_hops": hops,
                "total_us": row.total_us(),
            })
        })
        .collect();
    json!({
        "threshold_us": u64::try_from(SLOW_REQUEST_THRESHOLD.as_micros()).unwrap_or(u64::MAX),
        "capacity": SLOW_REQUEST_CAPACITY,
        "admitted": admitted,
        "rows": rows,
    })
}

/// 스트림 허브의 밀어내기 누계와 지금의 backlog. 허브가 없으면 `null` — 0 을 내면 "관측된
/// 0" 으로 읽힌다(이 파일 머리말 "아직 안 재는 값의 자리는 미리 비워 두지 않는다").
pub(super) fn stream_push_json(
    loss: Option<tasty_ipc::stream_hub::StreamLossSnapshot>,
) -> serde_json::Value {
    match loss {
        Some(l) => json!({
            "frames_dropped": l.frames_dropped,
            "clients_lagged_out": l.clients_lagged_out,
            "backlog": l.backlog,
            "sink_capacity": tasty_ipc::stream_hub::SINK_CAPACITY,
        }),
        None => serde_json::Value::Null,
    }
}

/// 두 DB 의 pragma 적용 결과. 누계 덩어리들과 성격이 달라 `snapshot_json` 밖에 둔다 —
/// 저것은 게이지 스냅샷만 받고, 이것은 열 때 한 번 정해진 값이다.
///
/// `memory_db` 에는 `init_failure` 가 하나 더 붙는다 — `memory.db` 를 못 열어 in-memory
/// 대체로 떴으면 `{cause, error}`, 아니면 `null`. 대체면 pragma 가 다 섰어도 `degraded`
/// 가 `true` 다: 파일을 못 연 저장소는 정상 상태가 아니고, `in_memory: true` 만으로는
/// "원래 in-memory" 와 "파일을 못 열어 in-memory" 가 안 갈린다(ADR-0485). `state_db`
/// 에는 이 칸이 없다 — `state.db` 초기화 실패는 대체 없이 안내 후 종료다.
pub(super) fn db_pragmas_json(
    memory_db: Option<&tasty_memory::pragma::AppliedPragmas>,
    memory_fallback: Option<&tasty_memory::InitFallback>,
    state_db: Option<&tasty_memory::pragma::AppliedPragmas>,
) -> serde_json::Value {
    let memory = memory_db.map(|a| {
        let mut v = applied_json(a);
        if memory_fallback.is_some() {
            v["degraded"] = json!(true);
        }
        v["init_failure"] = match memory_fallback {
            Some(f) => json!({ "cause": f.cause, "error": f.error }),
            None => serde_json::Value::Null,
        };
        v
    });
    json!({
        "memory_db": memory,
        "state_db": state_db.map(applied_json),
    })
}

fn applied_json(a: &tasty_memory::pragma::AppliedPragmas) -> serde_json::Value {
    let pragmas: serde_json::Map<String, serde_json::Value> = a
        .readings
        .iter()
        .map(|r| {
            (
                r.name.to_string(),
                json!({
                    "requested": r.requested,
                    "effective": r.effective,
                    "took": r.took,
                    "error": r.error,
                }),
            )
        })
        .collect();
    json!({
        "in_memory": a.in_memory,
        "degraded": a.degraded(),
        "pragmas": pragmas,
    })
}

/// 스냅샷 하나를 응답 본문으로 옮긴다.
///
/// 핸들러에서 갈라 둔 이유는 시험이 **아는 값**을 넣고 자리마다 대조할 수 있게 하려는
/// 것이다. `Core` 를 세우면 그 안의 누계가 0 이 아니라 시험이 자기 입력을 못 고른다.
pub(super) fn snapshot_json(
    s: &tasty_telemetry::PressureSnapshot,
    p: &tasty_telemetry::PluginWaitSnapshot,
    d: &tasty_memory::DbLatencySnapshot,
    c: &tasty_telemetry::ConnectionSnapshot,
) -> serde_json::Value {
    json!({
        "queue_before_gate": {
            "drains": s.queue_drains,
            "commands": s.queue_commands,
            "waits": s.queue_waits,
            "depth_max": s.queue_depth_max,
            "depth_mean": s.queue_depth_mean(),
            "wait_us_sum": s.queue_wait_us_sum,
            "wait_us_max": s.queue_wait_us_max,
            "wait_us_mean": s.queue_wait_us_mean(),
            "wait_us_hist": hist_json(&s.queue_wait_hist),
        },
        "handler_after_gate": {
            "calls": s.handler_calls,
            "us_sum": s.handler_us_sum,
            "us_max": s.handler_us_max,
            "us_mean": s.handler_us_mean(),
            "us_hist": hist_json(&s.handler_hist),
        },
        "plugin_round_trip": {
            "matched": p.matched,
            "us_sum": p.us_sum,
            "us_max": p.us_max,
            "us_mean": p.us_mean(),
            "us_hist": hist_json(&p.hist),
        },
        "db": {
            "commits": d.commits,
            "commit_us_sum": d.commit_us_sum,
            "commit_us_max": d.commit_us_max,
            "commit_us_mean": d.commit_us_mean(),
            "checkpoints": d.checkpoints,
            "checkpoints_busy": d.checkpoints_busy,
            "checkpoint_us_sum": d.checkpoint_us_sum,
            "checkpoint_us_max": d.checkpoint_us_max,
            "checkpoint_us_mean": d.checkpoint_us_mean(),
        },
        "connections": {
            "live": c.live,
            "live_max": c.live_max,
            "limit": crate::adapters::production::tcp_ipc_server::MAX_CONCURRENT_CONNECTIONS,
            "accepted": c.accepted,
            "refused_saturated": c.refused_saturated,
            "accept_waits": c.accept_waits,
            "accept_wait_bound_us_sum": c.accept_wait_bound_us_sum,
            "accept_wait_bound_us_max": c.accept_wait_bound_us_max,
            "accept_wait_bound_us_mean": c.accept_wait_bound_us_mean(),
        },
    })
}

/// 분포 하나를 경계와 **함께** 내보낸다. 되풀이되는 `bounds_us` 가 낭비로 보일 수
/// 있지만, 그것을 응답 어딘가 한 곳으로 빼면 칸의 뜻이 그 한 곳에만 있게 된다.
fn hist_json(h: &tasty_telemetry::HistogramSnapshot) -> serde_json::Value {
    json!({
        "bounds_us": tasty_telemetry::HistogramSnapshot::bounds_us(),
        "counts": h.counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tasty_memory::DbLatencyStats;
    use tasty_telemetry::{ConnectionStats, PluginWaitStats, PressureStats};

    /// 게이트 앞/뒤 두 모수가 **서로 다른 자리로** 나간다.
    ///
    /// 값을 전부 다르게 골라, 한 자리라도 다른 자리로 새면 대조가 깨지게 한다. 두
    /// 덩어리를 통째로 맞바꾸는 변경도 여기서 죽는다.
    #[test]
    fn the_two_moduli_land_in_their_own_blocks() {
        let p = PressureStats::default();
        p.record_drain(3);
        p.record_drain(5);
        p.record_queue_wait(Duration::from_micros(70));
        p.record_queue_wait(Duration::from_micros(130));
        p.record_handler(Duration::from_micros(11));

        let v = snapshot_json(
            &p.snapshot(),
            &PluginWaitStats::default().snapshot(),
            &DbLatencyStats::default().snapshot(),
            &ConnectionStats::default().snapshot(),
        );
        let q = &v["queue_before_gate"];
        let h = &v["handler_after_gate"];

        assert_eq!(q["drains"], 2);
        assert_eq!(q["commands"], 8, "집어 든 명령 수의 합");
        assert_eq!(q["depth_max"], 5);
        assert_eq!(q["depth_mean"], 4);
        assert_eq!(q["wait_us_sum"], 200);
        assert_eq!(q["wait_us_max"], 130);
        assert_eq!(
            q["waits"], 2,
            "대기를 기록한 명령 수 — 회차 합(8)과 따로 센다"
        );
        assert_eq!(
            q["wait_us_mean"], 100,
            "분모는 대기를 기록한 수(2)다 — 회차 합(8)이면 한 스냅샷 안에서 모수가 갈린다"
        );

        assert_eq!(h["calls"], 1);
        assert_eq!(h["us_sum"], 11);
        assert_eq!(h["us_max"], 11);
        assert_eq!(h["us_mean"], 11);

        assert!(
            h.get("wait_us_max").is_none() && q.get("us_max").is_none(),
            "두 모수가 같은 덩어리에 섞이면 안 된다"
        );
    }

    /// DB 지연은 **자기 덩어리**로 나가고 commit 과 checkpoint 가 섞이지 않는다.
    ///
    /// 두 모수를 다른 값으로 넣어, 한쪽이 다른 쪽 자리로 새면 대조가 깨지게 한다.
    #[test]
    fn the_db_populations_do_not_mix() {
        let d = DbLatencyStats::default();
        d.record_commit(Duration::from_micros(40));
        d.record_commit(Duration::from_micros(60));
        d.record_checkpoint(Duration::from_micros(900), false);

        let v = snapshot_json(
            &PressureStats::default().snapshot(),
            &PluginWaitStats::default().snapshot(),
            &d.snapshot(),
            &ConnectionStats::default().snapshot(),
        );
        let db = &v["db"];
        assert_eq!(db["commits"], 2);
        assert_eq!(db["commit_us_sum"], 100);
        assert_eq!(db["commit_us_mean"], 50);
        assert_eq!(db["checkpoints"], 1);
        assert_eq!(db["checkpoint_us_max"], 900);
        assert_eq!(db["checkpoints_busy"], 1, "끝까지 못 간 되감기를 따로 센다");
        assert_eq!(
            db["commit_us_max"], 60,
            "checkpoint 시간이 commit 최댓값으로 새면 안 된다"
        );
        assert!(
            v["handler_after_gate"].get("commits").is_none(),
            "DB 모수가 handler 덩어리에 섞이면 안 된다"
        );
    }

    /// 라우터가 이 이름에 **실제로 답한다.**
    ///
    /// 위 시험들은 `snapshot_json` 만 보므로 dispatch 팔이 사라져도 살아남는다.
    /// 이것이 그 나머지 반이다 — 프로덕션 진입점(`handle_with_caller`)을 그대로
    /// 지나 덩어리들이 응답에 있는지 본다.
    #[test]
    fn the_router_answers_this_name_for_a_local_caller() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
        let req = tasty_ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "system.pressure".into(),
            params: json!({}),
            session_token: None,
        };

        let resp = super::super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &crate::ipc::caller::CallerContext::Local,
        );

        assert!(
            resp.error.is_none(),
            "local caller 는 통과한다: {:?}",
            resp.error
        );
        let result = resp.result.expect("result");
        assert!(
            result.get("queue_before_gate").is_some()
                && result.get("handler_after_gate").is_some()
                && result.get("db").is_some()
                && result.get("connections").is_some()
                && result.get("db_pragmas").is_some()
                && result.get("stream_push").is_some()
                && result.get("queue_admission").is_some()
                && result.get("queue_dispatch").is_some()
                && result.get("keyed_requests").is_some()
                && result.get("slow_requests").is_some()
                && result.get("gate_refusals").is_some(),
            "덩어리들이 응답에 있어야 한다: {result}"
        );
        assert!(
            result["queue_admission"].is_null(),
            "주입기가 없는 조립에서 장부 값을 지어냈다: {result}"
        );
        assert!(
            result["stream_push"].is_null(),
            "허브가 주입되지 않은 조립에서 스트림 값을 지어냈다: {result}"
        );
        // 이 조립은 mock 스토어라 되읽은 값이 없다 — `null` 이어야 "잰 적이 없다" 로
        // 읽힌다. 핸들러가 `Core` 대신 아무 값이나 지어내면 여기서 죽는다.
        assert!(
            result["db_pragmas"]["memory_db"].is_null(),
            "스토어가 없는 조립에서 적용값을 지어냈다: {result}"
        );
    }

    /// pragma 덩어리는 DB 마다 요청값·실제값·판정을 **함께** 싣고, 하나라도 안 섰으면
    /// 그 DB 가 `degraded` 다. 열리지 않은 DB 는 `null` 이다.
    #[test]
    fn the_pragma_block_carries_requested_effective_and_degraded_per_database() {
        use tasty_memory::pragma::{AppliedPragmas, PragmaReading};
        let reading = |name, requested: &str, effective: &str, took| PragmaReading {
            name,
            requested: requested.to_string(),
            effective: Some(effective.to_string()),
            error: None,
            took,
        };
        let healthy = AppliedPragmas {
            in_memory: false,
            readings: vec![reading("journal_mode", "WAL", "wal", true)],
        };
        let degraded = AppliedPragmas {
            in_memory: false,
            readings: vec![
                reading("journal_mode", "WAL", "delete", false),
                reading("synchronous", "NORMAL", "NORMAL", true),
            ],
        };

        let v = db_pragmas_json(Some(&healthy), None, Some(&degraded));
        let m = &v["memory_db"];
        assert_eq!(m["degraded"], false);
        assert_eq!(m["in_memory"], false);
        assert_eq!(m["pragmas"]["journal_mode"]["requested"], "WAL");
        assert_eq!(m["pragmas"]["journal_mode"]["effective"], "wal");
        let s = &v["state_db"];
        assert_eq!(
            s["degraded"], true,
            "안 선 pragma 가 있는데 degraded 가 아니다"
        );
        assert_eq!(s["pragmas"]["journal_mode"]["took"], false);
        assert_eq!(
            s["pragmas"]["journal_mode"]["effective"], "delete",
            "실제값이 요청값으로 덮이면 안 된다"
        );
        assert_eq!(s["pragmas"]["synchronous"]["took"], true);

        assert!(
            m["init_failure"].is_null(),
            "대체가 아닌데 초기화 실패를 지어냈다"
        );
        assert!(
            s.get("init_failure").is_none(),
            "state.db 는 대체가 없다 — 칸을 싣지 않는다"
        );

        let none = db_pragmas_json(Some(&healthy), None, None);
        assert!(none["state_db"].is_null(), "열리지 않은 DB 는 null 이다");
    }

    /// `memory.db` 를 못 열어 in-memory 로 대체했으면, pragma 가 다 섰어도 `degraded` 이고
    /// 원인이 실린다. 이것이 없던 때 손상된 DB 로 부팅한 호스트가 `degraded: false` 로 답했다.
    #[test]
    fn a_fallback_memory_store_is_degraded_and_names_its_cause() {
        use tasty_memory::pragma::{AppliedPragmas, PragmaReading};
        let all_took = AppliedPragmas {
            in_memory: true,
            readings: vec![PragmaReading {
                name: "journal_mode",
                requested: "WAL".into(),
                effective: Some("memory".into()),
                error: None,
                took: true,
            }],
        };
        let fallback = tasty_memory::InitFallback {
            cause: "corrupt",
            error: "memory.db corrupted: /x/memory.db".into(),
        };
        let v = db_pragmas_json(Some(&all_took), Some(&fallback), None);
        let m = &v["memory_db"];
        assert_eq!(m["degraded"], true, "대체 저장소가 정상으로 보고됐다: {v}");
        assert_eq!(m["in_memory"], true);
        assert_eq!(m["init_failure"]["cause"], "corrupt");
        assert_eq!(
            m["init_failure"]["error"],
            "memory.db corrupted: /x/memory.db"
        );
        assert_eq!(m["pragmas"]["journal_mode"]["took"], true);
    }

    /// ★ 연결 덩어리가 **`Core` 가 들고 있는 그 게이지**를 읽는다.
    ///
    /// 위 `snapshot_json` 시험들은 인자로 준 스냅샷만 보므로, 핸들러가 `Core` 대신
    /// 새 기본값을 만들어 읽어도 살아남는다. 이것이 그 자리를 잰다 — `Core` 의
    /// 게이지에 자리를 열어 두고 라우터를 지나, 응답의 `live` 가 그 값을 말하는지
    /// 본다. 핸들러가 다른 게이지를 읽으면 0 이 와서 죽는다.
    #[test]
    fn the_connection_block_reads_the_gauge_the_core_hands_to_the_server() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
        // 서버가 하는 일을 그대로 한다 — `Core` 가 건네는 핸들에 자리를 연다.
        let gauge = core.connections().clone();
        assert!(gauge.try_open(4).is_some());
        assert!(gauge.try_open(4).is_some());
        gauge.close();

        let req = tasty_ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "system.pressure".into(),
            params: json!({}),
            session_token: None,
        };
        let resp = super::super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &crate::ipc::caller::CallerContext::Local,
        );
        let c = resp.result.expect("result")["connections"].clone();
        assert_eq!(c["live"], 1, "핸들러가 Core 의 게이지를 읽어야 한다");
        assert_eq!(c["live_max"], 2);
        assert_eq!(c["accepted"], 2);
        assert_eq!(
            c["limit"],
            crate::adapters::production::tcp_ipc_server::MAX_CONCURRENT_CONNECTIONS,
            "상한은 서버가 집행하는 그 상수여야 한다"
        );
    }

    /// ★ 스트림 덩어리가 **엔진에 주입된 그 허브**를 읽는다.
    ///
    /// `stream_push_json` 만 시험하면 핸들러가 새 허브를 만들어 읽어도 살아남는다. 부팅이
    /// 하는 주입(`set_notifier`)을 그대로 하고 라우터를 지나, 그 허브에서 난 손실과
    /// backlog 이 응답에 나오는지 본다.
    #[test]
    fn the_stream_block_reads_the_hub_injected_into_the_engine() {
        use tasty_ipc::stream::{StreamFrame, StreamTag};
        use tasty_ipc::stream_hub::{PushResult, SINK_CAPACITY, StreamHub};

        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
        let hub = StreamHub::new();
        engine.attach.set_notifier(hub.clone());

        let id = hub.alloc_id();
        let rx = hub.register(id);
        for _ in 0..SINK_CAPACITY {
            assert_eq!(
                hub.push(id, StreamFrame::new(StreamTag::Data, b"x".to_vec())),
                PushResult::Sent
            );
        }
        assert_eq!(
            hub.push(id, StreamFrame::new(StreamTag::Data, b"lost".to_vec())),
            PushResult::Dropped
        );
        rx.recv().expect("한 장 꺼낸다");

        let req = tasty_ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "system.pressure".into(),
            params: json!({}),
            session_token: None,
        };
        let resp = super::super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &crate::ipc::caller::CallerContext::Local,
        );
        let s = resp.result.expect("result")["stream_push"].clone();
        assert_eq!(s["frames_dropped"], 1, "주입된 허브의 손실이 아니다: {s}");
        assert_eq!(s["clients_lagged_out"], 0);
        assert_eq!(
            s["backlog"],
            (SINK_CAPACITY - 1) as u64,
            "꺼낸 한 장만큼 내려간 지금의 값이어야 한다: {s}"
        );
        assert_eq!(s["sink_capacity"], SINK_CAPACITY);
    }

    /// 큐의 두 덩어리와 키 실은 요청 덩어리가 원천의 칸을 **자기 이름 그대로** 싣는다.
    ///
    /// 칸마다 다른 값을 넣어, 한 칸이 다른 칸 자리로 새면 대조가 깨지게 한다. 장부가 없으면
    /// `queue_admission` 은 0 이 든 덩어리가 아니라 `null` 이다.
    #[test]
    fn the_queue_and_keyed_blocks_carry_their_sources_slot_by_slot() {
        use tasty_ipc::admission::{AdmissionSnapshot, QueueLimits};
        use tasty_ipc::dispatch::DispatchSnapshot;
        let a = AdmissionSnapshot {
            queued_bytes: 1,
            queued_commands: 2,
            queued_injected: 3,
            peak_bytes: 4,
            refused_bytes: 5,
            refused_depth: 6,
        };
        let limits = QueueLimits {
            queued_bytes: 7,
            injected_depth: 8,
        };
        let q = queue_admission_json(Some((a, limits)));
        for (k, v) in [
            ("queued_bytes", 1),
            ("queued_commands", 2),
            ("queued_injected", 3),
            ("peak_bytes", 4),
            ("refused_bytes", 5),
            ("refused_depth", 6),
            ("limit_bytes", 7),
            ("limit_injected_depth", 8),
        ] {
            assert_eq!(q[k], v, "queue_admission.{k}: {q}");
        }
        assert!(
            queue_admission_json(None).is_null(),
            "장부가 없는데 값을 지어냈다"
        );

        let d = queue_dispatch_json(&DispatchSnapshot {
            rounds: 11,
            rounds_stopped_by_count: 12,
            rounds_stopped_by_time: 13,
            expired_before_run: 14,
            started: 15,
            in_flight: 16,
            in_flight_max: 17,
        });
        for (k, v) in [
            ("rounds", 11),
            ("rounds_stopped_by_count", 12),
            ("rounds_stopped_by_time", 13),
            ("expired_before_run", 14),
            ("started", 15),
            ("in_flight", 16),
            ("in_flight_max", 17),
        ] {
            assert_eq!(d[k], v, "queue_dispatch.{k}: {d}");
        }

        let r = keyed_requests_json(&super::super::idempotency::RetryCounts {
            executed: 21,
            replayed: 22,
            conflicted: 23,
            discarded: 24,
            in_flight: 25,
        });
        for (k, v) in [
            ("executed", 21),
            ("replayed", 22),
            ("conflicted", 23),
            ("discarded", 24),
            ("in_flight", 25),
        ] {
            assert_eq!(r[k], v, "keyed_requests.{k}: {r}");
        }
    }

    /// ★ 새 세 덩어리가 **프로세스가 실제로 세는 그 원천**을 읽는다 — 주입기가 든 입장 장부 ·
    /// `Core` 의 dispatch 누계 · 프로세스 보존소.
    ///
    /// 위 시험은 인자로 준 스냅샷만 보므로 핸들러가 원천 대신 기본값을 읽어도 살아남는다.
    /// 여기서는 부팅이 하는 주입(`set_host_ipc_injector` + 장부)을 그대로 하고, 장부에 자리를
    /// 잡고 · 하나를 거절하고 · 실행 중 표를 하나 들고 · 키 실은 요청을 재생시킨 뒤 라우터를
    /// 지나 읽는다.
    #[test]
    fn the_new_blocks_read_the_ledger_the_dispatch_gauge_and_the_process_store() {
        use tasty_ipc::admission::{CommandAdmission, Origin, QueueLimits};
        use tasty_ipc::host_call::HostIpcInjector;

        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
        let limits = QueueLimits {
            queued_bytes: 50,
            injected_depth: 3,
        };
        let ledger = CommandAdmission::new(limits);
        let (tx, _rx) = std::sync::mpsc::channel();
        core.set_host_ipc_injector(
            HostIpcInjector::new(tx, std::sync::Arc::new(|| {})).with_admission(ledger.clone()),
        );
        let _held = ledger.admit(40, Origin::Socket).expect("빈 큐는 받는다");
        ledger
            .admit(20, Origin::Socket)
            .expect_err("바이트 상한을 넘는다");
        // 주입 깊이 상한(3)을 넘는 주입이 두 번 거절된다 — 바이트 거절(1)과 다른 값이라 두 칸이
        // 뒤바뀌면 갈린다. 표는 이 블록 끝에서 놓아, 아래의 지금 값(`queued_bytes` ·
        // `queued_commands`) 단언은 소켓 한 건만 본다 — 거절 누계는 남는다.
        {
            let _injected: Vec<_> = (0..3)
                .map(|_| ledger.admit(1, Origin::Injected).expect("깊이 상한 안"))
                .collect();
            for _ in 0..2 {
                ledger
                    .admit(1, Origin::Injected)
                    .expect_err("주입 깊이 상한을 넘는다");
            }
        }
        // 루프가 하는 일을 그대로 한다 — 꺼낸 명령을 실행 직전에 집는다. 명령과 기다리는
        // 쪽을 둘 다 들고 있는 동안 그 요청은 실행 중이다(ADR-0412).
        let (reply_tx, _reply_rx) = std::sync::mpsc::sync_channel(1);
        let running = tasty_ipc::server::IpcCommand::new(
            tasty_ipc::protocol::JsonRpcRequest {
                response_timeout_ms: None,
                idempotency_key: None,
                jsonrpc: "2.0".into(),
                id: Some(json!(9)),
                method: "system.info".into(),
                params: json!({}),
                session_token: None,
            },
            reply_tx,
        );
        assert!(crate::app::ipc_round::claim_or_answer(
            &running,
            core.dispatch()
        ));
        core.dispatch()
            .record_round(tasty_ipc::dispatch::RoundEnd::TimeBudget);

        let call = |core: &mut crate::core::Core,
                    state: &mut crate::state::AppState,
                    engine: &mut crate::core::CoreState,
                    method: &str,
                    params: serde_json::Value,
                    key: Option<&str>| {
            let req = tasty_ipc::protocol::JsonRpcRequest {
                response_timeout_ms: None,
                idempotency_key: key.map(Into::into),
                jsonrpc: "2.0".into(),
                id: Some(json!(1)),
                method: method.into(),
                params,
                session_token: None,
            };
            super::super::handle_with_caller(
                core,
                state,
                engine,
                &req,
                &crate::ipc::caller::CallerContext::Local,
            )
        };
        // 같은 키 · 같은 요청 두 번 — 처음은 실행, 다음은 재생이다.
        for _ in 0..2 {
            call(
                &mut core,
                &mut state,
                &mut engine,
                "workspace.create",
                json!({"name": "pressure-keyed-probe"}),
                Some("pressure-keyed-probe"),
            );
        }
        let floor = super::super::idempotency::retry_counts();

        let result = call(
            &mut core,
            &mut state,
            &mut engine,
            "system.pressure",
            json!({}),
            None,
        )
        .result
        .expect("result");
        let a = &result["queue_admission"];
        assert_eq!(a["queued_bytes"], 40, "주입기가 든 장부가 아니다: {a}");
        assert_eq!(a["queued_commands"], 1);
        assert_eq!(a["refused_bytes"], 1, "거절 누계가 장부의 것이 아니다: {a}");
        assert_eq!(
            a["refused_depth"], 2,
            "깊이 거절 누계가 장부의 것이 아니다: {a}"
        );
        assert_eq!(a["queued_injected"], 0, "놓은 주입 표가 남아 있다: {a}");
        assert_eq!(
            a["limit_bytes"], 50,
            "상한은 그 장부가 집행하는 값이어야 한다"
        );
        assert_eq!(a["limit_injected_depth"], 3);
        let d = &result["queue_dispatch"];
        assert_eq!(d["in_flight"], 1, "Core 의 dispatch 누계가 아니다: {d}");
        assert_eq!(d["started"], 1);
        assert_eq!(d["rounds"], 1);
        assert_eq!(d["rounds_stopped_by_time"], 1);
        // 보존소는 전역이고 시험이 병렬로 돌아 다른 시험도 같은 칸을 올린다 — 정확한 값이
        // 아니라 **이 시험이 올린 뒤의 값 이상**을 본다. 누계는 안 내려가므로 이 하한은 선다.
        let k = &result["keyed_requests"];
        assert!(
            floor.replayed >= 1
                && k["replayed"].as_u64() >= Some(floor.replayed)
                && k["executed"].as_u64() >= Some(floor.executed),
            "프로세스 보존소의 판정이 아니다: {floor:?} → {k}"
        );
    }

    /// 분포가 **자기 덩어리 안에** 들어가고, 경계가 값과 같은 자리에 나간다.
    ///
    /// 세 분포를 서로 다른 칸에 떨어지는 값으로 채워, 한 분포가 다른 덩어리로 새면
    /// 대조가 깨지게 한다.
    ///
    /// 칸을 가리킬 때는 서수("몇 번째")를 쓰지 않고 **`counts` 의 첨자**로 적는다 —
    /// 0-기점이고, 이 시험이 단언하는 것과 같은 표기라 기점이 흔들릴 자리가 없다.
    #[test]
    fn each_distribution_ships_inside_its_own_block_with_its_bounds() {
        let p = PressureStats::default();
        // 31_623 초과 100_000 이하 → counts[8]
        p.record_queue_wait(Duration::from_micros(40_000));
        // 10 이하 → counts[0]
        p.record_handler(Duration::from_micros(5));
        let w = PluginWaitStats::default();
        // 마지막 상한(1_000_000) 초과 → 넘침 칸 counts[LATENCY_BUCKET_COUNT - 1]
        w.record(Duration::from_micros(2_000_000));

        let v = snapshot_json(
            &p.snapshot(),
            &w.snapshot(),
            &DbLatencyStats::default().snapshot(),
            &ConnectionStats::default().snapshot(),
        );
        let qh = &v["queue_before_gate"]["wait_us_hist"];
        let hh = &v["handler_after_gate"]["us_hist"];
        let ph = &v["plugin_round_trip"]["us_hist"];

        let bounds = qh["bounds_us"].as_array().expect("경계 배열");
        assert_eq!(
            bounds.len(),
            tasty_telemetry::LATENCY_BUCKET_COUNT - 1,
            "칸이 상한보다 하나 많다 — 그 하나가 넘침이다"
        );
        assert_eq!(
            hh["bounds_us"], qh["bounds_us"],
            "경계는 덩어리마다 같은 값이어야 한다"
        );
        assert_eq!(
            ph["bounds_us"], qh["bounds_us"],
            "경계는 덩어리마다 같은 값이어야 한다"
        );

        let counts = |x: &serde_json::Value| -> Vec<u64> {
            x["counts"]
                .as_array()
                .expect("칸 배열")
                .iter()
                .map(|n| n.as_u64().expect("u64"))
                .collect()
        };
        let (q, h, pl) = (counts(qh), counts(hh), counts(ph));
        assert_eq!(q.iter().sum::<u64>(), 1);
        assert_eq!(h[0], 1, "5 µs 는 counts[0]");
        assert_eq!(q[0], 0, "큐 대기 40 ms 가 counts[0] 에 오면 안 된다");
        assert_eq!(q[8], 1, "40 ms 는 상한 100_000 인 counts[8] 이다");
        assert_eq!(
            pl[tasty_telemetry::LATENCY_BUCKET_COUNT - 1],
            1,
            "2 s 는 넘침 칸이다"
        );
        assert!(
            v["db"].get("us_hist").is_none() && v["connections"].get("us_hist").is_none(),
            "분포가 없는 덩어리에 빈 분포를 넣지 않는다"
        );
    }

    /// accept 대기 상한은 `connections` 덩어리에 자기 기록 수와 함께 나간다 — 모수가 그 덩어리와
    /// 같다(accept 루프가 꺼낸 TCP 연결 전부). 큐 대기(`queue_before_gate`)에는 안 섞인다.
    #[test]
    fn the_accept_wait_bound_ships_in_the_connection_block() {
        let c = ConnectionStats::default();
        c.record_accept_wait(Duration::from_micros(100_100));
        c.record_accept_wait(Duration::from_micros(300));
        let v = snapshot_json(
            &PressureStats::default().snapshot(),
            &PluginWaitStats::default().snapshot(),
            &DbLatencyStats::default().snapshot(),
            &c.snapshot(),
        );
        let k = &v["connections"];
        assert_eq!(k["accept_waits"], 2);
        assert_eq!(k["accept_wait_bound_us_sum"], 100_400);
        assert_eq!(k["accept_wait_bound_us_max"], 100_100);
        assert_eq!(k["accept_wait_bound_us_mean"], 50_200);
        assert_eq!(
            v["queue_before_gate"]["wait_us_sum"], 0,
            "큐 대기에 섞이면 안 된다"
        );
        let unobserved = snapshot_json(
            &PressureStats::default().snapshot(),
            &PluginWaitStats::default().snapshot(),
            &DbLatencyStats::default().snapshot(),
            &ConnectionStats::default().snapshot(),
        );
        assert!(unobserved["connections"]["accept_wait_bound_us_mean"].is_null());
    }

    /// 자원 축이 시간 축과 **섞이지 않는다.** 연결이 꽉 차 거절이 나도 handler 는
    /// 한 번도 안 돌 수 있다 — 그 둘이 한 덩어리에 있으면 "느리다" 와 "자리가
    /// 없다" 가 같은 수로 보인다.
    #[test]
    fn a_saturated_port_is_not_a_slow_handler() {
        let c = ConnectionStats::default();
        assert!(c.try_open(1).is_some());
        assert!(c.try_open(1).is_none());

        let v = snapshot_json(
            &PressureStats::default().snapshot(),
            &PluginWaitStats::default().snapshot(),
            &DbLatencyStats::default().snapshot(),
            &c.snapshot(),
        );
        assert_eq!(v["connections"]["live"], 1);
        assert_eq!(v["connections"]["refused_saturated"], 1);
        assert_eq!(
            v["handler_after_gate"]["calls"], 0,
            "거절된 연결은 요청이 된 적이 없다"
        );
        assert!(
            v["handler_after_gate"].get("refused_saturated").is_none()
                && v["connections"].get("calls").is_none(),
            "자원 모수가 시간 덩어리에 섞이면 안 된다"
        );
    }

    /// plugin 은 못 부른다 — `local_only` 판정이 표에서 온다.
    ///
    /// 권한을 하나도 안 준 agent 로 부른다. 이 게이지는 caller 로 나누지 않으므로
    /// 어떤 권한을 준다고 열리는 값이 아니다(그것이 `local_only` 인 이유다).
    #[test]
    fn a_plugin_caller_is_refused_this_gauge() {
        let meta = tasty_ipc::method_meta::method_meta("system.pressure")
            .expect("표에 등재돼 있어야 한다 — 없으면 거부가 정책인지 누락인지 갈리지 않는다");
        assert!(
            !meta.plugin_callable,
            "프로세스 게이지는 caller 별 값이 아니라 plugin 표면이 아니다"
        );
        assert!(!meta.plugin_only, "local 은 부를 수 있어야 한다");
    }

    /// 첫 조회는 자기 회차 안에서 답한다 — 명령을 꺼내 대기는 기록했지만 회차는 아직 안 끝났다.
    /// 그 답에서도 평균은 최댓값을 안 넘고, 분포의 합이 `waits` 와 같다(ADR-0466).
    #[test]
    fn a_query_inside_an_open_round_keeps_the_mean_under_the_max() {
        let p = PressureStats::default();
        p.record_queue_wait(Duration::from_micros(207_972));
        p.record_queue_wait(Duration::from_micros(327));
        let v = snapshot_json(
            &p.snapshot(),
            &PluginWaitStats::default().snapshot(),
            &DbLatencyStats::default().snapshot(),
            &ConnectionStats::default().snapshot(),
        );
        let q = &v["queue_before_gate"];
        assert_eq!(q["commands"], 0, "대조군: 회차가 아직 안 끝났다");
        let mean = q["wait_us_mean"]
            .as_u64()
            .expect("대기를 쟀으니 평균이 있다");
        let max = q["wait_us_max"].as_u64().expect("최댓값");
        assert!(mean <= max, "평균 {mean} 이 최댓값 {max} 을 넘었다");
        let hist_total: u64 = q["wait_us_hist"]["counts"]
            .as_array()
            .expect("counts")
            .iter()
            .map(|c| c.as_u64().expect("정수"))
            .sum();
        assert_eq!(
            q["waits"].as_u64(),
            Some(hist_total),
            "분포의 합이 평균의 분모다"
        );
    }

    /// 관측이 없으면 평균은 `null` 이다 — 0 이 아니다.
    #[test]
    fn an_unobserved_average_is_null_not_zero() {
        let v = snapshot_json(
            &PressureStats::default().snapshot(),
            &PluginWaitStats::default().snapshot(),
            &DbLatencyStats::default().snapshot(),
            &ConnectionStats::default().snapshot(),
        );
        assert!(v["queue_before_gate"]["wait_us_mean"].is_null());
        assert!(v["queue_before_gate"]["depth_mean"].is_null());
        assert!(v["handler_after_gate"]["us_mean"].is_null());
        assert!(v["db"]["commit_us_mean"].is_null());
        assert!(v["db"]["checkpoint_us_mean"].is_null());
        assert_eq!(
            v["queue_before_gate"]["wait_us_sum"], 0,
            "합은 0 이 맞다 — null 인 것은 평균뿐이다"
        );
    }

    /// 느린 요청 줄은 호스트 몫과 hop 을 **칸마다 제자리에** 싣고, 싣지 않기로 한 것(params ·
    /// 토큰 · RPC id)은 줄에 없다.
    #[test]
    fn the_slow_request_block_carries_each_row_slot_by_slot() {
        use tasty_telemetry::slow_requests::{CallerKind, HopOutcome, HostLeg, PluginHop};
        let log = tasty_telemetry::SlowRequestLog::default();
        log.note_forwarded(42);
        log.finish_host(HostLeg {
            request_seq: 42,
            method: "orig.run",
            caller: CallerKind::Local,
            queue_wait: Duration::from_micros(300),
            host: Duration::from_micros(20),
            outcome: Default::default(),
        });
        log.finish_plugin_hop(
            42,
            PluginHop {
                plugin_id: "com.example.owner".into(),
                host_request_id: 9001,
                wait_us: 250_000,
                outcome: HopOutcome::Expired,
            },
            true,
        );
        let v = slow_requests_json(&log.snapshot());
        assert_eq!(v["threshold_us"], 100_000);
        assert_eq!(v["capacity"], 32);
        assert_eq!(v["admitted"], 1);
        let row = &v["rows"][0];
        assert_eq!(row["request_seq"], 42);
        assert_eq!(row["total_us"], 250_320);
        assert_eq!(
            row["host"],
            json!({"method": "orig.run", "caller": "local", "queue_wait_us": 300, "host_us": 20,
                   "outcome": null, "error_code": null})
        );
        assert_eq!(
            row["plugin_hops"],
            json!([{"plugin_id": "com.example.owner", "host_request_id": 9001,
                    "wait_us": 250_000, "outcome": "expired"}])
        );
        let mut keys: Vec<&str> = row
            .as_object()
            .expect("row")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["host", "plugin_hops", "request_seq", "total_us"]);
    }

    /// 호스트 몫의 결과는 **읽는 순간의** 칸 값이다 — 줄이 들어간 뒤 답이 나가도 보이고, 성공 ·
    /// 거절 · 실행 전 만료가 링만으로 갈린다.
    #[test]
    fn the_host_part_carries_the_answer_the_caller_got() {
        use tasty_telemetry::slow_requests::{CallerKind, HostLeg, HostOutcome, HostOutcomeCell};
        let log = tasty_telemetry::SlowRequestLog::default();
        let cells: Vec<HostOutcomeCell> = (0..3).map(|_| HostOutcomeCell::default()).collect();
        for (seq, cell) in (1..).zip(&cells) {
            log.finish_host(HostLeg {
                request_seq: seq,
                method: "workspace.list",
                caller: CallerKind::Local,
                queue_wait: Duration::from_millis(150),
                host: Duration::ZERO,
                outcome: cell.clone(),
            });
        }
        cells[0].set(HostOutcome::Ok);
        cells[1].set(HostOutcome::Error { code: -32001 });
        cells[2].set(HostOutcome::Error { code: -32067 });
        cells[2].set(HostOutcome::Ok);
        let v = slow_requests_json(&log.snapshot());
        let seen: Vec<(serde_json::Value, serde_json::Value)> = (0..3)
            .map(|i| {
                let host = &v["rows"][i]["host"];
                (host["outcome"].clone(), host["error_code"].clone())
            })
            .collect();
        assert_eq!(
            seen,
            [
                (json!("ok"), json!(null)),
                (json!("error"), json!(-32001)),
                (json!("error"), json!(-32067)),
            ],
            "두 번째 답은 첫 답을 덮지 않는다"
        );
    }

    /// 게이트 덩어리는 원천의 칸을 **이름 그대로** 옮긴다 — 값을 전부 다르게 골라 두 칸이 맞바뀌면
    /// 대조가 깨지게 한다.
    #[test]
    fn the_gate_block_carries_its_source_slot_by_slot() {
        let v = gate_refusals_json(&tasty_telemetry::GateSnapshot {
            judged: 17,
            permission_denied: 2,
            cap_blocked: 3,
            throttled: 5,
        });
        assert_eq!(
            v,
            json!({"judged": 17, "permission_denied": 2, "cap_blocked": 3, "throttled": 5})
        );
    }

    /// 라우터가 `Core` 가 든 **그 링**을 읽는다 — 핸들러가 빈 링을 새로 세우면 여기서 죽는다.
    #[test]
    fn the_router_reads_the_ring_the_core_holds() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
        core.slow_requests()
            .finish_host(tasty_telemetry::slow_requests::HostLeg {
                request_seq: 77,
                method: "workspace.list",
                caller: tasty_telemetry::slow_requests::CallerKind::Local,
                queue_wait: Duration::from_millis(150),
                host: Duration::ZERO,
                outcome: Default::default(),
            });
        let req = tasty_ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "system.pressure".into(),
            params: json!({}),
            session_token: None,
        };
        let resp = super::super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &crate::ipc::caller::CallerContext::Local,
        );
        let result = resp.result.expect("result");
        assert_eq!(
            result["slow_requests"]["rows"][0]["request_seq"], 77,
            "{result}"
        );
    }
}
