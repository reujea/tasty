# IPC 서버

## 원칙과 현재 동작

현재 Local 전용 진단·호출 권한 제한은 구현 계약이다. [사용자 통제 원칙](../principles/control-and-data.md)과 [플러그인 원칙](../principles/plugins.md)에 따라 포커스와 무관한 추가 보호 제한을 정합해야 하며, 다른 호출자의 부하가 보인다는 사실을 영구 비공개 원칙으로 삼지 않는다.

호스트 IPC의 수신, 큐 처리, 응답 대기와 진단 방법을 설명한다. 메서드 이름·오류 코드·멱등 키의 공개 계약은 [API 규약](../dev-guide/api-conventions.md)을 따른다.

## 연결과 신뢰 범위

로컬 IPC는 127.0.0.1의 동적 TCP 포트와 JSON-RPC를 사용하고 포트 파일로 접속 위치를 알린다. 단일 사용자 데스크톱을 대상으로 OS별 socket·pipe 구현을 두지 않는다. loopback 자체는 같은 머신의 다른 OS 사용자를 인증하거나 차단하지 않는다. 원격 접속은 SSH에 위임한다. agent 세션 토큰은 요청이 어느 agent 의 것인지 구분하는 표지이며 인증 수단이 아니다. 토큰 없는 Local 연결은 사용자로 취급하고, 세션 권한 검사는 협조적인 agent 의 실수를 막는다([플러그인 권한 모델](../dev-guide/plugin-permissions.md)). 공유 머신·다중 사용자 서비스를 지원하려면 Unix socket 권한이나 Windows named pipe ACL을 다시 검토한다.

## 입장 상한

전송 줄 상한은 `tasty_ipc::protocol::MAX_REQUEST_LINE_BYTES`가 정의한다. Journal 명령 입력·프리셋 캡처·로컬 스크립트 읽기는 각 소유자의 별도 정책 상수를 사용하며, 모두 현재 8 MiB다. 전송 상한을 바꿔도 이 저장·읽기 예산은 바뀌지 않는다.

일반 요청 한 줄은 8 MiB, 동시 연결은 256으로 제한한다. 개행 전 메모리 무한 증가와 연결별 스레드 무한 증가를 막기 위해 파싱 전·스레드 생성 전에 각각 제한한다. 연결 자리는 RAII로 반환하고 긴 줄은 나머지를 다른 요청으로 해석하지 않도록 연결을 닫는다. 8 MiB는 기본 memory 항목 1 MiB의 최악 JSON escape와 봉투를 수용하지만 사용자가 memory 상한을 올려도 자동으로 늘지 않는다. 정상 부하의 연결 포화와 큰 memory 설정을 함께 관측해 제한값을 재검토한다.

명령 큐는 소켓·내부 주입이 공유하는 CommandAdmission으로 64 MiB 대기 바이트와 내부 주입 256건을 제한한다.
소켓 요청은 trim한 원문 JSON 길이, 내부 요청은 compact 직렬화 길이를 세며 실제 heap 크기는 아니다.
빈 큐의 한 건 예외를 허용하고 dequeue 즉시 사용량을 반환한다.
소켓 포화는 -32065 미실행 후 연결 유지, 내부 포화는 InjectError::Refused다.
webhook은 로그 후 다음 단계로 가며 runner는 task 결과로 전달한다.
지연된 Enter 재주입은 사용자 입력을 잘못 제출할 수 있어 재시도하지 않는다.
debug 전용 제한 축소로 포화를 재현할 수 있다.
값은 정상 부하 실측으로 보정된 값이 아니며 peak와 거절률로 재검토한다.

## 첫 요청과 응답 쓰기

연결 직후 첫 요청 줄 전체에는 attach heartbeat와 별개인 20초 기한을 둔다. 읽을 때마다 남은 시간을 적용해 한 바이트씩 보내는 연결도 기한을 연장하지 못한다. 만료는 id:null -32066 미실행 응답을 시도한 뒤 닫는다. 첫 줄 후 read timeout을 해제하고 요청 사이 idle에는 제한하지 않아 long-poll과 제3자 장수 연결을 보존한다. 첫 요청 후 쉬는 연결이 자리를 점유하는 한계는 남는다.

request-response 쓰기 timeout은 attach heartbeat와 별개인 20초이고 stream 업그레이드 판정 뒤에만 적용한다.
부분 전송 뒤 이어 쓰면 JSON 경계가 깨질 수 있어 parse 오류 응답을 포함한 모든 쓰기 실패에서 연결을 닫는다.
긴 요청 줄은 id:null의 -32060 후 닫고, 연결 포화는 accept를 막지 않는 nonblocking 1회 쓰기로 -32062를 최선 노력 전송한 뒤 닫는다.
읽지 않은 수신 데이터가 남은 소켓을 닫으면 Linux와 Windows 모두 RST를 보낸다. 차이는 받는 쪽이다. Linux 클라이언트는 RST 전에 도착한 응답을 먼저 읽은 뒤 연결 재설정을 받고, Windows 클라이언트는 아직 읽지 않은 응답까지 버리고 `WSAECONNRESET`을 받는다. 그래서 두 거절 모두 응답을 쓴 뒤 쓰기 쪽을 먼저 닫고(FIN) 클라이언트가 이미 보낸 바이트를 읽어 버린 뒤 닫는다.
-32060(연결 스레드)은 모든 OS에서 이 단계를 둔다. Windows와 macOS는 기다리며 읽고, 50 ms 동안 새 바이트가 없거나 0.5초 또는 8 MiB(요청 줄 상한)에 이르면 멈춘다. macOS CI에서도 요청을 보내던 클라이언트가 거절 응답 대신 `ECONNRESET`을 받은 적이 있어 Windows와 같은 경로를 쓴다. Linux는 기다리지 않고 이미 도착한 바이트만 non-blocking으로 읽으며, 읽을 것이 없거나 8 MiB에 이르면 멈춘다. Linux 클라이언트는 응답을 읽을 수 있으므로 요청을 다 보낸 클라이언트의 연결 재설정만 막으면 된다.
-32062(accept 스레드)는 모든 OS에서 이 단계를 두되 기다리지 않는다. 이미 도착한 바이트만 non-blocking으로 읽고, 읽을 것이 없거나 1 MiB에 이르면 멈춘다. 과부하 때 accept 스레드에서 도는 경로라 지연을 줄이려고 요청 줄 상한(8 MiB)보다 작게 둔다. 닫은 뒤 도착한 바이트는 여전히 RST를 부를 수 있다.
모든 OS에서 두는 근거는 원인이 OS 공통이라는 것이다. 비우지 않던 때 Linux 서버도 같은 조건에서 RST를 보냈고(Linux 클라이언트는 응답 뒤 연결 재설정을 받았다), 다른 것은 클라이언트가 응답을 버리느냐뿐이다.
두 경로 모두 상한을 넘는 요청은 최선 노력 범위 밖이다. 상한에 이르면 그대로 닫고 RST를 받아들인다.
거절 응답을 못 받았다고 포화가 아니라고 단정하지 않는다.
stream client는 이 JSON을 프레임으로 읽어 사유를 해석하지 못할 수 있다.
전송 오류 코드마다 미실행인지 결과 불명인지 구분해야 한다.

## 플러그인 채널의 상한

플러그인 request·response·event 채널은 각 1024개로 제한한다.
호스트에서 plugin으로 보내는 요청은 main thread를 멈추지 않도록 try_send로 거절하고 Full·OverBytes·Disconnected를 구분한다.
plugin에서 호스트로 오는 응답·event는 reader thread를 대기시켜 유실을 막는다.
Hello나 완료 응답을 버리면 등록·수명 관리가 깨진다.
렌더 데이터는 공유메모리에 최신 프레임을 덮어쓰고 dirty 영역을 합친다.
set_context는 키·마우스 입력도 나르므로 단순 최신값 병합을 하지 않는다.
요청 FIFO에서 create가 첫 context보다 앞서야 한다.
느린 plugin의 reader가 막히면 다른 응답도 지연될 수 있어 만료율을 함께 관측한다.

거절된 호스트 요청을 채널이 보관하거나 자동 재전송하지는 않는다.
`set_context`가 거절되면 그 요청에 담긴 키·마우스 입력을 잃을 수 있다.
첫 화면을 요청하는 bootstrap이 거절된 경우에도 전송 기록은 남으므로,
새 입력·크기·테마 변경 등 다음 전송 계기가 생길 때까지 빈 화면이 남을 수 있다.
프레임이 한 번도 오지 않은 상태에서는 bootstrap 표시가 스스로 해제되지 않는다.
`ipc.result`가 거절되면 호출한 플러그인이 결과를 받지 못하고,
`shutdown`이 거절되면 정상 종료 요청을 받지 못한 채 기한 뒤 강제로 종료될 수 있다.

plugin 채널은 큐별 16 MiB·프로세스 합계 64 MiB의 직렬화 바이트를 공용 장부에서 제한한다.
빈 큐는 큰 메시지 한 건을 허용해 소비할 것이 없는 상태에서 대기가 영원히 지속되지 않게 한다.
따라서 엄밀한 RSS 상한은 아니며 plugin 한 메시지 크기 상한도 별도 문제다.
큐 receiver가 없어지면 잔여 바이트를 반환한다.

ping·shutdown은 다른 plugin의 포화로 재시작·강제종료되지 않도록 합계 입장 제한만 면제한다.
큐별 바이트·개수 제한과 실제 사용량 집계는 유지한다.
합계 제한은 plugin 간 영향을 만들 수 있고 main thread의 직렬화 비용도 관측해야 한다.

호스트가 포화로 버린 요청 수는 다음으로 실제 전달되는 PluginRequest의 dropped_requests 델타에 넣는다.
알린 수만 차감하고 동시 증가분은 다음 요청에 남긴다.
별도 통지도 같은 큐에서 막히므로 새 메시지나 무제한 재전송 큐를 만들지 않는다.
SDK는 분기 전에 누계·warn을 갱신하고 worker 요청에는 callback을 제공한다.

이 값은 작업량을 줄이기 위한 신호이며, 유실된 요청이나 입력을 복구하지 않는다.
통지는 다음 전달까지 늦어질 수 있고 소비가 완전히 멈춘 plugin은 healthcheck가 처리한다.

## dispatch 회차 예산

정상 dispatch 회차는 연결 상한에서 파생한 최대 256명령까지만 처리한다. TCP는 한 연결에서 응답 전 다음 요청을 넣지 않아 연결당 대기 요청이 하나지만 내부 주입은 별도로 제한한다. 종료 시 남은 요청을 거절하는 drain에는 정상 회차 제한을 그대로 적용하지 않는다. 명령 수와 시간 예산은 이미 실행 중인 동기 handler를 선점하지 못한다.

GUI와 헤드리스는 IpcRound로 256명령 또는 16ms 중 먼저 닿는 제한까지 처리한다. 한 건씩 꺼내 실행해 나머지는 큐와 바이트 장부에 남기고, 첫 명령은 항상 한 번 처리한다. FIFO는 실행 시작 순서를 정하며 비동기 완료 순서는 달라질 수 있다. 호출자별 공정성은 보장하지 않고 여러 연결을 가진 caller는 더 많은 자리를 차지할 수 있다. 회차 제한은 16ms와 마지막 한 handler의 실행 시간까지 늘어날 수 있다. 시간 중단률·실제 렌더 및 timer 진행·caller별 기아 요구로 다시 검토한다.

## 기한

응답 대기 상한은 호출자가 response_timeout_ms로 정한다. 없거나 0이면 무한 대기를 유지해 승인·task await의 의도적 대기를 자르지 않는다. 시작 뒤 만료는 -32061 결과 불명이며 취소가 아니다. 만료해도 작업은 계속될 수 있고 연결은 유지한다. 늦은 응답은 사라진 전용 수신자에게 전달 실패해 다음 요청과 섞이지 않는다. capability ipc.response-timeout으로 지원을 확인한다. 연결 끊김을 작업 취소로 해석하지 않으며 소켓 생존 감지를 통한 대기 회수는 별도 검토 대상이다.

세션 토큰이 없는 요청의 선택 필드 caller_agent_id는 호출자가 스스로 밝힌 agent ID다. CLI는 자기 환경의 TASTY_AGENT_ID를 싣는다. 호스트는 이 값을 텔레메트리·감사·헤드리스 PTY 소유자 표시에만 쓰고 권한 판단에는 쓰지 않는다. 형식이 틀리면 버리고 요청은 처리한다. 세션 토큰이 있으면 무시한다. capability ipc.caller-agent-id로 지원을 알린다([플러그인 권한 모델](../dev-guide/plugin-permissions.md)).

요청은 QUEUED에서 STARTED 또는 WITHDRAWN으로 한 번만 전환한다. dequeue와 응답 대기 timeout이 공유 lifecycle을 원자적으로 비교·교환해 실행 시작 여부를 결정한다. 큐에서 기한이 지나면 권한·rate·audit 전에 -32067 미실행으로 답하고 나중에도 실행하지 않는다. 시작 뒤 만료만 -32061이다. ipc.response-timeout.not-run capability로 이를 알린다. 미실행 만료도 dequeue 전까지는 큐 장부에 남는다. timeout과 연결 종료는 실행 중 작업의 취소를 뜻하지 않는다.

HostIpcInjector::dispatch는 자신의 대기 시간을 deadline으로 전달하며 1ms 미만도 1ms로 올린다. 큐에서 포기한 요청은 Expired(nothing_ran), 시작한 뒤 만료는 Timeout(결과 불명)이다. tell의 Enter 재주입과 runner는 이 기본 경로를 쓴다. 이미 발생한 사건을 늦게라도 반영해야 하는 execute_sequence의 모든 훅 단계만 dispatch_even_if_abandoned로 deadline 없이 남긴다. 시작 직전 경합과 시작 뒤 실행은 여전히 취소할 수 없다. 새 주입자는 이 차이를 명시적으로 선택해야 한다.

release IPC가 실패할 수 있는 작업을 winit에 예약하면 완료 채널로 작업 결과를 요청자에게 돌려준다. window.create/view.create는 생성된 window_id 또는 -32000 원인을 반환하고 에이전트 실패를 사용자 toast로 알리지 않는다. 메인 루프에서 결과를 기다리면 예약된 작업도 실행할 수 없으므로 응답은 나중에 전달한다. 채널이 결과 없이 닫히면 disconnect다. debug 작업이나 렌더·worker 경로는 각 완료 위치에 맞는 별도 계약을 판단한다. 새 실패 가능 예약 작업도 예약 성공만으로 완료를 가장하지 않는다.

## wake

GUI의 IpcReady는 직전 회차 종료에서 한 시간 예산이 지나지 않았으면 about_to_wait에 차례를 넘긴다. 예산으로 중단한 회차는 스스로 다시 깨우고 about_to_wait 사이 한 번만 양보를 건너뛴다. 무제한 재깨움은 사용자 이벤트를 끝없이 이어 timer·render를 다시 굶기므로 제한한다. 사용자 이벤트 경로를 완전히 없애지는 않아 OS 모달 루프 가능성에 대비한다. winit event 처리 방식 변경, macOS·Windows 메뉴·resize 중 지연은 실제 환경에서 재검토한다. 순수 pacer 테스트가 실제 GUI 루프의 모든 호출 위치를 보장하지는 않는다.

헤드리스 이벤트 채널에는 IpcReady를 최대 하나만 대기시킨다. 소비 시 회차 전에 공용 gate를 해제하고, 회차 뒤 queued_commands가 남으면 채널 끝에 wake를 다시 넣어 PTY·plugin event가 먼저 처리되게 한다. 빈 큐와 종료 회차는 다시 깨우지 않는다. 이 방식은 요청 수만큼 처리 완료 wake가 쌓여 다른 이벤트를 늦추는 문제를 막는다. 새 producer가 admission을 우회하거나 새로운 wake 소비자가 생기면 gate·잔여 판정을 함께 확인한다. 루프는 일반 timer와 plugin deadline을 모두 처리한다.

## 요청 압력 게이지

요청 압력은 고정 크기 프로세스 메모리 집계로 유지하고 요청마다 DB에 기록하거나 caller별 시계열을 만들지 않는다. 거절된 요청도 큐 대기를 소비했으므로 대기·깊이는 게이트 전에, handler 시간은 통과한 요청에서만 측정한다. caller별 telemetry와 Allow audit은 허용 요청에 한 번 기록하는 기존 계약을 유지한다. 그 기록은 cap·이상 탐지 입력이며 별도 token bucket의 rate-limit과 동일하지 않다. 누계 snapshot의 원자 필드들은 정확히 같은 순간을 보장하지 않는다.

system.pressure와 tasty list pressure로 프로세스 진단을 읽는다. Local 전용이며 다른 caller의 부하를 나눌 수 없는 값을 plugin에 공개하지 않는다. queue_before_gate·handler_after_gate·plugin_round_trip·db는 서로 다른 요청 집합을 측정하므로 큐 수에서 handler 수를 빼 거절 수로 읽지 않는다. 관측 없는 평균은 null이다. DB 진단 핸들은 저장소가 생성해 Core에 공유하므로 진단이 DB mutex를 기다리지 않고 부팅 checkpoint도 포함한다. snapshot은 여러 시점의 필드를 포함할 수 있다.

## 연결 수와 시간 분포

connections는 요청을 보내지 않은 TCP·attach·mesh 연결도 live/live_max/limit/accepted/refused_saturated로 센다. 로그를 반복 억제해도 거절 누계는 줄이지 않는다. 시간 분포는 큐·handler·plugin 왕복마다 bounds_us와 서로 겹치지 않는 counts를 함께 제공한다. 10µs~1s의 반십진 경계와 넘침 bucket으로 비교 가능성을 유지하고 정밀도를 가장하는 p50/p99 값은 계산하지 않는다. DB에는 이 histogram을 제공하지 않는다. 양 끝 bucket에 관측이 몰리거나 정상 사용에서 연결 거절이 늘면 경계·상한을 다시 측정한다.

connections의 accept_wait_bound는 accept 큐를 마지막으로 비어 있다고 본 시점부터 연결을 꺼낼 때까지의 시간이다. 실제 대기시간보다 크거나 같은 상한이며 연결 도착 시각을 직접 잰 값은 아니다. 허용·포화 거절 연결 모두 포함해 accept_waits는 accepted+refused_saturated와 대응한다. 상한의 절반을 실제 지연으로 추정하거나 histogram을 붙이지 않는다. readiness 또는 blocking accept로 바뀌면 이 측정 정의도 다시 검토한다.

큐 대기 평균은 wait_us_sum/waits다. waits는 대기를 기록할 때 증가하고 commands는 회차가 끝날 때 증가하므로 둘은 진행 중 회차에서 다를 수 있다. depth_mean은 기존 commands/drains 의미를 유지한다. waits와 commands의 차이를 미실행 요청 수로 해석하지 않는다. histogram counts 합과 waits를 함께 확인해 집계 위치 변경을 검증한다.

## 큐와 재시도 집계

queue_dispatch의 in_flight는 실행을 시작했고 명령 또는 응답 대기자가 lifecycle을 보유하고 있는 요청 수다. worker로 넘긴 응답 대기도 포함하지만 timeout 뒤에도 계속되는 고아 작업의 전체 수는 아니다. started와 in_flight_max도 제공한다. queue_admission은 주입기가 서버와 공유하는 장부를 읽으며 장부 없는 조립에서는 null이다. 큐 사용량과 실행 누계는 CommandQueueSnapshot에서 함께 읽되 각 필드의 의미를 다시 정의하지 않는다.

멱등 재시도 집계는 판정을 내리는 Store의 잠금 안에서 executed·replayed·conflicted·discarded·in_flight를 올린다. 층마다 별도 카운터를 두지 않는다. 담당하지 않는 층이 열었다 닫은 항목의 executed는 되돌려 실제 담당 층에서 한 번만 센다. in_flight는 현재 수가 아니라 진행 중 요청에 합류한 누적 횟수다. 메서드·caller ID별 label은 두지 않는다.

queue_admission·queue_dispatch·keyed_requests는 입장·실행·재시도 판정이라는 다른 대상을 별도 객체로 제공한다. 원천 snapshot의 필드 이름을 유지하고 제한값도 함께 싣는다. snapshot을 모든 필드 이름으로 분해해 추가 필드가 노출 코드에서 조용히 누락되지 않게 한다. queue_dispatch.in_flight는 현재 요청 수, keyed_requests.in_flight는 합류 누계이므로 같은 값으로 합치지 않는다. 장부·저장소 조회에는 잠금이 있을 수 있으며 전체 응답의 동일 시점 원자성을 보장하지 않는다.

## 느린 요청 추적

호스트는 IpcCommand 생성 때 프로세스 단조 RequestSeq를 부여한다.
RPC id·event trace·멱등 키와 구분하고 relay 사본은 같은 번호를 유지한다.
slow_requests는 큐+호스트+plugin 대기 합계가 100ms 이상인 최근 32건을 메모리에 보관하며 열린 forward 최대 256건·hop 최대 3개·메서드명 최대 128바이트를 둔다.
조회 자체는 링을 밀어내지 않는다.
원문 params·토큰·멱등 키·RPC id는 저장하지 않는다.
plugin hop은 원 번호와 host_request_id로 연결하지만 plugin→host 부모를 추측하지 않는다.
intent 뒤 파일 핸들러 forward와 큐를 안 지난 host-call은 연계되지 않는다.
IPC 명령 하나가 plugin namespace 메서드로 오면 plugin 전달은 한 번이고, 그 대기 항목이 명령의 RequestSeq를 든다.
각 dispatch의 `namespace_forward_tests`가 namespace를 소유한 stub plugin으로 전달 수와 번호를 확인하지만 범위가 다르다.
GUI 단위 시험은 namespace 단계 함수(`forward_owned_namespace`) 안의 전달만 잰다. GUI App은 시험에서 만들 수 없어 `ipc_step_routing`에서 그 함수 밖에 둔 전달은 단위 시험이 덮지 않는다.
그 전달은 실제 인스턴스 e2e `a_plugin_namespace_call_is_forwarded_to_the_plugin_exactly_once`(`tests/e2e_tests.rs`)가 덮는다. `markdown.recent` 하나를 보낸 전후로 `system.pressure`의 `plugin_round_trip.matched`(응답이 매칭된 plugin 왕복 수)가 멈출 때까지 기다려 차분이 1인지 본다. `ipc_step_routing`에서 그 함수 밖에 추가 전달을 넣은 변이는 기본(gui) 조합에서 이 시험을 실패시켰다(차분 2).
세는 것은 응답이 매칭된 왕복이라, tasty-host-plugin 내부에서 pending 없이 보낸 중복은 이 e2e가 검출하지 못하고 stub 단위 시험(`namespace_forward_tests`)이 검출한다.
헤드리스 시험은 `dispatch_command` 전체를 지나므로 구조상 그 단계 밖의 전달도 드러나지만, 그 위치의 변이는 재지 않았다. 헤드리스 쪽은 `--no-default-features` 조합에서만 실행된다.
이 기록은 재시작하면 사라진다. 지연 원인을 볼 때 전체 histogram과 함께 확인한다.

느린 요청의 host.outcome과 error_code는 응답 대기자가 실제로 받은 결과를 한 번 기록하는 공유 상태에서 읽는다. dispatch 끝에서 값을 복사하면 plugin의 늦은 답과 timeout 결과가 빠지므로 OnceLock 참조를 링에 둔다. 아직 결과가 없거나 기한 없는 내부 대기가 포기하면 null일 수 있다. 같은 행을 나중에 읽으면 null이 결과로 채워질 수 있지만 채운 값은 바꾸지 않는다. 새 대기 경로는 record_answer를 호출해야 한다.

## 진입 검사 거절 집계

gate_refusals는 요청 게이트의 판정 단계(`handler/gate.rs`)에 들어온 judged와 permission_denied·cap_blocked·throttled를 프로세스 누계로 제공한다.
앞 검사에서 거절되면 뒤 검사는 실행하지 않아 요청당 거절은 최대 하나다.
permission_denied에는 UnknownMethod·NotPluginCallable도 포함돼 모두 권한 요청으로 해결되지는 않는다.
토큰 해석 실패·엔진 없는 경로·키 길이 검사·handler 내부 오류는 이 세 거절 수와 다르다.
plugin host-call도 포함되므로 큐 요청 수와 직접 빼지 않는다.
caller별 값이 필요하면 별도의 개인정보·권한 범위를 검토한다.

## 진단값을 읽을 때

`tasty list pressure`의 값은 대부분 프로세스 시작 이후 누계다. 두 번 조회한 차이를 보면 현재 부하를 비교할 수 있다. `db_pragmas`는 연결을 열 때의 상태이며 누계가 아니다. 일부 값은 원자 변수에서, 큐·재시도 정보는 각 저장소 잠금 안에서 읽는다. 응답 전체가 같은 순간의 상태라는 보장은 없다.

정상 사용 중 상한 거절이 늘면 값만 올리지 말고 요청 크기, plugin별 적체, handler 지연을 함께 확인한다. 반대로 관측값이 0이라는 이유만으로 호출 경로의 계측이 연결됐다고 단정하지 않는다. 실제 요청을 보냈을 때 해당 누계가 증가하는지 확인한다.

## 관련

- 전송 설계
- 실행 순서와 기한
- 진단 설계
- [데이터 흐름](data-flows.md)
- [저장소 상태](../design/systems/storage.md)

구조 journal의 입장 거절은 worker의 오류 타입으로 응답 코드를 정한다. 대기열/저장 예산 부족은 `-32065`, 멱등 키 충돌은 `-32063`이며 진단 문자열의 단어로 판별하지 않는다. 다른 명령이 보유한 disk credit만 부족하면 기존 64개 입장 한도 안에서 기다리고, 실제 저장 예산이 부족하면 거절한다.

journal worker 대기열의 바이트 예산은 요청의 compact JSON 길이로 센다. 이 계산은 App 스레드에서
제출마다 실행되므로 직렬화 버퍼를 만들지 않고 길이만 세며, 예산(`QUEUED_BYTES_LIMIT`)을 넘으면 그
자리에서 멈춘다. 예산 이하의 값은 실제 직렬화 길이와 같다.
한계: 길이를 세는 시간은 직렬화 출력 길이에 비례하며(예산을 넘으면 예산까지), 머신 부하에 따라 크게 변한다.
Linux release 빌드(20코어 공유 머신)의 측정값은 다음과 같다. 출력 16 MiB(이스케이프 없는 입력 문자열 16 MiB)는
7–9 ms였다. 예산(64 MiB 출력)에 닿거나 넘는 작업은 부하 평균 약 20일 때 약 18 ms, 더 무거운 부하에서 잰 별도
측정에서는 24–67 ms였다. 따라서 예산 근처의 작업은 App 스레드에서 한 프레임(16 ms)을 넘을 수 있다. 이 비용을 없애려면 예산을 메모리상 필드 길이로 재거나 크기 판정을 worker로 옮겨야 하며,
둘 다 큐 예산의 단위나 제출 계약을 바꾼다.

credit이 반환되면 대기 요청을 도착 순서대로 재시도한다. journal이 중단되면 대기 요청에도
같은 typed 중단 오류를 반환하며, 이 처리는 명령 실행·publication 처리와 분리된 함수가 맡는다.

구조 명령은 대상 engine을 고정한 뒤 준비·commit을 진행한다. 앞선 명령이 commit을 마치고 외부 effect 결과를 기다릴 때, 서로 다른 engine으로 확정된 후속 명령은 진행할 수 있다. 같은 engine과 아직 범위가 정해지지 않은 명령, 다중 stream 명령은 순서 장벽을 유지한다. 저널 commit과 전체 batch publication/ACK는 여전히 단일 순서다. 대기 중 GUI 선택이 바뀌어도 고정된 engine을 다시 선택하지 않는다. 응답 조회·거절 확정은 외부 effect 대기에 막히지 않는다.

engine 고정은 IPC 요청을 사용자 intent로 바꾸지 않는다. 대기 후 재해소되는 workspace/tab 생성도 원 IPC의 생성 전용 해소와 CWD 정책을 유지한다.
