# 자원 회수와 Recovery 관측

구조 journal의 operation 결과와 실제 실행 자원의 관측은 구별한다. 새 writer lease,
View 소멸, timeout, PID 부재만으로 이전 PTY·plugin 실행이 끝났다고 기록하지 않는다.
실행 결과를 모르면 `Uncertain`과 원래 generation·attempt·자료 참조를 유지한다.

## 현재 프로세스의 회수

`ResourceRetirement`와 `Installation`은 정확한 이전 PTY의 `PtyRetirement`를 보유한다.
PTY의 `Reaped` 관측이 있어야 OS 회수가 끝난다. `WaitFailed`나 기한 경과는 불명 결과다.
설치 중 후속 단계가 실패해도 이전 owner receipt와 남은 candidate는 버리지 않는다.

Plugin 회수는 원 surface binding과 process binding에 연결한 destroy RPC의 응답으로
관측한다. FIFO enqueue나 host 등록 제거는 응답이 아니다. 원 process의 성공 응답 또는
host FIFO에서 생성 요청이 아직 전달되지 않았다는 정확한 취소 근거가 있어야 회수를
완료한다. 오류·연결 종료·process 교체는 불명이며, 살아 있는 process의 timeout만으로
pending 응답을 없애지 않는다. 이후 실제 응답이 오면 같은 retained owner가 확인한다.
일반 remote surface도 create/restore가 실제 큐에 수락된 process identity를 등록과 함께
보관한다. destroy와 별도 완료 응답을 기다리지 않는 명시 회수 통지는 그 process에만 보내며 같은 plugin ID로 재시작한
process에 대신 보내지 않는다. 생성 요청의 큐 미수락은 알려진 미전송이지만, 등록 부재나
원 process 소멸은 미전송 증거가 아니다.

Mesh도 각 runtime 인스턴스의 불투명 binding으로 bootstrap을 추적한다. 큐에 제출하지
못한 create는 미실행으로 구별하며, 제출된 create는 원 process와 request identity를
고정한다. 명시 bootstrap 요청이 새 process에 도착하면 별도 세대를 추가한다. 이전
세대도 보존하며 process 교체를 회수 성공으로 취급하지 않는다. 회수는 각 원 process에만
destroy를 보내고 모든 세대의 실제 응답을 기다린다. 같은 surface
ID의 새 인스턴스나 새 process를 찾아 대신 종료하지 않는다. View는 binding을 전달할
수 있지만 내부 상태는 plugin manager만 변경한다.

정상 구조 공개 장벽에서도 plugin control pump는 필수 회수 요청과 해당 응답을 처리한다.
새 create/restore는 원 FIFO 순서로 보관하고 일반 callback은 실행하지 않는다. 아직
보내지 않은 create/restore와 정확히 같은 binding의 retire가 만나면 그 한 쌍만 취소한다.
장벽이 풀리면 보관한 publication부터 정상 FIFO 처리를 재개한다. 보관 예산을 넘으면
초과 항목의 소유를 유지한 채 journal의 기존 halt/disposal 경계로 전이하며 성공이나
계속 진행 가능한 대기로 표시하지 않는다.

Journal이 멈춘 경우의 plugin control pump는 회수 요청과 해당 응답만 실행한다.
새 create/restore·hook·namespace continuation·자동 재시작은 실행하지 않는다.
일반 응답은 제한된 backlog에 남기며 정상 pump가 명시적으로 재개될 때 원 process
binding을 확인한다. backlog가 차면 읽기를 멈추고 완료를 추측하지 않는다.

## 생성의 완료 조건

생성·교체(신규 탭·pane·surface 분할·workspace·convert·respawn·adopt·lazy restore·assembly·새
윈도우 초기화)는 아래 세 사실을 구별한다. 셋은 서로의 증거가 아니다.

| 사실 | 판정 근거 | 증거가 아닌 것 |
|---|---|---|
| 구조 사용 가능 | 최종 완료 batch(`FinishCleanup`)를 담은 전체 publication ACK | 설치 허가(`FinishCreation`), 후보 설치, journal commit만 끝난 상태 |
| 원 자원 회수 | 교체한 이전 PTY의 `Reaped`, 이전 plugin binding의 원 process destroy 응답 | 신호 전송, 등록 제거, FIFO enqueue, 기한 경과, PID 부재 |
| 입력 전달 | child 본문 ACK(또는 본문 기한) 뒤 settle을 거친 CR의 큐 수락 | 외부 앱의 명령 실행 완료. CR 수락도 실행 결과를 뜻하지 않는다 |

GUI intent·IPC·plugin·headless 진입점은 모두 같은 command 경계를 거치며, 원 명령의 성공 응답은
해당 command의 확정 결과를 읽은 뒤 한 번 전달한다. 같은 키의 재요청은 저장된 결과를 돌려주고
외부 실행을 새로 만들지 않는다.

현재 구현의 대기 관계는 다음과 같다.

| 단계 | 기다리는 조건 | App 관측 |
|---|---|---|
| 준비(`PrepareCreation` 뒤 claim) | 터미널은 전용 준비 스레드의 PTY spawn·capture 주입. kind factory·standalone 채택은 App 안에서 실행 | 터미널 준비 동안 보류하지 않는다. kind factory 실행 동안은 App 이벤트 처리가 멈춘다 |
| 설치(`FinishCreation` 공개 중) | publication 처리 안에서 후보 설치·이전 owner 분리 | 전역 보류 |
| 설치 뒤 | 기다리지 않고 `CleanupFinished` 제출. child 본문은 설치 때 이미 썼다 | 전역 보류 |
| 최종 공개 | `FinishCleanup` commit과 전체 publication ACK | commit 전까지 전역 보류 |
| 공개 뒤 정리(`Settling`) | child 본문 ACK(기한 5초)와 settle 20ms 뒤 CR, 교체한 이전 PTY·plugin binding의 receipt(기한 5초) | 보류하지 않는다. 종료는 이 단계가 끝나기를 기다린다 |
| 불명(`Uncertain`) | 불명 기록 확정 뒤 receipt로 정합할 수 있는 owner(설치 owner·폐기 receipt·회수한 실패 설치)만 1초 간격으로 5초 동안 재확인한다. 그 뒤 owner를 `EngineRelease`로 넘기고 continuation을 끝낸다. 증거 수단이 없는 불명(준비 중 불명, 원 standalone 등록으로 돌려놓지 못한 채택)은 기록 확정 즉시 끝낸다. 기록 확정 전에는 App 깨움 기한을 내지 않으며 worker의 응답이 App을 깨운다 | 미공개 설치 owner를 가진 경우만 보류 |
| 원 명령 응답 | 최종 commit의 확정 결과를 공개 뒤 정리가 끝난 다음 읽는다. 불명 continuation이 끝나면 복구 필요(`ERR_OPERATION_RECOVERY_REQUIRED`)로 응답한다. durable 명령은 InProgress로 남는다 | - |

공개 뒤 회수가 기한을 넘기거나 실패하면 receipt를 EngineSession의 해제 owner(`EngineRelease`)로
넘기고 원 요청에 복구 필요(`ERR_OPERATION_RECOVERY_REQUIRED`)를 응답한다. 구조와 명령 결과는 이미
성공으로 확정됐으므로, 이 응답 뒤 같은 키로 다시 요청하면 확정된 성공 결과를 받는다. 기다리는 동안 도착한
같은 키 요청만 대기에 합류해 같은 복구 필요 응답을 받는다. 결과를 알 수 없는 생성의 재요청과 다르다. child의 CR을 보내기 전에
대상의 물리 generation이 바뀌었거나 hard 점유됐거나 CR 큐가 닫혔으면 CR을 보내지 않고 같은 오류로
응답한다. 본문은 설치 때 한 번만 쓰며 다시 보내지 않는다. 구조는 이미 공개됐으므로 이 응답은 구조
실패를 뜻하지 않는다. 같은 키의 이후 재요청은 확정된 구조 결과를 돌려주고 본문을 다시 쓰지 않는다.
CR을 기다리는 동안 그 터미널의 출력과 다른 창의 입력·렌더·조회는 진행한다. 사용자가 그 터미널에
입력하면 본문과 CR 사이에 들어갈 수 있다. terminal tell의 본문·CR 사이 입력과 같은 조건이다.

설치 실패는 두 가지로 나눈다. 이전 owner를 은퇴시키거나 새 owner를 등록·설치하기 전의 실패는
알려진 거절이다. 후보를 기존 폐기 경로로 회수하고 원 명령은 실패로 끝난다. 그 뒤의 실패는 미공개 새
owner(PTY·plugin 등록·child 관계)를 회수하고, 이전 owner가 이미 은퇴한 대상만 Recovery 차단
placeholder로 바꿔 사용할 수 없음을 표시한다. operation은 `Uncertain`으로 기록하며, 재확인 기간 안에 회수
receipt가 모두 완료되면 폐기 evidence로 정합해 실패로 끝낸다. 기간이 지나면 남은 receipt는
`EngineRelease`가 보관하고 원 명령은 복구 필요로 응답한다. 이 동안 관측은 보류하지 않는다.
standalone 채택 후보는 회수할 때 종료하지 않는다. 연결을 끊지 않은 채 원래 standalone 등록(pty ID·
metadata·scrollback 보존 ID)으로 돌려놓는다. 설치 전 거절로 후보를 폐기할 때와 설치 뒤 실패로 새 owner를
회수할 때가 같다. 원 ID를 다른 owner가 차지해 돌려놓지 못하면 operation을 `Uncertain`으로 기록한다. 이
프로세스를 정합할 receipt는 없으므로 기록이 확정되면 바로 continuation을 끝내고 원 명령에 복구 필요로
응답한다. 프로세스는 EngineSession의 대기 후보(`pending_materializations`)에 남지 않고 `EngineRelease`의
반납 불가 standalone으로 넘어간다. `EngineRelease`는 이 프로세스를 retire하지 않고 해제 완료 조건에도
넣지 않으므로 그 엔진의 닫기와 해제를 막지 않는다. 엔진 해제가 끝나면 App이 이 프로세스를 넘겨받아 앱이
끝날 때까지 보관한다. 이때 다른 PTY처럼 drop에서 child 종료를 시도한다. 보관 중인 프로세스의 종료는 따로
관측하지 않는다. `EngineRelease`가 있는 엔진의 해제가 끝날 때마다 App이 보관 목록의 종료를 확인하고, 끝난
프로세스는 reap한 뒤 목록에서 뺀다. `EngineRelease`가 새 프로세스를 넘겨받을 때도 자기 목록을 같은 방법으로
정리한다. 그 사이에 스스로 끝난 프로세스는 reap되지 않은 채(zombie) 남고, 그 PTY reader는 종료가 관측될
때까지 늘어나는 간격(최대 500ms)으로 깨움을 반복한다. 보관 중인 프로세스는 `pty.list`와 surface 조회에
나타나지 않는다. 중단(halt) 경로가 owner를 `EngineRelease`에 모을 때도 같다. 중단 시점에 아직 설치 전인
채택 후보(`pending_materializations`)도 원 프로세스를 retire하지 않고 같은 방법으로 보관한다. 이 처리는 사용자 프로세스를 종료하지 않으면서
엔진 해제를 끝내기 위한 것이며, 그 엔진의 다른 standalone PTY가 엔진 해제 때 retire되는 것과 다르다.

생성이 불명이 되는 순간(child가 생겼을 수 있는 준비 오류·폐기 실패·폐기 reap 기한 경과·설치 실패)
operation·stream·attempt·generation·단계·원인을 경고 로그로 한 번 남긴다. 입력 본문은 기록하지 않는다.
같은 키의 재요청은 기록된 불명 결과를 읽어 복구 필요로 응답하며 다시 실행하지 않는다.

터미널 spawn 오류는 child가 생기기 전 실패와 생긴 뒤 실패로 나뉜다(`tasty_terminal::SpawnStage`).
경계는 portable-pty `spawn_command`의 반환이다. PTY 할당·셸 실행 파일 확인·프로세스 생성 실패는
child를 남기지 않으므로 알려진 준비 실패로 응답하고 불명 기록을 남기지 않는다. 같은 키의 재요청은
기록된 실패를 돌려주고, 설정을 고친 뒤 새 키로 요청하면 다시 실행한다. macOS·Linux 에서 설정한 셸 경로에 파일이 없으면
실행 시 탐지한 셸로 대신하므로(`GeneralSettings::runtime_shell`) spawn 오류가 되지 않는다. 대신할 셸도 없을 때만 실행 파일
확인에서 실패한다. macOS·Linux 에서 설정 셸이 디렉터리이거나 실행 권한 없는 파일이면 대신하지 않고 아래 확인에서
원인과 함께 실패한다. Windows 는 Git Bash 가 아니거나 없는 경로, 빈 실행값을 아래 확인에서 거절하고, 원인 문장(`terminal_error.*`)은 그 시점의 표시 언어로 만들어 화면과 IPC 응답에 같은 값을 넘긴다. child가 생긴 뒤 입출력 준비·
reader/writer worker 시작·capture 주입이 실패하면 불명이다.

- Unix: `openpty`와 실행 파일 검색은 fork 전에 실패한다. 설정 셸은 `ShellConfig`가 판정에 쓴 절대 경로로 넘어오므로
  ([경로 해석](../concepts/path-resolution.md)) 아래 상대 경로 검색은 해석하지 못한 값(대신할 셸도 없을 때)에만 쓰인다.
  portable-pty는 절대 경로를 그대로 쓰고, `./`·`../`로
  시작하는 상대 경로는 작업 디렉터리 기준으로 찾는다. 이 두 경우 후보가 디렉터리이거나 `access(X_OK)`를 통과하지
  못하면 원인과 함께 거절한다. 그 밖의 상대 이름은 PATH 항목에서 차례로 찾는다. 상대 PATH 항목은 작업 디렉터리
  기준이다. 쓸 수 없는 후보(디렉터리, 실행 권한 없음)는 사유만 모으고 다음 항목으로 넘어가며, 끝까지 쓸 수 있는
  후보가 없을 때만 모은 사유와 함께 실패한다. 디렉터리는 `access(X_OK)`를 통과하므로 `tasty-terminal`이
  portable-pty보다 먼저 디렉터리인지 확인해 "is a directory" 원인과 함께 거절한다. fork 뒤
  pre_exec의 `setsid`·`TIOCSCTTY` 실패는 std `Command::spawn`이 오류 파이프로 받아 그 child를 회수한 뒤
  돌려준다. exec 실패는 오류로 돌아오지 않는다. portable-pty의 pre_exec가 stderr 위 descriptor를 모두
  닫아 std의 오류 파이프도 닫히기 때문이다. 이 경우 생성은 성공하고 child가 곧바로 종료된다(실행 비트가
  있지만 해석기가 없는 스크립트 등).
- Windows: `CreatePseudoConsole`과 `CreateProcessW` 전 준비가 실패하거나 `CreateProcessW`가 실패하면
  프로세스가 만들어지지 않는다.

이 분류는 생성 경로마다 다음과 같이 끝난다.

- IPC 공개 생성: 호출자에게 알려진 실패로 응답한다.
- 선택 복원: placeholder에 실패 이유를 기록하고 시도 횟수를 늘린다. 자동 재시도는 시도 5회
  (`MAX_ACTIVATION_ATTEMPTS`)에서 멈추며 placeholder는 그대로 남는다.
- 엔진 첫 workspace: 응답할 호출자가 없으므로 그 생성만 실패로 끝낸다. journal은 계속 동작하며,
  실패 이유는 engine별로 보관해 아래 실패 표시의 오류 본문에 붙인다.

새 창이나 첫 부팅의 첫 workspace 생성이 실패하거나 불명이 되면 engine은 준비됐지만 workspace가 없다.
View는 workspace 없이 그릴 수 없으므로 새 창은 창 생성 실패(`window.create` 오류와 실패 알림)로
끝나고 그 engine을 실패한 opening으로 해제한다. 첫 부팅은 아직 View와 IPC를 공개하지 않았으므로,
홈 사용 중이 아니고 임시 세션을 아직 시도하지 않았으면 원 journal 실행을 중단하고 빈 임시
journal로 계속한다. binding 이후에도 같은 경로이며, 기존 자원은 분리된 `EngineRelease`가
정확한 receipt를 보관·관측하므로 새 세션을 여는 시점에 reap이 모두 끝났다는 뜻은 아니다.
원본과 미완 effect는 보존하고 복원 명령을 재실행하지 않는다. 임시 세션도 첫 workspace를
만들지 못하면 엔진 생성 실패 화면으로 넘어간다. 알려진 실패라면 오류 본문에 그 이유
(예: 셸 실행 파일 확인 실패)를 함께 보여 준다. 세부 순서는 [부팅 시퀀스](boot-sequence.md)에 있다.

터미널 준비는 App이 검증과 고정 입력(셸 실행 정보·waker·색·복원 capture)을 마친 뒤 전용 스레드
하나에 맡긴다. 동시 실행은 하나이며 앞선 작업 뒤 대기열은 16개로 제한한다. 대기열이 차면 아무것도
실행하지 않은 채 알려진 준비 실패로 응답한다. 완료된 child는 App이 claim 때 본 대상과 다시 대조한다.
대상 터미널의 물리 generation이 바뀌었거나 새 대상에 다른 owner가 생겼으면 설치하지 않고 새 child를
retire하며, reap이 확인되면 알려진 준비 실패로, 기한 5초 안에 확인하지 못하면 불명으로 기록한다.
엔진 해제가 준비 중인 생성을 넘겨받으면 `EngineRelease`가 결과를 기다려 PTY를 retire하고 reap 전에는
해제를 끝내지 않는다. 결과를 받을 owner가 사라졌으면 준비 스레드가 child를 retire해 reap까지 보관하고,
시작 전이면 실행하지 않는다. 프로세스 종료 때 App은 생성 continuation보다 준비 스레드를 먼저 닫는다. 이때
대기열에 남은 작업은 결과를 받을 owner가 있어도 실행하지 않고, 닫히는 동안 시작을 마친 child는 결과로
넘기지 않고 retire해 보관한다. 준비 스레드는 보관한 child의 reap을 확인한 뒤 끝난다.
kind factory는 반환하는 surface와 native 자원의 스레드 제약을 일반적으로 보장할 수 없어 App
스레드에 남긴다. standalone 채택은 이미 있는 PTY의 소유권 이전이라 다시 실행할 작업이 아니다.

standalone 채택은 준비 완료부터 설치 또는 원위치 반납까지도 전역 보류한다.

GUI 종료의 `SavingLayout`과 headless 종료는 journal 대기를 첫 대기 때 정한 기한 15초까지만 기다린다.
대상은 관측 보류(미공개 설치·standalone 채택 보류), 공개됐지만 끝나지 않은 생성(공개 ACK·child CR·교체한
이전 owner의 receipt), 닫기 정리의 receipt 대기다. GUI 종료는 그 뒤에 이어지는 종료 시점 View 저장(View
기록·surface capture)과 `restore_layout`이 꺼진 엔진의 slot retirement도 같은 기한 안에서 기다린다. 기한은
`SavingLayout` 전체에 하나이며 이후 회차는 늘리지 않는다. 15초는 앞의 세 대기의 단계별 기한(child 입력 5초,
교체 receipt 5초, 닫기 receipt 5초)보다 길어 진행 중인 대기를 끊지 않도록 정한 값이며 측정값이 아니다. View
저장과 slot retirement는 worker 응답만 기다리며 자체 단계 기한이 없다. 기한이 지나면 journal
실행을 중단(halt)한다. 대기 중인 요청은 중단 사유로 응답하고, 미완 operation은 다음 시작의 Recovery
대상으로 남는다. 남은 owner의 처리는 종료 방식마다 다르다. headless 종료는 중단 경로로 남은 생성·미공개
후보·회수 receipt를 `EngineRelease`에 모아 종료 상한 안에서 reap·plugin 응답을 관측한다. GUI 종료는 중단 뒤
이 경로를 부르지 않는다. 남은 owner는 App과 함께 버려지며, PTY는 drop 때 child 종료를 시도하지만 reap을
관측하거나 회수 완료를 기록하지 않는다. 중단 뒤에는 종료 시점 View를 저장하지 않으므로
마지막으로 저장된 View가 남는다. View 저장이나 surface capture를 기다리던 중 기한이 지나면 아직 응답받지 못한
기록과 capture는 저장됐는지 알 수 없고, 다음 시작은 마지막으로 확정된 View·capture를 읽는다. slot
retirement를 기다리던 중 기한이 지나면 그 slot은 은퇴하지 않고 stream이 journal에 남는다. `restore_layout`이
꺼진 GUI의 다음 부팅은 slot을 복원(resume)하지 않으므로 남은 stream을 화면에 되살리지 않는다. 이 중단은 회수
완료나 명령 결과를 뜻하지 않는다.

## EngineSession 해제

닫힌 View나 실패한 View 생성의 EngineSession도 필수 의무를 먼저 배출한다.
`EngineRelease`는 남은 kind 인스턴스, standalone을 포함한 PTY, 기존 installation 및
private candidate의 회수 receipt를 보관한다. 원 등록 밖에 있는 채택 프로세스(반납 실패, 중단 시점의 설치 전 후보)는 예외로 retire하지
않고 해제 완료를 기다리게 하지도 않는다(위 생성의 완료 조건 참조). 실제 reap·plugin 응답이 끝나기 전에는
registry owner를 버리지 않는다. stream 보존 close는 구조·metadata를 지우지 않는다.
private candidate가 아직 publication closure를 보유하면 그 closure를 실행하지 않고
폐기했다는 원 binding의 완료 receipt를 전달한다. closure가 Installation으로 이동한
경우도 해당 owner가 증거를 보관한다. 이미 소비한 closure나 등록 부재로 이 증거를
만들지는 않는다.

이 경계는 TaskScope 취소가 아니다. runner stop·task cancel·OS kill은 기존의 별도
명령 의미를 따른다. surface에 묶인 observer와 hook을 해제한 뒤 PTY를 retire하며,
TaskScope의 비취소 Drop 의미를 바꾸지 않는다. 소멸 시점의 미완 관측을 성공으로
보고하거나 raw 입력을 재전송하지 않는다.

## 대조 결과 확정

현재 runtime의 정확한 effect lease와 retained resource receipt가 있어야 준비·회수의
불명 결과를 다시 대조한다. 늦은 완료를 처리할 때도 stream, operation, attempt,
runtime epoch, engine incarnation, activation 및 physical generation을 구분한다.
대조 evidence payload와 결과 event/effect 갱신은 journal의 같은 commit 경계를 지난다.
여러 leaf를 복원하는 묶음에서 미실행 member의 취소도 준비 결과 집계에 반영한다.
이미 준비한 private peer에는 원 generation의 폐기 의무를 만들고 receipt 뒤 묶음을 닫는다.
모든 member가 알려진 실패·취소로 끝났다면 대상이 사라져도 실패로 완료하며, 실제 설치나
불명 결과가 있는 peer를 새 준비 대상으로 다시 허가하지 않는다.
그 뒤에 전체 batch publication ACK와 최초 command 결과 완료를 처리한다.

재시작 시 미확정 creation이 남은 placeholder는 Recovery 차단 상태를 갖는다.
선택 복원은 이를 건너뛰며 이전 실행을 새 factory 호출로 대체하지 않는다. 준비 단계는
실행에 들어가기 전 실패와 factory·spawn 진입 이후의 불명 오류를 구분한다.

이 문서는 구현의 소유 계약을 설명한다. 장애 재현·플랫폼별 실행 결과는 별도의 검증
기록으로 확인하며, source에 이 경계가 있다는 사실이 실행 검증 통과를 뜻하지 않는다.

원격 user close의 undo 권한은 로컬 사용자 lifecycle origin과 별도로 저장한다.
원격 사용자가 닫은 항목은 undo에 남길 수 있지만 plugin의 `is_user_close` 통지는
로컬 사용자 동작으로 바뀌지 않는다. 기존 저장 기록에는 원격 flag가 없으므로 false로 읽는다.

원격 forward의 `op_id`는 Result/Delta 상관관계 값이며 durable 멱등 키를 만들지 않는다.
서버 journal은 각 접수의 command identity를 발급하고 원 connection registration과
응답 binding을 유지한다. client/runtime epoch/registration/op_id는 내부 요청 envelope의
correlation 값으로 원 command 입력 바이트에 보존하며 중복 제거에 사용하지 않는다.
공개 IPC에서 명시한 멱등 키의 기존 계약과는 별개다.

새 pane의 live 생성 완료는 원 target pane과 확정 split 방향으로 `pane.split`을 통지한다.
이 통지는 최초 성공 command continuation에서만 생성한다. Stored 재시도, journal replay와
기존 surface의 materialization은 같은 사용자 생성 통지를 다시 만들지 않는다.

Headless 종료도 journal 실행을 먼저 중단한 뒤 미공개 후보와 기존 설치·PTY·plugin 회수 receipt를 같은 EngineRelease에 모은다. 종료의 5초 상한 안에서 runner join, 감지 worker join, PTY reap 및 원 plugin destroy 응답을 각각 관측한다. 시간초과는 회수 완료나 durable 명령 취소를 뜻하지 않으며, 미완 operation은 다음 시작의 Recovery 대상으로 남는다.

View의 구조 정합은 실제 Explorer·DAG cache 및 셸 안내 기록의 ID를 현재 descriptor와 대조해 사라진 항목을 회수한다. terminal viewport는 기존 retain 경로를 유지한다. 늦게 도착한 닫기 통지의 ID만으로 cache를 지우지 않으므로 동일 ID에 현재 descriptor가 있는 후속 owner의 표시 상태는 보존된다.

튜토리얼 연습 workspace 준비는 최초 생성 완료를 원 View와 원 준비 요청 token에 전달한다. 성공 시 실제 생성 surface의 현재 activation·physical binding을 확인해 practice workspace/pane/tab을 설정하고, 실패 시 같은 요청의 preparing을 해제한다. 중단·재시작된 튜토리얼의 이전 결과나 replay는 새 준비 요청을 완료시키지 않는다.

구조 완료 통지는 journal의 live completion과 host event에서 전달한다. 옛 동기 CoreEvent의 create/close/move/restore 결과와 즉시 owner 삭제 API는 제품 실행 경로로 유지하지 않는다. snapshot과 전체 이력 replay 비교용 보조는 시험에 한정하고, 제품 저장은 live pin과 View manifest를 포함한 checkpoint를 사용한다.

App journal의 publication pump는 `journal/publication.rs`에서 batch 설치와 ACK 순서를 소유한다. `commands/completion.rs`는 worker 결과와 원 응답을 결합하고, GUI의 고정 대상 해소·원 View 후속 처리는 `commands/view_completion.rs`에서 수행한다. 모듈 분리는 동일한 큐·owner·barrier를 유지하며 별도 실행 경로나 worker를 추가하지 않는다.

슬롯 없는 headless 시작은 영속 예약한 새 engine 번호의 stream을 연다. 정상 종료의 process owner 회수는 `engine.retired` 확정과 같지 않으며, 과거 headless stream을 자동 resume하지 않는다. 현재 과거 stream의 모델 참조는 후속 snapshot의 pin에 남을 수 있고, 저장 예산 도달 시 신규 admission을 제한한다. 이는 과거 headless 자료의 영구 보존 요구나 모든 미사용 자료의 GC 완료를 뜻하지 않는다.
