# 파일 핸들러 (File handler)

## 원칙과 현재 동작

[피드백 원칙](../../principles/feedback-and-recovery.md)에 따라 접수와 최종 성공·실패를 구분한다. `file_handler.dispatch`와 `markdown.navigate`의 응답은 접수이며, 에이전트·plugin 호출자는 응답의 `run`으로 최종 결과 문자열을 조회한다([접수 뒤 최종 결과](#접수-뒤-최종-결과)). 사용자 요청은 기존 UI 결과(탭·picker·토스트)를 그대로 받는다.

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent (`file_handler.dispatch`) · plugin (contribute)
- **설계**: URL 대상은 picker·실행 계층에만
- **코드**: `crates/tasty-file-format/`(식별) + `crates/tasty-file-handler/`(핸들러 정책·레지스트리) + `src/file/dispatch.rs`(디스패치); IPC `file_handler.{reload,detectors,dispatch}`
- **화면**: [설정 창](../settings/screens/settings.md) Handler 탭의 파일 서브탭 3 종 · file_handler_picker popup

## 목적

URI/경로 입력을 **(1) 형식 식별 → (2) 등록 핸들러 디스패치** 두 단계로 라우팅한다(예: `.md` 더블클릭 → markdown surface 새 탭). 두 단계는 독립 모듈로, `file_handler` 만 `file_format::DetectorId` 를 import 하는 단방향 의존.

## 내부 동작

### 형식 식별 (`FileFormatRegistry`)

DetectorId 는 일반 `[a-z0-9-]` / 호스트 예약 `$<word>`(예: `$directory`). 식별 깊이는 셋이다 — Name(파일시스템 접근 없음, 대상을 파일로 보고 `extension`·`path_glob` 만): 원격 대상용; rule 종류 — Cheap(디렉터리 확인 외 IO 없음): `extension`·`path_glob`·`is_directory`; Deep(8KB head + MIME): `magic`·`mime`·`lua`(sandbox 5.4)·`structure_check`(JSON Schema). Deep 평가는 호출당 head/MIME 를 캐시(IO 1 회). pre-filter 로 디렉토리/파일 대상에 맞는 detector 만 평가. 호스트 default 는 `default-file-format.toml`(html, svg, `$directory`), markdown/image detector 는 각 plugin 이 contribute.

### 핸들러 디스패치 (`FileHandlerRegistry`)

HandlerId: `host/<name>` · `<plugin_id>/<name>` · `user/<name>`. HandlerAction: `OpenSurface{surface_kind, param_key}` · `Ipc{method}` · `System`(OS 위임). actor 별 schema 강제 — **plugin TOML 은 System 금지**(sandbox 일관성), user TOML 은 전부 허용. `handlers_for` 정렬: priority asc → owner(`user > plugin > host`) → id 사전순. 정렬 키는 `run_order_key` 하나이며 설정 File Handlers 목록도 같은 키로 정렬한다. 같은 detector 에 핸들러 여럿이면 picker 없이 1순위 자동(결정론적).

### 대상 — 파일 경로와 URL

dispatch 대상(`DispatchTarget`)은 파일 경로(`File`) 또는 `http`/`https` URL(`Url`)이다. 형식 식별은 파일에만 적용하고, URL 은 detector 없이 picker 로 직행한다 — 그래서 URL 에는 1순위 자동 실행이 없다. 식별·평가 계층은 경로 문자열에 URL(`<scheme>://`, scheme 두 글자 이상)이 담겨 들어와도 매칭하지도 읽지도 않는다.

액션별로 URL 을 받는지는 한 판정이 정하고, picker 의 후보 목록 · Recent 목록 · 최종 실행에 똑같이 걸린다(거절된 선택은 recent 에 기록하지 않는다).

URL 대상의 picker 헤더에는 **URL 전용 형태가 따로 없다** — detector 를 안 거치므로 형식 Tag 가 "형식 알 수 없음" 이고, 경로 자리에 URL 원문이 파일 경로와 같은 규칙으로(길면 앞에서 잘려) 들어간다. 디자인 원본 은 파일 경로만 다루므로 이 자리는 파일 규칙을 그대로 쓴 것이다.

| 액션 | 파일 대상 | URL 대상 |
|------|-----------|----------|
| `OpenSurface` | `{param_key: 경로}` | `param_key = "url"` 인 핸들러만 — `{url: 원문}` (html 핸들러 → webview) |
| `System` | `file://` URI 로 OS opener | 원문 그대로 OS opener |
| `Ipc` | plugin 에 `{"path": 경로}` | 받지 않음 |

`System` 의 OS 열기가 실패하면 `execute_handler_action` 은 false 를 돌려주고(picker 선택이면 Recent 에 기록하지 않는다), 출처가 사용자(`FileDispatchOrigin::User`)일 때만 창 범위 실패 토스트(`toast.open_system_failed`)를 띄운다. Agent·PluginUnverified 출처는 `open_uri` 의 warn 로그만 남기고 사용자 화면에는 띄우지 않는다([토스트 트리거 정책](../../design/systems/toast.md#트리거-정책-critical)).

### 원격(mirror) 대상

`origin_surface_id` 가 mirror workspace 의 surface 이면 경로는 원격 호스트의 파일이다. 입구는 mirror explorer 의 더블클릭, mirror 터미널의 파일 경로 링크 수식키 클릭, mirror origin 을 준 `file_handler.dispatch` 다. 터미널 링크 우클릭의 "연결 동작"은 아래 picker 전용 경로를 쓴다.

- 식별은 요청한 깊이와 관계없이 Name 으로 한다. client 에 같은 경로의 파일·디렉터리가 있어도 읽지 않는다. CLI `--depth` 도 무시된다. `file_handler.dispatch` 응답의 `depth` 는 요청값을 그대로 돌려주므로 실제 식별 방식을 나타내지 않는다.
- 한계: 확장자·파일명으로 정해지지 않는 형식(magic·MIME 으로만 식별되는 파일)은 매칭되지 않는다. Name 은 대상을 항상 파일로 보므로 디렉터리 경로는 `$directory` 에 매칭되지 않는다. 탐색기는 원격 목록이 파일이라고 알려 준 항목만 보내지만 IPC 입구는 이를 보장하지 않는다. 두 경우 모두 Agent(외부 IPC) 요청은 `accepted:true` 응답 뒤 run 이 `failed` 로 끝나고, 사용자 요청과 PluginUnverified 요청은 아래의 다운로드 후 실행 대화상자로 묻는다(run 이 있으면 같은 실패를 기록한다).
- 원격에 열 수 있는 핸들러는 `OpenSurface` 이면서 client 에 등록한 그 kind 가 원문 전달 허용 목록에 있거나 egui-mesh 인 것이다. 현재 markdown(`com.tasty.markdown`)과 egui-mesh kind 전부(plugin ID 무관)가 해당한다. html 은 client kind 를 `com.tasty.html` 이 등록했고 origin surface 가 속한 mirror workspace 의 연결이 HTML entry viewer 를 협상했을 때만 해당한다. 협상하지 않은 연결에서는 placeholder 라 제외한다. 원격에 같은 kind 가 없으면 원격의 새 탭 생성이 실패한다.
- 1순위 핸들러가 원격에 열 수 있으면 로컬과 같이 바로 실행한다.
- 1순위가 원격에 열 수 없고 다른 후보가 있으면 사용자 요청과 PluginUnverified 요청은 기존 핸들러 picker 를 띄운다. 후보와 최근 목록에는 원격에 열 수 있는 핸들러만 남기고, 1순위가 목록에 없으므로 기본 핸들러 표시는 두지 않는다. fallback 표시(전체 핸들러 그룹·1 회성 띠)도 쓰지 않는다. picker 에서 고른 핸들러도 같은 조건을 다시 확인한 뒤 원격 새 탭으로 실행한다.
- Agent(외부 IPC) 요청은 로컬과 마찬가지로 사용자 화면에 picker 를 띄우지 않는다. 우선순위 순서로 원격에 열 수 있는 첫 핸들러를 실행한다.
- 원격에서 받은 파일(데이터 홈의 `download-run` 아래)을 사용자 요청으로 여는데 활성 workspace 가 mirror 이고 실행할 핸들러(1순위나 picker 선택)가 `OpenSurface` 이면, 활성 workspace 에 `NewTab` 을 내지 않고 새 로컬 workspace 를 만들어 연다([다운로드 후 실행](../remote-attach/index.md#원격-파일-다운로드-후-실행)). 에이전트 요청은 활성 workspace 를 바꾸지 않으므로 이 분기를 쓰지 않는다.
- 실행한 `CreateTab` 은 원격 `StructuralOp::NewTab` 으로 forward 된다. 이 forward 는 실패가 아니며 `file_dispatch CreateTab failed` 경고를 남기지 않는다. 사용자 요청은 client 가 새 탭을 선택하고, 원격(서버)은 자기 선택을 유지한다(공통 forward 경로가 서버 쪽 선택을 바꾸지 않는다). Agent 와 PluginUnverified 요청은 client 선택도 유지하고 원격 실패를 warn 로그로만 남긴다.
- 원격에 열 수 있는 핸들러가 하나도 없으면(`System`·`Ipc` 뿐, 허용되지 않은 kind, 매칭 없음) picker 를 띄우지 않는다. 사용자 요청과 PluginUnverified 요청은 입구(경로 링크·이름으로 연 경로)와 관계없이 [다운로드 후 실행](../remote-attach/index.md#원격-파일-다운로드-후-실행) 대화상자로 묻는다. PluginUnverified 요청의 대화상자는 picker 와 같이 에이전트 출처로 열리고, 다운로드는 사용자가 Download and run 을 누를 때만 시작한다. Agent(외부 IPC) 요청은 사용자 화면에 팝업을 만들지 않으므로 warn 로그로 끝난다.
- 원격 경로는 로컬 최근 목록에 기록하지 않는다.
- 터미널 링크 우클릭의 "연결 동작"(`file::dispatch::remote::open_remote_link_picker`)은 식별·자동 실행 없이 원격에 열 수 있는 핸들러 전체를 picker 로 연다. 로컬 링크의 "연결 동작"이 전체 핸들러를 보여 주는 것과 같다. 최근 목록도 같은 조건으로 거르고 기본 핸들러 표시는 두지 않는다. 그런 핸들러가 하나도 없으면 빈 picker 대신 같은 "다운로드 후 실행" 대화상자로 묻는다.
- 파일 선택 창(원격 경로 복사)은 이 경로를 쓰지 않는다. 판정은 `file::dispatch::remote` 한 곳에 있다. 근거 [원격 attach (Remote attach)](../remote-attach/index.md).

### Contribution 머지 + 부팅 자동 등록

handler와 detector는 출처별 contribution을 보관하고 Host→Plugin→User 순서로 병합한다.
같은 출처 안에서는 설치 순서를 유지하며 제공한 값만 덮는다. rule은 합치고 중복을 제거한다.
따라서 plugin이 나중에 등록되거나 재기동해도 user patch가 reload 없이 적용된다.

handler ID는 host/<name>·<plugin_id>/<name>·user/<name>으로 나뉜다.
같은 owner의 새 contribution은 이전 것을 교체하므로 현재 같은 ID에는 원 출처와 user patch가 모인다.
detector ID에는 이 구분이 없어 같은 ID를 선언한 plugin이 host 기본값을 덮는다.

DetectorDecl.disabled는 Option<bool>이다. user의 명시적 false는 다시 켜기로 처리한다.
host·plugin의 false는 기존처럼 끄지 않는다는 뜻이며 다른 출처의 disable을 해제하지 않는다.
Settings 삭제 버튼은 user contribution 존재 여부를, 출처 칸은 rule 선언 출처를 읽는다.
메타데이터만 바꾼 user patch도 삭제할 수 있고 export는 user contribution을 보존한다.

GUI에서는 부팅 때 활성 plugin의 detector·handler를 등록한다.
헤드리스의 메타데이터 조회는 설치 목록만 읽으며 기여를 등록하지 않는다.
필요한 plugin을 시작하거나 enable할 때 등록하며, 등록은 프로세스 spawn 성공 여부와 별개다.
재등록은 교체 방식으로 중복을 만들지 않으며 제거할 때 해당 plugin 기여만 지운다.

### Picker + Recent

사용자가 고르게 할 때 `file_handler_picker` popup 이 뜬다. Recent 는 `~/.tasty/file-handler-recent.json` LRU(cap 10, atomic write). picker 는 dispatch 하지 않고 결과만 남기고 호스트 layer 가 frame 끝에 실행 + 저장.

#### 창과 목록

폭은 420px이며 공통 타이틀바 대신 자체 헤더에 제목·형식 Tag·고정폭 경로를 표시한다.
경로를 한 번만 보여주고, 길면 파일명이 남도록 앞부분을 줄인다. 헤더의 실제 글자 공간은
390px(420 − 보더 1×2 − 여백 14×2)이다. 매 프레임 글자 폭을 재서 앞쪽 경로 조각을
`…/`로 바꾸며, 한 조각이 공간보다 길 때만 문자 단위로 자른다.

폭은 한 글자를 더했을 때의 증가량으로 잰다. D2Coding 11px의 공칭 폭은 5.5556px이지만
정수 픽셀 반올림 뒤에는 6px이 되기 때문이다. 공칭 폭으로 계산한 70자는 실제로
419.56px을 차지해 390px보다 29.56px 길어진다. 측정에 실패한 경우에만 390 ÷ 6인
65자를 상한으로 쓴다.

후보와 Recent는 한 목록에 배치하며 선택도 하나만 유지한다. 목록이 264px보다 길면
목록만 스크롤하고 아래에 페이드를 표시한다. 헤더와 footer는 고정된다. footer에는
[취소]·[열기]가 있고 선택이 없으면 [열기]를 비활성화한다. Esc와 [취소]로 닫으며
scrim 클릭으로는 닫지 않는다.

#### 행의 이름과 출처

`FileHandler`에 icon 필드는 없다. action이 여는 surface kind에서 아이콘을 구한다:
markdown → markdown, editor → edit, pager → terminal, directory → folder,
table → columns, binary → layers, log → listView. 알 수 없는 kind와 surface를 열지
않는 `Ipc`·`System`은 `file`을 쓴다.

이름은 `display_name_i18n_key`로 표시하고, 선언이 없으면 ID의 마지막 `/` 뒤 부분을
고정폭 글꼴로 표시한다. 둘째 줄에는 출처(built-in / you / plugin)와 전체 ID를 한 번씩
표시한다. ID가 34자를 넘으면 앞부분을 줄인다. plugin 출처에서는 출처 글자와 아이콘만
mauve로 표시하고 이름에는 이 색을 적용하지 않는다.

Recent 행에는 마지막 사용 시각도 표시한다. `n`은 소수점 이하를 버린 정수다.

| 경과 시간 | 표시 |
|---|---|
| 60초 미만 | `방금` |
| 60분 미만 | `{n}분 전` |
| 24시간 미만 | `{n}시간 전` |
| 24–48시간 | `어제` |
| 2–7일 | `{n}일 전` |
| 7일 이상 | `YYYY-MM-DD` |

절대 날짜는 10자로 고정되고 언어와 무관해 번역 키를 쓰지 않는다. 이 길이를 기준으로
`component.fh-when-width`(56px)를 확보해 시간이 바뀌어도 옆 ID가 움직이지 않게 한다.
공간이 부족하면 시각 대신 ID를 줄인다. 시각은 매 프레임 계산하므로 열린 채로 자정을
넘어도 표시가 갱신된다.

picker에서 고른 핸들러는 이번 한 번만 실행한다. 형식별 기본 핸들러를 저장하는 체크박스는
없다. 지속적인 연결 설정은 설정 › 핸들러에서 확인하고 바꾼다.

**어떤 형식으로 뜨는가**: 매칭 핸들러가 있으면 picker 없이 1순위가 자동 실행되므로(`handlers_for` 정렬), picker 가 뜨는 경로는 **셋**이고 셋 모두 fallback 이다.

| 경로 | 후보 | fallback 인가 |
|---|---|---|
| 사용자 요청에서 이 detector 에 매칭되는 handler 가 0 개 (`file::dispatch::apply_identify_result`) | `all_handlers()` | 예 |
| 터미널 링크 메뉴의 "연결 동작" — 식별을 건너뛰고 강제로 연다 | `all_handlers()` | 예 |
| 원격(mirror) surface 경로 링크의 "연결 동작" (`open_remote_link_picker`) | `all_handlers()` 중 원격에 열 수 있는 것 — recent 도 같은 조건으로 거른다 | 예 |

셋째는 원격에 열 수 있는 핸들러가 하나도 없으면 picker 를 띄우지 않는다. 이 picker 는 사용자 요청만 열므로 [다운로드 후 실행](../remote-attach/index.md#원격-파일-다운로드-후-실행) 대화상자로 묻는다. 설정 등록을 권하는 빈 picker 는 원격 경로에 뜨지 않는다.

셋째 경로(원격에 열 수 있는 핸들러만 담은 원격 picker)에서는 머리 경로 줄 맨 앞에 원격 표지(remote 글리프 12 + 호스트 글자, `fh-remote-fg`, mono caption, 채움 없음)를 둔다. 글리프와 호스트 사이는 `fh-remote-gap`(4), 표지와 경로 사이는 space-sm(8)이다. 호스트 글자는 `fh-remote-max-width`(160)에서 끝을 줄이고, 비면 그 자리에 `attach.mirror_pill_label`(Remote)을 font-label 로 쓴다. 경로는 표지가 남긴 상자(390 − 표지 − 8)에 대해 같은 앞 생략 규칙으로 줄인다(`file_handler::target_budget_chars_beside`, 글자 폭을 잴 수 없으면 34자). 로컬 대상 picker 에는 표지가 없다. 호스트 글자는 [원격 위치 표시](../remote-attach/index.md#원격-위치-표시)와 같고 picker 를 연 시점 값이다. 표지는 본체와 갤러리가 공용 `tasty_ui_widgets::paint_fh_remote_mark` 로 그린다. Ctrl+클릭이 1순위를 원격에 열 수 없어 띄우는 원격 picker 도 같은 표지를 둔다.

세 경로에서는 후보가 `handlers_for(d)` 가 아니라 `all_handlers()` 이고(recent 와 중복 제거), 다음 표시로 매칭 실패를 알린다: 그룹 라벨이 attention 톤의 "전체 핸들러", caption 이 "이 형식에 맞는 핸들러가 없습니다.", 헤더 Tag 가 "형식 알 수 없음", 그리고 헤더 아래 띠가 **1 회성**이고 다음에도 이 화면이 나온다고 적는다(user TOML 미변경). 기본 핸들러 Tag 는 이 상태에 붙지 않는다 — 매칭이 없으면 기본도 없다.

**빈 상태**: 이 popup에 표시할 행이 없으면 빈 상태를 보여준다 — 후보와 recent 가 **둘 다 비면** 목록 자리가 중앙 블록으로 바뀐다. 지금 이 상태에 닿는 경로는 없다. 앞의 두 경로는 `all_handlers()` 를 싣는데 host 기본 핸들러가 `default-file-handlers.toml` 에 박혀 있어 plugin 을 전부 꺼도 0 이 되지 않는다. 셋째 경로(원격 후보 없음)는 picker 를 열지 않는다. 블록은 흐린 파일 글리프 + "등록된 핸들러가 없습니다." + 한 줄 안내 + "설정에서 핸들러 등록" 버튼(Settings 를 `FileHandler` 탭으로 오픈)이고, 프레임 폭과 footer 는 그대로고 [열기]만 비활성이다 — 같은 다이얼로그의 한 상태이지 다른 화면이 아니다.

### 접수 뒤 최종 결과

`file_handler.dispatch`와 `markdown.navigate`의 응답은 접수 확인이다. `accepted: true`의 의미는 그대로이며 `run`(실행 번호)이 추가됐다. 호출자는 `run.result {run}`으로 진행·최종 결과를 조회한다(`src/request_runs.rs`, 처리기 `src/adapters/ipc/handler/run_result.rs`).

- 응답: `{run, method, done, state, result, ...}`. 진행 중이면 `done: false`, `state: "running"`이다. `state`는 `succeeded`·`failed`·`cancelled` 중 하나로 끝난다. `result`는 표시 언어와 관계없는 영어 한 줄이다.
- 처음 기록한 최종 결과만 남는다. 같은 파일을 겹쳐 요청해도 요청마다 run이 따로 있어 결과가 섞이지 않는다.
- 조회 범위: 토큰 없는 로컬 IPC 호출자는 모든 run을 읽는다. plugin과 세션 토큰을 쓰는 에이전트는 자기가 시작한 run만 읽고, 다른 호출자의 run은 알 수 없는 run과 같은 `-32602`다. 외부 `markdown.navigate`는 markdown plugin이 host로 다시 보내 run을 시작한다. host는 namespace 전달 응답을 원래 호출자에게 보내기 전에 그 run을 원래 호출자에게 넘기므로(`src/app/ipc/routing.rs`의 `RELAYED_RUN_METHODS`), 요청한 에이전트가 자기 토큰으로 결과를 읽는다. plugin이 시작한 run만 넘기며, 다른 plugin이나 에이전트는 여전히 읽지 못한다. 자기 namespace를 거치지 않고 plugin이 직접 부른 `markdown.navigate`(주소창 이동 등)의 run은 그 plugin의 것이다.
- 보관: `hook_handler.dispatch`의 run과 같은 저장소에서 합쳐 최근 128건(`request_runs::KEEP`)만 보관한다. 보관 범위를 벗어난 run은 `-32602`다.
- 접수 전 동기 오류(파라미터, origin pane 부재, URL, 없는 경로, 없는 surface, headless의 `-32017`)는 run을 만들지 않는다.

| 경로 | 최종 결과 |
|------|-----------|
| `OpenSurface` 핸들러 | 탭 생성 journal 완료로 `succeeded`("surface N is ready", `surface_id`). 생성 오류는 `failed`(오류 문장, `code`) |
| `System` 핸들러 | OS opener에 넘기면 `succeeded`("handed to the OS opener"). 외부 프로그램의 작업 완료는 판정하지 않는다. 넘기지 못하면 `failed` |
| `Ipc` 핸들러 | plugin 응답이 오면 `succeeded`("<method> answered", `response`), 오류 응답은 `failed`("<method> failed: <message>", `code`). plugin manager가 없거나 응답 없이 채널이 닫히면 `failed` |
| `markdown.navigate` | 변환 journal 완료로 `succeeded`(같은 `surface_id`). 접수 뒤 대상 surface가 사라지는 등 변환 오류는 `failed`(오류 문장, `code`) |
| mirror 대상(원격 `NewTab`·변환 forward) | 원격 큐 접수(`forwarded`)는 결과가 아니다. run은 보낸 연결과 op_id로 원격 `StructuralResult`를 기다린다(`src/app/attach_client/op_runs.rs`). 원격 성공은 `succeeded`이며 원격이 알린 `created_surface_id`를 다음 delta에서 로컬 ID로 바꿔 `surface_id`(로컬)·`remote_surface_id`로 싣는다. 그 delta에 없는 surface는 로컬 ID 없이 끝낸다. 원격 실패는 `failed`("the remote could not apply the change: <원격 사유>")이며 사유 원문을 `remote_reason` 에도 싣는다(사유가 없거나 비면 싣지 않는다). 원격이 없는 파일로 markdown surface 를 바꾸려는 변환을 거절하면(사유 `path not found: <경로>`) 결과에 `reason_code: "not_found"` 를 함께 싣는다. 호출자는 문구를 해석하지 않고 이 값으로 파일 없음을 구분한다. 보내기 전에 연결이나 매핑이 바뀌면 `cancelled`, 보내기 전에 journal이 멈추면 `failed`("the remote change was not sent because the journal stopped: <원인>"), 보낸 뒤 결과 전에 연결이 끝나면(재연결 포함) 원격 적용 여부를 모르므로 `unknown`이다 |
| 실행 전 실패 | 맞는 핸들러 없음(Agent), 접수 뒤 origin pane 소멸, 원격에서 열 수 없음, 핸들러가 대상을 받지 않음, 식별 worker 없음, picker에서 고른 핸들러가 사라짐, origin 없는 요청의 식별이 끝났을 때 포커스된 창이 없음(마지막 창 park·닫힘 등, "no window was focused to open the file in")은 모두 `failed`와 그 이유다 |
| fallback picker(PluginUnverified) | 선택하면 그 실행의 결과다. 취소·설정 열기는 `cancelled`, 새 picker로 대체되면 `cancelled`("replaced by a newer file handler picker"), picker 상태가 결과 없이 사라지면(창 닫힘 등) `cancelled`. 창이 park되면 picker 상태는 복원용으로 남지만 그것을 보여 주던 View가 사라졌으므로 park 시점에 `cancelled`("the window showing the file handler picker was closed")로 끝낸다. 복원한 창의 picker는 그대로 쓸 수 있고 그 선택은 결과를 남기지 않는다 |

- 실행한 핸들러는 결과의 `handler`와 `action`(`open_surface`·`ipc`·`system`)에 실린다.
- 대용량 markdown 확인은 plugin이 surface를 만든 뒤 하므로 host에는 크기 상한 거절 경로가 없다. surface가 만들어지면 `succeeded`다.
- 사용자 출처의 toast·탭 선택 동작은 바뀌지 않는다. Agent 출처는 여전히 picker를 띄우지 않는다.
- CLI `tasty file-handler dispatch`는 접수 줄을 출력한 뒤 `run.result`를 200ms 간격으로 최대 30초 조회해 결과 줄을 출력한다. 최종 상태가 `succeeded`가 아니면 종료 코드 1이다. 조회 응답도 남은 시간까지만 읽고, 상한을 넘기면 `timed_out: true`를 단 진행 중 결과를 출력한다. `--no-wait`는 접수 줄만 출력하며, 결과는 `tasty file-handler result --run <N>`(`run.result`)으로 조회한다. 기다리는 dispatch는 client-driven 명령이라 루트 옵션 `--response-timeout-ms`를 받지 않는다(종료 코드 2). `--no-wait`는 요청 하나라 그 옵션이 적용된다.

### 권한

`file_handler.define`(새 detector+handler) · `file_handler.extend:<id>`(기존 detector 에 rule) · `file_handler.handle:<id>`(기존 detector 에 handler). `$` sentinel 은 모든 토큰에서 reject. ([plugin-permissions](../../dev-guide/plugin-permissions.md).)

### Origin 소유권과 비동기 완료

origin_surface_id는 결과를 만들 대상이다. 최초 요청, 식별 worker 결과, picker 선택 모두
그 surface의 소유 engine과 pane으로 보낸다. parked engine도 포함한다.
대상이 처음부터 없으면 -32602로 거절하고 접수 뒤 사라지면 warning을 남기고 실행하지 않으며 run을 `failed`로 끝낸다.
다른 focused pane으로 바꾸지 않는다. accepted:true는 큐 접수이며 완료 RPC를 한 번 더 보내지 않는다. 최종 결과는 `run.result`로 조회한다.
picker 취소는 action과 recent 기록을 모두 생략한다. Ipc action payload는 path-only다.

탭 선택 여부는 별도 FileDispatchOrigin으로 결정한다.
사용자 Explorer 더블클릭·terminal link·drop·picker 확인·연결 동작 메뉴는 User라 결과를 선택한다.
Agent 요청과 사용자 입력을 증명하지 못한 plugin 중계 요청(PluginUnverified)은 기존 탭과 surface 선택을 유지한다.
origin 생략도 같은 원인 규칙을 적용한다.
외부 IPC(CLI·에이전트 세션) 요청인 Agent는 로컬·원격 모두 사용자 화면에 picker 를 띄우지 않는다. 로컬에서 매칭 핸들러가 없으면
fallback picker 대신 warn 로그를 남기고 실행하지 않으며 run 을 `failed`("no file handler matches <경로>")로 끝낸다.
PluginUnverified는 사용자 클릭일 수 있어 아무 반응 없이 끝나지 않게 한다. 로컬에서 매칭 핸들러가 없으면 fallback picker 를 열고,
mirror 에서 1순위를 원격에 열 수 없으면 원격 picker 를 열며, 원격에 열 수 있는 핸들러가 없으면 다운로드 후 실행 대화상자로 묻는다.
그 picker 에서 확정해도 요청 출처는 그대로라 결과 탭을 선택하지 않는다.
현재 focus로 사용자 여부를 추측하지 않는다.

plugin이 중계한 IPC를 User로 인정하는 근거는 두 가지다.

| 근거 | 호스트가 확인하는 조건 | 기록 수명 |
|------|----------------------|-----------|
| owner_popup_instance | Plugin caller의 popup이 같은 창에 열려 있고 host가 버튼 또는 키 누름을 전달했다 | popup이 닫힐 때까지 재사용. 이동·wheel·release는 입력 근거가 아님 |
| user_navigation_url | origin webview의 최신 적격 navigation을 같은 plugin에 통지했고 URL이 일치한다 | 성공한 한 호출이 소비. 부적격 navigation과 webview 제거는 이전 기록 삭제 |

webview 적격 조건은 native 엔진의 user gesture와 현재 페이지를 작성한 소유 plugin이다.
외부 caller가 같은 key를 보내면 거절 대신 Agent로 처리한다. plugin caller의 근거가 부족하면 PluginUnverified로 처리한다.
작성자가 외부에서 소유 plugin으로 바뀐 첫 drain 프레임은 이전 페이지 클릭과 혼동할 수 있어 인정하지 않는다.
표지는 시도를 모두 처리한 뒤 지운다. 이전 클릭이 그 프레임 이후에 늦게 도착하는 경우까지 막는다고 보장하지 않는다.

macOS는 동등한 공개 gesture 판정이 없어 WebView가 OS에서 받은 클릭 뒤 1초 안의 main frame 탐색을 gesture로 본다([WebView 탐색](../../design/systems/webview.md#탐색-상태와-실행-소유)). 실제 클릭 결과는 측정하지 않았다.
Linux의 실행 확인과 Windows의 코드·컴파일 확인은 구별한다.
같은 프레임의 앞 클릭이나 재로드가 끼어든 정당 클릭도 PluginUnverified가 될 수 있다.
사용자 원인은 적용 실패 toast에도 쓰고 Agent·PluginUnverified 실패는 로그로 알린다(원격에 열 수 없을 때의 다운로드 후 실행 대화상자는 위 예외).
이 규칙은 파일 열기에 한정하며 plugin의 임의 사용자 선언을 신뢰하는 API가 아니다.

## 인터페이스

- **사용자**: Settings **Handler** 탭의 파일 서브탭(File Detectors / File Handlers / File Extension Mapping — 토글·user 항목 추가/삭제, 확장자 우선순위). 이미 있는 detector id 로 추가하면 user contribution 을 더하거나 바꾸고(`upsert_user_detector`), detector 삭제는 user contribution 만 지운다(`remove_user_detector`) — host·plugin 기여는 남는다. 화면 계약은 [설정](../settings/index.md)에 있다. user 설정은 `~/.tasty/file-handlers.toml`(부팅 1 회 로드, Save는 합친 snapshot을 file writer에 제출하고 receipt로 완료·실패를 구분). 같은 탭의 Hook Handlers 서브탭은 파일 핸들러가 아니라 [공유 훅 핸들러 레지스트리](../webhook/index.md) 편집이다.
- **저장 완료**: Save는 표시값을 먼저 적용하고 detector·handler·확장자 우선순위의 사용자 기여를 한 파일에 합쳐 비동기로 저장한다. 다른 최상위 절은 보존하며 외부 편집·읽기 실패·손상 원본은 덮지 않는다. 저장 실패는 설정 결과 토스트와 알림에 남고 표시값은 이번 실행에 유지된다. 저장 대기와 완료는 [파일 registry snapshot](../../design/systems/storage-contract.md#파일-registry-snapshot)에 있다.
- **AI Agent / CLI**: `file_handler.dispatch`(임의 경로를 흐름에 진입, plugin 호출은 FsRead 권한 — 경로 자리의 URL 은 `-32602` 로 거절) · `file_handler.reload`(user 설정 reload — 응답 `{path, exists, rejected}`) · `file_handler.detectors`(아래) · `tasty file-handler` CLI.
- **detector 조회**: `file_handler.detectors`(`tasty file-handler detectors`)는 finalize 된 detector 전부를 id 순으로 돌려준다 — `{"detectors": [...]}`. 항목마다 병합 결과(`id` · `display_name_i18n_key` · `icon` · `disabled` · `install_order` · `rules`)와 그것을 만든 출처별 원본 `contributions`(`origin` · `display_name_i18n_key` · `icon` · `disabled` · `rules`)를 함께 싣는다. `origin` 은 `host` · `plugin:<id>` · `user` 이고, rule 은 user 설정의 `[[detector.rule]]` 과 같은 키로 적히며 병합 결과 쪽 rule 에만 `origin` 이 붙는다. contribution 의 `disabled` 는 그 출처가 적은 켜기/끄기 patch 이고 적지 않았으면 `null` 이다. `contributions` 는 **설치 순서**이지 병합 순서가 아니다 — 어느 값이 이겼는지는 병합 결과 쪽을 본다. 읽기 전용(local-only, plugin 비노출)이고 두 빌드 모두 답한다.
- **reload 의 거절 보고**: `rejected` 는 이번 reload 가 **적용하지 않은** user 항목의 배열이다 — `[{"id", "reason"}]`, 없으면 `[]`. 사유는 셋이다. `missing_owner_prefix`(id 에 `<owner>/` 접두사가 없어 설치 안 함 — 버려짐) · `missing_detector_or_action`(`user/…` 항목인데 detector 나 action 이 없어 등록 안 됨 — 버려짐) · `target_not_contributed`(host · plugin handler 를 patch 하는 항목인데 대상이 지금 없음 — plugin 이 안 떠 있거나 id 가 틀렸다. 둘은 가를 수 없다. 항목은 남아 있어 대상이 contribute 되면 그대로 적용된다). 헤드리스는 부팅 시 번들 plugin 이 떠 있지 않아, plugin 을 켜기 전까지 정상 plugin patch 도 `target_not_contributed` 로 보고된다. 저장이 pending이면 reload는 오류로 답하므로 저장 완료 뒤 다시 요청한다. 파일을 읽거나 파싱하지 못한 reload도 오류로 답하고 두 registry의 이전 user 설정과 원본 관측을 유지한다. 성공한 명시적 reload는 저장하지 못한 표시 변경도 파일 내용으로 바꾼다.
- **헤드리스 제약**: `file_handler.dispatch` 는 gui 빌드에만 있다. 헤드리스(`--no-default-features`) 데몬은 식별 결과를 적용할 worker 도, 결과를 열 창도 없어 이 요청에 `-32017`(이 빌드 조합에 arm 이 없다)로 답한다 — 수락하고 버리지 않는다. `file_handler.reload` · `file_handler.detectors` 는 두 빌드 모두 답한다.

## 비-목표

- 개별 surface kind 의 렌더/동작 — [concepts/plugins](../../concepts/plugins.md), [plugins/](../../plugins/index.md).

## 관련

- **레지스트리 정본 템플릿**: 이 `FileHandlerRegistry`(3출처 patch 병합 + actor 별 action 스키마 + owner tie-break 정렬)를 공유 훅 핸들러 레지스트리가 미러링한다 — [webhook](../webhook/index.md) · [훅 (Surface / Global hooks)](../hooks/index.md). 차이: file handler 의 `detector`↔hook 의 `source` 게이트, `System` action↔`ShellCommand`(hook 출처 전용).
- [plugin-permissions](../../dev-guide/plugin-permissions.md) · [plugin-development](../../dev-guide/plugin-development.md) · [settings](../settings/index.md)
