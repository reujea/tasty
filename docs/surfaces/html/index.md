# HTML Viewer (`com.tasty.html`)

## 원칙과 현재 동작

[주체별 원칙](../../principles/control-and-data.md)에 따라 포커스와 무관한 문서 실행 제어와 사용자 입력 재현은 구분한다. 스크립트 허용은 포커스와 무관한 문서 실행 제어이므로 release에서 사용자(배너)와 에이전트(`surface.html_script_allow`) 모두 할 수 있다. 원본 스캔은 [응답성 원칙](../../principles/performance-and-settings.md)에 따라 UI 스레드 밖에서 한다(아래 [스캔](#스캔)). Windows·macOS의 결정 보류는 Tasty 실 기기에서 측정하지 않았다. native 생성자·gate의 ARM 교차 컴파일과 전체 GUI 앱 빌드는 검증 범위가 다르다(아래 [OS별 적용](#os별-적용)).

- **Status**: Implemented (bundled plugin)
- **주체**: 로컬 사용자 (GUI surface) · AI Agent (`tasty html` CLI)
- **배포/통합**: bundled · surface_kind(webview) · 파일 핸들러 — [plugins 개념](../../concepts/plugins.md)
- **코드**: `crates/tasty-plugin-html/`, host WebView 오버레이
- **권한**: 매니페스트 `permissions`
- **화면**: [아래 절](#화면)

> **예제로서**: `rendering = "webview"` surface 의 예제 → [plugin-development](../../dev-guide/plugin-development.md#surface-kind--rendering-3-종).

## 원격 전송 기반

서버는 생성·복원 URL과 `html.open`의 최신 요청 URL을 GUI/headless 공통 source 세대로 관리한다. 파일 source의 부모가 전송 root·cwd이며 HTTP(S)는 전달받은 cwd를 보관하고 세션 저장 자료에 함께 담아 복원한다. snapshot 영속화와 native commit 관측은 별개다. [HTML 전송 계약](../../dev-guide/html-transfer.md)에 따라 Linux·Windows·macOS GUI client는 완성된 파일 entry를 받아 native WebView로 표시한다. 양쪽 entry viewer 협상이 없거나 client HTML plugin이 미등록이면 placeholder/deferred를 유지한다. 서버와 하위 파일 capability를 협상하면 같은 root의 CSS·이미지·script·하위HTML 등 파일 GET도 받는다. 구 서버는 entry만 표시하고 HTTP(S)를 제공하지 않는다. HTTP(S)는 [원격 HTTP(S) native viewer](#원격-https-native-viewer)가 맡는다. client 파일·네트워크로 대체하지 않는다.

원격 주소는 읽기 전용이며 Reload만 제공한다. 100000 bytes를 넘는 각 파일은 Allow·Decline을 기다리고 여러 질문은 하나씩 보인다. 문서 누적2000000 bytes에서 Continue·Stop을 기다린다. Decline·실패는 해당 파일만 끝내고 skipped를 Reload까지 유지한다. Stop은 완료한 본문을 보존하고 나머지 요청을 끝낸다. 크기 → 스크립트 → 페이지 순서로 배치한다. 스크립트는 client 설정으로 판정하며 원격 설정·허용을 전달하지 않는다. Allow는 이미 스캔한 body를 재사용하고 사용자 Reload는 새 요청으로 누적량·크기 허용을 초기화한다. 같은 지문은 기존 스크립트 허용을 유지하고 다른 지문은 다시 차단한다. 에이전트의 `surface.html_reload`도 새 entry를 요청하되 호스트 출처이므로 배너 열람을 이어받지 않는다. 서버의 metadata 이후 30초 credit timeout으로 전송이 끝나도 크기 질문은 남으며, 그 뒤의 Allow는 같은 대상을 새로 요청해 받는다(크기가 바뀌었으면 다시 묻는다). 누적 정지 중 만료된 파일은 Continue 때 다시 요청한다. 세부 계약은 [HTML 전송 계약](../../dev-guide/html-transfer.md)에 있다. 완료한 본문은 연결이 끊겨도 보존하지만 Reload는 비활성이다. 재연결은 새 connection epoch로 다시 받는다.

## 목적

HTML / 웹 콘텐츠를 보는 **`html` surface 종류**를 제공한다. `rendering = "webview"` — tasty 의 **네이티브 WebView 오버레이**로 그린다(host 가 surface 별 URL 을 동기화).

## 내부 동작

- **surface_kind `html` (webview)** — host 트리엔 `RemoteSurface` marker, 실제 콘텐츠는 네이티브 WebView 오버레이. surface 의 `webview_url()` 로 URL 식별.
- **파일 핸들러** — `handler` 둘: `viewer`(detector `html`)와 `svg-viewer`(detector `svg`), 둘 다 `open_surface{surface_kind:"html", param_key:"url"}`. `detector "html"`·`detector "svg"` 는 **host 가 유지**(`default-file-format.toml`) — 플러그인 disable 시에도 확장자 인식이 남도록. HTML·SVG 파일 열기 시 이 surface. SVG 는 image 플러그인이 디코드하지 못하므로 WebView 가 렌더하며, `svg` detector 에 붙는 기본 핸들러가 이것 하나라 picker 없이 열린다.
- **cli** — `tasty html open …`. `html.*` IPC(URL 설정 등 — `webview.set_url`).
  - `url` 인자는 URL(`http://`·`https://`·`file://`) 또는 로컬 파일 경로다. CLI 매니페스트가 `path_kind = "url_or_file"`이라 상대 경로는 CLI를 호출한 디렉터리 기준 절대 경로로 바뀌고, 파일이 없으면 요청을 보내지 않고 실패한다.
  - `html.open`은 생성 경로와 같은 `local_path_to_file_uri`로 절대 경로를 `file://` URI로 바꿔 `webview.set_url`에 넘긴다. IPC로 직접 온 상대 경로는 기준 디렉터리가 없어 `invalid_params`로 거절한다.
  - `tab.create` 응답 직후의 `html.open`도 적용된다. 호스트는 surface를 만들고 응답한 뒤 생성 요청을 플러그인에 넘기므로, 플러그인이 `html.open`을 생성 요청보다 먼저 처리할 수 있다. 플러그인은 생성 전에 `html.open`을 받은 surface를 기억했다가 생성 처리 때 생성 URL의 `webview.set_url`을 생략해 더 새로운 요청을 덮지 않는다. 스냅샷은 그대로 생성 URL이다.
  - `html.open`으로 바꾼 URL과 주소 표시줄로 연 주소는 스냅샷에 남지 않는다. 스냅샷은 생성·복원 때의 URL이며 호스트가 `surface.snapshot`을 다시 묻지 않으므로, 재시작하거나 플러그인 프로세스가 바뀌면 생성 URL로 돌아간다. 빈 탭으로 만든 surface는 빈 탭으로 복원된다. 두 경로를 같은 정책으로 두어 페이지 안 링크 이동과도 맞춘다.
- **스크립트 감지와 문서 단위 허용** — 전역 설정 sandbox scripts가 켜져 있으면 host가 main frame 문서마다 JS를 끈다. `file://` 문서는 응답 단계에서 원본 파일을 읽어 스크립트를 감지하고 파일 전체 지문을 구한다. 허용은 URL(fragment 제외)과 지문에 묶이며 다른 문서가 commit되면 풀린다. 규칙은 아래 [스크립트 감지와 허용 규칙](#스크립트-감지와-허용-규칙) 절.
  - 스캔 상한: 감지는 파일 앞 4 MiB(`SCAN_LIMIT_BYTES`)까지만 읽는다. 지문은 상한과 관계없이 파일 전체를 해시한다.
  - 정규 파일만 스캔한다. FIFO·장치 같은 파일은 읽지 않고 지문 없음으로 두므로 그 문서는 허용할 수 없다.
  - 파일 열기와 정규 파일 확인은 host(`src/host_api/webview/script_gate.rs`)가 한다. `tasty_model::html_script::scan`은 받은 바이트로 감지와 지문만 계산하며 파일 시스템에 접근하지 않는다.
  - Linux html surface는 WebKit page cache를 끈다. 뒤로·앞으로 가기도 캐시 복원이 아니라 파일을 다시 읽는 새 로드가 되어 같은 게이트를 거친다.
  - 배너 발화 판정은 `tasty_model::html_script::banner`에 있다. 사용자가 그 문서를 봤을 때만 배너 단계가 `blocked`가 되고, 에이전트가 연 문서·세션 복원·보이지 않는 탭의 문서는 `pending_view`만 기록한다. 판정은 포커스와 활성 탭을 바꾸지 않는다.
  - 배너 그리기: `src/adapters/ui/surface/html_script_banner.rs`가 webview chrome 위에 inset 배너를 그린다([배너 시스템 §inset 배치](../../design/systems/banner.md#inset-배치)). 그린 카드 아래 `banner_inset_gap`까지의 높이를 MainViewState에 남기면 같은 프레임의 WebView 동기화가 그만큼 WebView를 내리고 줄인다. surface 폭이 `banner_narrow_below` 미만이면 narrow 배치(액션이 다음 줄)로 그린다. surface가 새 문서를 로드하는 동안(로드 시작부터 main frame commit 전까지) 단계는 `loading`이고 허용 버튼은 비활성이며 hover하면 위쪽 툴팁으로 이유를 보인다. 이 동안 모델도 허용을 받지 않는다(`AllowError::Loading`). commit 뒤 새 문서에 스크립트가 있으면 `blocked`로 돌아오고 없으면 배너가 사라진다. 허용을 누르면 재로드 중 배너가 액션 자리에 스피너를 보이고, 새 문서가 commit되어 단계가 `hidden`이 되면 `banner_fade` 동안 흐려지며 사라진다. 닫기(×)와 문서 교체는 즉시 사라진다. 배너는 키보드 포커스를 가져가지 않는다.
  - 로드 실패: backend가 main frame 로드 실패를 알리면(WebKitGTK `load-failed`·web process 종료, WKWebView `didFail`·`didFailProvisional`·web content process 종료, WebView2 `NavigationCompleted` 실패·렌더러/브라우저 프로세스 종료) `HtmlScriptState::on_load_failed`가 실패 표지를 세운다. 표지가 있는 동안 배너 단계는 `hidden`이고 탭 표지는 없다. 재로드 중 배너도 페이드 없이 바로 내린다. 둘 다 이전 문서를 가리키고 허용할 문서가 없기 때문이다. 다음 main frame commit이 표지를 지우고 배너·표지를 새 문서로 다시 정한다. commit 전의 재로드 시작만으로는 지우지 않는다.
    - chrome의 Failed와 이 실패 표지는 같은 로드의 실패에만 선다. Windows·macOS는 새 로드가 앞 로드를 취소해 앞 로드의 실패가 늦게 오면 둘 다 건드리지 않는다. 앞 로드의 늦은 성공도 새 로드의 Loading을 Done으로 바꾸지 않는다(로드 세대 판정, [OS별 적용](#os별-적용)). web process 종료는 세대와 관계없이 둘 다 세운다. 화면 문서를 잃었기 때문이다. Linux는 취소된 앞 로드의 `load-failed`가 새 로드의 시작보다 먼저 와서 세대 판정 없이 같은 결과가 된다(측정).
  - 탭 표지: `HtmlScriptState::marker`가 표지를 정한다. 차단된 배너를 닫았으면 lock(`Blocked`), 현재 문서를 허용했으면 scriptFile(`Allowed`)이고 sandbox가 꺼져 있으면 없다. 탭 바는 이동 글리프 왼쪽에 `html_script_marker_hit` 칸과 `tab_status_gap`을 잡아 표지를 그린다. 탭에 html surface가 여럿이면 `Blocked`를 먼저 보인다. lock 클릭은 배너만 다시 보이며 탭 전환·포커스 변경은 하지 않는다. scriptFile은 툴팁만 있다. 표지 툴팁은 [탭 스트립 툴팁 규칙](../../design/systems/banner.md#탭-스트립-툴팁)을 따라 탭 위쪽에 뜨므로 html 탭이 활성이어도 WebView에 가려지지 않는다.
  - 조회: `tasty surface html-script --surface <id>`(IPC `surface.html_script`)가 현재 문서의 URL·감지 결과(`none`/`scripts`/`scripts_remote_only`)·지문, 허용 기록, 현재 JS 적용 값, 로드 진행 여부(`loading`, 로드 시작 뒤 commit 전이면 `true`), 스캔 진행 여부(`scanning`, 작업 스레드의 스캔이 끝나지 않았으면 `true`. 이때 `document.fingerprint`는 `null`일 수 있다), 배너 단계(`hidden`/`blocked`/`loading`/`reloading`)와 표지(`viewed`·`pending_view`·`shown`·`dismissed`)를 돌려준다. 읽기만 하므로 배너를 띄우거나 사용자가 본 것으로 기록하지 않는다. html이 아닌 surface는 거절하고 헤드리스 빌드는 빌드 미지원 오류(`-32017`)로 답한다.
  - 허용: 사용자는 배너의 허용 버튼으로, 에이전트는 `tasty surface html-script-allow --surface <id> [--url <url> --fingerprint <hex>]`(IPC `surface.html_script_allow`, release, 권한 `surface.write`)로 허용한다. 둘 다 `HtmlScriptState::allow_current`로 현재 문서의 URL·지문을 허용 기록에 넣고 재로드를 요청한다. 에이전트 경로는 포커스·선택·활성 탭과 배너 표지를 바꾸지 않는다.
    - 응답은 `{surface_id, result, reload, document}`다. `result`는 새로 허용했으면 `allowed`(`reload: true`), 이미 허용된 문서면 `already_allowed`(`reload: false`, 재로드하지 않음)다.
    - `url`·`fingerprint`는 `surface.html_script` 조회의 `document.url`·`document.fingerprint`를 넘긴다. 현재 문서와 다르면 허용하지 않는다. 생략하면 그 비교를 하지 않는다.
    - 거절은 `-32022`이고 `error.data.reason`으로 이유를 구분한다. `document_changed`(조회 뒤 문서가 바뀌었다), `loading`(새 문서가 commit 전이다), `no_document`(commit된 문서가 없다), `no_fingerprint`(`file://`이 아니거나 응답 단계 없이 commit된 문서다). `error.data.document`에 현재 문서를 싣는다. 거절하면 상태를 바꾸지 않는다.
    - html이 아닌 surface·없는 surface·미지원 placeholder는 invalid params로 거절하고, 헤드리스 빌드는 `-32017`로 답한다.
- **문서 주소와 히스토리** — 주소 표시용 주소는 backend가 관측한 commit 주소다. `webview_url()`은 호스트가 넣은 주소라 페이지 안 이동을 반영하지 않으므로 표시에 쓰지 않는다.
  - backend는 main frame commit마다 그 시점의 엔진 주소를 기록한다(WebKitGTK `load-changed` Committed, WKWebView `didCommitNavigation`, WebView2 `ContentLoading`). 주소 선택은 `tasty_model::committed_document_url`이 정한다. 로드가 끝난 상태(`Done`·`Idle`)에서는 엔진의 현재 주소를 쓰고, 로드 중(`Loading`)이나 실패(`Failed`)에서는 마지막 commit 주소를 쓴다.
  - 그래서 리다이렉트 뒤에는 최종 주소가 보이고, 같은 문서 안 `#fragment` 이동과 history API 이동은 commit 신호 없이도 반영된다. commit 전에 실패한 로드는 이전 문서의 주소를 남긴다. commit 뒤 로드가 끝나기 전에는 새 문서 주소다.
  - 뒤로·앞으로 이동 가능 여부는 엔진의 세션 히스토리 조회값이다(`can_go_back`·`can_go_forward`, WKWebView `canGoBack`·`canGoForward`, WebView2 `CanGoBack`·`CanGoForward`).
  - 호스트는 WebView 동기화 때 프레임마다 이 값들을 `RemoteSurface::webview_nav`에 옮기고, 바뀌면 한 프레임 더 그린다. 주소 표시줄 UI가 이 사본을 읽는다. 프레임마다 조회하므로 Windows는 조회 실패를 웹뷰별 첫 번째만 warn으로 남기고 이후는 debug로 남긴다.
  - 조회: `tasty surface html-nav --surface <id>`(IPC `surface.html_nav`)가 `url`(commit 주소), `requested_url`(호스트가 넣은 주소), `nav_state`(`idle`/`loading`/`done`/`failed`), `can_go_back`·`can_go_forward`를 돌려준다. 읽기만 한다. html이 아닌 surface는 거절하고 헤드리스 빌드는 `-32017`로 답한다.
  - 지금 문서 안의 `#fragment` 주소를 열면 WebKitGTK는 load 신호 없이 같은 문서 이동으로 처리한다(측정). 그래서 Linux `load_url`은 로드가 끝난(`Done`) 상태에서 이 경우를 만나면 `Loading`을 세우지 않는다. 세우면 끝나지 않는 로드로 남는다. 판정은 `tasty_model::is_same_document_fragment_move`다.
  - 남은 한계: `Failed`·`Loading` 상태에서 같은 문서의 fragment 주소를 열면 이 처리를 하지 않고 `Loading`을 세운다. 그 이동이 load 신호 없이 끝나면 `Loading`이 풀리지 않을 수 있다(미측정).
  - 남은 한계: 같은 문서 판정은 `#` 앞 문자열 비교다. 엔진이 정규화한 표기(끝 슬래시, `%20` 같은 퍼센트 인코딩, 대소문자)와 넣은 주소의 표기가 다르면 같은 문서로 보지 않고 새 로드로 처리한다.
  - Linux 측정: 사용자 탐색의 뒤로·앞으로와 이동 가능 여부, 같은 문서 fragment 열기와 페이지의 fragment 이동(meta refresh), 없는 파일 열기 실패 뒤 이전 주소 유지, meta refresh와 HTTP 302 뒤 최종 주소를 격리 debug 인스턴스에서 확인했다.
  - macOS·Windows 경로는 Linux와 같은 규칙으로 구현했지만 실 기기에서 측정하지 않았다. 같은 문서 fragment 열기의 로드 신호도 확인하지 않았다.
- **사용자 탐색** — 주소 입력·뒤로·앞으로·새로고침은 사용자 탐색 큐(`MainViewState::webview_user_nav`, `WebViewUserNav`)를 지난다. WebView 동기화가 다음 redraw에서 꺼내 backend의 `load_url`·`go_back`·`go_forward`·`reload`를 부르기 전에 `HtmlScriptState::on_user_load_requested`로 사용자 출처를 표시한다.
  - 호스트 로드(`webview_url` 변경 → `on_host_load_requested`)와 달리 사용자 출처 로드의 문서는 사용자가 본 것으로 한다. 그래서 스크립트 배너가 보류되지 않고 commit 뒤 바로 뜬다. 앞선 호스트 로드 표지도 지운다.
  - 로드가 일어나지 않을 조작은 표지를 남기지 않고 버린다. 히스토리 처음의 뒤로, 끝의 앞으로, 문서 없는 새로고침, `file://`·`http(s)://`가 아닌 주소 열기가 그렇다.
  - 지금 문서의 `#fragment` 주소 열기(`is_same_document_fragment_move`)는 탐색하되 사용자 출처 표지를 세우지 않는다. 같은 문서 이동은 commit·로드 종료 신호가 없어 표지를 내릴 수 없고, 남은 표지가 다음 문서(페이지 스크립트·meta refresh로 바뀐 문서)에 잘못 적용되기 때문이다. 호스트 로드도 같다. `webview_url`이 지금 문서의 fragment 주소로 바뀌면 호스트 로드 표지(`on_host_load_requested`)를 세우지 않는다.
  - 남은 한계: 뒤로·앞으로가 같은 문서의 fragment 히스토리 항목으로 가는지는 미리 알 수 없어 표지를 세운다. 그 이동도 load 신호 없이 끝나므로 표지가 남고, 다음에 commit되는 문서를 사용자가 본 것으로 기록한다. 사용자가 그 surface를 조작한 뒤라 영향은 작다.
  - 주소 열기는 해석을 마친 주소만 받는다. 상대 경로 해석과 없는 파일 거절은 입력 쪽이 한다. 이 경로는 `webview_url`을 바꾸지 않으므로 페이지 안 링크 이동과 같이 snapshot에 남지 않는다.
  - 에이전트 경로는 이 큐를 쓰지 않는다. 에이전트가 주소를 바꾸는 방법은 `tasty html open`(호스트 로드)이고, 뒤로·앞으로·새로고침은 아래 [에이전트 히스토리 이동](#내부-동작)이다.
  - 사용자 진입점은 아래 [주소 표시줄](#주소-표시줄)이다. debug 빌드의 `debug.webview.history`(CLI `tasty debug webview-history`)가 같은 큐로 사용자 조작을 재현한다([debug IPC](../../dev-guide/debug-ipc.md)).
- **에이전트 히스토리 이동** — `tasty surface html-back`·`html-forward`·`html-reload --surface <id>`(IPC `surface.html_back`·`surface.html_forward`·`surface.html_reload`, release). 대상은 surface ID로만 받고 포커스·선택·활성 탭을 바꾸지 않는다.
  - 핸들러는 요청을 대상 surface가 속한 창의 에이전트 히스토리 큐(`MainViewState::webview_agent_nav`, `WebViewHistoryStep`)에 넣고, WebView 동기화가 다음 redraw에서 꺼내 backend의 `go_back`·`go_forward`·`reload`를 부른다. 사용자 탐색 큐와 나뉜다.
  - 이 이동은 호스트 출처 로드다. 실행 전에 `HtmlScriptState::on_host_load_requested`를 세우므로, 사용자가 보던 surface라도 다음 문서는 그 열람을 이어받지 않고 스크립트 배너는 사용자가 그 surface를 선택할 때까지 `pending_view`로 보류된다.
  - 로드가 일어나는지는 사용자 탐색의 뒤로·앞으로·새로고침과 같은 기준(`WebViewHistoryStep::loads`)으로 정한다. 히스토리 처음의 뒤로, 끝의 앞으로, 문서 없는 새로고침은 로드가 없으므로 표지를 세우지 않는다. 같은 문서 fragment 히스토리 항목으로 가는지는 미리 알 수 없어 표지를 세운다(사용자 탐색과 같은 남은 한계). 그 이동은 load 신호 없이 끝나므로 호스트 표지가 남고, 다음에 commit되는 문서도 호스트 로드로 처리한다.
  - 응답은 대상 확인을 통과하면 항상 `{surface_id, queued: true}`다. 갈 곳이 없는 이동(히스토리 처음의 뒤로, 끝의 앞으로, 문서 없는 새로고침)도 오류가 아니라 성공이며, 적용할 때 backend 값으로 판정해 표지 없이 버리므로 아무 일도 일어나지 않는다. 결과는 `surface.html_nav`로 확인한다. 적용할 때 그 창에 WebView가 아직 없으면(만들기 전이거나 만들기에 실패한 surface) 경고 로그만 남기고 버린다.
  - 같은 redraw에서는 사용자 탐색 큐를 먼저, 에이전트 히스토리 큐를 다음에 처리한다.
  - 창이 없는 parked 세션의 surface도 대상 확인을 통과해 `queued: true`로 답하고, 요청은 그 세션의 큐에 쌓인다. 창이 복원되면 첫 redraw가 WebView를 새로 만들고 초기 로드를 시작한 뒤 큐를 적용한다. 새 WebView는 히스토리가 없고 아직 commit한 문서도 없어 뒤로·앞으로·새로고침 모두 갈 곳 없는 이동으로 버려진다(코드 순서와 공통 주소 선택 규칙에 따른 서술이며 실제 실행으로 재지 않았다).
  - 핸들러가 관측 사본(`surface.html_nav`가 읽는 값)으로 미리 판정하지 않는 이유: 사본은 redraw 때만 갱신된다. Linux 실제 실행에서 뒤로 간 뒤 3초 동안 다른 요청 없이 기다렸다가 앞으로를 요청하면, 사본의 `can_go_forward`가 아직 `false`라 4회 중 2회 갈 수 있는 이동을 거절했다. 그래서 판정은 적용 시점의 backend 값으로만 한다.
  - html이 아닌 surface와 없는 surface는 `surface.html_nav`와 같이 invalid params로 거절한다. 협상된 HTML mirror의 Reload는 새 entry를 받고 수신량·크기 허용을 초기화한다. Back·Forward는 적용하지 않으며 미지원 placeholder는 거절한다. 원격 호스트로 스크립트 허용을 전달하지 않는다. 헤드리스 빌드는 `-32017`로 답한다.
  - 멱등 계약은 Mutate다. 앞 이동이 적용된 뒤(다른 redraw)에 같은 요청을 다시 보내면 두 단계 이동하거나 두 번 새로고침한다(Linux 측정: 약 0.1초 간격으로 보낸 뒤로 두 번은 다른 redraw에 적용되어 c에서 a까지 갔다, 3회).
  - 같은 redraw에 들어간 요청들은 모두 이동 전 항목을 기준으로 판정하고 실행한다. 앞 이동의 commit 전이기 때문이다. Linux(WebKitGTK)에서 측정한 결과는 다음과 같다. 요청을 한 redraw에 모으려고 debug `gpu-stall`로 이벤트 루프를 멈춘 사이에 보냈고, 적용 로그의 두 줄 간격은 0.02~0.27ms였다.
    - 뒤로 두 번: 두 번째도 갈 곳이 있다고 판정해 실행했지만 한 단계만 이동했다(c → b, 3회).
    - 히스토리 끝에서 뒤로 다음 앞으로: 앞으로는 이동 전 값(`can_go_forward=false`)으로 판정되어 버려지고 뒤로만 남았다(c → b, 4회).
    - 히스토리 처음에서 뒤로 다음 앞으로: 뒤로는 버려지고 앞으로만 실행됐다(a → b, 2회).
    - 가운데에서 뒤로 다음 앞으로: 둘 다 실행했지만 결과는 이동 전 항목(b)에서 앞으로 한 단계였다(b → c, 3회).
    - 즉 측정한 경우에는 마지막으로 실행된 이동 하나만 결과에 남았다. 응답은 모두 `queued: true`이므로 결과는 `surface.html_nav`로 확인한다. macOS·Windows는 측정하지 않았다.

## 주소 표시줄

html surface 위쪽에 host가 주소 띠를 그린다. 띠는 공용 view `tasty_ui_widgets::html_address_bar`이고 갤러리와 같은 코드다([UI widgets](../../architecture/ui-widgets-crate.md#html-address-band-primitive)). 연결은 `src/adapters/ui/surface/html_address.rs`가 맡는다. 플러그인은 바뀌지 않고, 문서 DOM에 아무것도 넣지 않는다.

- **구성** — 뒤로·앞으로·새로고침 버튼과 주소 입력란. Go 버튼은 없고 Enter로 연다. 버튼은 사용자 탐색 큐(`WebViewUserNav::Back`·`Forward`·`Reload`)에 넣고, 뒤로·앞으로의 비활성은 `can_go_back`·`can_go_forward`가 정한다. 로딩 중에는 새로고침 자리에 Spinner가 보이고 누를 수 없다. surface 폭이 `banner_narrow_below`(440) 미만이면 앞으로 버튼을 숨긴다.
- **보이는 주소** — 로드가 끝난 상태(`Done`·`Idle`)에서는 commit 주소(`surface.html_nav`의 `url`)를 보인다. 로드 중(`Loading`)과 실패(`Failed`)에서는 시도 주소를 보인다. 시도 주소는 띠로 연 주소와, 호스트가 넣은 주소(`webview_url`, `tasty html open`)가 바뀌었을 때의 새 주소다. 시도 주소는 로드를 본 뒤 로드가 끝나거나 commit 주소가 그 주소가 되면 지운다. 실패하면 다음 시도까지 남으므로 chrome은 "Failed to load"를, 띠는 시도한 주소를 보이고 commit 주소는 이전 문서를 가리킨다. 페이지 안 링크로 시작한 로드가 실패하면 시도 주소를 알 수 없어 commit 주소를 보인다(남은 한계).
- **표시 형식** — 로컬 문서는 파일 경로(홈 폴더는 `~`)와 query·fragment, 웹 문서는 URL 전체다. 앞 글리프는 로컬 file, 웹 link, 빈 탭 html이다. 입력란이 비어 있으면 placeholder "Enter a file path or URL"을 보인다. 넘치는 주소는 왼쪽부터 말줄임한다.
- **입력 해석** — Enter로 확정한 입력을 `tasty_utils::html_address::resolve_html_address`가 해석한다. 절대 경로, `~` 경로, `file://`·`http(s)://` 주소를 받는다. 상대 입력은 현재 문서가 로컬 파일이면 그 폴더 기준, 웹 문서면 그 URL 기준, 문서가 없으면 surface cwd 기준이다. 기준이 없으면 요청을 보내지 않고 거절한다. 앞뒤 공백만 있는 입력은 아무것도 하지 않는다.
- **로컬 파일 확인** — 로컬 대상은 열기 전에 작업 스레드가 파일인지 확인한다(`std::fs::metadata`). 결과를 기다리는 동안 띠는 50ms마다 다시 그려 결과를 본다. 결과가 오기 전에 입력이 바뀌거나 Esc로 취소하면 그 결과를 버린다. 없는 파일과 확인할 수 없는 경로는 "File not found", 폴더 같은 파일이 아닌 대상과 지원하지 않는 주소는 "Enter a valid file path or HTTP(S) URL", 기준 없는 상대 입력은 "Can't resolve a relative path"로 거절한다. 확인을 통과해도 실제 로드가 성공한다는 보장은 아니다. 실제 실패는 위 실패 상태로 보인다.
- **오류 행** — 거절하면 입력란 테두리가 `input-border-invalid`가 되고 띠 아래에 24px 오류 행이 생긴다. 편집 중인 입력과 이전 문서·주소는 그대로다. 다음 키 입력, Esc, 성공한 열기에서 지운다. 타이머로 지우지 않는다.
- **열기** — 확정한 주소만 `WebViewUserNav::Open`으로 사용자 탐색 큐에 넣는다. 사용자 출처 로드이므로 스크립트가 있는 문서는 commit 뒤 배너가 바로 뜬다. `allow_remote_content` 설정과 문서 단위 원격 허용은 연 주소의 top-level 요청을 막지 않고, 새 문서의 하위 원격 요청에만 적용된다([top-level 탐색](#top-level-탐색)). 연 뒤에는 편집을 끝내 띠가 시도 주소와 commit 주소를 보인다.
- **빈 탭** — 호스트 주소가 없는 html surface(`tasty new tab --type html`)는 처음에 native WebView가 없다. 띠로 주소를 열어 사용자 탐색 큐에 들어오면 그 redraw에서 WebView를 만들고 같은 큐로 첫 로드를 한다. 그 뒤로 WebView를 유지한다. 빈 상태 chrome은 html 글리프, "No page loaded", 그리고 "Type a path or URL in the address bar · <단축키>" 힌트를 보인다. 힌트의 단축키는 주소창 포커스 바인딩의 첫 값이며 바인딩이 없으면 힌트를 그리지 않는다.
- **배치** — 쌓는 순서는 탭 스트립 → 띠(+오류 행) → 크기 안내(mirror) → 원격 콘텐츠 안내 → 스크립트 배너 → 페이지다. 안내는 합치지 않고 따로 카드로 쌓는다. 카드마다 위에 `banner_margin`, 아래에 `banner_inset_gap`을 두며 다음 카드는 앞 카드의 간격 끝에서 `banner_inset_gap`만큼 올라가 시작하므로 카드 사이 간격도 8이다(`html_script_banner::draw_stack`). egui 패스가 띠와 배너의 높이 합을 `MainViewState::html_content_insets`에 남기고, WebView 동기화(`MainView::html_content_top`)가 같은 프레임에 물리 px로 바꿔 페이지를 그만큼 내린다. 배너가 사라지면 페이지는 띠 바로 아래로 돌아온다. 빈 상태·로딩·실패 chrome도 띠 아래에 그린다.
- **포커스와 키** — 입력란을 누르거나 주소창 포커스 단축키를 누르면 egui 입력란이 키보드 포커스를 받는다. 띠가 포커스를 가진 동안에는 host 팝업·복원 안내 입력과 같은 키보드 게이트(`MainViewState::keyboard_overlay_open`)가 열려 실제 키·IME가 egui로 가고, 터미널과 Tasty 단축키로는 가지 않는다. 포커스를 가진 띠가 그 프레임에 그려지지 않으면(탭·workspace 전환) 포커스를 놓아 게이트를 닫는다. 띠가 포커스를 새로 얻으면 WebView 동기화가 그 surface의 native 페이지에서 OS 키보드 포커스를 회수한다. Linux X는 포커스 창의 자손 위에 포인터가 있으면 키를 그 자손에 보내므로, 띠가 포커스를 가진 동안 그 페이지 창 전체를 입력 shape에서 뺀다. 그래서 포인터가 페이지 위에 있어도 키가 페이지로 가지 않는다. 이 동안 페이지는 포인터도 받지 않으며, 페이지 자리를 누르면 그 누름으로 띠의 편집이 끝나고 shape가 돌아온다(XFixes가 없으면 shape를 바꾸지 못한다). macOS·Windows는 키를 키보드 포커스 창에만 보내므로 shape를 바꾸지 않는다. 띠가 포커스를 가진 동안 페이지가 포커스를 새로 얻으면 띠의 편집을 끝낸다. 회수 요청이 늦게 반영되는 동안 남은 페이지 포커스는 새로 얻은 것으로 보지 않는다. OS 포커스 조회는 띠가 포커스를 가진 동안에만 한다. 띠의 포커스는 주소 입력과 사용 가능한 탐색 버튼(뒤로·앞으로·새로고침. 비활성 버튼, 숨은 앞으로, 로딩 중 Spinner는 제외)을 모두 포함한다. 그 안에서 `Tab`·`Shift+Tab`은 버튼과 주소 입력 사이를 순서대로 돈다. Ctrl·Alt·Cmd를 함께 누른 `Tab`은 돌지 않는다. 띠의 위젯은 `Tab`과 방향키를 focus lock filter로 잡아 egui 기본 순회가 띠 밖 host 위젯으로 포커스를 옮기지 않게 하고, 다음 대상은 띠가 고른다. 포커스를 받은 버튼은 `Enter`·`Space`로 눌리고 바깥에 포커스 테두리를 그린다. 테두리는 새 디자인 값 없이 이미 승인된 토큰 `border_focus`·`focus_ring_width`를 쓰며, 플러그인 추가 화면의 포커스 테두리와 egui 선택 테두리가 같은 쌍을 쓴다. 주소 입력에서 Tab으로 떠나면 다른 곳으로 포커스를 옮길 때처럼 거절되지 않은 초안은 지금 주소로 돌아가고 편집 표시가 꺼진다. 주소 입력으로 돌아오면 주소 전체를 고른다. 미러의 읽기 전용 주소는 고르지 않고 편집 상태도 되지 않는다. `Esc`와 페이지의 포커스 획득은 버튼 포커스도 놓는다. 포커스를 가진 버튼이 그 동작으로 사라지거나 비활성이 되면(새로고침이 Spinner로 바뀌거나 뒤로가 히스토리 처음에 닿을 때) 포커스가 풀리고 게이트가 닫힌다.
- **주소창 포커스 단축키** — 설정 키 `explorer_focus_address`(라벨 "주소창으로 이동", 기본 `Alt+L`, macOS `⌘L`, 단축키 설정의 Surface 탭)를 탐색기와 함께 쓴다. 포커스된 surface가 html이면 그 surface의 띠에 포커스를 주고 주소 전체를 고른다. 다른 surface의 띠는 바꾸지 않는다. 주소 띠가 있는 html 페이지(mirror 제외)가 키보드 포커스를 가진 상태에서도 host가 이 키를 가져간다([WebView 단축키](../../design/systems/webview.md)). markdown 페이지는 주소 입력이 host 띠가 아니라 문서 DOM 안에 있으므로 가져가지 않고 페이지로 보내며, 페이지가 같은 바인딩으로 자기 주소 입력을 고른다([markdown 주소 띠](../markdown/index.md)). 띠가 이미 포커스를 가진 동안 이 키를 다시 누르면 주소 전체를 다시 고른다. 키를 바꾸면 새 키만 가져가고 이전 키는 페이지로 간다. 뒤로·앞으로에는 새 단축키를 두지 않는다. 페이지가 처리하는 `Alt+←`·`Alt+→`와 겹치지 않게 하기 위해서다.
- **주소 입력의 편집 키** — 띠가 포커스를 가진 동안 단축키 설정의 `copy`·`cut`·`paste`·`select_all`·`page_undo`·`page_redo` 바인딩은 주소 입력에만 적용한다. 바인딩한 키는 그 키의 egui 기본 효과(글자 입력, 기본 복사·잘라내기·붙여넣기, 삭제·이동) 대신 바인딩한 편집 하나만 한 번 실행한다. 예를 들어 `paste`에 `Ctrl+X`를 주면 잘라내지 않고 붙여넣기만 한다. 어느 편집에도 바인딩하지 않은 egui 기본 편집 키(Ctrl/⌘+C·X·V·A 등)는 그대로 동작하므로 바인딩을 지워도 남는다. 실행 취소·다시 실행 바인딩은 egui 입력의 실행 취소·다시 실행 하나로 바뀐다. 단어 이동·삭제, Shift 확장 선택은 egui 입력의 기본 키다. 페이지 본문·터미널·Explorer로 키가 새지 않는다. 읽기 전용 mirror 주소도 눌러 포커스를 준 뒤 전체 선택·복사로 말줄임 라벨이 아닌 원문 전체를 복사하며, 잘라내기·붙여넣기·Enter는 값을 바꾸거나 이동하지 않는다. 페이지 본문의 편집 키는 [WebView 편집 액션](../../design/systems/webview.md#편집-액션)을 따른다.
- **겹친 바인딩** — 띠가 포커스를 가진 동안 한 키가 편집·주소창 포커스·공통 새로고침 중 여럿에 묶여 있으면 편집, 주소창 포커스, 새로고침 순으로 하나만 실행한다. 예를 들어 `copy`와 새로고침에 같은 `F5`를 주면 복사만 하고 새로고침하지 않는다. 다른 바인딩과 겹치지 않은 새로고침 키(기본 `F5`)는 그대로 그 surface를 새로고침한다.
- **편집 중 에이전트 변경** — 사용자가 편집하는 동안 에이전트가 `tasty html open`으로 주소를 바꾸면 입력 텍스트와 포커스는 그대로이고 표지도 없다. Esc는 그때의 commit 주소로 되돌린다. 에이전트 경로는 띠에 포커스를 주지 않는다.
- **Esc** — 편집을 취소하고 기다리던 파일 확인을 버린 뒤 commit 주소로 돌아간다. 실패한 시도 주소로 되돌리지 않는다.
- **원격 콘텐츠 표지** — 원격 안내가 화면에 없을 때(닫았거나, 정적 참조 없이 OS가 차단을 알렸거나, 허용했을 때) 주소 입력란 끝에 표지를 둔다. mirror의 건너뛴 파일 Tag가 있으면 그 뒤다. 표지 칸(`html-remote-marker-hit`, 20)은 주소 말줄임 예산에 들어가 주소가 먼저 줄고 표지는 줄지 않는다. 차단 표지는 globeOff(`html-remote-marker-fg`)이고 hover 채움(`html-remote-marker-hover-bg`)이 있으며, 누르면 원격 안내를 다시 보일 뿐 허용하지 않는다. 그 누름은 주소 입력 편집을 시작하지 않는다. 허용 표지는 globe(`html-remote-marker-allowed-fg`)이고 툴팁만 있다. 툴팁은 위쪽에 뜨고 WebView 영역을 피한다(`placement_clear_of_native`). 안내가 페이드로 사라지는 동안과 재로드 중에는 표지를 그리지 않는다. mirror·HTTP view에는 원격 안내와 표지가 없다.
- **mirror** — 협상된 entry mirror에는 원격 경로의 읽기 전용 입력란과 새로고침만 그린다. 미지원 placeholder에는 띠를 숨긴다.
- **같은 문서 판정** — 띠로 연 주소의 같은 문서 `#fragment` 판정도 위 "사용자 탐색"과 같은 문자열 비교다. 입력 해석은 파일 시스템 정규화를 하지 않으므로 표기가 다른 같은 파일은 새 로드가 된다.
- **색** — 편집 중이 아닌 실제 주소는 `html-address-value-fg`(text-secondary-raised), 편집 중에는 `input-fg`, mirror의 읽기 전용 주소는 `input-readonly-fg`다. placeholder와 비활성은 기존 역할을 쓴다. 기하는 `html-address-*`·입력·배너 토큰이다.

## 스크립트 감지와 허용 규칙

 이 절은 현재 규칙과 OS별 구현·측정이다.

### 감지 대상

로컬 문서에서 다음 셋 중 하나라도 있으면 실행 가능한 스크립트가 있다고 판단한다.

- `<script>` 요소. inline이든 `src`든 같다. `<svg>` 안의 inline `<script>`도 포함한다. 단 `type`이 JavaScript MIME 형식이나 `module`이 아니면 제외한다. `application/json`, `text/template` 같은 데이터 블록이 이 예외에 해당한다.
- 요소의 `on*` 이벤트 핸들러 속성.
- `javascript:` 스킴 URL. `href`, `src`, `action`, `formaction` 속성의 값을 검사한다.

스크립트 배너는 지금 실행될 수 있는 스크립트가 있을 때만 뜬다. 감지 결과가 `http(s)` `src` 스크립트뿐(`ScriptsRemoteOnly`)이면 그 view에서 원격이 열려 있을 때만 배너를 띄운다. 막혀 있으면 허용해도 실행되지 않기 때문이다. 로컬 문서는 전역 설정이나 이 문서의 원격 허용이 있을 때, entry mirror는 늘 막으므로 띄우지 않고, HTTP view는 같은 origin 스크립트를 endpoint가 허용하므로 원래대로 띄운다. 감지 결과(`ScriptDetection`)는 `<base href>`를 보지 않는다. 원격 base 아래의 상대 `src` 스크립트는 로컬 스크립트로 세므로 배너가 뜨지만, 원격을 막는 동안에는 허용해도 그 스크립트는 받지 못한다. base를 반영한 원격 스크립트 여부는 따로 둔다([원격 콘텐츠 차단](#원격-콘텐츠-차단)). 원격 스크립트가 있고 원격 안내도 보이면 스크립트 배너 본문을 원격 안내 문구(`bodyRemote`)로 바꾼다.

### 스캔

스캔하는 크기에는 상한을 둔다. 상한 값은 구현 문서에서 정한다. 상한은 감지 스캔에만 적용하며, 아래 내용 지문은 상한과 관계없이 파일 전체를 덮는다.
상한을 넘은 부분에만 스크립트가 있으면 감지하지 못한다. 이 경우 JS가 꺼진 채 배너가 뜨지 않는다. 즉 실패해도 JS가 켜지지는 않는다.
정규 파일만 스캔한다. FIFO·장치 파일은 거절하므로 지문이 없고, JS가 꺼진 채 허용할 수 없다.
스캔은 UI 스레드 밖에서 한다. 응답 단계는 그 로드의 스캔 표(ticket)를 받고, 파일 읽기와 전체 해시는 작업 스레드가 한다.

- 허용 기록과 URL이 다른 응답은 스캔 결과와 관계없이 JS 값이 같다(sandbox가 켜져 있으면 끔, 꺼져 있으면 켬). 그래서 결정은 바로 하고 스캔은 작업 스레드(`html-script-scan`)가 끝낸다. 결과가 붙는 범위와 버리는 경우는 아래 "표의 범위"에 있다.
- 허용 기록과 같은 URL의 응답(허용 직후 재로드, 허용한 문서의 재로드)은 지문이 JS 값을 정한다. Linux는 응답 결정을 보류하고 작업 스레드에서 파일만 읽는다. 상태 반영과 표 확인은 결정을 넘기는 GTK 작업 안에서 그 직전에 한다. 읽는 동안이나 읽기가 끝난 뒤 적용 전에 새 로드가 시작됐거나 다른 문서가 commit됐으면 표가 낡아 보류한 응답을 거절한다. macOS는 정책 결정 handler와 preferences를 붙잡아 두고 반환한다. Windows는 탐색을 취소한다. 두 플랫폼 모두 작업 스레드가 파일만 읽어 결과 칸에 넣고 그 윈도우를 깨운다. 그다음 WebView 동기화에서 UI 스레드가 표를 확인한 뒤 결정을 끝낸다. 표가 낡았으면 macOS는 Cancel로 끝내고 Windows는 다시 시작하지 않는다. 자세한 절차는 [OS별 적용](#os별-적용)에 있다.
- 작업 스레드를 띄우지 못하면 UI 스레드에서 다시 읽지 않는다. 그 로드는 읽기 실패와 같이 지문 없는 문서가 된다(허용 불가). 이미 정한 JS 값은 바꾸지 않는다.
- 스캔이 끝나기 전에 commit된 문서는 지문이 없는 상태로 기록되고 결과가 오면 채워진다. 이 동안 허용을 받지 않고(`AllowError::Loading`) 배너는 로드 중 단계로 허용 버튼을 비활성으로 그린다. 상태 조회의 `scanning`이 참이다.
- 같은 URL의 재로드는 결과가 오기 전까지 이전 문서의 감지 결과와 배너 표지(닫힘 등)를 잠정 유지한다. 결과의 지문이 이전 문서와 같으면 그대로 두고, 다르면 새 문서로 보고 표지를 새로 시작한다.
- 표의 범위: 한 로드의 결과는 두 경우에만 붙는다. 그 로드가 아직 commit 전이면 그 로드의 응답에 붙고, 그 로드가 commit한 문서가 아직 화면 문서이면 그 문서에 붙는다.
  - commit 전 대기는 새 로드가 시작되거나 그 로드가 실패하면 버린다.
  - commit 뒤 대기는 다른 문서가 commit되거나 그 로드가 commit 뒤 실패하면 버린다. 실패한 문서는 지문 없이 남아 허용할 수 없다.
  - commit 뒤 대기는 새 로드가 시작되는 것만으로는 버리지 않는다. 새 로드가 commit하기 전까지 화면에는 그 문서가 남기 때문이다. 이때 늦게 온 결과는 화면 문서에만 붙고 새 로드에는 붙지 않는다. 새 로드가 commit하기 전에 실패해도 그 대기는 유지된다.
- 결과를 기다리며 결정을 미룬 동안의 화면은 다른 로드와 같은 규칙을 따른다. 호스트는 chrome 상태가 `Done`일 때만 native 페이지를 보이고 그 밖에는 숨긴다(`webview_sync`). Linux는 응답 단계 전에 로드 시작 신호로 `Loading`이 되므로 확인 동안 native 페이지가 숨는다. Windows는 `NavigationStarting`에서 `Loading`이 되고 내부 취소의 완료 신호를 무시하므로, 다시 시작한 탐색이 끝날 때까지 숨는다. macOS의 정책 결정은 provisional 시작 전이다. 호스트의 `load_url`은 미리 `Loading`을 세우므로 숨는다. 재로드·뒤로·앞으로와 페이지가 시작한 탐색은 `Done`이 유지되어 확인 동안 이전 페이지가 보인다(소스 대조, 실 기기 미측정).
- 작업 스레드가 끝나면 그 윈도우를 다시 그리도록 깨운다.

FIFO는 여는 순간 쓰는 쪽을 기다리며 막혔고 `/dev/zero`는 읽기가 끝나지 않았다(측정). 그래서 막히지 않게 연 핸들로 종류를 먼저 확인한다.

### 허용의 수명

허용은 surface, main frame 문서의 URL, 내용 지문에 묶인다. URL은 fragment를 뺀 값이다. 내용 지문은 파일 전체를 스트리밍 해시로 구한다.
지문을 스캔 상한까지만 구하면 상한 뒤에 덧붙인 스크립트가 허용된 채 실행된다(리뷰 측정). 그래서 지문 범위를 스캔 범위와 분리한다.

지문은 로드할 때 구한다.

- html surface에서 main frame `file://` 문서를 로드할 때마다 감지 스캔과 같은 읽기로 파일 전체 지문을 구한다. 허용 여부와 관계없이 모든 로드에서 구한다. 읽기는 그 로드의 응답 단계에서 시작한 작업 스레드가 한다([스캔](#스캔)).
- WebView가 읽은 바이트와 스캔이 읽은 바이트는 같은 읽기가 아니다. 두 읽기 사이에 파일이 바뀌면 화면 내용과 지문이 다를 수 있다. 스캔을 작업 스레드로 옮겨 이 간격이 길어질 수 있지만, 허용은 여전히 그 로드에서 읽은 지문에 묶이고 클릭 때 다시 읽지 않는다. 외부 교체를 원자적으로 막는 장치는 두지 않는다.
- 구한 지문과 감지 결과는 그 문서가 commit될 때 surface의 "현재 문서" 기록이 된다. 기록 항목은 URL, 지문, 감지 결과다.
- 허용 클릭은 그 시점의 현재 문서 `{URL, 지문}`을 허용으로 기록한다. 클릭할 때 파일을 다시 읽지 않는다.
- 이유: 허용 대상은 사용자가 보고 배너 감지 결과가 나온 그 내용이어야 한다. 클릭할 때 지문을 구하면, 사용자가 본 뒤 파일을 바꿔 둔 내용이 경합 없이 실행된다(리뷰 측정).
- 로드가 시작된 뒤 main frame commit 전에는 허용을 받지 않는다(`AllowError::Loading`). 배너는 이 동안 허용 버튼을 비활성으로 그린다.
  - 이유: 이 사이에 누른 허용은 이전 문서를 기록한다. 재로드 도중 파일 내용이 바뀌었다면 commit 때 지문이 달라 허용이 풀리고, 사용자는 "허용했는데 다시 막혔다"를 겪는다. commit 뒤 재평가로 허용을 옮기는 안은 사용자가 보지 않은 내용을 허용하게 되어 택하지 않았다.
- 화면 문서는 마지막 main frame commit에서 호스트가 기록한 현재 문서다. webview의 현재 URI 조회로 판정하지 않는다.

허용의 수명은 다음 규칙을 따른다. 사용자에게 보이는 문안("until Tasty restarts", 갤러리의 "cleared by: navigation to another document · app restart")과 같은 규칙이다.

- **같은 문서의 재로드**: 허용을 유지한다. 사용자의 재로드(F5 등)와 허용 직후 호스트가 하는 재로드를 구분하지 않는다.
- **다른 문서로 이동**: 허용을 푼다. 그 뒤 같은 URL로 돌아와도 다시 허용해야 한다. 뒤로 가기와 bfcache 복원도 여기에 포함된다.
- **내용이 바뀐 뒤의 재로드**: 다른 문서로 본다. 다시 스캔한 결과나 내용 지문이 허용 당시와 다르면 허용을 푼다.
- **허용 기록의 갱신 시점**: 허용 기록은 다른 main frame 문서가 commit될 때 해제한다.
  - 새 문서의 JS를 켤지 끌지는 commit 전의 응답 단계에서 정한다. 이 판단은 허용 기록을 바꾸지 않는다.
  - 응답 단계에서 허용과 일치한 문서가 commit되면 허용을 유지한다. 그 밖의 문서가 commit되면 허용을 해제한다. 응답 단계 없이 commit되는 bfcache 복원도 해제에 포함된다.
  - 응답 단계의 결과는 로드마다 새로 둔다. 로드가 시작될 때 이전 로드의 결과를 비우고, commit에서 소비한다. 비우지 않으면 앞선 재로드의 "일치"가 뒤이은 bfcache commit에 남아 허용이 유지된다(page cache를 켠 상태의 리뷰 측정).
  - 응답 단계 없이 commit된 문서는 지문이 없다. 이 문서는 허용할 수 없다. Linux는 아래처럼 page cache를 꺼서 이 경우를 없앤다.
- **문서가 바뀌지 않고 끝나는 로드**: 로드 실패, 중단, 다운로드, web process 종료처럼 main frame 문서가 commit되지 않고 로드가 끝나는 경우다.
  - 화면에 남은 문서가 허용된 문서이면 JS를 허용 상태로 되돌린다. 멈춘 채 두지 않는다.
  - 허용 기록은 그대로 둔다.
- **세션 한정**: 허용은 세션 안에서만 유지한다. 레이아웃에 저장하지 않으므로 재시작하면 다시 차단 상태가 된다.

허용 판단은 **main frame의 문서가 바뀔 때에만** 적용한다. 다음 경우에는 허용을 바꾸지 않는다.

- 같은 문서 안의 서브프레임(`<iframe>`, `<object>`, `<embed>`) navigation.
- 새 창 요청(`target=_blank` 등). 현재 문서를 바꾸지 않는다.
- fragment만 바뀌는 이동.

문서가 바뀌면 새 문서의 스크립트가 실행되기 전에 JS를 끈다. redraw에서 사후에 되돌리지 않는다.

- 설정 경로(`resolve_webview_settings`)는 전역 "Sandbox scripts" 값만 backend에 넘긴다. 문서 단위 JS는 backend의 navigation 콜백이 surface의 허용 상태로 정해 그 자리에서 적용한다.
- 이유: redraw에서 문서 단위 값을 사후에 적용하면 그 사이 시작된 다른 문서의 로드에 이전 문서의 허용이 샌다. 콜백은 로드 단계와 같은 순서로 불리므로 이 경합이 없다.
- 전역 값이 바뀌면 backend는 그 값을 허용 상태에 반영하고, 진행 중인 로드가 있으면 그 로드의 응답 단계 판단을, 없으면 화면 문서의 허용 여부를 적용한다.

### OS별 적용

OS별 구현은 다음과 같다.

- **Linux(측정함, WebKitGTK 2.50.4)**: `NavigationAction`에는 main frame과 서브프레임을 구분할 정보가 없다. iframe 문서의 navigation도 `get_frame_name()`이 None이고 type이 `other`로 같았다. 그래서 navigation action에서는 판단하지 않고, 다음 두 신호를 쓴다.
  - `load-changed` `STARTED`: main frame 로드에서만 온다(측정 결과 iframe 로드에서는 오지 않았다). 여기서 JS를 무조건 끈다.
    - 끈 설정은 화면에 남아 있는 현재 문서에도 적용된다. 허용된 문서에서 응답이 오지 않는 로드(FIFO)를 시작하자 1초 간격 timer가 5.3초 동안 멈췄고, 중단 뒤 복원하자 다시 돌았다(측정). 끈 직후 같은 이벤트 처리 턴에서 이미 예약된 timer가 한 번 실행된 경우는 있었다(측정 1회).
    - 이 시점의 `get_uri()`는 링크 이동에서 이전 문서 URI를 돌려준다(측정). 그래서 URL을 판정에 쓰지 않는다.
    - 응답 단계의 결과(대기 값)를 여기서 비운다.
  - `decide-policy`의 `RESPONSE` 결정: `ResponsePolicyDecision::is_main_frame_main_resource()`(2.40부터)가 참일 때만 동작한다.
    - `file://`이면 그 로드의 스캔을 시작한다. 허용 기록과 URL이 다르면 JS를 바로 정하고 결정을 넘기며, 스캔은 작업 스레드가 끝낸다. 같으면 결정 객체를 잡아 두고 신호 핸들러에서 `true`를 돌려준 뒤 `gio::spawn_blocking`으로 읽고, GTK 스레드의 `spawn_local`에서 JS를 정한 뒤 `use_()`한다. 결과가 낡았으면 `ignore()`한다. 결과(URL, 지문, 감지 결과, 일치 여부)는 대기 값으로 둔다.
    - 측정에서 iframe 응답은 거짓이었다.
    - 여기서 켜고 끈 설정은 그 문서의 첫 스크립트부터 적용됐다.
    - 이 단계에서는 JS만 정한다. 허용 기록은 `load-changed` `COMMITTED`에서 갱신한다.
    - page cache를 끈 상태에서는 뒤로·앞으로 가기를 포함한 모든 main frame 로드의 순서가 `STARTED` → `RESPONSE` → `COMMITTED`였다(측정). 리뷰 측정의 일반 로드와 재로드도 같은 순서였다.
  - `NEW_WINDOW_ACTION`은 별도 결정 type으로 오며 허용에 영향을 주지 않는다.
  - fragment 이동에는 `load-changed`가 오지 않는다. JS 상태가 유지됐다.
  - page cache가 켜져 있으면 bfcache 복원에 `RESPONSE` 결정이 오지 않는다. `STARTED`에서 끈 상태가 유지되어, 복원된 문서의 timer가 멈춘 채로 남았다.
    - page cache가 켜진 상태에서 `STARTED`에서 끄지 않으면, 허용된 문서에서 뒤로 가기로 복원한 허용되지 않은 문서의 timer가 다시 실행됐다(측정).
    - 이렇게 응답 단계 없이 commit된 문서는 지문이 없다. 그래서 아래처럼 page cache를 끈다.
  - html surface는 page cache(`enable-page-cache`)를 끈다.
    - 끄면 뒤로·앞으로 가기도 `STARTED` → main `RESPONSE` → `COMMITTED` 순서의 새 로드가 됐다(측정). 모든 main frame commit이 지문을 가진다.
    - `STARTED`에서 끄는 규칙은 그대로 둔다.
  - `COMMITTED`에서 대기 값을 소비해 현재 문서를 기록한다.
  - Tasty는 `load-failed` 핸들러가 `true`를 돌려 WebKit 기본 오류 페이지를 띄우지 않는다(`src/host_api/webview/linux.rs`의 `connect_load_failed`).
    - 이 설정에서 main frame 로드가 실패하면 문서가 바뀌지 않는다. 대상은 없는 파일, 그리고 "Frame load interrupted"로 끝나는 다운로드 응답이다.
    - 두 경우 모두 `STARTED`(끔) 뒤 `COMMITTED` 없이 `load-failed`와 `FINISHED`가 왔다. 화면에는 이전 문서가 남았다. 복원하지 않으면 화면의 허용 문서가 JS 꺼진 채 멈췄다(리뷰 측정).
  - 복원은 `load-changed` `FINISHED`에서 한다. 직전 `STARTED` 뒤 `COMMITTED`가 없었으면, 화면 문서의 허용 여부대로 JS를 되돌린다.
    - 화면 문서는 마지막 `COMMITTED`에서 호스트가 기록한 현재 문서다. `FINISHED` 시점이나 `stop_loading` 뒤의 `get_uri()`는 쓰지 않는다.
      - 취소된 로드의 `FINISHED` 시점 `get_uri()`는 아직 시작하지 않은 다음 URL을 돌려줬다(리뷰 측정).
      - `load_uri` 직후 `stop_loading`하면 이벤트가 없는데도 `get_uri()`가 새 URL로 남았다. 화면은 이전 문서였다(리뷰 측정).
    - `load-failed`는 기록만 한다. 측정한 두 경우 모두 `load-failed` 뒤에 `FINISHED`가 왔다. `FINISHED`에서 처리하면 commit 없이 끝나는 다른 경로도 함께 다룬다. 예외는 web process 종료다.
    - commit 전에 web process가 종료되면 `web-process-terminated`만 오고 `load-failed`와 `FINISHED`는 오지 않았다. 느린 http 응답을 기다리는 로드에서 web process를 강제 종료해 측정했고, 3분 뒤에도 신호가 없었다. 복원하지 않으면 로드 중 상태가 남아 배너가 허용 버튼을 비활성으로 두고 허용 요청이 `AllowError::Loading`으로 거절됐다(측정).
      - 그래서 `web-process-terminated`에서도 `FINISHED`와 같은 방식으로 로드를 끝내고 JS를 되돌린다. 로드가 이미 끝났으면 아무것도 바꾸지 않으므로, 뒤에 `FINISHED`가 와도 결과가 같다.
      - Windows·macOS 백엔드도 같은 방식으로 로드를 끝낸다. 구현은 했지만 실 기기에서 측정하지 않았다. 같은 증상이 나는지와 종료 뒤 어떤 navigation 신호가 오는지는 모른다.
        - macOS는 `webViewWebContentProcessDidTerminate:`에서 로드를 끝낸다.
        - Windows는 `ProcessFailed` 중 main frame 문서를 그리던 process가 끝난 종류(`RENDER_PROCESS_EXITED`·`BROWSER_PROCESS_EXITED`)에서만 끝낸다. iframe·GPU·utility process 종료와 `RENDER_PROCESS_UNRESPONSIVE`는 main frame 로드를 끝내지 않으므로 건너뛴다.
    - 측정 결과 허용 문서의 timer가 다시 돌았다(tick 14→19, JS True).
  - `stop_loading`이나 기존 정책이 무시하는 `http(s)` navigation에는 `STARTED`가 오지 않았다. 따라서 JS와 문서가 그대로다(리뷰 측정).
- **Windows(Tasty 실행은 미측정, API 문서와 하네스 측정 근거)**: `NavigationStarting`은 main frame navigation에서만 발생한다. 서브프레임은 `FrameNavigationStarting`, 새 창은 `NewWindowRequested`로 따로 온다. 이 핸들러 안에서 `IsScriptEnabled`를 정한다. 이 이벤트에는 deferral이 없다. 허용 기록과 URL이 다른 탐색은 JS를 바로 정하고 작업 스레드에서 읽는다. 같은 탐색은 다음 순서로 처리한다(`scan_resume`).
  - `SetCancel(true)`로 취소하고 그 `NavigationId`를 내부 취소로 기록한다. WebView2는 이전 문서를 유지하지만 chrome이 `Loading`이라 호스트는 native 페이지를 숨긴다. 그 탐색의 `NavigationCompleted`(취소 완료)는 로드 종료로 보지 않아 게이트 대기와 chrome의 Loading을 그대로 둔다.
  - 작업 스레드가 읽기를 끝내면 UI 스레드가 표를 확인한다. 아직 현재 로드면 원래 종류로 다시 시작한다. 재로드는 `Reload`, 새 문서는 `Navigate`, 뒤로·앞으로는 호스트가 요청한 방향의 `GoBack`·`GoForward`다. 그래서 재로드가 히스토리 항목을 늘리지 않는다.
  - 다시 시작한 탐색의 `NavigationStarting`은 파일을 다시 읽지 않고 읽어 둔 결과로 JS를 정한다. 이 결과는 한 번만 쓴다. 결과를 받은 탐색의 실제 문서 요청에서 지문을 다시 확인한다(아래). 탐색 시도 기록(사용자 제스처 판정)은 취소한 원래 탐색 것만 남긴다.
  - 다시 시작한 탐색을 확정할 식별자는 없다. `Navigate`·`Reload`·`GoBack`·`GoForward`는 `NavigationId`를 돌려주지 않는다. 다시 시작한 탐색의 `IsUserInitiated`도 일정하지 않다. Windows 실 기기에서 허용 뒤 다시 시작한 재로드가 거짓으로 온 회차가 있었고, 그 회차마다 재시작을 알아보지 못해 상한에 걸려 허용한 문서가 스크립트 없이 열렸다. 그래서 `IsUserInitiated`는 조건에서 뺐고 진단 로그에만 남긴다. 재시작을 기다리는 동안 온 `NavigationStarting`은 다음이 모두 맞을 때만 그 재시작으로 본다. 주소, 다시 시작한 종류(`Reload`→`RELOAD`, `Navigate`→`NEW_DOCUMENT`, 뒤로·앞으로→`BACK_OR_FORWARD`), 이 모듈이 취소한 탐색의 `NavigationId`가 아닐 것, 그리고 아래 파일 판이다. 하나라도 다르면 새 탐색이다. 읽어 둔 결과를 쓰지 않고 다시 취소·스캔하며, 기다리던 재시작은 버린다. 같은 주소라도 종류가 다른 탐색은 결과를 받지 못한다. 같은 주소·종류의 페이지 스크립트 탐색은 재시작과 구별하지 못하므로, 결과가 다른 내용에 붙지 않는 것은 파일 판과 아래 문서 요청 지문 확인이 맡는다. 이 판정은 사용자 입력의 증거나 포커스 근거로 쓰지 않는다. 탐색 시도 기록은 취소한 원래 탐색의 것만 남는다. 시험은 `a_replay_reported_as_not_user_initiated_is_still_recognised`와 `a_start_with_the_cancelled_id_is_not_the_replay`다.
  - 파일 판: 주소·종류가 같은 다른 탐색(재시작 호출과 그 시작 신호 사이에 같은 주소를 같은 종류로 연 경우)은 위 API 제약 때문에 재시작과 구별하지 못한다. 그래서 작업 스레드는 스캔할 때 같은 핸들로 읽기 전후의 파일 식별값(크기·수정 시각·생성 시각과 파일 식별자: unix는 장치·inode, Windows는 볼륨 일련번호·파일 인덱스)을 기록한다. 시작 신호를 받으면 그 경로의 식별값을 다시 구해(unix는 메타데이터 조회, Windows는 내용을 읽지 않고 열어 같은 핸들로 조회) 같을 때만 결과를 준다(`FileStamp`). 파일을 제자리에서 고치거나 이름 바꾸기로 교체했거나 경로가 사라졌으면, 또는 읽는 동안 식별값이 바뀌었으면 결과를 넘기지 않는다. 그 탐색은 새 탐색으로 다시 취소되어 자기 스캔을 받는다. 시험은 `a_file_rewritten_before_the_replay_starts_is_scanned_again`(같은 길이 덮어쓰기)·`a_file_replaced_by_rename_is_another_version`·`a_missing_file_gets_no_replay_scan`·`an_identical_navigation_gets_the_scan_only_for_the_same_file_version`이다.
  - 파일 판 식별의 한계: 식별값이 같다고 내용이 같다는 보장은 없다. Windows의 파일 시각은 갱신 단위가 거칠어 같은 단위 안에 쓴 같은 길이의 교체는 크기·생성·수정 시각이 모두 같고, `ReplaceFile`식 교체는 옛 생성 시각을 물려받는다. 그래서 교체 판별은 파일 인덱스에 기댄다. 파일 시스템이 인덱스를 주지 않으면 크기·시각만 비교한다. 쓰기 핸들이 닫히기 전에는 수정 시각이 갱신되지 않을 수 있으며 FAT의 수정 시각은 2초 단위다([File Times](https://learn.microsoft.com/en-us/windows/win32/sysinfo/file-times)). 그래서 같은 길이로 바꾼 내용이 같은 식별값을 가질 수 있고, 재시작과 구별하지 못한 같은 조건의 탐색이 결과를 받을 수 있다. 숨김과 `host_request`도 이미 전달 중인 탐색을 배제한다는 API 보장이 없다. 그래서 결과를 받은 탐색의 본문을 아래와 같이 확인한다.
  - 문서 요청 확인: 결과를 받은 탐색이 시작되면 확인 창을 연다. 그 탐색이 commit되거나 끝나거나 더 새 탐색이 시작될 때까지, 같은 파일의 `WebResourceRequested`(DOCUMENT 문맥) 요청을 deferral로 붙잡는다([add_WebResourceRequested](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2?view=webview2-1.0.3405.78)는 file 요청에도 발생하고, deferral을 완료할 때까지 요청을 막는다). 작업 스레드가 그 파일의 전체 지문을 다시 구하고, UI 스레드가 결과의 지문과 비교한다.
    - 같으면 응답을 지정하지 않고 `Complete`해 WebView가 원래대로 파일을 읽게 한다.
    - 다르면 본문을 넘기기 전에 `Stop`으로 탐색을 멈춘다. 그 탐색은 내부 취소로 기록하고, 그 `NavigationCompleted`가 온 뒤에 요청을 `Complete`한다. 이어서 원래 종류로 다시 시작한다. 다시 시작한 탐색은 새 탐색으로 취소되어 자기 스캔을 받고, 바뀐 내용은 허용을 이어받지 않는다. 지문이 연달아 세 번 다르면 다음 로드를 스캔 없이 지문 없는 문서로 둬 스크립트 없이 진행한다.
    - 요청을 붙잡지 못했거나(`GetDeferral` 실패), 확인 창의 탐색이 문서 요청 없이 commit했으면 확인하지 못한 본문이다. 이때도 탐색을 멈추고 재로드한다.
    - `Stop`은 숨겨진 이전 페이지가 기다리던 자원 요청도 멈춘다. 이전 페이지는 곧 바뀌는 숨겨진 문서이므로 이 범위를 받아들였다.
    - 결과를 어느 탐색이 받았든 그 탐색이 실제로 받는 본문의 지문이 결과와 같으므로, 탐색 식별자 없이 결과가 다른 내용에 붙지 않는다. 같은 파일을 요청하는 iframe도 같은 기준으로 확인한다. 같은 지문으로 `Complete`한 뒤 WebView가 실제로 읽기 전에 파일이 바뀌는 간격은 기존의 스캔·WebView 읽기 비원자성과 같다.
    - 관측(Windows 11 25H2, WebView2 런타임 155.0.4283.62, 격리 user data 폴더, 서로 독립인 하네스 두 개, 시나리오마다 1회):
      - 문서 요청을 붙잡은 채 `Stop`하면 그 `NavigationId`의 `NavigationCompleted`(실패, `WebErrorStatus` 9)가 1ms 안에, `Complete`보다 먼저 왔다.
      - 그 뒤 `Complete`해도 그 문서의 재요청·`ContentLoading`·스크립트 실행이 없었고 이전 문서가 남았다. `Stop`과 `Complete`를 같은 tick에 호출해도, `Complete`하지 않고 deferral을 놓거나 늦게 `Complete`해도 같았다.
      - `Stop` 없이 붙잡기만 하면 6초 동안 완료 신호가 없었다. 그래서 지문이 다르면 반드시 `Stop`한다.
      - `Stop` 뒤 같은 종류(`Navigate`·`Reload`·`GoBack`)로 다시 시작하면 같은 목표를 정상으로 로드했고, 바뀐 내용을 새로 읽었다.
    - API 보장 범위: 공식 문서가 보장하는 것은 세 가지다. deferral을 완료할 때까지 요청이 막힌다. `Stop`은 모든 탐색과 대기 중 자원 요청을 멈춘다. `NavigationCompleted`는 로드가 오류로 멈췄을 때도 온다. `Stop`의 완료 신호가 `Complete`보다 먼저 온다는 순서는 관측일 뿐이다. 그래서 구현은 이 순서에 의존하지 않고, 멈춘 탐색의 `NavigationCompleted`를 받은 뒤에만 `Complete`한다. `Stop` 호출이 실패하면 완료 신호를 기다리지 않는다. 재시작을 버리고 이 로드를 실패(Failed)로 끝내 Loading에 남지 않게 한다. 본문을 막고 있는지는 경우에 따라 다르다.
      - 지문이 달라 멈춘 경우: 문서 요청을 deferral로 붙잡고 있으므로 본문은 막힌 채 남는다. 그 요청은 그 탐색이 끝나거나 다음 탐색이 commit할 때 놓아 준다.
      - `GetDeferral`이 실패했거나 요청 없이 이미 commit한 경우: 붙잡은 본문이 없다. `Stop`까지 실패하면 확인하지 못한 문서가 그려질 수 있다. 이때는 로드를 실패로 끝내고 error 로그를 남긴다. 두 API가 함께 실패하는 경우에 대비한 별도 차단(본문 대체 등)은 두지 않았다.
    - 늦은 확인 결과: 확인하는 동안 그 탐색이 끝나거나 다른 문서가 commit하면, 아직 읽는 요청에 종료 표시를 남긴다. 읽기가 늦게 끝나면 결과와 관계없이 바로 놓아 준다. 그 탐색은 이미 끝나 본문이 문서에 닿지 않는다. 그래서 다음 탐색이 없어도 deferral이 남지 않는다. 겹친 main·iframe 요청, 이전 페이지에 대기 중인 하위 요청이 있을 때의 `Stop` 영향, 다른 런타임 버전은 측정하지 않았다. Tasty 자체를 Windows에서 실행한 측정도 하지 않았다.
    - 시험: `the_document_request_of_a_resumed_navigation_proceeds_when_its_fingerprint_matches`, `a_same_stamp_rewrite_is_caught_at_the_document_request`(식별값이 그대로인 같은 길이 쓰기), `an_unchecked_resumed_document_is_stopped_and_reloaded`, `a_file_that_keeps_changing_loads_without_scripts_after_the_limit`, `a_failed_stop_drops_the_restart_and_keeps_the_request_until_another_commit`, `a_stopped_history_navigation_restarts_in_the_same_direction_and_is_scanned_again`(뒤로·앞으로 재시작의 방향 보존), `a_late_check_result_of_an_ended_navigation_is_released_at_once`.
  - 다시 시작한 탐색의 실제 종류가 예상과 다르면 매번 새 탐색으로 보여 취소가 이어진다. 그래서 알아보지 못한 재시작이 두 번 이어지면 더 취소하지 않고 지문 없는 문서로 스크립트 없이 진행한다. 알아본 재시작, 스캔이 필요 없는 탐색, 호스트 요청이 이 횟수를 되돌린다. `Navigate`로 같은 주소를 다시 열 때 런타임이 어떤 종류를 보고하는지는 측정하지 않았다.
  - 호스트의 로드·재로드·뒤로·앞으로 요청은 기다리던 스캔과 재시작을 먼저 버린다.
  - 페이지가 시작한 뒤로·앞으로는 방향을 알 수 없다. 종류를 읽지 못한 탐색(구형 런타임, `ICoreWebView2NavigationStartingEventArgs3` 없음)은 재시작을 종류로 알아볼 수 없다. `NavigationId`를 읽지 못한 탐색은 취소를 구별할 수 없다. 이 경우들은 취소하지 않고 지문 없는 문서로 둬 스크립트 없이 진행한다. `SetCancel`이 실패해도 같다.
  - API 문서의 예제도 이 핸들러에서 해당 navigation에 적용되도록 설정을 바꾼다. `NavigationStarting` 이후에 바꾸면 다음 top-level navigation부터 적용된다.
- **macOS(문서 단위 적용과 서브프레임 측정)**: `webView:decidePolicyForNavigationAction:preferences:decisionHandler:`에서 navigation별 `WKWebpagePreferences.allowsContentJavaScript`를 정한다. 허용 기록과 같은 URL의 main frame 탐색은 `decisionHandler`를 복사하고 preferences를 retain해 둔 채 반환한다. 작업 스레드는 경로만 받아 읽는다. WebView 동기화에서 메인 스레드가 표를 확인하고 preferences에 JS를 정한 뒤 Allow를 부른다. 표가 낡았으면 Cancel을 부른다. 새 main frame 탐색, 호스트의 로드·재로드·뒤로·앞으로 요청, web process 종료, webview 해제는 보류한 결정을 Cancel로 먼저 끝낸다. 그래서 handler는 정확히 한 번 불린다. 나머지 탐색은 바로 정하고 작업 스레드에서 읽는다.
  - WebKit은 정책 Cancel 뒤 그 navigation에 `didFailProvisionalNavigation` 계열을 부를 수 있다([WebKit 변경 기록](https://lists.webkit.org/pipermail/webkit-changes/2025-August/322151.html)). 1:1로 부른다는 보장은 없다. 정책 단계의 진짜 실패(sandbox 등)도 provisional 시작 없이 온다. 새 로드의 정책 결정부터 첫 provisional 시작까지는 게이트 세대가 비어 있다. 그래서 이 구간의 실패를 어느 결정에 귀속할지 좁혀야 한다. 앞 결정의 실패를 새 로드 것으로 보면 새 로드가 보류한 결정까지 Cancel된다. 반대로 새 로드의 진짜 실패를 삼키면 그 로드가 `Loading`에 남는다. 귀속 규칙은 `load_generation::CancelledDecisions`에 있다.
    - 정책 결정 콜백은 navigation을 알려 주지 않는다. 호스트 API(`loadFileURL`·`reload` 등)가 돌려준 `WKNavigation`을 종류가 맞는 다음 결정에 붙이면, 같은 종류의 페이지 탐색 결정이 먼저 와서 그 navigation을 가져갈 수 있다. 그러면 Cancel한 결정의 실패 대신 새 로드의 진짜 실패를 흡수한다. 그래서 실패를 navigation으로 귀속하지 않고 오류로 가린다.
    - 세대가 빈 구간에서 정책 변경으로 중단됐다는 오류(`WebKitErrorDomain` 102)만 Cancel한 수만큼 흡수한다. Tasty는 main frame 탐색 정책을 보류 결정의 Cancel로만 바꾸고 응답 정책 delegate가 없으므로, 이 구간의 102는 Tasty가 Cancel한 결정에서 나온다. 다른 오류(정책 단계의 진짜 실패)는 흡수하지 않는다.
    - 흡수한 신호는 chrome과 게이트를 건드리지 않는다. provisional 시작이 수를 비운다. 그 뒤 오는 앞 결정의 실패는 세대 판정이 무시한다.
    - 시험: `load_generation::only_policy_interruptions_are_absorbed_for_cancelled_decisions`, 그리고 순서 시험 `a_late_failure_of_a_cancelled_decision_does_not_cancel_the_next_one`. 순서 시험은 호스트 요청 B보다 같은 종류의 페이지 탐색 A의 결정이 먼저 오고, B의 결정이 A를 Cancel하는 순서를 쓴다. A의 102 실패는 흡수되어 B가 자기 스캔으로 Allow된다. B의 진짜 실패는 A의 실패보다 먼저 오든 나중에 오든 흡수되지 않고 B의 로드를 끝낸다.
    - 남은 경계: Cancel한 결정에 WebKit이 실패를 보내지 않으면 남은 수는 다음 provisional 시작까지 102 오류만 흡수한다. 이 경로는 실 기기에서 측정하지 않았다. native 생성자·gate의 ARM 교차 컴파일은 확인했지만 전체 GUI 앱 빌드·링크는 확인하지 않았다.
  - macOS 27.0.1 실 기기에서 debug 빌드로 실제 실행해 측정했다. 스크립트와 스크립트가 있는 iframe을 가진 `file://` 문서는 main frame과 서브프레임 모두 JS가 꺼진 채 그려졌고, 사용자가 본 뒤 배너가 `blocked`로 떴다. 허용하면 재로드 뒤 두 frame 모두 JS가 돌았다. 다른 문서로 이동하면 그 문서의 JS가 꺼졌고, 허용했던 문서로 돌아와도 허용이 풀려 다시 꺼졌다.
  - 허용 판단이 붙은 webview에서는 전역 `javaScriptEnabled`(deprecated)를 켜 둔다. 전역 값이 꺼져 있으면 navigation별 `allowsContentJavaScript`를 켜도 스크립트가 실행되지 않기 때문이다. 문서의 JS는 navigation별 값으로만 정한다.
  - 그래서 전역 "Sandbox scripts" 변경은 화면 문서에 바로 적용되지 않고 다음 navigation부터 적용된다(미측정).
- Windows bfcache:
  - 소스 근거: Chromium의 `BackForwardCacheImpl::PopulateReasonsForMainDocument`는 main 문서의 마지막 commit URL이 HTTP/HTTPS가 아니면 `kSchemeNotHTTPOrHTTPS` 사유로 저장하지 않는다([back_forward_cache_impl.cc](https://github.com/chromium/chromium/blob/a92605a10f26b2edff954ec56a531f06716a807a/content/browser/back_forward_cache/back_forward_cache_impl.cc#L1079-L1084), Chromium main 기준). 런타임 155가 쓰는 Chromium 분기와 Edge 쪽 변경은 대조하지 않았다.
  - 관측: 위 하네스에서 `file://` 문서의 `Reload`·`GoBack`·`GoForward`, 페이지의 `history.back()`·`location.reload()`, `SetCancel` 뒤 다시 시작한 `GoBack`은 매번 문서 요청(`WebResourceRequested` DOCUMENT 문맥)을 거쳐 바뀐 파일을 다시 실행했다. `pageshow`·`pagehide`의 `persisted`는 false였다.
  - 보장하지 않는 것: 런타임이 file 문서를 요청 없이 복원하지 않는다는 API 보장은 없다. 그런 경로가 생기면 위 문서 요청 확인이 요청 없는 commit으로 보고 그 탐색을 멈춘 뒤 재로드한다. 그래서 허용은 실제로 읽은 지문에만 붙는다. macOS의 bfcache 복원에 navigation별 설정이 적용되는지는 측정하지 않았다.
- 두 OS에서 bfcache를 끌 수 있는지는 구현할 때 확인한다. 응답 단계 없이 commit된 문서는 지문이 없어 허용할 수 없다.
- 두 OS에서도 허용 기록은 새 main frame 문서가 commit될 때 갱신한다. commit 없이 끝난 로드에서는 화면 문서의 허용 상태로 JS를 되돌린다. 이 시점에 대응하는 이벤트는 구현에서 정하며 측정하지 않았다.
  - 두 OS는 로드 시작과 응답 결정을 navigation 시작 콜백에서 함께 한다. 새 로드가 앞 로드를 취소하면 앞 로드의 종료 신호가 새 로드의 시작 뒤에 올 수 있다. 이 신호가 새 로드의 대기 값을 지우면 새 문서가 지문 없이 기록된다.
  - Windows는 `NavigationStarting`의 `NavigationId`를 로드 세대로 기록하고, `NavigationCompleted`는 자기 `NavigationId`가 현재 세대일 때만 복원한다. 앞 로드의 늦은 종료는 무시한다(순서 자체는 미측정).
  - macOS는 `WKNavigation`을 로드 세대로 쓴다. 정책 결정(`decidePolicyForNavigationAction`)에는 `WKNavigation`이 없으므로, 게이트가 로드를 시작한 정책 결정 뒤 첫 `didStartProvisionalNavigation`의 `WKNavigation`을 세대로 기록한다. `didFinishNavigation`·`didFailNavigation`·`didFailProvisionalNavigation`은 자기 `WKNavigation`이 현재 세대일 때만 복원한다. 판정은 Windows와 같은 함수(`load_generation::is_current_load`)이며, 어느 쪽이든 `WKNavigation`을 알 수 없으면 현재 로드로 본다.
    - 정책 결정부터 provisional 시작까지는 세대를 비워 둔다. 이 사이에 온 종료는 모두 현재 로드로 본다. 앞 로드의 종료가 이 구간에 오면 새 로드의 대기 값을 지울 수 있다. 이 순서가 실제로 생기는지는 측정하지 않았다.
    - 게이트 로드가 끝나면(`gate_finished`) provisional 시작 대기 표시도 내린다. provisional 시작 없이 끝난 로드 뒤에 오는 다른 provisional 시작을 세대로 기록하지 않기 위해서다. 그래서 위 구간에 앞 로드의 종료가 오면 새 로드는 세대 없이 진행하고, 그 로드의 종료는 모두 현재 로드로 본다.
    - 기록한 `WKNavigation`은 다음 로드가 시작할 때까지 붙잡아 둔다. 같은 주소가 다른 navigation에 다시 쓰여 앞 로드의 종료가 현재 세대로 보이는 일을 막는다.
    - 실 기기 측정 없이 구현했다. native 생성자와 gate·key·overlay helper는 Mac/Windows ARM 대상으로 교차 컴파일했다. immutable body provider와 host overlay enum을 stub으로 둔 범위이며 전체 GUI 앱 컴파일·링크·native 실행을 보장하지 않는다.
  - chrome의 `NavState` 종료 전이(Done·Failed)도 같은 판정 함수(`load_generation::nav_state_after_end`)를 따른다. 앞 로드의 늦은 실패가 chrome만 Failed로 두거나, 늦은 성공이 새 로드의 Loading을 Done으로 덮어 새 로드가 끝나기 전에 overlay를 보이는 일을 막는다. Windows는 `NavigationCompleted`의 성공·실패, macOS는 `didFinishNavigation`·`didFailNavigation`·`didFailProvisionalNavigation`이 대상이다. 실 기기 측정은 하지 않았다.
    - chrome 세대는 게이트 세대와 따로 둔다. 마지막에 시작한 main frame navigation(Windows `NavigationStarting`의 `NavigationId`, macOS `didStartProvisionalNavigation`의 `WKNavigation`)을 모두 기록한다. 게이트 세대는 fragment 이동과 게이트가 없는 surface의 탐색을 기록하지 않으므로, 이를 chrome에 쓰면 그 탐색의 종료가 앞 로드로 보여 Loading이 남는다.
    - `load_url`·`load_html`은 navigation 시작 신호 전에 Loading을 두고 chrome 세대를 비운다(모름 = 현재). 게이트가 정책 결정에서 세대를 비우는 것과 같은 장치다. 요청한 로드가 시작 신호 없이 끝나도 그 종료를 현재 로드로 보아 Loading에 남지 않는다. 그런 경로가 실제로 있는지는 측정하지 않았다. 후보는 macOS의 provisional 시작 전 정책·스킴 단계 실패와 Windows `NavigationStarting`의 args가 없는 경우다.
      - 대가로 그 사이에 오는 앞 로드의 종료도 현재 로드로 보아 상태를 바꾼다. 뒤이은 시작 신호가 다시 Loading으로 둔다.
    - web process 종료(Windows `ProcessFailed`의 렌더러·브라우저 종료, macOS `webViewWebContentProcessDidTerminate:`)는 세대와 관계없이 Failed로 둔다. 종료 신호에는 navigation이 없고 화면 문서를 잃었기 때문이다.
    - Linux도 같은 함수를 따른다. WebKitGTK의 종료 신호에는 navigation 식별자가 없어 신호 순서로 세대를 매긴다(`load_generation::SignalOrderLoads`).
      - `load_url`·`load_html`이 새 세대를 현재 로드로 둔다. 요청 없이 온 main frame `STARTED`(링크·뒤로 가기)도 새 세대다.
      - `load-failed`와 `FINISHED`는 마지막 `STARTED`를 받은 로드의 것으로 본다. `FINISHED`가 그 로드를 비우고, web process 종료도 비운다.
      - `load_uri`로 앞 로드를 취소하면 앞 로드의 `load-failed`("Load request cancelled")와 `FINISHED`가 새 로드의 navigation 결정과 `STARTED`보다 먼저 온다. 이 두 신호는 앞 로드의 늦은 종료가 되어 chrome은 Loading에 머문다. 판정 전에는 새 `STARTED`가 올 때까지 Failed였다(측정: FIFO `file://` 로드를 걸어 둔 채 다른 파일로 바꾸기, 판정 전 3회·판정 후 4회, WebKitGTK 2.50.4). 없는 파일처럼 현재 로드가 실패하면 Failed가 되고, 뒤따르는 `FINISHED`가 Done으로 덮지 않는다(측정 1회).
      - 앞 로드가 끝나기 전에 요청한 로드가 `STARTED` 없이 실패하면 그 실패도 앞 로드의 것으로 보아 Loading에 남는다. 측정한 순서에서는 앞 로드의 종료가 먼저 와서 생기지 않았다.

#### 로컬 문서의 하위 리소스 읽기 범위

`file://` 문서는 문서 폴더 밖의 상대 CSS·이미지·폰트와 링크·meta refresh 이동 대상도 읽는다. 로컬 파일을 화면에 그리는 것은 막을 대상이 아니다. 스크립트 실행은 위 스크립트 게이트가, 외부 전송은 원격 차단이 따로 막는다.

- Linux·Windows: 일반 WebView에 폴더 범위 제한을 두지 않는다. Linux는 형제 폴더의 CSS·이미지 적용과 meta refresh 이동을 측정했다.
- macOS: `load_url`은 `loadFileURL:allowingReadAccessToURL:`에 파일 시스템 루트를 준다(`file_read_scope::read_access_root`). WKWebView는 이 범위 밖의 하위 리소스를 알림 없이 거부하고, 처음 준 범위가 그 뒤 페이지 안 이동에도 쓰인다. 문서 폴더만 주면 배너·실패 표시 없이 스타일이 빠진 문서가 보였다. `reload`·`go_back`·`go_forward`는 범위를 다시 주지 않으므로 처음 준 범위를 따른다(코드 경로 기준, 측정하지 않았다).
- 범위를 넓혀도 허용한 스크립트가 다른 로컬 파일 내용을 읽지 못했다(macOS 27.2 측정). `fetch`·`XMLHttpRequest`로 `file:///etc/hosts`와 문서 밖 형제 폴더의 일반 파일을 읽지 못했다. 그 일반 파일을 띄운 iframe은 로드되지만 `contentDocument`가 `null`이었다. `file:///etc/hosts` iframe은 로드되지 않아 빈 `about:blank` 문서만 남았고, 로드되지 않은 이유는 조사하지 않았다.
- Linux는 `file:///etc/hosts`에 대한 `fetch`·`XMLHttpRequest`·iframe `contentDocument`가 막힌 것만 측정했다. 문서 밖 일반 파일은 측정하지 않았다. 다른 macOS 버전과 Windows는 측정하지 않았다.
- markdown surface는 렌더한 HTML을 `load_html`로 올리므로 이 범위와 관계없다. 문서 밖 이미지는 [markdown 인라인 이미지 규칙](../markdown/index.md)을 따른다.
- 원격 attach entry 모드는 custom scheme을 쓰므로 이 경로와 별개다.

### 사용자가 봤다는 판정

배너는 사용자가 그 문서를 볼 때만 띄운다. "사용자가 봤다"는 다음 신호로 판정한다.

"사용자가 봤다"는 다음 신호로 판정한다.

- 포커스된 surface가 바뀐 순간을 사용자 선택으로 본다. release의 에이전트 경로는 탭과 surface 선택을 바꾸지 않기 때문이다. 창을 처음 그리는 프레임의 포커스는 선택으로 세지 않는다. 그래서 세션 복원 문서는 기록만 한다.
- 문서가 없거나 로드 중일 때 선택되면 다음에 commit되는 문서를 본 것으로 한다. 사용자가 연 파일은 새 탭이 선택된 뒤 첫 문서가 오기 때문이다.
- 사용자가 그 surface에서 직접 시작한 로드(주소 입력·뒤로·앞으로·새로고침, [사용자 탐색](#내부-동작))의 문서는 본 것으로 한다. 그 surface를 보고 조작한 것이기 때문이다.
- 사용자가 보던 문서에서 링크나 스크립트로 바뀐 문서는 본 상태를 이어받는다. 호스트가 URL을 넣은 로드(plugin·IPC·복원)와 에이전트 히스토리 이동은 이어받지 않는다. 그래서 에이전트가 사용자가 보던 surface에 넣은 문서도 다음 선택 때까지 기록만 한다.
- 한계: 에이전트가 포커스된 탭을 닫아 포커스가 다른 surface로 옮겨 가는 경우도 선택으로 센다.

## 원격 콘텐츠 차단

로컬 html 문서가 네트워크(`http(s)`)에서 스타일시트·이미지·글꼴·프레임·스크립트를 받는 것은 기본으로 막는다. 전역 "Allow remote content" 설정은 모든 문서에서 막지 않게 한다. 문서 단위 허용은 한 문서에만 막지 않게 한다.

### 감지

스크립트 스캔과 같은 순회에서 정적 원격 참조를 찾는다(`html_script::scan::detect`). 원격 안내와 표지의 근거로만 쓴다. 차단 자체는 감지와 관계없이 OS 쪽 규칙이 한다.

- 대상 속성
  - `link`(rel이 stylesheet·icon·apple-touch-icon·preload·prefetch·modulepreload·manifest)의 `href`와 `imagesrcset`
  - `img`·`source`의 `src`와 `srcset`
  - `video`의 `src`와 `poster`
  - `audio`·`track`·`iframe`·`frame`·`embed`·`input type=image`의 `src`
  - `object`의 `data`
  - SVG `image`·`feImage`·`use`의 `href`와 `xlink:href`
  - `body`·`table`·`td`·`th`의 `background`
  - `style` 속성, `<style>` 내용의 `url()`과 `@import`
- 상대 URL: 첫 `<base href>`가 원격이면 상대 URL도 원격으로 본다. `srcset`은 HTML 파서처럼 공백까지를 URL로 읽어 `data:` URL 안의 쉼표를 후보 구분으로 보지 않는다.
- `<template>` 내용은 문서에 붙기 전에는 받지 않으므로 원격 참조, base, 원격 스크립트로 세지 않는다. 스크립트 감지 결과는 template을 보통 태그로 읽던 기존 판정과 같다. 중첩되거나 닫히지 않은 template도 재귀 없이 한 번 순회한다.
- 감지하지 않는 범위
  - `link rel=preconnect`(연결만 함)
  - `iframe srcdoc` 안의 HTML
  - CSS `image-set()`의 문자열 URL
  - scheme만 있는 base(`http:`)
  - 스크립트가 만드는 요청
  - 스캔 상한 뒤의 참조

  이런 문서는 정적 참조가 없다고 판정한다. 그래도 차단은 OS 규칙이 하므로 요청은 막힌다.

### 문서 단위 허용

원격 허용 기록은 스크립트 허용과 따로 두고 서로 바꾸지 않는다. 수명 규칙은 [허용의 수명](#허용의-수명)과 같다. 그 문서(URL과 내용 지문)에만 묶이고, 같은 문서의 재로드는 유지하며, 다른 문서가 commit되거나 내용이 바뀌면 푼다. 세션 안에서만 유지한다.

- 원격 허용 기록과 같은 URL의 응답은 스크립트 허용처럼 스캔 결과를 기다려 지문이 맞을 때만 그 로드의 차단을 푼다. 전역 설정이 켜져 있어도 기다린다. 기다리지 않고 commit하면 허용 기록이 풀려, 전역을 다시 껐을 때 같은 문서가 막히기 때문이다.
- 전역 설정을 켰다 꺼도 허용 기록은 지우지 않는다.
- 로드 중(commit 전)이거나 지문이 없는 문서는 허용하지 않는다. 전역 설정이 이미 허용하면 `globally_allowed`로 거절한다.
- entry mirror와 HTTP view는 차단을 다른 곳이 정한다. 그래서 이 view에서는 문서 단위 허용과 안내를 지원하지 않고 `unsupported_view`로 거절한다. 두 view의 정책은 달라서 `remote.view`(`local`·`mirror`·`http`)로 구분하고 실효값(`remote.effective`)도 view마다 정한다.
  - mirror: 응답 경계가 전역 설정과 관계없이 원격을 늘 막는다. 실효값은 늘 거짓이다.
  - HTTP view: endpoint가 client의 전역 설정과 같은 origin 기본 허용으로 정한다. 실효값은 전역 설정이다. 같은 origin 요청은 설정과 관계없이 열려 있다.
- top-level `http(s)` 탐색은 아래 [top-level 탐색](#top-level-탐색)의 규칙을 따른다. 허용한 문서에서 연 탐색이 commit되면 다른 문서이므로 허용이 풀리고 새 문서의 하위 원격 요청은 다시 막힌다. 원래 문서로 돌아와도 허용 기록이 없으므로 다시 허용해야 열린다. 시험은 `a_remote_navigation_from_an_allowed_document_releases_the_allowance`와 `the_navigation_origin_stays_with_the_committed_document`다.

### top-level 탐색

원격 차단은 화면 문서의 하위 리소스(스타일시트·이미지·글꼴·스크립트·fetch 등)와 서브프레임 문서를 막는다. main frame의 `http(s)` 문서 요청은 다음 출처일 때 연다(`tasty_model::html_script::top_level`).

- 호스트가 지목한 로드: 주소창 Enter, `tasty html open`, plugin·복원이 넣은 `webview_url`, 새로고침, 뒤로·앞으로. 화면 문서의 차단 여부와 관계없다.
- 원격이 열린 화면 문서(전역 설정 또는 문서 단위 허용)가 시작한 링크·meta refresh·`location` 이동.
- `http(s)` 화면 문서에서 사용자가 링크를 눌러 시작한 이동(사용자 제스처). 그 문서의 meta refresh·스크립트 이동은 열지 않는다.

연 판단은 그 로드의 리다이렉트 hop에도 이어지고 commit이나 그 로드의 종료에서 닫힌다. 근거는 탐색이 시작된 시점의 commit된 화면 문서다. 새 문서는 허용을 받지 않으므로 `remote.allowance`가 없고 `allowed`·`effective`가 거짓이다. 새 문서가 시작한 다음 이동도 위 규칙으로 다시 정한다. 위 출처가 아닌 이동(허용 없는 로컬 문서의 링크 등)은 첫 요청부터 보내지 않는다. 리다이렉트 대상이 `file://`이면 WebKit이 로드를 끝내지 않는다(`Load request cancelled`).

차단은 로드 시작부터 켜 둔 채로 두고 top-level 문서 요청만 따로 연다. commit 때 차단을 다시 켜지 않으므로 새 문서의 첫 하위 요청이 나갈 틈이 없다. 연 `http(s)` 문서는 스캔이 없어 지문이 없으므로 원격 허용을 받을 수 없고, 정적 참조 감지도 없어 원격 안내가 뜨지 않는다.

- Linux: content filter가 `^https?://`를 막되 `load-context: top-frame`인 `document` 요청은 빼고(`ignore-previous-rules`), 탐색 정책이 navigation action과 main frame 응답을 위 규칙으로 정한다. 서브프레임 문서는 `child-frame`이라 filter가 막는다. navigation action은 main frame과 서브프레임을 구분하지 못하므로 연 판단은 서브프레임 문서를 열지 않는다. 호스트 지목은 `load_url`의 주소, 새로고침·뒤로·앞으로의 navigation type으로 대조한다.
- Windows: `NavigationStarting`(main frame만)에서 허용을 정하고 리다이렉트(`IsRedirected`)마다 대상 주소를 옮긴다. `WebResourceRequested`는 `DOCUMENT` 문맥이고 그 hop 주소인 요청만 거절하지 않는다. 호스트 지목은 `Navigate`·`Reload`·`GoBack`·`GoForward` 직후의 다음 탐색이다. 뒤로·앞으로는 이동할 이력이 있을 때만 지목한다. `IsUserInitiated`는 meta refresh에도 참으로 오므로, 사용자 제스처는 host 입력 경로가 1초 안에 본 OS 입력(WebView로 간 왼쪽 클릭, 페이지로 넘긴 Enter·Space)도 있을 때만 인정한다([webview 탐색](../../design/systems/webview.md)의 입력 증명과 같다). host 입력 경로를 쓰지 못한 view에서는 `http(s)` 문서 안 링크 이동을 열지 않는다.
- Linux 측정(WebKitGTK 2.50.4, 서버 접근 로그를 tag로 센 값): 허용한 문서의 클릭·meta refresh·`location`, 주소 입력 재현(`debug webview-history --action open`), `tasty html open`, 허용 전 문서와 빈 탭의 주소 입력에서 B가 commit됐고 B 하위 요청은 모두 0건이었다. 302·301·303·307·308, 3단·지연 리다이렉트도 같았다. 전역 허용 대조군에서는 같은 이동의 B 하위 요청 2건이 서버에 도착했다. 허용 없는 로컬 문서의 클릭은 첫 hop부터 0건이었고 `STARTED`도 오지 않았다. B의 원격 iframe, B의 meta refresh가 가는 C, 허용 문서에서 간 로컬 문서의 meta refresh가 가는 C도 0건이었다. B 안에서 사용자가 누른 링크와 주소창으로 연 B의 새로고침·뒤로·앞으로는 이동했다. 지연 없는 서버로 허용 문서 클릭 20회, 주소 입력 20회를 반복해 40회 모두 commit됐고 B 하위 요청 합계는 0건이었다. 허용한 로컬 문서의 로컬 iframe이 http로 이동하면 요청이 나가지만 main 문서의 허용은 바뀌지 않았다. https는 측정하지 않았다.
- Windows 측정(Windows 11 x64 WebView2, 서버 접근 로그를 tag로 센 값): 허용한 문서의 meta refresh(직접·302), 주소 입력 재현, `tasty html open`, 허용 전 문서와 빈 탭의 주소 입력에서 B가 commit됐고 B 하위 요청은 0건이었다. 전역 허용 대조군에서는 B 하위 요청 3건이 도착했다. 허용 없는 로컬 문서의 meta refresh와 링크 클릭은 0건이었다. B의 원격 iframe과 B의 meta refresh가 가는 C도 0건이었다. 주소 입력으로 연 http 문서 안의 실제 마우스 클릭은 302를 거쳐 C를 commit했고 C 하위 요청은 0건이었다. 주소창으로 연 B의 새로고침·뒤로·앞으로는 이동했다. 주소 입력 20회는 모두 commit됐다. 연 B는 OS가 막은 하위 요청을 관측하므로 `blocked` 표지가 남는다(Linux는 남지 않는다). https는 측정하지 않았다.
- macOS: 이 규칙을 아직 적용하지 않았다. 규칙 목록이 문서 요청도 막는다. 문서 단위 허용 중 meta refresh·스크립트 이동과 주소 입력 재현(`debug webview-history --action open`)이 `blocked by a content blocker`로 실패했다(측정). 허용 전 문서의 주소창·`html open`과 실제 링크 클릭은 측정하지 않았다.

### 적용

backend는 실효값 `전역 설정 || 문서 단위 허용` 하나를 한 곳에서 native에 쓴다. 문서 단위 값은 스크립트 게이트가 상태를 바꾸는 지점마다 다시 읽는다. 그 지점은 로드 시작, 응답 결정, 보류한 결정의 재개, commit, 종료·실패다. commit 전에는 그 로드의 응답 결정을 따르고, commit 뒤에는 화면 문서의 허용을 따른다. 그래서 전역 설정 변경과 문서 허용이 서로를 덮지 않고, 허용이 다음 문서로 새지 않는다.

- Linux: http(s) content filter와 탐색 정책을 `linux/remote_block.rs`가 켜고 끈다. 실효값은 하위 리소스와 서브프레임에 적용하고, main frame 문서 요청은 [top-level 탐색](#top-level-탐색) 규칙이 정한다.
- macOS: WKContentRuleList를 `macos/remote_block.rs`가 붙이고 뗀다.
- Windows: `WebResourceRequested`가 요청마다 게이트의 문서 값을 읽는다(`windows/remote_block.rs`). 연 top-level 탐색의 main 문서 요청은 거절하지 않는다. 막은 하위 리소스 요청은 OS 관측으로 게이트에 알린다. 정적 참조 없이 막힌 문서의 표지 근거다.
- 규칙·필터 컴파일 전이나 컴파일 실패 동안에는 막지 못한다.

### 안내와 표지

안내 판정은 스크립트 배너와 같은 열람 규칙([사용자가 봤다는 판정](#사용자가-봤다는-판정))을 따른다. 정적 원격 참조가 있고 원격이 막혀 있으며 닫지 않은 문서에서 안내가 필요하다. 정적 참조 없이 OS가 차단을 알린 문서는 안내 대신 표지를 남기고, 표지를 누르면 안내를 보인다(허용하지 않는다). 허용한 문서는 `allowed` 표지가 된다. 전역 설정이 켜져 있으면 안내와 표지가 없다. 상태는 `surface.html_script`의 `remote` 객체로 조회한다.

- **안내 카드** — 공용 view `tasty_ui_widgets::html_remote_banner`다. 스크립트 배너와 같은 inset 셸·단계(`blocked`·`loading`·`reloading`)·좁은 배치·닫기 규칙을 쓰고 글리프만 globeOff(`html-remote-banner-glyph`, accent-info)다. 제목 "Remote content in this document is blocked", 본문은 막는 종류(스타일시트·이미지·글꼴·프레임·스크립트)와 허용 범위(이 문서만)를 말하고, 액션은 [Allow remote content]다. 제목은 정적 참조와 OS 관측을 구분하지 않는다. 문구 키는 `banner.html_script.*` 옆의 `banner.html_remote.*`다. mirror 원격 수신 문구 `[html_remote]`와 헷갈리지 않게 하려는 이름이며, 디자인 leaf `allow`·`allow_loading`은 앱 `action`·`action_loading`에 대응한다.
- **위치** — 주소 띠 아래, mirror 크기 안내 뒤, 스크립트 배너 앞이다([주소 표시줄](#주소-표시줄)의 배치). 원격 스크립트가 있는 문서에서 두 안내가 함께 보이면 스크립트 배너 본문은 `banner.html_script.body_remote` 문구다.
- **동작** — 허용·닫기·다시 보이기는 스크립트 배너와 같은 문서 묶음 요청(`HtmlActionKind::RemoteAllow`·`RemoteDismiss`·`RemoteReshow`)으로 다음 프레임에 적용한다. 요청을 만든 문서가 바뀌었으면 버린다. 허용은 사용자 출처 요청만 받는다. 허용을 누르면 재로드 중 카드가 스피너를 보이고, 새 문서가 commit되면 `banner_fade` 동안 흐려지며 사라진다. 원격 허용과 스크립트 허용은 서로 바꾸지 않는다. 배너는 키보드 포커스를 가져가지 않는다.
- **페이드 분리** — 원격·스크립트 카드는 페이드 메모를 따로 둔다. 한 카드가 사라지는 동안 다른 카드의 위치는 사라지는 카드의 높이가 빠진 뒤 한 번 바뀐다.

## 인터페이스

- **사용자**: html surface 위쪽 [주소 표시줄](#주소-표시줄)에서 경로·URL 입력, 뒤로·앞으로·새로고침, 주소창 포커스 단축키(`Alt+L`, macOS `⌘L`).
- **사용자**: HTML·SVG 파일 열기 → html surface(WebView). 탭 스트립 빈 곳 메뉴의 **새 HTML...** 은 파일 피커(`html`·`htm`·`svg` 필터)를 열고, 고른 파일의 절대 경로를 `url` 파라미터로 넘겨 메뉴를 연 pane 에 html 탭을 만든다([파일 피커 확정](../../features/native-file-picker/index.md#확정confirm)). 지원 연결의 mirror pane에서는 원격 cwd로 피커를 열고 원래 pane에 원격 탭을 만든다. 미지원 연결은 원격 열기 불가 토스트를 띄운다.
- **AI Agent**: `tasty html …` CLI / `html.*` IPC. surface 생성은 [work-area](../../features/work-area/index.md) (`--type html --url …`).
- **AI Agent**: 스크립트·원격 콘텐츠의 감지·허용·안내 상태 조회는 `tasty surface html-script --surface <id>`(IPC `surface.html_script`), 현재 문서의 스크립트 허용은 `tasty surface html-script-allow --surface <id>`(IPC `surface.html_script_allow`), 원격 콘텐츠 허용은 `tasty surface html-remote-allow --surface <id> [--url <url> --fingerprint <hex>]`(IPC `surface.html_remote_allow`, 권한 `surface.write`).
- **AI Agent**: 보여 주는 문서 주소와 뒤로·앞으로 이동 가능 여부 조회는 `tasty surface html-nav --surface <id>`(IPC `surface.html_nav`).
- **AI Agent**: 뒤로·앞으로·새로고침은 `tasty surface html-back`·`html-forward`·`html-reload --surface <id>`(IPC `surface.html_back`·`surface.html_forward`·`surface.html_reload`).

## 비-목표

- WebView 오버레이 동기화 메커니즘 — host(gpu/webview) 구현.
- surface 배치/생성 도메인 — [work-area](../../features/work-area/index.md).

## Acceptance Criteria

- Given html 플러그인 활성 When `tasty new tab --type html --url <u>` Then WebView surface 가 그 URL 을 띄운다.
- Given HTML 파일 열기 Then html surface 로 뜬다.
- Given 플러그인 disable Then `html` 확장자 detector 는 host 가 유지한다.
- Given html 플러그인 활성 When 탐색기에서 `.svg` 파일을 더블클릭 Then 핸들러 선택 창 없이 html surface 새 탭이 열려 SVG 를 렌더한다.
- Given sandbox scripts가 켜져 있고 에이전트가 스크립트가 있는 `file://` 문서를 html 탭으로 열었다 When `tasty surface html-script --surface <id>` Then `document.detection`은 `scripts`, `javascript`는 `false`, `banner.phase`는 `hidden`, `banner.pending_view`는 `true`이고 조회 전후 상태가 같다.
- Given 같은 문서 When 사용자가 그 surface를 선택한다 Then 다음 조회의 `banner.phase`는 `blocked`다.
- Given 사용자가 보고 있는 surface의 배너가 `blocked` When 배너를 그린다 Then WebView는 배너 카드 아래 `banner_inset_gap` 뒤에서 시작하고, 배너가 사라지면 콘텐츠 위 끝으로 돌아간다.
- Given surface 폭이 `banner_narrow_below` 미만 When 배너를 그린다 Then 허용 버튼이 본문 아래 줄에 놓인다.
- Given 사용자가 배너의 ×를 눌렀다 When 탭 바를 그린다 Then 그 탭에 lock 표지가 보이고 조회의 `banner.dismissed`는 `true`다.
- Given lock 표지가 있는 탭이 비활성 When 사용자가 lock을 누른다 Then 조회의 `banner.phase`는 `blocked`이고 활성 탭은 바뀌지 않는다.
- Given 사용자가 [이 문서에서 허용]을 눌렀다 When 재로드가 끝난다 Then 조회의 `allowed`·`javascript`는 `true`, `banner.phase`는 `hidden`이고 탭에 scriptFile 표지가 보인다.
- Given 사용자가 보고 있는 surface의 배너가 `blocked` When 그 surface가 새 문서를 로드하기 시작했고 아직 commit 전이다 Then 조회의 `loading`은 `true`, `banner.phase`는 `loading`이고 허용 버튼은 비활성이며 ×는 그대로 누를 수 있다.
- Given 배너 단계가 `loading`이고 그 로드는 사용자가 연 이동(링크·뒤로 가기·재로드처럼 호스트가 URL을 넣지 않은 이동)이다 When 새 문서가 commit되고 그 문서에 스크립트가 있다 Then 조회의 `loading`은 `false`, `banner.phase`는 `blocked`이고 허용 버튼이 다시 활성이다.
- Given 배너가 `blocked`이거나 탭에 lock·scriptFile 표지가 있다 When 다음 로드가 문서를 commit하지 못하고 실패한다 Then host chrome이 "Failed to load"를 보이는 동안 조회의 `banner.phase`는 `hidden`이고 탭 표지가 없다. 그 뒤 재로드가 commit되면 배너와 표지를 새 문서로 다시 정한다.
- Given 배너 단계가 `loading`이고 그 로드는 에이전트가 넣은 이동(`tasty html open` 등)이다 When 스크립트가 있는 새 문서가 commit된다 Then [사용자가 봤다는 판정](#사용자가-봤다는-판정)대로 `banner.phase`는 `hidden`, `banner.pending_view`는 `true`이고 사용자가 그 surface를 선택할 때 배너가 `blocked`로 뜬다.
- Given 터미널이나 markdown surface When `surface.html_script` Then invalid params로 거절한다.
- Given sandbox 기본값에서 에이전트가 연 스크립트 있는 `file://` 문서와 사용자가 보는 다른 탭 When 에이전트가 `surface.html_script` 응답의 `url`·`fingerprint`로 `tasty surface html-script-allow`를 실행한다 Then 응답 `result`는 `allowed`이고, 재로드 뒤 조회의 `allowance`·`allowed`·`javascript`는 배너로 허용했을 때와 같으며, 사용자가 보던 탭과 포커스는 그대로다.
- Given 조회 뒤 문서가 바뀌었거나 로드 중이거나 지문이 없는 문서 When `surface.html_script_allow` Then `-32022`와 `error.data.reason` `document_changed`·`loading`·`no_fingerprint`로 거절하고 허용 기록은 그대로다.
- Given 원격 `http` 이미지·스타일시트를 참조하는 로컬 문서(전역 원격 허용 끔) When 문서를 연다 Then 원격 서버에 요청이 가지 않고, `surface.html_script`의 `remote.refs`는 참, `remote.effective`는 거짓이다(Linux 실제 실행, http 접근 로그로 확인).
- Given 그 문서 When 조회 응답의 `url`·`fingerprint`로 `tasty surface html-remote-allow`를 실행한다 Then `result`는 `allowed`이고, 재로드에서 원격 요청이 서버에 도착하며, 스크립트 허용 기록과 JS 값은 그대로다(Linux 실제 실행).
- Given 원격을 허용한 문서 When 같은 문서를 재로드하거나, 전역 원격 허용을 켠 채 재로드한 뒤 다시 끈다 Then 허용 기록과 원격 요청이 유지된다(Linux 실제 실행, 모델 시험 `reloading_with_the_global_switch_on_keeps_the_document_allowance`).
- Given 원격을 허용한 문서 When 파일 내용을 바꾼 뒤 재로드하거나 다른 문서를 연다 Then 허용 기록이 풀리고 새 문서의 원격 요청은 가지 않는다(Linux 실제 실행).
- Given 원격을 허용한 로컬 문서 When 클릭·meta refresh·`location`으로 `http(s)` 문서로 바로 또는 리다이렉트를 거쳐 이동한다 Then 새 문서가 commit되고 새 문서의 하위 원격 요청은 0건이며 `remote.allowance`가 없다(Linux 실제 실행).
- Given 원격 허용 여부와 관계없는 html surface When 주소창 Enter나 `tasty html open`으로 `http(s)` 주소를 연다 Then 그 문서가 commit되고 하위 원격 요청은 0건이다. 그 문서에서 사용자가 누른 링크와 새로고침·뒤로·앞으로는 이동하고, 문서 자신의 meta refresh·스크립트 이동과 원격 iframe은 요청하지 않는다(Linux 실제 실행).
- Given 에이전트가 원격 참조가 있는 로컬 문서를 html 탭으로 열었다 When 사용자가 그 탭을 선택한다 Then 원격 안내 카드가 주소 띠 아래에 보이고 WebView는 카드 아래 `banner_inset_gap` 뒤에서 시작하며, 원격 요청은 가지 않는다(Linux 실제 실행).
- Given 원격 안내가 보인다 When 사용자가 ×를 누른다 Then 카드가 사라지고 주소 입력란 끝에 globeOff 표지가 보이며 조회의 `remote.marker`는 `blocked`다. When 그 표지를 누른다 Then 안내가 다시 `blocked`로 보이고 `remote.allowance`는 `null`이며 활성 탭은 그대로다(Linux 실제 실행).
- Given 원격 안내가 보인다 When 사용자가 [Allow remote content]를 누른다 Then 재로드 중 카드가 스피너를 보인 뒤 사라지고, 원격 요청이 서버에 도착하며, 주소 입력란 끝에 globe 표지가 보이고 스크립트 허용과 JS 값은 그대로다(Linux 실제 실행).
- Given 원격 참조와 인라인 스크립트가 함께 있는 문서(sandbox 켜짐) When 사용자가 본다 Then 주소 띠 아래에 원격 안내, 그 아래 스크립트 배너 순서로 두 카드가 따로 쌓인다(Linux 실제 실행).
- Given 전역 원격 허용이 켜져 있다 When 원격 참조가 있는 문서를 본다 Then 원격 안내와 표지가 없다(Linux 실제 실행).
- Given 전역 원격 허용이 켜져 있거나 대상이 mirror·HTTP view When `surface.html_remote_allow` Then `-32022`와 `reason` `globally_allowed`·`unsupported_view`로 거절하고 상태를 바꾸지 않는다.
- Given 전역 원격 허용이 켜진 entry mirror에서 `http(s)` `src` 스크립트만 있는 문서 When `surface.html_script` Then `remote.view`는 `mirror`, `remote.effective`는 거짓이고 스크립트 배너가 뜨지 않는다. HTTP view의 같은 문서는 전역 설정과 관계없이 스크립트 배너가 뜬다(모델 시험 `each_view_reports_its_own_remote_policy`).
- Given html 탭이 `file:///tmp/x/a.html`을 보여 준다 When 페이지 안 `#sec` 링크를 누른다 Then `surface.html_nav`의 `url`은 `file:///tmp/x/a.html#sec`이고 `requested_url`은 그대로다.
- Given `a/index.html`이 `../b/style.css`와 `../b/dot.png`를 쓴다 When `tasty html open`으로 연다 Then 세 OS 모두 스타일과 이미지가 적용되고 스크립트 배너는 뜨지 않는다.
- Given `a/redirect.html`이 meta refresh로 `../b/next.html`을 가리킨다 When 연다 Then `surface.html_nav`의 `url`은 `b/next.html`이고 `nav_state`는 `done`이다.
- Given 스크립트를 허용한 `file://` 문서 When `fetch`·`XMLHttpRequest`로 다른 로컬 파일을 읽거나 그 파일을 띄운 iframe의 `contentDocument`를 읽는다 Then 내용을 얻지 못한다.
- Given html 탭이 리다이렉트되는 주소를 열었다 When 로드가 끝난다 Then `url`은 최종 주소다.
- Given html 탭이 문서 A를 보여 준다 When 다음 로드가 commit 전에 실패한다 Then `nav_state`는 `failed`이고 `url`은 A다.
- Given html 탭에서 A 다음 B를 열었다 When 조회한다 Then `can_go_back`은 `true`, `can_go_forward`는 `false`다. When 뒤로 간다 Then `url`은 A이고 `can_go_forward`는 `true`다.
- Given 사용자가 보지 않은 html 탭(에이전트가 연 문서) When 사용자 탐색으로 스크립트가 있는 문서를 연다 Then commit 뒤 `banner.phase`는 `blocked`이고 `banner.pending_view`는 `false`다.
- Given 히스토리 처음에 있는 html 탭 When 사용자 탐색으로 뒤로 간다 Then 로드가 일어나지 않고 배너 판정도 바뀌지 않는다.
- Given 터미널이나 markdown surface When `surface.html_nav` Then invalid params로 거절한다.
- Given 에이전트가 연 html surface에서 A 다음 B로 이동했다 When `tasty surface html-back --surface <id>` Then 응답의 `queued`는 `true`이고 다음 redraw 뒤 `surface.html_nav`의 `url`은 A, `can_go_forward`는 `true`이며 포커스된 surface와 활성 탭은 그대로다.
- Given 사용자가 보던 html surface When 에이전트가 `html-back`·`html-forward`·`html-reload`로 스크립트가 있는 문서를 연다 Then commit 뒤 `banner.phase`는 `hidden`, `banner.pending_view`는 `true`이고 사용자가 그 surface를 선택할 때 `blocked`가 된다.
- Given 히스토리 처음에 있는 html surface When `html-back` Then 응답은 성공(`queued: true`)이고 다음 redraw에서 로드가 일어나지 않으며 배너 판정도 바뀌지 않는다. `html-forward`(끝)·문서 없는 `html-reload`도 같다.
- Given 터미널·markdown surface, 없는 surface 또는 미지원 placeholder When `surface.html_back`·`surface.html_forward`·`surface.html_reload` Then invalid params로 거절하고 큐에 넣지 않는다.
- Given 빈 html 탭(`tasty new tab --type html`) Then 위쪽에 placeholder가 있는 주소 띠가 보이고 아래는 html 글리프·"No page loaded"·주소창 힌트다.
- Given 빈 html 탭 When 주소 띠에 `/tmp/x/index.html`을 입력하고 Enter Then 그 파일이 열리고 띠가 그 경로를 보인다.
- Given `/tmp/x/index.html`이 열린 탭 When `b.html`을 입력 Then `/tmp/x/b.html`을 연다. Given 웹 문서 `https://example.com/a/` When `b.html` Then `https://example.com/a/b.html`을 연다. Given cwd가 `/tmp/y`인 빈 탭 When `c.html` Then `/tmp/y/c.html`을 연다.
- Given cwd도 문서도 없는 빈 탭 When 상대 경로를 입력 Then 요청을 보내지 않고 오류 행이 보인다.
- Given 없는 파일 경로 입력 Then 요청을 보내지 않고 오류 행이 보이며 이전 문서와 주소가 그대로다. When 다음 키를 입력하거나 Esc Then 오류가 사라진다.
- Given 편집 중 When Esc Then 편집이 취소되고 commit 주소로 돌아간다.
- Given WebView 페이지에 키보드 포커스 When 주소창 포커스 단축키 Then 그 surface의 주소창에 포커스가 가고 주소 전체가 선택되며 키가 페이지로 가지 않는다. When 설정에서 단축키를 바꾼다 Then 새 키로 동작하고 이전 키는 페이지로 간다.
- Given 사전 확인을 통과한 주소의 실제 로드가 실패한다 Then chrome은 "Failed to load", 띠는 시도한 주소를 보이고 `surface.html_nav`의 `url`은 이전 문서다.
- Given 띠로 스크립트가 있는 문서를 연다(sandbox 켜짐) Then commit 뒤 배너가 바로 뜨고 띠 → 배너 → 페이지 순서로 쌓이며 WebView는 둘의 높이만큼 내려간다. When 배너를 닫는다 Then WebView가 띠 바로 아래로 올라온다.
- Given 띠에서 입력 중 When 에이전트가 `tasty html open <url>` Then 입력 텍스트와 포커스는 그대로다.
- Given 히스토리 처음 Then 뒤로가, 끝이면 앞으로가 비활성이다. Given 폭 440 미만 Then 앞으로 버튼이 숨는다.
- Given 띠로 연 문서 When 재시작 Then 생성 때의 URL(빈 탭이면 빈 탭)로 복원된다.
- Given entry viewer를 협상한 mirror의 html leaf Then 원격 주소가 읽기 전용이고 Reload만 보인다. 미지원 placeholder에는 주소 띠가 없다.
- Given 플러그인 disable When `.svg` 파일을 열기 Then `svg` detector 는 남고 핸들러가 없어 선택 창이 뜨며, 헤더 형식 표시는 `svg` 다.
- Given 로컬 workspace 의 pane When 탭 스트립 빈 곳 메뉴에서 **새 HTML...** 을 고른다 Then 그 pane 의 선택 탭 cwd 에서 파일 피커가 열리고 디렉터리와 `.html`·`.htm`·`.svg` 파일만 보인다.
- Given 그 피커 When 파일을 확정한다 Then 메뉴를 연 pane 에 html 탭이 하나 생겨 활성화되고, `url` 파라미터는 고른 절대 경로이며 WebView 는 `file://` URI 로 띄운다.
- Given 그 피커 When 취소한다 Then 탭 수가 그대로다.
- Given 그 피커가 열린 동안 메뉴를 연 pane 이 닫혔다 When 확정한다 Then 탭이 생기지 않는다.
- Given 활성 pane 이 아닌 pane 에서 메뉴를 열었다 When 피커가 열린 동안 활성 pane 이 바뀐 뒤 확정한다 Then 탭은 메뉴를 연 pane 에 생긴다.
- Given 지원 연결의 mirror pane When **새 HTML...** 에서 확정한다 Then 원래 pane에 원격 탭을 만들고 client 파일을 읽지 않는다. 취소·pane 종료·session 교체 뒤 늦은 결과는 탭을 만들지 않는다.

## 화면

화면정의서 — **HTML surface 화면**.

- **시각 소스**: 네이티브 WebView 오버레이 — 콘텐츠는 OS WebView 가 그린다(디자인 토큰 무관).

[작업 영역](../../features/work-area/index.md#화면) 타일 위치에 WebView 오버레이로 그려지는 HTML surface.

### 트리거

HTML·SVG 파일 열기, 탭 스트립 메뉴 **새 HTML...**(파일 피커), 또는 `html` surface 생성(`--url`).

### UI 요소 인벤토리

- **주소 띠** — 높이 40(`html-address-bar-height`), `bg-sidebar`, 아래 1px separator. 뒤로·앞으로·새로고침(IconButton sm 24)과 주소 입력란(높이 28, 최소 96). 입력 오류 때 24px 오류 행이 더해진다. 동작은 [주소 표시줄](#주소-표시줄).
- **안내 카드** — 주소 띠 아래 inset 배너. mirror 크기 안내 → 원격 콘텐츠 안내(globeOff) → 스크립트 안내(lock) 순서로 따로 쌓는다. 동작은 [안내와 표지](#안내와-표지)와 스크립트 배너 절.
- **원격 콘텐츠 표지** — 주소 입력란 끝의 globeOff(차단)·globe(허용) 표지. 칸 20, 글리프 14.
- **WebView 콘텐츠** — URL/파일의 웹 렌더. 타일 rect 에서 주소 띠와 배너 아래에 오버레이로 정렬.
- 탭 표시명은 파일명/URL.

### 상태별 시각

surface 는 트리에선 `RemoteSurface` marker. 네이티브 WebView 의 navigation 생명주기
(start/finish/fail)를 3 backend(WebView2 / WKNavigationDelegate / WebKitGTK)가 `NavState`
(Idle/Loading/Done/Failed)로 host 에 전달하고, host 가 그 상태에 따라 chrome 을 그린다:

- **Idle** — 문서 없음. placeholder(html 글리프 · "No page loaded"). html surface 는 그 아래에 주소창 힌트 caption(`text-muted`)을 더한다.
- **Loading** — 탐색 중. WebView overlay 를 숨기고 `Spinner` + "Loading…" chrome.
- **Done** — 성공. WebView overlay 가 페이지를 그린다. 메뉴/팝업으로 overlay 가 일시
  숨겨지거나(오버레이 모드가 숨김일 때다. 투명 모드에서는 페이지 위에 팝업을 합성하고, 불투명 모드에서는 페이지를 보인 채 팝업 상자만
  가린다. OS 별 모드는 [WebView §egui 오버레이와 native WebView](../../design/systems/webview.md#egui-오버레이와-native-webview)에 있다), 토스트 카드가 이 surface 와 겹쳐 이 WebView 만 숨겨지면(지금 키를 받는 WebView 는 숨기지 않는다) 그 동안 boundary 타일이 보인다. boundary 는 `bg-panel` 배경과 1px `border-default`
  테두리만 있는 빈 타일이며 글리프·라벨·URL 을 그리지 않는다.
- **Failed** — 실패. overlay 를 숨긴 채 `ALERT_CIRCLE`(`accent-danger`) + "Failed to load"
  + URL chrome. 실패 사유는 화면 대신 `tracing::warn!` 로그로만 남긴다. 이 상태에서는 스크립트
  차단 배너와 탭 표지를 그리지 않는다(아래 "스크립트 허용" 절).

Failed chrome 의 URL 줄은 `webview.set_url` 의 선택 인자 `label` 을 먼저 보이고, label 이 없으면
탐색 가능한 URL(`http(s)://`·`file://`)만 보인다. raw HTML 을 url 로 싣는 surface(markdown)는 label 로
문서 경로를 보내므로 chrome 에 HTML 원문이 나오지 않는다. 줄은 mono `font_size_caption`(11) ·
`text_disabled` 한 줄이며 넘치면 말줄임한다. chrome 내용은 좌우에 `space-lg`(16) 여백을 둬(시안 타일의 내용 padding) 긴 줄이 패널 가장자리에 닿지 않는다. 판정은 `webview_chrome.rs::chrome_caption` 이며, 주소로 볼지는 로드 분기(주소로 열기 / raw HTML 싣기)와 같은 `webview::is_navigable_url` 이 정한다.

### 디자인 토큰 매핑

페이지 콘텐츠는 해당 페이지의 스타일을 따른다. Tasty가 그리는 로딩·오류·빈 상태에는 공용 Theme 토큰을 사용한다.

### 갤러리 specimen

`crates/tasty-gallery/src/catalog/components/html_chrome.rs` — Layouts › `Content viewers` ›
`HTML (webview) chrome`. boundary / placeholder / loading / error 4 chrome 상태만 전사(콘텐츠는
overlay). 3자 매핑: [design-gallery-mapping.md](../../design/systems/design-gallery-mapping.md#surface-viewers-plugins).

### 시각 소스

콘텐츠는 OS 네이티브 WebView 가 렌더하므로 design-system 토큰이 적용되지 않는다(웹 페이지 자체 스타일). 타일 정렬/경계만 작업영역 레이아웃을 따른다.

### 원격 HTTP(S) native viewer

Linux X11·Windows·macOS 27 이상 client는 지원하는 GUI·headless 원격 서버에 attach하면 HTTP·HTTPS·Reload를 별도로 협상한다. `DISPLAY`가 있는 Wayland 세션의 XWayland도 X11 경로다([백엔드 선택](../../design/systems/linux-display-backend.md)). 격리 GNOME 세션에서 가용 판정이 참인 것까지 확인했고 XWayland의 실제 attach는 미측정이다. Linux는 실제 등록된 Xlib 창과 X11 GDK backend가 준비되어야 하며 환경 변수로 켜지 않는다. Windows는 문서마다 전용 WebView2 context가 인증 proxy로 원격 호스트에 요청한다([Windows HTTP context](../../dev-guide/windows-http-context.md)). macOS는 문서마다 비영속 WebKit 저장소가 같은 인증 proxy로 요청한다. WebKit은 loopback 주소에 직접 연결하므로 원격 `localhost`·`127.x`·`[::1]`은 `*.tasty.localhost` 별칭 주소로 열린다. 주소 표시는 원래 주소를 보이지만 페이지 스크립트의 `location.host`는 별칭이고, 문서 안에서 원격 loopback을 절대 주소로 가리킨 하위 리소스·fetch·WebSocket은 막힌다([macOS HTTP context](../../dev-guide/macos-http-context.md)). native Wayland와 macOS 11–26 client는 이 경로를 알리지 않고 미지원 표시를 유지한다. macOS 최소 버전 11은 바뀌지 않는다. 파일 mirror·구 peer fallback은 별도 기존 경로다.

원격 호스트가 실제 HTTP 요청을 실행한다. POST 같은 요청은 원격 서비스의 상태를 바꿀 수 있다. client의 sandboxScripts·allowRemoteContent 설정, 같은 origin 기본 허용과 각 redirect/cross-origin 검사, 브라우저 CORS·CSP·mixed-content 정책을 유지한다. 수신한 정확한 본문을 검사하고 지문과 문서에 묶인 스크립트 허용은 이미 받은 응답으로 다시 표시하여 POST를 재실행하지 않는다. 명시적 Reload는 같은 native view에서 원래 method/upload와 cookie로 새 요청을 보낸다. Windows·macOS는 이전 endpoint가 닫힌 뒤 새 문서의 endpoint가 같은 proxy 주소를 이어받는다. Reload 영수증이 없는 문서(첫 로드 실패, Reload 미협상)는 새 view에서 entry 주소를 GET으로 다시 열며 이전 cookie를 옮기지 않는다.

각 응답의 encoded 100000바이트와 문서 encoded 누적 2000000바이트 한도에서 기존 허용/거절·계속/중지 UI를 사용한다. Reload는 수신 동의를 초기화하며 본문 지문이 바뀌면 스크립트 동의도 초기화한다. 연결 종료 뒤 받은 본문은 JS를 끄고 남길 수 있으나 재연결과 source 교체에는 cookie나 스크립트 동의를 옮기지 않는다. HTTP 요청의 client 직접 접속·global TLS trust·미지원 backend fallback은 없다. Windows·macOS는 문서마다 WebRTC 생성자를 지우는 완화를 적용하지만 차단을 보장하지 않으므로, 스크립트를 허용한 문서의 WebRTC는 client에서 직접 STUN UDP를 보낼 수 있다([webview 원격 HTTP 직접 경로](../../design/systems/webview.md#원격-http-직접-경로)). native owner와 전송 수명은 [HTTP bridge](../../dev-guide/html-http-bridge.md#linux-app-session-and-native-factory)를 따른다.
