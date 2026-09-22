# 레이아웃 영속화 (Layout persistence)

- **Status**: Implemented
- **주체**: 로컬 사용자 (설정 토글)
- **ADR**: [0087](../../adr/0087-layout-slot-occupancy-model.md) — 슬롯 점유 모델
- **코드**: `src/core/layout_persistence/`, `~/.tasty/layouts/NN.json` · `~/.tasty/scrollback/<id>.bin`
- **화면**: 없음 (앱 시작 시 자동 복원)

## 목적

`general.restore_layout`(기본 **on**) 활성 시 워크스페이스/페인/탭/서피스 구조를 **슬롯 파일** `~/.tasty/layouts/NN.json` 에 저장하고, 앱 시작 시 복원해 이전 세션 창 배치를 재현한다.

## 내부 동작

### 저장 대상 / 타이밍

워크스페이스(이름·부제·설명) · 페인 트리(split direction/ratio) · 탭(이름·active) · 서피스 레이아웃 트리 · 서피스 타입별 최소 정보(Terminal: cwd·`restore.command`·`scrollback_ref` / Markdown·Image: path / Explorer: root / Html: url) · 활성 워크스페이스·포커스 페인 인덱스. 구조 변경 시 dirty + **500ms 디바운스**, 종료 시 dirty 면 즉시 flush.

복원은 engine(=창) 하나가 만들어질 때 1회 — 앱 시작 시의 첫 창과 이후 새로 여는 창 모두. 파싱 실패/파일 없음 → 기본 "Workspace 1" 로 시작. 개별 서피스 복원 실패 시 그 서피스만 스킵하고, 워크스페이스를 **하나도** 복원하지 못하면 복원 적용 직후의 안전망이 기본 "Workspace 1" 하나를 만든다(빈 창이 뜨지 않는다).

복원할 레이아웃이 있는 engine 은 그 기본 워크스페이스를 **미리 만들지 않는다**. 미리 만들면 딸려 spawn 된 셸 프로세스가 복원 후 어떤 워크스페이스에도 속하지 않은 채 남아 회수되지 않는다(engine 하나당 셸 하나 누수). 복원이 끝난 뒤에 창의 view 상태를 조립하므로 그 사이에 빈 화면이 보이지도 않는다.

### 슬롯 파일

레이아웃은 `~/.tasty/layouts/NN.json` 슬롯 파일 하나 = engine(=창) 하나의 전체 상태다(워크스페이스 목록 · 활성 워크스페이스 · 카테고리). 슬롯 목록과 순서는 파일명의 숫자에서 전부 파생되며 별도 인덱스 파일을 두지 않는다 — 인덱스는 실제 파일과 desync 되는 두 번째 진실원이 된다. 번호는 2자리 zero-pad(`01.json`)이고 100 이상은 자연 확장된다.

write 는 `NN.json.tmp` 에 쓴 뒤 rename 하는 **원자적** 교체다. 슬롯이 여러 개이므로 잘린 JSON 하나가 아래 scrollback 정리를 통해 다른 슬롯의 `.bin` 까지 잃게 만들 수 있다.

저장은 각 engine 이 자기 슬롯 파일에만 한다 — 종료 시의 일괄 flush 도 창마다 자기 파일로 나뉜다. 창 사이에 덮어쓰기가 없다는 것이 이 모델의 출발점이다([ADR-0087](../../adr/0087-layout-slot-occupancy-model.md)).

**레거시 마이그레이션** — 단일 파일 시절의 `~/.tasty/layout.json` 은 부팅 1회 `layouts/01.json` 으로 **이동**(rename)된다. `layouts/` 가 이미 있으면 이동하지 않고 남은 레거시 파일을 로그로 알린다.

### 읽지 못한 슬롯

**"슬롯이 없다" 와 "슬롯을 못 읽었다" 는 다르게 다룬다.** 둘을 같은 값으로 뭉개면 권한 오류나
손상된 JSON 이 "이 슬롯을 쓴 적 없음" 과 같아지고, 그 창이 이어서 자기 상태를 같은 슬롯에 저장해
사용자의 창 구성을 대체한다.

| 상태 | 그 슬롯에 저장 | 원본 |
|------|----------------|------|
| 없음 | 한다 | — |
| 파싱 실패 | 한다 — **저장 직전에** `NN.json.bak` 으로 옮긴 뒤 쓴다 | 백업에 남는다 |
| 파싱 실패 + 백업 실패 | **안 한다** | 그 자리에 그대로 |
| 읽기 실패 (권한 · IO) | **안 한다** | 그 자리에 그대로 |
| version 이 이 빌드보다 높음 | **안 한다** | 그 자리에 그대로 |

**읽기는 파일을 건드리지 않는다.** 보존은 실제로 덮어쓰려는 순간에 한다. 부팅 중 이 슬롯을 읽는
곳이 하나가 아니고(scrollback GC 와 engine 복원), 런처와 GUI 는 서로 다른 프로세스라 — 읽는 쪽이
파일을 옮겨버리면 나중에 읽는 쪽은 그저 "파일 없음" 을 보게 되어, 정작 사용자에게 알릴 프로세스가
사건을 모르는 상태가 된다.

읽기 자체가 실패한 파일은 내용을 확인하지 못한 것이므로 옮기지 않는다 — 일시적 오류에 사용자
레이아웃이 자리를 뜨면 안 된다. 미래 version 슬롯을 백업하지 않는 이유도 같다: 파일은 멀쩡하고
새 버전이 읽을 수 있으므로, 구버전으로 한 번 켰다고 신버전의 레이아웃이 사라지면 안 된다.

백업 파일명은 `NN.json.bak` 이고 이미 있으면 `NN.json.bak.2` … `NN.json.bak.9` 로 늘어난다.
9개가 차면 백업을 만들지 않고 저장을 막는 쪽을 택한다. 저장이 막힌 창에는 토스트로 알린다.

### 슬롯 배정

창 ↔ engine ↔ 슬롯은 1:1 이다. 창을 새로 열면 이미 다른 창이 쓰고 있는 슬롯이 아니라 **다음 free 슬롯**을 잡으므로, 두 창이 같은 레이아웃을 복제해 보여주지 않는다.

규칙 — **실제 존재하는 슬롯 파일** 중 점유되지 않은 가장 낮은 번호를 쓴다.

- 번호 공백은 채우지 않는다: 파일이 `02.json`·`03.json` 뿐이고 점유가 없으면 답은 `1` 이 아니라 **2** 다. 저장된 레이아웃을 건너뛰고 빈 창을 띄우지 않기 위한 것이다.
- 기존 슬롯 파일이 전부 점유면 `max(파일 슬롯 ∪ 점유 슬롯) + 1` 로 **새 슬롯**을 만든다. 파일이 없으므로 그 창은 기본 워크스페이스 하나로 시작한다.
- 파일도 점유도 없으면(첫 설치) `1`.

저장된 슬롯이 여러 개여도 **부팅 시 창은 1개**다. 나머지 슬롯은 free 로 남아 있다가 창을 더 열면 순서대로 복원된다.

headless(`--headless`)는 레이아웃 복원을 적용하지 않으므로 슬롯을 점유하지도, 저장하지도 않는다.

### 슬롯 점유

점유는 **휘발성**이다 — 디스크에 기록하지 않고 별도 레지스트리도 두지 않는다. 살아있는 engine 들이 들고 있는 슬롯 번호를 모은 것이 곧 점유 집합이므로, 재시작하면 전부 free 로 돌아온다(크래시가 슬롯을 영구 점유로 남기지 않는다). 창이 닫혔다가 되살아나는 parked engine 은 자기 슬롯을 그대로 이어쓴다 — 재배정하면 남의 슬롯 파일을 덮어쓴다. 구조적 배경은 [멀티 윈도우 아키텍처](../../architecture/multi-window.md).

#### 점유 조회

어느 창이 어느 슬롯을 쓰는지는 `window.list` IPC(= `tasty list windows`)의 `layout_slot` 필드로 본다. 슬롯 점유는 "새 창을 열면 어떤 레이아웃이 뜰지" 를 결정하는 상태라, 관측 수단이 없으면 에이전트가 창 생성 결과를 예측·검증할 수 없다.

```json
[
  { "id": 1, "focused": true,  "title": "Tasty",       "layout_slot": 1 },
  { "id": 2, "focused": false, "title": "Workspace 1", "layout_slot": 2 }
]
```

`layout_slot` 은 슬롯을 잡지 않는 engine 에서 `null` 이다(headless). 순수 read 라 포커스·선택 등 사용자 상태를 건드리지 않는다.

**parked engine 은 이 목록에 없다.** 파킹된 engine 도 슬롯을 점유하지만 창이 아니어서 창 id 가 없고, `window.list` 의 `{id, focused, title}` 계약이 깨진다. 따라서 이 목록의 `layout_slot` 집합은 살아있는 창의 점유일 뿐 **점유 집합 전체가 아니다** — 파킹분까지 봐야 할 일이 생기면 별도 조회 메서드로 분리한다.

### 창 닫힘 시 슬롯 처리

창을 닫으면 그 engine 이 drop 되며 슬롯 점유가 자동으로 풀린다(점유가 살아있는 engine 에서 파생되므로 별도 해제 호출이 없다 = 누락으로 인한 슬롯 누수도 없다). drop 직전에 슬롯 파일을 어떻게 할지는 `general.restore_layout` 하나로 갈린다.

| `restore_layout` | 슬롯 파일 |
|---|---|
| **on** | 강제 flush 후 **보존**. 디바운스 500ms 를 무시하므로 워크스페이스를 추가한 직후 닫아도 그 변경이 남고, 새 창이 그 슬롯을 잡으면 레이아웃이 되살아난다. |
| **off** | **삭제**. |

마지막 창을 닫아 engine 이 파킹되는 경우(그리고 macOS 최소화)는 engine 이 살아 있으므로 이 처리를 하지 않는다 — 슬롯 점유가 유지되고 다시 창을 열면 같은 슬롯을 이어쓴다.

닫힌 창의 scrollback `.bin` 은 따로 지우지 않는다: 보존 분기에선 슬롯 파일이 계속 참조하고, 삭제 분기에선 참조가 사라져 다음 부팅의 union 정리가 회수한다.

### Surface 내용 복원 (현재: 터미널 scrollback)

`general.restore_surface_content`(기본 **on**) 시 각 터미널의 scrollback + 현재 화면 라인을 `~/.tasty/scrollback/<persist_id>.bin`(magic `TSSB`)에 보존 → 재시작 후 위로 스크롤하면 [이전 scrollback → 이전 화면 → 새 prompt] 순. `persist_id` 는 surface-meta(`scrollback.persist_id`)에 보관, 같은 surface 면 atomic 덮어쓰기(orphan 없음). 옵션 OFF→ON 전환 시 capture/restore 스킵, ON→OFF 시 `~/.tasty/scrollback/` 전체 삭제. Lifecycle: surface 닫힘 시 `.bin` 삭제, 앱 시작 시 **전 슬롯의** `scrollback_ref` 합집합 외 `.bin` 일괄 정리(크래시 잔재) — 슬롯 하나만 보고 정리하면 다른 슬롯이 참조하는 `.bin` 을 지운다. 읽을 수 없는 슬롯이 하나라도 있으면 그 부팅에서는 정리 자체를 건너뛴다(모르면 지우지 않는다).

### Plugin surface 복원 (hello 창)

markdown·image 같은 **plugin surface** 는 호스트가 plugin 프로세스를 spawn 한 뒤 그 plugin 이 `hello` 를 보내 자기 kind 를 등록하기까지 짧은 창이 있다(부팅 부하에 따라 흔들린다). 레이아웃 복원이 이 창에 걸려 kind 가 아직 없으면, 그 surface 를 **kind/snapshot 을 보존한 placeholder 로** 복원한다 — 그 자리를 그냥 버리면 같은 pane 의 형제 tab(무고한 터미널 포함)과 상위 형제 pane 까지 함께 사라지기 때문이다. 화면에 표시될 때마다 도는 reify 가 kind 등록을 확인해 placeholder 를 실제 surface 로 채운다.

plugin 이 끝내 뜨지 않으면(프로세스가 죽었거나 매니페스트에서 그 kind 가 사라진 경우) placeholder 가 **그대로 남는다 — 의도된 동작이다.** 이 상태는 두 얼굴을 갖는다: 사용자에게는 빈 자리로 보이고, 에이전트에게는 `surface.list`·surface tree 에서 원래 kind + `ready: false` + `pending_reason: "plugin_not_loaded"` 로 보인다. "있다" 와 "쓸 수 있다" 를 응답에서 가른 것이라 — 한쪽(빈 탭 표시 또는 `ready` 플래그)만 손대면 둘이 어긋난다. 상태를 바꿀 때는 두 얼굴을 함께 본다.

### TUI 세션 복원 (`restore.command`)

claude plugin 등이 `tasty claude install` 로 SessionStart/End hook 을 걸면 세션 시작 시 `restore.command`(예: `claude -r <session-id>`)를 surface-meta 에 set. 호스트는 **agent-agnostic** 하게 `restore.command` 값만 읽어 복원에 쓴다 — 그 문자열이 무엇을 싣는지는 전적으로 plugin 소관이다. 예컨대 claude plugin 은 세션 프로필이 부착돼 있으면 `claude -r <id> --settings "<프로필 경로>"` 형태로 써서 **복원된 프로세스에도 프로필이 그대로 붙게** 한다([claude plugin](../../plugins/claude/index.md) "복원을 건너 프로필이 유지되는 방식") — 복원이 발급하는 새 surface id 때문에 surface meta 는 복원을 넘지 못하므로, 프로필을 실어 나르는 유일한 통로가 이 문자열이다. 명령 주입 타이밍은 PTY spawn 그 순간 — `TerminalConfig.initial_input` 으로 writer thread 시작 전 master fd 에 동기 write, child shell 의 첫 stdin read 에 무조건 첫 입력으로 들어감(추가 트리거 없이 spawn 과 동시 실행). 발동 경로 둘: 앱 재시작(레이아웃 복원) · [닫힌 항목 복원](../closed-tab-restore/index.md)(Ctrl+Shift+T).

## 저장하지 않는 것

현재 화면 cells(새 prompt 로 채움) · PTY 상태/환경변수/실행 중 명령 · 팝업 상태.

## 관련

- [closed-tab-restore](../closed-tab-restore/index.md) · [layout-presets](../layout-presets/index.md) · [terminal](../terminal/index.md)(scrollback)
- [ADR-0087](../../adr/0087-layout-slot-occupancy-model.md) — 슬롯 모델을 고른 이유·대안·재검토 조건
- [멀티 윈도우 아키텍처](../../architecture/multi-window.md) — 창 ↔ engine ↔ 슬롯 1:1 구조
