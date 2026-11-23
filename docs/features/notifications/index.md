# 알림 (Notifications)

## 전용 알림 영역의 기준과 현재 범위

[피드백 원칙](../../principles/feedback-and-recovery.md)에 따른 전용 알림 영역은 LNB 하단 Tools 등 버튼 블록 바로 위의 작은 공간에 둔다. 워크스페이스 행의 attention 배지와 별개이며, 간단한 정보를 표시하고 사용자가 직접 누르면 팝업에서 상세 내용을 확인한다.

오류·경고·정보마다 확인하지 않은 알림 개수를 표시한다. 오류는 빨간색, 경고는 노란색, 정보는 파란색이며 정보 색은 LNB의 완료된 surface 개수 표시와 같다. 색을 공유해도 완료 surface 수와 미확인 알림 수를 합치지 않는다.

LNB 요약 영역은 펼친 사이드바와 접힌 레일 모두에서 세 레벨의 미확인 개수를 표시한다. 사용자가 요약을 클릭하면 현재 윈도우의 상세 팝업을 연다. 알림 도착은 포커스를 옮기지 않으며, 팝업을 열거나 닫는 것만으로 읽음 처리하지 않는다. workspace attention 표시는 별도로 유지한다.

[피드백 원칙](../../principles/feedback-and-recovery.md#알림에-남길-문제)에 따라 호스트가 스스로 남기는 오류·경고는 사용자가 직접 조치할 수 있는 문제로 한정한다. 조치할 수 없는 내부 문제는 로그에만 남긴다. 현재 `src/system_notice.rs`로 알림을 남기는 기존 경로는 이 기준으로 다시 분류하지 않았다. 로그는 release에서 `~/.tasty/debug.log`(warn 이상)에 남지만 host를 시작할 때마다 비우며, crash report 파일에는 담기지 않는다([크래시 진단](../../dev-guide/crash-diagnostics.md)).

- **Status**: Implemented
- **주체**: 로컬 사용자 (에이전트도 `notification.create` 로 발행 가능)
- **설계**: [국제화 (i18n)](../../dev-guide/i18n.md) (제목은 표시 전용 · `notification.create` 기본 제목은 UI 문구)
- **코드**: `NotificationStore` (notification 모델); IPC `notification.{list,create,counts,read}`
- **화면**: LNB 알림 요약 · 알림 패널 popup/fullscreen (Window 스코프) · workspace attention 배지

## 목적

터미널이 보낸 OSC 알림 시퀀스와 시스템 이벤트를 모아 **인앱 알림 패널·소리·surface 하이라이트·사이드바 배지**로 노출한다.

## 내부 동작

### OSC 시퀀스 감지

termwiz Parser 의 OSC 액션을 인터셉트해 알림 이벤트 생성 — OSC 9(iTerm2/ConEmu), OSC 99(Kitty), OSC 777(rxvt), BEL. (OSC 7=cwd 변경, OSC 0/2=타이틀 변경은 알림이 아닌 별도 처리.)

**벨(BEL) 토글**: BEL 알림(제목은 `notification.bell_title` 번역값)은 전역 `notification.enabled` 위에 벨 전용 `general.bell_notification`(기본 on)을 한 겹 더 얹어 게이트한다. off 면 토스트를 억제하되, 사용자가 등록한 `bell` 훅은 그대로 실행한다(훅=명시적 자동화 → 수동 반응인 토스트와 분리). `cascade_terminal_bell_ring` 참조.

### 제목은 표시 전용 — 식별은 별도 필드로

호스트가 스스로 만드는 알림 제목은 전부 `t()` 번역값이라 `general.language` 에 따라 달라진다 — 벨은 `notification.bell_title`, `notification.create` 가 `title` 없이 호출되면(`tasty notify "본문"`) `notification.default_title`. 따라서 제목 문자열은 **기계적 식별자가 아니다**. 알림의 출처를 구분해야 하는 소비자는 식별 필드를 본다 — 훅은 `HookEvent::Bell`(셸 env `TASTY_HOOK_EVENT=bell`, payload 에 제목 없음 — [hooks](../hooks/index.md)), plugin 은 `notification.created` 이벤트의 `source`. 제목 비교(`title == "Bell"`)로 분기하는 코드는 언어를 바꾸는 순간 깨지므로 두지 않는다. 근거·대안: [국제화 (i18n)](../../dev-guide/i18n.md).

### NotificationStore

`VecDeque`에 최대 100개를 보관하고 초과하면 가장 오래된 항목을 `pop_front`로 제거한다.
같은 workspace·surface·level의 알림이 설정 간격(기본 500ms) 안에 이어지면 기존 항목에 합친다.
새 내용이 읽은 항목에 합쳐지면 그 항목은 다시 미확인 상태가 된다. 다른 레벨은 합치지 않는다.
Tasty 자체가 남기는 호스트 알림(저장소 초기화 실패, headless PTY 회수 등)은 서로 다른 사건이므로 간격 안에 이어져도 합치지 않고 각각 남긴다.
개별 또는 전체 읽음 처리를 지원한다.

새 알림은 해당 surface의 `AttentionStore`에도 주의 표시를 요청한다. 알림 기록과 주의
표시는 서로 다른 저장소이며, 사이드바 배지는 읽지 않은 알림 수가 아니라 주의 표시가 있는
surface 수(`attention_count`)를 보여준다. 자세한 규칙은 [surface-highlight](../surface-highlight/index.md)를 따른다.

### 작업 스레드의 시스템 알림

사용자 입력에서 시작하지 않은 작업(웹훅 리스너 시작, 자동 훅·웹훅 실행 실패, [Lua 자동실행](../lua-hooks/index.md#실행-결과-전달)의 실패·차단·억제 등)은 `src/system_notice.rs`의 대기열에 알림을 남긴다. GUI는 메인 루프의 `about_to_wait`에서, headless는 루프 회차마다 시스템 출처 알림으로 저장한다. surface를 지정한 알림(surface 훅)은 그 surface가 속한 workspace와 surface가 받는다. 나머지와 surface가 이미 사라진 알림은 GUI에서는 포커스 윈도우(없으면 첫 윈도우)의 활성 workspace, headless에서는 활성 workspace가 `surface_id: 0`으로 받는다. 받을 윈도우·workspace가 아직 없으면 대기열에 그대로 두므로, 부팅 중에 생긴 경고는 첫 윈도우가 등록된 뒤 저장된다.

- 서로 다른 사건은 저장소의 500ms 합치기를 거치지 않는다. 대기열의 알림은 `NotificationMerge::Separate`로 `add_separate`에 넣으므로 같은 workspace·surface·레벨로 이어져도 제목·본문·미확인 수가 각각 남는다. 터미널 OSC·벨, `notification.create` 등 나머지 알림은 `Coalesce`로 기존 합치기를 따른다.
- 같은 source·레벨·제목·본문의 알림은 10초(`REPEAT_WINDOW`) 안에 한 번만 남긴다. 이 억제는 내용이 같은 반복에만 적용되어 반복 실패가 목록을 덮지 않게 한다.
- 대기열 상한은 32건이며 넘으면 가장 오래된 항목을 버리고 경고 로그를 남긴다.
- source는 GUI `notification.created` 이벤트에만 실리며 저장소와 `notification.list`에는 남지 않는다.

### IPC 목록의 범위·ID·순서·상한

`notification.list`는 모든 main/parked engine의 알림을 합쳐 반환한다. headless는
하나의 engine에 같은 규칙을 적용한다. UI 패널과 저장소는 여전히 창별이고, 각
저장소의 FIFO 보존 상한 100개와 읽음 상태는 공유하지 않는다.

ID는 인스턴스의 공유 IdGenerator에서 발급하는 u64이며 재시작 동안 보존되는 ID가
아니다. 목록은 ID 내림차순, 즉 **신규 생성 순서의 역순**으로 전체 최대 50개다.
각 항목은 `level: error|warning|info`를 포함한다. 창 순회 순서나 포커스는 결과를 바꾸지 않는다. coalescing은 기존 ID와 생성 순서를
유지하므로 최근 내용 갱신이 항목을 앞으로 옮기지는 않는다. 기존 한 engine의 순서와
고정 50개 응답 계약을 유지하며 새로운 limit/필터 인자를 도입하지 않는다.

이는 [목록 합산 원칙](../../design/policies/focus.md)의
적용이며, GUI 패널을 전역 패널로 바꾸는 결정이 아니다.

<a id="시스템-알림--사운드"></a>

### 알림 소리

`notification.sound`가 true이면 새 알림에 소리를 한 번 재생한다. macOS는 `NSBeep`,
Windows는 `MessageBeep`, Linux는 `paplay` → `aplay` → `\a` 순서로 시도한다.
headless는 소리를 내지 않는다. 기존 항목에 합쳐진 알림은 소리를 재생하지 않는다.
터미널 Bell(BEL)은 그 자리에서 OS 소리를 내지 않고, Bell 알림(`general.bell_notification`)이
켜져 있으면 그 알림이 새 항목일 때 위 소리를 한 번 낸다.
현재 OS 알림 센터로 보내는 기능은 없다.

**원격 attach 대상의 알림 소리**는 접속한 client 에서 난다. 알림 저장·attention·`NotificationCreated`·훅은 서버에 그대로 남고 소리만 옮긴다.

- 기준은 서버에 저장된 새 알림이다. 출처(IPC `notification.create`, 플러그인·시스템 안내, 터미널 바이트의 OSC 9/99/777·Bell·명령 기록 한도)와 무관하게, 대상 surface 를 GUI client 가 준비된 hard 점유로 가졌거나 surface 가 없거나 점유되지 않은 surface 이고 그 workspace 를 점유했으면, 서버는 소리를 내지 않고 그 client 에 `NotificationSound` 를 한 번 보낸다. client 는 자기 `notification.sound` 가 켜져 있으면 한 번 재생한다. 서버의 `notification.sound` 값은 여기에 영향이 없다. headless 서버도 같은 조건이면 보낸다.
- GUI client 의 mirror 도 같은 터미널 바이트로 자기 알림을 만든다. GUI 서버에 붙은 동안 이 터미널 유래 알림은 client 패널에 기록만 하고 소리를 내지 않는다(소리는 서버 알림에서 온다). headless 서버는 터미널 바이트에서 알림을 만들지 않으므로, headless 서버에 붙은 mirror 는 자기 터미널 유래 알림으로 소리를 낸다.
- CLI attach 와 이전 버전 client 가 점유한 대상은 서버가 지금처럼 소리를 낸다.
- 합쳐진 항목은 어느 쪽에서도 소리를 내지 않는다. 알림을 만든 순간에만 보내므로 재연결·초기 상태 복원은 과거 소리를 재생하지 않는다. 보내지 못한 소리(연결 큐가 가득 참)는 화면 재동기화 없이 버리고 다시 보내거나 서버에서 대신 내지 않는다. client 가 출력 손실을 알게 된 뒤 재동기화가 끝날 때까지 도착한 소리도 버린다. client 가 연결 직후 선언을 보내기 전 잠깐은 서버가 낸다.
- 점유되지 않은 서버 workspace 의 알림은 위 서버 설정대로 동작한다.

### 시각 표시

- **surface 하이라이트**: 알림 발생 surface 에 파란 테두리, 포커스 시 자동 해제 — 또는 그
  surface 발 알림을 읽음 처리(개별/모두 읽음)했을 때 그 surface 에 남은 안읽음 알림이 없으면
  해제(같은 surface 의 다른 알림이 아직 안읽음이면 유지). 단 그 surface 가 **하드 점유
  (attach)** 중이면 해제되지 않는다 — 점유 중 attention 의 해제 주체는 홀더다
  ([Surface Highlight (주의 환기)](../surface-highlight/index.md)). **알림 자체의
  읽음 처리는 점유와 무관하게 그대로 동작한다** — 게이트가 걸리는 것은 attention 해제
  하나뿐이다. 상세 [`surface-highlight`](../surface-highlight/index.md).
- **사이드바 배지**: attention surface 가 있는 워크스페이스 행 우측에 `attention_count` 개수 pill
  배지(`paint_workspace_count_badge`, 99 초과 시 "99+" — 확장 사이드바는 이름 우측 숫자 배지, 축소
  사이드바는 dot). 모두 방문하거나 읽음 처리하면 소멸.
- **LNB 알림 요약**: 도구/플러그인/설정 바로 위의 별도 버튼이다. 현재 창의 미확인 알림을 오류(빨강), 경고(노랑), 정보(완료 attention과 같은 파랑) 순서로 글리프와 숫자를 함께 표시한다. 0은 숨기고 99 초과는 `99+`로 표시한다. full은 제목·배지 아래에 가장 최근 생성된 미확인 항목의 제목·시간을 한 줄로 보여주며, 미확인이 없으면 한 줄로 접힌다. collapsed는 벨 아래 세로 배지를 쌓는다. 클릭하면 기존 알림 팝업을 토글하며 알림 도착만으로 팝업이나 사용자 포커스를 바꾸지 않는다. 워크스페이스 행의 attention 배지와 별도로 집계한다.
- **알림 패널** (Popup, Window 스코프): 최신순 목록, 워크스페이스·제목·본문·경과시간 + "Jump" 버튼. 열거나 닫는 것만으로 읽음 처리하지 않는다. "Mark all read"는 현재 창의 알림 ID 집합에만 적용하고 다른 창의 알림은 보존한다. 레벨 글리프·미확인 점·개별 읽음 버튼을 제공한다. 본문은 popup에서 2줄, fullscreen에서 3줄로 접으며 넘칠 때만 더 보기 링크를 표시한다. 행 또는 더 보기를 눌러 펼치면 해당 항목만 읽음 처리하고 전문을 선택할 수 있다. Jump는 출처 surface를 드러내거나 surface 없는 알림의 workspace로 이동한다. 출처가 닫혀도 기록은 유지하며 닫힘 표시와 비활성 Jump를 보여준다. 출처 workspace가 사라졌으면 이름은 `notification_panel.unknown_workspace` 번역값이다. 목록을 스크롤한 상태에서 새 항목이 도착하거나 같은 ID에 병합된 내용으로 위쪽 행 높이가 바뀌어도 보던 항목과 오프셋을 유지한다. Popup 이라 터미널 입력을 차단하지 않고 워크스페이스 전환과 무관하게 보임([popup](../../design/systems/popup.md)).

### 알림 레벨과 미확인 집계

알림의 `level`은 로그 레벨이나 `AttentionKind`와 별도다. `error`는 크리티컬한 실패,
`warning`은 문제가 있지만 크리티컬하지 않은 실패, `info`는 기능이 의도적으로 전달하는 정보다.
생성 시 생략한 레벨은 `info`이며 잘못된 값은 오류로 반환한다. 기존 OSC·벨·승인 요청은
정보 알림이고 telemetry anomaly/cap은 경고다. 임의 터미널 출력을 분석해 실패를 추정하지 않는다.

`notification.counts`는 모든 main/parked engine의 전체 보관 항목에서 미확인 수를 합산해
`{error, warning, info, total}`을 반환한다. 목록의 50개 응답 상한과 무관하다.
선택적 `workspace_id`는 출처 필터이며 닫힌 워크스페이스의 보관 항목에도 적용한다.
이 수는 완료·입력대기 surface 수를 합산하거나 대체하지 않는다.

`notification.read`는 `notification_id` 하나 또는 `all:true` 중 하나를 반드시 받는다.
선택적 `workspace_id`로 출처 범위를 좁힌다. `all:true`에 필터가 없으면 인스턴스의 모든
engine이 대상이다. 응답은 `{read:true, changed:N}`이며 이미 읽은 항목은 변경 수에 넣지 않는다.
ID가 해당 범위에 없으면 오류다. `idempotency_key`를 붙인 읽음은 GUI와 headless 모두
첫 응답을 보관한다. GUI에서는 engine 핸들러 전에 실행하므로 App 층의
`Kept { since: 2 }`를 선언하고 공통 저장소의 begin/finish를 사용한다. 같은 키·같은 인자의 재시도는 `idempotent_replay:true`로 원래 응답을
돌려주며 그 사이 도착한 알림을 읽지 않는다. 같은 키에 다른 인자를 보내면 `-32063`이다.
읽음은 기존 surface attention 해제 규칙을 따르며 hard 점유
surface는 해제하지 않는다. 사용자 포커스·팝업 열림 상태는 바꾸지 않는다. headless도 같은 계약이다.

`notification.created` 이벤트에도 `level`을 전달한다. 새 CLI의 error/warning 생성은
`notification.levels` capability를 확인해 구 서버가 레벨을 무시하는 경우 전송 전에 거절한다.
기존 info 생성 호출은 이 추가 capability를 요구하지 않는다.

## 인터페이스

- **사용자**: LNB 요약 클릭 또는 설정 가능한 단축키로 알림 패널 토글, 행 펼치기/개별 읽음/Jump/Mark all read. 기존 fullscreen 전환을 지원한다.
- **AI Agent / CLI**: `notification.list` / `notification.create`(workspace_id/surface_id 라우팅, 포커스 비의존). `notification.create` 의 `title` 은 선택 — 생략하면 호스트가 `notification.default_title` 번역값을 채운다(`tasty notify <body> [--title ..]`). 기본 제목은 프로토콜 토큰이 아니라 UI 문구이므로 언어에 따라 달라진다.

```sh
tasty notify "Job stopped" --level error --workspace 12
tasty notify "Retry scheduled" --level warning --surface 42
tasty notification counts
tasty notification counts --workspace 12
tasty notification read 7
tasty notification read --all --workspace 12
```

`notify`는 명시적 `--surface` 또는 `--workspace`를 받는다. workspace를 지정하면
환경의 surface ID를 자동으로 더하지 않는다. 둘 다 지정하면 소속을 검증한다.
둘 다 생략하면 기존 `TASTY_SURFACE_ID` 기본값을 유지한다.

## 비-목표

- **busy indicator**(실행 중 표시)·**도구 메뉴**·**토스트** 는 별도 — 각각 [busy-indicator](../../design/policies/busy-indicator.md)·[tools-menu](../tools-menu/index.md)·[toast](../../design/systems/toast.md).

## 관련

- [design/systems/popup](../../design/systems/popup.md) · [design/systems/toast](../../design/systems/toast.md) · [settings](../settings/index.md)

목록의 warning 글리프는 `notification_item_warning_glyph()` → `accent_warning_glyph()`를 팝업과 전체화면에서 공유한다. 읽음·미읽음은 같은 색이며, Latte에서는 기존 yellow보다 어두운 전경으로 휴지·hover의 3:1 목표를 유지한다. 배지와 error·info 역할은 별개다.
