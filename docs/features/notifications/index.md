# 알림 (Notifications)

- **Status**: Implemented
- **주체**: 로컬 사용자 (에이전트도 `notification.create` 로 발행 가능)
- **ADR**: [ADR-0106](../../adr/0106-non-widget-user-strings-go-through-i18n.md) (제목은 표시 전용 · `notification.create` 기본 제목은 UI 문구)
- **코드**: `NotificationStore` (notification 모델); IPC `notification.{list,create}`
- **화면**: 알림 패널 popup (Window 스코프) · 사이드바 배지

## 목적

터미널이 보낸 OSC 알림 시퀀스와 시스템 이벤트를 모아 **인앱 알림 패널 + 시스템 OS 알림 + 사운드 + surface 하이라이트 + 사이드바 배지**로 노출한다.

## 내부 동작

### OSC 시퀀스 감지

termwiz Parser 의 OSC 액션을 인터셉트해 알림 이벤트 생성 — OSC 9(iTerm2/ConEmu), OSC 99(Kitty), OSC 777(rxvt), BEL. (OSC 7=cwd 변경, OSC 0/2=타이틀 변경은 알림이 아닌 별도 처리.)

**벨(BEL) 토글**: BEL 알림(제목은 `notification.bell_title` 번역값)은 전역 `notification.enabled` 위에 벨 전용 `general.bell_notification`(기본 on)을 한 겹 더 얹어 게이트한다. off 면 토스트를 억제하되, 사용자가 등록한 `bell` 훅은 그대로 발화한다(훅=명시적 자동화 → 수동 반응인 토스트와 분리). `cascade_terminal_bell_ring` 참조.

### 제목은 표시 전용 — 식별은 별도 필드로

호스트가 스스로 만드는 알림 제목은 전부 `t()` 번역값이라 `general.language` 에 따라 달라진다 — 벨은 `notification.bell_title`, `notification.create` 가 `title` 없이 호출되면(`tasty notify "본문"`) `notification.default_title`. 따라서 제목 문자열은 **기계적 식별자가 아니다**. 알림의 출처를 구분해야 하는 소비자는 식별 필드를 본다 — 훅은 `HookEvent::Bell`(셸 env `TASTY_HOOK_EVENT=bell`, payload 에 제목 없음 — [hooks](../hooks/index.md)), plugin 은 `notification.created` 이벤트의 `source`. 제목 비교(`title == "Bell"`)로 분기하는 코드는 언어를 바꾸는 순간 깨지므로 두지 않는다. 근거·대안: [ADR-0106](../../adr/0106-non-widget-user-strings-go-through-i18n.md).

### NotificationStore

VecDeque FIFO(최대 100, 초과 시 `pop_front` O(1)). **병합(coalescing)**: 같은 source 에서 설정 간격(기본 500ms) 내 연속 알림은 기존에 합침. 개별/전체 읽음 처리. 신규 알림 발화 시 그 source surface 를 attention 발동한다 — toast 는 attention(주의 환기)의 **producer 중 하나**이며, attention 상태 자체는 NotificationStore 가 아니라 producer 중립 공유 primitive(CoreState `attention: AttentionStore`)에 있다 — 알림 레코드가 곧 attention 레코드는 아니다(별개 저장소). 사이드바 워크스페이스 배지는 워크스페이스별 unread 개수가 아니라 `attention_count`(surface 단위 attention 개수) 기준이다. 상세 [`surface-highlight`](../surface-highlight/index.md).

### IPC 목록의 범위·ID·순서·상한

`notification.list`는 모든 main/parked engine의 알림을 합쳐 반환한다. headless는
하나의 engine에 같은 규칙을 적용한다. UI 패널과 저장소는 여전히 창별이고, 각
저장소의 FIFO 보존 상한 100개와 읽음 상태는 공유하지 않는다.

ID는 인스턴스의 공유 IdGenerator에서 발급하는 u64이며 재시작 동안 보존되는 ID가
아니다. 목록은 ID 내림차순, 즉 **신규 생성 순서의 역순**으로 전체 최대 50개다.
창 순회 순서나 포커스는 결과를 바꾸지 않는다. coalescing은 기존 ID와 생성 순서를
유지하므로 최근 내용 갱신이 항목을 앞으로 옮기지는 않는다. 기존 한 engine의 순서와
고정 50개 응답 계약을 유지하며 새로운 limit/필터 인자를 도입하지 않는다.

이는 [목록 합산 원칙](../../adr/0175-window-owned-list-membership-is-judged-by-shape-not-by-name.md)의
적용이며, GUI 패널을 전역 패널로 바꾸는 결정이 아니다.

### 시스템 알림 + 사운드

윈도우 비활성 시 OS 네이티브 알림(notify-rust, 초당 1회 rate limit). `notification.sound` 가 true 면 신규 알림 발화 시 OS beep 1회(macOS `NSBeep` / Windows `MessageBeep` / Linux `paplay→aplay→\a` 3단 폴백, headless 는 Noop). coalesce 로 묶인 알림은 host event 미생성이라 자동 비음. 터미널 `\a`(Bell)는 OS 가 자체 beep 할 수 있어 안전 default 로 skip.

### 시각 표시

- **surface 하이라이트**: 알림 발생 surface 에 파란 테두리, 포커스 시 자동 해제 — 또는 그
  surface 발 알림을 읽음 처리(개별/모두 읽음)했을 때 그 surface 에 남은 안읽음 알림이 없으면
  해제(같은 surface 의 다른 알림이 아직 안읽음이면 유지). 단 그 surface 가 **하드 점유
  (attach)** 중이면 해제되지 않는다 — 점유 중 attention 의 해제 주체는 홀더다
  ([ADR-0109](../../adr/0109-hard-occupancy-attention-clear-holder-only.md)). **알림 자체의
  읽음 처리는 점유와 무관하게 그대로 동작한다** — 게이트가 걸리는 것은 attention 해제
  하나뿐이다. 상세 [`surface-highlight`](../surface-highlight/index.md).
- **사이드바 배지**: attention surface 가 있는 워크스페이스 행 우측에 `attention_count` 개수 pill
  배지(`paint_workspace_count_badge`, 99 초과 시 "99+" — 확장 사이드바는 이름 우측 숫자 배지, 축소
  사이드바는 dot). 모두 방문하거나 읽음 처리하면 소멸.
- **알림 패널** (Popup, Window 스코프): 최신순 목록, 워크스페이스·제목·본문·경과시간 + "Jump" 버튼. 열 때 전체 읽음, "Mark all read". 출처 워크스페이스가 이미 닫힌 알림은 워크스페이스명 자리에 `notification_panel.unknown_workspace` 번역값을 표시한다. Popup 이라 터미널 입력을 차단하지 않고 워크스페이스 전환과 무관하게 보임([popup](../../design/systems/popup.md)).

## 인터페이스

- **사용자**: 단축키로 알림 패널 토글, Jump/Mark all read.
- **AI Agent / CLI**: `notification.list` / `notification.create`(workspace_id/surface_id 라우팅, 포커스 비의존). `notification.create` 의 `title` 은 선택 — 생략하면 호스트가 `notification.default_title` 번역값을 채운다(`tasty notify <body> [--title ..]`). 기본 제목은 프로토콜 토큰이 아니라 UI 문구이므로 언어에 따라 달라진다.

## 비-목표

- **busy indicator**(실행 중 표시)·**도구 메뉴**·**토스트** 는 별도 — 각각 [busy-indicator](../../design/policies/busy-indicator.md)·[tools-menu](../tools-menu/index.md)·[toast](../../design/systems/toast.md).

## 관련

- [design/systems/popup](../../design/systems/popup.md) · [design/systems/toast](../../design/systems/toast.md) · [settings](../settings/index.md)
