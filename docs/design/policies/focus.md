# 포커스 정책 (운영 상세)

> 정체성 차원의 근거는 [identity §2.3 포커스 독립성](../../identity.md). 본 문서는 *현재 운영 동작* 만 기술한다. 계층 용어(View)는 [concepts/hierarchy](../../concepts/hierarchy.md).

**포커스(활성 윈도우/탭/워크스페이스/Pane/Surface)는 사용자의 것**이다 — 사용자가 지금 무엇을 보고 어디에 입력하는지의 시점. 에이전트 행동(IPC/CLI)은 포커스를 바꾸지 않으며, release 엔 포커스 변경 API 가 없다.

## 계층

```
Engine
└── View (여러 개, HashMap<WindowId, …>)
    ├── ModalView    — 활성 시 모든 입력 독점 (엔진 전역 최대 1개)
    └── 그 외 (MainView / PresetView 등) — Modal 없을 때 OS 네이티브 포커스
        └── Pane / Surface — View 내부 포커스
```

## Modal 포커스 차단

앱(ViewRegistry)이 `active_modal_id: Option<WindowId>` 를 보유한다.

- **Modal 없음**(`active_modal_id == None`): 각 View 는 OS 네이티브 포커스를 따른다. View 들은 독립적으로 포커스를 받고(z-order 독립), 사이에 다른 앱 창이 있을 수 있다.
- **Modal 있음**(`active_modal_id == Some(id)`): 이벤트 디스패처가 각 View 에 `modal_active: bool` 을 전달한다. Modal 이 아닌 View 는 입력 이벤트를 무시하고(`Resized`/`RedrawRequested`/`ScaleFactorChanged`/`ModifiersChanged`/`Focused` 만 통과), Modal View 만 `modal_active: false` 로 받아 정상 동작한다. Modal 을 닫으면 기존 포커스로 자연 복귀한다.

| 상태 | 구현체 | 동작 |
|----------|--------|------|
| Modal | `SettingsView` · `PluginsView` · `QuitView` (`ModalView`) | 전체 입력 차단, 닫기 전까지 다른 조작 불가 |
| Modeless | `MainView` · `PresetView` (`View` + `sealed::Sealed` 직접 구현) | 독립 포커스, 다른 윈도우와 공존 |

**OS 네이티브 윈도우 비활성화(Win32 `EnableWindow` 등)는 쓰지 않는다** — 플랫폼별 동작 차이로 크로스플랫폼 일관성이 깨진다. 앱 레벨 `modal_active` 게이트로 처리한다.

## View 내부 포커스

Modal/View 레벨과 별개로, 각 View 내부에서 Pane 간·Surface 간 포커스 이동과 탭 전환이 일어난다 (단축키/클릭). 단축키는 [`KeybindingSettings`](key-mapping.md) — 하드코딩 아님. 이 내부 포커스는 그 View 가 OS 포커스를 갖고 Modal 이 비활성일 때만 동작한다.

**탭바 클릭 → 그 pane 으로 focus 이동**: 콘텐츠 영역 클릭과 대칭으로, 비-focused pane 의 탭바(탭 본체·탭이 없는 빈 영역·스크롤 화살표·"+"/split/search 버튼)를 primary click 하면 그 pane 으로 focus 가 이동한다. 탭바는 그 pane 을 직접 조작하는 사용자 행위이므로 클릭 대상 pane 과 focus 가 어긋나면 안 된다(비-focused pane 의 탭을 클릭해도 탭 전환만 일어나고 focus 는 그대로 남는 것은 결함). 빈 영역 클릭은 탭 전환 없이 focus 만 옮긴다. 우클릭 컨텍스트 메뉴(탭/pane/새 탭 버튼)는 대상 `pane_id`/`tab_index` 를 메뉴 항목에 직접 실어 나르므로 focus 이동이 필요 없다 — 우클릭은 조회/메뉴-오픈이지 조작 commit 이 아니다. 구현: `src/adapters/ui/tab_bar.rs` `TabBarAction::focus_target_pane` + `src/adapters/ui/tab_bar/apply.rs` `apply_tab_bar_actions`.

이 규칙은 **사용자 마우스 클릭**에 의한 focus 이동이므로 아래 "CLI/IPC 포커스 독립 원칙"(에이전트/명령 유래 focus 강제 금지)과 별개다 — 혼동 금지. 그 원칙은 IPC/CLI 명령이 focus 를 대상 결정 수단으로 쓰거나 강제 변경하는 것을 막는 것이지, 사용자가 GUI 를 직접 클릭했을 때 그 결과로 focus 가 따라가는 것을 막지 않는다.

## CLI/IPC 포커스 독립 원칙

**focus 는 사용자의 시선·관심을 나타내는 독립 행위이며, IPC/CLI 명령의 대상을 결정하는 수단이 아니다.** focus 를 대상 결정에 쓰면 race condition 이 내재한다(명령 발행 후 실행 전 사용자가 focus 를 옮기면 엉뚱한 대상에 실행; 에이전트가 다른 작업으로 focus 를 옮기면 의도 붕괴).

따라서:

- **IPC/CLI 로 focus 를 변경할 수 없다.** focus 변경 API(`surface.focus` / `pane.focus` / `workspace.select` / `focus.direction`)는 release 에 없다(제거됨). focus 는 오직 사용자 행위(단축키·마우스)로만 바뀐다.
- 모든 명령은 대상을 **ID 로 직접 지정**한다. `list` 는 **전 워크스페이스 순회**(활성 상태 비의존).
  - 순회하는 `list` 는 호스트가 명시적으로 합산하는 것뿐이다(`src/app/dispatch/list_global.rs`). **그 집합의 소속은 이름이 아니라 성질로 판정한다**(핸들러가 창 소유 컬렉션을 순회하는가 · 대상 인자가 없는가 · 합산 집합에 없는가) — 이름 모양(`*.list`)으로 훑는 눈에는 `tree` 가 안 걸려 오래 빠져 있었다([ADR-0175](../../adr/0175-window-owned-list-membership-is-judged-by-shape-not-by-name.md)). **그 목록에 없는 `list` 는 포커스된 창의 것만 답하고, 에러가 없다.** 실측(창 둘): 창1 에서 만든 headless pty 가 창2 포커스의 `pty.list` 에 안 나오는데 `pty.read {id}` 는 그 pty 를 읽었다 — **조작할 수 있는데 볼 수 없는** 상태다. 창 소유 자원의 `list` 를 새로 만들면 거기에 등록한다.
  - `notification.list`도 main/parked engine의 알림을 합산한다. ID는 공유
    IdGenerator에서 발급하고 생성 순서 역순으로 전체 50개를 반환한다. 병합은 기존
    ID와 생성 순서를 유지한다. UI 알림 패널과 읽음 처리는 각 engine 소유로 남는다.
    소속 명부는 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs`다.
    - **`approval.list` 는 창별이 아니었다.** `CoreState` 생성자만 읽으면 engine 마다
      `Arc::new` 라 창별로 보이는데, 두 번째 main window 를 세우는
      `App::ensure_engine_and_plugins`(`src/app/window_lifecycle.rs`)가 첫 engine 의
      `approval_store` Arc 로 덮어쓴다. **저장소가 어디 사는지는 생성자와 창 생성
      경로를 함께 읽어야 정해진다** — 한쪽만 읽으면 공유를 창별로도, 창별을 공유로도
      잘못 읽는다. 같은 경로가 `surface_registry` · `file_format` · `file_handler` ·
      `identify_worker` · `telemetry_seq` · `anomaly_detector` · `agent_seq` ·
      `next_ids` 도 함께 넘긴다.
    - **`attach.list` 는 합산으로 갔다.** 점유 레지스트리는 engine 별이지만 두 배열의
      키(`surface_id`·`workspace_id`)가 `IdGenerator` 공유라 이어 붙이면 그대로 키가
      되고, 지목 축이 막혀 있지 않아 합산만으로 답이 닫힌다. 합치기 전에는 다른 창의
      점유가 보이지 않아, 그 목록을 free 로 읽은 호출자가 **이미 점유된 surface 를
      집으러** 갔다. 결과가 이름 붙은 배열 둘이라 합산은 engine 을 한 번만 돌며 둘을
      함께 꺼낸다 — 필드마다 따로 돌면 한 응답의 두 배열이 서로 다른 시점의 스냅샷이
      된다.
    - **`hook.list` · `global_hook.list` 는 합산으로 갔다.** 선행 조건이던 id 공간이
      닫혔기 때문이다 — `IdGenerator` 의 `hook`·`global_hook` 카운터가 공유 `Arc` 라
      두 창의 훅이 같은 id 를 받지 않는다. 지목은 합산과 별개 축이고
      `Kind::Hook`·`Kind::GlobalHook` 이 각각 푼다.
  - **번들 plugin 이 점유한 namespace 아래의 host `list` 도 합산 대상이다.** `image.list` 가
    그 형태다 — 외부 호출은 step 5 의 plugin namespace forward 가 먼저 집지만, plugin 이
    그 메서드를 자기가 답하지 않고 trampoline 으로 host 에 되돌리고(`host.call`) 그
    되돌림은 forward 단계가 없는 경로로 들어와 합산 지점을 지난다. **plugin 이 앞에
    선다는 것은 합산 면제의 근거가 아니다** — 면제 여부는 그 host 핸들러가 창 소유
    컬렉션을 순회하는가로만 정해진다. 실측(창 둘): 두 창에 이미지를 하나씩 열면
    `image.list` 가 둘을 답하고(서로 다른 `surface_id`), 창 하나를 닫으면 다시 하나다.
    합산 arm 을 지우고 같은 상태를 재면 **하나만** 답하며 그 하나는 요청을 받은 engine
    의 것이다 — 그 둘이 합산이 만든 값이라는 뜻이다. 되돌림 경로가 실제로 도는지는
    **실패 응답의 SDK 래퍼**(`host call '<call#N>' failed: …`)로 갈랐다. 성공 응답은
    host 것이 그대로 통과해 래퍼가 없으므로 성공으로는 누가 답했는지 안 갈린다.
  - **모든 창에 상수로 존재하는 예약 항목은 합산에서 한 줄로 접는다.** `workspace_category` 의 `normal`(id 0)이 그 형태다 — 창마다 하나씩 있어 합치면 같은 id 가 창 수만큼 나온다. 접은 줄은 "어느 창의 것인가" 에 답할 수 없지만(창을 안 적으면 지목이 안 되고, 하나를 고르면 거짓이다) **그 물음이 생기지 않는 항목일 때만** 접어도 된다: `normal` 은 rename·delete·move 가 전부 거부해 어떤 요청의 대상도 아니다. 대상이 될 수 있는 항목이라면 접지 말고 id 공간을 고쳐야 한다. 접은 줄의 집계 필드는 뜻을 명시한다 — 개수는 전 창 합, 위치는 고정 불변량, 접힘은 **모든** 창에서 접혀 있을 때만 참.
  - **순서를 바꾸는 명령은 대상을 id 로 받는다 — index 는 목적지에만 쓴다.** index 는 창 안의 위치라 그것만으로는 창이 정해지지 않는다. `tab.move` 가 `pane_id` + `from_index`/`to_index` 로 주인을 먼저 짚는 형태이고, `workspace.move`·`workspace_category.move` 도 `id` + `to_index` 를 받는다. 주인이 정해진 뒤의 `to_index` 는 그 창 안의 자리라 뜻이 분명하다. 종전의 index-only 형태는 호환을 위해 남아 있고 그때는 포커스된 창에 떨어진다 — 둘을 함께 주면 거절한다(어긋났을 때 조용히 한쪽을 고르지 않는다).
  - 합산이 옳으려면 그 자원의 **id 가 창을 건너 유일해야** 한다. 안 그러면 합친 목록에 같은 id 가 둘 들어가 호출자가 어느 쪽도 지목할 수 없다. 유일성은 `IdGenerator` 가 카운터를 engine 간에 공유해 보장한다 — 그 공유가 빠져 있던 동안 두 창의 pty 가 같은 id 를 받았고, **먼저 만든 쪽은 어떤 요청으로도 닿을 수 없었다**(라우팅이 먼저 찾힌 engine 을 고른다).
- 활성 상태 *조회* 는 허용(`focused` 필드 등). 활성 상태에 *의존* 하는 동작은 금지.
- target 미지정 명령은 **에러 + 사용법 안내**(silent fallback 금지). 호출자는 조회(`list surfaces` 등)로 ID 를 확인해 전달한다. 리소스 생성 명령은 응답에 생성된 ID 를 포함한다.
  - `terminal.kill`/`terminal.release`/`terminal.respawn`/`terminal.broadcast` 는 `--surface`(parent) 를 생략하면 host 가 "현재 engine 에 등록된 parent 가 정확히 1개"일 때만 그 parent 로 폴백한다(단일 윈도우 세션의 하위 호환). main window 가 **2개 이상** 열려 있는데 이 4개 메서드가 `--surface` 없이 호출되면, 어느 window 를 봐야 하는지 자체가 정해지지 않으므로 focused window 로 조용히 새지 않고 명시적 에러로 거부한다(`src/app/request_owner.rs` `find_request_owner`/`ambiguous_parent_fallback_requires_surface`).
- **요청은 자기가 실은 ID 가 가리키는 창으로 간다.** 창이 여럿일 때 요청의 주인 창을 못 찾으면 라우터는 마지막 수단으로 **포커스된 창**에 넘긴다. 그 폴백이 답이 되는 순간 그 메서드는 포커스 의존이 되고, 증상은 에러가 아니라 "다른 창에서 not found" 라 원인이 라우팅이라는 것이 드러나지 않는다. 그래서 요청이 실은 id 로 주인 창을 찾는 범위를 넓게 잡는다 — surface·workspace·pane·tab, headless PTY, surface hook, **global hook**, output observer, preset capture 의 source, `split` 의 `target_surface`/`target_pane` 까지. global hook 이 뒤늦게 들어온 이유는 그 이름이 "창에 안 매인다" 로 읽혔기 때문이다 — 실제로는 그 매니저가 engine 마다 하나라 창 소유이고, 그 오독이 라우팅을 비워 두는 동안 다른 창의 global hook 은 **존재하는데 어떤 요청으로도 닿지 않았다.** 자원이 창에 매이는지는 이름이 아니라 **그 저장소가 어디 사는지**로 판정한다 — 같은 원리를 합산 집합의 소속에 적용한 것이 위 [ADR-0175](../../adr/0175-window-owned-list-membership-is-judged-by-shape-not-by-name.md) 이고, 두 자리에서 각각 나왔다는 것은 **이 저장소가 반복해서 밟는 형태**라는 뜻이다.
  - **인식은 핸들러가 그 키를 읽는 형태까지 따라간다.** `split` 의 `target_surface` 가 그 예다: 인자가 id 와 nickname 을 함께 받는 자리라 CLI 는 값을 항상 **문자열로** 싣는다. 숫자만 보는 인식은 그래서 CLI 로 들어온 split 을 하나도 못 풀고, 라우팅이 아니라 핸들러가 답을 낸다 — 증상은 `"surface N not found"` 이고 그 surface 는 **다른 창에 멀쩡히 살아 있다.** 숫자로 안 읽히는 값(=nickname)은 memory store 를 봐야 풀리므로 순수 라우팅 밖에서(`App::find_request_owner`) 이어 푼다. nickname→surface 매핑은 창에 안 매여 있어 창을 건너 정확히 풀린다. **라우팅이 핸들러보다 더 관대하면 안 된다** — 핸들러가 숫자를 먼저 보므로 라우팅도 그 순서를 따른다.
  - 대상이 아닌 id 는 라우팅에 쓰지 않는다. `from_surface_id`(발신자)로 보내면 큐가 받는 쪽 engine 에 안 쌓여 **읽는 쪽이 영영 못 본다** — 조용한 폴백보다 나쁜 오배송이다. `caller_surface_id`(호출자)와 stream 클라이언트 id 도 같은 이유로 대상이 아니다.
  - **지목한 대상을 아무도 안 가졌으면 거절한다.** 예전에는 그 요청이 포커스된 창으로 갔고, 그래서 존재하지 않는 `workspace_id` 를 실은 `workspace.create` 가 포커스된 창에 워크스페이스를 만들고 **성공을 돌려줬다**. 호출자는 자기가 지목한 곳에 만들어진 줄 안다. 지금은 무엇을 못 찾았는지 말하는 에러가 나간다. 대상을 **지목하지 않은** 요청(생성 등)은 그대로 포커스된 창으로 간다 — 폴백을 통째로 없앤 것이 아니라 두 경우를 가른 것이다. 헤드리스도 **같은 판정**을 받는다 — engine 이 하나라 라우팅할 곳은 없지만 판정은 있어야 하고, 없으면 같은 요청이 조합에 따라 다르게 끝난다([ADR-0143](../../adr/0143-a-named-target-is-checked-before-the-engine-in-headless.md)). 다만 거기서는 호스트 **예약 prefix** 에 한정한다 — 예약되지 않은 prefix 는 plugin 이 답할 수 있어서, 자르면 forward 될 호출을 불러 보기도 전에 죽인다.
  - 이 인식 목록은 주석의 "확인했다" 로 유지되다 새 키 일곱 개를 놓쳤다. 지금은 핸들러 소스에서 뽑은 키 집합과 **집합 동등**으로 맞물려 있고, 대상이 아닌 키는 사유와 함께 면제 목록에 남는다(`every_id_key_a_handler_reads_is_routed_or_exempt`).
- 리소스 생성/삭제 명령이 내부적으로 focus 를 일시 이동해야 하면 작업 후 **원래 focus 를 복원**한다.
- `TASTY_SURFACE_ID` 환경변수(= "내가 있는 surface")는 focus 와 다르다. CLI `--surface` 기본값으로 쓸 수 있다.

## 폴백으로 가는 메서드는 이름과 사유로 남는다

위 폴백이 답이 되는 메서드가 **어느 것인지**를 값으로 든 자리가
`src/source_guards/unrouted_dispatch_reasons.rs` 다. 모수는 "라우팅이 인식하는 키를
하나도 안 읽는 dispatch arm" 이고 명부와 집합 동등이라, 새 메서드가 지목 없이 들어오는
것과 명부가 낡는 것을 양방향으로 잡는다. 갈래와 사유의 근거는
[ADR-0251](../../adr/0251-unrouted-dispatch-methods-carry-a-reason-not-a-predicate.md).

분류가 묻는 것은 하나다 — **주인 창이 안 정해져도 답이 옳은 이유가
무엇인가.** 저장소가 창 밖이면 고칠 것이 없고(`NotWindowOwned`), 창 소유인데 합산이
답하면 정본이 합산 명부이며(`AggregatedList`), 생성이면 실을 id 가 애초에
없다(`CreatesWithoutATarget`). 창별 읽기 전용 관측은 응답에 소유 ID와 범위를
명시한다(`ScopedObservation`). 기존 count/index를 전역값으로 바꾸지 않고 소속을
밝히는 `system.info`가 여기에 해당한다. 열린 결함이면 축을 세워야 한다(`PerWindowOpenDefect`).
남은 셋은 **이 스캔이 못 보는 자리**다: 대상 키를 serde 구조체로 읽어 안 보이는 것
(`TargetReadByDeserializer`), `request_target` 밖에서 풀려 안 보이는 것
(`RoutedOutsideRequestTarget`), 그리고 debug 표면(`DebugOnly`). 사각을 술어에서 지우는
대신 명부의 행으로 만들어 검토받게 한다.

**현재 그 명부의 열린 항목은 없다.** 이는 명부에 분류한 대상의 판정이며,
아래 별도 IPC 진입 게이트의 소비·관측 문제까지 해결했다는 뜻은 아니다.
`system.info`는 기존 count/index를 engine 관측값으로 유지하되 `scope=engine`,
`workspace_ids`, `active_workspace_id`, `layout_slot`으로 소속을 명시한다.
`workspace_id` 등 기존 대상 키로 비포커스 engine도 조회할 수 있다. `window.list`는
같은 관측 필드를 OS window ID와 함께 전 창에 대해 반환한다. 전역 workspace 목록과
총수는 기존 `workspace.list`가 소유하며 parked engine도 포함한다. 버전은 프로세스 전역이다.
`git_viewer.query`는 창별 큐를 전 main에서 수집한 뒤 전역 attach 세션에서
`local_surface_id`를 해소한다. 큐 위치만으로 다른 창을 조회한다고 판단하지 않는다.
parked 큐의 생명주기와 SSH 전송 성공은 이 대상 선택 판정과 별개다.
`file_handler.dispatch`는 명시 origin으로 초기 요청과 비동기 완료를 같은 소유 engine에
연결하고 picker 선택까지 유지한다. **에이전트** 요청이 명시 origin의 pane에 새 결과 탭을
추가할 때는 기존 활성 탭·surface 선택을 유지한다(비터미널 kind 포함). origin이 사라지면 실행하지 않고, 다른 창의
NewTab으로 폴백하지 않는다. origin 생략의 기존 사용자 경로는 유지한다
([ADR-0279](../../adr/0279-file-dispatch-retains-origin-through-completion.md)).
**이 선택 유지는 에이전트 경로에만 걸린다** — 사용자가 GUI 에서 직접 연 파일(explorer
더블클릭 · 터미널 링크 클릭 · 드롭 · 파일 피커 확정 · 링크 우클릭 메뉴)은 그 결과 탭이
**선택된다.** 라우팅은 양쪽이 같다(origin 의 pane 에 붙고, 다른 pane 이 포커스를 쥐고
있어도 그리로 새지 않는다) — 갈리는 것은 선택뿐이다
([ADR-0302](../../adr/0302-a-user-file-open-selects-its-result-tab.md)). origin 을 생략한
에이전트 요청도 새 탭을 뒤에 붙이기만 하고, 사용자가 plugin popup 에서 연 파일(markdown 파일열기
팝업)은 선택된다([ADR-0526](../../adr/0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md)). `recent.query`는 state.db에 귀속된 공유 캐시를 조회하므로 어느 창에서 호출해도
같은 종류의 최근 목록을 최신순 최대 10개 반환한다. 창이 열린 뒤 다른 창에서 기록한
파일도 반영되며, 조회는 목록 순서나 사용자 포커스를 바꾸지 않는다.

## 라우팅 아래에도 층이 하나 더 있다 — 그 층은 대상을 안 고른다

위의 폴백은 **창을 고르는** 한 층이다. 창이 정해진 뒤 핸들러가 그 창의 활성 포인터를
다시 읽는 층이 하나 더 있고, 그 사실이 여기 적혀 있지 않아 읽는 사람이 "핸들러가
`active_workspace` 를 읽는다" 를 위반으로 볼지 설계로 볼지 가를 수 없었다.

**가른 결과: 대상을 포커스로 고르는 핸들러는 없다.** 출하 코드에서 에이전트가 닿는
IPC 핸들러(`src/adapters/ipc/`)가 활성 포인터를 읽는 자리를 전수로 뽑아 쓰임새별로
가르면 다섯 부류이고, 어느 것도 요청의 **대상**을 포커스로 정하지 않는다.

| 부류 | 하는 일 | 판정 |
|---|---|---|
| 보고 | 응답에 활성 상태를 싣는다(`"focused"` · `"active"` · `active_workspace`) | 위 "활성 상태 *조회* 는 허용" 그대로 |
| 기본값 채우기 | 대상은 인자로 지목됐고, **미지정 인자**만 포커스가 채운다 — 새 탭·split 의 cwd 상속, `surface_id` 를 안 실은 `workspace.create` 의 cwd 상속, `telemetry.record` 의 workspace, `approval.request` 의 workspace(단 `surface_id` 를 줬으면 그 surface 의 워크스페이스라 포커스를 안 읽는다) | 호출자가 명시하면 안 읽는다. 명시하지 않으면 같은 인자로 두 번 불러도 값이 다를 수 있다 |
| 계측 태그 | 공통 `check_request`가 audit 귀속에 engine의 활성 workspace를 읽는다. telemetry는 같은 engine의 첫 workspace를 태그로 쓴다 | **판정에는 안 쓰인다** — 게이트가 빌린 engine의 관측 문맥이며, 요청이 작용한 대상이나 사용자의 포커스를 증명하지 않는다 |
| 알림 배치 | cap 임계·이상 탐지·승인 요청 알림이 활성 워크스페이스에 뜬다 | 사용자에게 보이라고 두는 자리라 에이전트 대상 결정이 아니다 |
| 효과 scope | `debug.host_popup.open` 의 `workspace_scope` | debug 전용. 사용자 조작 재현이라 창 종속이 뜻 자체다 |

두 번째 부류가 이 축에서 유일하게 관측 가능한 흔들림이다 — **인자를 명시하지 않은
호출은 재현 가능하지 않다.** 금지가 아니라 성질이고, 재현이 필요하면 인자를 준다.
요청이 이미 대상을 댄 자리에서는 그 대상이 귀속을 정하고, 아무것도 대지 않은 자리에만
포커스 기본값이 남는다. 자리별 갈래와 남긴 이유는 [ADR-0533](../../adr/0533-an-omitted-target-keeps-its-focus-default-only-where-nothing-names-one.md).

세 번째 부류는 **감사 로그를 workspace 로 조회할 때만 드러난다.** 행의 workspace 는
요청의 대상이 아니므로, 그 열로 "이 워크스페이스에서 무슨 일이 있었나" 를 물으면
답이 어긋난다.

### 재는 명령

    # 출하되는 줄만 남긴 사본을 만들고(테스트 코드가 같은 이름을 쓴다)
    cargo run -q -p tasty-doc-guards --bin strip-cfg-test -- \
        --blank-test-only-files <out> . src/adapters/ipc src/app
    # 다섯 포인터를 센다
    grep -rn 'focused_view_id\|active_workspace\|focused_pane\|active_tab\|focused_surface' <out>

    # 합산 집합의 소속 판정(ADR-0175) — 창 소유 컬렉션을 순회하는 핸들러를 뽑아
    # `src/app/dispatch/list_global.rs` 의 arm 과 대조한다. 대상 인자가 있는 것
    # (`surface_id` 등을 받는 것)은 라우터가 주인 창을 푸니 합산 대상이 아니다.
    grep -rn 'for ws in &engine.workspaces\|engine.workspaces.iter()' <out>/src/adapters/ipc
    grep -oE '"[a-z_.]+" =>' src/app/dispatch/list_global.rs

`focused_view_id` 는 핸들러 층에 **0** 이어야 한다 — 창을 고르는 것은 라우터의 일이고,
핸들러는 이미 정해진 창 안에서만 산다. 그 값이 0 이 아니게 되면 층이 섞인 것이다.

## 삭제로 인한 인덱스 이동에서도 포커스 대상은 보존된다

**시야가 움직이는 경우는 하나뿐 — 사용자가 보고 있던 대상 *자체* 가 사라졌을 때다.** 보고 있지 않은 워크스페이스/탭/pane 이 닫혔는데 화면이 바뀌면 결함이다. 근거 [ADR-0113](../../adr/0113-close-preserves-the-focused-target.md).

활성 포인터 셋 중 둘은 **인덱스**가 진실 소스다 — `AppState::active_workspace` 와 `Pane::active_tab`. 인덱스는 앞쪽 원소가 빠지면 손대지 않아도 **가리키는 대상이 바뀐다.** 그래서 범위 초과 clamp 만으로는 부족하고, 제거 위치를 기준으로 함께 당겨야 한다.

| 계층 | 포인터 | 제거가 앞쪽일 때 | 제거된 것이 보던 대상일 때 |
|---|---|---|---|
| workspace | `active_workspace`(인덱스) | 한 칸 당김 — 같은 워크스페이스 유지 | 그 자리로 밀려 들어온 워크스페이스(마지막이었으면 직전) |
| tab | `Pane::active_tab`(인덱스) | 한 칸 당김 — 같은 탭 유지 | 그 자리로 밀려 들어온 탭(마지막이었으면 직전) |
| pane | `Workspace::focused_pane`(id) | 그대로 — id 는 밀리지 않는다 | 생존 pane 으로 재배정 |

- 이 보정은 **origin 으로 분기하지 않는다.** 대상 기준 보정은 사용자 경로(컨텍스트 메뉴로 앞쪽 탭 닫기)에서도 옳다. origin 게이트는 "에이전트가 새로 만든 것으로 포커스를 옮기지 않는다"(`cascade_workspace_created` · `cascade_surface_split`)처럼 이동 여부가 정책적으로 갈리는 곳에만 쓴다.
- 카테고리 quick-switch 착지점(`AppState::category_last_active`)은 인덱스가 아니라 **워크스페이스 id** 를 값으로 든다. 그래서 제거·재정렬 어느 쪽으로도 밀리지 않는다 — 보정 대상이 아니다. 착지 시점에 id 로 워크스페이스를 찾고, 사라졌거나 다른 카테고리로 옮겨졌으면 그 카테고리의 first 로 폴백한다.
- 원격 attach 로 forward 된 구조 변경(`execute_forwarded_structural_op`)과 mirror 워크스페이스 teardown 도 같은 close 경로를 타므로 같은 규칙이 적용된다.

구현: tab 은 `Pane::remove_tab_preserving_active`(`crates/tasty-model/src/pane.rs`), workspace 는 `active_index_after_removal` + `AppState::fix_workspace_pointers_after_removal`(`src/state/workspace.rs`), pane 은 각 close 경로의 `was_focused` 가드. 제거 위치는 `CoreEvent::SurfaceClosed { workspace_index_purged }` 로 cascade 에 전달된다 — Core 는 `active_workspace` 를 모르고, cascade 시점엔 워크스페이스가 이미 사라져 위치를 알 수 없기 때문이다. 워크스페이스를 제거하는 **새 경로**를 추가하면 그 헬퍼를 함께 태운다.

## 자기 자신 닫기 보호 (Self-Close Protection)

**명령으로 자신이 속한 리소스를 닫을 수 없다** — 에이전트가 target ID 를 잘못 지정해 자기 터미널을 종료하는 사고 방지.

- ID 지정 close(`close surface --surface <ID>` / `close pane --pane <ID>` / `close tab --tab <ID>`)에서 caller 자신이 속한 대상을 지정하면 거부한다.
- **자기 자신을 닫는 유일한 방법은 `tasty close self`** (`TASTY_SURFACE_ID` 로 자신을 식별, 해당 surface 만 닫음).
- 워크스페이스를 통째로 정리할 때는 `tasty close workspace --id <W>` 를 쓴다 — 안의 surface 를 하나씩 닫을 필요가 없다. 자기 자신 닫기 보호는 여기에도 걸린다: caller 의 surface 가 그 워크스페이스 안에 있으면 거부한다.

## 에이전트 닫기와 포커스

에이전트가 **보고 있지 않은** 대상을 닫아도 사용자 화면은 움직이지 않는다.

- `active_workspace` 는 인덱스라 앞쪽 워크스페이스가 빠지면 통째로 밀린다. `workspace.close` 도 위 "삭제로 인한 인덱스 이동" 과 **같은 헬퍼**를 지난다 — 제거 직후 `AppState::fix_workspace_pointers_after_removal` 이 제거 위치를 기준으로 인덱스를 보정하므로, 손대지 않은 포인터가 계속 같은 워크스페이스를 가리킨다. 워크스페이스를 제거하는 새 경로를 추가하면 그 헬퍼를 반드시 함께 태운다.
- **활성 워크스페이스 자신을 닫을 때만** 이웃으로 이동한다.
- 에이전트가 닫은 것은 사용자의 "닫은 항목" 되돌리기 스택에 쌓이지 않는다. 사용자 경로와 에이전트 경로의 차이는 `close_workspace_at` 의 `WorkspaceCloseOrigin` **하나**로 표현하고, 갈리는 부수효과(되돌리기 스택 · plugin `surface.closed` 의 reason · close 계측 경로값)를 전부 거기서 파생시킨다 — 같은 축을 나타내는 값을 여럿 두면 그중 하나만 갈리는 사고가 난다.
- 파일 열기도 같은 형태다 — `FileDispatchOrigin` **하나**가 사용자/에이전트를 가르고, 결과 탭을 선택하는지와 `None` 분기가 발화하는 intent 의 출처가 거기서 파생된다. **전송 채널이 아니라 행위의 성질로 정한다**: plugin 이 사용자의 클릭을 `file_handler.dispatch` 로 중계하는 경로가 있어(markdown 문서 안의 링크 · 파일열기 팝업) "IPC 로 들어왔는가" 는 좌변이 아니다. plugin 이 그 호출에 **자기 popup** 을 `owner_popup_instance` 로 실으면, host 는 호출자가 그 popup 의 소유 plugin 이고 그 popup 이 사용자의 확정형 입력(포인터 버튼 · 키 누름)을 받았을 때만 사용자로 친다 — 외부 IPC 호출자는 같은 키를 실어도 에이전트다. popup 밖에서 오는 중계(markdown 문서 안의 링크)는 아직 이 채널이 안 덮어 에이전트로 도착한다 ([ADR-0526](../../adr/0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md)). 그 값이 `IntentOrigin` 과 별개인 이유는 파일 식별이 워커 스레드를 왕복하면서 발화 당시 intent 를 잃기 때문이다 ([ADR-0302](../../adr/0302-a-user-file-open-selects-its-result-tab.md)).
- `workspace.closed` host event 는 origin 과 무관하게 발화한다. 워크스페이스가 사라졌다는 사실 자체는 누가 닫았든 같기 때문이다. 워크스페이스를 제거하는 경로는 셋(GUI·IPC 닫기 · Core cascade · 인라인 cascade)이고, 발화는 각 경로가 아니라 그 셋이 공유하는 초크포인트 `AppState::after_workspace_removed`(`src/state.rs`)가 한다 — 경로마다 각자 쏘던 때 인라인 cascade 하나가 실제로 빠져 있었다. 워크스페이스를 제거하는 새 경로를 추가하면 그 초크포인트를 반드시 지나게 한다.

## 에이전트가 만든 창과 포커스

에이전트가 창을 만들어도 사용자가 보던 창은 그대로 focused 창이다.

- 창을 만든 주체는 `WindowRequestOrigin` **하나**가 정한다.
  - `User`: 부팅 첫 창 · 단축키 · 명령 팔레트 · CSD 버튼 · 트레이 · macOS dock.
  - `Agent`: IPC `window.create` / `view.create`. CLI `tasty new window` 와 hook 의
    `ipc_sequence` 도 여기로 온다.
  - 창 생성 실패를 누구에게 알릴지도 같은 값이 가른다. 새 경로도 이 값을 정해 넘긴다.
- `User` 창은 `focused_view_id` 를 새 창으로 옮긴다. `Agent` 창은 옮기지 않는다. 그래서 뒤이은
  대상 없는 요청(`tasty new workspace` 등)은 사용자가 보던 창에 떨어진다. 새 창에 워크스페이스를
  만들려면 그 창의 surface 를 `workspace.create` 의 `surface_id`(CLI `tasty new workspace --surface`)로
  지목한다 — `window_id` 는 창 자체를 다루는 요청(닫기 · 스크린샷 등)에만 쓴다
  ([ADR-0514](../../adr/0514-new-workspace-names-its-window-by-a-surface-and-keeps-no-env-default.md)).
  그 `surface_id` 는 `cwd` 를 생략했을 때의 상속 원본도 정한다 — 지목했는데 그 창의 포커스 surface 를
  읽으면 결과가 사용자가 그 창에서 보는 탭에 좌우되기 때문이다
  ([ADR-0532](../../adr/0532-workspace-create-inherits-cwd-from-the-surface-that-names-its-window.md)).
  - 예외: 가리키던 창이 없으면(main 창이 0 개였으면) 에이전트 창이 잡는다. 빼앗을 포커스가 없다.
- 에이전트 창은 숨긴 채 만들어, 등록 뒤 사용자가 보던 창 **뒤에** 키 포커스 없이 보인다
  (`tasty_platform::window_stacking::show_behind`). OS 마다 할 수 있는 데까지다.
  - macOS · Windows: 사용자 창 바로 아래에 둔다. 키 포커스를 가져가지 않는다.
  - X11: 창 관리자에게 **요청한다** — `_NET_WM_USER_TIME = 0`(포커스를 주지 말라)과
    `_NET_RESTACK_WINDOW`(사용자 창 아래). 창 관리자가 무시하거나 "맨 아래" 로 다룰 수 있다.
    `_NET_WM_USER_TIME` 은 map 된 뒤 지운다 — 사용자가 나중에 그 창을 고를 때 걸림이 없게.
  - Wayland: 할 수단이 없다. 컴포지터가 정한다.
  - 네이티브 호출이 실패하면 경고 뒤 winit 기본 경로로 보인다.
- 사용자가 에이전트 창을 직접 고르면 `WindowEvent::Focused(true)` 추적이 `focused_view_id` 를
  옮긴다.

근거와 플랫폼별 결과는 [ADR-0497](../../adr/0497-an-agent-created-window-does-not-take-the-users-focus.md).

## 에이전트가 만든 탭과 선택

에이전트가 탭을 만들어도 그 pane 의 활성 탭은 그대로다 — 위 "에이전트가 만든 창" 의 탭 판이다.

- 선택 여부는 `DomainIntent::CreateTab` 의 `activate` **하나**가 정한다. 각 진입점이 값을 정해
  싣는다.
  - IPC `tab.create`(CLI `tasty new tab`): `false`. 새 탭은 뒤에 붙기만 한다.
  - attach forward 의 `NewTab`: 원격 **사용자**의 손 조작이면 `true`, 원격 에이전트면 `false`
    (복원 스택을 가르는 `ForwardOrigin` 과 같은 축).
  - 파일 열기의 origin 갈래: `FileDispatchOrigin` 에서 파생(위 "에이전트 닫기와 포커스" 의 파일
    열기 항목).
  - `Intent::NewTab`: 발화 origin 이 사용자면 `true`, 아니면 `false`. 에이전트 라벨로 오는
    발화점은 origin 없는 `file_handler.dispatch` 하나이고, 그 갈래는 이제 사용자가 보던 탭을
    바꾸지 않는다. markdown plugin 의 파일열기 팝업(사용자의 `open_markdown`)은 같은 호출에 자기
    popup 을 실어 사용자로 도착하므로 종전대로 새 탭을 선택한다(아래 "에이전트 닫기와 포커스" 의
    파일 열기 항목 · [ADR-0526](../../adr/0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md)).
- terminal kind 는 `activate` 와 무관하게 background 다. 사용자의 새 터미널 탭은 이 인텐트가
  아니라 `AppState::add_tab` 이 연다.
- 응답의 `active_tab` 은 "생성 뒤 그 pane 의 활성 탭" 이다 — 에이전트가 만든 탭이면 사용자가
  보던 탭의 인덱스다. 새 탭은 응답의 `surface_id` 로 다룬다.

근거는 [ADR-0502](../../adr/0502-an-agent-created-tab-does-not-take-the-users-tab.md).

## 원격이 점유한 surface 는 닫기 요청이 죽이지 않는다

하드 점유(ADR-0040)는 "이 surface 는 지금 원격 사용자가 쓰고 있다" 는 선언이다. 닫기는
비가역이고 — 되돌리기 스택에 남는 것은 살아 있는 PTY 가 아니라 같은 명령으로 새 세션을
여는 레시피다 — 그래서 **닫기 요청은 거절한다.** 에이전트 경로만이 아니라 **사용자
경로도 같다**: 여기서 보호 대상은 로컬 사용자가 아니라 원격 사용자이고, 로컬 사용자는
점유 표시 위의 강제 끊기 버튼으로 점유를 회수한 뒤 닫을 수 있어 갇히지 않는다.

- 규칙의 소유자는 `AppState::refuse_if_hard_occupied` 하나다. 닫기 진입점은 자기가 죽일
  **대상 집합만** 넘긴다 — 워크스페이스 닫기는 그 워크스페이스의 surface 전부, 탭·페인
  닫기는 그 안의 전부, surface 닫기는 그 하나.
- **요청 경로만 이 검사를 지난다.** 셸이 스스로 끝나서 도는 사후 정리는 이미 죽은 프로세스를
  치우는 것이라 거절하면 좀비 surface 가 남는다. 그래서 검사는 공용 cascade 초크포인트가
  아니라 사용자 제스처·에이전트 요청의 진입점에 붙는다.
- IPC 는 토스트가 아니라 사유가 실린 에러로 거절한다. `surface.close_self` 도 예외가 아니다
  — 그 메서드는 호출자를 확인하지 않고 params 의 id 를 받으므로, 예외로 두면 그대로 우회
  통로가 된다.

## 재정렬에서도 포커스 대상은 보존된다

워크스페이스를 재정렬하면 인덱스가 가리키는 대상이 바뀐다 — 제거와 같은 종류의 밀림이다.
사용자가 보고 있던 워크스페이스는 재정렬 뒤에도 그대로 보고 있어야 한다.

- 옮겨진 것을 보고 있었으면 포인터가 **따라간다**.
- 옮겨진 구간을 자기 위치가 통과당하면 한 칸 당겨지거나 밀린다.
- 구간 밖이면 그대로다.

재정렬은 두 경로로 들어온다 — 사이드바 드래그·컨텍스트 메뉴가 부르는
`AppState::move_workspace`, 그리고 `CoreEvent::WorkspaceMoved` 의 `cascade_workspace_moved`
(IPC `workspace.move` 도 이쪽). 규칙은 **한 곳에만** 있다: 순수함수
`active_index_after_move` 와 그것을 적용하는 `AppState::fix_workspace_pointers_after_move`.
재정렬하는 새 경로를 추가하면 그 헬퍼를 함께 태운다 — 규칙을 복제하면 어느 경로로
재정렬했느냐에 따라 포커스가 달라진다.

카테고리 quick-switch 착지점은 id 를 들어 이 축의 보정 대상이 아니다(위 참조).

## 코드 위치

- `active_modal_id` / `modal_active` 게이트: `src/app/event_handler.rs`(`self.view.active_modal_id`), View 디스패치.
- focus 대상 해석 / `TASTY_SURFACE_ID` / `this`: `crates/tasty-cli/src/request.rs`.
- `tasty close self`: `crates/tasty-cli/src/commands/new_close.rs`(`CloseCommands::CloseSelf`).
- 삭제 시 활성 포인터 보정: `Pane::remove_tab_preserving_active`(`crates/tasty-model/src/pane.rs`) · `active_index_after_removal` / `AppState::fix_workspace_pointers_after_removal`(`src/state/workspace.rs`) · cascade 진입점 `cascade_surface_closed`(`src/core/structural_cascade.rs` — 두 빌드가 같은 본문).
- 재정렬 시 활성 포인터 보정: `active_index_after_move` / `AppState::fix_workspace_pointers_after_move`(`src/state/workspace.rs`) · 호출 경로 `AppState::move_workspace` 와 `cascade_workspace_moved`(`src/app/dispatch_domain.rs`, headless 는 `dispatch_domain_stubs.rs`).
- 창 생성의 origin 분기: `WindowRequestOrigin`(`src/app/event.rs`) → `focus_after_register` · `origin_window_attributes`(`src/app/window_lifecycle.rs`) — 등록 뒤 focused 창과 생성 속성(`with_active` · `with_visible`)이 여기서 파생된다. 에이전트 창을 사용자 창 뒤에 보이는 OS 호출은 `crates/tasty-platform/src/window_stacking.rs`.
- 탭 생성의 선택 분기: `DomainIntent::CreateTab` 의 `activate` → `Core::apply_create_tab`(`src/core/impl_tab.rs`) 이 `Pane::add_surface_tab` / `Pane::add_surface_tab_background`(`crates/tasty-model/src/pane.rs`) 중 하나를 고른다. 값을 정하는 진입점은 `structural_exec::create_tab`(`src/core/structural_exec.rs`) 의 호출자 · `src/intent/tab.rs` · `open_surface_tab`(`src/file/dispatch.rs`).
- 워크스페이스 close 의 origin 분기: `WorkspaceCloseOrigin`(`src/state/workspace.rs`) — 되돌리기 스택 · plugin close reason · 계측 경로값이 여기서 파생된다.
- 워크스페이스 제거 후 공통 뒷정리(`workspace.closed` 발화 + workspace scope memory purge): `AppState::after_workspace_removed`(`src/state.rs`).

외부 소켓의 전 창 합산·namespace·App 조기 응답도 일반 handler와 같은 게이트/허용 관측을 한 번 거친다. 검사 완료 요청을 하위 라우터에 전달하므로 라우팅 층 수만큼 예산이 소비되지 않는다. [ADR-0277](../../adr/0277-ipc-admission-and-observation-run-once.md).
