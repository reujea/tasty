# 터미널 링크 hover·클릭 (Terminal link)

- **Status**: Implemented
- **주체**: 로컬 사용자 전용 (마우스 + 수식키 — CLI/IPC 비노출)
- **설계**: [URL 대상은 picker·실행 계층에만](../file-handler/index.md) (원칙은 [사용자 통제와 데이터](../../principles/control-and-data.md#사용자에이전트원격-사용자))
- **코드**: 검출·하이라이트 타입 `crates/tasty-terminal-link/` · 링크 열기 `src/adapters/ui/terminal_link.rs`(`open_uri` — OS 부수효과라 `gui` 뒤에 남는다) · 클릭 `src/view/main/mouse.rs` · `LinkModifier`(settings) · 링크 우클릭 메뉴 `src/view/main/link_menu.rs` · 드래그선택 우클릭 메뉴 `src/view/main/redraw.rs`(`handle_terminal_surface_native_menu`)
- **화면**: 링크 hover 하이라이트 (GPU)

## 목적

터미널 출력 속 URL/경로를 수식키+좌클릭 또는 링크 위 우클릭 메뉴로 연다. 좌클릭 열기를 끄면 포커스/selection 동작을 유지하면서 링크 메뉴를 사용할 수 있다.

## 내부 동작

픽셀 좌표 해소와 링크 hit-test는 같은 `TerminalReadView` 안에서 수행한다. 링크
강조와 메뉴의 선택 좌표는 content epoch를 보관하므로 기록 reset이나 alternate clear
뒤 같은 숫자의 행을 이전 링크로 표시하지 않는다.

### 감지 대상

- URL: `http://`·`https://`·`ftp://`·`file://`, OSC 8 hyperlink.
- 스키마 없는 경로: Unix 절대(`/foo/bar`)·Windows 절대(`C:\foo`/`C:/foo`)·접두사 상대(`./`,`../`)·**접두사 없는 상대(`src/main.rs`)** — 단, **OSC 7 기반 CWD 기준 실제 존재할 때만** 링크로 판정(오탐 방지). 접두사 없는 상대는 **경로 구분자 1개 이상 토큰만** 후보(2단 방어: 슬래시 prefilter → `exists()` 검사 — `Makefile`·`and/or`·`TCP/IP` 제외).

### 트리거 (사용자 행동)

설정 `링크 클릭 수식키`의 기본값은 `Ctrl`이며 `Alt` 또는 `좌클릭 열기 끄기`를 선택할 수 있다.

- `Ctrl`/`Alt`: 설정 수식키 조건이 맞으면 hover 하이라이트([터미널 색상](../../surfaces/terminal/index.md#색상--폰트))와 PointingHand 커서가 나타나고, 좌클릭으로 링크를 연다. 수식키+클릭이 링크 위가 아니면 no-op이며 selection을 시작/변경하지 않는다.
- `좌클릭 열기 끄기`(`LinkModifier::None`, 저장값 `none`): 수식키 상태와 무관하게 좌클릭 링크 열기는 실행하지 않는다. 기존 포커스/selection 경로로 진행하며, 링크 위 우클릭 메뉴는 사용할 수 있다.
- `None`에서 항상 참인 것은 **hover의 수식키 조건만**이다. 설정/전체화면 무대 같은 마우스 오버레이, popup·banner hover 차단과 터미널 영역·surface·실제 링크 hit-test는 그대로 적용한다. 따라서 모든 위치에서 hover 표시가 항상 켜지는 모드가 아니다.
- `copy_link` 단축키(기본 키 없음): 마우스 포인터 아래 링크의 화면 텍스트를 메뉴 없이 복사하고 `toast.copied` 를 그 surface 범위로 띄운다. 링크 클릭 수식키는 요구하지 않지만 마우스 오버레이·popup·banner hover 차단, hard 점유 배제, 링크 hit-test 는 hover 와 같다. 포인터 아래 링크가 없으면 키를 소비하지 않고 다음 단축키·터미널 입력 경로로 넘긴다. 키로 실행했으므로 토스트에 hint 를 붙이지 않는다.

### 링크 위 우클릭 → 링크 메뉴

hover 가 판정해 둔 링크(`hovered_link`) 위에서 우클릭하면 기존 터미널 메뉴 대신 링크 전용 네이티브 메뉴가 뜬다. 대상은 드래그로 확정한 selection 문자열이 아니라 hover 의 `LinkSpan` 이다(아래 "드래그/더블클릭 선택 → 우클릭 메뉴" 와 다른 입력 경로).

- **게이트**: hover 링크가 우클릭한 surface 의 것이고, 그 surface 가 hard 점유 mirror 가 아닐 것(좌클릭과 같은 배제 — docs/dev-guide/attach-behavior.md). `LinkModifier::None` 이어도 뜬다 — `hovered_link` 자체가 수식키 게이트를 통과한 결과이고, 좌클릭이 `None` 을 배제하는 이유("수식키 없는 클릭이 링크를 열어버리는 사고")가 우클릭 → 메뉴 → 항목 선택에는 성립하지 않는다.
- **mouse tracking 보다 먼저**: 링크 판정이 `right_click_delegates_to_app` 앞에 있어, tracking 앱(vim/tmux) 안에서도 링크 위 우클릭은 앱으로 가지 않는다(좌클릭 링크와 대칭). press 와 release 모두 로컬 소비된다. Shift+우클릭도 링크 위면 링크 메뉴다.
- **스냅샷**: press 시점에 링크 범위(첫 세그먼트 시작 ~ 마지막 세그먼트 끝)와 그 범위의 화면 텍스트를 찍어 둔다. Linux 는 메뉴를 release 에 열므로 그 사이 수식키를 떼거나 포인터가 움직여도 press 때의 링크로 뜬다.
- **포커스 불변**: 좌클릭 링크와 달리 포커스를 옮기지 않는다(컨텍스트 메뉴 관례).

| 항목 | 동작 |
|------|------|
| 선택 | 스냅샷 범위로 `text_selection`을 설정한다. 메뉴가 떠 있는 동안 화면이 바뀌어 같은 범위가 다른 텍스트를 가리키면 아무것도 안 한다 |
| 복사 | 스냅샷 텍스트를 클립보드로(+토스트). OSC 8 이면 대상 URI 가 아니라 표시 라벨. 토스트 hint 는 `copy_link` binding 이다(비어 있으면 hint 없음, `copy` 는 빌리지 않는다) |
| 연결 동작 | 자동 1순위 실행을 건너뛰고 [핸들러 picker](../file-handler/index.md) 를 연다 — 식별 없이 전체 핸들러가 fallback 후보. 경로 링크와 `http(s)` 링크에서 노출, 그 밖의 scheme(mailto 등)은 항목 없음. 원격(mirror) surface 의 경로 링크는 원격에 열 수 있는 핸들러만 후보·recent 에 싣고, 그런 핸들러가 없으면 picker 대신 `explorer.state.remote_open_unsupported` toast |

picker 에서 고른 핸들러는 focused pane 에 연다(좌클릭 링크와 같다).

### 소프트 wrap(멀티라인) 링크

터미널 폭을 넘는 OSC 8 하이퍼링크가 화면상 여러 줄로 소프트 wrap 되면, hover 하이라이트는 마우스가 그 체인의 몇 번째 줄 위에 있든 **체인에 속한 모든 화면상 줄을 동시에** 하이라이트한다. 판정 기준은 두 조건의 AND: 인접 행이 소프트 wrap(다음 행이 논리적 연속)이고, 그 다음/이전 행이 **동일 URI**의 OSC 8 링크로 이어지는 경우만 병합한다 — 위/아래 양방향, 3줄 이상 체인도 재귀적으로 훑는다. plain-text URL/경로(regex 검출)는 wrap continuation 행에 스킴 프리픽스가 없어 그 행 자체에서 애초에 매치가 나지 않으므로(따라서 URI 도 일치하지 않으므로) 별도 처리 없이 병합 대상에서 자연히 제외된다 — 단일 wrap 행 안에서만 표시된다.

한 번의 조회는 같은 content cut 안에서 각 행을 한 번만 읽는다. 처음 찾는 scrollback 행은 위아래 두 행씩을 함께 한 구간으로 읽어 두므로, 디스크 스왑이 켜진 터미널에서 wrap 체인이 이 범위를 벗어나지 않으면 조회 한 번에 스크롤백 파일을 한 번 연다. 현재 화면 행은 조회마다 한 번 복사한다. 읽어 둔 행은 그 조회에서만 쓰고 다음 마우스 이동에서 다시 읽는다.

### 원격(mirror) surface 라우팅

attach mirror surface(자식 PTY 없음 — `process_id().is_none()` 으로 판별)는 화면 경로가 **원격 호스트 경로**라 로컬 핸들러에 넘기지 않는다:
- 로컬 `exists()` 검증 건너뛰고 OSC 7 원격 cwd 기준 경로를 그대로 링크로 emit(원격 경로도 하이라이트).
- 수식키 클릭은 그 mirror surface 를 origin 으로 사용자 출처의 파일 열기(`DispatchFile`)를 보낸다(`file::dispatch::remote::terminal_link_file_intent`). 이후는 mirror explorer 더블클릭과 같은 [원격 대상 규칙](../file-handler/index.md#원격mirror-대상)이다: 파일 이름만으로 식별하고(client 의 같은 경로 파일은 읽지 않는다), 1순위가 원격에 열 수 있으면 원격 새 탭으로 열어 양쪽에서 선택한다. 1순위가 아니면 원격에 열 수 있는 핸들러만 담은 picker, 하나도 없으면 `explorer.state.remote_open_unsupported` toast 로 끝난다. 로컬 핸들러·OS 열기·브라우저로는 넘기지 않는다.
- `http://` 등 스킴 있는 URL 은 mirror 여부 무관하게 `webbrowser` 로 연다.
- 수식키 hover 로 경로 링크가 잡히면 링크 셀 모양은 그대로 두고 링크 런 아래에 원격 위치 툴팁을 띄운다. 내용은 `attach.mirror_pill_label`(Remote, tooltip-fg) · space-sm · remote 글리프 + 호스트 글자(`link-remote-fg`, mono)다. 호스트 글자가 비면 낱말과 글리프만 그린다. 툴팁 최대 폭(240) 안에서 호스트 끝을 줄인다. 아래에 자리가 없으면 위에 두고, 가로는 창 안으로 당긴다. `tooltip-delay` 뒤에 나타나고 링크가 풀린 프레임에 사라진다. 로컬 surface 의 경로와 스킴 있는 URL 에는 띄우지 않는다. 호스트 글자는 [원격 위치 표시](../remote-attach/index.md#원격-위치-표시)와 같다(`src/adapters/ui/link_remote_tip.rs`, 공용 `tasty_ui_widgets::LinkRemoteTip`).
- 빈 picker 의 머리 경로 줄 맨 앞에는 같은 호스트 표지가 붙는다([핸들러 picker](../file-handler/index.md)).

### 열기 실패

수식키 클릭으로 연 외부 URL 을 OS 가 넘겨받지 못하면(`open_uri` 가 false) 링크가 있던 surface 범위에 실패 토스트(`toast.open_link_failed`, Error)를 하나 띄운다. 성공에는 토스트를 더하지 않는다. `open_uri` 는 열기 프로그램을 실행했는지만 확인하므로 브라우저가 실제로 페이지를 보였는지는 판정하지 않는다. debug 빌드에서 `TASTY_DEBUG_OS_OPEN_LOG` 로 OS 열기를 가로채면 항상 성공으로 처리하므로, 실패 재현은 PATH 에서 열기 프로그램을 뺀 격리 실행으로 한다. 문구 선택은 `terminal_link::open_failure_message` 하나가 맡고 Plugins 창과 System 핸들러도 같은 함수를 쓴다.

> 비-목표: 원격 호스트의 프로그램으로 여는 것(`ssh host vim {path}` 등)과 그 host 컨텍스트 주입.

### 드래그/더블클릭 선택 → 우클릭 메뉴 "경로 열기"

hover+수식키 클릭과는 별개 입력 경로 — 사용자가 드래그(또는 더블/트리플클릭)로 이미 확정한 selection 텍스트가 대상이다. 선택 모드(문자/단어/줄/블록) 구분 없이 `extract_selected_text` 가 뽑아준 문자열을 그대로 1차 후보로 쓴다(hover 경로의 `path_regex()` 는 재사용하지 않음 — 비-ASCII 후행 문자가 매치 단계에서 잘려나가면 아래 축약 재검사 자체가 무의미해지기 때문).

- **판별**: `longest_existing_selection_path`(`terminal_link.rs`). 1차 후보(선택 문자열 trim + `trim_trailing_punct`)가 (cwd 결합 후) 존재하지 않으면 마지막 `/` 앞까지 잘라 재검사하고, 실패하면 그 앞 `/`로 계속 반복 — 실재하는 가장 긴 접두사를 채택한다. 예: 실제 경로 뒤로 슬래시 없이 문자(한글 조사 등)가 붙어 선택돼도, 그 앞 경로 접두사가 실재하면 그 경로로 채택.
- **노출 조건**: 우클릭한 surface 와 `text_selection.surface_id` 가 같을 때만 "경로 열기" 항목을 추가한다 — surface 별로 독립적인 드래그 상태를 가질 수 있어, 다르면 노출하지 않는다(기존 복사 메뉴는 surface 무관 전역 selection 관례 그대로 유지).
- **라벨**: `Path::is_dir()` 로 파일/폴더를 구분해 다른 라벨을 표시(`terminal_context_menu.open_file`/`open_folder`).
- **열기**: `crate::platform::reveal::open_path`(explorer 컨텍스트 메뉴의 "시스템에서 열기"와 동일 함수).
- **mirror(원격 attach) surface**: 로컬 파일 관리자로 원격 경로를 여는 기능이 없어 제외(`terminal.process_id().is_none()`이면 항목 자체를 노출하지 않음). 이 기능을 지원하면 재검토.

## 인터페이스

- **사용자**: 수식키 + hover/클릭, hover 링크 위 우클릭 → 선택 / 복사 / 연결 동작, 링크 위에서 `copy_link` 키, 또는 드래그/더블클릭 선택 후 우클릭 → "경로 열기".
- **AI Agent**: **없음(비노출)** — 사용자 입력 재현 금지([사용자 통제와 데이터](../../principles/control-and-data.md#사용자에이전트원격-사용자)). 링크/경로 *데이터* 가 필요하면 읽기 전용 [terminal-output](../terminal-output/index.md) 의 `path`/`url`/`osc_link` 파서([reference/output-parsers](../../reference/output-parsers.md)).

## 비-목표

- 출력 감지 시 자동 열기 — 열기는 항상 사용자 클릭.
- 존재하지 않는 경로의 추측성 링크화 — CWD 실존 검사 통과분만.
- 수식키 없는 좌클릭으로 링크 열기 — 좌클릭은 포커스/selection 경로를 따른다.

## 관련

- [terminal](../../surfaces/terminal/index.md) · [terminal-output](../terminal-output/index.md)(읽기 경로) · [file-handler](../file-handler/index.md)(경로 핸들러)
